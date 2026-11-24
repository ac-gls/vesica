# Vesica Architecture

> **Status: prototype / incomplete.** Vesica is the renamed product identity of the historical Agent Home Kit project; paths and repo names retain compatibility identifiers. This document describes the code that exists in this checkout, not the aspirational multi-agent installer described in older planning notes. Do not use it to install a production team setup yet.
## Current executable flow
```text
vesica CLI
  ├─ doctor   parse team config/catalog; Linux preflight (platform, required/optional commands,
  │           node minimum); blockers exit non-zero; WSL status informational; optional --json
  ├─ install  build a plan; preflight destinations/manifest; back up existing
  │           Claude/Codex instruction files and ~/.bashrc; execute supported actions; save manifest
  ├─ update   resolve a release (or --tag), curl the tarball + SHA256SUMS, verify the SHA-256 and
  │           that the binary reports the tag's version, rename it into place, re-link from the
  │           recorded manifest through the new executable; guarded binary + managed-state
  │           recovery on failure; conflicting edits/journals are retained and reported
  ├─ share    person-confirmed promotion of a personal note into the shared home: plan only
  │           without --yes; branch share/<note>; rewrite vault-view links; add owner/authority;
  │           lint + vault-scan gate with guarded rollback; never writes trunk; optional push
  ├─ sync-standards  --dry-run plans Standards/* -> <repo>/.github/instructions/*.instructions.md
  │                  for repos with `standards = true`; writes nothing
  ├─ new-skill scaffold Agents/Skills/<name>/SKILL.md on a new skill/<name> branch (committed,
  │           never pushed); refuses a dirty checkout, an existing skill or branch, unsafe names
  ├─ status   read the install manifest and report recorded entries
  ├─ uninstall verify hashes/links, then remove or restore only managed entries
  ├─ team status/pull  inspect local shared-home refs / guarded fast-forward pull
  ├─ lint     read-only shared-home checks with secret-safe diagnostics
  ├─ vault-scan recursively check bounded-size UTF-8 files for common secret patterns
  ├─ vault init/fix   create (empty dir only) or adopt (add missing, never overwrite) the
  │                   embedded skeleton (assets/vault-skeleton); ensure /Team is ignored; git init
  ├─ vault check      read-only contract check (VVC001 required entries, VVC002 Team excluded
  │                   and untracked, VVC003 Git repo, VVC005/006 frontmatter and type)
  ├─ vault new-note   ledger note from assets/templates/ledger-note.md; never overwrites
  ├─ vault snapshot   commit with the person's Git identity; refuses unless Team is ignored and
  │                   untracked, and if a file to be committed matches a secret pattern (D30)

shared-home checkout
  ├─ AGENTS.md                         → ~/Team symlink, or <vault>/Team with --vault (install
  │                                      step 3; vault created or adopted via ah-vault)
  ├─ AGENTS.md                         → managed import in Claude and Codex files, merged into
  │                                      OpenCode's instructions list
  ├─ agent-home.toml team.env_prefix   → ~/.config/agent-home/config.env (<P>_HOME, <P>_PROFILE,
  │                                      <P>_REPOS for developer/lead with --repos-dir), sourced
  │                                      by a managed block in ~/.bashrc
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

## Configuration schemas

`schemas/agent-home.schema.json` and `schemas/foundry.catalog.schema.json` (JSON Schema draft 2020-12) describe the two shared-home files. They mirror the parser exactly:
- required fields and the schema version;
- the `env_prefix` pattern and the profile enum;
- repository name and URL safety, and no `core` in a repository's `profiles`;
- the recursive credential-key ban;
- unknown keys allowed, as the parser ignores them.

`repos[].standards` (boolean, default `false`) is the per-repo opt-in for `vesica sync-standards` (decision D-I).

The TOML schema validates the document after conversion to JSON (for example with Taplo's `#:schema` directive). `crates/ah-core/tests/schema_parity.rs` is the contract: every corpus case must get the same verdict from `AgentHomeConfig::parse`/`FoundryCatalog::parse` and from the schema (validated with `boon`, a dev-only dependency). A rule changed in one place and not the other fails CI.

## Agent wiring

| Harness | Rules | Skills |
|---|---|---|
| Claude Code | managed block in `~/.claude/CLAUDE.md` | `.claude/skills/<name>` |
| Codex | managed block in `~/.codex/AGENTS.md` | `.agents/skills/<name>` |
| OpenCode | `instructions` merged into `~/.config/opencode/opencode.json` (JSONC) | `.agents/skills/<name>` |
| Copilot CLI | `COPILOT_CUSTOM_INSTRUCTIONS_DIRS` in `config.env` | `.agents/skills/<name>` |
| Hermes (lead) | printed command plan (`skills.external_dirs`, `skills.create_dir`) | shared `Agents/Skills` + `Agents/Hermes/Skills` |

Merged files are a first-class manifest kind (`merged-file`): backed up before the first merge, verified on reinstall, and restored byte-for-byte by `uninstall`. **OpenCode-only role generation is implemented locally, not yet published**, at `~/.config/opencode/agents/<prefix>-<name>.md` with generated-file ownership and repeat-install/uninstall tests. Claude/Copilot roles remain unsupported. Hermes runtime configuration is still a printed plan, edited by `hermes` itself.

