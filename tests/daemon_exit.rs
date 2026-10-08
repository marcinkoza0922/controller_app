//! Stops the real `padwight daemon` while it holds a key and a mouse button, against a fake
//! controller made with uhid, and checks what it leaves behind: the controller's grab is released
//! and the daemon's virtual devices are gone. For SIGTERM, which a daemon can handle, the keys must
//! also be released before it exits; SIGKILL can't be handled, so only the cleanup is checked.
//!
//! Needs /dev/uinput and read access to /dev/input, so it is opt-in:
//! `cargo test --test daemon_exit -- --ignored`. The daemon grabs any gamepad it finds, so run it
//! with no real controller attached.

use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use evdev::{Device, EventType, InputEvent, KeyCode};

/// The daemon's virtual devices have fixed names, so scenarios can't overlap.
static SERIAL: Mutex<()> = Mutex::new(());

/// The daemon's default config, as it writes it on first start. Its Gamepad profile (the active one)
/// passes every button through; the test changes South and East, to hold a key and a mouse button.
/// Regenerate it if the config format changes: start the daemon with an empty XDG_CONFIG_HOME.
const DEFAULT_CONFIG: &str = include_str!("fixtures/default-config.toml");

fn hold_config() -> String {
    let text = DEFAULT_CONFIG.replace("[general.profiles.buttons.South]\ngamepad = \"South\"", "[general.profiles.buttons.South]\nkeys = [\"KEY_A\"]");
    let text = text.replace("[general.profiles.buttons.East]\ngamepad = \"East\"", "[general.profiles.buttons.East]\nmouse = \"Left\"");
    assert!(text.contains("keys = [\"KEY_A\"]") && text.contains("mouse = \"Left\""), "the default config has South and East as passthrough");
    text
}

const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug)]
enum Signal {
    Kill,
    Term,
}

/// The daemon, killed if the test fails before it has been stopped.
struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn temp_root() -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("padwight-exit-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Polls `f` until it returns a value, or panics after `TIMEOUT`.
fn wait_for<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(value) = f() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(20));
    }
}

fn start_daemon(root: &Path) -> Daemon {
    let config = root.join("config/padwight");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("config.toml"), hold_config()).unwrap();
    let run = root.join("run");
    fs::create_dir_all(&run).unwrap();
    let log = fs::File::create(root.join("daemon.log")).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_padwight"))
        .arg("daemon")
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_RUNTIME_DIR", &run)
        .env("XDG_STATE_HOME", root.join("state"))
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .expect("starting the daemon");
    Daemon(child)
}

/// Report descriptor of a plain gamepad: four buttons (the first four are South, East, North and
/// West once the kernel maps them) and an X/Y stick, with one button byte then two 16-bit axes.
const DESCRIPTOR: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x05, // Usage (Gamepad)
    0xa1, 0x01, // Collection (Application)
    0x05, 0x09, // Usage Page (Button)
    0x19, 0x01, // Usage Minimum (1)
    0x29, 0x04, // Usage Maximum (4)
    0x15, 0x00, // Logical Minimum (0)
    0x25, 0x01, // Logical Maximum (1)
    0x75, 0x01, // Report Size (1)
    0x95, 0x04, // Report Count (4)
    0x81, 0x02, // Input (Data, Variable, Absolute)
    0x75, 0x04, // Report Size (4)
    0x95, 0x01, // Report Count (1)
    0x81, 0x03, // Input (Constant, Variable, Absolute): padding
    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x30, // Usage (X)
    0x09, 0x31, // Usage (Y)
    0x16, 0x00, 0x80, // Logical Minimum (-32768)
    0x26, 0xff, 0x7f, // Logical Maximum (32767)
    0x75, 0x10, // Report Size (16)
    0x95, 0x02, // Report Count (2)
    0x81, 0x02, // Input (Data, Variable, Absolute)
    0xc0, // End Collection
];

const BUTTON_SOUTH: u8 = 0b0001;
const BUTTON_EAST: u8 = 0b0010;

/// Size of the kernel's `struct uhid_event` (packed): a type, then the largest payload.
const UHID_EVENT_SIZE: usize = 4376;
const UHID_CREATE2: u32 = 11;
const UHID_INPUT2: u32 = 12;

