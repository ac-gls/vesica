#!/usr/bin/env bash
# Build a Vesica release artifact (plan §5.5 / B8):
#   static x86_64 musl binary -> vesica-<tag>-x86_64-linux.tar.gz + SHA256SUMS
#
#   bash scripts/build-release.sh <tag> [outdir]     # e.g. v0.1.0-alpha.1 dist
#
# Refuses (before building) when the tag does not match the workspace version or the
# bootstrap's default version, so a release can never ship an install.sh that points elsewhere.
# The binary must have no dynamic loader and no shared-library dependencies (static-PIE is
# accepted: it keeps ASLR, and older `file` versions label it "dynamically linked").
set -euo pipefail

die() { printf 'build-release: %s\n' "$*" >&2; exit 1; }
TAG=${1:-}
OUT=${2:-dist}
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
TARGET=x86_64-unknown-linux-musl

[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || die "tag must look like vX.Y.Z or vX.Y.Z-pre.N (got '${TAG}')"
VERSION=${TAG#v}
WORKSPACE_VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -n 1)
[[ "$VERSION" == "$WORKSPACE_VERSION" ]] || die "tag $TAG does not match the workspace version $WORKSPACE_VERSION"
# shellcheck disable=SC2016 # the ${...} below is literal install.sh text, not an expansion
BOOTSTRAP_VERSION=$(sed -n 's/^VERSION="\${VESICA_VERSION:-\${AGENT_HOME_VERSION:-\(v[^}]*\)}}"$/\1/p' "$ROOT/install.sh")
[[ "$BOOTSTRAP_VERSION" == "$TAG" ]] || die "install.sh defaults to ${BOOTSTRAP_VERSION:-<unparsed>}, not $TAG"

cd "$ROOT"
cargo build --release --locked --target "$TARGET" -p vesica
BIN="target/$TARGET/release/vesica"

readelf -hW "$BIN" | grep -q 'Machine:.*X86-64' || die "binary is not x86-64"
if readelf -lW "$BIN" | grep -q INTERP || readelf -dW "$BIN" | grep -q NEEDED; then
  die "binary is not statically linked"
fi
[[ "$("$BIN" --version)" == "vesica $VERSION" ]] || die "binary reports '$("$BIN" --version)', expected 'vesica $VERSION'"

ASSET="vesica-$TAG-x86_64-linux.tar.gz"
mkdir -p "$OUT"
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
install -m 0755 "$BIN" "$STAGE/vesica"
# Reproducible packaging: fixed owner, commit timestamp, sorted entries, no gzip timestamp.
EPOCH=$(git -C "$ROOT" log -1 --format=%ct 2>/dev/null || echo 0)
tar --sort=name --mtime="@$EPOCH" --owner=0 --group=0 --numeric-owner -C "$STAGE" -cf - vesica | gzip -n -9 >"$OUT/$ASSET"
(cd "$OUT" && sha256sum "$ASSET" >SHA256SUMS)

printf 'built %s\n' "$OUT/$ASSET"
printf 'file: %s\n' "$(file -b "$BIN")"
cat "$OUT/SHA256SUMS"
