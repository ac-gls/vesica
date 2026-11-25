//! Disposable synthetic acceptance matrices for the remediation report.
use assert_cmd::Command;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home")
}
fn install(root: &Path) {
    Command::cargo_bin("vesica")
        .unwrap()
        .arg("install")
        .arg("--home-repo")
        .arg(fixture())
        .arg("--root")
        .arg(root)
        .assert()
        .success();
}
fn uninstall(root: &Path) {
    Command::cargo_bin("vesica")
        .unwrap()
        .arg("uninstall")
        .arg("--root")
        .arg(root)
        .assert()
        .success();
}
fn manifest(root: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(root.join(".config/agent-home/manifest.json")).unwrap())
        .unwrap()
}

/// Snapshot every node without following symlinks, including empty directories and permissions.
fn tree_snapshot(root: &Path) -> std::collections::BTreeMap<PathBuf, (String, Vec<u8>, u32)> {
    fn visit(
        root: &Path,
        path: &Path,
        result: &mut std::collections::BTreeMap<PathBuf, (String, Vec<u8>, u32)>,
    ) {
        let metadata = std::fs::symlink_metadata(path).unwrap();
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o777
        };
        #[cfg(not(unix))]
        let mode = u32::from(metadata.permissions().readonly());
        let (kind, bytes) = if metadata.file_type().is_symlink() {
            (
                "link",
                std::fs::read_link(path)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec(),
            )
        } else if metadata.is_dir() {
            ("directory", Vec::new())
        } else if metadata.is_file() {
            ("file", std::fs::read(path).unwrap())
        } else {
            ("other", Vec::new())
        };
        result.insert(
            path.strip_prefix(root).unwrap().to_path_buf(),
            (kind.into(), bytes, mode),
        );
        if metadata.is_dir() {
            for child in std::fs::read_dir(path).unwrap() {
                visit(root, &child.unwrap().path(), result);
            }
        }
    }
    let mut result = std::collections::BTreeMap::new();
    visit(root, root, &mut result);
    result
}

