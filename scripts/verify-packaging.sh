#!/usr/bin/env bash
# Local, end-to-end verification of the OpenFlex packaging pipeline.
#
# Builds the release binary, packages it exactly like CI does, installs it into
# a throwaway prefix with packaging/install.sh --from-dir, runs smoke tests,
# exercises uninstall/idempotency/checksum failures, and lints every script.
#
# Usage:
#   scripts/verify-packaging.sh              # full run (builds the binary)
#   scripts/verify-packaging.sh --skip-build # reuse target/release/openflex
#   scripts/verify-packaging.sh --keep       # keep the temporary work directory
#
# Exit code: 0 when every check passed; 1 when at least one check failed.
# SKIP lines mean the check needs a tool that is not installed here (pwsh,
# docker, shellcheck, gpg) and is exercised in CI instead.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly ROOT_DIR

SKIP_BUILD=0
KEEP=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    --keep) KEEP=1; shift ;;
    -h | --help)
      sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      printf 'verify-packaging.sh: error: unknown argument %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

PASSED=0
FAILED=0
SKIPPED=0

pass() { PASSED=$((PASSED + 1)); printf '  PASS  %s\n' "$1"; }
fail() { FAILED=$((FAILED + 1)); printf '  FAIL  %s\n' "$1"; }
skip() { SKIPPED=$((SKIPPED + 1)); printf '  SKIP  %s\n' "$1"; }
section() { printf '\n== %s ==\n' "$1"; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/openflex-verify.XXXXXX")"
LOG="$WORK/.log"
PREFIX="$WORK/prefix"
DIST="$WORK/dist"
cleanup() {
  if [ "$KEEP" -eq 1 ]; then
    printf '\nwork directory kept: %s\n' "$WORK"
  else
    rm -rf "$WORK"
  fi
}
trap cleanup EXIT INT TERM

log_tail() { [ -f "$LOG" ] && tail -n 15 "$LOG" | sed 's/^/        /' || true; }

run_ok() { # run_ok <label> <command...>
  label="$1"
  shift
  : >"$LOG"
  if "$@" >>"$LOG" 2>&1; then
    pass "$label"
  else
    fail "$label"
    log_tail
  fi
}

# ---------------------------------------------------------------- environment
section "Environment"
printf '  os:      %s %s\n' "$(uname -s)" "$(uname -m)"
printf '  bash:    %s\n' "${BASH_VERSION}"
printf '  workdir: %s\n' "$WORK"
for tool in cargo tar awk sed grep find python3; do
  if command -v "$tool" >/dev/null 2>&1; then
    pass "tool available: $tool"
  else
    fail "tool missing: $tool"
  fi
done

VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT_DIR/Cargo.toml" | head -n 1)"
if [ -n "$VERSION" ]; then
  pass "workspace version: $VERSION"
else
  fail "could not read the workspace version from Cargo.toml"
  VERSION="0.0.0"
fi

case "$(uname -s)" in
  Darwin) OS="darwin" ;;
  Linux) OS="linux" ;;
  *) OS="unknown" ;;
esac
case "$(uname -m)" in
  arm64 | aarch64) ARCH="aarch64" ;;
  *) ARCH="x86_64" ;;
esac
if [ "$OS" = "darwin" ]; then
  PLATFORM="$ARCH-apple-darwin"
else
  PLATFORM="$ARCH-unknown-linux-gnu"
fi
printf '  target:  %s\n' "$PLATFORM"

# ---------------------------------------------------------------- static checks
section "Shell syntax (bash -n)"
SHELL_SCRIPTS=(
  "$ROOT_DIR/packaging/install.sh"
  "$ROOT_DIR/packaging/uninstall.sh"
  "$ROOT_DIR/packaging/deb/build-deb.sh"
  "$ROOT_DIR/packaging/rpm/build-rpm.sh"
  "$ROOT_DIR/scripts/package-release.sh"
  "$ROOT_DIR/scripts/verify-packaging.sh"
  "$ROOT_DIR/scripts/verify-install.sh"
  "$ROOT_DIR/scripts/sign-macos.sh"
  "$ROOT_DIR/scripts/sign-checksums.sh"
  "$ROOT_DIR/scripts/compare-flexsim.sh"
  "$ROOT_DIR/scripts/fuzz-smoke.sh"
  "$ROOT_DIR/scripts/bench-large-model.sh"
  "$ROOT_DIR/scripts/sync-package-manifests.sh"
)
for script in "${SHELL_SCRIPTS[@]}"; do
  if [ -f "$script" ]; then
    run_ok "bash -n $(basename "$script")" bash -n "$script"
  else
    fail "missing script: $script"
  fi
