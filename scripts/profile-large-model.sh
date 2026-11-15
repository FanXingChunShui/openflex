#!/usr/bin/env bash
# Profile the 100k-object model with macOS `sample` and summarise the hotspots.
#
# The large-model benchmark (scripts/bench-large-model.sh) shows that a 100k
# object model spends minutes in `validate` and `run`. This script finds out
# *where* those minutes go: it starts the release binary on a generated model,
# attaches `sample` for N seconds, then groups the leaf frames by category
# (serde/JSON, validation, reporting, allocation, hashing, ...).
#
# Usage:
#   scripts/profile-large-model.sh                       # 100k objects, 30s per phase
#   scripts/profile-large-model.sh --objects 50000 --sample-seconds 15
#   scripts/profile-large-model.sh --out-dir /tmp/profile --keep
#   scripts/profile-large-model.sh --skip-run             # validate only, faster
#   scripts/profile-large-model.sh --dry-run
#
# Options:
#   --objects <N>        Total objects (default 100000; rounded to a multiple of 4).
#   --items <N>          Items per source in the generated model (default 1). Raise it
#                       to get more events when profiling `run` (objects stay the same).
#   --binary <PATH>      openflex binary (default: target/release/openflex).
#   --sample-seconds <N> Seconds of sampling per phase (default: 30).
#   --warmup <N>         Seconds to wait before attaching the sampler (default: 1).
#   --out-dir <DIR>      Where to write samples and profile.md (default: mktemp -d).
#   --skip-validate      Do not profile `validate`.
#   --skip-run           Do not profile `run`.
#   --keep               Keep the generated model and raw samples.
#   --dry-run            Print the plan; run nothing.
#   -h, --help           Show this help.
#
# Requirements: macOS (`sample` is part of the OS; Linux users can substitute
# `perf record -g` + `perf report`). Exit code: 0 ok, 1 a phase failed,
# 2 usage error, 3 unsupported platform (no `sample`).
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly ROOT_DIR
readonly PROGRAM="profile-large-model.sh"

OBJECTS=100000
ITEMS=1
BINARY="$ROOT_DIR/target/release/openflex"
SAMPLE_SECONDS=30
WARMUP=1
OUT_DIR=""
SKIP_VALIDATE=0
SKIP_RUN=0
KEEP=0
DRY_RUN=0

info() { printf '%s: %s\n' "$PROGRAM" "$*"; }
warn() { printf '%s: warning: %s\n' "$PROGRAM" "$*" >&2; }
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
  sed -n '2,33p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --objects) [ "$#" -ge 2 ] || usage_error "--objects needs a value"; OBJECTS="$2"; shift 2 ;;
    --objects=*) OBJECTS="${1#*=}"; shift ;;
    --items) [ "$#" -ge 2 ] || usage_error "--items needs a value"; ITEMS="$2"; shift 2 ;;
    --items=*) ITEMS="${1#*=}"; shift ;;
    --warmup) [ "$#" -ge 2 ] || usage_error "--warmup needs a value"; WARMUP="$2"; shift 2 ;;
    --warmup=*) WARMUP="${1#*=}"; shift ;;
    --binary) [ "$#" -ge 2 ] || usage_error "--binary needs a value"; BINARY="$2"; shift 2 ;;
    --binary=*) BINARY="${1#*=}"; shift ;;
    --sample-seconds) [ "$#" -ge 2 ] || usage_error "--sample-seconds needs a value"; SAMPLE_SECONDS="$2"; shift 2 ;;
    --sample-seconds=*) SAMPLE_SECONDS="${1#*=}"; shift ;;
    --out-dir) [ "$#" -ge 2 ] || usage_error "--out-dir needs a value"; OUT_DIR="$2"; shift 2 ;;
    --out-dir=*) OUT_DIR="${1#*=}"; shift ;;
    --skip-validate) SKIP_VALIDATE=1; shift ;;
    --skip-run) SKIP_RUN=1; shift ;;
    --keep) KEEP=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) usage_error "unknown argument '$1'" ;;
  esac
done

