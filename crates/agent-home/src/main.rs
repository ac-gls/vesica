mod preflight;
mod recovery;
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
    /// Internal migration entry point, invoked only by a verified update executable.
    #[command(name = "__update-relink", hide = true)]
    UpdateRelink(UpdateRelinkOptions),
    /// Scaffold a skill on a branch in the shared home
    NewSkill(NewSkillOptions),
    /// Plan copying Standards/* into opted-in code repositories
    SyncStandards(SyncStandardsOptions),
    /// Promote a personal note into the shared home as a reviewed PR
    Share(ShareOptions),
    /// Show the configuration Vesica resolves and the layer behind each value
    Config(ConfigOptions),
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
    #[arg(long, env = "AGENT_HOME_REPO", env = "VESICA_HOME_REPO")]
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
/// The running CLI's version, compared against the home's `[minimum] vesica` (plan F11).
const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, clap::Args)]
struct ConfigOptions {
    #[command(subcommand)]
    command: ConfigCommand,
    // These repeat two of `Options`' fields rather than flattening `Options`, so they can be
    // `global`: a read-only diagnostic is naturally typed `vesica config list --root ...`, and
    // clap would otherwise reject everything after the subcommand.
    #[arg(
        long,
        global = true,
        env = "AGENT_HOME_REPO",
        env = "VESICA_HOME_REPO",
        help = "Local team shared-home checkout (or set home_repo in the local config)"
    )]
    home_repo: Option<PathBuf>,
    #[arg(long, global = true, value_enum, help = "Select a team profile")]
    profile: Option<ProfileArg>,
    #[arg(
        long,
        global = true,
        env = "AGENT_HOME_ROOT",
        env = "VESICA_ROOT",
        help = "Machine root to read the local config from"
    )]
    root: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        env = "VESICA_VAULT",
        help = "Explicit personal-vault path"
    )]
    vault: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        env = "VESICA_REPOS_DIR",
        help = "Explicit repositories directory"
    )]
    repos_dir: Option<PathBuf>,
    #[arg(long, global = true, help = "Emit the resolved configuration as JSON")]
    json: bool,
}

impl ConfigOptions {
    /// The shared `Options` shape the resolver takes, so these fields keep a single precedence
    /// path and `config` cannot drift from what `install` does.
    fn as_options(&self) -> Options {
        Options {
            home_repo: self.home_repo.clone(),
            profile: self.profile,
        }
    }
}
#[derive(Debug, clap::Subcommand)]
enum ConfigCommand {
    /// Every resolved value and the layer that supplied it
    List,
    /// One resolved value and the layer that supplied it
    Get {
        /// team, home_repo, profile, vault, repos_dir or repos
        key: String,
    },
}
#[derive(Debug, clap::Args)]
struct InstallOptions {
    #[arg(long, env = "AGENT_HOME_REPO", env = "VESICA_HOME_REPO")]
    home_repo: Option<PathBuf>,
    #[arg(long, env = "AGENT_HOME_ROOT", env = "VESICA_ROOT")]
    root: Option<PathBuf>,
    #[arg(long, value_enum, help = "Select a team profile")]
    profile: Option<ProfileArg>,
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
    /// Personal note to promote (must be inside the configured personal vault)
    note: PathBuf,
    #[command(flatten)]
    options: Options,
    #[arg(long, env = "AGENT_HOME_ROOT", env = "VESICA_ROOT")]
    root: Option<PathBuf>,
    #[arg(long, env = "VESICA_VAULT", help = "Explicit personal-vault path")]
    vault: Option<PathBuf>,
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
#[derive(Debug, clap::Args)]
struct UpdateRelinkOptions {
    #[arg(long, hide = true)]
    recovery_path: Option<PathBuf>,
    #[arg(long)]
    shared_home: PathBuf,
    #[arg(long, value_enum)]
    profile: ProfileArg,
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long)]
    repos_dir: Option<PathBuf>,
    #[arg(long)]
    vault: Option<PathBuf>,
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
    #[arg(
        long,
        env = "VESICA_ROOT",
        help = "Machine configuration root (fallback: AGENT_HOME_ROOT, then HOME)"
    )]
    root: Option<PathBuf>,
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
        Command::UpdateRelink(o) => update_relink(o),
        Command::NewSkill(o) => new_skill(o),
        Command::SyncStandards(o) => sync_standards(o),
        Command::Share(o) => share(o),
        Command::Config(o) => config(&o),
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
    // F11: a pinned machine does not drift. Refuse rather than silently move past the pin.
    let config_path = config_dir(None)?;
    let local = ah_core::LocalConfig::load(&config_path)?;
    if let Some(pin) = &local.home.reference {
        if let Some(actual) = pin_mismatch(pin, path) {
            anyhow::bail!(
                "this machine pins {pin} but {} is on {actual}; change `[home] ref` in {} to move the pin, or remove it",
                path.display(),
                ah_core::LocalConfig::path_in(&config_path).display()
            );
        }
        println!(
            "Pinned to {pin}; the shared home is left where it is. Edit `[home] ref` in {} to move the pin.",
            ah_core::LocalConfig::path_in(&config_path).display()
        );
        return Ok(());
    }
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

/// Normalize an absolute path without consulting the filesystem, so missing managed targets can
/// still be validated and `..` cannot escape a permitted root.
fn normalized_absolute(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        anyhow::bail!("path is not absolute: {}", path.display());
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    anyhow::bail!("path escapes its root: {}", path.display());
                }
            }
            std::path::Component::Normal(part) => normalized.push(part),
        }
    }
    Ok(normalized)
}

fn reject_symlink_components(path: &Path) -> Result<()> {
    let path = normalized_absolute(path)?;
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                anyhow::bail!(
                    "vault path contains a symlink component: {}",
                    current.display()
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// Resolve parent directories but not the final node, which may itself be a managed symlink.
fn canonical_path_with_missing(path: &Path) -> Result<PathBuf> {
    let path = normalized_absolute(path)?;
    let mut ancestor = path.clone();
    let mut missing = Vec::new();
    loop {
        match std::fs::canonicalize(&ancestor) {
            Ok(mut canonical) => {
                for component in missing.iter().rev() {
                    canonical.push(component);
                }
                return Ok(canonical);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = ancestor.file_name().with_context(|| {
                    format!("cannot resolve existing ancestor of {}", path.display())
                })?;
                missing.push(name.to_os_string());
                ancestor.pop();
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("canonicalizing existing ancestor of {}", path.display())
                });
            }
        }
    }
}

/// Resolve parent directories but not the final node, which may itself be a managed symlink.
fn canonicalized_destination(path: &Path) -> Result<PathBuf> {
    let path = normalized_absolute(path)?;
    let name = path
        .file_name()
        .with_context(|| format!("manifest path has no filename: {}", path.display()))?;
    let mut canonical = canonical_path_with_missing(path.parent().unwrap_or(Path::new("/")))?;
    canonical.push(name);
    Ok(canonical)
}

