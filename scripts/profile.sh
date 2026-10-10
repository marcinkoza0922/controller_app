#!/bin/sh
# Runs the profiling suite (tests/profile) in a container: input latency, CPU and memory use,
# daemon start-up (cold and warm) and settings-window start-up. It builds in release mode, under
# a virtual X server (Xvfb) for the window, and as root, so the uhid controller and the uinput
# devices work. The results are written to target/profile/ in this checkout, one JSON file each.
#
# The container's daemon can see the host's input devices (the test needs them), so a cold start
# grabs real controllers for a moment, as a first launch would. Close games first.
#
# When the run finishes, the charts and target/profile/report.html are made from the results
# (scripts/plot_profile.py, on the host, needs matplotlib). Rerun that alone with
# `python3 scripts/plot_profile.py`.
#
# Usage: scripts/profile.sh [test filter, default: profile_]
#   scripts/profile.sh profile_input_latency   runs one test
set -eu

filter=${1:-profile_}
root=$(cd "$(dirname "$0")/.." && pwd)

for dev in /dev/uinput /dev/uhid; do
    if [ ! -e "$dev" ]; then
        echo "$dev is missing: load its module first (sudo modprobe ${dev#/dev/})" >&2
        exit 1
    fi
done

mkdir -p "$root/target/profile"

# Build output and the cargo registry live in named volumes, so the container doesn't write
# root-owned files into the checkout. Only the results come back, and they are given to this user.
status=0
docker run --rm \
    --device /dev/uinput \
    --device /dev/uhid \
    --device-cgroup-rule 'c 13:* rwm' \
    -v /dev/input:/dev/input \
    -v "$root":/src \
    -v "$root/target/profile":/out \
    -v padwight-profile-target:/target \
    -v padwight-profile-cargo:/usr/local/cargo/registry \
    -e CARGO_TARGET_DIR=/target \
    -e PADWIGHT_PROFILE_DIR=/out \
    -e RUST_BACKTRACE=0 \
    -w /src \
    -u 0 \
    rust:1 \
    sh -c "apt-get update -qq && apt-get install -y -qq pkg-config libxkbcommon-dev libwayland-dev libudev-dev libfontconfig1-dev libdbus-1-dev libasound2-dev xvfb xauth libxcursor1 libxrandr2 libxi6 libxinerama1 libxkbcommon-x11-0 x11-utils >/dev/null \
        && xvfb-run -a cargo test --release --test profile -- --ignored --test-threads=1 --nocapture $filter; status=\$?; \
        chown -R $(id -u):$(id -g) /out; exit \$status" || status=$?

# The charts are made even when a test failed, from whatever results were written.
python3 "$root/scripts/plot_profile.py" "$root/target/profile"
exit $status
