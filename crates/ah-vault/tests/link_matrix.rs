use ah_vault::{
    lint_shared_home,
    share::{apply, plan, promoted_content, ShareRequest},
};
use std::{path::Path, process::Command};

fn git(root: &Path, args: &[&str]) {
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
}
const FM: &str = "---\ntitle: Note\ncreated: 2026-11-23\nupdated: 2026-11-23\ntype: note\narea: test\ntags: [test]\nowner: test\nauthority: working\n---\n";

#[test]
fn inbound_link_matrix_resolves_only_selected_source_and_preserves_suffixes() {
    for (target, matches_source) in [
        ("Projects/A/cache", true),
        ("Projects/A/cache.md", true),
        ("Projects/A/cache#Heading|Alias", true),
        ("Projects/A/cache.md#^block", true),
        ("Projects/B/cache", false),
        ("cache", false),
        ("cache.md|Alias", false),
        ("Projects/A/CACHE", false),
        ("projects/A/cache", false),
        ("Team/Projects/A/cache", false),
        ("../Projects/A/cache", false),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("vault");
        let shared = temp.path().join("shared");
        for path in [
            vault.join("Projects/A"),
            vault.join("Projects/B"),
            shared.join("Knowledge/Architecture"),
        ] {
            std::fs::create_dir_all(path).unwrap();
        }
        let note = vault.join("Projects/A/cache.md");
        std::fs::write(&note, FM).unwrap();
        std::fs::write(vault.join("Projects/B/cache.md"), FM).unwrap();
        std::fs::write(vault.join("reader.md"), format!("[[{target}]]\n")).unwrap();
        std::fs::write(shared.join("README.md"), FM).unwrap();
        git(&shared, &["init", "-q", "-b", "main"]);
        git(&shared, &["config", "user.name", "Test"]);
        git(&shared, &["config", "user.email", "test@example.invalid"]);
        git(&shared, &["add", "."]);
        git(&shared, &["commit", "-qm", "baseline"]);
        let request = ShareRequest {
            vault: &vault,
            shared: &shared,
            note: &note,
            destination_dir: "Knowledge/Architecture",
            owner: "test",
            drop_unresolved_links: false,
        };
        let planned = plan(&request).unwrap();
        assert_eq!(
            !planned.rewrites.is_empty(),
            matches_source,
            "{target}: {:?}",
            planned.rewrites
        );
    }
}

#[test]
fn inbound_readme_promotion_keeps_explicit_destination_and_unrelated_collision_links() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    let shared = temp.path().join("shared");
    for dir in [vault.join("Guides"), shared.join("Knowledge/Architecture")] {
        std::fs::create_dir_all(dir).unwrap();
    }
    for relative in ["README.md", "Knowledge/Architecture.md"] {
        std::fs::write(shared.join(relative), format!("{FM}Shared shadow.\n")).unwrap();
    }
    let note = vault.join("Guides/README.md");
    let source = format!("{FM}Selected personal README.\n");
    std::fs::write(&note, &source).unwrap();
    let personal_shadow = format!("{FM}Distinct personal shadow.\n");
    std::fs::write(vault.join("Guides.md"), &personal_shadow).unwrap();
    let reader = vault.join("reader.md");
    let before = "[[Guides/README.md]]\n[[Guides/README.md#Heading|Alias]]\n[[Guides/README#^block|Block]]\n[[Guides]]\n[[Guides.md|Other]]\n";
    std::fs::write(&reader, before).unwrap();
    git(&shared, &["init", "-q", "-b", "main"]);
    git(&shared, &["config", "user.name", "Test"]);
    git(&shared, &["config", "user.email", "test@example.invalid"]);
    git(&shared, &["add", "."]);
    git(&shared, &["commit", "-qm", "baseline"]);
    let request = ShareRequest {
        vault: &vault,
        shared: &shared,
        note: &note,
        destination_dir: "Knowledge/Architecture",
        owner: "test",
        drop_unresolved_links: false,
    };
    let planned = plan(&request).unwrap();
    assert_eq!(planned.destination_rel, "Knowledge/Architecture/README.md");
    assert_eq!(std::fs::read_to_string(&reader).unwrap(), before);
    assert!(!shared.join(&planned.destination_rel).exists());
    // Plans deduplicate aliases/anchors of the same target into one rewrite.
    assert_eq!(planned.rewrites.len(), 2, "{:?}", planned.rewrites);
    assert!(planned.rewrites.iter().all(|rewrite| {
        rewrite.file == "reader.md" && rewrite.to == "Team/Knowledge/Architecture/README"
    }));
    let result = apply(&request, &planned, "promote README", false).unwrap();
    assert_eq!(result.rewrites_applied, 2);
    assert!(!result.moved);
    assert!(lint_shared_home(&shared).unwrap().is_empty());
    let after = std::fs::read_to_string(&reader).unwrap();
    assert_eq!(after, "[[Team/Knowledge/Architecture/README]]\n[[Team/Knowledge/Architecture/README#Heading|Alias]]\n[[Team/Knowledge/Architecture/README#^block|Block]]\n[[Guides]]\n[[Guides.md|Other]]\n");
    // The retained README segment selects the promoted file, not Architecture.md.
    assert_eq!(
        std::fs::read_to_string(shared.join("Knowledge/Architecture/README.md")).unwrap(),
        promoted_content(&request, &planned)
    );
    assert_eq!(std::fs::read_to_string(&note).unwrap(), source);
    assert_eq!(
        std::fs::read_to_string(vault.join("Guides.md")).unwrap(),
        personal_shadow
    );
    assert_eq!(
        std::fs::read_to_string(shared.join("Knowledge/Architecture.md")).unwrap(),
        format!("{FM}Shared shadow.\n")
    );
}

