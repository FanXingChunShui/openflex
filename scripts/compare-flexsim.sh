#!/usr/bin/env bash
# Compare an external simulation result (CSV) with an OpenFlex report.json.
#
# Purpose: D1 in docs/BACKLOG.md — a numerical comparison against FlexSim (or
# any other simulator / analytic result) needs a reproducible, reviewable tool.
# This script does *not* run FlexSim and does not interpret it: it takes a CSV
# whose values were produced elsewhere (a real FlexSim run, a textbook, an
# analytic formula) and prints the differences per object/metric.
#
# CSV format (header optional, values numeric):
#
#   object,metric,value
#   Proc1,utilization,0.80
#   Queue1,avg_content,3.2
#   Queue1,avg_stay_time,16.0
#
# `object` is the object name inside the model (or "@model" / empty for the
# whole-run counters such as `completed`); `metric` is the report field, and a
# dotted path is accepted too (`input_count.mean`, `objects.Queue1.utilization`).
# A trailing "%" is accepted and divided by 100.
#
# Usage:
#   scripts/compare-flexsim.sh --report reports/report.json --csv flexsim.csv
#   scripts/compare-flexsim.sh --report r.json --csv f.csv --tolerance 0.05
#   scripts/compare-flexsim.sh --report r.json --csv f.csv --format csv
#   scripts/compare-flexsim.sh --report r.json --csv f.csv --fail-on-mismatch
#   scripts/compare-flexsim.sh --help
#
# Options:
#   --report <FILE>        OpenFlex report.json (default: report.json).
#   --csv <FILE>           External results CSV (required).
#   --tolerance <T>        Relative tolerance, e.g. 0.05 = ±5% (default: 0.05).
#   --format <F>           markdown (default) | csv | json | table.
#   --fail-on-mismatch     Exit 1 when a value is outside the tolerance.
#   --allow-missing        Do not fail when a CSV row has no OpenFlex metric.
#   --label <TEXT>         Label for the external column (default: "External").
#   -h, --help             Show this help.
#
# Exit code: 0 ok (differences are reported, not fatal), 1 a value is outside
# the tolerance and --fail-on-mismatch was given (or a required metric is
# missing), 2 usage error.
set -euo pipefail

readonly PROGRAM="compare-flexsim.sh"

REPORT="report.json"
CSV=""
TOLERANCE="0.05"
FORMAT="markdown"
FAIL_ON_MISMATCH=0
ALLOW_MISSING=0
LABEL="External"

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
  sed -n '2,50p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --report) [ "$#" -ge 2 ] || usage_error "--report needs a value"; REPORT="$2"; shift 2 ;;
    --report=*) REPORT="${1#*=}"; shift ;;
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
    -h | --help) usage; exit 0 ;;
    *) usage_error "unknown argument '$1'" ;;
  esac
done

[ -n "$CSV" ] || usage_error "--csv <FILE> is required"
[ -f "$CSV" ] || die "no such CSV: $CSV"
[ -f "$REPORT" ] || die "no such report: $REPORT (run: openflex run model.ofm --report-dir <dir>)"
case "$FORMAT" in
  markdown | csv | json | table) : ;;
  *) usage_error "--format must be markdown, csv, json or table (got '$FORMAT')" ;;
esac
if ! command -v python3 >/dev/null 2>&1; then
  die "python3 is required to compare the values"
fi
case "$TOLERANCE" in
  '' | *[!0-9.]*) usage_error "--tolerance must be a number like 0.05" ;;
esac

python3 - "$REPORT" "$CSV" "$TOLERANCE" "$FORMAT" "$FAIL_ON_MISMATCH" "$ALLOW_MISSING" "$LABEL" <<'PY'
import csv
import json
import math
import sys

report_path, csv_path, tolerance, fmt, fail_on_mismatch, allow_missing, label = sys.argv[1:8]
tolerance = float(tolerance)
fail_on_mismatch = fail_on_mismatch == "1"
allow_missing = allow_missing == "1"

with open(report_path, encoding="utf-8") as handle:
    report = json.load(handle)
objects = report.get("objects") or {}


def lookup(path):
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


