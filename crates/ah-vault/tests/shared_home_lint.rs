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

/// Characterisation tests pinning frontmatter semantics across YAML parser changes.
fn lint_note(frontmatter: &str) -> Vec<&'static str> {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("Knowledge")).unwrap();
    std::fs::write(
        temp.path().join("Knowledge/README.md"),
        "---\ntitle: Knowledge\ncreated: 2026-11-16\nupdated: 2026-11-16\ntype: readme\narea: team\ntags: [index]\nowner: team\nauthority: working\n---\n# Knowledge\n\n- [[note]]\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Knowledge/note.md"),
        format!("---\n{frontmatter}---\nbody\n"),
    )
    .unwrap();
    let mut rules: Vec<_> = lint_shared_home(temp.path())
        .unwrap()
        .into_iter()
        .filter(|f| f.path == Path::new("Knowledge/note.md"))
        .map(|f| f.rule)
        .collect();
    rules.sort_unstable();
    rules
}

const VALID: &str = "title: Note\ncreated: 2026-11-16\nupdated: 2026-11-16\ntype: note\narea: team\ntags: [demo]\nowner: team\nauthority: working\n";

#[test]
fn frontmatter_semantics_are_stable() {
    assert!(lint_note(VALID).is_empty(), "{:?}", lint_note(VALID));
    // Unquoted ISO dates are accepted as text.
    assert!(lint_note(&VALID.replace("2026-11-16", "'2026-11-16'")).is_empty());
    // A duplicate key makes the metadata ambiguous: invalid YAML.
    assert_eq!(
        lint_note(&format!("{VALID}owner: someone-else\n")),
        ["VSH008"]
    );
    // A non-text title is an invalid required field.
    assert_eq!(
        lint_note(&VALID.replace("title: Note", "title: 2026")),
        ["VSH009"]
    );
    // Block-style tag lists are sequences too.
    assert!(lint_note(&VALID.replace("tags: [demo]", "tags:\n  - demo\n  - other")).is_empty());
    // Empty frontmatter is not a mapping.
    assert_eq!(lint_note(""), ["VSH008"]);
    // A null source on a mirror is missing.
    assert_eq!(
        lint_note(&VALID.replace("authority: working", "authority: mirror\nsource: ~")),
        ["VSH011"]
    );
}

/// A scratch copy of the fixture shared home, for lint cases that need to add files.
fn scratch_home() -> tempfile::TempDir {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    let temp = tempfile::tempdir().unwrap();
    copy(&fixture_home(), temp.path());
    temp
}

fn rules_for(root: &Path, rel: &str) -> Vec<&'static str> {
    lint_shared_home(root)
        .unwrap()
        .into_iter()
        .filter(|f| f.path == Path::new(rel))
        .map(|f| f.rule)
        .collect()
}

#[test]
fn size_limit_ignores_git_metadata() {
    // A real shared-home clone has pack files over 1 MB; they are not shared-home content.
    let home = scratch_home();
    std::fs::create_dir_all(home.path().join(".git/objects/pack")).unwrap();
    std::fs::write(
        home.path().join(".git/objects/pack/pack-1.pack"),
        vec![0u8; 1_500_000],
    )
    .unwrap();
    assert!(
        lint_shared_home(home.path())
            .unwrap()
            .iter()
            .all(|f| !f.path.starts_with(".git")),
        "lint must not inspect .git/"
    );
}

