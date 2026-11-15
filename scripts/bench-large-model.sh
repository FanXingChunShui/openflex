#!/usr/bin/env bash
# Large-model performance benchmark for OpenFlex.
#
# Generates a synthetic .ofm with N objects (default 10,000), then measures the
# release binary on two phases users actually pay for:
#
#   validate  parse + semantic validation
#   run       parse + simulate + format the report (CSV)
#
# For each phase it records wall-clock time and the child's peak RSS (from
# os.wait4 rusage: bytes on macOS, KiB converted to bytes on Linux). A third,
# untimed pass with `--trace` counts the exact number of events so the report
# can show events/second alongside items/second.
#
# Model shape: groups of Source -> Queue -> Processor -> Sink. Each source emits
# `--items` items at a constant inter-arrival; processors have a near-zero
# constant service time, so the benchmark measures engine bookkeeping rather
# than queueing dynamics. Total objects = groups * 4.
#
# Usage:
#   scripts/bench-large-model.sh                        # 10k objects (quick, ~2 s)
#   scripts/bench-large-model.sh --objects 100000       # full 100k baseline (~10 min)
#   scripts/bench-large-model.sh --objects 10000        # quick smoke run
#   scripts/bench-large-model.sh --items 5 --stop-time 200
#   scripts/bench-large-model.sh --json bench.json --markdown row.md
#   scripts/bench-large-model.sh --dry-run
#   scripts/bench-large-model.sh --objects 100000 --generate-only /tmp/bench   # model only
#   scripts/bench-large-model.sh --help
#
# Options:
#   --objects <N>       Total objects (rounded down to a multiple of 4; default 10000).
#                       The 100k baseline takes ~10 minutes on the v0.11.0 engine.
#   --items <N>         Items per source (default 1).
#   --stop-time <T>     Simulation stop time (default 100).
#   --replications <N>  Replications (default 1; >1 multiplies runtime by N).
#   --seed <N>          Base seed (default 1).
#   --binary <PATH>     openflex binary (default: target/release/openflex).
#   --json <FILE>       Write the raw measurements as JSON.
#   --markdown <FILE>   Append a markdown table row (for docs/verification/performance.md).
#   --skip-trace        Skip the untimed event-counting pass (faster, no events/s).
#   --keep              Keep the generated model and outputs in the work directory.
#   --generate-only <DIR>  Write the model to DIR/large-model.ofm and exit (no measurement).
#                       Useful for profilers: `sample $(pgrep openflex) 30`.
#   --dry-run           Print the plan; generate nothing.
#   -h, --help          Show this help.
#
# Exit code: 0 when both phases succeed, 1 on a failed phase, 2 usage error.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly ROOT_DIR
readonly PROGRAM="bench-large-model.sh"

OBJECTS=10000
ITEMS=1
STOP_TIME=100
REPLICATIONS=1
SEED=1
BINARY=""
OUT_JSON=""
OUT_MARKDOWN=""
KEEP=0
DRY_RUN=0
SKIP_TRACE=0
GENERATE_ONLY=""

info() { printf '%s: %s\n' "$PROGRAM" "$*"; }
die() {
  printf '%s: error: %s\n' "$PROGRAM" "$*" >&2
  exit 1
}
usage_error() {
  printf '%s: error: %s\n' "$PROGRAM" "$*" >&2
  printf 'Try %s --help\n' "$PROGRAM" >&2
  exit 2
}

