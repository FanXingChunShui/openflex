#!/usr/bin/env bash
# Check that the OpenFlex container images really exist in the registry.
#
# Uses only the GHCR HTTP API (anonymous pull token): no Docker, no GitHub
# token, no credentials. It answers the questions that matter after a release:
#
#   * does tag <version> exist?
#   * is it a multi-arch manifest list (linux/amd64 + linux/arm64)?
#   * what is its digest?
#   * does `latest` point at the expected version?
#
# Usage:
#   scripts/verify-ghcr.sh                         # checks 0.10.0, 0.11.0, 0.12.0, 0.13.0, latest
#   scripts/verify-ghcr.sh --tags 0.12.0,0.13.0    # explicit tags
#   scripts/verify-ghcr.sh --expect-latest 0.13.0  # fail when latest != 0.13.0
#   scripts/verify-ghcr.sh --retry 6 --wait 30     # poll up to 6 times, 30s apart
#   scripts/verify-ghcr.sh --json /tmp/ghcr.json
#   scripts/verify-ghcr.sh --help
#
# Options:
#   --image <REF>        Image without a tag (default: ghcr.io/fanxingchunshui/openflex).
#   --tags <LIST>        Comma-separated tags (default: 0.10.0,0.11.0,0.12.0,0.13.0,latest).
#   --expect-latest <V>  Require `latest` to resolve to version V (digest equality).
#   --retry <N>          Re-check N times while a tag is missing (default: 1 = no retry).
#   --wait <SECONDS>     Sleep between retries (default: 30).
#   --json <FILE>        Write the raw result as JSON.
#   -h, --help           Show this help.
#
# Exit code: 0 when every requested tag exists and --expect-latest holds,
# 1 otherwise, 2 on a usage error.
set -euo pipefail

readonly PROGRAM="verify-ghcr.sh"

IMAGE="ghcr.io/fanxingchunshui/openflex"
TAGS="0.10.0,0.11.0,0.12.0,0.13.0,latest"
EXPECT_LATEST=""
RETRY=1
WAIT=30
OUT_JSON=""

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
  sed -n '2,33p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --image) [ "$#" -ge 2 ] || usage_error "--image needs a value"; IMAGE="$2"; shift 2 ;;
    --image=*) IMAGE="${1#*=}"; shift ;;
    --tags) [ "$#" -ge 2 ] || usage_error "--tags needs a value"; TAGS="$2"; shift 2 ;;
    --tags=*) TAGS="${1#*=}"; shift ;;
    --expect-latest) [ "$#" -ge 2 ] || usage_error "--expect-latest needs a value"; EXPECT_LATEST="$2"; shift 2 ;;
    --expect-latest=*) EXPECT_LATEST="${1#*=}"; shift ;;
    --retry) [ "$#" -ge 2 ] || usage_error "--retry needs a value"; RETRY="$2"; shift 2 ;;
    --retry=*) RETRY="${1#*=}"; shift ;;
    --wait) [ "$#" -ge 2 ] || usage_error "--wait needs a value"; WAIT="$2"; shift 2 ;;
    --wait=*) WAIT="${1#*=}"; shift ;;
    --json) [ "$#" -ge 2 ] || usage_error "--json needs a value"; OUT_JSON="$2"; shift 2 ;;
    --json=*) OUT_JSON="${1#*=}"; shift ;;
    -h | --help) usage; exit 0 ;;
    *) usage_error "unknown argument '$1'" ;;
  esac
done

