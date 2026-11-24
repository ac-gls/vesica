use ah_vault::share::{apply, plan, ShareRequest};
use std::path::{Path, PathBuf};
use std::process::Command;

struct ShareFixture {
    _temp: tempfile::TempDir,
    vault: PathBuf,
    shared: PathBuf,
    note: PathBuf,
}

impl ShareFixture {
    fn new(body: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("Personal");
        let shared = temp.path().join("Shared");
        std::fs::create_dir_all(vault.join("Inbox")).unwrap();
        std::fs::create_dir_all(shared.join("Knowledge/Architecture")).unwrap();
        std::fs::write(shared.join("README.md"), "shared\n").unwrap();
        git(&shared, ["init", "-q", "-b", "main"]);
        git(&shared, ["config", "user.name", "Vesica Test"]);
        git(&shared, ["config", "user.email", "vesica@example.invalid"]);
        git(&shared, ["add", "."]);
        git(&shared, ["commit", "-q", "-m", "initial"]);
        let note = vault.join("Inbox/note.md");
        std::fs::write(
            &note,
            format!(
                "---\ntitle: Note\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: note\narea: test\ntags: [test]\n---\n{body}"
            ),
        )
        .unwrap();
        Self {
            _temp: temp,
            vault,
            shared,
            note,
        }
    }

