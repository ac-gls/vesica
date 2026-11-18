//! The published JSON schemas (`schemas/*.schema.json`) must accept and reject exactly what the
//! CLI's parsers accept and reject. Each case runs through both the real parser and the schema.

use ah_core::{AgentHomeConfig, FoundryCatalog};
use std::path::PathBuf;

fn repo_file(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn compile(schema_file: &str) -> (boon::Schemas, boon::SchemaIndex) {
    let path = repo_file(schema_file);
    let schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut schemas = boon::Schemas::new();
    let mut compiler = boon::Compiler::new();
    compiler.enable_format_assertions();
    let url = format!("file://{}", path.display());
    compiler.add_resource(&url, schema).unwrap();
    let index = compiler.compile(&url, &mut schemas).unwrap();
    (schemas, index)
}

fn check(
    kind: &str,
    cases: &[(&str, bool)],
    parser_ok: impl Fn(&str) -> bool,
    schema_ok: impl Fn(&str) -> bool,
) {
    let mut failures = Vec::new();
    for (i, (input, valid)) in cases.iter().enumerate() {
        let (parser, schema) = (parser_ok(input), schema_ok(input));
        if parser != *valid || schema != *valid {
            failures.push(format!(
                "{kind} case {i} (expected valid={valid}): parser={parser} schema={schema}\n{input}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}

const TEAM: &str =
    "[team]\nname = \"T\"\norg = \"o\"\nenv_prefix = \"T_1\"\nrole_prefix = \"t-\"\n";
const PROFILES: &str = "[profiles]\ndefault = \"developer\"\n";

fn config(extra: &str) -> String {
    format!("schema = 1\n{TEAM}{PROFILES}{extra}")
}

fn repo(fields: &str) -> String {
    config(&format!("[[repos]]\n{fields}\n"))
}

#[test]
fn agent_home_schema_matches_parser() {
    let fixture =
        std::fs::read_to_string(repo_file("tests/fixtures/shared-home/agent-home.toml")).unwrap();
    let ok_repo = "name = \"app\"\nurl = \"https://example.invalid/app.git\"";
    let cases: Vec<(String, bool)> = vec![
        (fixture, true),
        (config(""), true),
        (
            config("[minimum]\nnode = \"22.0.0\"\n[hermes]\nprofile = \"p\"\n"),
            true,
        ),
        (repo(ok_repo), true),
        (repo(&format!("{ok_repo}\nprofiles = []")), true),
        (repo(&format!("{ok_repo}\nprofiles = [\"lead\"]")), true),
        (
            repo("name = \"a_b.c-d\"\nurl = \"git@example.invalid:o/a.git\""),
            true,
        ),
        (config("[future]\nsetting = true\n"), true),
        (config("").replace("schema = 1", "schema = 2"), false),
        (format!("schema = 1\n{PROFILES}"), false),
        (format!("schema = 1\n{TEAM}"), false),
        (format!("{TEAM}{PROFILES}"), false),
        (config("").replace("name = \"T\"", "name = \"   \""), false),
        (
            config("").replace("role_prefix = \"t-\"", "role_prefix = \"\""),
            false,
        ),
        (
            config("").replace("env_prefix = \"T_1\"", "env_prefix = \"T-1\""),
            false,
        ),
        (config("").replace("org = \"o\"\n", ""), false),
        (config("").replace("\"developer\"", "\"admin\""), false),
        (config("[minimum]\nnode = 22\n"), false),
        (config("[hermes]\nprofile = 1\n"), false),
        (repo("name = \"app\""), false),
        (repo("name = \".hidden\"\nurl = \"u\""), false),
        (repo("name = \"-x\"\nurl = \"u\""), false),
        (repo("name = \"a/b\"\nurl = \"u\""), false),
        (repo("name = \"\"\nurl = \"u\""), false),
        (repo("name = \"app\"\nurl = \"\""), false),
        (repo("name = \"app\"\nurl = \"   \""), false),
        (
            repo("name = \"app\"\nurl = \"--upload-pack=touch x\""),
            false,
        ),
        (repo("name = \"app\"\nurl = \" -u\""), false),
        (repo("name = \"app\"\nurl = \"ext::sh -c x\""), false),
        (repo("name = \"app\"\nurl = \" EXT::sh\""), false),
        (repo("name = \"app\"\nurl = \"fd::3\""), false),
        (repo(&format!("{ok_repo}\nprofiles = [\"core\"]")), false),
        (repo(&format!("{ok_repo}\nprofiles = [\"admin\"]")), false),
        (
            config("").replace("[profiles]", "api_key = \"x\"\n[profiles]"),
            false,
        ),
        (config("[extra]\nclient-secret = \"x\"\n"), false),
        (config("[extra.deeper]\nAccess_Token = \"x\"\n"), false),
        (config("[[extra]]\nPassword = \"x\"\n"), false),
        (repo(&format!("{ok_repo}\nsecret = \"x\"")), false),
        (config("[extra]\nmy-apikey-name = \"x\"\n"), false),
        (
            config("[extra]\nsecretary = \"allowed\"\npassword_hint = \"allowed\"\n"),
            true,
        ),
    ];
    let (schemas, index) = compile("schemas/agent-home.schema.json");
    let cases: Vec<(&str, bool)> = cases.iter().map(|(s, v)| (s.as_str(), *v)).collect();
    check(
        "agent-home.toml",
        &cases,
        |input| AgentHomeConfig::parse(input).is_ok(),
        |input| {
            let value: toml::Value = toml::from_str(input).expect("corpus is valid TOML");
            schemas
                .validate(&serde_json::to_value(value).unwrap(), index)
                .is_ok()
        },
    );
}

#[test]
fn foundry_catalog_schema_matches_parser() {
    let fixture =
        std::fs::read_to_string(repo_file("tests/fixtures/shared-home/foundry.catalog.json"))
            .unwrap();
    let cases: Vec<(&str, bool)> = vec![
        (&fixture, true),
        ("{}", true),
        (r#"{"tenant": null}"#, true),
        (
            r#"{"$schema": "../schemas/foundry.catalog.schema.json"}"#,
            true,
        ),
        (r#"{"resources": {}, "notes": ["names only"]}"#, true),
        ("[]", false),
        (
            r#"["00000000-0000-0000-0000-000000000000", {}, {}, {}]"#,
            false,
        ),
        ("5", false),
        ("null", false),
        (r#"{"tenant": 5}"#, false),
        (r#"{"resources": {"openai": 1}}"#, false),
        (r#"{"deployments": []}"#, false),
        (r#"{"defaults": null}"#, false),
        (r#"{"api_key": "x"}"#, false),
        (r#"{"resources": {"apiKey": "x"}}"#, false),
        (r#"{"extra": [{"refresh-token": "y"}]}"#, false),
        (r#"{"Secret": "x"}"#, false),
    ];
    let (schemas, index) = compile("schemas/foundry.catalog.schema.json");
    check(
        "foundry.catalog.json",
        &cases,
        |input| FoundryCatalog::parse(input).is_ok(),
        |input| {
            let value: serde_json::Value = serde_json::from_str(input).expect("valid JSON");
            schemas.validate(&value, index).is_ok()
        },
    );
}
