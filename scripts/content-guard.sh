#!/usr/bin/env bash
# Content-free guard for the public Vesica app repository (plan §5.5, task B8).
#
# The app must hold no team content. This fails (exit 1) when tracked content contains:
#   VCG001  company/organisation names or domains
#   VCG002  tenant/subscription GUIDs (the all-zero placeholder is allowed)
#   VCG003  internal/Azure resource hostnames (example/placeholder names are allowed)
#   VCG004  SKILL.md or AGENTS.md outside tests/fixtures/
#   VCG005  secrets found by gitleaks (when installed; required with VESICA_GUARD_REQUIRE_GITLEAKS=1)
# Findings name the rule, file and line only, never the matched text.
#
#   bash scripts/content-guard.sh [directory]     # default: repository root
set -euo pipefail

ROOT=$(cd "${1:-$(dirname "${BASH_SOURCE[0]}")/..}" && pwd)
cd "$ROOT"

# Patterns are written so that this file does not match itself.
ORG_PATTERN='boost[c]x|goallinesolution[s]'
GUID_PATTERN='[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}'
HOST_PATTERN='[a-z0-9][a-z0-9-]*\.(openai\.azure\.com|cognitiveservices\.azure\.com|services\.ai\.azure\.com|azurewebsites\.net|azure-api\.net|vault\.azure\.net|database\.windows\.net|(blob|file|queue|table|dfs)\.core\.windows\.net|azurecr\.io)|[a-z0-9][a-z0-9.-]*\.(internal|corp)\b'
PLACEHOLDER_HOST='^(example|placeholder|your-[a-z0-9-]*|my-[a-z0-9-]*)\.'

FINDINGS=0
finding() { FINDINGS=$((FINDINGS + 1)); printf 'content-guard: %s %s:%s %s\n' "$1" "$2" "$3" "$4"; }

FILES="$(mktemp)"
trap 'rm -f "$FILES"' EXIT
if git -C "$ROOT" rev-parse --show-toplevel >/dev/null 2>&1 && [[ "$(git -C "$ROOT" rev-parse --show-toplevel)" == "$ROOT" ]]; then
  git ls-files -z >"$FILES"
  MODE=git
else
  find . -type f -not -path './.git/*' -not -path './target/*' -printf '%P\0' >"$FILES"
  MODE=dir
fi

# grep -I skips binary files; -o with -n lets us filter individual matches per line.
scan() { # <rule> <pattern> <message> [allow-filter-regex]
  local rule=$1 pattern=$2 message=$3 allow=${4:-}
  while IFS=: read -r file line match; do
    [[ -n "$allow" && "$match" =~ $allow ]] && continue
    finding "$rule" "$file" "$line" "$message"
  done < <(xargs -0 -r grep -I -n -o -i -E "$pattern" -- <"$FILES" 2>/dev/null || true)
}

scan VCG001 "$ORG_PATTERN" "company or organisation name"
scan VCG002 "$GUID_PATTERN" "tenant/subscription GUID" '^0{8}-0{4}-0{4}-0{4}-0{12}$'
scan VCG003 "$HOST_PATTERN" "internal or Azure resource hostname" "$PLACEHOLDER_HOST"

while IFS= read -r -d '' file; do
  case "$file" in
    tests/fixtures/*) ;;
    SKILL.md | */SKILL.md | AGENTS.md | */AGENTS.md)
      finding VCG004 "$file" 1 "SKILL.md/AGENTS.md belongs in the private shared home, not the app" ;;
  esac
done <"$FILES"

if command -v gitleaks >/dev/null 2>&1; then
  report="$(mktemp)"
  if [[ "$MODE" == git ]]; then
    gitleaks git --no-banner --redact --exit-code 2 --report-format csv --report-path "$report" . >/dev/null 2>&1 && rc=0 || rc=$?
  else
    gitleaks dir --no-banner --redact --exit-code 2 --report-format csv --report-path "$report" . >/dev/null 2>&1 && rc=0 || rc=$?
  fi
  if [[ $rc -eq 2 ]]; then
    # CSV columns: RuleID,Commit,File,SymlinkFile,Secret,Match,StartLine,... (secret is redacted).
    while IFS=, read -r rule _ file _ _ _ line _; do
      finding VCG005 "$file" "$line" "secret detected by gitleaks rule $rule"
    done < <(tail -n +2 "$report")
  elif [[ $rc -ne 0 ]]; then
    rm -f "$report"; echo "content-guard: gitleaks failed to run (exit $rc)" >&2; exit 1
  fi
  rm -f "$report"
elif [[ "${VESICA_GUARD_REQUIRE_GITLEAKS:-0}" == 1 ]]; then
  echo "content-guard: gitleaks is required but not installed" >&2
  exit 1
fi

if [[ $FINDINGS -gt 0 ]]; then
  printf 'content-guard: %d finding(s); the public app repository must stay content-free\n' "$FINDINGS"
  exit 1
fi
printf 'content-guard: clean (%s mode%s)\n' "$MODE" "$(command -v gitleaks >/dev/null 2>&1 && echo ', gitleaks' || echo ', gitleaks not installed')"
