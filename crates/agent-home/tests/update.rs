//! `vesica update` (plan §5.4 command table, F3.5): resolve a release, download the tarball and
//! `SHA256SUMS`, verify the checksum, self-replace, then re-link from the manifest.
//!
//! Fixture releases are served from a local directory through a `file://` base URL, which mirrors
//! exactly what `scripts/build-release.sh` publishes.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// A stub `vesica` binary that reports `version`, packed the way the release workflow packs it.
fn fake_release(dist: &Path, tag: &str, version: &str) -> PathBuf {
    let version = version.trim_start_matches('v');
    // GitHub serves assets as /releases/download/<tag>/<asset>; the fixture mirrors that layout.
    let dist = &dist.join(tag);
    std::fs::create_dir_all(dist).unwrap();
    let staging = dist.join(format!("staging-{tag}"));
    std::fs::create_dir_all(&staging).unwrap();
    let binary = staging.join("vesica");
    std::fs::write(
        &binary,
        format!("#!/bin/sh\n[ \"$1\" = --version ] && echo \"vesica {version}\"\n"),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&binary).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&binary, perms).unwrap();

    let asset = format!("vesica-{tag}-x86_64-linux.tar.gz");
    let out = std::process::Command::new("tar")
        .arg("-czf")
        .arg(dist.join(&asset))
        .arg("-C")
        .arg(&staging)
        .arg("vesica")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    append_checksum(dist, &asset);
    std::fs::remove_dir_all(&staging).unwrap();
    dist.join(asset)
}

fn tag_dir(dist: &Path, tag: &str) -> PathBuf {
    dist.join(tag)
}

fn append_checksum(dist: &Path, asset: &str) {
    let bytes = std::fs::read(dist.join(asset)).unwrap();
    let hash = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&bytes));
    let mut sums = std::fs::read_to_string(dist.join("SHA256SUMS")).unwrap_or_default();
    sums.push_str(&format!("{hash}  {asset}\n"));
    std::fs::write(dist.join("SHA256SUMS"), sums).unwrap();
}

fn base_url(dist: &Path) -> String {
    format!("file://{}", dist.display())
}

/// `--root` is always explicit: without it the re-link step would resolve `$HOME`.
fn update(target: &Path, base: &str, root: &Path, extra: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("vesica")
        .unwrap()
        .arg("update")
        .arg("--target")
        .arg(target)
        .arg("--root")
        .arg(root)
        .env("VESICA_RELEASE_BASE_URL", base)
        .args(extra)
        .assert()
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn empty_root(temp: &tempfile::TempDir) -> PathBuf {
    temp.path().join("root")
}

/// An installed stub binary at `target` whose `--version` prints `version`.
fn installed_stub(target: &Path, version: &str) -> String {
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    let script = format!("#!/bin/sh\n[ \"$1\" = --version ] && echo \"vesica {version}\"\n");
    std::fs::write(target, &script).unwrap();
    let mut perms = std::fs::metadata(target).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(target, perms).unwrap();
    script
}

#[test]
fn update_downloads_verifies_and_replaces_the_binary() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    let target = temp.path().join("bin/vesica");
    fake_release(&dist, "v9.9.9", "9.9.9");
    installed_stub(&target, "0.0.1");

    update(
        &target,
        &base_url(&dist),
        &empty_root(&temp),
        &["--tag", "v9.9.9"],
    )
    .success()
    .stdout(predicate::str::contains(
        "Downloaded vesica-v9.9.9-x86_64-linux.tar.gz",
    ))
    .stdout(predicate::str::contains("checksum verified").and(predicate::str::contains("v9.9.9")))
    .stdout(predicate::str::contains("Replaced"));

    let replaced = std::fs::read_to_string(&target).unwrap();
    assert!(
        replaced.contains("9.9.9") && !replaced.contains("0.0.1"),
        "{replaced}"
    );
    let version = std::process::Command::new(&target)
        .arg("--version")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        "vesica 9.9.9"
    );
    // Nothing left behind next to the binary.
    let leftovers: Vec<_> = std::fs::read_dir(target.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(leftovers, ["vesica"]);
}

