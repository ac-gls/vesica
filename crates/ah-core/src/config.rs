use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    Core,
    Developer,
    Lead,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    pub name: String,
    pub org: String,
    pub env_prefix: String,
    pub role_prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilesConfig {
    pub default: Profile,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MinimumConfig {
    pub herdr: Option<String>,
    pub node: Option<String>,
    /// Oldest Vesica CLI this home works with (plan F11). An older CLI is blocked by `doctor`.
    pub vesica: Option<String>,
    /// Oldest home version this home's content is compatible with (plan F11).
    pub home: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesConfig {
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Opt-in for `vesica sync-standards`: this repository receives copies of `Standards/*`
    /// as `.github/instructions/*.instructions.md` (plan §5.4, decision D-I).
    #[serde(default)]
    pub standards: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHomeConfig {
    pub schema: u32,
    pub team: TeamConfig,
    pub profiles: ProfilesConfig,
    #[serde(default)]
    pub minimum: MinimumConfig,
    #[serde(default)]
    pub hermes: HermesConfig,
    #[serde(default)]
    pub repos: Vec<RepoConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FoundryCatalog {
    #[serde(default)]
    pub tenant: Option<String>,
    #[serde(default)]
    pub resources: BTreeMap<String, String>,
    #[serde(default)]
    pub deployments: BTreeMap<String, String>,
    #[serde(default)]
    pub defaults: BTreeMap<String, String>,
}

impl AgentHomeConfig {
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        let value: toml::Value = toml::from_str(input)?;
        reject_credentials_toml(&value, "", "agent-home.toml")?;
        let config: Self = value.try_into()?;
        if config.schema != 1 {
            anyhow::bail!(
                "unsupported agent-home.toml schema {}; expected 1",
                config.schema
            );
        }
        for (field, value) in [
            ("team.name", &config.team.name),
            ("team.org", &config.team.org),
            ("team.env_prefix", &config.team.env_prefix),
            ("team.role_prefix", &config.team.role_prefix),
        ] {
            if value.trim().is_empty() {
                anyhow::bail!("required configuration field {field} is empty");
            }
        }
        // The prefix names shell variables (`<PREFIX>_HOME`), so it must start like one.
        let prefix = &config.team.env_prefix;
        if prefix.starts_with(|c: char| c.is_ascii_digit())
            || prefix
                .chars()
                .any(|c| !c.is_ascii_alphanumeric() && c != '_')
        {
            anyhow::bail!(
                "team.env_prefix must contain only ASCII letters, digits, and underscores, and must not start with a digit"
            );
        }
        let role_prefix = &config.team.role_prefix;
        if role_prefix
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, '_' | '-'))
        {
            anyhow::bail!(
                "team.role_prefix must contain only ASCII letters, digits, underscores, and hyphens"
            );
        }
        for repo in &config.repos {
            validate_repo(repo)?;
        }
        Ok(config)
    }

    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        Self::parse(&fs::read_to_string(path)?)
    }
}

impl FoundryCatalog {
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        let value: serde_json::Value = serde_json::from_str(input)?;
        // serde would otherwise accept a JSON array positionally (e.g. `["tenant"]`).
        if !value.is_object() {
            anyhow::bail!("foundry.catalog.json must be a JSON object");
        }
        reject_credentials_json(&value, "")?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        Self::parse(&fs::read_to_string(path)?)
    }
}

/// A repository entry becomes a directory name and a `git clone` argument, so keep both inert:
/// a single safe path component, and a URL that cannot be read as an option or a
/// command-executing transport (`ext::`, `fd::`).
pub(crate) fn validate_repo(repo: &RepoConfig) -> anyhow::Result<()> {
    let name = repo.name.as_str();
    let safe_name = !name.is_empty()
        && !name.starts_with(['.', '-'])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !safe_name {
        anyhow::bail!(
            "repository name {name:?} must be a single directory name of ASCII letters, digits, '.', '_' or '-', not starting with '.' or '-'"
        );
    }
    let url = repo.url.trim();
    if url.is_empty() || url.starts_with('-') {
        anyhow::bail!("repository {name} has an empty or option-like url");
    }
    let lowered = url.to_ascii_lowercase();
    if lowered.starts_with("ext::") || lowered.starts_with("fd::") {
        anyhow::bail!("repository {name} uses a forbidden git transport");
    }
    if repo.profiles.contains(&Profile::Core) {
        anyhow::bail!(
            "repository {name} lists the core profile; the core profile has no repositories (use developer and/or lead, or leave profiles empty for both)"
        );
    }
    Ok(())
}

