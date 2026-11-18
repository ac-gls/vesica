//! `vesica vault init|fix|check|new-note|snapshot` (plan §5.1 personal vault skeleton, §5.6 vault
//! contract, D30: personal snapshots exclude `Team/**`).

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

fn fixture_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home")
}

fn vesica(args: &[&str], vault: &Path) -> assert_cmd::assert::Assert {
    Command::cargo_bin("vesica")
        .unwrap()
        .arg("vault")
        .args(args)
        .arg("--path")
        .arg(vault)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Vesica Test")
        .env("GIT_AUTHOR_EMAIL", "vesica@example.invalid")
        .env("GIT_COMMITTER_NAME", "Vesica Test")
        .env("GIT_COMMITTER_EMAIL", "vesica@example.invalid")
        .assert()
}

fn git(vault: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(vault)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

const SKELETON: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "README.md",
    ".gitignore",
    ".gitattributes",
    "Inbox",
    "Projects",
    "Decisions",
    "Calendar",
    "Archive",
    "Sessions/agent-runs",
];

/// A freshly initialised vault with `Team` linked to the fixture shared home.
fn new_vault() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("my vault");
    vesica(&["init"], &vault).success();
    std::os::unix::fs::symlink(fixture_home().canonicalize().unwrap(), vault.join("Team")).unwrap();
    (temp, vault)
}

#[test]
fn init_creates_the_skeleton_and_a_git_repository_and_check_passes() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    vesica(&["init", "--dry-run"], &vault)
        .success()
        .stdout(predicate::str::contains("Would create"));
    assert!(!vault.exists(), "dry run must not create anything");

    vesica(&["init"], &vault).success();
    for entry in SKELETON {
        assert!(vault.join(entry).exists(), "missing {entry}");
    }
    assert!(vault.join(".git").is_dir());
    let claude = std::fs::read_to_string(vault.join("CLAUDE.md")).unwrap();
    assert!(claude.contains("@AGENTS.md") && claude.contains("@Team/AGENTS.md"));
    vesica(&["check"], &vault)
        .success()
        .stdout(predicate::str::contains("vault check: ok"));
}

#[test]
fn init_refuses_a_non_empty_directory_and_changes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::write(vault.join("mine.md"), "my note\n").unwrap();
    vesica(&["init"], &vault)
        .failure()
        .stderr(predicate::str::contains("vault fix"));
    let entries: Vec<_> = std::fs::read_dir(&vault).unwrap().collect();
    assert_eq!(entries.len(), 1);
}

#[test]
fn fix_adopts_an_existing_folder_without_overwriting_anything() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let mine = b"# my own agent rules\n";
    std::fs::write(vault.join("AGENTS.md"), mine).unwrap();
    std::fs::write(vault.join(".gitignore"), "*.tmp").unwrap();

    vesica(&["fix", "--dry-run"], &vault)
        .success()
        .stdout(predicate::str::contains("Would create"));
    assert!(!vault.join("CLAUDE.md").exists());

    vesica(&["fix"], &vault)
        .success()
        .stdout(predicate::str::contains("Kept existing: AGENTS.md"));
    assert_eq!(std::fs::read(vault.join("AGENTS.md")).unwrap(), mine);
    let gitignore = std::fs::read_to_string(vault.join(".gitignore")).unwrap();
    assert!(gitignore.starts_with("*.tmp\n"), "{gitignore}");
    assert!(gitignore.lines().any(|l| l == "/Team"), "{gitignore}");
    for entry in SKELETON {
        assert!(vault.join(entry).exists(), "missing {entry}");
    }
    // A second run is a no-op.
    vesica(&["fix"], &vault)
        .success()
        .stdout(predicate::str::contains("Nothing to fix"));
}

