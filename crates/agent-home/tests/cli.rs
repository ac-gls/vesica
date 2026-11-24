use assert_cmd::Command;
use predicates::prelude::*;
use sha2::Digest;
use std::path::PathBuf;

fn fixture_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home")
}

struct SharedGitFixture {
    _temp: tempfile::TempDir,
    remote: PathBuf,
    local: PathBuf,
}
impl SharedGitFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let remote = temp.path().join("remote.git");
        let seed = temp.path().join("seed");
        let local = temp.path().join("local");
        run_git(temp.path(), ["init", "--bare", remote.to_str().unwrap()]);
        run_git(temp.path(), ["init", "-b", "main", seed.to_str().unwrap()]);
        run_git(&seed, ["config", "user.name", "Vesica Test"]);
        run_git(&seed, ["config", "user.email", "vesica@example.invalid"]);
        std::fs::write(seed.join("README.md"), "shared home\\n").unwrap();
        run_git(&seed, ["add", "README.md"]);
        run_git(&seed, ["commit", "-m", "initial"]);
        run_git(&seed, ["remote", "add", "origin", remote.to_str().unwrap()]);
        run_git(&seed, ["push", "-u", "origin", "main"]);
        run_git(&remote, ["symbolic-ref", "HEAD", "refs/heads/main"]);
        run_git(
            temp.path(),
            ["clone", remote.to_str().unwrap(), local.to_str().unwrap()],
        );
        run_git(&local, ["config", "user.name", "Vesica Test"]);
        run_git(&local, ["config", "user.email", "vesica@example.invalid"]);
        Self {
            _temp: temp,
            remote,
            local,
        }
    }
    fn advance_remote(&self) {
        let update = self._temp.path().join("update");
        run_git(
            self._temp.path(),
            [
                "clone",
                self.remote.to_str().unwrap(),
                update.to_str().unwrap(),
            ],
        );
        run_git(&update, ["config", "user.name", "Vesica Test"]);
        run_git(&update, ["config", "user.email", "vesica@example.invalid"]);
        std::fs::write(update.join("shared.md"), "approved update\\n").unwrap();
        run_git(&update, ["add", "shared.md"]);
        run_git(&update, ["commit", "-m", "approved update"]);
        run_git(&update, ["push", "origin", "HEAD:main"]);
    }
}
fn run_git<I, S>(cwd: &std::path::Path, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn team_cli_reports_and_fast_forwards_the_shared_checkout() {
    let fixture = SharedGitFixture::new();
    fixture.advance_remote();
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["team", "status", "--path"])
        .arg(&fixture.local)
        .assert()
        .success()
        .stdout(predicate::str::contains("Working tree: clean"))
        .stdout(predicate::str::contains("origin/main"));
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["team", "pull", "--path"])
        .arg(&fixture.local)
        .assert()
        .success()
        .stdout(predicate::str::contains("Fast-forwarded origin/main"));
    assert_eq!(
        std::fs::read_to_string(fixture.local.join("shared.md")).unwrap(),
        "approved update\\n"
    );
}

#[test]
fn team_cli_rejects_dirty_checkout_without_overwriting_local_content() {
    let fixture = SharedGitFixture::new();
    std::fs::write(
        fixture.local.join("README.md"),
        "personal uncommitted edit\\n",
    )
    .unwrap();
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["team", "pull", "--path"])
        .arg(&fixture.local)
        .assert()
        .failure()
        .stderr(predicate::str::contains("uncommitted changes"));
    assert_eq!(
        std::fs::read_to_string(fixture.local.join("README.md")).unwrap(),
        "personal uncommitted edit\\n"
    );
}

#[test]
fn team_cli_uses_team_home_environment_variable() {
    let fixture = SharedGitFixture::new();
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["team", "status"])
        .env("VESICA_TEAM_HOME", &fixture.local)
        .assert()
        .success()
        .stdout(predicate::str::contains("Branch: main"));
}

#[test]
fn help_exposes_team_status_and_pull_commands() {
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["team", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("pull"));
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
        .stdout(predicate::str::contains("Shared home: "))
        .stdout(predicate::str::contains("(flag/env)"))
        .stdout(predicate::str::contains("Profile: developer (flag/env)"));
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
        .stdout(predicate::str::contains("Shared home: "))
        .stdout(predicate::str::contains("(flag/env)"));
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
        ("core", Vec::<&str>::new()),
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
        assert_eq!(report["repos_in_config"], 2);
        assert_eq!(report["repos"], serde_json::json!(expected));
    }
}