usage() {
  sed -n '2,45p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --objects) [ "$#" -ge 2 ] || usage_error "--objects needs a value"; OBJECTS="$2"; shift 2 ;;
    --objects=*) OBJECTS="${1#*=}"; shift ;;
    --items) [ "$#" -ge 2 ] || usage_error "--items needs a value"; ITEMS="$2"; shift 2 ;;
    --items=*) ITEMS="${1#*=}"; shift ;;
    --stop-time) [ "$#" -ge 2 ] || usage_error "--stop-time needs a value"; STOP_TIME="$2"; shift 2 ;;
    --stop-time=*) STOP_TIME="${1#*=}"; shift ;;
    --replications) [ "$#" -ge 2 ] || usage_error "--replications needs a value"; REPLICATIONS="$2"; shift 2 ;;
    --replications=*) REPLICATIONS="${1#*=}"; shift ;;
    --seed) [ "$#" -ge 2 ] || usage_error "--seed needs a value"; SEED="$2"; shift 2 ;;
    --seed=*) SEED="${1#*=}"; shift ;;
    --binary) [ "$#" -ge 2 ] || usage_error "--binary needs a value"; BINARY="$2"; shift 2 ;;
    --binary=*) BINARY="${1#*=}"; shift ;;
    --json) [ "$#" -ge 2 ] || usage_error "--json needs a value"; OUT_JSON="$2"; shift 2 ;;
    --json=*) OUT_JSON="${1#*=}"; shift ;;
    --markdown) [ "$#" -ge 2 ] || usage_error "--markdown needs a value"; OUT_MARKDOWN="$2"; shift 2 ;;
    --markdown=*) OUT_MARKDOWN="${1#*=}"; shift ;;
    --generate-only) [ "$#" -ge 2 ] || usage_error "--generate-only needs a directory"; GENERATE_ONLY="$2"; shift 2 ;;
    --generate-only=*) GENERATE_ONLY="${1#*=}"; shift ;;
    --skip-trace) SKIP_TRACE=1; shift ;;
    --keep) KEEP=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) usage_error "unknown argument '$1'" ;;
  esac
done

case "$OBJECTS" in
  '' | *[!0-9]*) usage_error "--objects must be a positive integer" ;;
esac
case "$ITEMS" in
  '' | *[!0-9]*) usage_error "--items must be a positive integer" ;;
esac
case "$STOP_TIME" in
  '' | *[!0-9.]*) usage_error "--stop-time must be a number" ;;
esac
case "$REPLICATIONS" in
  '' | *[!0-9]*) usage_error "--replications must be a positive integer" ;;
esac
[ "$OBJECTS" -ge 4 ] || usage_error "--objects must be at least 4"
[ "$ITEMS" -ge 1 ] || usage_error "--items must be at least 1"
[ "$REPLICATIONS" -ge 1 ] || usage_error "--replications must be at least 1"

if [ -z "$BINARY" ]; then
  BINARY="$ROOT_DIR/target/release/openflex"
fi
if ! command -v python3 >/dev/null 2>&1; then
  die "python3 is required (model generation + rusage measurement)"
fi
if [ ! -x "$BINARY" ]; then
  die "no executable at $BINARY — build it first: cargo build --release -p openflex-cli"
fi

snapshot_dir() {
  mktemp -d "${TMPDIR:-/tmp}/openflex-bench.XXXXXX"
}

WORK="$(snapshot_dir)"
cleanup() {
  if [ "$KEEP" -eq 1 ]; then
    printf '\nwork directory kept: %s\n' "$WORK"
  else
    rm -rf "$WORK"
  fi
}
trap cleanup EXIT INT TERM

groups=$((OBJECTS / 4))
objects=$((groups * 4))
if [ "$objects" -ne "$OBJECTS" ]; then
  info "rounding --objects $OBJECTS down to $objects (multiple of 4: Source/Queue/Processor/Sink)"
fi
[ "$groups" -ge 1 ] || die "--objects too small: need at least 4"

if [ -n "$GENERATE_ONLY" ]; then
  mkdir -p "$GENERATE_ONLY"
  GENERATE_ONLY="$(cd "$GENERATE_ONLY" && pwd)"
  info "generating a model only: $GENERATE_ONLY/large-model.ofm"
  python3 - "$GENERATE_ONLY" "$groups" "$ITEMS" "$STOP_TIME" "$REPLICATIONS" "$SEED" <<'PY'
