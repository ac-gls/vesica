//! `vesica sync-standards --dry-run` (plan §5.4): a `Standards/*` → code-repo sync, with the
//! per-repo opt-in being `standards = true` in `agent-home.toml` (decision D-I).
//!
//! Only the plan is produced: nothing is written here, because opening PRs needs separate approval.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

struct Fixture {
    _temp: tempfile::TempDir,
    shared: PathBuf,
    repos: PathBuf,
}

impl Fixture {
    /// A shared home with one opted-in repo (`opted`) and one that isn't (`plain`), plus
    /// checkouts for both in the repos directory.
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let shared = temp.path().join("shared");
        copy_tree(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home"),
            &shared,
        );
        let toml = std::fs::read_to_string(shared.join("agent-home.toml")).unwrap();
        let head = toml.split("[[repos]]").next().unwrap();
        std::fs::write(
            shared.join("agent-home.toml"),
            format!(
                "{head}[[repos]]\nname = \"opted\"\nurl = \"https://example.invalid/opted.git\"\nprofiles = [\"developer\"]\nstandards = true\n\n[[repos]]\nname = \"plain\"\nurl = \"https://example.invalid/plain.git\"\nprofiles = [\"developer\"]\n"
            ),
        )
        .unwrap();
        let repos = temp.path().join("repos");
        for name in ["opted", "plain"] {
            std::fs::create_dir_all(repos.join(name)).unwrap();
        }
        Self {
            _temp: temp,
            shared,
            repos,
        }
    }

    fn sync(&self, extra: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("vesica")
            .unwrap()
            .args(["sync-standards", "--home-repo"])
            .arg(&self.shared)
            .arg("--repos-dir")
            .arg(&self.repos)
            .args(extra)
            .assert()
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

#[test]
fn dry_run_plans_the_writes_for_opted_in_repositories_only() {
    let fixture = Fixture::new();
    let out = fixture.sync(&["--dry-run"]).success().get_output().clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let destination = fixture
        .repos
        .join("opted/.github/instructions/coding-standards.instructions.md");
    assert!(
        stdout.contains(destination.to_str().unwrap()),
        "expected the destination in:\n{stdout}"
    );
    assert!(stdout.contains("Would create"), "{stdout}");
    assert!(
        stdout.contains("plain") && stdout.contains("not opted in"),
        "the non-opted repo must be reported as skipped:\n{stdout}"
    );
    assert!(
        !stdout.contains("plain/.github"),
        "nothing may be planned for a repo that isn't opted in:\n{stdout}"
    );
    // Nothing was written, in any repo.
    for name in ["opted", "plain"] {
        assert!(
            !fixture.repos.join(name).join(".github").exists(),
            "{name} was modified"
        );
    }
    assert!(stdout.contains("Dry run:"), "{stdout}");
}

#[test]
fn dry_run_reports_update_and_unchanged_states() {
    let fixture = Fixture::new();
    let instructions = fixture.repos.join("opted/.github/instructions");
    std::fs::create_dir_all(&instructions).unwrap();
    // Identical content: nothing to do.
    std::fs::copy(
        fixture.shared.join("Standards/coding-standards.md"),
        instructions.join("coding-standards.instructions.md"),
    )
    .unwrap();
    // Different content: would be updated.
    std::fs::write(instructions.join("testing.instructions.md"), "stale\n").unwrap();
    std::fs::write(
        fixture.shared.join("Standards/testing.md"),
        "# Testing standards\n",
    )
    .unwrap();

    let out = fixture.sync(&["--dry-run"]).success().get_output().clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("unchanged: .github/instructions/coding-standards.instructions.md"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Would update: .github/instructions/testing.instructions.md"),
        "{stdout}"
    );
    // Still nothing written.
    assert_eq!(
        std::fs::read_to_string(instructions.join("testing.instructions.md")).unwrap(),
        "stale\n"
    );
}

#[test]
fn dry_run_skips_repositories_that_are_not_cloned() {
    let fixture = Fixture::new();
    std::fs::remove_dir_all(fixture.repos.join("opted")).unwrap();
    fixture
        .sync(&["--dry-run"])
        .success()
        .stdout(predicate::str::contains("not cloned"));
    assert!(!fixture.repos.join("opted").exists());
}

#[test]
fn without_dry_run_it_refuses_because_opening_prs_needs_approval() {
    let fixture = Fixture::new();
    fixture
        .sync(&[])
        .failure()
        .stderr(predicate::str::contains("separate approval"));
    assert!(!fixture.repos.join("opted/.github").exists());
}

#[test]
fn dry_run_requires_a_repos_directory() {
    let fixture = Fixture::new();
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["sync-standards", "--home-repo"])
        .arg(&fixture.shared)
        .arg("--dry-run")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--repos-dir"));
}

#[test]
fn dry_run_reports_when_no_repository_is_opted_in() {
    let fixture = Fixture::new();
    let toml = std::fs::read_to_string(fixture.shared.join("agent-home.toml")).unwrap();
    std::fs::write(
        fixture.shared.join("agent-home.toml"),
        toml.replace("standards = true", "standards = false"),
    )
    .unwrap();
    fixture
        .sync(&["--dry-run"])
        .success()
        .stdout(predicate::str::contains("No repository is opted in"));
}