case "$IMAGE" in
  "" | */*) : ;;
  *) usage_error "--image must look like ghcr.io/<owner>/<name>" ;;
esac
case "$RETRY" in
  '' | *[!0-9]*) usage_error "--retry must be a positive integer" ;;
esac
case "$WAIT" in
  '' | *[!0-9]*) usage_error "--wait must be a non-negative integer" ;;
esac
[ "$RETRY" -ge 1 ] || usage_error "--retry must be at least 1"

if ! command -v curl >/dev/null 2>&1; then
  die "curl is required"
fi
if ! command -v python3 >/dev/null 2>&1; then
  die "python3 is required"
fi

# Strip the tag from --image if someone passed one.
IMAGE="${IMAGE%%:*}"

info "image: $IMAGE"
info "tags:  $TAGS"
[ -z "$EXPECT_LATEST" ] || info "expected latest: $EXPECT_LATEST"
[ "$RETRY" -le 1 ] || info "retries: $RETRY (wait ${WAIT}s)"

attempt=1
while :; do
  if python3 - "$IMAGE" "$TAGS" "$EXPECT_LATEST" "$OUT_JSON" <<'PY'
import json
import sys
import urllib.error
import urllib.request

image, tags_arg, expect_latest, out_json = sys.argv[1:5]
tags = [tag.strip() for tag in tags_arg.split(",") if tag.strip()]
repository = image.split("/", 1)[1]

ACCEPT = ", ".join(
    [
        "application/vnd.oci.image.index.v1+json",
        "application/vnd.docker.distribution.manifest.list.v2+json",
        "application/vnd.oci.image.manifest.v1+json",
        "application/vnd.docker.distribution.manifest.v2+json",
    ]
)


def header(headers, name):
    for key, value in headers.items():
        if key.lower() == name.lower():
            return value
    return ""


def http(url, headers, method="GET"):
    request = urllib.request.Request(url, headers=headers, method=method)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.status, dict(response.headers), response.read()
    except urllib.error.HTTPError as error:
        return error.code, dict(error.headers or {}), b""
    except Exception as error:  # noqa: BLE001 - network errors are reported as 0
        print("verify-ghcr: network error for %s: %s" % (url, error), file=sys.stderr)
        return 0, {}, b""


token_url = "https://ghcr.io/token?scope=repository:%s:pull&service=ghcr.io" % repository
status, _, body = http(token_url, {"Accept": "application/json"})
if status != 200:
    print("verify-ghcr: error: could not get an anonymous pull token (HTTP %s)" % status, file=sys.stderr)
    print("verify-ghcr: the package may be private; make it public or pass a token", file=sys.stderr)
    sys.exit(1)
token = json.loads(body.decode()).get("token", "")
headers = {"Authorization": "Bearer %s" % token, "Accept": ACCEPT}

results = {}
for tag in tags:
    url = "https://ghcr.io/v2/%s/manifests/%s" % (repository, tag)
    status, response_headers, body = http(url, headers)
    digest = header(response_headers, "Docker-Content-Digest")
    platforms = []
    if status == 200 and body:
        try:
            document = json.loads(body.decode())
            for manifest in document.get("manifests", []):
                platform = manifest.get("platform") or {}
                os_name = platform.get("os")
                arch = platform.get("architecture")
                if os_name and arch and os_name != "unknown":
                    platforms.append("%s/%s" % (os_name, arch))
        except json.JSONDecodeError:
            pass
    results[tag] = {"status": status, "digest": digest, "platforms": platforms}

print()
print("  %-10s %-6s %-22s %s" % ("tag", "HTTP", "digest", "platforms"))
print("  " + "-" * 68)
for tag in tags:
    entry = results[tag]
    print(
        "  %-10s %-6s %-22s %s"
        % (
            tag,
            entry["status"],
            (entry["digest"] or "-")[7:19],
            ", ".join(entry["platforms"]) or "-",
        )
    )

missing = [tag for tag in tags if results[tag]["status"] != 200]
latest_ok = True
if expect_latest:
    expected = results.get(expect_latest)
    latest = results.get("latest")
    if expected and latest:
        latest_ok = expected["digest"] == latest["digest"] and expected["status"] == 200
        print()
        print(
            "  latest == %s: %s (%s vs %s)"
            % (
                expect_latest,
                "yes" if latest_ok else "NO",
                latest["digest"][7:19] or "-",
                expected["digest"][7:19] or "-",
            )
        )
    else:
        latest_ok = False

if out_json:
    with open(out_json, "w", encoding="utf-8") as handle:
        json.dump(
            {
                "image": image,
                "tags": tags,
                "results": results,
                "missing": missing,
                "expect_latest": expect_latest or None,
                "latest_ok": latest_ok,
            },
            handle,
            indent=2,
        )
        handle.write("\n")

if missing:
    print()
    print("verify-ghcr: missing tags: %s" % ", ".join(missing), file=sys.stderr)
if not latest_ok:
    print("verify-ghcr: latest does not point at %s" % expect_latest, file=sys.stderr)
sys.exit(0 if not missing and latest_ok else 1)
PY
  then
    exit 0
  fi
  if [ "$attempt" -ge "$RETRY" ]; then
    exit 1
  fi
  info "attempt $attempt/$RETRY failed; retrying in ${WAIT}s"
  sleep "$WAIT"
  attempt=$((attempt + 1))
done