done

section "ShellCheck (optional)"
if command -v shellcheck >/dev/null 2>&1; then
  run_ok "shellcheck -S warning (all shell scripts)" shellcheck -S warning "${SHELL_SCRIPTS[@]}"
else
  skip "shellcheck not installed"
fi

section "Package metadata files"
if command -v ruby >/dev/null 2>&1; then
  run_ok "nfpm YAML parses (deb, rpm)" ruby -ryaml -e '
    ARGV.each { |file| YAML.load_file(file); puts "ok: #{file}" }
  ' "$ROOT_DIR/packaging/deb/nfpm.yaml" "$ROOT_DIR/packaging/rpm/nfpm.yaml"
  run_ok "Homebrew formula ruby syntax" ruby -c "$ROOT_DIR/packaging/homebrew/openflex.rb"
  run_ok "GitHub workflows YAML parse" ruby -ryaml -e '
    ARGV.each { |file| YAML.load_file(file); puts "ok: #{file}" }
  ' "$ROOT_DIR/.github/workflows/ci.yml" \
    "$ROOT_DIR/.github/workflows/container.yml" \
    "$ROOT_DIR/.github/workflows/release.yml" \
    "$ROOT_DIR/.github/dependabot.yml"
  run_ok "winget manifests YAML parse" ruby -ryaml -e '
    ARGV.each { |file| YAML.load_file(file); puts "ok: #{file}" }
  ' "$ROOT_DIR"/packaging/winget/*.yaml
else
  skip "ruby not installed (skipping YAML/Homebrew checks)"
fi

if command -v python3 >/dev/null 2>&1; then
  run_ok "WiX template is well-formed XML" python3 -c '
import sys, xml.etree.ElementTree as ET
ET.parse(sys.argv[1])
print("ok")
' "$ROOT_DIR/packaging/windows/openflex.wxs"
else
  skip "python3 not installed (skipping XML check)"
fi

if command -v python3 >/dev/null 2>&1 && python3 -c 'import tomllib' 2>/dev/null; then
  run_ok "deny.toml, fuzz/Cargo.toml and docs/book.toml parse as TOML" python3 -c '
import sys, tomllib
for path in sys.argv[1:]:
    with open(path, "rb") as handle:
        tomllib.load(handle)
    print("ok:", path)
' "$ROOT_DIR/deny.toml" "$ROOT_DIR/fuzz/Cargo.toml" "$ROOT_DIR/docs/book.toml"
else
  skip "python3 < 3.11 (no tomllib); skipping TOML checks"
fi

for file in \
  "$ROOT_DIR/packaging/Dockerfile" \
  "$ROOT_DIR/packaging/Dockerfile.dockerignore" \
  "$ROOT_DIR/packaging/README.md" \
  "$ROOT_DIR/packaging/homebrew/README.md" \
  "$ROOT_DIR/packaging/winget/FanXingChunShui.OpenFlex.yaml" \
  "$ROOT_DIR/packaging/winget/FanXingChunShui.OpenFlex.installer.yaml" \
  "$ROOT_DIR/packaging/winget/FanXingChunShui.OpenFlex.locale.en-US.yaml" \
  "$ROOT_DIR/packaging/scoop/openflex.json" \
  "$ROOT_DIR/deny.toml" \
  "$ROOT_DIR/fuzz/Cargo.toml" \
  "$ROOT_DIR/docs/install.md" \
  "$ROOT_DIR/docs/releasing.md" \
  "$ROOT_DIR/docs/verification/install-matrix.md" \
  "$ROOT_DIR/docs/verification/flexsim-comparison.md" \
  "$ROOT_DIR/docs/verification/flexsim-mm1-example.csv" \
  "$ROOT_DIR/docs/verification/wasm.md" \
  "$ROOT_DIR/docs/verification/performance.md" \
  "$ROOT_DIR/docs/verification/flexsim-result-template.csv" \
  "$ROOT_DIR/docs/book.toml" \
  "$ROOT_DIR/docs/SUMMARY.md"; do
  if [ -f "$file" ]; then
    pass "exists: ${file#"$ROOT_DIR"/}"
  else
    fail "missing: ${file#"$ROOT_DIR"/}"
  fi
