//! `vesica share` (plan §5.4, D26, S9): promote a personal note into the shared home.
//!
//! The flow is the plan's: create a branch, move the note in, rewrite inbound personal wikilinks to
//! `[[Team/…]]`, add `owner`/`authority`, gate on `lint` + `vault-scan`, then request a PR. Three
//! properties matter most:
//!
//! - **Trunk is never written.** Everything happens on `share/<note>`; nothing is pushed here.
//! - **A person confirms.** Without an explicit confirmation the command only prints its plan.
//! - **A failed gate leaves nothing behind.** The branch and the staged note are rolled back.

use crate::rollback;
use crate::{git, lint_shared_home, personal, scan_secrets};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// What `share` intends to do, computed before anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharePlan {
    /// Note path relative to the vault.
    pub source_rel: String,
    /// Destination path relative to the shared home.
    pub destination_rel: String,
    pub branch: String,
    /// Frontmatter fields added to the promoted note.
    pub additions: Vec<&'static str>,
    /// Links inside the note that cannot resolve in the shared home and would be dropped.
    pub dropped_links: Vec<String>,
    /// The note's content as it will be written, with links already resolved. Frontmatter
    /// additions are applied on top by [`apply`].
    pub content: String,
    /// Links inside the note rewritten from a vault path to a shared-home path.
    pub link_rewrites: Vec<(String, String)>,
    /// Personal notes whose links into the promoted note would be rewritten.
    pub rewrites: Vec<Rewrite>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    /// Note path relative to the vault.
    pub file: String,
    pub from: String,
    pub to: String,
}

/// A [`SharePlan`] that has been applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub commit: String,
    pub rewrites_applied: usize,
    pub moved: bool,
}

pub struct ShareRequest<'a> {
    pub vault: &'a Path,
    pub shared: &'a Path,
    /// The personal note to promote.
    pub note: &'a Path,
    /// Destination directory relative to the shared home, e.g. `Knowledge/Architecture`.
    pub destination_dir: &'a str,
    pub owner: &'a str,
    pub drop_unresolved_links: bool,
}

/// Validate everything and return the plan. Writes nothing.
pub fn plan(request: &ShareRequest) -> Result<SharePlan> {
    let vault = request
        .vault
        .canonicalize()
        .with_context(|| format!("vault path does not exist: {}", request.vault.display()))?;
    let shared = request.shared.canonicalize().with_context(|| {
        format!(
            "shared-home path does not exist: {}",
            request.shared.display()
        )
    })?;
    let note = request
        .note
        .canonicalize()
        .with_context(|| format!("note does not exist: {}", request.note.display()))?;
    if !note.is_file() {
        anyhow::bail!("{} is not a file", note.display());
    }
    let source_rel = note
        .strip_prefix(&vault)
        .map_err(|_| {
            anyhow::anyhow!(
                "{} is outside the vault {}",
                note.display(),
                vault.display()
            )
        })?
        .to_string_lossy()
        .replace('\\', "/");
    if !source_rel.ends_with(".md") {
        anyhow::bail!("only Markdown notes can be shared: {source_rel}");
    }
    let stem = Path::new(&source_rel)
        .file_stem()
        .and_then(|s| s.to_str())
        .context("note has no file name")?;
    let text = std::fs::read_to_string(&note)?;
    let frontmatter = personal::frontmatter_of(&text)
        .with_context(|| format!("{source_rel} is missing a valid YAML frontmatter block"))?;

    let destination_rel = format!("{}/{stem}.md", request.destination_dir.trim_matches('/'));
    let destination_dir = shared.join(request.destination_dir.trim_matches('/'));
    if !destination_dir.is_dir() {
        anyhow::bail!(
            "{} has no {}: create that area in the shared home first",
            shared.display(),
            request.destination_dir.trim_matches('/')
        );
    }
    let destination = shared.join(&destination_rel);
    if destination.exists() {
        anyhow::bail!("{destination_rel} already exists in the shared home");
    }
    if !git::is_repository(&shared) {
        anyhow::bail!("{} is not a Git repository", shared.display());
    }
    if !git::isolated(&shared, &["status", "--porcelain"])?.is_empty() {
        anyhow::bail!(
            "{} has uncommitted changes; commit or stash them before sharing",
            shared.display()
        );
    }
    let branch = format!("share/{stem}");
    if !git::isolated(&shared, &["branch", "--list", &branch])?
        .trim()
        .is_empty()
    {
        anyhow::bail!("branch {branch} already exists");
    }

    let links = resolve_links(
        &text,
        &shared,
        Path::new(&destination_rel),
        request.drop_unresolved_links,
    )?;
    let mut additions = Vec::new();
    if !frontmatter.iter().any(|l| l.starts_with("owner:")) {
        additions.push("owner");
    }
    if !frontmatter.iter().any(|l| l.starts_with("authority:")) {
        additions.push("authority");
    }
    let rewrites = inbound_rewrites(&vault, stem, &destination_rel)?;
    Ok(SharePlan {
        source_rel,
        destination_rel,
        branch,
        additions,
        dropped_links: links.dropped,
        content: links.content,
        link_rewrites: links.rewrites,
        rewrites,
    })
}