#[test]
fn d28_binaries_must_be_lfs_tracked_and_lfs_files_are_exempt_from_the_size_cap() {
    let home = scratch_home();
    std::fs::create_dir_all(home.path().join("Knowledge/Tools/assets")).unwrap();
    // Covered by the fixture's `*.svg filter=lfs` rule: allowed, and exempt from the 1 MB cap.
    std::fs::write(
        home.path().join("Knowledge/Tools/assets/big.svg"),
        vec![b' '; 2_000_000],
    )
    .unwrap();
    // A D28 type whose LFS rule is removed below: must be flagged even when small.
    std::fs::write(
        home.path().join("Knowledge/Tools/assets/scan.pdf"),
        b"%PDF-1.7",
    )
    .unwrap();
    // Not a D28 type and not LFS: the 1 MB cap still applies.
    std::fs::write(
        home.path().join("Knowledge/Tools/assets/dump.txt"),
        vec![b'x'; 1_500_000],
    )
    .unwrap();
    let attrs = std::fs::read_to_string(home.path().join(".gitattributes")).unwrap();
    std::fs::write(
        home.path().join(".gitattributes"),
        attrs.replace("*.pdf filter=lfs diff=lfs merge=lfs -text\n", ""),
    )
    .unwrap();

    assert!(rules_for(home.path(), "Knowledge/Tools/assets/big.svg").is_empty());
    assert_eq!(
        rules_for(home.path(), "Knowledge/Tools/assets/scan.pdf"),
        ["VSH012"]
    );
    assert_eq!(
        rules_for(home.path(), "Knowledge/Tools/assets/dump.txt"),
        ["VSH002"]
    );

    // A later rule that unsets the filter wins, as in Git.
    std::fs::write(
        home.path().join(".gitattributes"),
        format!("{attrs}Knowledge/**/big.svg -filter\n"),
    )
    .unwrap();
    let big = rules_for(home.path(), "Knowledge/Tools/assets/big.svg");
    assert!(
        big.contains(&"VSH012") && big.contains(&"VSH002"),
        "{big:?}"
    );
}

#[test]
fn skill_lint_checks_name_folder_and_description() {
    let home = scratch_home();
    let skill = |dir: &str, body: &str| {
        let path = home.path().join(dir);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("SKILL.md"), body).unwrap();
    };
    skill(
        "Agents/Skills/good-skill",
        "---\nname: good-skill\ndescription: Use when testing.\n---\nbody\n",
    );
    skill(
        "Agents/Skills/renamed",
        "---\nname: other-name\ndescription: Use when testing.\n---\n",
    );
    skill("Agents/Skills/no-desc", "---\nname: no-desc\n---\n");
    skill(
        "Agents/Skills/blank-desc",
        "---\nname: blank-desc\ndescription: \"  \"\n---\n",
    );
    skill("Agents/Skills/no-frontmatter", "just text\n");
    skill(
        "Standards/Skills/bad-yaml",
        "---\nname: [unclosed\ndescription: x\n---\n",
    );
    skill(
        "Agents/Hermes/Skills/hermes-x",
        "---\nname: hermes-x\ndescription: Use when orchestrating.\n---\n",
    );
    // Outside Agents/ and Standards/, SKILL.md files are not skill-lint's business.
    skill("Knowledge/Tools/not-a-skill", "---\ntitle: x\n---\n");

    assert!(rules_for(home.path(), "Agents/Skills/good-skill/SKILL.md").is_empty());
    assert!(rules_for(home.path(), "Agents/Hermes/Skills/hermes-x/SKILL.md").is_empty());
    assert_eq!(
        rules_for(home.path(), "Agents/Skills/renamed/SKILL.md"),
        ["VSH014"]
    );
    assert_eq!(
        rules_for(home.path(), "Agents/Skills/no-desc/SKILL.md"),
        ["VSH015"]
    );
    assert_eq!(
        rules_for(home.path(), "Agents/Skills/blank-desc/SKILL.md"),
        ["VSH015"]
    );
    assert_eq!(
        rules_for(home.path(), "Agents/Skills/no-frontmatter/SKILL.md"),
        ["VSH013"]
    );
    assert_eq!(
        rules_for(home.path(), "Standards/Skills/bad-yaml/SKILL.md"),
        ["VSH013"]
    );
    assert!(
        !rules_for(home.path(), "Knowledge/Tools/not-a-skill/SKILL.md")
            .iter()
            .any(|r| matches!(*r, "VSH013" | "VSH014" | "VSH015"))
    );
}
