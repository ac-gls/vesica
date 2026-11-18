//! B2 acceptance (plan §8): comments and formatting are preserved byte-for-byte outside the
//! edited span. Exact-byte assertions pin the untouched text; `insta` snapshots make every
//! full output reviewable.

use ah_config::{jsonc::merge_jsonc, toml_edit::merge_toml};
use serde_json::json;

/// Realistic OpenCode-style JSONC: line, block and trailing comments plus trailing commas.
const OPENCODE: &str = "{\n  // user's OpenCode settings\n  \"$schema\": \"https://opencode.ai/config.json\",\n  /* block comment\n     spanning lines */\n  \"theme\": \"tokyonight\", // trailing comment\n  \"provider\": {\n    \"anthropic\": { \"options\": { \"timeout\": 600 } }, // keep\n  },\n  \"instructions\": [\"~/old.md\"],\n}\n";

/// Realistic Codex-style TOML: file header comment, inline comments, comment above a key.
const CODEX: &str = "# Codex config\nmodel = \"gpt-5\"  # inline note\n\n[history]\n# why we persist\npersistence = \"save-all\" # keep\n\n# providers below\n[model_providers.openai]\nname = \"OpenAI\"\n";

#[test]
fn jsonc_replacing_a_value_changes_only_that_value() {
    let out = merge_jsonc(OPENCODE, &json!({"instructions": ["~/Team/AGENTS.md"]})).unwrap();
    assert_eq!(
        out,
        OPENCODE.replace(r#"["~/old.md"]"#, r#"["~/Team/AGENTS.md"]"#)
    );
}

#[test]
fn jsonc_adding_a_nested_object_keeps_every_existing_byte() {
    let out = merge_jsonc(
        OPENCODE,
        &json!({"provider": {"azure": {"enabled": false}}}),
    )
    .unwrap();
    let anchor = "    \"anthropic\": { \"options\": { \"timeout\": 600 } }, // keep\n";
    let (before, after) = OPENCODE.split_once(anchor).unwrap();
    assert!(out.starts_with(&format!("{before}{anchor}")), "{out}");
    assert!(out.ends_with(after), "{out}");
    insta::assert_snapshot!("jsonc_add_nested_object", out);
}

#[test]
fn jsonc_appending_a_top_level_key_keeps_every_existing_byte() {
    let out = merge_jsonc(OPENCODE, &json!({"vesica": true})).unwrap();
    let body = OPENCODE.strip_suffix("}\n").unwrap();
    assert!(out.starts_with(body), "{out}");
    insta::assert_snapshot!("jsonc_append_top_level_key", out);
}

#[test]
fn toml_replacing_a_top_level_value_keeps_header_and_inline_comments() {
    let out = merge_toml(CODEX, &json!({"model": "gpt-codex"})).unwrap();
    assert_eq!(out, CODEX.replace("\"gpt-5\"", "\"gpt-codex\""));
}

#[test]
fn toml_replacing_a_nested_value_keeps_the_comment_above_and_beside_it() {
    let out = merge_toml(CODEX, &json!({"history": {"persistence": "none"}})).unwrap();
    assert_eq!(out, CODEX.replace("\"save-all\"", "\"none\""));
}

#[test]
fn toml_adding_a_table_appends_without_touching_existing_bytes() {
    let out = merge_toml(
        CODEX,
        &json!({"model_providers": {"azure": {
            "name": "Azure",
            "base_url": "https://example.openai.azure.com/openai/v1",
            "wire_api": "responses"
        }}}),
    )
    .unwrap();
    assert!(out.starts_with(CODEX), "{out}");
    insta::assert_snapshot!("toml_add_table", out);
}

#[test]
fn merges_are_idempotent_on_realistic_documents() {
    let jpatch =
        json!({"instructions": ["~/Team/AGENTS.md"], "provider": {"azure": {"enabled": false}}});
    let once = merge_jsonc(OPENCODE, &jpatch).unwrap();
    assert_eq!(merge_jsonc(&once, &jpatch).unwrap(), once);
    let tpatch = json!({"model": "gpt-codex", "model_providers": {"azure": {"name": "Azure"}}});
    let once = merge_toml(CODEX, &tpatch).unwrap();
    assert_eq!(merge_toml(&once, &tpatch).unwrap(), once);
}
