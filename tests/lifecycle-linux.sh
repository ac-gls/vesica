#!/usr/bin/env bash
# SC2016: single-quoted `sh -c` scripts receive their values as positional arguments on purpose.
# shellcheck disable=SC2016
# Linux lifecycle evidence for the Vesica plan's S7 (install/personal-vault lifecycle) and
# S8 (personal-vault / shared-home Git isolation) spikes.
#
# Everything runs in a disposable temporary HOME/fixture tree that is deleted on exit. It never
# touches the real home, a real vault, or a real shared-home repository, and makes no network calls.
#
#   VESICA_BIN=target/release/vesica bash tests/lifecycle-linux.sh
#
# Environment:
#   VESICA_BIN              binary under test (default: target/release/vesica)
#   VESICA_EVIDENCE         path for the Markdown evidence report (default: ./lifecycle-evidence.md)
#   VESICA_REQUIRE_NATIVE=1 fail if the host looks like WSL or a container (set in CI on VM runners)
#   VESICA_TEST_TMP_ROOT    exec-capable directory for temporary fixtures (default: $TMPDIR or /tmp)
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
BIN=$(realpath "${VESICA_BIN:-$ROOT/target/release/vesica}")
EVIDENCE=${VESICA_EVIDENCE:-$PWD/lifecycle-evidence.md}
[[ -x "$BIN" ]] || { echo "FAIL: vesica binary not found at $BIN (build with cargo build --release -p vesica)" >&2; exit 1; }