#[test]
fn config_rejects_repositories_assigned_to_the_core_profile() {
    let temp = tempfile::tempdir().unwrap();
    let toml = std::fs::read_to_string(fixture_home().join("agent-home.toml")).unwrap();
    std::fs::write(
        temp.path().join("agent-home.toml"),
        toml.replace(
            r#"profiles = ["developer", "lead"]"#,
            r#"profiles = ["core", "lead"]"#,
        ),
    )
    .unwrap();
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["doctor", "--home-repo"])
        .arg(temp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "repository sample-app lists the core profile; the core profile has no repositories",
        ));
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
            "install plan (profile: core; eligible repositories: none)",
        ))
        .stdout(predicate::str::contains(
            "Repository cloning skipped: the core profile does not clone repositories",
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
fn uninstall_rejects_legacy_relative_manifest_targets_before_mutation() {
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
    let manifest_path = root.join(".config/agent-home/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    let (key, entry) = manifest["entries"]
        .as_object_mut()
        .unwrap()
        .iter_mut()
        .next()
        .unwrap();
    let target = PathBuf::from(entry["target"].as_str().unwrap());
    *entry.get_mut("target").unwrap() =
        serde_json::Value::String(target.file_name().unwrap().to_string_lossy().into_owned());
    let key = key.clone();
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let original_manifest = std::fs::read(&manifest_path).unwrap();
    let team_link = root.join("Team");
    assert!(team_link.is_symlink());

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not absolute"));

    assert!(
        team_link.is_symlink(),
        "uninstall must fail before mutation"
    );
    assert_eq!(std::fs::read(&manifest_path).unwrap(), original_manifest);
    assert!(manifest["entries"].get(&key).is_some());
}

#[test]
fn install_persists_absolute_manifest_paths_for_a_relative_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = PathBuf::from("relative-home");
    Command::cargo_bin("vesica")
        .unwrap()
        .current_dir(temp.path())
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .arg("--root")
        .arg(&root)
        .assert()
        .success();

    let manifest_path = temp
        .path()
        .join(&root)
        .join(".config/agent-home/manifest.json");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    for (key, entry) in manifest["entries"].as_object().unwrap() {
        let target = PathBuf::from(entry["target"].as_str().unwrap());
        assert!(
            target.is_absolute(),
            "manifest target for {key} must be absolute: {target:?}"
        );
        if let Some(backup) = entry["backup"].as_str() {
            assert!(
                PathBuf::from(backup).is_absolute(),
                "manifest backup for {key} must be absolute: {backup}"
            );
        }
    }
}

#[cfg(unix)]
fn install_with_symlinked_config_ancestor() -> (tempfile::TempDir, PathBuf, PathBuf, Vec<u8>) {
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

    let config = root.join(".config");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::rename(&config, outside.join(".config")).unwrap();
    let manifest_path = outside.join(".config/agent-home/manifest.json");
    let manifest = std::fs::read(&manifest_path).unwrap();
    let sentinel = outside.join("opencode/opencode.json");
    std::fs::create_dir_all(sentinel.parent().unwrap()).unwrap();
    std::fs::write(&sentinel, b"{\"user\": true}\n").unwrap();
    std::os::unix::fs::symlink(outside.join(".config"), &config).unwrap();
    (temp, root, sentinel, manifest)
}

#[cfg(unix)]
#[test]
fn uninstall_rejects_symlinked_config_ancestor_before_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    std::fs::create_dir_all(&root).unwrap();
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::create_dir_all(outside.join("agent-home")).unwrap();
    let manifest = root.join(".config/agent-home/manifest.json");
    let sentinel = outside.join("outside-sentinel");
    std::fs::write(&sentinel, b"unchanged\n").unwrap();
    std::os::unix::fs::symlink(&outside, root.join(".config")).unwrap();
    let hostile_manifest = serde_json::json!({
        "schema": 1,
        "app_version": "0.1.0-alpha.4",
        "shared_home": fixture_home().canonicalize().unwrap(),
        "profile": "core",
        "vault": null,
        "entries": {
            sentinel.to_string_lossy().as_ref(): {
                "kind": "managed-block",
                "target": sentinel,
                "source": null,
                "backup": null,
                "original_sha256": null,
                "installed_sha256": null
            }
        }
    });
    std::fs::write(
        &manifest,
        serde_json::to_vec_pretty(&hostile_manifest).unwrap(),
    )
    .unwrap();

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "managed config directory resolves outside the selected root",
        ));

    assert_eq!(std::fs::read(&sentinel).unwrap(), b"unchanged\n");
}

#[cfg(unix)]
#[test]
fn install_rejects_symlinked_manifest_path_ancestor_before_mutation() {
    let (_temp, root, sentinel, manifest) = install_with_symlinked_config_ancestor();

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "managed config directory resolves outside the selected root",
        ));

    assert_eq!(std::fs::read(&sentinel).unwrap(), b"{\"user\": true}\n");
    assert_eq!(
        std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap(),
        manifest
    );
}

#[cfg(unix)]
#[test]
fn prune_rejects_symlinked_manifest_path_ancestor_before_mutation() {
    let (_temp, root, sentinel, manifest) = install_with_symlinked_config_ancestor();

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "managed config directory resolves outside the selected root",
        ));

    assert_eq!(std::fs::read(&sentinel).unwrap(), b"{\"user\": true}\n");
    assert_eq!(
        std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap(),
        manifest
    );
}

#[cfg(unix)]
#[test]
fn uninstall_rejects_symlinked_manifest_path_ancestor_before_mutation() {
    let (_temp, root, sentinel, manifest) = install_with_symlinked_config_ancestor();

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "managed config directory resolves outside the selected root",
        ));

    assert_eq!(std::fs::read(&sentinel).unwrap(), b"{\"user\": true}\n");
    assert_eq!(
        std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap(),
        manifest
    );
}

