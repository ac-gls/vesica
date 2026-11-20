//! F11: the versioning surface — the `[home] ref` pin, the home's `[minimum] vesica`/`home`
//! compatibility declaration, `status` drift, and `home_ref` in the manifest.
//!
//! These tests use a **copy** of the fixture shared home turned into a real git repository with
//! tags, because the pin is about a checkout's ref. The fixture itself is never modified.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

/// The same version the binary was built with.
const CLI: &str = env!("CARGO_PKG_VERSION");

fn fixture_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home")
}

fn run_git(cwd: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A copy of the fixture shared home as a git checkout on branch `main`, with `tags` and an
/// optional extra `[minimum]` line, committed so the whole tree is clean.
fn tagged_home(tags: &[&str], minimum_extra: Option<&str>) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    copy_dir(&fixture_home(), &home);
    if let Some(extra) = minimum_extra {
        let path = home.join("agent-home.toml");
        let text = std::fs::read_to_string(&path).unwrap();
        let replaced = text.replace("node = \"22.0.0\"", &format!("node = \"22.0.0\"\n{extra}"));
        assert_ne!(text, replaced, "fixture must declare node");
        std::fs::write(&path, replaced).unwrap();
    }
    run_git(&home, &["init", "-q", "-b", "main"]);
    run_git(&home, &["config", "user.name", "Vesica Test"]);
    run_git(&home, &["config", "user.email", "vesica@example.invalid"]);
    run_git(&home, &["add", "-A"]);
    run_git(&home, &["commit", "-qm", "fixture"]);
    for tag in tags {
        run_git(&home, &["tag", tag]);
    }
    (temp, home)
}

/// A disposable machine root carrying a local config file.
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
    for var in [
        "VESICA_HOME_REPO",
        "AGENT_HOME_REPO",
        "VESICA_VAULT",
        "VESICA_REPOS_DIR",
        "VESICA_TEAM_HOME",
    ] {
        cmd.env_remove(var);
    }
    cmd
}

#[test]
fn a_home_that_needs_a_newer_cli_blocks_doctor_and_names_both_versions() {
    let (_temp, home) = tagged_home(&[], Some("vesica = \"99.0.0\""));
    let output = command()
        .args(["doctor", "--json", "--home-repo"])
        .arg(&home)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "an incompatible home must exit non-zero"
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "blocked");
    assert_eq!(
        report["prerequisites"]["optional"]["home version"],
        "incompatible: the home and this CLI disagree"
    );
    // The JSON contract: `prerequisites.required` values are only ever "found"/"missing"
    // (schemas/doctor.schema.json constrains that map), so an incompatibility must never
    // appear there. This is the assertion that caught the first attempt.
    for (name, state) in report["prerequisites"]["required"].as_object().unwrap() {
        assert!(
            state == "found" || state == "missing",
            "required entry {name} = {state} violates the schema"
        );
    }
    let blockers = report["blockers"].as_array().unwrap();
    assert!(
        blockers.iter().any(|b| {
            let b = b.as_str().unwrap_or_default();
            b.contains("99.0.0") && b.contains(CLI)
        }),
        "the blocker must name both versions: {blockers:?}"
    );

    // The human report says the same thing.
    command()
        .args(["doctor", "--home-repo"])
        .arg(&home)
        .assert()
        .failure()
        .stdout(predicate::str::contains("Requires Vesica: 99.0.0"))
        .stdout(predicate::str::contains(CLI));
}

#[test]
fn a_home_that_needs_an_older_or_equal_cli_does_not_block() {
    for requirement in ["0.0.1", CLI] {
        let (_temp, home) = tagged_home(&[], Some(&format!("vesica = \"{requirement}\"")));
        command()
            .args(["doctor", "--home-repo"])
            .arg(&home)
            .assert()
            .success()
            .stdout(predicate::str::contains(format!(
                "Requires Vesica: {requirement}"
            )));
    }
}

#[test]
fn a_home_that_needs_a_newer_home_version_blocks_doctor() {
    let (_temp, home) = tagged_home(&["v1.0"], Some("home = \"v2.0\""));
    command()
        .args(["doctor", "--home-repo"])
        .arg(&home)
        .assert()
        .failure()
        .stdout(predicate::str::contains("needs home version v2.0"))
        .stdout(predicate::str::contains("the checkout is v1.0"));
}

