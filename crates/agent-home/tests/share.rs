//! `vesica share` (plan §5.4, D26, S9): promote a personal note into the shared home.
//!
//! Plan wording this follows: "branch → move → rewrite inbound personal wikilinks to `[[Team/…]]`
//! → add `owner`/`authority`/`source` → note-lint → open PR (after approval)". S9 also requires
//! refusals for an existing destination, an outside-Team link, a dirty shared checkout,
//! cancellation before confirmation, and no direct trunk writes. No real GitHub submission here:
//! fixture remotes only.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

struct Fixture {
    _temp: tempfile::TempDir,
    shared: PathBuf,
    vault: PathBuf,
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
        for dir in [
            "Knowledge/Architecture",
            "Decisions",
            "Inbox",
            "Sessions/agent-runs",
        ] {
            std::fs::create_dir_all(vault.join(dir)).unwrap();
        }
        let fixture = Self {
            _temp: temp,
            shared,
            vault,
        };
        let shared = fixture.shared.clone();
        fixture.git(&shared, &["init", "-q", "-b", "main"]);
        fixture.git(&shared, &["add", "-A"]);
        fixture.git(&shared, &["commit", "-q", "-m", "initial"]);
        fixture
    }

    fn git(&self, dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Vesica Test")
            .env("GIT_AUTHOR_EMAIL", "vesica@example.invalid")
            .env("GIT_COMMITTER_NAME", "Vesica Test")
            .env("GIT_COMMITTER_EMAIL", "vesica@example.invalid")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn note(&self, rel: &str, body: &str) -> PathBuf {
        let path = self.vault.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();
        path
    }

    /// A valid vault note with no unexpected links.
    fn valid_note(&self, rel: &str) -> PathBuf {
        self.note(
            rel,
            "---\ntitle: Cache strategy\ncreated: 2026-11-18\nupdated: 2026-11-18\ntype: note\narea: general\ntags: [architecture]\n---\n# Cache strategy\n\nBody text.\n",
        )
    }

    fn share(&self, note: &str, extra: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("vesica")
            .unwrap()
            .arg("share")
            .arg(self.vault.join(note))
            .arg("--home-repo")
            .arg(&self.shared)
            .arg("--root")
            .arg(self._temp.path().join("machine"))
            .arg("--vault")
            .arg(&self.vault)
            .args(extra)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("AGENT_HOME_ROOT", self._temp.path().join("machine"))
            .env("VESICA_VAULT", &self.vault)
            .env("GIT_AUTHOR_NAME", "Vesica Test")
            .env("GIT_AUTHOR_EMAIL", "vesica@example.invalid")
            .env("GIT_COMMITTER_NAME", "Vesica Test")
            .env("GIT_COMMITTER_EMAIL", "vesica@example.invalid")
            .assert()
    }
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

#[cfg(unix)]
#[test]
fn share_rollback_preserves_permission_edited_committed_promotion() {
    use std::os::unix::fs::PermissionsExt;
    for mode in [0o400, 0o640] {
        for move_source in [false, true] {
            let fixture = Fixture::new();
            let source = fixture.valid_note("Inbox/cache.md");
            let first = fixture.note("Inbox/a.md", "See [[cache]].\n");
            let later = fixture.note("Inbox/z.md", "Later [[cache]].\n");
            for path in [&source, &first, &later] {
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            let source_before = std::fs::read(&source).unwrap();
            let first_before = std::fs::read(&first).unwrap();
            let later_before = std::fs::read(&later).unwrap();
            let main = fixture.git(&fixture.shared, &["rev-parse", "main"]);
            let promotion = fixture.shared.join("Knowledge/Architecture/cache.md");
            let receipt = fixture._temp.path().join("promotion-receipt");
            let mode_receipt = fixture._temp.path().join("mode-receipt");
            let hook = fixture.shared.join(".git/hooks/post-commit");
            // Git invokes hooks at the shared repository root; all paths below are
            // fixed fixture-relative strings, never interpolated personal paths.
            std::fs::write(&hook, format!(
                "#!/bin/sh\nset -eu\ncp Knowledge/Architecture/cache.md ../promotion-receipt\nchmod {mode:o} Knowledge/Architecture/cache.md\nstat -c %a Knowledge/Architecture/cache.md > ../mode-receipt\nchmod 400 ../vault/Inbox/z.md\n"
            )).unwrap();
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700)).unwrap();
            let extra = if move_source {
                vec!["--area", "Architecture", "--yes", "--move"]
            } else {
                vec!["--area", "Architecture", "--yes"]
            };
            let output = fixture
                .share("Inbox/cache.md", &extra)
                .failure()
                .get_output()
                .clone();
            assert_eq!(output.status.code(), Some(1));
            assert_eq!(
                std::fs::read_to_string(&mode_receipt).unwrap().trim(),
                format!("{mode:o}")
            );
            assert!(
                promotion.is_file(),
                "rollback silently deleted permission-edited promotion"
            );
            assert_eq!(
                std::fs::read(&promotion).unwrap(),
                std::fs::read(&receipt).unwrap()
            );
            assert_eq!(
                std::fs::symlink_metadata(&promotion)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                mode
            );
            let error = String::from_utf8(output.stderr).unwrap();
            assert!(error.contains("rewrite changed after preflight"), "{error}");
            assert!(
                error.contains("share rollback incomplete")
                    && error.contains("destination cleanup refused"),
                "{error}"
            );
            assert_eq!(std::fs::read(&source).unwrap(), source_before);
            assert_eq!(std::fs::read(&first).unwrap(), first_before);
            assert_eq!(std::fs::read(&later).unwrap(), later_before);
            assert_eq!(
                std::fs::metadata(&later).unwrap().permissions().mode() & 0o777,
                0o400
            );
            assert_eq!(fixture.git(&fixture.shared, &["rev-parse", "main"]), main);
            assert_eq!(
                fixture
                    .git(&fixture.shared, &["branch", "--show-current"])
                    .trim(),
                "share/cache"
            );
            assert!(!fixture
                .git(&fixture.shared, &["branch", "--list", "share/cache"])
                .is_empty());
            assert_eq!(
                fixture
                    .git(
                        &fixture.shared,
                        &["show", "share/cache:Knowledge/Architecture/cache.md"]
                    )
                    .as_bytes(),
                std::fs::read(&receipt).unwrap()
            );
            assert_eq!(fixture.git(&fixture.shared, &["status", "--porcelain"]), "");
            let shared_head = fixture.git(&fixture.shared, &["rev-parse", "HEAD"]);
            for _ in 0..2 {
                fixture
                    .share("Inbox/cache.md", &extra)
                    .failure()
                    .stderr(predicate::str::contains("already exists"));
                assert_eq!(
                    fixture.git(&fixture.shared, &["rev-parse", "HEAD"]),
                    shared_head
                );
                assert_eq!(
                    std::fs::read(&promotion).unwrap(),
                    std::fs::read(&receipt).unwrap()
                );
                assert_eq!(
                    std::fs::metadata(&promotion).unwrap().permissions().mode() & 0o777,
                    mode
                );
                assert_eq!(std::fs::read(&source).unwrap(), source_before);
                assert_eq!(std::fs::read(&first).unwrap(), first_before);
                assert_eq!(std::fs::read(&later).unwrap(), later_before);
            }
        }
    }
}

