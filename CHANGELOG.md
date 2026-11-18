# Changelog

All notable changes to this project are documented here. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed
- Bootstrap supports native Linux x86_64; the WSL-only gate and `VESICA_ALLOW_NON_WSL` requirement are removed from `install.sh`. Non-Linux hosts are rejected.
- Installer tests verify platform gating with a fake `uname` and fail clearly on a `noexec` temp directory.

- `vesica doctor` runs a Linux preflight: platform/distro/kernel, required `git`/`gh`/`curl`/`sha256sum` (missing = blocker, non-zero exit), optional `az` and `node` against `minimum.node`. JSON adds `status`, `blockers`, `platform` and `prerequisites`.

### Fixed
- Repeat `install` no longer writes a new, unrecorded backup of a Vesica-created managed file (e.g. `~/.codex/AGENTS.md`) on every run.

### Added
- `tests/lifecycle-linux.sh` and the `lifecycle` CI job (Ubuntu 24.04/22.04 VM runners): S7 install/doctor/idempotence/refusal/uninstall lifecycle and S8 personal-vault/shared-home Git isolation evidence, published to the job summary.
- `vesica doctor`, `install`, `status`, `uninstall`, `lint`, `vault-scan`, `team status`, and guarded fast-forward `team pull` prototype commands.
- Cargo workspace scaffold (`agent-home`, `ah-core`, `ah-config`, `ah-wsl`, `ah-agents`, `ah-vault`, `ah-herdr`).
- `install.sh` bootstrap: pinned release download, sha256 verification, `--from-file` for local testing.
