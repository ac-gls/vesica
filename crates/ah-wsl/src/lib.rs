//! WSL detection and Windows interop: user profile, drvfs paths, OneDrive check, schtasks, winget, obsidian.

/// Detect WSL from the kernel release, with an explicit CI override for tests.
pub fn is_wsl_with(kernel_release: &str, allow_non_wsl: Option<&str>) -> bool {
    allow_non_wsl == Some("1") || {
        let value = kernel_release.to_ascii_lowercase();
        value.contains("microsoft") || value.contains("wsl")
    }
}

pub fn is_wsl() -> anyhow::Result<bool> {
    let release = std::fs::read_to_string("/proc/sys/kernel/osrelease")?;
    Ok(is_wsl_with(
        &release,
        std::env::var("AGENT_HOME_ALLOW_NON_WSL").ok().as_deref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_wsl_and_rejects_non_wsl_by_default() {
        assert!(is_wsl_with(
            "Linux version 6.6.1-microsoft-standard-WSL2",
            None
        ));
        assert!(!is_wsl_with("Linux version 6.6.1-generic", None));
        assert!(is_wsl_with("Linux version 6.6.1-generic", Some("1")));
    }
}
