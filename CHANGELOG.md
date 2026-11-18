# Changelog

All notable changes to this project are documented here. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.1.0-alpha.3] - 2026-11-18

Third pre-release, still a prototype for disposable trials on native Linux x86_64. It brings the release in line with the documented command set: `share`, `new-skill` and `sync-standards` exist here (they did not in `alpha.2`, which ONBOARDING already described), alongside `install --vault` and `update`.

### Added
- `doctor` (text and `--json`) reports whether a **Git identity** is available, as an optional entry: the commit-writing commands (`vault snapshot`, `new-skill`, `share`) need one, while `install` does not. It is informational and never a blocker.

### Added
- `vesica share <note> --area <name> | --decisions` (plan §5.4, D26, S9): promote a personal note into the shared home as a reviewed PR.
  - **Nothing happens without `--yes`**: without it (or with `--dry-run`) the full plan prints and nothing is written, so cancelling before confirmation leaves no trace.
  - The promotion happens on a **`share/<note>` branch**; trunk is never written and nothing is pushed (`--open-pr` pushes only when asked).
  - **Links are handled, not assumed:** a `[[Team/…]]` link is the vault's view, so it's rewritten to the shared-home path; a link that cannot resolve in the shared home is refused, or converted to plain text with `--drop-unresolved-links`.
  - Inbound links in personal notes that point at the promoted note are rewritten to `[[Team/…]]`.
  - `owner` (your Git name, or `--owner`) and `authority: working` are added to the frontmatter when absent. `source` is only required for `mirror`/`digest` notes, so a promoted note doesn't need it.
  - `lint` and `vault-scan` gate the commit. A failed gate rolls the branch back **and removes the staged note**, leaving the checkout exactly as it was.
  - `--move` removes the personal note after the commit succeeds; by default it's kept.

### Added
- `vesica sync-standards --dry-run` (plan §5.4), with the per-repo opt-in being **`standards = true`** on a `[[repos]]` entry in `agent-home.toml` (decision D-I). It plans copying each `Standards/*.md` into an opted-in checkout as `.github/instructions/<name>.instructions.md`, reporting `Would create`, `Would update` or `unchanged` per file, and never writes anything. Repos that aren't opted in or aren't cloned are reported and skipped. Anything but `--dry-run` refuses, because opening the PRs needs separate approval.
  - `schemas/agent-home.schema.json` documents the flag, and the parser/schema parity corpus covers it (`standards` must be a boolean).

### Added
- `vesica new-skill <name>` (plan §5.4): scaffolds `Agents/Skills/<name>/SKILL.md` in the shared home **on a new `skill/<name>` branch**, committed locally and never pushed, so a person edits it and opens the PR. It refuses a dirty checkout, an existing skill, an existing branch, and unsafe names (1–64 lowercase characters, digits, `-` or `_`). `--description` sets the one-line description; without it the scaffold carries a visible TODO placeholder. The scaffold passes `vesica lint` and `vesica vault-scan` as-is. `--dry-run` prints the plan.
  - A shared home that isn't a Git repository is scaffolded without a branch or commit.
  - If the commit fails, the checkout is restored and the branch removed, so no half-written branch is left behind.

## [0.1.0-alpha.2] - 2026-11-18

Second pre-release, still a prototype for disposable trials on native Linux x86_64. Adds `vesica update`, which installs a newer release of the app itself after verifying its checksum and reported version, and the vault primitives and `install --vault` from the previous unreleased work.

### Added
- `vesica update` (plan §5.4): resolves a release (newest published, or `--tag`), downloads the tarball and `SHA256SUMS`, verifies the SHA-256, checks that the extracted binary reports the tag's version, replaces the binary atomically, then re-links from the recorded manifest. `--channel alpha` (default) includes pre-releases; `--channel stable` doesn't. `--dry-run` downloads and verifies but replaces nothing. Failures (checksum mismatch, missing checksum entry, wrong version) leave the installed binary untouched, and the staging directory is always removed.
  - The manifest now records the vault path, so a re-link after an update keeps `Team` where it is.
  - `VESICA_RELEASE_BASE_URL` and `VESICA_RELEASES_API_URL` override the download and release-list endpoints (used by tests, and usable against GitHub Enterprise mirrors).
