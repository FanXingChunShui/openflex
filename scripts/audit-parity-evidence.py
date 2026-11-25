#!/usr/bin/env python3
"""Flag parity rows whose ✅ rests on weak or missing evidence.

The first audit only checked that cited test names exist, and a row slipped
through: `source_can_load_its_arrival_schedule_from_a_table` was a *parser* test,
while the engine never called the parser at all — the feature created nothing.
So this script classifies where each cited test lives:

  behaviour  openflex-core (src or tests) / openflex-cli tests  → runs the engine
  model      openflex-model (src or tests)                      → parsing/validation only
  report     openflex-report, openflex-gui tests                → output/UI

A ✅ row whose evidence is *only* model-layer is a suspect: the model can be
correct while the engine ignores the field. Rows with no cited test at all are
reported as having no evidence.

Usage: python3 scripts/audit-parity-evidence.py [--strict]
  --strict  exit 1 when a row has no evidence (for CI); suspects stay warnings.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MATRIX = os.path.join(ROOT, "docs", "FEATURE_PARITY_75.md")
BACKTICK = re.compile(r"`([a-z][a-z0-9_]*)`")


def test_index() -> dict:
    """test name -> list of files that define it."""
    index = {}
    for base, _, files in os.walk(os.path.join(ROOT, "crates")):
        for name in files:
            if not name.endswith(".rs"):
                continue
            path = os.path.join(base, name)
            rel = os.path.relpath(path, ROOT)
            try:
                text = open(path, encoding="utf-8").read()
            except OSError:
                continue
            for match in re.finditer(r"\bfn\s+([a-z][a-z0-9_]*)\s*\(", text):
                index.setdefault(match.group(1), []).append(rel)
    return index


def classify(paths) -> str:
    if not paths:
        return "missing"
    kinds = set()
    for path in paths:
        if path.startswith("crates/openflex-core/"):
            kinds.add("behaviour")
        elif path.startswith("crates/openflex-cli/tests/"):
            kinds.add("behaviour")
        elif path.startswith("crates/openflex-model/"):
            kinds.add("model")
        else:
            kinds.add("report")
    for kind in ("behaviour", "report", "model"):
        if kind in kinds:
            return kind
    return "model"


def matrix_rows():
    """Yield (row number, feature, status, evidence cell) for matrix tables."""
    lines = open(MATRIX, encoding="utf-8").read().split("\n")
    in_matrix = False
    number = 0
    for line in lines:
        if line.startswith("|") and "OpenFlex 现状" in line:
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
        if set(cells[2]) <= set("-: "):
            continue
        number += 1
        yield number, cells[0], cells[2], cells[3] if len(cells) > 3 else ""


def ledger_entries() -> dict:
    """feature (normalised) -> (test names, conclusion)."""
    import difflib

    path = os.path.join(ROOT, "docs", "PARITY_EVIDENCE.md")
    entries = {}
    for line in open(path, encoding="utf-8").read().split("\n"):
        if not line.startswith("| "):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) < 5 or cells[0] in ("#", "") or set(cells[0]) <= set("-: "):
            continue
        feature, evidence, conclusion = cells[1], cells[3], cells[4]
        names = [name for name in BACKTICK.findall(evidence) if "_" in name]
        entries[normalise(feature)] = (names, conclusion)
    return entries


def normalise(feature: str) -> str:
    text = re.split(r"[（(]", feature)[0]
    text = text.replace("`", "").strip()
    return text.lower()


def lookup_ledger(entries: dict, feature: str):
    import difflib

    key = normalise(feature)
    if key in entries:
        return entries[key]
    match = difflib.get_close_matches(key, list(entries), n=1, cutoff=0.72)
    return entries[match[0]] if match else (None, None)


def load_allowlist() -> set:
    path = os.path.join(ROOT, "docs", "evidence-non-test-names.txt")
    if not os.path.exists(path):
        return set()
    names = set()
    for line in open(path, encoding="utf-8"):
        line = line.split("#", 1)[0].strip()
        if line:
            names.add(line)
    return names


def main() -> int:
    strict = "--strict" in sys.argv
    allowlist = load_allowlist()
    index = test_index()
    no_evidence = []
    weak = []
    soft = []
    missing_names = []
    checked = 0

    ledger = ledger_entries()

    for number, feature, status, evidence in matrix_rows():
        if not status.startswith("✅"):
            continue
        checked += 1
        ledger_names, conclusion = lookup_ledger(ledger, feature)
        raw = [
            name
            for name in BACKTICK.findall(evidence)
            if "_" in name and len(name) > 12
        ] + list(ledger_names or [])
        names = sorted(set(raw))
        if conclusion and conclusion not in ("有证据",):
            soft.append((number, feature, conclusion))
        # Keep only the ones that really are test names.
        known = [name for name in names if name in index]
        unknown = [
            name for name in names if name not in index and name not in allowlist
        ]
        if unknown:
            missing_names.extend((number, feature, name) for name in unknown)
        if not known:
            no_evidence.append((number, feature))
            continue
        kinds = {classify(index[name]) for name in known}
        if kinds <= {"model"}:
            weak.append((number, feature, sorted(kinds)))

    print(f"checked {checked} ✅ row(s)")
    if no_evidence:
        print(f"\nno cited test ({len(no_evidence)}):")
        for number, feature in no_evidence:
            print(f"  #{number} {feature}")
    if weak:
        print(f"\nmodel-layer evidence only — the engine may ignore the field ({len(weak)}):")
        for number, feature, kinds in weak:
            print(f"  #{number} {feature} [{','.join(kinds)}]")
    if soft:
        print(f"\nledger marks these weaker than plain evidence ({len(soft)}):")
        for number, feature, conclusion in soft:
            print(f"  #{number} {feature}: {conclusion}")
    if missing_names:
        print(
            f"\ncited names that are neither tests nor allow-listed identifiers "
            f"({len(missing_names)}):"
        )
        for number, feature, name in missing_names:
            print(f"  #{number} {feature}: {name}")
    if not (no_evidence or weak or soft or missing_names):
        print("\nno weak or missing evidence found")
    if strict and (no_evidence or missing_names):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
