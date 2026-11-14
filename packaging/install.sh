#!/usr/bin/env bash
# OpenFlex installer for macOS and Linux.
#
# Installs the prebuilt `openflex` binary from a GitHub release (or from a
# local release directory) into a user-writable prefix. No sudo, no system
# PATH edits, no daemons: the script only writes inside --prefix.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/openflex/openflex/main/packaging/install.sh | bash
#   bash packaging/install.sh --version 0.1.0 --prefix "$HOME/.local"
#   bash packaging/install.sh --from-dir dist --prefix /tmp/openflex-test
#
# Options:
#   --version <V>     Release version to install (default: latest release).
#   --prefix <DIR>    Install prefix (default: $HOME/.local).
#   --bin-dir <DIR>   Directory for the binary (default: <prefix>/bin).
#   --from-dir <DIR>  Install from a local release directory instead of the network.
#   --base-url <URL>  Release download base URL
#                     (default: https://github.com/openflex/openflex/releases/download).
#   --dry-run         Print what would happen; change nothing.
#   --force           Reinstall even when the same version is already present.
#   --require-checksum  Fail when no SHA256SUMS.txt is available.
#   --no-path-hint    Do not print the PATH instructions.
#   -h, --help        Show this help.
#
# Exit codes: 0 ok, 1 failure, 2 usage error.
set -euo pipefail

if [ -z "${BASH_VERSION:-}" ]; then
  printf 'install.sh: error: this installer requires bash (not sh/dash)\n' >&2
  exit 1
fi

PROGRAM="install.sh"
REPO="${OPENFLEX_REPO:-openflex/openflex}"
BASE_URL="${OPENFLEX_BASE_URL:-https://github.com/${REPO}/releases/download}"
API_URL="${OPENFLEX_API_URL:-https://api.github.com/repos/${REPO}/releases/latest}"
VERSION=""
PREFIX="${HOME}/.local"
BIN_DIR=""
FROM_DIR=""
DRY_RUN=0
FORCE=0
REQUIRE_CHECKSUM=0
PATH_HINT=1

say() { printf '%s\n' "$*"; }
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
  cat <<'EOF'
OpenFlex installer for macOS and Linux.

Usage:
  curl -fsSL https://raw.githubusercontent.com/openflex/openflex/main/packaging/install.sh | bash
  bash packaging/install.sh --version 0.1.0 --prefix "$HOME/.local"
  bash packaging/install.sh --from-dir dist --prefix /tmp/openflex-test

Options:
  --version <V>        Release version to install (default: latest release).
  --prefix <DIR>       Install prefix (default: $HOME/.local).
  --bin-dir <DIR>      Directory for the binary (default: <prefix>/bin).
  --from-dir <DIR>     Install from a local release directory instead of the network.
  --base-url <URL>     Release download base URL
                       (default: https://github.com/openflex/openflex/releases/download).
  --dry-run, -n        Print what would happen; change nothing.
  --force, -f          Reinstall even when the same version is already present.
  --require-checksum   Fail when no SHA256SUMS.txt is available.
  --no-path-hint       Do not print the PATH instructions.
  -h, --help           Show this help.

Environment:
  OPENFLEX_REPO      GitHub repository (default: openflex/openflex).
  OPENFLEX_BASE_URL  Override the release download base URL.
  OPENFLEX_API_URL   Override the "latest release" API URL.

Exit codes: 0 ok, 1 failure, 2 usage error.
EOF
}