#[test]
fn update_creates_the_target_when_it_does_not_exist_yet() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    let target = temp.path().join("fresh/bin/vesica");
    fake_release(&dist, "v9.9.9", "9.9.9");
    update(
        &target,
        &base_url(&dist),
        &empty_root(&temp),
        &["--tag", "v9.9.9"],
    )
    .success();
    assert!(target.is_file());
}

#[test]
fn update_refuses_a_checksum_mismatch_and_leaves_the_binary_alone() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    let target = temp.path().join("bin/vesica");
    let asset = fake_release(&dist, "v9.9.9", "9.9.9");
    // Corrupt the artifact after its checksum was recorded.
    let mut bytes = std::fs::read(&asset).unwrap();
    bytes.extend_from_slice(b"tampered");
    std::fs::write(&asset, bytes).unwrap();
    let original = installed_stub(&target, "0.0.1");

    update(
        &target,
        &base_url(&dist),
        &empty_root(&temp),
        &["--tag", "v9.9.9"],
    )
    .failure()
    .stderr(predicate::str::contains("checksum mismatch"));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
}

#[test]
fn update_refuses_a_missing_checksum_entry() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    let target = temp.path().join("bin/vesica");
    fake_release(&dist, "v9.9.9", "9.9.9");
    std::fs::write(tag_dir(&dist, "v9.9.9").join("SHA256SUMS"), "").unwrap();
    let original = installed_stub(&target, "0.0.1");
    update(
        &target,
        &base_url(&dist),
        &empty_root(&temp),
        &["--tag", "v9.9.9"],
    )
    .failure()
    .stderr(predicate::str::contains("no checksum for"));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
}

#[test]
fn update_refuses_a_binary_that_does_not_report_the_tag() {
    // Guards against a mis-built release: the checksum can match while the binary is wrong.
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    let target = temp.path().join("bin/vesica");
    fake_release(&dist, "v9.9.9", "9.9.8");
    let original = installed_stub(&target, "0.0.1");
    update(
        &target,
        &base_url(&dist),
        &empty_root(&temp),
        &["--tag", "v9.9.9"],
    )
    .failure()
    .stderr(predicate::str::contains("reports 'vesica 9.9.8'"));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
}

#[test]
fn update_reports_up_to_date_without_downloading() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("empty-dist");
    let target = temp.path().join("bin/vesica");
    let original = installed_stub(&target, CURRENT);
    update(
        &target,
        &base_url(&dist),
        &empty_root(&temp),
        &["--tag", &format!("v{CURRENT}")],
    )
    .success()
    .stdout(predicate::str::contains("Already up to date"));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
}

#[test]
fn update_dry_run_downloads_and_verifies_but_replaces_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    let target = temp.path().join("bin/vesica");
    fake_release(&dist, "v9.9.9", "9.9.9");
    let original = installed_stub(&target, "0.0.1");
    update(
        &target,
        &base_url(&dist),
        &empty_root(&temp),
        &["--tag", "v9.9.9", "--dry-run"],
    )
    .success()
    .stdout(predicate::str::contains("Would replace"));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
}

#[test]
fn update_relinks_from_the_recorded_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    fake_release(&dist, "v9.9.9", "9.9.9");
    let shared = temp.path().join("shared");
    copy_tree(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home"),
        &shared,
    );
    let home = temp.path().join("home");
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(&shared)
        .arg("--root")
        .arg(&home)
        .assert()
        .success();
    // A skill added to the shared home after install is exactly what re-linking must pick up.
    let added = shared.join("Agents/Skills/later-skill");
    std::fs::create_dir_all(&added).unwrap();
    std::fs::write(
        added.join("SKILL.md"),
        "---\nname: later-skill\ndescription: Use when testing re-link.\n---\n",
    )
    .unwrap();
    assert!(!home.join(".claude/skills/later-skill").exists());

    let target = home.join(".local/bin/vesica");
    let base = base_url(&dist);
    Command::cargo_bin("vesica")
        .unwrap()
        .arg("update")
        .arg("--tag")
        .arg("v9.9.9")
        .arg("--target")
        .arg(&target)
        .arg("--root")
        .arg(&home)
        .env("VESICA_RELEASE_BASE_URL", &base)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Re-linking from the recorded manifest",
        ))
        .stdout(predicate::str::contains("Created:"));
    assert!(home.join(".claude/skills/demo-skill").is_symlink());
    assert!(home.join(".claude/skills/later-skill").is_symlink());
    assert!(home.join(".agents/skills/later-skill").is_symlink());
    assert!(target.is_file());
}