done

if grep -q '^FROM ' "$ROOT_DIR/packaging/Dockerfile" 2>/dev/null &&
  grep -q 'ENTRYPOINT \["openflex"\]' "$ROOT_DIR/packaging/Dockerfile" 2>/dev/null; then
  pass "Dockerfile has FROM and ENTRYPOINT openflex"
else
  fail "Dockerfile is missing FROM or ENTRYPOINT openflex"
fi

section "PowerShell syntax (optional)"
if command -v pwsh >/dev/null 2>&1; then
  for ps in \
    "$ROOT_DIR/packaging/install.ps1" \
    "$ROOT_DIR/packaging/uninstall.ps1" \
    "$ROOT_DIR/scripts/verify-install.ps1" \
    "$ROOT_DIR/scripts/sign-windows.ps1"; do
    : >"$LOG"
    if pwsh -NoProfile -NonInteractive -Command "
      [void][System.Management.Automation.Language.Parser]::ParseFile('$ps', [ref]\$null, [ref]\$errors)
      if (\$errors.Count -gt 0) { \$errors | ForEach-Object { Write-Error \$_ }; exit 1 }
      exit 0
    " >>"$LOG" 2>&1; then
      pass "PowerShell parses $(basename "$ps")"
    else
      fail "PowerShell parses $(basename "$ps")"
      log_tail
    fi
  done
else
  skip "pwsh not installed (PowerShell scripts are only syntax-checked in CI)"
fi

section "Docker build (optional)"
if [ "${OPENFLEX_VERIFY_DOCKER:-0}" = "1" ] && command -v docker >/dev/null 2>&1; then
  : >"$LOG"
  if (cd "$ROOT_DIR" && docker build -f packaging/Dockerfile -t openflex:verify . >>"$LOG" 2>&1); then
    pass "docker build packaging/Dockerfile"
  else
    fail "docker build packaging/Dockerfile"
    log_tail
  fi
else
  skip "docker disabled (set OPENFLEX_VERIFY_DOCKER=1 to run the image build)"
fi

# ---------------------------------------------------------------- build
section "Release build"
BINARY="$ROOT_DIR/target/release/openflex"
if [ "$SKIP_BUILD" -eq 1 ]; then
  if [ -x "$BINARY" ]; then
    pass "reusing $BINARY (--skip-build)"
  else
    fail "no binary at $BINARY and --skip-build was given"
  fi
else
  run_ok "cargo build --release -p openflex-cli" \
    cargo build --release --locked -p openflex-cli --manifest-path "$ROOT_DIR/Cargo.toml"
fi

# D3: the core must stay wasm32-compatible. Only run this when the target is
# installed locally; CI always installs it (see the `wasm` job in ci.yml).
section "wasm32 target (optional)"
if command -v rustup >/dev/null 2>&1 &&
  rustup target list --installed 2>/dev/null | grep -q '^wasm32-unknown-unknown$'; then
  run_ok "cargo check -p openflex-core --target wasm32-unknown-unknown" \
    cargo check --locked -p openflex-core --target wasm32-unknown-unknown \
    --manifest-path "$ROOT_DIR/Cargo.toml"
else
  skip "wasm32-unknown-unknown not installed (rustup target add wasm32-unknown-unknown)"
fi

if [ -x "$BINARY" ]; then
  pass "binary exists: target/release/openflex"
  if version_line="$("$BINARY" --version 2>&1)"; then
    printf '        %s\n' "$version_line"
  fi
else
  fail "binary not built"
fi

