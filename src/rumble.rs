//! Forwards force feedback (rumble) a game sends to our virtual pad to the physical controller.
//!
//! Games upload effects to the virtual pad through uinput; we mirror each upload, erase and
//! play/stop onto the physical device. That needs its own handle to the physical device: the
//! input reader thread owns the first one, and the exclusive grab only affects reading.

use std::{
    collections::HashMap,
    io,
    os::fd::AsRawFd,
    path::PathBuf,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use evdev::{
    Device, EventSummary, FFEffect, FFEffectCode, FFEffectData, FFEffectKind, FFEvent, FFReplay,
    FFTrigger, UInputCode, uinput::VirtualDevice,
};

use crate::monitor::log;

const POLL_TIMEOUT_MS: i32 = 200;

/// Mirrors rumble from `pad` onto the device at `physical` until `stop` is set. Effects are
/// erased from the physical device when the thread ends, which also stops any rumble.
pub fn spawn(pad: Arc<Mutex<VirtualDevice>>, physical: PathBuf, stop: Arc<AtomicBool>) {
    thread::spawn(move || {
        if let Err(e) = run(&pad, &physical, &stop) {
            log!("rumble forwarding for {} stopped: {e}", physical.display());
        }
    });
}

/// Whether the device can rumble at all.
pub fn supports_rumble(dev: &Device) -> bool {
    dev.supported_ff().is_some_and(|ff| ff.contains(FFEffectCode::FF_RUMBLE))
}

/// Plays a short test pattern on the controller at `path` in the background: a strong-motor
/// pulse, then a weak-motor pulse, so both motors can be felt.
pub fn test(path: PathBuf) {
    thread::spawn(move || {
        if let Err(e) = play_test_pattern(&path) {
            log!("rumble test on {} failed: {e}", path.display());
        }
    });
}

const PULSE: Duration = Duration::from_millis(300);
const PAUSE: Duration = Duration::from_millis(150);

fn rumble(strong_magnitude: u16, weak_magnitude: u16) -> FFEffectData {
    FFEffectData {
        direction: 0,
        trigger: FFTrigger::default(),
        replay: FFReplay { length: PULSE.as_millis() as u16, delay: 0 },
        kind: FFEffectKind::Rumble { strong_magnitude, weak_magnitude },
    }
}

fn play_test_pattern(path: &PathBuf) -> io::Result<()> {
    let mut dev = Device::open(path)?;
    let mut effect = dev.upload_ff_effect(rumble(0xc000, 0))?;
    effect.play(1)?;
    thread::sleep(PULSE + PAUSE);
    effect.update(rumble(0, 0xffff))?;
    effect.play(1)?;
    thread::sleep(PULSE);
    // Dropping the effect erases it, which also stops it.
    Ok(())
}

fn lock(pad: &Mutex<VirtualDevice>) -> MutexGuard<'_, VirtualDevice> {
    pad.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn run(pad: &Mutex<VirtualDevice>, physical: &PathBuf, stop: &AtomicBool) -> io::Result<()> {
    let mut phys = Device::open(physical)?;
    let supports_gain = phys.supported_ff().is_some_and(|ff| ff.contains(FFEffectCode::FF_GAIN));
    // Virtual effect id -> the matching effect uploaded to the physical device.
    let mut effects: HashMap<i16, FFEffect> = HashMap::new();
    let mut pfd = libc::pollfd { fd: lock(pad).as_raw_fd(), events: libc::POLLIN, revents: 0 };

    while !stop.load(Ordering::Relaxed) {
        // SAFETY: pfd points to a single valid pollfd for the duration of the call.
        let n = unsafe { libc::poll(&mut pfd, 1, POLL_TIMEOUT_MS) };
        if n < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        if n == 0 {
            continue;
        }
        // Readable, so this won't block. Only FF/uinput requests arrive on this fd.
        let events: Vec<_> = lock(pad).fetch_events()?.collect();
        for ev in events {
            match ev.destructure() {
                EventSummary::UInput(ev, UInputCode::UI_FF_UPLOAD, _) => {
                    // The game's EVIOCSFF blocks until `upload` is dropped, which ends the request.
                    let mut upload = lock(pad).process_ff_upload(ev)?;
                    let id = upload.effect_id();
                    let data = upload.effect();
                    let result = match effects.get_mut(&id) {
                        Some(effect) => effect.update(data),
                        None => phys.upload_ff_effect(data).map(|effect| {
                            effects.insert(id, effect);
                        }),
                    };
                    if let Err(e) = result {
                        log!("rumble: physical pad rejected an effect: {e}");
                        upload.set_retval(-e.raw_os_error().unwrap_or(libc::EIO));
                    }
                }
                EventSummary::UInput(ev, UInputCode::UI_FF_ERASE, _) => {
                    let erase = lock(pad).process_ff_erase(ev)?;
                    // Dropping the FFEffect erases it from the physical device.
                    effects.remove(&(erase.effect_id() as i16));
                }
                EventSummary::ForceFeedback(_, FFEffectCode::FF_GAIN, value) => {
                    if supports_gain {
                        phys.send_events(&[*FFEvent::new(FFEffectCode::FF_GAIN, value)])?;
                    }
                }
                // For play events the code is the effect id and the value a repeat count.
                EventSummary::ForceFeedback(_, code, value) => {
                    if let Some(effect) = effects.get_mut(&(code.0 as i16)) {
                        if value > 0 { effect.play(value)? } else { effect.stop()? }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use evdev::{AttributeSet, FFEffectData, FFEffectKind, FFReplay, FFTrigger};

    use super::*;
    use crate::output::{FfCaps, VirtualPad};

    /// The test pattern must upload a rumble and play it twice (strong, then weak).
    /// Needs /dev/uinput access: `cargo test -- --ignored rumble`.
    #[test]
    #[ignore]
    fn test_pattern_plays_both_motors() {
        let mut effects = AttributeSet::<FFEffectCode>::new();
        effects.insert(FFEffectCode::FF_RUMBLE);
        let pad = VirtualPad::new(Some(FfCaps { effects: &effects, max_effects: 4 })).unwrap();
        let shared = pad.shared();
        let path = lock(&shared).enumerate_dev_nodes_blocking().unwrap().flatten().next().unwrap();
        thread::sleep(Duration::from_millis(500));
        assert!(supports_rumble(&Device::open(&path).unwrap()));
        test(path);

        let mut seen = Vec::new();
        while !seen.last().is_some_and(|s| s == "erase") {
            let events: Vec<_> = lock(&shared).fetch_events().unwrap().collect();
            for ev in events {
                match ev.destructure() {
                    EventSummary::UInput(ev, UInputCode::UI_FF_UPLOAD, _) => {
                        let up = lock(&shared).process_ff_upload(ev).unwrap();
                        if let FFEffectKind::Rumble { strong_magnitude, weak_magnitude } = up.effect().kind {
                            seen.push(format!("rumble {strong_magnitude:#x}/{weak_magnitude:#x}"));
                        }
                    }
                    EventSummary::UInput(ev, UInputCode::UI_FF_ERASE, _) => {
                        lock(&shared).process_ff_erase(ev).unwrap();
                        seen.push("erase".into());
                    }
                    EventSummary::ForceFeedback(_, code, 1) if code != FFEffectCode::FF_GAIN => seen.push("play".into()),
                    _ => {}
                }
            }
        }
        assert_eq!(seen, ["rumble 0xc000/0x0", "play", "rumble 0x0/0xffff", "play", "erase"]);
    }

    /// End-to-end through the kernel: a "game" rumbles our virtual pad and the request must
    /// arrive at a stand-in physical controller (another virtual pad we read requests from).
    /// Needs /dev/uinput access, so it is opt-in: `cargo test -- --ignored rumble`.
    #[test]
    #[ignore]
    fn rumble_reaches_physical_device() {
        let mut effects = AttributeSet::<FFEffectCode>::new();
        effects.insert(FFEffectCode::FF_RUMBLE);
        let make = || VirtualPad::new(Some(FfCaps { effects: &effects, max_effects: 4 })).unwrap();
        let node = |pad: &VirtualPad| {
            let shared = pad.shared();
            let mut dev = lock(&shared);
            dev.enumerate_dev_nodes_blocking().unwrap().flatten().next().unwrap()
        };

        let physical = make();
        let physical_path = node(&physical);
        let pad = make();
        let pad_path = node(&pad);
        // Give udev a moment to grant the seat user access to the new nodes.
        thread::sleep(Duration::from_millis(500));
        let stop = Arc::new(AtomicBool::new(false));
        spawn(pad.shared(), physical_path, stop.clone());

        // Answer the stand-in physical controller's requests and record what it sees. Unanswered
        // uinput FF requests block their caller for 30 s, so it serves until the erase arrives.
        let physical_dev = physical.shared();
        let physical_thread = thread::spawn(move || {
            let mut seen = Vec::new();
            while !seen.last().is_some_and(|s| s == "erase") {
                let events: Vec<_> = lock(&physical_dev).fetch_events().unwrap().collect();
                for ev in events {
                    match ev.destructure() {
                        EventSummary::UInput(ev, UInputCode::UI_FF_UPLOAD, _) => {
                            let up = lock(&physical_dev).process_ff_upload(ev).unwrap();
                            seen.push(format!("upload {:?}", up.effect().kind));
                        }
                        EventSummary::UInput(ev, UInputCode::UI_FF_ERASE, _) => {
                            lock(&physical_dev).process_ff_erase(ev).unwrap();
                            seen.push("erase".into());
                        }
                        EventSummary::ForceFeedback(_, code, value) if code != FFEffectCode::FF_GAIN => {
                            seen.push(format!("play {value}"));
                        }
                        _ => {}
                    }
                }
            }
            seen
        });

        thread::sleep(Duration::from_millis(200));
        let mut game = Device::open(&pad_path).unwrap();
        let mut effect = game
            .upload_ff_effect(FFEffectData {
                direction: 0,
                trigger: FFTrigger::default(),
                replay: FFReplay { length: 200, delay: 0 },
                kind: FFEffectKind::Rumble { strong_magnitude: 0xc000, weak_magnitude: 0x4000 },
            })
            .unwrap();
        effect.play(1).unwrap();
        // The game freeing its effect must free the physical one too.
        drop(effect);

        let seen = physical_thread.join().unwrap();
        stop.store(true, Ordering::Relaxed);
        assert!(seen[0].starts_with("upload Rumble"), "{seen:?}");
        assert_eq!(seen[1], "play 1", "{seen:?}");
        // Erasing also makes the kernel stop the effect (play 0) on each side first.
        assert_eq!(seen.last().map(String::as_str), Some("erase"), "{seen:?}");
        assert!(seen[2..seen.len() - 1].iter().all(|s| s == "play 0"), "{seen:?}");
    }
}