#[test]
fn planner_and_lint_use_same_case_sensitive_path_readme_alias_anchor_ambiguity_matrix() {
    for (target, resolves) in [
        ("unique", true),
        ("unique.md", true),
        ("Standards/unique", true),
        ("Standards/unique.md|Alias", true),
        ("Standards/unique#Heading|Alias", true),
        ("Standards/unique#^block", true),
        ("Guides", true),
        ("Guides/README.md#Heading", true),
        ("ambiguous", false),
        ("Standards/UNIQUE", false),
        ("standards/unique", false),
        ("unique.MD", false),
        ("Wrong/unique", false),
        ("../Standards/unique", false),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("vault");
        let shared = temp.path().join("shared");
        std::fs::create_dir(&vault).unwrap();
        for dir in ["Knowledge/Architecture", "Standards", "Guides", "Other"] {
            std::fs::create_dir_all(shared.join(dir)).unwrap();
        }
        for file in [
            "README.md",
            "Knowledge/Architecture/README.md",
            "Standards/unique.md",
            "Standards/ambiguous.md",
            "Other/ambiguous.md",
            "Guides/README.md",
        ] {
            std::fs::write(shared.join(file), format!("{FM}Body.\n")).unwrap();
        }
        let reader = shared.join("Knowledge/Architecture/reader.md");
        std::fs::write(&reader, format!("{FM}[[{target}]]\n")).unwrap();
        let lint = lint_shared_home(&shared).unwrap();
        let lint_resolves = !lint
            .iter()
            .any(|f| f.rule == "VSH004" && f.path == Path::new("Knowledge/Architecture/reader.md"));
        std::fs::remove_file(reader).unwrap();
        git(&shared, &["init", "-q", "-b", "main"]);
        git(&shared, &["config", "user.name", "Test"]);
        git(&shared, &["config", "user.email", "test@example.invalid"]);
        git(&shared, &["add", "."]);
        git(&shared, &["commit", "-qm", "baseline"]);
        let note = vault.join("note.md");
        std::fs::write(&note, format!("{FM}[[{target}]]\n")).unwrap();
        let mut request = ShareRequest {
            vault: &vault,
            shared: &shared,
            note: &note,
            destination_dir: "Knowledge/Architecture",
            owner: "test",
            drop_unresolved_links: false,
        };
        let planned = plan(&request);
        assert_eq!(planned.is_ok(), resolves, "planner {target}: {planned:?}");
        assert_eq!(lint_resolves, resolves, "lint {target}");
        request.drop_unresolved_links = true;
        let planned = plan(&request).unwrap();
        if !resolves {
            assert_eq!(planned.dropped_links.len(), 1, "{target}");
            assert!(!planned.content.contains("[["), "{target}");
        }
        std::fs::write(
            shared.join("Knowledge/Architecture/note.md"),
            planned.content,
        )
        .unwrap();
        let lint = lint_shared_home(&shared).unwrap();
        assert!(
            lint.iter().all(|f| f.rule != "VSH004"),
            "emitted {target}: {lint:?}"
        );
    }
}
