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
    assert_eq!(std::fs::read_dir(codex_backup_dir).unwrap().count(), 2);
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