# Read the commit before the scratch git config replaces the caller's (e.g. safe.directory).
COMMIT=${GITHUB_SHA:-$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)}
if [[ -n "${VESICA_TEST_TMP_ROOT:-}" ]]; then export TMPDIR="$VESICA_TEST_TMP_ROOT"; fi
TMP=$(mktemp -d "${TMPDIR:-/tmp}/vesica-lifecycle.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
export HOME="$TMP/home"
mkdir -p "$HOME"
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$TMP/gitconfig"
git config --global user.name "Vesica Lifecycle"
git config --global user.email "lifecycle@example.invalid"
git config --global init.defaultBranch main
git config --global advice.detachedHead false
# Never let the outer environment redirect Vesica at a real checkout.
unset VESICA_HOME_REPO AGENT_HOME_REPO VESICA_ROOT AGENT_HOME_ROOT VESICA_TEAM_HOME VESICA_ALLOW_NON_WSL AGENT_HOME_ALLOW_NON_WSL || true

PASSES=0
FAILURES=0
RESULTS=()
pass() { PASSES=$((PASSES + 1)); RESULTS+=("| $1 | $2 | PASS | ${3:-} |"); printf 'PASS %s %s\n' "$1" "$2"; }
failc() { FAILURES=$((FAILURES + 1)); RESULTS+=("| $1 | $2 | **FAIL** | ${3:-} |"); printf 'FAIL %s %s %s\n' "$1" "$2" "${3:-}" >&2; }
# check <spike> <name> <command...>: pass if the command succeeds.
check() { local s=$1 n=$2; shift 2; if "$@" >"$TMP/last.out" 2>&1; then pass "$s" "$n"; else failc "$s" "$n" "$(tr '\n' ' ' <"$TMP/last.out" | cut -c1-160)"; fi; }
# refuse <spike> <name> <expected-text> <command...>: pass if the command fails and prints the text.
refuse() {
  local s=$1 n=$2 want=$3; shift 3
  if "$@" >"$TMP/last.out" 2>&1; then failc "$s" "$n" "command unexpectedly succeeded"
  elif grep -q -- "$want" "$TMP/last.out"; then pass "$s" "$n" "refused: $(grep -m1 -- "$want" "$TMP/last.out" | cut -c1-120)"
  else failc "$s" "$n" "failed without expected text '$want': $(tr '\n' ' ' <"$TMP/last.out" | cut -c1-160)"; fi
}
# Deterministic listing of a tree: type, relative path, link target or content hash.
snapshot() {
  (cd "$1" && find . -mindepth 1 -not -path './.git/*' -not -name '.git' -print0 | LC_ALL=C sort -z | while IFS= read -r -d '' p; do
    if [[ -L "$p" ]]; then printf 'l %s -> %s\n' "$p" "$(readlink "$p")"
    elif [[ -f "$p" ]]; then printf 'f %s %s %s\n' "$p" "$(stat -c %a "$p")" "$(sha256sum <"$p" | cut -d' ' -f1)"
    else printf 'd %s\n' "$p"; fi
  done)
}
same() { cmp -s "$1" "$2"; }
# expect <spike> <name> <detail> <command...>: like check, but records a fixed detail string.
expect() { local s=$1 n=$2 d=$3; shift 3; if "$@" >/dev/null 2>&1; then pass "$s" "$n" "$d"; else failc "$s" "$n" "$d"; fi; }

# ---------------------------------------------------------------- host record
# shellcheck disable=SC1091
OS=$( (. /etc/os-release 2>/dev/null && printf '%s' "${PRETTY_NAME:-unknown}") || echo unknown)
KERNEL=$(uname -sr)
ARCH=$(uname -m)
FS=$(stat -f -c %T "$TMP")
VERSION=$("$BIN" --version)
PLATFORM="native Linux"
if grep -qi microsoft /proc/version 2>/dev/null; then PLATFORM="WSL (supplemental, not native evidence)"
elif [[ -f /.dockerenv ]] || grep -qaE '(docker|containerd|kubepods|lxc)' /proc/1/cgroup 2>/dev/null \
  || { command -v systemd-detect-virt >/dev/null && systemd-detect-virt --container >/dev/null 2>&1; }; then
  PLATFORM="container (supplemental, not native evidence)"
fi
VIRT=$( (command -v systemd-detect-virt >/dev/null && systemd-detect-virt 2>/dev/null) || echo unknown)

expect S7 "host is Linux" "$KERNEL" test "$(uname -s)" = Linux
expect S7 "host is x86_64" "$ARCH" test "$ARCH" = x86_64
if [[ "${VESICA_REQUIRE_NATIVE:-0}" == 1 ]]; then
  expect S7 "host is native (not WSL/container)" "$PLATFORM; virt=$VIRT" test "$PLATFORM" = "native Linux"
fi

# Disposable copy of the fixture shared home so link targets and hashes live in the scratch tree.
cp -a "$ROOT/tests/fixtures/shared-home" "$TMP/shared"
snapshot "$TMP/shared" >"$TMP/shared.before"

# ---------------------------------------------------------------- S7: bootstrap with the real binary
mkdir -p "$TMP/release"
cp "$BIN" "$TMP/release/vesica"
tar --mtime='@0' --owner=0 --group=0 --numeric-owner -C "$TMP/release" -czf "$TMP/release/vesica-v0.1.0-x86_64-linux.tar.gz" vesica
(cd "$TMP/release" && sha256sum vesica-v0.1.0-x86_64-linux.tar.gz >SHA256SUMS)
export VESICA_BIN_DIR="$TMP/bin"
H1="$TMP/h-bootstrap"
check S7 "bootstrap verifies checksum, installs binary and runs install" \
  sh "$ROOT/install.sh" --from-file "$TMP/release/vesica-v0.1.0-x86_64-linux.tar.gz" --home-repo "$TMP/shared" --root "$H1"
check S7 "bootstrapped binary is byte-identical to the build" cmp -s "$BIN" "$VESICA_BIN_DIR/vesica"
check S7 "bootstrap-driven install recorded a manifest" test -f "$H1/.config/agent-home/manifest.json"
printf 'tampered\n' >>"$TMP/release/vesica-v0.1.0-x86_64-linux.tar.gz"
rm -rf "$TMP/bin2"
refuse S7 "bootstrap rejects a checksum mismatch" "checksum" \
  env VESICA_BIN_DIR="$TMP/bin2" sh "$ROOT/install.sh" --from-file "$TMP/release/vesica-v0.1.0-x86_64-linux.tar.gz" --bootstrap-only
check S7 "rejected bootstrap left no binary" test ! -e "$TMP/bin2/vesica"
mkdir -p "$TMP/fake-bin"
cat >"$TMP/fake-bin/uname" <<'UNAME'
#!/bin/sh
case "${1:-}" in
  -s) printf '%s\n' "${FAKE_S:-Linux}" ;;
  -m) printf '%s\n' "${FAKE_M:-x86_64}" ;;
  *) exit 1 ;;
