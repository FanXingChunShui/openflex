#!/usr/bin/env python3
"""Flag matrix gaps that already have evidence.

Why this exists: a 🟡 row's "缺 X" is a *claim*. Twice now a claim was out of
date — the table-driven arrival fix landed after the row was written, and the
"checkpoint does not save chain state" gap had already been closed by task-183.
Reading a stale gap is worse than a stale ✅: it invites a week of work on
something that already runs.

The check is deliberately narrow (low false positives, some false negatives):

    a 🟡 row's 缺-phrase mentions a backticked identifier, that identifier is a
    `pub` field in model.rs, and a test function's name contains it.

The last condition is what makes it actionable: a field with *behaviour tests
named after it* is very likely implemented, not missing. Identifiers that are
mere asides ("仍缺 … 的 `extra_failures` 语义") live in the allowlist.

Usage: python3 scripts/check-stale-gaps.py [--list]
Exit: 0 clean, 1 unlisted suspects, 2 usage error
"""
import glob
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MATRIX = ROOT / "docs" / "FEATURE_PARITY_75.md"
ALLOW = ROOT / "docs" / "stale-gap-allowlist.txt"
MODEL = ROOT / "crates" / "openflex-model" / "src" / "model.rs"


def tests():
    names = set()
    for path in glob.glob(str(ROOT / "crates" / "*" / "src" / "*.rs")) + glob.glob(
        str(ROOT / "crates" / "*" / "tests" / "*.rs")
    ):
        for line in open(path, encoding="utf-8", errors="ignore"):
            match = re.match(r"\s*fn ([a-z0-9_]+)\(", line)
            if match:
                names.add(match.group(1))
    return names


def main():
    list_mode = "--list" in sys.argv[1:]
    fields = set(re.findall(r"\bpub ([a-z_][a-z0-9_]*):", MODEL.read_text(encoding="utf-8")))
    test_names = tests()
    allow = set()
    if ALLOW.exists():
        for line in ALLOW.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line and not line.startswith("#"):
                allow.add(line)

    suspects = []
    for number, line in enumerate(MATRIX.read_text(encoding="utf-8").splitlines(), 1):
        if not line.startswith("|") or "🟡" not in line:
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) < 3:
            continue
        feature, gap = cells[0], cells[-1]
        for chunk in re.finditer(r"缺[^|。]{0,120}", gap):
            for name in re.findall(r"`([a-zA-Z_][a-zA-Z0-9_]*)`", chunk.group(0)):
                if name not in fields or name in allow:
                    continue
                # The name must be a whole word at the start of the test name:
                # `group` vs `groups_steps_per_item_in_time_order` is not a hit.
                hit = next((t for t in test_names if t.startswith(name + "_")), None)
                if hit:
                    suspects.append((number, feature[:44], name, hit))

    if list_mode:
        print(f"allowlisted: {', '.join(sorted(allow)) or '(none)'}")
    for line, feature, name, test in suspects:
        print(f"{MATRIX.name}:{line}: {feature} :: 缺 `{name}`, but `{test}` exists")
    if suspects:
        print(f"\n{len(suspects)} stale-gap suspect(s); confirm the row and update it", file=sys.stderr)
        return 1
    print("no stale gaps: every 🟡 缺-claim is still a claim")
    return 0


if __name__ == "__main__":
    sys.exit(main())
