#!/usr/bin/env bash
# Compare simulation results, in one of two modes:
#
#   csv      external result CSV (object,metric,value)  vs  report.json
#   reports  report.json (A)  vs  report.json (B)   — no external CSV needed
#
# Purpose: D1 in docs/BACKLOG.md — a numerical comparison against FlexSim (or
# any other simulator / analytic result) needs a reproducible, reviewable tool.
# This script does *not* run FlexSim and does not interpret it: it only compares
# numbers that were produced elsewhere (a real FlexSim run, a textbook, an
# analytic formula, or a second OpenFlex run).
#
# CSV format (header optional, values numeric):
#
#   object,metric,value
#   Proc1,utilization,0.80
#   Queue1,avg_content,3.2
#
# `object` is the object name inside the model (or "@model" / empty for the
# whole-run counters such as `completed`); `metric` is the report field, and a
# dotted path is accepted too (`input_count.mean`, `objects.Queue1.utilization`).
# A trailing "%" is accepted and divided by 100.
#
# Usage:
#   # CSV vs one report
#   scripts/compare-flexsim.sh --report reports/report.json --csv flexsim.csv
#   scripts/compare-flexsim.sh --report r.json --csv f.csv --tolerance 0.01
#
#   # two OpenFlex reports (object-level delta, no CSV)
#   scripts/compare-flexsim.sh --report-a before/report.json --report-b after/report.json
#   scripts/compare-flexsim.sh --report-a a.json --report-b b.json --fail-on-mismatch
#
# Options:
#   --report <FILE>        report.json (A; also the input for --csv mode).
#   --report-a <FILE>      report.json A (same as --report, clearer in reports mode).
#   --report-b <FILE>      report.json B; enables reports mode.
#   --csv <FILE>           External results CSV; enables csv mode.
#   --tolerance <T>        Relative tolerance, e.g. 0.05 = ±5% (default: 0.05).
#   --format <F>           markdown (default) | csv | json | table.
#   --fail-on-mismatch     Exit 1 when a value is outside the tolerance.
#   --allow-missing        Do not fail when a metric exists on only one side.
#   --label <TEXT>         Label of the external column (csv mode, default: "External").
#   --label-a <TEXT>       Label of column A (reports mode, default: "A").
#   --label-b <TEXT>       Label of column B (reports mode, default: "B").
#   -h, --help             Show this help.
#
# Exit code: 0 ok (differences are reported, not fatal), 1 a value is outside
# the tolerance and --fail-on-mismatch was given (or a required metric is missing),
# 2 usage error.
set -euo pipefail

readonly PROGRAM="compare-flexsim.sh"

REPORT=""
REPORT_A=""
REPORT_B=""
CSV=""
TOLERANCE="0.05"
FORMAT="markdown"
FAIL_ON_MISMATCH=0
ALLOW_MISSING=0
LABEL="External"
LABEL_A="A"
LABEL_B="B"

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
  sed -n '2,47p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --report) [ "$#" -ge 2 ] || usage_error "--report needs a value"; REPORT="$2"; shift 2 ;;
    --report=*) REPORT="${1#*=}"; shift ;;
    --report-a) [ "$#" -ge 2 ] || usage_error "--report-a needs a value"; REPORT_A="$2"; shift 2 ;;
    --report-a=*) REPORT_A="${1#*=}"; shift ;;
    --report-b) [ "$#" -ge 2 ] || usage_error "--report-b needs a value"; REPORT_B="$2"; shift 2 ;;
    --report-b=*) REPORT_B="${1#*=}"; shift ;;
    --csv) [ "$#" -ge 2 ] || usage_error "--csv needs a value"; CSV="$2"; shift 2 ;;
    --csv=*) CSV="${1#*=}"; shift ;;
    --tolerance) [ "$#" -ge 2 ] || usage_error "--tolerance needs a value"; TOLERANCE="$2"; shift 2 ;;
    --tolerance=*) TOLERANCE="${1#*=}"; shift ;;
    --format) [ "$#" -ge 2 ] || usage_error "--format needs a value"; FORMAT="$2"; shift 2 ;;
    --format=*) FORMAT="${1#*=}"; shift ;;
    --fail-on-mismatch) FAIL_ON_MISMATCH=1; shift ;;
    --allow-missing) ALLOW_MISSING=1; shift ;;
    --label) [ "$#" -ge 2 ] || usage_error "--label needs a value"; LABEL="$2"; shift 2 ;;
    --label=*) LABEL="${1#*=}"; shift ;;
    --label-a) [ "$#" -ge 2 ] || usage_error "--label-a needs a value"; LABEL_A="$2"; shift 2 ;;
    --label-a=*) LABEL_A="${1#*=}"; shift ;;
    --label-b) [ "$#" -ge 2 ] || usage_error "--label-b needs a value"; LABEL_B="$2"; shift 2 ;;
    --label-b=*) LABEL_B="${1#*=}"; shift ;;
    -h | --help) usage; exit 0 ;;
    *) usage_error "unknown argument '$1'" ;;
  esac