import json
import os
import sys

out_dir, groups, items, stop_time, replications, seed = sys.argv[1:7]
groups = int(groups)
items = int(items)
stop_time = float(stop_time)
replications = int(replications)
seed = int(seed)

interarrival = stop_time / (items + 1)
objects = []
connections = []
for index in range(groups):
    source, queue, processor, sink = ("S%d" % index, "Q%d" % index, "P%d" % index, "K%d" % index)
    objects.append({"name": source, "kind": "source", "pos": [0, index], "params": {
        "interarrival": {"distribution": "constant", "value": interarrival},
        "items_per_arrival": 1, "first_arrival": interarrival}})
    objects.append({"name": queue, "kind": "queue", "pos": [1, index], "params": {"max_content": 1000, "order": "fifo"}})
    objects.append({"name": processor, "kind": "processor", "pos": [2, index], "params": {
        "setup_time": {"distribution": "constant", "value": 0.0},
        "process_time": {"distribution": "constant", "value": 0.0001},
        "max_content": 1}})
    objects.append({"name": sink, "kind": "sink", "pos": [3, index], "params": {}})
    connections.append({"from": source, "to": queue})
    connections.append({"from": queue, "to": processor})
    connections.append({"from": processor, "to": sink})

model = {
    "format": "openflex-model",
    "version": 1,
    "name": "bench-large",
    "description": "synthetic benchmark: %d Source/Queue/Processor/Sink chains" % groups,
    "time_unit": "seconds",
    "run": {"stop_time": stop_time, "warmup": 0.0, "seed": seed, "replications": replications},
    "objects": objects,
    "connections": connections,
}
path = os.path.join(out_dir, "large-model.ofm")
with open(path, "w", encoding="utf-8") as handle:
    json.dump(model, handle, separators=(",", ":"))
print("bench-large-model: wrote %s (%d objects, %.1f MB)" % (path, len(objects), os.path.getsize(path) / (1024 * 1024)))
PY
  printf '\nRESULT: PASS (model generated)\n'
  exit 0
fi

info "binary:     $BINARY ($("$BINARY" --version 2>&1 || echo unknown))"
info "objects:    $objects ($groups chains x 4)"
info "items/source: $ITEMS, stop time: $STOP_TIME, replications: $REPLICATIONS"
info "workdir:    $WORK"

if [ "$DRY_RUN" -eq 1 ]; then
  printf '\n[dry-run] would generate %s objects (%s chains) into %s\n' "$objects" "$groups" "$WORK"
  printf '[dry-run] would measure: %s validate <model>; %s run <model> --format csv\n' "$BINARY" "$BINARY"
  if [ "$SKIP_TRACE" -eq 0 ]; then
    printf '[dry-run] would count events with: %s run <model> --trace <file> --format csv\n' "$BINARY"
  fi
  printf '\nRESULT: PASS (dry-run)\n'
  exit 0
fi

python3 - "$BINARY" "$groups" "$ITEMS" "$STOP_TIME" "$REPLICATIONS" "$SEED" \
  "$WORK" "$OUT_JSON" "$OUT_MARKDOWN" "$SKIP_TRACE" <<'PY'
import csv
import json
import os
import platform
import subprocess
import sys
import time

(
    binary,
    groups,
    items,
    stop_time,
    replications,
    seed,
    work,
    out_json,
    out_markdown,
    skip_trace,
) = sys.argv[1:11]
groups = int(groups)
items = int(items)
stop_time = float(stop_time)
replications = int(replications)
seed = int(seed)
skip_trace = skip_trace == "1"

model_path = os.path.join(work, "large-model.ofm")
run_csv = os.path.join(work, "run.csv")
trace_path = os.path.join(work, "trace.jsonl")

