//! Virtual uinput devices the daemon writes to.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use anyhow::Result;
use evdev::{
    AbsInfo, AbsoluteAxisCode as Abs, AttributeSet, AttributeSetRef, BusType, EventType,
    FFEffectCode, InputEvent, InputId, KeyCode, RelativeAxisCode as Rel, UinputAbsSetup,
    uinput::VirtualDevice,
};

use crate::{
    config::{Button, MouseButton},
    input::Axis,
};

/// Prefix of every device we create, so the daemon never grabs its own output.
pub const VIRTUAL_PREFIX: &str = "Padwight Virtual";

/// Abstract output produced by the mapping engine.
#[derive(Debug, Clone, PartialEq)]
pub enum OutEvent {
    PadButton(Button, bool),
    PadAxis(Axis, f32),
    Key(KeyCode, bool),
    MouseButton(MouseButton, bool),
    MouseMove(i32, i32),
    /// High-resolution wheel units (120 = one notch).
    Wheel { vertical: i32, horizontal: i32 },
}

pub struct VirtualPad {
    /// Shared with the rumble thread, which reads force-feedback requests from it.
    dev: Arc<Mutex<VirtualDevice>>,
    dpad: HashSet<Button>,
}

/// Force-feedback support to advertise, copied from the physical controller.
pub struct FfCaps<'a> {
    pub effects: &'a AttributeSetRef<FFEffectCode>,
    pub max_effects: u32,
}

impl VirtualPad {
    pub fn new(ff: Option<FfCaps>) -> Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for k in [
            KeyCode::BTN_SOUTH,
            KeyCode::BTN_EAST,
            KeyCode::BTN_NORTH,
            KeyCode::BTN_WEST,
            KeyCode::BTN_TL,
            KeyCode::BTN_TR,
            KeyCode::BTN_SELECT,
            KeyCode::BTN_START,
            KeyCode::BTN_MODE,
            KeyCode::BTN_THUMBL,
            KeyCode::BTN_THUMBR,
        ] {
            keys.insert(k);
        }
        let stick = AbsInfo::new(0, -32768, 32767, 16, 128, 0);
        let trigger = AbsInfo::new(0, 0, 255, 0, 0, 0);
        let hat = AbsInfo::new(0, -1, 1, 0, 0, 0);

