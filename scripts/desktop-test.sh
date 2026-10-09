#!/usr/bin/env bash
# Checks focus tracking and profile switching inside a real desktop session on a test machine.
# It syncs the checkout, builds the release binary, starts the daemon in the session, opens a
# terminal, and checks that the daemon reports the right tracker and window, switches to a rule's
# profile while the window has focus, and falls back to the default profile after it closes.
#
# Only ever point this at a throwaway VM. The daemon grabs gamepads and creates virtual input
# devices, which would take over your own desktop.
#
# Usage: DESKTOP_TEST_HOST=dev@127.0.0.1 DESKTOP_TEST_SSH_OPTS="-p 2457 -i vms/id_ed25519" \
#        scripts/desktop-test.sh <gnome|kde|sway|hyprland|labwc>
#   DESKTOP_TEST_QMP=<qmp socket>  optional: sends Escape through QEMU first, to close GNOME's
#                                  Overview after login (VMs started by vms/vm.sh have one).
#
# Re-run it after changing how the daemon talks to the system, and when a desktop, the kernel or
# the input stack gets a new release. Record the versions it prints with the result.
set -euo pipefail

desktop=${1:?usage: scripts/desktop-test.sh gnome|kde|sway|hyprland|labwc}
case $desktop in
gnome | kde | sway | hyprland | labwc) ;;
*)
    echo "unknown desktop: $desktop" >&2
    exit 2
    ;;
esac
host=${DESKTOP_TEST_HOST:?set DESKTOP_TEST_HOST, for example dev@127.0.0.1}
read -ra ssh_opts <<<"${DESKTOP_TEST_SSH_OPTS:-}"
root=$(cd "$(dirname "$0")/.." && pwd)
profile=$(mktemp)
trap 'rm -f "$profile"' EXIT

remote() { ssh "${ssh_opts[@]}" "$host" "$@"; }

# The rule's profile is the default Gamepad profile under another name, so it maps the same buttons.
python3 - "$root/tests/fixtures/default-config.toml" >"$profile" <<'PY'
import sys
lines, seen, out = open(sys.argv[1]).read().splitlines(), False, []
for line in lines:
    if line.startswith("[[general.profiles]]"):
        if seen:
            break
        seen = True
        continue
    if seen and line.startswith("[") and not line.startswith("[general.profiles."):
        break
    if seen:
        out.append(line.replace("[general.profiles.", "["))
print("\n".join(out).replace('name = "Gamepad"', 'name = "Console Pad"', 1))
PY

echo "== syncing the checkout to $host"
rsync -az --delete --exclude target --exclude vms --exclude .git -e "ssh ${DESKTOP_TEST_SSH_OPTS:-}" "$root/" "$host:controller_app/"

echo "== building the release binary"
remote 'cd controller_app && cargo build --release'

if [ "$desktop" = gnome ] && [ -n "${DESKTOP_TEST_QMP:-}" ]; then
    echo "== closing the GNOME Overview"
    printf '%s\n' '{"execute":"qmp_capabilities"}' \
        '{"execute":"input-send-event","arguments":{"events":[{"type":"key","data":{"down":true,"key":{"type":"qcode","data":"esc"}}},{"type":"key","data":{"down":false,"key":{"type":"qcode","data":"esc"}}}]}}' |
        timeout 5 socat - "UNIX-CONNECT:$DESKTOP_TEST_QMP" >/dev/null
    sleep 2
fi

echo "== running the $desktop checks"
{
    printf 'PROFILE_B64=%s\n' "$(base64 -w0 "$profile")"
    cat <<'SCENARIO'
set -u
uid=$(id -u)
export XDG_RUNTIME_DIR=/run/user/$uid DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$uid/bus
export WAYLAND_DISPLAY=$(basename "$(ls /run/user/$uid/wayland-* | grep -v lock | head -1)")
bin=$HOME/controller_app/target/release/padwight
config=$HOME/.config/padwight
desktop=$1

case $desktop in
gnome) window=kgx; class=org.gnome.Console; tracker="GNOME Shell" ;;
kde) window=konsole; class=org.kde.konsole; tracker=KWin ;;
sway) window=foot; class=foot; tracker=Sway ;;
hyprland) window=kitty; class=kitty; tracker=Hyprland ;;
labwc) window=foot; class=foot; tracker=wlroots ;;
esac

