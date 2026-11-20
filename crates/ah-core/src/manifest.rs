use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;

use crate::Action;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub kind: String,
    pub target: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub backup: Option<String>,
    #[serde(default)]
    pub original_sha256: Option<String>,
    #[serde(default)]
    pub installed_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub app_version: String,
    pub shared_home: String,
    pub profile: String,
    /// Personal vault linked by `install --vault`, so `update` re-links to the same place.
    #[serde(default)]
    pub vault: Option<String>,
    /// The shared-home ref recorded at install time (plan F11): a tag, a branch or a short sha.
    /// Absent when the checkout is not a git repository.
    #[serde(default)]
    pub home_ref: Option<String>,
    #[serde(default)]
    pub entries: BTreeMap<String, ManifestEntry>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            schema: 1,
            app_version: env!("CARGO_PKG_VERSION").into(),
            shared_home: String::new(),
            profile: "developer".into(),
            vault: None,
            home_ref: None,
            entries: BTreeMap::new(),
        }
    }
}

impl Manifest {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read(path)?;
        let manifest: Self = serde_json::from_slice(&contents)?;
        if manifest.schema != 1 {
            anyhow::bail!("unsupported manifest schema {}", manifest.schema);
        }
        Ok(manifest)
    }

    pub fn save_atomic(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self)?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        #[cfg(windows)]
        options.share_mode(0);
        let mut file = options
            .open(&temporary)
            .with_context(|| format!("creating manifest temp file {}", temporary.display()))?;
        if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = std::fs::remove_file(&temporary);
            return Err(error)
                .with_context(|| format!("writing manifest temp file {}", temporary.display()));
        }
        drop(file);
        if let Err(error) = std::fs::rename(&temporary, path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error).with_context(|| format!("replacing manifest {}", path.display()));
        }
        Ok(())
    }

    pub fn record(&mut self, action: &Action, entry: ManifestEntry) {
        self.entries.insert(action.key().to_owned(), entry);
    }

    pub fn remove(&mut self, target: &str) -> Option<ManifestEntry> {
        self.entries.remove(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_sets_restrictive_permissions_on_unix() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("manifest.json");
            Manifest::default().save_atomic(&path).unwrap();
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn save_fails_without_following_or_replacing_manifest_temp_symlink() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("state/manifest.json");
            let outside = dir.path().join("outside.json");
            std::fs::write(&outside, "user-owned target").unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            symlink(&outside, path.with_extension("json.tmp")).unwrap();

            assert!(Manifest::default().save_atomic(&path).is_err());
            assert_eq!(std::fs::read(outside).unwrap(), b"user-owned target");
            assert!(std::fs::symlink_metadata(path.with_extension("json.tmp"))
                .unwrap()
                .file_type()
                .is_symlink());
            assert!(!path.exists());
        }
    }

    #[test]
    fn save_does_not_overwrite_a_concurrent_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state/manifest.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, "user-owned temp").unwrap();

        assert!(Manifest::default().save_atomic(&path).is_err());
        assert_eq!(std::fs::read(&temporary).unwrap(), b"user-owned temp");
        assert!(!path.exists());
    }

    #[test]
    fn save_preserves_the_existing_manifest_when_commit_cannot_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        let existing = b"existing valid-enough bytes";
        std::fs::write(&path, existing).unwrap();
        std::fs::write(path.with_extension("json.tmp"), "temp blocker").unwrap();

        assert!(Manifest::default().save_atomic(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), existing);
        assert_eq!(
            std::fs::read(path.with_extension("json.tmp")).unwrap(),
            b"temp blocker"
        );
    }

    #[test]
    fn saves_and_loads_a_manifest_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state/manifest.json");
        let manifest = Manifest {
            profile: "lead".into(),
            ..Manifest::default()
        };
        manifest.save_atomic(&path).unwrap();
        let loaded = Manifest::load(&path).unwrap();
        assert_eq!(loaded.profile, "lead");
        assert!(!path.with_extension("json.tmp").exists());
    }
}
