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
    install_shared(home, vault, profile, &fixture_home())
}

fn install_shared(
    home: &Path,
    vault: Option<&Path>,
    profile: &str,
    shared: &Path,
) -> assert_cmd::assert::Assert {
    let mut command = Command::cargo_bin("vesica").unwrap();
    command
        .args(["install", "--home-repo"])
        .arg(shared)
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

    install_shared(&home, Some(&vault), "developer", &fixture_home()).success();

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
    install_shared(&home, Some(&vault), "developer", &fixture_home()).success();
    assert_eq!(std::fs::read_to_string(&config).unwrap(), merged);
}

#[test]
fn opencode_instructions_are_created_when_absent_and_cover_the_shared_home() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    install_shared(&home, None, "core", &fixture_home()).success();
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
    let out = install_shared(&other, None, "developer", &fixture_home())
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
fn opencode_role_install_renders_files_and_tracks_generated_files_for_uninstall() {
    let temp = tempfile::tempdir().unwrap();
    let shared = temp.path().join("shared");
    let roles = shared.join("Agents/Roles");
    std::fs::create_dir_all(&roles).unwrap();
    std::fs::write(shared.join("AGENTS.md"), "# Shared instructions\n").unwrap();
    std::fs::write(
        shared.join("agent-home.toml"),
        "schema = 1\n[team]\nname = \"Example\"\norg = \"Example\"\nenv_prefix = \"EX\"\nrole_prefix = \"ex-\"\n[profiles]\ndefault = \"developer\"\n",
    ).unwrap();
    std::fs::write(
        roles.join("example.md"),
        "---\nname: example\ndescription: A neutral example role.\n---\nWrite a careful summary.\n",
    )
    .unwrap();
    let home = temp.path().join("home");
    let agents = home.join(".config/opencode/agents");
    let generated = agents.join("ex-example.md");

    install_shared(&home, None, "core", &shared).success();
    let expected = "---\ndescription: A neutral example role.\nmode: subagent\n---\nWrite a careful summary.\n";
    assert_eq!(std::fs::read(&generated).unwrap(), expected.as_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&generated).unwrap().permissions().mode() & 0o777,
            0o600,
            "generated role prompts use private permissions independent of umask"
        );
    }
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(home.join(".config/agent-home/manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        manifest["entries"][generated.to_str().unwrap()]["kind"],
        "generated-file"
    );

    install_shared(&home, None, "core", &shared).success();
    assert_eq!(
        std::fs::read(&generated).unwrap(),
        expected.as_bytes(),
        "repeat install is idempotent"
    );
    uninstall(&home).success();
    assert!(
        !generated.exists(),
        "uninstall removes only the unchanged generated role"
    );
}

#[test]
fn opencode_role_install_refuses_user_collisions_and_preserves_modified_roles_on_uninstall() {
    let temp = tempfile::tempdir().unwrap();
    let shared = temp.path().join("shared");
    let roles = shared.join("Agents/Roles");
    std::fs::create_dir_all(&roles).unwrap();
    std::fs::write(shared.join("AGENTS.md"), "# Shared instructions\n").unwrap();
    std::fs::write(
        shared.join("agent-home.toml"),
        "schema = 1\n[team]\nname = \"Example\"\norg = \"Example\"\nenv_prefix = \"EX\"\nrole_prefix = \"ex-\"\n[profiles]\ndefault = \"developer\"\n",
    ).unwrap();
    std::fs::write(
        roles.join("example.md"),
        "---\nname: example\ndescription: Example.\n---\nPrompt.\n",
    )
    .unwrap();
    let home = temp.path().join("home");
    let target = home.join(".config/opencode/agents/ex-example.md");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "user-owned bytes\n").unwrap();
    let out = install_shared(&home, None, "core", &shared)
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unmanaged destination already exists"),
        "{stderr}"
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"user-owned bytes\n");
    assert!(
        !home.join(".config/agent-home/manifest.json").exists(),
        "failed install leaves no manifest"
    );
}

