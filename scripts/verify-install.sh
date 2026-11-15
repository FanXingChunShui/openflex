#!/usr/bin/env bash
# Real-machine install verification for OpenFlex (macOS and Linux).
#
# Unlike scripts/verify-packaging.sh (which builds the binary from source and
# packages it), this script consumes the *published* artifacts exactly as a
# user would: a release archive from a local directory (`--from-dir`) or from
# the GitHub release (`--version`). It installs into a throwaway prefix, runs
# `--version`, `new`, `validate` and `run`, then uninstalls and proves that
# nothing was left behind.
#
# On Linux the script can additionally check a `.deb` (installed into a private
# dpkg root, no root privileges needed) and a `.rpm` (extracted with rpm2cpio).
#
# Usage:
#   scripts/verify-install.sh --from-dir dist
#   scripts/verify-install.sh --version 0.1.0 --platform universal-apple-darwin
#   scripts/verify-install.sh --from-dir dist --deb dist/openflex_0.1.0_amd64.deb
#   scripts/verify-install.sh --rpm dist/openflex-0.1.0-1.x86_64.rpm
#   scripts/verify-install.sh --help
#
# Options:
#   --from-dir <DIR>   Use a local release directory (no network).
#   --version <V>      Download v<V> from the GitHub release and install it.
#   --base-url <URL>   Release download base URL (default: install.sh's default,
#                      https://github.com/openflex/openflex/releases/download;
#                      override with a mirror or the real repository).
#   --platform <P>     Force the archive platform suffix (default: autodetect).
#   --deb <FILE>       Also verify this .deb (Linux only).
#   --rpm <FILE>       Also verify this .rpm (Linux only).
#   --workdir <DIR>    Scratch directory (default: mktemp -d).
#   --report <FILE>    Append a markdown table row with the result to FILE.
#   --keep             Keep the scratch directory.
#   --print-commands   Print copy-paste command sheets for every platform and exit
#                      (useful when the target machine is not this one).
#   -h, --help         Show this help.
#
# Exit code: 0 when every check passed (SKIP does not fail the run), 1 when at
# least one check failed, 2 on a usage error.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly ROOT_DIR
readonly PROGRAM="verify-install.sh"

FROM_DIR=""
VERSION=""
PLATFORM=""
BASE_URL="${OPENFLEX_BASE_URL:-}"
DEB_FILE=""
RPM_FILE=""
WORKDIR=""
REPORT=""
KEEP=0
PRINT_COMMANDS=0

PASSED=0
FAILED=0
SKIPPED=0

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
pass() { PASSED=$((PASSED + 1)); printf '  PASS  %s\n' "$1"; }
fail() { FAILED=$((FAILED + 1)); printf '  FAIL  %s\n' "$1"; }
skip() { SKIPPED=$((SKIPPED + 1)); printf '  SKIP  %s\n' "$1"; }
section() { printf '\n== %s ==\n' "$1"; }

usage() {
  sed -n '2,35p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --from-dir) [ "$#" -ge 2 ] || usage_error "--from-dir needs a value"; FROM_DIR="$2"; shift 2 ;;
    --from-dir=*) FROM_DIR="${1#*=}"; shift ;;
    --version) [ "$#" -ge 2 ] || usage_error "--version needs a value"; VERSION="$2"; shift 2 ;;
    --version=*) VERSION="${1#*=}"; shift ;;
    --base-url) [ "$#" -ge 2 ] || usage_error "--base-url needs a value"; BASE_URL="$2"; shift 2 ;;
    --base-url=*) BASE_URL="${1#*=}"; shift ;;
    --platform) [ "$#" -ge 2 ] || usage_error "--platform needs a value"; PLATFORM="$2"; shift 2 ;;
    --platform=*) PLATFORM="${1#*=}"; shift ;;
    --deb) [ "$#" -ge 2 ] || usage_error "--deb needs a value"; DEB_FILE="$2"; shift 2 ;;
    --deb=*) DEB_FILE="${1#*=}"; shift ;;
    --rpm) [ "$#" -ge 2 ] || usage_error "--rpm needs a value"; RPM_FILE="$2"; shift 2 ;;
    --rpm=*) RPM_FILE="${1#*=}"; shift ;;
    --workdir) [ "$#" -ge 2 ] || usage_error "--workdir needs a value"; WORKDIR="$2"; shift 2 ;;
    --workdir=*) WORKDIR="${1#*=}"; shift ;;
    --report) [ "$#" -ge 2 ] || usage_error "--report needs a value"; REPORT="$2"; shift 2 ;;
    --report=*) REPORT="${1#*=}"; shift ;;
    --keep) KEEP=1; shift ;;
    --print-commands) PRINT_COMMANDS=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) usage_error "unknown argument '$1'" ;;
  esac
