use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Action {
    CreateDir {
        path: String,
    },
    WriteFile {
        path: String,
        content: String,
    },
    Symlink {
        source: String,
        destination: String,
    },
    RemovePath {
        path: String,
    },
    UpsertBlock {
        path: String,
        style: String,
        name: String,
        body: String,
    },
    MergeJsonc {
        path: String,
        patch: serde_json::Value,
    },
    MergeToml {
        path: String,
        patch: serde_json::Value,
    },
}

impl Action {
    pub fn key(&self) -> &str {
        match self {
            Self::CreateDir { path }
            | Self::WriteFile { path, .. }
            | Self::RemovePath { path }
            | Self::UpsertBlock { path, .. }
            | Self::MergeJsonc { path, .. }
            | Self::MergeToml { path, .. } => path,
            Self::Symlink { destination, .. } => destination,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionOutcome {
    Created,
    Updated,
    Removed,
    AlreadyCorrect,
    Collision,
    DryRun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub actions: Vec<Action>,
    pub dry_run: bool,
}

#[derive(Debug, Default)]
pub struct Planner {
    actions: BTreeMap<String, Action>,
}

impl Planner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, action: Action) -> anyhow::Result<()> {
        let key = action.key().to_owned();
        if let Some(existing) = self.actions.get(&key) {
            if existing != &action {
                anyhow::bail!("conflicting planned changes for {key}");
            }
            return Ok(());
        }
        self.actions.insert(key, action);
        Ok(())
    }

    pub fn finish(self, dry_run: bool) -> Plan {
        Plan {
            actions: self.actions.into_values().collect(),
            dry_run,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    thread_local! {
        pub(super) static AFTER_ACTION_HOOK: Cell<usize> = const { Cell::new(0) };
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum ReplacementStage {
        Write,
        Sync,
        Rename,
    }

    thread_local! {
        static REPLACEMENT_FAILURE: Cell<Option<ReplacementStage>> = const { Cell::new(None) };
    }

    struct ReplacementFailureGuard;

    impl ReplacementFailureGuard {
        fn at(stage: ReplacementStage) -> Self {
            REPLACEMENT_FAILURE.set(Some(stage));
            Self
        }
    }

    impl Drop for ReplacementFailureGuard {
        fn drop(&mut self) {
            REPLACEMENT_FAILURE.set(None);
        }
    }

    pub(super) fn fail_replacement_at(stage: ReplacementStage) -> std::io::Result<()> {
        if REPLACEMENT_FAILURE.get() == Some(stage) {
            REPLACEMENT_FAILURE.set(None);
            return Err(std::io::Error::other(format!("injected {stage:?} failure")));
        }
        Ok(())
    }

    fn assert_replacement_failure_is_atomic(stage: ReplacementStage) {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.toml");
        let original = b"# private synthetic config\r\nvalue = 1";
        std::fs::write(&target, original).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
        }
        let original_metadata = std::fs::symlink_metadata(&target).unwrap();
        let _guard = ReplacementFailureGuard::at(stage);

        let error = atomic_replace_file(&target, b"replacement synthetic config").unwrap_err();

        assert!(
            error
                .to_string()
                .contains(&format!("injected {stage:?} failure")),
            "{error:#}"
        );
        assert_eq!(std::fs::read(&target).unwrap(), original);
        let metadata = std::fs::symlink_metadata(&target).unwrap();
        assert!(metadata.is_file() && !metadata.file_type().is_symlink());
        assert_eq!(metadata.permissions(), original_metadata.permissions());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                metadata.ino(),
                original_metadata.ino(),
                "original inode replaced on failure"
            );
        }
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "owned temp leaked"
        );
    }

    #[test]
    fn atomic_replacement_write_failure_preserves_original_and_cleans_owned_temp() {
        assert_replacement_failure_is_atomic(ReplacementStage::Write);
    }

    #[test]
    fn atomic_replacement_sync_failure_preserves_original_and_cleans_owned_temp() {
        assert_replacement_failure_is_atomic(ReplacementStage::Sync);
    }

    #[test]
    fn atomic_replacement_rename_failure_preserves_original_and_cleans_owned_temp() {
        assert_replacement_failure_is_atomic(ReplacementStage::Rename);
    }

