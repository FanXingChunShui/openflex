#!/usr/bin/env bash
# Sign (or verify) OpenFlex's SHA256SUMS.txt with a detached GPG signature.
#
# The release checksum file proves integrity; a detached signature proves who
# produced it. Both are published next to each other:
#
#   SHA256SUMS.txt        sha256 of every release artifact
#   SHA256SUMS.txt.asc    ASCII-armoured detached signature of that file
#
# Usage:
#   scripts/sign-checksums.sh --file dist/SHA256SUMS.txt
#   scripts/sign-checksums.sh --file dist/SHA256SUMS.txt --key 0xDEADBEEF
#   scripts/sign-checksums.sh --file dist/SHA256SUMS.txt --verify
#   GPG_PASSPHRASE=... scripts/sign-checksums.sh --file dist/SHA256SUMS.txt --passphrase-env
#   scripts/sign-checksums.sh --file dist/SHA256SUMS.txt --dry-run
#
# Options:
#   --file <FILE>        Checksum file to sign (default: dist/SHA256SUMS.txt).
#   --output <FILE>      Signature path (default: <file>.asc).
#   --key <KEYID>        Signing key (default: gpg's default key).
#   --passphrase-env     Read the passphrase from $GPG_PASSPHRASE (CI friendly).
#   --no-armor           Write a binary .sig instead of an ASCII-armoured .asc.
#   --verify             Verify an existing signature instead of creating one.
#   --dry-run            Print the gpg command and exit 0.
#   -h, --help           Show this help.
#
# Exit code: 0 ok, 1 failure, 2 usage error.
set -euo pipefail

readonly PROGRAM="sign-checksums.sh"

FILE="dist/SHA256SUMS.txt"
OUTPUT=""
KEY=""
ARMOR=1
VERIFY=0
DRY_RUN=0
USE_ENV_PASSPHRASE=0

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
  sed -n '2,32p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --file) [ "$#" -ge 2 ] || usage_error "--file needs a value"; FILE="$2"; shift 2 ;;
    --file=*) FILE="${1#*=}"; shift ;;
    --output) [ "$#" -ge 2 ] || usage_error "--output needs a value"; OUTPUT="$2"; shift 2 ;;
    --output=*) OUTPUT="${1#*=}"; shift ;;
    --key) [ "$#" -ge 2 ] || usage_error "--key needs a value"; KEY="$2"; shift 2 ;;
    --key=*) KEY="${1#*=}"; shift ;;
    --passphrase-env) USE_ENV_PASSPHRASE=1; shift ;;
    --no-armor) ARMOR=0; shift ;;
    --verify) VERIFY=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) usage_error "unknown argument '$1'" ;;
  esac
done

[ -f "$FILE" ] || die "no such checksum file: $FILE"
if [ -z "$OUTPUT" ]; then
  if [ "$ARMOR" -eq 1 ]; then
    OUTPUT="$FILE.asc"
  else
    OUTPUT="$FILE.sig"
  fi
fi

# Show what we are about to sign so a mistyped path is visible in CI logs.
line_count="$(grep -c . "$FILE" || true)"
info "checksum file: $FILE ($line_count entries)"
info "signature:     $OUTPUT"

if [ "$VERIFY" -eq 1 ]; then
  if [ ! -f "$OUTPUT" ]; then
    die "no signature at $OUTPUT (expected the detached signature next to $FILE)"
  fi
  if ! command -v gpg >/dev/null 2>&1; then
    die "gpg is not installed; install GnuPG (macOS: brew install gnupg, Debian: apt-get install gnupg) to verify $OUTPUT"
  fi
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '[dry-run] gpg --verify %s %s\n' "$OUTPUT" "$FILE"
    exit 0
  fi
  gpg --verify "$OUTPUT" "$FILE"
  info "signature verifies"
  exit 0
fi

if ! command -v gpg >/dev/null 2>&1; then
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '[dry-run] gpg --detach-sign (gpg is not installed on this host)\n'
    exit 0
  fi
  die "gpg is not installed; install GnuPG (macOS: brew install gnupg, Debian: apt-get install gnupg)"
fi

gpg_args=(--batch --yes --detach-sign)
if [ "$ARMOR" -eq 1 ]; then
  gpg_args+=(--armor)
fi
gpg_args+=(--output "$OUTPUT")
if [ -n "$KEY" ]; then
  gpg_args+=(--local-user "$KEY")
fi

PASSPHRASE=""
if [ "$USE_ENV_PASSPHRASE" -eq 1 ]; then
  PASSPHRASE="${GPG_PASSPHRASE:-}"
  if [ -z "$PASSPHRASE" ]; then
    if [ -t 0 ]; then
      warn "GPG_PASSPHRASE is empty; gpg will use its normal pinentry"
    else
      die "--passphrase-env was given but GPG_PASSPHRASE is empty"
    fi
  fi
fi

if [ "$DRY_RUN" -eq 1 ]; then
  printf '[dry-run] gpg %s %s\n' "${gpg_args[*]}" "$FILE"
  exit 0
fi

if [ -n "$PASSPHRASE" ]; then
  # Feed the passphrase over fd 3 so it never appears in argv/process listings.
  gpg "${gpg_args[@]}" --pinentry-mode loopback --passphrase-fd 3 3<<<"$PASSPHRASE" "$FILE"
else
  gpg "${gpg_args[@]}" "$FILE"
fi

[ -f "$OUTPUT" ] || die "gpg exited 0 but $OUTPUT does not exist"

gpg --verify "$OUTPUT" "$FILE"
info "wrote $OUTPUT"
printf '\nPublish both files:\n  %s\n  %s\n' "$FILE" "$OUTPUT"
printf 'Users verify with: gpg --verify %s %s\n' "$OUTPUT" "$FILE"