#[test]
fn check_reports_contract_violations_without_echoing_note_content() {
    let (_temp, vault) = new_vault();
    std::fs::remove_file(vault.join("CLAUDE.md")).unwrap();
    std::fs::write(vault.join(".gitignore"), "Team/\n").unwrap(); // misses a Team symlink
    std::fs::write(
        vault.join("Inbox/no-frontmatter.md"),
        "secret-looking body text\n",
    )
    .unwrap();
    std::fs::write(
        vault.join("Projects/bad-type.md"),
        "---\ntitle: T\ncreated: 2026-01-01\nupdated: 2026-01-01\ntype: diary\narea: x\ntags: [a]\n---\nbody\n",
    )
    .unwrap();
    std::fs::write(
        vault.join("Projects/missing-tags.md"),
        "---\ntitle: T\ncreated: 2026-01-01\nupdated: 2026-01-01\ntype: note\narea: x\n---\n",
    )
    .unwrap();
    // Exempt from vault upkeep: Team/**, AGENTS.md, CLAUDE.md (plan §5.6).
    let out = vesica(&["check"], &vault).failure().get_output().clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    for expected in [
        "VVC001 CLAUDE.md",
        "VVC002 Team",
        "VVC005 Inbox/no-frontmatter.md:1",
        "VVC006 Projects/bad-type.md",
        "VVC005 Projects/missing-tags.md",
    ] {
        assert!(
            stdout.contains(expected),
            "missing {expected:?} in\n{stdout}"
        );
    }
    assert!(!stdout.contains("secret-looking"), "{stdout}");
    assert!(
        !stdout.contains("Team/"),
        "Team content must not be checked: {stdout}"
    );
}

#[test]
fn check_reports_a_vault_that_is_not_a_git_repository() {
    let (_temp, vault) = new_vault();
    std::fs::remove_dir_all(vault.join(".git")).unwrap();
    vesica(&["check"], &vault)
        .failure()
        .stdout(predicate::str::contains("VVC003"));
}

#[test]
fn new_note_creates_a_contract_compliant_ledger_note_once() {
    let (_temp, vault) = new_vault();
    vesica(
        &[
            "new-note",
            "--task",
            "F3.3-vault",
            "--agent",
            "hermes",
            "--date",
            "2026-11-18",
        ],
        &vault,
    )
    .success()
    .stdout(predicate::str::contains(
        "Sessions/agent-runs/2026-11-18_F3.3-vault_hermes.md",
    ));
    let note = vault.join("Sessions/agent-runs/2026-11-18_F3.3-vault_hermes.md");
    let text = std::fs::read_to_string(&note).unwrap();
    assert!(text.starts_with("---\n"), "{text}");
    assert!(text.contains("type: report") && text.contains("created: 2026-11-18"));
    vesica(&["check"], &vault).success();

    // Never overwrites.
    std::fs::write(&note, "edited\n").unwrap();
    vesica(
        &[
            "new-note",
            "--task",
            "F3.3-vault",
            "--agent",
            "hermes",
            "--date",
            "2026-11-18",
        ],
        &vault,
    )
    .failure()
    .stderr(predicate::str::contains("already exists"));
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "edited\n");
}

