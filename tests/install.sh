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
  chmod 0700 "$bin/vesica"
  cp -p "$bin/vesica" "$TMP/$name-preimage"
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
  [[ -f "$bin/vesica" && ! -L "$bin/vesica" ]] || fail "$name validation changed target type"
  assert_eq "$(stat -c %a "$bin/vesica")" 700 "$name validation changed target mode"
  ! grep -q " installed to " "$TMP/$name-out" || fail "$name validation falsely reported success"
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
identity_rejection nul-output 'version mismatch' $'#!/bin/sh\nprintf \"vesica %s\\000\\n\" \"${VESICA_VERSION#v}\"\n' 0755

# Cleanup owns only artifacts created by this invocation, never caller environment paths.
inherited_cleanup_path_rejection() {
  local name=$1 variable=$2 failure=$3 victim="$TMP/inherited-$1-victim"
  local bin="$TMP/inherited-$1-bin" archive expected
  mkdir -p "$bin"
  printf 'owner bytes\000\377\r\nno-final-newline' > "$bin/vesica"
  chmod 0700 "$bin/vesica"
  if [[ "$variable" == STAGED ]]; then
    victim="$bin/vesica"
  else
    mkdir -p "$victim"
    printf 'owner directory sentinel\000\377' > "$victim/sentinel"
    chmod 0700 "$victim"
  fi
  cp -p "$bin/vesica" "$TMP/inherited-$name-before"
  if [[ "$variable" == WORK ]]; then cp -p "$victim/sentinel" "$TMP/inherited-$name-sentinel"; fi
  archive="$TMP/mismatch-release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz"
  expected='version mismatch'
  local path="$PATH" platform=Linux
  if [[ "$failure" == platform ]]; then
    path="$TMP/fake-bin:$PATH"; platform=Darwin; expected='only Linux hosts are supported'
  fi
  if env "$variable=$victim" PATH="$path" VESICA_TEST_UNAME_S="$platform" VESICA_BIN_DIR="$bin" sh "$INSTALLER" --from-file "$archive" --bootstrap-only >"$TMP/inherited-$name-out" 2>&1; then
    fail "inherited $name unexpectedly succeeded"
  fi
  grep -q "$expected" "$TMP/inherited-$name-out" || fail "inherited $name failed at wrong boundary"
  [[ -f "$bin/vesica" && ! -L "$bin/vesica" ]] || fail "inherited $name deleted/replaced existing binary"
  cmp -s "$bin/vesica" "$TMP/inherited-$name-before" || fail "inherited $name changed existing binary"
  assert_eq "$(stat -c %a "$bin/vesica")" 700 "inherited $name changed binary mode"
  if [[ "$variable" == WORK ]]; then
    [[ -d "$victim" && ! -L "$victim" ]] || fail "inherited $name deleted owner directory"
    cmp -s "$victim/sentinel" "$TMP/inherited-$name-sentinel" || fail "inherited $name changed sentinel"
    assert_eq "$(stat -c %a "$victim")" 700 "inherited $name changed directory mode"
  fi
  printf 'PASS: bootstrap ignores inherited %s cleanup path on %s rejection\n' "$variable" "$failure"
}
if [[ "${VESICA_TEST_CLEANUP_ROW:-all}" == all || "${VESICA_TEST_CLEANUP_ROW:-}" == staged ]]; then inherited_cleanup_path_rejection staged STAGED version; fi
if [[ "${VESICA_TEST_CLEANUP_ROW:-all}" == all || "${VESICA_TEST_CLEANUP_ROW:-}" == work ]]; then inherited_cleanup_path_rejection work WORK platform; fi

# Destination-side copy/rename failures must never damage a previously installed binary.
bootstrap_replacement_failure() {
  local operation=$1 bin="$TMP/replacement-$1-bin" shims="$TMP/replacement-$1-shims"
  mkdir -p "$bin" "$shims"
  printf 'old executable\000\377\r\nno-final-newline' > "$bin/vesica"
  chmod 0700 "$bin/vesica"
  cp -p "$bin/vesica" "$TMP/replacement-$1-before"
  if [[ "$operation" == copy ]]; then
    cat > "$shims/install" <<'SHIM'
#!/bin/sh
# Record the real copy destination, write a partial result, and fail synchronously.
for target do :; done
printf '%s\n' "$target" > "$VESICA_BOOTSTRAP_COPY_RECEIPT"
printf 'partial new executable' > "$target"
exit 29
SHIM
  else
    cat > "$shims/mv" <<'SHIM'
#!/bin/sh
printf 'injected rename failure\n' >&2
exit 30
SHIM
  fi
  chmod +x "$shims"/*
  if PATH="$shims:$PATH" VESICA_BOOTSTRAP_COPY_RECEIPT="$TMP/copy-receipt" VESICA_BIN_DIR="$bin" sh "$INSTALLER" --from-file "$TMP/release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" --bootstrap-only >"$TMP/replacement-$1-out" 2>&1; then
    fail "bootstrap accepted injected $operation failure"
  fi
  cmp -s "$bin/vesica" "$TMP/replacement-$1-before" || fail "$operation failure damaged old executable"
  assert_eq "$(stat -c %a "$bin/vesica")" 700 "$operation failure changed old executable mode"
  if [[ "$operation" == copy ]]; then
    [[ "$(<"$TMP/copy-receipt")" == "$bin"/.vesica-bootstrap.* ]] || fail "copy was not staged on target filesystem"
  fi
  shopt -s dotglob nullglob
  local entries=("$bin"/*)
  shopt -u dotglob nullglob
  assert_eq "${#entries[@]}" 1 "$operation failure left owned staging artifacts"
  printf 'PASS: bootstrap %s failure preserves existing bytes/mode and cleans staging\n' "$operation"
}
bootstrap_replacement_failure copy
bootstrap_replacement_failure rename

# Report a retained owned sibling when preparation and its cleanup both fail.
mkdir -p "$TMP/cleanup-bin" "$TMP/cleanup-shims"
printf 'old binary\000\377' > "$TMP/cleanup-bin/vesica"
chmod 0700 "$TMP/cleanup-bin/vesica"
cp -p "$TMP/cleanup-bin/vesica" "$TMP/cleanup-before"
cat > "$TMP/cleanup-shims/install" <<'SHIM'
#!/bin/sh
for target do :; done
printf '%s\n' "$target" > "$VESICA_BOOTSTRAP_COPY_RECEIPT"
printf 'partial candidate' > "$target"
exit 29
SHIM
cat > "$TMP/cleanup-shims/rm" <<'SHIM'
#!/bin/sh
for arg do
  case "$arg" in */.vesica-bootstrap.*) printf 'injected staging unlink failure\n' >&2; exit 31 ;; esac
 done
