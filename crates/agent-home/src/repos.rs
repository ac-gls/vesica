//! Install step 10 (plan §5.4): clone missing profile repositories into a repos directory.
//!
//! Safety rules:
//! - Opt-in: nothing happens without `--repos-dir`.
//! - The `core` profile never clones (plan: repositories belong to `developer` and `lead`).
//! - An existing destination is never modified, whatever it contains; it is reported instead.
//! - A clone goes into a temporary sibling directory and is renamed into place only after
//!   `git clone` succeeds, so a failure leaves no partial checkout.
//! - Clones are not manifest entries: `uninstall` keeps repositories (plan §5.4).

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoState {
    /// Destination is absent; it will be cloned.
    Clone,
    /// Destination is a Git checkout whose `origin` is the configured URL.
    Present,
    /// Destination exists but is not a matching checkout; it is left untouched.
    Skipped(String),
}

#[derive(Debug, Clone)]
pub struct RepoStep {
    pub name: String,
    pub url: String,
    pub destination: PathBuf,
    pub state: RepoState,
}

/// Read-only planning: inspects destinations but never creates or changes anything.
pub fn plan(repositories: &[ah_core::RepoConfig], repos_dir: &Path) -> Result<Vec<RepoStep>> {
    match std::fs::metadata(repos_dir) {
        Ok(meta) if !meta.is_dir() => {
            anyhow::bail!(
                "repos directory is not a directory: {}",
                repos_dir.display()
            )
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context(format!("inspecting {}", repos_dir.display())),
    }
    repositories
        .iter()
        .map(|repo| {
            let destination = repos_dir.join(&repo.name);
            let state = match std::fs::symlink_metadata(&destination) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => RepoState::Clone,
                Err(error) => {
                    return Err(error).context(format!("inspecting {}", destination.display()))
                }
                Ok(_) => match origin_url(&destination) {
                    Some(origin) if origin == repo.url.trim() => RepoState::Present,
                    Some(origin) => RepoState::Skipped(format!(
                        "existing checkout has a different origin ({origin})"
                    )),
                    None => {
                        RepoState::Skipped("destination exists and is not a Git checkout".into())
                    }
                },
            };
            Ok(RepoStep {
                name: repo.name.clone(),
                url: repo.url.trim().to_owned(),
                destination,
                state,
            })
        })
        .collect()
}

/// `origin` URL of a checkout rooted exactly at `dir`, if it is one.
fn origin_url(dir: &Path) -> Option<String> {
    if !dir.join(".git").exists() {
        return None;
    }
    let output = git()
        .arg("-C")
        .arg(dir)
        .args(["config", "--get", "remote.origin.url"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git() -> std::process::Command {
    let mut command = std::process::Command::new("git");
    // Never prompt (would hang an unattended install) and never allow command transports.
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .args([
            "-c",
            "protocol.ext.allow=never",
            "-c",
            "protocol.fd.allow=never",
        ])
        .stdin(std::process::Stdio::null());
    command
}

/// Clone one planned repository. Returns an error describing the failure; never leaves a
/// partial destination behind.
pub fn clone(step: &RepoStep, repos_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(repos_dir)
        .with_context(|| format!("creating repos directory {}", repos_dir.display()))?;
    let staging = tempfile::Builder::new()
        .prefix(&format!(".{}.vesica-clone-", step.name))
        .tempdir_in(repos_dir)
        .context("creating a temporary clone directory")?;
    let checkout = staging.path().join("checkout");
    let output = git()
        .args(["clone", "--quiet", "--"])
        .arg(&step.url)
        .arg(&checkout)
        .output()
        .context("running git clone")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr.lines().last().unwrap_or("git clone failed").trim();
        anyhow::bail!("{reason}");
    }
    // `rename` fails if the destination appeared meanwhile as a file or a non-empty directory
    // (an empty directory may be replaced, losing nothing), so user content is never
    // overwritten. The staging directory is removed when `staging` drops.
    std::fs::rename(&checkout, &step.destination).with_context(|| {
        format!(
            "moving the clone into {} (did it appear during install?)",
            step.destination.display()
        )
    })?;
    Ok(())
}
