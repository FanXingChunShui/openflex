#!/usr/bin/env bash
# Short fuzz run for OpenFlex, usable locally and in the optional CI job.
#
# Two modes:
#   cargo-fuzz  when cargo-fuzz and a nightly toolchain are installed: runs the
#               real libFuzzer targets in fuzz/fuzz_targets for --seconds each.
#   fallback    otherwise: generates a corpus of mutated models and expression
#               payloads, feeds them to `openflex validate`, and fails if any of
#               them makes the binary panic, abort or exit with an unexpected
#               status code (0 = valid, 2 = rejected model are both fine).
#
# The fallback exists because libFuzzer needs a nightly toolchain; the target
# parsing paths are still exercised on stable, just with a smaller corpus.
#
# Usage:
#   scripts/fuzz-smoke.sh                     # auto: cargo-fuzz if available
#   scripts/fuzz-smoke.sh --seconds 60
#   scripts/fuzz-smoke.sh --mode fallback --iterations 500
#   scripts/fuzz-smoke.sh --dry-run
#   scripts/fuzz-smoke.sh --keep
#
# Options:
#   --seconds <N>      Seconds per fuzz target in cargo-fuzz mode (default: 30).
#   --iterations <N>   Corpus size in fallback mode (default: 200).
#   --mode <M>         auto | cargo-fuzz | fallback (default: auto).
#   --seed <N>         Seed for the corpus generator (default: 1).
#   --keep             Keep the generated corpus directory.
#   --dry-run          Print what would run; change nothing.
#   -h, --help         Show this help.
#
# Exit code: 0 ok, 1 a crash or unexpected failure, 2 usage error.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly ROOT_DIR
readonly PROGRAM="fuzz-smoke.sh"

FUZZ_SECONDS=30
ITERATIONS=200
MODE="auto"
SEED=1
KEEP=0
DRY_RUN=0
PASSED=0
FAILED=0
SKIPPED=0

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
pass() { PASSED=$((PASSED + 1)); printf '  PASS  %s\n' "$1"; }
fail() { FAILED=$((FAILED + 1)); printf '  FAIL  %s\n' "$1"; }
skip() { SKIPPED=$((SKIPPED + 1)); printf '  SKIP  %s\n' "$1"; }
section() { printf '\n== %s ==\n' "$1"; }

usage() {
  sed -n '2,31p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --seconds) [ "$#" -ge 2 ] || usage_error "--seconds needs a value"; FUZZ_SECONDS="$2"; shift 2 ;;
    --seconds=*) FUZZ_SECONDS="${1#*=}"; shift ;;
    --iterations) [ "$#" -ge 2 ] || usage_error "--iterations needs a value"; ITERATIONS="$2"; shift 2 ;;
    --iterations=*) ITERATIONS="${1#*=}"; shift ;;
    --mode) [ "$#" -ge 2 ] || usage_error "--mode needs a value"; MODE="$2"; shift 2 ;;
    --mode=*) MODE="${1#*=}"; shift ;;
    --seed) [ "$#" -ge 2 ] || usage_error "--seed needs a value"; SEED="$2"; shift 2 ;;
    --seed=*) SEED="${1#*=}"; shift ;;
    --keep) KEEP=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) usage_error "unknown argument '$1'" ;;
  esac
done

case "$MODE" in
  auto | cargo-fuzz | fallback) : ;;
  *) usage_error "--mode must be auto, cargo-fuzz or fallback (got '$MODE')" ;;
esac
case "$FUZZ_SECONDS" in
  '' | *[!0-9]*) usage_error "--seconds must be a positive integer" ;;
esac
case "$ITERATIONS" in
  '' | *[!0-9]*) usage_error "--iterations must be a positive integer" ;;
esac
case "$SEED" in
  '' | *[!0-9]*) usage_error "--seed must be a non-negative integer" ;;
esac

WORK="$(mktemp -d "${TMPDIR:-/tmp}/openflex-fuzz.XXXXXX")"
CORPUS="$WORK/corpus"
mkdir -p "$CORPUS"
cleanup() {
  if [ "$KEEP" -eq 1 ]; then
    printf '\ncorpus kept: %s\n' "$CORPUS"
  else
    rm -rf "$WORK"
  fi
}
trap cleanup EXIT INT TERM

section "Environment"
printf '  host:       %s %s\n' "$(uname -s)" "$(uname -m)"
printf '  mode:       %s\n' "$MODE"
printf '  seconds:    %s (cargo-fuzz)\n' "$FUZZ_SECONDS"
printf '  iterations: %s (fallback)\n' "$ITERATIONS"

