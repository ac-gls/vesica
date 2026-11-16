//! Per-agent wiring (Claude Code, OpenCode, Copilot CLI, Codex, Hermes), role rendering, Foundry model config.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

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
