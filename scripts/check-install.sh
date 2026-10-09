#!/bin/sh
# Validates the files a user installs: the systemd units, the udev rule, the desktop entry, the
# AppStream metainfo and the Flatpak manifest. A missing validator is an error, not a skip, so a
# run can't pass without checking anything.
set -eu
cd "$(dirname "$0")/.."

need() {
    command -v "$1" >/dev/null 2>&1 || { echo "missing tool: $1 (install it, or run this in CI)" >&2; exit 1; }
}
need systemd-analyze
need udevadm
need desktop-file-validate
need appstreamcli
need python3

for unit in dist/padwight.service dist/padwight-flatpak.service; do
    systemd-analyze verify --man=no "$unit"
    echo "ok: $unit"
done

udevadm verify dist/70-padwight-motion.rules
echo "ok: dist/70-padwight-motion.rules"

desktop-file-validate flatpak/io.github.marcinkoza0922.Padwight.desktop
echo "ok: desktop entry"

# --no-net: no network access in CI. Pedantic hints are not failures.
appstreamcli validate --no-net flatpak/io.github.marcinkoza0922.Padwight.metainfo.xml
echo "ok: metainfo"

python3 - <<'PY'
import sys
try:
    import yaml
except ImportError:
    sys.exit("missing Python module: PyYAML (pip install pyyaml, or python3-yaml)")

path = "flatpak/io.github.marcinkoza0922.Padwight.yml"
with open(path) as f:
    manifest = yaml.safe_load(f)

problems = []
for key in ("id", "runtime", "runtime-version", "sdk", "command", "modules", "finish-args"):
    if key not in manifest:
        problems.append(f"{path}: missing `{key}`")
for module in manifest.get("modules", []):
    if "name" not in module:
        problems.append(f"{path}: a module has no name")
if manifest.get("command") != "padwight":
    problems.append(f"{path}: command should be padwight, is {manifest.get('command')!r}")
if problems:
    sys.exit("\n".join(problems))
print(f"ok: {path}")
PY