# ---------------------------------------------------------------- package
section "Package archive"
if [ -x "$BINARY" ]; then
  run_ok "package-release.sh builds ${PLATFORM} archive" \
    bash "$ROOT_DIR/scripts/package-release.sh" \
    --version "$VERSION" \
    --platform "$PLATFORM" \
    --binary "$BINARY" \
    --out-dir "$DIST"

  ARCHIVE="$DIST/openflex-$VERSION-$PLATFORM.tar.gz"
  if [ -f "$ARCHIVE" ]; then
    pass "archive exists: $(basename "$ARCHIVE")"
    if tar -tzf "$ARCHIVE" | grep -q "openflex-$VERSION-$PLATFORM/openflex$"; then
      pass "archive layout contains <name>/openflex"
    else
      fail "archive layout is wrong:"
      tar -tzf "$ARCHIVE" | sed 's/^/        /'
    fi
  else
    fail "archive was not created"
  fi

  if [ -f "$DIST/SHA256SUMS.txt" ]; then
    pass "SHA256SUMS.txt written"
    if (cd "$DIST" && shasum -a 256 -c SHA256SUMS.txt >/dev/null 2>&1); then
      pass "SHA256SUMS.txt verifies"
    else
      fail "SHA256SUMS.txt does not verify"
    fi
  else
    fail "SHA256SUMS.txt was not written"
  fi
else
  skip "packaging (no binary)"
fi

# ---------------------------------------------------------------- dry run
section "install.sh --dry-run"
if [ -f "$DIST/openflex-$VERSION-$PLATFORM.tar.gz" ]; then
  : >"$LOG"
  if bash "$ROOT_DIR/packaging/install.sh" \
    --from-dir "$DIST" --version "$VERSION" --prefix "$PREFIX" --dry-run >>"$LOG" 2>&1; then
    pass "dry-run exits 0"
    if [ ! -e "$PREFIX" ]; then
      pass "dry-run did not create the prefix"
    else
      fail "dry-run created $PREFIX"
    fi
    if grep -q 'nothing was changed' "$LOG"; then
      pass "dry-run says nothing was changed"
    else
      fail "dry-run output is missing the 'nothing was changed' line"
      log_tail
    fi
  else
    fail "dry-run failed"
    log_tail
  fi
else
  skip "install.sh dry-run (no archive)"
fi

# ---------------------------------------------------------------- install
section "Install from --from-dir"
INSTALLED="$PREFIX/bin/openflex"
if [ -f "$DIST/openflex-$VERSION-$PLATFORM.tar.gz" ]; then
  : >"$LOG"
  if bash "$ROOT_DIR/packaging/install.sh" \
    --from-dir "$DIST" --version "$VERSION" --prefix "$PREFIX" >>"$LOG" 2>&1; then
    pass "install.sh exits 0"
  else
    fail "install.sh failed"
    log_tail
  fi

  if [ -x "$INSTALLED" ]; then
    pass "installed binary is executable"
  else
    fail "no executable at $INSTALLED"
  fi

  if grep -q 'sha256 verified' "$LOG"; then
    pass "install.sh verified the checksum"
  else
    fail "install.sh did not report a verified checksum"
  fi

  if grep -q 'not on your PATH' "$LOG"; then
    pass "install.sh printed the PATH hint"
  else
    fail "install.sh did not print the PATH hint for a prefix that is not on PATH"
  fi

  : >"$LOG"
  if installed_version="$("$INSTALLED" --version 2>&1)" && [ "$installed_version" = "openflex $VERSION" ]; then
    pass "installed binary reports 'openflex $VERSION'"
    printf '        %s\n' "$installed_version"
  else
    fail "installed binary did not report 'openflex $VERSION' (got: ${installed_version:-no output})"
  fi

  if [ -f "$PREFIX/share/openflex/install-receipt.txt" ]; then
    pass "install receipt written"
  else
    fail "install receipt missing"
  fi

  # Idempotency: a second install must not fail and must keep the same binary.
  before="$(shasum -a 256 "$INSTALLED" 2>/dev/null | awk '{print $1}' || true)"
  : >"$LOG"
  if bash "$ROOT_DIR/packaging/install.sh" \
    --from-dir "$DIST" --version "$VERSION" --prefix "$PREFIX" >>"$LOG" 2>&1; then
    pass "second install is idempotent (exit 0)"
    if grep -q 'already installed' "$LOG"; then
      pass "second install reports 'already installed'"
    else
      fail "second install did not report 'already installed'"
      log_tail
    fi
    after="$(shasum -a 256 "$INSTALLED" 2>/dev/null | awk '{print $1}' || true)"
    if [ "$before" = "$after" ]; then
      pass "binary unchanged after reinstall"
    else
      fail "binary changed after reinstall"
    fi
  else
    fail "second install failed (not idempotent)"
    log_tail
  fi