    #[cfg(unix)]
    #[test]
    fn executor_rollback_preserves_intervening_permission_edit_and_reports_it() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.toml");
        std::fs::write(&target, b"# private\nvalue = 1\n").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        let plan = Plan {
            actions: vec![Action::MergeToml {
                path: target.display().to_string(),
                patch: serde_json::json!({"managed": true}),
            }],
            dry_run: false,
        };
        let mut applied = Vec::new();
        let error = Executor
            .execute_with_commit(&plan, |_| {
                applied = std::fs::read(&target)?;
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640))?;
                anyhow::bail!("persist rejected")
            })
            .unwrap_err();

        assert!(
            error.to_string().contains("rollback also failed"),
            "{error:#}"
        );
        assert_eq!(
            std::fs::read(&target).unwrap(),
            applied,
            "user permission edit must retain post-apply bytes"
        );
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn executor_restore_uses_original_permissions_not_applied_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, b"# private\nvalue = 1\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        let action = Action::MergeToml {
            path: path.display().to_string(),
            patch: serde_json::json!({"managed": true}),
        };
        let before = snapshot_for_rollback(&action).unwrap();
        Executor.execute_one(&action, false).unwrap();
        // Model a forward writer with a more restrictive applied mode. Restore
        // must use the preimage mode, not whatever inode is currently installed.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let after = snapshot_after_apply(&action).unwrap();
        Executor
            .rollback_one(
                &action,
                &ActionOutcome::Updated,
                before.as_ref(),
                after.as_ref(),
            )
            .unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"# private\nvalue = 1\n");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    thread_local! {
        static TEMP_CANDIDATES: std::cell::RefCell<std::collections::VecDeque<std::path::PathBuf>> = const {
            std::cell::RefCell::new(std::collections::VecDeque::new())
        };
    }

    pub(super) fn next_temp_candidate() -> Option<std::path::PathBuf> {
        TEMP_CANDIDATES.with(|candidates| candidates.borrow_mut().pop_front())
    }

    #[cfg(unix)]
    fn assert_temp_collision_is_untouched(kind: &str) {
        use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.toml");
        let collision = dir.path().join("forced-candidate.tmp");
        let outside = dir.path().join("outside");
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(&outside, b"outside sentinel").unwrap();
        match kind {
            "regular" => std::fs::write(&collision, b"unmanaged temp").unwrap(),
            "live" => symlink(&outside, &collision).unwrap(),
            "dangling" => symlink(dir.path().join("absent"), &collision).unwrap(),
            "directory" => {
                std::fs::create_dir(&collision).unwrap();
                std::fs::write(collision.join("sentinel"), b"directory sentinel").unwrap();
            }
            _ => unreachable!(),
        }
        let metadata = std::fs::symlink_metadata(&collision).unwrap();
        TEMP_CANDIDATES.with(|candidates| candidates.borrow_mut().push_back(collision.clone()));

        atomic_replace_file(&target, b"new").unwrap();

        assert!(
            TEMP_CANDIDATES.with(|candidates| candidates.borrow().is_empty()),
            "collision was not actually attempted"
        );
        let after = std::fs::symlink_metadata(&collision).unwrap();
        assert_eq!(after.ino(), metadata.ino());
        assert_eq!(after.file_type(), metadata.file_type());
        assert_eq!(after.permissions().mode(), metadata.permissions().mode());
        match kind {
            "regular" => assert_eq!(std::fs::read(&collision).unwrap(), b"unmanaged temp"),
            "live" => assert_eq!(std::fs::read_link(&collision).unwrap(), outside),
            "dangling" => {
                assert_eq!(
                    std::fs::read_link(&collision).unwrap(),
                    dir.path().join("absent")
                );
                assert!(!dir.path().join("absent").exists());
            }
            "directory" => assert_eq!(
                std::fs::read(collision.join("sentinel")).unwrap(),
                b"directory sentinel"
            ),
            _ => unreachable!(),
        }
        assert_eq!(std::fs::read(&outside).unwrap(), b"outside sentinel");
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert!(std::fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_file());
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            3,
            "owned temp leaked"
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replacement_leaves_regular_temp_collision_untouched() {
        assert_temp_collision_is_untouched("regular");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replacement_leaves_live_symlink_temp_collision_untouched() {
        assert_temp_collision_is_untouched("live");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replacement_leaves_dangling_symlink_temp_collision_untouched() {
        assert_temp_collision_is_untouched("dangling");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replacement_leaves_directory_temp_collision_untouched() {
        assert_temp_collision_is_untouched("directory");
    }

    #[cfg(unix)]
    #[test]
    fn executor_replacement_and_rollback_preserve_modes_across_umasks() {
        for umask in ["000", "022", "077"] {
            for mode in ["600", "640"] {
                let output = std::process::Command::new("sh")
                    .args(["-c", "umask \"$1\"; exec \"$2\" --exact action::tests::executor_permission_matrix_worker --nocapture", "sh", umask])
                    .arg(std::env::current_exe().unwrap())
                    .env("VESICA_CORE_PERMISSION_MATRIX_MODE", mode)
                    .output().unwrap();
                assert!(
                    output.status.success(),
                    "umask={umask} mode={mode}: {} {}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
                println!("umask={umask} original_mode={mode}: replacement / commit rollback / later-action rollback passed");
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn executor_permission_matrix_worker() {
        use std::os::unix::fs::PermissionsExt;
        let Ok(mode) = std::env::var("VESICA_CORE_PERMISSION_MATRIX_MODE") else {
            return;
        };
        let mode = u32::from_str_radix(&mode, 8).unwrap();
        for kind in ["block", "jsonc", "toml"] {
            for failure in ["commit", "later-action"] {
                let dir = tempfile::tempdir().unwrap();
                let target = dir.path().join("settings");
                let original = match kind {
                    "block" => "# private synthetic config\r\nexport VALUE=1",
                    "jsonc" => "{\n  // private synthetic config\n  \"value\": 1\n}\n",
                    "toml" => "# private synthetic config\nvalue = 1\n",
                    _ => unreachable!(),
                };
                std::fs::write(&target, original).unwrap();
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode)).unwrap();
                let action = match kind {
                    "block" => Action::UpsertBlock {
                        path: target.display().to_string(),
                        style: "hash".into(),
                        name: "test".into(),
                        body: "managed value".into(),
                    },
                    "jsonc" => Action::MergeJsonc {
                        path: target.display().to_string(),
                        patch: serde_json::json!({"managed": true}),
                    },
                    "toml" => Action::MergeToml {
                        path: target.display().to_string(),
                        patch: serde_json::json!({"managed": true}),
                    },
                    _ => unreachable!(),
                };
                let plan = Plan {
                    actions: vec![action.clone()],
                    dry_run: false,
                };
                let mut executor_actions = plan.actions.clone();
                let error = if failure == "commit" {
                    Executor
                        .execute_with_commit(&plan, |_| {
                            assert!(std::fs::symlink_metadata(&target)?.is_file());
                            assert_eq!(
                                std::fs::metadata(&target)?.permissions().mode() & 0o777,
                                mode
                            );
                            assert_ne!(std::fs::read(&target)?, original.as_bytes());
                            anyhow::bail!("persist rejected")
                        })
                        .unwrap_err()
                } else {
                    let blocked = dir.path().join("blocked");
                    std::fs::write(&blocked, b"obstruction").unwrap();
                    executor_actions.push(Action::WriteFile {
                        path: blocked.join("child").display().to_string(),
                        content: "unreachable".into(),
                    });
                    Executor
                        .execute(&Plan {
                            actions: executor_actions,
                            dry_run: false,
                        })
                        .unwrap_err()
                };
                assert!(
                    !error.to_string().contains("rollback also failed"),
                    "{error:#}"
                );
                assert_eq!(std::fs::read(&target).unwrap(), original.as_bytes());
                let metadata = std::fs::symlink_metadata(&target).unwrap();
                assert!(metadata.is_file() && !metadata.file_type().is_symlink());
                assert_eq!(metadata.permissions().mode() & 0o777, mode);
                assert_eq!(
                    std::fs::read_dir(dir.path()).unwrap().count(),
                    if failure == "commit" { 1 } else { 2 }
                );

                let new_target = dir.path().join("new-private-config");
                atomic_replace_file(&new_target, b"new private synthetic config").unwrap();
                assert_eq!(
                    std::fs::metadata(&new_target).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn executor_rollback_retains_changed_target_types_and_outside_sentinels() {
        use std::os::unix::fs::symlink;
        for kind in ["live", "dangling", "directory"] {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("settings.toml");
            let outside = dir.path().join("outside");
            std::fs::write(&target, b"value = 1\n").unwrap();
            let plan = Plan {
                actions: vec![Action::MergeToml {
                    path: target.display().to_string(),
                    patch: serde_json::json!({"managed": true}),
                }],
                dry_run: false,
            };
            let mut outside_expected = Vec::new();
            let error = Executor
                .execute_with_commit(&plan, |_| {
                    let installed = std::fs::read(&target)?;
                    outside_expected = installed.clone();
                    std::fs::remove_file(&target)?;
                    match kind {
                        "live" => {
                            std::fs::write(&outside, installed)?;
                            symlink(&outside, &target)?;
                        }
                        "dangling" => symlink(&outside, &target)?,
                        "directory" => {
                            std::fs::create_dir(&target)?;
                            std::fs::write(target.join("sentinel"), b"directory sentinel")?;
                        }
                        _ => unreachable!(),
                    }
                    anyhow::bail!("persist rejected")
                })
                .unwrap_err();
            assert!(
                error.to_string().contains("rollback also failed"),
                "{error:#}"
            );
            if kind == "directory" {
                assert!(std::fs::symlink_metadata(&target).unwrap().is_dir());
                assert_eq!(
                    std::fs::read(target.join("sentinel")).unwrap(),
                    b"directory sentinel"
                );
            } else {
                assert_eq!(std::fs::read_link(&target).unwrap(), outside);
                if kind == "live" {
                    assert_eq!(std::fs::read(&outside).unwrap(), outside_expected);
                } else {
                    assert!(!outside.exists());
                }
            }
            assert_eq!(
                std::fs::read_dir(dir.path()).unwrap().count(),
                if kind == "live" { 2 } else { 1 }
            );
        }
    }

    #[test]
    fn executor_restore_failure_reports_primary_and_rollback_errors_without_temp_leaks() {
        for stage in [
            ReplacementStage::Write,
            ReplacementStage::Sync,
            ReplacementStage::Rename,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("settings.toml");
            std::fs::write(&target, b"value = 1\n").unwrap();
            let plan = Plan {
                actions: vec![Action::MergeToml {
                    path: target.display().to_string(),
                    patch: serde_json::json!({"managed": true}),
                }],
                dry_run: false,
            };
            let mut installed = Vec::new();
            let error = Executor
                .execute_with_commit(&plan, |_| {
                    installed = std::fs::read(&target)?;
                    REPLACEMENT_FAILURE.set(Some(stage));
                    anyhow::bail!("persist rejected")
                })
                .unwrap_err();
            assert!(error.to_string().contains("persist rejected"), "{error:#}");
            assert!(
                error.to_string().contains("rollback also failed"),
                "{error:#}"
            );
            assert!(
                error
                    .to_string()
                    .contains(&format!("injected {stage:?} failure")),
                "{error:#}"
            );
            assert_eq!(std::fs::read(&target).unwrap(), installed);
            assert!(std::fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_file());
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn atomic_replacement_failure_keeps_absent_target_absent() {
        for stage in [
            ReplacementStage::Write,
            ReplacementStage::Sync,
            ReplacementStage::Rename,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("new-private-config");
            let _guard = ReplacementFailureGuard::at(stage);
            let error = atomic_replace_file(&target, b"synthetic private config").unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains(&format!("injected {stage:?} failure")),
                "{error:#}"
            );
            assert!(std::fs::symlink_metadata(&target).is_err());
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        }
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replacement_refuses_nonregular_targets_without_mutation() {
        use std::os::unix::fs::symlink;
        for kind in ["live", "dangling", "directory"] {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("settings");
            let outside = dir.path().join("outside");
            match kind {
                "live" => {
                    std::fs::write(&outside, b"outside sentinel").unwrap();
                    symlink(&outside, &target).unwrap();
                }
                "dangling" => symlink(&outside, &target).unwrap(),
                "directory" => {
                    std::fs::create_dir(&target).unwrap();
                    std::fs::write(target.join("sentinel"), b"directory sentinel").unwrap();
                }
                _ => unreachable!(),
            }
            let error = atomic_replace_file(&target, b"replacement").unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("replacement target must be a regular file"),
                "{error:#}"
            );
            if kind == "directory" {
                assert!(std::fs::symlink_metadata(&target).unwrap().is_dir());
                assert_eq!(
                    std::fs::read(target.join("sentinel")).unwrap(),
                    b"directory sentinel"
                );
            } else {
                assert_eq!(std::fs::read_link(&target).unwrap(), outside);
                if kind == "live" {
                    assert_eq!(std::fs::read(&outside).unwrap(), b"outside sentinel");
                } else {
                    assert!(!outside.exists());
                }
            }
            assert_eq!(
                std::fs::read_dir(dir.path()).unwrap().count(),
                if kind == "live" { 2 } else { 1 }
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn executor_refuses_symlinked_config_targets_before_actions_or_commit() {
        use std::os::unix::fs::symlink;
        for kind in ["live", "dangling"] {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("settings");
            let outside = dir.path().join("outside");
            if kind == "live" {
                std::fs::write(&outside, b"{}").unwrap();
            }
            symlink(&outside, &target).unwrap();
            for action in [
                Action::UpsertBlock {
                    path: target.display().to_string(),
                    style: "hash".into(),
                    name: "test".into(),
                    body: "body".into(),
                },
                Action::MergeJsonc {
                    path: target.display().to_string(),
                    patch: serde_json::json!({"managed": true}),
                },
                Action::MergeToml {
                    path: target.display().to_string(),
                    patch: serde_json::json!({"managed": true}),
                },
            ] {
                let error = Executor
                    .execute_with_commit(
                        &Plan {
                            actions: vec![action],
                            dry_run: false,
                        },
                        |_| panic!("commit must not run"),
                    )
                    .unwrap_err();
                assert!(
                    error
                        .to_string()
                        .contains("snapshot target must be a regular file"),
                    "{error:#}"
                );
                assert_eq!(std::fs::read_link(&target).unwrap(), outside);
                if kind == "live" {
                    assert_eq!(std::fs::read(&outside).unwrap(), b"{}");
                } else {
                    assert!(!outside.exists());
                }
                assert_eq!(
                    std::fs::read_dir(dir.path()).unwrap().count(),
                    if kind == "live" { 2 } else { 1 }
                );
            }
        }
    }

    #[test]
    fn atomic_replacement_preserves_existing_mode_and_ignores_temp_collisions() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::{symlink, PermissionsExt};

            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("settings.toml");
            std::fs::write(&target, b"old").unwrap();
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
            let collision = target.with_extension("agent-home.tmp");
            std::fs::write(&collision, b"unmanaged temp").unwrap();

            write_replacement(&target, b"new").unwrap();

            assert_eq!(std::fs::read(&target).unwrap(), b"new");
            assert_eq!(std::fs::read(&collision).unwrap(), b"unmanaged temp");
            assert_eq!(
                std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                0o640
            );
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);

            let outside = dir.path().join("outside");
            std::fs::write(&outside, b"outside sentinel").unwrap();
            let link_collision = dir.path().join("other.agent-home.tmp");
            symlink(&outside, &link_collision).unwrap();
            let second_target = dir.path().join("other");
            std::fs::write(&second_target, b"before").unwrap();
            write_replacement(&second_target, b"after").unwrap();
            assert_eq!(std::fs::read(&outside).unwrap(), b"outside sentinel");
            assert!(link_collision.is_symlink());
        }
    }

    #[test]
    fn commit_failure_rolls_back_created_and_updated_actions() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let created = dir.path().join("created.txt");
        let existing = dir.path().join("existing.toml");
        fs::write(&existing, "value = \"original\"\n").unwrap();
        let plan = Plan {
            actions: vec![
                Action::WriteFile {
                    path: created.to_string_lossy().into_owned(),
                    content: "new bytes".into(),
                },
                Action::MergeToml {
                    path: existing.to_string_lossy().into_owned(),
                    patch: serde_json::json!({"managed": true}),
                },
            ],
            dry_run: false,
        };

        let error = Executor
            .execute_with_commit(&plan, |_| anyhow::bail!("persist rejected"))
            .unwrap_err();

        assert!(error.to_string().contains("persist rejected"));
        assert!(!created.exists());
        assert_eq!(fs::read(existing).unwrap(), b"value = \"original\"\n");
    }

    #[test]
    fn commit_failure_during_dry_run_returns_error_without_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dry-run.txt");
        let plan = Plan {
            actions: vec![Action::WriteFile {
                path: path.to_string_lossy().into_owned(),
                content: "not applied".into(),
            }],
            dry_run: true,
        };

        let error = Executor
            .execute_with_commit(&plan, |_| anyhow::bail!("dry-run report rejected"))
            .unwrap_err();

        assert!(error.to_string().contains("dry-run report rejected"));
        assert!(!path.exists());
    }

    #[test]
    fn commit_failure_preserves_intervening_user_edit_and_reports_it() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("created.txt");
        let plan = Plan {
            actions: vec![Action::WriteFile {
                path: path.to_string_lossy().into_owned(),
                content: "vesica content".into(),
            }],
            dry_run: false,
        };

        let error = Executor
            .execute_with_commit(&plan, |_| {
                fs::write(&path, "user edit during commit")?;
                anyhow::bail!("persist rejected")
            })
            .unwrap_err();

        assert!(error.to_string().contains("persist rejected"));
        assert!(error.to_string().contains("rollback also failed"));
        assert_eq!(fs::read(path).unwrap(), b"user edit during commit");
    }

    #[test]
    fn successful_commit_keeps_applied_actions() {
        use std::fs;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("created.txt");
        let plan = Plan {
            actions: vec![Action::WriteFile {
                path: path.to_string_lossy().into_owned(),
                content: "committed".into(),
            }],
            dry_run: false,
        };

        let outcomes = Executor.execute_with_commit(&plan, |_| Ok(())).unwrap();

        assert_eq!(outcomes, vec![ActionOutcome::Created]);
        assert_eq!(fs::read(path).unwrap(), b"committed");
    }

    #[test]
    fn planner_sorts_and_deduplicates_actions_deterministically() {
        let mut planner = Planner::new();
        let first = Action::CreateDir { path: "/b".into() };
        planner.push(first.clone()).unwrap();
        planner.push(first).unwrap();
        planner
            .push(Action::CreateDir { path: "/a".into() })
            .unwrap();
        let plan = planner.finish(true);
        assert!(plan.dry_run);
        assert_eq!(plan.actions.len(), 2);
        assert_eq!(plan.actions[0].key(), "/a");
    }

    #[test]
    fn dry_run_records_actions_without_creating_files() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("nested/managed.txt");
        let plan = Plan {
            actions: vec![Action::WriteFile {
                path: target.display().to_string(),
                content: "managed".into(),
            }],
            dry_run: true,
        };
        let results = Executor.execute(&plan).unwrap();
        assert_eq!(results, vec![ActionOutcome::DryRun]);
        assert!(!target.exists());
    }

    #[test]
    fn execution_is_idempotent_and_never_overwrites_an_unmanaged_file() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("managed.txt");
        let path = target.display().to_string();
        let executor = Executor;
        let plan = Plan {
            actions: vec![Action::WriteFile {
                path: path.clone(),
                content: "managed".into(),
            }],
            dry_run: false,
        };
        assert_eq!(
            executor.execute(&plan).unwrap(),
            vec![ActionOutcome::Created]
        );
        assert_eq!(
            executor.execute(&plan).unwrap(),
            vec![ActionOutcome::AlreadyCorrect]
        );

        std::fs::write(&target, "user content").unwrap();
        assert_eq!(
            executor.execute(&plan).unwrap(),
            vec![ActionOutcome::Collision]
        );
        assert_eq!(std::fs::read_to_string(target).unwrap(), "user content");
    }

    #[test]
    fn created_outcome_serializes_with_stable_kebab_case_name() {
        assert_eq!(
            serde_json::to_string(&ActionOutcome::Created).unwrap(),
            "\"created\""
        );
    }

    #[test]
    fn updated_outcome_serializes_with_stable_kebab_case_name() {
        assert_eq!(
            serde_json::to_string(&ActionOutcome::Updated).unwrap(),
            "\"updated\""
        );
    }

    #[test]
    fn apply_failure_does_not_remove_user_replacement_of_created_file() {
        let temp = tempfile::tempdir().unwrap();
        let created = temp.path().join("created.txt");
        let blocked = temp.path().join("blocked");
        std::fs::write(&blocked, "not a directory").unwrap();
        AFTER_ACTION_HOOK.set(1);

        let plan = Plan {
            actions: vec![
                Action::WriteFile {
                    path: created.display().to_string(),
                    content: "managed".into(),
                },
                Action::WriteFile {
                    path: blocked.join("cannot-write.txt").display().to_string(),
                    content: "unreachable".into(),
                },
            ],
            dry_run: false,
        };
        let error = Executor.execute(&plan).unwrap_err().to_string();
        assert!(error.contains("rollback also failed"), "{error}");
        assert_eq!(std::fs::read_to_string(created).unwrap(), "user changed it");
    }

    #[test]
    fn apply_failure_does_not_remove_user_replacement_of_created_symlink() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let link = temp.path().join("link");
        let blocked = temp.path().join("blocked");
        std::fs::write(&source, "source").unwrap();
        std::fs::write(&blocked, "not a directory").unwrap();
        AFTER_ACTION_HOOK.set(2);
        let plan = Plan {
            actions: vec![
                Action::Symlink {
                    source: source.display().to_string(),
                    destination: link.display().to_string(),
                },
                Action::WriteFile {
                    path: blocked.join("cannot-write.txt").display().to_string(),
                    content: "unreachable".into(),
                },
            ],
            dry_run: false,
        };

        let error = Executor.execute(&plan).unwrap_err().to_string();
        assert!(error.contains("rollback also failed"), "{error}");
        assert_eq!(std::fs::read_to_string(link).unwrap(), "user file");
    }

    #[test]
    fn apply_failure_does_not_clobber_user_edit_during_config_rollback() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("settings.jsonc");
        let blocked = temp.path().join("blocked");
        std::fs::write(&config, "{}\n").unwrap();
        std::fs::write(&blocked, "not a directory").unwrap();
        let plan = Plan {
            actions: vec![
                Action::MergeJsonc {
                    path: config.display().to_string(),
                    patch: serde_json::json!({"managed": true}),
                },
                Action::WriteFile {
                    path: blocked.join("cannot-write.txt").display().to_string(),
                    content: "unreachable".into(),
                },
            ],
            dry_run: false,
        };
        AFTER_ACTION_HOOK.set(3);
        let error = Executor.execute(&plan).unwrap_err().to_string();
        assert!(error.contains("rollback also failed"), "{error}");
        assert_eq!(
            std::fs::read_to_string(config).unwrap(),
            "{\"user\": true}\n"
        );
    }

    #[test]
    fn apply_failure_rolls_back_prior_created_file() {
        let temp = tempfile::tempdir().unwrap();
        let created = temp.path().join("first.txt");
        let blocked = temp.path().join("blocked");
        std::fs::write(&blocked, "not a directory").unwrap();
        let plan = Plan {
            actions: vec![
                Action::WriteFile {
                    path: created.display().to_string(),
                    content: "managed".into(),
                },
                Action::WriteFile {
                    path: blocked.join("cannot-write.txt").display().to_string(),
                    content: "unreachable".into(),
                },
            ],
            dry_run: false,
        };

        assert!(Executor.execute(&plan).is_err());
        assert!(!created.exists(), "prior mutation was not rolled back");
        assert_eq!(
            std::fs::read_to_string(&blocked).unwrap(),
            "not a directory"
        );
    }

    #[test]
    fn apply_failure_preserves_preexisting_files_while_rolling_back() {
        let temp = tempfile::tempdir().unwrap();
        let existing = temp.path().join("existing.txt");
        let blocked = temp.path().join("blocked");
        std::fs::write(&existing, "user-owned").unwrap();
        std::fs::write(&blocked, "not a directory").unwrap();
        let plan = Plan {
            actions: vec![
                Action::WriteFile {
                    path: existing.display().to_string(),
                    content: "replacement".into(),
                },
                Action::WriteFile {
                    path: blocked.join("cannot-write.txt").display().to_string(),
                    content: "unreachable".into(),
                },
            ],
            dry_run: false,
        };

        assert!(Executor.execute(&plan).is_err());
        assert_eq!(std::fs::read_to_string(&existing).unwrap(), "user-owned");
    }

    #[test]
    fn apply_failure_preserves_already_correct_preexisting_file() {
        let temp = tempfile::tempdir().unwrap();
        let existing = temp.path().join("existing.txt");
        let blocked = temp.path().join("blocked");
        std::fs::write(&existing, "managed").unwrap();
        std::fs::write(&blocked, "not a directory").unwrap();
        let plan = Plan {
            actions: vec![
                Action::WriteFile {
                    path: existing.display().to_string(),
                    content: "managed".into(),
                },
                Action::WriteFile {
                    path: blocked.join("cannot-write.txt").display().to_string(),
                    content: "unreachable".into(),
                },
            ],
            dry_run: false,
        };

        assert!(Executor.execute(&plan).is_err());
        assert_eq!(std::fs::read_to_string(existing).unwrap(), "managed");
    }

    #[test]
    fn apply_failure_restores_updated_toml_exactly() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("settings.toml");
        let blocked = temp.path().join("blocked");
        let original = "# preserve comment\n[existing]\nvalue = 1\n";
        std::fs::write(&config, original).unwrap();
        std::fs::write(&blocked, "not a directory").unwrap();
        let plan = Plan {
            actions: vec![
                Action::MergeToml {
                    path: config.display().to_string(),
                    patch: serde_json::json!({"managed": {"enabled": true}}),
                },
                Action::WriteFile {
                    path: blocked.join("cannot-write.txt").display().to_string(),
                    content: "unreachable".into(),
                },
            ],
            dry_run: false,
        };

        assert!(Executor.execute(&plan).is_err());
        assert_eq!(std::fs::read(&config).unwrap(), original.as_bytes());
    }

    #[test]
    fn apply_failure_preserves_user_owned_symlink_and_target() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        let blocked = temp.path().join("blocked");
        std::fs::write(&target, "user target").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &source).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&target, &source).unwrap();
        std::fs::write(&blocked, "not a directory").unwrap();
        let plan = Plan {
            actions: vec![
                Action::Symlink {
                    source: source.display().to_string(),
                    destination: temp.path().join("existing-link").display().to_string(),
                },
                Action::WriteFile {
                    path: blocked.join("cannot-write.txt").display().to_string(),
                    content: "unreachable".into(),
                },
            ],
            dry_run: false,
        };
        assert!(Executor.execute(&plan).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"user target");
    }

    #[test]
    fn apply_failure_restores_updated_file_byte_for_byte() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("settings.jsonc");
        let blocked = temp.path().join("blocked");
        let original = "{\n  // keep comment\n  \"existing\": true\n}\n";
        std::fs::write(&config, original).unwrap();
        std::fs::write(&blocked, "not a directory").unwrap();
        let plan = Plan {
            actions: vec![
                Action::MergeJsonc {
                    path: config.display().to_string(),
                    patch: serde_json::json!({"managed": true}),
                },
                Action::WriteFile {
                    path: blocked.join("cannot-write.txt").display().to_string(),
                    content: "unreachable".into(),
                },
            ],
            dry_run: false,
        };

        assert!(Executor.execute(&plan).is_err());
        assert_eq!(
            std::fs::read_to_string(&config).unwrap().as_bytes(),
            original.as_bytes()
        );
    }

    #[test]
    fn apply_failure_removes_created_symlink_and_empty_parent_dir() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let nested_dir = temp.path().join("created-dir");
        let link = nested_dir.join("link");
        let blocked = temp.path().join("blocked");
        std::fs::write(&source, "source").unwrap();
        std::fs::write(&blocked, "not a directory").unwrap();
        let plan = Plan {
            actions: vec![
                Action::CreateDir {
                    path: nested_dir.display().to_string(),
                },
                Action::Symlink {
                    source: source.display().to_string(),
                    destination: link.display().to_string(),
                },
                Action::WriteFile {
                    path: blocked.join("cannot-write.txt").display().to_string(),
                    content: "unreachable".into(),
                },
            ],
            dry_run: false,
        };

        assert!(Executor.execute(&plan).is_err());
        assert!(!link.exists() && !link.is_symlink());
        assert!(!nested_dir.exists());
        assert_eq!(std::fs::read_to_string(source).unwrap(), "source");
    }

    #[test]
    fn apply_failure_removes_config_file_created_by_merge() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("new-settings.jsonc");
        let blocked = temp.path().join("blocked");
        std::fs::write(&blocked, "not a directory").unwrap();
        let plan = Plan {
            actions: vec![
                Action::MergeJsonc {
                    path: config.display().to_string(),
                    patch: serde_json::json!({"managed": true}),
                },
                Action::WriteFile {
                    path: blocked.join("cannot-write.txt").display().to_string(),
                    content: "unreachable".into(),
                },
            ],
            dry_run: false,
        };

        assert!(Executor.execute(&plan).is_err());
        assert!(!config.exists());
    }

    #[test]
    fn rejects_removal_until_manifest_checked_uninstall_is_implemented() {
        let plan = Plan {
            actions: vec![Action::RemovePath {
                path: "/tmp/not-allowed".into(),
            }],
            dry_run: false,
        };
        assert!(Executor.execute(&plan).is_err());
    }
}

