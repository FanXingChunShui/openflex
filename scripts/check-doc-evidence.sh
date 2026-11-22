#!/usr/bin/env bash
# Every test name cited in the parity evidence ledger must exist in the source.
#
# "Cited as a test" means the backticked name is followed by a full-width
# parenthesis, which is how the ledger writes `test_name`（file.rs，what it
# asserts）. Bare identifiers are parameters or types and are not checked.
#
# A ledger that cites invented test names is a new kind of lie: it looks like
# proof and is not. This check greps each backticked identifier from
# docs/PARITY_EVIDENCE.md and fails when no `fn <name>` declares it.
#
# Usage: bash scripts/check-doc-evidence.sh [--quiet]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

LEDGER="docs/PARITY_EVIDENCE.md"
[ -f "$LEDGER" ] || { echo "missing $LEDGER" >&2; exit 1; }

# Identifiers that are deliberately not functions: file names, fields, options.
IGNORE='^(run_model|run_model_parallel|simulate_traced|validate_collect|check_scripts|openflex|schema|model|engine|stats|conveyor|item_trace|experiment|chart|report|cli)$'

missing=0
checked=0
while read -r name; do
  case "$name" in
    *.rs|*.ofm|*.json|*.md|*.txt) continue ;;
  esac
  [[ "$name" =~ $IGNORE ]] && continue
  # Only snake_case identifiers of test-ish length.
  [[ "$name" =~ ^[a-z][a-z0-9_]{11,}$ ]] || continue
  checked=$((checked + 1))
  if ! grep -rqE "\bfn +${name}\b" crates/ --include=*.rs; then
    echo "MISSING fn: $name (cited in $LEDGER)"
    missing=$((missing + 1))
  fi
done < <(grep -oE '`[a-z][a-z0-9_]{11,}`（' "$LEDGER" | tr -d '`（' | sort -u)

if [ "$missing" -gt 0 ]; then
  echo "---"
  echo "$missing of $checked cited names do not exist in the source"
  exit 1
fi
[ "${1:-}" = "--quiet" ] || echo "all $checked cited names resolve to a fn"