#[test]
fn without_confirmation_it_prints_the_plan_and_writes_nothing() {
    let fixture = Fixture::new();
    fixture.valid_note("Knowledge/Architecture/cache.md");
    let head = fixture.git(&fixture.shared, &["rev-parse", "HEAD"]);
    let out = fixture
        .share(
            "Knowledge/Architecture/cache.md",
            &["--area", "Architecture"],
        )
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("Knowledge/Architecture/cache.md"),
        "{stdout}"
    );
    assert!(stdout.contains("Branch: share/cache"), "{stdout}");
    assert!(
        stdout.contains("--yes") && stdout.contains("Nothing was written"),
        "{stdout}"
    );
    // Cancelling before confirmation leaves no trace.
    assert_eq!(
        fixture.git(&fixture.shared, &["rev-parse", "HEAD"]).trim(),
        head.trim()
    );
    assert_eq!(
        fixture
            .git(&fixture.shared, &["branch", "--show-current"])
            .trim(),
        "main"
    );
    assert!(!fixture
        .shared
        .join("Knowledge/Architecture/cache.md")
        .exists());
}

#[test]
fn dry_run_also_writes_nothing() {
    let fixture = Fixture::new();
    fixture.valid_note("Knowledge/Architecture/cache.md");
    fixture
        .share(
            "Knowledge/Architecture/cache.md",
            &["--area", "Architecture", "--dry-run"],
        )
        .success()
        .stdout(predicate::str::contains("Would create"));
    assert!(!fixture
        .shared
        .join("Knowledge/Architecture/cache.md")
        .exists());
}

