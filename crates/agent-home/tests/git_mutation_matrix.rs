//! Local-only Git selector and selected-vault acceptance fixtures (VES-BUG-005/006).
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SELECTORS: [&str; 6] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];
const NOTE: &str = "---\ntitle: Synthetic note\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: note\narea: general\ntags: [test]\n---\nSynthetic body.\n";

fn isolated(command: &mut Command, home: &Path) {
    for key in SELECTORS.into_iter().chain([
        "GIT_NAMESPACE",
        "GIT_CEILING_DIRECTORIES",
        "GIT_PREFIX",
        "GIT_SHALLOW_FILE",
        "GIT_CONFIG_COUNT",
        "VESICA_VAULT",
        "VESICA_ROOT",
        "AGENT_HOME_ROOT",
        "VESICA_HOME_REPO",
        "AGENT_HOME_REPO",
        "VESICA_TEAM_HOME",
    ]) {
        command.env_remove(key);
    }
    command
        .env("HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Vesica Matrix")
        .env("GIT_AUTHOR_EMAIL", "matrix@example.invalid")
        .env("GIT_COMMITTER_NAME", "Vesica Matrix")
        .env("GIT_COMMITTER_EMAIL", "matrix@example.invalid");
}

fn fixture_git_command(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    // Fixture setup must finish before exact-state snapshots. Detached maintenance
    // can otherwise mutate even the unrelated sentinel repository after commit.
    // These overrides apply only to setup/assertion commands, never the Vesica CLI.
    command
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .arg("-C")
        .arg(dir)
        .args(args);
    isolated(&mut command, dir);
    command
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = fixture_git_command(dir, args).output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &to.join(entry.file_name()));
        } else {
            std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Node {
    kind: &'static str,
    bytes: Vec<u8>,
    mode: u32,
}

fn tree(root: &Path) -> BTreeMap<PathBuf, Node> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Node>) {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::symlink_metadata(path).unwrap();
        let (kind, bytes) = if metadata.file_type().is_symlink() {
            (
                "symlink",
                std::fs::read_link(path)
                    .unwrap()
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec(),
            )
        } else if metadata.is_dir() {
            ("dir", Vec::new())
        } else {
            ("file", std::fs::read(path).unwrap())
        };
        result.insert(
            path.strip_prefix(root).unwrap().to_owned(),
            Node {
                kind,
                bytes,
                mode: metadata.permissions().mode(),
            },
        );
        if metadata.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), result);
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

struct Fixture {
    temp: tempfile::TempDir,
    shared: PathBuf,
    vault: PathBuf,
    other: PathBuf,
    machine: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let shared = temp.path().join("shared");
        copy_tree(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home"),
            &shared,
        );
        let vault = temp.path().join("vault");
        let other = temp.path().join("other");
        let machine = temp.path().join("machine");
        for dir in [&vault, &other, &machine] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(
            other.join("important.md"),
            "unrelated repository sentinel\n",
        )
        .unwrap();
        for dir in [&shared, &other] {
            git(dir, &["init", "-q", "-b", "main"]);
            git(dir, &["add", "-A"]);
            git(dir, &["commit", "-qm", "synthetic baseline"]);
        }
        Self {
            temp,
            shared,
            vault,
            other,
            machine,
        }
    }

    fn cli(&self, args: &[&str]) -> Command {
        let mut command = Command::new(assert_cmd::cargo::cargo_bin!("vesica"));
        command.args(args).current_dir(self.temp.path());
        isolated(&mut command, &self.machine);
        command
    }

    fn inject(&self, command: &mut Command, scenario: usize) {
        let storage = self.other.join(".git");
        let values = [
            storage.clone(),
            self.other.clone(),
            storage.join("index"),
            storage.clone(),
            storage.join("objects"),
            storage.join("objects"),
        ];
        for (index, (key, value)) in SELECTORS.into_iter().zip(values).enumerate() {
            if scenario == 6 || scenario == index {
                command.env(key, value);
            }
        }
    }

    fn remote(&self) -> (PathBuf, PathBuf) {
        let remote = self.temp.path().join("remote.git");
        let seed = self.temp.path().join("seed");
        std::fs::create_dir(&remote).unwrap();
        git(&remote, &["init", "--bare", "-q", "-b", "main"]);
        git(
            &self.shared,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(
            &self.shared,
            &["push", "-q", "-u", "origin", "HEAD:refs/heads/main"],
        );
        git(
            self.temp.path(),
            &[
                "clone",
                "-q",
                "-b",
                "main",
                remote.to_str().unwrap(),
                seed.to_str().unwrap(),
            ],
        );
        let initial = git(&self.shared, &["rev-parse", "HEAD"]);
        assert_eq!(git(&remote, &["rev-parse", "refs/heads/main"]), initial);
        assert_eq!(
            git(&self.shared, &["rev-parse", "refs/remotes/origin/main"]),
            initial
        );
        (remote, seed)
    }

    fn advance(&self, remote: &Path, seed: &Path) -> (String, String) {
        let old = git(&self.shared, &["rev-parse", "HEAD"]);
        std::fs::write(seed.join("remote-advance.txt"), "verified remote advance\n").unwrap();
        git(seed, &["add", "remote-advance.txt"]);
        git(seed, &["commit", "-qm", "remote advance"]);
        git(seed, &["push", "-q", "origin", "HEAD:refs/heads/main"]);
        let new = git(seed, &["rev-parse", "HEAD"]);
        assert_ne!(old, new);
        assert_eq!(git(remote, &["rev-parse", "refs/heads/main"]), new);
        assert_eq!(
            git(&self.shared, &["rev-parse", "refs/remotes/origin/main"]),
            old
        );
        git(seed, &["merge-base", "--is-ancestor", &old, &new]);
        (old, new)
    }
}