failures=0
pass() { echo "PASS $1"; }
fail() { echo "FAIL $1"; failures=$((failures + 1)); }
# Polls until the command succeeds, for up to 15 seconds.
eventually() { for _ in $(seq 1 15); do "$@" && return 0; sleep 1; done; return 1; }
status() { "$bin" status 2>&1; }
on_profile() { status | grep -q "^profile: $1"; }
focused_is() { status | grep -q "focused: .*(class $1)"; }
log_has() { grep -q "$1" /tmp/desktop-test-daemon.log 2>/dev/null; }
stop_daemon() { pkill -x padwight 2>/dev/null || true; sleep 1; }

echo "== versions"
echo "kernel $(uname -r)"
for tool in gnome-shell plasmashell sway Hyprland labwc; do
    command -v "$tool" >/dev/null && echo "$tool: $("$tool" --version 2>/dev/null | head -1)"
done

# Test VMs don't always have the udev rule applied (see the notes in vms/provision.sh).
if [ ! -w /dev/uinput ]; then
    sudo -n chmod 0666 /dev/uinput || { echo "/dev/uinput is not writable and sudo needs a password"; exit 1; }
fi

# The daemon makes the config on its first start, and that start would clear setups written before
# it, so make the config first.
stop_daemon
if [ ! -f "$config/config.toml" ]; then
    (setsid nohup "$bin" daemon >/tmp/desktop-test-daemon.log 2>&1 </dev/null &)
    eventually log_has "daemon started" || true
    stop_daemon
fi
rm -rf "$config/setups/001-desktop-test"
setup="$config/setups/001-desktop-test"
mkdir -p "$setup/profiles"
printf 'name = "Desktop test"\n\n[[rules]]\nkind = "window_class"\nvalue = "%s"\nprofile = "Console Pad"\n' "$class" >"$setup/setup.toml"
echo "$PROFILE_B64" | base64 -d >"$setup/profiles/001-Console-Pad.toml"

: >/tmp/desktop-test-daemon.log
(setsid nohup "$bin" daemon >/tmp/desktop-test-daemon.log 2>&1 </dev/null &)

if eventually log_has "focus tracking: focused window ($tracker)"; then
    pass "daemon tracks focus through $tracker"
else
    fail "daemon tracks focus through $tracker"
    grep "focus tracking" /tmp/desktop-test-daemon.log | tail -1 >&2 || true
fi

if [ "$desktop" = gnome ]; then
    if eventually bash -c 'gdbus call --session -d org.gnome.Shell -o /org/gnome/Shell -m org.gnome.Shell.Extensions.GetExtensionInfo padwight-focus@io.github.marcinkoza0922 | grep -q "state.: <1.0>"'; then
        pass "the GNOME extension is enabled"
    else
        fail "the GNOME extension is enabled (a log out and back in may be needed after an update)"
    fi
fi

(setsid nohup "$window" >/tmp/desktop-test-window.log 2>&1 </dev/null &)
if eventually focused_is "$class"; then
    pass "a focused $window window is reported"
else
    fail "a focused $window window is reported (is the overview or a dialog holding focus?)"
fi

if eventually on_profile "Console Pad"; then
    pass "the rule switches to Console Pad while $window has focus"
else
    fail "the rule switches to Console Pad while $window has focus"
fi

pkill -x "$window" 2>/dev/null || true
if eventually on_profile "Gamepad"; then
    pass "the profile falls back to Gamepad after $window closes"
else
    fail "the profile falls back to Gamepad after $window closes"
fi

if grep -q panicked /tmp/desktop-test-daemon.log; then
    fail "the daemon didn't panic"
else
    pass "the daemon didn't panic"
fi

stop_daemon
rm -rf "$setup"
echo "== $failures failure(s)"
exit "$failures"
SCENARIO
} | remote "bash -s -- $desktop"