#[test]
fn share_promotes_the_note_on_a_branch_with_owner_and_authority() {
    let fixture = Fixture::new();
    fixture.valid_note("Knowledge/Architecture/cache.md");
    fixture
        .share(
            "Knowledge/Architecture/cache.md",
            &["--area", "Architecture", "--yes"],
        )
        .success()
        .stdout(predicate::str::contains("Committed"));

    assert_eq!(
        fixture
            .git(&fixture.shared, &["branch", "--show-current"])
            .trim(),
        "share/cache",
        "the person is left on the share branch, never on trunk"
    );
    let promoted = fixture.shared.join("Knowledge/Architecture/cache.md");
    let text = std::fs::read_to_string(&promoted).unwrap();
    assert!(text.contains("title: Cache strategy"), "{text}");
    assert!(text.contains("owner: Vesica Test"), "{text}");
    assert!(text.contains("authority: working"), "{text}");
    assert!(text.contains("Body text."), "{text}");
    // Trunk is untouched, and the commit exists on the branch.
    assert!(fixture
        .git(&fixture.shared, &["log", "main", "--format=%s"])
        .contains("initial"));
    assert!(!fixture
        .git(&fixture.shared, &["log", "main", "--format=%s"])
        .contains("cache"));
    assert!(fixture
        .git(&fixture.shared, &["log", "share/cache", "--format=%s"])
        .contains("share/cache"));
    assert_eq!(fixture.git(&fixture.shared, &["status", "--porcelain"]), "");
    // The personal note is kept by default.
    assert!(fixture
        .vault
        .join("Knowledge/Architecture/cache.md")
        .is_file());
}

#[test]
fn share_can_promote_into_decisions_and_rewrites_inbound_personal_links() {
    let fixture = Fixture::new();
    fixture.note(
        "Decisions/cache-decision.md",
        "---\ntitle: Cache decision\ncreated: 2026-11-18\nupdated: 2026-11-18\ntype: note\narea: general\ntags: [decision]\n---\n# Cache decision\n",
    );
    // Two personal notes pointing at the promoted note, one unrelated.
    fixture.note(
        "Inbox/pointer.md",
        "See [[cache-decision]] and [[Decisions/cache-decision|the decision]].\n",
    );
    fixture.note("Inbox/other.md", "See [[something-else]].\n");

    fixture
        .share("Decisions/cache-decision.md", &["--decisions", "--yes"])
        .success()
        .stdout(predicate::str::contains("Rewrote"))
        .stdout(predicate::str::contains(
            "[[Team/Decisions/cache-decision]]",
        ));

    assert!(fixture.shared.join("Decisions/cache-decision.md").is_file());
    // Inbound personal links now point into the shared home, aliases preserved.
    let pointer = std::fs::read_to_string(fixture.vault.join("Inbox/pointer.md")).unwrap();
    assert!(
        pointer.contains("[[Team/Decisions/cache-decision]]")
            && pointer.contains("[[Team/Decisions/cache-decision|the decision]]"),
        "{pointer}"
    );
    // Unrelated notes and links are untouched.
    assert_eq!(
        std::fs::read_to_string(fixture.vault.join("Inbox/other.md")).unwrap(),
        "See [[something-else]].\n"
    );
}

