//! Profiling suite: input latency, CPU and memory use, daemon start-up (cold and warm) and GUI
//! start-up. Each is an `#[ignore]` test. It prints its figures and writes them as JSON to
//! `target/profile/` (or `$PADWIGHT_PROFILE_DIR`).
//!
//! Latency runs from just before a South press is written to a uhid controller, to the kernel's
//! time stamp on the matching event from the daemon's virtual pad. So it covers the daemon's whole
//! path, but not USB polling or the controller's own scan, which a uhid pad doesn't have.
//!
//! Needs /dev/uhid and /dev/uinput: `scripts/profile.sh` runs it in a container, under Xvfb for
//! the GUI. On the host, grant the account access to /dev/uhid once
//! (`sudo setfacl -m u:$USER:rw /dev/uhid`), then:
//! `cargo test --release --test profile -- --ignored --test-threads=1 --nocapture`.
//!
//! Build in release mode, so the figures are for the optimized daemon. A cold start has no config,
//! so the daemon grabs every gamepad on the machine, real ones too, until it stops. Close games first.

#![expect(clippy::print_stderr, reason = "the figures are the suite's output")]

#[path = "../common/mod.rs"]
mod common;

mod gui;
mod latency;
mod overlay;
mod resources;
mod startup;
mod support;