done

print_commands() {
  cat <<'EOF'
# OpenFlex 真机安装验证命令清单
#
# 每段都可以直接复制执行；最后一行是"通过"的判定标准。
# 仓库地址：https://github.com/FanXingChunShui/openflex

## macOS（tar.gz + install.sh）
```
VERSION=0.14.0
OPENFLEX_REPO=FanXingChunShui/openflex \
  bash scripts/verify-install.sh --version "$VERSION"
# 或离线：bash scripts/verify-install.sh --from-dir dist
```
通过标准：RESULT: PASS，20 项；安装后的二进制 `openflex --version` 输出 `openflex 0.14.0`。

## Linux x86_64 / aarch64（tar.gz）
```
OPENFLEX_REPO=FanXingChunShui/openflex \
  bash scripts/verify-install.sh --version 0.14.0
# 期望：RESULT: PASS（安装 → 校验和 → --version → new/validate/run → 卸载）
```

## Debian / Ubuntu（.deb，需要 root 或 sudo）
```
sudo dpkg -i openflex_0.14.0_amd64.deb     # arm64 用 openflex_0.14.0_arm64.deb
openflex --version                          # 期望：openflex 0.14.0
openflex new /tmp/of-check --template mm1
openflex validate /tmp/of-check/mm1.ofm
openflex run /tmp/of-check/mm1.ofm --format json | head -c 200
sudo dpkg -r openflex
dpkg -l | grep openflex || echo "removed"
```
不用 root 的替代（私有 dpkg root，验证包内容）：
```
bash scripts/verify-install.sh --deb openflex_0.14.0_amd64.deb
```

## Fedora / RHEL / openSUSE（.rpm，需要 root 或 sudo）
```
sudo rpm -i openflex-0.14.0-1.x86_64.rpm    # aarch64 用 openflex-0.14.0-1.aarch64.rpm
openflex --version
openflex new /tmp/of-check --template mm1 && openflex validate /tmp/of-check/mm1.ofm
sudo rpm -e openflex
rpm -q openflex || echo "removed"
```
不用 root 的替代（rpm2cpio 解包验证）：
```
bash scripts/verify-install.sh --rpm openflex-0.14.0-1.x86_64.rpm
```

## Windows 10/11（见 scripts/verify-install.ps1）
```
# zip + install.ps1（装到临时目录，不动用户 PATH）
.\scripts\verify-install.ps1 -FromDir .\dist -Version 0.14.0

# MSI（perUser 安装到 %LOCALAPPDATA%\OpenFlex\bin）
.\scripts\verify-install.ps1 -Msi .\dist\openflex-0.14.0-windows-x64.msi

# 只打印命令清单（不执行）
.\scripts\verify-install.ps1 -PrintOnly
```
通过标准：`--version` 输出 `openflex 0.14.0`；`new`/`validate`/`run` 退出码 0；
MSI 卸载后 `%LOCALAPPDATA%\OpenFlex\bin\openflex.exe` 不存在。

## 记录
把结果追加到 docs/verification/install-matrix.md（行格式见该文件的「记录模板」），
并同步 docs/VERIFICATION.md 的三态表。
EOF
}

if [ "$PRINT_COMMANDS" -eq 1 ]; then
  print_commands
  exit 0
fi

if [ -z "$FROM_DIR" ] && [ -z "$VERSION" ] && [ -z "$DEB_FILE" ] && [ -z "$RPM_FILE" ]; then
  usage_error "pass --from-dir <DIR> or --version <V> (or --deb/--rpm on Linux)"
fi

