use assert_cmd::Command;
use predicates::prelude::*;
use sha2::Digest;
use std::path::PathBuf;

fn fixture_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home")
}

#[test]
fn cli_identifies_itself_as_vesica() {
    Command::cargo_bin("vesica")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("vesica "));
}

#[test]
fn help_displays_vesica_as_the_binary_name() {
    Command::cargo_bin("vesica")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: vesica <COMMAND>"));
}

#[test]
fn lint_reports_findings_without_echoing_secret_values() {
    let temp = tempfile::tempdir().unwrap();
    let marker = format!("ghp_{}", "B".repeat(32));
    std::fs::write(temp.path().join("AGENTS.md"), format!("{marker}\n")).unwrap();
    let output = Command::cargo_bin("vesica")
        .unwrap()
        .args(["lint", "--home-repo"])
        .arg(temp.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(report.contains("AGENTS.md:1:VSH006"));
    assert!(!report.contains(&marker));
}

#[test]
fn lint_accepts_the_fixture_home_from_the_cli() {
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["lint", "--home-repo"])
        .arg(fixture_home())
        .assert()
        .success()
        .stdout(predicate::str::contains("No lint findings"));
}

#[test]
fn vault_scan_fails_on_secret_patterns_without_echoing_matched_values() {
    let temp = tempfile::tempdir().unwrap();
    let note = temp.path().join("note.md");
    let marker = format!("ghp_{}", "A".repeat(32));
    std::fs::write(&note, format!("ordinary note\n{marker}\n")).unwrap();
    let output = Command::cargo_bin("vesica")
        .unwrap()
        .args(["vault-scan", "--path"])
        .arg(temp.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(report.contains("note.md:2"));
    assert!(!report.contains(&marker));
}

#[test]
fn doctor_validates_the_shared_home_and_reports_selected_profile() {
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["doctor", "--home-repo"])
        .arg(fixture_home())
        .args(["--profile", "developer"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Shared home: valid"))
        .stdout(predicate::str::contains("Profile: developer"));
}

#[test]
fn doctor_json_emits_machine_readable_shared_home_and_wsl_status() {
    let output = Command::cargo_bin("vesica")
        .unwrap()
        .args(["doctor", "--json", "--home-repo"])
        .arg(fixture_home())
        .env("VESICA_ALLOW_NON_WSL", "1")
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["shared_home"], "valid");
    assert_eq!(report["team"]["name"], "Example Team");
    assert_eq!(report["team"]["org"], "example-org");
    assert_eq!(report["profile"], "developer");
    assert_eq!(report["wsl_kernel"], "detected");
    assert_eq!(report["live_harness"], "not-implemented");
    assert_eq!(report["foundry"], "not-implemented");
}

#[test]
fn doctor_prefers_vesica_environment_variables_over_legacy_names() {
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["doctor"])
        .env("VESICA_HOME_REPO", fixture_home())
        .env("AGENT_HOME_REPO", "/vesica-should-win-over-legacy")
        .assert()
        .success()
        .stdout(predicate::str::contains("Shared home: valid"));
}

#[test]
fn doctor_json_applies_requested_profile_to_repository_listing() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("agent-home.toml");
    std::fs::write(
        &config,
        r#"schema = 1
[team]
name = "Example Team"
org = "example-org"
env_prefix = "EX"
role_prefix = "ex-"
[profiles]
default = "developer"
[[repos]]
name = "core-only"
url = "https://github.com/example-org/core-only.git"
profiles = ["core"]
[[repos]]
name = "dev-lead"
url = "https://github.com/example-org/dev-lead.git"
profiles = ["developer", "lead"]
[[repos]]
name = "all-profiles"
url = "https://github.com/example-org/all.git"
profiles = []
"#,
    )
    .unwrap();

    for (profile, expected) in [
        ("core", vec!["core-only", "all-profiles"]),
        ("developer", vec!["dev-lead", "all-profiles"]),
        ("lead", vec!["dev-lead", "all-profiles"]),
    ] {
        let output = Command::cargo_bin("vesica")
            .unwrap()
            .args(["doctor", "--json", "--home-repo"])
            .arg(temp.path())
            .args(["--profile", profile])
            .env("VESICA_ALLOW_NON_WSL", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["profile"], profile);
        assert_eq!(report["repos_in_config"], 3);
        assert_eq!(report["repos"], serde_json::json!(expected));
    }
}

