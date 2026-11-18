//! Profile-aware repository selection and per-agent skill discovery.

use ah_core::{Profile, RepoConfig};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ProfileSelection {
    pub profile: Profile,
    pub repositories: Vec<RepoConfig>,
}

impl ProfileSelection {
    /// Repositories for `profile` (plan §3): `core` has none; an empty `profiles` list means
    /// every profile that has repositories (developer and lead).
    pub fn from_config(config: &ah_core::AgentHomeConfig, profile: Profile) -> Self {
        let repositories = config
            .repos
            .iter()
            .filter(|_| profile != Profile::Core)
            .filter(|repo| repo.profiles.is_empty() || repo.profiles.contains(&profile))
            .cloned()
            .collect();
        Self {
            profile,
            repositories,
        }
    }

    pub fn from_name(config: &ah_core::AgentHomeConfig, name: &str) -> anyhow::Result<Self> {
        let profile = match name {
            "core" => Profile::Core,
            "developer" => Profile::Developer,
            "lead" => Profile::Lead,
            _ => anyhow::bail!("unsupported Vesica profile: {name}"),
        };
        Ok(Self::from_config(config, profile))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub source: PathBuf,
}

/// Discover skill directories that contain a `SKILL.md`, in stable name order.
pub fn discover_skills(root: &Path) -> Result<Vec<Skill>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut skills = Vec::new();
    for item in std::fs::read_dir(root)
        .with_context(|| format!("reading skills directory {}", root.display()))?
    {
        let item = item?;
        let path = item.path();
        if path.is_dir() && path.join("SKILL.md").is_file() {
            let name = item.file_name().into_string().map_err(|_| {
                anyhow::anyhow!(
                    "skill directory name is not valid UTF-8: {}",
                    path.display()
                )
            })?;
            skills.push(Skill { name, source: path });
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_selection_includes_only_matching_or_shared_repositories() {
        let config = ah_core::AgentHomeConfig::from_path(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/shared-home/agent-home.toml"),
        )
        .unwrap();
        let selected = ProfileSelection::from_config(&config, Profile::Core);
        assert!(selected.repositories.is_empty());
        // An empty `profiles` list means developer and lead; core never has repositories.
        let mut shared = config.clone();
        shared.repos[0].profiles.clear();
        assert!(ProfileSelection::from_config(&shared, Profile::Core)
            .repositories
            .is_empty());
        for profile in [Profile::Developer, Profile::Lead] {
            assert_eq!(
                ProfileSelection::from_config(&shared, profile)
                    .repositories
                    .len(),
                1
            );
        }
        let developer = ProfileSelection::from_config(&config, Profile::Developer);
        assert_eq!(
            developer
                .repositories
                .iter()
                .map(|repo| repo.name.as_str())
                .collect::<Vec<_>>(),
            vec!["sample-app"]
        );
        assert!(ProfileSelection::from_name(&config, "unrecognized").is_err());
    }

    #[test]
    fn discovers_skill_directories_deterministically_and_ignores_non_skills() {
        let root = tempfile::tempdir().unwrap();
        let skills = root.path().join("Agents/Skills");
        std::fs::create_dir_all(skills.join("zeta")).unwrap();
        std::fs::create_dir_all(skills.join("alpha")).unwrap();
        std::fs::create_dir_all(skills.join("notes-only")).unwrap();
        std::fs::write(skills.join("zeta/SKILL.md"), "z").unwrap();
        std::fs::write(skills.join("alpha/SKILL.md"), "a").unwrap();
        std::fs::write(skills.join("notes-only/README.md"), "ignore").unwrap();

        let found = discover_skills(&skills).unwrap();
        assert_eq!(
            found
                .iter()
                .map(|skill| skill.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "zeta"]
        );
        assert_eq!(found[0].source, skills.join("alpha"));
    }
}