#[derive(Debug)]
struct Snapshot {
    bytes: Vec<u8>,
    permissions: Option<std::fs::Permissions>,
}

impl Snapshot {
    fn regular_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            anyhow::bail!("snapshot target must be a regular file: {}", path.display());
        }
        Ok(Self {
            bytes: std::fs::read(path)?,
            permissions: Some(metadata.permissions()),
        })
    }

    fn symlink(target: &std::path::Path) -> Self {
        Self {
            bytes: target.to_string_lossy().as_bytes().to_vec(),
            permissions: None,
        }
    }
}

fn snapshot_after_apply(action: &Action) -> anyhow::Result<Option<Snapshot>> {
    match action {
        Action::UpsertBlock { path, .. }
        | Action::MergeJsonc { path, .. }
        | Action::MergeToml { path, .. }
        | Action::WriteFile { path, .. } => {
            Snapshot::regular_file(std::path::Path::new(path)).map(Some)
        }
        Action::Symlink { destination, .. } => match std::fs::read_link(destination) {
            Ok(target) => Ok(Some(Snapshot::symlink(&target))),
            Err(error) => Err(error.into()),
        },
        Action::CreateDir { .. } | Action::RemovePath { .. } => Ok(None),
    }
}

struct AppliedAction {
    action: Action,
    outcome: ActionOutcome,
    before: Option<Snapshot>,
    after: Option<Snapshot>,
}

