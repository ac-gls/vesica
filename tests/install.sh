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
if [ "${1:-}" = --version ]; then
  printf '%s\n' "vesica ${VESICA_VERSION#v}"
  exit 0
fi
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
sh "$INSTALLER" --from-file "$TMP/release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" --bootstrap-only >"$TMP/success-out" 2>&1
[[ -x "$VESICA_BIN_DIR/vesica" ]] || fail "binary was not installed"
VERIFIED_IDENTITY=$("$VESICA_BIN_DIR/vesica" --version)
assert_eq "$(<"$TMP/success-out")" "$VERIFIED_IDENTITY installed to $VESICA_BIN_DIR/vesica" "success message must use the verified executable identity"
[[ ! -e "$VESICA_CALLS" ]] || fail "bootstrap-only unexpectedly invoked install"

# A valid checksum does not establish the binary's identity: the executable must report the
# pinned bootstrap version before it is installed.
mkdir -p "$TMP/mismatch-release" "$TMP/mismatch-bin"
printf 'user-owned binary bytes\\000\\377\\n' > "$TMP/mismatch-bin/vesica"
MISMATCH_TARGET_BEFORE=$(sha256sum "$TMP/mismatch-bin/vesica")
cat > "$TMP/mismatch-release/vesica" <<'STUB'
#!/bin/sh
if [ "${1:-}" = --version ]; then
  printf '%s\n' "vesica 9.9.9"
  exit 0
fi
STUB
chmod +x "$TMP/mismatch-release/vesica"
tar --mtime='@0' --owner=0 --group=0 --numeric-owner -C "$TMP/mismatch-release" -czf "$TMP/mismatch-release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" vesica
(
  cd "$TMP/mismatch-release"
  sha256sum "vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" > SHA256SUMS
)
if VESICA_BIN_DIR="$TMP/mismatch-bin" sh "$INSTALLER" --from-file "$TMP/mismatch-release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" --bootstrap-only >"$TMP/out" 2>&1; then
  fail "bootstrap accepted a binary whose reported version did not match VESICA_VERSION"
fi
grep -q 'version mismatch' "$TMP/out" || fail "bootstrap version identity failure was not reported"
assert_eq "$(sha256sum "$TMP/mismatch-bin/vesica")" "$MISMATCH_TARGET_BEFORE" "failed version check changed the pre-existing binary"

# A staged executable that reports the expected identity but exits unsuccessfully must not
# be trusted, and rejection must happen before replacing an existing target.
mkdir -p "$TMP/nonzero-release" "$TMP/nonzero-bin"
printf 'pre-existing binary bytes\\000\\377\\n' > "$TMP/nonzero-bin/vesica"
NONZERO_TARGET_BEFORE=$(sha256sum "$TMP/nonzero-bin/vesica")
cat > "$TMP/nonzero-release/vesica" <<'STUB'
#!/bin/sh
if [ "${1:-}" = --version ]; then
  printf '%s\n' "vesica ${VESICA_VERSION#v}"
  exit 23
fi
STUB
chmod +x "$TMP/nonzero-release/vesica"
tar --mtime='@0' --owner=0 --group=0 --numeric-owner -C "$TMP/nonzero-release" -czf "$TMP/nonzero-release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" vesica
(
  cd "$TMP/nonzero-release"
  sha256sum "vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" > SHA256SUMS
)
if VESICA_BIN_DIR="$TMP/nonzero-bin" sh "$INSTALLER" --from-file "$TMP/nonzero-release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" --bootstrap-only >"$TMP/out" 2>&1; then
  fail "bootstrap accepted a version-matching executable whose --version exited nonzero"
fi
grep -q 'version probe failed' "$TMP/out" || fail "nonzero version-probe failure was not reported"
assert_eq "$(sha256sum "$TMP/nonzero-bin/vesica")" "$NONZERO_TARGET_BEFORE" "failed version probe changed the pre-existing binary"

# Checksum-valid rejection matrix. Every case starts with an existing binary, checks exact
# bytes via cmp (including NUL/non-UTF8), and proves that install was never invoked.
identity_rejection() {
  local name=$1 expected=$2 body=$3 mode=$4
  local release="$TMP/$name-release" bin="$TMP/$name-bin"
  mkdir -p "$release" "$bin"
  printf 'existing executable\000\377\r\nno-final-newline' > "$bin/vesica"
  cp "$bin/vesica" "$TMP/$name-preimage"
  if [[ "$name" == missing ]]; then
    printf 'release metadata\n' > "$release/metadata"
    tar -C "$release" -czf "$release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" metadata
  else
    printf '%s' "$body" > "$release/vesica"
    chmod "$mode" "$release/vesica"
    tar -C "$release" -czf "$release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" vesica
  fi
  (cd "$release" && sha256sum "vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" > SHA256SUMS)
  rm -f "$VESICA_CALLS"
  if VESICA_BIN_DIR="$bin" sh "$INSTALLER" --from-file "$release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" --bootstrap-only >"$TMP/$name-out" 2>&1; then
    fail "checksum-valid $name artifact was accepted"
  fi
  grep -q "$expected" "$TMP/$name-out" || fail "$name rejection did not report $expected"
  cmp -s "$bin/vesica" "$TMP/$name-preimage" || fail "$name validation changed existing bytes"
  [[ ! -e "$VESICA_CALLS" ]] || fail "$name rejection invoked install"
  shopt -s dotglob nullglob
  local entries=("$bin"/*)
  shopt -u dotglob nullglob
  assert_eq "${#entries[@]}" 1 "$name rejection left target-side artifacts"
  [[ "${entries[0]}" == "$bin/vesica" ]] || fail "$name rejection changed target layout"
  printf 'PASS: bootstrap rejects %s and preserves existing bytes\n' "$name"
}
identity_rejection missing 'does not contain vesica' '' 0755
identity_rejection non-executable 'version probe failed' $'#!/bin/sh\nprintf "vesica %s\\n" "${VESICA_VERSION#v}"\n' 0644
identity_rejection failed-command 'version probe failed' $'#!/does/not/exist\n' 0755
identity_rejection malformed 'version mismatch' $'#!/bin/sh\nprintf "vesica %s\\nextra output\\n" "${VESICA_VERSION#v}"\n' 0755
identity_rejection empty-output 'version mismatch' $'#!/bin/sh\nexit 0\n' 0755

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

printf 'installer tests passed (platform, verified identity success, missing/nonexec/failed/malformed/nonzero/wrong identity preservation, forwarding, checksum rejection)\n'
