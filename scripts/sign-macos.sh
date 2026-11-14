#!/usr/bin/env bash
# Sign and notarise the macOS OpenFlex binary.
#
# Two independent steps, both optional on their own:
#   1. codesign with a Developer ID Application identity and the hardened
#      runtime (required for notarisation).
#   2. xcrun notarytool submit --wait, followed by `xcrun stapler staple` when
#      the target is a bundle/disk image/installer. A bare Mach-O binary gets
#      notarised (the ticket is served online) but cannot be stapled; the
#      script reports that as SKIP instead of failing.
#
# Nothing here runs without an identity. Without an Apple ID / team ID / app
# password (or a stored notarytool keychain profile) the notarisation step
# refuses to start and says exactly which variables are missing.
#
# Usage:
#   scripts/sign-macos.sh --identity "Developer ID Application: ACME (TEAMID)" \
#       --path dist/openflex
#   scripts/sign-macos.sh --identity "$ID" --path dist/openflex --skip-notarize
#   scripts/sign-macos.sh --identity "$ID" --path dist/openflex \
#       --apple-id me@example.com --team-id TEAMID --password "$APP_PASSWORD" \
#       --archive dist/openflex-notarize.zip --zip-out dist/openflex-notarize.zip
#   scripts/sign-macos.sh --identity "$ID" --path dist/openflex --dry-run
#
# Options:
#   --identity <ID>          codesign identity (required unless --skip-sign).
#   --path <FILE>            File or bundle to sign; repeatable.
#   --p12 <FILE>             PKCS#12 certificate to import into a temp keychain.
#   --p12-password <PW>      Password of --p12 (or $MACOS_CERTIFICATE_PASSWORD).
#   --keychain <FILE>        Keychain used for the import (default: temp file).
#   --keychain-password <PW> Password for that keychain (or $MACOS_KEYCHAIN_PASSWORD).
#   --apple-id <EMAIL>       Apple ID for notarytool (or $APPLE_ID).
#   --team-id <TEAMID>       Apple team ID (or $APPLE_TEAM_ID).
#   --password <PW>          App-specific password (or $APPLE_APP_PASSWORD).
#   --keychain-profile <P>   Stored notarytool profile (alternative to the three above).
#   --archive <FILE>         Pre-built zip/dmg/pkg to submit (otherwise zipped from --path).
#   --zip-out <FILE>         Where to write the temporary notarisation zip.
#   --skip-sign              Only notarise (useful when the binary is already signed).
#   --skip-notarize          Only sign.
#   --require-gatekeeper     Fail when `spctl --assess` rejects the result.
#   --require-staple         Fail when the ticket cannot be stapled.
#   --keep-keychain          Do not delete the temporary keychain on exit.
#   --dry-run                Print the commands instead of running them.
#   -h, --help               Show this help.
#
# Exit code: 0 ok, 1 failure, 2 usage error.
set -euo pipefail

readonly PROGRAM="sign-macos.sh"

IDENTITY=""
TARGETS=()
P12=""
P12_PASSWORD=""
KEYCHAIN=""
KEYCHAIN_PASSWORD=""
APPLE_ID="${APPLE_ID:-}"
TEAM_ID="${APPLE_TEAM_ID:-}"
APPLE_PASSWORD="${APPLE_APP_PASSWORD:-}"
KEYCHAIN_PROFILE=""
ARCHIVE=""
ZIP_OUT=""
SKIP_SIGN=0
SKIP_NOTARIZE=0
REQUIRE_GATEKEEPER=0
REQUIRE_STAPLE=0
KEEP_KEYCHAIN=0
DRY_RUN=0
VERBOSE=0
TEMP_KEYCHAIN=0

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
section() { printf '\n== %s ==\n' "$1"; }