/// A gamepad made through the kernel's uhid, so it is a HID device like a real controller. The
/// daemon ignores uinput devices (they count as its own), so a uinput stand-in won't be grabbed.
struct HidPad {
    file: File,
}

impl HidPad {
    fn new() -> Self {
        let file = OpenOptions::new().read(true).write(true).open("/dev/uhid").expect("opening /dev/uhid (needs root or the uhid device)");
        let mut pad = HidPad { file };
        let mut event = vec![0u8; UHID_EVENT_SIZE];
        event[0..4].copy_from_slice(&UHID_CREATE2.to_le_bytes());
        // Packed `uhid_create2_req`, starting after the type: name[128], phys[64], uniq[64], then:
        let name = b"Test Controller";
        event[4..4 + name.len()].copy_from_slice(name);
        event[260..262].copy_from_slice(&(DESCRIPTOR.len() as u16).to_le_bytes()); // rd_size
        event[262..264].copy_from_slice(&0x0003u16.to_le_bytes()); // bus: USB
        event[264..268].copy_from_slice(&0x045eu32.to_le_bytes()); // vendor
        event[268..272].copy_from_slice(&0x028eu32.to_le_bytes()); // product
        event[272..276].copy_from_slice(&1u32.to_le_bytes()); // version
        event[280..280 + DESCRIPTOR.len()].copy_from_slice(DESCRIPTOR); // rd_data
        pad.file.write_all(&event).unwrap();
        pad
    }

    /// Sends the controller's state. The kernel drops reports until the daemon has opened the
    /// device, so callers send the state again until it shows up.
    fn report(&mut self, buttons: u8, x: i16, y: i16) {
        let mut event = vec![0u8; UHID_EVENT_SIZE];
        event[0..4].copy_from_slice(&UHID_INPUT2.to_le_bytes());
        event[4..6].copy_from_slice(&5u16.to_le_bytes()); // size: buttons, then X and Y
        event[6] = buttons;
        event[7..9].copy_from_slice(&x.to_le_bytes());
        event[9..11].copy_from_slice(&y.to_le_bytes());
        self.file.write_all(&event).unwrap();
    }
}

fn find_node(name: &str) -> Option<PathBuf> {
    for entry in fs::read_dir("/dev/input").ok()?.flatten() {
        let path = entry.path();
        if let Ok(dev) = Device::open(&path)
            && dev.name() == Some(name)
        {
            return Some(path);
        }
    }
    None
}

/// True when another process holds the controller's grab.
fn grabbed(path: &Path) -> bool {
    let mut dev = Device::open(path).unwrap();
    match dev.grab() {
        Ok(()) => {
            let _ = dev.ungrab();
            false
        }
        Err(e) => e.raw_os_error() == Some(libc::EBUSY),
    }
}

/// Reads the key and button events the kernel reports on one of the daemon's virtual devices, and
/// keeps the set it shows as pressed. The device is kept open, so its release events are seen
/// even after the daemon has gone.
struct Reader {
    dev: Device,
    pressed: BTreeSet<u16>,
}

impl Reader {
    fn open(name: &str) -> Self {
        let path = wait_for(&format!("the virtual device {name:?}"), || find_node(name));
        let dev = Device::open(&path).unwrap();
        dev.set_nonblocking(true).unwrap();
        Reader { dev, pressed: BTreeSet::new() }
    }

    /// Reads what is queued. Returns true once the device has gone (its read fails with ENODEV).
    fn drain(&mut self) -> bool {
        loop {
            let events: Vec<InputEvent> = match self.dev.fetch_events() {
                Ok(events) => events.collect(),
                Err(e) if e.kind() == ErrorKind::WouldBlock => return false,
                Err(e) if e.raw_os_error() == Some(libc::ENODEV) => return true,
                Err(e) => panic!("reading a virtual device: {e}"),
            };
            if events.is_empty() {
                return false;
            }
            for ev in events.iter().filter(|ev| ev.event_type() == EventType::KEY) {
                match ev.value() {
                    1 => {
                        self.pressed.insert(ev.code());
                    }
                    0 => {
                        self.pressed.remove(&ev.code());
                    }
                    _ => {}
                }
            }
        }
    }
}

