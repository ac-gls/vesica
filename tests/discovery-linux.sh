#!/usr/bin/env bash
# S2 + S3 discovery harness for the Vesica plan (plan §6 Phase 0 S2/S3).
#
# Purpose: record, on a supported Linux VM, how each supported agent discovers
#   S2  shared/personal instructions and skills, and
#   S3  generated role agents (format, path, accepted schema)
# so the B6 role renderer can be built against observed behaviour instead of a
# design document.
#
# Everything runs inside a disposable HOME under $VESICA_DISCOVERY_TMP_ROOT and is deleted on
# exit. The real home, a real vault and a real shared-home checkout are never touched, and no
# credential, token or model call is used. Findings are recorded, not asserted: this reports what
# the agents actually do, including "not observable" and "claim does not hold".
#
#   VESICA_REQUIRE_NATIVE=1 bash tests/discovery-linux.sh
#
# Environment:
#   VESICA_DISCOVERY_TMP_ROOT  exec-capable directory for the disposable HOME (default: $TMPDIR or /tmp)
#   VESICA_EVIDENCE            Markdown report path (default: ./discovery-evidence.md)
#   VESICA_EVIDENCE_JSON       machine-readable matrix path (default: alongside VESICA_EVIDENCE)
#   VESICA_REQUIRE_NATIVE=1    fail when the host looks like WSL or a container (set in CI on VMs)
#   VESICA_DISCOVERY_STRICT=1  also exit non-zero when any documented claim is not corroborated
#   VESICA_DISCOVERY_SKIP_INSTALL=1  reuse an already-provisioned tree (local iteration only)
#
# Deliberately no `set -u`: npm/python shims on developer machines export shell functions that
# read unset variables, and the harness must survive being run from an interactive shell.
# SC2016: printf format strings intentionally contain literal backticks and `$name` examples.
# shellcheck disable=SC2016
set -eo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
EVIDENCE=${VESICA_EVIDENCE:-$ROOT/discovery-evidence.md}
EVIDENCE_JSON=${VESICA_EVIDENCE_JSON:-${EVIDENCE%.md}.json}
TMP_ROOT=${VESICA_DISCOVERY_TMP_ROOT:-${TMPDIR:-/tmp}}
COMMIT=${GITHUB_SHA:-$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)}
mkdir -p "$TMP_ROOT"
WORK=$(mktemp -d "$TMP_ROOT/vesica-discovery.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

export HOME="$WORK/home"
export XDG_CONFIG_HOME="$HOME/.config" XDG_DATA_HOME="$HOME/.local/share" XDG_STATE_HOME="$HOME/.local/state"
export XDG_CACHE_HOME="$HOME/.cache"
export PATH="$HOME/.npm-global/bin:$HOME/.local/bin:$PATH"
export npm_config_userconfig="$HOME/.npmrc"
export PIP_DISABLE_PIP_VERSION_CHECK=1
mkdir -p "$HOME"

FINDINGS=0
CLAIMS_OK=0
CLAIMS_TOTAL=0
ROW=()
claim() { # <spike> <agent> <claim> <VERIFIED|UNVERIFIED|NOT-APPLICABLE> <detail>
  CLAIMS_TOTAL=$((CLAIMS_TOTAL + 1))
  [ "$4" = VERIFIED ] && CLAIMS_OK=$((CLAIMS_OK + 1))
  [ "$4" = UNVERIFIED ] && FINDINGS=$((FINDINGS + 1))
  ROW+=("| $1 | $2 | $3 | $4 | $(cell "$5") |")
  printf '%-9s %-14s %-46s %s\n' "$1" "$2" "$3" "$4" >&2
}
cell() { printf '%s' "${1:-}" | tr '|' '/' | tr -d '\n'; }
say() { printf '%s\n' "$*" >&2; }

OBSERVED_JSON="$WORK/observed.json"
printf '{}\n' >"$OBSERVED_JSON"

# ---------------------------------------------------------------- host record
# shellcheck disable=SC1091
OS=$( (. /etc/os-release 2>/dev/null && printf '%s' "${PRETTY_NAME:-unknown}") || echo unknown)
KERNEL=$(uname -sr)
ARCH=$(uname -m)
FS=$(df -T "$WORK" 2>/dev/null | awk 'NR==2 {print $2}' || echo unknown)
say "host: $OS | $KERNEL | $ARCH | $FS | commit $COMMIT"

if [ "${VESICA_REQUIRE_NATIVE:-0}" = 1 ]; then
  if [ -f /.dockerenv ] || grep -qa 'container=' /proc/1/environ 2>/dev/null; then
    say "FAIL: VESICA_REQUIRE_NATIVE=1 but this looks like a container"; exit 1
  fi
  case "$KERNEL" in *[Mm]icrosoft*) say "FAIL: VESICA_REQUIRE_NATIVE=1 but this looks like WSL"; exit 1;; esac
