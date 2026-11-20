mod preflight;
mod release;
mod repos;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(name = "vesica", version, about = "Vesica team agent installer")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Debug, Subcommand)]
enum Command {
    Doctor(DoctorOptions),
    Install(InstallOptions),
    Status(RootOptions),
    Uninstall(UninstallOptions),
    Lint(LintOptions),
    VaultScan(VaultScanOptions),
    Team(TeamOptions),
    /// Personal vault: create, adopt, check, ledger notes and snapshots
    Vault(VaultOptions),
    /// Download a release, verify it and replace this binary, then re-link
    Update(UpdateOptions),
    /// Scaffold a skill on a branch in the shared home
    NewSkill(NewSkillOptions),
    /// Plan copying Standards/* into opted-in code repositories
    SyncStandards(SyncStandardsOptions),
    /// Promote a personal note into the shared home as a reviewed PR
    Share(ShareOptions),
}
#[derive(Debug, Clone, Copy, ValueEnum)]
enum ProfileArg {
    Core,
    Developer,
    Lead,
}
impl ProfileArg {
    fn text(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Developer => "developer",
            Self::Lead => "lead",
        }
    }
}
#[derive(Debug, clap::Args)]
struct Options {
    #[arg(
        long,
        env = "AGENT_HOME_REPO",
        env = "VESICA_HOME_REPO",
        help = "Local team shared-home checkout (or set home_repo in the local config)"
    )]
    home_repo: Option<PathBuf>,
    #[arg(long, value_enum, help = "Select a team profile")]
    profile: Option<ProfileArg>,
}
#[derive(Debug, clap::Args)]
struct DoctorOptions {
    #[command(flatten)]
    options: Options,
    #[arg(long, help = "Emit the doctor report as JSON")]
    json: bool,
}
#[derive(Debug, clap::Args)]
struct InstallOptions {
    #[command(flatten)]
    options: Options,
    #[arg(long, env = "AGENT_HOME_ROOT", env = "VESICA_ROOT")]
    root: Option<PathBuf>,
    #[arg(long)]
    dry_run: bool,
    #[arg(
        long,
        env = "VESICA_REPOS_DIR",
        help = "Clone missing profile repositories into this directory (developer/lead profiles)"
    )]
    repos_dir: Option<PathBuf>,
    #[arg(
        long,
        env = "VESICA_VAULT",
        help = "Create or adopt this personal vault and link the shared home at <vault>/Team"
    )]
    vault: Option<PathBuf>,
}
#[derive(Debug, clap::Args)]
struct VaultScanOptions {
    #[arg(long)]
    path: PathBuf,
}

#[derive(Debug, clap::Args)]
struct LintOptions {
    #[arg(
        long,
        env = "VESICA_HOME_REPO",
        env = "AGENT_HOME_REPO",
        help = "Shared-home repository to lint"
    )]
    home_repo: PathBuf,
}

#[derive(Debug, clap::Args)]
struct TeamStatusOptions {
    #[arg(long, env = "VESICA_TEAM_HOME")]
    path: Option<PathBuf>,
}
#[derive(Debug, clap::Args)]
struct TeamPullOptions {
    #[arg(long, env = "VESICA_TEAM_HOME")]
    path: Option<PathBuf>,
}
#[derive(Debug, clap::Args)]
struct TeamOptions {
    #[command(subcommand)]
    command: TeamCommand,
}
#[derive(Debug, Subcommand)]
enum TeamCommand {
    Status(TeamStatusOptions),
    Pull(TeamPullOptions),
}
#[derive(Debug, clap::Args)]
struct ShareOptions {
    /// Personal note to promote (must be inside the vault)
    note: PathBuf,
    #[command(flatten)]
    options: Options,
    #[arg(long, env = "AGENT_HOME_ROOT", env = "VESICA_ROOT")]
    root: Option<PathBuf>,
    #[arg(long, help = "Destination area under Knowledge/ in the shared home")]
    area: Option<String>,
    #[arg(long, help = "Promote into Decisions/ instead of Knowledge/<area>")]
    decisions: bool,
    #[arg(
        long,
        help = "Owner recorded in the promoted note frontmatter (default: your Git name)"
    )]
    owner: Option<String>,
    #[arg(
        long,
        help = "Confirm: apply the promotion. Without it the plan is printed and nothing is written"
    )]
    yes: bool,
    #[arg(
        long = "move",
        help = "Remove the personal note once the promotion is committed"
    )]
    move_source: bool,
    #[arg(
        long,
        help = "Convert links that cannot resolve in the shared home to plain text"
    )]
    drop_unresolved_links: bool,
    #[arg(long, help = "Push the branch and open the PR (needs gh auth)")]
    open_pr: bool,
    #[arg(long)]
    dry_run: bool,
}
#[derive(Debug, clap::Args)]
struct SyncStandardsOptions {
    #[command(flatten)]
    options: Options,
    #[arg(
        long,
        env = "VESICA_REPOS_DIR",
        help = "Directory holding the code-repo checkouts"
    )]
    repos_dir: Option<PathBuf>,
    #[arg(
        long,
        help = "Plan the sync without writing (the only implemented mode)"
    )]
    dry_run: bool,
}
#[derive(Debug, clap::Args)]
struct NewSkillOptions {
    /// Skill name (lowercase, digits, '-' or '_'; also the folder name)
    name: String,
    #[arg(
        long,
        env = "VESICA_HOME_REPO",
        env = "AGENT_HOME_REPO",
        help = "Shared-home checkout to scaffold the skill in"
    )]
    home_repo: PathBuf,
    #[arg(
        long,
        help = "One-line description, e.g. Use when ... (default: a TODO placeholder)"
    )]
    description: Option<String>,
    #[arg(long)]
    dry_run: bool,
}
#[derive(Debug, clap::Args)]
struct UpdateOptions {
    #[arg(
        long,
        help = "Release tag to install (default: the newest published release)"
    )]
    tag: Option<String>,
    #[arg(
        long,
        value_enum,
        default_value = "alpha",
        help = "alpha includes pre-releases"
    )]
    channel: ChannelArg,
    #[arg(long, help = "Binary to replace (default: this executable)")]
    target: Option<PathBuf>,
    #[arg(long, env = "AGENT_HOME_ROOT", env = "VESICA_ROOT")]
    root: Option<PathBuf>,
    #[arg(long, env = "VESICA_REPOS_DIR")]
    repos_dir: Option<PathBuf>,
    #[arg(long)]
    dry_run: bool,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
enum ChannelArg {
    Alpha,
    Stable,
}
#[derive(Debug, clap::Args)]
struct VaultOptions {
    #[command(subcommand)]
    command: VaultCommand,
}
#[derive(Debug, clap::Args)]
struct VaultPath {
    #[arg(long, env = "VESICA_VAULT", help = "Personal vault directory")]
    path: PathBuf,
}
#[derive(Debug, Subcommand)]
enum VaultCommand {
    /// Create a new vault from the skeleton (directory must be absent or empty)
    Init {
        #[command(flatten)]
        vault: VaultPath,
        #[arg(long)]
        dry_run: bool,
    },
    /// Adopt an existing folder: add missing skeleton entries and exclude Team; never overwrites
    Fix {
        #[command(flatten)]
        vault: VaultPath,
        #[arg(long)]
        dry_run: bool,
    },
    /// Read-only vault contract check
    Check {
        #[command(flatten)]
        vault: VaultPath,
    },
    /// Create a ledger note in Sessions/agent-runs/
    NewNote {
        #[command(flatten)]
        vault: VaultPath,
        #[arg(long)]
        task: String,
        #[arg(long)]
        agent: String,
        #[arg(long, help = "YYYY-MM-DD (default: today, UTC)")]
        date: Option<String>,
        #[arg(long, default_value = "general")]
        area: String,
        #[arg(long)]
        title: Option<String>,
    },
    /// Commit the vault to its own Git repository, never including Team
    Snapshot {
        #[command(flatten)]
        vault: VaultPath,
        #[arg(long)]
        message: Option<String>,
    },
}
#[derive(Debug, clap::Args)]
struct RootOptions {
    #[arg(long, env = "AGENT_HOME_ROOT", env = "VESICA_ROOT")]
    root: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct UninstallOptions {
    #[arg(long, env = "AGENT_HOME_ROOT", env = "VESICA_ROOT")]
    root: Option<PathBuf>,
    #[arg(long)]
    dry_run: bool,
}
fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Doctor(o) => doctor(&o),
        Command::Install(o) => install(o),
        Command::Status(o) => status(o.root.as_deref()),
        Command::Uninstall(o) => uninstall(o.root.as_deref(), o.dry_run),
        Command::Lint(o) => lint(&o.home_repo),
        Command::VaultScan(o) => vault_scan(&o.path),
        Command::Vault(o) => vault(o.command),
        Command::Update(o) => update(o),
        Command::NewSkill(o) => new_skill(o),
        Command::SyncStandards(o) => sync_standards(o),
        Command::Share(o) => share(o),
        Command::Team(o) => match o.command {
            TeamCommand::Status(options) => team_status(&required_team_path(options.path)?),
            TeamCommand::Pull(options) => team_pull(&required_team_path(options.path)?),
        },
    }
}
fn required_team_path(path: Option<PathBuf>) -> Result<PathBuf> {
    path.context("missing team checkout path: pass --path or set VESICA_TEAM_HOME")
}

