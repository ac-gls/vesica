use ah_vault::personal::{init, snapshot, SnapshotOutcome};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(unix)]
#[test]
fn snapshot_rejects_symlinked_git_directory_before_other_repository_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    let other = temp.path().join("other");
    std::fs::create_dir(&vault).unwrap();
    std::fs::create_dir(&other).unwrap();
    git(&other, ["init", "-q", "-b", "main"]);
    git(&other, ["config", "user.name", "Test"]);
    git(&other, ["config", "user.email", "test@example.invalid"]);
    std::fs::write(other.join("important.md"), "unchanged").unwrap();
    git(&other, ["add", "."]);
    git(&other, ["commit", "-qm", "baseline"]);
    std::fs::write(vault.join(".gitignore"), "/Team\n").unwrap();
    std::fs::write(vault.join("private.md"), "synthetic private").unwrap();
    std::os::unix::fs::symlink(other.join(".git"), vault.join(".git")).unwrap();
    let metadata = tree_fingerprint(&other.join(".git"));
    let result = snapshot(&vault, "unsafe");
    assert!(
        result.is_err(),
        "snapshot accepted foreign symlinked Git storage"
    );
    assert_eq!(tree_fingerprint(&other.join(".git")), metadata);
    assert_eq!(
        std::fs::read(other.join("important.md")).unwrap(),
        b"unchanged"
    );
}

#[test]
fn linked_worktree_read_only_root_and_status_are_supported_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let primary = temp.path().join("primary");
    let linked = temp.path().join("linked");
    std::fs::create_dir(&primary).unwrap();
    git(&primary, ["init", "-q", "-b", "main"]);
    git(&primary, ["config", "user.name", "Test"]);
    git(&primary, ["config", "user.email", "test@example.invalid"]);
    std::fs::write(primary.join("note.md"), "baseline").unwrap();
    git(&primary, ["add", "."]);
    git(&primary, ["commit", "-qm", "baseline"]);
    git(
        &primary,
        [
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            linked.to_str().unwrap(),
        ],
    );
    let metadata = tree_fingerprint(&primary.join(".git"));
    let worktree = tree_fingerprint(&linked);
    assert!(linked.join(".git").is_file());
    assert!(ah_vault::git::is_repository(&linked));
    assert!(ah_vault::git::validate_mutation_repository(&linked).is_err());
    assert!(
        ah_vault::git::user(&linked, &["add", "."]).is_err(),
        "linked worktree mutation should refuse before touching common metadata"
    );
    assert!(!ah_vault::git::is_repository(&linked.join("missing")));
    assert_eq!(
        ah_vault::git::isolated(&linked, &["status", "--porcelain"]).unwrap(),
        ""
    );
    assert_eq!(tree_fingerprint(&primary.join(".git")), metadata);
    assert_eq!(tree_fingerprint(&linked), worktree);
}

