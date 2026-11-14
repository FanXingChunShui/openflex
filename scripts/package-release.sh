#!/usr/bin/env bash
# Package one OpenFlex release archive and record its SHA-256 checksum.
#
# Used by .github/workflows/release.yml and by scripts/verify-packaging.sh.
# Works on macOS and Linux with bash (3.2 or newer). On Windows, run it from
# Git Bash, or let the workflow use the PowerShell fallback instead.
#
# Usage:
#   scripts/package-release.sh --version 0.1.0 \
#       --platform x86_64-apple-darwin \
#       --binary target/release/openflex \
#       [--out-dir dist] [--archive tar.gz|zip] [--no-checksum]
#
# The archive layout is:
#   openflex-<version>-<platform>/openflex[.exe]
#   openflex-<version>-<platform>/README.md        (when present in the repo)
#
# Outputs:
#   <out-dir>/openflex-<version>-<platform>.tar.gz  (or .zip)
#   <out-dir>/SHA256SUMS.txt                        (merged, sorted, one line per archive)
set -euo pipefail

readonly SCRIPT_NAME="package-release"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly ROOT_DIR

VERSION=""
PLATFORM=""
BINARY=""
OUT_DIR="dist"
ARCHIVE_FORMAT=""
NO_CHECKSUM=0

log() { printf '%s: %s\n' "$SCRIPT_NAME" "$*"; }
warn() { printf '%s: warning: %s\n' "$SCRIPT_NAME" "$*" >&2; }
die() { printf '%s: error: %s\n' "$SCRIPT_NAME" "$*" >&2; exit 1; }

usage() {
  sed -n '2,25p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version) VERSION="${2:-}"; shift 2 ;;
    --version=*) VERSION="${1#*=}"; shift ;;
    --platform) PLATFORM="${2:-}"; shift 2 ;;
    --platform=*) PLATFORM="${1#*=}"; shift ;;
    --binary) BINARY="${2:-}"; shift 2 ;;
    --binary=*) BINARY="${1#*=}"; shift ;;
    --out-dir) OUT_DIR="${2:-}"; shift 2 ;;
    --out-dir=*) OUT_DIR="${1#*=}"; shift ;;
    --archive) ARCHIVE_FORMAT="${2:-}"; shift 2 ;;
    --archive=*) ARCHIVE_FORMAT="${1#*=}"; shift ;;
    --no-checksum) NO_CHECKSUM=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) die "unknown argument '$1' (try --help)" ;;
  esac
done

[ -n "$VERSION" ] || die "--version is required"
[ -n "$PLATFORM" ] || die "--platform is required"
[ -n "$BINARY" ] || die "--binary is required"
[ -f "$BINARY" ] || die "binary not found: $BINARY"

# Allow a relative path for --binary even when called from another directory.
case "$BINARY" in
  /*) : ;;
  *) BINARY="$ROOT_DIR/$BINARY" ;;
esac
[ -f "$BINARY" ] || die "binary not found: $BINARY"

# Default archive format: zip for Windows, tar.gz everywhere else.
if [ -z "$ARCHIVE_FORMAT" ]; then
  case "$PLATFORM" in
    *windows*) ARCHIVE_FORMAT="zip" ;;
    *) ARCHIVE_FORMAT="tar.gz" ;;
  esac
fi
case "$ARCHIVE_FORMAT" in
  tar.gz | tgz | zip) : ;;
  *) die "unsupported archive format '$ARCHIVE_FORMAT' (expected tar.gz or zip)" ;;
esac
[ "$ARCHIVE_FORMAT" = "tgz" ] && ARCHIVE_FORMAT="tar.gz"

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$1" | awk '{print $NF}'
  else
    die "no SHA-256 tool found (need sha256sum, shasum or openssl)"
  fi
}

stage="$(mktemp -d "${TMPDIR:-/tmp}/openflex-package.XXXXXX")"
cleanup() { rm -rf "$stage"; }
trap cleanup EXIT INT TERM

pkg_name="openflex-${VERSION}-${PLATFORM}"
pkg_dir="$stage/$pkg_name"
mkdir -p "$pkg_dir"

# Ship the binary under its public name, whatever the host calls it.
case "$ARCHIVE_FORMAT" in
  zip) cp "$BINARY" "$pkg_dir/openflex.exe" ;;
  *) cp "$BINARY" "$pkg_dir/openflex" ;;
esac
chmod 755 "$pkg_dir"/openflex*

# README and (if it ever exists) the licence files make the archive usable
# without a network round trip. They are optional so this script still works
# for a bare binary.
for extra in README.md LICENSE-MIT LICENSE-APACHE LICENSE; do
  if [ -f "$ROOT_DIR/$extra" ]; then
    cp "$ROOT_DIR/$extra" "$pkg_dir/$extra"
  fi
done
[ -f "$pkg_dir/README.md" ] || warn "no README.md at repository root; archive contains only the binary"

mkdir -p "$ROOT_DIR/$OUT_DIR" 2>/dev/null || true
case "$OUT_DIR" in
  /*) out_dir="$OUT_DIR" ;;
  *) out_dir="$ROOT_DIR/$OUT_DIR" ;;
esac
mkdir -p "$out_dir"

archive="$out_dir/$pkg_name.$ARCHIVE_FORMAT"
rm -f "$archive"

case "$ARCHIVE_FORMAT" in
  tar.gz)
    tar -czf "$archive" -C "$stage" "$pkg_name"
    ;;
  zip)
    if command -v zip >/dev/null 2>&1; then
      (cd "$stage" && zip -qr "$archive" "$pkg_name")
    elif command -v python3 >/dev/null 2>&1; then
      python3 - "$stage" "$pkg_name" "$archive" <<'PY'
import os
import sys
import zipfile

stage, pkg_name, archive = sys.argv[1], sys.argv[2], sys.argv[3]
with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zf:
    for base, _dirs, files in os.walk(os.path.join(stage, pkg_name)):
        for name in sorted(files):
            full = os.path.join(base, name)
            rel = os.path.relpath(full, stage)
            zf.write(full, rel)
PY
    else
      die "no zip tool found (need zip or python3)"
    fi
    ;;
esac

[ -f "$archive" ] || die "archive was not created: $archive"
size="$(wc -c <"$archive" | tr -d ' ')"
log "created $archive ($size bytes)"

if [ "$NO_CHECKSUM" -eq 1 ]; then
  log "skipping checksum (--no-checksum)"
  exit 0
fi

digest="$(sha256_of "$archive")"
sums="$out_dir/SHA256SUMS.txt"
touch "$sums"
# Replace any previous entry for this archive, then keep the file sorted.
tmp_sums="$sums.tmp.$$"
awk -v name="$pkg_name.$ARCHIVE_FORMAT" '$2 != name { print }' "$sums" >"$tmp_sums"
printf '%s  %s\n' "$digest" "$pkg_name.$ARCHIVE_FORMAT" >>"$tmp_sums"
LC_ALL=C sort -k2 "$tmp_sums" >"$sums"
rm -f "$tmp_sums"
log "recorded sha256 in $sums"
printf '%s  %s\n' "$digest" "$pkg_name.$ARCHIVE_FORMAT"