esac
UNAME
chmod +x "$TMP/fake-bin/uname"
[[ "$("$TMP/fake-bin/uname" -s)" == Linux ]] || { echo "FAIL: cannot execute shims under $TMP (noexec?); set VESICA_TEST_TMP_ROOT" >&2; exit 1; }
refuse S7 "bootstrap refuses a non-Linux host" "only Linux hosts are supported" \
  env PATH="$TMP/fake-bin:$PATH" FAKE_S=Darwin VESICA_BIN_DIR="$TMP/bin3" sh "$ROOT/install.sh" --bootstrap-only
refuse S7 "bootstrap refuses a non-x86_64 host" "only x86_64 is supported" \
  env PATH="$TMP/fake-bin:$PATH" FAKE_M=aarch64 VESICA_BIN_DIR="$TMP/bin3" sh "$ROOT/install.sh" --bootstrap-only

# ---------------------------------------------------------------- S7: doctor, dry-run, install, idempotence
check S7 "doctor accepts the fixture shared home" "$BIN" doctor --home-repo "$TMP/shared"
if "$BIN" doctor --json --home-repo "$TMP/shared" >"$TMP/doctor.json" 2>&1 \
  && python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); assert d["shared_home"]=="valid"; [d[k] for k in ("profile","repos","foundry","herdr","live_harness")]; assert d["foundry"]=="not-implemented" and d["herdr"]=="not-implemented"' "$TMP/doctor.json"; then
  pass S7 "doctor --json is valid and reports unimplemented integrations as not-implemented"
else failc S7 "doctor --json is valid and reports unimplemented integrations as not-implemented" "$(head -c 160 "$TMP/doctor.json")"; fi

H="$TMP/h"
mkdir -p "$H/.claude" "$H/notes"
printf '# my existing Claude rules\n' >"$H/.claude/CLAUDE.md"
printf 'personal note\n' >"$H/notes/private.md"
cp "$H/.claude/CLAUDE.md" "$TMP/claude.orig"
snapshot "$H" >"$TMP/h.pre"
check S7 "install --dry-run succeeds" sh -c '"$1" install --home-repo "$2" --root "$3" --dry-run >"$4" 2>&1' _ "$BIN" "$TMP/shared" "$H" "$TMP/dry.out"
snapshot "$H" >"$TMP/h.afterdry"
check S7 "install --dry-run leaves the home unchanged" same "$TMP/h.pre" "$TMP/h.afterdry"
check S7 "install succeeds" sh -c '"$1" install --home-repo "$2" --root "$3" >"$4" 2>&1' _ "$BIN" "$TMP/shared" "$H" "$TMP/install1.out"
grep '^DryRun: ' "$TMP/dry.out" | sed 's/^DryRun: //' | sort >"$TMP/plan.dry"
grep -E '^(Created|Updated|AlreadyCorrect|BackedUp[A-Za-z]*): ' "$TMP/install1.out" | sed -E 's/^[A-Za-z]+: //' | sort >"$TMP/plan.applied"
check S7 "dry-run plan matches the applied action targets" same "$TMP/plan.dry" "$TMP/plan.applied"
check S7 "Team is a link to the shared home" test "$(readlink "$H/Team")" = "$(realpath "$TMP/shared")"
check S7 "managed block added and existing Claude rules preserved" sh -c 'grep -q "my existing Claude rules" "$1" && grep -q ">>> vesica" "$1"' _ "$H/.claude/CLAUDE.md"
check S7 "skills linked for .agents and .claude" sh -c 'for s in demo-skill second-skill; do test -L "$1/.agents/skills/$s" && test -L "$1/.claude/skills/$s" || exit 1; done' _ "$H"
check S7 "status reports recorded entries" sh -c '"$1" status --root "$2" | grep -q "Recorded entries: [1-9]"' _ "$BIN" "$H"
check S7 "doctor passes after install" "$BIN" doctor --home-repo "$TMP/shared"
snapshot "$H" >"$TMP/h.install1"
check S7 "repeat install succeeds" sh -c '"$1" install --home-repo "$2" --root "$3" >"$4" 2>&1' _ "$BIN" "$TMP/shared" "$H" "$TMP/install2.out"
check S7 "repeat install reports every action AlreadyCorrect" sh -c '! grep -E "^(Created|Updated|Collision)" "$1" && grep -q "^AlreadyCorrect" "$1"' _ "$TMP/install2.out"
snapshot "$H" | grep -v ' ./.config/agent-home/manifest.json ' >"$TMP/h.install2.cmp"
grep -v ' ./.config/agent-home/manifest.json ' "$TMP/h.install1" >"$TMP/h.install1.cmp"
check S7 "repeat install leaves managed files and links unchanged" same "$TMP/h.install1.cmp" "$TMP/h.install2.cmp"
check S7 "repeat install leaves no manifest temp file" test ! -e "$H/.config/agent-home/manifest.json.tmp"

