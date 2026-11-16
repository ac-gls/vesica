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
    Doctor(Options),
    Install(InstallOptions),
    Status(RootOptions),
    Uninstall(UninstallOptions),
    VaultScan(VaultScanOptions),
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
    #[arg(long, env = "AGENT_HOME_REPO")]
    home_repo: PathBuf,
    #[arg(long, value_enum)]
    profile: Option<ProfileArg>,
}
#[derive(Debug, clap::Args)]
struct InstallOptions {
    #[command(flatten)]
    options: Options,
    #[arg(long, env = "AGENT_HOME_ROOT")]
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
struct RootOptions {
    #[arg(long)]
    root: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct UninstallOptions {
    #[arg(long, env = "AGENT_HOME_ROOT")]
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
        Command::VaultScan(o) => vault_scan(&o.path),
    }
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
fn doctor(o: &Options) -> Result<()> {
    let (c, f) = load(o)?;
    println!(
        "Shared home: valid
Team: {} ({})
Profile: {}
Repos in config: {}
Foundry deployments: {}",
        c.team.name,
        c.team.org,
        prof(o, &c),
        c.repos.len(),
        f.deployments.len()
    );
    let wsl = ah_wsl::is_wsl()?;
    println!(
        "WSL kernel: {}",
        if wsl { "detected" } else { "not detected" }
    );
    println!("Live harness, Foundry, Windows and Herdr checks are not implemented.");
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
    let plan = planner.finish(o.dry_run);
    println!(
        "{} install plan (profile: {selected})",
        if o.dry_run {
            "DRY RUN"
        } else {
            "Applying supported actions"
        }
    );
    // Check all destinations before creating backups or executing any action.
    if !o.dry_run {
        let existing = ah_core::Manifest::load(&cfg.join("manifest.json"))?;
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
    let backups = if o.dry_run {
        std::collections::BTreeMap::new()
    } else {
        let old = ah_core::Manifest::load(&cfg.join("manifest.json"))?;
        let mut result = std::collections::BTreeMap::new();
        for action in &plan.actions {
            if let Some(existing) = old.entries.get(action.key()) {
                if let Some(path) = existing.backup.as_deref() {
                    result.insert(action.key().to_owned(), PathBuf::from(path));
                    continue;
                }
            }
            if matches!(action, ah_core::Action::UpsertBlock { .. }) {
                let target = Path::new(action.key());
                match std::fs::read(target) {
                    Ok(bytes) => {
                        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.fZ");
                        let backup = target.with_file_name(format!(
                            "{}.vesica-backup-{stamp}",
                            target.file_name().unwrap_or_default().to_string_lossy()
                        ));
                        std::fs::write(&backup, bytes)?;
                        result.insert(action.key().to_owned(), backup);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        result
    };
    let outcomes = ah_core::action::Executor.execute(&plan)?;
    if outcomes.contains(&ah_core::ActionOutcome::Collision) {
        anyhow::bail!("install stopped on an existing destination; no changes should be treated as fully applied");
    }
    for (a, r) in plan.actions.iter().zip(&outcomes) {
        println!("{r:?}: {}", a.key())
    }
    if o.dry_run {
        return Ok(());
    }
    let mut m = ah_core::Manifest::load(&cfg.join("manifest.json"))?;
    let previous = m.clone();
    m.shared_home = o.options.home_repo.to_string_lossy().into_owned();
    m.profile = selected.into();
    for (a, r) in plan.actions.iter().zip(&outcomes) {
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
    println!("Manifest recorded: {}", cfg.join("manifest.json").display());
    println!("Partial wiring: Claude and Codex only; other harnesses are not yet wired.");
    Ok(())
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
    let mut m = ah_core::Manifest::load(&p)?;
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
