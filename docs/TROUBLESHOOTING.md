# Vesica Troubleshooting

Vesica is a prototype for disposable roots. Preflight refusals avoid managed mutations; later failures may leave accurate resumable state, retained owner edits or recovery journals. Read the complete diagnostic before retrying. Do not delete a retained journal/branch/backup blindly, and do not assume a nonzero exit means nothing changed.

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

## Agent wiring

| Message | Meaning |
|---|---|
| `Partial wiring: Claude Code, Codex, OpenCode and Copilot CLI. Role agents and Hermes runtime configuration are not wired yet.` | Expected for now. |
| `Skipped modified merged file: …` (uninstall) | You edited a file Vesica merged into (for example `~/.config/opencode/opencode.json`) after installing. It's kept as-is, and its manifest entry is retained. |
| `Restoring original merged file: …` | Uninstall restored your pre-merge file byte-for-byte. |

## `vesica share`

| Message | Fix |
|---|---|
| `Nothing was written. Re-run with --yes to apply` | Expected: `share` only plans without `--yes`. |
| `… is outside the vault …` | `share` only promotes notes inside the selected personal vault (use `--vault`, `VESICA_VAULT`, or local config); `--root` selects machine state, not the vault. |
| `… is missing a valid YAML frontmatter block` | Give the note `title, created, updated, type, area, tags` (see the vault contract). |
| `link [[…]] would point outside the shared home` | The link would break inside the team repo. Fix it, or pass `--drop-unresolved-links` to keep just its text. |
| `<shared>: has no Knowledge/<area>` | Create that area in the shared home first (it needs a `README.md`). |
| `… already exists in the shared home` | The note is already there. Edit the shared copy, or pick another note. |
| `… has uncommitted changes; commit or stash them before sharing` | Your shared-home checkout is dirty; Vesica won't mix your edits into the promotion commit. |
| `branch share/<note> already exists` | A previous attempt left the branch. Delete it (`git branch -D share/<note>`) or finish the PR. |
| `shared-home lint failed (…)` / `vault-scan found … credential pattern(s)` | The gate blocked the commit; inspect any additional rollback/conflict diagnostic before retrying. Complete rollback permits a safe retry; incomplete rollback can retain the promotion to keep links valid. |
| `cannot read your Git identity; … or pass --owner` | Set `git config --global user.name`, or pass `--owner`. |

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

## `vesica doctor`

Beyond the blocker rows above:

| Output | Meaning |
|---|---|
| `Optional git identity: found: <name>` | Commits from `vault snapshot`, `new-skill` and `share` will be attributed to that identity. |
| `Optional git identity: missing (needed by vault snapshot, new-skill and share)` | Install still works. Set `git config --global user.name` and `user.email` before using the commit-writing commands, or they fail with Git's "Author identity unknown". |

## `vesica update`

| Message | Meaning / fix |
|---|---|
| `Already up to date: vesica X (vY)` | The resolved tag is already installed; manifest relink may still run. Dry-run reports pending relink without mutation. |
| `no published release found for the stable channel` | Only pre-releases exist so far. Use `--channel alpha` (the default), or wait for a stable release. |
| `checksum mismatch for …: refusing to replace the binary` | The download doesn't match `SHA256SUMS`. The installed binary is untouched. Retry; if it persists, don't work around it. |
| `SHA256SUMS has no checksum for …` | The release is incomplete or was tampered with. Nothing was installed. |
| `the released binary reports '…', expected '…'` | The artifact and the tag disagree. Nothing was installed. Report it. |
| `download failed: …` | Network, or the tag/release doesn't exist. `GITHUB_TOKEN`/`GH_TOKEN` raises the API rate limit. |
| `tag must look like vX.Y.Z or vX.Y.Z-pre.N` | Fix `--tag`. |
| `No manifest recorded at …; skipping re-link` | The binary was replaced, but there's no install to re-link. Run `vesica install` if you expected one. |

### Recovery and scan outcomes