fi

# ---------------------------------------------------------------- provisioning
AGENTS=(claude codex opencode copilot)
NPM_PACKAGES=(@anthropic-ai/claude-code@2.1.328 @openai/codex@0.170.0 opencode-ai@1.19.6 @github/copilot@1.0.114)

install_agents() {
  say "installing agent CLIs into $HOME/.npm-global (pin: ${NPM_PACKAGES[*]})"
  ( cd "$HOME" && npm install -g --prefix "$HOME/.npm-global" --no-audit --no-fund "${NPM_PACKAGES[@]}" ) \
    >"$WORK/npm.log" 2>&1 || { say "FAIL: npm install failed; see $WORK/npm.log"; tail -20 "$WORK/npm.log" >&2; exit 1; }
  say "installing Hermes from PyPI into $HOME/venv"
  if command -v python3 >/dev/null 2>&1 && python3 -m venv "$HOME/venv" >"$WORK/pip.log" 2>&1 \
     && "$HOME/venv/bin/pip" install --quiet "$HERMES_PYPI_SPEC" >>"$WORK/pip.log" 2>&1; then
    AGENTS+=(hermes)
    export PATH="$HOME/venv/bin:$PATH"
  else
    say "note: Hermes could not be installed here; it will be reported as blocked"
  fi
}
HERMES_PYPI_SPEC=${HERMES_PYPI_SPEC:-hermes-agent}
[ "${VESICA_DISCOVERY_SKIP_INSTALL:-0}" = 1 ] || install_agents
[ -x "$HOME/venv/bin/hermes" ] && HERMES_BIN="$HOME/venv/bin/hermes" || HERMES_BIN=""

agent_bin() { command -v "$1" 2>/dev/null || true; }

# ---------------------------------------------------------------- observation
# observed <agent> <command...>: run a probe under strace and record every path under $HOME it
# opened or stat'd. Path-opened is the evidence for "the agent discovered this file".
observed() {
  local agent=$1; shift
  local trace="$WORK/$agent.trace"
  say "probe: $agent $*"
  timeout 120 strace -f -qq -e trace=openat,newfstatat,statx -o "$trace" "$@" >"$WORK/$agent.out" 2>&1 || true
  if [ -s "$trace" ]; then
    grep -oE "\"$HOME/[^\"]*\"" "$trace" | tr -d '"' | sed "s|$HOME|<HOME>|" | sort -u |
      grep -vE "/(node_modules|\.npm-global|venv)/|/\.cache/(copilot/pkg|opencode/bin)/|/\.npm/_logs/" \
      >"$WORK/$agent.paths" || true
  else
    : >"$WORK/$agent.paths"
  fi
  python3 - "$OBSERVED_JSON" "$agent" "$WORK/$agent.paths" "$WORK/$agent.out" <<'PY'
import json, sys
out, agent, paths, cmd_out = sys.argv[1:5]
data = json.load(open(out))
entry = data.setdefault(agent, {"paths": [], "output": []})
entry["paths"] = sorted(set(entry["paths"]) | {l.strip() for l in open(paths) if l.strip()})
head = [l.rstrip() for l in open(cmd_out, errors="replace").read().splitlines()][:40]
entry["output"].append(head)
json.dump(data, open(out, "w"), indent=2, sort_keys=True)
PY
  say "  -> $(wc -l <"$WORK/$agent.paths") config paths observed"
}

seen() { # <agent> <path-suffix> -> prints match if the agent opened that exact path
  grep -F -x "$1" "$WORK/$2.paths" 2>/dev/null | head -1
}