#[cfg(unix)]
fn add_hostile_manifest_symlink(root: &std::path::Path, target: &std::path::Path) -> Vec<u8> {
    use std::os::unix::fs::symlink;

    let manifest_path = root.join(".config/agent-home/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    let shared = manifest["shared_home"].as_str().unwrap().to_owned();
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    symlink(&shared, target).unwrap();
    manifest["entries"][target.to_string_lossy().as_ref()] = serde_json::json!({
        "kind": "symlink",
        "target": target,
        "source": shared,
        "backup": null,
        "original_sha256": null,
        "installed_sha256": null
    });
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::read(manifest_path).unwrap()
}

fn add_hostile_manifest_backup(root: &std::path::Path, backup: &std::path::Path) -> Vec<u8> {
    let manifest_path = root.join(".config/agent-home/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    let entry = manifest["entries"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    entry["backup"] = serde_json::Value::String(backup.to_string_lossy().into_owned());
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::read(manifest_path).unwrap()
}

#[cfg(unix)]
#[test]
fn install_rejects_external_manifest_backup_before_mutation() {
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
    let manifest = add_hostile_manifest_backup(&root, &temp.path().join("outside/backup"));

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("backup escapes"));

    assert!(root.join("Team").is_symlink());
    assert_eq!(
        std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap(),
        manifest
    );
}

#[cfg(unix)]
#[test]
fn uninstall_rejects_external_manifest_backup_before_mutation() {
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
    let manifest = add_hostile_manifest_backup(&root, &temp.path().join("outside/backup"));

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("backup escapes"));

    assert!(root.join("Team").is_symlink());
    assert_eq!(
        std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap(),
        manifest
    );
}

#[cfg(unix)]
#[test]
fn prune_rejects_external_manifest_backup_before_mutation() {
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
    let manifest = add_hostile_manifest_backup(&root, &temp.path().join("outside/backup"));

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("backup escapes"));

    assert!(root.join("Team").is_symlink());
    assert_eq!(
        std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap(),
        manifest
    );
}

#[cfg(unix)]
#[test]
fn uninstall_rejects_manifest_vault_team_escape_before_removal() {
    use std::os::unix::fs::symlink;

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

    let shared_home = fixture_home().canonicalize().unwrap();
    let outside_vault = temp.path().join("outside-vault");
    std::fs::create_dir_all(&outside_vault).unwrap();
    let outside_team = outside_vault.join("Team");
    symlink(&shared_home, &outside_team).unwrap();

    let manifest_path = root.join(".config/agent-home/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["vault"] = serde_json::Value::String(outside_vault.to_string_lossy().into_owned());
    manifest["entries"][outside_team.to_string_lossy().as_ref()] = serde_json::json!({
        "kind": "symlink",
        "target": outside_team,
        "source": shared_home,
        "backup": null,
        "original_sha256": null,
        "installed_sha256": null
    });
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let malicious_manifest = std::fs::read(&manifest_path).unwrap();

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "manifest vault is not authorized by local config",
        ));

    assert!(outside_team.is_symlink(), "outside Team link must remain");
    assert_eq!(std::fs::read(&manifest_path).unwrap(), malicious_manifest);
}

#[cfg(unix)]
#[test]
fn install_allows_external_vault_and_persists_authorization_for_uninstall() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    let vault = temp.path().join("outside-vault");

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .args(["--vault"])
        .arg(&vault)
        .assert()
        .success();

    let config = std::fs::read_to_string(root.join(".config/agent-home/config.toml")).unwrap();
    assert!(
        config.contains(&format!("vault = \"{}\"", vault.display())),
        "{config}"
    );
    let team_link = vault.join("Team");
    assert!(team_link.is_symlink());
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .success();
    assert!(!team_link.exists());
}

#[cfg(unix)]
#[test]
fn external_vault_path_with_symlinked_ancestor_is_rejected_before_mutation() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    let real_parent = temp.path().join("real-parent");
    std::fs::create_dir(&real_parent).unwrap();
    let alias = temp.path().join("alias");
    symlink(&real_parent, &alias).unwrap();
    let vault = alias.join("vault");

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .args(["--vault"])
        .arg(&vault)
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink"));

    assert!(!root.exists(), "refusal must precede machine-root mutation");
    assert!(!real_parent.join("vault").exists());
}

#[cfg(unix)]
#[test]
fn uninstall_rejects_symlinked_local_config_as_vault_authorization() {
    use std::os::unix::fs::{symlink, symlink as symlink_file};

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

    let shared = fixture_home().canonicalize().unwrap();
    let outside_vault = temp.path().join("outside-vault");
    std::fs::create_dir_all(&outside_vault).unwrap();
    let outside_team = outside_vault.join("Team");
    symlink(&shared, &outside_team).unwrap();
    let manifest_path = root.join(".config/agent-home/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["vault"] = serde_json::Value::String(outside_vault.to_string_lossy().into_owned());
    manifest["entries"][outside_team.to_string_lossy().as_ref()] = serde_json::json!({
        "kind": "symlink", "target": outside_team, "source": shared,
        "backup": null, "original_sha256": null, "installed_sha256": null
    });
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let manifest_before = std::fs::read(&manifest_path).unwrap();

    let config_path = root.join(".config/agent-home/config.toml");
    std::fs::remove_file(&config_path).unwrap();
    let attacker_config = temp.path().join("attacker-config.toml");
    std::fs::write(
        &attacker_config,
        format!("schema = 1\nvault = \"{}\"\n", outside_vault.display()),
    )
    .unwrap();
    symlink_file(&attacker_config, &config_path).unwrap();

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "local config path must be a regular file",
        ));

    assert!(outside_team.is_symlink());
    assert_eq!(std::fs::read(&manifest_path).unwrap(), manifest_before);
}

