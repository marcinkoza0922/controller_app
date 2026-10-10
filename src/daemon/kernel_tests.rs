//! Kernel-level check for stuck keys. The engine's output goes through the daemon's own
//! `dispatch` into the real virtual keyboard and mouse, and what the kernel reports on their
//! event nodes must match what was sent. Needs /dev/uinput: `cargo test -- --ignored kernel`.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::ErrorKind,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use evdev::{Device, EventType, KeyCode};

use crate::{
    config::Button,
    engine::release_tests,
    monitor::OutputView,
    output::{OutEvent, VIRTUAL_PREFIX, VirtualKbm, VirtualPad, mouse_code},
    pad_identity::PadIdentity,
};

/// What the kernel should show as pressed. The virtual keyboard counts presses per key, so a
/// key is down while its count is above zero (see `VirtualKbm::key`), and this keeps the same
/// count independently.
#[derive(Default)]
struct Expected(BTreeMap<u16, u32>);

impl Expected {
    fn apply(&mut self, out: &[OutEvent]) {
        for ev in out {
            let (code, down) = match *ev {
                OutEvent::Key(k, down) => (k.0, down),
                OutEvent::MouseButton(b, down) => (mouse_code(b).0, down),
                _ => continue,
            };
            let count = self.0.entry(code).or_insert(0);
            if down {
                *count += 1;
            } else {
                *count = count.saturating_sub(1);
            }
        }
        self.0.retain(|_, count| *count > 0);
    }

    fn pressed(&self) -> BTreeSet<u16> {
        self.0.keys().copied().collect()
    }
}

/// Reads the key and button events the kernel has delivered to our keyboard and mouse nodes,
/// and keeps the set of codes it shows as pressed.
struct Kernel {
    devices: Vec<Device>,
    pressed: BTreeSet<u16>,
}

impl Kernel {
    fn open(nodes: &[PathBuf]) -> Self {
        let devices = nodes
            .iter()
            .map(|path| {
                // The keyboard and mouse nodes are readable only by the `input` group here, so say
                // so rather than failing with a bare permission error.
                let dev = Device::open(path).unwrap_or_else(|e| {
                    panic!("cannot read {}: {e}. Add this user to the `input` group, then rerun", path.display())
                });
                dev.set_nonblocking(true).unwrap();
                dev
            })
            .collect();
        Kernel { devices, pressed: BTreeSet::new() }
    }

    /// Reads everything delivered so far. Events written with `emit` are queued before it
    /// returns, so one read after a dispatch sees all of them.
    fn read(&mut self) {
        for dev in &mut self.devices {
            loop {
                let events: Vec<_> = match dev.fetch_events() {
                    Ok(events) => events.collect(),
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) => panic!("reading the virtual device: {e}"),
                };
                if events.is_empty() {
                    break;
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
}

#[test]
#[ignore]
fn nothing_stays_pressed_in_the_kernel() {
    let mut kbm = VirtualKbm::new().unwrap();
    let mut pad = VirtualPad::new(None).unwrap();
    let (keyboard, mouse) = kbm.event_nodes();
    // Give udev a moment to grant the seat user access to the new nodes.
    thread::sleep(Duration::from_millis(500));
    let mut kernel = Kernel::open(&[keyboard, mouse]);

    // Without this, the reader could see nothing and every check below would pass.
    let mut view = OutputView::default();
    super::dispatch(&mut pad, &mut view, &mut kbm, vec![OutEvent::Key(KeyCode::KEY_A, true)]);
    kernel.read();
    assert!(kernel.pressed.contains(&KeyCode::KEY_A.0), "the kernel reports a press dispatched through the daemon");
    super::dispatch(&mut pad, &mut view, &mut kbm, vec![OutEvent::Key(KeyCode::KEY_A, false)]);
    kernel.read();
    assert!(kernel.pressed.is_empty(), "and its release");

    let profile = release_tests::busy_profile();
    let step = Duration::from_millis(16);
    for seed in 1..=20u64 {
        let mut e = release_tests::engine();
        let mut rng = release_tests::Rng::new(seed);
        let mut view = OutputView::default();
        let mut expected = Expected::default();
        let mut now = Instant::now();
        for i in 0..300 {
            let mut out = Vec::new();
            e.handle(&profile, release_tests::random_event(&mut rng), now, &mut out);
            e.timers(&profile, now, &mut out);
            if e.needs_tick(&profile) {
                e.tick(&profile, step.as_secs_f32(), &mut out);
            }
            // Every 50th step, unplug the controller: the daemon releases what the engine holds.
            let unplug = i % 50 == 49;
            if unplug {
                e.release_all(false, &mut out);
            }
            expected.apply(&out);
            super::dispatch(&mut pad, &mut view, &mut kbm, out);
            kernel.read();
            assert_eq!(kernel.pressed, expected.pressed(), "seed {seed}, step {i}: kernel and daemon disagree");
            if unplug {
                assert!(kernel.pressed.is_empty(), "seed {seed}, step {i}: keys still down after an unplug");
            }
            now += step;
        }

        let mut out = Vec::new();
        e.release_all(false, &mut out);
        expected.apply(&out);
        super::dispatch(&mut pad, &mut view, &mut kbm, out);
        kernel.read();
        assert!(kernel.pressed.is_empty(), "seed {seed}: still pressed at the end: {:?}", kernel.pressed);
    }
}

#[test]
#[ignore]
fn a_pad_presents_the_identity_it_was_given() {
    for identity in [PadIdentity::Xbox360, PadIdentity::DualShock4, PadIdentity::DualSense, PadIdentity::SwitchPro] {
        let mut pad = VirtualPad::with_identity(None, identity).unwrap();
        let node = {
            let shared = pad.shared();
            let mut vdev = shared.lock().unwrap();
            vdev.enumerate_dev_nodes_blocking().unwrap().flatten().next().unwrap()
        };
        // Give udev a moment to grant the seat user access to the new node.
        let mut dev = open_when_granted(&node);
        let (vendor, product, _) = identity.usb_id();
        assert_eq!((dev.input_id().vendor(), dev.input_id().product()), (vendor, product), "{identity:?}");
        assert_eq!(dev.name(), Some(format!("{VIRTUAL_PREFIX} {}", identity.name()).as_str()), "{identity:?}");
        // Button::North is the top button. The xpad driver's labels put the top button at BTN_WEST.
        let want = if identity.xpad_labels() { KeyCode::BTN_WEST } else { KeyCode::BTN_NORTH };
        pad.button(Button::North, true).unwrap();
        let pressed: Vec<u16> = dev.fetch_events().unwrap().filter(|e| e.event_type() == EventType::KEY && e.value() == 1).map(|e| e.code()).collect();
        assert_eq!(pressed, vec![want.0], "{identity:?}");
    }
}

/// Opens an event node once udev has granted access to it, which takes a moment.
fn open_when_granted(node: &std::path::Path) -> Device {
    let start = Instant::now();
    loop {
        match Device::open(node) {
            Ok(dev) => return dev,
            Err(e) if e.kind() == ErrorKind::PermissionDenied && start.elapsed() < Duration::from_secs(5) => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => panic!("cannot open {}: {e}", node.display()),
        }
    }
}