// Compare exact bytes, types and modes without printing private-looking byte arrays.
fn unchanged(root: &Path, before: &BTreeMap<PathBuf, Node>, label: &str) {
    let after = tree(root);
    let changed: Vec<_> = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .collect();
    assert!(changed.is_empty(), "{label}: changed paths {changed:?}");
}

fn success(output: &Output) -> String {
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn fixture_commits_do_not_spawn_background_maintenance() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    // Make automatic maintenance eligible even on a tiny fixture. Command-local
    // fixture isolation must override local configuration without rewriting it.
    git(&repo, &["config", "maintenance.auto", "true"]);
    git(&repo, &["config", "gc.auto", "1"]);
    std::fs::write(repo.join("note.md"), "Synthetic fixture.\n").unwrap();
    git(&repo, &["add", "note.md"]);
    let trace = temp.path().join("trace.jsonl");
    let output = fixture_git_command(&repo, &["commit", "-qm", "fixture baseline"])
        .env("GIT_TRACE2_EVENT", &trace)
        .output()
        .unwrap();
    success(&output);
    let events: Vec<serde_json::Value> = std::fs::read_to_string(&trace)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(events.iter().any(|event| event["event"] == "start"));
    let maintenance: Vec<_> = events
        .iter()
        .filter(|event| {
            event["event"] == "child_start"
                && event["argv"]
                    .as_array()
                    .is_some_and(|args| args.iter().any(|arg| arg == "maintenance" || arg == "gc"))
        })
        .collect();
    assert!(
        maintenance.is_empty(),
        "fixture commit launched asynchronous Git maintenance: {maintenance:?}"
    );
    assert_eq!(
        git(&repo, &["config", "--local", "maintenance.auto"]),
        "true"
    );
    assert_eq!(git(&repo, &["config", "--local", "gc.auto"]), "1");
    assert_eq!(git(&repo, &["show", "HEAD:note.md"]), "Synthetic fixture.");
}

