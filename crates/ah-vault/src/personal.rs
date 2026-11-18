//! Personal vault primitives (plan §5.1 skeleton, §5.6 vault contract, D30).
//!
//! Every operation is conservative: nothing that already exists is overwritten, `Team/` (the
//! shared home) is never read for upkeep or included in a snapshot, and diagnostics never echo
//! note content.

use anyhow::Context;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const AGENTS: &str = include_str!("../../../assets/vault-skeleton/personal-AGENTS.md");
const CLAUDE: &str = include_str!("../../../assets/vault-skeleton/CLAUDE.md");
const README: &str = include_str!("../../../assets/vault-skeleton/README.md");
const GITIGNORE: &str = include_str!("../../../assets/vault-skeleton/gitignore");
const GITATTRIBUTES: &str = include_str!("../../../assets/vault-skeleton/gitattributes");
const LEDGER: &str = include_str!("../../../assets/templates/ledger-note.md");

const DIRECTORIES: &[&str] = &[
    "Inbox",
    "Projects",
    "Decisions",
    "Calendar",
    "Archive",
    "Sessions/agent-runs",
];

/// Allowed note `type` values (plan §5.6).
pub const NOTE_TYPES: &[&str] = &[
    "standup-report",
    "report",
    "guide",
    "checklist",
    "readme",
    "template",
    "status",
    "audit",
    "workflow",
    "troubleshooting",
    "session-log",
    "note",
    "daily-note",
];
const REQUIRED_FIELDS: &[&str] = &["title", "created", "updated", "type", "area", "tags"];
/// Not subject to vault upkeep (plan §5.6): agent instruction files and the shared home.
const EXEMPT_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md"];
const SKIPPED_DIRS: &[&str] = &[".git", ".obsidian", ".trash", "Team"];

fn skeleton_files(date: &str) -> Vec<(&'static str, String)> {
    vec![
        ("AGENTS.md", AGENTS.to_owned()),
        ("CLAUDE.md", CLAUDE.to_owned()),
        ("README.md", README.replace("{{date}}", date)),
        (".gitignore", GITIGNORE.to_owned()),
        (".gitattributes", GITATTRIBUTES.to_owned()),
    ]
}

/// One planned or applied change, printed by the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultChange {
    Create(String),
    Kept(String),
    ExcludeTeam,
    GitInit,
}

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn is_git_root(vault: &Path) -> bool {
    exists(&vault.join(".git"))
}

/// Create a new vault from the skeleton. Refuses unless `vault` is absent or an empty directory.
pub fn init(vault: &Path, date: &str, dry_run: bool) -> anyhow::Result<Vec<VaultChange>> {
    if exists(vault) {
        let empty = vault.is_dir() && std::fs::read_dir(vault)?.next().is_none();
        if !empty {
            anyhow::bail!(
                "{} already exists and is not empty; use `vesica vault fix --path {}` to adopt an existing folder",
                vault.display(),
                vault.display()
            );
        }
    }
    let mut changes: Vec<VaultChange> = skeleton_files(date)
        .into_iter()
        .map(|(name, _)| VaultChange::Create(name.to_owned()))
        .chain(
            DIRECTORIES
                .iter()
                .map(|d| VaultChange::Create(format!("{d}/"))),
        )
        .collect();
    changes.push(VaultChange::GitInit);
    if !dry_run {
        std::fs::create_dir_all(vault)
            .with_context(|| format!("cannot create {}", vault.display()))?;
        apply(vault, date, &changes)?;
    }
    Ok(changes)
}

/// Adopt an existing folder: add only what is missing, never overwrite, and make sure `/Team`
/// is excluded from the vault repository.
pub fn fix(vault: &Path, date: &str, dry_run: bool) -> anyhow::Result<Vec<VaultChange>> {
    if !vault.is_dir() {
        anyhow::bail!(
            "{} is not a directory; use `vesica vault init` to create a vault",
            vault.display()
        );
    }
    let mut changes = Vec::new();
    for (name, _) in skeleton_files(date) {
        changes.push(if exists(&vault.join(name)) {
            VaultChange::Kept(name.to_owned())
        } else {
            VaultChange::Create(name.to_owned())
        });
    }
    for dir in DIRECTORIES {
        if !exists(&vault.join(dir)) {
            changes.push(VaultChange::Create(format!("{dir}/")));
        }
    }
    let gitignore = vault.join(".gitignore");
    if exists(&gitignore) && !excludes_team(&std::fs::read_to_string(&gitignore)?) {
        changes.push(VaultChange::ExcludeTeam);
    }
    if !is_git_root(vault) {
        changes.push(VaultChange::GitInit);
    }
    if !dry_run {
        apply(vault, date, &changes)?;
    }
    Ok(changes)
}

