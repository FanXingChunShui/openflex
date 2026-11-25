#!/usr/bin/env bash
# Every shipped example must validate, run, and be deterministic.
#
# The examples are documentation that happens to be executable: if one starts
# failing after an engine change, that is a user-visible regression that no unit
# test necessarily covers. Each model is run twice with the same seed and the two
# reports must be byte-identical (the engine promises determinism).
#
# Usage: bash scripts/check-examples.sh [--release]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

profile="debug"
if [ "${1:-}" = "--release" ]; then profile="release"; fi
BIN="$ROOT/target/$profile/openflex"
if [ ! -x "$BIN" ]; then
    echo "build the CLI first: $BIN is missing" >&2
    echo "  cargo build -p openflex-cli${1:+ --release}" >&2
    exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

total=0
failures=0
for model in examples/*/*.ofm; do
    total=$((total + 1))
    name="${model#examples/}"
    if ! "$BIN" validate "$model" >"$work/validate.log" 2>&1; then
        echo "  FAIL  $name: does not validate"
        sed -n '2,4p' "$work/validate.log" | sed 's/^/        /'
        failures=$((failures + 1))
        continue
    fi
    if ! "$BIN" run "$model" --format json >"$work/a.json" 2>"$work/run.log"; then
        echo "  FAIL  $name: does not run"
        sed -n '1,3p' "$work/run.log" | sed 's/^/        /'
        failures=$((failures + 1))
        continue
    fi
    if ! "$BIN" run "$model" --format json >"$work/b.json" 2>/dev/null; then
        echo "  FAIL  $name: second run failed"
        failures=$((failures + 1))
        continue
    fi
    first="$(shasum -a 256 "$work/a.json" | awk '{print $1}')"
    second="$(shasum -a 256 "$work/b.json" | awk '{print $1}')"
    if [ "$first" != "$second" ]; then
        echo "  FAIL  $name: two runs with the same seed differ"
        failures=$((failures + 1))
    fi
done

if [ "$failures" -ne 0 ]; then
    echo "---"
    echo "$failures of $total example(s) failed" >&2
    exit 1
fi
echo "all $total example models validate, run, and are deterministic"
