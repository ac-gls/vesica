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
    let Ok(root) = isolated(dir, &["rev-parse", "--show-toplevel"]) else {
        return false;
    };
    let Ok(root) = Path::new(root.trim()).canonicalize() else {
        return false;
    };
    let Ok(dir) = dir.canonicalize() else {
        return false;
    };
    root == dir
}

/// Verify mutation storage belongs to this root. Linked worktrees are supported for read-only
/// queries, but mutation requires a real in-root `.git` directory (no gitfile/separate git-dir).
pub fn validate_mutation_repository(dir: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(dir.join(".git"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() || !is_repository(dir) {
        anyhow::bail!(
            "Git mutation requires a real in-root .git directory; linked/separate worktrees are read-only"
        );
    }
    let effective = isolated(dir, &["rev-parse", "--absolute-git-dir"])?;
    if Path::new(effective.trim()).canonicalize()? != dir.join(".git").canonicalize()? {
        anyhow::bail!("effective Git directory does not belong to selected repository");
    }
    Ok(())
}

/// Construct a Git command with repository-selection environment removed.
pub fn command(dir: &Path, args: &[&str], isolated: bool) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null());
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_CEILING_DIRECTORIES",
        "GIT_PREFIX",
        "GIT_SHALLOW_FILE",
    ] {
        command.env_remove(key);
    }
    if isolated {
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null");
    }
    command
}

fn run(dir: &Path, args: &[&str], isolated: bool) -> Result<String> {
    // Structural queries work in linked worktrees. Our mutators require independently
    // verifiable in-root storage, rather than trusting a gitfile or symlink as authority.
    if args.first().is_some_and(|arg| {
        matches!(
            *arg,
            "add"
                | "commit"
                | "checkout"
                | "switch"
                | "reset"
                | "branch"
                | "fetch"
                | "merge"
                | "pull"
                | "push"
                | "rm"
        )
    }) {
        validate_mutation_repository(dir)?;
    }
    let output = command(dir, args, isolated)
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
