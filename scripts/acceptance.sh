#!/usr/bin/env bash
# Acceptance check for the golden cases in examples/golden/.
#
# Runs every model with the release binary, reads the JSON report and compares
# the key metrics with the values derived in docs/spec/golden-cases.md. Unlike
# the Rust tests this also exercises the packaged default behaviour end to end.
#
# Usage: bash scripts/acceptance.sh [--debug]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BIN="$ROOT/target/release/openflex"
if [[ "${1:-}" == "--debug" ]]; then
  BIN="$ROOT/target/debug/openflex"
fi
if [[ ! -x "$BIN" ]]; then
  echo "building $BIN ..."
  cargo build --release -p openflex-cli
fi

python3 - "$BIN" <<'PY'
import json
import subprocess
import sys

BIN = sys.argv[1]
CASES = {
    "examples/golden/mm1.ofm": {
        "Server utilization": ("objects.Proc1.utilization.mean", 0.80, 0.05),
        "Queue Lq": ("objects.Queue1.avg_content.mean", 3.2, 0.8),
        "Queue Wq": ("objects.Queue1.avg_stay_time.mean", 16.0, 3.0),
        "Service time": ("objects.Proc1.avg_stay_time.mean", 4.0, 0.4),
        "Proc content": ("objects.Proc1.avg_content.mean", 0.80, 0.05),
    },
    "examples/golden/det-line.ofm": {
        "Sink input": ("objects.Sink1.input_count.mean", 48.0, 0.0),
        "Proc output": ("objects.Proc1.output_count.mean", 48.0, 0.0),
        "Proc utilisation": ("objects.Proc1.utilization.mean", 0.8, 1e-9),
        "Queue content": ("objects.Queue1.avg_content.mean", 0.0, 1e-9),
        "Proc content": ("objects.Proc1.avg_content.mean", 0.8, 1e-9),
        "Proc stay": ("objects.Proc1.avg_stay_time.mean", 8.0, 1e-9),
    },
    "examples/golden/batching.ofm": {
        "Sink input": ("objects.Sink1.input_count.mean", 40.0, 0.0),
        "Queue content": ("objects.Queue1.avg_content.mean", 1.8182, 0.01),
        "Queue stay": ("objects.Queue1.avg_stay_time.mean", 20.0, 0.01),
        "Proc utilisation": ("objects.Proc1.utilization.mean", 0.36364, 0.002),
        "Proc content": ("objects.Proc1.avg_content.mean", 0.36364, 0.002),
    },
    "examples/golden/breakdown.ofm": {
        "Sink input": ("objects.Sink1.input_count.mean", 5455.0, 55.0),
        "Down fraction": ("objects.Proc1.down_fraction.mean", 0.0909, 0.002),
        "Proc utilisation": ("objects.Proc1.utilization.mean", 0.9091, 0.01),
    },
    "examples/golden/combiner-pack.ofm": {
        "Sink input": ("objects.Sink1.input_count.mean", 10.0, 1.0),
        "Combiner output": ("objects.Combiner1.output_count.mean", 10.0, 1.0),
    },
    "examples/golden/separator-unpack.ofm": {
        "Sink input": ("objects.Sink1.input_count.mean", 40.0, 0.0),
        "Separator output": ("objects.Sep1.output_count.mean", 40.0, 0.0),
    },
}

def lookup(report, path):
    value = report
    for part in path.split("."):
        if isinstance(value, list):
            value = value[int(part)]
        else:
            value = value[part]
    return value

failures = []
for model, checks in CASES.items():
    out = subprocess.run([BIN, "run", model, "--format", "json"], capture_output=True, text=True)
    if out.returncode != 0:
        failures.append(f"{model}: exit {out.returncode}: {out.stderr.strip()[:200]}")
        continue
    report = json.loads(out.stdout)
    for label, (path, expected, tolerance) in checks.items():
        actual = lookup(report, path)
        ok = abs(actual - expected) <= tolerance + 1e-12
        status = "PASS" if ok else "FAIL"
        print(f"{status} {model.split('/')[-1]:24} {label:20} expected {expected:>10.4f} got {actual:>10.4f}")
        if not ok:
            failures.append(f"{model}: {label} expected {expected} ± {tolerance}, got {actual}")

if failures:
    print(f"\n{len(failures)} check(s) failed")
    sys.exit(1)
print("\nall golden checks passed")
PY