#[cfg(unix)]
#[test]
fn uninstall_rejects_forged_external_vault_team_exception_without_local_authorization() {
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

    let external_vault = temp.path().join("outside-vault");
    std::fs::create_dir_all(&external_vault).unwrap();
    let manifest = add_hostile_manifest_symlink(&root, &external_vault.join("Team"));
    let manifest_path = root.join(".config/agent-home/manifest.json");
    let mut value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    value["vault"] = serde_json::Value::String(external_vault.to_string_lossy().into_owned());
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let manifest = std::fs::read(&manifest_path).unwrap();
    let external_team = external_vault.join("Team");
    assert!(external_team.is_symlink());

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not authorized by local config"));

    assert!(external_team.is_symlink());
    assert_eq!(std::fs::read(&manifest_path).unwrap(), manifest);
}

#[cfg(unix)]
#[test]
fn uninstall_rejects_manifest_vault_different_from_local_config() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    let configured = temp.path().join("configured-vault");
    let forged = temp.path().join("forged-vault");
    std::fs::create_dir_all(&configured).unwrap();
    std::fs::create_dir_all(&forged).unwrap();
    let config_path = root.join(".config/agent-home/config.toml");
    std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    std::fs::write(
        &config_path,
        format!("schema = 1\nvault = \"{}\"\n", configured.display()),
    )
    .unwrap();
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .assert()
        .success();

    let manifest_path = root.join(".config/agent-home/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    let shared_home = manifest["shared_home"].as_str().unwrap().to_owned();
    let target = forged.join("Team");
    std::os::unix::fs::symlink(&shared_home, &target).unwrap();
    manifest["vault"] = serde_json::Value::String(forged.to_string_lossy().into_owned());
    manifest["entries"][target.to_string_lossy().as_ref()] = serde_json::json!({
        "kind": "symlink", "target": target, "source": shared_home, "backup": null,
        "original_sha256": null, "installed_sha256": null
    });
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let before = std::fs::read(&manifest_path).unwrap();

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not match local config"));

    assert!(target.is_symlink());
    assert_eq!(std::fs::read(&manifest_path).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn install_rejects_external_manifest_symlink_before_mutation() {
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
    let external = temp.path().join("outside/managed-link");
    let manifest = add_hostile_manifest_symlink(&root, &external);

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("target escapes"));

    assert!(external.is_symlink());
    assert!(root.join("Team").is_symlink());
    assert_eq!(
        std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap(),
        manifest
    );
}

#[cfg(unix)]
#[test]
fn install_rejects_traversal_manifest_target_before_mutation() {
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
    let external = temp.path().join("outside/managed-link");
    let manifest_path = root.join(".config/agent-home/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    let shared = manifest["shared_home"].as_str().unwrap().to_owned();
    std::fs::create_dir_all(external.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&shared, &external).unwrap();
    let traversal = root.join("../outside/managed-link");
    manifest["entries"][traversal.to_string_lossy().as_ref()] = serde_json::json!({
        "kind": "symlink", "target": traversal, "source": shared, "backup": null,
        "original_sha256": null, "installed_sha256": null
    });
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let bytes = std::fs::read(&manifest_path).unwrap();

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("target escapes"));

    assert!(external.is_symlink());
    assert!(root.join("Team").is_symlink());
    assert_eq!(std::fs::read(&manifest_path).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn prune_rejects_external_manifest_symlink_before_removal() {
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
    let external = temp.path().join("outside/managed-link");
    let manifest = add_hostile_manifest_symlink(&root, &external);

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("target escapes"));

    assert!(external.is_symlink());
    assert!(root.join("Team").is_symlink());
    assert_eq!(
        std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap(),
        manifest
    );
}

#[cfg(unix)]
#[test]
fn uninstall_rejects_external_manifest_symlink_before_removal() {
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
    let external = temp.path().join("outside/managed-link");
    let manifest = add_hostile_manifest_symlink(&root, &external);

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("target escapes"));

    assert!(external.is_symlink());
    assert!(root.join("Team").is_symlink());
    assert_eq!(
        std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap(),
        manifest
    );
}

#[cfg(unix)]
#[test]
fn uninstall_rejects_traversal_manifest_target_before_removal() {
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
    let external = temp.path().join("outside/managed-link");
    let manifest_path = root.join(".config/agent-home/manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    let shared = manifest["shared_home"].as_str().unwrap().to_owned();
    std::fs::create_dir_all(external.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&shared, &external).unwrap();
    let traversal = root.join("../outside/managed-link");
    manifest["entries"][traversal.to_string_lossy().as_ref()] = serde_json::json!({
        "kind": "symlink", "target": traversal, "source": shared, "backup": null,
        "original_sha256": null, "installed_sha256": null
    });
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let bytes = std::fs::read(&manifest_path).unwrap();

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("target escapes"));

    assert!(external.is_symlink());
    assert!(root.join("Team").is_symlink());
    assert_eq!(std::fs::read(&manifest_path).unwrap(), bytes);
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

/// A PATH containing only executable stubs for the named tools (plus optional `node` output).
fn stub_path(tools: &[&str], node_version: Option<&str>) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, body: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    for tool in tools {
        write(tool, "exit 0");
    }
    if let Some(version) = node_version {
        write("node", &format!("echo {version} >&2"));
    }
    // A non-executable file must not count as an installed tool.
    std::fs::write(dir.path().join("az"), "not executable").unwrap();
    dir
}

fn doctor_json_with_path(path: &std::path::Path) -> (bool, serde_json::Value) {
    let output = Command::cargo_bin("vesica")
        .unwrap()
        .args(["doctor", "--json", "--home-repo"])
        .arg(fixture_home())
        .env("PATH", path)
        .output()
        .unwrap();
    let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "doctor did not print JSON ({error}): {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), report)
}

#[test]
fn doctor_reports_linux_platform_and_required_prerequisites() {
    let path = stub_path(&["git", "gh", "curl", "sha256sum"], None);
    let (ok, report) = doctor_json_with_path(path.path());
    assert!(ok, "all required tools present: {report:#}");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["platform"]["os"], "linux");
    assert_eq!(report["platform"]["arch"], "x86_64");
    assert!(report["platform"]["kernel"].as_str().is_some());
    for tool in ["git", "gh", "curl", "sha256sum"] {
        assert_eq!(report["prerequisites"]["required"][tool], "found", "{tool}");
    }
    assert_eq!(report["prerequisites"]["optional"]["az"], "missing");
    assert_eq!(report["prerequisites"]["optional"]["node"], "missing");
    assert_eq!(report["blockers"], serde_json::json!([]));
}