usage() {
  sed -n '2,54p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --identity) [ "$#" -ge 2 ] || usage_error "--identity needs a value"; IDENTITY="$2"; shift 2 ;;
    --identity=*) IDENTITY="${1#*=}"; shift ;;
    --path) [ "$#" -ge 2 ] || usage_error "--path needs a value"; TARGETS+=("$2"); shift 2 ;;
    --path=*) TARGETS+=("${1#*=}"); shift ;;
    --p12) [ "$#" -ge 2 ] || usage_error "--p12 needs a value"; P12="$2"; shift 2 ;;
    --p12=*) P12="${1#*=}"; shift ;;
    --p12-password) [ "$#" -ge 2 ] || usage_error "--p12-password needs a value"; P12_PASSWORD="$2"; shift 2 ;;
    --p12-password=*) P12_PASSWORD="${1#*=}"; shift ;;
    --keychain) [ "$#" -ge 2 ] || usage_error "--keychain needs a value"; KEYCHAIN="$2"; shift 2 ;;
    --keychain=*) KEYCHAIN="${1#*=}"; shift ;;
    --keychain-password) [ "$#" -ge 2 ] || usage_error "--keychain-password needs a value"; KEYCHAIN_PASSWORD="$2"; shift 2 ;;
    --keychain-password=*) KEYCHAIN_PASSWORD="${1#*=}"; shift ;;
    --apple-id) [ "$#" -ge 2 ] || usage_error "--apple-id needs a value"; APPLE_ID="$2"; shift 2 ;;
    --apple-id=*) APPLE_ID="${1#*=}"; shift ;;
    --team-id) [ "$#" -ge 2 ] || usage_error "--team-id needs a value"; TEAM_ID="$2"; shift 2 ;;
    --team-id=*) TEAM_ID="${1#*=}"; shift ;;
    --password) [ "$#" -ge 2 ] || usage_error "--password needs a value"; APPLE_PASSWORD="$2"; shift 2 ;;
    --password=*) APPLE_PASSWORD="${1#*=}"; shift ;;
    --keychain-profile) [ "$#" -ge 2 ] || usage_error "--keychain-profile needs a value"; KEYCHAIN_PROFILE="$2"; shift 2 ;;
    --keychain-profile=*) KEYCHAIN_PROFILE="${1#*=}"; shift ;;
    --archive) [ "$#" -ge 2 ] || usage_error "--archive needs a value"; ARCHIVE="$2"; shift 2 ;;
    --archive=*) ARCHIVE="${1#*=}"; shift ;;
    --zip-out) [ "$#" -ge 2 ] || usage_error "--zip-out needs a value"; ZIP_OUT="$2"; shift 2 ;;
    --zip-out=*) ZIP_OUT="${1#*=}"; shift ;;
    --skip-sign) SKIP_SIGN=1; shift ;;
    --skip-notarize) SKIP_NOTARIZE=1; shift ;;
    --require-gatekeeper) REQUIRE_GATEKEEPER=1; shift ;;
    --require-staple) REQUIRE_STAPLE=1; shift ;;
    --keep-keychain) KEEP_KEYCHAIN=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -v | --verbose) VERBOSE=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) usage_error "unknown argument '$1'" ;;
  esac
done

[ "${#TARGETS[@]}" -gt 0 ] || usage_error "pass at least one --path <FILE>"
if [ "$SKIP_SIGN" -eq 0 ] && [ -z "$IDENTITY" ]; then
  die "--identity is required to sign (or pass --skip-sign to only notarise)"
fi
if [ "$SKIP_SIGN" -eq 0 ] && [ "$(uname -s)" != "Darwin" ]; then
  die "codesign only exists on macOS; run this script on a macOS host or in CI"
fi

for target in "${TARGETS[@]}"; do
  [ -e "$target" ] || die "--path: no such file or directory: $target"
done
if [ -n "$ARCHIVE" ] && [ ! -f "$ARCHIVE" ]; then
  die "--archive: no such file: $ARCHIVE"
fi

