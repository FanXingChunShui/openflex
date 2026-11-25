#!/usr/bin/env bash
# Structural checks for CHANGELOG.md.
#
# Written after making the same mistake three times: an entry added with a
# global search-and-replace landed in an *already released* section instead of
# `## Unreleased`, and duplicated `###` headings appeared inside one section.
# Content placement cannot be checked mechanically, but the shape can.
#
# Usage: bash scripts/check-changelog.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

python3 - <<'PY'
import re
import sys

text = open("CHANGELOG.md", encoding="utf-8").read()
lines = text.split("\n")

# 1. `## Unreleased` must be the first section (right after the title).
headings = [(i, line) for i, line in enumerate(lines) if line.startswith("## ")]
if not headings:
    sys.exit("CHANGELOG.md has no `## ` sections")
first = headings[0][1]
# Right after a release there is no `## Unreleased` section yet, and the newest
# version becomes the first section — both shapes are valid. What is never
# valid is an older version sitting above a newer one (checked below), or
# `## Unreleased` somewhere in the middle.
if first.startswith("## Unreleased"):
    pass
elif not re.match(r"## v\d+\.\d+\.\d+", first):
    sys.exit(f"the first section must be `## Unreleased` or a version, found {first!r}")
for index, (_, line) in enumerate(headings):
    if line.startswith("## Unreleased") and index != 0:
        sys.exit(f"`## Unreleased` must be the first section, found at position {index + 1}")

# 2. Version sections must appear newest-first.
versions = []
for _, line in headings:
    match = re.match(r"## v(\d+)\.(\d+)\.(\d+)", line)
    if match:
        versions.append(tuple(int(part) for part in match.groups()))
for newer, older in zip(versions, versions[1:]):
    if older >= newer:
        sys.exit(f"versions out of order: {older} listed after {newer}")

# 3. No empty `###` section: a heading whose body is only blank lines before the
#    next heading. Happens when an entry is moved and its heading is left behind.
empty_sections = []
lines = text.split("\n")
for index, line in enumerate(lines):
    if not line.startswith("### "):
        continue
    cursor = index + 1
    body = []
    while cursor < len(lines) and not lines[cursor].startswith("### ") and not lines[cursor].startswith("## "):
        if lines[cursor].strip():
            body.append(lines[cursor])
        cursor += 1
    if not body:
        empty_sections.append(line.strip())
if empty_sections:
    sys.exit("empty changelog section(s): " + ", ".join(empty_sections))

# 4. No duplicated `###` heading inside one section.
problems = []
current = None
seen = set()
for line in lines:
    if line.startswith("## "):
        current = line.strip()
        seen = set()
        continue
    if line.startswith("### "):
        name = line.strip()
        if name in seen and current:
            problems.append(f"{current}: `{name}` appears twice")
        seen.add(name)
if problems:
    sys.exit("\n".join(problems))

print(f"changelog ok: {len(versions)} version section(s), newest {versions[0] if versions else '-'}")
PY
