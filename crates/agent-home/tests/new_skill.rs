//! `vesica new-skill` (plan §5.4): scaffold a skill on a branch in the shared home.
//!
//! A person runs this, edits the scaffold and opens the PR. It never writes to the current branch,
//! never pushes, and refuses a dirty checkout.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

/// A committed shared-home Git checkout (a copy of the fixture) with a git identity available.
struct SharedFixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
}

impl SharedFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("shared");
        copy_tree(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home"),
            &root,
        );
        let fixture = Self { _temp: temp, root };
        fixture.git(&["init", "-q", "-b", "main"]);
        fixture.git(&["add", "-A"]);
        fixture.git(&["commit", "-q", "-m", "initial"]);
        fixture
    }

    fn git(&self, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Vesica Test")
            .env("GIT_AUTHOR_EMAIL", "vesica@example.invalid")
            .env("GIT_COMMITTER_NAME", "Vesica Test")
            .env("GIT_COMMITTER_EMAIL", "vesica@example.invalid")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn status(&self) -> String {
        self.git(&["status", "--porcelain"])
    }
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn new_skill(shared: &Path, name: &str, extra: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("vesica")
        .unwrap()
        .arg("new-skill")
        .arg(name)
        .arg("--home-repo")
        .arg(shared)
        .args(extra)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Vesica Test")
        .env("GIT_AUTHOR_EMAIL", "vesica@example.invalid")
        .env("GIT_COMMITTER_NAME", "Vesica Test")
        .env("GIT_COMMITTER_EMAIL", "vesica@example.invalid")
        .assert()
}

/// `vesica <args> <path>`, e.g. `lint --home-repo <shared>`.
fn vesica_with_path(args: &[&str], path: &Path) -> assert_cmd::assert::Assert {
    Command::cargo_bin("vesica")
        .unwrap()
        .args(args)
        .arg(path)
        .assert()
}

#[test]
fn new_skill_scaffolds_on_a_branch_and_leaves_main_untouched() {
    let fixture = SharedFixture::new();
    let before = fixture.git(&["rev-parse", "HEAD"]);
    new_skill(
        &fixture.root,
        "my-skill",
        &["--description", "Use when testing."],
    )
    .success()
    .stdout(predicate::str::contains("skill/my-skill"))
    .stdout(predicate::str::contains("Sessions").or(predicate::str::contains("Next")));

    // On the new branch, committed, with lint-clean content.
    assert_eq!(
        fixture.git(&["branch", "--show-current"]).trim(),
        "skill/my-skill"
    );
    let file = fixture.root.join("Agents/Skills/my-skill/SKILL.md");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.starts_with("---\n"), "{text}");
    assert!(text.contains("name: my-skill"), "{text}");
    assert!(
        text.contains("description: \"Use when testing.\""),
        "{text}"
    );
    assert_eq!(fixture.status(), "");
    assert!(fixture
        .git(&["log", "-1", "--format=%s"])
        .contains("my-skill"));
    // Never pushed: the branch has no upstream and the remote (none) is untouched.
    assert!(fixture.git(&["branch", "-vv"]).contains("skill/my-skill"));

    // main is unchanged, and the skill is absent there.
    fixture.git(&["checkout", "-q", "main"]);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), before.trim());
    assert!(!fixture.root.join("Agents/Skills/my-skill").exists());

    // The scaffold passes the shared-home lint.
    vesica_with_path(&["lint", "--home-repo"], &fixture.root)
        .success()
        .stdout(predicate::str::contains("No lint findings"));
}

#[test]
fn new_skill_uses_a_placeholder_description_that_lint_accepts() {
    let fixture = SharedFixture::new();
    new_skill(&fixture.root, "no-desc", &[])
        .success()
        .stdout(predicate::str::contains("TODO"));
    let text =
        std::fs::read_to_string(fixture.root.join("Agents/Skills/no-desc/SKILL.md")).unwrap();
    assert!(text.contains("description:"), "{text}");
    vesica_with_path(&["lint", "--home-repo"], &fixture.root).success();
}