# Notarisation credentials: either the three notarytool flags or a profile.
if [ "$SKIP_NOTARIZE" -eq 0 ]; then
  if [ -z "$KEYCHAIN_PROFILE" ]; then
    missing=""
    [ -n "$APPLE_ID" ] || missing="$missing APPLE_ID(--apple-id)"
    [ -n "$TEAM_ID" ] || missing="$missing APPLE_TEAM_ID(--team-id)"
    [ -n "$APPLE_PASSWORD" ] || missing="$missing APPLE_APP_PASSWORD(--password)"
    if [ -n "$missing" ]; then
      die "notarisation needs credentials; missing:$missing. Set the environment variables, pass the flags, or use --skip-notarize."
    fi
  fi
fi

run() { # run <command...>
  if [ "$VERBOSE" -eq 1 ] || [ "$DRY_RUN" -eq 1 ]; then
    printf '[%s] %s\n' "$([ "$DRY_RUN" -eq 1 ] && echo dry-run || echo run)" "$*"
  fi
  if [ "$DRY_RUN" -eq 0 ]; then
    "$@"
  fi
}

WORK=""
if [ "$DRY_RUN" -eq 0 ]; then
  WORK="$(mktemp -d "${TMPDIR:-/tmp}/openflex-sign.XXXXXX")"
else
  WORK="${TMPDIR:-/tmp}/openflex-sign-dry-run"
fi

cleanup_keychain() {
  if [ "$TEMP_KEYCHAIN" -eq 1 ] && [ "$KEEP_KEYCHAIN" -eq 0 ] && [ "$DRY_RUN" -eq 0 ]; then
    security delete-keychain "$KEYCHAIN" 2>/dev/null || true
  fi
}
cleanup() {
  cleanup_keychain
  if [ "$DRY_RUN" -eq 0 ] && [ -n "$WORK" ] && [ -d "$WORK" ]; then
    rm -rf "$WORK"
  fi
}
trap cleanup EXIT INT TERM

section "Environment"
printf '  host:   %s %s\n' "$(uname -s)" "$(uname -m)"
printf '  target: %s\n' "${TARGETS[*]}"
printf '  dry-run: %s\n' "$([ "$DRY_RUN" -eq 1 ] && echo yes || echo no)"

# --- optional certificate import (CI) ---------------------------------------
if [ -n "$P12" ]; then
  [ -f "$P12" ] || die "--p12: no such file: $P12"
  [ -n "$P12_PASSWORD" ] || die "--p12-password is required when --p12 is given"
  if [ -z "$KEYCHAIN" ]; then
    KEYCHAIN="$WORK/openflex-signing.keychain-db"
    TEMP_KEYCHAIN=1
  fi
  if [ -z "$KEYCHAIN_PASSWORD" ]; then
    KEYCHAIN_PASSWORD="$(openssl rand -hex 16 2>/dev/null || echo openflex-temp-keychain)"
  fi
  section "Import certificate into a temporary keychain"
  run security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
  run security set-keychain-settings -lut 3600 "$KEYCHAIN"
  run security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
  run security import "$P12" -k "$KEYCHAIN" -P "$P12_PASSWORD" -T /usr/bin/codesign
  run security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN"
  info "certificate imported into $KEYCHAIN"
fi

# --- sign --------------------------------------------------------------------
sign_one() { # sign_one <path>
  target="$1"
  if [ -d "$target" ]; then
    codesign_args=(--deep --force --options runtime --timestamp --sign "$IDENTITY")
  else
    # --deep is meaningless for a bare Mach-O file and newer codesign versions
    # warn about it, so only bundles get it.
    codesign_args=(--force --options runtime --timestamp --sign "$IDENTITY")
  fi
  run codesign "${codesign_args[@]}" "$target"
  run codesign --verify --strict --verbose=2 "$target"
  run codesign --display --verbose=2 "$target"
}

if [ "$SKIP_SIGN" -eq 0 ]; then
  section "codesign"
  for target in "${TARGETS[@]}"; do
    info "signing $target"
    sign_one "$target"
  done
else
  section "codesign"
  info "skipped (--skip-sign)"
fi

