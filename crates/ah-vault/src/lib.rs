//! Vault safety utilities and personal vault workflows.

use anyhow::Context;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Read-only view of a shared-home Git checkout. Remote refs are never refreshed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedHomeStatus {
    pub branch: String,
    pub upstream: Option<String>,
    pub clean: bool,
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullResult {
    pub old_head: String,
    pub new_head: String,
    pub upstream: String,
}

/// Inspect a shared-home checkout without fetching or changing any repository state.
pub fn shared_home_status(path: &Path) -> anyhow::Result<SharedHomeStatus> {
    let root = checkout_root(path)?;
    let branch = git_output(&root, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let upstream = git_output_optional(
        &root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )?;
    let clean = git_output(
        &root,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?
    .is_empty();
    let (ahead, behind) = if upstream.is_some() {
        let counts = git_output(
            &root,
            &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
        )?;
        let mut counts = counts.split_whitespace();
        let ahead = counts
            .next()
            .context("Git returned no ahead count")?
            .parse()?;
        let behind = counts
            .next()
            .context("Git returned no behind count")?
            .parse()?;
        (ahead, behind)
    } else {
        (0, 0)
    };
    Ok(SharedHomeStatus {
        branch,
        upstream,
        clean,
        ahead,
        behind,
    })
}

/// Fetch the configured upstream and fast-forward only a clean local branch.
/// Local changes and divergent histories are preserved and reported, never merged.
pub fn pull_shared_home(path: &Path) -> anyhow::Result<PullResult> {
    let root = checkout_root(path)?;
    let initial = shared_home_status(&root)?;
    if !initial.clean {
        anyhow::bail!(
            "shared home has uncommitted changes; commit or preserve them before pulling"
        );
    }
    let upstream = initial
        .upstream
        .context("shared-home branch has no configured upstream")?;
    let (remote, branch) = upstream
        .split_once('/')
        .context("configured upstream must have the form remote/branch")?;
    if remote == "." || remote.is_empty() || branch.is_empty() {
        anyhow::bail!("configured upstream must name a remote branch");
    }
    let remote_ref = format!("refs/remotes/{upstream}");
    let source_ref = format!("refs/heads/{branch}");
    let refspec = format!("+{source_ref}:{remote_ref}");
    git_output(&root, &["check-ref-format", &source_ref])?;
    let old_head = git_output(&root, &["rev-parse", "HEAD"])?;
    let old_remote_ref = git_output_optional(&root, &["rev-parse", "--verify", &remote_ref])?;
    if let Some(old_remote_ref) = old_remote_ref.as_deref() {
        let remote_relation = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args([
                "merge-base",
                "--is-ancestor",
                old_remote_ref,
                old_head.as_str(),
            ])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("starting git merge-base")?;
        if !remote_relation.success() {
            anyhow::bail!(
                "local branch has diverged from or rewound its recorded upstream; refusing pull"
            );
        }
    }
    git_output(&root, &["fetch", "--no-tags", remote, &refspec])?;
    let fetched_ref = git_output(&root, &["rev-parse", "--verify", &remote_ref])?;
    if let Some(old_remote_ref) = old_remote_ref.as_deref() {
        if old_remote_ref != fetched_ref {
            let remote_advance = Command::new("git")
                .arg("-C")
                .arg(&root)
                .args([
                    "merge-base",
                    "--is-ancestor",
                    old_remote_ref,
                    fetched_ref.as_str(),
                ])
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_TERMINAL_PROMPT", "0")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .context("starting git merge-base")?;
            if !remote_advance.success() {
                anyhow::bail!("shared-home upstream was rewound or diverged; refusing pull");
            }
        }
    }
    let current_head = git_output(&root, &["rev-parse", "HEAD"])?;
    if current_head != old_head {
        anyhow::bail!("shared-home HEAD changed while fetching; refusing to pull");
    }
    let still_clean = git_output(
        &root,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?
    .is_empty();
    if !still_clean {
        anyhow::bail!("shared home changed while fetching; refusing to pull");
    }
    let current_upstream = git_output_optional(
        &root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )?;
    if current_upstream.as_deref() != Some(upstream.as_str()) {
        anyhow::bail!("shared-home upstream changed while fetching; refusing to pull");
    }
    if old_remote_ref.is_some() {
        let ancestry = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["merge-base", "--is-ancestor", "HEAD", remote_ref.as_str()])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("starting git merge-base")?;
        if !ancestry.success() {
            anyhow::bail!("shared-home history is divergent or cannot be verified; refusing non-fast-forward pull");
        }
    }
    let still_clean = git_output(
        &root,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?
    .is_empty();
    if !still_clean {
        anyhow::bail!("shared home changed before fast-forward; refusing to pull");
    }
    let final_head = git_output(&root, &["rev-parse", "HEAD"])?;
    if final_head != old_head {
        anyhow::bail!("shared-home HEAD changed before fast-forward; refusing to pull");
    }
    git_output(&root, &["merge", "--ff-only", remote_ref.as_str()])?;
    let new_head = git_output(&root, &["rev-parse", "HEAD"])?;
    Ok(PullResult {
        old_head,
        new_head,
        upstream,
    })
}

fn checkout_root(path: &Path) -> anyhow::Result<PathBuf> {
    let root = path
        .canonicalize()
        .with_context(|| format!("shared-home path does not exist: {}", path.display()))?;
    let top = git_output(&root, &["rev-parse", "--show-toplevel"])?;
    let top = PathBuf::from(top).canonicalize()?;
    if root != top {
        anyhow::bail!(
            "shared-home path must be the Git checkout root: {}",
            root.display()
        );
    }
    Ok(root)
}

fn git_output(path: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .with_context(|| {
            format!(
                "starting git {}",
                args.first().copied().unwrap_or("command")
            )
        })?;
    if !output.status.success() {
        let diagnostic = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        anyhow::bail!(
            "git {} failed{}",
            args.first().copied().unwrap_or("command"),
            if diagnostic.is_empty() {
                String::new()
            } else {
                format!(": {diagnostic}")
            }
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn git_output_optional(path: &Path, args: &[&str]) -> anyhow::Result<Option<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .with_context(|| {
            format!(
                "starting git {}",
                args.first().copied().unwrap_or("command")
            )
        })?;
    if output.status.success() {
        Ok(Some(String::from_utf8(output.stdout)?.trim().to_owned()))
    } else {
        Ok(None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretFinding {
    pub path: std::path::PathBuf,
    pub line: usize,
}

/// A stable, secret-safe lint diagnostic. `message` contains rule-specific prose only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintFinding {
    pub path: std::path::PathBuf,
    pub line: usize,
    pub rule: &'static str,
    pub message: &'static str,
}

/// Check shared-home structure and Markdown links without including document text.
pub fn lint_shared_home(root: &std::path::Path) -> anyhow::Result<Vec<LintFinding>> {
    use std::collections::BTreeSet;
    use walkdir::WalkDir;

    if !root.is_dir() {
        anyhow::bail!("shared-home root is not a directory");
    }
    let mut findings = Vec::new();
    let mut files = Vec::new();
    let mut directories = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(root)?.to_path_buf();
        if entry.file_type().is_symlink() {
            continue;
        }
        if entry.file_type().is_dir() {
            directories.push(relative);
        } else if entry.file_type().is_file() {
            files.push((
                relative,
                entry.path().to_path_buf(),
                entry.metadata()?.len(),
            ));
        }
    }
    let mut all_paths: BTreeSet<String> = files
        .iter()
        .map(|(p, _, _)| p.to_string_lossy().replace('\\', "/"))
        .collect();
    all_paths.extend(
        directories
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/")),
    );
    let markdown: BTreeSet<String> = files
        .iter()
        .filter(|(p, _, _)| p.extension().is_some_and(|e| e == "md"))
        .map(|(p, _, _)| p.to_string_lossy().replace('\\', "/"))
        .collect();
    for directory in &directories {
        let mut components = directory.components();
        let area = components
            .next()
            .and_then(|c| c.as_os_str().to_str())
            .is_some_and(|p| matches!(p, "Knowledge" | "Decisions" | "Templates"));
        if !area || components.next().is_none() {
            continue;
        }
        if !files.iter().any(|(p, _, _)| {
            p.parent() == Some(directory.as_path())
                && p.file_name().is_some_and(|n| n == "README.md")
        }) {
            findings.push(LintFinding {
                path: directory.clone(),
                line: 0,
                rule: "VSH001",
                message: "directory is missing README.md",
            });
        }
    }
    for (relative, full, size) in &files {
        let rel = relative.to_string_lossy().replace('\\', "/");
        if *size > 1_000_000 {
            findings.push(LintFinding {
                path: relative.clone(),
                line: 0,
                rule: "VSH002",
                message: "file exceeds the 1 MB shared-home size limit",
            });
            continue;
        }
        if rel.starts_with(".git/")
            || rel.starts_with(".obsidian/")
            || relative.extension().is_none_or(|e| e != "md")
        {
            continue;
        }
        let bytes = std::fs::read(full)?;
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        if is_policy_markdown(relative) {
            findings.extend(lint_frontmatter(relative, text));
        }
        for (i, line) in text.lines().enumerate() {
            let line_no = i + 1;
            let lower = line.to_ascii_lowercase();
            if line.contains("/mnt/")
                || line.contains("/home/")
                || lower.contains("c:/users/")
                || lower.contains("\\\\users\\\\")
            {
                findings.push(LintFinding {
                    path: relative.clone(),
                    line: line_no,
                    rule: "VSH003",
                    message: "personal filesystem path found",
                });
            }
            if email_address_pattern()
                .find_iter(line)
                .any(|m| valid_email_match(line, m.start(), m.end()))
            {
                findings.push(LintFinding {
                    path: relative.clone(),
                    line: line_no,
                    rule: "VSH007",
                    message: "email address found",
                });
            }
            let mut rest = line;
            while let Some(start) = rest.find("[[") {
                let after = &rest[start + 2..];
                let Some(end) = after.find("]]") else {
                    break;
                };
                let target = after[..end]
                    .split('|')
                    .next()
                    .unwrap_or("")
                    .split('#')
                    .next()
                    .unwrap_or("")
                    .trim();
                if !target.is_empty() {
                    let resolved = if std::path::Path::new(target).extension().is_some() {
                        target.to_owned()
                    } else {
                        format!("{target}.md")
                    };
                    let candidate = relative
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new(""))
                        .join(&resolved)
                        .to_string_lossy()
                        .replace('\\', "/");
                    let rooted = resolved.replace('\\', "/");
                    let alias = candidate
                        .strip_suffix("/README.md")
                        .or_else(|| candidate.strip_suffix(".md"))
                        .unwrap_or(&candidate);
                    let rooted_alias = rooted
                        .strip_suffix("/README.md")
                        .or_else(|| rooted.strip_suffix(".md"))
                        .unwrap_or(&rooted);
                    if !markdown.contains(&candidate)
                        && !markdown.contains(&rooted)
                        && !all_paths.contains(&candidate)
                        && !all_paths.contains(&rooted)
                        && !all_paths.contains(alias)
                        && !all_paths.contains(rooted_alias)
                    {
                        findings.push(LintFinding {
                            path: relative.clone(),
                            line: line_no,
                            rule: "VSH004",
                            message: "wikilink target does not resolve within the shared home",
                        });
                    }
                }
                rest = &after[end + 2..];
            }
        }
    }
    for finding in scan_secrets(root)? {
        if let Ok(relative) = finding.path.strip_prefix(root) {
            findings.push(LintFinding {
                path: relative.to_path_buf(),
                line: finding.line,
                rule: "VSH006",
                message: "potential credential pattern detected",
            });
        }
    }
    findings.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.line.cmp(&b.line))
            .then(a.rule.cmp(b.rule))
    });
    Ok(findings)
}

fn is_policy_markdown(path: &std::path::Path) -> bool {
    path.components()
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .is_some_and(|area| matches!(area, "Knowledge" | "Decisions" | "Templates"))
}

fn lint_frontmatter(path: &std::path::Path, text: &str) -> Vec<LintFinding> {
    let mut findings = Vec::new();
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        let is_area_index = path.file_name().is_some_and(|name| name == "README.md")
            && path
                .parent()
                .is_some_and(|parent| parent.components().count() == 1);
        if !is_area_index {
            findings.push(LintFinding {
                path: path.to_path_buf(),
                line: 1,
                rule: "VSH009",
                message: "required YAML frontmatter is missing",
            });
        }
        return findings;
    }
    let mut yaml_lines = Vec::new();
    let mut closed = false;
    for line in lines {
        if line == "---" {
            closed = true;
            break;
        }
        yaml_lines.push(line);
    }
    if !closed {
        findings.push(LintFinding {
            path: path.to_path_buf(),
            line: 1,
            rule: "VSH008",
            message: "YAML frontmatter is not closed",
        });
        return findings;
    }
    let yaml = yaml_lines.join("\n");
    // Templates may use unquoted date placeholders that are not valid YAML scalars.
    let parse_yaml = yaml
        .lines()
        .map(|line| {
            if let Some((key, value)) = line.split_once(':') {
                let value = value.trim();
                if value.starts_with("{{") && value.ends_with("}}") {
                    return format!("{key}: \"{value}\"");
                }
            }
            line.to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let value: serde_yaml::Value = match serde_yaml::from_str(&parse_yaml) {
        Ok(value) => value,
        Err(error) => {
            findings.push(LintFinding {
                path: path.to_path_buf(),
                line: error.location().map(|loc| loc.line() + 1).unwrap_or(2),
                rule: "VSH008",
                message: "YAML frontmatter is invalid",
            });
            return findings;
        }
    };
    let Some(mapping) = value.as_mapping() else {
        findings.push(LintFinding {
            path: path.to_path_buf(),
            line: 2,
            rule: "VSH008",
            message: "YAML frontmatter must be a mapping",
        });
        return findings;
    };
    let field = |name: &str| mapping.get(serde_yaml::Value::String(name.to_owned()));
    let is_template = path
        .components()
        .next()
        .is_some_and(|c| c.as_os_str() == "Templates");
    for name in [
        "title",
        "created",
        "updated",
        "type",
        "area",
        "tags",
        "owner",
        "authority",
    ]
    .into_iter()
    .filter(|name| !is_template || !matches!(*name, "owner" | "authority"))
    {
        let value = field(name);
        let valid_shape = match name {
            "title" | "created" | "updated" | "type" | "area" | "owner" | "authority" => {
                value.is_some_and(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()))
            }
            "tags" => value.is_some_and(serde_yaml::Value::is_sequence),
            _ => true,
        };
        if !valid_shape {
            findings.push(LintFinding {
                path: path.to_path_buf(),
                line: 1,
                rule: "VSH009",
                message: "required frontmatter field is missing or invalid",
            });
            break;
        }
    }
    if is_template {
        return findings;
    }
    if let Some(authority) = field("authority").and_then(serde_yaml::Value::as_str) {
        if !matches!(authority, "working" | "mirror" | "digest") {
            findings.push(LintFinding {
                path: path.to_path_buf(),
                line: 1,
                rule: "VSH010",
                message: "frontmatter authority must be working, mirror, or digest",
            });
        } else if matches!(authority, "mirror" | "digest")
            && field("source").is_none_or(serde_yaml::Value::is_null)
        {
            findings.push(LintFinding {
                path: path.to_path_buf(),
                line: 1,
                rule: "VSH011",
                message: "mirror and digest notes require a source field",
            });
        }
    }
    findings
}

fn valid_email_match(line: &str, start: usize, end: usize) -> bool {
    let candidate = &line[start..end];
    let Some((local, domain)) = candidate.rsplit_once('@') else {
        return false;
    };
    let local = local.trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
    let Some((_, tld)) = domain.rsplit_once('.') else {
        return false;
    };
    !local.is_empty()
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && domain
            .split('.')
            .all(|label| !label.is_empty() && !label.starts_with('-') && !label.ends_with('-'))
        && tld.chars().all(|c| c.is_ascii_alphabetic())
        && !line[end..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

fn email_address_pattern() -> &'static regex::Regex {
    static EMAIL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    EMAIL.get_or_init(|| {
        regex::Regex::new(r"(?i)(?:^|[^\w.+-])[a-z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-z0-9-]+(?:\.[a-z0-9-]+)*\.[a-z]{2,63}")
            .expect("the email-address pattern is a compile-time constant")
    })
}

/// Scan text files for common high-confidence credential patterns. Findings omit matched text.
pub fn scan_secrets(root: &std::path::Path) -> anyhow::Result<Vec<SecretFinding>> {
    use regex::Regex;
    use walkdir::WalkDir;
    let patterns = [
        Regex::new(r"\bgh[pousr]_[A-Za-z0-9]{30,}\b")?,
        Regex::new(r"\bgithub_pat_[A-Za-z0-9_]{30,}\b")?,
        Regex::new(r"\bsk-[A-Za-z0-9_-]{24,}\b")?,
        Regex::new(r"(?i)\b(password|client_secret|api_key)\s*[:=]\s*[^\s#]{8,}")?,
        Regex::new(r"-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----")?,
    ];
    let mut found = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_symlink()
            || !entry.file_type().is_file()
            || entry.metadata()?.len() > 2_000_000
        {
            continue;
        }
        let bytes = std::fs::read(entry.path())?;
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            if patterns.iter().any(|pattern| pattern.is_match(line)) {
                found.push(SecretFinding {
                    path: entry.path().to_path_buf(),
                    line: index + 1,
                });
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_credential_matches_without_including_the_secret_value() {
        let root = tempfile::tempdir().unwrap();
        let note = root.path().join("note.md");
        let secret = "ghp_1234567890abcdefghijklmnopqrstuvwxyzABC";
        std::fs::write(&note, format!("safe text\n{secret}\n")).unwrap();
        let findings = scan_secrets(root.path()).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].path, note);
        assert_eq!(findings[0].line, 2);
        assert!(!format!("{:?}", findings[0]).contains(secret));
    }
}
