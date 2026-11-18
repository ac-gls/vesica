# Changelog

All notable changes to this project are documented here. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Changed
- Bootstrap supports native Linux x86_64; the WSL-only gate and `VESICA_ALLOW_NON_WSL` requirement are removed from `install.sh`. Non-Linux hosts are rejected.
- Installer tests verify platform gating with a fake `uname` and fail clearly on a `noexec` temp directory.

### Added
- `vesica doctor`, `install`, `status`, `uninstall`, `lint`, `vault-scan`, `team status`, and guarded fast-forward `team pull` prototype commands.
- Cargo workspace scaffold (`agent-home`, `ah-core`, `ah-config`, `ah-wsl`, `ah-agents`, `ah-vault`, `ah-herdr`).
- `install.sh` bootstrap: pinned release download, sha256 verification, `--from-file` for local testing.
