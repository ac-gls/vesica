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
    let file_paths: BTreeSet<String> = files
        .iter()
        .map(|(relative, _, _)| relative.to_string_lossy().replace('\\', "/"))
        .collect();
    let markdown_paths: BTreeSet<String> = files
        .iter()
        .filter(|(relative, _, _)| relative.extension().is_some_and(|ext| ext == "md"))
        .map(|(relative, _, _)| relative.to_string_lossy().replace('\\', "/"))
        .collect();

    for directory in &directories {
        let components: Vec<String> = directory
            .components()
            .filter_map(|component| component.as_os_str().to_str().map(str::to_owned))
            .collect();
        let is_area = components
            .first()
            .is_some_and(|part| matches!(part.as_str(), "Knowledge" | "Decisions" | "Templates"));
        if !is_area || components.len() < 2 {
            continue;
        }
        let has_readme = files.iter().any(|(path, _, _)| {
            path.parent() == Some(directory.as_path())
                && path.file_name().is_some_and(|name| name == "README.md")
        });
        if !has_readme {
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
        let lower = rel.to_ascii_lowercase();
        if lower.starts_with(".git/") || lower.starts_with(".obsidian/") {
            continue;
        }
        if relative.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        let bytes = std::fs::read(full)?;
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        for (line_index, line) in text.lines().enumerate() {
            let line_no = line_index + 1;
            if line.contains("/mnt/") || line.contains("/home/") || line.contains("C:\\\\Users\\\\")
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
                    let target_path = std::path::Path::new(target);
                    let resolved = if target_path.extension().is_some() {
                        target.to_owned()
                    } else {
                        format!("{target}.md")
                    };
                    let parent = relative
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new(""));
                    let candidate = parent.join(&resolved).to_string_lossy().replace('\\', "/");
                    let rooted = std::path::Path::new(&resolved)
                        .to_string_lossy()
                        .replace('\\', "/");
                    if !markdown_paths.contains(&candidate)
                        && !markdown_paths.contains(&rooted)
                        && !file_paths.contains(&candidate)
                        && !file_paths.contains(&rooted)
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
