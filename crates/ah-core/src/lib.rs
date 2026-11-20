pub mod action;
pub mod config;
pub mod local;
pub mod manifest;

pub use action::{Action, ActionOutcome, Plan, Planner};
pub use config::{AgentHomeConfig, FoundryCatalog, Profile, RepoConfig};
pub use local::{LocalConfig, Source};
pub use manifest::{Manifest, ManifestEntry};

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paths {
    pub home_repo: PathBuf,
    pub vault: PathBuf,
    pub config_dir: PathBuf,
    pub manifest_file: PathBuf,
}

impl Paths {
    pub fn new(
        home_repo: impl Into<PathBuf>,
        vault: impl Into<PathBuf>,
        config_dir: impl Into<PathBuf>,
    ) -> Self {
        let config_dir = config_dir.into();
        Self {
            home_repo: home_repo.into(),
            vault: vault.into(),
            manifest_file: config_dir.join("manifest.json"),
            config_dir,
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if !self.home_repo.is_absolute()
            || !self.vault.is_absolute()
            || !self.config_dir.is_absolute()
        {
            anyhow::bail!("resolved Agent Home paths must be absolute");
        }
        if self.home_repo == self.vault {
            anyhow::bail!("shared home cannot be the personal vault root");
        }
        Ok(())
    }

    pub fn is_within_vault(&self, path: &Path) -> bool {
        path.starts_with(&self.vault)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_include_a_deterministic_manifest_location() {
        let paths = Paths::new("/team/home", "/user/vault", "/user/config/agent-home");
        assert_eq!(
            paths.manifest_file,
            PathBuf::from("/user/config/agent-home/manifest.json")
        );
        assert!(paths.validate().is_ok());
    }

    #[test]
    fn paths_reject_relative_paths_and_same_root() {
        assert!(Paths::new("relative/home", "/user/vault", "/config")
            .validate()
            .is_err());
        assert!(Paths::new("/same", "/same", "/config").validate().is_err());
    }
}
