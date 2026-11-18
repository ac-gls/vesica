# Changelog

All notable changes to this project are documented here. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed
- Bootstrap supports native Linux x86_64; the WSL-only gate and `VESICA_ALLOW_NON_WSL` requirement are removed from `install.sh`. Non-Linux hosts are rejected.
- Installer tests verify platform gating with a fake `uname` and fail clearly on a `noexec` temp directory.

- `vesica doctor` runs a Linux preflight: platform/distro/kernel, required `git`/`gh`/`curl`/`sha256sum` (missing = blocker, non-zero exit), optional `az` and `node` against `minimum.node`. JSON adds `status`, `blockers`, `platform` and `prerequisites`.
- `vesica install --repos-dir <dir>` clones missing repositories for the `developer`/`lead` profiles (plan step 10). It's opt-in, never touches an existing destination, stages each clone in a temporary directory, has no Git prompts or `ext`/`fd` transports, and leaves repos in place on `uninstall`.
- `agent-home.toml` repository entries are validated: the name must be a single safe directory name, and option-like URLs and `ext::`/`fd::` URLs are rejected.
- **Breaking (config):** the `core` profile has no repositories (plan §3). A repo whose `profiles` includes `core` is rejected. An empty `profiles` list now means developer and lead; previously it also included core.

### Fixed
- TOML merge (`ah-config`) no longer drops comments when it replaces an existing value. The key's leading comment (including a file-header comment on the first key) and the value's inline comment are kept. It's covered by exact-byte and `insta` snapshot tests on realistic Codex/OpenCode configs.
- `uninstall` now restores a managed file byte-for-byte when the original had no final newline (previously it safely refused and left the block in place). Restoration is proven by re-applying the install transform to the verified backup and comparing against the current bytes.
- Repeat `install` no longer writes a new, unrecorded backup of a Vesica-created managed file (e.g. `~/.codex/AGENTS.md`) on every run.

### Added
- `tests/lifecycle-linux.sh` and the `lifecycle` CI job (Ubuntu 24.04/22.04 VM runners): S7 install/doctor/idempotence/refusal/uninstall lifecycle and S8 personal-vault/shared-home Git isolation evidence, published to the job summary.
- `vesica doctor`, `install`, `status`, `uninstall`, `lint`, `vault-scan`, `team status`, and guarded fast-forward `team pull` prototype commands.
- Cargo workspace scaffold (`agent-home`, `ah-core`, `ah-config`, `ah-wsl`, `ah-agents`, `ah-vault`, `ah-herdr`).
- `install.sh` bootstrap: pinned release download, sha256 verification, `--from-file` for local testing.
