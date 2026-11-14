#!/usr/bin/env bash
# OpenFlex uninstaller for macOS and Linux.
#
# Removes the binary installed by packaging/install.sh (and the receipt it
# wrote). It only touches files under --prefix; PATH entries are never edited,
# but the script tells you how to remove them.
#
# Usage:
#   bash packaging/uninstall.sh
#   bash packaging/uninstall.sh --prefix "$HOME/.local"
#   bash packaging/uninstall.sh --bin-dir /tmp/openflex-test/bin --dry-run
#
# Options:
#   --prefix <DIR>    Install prefix the binary was installed into
#                     (default: $HOME/.local).
#   --bin-dir <DIR>   Override the binary directory (default: <prefix>/bin).
#   --dry-run, -n     Print what would be removed; change nothing.
#   -h, --help        Show this help.
#
# Exit codes: 0 ok (also when nothing was installed), 1 failure, 2 usage error.
set -euo pipefail

if [ -z "${BASH_VERSION:-}" ]; then
  printf 'uninstall.sh: error: this script requires bash (not sh/dash)\n' >&2
  exit 1
fi

PROGRAM="uninstall.sh"
PREFIX="${HOME}/.local"
BIN_DIR=""
DRY_RUN=0

info() { printf '%s: %s\n' "$PROGRAM" "$*"; }
warn() { printf '%s: warning: %s\n' "$PROGRAM" "$*" >&2; }
die() {
  printf '%s: error: %s\n' "$PROGRAM" "$*" >&2
  exit 1
}
usage_error() {
  printf '%s: error: %s\n' "$PROGRAM" "$*" >&2
  exit 2
}

usage() {
  cat <<'EOF'
OpenFlex uninstaller for macOS and Linux.

Usage:
  bash packaging/uninstall.sh [--prefix DIR] [--bin-dir DIR] [--dry-run]

Options:
  --prefix <DIR>   Install prefix the binary was installed into (default: $HOME/.local).
  --bin-dir <DIR>  Override the binary directory (default: <prefix>/bin).
  --dry-run, -n    Print what would be removed; change nothing.
  -h, --help       Show this help.
EOF
}

normalize_dir() {
  dir="$1"
  case "$dir" in
    "~") dir="$HOME" ;;
    \~/*) dir="$HOME/${dir#\~/}" ;;
  esac
  case "$dir" in
    /*) ;;
    *) dir="$PWD/$dir" ;;
  esac
  while [ "$dir" != "/" ] && [ "${dir%/}" != "$dir" ]; do dir="${dir%/}"; done
  printf '%s\n' "$dir"
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --prefix) [ "$#" -ge 2 ] || usage_error "--prefix needs a value"; PREFIX="$2"; shift 2 ;;
    --prefix=*) PREFIX="${1#*=}"; shift ;;
    --bin-dir) [ "$#" -ge 2 ] || usage_error "--bin-dir needs a value"; BIN_DIR="$2"; shift 2 ;;
    --bin-dir=*) BIN_DIR="${1#*=}"; shift ;;
    --dry-run | -n) DRY_RUN=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) usage_error "unknown argument '$1'" ;;
  esac
done

PREFIX="$(normalize_dir "$PREFIX")"
if [ -z "$BIN_DIR" ]; then
  BIN_DIR="$PREFIX/bin"
else
  BIN_DIR="$(normalize_dir "$BIN_DIR")"
fi

receipt_dir="$PREFIX/share/openflex"
receipt="$receipt_dir/install-receipt.txt"
binary="$BIN_DIR/openflex"

# A receipt narrows down the exact binary path (the user may have installed
# with --bin-dir). Never trust it blindly: only accept a path that looks like
# an openflex binary inside the recorded prefix.
if [ -f "$receipt" ]; then
  recorded_bin="$(sed -n 's/^binary=//p' "$receipt" | head -n 1)"
  recorded_prefix="$(sed -n 's/^prefix=//p' "$receipt" | head -n 1)"
  if [ -n "$recorded_bin" ] && [ "$(basename "$recorded_bin")" = "openflex" ]; then
    case "$recorded_bin" in
      "$PREFIX"/*) binary="$recorded_bin" ;;
      *) [ "$(normalize_dir "${recorded_prefix:-$PREFIX}")" = "$PREFIX" ] && binary="$recorded_bin" ;;
    esac
  fi
fi

removed=0

if [ -f "$binary" ]; then
  if [ "$DRY_RUN" -eq 1 ]; then
    info "[dry-run] would remove $binary"
  else
    rm -f "$binary"
    info "removed $binary"
  fi
  removed=1
else
  info "no binary at $binary"
fi

if [ -d "$receipt_dir" ]; then
  if [ "$DRY_RUN" -eq 1 ]; then
    info "[dry-run] would remove $receipt_dir"
  else
    rm -rf "$receipt_dir"
    info "removed $receipt_dir"
  fi
  removed=1
fi

# Remove directories only when they are empty: they may contain unrelated
# tools the user installed into the same prefix.
for dir in "$BIN_DIR" "$PREFIX/share" "$PREFIX"; do
  [ -d "$dir" ] || continue
  if [ "$DRY_RUN" -eq 1 ]; then
    info "[dry-run] would remove $dir if empty"
  elif rmdir "$dir" 2>/dev/null; then
    info "removed empty directory $dir"
  fi
done

case ":$PATH:" in
  *":$BIN_DIR:"*)
    warn "$BIN_DIR is still on your PATH; remove the export line from your shell rc file to finish"
    ;;
  *) : ;;
esac

if [ "$removed" -eq 0 ] && [ ! -d "$receipt_dir" ]; then
  info "nothing to uninstall under $PREFIX"
fi

if [ "$DRY_RUN" -eq 1 ]; then
  info "[dry-run] nothing was changed"
fi
info "done"