/// `/Team` or `Team` matches the shared home whether it is a directory or a symlink;
/// `Team/` alone would miss a symlink.
fn excludes_team(gitignore: &str) -> bool {
    gitignore
        .lines()
        .map(str::trim)
        .any(|line| line == "/Team" || line == "Team")
}

fn apply(vault: &Path, date: &str, changes: &[VaultChange]) -> anyhow::Result<()> {
    let files = skeleton_files(date);
    for change in changes {
        match change {
            VaultChange::Create(name) if name.ends_with('/') => {
                std::fs::create_dir_all(vault.join(name))?
            }
            VaultChange::Create(name) => {
                let (_, content) = files
                    .iter()
                    .find(|(file, _)| file == name)
                    .context("unknown skeleton file")?;
                write_new(&vault.join(name), content.as_bytes())?;
            }
            VaultChange::ExcludeTeam => {
                let path = vault.join(".gitignore");
                let mut text = std::fs::read_to_string(&path)?;
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str("/Team\n");
                std::fs::write(&path, text)?;
            }
            VaultChange::GitInit => {
                git_isolated(vault, &["init", "-q", "--initial-branch=main"])?;
            }
            VaultChange::Kept(_) => {}
        }
    }
    Ok(())
}

/// Create a file that must not exist yet (also refuses to follow a dangling symlink).
fn write_new(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("{} already exists or cannot be created", path.display()))?;
    file.write_all(bytes)?;
    Ok(())
}

/// A secret-safe vault contract diagnostic; `path` is relative to the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultFinding {
    pub rule: &'static str,
    pub path: String,
    pub line: Option<usize>,
    pub message: &'static str,
}

/// Read-only vault contract check (plan §5.6).
pub fn check(vault: &Path) -> anyhow::Result<Vec<VaultFinding>> {
    if !vault.is_dir() {
        anyhow::bail!("{} is not a directory", vault.display());
    }
    let mut findings = Vec::new();
    let structural = |rule, path: &str, message| VaultFinding {
        rule,
        path: path.to_owned(),
        line: None,
        message,
    };
    for required in ["AGENTS.md", "CLAUDE.md", "README.md", "Sessions/agent-runs"] {
        if !exists(&vault.join(required)) {
            findings.push(structural(
                "VVC001",
                required,
                "required vault entry is missing (run `vesica vault fix`)",
            ));
        }
    }
    if is_git_root(vault) {
        if let Some(problem) = team_exclusion_problem(vault)? {
            findings.push(structural("VVC002", "Team", problem));
        }
    } else {
        findings.push(structural(
            "VVC003",
            ".",
            "vault is not a Git repository (run `vesica vault fix`)",
        ));
    }
    for note in notes(vault)? {
        let rel = note
            .strip_prefix(vault)
            .unwrap_or(&note)
            .to_string_lossy()
            .into_owned();
        let Ok(text) = std::fs::read_to_string(&note) else {
            continue;
        };
        if let Some((rule, line, message)) = frontmatter_problem(&text) {
            findings.push(VaultFinding {
                rule,
                path: rel,
                line: Some(line),
                message,
            });
        }
    }
    Ok(findings)
}

/// `Team` must be ignored by the vault's own `.gitignore` and must not be tracked.
fn team_exclusion_problem(vault: &Path) -> anyhow::Result<Option<&'static str>> {
    if !git_isolated(vault, &["ls-files", "--", "Team"])?.is_empty() {
        return Ok(Some(
            "Team is tracked by the vault repository; remove it with `git rm -r --cached Team`",
        ));
    }
    // `Team` is checked as it exists on disk (git sees a symlink as a file, so `Team/` misses it);
    // when absent it is checked as a file, which only `/Team` or `Team` covers.
    {
        let path = "Team";
        let ignored = Command::new("git")
            .arg("-C")
            .arg(vault)
            .args(["check-ignore", "-q", "--no-index", "--", path])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success();
        if !ignored {
            return Ok(Some(
                "Team is not excluded by .gitignore; add a line `/Team` (run `vesica vault fix`)",
            ));
        }
    }
    Ok(None)
}