#[test]
fn hostile_git_selectors_cannot_redirect_vault_snapshot() {
    if std::env::var_os("VESICA_GIT_TEST_CHILD").is_some() {
        let vault = PathBuf::from(std::env::var_os("VESICA_VAULT_TEST_PATH").unwrap());
        let result = snapshot(&vault, "synthetic vault snapshot").unwrap();
        assert!(matches!(result, SnapshotOutcome::Committed { .. }));
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("Personal");
    let other = temp.path().join("Other");
    std::fs::create_dir_all(&other).unwrap();
    init(&vault, "2026-11-23", false).unwrap();
    std::fs::create_dir_all(vault.join("Knowledge/Architecture")).unwrap();
    std::fs::write(
        vault.join("Knowledge/Architecture/README.md"),
        "---\ntitle: Architecture\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: readme\narea: team\ntags: [test]\nowner: test\nauthority: working\n---\nArchitecture.\n",
    )
    .unwrap();
    std::fs::write(vault.join("Inbox/private.md"), "synthetic private note\n").unwrap();

    git(&other, ["init", "-q", "-b", "main"]);
    git(&other, ["config", "user.name", "Vesica Test"]);
    git(&other, ["config", "user.email", "vesica@example.invalid"]);
    std::fs::write(other.join(".gitignore"), "/Team\n").unwrap();
    std::fs::write(
        other.join("important.md"),
        "must remain in the other repository\n",
    )
    .unwrap();
    git(&other, ["add", "."]);
    git(&other, ["commit", "-q", "-m", "other repository baseline"]);

    let other_git = other.join(".git");
    let selectors = [
        ("GIT_DIR", other_git.clone()),
        ("GIT_WORK_TREE", other.clone()),
        ("GIT_INDEX_FILE", other_git.join("index")),
        ("GIT_COMMON_DIR", other_git.clone()),
        ("GIT_OBJECT_DIRECTORY", other_git.join("objects")),
        (
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            other_git.join("objects"),
        ),
    ];
    let scenarios: Vec<(&str, Vec<(&str, PathBuf)>)> = selectors
        .iter()
        .map(|(name, value)| (*name, vec![(*name, value.clone())]))
        .chain(std::iter::once(("combined", selectors.to_vec())))
        .collect();

    for (scenario, injected) in scenarios {
        std::fs::write(
            vault.join(format!("Inbox/{scenario}.md")),
            format!("synthetic private note for {scenario}\n"),
        )
        .unwrap();
        let head_before = git_text(&other, ["rev-parse", "HEAD"]);
        let index_before = std::fs::read(other_git.join("index")).unwrap();
        let objects_before = tree_fingerprint(&other_git.join("objects"));
        let important_before = std::fs::read(other.join("important.md")).unwrap();

        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "hostile_git_selectors_cannot_redirect_vault_snapshot",
                "--nocapture",
            ])
            .env("VESICA_GIT_TEST_CHILD", "1")
            .env("VESICA_VAULT_TEST_PATH", &vault)
            .env("VESICA_GIT_SELECTOR_CASE", scenario)
            .env("GIT_AUTHOR_NAME", "Vesica Test")
            .env("GIT_AUTHOR_EMAIL", "vesica@example.invalid")
            .env("GIT_COMMITTER_NAME", "Vesica Test")
            .env("GIT_COMMITTER_EMAIL", "vesica@example.invalid");
        for key in selector_names() {
            child.env_remove(key);
        }
        for (key, value) in injected {
            child.env(key, value);
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "snapshot subprocess failed for {scenario}: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert_eq!(
            git_text(&other, ["rev-parse", "HEAD"]),
            head_before,
            "{scenario}: snapshot must not change unrelated repository HEAD"
        );
        assert_eq!(
            std::fs::read(other_git.join("index")).unwrap(),
            index_before,
            "{scenario}: snapshot must not change unrelated repository index"
        );
        assert_eq!(
            tree_fingerprint(&other_git.join("objects")),
            objects_before,
            "{scenario}: snapshot must not add or alter unrelated repository objects"
        );
        assert_eq!(
            std::fs::read(other.join("important.md")).unwrap(),
            important_before,
            "{scenario}: snapshot must not change unrelated worktree files"
        );
        assert!(!git_text(&vault, ["rev-parse", "--verify", "HEAD"]).is_empty());
    }
}