#[test]
fn share_refuses_an_existing_destination_a_dirty_checkout_and_an_unknown_area() {
    let fixture = Fixture::new();
    fixture.valid_note("Knowledge/Architecture/cache.md");
    let existing = fixture.shared.join("Knowledge/Architecture/cache.md");
    std::fs::write(&existing, "already there\n").unwrap();
    fixture
        .share(
            "Knowledge/Architecture/cache.md",
            &["--area", "Architecture", "--yes"],
        )
        .failure()
        .stderr(predicate::str::contains("already exists"));
    assert_eq!(
        std::fs::read_to_string(&existing).unwrap(),
        "already there\n"
    );
    std::fs::remove_file(&existing).unwrap();

    // An area that the shared home doesn't have yet.
    fixture
        .share(
            "Knowledge/Architecture/cache.md",
            &["--area", "Nonexistent", "--yes"],
        )
        .failure()
        .stderr(predicate::str::contains("has no Knowledge/Nonexistent"));

    // A dirty shared checkout is never mixed into the commit.
    std::fs::write(
        fixture.shared.join("Standards/coding-standards.md"),
        "edited\n",
    )
    .unwrap();
    fixture
        .share(
            "Knowledge/Architecture/cache.md",
            &["--area", "Architecture", "--yes"],
        )
        .failure()
        .stderr(predicate::str::contains("uncommitted changes"));
    assert_eq!(
        fixture
            .git(&fixture.shared, &["branch", "--show-current"])
            .trim(),
        "main"
    );
    assert!(!fixture
        .shared
        .join("Knowledge/Architecture/cache.md")
        .exists());
}

#[test]
fn share_refuses_a_link_that_would_not_resolve_inside_the_shared_home() {
    let fixture = Fixture::new();
    fixture.note(
        "Knowledge/Architecture/cache.md",
        "---\ntitle: Cache strategy\ncreated: 2026-11-18\nupdated: 2026-11-18\ntype: note\narea: general\ntags: [architecture]\n---\nSee [[my private scratch note]] and [[Standards/coding-standards]].\n",
    );
    let out = fixture
        .share(
            "Knowledge/Architecture/cache.md",
            &["--area", "Architecture", "--yes"],
        )
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("outside the shared home"),
        "expected the outside-Team refusal in:\n{stderr}"
    );
    assert!(!fixture
        .shared
        .join("Knowledge/Architecture/cache.md")
        .exists());

    // The explicit override converts the unresolvable link to plain text and reports it.
    fixture
        .share(
            "Knowledge/Architecture/cache.md",
            &["--area", "Architecture", "--yes", "--drop-unresolved-links"],
        )
        .success()
        .stdout(predicate::str::contains("my private scratch note"));
    let promoted =
        std::fs::read_to_string(fixture.shared.join("Knowledge/Architecture/cache.md")).unwrap();
    assert!(
        !promoted.contains("[[my private scratch note]]"),
        "{promoted}"
    );
    // A `Team/…` link names the vault view; inside the shared home it is rewritten, and reported.
    assert!(
        promoted.contains("[[Standards/coding-standards]]"),
        "a resolvable link must be rewritten to the shared-home path: {promoted}"
    );
    assert!(
        !promoted.contains("[[Team/Standards"),
        "the vault-relative prefix must not survive: {promoted}"
    );
}

#[test]
fn share_refuses_a_note_without_the_vault_frontmatter() {
    let fixture = Fixture::new();
    fixture.note("Inbox/raw.md", "no frontmatter here\n");
    fixture
        .share("Inbox/raw.md", &["--area", "Architecture", "--yes"])
        .failure()
        .stderr(predicate::str::contains("frontmatter"));
    assert!(!fixture
        .shared
        .join("Knowledge/Architecture/raw.md")
        .exists());
}