#[test]
fn install_records_the_selected_profile_and_keeps_shared_skills_for_every_profile() {
    let temp = tempfile::tempdir().unwrap();
    let home_repo = temp.path().join("shared-home");
    std::fs::create_dir_all(home_repo.join("Agents/Skills")).unwrap();
    std::fs::write(home_repo.join("AGENTS.md"), "# Shared instructions\n").unwrap();
    std::fs::write(
        home_repo.join("agent-home.toml"),
        r#"schema = 1
[team]
name = "Example Team"
org = "example-org"
env_prefix = "EX"
role_prefix = "ex-"
[profiles]
default = "developer"
[[repos]]
name = "core-only"
url = "https://github.com/example-org/core-only.git"
profiles = ["core"]
[[repos]]
name = "developer-only"
url = "https://github.com/example-org/developer-only.git"
profiles = ["developer"]
[[repos]]
name = "shared-repo"
url = "https://github.com/example-org/shared-repo.git"
profiles = []
"#,
    )
    .unwrap();
    std::fs::create_dir_all(home_repo.join("Agents/Skills/shared-skill")).unwrap();
    std::fs::write(
        home_repo.join("Agents/Skills/shared-skill/SKILL.md"),
        "---\nname: shared-skill\ndescription: Shared example\n---\n",
    )
    .unwrap();

    let root = temp.path().join("user-home");
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(&home_repo)
        .args(["--profile", "core", "--root"])
        .arg(&root)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "install plan (profile: core; eligible repositories: core-only, shared-repo)",
        ))
        .stdout(predicate::str::contains(
            "Repository cloning is not implemented",
        ));

    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["profile"], "core");
    let skill = root.join(".agents/skills/shared-skill");
    assert!(std::fs::symlink_metadata(&skill)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read_link(skill).unwrap(),
        home_repo
            .canonicalize()
            .unwrap()
            .join("Agents/Skills/shared-skill")
    );
}

#[test]
fn install_rejects_unsupported_profile_before_mutating_user_home() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("user-home");
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--profile", "invalid", "--root"])
        .arg(&root)
        .assert()
        .failure();
    assert!(!root.exists());
}

#[test]
fn install_fails_closed_on_a_collision_without_creating_backups_or_other_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("user-home");
    let team = root.join("Team");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(&team, "user-owned destination").unwrap();
    let claude = root.join(".claude/CLAUDE.md");
    std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
    std::fs::write(&claude, "# existing rules\n").unwrap();
    let original = std::fs::read(&claude).unwrap();
    let output = Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .arg("--root")
        .arg(&root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&claude).unwrap(), original);
    assert_eq!(std::fs::read(&team).unwrap(), b"user-owned destination");
    assert!(!root.join(".codex/AGENTS.md").exists());
    assert!(!root.join(".config/agent-home/manifest.json").exists());
    assert_eq!(
        std::fs::read_dir(claude.parent().unwrap()).unwrap().count(),
        1,
        "preflight failure must not leave backup artifacts"
    );
}

#[test]
fn install_links_shared_skills_for_agents_and_uninstall_removes_only_those_links() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .arg("--root")
        .arg(&root)
        .assert()
        .success();

    for name in ["demo-skill", "second-skill"] {
        for base in [root.join(".agents/skills"), root.join(".claude/skills")] {
            let link = base.join(name);
            assert!(std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink());
            assert_eq!(
                std::fs::read_link(link).unwrap(),
                fixture_home()
                    .canonicalize()
                    .unwrap()
                    .join("Agents/Skills")
                    .join(name)
            );
        }
    }
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .success();
    assert!(!root.join(".agents/skills/demo-skill").exists());
    assert!(!root.join(".claude/skills/second-skill").exists());
}

#[test]
fn doctor_reports_wsl_kernel_detection_with_vesica_override() {
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["doctor", "--home-repo"])
        .arg(fixture_home())
        .env("VESICA_ALLOW_NON_WSL", "1")
        .assert()
        .success()
        .stdout(predicate::str::contains("WSL kernel: detected"));
}