#[test]
fn new_note_rejects_unsafe_names_and_dates() {
    let (_temp, vault) = new_vault();
    for args in [
        [
            "--task",
            "../escape",
            "--agent",
            "a",
            "--date",
            "2026-11-18",
        ],
        ["--task", "t", "--agent", "a/b", "--date", "2026-11-18"],
        ["--task", "-t", "--agent", "a", "--date", "2026-11-18"],
        ["--task", "t", "--agent", "a", "--date", "2026-13-01"],
        ["--task", "t", "--agent", "a", "--date", "18-11-2026"],
    ] {
        let mut full = vec!["new-note"];
        full.extend(args);
        vesica(&full, &vault).failure();
    }
    assert_eq!(
        std::fs::read_dir(vault.join("Sessions/agent-runs"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn snapshot_commits_personal_notes_and_excludes_team() {
    let (_temp, vault) = new_vault();
    // A real nested Team directory as well as the symlink case: both must stay out.
    std::fs::remove_file(vault.join("Team")).unwrap();
    std::fs::create_dir_all(vault.join("Team/Knowledge")).unwrap();
    std::fs::write(vault.join("Team/Knowledge/shared.md"), "team note\n").unwrap();
    std::fs::write(vault.join("Inbox/idea.md"), "---\ntitle: I\n---\nidea\n").unwrap();

    vesica(&["snapshot"], &vault)
        .success()
        .stdout(predicate::str::contains("Snapshot"));
    let tracked = git(&vault, &["ls-files"]);
    assert!(tracked.lines().any(|l| l == "Inbox/idea.md"), "{tracked}");
    assert!(tracked.lines().any(|l| l == "AGENTS.md"), "{tracked}");
    assert!(
        !tracked.contains("Team"),
        "Team must never be snapshotted:\n{tracked}"
    );
    assert_eq!(git(&vault, &["status", "--porcelain"]), "");

    vesica(&["snapshot"], &vault)
        .success()
        .stdout(predicate::str::contains("Nothing to snapshot"));

    // Data is preserved: an edit becomes a second commit.
    std::fs::write(vault.join("Inbox/idea.md"), "---\ntitle: I\n---\nidea v2\n").unwrap();
    vesica(&["snapshot", "--message", "my checkpoint"], &vault).success();
    assert_eq!(git(&vault, &["rev-list", "--count", "HEAD"]).trim(), "2");
    assert_eq!(
        git(&vault, &["log", "-1", "--format=%s"]).trim(),
        "my checkpoint"
    );
}

#[test]
fn snapshot_fails_closed_when_team_is_not_excluded() {
    let (_temp, vault) = new_vault();
    std::fs::write(vault.join(".gitignore"), "").unwrap();
    vesica(&["snapshot"], &vault)
        .failure()
        .stderr(predicate::str::contains("Team"));
    assert!(
        git(&vault, &["ls-files"]).is_empty(),
        "nothing may be committed"
    );

    // Team already tracked from an earlier mistake: refuse rather than rewrite the index.
    std::fs::write(vault.join(".gitignore"), "").unwrap();
    git(&vault, &["add", "Team"]);
    std::fs::write(vault.join(".gitignore"), "/Team\n").unwrap();
    vesica(&["snapshot"], &vault)
        .failure()
        .stderr(predicate::str::contains("tracked"));
}

#[test]
fn snapshot_refuses_when_a_personal_note_contains_a_secret() {
    let (_temp, vault) = new_vault();
    let key = format!("AKIA{}", "Q".repeat(16));
    std::fs::write(vault.join("Inbox/leak.md"), format!("aws {key}\n")).unwrap();
    let out = vesica(&["snapshot"], &vault).failure().get_output().clone();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("Inbox/leak.md:1"), "{stderr}");
    assert!(!stderr.contains(&key), "{stderr}");
    assert!(git(&vault, &["ls-files"]).is_empty());
}

// ------------------------------------------------------------------ install --vault (step 3, D-H)

fn install(home: &Path, shared: &Path, extra: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(shared)
        .arg("--root")
        .arg(home)
        .args(extra)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Vesica Test")
        .env("GIT_AUTHOR_EMAIL", "vesica@example.invalid")
        .env("GIT_COMMITTER_NAME", "Vesica Test")
        .env("GIT_COMMITTER_EMAIL", "vesica@example.invalid")
        .assert()
}

fn uninstall(home: &Path) -> assert_cmd::assert::Assert {
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(home)
        .assert()
}

/// `EX_VAULT` (fixture `team.env_prefix` = EX) after sourcing `<home>/.bashrc`.
fn sourced_vault(home: &Path) -> String {
    let out = std::process::Command::new("bash")
        .args(["--noprofile", "--norc", "-c"])
        .arg(r#". "$1/.bashrc" && printf '%s' "${EX_VAULT-unset}""#)
        .arg("bash")
        .arg(home)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
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
fn install_with_vault_creates_it_links_team_inside_and_exports_it() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let vault = temp.path().join("vault");
    let shared = fixture_home().canonicalize().unwrap();
    install(&home, &shared, &["--vault", vault.to_str().unwrap()])
        .success()
        .stdout(predicate::str::contains("Created: AGENTS.md"));

    assert!(vault.join("AGENTS.md").is_file() && vault.join(".git").is_dir());
    assert_eq!(std::fs::read_link(vault.join("Team")).unwrap(), shared);
    assert!(
        !home.join("Team").exists() && !home.join("Team").is_symlink(),
        "with --vault, Team lives in the vault instead of ~/Team"
    );
    assert_eq!(sourced_vault(&home), vault.to_str().unwrap());
    let claude = std::fs::read_to_string(home.join(".claude/CLAUDE.md")).unwrap();
    assert!(
        claude.contains(&format!("@{}", shared.join("AGENTS.md").display()))
            && claude.contains(&format!("@{}", vault.join("AGENTS.md").display())),
        "{claude}"
    );
    vesica(&["check"], &vault).success();
    vesica(&["snapshot"], &vault).success();
    assert!(!git(&vault, &["ls-files"]).contains("Team"));

    // Idempotent: a second install changes nothing.
    let out = install(&home, &shared, &["--vault", vault.to_str().unwrap()])
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).unwrap();
    assert!(
        !out.lines()
            .any(|l| l.starts_with("Created") || l.starts_with("Updated")),
        "{out}"
    );

    // Uninstall removes only the managed link; the vault is personal data and stays.
    std::fs::write(vault.join("Inbox/mine.md"), "mine\n").unwrap();
    uninstall(&home).success();
    assert!(!vault.join("Team").is_symlink());
    assert_eq!(
        std::fs::read_to_string(vault.join("Inbox/mine.md")).unwrap(),
        "mine\n"
    );
    assert!(vault.join("AGENTS.md").is_file() && vault.join(".git").is_dir());
}

#[test]
fn install_with_vault_adopts_an_existing_vault_without_overwriting() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let vault = temp.path().join("vault");
    std::fs::create_dir_all(vault.join("Projects")).unwrap();
    std::fs::write(vault.join("AGENTS.md"), "# mine\n").unwrap();
    std::fs::write(vault.join("Projects/p.md"), "project\n").unwrap();
    install(
        &home,
        &fixture_home(),
        &["--vault", vault.to_str().unwrap()],
    )
    .success()
    .stdout(predicate::str::contains("Kept existing: AGENTS.md"));
    assert_eq!(
        std::fs::read_to_string(vault.join("AGENTS.md")).unwrap(),
        "# mine\n"
    );
    assert_eq!(
        std::fs::read_to_string(vault.join("Projects/p.md")).unwrap(),
        "project\n"
    );
    assert!(vault.join("CLAUDE.md").is_file() && vault.join("Team").is_symlink());
}

#[test]
fn install_with_vault_dry_run_changes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let vault = temp.path().join("vault");
    install(
        &home,
        &fixture_home(),
        &["--vault", vault.to_str().unwrap(), "--dry-run"],
    )
    .success()
    .stdout(predicate::str::contains("Would create: AGENTS.md"))
    .stdout(predicate::str::contains(
        vault.join("Team").to_str().unwrap(),
    ));
    assert!(!vault.exists() && !home.exists());
}

#[test]
fn install_with_vault_refuses_an_unmanaged_team_before_touching_the_vault() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let vault = temp.path().join("vault");
    std::fs::create_dir_all(vault.join("Team")).unwrap();
    std::fs::write(vault.join("Team/other.md"), "not the shared home\n").unwrap();
    install(
        &home,
        &fixture_home(),
        &["--vault", vault.to_str().unwrap()],
    )
    .failure()
    .stderr(predicate::str::contains(
        "unmanaged destination already exists",
    ));
    let entries: Vec<_> = std::fs::read_dir(&vault)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        entries,
        ["Team"],
        "the vault must not be scaffolded on refusal"
    );
}

