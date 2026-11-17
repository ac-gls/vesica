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
        help = "Local team shared-home checkout"
    )]
    home_repo: PathBuf,
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

fn load(o: &Options) -> Result<(ah_core::AgentHomeConfig, ah_core::FoundryCatalog)> {
    let p = o.home_repo.join("agent-home.toml");
    let c = ah_core::AgentHomeConfig::from_path(&p)
        .with_context(|| format!("cannot load shared-home configuration {}", p.display()))?;
    let p = o.home_repo.join("foundry.catalog.json");
    let f = if p.exists() {
        ah_core::FoundryCatalog::from_path(&p)?
    } else {
        ah_core::FoundryCatalog::default()
    };
    Ok((c, f))
}
fn prof(o: &Options, c: &ah_core::AgentHomeConfig) -> &'static str {
    o.profile
        .map(ProfileArg::text)
        .unwrap_or_else(|| match c.profiles.default {
            ah_core::Profile::Core => "core",
            ah_core::Profile::Developer => "developer",
            ah_core::Profile::Lead => "lead",
        })
}
fn doctor(o: &DoctorOptions) -> Result<()> {
    let (c, f) = load(&o.options)?;
    let wsl = ah_wsl::is_wsl()?;
    let wsl_status = if wsl { "detected" } else { "not detected" };
    let selected = prof(&o.options, &c);
    let selected_profile = ah_agents::ProfileSelection::from_name(&c, selected)?;
    if o.json {
        let report = serde_json::json!({
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
            "Shared home: valid
Team: {} ({})
Profile: {}
Repos in config: {}
Repos for profile: {}\nFoundry deployments: {}",
            c.team.name,
            c.team.org,
            selected,
            c.repos.len(),
            selected_profile.repositories.len(),
            f.deployments.len()
        );
        println!("WSL kernel: {wsl_status}");
        println!("Live harness, Foundry, Windows and Herdr checks are not implemented.");
    }
    Ok(())
}
fn install(o: InstallOptions) -> Result<()> {
    let (c, _) = load(&o.options)?;
    let selected = prof(&o.options, &c);
    let home = match o.root {
        Some(p) => p,
        None => std::env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is not set")?,
    };
    let cfg = home.join(".config/agent-home");
    let shared = o.options.home_repo.canonicalize().with_context(|| {
        format!(
            "shared-home path does not exist: {}",
            o.options.home_repo.display()
        )
    })?;
    if !shared.join("AGENTS.md").is_file() {
        anyhow::bail!("shared home must contain AGENTS.md: {}", shared.display());
    }
    let import = format!("@{}", shared.join("AGENTS.md").display());
    let profile = ah_agents::ProfileSelection::from_name(&c, selected)?;
    let actions = vec![
        ah_core::Action::Symlink {
            source: shared.to_string_lossy().into_owned(),
            destination: home.join("Team").to_string_lossy().into_owned(),
        },
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
    ];
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
                    "managed-block" => {
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
            }
            if matches!(action, ah_core::Action::UpsertBlock { .. }) {
                let target = Path::new(action.key());
                match std::fs::read(target) {
                    Ok(bytes) => {
                        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.fZ");
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
            m.shared_home = o.options.home_repo.to_string_lossy().into_owned();
            m.profile = selected.into();
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
    if o.dry_run {
        return Ok(());
    }
    println!("Manifest recorded: {}", cfg.join("manifest.json").display());
    println!("Repository cloning is not implemented; listed repositories are informational only.");
    println!("Partial wiring: Claude and Codex only; other harnesses are not yet wired.");
    Ok(())
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
                let cleaned = ah_config::block::remove(
                    &current,
                    ah_config::block::CommentStyle::Hash,
                    "vesica",
                );
                let cleaned = if cleaned == original {
                    cleaned
                } else {
                    ah_config::block::remove(
                        &current,
                        ah_config::block::CommentStyle::Hash,
                        "agent-home",
                    )
                };
                if cleaned != original {
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