fn is_credential_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase().replace('-', "_");
    key.contains("api_key")
        || key.contains("apikey")
        || key.contains("client_secret")
        || key.contains("access_token")
        || key.contains("refresh_token")
        || key == "password"
        || key == "secret"
}

fn reject_credentials_json(value: &serde_json::Value, path: &str) -> anyhow::Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if is_credential_key(key) {
                    anyhow::bail!("credential-like field {child_path} is forbidden in the catalog");
                }
                reject_credentials_json(child, &child_path)?;
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                reject_credentials_json(child, path)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn reject_credentials_toml(
    value: &toml::Value,
    path: &str,
    file: &str,
) -> anyhow::Result<()> {
    match value {
        toml::Value::Table(map) => {
            for (key, child) in map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if is_credential_key(key) {
                    anyhow::bail!("credential-like field {child_path} is forbidden in {file}");
                }
                reject_credentials_toml(child, &child_path, file)?;
            }
        }
        toml::Value::Array(items) => {
            for child in items {
                reject_credentials_toml(child, path, file)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_team_config_and_profiles_from_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/shared-home/agent-home.toml");
        let config = AgentHomeConfig::from_path(&path).unwrap();
        assert_eq!(config.team.name, "Example Team");
        assert_eq!(config.profiles.default, Profile::Developer);
        assert_eq!(config.repos[0].name, "sample-app");
        assert!(config.repos[0].profiles.contains(&Profile::Lead));
    }

    #[test]
    fn rejects_unsafe_repository_names_and_urls() {
        let base = "schema = 1\n[team]\nname = \"T\"\norg = \"o\"\nenv_prefix = \"T\"\nrole_prefix = \"t-\"\n[profiles]\ndefault = \"developer\"\n";
        let with = |name: &str, url: &str| {
            AgentHomeConfig::parse(&format!(
                "{base}[[repos]]\nname = \"{name}\"\nurl = \"{url}\"\n"
            ))
        };
        assert!(with(
            "sample-app",
            "https://github.com/example-org/sample-app.git"
        )
        .is_ok());
        assert!(with("sample.app_2", "git@github.com:example-org/x.git").is_ok());
        for name in ["", ".", "..", "../evil", "a/b", ".hidden", "-flag"] {
            assert!(
                with(name, "https://example.invalid/r.git").is_err(),
                "{name:?}"
            );
        }
        for url in ["", "-uexploit", "ext::sh -c touch%20/tmp/x", "fd::3"] {
            assert!(with("ok", url).is_err(), "{url:?}");
        }
        let core = AgentHomeConfig::parse(&format!(
            "{base}[[repos]]\nname = \"r\"\nurl = \"https://example.invalid/r.git\"\nprofiles = [\"core\", \"lead\"]\n"
        ))
        .unwrap_err();
        assert!(core
            .to_string()
            .contains("the core profile has no repositories"));
    }

    #[test]
    fn validates_role_prefix_as_a_safe_filename_prefix() {
        let config_with_role_prefix = |role_prefix: &str| {
            AgentHomeConfig::parse(&format!(
                "schema = 1\n[team]\nname = \"T\"\norg = \"o\"\nenv_prefix = \"T\"\nrole_prefix = \"{role_prefix}\"\n[profiles]\ndefault = \"developer\"\n"
            ))
        };

        assert!(config_with_role_prefix("t-").is_ok());
        for role_prefix in ["../", "../../", "../outside/", "a/b", "a\\\\b", ".", ".."] {
            assert!(
                config_with_role_prefix(role_prefix).is_err(),
                "{role_prefix:?}"
            );
        }
    }

    #[test]
    fn rejects_invalid_profile_and_missing_required_team_fields() {
        let invalid = "schema=1\n[team]\nname='X'\norg='x'\nenv_prefix='X'\nrole_prefix='x-'\n[profiles]\ndefault='unknown'\n";
        assert!(AgentHomeConfig::parse(invalid).is_err());
        let missing = "schema=1\n[team]\nname='X'\n";
        assert!(AgentHomeConfig::parse(missing).is_err());
    }

    #[test]
    fn parses_catalog_and_rejects_credential_like_fields() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/shared-home/foundry.catalog.json");
        let catalog = FoundryCatalog::from_path(&path).unwrap();
        assert_eq!(catalog.defaults["codex"], "gpt-codex");
        assert!(FoundryCatalog::parse(r#"{"api_key":"must-not-be-here"}"#).is_err());
        // serde reads structs positionally from arrays; the catalog root must be an object.
        assert!(FoundryCatalog::parse("[]").is_err());
        assert!(FoundryCatalog::parse(r#"["00000000-0000-0000-0000-000000000000"]"#).is_err());
    }
}
