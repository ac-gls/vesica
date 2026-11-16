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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    static ACTION_TEST_HOOK: Mutex<()> = Mutex::new(());
    pub(super) static AFTER_ACTION_HOOK: AtomicUsize = AtomicUsize::new(0);

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
        let _guard = ACTION_TEST_HOOK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        AFTER_ACTION_HOOK.store(1, Ordering::SeqCst);

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
        let _guard = ACTION_TEST_HOOK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        AFTER_ACTION_HOOK.store(2, Ordering::SeqCst);
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
        let _guard = ACTION_TEST_HOOK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        AFTER_ACTION_HOOK.store(3, Ordering::SeqCst);
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

fn snapshot_after_apply(action: &Action) -> anyhow::Result<Option<Vec<u8>>> {
    match action {
        Action::UpsertBlock { path, .. }
        | Action::MergeJsonc { path, .. }
        | Action::MergeToml { path, .. }
        | Action::WriteFile { path, .. } => match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) => Err(error.into()),
        },
        Action::Symlink { destination, .. } => match std::fs::read_link(destination) {
            Ok(target) => Ok(Some(target.to_string_lossy().as_bytes().to_vec())),
            Err(error) => Err(error.into()),
        },
        Action::CreateDir { .. } | Action::RemovePath { .. } => Ok(None),
    }
}

type AppliedAction = (Action, ActionOutcome, Option<Vec<u8>>, Option<Vec<u8>>);

#[derive(Debug, Default)]
pub struct Executor;

impl Executor {
    pub fn execute(&self, plan: &Plan) -> anyhow::Result<Vec<ActionOutcome>> {
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
                    applied.push((action.clone(), outcome, before, applied_bytes));
                    #[cfg(test)]
                    if !plan.dry_run {
                        match tests::AFTER_ACTION_HOOK.swap(0, std::sync::atomic::Ordering::SeqCst)
                        {
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
        Ok(applied
            .into_iter()
            .map(|(_, outcome, _, _)| outcome)
            .collect())
    }

    fn rollback(&self, applied: &[AppliedAction]) -> Vec<String> {
        let mut errors = Vec::new();
        for (action, outcome, before, applied_bytes) in applied.iter().rev() {
            if !matches!(outcome, ActionOutcome::Created | ActionOutcome::Updated) {
                continue;
            }
            if let Err(error) =
                self.rollback_one(action, outcome, before.as_deref(), applied_bytes.as_deref())
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
        before: Option<&[u8]>,
        applied_bytes: Option<&[u8]>,
    ) -> anyhow::Result<()> {
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
                    write_replacement(path, original)?;
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

fn snapshot_for_rollback(action: &Action) -> anyhow::Result<Option<Vec<u8>>> {
    match action {
        Action::UpsertBlock { path, .. }
        | Action::MergeJsonc { path, .. }
        | Action::MergeToml { path, .. } => match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        },
        Action::Symlink { destination, .. } => match std::fs::read_link(destination) {
            Ok(target) => Ok(Some(target.to_string_lossy().as_bytes().to_vec())),
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
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
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

fn write_replacement(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<ActionOutcome> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("agent-home.tmp");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, path)?;
    Ok(ActionOutcome::Updated)
}
