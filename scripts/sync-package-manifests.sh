#!/usr/bin/env bash
# Keep the package-manager manifests in sync with a release.
#
# The Homebrew formula, the three winget manifests and the Scoop manifest all
# hard-code a version and one or more SHA-256 values. Forgetting to bump them
# makes `brew install` / `winget install` / `scoop install` fail with a checksum
# mismatch, and `scripts/verify-packaging.sh` now fails its manifest check — so
# this script does the mechanical part in one go.
#
# It reads a release's SHA256SUMS.txt (local file or from the GitHub release)
# and rewrites:
#
#   packaging/homebrew/openflex.rb                         version + 3 sha256
#   packaging/winget/FanXingChunShui.OpenFlex.yaml         PackageVersion
#   packaging/winget/FanXingChunShui.OpenFlex.installer.yaml  version, URL, sha256, date
#   packaging/winget/FanXingChunShui.OpenFlex.locale.en-US.yaml  PackageVersion
#   packaging/scoop/openflex.json                          version, 2 URLs/hashes/extract_dir
#
# Usage:
#   scripts/sync-package-manifests.sh --version 0.11.0            # download SHA256SUMS
#   scripts/sync-package-manifests.sh --version 0.11.0 --sums /tmp/SHA256SUMS.txt
#   scripts/sync-package-manifests.sh --version 0.11.0 --dry-run  # show the edits
#   scripts/sync-package-manifests.sh --version 0.11.0 --check    # exit 1 when out of sync
#
# Options:
#   --version <V>    Release version (required, with or without a leading "v").
#   --sums <FILE>    Local SHA256SUMS.txt (default: download from the release).
#   --repo <OWNER/NAME>  GitHub repository (default: FanXingChunShui/openflex).
#   --date <YYYY-MM-DD>  ReleaseDate for the winget installer (default: today, UTC).
#   --dry-run        Print the planned edits; write nothing.
#   --check          Do not write; exit 1 when any manifest is out of sync.
#   -h, --help       Show this help.
#
# Exit code: 0 when everything is (or was made) consistent, 1 when --check found
# a mismatch or a release artifact is missing, 2 on a usage error.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly ROOT_DIR
readonly PROGRAM="sync-package-manifests.sh"

VERSION=""
SUMS=""
REPO="FanXingChunShui/openflex"
DATE=""
DRY_RUN=0
CHECK=0

info() { printf '%s: %s\n' "$PROGRAM" "$*"; }
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
  sed -n '2,40p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version) [ "$#" -ge 2 ] || usage_error "--version needs a value"; VERSION="$2"; shift 2 ;;
    --version=*) VERSION="${1#*=}"; shift ;;
    --sums) [ "$#" -ge 2 ] || usage_error "--sums needs a value"; SUMS="$2"; shift 2 ;;
    --sums=*) SUMS="${1#*=}"; shift ;;
    --repo) [ "$#" -ge 2 ] || usage_error "--repo needs a value"; REPO="$2"; shift 2 ;;
    --repo=*) REPO="${1#*=}"; shift ;;
    --date) [ "$#" -ge 2 ] || usage_error "--date needs a value"; DATE="$2"; shift 2 ;;
    --date=*) DATE="${1#*=}"; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    --check) CHECK=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) usage_error "unknown argument '$1'" ;;
  esac
done

[ -n "$VERSION" ] || usage_error "--version <V> is required"
VERSION="${VERSION#v}"
case "$VERSION" in
  *[!0-9A-Za-z.+_-]*) usage_error "invalid version '$VERSION'" ;;
esac
[ -n "$DATE" ] || DATE="$(date -u '+%Y-%m-%d')"
if [ -n "$SUMS" ] && [ ! -f "$SUMS" ]; then
  die "--sums: no such file: $SUMS"
fi
if ! command -v python3 >/dev/null 2>&1; then
  die "python3 is required"
fi

info "repository: $REPO"
info "version:    $VERSION"
info "date:       $DATE"
if [ -n "$SUMS" ]; then
  info "checksums:  $SUMS (local)"