done

case "$FORMAT" in
  markdown | csv | json | table) : ;;
  *) usage_error "--format must be markdown, csv, json or table (got '$FORMAT')" ;;
esac
case "$TOLERANCE" in
  '' | *[!0-9.]*) usage_error "--tolerance must be a number like 0.05" ;;
esac
if ! command -v python3 >/dev/null 2>&1; then
  die "python3 is required to compare the values"
fi
if [ -n "$REPORT" ] && [ -n "$REPORT_A" ] && [ "$REPORT" != "$REPORT_A" ]; then
  usage_error "--report and --report-a point at different files; pass only one"
fi

# --- mode selection ---------------------------------------------------------
MODE=""
REPORT_A_PATH=""
REPORT_B_PATH=""
CSV_PATH=""

if [ -n "$CSV" ]; then
  MODE="csv"
  REPORT_A_PATH="${REPORT_A:-${REPORT:-report.json}}"
  CSV_PATH="$CSV"
elif [ -n "$REPORT_B" ]; then
  MODE="reports"
  REPORT_A_PATH="${REPORT_A:-${REPORT:-report.json}}"
  REPORT_B_PATH="$REPORT_B"
else
  usage_error "pass --csv <FILE> (external results) or --report-b <FILE> (compare two reports)"
fi

[ -f "$REPORT_A_PATH" ] || die "no such report: $REPORT_A_PATH (run: openflex run model.ofm --report-dir <dir>)"
if [ "$MODE" = "reports" ]; then
  [ -f "$REPORT_B_PATH" ] || die "no such report: $REPORT_B_PATH"
else
  [ -f "$CSV_PATH" ] || die "no such CSV: $CSV_PATH"
fi

info "mode:     $MODE"
info "report A: $REPORT_A_PATH"
if [ "$MODE" = "reports" ]; then
  info "report B: $REPORT_B_PATH"
else
  info "csv:      $CSV_PATH"
fi

python3 - "$MODE" "$REPORT_A_PATH" "$REPORT_B_PATH" "$CSV_PATH" "$TOLERANCE" "$FORMAT" \
  "$FAIL_ON_MISMATCH" "$ALLOW_MISSING" "$LABEL" "$LABEL_A" "$LABEL_B" <<'PY'
import csv
import json
import math
import sys

(
    mode,
    report_a_path,
    report_b_path,
    csv_path,
    tolerance,
    fmt,
    fail_on_mismatch,
    allow_missing,
    label,
    label_a,
    label_b,
) = sys.argv[1:12]
tolerance = float(tolerance)
fail_on_mismatch = fail_on_mismatch == "1"
allow_missing = allow_missing == "1"

with open(report_a_path, encoding="utf-8") as handle:
    report_a = json.load(handle)


def report_name(path):
    return path.rsplit("/", 1)[-1]


def lookup(report, path):
    node = report
    for part in path.split("."):
        if not isinstance(node, dict) or part not in node:
            return None
        node = node[part]
    return node


def numeric(node):
    if isinstance(node, bool):
        return None
    if isinstance(node, (int, float)):
        return float(node)
    if isinstance(node, dict):
        for key in ("mean", "value", "average"):
            candidate = node.get(key)
            if isinstance(candidate, (int, float)) and not isinstance(candidate, bool):
                return float(candidate)
    return None