fn team_status(path: &Path) -> Result<()> {
    let status = ah_vault::shared_home_status(path)?;
    println!("Shared home: {}", path.display());
    println!("Branch: {}", status.branch);
    println!(
        "Upstream: {}",
        status.upstream.as_deref().unwrap_or("(none)")
    );
    println!(
        "Working tree: {}",
        if status.clean { "clean" } else { "dirty" }
    );
    println!("Ahead: {}", status.ahead);
    println!("Behind (last fetched): {}", status.behind);
    println!("Read-only status; remote refs are not fetched.");
    Ok(())
}

fn team_pull(path: &Path) -> Result<()> {
    let result = ah_vault::pull_shared_home(path)?;
    if result.old_head == result.new_head {
        println!("Shared home already up to date ({})", result.new_head);
    } else {
        println!(
            "Fast-forwarded {} from {} to {}",
            result.upstream, result.old_head, result.new_head
        );
    }
    Ok(())
}

fn lint(path: &Path) -> Result<()> {
    let findings = ah_vault::lint_shared_home(path)
        .with_context(|| format!("cannot lint shared home {}", path.display()))?;
    if findings.is_empty() {
        println!("No lint findings under {}", path.display());
        return Ok(());
    }
    for finding in &findings {
        println!(
            "{}:{}:{}: {}",
            finding.path.display(),
            finding.line,
            finding.rule,
            finding.message
        );
    }
    anyhow::bail!("{} lint finding(s)", findings.len())
}

/// Remove symlinks recorded by an earlier install that this install no longer plans (for example
/// `~/Team` after switching to `--vault`). A link is removed only if it still points exactly where
/// Vesica put it; a link the user changed is left in place and released from the manifest.
fn prune_stale_links(cfg: &Path, plan: &ah_core::Plan, dry: bool) -> Result<()> {
    let manifest_path = cfg.join("manifest.json");
    let mut manifest = ah_core::Manifest::load(&manifest_path)?;
    let planned: std::collections::BTreeSet<&str> =
        plan.actions.iter().map(ah_core::Action::key).collect();
    let stale: Vec<String> = manifest
        .entries
        .iter()
        .filter(|(key, entry)| entry.kind == "symlink" && !planned.contains(key.as_str()))
        .map(|(key, _)| key.clone())
        .collect();
    let mut changed = false;
    for key in stale {
        let entry = &manifest.entries[&key];
        let target = Path::new(&entry.target);
        let current = std::fs::read_link(target).ok().map(|link| {
            if link.is_absolute() {
                link
            } else {
                target.parent().unwrap_or(Path::new(".")).join(link)
            }
        });
        let expected = entry.source.as_deref().map(PathBuf::from);
        if current.is_some() && current == expected {
            if dry {
                println!("Would remove stale link: {key}");
                continue;
            }
            std::fs::remove_file(target)?;
            println!("Removed stale link: {key}");
        } else if std::fs::symlink_metadata(target).is_err() {
            println!("Forgot missing stale link: {key}");
        } else {
            println!("Kept modified stale link: {key} (no longer managed by Vesica)");
        }
        if !dry {
            manifest.entries.remove(&key);
            changed = true;
        }
    }
    if changed {
        manifest.save_atomic(&manifest_path)?;
    }
    Ok(())
}

fn print_vault_changes(changes: &[ah_vault::personal::VaultChange], dry: bool) -> bool {
    use ah_vault::personal::VaultChange;
    let mut any = false;
    for change in changes {
        let line = match (change, dry) {
            (VaultChange::Kept(name), _) => format!("Kept existing: {name}"),
            (VaultChange::Create(name), true) => format!("Would create: {name}"),
            (VaultChange::Create(name), false) => format!("Created: {name}"),
            (VaultChange::ExcludeTeam, true) => "Would update: .gitignore (add /Team)".into(),
            (VaultChange::ExcludeTeam, false) => "Updated: .gitignore (added /Team)".into(),
            (VaultChange::GitInit, true) => "Would initialise a Git repository".into(),
            (VaultChange::GitInit, false) => "Initialised a Git repository".into(),
        };
        any |= !matches!(change, VaultChange::Kept(_));
        println!("{line}");
    }
    any
}

/// `vesica update` (plan §5.4): resolve a release, download and verify it, replace the binary,
/// then re-link from the recorded manifest. The staged download lives beside the target so the
/// final `rename` is atomic, and it is removed whatever happens.
fn update(o: UpdateOptions) -> Result<()> {
    let tag = match &o.tag {
        Some(tag) => {
            release::validate_tag(tag)?;
            tag.clone()
        }
        None => {
            let api = std::env::var("VESICA_RELEASES_API_URL")
                .unwrap_or_else(|_| release::DEFAULT_RELEASES_API.to_owned());
            let channel = match o.channel {
                ChannelArg::Alpha => release::Channel::Alpha,
                ChannelArg::Stable => release::Channel::Stable,
            };
            let tag = release::resolve_tag(&api, channel)?;
            println!("Newest {} release: {tag}", channel.text());
            tag
        }
    };
    let current = env!("CARGO_PKG_VERSION");
    if tag.trim_start_matches('v') == current {
        println!("Already up to date: vesica {current} ({tag})");
        return Ok(());
    }
    let target = match &o.target {
        Some(target) => target.clone(),
        None => std::env::current_exe().context("cannot determine the running executable")?,
    };
    let parent = target
        .parent()
        .context("target has no parent directory")?
        .to_path_buf();
    let base_url = std::env::var("VESICA_RELEASE_BASE_URL")
        .unwrap_or_else(|_| release::DEFAULT_BASE_URL.to_owned());
    let staging = parent.join(format!(
        ".vesica-update-{}",
        utc_stamp(std::time::SystemTime::now())
    ));
    std::fs::create_dir_all(&staging).with_context(|| format!("creating {}", staging.display()))?;
    let outcome = (|| -> Result<()> {
        let tarball = release::download(&staging, &base_url, &tag)?;
        release::verify_checksum(&staging, &tarball)?;
        let binary = release::extract(&staging, &tarball, &tag)?;
        release::check_reports_version(&binary, &tag)?;
        if o.dry_run {
            println!(
                "Would replace {} (vesica {})",
                target.display(),
                tag.trim_start_matches('v')
            );
            return Ok(());
        }
        release::replace(&target, &binary)?;
        println!(
            "Replaced {} (vesica {})",
            target.display(),
            tag.trim_start_matches('v')
        );
        Ok(())
    })();
    // Always remove the staging directory, on success and on every failure path.
    let cleanup = std::fs::remove_dir_all(&staging);
    outcome?;
    cleanup.with_context(|| format!("removing {}", staging.display()))?;
    if o.dry_run {
        return Ok(());
    }
    relink_from_manifest(&o)
}

/// Re-run the install plan from what the manifest recorded, so links follow the new binary.
fn relink_from_manifest(o: &UpdateOptions) -> Result<()> {
    let cfg = root(o.root.as_deref())?.join(".config/agent-home");
    let manifest_path = cfg.join("manifest.json");
    if !manifest_path.is_file() {
        println!(
            "No manifest recorded at {}; skipping re-link",
            manifest_path.display()
        );
        return Ok(());
    }
    let manifest = ah_core::Manifest::load(&manifest_path)?;
    if manifest.shared_home.is_empty() {
        println!("Manifest records no shared home; skipping re-link");
        return Ok(());
    }
    let profile = match manifest.profile.as_str() {
        "core" => Some(ProfileArg::Core),
        "lead" => Some(ProfileArg::Lead),
        _ => Some(ProfileArg::Developer),
    };
    println!(
        "Re-linking from the recorded manifest ({})",
        manifest.shared_home
    );
    install(InstallOptions {
        options: Options {
            home_repo: Some(PathBuf::from(&manifest.shared_home)),
            profile,
        },
        root: o.root.clone(),
        dry_run: false,
        repos_dir: o.repos_dir.clone(),
        vault: manifest.vault.as_ref().map(PathBuf::from),
    })
}

