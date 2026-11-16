use std::path::{Path, PathBuf};

/// Outcome of an idempotent link operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkOutcome {
    Created,
    AlreadyCorrect,
    Collision,
}

/// Create a symlink unless the destination exists. Existing user content is never replaced.
pub fn link_entry(source: &Path, destination: &Path) -> std::io::Result<LinkOutcome> {
    if let Ok(existing) = std::fs::read_link(destination) {
        let existing = normalize(&existing, destination.parent().unwrap_or(Path::new(".")));
        let desired = normalize(source, Path::new("."));
        return Ok(if existing == desired {
            LinkOutcome::AlreadyCorrect
        } else {
            LinkOutcome::Collision
        });
    }
    if destination.exists() {
        return Ok(LinkOutcome::Collision);
    }
    let parent = destination.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let source = if source.is_absolute() {
        source.to_path_buf()
    } else {
        std::env::current_dir()?.join(source)
    };
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, destination)?;
    #[cfg(windows)]
    {
        if source.is_dir() {
            std::os::windows::fs::symlink_dir(source, destination)?;
        } else {
            std::os::windows::fs::symlink_file(source, destination)?;
        }
    }
    Ok(LinkOutcome::Created)
}

fn normalize(path: &Path, relative_to: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        relative_to.join(path)
    };
    let mut result = PathBuf::new();
    for component in joined.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn creates_a_link_and_reports_it_as_unchanged_on_repeat() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("shared-skill");
        let destination = temp.path().join("skills/demo");
        std::fs::write(&source, "skill").unwrap();

        assert_eq!(
            link_entry(&source, &destination).unwrap(),
            LinkOutcome::Created
        );
        assert_eq!(
            link_entry(&source, &destination).unwrap(),
            LinkOutcome::AlreadyCorrect
        );
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "skill");
    }

    #[test]
    fn skips_any_existing_destination_without_overwriting_it() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("shared-skill");
        let destination = temp.path().join("skills/demo");
        std::fs::write(&source, "managed").unwrap();
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(&destination, "personal skill").unwrap();

        assert_eq!(
            link_entry(&source, &destination).unwrap(),
            LinkOutcome::Collision
        );
        assert_eq!(
            std::fs::read_to_string(destination).unwrap(),
            "personal skill"
        );
    }

    #[test]
    fn does_not_treat_a_different_symlink_as_managed() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("shared-skill");
        let other = temp.path().join("other-skill");
        let destination = temp.path().join("skills/demo");
        std::fs::write(&source, "managed").unwrap();
        std::fs::write(&other, "personal").unwrap();
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        symlink(&other, &destination).unwrap();

        assert_eq!(
            link_entry(&source, &destination).unwrap(),
            LinkOutcome::Collision
        );
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "personal");
    }
}
