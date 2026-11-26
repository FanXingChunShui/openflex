#!/usr/bin/env bash
# The "unimplemented" list must say which version it describes.
#
# `docs/UNIMPLEMENTED.md` sat at "基线 v0.10.0" through v1.56.0: it kept claiming
# things were missing that had since been built (per-step operators, table-driven
# arrivals). A stale list is worse than none — it tells readers a capability is
# absent when it exists. This check keeps the baseline tag honest.
#
# Usage: bash scripts/check-doc-baselines.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

version="$(grep -m1 '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')"

for doc in docs/UNIMPLEMENTED.md docs/BACKLOG.md; do
    baseline="$(grep -m1 '基线' "$doc" | grep -o 'v[0-9][0-9.]*' | head -1 || true)"
    if [ -z "$baseline" ]; then
        echo "$doc has no 基线 (baseline) version" >&2
        exit 1
    fi
    if [ "v$version" != "$baseline" ]; then
        echo "$doc baseline is $baseline, Cargo.toml says v$version" >&2
        echo "update the baseline after reviewing the list" >&2
        exit 1
    fi
    echo "doc baselines ok: $doc at $baseline"
done