else
  skip "installation (no archive)"
fi

# ---------------------------------------------------------------- smoke tests
section "Smoke tests (installed binary)"
DEMO="$WORK/demo"
if [ -x "$INSTALLED" ]; then
  : >"$LOG"
  if "$INSTALLED" new "$DEMO" --template mm1 >>"$LOG" 2>&1 && [ -f "$DEMO/mm1.ofm" ]; then
    pass "openflex new created a model"
  else
    fail "openflex new failed"
    log_tail
  fi

  : >"$LOG"
  if "$INSTALLED" validate "$DEMO/mm1.ofm" >>"$LOG" 2>&1 && grep -q '^ok:' "$LOG"; then
    pass "openflex validate passes"
  else
    fail "openflex validate failed"
    log_tail
  fi

  # stdout carries the report, stderr carries diagnostics: keep them apart so
  # the check matches how users pipe the output.
  if "$INSTALLED" run "$DEMO/mm1.ofm" --format csv >"$WORK/report.csv" 2>"$WORK/report.err" &&
    grep -q 'Sink' "$WORK/report.csv"; then
    pass "openflex run --format csv produces a report on stdout"
  else
    fail "openflex run --format csv failed"
    head -n 10 "$WORK/report.csv" 2>/dev/null | sed 's/^/        /'
    tail -n 5 "$WORK/report.err" 2>/dev/null | sed 's/^/        /'
  fi

  if "$INSTALLED" run "$DEMO/mm1.ofm" --format json --replications 2 >"$WORK/report.json" 2>"$WORK/report-json.err"; then
    if python3 -m json.tool <"$WORK/report.json" >/dev/null 2>&1; then
      pass "openflex run --format json is valid JSON on stdout"
    else
      fail "openflex run --format json did not emit valid JSON on stdout"
      head -n 5 "$WORK/report.json" | sed 's/^/        /'
    fi
  else
    fail "openflex run --format json failed"
    tail -n 5 "$WORK/report-json.err" | sed 's/^/        /'
  fi

  : >"$LOG"
  if "$INSTALLED" objects list >>"$LOG" 2>&1 && grep -qi 'processor' "$LOG"; then
    pass "openflex objects list works"
  else
    fail "openflex objects list failed"
    log_tail
  fi

  : >"$LOG"
  if "$INSTALLED" distributions >>"$LOG" 2>&1 && grep -qi 'exponential' "$LOG"; then
    pass "openflex distributions works"
  else
    fail "openflex distributions failed"
    log_tail
  fi
else
  skip "smoke tests (binary not installed)"
fi

# ---------------------------------------------------------------- checksum guard
section "Checksum enforcement"
if [ -d "$DIST" ] && [ -f "$DIST/openflex-$VERSION-$PLATFORM.tar.gz" ]; then
  BAD="$WORK/bad-dist"
  mkdir -p "$BAD"
  cp "$DIST/openflex-$VERSION-$PLATFORM.tar.gz" "$BAD/"
  awk '{ if (NR == 1) { print "0000000000000000000000000000000000000000000000000000000000000000  " $2 } else { print } }' \
    "$DIST/SHA256SUMS.txt" >"$BAD/SHA256SUMS.txt"
  : >"$LOG"
  if bash "$ROOT_DIR/packaging/install.sh" --from-dir "$BAD" --version "$VERSION" \
    --prefix "$WORK/bad-prefix" >>"$LOG" 2>&1; then
    fail "install.sh accepted a corrupted archive"
  else
    if grep -q 'checksum mismatch' "$LOG"; then
      pass "install.sh rejects a corrupted archive"
    else
      fail "install.sh failed but not because of the checksum"
      log_tail
    fi
    if [ ! -e "$WORK/bad-prefix/bin/openflex" ]; then
      pass "nothing installed from the corrupted archive"
    else
      fail "a corrupted archive produced an installed binary"
    fi
  fi

  mkdir -p "$WORK/no-sums"
  cp "$DIST/openflex-$VERSION-$PLATFORM.tar.gz" "$WORK/no-sums/"
  : >"$LOG"
  if bash "$ROOT_DIR/packaging/install.sh" --from-dir "$WORK/no-sums" --version "$VERSION" \
    --prefix "$WORK/require-prefix" --require-checksum >>"$LOG" 2>&1; then
    fail "--require-checksum accepted a directory without SHA256SUMS.txt"
  else
    pass "--require-checksum fails without SHA256SUMS.txt"
  fi