# --- gatekeeper assessment ---------------------------------------------------
if [ "$SKIP_SIGN" -eq 0 ]; then
  section "Gatekeeper assessment (spctl)"
  for target in "${TARGETS[@]}"; do
    if [ "$DRY_RUN" -eq 1 ]; then
      printf '[dry-run] spctl --assess --type execute --verbose=2 %s\n' "$target"
      continue
    fi
    if spctl --assess --type execute --verbose=2 "$target" 2>"$WORK/spctl.log"; then
      info "spctl accepted $target"
    else
      if [ "$REQUIRE_GATEKEEPER" -eq 1 ]; then
        cat "$WORK/spctl.log" >&2 || true
        die "spctl rejected $target (notarisation is what makes Gatekeeper accept a downloaded binary)"
      fi
      warn "spctl rejected $target (expected before notarisation / with --skip-notarize)"
    fi
  done
fi

# --- notarise ----------------------------------------------------------------
if [ "$SKIP_NOTARIZE" -eq 1 ]; then
  section "Notarisation"
  info "skipped (--skip-notarize)"
  section "Summary"
  printf '  signed:     %s\n' "$([ "$SKIP_SIGN" -eq 1 ] && echo no || echo yes)"
  printf '  notarised:  no\n'
  printf '\nRESULT: PASS\n'
  exit 0
fi

section "Notarisation (notarytool submit --wait)"
ARCHIVE_TO_SUBMIT="$ARCHIVE"
if [ -z "$ARCHIVE_TO_SUBMIT" ]; then
  # notarytool accepts zip, dmg and pkg. A bare Mach-O binary has to be zipped
  # (with its parent directory kept, so the ticket is bound to the file path in
  # the archive).
  ARCHIVE_TO_SUBMIT="${ZIP_OUT:-$WORK/notarize.zip}"
  if [ "${#TARGETS[@]}" -eq 1 ] && [ -f "${TARGETS[0]}" ]; then
    run ditto -c -k --keepParent "${TARGETS[0]}" "$ARCHIVE_TO_SUBMIT"
  else
    run ditto -c -k "${TARGETS[@]}" "$ARCHIVE_TO_SUBMIT"
  fi
  [ "$DRY_RUN" -eq 1 ] || [ -f "$ARCHIVE_TO_SUBMIT" ] || die "could not build the notarisation archive"
fi

notary_args=(xcrun notarytool submit "$ARCHIVE_TO_SUBMIT" --wait)
if [ -n "$KEYCHAIN_PROFILE" ]; then
  notary_args+=(--keychain-profile "$KEYCHAIN_PROFILE")
else
  notary_args+=(--apple-id "$APPLE_ID" --team-id "$TEAM_ID" --password "$APPLE_PASSWORD")
fi
run "${notary_args[@]}"

section "Stapler"
stapled=0
for target in "${TARGETS[@]}"; do
  case "$target" in
    *.app | *.dmg | *.pkg) run xcrun stapler staple "$target"; stapled=$((stapled + 1)) ;;
    *)
      # A notarised bare binary carries no stapleable ticket: Gatekeeper
      # validates it online. Say so instead of pretending to staple.
      info "not stapling $target (stapler only supports .app/.dmg/.pkg)"
      ;;
  esac
done
if [ "$stapled" -eq 0 ] && [ "$REQUIRE_STAPLE" -eq 1 ]; then
  die "--require-staple was given but no stapleable target (.app/.dmg/.pkg) was passed"
fi
if [ "$stapled" -gt 0 ]; then
  for target in "${TARGETS[@]}"; do
    case "$target" in
      *.app | *.dmg | *.pkg) run xcrun stapler validate "$target" ;;
    esac
  done
fi

section "Summary"
printf '  signed:     %s\n' "$([ "$SKIP_SIGN" -eq 1 ] && echo no || echo yes)"
printf '  archive:    %s\n' "$ARCHIVE_TO_SUBMIT"
printf '  stapled:    %s\n' "$stapled"
printf '\nRESULT: PASS\n'
