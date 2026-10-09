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
text = "\n".join(out).replace('name = "Gamepad"', 'name = "Console Pad"', 1)
south = '[buttons.South]\ngamepad = "South"'
assert south in text, "the default Gamepad profile maps South to a gamepad button"
print(text.replace(south, '[buttons.South]\nkeys = ["KEY_A"]', 1))
PY

echo "== syncing the checkout to $host"
rsync -az --delete --exclude target --exclude vms --exclude .git --exclude .flatpak-builder --exclude build-dir --exclude repo -e "ssh ${DESKTOP_TEST_SSH_OPTS:-}" "$root/" "$host:controller_app/"

if [ -n "${DESKTOP_TEST_FLATPAK:-}" ]; then
    echo "== building the Flatpak (the first time takes a long while: it downloads the runtimes)"
    remote 'set -e
command -v flatpak-builder >/dev/null || sudo pacman -S --noconfirm --needed flatpak flatpak-builder >/dev/null
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user -y --noninteractive flathub org.freedesktop.Platform//25.08 org.freedesktop.Sdk//25.08 org.freedesktop.Sdk.Extension.rust-stable//25.08 >/dev/null
cd controller_app && flatpak-builder --user --install --force-clean --disable-rofiles-fuse build-dir flatpak/io.github.marcinkoza0922.Padwight.yml >/tmp/flatpak-build.log 2>&1'
else
    echo "== building the release binary"
    remote 'cd controller_app && cargo build --release'
fi

if [ "$desktop" = gnome ] && [ -n "${DESKTOP_TEST_QMP:-}" ]; then
    echo "== closing the GNOME Overview"
    printf '%s\n' '{"execute":"qmp_capabilities"}' \
        '{"execute":"input-send-event","arguments":{"events":[{"type":"key","data":{"down":true,"key":{"type":"qcode","data":"esc"}}},{"type":"key","data":{"down":false,"key":{"type":"qcode","data":"esc"}}}]}}' |
        timeout 5 socat - "UNIX-CONNECT:$DESKTOP_TEST_QMP" >/dev/null
    sleep 2
fi

echo "== running the $desktop checks"
{
    printf 'PROFILE_B64=%s\nFLATPAK=%s\n' "$(base64 -w0 "$profile")" "${DESKTOP_TEST_FLATPAK:-}"
    cat <<'SCENARIO'
set -u
uid=$(id -u)
export XDG_RUNTIME_DIR=/run/user/$uid DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$uid/bus
export WAYLAND_DISPLAY=$(basename "$(ls /run/user/$uid/wayland-* | grep -v lock | head -1)")
# A Flatpak runs the app in its own sandbox, with its own config folder.
if [ -n "${FLATPAK:-}" ]; then
    bin=(flatpak run io.github.marcinkoza0922.Padwight)
    config=$HOME/.var/app/io.github.marcinkoza0922.Padwight/config/padwight
else
    bin=("$HOME/controller_app/target/release/padwight")
    config=$HOME/.config/padwight
fi
desktop=$1

# The window runs a shell that reads one key and records it, so a pad press shows where it went.
read_key='read -n1 key; printf %s "$key" > /tmp/desktop-test-key; sleep 120'
case $desktop in
gnome) window=kgx; class=org.gnome.Console; tracker="GNOME Shell"; launch=(kgx -- sh -c "$read_key") ;;
kde) window=konsole; class=org.kde.konsole; tracker=KWin; launch=(konsole -e sh -c "$read_key") ;;
sway) window=foot; class=foot; tracker=Sway; launch=(foot sh -c "$read_key") ;;
hyprland) window=kitty; class=kitty; tracker=Hyprland; launch=(kitty sh -c "$read_key") ;;
labwc) window=foot; class=foot; tracker=wlroots; launch=(foot sh -c "$read_key") ;;
esac

failures=0
pass() { echo "PASS $1"; }
fail() { echo "FAIL $1"; failures=$((failures + 1)); }
# Polls until the command succeeds, for up to 15 seconds.
eventually() { for _ in $(seq 1 15); do "$@" && return 0; sleep 1; done; return 1; }
status() { "${bin[@]}" status 2>&1; }
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
if [ ! -w /dev/uhid ]; then
    sudo -n chmod 0666 /dev/uhid || { echo "/dev/uhid is not writable and sudo needs a password"; exit 1; }
fi

# A simulated gamepad, made before the daemon starts so the daemon finds it on its first scan.
pad=/tmp/desktop-test-pad.fifo
pad_command() { timeout 3 sh -c "echo $1 > '$pad'" 2>/dev/null; }
stop_pad() { pad_command quit || true; sleep 1; }
stop_pad
rm -f "$pad"
(setsid nohup python3 "$HOME/controller_app/scripts/fake-pad.py" "$pad" >/tmp/desktop-test-pad.log 2>&1 </dev/null &)
if eventually grep -q "pad created" /tmp/desktop-test-pad.log; then
    pass "a simulated gamepad is created through uhid"