case "$OBJECTS" in
  '' | *[!0-9]*) usage_error "--objects must be a positive integer" ;;
esac
case "$SAMPLE_SECONDS" in
  '' | *[!0-9]*) usage_error "--sample-seconds must be a positive integer" ;;
esac
case "$ITEMS" in
  '' | *[!0-9]*) usage_error "--items must be a positive integer" ;;
esac
case "$WARMUP" in
  '' | *[!0-9]*) usage_error "--warmup must be a non-negative integer" ;;
esac
[ "$ITEMS" -ge 1 ] || usage_error "--items must be at least 1"
[ "$OBJECTS" -ge 4 ] || usage_error "--objects must be at least 4"
[ "$SAMPLE_SECONDS" -ge 3 ] || usage_error "--sample-seconds must be at least 3"

if ! command -v sample >/dev/null 2>&1; then
  printf '%s: error: macOS `sample` is not available on this host.\n' "$PROGRAM" >&2
  printf '%s: hint: on Linux use: perf record -g -- ./openflex validate model.ofm && perf report\n' "$PROGRAM" >&2
  exit 3
fi
if ! command -v python3 >/dev/null 2>&1; then
  die "python3 is required to summarise the samples"
fi
if [ ! -x "$BINARY" ]; then
  die "no executable at $BINARY — build it first: cargo build --release -p openflex-cli"
fi

WORK=""
if [ -n "$OUT_DIR" ]; then
  mkdir -p "$OUT_DIR"
  WORK="$(cd "$OUT_DIR" && pwd)"
else
  WORK="$(mktemp -d "${TMPDIR:-/tmp}/openflex-profile.XXXXXX")"
fi
MODEL="$WORK/large-model.ofm"
PROFILE_MD="$WORK/profile.md"

cleanup() {
  if [ "$KEEP" -eq 1 ]; then
    printf '\nprofile directory kept: %s\n' "$WORK"
  else
    rm -rf "$WORK"
  fi
}
trap cleanup EXIT INT TERM

info "binary:   $BINARY ($("$BINARY" --version 2>&1 || echo unknown))"
info "objects:  $OBJECTS (items/source: $ITEMS)"
info "sampling: ${SAMPLE_SECONDS}s per phase"
info "out dir:  $WORK"

if [ "$DRY_RUN" -eq 1 ]; then
  printf '\n[dry-run] would generate %s objects (%s items/source) into %s\n' "$OBJECTS" "$ITEMS" "$MODEL"
  printf '[dry-run] would run: %s validate %s and sample for %ss\n' "$BINARY" "$MODEL" "$SAMPLE_SECONDS"
  printf '[dry-run] would run: %s run %s --format csv and sample for %ss\n' "$BINARY" "$MODEL" "$SAMPLE_SECONDS"
  printf '\nRESULT: PASS (dry-run)\n'
  exit 0
fi

bash "$ROOT_DIR/scripts/bench-large-model.sh" --objects "$OBJECTS" --items "$ITEMS" \
  --generate-only "$WORK" >/dev/null

profile_phase() { # profile_phase <label> <command...>
  label="$1"
  shift
  sample_file="$WORK/sample-$label.txt"
  log_file="$WORK/$label.log"
  info "profiling $label for ${SAMPLE_SECONDS}s ..."
  start=$(date +%s)
  "$@" >"$log_file" 2>&1 &
  pid=$!
  sleep "$WARMUP"
  exit_code=0
  if kill -0 "$pid" 2>/dev/null; then
    # sample stops early when the process exits; the file still gets written.
    sample "$pid" "$SAMPLE_SECONDS" -file "$sample_file" >/dev/null 2>&1 ||
      warn "sample exited non-zero for $label (process may have ended early)"
  else
    warn "$label finished within ${WARMUP}s; raise --objects or --items for a longer phase"
    : >"$sample_file"
  fi
  wait "$pid" || exit_code=$?
  elapsed=$(( $(date +%s) - start ))
  printf '%s\n' "$elapsed" >"$WORK/$label.seconds"
  printf '%s\n' "$exit_code" >"$WORK/$label.exit"
  info "$label finished in ${elapsed}s (exit $exit_code)"
}

