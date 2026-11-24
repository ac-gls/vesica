//! Exact, symlink-aware snapshots used to undo failed uninstall progress commits.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Node {
    Absent,
    File {
        bytes: Vec<u8>,
        #[serde(with = "permission_mode")]
        permissions: std::fs::Permissions,
    },
    Directory {
        #[serde(with = "permission_mode")]
        permissions: std::fs::Permissions,
    },
    Link(PathBuf),
    Other,
}
impl Node {
    pub fn read(path: &Path) -> Result<Self> {
        match std::fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::Absent),
            Err(e) => Err(e.into()),
            Ok(m) if m.file_type().is_symlink() => Ok(Self::Link(std::fs::read_link(path)?)),
            Ok(m) if m.is_file() => Ok(Self::File {
                bytes: std::fs::read(path)?,
                permissions: m.permissions(),
            }),
            Ok(m) if m.is_dir() => Ok(Self::Directory {
                permissions: m.permissions(),
            }),
            Ok(_) => Ok(Self::Other),
        }
    }
    pub fn matches(&self, path: &Path) -> Result<bool> {
        let current = Self::read(path)?;
        Ok(match (self, current) {
            (Self::Absent, Self::Absent) => true,
            (Self::Link(a), Self::Link(b)) => *a == b,
            (Self::Directory { .. }, Self::Directory { .. }) => true,
            (
                Self::File {
                    bytes: a,
                    permissions: ap,
                },
                Self::File {
                    bytes: b,
                    permissions: bp,
                },
            ) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    *a == b && ap.mode() == bp.mode()
                }
                #[cfg(not(unix))]
                {
                    *a == b && ap.readonly() == bp.readonly()
                }
            }
            _ => false,
        })
    }
    pub fn restore(&self, path: &Path, expected: &Self) -> Result<()> {
        if !expected.matches(path)? {
            anyhow::bail!(
                "{} changed during operation; preserving user changes (replacement retained)",
                path.display()
            );
        }
        super::reject_symlink_components(path.parent().context("recovery path has no parent")?)?;
        match self {
            Self::Absent => {
                if matches!(expected, Self::Directory { .. }) {
                    std::fs::remove_dir(path)?;
                } else if !matches!(expected, Self::Absent) {
                    std::fs::remove_file(path)?;
                }
            }
            Self::File { bytes, permissions } => {
                ah_core::action::atomic_replace_file(path, bytes)?;
                std::fs::set_permissions(path, permissions.clone())?;
            }
            Self::Link(target) => {
                if !matches!(expected, Self::Absent) {
                    std::fs::remove_file(path)?;
                }
                #[cfg(unix)]
                std::os::unix::fs::symlink(target, path)?;
                #[cfg(windows)]
                std::os::windows::fs::symlink_file(target, path)?;
            }
            Self::Directory { permissions } => {
                std::fs::create_dir(path)?;
                std::fs::set_permissions(path, permissions.clone())?;
            }
            Self::Other => anyhow::bail!("cannot restore unsupported node {}", path.display()),
        }
        Ok(())
    }
}

mod permission_mode {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::os::unix::fs::PermissionsExt;
    pub fn serialize<S: Serializer>(p: &std::fs::Permissions, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u32(p.mode())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<std::fs::Permissions, D::Error> {
        Ok(std::fs::Permissions::from_mode(u32::deserialize(d)?))
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Change {
    path: PathBuf,
    before: Node,
    after: Option<Node>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Journal {
    path: PathBuf,
    changes: Vec<Change>,
}
impl Journal {
    pub fn prepare(path: &Path, paths: Vec<PathBuf>) -> Result<Self> {
        let mut all = std::collections::BTreeSet::new();
        for path in paths {
            all.insert(path.clone());
            let mut parent = path.parent();
            while let Some(p) = parent {
                if !matches!(Node::read(p)?, Node::Absent) {
                    break;
                }
                all.insert(p.to_owned());
                parent = p.parent();
            }
        }
        let mut changes = Vec::new();
        for path in all {
            changes.push(Change {
                before: Node::read(&path)?,
                path,
                after: None,
            });
        }
        let journal = Self {
            path: path.to_owned(),
            changes,
        };
        journal.save()?;
        Ok(journal)
    }
    pub fn save(&self) -> Result<()> {
        ah_core::action::atomic_replace_file(&self.path, &serde_json::to_vec(self)?)
    }
    pub fn seal(&mut self) -> Result<()> {
        for change in &mut self.changes {
            change.after = Some(Node::read(&change.path)?);
        }
        self.save()
    }
    pub fn record_postimage(&mut self, path: &Path, after: Node) -> Result<()> {
        let change = self
            .changes
            .iter_mut()
            .find(|c| c.path == path)
            .context("mutation path missing from recovery journal")?;
        change.after = Some(after);
        self.save()
    }
    pub fn load(path: &Path) -> Result<Option<Self>> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    pub fn rollback_file(path: &Path) -> Result<()> {
        let Some(mut journal) = Self::load(path)? else {
            return Ok(());
        };
        journal.rollback()
    }
    pub fn rollback(&mut self) -> Result<()> {
        // Children before parents; persist the manifest preimage last.
        self.changes.sort_by_key(|c| {
            (
                c.path.file_name().is_some_and(|n| n == "manifest.json"),
                std::cmp::Reverse(c.path.components().count()),
            )
        });
        let mut errors = Vec::new();
        for change in &self.changes {
            if change.before.matches(&change.path)? {
                continue;
            }
            let Some(after) = change.after.as_ref() else {
                errors.push(format!(
                    "{}: postimage was not recorded; preserve journal for explicit recovery",
                    change.path.display()
                ));
                continue;
            };
            if change
                .path
                .file_name()
                .is_some_and(|n| n == "manifest.json")
                && !errors.is_empty()
            {
                continue;
            }
            if let Err(error) = change.before.restore(&change.path, after) {
                errors.push(format!("{}: {error:#}", change.path.display()));
            }
        }
        if !errors.is_empty() {
            anyhow::bail!("{}", errors.join("; "));
        }
        Ok(())
    }
}