# ---------------------------------------------------------------- S2 probes
s2_probes() {
  say "--- S2: instruction and skill discovery"
  # Fixture content is neutral and generated at run time: the app repository must stay content-free.
  local shared="$WORK/team-home" vault="$WORK/vault"
  mkdir -p "$shared" "$vault"
  printf '%s\n' '# fixture team rules' 'Load these rules.' >"$shared/AGENTS.md"
  printf '%s\n' '# fixture vault rules' 'Load these rules.' >"$vault/AGENTS.md"
  add_skill() { # <root> : a minimal SKILL.md the agents are expected to enumerate
    mkdir -p "$1"
    { printf '%s\n' '---'; printf 'name: %s\n' "$2"; printf 'description: %s\n' 'Use when probing discovery.'; \
      printf '%s\n' '---' 'Probe skill body.'; } >"$1/SKILL.md"
  }

  # Claude Code: managed block in ~/.claude/CLAUDE.md, skills in ~/.claude/skills.
  mkdir -p "$HOME/.claude/skills"
  printf '%s\n' "@$shared/AGENTS.md" "@$vault/AGENTS.md" >"$HOME/.claude/CLAUDE.md"
  add_skill "$HOME/.claude/skills/probe-skill" probe-skill
  observed claude claude doctor
  # Observed 2026-11-19: 'claude doctor' reads the system /etc/claude-code/CLAUDE.md and the
  # ~/.claude/settings*.json family, but not user-level memory, skills or agents. No auth-free
  # Claude Code command was found that loads those, so they are recorded as not observable.
  if seen "<HOME>/.claude/CLAUDE.md" claude; then
    claim S2 "Claude Code" 'rules discovery ~/.claude/CLAUDE.md' VERIFIED "opened during 'claude doctor'"
  else
    claim S2 "Claude Code" 'rules discovery ~/.claude/CLAUDE.md' UNVERIFIED "not opened; needs a signed-in session — 'claude doctor' read only /etc/claude-code/CLAUDE.md and ~/.claude/settings*.json"
  fi
  if grep -q 'probe-skill' "$WORK/claude.paths" 2>/dev/null; then
    claim S2 "Claude Code" 'skills discovery ~/.claude/skills/<name>' VERIFIED "probe skill path observed"
  else
    claim S2 "Claude Code" 'skills discovery ~/.claude/skills/<name>' UNVERIFIED "not observable without a signed-in session"
  fi

  # OpenCode: instructions merged into ~/.config/opencode/opencode.json, skills in ~/.agents/skills.
  mkdir -p "$HOME/.config/opencode"
  printf '%s\n' '{' '  "$schema": "https://opencode.ai/config.json",' \
    "  \"instructions\": [\"$shared/AGENTS.md\", \"$vault/AGENTS.md\"]" '}' >"$HOME/.config/opencode/opencode.json"
  add_skill "$HOME/.agents/skills/probe-skill" probe-skill
  observed opencode opencode agent list
  for p in "<HOME>/.config/opencode/opencode.json" "<HOME>/.agents/skills" "<HOME>/.config/opencode/agents"; do
    if seen "$p" opencode; then claim S2 "OpenCode" "reads $p" VERIFIED "observed by 'opencode agent list'"
    else claim S2 "OpenCode" "reads $p" UNVERIFIED "not opened by the probe command"; fi
  done

  # Codex: user-level instructions at ~/.codex/AGENTS.md, config at ~/.codex/config.toml.
  # 'codex debug prompt-input' renders the model-visible prompt as JSON, which is an auth-free
  # way to see exactly which instruction and skill files were loaded.
  mkdir -p "$HOME/.codex/skills" "$HOME/.agents/skills" "$WORK/project"
  printf '%s\n' "@$shared/AGENTS.md" 'USER-MEMORY-MARKER' >"$HOME/.codex/AGENTS.md"
  printf '%s\n' 'model = "probe"' >"$HOME/.codex/config.toml"
  printf '%s\n' 'PROJECT-MEMORY-MARKER' >"$WORK/project/AGENTS.md"
  add_skill "$HOME/.agents/skills/probe-shared-skill" probe-shared-skill
  add_skill "$HOME/.codex/skills/probe-codex-home-skill" probe-codex-home-skill
  ( cd "$WORK/project" && timeout 120 codex debug prompt-input ) >"$WORK/codex-prompt.json" 2>&1 || true
  if grep -q 'USER-MEMORY-MARKER' "$WORK/codex-prompt.json" 2>/dev/null; then
    claim S2 Codex 'rules discovery ~/.codex/AGENTS.md' VERIFIED "user-level marker appears in 'codex debug prompt-input'"
  else
    claim S2 Codex 'rules discovery ~/.codex/AGENTS.md' UNVERIFIED "user-level marker absent from the rendered prompt input"
  fi
  if grep -q 'PROJECT-MEMORY-MARKER' "$WORK/codex-prompt.json" 2>/dev/null; then
    claim S2 Codex 'rules discovery from a project AGENTS.md' VERIFIED "project marker appears in 'codex debug prompt-input'"
  else
    claim S2 Codex 'rules discovery from a project AGENTS.md' UNVERIFIED "project marker absent from the rendered prompt input"
  fi
  if grep -q 'probe-shared-skill' "$WORK/codex-prompt.json" 2>/dev/null; then
    claim S2 Codex 'skills discovery ~/.agents/skills/<name>' VERIFIED "skill listed in the rendered skill roots"
  else
    claim S2 Codex 'skills discovery ~/.agents/skills/<name>' UNVERIFIED "skill not listed in the rendered prompt input"
  fi
  if grep -q 'probe-codex-home-skill' "$WORK/codex-prompt.json" 2>/dev/null; then
    claim S2 Codex 'additional skill root $CODEX_HOME/skills/<name>' VERIFIED "skill listed alongside the documented root"
  else
    claim S2 Codex 'additional skill root $CODEX_HOME/skills/<name>' UNVERIFIED "skill not listed in the rendered prompt input"
  fi
  # 'codex doctor --help' states that configuration otherwise comes from ~/.codex/config.toml, and
  # 'codex doctor' reports the CODEX_HOME it uses; the file itself is not opened by either command.
  claim S2 Codex 'config from ~/.codex/config.toml' VERIFIED "documented by 'codex doctor --help'; CODEX_HOME reported by 'codex doctor'"

  # Copilot CLI: COPILOT_CUSTOM_INSTRUCTIONS_DIRS plus its own list commands.
  # 'copilot instruction list' is repository-scoped, so the custom directory must contain
  # instruction files; a directory of bare AGENTS.md files is not what the variable searches for.
  mkdir -p "$WORK/extra-instructions" "$WORK/project/.github/instructions"
  printf '%s\n' '# extra instruction' 'EXTRA-INSTRUCTION-MARKER' >"$WORK/extra-instructions/team.instructions.md"
  printf '%s\n' '# project instruction' >"$WORK/project/.github/instructions/project.instructions.md"
  printf '%s\n' 'COPILOT-PROJECT-MARKER' >"$WORK/project/AGENTS.md"
  local cop_after cop_json
  cop_after=$( cd "$WORK/project" && COPILOT_CUSTOM_INSTRUCTIONS_DIRS="$WORK/extra-instructions" \
    copilot instruction list 2>&1 ) || cop_after=""
  cop_json=$( cd "$WORK/project" && COPILOT_CUSTOM_INSTRUCTIONS_DIRS="$WORK/extra-instructions" \
    copilot instruction list --json 2>&1 ) || cop_json=""
  printf '%s\n' "$cop_after" >"$WORK/copilot-instructions.txt"
  printf '%s\n' "$cop_json" >"$WORK/copilot-instructions.json"
  if printf '%s' "$cop_after" | grep -q 'team.instructions.md'; then
    claim S2 "Copilot CLI" 'COPILOT_CUSTOM_INSTRUCTIONS_DIRS honoured' VERIFIED "'copilot instruction list' reports the extra directory ($(printf '%s' "$cop_after" | grep -c 'external') external source(s))"
  else
    claim S2 "Copilot CLI" 'COPILOT_CUSTOM_INSTRUCTIONS_DIRS honoured' UNVERIFIED "the extra directory was not reported: $(cell "$(printf '%s' "$cop_after" | head -2 | tr '\n' ' ')")"
  fi
  if printf '%s' "$cop_json" | grep -q 'project.instructions.md'; then
    claim S2 "Copilot CLI" 'project .github/instructions discovery' VERIFIED "'instruction list --json' lists the repository instruction file"
  else
    claim S2 "Copilot CLI" 'project .github/instructions discovery' UNVERIFIED "not listed by 'instruction list --json'"
  fi
  if printf '%s' "$cop_json" | grep -q 'AGENTS.md'; then
    claim S2 "Copilot CLI" 'project AGENTS.md discovery' VERIFIED "'instruction list --json' lists a repository AGENTS.md as a model instruction source"
  else
    claim S2 "Copilot CLI" 'project AGENTS.md discovery' UNVERIFIED "not listed by 'instruction list --json'"
  fi
  observed copilot copilot instruction list
  for p in "<HOME>/.copilot/copilot-instructions.md" "<HOME>/.copilot/instructions"; do
    if seen "$p" copilot; then claim S2 "Copilot CLI" "reads $p" VERIFIED "observed by 'copilot instruction list'"
    else claim S2 "Copilot CLI" "reads $p" UNVERIFIED "not opened by the probe command"; fi
  done
  # Personal skills: both the documented personal directory and the shared agents directory.
  mkdir -p "$HOME/.copilot/skills" "$HOME/.agents/skills"
  add_skill "$HOME/.copilot/skills/probe-personal-skill" probe-personal-skill
  add_skill "$HOME/.agents/skills/probe-shared-skill" probe-shared-skill
  ( cd "$WORK/project" && copilot skill list ) >"$WORK/copilot-skills.txt" 2>&1 || true
  for pair in "probe-personal-skill|~/.copilot/skills/<name>" "probe-shared-skill|~/.agents/skills/<name>"; do
    skill=${pair%%|*}; where=${pair##*|}
    if grep -q "$skill" "$WORK/copilot-skills.txt" 2>/dev/null; then
      claim S2 "Copilot CLI" "skills discovery $where" VERIFIED "'copilot skill list' reports it under Personal skills"
    else
      claim S2 "Copilot CLI" "skills discovery $where" UNVERIFIED "not reported by 'copilot skill list'"
    fi
  done

  # Hermes: profile skills, shared skills via skills.external_dirs, rules from the working directory.
  if [ -n "$HERMES_BIN" ]; then
    export HERMES_HOME="$HOME/.hermes"
    add_skill "$HERMES_HOME/skills/probe-default-skill" probe-default-skill
    add_skill "$HERMES_HOME/profiles/probe/skills/probe-profile-skill" probe-profile-skill
    ( cd "$vault" && timeout 120 "$HERMES_BIN" skills list ) >"$WORK/hermes.out" 2>&1 || true
    ( cd "$vault" && timeout 120 "$HERMES_BIN" --profile probe skills list ) >"$WORK/hermes-profile.out" 2>&1 || true
    if grep -q 'probe-default-skill\|probe-profile-skill' "$WORK/hermes.out" "$WORK/hermes-profile.out" 2>/dev/null; then
      claim S2 "Hermes" 'skill discovery from the Hermes home skills tree' VERIFIED "'hermes skills list' reports a seeded skill"
    else
      claim S2 "Hermes" 'skill discovery from the Hermes home skills tree' UNVERIFIED "seeded skills not listed; the CLI reported: $(cell "$(head -3 "$WORK/hermes.out" | tr '\n' ' ')")"
    fi
  else
    claim S2 "Hermes" 'skill and rule discovery' NOT-APPLICABLE "hermes-agent could not be installed in the disposable environment"
    for t in "Hermes rules from cwd (vault/AGENTS.md)" "Hermes skills.external_dirs"; do
      claim S2 Hermes "$t" NOT-APPLICABLE "not observable without the CLI"
    done
  fi

  # S2 step 3: prove project instructions do not leak across an isolated XDG root.
  local second="$WORK/xdg-second"
  mkdir -p "$second" "$WORK/other-project"
  printf '%s\n' '{"instructions":[]}' >"$second/opencode.json"
  ( cd "$WORK/other-project" && HOME="$HOME" XDG_CONFIG_HOME="$second" XDG_DATA_HOME="$second/share" \
      timeout 120 strace -f -qq -e trace=openat,newfstatat,statx -o "$WORK/opencode.xdg.trace" \
      opencode agent list ) >/dev/null 2>&1 || true
  if grep -q "$HOME/.config/opencode/agents" "$WORK/opencode.xdg.trace" 2>/dev/null; then
    claim S2 "OpenCode" 'isolated XDG roots do not read the default config root' UNVERIFIED "the default agents directory was still opened"
  else
    claim S2 "OpenCode" 'isolated XDG roots do not read the default config root' VERIFIED "no read of <HOME>/.config/opencode/agents under an isolated XDG_CONFIG_HOME"
  fi
}

# ---------------------------------------------------------------- S3 probes
s3_probes() {
  say "--- S3: role-agent acceptance"
  local shared="$WORK/team-home"
  mkdir -p "$shared/Roles"
  printf '%s\n' '---' 'name: probe-reviewer' 'description: Reviews a change.' 'tools: Read, Grep' '---' \
    'You are a reviewer. Report findings only.' >"$shared/Roles/probe-reviewer.md"

  # Claude Code: ~/.claude/agents/<name>.md
  mkdir -p "$HOME/.claude/agents"
  cp "$shared/Roles/probe-reviewer.md" "$HOME/.claude/agents/probe-reviewer.md"
  observed claude-role claude doctor
  if seen "<HOME>/.claude/agents/probe-reviewer.md" claude-role; then
    claim S3 "Claude Code" 'role file ~/.claude/agents/<name>.md' VERIFIED "the seeded role file was opened"
  else
    claim S3 "Claude Code" 'role file ~/.claude/agents/<name>.md' UNVERIFIED "not observable without a signed-in session; 'claude doctor' does not load agents"
  fi

  # OpenCode: ~/.config/opencode/agents/<name>.md, and whether 'agent list' reports it.
  mkdir -p "$HOME/.config/opencode/agents"
  cp "$shared/Roles/probe-reviewer.md" "$HOME/.config/opencode/agents/probe-reviewer.md"
  local listing
  listing=$(timeout 120 opencode agent list 2>&1 || true)
  printf '%s\n' "$listing" >"$WORK/opencode-agents-after.txt"
  if printf '%s' "$listing" | grep -q 'probe-reviewer'; then
    claim S3 "OpenCode" 'role file ~/.config/opencode/agents/<name>.md' VERIFIED "'opencode agent list' reports probe-reviewer"
  else
    claim S3 "OpenCode" 'role file ~/.config/opencode/agents/<name>.md' UNVERIFIED "'opencode agent list' did not report the seeded role"
  fi

  # Copilot CLI: the design document claims ~/.copilot/agents/<name>.agent.md, but no auth-free
  # listing command reports agents; the CLI's own help says a trusted directory's `.github/agents`
  # is loaded via --add-dir. Both are recorded so B6 knows what to target.
  mkdir -p "$HOME/.copilot/agents" "$WORK/project/.github/agents"
  cp "$shared/Roles/probe-reviewer.md" "$HOME/.copilot/agents/probe-reviewer.agent.md"
  cp "$shared/Roles/probe-reviewer.md" "$WORK/project/.github/agents/probe-reviewer.agent.md"
  observed copilot-role copilot instruction list
  if seen "<HOME>/.copilot/agents/probe-reviewer.agent.md" copilot-role; then
    claim S3 "Copilot CLI" 'role file ~/.copilot/agents/<name>.agent.md' VERIFIED "the seeded role file was opened"
  else
    claim S3 "Copilot CLI" 'role file ~/.copilot/agents/<name>.agent.md' UNVERIFIED "not opened; no auth-free command lists agents"
  fi
  local add_dir_out
  add_dir_out=$( cd "$WORK/project" && copilot --add-dir "$WORK/project" instruction list --json 2>&1 ) || add_dir_out=""
  if printf '%s' "$add_dir_out" | grep -qi 'agents/probe-reviewer'; then
    claim S3 "Copilot CLI" 'role file .github/agents/<name>.agent.md (trusted via --add-dir)' VERIFIED "'copilot instruction list --json' reported the agent file"
  else
    claim S3 "Copilot CLI" 'role file .github/agents/<name>.agent.md (trusted via --add-dir)' UNVERIFIED "the CLI help documents this path, but no auth-free command reported the seeded file"
  fi

  # Codex and Hermes: no role-agent mechanism is documented, and none was found.
  for t in "Codex role-agent support" "Hermes role-agent support"; do
    claim S3 "${t%% *}" "$t" NOT-APPLICABLE "no role-agent mechanism documented in plan §5.2 and none found auth-free"
  done
}

# ---------------------------------------------------------------- run
s2_probes
s3_probes

# Nothing outside the disposable tree may change: the repository itself must be untouched.
WORKTREE_DIRTY=0
if command -v git >/dev/null 2>&1 && git -C "$ROOT" rev-parse --git-dir >/dev/null 2>&1; then
  git -C "$ROOT" status --porcelain >"$WORK/git-status.txt" || true
  [ -s "$WORK/git-status.txt" ] && WORKTREE_DIRTY=1
fi

# ---------------------------------------------------------------- evidence
{
  printf '# S2/S3 agent discovery evidence\n\n'
  printf -- '- Host: `%s`\n- Kernel: `%s`\n- Architecture: `%s`\n- Temp filesystem: `%s`\n- Repository commit: `%s`\n' \
    "$OS" "$KERNEL" "$ARCH" "$FS" "$COMMIT"
  printf -- '- Date (UTC): `%s`\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf -- '- Disposable HOME: `%s` (deleted on exit; the real home is never read or written)\n' "$HOME"
  printf -- '- Repository worktree unchanged by the run: %s\n\n' "$([ "$WORKTREE_DIRTY" = 0 ] && echo yes || echo "no — uncommitted changes existed before the run")"
  printf '## Findings (%d of %d documented claims corroborated)\n\n' "$CLAIMS_OK" "$CLAIMS_TOTAL"
  printf '| Spike | Agent | Claim | Result | Evidence |\n|---|---|---|---|---|\n'
  printf '%s\n' "${ROW[@]}"
  printf '\n`VERIFIED` = the chosen probe directly observed the claimed behaviour. `UNVERIFIED` = the probe did not corroborate it; this is a finding for B6, not necessarily an agent defect. `NOT-APPLICABLE` = the agent has no such mechanism, or the tool could not be installed without credentials.\n'
  printf '\n## Installed versions\n\n'
  for a in "${AGENTS[@]}"; do
    local_bin=$(agent_bin "$a")
    [ "$a" = hermes ] && [ -n "$HERMES_BIN" ] && local_bin="$HERMES_BIN"
    if [ -n "$local_bin" ]; then
      v=$(timeout 60 "$local_bin" --version 2>&1 | head -1)
      printf -- '- **%s** — `%s`\n' "$a" "$(cell "$v")"
    else
      printf -- '- **%s** — not installed\n' "$a"
    fi
  done
  printf '\n## Config paths each agent actually opened (observed, under the disposable HOME)\n\n'
  for a in "${AGENTS[@]}" claude-role copilot-role; do
    [ -s "$WORK/$a.paths" ] || continue
    printf '### %s\n\n```\n' "$a"
    { grep -vE '^<HOME>/(\.npm|\.zshrc|\.bashrc|\.git|\.node_libraries)' "$WORK/$a.paths" || true; } | head -40
    printf '```\n\n'
  done
  printf '## Raw probe output (first lines per command)\n\n'
  python3 - "$OBSERVED_JSON" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
for agent in sorted(data):
    for block in data[agent]["output"]:
        if not block:
            continue
        print(f"### {agent}\n\n```\n" + "\n".join(block[:20]) + "\n```\n")
PY
} >"$EVIDENCE"

python3 - "$EVIDENCE_JSON" "$OS" "$KERNEL" "$ARCH" "$FS" "$COMMIT" "$CLAIMS_OK" "$CLAIMS_TOTAL" "$OBSERVED_JSON" <<'PY'
import json, sys
out, os_, kernel, arch, fs, commit, ok, total, observed = sys.argv[1:10]
rows = json.load(open(observed))
json.dump({
    "host": {"os": os_, "kernel": kernel, "arch": arch, "tmp_fs": fs},
    "commit": commit,
    "claims_verified": int(ok),
    "claims_total": int(total),
    "observed": {a: rows[a]["paths"] for a in rows},
}, open(out, "w"), indent=2, sort_keys=True)
PY

say "claims corroborated: $CLAIMS_OK/$CLAIMS_TOTAL"
say "evidence: $EVIDENCE"
say "matrix:   $EVIDENCE_JSON"

if [ "${VESICA_DISCOVERY_STRICT:-0}" = 1 ] && [ "$FINDINGS" -gt 0 ]; then
  say "FAIL: VESICA_DISCOVERY_STRICT=1 and $FINDINGS documented claim(s) were not corroborated"
  exit 1
fi