#[test]
fn doctor_fails_closed_when_a_required_prerequisite_is_missing() {
    let path = stub_path(&["git", "curl", "sha256sum"], None);
    let (ok, report) = doctor_json_with_path(path.path());
    assert!(!ok, "missing gh must fail doctor");
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["prerequisites"]["required"]["gh"], "missing");
    assert_eq!(
        report["blockers"],
        serde_json::json!(["required command not found: gh"])
    );

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["doctor", "--home-repo"])
        .arg(fixture_home())
        .env("PATH", path.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "Blocker: required command not found: gh",
        ));
}

#[test]
fn doctor_reports_node_below_the_team_minimum_without_blocking() {
    let path = stub_path(&["git", "gh", "curl", "sha256sum"], Some("v18.19.0"));
    let (ok, report) = doctor_json_with_path(path.path());
    assert!(ok, "node is optional: {report:#}");
    assert_eq!(
        report["prerequisites"]["optional"]["node"],
        "too old: 18.19.0 < 22.0.0"
    );

    let path = stub_path(&["git", "gh", "curl", "sha256sum"], Some("v22.15.0"));
    let (_, report) = doctor_json_with_path(path.path());
    assert_eq!(
        report["prerequisites"]["optional"]["node"],
        "found: 22.15.0"
    );
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
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

/// A shared home whose single repo `sample-app` points at `url`, plus a bare remote with one commit.
struct RepoFixture {
    temp: tempfile::TempDir,
    shared: PathBuf,
    remote: PathBuf,
    repos: PathBuf,
}
impl RepoFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let remote = temp.path().join("sample-app.git");
        let seed = temp.path().join("seed");
        run_git(temp.path(), ["init", "--bare", remote.to_str().unwrap()]);
        run_git(temp.path(), ["init", "-b", "main", seed.to_str().unwrap()]);
        run_git(&seed, ["config", "user.name", "Vesica Test"]);
        run_git(&seed, ["config", "user.email", "vesica@example.invalid"]);
        std::fs::write(seed.join("README.md"), "sample app\n").unwrap();
        run_git(&seed, ["add", "README.md"]);
        run_git(&seed, ["commit", "-m", "initial"]);
        run_git(&seed, ["push", remote.to_str().unwrap(), "main"]);
        run_git(&remote, ["symbolic-ref", "HEAD", "refs/heads/main"]);
        let fixture = Self {
            shared: temp.path().join("shared"),
            repos: temp.path().join("repos"),
            remote,
            temp,
        };
        fixture.write_config(fixture.remote.to_str().unwrap());
        fixture
    }
    fn write_config(&self, url: &str) {
        copy_tree(&fixture_home(), &self.shared);
        let toml = std::fs::read_to_string(fixture_home().join("agent-home.toml")).unwrap();
        let head = toml.split("[[repos]]").next().unwrap();
        std::fs::write(
            self.shared.join("agent-home.toml"),
            format!("{head}[[repos]]\nname = \"sample-app\"\nurl = \"{url}\"\nprofiles = [\"developer\", \"lead\"]\n"),
        )
        .unwrap();
    }
    fn install(&self, extra: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("vesica")
            .unwrap()
            .args(["install", "--home-repo"])
            .arg(&self.shared)
            .arg("--root")
            .arg(self.temp.path().join("home"))
            .arg("--repos-dir")
            .arg(&self.repos)
            .args(extra)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .assert()
    }
}