#[cfg(unix)]
#[test]
fn replaced_managed_file_ownership_matrix_retains_exact_entries_and_outside_nodes() {
    use std::os::unix::fs::symlink;
    for (relative, expected_kind) in [
        (".bashrc", "managed-block"),
        (".config/opencode/opencode.json", "merged-file"),
    ] {
        for kind in ["same-link", "different-link", "dangling-link", "directory"] {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path().join("home");
            let target = root.join(relative);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            let original: &[u8] = if relative.ends_with(".json") {
                b"{\"private\":true}\n"
            } else {
                b"user original\n"
            };
            std::fs::write(&target, original).unwrap();
            install(&root);
            let before_manifest = manifest(&root);
            let recorded = before_manifest["entries"][target.to_str().unwrap()].clone();
            assert_eq!(recorded["kind"], expected_kind, "{relative}");
            assert!(recorded["backup"].is_string());
            let installed = std::fs::read(&target).unwrap();
            std::fs::remove_file(&target).unwrap();
            let outside = tmp.path().join("outside");
            let outside_bytes = if kind == "same-link" {
                installed
            } else {
                b"outside sentinel\n".to_vec()
            };
            if kind != "dangling-link" {
                std::fs::write(&outside, &outside_bytes).unwrap();
            }
            if kind == "directory" {
                std::fs::create_dir(&target).unwrap();
                std::fs::write(target.join("sentinel"), &outside_bytes).unwrap();
            } else {
                symlink(&outside, &target).unwrap();
            }
            for attempt in 0..2 {
                let output = Command::cargo_bin("vesica")
                    .unwrap()
                    .arg("uninstall")
                    .arg("--root")
                    .arg(&root)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{relative}/{kind} attempt {attempt}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let stdout = String::from_utf8_lossy(&output.stdout);
                assert!(
                    stdout.contains(&format!(
                        "Skipped replaced managed target: {}",
                        target.display()
                    )),
                    "{relative}/{kind} attempt {attempt}: {stdout}"
                );
            }
            let retained = manifest(&root);
            assert_eq!(
                retained["entries"][target.to_str().unwrap()],
                recorded,
                "{relative}/{kind}"
            );
            assert_eq!(retained["entries"].as_object().unwrap().len(), 1);
            if kind == "directory" {
                assert_eq!(
                    std::fs::read(target.join("sentinel")).unwrap(),
                    outside_bytes
                );
            } else {
                assert_eq!(std::fs::read_link(&target).unwrap(), outside);
            }
            if kind == "dangling-link" {
                assert!(!outside.exists());
            } else {
                assert_eq!(std::fs::read(&outside).unwrap(), outside_bytes);
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn configuration_symlink_refusal_matrix_preserves_links_referents_and_other_targets() {
    use std::os::unix::fs::symlink;
    for relative in [
        ".bashrc",
        ".claude/CLAUDE.md",
        ".codex/AGENTS.md",
        ".config/opencode/opencode.json",
        ".config/agent-home/config.toml",
    ] {
        for dangling in [false, true] {
            for obstruct_commit in [false, true] {
                let tmp = tempfile::tempdir().unwrap();
                let root = tmp.path().join("home");
                let target = root.join(relative);
                std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                let outside = tmp.path().join("outside");
                let outside_bytes: &[u8] = if relative == ".config/agent-home/config.toml" {
                    b"schema = 1\n# owner machine config\n"
                } else {
                    b"outside sentinel\n"
                };
                if !dangling {
                    std::fs::write(&outside, outside_bytes).unwrap();
                }
                symlink(&outside, &target).unwrap();
                if obstruct_commit {
                    std::fs::create_dir_all(root.join(".config/agent-home/manifest.json.tmp"))
                        .unwrap();
                }
                let vault = tmp.path().join("vault");
                std::fs::create_dir(&vault).unwrap();
                std::fs::write(vault.join("owner-note.md"), b"unmanaged owner note\n").unwrap();
                let before = tree_snapshot(tmp.path());
                let output = Command::cargo_bin("vesica")
                    .unwrap()
                    .arg("install")
                    .arg("--home-repo")
                    .arg(fixture())
                    .arg("--root")
                    .arg(&root)
                    .arg("--vault")
                    .arg(&vault)
                    .assert()
                    .failure()
                    .get_output()
                    .clone();
                let diagnostic = String::from_utf8_lossy(&output.stderr);
                assert!(
                    diagnostic.contains("regular file")
                        || diagnostic.contains("cannot load local configuration"),
                    "{relative}/{dangling}/{obstruct_commit}: {diagnostic}"
                );
                assert_eq!(
                    tree_snapshot(tmp.path()),
                    before,
                    "{relative}/{dangling}/{obstruct_commit}: {diagnostic}"
                );
                assert_eq!(std::fs::read_link(&target).unwrap(), outside);
                if dangling {
                    assert!(!outside.exists());
                } else {
                    assert_eq!(std::fs::read(&outside).unwrap(), outside_bytes);
                }
                assert!(!root.join("Team").is_symlink());
                assert!(!root.join(".config/agent-home/manifest.json").exists());
            }
        }
    }
}

#[test]
fn absent_managed_target_kind_matrix_is_idempotent_without_stale_ownership() {
    use sha2::Digest;
    let kinds = [
        "symlink",
        "managed-block",
        "merged-file",
        "generated-file",
        "file",
    ];
    for kind in kinds.into_iter().chain(["all"]) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("home");
        install(&root);
        let mut m = manifest(&root);
        // The CLI generates four kinds; retain a hashed legacy `file` entry to exercise its
        // supported fallback consumer, both by itself and alongside every planner-produced kind.
        let extra = root.join("synthetic.txt");
        std::fs::write(&extra, b"synthetic\n").unwrap();
        m["entries"][extra.to_str().unwrap()] = serde_json::json!({
            "kind":"file", "target":extra, "source":null, "backup":null,
            "original_sha256":null,
            "installed_sha256":hex::encode(sha2::Sha256::digest(b"synthetic\n"))
        });
        let actual: std::collections::BTreeSet<_> = m["entries"]
            .as_object()
            .unwrap()
            .values()
            .map(|entry| entry["kind"].as_str().unwrap())
            .collect();
        assert_eq!(actual, std::collections::BTreeSet::from(kinds));
        let matched: Vec<_> = m["entries"]
            .as_object()
            .unwrap()
            .values()
            .filter(|entry| kind == "all" || entry["kind"] == kind)
            .map(|entry| PathBuf::from(entry["target"].as_str().unwrap()))
            .collect();
        assert!(!matched.is_empty(), "fixture must cover {kind}");
        for target in &matched {
            std::fs::remove_file(target).unwrap();
        }
        std::fs::write(
            root.join(".config/agent-home/manifest.json"),
            serde_json::to_vec(&m).unwrap(),
        )
        .unwrap();
        uninstall(&root);
        uninstall(&root);
        assert!(
            !root.join(".config/agent-home/manifest.json").exists(),
            "{kind}"
        );
        assert!(!root.join("Team").is_symlink());
        for target in matched {
            assert!(
                std::fs::symlink_metadata(&target).is_err(),
                "missing target must stay absent: {}",
                target.display()
            );
        }
        assert!(
            !extra.exists(),
            "verified legacy file is removed in every case"
        );
    }
}

#[test]
fn later_uninstall_read_error_keeps_exact_pending_ownership_and_retry_completes() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("home");
    install(&root);
    let before = manifest(&root);
    let target = root.join(".codex/AGENTS.md");
    let installed = std::fs::read(&target).unwrap();
    // A real later read_to_string failure, deterministic under root, after earlier removals.
    std::fs::write(&target, [0xff, 0xfe]).unwrap();
    let output = Command::cargo_bin("vesica")
        .unwrap()
        .args(["uninstall", "--root"])
        .arg(&root)
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("valid UTF-8"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pending = manifest(&root);
    let entries = pending["entries"].as_object().unwrap();
    assert!(!entries.is_empty());
    assert!(
        entries.len() < before["entries"].as_object().unwrap().len(),
        "must reach later failure after recorded progress"
    );
    assert_eq!(
        pending["entries"][target.to_str().unwrap()],
        before["entries"][target.to_str().unwrap()]
    );
    for (key, entry) in entries {
        assert_eq!(
            entry, &before["entries"][key],
            "pending ownership stays exact"
        );
        assert!(
            std::fs::symlink_metadata(key).is_ok(),
            "no stale ownership: {key}"
        );
    }
    for key in before["entries"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| !entries.contains_key(*key))
    {
        assert!(
            std::fs::symlink_metadata(key).is_err(),
            "completed target must be absent: {key}"
        );
    }
    assert_eq!(
        std::fs::read(&target).unwrap(),
        [0xff, 0xfe],
        "unreadable user content is retained"
    );
    std::fs::write(&target, installed).unwrap(); // Owner repairs the obstruction before retry.
    uninstall(&root);
    uninstall(&root);
    assert!(!root.join(".config/agent-home/manifest.json").exists());
}

#[cfg(unix)]
#[test]
fn private_configuration_permission_matrix_install_rollback_and_uninstall() {
    use std::os::unix::fs::PermissionsExt;
    for umask in ["000", "022", "077"] {
        for mode in [0o600, 0o640] {
            for fail_commit in [false, true] {
                let tmp = tempfile::tempdir().unwrap();
                let root = tmp.path().join("home");
                let paths = [
                    ".bashrc",
                    ".claude/CLAUDE.md",
                    ".codex/AGENTS.md",
                    ".config/opencode/opencode.json",
                    ".config/agent-home/config.env",
                ];
                for relative in paths {
                    let p = root.join(relative);
                    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                    std::fs::write(
                        &p,
                        if relative.ends_with(".json") {
                            b"{}\n".as_slice()
                        } else {
                            b"private user config\n".as_slice()
                        },
                    )
                    .unwrap();
                    std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap();
                }
                if fail_commit {
                    std::fs::create_dir_all(root.join(".config/agent-home/manifest.json.tmp"))
                        .unwrap();
                }
                let bin = assert_cmd::cargo::cargo_bin("vesica");
                let output = std::process::Command::new("sh")
                    .args(["-c", "umask \"$1\"; shift; exec \"$@\"", "sh", umask])
                    .arg(bin)
                    .args(["install", "--home-repo"])
                    .arg(fixture())
                    .arg("--root")
                    .arg(&root)
                    .output()
                    .unwrap();
                assert_eq!(
                    output.status.success(),
                    !fail_commit,
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                if !fail_commit {
                    let installed = manifest(&root);
                    for relative in paths {
                        let target = root.join(relative);
                        assert_eq!(
                            std::fs::symlink_metadata(&target)
                                .unwrap()
                                .permissions()
                                .mode()
                                & 0o777,
                            mode,
                            "installed target {relative}/{umask}/{mode:o}"
                        );
                        let backup = installed["entries"][target.to_str().unwrap()]["backup"]
                            .as_str()
                            .expect("every original managed config must have a recorded backup");
                        let metadata = std::fs::symlink_metadata(backup).unwrap();
                        assert!(metadata.is_file() && !metadata.file_type().is_symlink());
                        assert_eq!(
                            metadata.permissions().mode() & 0o777,
                            0o600,
                            "backup {relative}/{umask}/{mode:o}"
                        );
                        assert_eq!(
                            std::fs::read(backup).unwrap(),
                            if relative.ends_with(".json") {
                                b"{}\n".as_slice()
                            } else {
                                b"private user config\n".as_slice()
                            }
                        );
                    }
                    uninstall(&root);
                }
                for relative in paths {
                    let p = root.join(relative);
                    assert_eq!(
                        std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
                        mode,
                        "{relative}/{umask}/{fail_commit}"
                    );
                    assert_eq!(
                        std::fs::read(p).unwrap(),
                        if relative.ends_with(".json") {
                            b"{}\n".as_slice()
                        } else {
                            b"private user config\n".as_slice()
                        }
                    );
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn install_rejects_symlinked_machine_root_ancestor_before_mutation() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    let outside = tmp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("sentinel"), b"unchanged").unwrap();
    let link = tmp.path().join("link");
    symlink(&outside, &link).unwrap();
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(fixture())
        .arg("--root")
        .arg(link.join("home"))
        .assert()
        .failure();
    assert_eq!(
        std::fs::read(outside.join("sentinel")).unwrap(),
        b"unchanged"
    );
    assert!(!outside.join("home").exists());
}

#[cfg(unix)]
#[test]
fn failed_manifest_commit_roundtrips_present_and_absent_local_config_mode() {
    use std::os::unix::fs::PermissionsExt;
    for present in [true, false] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("home");
        let cfg = root.join(".config/agent-home");
        std::fs::create_dir_all(cfg.join("manifest.json.tmp")).unwrap();
        let local = cfg.join("config.toml");
        let bytes = b"schema = 1\nprofile = \"core\"\n# preserve\n";
        if present {
            std::fs::write(&local, bytes).unwrap();
            std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o640)).unwrap();
        }
        let output = Command::cargo_bin("vesica")
            .unwrap()
            .args(["install", "--home-repo"])
            .arg(fixture())
            .args(["--profile", "lead", "--root"])
            .arg(&root)
            .assert()
            .failure()
            .get_output()
            .clone();
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("manifest"),
            "must reach manifest persistence failure: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            cfg.join("manifest.json.tmp").is_dir(),
            "keep the user-owned obstruction"
        );
        assert!(
            tree_snapshot(&root)
                .keys()
                .all(|path| !path.to_string_lossy().contains("vesica-backup")),
            "failed commit must remove transaction-created backups"
        );
        if present {
            assert_eq!(std::fs::read(&local).unwrap(), bytes);
            assert_eq!(
                std::fs::symlink_metadata(local)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o640
            );
        } else {
            assert!(std::fs::symlink_metadata(local).is_err());
        }
        assert!(!root.join("Team").is_symlink());
        assert!(!cfg.join("manifest.json").exists());
    }
}

#[test]
fn relative_root_cross_working_directory_status_update_uninstall_keeps_copied_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    let root = a.join("home");
    Command::cargo_bin("vesica")
        .unwrap()
        .current_dir(&a)
        .args(["install", "--root", "home", "--home-repo"])
        .arg(fixture())
        .assert()
        .success();
    let copy = b.join("home");
    assert!(std::process::Command::new("cp")
        .args(["-a"])
        .arg(&root)
        .arg(&copy)
        .status()
        .unwrap()
        .success());
    let copy_manifest = std::fs::read(copy.join(".config/agent-home/manifest.json")).unwrap();
    let copy_config = std::fs::read(copy.join(".config/opencode/opencode.json")).unwrap();
    let copy_team = std::fs::read_link(copy.join("Team")).unwrap();
    for entry in manifest(&root)["entries"].as_object().unwrap().values() {
        assert!(Path::new(entry["target"].as_str().unwrap()).is_absolute());
        if let Some(p) = entry["backup"].as_str() {
            assert!(Path::new(p).is_absolute());
        }
    }
    Command::cargo_bin("vesica")
        .unwrap()
        .current_dir(&b)
        .arg("status")
        .arg("--root")
        .arg(&root)
        .assert()
        .success();
    let binary = assert_cmd::cargo::cargo_bin("vesica");
    Command::cargo_bin("vesica")
        .unwrap()
        .current_dir(&b)
        .args([
            "update",
            "--tag",
            concat!("v", env!("CARGO_PKG_VERSION")),
            "--target",
        ])
        .arg(binary)
        .arg("--root")
        .arg(&root)
        .assert()
        .success();
    Command::cargo_bin("vesica")
        .unwrap()
        .current_dir(&b)
        .arg("uninstall")
        .arg("--root")
        .arg(&root)
        .assert()
        .success();
    assert!(!root.join("Team").is_symlink());
    assert_eq!(
        std::fs::read(copy.join(".config/agent-home/manifest.json")).unwrap(),
        copy_manifest
    );
    assert_eq!(
        std::fs::read(copy.join(".config/opencode/opencode.json")).unwrap(),
        copy_config
    );
    assert_eq!(std::fs::read_link(copy.join("Team")).unwrap(), copy_team);
}

#[cfg(unix)]
#[test]
fn merged_toml_planner_symlink_refusal_matrix_preserves_link_referent_and_prior_actions() {
    use std::os::unix::fs::symlink;
    for dangling in [false, true] {
        for obstruct_commit in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path().join("home");
            std::fs::create_dir(&root).unwrap();
            let target = root.join("settings.toml");
            let outside = tmp.path().join("outside.toml");
            if !dangling {
                std::fs::write(&outside, b"# owner settings\nvalue = 1 # keep\n").unwrap();
            }
            symlink(&outside, &target).unwrap();
            let blocker = root.join("manifest.json.tmp");
            if obstruct_commit {
                std::fs::create_dir(&blocker).unwrap();
            }
            let before = tree_snapshot(tmp.path());
            let prior = root.join("a-prior-action.txt");
            // install currently emits no MergeToml actions; exercise its supported core
            // planner/executor path directly, not a fabricated CLI configuration schema.
            let mut planner = ah_core::Planner::new();
            planner
                .push(ah_core::Action::WriteFile {
                    path: prior.display().to_string(),
                    content: "created before merge\n".into(),
                })
                .unwrap();
            planner
                .push(ah_core::Action::MergeToml {
                    path: target.display().to_string(),
                    patch: serde_json::json!({"managed":true}),
                })
                .unwrap();
            let mut commit_called = false;
            let error = ah_core::action::Executor
                .execute_with_commit(&planner.finish(false), |_| {
                    commit_called = true;
                    if obstruct_commit {
                        ah_core::Manifest::default().save_atomic(&root.join("manifest.json"))?;
                    }
                    Ok(())
                })
                .unwrap_err();
            assert!(!commit_called, "symlink must refuse before commit");
            assert!(format!("{error:#}").contains("regular file"), "{error:#}");
            assert_eq!(
                tree_snapshot(tmp.path()),
                before,
                "dangling={dangling}, obstruct_commit={obstruct_commit}"
            );
            assert_eq!(std::fs::read_link(&target).unwrap(), outside);
            assert!(!prior.exists());
        }
    }
}

#[cfg(unix)]
#[test]
fn successful_install_records_only_committed_machine_facts() {
    use std::os::unix::fs::PermissionsExt;
    for present in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("home");
        let local = root.join(".config/agent-home/config.toml");
        if present {
            std::fs::create_dir_all(local.parent().unwrap()).unwrap();
            std::fs::write(&local, b"schema = 1\nprofile = \"lead\"\n").unwrap();
            std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o640)).unwrap();
        }
        let vault = root.join("vault");
        Command::cargo_bin("vesica")
            .unwrap()
            .args(["install", "--home-repo"])
            .arg(fixture())
            .args(["--profile", "core", "--root"])
            .arg(&root)
            .arg("--vault")
            .arg(&vault)
            .assert()
            .success();
        let committed = manifest(&root);
        let config = ah_core::LocalConfig::load(local.parent().unwrap()).unwrap();
        assert_eq!(
            config.home_repo.as_ref().unwrap(),
            &fixture().canonicalize().unwrap()
        );
        assert_eq!(config.profile, Some(ah_core::Profile::Core));
        assert_eq!(config.vault.as_ref().unwrap(), &vault);
        assert!(config.repos_dir.is_none());
        assert!(
            config.repos.is_empty(),
            "team repositories must not become local machine facts"
        );
        assert_eq!(
            committed["shared_home"],
            config.home_repo.unwrap().to_str().unwrap()
        );
        assert_eq!(committed["profile"], "core");
        assert_eq!(committed["vault"], vault.to_str().unwrap());
        for entry in committed["entries"].as_object().unwrap().values() {
            assert!(std::fs::symlink_metadata(entry["target"].as_str().unwrap()).is_ok());
        }
        assert_eq!(
            std::fs::metadata(local).unwrap().permissions().mode() & 0o777,
            if present { 0o640 } else { 0o600 }
        );
    }
}