/// Validate every manifest path before install/prune/uninstall makes filesystem changes.
fn validate_manifest_scope(
    manifest: &ah_core::Manifest,
    machine_root: &Path,
    config_dir: &Path,
) -> Result<()> {
    // A legacy relative path has no trustworthy base in the current invocation.
    // Diagnose it without interpreting or rewriting it; all scope checks below remain required.
    let require_absolute = |path: &str, field: &str| -> Result<()> {
        if !Path::new(path).is_absolute() {
            anyhow::bail!(
                "manifest {field} path is not absolute: {path}; preserve the manifest and backups; \
                 do not rebase using the current directory; establish the original install base \
                 with the machine owner before repair; see docs/TROUBLESHOOTING.md \
                 (Legacy relative manifests)"
            );
        }
        Ok(())
    };
    if !manifest.shared_home.is_empty() {
        require_absolute(&manifest.shared_home, "shared_home")?;
    }
    if let Some(vault) = manifest.vault.as_deref() {
        require_absolute(vault, "vault")?;
    }
    for (key, entry) in &manifest.entries {
        require_absolute(&entry.target, &format!("entry {key} target"))?;
        if let Some(backup) = entry.backup.as_deref() {
            require_absolute(backup, &format!("entry {key} backup"))?;
        }
        if let Some(source) = entry.source.as_deref() {
            require_absolute(source, &format!("entry {key} source"))?;
        }
    }
    let machine_root = normalized_absolute(machine_root)?;
    let canonical_machine_root = canonical_path_with_missing(&machine_root)?;
    let config_dir = normalized_absolute(config_dir)?;
    let canonical_config_dir = canonical_path_with_missing(&config_dir)?;
    if !canonical_config_dir.starts_with(&canonical_machine_root) {
        anyhow::bail!("managed config directory resolves outside the selected root");
    }
    let shared_home = if manifest.shared_home.is_empty() {
        None
    } else {
        Some(normalized_absolute(Path::new(&manifest.shared_home))?)
    };
    let vault = manifest
        .vault
        .as_deref()
        .map(|path| canonical_path_with_missing(Path::new(path)))
        .transpose()?;
    let local_config_path = ah_core::LocalConfig::path_in(&config_dir);
    match std::fs::symlink_metadata(&local_config_path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            anyhow::bail!(
                "Vesica local config path must be a regular file: {}",
                local_config_path.display()
            );
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let configured_vault = ah_core::LocalConfig::load(&config_dir)?
        .vault
        .as_deref()
        .map(canonical_path_with_missing)
        .transpose()?;
    if let Some(vault) = &vault {
        if let Some(configured) = &configured_vault {
            if vault != configured {
                anyhow::bail!("manifest vault does not match local config");
            }
        }
        if !vault.starts_with(&canonical_machine_root) && configured_vault.as_ref() != Some(vault) {
            // A manifest is editable and cannot authorize deletion outside the selected root.
            anyhow::bail!("manifest vault is not authorized by local config");
        }
    }

    for (key, entry) in &manifest.entries {
        let target = normalized_absolute(Path::new(&entry.target))
            .with_context(|| format!("manifest entry {key} target must be absolute"))?;
        let canonical_target = canonicalized_destination(&target)
            .with_context(|| format!("resolving manifest entry {key} target"))?;
        let in_managed_scope = (target.starts_with(&machine_root)
            && canonical_target.starts_with(&canonical_machine_root))
            || (target.starts_with(&config_dir)
                && canonical_target.starts_with(&canonical_config_dir));
        let approved_vault_team = entry.kind == "symlink"
            && vault.as_ref().is_some_and(|vault| {
                target == vault.join("Team")
                    && canonicalized_destination(&target)
                        .ok()
                        .and_then(|resolved| resolved.parent().map(Path::to_path_buf))
                        .as_ref()
                        == Some(vault)
            })
            && shared_home.as_ref().is_some_and(|shared| {
                entry
                    .source
                    .as_deref()
                    .and_then(|source| normalized_absolute(Path::new(source)).ok())
                    .is_some_and(|source| source.starts_with(shared))
            });
        if !in_managed_scope && !approved_vault_team {
            anyhow::bail!("manifest entry {key} target escapes the managed install scope");
        }
        if let Some(backup) = entry.backup.as_deref() {
            let backup = normalized_absolute(Path::new(backup))
                .with_context(|| format!("manifest entry {key} backup must be absolute"))?;
            let canonical_backup = canonicalized_destination(&backup)
                .with_context(|| format!("resolving manifest entry {key} backup"))?;
            if !(backup.starts_with(&machine_root)
                && canonical_backup.starts_with(&canonical_machine_root))
                && !(backup.starts_with(&config_dir)
                    && canonical_backup.starts_with(&canonical_config_dir))
            {
                anyhow::bail!("manifest entry {key} backup escapes the managed install scope");
            }
        }
        if let Some(source) = entry.source.as_deref() {
            normalized_absolute(Path::new(source))
                .with_context(|| format!("manifest entry {key} source must be absolute"))?;
        }
    }
    Ok(())
}

/// Remove symlinks recorded by an earlier install that this install no longer plans (for example
/// `~/Team` after switching to `--vault`). A link is removed only if it still points exactly where
/// Vesica put it; a link the user changed is left in place and released from the manifest.
fn prune_stale_impl<F>(
    cfg: &Path,
    plan: &ah_core::Plan,
    machine_root: &Path,
    dry: bool,
    mut journal: Option<&mut recovery::Journal>,
    mut before_remove: F,
) -> Result<()>
where
    F: FnMut(usize, &Path) -> Result<()>,
{
    let manifest_path = cfg.join("manifest.json");
    let mut manifest = ah_core::Manifest::load(&manifest_path)?;
    validate_manifest_scope(&manifest, machine_root, cfg)?;
    let mut removal_count = 0;
    let planned: std::collections::BTreeSet<&str> =
        plan.actions.iter().map(ah_core::Action::key).collect();
    let stale: Vec<String> = manifest
        .entries
        .iter()
        .filter(|(key, entry)| {
            matches!(entry.kind.as_str(), "symlink" | "generated-file")
                && !planned.contains(key.as_str())
        })
        .map(|(key, _)| key.clone())
        .collect();
    let mut changed = false;
    for key in stale {
        let entry = &manifest.entries[&key];
        let target = Path::new(&entry.target);
        if entry.kind == "generated-file" {
            match std::fs::symlink_metadata(target) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    println!("Forgot missing stale generated file: {key}");
                }
                Err(error) => return Err(error.into()),
                Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                    println!("Kept replaced stale generated file: {key}");
                    continue;
                }
                Ok(_) => {
                    let current = std::fs::read(target)?;
                    let expected = entry.installed_sha256.as_deref();
                    let matches = expected.is_some_and(|expected| {
                        use sha2::Digest;
                        hex::encode(sha2::Sha256::digest(&current)) == expected
                    });
                    if !matches {
                        println!("Kept modified stale generated file: {key}");
                        continue;
                    }
                    if dry {
                        println!("Would remove stale generated file: {key}");
                        continue;
                    }
                    removal_count += 1;
                    before_remove(removal_count, target)?;
                    remove_pruned_node(target, journal.as_deref_mut())?;
                    println!("Removed stale generated file: {key}");
                }
            }
        } else {
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
                removal_count += 1;
                before_remove(removal_count, target)?;
                remove_pruned_node(target, journal.as_deref_mut())?;
                println!("Removed stale link: {key}");
            } else if std::fs::symlink_metadata(target).is_err() {
                println!("Forgot missing stale link: {key}");
            } else {
                println!("Kept modified stale link: {key} (no longer managed by Vesica)");
            }
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

fn remove_pruned_node(target: &Path, journal: Option<&mut recovery::Journal>) -> Result<()> {
    let before = recovery::Node::read(target)?;
    std::fs::remove_file(target)?;
    if let Some(journal) = journal {
        if let Err(error) = journal.record_postimage(target, recovery::Node::Absent) {
            // The deletion is ours even when saving its checkpoint fails. Undo it
            // in the child using the known postimage before the parent loads old evidence.
            if let Err(rollback) = before.restore(target, &recovery::Node::Absent) {
                let saved = journal.save().err();
                anyhow::bail!("prune checkpoint failed: {error:#}; prune rollback failed: {rollback:#}; recovery checkpoint retry: {saved:?}");
            }
            anyhow::bail!("prune checkpoint failed: {error:#}; pruned node restored");
        }
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
    let state_dir = machine_root(o.root.as_deref())?.join(".config/agent-home");
    if let Ok(entries) = std::fs::read_dir(&state_dir) {
        for entry in entries {
            let entry = entry?;
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(".vesica-relink-")
            {
                anyhow::bail!("pending recovery at {}; preserve this journal, resolve reported user edits or restore its recorded postimage before retrying update; no migration was started", entry.path().display());
            }
        }
    }
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
    let target = match &o.target {
        Some(target) => target.clone(),
        None => std::env::current_exe().context("cannot determine the running executable")?,
    };
    let target = std::path::absolute(&target)?;
    let parent = target.parent().context("target has no parent directory")?;
    reject_symlink_components(parent)?;
    let previous = recovery::Node::read(&target)?;
    if !matches!(
        previous,
        recovery::Node::Absent | recovery::Node::File { .. }
    ) {
        anyhow::bail!("update target must be a regular file: {}", target.display());
    }
    let reports_current = release::reports_version(&target, current);
    if tag.trim_start_matches('v') == current && reports_current {
        println!("Already up to date: vesica {current} ({tag})");
        if o.dry_run {
            println!("Dry run: pending re-link would use the recorded manifest; nothing changed");
        } else {
            relink_from_manifest(&o, &target)?;
        }
        return Ok(());
    }
    if tag.trim_start_matches('v') == current && !reports_current {
        println!("Installing {tag}: target does not report vesica {current}");
    }
    std::fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".vesica-update-")
        .tempdir_in(parent)?;
    let base_url = std::env::var("VESICA_RELEASE_BASE_URL")
        .unwrap_or_else(|_| release::DEFAULT_BASE_URL.to_owned());
    let mut installed = None;
    let outcome = (|| -> Result<()> {
        let tarball = release::download(staging.path(), &base_url, &tag)?;
        release::verify_checksum(staging.path(), &tarball)?;
        let binary = release::extract(staging.path(), &tarball, &tag)?;
        release::check_reports_version(&binary, &tag)?;
        if o.dry_run {
            println!(
                "Would replace {} (vesica {})",
                target.display(),
                tag.trim_start_matches('v')
            );
            return Ok(());
        }
        if !previous.matches(&target)? {
            anyhow::bail!("update target changed during verification; preserving user changes");
        }
        replace_update_binary(&target, &binary, &mut installed, recovery::Node::read)?;
        println!(
            "Replaced {} (vesica {})",
            target.display(),
            tag.trim_start_matches('v')
        );
        relink_impl(&o, &target, |_| {
            std::fs::remove_dir_all(staging.path()).context("update staging cleanup failed")
        })?;
        Ok(())
    })();
    let mut errors = Vec::new();
    if outcome.is_err() {
        if let Some(expected) = installed.as_ref() {
            if let Err(error) = previous.restore(&target, expected) {
                errors.push(format!("binary rollback failed: {error:#}"));
            }
        }
    }
    if staging.path().exists() {
        if let Err(error) = staging.close() {
            errors.push(format!("staging cleanup failed: {error}"));
        }
    }
    match (outcome, errors.is_empty()) {
        (Err(error), true) => Err(error),
        (Err(error), false) => anyhow::bail!("{error:#}; {}", errors.join("; ")),
        (Ok(()), false) => anyhow::bail!("{}", errors.join("; ")),
        (Ok(()), true) => Ok(()),
    }
}

fn replace_update_binary<F>(
    target: &Path,
    binary: &Path,
    installed: &mut Option<recovery::Node>,
    read_postimage: F,
) -> Result<()>
where
    F: FnOnce(&Path) -> Result<recovery::Node>,
{
    // The staged candidate bytes are trusted before rename; replace fixes its mode
    // to 0755. Establish ownership as soon as rename succeeds, before fallible I/O.
    let recovery::Node::File { bytes, .. } = recovery::Node::read(binary)? else {
        anyhow::bail!("verified candidate must be a regular file");
    };
    let expected = recovery::Node::File {
        bytes,
        permissions: {
            use std::os::unix::fs::PermissionsExt;
            std::fs::Permissions::from_mode(0o100755)
        },
    };
    release::replace(target, binary)?;
    *installed = Some(expected);
    read_postimage(target)?;
    Ok(())
}

fn run_manifest_relink_from_path(
    executable: &Path,
    manifest_path: &Path,
    repos_dir: Option<&Path>,
    root_dir: Option<&Path>,
    recovery_path: &Path,
) -> Result<()> {
    let manifest = ah_core::Manifest::load(manifest_path)?;
    if manifest.shared_home.is_empty() {
        println!("Manifest records no shared home; skipping re-link");
        return Ok(());
    }
    let profile = match manifest.profile.as_str() {
        "core" => "core",
        "lead" => "lead",
        _ => "developer",
    };
    let mut command = std::process::Command::new(executable);
    command
        .arg("__update-relink")
        .arg("--recovery-path")
        .arg(recovery_path)
        .arg("--shared-home")
        .arg(&manifest.shared_home)
        .arg("--profile")
        .arg(profile);
    if let Some(root) = root_dir {
        command.arg("--root").arg(root);
    }
    if let Some(repos) = repos_dir {
        command.arg("--repos-dir").arg(repos);
    }
    if let Some(vault) = &manifest.vault {
        command.arg("--vault").arg(vault);
    }
    let status = command
        .status()
        .context("running verified replacement for re-link")?;
    if !status.success() {
        anyhow::bail!("verified replacement re-link failed with {status}");
    }
    Ok(())
}

fn update_relink(o: UpdateRelinkOptions) -> Result<()> {
    println!(
        "Re-linking from the recorded manifest ({})",
        o.shared_home.display()
    );
    install_with_recovery(
        InstallOptions {
            home_repo: Some(o.shared_home),
            profile: Some(o.profile),
            root: o.root,
            dry_run: false,
            repos_dir: o.repos_dir,
            vault: o.vault,
        },
        o.recovery_path.as_deref(),
    )
}