/// `vesica new-skill` (plan §5.4): scaffold a skill on a branch. A person edits it and opens the
/// PR; nothing is pushed, and the current branch is never written to.
fn new_skill(o: NewSkillOptions) -> Result<()> {
    let shared = o
        .home_repo
        .canonicalize()
        .with_context(|| format!("shared-home path does not exist: {}", o.home_repo.display()))?;
    let scaffold = ah_vault::new_skill(&shared, &o.name, o.description.as_deref(), o.dry_run)?;
    println!(
        "{}: {}",
        if o.dry_run { "Would create" } else { "Created" },
        scaffold.path
    );
    match (&scaffold.branch, scaffold.committed) {
        (Some(branch), true) => {
            println!("Branch: {branch} (committed locally; nothing was pushed)");
            println!(
                "Next: edit {} in {}, then run `vesica lint --home-repo {}` and `vesica vault-scan --path {}`, and open a PR for review.",
                scaffold.path,
                shared.display(),
                shared.display(),
                shared.join(&scaffold.path).display()
            );
        }
        (Some(branch), false) => println!("Would create branch: {branch}"),
        (None, _) => println!(
            "{} is not a Git repository: the file was written without a branch or commit",
            shared.display()
        ),
    }
    if o.description.is_none() {
        println!(
            "Note: the scaffold carries a TODO description; replace it before opening the PR."
        );
    }
    Ok(())
}

/// `vesica sync-standards --dry-run` (plan §5.4, decision D-I). The opt-in is
/// `standards = true` on a `[[repos]]` entry. Only the plan is produced: opening the PRs needs
/// separate approval, so anything but `--dry-run` refuses.
fn sync_standards(o: SyncStandardsOptions) -> Result<()> {
    let resolved = resolve_home_repo(&o.options, None)?;
    let (c, _) = load(&resolved.home_repo, &resolved.local)?;
    let shared = resolved.home_repo.canonicalize().with_context(|| {
        format!(
            "shared-home path does not exist: {}",
            resolved.home_repo.display()
        )
    })?;
    let repos_dir = o.repos_dir.as_ref().context(
        "sync-standards needs --repos-dir <dir>: the directory holding the code-repo checkouts",
    )?;
    if !o.dry_run {
        anyhow::bail!(
            "opening a PR per repository needs separate approval; run `vesica sync-standards --dry-run`"
        );
    }
    if !repos_dir.is_dir() {
        anyhow::bail!("repos directory does not exist: {}", repos_dir.display());
    }
    let standards = ah_vault::standards(&shared)?;
    if standards.is_empty() {
        println!("No Standards/*.md files in {}", shared.display());
        return Ok(());
    }
    let mut planned = 0_usize;
    let mut opted_in = 0_usize;
    let mut unchanged = 0_usize;
    for repo in &c.repos {
        if !repo.standards {
            println!(
                "{}: not opted in (set `standards = true` in agent-home.toml)",
                repo.name
            );
            continue;
        }
        let checkout = repos_dir.join(&repo.name);
        if !checkout.is_dir() {
            println!("{}: not cloned at {}", repo.name, checkout.display());
            continue;
        }
        opted_in += 1;
        println!("{}: plan for {}", repo.name, checkout.display());
        for standard in &standards {
            let destination = checkout
                .join(".github/instructions")
                .join(format!("{}.instructions.md", standard.name));
            let relative = destination
                .strip_prefix(&checkout)
                .unwrap_or(&destination)
                .display()
                .to_string();
            let state = match std::fs::read(&destination) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    planned += 1;
                    "Would create"
                }
                Ok(existing) if existing == standard.bytes => {
                    unchanged += 1;
                    "unchanged"
                }
                Ok(_) => {
                    planned += 1;
                    "Would update"
                }
                Err(error) => return Err(error.into()),
            };
            println!("  {state}: {relative} -> {}", destination.display());
        }
    }
    if opted_in == 0 {
        println!("No repository is opted in (`standards = true` in agent-home.toml).");
        return Ok(());
    }
    println!(
        "Dry run: {planned} file(s) would change, {unchanged} already match; nothing was written."
    );
    Ok(())
}

/// The person's Git author name, used as the promoted note's `owner`.
fn git_author_name(shared: &Path) -> Result<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(shared)
        .args(["var", "GIT_AUTHOR_IDENT"])
        .output()
        .context("starting git to read your Git identity")?;
    if !out.status.success() {
        anyhow::bail!(
            "cannot read your Git identity; set `git config --global user.name` or pass --owner"
        );
    }
    let ident = String::from_utf8_lossy(&out.stdout);
    ident
        .split(" <")
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .context("your Git identity has no name; pass --owner")
}

/// `vesica share` (plan §5.4, D26, S9): promote a personal note into the shared home.
///
/// Nothing is written without `--yes`: that is the person's confirmation step, and cancelling
/// before it leaves no trace. The whole promotion happens on a `share/<note>` branch, the local
/// checks (lint and vault-scan) gate the commit, and a failed gate rolls the branch back.
fn share(o: ShareOptions) -> Result<()> {
    use ah_vault::share::{self, ShareRequest};
    let vault = root(o.root.as_deref())?;
    let resolved = resolve_home_repo(&o.options, o.root.as_deref())?;
    let shared = resolved.home_repo.canonicalize().with_context(|| {
        format!(
            "shared-home path does not exist: {}",
            resolved.home_repo.display()
        )
    })?;
    let destination_dir = match (&o.area, o.decisions) {
        (Some(area), false) => {
            let safe = !area.is_empty()
                && !area.starts_with(['/', '.'])
                && area
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ' ' | '/'));
            if !safe {
                anyhow::bail!("--area must be a name under Knowledge/ (letters, digits, '-', '_', '/' and spaces)");
            }
            format!("Knowledge/{area}")
        }
        (None, true) => "Decisions".to_owned(),
        _ => anyhow::bail!("exactly one of --area <name> or --decisions is required"),
    };
    let mut request = ShareRequest {
        vault: &vault,
        shared: &shared,
        note: &o.note,
        destination_dir: &destination_dir,
        owner: "",
        drop_unresolved_links: o.drop_unresolved_links,
    };
    // Validate the note, the destination and the checkout before asking Git for an identity, so a
    // refusal never depends on local Git configuration.
    let plan = share::plan(&request)?;
    let owner = match &o.owner {
        Some(owner) => owner.clone(),
        None => git_author_name(&shared)?,
    };
    request.owner = &owner;
    let confirm = o.yes && !o.dry_run;
    println!(
        "{}: {} -> {}",
        if confirm { "Promoting" } else { "Would create" },
        plan.source_rel,
        plan.destination_rel
    );
    println!("Branch: {} (new; trunk is never written)", plan.branch);
    for field in &plan.additions {
        let value = match *field {
            "owner" => owner.as_str(),
            _ => "working",
        };
        println!("  frontmatter: {field} = {value}");
    }
    for (from, to) in &plan.link_rewrites {
        println!("  link rewrite: [[{from}]] -> [[{to}]] (Team/ prefix is the vault view)");
    }
    for link in &plan.dropped_links {
        println!("  would drop unresolvable link [[{link}]] and keep the text");
    }
    for rewrite in &plan.rewrites {
        println!(
            "  {}: [[{}]] -> [[{}]]",
            rewrite.file, rewrite.from, rewrite.to
        );
    }
    if !confirm {
        println!(
            "Nothing was written. Re-run with --yes to apply{}.",
            if o.dry_run {
                ""
            } else {
                ", or --dry-run for a preview only"
            }
        );
        return Ok(());
    }
    let message = format!(
        "share/{}: promote the personal note into {destination_dir}",
        Path::new(&plan.source_rel)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("note")
    );
    let applied = share::apply(&request, &plan, &message, o.move_source)?;
    println!("Committed: {} ({})", applied.commit, plan.branch);
    if applied.rewrites_applied > 0 {
        println!(
            "Rewrote {} personal note(s) to link into the shared home",
            applied.rewrites_applied
        );
    }
    if applied.moved {
        println!(
            "Moved the personal note out of the vault: {}",
            plan.source_rel
        );
    }
    println!(
        "The branch was not pushed. Next: git -C {} push -u origin {}, then `gh pr create --repo <owner>/<shared-home> --head {}` for review.",
        shared.display(),
        plan.branch,
        plan.branch
    );
    if o.open_pr {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&shared)
            .args(["push", "-u", "origin", &plan.branch])
            .status()
            .context("starting git push")?;
        if !status.success() {
            anyhow::bail!("git push failed; the commit is local on {}", plan.branch);
        }
        println!("Pushed {} (open the PR with `gh pr create`)", plan.branch);
    }
    Ok(())
}

