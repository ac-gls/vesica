#!/bin/sh
# Linux bootstrap: download the pinned Linux x86_64 release binary, verify its sha256,
# install it to ~/.local/bin, then hand over to `vesica install "$@"`. Everything else lives in the binary.
#
#   curl -fsSL https://raw.githubusercontent.com/ac-gls/vesica/v0.1.0-alpha.4/install.sh | sh -s -- --bootstrap-only
#
# Without --bootstrap-only, the remaining arguments are passed to `vesica install`
# (e.g. --home-repo /path/to/shared-home-checkout --root /tmp/vesica-trial).
#
# Bootstrap-only options (consumed here, not passed on):
#   --from-file <tarball>   install from a local release tarball (needs SHA256SUMS next to it)
#   --bootstrap-only        install the binary but do not run `vesica install`
set -eu

VERSION="${VESICA_VERSION:-${AGENT_HOME_VERSION:-v0.1.0-alpha.4}}"
REPO="${VESICA_REPO:-${AGENT_HOME_REPO:-ac-gls/vesica}}"
BIN_DIR="${VESICA_BIN_DIR:-${AGENT_HOME_BIN_DIR:-$HOME/.local/bin}}"
ASSET="vesica-${VERSION}-x86_64-linux.tar.gz"
FROM_FILE=""
BOOTSTRAP_ONLY=0

die() { printf 'vesica bootstrap: %s\n' "$1" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"; }

# Split our options from the ones meant for `vesica install`.
# Ownership starts empty; inherited environment paths are never cleanup authority.
STAGED=""
WORK=""
ARGS_FILE=$(mktemp)
cleanup() {
  status=$?
  if [ -n "${STAGED:-}" ] && [ -e "$STAGED" ]; then
    rm -f "$STAGED" || { printf 'vesica bootstrap: staging cleanup failed; retained %s\n' "$STAGED" >&2; status=1; }
  fi
  rm -f "$ARGS_FILE" || status=1
  if [ -n "${WORK:-}" ]; then rm -rf "$WORK" || status=1; fi
  trap - EXIT
  exit "$status"
}
trap cleanup EXIT
while [ $# -gt 0 ]; do
  case "$1" in
    --from-file) [ $# -ge 2 ] || die "--from-file needs a path"; FROM_FILE=$2; shift 2 ;;
    --bootstrap-only) BOOTSTRAP_ONLY=1; shift ;;
    *) printf '%s\n' "$1" >>"$ARGS_FILE"; shift ;;
  esac
done

[ "$(uname -s)" = "Linux" ] || die "only Linux hosts are supported"
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
[ -f "$WORK/vesica" ] || die "release tarball does not contain vesica"
# Shell substitution erases NUL bytes. Preserve raw stdout and reject that lossy case first.
need tr
need cmp
need cat
"$WORK/vesica" --version > "$WORK/version.stdout" 2>/dev/null || die "version probe failed: staged vesica --version exited unsuccessfully"
LC_ALL=C tr -d '\000' < "$WORK/version.stdout" > "$WORK/version.nonul"
cmp -s "$WORK/version.stdout" "$WORK/version.nonul" || die "version mismatch: malformed NUL in staged version output"
# Only trailing newlines are removed, matching the updater's existing identity rule.
version_output=$(cat "$WORK/version.stdout")
REPORTED_VERSION=$version_output
[ "$REPORTED_VERSION" = "vesica ${VERSION#v}" ] || die "version mismatch: expected vesica ${VERSION#v}, got ${REPORTED_VERSION:-no version}"
mkdir -p "$BIN_DIR"
check_target() {
  [ ! -L "$BIN_DIR/vesica" ] || die "existing binary is a symlink: refusing replacement"
  [ ! -e "$BIN_DIR/vesica" ] || [ -f "$BIN_DIR/vesica" ] || die "existing binary is not a regular file: refusing replacement"
}
check_target
# A private, exclusively created sibling stays on the destination filesystem. Preparation
# never opens the installed executable; Linux rename is the publication/acceptance boundary.
STAGED=$(mktemp "$BIN_DIR/.vesica-bootstrap.XXXXXX") || die "cannot prepare target-side staging"
install -m 0755 "$WORK/vesica" "$STAGED" || die "cannot prepare staged executable; existing binary unchanged"
check_target
mv -fT "$STAGED" "$BIN_DIR/vesica" || die "cannot publish staged executable; existing binary unchanged"
STAGED=""
printf '%s installed to %s\n' "$REPORTED_VERSION" "$BIN_DIR/vesica"

[ "$BOOTSTRAP_ONLY" = "1" ] && exit 0
# Re-assemble the remaining arguments safely (one per line, no word splitting).
set --
while IFS= read -r a; do set -- "$@" "$a"; done <"$ARGS_FILE"
rm -f "$ARGS_FILE"
rm -rf "$WORK"
exec "$BIN_DIR/vesica" install "$@"
