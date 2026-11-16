use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
        let bytes = serde_json::to_vec_pretty(self)?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, bytes)?;
        std::fs::rename(temporary, path)?;
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
