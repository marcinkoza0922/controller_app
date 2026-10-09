#!/bin/sh
# Runs the kernel-level uinput, daemon and GUI tests in a container, under a virtual X server (Xvfb),
# so the test user doesn't need to be in the host's `input` group. The container runs as root and reads the virtual keyboard and mouse
# nodes through a bind mount of /dev/input. Your host account is unchanged.
#
# Caveat: the container can read every host input device, including your real keyboard. The
# test only opens its own virtual nodes, but the mount makes the others visible to it.
#
# Usage: scripts/kernel-test-docker.sh [test filter, default: nothing_stays]
#   scripts/kernel-test-docker.sh daemon_   runs the daemon-exit scenarios
set -eu

filter=${1:-nothing_stays}
root=$(cd "$(dirname "$0")/.." && pwd)

for dev in /dev/uinput /dev/uhid; do
    if [ ! -e "$dev" ]; then
        echo "$dev is missing: load its module first (sudo modprobe ${dev#/dev/})" >&2
        exit 1
    fi
done

# Build output and the cargo registry live in named volumes, so the container doesn't write
# root-owned files into the checkout.
exec docker run --rm \
    --device /dev/uinput \
    --device /dev/uhid \
    --device-cgroup-rule 'c 13:* rwm' \
    -v /dev/input:/dev/input \
    -v "$root":/src \
    -v padwight-kernel-target:/target \
    -v padwight-kernel-cargo:/usr/local/cargo/registry \
    -e CARGO_TARGET_DIR=/target \
    -e RUST_BACKTRACE=0 \
    -w /src \
    -u 0 \
    rust:1 \
    sh -c "apt-get update -qq && apt-get install -y -qq pkg-config libxkbcommon-dev libwayland-dev libudev-dev libfontconfig1-dev libdbus-1-dev libasound2-dev xvfb xauth libxcursor1 libxrandr2 libxi6 libxinerama1 libxkbcommon-x11-0 x11-utils >/dev/null \
        && xvfb-run -a cargo test -- --ignored $filter"