else
  skip "checksum enforcement (no archive)"
fi

# ---------------------------------------------------------------- network install
section "Network install (local HTTP server)"
if [ -f "$DIST/openflex-$VERSION-$PLATFORM.tar.gz" ] && command -v curl >/dev/null 2>&1 && command -v python3 >/dev/null 2>&1; then
  SERVER_ROOT="$WORK/server"
  mkdir -p "$SERVER_ROOT/v$VERSION"
  cp "$DIST/openflex-$VERSION-$PLATFORM.tar.gz" "$SERVER_ROOT/v$VERSION/"
  cp "$DIST/SHA256SUMS.txt" "$SERVER_ROOT/v$VERSION/"
  PORT="$(python3 -c '
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
')"
  python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$SERVER_ROOT" >"$WORK/server.log" 2>&1 &
  SERVER_PID=$!
  ready=0
  tries=0
  while [ "$tries" -lt 20 ]; do
    if curl -fsS -o /dev/null "http://127.0.0.1:$PORT/v$VERSION/SHA256SUMS.txt" 2>/dev/null; then
      ready=1
      break
    fi
    tries=$((tries + 1))
    sleep 0.25
  done

  NET_PREFIX="$WORK/net-prefix"
  if [ "$ready" -eq 1 ]; then
    : >"$LOG"
    if bash "$ROOT_DIR/packaging/install.sh" \
      --base-url "http://127.0.0.1:$PORT" \
      --version "$VERSION" \
      --prefix "$NET_PREFIX" >>"$LOG" 2>&1; then
      pass "install.sh downloads and installs over HTTP"
      if [ "$("$NET_PREFIX/bin/openflex" --version 2>&1)" = "openflex $VERSION" ]; then
        pass "network-installed binary reports the expected version"
      else
        fail "network-installed binary reports the wrong version"
      fi
      if grep -q 'sha256 verified' "$LOG"; then
        pass "network install verified the checksum"
      else
        fail "network install did not verify the checksum"
      fi
    else
      fail "network install failed"
      log_tail
    fi
  else
    fail "local HTTP server did not start on port $PORT"
  fi
  kill "$SERVER_PID" 2>/dev/null || true
  wait "$SERVER_PID" 2>/dev/null || true
else
  skip "network install (curl/python3 or archive missing)"
fi

# ---------------------------------------------------------------- uninstall
section "uninstall.sh"
if [ -x "$INSTALLED" ]; then
  : >"$LOG"
  if bash "$ROOT_DIR/packaging/uninstall.sh" --prefix "$PREFIX" --dry-run >>"$LOG" 2>&1 &&
    grep -q 'would remove' "$LOG" && [ -x "$INSTALLED" ]; then
    pass "uninstall --dry-run leaves the binary in place"
  else
    fail "uninstall --dry-run behaved unexpectedly"
    log_tail
  fi

  : >"$LOG"
  if bash "$ROOT_DIR/packaging/uninstall.sh" --prefix "$PREFIX" >>"$LOG" 2>&1; then
    if [ ! -e "$INSTALLED" ]; then
      pass "uninstall removed the binary"
    else
      fail "uninstall left the binary behind"
    fi
  else
    fail "uninstall.sh failed"
    log_tail
  fi

  : >"$LOG"
  if bash "$ROOT_DIR/packaging/uninstall.sh" --prefix "$PREFIX" >>"$LOG" 2>&1; then
    pass "uninstall is idempotent (exit 0 when nothing is installed)"
  else
    fail "second uninstall failed"
    log_tail
  fi