#[derive(Debug, Default)]
pub struct Executor;

impl Executor {
    pub fn execute(&self, plan: &Plan) -> anyhow::Result<Vec<ActionOutcome>> {
        self.execute_with_commit(plan, |_| Ok(()))
    }

    /// Execute a plan and run `commit` only after all actions succeed.
    /// If either an action or the commit fails, roll back successful actions.
    pub fn execute_with_commit<F>(
        &self,
        plan: &Plan,
        commit: F,
    ) -> anyhow::Result<Vec<ActionOutcome>>
    where
        F: FnOnce(&[ActionOutcome]) -> anyhow::Result<()>,
    {
        let mut applied = Vec::new();
        for action in &plan.actions {
            let before = if plan.dry_run {
                None
            } else {
                match snapshot_for_rollback(action) {
                    Ok(before) => before,
                    Err(error) => {
                        let rollback_errors = self.rollback(&applied);
                        if !rollback_errors.is_empty() {
                            anyhow::bail!(
                                "failed to snapshot action: {error:#}; rollback also failed: {}",
                                rollback_errors.join("; ")
                            );
                        }
                        return Err(error);
                    }
                }
            };
            match self.execute_one(action, plan.dry_run) {
                Ok(outcome) => {
                    let applied_bytes = if !plan.dry_run
                        && matches!(outcome, ActionOutcome::Created | ActionOutcome::Updated)
                    {
                        snapshot_after_apply(action).ok().flatten()
                    } else {
                        None
                    };
                    applied.push(AppliedAction {
                        action: action.clone(),
                        outcome,
                        before,
                        after: applied_bytes,
                    });
                    #[cfg(test)]
                    if !plan.dry_run {
                        match tests::AFTER_ACTION_HOOK.with(|hook| hook.replace(0)) {
                            1 => {
                                if let Action::WriteFile { path, .. } = action {
                                    std::fs::write(path, "user changed it").unwrap();
                                }
                            }
                            2 => {
                                if let Action::Symlink { destination, .. } = action {
                                    std::fs::remove_file(destination).unwrap();
                                    std::fs::write(destination, "user file").unwrap();
                                }
                            }
                            3 => {
                                if let Action::MergeJsonc { path, .. } = action {
                                    std::fs::write(path, "{\"user\": true}\n").unwrap();
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Err(error) => {
                    if !plan.dry_run {
                        let rollback_errors = self.rollback(&applied);
                        if !rollback_errors.is_empty() {
                            anyhow::bail!(
                                "action failed: {error:#}; rollback also failed: {}",
                                rollback_errors.join("; ")
                            );
                        }
                    }
                    return Err(error);
                }
            }
        }
        let outcomes = applied
            .iter()
            .map(|applied| applied.outcome.clone())
            .collect::<Vec<_>>();
        if let Err(error) = commit(&outcomes) {
            if !plan.dry_run {
                let rollback_errors = self.rollback(&applied);
                if !rollback_errors.is_empty() {
                    anyhow::bail!(
                        "commit failed: {error:#}; rollback also failed: {}",
                        rollback_errors.join("; ")
                    );
                }
            }
            return Err(error);
        }
        Ok(outcomes)
    }

    fn rollback(&self, applied: &[AppliedAction]) -> Vec<String> {
        let mut errors = Vec::new();
        for applied in applied.iter().rev() {
            let AppliedAction {
                action,
                outcome,
                before,
                after,
            } = applied;
            if !matches!(outcome, ActionOutcome::Created | ActionOutcome::Updated) {
                continue;
            }
            if let Err(error) = self.rollback_one(action, outcome, before.as_ref(), after.as_ref())
            {
                errors.push(format!("{}: {error:#}", action.key()));
            }
        }
        errors
    }

    fn rollback_one(
        &self,
        action: &Action,
        outcome: &ActionOutcome,
        before_snapshot: Option<&Snapshot>,
        after_snapshot: Option<&Snapshot>,
    ) -> anyhow::Result<()> {
        if let Some(permissions) = after_snapshot.and_then(|snapshot| snapshot.permissions.as_ref())
        {
            let metadata = std::fs::symlink_metadata(action.key())?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                anyhow::bail!("managed file was replaced before rollback");
            }
            if &metadata.permissions() != permissions {
                anyhow::bail!("managed file permissions changed before rollback");
            }
        }
        let before = before_snapshot.map(|snapshot| snapshot.bytes.as_slice());
        let applied_bytes = after_snapshot.map(|snapshot| snapshot.bytes.as_slice());
        match action {
            Action::WriteFile { path, .. }
                if *outcome == ActionOutcome::Created || *outcome == ActionOutcome::Updated =>
            {
                let path = std::path::Path::new(path);
                if path.is_file() && !path.is_symlink() {
                    let current = std::fs::read(path)?;
                    match outcome {
                        ActionOutcome::Created
                            if before.is_none()
                                && applied_bytes
                                    .is_some_and(|managed| managed == current.as_slice()) =>
                        {
                            std::fs::remove_file(path)?
                        }
                        ActionOutcome::Updated
                            if before.is_some_and(|original| original == current.as_slice()) => {}
                        _ => anyhow::bail!("managed file changed before rollback"),
                    }
                } else {
                    anyhow::bail!("managed file was replaced before rollback");
                }
            }
            Action::Symlink { destination, .. } if *outcome == ActionOutcome::Created => {
                let path = std::path::Path::new(destination);
                if path.is_symlink() {
                    let current_target = std::fs::read_link(path)?;
                    if applied_bytes.is_none_or(|managed| {
                        managed != current_target.to_string_lossy().as_bytes()
                    }) {
                        anyhow::bail!("created symlink changed before rollback");
                    }
                    std::fs::remove_file(path)?;
                } else {
                    anyhow::bail!("created symlink was replaced before rollback");
                }
            }
            Action::UpsertBlock { path, .. }
            | Action::MergeJsonc { path, .. }
            | Action::MergeToml { path, .. }
                if *outcome == ActionOutcome::Updated =>
            {
                let path = std::path::Path::new(path);
                if !path.is_file() || path.is_symlink() {
                    anyhow::bail!("managed config was replaced before rollback");
                }
                let original =
                    before.ok_or_else(|| anyhow::anyhow!("original snapshot missing"))?;
                let current = std::fs::read(path)?;
                if applied_bytes.is_some_and(|managed| managed == current.as_slice()) {
                    let permissions = before_snapshot
                        .and_then(|snapshot| snapshot.permissions.as_ref())
                        .ok_or_else(|| anyhow::anyhow!("original permissions missing"))?;
                    write_replacement_with_permissions(path, original, Some(permissions))?;
                } else {
                    anyhow::bail!("managed config changed before rollback");
                }
            }
            Action::UpsertBlock { path, .. }
            | Action::MergeJsonc { path, .. }
            | Action::MergeToml { path, .. }
                if *outcome == ActionOutcome::Created =>
            {
                let path = std::path::Path::new(path);
                if path.is_file() && !path.is_symlink() {
                    let current = std::fs::read(path)?;
                    if applied_bytes.is_some_and(|managed| managed == current.as_slice()) {
                        std::fs::remove_file(path)?;
                    } else {
                        anyhow::bail!("created config changed before rollback");
                    }
                } else {
                    anyhow::bail!("created config file was replaced before rollback");
                }
            }
            Action::CreateDir { path } if *outcome == ActionOutcome::Created => {
                let path = std::path::Path::new(path);
                if path.is_dir() && std::fs::read_dir(path)?.next().is_none() {
                    std::fs::remove_dir(path)?;
                }
            }
            Action::Symlink { destination, .. } if *outcome == ActionOutcome::Updated => {
                let path = std::path::Path::new(destination);
                let original =
                    before.ok_or_else(|| anyhow::anyhow!("original link snapshot missing"))?;
                let current = std::fs::read_link(path)?;
                if original == current.to_string_lossy().as_bytes() {
                    anyhow::bail!("managed symlink changed before rollback");
                }
                let source = path
                    .parent()
                    .map(|parent| parent.join(&current))
                    .unwrap_or(current);
                let replacement = path.with_extension("agent-home.rollback");
                std::fs::remove_file(&replacement).ok();
                #[cfg(unix)]
                std::os::unix::fs::symlink(source, &replacement)?;
                #[cfg(windows)]
                std::os::windows::fs::symlink_file(source, &replacement)?;
                std::fs::rename(replacement, path)?;
            }
            Action::WriteFile { path, .. } if *outcome == ActionOutcome::Created => {
                let path = std::path::Path::new(path);
                if path.exists() || path.is_symlink() {
                    let current = std::fs::read(path)?;
                    if before.is_none_or(|managed| managed != current.as_slice()) {
                        anyhow::bail!("created file changed before rollback");
                    }
                    std::fs::remove_file(path)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn execute_one(&self, action: &Action, dry_run: bool) -> anyhow::Result<ActionOutcome> {
        if dry_run {
            return Ok(ActionOutcome::DryRun);
        }
        match action {
            Action::CreateDir { path } => {
                let path = std::path::Path::new(path);
                if path.is_dir() {
                    return Ok(ActionOutcome::AlreadyCorrect);
                }
                if path.exists() {
                    return Ok(ActionOutcome::Collision);
                }
                std::fs::create_dir_all(path)?;
                Ok(ActionOutcome::Created)
            }
            Action::WriteFile { path, content } => {
                write_new_or_equal(std::path::Path::new(path), content.as_bytes())
            }
            Action::Symlink {
                source,
                destination,
            } => {
                let outcome = ah_config::symlink::link_entry(
                    std::path::Path::new(source),
                    std::path::Path::new(destination),
                )?;
                Ok(match outcome {
                    ah_config::symlink::LinkOutcome::Created => ActionOutcome::Created,
                    ah_config::symlink::LinkOutcome::AlreadyCorrect => {
                        ActionOutcome::AlreadyCorrect
                    }
                    ah_config::symlink::LinkOutcome::Collision => ActionOutcome::Collision,
                })
            }
            Action::UpsertBlock {
                path,
                style,
                name,
                body,
            } => {
                let path = std::path::Path::new(path);
                let original = match std::fs::read_to_string(path) {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                    Err(error) => return Err(error.into()),
                };
                let style = match style.as_str() {
                    "hash" => ah_config::block::CommentStyle::Hash,
                    "html" => ah_config::block::CommentStyle::Html,
                    _ => anyhow::bail!("unsupported managed-block comment style: {style}"),
                };
                let updated = ah_config::block::upsert(&original, style, name, body);
                write_merge_outcome(path, &original, &updated)
            }
            Action::MergeJsonc { path, patch } => {
                let path = std::path::Path::new(path);
                let original = match std::fs::read_to_string(path) {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => "{}".to_owned(),
                    Err(error) => return Err(error.into()),
                };
                let updated = ah_config::jsonc::merge_jsonc(&original, patch)?;
                write_merge_outcome(path, &original, &updated)
            }
            Action::MergeToml { path, patch } => {
                let path = std::path::Path::new(path);
                let original = match std::fs::read_to_string(path) {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                    Err(error) => return Err(error.into()),
                };
                let updated = ah_config::toml_edit::merge_toml(&original, patch)?;
                write_merge_outcome(path, &original, &updated)
            }
            Action::RemovePath { .. } => anyhow::bail!(
                "RemovePath is disabled until manifest-verified uninstall is implemented"
            ),
        }
    }
}

fn write_merge_outcome(
    path: &std::path::Path,
    original: &str,
    updated: &str,
) -> anyhow::Result<ActionOutcome> {
    if updated == original {
        return Ok(ActionOutcome::AlreadyCorrect);
    }
    let outcome = if path.exists() {
        ActionOutcome::Updated
    } else {
        ActionOutcome::Created
    };
    write_replacement(path, updated.as_bytes())?;
    Ok(outcome)
}

fn snapshot_for_rollback(action: &Action) -> anyhow::Result<Option<Snapshot>> {
    match action {
        Action::UpsertBlock { path, .. }
        | Action::MergeJsonc { path, .. }
        | Action::MergeToml { path, .. } => {
            let path = std::path::Path::new(path);
            match std::fs::symlink_metadata(path) {
                Ok(_) => Snapshot::regular_file(path).map(Some),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error.into()),
            }
        }
        Action::Symlink { destination, .. } => match std::fs::read_link(destination) {
            Ok(target) => Ok(Some(Snapshot::symlink(&target))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        },
        _ => Ok(None),
    }
}

fn write_new_or_equal(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<ActionOutcome> {
    if path.exists() {
        return Ok(if std::fs::read(path)? == bytes {
            ActionOutcome::AlreadyCorrect
        } else {
            ActionOutcome::Collision
        });
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(bytes)?;
            Ok(ActionOutcome::Created)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Ok(if std::fs::read(path)? == bytes {
                ActionOutcome::AlreadyCorrect
            } else {
                ActionOutcome::Collision
            })
        }
        Err(error) => Err(error.into()),
    }
}

pub fn atomic_replace_file(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    write_replacement(path, bytes).map(|_| ())
}

fn write_replacement(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<ActionOutcome> {
    write_replacement_with_permissions(path, bytes, None)
}

fn write_replacement_with_permissions(
    path: &std::path::Path,
    bytes: &[u8],
    restore_permissions: Option<&std::fs::Permissions>,
) -> anyhow::Result<ActionOutcome> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let original = match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            anyhow::bail!(
                "replacement target must be a regular file: {}",
                path.display()
            );
        }
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let (temporary, mut file) = loop {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{name}.agent-home-{}-{id}.tmp",
            std::process::id()
        ));
        #[cfg(test)]
        let candidate = tests::next_temp_candidate().unwrap_or(candidate);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&candidate) {
            Ok(file) => break (candidate, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };

    let result = (|| -> anyhow::Result<()> {
        file.write_all(bytes)?;
        #[cfg(test)]
        tests::fail_replacement_at(tests::ReplacementStage::Write)?;
        #[cfg(test)]
        tests::fail_replacement_at(tests::ReplacementStage::Sync)?;
        file.sync_all()?;
        if let Some(permissions) = restore_permissions
            .cloned()
            .or_else(|| original.as_ref().map(std::fs::Metadata::permissions))
        {
            file.set_permissions(permissions)?;
        }
        Ok(())
    })();
    drop(file);
    if let Err(error) = result {
        return match std::fs::remove_file(&temporary) {
            Ok(()) => Err(error),
            Err(cleanup) if cleanup.kind() == std::io::ErrorKind::NotFound => Err(error),
            Err(cleanup) => anyhow::bail!(
                "replacement failed: {error:#}; temporary cleanup failed for {}: {cleanup}",
                temporary.display()
            ),
        };
    }
    #[cfg(not(test))]
    let rename_result = std::fs::rename(&temporary, path);
    #[cfg(test)]
    let rename_result = tests::fail_replacement_at(tests::ReplacementStage::Rename)
        .and_then(|()| std::fs::rename(&temporary, path));
    if let Err(error) = rename_result {
        return match std::fs::remove_file(&temporary) {
            Ok(()) => Err(error.into()),
            Err(cleanup) if cleanup.kind() == std::io::ErrorKind::NotFound => Err(error.into()),
            Err(cleanup) => anyhow::bail!(
                "replacement failed: {error}; temporary cleanup failed for {}: {cleanup}",
                temporary.display()
            ),
        };
    }
    Ok(ActionOutcome::Updated)
}
