//! Local (machine) configuration — plan F8.
//!
//! The team file (`agent-home.toml`) lives *inside* the shared home, so it cannot record where the
//! shared home is: that is circular. This file lives on the machine instead, is never committed and
//! never shared, and is the only layer that can supply the home path before the home is known.
//! It also carries the machine's profile choice, vault and repos directory, and any repositories
//! that are local to this machine (a fork, a scratch checkout) rather than team-wide.
//!
//! Precedence, highest first: command-line flag / environment variable, this file, the team
//! `agent-home.toml`, then the built-in default. See [`resolve`].

use crate::config::{validate_repo, Profile, RepoConfig};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Schema of the local file. Bumped only for a breaking change, independently of the team file's
/// schema and of the home's release version.
pub const SCHEMA: u32 = 1;

/// File name inside the agent-home configuration directory (`<root>/.config/agent-home/`).
pub const LOCAL_CONFIG_NAME: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConfig {
    pub schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_repo: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<Profile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repos_dir: Option<PathBuf>,
    /// Machine-local repositories. Merged with the team list by name; see [`merge_repos`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<RepoConfig>,
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            home_repo: None,
            profile: None,
            vault: None,
            repos_dir: None,
            repos: Vec::new(),
        }
    }
}

impl LocalConfig {
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        let value: toml::Value = toml::from_str(input)?;
        // A machine file is still not a credential store: the same rule as the team file.
        crate::config::reject_credentials_toml(&value, "", LOCAL_CONFIG_NAME)?;
        let config: Self = value.try_into()?;
        if config.schema != SCHEMA {
            anyhow::bail!(
                "unsupported local configuration schema {}; expected {SCHEMA}",
                config.schema
            );
        }
        for repo in &config.repos {
            validate_repo(repo)?;
        }
        Ok(config)
    }

    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        Self::parse(&fs::read_to_string(path)?)
            .with_context(|| format!("cannot load local configuration {}", path.display()))
    }

    /// Path of the local file inside a configuration directory.
    pub fn path_in(config_dir: &Path) -> PathBuf {
        config_dir.join(LOCAL_CONFIG_NAME)
    }

    /// Reads the local file, treating "not there yet" as the empty default. A machine that has
    /// never run `install` must still resolve flags, environment and the team file normally.
    pub fn load(config_dir: &Path) -> anyhow::Result<Self> {
        let path = Self::path_in(config_dir);
        if !path.exists() {
            return Ok(Self::default());
        }
        Self::from_path(&path)
    }

    pub fn to_toml(&self) -> anyhow::Result<String> {
        toml::to_string_pretty(self).map_err(Into::into)
    }

    /// Persists the local file (and its directory). Returns the path written.
    pub fn write(&self, config_dir: &Path) -> anyhow::Result<PathBuf> {
        fs::create_dir_all(config_dir)
            .with_context(|| format!("cannot create {}", config_dir.display()))?;
        let path = Self::path_in(config_dir);
        fs::write(&path, self.to_toml()?)
            .with_context(|| format!("cannot write {}", path.display()))?;
        Ok(path)
    }
}

/// Which layer supplied a resolved value. Reported by `doctor` so a surprising value can be traced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    FlagEnv,
    LocalConfig,
    Team,
    Default,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::FlagEnv => "flag/env",
            Source::LocalConfig => "config.toml",
            Source::Team => "team",
            Source::Default => "default",
        }
    }
}

/// First present value wins, in precedence order. Returns the value and where it came from.
pub fn resolve<T>(
    flag_env: Option<T>,
    local: Option<T>,
    team: Option<T>,
    built_in: Option<T>,
) -> (Option<T>, Source) {
    match (flag_env, local, team, built_in) {
        (Some(value), ..) => (Some(value), Source::FlagEnv),
        (None, Some(value), ..) => (Some(value), Source::LocalConfig),
        (None, None, Some(value), _) => (Some(value), Source::Team),
        (None, None, None, Some(value)) => (Some(value), Source::Default),
        (None, None, None, None) => (None, Source::Default),
    }
}