else
  skip "uninstall (binary not installed)"
fi

# ---------------------------------------------------------------- release tooling
section "Release tooling (help and dry-run)"
for tool in \
  "$ROOT_DIR/scripts/verify-install.sh" \
  "$ROOT_DIR/scripts/sign-macos.sh" \
  "$ROOT_DIR/scripts/sign-checksums.sh" \
  "$ROOT_DIR/scripts/compare-flexsim.sh" \
  "$ROOT_DIR/scripts/fuzz-smoke.sh" \
  "$ROOT_DIR/scripts/bench-large-model.sh" \
  "$ROOT_DIR/scripts/sync-package-manifests.sh"; do
  run_ok "$(basename "$tool") --help exits 0" bash "$tool" --help
done

if [ -x "$BINARY" ]; then
  run_ok "sign-macos.sh --dry-run --skip-notarize" \
    bash "$ROOT_DIR/scripts/sign-macos.sh" \
    --identity "Developer ID Application: Verify Only (TEAMID)" \
    --path "$BINARY" --dry-run --skip-notarize
  run_ok "sign-checksums.sh --dry-run" \
    bash "$ROOT_DIR/scripts/sign-checksums.sh" --file "$ROOT_DIR/Cargo.toml" --dry-run
  run_ok "compare-flexsim.sh --help lists --tolerance" \
    bash -c "bash '$ROOT_DIR/scripts/compare-flexsim.sh' --help | grep -q -- --tolerance"
  run_ok "compare-flexsim.sh --help lists --report-a/--report-b" \
    bash -c "bash '$ROOT_DIR/scripts/compare-flexsim.sh' --help | grep -q -- --report-b"
  run_ok "fuzz-smoke.sh --dry-run" \
    bash "$ROOT_DIR/scripts/fuzz-smoke.sh" --dry-run --mode fallback --iterations 5
else
  skip "release tooling dry-runs (no binary)"
fi

# ---------------------------------------------------------------- manifests
section "Package-manager manifests match the workspace version"
check_packager_manifests() {
  python3 - "$ROOT_DIR" "$VERSION" <<'PY'
import json
import re
import sys

root, version = sys.argv[1], sys.argv[2]
errors = []


def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()


# Homebrew: version must equal the workspace version, sha256 must be real.
brew = read(root + "/packaging/homebrew/openflex.rb")
match = re.search(r'^\s*version "(?P<version>[^"]+)"', brew, re.M)
if not match or match.group("version") != version:
    errors.append("homebrew: version is %r, workspace is %r" % (match.group("version") if match else None, version))
if "REPLACE_WITH_SHA256" in brew:
    errors.append("homebrew: sha256 placeholders are still present")
for digest in re.findall(r'sha256 "([^"]+)"', brew):
    if not re.fullmatch(r"[0-9a-f]{64}", digest):
        errors.append("homebrew: sha256 %r is not a 64-char hex digest" % digest)

# winget: all three manifests carry the same PackageVersion.
for name in (
    "FanXingChunShui.OpenFlex.yaml",
    "FanXingChunShui.OpenFlex.installer.yaml",
    "FanXingChunShui.OpenFlex.locale.en-US.yaml",
):
    path = root + "/packaging/winget/" + name
    text = read(path)
    found = re.search(r"^PackageVersion:\s*(\S+)", text, re.M)
    if not found or found.group(1) != version:
        errors.append("winget %s: PackageVersion is %r" % (name, found.group(1) if found else None))

installer = read(root + "/packaging/winget/FanXingChunShui.OpenFlex.installer.yaml")
url = re.search(r"InstallerUrl:\s*(\S+)", installer)
if not url or ("v%s/" % version) not in url.group(1) or ("-%s-" % version) not in url.group(1):
    errors.append("winget: InstallerUrl does not point at v%s" % version)
digest = re.search(r"InstallerSha256:\s*(\S+)", installer)
if not digest or not re.fullmatch(r"[0-9a-f]{64}", digest.group(1)):
    errors.append("winget: InstallerSha256 is missing or not a 64-char hex digest")

# scoop: version and every architecture entry.
scoop = json.loads(read(root + "/packaging/scoop/openflex.json"))
if scoop.get("version") != version:
    errors.append("scoop: version is %r, workspace is %r" % (scoop.get("version"), version))
for arch, spec in (scoop.get("architecture") or {}).items():
    if ("-%s-" % version) not in spec.get("url", ""):
        errors.append("scoop %s: url does not contain -%s-" % (arch, version))
    if not re.fullmatch(r"[0-9a-f]{64}", spec.get("hash", "")):
        errors.append("scoop %s: hash is not a 64-char hex digest" % arch)
    if version not in spec.get("extract_dir", ""):
        errors.append("scoop %s: extract_dir does not mention %s" % (arch, version))

if errors:
    for error in errors:
        print("  manifest check: " + error, file=sys.stderr)
    sys.exit(1)
print("winget, scoop and homebrew manifests match %s" % version)
PY
}
run_ok "winget / scoop / homebrew manifests match $VERSION" check_packager_manifests