#[test]
fn install_clones_missing_profile_repositories_and_is_idempotent() {
    let fixture = RepoFixture::new();
    fixture
        .install(&["--profile", "developer"])
        .success()
        .stdout(predicate::str::contains("Cloned: sample-app"));
    let checkout = fixture.repos.join("sample-app");
    assert_eq!(
        std::fs::read_to_string(checkout.join("README.md")).unwrap(),
        "sample app\n"
    );
    let origin = std::process::Command::new("git")
        .arg("-C")
        .arg(&checkout)
        .args(["remote", "get-url", "origin"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&origin.stdout).trim(),
        fixture.remote.to_str().unwrap()
    );
    let entries: Vec<_> = std::fs::read_dir(&fixture.repos)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        entries,
        ["sample-app"],
        "no temporary clone directories left"
    );

    fixture
        .install(&["--profile", "developer"])
        .success()
        .stdout(predicate::str::contains("Present: sample-app"));
}

#[test]
fn install_dry_run_lists_planned_clones_without_cloning() {
    let fixture = RepoFixture::new();
    fixture
        .install(&["--profile", "lead", "--dry-run"])
        .success()
        .stdout(predicate::str::contains("Would clone: sample-app"));
    assert!(!fixture.repos.exists());
}

#[test]
fn install_core_profile_does_not_clone_repositories() {
    let fixture = RepoFixture::new();
    fixture
        .install(&["--profile", "core"])
        .success()
        .stdout(predicate::str::contains(
            "Repository cloning skipped: the core profile does not clone repositories",
        ));
    assert!(!fixture.repos.exists());
}

#[test]
fn install_leaves_an_existing_repository_destination_untouched() {
    let fixture = RepoFixture::new();
    let destination = fixture.repos.join("sample-app");
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(destination.join("mine.txt"), "user work\n").unwrap();
    fixture
        .install(&["--profile", "developer"])
        .success()
        .stdout(predicate::str::contains("Skipped: sample-app"));
    assert_eq!(
        std::fs::read_to_string(destination.join("mine.txt")).unwrap(),
        "user work\n"
    );
    assert!(!destination.join(".git").exists());
}

#[test]
fn install_reports_a_failed_clone_without_leaving_a_partial_checkout() {
    let fixture = RepoFixture::new();
    let missing = fixture.temp.path().join("no-such-remote.git");
    fixture.write_config(missing.to_str().unwrap());
    fixture
        .install(&["--profile", "developer"])
        .failure()
        .stderr(predicate::str::contains("failed to clone sample-app"));
    assert!(!fixture.repos.join("sample-app").exists());
    let leftovers: Vec<_> = std::fs::read_dir(&fixture.repos)
        .map(|it| it.map(|e| e.unwrap().file_name()).collect())
        .unwrap_or_default();
    assert!(
        leftovers.is_empty(),
        "temporary clone left behind: {leftovers:?}"
    );
    assert!(
        fixture
            .temp
            .path()
            .join("home/.config/agent-home/manifest.json")
            .is_file(),
        "managed configuration is installed even when a clone fails"
    );
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
fn install_refuses_a_symlinked_local_config_before_mutating_managed_paths() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("home");
        let config_dir = root.join(".config/agent-home");
        std::fs::create_dir_all(&config_dir).unwrap();
        let outside = temp.path().join("outside-config.toml");
        std::fs::write(&outside, b"schema = 1\nprofile = \"core\"\n").unwrap();
        let local = config_dir.join("config.toml");
        symlink(&outside, &local).unwrap();

        Command::cargo_bin("vesica")
            .unwrap()
            .args(["install", "--home-repo"])
            .arg(fixture_home())
            .args(["--root"])
            .arg(&root)
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "local config path must be a regular file",
            ));

        assert!(local.is_symlink());
        assert_eq!(
            std::fs::read(&outside).unwrap(),
            b"schema = 1\nprofile = \"core\"\n"
        );
        assert!(!root.join("Team").exists());
        assert!(!root.join(".config/agent-home/manifest.json").exists());
    }
}

#[test]
fn failed_install_manifest_commit_restores_existing_local_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    let config = root.join(".config/agent-home");
    std::fs::create_dir_all(config.join("manifest.json.tmp")).unwrap();
    let local_path = config.join("config.toml");
    let original = b"schema = 1\nprofile = \"core\"\n# machine-local comment\n";
    std::fs::write(&local_path, original).unwrap();

    let result = Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--profile", "lead", "--root"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert_eq!(std::fs::read(&local_path).unwrap(), original);
    assert!(!config.join("manifest.json").exists());
    assert!(config.join("manifest.json.tmp").is_dir());
    assert!(!root.join("Team").exists());
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
fn uninstall_with_retained_entries_commits_progress_before_removing_files() {
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

    let claude = root.join(".claude/CLAUDE.md");
    std::fs::write(&claude, "user replacement\n").unwrap();
    let manifest_path = root.join(".config/agent-home/manifest.json");
    let obstruction = root.join(".config/agent-home/manifest.json.tmp");
    std::fs::create_dir(&obstruction).unwrap();
    let managed_link = root.join("Team");

    let result = Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!result.status.success());
    let diagnostic = String::from_utf8_lossy(&result.stdout).to_string()
        + &String::from_utf8_lossy(&result.stderr);
    assert!(diagnostic.contains("manifest temp file"), "{diagnostic}");
    assert_eq!(std::fs::read(&claude).unwrap(), b"user replacement\n");
    assert!(
        managed_link.is_symlink(),
        "save failure must occur before unlink"
    );
    assert!(manifest_path.is_file());
    assert!(obstruction.is_dir());
}