#[test]
fn uninstall_uses_vesica_root_environment_variable() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    let config = root.join(".config/agent-home");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("manifest.json"),
        r#"{"schema":1,"app_version":"0.1.0","shared_home":"","profile":"developer","entries":{}}"#,
    )
    .unwrap();

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["status"])
        .env("VESICA_ROOT", &root)
        .env("AGENT_HOME_ROOT", "/vesica-should-win-over-legacy")
        .assert()
        .success()
        .stdout(predicate::str::contains(root.to_string_lossy().as_ref()))
        .stdout(predicate::str::contains("(not installed)"));
}

#[test]
fn doctor_reports_kernel_detection_and_explicit_override() {
    let without = Command::cargo_bin("vesica")
        .unwrap()
        .args(["doctor", "--home-repo"])
        .arg(fixture_home())
        .env_remove("VESICA_ALLOW_NON_WSL")
        .env_remove("AGENT_HOME_ALLOW_NON_WSL")
        .assert()
        .success();
    without.stdout(predicate::str::contains("WSL kernel:"));

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["doctor", "--home-repo"])
        .arg(fixture_home())
        .env("VESICA_ALLOW_NON_WSL", "1")
        .assert()
        .success()
        .stdout(predicate::str::contains("WSL kernel: detected"));
}

#[test]
fn doctor_fails_for_a_missing_shared_home() {
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["doctor", "--home-repo", "/no/such/shared-home"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("agent-home.toml"));
}

#[test]
fn install_dry_run_shows_plan_without_mutating_the_selected_home() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("user-home");
    std::fs::create_dir_all(&root).unwrap();
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--profile", "developer", "--root"])
        .arg(&root)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("DRY RUN"))
        .stdout(predicate::str::contains("AGENTS.md"));
    assert!(!root.join(".config/agent-home/manifest.json").exists());
    assert!(!root.join(".claude/CLAUDE.md").exists());
}

#[test]
fn install_preserves_and_backs_up_existing_user_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("user-home");
    let config_dir = root.join(".config/agent-home");
    let claude = root.join(".claude/CLAUDE.md");
    let codex = root.join(".codex/AGENTS.md");
    std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
    std::fs::create_dir_all(codex.parent().unwrap()).unwrap();
    std::fs::write(&claude, "# My own Claude rules\nkeep this\n").unwrap();
    std::fs::write(&codex, "my codex rules\n").unwrap();

    let run = || {
        Command::cargo_bin("vesica")
            .unwrap()
            .args(["install", "--home-repo"])
            .arg(fixture_home())
            .args(["--profile", "developer", "--root"])
            .arg(&root)
            .assert()
            .success();
    };
    run();
    let installed = std::fs::read_to_string(&claude).unwrap();
    assert!(installed.starts_with("# My own Claude rules\nkeep this\n"));
    assert_eq!(installed.matches("# >>> vesica >>>").count(), 1);
    let backup_count = std::fs::read_dir(claude.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains("vesica-backup")
        })
        .count();
    run();
    assert_eq!(std::fs::read_to_string(&claude).unwrap(), installed);
    let backup_count_after_repeat = std::fs::read_dir(claude.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains("vesica-backup")
        })
        .count();
    assert_eq!(
        backup_count_after_repeat, backup_count,
        "reinstall should not leave redundant backups"
    );
    assert!(
        std::fs::symlink_metadata(config_dir.join("manifest.json.tmp")).is_err(),
        "a successful reinstall should not leave its atomic-save temp file"
    );

    let codex_backup = std::fs::read_dir(codex.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains("vesica-backup")
        })
        .unwrap()
        .path();
    assert_eq!(std::fs::read(codex_backup).unwrap(), b"my codex rules\n");
    let codex_backup_dir = codex.parent().unwrap().to_path_buf();
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap(),
    )
    .unwrap();
    let entry = &manifest["entries"][claude.to_string_lossy().as_ref()];
    assert!(entry["backup"].as_str().is_some());
    assert!(std::path::Path::new(entry["backup"].as_str().unwrap()).exists());
    let codex_manifest = &manifest["entries"][codex.to_string_lossy().as_ref()];
    let codex_recorded_backup = codex_manifest["backup"].as_str().unwrap();
    assert!(std::path::Path::new(codex_recorded_backup).exists());
    run();
    assert_eq!(
        std::fs::read_dir(codex_backup_dir).unwrap().count(),
        2,
        "repeat install preserves the existing recorded backup rather than creating another"
    );
    assert!(
        std::fs::symlink_metadata(root.join(".config/agent-home/manifest.json.tmp")).is_err(),
        "a successful reinstall should not leave its atomic-save temp file"
    );
}

