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
    for relative in [
        ".bashrc",
        ".claude/CLAUDE.md",
        ".codex/AGENTS.md",
        ".config/opencode/opencode.json",
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
            uninstall(&root);
            uninstall(&root);
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
