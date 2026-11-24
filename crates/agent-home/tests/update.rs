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
    fake_release_script(
        dist,
        tag,
        &format!("#!/bin/sh\n[ \"$1\" = --version ] && echo \"vesica {version}\"\n"),
    )
}

fn fake_release_script(dist: &Path, tag: &str, script: &str) -> PathBuf {
    // GitHub serves assets as /releases/download/<tag>/<asset>; the fixture mirrors that layout.
    let dist = &dist.join(tag);
    std::fs::create_dir_all(dist).unwrap();
    let staging = dist.join(format!("staging-{tag}"));
    std::fs::create_dir_all(&staging).unwrap();
    let binary = staging.join("vesica");
    std::fs::write(&binary, script).unwrap();
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
        .env("HOME", root)
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

#[derive(Debug, PartialEq, Eq)]
struct HomeNode {
    mode: u32,
    bytes: Option<Vec<u8>>,
    link: Option<PathBuf>,
}

/// A symlink-aware exact tree preimage: never follow managed links into the shared fixture.
fn home_snapshot(root: &Path) -> std::collections::BTreeMap<PathBuf, HomeNode> {
    fn visit(root: &Path, path: &Path, nodes: &mut std::collections::BTreeMap<PathBuf, HomeNode>) {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::symlink_metadata(path).unwrap();
        nodes.insert(
            path.strip_prefix(root).unwrap().to_owned(),
            HomeNode {
                mode: metadata.permissions().mode(),
                bytes: metadata.is_file().then(|| std::fs::read(path).unwrap()),
                link: metadata
                    .file_type()
                    .is_symlink()
                    .then(|| std::fs::read_link(path).unwrap()),
            },
        );
        if metadata.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), nodes);
            }
        }
    }
    let mut nodes = std::collections::BTreeMap::new();
    visit(root, root, &mut nodes);
    nodes
}

fn assert_home_preimage(home: &Path, before: &std::collections::BTreeMap<PathBuf, HomeNode>) {
    let after = home_snapshot(home);
    let paths: std::collections::BTreeSet<_> = before.keys().chain(after.keys()).collect();
    let changed: Vec<_> = paths
        .into_iter()
        .filter(|p| before.get(*p) != after.get(*p))
        .collect();
    assert!(
        changed.is_empty(),
        "installer-managed preimage differs at {changed:?} (exact type/mode/bytes/link comparison)"
    );
}

fn installed_home(temp: &tempfile::TempDir) -> (PathBuf, PathBuf) {
    let home = temp.path().join("home");
    let shared = temp.path().join("shared");
    copy_tree(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home"),
        &shared,
    );
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(&shared)
        .arg("--root")
        .arg(&home)
        .env("HOME", &home)
        .assert()
        .success();
    (home, shared)
}

fn add_relink_skill(shared: &Path) {
    let added = shared.join("Agents/Skills/update-transaction-skill");
    std::fs::create_dir_all(&added).unwrap();
    std::fs::write(added.join("SKILL.md"), "---\nname: update-transaction-skill\ndescription: Use when testing transactional update.\n---\n").unwrap();
}

#[test]
fn update_failed_candidate_after_successful_managed_relink_restores_complete_home_preimage() {
    let temp = tempfile::tempdir().unwrap();
    let (home, shared) = installed_home(&temp);
    let target = temp.path().join("bin/vesica");
    let original = installed_stub(&target, "0.0.1");
    let before = home_snapshot(&home);
    add_relink_skill(&shared);
    let dist = temp.path().join("dist");
    let marker = temp.path().join("migration-count");
    fake_release_script(
        &dist,
        "v9.9.9",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica 9.9.9'; exit 0; fi\nif [ \"$1\" = __update-relink ]; then printf '%s\\n' \"$0\" >> '{}'; '{}' \"$@\" || exit 91; exit 23; fi\nexit 64\n",
            marker.display(),
            env!("CARGO_BIN_EXE_vesica")
        ),
    );
    update(&target, &base_url(&dist), &home, &["--tag", "v9.9.9"])
        .failure()
        .stderr(predicate::str::contains("re-link failed"));
    assert_eq!(std::fs::read(&target).unwrap(), original.as_bytes());
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap().lines().count(),
        1,
        "candidate migration must run exactly once"
    );
    assert_home_preimage(&home, &before);
    assert_eq!(
        std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
        1
    );
}