#[test]
fn opencode_role_uninstall_keeps_user_modified_generated_role_and_manifest_entry() {
    let temp = tempfile::tempdir().unwrap();
    let shared = temp.path().join("shared");
    let roles = shared.join("Agents/Roles");
    std::fs::create_dir_all(&roles).unwrap();
    std::fs::write(shared.join("AGENTS.md"), "# Shared instructions\n").unwrap();
    std::fs::write(
        shared.join("agent-home.toml"),
        "schema = 1\n[team]\nname = \"Example\"\norg = \"Example\"\nenv_prefix = \"EX\"\nrole_prefix = \"ex-\"\n[profiles]\ndefault = \"developer\"\n",
    ).unwrap();
    std::fs::write(
        roles.join("example.md"),
        "---\nname: example\ndescription: Example.\n---\nPrompt.\n",
    )
    .unwrap();
    let home = temp.path().join("home");
    let target = home.join(".config/opencode/agents/ex-example.md");
    install_shared(&home, None, "core", &shared).success();
    std::fs::write(&target, "user edit\n").unwrap();
    let output = uninstall(&home).success().get_output().clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("Skipped modified generated file"),
        "{stdout}"
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"user edit\n");
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(home.join(".config/agent-home/manifest.json")).unwrap(),
    )
    .unwrap();
    assert!(
        manifest["entries"][target.to_str().unwrap()].is_object(),
        "modified generated file stays recorded"
    );
}

#[test]
fn opencode_config_uninstall_preserves_replacement_symlink_and_referent() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        install_shared(&home, None, "core", &fixture_home()).success();
        let target = opencode_config(&home);
        let installed = std::fs::read(&target).unwrap();
        let outside = temp.path().join("outside-config.json");
        std::fs::write(&outside, &installed).unwrap();
        std::fs::remove_file(&target).unwrap();
        symlink(&outside, &target).unwrap();

        let output = uninstall(&home).success().get_output().clone();
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("Skipped replaced managed target"),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );

        assert!(target.is_symlink());
        assert_eq!(std::fs::read(&outside).unwrap(), installed);
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(home.join(".config/agent-home/manifest.json")).unwrap(),
        )
        .unwrap();
        assert!(manifest["entries"][target.to_str().unwrap()].is_object());
    }
}

#[test]
fn opencode_role_reinstall_prunes_unchanged_files_after_prefix_change() {
    let temp = tempfile::tempdir().unwrap();
    let shared = temp.path().join("shared");
    let roles = shared.join("Agents/Roles");
    std::fs::create_dir_all(&roles).unwrap();
    std::fs::write(shared.join("AGENTS.md"), "# Shared instructions\n").unwrap();
    let config = shared.join("agent-home.toml");
    std::fs::write(
        &config,
        "schema = 1\n[team]\nname = \"Example\"\norg = \"Example\"\nenv_prefix = \"EX\"\nrole_prefix = \"ex-\"\n[profiles]\ndefault = \"developer\"\n",
    )
    .unwrap();
    std::fs::write(
        roles.join("example.md"),
        "---\nname: example\ndescription: Example.\n---\nPrompt.\n",
    )
    .unwrap();
    let home = temp.path().join("home");
    let agents = home.join(".config/opencode/agents");
    let old = agents.join("ex-example.md");
    let new = agents.join("team-example.md");
    install_shared(&home, None, "core", &shared).success();

    let initial = std::fs::read(&config).unwrap();
    let changed = String::from_utf8(initial)
        .unwrap()
        .replace("role_prefix = \"ex-\"", "role_prefix = \"team-\"");
    std::fs::write(&config, changed).unwrap();
    install_shared(&home, None, "core", &shared).success();

    assert!(!old.exists(), "unchanged stale generated role is removed");
    assert!(new.is_file(), "current-prefix role is installed");
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(home.join(".config/agent-home/manifest.json")).unwrap(),
    )
    .unwrap();
    assert!(manifest["entries"][old.to_str().unwrap()].is_null());
    assert!(manifest["entries"][new.to_str().unwrap()].is_object());
}

#[test]
fn opencode_role_reinstall_preserves_modified_stale_file_and_manifest_ownership() {
    let temp = tempfile::tempdir().unwrap();
    let shared = temp.path().join("shared");
    let roles = shared.join("Agents/Roles");
    std::fs::create_dir_all(&roles).unwrap();
    std::fs::write(shared.join("AGENTS.md"), "# Shared instructions\n").unwrap();
    let config = shared.join("agent-home.toml");
    std::fs::write(
        &config,
        "schema = 1\n[team]\nname = \"Example\"\norg = \"Example\"\nenv_prefix = \"EX\"\nrole_prefix = \"ex-\"\n[profiles]\ndefault = \"developer\"\n",
    )
    .unwrap();
    std::fs::write(
        roles.join("example.md"),
        "---\nname: example\ndescription: Example.\n---\nPrompt.\n",
    )
    .unwrap();
    let home = temp.path().join("home");
    let agents = home.join(".config/opencode/agents");
    let old = agents.join("ex-example.md");
    let new = agents.join("team-example.md");
    install_shared(&home, None, "core", &shared).success();
    std::fs::write(&old, b"user-edited role bytes\n").unwrap();

    let changed = std::fs::read_to_string(&config)
        .unwrap()
        .replace("role_prefix = \"ex-\"", "role_prefix = \"team-\"");
    std::fs::write(&config, changed).unwrap();
    let output = install_shared(&home, None, "core", &shared)
        .success()
        .get_output()
        .clone();

    assert!(String::from_utf8_lossy(&output.stdout).contains("Kept modified stale generated file"));
    assert_eq!(std::fs::read(&old).unwrap(), b"user-edited role bytes\n");
    assert!(new.is_file());
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(home.join(".config/agent-home/manifest.json")).unwrap(),
    )
    .unwrap();
    assert!(
        manifest["entries"][old.to_str().unwrap()].is_object(),
        "modified stale file remains recorded as managed"
    );
    assert!(manifest["entries"][new.to_str().unwrap()].is_object());
}