#[test]
fn an_unparseable_requirement_is_not_treated_as_a_blocker() {
    let (_temp, home) = tagged_home(&[], Some("vesica = \"latest\""));
    command()
        .args(["doctor", "--home-repo"])
        .arg(&home)
        .assert()
        .success();
}

#[test]
fn install_honours_the_pin_and_records_it_in_the_manifest() {
    let (_temp, home) = tagged_home(&["v1.0"], None);
    let (_root_temp, root) = machine(&format!(
        "schema = 1\nhome_repo = \"{}\"\n[home]\nref = \"v1.0\"\n",
        home.display()
    ));
    command()
        .args(["install", "--profile", "core", "--root"])
        .arg(&root)
        .assert()
        .success();

    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(".config/agent-home/manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["home_ref"], "v1.0");
}

#[test]
fn install_refuses_when_the_checkout_is_not_at_the_pin() {
    // The checkout is at v1.0; the machine pins v2.0. Installing must not silently use v1.0.
    let (_temp, home) = tagged_home(&["v1.0"], None);
    let (_root_temp, root) = machine(&format!(
        "schema = 1\nhome_repo = \"{}\"\n[home]\nref = \"v2.0\"\n",
        home.display()
    ));
    command()
        .args(["install", "--profile", "core", "--root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("pins v2.0"))
        .stderr(predicate::str::contains("v1.0"));
}

#[test]
fn team_pull_does_not_move_a_pinned_checkout() {
    // Pinned and at the pin: nothing to do, and the pull is not performed.
    let (_temp, home) = tagged_home(&["v1.0"], None);
    let (_root_temp, root) = machine("schema = 1\n[home]\nref = \"v1.0\"\n");
    command()
        .args(["team", "pull", "--path"])
        .arg(&home)
        .env("HOME", &root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Pinned to v1.0"));

    // Pinned to something the checkout is not on: refuse rather than drift.
    let (_root_temp2, root2) = machine("schema = 1\n[home]\nref = \"v3.0\"\n");
    command()
        .args(["team", "pull", "--path"])
        .arg(&home)
        .env("HOME", &root2)
        .assert()
        .failure()
        .stderr(predicate::str::contains("pins v3.0"))
        .stderr(predicate::str::contains("v1.0"));
}

#[test]
fn status_reports_a_pinned_ref_as_informational_drift() {
    let (_temp, home) = tagged_home(&["v1.0"], None);
    let (_root_temp, root) = machine(&format!(
        "schema = 1\nhome_repo = \"{}\"\n[home]\nref = \"v1.0\"\n",
        home.display()
    ));
    command()
        .args(["install", "--profile", "core", "--root"])
        .arg(&root)
        .assert()
        .success();

    // Two newer releases exist locally; the checkout is still at the pin.
    run_git(&home, &["tag", "v1.1"]);
    run_git(&home, &["tag", "v1.2"]);
    command()
        .args(["status", "--root"])
        .arg(&root)
        .assert()
        .success()
        .stdout(predicate::str::contains("Pinned ref: v1.0"))
        .stdout(predicate::str::contains("2 releases behind"))
        .stdout(predicate::str::contains("informational"));
}

#[test]
fn status_reports_a_floating_pin_as_never_drifting() {
    let (_temp, home) = tagged_home(&["v1.0"], None);
    let (_root_temp, root) = machine(&format!(
        "schema = 1\nhome_repo = \"{}\"\n[home]\nref = \"main\"\n",
        home.display()
    ));
    command()
        .args(["install", "--profile", "core", "--root"])
        .arg(&root)
        .assert()
        .success();
    run_git(&home, &["tag", "v9.9"]);
    command()
        .args(["status", "--root"])
        .arg(&root)
        .assert()
        .success()
        .stdout(predicate::str::contains("floating"));
}

#[test]
fn a_local_file_with_a_bad_pin_is_rejected() {
    let (_temp, root) = machine("schema = 1\nhome_repo = \"/nonexistent\"\n[home]\nref = 10\n");
    command()
        .args(["config", "list"])
        .env("HOME", &root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("local configuration"));
}