#[test]
fn failed_candidate_restores_pruned_generated_roles() {
    let temp = tempfile::tempdir().unwrap();
    let (home, shared) = installed_home(&temp);
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(home.join(".config/agent-home/manifest.json")).unwrap(),
    )
    .unwrap();
    assert!(
        manifest["entries"]
            .as_object()
            .unwrap()
            .values()
            .any(|e| e["kind"] == "generated-file"),
        "fixture must install a role"
    );
    let target = temp.path().join("bin/vesica");
    let original = installed_stub(&target, "0.0.1");
    let before = home_snapshot(&home);
    std::fs::remove_dir_all(shared.join("Agents/Roles")).unwrap();
    let dist = temp.path().join("dist");
    fake_release_script(&dist, "v9.9.9", &format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica 9.9.9'; exit 0; fi\n'{}' \"$@\" || exit 91\nexit 23\n",
        env!("CARGO_BIN_EXE_vesica")
    ));
    update(&target, &base_url(&dist), &home, &["--tag", "v9.9.9"])
        .failure()
        .stderr(predicate::str::contains("re-link failed"));
    assert_eq!(std::fs::read(&target).unwrap(), original.as_bytes());
    assert_home_preimage(&home, &before);
}

#[test]
fn update_equal_version_dry_run_does_not_invoke_mutating_relink() {
    let temp = tempfile::tempdir().unwrap();
    let (home, shared) = installed_home(&temp);
    let target = temp.path().join("bin/vesica");
    let marker = temp.path().join("dry-run-migration");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica {CURRENT}'; exit 0; fi\nif [ \"$1\" = __update-relink ]; then echo called >> '{}'; exec '{}' \"$@\"; fi\nexit 64\n",
        marker.display(),
        env!("CARGO_BIN_EXE_vesica")
    );
    installed_stub(&target, CURRENT);
    std::fs::write(&target, &script).unwrap();
    let before = home_snapshot(&home);
    add_relink_skill(&shared);
    update(
        &target,
        "file:///must-not-download",
        &home,
        &["--tag", &format!("v{CURRENT}"), "--dry-run"],
    )
    .success();
    assert!(
        !marker.exists(),
        "dry-run must not launch a mutating candidate migration"
    );
    assert_eq!(home_snapshot(&home), before);
    assert_eq!(std::fs::read(&target).unwrap(), script.as_bytes());
    assert_eq!(
        std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
        1
    );
}

#[test]
fn update_unavailable_shared_home_retains_binary_and_complete_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let (home, shared) = installed_home(&temp);
    let target = temp.path().join("bin/vesica");
    let original = installed_stub(&target, "0.0.1");
    let before = home_snapshot(&home);
    std::fs::remove_dir_all(&shared).unwrap();
    let dist = temp.path().join("dist");
    fake_release_script(
        &dist,
        "v9.9.9",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica 9.9.9'; exit 0; fi\nif [ \"$1\" = __update-relink ]; then exec '{}' \"$@\"; fi\nexit 64\n",
            env!("CARGO_BIN_EXE_vesica")
        ),
    );
    update(&target, &base_url(&dist), &home, &["--tag", "v9.9.9"])
        .failure()
        .stderr(predicate::str::contains(
            "cannot load shared-home configuration",
        ));
    assert_eq!(std::fs::read(&target).unwrap(), original.as_bytes());
    assert_eq!(home_snapshot(&home), before);
    assert_eq!(
        std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
        1
    );
}

#[test]
fn update_failed_candidate_preserves_same_byte_symlink_replacement_and_referent() {
    let temp = tempfile::tempdir().unwrap();
    let (home, _) = installed_home(&temp);
    let target = temp.path().join("bin/vesica");
    installed_stub(&target, "0.0.1");
    let before = home_snapshot(&home);
    let referent = temp.path().join("user-replacement");
    let dist = temp.path().join("dist");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica 9.9.9'; exit 0; fi\nif [ \"$1\" = __update-relink ]; then cp \"$0\" '{}'; rm -f '{}'; ln -s '{}' '{}'; exit 23; fi\nexit 64\n",
        referent.display(),
        target.display(),
        referent.display(),
        target.display()
    );
    fake_release_script(&dist, "v9.9.9", &script);
    update(&target, &base_url(&dist), &home, &["--tag", "v9.9.9"]).failure();
    assert_eq!(std::fs::read_link(&target).unwrap(), referent);
    assert_eq!(
        std::fs::read(&referent).unwrap(),
        script.as_bytes(),
        "binary rollback must not write through a user replacement symlink, even when its referent matches verified bytes"
    );
    assert_eq!(home_snapshot(&home), before);
}