# ---------------------------------------------------------------- S7: safe refusal cases (fresh homes)
R1="$TMP/r-occupied"; mkdir -p "$R1"; printf 'user-owned destination\n' >"$R1/Team"
snapshot "$R1" >"$TMP/r1.before"
refuse S7 "install refuses an occupied destination" "error:" "$BIN" install --home-repo "$TMP/shared" --root "$R1"
snapshot "$R1" >"$TMP/r1.after"
check S7 "occupied-destination refusal changed nothing" same "$TMP/r1.before" "$TMP/r1.after"

R2="$TMP/r-state"; mkdir -p "$R2/.config" "$R2/.claude"; ln -s missing-target "$R2/.config/agent-home"; printf 'mine\n' >"$R2/.claude/CLAUDE.md"
snapshot "$R2" >"$TMP/r2.before"
refuse S7 "install refuses an obstructed state path" "error:" "$BIN" install --home-repo "$TMP/shared" --root "$R2"
snapshot "$R2" >"$TMP/r2.after"
check S7 "obstructed-state refusal changed nothing" same "$TMP/r2.before" "$TMP/r2.after"

R3="$TMP/r-tmp"
"$BIN" install --home-repo "$TMP/shared" --root "$R3" >/dev/null 2>&1
mkdir "$R3/.config/agent-home/manifest.json.tmp"
snapshot "$R3" >"$TMP/r3.before"
refuse S7 "uninstall refuses an obstructed manifest temp path" "error:" "$BIN" uninstall --root "$R3"
snapshot "$R3" >"$TMP/r3.after"
check S7 "obstructed-manifest uninstall refusal changed nothing" same "$TMP/r3.before" "$TMP/r3.after"

# ---------------------------------------------------------------- S7: uninstall
R4="$TMP/r-modified"
"$BIN" install --home-repo "$TMP/shared" --root "$R4" >/dev/null 2>&1
printf 'user-authored replacement\n' >"$R4/.codex/AGENTS.md"
"$BIN" uninstall --root "$R4" >"$TMP/r4.out" 2>&1 || true
check S7 "uninstall retains a user-modified managed file" sh -c 'grep -q "user-authored replacement" "$1"' _ "$R4/.codex/AGENTS.md"

check S7 "uninstall succeeds" "$BIN" uninstall --root "$H"
check S7 "uninstall removed Team and skill links" sh -c 'test ! -e "$1/Team" && test ! -L "$1/.agents/skills/demo-skill" && test ! -L "$1/.claude/skills/demo-skill"' _ "$H"
check S7 "uninstall restored pre-existing Claude rules byte-for-byte" cmp -s "$TMP/claude.orig" "$H/.claude/CLAUDE.md"
check S7 "uninstall kept the personal note" sh -c 'grep -qx "personal note" "$1"' _ "$H/notes/private.md"
snapshot "$TMP/shared" >"$TMP/shared.after"
check S7 "shared-home content unchanged by the whole lifecycle" same "$TMP/shared.before" "$TMP/shared.after"

