#!/usr/bin/env bash
# Tests for scripts/build-release.sh (plan §5.5 / B8 acceptance: a dry-run release produces a
# static binary plus checksums). Builds the real x86_64 musl artifact into a scratch directory;
# nothing is published.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
if [[ -n "${VESICA_TEST_TMP_ROOT:-}" ]]; then export TMPDIR="$VESICA_TEST_TMP_ROOT"; fi
TMP=$(mktemp -d "${TMPDIR:-/tmp}/vesica-release-test.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -n 1)
[[ -n "$VERSION" ]] || fail "could not read the workspace version"
TAG="v$VERSION"
ASSET="vesica-$TAG-x86_64-linux.tar.gz"

# A tag that does not match the Cargo version is refused before building.
if bash "$ROOT/scripts/build-release.sh" v9.9.9 "$TMP/wrong" >"$TMP/out" 2>&1; then
  fail "mismatched tag was accepted"
fi
grep -q 'does not match the workspace version' "$TMP/out" || { cat "$TMP/out" >&2; fail "mismatch not reported"; }
[[ ! -e "$TMP/wrong/$ASSET" ]] || fail "artifact produced for a mismatched tag"

# A matching dry-run build produces the tarball and checksums.
bash "$ROOT/scripts/build-release.sh" "$TAG" "$TMP/dist" >"$TMP/build.out" 2>&1 || { cat "$TMP/build.out" >&2; fail "release build failed"; }
[[ -f "$TMP/dist/$ASSET" && -f "$TMP/dist/SHA256SUMS" ]] || fail "missing $ASSET or SHA256SUMS"
(cd "$TMP/dist" && sha256sum -c SHA256SUMS >/dev/null) || fail "SHA256SUMS does not verify"
[[ "$(tar -tzf "$TMP/dist/$ASSET")" == "vesica" ]] || fail "tarball must contain exactly the vesica binary"

mkdir -p "$TMP/unpacked" && tar -xzf "$TMP/dist/$ASSET" -C "$TMP/unpacked"
BIN="$TMP/unpacked/vesica"
readelf -hW "$BIN" | grep -q 'Machine:.*X86-64' || fail "binary is not x86-64"
! readelf -lW "$BIN" | grep -q 'INTERP' || fail "binary requests a dynamic loader"
! readelf -dW "$BIN" | grep -q 'NEEDED' || fail "binary has shared-library dependencies"
[[ "$("$BIN" --version)" == "vesica $VERSION" ]] || fail "binary reports the wrong version"

# The shipped bootstrap installs this exact artifact.
VESICA_VERSION="$TAG" VESICA_BIN_DIR="$TMP/bin" HOME="$TMP/home" \
  sh "$ROOT/install.sh" --from-file "$TMP/dist/$ASSET" --bootstrap-only >/dev/null
cmp -s "$BIN" "$TMP/bin/vesica" || fail "bootstrap did not install the released binary"

printf 'release tests passed (%s: static x86-64, checksums, bootstrap install; mismatched tag refused)\n' "$ASSET"