else
  info "checksums:  downloading SHA256SUMS.txt for v$VERSION"
fi

python3 - "$ROOT_DIR" "$VERSION" "$SUMS" "$REPO" "$DATE" "$DRY_RUN" "$CHECK" <<'PY'
import json
import os
import re
import subprocess
import sys
import urllib.request

root, version, sums_path, repo, date, dry_run, check = sys.argv[1:8]
dry_run = dry_run == "1"
check = check == "1"

ARTIFACTS = {
    "darwin": "openflex-%s-universal-apple-darwin.tar.gz" % version,
    "linux_x86_64": "openflex-%s-x86_64-unknown-linux-gnu.tar.gz" % version,
    "linux_aarch64": "openflex-%s-aarch64-unknown-linux-gnu.tar.gz" % version,
    "windows_x86_64": "openflex-%s-x86_64-pc-windows-msvc.zip" % version,
    "windows_aarch64": "openflex-%s-aarch64-pc-windows-msvc.zip" % version,
    "msi": "openflex-%s-windows-x64.msi" % version,
}


def load_sums():
    if sums_path:
        with open(sums_path, encoding="utf-8") as handle:
            return handle.read()
    url = "https://github.com/%s/releases/download/v%s/SHA256SUMS.txt" % (repo, version)
    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            return response.read().decode("utf-8")
    except Exception as error:  # noqa: BLE001 - report the URL and the reason
        print("sync-package-manifests: error: could not download %s: %s" % (url, error), file=sys.stderr)
        sys.exit(1)


def parse_sums(text):
    hashes = {}
    for line in text.splitlines():
        parts = line.split()
        if len(parts) == 2 and re.fullmatch(r"[0-9a-f]{64}", parts[0]):
            hashes[parts[1].lstrip("*")] = parts[0]
    missing = [name for name in ARTIFACTS.values() if name not in hashes]
    if missing:
        print("sync-package-manifests: error: SHA256SUMS is missing:", file=sys.stderr)
        for name in missing:
            print("  " + name, file=sys.stderr)
        sys.exit(1)
    return {key: hashes[name] for key, name in ARTIFACTS.items()}


hashes = parse_sums(load_sums())

changes = []  # (path, old_line_or_value, new_line_or_value)


def plan(path, old, new):
    if old != new:
        changes.append((path, old, new))


def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()


def write(path, text):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(text)


# ---------------------------------------------------------------- Homebrew
brew_path = os.path.join(root, "packaging/homebrew/openflex.rb")
brew = read(brew_path)
new_brew = brew
new_brew, count = re.subn(r'^(\s*)version "[^"]+"', r'\1version "%s"' % version, new_brew, count=1, flags=re.M)
if count != 1:
    print("sync-package-manifests: error: could not find the Homebrew version line", file=sys.stderr)
    sys.exit(1)
digests = re.findall(r'sha256 "([^"]+)"', new_brew)
if len(digests) != 3:
    print("sync-package-manifests: error: expected 3 sha256 lines in the Homebrew formula, found %d" % len(digests), file=sys.stderr)
    sys.exit(1)
for old_digest, new_digest in zip(digests, (hashes["darwin"], hashes["linux_aarch64"], hashes["linux_x86_64"])):
    new_brew = new_brew.replace('sha256 "%s"' % old_digest, 'sha256 "%s"' % new_digest, 1)
plan(brew_path, brew, new_brew)

# ---------------------------------------------------------------- winget
winget_dir = os.path.join(root, "packaging/winget")
version_path = os.path.join(winget_dir, "FanXingChunShui.OpenFlex.yaml")
locale_path = os.path.join(winget_dir, "FanXingChunShui.OpenFlex.locale.en-US.yaml")
installer_path = os.path.join(winget_dir, "FanXingChunShui.OpenFlex.installer.yaml")

for path in (version_path, locale_path):
    text = read(path)
    new_text, count = re.subn(r"^PackageVersion:\s*\S+", "PackageVersion: %s" % version, text, count=1, flags=re.M)
    if count != 1:
        print("sync-package-manifests: error: no PackageVersion in %s" % path, file=sys.stderr)
        sys.exit(1)
    plan(path, text, new_text)