#[test]
fn uninstall_keeps_symlink_replacements_and_referents_untouched() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

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
        let target = root.join(".claude/CLAUDE.md");
        let outside = temp.path().join("outside.txt");
        std::fs::write(&outside, std::fs::read(&target).unwrap()).unwrap();
        std::fs::remove_file(&target).unwrap();
        symlink(&outside, &target).unwrap();

        Command::cargo_bin("vesica")
            .unwrap()
            .args(["uninstall", "--root"])
            .arg(&root)
            .assert()
            .success()
            .stdout(predicate::str::contains("Skipped replaced managed target"));

        assert!(target.is_symlink());
        assert_eq!(
            std::fs::read(&outside).unwrap(),
            std::fs::read(&target).unwrap()
        );
        assert!(root.join(".config/agent-home/manifest.json").is_file());
    }
}

#[test]
fn uninstall_treats_missing_managed_targets_as_already_removed() {
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

    std::fs::remove_file(root.join(".config/opencode/opencode.json")).unwrap();
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipped missing managed target"));

    assert!(!root.join("Team").exists());
    assert!(!root.join(".agents/skills/demo-skill").exists());
    assert!(!root.join(".config/agent-home/manifest.json").exists());
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
        .stdout(predicate::str::contains("Recorded entries: 12"));

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
fn install_backups_are_private_under_permissive_umask() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("home");
        let target = root.join(".claude/CLAUDE.md");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "synthetic private config\n").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        Command::cargo_bin("vesica")
            .unwrap()
            .args(["install", "--home-repo"])
            .arg(fixture_home())
            .args(["--root"])
            .arg(&root)
            .env("TMPDIR", "/mnt/wsl-home")
            .assert()
            .success();
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap(),
        )
        .unwrap();
        let backup = std::path::PathBuf::from(
            manifest["entries"][target.to_string_lossy().as_ref()]["backup"]
                .as_str()
                .unwrap(),
        );
        assert_eq!(
            std::fs::metadata(backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn repeat_install_does_not_create_orphan_backups() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    let claude = root.join(".claude/CLAUDE.md");
    std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
    std::fs::write(&claude, "# existing user rules\n").unwrap();
    let backups = |root: &std::path::Path| -> Vec<String> {
        let mut found = Vec::new();
        for dir in [".claude", ".codex"] {
            if let Ok(entries) = std::fs::read_dir(root.join(dir)) {
                for entry in entries {
                    let name = entry.unwrap().file_name().to_string_lossy().into_owned();
                    if name.contains(".vesica-backup-") {
                        found.push(format!("{dir}/{name}"));
                    }
                }
            }
        }
        found.sort();
        found
    };
    let install = || {
        Command::cargo_bin("vesica")
            .unwrap()
            .args(["install", "--home-repo"])
            .arg(fixture_home())
            .arg("--root")
            .arg(&root)
            .assert()
            .success();
    };

    install();
    let first = backups(&root);
    assert_eq!(
        first.len(),
        1,
        "only the pre-existing Claude file is backed up: {first:?}"
    );
    install();
    install();
    assert_eq!(
        backups(&root),
        first,
        "repeat installs must not add backups"
    );

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .success();
    assert_eq!(std::fs::read(&claude).unwrap(), b"# existing user rules\n");
    assert!(!root.join(".codex/AGENTS.md").exists());
    // Uninstall conservatively keeps the recorded backup; there must be no unrecorded extras.
    assert_eq!(backups(&root), first, "no orphan backups after uninstall");
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
fn uninstall_restores_awkward_user_files_byte_for_byte() {
    // B2 acceptance: uninstall restores files byte-identical to the originals, including CRLF
    // line endings, a missing final newline, tabs, trailing spaces and non-ASCII text.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    let originals: [(&str, &[u8]); 2] = [
        (
            ".claude/CLAUDE.md",
            b"# R\xc3\xa8gles\r\n\tindented  \r\n\r\nlast line, no newline",
        ),
        (
            ".codex/AGENTS.md",
            b"line one   \nline two\twith tab\n\n\n\xe2\x9c\x93 done",
        ),
    ];
    for (path, bytes) in originals {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .arg("--root")
        .arg(&root)
        .assert()
        .success();
    for (path, bytes) in originals {
        assert_ne!(
            std::fs::read(root.join(path)).unwrap(),
            bytes,
            "{path} was wired"
        );
    }
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .success();
    for (path, bytes) in originals {
        assert_eq!(
            std::fs::read(root.join(path)).unwrap(),
            bytes,
            "{path} must be restored byte-for-byte"
        );
    }
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
fn uninstall_manifest_temp_obstruction_preserves_all_managed_paths() {
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

    let claude = root.join(".claude/CLAUDE.md");
    std::fs::write(&claude, "user-authored replacement\n").unwrap();
    let config_before = std::fs::read(&claude).unwrap();
    let managed_links: Vec<_> = [
        ".agents/skills/demo-skill",
        ".agents/skills/second-skill",
        ".claude/skills/demo-skill",
        ".claude/skills/second-skill",
        "Team",
    ]
    .iter()
    .map(|relative| root.join(relative))
    .collect();
    let manifest_path = root.join(".config/agent-home/manifest.json");
    let manifest_before = std::fs::read(&manifest_path).unwrap();
    let obstruction = root.join(".config/agent-home/manifest.json.tmp");
    std::fs::create_dir(&obstruction).unwrap();

    let result = Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(!result.status.success());
    let diagnostic = String::from_utf8_lossy(&result.stdout).to_string()
        + &String::from_utf8_lossy(&result.stderr);
    assert!(diagnostic.contains("manifest temp file"));
    assert_eq!(std::fs::read(&claude).unwrap(), config_before);
    assert_eq!(std::fs::read(&manifest_path).unwrap(), manifest_before);
    assert!(obstruction.is_dir());
    for path in managed_links {
        let metadata = std::fs::symlink_metadata(&path).unwrap_or_else(|error| {
            panic!(
                "managed link should remain after failed uninstall: {} ({error})",
                path.display()
            )
        });
        assert!(
            metadata.file_type().is_symlink(),
            "managed link should remain after failed uninstall: {}",
            path.display()
        );
    }
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

/// Source `<root>/.bashrc` in a clean, non-interactive bash and return the Vesica variables
/// (`EX` is the fixture's `team.env_prefix`): HOME, PROFILE, REPOS, VAULT.
fn sourced_env(root: &std::path::Path) -> Vec<String> {
    let out = std::process::Command::new("bash")
        .args(["--noprofile", "--norc", "-c"])
        .arg(r#". "$1/.bashrc" && printf '%s\n' "${EX_HOME-unset}" "${EX_PROFILE-unset}" "${EX_REPOS-unset}" "${EX_VAULT-unset}""#)
        .arg("bash")
        .arg(root)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn vesica_install(root: &std::path::Path, extra: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .arg("--root")
        .arg(root)
        .args(extra)
        .assert()
}

#[test]
fn install_writes_config_env_sourced_from_bashrc_and_uninstall_restores_it() {
    // Install step 2 (plan §5.4): config.env with <PREFIX>_HOME and <PREFIX>_PROFILE, plus a
    // ~/.bashrc managed block that sources it. The root contains a space and a single quote to
    // exercise shell quoting end to end.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("it's a home");
    let bashrc: &[u8] = b"# user bashrc\nalias ll='ls -l'\nexport KEEP=1";
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(".bashrc"), bashrc).unwrap();
    let config_env = root.join(".config/agent-home/config.env");

    vesica_install(&root, &["--profile", "lead"])
        .success()
        .stdout(predicate::str::contains(config_env.to_str().unwrap()))
        .stdout(predicate::str::contains(
            root.join(".bashrc").to_str().unwrap(),
        ));
    let shared = fixture_home().canonicalize().unwrap();
    assert_eq!(
        sourced_env(&root),
        [shared.to_str().unwrap(), "lead", "unset", "unset"]
    );
    let written = std::fs::read_to_string(&config_env).unwrap();
    assert!(written.starts_with("# >>> vesica >>>\n"), "{written}");

    // Reinstalling changes nothing.
    let before = (
        std::fs::read(&config_env).unwrap(),
        std::fs::read(root.join(".bashrc")).unwrap(),
    );
    vesica_install(&root, &["--profile", "lead"]).success();
    assert_eq!(
        before,
        (
            std::fs::read(&config_env).unwrap(),
            std::fs::read(root.join(".bashrc")).unwrap()
        )
    );

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .success();
    assert_eq!(std::fs::read(root.join(".bashrc")).unwrap(), bashrc);
    assert!(!config_env.exists(), "config.env must be removed");
}

#[test]
fn install_creates_bashrc_when_absent_and_uninstall_removes_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    vesica_install(&root, &["--profile", "core"]).success();
    assert_eq!(sourced_env(&root)[1..3], ["core", "unset"]);
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .success();
    assert!(!root.join(".bashrc").exists());
    assert!(!root.join(".config/agent-home/config.env").exists());
}

#[test]
fn install_exports_an_absolute_repos_dir_for_repository_profiles_only() {
    let fixture = RepoFixture::new();
    let home = fixture.temp.path().join("home");
    // A relative --repos-dir is exported as an absolute path.
    Command::cargo_bin("vesica")
        .unwrap()
        .current_dir(fixture.temp.path())
        .args(["install", "--home-repo"])
        .arg(&fixture.shared)
        .arg("--root")
        .arg(&home)
        .args(["--repos-dir", "repos", "--profile", "developer"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .assert()
        .success();
    let env = sourced_env(&home);
    assert_eq!(env[1], "developer");
    assert_eq!(
        std::fs::canonicalize(&env[2]).unwrap(),
        fixture.repos.canonicalize().unwrap()
    );
    // The core profile never clones, so it gets no repos variable; the block is updated in place.
    fixture.install(&["--profile", "core"]).success();
    assert_eq!(sourced_env(&home)[1..3], ["core", "unset"]);
}

#[test]
fn install_dry_run_plans_shell_environment_without_writing_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    vesica_install(&root, &["--dry-run"])
        .success()
        .stdout(predicate::str::contains("config.env"))
        .stdout(predicate::str::contains(".bashrc"));
    assert!(!root.join(".bashrc").exists());
    assert!(!root.join(".config/agent-home/config.env").exists());
}

#[test]
fn install_refuses_values_that_cannot_be_written_to_config_env() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("line\nbreak");
    vesica_install(&root, &[])
        .failure()
        .stderr(predicate::str::contains("cannot be written to config.env"));
    assert!(!root.exists(), "nothing may be created before the refusal");
}
