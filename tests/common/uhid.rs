//! A gamepad made through the kernel's uhid, so the daemon sees a HID device like a real controller.
//! The daemon ignores uinput devices (they count as its own), so a uinput stand-in won't be grabbed.

use std::{
    fs::{File, OpenOptions},
    io::Write,
};

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

pub const BUTTON_SOUTH: u8 = 0b0001;
pub const BUTTON_EAST: u8 = 0b0010;

/// Size of the kernel's `struct uhid_event` (packed): a type, then the largest payload.
const UHID_EVENT_SIZE: usize = 4376;
const UHID_CREATE2: u32 = 11;
const UHID_INPUT2: u32 = 12;

/// A gamepad made through the kernel's uhid, so it is a HID device like a real controller. The
/// daemon ignores uinput devices (they count as its own), so a uinput stand-in won't be grabbed.
pub struct HidPad {
    file: File,
}

impl HidPad {
    /// A controller named `name`. Dropping it removes the device, as an unplug does.
    pub fn new(name: &str) -> Self {
        let file = OpenOptions::new().read(true).write(true).open("/dev/uhid").expect("opening /dev/uhid (needs root or the uhid device)");
        let mut pad = HidPad { file };
        let mut event = vec![0u8; UHID_EVENT_SIZE];
        event[0..4].copy_from_slice(&UHID_CREATE2.to_le_bytes());
        // Packed `uhid_create2_req`, starting after the type: name[128], phys[64], uniq[64], then:
        event[4..4 + name.len()].copy_from_slice(name.as_bytes());
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
    pub fn report(&mut self, buttons: u8, x: i16, y: i16) {
        let mut event = vec![0u8; UHID_EVENT_SIZE];
        event[0..4].copy_from_slice(&UHID_INPUT2.to_le_bytes());
        event[4..6].copy_from_slice(&5u16.to_le_bytes()); // size: buttons, then X and Y
        event[6] = buttons;
        event[7..9].copy_from_slice(&x.to_le_bytes());
        event[9..11].copy_from_slice(&y.to_le_bytes());
        self.file.write_all(&event).unwrap();
    }
}
