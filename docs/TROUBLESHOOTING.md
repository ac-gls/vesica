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
| `team.env_prefix must contain only ASCII letters, digits, and underscores, and must not start with a digit` | The prefix names shell variables (`<PREFIX>_HOME`). Use something like `TEAM`. |
| `repository … lists the core profile; the core profile has no repositories` | In `agent-home.toml`, list repos under `developer` and/or `lead`, or leave `profiles` empty for both. |
| `repository name … must be a single directory name …` / `option-like url` / `forbidden git transport` | Fix that entry in `agent-home.toml`. Names are one safe directory name, and URLs are ordinary `https://`, `git@` or local paths. |

## `vesica install`

Always start with `--dry-run`. It prints the plan and changes nothing.

| Message | Cause | Fix |
|---|---|---|
| `unmanaged destination already exists: …` / `install stopped on an existing destination` | Something that Vesica didn't create already sits where it would put a link (for example `~/Team` or a skill link). Nothing was changed. | Move or rename that file yourself, then rerun. Vesica never overwrites it. |
| State-path errors about `~/.config/agent-home` | The Vesica state directory or manifest path is a symlink or the wrong file type. | Inspect and repair the path, then rerun. |
| `recorded backup was modified` / `is not a regular file` | A backup Vesica made earlier has changed since. Install refuses to trust it. | Check the named `*.vesica-backup-*` file. Restore it, or move it aside after you've checked what changed. |
| `… contains a line break or NUL and cannot be written to config.env` / `… is not valid UTF-8 and cannot be written to config.env` | A path that would be exported (shared home, `--repos-dir`, or the home itself) can't be represented safely as a shell value. Nothing was changed. | Use a path without line breaks and with valid UTF-8 names. |
| The `<PREFIX>_HOME` variables are not set after install | `~/.bashrc` is only read by new interactive shells. | Open a new terminal, or run `. ~/.config/agent-home/config.env`. Login shells that skip `~/.bashrc` need it sourced from `~/.profile`. |
| `unmanaged destination already exists: <vault>/Team` | With `--vault`, the vault already has a `Team` that isn't the shared home (or a link Vesica didn't make). Nothing was changed, and the vault wasn't scaffolded. | Move it aside, or pass that checkout as `--home-repo` if it's the shared home. |
| `Kept modified stale link: ~/Team (no longer managed by Vesica)` | You switched to `--vault`, but the old `~/Team` link had been changed, so Vesica left it in place. | Remove it yourself if you no longer need it. |
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

## `vesica lint` (shared home)

Findings print `path:line:rule: message` and never the offending text.