#[test]
fn update_non_equal_dry_run_preserves_complete_installed_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let (home, shared) = installed_home(&temp);
    let target = temp.path().join("bin/vesica");
    let original = installed_stub(&target, "0.0.1");
    let before = home_snapshot(&home);
    add_relink_skill(&shared);
    let dist = temp.path().join("dist");
    let marker = temp.path().join("dry-run-called");
    fake_release_script(
        &dist,
        "v9.9.9",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica 9.9.9'; exit 0; fi\necho called >> '{}'; exit 23\n",
            marker.display()
        ),
    );
    update(
        &target,
        &base_url(&dist),
        &home,
        &["--tag", "v9.9.9", "--dry-run"],
    )
    .success()
    .stdout(predicate::str::contains("Would replace"));
    assert!(!marker.exists());
    assert_home_preimage(&home, &before);
    assert_eq!(std::fs::read(&target).unwrap(), original.as_bytes());
    assert_eq!(
        std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
        1
    );
}

#[test]
fn update_equal_version_real_relink_retries_idempotently() {
    let temp = tempfile::tempdir().unwrap();
    let (home, shared) = installed_home(&temp);
    let target = temp.path().join("bin/vesica");
    let marker = temp.path().join("retry-count");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica {CURRENT}'; exit 0; fi\nif [ \"$1\" = __update-relink ]; then echo called >> '{}'; exec '{}' \"$@\"; fi\nexit 64\n",
        marker.display(),
        env!("CARGO_BIN_EXE_vesica")
    );
    installed_stub(&target, CURRENT);
    std::fs::write(&target, &script).unwrap();
    add_relink_skill(&shared);
    update(
        &target,
        "file:///must-not-download",
        &home,
        &["--tag", &format!("v{CURRENT}")],
    )
    .success();
    assert!(home
        .join(".claude/skills/update-transaction-skill")
        .is_symlink());
    let recovered = home_snapshot(&home);
    update(
        &target,
        "file:///must-not-download",
        &home,
        &["--tag", &format!("v{CURRENT}")],
    )
    .success();
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap(),
        "called\ncalled\n"
    );
    assert_home_preimage(&home, &recovered);
    assert_eq!(std::fs::read(&target).unwrap(), script.as_bytes());
    assert_eq!(
        std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
        1
    );
}

#[test]
fn update_failed_migration_retains_intervening_regular_binary_edit_and_reports_it() {
    let temp = tempfile::tempdir().unwrap();
    let (home, _) = installed_home(&temp);
    let target = temp.path().join("bin/vesica");
    installed_stub(&target, "0.0.1");
    let before = home_snapshot(&home);
    let dist = temp.path().join("dist");
    fake_release_script(
        &dist,
        "v9.9.9",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica 9.9.9'; exit 0; fi\nif [ \"$1\" = __update-relink ]; then printf 'intervening user binary edit' > '{}'; exit 23; fi\nexit 64\n",
            target.display()
        ),
    );
    update(&target, &base_url(&dist), &home, &["--tag", "v9.9.9"])
        .failure()
        .stderr(predicate::str::contains("preserving user changes"));
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"intervening user binary edit"
    );
    assert_home_preimage(&home, &before);
    assert_eq!(
        std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
        1
    );
}