exec /bin/rm "$@"
SHIM
chmod +x "$TMP/cleanup-shims"/*
if PATH="$TMP/cleanup-shims:$PATH" VESICA_BOOTSTRAP_COPY_RECEIPT="$TMP/cleanup-receipt" VESICA_BIN_DIR="$TMP/cleanup-bin" sh "$INSTALLER" --from-file "$TMP/release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" --bootstrap-only >"$TMP/cleanup-out" 2>&1; then
  fail 'bootstrap accepted preparation/cleanup failure'
fi
retained_stage=$(<"$TMP/cleanup-receipt")
[[ "$retained_stage" == "$TMP/cleanup-bin"/.vesica-bootstrap.* ]] || fail 'cleanup receipt was not owned staging'
[[ -f "$retained_stage" ]] || fail 'cleanup failure did not retain staged file'
grep -q 'cannot prepare staged executable; existing binary unchanged' "$TMP/cleanup-out" || fail 'cleanup failure hid primary error'
grep -Fq "staging cleanup failed; retained $retained_stage" "$TMP/cleanup-out" || fail 'cleanup error omitted exact retained path'
cmp -s "$TMP/cleanup-bin/vesica" "$TMP/cleanup-before" || fail 'cleanup failure changed old binary'
assert_eq "$(stat -c %a "$TMP/cleanup-bin/vesica")" 700 'cleanup failure changed old mode'
printf 'PASS: bootstrap cleanup failure preserves old binary and reports retained owned staging\n'

# Refuse non-regular existing targets without following or replacing them.
for shape in live-link dangling-link directory; do
  bin="$TMP/type-$shape-bin"
  mkdir -p "$bin"
  printf 'unmanaged referent bytes\000\377' > "$TMP/type-$shape-outside"
  cp "$TMP/type-$shape-outside" "$TMP/type-$shape-before"
  case "$shape" in
    live-link) ln -s "$TMP/type-$shape-outside" "$bin/vesica" ;;
    dangling-link) ln -s "$TMP/absent-referent" "$bin/vesica" ;;
    directory) mkdir "$bin/vesica"; printf 'owner directory' > "$bin/vesica/sentinel" ;;
  esac
  if VESICA_BIN_DIR="$bin" sh "$INSTALLER" --from-file "$TMP/release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" --bootstrap-only >"$TMP/type-$shape-out" 2>&1; then
    fail "bootstrap accepted existing $shape"
  fi
  grep -Eq 'symlink|not a regular file' "$TMP/type-$shape-out" || fail "missing target-type refusal"
  cmp -s "$TMP/type-$shape-outside" "$TMP/type-$shape-before" || fail "bootstrap wrote through $shape referent"
  case "$shape" in
    live-link) assert_eq "$(readlink "$bin/vesica")" "$TMP/type-$shape-outside" "live link changed" ;;
    dangling-link) assert_eq "$(readlink "$bin/vesica")" "$TMP/absent-referent" "dangling link changed" ;;
    directory) assert_eq "$(<"$bin/vesica/sentinel")" 'owner directory' "directory changed" ;;
  esac
  printf 'PASS: bootstrap refuses %s and preserves unmanaged nodes\n' "$shape"
done

# A successful replacement publishes a new inode rather than truncating a hardlinked old binary.
mkdir -p "$TMP/hardlink-bin"
printf 'old hardlinked executable' > "$TMP/hardlink-bin/vesica"
ln "$TMP/hardlink-bin/vesica" "$TMP/hardlink-outside"
VESICA_BIN_DIR="$TMP/hardlink-bin" sh "$INSTALLER" --from-file "$TMP/release/vesica-${VESICA_VERSION}-x86_64-linux.tar.gz" --bootstrap-only >"$TMP/hardlink-out" 2>&1
assert_eq "$(<"$TMP/hardlink-outside")" 'old hardlinked executable' 'successful bootstrap truncated old hardlink'
assert_eq "$("$TMP/hardlink-bin/vesica" --version)" "vesica ${VESICA_VERSION#v}" 'replacement identity differs'
assert_eq "$(stat -c %a "$TMP/hardlink-bin/vesica")" 755 'replacement mode differs'
shopt -s dotglob nullglob
hardlink_entries=("$TMP/hardlink-bin"/*)
shopt -u dotglob nullglob
assert_eq "${#hardlink_entries[@]}" 1 'success left staging artifacts'
printf 'PASS: bootstrap successful replacement preserves old hardlink and cleans staging\n'

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