- `pending recovery at ... .vesica-relink-*`: no new migration starts. Preserve the journal and inspect its reported conflicting paths. Resolve owner edits explicitly or restore the recorded postimage before a retry; do not delete the evidence to bypass refusal.
- `changed during operation; preserving user changes`: rollback refused to overwrite an intervening node/byte/mode replacement. Both the primary failure and retained recovery evidence matter.
- `uninstall target changed before mutation`: the target was replaced after planning; the pending manifest records remain for a reviewed retry.
- `cannot prepare local configuration ... Permission denied`: atomic persistence needs a writable real state parent, even if `config.toml` is readable. Preparation happens before unmanaged vault initialization.
- `vault-scan` reports scanned text separately from excluded/unscanned assets; snapshot and share/lint gates also report asset exclusions to stderr before commit. Images/PDFs are not inspected for credentials; unsupported binary text, oversized text and read errors are not clean results. Diagnostics contain locations/rules, never matched tokens.

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

## Install stale cleanup and returned-error recovery

Ordinary install accepts machine facts and its manifest only when managed actions
and stale symlink/generated-file cleanup have succeeded together. If a stale
removal or subsequent manifest persistence fails, it attempts reverse restoration
of the removed nodes plus local configuration and managed actions. Existing local
configuration bytes and mode, or its prior absence, are restored on a fully
recovered failure. A failed removal can therefore be retried after the owner
resolves the original access obstruction; do not bypass it by deleting state.

New generated files are written and synced in exclusive private siblings before
no-clobber publication, so a returned short-write error does not publish a partial
final target. Cleanup verifies the created staging node identity, type, mode and
expected byte prefix. A replaced or edited staging node is retained with cleanup
diagnostics rather than blindly deleted. This is guarded returned-error recovery,
not an arbitrary-interruption or race-proof guarantee.

The executor records the writer's known bytes and prepared permissions when a
mutation succeeds, before fallible post-mutation verification. Verification
errors stop acceptance and use that recorded state for guarded undo; owner-edited
bytes, type or mode remain conflicts rather than being overwritten.

Backup ownership remains active across every fallible acceptance step after
creation, including manifest snapshot acquisition before local persistence. On
returned acceptance errors, explicit ownership-aware backup finalization waits
until executor action rollback has finished. If action or nested local/manifest
restoration is incomplete, original backups are retained with their paths for
owner inspection; fully restored failures clean only still-owned backups. Backup
creation errors also transfer the incrementally recorded inventory to that outer
owner: a later create/write/sync error must not delete earlier completed originals
before rollback. Only the current known incomplete output is creator-cleaned;
failed partial cleanup is retained and reported, not destructively retried by the
outer finalizer. Fully written originals remain recovery evidence even when sync
fails. Finalization cleanup errors are appended to the primary failure, not discarded by a silent
destructor. Manifest entry construction must also return required backup and
installed regular-file read errors rather than accepting missing hashes. This
restores already-persisted local configuration through the same guarded error
path. Empty regular files receive their valid SHA-256 hash; symlink/directory
actions deliberately have no byte hash and do not read a referent. This does not
add panic/crash recovery.

Failed backup writes also clean up only the recorded created node: identity, type,
mode and the known expected byte prefix must match. Completed backups require exact
bytes before cleanup. A replacement or removal failure retains the backup and
reports its path and cleanup error alongside the original write failure; inspect
that artifact before owner-led retry. Successful backup writes are synced before
transaction acceptance. Prefix-equivalent edits cannot always be distinguished
from this writer's partial output; these guards are not atomic confinement.

A stale node is revalidated against its captured type, bytes/link target and mode
before removal. If it changed, removal refuses. If a replacement appears at an
already-removed path before rollback, the replacement is kept and the primary
failure includes cleanup/rollback diagnostics. Preserve the original manifest,
backups and any updater recovery journal for owner-led inspection; a failed
rollback is not a successful install. A bare ordinary install does not introduce
a durable standalone recovery journal.