#[cfg(unix)]
#[test]
fn local_config_collision_and_type_errors_refuse_before_unmanaged_writes() {
    for kind in ["directory", "state-directory-is-file", "invalid-toml"] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("home");
        let cfg = root.join(".config/agent-home");
        let local = cfg.join("config.toml");
        if kind == "state-directory-is-file" {
            std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
            std::fs::write(&cfg, b"user-owned collision\n").unwrap();
        } else {
            std::fs::create_dir_all(&cfg).unwrap();
            if kind == "directory" {
                std::fs::create_dir(&local).unwrap();
                std::fs::write(local.join("sentinel"), b"user directory\n").unwrap();
            } else {
                std::fs::write(&local, b"schema = [unterminated\n").unwrap();
            }
        }
        let vault = tmp.path().join("vault");
        std::fs::create_dir(&vault).unwrap();
        std::fs::write(vault.join("owner-note.md"), b"unmanaged owner note\n").unwrap();
        let before = tree_snapshot(tmp.path());
        let output = Command::cargo_bin("vesica")
            .unwrap()
            .args(["install", "--home-repo"])
            .arg(fixture())
            .args(["--profile", "core", "--root"])
            .arg(&root)
            .arg("--vault")
            .arg(&vault)
            .assert()
            .failure()
            .get_output()
            .clone();
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        let expected = match kind {
            "directory" => "Is a directory",
            "state-directory-is-file" => "Not a directory",
            _ => "cannot load local configuration",
        };
        assert!(diagnostic.contains(expected), "{kind}: {diagnostic}");
        assert_eq!(tree_snapshot(tmp.path()), before, "{kind}: {diagnostic}");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn local_config_permission_error_refuses_before_unmanaged_vault_writes() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::os::unix::process::CommandExt;

    let tmp = tempfile::tempdir().unwrap();
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let root = tmp.path().join("home");
    let cfg = root.join(".config/agent-home");
    std::fs::create_dir_all(&cfg).unwrap();
    for directory in [&root, &root.join(".config")] {
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o777)).unwrap();
    }
    let local = cfg.join("config.toml");
    let bytes = b"schema = 1\n# private machine facts\n";
    std::fs::write(&local, bytes).unwrap();
    std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o644)).unwrap();
    // Atomic replacement needs a writable parent, even when the target itself is readable.
    std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o555)).unwrap();
    let vault = tmp.path().join("vault");
    std::fs::create_dir(&vault).unwrap();
    std::fs::set_permissions(&vault, std::fs::Permissions::from_mode(0o777)).unwrap();
    let sentinel = vault.join("owner-note.md");
    std::fs::write(&sentinel, b"owner note, unchanged\n").unwrap();

    let mut command = std::process::Command::new(assert_cmd::cargo::cargo_bin("vesica"));
    command
        .args(["install", "--home-repo"])
        .arg(fixture())
        .args(["--profile", "core", "--root"])
        .arg(&root)
        .arg("--vault")
        .arg(&vault);
    if std::fs::metadata("/proc/self").unwrap().uid() == 0 {
        // Drop root's DAC bypass. TMPDIR's owner can traverse the checkout's ancestors.
        let owner = std::fs::metadata(tmp.path().parent().unwrap()).unwrap();
        assert_ne!(
            owner.uid(),
            0,
            "root-safe permission fixture needs a non-root TMPDIR owner"
        );
        command.gid(owner.gid()).uid(owner.uid());
    }
    let output = command.output().unwrap();
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "permission obstruction must refuse: {diagnostic}"
    );
    assert!(
        diagnostic.contains("Permission denied"),
        "must reach the permission failure, not a setup error: {diagnostic}"
    );
    assert_eq!(std::fs::read(&local).unwrap(), bytes);
    assert_eq!(
        std::fs::metadata(&local).unwrap().permissions().mode() & 0o777,
        0o644
    );
    assert_eq!(
        std::fs::read(&sentinel).unwrap(),
        b"owner note, unchanged\n"
    );
    let names: std::collections::BTreeSet<_> = std::fs::read_dir(&vault)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(
        names,
        std::collections::BTreeSet::from([std::ffi::OsString::from("owner-note.md")]),
        "local config persistence must fail before unmanaged vault initialization: {diagnostic}"
    );
    assert!(!root.join("Team").is_symlink());
    assert!(!cfg.join("manifest.json").exists());
}