installer = read(installer_path)
new_installer = installer
new_installer, count = re.subn(r"^PackageVersion:\s*\S+", "PackageVersion: %s" % version, new_installer, count=1, flags=re.M)
if count != 1:
    print("sync-package-manifests: error: no PackageVersion in the winget installer manifest", file=sys.stderr)
    sys.exit(1)
new_installer, count = re.subn(
    r"^(\s*)InstallerUrl:\s*\S+",
    r"\1InstallerUrl: https://github.com/%s/releases/download/v%s/openflex-%s-windows-x64.msi" % (repo, version, version),
    new_installer,
    count=1,
    flags=re.M,
)
if count != 1:
    print("sync-package-manifests: error: no InstallerUrl in the winget installer manifest", file=sys.stderr)
    sys.exit(1)
new_installer, count = re.subn(
    r"^(\s*)InstallerSha256:\s*\S+", r"\1InstallerSha256: %s" % hashes["msi"], new_installer, count=1, flags=re.M
)
if count != 1:
    print("sync-package-manifests: error: no InstallerSha256 in the winget installer manifest", file=sys.stderr)
    sys.exit(1)
new_installer, count = re.subn(r"^ReleaseDate:\s*\S+", "ReleaseDate: %s" % date, new_installer, count=1, flags=re.M)
if count != 1:
    print("sync-package-manifests: error: no ReleaseDate in the winget installer manifest", file=sys.stderr)
    sys.exit(1)
plan(installer_path, installer, new_installer)

# ---------------------------------------------------------------- scoop
scoop_path = os.path.join(root, "packaging/scoop/openflex.json")
scoop = json.loads(read(scoop_path))
scoop["version"] = version
scoop["architecture"]["64bit"]["url"] = (
    "https://github.com/%s/releases/download/v%s/openflex-%s-x86_64-pc-windows-msvc.zip" % (repo, version, version)
)
scoop["architecture"]["64bit"]["hash"] = hashes["windows_x86_64"]
scoop["architecture"]["64bit"]["extract_dir"] = "openflex-%s-x86_64-pc-windows-msvc" % version
scoop["architecture"]["arm64"]["url"] = (
    "https://github.com/%s/releases/download/v%s/openflex-%s-aarch64-pc-windows-msvc.zip" % (repo, version, version)
)
scoop["architecture"]["arm64"]["hash"] = hashes["windows_aarch64"]
scoop["architecture"]["arm64"]["extract_dir"] = "openflex-%s-aarch64-pc-windows-msvc" % version
new_scoop = json.dumps(scoop, indent=2, ensure_ascii=False) + "\n"
plan(scoop_path, read(scoop_path), new_scoop)

# ---------------------------------------------------------------- report
if check:
    if changes:
        print("sync-package-manifests: %d file(s) out of sync with v%s:" % (len(changes), version), file=sys.stderr)
        for path, _, _ in changes:
            print("  " + os.path.relpath(path, root), file=sys.stderr)
        sys.exit(1)
    print("sync-package-manifests: all manifests already match v%s" % version)
    sys.exit(0)

if not changes:
    print("sync-package-manifests: nothing to do; manifests already match v%s" % version)
    sys.exit(0)

for path, old, new in changes:
    relative = os.path.relpath(path, root)
    if dry_run:
        print("[dry-run] would update %s" % relative)
        for old_line, new_line in zip(old.splitlines(), new.splitlines()):
            if old_line != new_line:
                print("  - " + old_line.strip()[:120])
                print("  + " + new_line.strip()[:120])
    else:
        write(path, new)
        print("updated %s" % relative)

print()
if dry_run:
    print("sync-package-manifests: dry-run, %d file(s) would change (v%s)" % (len(changes), version))
else:
    print("sync-package-manifests: %d file(s) updated to v%s" % (len(changes), version))
    print("next: scripts/verify-packaging.sh --skip-build   # manifest check should pass")
PY