/// Team entries in the team's order, with any local entry of the same name replacing it, followed
/// by the local-only additions. One repository is one entry.
pub fn merge_repos(team: &[RepoConfig], local: &[RepoConfig]) -> Vec<RepoConfig> {
    let mut merged: Vec<RepoConfig> = team
        .iter()
        .map(|entry| {
            local
                .iter()
                .find(|local_entry| local_entry.name == entry.name)
                .cloned()
                .unwrap_or_else(|| entry.clone())
        })
        .collect();
    for local_entry in local {
        if !team.iter().any(|entry| entry.name == local_entry.name) {
            merged.push(local_entry.clone());
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str, url: &str) -> RepoConfig {
        RepoConfig {
            name: name.to_owned(),
            url: url.to_owned(),
            profiles: Vec::new(),
            standards: false,
        }
    }

    #[test]
    fn missing_file_is_the_empty_default_but_a_broken_file_is_an_error() {
        let dir = std::env::temp_dir().join(format!("ah-local-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let empty = LocalConfig::load(&dir).unwrap();
        assert_eq!(empty.schema, SCHEMA);
        assert!(empty.home_repo.is_none());
        fs::create_dir_all(&dir).unwrap();
        fs::write(LocalConfig::path_in(&dir), "schema = 9\n").unwrap();
        assert!(LocalConfig::load(&dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn round_trips_and_rejects_credentials_and_unsafe_repos() {
        let config = LocalConfig {
            home_repo: Some(PathBuf::from("/home/me/source/repos/home")),
            profile: Some(Profile::Lead),
            vault: Some(PathBuf::from("/home/me/vault")),
            repos_dir: Some(PathBuf::from("/home/me/source/repos")),
            repos: vec![repo("sample-app", "https://example.invalid/sample-app.git")],
            ..LocalConfig::default()
        };
        let text = config.to_toml().unwrap();
        let parsed = LocalConfig::parse(&text).unwrap();
        assert_eq!(parsed.home_repo, config.home_repo);
        assert_eq!(parsed.profile, Some(Profile::Lead));
        assert_eq!(parsed.repos.len(), 1);

        assert!(LocalConfig::parse("schema = 1\nclient_secret = \"x\"\n").is_err());
        assert!(LocalConfig::parse(
            "schema = 1\n[[repos]]\nname = \"../evil\"\nurl = \"https://example.invalid/r.git\"\n"
        )
        .is_err());
        assert!(LocalConfig::parse(
            "schema = 1\n[[repos]]\nname = \"ok\"\nurl = \"ext::sh -c touch%20/tmp/x\"\n"
        )
        .is_err());
        // A local file may not claim the core profile for a repository either.
        assert!(LocalConfig::parse(
            "schema = 1\n[[repos]]\nname = \"ok\"\nurl = \"https://example.invalid/r.git\"\nprofiles = [\"core\"]\n"
        )
        .is_err());
    }

    #[test]
    fn precedence_is_flag_env_then_local_then_team_then_default() {
        let (value, source) = resolve(Some("flag"), Some("local"), Some("team"), Some("default"));
        assert_eq!((value, source), (Some("flag"), Source::FlagEnv));
        let (value, source) = resolve(None, Some("local"), Some("team"), Some("default"));
        assert_eq!((value, source), (Some("local"), Source::LocalConfig));
        let (value, source) = resolve(None, None, Some("team"), Some("default"));
        assert_eq!((value, source), (Some("team"), Source::Team));
        let (value, source) = resolve(None, None, None, Some("default"));
        assert_eq!((value, source), (Some("default"), Source::Default));
        let (value, source) = resolve::<&str>(None, None, None, None);
        assert_eq!((value, source), (None, Source::Default));
    }

    #[test]
    fn repos_merge_by_name_and_keep_team_order() {
        let team = vec![
            repo("alpha", "https://example.invalid/alpha.git"),
            repo("beta", "https://example.invalid/beta.git"),
        ];
        let local = vec![
            repo("beta", "https://example.invalid/beta-fork.git"),
            repo("gamma", "https://example.invalid/gamma.git"),
        ];
        let merged = merge_repos(&team, &local);
        let names: Vec<&str> = merged.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["alpha", "beta", "gamma"]);
        assert_eq!(merged[1].url, "https://example.invalid/beta-fork.git");
        assert_eq!(merged[0].url, "https://example.invalid/alpha.git");
    }
}