fn relink_from_manifest(o: &UpdateOptions, executable: &Path) -> Result<()> {
    relink_impl(o, executable, |_| Ok(()))
}

fn relink_impl<F>(o: &UpdateOptions, executable: &Path, before_cleanup: F) -> Result<()>
where
    F: FnOnce(&Path) -> Result<()>,
{
    relink_with_cleanup(o, executable, before_cleanup, |path| {
        std::fs::remove_dir_all(path)
    })
}

fn relink_with_cleanup<F, C>(
    o: &UpdateOptions,
    executable: &Path,
    before_cleanup: F,
    cleanup: C,
) -> Result<()>
where
    F: FnOnce(&Path) -> Result<()>,
    C: FnOnce(&Path) -> std::io::Result<()>,
{
    relink_with_checkpoint_loader(
        o,
        executable,
        before_cleanup,
        cleanup,
        recovery::Journal::load,
    )
}

fn relink_with_checkpoint_loader<F, C, L>(
    o: &UpdateOptions,
    executable: &Path,
    before_cleanup: F,
    cleanup: C,
    load_checkpoint: L,
) -> Result<()>
where
    F: FnOnce(&Path) -> Result<()>,
    C: FnOnce(&Path) -> std::io::Result<()>,
    L: FnOnce(&Path) -> Result<Option<recovery::Journal>>,
{
    let manifest_path = root(o.root.as_deref())?.join(".config/agent-home/manifest.json");
    if !manifest_path.is_file() {
        println!(
            "No manifest recorded at {}; skipping re-link",
            manifest_path.display()
        );
        before_cleanup(manifest_path.parent().context("manifest has no parent")?)?;
        return Ok(());
    }
    let journal_dir = tempfile::Builder::new()
        .prefix(".vesica-relink-")
        .tempdir_in(manifest_path.parent().context("manifest has no parent")?)?;
    let journal_path = journal_dir.path().join("transaction.json");
    let mut result = run_manifest_relink_from_path(
        executable,
        &manifest_path,
        o.repos_dir.as_deref(),
        Some(&machine_root(o.root.as_deref())?),
        &journal_path,
    );
    if let Err(cleanup) = before_cleanup(journal_dir.path()) {
        result = match result {
            Ok(()) => Err(anyhow::anyhow!("re-link cleanup failed: {cleanup:#}")),
            Err(error) => Err(anyhow::anyhow!(
                "{error:#}; re-link cleanup failed: {cleanup:#}"
            )),
        };
    }
    let mut errors = Vec::new();
    if result.is_err() {
        if let Err(error) = recovery::Journal::rollback_file(&journal_path) {
            errors.push(format!("configuration rollback failed: {error:#}"));
        }
    }
    if !errors.is_empty()
        || result
            .as_ref()
            .is_err_and(|e| format!("{e:#}").contains("cleanup failed"))
    {
        let retained = journal_dir.keep();
        errors.push(format!(
            "recovery journal retained at {}; resolve conflicts then retry",
            retained.display()
        ));
    } else {
        // Remove payloads before the journal itself; a cleanup failure retains recoverable state.
        let journal_parent = journal_dir.path().to_owned();
        // Hold the checkpoint until cleanup is accepted: removal can unlink the
        // journal before failing, and a missing file is not rollback evidence.
        let checkpoint = match load_checkpoint(&journal_path) {
            Ok(checkpoint) => checkpoint,
            Err(error) => {
                // Loading failed after migration; never let TempDir discard the durable
                // checkpoint. Retry only recovery, not migration, then retain evidence.
                if let Err(rollback) = recovery::Journal::rollback_file(&journal_path) {
                    errors.push(format!("configuration rollback failed: {rollback:#}"));
                }
                let retained = journal_dir.keep();
                anyhow::bail!(
                    "checkpoint load failed: {error:#}; {}; recovery journal retained at {}",
                    errors.join("; "),
                    retained.display()
                );
            }
        };
        if let Err(error) = cleanup(&journal_parent) {
            if let Some(mut checkpoint) = checkpoint {
                if let Err(rollback) = checkpoint.rollback() {
                    errors.push(format!("configuration rollback failed: {rollback:#}"));
                }
                if let Err(save) = checkpoint.save() {
                    errors.push(format!("recovery evidence recreation failed: {save:#}"));
                }
            }
            let retained = journal_dir.keep();
            errors.push(format!(
                "re-link cleanup failed: {error}; recovery journal retained at {}",
                retained.display()
            ));
        }
    }
    match (result, errors.is_empty()) {
        (Err(error), true) => Err(error),
        (Err(error), false) => anyhow::bail!("{error:#}; {}", errors.join("; ")),
        (Ok(()), false) => anyhow::bail!("{}", errors.join("; ")),
        (Ok(()), true) => Ok(()),
    }
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
    let out = ah_vault::git::command(shared, &["var", "GIT_AUTHOR_IDENT"], false)
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
    let resolved = resolve_home_repo(&o.options, o.root.as_deref())?;
    let (vault, _source) =
        ah_core::local::resolve(o.vault.clone(), resolved.local.vault.clone(), None, None);
    let vault = vault.ok_or_else(|| {
        anyhow::anyhow!(
            "no personal vault configured: pass --vault <path>, set VESICA_VAULT, or add `vault` to {}",
            ah_core::LocalConfig::path_in(&resolved.config_dir).display()
        )
    })?;
    let vault = std::fs::canonicalize(&vault)
        .with_context(|| format!("personal vault path does not exist: {}", vault.display()))?;
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
        let status =
            ah_vault::git::command(&shared, &["push", "-u", "origin", &plan.branch], false)
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

/// Personal writers must not treat known shared storage as a personal root. Consider all
/// configured/env sources, not just the winning resolver layer, so an override cannot hide
/// another known shared boundary. Canonicalize missing suffixes without creating them.
fn canonical_shared_storage(path: &Path) -> Result<PathBuf> {
    // Resolve symlink/.. with filesystem semantics first. Only missing normal suffixes
    // may be appended lexically; dangling markers and unresolved traversal fail closed.
    let mut ancestor = std::path::absolute(path)?;
    let mut missing = Vec::new();
    loop {
        match std::fs::canonicalize(&ancestor) {
            Ok(mut resolved) => {
                for part in missing.iter().rev() {
                    resolved.push(part);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if std::fs::symlink_metadata(&ancestor).is_ok() {
                    anyhow::bail!("cannot resolve shared-home storage: {}", ancestor.display());
                }
                let Some(std::path::Component::Normal(part)) = ancestor.components().next_back()
                else {
                    anyhow::bail!(
                        "cannot safely resolve shared-home storage: {}",
                        path.display()
                    );
                };
                missing.push(part.to_os_string());
                ancestor.pop();
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("resolving shared-home storage {}", path.display()))
            }
        }
    }
}

fn validate_personal_write_scope(vault: &VaultPath) -> Result<()> {
    // clap already applies --root > VESICA_ROOT. Its repeated env setters do not
    // define aliases, so preserve legacy AGENT_HOME_ROOT explicitly before HOME.
    let machine = vault
        .root
        .clone()
        .or_else(|| std::env::var_os("AGENT_HOME_ROOT").map(PathBuf::from));
    validate_personal_storage(&vault.path, &config_dir(machine.as_deref())?, None)
}

fn validate_personal_storage(
    path: &Path,
    cfg: &Path,
    selected_shared: Option<&Path>,
) -> Result<()> {
    // Inspect the raw personal selector before helpers can erase symlink/.. semantics.
    // All callers keep this same selector for I/O, so traversal is refused, not rewritten.
    if path
        .components()
        .any(|part| part == std::path::Component::ParentDir)
    {
        anyhow::bail!(
            "personal vault path must not contain parent traversal (..): {}",
            path.display()
        );
    }
    let target = std::path::absolute(path)?;
    reject_symlink_components(&target)?;
    reject_symlink_components(cfg)?;
    let config_path = ah_core::LocalConfig::path_in(cfg);
    match std::fs::symlink_metadata(&config_path) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() => {
            anyhow::bail!(
                "local configuration is not a regular file: {}",
                config_path.display()
            );
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let local = ah_core::LocalConfig::load(cfg)?;
    let target = canonical_path_with_missing(&target)?;
    let shared_paths = local
        .home_repo
        .into_iter()
        .chain(selected_shared.map(Path::to_path_buf))
        .chain(
            ["VESICA_HOME_REPO", "AGENT_HOME_REPO", "VESICA_TEAM_HOME"]
                .into_iter()
                .filter_map(|key| std::env::var_os(key).map(PathBuf::from)),
        );
    for shared in shared_paths {
        let shared = canonical_shared_storage(&shared)?;
        if target.starts_with(&shared) {
            anyhow::bail!("personal write target is inside shared-home storage: {}; choose a separate personal vault with --path", target.display());
        }
        // Team is the existing excluded subtree. A real nested shared directory
        // anywhere else would be writable by fix/new-note or eligible for snapshot.
        if shared.starts_with(&target) && !shared.starts_with(target.join("Team")) {
            anyhow::bail!("shared-home storage is nested outside the personal vault's excluded Team subtree: {}; use separate storage or place shared storage under <personal>/Team", shared.display());
        }
    }
    Ok(())
}

fn vault(command: VaultCommand) -> Result<()> {
    use ah_vault::personal;
    let print_changes = print_vault_changes;
    match command {
        VaultCommand::Init { vault, dry_run } => {
            validate_personal_write_scope(&vault)?;
            let changes = personal::init(&vault.path, &utc_date(), dry_run)?;
            print_changes(&changes, dry_run);
            if !dry_run {
                println!("Vault created: {}", vault.path.display());
            }
        }
        VaultCommand::Fix { vault, dry_run } => {
            validate_personal_write_scope(&vault)?;
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
            validate_personal_write_scope(&vault)?;
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
            validate_personal_write_scope(&vault)?;
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
    let report = ah_vault::scan_report(path)?;
    println!(
        "Scanned {} text file(s) (bounded UTF-8, <=2,000,000 bytes; heuristic, not comprehensive)",
        report.scanned_files
    );
    for asset in &report.excluded_assets {
        println!(
            "Excluded/unscanned asset: {} (image/PDF exemption; not counted as clean)",
            asset.display()
        );
    }
    let findings = report.findings;
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
/// F9: read-only view of the configuration Vesica resolves, naming the layer behind every value.
/// Nothing here writes — `config set` is deliberately not implemented yet (plan: read-only first).
fn config(o: &ConfigOptions) -> Result<()> {
    let options = o.as_options();
    let resolved = resolve_home_repo(&options, o.root.as_deref())?;
    let (c, _) = load(&resolved.home_repo, &resolved.local)?;
    let (profile, profile_source) = prof(&options, &resolved.local, &c);
    let (vault, vault_source) =
        ah_core::local::resolve(o.vault.clone(), resolved.local.vault.clone(), None, None);
    let (repos_dir, repos_dir_source) = ah_core::local::resolve(
        o.repos_dir.clone(),
        resolved.local.repos_dir.clone(),
        None,
        None,
    );
    let repo_names: Vec<&str> = c.repos.iter().map(|repo| repo.name.as_str()).collect();
    // A merged list is a team fact plus, when the machine adds entries, a local one.
    let repos_source = if resolved.local.repos.is_empty() {
        "team"
    } else {
        "team + config.toml"
    };
    let rows: Vec<(&str, String, &str)> = vec![
        ("team", format!("{} ({})", c.team.name, c.team.org), "team"),
        (
            "home_repo",
            resolved.home_repo.display().to_string(),
            resolved.source.as_str(),
        ),
        ("profile", profile.to_owned(), profile_source.as_str()),
        (
            "vault",
            path_or_unset(&vault),
            path_source(&vault, vault_source),
        ),
        (
            "repos_dir",
            path_or_unset(&repos_dir),
            path_source(&repos_dir, repos_dir_source),
        ),
        (
            "repos",
            if repo_names.is_empty() {
                "(none)".to_owned()
            } else {
                repo_names.join(", ")
            },
            repos_source,
        ),
    ];
    let wanted: Vec<(&str, String, &str)> = match &o.command {
        ConfigCommand::List => rows,
        ConfigCommand::Get { key } => {
            let found: Vec<_> = rows
                .into_iter()
                .filter(|(name, _, _)| name == key)
                .collect();
            if found.is_empty() {
                anyhow::bail!(
                    "unknown configuration key {key:?}; try one of: team, home_repo, profile, vault, repos_dir, repos"
                );
            }
            found
        }
    };
    if o.json {
        let mut map = serde_json::Map::new();
        for (name, value, source) in wanted {
            map.insert(
                name.to_owned(),
                serde_json::json!({"value": value, "source": source}),
            );
        }
        println!("{}", serde_json::to_string_pretty(&map)?);
    } else {
        let width = wanted
            .iter()
            .map(|(name, _, _)| name.len())
            .max()
            .unwrap_or(0);
        for (name, value, source) in wanted {
            println!("{name:<width$} = {value} ({source})");
        }
    }
    Ok(())
}
/// A path value for display, or the word for "no layer supplied one".
fn path_or_unset(value: &Option<PathBuf>) -> String {
    match value {
        Some(path) => path.display().to_string(),
        None => "(unset)".to_owned(),
    }
}
/// The layer for a path value. An unset value has no layer, so report the built-in default rather
/// than a layer that did not supply it.
fn path_source(value: &Option<PathBuf>, source: ah_core::Source) -> &'static str {
    if value.is_some() {
        source.as_str()
    } else {
        ah_core::Source::Default.as_str()
    }
}
fn doctor(o: &DoctorOptions) -> Result<()> {
    let resolved = resolve_home_repo(&o.options, None)?;
    let (c, f) = load(&resolved.home_repo, &resolved.local)?;
    let wsl = ah_wsl::is_wsl()?;
    let wsl_status = if wsl { "detected" } else { "not detected" };
    let (selected, profile_source) = prof(&o.options, &resolved.local, &c);
    let selected_profile = ah_agents::ProfileSelection::from_name(&c, selected)?;
    let mut checks = preflight::Preflight::run(c.minimum.node.as_deref());
    // F11: the home may declare a minimum CLI and a minimum home version. A mismatch blocks.
    let home_ref = detected_ref(&resolved.home_repo);
    if let Some(reason) = compatibility_blocker(&c, home_ref.as_deref()) {
        // The blocker is what blocks. It is not a "required command" row: the report's
        // `prerequisites.required` values are constrained to `found`/`missing`, so an
        // incompatibility belongs in `blockers` with a descriptive informational row.
        checks.optional.push((
            "home version",
            "incompatible: the home and this CLI disagree".to_owned(),
        ));
        checks.blockers.push(reason);
    }
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
            "Home ref: {} · CLI {CLI_VERSION}\nPlatform: {} {} ({}, kernel {})",
            home_ref
                .as_deref()
                .unwrap_or("unknown (not a git checkout)"),
            checks.platform.os,
            checks.platform.arch,
            checks.platform.distro,
            checks.platform.kernel
        );
        if let Some(required) = &c.minimum.vesica {
            println!("Requires Vesica: {required}");
        }
        if let Some(required) = &c.minimum.home {
            println!("Requires home: {required}");
        }
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
/// The ref a shared-home checkout is on: its exact tag, else its branch, else a short sha.
/// Read-only and offline: it never fetches, so `status` stays safe to run anywhere.
fn detected_ref(shared: &Path) -> Option<String> {
    let git = |args: &[&str]| -> Option<String> {
        let output = ah_vault::git::command(shared, args, true).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8(output.stdout).ok()?;
        let text = text.trim().to_owned();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    };
    if let Some(tag) = git(&["describe", "--tags", "--exact-match"]) {
        return Some(tag);
    }
    match git(&["rev-parse", "--abbrev-ref", "HEAD"]) {
        Some(branch) if branch != "HEAD" => Some(branch),
        _ => git(&["rev-parse", "--short", "HEAD"]),
    }
}

/// The branch a checkout is on, or a short sha when detached.
fn detected_branch(shared: &Path) -> Option<String> {
    let output = ah_vault::git::command(shared, &["rev-parse", "--abbrev-ref", "HEAD"], true)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    if branch.is_empty() || branch == "HEAD" {
        return detected_ref(shared);
    }
    Some(branch)
}

/// Whether a checkout satisfies a pin. `Some(actual)` means it does not, naming what it is on.
///
/// A release pin (`v10.5`) requires the checkout to be **at that release** — the exact tag, since
/// that is what "installed from v10.5" means. A floating pin (`main`) requires the checkout to be
/// **on that ref**, so the tag sitting at the branch head must not be mistaken for the pin.
fn pin_mismatch(pin: &str, shared: &Path) -> Option<String> {
    let actual = if ah_core::version::floats(pin) {
        detected_branch(shared)
    } else {
        detected_ref(shared)
    };
    match actual {
        Some(actual) if actual == pin => None,
        Some(actual) => Some(actual),
        None => Some("no detectable git ref".to_owned()),
    }
}

/// Local release tags. Never fetches: drift reports only what this checkout already knows.
fn local_tags(shared: &Path) -> Vec<String> {
    let output = match ah_vault::git::command(shared, &["tag", "--list"], true).output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Whether the running CLI and the checked-out home satisfy what the home declares in
/// `[minimum]` (plan F11). Returns the reason `doctor` must block, or `None`.
/// An unparseable value is never a blocker — it cannot be checked, so it is carried on.
fn compatibility_blocker(c: &ah_core::AgentHomeConfig, detected: Option<&str>) -> Option<String> {
    use ah_core::version::{check_minimum, Compatibility};
    if let Some(required) = &c.minimum.vesica {
        if check_minimum(CLI_VERSION, required) == Compatibility::TooOld {
            return Some(format!(
                "this home needs Vesica {required} or newer, but this CLI is {CLI_VERSION}; upgrade with `vesica update` and re-run"
            ));
        }
    }
    if let (Some(required), Some(detected)) = (&c.minimum.home, detected) {
        if check_minimum(detected, required) == Compatibility::TooOld {
            return Some(format!(
                "this home's content needs home version {required} or newer, but the checkout is {detected}; run `vesica team pull`, or check out {required}"
            ));
        }
    }
    None
}

fn install(o: InstallOptions) -> Result<()> {
    install_with_recovery(o, None)
}

fn install_with_recovery(o: InstallOptions, recovery_path: Option<&Path>) -> Result<()> {
    install_impl(o, recovery_path, || Ok(()))
}

fn install_impl<F>(o: InstallOptions, recovery_path: Option<&Path>, before_seal: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    let InstallOptions {
        home_repo,
        root,
        profile,
        dry_run,
        repos_dir,
        vault,
    } = o;
    let options = Options { home_repo, profile };
    let resolved = resolve_home_repo(&options, root.as_deref())?;
    let vault = vault.or_else(|| resolved.local.vault.clone());
    if let Some(vault) = &vault {
        // Covers both ordinary install and the new-executable relink caller before
        // machine preparation, backups, actions or personal scaffolding.
        validate_personal_storage(vault, &resolved.config_dir, Some(&resolved.home_repo))?;
    }
    let (c, _) = load(&resolved.home_repo, &resolved.local)?;
    let selected = profile
        .map(ProfileArg::text)
        .unwrap_or_else(|| prof(&options, &resolved.local, &c).0);
    let home = machine_root(root.as_deref())?;
    let cfg = resolved.config_dir.clone();
    validate_manifest_scope(&ah_core::Manifest::default(), &home, &cfg)?;
    let shared = resolved.home_repo.canonicalize().with_context(|| {
        format!(
            "shared-home path does not exist: {}",
            resolved.home_repo.display()
        )
    })?;
    // F11: a pinned machine installs from the pin, not from whatever the checkout happens to be on.
    let home_ref = detected_ref(&shared);
    if let Some(pin) = &resolved.local.home.reference {
        if let Some(actual) = pin_mismatch(pin, &shared) {
            anyhow::bail!(
                "the shared home at {} is on {actual} but this machine pins {pin}; check out {pin} in that checkout, or change `[home] ref` in {}",
                shared.display(),
                ah_core::LocalConfig::path_in(&cfg).display()
            );
        }
    }
    if !shared.join("AGENTS.md").is_file() {
        anyhow::bail!("shared home must contain AGENTS.md: {}", shared.display());
    }
    // Install step 3 (D-H): with --vault the shared home is linked at <vault>/Team instead of
    // ~/Team, unless the shared-home checkout already lives there (plan step 4 layout).
    let vault = match &vault {
        Some(dir) => {
            let absolute = std::path::absolute(dir)
                .with_context(|| format!("cannot resolve --vault {}", dir.display()))?;
            reject_symlink_components(&absolute)?;
            Some(absolute)
        }
        None => None,
    };
    let team_link = vault
        .as_ref()
        .map_or_else(|| home.join("Team"), |v| v.join("Team"));
    let resolved_root = canonical_path_with_missing(&home)?;
    if let Some(vault) = &vault {
        // Explicit --vault authorizes the initial external location. Reinstalling an already
        // recorded external vault is authorized only by config.toml, never by manifest.json.
        if !vault.starts_with(&resolved_root) {
            let configured = resolved
                .local
                .vault
                .as_deref()
                .map(canonical_path_with_missing)
                .transpose()?;
            let requested = canonical_path_with_missing(vault)?;
            if configured
                .as_ref()
                .is_some_and(|configured| configured != &requested)
            {
                anyhow::bail!("--vault does not match the vault in local config");
            }
        }
    }
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
    if let (false, Some(dir)) = (profile.profile == ah_core::Profile::Core, &repos_dir) {
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
    for role in ah_agents::discover_roles(&shared.join("Agents/Roles"))? {
        let (name, content) = ah_agents::render_opencode_role(&role, &c.team.role_prefix);
        let path = home
            .join(".config/opencode/agents")
            .join(format!("{name}.md"));
        actions.push(ah_core::Action::WriteFile {
            path: path.to_string_lossy().into_owned(),
            content,
        });
    }
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
    let plan = planner.finish(dry_run);
    println!(
        "{} install plan (profile: {selected}; eligible repositories: {})",
        if dry_run {
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
    let repo_plan = match (&profile.profile, &repos_dir) {
        (ah_core::Profile::Core, _) => Err(
            "Repository cloning skipped: the core profile does not clone repositories".to_owned(),
        ),
        (_, None) => Err(format!(
            "Repository cloning skipped: pass --repos-dir <dir> to clone {} eligible repositories",
            profile.repositories.len()
        )),
        (_, Some(dir)) => Ok((dir.clone(), repos::plan(&profile.repositories, dir)?)),
    };
    let local_path = ah_core::LocalConfig::path_in(&cfg);
    let mut local_preimage = None;
    if !dry_run {
        let local_metadata = match std::fs::symlink_metadata(&local_path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                anyhow::bail!(
                    "Vesica local config path must be a regular file: {}",
                    local_path.display()
                );
            }
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        local_preimage = match local_metadata {
            Some(metadata) => Some((std::fs::read(&local_path)?, metadata.permissions())),
            None => None,
        };
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
        validate_manifest_scope(&existing, &home, &cfg)?;
        for action in &plan.actions {
            let target = Path::new(action.key());
            let target_metadata = match std::fs::symlink_metadata(target) {
                Ok(metadata) => Some(metadata),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            match action {
                ah_core::Action::WriteFile { .. }
                | ah_core::Action::UpsertBlock { .. }
                | ah_core::Action::MergeJsonc { .. }
                | ah_core::Action::MergeToml { .. }
                    if target_metadata.as_ref().is_some_and(|metadata| {
                        metadata.file_type().is_symlink() || !metadata.is_file()
                    }) =>
                {
                    anyhow::bail!(
                        "managed file destination must be a regular file: {}",
                        target.display()
                    );
                }
                ah_core::Action::Symlink { .. }
                    if target_metadata.is_some()
                        && !existing.entries.contains_key(action.key()) =>
                {
                    anyhow::bail!("unmanaged destination already exists: {}", target.display());
                }
                _ => {}
            }
            if let Some(record) = existing.entries.get(action.key()) {
                match record.kind.as_str() {
                    "symlink" => {
                        let metadata = target_metadata
                            .as_ref()
                            .context("recorded symlink target is missing")?;
                        if !metadata.file_type().is_symlink() {
                            anyhow::bail!("recorded symlink was replaced: {}", target.display());
                        }
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
                    "generated-file" => match target_metadata.as_ref() {
                        None => {}
                        Some(metadata)
                            if metadata.is_file() && !metadata.file_type().is_symlink() =>
                        {
                            let current = std::fs::read(target)?;
                            let hash = {
                                use sha2::Digest;
                                hex::encode(sha2::Sha256::digest(current))
                            };
                            if record.installed_sha256.as_deref() != Some(hash.as_str()) {
                                anyhow::bail!(
                                    "recorded generated file was modified: {}",
                                    target.display()
                                );
                            }
                        }
                        Some(_) => anyhow::bail!(
                            "recorded generated file is not a regular file: {}",
                            target.display()
                        ),
                    },
                    "managed-block" | "merged-file" => match target_metadata.as_ref() {
                        Some(metadata)
                            if metadata.file_type().is_symlink() || !metadata.is_file() =>
                        {
                            anyhow::bail!(
                                "recorded managed target is not a regular file: {}",
                                target.display()
                            );
                        }
                        Some(_) => {
                            let bytes = std::fs::read(target)?;
                            let current_hash = {
                                use sha2::Digest;
                                hex::encode(sha2::Sha256::digest(&bytes))
                            };
                            if record.installed_sha256.as_deref() != Some(current_hash.as_str()) {
                                anyhow::bail!(
                                    "recorded managed file was modified: {}",
                                    target.display()
                                );
                            }
                        }
                        None => {}
                    },
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
                    ah_core::Action::WriteFile { .. } => {
                        let existing = std::fs::symlink_metadata(target)?;
                        if !existing.is_file() || existing.file_type().is_symlink() {
                            anyhow::bail!(
                                "generated-file destination is not a regular file: {}",
                                target.display()
                            );
                        }
                        anyhow::bail!("unmanaged destination already exists: {}", target.display());
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
    // Prepare and sync the actual local-state replacement before unmanaged vault writes.
    // This checks parent write permissions without committing uninstalled machine facts.
    let local_before = recovery::Node::read(&local_path)?;
    let mut prepared_local = if dry_run {
        None
    } else {
        let mut local = resolved.local.clone();
        local.home_repo = Some(shared.to_path_buf());
        if let Some(profile) = options.profile {
            local.profile = Some(profile_value(profile));
        }
        if let Some(v) = &vault {
            local.vault = Some(v.clone());
        }
        if let Some(dir) = &repos_dir {
            local.repos_dir = Some(
                std::path::absolute(dir)
                    .with_context(|| format!("cannot resolve --repos-dir {}", dir.display()))?,
            );
        }
        std::fs::create_dir_all(&cfg)?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".vesica-local-")
            .tempfile_in(&cfg)
            .with_context(|| format!("cannot prepare local configuration in {}", cfg.display()))?;
        use std::io::Write;
        temporary.write_all(local.to_toml()?.as_bytes())?;
        if let Some((_, permissions)) = &local_preimage {
            temporary.as_file().set_permissions(permissions.clone())?;
        }
        temporary.as_file().sync_all()?;
        Some(temporary)
    };
    // Vault scaffolding runs only after every refusal above. It is personal data: never
    // overwritten, not recorded in the manifest, and kept by uninstall (like cloned repositories).
    if let Some(vault) = &vault {
        let adopt = std::fs::read_dir(vault).is_ok_and(|mut entries| entries.next().is_some());
        let date = utc_date();
        let changes = if adopt {
            ah_vault::personal::fix(vault, &date, dry_run)?
        } else {
            ah_vault::personal::init(vault, &date, dry_run)?
        };
        println!(
            "Vault: {} ({})",
            vault.display(),
            if adopt { "adopting" } else { "creating" }
        );
        print_vault_changes(&changes, dry_run);
        if team_in_place {
            println!(
                "Team: the shared home is already at {} (no link needed)",
                team_link.display()
            );
        }
    }
    let mut pending_backups = Vec::new();
    let backups = if dry_run {
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
    let mut recovery_journal = match recovery_path {
        Some(path) if !dry_run => {
            let mut paths: Vec<PathBuf> = plan
                .actions
                .iter()
                .map(|a| PathBuf::from(a.key()))
                .collect();
            paths.extend([local_path.clone(), cfg.join("manifest.json")]);
            paths.extend(pending_backups.iter().map(|(path, _)| path.clone()));
            let old = ah_core::Manifest::load(&cfg.join("manifest.json"))?;
            paths.extend(
                old.entries
                    .values()
                    .filter(|e| matches!(e.kind.as_str(), "symlink" | "generated-file"))
                    .map(|e| PathBuf::from(&e.target)),
            );
            Some(recovery::Journal::prepare(path, paths)?)
        }
        _ => None,
    };
    let outcomes = ah_core::action::Executor.execute_with_commit(&plan, |outcomes| {
        if plan.dry_run {
            return Ok(());
        }
        if outcomes.contains(&ah_core::ActionOutcome::Collision) {
            anyhow::bail!("install stopped on an existing destination; no changes should be treated as fully applied");
        }
        let local_preimage = local_preimage.as_ref();
        let created_backups = create_install_backups(&pending_backups)?;
        let mut local_postimage = None;
        let manifest_path = cfg.join("manifest.json");
        let manifest_preimage = recovery::Node::read(&manifest_path)?;
        let mut manifest_postimage = None;
        let persist_result = (|| -> Result<()> {
            let mut m = ah_core::Manifest::load(&cfg.join("manifest.json"))?;
            let previous = m.clone();
            m.shared_home = shared.to_string_lossy().into_owned();
            m.home_ref = home_ref.clone();
            m.profile = selected.into();
            m.vault = vault.as_ref().map(|v| v.to_string_lossy().into_owned());
            // Commit only after managed actions succeed; preserve any intervening owner edit.
            if !local_before.matches(&local_path)? {
                anyhow::bail!("local configuration changed before commit; preserving user changes");
            }
            reject_symlink_components(&cfg)?;
            persist_prepared_local(prepared_local.take().context("local configuration was not prepared")?, &local_path, &mut local_postimage, |path| Ok(std::fs::read(path)?))?;
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
            let expected_manifest = recovery::Node::File {
                bytes: serde_json::to_vec_pretty(&m)?,
                permissions: {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::Permissions::from_mode(0o100600)
                },
            };
            m.save_atomic(&manifest_path)?;
            manifest_postimage = Some(expected_manifest);
            if let Some(journal) = recovery_journal.as_mut() {
                before_seal()?;
                journal.seal()?;
            }
            Ok(())
        })();
        let cleanup_errors = if persist_result.is_err() {
            let mut errors = remove_install_backups(&created_backups, &pending_backups);
            if let Some(postimage) = &manifest_postimage {
                if let Err(error) = manifest_preimage.restore(&manifest_path, postimage) {
                    errors.push(format!("manifest rollback failed: {error:#}"));
                }
            }
            match restore_local_config(&local_path, local_preimage, local_postimage.as_ref()) {
                Ok(()) => {}
                Err(error) => errors.push(format!("{}: {error:#}", local_path.display())),
            }
            errors
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
    prune_stale_impl(
        &cfg,
        &plan,
        &home,
        dry_run,
        recovery_journal.as_mut(),
        |_, _| Ok(()),
    )?;
    if let Some(journal) = recovery_journal.as_mut() {
        journal.seal()?;
    }
    if dry_run {
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
fn persist_prepared_local<F>(
    prepared: tempfile::NamedTempFile,
    path: &Path,
    postimage: &mut Option<recovery::Node>,
    read_after: F,
) -> Result<()>
where
    F: FnOnce(&Path) -> Result<Vec<u8>>,
{
    let expected = recovery::Node::read(prepared.path())?;
    prepared
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("cannot safely persist {}", path.display()))?;
    *postimage = Some(expected);
    read_after(path)?;
    Ok(())
}

fn restore_local_config(
    path: &Path,
    preimage: Option<&(Vec<u8>, std::fs::Permissions)>,
    expected_current: Option<&recovery::Node>,
) -> Result<()> {
    let before = match preimage {
        Some((bytes, permissions)) => recovery::Node::File {
            bytes: bytes.clone(),
            permissions: permissions.clone(),
        },
        None => recovery::Node::Absent,
    };
    if let Some(expected) = expected_current {
        before.restore(path, expected)
    } else if before.matches(path)? {
        Ok(())
    } else {
        anyhow::bail!("local config changed during install; preserved")
    }
}

fn create_install_backups(pending: &[(PathBuf, Vec<u8>)]) -> Result<Vec<PathBuf>> {
    use std::io::Write;

    let mut created = Vec::new();
    for (path, contents) in pending {
        let write_result = (|| -> std::io::Result<()> {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(path)?;
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
        ah_core::Action::WriteFile { .. } => ("generated-file", None),
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
fn machine_root(r: Option<&Path>) -> Result<PathBuf> {
    let path = r
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .context("HOME is not set")?;
    let absolute = std::path::absolute(&path)
        .with_context(|| format!("cannot resolve machine root {}", path.display()))?;
    // Machine roots and their ancestors must be real directories, not aliases into another tree.
    reject_symlink_components(&absolute)?;
    Ok(absolute)
}
fn root(r: Option<&Path>) -> Result<PathBuf> {
    machine_root(r)
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
    // F11: informational only, never a blocker, and never a fetch.
    let local = ah_core::LocalConfig::load(&config_dir(r)?)?;
    if let (Some(pin), false) = (&local.home.reference, m.shared_home.is_empty()) {
        let shared = PathBuf::from(&m.shared_home);
        let detected = detected_ref(&shared);
        print!(
            "Pinned ref: {pin} (checkout is {})",
            detected.as_deref().unwrap_or("not a git checkout")
        );
        if let Some((count, newest)) = ah_core::version::behind(pin, &local_tags(&shared)) {
            println!(" — {count} releases behind ({pin} → {newest}), informational only");
        } else if ah_core::version::floats(pin) {
            println!(" — floating, so drift is not reported");
        } else {
            println!(" — up to date with the local tags");
        }
    }
    Ok(())
}
fn uninstall(r: Option<&Path>, dry: bool) -> Result<()> {
    uninstall_impl(r, dry, |_, _| Ok(()))
}

fn uninstall_impl<F>(r: Option<&Path>, dry: bool, before_mutation: F) -> Result<()>
where
    F: FnMut(usize, &Path) -> Result<()>,
{
    uninstall_with_postimage_reader(r, dry, before_mutation, recovery::Node::read)
}

fn uninstall_with_postimage_reader<F, P>(
    r: Option<&Path>,
    dry: bool,
    mut before_mutation: F,
    mut read_postimage: P,
) -> Result<()>
where
    F: FnMut(usize, &Path) -> Result<()>,
    P: FnMut(&Path) -> Result<recovery::Node>,
{
    use sha2::Digest;
    let mut mutation_count = 0;
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
    let machine_root = machine_root(r)?;
    let managed_root = machine_root.join(".config/agent-home");
    validate_manifest_scope(&m, &machine_root, &managed_root)?;
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
    // Inspect every target and compute restorations before the first mutation. Progress is saved
    // before removals so a later filesystem error never leaves stale ownership for completed work.
    let mut retained = std::collections::BTreeMap::new();
    for (key, entry) in m.entries.clone() {
        let target = Path::new(&entry.target);
        let preimage = recovery::Node::read(target)?;
        let mut expected_postimage = recovery::Node::Absent;
        let result = (|| -> Result<()> {
            match entry.kind.as_str() {
                "symlink" => {
                    match std::fs::symlink_metadata(target) {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            println!("Skipped missing managed target: {key}");
                            return Ok(());
                        }
                        Err(error) => return Err(error.into()),
                        Ok(metadata) if !metadata.file_type().is_symlink() => {
                            println!("Skipped replaced managed target: {key}");
                            retained.insert(key.clone(), entry.clone());
                            return Ok(());
                        }
                        Ok(_) => {}
                    }
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
                        println!("Skipped modified symlink: {key}");
                        retained.insert(key.clone(), entry.clone());
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
                            mutation_count += 1;
                            before_mutation(mutation_count, target)?;
                            if !preimage.matches(target)? {
                                anyhow::bail!("uninstall target changed before mutation; preserving user changes: {}", target.display());
                            }
                            std::fs::remove_file(target)?;
                        }
                    }
                }
                "generated-file" => {
                    let metadata = match std::fs::symlink_metadata(target) {
                        Ok(metadata) => metadata,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            println!("Skipped missing generated file: {key}");
                            return Ok(());
                        }
                        Err(error) => return Err(error.into()),
                    };
                    if !metadata.is_file() || metadata.file_type().is_symlink() {
                        println!("Skipped replaced generated file: {key}");
                        retained.insert(key.clone(), entry.clone());
                        return Ok(());
                    }
                    let current = std::fs::read(target)?;
                    let matches = entry
                        .installed_sha256
                        .as_deref()
                        .map(|expected| hex::encode(sha2::Sha256::digest(&current)) == expected)
                        .unwrap_or(false);
                    if !matches {
                        println!("Skipped modified generated file: {key}");
                        retained.insert(key.clone(), entry.clone());
                        return Ok(());
                    }
                    println!(
                        "{}: {key}",
                        if dry {
                            "Would remove generated file"
                        } else {
                            "Removing generated file"
                        }
                    );
                    if !dry {
                        mutation_count += 1;
                        before_mutation(mutation_count, target)?;
                        if !preimage.matches(target)? {
                            anyhow::bail!("uninstall target changed before mutation; preserving user changes: {}", target.display());
                        }
                        std::fs::remove_file(target)?;
                    }
                }
                "merged-file" => {
                    let metadata = match std::fs::symlink_metadata(target) {
                        Ok(metadata) => metadata,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            println!("Skipped missing managed target: {key}");
                            return Ok(());
                        }
                        Err(error) => return Err(error.into()),
                    };
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
                        println!("Skipped replaced managed target: {key}");
                        retained.insert(key.clone(), entry.clone());
                        return Ok(());
                    }
                    let current = std::fs::read(target)?;
                    let installed_matches = entry
                        .installed_sha256
                        .as_deref()
                        .map(|hash| hex::encode(sha2::Sha256::digest(&current)) == hash)
                        .unwrap_or(false);
                    if !installed_matches {
                        println!("Skipped modified merged file: {key}");
                        retained.insert(key.clone(), entry.clone());
                        return Ok(());
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
                                mutation_count += 1;
                                before_mutation(mutation_count, target)?;
                                if !preimage.matches(target)? {
                                    anyhow::bail!("uninstall target changed before mutation; preserving user changes: {}", target.display());
                                }
                                std::fs::remove_file(target)?;
                            }
                        }
                        Some(backup_path) => {
                            let Some(expected) = entry.original_sha256.as_deref() else {
                                println!("Skipped merged file without original hash: {key}");
                                retained.insert(key.clone(), entry.clone());
                                return Ok(());
                            };
                            let backup = match std::fs::read(backup_path) {
                                Ok(bytes) => bytes,
                                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                    println!("Skipped missing backup: {key}");
                                    retained.insert(key.clone(), entry.clone());
                                    return Ok(());
                                }
                                Err(error) => return Err(error.into()),
                            };
                            if hex::encode(sha2::Sha256::digest(&backup)) != expected {
                                println!("Skipped altered backup: {key}");
                                retained.insert(key.clone(), entry.clone());
                                return Ok(());
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
                                mutation_count += 1;
                                before_mutation(mutation_count, target)?;
                                if !preimage.matches(target)? {
                                    anyhow::bail!("uninstall target changed before mutation; preserving user changes: {}", target.display());
                                }
                                expected_postimage = recovery::Node::File {
                                    bytes: backup.clone(),
                                    permissions: match &preimage {
                                        recovery::Node::File { permissions, .. } => {
                                            permissions.clone()
                                        }
                                        _ => anyhow::bail!(
                                            "restoration preimage must be a regular file"
                                        ),
                                    },
                                };
                                ah_core::action::atomic_replace_file(target, &backup)?;
                            }
                        }
                    }
                }
                "managed-block" => {
                    let metadata = match std::fs::symlink_metadata(target) {
                        Ok(metadata) => metadata,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            println!("Skipped missing managed target: {key}");
                            return Ok(());
                        }
                        Err(error) => return Err(error.into()),
                    };
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
                        println!("Skipped replaced managed target: {key}");
                        retained.insert(key.clone(), entry.clone());
                        return Ok(());
                    }
                    let backup_path = entry.backup.as_deref().map(Path::new);
                    if backup_path.is_none() {
                        let current = std::fs::read_to_string(target)?;
                        let installed_matches = entry
                            .installed_sha256
                            .as_deref()
                            .map(|hash| {
                                hex::encode(sha2::Sha256::digest(current.as_bytes())) == hash
                            })
                            .unwrap_or(false);
                        if !installed_matches {
                            println!("Skipped modified managed file: {key}");
                            retained.insert(key.clone(), entry.clone());
                            return Ok(());
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
                            retained.insert(key.clone(), entry.clone());
                            return Ok(());
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
                            mutation_count += 1;
                            before_mutation(mutation_count, target)?;
                            if !preimage.matches(target)? {
                                anyhow::bail!("uninstall target changed before mutation; preserving user changes: {}", target.display());
                            }
                            std::fs::remove_file(target)?;
                        }
                        return Ok(());
                    }
                    let Some(expected_hash) = entry.original_sha256.as_deref() else {
                        println!("Skipped block without original hash: {key}");
                        retained.insert(key.clone(), entry.clone());
                        return Ok(());
                    };
                    let backup_path = backup_path.unwrap();
                    let metadata = match std::fs::symlink_metadata(backup_path) {
                        Ok(metadata) => metadata,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            println!("Skipped missing backup: {key}");
                            retained.insert(key.clone(), entry.clone());
                            return Ok(());
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
                        retained.insert(key.clone(), entry.clone());
                        return Ok(());
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
                        retained.insert(key.clone(), entry.clone());
                        return Ok(());
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
                        retained.insert(key.clone(), entry.clone());
                        return Ok(());
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
                        mutation_count += 1;
                        before_mutation(mutation_count, target)?;
                        if !preimage.matches(target)? {
                            anyhow::bail!("uninstall target changed before mutation; preserving user changes: {}", target.display());
                        }
                        expected_postimage = recovery::Node::File {
                            bytes: original.as_bytes().to_vec(),
                            permissions: match &preimage {
                                recovery::Node::File { permissions, .. } => permissions.clone(),
                                _ => anyhow::bail!("restoration preimage must be a regular file"),
                            },
                        };
                        ah_core::action::atomic_replace_file(target, original.as_bytes())?;
                    }
                }
                _ => {
                    let metadata = match std::fs::symlink_metadata(target) {
                        Ok(metadata) => metadata,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            println!("Skipped missing managed target: {key}");
                            return Ok(());
                        }
                        Err(error) => return Err(error.into()),
                    };
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
                        println!("Skipped replaced managed target: {key}");
                        retained.insert(key.clone(), entry.clone());
                        return Ok(());
                    }
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
                        retained.insert(key.clone(), entry.clone());
                    } else {
                        println!("{}: {key}", if dry { "Would remove" } else { "Removing" });
                        if !dry {
                            mutation_count += 1;
                            before_mutation(mutation_count, target)?;
                            if !preimage.matches(target)? {
                                anyhow::bail!("uninstall target changed before mutation; preserving user changes: {}", target.display());
                            }
                            std::fs::remove_file(target)?;
                        }
                    }
                }
            }
            Ok(())
        })();
        result?;
        if !dry && !retained.contains_key(&key) {
            let postimage = match read_postimage(target) {
                Ok(postimage) => postimage,
                Err(error) => {
                    if let Err(restore) = preimage.restore(target, &expected_postimage) {
                        anyhow::bail!("{error:#}; uninstall rollback failed: {restore:#}");
                    }
                    return Err(error);
                }
            };
            m.entries.remove(&key);
            if let Err(error) = m.save_atomic(&p) {
                if let Err(restore) = preimage.restore(target, &postimage) {
                    anyhow::bail!("{error:#}; uninstall rollback failed: {restore:#}");
                }
                return Err(error);
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
    fn uninstall_restoration_postimage_read_failure_rolls_back_current_entry() {
        use super::*;
        use sha2::Digest;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let cfg = root.join(".config/agent-home");
        std::fs::create_dir_all(&cfg).unwrap();
        let target = root.join("a");
        let backup = cfg.join("a.bak");
        std::fs::write(&target, b"installed").unwrap();
        std::fs::write(&backup, b"original").unwrap();
        let before = recovery::Node::read(&target).unwrap();
        let mut manifest = ah_core::Manifest::default();
        manifest.entries.insert(
            target.display().to_string(),
            ah_core::ManifestEntry {
                kind: "merged-file".into(),
                target: target.display().to_string(),
                source: None,
                backup: Some(backup.display().to_string()),
                original_sha256: Some(hex::encode(sha2::Sha256::digest(b"original"))),
                installed_sha256: Some(hex::encode(sha2::Sha256::digest(b"installed"))),
            },
        );
        let mp = cfg.join("manifest.json");
        manifest.save_atomic(&mp).unwrap();
        let old_manifest = std::fs::read(&mp).unwrap();
        let error = uninstall_with_postimage_reader(
            Some(&root),
            false,
            |_, _| Ok(()),
            |_| anyhow::bail!("injected uninstall postimage read failure"),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("injected uninstall postimage read failure"));
        assert!(
            before.matches(&target).unwrap(),
            "failed restoration verification must not leave stale installed ownership"
        );
        assert_eq!(std::fs::read(&mp).unwrap(), old_manifest);
        assert_eq!(std::fs::read(&backup).unwrap(), b"original");
        uninstall(Some(&root), false).unwrap();
        uninstall(Some(&root), false).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
    }

    #[test]
    fn local_postimage_read_failure_roundtrips_present_and_absent_configuration() {
        use super::*;
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        for present in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("config.toml");
            let preimage = if present {
                std::fs::write(&path, b"schema = 1\n# original\n").unwrap();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
                Some((
                    std::fs::read(&path).unwrap(),
                    std::fs::metadata(&path).unwrap().permissions(),
                ))
            } else {
                None
            };
            let before = recovery::Node::read(&path).unwrap();
            let mut prepared = tempfile::NamedTempFile::new_in(temp.path()).unwrap();
            prepared.write_all(b"schema = 1\n# committed\n").unwrap();
            let mut postimage = None;
            let error = persist_prepared_local(prepared, &path, &mut postimage, |_| {
                anyhow::bail!("injected local postimage read failure")
            })
            .unwrap_err();
            assert!(format!("{error:#}").contains("injected local postimage read failure"));
            let restore = restore_local_config(&path, preimage.as_ref(), postimage.as_ref());
            assert!(
                restore.is_ok(),
                "own local persist must remain recoverable: {restore:?}"
            );
            assert!(
                before.matches(&path).unwrap(),
                "local configuration must roundtrip exactly"
            );
        }
    }

    #[test]
    fn binary_postimage_read_failure_retains_rollback_ownership() {
        use super::*;
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("vesica");
        let candidate = temp.path().join("candidate");
        std::fs::write(&target, b"old executable").unwrap();
        std::fs::write(&candidate, b"new executable").unwrap();
        let previous = recovery::Node::read(&target).unwrap();
        let mut installed = None;
        let error = replace_update_binary(&target, &candidate, &mut installed, |_| {
            anyhow::bail!("injected binary postimage read failure")
        })
        .unwrap_err();
        assert!(format!("{error:#}").contains("injected binary postimage read failure"));
        if let Some(expected) = installed.as_ref() {
            previous.restore(&target, expected).unwrap();
        }
        assert!(
            previous.matches(&target).unwrap(),
            "failed postreplacement read must not lose binary rollback ownership"
        );
    }

    #[test]
    fn prune_checkpoint_failure_undoes_own_deletion_before_return() {
        use super::*;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let cfg = root.join(".config/agent-home");
        std::fs::create_dir_all(&cfg).unwrap();
        let target = root.join("stale-link");
        std::os::unix::fs::symlink(temp.path(), &target).unwrap();
        let mut manifest = ah_core::Manifest::default();
        manifest.entries.insert(
            target.display().to_string(),
            ah_core::ManifestEntry {
                kind: "symlink".into(),
                target: target.display().to_string(),
                source: Some(temp.path().display().to_string()),
                backup: None,
                original_sha256: None,
                installed_sha256: None,
            },
        );
        let mp = cfg.join("manifest.json");
        manifest.save_atomic(&mp).unwrap();
        let before = std::fs::read(&mp).unwrap();
        let jp = temp.path().join("transaction.json");
        let mut journal =
            recovery::Journal::prepare(&jp, vec![target.clone(), mp.clone()]).unwrap();
        journal.seal().unwrap();
        let plan = ah_core::Planner::new().finish(false);
        prune_stale_impl(&cfg, &plan, &root, false, Some(&mut journal), |_, _| {
            std::fs::remove_file(&jp)?;
            std::fs::create_dir(&jp)?; // deterministic journal persistence failure
            Ok(())
        })
        .unwrap_err();
        assert_eq!(
            std::fs::read_link(&target).unwrap(),
            temp.path(),
            "own deletion must be reversed before returning failed checkpoint"
        );
        assert_eq!(std::fs::read(&mp).unwrap(), before);
    }

    #[test]
    fn partial_stale_pruning_restores_owned_deletions() {
        use super::*;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let cfg = root.join(".config/agent-home");
        std::fs::create_dir_all(&cfg).unwrap();
        let mut manifest = ah_core::Manifest::default();
        for name in ["a", "b"] {
            let target = root.join(name);
            std::os::unix::fs::symlink(temp.path(), &target).unwrap();
            manifest.entries.insert(
                target.display().to_string(),
                ah_core::ManifestEntry {
                    kind: "symlink".into(),
                    target: target.display().to_string(),
                    source: Some(temp.path().display().to_string()),
                    backup: None,
                    original_sha256: None,
                    installed_sha256: None,
                },
            );
        }
        let mp = cfg.join("manifest.json");
        manifest.save_atomic(&mp).unwrap();
        let before = std::fs::read(&mp).unwrap();
        let jp = temp.path().join("transaction.json");
        let mut journal =
            recovery::Journal::prepare(&jp, vec![root.join("a"), root.join("b"), mp.clone()])
                .unwrap();
        journal.seal().unwrap();
        let plan = ah_core::Planner::new().finish(false);
        let error = prune_stale_impl(&cfg, &plan, &root, false, Some(&mut journal), |i, _| {
            if i == 2 {
                anyhow::bail!("injected second prune failure");
            }
            Ok(())
        })
        .unwrap_err();
        assert!(format!("{error:#}").contains("injected second prune failure"));
        recovery::Journal::rollback_file(&jp).unwrap();
        for name in ["a", "b"] {
            assert_eq!(std::fs::read_link(root.join(name)).unwrap(), temp.path());
        }
        assert_eq!(std::fs::read(&mp).unwrap(), before);
    }

    #[test]
    fn first_seal_failure_restores_manifest_and_all_managed_nodes() {
        use super::*;
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let shared = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home");
        let options = || InstallOptions {
            home_repo: Some(shared.clone()),
            root: Some(home.clone()),
            profile: Some(ProfileArg::Core),
            dry_run: false,
            repos_dir: None,
            vault: None,
        };
        install(options()).unwrap();
        let cfg = home.join(".config/agent-home");
        let mp = cfg.join("manifest.json");
        let mut old = ah_core::Manifest::load(&mp).unwrap();
        old.profile = "lead".into(); // ensure the new commit differs from the preimage
        old.save_atomic(&mp).unwrap();
        let before = std::fs::read(&mp).unwrap();
        let local_before = std::fs::read(cfg.join("config.toml")).unwrap();
        let jp = temp.path().join("transaction.json");
        let error = install_impl(options(), Some(&jp), || {
            anyhow::bail!("injected first journal seal failure")
        })
        .unwrap_err();
        assert!(format!("{error:#}").contains("injected first journal seal failure"));
        assert_eq!(
            std::fs::read(&mp).unwrap(),
            before,
            "manifest committed before failed seal must roll back"
        );
        assert_eq!(
            std::fs::read(cfg.join("config.toml")).unwrap(),
            local_before
        );
        recovery::Journal::rollback_file(&jp).unwrap();
    }

    #[test]
    fn checkpoint_load_failure_restores_managed_state_and_retains_evidence() {
        use super::*;
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let cfg = root.join(".config/agent-home");
        std::fs::create_dir_all(&cfg).unwrap();
        let mp = cfg.join("manifest.json");
        ah_core::Manifest {
            shared_home: temp.path().join("shared").display().to_string(),
            ..Default::default()
        }
        .save_atomic(&mp)
        .unwrap();
        let target = root.join("managed.txt");
        std::fs::write(&target, b"before").unwrap();
        let script = temp.path().join("candidate");
        std::fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let options = UpdateOptions {
            tag: Some("v9.9.9".into()),
            channel: ChannelArg::Alpha,
            target: Some(script.clone()),
            root: Some(root),
            dry_run: false,
            repos_dir: None,
        };
        let mut retained = None;
        let error = relink_with_checkpoint_loader(
            &options,
            &script,
            |dir| {
                let mut journal = recovery::Journal::prepare(
                    &dir.join("transaction.json"),
                    vec![target.clone()],
                )?;
                std::fs::write(&target, b"installed")?;
                journal.seal()?;
                retained = Some(dir.to_owned());
                Ok(())
            },
            |dir| std::fs::remove_dir_all(dir),
            |_| anyhow::bail!("injected checkpoint load failure"),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("injected checkpoint load failure"));
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"before",
            "unreadable checkpoint must not silently commit migration"
        );
        assert!(
            retained.unwrap().join("transaction.json").is_file(),
            "reported recovery evidence must exist"
        );
    }

    #[test]
    fn partial_cleanup_without_journal_restores_managed_state_and_recreates_evidence() {
        use super::*;
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let cfg = root.join(".config/agent-home");
        std::fs::create_dir_all(&cfg).unwrap();
        let mp = cfg.join("manifest.json");
        ah_core::Manifest {
            shared_home: temp.path().join("shared").display().to_string(),
            ..Default::default()
        }
        .save_atomic(&mp)
        .unwrap();
        let target = root.join("managed.txt");
        std::fs::write(&target, b"before").unwrap();
        let script = temp.path().join("candidate");
        std::fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let options = UpdateOptions {
            tag: Some("v9.9.9".into()),
            channel: ChannelArg::Alpha,
            target: Some(script.clone()),
            root: Some(root),
            dry_run: false,
            repos_dir: None,
        };
        let mut retained = None;
        let error = relink_with_cleanup(
            &options,
            &script,
            |dir| {
                let mut journal = recovery::Journal::prepare(
                    &dir.join("transaction.json"),
                    vec![target.clone()],
                )?;
                std::fs::write(&target, b"installed")?;
                journal.seal()?;
                retained = Some(dir.to_owned());
                Ok(())
            },
            |dir| {
                std::fs::remove_file(dir.join("transaction.json"))?;
                Err(std::io::Error::other(
                    "injected failure after journal unlink",
                ))
            },
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("injected failure after journal unlink"));
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"before",
            "lost journal must not silently commit migration"
        );
        assert!(
            retained.unwrap().join("transaction.json").is_file(),
            "reported recovery evidence must exist"
        );
    }

    #[test]
    fn relink_cleanup_failure_restores_managed_preimage_and_retains_journal() {
        use super::*;
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let cfg = root.join(".config/agent-home");
        std::fs::create_dir_all(&cfg).unwrap();
        let mp = cfg.join("manifest.json");
        let manifest = ah_core::Manifest {
            shared_home: temp.path().join("shared").display().to_string(),
            ..Default::default()
        };
        manifest.save_atomic(&mp).unwrap();
        let target = root.join("managed.txt");
        std::fs::write(&target, b"before").unwrap();
        let script = temp.path().join("candidate");
        std::fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let options = UpdateOptions {
            tag: Some("v9.9.9".into()),
            channel: ChannelArg::Alpha,
            target: Some(script.clone()),
            root: Some(root),
            dry_run: false,
            repos_dir: None,
        };
        let mut retained = None;
        let error = relink_impl(&options, &script, |dir| {
            let mut journal =
                recovery::Journal::prepare(&dir.join("transaction.json"), vec![target.clone()])?;
            std::fs::write(&target, b"installed")?;
            journal.seal()?;
            retained = Some(dir.to_owned());
            anyhow::bail!("injected cleanup failure");
        })
        .unwrap_err();
        assert!(format!("{error:#}").contains("injected cleanup failure"));
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"before",
            "cleanup failure must not commit managed migration"
        );
        assert!(retained.unwrap().join("transaction.json").is_file());
    }

    #[test]
    fn uninstall_failure_position_matrix_retains_exact_pending_state_and_retries() {
        use super::*;
        use sha2::Digest;
        for failure in [1, 2, 3] {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path().join("root");
            let cfg = root.join(".config/agent-home");
            std::fs::create_dir_all(&cfg).unwrap();
            let mut manifest = ah_core::Manifest::default();
            for name in ["a", "b", "c"] {
                let target = root.join(name);
                std::fs::write(&target, name).unwrap();
                manifest.entries.insert(
                    target.display().to_string(),
                    ah_core::ManifestEntry {
                        kind: "generated-file".into(),
                        target: target.display().to_string(),
                        source: None,
                        backup: None,
                        original_sha256: None,
                        installed_sha256: Some(hex::encode(sha2::Sha256::digest(name.as_bytes()))),
                    },
                );
            }
            let mp = cfg.join("manifest.json");
            manifest.save_atomic(&mp).unwrap();
            uninstall_impl(Some(&root), false, |i, _| {
                if i == failure {
                    anyhow::bail!("injected read/write error");
                }
                Ok(())
            })
            .unwrap_err();
            let state = ah_core::Manifest::load(&mp).unwrap();
            assert_eq!(state.entries.len(), 4 - failure);
            for (index, name) in ["a", "b", "c"].iter().enumerate() {
                if index + 1 < failure {
                    assert!(!root.join(name).exists());
                } else {
                    assert_eq!(std::fs::read(root.join(name)).unwrap(), name.as_bytes());
                }
            }
            uninstall(Some(&root), false).unwrap();
            uninstall(Some(&root), false).unwrap();
            assert!(!mp.exists());
        }
    }

    #[test]
    fn uninstall_preserves_intervening_target_replacement_before_removal() {
        use super::*;
        use sha2::Digest;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let cfg = root.join(".config/agent-home");
        std::fs::create_dir_all(&cfg).unwrap();
        let target = root.join("a");
        std::fs::write(&target, b"installed").unwrap();
        let mut manifest = ah_core::Manifest::default();
        manifest.entries.insert(
            target.display().to_string(),
            ah_core::ManifestEntry {
                kind: "generated-file".into(),
                target: target.display().to_string(),
                source: None,
                backup: None,
                original_sha256: None,
                installed_sha256: Some(hex::encode(sha2::Sha256::digest(b"installed"))),
            },
        );
        let mp = cfg.join("manifest.json");
        manifest.save_atomic(&mp).unwrap();
        let original_manifest = std::fs::read(&mp).unwrap();
        let result = uninstall_impl(Some(&root), false, |_, path| {
            std::fs::write(path, b"intervening edit")?;
            Ok(())
        });
        assert!(
            result.is_err(),
            "must refuse replacement at mutation boundary"
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"intervening edit");
        assert_eq!(std::fs::read(&mp).unwrap(), original_manifest);
    }

    #[test]
    fn later_uninstall_mutation_failure_preserves_accurate_resumable_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let cfg = root.join(".config/agent-home");
        std::fs::create_dir_all(&cfg).unwrap();
        let mut manifest = ah_core::Manifest::default();
        use sha2::Digest;
        for name in ["a.txt", "b.txt", "c.txt"] {
            let path = root.join(name);
            std::fs::write(&path, name).unwrap();
            manifest.entries.insert(
                path.display().to_string(),
                ah_core::ManifestEntry {
                    kind: "generated-file".into(),
                    target: path.display().to_string(),
                    source: None,
                    backup: None,
                    original_sha256: None,
                    installed_sha256: Some(hex::encode(sha2::Sha256::digest(name.as_bytes()))),
                },
            );
        }
        let mp = cfg.join("manifest.json");
        manifest.save_atomic(&mp).unwrap();
        let error = super::uninstall_impl(Some(&root), false, |i, _| {
            if i == 2 {
                anyhow::bail!("injected second mutation failure");
            }
            Ok(())
        })
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("injected second mutation failure"));
        let remaining = ah_core::Manifest::load(&mp).unwrap();
        assert_eq!(
            remaining.entries.len(),
            2,
            "completed target must not retain stale ownership"
        );
        assert!(!remaining
            .entries
            .contains_key(root.join("a.txt").to_str().unwrap()));
        assert!(!root.join("a.txt").exists());
        assert_eq!(std::fs::read(root.join("b.txt")).unwrap(), b"b.txt");
        super::uninstall(Some(&root), false).unwrap();
        super::uninstall(Some(&root), false).unwrap();
        assert!(!mp.exists());
    }

    #[test]
    fn uninstall_later_progress_persistence_failure_restores_current_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let cfg = root.join(".config/agent-home");
        std::fs::create_dir_all(&cfg).unwrap();
        let mut manifest = ah_core::Manifest::default();
        use sha2::Digest;
        for name in ["a.txt", "b.txt"] {
            let path = root.join(name);
            std::fs::write(&path, name).unwrap();
            manifest.entries.insert(
                path.display().to_string(),
                ah_core::ManifestEntry {
                    kind: "generated-file".into(),
                    target: path.display().to_string(),
                    source: None,
                    backup: None,
                    original_sha256: None,
                    installed_sha256: Some(hex::encode(sha2::Sha256::digest(name.as_bytes()))),
                },
            );
        }
        let mp = cfg.join("manifest.json");
        manifest.save_atomic(&mp).unwrap();
        let error = super::uninstall_impl(Some(&root), false, |i, _| {
            if i == 2 {
                std::fs::create_dir(cfg.join("manifest.json.tmp"))?;
            }
            Ok(())
        })
        .unwrap_err();
        assert!(format!("{error:#}").contains("manifest temp file"));
        assert_eq!(
            std::fs::read(root.join("b.txt")).unwrap(),
            b"b.txt",
            "failed progress commit must restore its mutation"
        );
        let remaining = ah_core::Manifest::load(&mp).unwrap();
        assert_eq!(remaining.entries.len(), 1);
        std::fs::remove_dir(cfg.join("manifest.json.tmp")).unwrap();
        super::uninstall(Some(&root), false).unwrap();
        assert!(!mp.exists());
    }

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