#[test]
fn share_gates_on_lint_and_vault_scan_and_leaves_nothing_behind() {
    // A personal path is a lint finding (VSH003) in the shared home.
    let fixture = Fixture::new();
    fixture.note(
        "Knowledge/Architecture/leaky.md",
        "---\ntitle: Leaky\ncreated: 2026-11-18\nupdated: 2026-11-18\ntype: note\narea: general\ntags: [x]\n---\nWritten at /home/someone/notes.\n",
    );
    fixture
        .share(
            "Knowledge/Architecture/leaky.md",
            &["--area", "Architecture", "--yes"],
        )
        .failure()
        .stderr(predicate::str::contains("lint"));
    assert!(!fixture
        .shared
        .join("Knowledge/Architecture/leaky.md")
        .exists());
    assert_eq!(
        fixture
            .git(&fixture.shared, &["branch", "--show-current"])
            .trim(),
        "main"
    );
    assert!(!fixture
        .git(&fixture.shared, &["branch", "--list", "share/leaky"])
        .contains("share/leaky"));

    // A credential is a vault-scan finding.
    let key = format!("ghp_{}", "A".repeat(36));
    fixture.note(
        "Knowledge/Architecture/secret.md",
        &format!("---\ntitle: Secret\ncreated: 2026-11-18\nupdated: 2026-11-18\ntype: note\narea: general\ntags: [x]\n---\ntoken {key}\n"),
    );
    fixture
        .share(
            "Knowledge/Architecture/secret.md",
            &["--area", "Architecture", "--yes"],
        )
        .failure()
        .stderr(predicate::str::contains("credential"));
    assert!(!fixture
        .shared
        .join("Knowledge/Architecture/secret.md")
        .exists());
}

#[test]
fn share_refuses_a_note_outside_the_vault_and_a_taken_branch() {
    let fixture = Fixture::new();
    let outside = fixture._temp.path().join("outside.md");
    std::fs::write(&outside, "---\ntitle: X\n---\n").unwrap();
    Command::cargo_bin("vesica")
        .unwrap()
        .arg("share")
        .arg(&outside)
        .arg("--home-repo")
        .arg(&fixture.shared)
        .arg("--root")
        .arg(fixture._temp.path().join("machine"))
        .arg("--vault")
        .arg(&fixture.vault)
        .args(["--area", "Architecture", "--yes"])
        .env("AGENT_HOME_ROOT", fixture._temp.path().join("machine"))
        .env("VESICA_VAULT", &fixture.vault)
        .assert()
        .failure()
        .stderr(predicate::str::contains("outside the vault"));

    fixture.valid_note("Knowledge/Architecture/cache.md");
    fixture.git(&fixture.shared, &["branch", "share/cache"]);
    fixture
        .share(
            "Knowledge/Architecture/cache.md",
            &["--area", "Architecture", "--yes"],
        )
        .failure()
        .stderr(predicate::str::contains(
            "branch share/cache already exists",
        ));
}

#[test]
fn move_removes_the_personal_note_only_after_the_commit() {
    let fixture = Fixture::new();
    fixture.valid_note("Knowledge/Architecture/cache.md");
    fixture
        .share(
            "Knowledge/Architecture/cache.md",
            &["--area", "Architecture", "--yes", "--move"],
        )
        .success()
        .stdout(predicate::str::contains("Moved"));
    assert!(fixture
        .shared
        .join("Knowledge/Architecture/cache.md")
        .is_file());
    assert!(!fixture
        .vault
        .join("Knowledge/Architecture/cache.md")
        .exists());
}

#[test]
fn opening_a_pr_is_gated_on_an_explicit_flag() {
    let fixture = Fixture::new();
    fixture.valid_note("Knowledge/Architecture/cache.md");
    // Without --open-pr the run stops after the local commit and prints the next steps.
    fixture
        .share(
            "Knowledge/Architecture/cache.md",
            &["--area", "Architecture", "--yes"],
        )
        .success()
        .stdout(predicate::str::contains("gh pr create"))
        .stdout(predicate::str::contains("not pushed"));
    // No remote was created, so the shared home still has no upstream.
    assert!(!fixture.git(&fixture.shared, &["remote"]).contains("origin"));
}

