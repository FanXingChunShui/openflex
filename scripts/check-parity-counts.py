#!/usr/bin/env python3
"""Verify the parity matrix's summary line against the tables themselves.

The counts drifted more than once: rows were added or re-classified and the
"矩阵共 N 行 / ✅ A / 🟡 B / ⛔ C / 📄 D" summary was edited by hand (or not at
all). This check recomputes them from the tables and fails when they disagree,
so the headline number cannot lie about the body.
"""

import re
import sys

PATH = "docs/FEATURE_PARITY_75.md"
EMOJI = ("✅", "🟡", "⛔", "📄")


def main() -> int:
    lines = open(PATH, encoding="utf-8").read().split("\n")
    counts = {emoji: 0 for emoji in EMOJI}
    total = 0
    in_matrix = False
    for line in lines:
        if line.startswith("| FlexSim 7.5 功能"):
            in_matrix = True
            continue
        if not line.startswith("|"):
            in_matrix = False
            continue
        if not in_matrix:
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) < 3:
            continue
        status = cells[2]
        if set(status) <= set("-: "):
            continue
        for emoji in EMOJI:
            if status.startswith(emoji):
                counts[emoji] += 1
                total += 1
                break

    text = "\n".join(lines)
    match = re.search(r"矩阵共 \*\*(\d+) 行\*\*", text)
    if not match:
        print("no `矩阵共 **N 行**` summary line found", file=sys.stderr)
        return 1
    declared_total = int(match.group(1))
    declared = {}
    for emoji in EMOJI:
        found = re.search(rf"{emoji} (\d+) 行", text)
        declared[emoji] = int(found.group(1)) if found else None

    problems = []
    if declared_total != total:
        problems.append(f"total: summary says {declared_total}, tables have {total}")
    for emoji in EMOJI:
        if declared[emoji] is not None and declared[emoji] != counts[emoji]:
            problems.append(
                f"{emoji}: summary says {declared[emoji]}, tables have {counts[emoji]}"
            )
    if problems:
        print("parity summary does not match the tables:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1

    print(
        "parity counts ok: "
        + ", ".join(f"{emoji} {counts[emoji]}" for emoji in EMOJI)
        + f", total {total}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
