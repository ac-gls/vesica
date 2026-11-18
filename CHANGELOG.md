# Changelog

All notable changes to this project are documented here. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- `vesica vault init|fix|check|new-note|snapshot` (plan §5.1, §5.6, D30). The personal-vault skeleton and the ledger-note template are embedded from `assets/`.
  - `init` creates a vault in an absent or empty directory.
  - `fix` adopts an existing folder: it adds only what's missing, never overwrites, and makes sure `/Team` is ignored.
  - `check` is a read-only contract check: required entries, Git repository, Team excluded and untracked, and frontmatter with the allowed `type` values on every note outside `Team/`, `AGENTS.md` and `CLAUDE.md`. Findings report the rule, path and line, never note text.
  - `new-note` creates `Sessions/agent-runs/<date>_<task>_<agent>.md` and never overwrites.
  - `snapshot` commits the vault to its own repository with the person's Git identity and never includes `Team`. It refuses when `Team` isn't excluded or is already tracked, or when a file to be committed matches a secret pattern.
- `vault-scan` also detects AWS access key IDs.
- Install step 2: `~/.config/agent-home/config.env` exports `<PREFIX>_HOME`, `<PREFIX>_PROFILE` and (developer/lead with `--repos-dir`) an absolute `<PREFIX>_REPOS`, single-quoted for POSIX shells. A managed block in `~/.bashrc` sources it. Both are managed blocks, so backups, modified-file refusal and byte-exact uninstall apply. Paths with line breaks, NUL or non-UTF-8 names are refused before any change. The S7 lifecycle checks the sourced values and the restore.
- JSON schemas `schemas/agent-home.schema.json` and `schemas/foundry.catalog.schema.json` (draft 2020-12) for shared-home authors, editors and CI. `crates/ah-core/tests/schema_parity.rs` runs a shared corpus of valid and invalid documents through both the CLI parser and the schema, and requires both to agree.

### Changed
- `team.env_prefix` must not start with a digit (it names shell variables); the parser and `agent-home.schema.json` both enforce it.

### Fixed
- `foundry.catalog.json` whose root is a JSON array was accepted (serde reads structs positionally from arrays, so `["<tenant>"]` set the tenant). The catalog root must now be an object.

## [0.1.0-alpha.1] - 2026-11-18

First pre-release. It's a prototype for disposable trials on native Linux x86_64, not for real home directories. It installs only part of the target architecture: no shared-home bootstrap, `vesica share`, Foundry, Herdr or Hermes integration. Verified on Ubuntu 24.04 and 22.04 by the S7/S8 lifecycle suite, including against the published release binary.

### Changed
- Bootstrap supports native Linux x86_64; the WSL-only gate and `VESICA_ALLOW_NON_WSL` requirement are removed from `install.sh`. Non-Linux hosts are rejected.
- Installer tests verify platform gating with a fake `uname` and fail clearly on a `noexec` temp directory.
- `vesica doctor` runs a Linux preflight: platform/distro/kernel, required `git`/`gh`/`curl`/`sha256sum` (missing = blocker, non-zero exit), optional `az` and `node` against `minimum.node`. JSON adds `status`, `blockers`, `platform` and `prerequisites`.
- `vesica install --repos-dir <dir>` clones missing repositories for the `developer`/`lead` profiles (plan step 10). It's opt-in, never touches an existing destination, stages each clone in a temporary directory, has no Git prompts or `ext`/`fd` transports, and leaves repos in place on `uninstall`.
- `agent-home.toml` repository entries are validated: the name must be a single safe directory name, and option-like URLs and `ext::`/`fd::` URLs are rejected.
- **Breaking (config):** the `core` profile has no repositories (plan §3). A repo whose `profiles` includes `core` is rejected. An empty `profiles` list now means developer and lead; previously it also included core.

- Content-free guard `scripts/content-guard.sh` and the `content-guard` CI job (plan §5.5 / B8), with tests that seed each violation.
- `supply-chain` CI job: `cargo deny --locked check` and `cargo audit --deny warnings`. The licence allow-list is trimmed to the licences actually in use.

- Release pipeline: `scripts/build-release.sh` (static musl artifact, reproducible tarball, `SHA256SUMS`, version consistency checks), the `release-dry-run` CI job, and `.github/workflows/release.yml` (tag → GitHub Release → verify by bootstrapping the published release and running S7/S8 on Ubuntu 24.04/22.04).
- `docs/TROUBLESHOOTING.md`.

### Removed
- `chrono` (RUSTSEC-2026-0543, unmaintained). Backup timestamps now use a small tested UTC formatter.
- `serde_yaml` (deprecated) and its `unsafe-libyaml` dependency (RUSTSEC-2024-0449, unmaintained). Frontmatter lint uses `yaml-rust2`, with characterization tests pinning the previous semantics. The dependency graph went from 121 to 94 crates.

### Fixed
- TOML merge (`ah-config`) no longer drops comments when it replaces an existing value. The key's leading comment (including a file-header comment on the first key) and the value's inline comment are kept. It's covered by exact-byte and `insta` snapshot tests on realistic Codex/OpenCode configs.
- `uninstall` now restores a managed file byte-for-byte when the original had no final newline (previously it safely refused and left the block in place). Restoration is proven by re-applying the install transform to the verified backup and comparing against the current bytes.
- Repeat `install` no longer writes a new, unrecorded backup of a Vesica-created managed file (e.g. `~/.codex/AGENTS.md`) on every run.

### Added
- `tests/lifecycle-linux.sh` and the `lifecycle` CI job (Ubuntu 24.04/22.04 VM runners): S7 install/doctor/idempotence/refusal/uninstall lifecycle and S8 personal-vault/shared-home Git isolation evidence, published to the job summary.
- `vesica doctor`, `install`, `status`, `uninstall`, `lint`, `vault-scan`, `team status`, and guarded fast-forward `team pull` prototype commands.
- Cargo workspace scaffold (`agent-home`, `ah-core`, `ah-config`, `ah-wsl`, `ah-agents`, `ah-vault`, `ah-herdr`).
- `install.sh` bootstrap: pinned release download, sha256 verification, `--from-file` for local testing.