/// The promoted note's final content: the planned content plus the frontmatter the shared home
/// requires. Applying the plan never re-reads the source note, so what was shown is what is written.
pub fn promoted_content(request: &ShareRequest, plan: &SharePlan) -> String {
    add_frontmatter_fields(&plan.content, request.owner, &plan.additions)
}

/// Insert the missing fields at the end of the frontmatter block.
fn add_frontmatter_fields(text: &str, owner: &str, additions: &[&str]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let closing = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, line)| line.trim_end() == "---")
        .map(|(index, _)| index);
    if let Some(index) = closing {
        let mut insert = Vec::new();
        if additions.contains(&"owner") {
            insert.push(format!("owner: {owner}"));
        }
        if additions.contains(&"authority") {
            insert.push("authority: working".to_owned());
        }
        for (offset, line) in insert.into_iter().enumerate() {
            lines.insert(index + offset, line);
        }
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// The result of link handling for a promoted note.
struct LinkOutcome {
    content: String,
    /// `(from, to)` for links rewritten from the vault view to the shared-home view.
    rewrites: Vec<(String, String)>,
    /// Link targets kept as plain text because they cannot resolve in the shared home.
    dropped: Vec<String>,
}

/// Link handling for a promoted note. A link written in the vault may name `Team/…`, which does
/// not exist inside the shared home, so it is rewritten to the shared-home path. Anything that
/// still cannot resolve is refused, unless the caller explicitly asks to keep the text only.
fn resolve_links(
    text: &str,
    shared: &Path,
    destination: &Path,
    drop_unresolved: bool,
) -> Result<LinkOutcome> {
    let mut out = String::with_capacity(text.len());
    let mut rewrites = Vec::new();
    let mut dropped = Vec::new();
    let mut rest = text;
    loop {
        let Some(start) = rest.find("[[") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else {
            out.push_str(&rest[start..]);
            break;
        };
        let inner = &after[..end];
        let split = inner.find(['|', '#']).unwrap_or(inner.len());
        let (target, suffix) = inner.split_at(split);
        let target = target.trim();
        let stripped = target.strip_prefix("Team/");
        let keep_as = match stripped {
            Some(stripped) if resolves_in_shared(shared, destination, stripped) => {
                if stripped != target {
                    rewrites.push((target.to_owned(), stripped.to_owned()));
                }
                Some(stripped.to_owned())
            }
            Some(_) => None,
            None if target.is_empty() || resolves_in_shared(shared, destination, target) => {
                Some(target.to_owned())
            }
            None => None,
        };
        match keep_as {
            Some(target) => out.push_str(&format!("[[{target}{suffix}]]")),
            None if drop_unresolved => {
                dropped.push(target.to_owned());
                out.push_str(target);
            }
            None => anyhow::bail!(
                "link [[{target}]] would point outside the shared home; fix it, or pass --drop-unresolved-links to keep the text only"
            ),
        }
        rest = &after[end + 2..];
    }
    Ok(LinkOutcome {
        content: out,
        rewrites,
        dropped,
    })
}

/// The distinct link targets in a body of text, without aliases or heading anchors.
fn wikilinks(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else {
            break;
        };
        let target = after[..end]
            .split('|')
            .next()
            .unwrap_or_default()
            .split('#')
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned();
        if !target.is_empty() && !targets.contains(&target) {
            targets.push(target);
        }
        rest = &after[end + 2..];
    }
    targets
}

/// A link resolves when the shared home holds that note, by path or by unique basename.
fn resolves_in_shared(shared: &Path, destination: &Path, target: &str) -> bool {
    let relative = target.trim_start_matches('/');
    let with_extension = if Path::new(relative).extension().is_some() {
        relative.to_owned()
    } else {
        format!("{relative}.md")
    };
    let candidates = [
        destination
            .parent()
            .map(|dir| dir.join(&with_extension))
            .unwrap_or_else(|| PathBuf::from(&with_extension)),
        shared.join(&with_extension),
    ];
    if candidates.iter().any(|c| c.is_file()) {
        return true;
    }
    let stem = Path::new(relative)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if stem.is_empty() {
        return false;
    }
    let mut matches = 0;
    for entry in walkdir::WalkDir::new(shared)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !(e.depth() == 1 && e.file_name() == ".git"))
        .flatten()
    {
        let path = entry.path();
        if path.is_file()
            && path.extension().is_some_and(|e| e == "md")
            && path
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case(&stem))
        {
            matches += 1;
        }
    }
    matches == 1
}

