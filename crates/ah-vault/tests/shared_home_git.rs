use ah_vault::{pull_shared_home, shared_home_status};
use std::path::{Path, PathBuf};
use std::process::Command;

struct GitFixture {
    _temp: tempfile::TempDir,
    remote: PathBuf,
    local: PathBuf,
}

impl GitFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let remote = temp.path().join("remote.git");
        let seed = temp.path().join("seed");
        let local = temp.path().join("local");
        git(temp.path(), ["init", "--bare", remote.to_str().unwrap()]);
        git(&remote, ["config", "receive.denyNonFastForwards", "false"]);
        std::fs::create_dir_all(&seed).unwrap();
        git(&seed, ["init", "-b", "main"]);
        git(&seed, ["config", "user.name", "Vesica Test"]);
        git(&seed, ["config", "user.email", "vesica@example.invalid"]);
        std::fs::write(seed.join("README.md"), "shared home\n").unwrap();
        git(&seed, ["add", "README.md"]);
        git(&seed, ["commit", "-m", "initial"]);
        git(&seed, ["remote", "add", "origin", remote.to_str().unwrap()]);
        git(&seed, ["push", "-u", "origin", "main"]);
        git(&remote, ["symbolic-ref", "HEAD", "refs/heads/main"]);
        git(
            temp.path(),
            ["clone", remote.to_str().unwrap(), local.to_str().unwrap()],
        );
        git(&local, ["config", "user.name", "Vesica Test"]);
        git(&local, ["config", "user.email", "vesica@example.invalid"]);
        Self {
            _temp: temp,
            remote,
            local,
        }
    }

    fn advance_remote(&self, name: &str, contents: &str) {
        let checkout = self._temp.path().join(name);
        std::fs::create_dir_all(&checkout).unwrap();
        git(&checkout, ["init", "-b", "main"]);
        git(&checkout, ["symbolic-ref", "HEAD", "refs/heads/main"]);
        git(&checkout, ["config", "user.name", "Vesica Test"]);
        git(
            &checkout,
            ["config", "user.email", "vesica@example.invalid"],
        );
        git(
            &checkout,
            ["remote", "add", "origin", self.remote.to_str().unwrap()],
        );
        git(&checkout, ["fetch", "origin", "main"]);
        git(&checkout, ["reset", "--hard", "origin/main"]);
        git(&checkout, ["branch", "--set-upstream-to", "origin/main"]);
        std::fs::write(checkout.join("team-note.md"), contents).unwrap();
        git(&checkout, ["add", "team-note.md"]);
        git(&checkout, ["commit", "-m", "remote update"]);
        git(&checkout, ["push", "origin", "HEAD:main"]);
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
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn status_reports_clean_tracking_branch_without_fetching() {
    let fixture = GitFixture::new();
    fixture.advance_remote("remote-ahead", "from remote\n");

    let status = shared_home_status(&fixture.local).unwrap();
    assert_eq!(status.branch, "main");
    assert_eq!(status.upstream.as_deref(), Some("origin/main"));
    assert!(status.clean);
    assert_eq!(status.ahead, 0);
    assert_eq!(status.behind, 0, "status is read-only and does not fetch");
}

#[test]
fn pull_reports_an_already_current_checkout() {
    let fixture = GitFixture::new();

    let result = pull_shared_home(&fixture.local).unwrap();

    assert_eq!(result.old_head, result.new_head);
    assert_eq!(shared_home_status(&fixture.local).unwrap().behind, 0);
}

#[test]
fn pull_fast_forwards_a_clean_shared_home() {
    let fixture = GitFixture::new();
    fixture.advance_remote("remote-ahead", "approved shared update\n");

    let result = pull_shared_home(&fixture.local).unwrap();

    assert_ne!(result.old_head, result.new_head);
    assert_eq!(
        std::fs::read_to_string(fixture.local.join("team-note.md")).unwrap(),
        "approved shared update\n"
    );
    assert_eq!(shared_home_status(&fixture.local).unwrap().behind, 0);
}

#[test]
fn pull_refuses_local_changes_without_overwriting_them() {
    let fixture = GitFixture::new();
    std::fs::write(
        fixture.local.join("README.md"),
        "uncommitted personal edit\n",
    )
    .unwrap();

    let error = pull_shared_home(&fixture.local).unwrap_err().to_string();

    assert!(error.contains("uncommitted changes"));
    assert_eq!(
        std::fs::read_to_string(fixture.local.join("README.md")).unwrap(),
        "uncommitted personal edit\n"
    );
}

#[test]
fn pull_refuses_divergence_without_moving_head_or_touching_files() {
    let fixture = GitFixture::new();
    std::fs::write(fixture.local.join("local-only.md"), "keep me\n").unwrap();
    git(&fixture.local, ["add", "local-only.md"]);
    git(&fixture.local, ["commit", "-m", "local change"]);
    let before = git_text(&fixture.local, ["rev-parse", "HEAD"]);
    fixture.advance_remote("remote-ahead", "remote change\n");

    let error = pull_shared_home(&fixture.local).unwrap_err().to_string();

    assert!(error.contains("fast-forward"));
    assert_eq!(git_text(&fixture.local, ["rev-parse", "HEAD"]), before);
    assert_eq!(
        std::fs::read_to_string(fixture.local.join("local-only.md")).unwrap(),
        "keep me\n"
    );
}

#[test]
fn pull_refuses_a_remote_rewind_without_moving_local_head() {
    let fixture = GitFixture::new();
    let updater = fixture._temp.path().join("remote-advance");
    std::fs::create_dir(&updater).unwrap();
    git(&updater, ["init", "-b", "main"]);
    git(&updater, ["config", "user.name", "Vesica Test"]);
    git(&updater, ["config", "user.email", "vesica@example.invalid"]);
    git(
        &updater,
        ["remote", "add", "origin", fixture.remote.to_str().unwrap()],
    );
    git(&updater, ["fetch", "origin", "main"]);
    git(&updater, ["reset", "--hard", "origin/main"]);
    git(&updater, ["branch", "--set-upstream-to", "origin/main"]);
    std::fs::write(updater.join("team-note.md"), "remote update\n").unwrap();
    git(&updater, ["add", "team-note.md"]);
    git(&updater, ["commit", "-m", "remote update"]);
    git(&updater, ["push", "origin", "HEAD:main"]);
    let base = git_text(&fixture.local, ["rev-parse", "HEAD"]);
    let remote_head = git_text(&updater, ["rev-parse", "HEAD"]);
    assert_ne!(base, remote_head);
    git(&fixture.local, ["fetch", "origin", "main"]);
    let fetched_remote = git_text(&fixture.local, ["rev-parse", "refs/remotes/origin/main"]);
    assert_eq!(fetched_remote, remote_head);
    git(&fixture.remote, ["update-ref", "refs/heads/main", &base]);
    let before = git_text(&fixture.local, ["rev-parse", "HEAD"]);

    let error = pull_shared_home(&fixture.local).unwrap_err().to_string();

    assert!(error.contains("rewound"));
    assert_eq!(git_text(&fixture.local, ["rev-parse", "HEAD"]), before);
}

#[test]
fn shared_home_git_command_ignores_hostile_external_diff_configuration() {
    let fixture = GitFixture::new();
    git(&fixture.local, ["config", "diff.external", "false"]);
    git(&fixture.local, ["config", "core.pager", "false"]);

    let status = shared_home_status(&fixture.local).unwrap();

    assert_eq!(status.branch, "main");
    assert!(status.clean);
}

#[test]
fn status_rejects_detached_head() {
    let fixture = GitFixture::new();
    let head = git_text(&fixture.local, ["rev-parse", "HEAD"]);
    git(&fixture.local, ["checkout", "--detach", &head]);

    assert!(shared_home_status(&fixture.local).is_err());
}

#[test]
fn pull_requires_an_upstream() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("no-upstream");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, ["init"]);

    assert!(shared_home_status(&repo).unwrap().upstream.is_none());
    assert!(pull_shared_home(&repo).is_err());
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
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