if [ -n "$FROM_DIR" ]; then
  [ -d "$FROM_DIR" ] || die "--from-dir: no such directory: $FROM_DIR"
  FROM_DIR="$(cd "$FROM_DIR" && pwd)"
fi
VERSION="${VERSION#v}"

# ---------------------------------------------------------------- environment
OS_NAME="$(uname -s)"
ARCH_NAME="$(uname -m)"
case "$ARCH_NAME" in
  x86_64 | amd64 | x64) ARCH="x86_64" ;;
  arm64 | aarch64) ARCH="aarch64" ;;
  *) ARCH="$ARCH_NAME" ;;
esac
case "$OS_NAME" in
  Darwin)
    OS="darwin"
    # An x86_64 shell under Rosetta still runs on Apple silicon; the published
    # macOS archive is universal, so prefer the native suffix.
    if [ "$ARCH" = "x86_64" ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null || echo 0)" = "1" ]; then
      ARCH="aarch64"
    fi
    ;;
  Linux) OS="linux" ;;
  *) OS="unknown" ;;
esac

PLATFORMS=()
for candidate in ${PLATFORM:-}; do
  PLATFORMS+=("$candidate")
done
if [ "${#PLATFORMS[@]}" -eq 0 ]; then
  case "$OS" in
    darwin)
      PLATFORMS+=("universal-apple-darwin")
      PLATFORMS+=("$ARCH-apple-darwin")
      ;;
    linux)
      PLATFORMS+=("$ARCH-unknown-linux-gnu")
      PLATFORMS+=("$ARCH-unknown-linux-musl")
      ;;
    *) die "unsupported operating system '$OS_NAME'; use scripts/verify-install.ps1 on Windows" ;;
  esac
fi

WORK=""
if [ -n "$WORKDIR" ]; then
  mkdir -p "$WORKDIR"
  WORK="$(cd "$WORKDIR" && pwd)"
else
  WORK="$(mktemp -d "${TMPDIR:-/tmp}/openflex-verify-install.XXXXXX")"
fi
PREFIX="$WORK/prefix"
MODEL_DIR="$WORK/model"
cleanup() {
  if [ "$KEEP" -eq 1 ]; then
    printf '\nscratch directory kept: %s\n' "$WORK"
  else
    rm -rf "$WORK"
  fi
}
trap cleanup EXIT INT TERM

section "Environment"
printf '  os:      %s %s (%s)\n' "$OS_NAME" "$(uname -r)" "$ARCH"
printf '  bash:    %s\n' "${BASH_VERSION}"
printf '  workdir: %s\n' "$WORK"
printf '  mode:    %s\n' "$([ -n "$FROM_DIR" ] && echo "from-dir: $FROM_DIR" || echo "network: v${VERSION}")"

for tool in tar grep sed awk; do
  if command -v "$tool" >/dev/null 2>&1; then
    pass "tool available: $tool"
  else
    fail "tool missing: $tool"
  fi
done
if [ -f "$ROOT_DIR/packaging/install.sh" ] && [ -f "$ROOT_DIR/packaging/uninstall.sh" ]; then
  pass "packaging/install.sh and packaging/uninstall.sh present"
else
  fail "packaging scripts are missing under $ROOT_DIR/packaging"
fi

# ---------------------------------------------------------------- locate package
archive_platform_of() { # archive_platform_of <file> -> platform suffix
  file_name="$(basename "$1")"
  stem="${file_name%.tar.gz}"
  stem="${stem%.tgz}"
  case "$stem" in
    openflex-*) rest="${stem#openflex-}" ;;
    *) printf 'unknown\n'; return 0 ;;
  esac
  for candidate in "${PLATFORMS[@]}"; do
    case "$rest" in
      *"-$candidate") printf '%s\n' "$candidate"; return 0 ;;
    esac
  done
  printf 'unknown\n'
}

version_of() { # version_of <file> <platform> -> version
  file_name="$(basename "$1")"
  stem="${file_name%.tar.gz}"
  stem="${stem%.tgz}"
  stem="${stem#openflex-}"
  printf '%s\n' "${stem%-"$2"}"
}

