#!/usr/bin/env bash
# Release checklist for OpenFlex: bump, check, sync, verify, print the tag commands.
#
# The order is fixed on purpose — the failure mode this prevents is releasing a
# version whose package-manager manifests, Cargo.lock or verification docs still
# point at the previous one (see docs/VERIFICATION.md "已修复问题").
#
#   [1/5] preflight      branch, clean tree, semver sanity, version must increase
#   [2/5] bump           Cargo.toml [workspace.package].version + cargo check (updates Cargo.lock)
#   [3/5] sync           scripts/sync-package-manifests.sh --version <V>  (needs the release's SHA256SUMS)
#   [4/5] verify         scripts/verify-packaging.sh --skip-build
#   [5/5] next steps     print git tag/push commands and the post-release checks
#
# Steps 3–4 can only run *after* the tag has been pushed and the release
# workflow has published the artifacts, so the script has two phases:
#
#   prepare (default)  steps 1–2 (+ prints 3–5 as todo)
#   publish            steps 3–4 (+ prints 5); run after the release exists
#   all                steps 1–5 in one go (only when the release already exists,
#                      e.g. re-running a failed publish on an existing tag)
#
# It never commits, tags or pushes: those commands are printed for a human.
#
# Usage:
#   scripts/release.sh --version 0.22.0 --dry-run      # show everything, change nothing
#   scripts/release.sh --version 0.22.0                # bump + cargo check
#   scripts/release.sh --version 0.22.0 --phase publish  # sync manifests + verify
#   scripts/release.sh --version 0.22.0 --phase all
#
# Options:
#   --version <V>     New version, X.Y.Z (or X.Y.Z-rc.N); required.
#   --phase <P>       prepare (default) | publish | all.
#   --dry-run         Print every command; change nothing.
#   --allow-dirty     Skip the clean-working-tree preflight.
#   --allow-branch <B>  Allowed branch (default: main).
#   --repo <OWNER/NAME>  Repository for the release URL (default: FanXingChunShui/openflex).
#   -h, --help        Show this help.
#
# Exit code: 0 ok, 1 a step failed, 2 usage error.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly ROOT_DIR
readonly PROGRAM="release.sh"

VERSION=""
PHASE="prepare"
DRY_RUN=0
ALLOW_DIRTY=0
ALLOW_BRANCH="main"
REPO="FanXingChunShui/openflex"

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
step() { printf '\n[%s/5] %s\n' "$1" "$2"; }
run() { # run <command...>
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '[dry-run] %s\n' "$*"
    return 0
  fi
  "$@"
}

usage() {
  sed -n '2,45p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version) [ "$#" -ge 2 ] || usage_error "--version needs a value"; VERSION="$2"; shift 2 ;;
    --version=*) VERSION="${1#*=}"; shift ;;
    --phase) [ "$#" -ge 2 ] || usage_error "--phase needs a value"; PHASE="$2"; shift 2 ;;
    --phase=*) PHASE="${1#*=}"; shift ;;
    --repo) [ "$#" -ge 2 ] || usage_error "--repo needs a value"; REPO="$2"; shift 2 ;;
    --repo=*) REPO="${1#*=}"; shift ;;
    --allow-branch) [ "$#" -ge 2 ] || usage_error "--allow-branch needs a value"; ALLOW_BRANCH="$2"; shift 2 ;;
    --allow-branch=*) ALLOW_BRANCH="${1#*=}"; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    --allow-dirty) ALLOW_DIRTY=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) usage_error "unknown argument '$1'" ;;
  esac
done

[ -n "$VERSION" ] || usage_error "--version <V> is required (e.g. --version 0.22.0)"
VERSION="${VERSION#v}"
case "$VERSION" in
  [0-9]*.[0-9]*.[0-9]*) : ;;
  *) usage_error "invalid version '$VERSION' (expected X.Y.Z or X.Y.Z-rc.N)" ;;
esac
case "$VERSION" in
  *[!0-9A-Za-z.+-]*) usage_error "invalid version '$VERSION' (expected X.Y.Z or X.Y.Z-rc.N)" ;;
esac
case "$PHASE" in
  prepare | publish | all) : ;;
  *) usage_error "--phase must be prepare, publish or all (got '$PHASE')" ;;
esac
if ! command -v python3 >/dev/null 2>&1; then
  die "python3 is required"
fi
if ! command -v cargo >/dev/null 2>&1; then
  die "cargo is required"
fi

MANIFEST="$ROOT_DIR/Cargo.toml"
[ -f "$MANIFEST" ] || die "no Cargo.toml at $MANIFEST"

# Current version is the first `version = "..."` line, which belongs to
# [workspace.package] in this workspace.
CURRENT_VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$MANIFEST" | head -n 1)"
[ -n "$CURRENT_VERSION" ] || die "could not read the workspace version from Cargo.toml"

if ! python3 - "$CURRENT_VERSION" "$VERSION" <<'PY'
import re
import sys


def parse(text):
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?", text)
    if not match:
        sys.exit(2)
    major, minor, patch, suffix = match.groups()
    return (int(major), int(minor), int(patch)), suffix or ""


try:
    current, _ = parse(sys.argv[1])
    wanted, _ = parse(sys.argv[2])
except SystemExit:
    sys.exit(2)
sys.exit(0 if wanted > current else 1)
PY
then
  die "--version $VERSION must be greater than the current version $CURRENT_VERSION"
fi

info "repository: $REPO"
info "version:    $CURRENT_VERSION -> $VERSION"
info "phase:      $PHASE"
info "dry-run:    $([ "$DRY_RUN" -eq 1 ] && echo yes || echo no)"