This boundary covers guarded recovery from returned errors during managed
installation and stale cleanup, not crash/power-loss recovery, race-proof path
confinement or rollback of personal vault scaffolding and repository clones.
Those personal/unmanaged resources remain outside managed ownership.

## Share preflight and failure-return recovery

Before creating the share branch, apply captures original rewrite bytes, real-file
paths and permissions, and computes the complete planned rewrite set. It briefly
creates/removes an exclusive empty private sibling in each affected personal
parent to check current staging access (also the source parent for `--move`).
`cannot preflight rewrite ... Permission denied` refuses before shared Git/content
mutation. Empty preflight siblings are not durable recovery journals.

Immediately before each rewrite, apply revalidates its real path/type, original
bytes and permissions. `rewrite changed after preflight` preserves an intervening
personal edit and attempts guarded rollback of earlier owned rewrites. A normal
fully restored failure can be retried safely. If personal restoration conflicts,
the promotion/share branch is retained to avoid deleting the target of a remaining
rewritten link; repeated apply refuses the occupied destination without overwriting
it. Preserve that state and resolve it with the machine owner; do not delete the
promotion to bypass refusal. Promotion permissions are captured from the created
file before Git hooks run. A subsequent type, byte or permission change refuses
destructive destination cleanup and retains the promotion/share branch with
`destination cleanup refused`; a permission-only owner edit is not discarded.

This is best-effort recovery from a returned error, not crash recovery, a
standalone recovery command, or an atomic no-follow/race guarantee. Parent access
can change after preflight; source-removal errors still run guarded rollback.
Semantic Git index entries are restored after normal rollback, not necessarily
byte-identical index metadata or reflog history.

## Personal vault write boundaries

`vault init`, `fix`, `new-note`, `snapshot`, and install/update re-link adoption
refuse personal paths containing `..` or existing symlink components before
personal writes or installer machine preparation. Use a direct, real-directory
path; ordinary relative paths without parent traversal remain supported.

Personal storage must be separate from every known shared-home root. When shared
storage is inside the personal vault, it must be under `<personal>/Team`, the
subtree excluded from personal upkeep and snapshots. A known shared root under
`Sessions`, `Archive`, or any other non-`Team` subtree is refused, even when that
shared directory has not been created yet. Choose separate storage or relocate
shared storage under `Team` yourself; Vesica does not move or delete it for you.

The check considers the selected machine's local `home_repo`, all of
`VESICA_HOME_REPO`, `AGENT_HOME_REPO`, `VESICA_TEAM_HOME`, and the explicitly
selected install/re-link shared root. An override does not hide another known
shared boundary. For standalone writers, `--root` selects machine configuration
with precedence over `VESICA_ROOT`, then `AGENT_HOME_ROOT`, then `HOME`.

These are conservative metadata checks, not an atomic no-follow guarantee against
concurrent filesystem replacement. Keep real/private data out of disposable trials
until the separate rollout gates have been accepted.


## Private configuration permissions

On supported Unix hosts, new managed regular configuration files (Bash blocks,
Claude/Codex instruction files, merged JSON, `config.env`, machine-local
`config.toml`, and `manifest.json`) request mode `0600` at creation. A umask may
restrict that further; it cannot add group/other access. Backups and atomic
replacement staging also start private (`0600`), before any content is written.
The tested umasks are `000`, `022`, and `077`.

Replacement of existing managed content targets and machine-local `config.toml`
preserves their current permissions, including an owner-selected `0640`.
**Exception:** `manifest.json` is rewritten through a new private file requesting
`0600` (subject to umask); owner-added group access on that manifest is not
preserved. Executor rollback restores captured managed-content preimage permissions. Uninstall restores original bytes while preserving
the still-owned target's current permissions; it does not infer a historic mode
from a backup's `0600`. The schema-1 manifest does not persist original modes, so
there is no mode-field migration in this change and no automatic reconstruction
of permissions lost by an older release. Node/type/content checks and retained
owner edits still apply. Unix mode tests are not Windows ACL validation.