#[test]
fn update_repairs_malformed_installed_identity_instead_of_skipping_the_download() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("bin/vesica");
    installed_stub(&target, CURRENT);
    std::fs::write(
        &target,
        format!("#!/bin/sh\nprintf ' vesica {CURRENT} \\n'\n"),
    )
    .unwrap();
    let dist = temp.path().join("dist");
    fake_release(&dist, &format!("v{CURRENT}"), CURRENT);
    update(
        &target,
        &base_url(&dist),
        &empty_root(&temp),
        &["--tag", &format!("v{CURRENT}")],
    )
    .success()
    .stdout(predicate::str::contains("Downloaded"));
    let actual = std::process::Command::new(&target)
        .arg("--version")
        .output()
        .unwrap();
    assert_eq!(actual.stdout, format!("vesica {CURRENT}\n").as_bytes());
    assert_eq!(
        std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
        1
    );
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
fn update_refuses_a_binary_that_prints_the_tag_but_exits_unsuccessfully() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    let target = temp.path().join("bin/vesica");
    fake_release_script(&dist, "v9.9.9", "#!/bin/sh\necho 'vesica 9.9.9'\nexit 17\n");
    let original = installed_stub(&target, "0.0.1");

    update(
        &target,
        &base_url(&dist),
        &empty_root(&temp),
        &["--tag", "v9.9.9"],
    )
    .failure()
    .stderr(predicate::str::contains("success=false"));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
    let leftovers: Vec<_> = std::fs::read_dir(target.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(leftovers, ["vesica"]);
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
fn update_refuses_malformed_version_whitespace_before_replacing_existing_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    let target = temp.path().join("bin/vesica");
    fake_release_script(&dist, "v9.9.9", "#!/bin/sh\nprintf ' vesica 9.9.9 \\n'\n");
    let original = installed_stub(&target, "0.0.1");
    update(
        &target,
        &base_url(&dist),
        &empty_root(&temp),
        &["--tag", "v9.9.9"],
    )
    .failure()
    .stderr(predicate::str::contains("expected 'vesica 9.9.9'"));
    assert_eq!(std::fs::read(&target).unwrap(), original.as_bytes());
    assert_eq!(
        std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
        1
    );
}

#[test]
fn update_installs_release_when_target_is_older_than_the_requested_current_version() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    let target = temp.path().join("bin/vesica");
    fake_release_script(
        &dist,
        &format!("v{CURRENT}"),
        &format!("#!/bin/sh\n[ \"$1\" = --version ] && echo \"vesica {CURRENT}\"\n"),
    );
    let original = installed_stub(&target, "0.0.1");

    update(
        &target,
        &base_url(&dist),
        &empty_root(&temp),
        &["--tag", &format!("v{CURRENT}")],
    )
    .success()
    .stdout(predicate::str::contains("Replaced"));

    assert_ne!(std::fs::read_to_string(&target).unwrap(), original);
    let version = std::process::Command::new(&target)
        .arg("--version")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        format!("vesica {CURRENT}")
    );
    assert_eq!(
        std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
        1,
        "update staging must be removed"
    );
}

#[test]
fn update_equal_version_retries_manifest_relink() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let shared = temp.path().join("shared");
    let marker = temp.path().join("retry-relink-invoked");
    copy_tree(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home"),
        &shared,
    );
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(&shared)
        .arg("--root")
        .arg(&home)
        .assert()
        .success();
    let target = home.join(".local/bin/vesica");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica {CURRENT}'; exit 0; fi\nif [ \"$1\" = __update-relink ]; then echo called > '{}'; exit 0; fi\nexit 64\n",
        marker.display()
    );
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, &script).unwrap();
    let mut perms = std::fs::metadata(&target).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&target, perms).unwrap();

    update(
        &target,
        "file:///must-not-download",
        &home,
        &["--tag", &format!("v{CURRENT}")],
    )
    .success()
    .stdout(predicate::str::contains("Already up to date"));
    assert!(marker.is_file(), "equal-version retry must recover setup");
}