# ---------------------------------------------------------------- S8: personal vault / shared home Git isolation
REMOTE="$TMP/s8/remote.git"; SEED="$TMP/s8/seed"; V="$TMP/s8/vault"; UP="$TMP/s8/upstream"
mkdir -p "$TMP/s8"
git init -q --bare "$REMOTE"
cp -a "$TMP/shared" "$SEED"
printf '* text=auto eol=lf\n' >"$SEED/.gitattributes"
printf '#!/bin/sh\necho shared\n' >"$SEED/tool.sh"
git -C "$SEED" init -q && git -C "$SEED" add -A && git -C "$SEED" commit -q -m seed && git -C "$SEED" push -q "$REMOTE" HEAD:main
mkdir -p "$V/Projects" "$V/Sessions/agent-runs"
printf 'Team/\n.obsidian/workspace*.json\n.trash/\n' >"$V/.gitignore"
printf '* text=auto eol=lf\n' >"$V/.gitattributes"
printf -- '---\ntitle: private\n---\npersonal only\n' >"$V/Projects/private.md"
git -C "$V" init -q && git -C "$V" add -A && git -C "$V" commit -q -m personal
git clone -q "$REMOTE" "$V/Team"
TEAM_TOP=$(git -C "$V/Team" rev-parse --show-toplevel)
VAULT_TOP=$(git -C "$V" rev-parse --show-toplevel)
check S8 "shared checkout is its own repository root" test "$TEAM_TOP" = "$(realpath "$V/Team")"
check S8 "personal vault is a separate repository root" test "$VAULT_TOP" = "$(realpath "$V")"
check S8 "outer repo ignores Team/" sh -c 'git -C "$1" check-ignore -q Team/README.md' _ "$V"
check S8 "outer repo tracks no shared files" sh -c '! git -C "$1" ls-files | grep -q "^Team/"' _ "$V"
check S8 "inner repo tracks no personal files" sh -c '! git -C "$1" ls-files | grep -q "private.md"' _ "$V/Team"
check S8 "fresh checkouts have no line-ending churn" sh -c 'test -z "$(git -C "$1" status --porcelain)" && test -z "$(git -C "$1/Team" status --porcelain)"' _ "$V"
printf 'edited personally\n' >>"$V/Projects/private.md"
printf 'edited shared\n' >>"$V/Team/README.md"
chmod +x "$V/Team/tool.sh"
git -C "$V" add -A
check S8 "outer 'git add -A' stages only the personal edit" sh -c 'test "$(git -C "$1" diff --cached --name-only)" = "Projects/private.md"' _ "$V"
check S8 "inner status shows only shared edits (content + exec bit)" sh -c 'test "$(git -C "$1" status --porcelain | sort | tr "\n" " ")" = " M README.md  M tool.sh "' _ "$V/Team"
check S8 "exec-bit change recorded as a mode change in the shared repo" sh -c 'git -C "$1" diff tool.sh | grep -q "new mode 100755"' _ "$V/Team"
git -C "$V" commit -q -m "personal edit"

