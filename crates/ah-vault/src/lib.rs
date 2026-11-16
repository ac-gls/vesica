//! Vault safety utilities and personal vault workflows.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretFinding {
    pub path: std::path::PathBuf,
    pub line: usize,
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
        if !entry.file_type().is_file() || entry.metadata()?.len() > 2_000_000 {
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
