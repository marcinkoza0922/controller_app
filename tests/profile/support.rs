//! What the profiling suite shares: a uhid controller it can press and flood, timed reads of the
//! daemon's virtual pad, process sampling, and the files the results go to.

use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    os::{
        fd::{AsRawFd, RawFd},
        unix::net::UnixStream,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use evdev::{Device, EventType, InputEvent, KeyCode, SynchronizationCode};
use serde_json::{Value, json};

use crate::common::uhid::{BUTTON_SOUTH, HidPad};

/// Prefix of every virtual device the daemon creates.
const VIRTUAL_PREFIX: &str = "Padwight Virtual";
/// How often a flooding pad sends a report: one per millisecond, about a 1 kHz controller.
const FLOOD_INTERVAL: Duration = Duration::from_millis(1);
/// How often the resource monitor samples a process.
const SAMPLE_EVERY: Duration = Duration::from_millis(100);
/// How long a daemon may take to stop on SIGTERM before it is killed.
const STOP_LIMIT: Duration = Duration::from_secs(10);

/// Wall-clock time in nanoseconds: the clock evdev stamps events with.
pub fn now_ns() -> u64 {
    ns(SystemTime::now())
}

pub fn ns(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64)
}

/// A controller the suite drives: South, and the left stick X. While `flood` is on, `tick` keeps
/// sweeping the stick at [`FLOOD_INTERVAL`], so the daemon gets a steady stream of reports.
pub struct Pad {
    hid: HidPad,
    buttons: u8,
    x: i16,
    pub flood: bool,
    /// Reports sent so far.
    pub sent: u64,
    last: Instant,
}

impl Pad {
    pub fn new(name: &str) -> Self {
        Pad { hid: HidPad::new(name), buttons: 0, x: 0, flood: false, sent: 0, last: Instant::now() }
    }

    pub fn set_south(&mut self, pressed: bool) {
        self.set_button(BUTTON_SOUTH, pressed);
    }

    /// Presses or lets go of the buttons in `mask`.
    pub fn set_button(&mut self, mask: u8, pressed: bool) {
        if pressed {
            self.buttons |= mask;
        } else {
            self.buttons &= !mask;
        }
        self.send();
    }

    /// Moves the left stick's X axis to `x`.
    pub fn set_x(&mut self, x: i16) {
        self.x = x;
        self.send();
    }

    /// Sends the next stick step if flooding and a millisecond has passed. Does nothing otherwise.
    pub fn tick(&mut self) {
        if self.flood && self.last.elapsed() >= FLOOD_INTERVAL {
            self.x = self.x.wrapping_add(997);
            self.send();
        }
    }

    fn send(&mut self) {
        self.last = Instant::now();
        self.sent += 1;
        self.hid.report(self.buttons, self.x, 0);
    }
}

/// The daemon's virtual pad, read as the kernel reports it. `events` counts what came out, and
/// `dropped` counts the kernel's SYN_DROPPED marks, which show the reader fell behind.
pub struct Output {
    dev: Device,
    pub events: u64,
    pub dropped: u64,
}

impl Output {
    /// Opens the newest virtual pad the daemon made. Waits for it to appear.
    pub fn open() -> Self {
        let path = crate::common::wait_for("the daemon's virtual pad", newest_virtual_pad);
        let dev = Device::open(&path).expect("opening the virtual pad");
        dev.set_nonblocking(true).expect("making the virtual pad non-blocking");
        Output { dev, events: 0, dropped: 0 }
    }

    /// Reads events until `want` matches one or `timeout` passes, calling `tick` about every
    /// millisecond meanwhile. Returns the kernel's time stamp of the matching event, in nanoseconds.
    pub fn wait(&mut self, timeout: Duration, mut want: impl FnMut(&InputEvent) -> bool, mut tick: impl FnMut()) -> Option<u64> {
        let deadline = Instant::now() + timeout;
        loop {
            tick();
            for ev in self.read_queued() {
                if want(&ev) {
                    return Some(ns(ev.timestamp()));
                }
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            poll_readable(self.dev.as_raw_fd(), (deadline - now).min(FLOOD_INTERVAL));
        }
    }

    fn read_queued(&mut self) -> Vec<InputEvent> {
        let Ok(events) = self.dev.fetch_events() else { return Vec::new() };
        let events: Vec<InputEvent> = events.collect();
        self.events += events.len() as u64;
        self.dropped += events
            .iter()
            .filter(|ev| ev.event_type() == EventType::SYNCHRONIZATION && ev.code() == SynchronizationCode::SYN_DROPPED.0)
            .count() as u64;
        events
    }
}

/// Waits until `fd` has data or `timeout` passes. Sleeps at least a millisecond.
fn poll_readable(fd: RawFd, timeout: Duration) {
    let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    let millis = timeout.as_millis().max(1) as i32;
    // SAFETY: one valid pollfd, and the descriptor belongs to a device that outlives the call.
    unsafe { libc::poll(&mut pfd, 1, millis) };
}

/// The newest `event` node that is a virtual pad with a South button (not the keyboard or mouse).
fn newest_virtual_pad() -> Option<PathBuf> {
    let number = |path: &Path| path.file_name()?.to_str()?.strip_prefix("event")?.parse::<u32>().ok();
    fs::read_dir("/dev/input")
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| number(path).is_some() && is_virtual_pad(path))
        .max_by_key(|path| number(path))
}

