//! Agent wiring for the remaining §5.2 rows: OpenCode `instructions`, the Copilot CLI env var, and
//! the Hermes command plan (plan F3.4 / B6).
//!
//! Role rendering is deliberately absent: the plan says it uses "the S3 field map", and S3 (role
//! discovery per agent) has to produce that map first. The three items here are deterministic file
//! and environment changes specified in §5.2, so they can be built and asserted exactly.

use assert_cmd::Command;
use std::path::{Path, PathBuf};

fn fixture_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home")
}

fn install(home: &Path, vault: Option<&Path>, profile: &str) -> assert_cmd::assert::Assert {
    let mut command = Command::cargo_bin("vesica").unwrap();
    command
        .args(["install", "--home-repo"])
        .arg(fixture_home())
        .arg("--root")
        .arg(home)
        .args(["--profile", profile]);
    if let Some(vault) = vault {
        command.arg("--vault").arg(vault);
    }
    command.assert()
}

fn opencode_config(home: &Path) -> PathBuf {
    home.join(".config/opencode/opencode.json")
}

fn sourced(home: &Path, variable: &str) -> String {
    let out = std::process::Command::new("bash")
        .args(["--noprofile", "--norc", "-c"])
        .arg(format!(
            ". \"$1/.bashrc\" && printf '%s' \"${{{variable}-unset}}\""
        ))
        .arg("bash")
        .arg(home)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn opencode_instructions_merge_keeps_a_users_file_and_their_entries() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let vault = temp.path().join("vault");
    let config = opencode_config(&home);
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(
        &config,
        "{\n  // my OpenCode settings\n  \"theme\": \"tokyonight\",\n  \"instructions\": [\"~/old.md\"],\n}\n",
    )
    .unwrap();

    install(&home, Some(&vault), "developer").success();

    let merged = std::fs::read_to_string(&config).unwrap();
    assert!(merged.contains("// my OpenCode settings"), "{merged}");
    assert!(merged.contains("\"tokyonight\""), "{merged}");
    let json: serde_json::Value =
        jsonc_parser::parse_to_serde_value(&merged, &jsonc_parser::ParseOptions::default())
            .unwrap();
    let shared = fixture_home().canonicalize().unwrap();
    let instructions: Vec<&str> = json["instructions"]
        .as_array()
        .expect("instructions must be a list")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        instructions.contains(&"~/old.md"),
        "the user's own entry must survive: {instructions:?}"
    );
    assert!(
        instructions.contains(&format!("{}/AGENTS.md", shared.display()).as_str()),
        "{instructions:?}"
    );
    assert!(
        instructions.contains(&format!("{}/AGENTS.md", vault.display()).as_str()),
        "{instructions:?}"
    );

    // Idempotent: a second install must not duplicate or reorder anything.
    install(&home, Some(&vault), "developer").success();
    assert_eq!(std::fs::read_to_string(&config).unwrap(), merged);
}

#[test]
fn opencode_instructions_are_created_when_absent_and_cover_the_shared_home() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    install(&home, None, "core").success();
    let text = std::fs::read_to_string(opencode_config(&home)).unwrap();
    let json: serde_json::Value =
        jsonc_parser::parse_to_serde_value(&text, &jsonc_parser::ParseOptions::default()).unwrap();
    assert_eq!(
        json["instructions"].as_array().unwrap().len(),
        1,
        "without --vault only the shared home is referenced: {text}"
    );
    assert!(text.contains(&format!(
        "{}/AGENTS.md",
        fixture_home().canonicalize().unwrap().display()
    )));
}

#[test]
fn copilot_instruction_dirs_are_exported_in_the_shell_environment() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let vault = temp.path().join("vault");
    install(&home, Some(&vault), "lead").success();
    let shared = fixture_home().canonicalize().unwrap();
    let expected = format!("{},{}", shared.display(), vault.display());
    assert_eq!(sourced(&home, "COPILOT_CUSTOM_INSTRUCTIONS_DIRS"), expected);
    // The name is fixed by Copilot, so it must not take the team's env prefix.
    let config = std::fs::read_to_string(home.join(".config/agent-home/config.env")).unwrap();
    assert!(
        config.contains(&format!(
            "export COPILOT_CUSTOM_INSTRUCTIONS_DIRS='{expected}'"
        )),
        "{config}"
    );
}

#[test]
fn hermes_plan_is_printed_for_lead_and_touches_no_hermes_config() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let vault = temp.path().join("vault");
    let shared = fixture_home().canonicalize().unwrap();

    let out = install(&home, Some(&vault), "lead")
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    // The fixture sets [hermes] profile = "team".
    assert!(
        stdout.contains("hermes --profile team config set skills.external_dirs"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("{}/Agents/Skills", shared.display())),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("{}/Agents/Hermes/Skills", shared.display())),
        "{stdout}"
    );
    assert!(
        stdout.contains("hermes --profile team config set skills.create_dir"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&vault.display().to_string()),
        "the vault must appear as the working directory: {stdout}"
    );
    // Nothing under ~/.hermes is written: this is a plan, not an edit.
    assert!(
        !home.join(".hermes").exists(),
        "install must not create Hermes configuration"
    );

    // Only the lead profile gets it.
    let other = temp.path().join("home2");
    let out = install(&other, None, "developer")
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(!stdout.contains("hermes --profile"), "{stdout}");
}

/// `uninstall --root <home>`.
fn uninstall(home: &Path) -> assert_cmd::assert::Assert {
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(home)
        .assert()
}

#[test]
fn uninstall_restores_a_preexisting_opencode_config_byte_for_byte() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let vault = temp.path().join("vault");
    let config = opencode_config(&home);
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    // Awkward but valid JSONC: comments, a trailing comma and no final newline.
    let original: &[u8] =
        b"{\n  // keep\n  \"theme\": \"tokyonight\",\n  \"instructions\": [\"~/old.md\"],\n}";
    std::fs::write(&config, original).unwrap();

    install(&home, Some(&vault), "developer").success();
    assert_ne!(std::fs::read(&config).unwrap(), original, "install merged");
    uninstall(&home).success();
    assert_eq!(
        std::fs::read(&config).unwrap(),
        original,
        "uninstall must restore the user's file byte-for-byte"
    );
}

#[test]
fn uninstall_removes_an_opencode_config_vesica_created_and_keeps_a_modified_one() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    install(&home, None, "core").success();
    assert!(opencode_config(&home).is_file());
    uninstall(&home).success();
    assert!(
        !opencode_config(&home).exists(),
        "a file Vesica created is removed"
    );

    // A merged file the user edited afterwards is kept, and so is its manifest entry.
    let other = temp.path().join("home2");
    install(&other, None, "core").success();
    let config = opencode_config(&other);
    std::fs::write(&config, "{\n  \"instructions\": [\"mine\"]\n}\n").unwrap();
    let out = uninstall(&other).success().get_output().clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Skipped modified merged file"), "{stdout}");
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "{\n  \"instructions\": [\"mine\"]\n}\n",
        "the user's edit must survive"
    );
}
