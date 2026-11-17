#!/usr/bin/env bash
# Build the OpenFlex desktop GUI Linux packages (.deb / .rpm) with nfpm.
#
# Usage:
#   packaging/gui/build-gui-packages.sh [options]
#
# Options:
#   --version <V>    Package version (default: workspace version from Cargo.toml).
#   --arch <A>       amd64 or arm64 (default: derived from uname -m).
#   --binary <PATH>  Compiled GUI binary (default: target/release/openflex-gui).
#   --out-dir <DIR>  Output directory (default: dist).
#   --format <F>     deb, rpm or both (default: both).
#   --check          Only check that nfpm is available.
#   -h, --help       Show this help.
#
# The resulting packages install /usr/bin/openflex-gui and a README, and depend
# on the usual X11/GL runtime libraries that eframe needs.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly ROOT_DIR

PROGRAM="build-gui-packages.sh"
VERSION=""
ARCH=""
BINARY=""
OUT_DIR="dist"
FORMAT="both"
CHECK_ONLY=0

info() { printf '%s: %s\n' "$PROGRAM" "$*"; }
die() {
  printf '%s: error: %s\n' "$PROGRAM" "$*" >&2
  exit 1
}

usage() {
  sed -n '2,20p' "$0"
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
    --format) FORMAT="${2:-}"; shift 2 ;;
    --format=*) FORMAT="${1#*=}"; shift ;;
    --check) CHECK_ONLY=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) die "unknown argument '$1' (try --help)" ;;
  esac
done

if ! command -v nfpm >/dev/null 2>&1; then
  die "nfpm is not installed (https://nfpm.goreleaser.com)"
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
    *) die "unsupported architecture '$(uname -m)'; pass --arch amd64 or arm64" ;;
  esac
fi
case "$ARCH" in
  amd64 | arm64) : ;;
  *) die "unsupported --arch '$ARCH' (expected amd64 or arm64)" ;;
esac

if [ -z "$BINARY" ]; then
  BINARY="$ROOT_DIR/target/release/openflex-gui"
fi
case "$BINARY" in
  /*) : ;;
  *) BINARY="$ROOT_DIR/$BINARY" ;;
esac
[ -f "$BINARY" ] || die "binary not found: $BINARY (run: cargo build --release -p openflex-gui)"

case "$OUT_DIR" in
  /*) out_dir="$OUT_DIR" ;;
  *) out_dir="$ROOT_DIR/$OUT_DIR" ;;
esac
mkdir -p "$out_dir"

rpm_arch="$ARCH"
case "$ARCH" in
  amd64) rpm_arch="x86_64" ;;
  arm64) rpm_arch="aarch64" ;;
esac

expand() {
  # nfpm treats its config as a template; expand our placeholders explicitly so
  # the result does not depend on nfpm's own env handling.
  sed -e "s|\${VERSION}|$VERSION|g" \
      -e "s|\${ARCH}|$ARCH|g" \
      -e "s|\${BINARY}|$BINARY|g" \
      -e "s|\${README}|$ROOT_DIR/README.md|g" \
      "$1" > "$2"
}

build_one() {
  local packager="$1" config="$2" target="$3"
  local tmp_config
  tmp_config="$(mktemp "${TMPDIR:-/tmp}/openflex-gui-nfpm.XXXXXX.yaml")"
  expand "$config" "$tmp_config"
  info "building $target"
  nfpm package --config "$tmp_config" --packager "$packager" --target "$target"
  rm -f "$tmp_config"
  [ -f "$target" ] || die "nfpm did not produce $target"
  info "created $target"
}

case "$FORMAT" in
  deb)
    build_one deb "$ROOT_DIR/packaging/gui/nfpm-gui-deb.yaml" \
      "$out_dir/openflex-gui_${VERSION}_${ARCH}.deb"
    ;;
  rpm)
    build_one rpm "$ROOT_DIR/packaging/gui/nfpm-gui-rpm.yaml" \
      "$out_dir/openflex-gui-${VERSION}-1.${rpm_arch}.rpm"
    ;;
  both)
    build_one deb "$ROOT_DIR/packaging/gui/nfpm-gui-deb.yaml" \
      "$out_dir/openflex-gui_${VERSION}_${ARCH}.deb"
    build_one rpm "$ROOT_DIR/packaging/gui/nfpm-gui-rpm.yaml" \
      "$out_dir/openflex-gui-${VERSION}-1.${rpm_arch}.rpm"
    ;;
  *) die "unsupported --format '$FORMAT' (expected deb, rpm or both)" ;;
esac