def ci95(node):
    if isinstance(node, dict):
        value = node.get("ci95")
        if isinstance(value, (int, float)) and not isinstance(value, bool):
            return float(value)
    return None


def compare(expected, actual):
    """Return (abs_diff, rel_diff, within) with `expected` as the reference."""
    abs_diff = actual - expected
    if expected == 0:
        rel_diff = 0.0 if abs_diff == 0 else math.inf
        within = abs(abs_diff) <= tolerance
    else:
        rel_diff = abs_diff / abs(expected)
        within = abs(rel_diff) <= tolerance
    return abs_diff, rel_diff, within


rows = []
missing = []

if mode == "csv":
    # ---------------------------------------------------------------- csv mode
    def resolve(object_name, metric):
        candidates = []
        if object_name and object_name not in ("@model", "model", "-"):
            candidates.append("objects.%s.%s" % (object_name, metric))
            candidates.append("%s.%s" % (object_name, metric))
        candidates.append(metric)
        for path in candidates:
            node = lookup(report_a, path)
            value = numeric(node)
            if value is not None:
                return value, ci95(node), path
        return None, None, None

    def parse_value(raw):
        text = raw.strip()
        if text.endswith("%"):
            return float(text[:-1].strip()) / 100.0
        return float(text)

    with open(csv_path, newline="", encoding="utf-8-sig") as handle:
        reader = csv.reader(handle)
        all_rows = [row for row in reader if any(cell.strip() for cell in row)]
    if not all_rows:
        print("compare-flexsim: error: the CSV is empty", file=sys.stderr)
        sys.exit(2)

    header = [cell.strip().lower() for cell in all_rows[0]]
    if "object" in header and "metric" in header and "value" in header:
        body = all_rows[1:]
        index = {name: header.index(name) for name in ("object", "metric", "value")}
    else:
        body = all_rows
        index = {"object": 0, "metric": 1, "value": 2}

    parsed = []
    for line_number, row in enumerate(body, start=2):
        if len(row) < 3:
            print(
                "compare-flexsim: error: line %d has %d columns, expected 3 (object,metric,value)"
                % (line_number, len(row)),
                file=sys.stderr,
            )
            sys.exit(2)
        object_name = row[index["object"]].strip()
        metric = row[index["metric"]].strip()
        try:
            expected = parse_value(row[index["value"]])
        except ValueError:
            print(
                "compare-flexsim: error: line %d has a non-numeric value %r"
                % (line_number, row[index["value"]]),
                file=sys.stderr,
            )
            sys.exit(2)
        parsed.append((object_name, metric, expected))

    if not parsed:
        print("compare-flexsim: error: the CSV has no data rows", file=sys.stderr)
        sys.exit(2)

    for object_name, metric, expected in parsed:
        actual, spread, path = resolve(object_name, metric)
        if actual is None:
            missing.append((object_name or "@model", metric))
            continue
        abs_diff, rel_diff, within = compare(expected, actual)
        rows.append(
            {
                "object": object_name or "@model",
                "metric": metric,
                "a": expected,
                "b": actual,
                "abs_diff": abs_diff,
                "rel_diff": rel_diff,
                "ci95_a": None,
                "ci95_b": spread,
                "within": within,
                "path": path,
            }
        )
    title_a, title_b = label, "OpenFlex"