#[test]
fn hostile_git_selectors_cannot_redirect_share_mutations() {
    if std::env::var_os("VESICA_SHARE_TEST_CHILD").is_some() {
        let vault = PathBuf::from(std::env::var_os("VESICA_SHARE_VAULT").unwrap());
        let shared = PathBuf::from(std::env::var_os("VESICA_SHARE_REPO").unwrap());
        let note = vault.join("Inbox/note.md");
        let request = ah_vault::share::ShareRequest {
            vault: &vault,
            shared: &shared,
            note: &note,
            destination_dir: "Knowledge/Architecture",
            owner: "test",
            drop_unresolved_links: false,
        };
        let plan = ah_vault::share::plan(&request).unwrap();
        ah_vault::share::apply(&request, &plan, "synthetic share", false).unwrap();
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("Personal");
    let shared = temp.path().join("Shared");
    let other = temp.path().join("Other");
    std::fs::create_dir_all(vault.join("Inbox")).unwrap();
    std::fs::create_dir_all(shared.join("Knowledge/Architecture")).unwrap();
    std::fs::write(
        shared.join("Knowledge/Architecture/README.md"),
        "---\ntitle: Architecture\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: readme\narea: team\ntags: [test]\nowner: test\nauthority: working\n---\nArchitecture.\n",
    )
    .unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(
        vault.join("Inbox/note.md"),
        "---\ntitle: Note\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: note\narea: test\ntags: [test]\n---\nSynthetic note.\n",
    )
    .unwrap();
    std::fs::write(shared.join("README.md"), "Shared.\n").unwrap();
    git(&shared, ["init", "-q", "-b", "main"]);
    git(&shared, ["config", "user.name", "Vesica Test"]);
    git(&shared, ["config", "user.email", "vesica@example.invalid"]);
    git(&shared, ["add", "."]);
    git(&shared, ["commit", "-q", "-m", "shared baseline"]);
    git(&other, ["init", "-q", "-b", "main"]);
    git(&other, ["config", "user.name", "Vesica Test"]);
    git(&other, ["config", "user.email", "vesica@example.invalid"]);
    std::fs::write(other.join("important.md"), "untouched\n").unwrap();
    git(&other, ["add", "."]);
    git(&other, ["commit", "-q", "-m", "other baseline"]);

    let other_git = other.join(".git");
    let head_before = git_text(&other, ["rev-parse", "HEAD"]);
    let index_before = std::fs::read(other_git.join("index")).unwrap();
    let objects_before = tree_fingerprint(&other_git.join("objects"));
    let worktree_before = std::fs::read(other.join("important.md")).unwrap();
    let selectors = [
        ("GIT_DIR", other_git.clone()),
        ("GIT_WORK_TREE", other.clone()),
        ("GIT_INDEX_FILE", other_git.join("index")),
        ("GIT_COMMON_DIR", other_git.clone()),
        ("GIT_OBJECT_DIRECTORY", other_git.join("objects")),
        (
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            other_git.join("objects"),
        ),
    ];
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args([
            "--exact",
            "hostile_git_selectors_cannot_redirect_share_mutations",
            "--nocapture",
        ])
        .env("VESICA_SHARE_TEST_CHILD", "1")
        .env("VESICA_SHARE_VAULT", &vault)
        .env("VESICA_SHARE_REPO", &shared)
        .env("GIT_AUTHOR_NAME", "Vesica Test")
        .env("GIT_AUTHOR_EMAIL", "vesica@example.invalid")
        .env("GIT_COMMITTER_NAME", "Vesica Test")
        .env("GIT_COMMITTER_EMAIL", "vesica@example.invalid");
    for key in selector_names() {
        child.env_remove(key);
    }
    for (key, value) in selectors {
        child.env(key, value);
    }
    let output = child.output().unwrap();
    assert!(
        output.status.success(),
        "share subprocess failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(git_text(&other, ["rev-parse", "HEAD"]), head_before);
    assert_eq!(
        std::fs::read(other_git.join("index")).unwrap(),
        index_before
    );
    assert_eq!(tree_fingerprint(&other_git.join("objects")), objects_before);
    assert_eq!(
        std::fs::read(other.join("important.md")).unwrap(),
        worktree_before
    );
    assert!(!git_text(&shared, ["branch", "--show-current"]).is_empty());
}

fn selector_names() -> [&'static str; 6] {
    [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ]
}

fn tree_fingerprint(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, dir: &Path, result: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                visit(root, &path, result);
            } else {
                result.push((
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    std::fs::read(path).unwrap(),
                ));
            }
        }
    }

    let mut result = Vec::new();
    visit(root, root, &mut result);
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

fn git<I, S>(cwd: &Path, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_text<I, S>(cwd: &Path, args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
