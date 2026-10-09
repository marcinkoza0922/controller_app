#!/usr/bin/env bash
# Checks whether the desktops have released versions the app doesn't cover yet. Needs network
# access. Exits 1 when GNOME has a newer stable major release than the extension lists, since
# GNOME Shell refuses an extension for any version it isn't listed for.
#
# Usage: scripts/desktop-versions.sh
# Then run scripts/desktop-test.sh on a VM with the new version before you release.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
metadata="$root/gnome-extension/padwight-focus@io.github.marcinkoza0922/metadata.json"
status=0

listed=$(python3 -c 'import json, sys; print(max(int(v) for v in json.load(open(sys.argv[1]))["shell-version"]))' "$metadata")
# GNOME's release directory lists one folder per major version, like 51/. Older folders are
# named by minor version too (3.0/), so only the plain major folders are counted.
gnome=$(curl -fsSL https://download.gnome.org/sources/gnome-shell/ |
    grep -oE 'href="[0-9]+/"' | grep -oE '[0-9]+' | sort -n | tail -1)
if [ "$gnome" -gt "$listed" ]; then
    echo "GNOME: $gnome is out, the extension lists up to $listed. Add $gnome to $metadata, then run scripts/desktop-test.sh gnome."
    status=1
else
    echo "GNOME: $gnome is the latest stable major, the extension lists up to $listed. OK"
fi

# Plasma's folders are named by full version, like 6.5.2/. KWin's scripting API has stayed the
# same within a major version, so only a new major is flagged.
plasma=$(curl -fsSL https://download.kde.org/stable/plasma/ |
    grep -oE 'href="[0-9]+\.[0-9]+\.[0-9]+/"' | sed -E 's/href="([0-9]+)\..*/\1/' | sort -n | tail -1)
echo "Plasma: latest major is $plasma. Run scripts/desktop-test.sh kde after a new major, or after a KWin change that matters."

latest_tag() {
    curl -fsSL "https://api.github.com/repos/$1/releases/latest" | python3 -c 'import json, sys; print(json.load(sys.stdin)["tag_name"])'
}
echo "Sway: latest release is $(latest_tag swaywm/sway). Run scripts/desktop-test.sh sway when it changes."
echo "Hyprland: latest release is $(latest_tag hyprwm/Hyprland). Run scripts/desktop-test.sh hyprland when it changes."
echo "Kernel: this machine runs $(uname -r). Run the desktop checks on a VM with the kernel you ship."

exit "$status"