    fn request<'a>(&'a self, destination_dir: &'a str) -> ShareRequest<'a> {
        ShareRequest {
            vault: &self.vault,
            shared: &self.shared,
            note: &self.note,
            destination_dir,
            owner: "test",
            drop_unresolved_links: false,
        }
    }
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

#[test]
fn share_plan_rejects_destination_directory_outside_shared_home() {
    let fixture = ShareFixture::new("A note.\n");
    let outside = fixture._temp.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();

    let result = plan(&fixture.request("../outside"));

    assert!(result.is_err(), "share destination escaped the shared home");
    assert!(!outside.join("note.md").exists());
}

#[cfg(unix)]
#[test]
fn share_plan_rejects_a_note_symlink_that_escapes_the_personal_vault() {
    let fixture = ShareFixture::new("A note.\n");
    let outside = fixture._temp.path().join("outside-note.md");
    std::fs::write(&outside, "private\n").unwrap();
    let escaped = fixture.vault.join("Inbox/escaped.md");
    std::os::unix::fs::symlink(&outside, &escaped).unwrap();
    let mut request = fixture.request("Knowledge/Architecture");
    request.note = &escaped;

    let result = plan(&request);

    assert!(
        result.is_err(),
        "an escaping vault symlink was shareable: {result:?}"
    );
}

#[test]
fn share_link_resolver_does_not_accept_a_target_outside_shared_home() {
    let fixture = ShareFixture::new("See [[../../../outside-note]].\n");
    std::fs::write(fixture._temp.path().join("outside-note.md"), "private\n").unwrap();

    let result = plan(&fixture.request("Knowledge/Architecture"));

    assert!(
        result.is_err(),
        "link to a note outside the shared home resolved"
    );
}

#[cfg(unix)]
#[test]
fn share_link_resolver_does_not_follow_a_shared_home_symlink_outside() {
    let fixture = ShareFixture::new("See [[private-note]].\n");
    let outside = fixture._temp.path().join("outside-note.md");
    std::fs::write(
        &outside, "private
",
    )
    .unwrap();
    std::os::unix::fs::symlink(&outside, fixture.shared.join("private-note.md")).unwrap();
    git(&fixture.shared, ["add", "private-note.md"]);
    git(&fixture.shared, ["commit", "-q", "-m", "seed link target"]);

    let result = plan(&fixture.request("Knowledge/Architecture"));

    assert!(
        result.is_err(),
        "link resolver accepted an escaping symlink"
    );
}

#[test]
fn failed_share_gate_restores_original_branch_and_removes_created_artifacts() {
    let fixture = ShareFixture::new("A note.\n");
    std::fs::write(
        fixture.shared.join("AGENTS.md"),
        format!("leak {}\n", "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ123456"),
    )
    .unwrap();
    git(&fixture.shared, ["add", "AGENTS.md"]);
    git(&fixture.shared, ["commit", "-q", "-m", "seed scan finding"]);
    let original_branch = "main";
    let request = fixture.request("Knowledge/Architecture");
    let planned = plan(&request).unwrap();

    let result = apply(&request, &planned, "share note", false);

    assert!(result.is_err());
    assert_eq!(
        git_text(&fixture.shared, ["branch", "--show-current"]),
        original_branch
    );
    assert!(!fixture.shared.join(&planned.destination_rel).exists());
    assert!(git_text(&fixture.shared, ["branch", "--list", &planned.branch]).is_empty());
    assert_eq!(git_text(&fixture.shared, ["status", "--porcelain"]), "");
    assert!(fixture.note.exists());
}

#[cfg(unix)]
#[test]
fn failed_source_removal_restores_personal_link_rewrites() {
    let fixture = ShareFixture::new("A note.\n");
    let first = fixture.vault.join("Inbox/a.md");
    let first_before = b"See [[note]].\n";
    std::fs::write(&first, first_before).unwrap();
    let locked_dir = fixture.vault.join("Locked");
    std::fs::create_dir(&locked_dir).unwrap();
    let source = locked_dir.join("note.md");
    std::fs::rename(&fixture.note, &source).unwrap();
    std::fs::write(
        fixture.shared.join("README.md"),
        "---\ntitle: Shared\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: readme\narea: team\ntags: [test]\nowner: test\nauthority: working\n---\nShared.\n",
    )
    .unwrap();
    std::fs::write(
        fixture.shared.join("Knowledge/Architecture/README.md"),
        "---\ntitle: Architecture\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: readme\narea: team\ntags: [test]\nowner: test\nauthority: working\n---\nArchitecture.\n",
    )
    .unwrap();
    git(
        &fixture.shared,
        ["add", "README.md", "Knowledge/Architecture/README.md"],
    );
    git(
        &fixture.shared,
        ["commit", "-q", "-m", "valid shared home readmes"],
    );
    let mut request = fixture.request("Knowledge/Architecture");
    request.note = &source;
    let planned = plan(&request).unwrap();
    assert_eq!(planned.rewrites.len(), 1);
    assert_eq!(planned.rewrites[0].file, "Inbox/a.md");
    let source_before = std::fs::read(&source).unwrap();
    let obstructed_source = fixture.vault.join("Locked/parked-note.md");
    let initial_head = git_text(&fixture.shared, ["rev-parse", "HEAD"]);
    // Replacing the planned source with a directory makes remove_file fail on any uid.
    std::fs::rename(&source, &obstructed_source).unwrap();
    std::fs::create_dir(&source).unwrap();

    let result = apply(&request, &planned, "share note", true);

    let error = result.expect_err("injected source-removal failure was not surfaced");
    assert!(
        format!("{error:#}").contains("not a real regular file"),
        "planned source replacement should be rejected in preflight: {error:#}"
    );
    assert_eq!(std::fs::read(&first).unwrap(), first_before);
    assert!(source.is_dir(), "rollback removed the injected obstruction");
    assert_eq!(std::fs::read(&obstructed_source).unwrap(), source_before);
    assert_eq!(
        git_text(&fixture.shared, ["rev-parse", "HEAD"]),
        initial_head
    );
    assert_eq!(
        git_text(&fixture.shared, ["branch", "--show-current"]),
        "main"
    );
    assert!(git_text(&fixture.shared, ["branch", "--list", &planned.branch]).is_empty());
    assert!(!fixture.shared.join(&planned.destination_rel).exists());
    assert_eq!(git_text(&fixture.shared, ["status", "--porcelain"]), "");
}

#[cfg(unix)]
#[test]
fn share_plan_rejects_destination_directory_under_shared_home_symlink_ancestor() {
    let fixture = ShareFixture::new("A note.\n");
    let outside = fixture._temp.path().join("outside");
    std::fs::create_dir_all(outside.join("Architecture")).unwrap();
    std::fs::remove_dir_all(fixture.shared.join("Knowledge")).unwrap();
    std::os::unix::fs::symlink(&outside, fixture.shared.join("Knowledge")).unwrap();

    let result = plan(&fixture.request("Knowledge/Architecture"));

    assert!(
        result.is_err(),
        "share destination followed symlink ancestor"
    );
    assert!(!outside.join("Architecture/note.md").exists());
}

#[cfg(unix)]
#[test]
fn share_apply_revalidates_destination_after_plan_before_mutation() {
    let fixture = ShareFixture::new("A note.\n");
    let request = fixture.request("Knowledge/Architecture");
    let planned = plan(&request).unwrap();
    let outside = fixture._temp.path().join("outside");
    std::fs::create_dir_all(outside.join("Architecture")).unwrap();
    std::fs::remove_dir_all(fixture.shared.join("Knowledge")).unwrap();
    std::os::unix::fs::symlink(&outside, fixture.shared.join("Knowledge")).unwrap();

    let result = apply(&request, &planned, "share note", false);

    assert!(
        result.is_err(),
        "apply accepted a symlinked destination ancestor"
    );
    assert!(!outside.join("Architecture/note.md").exists());
    assert_eq!(
        git_text(&fixture.shared, ["branch", "--show-current"]),
        "main"
    );
    assert!(git_text(&fixture.shared, ["branch", "--list", &planned.branch]).is_empty());
}

#[test]
fn inbound_rewrites_match_qualified_paths_and_leave_ambiguous_basenames_alone() {
    let fixture = ShareFixture::new("A note.\n");
    std::fs::remove_file(&fixture.note).unwrap();
    let source = fixture.vault.join("Projects/A/cache.md");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(
        &source,
        "---\ntitle: Cache\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: note\narea: test\ntags: [test]\n---\nA cache.\n",
    )
    .unwrap();
    std::fs::create_dir_all(fixture.vault.join("Projects/B")).unwrap();
    std::fs::write(
        fixture.vault.join("Projects/B/cache.md"),
        "---\ntitle: Cache B\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: note\narea: test\ntags: [test]\n---\nB cache.\n",
    )
    .unwrap();
    std::fs::write(
        fixture.vault.join("Projects/reader.md"),
        "[[Projects/B/cache]] [[Projects/A/cache|A cache#details]] [[cache#details]]\n",
    )
    .unwrap();
    let mut request = fixture.request("Knowledge/Architecture");
    request.note = &source;

    let planned = plan(&request).unwrap();

    assert_eq!(planned.rewrites.len(), 1);
    assert_eq!(planned.rewrites[0].from, "Projects/A/cache");
    assert_eq!(planned.rewrites[0].to, "Team/Knowledge/Architecture/cache");
}

#[test]
fn unique_shared_basename_is_canonicalized_and_passes_apply_lint() {
    let fixture = ShareFixture::new("See [[coding-standards|guide#style]].\n");
    std::fs::create_dir_all(fixture.shared.join("Knowledge/Architecture")).unwrap();
    std::fs::write(
        fixture.shared.join("Knowledge/Architecture/README.md"),
        "---\ntitle: Architecture\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: readme\narea: team\ntags: [test]\nowner: test\nauthority: working\n---\nArchitecture.\n",
    )
    .unwrap();
    std::fs::create_dir_all(fixture.shared.join("Standards")).unwrap();
    std::fs::write(
        fixture.shared.join("Standards/coding-standards.md"),
        "---\ntitle: Coding standards\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: note\narea: standards\ntags: [test]\nowner: test\nauthority: working\n---\nStandards.\n",
    )
    .unwrap();
    git(
        &fixture.shared,
        [
            "add",
            "-f",
            "Knowledge/Architecture/README.md",
            "Standards/coding-standards.md",
        ],
    );
    git(&fixture.shared, ["commit", "-q", "-m", "add unique target"]);
    let request = fixture.request("Knowledge/Architecture");

    let planned = plan(&request).unwrap();

    assert_eq!(
        planned.content,
        "---\ntitle: Note\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: note\narea: test\ntags: [test]\n---\nSee [[Standards/coding-standards|guide#style]].\n"
    );
    let applied = apply(&request, &planned, "share note", false);
    assert!(
        applied.is_ok(),
        "planned links failed lint during apply: {applied:?}"
    );
    let lint = ah_vault::lint_shared_home(&fixture.shared).unwrap();
    assert!(
        lint.iter().all(|finding| finding.rule != "VSH004"),
        "applied planner output failed lint: {lint:#?}"
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