#[test]
fn new_skill_refuses_a_dirty_checkout() {
    let fixture = SharedFixture::new();
    std::fs::write(
        fixture.root.join("Standards/coding-standards.md"),
        "edited\n",
    )
    .unwrap();
    new_skill(&fixture.root, "my-skill", &[])
        .failure()
        .stderr(predicate::str::contains("uncommitted changes"));
    assert_eq!(fixture.git(&["branch", "--show-current"]).trim(), "main");
    assert!(!fixture.root.join("Agents/Skills/my-skill").exists());
}

#[test]
fn new_skill_refuses_an_existing_skill_or_branch() {
    let fixture = SharedFixture::new();
    new_skill(&fixture.root, "demo-skill", &[])
        .failure()
        .stderr(predicate::str::contains("already exists"));
    fixture.git(&["checkout", "-q", "main"]);
    // A stale branch from a previous attempt.
    fixture.git(&["branch", "skill/again"]);
    new_skill(&fixture.root, "again", &[])
        .failure()
        .stderr(predicate::str::contains(
            "branch skill/again already exists",
        ));
    assert_eq!(fixture.git(&["branch", "--show-current"]).trim(), "main");
}

#[test]
fn new_skill_rejects_unsafe_names() {
    let fixture = SharedFixture::new();
    for name in [
        "../escape",
        "Upper",
        "with space",
        "trailing-",
        "-leading",
        "a/b",
        "",
        &"x".repeat(65),
    ] {
        new_skill(&fixture.root, name, &[]).failure();
    }
    // No partial scaffolds: the fixture's two skills are still all there is, and the
    // checkout is clean.
    let skills: Vec<_> = std::fs::read_dir(fixture.root.join("Agents/Skills"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(skills.len(), 2, "{skills:?}");
    assert_eq!(fixture.status(), "");
}

#[test]
fn new_skill_dry_run_changes_nothing() {
    let fixture = SharedFixture::new();
    let before = fixture.status();
    new_skill(&fixture.root, "dry-run-skill", &["--dry-run"])
        .success()
        .stdout(predicate::str::contains("Would create"));
    assert_eq!(fixture.git(&["branch", "--show-current"]).trim(), "main");
    assert!(!fixture.root.join("Agents/Skills/dry-run-skill").exists());
    assert_eq!(fixture.status(), before);
}

#[test]
fn new_skill_works_on_a_standalone_checkout_without_committing() {
    // A checkout that is not a Git repository can be scaffolded but not committed.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("plain");
    copy_tree(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home"),
        &root,
    );
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["new-skill", "plain-skill", "--home-repo"])
        .arg(&root)
        .assert()
        .success()
        .stdout(predicate::str::contains("not a Git repository"));
    assert!(root.join("Agents/Skills/plain-skill/SKILL.md").is_file());
}

#[test]
fn a_failed_commit_leaves_no_branch_and_no_scaffold_file() {
    // The commit-failure path is otherwise unreachable, so force it with a failing hook.
    let fixture = SharedFixture::new();
    let hooks = fixture.root.join(".git/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join("pre-commit"), "#!/bin/sh\nexit 1\n").unwrap();
    let mut perms = std::fs::metadata(hooks.join("pre-commit"))
        .unwrap()
        .permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(hooks.join("pre-commit"), perms).unwrap();

    new_skill(&fixture.root, "hooked", &[]).failure();
    assert_eq!(fixture.git(&["branch", "--show-current"]).trim(), "main");
    assert!(
        !fixture
            .git(&["branch", "--list", "skill/hooked"])
            .contains("skill/hooked"),
        "the branch must be deleted"
    );
    assert!(
        !fixture.root.join("Agents/Skills/hooked").exists(),
        "the untracked scaffold must not be left behind"
    );
    assert_eq!(fixture.status(), "");
}