#[test]
fn opencode_role_reinstall_restores_a_missing_generated_file() {
    let temp = tempfile::tempdir().unwrap();
    let shared = temp.path().join("shared");
    let roles = shared.join("Agents/Roles");
    std::fs::create_dir_all(&roles).unwrap();
    std::fs::write(shared.join("AGENTS.md"), "# Shared instructions\n").unwrap();
    std::fs::write(
        shared.join("agent-home.toml"),
        "schema = 1\n[team]\nname = \"Example\"\norg = \"Example\"\nenv_prefix = \"EX\"\nrole_prefix = \"ex-\"\n[profiles]\ndefault = \"developer\"\n",
    )
    .unwrap();
    std::fs::write(
        roles.join("example.md"),
        "---\nname: example\ndescription: Example.\n---\nPrompt.\n",
    )
    .unwrap();
    let home = temp.path().join("home");
    let target = home.join(".config/opencode/agents/ex-example.md");
    install_shared(&home, None, "core", &shared).success();
    std::fs::remove_file(&target).unwrap();

    install_shared(&home, None, "core", &shared).success();

    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"---\ndescription: Example.\nmode: subagent\n---\nPrompt.\n"
    );
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(home.join(".config/agent-home/manifest.json")).unwrap(),
    )
    .unwrap();
    assert!(manifest["entries"][target.to_str().unwrap()].is_object());
}

#[test]
fn opencode_role_reinstall_refuses_modified_generated_file_before_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    let shared = temp.path().join("shared");
    let roles = shared.join("Agents/Roles");
    std::fs::create_dir_all(&roles).unwrap();
    std::fs::write(shared.join("AGENTS.md"), "# Shared instructions\n").unwrap();
    std::fs::write(
        shared.join("agent-home.toml"),
        "schema = 1\n[team]\nname = \"Example\"\norg = \"Example\"\nenv_prefix = \"EX\"\nrole_prefix = \"ex-\"\n[profiles]\ndefault = \"developer\"\n",
    ).unwrap();
    std::fs::write(
        roles.join("example.md"),
        "---\nname: example\ndescription: Example.\n---\nPrompt.\n",
    )
    .unwrap();
    let home = temp.path().join("home");
    let target = home.join(".config/opencode/agents/ex-example.md");
    install_shared(&home, None, "core", &shared).success();
    std::fs::write(&target, "user edit\n").unwrap();
    let output = install_shared(&home, None, "core", &shared)
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("recorded generated file was modified"),
        "{stderr}"
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"user edit\n");
    let backup_count = std::fs::read_dir(target.parent().unwrap())
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
        backup_count, 0,
        "failed reinstall creates no untracked backup"
    );
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
    install_shared(&home, Some(&vault), "developer", &fixture_home()).success();
    assert_ne!(std::fs::read(&config).unwrap(), original, "install merged");
    uninstall(&home).success();
    assert_eq!(
        std::fs::read(&config).unwrap(),
        original,
        "uninstall must restore byte-for-byte"
    );
}
#[test]
fn uninstall_removes_an_opencode_config_vesica_created_and_keeps_a_modified_one() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    install_shared(&home, None, "core", &fixture_home()).success();
    assert!(opencode_config(&home).is_file());
    uninstall(&home).success();
    assert!(
        !opencode_config(&home).exists(),
        "a file Vesica created is removed"
    );

    // A merged file the user edited afterwards is kept, and so is its manifest entry.
    let other = temp.path().join("home2");
    install_shared(&other, None, "core", &fixture_home()).success();
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
