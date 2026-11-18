//! Release mechanics for `vesica update` (plan §5.4 command table, §5.5 release section).
//!
//! Everything here follows the same contract as `install.sh` and `scripts/build-release.sh`:
//! the artifact's SHA-256 must match the entry in `SHA256SUMS`, and the extracted binary must
//! report the tag's version. Anything unexpected fails closed and leaves the installed binary
//! untouched.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Release channels: `alpha` includes pre-releases (all that exist so far), `stable` doesn't.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Alpha,
    Stable,
}

impl Channel {
    pub fn text(self) -> &'static str {
        match self {
            Self::Alpha => "alpha",
            Self::Stable => "stable",
        }
    }
}

pub const DEFAULT_BASE_URL: &str = "https://github.com/ac-gls/vesica/releases/download";
pub const DEFAULT_RELEASES_API: &str = "https://api.github.com/repos/ac-gls/vesica/releases";
pub const ASSET_SUFFIX: &str = "-x86_64-linux.tar.gz";

/// `vX.Y.Z` or `vX.Y.Z-pre.N`, as produced by `scripts/build-release.sh`.
pub fn validate_tag(tag: &str) -> Result<()> {
    let rest = tag.strip_prefix('v').unwrap_or("");
    let (core, suffix) = match rest.split_once('-') {
        Some((core, suffix)) => (core, Some(suffix)),
        None => (rest, None),
    };
    let core_ok = core.split('.').count() == 3
        && core
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    let suffix_ok = suffix.is_none_or(|s| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
    });
    if tag.is_empty() || !core_ok || !suffix_ok {
        anyhow::bail!("tag must look like vX.Y.Z or vX.Y.Z-pre.N (got {tag:?})");
    }
    Ok(())
}

pub fn asset_name(tag: &str) -> String {
    format!("vesica-{tag}{ASSET_SUFFIX}")
}

/// Newest published release tag, honouring the channel. Drafts are never considered.
pub fn resolve_tag(api_url: &str, channel: Channel) -> Result<String> {
    let body = fetch(api_url, None)?;
    let releases: Vec<serde_json::Value> = serde_json::from_str(&body)
        .with_context(|| format!("cannot parse the release list from {api_url}"))?;
    let newest = releases
        .iter()
        .filter(|release| release["draft"] != serde_json::Value::Bool(true))
        .filter(|release| {
            channel == Channel::Alpha || release["prerelease"] != serde_json::Value::Bool(true)
        })
        .max_by_key(|release| {
            release["published_at"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .and_then(|release| release["tag_name"].as_str())
        .with_context(|| {
            format!(
                "no published release found for the {} channel",
                channel.text()
            )
        })?;
    validate_tag(newest)?;
    Ok(newest.to_owned())
}

/// `curl -fsSL` a URL. `curl` is a required command (install step 1), so no HTTP crate is needed.
fn fetch(url: &str, dest: Option<&Path>) -> Result<String> {
    let mut command = Command::new("curl");
    command.args(["-fsSL"]).arg(url);
    if let Some(dest) = dest {
        command.arg("-o").arg(dest);
    }
    // Optional token: the public API allows only 60 anonymous requests per hour.
    if let Ok(token) = std::env::var("GITHUB_TOKEN").or_else(|_| std::env::var("GH_TOKEN")) {
        command
            .arg("-H")
            .arg(format!("Authorization: Bearer {token}"));
    }
    let output = command
        .stdin(Stdio::null())
        .output()
        .with_context(|| "starting curl (install step 1 requires curl on PATH)".to_owned())?;
    if !output.status.success() {
        anyhow::bail!(
            "download failed: {} ({})",
            url,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}

/// Download the release artifact into `staging`, using a `file://` base in tests.
pub fn download(staging: &Path, base_url: &str, tag: &str) -> Result<PathBuf> {
    let asset = asset_name(tag);
    let tarball = staging.join(&asset);
    fetch(&format!("{base_url}/{tag}/{asset}"), Some(&tarball))?;
    fetch(
        &format!("{base_url}/{tag}/SHA256SUMS"),
        Some(&staging.join("SHA256SUMS")),
    )?;
    println!("Downloaded {asset} ({tag})");
    Ok(tarball)
}

/// The recorded SHA-256 for `asset` must equal the artifact's. Fails closed otherwise.
pub fn verify_checksum(staging: &Path, tarball: &Path) -> Result<()> {
    let asset = tarball
        .file_name()
        .context("artifact has no file name")?
        .to_string_lossy()
        .into_owned();
    let sums = std::fs::read_to_string(staging.join("SHA256SUMS")).context("reading SHA256SUMS")?;
    let expected = sums
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let hash = fields.next()?;
            let name = fields.next()?.trim_start_matches('*');
            (name == asset).then(|| hash.to_owned())
        })
        .next();
    let Some(expected) = expected else {
        anyhow::bail!("SHA256SUMS has no checksum for {asset}");
    };
    let bytes = std::fs::read(tarball).with_context(|| format!("reading {}", tarball.display()))?;
    let actual = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&bytes));
    if actual != expected.to_ascii_lowercase() {
        anyhow::bail!("checksum mismatch for {asset}: refusing to replace the binary");
    }
    println!("checksum verified for {asset} (SHA-256)");
    Ok(())
}

/// Extract the single `vesica` binary from the tarball.
pub fn extract(staging: &Path, tarball: &Path, tag: &str) -> Result<PathBuf> {
    let out = Command::new("tar")
        .arg("-xzf")
        .arg(tarball)
        .arg("-C")
        .arg(staging)
        .stdin(Stdio::null())
        .output()
        .with_context(|| "starting tar (the release artifact is a .tar.gz)".to_owned())?;
    if !out.status.success() {
        anyhow::bail!(
            "extracting {}: {}",
            tarball.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let binary = staging.join("vesica");
    if !binary.is_file() {
        anyhow::bail!("{} does not contain a vesica binary", asset_name(tag));
    }
    Ok(binary)
}

/// The released binary must report the tag's version; a matching checksum alone is not enough.
pub fn check_reports_version(binary: &Path, tag: &str) -> Result<()> {
    let expected = format!("vesica {}", tag.trim_start_matches('v'));
    let out = Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("running {}", binary.display()))?;
    let actual = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if actual != expected {
        anyhow::bail!("the released binary reports '{actual}', expected '{expected}'");
    }
    println!("Version check: {actual}");
    Ok(())
}

/// Replace `target` with `binary`, atomically (same directory, so `rename` is atomic).
pub fn replace(target: &Path, binary: &Path) -> Result<()> {
    let parent = target.parent().context("target has no parent directory")?;
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let mut perms = std::fs::metadata(binary)?.permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(binary, perms)?;
    std::fs::rename(binary, target).with_context(|| format!("replacing {}", target.display()))?;
    Ok(())
}