fn utc_date() -> String {
    let stamp = utc_stamp(std::time::SystemTime::now());
    format!("{}-{}-{}", &stamp[0..4], &stamp[4..6], &stamp[6..8])
}

fn vault(command: VaultCommand) -> Result<()> {
    use ah_vault::personal;
    let print_changes = print_vault_changes;
    match command {
        VaultCommand::Init { vault, dry_run } => {
            let changes = personal::init(&vault.path, &utc_date(), dry_run)?;
            print_changes(&changes, dry_run);
            if !dry_run {
                println!("Vault created: {}", vault.path.display());
            }
        }
        VaultCommand::Fix { vault, dry_run } => {
            let changes = personal::fix(&vault.path, &utc_date(), dry_run)?;
            if !print_changes(&changes, dry_run) {
                println!("Nothing to fix: {}", vault.path.display());
            }
        }
        VaultCommand::Check { vault } => {
            let findings = personal::check(&vault.path)?;
            if findings.is_empty() {
                println!("vault check: ok ({})", vault.path.display());
                return Ok(());
            }
            for f in &findings {
                match f.line {
                    Some(line) => println!("{} {}:{line}: {}", f.rule, f.path, f.message),
                    None => println!("{} {}: {}", f.rule, f.path, f.message),
                }
            }
            anyhow::bail!("{} vault finding(s)", findings.len());
        }
        VaultCommand::NewNote {
            vault,
            task,
            agent,
            date,
            area,
            title,
        } => {
            let date = date.unwrap_or_else(utc_date);
            let rel = personal::new_note(
                &vault.path,
                &personal::LedgerNote {
                    task: &task,
                    agent: &agent,
                    date: &date,
                    area: &area,
                    title: title.as_deref(),
                },
            )?;
            println!("Created ledger note: {rel}");
        }
        VaultCommand::Snapshot { vault, message } => {
            let message = message.unwrap_or_else(|| {
                format!("vault snapshot {}", utc_stamp(std::time::SystemTime::now()))
            });
            match personal::snapshot(&vault.path, &message)? {
                personal::SnapshotOutcome::Committed { commit, files } => {
                    println!("Snapshot {commit}: {files} file(s) committed (Team excluded)")
                }
                personal::SnapshotOutcome::NothingToSnapshot => {
                    println!("Nothing to snapshot: the vault matches its last snapshot")
                }
            }
        }
    }
    Ok(())
}

fn vault_scan(path: &Path) -> Result<()> {
    let findings = ah_vault::scan_secrets(path)?;
    if findings.is_empty() {
        println!(
            "No common credential patterns found under {}",
            path.display()
        );
        return Ok(());
    }
    for finding in &findings {
        println!(
            "{}:{}: potential secret pattern",
            finding.path.display(),
            finding.line
        );
    }
    anyhow::bail!(
        "{} potential secret finding(s); matched values were not printed",
        findings.len()
    )
}

/// The agent-home configuration directory for a root (`<root>/.config/agent-home`).
fn config_dir(root_opt: Option<&Path>) -> Result<PathBuf> {
    Ok(root(root_opt)?.join(".config/agent-home"))
}

/// A resolved shared home plus the layer that supplied it, and this machine's local config.
struct ResolvedHome {
    home_repo: PathBuf,
    source: ah_core::Source,
    local: ah_core::LocalConfig,
    config_dir: PathBuf,
}

/// Resolves the shared home (plan F8). clap already folds the environment variables into the flag,
/// so "flag or env" is one layer; the local file is consulted only when neither is present. The
/// team file cannot supply this value: the home has to be known before it can be read.
fn resolve_home_repo(o: &Options, root_opt: Option<&Path>) -> Result<ResolvedHome> {
    let config_dir = config_dir(root_opt)?;
    let local = ah_core::LocalConfig::load(&config_dir)?;
    let (home_repo, source) =
        ah_core::local::resolve(o.home_repo.clone(), local.home_repo.clone(), None, None);
    let Some(home_repo) = home_repo else {
        anyhow::bail!(
            "no shared home configured: pass --home-repo <path>, set VESICA_HOME_REPO, or add `home_repo` to {}",
            ah_core::LocalConfig::path_in(&config_dir).display()
        );
    };
    Ok(ResolvedHome {
        home_repo,
        source,
        local,
        config_dir,
    })
}

fn profile_value(a: ProfileArg) -> ah_core::Profile {
    match a {
        ProfileArg::Core => ah_core::Profile::Core,
        ProfileArg::Developer => ah_core::Profile::Developer,
        ProfileArg::Lead => ah_core::Profile::Lead,
    }
}

fn profile_text(p: ah_core::Profile) -> &'static str {
    match p {
        ah_core::Profile::Core => "core",
        ah_core::Profile::Developer => "developer",
        ah_core::Profile::Lead => "lead",
    }
}

