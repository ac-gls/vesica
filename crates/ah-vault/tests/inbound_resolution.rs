use ah_vault::share::{apply, plan, ShareRequest};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

const FM: &str = "---\ntitle: Note\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: note\narea: test\ntags: [test]\nowner: test\nauthority: working\n---\n";

struct Fixture {
    _temp: tempfile::TempDir,
    vault: PathBuf,
    shared: PathBuf,
    source: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("vault");
        let shared = temp.path().join("shared");
        std::fs::create_dir_all(vault.join("Projects/A")).unwrap();
        std::fs::create_dir_all(vault.join("Readers")).unwrap();
        std::fs::create_dir_all(shared.join("Knowledge/Architecture")).unwrap();
        std::fs::write(shared.join("README.md"), FM).unwrap();
        std::fs::write(shared.join("Knowledge/Architecture/README.md"), FM).unwrap();
        git(&shared, &["init", "-q", "-b", "main"]);
        git(&shared, &["config", "user.name", "Vesica Test"]);
        git(&shared, &["config", "user.email", "vesica@example.invalid"]);
        git(&shared, &["add", "."]);
        git(&shared, &["commit", "-qm", "baseline"]);
        let source = vault.join("Projects/A/cache.md");
        std::fs::write(&source, format!("{FM}Selected source.\n")).unwrap();
        Self {
            _temp: temp,
            vault,
            shared,
            source,
        }
    }

    fn request(&self) -> ShareRequest<'_> {
        ShareRequest {
            vault: &self.vault,
            shared: &self.shared,
            note: &self.source,
            destination_dir: "Knowledge/Architecture",
            owner: "test",
            drop_unresolved_links: false,
        }
    }
}

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

#[cfg(unix)]
#[test]
fn unsafe_existing_target_blocks_personal_basename_fallback() {
    for case in [
        "leaf-live",
        "leaf-dangling",
        "ancestor-live",
        "ancestor-dangling",
    ] {
        let fixture = Fixture::new();
        let reader = fixture.vault.join("Readers/reader.md");
        let original = b"[[cache#Heading|Alias]]\n";
        std::fs::write(&reader, original).unwrap();
        let outside = fixture._temp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        let sentinel = outside.join("sentinel.md");
        std::fs::write(&sentinel, b"Outside bytes.\n").unwrap();
        let link = fixture.vault.join(if case.starts_with("leaf") {
            "Readers/cache.md"
        } else {
            "Readers/cache"
        });
        let referent = match case {
            "leaf-live" => sentinel.clone(),
            "leaf-dangling" => outside.join("missing.md"),
            "ancestor-live" => outside.clone(),
            "ancestor-dangling" => outside.join("missing"),
            _ => unreachable!(),
        };
        std::os::unix::fs::symlink(&referent, &link).unwrap();
        let request = fixture.request();
        let planned = plan(&request).unwrap();
        assert!(
            planned.rewrites.is_empty(),
            "{case}: unsafe candidate must not permit fallback to source: {:?}",
            planned.rewrites
        );
        let applied = apply(&request, &planned, "share cache", false).unwrap();
        assert_eq!(applied.rewrites_applied, 0, "{case}");
        assert_eq!(std::fs::read(&reader).unwrap(), original, "{case}");
        assert_eq!(
            std::fs::read(&sentinel).unwrap(),
            b"Outside bytes.\n",
            "{case}"
        );
        assert_eq!(std::fs::read_link(&link).unwrap(), referent, "{case}");
        assert!(!outside.join("missing").exists(), "{case}");
        assert!(!outside.join("missing.md").exists(), "{case}");
    }
}

#[test]
fn existing_reader_relative_readme_blocks_unqualified_basename_fallback() {
    let fixture = Fixture::new();
    let reader = fixture.vault.join("Readers/reader.md");
    let original = b"[[cache#Heading|Alias]]\n";
    std::fs::write(&reader, original).unwrap();
    let shadow = fixture.vault.join("Readers/cache/README.md");
    std::fs::create_dir_all(shadow.parent().unwrap()).unwrap();
    std::fs::write(&shadow, b"Unrelated directory target.\n").unwrap();
    let shadow_before = std::fs::read(&shadow).unwrap();
    let request = fixture.request();
    let planned = plan(&request).unwrap();
    let applied = apply(&request, &planned, "share cache", false).unwrap();
    assert_eq!(
        std::fs::read(&reader).unwrap(),
        original,
        "existing README target must block unique-stem fallback; rewrites: {:?}",
        planned.rewrites
    );
    assert_eq!(std::fs::read(&shadow).unwrap(), shadow_before);
    assert!(planned.rewrites.is_empty());
    assert_eq!(applied.rewrites_applied, 0);
}