#[test]
fn reinstall_refuses_to_use_a_modified_recorded_backup() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("user-home");
    let claude = root.join(".claude/CLAUDE.md");
    std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
    std::fs::write(&claude, "user-authored Claude rules\n").unwrap();

    let run = || {
        Command::cargo_bin("vesica")
            .unwrap()
            .args(["install", "--home-repo"])
            .arg(fixture_home())
            .args(["--profile", "developer", "--root"])
            .arg(&root)
            .output()
            .unwrap()
    };
    assert!(run().status.success(), "initial install should succeed");
    let installed = std::fs::read(&claude).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap(),
    )
    .unwrap();
    let backup = std::path::PathBuf::from(
        manifest["entries"][claude.to_string_lossy().as_ref()]["backup"]
            .as_str()
            .unwrap(),
    );
    std::fs::write(&backup, "user changed recorded backup\n").unwrap();

    let second = run();
    assert!(!second.status.success(), "modified backup must fail closed");
    let diagnostic = String::from_utf8_lossy(&second.stdout).to_string()
        + &String::from_utf8_lossy(&second.stderr);
    assert!(
        diagnostic.contains("recorded backup was modified"),
        "{diagnostic}"
    );

    assert_eq!(std::fs::read(&claude).unwrap(), installed);
    assert_eq!(
        std::fs::read(&backup).unwrap(),
        b"user changed recorded backup\n"
    );
}

#[test]
fn reinstall_refuses_to_follow_a_symlink_at_recorded_backup() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("user-home");
        let claude = root.join(".claude/CLAUDE.md");
        std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
        std::fs::write(&claude, "user-authored Claude rules\n").unwrap();
        let run = || {
            Command::cargo_bin("vesica")
                .unwrap()
                .args(["install", "--home-repo"])
                .arg(fixture_home())
                .args(["--profile", "developer", "--root"])
                .arg(&root)
                .output()
                .unwrap()
        };
        assert!(run().status.success());
        let installed = std::fs::read(&claude).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap(),
        )
        .unwrap();
        let backup = std::path::PathBuf::from(
            manifest["entries"][claude.to_string_lossy().as_ref()]["backup"]
                .as_str()
                .unwrap(),
        );
        let outside = temp.path().join("outside-user-file");
        std::fs::write(&outside, "outside file must never be followed").unwrap();
        std::fs::remove_file(&backup).unwrap();
        symlink(&outside, &backup).unwrap();

        let second = run();
        assert!(!second.status.success(), "backup symlink must fail closed");
        let diagnostic = String::from_utf8_lossy(&second.stdout).to_string()
            + &String::from_utf8_lossy(&second.stderr);
        assert!(diagnostic.contains("recorded backup is not a regular file"));
        assert_eq!(
            std::fs::read(&outside).unwrap(),
            b"outside file must never be followed"
        );
        assert_eq!(std::fs::read(&claude).unwrap(), installed);
        assert!(std::fs::symlink_metadata(&backup)
            .unwrap()
            .file_type()
            .is_symlink());
    }
}

#[test]
fn doctor_confirms_installed_fixture_from_selected_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .assert()
        .success();

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["doctor", "--json", "--home-repo"])
        .arg(fixture_home())
        .env("VESICA_ROOT", &root)
        .env("VESICA_ALLOW_NON_WSL", "1")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"shared_home\": \"valid\""));
}