#[test]
fn update_without_a_manifest_skips_relinking() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    fake_release(&dist, "v9.9.9", "9.9.9");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let target = home.join("bin/vesica");
    Command::cargo_bin("vesica")
        .unwrap()
        .arg("update")
        .arg("--tag")
        .arg("v9.9.9")
        .arg("--target")
        .arg(&target)
        .arg("--root")
        .arg(&home)
        .env("VESICA_RELEASE_BASE_URL", base_url(&dist))
        .assert()
        .success()
        .stdout(predicate::str::contains("No manifest recorded"));
}

#[test]
fn update_rejects_a_bad_tag_before_reaching_the_network() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("bin/vesica");
    installed_stub(&target, "0.0.1");
    for tag in ["9.9.9", "v../etc", "v9.9", "vnext"] {
        update(
            &target,
            "file:///nonexistent",
            &empty_root(&temp),
            &["--tag", tag],
        )
        .failure()
        .stderr(predicate::str::contains("tag must look like"));
    }
    assert!(std::fs::read_to_string(&target).unwrap().contains("0.0.1"));
}

/// A `file://` release-list fixture, so channel selection is testable offline.
fn fake_releases_api(dist: &Path) -> String {
    let releases = serde_json::json!([
        {"tag_name": "v0.1.0-alpha.1", "draft": false, "prerelease": true,
         "published_at": "2026-11-18T16:55:42Z"},
        {"tag_name": "v0.3.0", "draft": false, "prerelease": false,
         "published_at": "2026-11-19T10:00:00Z"},
        {"tag_name": "v0.2.0-alpha.1", "draft": false, "prerelease": true,
         "published_at": "2026-11-20T10:00:00Z"},
        {"tag_name": "v9.9.9", "draft": true, "prerelease": false,
         "published_at": "2026-12-01T00:00:00Z"}
    ]);
    std::fs::write(dist.join("releases.json"), releases.to_string()).unwrap();
    format!("file://{}/releases.json", dist.display())
}

#[test]
fn update_resolves_the_newest_release_per_channel() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    std::fs::create_dir_all(&dist).unwrap();
    let api = fake_releases_api(&dist);
    let target = temp.path().join("bin/vesica");
    installed_stub(&target, "0.0.1");
    // alpha: the newest published release, including pre-releases (drafts never count).
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["update", "--target"])
        .arg(&target)
        .arg("--root")
        .arg(empty_root(&temp))
        .args(["--channel", "alpha", "--dry-run"])
        .env("VESICA_RELEASES_API_URL", &api)
        .env("VESICA_RELEASE_BASE_URL", base_url(&dist))
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "Newest alpha release: v0.2.0-alpha.1",
        ))
        .stdout(predicate::str::contains("v0.2.0-alpha.1")); // download attempted
                                                             // stable: skips pre-releases and drafts.
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["update", "--target"])
        .arg(&target)
        .arg("--root")
        .arg(empty_root(&temp))
        .args(["--channel", "stable", "--dry-run"])
        .env("VESICA_RELEASES_API_URL", &api)
        .env("VESICA_RELEASE_BASE_URL", base_url(&dist))
        .assert()
        .failure()
        .stdout(predicate::str::contains("Newest stable release: v0.3.0"))
        .stdout(predicate::str::contains("v0.3.0"));
}

#[test]
fn update_reports_when_a_channel_has_no_release() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    std::fs::create_dir_all(&dist).unwrap();
    std::fs::write(dist.join("releases.json"), "[]").unwrap();
    let target = temp.path().join("bin/vesica");
    installed_stub(&target, "0.0.1");
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["update", "--target"])
        .arg(&target)
        .arg("--root")
        .arg(empty_root(&temp))
        .env(
            "VESICA_RELEASES_API_URL",
            format!("file://{}/releases.json", dist.display()),
        )
        .assert()
        .failure()
        .stderr(predicate::str::contains("no published release found"));
}