## Promotion (`share`)

`vesica share` is the only path by which personal content reaches the team's shared home, and it is
always person-invoked (plan §5.6, D26). The properties that matter:

- **Confirmation.** `--yes` is required to write anything; without it the command prints the plan.
- **No trunk writes.** Everything happens on `share/<note>`. Recoverable failures attempt restoration of personal bytes/modes/types and shared branch/index/content. Intervening edits are preserved and rollback errors reported; incomplete personal rollback retains the promotion rather than deleting the target of a retained rewritten link. This is not arbitrary crash recovery.
- **Links.** `[[Team/…]]` is the vault's view of the shared home, so promoting rewrites those links
  to the shared-home path; a link that resolves nowhere in the shared home is refused unless
  `--drop-unresolved-links` keeps its text. Inbound personal links to the promoted note become
  `[[Team/…]]`.
- **Gate.** `lint_shared_home` and `scan_secrets` run against the staged tree before the commit.
- **Transport.** The command never pushes; `--open-pr` pushes the branch and leaves PR creation to
  `gh`. Real submission is not exercised in tests.

## Git usage

Git subprocesses use `crates/ah-vault/src/git.rs` to remove inherited repository/index/object selectors. `isolated` blanks system/global config; `user` retains identity/hooks for commits. Mutators validate effective Git storage and refuse linked-worktree mutations; read-only linked-worktree inspection is supported. `share --open-pr` explicitly pushes the branch and prints instructions; it does not create a PR. Tests use local bare remotes only.

## Safety and transaction

The CLI preflights unsafe state paths. Installer mutations, backup creation, and manifest persistence now share a best-effort rollback transaction. Manifest writes use exclusive temporary-file creation, syncing before atomic replacement, and mode `0600` for new Unix manifests. Backups are created exclusively and only removed on transaction failure if their bytes remain unchanged; cleanup/rollback errors are included in the reported failure. Tests cover manifest temp-file and symlink collisions, user-content preservation, and CLI install rollback.

## Validation

- `cargo fmt --all -- --check`
- `cargo test --workspace --quiet`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build -p vesica`
- `VESICA_TEST_TMP_ROOT=/mnt/wsl-home bash tests/install.sh`
- `shellcheck install.sh tests/install.sh tests/lifecycle-linux.sh scripts/content-guard.sh tests/content-guard.sh`
- `git diff --check`
- `bash scripts/content-guard.sh` and `bash tests/content-guard.sh`: the content-free guard (plan §5.5). It flags org names/domains (VCG001), non-placeholder GUIDs (VCG002), internal/Azure resource hostnames (VCG003), `SKILL.md`/`AGENTS.md` outside `tests/fixtures/` (VCG004) and gitleaks findings over the full history (VCG005). Findings report rule, file and line only. CI pins gitleaks 8.30.1 by SHA-256.
- `cargo deny --locked check` and `cargo audit --deny warnings`: licences (minimal allow-list), bans, sources and RustSec advisories, including unmaintained crates.

These are Linux/WSL development-container results. Installer fixtures simulate Linux and non-Linux host identity and use a local tarball; they do not establish a real native-Linux distribution matrix, real profile install, or signed-release validation. Windows interoperability is optional and unvalidated. The transaction cannot guarantee success if the OS refuses rollback or a user replaces files concurrently; it avoids clobbering changed content and reports remaining errors. Vesica remains a prototype, not pilot/production-ready.

1. The shared-home repository holds team content; the installer repository does not.
2. Config/catalog parsing rejects credential-like keys.
3. The installer preflights all planned destinations and validates the state directory/manifest with symlink-aware metadata before creating backups or executing actions. An unmanaged collision or modified manifest-managed target fails closed.
4. Existing Claude/Codex files are backed up before managed-block changes. Repeat installs reuse recorded backups; backups use exclusive creation and are removed if a failed plan/manifest write rolls back, unless their content changed in the meantime.
5. Action execution and manifest persistence share a rollback transaction: if manifest commit fails after actions succeed, the executor attempts reverse-order rollback and reports rollback errors without overwriting intervening edits.
6. Manifest writes use a unique-create temp file, restrictive Unix permissions, flush/sync, then rename; a preexisting temp file or symlink is not followed or overwritten.
7. The JSON manifest records install targets, source paths, backups, and hashes. Uninstall restores/removes only entries it can verify; ambiguous or modified content is retained and reported. A backed-up managed file is restored only when it still matches the installed hash *and* re-applying the install transform to the verified backup reproduces it exactly. The backup bytes are then written back, so CRLF endings, a missing final newline and other formatting round-trip byte-for-byte.
8. Symlinks are created only at absent destinations or accepted if already correct; existing user destinations are never replaced.
9. Secret scan output contains locations only. Text candidates are limited to 2,000,000 bytes; unsupported binary, oversized or unreadable text fails closed. Known binary assets are exempt from content inspection, not proof of clean content; explicit scan reports and snapshot/share mutation gates list excluded assets. Symlinks are not traversed.
10. Install stages/syncs a private local-config replacement before personal vault scaffolding, then commits machine facts only after managed actions succeed. Vault scaffolding and repository clones are unmanaged and kept; later runtime failures do not promise their removal.
11. Uninstall checkpoints completed ownership removals and retains pending/modified entries. A failed progress save attempts guarded restoration of the current mutation; retries use the accurate remaining manifest.
12. Update recovery spans binary replacement, installer-managed relink acceptance and staging cleanup. Disk `.vesica-relink-*` journals retain managed pre/post state on conflict or cleanup failure. Outer binary recovery is in memory, not a crash-durable journal; a candidate's arbitrary writes are outside the installer-managed boundary. See the tests in `update.rs` and `remediation.rs`.

## Release

`vesica update` consumes a published release: it resolves the newest tag from the GitHub releases API (or `--tag`), downloads `vesica-<tag>-x86_64-linux.tar.gz` and `SHA256SUMS` with `curl`, verifies the checksum in-process, and requires the extracted binary to report the tag's version before an atomic `rename`. `VESICA_RELEASE_BASE_URL` and `VESICA_RELEASES_API_URL` point the commands elsewhere (tests use `file://`).