# ---------------------------------------------------------------- generate
interarrival = stop_time / (items + 1)
objects = []
connections = []
for index in range(groups):
    source, queue, processor, sink = ("S%d" % index, "Q%d" % index, "P%d" % index, "K%d" % index)
    objects.append(
        {
            "name": source,
            "kind": "source",
            "pos": [0, index],
            "params": {
                "interarrival": {"distribution": "constant", "value": interarrival},
                "items_per_arrival": 1,
                "first_arrival": interarrival,
            },
        }
    )
    objects.append(
        {"name": queue, "kind": "queue", "pos": [1, index], "params": {"max_content": 1000, "order": "fifo"}}
    )
    objects.append(
        {
            "name": processor,
            "kind": "processor",
            "pos": [2, index],
            "params": {
                "setup_time": {"distribution": "constant", "value": 0.0},
                "process_time": {"distribution": "constant", "value": 0.0001},
                "max_content": 1,
            },
        }
    )
    objects.append({"name": sink, "kind": "sink", "pos": [3, index], "params": {}})
    connections.append({"from": source, "to": queue})
    connections.append({"from": queue, "to": processor})
    connections.append({"from": processor, "to": sink})

model = {
    "format": "openflex-model",
    "version": 1,
    "name": "bench-large",
    "description": "synthetic benchmark: %d Source/Queue/Processor/Sink chains" % groups,
    "time_unit": "seconds",
    "run": {
        "stop_time": stop_time,
        "warmup": 0.0,
        "seed": seed,
        "replications": replications,
    },
    "objects": objects,
    "connections": connections,
}
with open(model_path, "w", encoding="utf-8") as handle:
    json.dump(model, handle, separators=(",", ":"))
model_bytes = os.path.getsize(model_path)


def measure(command, stdout_path):
    """Run `command`, return (seconds, peak_rss_bytes, exit_code, stderr)."""
    start = time.perf_counter()
    with open(stdout_path, "wb") as stdout:
        process = subprocess.Popen(command, stdout=stdout, stderr=subprocess.PIPE)
        pid, status, usage = os.wait4(process.pid, 0)
        process.returncode = os.waitstatus_to_exitcode(status)
        stderr = process.stderr.read().decode("utf-8", "replace") if process.stderr else ""
    elapsed = time.perf_counter() - start
    # ru_maxrss is bytes on macOS and KiB on Linux.
    rss = usage.ru_maxrss if sys.platform == "darwin" else usage.ru_maxrss * 1024
    return elapsed, rss, process.returncode, stderr


def fail(message):
    print("bench-large-model: error: " + message, file=sys.stderr)
    sys.exit(1)


# ---------------------------------------------------------------- validate
validate_seconds, validate_rss, validate_code, validate_err = measure(
    [binary, "validate", model_path], os.devnull
)
if validate_code != 0:
    fail("validate failed (exit %d): %s" % (validate_code, validate_err.strip()[:400]))

# ---------------------------------------------------------------- run
run_seconds, run_rss, run_code, run_err = measure(
    [binary, "run", model_path, "--format", "csv"], run_csv
)
if run_code != 0:
    fail("run failed (exit %d): %s" % (run_code, run_err.strip()[:400]))

# Summarise the CSV report: object rows and completed items at the sinks.
# The report columns are `object,kind,input,output,avg_wip,...`; older releases
# used `input_count`/`output_count`, so accept both names.
object_rows = 0
completed = 0.0
with open(run_csv, newline="", encoding="utf-8") as handle:
    reader = csv.DictReader(handle)
    fields = {name.strip().lower(): name for name in (reader.fieldnames or [])}

    def column(*names):
        for name in names:
            if name in fields:
                return fields[name]
        return None

    kind_column = column("kind")
    input_column = column("input", "input_count")
    for row in reader:
        object_rows += 1
        kind = (row.get(kind_column) or "").strip().lower() if kind_column else ""
        if kind == "sink" and input_column:
            raw = row.get(input_column) or "0"
            try:
                completed += float(raw)
            except ValueError:
                pass
