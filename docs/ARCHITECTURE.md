# Agent Home Kit Architecture

> **Status: prototype / incomplete.** This document describes the code that exists in this checkout, not the aspirational multi-agent installer described in older planning notes. Do not use it to install a production team setup yet.
## Current executable flow
```text
vesica CLI
  ├─ doctor   parse team config + optional Foundry catalog; report selected profile
  ├─ install  build a plan; preflight destinations/manifest; back up existing
  │           Claude/Codex instruction files; execute supported actions; save manifest
  ├─ status   read the install manifest and report recorded entries
  ├─ uninstall verify hashes/links, then remove or restore only managed entries
  └─ vault-scan recursively check bounded-size UTF-8 files for common secret patterns

shared-home checkout
  ├─ AGENTS.md                         → ~/Team symlink
  ├─ AGENTS.md                         → managed import in Claude and Codex files
  └─ Agents/Skills/*/SKILL.md          → skill symlinks for .agents and Claude
```

## Crates

- `ah-config`: JSONC/TOML recursive merges, managed text blocks, safe symlink primitive.
- `ah-core`: team/catalog parsing and validation, action planner/executor, manifest model.
- `ah-agents`: deterministic discovery of skill directories containing `SKILL.md`.
- `ah-vault`: bounded common-credential pattern scanner. It reports file and line, never matched text.
- `ah-wsl`: WSL kernel detection and a CI override; no Windows interop installer steps yet.
- `ah-herdr`: placeholder only; runtime client is not implemented.
- `vesica`: currently wires the limited workflows listed above.

## Safety model

1. The shared-home repository holds team content; the installer repository does not.
2. Config/catalog parsing rejects credential-like keys.
3. The installer preflights all planned destinations before creating backups or executing actions. An unmanaged collision or modified manifest-managed target fails closed.
4. Existing Claude/Codex files are backed up before managed-block changes. Repeat installs reuse recorded backups.
5. The JSON manifest records install targets, source paths, backups, and hashes. Uninstall restores/removes only entries it can verify; ambiguous or modified content is retained and reported.
6. Symlinks are created only at absent destinations or accepted if already correct; existing user destinations are never replaced.
7. Secret scan output contains locations only. Scanner limits individual files to 2 MB and does not traverse symlinks.

## Known limitations

- Installation currently targets a local shared-home checkout. Private-team cloning/bootstrap and rollback across a partially failed multi-action install are not implemented.
- Profile selection is recorded/reported but does not yet filter or render profile-specific content.
- Claude/Codex and discovered skill links are the only harness wiring. OpenCode, Copilot, Hermes, role-agent rendering, Foundry, and Herdr are not wired.
- WSL detection exists, but Windows profile discovery, drvfs/OneDrive validation, `winget`, Obsidian, scheduled tasks, and Windows-side operations do not.
- Vault scan is a heuristic check for common patterns, not a complete DLP/secret detector; absence of findings is not proof that a vault is secret-free.
- No signed release/update path, full rollback transaction, or end-to-end Windows+WSL dogfood validation is implemented.

## Verification

From the repository root:

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p vesica
```

CI additionally runs ShellCheck over `install.sh`. Current integration tests use temporary homes and the fixture at `tests/fixtures/shared-home`; they do not establish readiness on a real user's WSL/Windows profile.
