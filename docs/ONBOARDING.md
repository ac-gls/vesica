# Vesica Onboarding

> **Prototype notice:** Vesica is the product name; Agent Home Kit is the historical plan name. This checkout has working local install/status/uninstall slices, not the complete team setup. Do not follow the aspirational bootstrap example in the top-level README as a production-ready procedure yet. Foundry access and private shared-home repository access are external prerequisites and remain unresolved.

## Vesica rename and compatibility

**The installed program and CLI are named `vesica`;** the historical project/plan/repository may be called Agent Home Kit or Agent Home. The public repository/download URL remains `ac-gls/vesica`. The shared-home config stays `agent-home.toml`. To avoid orphaning existing install records, the manifest continues at `~/.config/agent-home/manifest.json`, and existing `AGENT_HOME_REPO`, `AGENT_HOME_ROOT`, and `AGENT_HOME_ALLOW_NON_WSL` variables remain accepted. The bootstrap accepts `VESICA_*` variables first, with legacy variable fallback. Uninstall handles both historical `agent-home` blocks and new `vesica` blocks. This identity change does not migrate an existing installation directory.

## What currently works

- `doctor --profile core|developer|lead` selects a profile; its JSON report lists repos assigned to that profile (empty `profiles` means shared across profiles). Install wiring does not yet apply profile-specific content.
- `install --dry-run` prints the supported install plan without mutating the chosen home.
- `install` links the local shared home at `~/Team` (or an explicit test root), adds managed imports to Claude Code and Codex instruction files, and links discovered skill directories to `.agents/skills` and `.claude/skills`.
- `status` inspects the installer manifest.
- `uninstall` removes or restores only entries that pass verification; modified or unverifiable entries are retained.
- `lint --home-repo <path>` checks a local shared-home checkout (area README coverage, size cap, unresolved internal wiki links, personal paths and common secrets). Findings contain relative paths, line, stable rule ID and fixed message only—not matched text. This is heuristic and not a complete DLP or policy validator.
- `vault-scan --path <directory>` flags common credential patterns by path and line without printing matched values.

## Build and test locally

Prerequisites: Rust toolchain from `rust-toolchain.toml`, Git, and a local shared-home directory containing `AGENTS.md` and a valid `agent-home.toml`. The fixture is suitable for tests only.

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p vesica
cargo run -p vesica -- --help
```

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

- `lint --home-repo <path>` validates Knowledge/Decisions/Templates README coverage, 1 MB file limit, internal wiki links and personal paths, and invokes the common-pattern credential scanner. Diagnostics contain relative paths, line numbers and stable rule IDs, never matched text. This is heuristic, not full vault-policy or DLP coverage.
- The CLI does not yet perform a private team-home clone, profile-specific repository cloning/installation, per-profile skill selection (all discovered shared skills remain linked), complete OpenCode/Copilot/Hermes wiring, role-agent generation, Foundry setup/auth probes, Herdr orchestration, Windows interoperability, update, signed release verification, or transactional rollback of a partially applied plan. `doctor` explicitly says live harness, Foundry, Windows and Herdr checks are not implemented.

Before a real pilot: resolve the private repo/org permissions and Foundry prerequisite, finish implementation and tests for the missing integrations, run dogfood on Windows+WSL with a disposable profile, verify CI/release artifacts, and obtain explicit release approval.
