# Vesica Troubleshooting

Vesica is a prototype. It refuses rather than overwrites, so most problems show up as a clear
refusal message and leave your files untouched. Find the message below, fix the cause, and
rerun the same command; every command here is safe to repeat.

## Bootstrap (`install.sh`)

| Message | Cause | Fix |
|---|---|---|
| `only Linux hosts are supported` / `only x86_64 is supported` | Vesica ships one static Linux x86_64 binary. | Use a Linux x86_64 host (a VM is fine). WSL works but is not the validated platform. |
| `missing required command: tar` (or `sha256sum`, `curl`) | Bootstrap prerequisites are missing. | Install them with your distro's package manager. |
| `download failed: …` | The tag, network or GitHub is unavailable, or the release does not exist. | Check that the release exists at `https://github.com/ac-gls/vesica/releases`. To install a specific version, set `VESICA_VERSION=vX.Y.Z[-pre]`. |
| `checksum mismatch for …: refusing to install` | The downloaded tarball does not match `SHA256SUMS`. Nothing was installed. | Retry the download. If it persists, don't work around it; report it, because the artifact may have been tampered with. |
| `SHA256SUMS not found next to …` | `--from-file` needs the release's `SHA256SUMS` in the same directory. | Download both files from the release page. |

## `vesica doctor`

`doctor` is read-only. `Blocker:` lines make it exit non-zero; `Optional` lines never do.

| Output | Meaning / fix |
|---|---|
| `Blocker: required command not found: git` (also `gh`, `curl`, `sha256sum`) | Install the command and make sure it's on `PATH` and executable. |
| `Blocker: unsupported operating system` / `architecture` | See the bootstrap row above. |
| `Optional az: missing` | Only needed for future Foundry model features. |
| `Optional node: too old: 18.19.0 < 22.0.0` | Agent CLIs need the shared home's `minimum.node`. Upgrade Node if you use those CLIs. |
| `WSL kernel: detected (informational …)` | Informational only. Native Linux is the supported platform. |
| `Live harness, Foundry, Windows and Herdr checks are not implemented.` | Expected: those integrations are not built yet. |
| `shared home must contain AGENTS.md` / config parse errors | `--home-repo` must point at a checkout of the team's shared home with a valid `agent-home.toml`. |
| `repository … lists the core profile; the core profile has no repositories` | In `agent-home.toml`, list repos under `developer` and/or `lead`, or leave `profiles` empty for both. |
| `repository name … must be a single directory name …` / `option-like url` / `forbidden git transport` | Fix that entry in `agent-home.toml`. Names are one safe directory name, and URLs are ordinary `https://`, `git@` or local paths. |

## `vesica install`

Always start with `--dry-run`. It prints the plan and changes nothing.

| Message | Cause | Fix |
|---|---|---|
| `unmanaged destination already exists: …` / `install stopped on an existing destination` | Something that Vesica didn't create already sits where it would put a link (for example `~/Team` or a skill link). Nothing was changed. | Move or rename that file yourself, then rerun. Vesica never overwrites it. |
| State-path errors about `~/.config/agent-home` | The Vesica state directory or manifest path is a symlink or the wrong file type. | Inspect and repair the path, then rerun. |
| `recorded backup was modified` / `is not a regular file` | A backup Vesica made earlier has changed since. Install refuses to trust it. | Check the named `*.vesica-backup-*` file. Restore it, or move it aside after you've checked what changed. |
| `Repository cloning skipped: pass --repos-dir …` | Cloning is opt-in. | Add `--repos-dir ~/source/repos` (developer or lead profile). |
| `Skipped: <repo> (destination exists and is not a Git checkout; left untouched)` | That folder already exists. | Vesica never touches it. Move it if you want Vesica to clone there. |
| `Skipped: <repo> (existing checkout has a different origin (…))` | You already have a clone with a different remote (for example SSH versus HTTPS). | Nothing to do. Vesica leaves it alone. |
| `failed to clone <repo>: …; managed configuration was installed, rerun install to retry` | A network, auth or URL problem. No partial checkout is left behind. | Fix access (for example `gh auth login`), then rerun `install`. Already-cloned repos show as `Present`. |

## `vesica uninstall`

Uninstall removes or restores only what it can prove it created. Everything else is kept and
reported, and its manifest entry is retained so you can rerun after fixing it.

| Message | Meaning |
|---|---|
| `Skipped modified managed file: …` | You edited a file after Vesica wrote its block. It's kept exactly as it is. Remove the `# >>> vesica >>>` block by hand if you want it gone. |
| `Skipped altered backup` / `Skipped missing backup` | The backup no longer matches the original that was recorded, so restoring it would be unsafe. |
| `Skipped modified, missing or unverifiable symlink: …` | A link now points somewhere else. It's left alone. |
| `Retained N skipped manifest entries` | Some entries were kept for the reasons above. |

Uninstall never deletes cloned repositories or your notes.

## `vesica team status` / `team pull`

| Message | Fix |
|---|---|
| `uncommitted changes` | Commit, stash or discard your local changes in the shared-home checkout, then pull again. |
| `refusing non-fast-forward pull` / `upstream was rewound or diverged` | Local and remote history have diverged. Resolve it yourself with Git. Vesica never merges or rebases for you. |
| `… changed while fetching; refusing to pull` | Something changed the checkout during the pull. Rerun. |
| `configured upstream must name a remote branch` | Set an upstream with `git branch --set-upstream-to origin/main`. |

## Developing Vesica

- **Tests fail in a sandbox with `/tmp` mounted `noexec`.** Some tests execute small stub programs from temporary directories. Set `TMPDIR` (Rust tests) and `VESICA_TEST_TMP_ROOT` (shell tests) to an exec-capable directory. `tests/install.sh` detects this case and says so.
- **`content-guard` fails.** The public app repository must not contain team content. The rule ID (`VCG001`–`VCG005`) and the file:line tell you what to remove. Team material belongs in the private shared home.
- **`supply-chain` fails.** A dependency has a RustSec advisory or a licence outside `deny.toml`. Prefer replacing the dependency over adding an ignore.