#[test]
fn update_equal_version_does_not_overwrite_a_modified_managed_file() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let shared = temp.path().join("shared");
    copy_tree(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home"),
        &shared,
    );
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(&shared)
        .arg("--root")
        .arg(&home)
        .assert()
        .success();
    let managed = home.join(".claude/CLAUDE.md");
    let user_edit = b"user edit must survive recovery";
    std::fs::write(&managed, user_edit).unwrap();
    let target = home.join(".local/bin/vesica");
    let test_bin = env!("CARGO_BIN_EXE_vesica");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica {CURRENT}'; exit 0; fi\nif [ \"$1\" = __update-relink ]; then exec \"{test_bin}\" \"$@\"; fi\nexit 64\n"
    );
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, script).unwrap();
    let mut perms = std::fs::metadata(&target).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&target, perms).unwrap();

    update(
        &target,
        "file:///must-not-download",
        &home,
        &["--tag", &format!("v{CURRENT}")],
    )
    .failure()
    .stderr(predicate::str::contains("modified"));
    assert_eq!(std::fs::read(&managed).unwrap(), user_edit);
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
fn update_runs_installed_new_binary_once_and_restores_previous_binary_on_relink_failure() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    let home = temp.path().join("home");
    let shared = temp.path().join("shared");
    let invoked_from = temp.path().join("relink-executable-path");
    copy_tree(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home"),
        &shared,
    );
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(&shared)
        .arg("--root")
        .arg(&home)
        .env("VESICA_TEST_BIN", env!("CARGO_BIN_EXE_vesica"))
        .assert()
        .success();
    let managed = home.join(".claude/CLAUDE.md");
    let user_edit = b"user edit that must survive migration failure";
    std::fs::write(&managed, user_edit).unwrap();

    fake_release_script(
        &dist,
        "v9.9.9",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica 9.9.9'; exit 0; fi\nif [ \"$1\" = __update-relink ]; then printf '%s\\n' \"$0\" >> '{}'; exec \"$VESICA_TEST_BIN\" \"$@\"; fi\nexit 64\n",
            invoked_from.display()
        ),
    );
    let target = home.join(".local/bin/vesica");
    let old_binary = installed_stub(&target, "0.0.1");
    let mut original_permissions = std::fs::metadata(&target).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut original_permissions, 0o750);
    std::fs::set_permissions(&target, original_permissions).unwrap();
    let base = base_url(&dist);

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["update", "--tag", "v9.9.9", "--target"])
        .arg(&target)
        .arg("--root")
        .arg(&home)
        .env("HOME", &home)
        .env("VESICA_TEST_BIN", env!("CARGO_BIN_EXE_vesica"))
        .env("VESICA_RELEASE_BASE_URL", &base)
        .assert()
        .failure()
        .stderr(predicate::str::contains("modified"));

    assert_eq!(std::fs::read(&target).unwrap(), old_binary.as_bytes());
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&target).unwrap().permissions())
            & 0o777,
        0o750,
        "rollback must restore original permissions"
    );
    assert_eq!(std::fs::read(&managed).unwrap(), user_edit);
    assert_eq!(
        std::fs::read_to_string(&invoked_from).unwrap(),
        format!("{}\n", target.display())
    );
    assert_eq!(
        std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
        1
    );
}

#[test]
fn update_removes_new_binary_when_relink_fails_and_no_previous_binary_existed() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    let home = temp.path().join("home");
    let shared = temp.path().join("shared");
    copy_tree(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home"),
        &shared,
    );
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["install", "--home-repo"])
        .arg(&shared)
        .arg("--root")
        .arg(&home)
        .env("VESICA_TEST_BIN", env!("CARGO_BIN_EXE_vesica"))
        .assert()
        .success();
    let managed = home.join(".claude/CLAUDE.md");
    std::fs::write(&managed, b"user edit must remain\n").unwrap();
    let target = home.join(".local/bin/vesica");
    assert!(!target.exists());
    fake_release_script(
        &dist,
        "v9.9.9",
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica 9.9.9'; exit 0; fi\nif [ \"$1\" = __update-relink ]; then exec \"$VESICA_TEST_BIN\" \"$@\"; fi\nexit 64\n",
    );

    Command::cargo_bin("vesica")
        .unwrap()
        .args(["update", "--tag", "v9.9.9", "--target"])
        .arg(&target)
        .arg("--root")
        .arg(&home)
        .env("HOME", &home)
        .env("VESICA_TEST_BIN", env!("CARGO_BIN_EXE_vesica"))
        .env("VESICA_RELEASE_BASE_URL", base_url(&dist))
        .assert()
        .failure()
        .stderr(predicate::str::contains("modified"));

    assert!(
        !target.exists(),
        "failed update must restore the absent preimage"
    );
    assert_eq!(std::fs::read(&managed).unwrap(), b"user edit must remain\n");
}

