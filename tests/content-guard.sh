#!/usr/bin/env bash
# Tests for scripts/content-guard.sh (plan §5.5 content-free guard; B8 acceptance: the guard
# fails on a seeded organisation-name string and passes without it). Runs against a scratch copy of the
# tracked files, never the real checkout.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
GUARD="$ROOT/scripts/content-guard.sh"
if [[ -n "${VESICA_TEST_TMP_ROOT:-}" ]]; then export TMPDIR="$VESICA_TEST_TMP_ROOT"; fi
TMP=$(mktemp -d "${TMPDIR:-/tmp}/vesica-guard-test.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

fresh_copy() {
  rm -rf "$TMP/repo"
  mkdir -p "$TMP/repo"
  (cd "$ROOT" && git ls-files -z | xargs -0 cp --parents -t "$TMP/repo")
}
expect_pass() {
  bash "$GUARD" "$TMP/repo" >"$TMP/out" 2>&1 || { cat "$TMP/out" >&2; fail "$1: guard failed on clean content"; }
}
expect_fail() { # <case> <rule-id>
  if bash "$GUARD" "$TMP/repo" >"$TMP/out" 2>&1; then fail "$1: guard passed seeded content"; fi
  grep -q "$2" "$TMP/out" || { cat "$TMP/out" >&2; fail "$1: expected rule $2"; }
  # Findings name the file and line, never the matched text.
  ! grep -q "$3" "$TMP/out" || fail "$1: guard echoed the matched text"
}

# Pieces are joined at runtime so this file never contains the forbidden strings itself.
ORG="boost""CX"
DOMAIN="goalline""solutions.com"
GUID="3f2a1b4c-9d8e-4f7a-b6c5-""d4e3f2a1b0c9"
HOST="bcx-prod.openai.azure"".com"
AWS_KEY="AKIA""Q2RSTUVWXYZ23456"

fresh_copy
expect_pass "clean tracked files"

fresh_copy; printf 'Contact the %s team.\n' "$ORG" >>"$TMP/repo/README.md"
expect_fail "organisation name" "VCG001" "$ORG"

fresh_copy; printf 'mail: someone@%s\n' "$DOMAIN" >>"$TMP/repo/docs/ARCHITECTURE.md"
expect_fail "company domain" "VCG001" "$DOMAIN"

fresh_copy; printf 'tenant = "%s"\n' "$GUID" >>"$TMP/repo/README.md"
expect_fail "tenant GUID" "VCG002" "$GUID"

fresh_copy; printf 'tenant = "00000000-0000-0000-0000-000000000000"\n' >>"$TMP/repo/README.md"
expect_pass "all-zero placeholder GUID"

fresh_copy; printf 'base_url = "https://%s/openai/v1"\n' "$HOST" >>"$TMP/repo/README.md"
expect_fail "internal Azure hostname" "VCG003" "$HOST"

fresh_copy; printf 'base_url = "https://example.openai.azure.com/openai/v1"\n' >>"$TMP/repo/README.md"
expect_pass "example hostname placeholder"

fresh_copy; mkdir -p "$TMP/repo/docs/skills/leak"; printf '# skill\n' >"$TMP/repo/docs/skills/leak/SKILL.md"
expect_fail "SKILL.md outside fixtures" "VCG004" "^$"

fresh_copy; printf '# rules\n' >"$TMP/repo/AGENTS.md"
expect_fail "AGENTS.md outside fixtures" "VCG004" "^$"

if command -v gitleaks >/dev/null 2>&1; then
  fresh_copy; printf 'aws_access_key_id = %s\n' "$AWS_KEY" >>"$TMP/repo/docs/ONBOARDING.md"
  expect_fail "gitleaks secret" "VCG005" "$AWS_KEY"
  gitleaks_case="gitleaks"
elif [[ "${VESICA_GUARD_REQUIRE_GITLEAKS:-0}" == 1 ]]; then
  fail "gitleaks is required but not installed"
else
  gitleaks_case="gitleaks skipped (not installed)"
fi

printf 'content-guard tests passed (org name, domain, GUID, hostname, SKILL/AGENTS placement, %s)\n' "$gitleaks_case"