fn is_virtual_pad(path: &Path) -> bool {
    Device::open(path).is_ok_and(|dev| {
        dev.name().is_some_and(|name| name.starts_with(VIRTUAL_PREFIX))
            && dev.supported_keys().is_some_and(|keys| keys.contains(KeyCode::BTN_SOUTH))
    })
}

/// A press (`value` 1) or release (0) of South on an output stream.
pub fn south(ev: &InputEvent, value: i32) -> bool {
    ev.event_type() == EventType::KEY && ev.code() == KeyCode::BTN_SOUTH.0 && ev.value() == value
}

/// Polls `f` until it returns true, and returns how long that took since `start`. None after `limit`.
pub fn elapsed_until(start: Instant, limit: Duration, mut f: impl FnMut() -> bool) -> Option<Duration> {
    loop {
        if f() {
            return Some(start.elapsed());
        }
        if start.elapsed() >= limit {
            return None;
        }
        thread::sleep(Duration::from_millis(1));
    }
}

/// `padwight <args>`, with its config, state, data, runtime and temp files under `root`. Its
/// output goes to `root/<args>.log`. Nothing is installed: the caller decides what config exists.
pub fn padwight(root: &Path, args: &[&str]) -> Command {
    let runtime = root.join("run");
    let tmp = root.join("tmp");
    fs::create_dir_all(&tmp).unwrap();
    fs::create_dir_all(&runtime).unwrap();
    // The daemon wants its runtime directory private to this user.
    fs::set_permissions(&runtime, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    let log = File::create(root.join(format!("{}.log", args.join("-")))).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_padwight"));
    command
        .args(args)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("TMPDIR", &tmp)
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log);
    command
}

/// A child process that is stopped (SIGTERM, then SIGKILL) when dropped, so a failed test leaves
/// no daemon behind.
pub struct Running(pub Child);