#[test]
fn update_relinks_from_the_recorded_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let dist = temp.path().join("dist");
    let invoked = temp.path().join("new-binary-relink-invoked");
    fake_release_script(
        &dist,
        "v9.9.9",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica 9.9.9'; exit 0; fi\nif [ \"$1\" = __update-relink ]; then echo invoked > '{}'; exec \"$VESICA_TEST_BIN\" \"$@\"; fi\nexit 64\n",
            invoked.display()
        ),
    );
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
        .env("VESICA_TEST_BIN", env!("CARGO_BIN_EXE_vesica"))
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
    installed_stub(&target, "0.0.1");
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
        .env("HOME", &home)
        .env("VESICA_TEST_BIN", env!("CARGO_BIN_EXE_vesica"))
        .env("VESICA_RELEASE_BASE_URL", &base)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Re-linking from the recorded manifest",
        ))
        .stdout(predicate::str::contains("Created:"));
    assert!(
        invoked.is_file(),
        "verified new executable must run migration"
    );
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
        .env("HOME", &home)
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
        .env("HOME", empty_root(&temp))
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
        .env("HOME", empty_root(&temp))
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

#[test]
fn failed_candidate_preserves_intervening_managed_edit_and_retains_recovery_record() {
    let temp = tempfile::tempdir().unwrap();
    let (home, shared) = installed_home(&temp);
    let target = temp.path().join("bin/vesica");
    let original = installed_stub(&target, "0.0.1");
    add_relink_skill(&shared);
    let dist = temp.path().join("dist");
    let config = home.join(".claude/CLAUDE.md");
    fake_release_script(&dist, "v9.9.9", &format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica 9.9.9'; exit 0; fi\nif [ \"$1\" = __update-relink ]; then '{}' \"$@\" || exit 91; printf 'intervening edit' > '{}'; exit 23; fi\nexit 64\n",
        env!("CARGO_BIN_EXE_vesica"), config.display()));
    update(&target, &base_url(&dist), &home, &["--tag", "v9.9.9"])
        .failure()
        .stderr(predicate::str::contains("recovery journal retained"));
    assert_eq!(std::fs::read(&config).unwrap(), b"intervening edit");
    assert_eq!(std::fs::read(&target).unwrap(), original.as_bytes());
    assert!(std::fs::read_dir(home.join(".config/agent-home"))
        .unwrap()
        .any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".vesica-relink-")));
}

#[test]
fn equal_version_retry_explains_retained_managed_recovery_conflict_before_migration() {
    let temp = tempfile::tempdir().unwrap();
    let (home, shared) = installed_home(&temp);
    let target = temp.path().join("bin/vesica");
    installed_stub(&target, "0.0.1");
    add_relink_skill(&shared);
    let dist = temp.path().join("dist");
    let config = home.join(".claude/CLAUDE.md");
    fake_release_script(&dist, "v9.9.9", &format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica 9.9.9'; exit 0; fi\nif [ \"$1\" = __update-relink ]; then '{}' \"$@\" || exit 91; printf 'intervening edit' > '{}'; exit 23; fi\nexit 64\n",
        env!("CARGO_BIN_EXE_vesica"), config.display()));
    update(&target, &base_url(&dist), &home, &["--tag", "v9.9.9"]).failure();
    let marker = temp.path().join("must-not-migrate");
    let current_script = format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'vesica {CURRENT}'; exit 0; fi\necho called > '{}'; exit 0\n", marker.display());
    std::fs::write(&target, &current_script).unwrap();
    let before = home_snapshot(&home);
    update(
        &target,
        "file:///must-not-download",
        &home,
        &["--tag", &format!("v{CURRENT}")],
    )
    .failure()
    .stderr(predicate::str::contains("pending recovery"));
    assert!(!marker.exists());
    assert_eq!(home_snapshot(&home), before);
}

#[test]
fn equal_version_update_rejects_symlink_target_without_running_referent() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let referent = temp.path().join("outside");
    let marker = temp.path().join("referent-invoked");
    installed_stub(&referent, CURRENT);
    std::fs::write(
        &referent,
        format!(
            "#!/bin/sh\necho called > '{}'; echo 'vesica {CURRENT}'; exit 0\n",
            marker.display()
        ),
    )
    .unwrap();
    let target = temp.path().join("bin/vesica");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&referent, &target).unwrap();
    let before = std::fs::read(&referent).unwrap();
    update(
        &target,
        "file:///must-not-download",
        &home,
        &["--tag", &format!("v{CURRENT}")],
    )
    .failure()
    .stderr(predicate::str::contains("target must be a regular file"));
    assert!(!marker.exists());
    assert_eq!(std::fs::read_link(&target).unwrap(), referent);
    assert_eq!(std::fs::read(&referent).unwrap(), before);
}