### Owner-led legacy backup exposure review

A newer Vesica binary does **not** automatically tighten old backup files. Do not
run an exposure audit or repair against a real home without explicit machine-owner
approval of the selected paths and the read-only/repair scope. Disposable tests do
not grant that approval.

1. Stop Vesica and other writers. Have the owner identify the affected install
   root and exact backup/config paths from their records. Use direct paths with
   real directory components; do not trust an editable manifest as authorization
   to inspect arbitrary locations. Preserve existing recovery evidence privately.
2. For each approved path, inspect metadata only; never print file content. On
   Linux, for an individually reviewed path:

   ```sh
   stat -c 'mode=%a uid=%u links=%h type=%F path=%n' -- '/reviewed/direct/path'
   ```

   Check every ancestor separately for symlinks; `stat` does not make ancestor
   traversal safe. Stop on a symlink, non-regular file, unexpected owner or hard
   link count greater than one. Record mode, owner, link count and parent-directory
   access/ACLs privately. Do not use recursive scans or publish home-path inventories.
3. Group/other bits on a secret-bearing backup require owner review even if a
   parent currently blocks traversal. Inspect ACLs with the host's ACL tools when
   applicable; mode bits alone do not establish who could read it. Tightening now
   cannot revoke past copies: review credential rotation if exposure was possible.
4. Obtain a **separate explicit repair approval** for named files. Tighten a
   confirmed private backup to `0600`; preserve an intentional configuration mode
   such as `0640` unless its owner chooses otherwise. Use a no-follow opened file
   handle, verify its type/owner/link count and identity again, then change only
   that file's mode and verify it through the same handle. Stop on a changed node;
   do not use blind `chmod`, recursive repair, content rewriting or backup deletion.
   Repair ACLs only under a separately reviewed host-specific policy.
5. Record before/after metadata without contents, and retry Vesica only after the
   owner has resolved any retained entries. Until this procedure is approved and
   executed on the affected real machine, legacy exposure remains unassessed.

## Legacy relative manifests

`manifest ... path is not absolute` means an older or edited manifest contains a
relative `shared_home`, `vault`, entry `target`, `backup` or `source`. Install,
including `--dry-run`, and uninstall refuse it before managed changes. **Do not
rebase using the current directory**, delete the manifest, discard skipped entries,
or remove backups merely to bypass this guard. No automatic migration is performed.

1. Stop writes and preserve a private copy of the manifest, machine configuration
   and backups with their original node types and permissions. Never paste those
   files or their contents into a public issue.
2. With the machine owner, establish the original install base from trustworthy
   installation records or a known-good snapshot. The current working directory,
   current `--root`, or a plausible existing filename is not evidence of that base.
   If it cannot be established, keep refusal in place and obtain an owner-led
   recovery plan; do not guess absolute paths.
3. If the base is proven, review a **copy** of the manifest with the owner. Resolve
   each relative field against its proven original base, checking entry keys and
   target correspondence, backup identity and hashes, and any traversal or symlink
   ancestry. Validate all resulting paths against the selected machine root and
   independently authorized local vault configuration. Absolute spelling alone
   does not grant permission to touch an outside path. Preserve schema and hashes;
   do not claim a hash proves filesystem ownership.
4. Only with explicit repair approval, install the reviewed manifest copy with
   private permissions while writers remain stopped. Preview with
   `vesica install --home-repo <shared> --root <machine> --dry-run`. This checks
   manifest absolute paths and scope, but **is not a complete type, ownership or
   backup-integrity validator**: an altered backup can produce a clean preview
   while a real install refuses it. Have the owner separately review target types,
   ownership and backup integrity; do not run a mutating install merely to test
   them. A clean plan is not permission to apply it. The real operation still must
   pass its own type, backup, containment and local-config authorization checks.
   Review the plan and retained state with the owner before approving any real
   install/uninstall. No recovery was performed automatically by the diagnostic.