#[test]
fn install_manifest_write_failure_rolls_back_managed_actions_and_backups() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    let config = root.join(".config/agent-home");
    std::fs::create_dir_all(root.join(".config")).unwrap();
    symlink("missing-manifest-target", &config).unwrap();
    let claude = root.join(".claude/CLAUDE.md");
    let codex = root.join(".codex/AGENTS.md");
    std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
    std::fs::create_dir_all(codex.parent().unwrap()).unwrap();
    std::fs::write(&claude, "user-owned Claude config\n").unwrap();
    std::fs::write(&codex, "user-owned Codex config\n").unwrap();

    let output = Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "manifest destination symlink must fail closed"
    );
    assert_eq!(
        ah_config::block::remove(
            &std::fs::read_to_string(&claude).unwrap(),
            ah_config::block::CommentStyle::Hash,
            "vesica",
        ),
        "user-owned Claude config\n"
    );
    assert_eq!(
        ah_config::block::remove(
            &std::fs::read_to_string(&codex).unwrap(),
            ah_config::block::CommentStyle::Hash,
            "vesica",
        ),
        "user-owned Codex config\n"
    );
    assert!(!root.join("Team").exists());
    assert!(!root.join(".agents/skills/demo-skill").exists());
    assert!(config.is_symlink(), "never replace the user-owned blocker");
    for directory in [claude.parent().unwrap(), codex.parent().unwrap()] {
        assert_eq!(
            std::fs::read_dir(directory).unwrap().count(),
            1,
            "failed install must remove any Vesica backup artifacts"
        );
    }
}

#[test]
fn install_manifest_persistence_failure_rolls_back_actions_and_new_backups() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    let config = root.join(".config/agent-home");
    let blocked_temp = config.join("manifest.json.tmp");
    std::fs::create_dir_all(&blocked_temp).unwrap();
    let claude = root.join(".claude/CLAUDE.md");
    let codex = root.join(".codex/AGENTS.md");
    std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
    std::fs::create_dir_all(codex.parent().unwrap()).unwrap();
    let claude_original = b"user-owned Claude config\n";
    let codex_original = b"user-owned Codex config\n";
    std::fs::write(&claude, claude_original).unwrap();
    std::fs::write(&codex, codex_original).unwrap();

    let output = Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "manifest temporary-path obstruction must fail install"
    );
    assert_eq!(std::fs::read(&claude).unwrap(), claude_original);
    assert_eq!(std::fs::read(&codex).unwrap(), codex_original);
    assert!(!root.join("Team").exists());
    assert!(!root.join(".agents/skills/demo-skill").exists());
    assert!(blocked_temp.is_dir(), "preserve the user-owned obstruction");
    assert!(!config.join("manifest.json").exists());
    for directory in [claude.parent().unwrap(), codex.parent().unwrap()] {
        let backups = std::fs::read_dir(directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .contains("vesica-backup")
            })
            .count();
        assert_eq!(
            backups, 0,
            "manifest-save failure must remove created backups"
        );
    }
}

#[test]
fn install_writes_a_manifest_and_uninstalls_only_untouched_managed_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .assert()
        .success();

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["status", "--root"])
        .arg(&root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Recorded entries: 7"));

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("DRY RUN uninstall inspection"));

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed manifest"));
    assert!(!root.join("Team").exists());
    assert!(!root.join(".claude/CLAUDE.md").exists());
}

#[test]
fn uninstall_restores_legacy_agent_home_managed_blocks() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    let target = root.join(".claude/CLAUDE.md");
    let original = "legacy user rules\n";
    let installed = format!(
        "{original}\n# >>> agent-home >>>\n@/legacy/shared/AGENTS.md\n# <<< agent-home <<<\n"
    );
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, &installed).unwrap();
    let backup = target.with_file_name("CLAUDE.md.agent-home-backup-legacy");
    std::fs::write(&backup, original).unwrap();

    let mut manifest = serde_json::json!({
        "schema": 1,
        "app_version": "0.1.0",
        "shared_home": "/legacy/shared",
        "profile": "developer",
        "entries": {}
    });
    let hash = hex::encode(sha2::Sha256::digest(installed.as_bytes()));
    let original_hash = hex::encode(sha2::Sha256::digest(original.as_bytes()));
    manifest["entries"][target.to_string_lossy().as_ref()] = serde_json::json!({
        "kind": "managed-block",
        "target": target.to_string_lossy(),
        "source": null,
        "backup": backup.to_string_lossy(),
        "original_sha256": original_hash,
        "installed_sha256": hash
    });
    let config = root.join(".config/agent-home");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Restoring original file"));

    assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
    assert!(!config.join("manifest.json").exists());
}