#[test]
fn all_cli_git_subprocess_construction_uses_the_sanitized_wrapper() {
    for name in ["src/main.rs", "src/repos.rs"] {
        let source =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(name)).unwrap();
        assert!(
            !source.contains("Command::new(\"git\")"),
            "{name} retains an unsanitized subprocess"
        );
    }
}

#[test]
fn legacy_relative_manifest_fields_refuse_with_owner_recovery_guidance_without_mutation() {
    for field in ["shared_home", "vault", "target", "backup", "source"] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("home");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".bashrc"), "synthetic original\n").unwrap();
        install(&root);
        let manifest_path = root.join(".config/agent-home/manifest.json");
        let mut old = manifest(&root);
        if matches!(field, "shared_home" | "vault") {
            old[field] = serde_json::json!("legacy-relative");
        } else {
            let entry = old["entries"]
                .as_object_mut()
                .unwrap()
                .values_mut()
                .find(|entry| entry["backup"].is_string())
                .unwrap();
            entry[field] = serde_json::json!("legacy-relative");
        }
        std::fs::write(&manifest_path, serde_json::to_vec_pretty(&old).unwrap()).unwrap();
        std::fs::write(
            tmp.path().join("outside-sentinel"),
            b"unmanaged outside bytes\n",
        )
        .unwrap();
        let other_cwd = tmp.path().join("other-cwd");
        std::fs::create_dir(&other_cwd).unwrap();
        let before = tree_snapshot(tmp.path());
        for (operation, dry) in [
            ("uninstall", false),
            ("uninstall", true),
            ("install", false),
            ("install", true),
        ] {
            let mut cmd = Command::cargo_bin("vesica").unwrap();
            cmd.current_dir(&other_cwd)
                .args([operation, "--root"])
                .arg(&root);
            if operation == "install" {
                cmd.arg("--home-repo").arg(fixture());
            }
            if dry {
                cmd.arg("--dry-run");
            }
            let out = cmd.output().unwrap();
            assert!(
                !out.status.success(),
                "{operation} accepted relative {field}"
            );
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(stderr.contains("not absolute"), "{field}: {stderr}");
            assert!(
                stderr.contains("do not rebase using the current directory"),
                "missing recovery guidance for {field}: {stderr}"
            );
            assert!(
                stderr.contains("docs/TROUBLESHOOTING.md"),
                "{field}: {stderr}"
            );
            assert_eq!(
                tree_snapshot(tmp.path()),
                before,
                "{operation}/{dry}/{field} changed state"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn new_private_configuration_defaults_remain_0600_across_supported_umasks() {
    use std::os::unix::fs::PermissionsExt;
    for umask in ["000", "022", "077"] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("home");
        let out = std::process::Command::new("sh")
            .args(["-c", "umask \"$1\"; shift; exec \"$@\"", "sh", umask])
            .arg(assert_cmd::cargo::cargo_bin("vesica"))
            .args(["install", "--home-repo"])
            .arg(fixture())
            .arg("--root")
            .arg(&root)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        for relative in [
            ".bashrc",
            ".claude/CLAUDE.md",
            ".codex/AGENTS.md",
            ".config/opencode/opencode.json",
            ".config/agent-home/config.env",
            ".config/agent-home/config.toml",
            ".config/agent-home/manifest.json",
        ] {
            let metadata = std::fs::symlink_metadata(root.join(relative)).unwrap();
            assert!(metadata.is_file() && !metadata.file_type().is_symlink());
            assert_eq!(
                metadata.permissions().mode() & 0o777,
                0o600,
                "{relative}/{umask}"
            );
        }
        uninstall(&root);
    }
}

#[test]
fn install_preview_does_not_certify_recorded_backup_integrity() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("home");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join(".bashrc"), b"synthetic original\n").unwrap();
    install(&root);
    let installed = manifest(&root);
    let key = root.join(".bashrc");
    let backup = installed["entries"][key.to_str().unwrap()]["backup"]
        .as_str()
        .unwrap();
    std::fs::write(backup, b"changed synthetic backup\n").unwrap();
    let before = tree_snapshot(tmp.path());
    for dry in [true, false] {
        let mut cmd = Command::cargo_bin("vesica").unwrap();
        cmd.args(["install", "--home-repo"])
            .arg(fixture())
            .arg("--root")
            .arg(&root);
        if dry {
            cmd.arg("--dry-run");
        }
        let out = cmd.output().unwrap();
        assert_eq!(
            out.status.success(),
            dry,
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        if !dry {
            assert!(String::from_utf8_lossy(&out.stderr).contains("recorded backup was modified"));
        }
        assert_eq!(
            tree_snapshot(tmp.path()),
            before,
            "dry={dry} changed fixture"
        );
    }
}

