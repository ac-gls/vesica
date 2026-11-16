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
        reject_credentials_toml(&value, "")?;
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
        if config
            .team
            .env_prefix
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && c != '_')
        {
            anyhow::bail!(
                "team.env_prefix must contain only ASCII letters, digits, and underscores"
            );
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
        reject_credentials_json(&value, "")?;
        Ok(serde_json::from_value(value)?)
    }

    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        Self::parse(&fs::read_to_string(path)?)
    }
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

fn reject_credentials_toml(value: &toml::Value, path: &str) -> anyhow::Result<()> {
    match value {
        toml::Value::Table(map) => {
            for (key, child) in map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if is_credential_key(key) {
                    anyhow::bail!(
                        "credential-like field {child_path} is forbidden in agent-home.toml"
                    );
                }
                reject_credentials_toml(child, &child_path)?;
            }
        }
        toml::Value::Array(items) => {
            for child in items {
                reject_credentials_toml(child, path)?;
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
    }
}