# --- argument parsing -------------------------------------------------------
while [ "$#" -gt 0 ]; do
  case "$1" in
    --version) [ "$#" -ge 2 ] || usage_error "--version needs a value"; VERSION="$2"; shift 2 ;;
    --version=*) VERSION="${1#*=}"; shift ;;
    --prefix) [ "$#" -ge 2 ] || usage_error "--prefix needs a value"; PREFIX="$2"; shift 2 ;;
    --prefix=*) PREFIX="${1#*=}"; shift ;;
    --bin-dir) [ "$#" -ge 2 ] || usage_error "--bin-dir needs a value"; BIN_DIR="$2"; shift 2 ;;
    --bin-dir=*) BIN_DIR="${1#*=}"; shift ;;
    --from-dir) [ "$#" -ge 2 ] || usage_error "--from-dir needs a value"; FROM_DIR="$2"; shift 2 ;;
    --from-dir=*) FROM_DIR="${1#*=}"; shift ;;
    --base-url) [ "$#" -ge 2 ] || usage_error "--base-url needs a value"; BASE_URL="$2"; shift 2 ;;
    --base-url=*) BASE_URL="${1#*=}"; shift ;;
    --dry-run | -n) DRY_RUN=1; shift ;;
    --force | -f) FORCE=1; shift ;;
    --require-checksum) REQUIRE_CHECKSUM=1; shift ;;
    --no-path-hint) PATH_HINT=0; shift ;;
    -h | --help) usage; exit 0 ;;
    *) usage_error "unknown argument '$1'" ;;
  esac
done

# --- helpers ----------------------------------------------------------------
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

fetch() { # fetch <url> <destination>
  url="$1"
  destination="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 --connect-timeout 15 -o "$destination" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$destination" "$url"
  else
    die "need curl or wget to download $url"
  fi
}

fetch_stdout() { # fetch_stdout <url>
  url="$1"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 2 --connect-timeout 10 "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O - "$url"
  else
    die "need curl or wget to reach $url"
  fi
}

url_exists() { # url_exists <url>
  url="$1"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSIL --retry 1 --connect-timeout 10 "$url" >/dev/null 2>&1
  else
    fetch_stdout "$url" >/dev/null 2>&1
  fi
}

normalize_dir() { # normalize_dir <dir> -> absolute, no trailing slash
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

hint_path() { # hint_path <bin-dir>
  bin_dir="$1"
  case ":$PATH:" in
    *":$bin_dir:"*)
      say ""
      info "$bin_dir is already on your PATH"
      return 0
      ;;
  esac
  shell_name="$(basename "${SHELL:-sh}" 2>/dev/null || echo sh)"
  say ""
  say "OpenFlex is installed but $bin_dir is not on your PATH yet."
  say "For this shell only:"
  say "  export PATH=\"$bin_dir:\$PATH\""
  say "Permanently:"
  case "$shell_name" in
    zsh) say "  echo 'export PATH=\"$bin_dir:\$PATH\"' >> ~/.zshrc" ;;
    fish) say "  fish_add_path \"$bin_dir\"" ;;
    bash) say "  echo 'export PATH=\"$bin_dir:\$PATH\"' >> ~/.bashrc" ;;
    *) say "  add the export line above to your shell's rc file" ;;
  esac
  say ""
}

detect_os() {
  uname_s="$(uname -s 2>/dev/null || echo unknown)"
  case "$uname_s" in
    Darwin) printf 'darwin\n' ;;
    Linux) printf 'linux\n' ;;
    MINGW* | MSYS* | CYGWIN*) die "this is a Windows shell; use packaging/install.ps1 instead (e.g. irm ... | iex)" ;;
    *) die "unsupported operating system '$uname_s'; OpenFlex publishes macOS and Linux builds (Windows: install.ps1)" ;;
  esac
}

detect_arch() {
  machine="$(uname -m 2>/dev/null || echo unknown)"
  case "$machine" in
    x86_64 | amd64 | x64) arch="x86_64" ;;
    arm64 | aarch64) arch="aarch64" ;;
    *) die "unsupported CPU architecture '$machine'; only x86_64 and aarch64 builds are published" ;;
  esac
  # An x86_64 shell under Rosetta still runs on Apple silicon: prefer the
  # native build, which is faster and matches the hardware.
  if [ "$(detect_os)" = "darwin" ] && [ "$arch" = "x86_64" ]; then
    if [ "$(sysctl -n hw.optional.arm64 2>/dev/null || echo 0)" = "1" ]; then
      arch="aarch64"
    fi
  fi
  printf '%s\n' "$arch"
}