fn load(
    shared: &Path,
    local: &ah_core::LocalConfig,
) -> Result<(ah_core::AgentHomeConfig, ah_core::FoundryCatalog)> {
    let p = shared.join("agent-home.toml");
    let mut c = ah_core::AgentHomeConfig::from_path(&p)
        .with_context(|| format!("cannot load shared-home configuration {}", p.display()))?;
    // Machine-local repositories add to the team list and may override an entry by name.
    c.repos = ah_core::local::merge_repos(&c.repos, &local.repos);
    let p = shared.join("foundry.catalog.json");
    let f = if p.exists() {
        ah_core::FoundryCatalog::from_path(&p)?
    } else {
        ah_core::FoundryCatalog::default()
    };
    Ok((c, f))
}
/// The selected profile, with the layer that supplied it (flag/env, config.toml, team, default).
fn prof(
    o: &Options,
    local: &ah_core::LocalConfig,
    c: &ah_core::AgentHomeConfig,
) -> (&'static str, ah_core::Source) {
    let (value, source) = ah_core::local::resolve(
        o.profile.map(ProfileArg::text),
        local.profile.map(profile_text),
        Some(profile_text(c.profiles.default)),
        Some("developer"),
    );
    (value.unwrap_or("developer"), source)
}
fn doctor(o: &DoctorOptions) -> Result<()> {
    let resolved = resolve_home_repo(&o.options, None)?;
    let (c, f) = load(&resolved.home_repo, &resolved.local)?;
    let wsl = ah_wsl::is_wsl()?;
    let wsl_status = if wsl { "detected" } else { "not detected" };
    let (selected, profile_source) = prof(&o.options, &resolved.local, &c);
    let selected_profile = ah_agents::ProfileSelection::from_name(&c, selected)?;
    let mut checks = preflight::Preflight::run(c.minimum.node.as_deref());
    // Informational, never a blocker: install works without a Git identity, but the personal-vault
    // commit paths (`vault snapshot`, `new-skill`, `share`) need one.
    checks.optional.push((
        "git identity",
        match git_author_name(&resolved.home_repo) {
            Ok(name) => format!("found: {name}"),
            Err(_) => "missing (needed by vault snapshot, new-skill and share)".to_owned(),
        },
    ));
    let status = if checks.ok() { "ok" } else { "blocked" };
    if o.json {
        let required: serde_json::Map<_, _> = checks
            .required
            .iter()
            .map(|(name, state)| ((*name).to_owned(), serde_json::json!(state)))
            .collect();
        let optional: serde_json::Map<_, _> = checks
            .optional
            .iter()
            .map(|(name, state)| ((*name).to_owned(), serde_json::json!(state)))
            .collect();
        let report = serde_json::json!({
            "status": status,
            "blockers": checks.blockers,
            "platform": {
                "os": checks.platform.os,
                "arch": checks.platform.arch,
                "kernel": checks.platform.kernel,
                "distro": checks.platform.distro,
            },
            "prerequisites": {"required": required, "optional": optional},
            "shared_home": "valid",
            "team": {"name": c.team.name, "org": c.team.org},
            "profile": selected,
            "repos_in_config": c.repos.len(),
            "repos": selected_profile.repositories.iter().map(|repo| repo.name.as_str()).collect::<Vec<_>>(),
            "foundry_deployments": f.deployments.len(),
            "wsl_kernel": wsl_status,
            "live_harness": "not-implemented",
            "foundry": "not-implemented",
            "windows": "not-implemented",
            "herdr": "not-implemented"
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "Shared home: {} ({})\nTeam: {} ({})\nProfile: {} ({})\nRepos in config: {}\nRepos for profile: {}\nFoundry deployments: {}",
            resolved.home_repo.display(),
            resolved.source.as_str(),
            c.team.name,
            c.team.org,
            selected,
            profile_source.as_str(),
            c.repos.len(),
            selected_profile.repositories.len(),
            f.deployments.len()
        );
        println!(
            "Platform: {} {} ({}, kernel {})",
            checks.platform.os,
            checks.platform.arch,
            checks.platform.distro,
            checks.platform.kernel
        );
        for (name, state) in &checks.required {
            println!("Required {name}: {state}");
        }
        for (name, state) in &checks.optional {
            println!("Optional {name}: {state}");
        }
        println!("WSL kernel: {wsl_status} (informational; native Linux is supported)");
        println!("Live harness, Foundry, Windows and Herdr checks are not implemented.");
        for blocker in &checks.blockers {
            println!("Blocker: {blocker}");
        }
        println!("Doctor status: {status}");
    }
    if !checks.ok() {
        anyhow::bail!(
            "doctor found {} blocker(s): {}",
            checks.blockers.len(),
            checks.blockers.join("; ")
        );
    }
    Ok(())
}
fn install(o: InstallOptions) -> Result<()> {
    let resolved = resolve_home_repo(&o.options, o.root.as_deref())?;
    let (c, _) = load(&resolved.home_repo, &resolved.local)?;
    let (selected, _profile_source) = prof(&o.options, &resolved.local, &c);
    let home = match o.root {
        Some(p) => p,
        None => std::env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is not set")?,
    };
    let cfg = resolved.config_dir.clone();
    let shared = resolved.home_repo.canonicalize().with_context(|| {
        format!(
            "shared-home path does not exist: {}",
            resolved.home_repo.display()
        )
    })?;
    if !shared.join("AGENTS.md").is_file() {
        anyhow::bail!("shared home must contain AGENTS.md: {}", shared.display());
    }
    // Install step 3 (D-H): with --vault the shared home is linked at <vault>/Team instead of
    // ~/Team, unless the shared-home checkout already lives there (plan step 4 layout).
    let vault = match &o.vault {
        Some(dir) => Some(
            std::path::absolute(dir)
                .with_context(|| format!("cannot resolve --vault {}", dir.display()))?,
        ),
        None => None,
    };
    let team_link = vault
        .as_ref()
        .map_or_else(|| home.join("Team"), |v| v.join("Team"));
    let team_in_place = vault.is_some()
        && !team_link.is_symlink()
        && team_link.canonicalize().ok().as_ref() == Some(&shared);
    let mut import = format!("@{}", shared.join("AGENTS.md").display());
    if let Some(vault) = &vault {
        import.push_str(&format!("\n@{}", vault.join("AGENTS.md").display()));
    }
    let profile = ah_agents::ProfileSelection::from_name(&c, selected)?;
    // Install step 2: config.env exports the team variables; ~/.bashrc sources it. Both are
    // managed blocks, so backups, modified-file refusal and exact uninstall apply unchanged.
    // The vault variable is added with install step 3 (vault primitives).
    let config_env = cfg.join("config.env");
    let prefix = &c.team.env_prefix;
    let mut exports = vec![
        shell_export(prefix, "HOME", path_text(&shared)?)?,
        shell_export(prefix, "PROFILE", selected)?,
    ];
    if let Some(vault) = &vault {
        exports.push(shell_export(prefix, "VAULT", path_text(vault)?)?);
    }
    // The Copilot CLI variable name is fixed by Copilot, and its value is a comma-separated list,
    // so it is written literally rather than through the team's env prefix.
    let mut instruction_files = vec![shared.join("AGENTS.md").to_string_lossy().into_owned()];
    if let Some(vault) = &vault {
        instruction_files.push(vault.join("AGENTS.md").to_string_lossy().into_owned());
    }
    let copilot_dirs = match &vault {
        Some(vault) => format!("{},{}", shared.display(), vault.display()),
        None => shared.display().to_string(),
    };
    exports.push(format!(
        "export COPILOT_CUSTOM_INSTRUCTIONS_DIRS={}",
        shell_quote(&copilot_dirs)?
    ));
    if let (false, Some(dir)) = (profile.profile == ah_core::Profile::Core, &o.repos_dir) {
        let dir = std::path::absolute(dir)
            .with_context(|| format!("cannot resolve --repos-dir {}", dir.display()))?;
        exports.push(shell_export(prefix, "REPOS", path_text(&dir)?)?);
    }
    let env_path = shell_quote(path_text(&config_env)?)?;
    let mut actions = Vec::new();
    if !team_in_place {
        actions.push(ah_core::Action::Symlink {
            source: shared.to_string_lossy().into_owned(),
            destination: team_link.to_string_lossy().into_owned(),
        });
    }
    // OpenCode reads `instructions` from its own JSONC config. Existing entries are kept: the
    // merge replaces array values, so the union is computed here.
    let opencode_path = home.join(".config/opencode/opencode.json");
    let mut opencode_instructions: Vec<String> = std::fs::read_to_string(&opencode_path)
        .ok()
        .and_then(|text| {
            jsonc_parser::parse_to_serde_value::<serde_json::Value>(
                &text,
                &jsonc_parser::ParseOptions::default(),
            )
            .ok()
        })
        .and_then(|value| value.get("instructions").cloned())
        .and_then(|value| value.as_array().cloned())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    for file in &instruction_files {
        if !opencode_instructions.contains(file) {
            opencode_instructions.push(file.clone());
        }
    }
    actions.push(ah_core::Action::MergeJsonc {
        path: opencode_path.to_string_lossy().into_owned(),
        patch: serde_json::json!({ "instructions": opencode_instructions }),
    });
    actions.extend([
        ah_core::Action::UpsertBlock {
            path: home
                .join(".claude/CLAUDE.md")
                .to_string_lossy()
                .into_owned(),
            style: "hash".into(),
            name: "vesica".into(),
            body: import.clone(),
        },
        ah_core::Action::UpsertBlock {
            path: home.join(".codex/AGENTS.md").to_string_lossy().into_owned(),
            style: "hash".into(),
            name: "vesica".into(),
            body: import,
        },
        ah_core::Action::UpsertBlock {
            path: config_env.to_string_lossy().into_owned(),
            style: "hash".into(),
            name: "vesica".into(),
            body: exports.join("\n"),
        },
        ah_core::Action::UpsertBlock {
            path: home.join(".bashrc").to_string_lossy().into_owned(),
            style: "hash".into(),
            name: "vesica".into(),
            body: format!("if [ -f {env_path} ]; then . {env_path}; fi"),
        },
    ]);
    let mut planner = ah_core::Planner::new();
    for a in actions {
        planner.push(a)?
    }
    for skill in ah_agents::discover_skills(&shared.join("Agents/Skills"))? {
        for destination in [
            home.join(".agents/skills").join(&skill.name),
            home.join(".claude/skills").join(&skill.name),
        ] {
            planner.push(ah_core::Action::Symlink {
                source: skill.source.to_string_lossy().into_owned(),
                destination: destination.to_string_lossy().into_owned(),
            })?;
        }
    }
    let selected_repos = profile
        .repositories
        .iter()
        .map(|repo| repo.name.as_str())
        .collect::<Vec<_>>();
    let plan = planner.finish(o.dry_run);
    println!(
        "{} install plan (profile: {selected}; eligible repositories: {})",
        if o.dry_run {
            "DRY RUN"
        } else {
            "Applying supported actions"
        },
        if selected_repos.is_empty() {
            "none".to_owned()
        } else {
            selected_repos.join(", ")
        }
    );
    // Install step 10: plan repository clones read-only, before any mutation, so an unusable
    // repos directory fails closed.
    let repo_plan = match (&profile.profile, &o.repos_dir) {
        (ah_core::Profile::Core, _) => Err(
            "Repository cloning skipped: the core profile does not clone repositories".to_owned(),
        ),
        (_, None) => Err(format!(
            "Repository cloning skipped: pass --repos-dir <dir> to clone {} eligible repositories",
            profile.repositories.len()
        )),
        (_, Some(dir)) => Ok((dir.clone(), repos::plan(&profile.repositories, dir)?)),
    };
    // Refuse unsafe state paths before creating backups or executing any action.
    if !o.dry_run {
        let metadata = match std::fs::symlink_metadata(&cfg) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if metadata.is_some_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_dir())
        {
            anyhow::bail!(
                "Vesica state path must be a real directory: {}",
                cfg.display()
            );
        }
        let manifest_path = cfg.join("manifest.json");
        let manifest_metadata = match std::fs::symlink_metadata(&manifest_path) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if manifest_metadata
            .is_some_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
        {
            anyhow::bail!(
                "Vesica manifest path must be a regular file: {}",
                manifest_path.display()
            );
        }
        let existing = ah_core::Manifest::load(&manifest_path)?;
        for action in &plan.actions {
            let target = Path::new(action.key());
            if let Some(record) = existing.entries.get(action.key()) {
                match record.kind.as_str() {
                    "symlink" => {
                        let current = std::fs::read_link(target).with_context(|| {
                            format!("recorded symlink missing: {}", target.display())
                        })?;
                        let current = if current.is_absolute() {
                            current
                        } else {
                            target.parent().unwrap_or(Path::new(".")).join(current)
                        };
                        let expected = record
                            .source
                            .as_deref()
                            .map(PathBuf::from)
                            .context("manifest symlink is missing its source")?;
                        if current != expected {
                            anyhow::bail!("recorded symlink was modified: {}", target.display());
                        }
                    }
                    "managed-block" | "merged-file" => {
                        let current = std::fs::read(target).with_context(|| {
                            format!("recorded managed file missing: {}", target.display())
                        })?;
                        let current_hash = {
                            use sha2::Digest;
                            hex::encode(sha2::Sha256::digest(&current))
                        };
                        if record.installed_sha256.as_deref() != Some(current_hash.as_str()) {
                            anyhow::bail!(
                                "recorded managed file was modified: {}",
                                target.display()
                            );
                        }
                    }
                    kind => anyhow::bail!(
                        "unsupported manifest entry kind {kind} at {}",
                        target.display()
                    ),
                }
            } else if target.exists() {
                match action {
                    ah_core::Action::Symlink { .. } => {
                        anyhow::bail!("unmanaged destination already exists: {}", target.display())
                    }
                    ah_core::Action::UpsertBlock { .. } if target.is_file() => {
                        let contents = std::fs::read_to_string(target)?;
                        if contents.contains("agent-home:begin")
                            || contents.contains("agent-home:end")
                        {
                            anyhow::bail!(
                                "unmanaged target contains reserved Vesica markers: {}",
                                target.display()
                            );
                        }
                    }
                    ah_core::Action::UpsertBlock { .. } => anyhow::bail!(
                        "managed-block target is not a regular file: {}",
                        target.display()
                    ),
                    _ => {}
                }
            }
        }
    }
    // Vault scaffolding runs only after every refusal above. It is personal data: never
    // overwritten, not recorded in the manifest, and kept by uninstall (like cloned repositories).
    if let Some(vault) = &vault {
        let adopt = std::fs::read_dir(vault).is_ok_and(|mut entries| entries.next().is_some());
        let date = utc_date();
        let changes = if adopt {
            ah_vault::personal::fix(vault, &date, o.dry_run)?
        } else {
            ah_vault::personal::init(vault, &date, o.dry_run)?
        };
        println!(
            "Vault: {} ({})",
            vault.display(),
            if adopt { "adopting" } else { "creating" }
        );
        print_vault_changes(&changes, o.dry_run);
        if team_in_place {
            println!(
                "Team: the shared home is already at {} (no link needed)",
                team_link.display()
            );
        }
    }
    let mut pending_backups = Vec::new();
    let backups = if o.dry_run {
        std::collections::BTreeMap::new()
    } else {
        let old = ah_core::Manifest::load(&cfg.join("manifest.json"))?;
        let mut result = std::collections::BTreeMap::new();
        for action in &plan.actions {
            if let Some(existing) = old.entries.get(action.key()) {
                if let Some(path) = existing.backup.as_deref() {
                    let metadata = std::fs::symlink_metadata(path).with_context(|| {
                        format!("recorded backup is missing or unreadable: {path}")
                    })?;
                    if !metadata.is_file() || metadata.file_type().is_symlink() {
                        anyhow::bail!("recorded backup is not a regular file: {path}");
                    }
                    let expected = existing
                        .original_sha256
                        .as_deref()
                        .context("recorded backup is missing its original hash")?;
                    let bytes = std::fs::read(path).with_context(|| {
                        format!("recorded backup is missing or unreadable: {path}")
                    })?;
                    let actual = {
                        use sha2::Digest;
                        hex::encode(sha2::Sha256::digest(&bytes))
                    };
                    if actual != expected {
                        anyhow::bail!("recorded backup was modified: {path}");
                    }
                    result.insert(action.key().to_owned(), PathBuf::from(path));
                    continue;
                }
                // Vesica created this target (its original state was "absent"), and it is still
                // exactly what Vesica wrote: a fresh backup would only be an unrecorded orphan.
                if let Some(installed) = existing.installed_sha256.as_deref() {
                    match std::fs::read(action.key()) {
                        Ok(bytes) => {
                            use sha2::Digest;
                            if hex::encode(sha2::Sha256::digest(&bytes)) == installed {
                                continue;
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error.into()),
                    }
                }
            }
            if matches!(
                action,
                ah_core::Action::UpsertBlock { .. }
                    | ah_core::Action::MergeJsonc { .. }
                    | ah_core::Action::MergeToml { .. }
            ) {
                let target = Path::new(action.key());
                match std::fs::read(target) {
                    Ok(bytes) => {
                        let stamp = utc_stamp(std::time::SystemTime::now());
                        let name = target.file_name().unwrap_or_default().to_string_lossy();
                        let mut index = 0_u32;
                        let backup = loop {
                            let candidate = target
                                .with_file_name(format!("{name}.vesica-backup-{stamp}-{index}"));
                            match std::fs::symlink_metadata(&candidate) {
                                Ok(_) => {
                                    index = index
                                        .checked_add(1)
                                        .context("backup path suffix overflow")?
                                }
                                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                    break candidate
                                }
                                Err(error) => return Err(error.into()),
                            }
                        };
                        result.insert(action.key().to_owned(), backup.clone());
                        pending_backups.push((backup, bytes));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        result
    };
    let outcomes = ah_core::action::Executor.execute_with_commit(&plan, |outcomes| {
        if plan.dry_run {
            return Ok(());
        }
        if outcomes.contains(&ah_core::ActionOutcome::Collision) {
            anyhow::bail!("install stopped on an existing destination; no changes should be treated as fully applied");
        }
        let created_backups = create_install_backups(&pending_backups)?;
        let persist_result = (|| -> Result<()> {
            let mut m = ah_core::Manifest::load(&cfg.join("manifest.json"))?;
            let previous = m.clone();
            m.shared_home = shared.to_string_lossy().into_owned();
            m.profile = selected.into();
            m.vault = vault.as_ref().map(|v| v.to_string_lossy().into_owned());
            // F8: record the resolved machine values (home path, profile, vault, repos dir) in the
            // local config, so later commands need no flag. Machine facts only: never team content.
            let mut local = resolved.local.clone();
            local.home_repo = Some(shared.to_path_buf());
            if let Some(profile) = o.options.profile {
                local.profile = Some(profile_value(profile));
            }
            if let Some(v) = &vault {
                local.vault = Some(v.clone());
            }
            if let Some(dir) = &o.repos_dir {
                local.repos_dir = Some(
                    std::path::absolute(dir)
                        .with_context(|| format!("cannot resolve --repos-dir {}", dir.display()))?,
                );
            }
            local.write(&cfg)?;
            for (a, r) in plan.actions.iter().zip(outcomes) {
                if matches!(
                    r,
                    ah_core::ActionOutcome::Created
                        | ah_core::ActionOutcome::Updated
                        | ah_core::ActionOutcome::AlreadyCorrect
                ) {
                    let mut item = entry(a, backups.get(a.key()))?;
                    if let Some(old) = previous.entries.get(a.key()) {
                        item.backup = old.backup.clone();
                        item.original_sha256 = old.original_sha256.clone();
                    }
                    m.record(a, item);
                }
            }
            m.save_atomic(&cfg.join("manifest.json"))?;
            Ok(())
        })();
        let cleanup_errors = if persist_result.is_err() {
            remove_install_backups(&created_backups, &pending_backups)
        } else {
            Vec::new()
        };
        match (persist_result, cleanup_errors.is_empty()) {
            (Err(error), true) => Err(error),
            (Err(error), false) => anyhow::bail!(
                "manifest commit failed: {error:#}; backup cleanup also failed: {}",
                cleanup_errors.join("; ")
            ),
            (Ok(()), _) => Ok(()),
        }
    })?;
    for (a, r) in plan.actions.iter().zip(&outcomes) {
        println!("{r:?}: {}", a.key())
    }
    prune_stale_links(&cfg, &plan, o.dry_run)?;
    if o.dry_run {
        match &repo_plan {
            Err(message) => println!("{message}"),
            Ok((_, steps)) => {
                for step in steps {
                    match &step.state {
                        repos::RepoState::Clone => println!(
                            "Would clone: {} -> {}",
                            step.name,
                            step.destination.display()
                        ),
                        repos::RepoState::Present => println!("Present: {}", step.name),
                        repos::RepoState::Skipped(reason) => {
                            println!("Skipped: {} ({reason}; left untouched)", step.name)
                        }
                    }
                }
            }
        }
        return Ok(());
    }
    println!("Manifest recorded: {}", cfg.join("manifest.json").display());
    let mut clone_failures = Vec::new();
    match &repo_plan {
        Err(message) => println!("{message}"),
        Ok((dir, steps)) => {
            for step in steps {
                match &step.state {
                    repos::RepoState::Clone => match repos::clone(step, dir) {
                        Ok(()) => {
                            println!("Cloned: {} -> {}", step.name, step.destination.display())
                        }
                        Err(error) => {
                            clone_failures.push(format!("failed to clone {}: {error:#}", step.name))
                        }
                    },
                    repos::RepoState::Present => println!("Present: {}", step.name),
                    repos::RepoState::Skipped(reason) => {
                        println!("Skipped: {} ({reason}; left untouched)", step.name)
                    }
                }
            }
        }
    }
    println!(
        "Shell environment: {} (sourced from ~/.bashrc; open a new shell to load it)",
        config_env.display()
    );
    println!(
        "Partial wiring: Claude Code, Codex, OpenCode and Copilot CLI. Role agents and Hermes runtime configuration are not wired yet."
    );
    // Hermes edits its own configuration, so this is a plan the person runs (plan §5.4 step 12).
    if profile.profile == ah_core::Profile::Lead {
        let hermes_profile = c.hermes.profile.clone().unwrap_or_else(|| {
            c.team
                .name
                .to_lowercase()
                .chars()
                .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
                .collect::<String>()
                .trim_matches('-')
                .to_owned()
        });
        let skills = shared.join("Agents/Skills").display().to_string();
        let hermes_skills = shared.join("Agents/Hermes/Skills").display().to_string();
        println!("Hermes plan (lead profile; run these yourself, nothing was written):");
        println!(
            "  hermes --profile {hermes_profile} config set skills.external_dirs '[\"{skills}\",\"{hermes_skills}\"]'"
        );
        println!(
            "  hermes --profile {hermes_profile} config set skills.create_dir '{hermes_skills}'"
        );
        match &vault {
            Some(vault) => println!(
                "  Run Hermes from the vault so its AGENTS.md loads: cd {} && hermes --profile {hermes_profile}",
                vault.display()
            ),
            None => println!(
                "  Run Hermes with the vault as the working directory once one is configured (--vault), so its AGENTS.md loads."
            ),
        }
    }
    if !clone_failures.is_empty() {
        anyhow::bail!(
            "{}; managed configuration was installed, rerun install to retry",
            clone_failures.join("; ")
        );
    }
    Ok(())
}
/// `export <PREFIX>_<NAME>='<value>'`, single-quoted for POSIX shells.
fn shell_export(prefix: &str, name: &str, value: &str) -> Result<String> {
    Ok(format!("export {prefix}_{name}={}", shell_quote(value)?))
}

fn shell_quote(value: &str) -> Result<String> {
    // A line break would split the managed block, and NUL cannot appear in a shell word.
    if value.contains(['\n', '\r', '\0']) {
        anyhow::bail!("{value:?} contains a line break or NUL and cannot be written to config.env");
    }
    Ok(format!("'{}'", value.replace('\'', r"'\''")))
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str().with_context(|| {
        format!(
            "{} is not valid UTF-8 and cannot be written to config.env",
            path.display()
        )
    })
}

/// UTC timestamp for backup file names, e.g. `20261118T145742.064527354Z`.
fn utc_stamp(time: std::time::SystemTime) -> String {
    let since_epoch = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = since_epoch.as_secs();
    let (year, month, day) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}.{:09}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        since_epoch.subsec_nanos()
    )
}

/// Proleptic Gregorian date for a count of days since 1970-01-01 (H. Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
fn create_install_backups(pending: &[(PathBuf, Vec<u8>)]) -> Result<Vec<PathBuf>> {
    use std::io::Write;

    let mut created = Vec::new();
    for (path, contents) in pending {
        let write_result = (|| -> std::io::Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            created.push(path.clone());
            file.write_all(contents)
        })();
        if let Err(error) = write_result {
            remove_install_backups(&created, pending);
            return Err(error).with_context(|| format!("writing backup {}", path.display()));
        }
    }
    Ok(created)
}

fn remove_install_backups(created: &[PathBuf], pending: &[(PathBuf, Vec<u8>)]) -> Vec<String> {
    let mut errors = Vec::new();
    for path in created.iter().rev() {
        let Some((_, expected)) = pending.iter().find(|(candidate, _)| candidate == path) else {
            continue;
        };
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                errors.push(format!(
                    "{}: cannot inspect backup: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            errors.push(format!(
                "{}: backup was replaced; preserved",
                path.display()
            ));
            continue;
        }
        match std::fs::read(path) {
            Ok(actual) if actual == *expected => {
                if let Err(error) = std::fs::remove_file(path) {
                    errors.push(format!("{}: cannot remove backup: {error}", path.display()));
                }
            }
            Ok(_) => errors.push(format!("{}: backup changed; preserved", path.display())),
            Err(error) => errors.push(format!("{}: cannot verify backup: {error}", path.display())),
        }
    }
    errors
}

