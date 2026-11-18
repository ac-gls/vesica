# Vesica Architecture

> **Status: prototype / incomplete.** Vesica is the renamed product identity of the historical Agent Home Kit project; paths and repo names retain compatibility identifiers. This document describes the code that exists in this checkout, not the aspirational multi-agent installer described in older planning notes. Do not use it to install a production team setup yet.
## Current executable flow
```text
vesica CLI
  ├─ doctor   parse team config/catalog; Linux preflight (platform, required/optional commands,
  │           node minimum); blockers exit non-zero; WSL status informational; optional --json
  ├─ install  build a plan; preflight destinations/manifest; back up existing
  │           Claude/Codex instruction files; execute supported actions; save manifest
  ├─ status   read the install manifest and report recorded entries
  ├─ uninstall verify hashes/links, then remove or restore only managed entries
  ├─ team status/pull  inspect local shared-home refs / guarded fast-forward pull
  ├─ lint     read-only shared-home checks with secret-safe diagnostics
  ├─ vault-scan recursively check bounded-size UTF-8 files for common secret patterns

shared-home checkout
  ├─ AGENTS.md                         → ~/Team symlink
  ├─ AGENTS.md                         → managed import in Claude and Codex files
  └─ Agents/Skills/*/SKILL.md          → skill symlinks for .agents and Claude
```

## Crates

- `ah-config`: JSONC/TOML recursive merges, managed text blocks, safe symlink primitive.
- `ah-core`: team/catalog parsing and validation, action planner/executor, manifest model.
- `ah-agents`: deterministic discovery of skill directories containing `SKILL.md`.
- `ah-vault`: bounded common-credential pattern scanner and read-only shared-home lint (area README coverage, size cap, local wikilinks, personal paths, heuristic email detection, policy-area frontmatter/ownership/source checks, secret-safe findings).
- `ah-wsl`: WSL kernel detection for informational doctor output; native Linux is supported, and Windows interop is not implemented.
- `ah-herdr`: placeholder only; runtime client is not implemented.
- `vesica`: currently wires the limited workflows listed above.

## Safety and transaction

The CLI preflights unsafe state paths. Installer mutations, backup creation, and manifest persistence now share a best-effort rollback transaction. Manifest writes use exclusive temporary-file creation, syncing before atomic replacement, and mode `0600` for new Unix manifests. Backups are created exclusively and only removed on transaction failure if their bytes remain unchanged; cleanup/rollback errors are included in the reported failure. Tests cover manifest temp-file and symlink collisions, user-content preservation, and CLI install rollback.

## Validation

- `cargo fmt --all -- --check`
- `cargo test --workspace --quiet`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build -p vesica`
- `VESICA_TEST_TMP_ROOT=/mnt/wsl-home bash tests/install.sh`
- `shellcheck install.sh tests/install.sh`
- `git diff --check`

These are Linux/WSL development-container results. Installer fixtures simulate Linux and non-Linux host identity and use a local tarball; they do not establish a real native-Linux distribution matrix, real profile install, or signed-release validation. Windows interoperability is optional and unvalidated. The transaction cannot guarantee success if the OS refuses rollback or a user replaces files concurrently; it avoids clobbering changed content and reports remaining errors. Vesica remains a prototype, not pilot/production-ready.

1. The shared-home repository holds team content; the installer repository does not.
2. Config/catalog parsing rejects credential-like keys.
3. The installer preflights all planned destinations and validates the state directory/manifest with symlink-aware metadata before creating backups or executing actions. An unmanaged collision or modified manifest-managed target fails closed.
4. Existing Claude/Codex files are backed up before managed-block changes. Repeat installs reuse recorded backups; backups use exclusive creation and are removed if a failed plan/manifest write rolls back, unless their content changed in the meantime.
5. Action execution and manifest persistence share a rollback transaction: if manifest commit fails after actions succeed, the executor attempts reverse-order rollback and reports rollback errors without overwriting intervening edits.
6. Manifest writes use a unique-create temp file, restrictive Unix permissions, flush/sync, then rename; a preexisting temp file or symlink is not followed or overwritten.
7. The JSON manifest records install targets, source paths, backups, and hashes. Uninstall restores/removes only entries it can verify; ambiguous or modified content is retained and reported.
8. Symlinks are created only at absent destinations or accepted if already correct; existing user destinations are never replaced.
9. Secret scan output contains locations only. Scanner limits individual files to 2 MB and does not traverse symlinks.

## Known limitations

- `bash tests/lifecycle-linux.sh` (CI job `lifecycle`) exercises the real binary through bootstrap → doctor → dry-run → install → repeat install → refusal cases → uninstall, plus personal-vault/shared-home Git isolation and `team status/pull`, in a disposable HOME. In CI it runs on GitHub-hosted Ubuntu VMs with `VESICA_REQUIRE_NATIVE=1`; local WSL/container runs are labelled supplemental.
- `bash tests/install.sh` builds a disposable fixture release and checks Linux host/architecture gating, bootstrap-only installation, preservation of spaced CLI arguments, and checksum rejection without side effects. It is network-free; CI runs it alongside ShellCheck. It does not establish the plan's complete install → doctor → idempotent reinstall parity acceptance.
- Private-team cloning/bootstrap remains incomplete; rollback covers bounded installer action/manifest failure cases only.
- Profile selection is recorded in the manifest; both `doctor` and `install` report the repositories assigned to the selected profile. With `--repos-dir` (or `VESICA_REPOS_DIR`), `install` clones missing repositories for the `developer` and `lead` profiles (plan §5.4 step 10). It plans read-only before any mutation, never modifies an existing destination (a matching checkout is `Present`; anything else is `Skipped` and left untouched), clones through a temporary sibling directory so a failure leaves no partial checkout, disables Git prompts and the `ext`/`fd` transports, and reports clone failures with a non-zero exit after the managed configuration is installed. Clones are not manifest entries; `uninstall` keeps repositories. The `core` profile has no repositories: config rejects `core` in a repo's `profiles`, and an empty list means developer and lead. Every profile links the full shared skill set (plan §3).
- Claude/Codex and discovered skill links are the only harness wiring. OpenCode, Copilot, Hermes, role-agent rendering, Foundry, and Herdr are not wired.
- WSL detection exists, but Windows profile discovery, drvfs/OneDrive validation, `winget`, Obsidian, scheduled tasks, and Windows-side operations do not.
- Vault scan is a heuristic check for common patterns, not a complete DLP/secret detector; absence of findings is not proof that a vault is secret-free.
- No signed release/update path or native-Linux lifecycle/dogfood validation is implemented (Windows+WSL is optional); rollback remains bounded to the tested action/manifest failure cases above.

## Verification

From the repository root:

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p vesica
```

CI additionally runs ShellCheck over `install.sh`. Current integration tests use temporary homes and the fixture at `tests/fixtures/shared-home`; they do not establish readiness on a real user's WSL/Windows profile.