`scripts/build-release.sh <tag>` builds the `x86_64-unknown-linux-musl` binary and checks that it's fully static: no `INTERP` segment and no `NEEDED` entries. It's static-PIE, which keeps ASLR; older `file` versions call that "dynamically linked". It also checks that the binary reports the tag's version. It packages `vesica-<tag>-x86_64-linux.tar.gz` reproducibly (commit timestamp, sorted entries, `gzip -n`) and writes `SHA256SUMS`. It refuses a tag that doesn't match the workspace version or `install.sh`'s default version.

- CI `release-dry-run` runs `tests/release.sh` (artifact, checksum, static, bootstrap-install and mismatched-tag checks), then S7/S8 against the musl binary.
- `.github/workflows/release.yml` runs on a `v*` tag: build, then a GitHub Release (a pre-release when the tag has a `-` suffix, with notes taken from that CHANGELOG section), then a `verify` job on Ubuntu 24.04 and 22.04 that bootstraps from the **published** release with `install.sh` and runs S7/S8 against that binary.

## Known limitations

- `bash tests/lifecycle-linux.sh` (CI job `lifecycle`) exercises the real binary through bootstrap → doctor → dry-run → install → repeat install → refusal cases → uninstall, plus personal-vault/shared-home Git isolation and `team status/pull`, in a disposable HOME. In CI it runs on GitHub-hosted Ubuntu VMs with `VESICA_REQUIRE_NATIVE=1`; local WSL/container runs are labelled supplemental.
- `bash tests/install.sh` builds a disposable fixture release and checks Linux host/architecture gating, bootstrap-only installation, preservation of spaced CLI arguments, and checksum rejection without side effects. It is network-free; CI runs it alongside ShellCheck. It does not establish the plan's complete install → doctor → idempotent reinstall parity acceptance.
- Private-team cloning/bootstrap remains incomplete; rollback covers bounded installer action/manifest failure cases only.
- Profile selection is recorded in the manifest; both `doctor` and `install` report the repositories assigned to the selected profile. With `--repos-dir` (or `VESICA_REPOS_DIR`), `install` clones missing repositories for the `developer` and `lead` profiles (plan §5.4 step 10). It plans read-only before any mutation, never modifies an existing destination (a matching checkout is `Present`; anything else is `Skipped` and left untouched), clones through a temporary sibling directory so a failure leaves no partial checkout, disables Git prompts and the `ext`/`fd` transports, and reports clone failures with a non-zero exit after the managed configuration is installed. Clones are not manifest entries; `uninstall` keeps repositories. The `core` profile has no repositories: config rejects `core` in a repo's `profiles`, and an empty list means developer and lead. Every profile links the full shared skill set (plan §3).
- Claude/Codex instruction blocks, OpenCode instruction merging, Copilot instruction-directory exports and shared skills are wired. OpenCode role generation is local/unpublished; Claude/Copilot roles are unsupported. Hermes runtime edits, Foundry and Herdr remain unimplemented/gated.
- WSL detection exists, but Windows profile discovery, drvfs/OneDrive validation, `winget`, Obsidian, scheduled tasks, and Windows-side operations do not.
- Vault scan is a heuristic check for common patterns, not a complete DLP/secret detector; absence of findings is not proof that a vault is secret-free.
- Native Ubuntu 22.04/24.04 lifecycle and release-verification CI jobs exist; historical green runs do not verify this uncommitted tree. Local WSL/container evidence is supplemental. No signed/publisher-authenticated release path or arbitrary power-loss recovery is claimed.

## Verification

From the repository root:

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p vesica
```

CI additionally runs ShellCheck over `install.sh`. Current integration tests use temporary homes and the fixture at `tests/fixtures/shared-home`; they do not establish readiness on a real user's WSL/Windows profile.