/// Personal notes whose wikilinks point at the note being promoted.
fn inbound_rewrites(vault: &Path, stem: &str, destination_rel: &str) -> Result<Vec<Rewrite>> {
    let target = destination_rel.trim_end_matches(".md");
    let new_target = format!("Team/{target}");
    let mut rewrites = Vec::new();
    let walker = walkdir::WalkDir::new(vault)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            !(e.depth() == 1
                && (e.file_name() == ".git"
                    || e.file_name() == ".obsidian"
                    || e.file_name() == ".trash"
                    || e.file_name() == "Team"))
                && !(e.file_type().is_dir()
                    && matches!(
                        e.file_name().to_str(),
                        Some(".git" | ".obsidian" | ".trash")
                    ))
        });
    for entry in walker {
        let entry = entry?;
        if !entry.file_type().is_file() || entry.path().extension().is_none_or(|e| e != "md") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for link in wikilinks(&text) {
            let normalised = link
                .strip_prefix("Team/")
                .unwrap_or(&link)
                .trim_end_matches(".md")
                .to_ascii_lowercase();
            let matches = normalised == target.to_ascii_lowercase()
                || Path::new(&normalised)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case(stem));
            if matches {
                rewrites.push(Rewrite {
                    file: entry
                        .path()
                        .strip_prefix(vault)
                        .unwrap_or(entry.path())
                        .to_string_lossy()
                        .replace('\\', "/"),
                    from: link,
                    to: new_target.clone(),
                });
            }
        }
    }
    rewrites.sort_by(|a, b| a.file.cmp(&b.file).then(a.from.cmp(&b.from)));
    Ok(rewrites)
}

/// Apply a plan: branch, write, gate on lint and vault-scan, commit, rewrite, optionally move.
pub fn apply(
    request: &ShareRequest,
    plan: &SharePlan,
    commit_message: &str,
    move_source: bool,
) -> Result<Applied> {
    let vault = request.vault.canonicalize()?;
    let shared = request.shared.canonicalize()?;
    let original = git::isolated(&shared, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_owned();
    git::isolated(&shared, &["checkout", "-q", "-b", &plan.branch])?;
    let destination = shared.join(&plan.destination_rel);
    let result = (|| -> Result<Applied> {
        std::fs::create_dir_all(destination.parent().unwrap_or(&shared))?;
        // The file is created before the gate, so it is untracked at that point: `git checkout -f`
        // alone would leave it behind on a refusal.
        rollback::note_created(&destination);
        std::fs::write(&destination, promoted_content(request, plan))?;

        // The gate runs against the staged tree, before any commit exists.
        let lint = lint_shared_home(&shared)?;
        if !lint.is_empty() {
            let first = &lint[0];
            anyhow::bail!(
                "shared-home lint failed ({} finding(s)); first: {}:{}:{} {}",
                lint.len(),
                first.path.display(),
                first.line,
                first.rule,
                first.message
            );
        }
        let secrets = scan_secrets(&shared)?;
        if !secrets.is_empty() {
            anyhow::bail!(
                "vault-scan found {} credential pattern(s); first at {}:{}",
                secrets.len(),
                secrets[0].path.display(),
                secrets[0].line
            );
        }

        git::isolated(&shared, &["add", "--", &plan.destination_rel])?;
        git::user(&shared, &["commit", "-q", "-m", commit_message])?;
        let commit = git::isolated(&shared, &["rev-parse", "--short", "HEAD"])?
            .trim()
            .to_owned();

        let mut rewrites_applied = 0_usize;
        for rewrite in &plan.rewrites {
            let path = vault.join(&rewrite.file);
            let text = std::fs::read_to_string(&path)?;
            let updated = text
                .replace(
                    &format!("[[{}]]", rewrite.from),
                    &format!("[[{}]]", rewrite.to),
                )
                .replace(
                    &format!("[[{}|", rewrite.from),
                    &format!("[[{}|", rewrite.to),
                )
                .replace(
                    &format!("[[{}#", rewrite.from),
                    &format!("[[{}#", rewrite.to),
                );
            if updated != text {
                std::fs::write(&path, updated)?;
                rewrites_applied += 1;
            }
        }

        let mut moved = false;
        if move_source {
            std::fs::remove_file(request.note)?;
            moved = true;
        }
        Ok(Applied {
            commit,
            rewrites_applied,
            moved,
        })
    })();

    if result.is_err() {
        // Leave no branch, no staged change and no untracked file behind.
        let _ = git::isolated(&shared, &["reset", "-q"]);
        rollback::remove_created(&shared, &destination);
        let _ = git::isolated(&shared, &["checkout", "-q", "-f", &original]);
        let _ = git::isolated(&shared, &["branch", "-D", &plan.branch]);
    }
    result
}