else
    fail "a simulated gamepad is created through uhid"
fi

# The daemon makes the config on its first start, and that start would clear setups written before
# it, so make the config first.
stop_daemon
if [ ! -f "$config/config.toml" ]; then
    (setsid nohup "${bin[@]}" daemon >/tmp/desktop-test-daemon.log 2>&1 </dev/null &)
    eventually log_has "daemon started" || true
    stop_daemon
fi
# The first setup whose rule matches the window wins, so remove the other setups (this VM is
# throwaway). The daemon names a setup's folder after the setup, so clear them all.
for old in "$config"/setups/*; do
    [ -d "$old" ] && rm -rf "$old"
done
setup="$config/setups/001-Desktop test"
mkdir -p "$setup/profiles"
printf 'name = "Desktop test"\n\n[[rules]]\nkind = "window_class"\nvalue = "%s"\nprofile = "Console Pad"\n' "$class" >"$setup/setup.toml"
echo "$PROFILE_B64" | base64 -d >"$setup/profiles/001-Console-Pad.toml"

: >/tmp/desktop-test-daemon.log
(setsid nohup "${bin[@]}" daemon >/tmp/desktop-test-daemon.log 2>&1 </dev/null &)

# The daemon retries the tracker every 10 seconds, so give it a little longer than that.
# wlroots compositors hide their window and overlay protocols from sandboxed apps, so a Flatpak
# there falls back to matching processes (README, Flatpak).
sandboxed_wlroots=0
if [ -n "${FLATPAK:-}" ] && { [ "$desktop" = sway ] || [ "$desktop" = labwc ]; }; then sandboxed_wlroots=1; fi
expected="focus tracking: focused window ($tracker)"
[ "$sandboxed_wlroots" = 1 ] && expected="focus tracking: running processes"
tracker_up() { for _ in $(seq 1 40); do log_has "$expected" && return 0; sleep 1; done; return 1; }
msg="daemon tracks focus through $tracker"
[ "$sandboxed_wlroots" = 1 ] && msg="a Flatpak on $tracker falls back to process matching"
if tracker_up; then
    pass "$msg"
else
    fail "$msg"
    grep "focus tracking" /tmp/desktop-test-daemon.log | tail -1 >&2 || true
fi

if [ "$desktop" = gnome ]; then
    if eventually bash -c 'gdbus call --session -d org.gnome.Shell -o /org/gnome/Shell -m org.gnome.Shell.Extensions.GetExtensionInfo padwight-focus@io.github.marcinkoza0922 | grep -q "state.: <1.0>"'; then
        pass "the GNOME extension is enabled"
    else
        fail "the GNOME extension is enabled (a log out and back in may be needed after an update)"
    fi
fi

(setsid nohup "${launch[@]}" >/tmp/desktop-test-window.log 2>&1 </dev/null &)
if [ "$sandboxed_wlroots" = 1 ]; then
    echo "SKIP focus, rules, pad input and the overlay in a Flatpak on $tracker (the compositor hides its protocols from the sandbox)"
    stop_daemon
    stop_pad
    rm -rf "$setup"
    echo "== $failures failure(s)"
    exit "$failures"
fi

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

# A press is dropped until the daemon has the pad, so press again until the key arrives.
pressed_key() { [ "$(cat /tmp/desktop-test-key 2>/dev/null)" = a ]; }
press_until_key() { for _ in $(seq 1 8); do pad_command press; sleep 2; pressed_key && return 0; done; return 1; }
if press_until_key; then
    pass "a pad press reaches the focused $window window as the mapped key"
else
    fail "a pad press reaches the focused $window window as the mapped key"
fi

# The on-screen keyboard is a layer-shell surface, which GNOME doesn't have.
overlay_running() { pgrep -f 'padwight overlay$' >/dev/null; }
# A Flatpak can't create the overlay's layer-shell surface on wlroots compositors (README, Flatpak).
if [ "$desktop" = gnome ]; then
    echo "SKIP the on-screen overlay (GNOME has no layer-shell)"
elif [ -n "${FLATPAK:-}" ] && [ "$desktop" = hyprland ]; then
    echo "SKIP the on-screen overlay (the compositor hides layer-shell from the sandbox)"
else
    "${bin[@]}" overlay-toggle >/dev/null 2>&1 || true
    if eventually overlay_running; then
        sleep 3
        if overlay_running; then
            pass "the on-screen overlay opens and stays up"
        else
            fail "the on-screen overlay opens and stays up"
        fi
    else
        fail "the on-screen overlay opens and stays up"
    fi
    "${bin[@]}" overlay-toggle >/dev/null 2>&1 || true
    # The overlay process stays running and goes back to idle, so the daemon's log is what says it closed.
    if eventually log_has "on-screen keyboard closed"; then
        pass "the on-screen overlay closes again"
    else
        fail "the on-screen overlay closes again"
    fi
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
stop_pad
rm -rf "$setup"
echo "== $failures failure(s)"
exit "$failures"
SCENARIO
} | remote "bash -s -- $desktop"