fn send(daemon: &mut Daemon, signal: Signal) {
    match signal {
        Signal::Kill => daemon.0.kill().unwrap(),
        Signal::Term => {
            // SAFETY: `kill` only sends a signal, to a child we started and have not yet waited for.
            let rc = unsafe { libc::kill(daemon.0.id() as libc::pid_t, libc::SIGTERM) };
            assert_eq!(rc, 0, "sending SIGTERM");
        }
    }
}

fn wait_exit(daemon: &mut Daemon) {
    wait_for("the daemon to exit", || daemon.0.try_wait().unwrap());
}

fn scenario(signal: Signal) {
    let _serial = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let root = temp_root();

    let mut pad = HidPad::new();
    let pad_node = wait_for("the controller's event node", || find_node("Test Controller"));

    let mut daemon = start_daemon(&root);
    let run = root.join("run");
    let socket = run.join("padwight.sock");
    let deadline = Instant::now() + TIMEOUT;
    while UnixStream::connect(&socket).is_err() {
        if let Ok(Some(status)) = daemon.0.try_wait() {
            panic!("the daemon exited ({status}): {}", fs::read_to_string(root.join("daemon.log")).unwrap_or_default());
        }
        assert!(Instant::now() < deadline, "timed out waiting for the daemon's socket: {}", fs::read_to_string(root.join("daemon.log")).unwrap_or_default());
        thread::sleep(Duration::from_millis(20));
    }
    wait_for("the daemon to grab the fake controller", || grabbed(&pad_node).then_some(()));

    // Hold South (a key) and East (a mouse button). Resent until the daemon has opened the pad.
    let mut keys = Reader::open("Padwight Virtual Keyboard");
    let mut mouse = Reader::open("Padwight Virtual Mouse");
    let held = BUTTON_SOUTH | BUTTON_EAST;
    wait_for("KEY_A and the mouse's left button to be pressed", || {
        pad.report(held, 0, 0);
        thread::sleep(Duration::from_millis(20));
        keys.drain();
        mouse.drain();
        (keys.pressed.contains(&KeyCode::KEY_A.0) && mouse.pressed.contains(&KeyCode::BTN_LEFT.0)).then_some(())
    });

    // Stop the daemon. While it is still running it can release what it holds, so watch the keys
    // until they are up or it exits. Once it has exited its virtual devices are gone, and a reader
    // gets no release events from them, so anything still held at that point counts as stuck.
    send(&mut daemon, signal);
    let deadline = Instant::now() + TIMEOUT;
    let mut exited = false;
    while !exited && !(keys.pressed.is_empty() && mouse.pressed.is_empty()) {
        assert!(Instant::now() < deadline, "{signal:?}: the daemon neither exited nor released its keys");
        keys.drain();
        mouse.drain();
        exited = matches!(daemon.0.try_wait(), Ok(Some(_)));
        thread::sleep(Duration::from_millis(5));
    }
    let released_before_exit = keys.pressed.is_empty() && mouse.pressed.is_empty();
    wait_exit(&mut daemon);

    // SIGTERM is the one a daemon can handle (systemctl stop sends it), so it must release first.
    // SIGKILL can't be handled: nothing can be asserted about the keys then, only the cleanup below.
    if let Signal::Term = signal {
        assert!(
            released_before_exit,
            "SIGTERM: the daemon did not release its keys before exiting (KEY_A released: {}, left button released: {}). \
             It has no SIGTERM handler, and the tray's Quit exits without releasing anything either",
            !keys.pressed.contains(&KeyCode::KEY_A.0),
            !mouse.pressed.contains(&KeyCode::BTN_LEFT.0)
        );
    }
    wait_for("the virtual keyboard and mouse to go away", || (keys.drain() && mouse.drain()).then_some(()));

    // The daemon's grab on the controller went with it.
    assert!(!grabbed(&pad_node), "{signal:?}: the controller is still grabbed");

    // No virtual device is left: a leaked file descriptor would keep one registered.
    wait_for("the daemon's virtual devices to disappear", || {
        (find_node("Padwight Virtual Keyboard").is_none() && find_node("Padwight Virtual Mouse").is_none()).then_some(())
    });

    let _ = fs::remove_dir_all(&root);
}

#[test]
#[ignore]
fn daemon_killed_with_sigkill_releases_everything() {
    scenario(Signal::Kill);
}

#[test]
#[ignore]
fn daemon_killed_with_sigterm_releases_everything() {
    scenario(Signal::Term);
}