#[test]
fn share_respects_the_area_flag_and_decisions_is_not_both() {
    let fixture = Fixture::new();
    fixture.valid_note("Knowledge/Architecture/cache.md");
    fixture
        .share(
            "Knowledge/Architecture/cache.md",
            &["--area", "Architecture", "--decisions", "--yes"],
        )
        .failure()
        .stderr(predicate::str::contains("exactly one of"));
    fixture
        .share("Knowledge/Architecture/cache.md", &["--yes"])
        .failure()
        .stderr(predicate::str::contains("exactly one of"));
}

impl Fixture {
    fn share_with_vault_sources(
        &self,
        note: &str,
        config_vault: Option<&Path>,
        env_vault: Option<&Path>,
        explicit_vault: Option<&Path>,
    ) -> assert_cmd::assert::Assert {
        let machine = self._temp.path().join("machine");
        let config_dir = machine.join(".config/agent-home");
        std::fs::create_dir_all(&config_dir).unwrap();
        if let Some(vault) = config_vault {
            let text = format!("schema = 1\nvault = {:?}\n", vault.to_string_lossy());
            std::fs::write(config_dir.join("config.toml"), text).unwrap();
        }

        let mut command = Command::cargo_bin("vesica").unwrap();
        command
            .arg("share")
            .arg(self.vault.join(note))
            .arg("--home-repo")
            .arg(&self.shared)
            .arg("--root")
            .arg(&machine)
            .args(["--area", "Architecture", "--yes"])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("AGENT_HOME_ROOT", &machine)
            .env_remove("VESICA_VAULT")
            .env("GIT_AUTHOR_NAME", "Vesica Test")
            .env("GIT_AUTHOR_EMAIL", "vesica@example.invalid")
            .env("GIT_COMMITTER_NAME", "Vesica Test")
            .env("GIT_COMMITTER_EMAIL", "vesica@example.invalid");
        if let Some(vault) = env_vault {
            command.env("VESICA_VAULT", vault);
        }
        if let Some(vault) = explicit_vault {
            command.arg("--vault").arg(vault);
        }
        command.assert()
    }

    fn assert_share_kept_personal_data(&self) {
        assert!(self.vault.join("Knowledge/Architecture/cache.md").is_file());
        assert_eq!(
            std::fs::read_to_string(self.vault.join("Inbox/unrelated.md")).unwrap(),
            "Do not change this unrelated personal note.\n"
        );
        assert_eq!(
            std::fs::read_to_string(self._temp.path().join("machine/machine-only.txt")).unwrap(),
            "Machine-root data stays separate.\n"
        );
        assert!(self
            .shared
            .join("Knowledge/Architecture/cache.md")
            .is_file());
    }
}

#[test]
fn share_uses_the_personal_vault_from_machine_config() {
    let fixture = Fixture::new();
    fixture.valid_note("Knowledge/Architecture/cache.md");
    fixture.note(
        "Inbox/unrelated.md",
        "Do not change this unrelated personal note.\n",
    );
    std::fs::create_dir_all(fixture._temp.path().join("machine")).unwrap();
    std::fs::write(
        fixture._temp.path().join("machine/machine-only.txt"),
        "Machine-root data stays separate.\n",
    )
    .unwrap();

    fixture
        .share_with_vault_sources(
            "Knowledge/Architecture/cache.md",
            Some(&fixture.vault),
            None,
            None,
        )
        .success()
        .stdout(predicate::str::contains("Committed"));
    fixture.assert_share_kept_personal_data();
}