#[test]
fn install_with_vault_accepts_the_shared_home_cloned_into_the_vault() {
    // Plan step 4 layout: the shared home checkout itself lives at <vault>/Team.
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let vault = temp.path().join("vault");
    copy_tree(&fixture_home(), &vault.join("Team"));
    install(
        &home,
        &vault.join("Team"),
        &["--vault", vault.to_str().unwrap()],
    )
    .success()
    .stdout(predicate::str::contains("already at"));
    assert!(vault.join("Team").is_dir() && !vault.join("Team").is_symlink());
    assert!(vault.join("Team/agent-home.toml").is_file());
    vesica(&["snapshot"], &vault).success();
    assert!(!git(&vault, &["ls-files"]).contains("Team"));
    uninstall(&home).success();
    assert!(
        vault.join("Team/agent-home.toml").is_file(),
        "uninstall never touches the shared-home checkout"
    );
}

#[test]
fn switching_to_a_vault_removes_the_stale_home_team_link_only_if_unmodified() {
    let temp = tempfile::tempdir().unwrap();
    let shared = fixture_home().canonicalize().unwrap();

    // Unmodified ~/Team link: removed when Team moves into the vault.
    let home = temp.path().join("home");
    let vault = temp.path().join("vault");
    install(&home, &shared, &[]).success();
    assert!(home.join("Team").is_symlink());
    install(&home, &shared, &["--vault", vault.to_str().unwrap()])
        .success()
        .stdout(predicate::str::contains("Removed stale link"));
    assert!(!home.join("Team").is_symlink());
    uninstall(&home).success();
    assert!(!vault.join("Team").is_symlink());

    // Repointed by the user: kept, reported, and not removed later by uninstall either.
    let home2 = temp.path().join("home2");
    let vault2 = temp.path().join("vault2");
    install(&home2, &shared, &[]).success();
    std::fs::remove_file(home2.join("Team")).unwrap();
    std::os::unix::fs::symlink(temp.path(), home2.join("Team")).unwrap();
    install(&home2, &shared, &["--vault", vault2.to_str().unwrap()])
        .success()
        .stdout(predicate::str::contains("Kept modified stale link"));
    assert_eq!(std::fs::read_link(home2.join("Team")).unwrap(), temp.path());
    uninstall(&home2).success();
    assert_eq!(std::fs::read_link(home2.join("Team")).unwrap(), temp.path());
}
