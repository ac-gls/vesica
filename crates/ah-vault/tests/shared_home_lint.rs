use ah_vault::lint_shared_home;
use std::path::PathBuf;

fn fixture_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home")
}

#[test]
fn lint_accepts_the_shared_home_fixture() {
    let findings = lint_shared_home(&fixture_home()).unwrap();
    assert!(findings.is_empty(), "unexpected findings: {findings:#?}");
}

#[test]
fn lint_reports_missing_readme_with_safe_path_and_rule_code() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("Knowledge/Guides")).unwrap();
    std::fs::write(temp.path().join("Knowledge/README.md"), "# Knowledge\n").unwrap();
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