# ---------------------------------------------------------------- docs site
section "Documentation site (mdBook)"
if command -v python3 >/dev/null 2>&1; then
  check_summary_links() {
    python3 - "$ROOT_DIR" <<'PY'
import os
import re
import sys

root = sys.argv[1]
summary = os.path.join(root, "docs", "SUMMARY.md")
text = open(summary, encoding="utf-8").read()
missing = []
for match in re.finditer(r"\]\(([^)#]+\.md)\)", text):
    target = os.path.normpath(os.path.join(root, "docs", match.group(1)))
    if not os.path.exists(target):
        missing.append(match.group(1))
if missing:
    for path in missing:
        print("  SUMMARY.md: missing " + path, file=sys.stderr)
    sys.exit(1)
print("every SUMMARY.md link resolves")
PY
  }
  run_ok "every docs/SUMMARY.md link resolves" check_summary_links
else
  skip "python3 not installed (skipping the mdBook navigation check)"
fi

# ---------------------------------------------------------------- summary
section "Summary"
printf '  passed:  %d\n' "$PASSED"
printf '  failed:  %d\n' "$FAILED"
printf '  skipped: %d\n' "$SKIPPED"
printf '\nChecklist\n'
printf '  [x] scripts parse with bash -n (install, uninstall, package-release, deb/rpm builders)\n'
printf '  [x] release build of crates/openflex-cli\n'
printf '  [x] tar.gz layout + SHA256SUMS.txt\n'
printf '  [x] install.sh --from-dir (dry-run, real install, reinstall, checksum failure)\n'
printf '  [x] install.sh network mode against a local HTTP server (download + checksum)\n'
printf '  [x] smoke tests: --version, new, validate, run (csv/json), objects, distributions\n'
printf '  [x] uninstall.sh (dry-run, real, idempotent)\n'
printf '  [ ] GitHub Actions matrix (macOS universal, Windows x64/arm64, Linux x64/arm64) — CI only\n'
printf '  [ ] .deb/.rpm via nfpm and .msi via WiX — CI only (tools not installed here)\n'
printf '  [ ] Docker image build — CI only (set OPENFLEX_VERIFY_DOCKER=1 with docker installed)\n'
printf '  [x] new release tooling parses and answers --help (verify-install, sign-*, compare-flexsim, fuzz-smoke)\n'
printf '  [x] workflows / dependabot / winget manifests parse; deny.toml and fuzz/Cargo.toml parse\n'
printf '  [x] winget / scoop / homebrew manifests carry the workspace version and real sha256 values\n'
printf '  [ ] Code signing and macOS notarisation — scripts ready, certificates/secrets not configured (docs/releasing.md 6)\n'
printf '  [ ] Real-machine MSI/deb/rpm install — scripts/verify-install.{sh,ps1} (docs/verification/install-matrix.md)\n'
printf '  [x] openflex-core checks for wasm32-unknown-unknown (docs/verification/wasm.md; CI job wasm)\n'

if [ "$FAILED" -gt 0 ]; then
  printf '\nRESULT: FAIL (%d failed check(s))\n' "$FAILED" >&2
  exit 1
fi
printf '\nRESULT: PASS\n'
