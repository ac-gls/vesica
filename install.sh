#!/bin/sh
# agent-home bootstrap: download the pinned release binary, verify its sha256, install it to
# ~/.local/bin, then hand over to `agent-home install "$@"`. Everything else lives in the binary.
#
#   curl -fsSL https://raw.githubusercontent.com/ac-gls/agent-home/v0.1.0/install.sh \
#     | sh -s -- --home-repo <owner/repo> --profile developer
#
# Bootstrap-only options (consumed here, not passed on):
#   --from-file <tarball>   install from a local release tarball (needs SHA256SUMS next to it)
#   --bootstrap-only        install the binary but do not run `agent-home install`
set -eu

VERSION="${AGENT_HOME_VERSION:-v0.1.0}"
REPO="${AGENT_HOME_REPO:-ac-gls/agent-home}"
BIN_DIR="${AGENT_HOME_BIN_DIR:-$HOME/.local/bin}"
ASSET="agent-home-${VERSION}-x86_64-linux.tar.gz"
FROM_FILE=""
BOOTSTRAP_ONLY=0

die() { printf 'agent-home bootstrap: %s\n' "$1" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"; }

# Split our options from the ones meant for `agent-home install`.
ARGS_FILE=$(mktemp)
trap 'rm -f "$ARGS_FILE"; [ -n "${WORK:-}" ] && rm -rf "$WORK"' EXIT
while [ $# -gt 0 ]; do
  case "$1" in
    --from-file) [ $# -ge 2 ] || die "--from-file needs a path"; FROM_FILE=$2; shift 2 ;;
    --bootstrap-only) BOOTSTRAP_ONLY=1; shift ;;
    *) printf '%s\n' "$1" >>"$ARGS_FILE"; shift ;;
  esac
done

if [ "${AGENT_HOME_ALLOW_NON_WSL:-0}" != "1" ]; then
  grep -qi microsoft /proc/version 2>/dev/null || die "this kit supports Windows + WSL Ubuntu only (set AGENT_HOME_ALLOW_NON_WSL=1 for CI)"
fi
[ "$(uname -m)" = "x86_64" ] || die "only x86_64 is supported"
need tar
need sha256sum

WORK=$(mktemp -d)
if [ -n "$FROM_FILE" ]; then
  [ -f "$FROM_FILE" ] || die "no such file: $FROM_FILE"
  cp "$FROM_FILE" "$WORK/$ASSET"
  SUMS="$(dirname "$FROM_FILE")/SHA256SUMS"
  [ -f "$SUMS" ] || die "SHA256SUMS not found next to $FROM_FILE"
  cp "$SUMS" "$WORK/SHA256SUMS"
else
  need curl
  BASE="https://github.com/${REPO}/releases/download/${VERSION}"
  curl -fsSL "$BASE/$ASSET" -o "$WORK/$ASSET" || die "download failed: $BASE/$ASSET"
  curl -fsSL "$BASE/SHA256SUMS" -o "$WORK/SHA256SUMS" || die "download failed: $BASE/SHA256SUMS"
fi

(cd "$WORK" && grep " $ASSET\$" SHA256SUMS | sha256sum -c - >/dev/null 2>&1) \
  || die "checksum mismatch for $ASSET: refusing to install"

tar -xzf "$WORK/$ASSET" -C "$WORK"
[ -f "$WORK/agent-home" ] || die "release tarball does not contain agent-home"
mkdir -p "$BIN_DIR"
install -m 0755 "$WORK/agent-home" "$BIN_DIR/agent-home"
printf 'agent-home %s installed to %s\n' "$VERSION" "$BIN_DIR/agent-home"

[ "$BOOTSTRAP_ONLY" = "1" ] && exit 0
# Re-assemble the remaining arguments safely (one per line, no word splitting).
set --
while IFS= read -r a; do set -- "$@" "$a"; done <"$ARGS_FILE"
rm -f "$ARGS_FILE"
rm -rf "$WORK"
exec "$BIN_DIR/agent-home" install "$@"