- `schemas/doctor.schema.json` and `crates/agent-home/tests/doctor_schema.rs`: `doctor --json` is validated against the schema for both the ok and blocked reports, and 13 document mutations (unknown status or profile, `herdr` reported as green, a required command missing, and so on) must all be rejected.
- Install step 3 (decision D-H): `install --vault <dir>` (or `VESICA_VAULT`).
  - Creates the vault from the skeleton, or adopts an existing folder without overwriting anything.
  - Links the shared home at `<vault>/Team` instead of `~/Team`. No link is made when the shared-home checkout already lives there.
  - Exports `<PREFIX>_VAULT` and adds `@<vault>/AGENTS.md` to the Claude/Codex import block.
  - Vault scaffolding runs only after every install refusal, and `uninstall` keeps the vault.
- `install` removes symlinks recorded by an earlier install that it no longer plans (for example `~/Team` after switching to `--vault`), but only when they still point where Vesica put them. A link the user changed is kept and released from the manifest.
- `vesica vault init|fix|check|new-note|snapshot` (plan §5.1, §5.6, D30). The personal-vault skeleton and the ledger-note template are embedded from `assets/`.
  - `init` creates a vault in an absent or empty directory.
  - `fix` adopts an existing folder: it adds only what's missing, never overwrites, and makes sure `/Team` is ignored.
  - `check` is a read-only contract check: required entries, Git repository, Team excluded and untracked, and frontmatter with the allowed `type` values on every note outside `Team/`, `AGENTS.md` and `CLAUDE.md`. Findings report the rule, path and line, never note text.
  - `new-note` creates `Sessions/agent-runs/<date>_<task>_<agent>.md` and never overwrites.
  - `snapshot` commits the vault to its own repository with the person's Git identity and never includes `Team`. It refuses when `Team` isn't excluded or is already tracked, or when a file to be committed matches a secret pattern.
- `vault-scan` also detects AWS access key IDs.
- `lint` D28 and skill-lint rules (plan §5.5):
  - **VSH012:** `svg`/`png`/`jpg`/`jpeg`/`pdf` files must be LFS-tracked by the root `.gitattributes`. The matcher is checked against `git check-attr`.
  - **VSH013–VSH015:** `SKILL.md` under `Agents/` and `Standards/` needs valid frontmatter, a `name` matching its folder, and a description.
- Install step 2: `~/.config/agent-home/config.env` exports `<PREFIX>_HOME`, `<PREFIX>_PROFILE` and (developer/lead with `--repos-dir`) an absolute `<PREFIX>_REPOS`, single-quoted for POSIX shells. A managed block in `~/.bashrc` sources it. Both are managed blocks, so backups, modified-file refusal and byte-exact uninstall apply. Paths with line breaks, NUL or non-UTF-8 names are refused before any change. The S7 lifecycle checks the sourced values and the restore.
- JSON schemas `schemas/agent-home.schema.json` and `schemas/foundry.catalog.schema.json` (draft 2020-12) for shared-home authors, editors and CI. `crates/ah-core/tests/schema_parity.rs` runs a shared corpus of valid and invalid documents through both the CLI parser and the schema, and requires both to agree.

### Changed
- `team.env_prefix` must not start with a digit (it names shell variables); the parser and `agent-home.schema.json` both enforce it.

### Fixed
- `lint` size cap (VSH002) inspected `.git/`, so any real shared-home clone with a pack file over 1 MB failed. It also capped LFS-tracked files, which D28 exempts. Both are fixed.
- The fixture skill `second-skill` had invalid YAML frontmatter (an unquoted `: ` in its description), found by the new skill-lint.
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
