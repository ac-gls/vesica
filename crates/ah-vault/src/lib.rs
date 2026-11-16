//! Vault safety utilities and personal vault workflows.

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