is_musl() {
  if [ -e /lib/ld-musl-x86_64.so.1 ] || [ -e /lib/ld-musl-aarch64.so.1 ]; then
    return 0
  fi
  ldd --version 2>&1 | head -n 1 | grep -qi musl
}

# Prints the platform suffixes to try, most specific first.
platform_candidates() {
  os="$1"
  arch="$2"
  case "$os" in
    darwin)
      printf 'universal-apple-darwin\n'
      printf '%s-apple-darwin\n' "$arch"
      ;;
    linux)
      if is_musl; then
        printf '%s-unknown-linux-musl\n' "$arch"
      fi
      printf '%s-unknown-linux-gnu\n' "$arch"
      ;;
    *) die "no platform mapping for $os" ;;
  esac
}

resolve_version() {
  if [ -n "$VERSION" ]; then
    VERSION="${VERSION#v}"
    printf '%s\n' "$VERSION"
    return 0
  fi
  info "resolving the latest release from $API_URL"
  tag="$(fetch_stdout "$API_URL" 2>/dev/null | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1 || true)"
  [ -n "$tag" ] || die "could not resolve the latest version (network, proxy or rate limit?); pass --version <V>"
  printf '%s\n' "${tag#v}"
}

# --- main flow ---------------------------------------------------------------
OS="$(detect_os)"
ARCH="$(detect_arch)"
PREFIX="$(normalize_dir "$PREFIX")"
if [ -z "$BIN_DIR" ]; then
  BIN_DIR="$PREFIX/bin"
else
  BIN_DIR="$(normalize_dir "$BIN_DIR")"
fi

if [ -n "$FROM_DIR" ]; then
  [ -d "$FROM_DIR" ] || die "--from-dir: no such directory: $FROM_DIR"
  FROM_DIR="$(normalize_dir "$FROM_DIR")"
fi

VERSION="$(resolve_version)"
case "$VERSION" in
  "" | *[!0-9A-Za-z.+_-]*) die "invalid version '$VERSION'" ;;
esac
info "installing OpenFlex $VERSION for $OS/$ARCH"

# Find the archive: a local file when --from-dir is given, otherwise the first
# candidate URL that exists on the release page.
ARCHIVE_PATH=""
ARCHIVE_URL=""
PLATFORM=""
CANDIDATES="$(platform_candidates "$OS" "$ARCH")"

if [ -n "$FROM_DIR" ]; then
  for platform in $CANDIDATES; do
    candidate="$FROM_DIR/openflex-$VERSION-$platform.tar.gz"
    if [ -f "$candidate" ]; then
      ARCHIVE_PATH="$candidate"
      PLATFORM="$platform"
      break
    fi
  done
  if [ -z "$ARCHIVE_PATH" ]; then
    found="$(find "$FROM_DIR" -maxdepth 1 -type f -name 'openflex-*.tar.gz' | LC_ALL=C sort | tail -n 1 || true)"
    if [ -n "$found" ]; then
      warn "no archive matched this platform; using $found"
      ARCHIVE_PATH="$found"
      PLATFORM="unknown"
    fi
  fi
  if [ -z "$ARCHIVE_PATH" ]; then
    if [ -f "$FROM_DIR/openflex" ] && [ -x "$FROM_DIR/openflex" ]; then
      ARCHIVE_PATH="$FROM_DIR/openflex"
      PLATFORM="local-binary"
    elif [ -f "$FROM_DIR/openflex.exe" ]; then
      die "--from-dir: found a Windows build (openflex.exe); use packaging/install.ps1 on Windows"
    else
      die "--from-dir: no openflex-$VERSION-<platform>.tar.gz or openflex binary found in $FROM_DIR"
    fi
  fi
  info "using local archive $ARCHIVE_PATH"