#[test]
fn uninstall_restores_backed_up_user_content_if_nothing_changed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    let claude = root.join(".claude/CLAUDE.md");
    std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
    std::fs::write(&claude, "my preserved rules\n").unwrap();
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .arg("--root")
        .arg(&root)
        .assert()
        .success();
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(&claude).unwrap(),
        "my preserved rules\n"
    );
    assert!(!root.join(".config/agent-home/manifest.json").exists());
}

#[test]
fn uninstall_refuses_to_follow_a_symlinked_recorded_backup() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("home");
        let claude = root.join(".claude/CLAUDE.md");
        let original = b"my preserved rules\n";
        std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
        std::fs::write(&claude, original).unwrap();
        Command::cargo_bin("vesica")
            .unwrap()
            .args(["install", "--home-repo"])
            .arg(fixture_home())
            .args(["--root"])
            .arg(&root)
            .assert()
            .success();
        let installed = std::fs::read(&claude).unwrap();
        let manifest_path = root.join(".config/agent-home/manifest.json");
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        let recorded = &manifest["entries"][claude.to_string_lossy().as_ref()];
        let backup = PathBuf::from(recorded["backup"].as_str().unwrap());
        let expected = recorded["original_sha256"].as_str().unwrap();
        let original_hash = hex::encode(sha2::Sha256::digest(original));
        assert_eq!(expected, original_hash);
        let outside = temp.path().join("outside-user-file");
        // Matching original bytes ensure a hash-only check would incorrectly accept the symlink.
        std::fs::write(&outside, original).unwrap();
        std::fs::remove_file(&backup).unwrap();
        symlink(&outside, &backup).unwrap();

        let result = Command::cargo_bin("vesica")
            .unwrap()
            .args(["uninstall", "--root"])
            .arg(&root)
            .output()
            .unwrap();
        assert!(
            !result.status.success(),
            "uninstall must fail closed on a symlinked backup"
        );
        let diagnostic = String::from_utf8_lossy(&result.stdout).to_string()
            + &String::from_utf8_lossy(&result.stderr);
        assert!(diagnostic.contains("recorded backup is not a regular file"));
        assert_eq!(std::fs::read(&claude).unwrap(), installed);
        assert_eq!(std::fs::read(&outside).unwrap(), original);
        assert!(
            root.join("Team").exists(),
            "preflight refusal must happen before removing any earlier manifest entries"
        );
        assert!(
            root.join(".agents/skills/demo-skill").exists(),
            "preflight refusal must preserve all managed links"
        );
        assert!(manifest_path.is_file(), "retain manifest for safe recovery");
        assert!(std::fs::symlink_metadata(&backup)
            .unwrap()
            .file_type()
            .is_symlink());
    }
}

#[test]
fn uninstall_preserves_hashless_backup_and_retains_manifest_entry() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    let claude = root.join(".claude/CLAUDE.md");
    std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
    std::fs::write(&claude, "user-authored Claude rules\n").unwrap();
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .assert()
        .success();

    let installed = std::fs::read(&claude).unwrap();
    let manifest_path = root.join(".config/agent-home/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    let record = manifest["entries"][claude.to_string_lossy().as_ref()]
        .as_object_mut()
        .unwrap();
    assert!(record.remove("original_sha256").is_some());
    let backup = PathBuf::from(record["backup"].as_str().unwrap());
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    std::fs::write(&manifest_path, &manifest_bytes).unwrap();

    let result = Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "uninstall should preserve unverifiable legacy backup entries"
    );
    let diagnostic = String::from_utf8_lossy(&result.stdout).to_string()
        + &String::from_utf8_lossy(&result.stderr);
    assert!(diagnostic.contains("Skipped block without original hash"));
    assert_eq!(std::fs::read(&claude).unwrap(), installed);
    assert!(backup.is_file(), "preserve backup without a trusted hash");
    assert!(!root.join("Team").exists());
    assert!(!root.join(".agents/skills/demo-skill").exists());
    let retained: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    assert!(retained["entries"][claude.to_string_lossy().as_ref()].is_object());
}

#[test]
fn uninstall_skips_a_user_modified_managed_file() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .arg("--root")
        .arg(&root)
        .assert()
        .success();
    let claude = root.join(".claude/CLAUDE.md");
    std::fs::write(&claude, "human added a rule\n").unwrap();
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(&claude).unwrap(),
        "human added a rule\n"
    );
    assert!(root.join(".config/agent-home/manifest.json").exists());
}