#[test]
fn cli_snapshot_ignores_each_git_selector_and_combination() {
    for scenario in 0..7 {
        let fixture = Fixture::new();
        // init requires an empty vault, available in each independent fixture.
        success(
            &fixture
                .cli(&["vault", "init", "--path"])
                .arg(&fixture.vault)
                .output()
                .unwrap(),
        );
        std::fs::write(
            fixture.vault.join("Inbox/private.md"),
            "synthetic private snapshot\n",
        )
        .unwrap();
        let before = tree(&fixture.other);
        let mut command = fixture.cli(&["vault", "snapshot", "--path"]);
        command
            .arg(&fixture.vault)
            .args(["--message", "matrix snapshot"]);
        fixture.inject(&mut command, scenario);
        assert!(success(&command.output().unwrap()).contains("committed"));
        assert_eq!(
            git(&fixture.vault, &["show", "HEAD:Inbox/private.md"]),
            "synthetic private snapshot"
        );
        unchanged(
            &fixture.other,
            &before,
            &format!("snapshot selector {scenario}"),
        );
    }
}

#[test]
fn cli_new_skill_ignores_each_git_selector_and_combination() {
    for scenario in 0..7 {
        let fixture = Fixture::new();
        let before = tree(&fixture.other);
        let main = git(&fixture.shared, &["rev-parse", "refs/heads/main"]);
        let mut command = fixture.cli(&["new-skill", "matrix-skill", "--home-repo"]);
        command
            .arg(&fixture.shared)
            .args(["--description", "Use when testing isolation."]);
        fixture.inject(&mut command, scenario);
        assert!(success(&command.output().unwrap()).contains("skill/matrix-skill"));
        assert_eq!(
            git(&fixture.shared, &["branch", "--show-current"]),
            "skill/matrix-skill"
        );
        assert_eq!(
            git(&fixture.shared, &["rev-parse", "refs/heads/main"]),
            main
        );
        assert!(!git(
            &fixture.shared,
            &["show", "HEAD:Agents/Skills/matrix-skill/SKILL.md"]
        )
        .is_empty());
        assert_eq!(git(&fixture.shared, &["status", "--porcelain"]), "");
        unchanged(
            &fixture.other,
            &before,
            &format!("new-skill selector {scenario}"),
        );
    }
}

#[test]
fn cli_team_status_and_pull_ignore_each_git_selector_and_combination() {
    for scenario in 0..7 {
        let fixture = Fixture::new();
        let (remote, seed) = fixture.remote();
        let (old, new) = fixture.advance(&remote, &seed);
        let before = tree(&fixture.other);
        let shared_before = tree(&fixture.shared);
        let remote_before = tree(&remote);
        let mut status = fixture.cli(&["team", "status", "--path"]);
        status.arg(&fixture.shared);
        fixture.inject(&mut status, scenario);
        assert!(success(&status.output().unwrap()).contains("origin/main"));
        unchanged(&fixture.shared, &shared_before, "team status is read-only");
        let mut pull = fixture.cli(&["team", "pull", "--path"]);
        pull.arg(&fixture.shared);
        fixture.inject(&mut pull, scenario);
        let stdout = success(&pull.output().unwrap());
        assert!(stdout.contains(&old) && stdout.contains(&new), "{stdout}");
        assert_eq!(git(&fixture.shared, &["rev-parse", "HEAD"]), new);
        assert_eq!(
            git(&fixture.shared, &["rev-parse", "refs/remotes/origin/main"]),
            new
        );
        assert_eq!(
            std::fs::read(fixture.shared.join("remote-advance.txt")).unwrap(),
            b"verified remote advance\n"
        );
        unchanged(
            &fixture.other,
            &before,
            &format!("team selector {scenario}"),
        );
        unchanged(&remote, &remote_before, "pull does not change bare remote");
    }
}

