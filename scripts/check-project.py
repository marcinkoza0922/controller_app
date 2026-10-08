#!/usr/bin/env python3
"""Project checks that the Rust tests don't cover. Standard library only.

  - every `padwight <command>` in the docs is a command the binary accepts
  - the pack format number agrees across the code and the docs
  - relative and github.com/…/blob/main links in the docs resolve (and their #anchors)
  - flatpak/cargo-sources.json lists exactly the crates in Cargo.lock
  - every bundled font has a license file

Run from anywhere: `scripts/check-project.py`. Exits 1 and lists each problem it finds.
"""

import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
problems = []


def fail(msg):
    problems.append(msg)


def rel(path):
    return path.relative_to(ROOT)


def doc_files():
    return [ROOT / "README.md", *sorted((ROOT / "docs").rglob("*.md")), *sorted((ROOT / "wiki").glob("*.md"))]


def check_commands():
    main = (ROOT / "src/main.rs").read_text()
    known = set(re.findall(r'\["([a-z][a-z-]*)"', main))
    if not known:
        fail("src/main.rs: found no subcommands to check against")
        return
    for path in doc_files():
        in_fence = False
        for n, line in enumerate(path.read_text().splitlines(), 1):
            if line.lstrip().startswith("```"):
                in_fence = not in_fence
                continue
            candidates = []
            if in_fence:
                m = re.match(r"\s*padwight\s+([a-z][a-z-]*)", line)
                if m:
                    candidates.append(m.group(1))
            else:
                for span in re.findall(r"`([^`]+)`", line):
                    m = re.match(r"padwight\s+([a-z][a-z-]*)", span)
                    if m:
                        candidates.append(m.group(1))
            for cmd in candidates:
                if cmd not in known:
                    fail(f"{rel(path)}:{n}: `padwight {cmd}` is not a command (known: {', '.join(sorted(known))})")


def check_format_versions():
    code = re.search(r"pub const FORMAT: u32 = (\d+);", (ROOT / "src/pack.rs").read_text())
    if not code:
        fail("src/pack.rs: no `pub const FORMAT: u32` found")
        return
    want = code.group(1)
    checks = {
        "README.md": r"\(format (\d+)\)",
        "docs/pack-format.md": r"describes \*\*format (\d+)\*\*",
    }
    for file, pattern in checks.items():
        m = re.search(pattern, (ROOT / file).read_text())
        if not m:
            fail(f"{file}: no pack format number found")
        elif m.group(1) != want:
            fail(f"{file}: says pack format {m.group(1)}, src/pack.rs has {want}")


def slug(heading):
    """GitHub's anchor for a heading."""
    s = heading.strip().lower()
    s = re.sub(r"[^\w\- ]", "", s)
    return s.replace(" ", "-")


def anchors(path):
    text = path.read_text()
    return {slug(m.group(1)) for m in re.finditer(r"^#{1,6}\s+(.+?)\s*$", text, re.M)}


def check_links():
    github = re.compile(r"https://github\.com/[^/]+/[^/]+/blob/main/([^)\s#]+)(?:#([^)\s]+))?")
    local = re.compile(r"\]\(([^)\s]+)\)")
    for path in doc_files():
        for n, line in enumerate(path.read_text().splitlines(), 1):
            for target in [m.group(0)[2:-1] for m in local.finditer(line)]:
                if target.startswith(("http://", "https://", "mailto:")):
                    m = github.fullmatch(target)
                    if m:
                        file_part, anchor = m.group(1), m.group(2)
                        dest = ROOT / file_part
                        if not dest.exists():
                            fail(f"{rel(path)}:{n}: {target} points at a missing file")
                        elif anchor and anchor not in anchors(dest):
                            fail(f"{rel(path)}:{n}: {target} has no such heading")
                    continue
                file_part, _, anchor = target.partition("#")
                if file_part == "":
                    dest = path
                else:
                    dest = (path.parent / file_part).resolve()
                    if not dest.exists() and (dest.parent / (dest.name + ".md")).exists():
                        dest = dest.parent / (dest.name + ".md")  # wiki links drop the extension
                if not dest.exists():
                    fail(f"{rel(path)}:{n}: link to missing file `{target}`")
                elif anchor and dest.suffix == ".md" and anchor not in anchors(dest):
                    fail(f"{rel(path)}:{n}: `{target}` has no such heading")


def check_flatpak_sources():
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    locked = {
        (p["name"], p["version"])
        for p in lock["package"]
        if p.get("source", "").startswith("registry+")
    }
    sources = json.loads((ROOT / "flatpak/cargo-sources.json").read_text())
    listed = set()
    for entry in sources:
        if entry.get("type") != "archive":
            continue
        # https://…/crates/<name>/<name>-<version>.crate (the name is the directory)
        m = re.search(r"/crates/([^/]+)/([^/]+)\.crate$", entry.get("url", ""))
        if m and m.group(2).startswith(m.group(1) + "-"):
            listed.add((m.group(1), m.group(2)[len(m.group(1)) + 1 :]))
    for name, version in sorted(locked - listed):
        fail(f"flatpak/cargo-sources.json lacks {name} {version} (Cargo.lock has it): regenerate it")
    for name, version in sorted(listed - locked):
        fail(f"flatpak/cargo-sources.json has {name} {version}, which Cargo.lock doesn't: regenerate it")


def check_font_licenses():
    fonts = ROOT / "assets/fonts"
    licenses = {p.name.split("-LICENSE")[0] for p in fonts.glob("*-LICENSE*")}
    for font in sorted(fonts.iterdir()):
        if "LICENSE" in font.name:
            continue
        family = font.name.split("-")[0]
        if family not in licenses:
            fail(f"assets/fonts/{font.name}: no {family}-LICENSE file beside it")


def main():
    for check in (check_commands, check_format_versions, check_links, check_flatpak_sources, check_font_licenses):
        check()
    if problems:
        print("\n".join(problems))
        print(f"\n{len(problems)} problem(s)")
        return 1
    print("project checks pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