ARCHIVE=""
ARCHIVE_PLATFORM=""
if [ -n "$FROM_DIR" ]; then
  section "Locate release archive in $FROM_DIR"
  best_rank=0
  for file in "$FROM_DIR"/openflex-*.tar.gz; do
    [ -f "$file" ] || continue
    platform_of="$(archive_platform_of "$file")"
    rank=1
    for candidate in "${PLATFORMS[@]}"; do
      [ "$platform_of" = "$candidate" ] && rank=2
    done
    if [ -n "$VERSION" ] && [ "$(version_of "$file" "$platform_of")" = "$VERSION" ]; then
      rank=$((rank + 2))
    fi
    if [ "$rank" -gt "$best_rank" ]; then
      best_rank="$rank"
      ARCHIVE="$file"
      ARCHIVE_PLATFORM="$platform_of"
    fi
  done

  if [ -z "$ARCHIVE" ]; then
    fail "no openflex-*.tar.gz found in $FROM_DIR"
  else
    pass "using $(basename "$ARCHIVE") (platform: $ARCHIVE_PLATFORM)"
    if [ "$ARCHIVE_PLATFORM" = "unknown" ]; then
      warn "the archive name does not match this machine; install.sh may refuse it"
    fi
    if [ -z "$VERSION" ]; then
      VERSION="$(version_of "$ARCHIVE" "$ARCHIVE_PLATFORM")"
    fi
    if [ -n "$VERSION" ]; then
      pass "version to verify: $VERSION"
    else
      fail "could not determine the version from $(basename "$ARCHIVE")"
    fi
  fi
else
  section "Download release v$VERSION"
  if [ -z "$VERSION" ]; then
    fail "no --version given and no --from-dir to read it from"
  else
    info "install.sh will download v$VERSION from the GitHub release"
  fi
fi

# ---------------------------------------------------------------- install
INSTALLED="$PREFIX/bin/openflex"
if [ -n "$ARCHIVE" ] || [ -n "$VERSION" ]; then
  section "Install into throwaway prefix"
  : >"$WORK/install.log"
  install_ok=0
  if [ -n "$FROM_DIR" ]; then
    if bash "$ROOT_DIR/packaging/install.sh" \
      --from-dir "$FROM_DIR" --version "$VERSION" --prefix "$PREFIX" --no-path-hint \
      >>"$WORK/install.log" 2>&1; then
      install_ok=1
    fi
  elif [ -n "$BASE_URL" ]; then
    if bash "$ROOT_DIR/packaging/install.sh" \
      --version "$VERSION" --prefix "$PREFIX" --no-path-hint --base-url "$BASE_URL" \
      >>"$WORK/install.log" 2>&1; then
      install_ok=1
    fi
  else
    if bash "$ROOT_DIR/packaging/install.sh" \
      --version "$VERSION" --prefix "$PREFIX" --no-path-hint \
      >>"$WORK/install.log" 2>&1; then
      install_ok=1
    fi
  fi

  if [ "$install_ok" -eq 1 ]; then
    pass "install completed (exit 0)"
    if [ -x "$INSTALLED" ]; then
      pass "installed binary is executable: $INSTALLED"
    else
      fail "no executable at $INSTALLED"
    fi
    if grep -q 'sha256 verified' "$WORK/install.log"; then
      pass "install verified the SHA-256 checksum"
    else
      fail "install did not verify a SHA-256 checksum"
      tail -n 10 "$WORK/install.log" | sed 's/^/        /'
    fi
  else
    if [ -z "$FROM_DIR" ]; then
        skip "network install failed (no network, wrong repository, or release missing); re-run with --from-dir <DIR>"
      tail -n 8 "$WORK/install.log" | sed 's/^/        /'
      printf '        hint: install.sh defaults to the placeholder repo openflex/openflex;\n'
      printf '        hint: set OPENFLEX_REPO=FanXingChunShui/openflex (or pass --base-url) for the real release page.\n'
    else
      fail "install.sh failed"
      tail -n 15 "$WORK/install.log" | sed 's/^/        /'
    fi
  fi
else
  skip "install (no archive and no version)"
fi