        // Present as an Xbox 360 pad so SDL/Steam/games pick the right mapping out of the box.
        let name = format!("{VIRTUAL_PREFIX} Pad");
        let mut builder = VirtualDevice::builder()?
            .name(&name)
            .input_id(InputId::new(BusType::BUS_USB, 0x045e, 0x028e, 0x0110))
            .with_keys(&keys)?;
        for (code, info) in [
            (Abs::ABS_X, stick),
            (Abs::ABS_Y, stick),
            (Abs::ABS_RX, stick),
            (Abs::ABS_RY, stick),
            (Abs::ABS_Z, trigger),
            (Abs::ABS_RZ, trigger),
            (Abs::ABS_HAT0X, hat),
            (Abs::ABS_HAT0Y, hat),
        ] {
            builder = builder.with_absolute_axis(&UinputAbsSetup::new(code, info))?;
        }
        if let Some(ff) = ff {
            builder = builder.with_ff(ff.effects)?.with_ff_effects_max(ff.max_effects);
        }
        Ok(VirtualPad { dev: Arc::new(Mutex::new(builder.build()?)), dpad: HashSet::new() })
    }

    pub fn shared(&self) -> Arc<Mutex<VirtualDevice>> {
        self.dev.clone()
    }

    fn emit(&self, events: &[InputEvent]) -> Result<()> {
        let mut dev = self.dev.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        dev.emit(events)?;
        Ok(())
    }

    pub fn button(&mut self, b: Button, pressed: bool) -> Result<()> {
        let code = match b {
            Button::South => KeyCode::BTN_SOUTH,
            Button::East => KeyCode::BTN_EAST,
            // We present as an xpad device, so use its label convention: BTN_X (== BTN_NORTH)
            // is the west button and BTN_Y (== BTN_WEST) the north one. SDL and games expect this.
            Button::North => KeyCode::BTN_WEST,
            Button::West => KeyCode::BTN_NORTH,
            Button::LeftBumper => KeyCode::BTN_TL,
            Button::RightBumper => KeyCode::BTN_TR,
            Button::Select => KeyCode::BTN_SELECT,
            Button::Start => KeyCode::BTN_START,
            Button::Guide => KeyCode::BTN_MODE,
            Button::LeftStick => KeyCode::BTN_THUMBL,
            Button::RightStick => KeyCode::BTN_THUMBR,
            Button::DpadUp | Button::DpadDown | Button::DpadLeft | Button::DpadRight => {
                return self.dpad(b, pressed);
            }
            // The engine turns stick directions into stick deflection; nothing to press.
            Button::LeftStickUp
            | Button::LeftStickDown
            | Button::LeftStickLeft
            | Button::LeftStickRight
            | Button::RightStickUp
            | Button::RightStickDown
            | Button::RightStickLeft
            | Button::RightStickRight => return Ok(()),
        };
        self.emit(&[key_event(code, pressed)])
    }

    fn dpad(&mut self, b: Button, pressed: bool) -> Result<()> {
        if pressed {
            self.dpad.insert(b);
        } else {
            self.dpad.remove(&b);
        }
        let axis = |neg, pos| self.dpad.contains(&pos) as i32 - self.dpad.contains(&neg) as i32;
        let x = axis(Button::DpadLeft, Button::DpadRight);
        let y = axis(Button::DpadUp, Button::DpadDown);
        self.emit(&[abs_event(Abs::ABS_HAT0X, x), abs_event(Abs::ABS_HAT0Y, y)])
    }

    pub fn axis(&mut self, axis: Axis, value: f32) -> Result<()> {
        let (code, raw) = match axis {
            Axis::LeftX => (Abs::ABS_X, stick_raw(value)),
            Axis::LeftY => (Abs::ABS_Y, stick_raw(value)),
            Axis::RightX => (Abs::ABS_RX, stick_raw(value)),
            Axis::RightY => (Abs::ABS_RY, stick_raw(value)),
            Axis::LeftTrigger => (Abs::ABS_Z, (value.clamp(0.0, 1.0) * 255.0).round() as i32),
            Axis::RightTrigger => (Abs::ABS_RZ, (value.clamp(0.0, 1.0) * 255.0).round() as i32),
        };
        self.emit(&[abs_event(code, raw)])
    }
}

fn stick_raw(v: f32) -> i32 {
    (v.clamp(-1.0, 1.0) * 32767.0).round() as i32
}

/// Shared virtual keyboard + mouse. Presses are reference counted so two inputs mapped to
/// the same key don't release it early.
pub struct VirtualKbm {
    keyboard: VirtualDevice,
    mouse: VirtualDevice,
    held: HashMap<KeyCode, u32>,
    /// Hi-res wheel units not yet emitted as whole REL_WHEEL notches.
    wheel_acc: (i32, i32),
}

impl VirtualKbm {
    pub fn new() -> Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        // Every regular keyboard key (KEY_ESC..KEY_MICMUTE).
        for code in 1..=248u16 {
            keys.insert(KeyCode::new(code));
        }
        let keyboard = VirtualDevice::builder()?
            .name(&format!("{VIRTUAL_PREFIX} Keyboard"))
            .input_id(InputId::new(BusType::BUS_VIRTUAL, 0x1209, 0x0001, 1))
            .with_keys(&keys)?
            .build()?;

        let mut buttons = AttributeSet::<KeyCode>::new();
        for b in MouseButton::ALL {
            buttons.insert(mouse_code(b));
        }
        let mut rel = AttributeSet::<Rel>::new();
        for r in [Rel::REL_X, Rel::REL_Y, Rel::REL_WHEEL, Rel::REL_HWHEEL, Rel::REL_WHEEL_HI_RES, Rel::REL_HWHEEL_HI_RES] {
            rel.insert(r);
        }
        let mouse = VirtualDevice::builder()?
            .name(&format!("{VIRTUAL_PREFIX} Mouse"))
            .input_id(InputId::new(BusType::BUS_VIRTUAL, 0x1209, 0x0002, 1))
            .with_keys(&buttons)?
            .with_relative_axes(&rel)?
            .build()?;