else
  for platform in $CANDIDATES; do
    candidate_url="$BASE_URL/v$VERSION/openflex-$VERSION-$platform.tar.gz"
    if url_exists "$candidate_url"; then
      ARCHIVE_URL="$candidate_url"
      PLATFORM="$platform"
      break
    fi
  done
  if [ -z "$ARCHIVE_URL" ]; then
    warn "no published build matched this platform; tried:"
    for platform in $CANDIDATES; do
      warn "  $BASE_URL/v$VERSION/openflex-$VERSION-$platform.tar.gz"
    done
    die "release v$VERSION does not provide a build for $OS/$ARCH"
  fi
  info "downloading $ARCHIVE_URL"
fi

if [ "$DRY_RUN" -eq 1 ]; then
  say ""
  say "[dry-run] would install OpenFlex $VERSION ($PLATFORM)"
  if [ -n "$ARCHIVE_URL" ]; then
    say "[dry-run]   download: $ARCHIVE_URL"
  else
    say "[dry-run]   source:   $ARCHIVE_PATH"
  fi
  say "[dry-run]   verify:   SHA-256 from $(dirname "$ARCHIVE_PATH")/SHA256SUMS.txt (or the release page)"
  say "[dry-run]   binary:   $BIN_DIR/openflex"
  say "[dry-run]   receipt:  $PREFIX/share/openflex/install-receipt.txt"
  say "[dry-run] nothing was changed"
  exit 0
fi

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/openflex-install.XXXXXX")"
cleanup() { rm -rf "$tmp_dir"; }
trap cleanup EXIT INT TERM

if [ -z "$ARCHIVE_PATH" ]; then
  ARCHIVE_PATH="$tmp_dir/openflex-$VERSION-$PLATFORM.tar.gz"
  fetch "$ARCHIVE_URL" "$ARCHIVE_PATH" || die "download failed: $ARCHIVE_URL"
fi
[ -f "$ARCHIVE_PATH" ] || die "archive not found after download: $ARCHIVE_PATH"

# --- checksum ---------------------------------------------------------------
SUMS_PATH=""
if [ -n "$FROM_DIR" ] && [ -f "$FROM_DIR/SHA256SUMS.txt" ]; then
  SUMS_PATH="$FROM_DIR/SHA256SUMS.txt"
elif [ -z "$FROM_DIR" ]; then
  sums_url="$BASE_URL/v$VERSION/SHA256SUMS.txt"
  if url_exists "$sums_url"; then
    SUMS_PATH="$tmp_dir/SHA256SUMS.txt"
    fetch "$sums_url" "$SUMS_PATH" || warn "could not download $sums_url"
  fi
fi

sha_verified=0
if [ -n "$SUMS_PATH" ] && [ -f "$SUMS_PATH" ]; then
  archive_name="$(basename "$ARCHIVE_PATH")"
  expected="$(awk -v name="$archive_name" '$2 == name || $2 == "*" name { print $1; exit }' "$SUMS_PATH" || true)"
  if [ -z "$expected" ]; then
    if [ "$REQUIRE_CHECKSUM" -eq 1 ]; then
      die "no checksum entry for $archive_name in $SUMS_PATH"
    fi
    warn "no checksum entry for $archive_name in $SUMS_PATH; continuing without verification"
  else
    actual="$(sha256_of "$ARCHIVE_PATH")"
    if [ "$actual" != "$expected" ]; then
      die "checksum mismatch for $archive_name: expected $expected, got $actual"
    fi
    sha_verified=1
    info "sha256 verified: $actual"
  fi
elif [ "$REQUIRE_CHECKSUM" -eq 1 ]; then
  die "no SHA256SUMS.txt available to verify $ARCHIVE_PATH"
else
  warn "no SHA256SUMS.txt available; skipping checksum verification"
fi

# --- extract ----------------------------------------------------------------
extract_dir="$tmp_dir/extract"
mkdir -p "$extract_dir"