#[test]
fn share_environment_vault_overrides_machine_config() {
    let fixture = Fixture::new();
    fixture.valid_note("Knowledge/Architecture/cache.md");
    fixture.note(
        "Inbox/unrelated.md",
        "Do not change this unrelated personal note.\n",
    );
    std::fs::create_dir_all(fixture._temp.path().join("machine")).unwrap();
    std::fs::write(
        fixture._temp.path().join("machine/machine-only.txt"),
        "Machine-root data stays separate.\n",
    )
    .unwrap();
    let config_vault = fixture._temp.path().join("config-vault-decoy");
    std::fs::create_dir_all(&config_vault).unwrap();

    fixture
        .share_with_vault_sources(
            "Knowledge/Architecture/cache.md",
            Some(&config_vault),
            Some(&fixture.vault),
            None,
        )
        .success()
        .stdout(predicate::str::contains("Committed"));
    fixture.assert_share_kept_personal_data();
}

#[test]
fn share_explicit_vault_overrides_environment_and_machine_config() {
    let fixture = Fixture::new();
    fixture.valid_note("Knowledge/Architecture/cache.md");
    fixture.note(
        "Inbox/unrelated.md",
        "Do not change this unrelated personal note.\n",
    );
    std::fs::create_dir_all(fixture._temp.path().join("machine")).unwrap();
    std::fs::write(
        fixture._temp.path().join("machine/machine-only.txt"),
        "Machine-root data stays separate.\n",
    )
    .unwrap();
    let config_vault = fixture._temp.path().join("config-vault-decoy");
    let env_vault = fixture._temp.path().join("env-vault-decoy");
    std::fs::create_dir_all(&config_vault).unwrap();
    std::fs::create_dir_all(&env_vault).unwrap();

    fixture
        .share_with_vault_sources(
            "Knowledge/Architecture/cache.md",
            Some(&config_vault),
            Some(&env_vault),
            Some(&fixture.vault),
        )
        .success()
        .stdout(predicate::str::contains("Committed"));
    fixture.assert_share_kept_personal_data();
}

#[test]
fn share_refuses_when_no_personal_vault_is_configured() {
    let fixture = Fixture::new();
    fixture.valid_note("Knowledge/Architecture/cache.md");
    fixture.note(
        "Inbox/unrelated.md",
        "Do not change this unrelated personal note.\n",
    );
    std::fs::create_dir_all(fixture._temp.path().join("machine")).unwrap();
    std::fs::write(
        fixture._temp.path().join("machine/machine-only.txt"),
        "Machine-root data stays separate.\n",
    )
    .unwrap();

    let output = fixture
        .share_with_vault_sources("Knowledge/Architecture/cache.md", None, None, None)
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("no personal vault configured"), "{stderr}");
    assert!(!fixture
        .shared
        .join("Knowledge/Architecture/cache.md")
        .exists());
    fixture.assert_share_kept_personal_data_without_shared_note();
}

impl Fixture {
    fn assert_share_kept_personal_data_without_shared_note(&self) {
        assert!(self.vault.join("Knowledge/Architecture/cache.md").is_file());
        assert_eq!(
            std::fs::read_to_string(self.vault.join("Inbox/unrelated.md")).unwrap(),
            "Do not change this unrelated personal note.\n"
        );
        assert_eq!(
            std::fs::read_to_string(self._temp.path().join("machine/machine-only.txt")).unwrap(),
            "Machine-root data stays separate.\n"
        );
        assert_eq!(
            self.git(&self.shared, &["branch", "--show-current"]).trim(),
            "main"
        );
    }
}

#[test]
fn share_reports_exempt_assets_in_its_mutation_scan_gate() {
    let f = Fixture::new();
    let asset = f.shared.join("asset.png");
    let bytes = b"\x00\xffexempt asset bytes";
    std::fs::write(&asset, bytes).unwrap();
    f.git(&f.shared, &["add", "asset.png"]);
    f.git(&f.shared, &["commit", "-qm", "asset baseline"]);
    let note = f.valid_note("Inbox/cache.md");
    let before = std::fs::read(&note).unwrap();
    f.share(
        "Inbox/cache.md",
        &["--area", "Architecture", "--owner", "Vesica Test", "--yes"],
    )
    .success()
    .stderr(predicate::str::contains(format!(
        "Excluded/unscanned asset: {}",
        asset.display()
    )))
    .stderr(predicate::str::contains("not counted as clean"));
    assert_eq!(std::fs::read(&asset).unwrap(), bytes);
    assert_eq!(std::fs::read(&note).unwrap(), before);
    assert_eq!(f.git(&f.shared, &["status", "--porcelain"]), "");
}

