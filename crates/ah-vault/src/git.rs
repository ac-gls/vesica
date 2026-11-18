//! Git helpers shared by the vault and shared-home commands.
//!
//! `user` keeps the person's own configuration (identity, hooks) and is used for anything that
//! creates commits; `isolated` blanks system and global config, so read-only structural checks
//! and pulls behave the same everywhere.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn user(dir: &Path, args: &[&str]) -> Result<String> {
    run(dir, args, false)
}

pub fn isolated(dir: &Path, args: &[&str]) -> Result<String> {
    run(dir, args, true)
}

/// True when `dir` is the root of its own Git working tree.
pub fn is_repository(dir: &Path) -> bool {
    dir.join(".git").exists()
}

fn run(dir: &Path, args: &[&str], isolated: bool) -> Result<String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null());
    if isolated {
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null");
    }
    let output = command
        .output()
        .with_context(|| format!("starting git {}", args[0]))?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args[0],
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}