fn notes(vault: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut notes = Vec::new();
    let walker = walkdir::WalkDir::new(vault)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            !(entry.depth() == 1
                && entry.file_type().is_dir()
                && SKIPPED_DIRS.iter().any(|d| entry.file_name() == *d))
                && !(entry.file_type().is_dir()
                    && [".git", ".obsidian", ".trash"]
                        .iter()
                        .any(|d| entry.file_name() == *d))
        });
    for entry in walker {
        let entry = entry?;
        let is_note = entry.file_type().is_file()
            && entry.path().extension().is_some_and(|e| e == "md")
            && !(entry.depth() == 1 && EXEMPT_FILES.iter().any(|f| entry.file_name() == *f));
        if is_note {
            notes.push(entry.into_path());
        }
    }
    notes.sort();
    Ok(notes)
}

/// The YAML lines of a note's frontmatter block, or `None` when it is missing or unclosed.
pub fn frontmatter_of(text: &str) -> Option<Vec<&str>> {
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return None;
    }
    let mut yaml = Vec::new();
    for line in lines {
        if line.trim_end() == "---" {
            return Some(yaml);
        }
        yaml.push(line);
    }
    None
}

fn frontmatter_problem(text: &str) -> Option<(&'static str, usize, &'static str)> {
    use yaml_rust2::Yaml;
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Some(("VVC005", 1, "YAML frontmatter is missing"));
    }
    let mut yaml = Vec::new();
    let mut closed = false;
    for line in lines {
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        yaml.push(line);
    }
    if !closed {
        return Some(("VVC005", 1, "YAML frontmatter is not closed"));
    }
    let Ok(documents) = yaml_rust2::YamlLoader::load_from_str(&yaml.join("\n")) else {
        return Some(("VVC005", 1, "YAML frontmatter is invalid"));
    };
    let Some(Yaml::Hash(map)) = documents.into_iter().next() else {
        return Some(("VVC005", 1, "YAML frontmatter is not a mapping"));
    };
    let field = |name: &str| map.get(&Yaml::String(name.to_owned()));
    let line_of = |name: &str| {
        yaml.iter()
            .position(|l| l.starts_with(&format!("{name}:")))
            .map_or(1, |i| i + 2)
    };
    for name in REQUIRED_FIELDS {
        let present = match field(name) {
            None | Some(Yaml::Null) => false,
            Some(Yaml::String(s)) => !s.trim().is_empty(),
            Some(Yaml::Array(_)) => true,
            Some(_) => *name != "tags",
        };
        if !present || (*name == "tags" && !matches!(field(name), Some(Yaml::Array(_)))) {
            return Some((
                "VVC005",
                1,
                "frontmatter needs title, created, updated, type, area and tags (a list)",
            ));
        }
    }
    match field("type") {
        Some(Yaml::String(kind)) if NOTE_TYPES.contains(&kind.as_str()) => None,
        _ => Some((
            "VVC006",
            line_of("type"),
            "note type is not one of the allowed vault types",
        )),
    }
}

/// Inputs for a ledger note `Sessions/agent-runs/<date>_<task>_<agent>.md`.
pub struct LedgerNote<'a> {
    pub task: &'a str,
    pub agent: &'a str,
    pub date: &'a str,
    pub area: &'a str,
    pub title: Option<&'a str>,
}

/// Create a ledger note from the embedded template; never overwrites. Returns the relative path.
pub fn new_note(vault: &Path, note: &LedgerNote) -> anyhow::Result<String> {
    for (what, value) in [
        ("task", note.task),
        ("agent", note.agent),
        ("area", note.area),
    ] {
        let safe = value.len() <= 80
            && value.starts_with(|c: char| c.is_ascii_alphanumeric())
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        if !safe {
            anyhow::bail!(
                "--{what} must be 1-80 ASCII letters, digits, '.', '_' or '-', starting with a letter or digit"
            );
        }
    }
    validate_date(note.date)?;
    if !vault.is_dir() {
        anyhow::bail!("{} is not a directory", vault.display());
    }
    let default_title = format!("{} ({})", note.task, note.agent);
    let title = note.title.unwrap_or(&default_title);
    if title.contains(['\n', '\r']) {
        anyhow::bail!("--title must be a single line");
    }
    let quoted = title.replace('\\', "\\\\").replace('"', "\\\"");
    let body = LEDGER
        .replace("\"{{title}}\"", &format!("\"{quoted}\""))
        .replace("{{title}}", title)
        .replace("{{date}}", note.date)
        .replace("{{area}}", note.area)
        .replace("{{agent}}", note.agent);
    let rel = format!(
        "Sessions/agent-runs/{}_{}_{}.md",
        note.date, note.task, note.agent
    );
    let path = vault.join(&rel);
    std::fs::create_dir_all(path.parent().context("note path has no parent")?)?;
    if exists(&path) {
        anyhow::bail!("{rel} already exists; ledger notes are never overwritten");
    }
    write_new(&path, body.as_bytes())?;
    Ok(rel)
}

