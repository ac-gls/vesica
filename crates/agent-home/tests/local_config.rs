//! Plan F8: the machine-local configuration file (`<root>/.config/agent-home/config.toml`).
//!
//! Every command below runs against a disposable root and removes both home-repo environment
//! variables, so nothing here can read the machine's real configuration.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

fn fixture_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home")
}

/// A disposable machine root, optionally carrying a local configuration file. Returns the temp
/// directory (keep it alive) and the root to pass as `--root` / `$HOME`.
fn machine(local_config: &str) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    let dir = root.join(".config/agent-home");
    std::fs::create_dir_all(&dir).unwrap();
    if !local_config.is_empty() {
        std::fs::write(dir.join("config.toml"), local_config).unwrap();
    }
    (temp, root)
}

/// A `vesica` command with both home-repo environment variables cleared.
fn command() -> Command {
    let mut cmd = Command::cargo_bin("vesica").unwrap();
    cmd.env_remove("VESICA_HOME_REPO")
        .env_remove("AGENT_HOME_REPO");
    cmd
}

fn local_config_with_home(home: &Path) -> String {
    format!("schema = 1\nhome_repo = \"{}\"\n", home.display())
}

#[test]
fn flag_beats_environment_and_environment_beats_the_local_file() {
    // The local file points somewhere that does not exist: any fall-through to it fails loudly,
    // which is what makes the two cases below meaningful.
    let (_temp, root) = machine("schema = 1\nhome_repo = \"/nonexistent/local-config-home\"\n");

    // 1. flag beats the environment
    command()
        .args(["doctor", "--home-repo"])
        .arg(fixture_home())
        .env("VESICA_HOME_REPO", "/nonexistent/environment-home")
        .env("AGENT_HOME_REPO", "/nonexistent/legacy-environment-home")
        .env("HOME", &root)
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "Shared home: {}",
            fixture_home().display()
        )))
        .stdout(predicate::str::contains("(flag/env)"));

    // 2. environment beats the local file
    command()
        .args(["doctor"])
        .env("VESICA_HOME_REPO", fixture_home())
        .env("HOME", &root)
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "Shared home: {}",
            fixture_home().display()
        )))
        .stdout(predicate::str::contains("(flag/env)"));
}

#[test]
fn the_local_file_supplies_the_home_and_no_flag_is_needed() {
    let (_temp, root) = machine(&local_config_with_home(&fixture_home()));
    command()
        .args(["doctor"])
        .env("HOME", &root)
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "Shared home: {}",
            fixture_home().display()
        )))
        .stdout(predicate::str::contains("(config.toml)"));
}

#[test]
fn the_team_file_supplies_the_profile_when_nothing_else_does() {
    // The local file sets the home only, so the profile falls through to the team file's default.
    let (_temp, root) = machine(&local_config_with_home(&fixture_home()));
    command()
        .args(["doctor"])
        .env("HOME", &root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Profile: developer (team)"));
}

#[test]
fn install_persists_the_resolved_values_and_a_later_command_needs_no_flag() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("home");
    std::fs::create_dir_all(&root).unwrap();

    command()
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .args(["--profile", "core", "--root"])
        .arg(&root)
        .assert()
        .success();

    let written = std::fs::read_to_string(root.join(".config/agent-home/config.toml"))
        .expect("install must write the local configuration");
    assert!(
        written.contains("home_repo"),
        "local config must record the home: {written}"
    );
    assert!(
        written.contains("profile = \"core\""),
        "local config must record the chosen profile: {written}"
    );

    // A later command in that machine resolves both from the file.
    command()
        .args(["doctor"])
        .env("HOME", &root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Profile: core (config.toml)"));
}

#[test]
fn local_repositories_merge_with_the_team_list_by_name() {
    let (_temp, root) = machine(&format!(
        "{}[[repos]]\nname = \"sample-app\"\nurl = \"https://github.com/example-org/sample-app-fork.git\"\nprofiles = [\"developer\", \"lead\"]\n[[repos]]\nname = \"local-only\"\nurl = \"https://github.com/example-org/local-only.git\"\nprofiles = [\"developer\"]\n",
        local_config_with_home(&fixture_home())
    ));

    let output = command()
        .args(["doctor", "--json"])
        .env("HOME", &root)
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    // Two entries, not three: the local entry replaced the team entry of the same name, and the
    // team's order survives.
    assert_eq!(report["repos_in_config"], 2);
    assert_eq!(
        report["repos"],
        serde_json::json!(["sample-app", "local-only"])
    );
}

#[test]
fn a_local_file_with_an_unsupported_schema_is_rejected() {
    let (_temp, root) = machine(&format!(
        "schema = 2\nhome_repo = \"{}\"\n",
        fixture_home().display()
    ));
    command()
        .args(["doctor"])
        .env("HOME", &root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("schema"));
}

#[test]
fn a_local_file_may_not_hold_a_credential_like_field() {
    // The guard is the shared denylist (client_secret, api_key, access_token, refresh_token,
    // password, secret). A token-shaped key outside it, such as `github_token`, is not detected:
    // unknown keys are dropped by the strict schema, so nothing is stored or used, but the file
    // is not *rejected* either. See the F8 report.
    let (_temp, root) = machine(&format!(
        "{}client_secret = \"not-a-real-secret\"\n",
        local_config_with_home(&fixture_home())
    ));
    command()
        .args(["doctor"])
        .env("HOME", &root)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "credential-like field client_secret",
        ))
        .stderr(predicate::str::contains("config.toml"));
}

#[test]
fn a_local_file_may_not_carry_unsafe_repository_entries() {
    let (_temp, root) = machine(&format!(
        "{}[[repos]]\nname = \"../escape\"\nurl = \"https://github.com/example-org/x.git\"\n",
        local_config_with_home(&fixture_home())
    ));
    command()
        .args(["doctor"])
        .env("HOME", &root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("name"));

    let (_temp2, root2) = machine(&format!(
        "{}[[repos]]\nname = \"ok\"\nurl = \"ext::sh -c touch%20/tmp/vesica-pwned\"\n",
        local_config_with_home(&fixture_home())
    ));
    command()
        .args(["doctor"])
        .env("HOME", &root2)
        .assert()
        .failure()
        .stderr(predicate::str::contains("forbidden git transport"));
    assert!(!Path::new("/tmp/vesica-pwned").exists());
}

#[test]
fn with_no_home_anywhere_the_error_names_every_way_to_supply_one() {
    let (_temp, root) = machine("");
    command()
        .args(["doctor"])
        .env("HOME", &root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--home-repo"))
        .stderr(predicate::str::contains("VESICA_HOME_REPO"))
        .stderr(predicate::str::contains("config.toml"));
}
