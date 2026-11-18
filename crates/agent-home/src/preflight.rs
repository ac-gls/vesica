//! Read-only Linux host preflight for `vesica doctor` (plan §5.4 install step 1).
//!
//! Checks only what can be verified locally without credentials or network access: platform
//! identity, presence of required/optional commands on `PATH`, and the team's minimum Node
//! version. Missing optional integrations are reported separately from blockers.

use std::path::{Path, PathBuf};

/// Commands the Linux install/update path relies on (plan §5.4 step 1).
pub const REQUIRED_COMMANDS: [&str; 4] = ["git", "gh", "curl", "sha256sum"];

#[derive(Debug, Clone)]
pub struct Platform {
    pub os: &'static str,
    pub arch: &'static str,
    pub kernel: String,
    pub distro: String,
}

#[derive(Debug, Clone)]
pub struct Preflight {
    pub platform: Platform,
    /// (command, "found" | "missing")
    pub required: Vec<(&'static str, &'static str)>,
    /// (integration, status text)
    pub optional: Vec<(&'static str, String)>,
    pub blockers: Vec<String>,
}

impl Preflight {
    pub fn run(minimum_node: Option<&str>) -> Self {
        let platform = Platform {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            kernel: read_trimmed("/proc/sys/kernel/osrelease").unwrap_or_else(|| "unknown".into()),
            distro: os_release_pretty_name().unwrap_or_else(|| "unknown".into()),
        };
        let mut blockers = Vec::new();
        if platform.os != "linux" {
            blockers.push(format!("unsupported operating system: {}", platform.os));
        }
        if platform.arch != "x86_64" {
            blockers.push(format!("unsupported architecture: {}", platform.arch));
        }
        let mut required = Vec::new();
        for command in REQUIRED_COMMANDS {
            if find_in_path(command).is_some() {
                required.push((command, "found"));
            } else {
                required.push((command, "missing"));
                blockers.push(format!("required command not found: {command}"));
            }
        }
        let az = if find_in_path("az").is_some() {
            "found".to_owned()
        } else {
            "missing".to_owned()
        };
        let optional = vec![("az", az), ("node", node_status(minimum_node))];
        Self {
            platform,
            required,
            optional,
            blockers,
        }
    }

    pub fn ok(&self) -> bool {
        self.blockers.is_empty()
    }
}

fn read_trimmed(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

fn os_release_pretty_name() -> Option<String> {
    let text = std::fs::read_to_string("/etc/os-release").ok()?;
    text.lines().find_map(|line| {
        line.strip_prefix("PRETTY_NAME=")
            .map(|value| value.trim().trim_matches('"').to_owned())
    })
}

/// First executable regular file named `name` on `PATH` (no shell, nothing executed).
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(name))
        .find(|candidate| {
            std::fs::metadata(candidate)
                .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        })
}

fn node_status(minimum: Option<&str>) -> String {
    let Some(node) = find_in_path("node") else {
        return "missing".into();
    };
    let Some(version) = command_version(&node) else {
        return "found: version unknown".into();
    };
    match minimum.and_then(parse_version) {
        Some(min) => match parse_version(&version) {
            Some(actual) if actual < min => format!(
                "too old: {version} < {}",
                minimum.unwrap_or_default().trim_start_matches('v')
            ),
            Some(_) => format!("found: {version}"),
            None => format!("found: unparseable version {version}"),
        },
        None => format!("found: {version}"),
    }
}

fn command_version(program: &Path) -> Option<String> {
    let output = std::process::Command::new(program)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    Some(line.trim_start_matches('v').to_owned()).filter(|v| !v.is_empty())
}

/// Parse `major.minor.patch` (missing parts default to 0; a leading `v` is ignored).
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim().trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = parts
        .next()
        .map_or(Some(0), |p| p.split(['-', '+']).next()?.parse().ok())?;
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn parses_and_orders_versions() {
        assert_eq!(parse_version("v22.15.0"), Some((22, 15, 0)));
        assert_eq!(parse_version("22"), Some((22, 0, 0)));
        assert_eq!(parse_version("20.1.2-rc.1"), Some((20, 1, 2)));
        assert_eq!(parse_version("node"), None);
        assert!(parse_version("18.19.0") < parse_version("22.0.0"));
    }
}
