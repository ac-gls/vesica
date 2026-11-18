# Vesica Onboarding

> **Prototype notice:** Vesica is the product name; Agent Home Kit is the historical plan name. This checkout has working local install/status/uninstall slices, not the complete team setup. Do not follow the aspirational bootstrap example in the top-level README as a production-ready procedure yet. Foundry access and private shared-home repository access are external prerequisites and remain unresolved.

## Vesica rename and compatibility

**The installed program and CLI are named `vesica`;** the historical project/plan/repository may be called Agent Home Kit or Agent Home. The public repository/download URL remains `ac-gls/vesica`. The shared-home config stays `agent-home.toml`. To avoid orphaning existing install records, the manifest continues at `~/.config/agent-home/manifest.json`, and existing `AGENT_HOME_REPO`, `AGENT_HOME_ROOT`, and `AGENT_HOME_ALLOW_NON_WSL` variables remain accepted. The bootstrap accepts `VESICA_*` variables first, with legacy variable fallback. Uninstall handles both historical `agent-home` blocks and new `vesica` blocks. This identity change does not migrate an existing installation directory.

## What currently works

- `doctor` runs a read-only Linux preflight: platform (OS, architecture, distro, kernel), required commands `git`, `gh`, `curl` and `sha256sum` (a missing one is a blocker and makes `doctor` exit non-zero), and optional `az` and `node` (checked against the team's `minimum.node`). WSL detection is informational only.
- `doctor --profile core|developer|lead` selects a profile, and its JSON report lists the repos for that profile. Profiles nest: `core` is the vault, rules, skills, roles and model config, with **no repositories**. `developer` adds repositories (and Herdr, not yet implemented). `lead` adds Hermes (not yet implemented). In `agent-home.toml`, a repo's `profiles` may list `developer` and/or `lead`; an empty list means both, and listing `core` is rejected.
- `install --dry-run` prints the supported install plan without mutating the chosen home, including `Would clone:` lines when `--repos-dir` is given.
- `install --repos-dir <dir>` (developer/lead profiles) clones missing profile repositories. Existing destinations are never modified, and `uninstall` keeps the clones.
- `install` links the local shared home at `~/Team` (or an explicit test root), adds managed imports to Claude Code and Codex instruction files, and links discovered skill directories to `.agents/skills` and `.claude/skills`.
- `status` inspects the installer manifest.
- `uninstall` removes or restores only entries that pass verification; modified or unverifiable entries are retained.
- `lint --home-repo <path>` checks area README coverage, 1 MB file limit, internal wiki links, personal paths, heuristic email patterns (`VSH007`), credential patterns (`VSH006`), and policy-area frontmatter: valid YAML (`VSH008`), required fields including owner/tags (`VSH009`), allowed authority (`VSH010`), and required source for mirror/digest notes (`VSH011`). These rules apply within `Knowledge/`, `Decisions/`, and `Templates/`; area-root README files without frontmatter are tolerated for compatibility, and template-variable fields must conform to valid YAML (not skipped wholesale). Findings are secret-safe; heuristics do not constitute complete DLP/policy enforcement.
- `vault-scan --path <directory>` flags common credential patterns by path and line without printing matched values.

If something refuses or fails, see [TROUBLESHOOTING.md](TROUBLESHOOTING.md).

## Build and test locally

Prerequisites: Rust toolchain from `rust-toolchain.toml`, Git, and a local shared-home directory containing `AGENTS.md` and a valid `agent-home.toml`. The fixture is suitable for tests only.

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p vesica
cargo run -p vesica -- --help
```

The shell bootstrap also has network-free integration coverage: `bash tests/install.sh` creates a temporary fixture release and tests Linux host gating, checksum validation and argument forwarding. It needs an exec-capable temp directory; if `/tmp` is mounted `noexec`, set `VESICA_TEST_TMP_ROOT` to another directory. Its checksum-failure case generates a dummy digest at runtime; no real secrets are used.

## Safe local trial

Use a disposable directory, not the real home, first:

```sh
cargo run -p vesica -- doctor --home-repo tests/fixtures/shared-home
cargo run -p vesica -- doctor --json --home-repo tests/fixtures/shared-home
cargo run -p vesica -- install --home-repo tests/fixtures/shared-home --root /tmp/vesica-test --dry-run
```

Only after inspecting the plan, use the same temporary root without `--dry-run`, then inspect it using `status --root /tmp/vesica-test`. Exercise conservative removal with `uninstall --root /tmp/vesica-test`. Never point this prototype at a real profile until it has been dogfooded and its backup/rollback behavior reviewed.

For scanner development, run against a disposable fixture—not a synced vault with real credentials:

```sh
cargo run -p vesica -- vault-scan --path tests/fixtures/shared-home
```

A match prints only file and line, returns a non-zero exit, and intentionally does not echo the detected value. The scanner is heuristic; a clean scan is not proof that no secrets exist.

## Current scope gaps

- `lint --home-repo <path>` validates Knowledge/Decisions/Templates README coverage, 1 MB file limit, internal wiki links, personal paths and email-address patterns, and invokes the common-pattern credential scanner. Diagnostics contain relative paths, line numbers and stable rule IDs, never matched text. This is heuristic, not full vault-policy or DLP coverage.
- The CLI does not yet perform a private team-home clone, complete OpenCode/Copilot/Hermes wiring, role-agent generation, Foundry setup/auth probes, Herdr orchestration, Windows interoperability, update, or signed release verification. Install actions and manifest commit now share a rollback transaction, but rollback is best-effort: intervening user edits are preserved and reported, and a rollback error may still leave partial state. Manifest commit uses a uniquely created temporary file; Unix temp permissions are restricted. Native Linux x86_64 is the supported platform; Windows/WSL interoperability is optional and unvalidated. `doctor` explicitly says live harness, Foundry, Windows and Herdr checks are not implemented.

Before a real pilot: resolve the private repo/org permissions and Foundry prerequisite, finish implementation and tests for the missing integrations, record native-Linux lifecycle evidence (plan S7/S8) and dogfood with a disposable Linux home/vault, verify CI/release artifacts, and obtain explicit release approval.
