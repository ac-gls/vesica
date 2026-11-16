use ah_vault::lint_shared_home;
use std::path::{Path, PathBuf};

fn fixture_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home")
}

#[test]
fn lint_accepts_the_shared_home_fixture() {
    let findings = lint_shared_home(&fixture_home()).unwrap();
    assert!(
        findings
            .iter()
            .all(|finding| !matches!(finding.rule, "VSH008" | "VSH009" | "VSH010" | "VSH011")),
        "fixture has policy metadata findings: {findings:#?}"
    );
}

#[test]
fn lint_reports_missing_readme_with_safe_path_and_rule_code() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("Knowledge/Guides")).unwrap();
    std::fs::write(
        temp.path().join("Knowledge/README.md"),
        "---\ntitle: Knowledge\ncreated: 2026-11-16\nupdated: 2026-11-16\ntype: readme\narea: team\ntags: [index]\nowner: team\nauthority: working\n---\n# Knowledge\n",
    )
    .unwrap();
    let findings = lint_shared_home(temp.path()).unwrap();
    assert!(findings.iter().any(|finding| {
        finding.rule == "VSH001" && finding.path == std::path::Path::new("Knowledge/Guides")
    }));
}

#[test]
fn lint_reports_unresolved_wikilink_without_echoing_linked_text() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("Knowledge")).unwrap();
    std::fs::write(temp.path().join("Knowledge/README.md"), "# Knowledge\n").unwrap();
    let marker = format!("unresolved-{}", "X".repeat(14));
    std::fs::write(
        temp.path().join("Knowledge/note.md"),
        format!("See [[{marker}]].\n"),
    )
    .unwrap();
    let findings = lint_shared_home(temp.path()).unwrap();
    let finding = findings
        .iter()
        .find(|finding| finding.rule == "VSH004")
        .unwrap();
    assert_eq!(finding.path, PathBuf::from("Knowledge/note.md"));
    assert_eq!(finding.line, 1);
    assert!(!format!("{finding:?}").contains(&marker));
}

#[test]
fn lint_flags_email_addresses_without_echoing_the_address() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("Knowledge")).unwrap();
    std::fs::write(temp.path().join("Knowledge/README.md"), "# Knowledge\n").unwrap();
    let marker = format!("person-{}@example.invalid", "X".repeat(8));
    std::fs::write(
        temp.path().join("Knowledge/contact.md"),
        format!("Contact: {marker}\n"),
    )
    .unwrap();

    let findings = lint_shared_home(temp.path()).unwrap();
    let finding = findings
        .iter()
        .find(|finding| finding.rule == "VSH007")
        .unwrap();
    assert_eq!(finding.path, PathBuf::from("Knowledge/contact.md"));
    assert_eq!(finding.line, 1);
    assert!(!format!("{finding:?}").contains(&marker));
}

#[test]
fn lint_ignores_email_like_text_without_a_valid_domain_suffix() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("Knowledge")).unwrap();
    std::fs::write(temp.path().join("Knowledge/README.md"), "# Knowledge\n").unwrap();
    std::fs::write(
        temp.path().join("Knowledge/note.md"),
        "literal name@localhost and user@example.123\n",
    )
    .unwrap();

    let findings = lint_shared_home(temp.path()).unwrap();
    assert!(!findings.iter().any(|finding| finding.rule == "VSH007"));
}

#[test]
fn lint_ignores_email_addresses_in_non_markdown_files() {
    let temp = tempfile::tempdir().unwrap();
    let marker = format!("person-{}@example.invalid", "X".repeat(8));
    std::fs::write(temp.path().join("data.txt"), format!("{marker}\n")).unwrap();
    let findings = lint_shared_home(temp.path()).unwrap();
    assert!(!findings.iter().any(|finding| finding.rule == "VSH007"));
}

#[test]
fn lint_reports_invalid_frontmatter_yaml_without_echoing_content() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("Knowledge")).unwrap();
    std::fs::write(
        temp.path().join("Knowledge/README.md"),
        "---\ntitle: Knowledge\ncreated: 2026-11-16\nupdated: 2026-11-16\ntype: readme\narea: team\ntags: [index]\nowner: team\nauthority: working\n---\n# Knowledge\n",
    )
    .unwrap();
    let marker = format!("invalid-{}", "X".repeat(12));
    std::fs::write(
        temp.path().join("Knowledge/bad.md"),
        format!("---\ntitle: '{marker}: bad\ncreated: 2026-11-16\nupdated: 2026-11-16\ntype: note\narea: team\ntags: [demo]\nowner: team\nauthority: working\n---\nbody\n"),
    )
    .unwrap();

    let findings = lint_shared_home(temp.path()).unwrap();
    let finding = findings
        .iter()
        .find(|finding| finding.rule == "VSH008")
        .unwrap();
    assert_eq!(finding.path, PathBuf::from("Knowledge/bad.md"));
    assert!(finding.line >= 2);
    assert!(!format!("{finding:?}").contains(&marker));
}