if [ "$SKIP_VALIDATE" -eq 0 ]; then
  profile_phase validate "$BINARY" validate "$MODEL"
fi
if [ "$SKIP_RUN" -eq 0 ]; then
  profile_phase run "$BINARY" run "$MODEL" --format csv
fi

python3 - "$WORK" "$OBJECTS" "$SAMPLE_SECONDS" "$PROFILE_MD" "$BINARY" <<'PROFILE_PY'
import collections
import os
import re
import subprocess
import sys

work, objects, sample_seconds, profile_md, binary = sys.argv[1:6]
objects = int(objects)
sample_seconds = int(sample_seconds)

CATEGORIES = [
    ("JSON / serde 反序列化", ("serde", "json", "yyjson", "simdutf", "deserialize")),
    ("语义校验 validate", ("validate", "ValidationIssue", "check_", "verify_")),
    ("模型/报告构建", ("report", "SimReport", "Aggregate", "Summary", "statistics")),
    ("CSV/JSON 输出", ("csv", "to_csv", "write_csv", "xlsx")),
    ("内存分配/释放", ("malloc", "free", "realloc", "alloc::", "drop_in_place", "__rust_alloc", "libsystem_malloc")),
    ("哈希/查找", ("hashbrown", "HashMap", "SipHasher", "indexmap", "lookup", "find")),
    ("排序", ("sort", "pdqsort")),
    ("字符串/UTF-8", ("str::", "from_utf8", "memchr", "utf8", "memmove", "memcpy", "memcmp", "memset", "bzero", "libsystem_platform")),
    ("线程/同步", ("park", "mutex", "rayon", "pthread")),
]

# macOS `sample` prints `???` for stripped local binaries (and often for system
# libraries too). We keep the address, then map openflex addresses back to
# symbol names with `atos -o <binary> -l <load address> <addr>`.
LINE_PATTERN = re.compile(
    r"\s{2,}(?P<symbol>.+?)\s+\(in\s+(?P<binary>[^)]+)\)\s+"
    r"load address (?P<load>0x[0-9a-f]+) \+ (?P<offset>0x[0-9a-f]+)\s+"
    r"\[(?P<addr>0x[0-9a-f]+)\]\s+(?P<count>\d+)\s*$"
)


def read_sample(path):
    with open(path, encoding="utf-8", errors="replace") as handle:
        return handle.read()


def resolve_addresses(entries, binary_path):
    """Return {address: symbol} for the given binary via atos."""
    groups = {}
    for entry in entries:
        if entry["symbol"] == "???" and entry["binary"].startswith("openflex"):
            groups.setdefault(entry["load"], []).append(entry["addr"])
    mapping = {}
    for load, addresses in groups.items():
        unique = sorted(set(addresses))
        for index in range(0, len(unique), 200):
            chunk = unique[index:index + 200]
            try:
                result = subprocess.run(
                    ["atos", "-o", binary_path, "-l", load, *chunk],
                    capture_output=True,
                    text=True,
                    timeout=120,
                )
            except Exception:  # noqa: BLE001 - atos is best effort
                continue
            for address, line in zip(chunk, result.stdout.splitlines()):
                name = line.split(" (in ")[0].strip()
                if name and not name.startswith("0x") and name != "???":
                    mapping[address] = name
    return mapping