fn entry(a: &ah_core::Action, backup_path: Option<&PathBuf>) -> Result<ah_core::ManifestEntry> {
    use sha2::{Digest, Sha256};
    let p = Path::new(a.key());
    let original = if backup_path.is_some() {
        backup_path.and_then(|b| std::fs::read(b).ok())
    } else {
        None
    };
    let (kind, source) = match a {
        ah_core::Action::Symlink { source, .. } => ("symlink", Some(source.clone())),
        ah_core::Action::UpsertBlock { .. } => ("managed-block", None),
        // A merged JSON/TOML file is a file Vesica edits, not one it owns: it is backed up first
        // and restored byte-for-byte on uninstall.
        ah_core::Action::MergeJsonc { .. } | ah_core::Action::MergeToml { .. } => {
            ("merged-file", None)
        }
        _ => ("file", None),
    };
    let bytes = std::fs::read(p).unwrap_or_default();
    let installed_sha256 = if bytes.is_empty() {
        None
    } else {
        Some(hex::encode(Sha256::digest(bytes)))
    };
    let backup = backup_path.map(|path| path.to_string_lossy().into_owned());
    let original_sha256 = original.map(|bytes| hex::encode(Sha256::digest(bytes)));
    Ok(ah_core::ManifestEntry {
        kind: kind.into(),
        target: a.key().into(),
        source,
        backup,
        original_sha256,
        installed_sha256,
    })
}
fn root(r: Option<&Path>) -> Result<PathBuf> {
    r.map(Path::to_path_buf)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .context("HOME is not set")
}
fn status(r: Option<&Path>) -> Result<()> {
    let p = root(r)?.join(".config/agent-home/manifest.json");
    let m = ah_core::Manifest::load(&p)?;
    println!(
        "Manifest: {}
Shared home: {}
Profile: {}
Recorded entries: {}",
        p.display(),
        if m.shared_home.is_empty() {
            "(not installed)"
        } else {
            &m.shared_home
        },
        m.profile,
        m.entries.len()
    );
    Ok(())
}
fn uninstall(r: Option<&Path>, dry: bool) -> Result<()> {
    use sha2::Digest;
    let p = root(r)?.join(".config/agent-home/manifest.json");
    if let Ok(metadata) = std::fs::symlink_metadata(&p) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            anyhow::bail!(
                "Vesica manifest path must be a regular file: {}",
                p.display()
            );
        }
    }
    let mut m = ah_core::Manifest::load(&p)?;
    // Do not let atomic manifest persistence fail on a known collision after
    // uninstall has already removed managed links or restored user files.
    if !dry {
        let temporary = p.with_extension("json.tmp");
        match std::fs::symlink_metadata(&temporary) {
            Ok(_) => anyhow::bail!(
                "Vesica manifest temp file already exists: {}",
                temporary.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    // Validate every recorded backup before removing any managed entry so an
    // invalid backup cannot cause a partially completed uninstall.
    for (key, entry) in &m.entries {
        if entry.kind != "managed-block" {
            continue;
        }
        let Some(backup) = entry.backup.as_deref() else {
            continue;
        };
        if entry.original_sha256.is_none() {
            // Legacy manifests can lack hashes; let the legacy-safe per-entry
            // path retain them without preventing unrelated entries cleaning up.
            continue;
        }
        let metadata = std::fs::symlink_metadata(backup).with_context(|| {
            format!("recorded backup is missing or unreadable for {key}: {backup}")
        })?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            anyhow::bail!("recorded backup is not a regular file: {backup}");
        }
        if let Some(expected_hash) = entry.original_sha256.as_deref() {
            let bytes = std::fs::read(backup)
                .with_context(|| format!("reading recorded backup for {key}: {backup}"))?;
            if hex::encode(sha2::Sha256::digest(&bytes)) != expected_hash {
                anyhow::bail!("recorded backup was modified: {backup}");
            }
        }
    }
    println!(
        "{} uninstall inspection",
        if dry {
            "DRY RUN"
        } else {
            "Applying verified removals"
        }
    );
    if m.entries.is_empty() {
        println!("Nothing recorded; no changes made.");
        return Ok(());
    }
    let mut retained = std::collections::BTreeMap::new();
    for (key, entry) in std::mem::take(&mut m.entries) {
        let target = Path::new(&entry.target);
        match entry.kind.as_str() {
            "symlink" => {
                let link = std::fs::read_link(target).ok().map(|p| {
                    if p.is_absolute() {
                        p
                    } else {
                        target.parent().unwrap_or(Path::new(".")).join(p)
                    }
                });
                let expected = entry.source.as_deref().map(Path::new).map(|p| {
                    if p.is_absolute() {
                        p.to_path_buf()
                    } else {
                        std::env::current_dir().unwrap_or_default().join(p)
                    }
                });
                if link != expected {
                    println!("Skipped modified, missing or unverifiable symlink: {key}");
                    retained.insert(key, entry);
                } else {
                    println!(
                        "{}: {key}",
                        if dry {
                            "Would remove symlink"
                        } else {
                            "Removing symlink"
                        }
                    );
                    if !dry {
                        std::fs::remove_file(target)?;
                    }
                }
            }
            "merged-file" => {
                // A merge can't be undone by stripping a block, so only an exact backup restore
                // (or removing a file Vesica created) is safe.
                let current = std::fs::read(target)?;
                let installed_matches = entry
                    .installed_sha256
                    .as_deref()
                    .map(|hash| hex::encode(sha2::Sha256::digest(&current)) == hash)
                    .unwrap_or(false);
                if !installed_matches {
                    println!("Skipped modified merged file: {key}");
                    retained.insert(key, entry);
                    continue;
                }
                match entry.backup.as_deref().map(Path::new) {
                    None => {
                        println!(
                            "{}: {key}",
                            if dry {
                                "Would remove created merged file"
                            } else {
                                "Removing created merged file"
                            }
                        );
                        if !dry {
                            std::fs::remove_file(target)?;
                        }
                    }
                    Some(backup_path) => {
                        let Some(expected) = entry.original_sha256.as_deref() else {
                            println!("Skipped merged file without original hash: {key}");
                            retained.insert(key, entry);
                            continue;
                        };
                        let backup = match std::fs::read(backup_path) {
                            Ok(bytes) => bytes,
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                println!("Skipped missing backup: {key}");
                                retained.insert(key, entry);
                                continue;
                            }
                            Err(error) => return Err(error.into()),
                        };
                        if hex::encode(sha2::Sha256::digest(&backup)) != expected {
                            println!("Skipped altered backup: {key}");
                            retained.insert(key, entry);
                            continue;
                        }
                        println!(
                            "{}: {key}",
                            if dry {
                                "Would restore original merged file"
                            } else {
                                "Restoring original merged file"
                            }
                        );
                        if !dry {
                            std::fs::write(target, backup)?;
                        }
                    }
                }
            }
            "managed-block" => {
                let backup_path = entry.backup.as_deref().map(Path::new);
                if backup_path.is_none() {
                    let current = std::fs::read_to_string(target)?;
                    let installed_matches = entry
                        .installed_sha256
                        .as_deref()
                        .map(|hash| hex::encode(sha2::Sha256::digest(current.as_bytes())) == hash)
                        .unwrap_or(false);
                    if !installed_matches {
                        println!("Skipped modified managed file: {key}");
                        retained.insert(key, entry);
                        continue;
                    }
                    let cleaned = ah_config::block::remove(
                        &current,
                        ah_config::block::CommentStyle::Hash,
                        "vesica",
                    );
                    if !cleaned.is_empty() {
                        println!(
                            "Skipped new managed file with unexpected unmanaged content: {key}"
                        );
                        retained.insert(key, entry);
                        continue;
                    }
                    println!(
                        "{}: {key}",
                        if dry {
                            "Would remove new managed file"
                        } else {
                            "Removing new managed file"
                        }
                    );
                    if !dry {
                        std::fs::remove_file(target)?;
                    }
                    continue;
                }
                let Some(expected_hash) = entry.original_sha256.as_deref() else {
                    println!("Skipped block without original hash: {key}");
                    retained.insert(key, entry);
                    continue;
                };
                let backup_path = backup_path.unwrap();
                let metadata = match std::fs::symlink_metadata(backup_path) {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        println!("Skipped missing backup: {key}");
                        retained.insert(key, entry);
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                };
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    anyhow::bail!(
                        "recorded backup is not a regular file: {}",
                        backup_path.display()
                    );
                }
                let backup = std::fs::read(backup_path)?;
                if hex::encode(sha2::Sha256::digest(&backup)) != expected_hash {
                    println!("Skipped altered backup: {key}");
                    retained.insert(key, entry);
                    continue;
                }
                let original = String::from_utf8(backup).context("backup is not UTF-8")?;
                let current = std::fs::read_to_string(target)?;
                if entry
                    .installed_sha256
                    .as_deref()
                    .map(|h| hex::encode(sha2::Sha256::digest(current.as_bytes())) != h)
                    .unwrap_or(true)
                {
                    println!("Skipped modified managed file: {key}");
                    retained.insert(key, entry);
                    continue;
                }
                // The file is exactly what install wrote (hash checked above). Restoring the
                // verified backup is exact when re-applying the install transform to it
                // reproduces the current bytes; this also covers originals without a final
                // newline, which block removal alone cannot reconstruct. Stripping the block is
                // kept as a fallback for older installs.
                let style = ah_config::block::CommentStyle::Hash;
                let restorable = ["vesica", "agent-home"].iter().any(|name| {
                    ah_config::block::get(&current, style, name).is_some_and(|body| {
                        ah_config::block::upsert(&original, style, name, &body) == current
                    }) || ah_config::block::remove(&current, style, name) == original
                });
                if !restorable {
                    println!("Skipped block because stripping the managed region would not restore backup: {key}");
                    retained.insert(key, entry);
                    continue;
                }
                println!(
                    "{}: {key}",
                    if dry {
                        "Would restore original file"
                    } else {
                        "Restoring original file"
                    }
                );
                if !dry {
                    std::fs::write(target, original)?;
                }
            }
            _ => {
                let verified = entry
                    .installed_sha256
                    .as_deref()
                    .and_then(|expected| {
                        std::fs::read(target)
                            .ok()
                            .map(|bytes| hex::encode(sha2::Sha256::digest(bytes)) == expected)
                    })
                    .unwrap_or(false);
                if !verified {
                    println!("Skipped modified, missing or unverifiable: {key}");
                    retained.insert(key, entry);
                } else {
                    println!("{}: {key}", if dry { "Would remove" } else { "Removing" });
                    if !dry {
                        std::fs::remove_file(target)?;
                    }
                }
            }
        }
    }
    if !dry {
        m.entries = retained;
        if m.entries.is_empty() {
            std::fs::remove_file(&p)?;
            println!("Removed manifest: {}", p.display());
        } else {
            m.save_atomic(&p)?;
            println!("Retained {} skipped manifest entries", m.entries.len());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::utc_stamp;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn utc_stamp_formats_calendar_dates_and_nanoseconds() {
        // Expected values computed independently with Python's datetime.
        for (secs, nanos, expected) in [
            (0, 0, "19700101T000000.000000000Z"),
            (1709251199, 7, "20240229T235959.000000007Z"),
            (951868800, 999999999, "20000301T000000.999999999Z"),
            (1795013862, 64527354, "20261118T145742.064527354Z"),
            (4133980799, 1, "21001231T235959.000000001Z"),
        ] {
            let time = UNIX_EPOCH + Duration::new(secs, nanos);
            assert_eq!(utc_stamp(time), expected);
        }
    }
}