# ---------------------------------------------------------------- [1/5] preflight
step "1" "Preflight (branch, working tree, versions)"
if [ "$DRY_RUN" -eq 0 ]; then
  branch="$(git -C "$ROOT_DIR" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
  if [ "$branch" != "$ALLOW_BRANCH" ]; then
    die "on branch '$branch', expected '$ALLOW_BRANCH' (use --allow-branch to override)"
  fi
  info "branch: $branch"

  if [ "$ALLOW_DIRTY" -eq 0 ]; then
    dirty="$(git -C "$ROOT_DIR" status --porcelain)"
    if [ -n "$dirty" ]; then
      printf '%s\n' "$dirty" | sed 's/^/  /' >&2
      die "working tree is not clean (commit first, or pass --allow-dirty for the bump-only phase)"
    fi
    info "working tree: clean"
  else
    warn "working tree may be dirty (--allow-dirty)"
  fi
else
  printf '[dry-run] would check: branch == %s, working tree clean\n' "$ALLOW_BRANCH"
fi

# ---------------------------------------------------------------- [2/5] bump
if [ "$PHASE" = "prepare" ] || [ "$PHASE" = "all" ]; then
  step "2" "Bump Cargo.toml ($CURRENT_VERSION -> $VERSION) and update Cargo.lock"
  bump_manifest() {
    python3 - "$MANIFEST" "$VERSION" <<'PY'
import re
import sys

path, version = sys.argv[1], sys.argv[2]
with open(path, encoding="utf-8") as handle:
    lines = handle.readlines()

in_section = False
replaced = False
for index, line in enumerate(lines):
    stripped = line.strip()
    if stripped.startswith("["):
        in_section = stripped == "[workspace.package]"
        continue
    if in_section and re.match(r'^version\s*=\s*"[^"]*"\s*$', stripped):
        lines[index] = 'version = "%s"\n' % version
        replaced = True
        break

if not replaced:
    print("release.sh: error: could not find version in [workspace.package]", file=sys.stderr)
    sys.exit(1)
with open(path, "w", encoding="utf-8") as handle:
    handle.writelines(lines)
print("bumped [workspace.package].version to %s" % version)
PY
  }

  if [ "$DRY_RUN" -eq 1 ]; then
    printf '[dry-run] would set [workspace.package].version = "%s" in Cargo.toml\n' "$VERSION"
    printf '[dry-run] would run: CARGO_INCREMENTAL=0 cargo check --workspace   # updates Cargo.lock\n'
  else
    run bump_manifest
    (cd "$ROOT_DIR" && CARGO_INCREMENTAL=0 cargo check --workspace)
    info "Cargo.lock updated by cargo check"
  fi
else
  step "2" "Bump Cargo.toml (skipped in --phase $PHASE)"
  info "assuming Cargo.toml is already at $VERSION"
fi

# ---------------------------------------------------------------- [3/5] sync
if [ "$PHASE" = "publish" ] || [ "$PHASE" = "all" ]; then
  step "3" "Sync package-manager manifests to v$VERSION"
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '[dry-run] would run: scripts/sync-package-manifests.sh --version %s\n' "$VERSION"
  else
    if ! bash "$ROOT_DIR/scripts/sync-package-manifests.sh" --version "$VERSION"; then
      die "manifest sync failed — does the v$VERSION release (with SHA256SUMS.txt) exist yet? push the tag first"
    fi
  fi
else
  step "3" "Sync package-manager manifests (needs the release; deferred)"
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '[dry-run] would run after the tag exists: scripts/sync-package-manifests.sh --version %s\n' "$VERSION"
  fi
  info "after the tag is pushed and the release exists:"
  info "  scripts/release.sh --version $VERSION --phase publish"
fi

# ---------------------------------------------------------------- [4/5] verify
if [ "$PHASE" = "publish" ] || [ "$PHASE" = "all" ]; then
  step "4" "Verify the packaging pipeline"
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '[dry-run] would run: CARGO_INCREMENTAL=0 scripts/verify-packaging.sh --skip-build\n'
  else
    (cd "$ROOT_DIR" && CARGO_INCREMENTAL=0 bash scripts/verify-packaging.sh --skip-build)
  fi
else
  step "4" "verify-packaging (needs synced manifests; deferred to --phase publish)"
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '[dry-run] would run after the release exists: CARGO_INCREMENTAL=0 scripts/verify-packaging.sh --skip-build\n'
  fi
fi

# ---------------------------------------------------------------- [5/5] next steps
step "5" "Next steps (commands are printed, never executed)"
cat <<EOF

  # 1) commit the bump (and, in --phase publish, the synced manifests + docs)
  git add Cargo.toml Cargo.lock packaging docs
  git commit -m "Release v${VERSION}: <one-line summary>"

  # 2) tag and push (this triggers .github/workflows/release.yml)
  git tag -a v${VERSION} -m "OpenFlex ${VERSION}"
  git push origin ${ALLOW_BRANCH}
  git push origin v${VERSION}

  # 3) after the release workflow finishes, finish the publish phase
  scripts/release.sh --version ${VERSION} --phase publish

  # 4) post-release checks
  scripts/verify-ghcr.sh --tags ${VERSION},latest --expect-latest ${VERSION}
  OPENFLEX_REPO=${REPO} scripts/verify-install.sh --version ${VERSION}
  # then update docs/verification/install-matrix.md + docs/VERIFICATION.md with the result
EOF

printf '\nRESULT: PASS%s\n' "$([ "$DRY_RUN" -eq 1 ] && echo " (dry-run)" || echo "")"
