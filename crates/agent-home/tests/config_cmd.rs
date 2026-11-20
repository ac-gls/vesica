//! F9: `vesica config list|get` — a read-only view of what the resolver produced, naming the layer
//! behind every value. Parent options come before the subcommand, as clap requires.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

fn fixture_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home")
}

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

fn command() -> Command {
    let mut cmd = Command::cargo_bin("vesica").unwrap();
    cmd.env_remove("VESICA_HOME_REPO")
        .env_remove("AGENT_HOME_REPO")
        .env_remove("VESICA_VAULT")
        .env_remove("VESICA_REPOS_DIR");
    cmd
}

fn listing(local_config: &str) -> String {
    let (_temp, root) = machine(local_config);
    let output = command()
        .args(["config", "list"])
        .env("HOME", &root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "config list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn list_names_every_value_and_the_layer_that_supplied_it() {
    let text = listing(&format!(
        "schema = 1\nhome_repo = \"{}\"\nvault = \"/example/vault\"\n",
        fixture_home().display()
    ));
    let line = |key: &str| {
        text.lines()
            .find(|l| l.starts_with(&format!("{key} ")))
            .unwrap_or_else(|| panic!("no {key} row in:\n{text}"))
            .to_owned()
    };
    assert!(line("home_repo").contains(&fixture_home().display().to_string()));
    assert!(line("home_repo").contains("(config.toml)"));
    // The local file sets no profile, so the team file's default supplies it.
    assert!(line("profile").contains("developer"));
    assert!(line("profile").contains("(team)"));
    assert!(line("vault").contains("/example/vault"));
    assert!(line("vault").contains("(config.toml)"));
    // Nothing supplies these, and the row says so rather than blaming a layer.
    assert!(line("repos_dir").contains("(unset)"));
    assert!(line("repos_dir").contains("(default)"));
    // The repository list is the merged team list.
    assert!(line("repos").contains("sample-app"));
    assert!(line("repos").contains("(team)"));
    assert!(line("team").contains("Example Team (example-org)"));
}

#[test]
fn the_repository_row_reports_a_local_merge() {
    let text = listing(&format!(
        "schema = 1\nhome_repo = \"{}\"\n[[repos]]\nname = \"local-only\"\nurl = \"https://github.com/example-org/local-only.git\"\nprofiles = [\"developer\"]\n",
        fixture_home().display()
    ));
    let repos = text
        .lines()
        .find(|l| l.starts_with("repos "))
        .expect("a repos row");
    assert!(repos.contains("sample-app"), "{repos}");
    assert!(repos.contains("local-only"), "{repos}");
    assert!(repos.contains("team + config.toml"), "{repos}");
}

#[test]
fn a_flag_outranks_the_local_file_and_the_row_says_so() {
    let (_temp, root) = machine(&format!(
        "schema = 1\nhome_repo = \"{}\"\nprofile = \"core\"\n",
        fixture_home().display()
    ));
    command()
        .args(["config", "--profile", "lead", "get", "profile"])
        .env("HOME", &root)
        .assert()
        .success()
        .stdout(predicate::str::contains("lead (flag/env)"));
}

#[test]
fn get_prints_one_value_with_its_layer() {
    let (_temp, root) = machine(&format!(
        "schema = 1\nhome_repo = \"{}\"\n",
        fixture_home().display()
    ));
    let output = command()
        .args(["config", "get", "home_repo"])
        .env("HOME", &root)
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert_eq!(text.lines().count(), 1, "get prints one row: {text}");
    assert!(text.contains(&fixture_home().display().to_string()));
    assert!(text.contains("(config.toml)"));
}

#[test]
fn get_rejects_an_unknown_key_and_lists_the_known_ones() {
    let (_temp, root) = machine(&format!(
        "schema = 1\nhome_repo = \"{}\"\n",
        fixture_home().display()
    ));
    command()
        .args(["config", "get", "nonsense"])
        .env("HOME", &root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown configuration key"))
        .stderr(predicate::str::contains("home_repo"))
        .stderr(predicate::str::contains("repos_dir"));
}

#[test]
fn json_carries_a_value_and_a_source_for_every_key() {
    let (_temp, root) = machine(&format!(
        "schema = 1\nhome_repo = \"{}\"\n",
        fixture_home().display()
    ));
    let output = command()
        .args(["config", "--json", "list"])
        .env("HOME", &root)
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for key in [
        "team",
        "home_repo",
        "profile",
        "vault",
        "repos_dir",
        "repos",
    ] {
        assert!(
            report[key]["value"].is_string(),
            "missing value for {key}: {report}"
        );
        assert!(
            report[key]["source"].is_string(),
            "missing source for {key}: {report}"
        );
    }
    assert_eq!(
        report["home_repo"]["value"],
        fixture_home().display().to_string()
    );
    assert_eq!(report["home_repo"]["source"], "config.toml");
    assert_eq!(report["profile"]["source"], "team");
    assert_eq!(report["repos_dir"]["source"], "default");
}

#[test]
fn config_never_writes_to_the_machine() {
    let (_temp, root) = machine(&format!(
        "schema = 1\nhome_repo = \"{}\"\n",
        fixture_home().display()
    ));
    let state_dir = root.join(".config/agent-home");
    let before: Vec<PathBuf> = std::fs::read_dir(&state_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    command()
        .args(["config", "list"])
        .env("HOME", &root)
        .assert()
        .success();
    let after: Vec<PathBuf> = std::fs::read_dir(&state_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let names = |v: &Vec<PathBuf>| {
        let mut n: Vec<String> = v
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        n.sort();
        n
    };
    assert_eq!(names(&before), names(&after), "config must be read-only");
    // The command must not fall back to writing the shared home's own state either.
    assert!(!Path::new(&format!("{}/manifest.json", state_dir.display())).exists());
}
