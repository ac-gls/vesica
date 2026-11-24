use ah_vault::personal::{fix, init, LedgerNote};
#[cfg(unix)]
#[test]
fn ledger_parent_live_dangling_type_matrix_and_normal_exclusive_creation() {
    for parent in ["Sessions", "Sessions/agent-runs"] {
        for kind in ["live", "dangling", "file"] {
            for operation in ["fix", "new-note"] {
                let temp = tempfile::tempdir().unwrap();
                let vault = temp.path().join("vault");
                let outside = temp.path().join("outside");
                init(&vault, "2026-11-23", false).unwrap();
                std::fs::create_dir(&outside).unwrap();
                std::fs::remove_dir_all(vault.join(parent)).unwrap();
                if kind == "file" {
                    std::fs::write(vault.join(parent), "obstruction").unwrap();
                } else {
                    std::os::unix::fs::symlink(
                        if kind == "live" {
                            outside.clone()
                        } else {
                            outside.join("missing")
                        },
                        vault.join(parent),
                    )
                    .unwrap();
                }
                let note = LedgerNote {
                    task: "task",
                    agent: "agent",
                    date: "2026-11-23",
                    area: "test",
                    title: None,
                };
                let result = if operation == "fix" {
                    fix(&vault, "2026-11-23", false).map(|_| ())
                } else {
                    ah_vault::personal::new_note(&vault, &note).map(|_| ())
                };
                assert!(result.is_err(), "{operation} {parent} {kind}");
                assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
            }
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    init(&vault, "2026-11-23", false).unwrap();
    let note = LedgerNote {
        task: "task",
        agent: "agent",
        date: "2026-11-23",
        area: "test",
        title: None,
    };
    let path = ah_vault::personal::new_note(&vault, &note).unwrap();
    let before = std::fs::read(vault.join(&path)).unwrap();
    assert!(ah_vault::personal::new_note(&vault, &note).is_err());
    assert_eq!(std::fs::read(vault.join(path)).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn all_personal_writers_reject_live_and_dangling_root_ancestors() {
    for operation in ["init", "fix", "new-note"] {
        for dangling in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let outside = temp.path().join("outside");
            std::fs::create_dir(&outside).unwrap();
            if operation != "init" {
                std::fs::create_dir(outside.join("vault")).unwrap();
            }
            let alias = temp.path().join("alias");
            std::os::unix::fs::symlink(
                if dangling {
                    outside.join("missing")
                } else {
                    outside.clone()
                },
                &alias,
            )
            .unwrap();
            let vault = alias.join("vault");
            let note = LedgerNote {
                task: "task",
                agent: "agent",
                date: "2026-11-23",
                area: "test",
                title: None,
            };
            let result = match operation {
                "init" => init(&vault, "2026-11-23", false).map(|_| ()),
                "fix" => fix(&vault, "2026-11-23", false).map(|_| ()),
                _ => ah_vault::personal::new_note(&vault, &note).map(|_| ()),
            };
            assert!(
                result.is_err(),
                "{operation}: dangling={dangling} followed root ancestor"
            );
            assert!(!outside.join("vault/AGENTS.md").exists());
            assert!(!outside.join("vault/Sessions").exists());
            assert!(!outside.join("missing").exists());
        }
    }
}

#[cfg(unix)]
#[test]
fn init_refuses_a_symlinked_vault_root() {
    let temp = tempfile::tempdir().unwrap();
    let outside = temp.path().join("outside");
    let vault = temp.path().join("Personal");
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, &vault).unwrap();

    let result = init(&vault, "2026-11-23", false);

    assert!(result.is_err(), "init followed a symlinked vault root");
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn fix_refuses_to_create_skeleton_files_through_symlinked_targets() {
    for file in ["AGENTS.md", "CLAUDE.md", "README.md", ".gitattributes"] {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("Personal");
        let outside = temp.path().join("outside-file");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(&outside, "external content\n").unwrap();
        std::os::unix::fs::symlink(&outside, vault.join(file)).unwrap();

        let result = fix(&vault, "2026-11-23", false);

        assert!(result.is_err(), "fix accepted symlinked {file}");
        assert_eq!(std::fs::read(&outside).unwrap(), b"external content\n");
    }
}

#[cfg(unix)]
#[test]
fn fix_refuses_to_create_ledger_directories_through_symlinked_parent() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("Personal");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, vault.join("Sessions")).unwrap();

    let result = fix(&vault, "2026-11-23", false);

    assert!(result.is_err(), "fix followed a symlinked vault ancestor");
    assert!(!outside.join("agent-runs").exists());
}

#[cfg(unix)]
#[test]
fn fix_refuses_to_append_through_symlinked_gitignore() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("Personal");
    let outside = temp.path().join("outside-gitignore");
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::write(&outside, "# externally owned\n").unwrap();
    std::os::unix::fs::symlink(&outside, vault.join(".gitignore")).unwrap();

    let result = fix(&vault, "2026-11-23", false);

    assert!(result.is_err(), "fix followed a symlinked .gitignore");
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap(),
        "# externally owned\n"
    );
}

#[cfg(unix)]
#[test]
fn new_note_refuses_to_create_through_a_symlinked_ledger_parent() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("Personal");
    let outside = temp.path().join("outside");
    init(&vault, "2026-11-23", false).unwrap();
    std::fs::remove_dir_all(vault.join("Sessions")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, vault.join("Sessions")).unwrap();
    let note = LedgerNote {
        task: "task",
        agent: "agent",
        date: "2026-11-23",
        area: "test",
        title: None,
    };

    let result = ah_vault::personal::new_note(&vault, &note);

    assert!(result.is_err());
    assert!(!outside.join("agent-runs").exists());
}
