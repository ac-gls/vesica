//! `vesica share` (plan §5.4, D26, S9): promote a personal note into the shared home.
//!
//! The flow is the plan's: create a branch, move the note in, rewrite inbound personal wikilinks to
//! `[[Team/…]]`, add `owner`/`authority`, gate on `lint` + `vault-scan`, then request a PR. Three
//! properties matter most:
//!
//! - **Trunk is never written.** Everything happens on `share/<note>`; nothing is pushed here.
//! - **A person confirms.** Without an explicit confirmation the command only prints its plan.
//! - **A failed gate leaves nothing behind.** The branch and the staged note are rolled back.

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
    let source_path = request
        .note
        .canonicalize()
        .with_context(|| format!("note does not exist: {}", request.note.display()))?;
    if !source_path.starts_with(&vault) {
        anyhow::bail!(
            "{} is outside the vault {}",
            request.note.display(),
            vault.display()
        );
    }
    let note = source_path.clone();
    let source_rel = note
        .strip_prefix(&vault)
        .map_err(|_| {
            anyhow::anyhow!(
                "{} is outside the vault {}",
                request.note.display(),
                vault.display()
            )
        })?
        .to_string_lossy()
        .replace('\\', "/");
    if !source_path.is_file() {
        anyhow::bail!("{} is not a file", request.note.display());
    }
    if !source_rel.ends_with(".md") {
        anyhow::bail!("only Markdown notes can be shared: {source_rel}");
    }
    let stem = Path::new(&source_rel)
        .file_stem()
        .and_then(|s| s.to_str())
        .context("note has no file name")?;
    let text = std::fs::read_to_string(&source_path)?;
    let frontmatter = personal::frontmatter_of(&text)
        .with_context(|| format!("{source_rel} is missing a valid YAML frontmatter block"))?;

    let destination_dir = checked_shared_relative_path(request.destination_dir)?;
    let destination_dir = validate_shared_destination(&shared, &destination_dir)?;
    let destination_rel = destination_dir
        .strip_prefix(&shared)?
        .join(format!("{stem}.md"))
        .to_string_lossy()
        .replace('\\', "/");
    let destination = validate_shared_destination(&shared, Path::new(&destination_rel))?;
    if !destination_dir.is_dir() {
        anyhow::bail!(
            "{} has no {}: create that area in the shared home first",
            shared.display(),
            request.destination_dir.trim_matches('/')
        );
    }
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
    let rewrites = inbound_rewrites(&vault, &shared, &source_rel, &destination_rel)?;
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

/// Interpret a caller-supplied path as an ordinary relative path below the shared-home root.
fn checked_shared_relative_path(path: &str) -> Result<PathBuf> {
    let mut relative = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            std::path::Component::Normal(part) => relative.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::RootDir
            | std::path::Component::ParentDir
            | std::path::Component::Prefix(_) => {
                anyhow::bail!("destination must stay inside the shared home");
            }
        }
    }
    if relative.as_os_str().is_empty() {
        anyhow::bail!("destination directory must not be empty");
    }
    Ok(relative)
}

