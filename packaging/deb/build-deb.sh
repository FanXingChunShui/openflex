#!/usr/bin/env bash
# Build the OpenFlex .deb package with nfpm.
#
# Requirements: nfpm >= 2.20 (https://nfpm.goreleaser.com). It is a single
# static binary, no rpmbuild/dpkg tooling needed:
#   brew install nfpm
#   go install github.com/goreleaser/nfpm/v2/cmd/nfpm@latest
#   https://github.com/goreleaser/nfpm/releases
#
# Usage:
#   packaging/deb/build-deb.sh
#   packaging/deb/build-deb.sh --version 0.1.0 --arch amd64 \
#       --binary target/release/openflex --out-dir dist
#
# Options:
#   --version <V>    Package version (default: workspace version from Cargo.toml).
#   --arch <A>       amd64 or arm64 (default: derived from `uname -m`).
#   --binary <PATH>  Compiled binary (default: target/release/openflex).
#   --out-dir <DIR>  Output directory (default: dist).
#   --check          Only check that nfpm is available.
#   -h, --help       Show this help.
#
# The resulting package installs /usr/bin/openflex and
# /usr/share/doc/openflex/README.md.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly ROOT_DIR

PROGRAM="build-deb.sh"
VERSION=""
ARCH=""
BINARY=""
OUT_DIR="dist"
CHECK_ONLY=0

info() { printf '%s: %s\n' "$PROGRAM" "$*"; }
die() {
  printf '%s: error: %s\n' "$PROGRAM" "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Build the OpenFlex .deb package with nfpm.

Usage: packaging/deb/build-deb.sh [options]

Options:
  --version <V>    Package version (default: workspace version from Cargo.toml).
  --arch <A>       amd64 or arm64 (default: derived from uname -m).
  --binary <PATH>  Compiled binary (default: target/release/openflex).
  --out-dir <DIR>  Output directory (default: dist).
  --check          Only check that nfpm is available.
  -h, --help       Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version) VERSION="${2:-}"; shift 2 ;;
    --version=*) VERSION="${1#*=}"; shift ;;
    --arch) ARCH="${2:-}"; shift 2 ;;
    --arch=*) ARCH="${1#*=}"; shift ;;
    --binary) BINARY="${2:-}"; shift 2 ;;
    --binary=*) BINARY="${1#*=}"; shift ;;
    --out-dir) OUT_DIR="${2:-}"; shift 2 ;;
    --out-dir=*) OUT_DIR="${1#*=}"; shift ;;
    --check) CHECK_ONLY=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) die "unknown argument '$1' (try --help)" ;;
  esac
done

if ! command -v nfpm >/dev/null 2>&1; then
  printf '%s: error: nfpm is not installed\n' "$PROGRAM" >&2
  cat >&2 <<'EOF'
Install nfpm (single static binary, no root required):
  go install github.com/goreleaser/nfpm/v2/cmd/nfpm@latest
  brew install nfpm
  curl -fsSL https://github.com/goreleaser/nfpm/releases/latest/download/nfpm_<os>_<arch>.tar.gz | tar -xz nfpm
The optional deb-rpm CI job installs it automatically.
EOF
  exit 1
fi
if [ "$CHECK_ONLY" -eq 1 ]; then
  info "nfpm found: $(command -v nfpm)"
  nfpm --version
  exit 0
fi

if [ -z "$VERSION" ]; then
  VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT_DIR/Cargo.toml" | head -n 1)"
fi
[ -n "$VERSION" ] || die "could not determine the version; pass --version"

if [ -z "$ARCH" ]; then
  case "$(uname -m)" in
    x86_64 | amd64) ARCH="amd64" ;;
    aarch64 | arm64) ARCH="arm64" ;;
    *) die "unsupported architecture '$(uname -m)'; pass --arch amd64 or --arch arm64" ;;
  esac
fi
case "$ARCH" in
  amd64 | arm64) : ;;
  *) die "unsupported --arch '$ARCH' (expected amd64 or arm64)" ;;
esac

if [ -z "$BINARY" ]; then
  BINARY="$ROOT_DIR/target/release/openflex"
fi
case "$BINARY" in
  /*) : ;;
  *) BINARY="$ROOT_DIR/$BINARY" ;;
esac
[ -f "$BINARY" ] || die "binary not found: $BINARY (run: cargo build --release -p openflex-cli)"

case "$OUT_DIR" in
  /*) out_dir="$OUT_DIR" ;;
  *) out_dir="$ROOT_DIR/$OUT_DIR" ;;
esac
mkdir -p "$out_dir"
target="$out_dir/openflex_${VERSION}_${ARCH}.deb"

info "building $target"
VERSION="$VERSION" ARCH="$ARCH" BINARY="$BINARY" README="$ROOT_DIR/README.md" \
  nfpm package \
  --config "$ROOT_DIR/packaging/deb/nfpm.yaml" \
  --packager deb \
  --target "$target"

[ -f "$target" ] || die "nfpm did not produce $target"
info "created $target"
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$target" | awk '{printf "%s  %s\n", $1, $2}'
elif command -v shasum >/dev/null 2>&1; then
  shasum -a 256 "$target" | awk '{printf "%s  %s\n", $1, $2}'
fi
