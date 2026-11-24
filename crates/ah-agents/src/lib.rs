//! Profile-aware repository selection and per-agent skill discovery.

use ah_core::{Profile, RepoConfig};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
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
/// A neutral, content-free role specification consumed by supported renderers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub name: String,
    pub description: String,
    pub prompt: String,
    /// OpenCode-specific frontmatter overrides, serialized as YAML key-value lines.
    pub opencode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RoleFrontmatter {
    name: String,
    description: String,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_yaml::Value>,
}

/// Discover role documents with canonical neutral frontmatter (`name`, `description`) and
/// the Markdown body as prompt. Optional `opencode` settings are rendered only for OpenCode.
pub fn discover_roles(root: &Path) -> Result<Vec<Role>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut roles = Vec::new();
    for item in std::fs::read_dir(root)
        .with_context(|| format!("reading roles directory {}", root.display()))?
    {
        let item = item?;
        let path = item.path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading role {}", path.display()))?;
        let (role, _) =
            parse_role(&text).with_context(|| format!("invalid role {}", path.display()))?;
        roles.push(role);
    }
    roles.sort_by(|a, b| a.name.cmp(&b.name));
    let mut names = std::collections::BTreeSet::new();
    for role in &roles {
        if !names.insert(role.name.as_str()) {
            anyhow::bail!("duplicate role name `{}` in {}", role.name, root.display());
        }
    }
    Ok(roles)
}

fn parse_role(text: &str) -> Result<(Role, String)> {
    let normalized = text
        .strip_prefix('\u{feff}')
        .unwrap_or(text)
        .replace("\r\n", "\n");
    let rest = normalized
        .strip_prefix("---\n")
        .context("role must begin with YAML frontmatter")?;
    let end = rest
        .find("\n---")
        .context("unterminated YAML frontmatter")?;
    let frontmatter = &rest[..end];
    let prompt = rest[end + 4..].trim().to_owned();
    if prompt.is_empty() {
        anyhow::bail!("missing role prompt in Markdown body");
    }
    let parsed: RoleFrontmatter =
        serde_yaml::from_str(frontmatter).context("parsing YAML frontmatter")?;
    if parsed
        .extra
        .keys()
        .any(|key| key != "opencode" && key != "model" && key != "tools")
    {
        anyhow::bail!(
            "unsupported role frontmatter fields: {}",
            parsed.extra.keys().cloned().collect::<Vec<_>>().join(", ")
        );
    }
    let mut opencode = match parsed.extra.get("opencode") {
        None => BTreeMap::new(),
        Some(serde_yaml::Value::Mapping(values)) => values
            .iter()
            .map(|(key, value)| {
                let key = key
                    .as_str()
                    .context("OpenCode override keys must be strings")?
                    .to_owned();
                Ok((key, value.clone()))
            })
            .collect::<Result<BTreeMap<_, _>>>()?,
        Some(_) => anyhow::bail!("`opencode` overrides must be a mapping"),
    };
    for key in ["model", "tools"] {
        if !opencode.contains_key(key) {
            if let Some(value) = parsed.extra.get(key) {
                opencode.insert(key.to_owned(), value.clone());
            }
        }
    }
    let opencode = opencode;
    if parsed.name.is_empty()
        || parsed
            .name
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    {
        anyhow::bail!("role name must contain only ASCII letters, numbers, `-` or `_`");
    }
    if parsed.description.trim().is_empty() {
        anyhow::bail!("role description must not be empty");
    }
    let opencode = if opencode.is_empty() {
        None
    } else {
        Some(serde_yaml::to_string(&opencode)?.trim().to_owned())
    };
    let role = Role {
        name: parsed.name,
        description: parsed.description,
        prompt,
        opencode,
    };
    Ok((role, normalized))
}

/// Render the OpenCode user-agent Markdown format using the observed discovery convention.
pub fn render_opencode_role(role: &Role, prefix: &str) -> (String, String) {
    let name = format!("{prefix}{}", role.name);
    let mut content = format!("---\ndescription: {}\n", yaml_quote(&role.description));
    if let Some(overrides) = &role.opencode {
        content.push_str(overrides);
        content.push('\n');
    }
    content.push_str("mode: subagent\n---\n");
    content.push_str(&role.prompt);
    content.push('\n');
    (name, content)
}

fn yaml_quote(value: &str) -> String {
    serde_yaml::to_string(value)
        .unwrap_or_else(|_| format!("\"{}\"", value.replace('"', "\\\"")))
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_roles_and_renders_opencode_agent_markdown() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("sample.md"),
            "---\nname: sample\ndescription: A neutral role.\n---\nSummarize carefully.\n",
        )
        .unwrap();
        let roles = discover_roles(temp.path()).unwrap();
        assert_eq!(roles.len(), 1);
        assert_eq!(roles[0].prompt, "Summarize carefully.");
        assert_eq!(render_opencode_role(&roles[0], "ex-").0, "ex-sample");
    }

    #[test]
    fn legacy_fields_are_not_rendered_and_unknown_fields_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("role.md");
        std::fs::write(
            &path,
            "---\nname: sample\ndescription: A neutral role.\nmodel: legacy-model\ntools: [Read]\n---\nPrompt text.\n",
        )
        .unwrap();
        let roles = discover_roles(temp.path()).unwrap();
        let rendered = render_opencode_role(&roles[0], "ex-").1;
        assert!(rendered.contains("model: legacy-model"));
        assert!(rendered.contains("tools:"));

        std::fs::write(
            &path,
            "---\nname: sample\ndescription: A neutral role.\nopencode:\n  temperature: 0.2\n---\nPrompt text.\n",
        )
        .unwrap();
        let roles = discover_roles(temp.path()).unwrap();
        let rendered = render_opencode_role(&roles[0], "ex-").1;
        assert!(rendered.contains("temperature: 0.2"));

        std::fs::write(
            &path,
            "---\nname: sample\ndescription: A neutral role.\nopencode:\n  temperature: 0.2\n---\nPrompt text.\n",
        )
        .unwrap();
        let roles = discover_roles(temp.path()).unwrap();
        let rendered = render_opencode_role(&roles[0], "ex-").1;
        assert!(!rendered.contains("legacy-only"));
        assert!(!rendered.contains("Read"));

        std::fs::write(
            &path,
            "---\nname: sample\ndescription: A neutral role.\nopencode:\n  temperature: 0.2\n---\nPrompt text.\n",
        )
        .unwrap();
        let roles = discover_roles(temp.path()).unwrap();
        let rendered = render_opencode_role(&roles[0], "ex-").1;
        assert!(rendered.contains("temperature: 0.2"));

        std::fs::write(
            &path,
            "---\nname: sample\ndescription: A neutral role.\nopencode: invalid\n---\nPrompt text.\n",
        )
        .unwrap();
        let err = discover_roles(temp.path()).unwrap_err();
        assert!(format!("{err:#}").contains("overrides must be a mapping"));

        std::fs::write(
            &path,
            "---\nname: sample\ndescription: A neutral role.\nunsupported: value\n---\nPrompt text.\n",
        )
        .unwrap();
        let err = discover_roles(temp.path()).unwrap_err();
        assert!(format!("{err:#}").contains("unsupported role frontmatter fields"));

        std::fs::write(
            &path,
            "---\nname: sample\ndescription: A neutral role.\n---\n",
        )
        .unwrap();
        let err = discover_roles(temp.path()).unwrap_err();
        assert!(format!("{err:#}").contains("missing role prompt"));
    }

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