have_cargo_fuzz=0
if command -v cargo-fuzz >/dev/null 2>&1; then
  have_cargo_fuzz=1
fi
have_nightly=0
if command -v rustup >/dev/null 2>&1 && rustup toolchain list 2>/dev/null | grep -q '^nightly'; then
  have_nightly=1
fi
if [ -f "$ROOT_DIR/fuzz/Cargo.toml" ]; then
  pass "fuzz targets present: fuzz/Cargo.toml"
else
  fail "fuzz/Cargo.toml is missing"
fi

if [ "$MODE" = "auto" ]; then
  if [ "$have_cargo_fuzz" -eq 1 ] && [ "$have_nightly" -eq 1 ]; then
    MODE="cargo-fuzz"
  else
    MODE="fallback"
  fi
  info "auto mode selected: $MODE"
fi

# ---------------------------------------------------------------- cargo-fuzz
run_cargo_fuzz() {
  target="$1"
  info "fuzzing $target for ${FUZZ_SECONDS}s"
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '[dry-run] (cd fuzz && cargo +nightly fuzz run %s -- -max_total_time=%s -rss_limit_mb=2048)\n' \
      "$target" "$FUZZ_SECONDS"
    return 0
  fi
  if (cd "$ROOT_DIR/fuzz" && cargo +nightly fuzz run "$target" -- \
    -max_total_time="$FUZZ_SECONDS" -rss_limit_mb=2048 -print_final_stats=1); then
    pass "fuzz target $target survived ${FUZZ_SECONDS}s"
  else
    fail "fuzz target $target crashed (see fuzz/artifacts/ for the reproducer)"
  fi
}

if [ "$MODE" = "cargo-fuzz" ]; then
  section "cargo-fuzz"
  if [ "$DRY_RUN" -eq 0 ] && { [ "$have_cargo_fuzz" -eq 0 ] || [ "$have_nightly" -eq 0 ]; }; then
    skip "cargo-fuzz or a nightly toolchain is missing; use --mode fallback"
  else
    run_cargo_fuzz model_json
    run_cargo_fuzz expr_parse
  fi
fi

# ---------------------------------------------------------------- fallback
BIN="$ROOT_DIR/target/release/openflex"
if [ ! -x "$BIN" ]; then
  BIN="$ROOT_DIR/target/debug/openflex"
fi

if [ "$MODE" = "fallback" ] || [ "$DRY_RUN" -eq 1 ]; then
  section "Fallback fuzzing (mutated models through openflex validate)"
  if [ "$DRY_RUN" -eq 0 ] && [ ! -x "$BIN" ]; then
    info "building a debug binary for the fallback run"
    if cargo build --manifest-path "$ROOT_DIR/Cargo.toml" -p openflex-cli >/dev/null 2>&1; then
      BIN="$ROOT_DIR/target/debug/openflex"
    else
      die "could not build openflex-cli"
    fi
  fi

  if ! command -v python3 >/dev/null 2>&1; then
    skip "python3 is required to generate the fallback corpus"
  elif [ "$DRY_RUN" -eq 0 ] && [ ! -x "$BIN" ]; then
    skip "no openflex binary available for the fallback run"
  else
    info "generating $ITERATIONS corpus files"
    if [ "$DRY_RUN" -eq 1 ]; then
      printf '[dry-run] python3 generate_corpus %s %s %s\n' "$ITERATIONS" "$SEED" "$CORPUS"
      printf '[dry-run] %s validate <each corpus file> (expect exit 0 or 2)\n' "$BIN"
      printf '\nRESULT: PASS (dry-run)\n'
      exit 0
    fi
    if [ "$DRY_RUN" -eq 0 ]; then
      python3 - "$ROOT_DIR" "$ITERATIONS" "$SEED" "$CORPUS" <<'PY'
import json
import os
import random
import sys

root, iterations, seed, corpus = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
random.seed(seed)

models = []
for name in ("examples/golden/mm1.ofm", "examples/triggers.ofm", "examples/routing.ofm"):
    path = os.path.join(root, name)
    if os.path.exists(path):
        models.append(open(path, "rb").read())
if not models:
    print("fuzz-smoke: no seed models found", file=sys.stderr)
    sys.exit(1)

EXPR_SNIPPETS = [
    "time", "item.type", "1 + 2 * 3", "(time > 10)", "if(time > 1, 1, 0)",
    "item.label.serial % 5 == 0", "((((((((((1))))))))))", "1 / 0",
    "time -", "\"unterminated", "abs(-3)", "min(1, 2, 3)", "unknown(1)",
    "1e309", "-1e309", "0x10", "a.b.c.d.e", "if(", "if(1,2)", "item.label['x']",
]