#[cfg(unix)]
#[test]
fn reinstall_rewrites_manifest_privately_but_preserves_managed_config_modes() {
    use std::os::unix::fs::PermissionsExt;
    for umask in ["000", "022", "077"] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("home");
        install(&root);
        let files = [
            ".bashrc",
            ".config/agent-home/config.toml",
            ".config/agent-home/manifest.json",
        ];
        for relative in files {
            std::fs::set_permissions(root.join(relative), std::fs::Permissions::from_mode(0o640))
                .unwrap();
        }
        let out = std::process::Command::new("sh")
            .args(["-c", "umask \"$1\"; shift; exec \"$@\"", "sh", umask])
            .arg(assert_cmd::cargo::cargo_bin("vesica"))
            .args(["install", "--home-repo"])
            .arg(fixture())
            .arg("--root")
            .arg(&root)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        for relative in files {
            let expected = if relative.ends_with("manifest.json") {
                0o600
            } else {
                0o640
            };
            assert_eq!(
                std::fs::symlink_metadata(root.join(relative))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                expected,
                "{relative}/{umask}"
            );
        }
        uninstall(&root);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn ordinary_install_stale_prune_failure_restores_machine_facts_and_all_links() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::os::unix::process::CommandExt;
    // Root must not bypass the real parent-directory denial. Copy only a disposable
    // executable to an accessible mount; never alter the developer's home modes.
    let exec_root = std::env::var_os("VESICA_TEST_UNPRIV_EXEC_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let executable = tempfile::tempdir_in(exec_root).unwrap();
    std::fs::set_permissions(executable.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let binary = executable.path().join("vesica");
    std::fs::copy(assert_cmd::cargo::cargo_bin("vesica"), &binary).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let root_run = std::fs::metadata("/proc/self").unwrap().uid() == 0;
    for present in [true, false] {
        let tmp = tempfile::tempdir_in(executable.path()).unwrap();
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let home = tmp.path().join("home");
        let shared = tmp.path().join("shared");
        fn copy_tree(source: &Path, target: &Path) {
            std::fs::create_dir_all(target).unwrap();
            for entry in std::fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    copy_tree(&entry.path(), &target.join(entry.file_name()));
                } else {
                    std::fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
                }
            }
        }
        copy_tree(&fixture(), &shared);
        std::fs::create_dir(&home).unwrap();
        if root_run {
            // The fixture contains no links yet, so chown cannot touch outside nodes.
            let status = std::process::Command::new("chown")
                .args(["-R", "65534:65534"])
                .arg(&home)
                .arg(&shared)
                .status()
                .unwrap();
            assert!(status.success());
        }
        let run = |extra: &[&str]| {
            let mut cmd = std::process::Command::new(&binary);
            cmd.current_dir(tmp.path())
                .env("HOME", &home)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_TERMINAL_PROMPT", "0");
            for (key, _) in std::env::vars_os() {
                let text = key.to_string_lossy();
                if text.starts_with("VESICA_")
                    || text.starts_with("AGENT_HOME_")
                    || text.starts_with("GIT_")
                    || ["GH_TOKEN", "GITHUB_TOKEN", "ADO_PAT"].contains(&text.as_ref())
                {
                    cmd.env_remove(key);
                }
            }
            cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_TERMINAL_PROMPT", "0")
                .args(["install", "--root"])
                .arg(&home)
                .arg("--home-repo")
                .arg(&shared)
                .args(["--profile", "core"])
                .args(extra);
            if root_run {
                cmd.uid(65534).gid(65534);
            }
            cmd.output().unwrap()
        };
        let first = run(&[]);
        assert!(
            first.status.success(),
            "setup CLI must execute: {}",
            String::from_utf8_lossy(&first.stderr)
        );
        let cfg = home.join(".config/agent-home");
        let local = cfg.join("config.toml");
        if present {
            std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o640)).unwrap();
        } else {
            std::fs::remove_file(&local).unwrap();
        }
        std::fs::remove_dir_all(shared.join("Agents/Skills/demo-skill")).unwrap();
        let earlier = home.join(".agents/skills/demo-skill");
        let later = home.join(".claude/skills/demo-skill");
        assert!(earlier.is_symlink() && later.is_symlink());
        assert_eq!(
            manifest(&home)["entries"][earlier.to_str().unwrap()]["kind"],
            "symlink"
        );
        assert_eq!(
            manifest(&home)["entries"][later.to_str().unwrap()]["kind"],
            "symlink"
        );
        let parent = later.parent().unwrap();
        let old_mode = std::fs::metadata(parent).unwrap().permissions();
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o555)).unwrap();
        let before = tree_snapshot(tmp.path());
        let new_repos = tmp.path().join("new-repos");
        let failed = run(&["--repos-dir", new_repos.to_str().unwrap()]);
        let diagnostic = String::from_utf8_lossy(&failed.stderr);
        let after = tree_snapshot(tmp.path());
        // Restore fixture access before any assertion can panic and cleanup runs.
        std::fs::set_permissions(parent, old_mode).unwrap();
        assert_eq!(failed.status.code(), Some(1), "{diagnostic}");
        assert!(
            diagnostic.contains("Permission denied"),
            "must reach real prune failure: {diagnostic}"
        );
        let local_key = PathBuf::from("home/.config/agent-home/config.toml");
        assert_eq!(
            after.get(&local_key),
            before.get(&local_key),
            "failed ordinary install persisted machine facts; present={present}"
        );
        assert_eq!(after, before, "failed ordinary install must restore all managed links/files/modes and manifest; present={present}");
        let retry = run(&["--repos-dir", new_repos.to_str().unwrap()]);
        assert!(
            retry.status.success(),
            "retry: {}",
            String::from_utf8_lossy(&retry.stderr)
        );
        assert!(!earlier.is_symlink() && !later.is_symlink());
        let recorded = manifest(&home);
        assert!(recorded["entries"].get(earlier.to_str().unwrap()).is_none());
        assert!(recorded["entries"].get(later.to_str().unwrap()).is_none());
        assert!(std::fs::read_to_string(local)
            .unwrap()
            .contains(new_repos.to_str().unwrap()));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn generated_file_returned_write_error_leaves_no_partial_target_and_allows_retry() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::os::unix::process::CommandExt;
    let exec_root = std::env::var_os("VESICA_TEST_UNPRIV_EXEC_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let executable = tempfile::tempdir_in(exec_root).unwrap();
    std::fs::set_permissions(executable.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let binary = executable.path().join("vesica");
    std::fs::copy(assert_cmd::cargo::cargo_bin("vesica"), &binary).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let root_run = std::fs::metadata("/proc/self").unwrap().uid() == 0;
    fn copy_tree(source: &Path, target: &Path) {
        std::fs::create_dir_all(target).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target.join(entry.file_name()));
            } else {
                std::fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
            }
        }
    }
    let mut failures = Vec::new();
    for boundary in ["fresh", "stale-reinstall", "updater-relink"] {
        let tmp = tempfile::tempdir_in(executable.path()).unwrap();
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let home = tmp.path().join("home");
        let shared = tmp.path().join("shared");
        std::fs::create_dir(&home).unwrap();
        copy_tree(&fixture(), &shared);
        let own = || {
            if root_run {
                assert!(std::process::Command::new("chown")
                    .arg("65534:65534")
                    .arg(tmp.path())
                    .status()
                    .unwrap()
                    .success());
                assert!(std::process::Command::new("chown")
                    .args(["-R", "65534:65534"])
                    .arg(&home)
                    .arg(&shared)
                    .status()
                    .unwrap()
                    .success());
            }
        };
        own();
        let run = |limited: bool, relink: bool| {
            // Ignore SIGXFSZ so a real short write is followed by synchronous EFBIG,
            // not process termination. The limit affects only this exec'd child.
            let mut cmd = if limited {
                let mut c = std::process::Command::new("python3");
                c.args(["-c", "import os,resource,signal,sys; signal.signal(signal.SIGXFSZ,signal.SIG_IGN); resource.setrlimit(resource.RLIMIT_FSIZE,(65536,65536)); os.execv(sys.argv[1],sys.argv[1:])"]).arg(&binary);
                c
            } else {
                std::process::Command::new(&binary)
            };
            for (key, _) in std::env::vars_os() {
                let name = key.to_string_lossy();
                if name.starts_with("GIT_")
                    || name.starts_with("VESICA_")
                    || name.starts_with("AGENT_HOME_")
                    || ["GH_TOKEN", "GITHUB_TOKEN", "ADO_PAT"].contains(&name.as_ref())
                {
                    cmd.env_remove(key);
                }
            }
            cmd.env("HOME", &home)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .current_dir(tmp.path());
            if relink {
                cmd.args(["__update-relink", "--root"])
                    .arg(&home)
                    .arg("--shared-home")
                    .arg(&shared)
                    .args(["--profile", "core", "--recovery-path"])
                    .arg(tmp.path().join("transaction.json"));
            } else {
                cmd.args(["install", "--root"])
                    .arg(&home)
                    .arg("--home-repo")
                    .arg(&shared)
                    .args(["--profile", "core"]);
            }
            if root_run {
                cmd.uid(65534).gid(65534);
            }
            cmd.output().unwrap()
        };
        let role = if boundary == "fresh" {
            "code-reviewer"
        } else {
            let first = run(false, false);
            assert!(
                first.status.success(),
                "setup CLI: {}",
                String::from_utf8_lossy(&first.stderr)
            );
            std::fs::set_permissions(
                home.join(".config/agent-home/config.toml"),
                std::fs::Permissions::from_mode(0o640),
            )
            .unwrap();
            std::fs::remove_dir_all(shared.join("Agents/Skills/demo-skill")).unwrap();
            "partial-role"
        };
        let large_role = std::fs::read_to_string(shared.join("Agents/Roles/code-reviewer.md"))
            .unwrap()
            .replace("name: code-reviewer", &format!("name: {role}"))
            + &"\nOrdinary fixture prose.\n".repeat(10000);
        std::fs::write(shared.join(format!("Agents/Roles/{role}.md")), large_role).unwrap();
        own();
        let target = home.join(format!(".config/opencode/agents/ex-{role}.md"));
        assert!(!target.exists());
        let before = tree_snapshot(&home);
        let failed = run(true, boundary == "updater-relink");
        let diagnostic = String::from_utf8_lossy(&failed.stderr);
        assert_eq!(failed.status.code(), Some(1), "{boundary}: {diagnostic}");
        assert!(
            diagnostic.contains("File too large"),
            "{boundary} must reach EFBIG: {diagnostic}"
        );
        let after = tree_snapshot(&home);
        // Existing directory modes are checked, but first-install empty state dirs
        // are not described as unmanaged content or crash-atomic recovery.
        for (path, node) in &before {
            if after.get(path) != Some(node) {
                failures.push(format!(
                    "{boundary}: original node changed {}",
                    path.display()
                ));
            }
        }
        for (path, node) in &after {
            if node.0 != "directory" && before.get(path) != Some(node) {
                failures.push(format!(
                    "{boundary}: partial generated target or unowned artifact {} ({} bytes)",
                    path.display(),
                    node.1.len()
                ));
            }
        }
        if boundary == "updater-relink" {
            let journal: serde_json::Value = serde_json::from_slice(
                &std::fs::read(tmp.path().join("transaction.json")).unwrap(),
            )
            .unwrap();
            let change = journal["changes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["path"].as_str() == target.to_str())
                .unwrap();
            assert_eq!(change["before"], "Absent");
            assert!(change["after"].is_null());
        }
        let retry = run(false, false);
        if !retry.status.success() {
            failures.push(format!(
                "{boundary}: retry blocked: {}",
                String::from_utf8_lossy(&retry.stderr)
            ));
        } else {
            assert!(std::fs::metadata(&target).unwrap().len() > 65536);
            assert_eq!(
                manifest(&home)["entries"][target.to_str().unwrap()]["kind"],
                "generated-file"
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