else:
    # ------------------------------------------------------------ reports mode
    def collect(report):
        collected = {}
        for name, stats in (report.get("objects") or {}).items():
            if not isinstance(stats, dict):
                continue
            for metric, node in stats.items():
                value = numeric(node)
                if value is None:
                    continue
                collected["objects.%s.%s" % (name, metric)] = (name, metric, value, ci95(node))
        for metric in ("completed", "created"):
            value = numeric(report.get(metric))
            if value is not None:
                collected[metric] = ("@model", metric, value, None)
        return collected

    with open(report_b_path, encoding="utf-8") as handle:
        report_b = json.load(handle)
    values_a = collect(report_a)
    values_b = collect(report_b)

    def sort_key(path):
        obj, metric, _, _ = values_a.get(path) or values_b.get(path)
        return (obj, metric)

    for path in sorted(set(values_a) | set(values_b), key=sort_key):
        side_a = values_a.get(path)
        side_b = values_b.get(path)
        if side_a is None or side_b is None:
            present = side_a if side_a is not None else side_b
            missing.append((present[0], present[1]))
            continue
        abs_diff, rel_diff, within = compare(side_a[2], side_b[2])
        rows.append(
            {
                "object": side_a[0],
                "metric": side_a[1],
                "a": side_a[2],
                "b": side_b[2],
                "abs_diff": abs_diff,
                "rel_diff": rel_diff,
                "ci95_a": side_a[3],
                "ci95_b": side_b[3],
                "within": within,
                "path": path,
            }
        )
    title_a = "%s (%s)" % (label_a, report_name(report_a_path))
    title_b = "%s (%s)" % (label_b, report_name(report_b_path))

mismatches = [row for row in rows if not row["within"]]

if fmt == "json":
    print(
        json.dumps(
            {
                "mode": mode,
                "model": report_a.get("model"),
                "stop_time": report_a.get("stop_time"),
                "replications": len(report_a.get("replications") or []),
                "tolerance": tolerance,
                "report_a": report_a_path,
                "report_b": report_b_path or None,
                "rows": [
                    {**row, "rel_diff": None if math.isinf(row["rel_diff"]) else row["rel_diff"]}
                    for row in rows
                ],
                "missing": [{"object": obj, "metric": metric} for obj, metric in missing],
                "mismatches": len(mismatches),
                "within_tolerance": len(rows) - len(mismatches),
            },
            indent=2,
        )
    )
elif fmt == "csv":
    writer = csv.writer(sys.stdout)
    writer.writerow(
        ["object", "metric", title_a, title_b, "abs_diff", "rel_diff", "within_tolerance"]
    )
    for row in rows:
        writer.writerow(
            [
                row["object"],
                row["metric"],
                "%g" % row["a"],
                "%g" % row["b"],
                "%+.6g" % row["abs_diff"],
                "n/a" if math.isinf(row["rel_diff"]) else "%+.2f%%" % (row["rel_diff"] * 100),
                "yes" if row["within"] else "no",
            ]
        )
    for object_name, metric in missing:
        writer.writerow([object_name, metric, "", "MISSING", "", "", "no"])
elif fmt == "table":
    print(
        "%-16s %-22s %12s %12s %12s %10s %s"
        % ("object", "metric", title_a, title_b, "abs_diff", "rel_diff", "within")
    )
    print("-" * 100)
    for row in rows:
        rel = "n/a" if math.isinf(row["rel_diff"]) else "%+.2f%%" % (row["rel_diff"] * 100)
        print(
            "%-16s %-22s %12.6g %12.6g %+12.4g %10s %s"
            % (
                row["object"],
                row["metric"],
                row["a"],
                row["b"],
                row["abs_diff"],
                rel,
                "yes" if row["within"] else "NO",
            )
        )
else:
    print(
        "| object | metric | %s | %s | abs diff | rel diff | within ±%g%% |"
        % (title_a, title_b, tolerance * 100)
    )
    print("|---|---|---|---|---|---|---|")
    for row in rows:
        rel = "n/a" if math.isinf(row["rel_diff"]) else "%+.2f%%" % (row["rel_diff"] * 100)
        mark = "yes" if row["within"] else "**NO**"
        print(
            "| %s | %s | %.6g | %.6g | %+.6g | %s | %s |"
            % (row["object"], row["metric"], row["a"], row["b"], row["abs_diff"], rel, mark)
        )

print()
print(
    "compare-flexsim: %d value(s) compared, %d within ±%g%%, %d outside, %d missing"
    % (len(rows), len(rows) - len(mismatches), tolerance * 100, len(mismatches), len(missing)),
    file=sys.stderr,
)
for object_name, metric in missing:
    print(
        "compare-flexsim: missing metric on one side: %s / %s" % (object_name, metric),
        file=sys.stderr,
    )

status = 0
if fail_on_mismatch and mismatches:
    status = 1
if missing and not allow_missing:
    status = 1
sys.exit(status)
PY