def random_scalar():
    return random.choice([
        0, -1, 1, 3.5, 1e15, -1e15, True, False, None, "", "x", "exponential",
        {"distribution": "exponential", "mean": 1.0}, [1, 2, 3], [{"a": 1}],
        {"nested": {"deep": [None, {"x": [1, {"y": [2]}]}]}},
    ])

def mutate_json(value, depth=0):
    """Randomly delete keys, change value types or inject unknown keys."""
    if isinstance(value, dict):
        out = {}
        for key, item in value.items():
            roll = random.random()
            if roll < 0.12:
                continue  # drop the key
            if roll < 0.24:
                out[key] = random_scalar()
            elif roll < 0.30:
                out[key] = None
            else:
                out[key] = mutate_json(item, depth + 1)
        if random.random() < 0.20:
            out["unknown_" + str(random.randint(0, 999))] = random_scalar()
        return out
    if isinstance(value, list):
        items = [mutate_json(item, depth + 1) for item in value if random.random() > 0.1]
        if random.random() < 0.12 and items:
            items = items[:1]
        return items
    if isinstance(value, str):
        roll = random.random()
        if roll < 0.10:
            return ""
        if roll < 0.20:
            return random.choice(EXPR_SNIPPETS)
        if roll < 0.26:
            return "x" * random.randint(1, 200)
        return value
    if isinstance(value, (int, float)) and random.random() < 0.12:
        return random.choice([0, -1, 1e18, -1e18])
    return value

def write(index, payload):
    path = os.path.join(corpus, "case-%04d.ofm" % index)
    with open(path, "wb") as handle:
        handle.write(payload)

count = 0
for index in range(iterations):
    base = random.choice(models)
    style = random.random()
    if style < 0.45:
        # Structural mutation of a valid model.
        try:
            doc = json.loads(base)
        except json.JSONDecodeError:
            continue
        payload = json.dumps(mutate_json(doc)).encode()
    elif style < 0.75:
        # Byte-level mutation: truncate, splice, insert junk.
        data = bytearray(base)
        for _ in range(random.randint(1, 6)):
            if not data:
                break
            op = random.random()
            pos = random.randrange(len(data))
            if op < 0.4:
                del data[pos:pos + random.randint(1, 20)]
            elif op < 0.7:
                data[pos:pos] = bytes([random.randrange(1, 128)])
            else:
                data = data[:random.randrange(len(data) + 1)]
        payload = bytes(data)
    else:
        # Expression payloads injected into trigger/condition strings.
        try:
            doc = json.loads(base)
        except json.JSONDecodeError:
            continue
        text = json.dumps(doc)
        for _ in range(random.randint(1, 3)):
            text = text.replace('"value": "time"', '"value": "%s"' % random.choice(EXPR_SNIPPETS), 1)
        payload = text.encode()
    write(count, payload)
    count += 1

print("fuzz-smoke: wrote %d corpus files" % count)
PY
      pass "generated corpus with $ITERATIONS iterations"
    fi

    # Run every generated file through `validate`. Exit code 0 (accepted) and
    # 2 (rejected model) are both correct; anything else is a crash.
    crashes=0
    checked=0
    for model in "$CORPUS"/*.ofm; do
      [ -f "$model" ] || continue
      checked=$((checked + 1))
      set +e
      output="$("$BIN" validate "$model" 2>&1)"
      code=$?
      set -e
      if [ "$code" -ne 0 ] && [ "$code" -ne 2 ]; then
        crashes=$((crashes + 1))
        fail "validate $(basename "$model") exited $code"
        printf '%s\n' "$output" | head -n 5 | sed 's/^/        /'
      elif printf '%s' "$output" | grep -q 'panicked at'; then
        crashes=$((crashes + 1))
        fail "validate $(basename "$model") panicked"
        printf '%s\n' "$output" | head -n 5 | sed 's/^/        /'
      fi
    done
    if [ "$crashes" -eq 0 ]; then
      pass "all $checked mutated models were rejected or accepted without a crash"
    else
      info "reproducers kept in $CORPUS"
    fi
  fi
fi

section "Summary"
printf '  passed:  %d\n' "$PASSED"
printf '  failed:  %d\n' "$FAILED"
printf '  skipped: %d\n' "$SKIPPED"

if [ "$FAILED" -gt 0 ]; then
  printf '\nRESULT: FAIL (%d failure(s))\n' "$FAILED" >&2
  exit 1
fi
printf '\nRESULT: PASS\n'
