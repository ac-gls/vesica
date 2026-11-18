#!/usr/bin/env bash
# Integration tests for the Vesica shell bootstrap using a local fixture release.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
INSTALLER="$ROOT/install.sh"

fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
assert_eq() {
  [[ "$1" == "$2" ]] || fail "$3 (expected '$2', got '$1')"
}

if [[ -n "${VESICA_TEST_TMP_ROOT:-}" ]]; then export TMPDIR="$VESICA_TEST_TMP_ROOT"; fi
TMP=$(mktemp -d "${TMPDIR:-/tmp}/vesica-test.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
export HOME="$TMP/home"
export VESICA_BIN_DIR="$TMP/bin"
# Pin a test-only version so these tests do not depend on the release default in install.sh.
export VESICA_VERSION=v0.0.0-test
mkdir -p "$HOME" "$TMP/release"

# Fake a release tarball with a binary that records argv and install invocations.
cat > "$TMP/release/vesica" <<'STUB'
#!/bin/sh
set -eu
if [ "${1:-}" = install ]; then
  shift
  printf '%s\n' install >>"$VESICA_CALLS"
  printf '<%s>\n' "$@" >>"$VESICA_CALLS"
fi
STUB
chmod +x "$TMP/release/vesica"
tar --mtime='@0' --owner=0 --group=0 --numeric-owner -C "$TMP/release" -czf "$TMP/release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" vesica
(
  cd "$TMP/release"
  sha256sum "vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" > SHA256SUMS
)
export VESICA_CALLS="$TMP/calls"

# Verify platform detection independently of the host running this test.
mkdir -p "$TMP/fake-bin"
cat > "$TMP/fake-bin/uname" <<'UNAME'
#!/bin/sh
case "${1:-}" in
  -s) printf '%s\n' "${VESICA_TEST_UNAME_S:-Linux}" ;;
  -m) printf '%s\n' x86_64 ;;
  *) exit 1 ;;
esac
UNAME
chmod +x "$TMP/fake-bin/uname"
# A noexec temp filesystem would silently fall through to the real uname and give a misleading result.
[[ "$("$TMP/fake-bin/uname" -s 2>/dev/null)" == Linux ]] \
  || fail "cannot execute test shims under $TMP (noexec temp dir?); set VESICA_TEST_TMP_ROOT to an exec-capable directory"
if PATH="$TMP/fake-bin:$PATH" VESICA_TEST_UNAME_S=Darwin sh "$INSTALLER" --from-file "$TMP/release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" --bootstrap-only >"$TMP/out" 2>&1; then
  fail "non-Linux host was accepted"
fi
grep -q 'only Linux hosts are supported' "$TMP/out" || fail "non-Linux rejection was not reported"

# Native Linux bootstrap does not depend on the historical WSL-only override.
unset VESICA_ALLOW_NON_WSL || true
sh "$INSTALLER" --from-file "$TMP/release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" --bootstrap-only
[[ -x "$VESICA_BIN_DIR/vesica" ]] || fail "binary was not installed"
[[ ! -e "$VESICA_CALLS" ]] || fail "bootstrap-only unexpectedly invoked install"

# Normal invocation forwards install arguments without word splitting.
rm -f "$VESICA_BIN_DIR/vesica"
sh "$INSTALLER" --from-file "$TMP/release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" --home-repo 'ac-gls/example repo' --profile developer
assert_eq "$(<"$VESICA_CALLS")" $'install\n<--home-repo>\n<ac-gls/example repo>\n<--profile>\n<developer>' "installer arguments were not preserved"

# Reject a checksum mismatch without leaving a binary behind.
printf '%064d  %s\n' 0 "vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" > "$TMP/release/SHA256SUMS"
rm -f "$VESICA_BIN_DIR/vesica" "$VESICA_CALLS"
if sh "$INSTALLER" --from-file "$TMP/release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" --bootstrap-only >"$TMP/out" 2>&1; then
  fail "checksum mismatch was accepted"
fi
grep -q 'checksum mismatch' "$TMP/out" || fail "checksum failure was not reported"
[[ ! -e "$VESICA_BIN_DIR/vesica" ]] || fail "binary left behind after checksum failure"
[[ ! -e "$VESICA_CALLS" ]] || fail "install invoked after checksum failure"

# Reject a checksum file that omits the target asset.
: > "$TMP/release/SHA256SUMS"
if sh "$INSTALLER" --from-file "$TMP/release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" --bootstrap-only >"$TMP/out" 2>&1; then
  fail "missing checksum entry was accepted"
fi
[[ ! -e "$VESICA_BIN_DIR/vesica" ]] || fail "binary left behind after missing checksum entry"

printf 'installer tests passed (Linux platform check, bootstrap-only, argument forwarding, checksum rejection)\n'