        Ok(VirtualKbm { keyboard, mouse, held: HashMap::new(), wheel_acc: (0, 0) })
    }

    pub fn key(&mut self, code: KeyCode, pressed: bool) -> Result<()> {
        if self.update_refcount(code, pressed) {
            self.keyboard.emit(&[key_event(code, pressed)])?;
        }
        Ok(())
    }

    pub fn mouse_button(&mut self, b: MouseButton, pressed: bool) -> Result<()> {
        let code = mouse_code(b);
        if self.update_refcount(code, pressed) {
            self.mouse.emit(&[key_event(code, pressed)])?;
        }
        Ok(())
    }

    /// Returns true when the press state actually changes.
    fn update_refcount(&mut self, code: KeyCode, pressed: bool) -> bool {
        let count = self.held.entry(code).or_insert(0);
        if pressed {
            *count += 1;
            *count == 1
        } else if *count > 0 {
            *count -= 1;
            *count == 0
        } else {
            false
        }
    }

    pub fn mouse_move(&mut self, dx: i32, dy: i32) -> Result<()> {
        let mut events = Vec::with_capacity(2);
        if dx != 0 {
            events.push(rel_event(Rel::REL_X, dx));
        }
        if dy != 0 {
            events.push(rel_event(Rel::REL_Y, dy));
        }
        if !events.is_empty() {
            self.mouse.emit(&events)?;
        }
        Ok(())
    }

    /// Takes hi-res units (120 per notch) and also emits legacy notches for older clients.
    pub fn wheel(&mut self, vertical: i32, horizontal: i32) -> Result<()> {
        self.wheel_acc.0 += vertical;
        self.wheel_acc.1 += horizontal;
        let notches = (self.wheel_acc.0 / 120, self.wheel_acc.1 / 120);
        self.wheel_acc.0 -= notches.0 * 120;
        self.wheel_acc.1 -= notches.1 * 120;

        let mut events = Vec::with_capacity(4);
        if vertical != 0 {
            events.push(rel_event(Rel::REL_WHEEL_HI_RES, vertical));
        }
        if horizontal != 0 {
            events.push(rel_event(Rel::REL_HWHEEL_HI_RES, horizontal));
        }
        if notches.0 != 0 {
            events.push(rel_event(Rel::REL_WHEEL, notches.0));
        }
        if notches.1 != 0 {
            events.push(rel_event(Rel::REL_HWHEEL, notches.1));
        }
        if !events.is_empty() {
            self.mouse.emit(&events)?;
        }
        Ok(())
    }
}

fn mouse_code(b: MouseButton) -> KeyCode {
    match b {
        MouseButton::Left => KeyCode::BTN_LEFT,
        MouseButton::Right => KeyCode::BTN_RIGHT,
        MouseButton::Middle => KeyCode::BTN_MIDDLE,
        MouseButton::Back => KeyCode::BTN_SIDE,
        MouseButton::Forward => KeyCode::BTN_EXTRA,
    }
}

fn key_event(code: KeyCode, pressed: bool) -> InputEvent {
    InputEvent::new(EventType::KEY.0, code.0, pressed as i32)
}

fn abs_event(code: Abs, value: i32) -> InputEvent {
    InputEvent::new(EventType::ABSOLUTE.0, code.0, value)
}

fn rel_event(code: Rel, value: i32) -> InputEvent {
    InputEvent::new(EventType::RELATIVE.0, code.0, value)
}

/// Moves the mouse `px` pixels to the right, smoothly over a second, after a short wait that
/// lets the user switch to a game. For checking how far that turns the game's camera. Runs on
/// its own thread with its own virtual mouse.
pub fn test_turn(px: i32) {
    use std::{thread, time::Duration};
    const WAIT: Duration = Duration::from_secs(3);
    const STEPS: i32 = 100;
    thread::spawn(move || {
        let Ok(mut kbm) = VirtualKbm::new() else { return };
        thread::sleep(WAIT);
        let mut sent = 0;
        for step in 1..=STEPS {
            let target = (i64::from(px) * i64::from(step) / i64::from(STEPS)) as i32;
            if kbm.mouse_move(target - sent, 0).is_err() {
                return;
            }
            sent = target;
            thread::sleep(Duration::from_millis(10));
        }
    });
}