# ---------------------------------------------------------------- smoke tests
section "Smoke tests on the installed binary"
if [ -x "$INSTALLED" ]; then
  run_version="$("$INSTALLED" --version 2>&1 || true)"
  if [ "$run_version" = "openflex $VERSION" ]; then
    pass "openflex --version reports 'openflex $VERSION'"
  else
    fail "openflex --version reported '${run_version:-no output}' (expected 'openflex $VERSION')"
  fi

  if "$INSTALLED" new "$MODEL_DIR" --template mm1 >"$WORK/new.log" 2>&1 &&
    [ -f "$MODEL_DIR/mm1.ofm" ]; then
    pass "openflex new created mm1.ofm"
  else
    fail "openflex new failed"
    tail -n 10 "$WORK/new.log" | sed 's/^/        /'
  fi

  if [ -f "$MODEL_DIR/mm1.ofm" ] &&
    "$INSTALLED" validate "$MODEL_DIR/mm1.ofm" >"$WORK/validate.log" 2>&1; then
    pass "openflex validate accepts the generated model"
  else
    fail "openflex validate rejected the generated model"
    tail -n 10 "$WORK/validate.log" 2>/dev/null | sed 's/^/        /'
  fi

  if [ -f "$MODEL_DIR/mm1.ofm" ] &&
    "$INSTALLED" run "$MODEL_DIR/mm1.ofm" --format json --replications 2 \
      >"$WORK/report.json" 2>"$WORK/run.err"; then
    pass "openflex run --format json exits 0"
    if [ -s "$WORK/report.json" ] && grep -q '"objects"' "$WORK/report.json"; then
      pass "report.json contains an objects section"
    else
      fail "report.json has no objects section"
    fi
    if command -v python3 >/dev/null 2>&1; then
      if python3 -m json.tool <"$WORK/report.json" >/dev/null 2>&1; then
        pass "report.json is valid JSON"
      else
        fail "report.json is not valid JSON"
      fi
    else
      skip "python3 not installed (skipping the strict JSON parse)"
    fi
  else
    fail "openflex run --format json failed"
    tail -n 8 "$WORK/run.err" 2>/dev/null | sed 's/^/        /'
  fi

  if [ -f "$MODEL_DIR/mm1.ofm" ] &&
    "$INSTALLED" run "$MODEL_DIR/mm1.ofm" --format csv >"$WORK/report.csv" 2>"$WORK/csv.err" &&
    grep -q 'Sink' "$WORK/report.csv"; then
    pass "openflex run --format csv names the sink"
  else
    fail "openflex run --format csv failed"
    tail -n 8 "$WORK/csv.err" 2>/dev/null | sed 's/^/        /'
  fi

  if [ -f "$PREFIX/share/openflex/install-receipt.txt" ]; then
    pass "install receipt written to $PREFIX/share/openflex"
  else
    fail "install receipt missing"
  fi
else
  skip "smoke tests (nothing installed)"
fi

# ---------------------------------------------------------------- uninstall
section "Uninstall cleans up"
if [ -x "$INSTALLED" ]; then
  : >"$WORK/uninstall.log"
  if bash "$ROOT_DIR/packaging/uninstall.sh" --prefix "$PREFIX" >>"$WORK/uninstall.log" 2>&1; then
    pass "uninstall exits 0"
  else
    fail "uninstall failed"
    tail -n 10 "$WORK/uninstall.log" | sed 's/^/        /'
  fi
  if [ ! -e "$INSTALLED" ]; then
    pass "uninstall removed the binary"
  else
    fail "uninstall left $INSTALLED behind"
  fi
  if [ ! -e "$PREFIX/share/openflex/install-receipt.txt" ]; then
    pass "uninstall removed the install receipt"
  else
    fail "uninstall left the install receipt behind"
  fi
  if bash "$ROOT_DIR/packaging/uninstall.sh" --prefix "$PREFIX" >>"$WORK/uninstall.log" 2>&1; then
    pass "second uninstall is a no-op (exit 0)"
  else
    fail "second uninstall failed"
  fi
else
  skip "uninstall (nothing installed)"
fi

