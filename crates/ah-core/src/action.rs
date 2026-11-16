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

#[derive(Debug, Default)]
pub struct Executor;

impl Executor {
    pub fn execute(&self, plan: &Plan) -> anyhow::Result<Vec<ActionOutcome>> {
        plan.actions
            .iter()
            .map(|action| self.execute_one(action, plan.dry_run))
            .collect()
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
                if updated == original {
                    return Ok(ActionOutcome::AlreadyCorrect);
                }
                write_replacement(path, updated.as_bytes())
            }
            Action::MergeJsonc { path, patch } => {
                let path = std::path::Path::new(path);
                let original = match std::fs::read_to_string(path) {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => "{}".to_owned(),
                    Err(error) => return Err(error.into()),
                };
                let updated = ah_config::jsonc::merge_jsonc(&original, patch)?;
                if updated == original {
                    return Ok(ActionOutcome::AlreadyCorrect);
                }
                write_replacement(path, updated.as_bytes())
            }
            Action::MergeToml { path, patch } => {
                let path = std::path::Path::new(path);
                let original = match std::fs::read_to_string(path) {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                    Err(error) => return Err(error.into()),
                };
                let updated = ah_config::toml_edit::merge_toml(&original, patch)?;
                if updated == original {
                    return Ok(ActionOutcome::AlreadyCorrect);
                }
                write_replacement(path, updated.as_bytes())
            }
            Action::RemovePath { .. } => anyhow::bail!(
                "RemovePath is disabled until manifest-verified uninstall is implemented"
            ),
        }
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