report_bytes = os.path.getsize(run_csv)

# ---------------------------------------------------------------- events
events = None
if not skip_trace:
    trace_seconds, trace_rss, trace_code, trace_err = measure(
        [binary, "run", model_path, "--trace", trace_path, "--format", "csv"], os.devnull
    )
    if trace_code == 0:
        with open(trace_path, encoding="utf-8") as handle:
            events = sum(1 for _ in handle)
    else:
        print(
            "bench-large-model: warning: event-counting trace failed (exit %d); "
            "events/s will be shown as n/a" % trace_code,
            file=sys.stderr,
        )

events_per_second = events / run_seconds if events and run_seconds > 0 else None
items_per_second = completed / run_seconds if run_seconds > 0 else None

result = {
    "host": {
        "system": platform.system(),
        "release": platform.release(),
        "machine": platform.machine(),
        "python": platform.python_version(),
    },
    "binary": binary,
    "binary_version": subprocess.run([binary, "--version"], capture_output=True, text=True).stdout.strip(),
    "model": {
        "path": model_path,
        "bytes": model_bytes,
        "objects": len(objects),
        "groups": groups,
        "items_per_source": items,
        "stop_time": stop_time,
        "replications": replications,
        "seed": seed,
    },
    "validate": {"seconds": validate_seconds, "peak_rss_bytes": validate_rss},
    "run": {
        "seconds": run_seconds,
        "peak_rss_bytes": run_rss,
        "report_bytes": report_bytes,
        "report_rows": object_rows,
        "completed_items": completed,
    },
    "events": events,
    "events_per_second": events_per_second,
    "items_per_second": items_per_second,
}
if out_json:
    with open(out_json, "w", encoding="utf-8") as handle:
        json.dump(result, handle, indent=2)
        handle.write("\n")

print()
print("== Large-model benchmark ==")
print("  binary:       %s" % result["binary_version"])
print(
    "  model:        %d objects (%d chains x 4), %.1f MB on disk"
    % (len(objects), groups, model_bytes / (1024 * 1024))
)
print("  settings:     %g items/source, stop_time=%g, replications=%d, seed=%d"
      % (items, stop_time, replications, seed))
print()
print("  %-10s %10s %12s" % ("phase", "wall", "peak RSS"))
print("  %-10s %9.2fs %9.1f MB" % ("validate", validate_seconds, validate_rss / (1024 * 1024)))
print("  %-10s %9.2fs %9.1f MB" % ("run", run_seconds, run_rss / (1024 * 1024)))
print()
if events is not None:
    print("  events:       %d total, %.0f events/s" % (events, events_per_second or 0))
else:
    print("  events:       n/a (--skip-trace)")
print("  items:        %.0f completed, %.0f items/s" % (completed, items_per_second or 0))
print("  report:       %.1f MB CSV, %d object rows" % (report_bytes / (1024 * 1024), object_rows))

if out_markdown:
    host = "%s %s (%s)" % (platform.system(), platform.release(), platform.machine())
    row = "| %s | %s | %d (%d chains) | %.1f MB | %.2f | %.0f | %.2f | %.0f | %s | %s | %s |" % (
        time.strftime("%Y-%m-%d", time.localtime()),
        host,
        len(objects),
        groups,
        model_bytes / (1024 * 1024),
        validate_seconds,
        validate_rss / (1024 * 1024),
        run_seconds,
        run_rss / (1024 * 1024),
        "%d" % events if events is not None else "n/a",
        "%.0f" % events_per_second if events_per_second else "n/a",
        "%.0f" % (items_per_second or 0),
    )
    with open(out_markdown, "a", encoding="utf-8") as handle:
        handle.write(row + "\n")
    print("\n  appended a markdown row to %s" % out_markdown)

if not skip_trace and os.path.exists(trace_path):
    os.remove(trace_path)
PY

printf '\nRESULT: PASS\n'
