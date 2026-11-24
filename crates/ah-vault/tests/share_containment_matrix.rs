use ah_vault::share::{apply, plan, ShareRequest};
use std::{path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

#[cfg(unix)]
#[test]
fn inbound_apply_preflight_rejects_traversal_symlink_and_type_matrix_before_branch() {
    for case in [
        "traversal",
        "absolute",
        "leaf-live",
        "leaf-dangling",
        "directory",
        "invalid-utf8",
        "ancestor-live",
        "ancestor-dangling",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("vault");
        let shared = temp.path().join("shared");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(vault.join("Inbox")).unwrap();
        std::fs::create_dir_all(shared.join("Knowledge/Architecture")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let fm = "---\ntitle: Note\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: note\narea: test\ntags: [test]\n";
        for path in ["README.md", "Knowledge/Architecture/README.md"] {
            std::fs::write(
                shared.join(path),
                format!("{fm}owner: test\nauthority: working\n---\nShared.\n"),
            )
            .unwrap();
        }
        git(&shared, &["init", "-q", "-b", "main"]);
        git(&shared, &["config", "user.name", "Test"]);
        git(&shared, &["config", "user.email", "test@example.invalid"]);
        git(&shared, &["add", "."]);
        git(&shared, &["commit", "-qm", "baseline"]);
        let note = vault.join("Inbox/note.md");
        std::fs::write(&note, format!("{fm}---\nNote.\n")).unwrap();
        let reader = vault.join("Inbox/reader.md");
        std::fs::write(&reader, "[[note]]\n").unwrap();
        std::fs::write(outside.join("reader.md"), "[[note]]\n").unwrap();
        let request = ShareRequest {
            vault: &vault,
            shared: &shared,
            note: &note,
            destination_dir: "Knowledge/Architecture",
            owner: "test",
            drop_unresolved_links: false,
        };
        let mut planned = plan(&request).unwrap();
        let head = git(&shared, &["rev-parse", "HEAD"]);
        let index = std::fs::read(shared.join(".git/index")).unwrap();
        let reflog = std::fs::read(shared.join(".git/logs/HEAD")).unwrap();
        match case {
            "traversal" => planned.rewrites[0].file = "../outside/reader.md".into(),
            "absolute" => {
                planned.rewrites[0].file = outside.join("reader.md").to_string_lossy().into_owned()
            }
            "leaf-live" | "leaf-dangling" => {
                std::fs::remove_file(&reader).unwrap();
                std::os::unix::fs::symlink(
                    outside.join(if case == "leaf-live" {
                        "reader.md"
                    } else {
                        "missing.md"
                    }),
                    &reader,
                )
                .unwrap();
            }
            "invalid-utf8" => {
                std::fs::write(&reader, [0xff]).unwrap();
            }
            "directory" => {
                std::fs::remove_file(&reader).unwrap();
                std::fs::create_dir(&reader).unwrap();
            }
            "ancestor-live" | "ancestor-dangling" => {
                std::fs::rename(vault.join("Inbox"), vault.join("parked")).unwrap();
                std::os::unix::fs::symlink(
                    if case == "ancestor-live" {
                        outside.clone()
                    } else {
                        outside.join("missing")
                    },
                    vault.join("Inbox"),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let result = apply(&request, &planned, "share", false);
        assert!(result.is_err(), "{case} accepted");
        assert_eq!(
            std::fs::read(outside.join("reader.md")).unwrap(),
            b"[[note]]\n",
            "{case}"
        );
        assert!(!outside.join("missing.md").exists(), "{case}");
        assert_eq!(git(&shared, &["rev-parse", "HEAD"]), head, "{case}");
        assert_eq!(
            std::fs::read(shared.join(".git/logs/HEAD")).unwrap(),
            reflog,
            "{case}: branch created before preflight"
        );
        assert_eq!(
            std::fs::read(shared.join(".git/index")).unwrap(),
            index,
            "{case}: index changed (preflight was late)"
        );
        assert_eq!(
            git(&shared, &["branch", "--show-current"]),
            "main",
            "{case}"
        );
        assert!(
            git(&shared, &["branch", "--list", &planned.branch]).is_empty(),
            "{case}"
        );
        assert!(!shared.join(&planned.destination_rel).exists(), "{case}");
    }
}