# ---------------------------------------------------------------- deb / rpm
verify_deb() { # verify_deb <file>
  deb="$1"
  [ -f "$deb" ] || {
    fail "deb not found: $deb"
    return 0
  }
  if ! command -v dpkg >/dev/null 2>&1; then
    skip "dpkg not installed (only the tar.gz path was verified)"
    return 0
  fi
  root="$WORK/deb-root"
  mkdir -p "$root/var/lib/dpkg"
  : >"$root/var/lib/dpkg/status"
  : >"$WORK/deb-install.log"
  if dpkg --root="$root" --force-not-root --instdir="$root" --admindir="$root/var/lib/dpkg" \
    -i "$deb" >>"$WORK/deb-install.log" 2>&1; then
    pass "dpkg -i succeeded in a private root"
  else
    fail "dpkg -i failed"
    tail -n 10 "$WORK/deb-install.log" | sed 's/^/        /'
    return 0
  fi
  deb_bin="$root/usr/bin/openflex"
  if [ -x "$deb_bin" ] && [ "$("$deb_bin" --version 2>&1)" = "openflex $VERSION" ]; then
    pass "deb-installed binary reports 'openflex $VERSION'"
  else
    fail "deb-installed binary is missing or reports the wrong version"
  fi
  if dpkg --root="$root" --force-not-root --instdir="$root" --admindir="$root/var/lib/dpkg" \
    -r openflex >>"$WORK/deb-install.log" 2>&1; then
    pass "dpkg -r removed the package"
  else
    fail "dpkg -r failed"
    tail -n 10 "$WORK/deb-install.log" | sed 's/^/        /'
  fi
  if [ ! -e "$deb_bin" ]; then
    pass "deb files were removed from the private root"
  else
    fail "dpkg -r left $deb_bin behind"
  fi
}

verify_rpm() { # verify_rpm <file>
  rpm_file="$1"
  [ -f "$rpm_file" ] || {
    fail "rpm not found: $rpm_file"
    return 0
  }
  if ! command -v rpm2cpio >/dev/null 2>&1 || ! command -v cpio >/dev/null 2>&1; then
    skip "rpm2cpio/cpio not installed (only the tar.gz path was verified)"
    return 0
  fi
  root="$WORK/rpm-root"
  mkdir -p "$root"
  if (cd "$root" && rpm2cpio "$rpm_file" | cpio -idm --quiet) >"$WORK/rpm.log" 2>&1; then
    pass "rpm payload extracted with rpm2cpio"
  else
    fail "rpm2cpio extraction failed"
    tail -n 10 "$WORK/rpm.log" | sed 's/^/        /'
    return 0
  fi
  rpm_bin="$root/usr/bin/openflex"
  if [ -x "$rpm_bin" ] && [ "$("$rpm_bin" --version 2>&1)" = "openflex $VERSION" ]; then
    pass "rpm payload binary reports 'openflex $VERSION'"
  else
    fail "rpm payload binary is missing or reports the wrong version"
  fi
}

if [ -n "$DEB_FILE" ] || [ -n "$RPM_FILE" ]; then
  section "Distro packages"
  [ -z "$DEB_FILE" ] || verify_deb "$DEB_FILE"
  [ -z "$RPM_FILE" ] || verify_rpm "$RPM_FILE"
fi

# ---------------------------------------------------------------- summary
section "Summary"
printf '  passed:  %d\n' "$PASSED"
printf '  failed:  %d\n' "$FAILED"
printf '  skipped: %d\n' "$SKIPPED"

RESULT="PASS"
if [ "$FAILED" -gt 0 ]; then
  RESULT="FAIL"
fi

if [ -n "$REPORT" ]; then
  os_label="$OS_NAME $(uname -r) ($ARCH)"
  mode_label="from-dir"
  [ -n "$FROM_DIR" ] || mode_label="network"
  printf '| %s | %s | %s | %s | %s | %s |\n' \
    "$os_label" "$mode_label" "${ARCHIVE_PLATFORM:-unknown}" \
    "${VERSION:-unknown}" "$RESULT" "$(date -u '+%Y-%m-%d %H:%M UTC')" >>"$REPORT"
  info "appended a result row to $REPORT"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\nRESULT: FAIL (%d failed check(s))\n' "$FAILED" >&2
  exit 1
fi
printf '\nRESULT: PASS\n'
