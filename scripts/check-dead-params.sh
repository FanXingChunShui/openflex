#!/usr/bin/env bash
# Ratchet against parameters that exist in the model but are never read by the
# engine. `max_travelers` was one of these: a field, a GUI control and a CLI
# description, with nobody reading the value — the model simply never changed.
#
# Known blind spot: a field whose *name* is a common word (`delay`, `size`,
# `count`) appears all over the engine for other structs, so a purely textual
# check cannot prove it is unused. `MixerStep.delay` hid behind exactly that
# until a human read the mixer branch. Treat a green run as "nothing obviously
# dead", not as proof.
#
# The allowlist freezes the fields we know about today. A new field that the
# engine never names fails this check, which is the moment to decide whether to
# wire it up or delete it.
#
# Usage: bash scripts/check-dead-params.sh [--update]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ALLOWLIST="docs/dead-params-allowlist.txt"

python3 - "$ALLOWLIST" "${1:-}" <<'PY'
import glob
import re
import sys

allow_path, mode = sys.argv[1], sys.argv[2]

model = open("crates/openflex-model/src/model.rs", encoding="utf-8").read()

# Check the top-level `*Params` structs **and** the structs they nest (MixerStep,
# PhotoEye, DecisionPoint, …): a nested field can be just as dead, and those
# structs were not covered at all before.
top_level = re.findall(r"pub struct (\w*Params) \{", model)
defined = set(re.findall(r"pub struct (\w+) \{", model))
nested = set()
for struct in re.finditer(r"pub struct \w*Params \{(.*?)\n\}", model, re.S):
    for field in re.finditer(r"pub \w+: (?:Option<)?(?:Vec<)?([A-Z]\w+)", struct.group(1)):
        nested.add(field.group(1))
nested = sorted(nested & defined - set(top_level))

checked_structs = top_level + nested
fields = []
for struct_name in checked_structs:
    struct = re.search(
        r"pub struct " + struct_name + r" \{(.*?)\n\}", model, re.S
    )
    if struct is None:
        continue
    for field in re.finditer(r"\n\s*pub (\w+):", struct.group(1)):
        fields.append((struct_name, field.group(1)))

core = "".join(open(p, encoding="utf-8").read() for p in glob.glob("crates/openflex-core/src/*.rs"))
named = set(re.findall(r"\b([a-z_][a-z0-9_]*)\b", core))


def used_through_accessor(field: str) -> bool:
    """A field can reach the engine through an accessor on the params struct.

    `SourceParams.arrival_style` is never named in the engine; it is read by
    `effective_arrival_style()`, which the engine does call. Treat that as used
    so the ratchet stays about genuinely unread fields.
    """
    for accessor in re.finditer(r"pub fn (\w+)\(&self\)[^{]*\{(.{0,600})", model, re.S):
        body = accessor.group(2)
        if re.search(r"\b" + field + r"\b", body) and re.search(
            r"\b" + accessor.group(1) + r"\b", core
        ):
            return True
    return False


dead = sorted(
    {
        f"{s}.{f}"
        for s, f in fields
        if f not in named and not used_through_accessor(f)
    }
)

if mode == "--update":
    with open(allow_path, "w", encoding="utf-8") as handle:
        handle.write(
            "# Fields declared in the model but never named anywhere in openflex-core.\n"
            "# Each line is technical debt: either wire the field up or delete it.\n"
        )
        for entry in dead:
            handle.write(entry + "\n")
    print(f"allowlist updated with {len(dead)} entries")
    raise SystemExit(0)

try:
    allowed = {
        line.strip()
        for line in open(allow_path, encoding="utf-8")
        if line.strip() and not line.startswith("#")
    }
except FileNotFoundError:
    allowed = set()

new = [entry for entry in dead if entry not in allowed]
stale = sorted(allowed - set(dead))

for entry in new:
    print(f"NEW dead parameter: {entry} — the model declares it and the engine never reads it")
for entry in stale:
    print(f"stale allowlist entry: {entry} — now read (or gone), prune {allow_path}")

if new:
    print("---")
    print(f"{len(new)} parameter(s) would be silently ignored; wire them up or delete them")
    raise SystemExit(1)
print(f"no new dead parameters ({len(dead)} known, {len(stale)} stale entries)")
PY