check S8 "team status reports the dirty shared checkout" sh -c '"$1" team status --path "$2" | grep -q "Working tree: dirty"' _ "$BIN" "$V/Team"
cp "$V/Team/README.md" "$TMP/s8/readme.dirty"
HEAD0=$(git -C "$V/Team" rev-parse HEAD)
git clone -q "$REMOTE" "$UP"
printf 'upstream change\n' >"$UP/Knowledge/upstream.md" && git -C "$UP" add -A && git -C "$UP" commit -q -m upstream && git -C "$UP" push -q origin main
refuse S8 "team pull refuses a dirty checkout" "uncommitted changes" "$BIN" team pull --path "$V/Team"
check S8 "dirty refusal preserved local edits and HEAD" sh -c 'cmp -s "$1" "$2/README.md" && test "$(git -C "$2" rev-parse HEAD)" = "$3"' _ "$TMP/s8/readme.dirty" "$V/Team" "$HEAD0"
git -C "$V/Team" checkout -q -- README.md tool.sh
check S8 "team status reports a clean checkout" sh -c '"$1" team status --path "$2" | grep -q "Working tree: clean"' _ "$BIN" "$V/Team"
VAULT_HEAD=$(git -C "$V" rev-parse HEAD)
check S8 "team pull fast-forwards a clean checkout" sh -c '"$1" team pull --path "$2" | grep -q "^Fast-forwarded"' _ "$BIN" "$V/Team"
check S8 "pull moved shared HEAD to the remote head" test "$(git -C "$V/Team" rev-parse HEAD)" = "$(git -C "$UP" rev-parse HEAD)"
check S8 "pull left the personal repo untouched" sh -c 'test "$(git -C "$1" rev-parse HEAD)" = "$2" && test -z "$(git -C "$1" status --porcelain)"' _ "$V" "$VAULT_HEAD"
check S8 "team pull reports an up-to-date checkout" sh -c '"$1" team pull --path "$2" | grep -q "already up to date"' _ "$BIN" "$V/Team"
printf 'local commit\n' >"$V/Team/Knowledge/local.md" && git -C "$V/Team" add -A && git -C "$V/Team" commit -q -m local
printf 'remote commit\n' >"$UP/Knowledge/remote.md" && git -C "$UP" add -A && git -C "$UP" commit -q -m remote && git -C "$UP" push -q origin main
HEAD1=$(git -C "$V/Team" rev-parse HEAD)
refuse S8 "team pull refuses divergence" "fast-forward" "$BIN" team pull --path "$V/Team"
check S8 "divergence refusal kept local HEAD and files" sh -c 'test "$(git -C "$1" rev-parse HEAD)" = "$2" && test -f "$1/Knowledge/local.md" && test -z "$(git -C "$1" status --porcelain)"' _ "$V/Team" "$HEAD1"
git -C "$V/Team" fetch -q origin && git -C "$V/Team" reset -q --hard origin/main
git -C "$UP" reset -q --hard HEAD~1 && git -C "$UP" push -q -f origin main
HEAD2=$(git -C "$V/Team" rev-parse HEAD)
refuse S8 "team pull refuses a remote rewind" "rewound" "$BIN" team pull --path "$V/Team"
check S8 "rewind refusal kept local HEAD" test "$(git -C "$V/Team" rev-parse HEAD)" = "$HEAD2"
if command -v git-lfs >/dev/null 2>&1; then LFS="git-lfs present ($(git lfs version | cut -d' ' -f1)); LFS behavior not in the fixture contract yet — not tested"
else LFS="git-lfs absent; LFS not in the fixture contract yet — not tested"; fi

# ---------------------------------------------------------------- report
{
  printf '# Vesica Linux lifecycle evidence (S7/S8)\n\n'
  printf '| Field | Value |\n|---|---|\n'
  printf '| Platform | %s |\n| Virtualization | %s |\n| OS | %s |\n| Kernel | %s |\n| Arch | %s |\n| Scratch filesystem | %s |\n| Commit | `%s` |\n| Binary | %s |\n| Run | %s |\n| LFS | %s |\n' \
    "$PLATFORM" "$VIRT" "$OS" "$KERNEL" "$ARCH" "$FS" "$COMMIT" "$VERSION" "${GITHUB_SERVER_URL:+$GITHUB_SERVER_URL/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID}" "$LFS"
  printf '\n**Result: %s passed, %s failed.** Prototype lifecycle validation only; not production readiness.\n\n' "$PASSES" "$FAILURES"
  printf '| Spike | Check | Result | Detail |\n|---|---|---|---|\n'
  printf '%s\n' "${RESULTS[@]}"
} >"$EVIDENCE"
[[ -n "${GITHUB_STEP_SUMMARY:-}" ]] && cat "$EVIDENCE" >>"$GITHUB_STEP_SUMMARY"
printf '\n%s passed, %s failed (%s). Evidence: %s\n' "$PASSES" "$FAILURES" "$PLATFORM" "$EVIDENCE"
[[ "$FAILURES" -eq 0 ]]