case "$ARCHIVE_PATH" in
  *.tar.gz | *.tgz)
    tar -xzf "$ARCHIVE_PATH" -C "$extract_dir" || die "could not extract $ARCHIVE_PATH"
    source_bin="$(find "$extract_dir" -type f -name openflex -perm -u+x | LC_ALL=C sort | head -n 1 || true)"
    [ -n "$source_bin" ] || source_bin="$(find "$extract_dir" -type f -name openflex | LC_ALL=C sort | head -n 1 || true)"
    ;;
  *.zip)
    die "this installer handles .tar.gz archives; on Windows use packaging/install.ps1"
    ;;
  *)
    if [ -f "$ARCHIVE_PATH" ] && [ -x "$ARCHIVE_PATH" ] && [ "$(basename "$ARCHIVE_PATH")" = "openflex" ]; then
      source_bin="$ARCHIVE_PATH"
    else
      die "unsupported archive type: $ARCHIVE_PATH"
    fi
    ;;
esac
[ -n "$source_bin" ] || die "the archive does not contain an 'openflex' binary"
info "found binary $(basename "$source_bin")"

# --- install ----------------------------------------------------------------
target="$BIN_DIR/openflex"

if [ -x "$target" ] && [ "$FORCE" -eq 0 ]; then
  current="$("$target" --version 2>/dev/null || true)"
  if [ "$current" = "openflex $VERSION" ]; then
    info "openflex $VERSION is already installed at $target (use --force to reinstall)"
    if [ "$PATH_HINT" -eq 1 ]; then
      hint_path "$BIN_DIR"
    fi
    exit 0
  fi
fi

if [ -e "$BIN_DIR" ] && [ ! -d "$BIN_DIR" ]; then
  die "$BIN_DIR exists and is not a directory"
fi
mkdir -p "$BIN_DIR" 2>/dev/null || die "cannot create $BIN_DIR (choose another --prefix, e.g. --prefix \"\$HOME/.local\")"
[ -w "$BIN_DIR" ] || die "$BIN_DIR is not writable (choose another --prefix, e.g. \"\$HOME/.local\")"

tmp_target="$BIN_DIR/.openflex.tmp.$$"
cp "$source_bin" "$tmp_target" || die "could not copy to $tmp_target"
chmod 755 "$tmp_target"
# Installed from an archive downloaded by curl, but clear quarantine anyway:
# a user who copied a release archive out of a browser would otherwise hit
# Gatekeeper on first run.
if command -v xattr >/dev/null 2>&1; then
  xattr -d com.apple.quarantine "$tmp_target" 2>/dev/null || true
fi
mv "$tmp_target" "$target"
info "installed $target"

# --- receipt ----------------------------------------------------------------
receipt_dir="$PREFIX/share/openflex"
receipt="$receipt_dir/install-receipt.txt"
mkdir -p "$receipt_dir" 2>/dev/null || warn "could not create $receipt_dir; skipping receipt"
if [ -d "$receipt_dir" ]; then
  {
    printf 'version=%s\n' "$VERSION"
    printf 'platform=%s\n' "$PLATFORM"
    printf 'binary=%s\n' "$target"
    printf 'prefix=%s\n' "$PREFIX"
    printf 'checksum_verified=%s\n' "$sha_verified"
    printf 'installed_at=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    if [ -n "$ARCHIVE_URL" ]; then printf 'source=%s\n' "$ARCHIVE_URL"; else printf 'source=%s\n' "$ARCHIVE_PATH"; fi
  } >"$receipt"
  info "wrote receipt $receipt"
fi

# --- verify -----------------------------------------------------------------
if [ ! -x "$target" ]; then
  die "installed binary is not executable: $target"
fi
installed_output="$("$target" --version 2>&1 || true)"
case "$installed_output" in
  "openflex $VERSION"*) info "verified: $installed_output" ;;
  *"cannot be opened"* | *"killed"* | *"quarantine"*)
    die "macOS refused to run the binary ($installed_output); try: xattr -d com.apple.quarantine \"$target\""
    ;;
  *)
    die "the installed binary did not report version $VERSION (got: ${installed_output:-no output})"
    ;;
esac

# --- PATH -------------------------------------------------------------------
if [ "$PATH_HINT" -eq 1 ]; then
  hint_path "$BIN_DIR"
fi
say ""
info "done. Try: openflex --help"