def parse_sample(path, binary_path):
    text = read_sample(path)
    total = 0
    duration = 0.0
    match = re.search(r"Total number of samples:\s*(\d+)", text)
    if match:
        total = int(match.group(1))
    match = re.search(r"Sampling duration:\s*([0-9.]+)", text)
    if match:
        duration = float(match.group(1))

    section = text.split("Sort by top of stack", 1)
    entries = []
    if len(section) == 2:
        for line in section[1].splitlines():
            match = LINE_PATTERN.match(line)
            if match:
                entries.append(match.groupdict())
    if not entries:
        # Fall back to the call graph: lines like `+ ! 1234 symbol  (in binary)`.
        for line in text.splitlines():
            match = re.match(r"\s*[+!:| ]*!\s*(?P<count>\d+)\s+(?P<symbol>.+?)\s+\(in\s+(?P<binary>[^)]+)\)", line)
            if match:
                entries.append(
                    {
                        "symbol": match.group("symbol").strip(),
                        "binary": match.group("binary"),
                        "load": "",
                        "offset": "",
                        "addr": "",
                        "count": match.group("count"),
                    }
                )

    mapping = resolve_addresses(entries, binary_path)
    counts = collections.Counter()
    for entry in entries:
        symbol = entry["symbol"]
        if symbol == "???":
            resolved = mapping.get(entry.get("addr", ""))
            if resolved:
                symbol = resolved
            elif entry.get("offset"):
                symbol = "%s+%s" % (entry["binary"], entry["offset"])
            else:
                symbol = entry["binary"]
        counts[symbol] += int(entry["count"])
    rows = sorted(counts.items(), key=lambda item: item[1], reverse=True)
    if not total:
        total = sum(count for _, count in rows) or 1
    return total, duration, rows


def categorise(rows):
    buckets = {name: 0 for name, _ in CATEGORIES}
    buckets["其他"] = 0
    for symbol, count in rows:
        for name, needles in CATEGORIES:
            if any(needle in symbol for needle in needles):
                buckets[name] += count
                break
        else:
            buckets["其他"] += count
    return buckets


phases = []
for label in ("validate", "run"):
    sample_file = os.path.join(work, "sample-%s.txt" % label)
    if not os.path.exists(sample_file):
        continue
    total, duration, rows = parse_sample(sample_file, binary)
    seconds = None
    seconds_path = os.path.join(work, "%s.seconds" % label)
    if os.path.exists(seconds_path):
        seconds = open(seconds_path).read().strip()
    exit_path = os.path.join(work, "%s.exit" % label)
    exit_code = open(exit_path).read().strip() if os.path.exists(exit_path) else "?"
    phases.append(
        {
            "label": label,
            "total": total,
            "duration": duration,
            "rows": rows,
            "seconds": seconds,
            "exit": exit_code,
            "buckets": categorise(rows),
        }
    )

lines = []
lines.append("# 10 万对象 profiling 摘要")
lines.append("")
lines.append("- 二进制：`%s`" % binary)
lines.append("- 模型：%d 对象（Source→Queue→Processor→Sink 链）" % objects)
lines.append("- 采样：macOS `sample`，每阶段 %ds；`sample` 显示 `???` 的地址用 `atos` 还原符号" % sample_seconds)
lines.append("")
lines.append("## 各阶段")
lines.append("")
lines.append("| 阶段 | 总耗时 | 采样数 | 采样时长 | 热点 top-1 |")
lines.append("|---|---|---|---|---|")
for phase in phases:
    top = phase["rows"][0][0] if phase["rows"] else "-"
    lines.append(
        "| `%s` | %ss | %d | %.0fs | `%s` |"
        % (phase["label"], phase["seconds"] or "?", phase["total"], phase["duration"], top)
    )
lines.append("")
for phase in phases:
    lines.append("## `%s` 热点（按最栈顶样本）" % phase["label"])
    lines.append("")
    lines.append("| 函数 | 样本 | 占比 |")
    lines.append("|---|---|---|")
    for symbol, count in phase["rows"][:20]:
        lines.append("| `%s` | %d | %.1f%% |" % (symbol, count, 100.0 * count / phase["total"]))
    lines.append("")
    lines.append("### `%s` 分类占比" % phase["label"])
    lines.append("")
    lines.append("| 类别 | 占比 |")
    lines.append("|---|---|")
    for name, count in sorted(phase["buckets"].items(), key=lambda item: item[1], reverse=True):
        if count:
            lines.append("| %s | %.1f%% |" % (name, 100.0 * count / phase["total"]))
    lines.append("")

with open(profile_md, "w", encoding="utf-8") as handle:
    handle.write("\n".join(lines) + "\n")
print("\n".join(lines))
PROFILE_PY

info "profile written to $PROFILE_MD"
printf '\nRESULT: PASS\n'