#[test]
fn lint_rejects_missing_owner_in_frontmatter() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("Knowledge")).unwrap();
    std::fs::write(temp.path().join("Knowledge/README.md"), "---\ntitle: Knowledge\ncreated: 2026-11-16\nupdated: 2026-11-16\ntype: readme\narea: team\ntags: [index]\nowner: team\nauthority: working\n---\n# Knowledge\n").unwrap();
    std::fs::write(
        temp.path().join("Knowledge/note.md"),
        "---\ntitle: Note\ncreated: 2026-11-16\nupdated: 2026-11-16\ntype: note\narea: team\ntags: [demo]\nauthority: invalid\n---\ncontent\n",
    )
    .unwrap();

    let findings = lint_shared_home(temp.path()).unwrap();
    assert!(findings.iter().any(|finding| finding.rule == "VSH009"));
}

#[test]
fn lint_rejects_invalid_authority() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("Knowledge")).unwrap();
    std::fs::write(temp.path().join("Knowledge/README.md"), "# Knowledge\n").unwrap();
    std::fs::write(
        temp.path().join("Knowledge/note.md"),
        "---\ntitle: Note\ncreated: 2026-11-16\nupdated: 2026-11-16\ntype: note\narea: team\ntags: [demo]\nowner: team\nauthority: invalid\n---\ncontent\n",
    )
    .unwrap();
    let findings = lint_shared_home(temp.path()).unwrap();
    assert!(findings.iter().any(|finding| finding.rule == "VSH010"));
}

#[test]
fn lint_requires_source_for_mirrors_and_digests() {
    for authority in ["mirror", "digest"] {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("Knowledge")).unwrap();
        std::fs::write(temp.path().join("Knowledge/README.md"), "# Knowledge\n").unwrap();
        std::fs::write(
            temp.path().join("Knowledge/note.md"),
            format!("---\ntitle: Note\ncreated: 2026-11-16\nupdated: 2026-11-16\ntype: note\narea: team\ntags: [demo]\nowner: team\nauthority: {authority}\n---\ncontent\n"),
        )
        .unwrap();
        let findings = lint_shared_home(temp.path()).unwrap();
        assert!(findings.iter().any(|finding| finding.rule == "VSH011"));
    }
}

#[test]
fn lint_accepts_complete_frontmatter_and_source_for_mirror() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("Knowledge")).unwrap();
    std::fs::write(temp.path().join("Knowledge/README.md"), "---\ntitle: Knowledge\ncreated: 2026-11-16\nupdated: 2026-11-16\ntype: readme\narea: team\ntags: [index]\nowner: team\nauthority: working\n---\n# Knowledge\n").unwrap();
    std::fs::write(
        temp.path().join("Knowledge/mirror.md"),
        "---\ntitle: Mirror\ncreated: 2026-11-16\nupdated: 2026-11-16\ntype: note\narea: team\ntags: [demo]\nowner: team\nauthority: mirror\nsource: https://example.invalid/source\n---\ncontent\n",
    )
    .unwrap();

    let findings = lint_shared_home(temp.path()).unwrap();
    assert!(!findings
        .iter()
        .any(|finding| { matches!(finding.rule, "VSH008" | "VSH009" | "VSH010" | "VSH011") }));
}

#[test]
fn lint_requires_frontmatter_for_notes_in_policy_areas() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("Knowledge")).unwrap();
    std::fs::write(temp.path().join("Knowledge/README.md"), "# Knowledge\n").unwrap();
    std::fs::write(temp.path().join("Knowledge/note.md"), "# Note\n").unwrap();

    let findings = lint_shared_home(temp.path()).unwrap();
    assert!(findings.iter().any(|finding| {
        finding.rule == "VSH009" && finding.path == Path::new("Knowledge/note.md")
    }));
}

#[test]
fn lint_rejects_missing_required_fields_and_non_sequence_tags() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("Knowledge")).unwrap();
    std::fs::write(temp.path().join("Knowledge/README.md"), "# Knowledge\n").unwrap();
    std::fs::write(
        temp.path().join("Knowledge/note.md"),
        "---\ntitle: Note\ncreated: 2026-11-16\nupdated: 2026-11-16\ntype: note\narea: team\ntags: plain\nowner: team\nauthority: working\n---\ncontent\n",
    )
    .unwrap();

    let findings = lint_shared_home(temp.path()).unwrap();
    assert!(findings.iter().any(|finding| finding.rule == "VSH009"));
}

#[test]
fn lint_flags_known_secret_pattern_without_emitting_value() {
    let temp = tempfile::tempdir().unwrap();
    let marker = format!("ghp_{}", "A".repeat(32));
    std::fs::write(temp.path().join("AGENTS.md"), format!("safe\n{marker}\n")).unwrap();
    let findings = lint_shared_home(temp.path()).unwrap();
    let finding = findings
        .iter()
        .find(|finding| finding.rule == "VSH006")
        .unwrap();
    assert_eq!(finding.path, PathBuf::from("AGENTS.md"));
    assert_eq!(finding.line, 2);
    assert!(!format!("{finding:?}").contains(&marker));
}