#[test]
fn cli_share_and_open_pr_push_ignore_each_git_selector_and_combination() {
    // The actual --open-pr implementation only pushes and prints a gh instruction;
    // it does not launch gh. Every remote here is an independent local bare fixture.
    for push in [false, true] {
        for scenario in 0..7 {
            let fixture = Fixture::new();
            let remote = push.then(|| fixture.remote().0);
            std::fs::write(fixture.vault.join("note.md"), NOTE).unwrap();
            std::fs::write(fixture.vault.join("pointer.md"), "See [[note]].\n").unwrap();
            let before = tree(&fixture.other);
            let main = git(&fixture.shared, &["rev-parse", "refs/heads/main"]);
            let mut command = fixture.cli(&["share"]);
            command
                .arg(fixture.vault.join("note.md"))
                .arg("--home-repo")
                .arg(&fixture.shared)
                .arg("--root")
                .arg(&fixture.machine)
                .arg("--vault")
                .arg(&fixture.vault)
                .args(["--area", "Architecture", "--yes"]);
            if push {
                command.arg("--open-pr");
            }
            fixture.inject(&mut command, scenario);
            let stdout = success(&command.output().unwrap());
            assert!(stdout.contains("Committed"), "{stdout}");
            assert_eq!(
                git(&fixture.shared, &["branch", "--show-current"]),
                "share/note"
            );
            assert_eq!(
                git(&fixture.shared, &["rev-parse", "refs/heads/main"]),
                main
            );
            assert!(git(
                &fixture.shared,
                &["show", "HEAD:Knowledge/Architecture/note.md"]
            )
            .contains("owner: Vesica Matrix"));
            assert_eq!(
                std::fs::read(fixture.vault.join("pointer.md")).unwrap(),
                b"See [[Team/Knowledge/Architecture/note]].\n"
            );
            if let Some(remote) = remote {
                assert!(stdout.contains("Pushed share/note"));
                assert_eq!(
                    git(&remote, &["rev-parse", "refs/heads/share/note"]),
                    git(&fixture.shared, &["rev-parse", "HEAD"])
                );
                assert_eq!(git(&remote, &["rev-parse", "refs/heads/main"]), main);
            } else {
                assert!(stdout.contains("not pushed"));
                assert_eq!(git(&fixture.shared, &["remote"]), "");
            }
            unchanged(
                &fixture.other,
                &before,
                &format!("share push={push} selector {scenario}"),
            );
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum VaultSelection {
    Configured,
    Environment,
    Explicit,
    Absent,
}

impl Fixture {
    fn selected_share(&self, selection: VaultSelection, source: &Path, apply: bool) -> Command {
        let config = self.machine.join(".config/agent-home");
        std::fs::create_dir_all(&config).unwrap();
        let config_vault = if matches!(selection, VaultSelection::Configured) {
            self.vault.clone()
        } else {
            self.temp.path().join("configured-decoy")
        };
        if !matches!(selection, VaultSelection::Absent) {
            std::fs::write(
                config.join("config.toml"),
                format!("schema = 1\nvault = {:?}\n", config_vault.to_str().unwrap()),
            )
            .unwrap();
        }
        let mut command = self.cli(&["share"]);
        command
            .arg(source)
            .arg("--root")
            .arg(&self.machine)
            .arg("--home-repo")
            .arg(&self.shared)
            .args(["--area", "Architecture"]);
        match selection {
            VaultSelection::Environment => {
                command.env("VESICA_VAULT", &self.vault);
            }
            VaultSelection::Explicit => {
                command.env("VESICA_VAULT", self.temp.path().join("environment-decoy"));
                command.arg("--vault").arg(&self.vault);
            }
            _ => {}
        }
        if apply {
            command.arg("--yes");
        } else {
            command.arg("--dry-run");
        }
        command
    }

    fn selected_notes(&self) {
        for root in [
            &self.vault,
            &self.temp.path().join("configured-decoy"),
            &self.temp.path().join("environment-decoy"),
            &self.machine.join("unrelated-project"),
        ] {
            std::fs::create_dir_all(root.join("Inbox")).unwrap();
            std::fs::write(root.join("Inbox/note.md"), NOTE).unwrap();
            std::fs::write(
                root.join("Inbox/pointer.md"),
                "See [[Inbox/note]] and [[note|named]].\n",
            )
            .unwrap();
            std::fs::write(
                root.join("Inbox/unrelated.md"),
                "Unrelated [[elsewhere]] bytes\r\n",
            )
            .unwrap();
        }
        // Give the selected source observable content different from every decoy.
        std::fs::write(
            self.vault.join("Inbox/note.md"),
            NOTE.replace("Synthetic body.", "Selected vault body."),
        )
        .unwrap();
    }
}

#[test]
fn cli_share_plan_and_apply_use_identical_selected_vault_without_unrelated_writes() {
    for selection in [
        VaultSelection::Configured,
        VaultSelection::Environment,
        VaultSelection::Explicit,
    ] {
        let fixture = Fixture::new();
        fixture.selected_notes();
        let source = fixture.vault.join("Inbox/note.md");
        // Construct once before snapshotting so local config persistence is fixture setup.
        let mut plan = fixture.selected_share(selection, &source, false);
        let vault_before = tree(&fixture.vault);
        let machine_before = tree(&fixture.machine);
        let config_decoy_before = tree(&fixture.temp.path().join("configured-decoy"));
        let env_decoy_before = tree(&fixture.temp.path().join("environment-decoy"));
        let shared_before = tree(&fixture.shared);
        let plan_stdout = success(&plan.output().unwrap());
        assert!(
            plan_stdout.contains("Would create: Inbox/note.md -> Knowledge/Architecture/note.md"),
            "{plan_stdout}"
        );
        assert!(
            plan_stdout.contains(
                "Inbox/pointer.md: [[Inbox/note]] -> [[Team/Knowledge/Architecture/note]]"
            ),
            "{plan_stdout}"
        );
        unchanged(
            &fixture.vault,
            &vault_before,
            "plan selected vault read-only",
        );
        unchanged(
            &fixture.shared,
            &shared_before,
            "plan no branch/reflog/index/object writes",
        );
        let apply_stdout = success(
            &fixture
                .selected_share(selection, &source, true)
                .output()
                .unwrap(),
        );
        assert!(
            apply_stdout.contains("Promoting: Inbox/note.md -> Knowledge/Architecture/note.md"),
            "{apply_stdout}"
        );
        let planned_rewrites: Vec<_> = plan_stdout
            .lines()
            .filter(|line| line.starts_with("  Inbox/"))
            .collect();
        let applied_rewrites: Vec<_> = apply_stdout
            .lines()
            .filter(|line| line.starts_with("  Inbox/"))
            .collect();
        assert_eq!(
            planned_rewrites, applied_rewrites,
            "plan/apply resolved different inbound files"
        );
        assert_eq!(std::fs::read(fixture.vault.join("Inbox/pointer.md")).unwrap(), b"See [[Team/Knowledge/Architecture/note]] and [[Team/Knowledge/Architecture/note|named]].\n");
        let mut expected_vault = vault_before;
        expected_vault.get_mut(Path::new("Inbox/pointer.md")).unwrap().bytes = b"See [[Team/Knowledge/Architecture/note]] and [[Team/Knowledge/Architecture/note|named]].\n".to_vec();
        unchanged(
            &fixture.vault,
            &expected_vault,
            "only selected inbound rewrite changed",
        );
        let promoted = git(
            &fixture.shared,
            &["show", "HEAD:Knowledge/Architecture/note.md"],
        );
        assert!(
            promoted.contains("Selected vault body.") && !promoted.contains("Synthetic body."),
            "source was not selected vault"
        );
        let mut shared_content = tree(&fixture.shared);
        shared_content.retain(|path, _| {
            !path.starts_with(".git") && path != Path::new("Knowledge/Architecture/note.md")
        });
        let mut expected_shared = shared_before;
        expected_shared.retain(|path, _| !path.starts_with(".git"));
        assert_eq!(
            shared_content, expected_shared,
            "unrelated shared-home content changed"
        );
        unchanged(
            &fixture.machine,
            &machine_before,
            "machine-root Markdown/config invariant",
        );
        unchanged(
            &fixture.temp.path().join("configured-decoy"),
            &config_decoy_before,
            "unselected configured vault invariant",
        );
        unchanged(
            &fixture.temp.path().join("environment-decoy"),
            &env_decoy_before,
            "unselected environment vault invariant",
        );
    }
}

#[test]
fn cli_share_outside_selected_vault_refuses_before_branch_for_every_selection() {
    for selection in [
        VaultSelection::Configured,
        VaultSelection::Environment,
        VaultSelection::Explicit,
    ] {
        for apply in [false, true] {
            let fixture = Fixture::new();
            fixture.selected_notes();
            let source = fixture.machine.join("unrelated-project/Inbox/note.md");
            let mut command = fixture.selected_share(selection, &source, apply);
            let shared_before = tree(&fixture.shared);
            let machine_before = tree(&fixture.machine);
            let vault_before = tree(&fixture.vault);
            let output = command.output().unwrap();
            assert!(
                !output.status.success(),
                "outside source accepted: {selection:?} apply={apply}"
            );
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("outside the vault"),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            unchanged(
                &fixture.shared,
                &shared_before,
                "outside refusal precedes branch/reflog/index/objects",
            );
            unchanged(
                &fixture.machine,
                &machine_before,
                "outside source and inbound Markdown unchanged",
            );
            unchanged(
                &fixture.vault,
                &vault_before,
                "selected vault untouched on refusal",
            );
        }
    }
}

#[test]
fn cli_share_absent_vault_refuses_plan_and_apply_without_mutation() {
    for apply in [false, true] {
        let fixture = Fixture::new();
        fixture.selected_notes();
        let mut command = fixture.selected_share(
            VaultSelection::Absent,
            &fixture.machine.join("unrelated-project/Inbox/note.md"),
            apply,
        );
        let shared_before = tree(&fixture.shared);
        let machine_before = tree(&fixture.machine);
        let vault_before = tree(&fixture.vault);
        let output = command.output().unwrap();
        assert!(
            !output.status.success(),
            "absent vault accepted apply={apply}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("no personal vault configured"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        unchanged(
            &fixture.shared,
            &shared_before,
            "absent refusal before branch/reflog",
        );
        unchanged(
            &fixture.machine,
            &machine_before,
            "absent machine root unchanged",
        );
        unchanged(&fixture.vault, &vault_before, "absent fallback not used");
    }
}

#[test]
fn cli_lint_accepts_noncanonical_selected_shared_path() {
    let fixture = Fixture::new();
    let path = fixture.shared.join("../shared");
    let before = tree(&fixture.shared);
    let output = fixture
        .cli(&["lint", "--home-repo"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(success(&output).contains("No lint findings"));
    unchanged(
        &fixture.shared,
        &before,
        "lint does not mutate noncanonical selected checkout",
    );
}

#[test]
fn team_pull_refuses_symlinked_git_storage_before_fetch_or_mutation() {
    let fixture = Fixture::new();
    let (remote, seed) = fixture.remote();
    fixture.advance(&remote, &seed);
    let external_git = fixture.temp.path().join("external.git");
    std::fs::rename(fixture.shared.join(".git"), &external_git).unwrap();
    std::os::unix::fs::symlink(&external_git, fixture.shared.join(".git")).unwrap();
    let shared_before = tree(&fixture.shared);
    let storage_before = tree(&external_git);
    let remote_before = tree(&remote);
    let output = fixture
        .cli(&["team", "pull", "--path"])
        .arg(&fixture.shared)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "team pull accepted external symlink Git storage"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("in-root .git"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    unchanged(
        &fixture.shared,
        &shared_before,
        "symlink Git worktree unchanged",
    );
    unchanged(
        &external_git,
        &storage_before,
        "symlink Git referent unchanged before fetch",
    );
    unchanged(
        &remote,
        &remote_before,
        "symlink refusal bare remote unchanged",
    );
}

#[test]
fn team_pull_refuses_linked_worktree_before_fetch_or_mutation() {
    let fixture = Fixture::new();
    let (remote, seed) = fixture.remote();
    let linked = fixture.temp.path().join("linked");
    git(
        &fixture.shared,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            linked.to_str().unwrap(),
        ],
    );
    git(
        &linked,
        &["branch", "--set-upstream-to", "origin/main", "linked"],
    );
    fixture.advance(&remote, &seed);
    let primary_before = tree(&fixture.shared);
    let linked_before = tree(&linked);
    let remote_before = tree(&remote);
    let output = fixture
        .cli(&["team", "pull", "--path"])
        .arg(&linked)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "team pull accepted linked-worktree mutation: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("in-root .git"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        tree(&fixture.shared),
        primary_before,
        "primary metadata mutated before refusal"
    );
    assert_eq!(
        tree(&linked),
        linked_before,
        "linked tree mutated before refusal"
    );
    assert_eq!(
        tree(&remote),
        remote_before,
        "remote mutated before refusal"
    );
}