/// Resolve a shared-home-relative path without traversing a symlinked ancestor. Missing suffix
/// components are allowed because `apply` may create them, but every existing ancestor must be a
/// real directory below the canonical shared-home root.
fn validate_shared_destination(shared: &Path, relative: &Path) -> Result<PathBuf> {
    let mut destination = shared.to_path_buf();
    let components: Vec<_> = relative.components().collect();
    for (index, component) in components.iter().enumerate() {
        let std::path::Component::Normal(part) = component else {
            anyhow::bail!("destination must stay inside the shared home");
        };
        destination.push(part);
        match std::fs::symlink_metadata(&destination) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || (index + 1 < components.len() && !metadata.is_dir())
                {
                    anyhow::bail!("destination must stay inside the shared home");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(destination)
}

/// A nested Git marker is an independent content boundary, including linked worktrees
/// and dangling markers. An unreadable marker must not authorize traversal.
fn independent_repository(path: &Path) -> bool {
    match std::fs::symlink_metadata(path.join(".git")) {
        Ok(_) => true,
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    }
}

/// Resolve a personal file without permitting symlink, traversal or shared-tree authority.
fn validate_personal_file(vault: &Path, shared: &Path, relative: &Path) -> Result<PathBuf> {
    let components: Vec<_> = relative.components().collect();
    if components.is_empty() {
        anyhow::bail!("personal path is empty");
    }
    let mut path = vault.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        let std::path::Component::Normal(part) = component else {
            anyhow::bail!(
                "personal path must stay inside the vault: {}",
                relative.display()
            );
        };
        if index == 0
            && matches!(
                part.to_str(),
                Some("Team" | ".git" | ".obsidian" | ".trash")
            )
        {
            anyhow::bail!(
                "personal path is excluded from sharing: {}",
                relative.display()
            );
        }
        if path != vault && independent_repository(&path) {
            anyhow::bail!(
                "personal path enters an independent repository: {}",
                path.display()
            );
        }
        path.push(part);
        let metadata = std::fs::symlink_metadata(&path)
            .with_context(|| format!("cannot inspect personal path {}", path.display()))?;
        if metadata.file_type().is_symlink()
            || (index + 1 < components.len() && !metadata.is_dir())
            || (index + 1 == components.len() && !metadata.is_file())
        {
            anyhow::bail!(
                "personal path is not a real {}: {}",
                if index + 1 == components.len() {
                    "regular file"
                } else {
                    "directory"
                },
                path.display()
            );
        }
    }
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(vault) || canonical.starts_with(shared) {
        anyhow::bail!(
            "personal path escapes vault or enters shared home: {}",
            path.display()
        );
    }
    Ok(path)
}

struct PersonalWrite {
    path: PathBuf,
    original: Vec<u8>,
    rewritten: Vec<u8>,
    permissions: std::fs::Permissions,
}

/// Replace a real regular file through an exclusive private sibling, retaining its mode.
fn atomic_replace(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let original = std::fs::symlink_metadata(path)?;
    if !original.is_file() || original.file_type().is_symlink() {
        return Err(std::io::Error::other(
            "replacement target is not a real regular file",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("target has no parent"))?;
    let (temporary, mut file) = loop {
        let temporary = parent.join(format!(
            ".vesica-share-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let result = (|| {
        file.write_all(bytes)?;
        file.set_permissions(original.permissions())?;
        file.sync_all()?;
        let current = std::fs::symlink_metadata(path)?;
        if current.file_type().is_symlink() || !current.is_file() {
            return Err(std::io::Error::other("replacement target changed type"));
        }
        std::fs::rename(&temporary, path)
    })();
    if let Err(error) = result {
        return match std::fs::remove_file(&temporary) {
            Ok(()) => Err(error),
            Err(cleanup) => Err(std::io::Error::other(format!(
                "{error}; temporary cleanup failed: {cleanup}"
            ))),
        };
    }
    Ok(())
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
        let target = target.strip_prefix("Team/").unwrap_or(target);
        let selected = resolve_shared_target(shared, destination, target);
        let resolved = selected.as_ref().and_then(|path| {
            // No spelling is inherently root-qualified under the existing resolver.
            // Verify every candidate in the future tree, including the promoted note.
            [
                wikilink_target(path),
                target.to_owned(),
                path.to_string_lossy().replace('\\', "/"),
            ]
            .into_iter()
            .find(|candidate| {
                resolve_shared_target_impl(shared, destination, candidate, Some(destination))
                    .as_ref()
                    == Some(path)
            })
        });
        let keep_as = match resolved {
            Some(resolved) => {
                if resolved != target {
                    rewrites.push((target.to_owned(), resolved.clone()));
                }
                Some(resolved)
            }
            None if target.is_empty() => Some(target.to_owned()),
            None => None,
        };
        match keep_as {
            Some(target) => out.push_str(&format!("[[{target}{suffix}]]")),
            None if drop_unresolved => {
                dropped.push(target.to_owned());
                out.push_str(target);
            }
            None if selected.is_some() => anyhow::bail!(
                "link [[{target}]] cannot preserve selected shared target {} after promotion; fix it, or pass --drop-unresolved-links to keep the text only",
                selected.as_ref().unwrap().display()
            ),
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

/// Resolve a link target to a Markdown file inside the shared home. Explicit paths are tried
/// relative to the promoted note and shared-home root; basename fallback is only for unqualified
/// targets with exactly one matching note. The returned path is shared-home-relative.
pub(crate) fn resolve_shared_target(
    shared: &Path,
    destination: &Path,
    target: &str,
) -> Option<PathBuf> {
    resolve_shared_target_impl(shared, destination, target, None)
}

/// Resolve against the current tree plus one planned, contained regular-file destination.
fn resolve_shared_target_impl(
    shared: &Path,
    destination: &Path,
    target: &str,
    prospective: Option<&Path>,
) -> Option<PathBuf> {
    let relative = target.trim_start_matches('/');
    let path = Path::new(relative);
    if path.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return None;
    }
    let with_extension = if path.extension().is_some() {
        path.to_path_buf()
    } else {
        PathBuf::from(format!("{relative}.md"))
    };
    let mut candidates = vec![
        destination
            .parent()
            .map(|dir| dir.join(&with_extension))
            .unwrap_or_else(|| with_extension.clone()),
        with_extension.clone(),
        PathBuf::from(relative).join("README.md"),
    ];
    if let Some(parent) = destination.parent() {
        candidates.push(parent.join(relative).join("README.md"));
    }
    for candidate in &candidates {
        if prospective == Some(candidate.as_path()) {
            return Some(candidate.clone());
        }
        let full = shared.join(candidate);
        if full.is_file()
            && full
                .canonicalize()
                .is_ok_and(|resolved| resolved.starts_with(shared))
        {
            return Some(candidate.clone());
        }
    }

    // Slash-qualified targets are deliberate paths, never basename-search requests.
    if path.components().count() != 1 {
        return None;
    }
    if path.extension().is_some_and(|extension| extension != "md") {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let mut matches = walkdir::WalkDir::new(shared)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !(entry.depth() == 1 && entry.file_name() == ".git"))
        .flatten()
        .filter(|entry| {
            let candidate = entry.path();
            entry.file_type().is_file()
                && candidate.extension().is_some_and(|ext| ext == "md")
                && candidate
                    .canonicalize()
                    .is_ok_and(|resolved| resolved.starts_with(shared))
                && candidate.file_stem().and_then(|name| name.to_str()) == Some(stem)
        });
    let first = matches
        .next()?
        .path()
        .strip_prefix(shared)
        .ok()?
        .to_path_buf();
    let prospective_match = prospective.filter(|candidate| {
        candidate
            .extension()
            .is_some_and(|extension| extension == "md")
            && candidate.file_stem().and_then(|name| name.to_str()) == Some(stem)
            && *candidate != first
    });
    if matches.next().is_some() || prospective_match.is_some() {
        None
    } else {
        Some(first)
    }
}

fn wikilink_target(path: &Path) -> String {
    let mut target = path.to_string_lossy().replace('\\', "/");
    if target.ends_with("/README.md") {
        target.truncate(target.len() - "/README.md".len());
    } else if target.ends_with(".md") {
        target.truncate(target.len() - ".md".len());
    }
    target
}

/// Resolve one existing personal target, using the shared resolver's explicit-candidate order:
/// reader-relative file, vault-root file, vault-root README, then reader-relative README.
/// Only unqualified targets may fall back to one unique Markdown basename. An unsafe existing
/// candidate blocks resolution rather than authorizing a later candidate or basename fallback.
fn resolve_personal_target(
    vault: &Path,
    shared: &Path,
    reader: &Path,
    target: &str,
    notes: &[PathBuf],
) -> Option<PathBuf> {
    let target_path = Path::new(target);
    if target_path.components().any(|component| {
        !matches!(
            component,
            std::path::Component::Normal(_) | std::path::Component::CurDir
        )
    }) || target_path
        .extension()
        .is_some_and(|extension| extension != "md")
    {
        return None;
    }
    let with_extension = if target_path.extension().is_some() {
        target_path.to_path_buf()
    } else {
        PathBuf::from(format!("{target}.md"))
    };
    let parent = reader.parent().unwrap_or_else(|| Path::new(""));
    let candidates = [
        parent.join(&with_extension),
        with_extension,
        target_path.join("README.md"),
        parent.join(target_path).join("README.md"),
    ];
    for candidate in candidates {
        // Inspect prefixes even when the leaf is absent: a dangling or escaping ancestor
        // must not turn a failed lookup into permission to use basename fallback.
        let mut prefix = vault.to_path_buf();
        let components: Vec<_> = candidate.components().collect();
        for (index, component) in components.iter().enumerate() {
            prefix.push(component.as_os_str());
            match std::fs::symlink_metadata(&prefix) {
                Ok(metadata)
                    if metadata.file_type().is_symlink()
                        || (index + 1 < components.len() && !metadata.is_dir()) =>
                {
                    return None;
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(_) => return None,
            }
        }
        let full = vault.join(&candidate);
        match std::fs::symlink_metadata(&full) {
            Ok(_) => return validate_personal_file(vault, shared, &candidate).ok(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
    }
    if target_path.components().count() != 1 {
        return None;
    }
    let stem = target_path.file_stem()?;
    let mut matches = notes.iter().filter(|path| path.file_stem() == Some(stem));
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    validate_personal_file(vault, shared, first.strip_prefix(vault).ok()?).ok()
}

fn inbound_rewrites(
    vault: &Path,
    shared: &Path,
    source_rel: &str,
    destination_rel: &str,
) -> Result<Vec<Rewrite>> {
    let source = validate_personal_file(vault, shared, Path::new(source_rel))?;
    let target = destination_rel.trim_end_matches(".md");
    let new_target = format!("Team/{target}");
    let mut notes = Vec::new();
    for entry in walkdir::WalkDir::new(vault)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            !entry.path().starts_with(shared)
                && !(entry.depth() > 0
                    && entry.file_type().is_dir()
                    && independent_repository(entry.path()))
                && !(entry.depth() == 1
                    && matches!(
                        entry.file_name().to_str(),
                        Some(".git" | ".obsidian" | ".trash" | "Team")
                    ))
        })
    {
        let entry = entry?;
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "md") {
            if let Ok(text) = std::fs::read_to_string(entry.path()) {
                notes.push((entry.path().to_path_buf(), text));
            }
        }
    }
    let note_paths: Vec<_> = notes.iter().map(|(path, _)| path.clone()).collect();
    let mut rewrites = Vec::new();
    for (path, text) in notes {
        let relative = path.strip_prefix(vault).unwrap_or(&path);
        for link in wikilinks(&text) {
            // Team links already name shared storage; they never resolve to personal notes.
            if link.starts_with("Team/") {
                continue;
            }
            let resolved = resolve_personal_target(vault, shared, relative, &link, &note_paths);
            if resolved.as_ref() == Some(&source) {
                rewrites.push(Rewrite {
                    file: relative.to_string_lossy().replace('\\', "/"),
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
    apply_impl(request, plan, commit_message, move_source, |path, bytes| {
        atomic_replace(path, bytes)
    })
}

fn apply_impl<W>(
    request: &ShareRequest,
    plan: &SharePlan,
    commit_message: &str,
    move_source: bool,
    write_personal: W,
) -> Result<Applied>
where
    W: FnMut(&Path, &[u8]) -> std::io::Result<()>,
{
    apply_transaction(
        request,
        plan,
        commit_message,
        move_source,
        write_personal,
        |path| std::fs::remove_file(path),
    )
}

fn apply_transaction<W, R>(
    request: &ShareRequest,
    plan: &SharePlan,
    commit_message: &str,
    move_source: bool,
    mut write_personal: W,
    mut remove_source: R,
) -> Result<Applied>
where
    W: FnMut(&Path, &[u8]) -> std::io::Result<()>,
    R: FnMut(&Path) -> std::io::Result<()>,
{
    let vault = request.vault.canonicalize()?;
    let shared = request.shared.canonicalize()?;
    git::validate_mutation_repository(&shared)?;
    if !git::isolated(&shared, &["status", "--porcelain"])?.is_empty() {
        anyhow::bail!("shared home changed after planning; refusing before branch creation");
    }
    let destination_relative = checked_shared_relative_path(&plan.destination_rel)?;
    validate_shared_destination(&shared, &destination_relative)?;
    let destination = shared.join(&destination_relative);
    if std::fs::symlink_metadata(&destination).is_ok() {
        anyhow::bail!(
            "share destination already exists: {}",
            destination.display()
        );
    }
    // A plan is public data, not authority to write arbitrary paths. Validate every target
    // before branch creation, and again immediately before each personal mutation.
    let source_path = validate_personal_file(&vault, &shared, Path::new(&plan.source_rel))?;
    let source_bytes = std::fs::read(&source_path)?;
    let source_permissions = std::fs::symlink_metadata(&source_path)?.permissions();
    for rewrite in &plan.rewrites {
        let path = validate_personal_file(&vault, &shared, Path::new(&rewrite.file))?;
        std::fs::read_to_string(&path)
            .with_context(|| format!("cannot preflight rewrite {}", path.display()))?;
    }
    let mut personal_writes: Vec<PersonalWrite> = Vec::new();
    let mut destination_written = false;
    let promoted = promoted_content(request, plan).into_bytes();
    let original = git::isolated(&shared, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_owned();
    git::isolated(&shared, &["checkout", "-q", "-b", &plan.branch])?;
    let result = (|| -> Result<Applied> {
        let destination = validate_shared_destination(&shared, &destination_relative)?;
        std::fs::create_dir_all(destination.parent().unwrap_or(&shared))?;
        let destination = validate_shared_destination(&shared, &destination_relative)?;
        // The file is created before the gate, so it is untracked at that point: `git checkout -f`
        // alone would leave it behind on a refusal.
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        destination_written = true;
        file.write_all(&promoted)?;
        file.sync_all()?;

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
            let path = validate_personal_file(&vault, &shared, Path::new(&rewrite.file))?;
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
                let updated = updated.into_bytes();
                personal_writes.push(PersonalWrite {
                    path: path.clone(),
                    original: text.into_bytes(),
                    rewritten: updated.clone(),
                    permissions: std::fs::symlink_metadata(&path)?.permissions(),
                });
                write_personal(&path, &updated)?;
                rewrites_applied += 1;
            }
        }

        let mut moved = false;
        if move_source {
            let source = validate_personal_file(&vault, &shared, Path::new(&plan.source_rel))?;
            if std::fs::read(&source)? != source_bytes
                || std::fs::symlink_metadata(&source)?.permissions() != source_permissions
            {
                anyhow::bail!("source changed during sharing; preserving current file");
            }
            remove_source(&source)?;
            moved = true;
        }
        Ok(Applied {
            commit,
            rewrites_applied,
            moved,
        })
    })();

    if let Err(error) = result {
        let mut rollback_errors = Vec::new();
        for PersonalWrite {
            path,
            original,
            rewritten,
            permissions,
        } in personal_writes.into_iter().rev()
        {
            let relative = path.strip_prefix(&vault)?;
            if let Err(verify_error) = validate_personal_file(&vault, &shared, relative) {
                rollback_errors.push(format!(
                    "cannot verify {} before restoration: {verify_error:#}",
                    path.display()
                ));
                continue;
            }
            match std::fs::read(&path) {
                Ok(current) if current == rewritten => {
                    if std::fs::symlink_metadata(&path)?.permissions() != permissions {
                        rollback_errors.push(format!(
                            "{} mode changed after rewrite; preserving current file",
                            path.display()
                        ));
                        continue;
                    }
                    if let Err(restore_error) = atomic_replace(&path, &original) {
                        rollback_errors.push(format!("{}: {restore_error}", path.display()));
                    }
                }
                Ok(current) if current == original => {}
                Ok(_) => rollback_errors.push(format!(
                    "{} changed after this share rewrote it; preserving current contents",
                    path.display()
                )),
                Err(read_error) => rollback_errors.push(format!(
                    "cannot verify {} before restoration: {read_error}",
                    path.display()
                )),
            }
        }
        // Never force-checkout over intervening edits. Keep the branch for manual recovery
        // if the promoted file no longer matches our post-state.
        let owned = !destination_written
            || validate_shared_destination(&shared, &destination_relative)
                .and_then(|path| {
                    let metadata = std::fs::symlink_metadata(&path)?;
                    Ok(metadata.is_file()
                        && !metadata.file_type().is_symlink()
                        && std::fs::read(&path)? == promoted)
                })
                .unwrap_or(false);
        if owned && rollback_errors.is_empty() {
            if let Err(cleanup) = git::isolated(&shared, &["reset", "-q"]) {
                rollback_errors.push(format!("{cleanup:#}"));
            }
            if destination_written {
                if let Err(cleanup) = std::fs::remove_file(&destination) {
                    rollback_errors.push(format!("destination cleanup failed: {cleanup}"));
                }
            }
            match git::isolated(&shared, &["checkout", "-q", &original]) {
                Ok(_) => {
                    if let Err(cleanup) = git::isolated(&shared, &["branch", "-D", &plan.branch]) {
                        rollback_errors.push(format!("{cleanup:#}"));
                    }
                }
                Err(cleanup) => rollback_errors.push(format!("{cleanup:#}")),
            }
        } else {
            rollback_errors.push(format!("destination cleanup refused: {} changed type or contents; preserving current state and share branch", destination.display()));
        }
        if rollback_errors.is_empty() {
            return Err(error);
        }
        anyhow::bail!(
            "{error:#}; share rollback incomplete: {}",
            rollback_errors.join("; ")
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{apply_impl, plan, ShareRequest};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    struct Fixture {
        _temp: tempfile::TempDir,
        vault: PathBuf,
        shared: PathBuf,
        note: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let vault = temp.path().join("Personal");
            let shared = temp.path().join("Shared");
            std::fs::create_dir_all(vault.join("Inbox")).unwrap();
            std::fs::create_dir_all(shared.join("Knowledge/Architecture")).unwrap();
            std::fs::write(
                shared.join("README.md"),
                "---\ntitle: Shared\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: readme\narea: team\ntags: [test]\nowner: test\nauthority: working\n---\nShared.\n",
            )
            .unwrap();
            std::fs::write(
                shared.join("Knowledge/Architecture/README.md"),
                "---\ntitle: Architecture\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: readme\narea: team\ntags: [test]\nowner: test\nauthority: working\n---\nArchitecture.\n",
            )
            .unwrap();
            git(&shared, ["init", "-q", "-b", "main"]);
            git(&shared, ["config", "user.name", "Vesica Test"]);
            git(&shared, ["config", "user.email", "vesica@example.invalid"]);
            git(&shared, ["add", "."]);
            git(&shared, ["commit", "-q", "-m", "initial"]);
            let note = vault.join("Inbox/note.md");
            std::fs::write(
                &note,
                "---\ntitle: Note\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: note\narea: test\ntags: [test]\n---\nA note.\n",
            )
            .unwrap();
            std::fs::write(vault.join("Inbox/a.md"), "See [[note]].\n").unwrap();
            std::fs::write(vault.join("Inbox/z.md"), "Also see [[note|the note]].\n").unwrap();
            Self {
                _temp: temp,
                vault,
                shared,
                note,
            }
        }

        fn request(&self) -> ShareRequest<'_> {
            ShareRequest {
                vault: &self.vault,
                shared: &self.shared,
                note: &self.note,
                destination_dir: "Knowledge/Architecture",
                owner: "test",
                drop_unresolved_links: false,
            }
        }
    }

    fn git<I, S>(cwd: &Path, args: I) -> String
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let output = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    #[cfg(unix)]
    #[test]
    fn apply_refuses_independent_repository_added_after_plan_before_branch() {
        for marker in ["directory", "worktree-file", "dangling-link"] {
            let fixture = Fixture::new();
            let request = fixture.request();
            let planned = plan(&request).unwrap();
            let root = fixture.vault.join("Inbox");
            match marker {
                "directory" => std::fs::create_dir(root.join(".git")).unwrap(),
                "worktree-file" => std::fs::write(root.join(".git"), "gitdir: missing\n").unwrap(),
                _ => std::os::unix::fs::symlink(root.join("missing"), root.join(".git")).unwrap(),
            }
            let head = git(&fixture.shared, ["rev-parse", "HEAD"]);
            let index = std::fs::read(fixture.shared.join(".git/index")).unwrap();
            let error = super::apply(&request, &planned, "share", false).unwrap_err();
            assert!(format!("{error:#}").contains("independent repository"));
            assert_eq!(git(&fixture.shared, ["rev-parse", "HEAD"]), head);
            assert_eq!(
                std::fs::read(fixture.shared.join(".git/index")).unwrap(),
                index
            );
            assert_eq!(git(&fixture.shared, ["branch", "--show-current"]), "main");
            assert!(!fixture.shared.join(&planned.destination_rel).exists());
            assert_eq!(
                std::fs::read(root.join("a.md")).unwrap(),
                b"See [[note]].\n"
            );
        }
    }

    #[test]
    fn injected_source_remove_io_failure_restores_entire_share_transaction() {
        let fixture = Fixture::new();
        let request = fixture.request();
        let planned = plan(&request).unwrap();
        let first = fixture.vault.join("Inbox/a.md");
        let bytes = std::fs::read(&first).unwrap();
        let source = std::fs::read(&fixture.note).unwrap();
        let head = git(&fixture.shared, ["rev-parse", "HEAD"]);
        let mut reached = false;
        let error = super::apply_transaction(
            &request,
            &planned,
            "share",
            true,
            super::atomic_replace,
            |_| {
                reached = true;
                Err(std::io::Error::other("injected source-removal IO error"))
            },
        )
        .unwrap_err();
        assert!(reached);
        assert!(format!("{error:#}").contains("injected source-removal IO error"));
        assert_eq!(std::fs::read(&first).unwrap(), bytes);
        assert_eq!(std::fs::read(&fixture.note).unwrap(), source);
        assert_eq!(git(&fixture.shared, ["rev-parse", "HEAD"]), head);
        assert_eq!(git(&fixture.shared, ["status", "--porcelain"]), "");
        assert_eq!(git(&fixture.shared, ["branch", "--show-current"]), "main");
        assert!(git(&fixture.shared, ["branch", "--list", &planned.branch]).is_empty());
        assert!(!fixture.shared.join(&planned.destination_rel).exists());
    }

    #[test]
    fn apply_refuses_shared_edits_after_planning_before_branch_creation() {
        let fixture = Fixture::new();
        let request = fixture.request();
        let planned = plan(&request).unwrap();
        std::fs::write(fixture.shared.join("README.md"), "user edit\n").unwrap();
        let reflog = std::fs::read(fixture.shared.join(".git/logs/HEAD")).unwrap();
        let result = super::apply(&request, &planned, "share", false);
        assert!(result.is_err());
        assert_eq!(
            std::fs::read(fixture.shared.join("README.md")).unwrap(),
            b"user edit\n"
        );
        assert_eq!(
            std::fs::read(fixture.shared.join(".git/logs/HEAD")).unwrap(),
            reflog,
            "apply created branch before stale shared-tree preflight"
        );
    }

    #[test]
    fn prospective_destination_does_not_shadow_selected_readme_link() {
        for original in ["Team/Guides/README.md", "Guides/README", "Team/Guides"] {
            let mut fixture = Fixture::new();
            let selected = Path::new("Guides/README.md");
            std::fs::create_dir_all(fixture.shared.join("Guides")).unwrap();
            let selected_bytes = std::fs::read(fixture.shared.join("README.md")).unwrap();
            std::fs::write(fixture.shared.join(selected), &selected_bytes).unwrap();
            git(&fixture.shared, ["add", "-A"]);
            git(
                &fixture.shared,
                ["commit", "-qm", "prospective identity fixture"],
            );
            let source_bytes = format!(
                "{}[[{original}#Heading|Chosen]]\n",
                std::fs::read_to_string(&fixture.note).unwrap()
            );
            let new_source = fixture.vault.join("Guides.md");
            std::fs::rename(&fixture.note, &new_source).unwrap();
            fixture.note = new_source;
            std::fs::write(&fixture.note, &source_bytes).unwrap();
            let request = fixture.request();
            let main = git(&fixture.shared, ["rev-parse", "main"]);
            let planned = plan(&request).unwrap();
            let destination = Path::new(&planned.destination_rel);
            assert_eq!(destination, Path::new("Knowledge/Architecture/Guides.md"));
            assert!(!fixture.shared.join(destination).exists());
            assert_eq!(git(&fixture.shared, ["rev-parse", "HEAD"]), main);
            assert_eq!(git(&fixture.shared, ["status", "--porcelain"]), "");
            assert_eq!(
                super::resolve_shared_target(&fixture.shared, destination, "Guides/README.md"),
                Some(selected.to_path_buf())
            );
            super::apply(&request, &planned, "share prospective identity", false).unwrap();
            assert!(crate::lint_shared_home(&fixture.shared).unwrap().is_empty());
            let emitted = std::fs::read_to_string(fixture.shared.join(destination)).unwrap();
            let targets = super::wikilinks(&emitted);
            assert_eq!(targets.len(), 1);
            assert_eq!(
                super::resolve_shared_target(&fixture.shared, destination, &targets[0]),
                Some(selected.to_path_buf()),
                "successful apply/lint must not redirect to the new destination"
            );
            let expected_spelling = if original == "Guides/README" {
                "Guides/README"
            } else {
                "Guides/README.md"
            };
            assert!(emitted.ends_with(&format!("[[{expected_spelling}#Heading|Chosen]]\n")));
            assert_eq!(
                std::fs::read_to_string(&fixture.note).unwrap(),
                source_bytes
            );
            assert_eq!(
                std::fs::read(fixture.shared.join(selected)).unwrap(),
                selected_bytes
            );
            assert_eq!(git(&fixture.shared, ["rev-parse", "main"]), main);
        }
    }

    #[test]
    fn promoted_readme_link_preserves_target_identity_through_plan_apply_and_lint() {
        for (shadow, suffix, prefix) in [
            (Some("Guides.md"), "", ""),
            (Some("Guides.md"), "#Heading|Alias", ""),
            (
                Some("Knowledge/Architecture/Guides.md"),
                "#^block|Alias",
                "Team/",
            ),
            (None, "#Heading|Alias", ""),
        ] {
            let fixture = Fixture::new();
            let selected = Path::new("Guides/README.md");
            std::fs::create_dir_all(fixture.shared.join("Guides")).unwrap();
            let selected_bytes = std::fs::read(fixture.shared.join("README.md")).unwrap();
            std::fs::write(fixture.shared.join(selected), &selected_bytes).unwrap();
            let shadow_bytes = shadow.map(|relative| {
                let bytes = [selected_bytes.as_slice(), b"Distinct shadow note.\n"].concat();
                std::fs::write(fixture.shared.join(relative), &bytes).unwrap();
                bytes
            });
            git(&fixture.shared, ["add", "-A"]);
            git(
                &fixture.shared,
                ["commit", "-qm", "README identity fixture"],
            );
            let original = format!("{prefix}Guides/README.md");
            let source_bytes = format!(
                "{}[[{original}{suffix}]]\n",
                std::fs::read_to_string(&fixture.note).unwrap()
            );
            std::fs::write(&fixture.note, &source_bytes).unwrap();
            let request = fixture.request();
            let main = git(&fixture.shared, ["rev-parse", "main"]);
            let planned = plan(&request).unwrap();
            assert_eq!(git(&fixture.shared, ["rev-parse", "HEAD"]), main);
            assert_eq!(git(&fixture.shared, ["status", "--porcelain"]), "");
            assert!(!fixture.shared.join(&planned.destination_rel).exists());
            let destination = Path::new(&planned.destination_rel);
            let selected_identity =
                super::resolve_shared_target(&fixture.shared, destination, "Guides/README.md")
                    .unwrap();
            assert_eq!(selected_identity, selected);
            super::apply(&request, &planned, "share README identity", false).unwrap();
            let lint = crate::lint_shared_home(&fixture.shared).unwrap();
            assert!(lint.is_empty(), "{shadow:?}: {lint:?}");
            let emitted = std::fs::read_to_string(fixture.shared.join(destination)).unwrap();
            assert_eq!(emitted, super::promoted_content(&request, &planned));
            assert_eq!(
                std::fs::read_to_string(&fixture.note).unwrap(),
                source_bytes
            );
            assert_eq!(git(&fixture.shared, ["rev-parse", "main"]), main);
            assert_eq!(git(&fixture.shared, ["status", "--porcelain"]), "");
            assert_eq!(
                std::fs::read(fixture.shared.join(selected)).unwrap(),
                selected_bytes
            );
            if let Some(relative) = shadow {
                assert_eq!(
                    std::fs::read(fixture.shared.join(relative)).unwrap(),
                    shadow_bytes.unwrap()
                );
            }
            let emitted_target = super::wikilinks(&emitted);
            assert_eq!(emitted_target.len(), 1);
            assert_eq!(
                super::resolve_shared_target(&fixture.shared, destination, &emitted_target[0]),
                Some(selected_identity),
                "{shadow:?}: plan/apply/lint succeeded but [[{}{suffix}]] changed the selected target",
                emitted_target[0]
            );
            let expected_target = if shadow.is_some() {
                "Guides/README.md"
            } else {
                "Guides"
            };
            assert!(
                emitted.ends_with(&format!("[[{expected_target}{suffix}]]\n")),
                "{emitted}"
            );
            assert!(planned.dropped_links.is_empty());
        }
    }

    #[test]
    fn inbound_directory_readme_resolves_relative_to_reader_and_preserves_anchor_alias() {
        let fixture = Fixture::new();
        let source = fixture.vault.join("Projects/A/guide/README.md");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, std::fs::read(&fixture.note).unwrap()).unwrap();
        let reader = fixture.vault.join("Projects/A/reader.md");
        std::fs::write(&reader, "[[guide#^block|Guide]]\n").unwrap();
        std::fs::remove_file(fixture.shared.join("Knowledge/Architecture/README.md")).unwrap();
        git(&fixture.shared, ["add", "-A"]);
        git(
            &fixture.shared,
            [
                "commit",
                "-qm",
                "remove destination collision for planner fixture",
            ],
        );
        let mut request = fixture.request();
        request.note = &source;
        let planned = plan(&request).unwrap();
        assert_eq!(
            planned
                .rewrites
                .iter()
                .filter(|rewrite| rewrite.file == "Projects/A/reader.md")
                .count(),
            1
        );
        assert_eq!(planned.rewrites[0].from, "guide");
    }

    #[test]
    fn planning_excludes_real_nested_shared_tree_from_personal_rewrites() {
        let fixture = Fixture::new();
        let nested = fixture.vault.join("shared-storage");
        std::fs::rename(&fixture.shared, &nested).unwrap();
        std::fs::write(nested.join("reader.md"), "[[note]]\n").unwrap();
        git(&nested, ["add", "."]);
        git(&nested, ["commit", "-qm", "reader"]);
        let mut request = fixture.request();
        request.shared = &nested;
        let planned = plan(&request).unwrap();
        assert!(
            planned
                .rewrites
                .iter()
                .all(|rewrite| !rewrite.file.starts_with("shared-storage/")),
            "{:?}",
            planned.rewrites
        );
    }

    #[cfg(unix)]
    #[test]
    fn source_removal_revalidates_intervening_replacement_and_rolls_back_rewrites() {
        use std::os::unix::fs::PermissionsExt;
        for replacement in ["directory", "symlink", "bytes"] {
            let fixture = Fixture::new();
            let parked = fixture.vault.join("parked-source");
            let sentinel = fixture.vault.join("outside-source");
            std::fs::write(&sentinel, "user owned").unwrap();
            let source_before = std::fs::read(&fixture.note).unwrap();
            let first = fixture.vault.join("Inbox/a.md");
            std::fs::set_permissions(&first, std::fs::Permissions::from_mode(0o600)).unwrap();
            let request = fixture.request();
            let planned = plan(&request).unwrap();
            let head = git(&fixture.shared, ["rev-parse", "HEAD"]);
            let mut calls = 0;
            let result = apply_impl(&request, &planned, "share", true, |path, bytes| {
                super::atomic_replace(path, bytes)?;
                calls += 1;
                if calls == planned.rewrites.len() {
                    std::fs::rename(&fixture.note, &parked).unwrap();
                    if replacement == "directory" {
                        std::fs::create_dir(&fixture.note).unwrap();
                    } else if replacement == "bytes" {
                        std::fs::write(&fixture.note, "intervening source edit").unwrap();
                    } else {
                        std::os::unix::fs::symlink(&sentinel, &fixture.note).unwrap();
                    }
                }
                Ok(())
            });
            assert!(result.is_err(), "{replacement}: source replacement removed");
            assert_eq!(std::fs::read(&first).unwrap(), b"See [[note]].\n");
            assert_eq!(
                std::fs::metadata(&first).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(std::fs::read(&parked).unwrap(), source_before);
            assert_eq!(std::fs::read(&sentinel).unwrap(), b"user owned");
            assert!(std::fs::symlink_metadata(&fixture.note).is_ok());
            assert_eq!(git(&fixture.shared, ["rev-parse", "HEAD"]), head);
            assert_eq!(git(&fixture.shared, ["status", "--porcelain"]), "");
            assert_eq!(git(&fixture.shared, ["branch", "--show-current"]), "main");
            assert!(git(&fixture.shared, ["branch", "--list", &planned.branch]).is_empty());
            assert!(!fixture.shared.join(&planned.destination_rel).exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn first_middle_last_rewrite_failures_restore_bytes_modes_types_and_source() {
        use std::os::unix::fs::PermissionsExt;
        for failure in 1..=3 {
            let fixture = Fixture::new();
            std::fs::write(
                fixture.vault.join("Inbox/m.md"),
                "Middle [[note#heading]].\n",
            )
            .unwrap();
            let paths = ["Inbox/a.md", "Inbox/m.md", "Inbox/z.md"];
            let before: Vec<_> = paths
                .iter()
                .enumerate()
                .map(|(i, rel)| {
                    let path = fixture.vault.join(rel);
                    let mode = if i == 1 { 0o640 } else { 0o600 };
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
                    (path.clone(), std::fs::read(path).unwrap(), mode)
                })
                .collect();
            let source = std::fs::read(&fixture.note).unwrap();
            let head = git(&fixture.shared, ["rev-parse", "HEAD"]);
            let request = fixture.request();
            let planned = plan(&request).unwrap();
            let mut calls = 0;
            let error = apply_impl(&request, &planned, "share", true, |path, bytes| {
                calls += 1;
                if calls == failure {
                    return Err(std::io::Error::other("position failure"));
                }
                super::atomic_replace(path, bytes)
            })
            .unwrap_err();
            assert!(format!("{error:#}").contains("position failure"));
            assert_eq!(calls, failure);
            for (path, bytes, mode) in before {
                assert_eq!(std::fs::read(&path).unwrap(), bytes);
                assert!(std::fs::symlink_metadata(&path).unwrap().is_file());
                assert_eq!(
                    std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                    mode
                );
            }
            assert_eq!(std::fs::read(&fixture.note).unwrap(), source);
            assert_eq!(git(&fixture.shared, ["rev-parse", "HEAD"]), head);
            assert_eq!(git(&fixture.shared, ["status", "--porcelain"]), "");
            assert_eq!(git(&fixture.shared, ["branch", "--show-current"]), "main");
            assert!(git(&fixture.shared, ["branch", "--list", &planned.branch]).is_empty());
            assert!(!fixture.shared.join(&planned.destination_rel).exists());
            // A fully restored transaction can be planned/applied again without stale state.
            let retry = plan(&request).unwrap();
            assert!(
                super::apply(&request, &retry, "retry share", true)
                    .unwrap()
                    .moved
            );
            assert!(!fixture.note.exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn personal_rollback_preserves_intervening_bytes_modes_and_symlink_replacements() {
        use std::os::unix::fs::PermissionsExt;
        for case in ["bytes", "mode", "symlink"] {
            let fixture = Fixture::new();
            let first = fixture.vault.join("Inbox/a.md");
            let sentinel = fixture.vault.join("sentinel");
            std::fs::write(&sentinel, "outside unchanged").unwrap();
            let request = fixture.request();
            let planned = plan(&request).unwrap();
            let mut calls = 0;
            let result = apply_impl(&request, &planned, "share", false, |path, bytes| {
                calls += 1;
                if calls == 2 {
                    match case {
                        "bytes" => std::fs::write(&first, "user edit").unwrap(),
                        "mode" => {
                            std::fs::set_permissions(&first, std::fs::Permissions::from_mode(0o400))
                                .unwrap()
                        }
                        "symlink" => {
                            std::fs::remove_file(&first).unwrap();
                            std::os::unix::fs::symlink(&sentinel, &first).unwrap();
                        }
                        _ => unreachable!(),
                    }
                    return Err(std::io::Error::other("injected failure"));
                }
                super::atomic_replace(path, bytes)
            });
            assert!(format!("{:#}", result.unwrap_err()).contains("rollback incomplete"));
            assert!(
                fixture.shared.join(&planned.destination_rel).exists(),
                "incomplete personal rollback must retain promotion for remaining links"
            );
            assert_eq!(std::fs::read(&sentinel).unwrap(), b"outside unchanged");
            match case {
                "bytes" => assert_eq!(std::fs::read(&first).unwrap(), b"user edit"),
                "mode" => assert_eq!(
                    std::fs::metadata(&first).unwrap().permissions().mode() & 0o777,
                    0o400
                ),
                "symlink" => assert!(std::fs::symlink_metadata(&first)
                    .unwrap()
                    .file_type()
                    .is_symlink()),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn rollback_preserves_shared_edits_and_reports_git_cleanup_failures() {
        for case in ["shared-edit", "git-lock", "destination-directory"] {
            let fixture = Fixture::new();
            let request = fixture.request();
            let planned = plan(&request).unwrap();
            let destination = fixture.shared.join(&planned.destination_rel);
            let result = apply_impl(&request, &planned, "share", false, |_, _| {
                match case {
                    "shared-edit" => {
                        std::fs::write(&destination, "intervening user edit\n").unwrap()
                    }
                    "git-lock" => {
                        std::fs::write(fixture.shared.join(".git/index.lock"), "user lock").unwrap()
                    }
                    "destination-directory" => {
                        std::fs::remove_file(&destination).unwrap();
                        std::fs::create_dir(&destination).unwrap();
                    }
                    _ => unreachable!(),
                }
                Err(std::io::Error::other("injected rewrite failure"))
            });
            let error = format!("{:#}", result.unwrap_err());
            assert!(error.contains("injected rewrite failure"));
            assert!(
                error.contains("rollback incomplete"),
                "{case}: cleanup failure discarded: {error}"
            );
            if case == "shared-edit" {
                assert_eq!(
                    std::fs::read(&destination).unwrap(),
                    b"intervening user edit\n"
                );
            }
            if case == "destination-directory" {
                assert!(destination.is_dir());
            }
            if case == "git-lock" {
                assert!(error.contains("git reset failed"), "{error}");
                assert_eq!(
                    std::fs::read(fixture.shared.join(".git/index.lock")).unwrap(),
                    b"user lock"
                );
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn share_rewrites_are_atomic_and_preserve_private_modes() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let fixture = Fixture::new();
        let first = fixture.vault.join("Inbox/a.md");
        std::fs::set_permissions(&first, std::fs::Permissions::from_mode(0o640)).unwrap();
        let alias = fixture.vault.join("original-inode");
        std::fs::hard_link(&first, &alias).unwrap();
        let request = fixture.request();
        let planned = plan(&request).unwrap();
        super::apply(&request, &planned, "share", false).unwrap();
        assert_eq!(
            std::fs::read(&alias).unwrap(),
            b"See [[note]].\n",
            "rewrite mutated original inode instead of atomic replacement"
        );
        assert_ne!(
            std::fs::metadata(&alias).unwrap().ino(),
            std::fs::metadata(&first).unwrap().ino()
        );
        assert_eq!(
            std::fs::metadata(&first).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[test]
    fn later_personal_rewrite_write_failure_restores_prior_rewrites_and_share_transaction() {
        let fixture = Fixture::new();
        let request = fixture.request();
        let planned = plan(&request).unwrap();
        assert_eq!(planned.rewrites.len(), 2);
        let first = fixture.vault.join("Inbox/a.md");
        let last = fixture.vault.join("Inbox/z.md");
        let first_before = b"See [[note]].\n";
        let last_before = b"Also see [[note|the note]].\n";
        let initial_head = git(&fixture.shared, ["rev-parse", "HEAD"]);
        let mut writes = 0;

        let result = apply_impl(&request, &planned, "share note", false, |path, bytes| {
            writes += 1;
            if writes == 2 {
                return Err(std::io::Error::other(
                    "injected later personal rewrite failure",
                ));
            }
            std::fs::write(path, bytes)
        });

        let error = result.expect_err("injected later-write failure was not surfaced");
        assert!(format!("{error:#}").contains("injected later personal rewrite failure"));
        assert_eq!(writes, 2);
        assert_eq!(std::fs::read(&first).unwrap(), first_before);
        assert_eq!(std::fs::read(&last).unwrap(), last_before);
        assert_eq!(git(&fixture.shared, ["rev-parse", "HEAD"]), initial_head);
        assert_eq!(git(&fixture.shared, ["branch", "--show-current"]), "main");
        assert!(git(&fixture.shared, ["branch", "--list", &planned.branch]).is_empty());
        assert!(!fixture.shared.join(&planned.destination_rel).exists());
        assert_eq!(git(&fixture.shared, ["status", "--porcelain"]), "");
    }
}