#[test]
fn existing_wrong_extension_target_does_not_resolve_to_markdown_source() {
    let fixture = Fixture::new();
    let reader = fixture.vault.join("Readers/reader.md");
    let original = b"Before\r\n[[cache.txt]] [[cache.txt#Heading|Alias]]\r\nAfter";
    std::fs::write(&reader, original).unwrap();
    let text_target = fixture.vault.join("Readers/cache.txt");
    std::fs::write(&text_target, b"Unrelated text target.\n").unwrap();
    let genuine = fixture.vault.join("genuine.md");
    std::fs::write(&genuine, "[[cache#Heading|Alias]]\n").unwrap();
    let source_before = std::fs::read(&fixture.source).unwrap();
    let target_before = std::fs::read(&text_target).unwrap();
    let request = fixture.request();
    let planned = plan(&request).unwrap();
    let applied = apply(&request, &planned, "share cache", false).unwrap();
    assert_eq!(
        std::fs::read(&reader).unwrap(),
        original,
        "wrong-extension link must leave the reader byte-identical; rewrites: {:?}",
        planned.rewrites
    );
    assert_eq!(std::fs::read(&text_target).unwrap(), target_before);
    assert_eq!(std::fs::read(&fixture.source).unwrap(), source_before);
    assert_eq!(
        std::fs::read(&genuine).unwrap(),
        b"[[Team/Knowledge/Architecture/cache#Heading|Alias]]\n"
    );
    assert_eq!(applied.rewrites_applied, 1);
    assert!(planned
        .rewrites
        .iter()
        .all(|rewrite| rewrite.file != "Readers/reader.md"));
    assert!(fixture.shared.join(&planned.destination_rel).is_file());
    assert_eq!(git(&fixture.shared, &["status", "--porcelain"]), "");
    assert!(!walkdir::WalkDir::new(&fixture.vault)
        .into_iter()
        .flatten()
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with(".vesica-share-")));
}

#[test]
fn reader_relative_existing_target_shadows_vault_root_source() {
    let fixture = Fixture::new();
    let reader = fixture.vault.join("Readers/reader.md");
    let original = b"Before\r\n[[Projects/A/cache]] [[Projects/A/cache.md#Heading|Alias]]\r\nAfter";
    std::fs::write(&reader, original).unwrap();
    let shadow = fixture.vault.join("Readers/Projects/A/cache.md");
    std::fs::create_dir_all(shadow.parent().unwrap()).unwrap();
    std::fs::write(&shadow, b"Unrelated reader-relative target.\n").unwrap();
    // A second reader has no local shadow and must still follow the selected root target.
    let genuine = fixture.vault.join("genuine.md");
    std::fs::write(&genuine, "[[Projects/A/cache.md#Heading|Alias]]\n").unwrap();
    let source_before = std::fs::read(&fixture.source).unwrap();
    let shadow_before = std::fs::read(&shadow).unwrap();
    let request = fixture.request();
    let planned = plan(&request).unwrap();
    let applied = apply(&request, &planned, "share cache", false).unwrap();
    assert_eq!(
        std::fs::read(&reader).unwrap(),
        original,
        "reader-relative shadow must leave the reader byte-identical; rewrites: {:?}",
        planned.rewrites
    );
    assert_eq!(std::fs::read(&shadow).unwrap(), shadow_before);
    assert_eq!(std::fs::read(&fixture.source).unwrap(), source_before);
    assert_eq!(
        std::fs::read(&genuine).unwrap(),
        b"[[Team/Knowledge/Architecture/cache#Heading|Alias]]\n"
    );
    assert_eq!(applied.rewrites_applied, 1);
    assert!(planned
        .rewrites
        .iter()
        .all(|rewrite| rewrite.file != "Readers/reader.md"));
    assert!(fixture.shared.join(&planned.destination_rel).is_file());
    assert_eq!(git(&fixture.shared, &["status", "--porcelain"]), "");
    assert!(!walkdir::WalkDir::new(&fixture.vault)
        .into_iter()
        .flatten()
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with(".vesica-share-")));
}