#[test]
fn share_refuses_identity_redirect_before_mutation_unless_explicitly_dropped() {
    fn snapshot(root: &Path) -> std::collections::BTreeMap<PathBuf, (String, Vec<u8>)> {
        fn visit(
            root: &Path,
            dir: &Path,
            out: &mut std::collections::BTreeMap<PathBuf, (String, Vec<u8>)>,
        ) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let meta = std::fs::symlink_metadata(&path).unwrap();
                let bytes = if meta.is_file() {
                    std::fs::read(&path).unwrap()
                } else {
                    Vec::new()
                };
                out.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    (
                        format!("{:?} {:?}", meta.file_type(), meta.permissions()),
                        bytes,
                    ),
                );
                if meta.is_dir() {
                    visit(root, &path, out);
                }
            }
        }
        let mut out = std::collections::BTreeMap::new();
        visit(root, root, &mut out);
        out
    }
    let fixture = Fixture::new();
    let selected = fixture.shared.join("Guides/README.md");
    let shadow = fixture
        .shared
        .join("Knowledge/Architecture/Guides/README.md");
    let valid = b"---\ntitle: README target\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: readme\narea: team\ntags: [test]\nowner: test\nauthority: working\n---\nTarget.\n".to_vec();
    for path in [&selected, &shadow] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &valid).unwrap();
    }
    fixture.git(&fixture.shared, &["add", "-A"]);
    fixture.git(
        &fixture.shared,
        &["commit", "-qm", "two README identity fixture"],
    );
    let source = fixture.valid_note("Guides.md");
    let source_bytes = [
        std::fs::read(&source).unwrap(),
        b"[[Team/Guides#Heading|Chosen]]\n".to_vec(),
    ]
    .concat();
    std::fs::write(&source, &source_bytes).unwrap();
    let before = snapshot(fixture._temp.path());
    for flags in [
        vec!["--area", "Architecture"],
        vec!["--area", "Architecture", "--yes"],
    ] {
        fixture
            .share("Guides.md", &flags)
            .failure()
            .stderr(predicate::str::contains(
                "cannot preserve selected shared target Guides/README.md",
            ))
            .stderr(predicate::str::contains("--drop-unresolved-links"));
        assert_eq!(
            snapshot(fixture._temp.path()),
            before,
            "refusal must precede all mutation"
        );
    }
    fixture
        .share(
            "Guides.md",
            &["--area", "Architecture", "--drop-unresolved-links"],
        )
        .success();
    assert_eq!(
        snapshot(fixture._temp.path()),
        before,
        "drop preview must be read-only"
    );
    let main = fixture.git(&fixture.shared, &["rev-parse", "main"]);
    fixture
        .share(
            "Guides.md",
            &["--area", "Architecture", "--yes", "--drop-unresolved-links"],
        )
        .success();
    let emitted =
        std::fs::read_to_string(fixture.shared.join("Knowledge/Architecture/Guides.md")).unwrap();
    assert!(
        emitted.ends_with("Guides\n"),
        "explicit drop uses existing text-only policy: {emitted}"
    );
    assert!(!emitted.contains("[["));
    assert_eq!(std::fs::read(&source).unwrap(), source_bytes);
    assert_eq!(std::fs::read(&selected).unwrap(), valid);
    assert_eq!(std::fs::read(&shadow).unwrap(), valid);
    assert_eq!(fixture.git(&fixture.shared, &["rev-parse", "main"]), main);
    assert_eq!(fixture.git(&fixture.shared, &["status", "--porcelain"]), "");
    Command::cargo_bin("vesica")
        .unwrap()
        .args(["lint", "--home-repo"])
        .arg(&fixture.shared)
        .assert()
        .success();
}
