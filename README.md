# Vesica

Vesica is a content-free installer and CLI that gives every developer the same AI-agent setup:
team rules, shared skills and repository pointers come from a separate, private **shared home**
repository, and each developer's personal vault stays private. This repository holds only the
tool. The CI content guard keeps team content out of it.

> **Pre-release prototype (`v0.1.0-alpha.3`).** Validated on native Linux x86_64 (Ubuntu 24.04
> and 22.04). Only some of the target architecture is built: no shared-home bootstrap, no `vesica share`,
> no Foundry, Herdr or Hermes integration, and only Claude Code and Codex instruction wiring.
> **Do not use it on your real home directory yet.** Try it with a disposable `--root`.

## Quick start (disposable trial)

```bash
# Install the binary only (downloads the release and verifies SHA256SUMS; no sudo).
curl -fsSL https://raw.githubusercontent.com/ac-gls/vesica/v0.1.0-alpha.3/install.sh | sh -s -- --bootstrap-only

# Point at a local checkout of your team's shared home and a throwaway root.
SHARED=/path/to/shared-home-checkout
TRIAL=$(mktemp -d)
vesica doctor --home-repo "$SHARED"
vesica install --home-repo "$SHARED" --root "$TRIAL" --dry-run
vesica install --home-repo "$SHARED" --root "$TRIAL" --vault "$TRIAL/vault"   # add --repos-dir DIR to clone repos
vesica status --root "$TRIAL"
vesica uninstall --root "$TRIAL"
```

## What works today

| Command | Does |
|---|---|
| `doctor` | Read-only Linux preflight: platform, required `git`/`gh`/`curl`/`sha256sum`, optional `az`/`node`, shared-home config. Blockers exit non-zero. |
| `install` | Links the shared home and its skills, adds managed import blocks to Claude Code and Codex instruction files, merges OpenCode's `instructions` list, writes `~/.config/agent-home/config.env` (sourced from `~/.bashrc`), and (with `--repos-dir`) clones missing developer/lead repositories. Never overwrites; refuses on any unmanaged destination. |
| `update` | Fetches a release, verifies its SHA-256 and version, replaces the binary, then re-links your setup from the recorded manifest. `--dry-run` verifies without replacing. |
| `status` / `uninstall` | Reports, or removes and restores, only what Vesica can prove it created. Originals are restored byte-for-byte. |
| `share` | Promotes a personal note into the shared home on a `share/<note>` branch: rewrites links, adds `owner`/`authority`, gates on `lint` and `vault-scan`, and asks for `--yes` before writing anything. Trunk is never written and nothing is pushed. |
| `sync-standards` | `--dry-run` only: plans copying `Standards/*` into the checkouts that opted in with `standards = true` in `agent-home.toml`. Opening the PRs needs separate approval. |
| `new-skill` | Scaffolds a new shared skill on a `skill/<name>` branch in the shared home, for you to edit and open as a PR. It never writes to your current branch and never pushes. |
| `team status` / `team pull` | Local status of a shared-home checkout (no fetch); fast-forward-only pull that refuses dirty or divergent checkouts. The path comes from `--path` or `VESICA_TEAM_HOME`. |
| `vault init` / `fix` / `check` / `new-note` / `snapshot` | Create or adopt a personal vault from the skeleton, check it against the vault contract, write ledger notes, and commit it to its own Git repository, never including `Team/`. |
| `lint` / `vault-scan` | Read-only shared-home policy lint and a heuristic credential scan. Findings never echo content. |

## Docs

- [ONBOARDING](docs/ONBOARDING.md): current behavior, compatibility names, local build and test.
- [TROUBLESHOOTING](docs/TROUBLESHOOTING.md): every refusal message, with its cause and fix.
- [ARCHITECTURE](docs/ARCHITECTURE.md): crates, safety model, CI, release and known limitations.
- [CHANGELOG](CHANGELOG.md)

## License

MIT