| Rule | Meaning / fix |
|---|---|
| VSH001 | An area folder under `Knowledge/`, `Decisions/` or `Templates/` has no `README.md`. |
| VSH002 | A non-LFS file is over 1 MB (D28). Move it to Git LFS or shrink it. `.git/` is not checked. |
| VSH003 | A personal filesystem path (`/mnt/`, `/home/`, `C:\Users\`). |
| VSH004 | A wikilink doesn't resolve inside the shared home. |
| VSH006 | A credential pattern. Remove it and rotate the credential. |
| VSH007 | An email address in a Markdown note. |
| VSH008–VSH011 | Note frontmatter: missing or invalid, required fields, `authority` (`working`/`mirror`/`digest`), and `source` for mirrors and digests. |
| VSH012 | An `svg`/`png`/`jpg`/`jpeg`/`pdf` file isn't covered by a `filter=lfs` rule in the root `.gitattributes` (D28). Add e.g. `*.svg filter=lfs diff=lfs merge=lfs -text`. A later `-filter` line overrides an earlier one, as in Git. |
| VSH013 | A `SKILL.md` under `Agents/` or `Standards/` has missing or invalid YAML frontmatter. Quote values that contain `: `. |
| VSH014 | The skill's `name` doesn't match its folder name. |
| VSH015 | The skill's `description` is missing or blank. |

## `vesica sync-standards`

| Message | Meaning / fix |
|---|---|
| `opening a PR per repository needs separate approval; run … --dry-run` | Only the plan is implemented. Real PR creation stays gated. |
| `sync-standards needs --repos-dir <dir>` | Pass the directory that holds the code-repo checkouts (or set `VESICA_REPOS_DIR`). |
| `<repo>: not opted in (set standards = true in agent-home.toml)` | Add `standards = true` to that repo's `[[repos]]` entry in the shared home. |
| `<repo>: not cloned at <path>` | Clone it first (`vesica install --repos-dir <dir>`). Nothing was planned for it. |
| `No repository is opted in …` | No repo in the shared config sets `standards = true`. |
| `No Standards/*.md files in …` | The shared home has no `Standards/` content to sync. |

## `vesica new-skill`

| Message | Fix |
|---|---|
| `… has uncommitted changes; commit or stash them before scaffolding a skill` | The shared-home checkout is dirty. Commit or stash first; Vesica won't mix your edits into the scaffold commit. |
| `skill <name> already exists (Agents/Skills/<name>/SKILL.md)` | Pick another name, or edit the existing skill. |
| `branch skill/<name> already exists` | A previous attempt left the branch. Delete it (`git branch -D skill/<name>`) or use another name. |
| `skill name must be 1-64 lowercase ASCII letters, digits, '-' or '_' …` | Fix the name. It's also the folder name. |
| `git commit failed: Author identity unknown` | `new-skill` commits with **your** Git identity. Set `git config --global user.name` and `user.email`. |
| `… is not a shared home (no Agents/ directory)` | `--home-repo` must point at a shared-home checkout. |

## `vesica update`

| Message | Meaning / fix |
|---|---|
| `Already up to date: vesica X (vY)` | The resolved tag is already installed. Nothing to do. |
| `no published release found for the stable channel` | Only pre-releases exist so far. Use `--channel alpha` (the default), or wait for a stable release. |
| `checksum mismatch for …: refusing to replace the binary` | The download doesn't match `SHA256SUMS`. The installed binary is untouched. Retry; if it persists, don't work around it. |
| `SHA256SUMS has no checksum for …` | The release is incomplete or was tampered with. Nothing was installed. |
| `the released binary reports '…', expected '…'` | The artifact and the tag disagree. Nothing was installed. Report it. |
| `download failed: …` | Network, or the tag/release doesn't exist. `GITHUB_TOKEN`/`GH_TOKEN` raises the API rate limit. |
| `tag must look like vX.Y.Z or vX.Y.Z-pre.N` | Fix `--tag`. |
| `No manifest recorded at …; skipping re-link` | The binary was replaced, but there's no install to re-link. Run `vesica install` if you expected one. |

## `vesica vault`

| Message | Fix |
|---|---|
| `… already exists and is not empty; use vesica vault fix …` | `init` only creates new vaults. To adopt an existing folder (it only adds what's missing), use `vault fix`. |
| `VVC001 <entry>: required vault entry is missing` | Run `vesica vault fix --path <vault>`. |
| `VVC002 Team: Team is not excluded by .gitignore` | Add a line `/Team` (`Team/` alone misses a symlinked `Team`), or run `vault fix`. |
| `VVC002 Team: Team is tracked by the vault repository` | Run `git rm -r --cached Team` in the vault, then commit. The shared home is its own repository. |
| `VVC003 .: vault is not a Git repository` | Run `vault fix`, which runs `git init`. |
| `VVC005` / `VVC006` frontmatter findings | Add `title, created, updated, type, area, tags` (tags as a list), and use an allowed `type` (see `AGENTS.md` in the vault). |
| `refusing to snapshot: possible secrets in <file>:<line>` | Remove the credential from the note. Nothing was committed. |
| `git commit failed: … Please tell me who you are` | Snapshots use your own Git identity. Set `git config --global user.name` and `user.email`. |

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