impl Running {
    pub fn pid(&self) -> u32 {
        self.0.id()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        // SAFETY: kill only sends a signal, to a child we started and have not waited for.
        unsafe { libc::kill(self.0.id() as libc::pid_t, libc::SIGTERM) };
        let deadline = Instant::now() + STOP_LIMIT;
        while Instant::now() < deadline {
            if matches!(self.0.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The daemon's reply to a status request, if it is answering on the socket in `runtime`.
pub fn status(runtime: &Path) -> Option<Value> {
    let stream = UnixStream::connect(runtime.join("padwight.sock")).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    (&stream).write_all(b"\"Status\"\n").ok()?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).ok()?;
    serde_json::from_str::<Value>(&line).ok()?.get("Status").cloned()
}

/// True when the status lists a device called `name` that the daemon is remapping.
pub fn managed(status: &Value, name: &str) -> bool {
    status["devices"]
        .as_array()
        .is_some_and(|devices| devices.iter().any(|d| d["name"].as_str() == Some(name) && d["managed"].as_bool() == Some(true)))
}

/// Waits for the daemon in `runtime` to grab the controller called `name`. Returns the time from `start`.
pub fn wait_managed(start: Instant, runtime: &Path, name: &str, limit: Duration) -> Option<Duration> {
    elapsed_until(start, limit, || status(runtime).is_some_and(|s| managed(&s, name)))
}

/// Summary of a list of durations in milliseconds: the values, and their median.
pub fn summarize_ms(mut values: Vec<f64>) -> Value {
    values.sort_by(f64::total_cmp);
    let median = values.get(values.len() / 2).copied();
    json!({ "runs": values.len(), "median_ms": median, "min_ms": values.first(), "max_ms": values.last(), "all_ms": values })
}

/// Latency summary, in microseconds, of nanosecond samples.
pub fn latency_stats(mut samples: Vec<u64>) -> Value {
    if samples.is_empty() {
        return json!({ "count": 0 });
    }
    samples.sort_unstable();
    let at = |p: f64| samples[((samples.len() - 1) as f64 * p).round() as usize] as f64 / 1000.0;
    let mean = samples.iter().sum::<u64>() as f64 / samples.len() as f64 / 1000.0;
    json!({
        "count": samples.len(),
        "min_us": at(0.0),
        "mean_us": mean,
        "p50_us": at(0.50),
        "p90_us": at(0.90),
        "p99_us": at(0.99),
        "max_us": at(1.0),
    })
}

/// Drops the file's pages from the page cache, so the next read comes from disk. Needs no root.
pub fn evict(path: &Path) {
    if let Ok(file) = File::open(path) {
        // SAFETY: posix_fadvise only reads the descriptor, which stays open for the call.
        unsafe { libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
    }
}

/// The binary, and the shared libraries `ldd` says it loads.
pub fn binary_and_libraries(binary: &Path) -> Vec<PathBuf> {
    let mut paths = vec![binary.to_path_buf()];
    if let Ok(out) = Command::new("ldd").arg(binary).output() {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let path = line.split("=> ").nth(1).and_then(|rest| rest.split(' ').next());
            if let Some(path) = path.filter(|p| p.starts_with('/')) {
                paths.push(PathBuf::from(path));
            }
        }
    }
    paths
}

/// CPU and memory use of one process over a window, sampled every [`SAMPLE_EVERY`] by a thread.
pub struct Monitor {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Value>,
}

impl Monitor {
    pub fn start(pid: u32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = thread::spawn(move || {
            let mut usage = Usage::new(pid);
            while !flag.load(Ordering::Relaxed) {
                thread::sleep(SAMPLE_EVERY);
                usage.sample();
            }
            usage.finish()
        });
        Monitor { stop, thread }
    }

    /// Ends the window and returns what the process used in it.
    pub fn finish(self) -> Value {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.join().unwrap_or(Value::Null)
    }
}

/// Process counters at one moment.
#[derive(Clone, Copy, Default)]
struct Snapshot {
    /// User plus system CPU time, in clock ticks.
    ticks: u64,
    rss_kb: u64,
    threads: u64,
    fds: u64,
}

struct Usage {
    pid: u32,
    started: Instant,
    first: Snapshot,
    last: Snapshot,
    peak_rss_kb: u64,
    peak_threads: u64,
    peak_fds: u64,
}

impl Usage {
    fn new(pid: u32) -> Self {
        let first = snapshot(pid).unwrap_or_default();
        Usage { pid, started: Instant::now(), first, last: first, peak_rss_kb: first.rss_kb, peak_threads: first.threads, peak_fds: first.fds }
    }

    fn sample(&mut self) {
        if let Some(now) = snapshot(self.pid) {
            self.last = now;
            self.peak_rss_kb = self.peak_rss_kb.max(now.rss_kb);
            self.peak_threads = self.peak_threads.max(now.threads);
            self.peak_fds = self.peak_fds.max(now.fds);
        }
    }

    fn finish(mut self) -> Value {
        self.sample();
        let seconds = self.started.elapsed().as_secs_f64();
        let ticks_per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
        let cpu = (self.last.ticks.saturating_sub(self.first.ticks)) as f64 / ticks_per_second / seconds * 100.0;
        json!({
            "seconds": seconds,
            "cpu_percent": cpu,
            "rss_start_kb": self.first.rss_kb,
            "rss_end_kb": self.last.rss_kb,
            "rss_peak_kb": self.peak_rss_kb,
            "rss_growth_kb": self.last.rss_kb as i64 - self.first.rss_kb as i64,
            "threads_peak": self.peak_threads,
            "fds_peak": self.peak_fds,
        })
    }
}

/// Reads the counters of `pid` from /proc, or None once it has gone.
fn snapshot(pid: u32) -> Option<Snapshot> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Fields after the parenthesised command name: utime and stime are 14 and 15 of the line.
    let after_name = &stat[stat.rfind(')')? + 2..];
    let fields: Vec<&str> = after_name.split_whitespace().collect();
    let ticks = fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?;
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let fds = fs::read_dir(format!("/proc/{pid}/fd")).ok()?.count() as u64;
    Some(Snapshot {
        ticks,
        rss_kb: status_field(&status, "VmRSS:")?,
        threads: status_field(&status, "Threads:")?,
        fds,
    })
}

/// The number in a `/proc/<pid>/status` line such as `VmRSS:   1234 kB`.
fn status_field(status: &str, key: &str) -> Option<u64> {
    status.lines().find_map(|line| line.strip_prefix(key)).and_then(|rest| rest.split_whitespace().next()?.parse().ok())
}

/// Where results go: `$PADWIGHT_PROFILE_DIR`, or `target/profile` in the checkout.
pub fn output_dir() -> PathBuf {
    std::env::var_os("PADWIGHT_PROFILE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/profile"))
}

/// Writes one result as `<name>.json` in the output directory, with the machine it ran on, and
/// prints it.
pub fn report(name: &str, mut result: Value) {
    result["environment"] = environment();
    let dir = output_dir();
    fs::create_dir_all(&dir).unwrap();
    let text = serde_json::to_string_pretty(&result).unwrap();
    fs::write(dir.join(format!("{name}.json")), &text).unwrap();
    eprintln!("== {name} ({})\n{text}", dir.join(format!("{name}.json")).display());
}

/// What the figures depend on: the commit, kernel, CPU, and whether this is a container or a
/// release build.
fn environment() -> Value {
    let cpu = fs::read_to_string("/proc/cpuinfo").ok().and_then(|text| {
        text.lines().find_map(|line| line.strip_prefix("model name")).and_then(|rest| rest.split_once(':')).map(|(_, name)| name.trim().to_string())
    });
    json!({
        "commit": commit(),
        "kernel": fs::read_to_string("/proc/sys/kernel/osrelease").ok().map(|s| s.trim().to_string()),
        "cpu": cpu,
        "cores": thread::available_parallelism().map_or(0, std::num::NonZero::get),
        "container": Path::new("/.dockerenv").exists(),
        "display": std::env::var("DISPLAY").ok(),
        "wayland": std::env::var_os("WAYLAND_DISPLAY").is_some(),
        "release_build": !cfg!(debug_assertions),
        "daemon_binary_bytes": fs::metadata(env!("CARGO_BIN_EXE_padwight")).map(|m| m.len()).ok(),
    })
}

/// Sends one request line to the daemon and reads its reply, which is ignored.
pub fn send_request(runtime: &Path, request: &str) {
    let mut stream = UnixStream::connect(runtime.join("padwight.sock")).expect("connecting to the daemon");
    stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    stream.write_all(format!("{request}\n").as_bytes()).unwrap();
    let mut reply = String::new();
    let _ = BufReader::new(stream).read_line(&mut reply);
}

/// A subscription to the daemon's overlay frames (`WatchOverlay`). Each frame is stamped with the
/// time it arrived, on the clock evdev uses, so it can be set against when a request was sent.
pub struct Watcher {
    lines: BufReader<UnixStream>,
    /// The keyboard or menu the latest frame shows, or null when none is up.
    pub active: Value,
}

impl Watcher {
    pub fn connect(runtime: &Path) -> Self {
        let mut stream = UnixStream::connect(runtime.join("padwight.sock")).expect("connecting to the daemon");
        stream.write_all(b"\"WatchOverlay\"\n").expect("subscribing to the overlay");
        Watcher { lines: BufReader::new(stream), active: Value::Null }
    }

    /// Reads frames until one satisfies `want`. Returns when that frame arrived, in nanoseconds,
    /// or None after `timeout`. Every frame read updates [`Watcher::active`].
    pub fn wait_for(&mut self, timeout: Duration, want: impl Fn(&Value) -> bool) -> Option<u64> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.checked_duration_since(Instant::now())?;
            self.lines.get_ref().set_read_timeout(Some(remaining)).ok()?;
            let mut line = String::new();
            match self.lines.read_line(&mut line) {
                Ok(n) if n > 0 => {}
                _ => return None,
            }
            let arrived = now_ns();
            let Ok(frame) = serde_json::from_str::<Value>(&line) else { continue };
            self.active = frame["active"].clone();
            if want(&frame) {
                return Some(arrived);
            }
        }
    }
}

/// The checkout's short commit, or None when git can't say (no git, or no repository).
pub fn commit() -> Option<String> {
    Command::new("git")
        .args(["-c", "safe.directory=*", "-C", env!("CARGO_MANIFEST_DIR"), "rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// A small xorshift generator, so the sample gaps are the same on every run.
pub struct Rng(pub u64);

impl Rng {
    /// A value in `lo..hi`.
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        lo + self.0 % (hi - lo)
    }
}
