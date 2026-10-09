#!/usr/bin/env python3
"""A gamepad made through /dev/uhid, so the daemon sees a HID device like a real controller.

Usage: fake-pad.py <fifo>   (creates the device, then reads commands from the FIFO)
  echo press > <fifo>   presses South once
  echo quit > <fifo>    removes the device and exits

The kernel drops reports until the daemon has opened the device, so send `press` again until the
press shows up. Same layout as tests/common/uhid.rs. Needs write access to /dev/uhid.
"""
import os
import struct
import sys
import time

EVENT_SIZE = 4376  # struct uhid_event, packed
UHID_DESTROY, UHID_CREATE2, UHID_INPUT2 = 1, 11, 12
BUS_USB = 0x0003
DESCRIPTOR = bytes([
    0x05, 0x01, 0x09, 0x05, 0xA1, 0x01,  # Generic Desktop, Gamepad, Collection
    0x05, 0x09, 0x19, 0x01, 0x29, 0x04,  # Buttons 1 to 4
    0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x04, 0x81, 0x02,  # four 1-bit buttons
    0x75, 0x04, 0x95, 0x01, 0x81, 0x03,  # padding
    0x05, 0x01, 0x09, 0x30, 0x09, 0x31,  # X and Y
    0x16, 0x00, 0x80, 0x26, 0xFF, 0x7F, 0x75, 0x10, 0x95, 0x02, 0x81, 0x02,  # two 16-bit axes
    0xC0,  # End Collection
])
SOUTH = 0b0001


def send(fd, event_type, payload=b""):
    event = struct.pack("<I", event_type) + payload
    os.write(fd, event + b"\0" * (EVENT_SIZE - len(event)))


def create(fd):
    payload = struct.pack(
        "<128s64s64sHHIIII",
        b"Padwight test pad", b"", b"", len(DESCRIPTOR), BUS_USB, 0x045E, 0x028E, 1, 0,
    ) + DESCRIPTOR.ljust(4096, b"\0")
    send(fd, UHID_CREATE2, payload)


def report(fd, buttons):
    send(fd, UHID_INPUT2, struct.pack("<H", 5) + struct.pack("<Bhh", buttons, 0, 0).ljust(4096, b"\0"))


def main():
    fifo = sys.argv[1]
    if not os.path.exists(fifo):
        os.mkfifo(fifo)
    fd = os.open("/dev/uhid", os.O_RDWR)
    create(fd)
    print("pad created", flush=True)
    while True:
        # Each writer opens the FIFO for one command, so reopen after every end of file.
        with open(fifo) as commands:
            for line in commands:
                command = line.strip()
                if command == "press":
                    report(fd, SOUTH)
                    time.sleep(0.15)
                    report(fd, 0)
                elif command == "quit":
                    send(fd, UHID_DESTROY)
                    return


if __name__ == "__main__":
    main()