fn validate_date(date: &str) -> anyhow::Result<()> {
    let parts: Vec<&str> = date.split('-').collect();
    let numeric = |s: &str, len| s.len() == len && s.bytes().all(|b| b.is_ascii_digit());
    let valid = parts.len() == 3
        && numeric(parts[0], 4)
        && numeric(parts[1], 2)
        && numeric(parts[2], 2)
        && {
            let year: u32 = parts[0].parse().unwrap_or(0);
            let month: u32 = parts[1].parse().unwrap_or(0);
            let day: u32 = parts[2].parse().unwrap_or(0);
            let leap =
                year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
            let days = match month {
                1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                4 | 6 | 9 | 11 => 30,
                2 if leap => 29,
                2 => 28,
                _ => 0,
            };
            (1..=days).contains(&day)
        };
    if !valid {
        anyhow::bail!("--date must be a calendar date in YYYY-MM-DD form");
    }
    Ok(())
}

/// Outcome of [`snapshot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotOutcome {
    Committed { commit: String, files: usize },
    NothingToSnapshot,
}

/// Commit the personal vault to its own repository, excluding `Team/**` (D30). Fails closed when
/// `Team` is not excluded or already tracked, and when a file to be committed matches a secret
/// pattern (reported by path and line only). Uses the person's own Git identity and config.
pub fn snapshot(vault: &Path, message: &str) -> anyhow::Result<SnapshotOutcome> {
    if !is_git_root(vault) {
        anyhow::bail!(
            "{} is not a Git repository; run `vesica vault fix --path {}` first",
            vault.display(),
            vault.display()
        );
    }
    if let Some(problem) = team_exclusion_problem(vault)? {
        anyhow::bail!("refusing to snapshot: {problem}");
    }
    // Team is verified as ignored and untracked above, so it can't appear in status or be added;
    // the staged-Team check below is a second guard.
    let status = git_user(
        vault,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let mut candidates = Vec::new();
    let mut entries = status.split('\0').filter(|e| !e.is_empty());
    while let Some(entry) = entries.next() {
        let (code, path) = entry.split_at(entry.len().min(3));
        if code.starts_with(['R', 'C']) {
            entries.next(); // original path of a rename/copy
        }
        candidates.push(path.to_owned());
    }
    let mut leaks = Vec::new();
    for rel in &candidates {
        let path = vault.join(rel);
        if path.is_file() && !path.is_symlink() {
            for line in crate::scan_file(&path)? {
                leaks.push(format!("{rel}:{line}"));
            }
        }
    }
    if !leaks.is_empty() {
        anyhow::bail!(
            "refusing to snapshot: possible secrets in {} (values not shown); remove them first",
            leaks.join(", ")
        );
    }
    git_user(vault, &["add", "-A", "--", "."])?;
    let staged_team = git_user(vault, &["diff", "--cached", "--name-only", "--", "Team"])?;
    if !staged_team.is_empty() {
        git_user(vault, &["reset", "-q", "--", "Team"])?;
        anyhow::bail!("refusing to snapshot: Team content was staged and has been unstaged");
    }
    let staged = git_user(vault, &["diff", "--cached", "--name-only", "-z"])?;
    let files = staged.split('\0').filter(|e| !e.is_empty()).count();
    if files == 0 {
        return Ok(SnapshotOutcome::NothingToSnapshot);
    }
    git_user(vault, &["commit", "-q", "-m", message])?;
    let commit = git_user(vault, &["rev-parse", "--short", "HEAD"])?
        .trim()
        .to_owned();
    Ok(SnapshotOutcome::Committed { commit, files })
}

/// Git with the person's own configuration (identity, hooks), no prompts.
fn git_user(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    crate::git::user(dir, args)
}

/// Git isolated from user and system configuration, for structural checks.
fn git_isolated(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    crate::git::isolated(dir, args)
}