def resolve(object_name, metric):
    """Return (value, ci95, path) for a CSV row, or (None, None, None)."""
    candidates = []
    if object_name and object_name not in ("@model", "model", "-"):
        candidates.append("objects.%s.%s" % (object_name, metric))
        candidates.append("%s.%s" % (object_name, metric))
    candidates.append(metric)
    if object_name and object_name not in ("@model", "model", "-"):
        candidates.append("objects.%s" % object_name)
    for path in candidates:
        node = lookup(path)
        value = numeric(node)
        if value is not None:
            return value, ci95(node), path
    return None, None, None


def parse_value(raw):
    text = raw.strip()
    if text.endswith("%"):
        return float(text[:-1].strip()) / 100.0
    return float(text)


rows = []
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

missing = []
for object_name, metric, expected in parsed:
    actual, spread, path = resolve(object_name, metric)
    if actual is None:
        missing.append((object_name, metric))
        continue
    abs_diff = actual - expected
    if expected == 0:
        rel_diff = 0.0 if abs_diff == 0 else math.inf
        within = abs(abs_diff) <= tolerance
    else:
        rel_diff = abs_diff / abs(expected)
        within = abs(rel_diff) <= tolerance
    rows.append(
        {
            "object": object_name or "@model",
            "metric": metric,
            "external": expected,
            "openflex": actual,
            "abs_diff": abs_diff,
            "rel_diff": rel_diff,
            "ci95": spread,
            "within": within,
            "path": path,
        }
    )

mismatches = [row for row in rows if not row["within"]]
if fmt == "json":
    print(
        json.dumps(
            {
                "model": report.get("model"),
                "stop_time": report.get("stop_time"),
                "replications": len(report.get("replications") or []),
                "tolerance": tolerance,
                "rows": [
                    {
                        **row,
                        "rel_diff": None if math.isinf(row["rel_diff"]) else row["rel_diff"],
                    }
                    for row in rows
                ],
                "missing": [{"object": o, "metric": m} for o, m in missing],
                "mismatches": len(mismatches),
                "within_tolerance": len(rows) - len(mismatches),
            },
            indent=2,
        )
    )
elif fmt == "csv":
    writer = csv.writer(sys.stdout)
    writer.writerow(
        ["object", "metric", "%s" % label, "openflex", "abs_diff", "rel_diff", "within_tolerance"]
    )
    for row in rows:
        writer.writerow(
            [
                row["object"],
                row["metric"],
                "%g" % row["external"],
                "%g" % row["openflex"],
                "%+.6g" % row["abs_diff"],
                "n/a" if math.isinf(row["rel_diff"]) else "%+.2f%%" % (row["rel_diff"] * 100),
                "yes" if row["within"] else "no",
            ]
        )
    for object_name, metric in missing:
        writer.writerow([object_name or "@model", metric, "", "MISSING", "", "", "no"])
elif fmt == "table":
    print("%-16s %-22s %12s %12s %12s %10s %s" % ("object", "metric", label, "openflex", "abs_diff", "rel_diff", "within"))
    print("-" * 96)
    for row in rows:
        rel = "n/a" if math.isinf(row["rel_diff"]) else "%+.2f%%" % (row["rel_diff"] * 100)
        print(
            "%-16s %-22s %12.6g %12.6g %+12.4g %10s %s"
            % (
                row["object"],
                row["metric"],
                row["external"],
                row["openflex"],
                row["abs_diff"],
                rel,
                "yes" if row["within"] else "NO",
            )
        )
else:
    print("| object | metric | %s | OpenFlex | abs diff | rel diff | within ±%g%% |" % (label, tolerance * 100))
    print("|---|---|---|---|---|---|---|")
    for row in rows:
        rel = "n/a" if math.isinf(row["rel_diff"]) else "%+.2f%%" % (row["rel_diff"] * 100)
        mark = "yes" if row["within"] else "**NO**"
        print(
            "| %s | %s | %.6g | %.6g | %+.6g | %s | %s |"
            % (row["object"], row["metric"], row["external"], row["openflex"], row["abs_diff"], rel, mark)
        )

print()
print(
    "compare-flexsim: %d value(s) compared, %d within ±%g%%, %d outside, %d missing"
    % (len(rows), len(rows) - len(mismatches), tolerance * 100, len(mismatches), len(missing)),
    file=sys.stderr,
)
if missing:
    for object_name, metric in missing:
        print(
            "compare-flexsim: missing OpenFlex metric: %s / %s" % (object_name or "@model", metric),
            file=sys.stderr,
        )

status = 0
if fail_on_mismatch and mismatches:
    status = 1
if missing and not allow_missing:
    status = 1
sys.exit(status)
PY
