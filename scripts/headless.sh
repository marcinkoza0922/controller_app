#!/usr/bin/env bash
# Runs padwight on a virtual display, with its own config, state and runtime folders. No monitor
# is needed, and the real desktop session and daemon are left alone. For tests and screenshots
# only: it isn't in the app's help and isn't part of normal use.
#
# Usage: scripts/headless.sh [padwight arguments...]     (default: gui)
#   PADWIGHT_HEADLESS_BACKEND=kwin|xvfb     kwin (default) or xvfb, see below
#   PADWIGHT_HEADLESS_SIZE=1920x1080        the virtual screen's size (kwin)
#   PADWIGHT_HEADLESS_SCREEN=1920x1080x24   the virtual screen's size and depth (xvfb)
#   PADWIGHT_HEADLESS_KEEP=1                keep the folders afterwards and print where they are
#
# kwin: KWin's own Wayland session on a virtual output, the compositor a KDE Plasma session runs.
#   The on-screen overlays are layer-shell surfaces and show here as they do on KDE. Needs
#   kwin_wayland and dbus-run-session.
# xvfb: an X server, for the settings window only. Xvfb has no layer-shell, so the overlays
#   can't show. Needs xvfb-run (xvfb and xauth).
#
# Neither backend starts a daemon. For the window to talk to one, start a daemon with its own
# XDG_RUNTIME_DIR, as tests/gui_smoke.rs does.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
backend=${PADWIGHT_HEADLESS_BACKEND:-kwin}
case $backend in
kwin) tools=(kwin_wayland dbus-run-session) ;;
xvfb) tools=(xvfb-run) ;;
*)
    echo "unknown backend: $backend (kwin or xvfb)" >&2
    exit 2
    ;;
esac
for tool in "${tools[@]}"; do
    command -v "$tool" >/dev/null || {
        echo "$tool is missing: the $backend backend needs ${tools[*]} (see the header of this script)" >&2
        exit 1
    }
done
cargo build --quiet --bin padwight
bin="${CARGO_TARGET_DIR:-$root/target}/debug/padwight"

home=$(mktemp -d "${TMPDIR:-/tmp}/padwight-headless.XXXXXX")
cleanup() {
    if [ "${PADWIGHT_HEADLESS_KEEP:-}" = 1 ]; then
        echo "kept $home" >&2
    else
        rm -rf "$home"
    fi
}
trap cleanup EXIT
mkdir -p "$home/config" "$home/state" "$home/run" "$home/tmp"
chmod 700 "$home/run"

# The app's own folders. Both backends run it with these.
app_env=(XDG_CONFIG_HOME="$home/config" XDG_STATE_HOME="$home/state" XDG_RUNTIME_DIR="$home/run" TMPDIR="$home/tmp")

run_kwin() {
    local socket="padwight-$$"
    local size=${PADWIGHT_HEADLESS_SIZE:-1920x1080}
    local width=${size%x*} height=${size#*x}
    # KWin runs this file as its session, so the app's environment and arguments are written into
    # it, quoted, rather than passed on KWin's command line (where they would start as apps).
    local session="$home/session.sh"
    {
        printf '#!/bin/sh\nexec env WAYLAND_DISPLAY=%q' "$socket"
        printf ' %q' "${app_env[@]}" "$bin" "$@"
        printf '\n'
    } >"$session"
    chmod +x "$session"
    # A session bus of its own, so KWin's D-Bus services don't reach the real desktop's.
    XDG_RUNTIME_DIR="$home/run" dbus-run-session -- \
        kwin_wayland --virtual --socket "$socket" --width "$width" --height "$height" \
        --no-lockscreen --no-global-shortcuts --exit-with-session "$session"
}

run_xvfb() {
    # xvfb-run -a picks a free display number and stops the server when the app exits.
    xvfb-run -a -s "-screen 0 ${PADWIGHT_HEADLESS_SCREEN:-1920x1080x24}" \
        env "${app_env[@]}" "$bin" "${@:-gui}"
}

# Arguments pass through to the app, so `"${@:-gui}"` defaults to the settings window.
case $backend in
kwin) run_kwin "$@" ;;
xvfb) run_xvfb "$@" ;;
esac
