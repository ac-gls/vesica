//! `vesica doctor --json` must satisfy `schemas/doctor.schema.json` (F3.5). The report is the
//! machine-readable contract for onboarding checks, so both the green and the blocked report are
//! validated, and mutations of the document must be rejected.

use assert_cmd::Command;
use std::path::PathBuf;

fn fixture_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/shared-home")
}

fn schema() -> (boon::Schemas, boon::SchemaIndex) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/doctor.schema.json");
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut schemas = boon::Schemas::new();
    let mut compiler = boon::Compiler::new();
    let url = format!("file://{}", path.display());
    compiler.add_resource(&url, value).unwrap();
    let index = compiler.compile(&url, &mut schemas).unwrap();
    (schemas, index)
}

/// A `PATH` directory holding symlinks to every given real command.
fn stub_path(keep: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for command in keep {
        let which = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {command}"))
            .output()
            .unwrap();
        let real = String::from_utf8(which.stdout).unwrap().trim().to_owned();
        if real.is_empty() {
            continue;
        }
        std::os::unix::fs::symlink(&real, dir.path().join(command)).unwrap();
    }
    dir
}

fn doctor_json(path: &std::path::Path, extra: &[&str]) -> (bool, serde_json::Value) {
    let out = Command::cargo_bin("vesica")
        .unwrap()
        .args(["doctor", "--json", "--home-repo"])
        .arg(fixture_home())
        .args(extra)
        .env("PATH", path)
        .output()
        .unwrap();
    (
        out.status.success(),
        serde_json::from_slice(&out.stdout).expect("doctor --json must emit JSON"),
    )
}

fn validate(schemas: &boon::Schemas, index: boon::SchemaIndex, value: &serde_json::Value) -> bool {
    schemas.validate(value, index).is_ok()
}

#[test]
fn doctor_json_matches_the_schema_for_the_ok_and_blocked_reports() {
    let (schemas, index) = schema();
    let full_path = std::env::var("PATH").unwrap();

    let (ok, report) = doctor_json(std::path::Path::new(&full_path), &[]);
    if ok {
        assert!(validate(&schemas, index, &report), "ok report: {report:#}");
        assert_eq!(report["status"], "ok");
        assert_eq!(report["blockers"].as_array().unwrap().len(), 0);
    } else {
        // The container may legitimately lack a required command; then blockers must explain it.
        assert_eq!(report["status"], "blocked");
        assert!(!report["blockers"].as_array().unwrap().is_empty());
        assert!(
            validate(&schemas, index, &report),
            "blocked report: {report:#}"
        );
    }

    // A blocked report from a PATH without `git`.
    let stub = stub_path(&["gh", "curl", "sha256sum"]);
    let (blocked_ok, blocked) = doctor_json(stub.path(), &[]);
    assert!(!blocked_ok, "a missing required command must exit non-zero");
    assert_eq!(blocked["status"], "blocked");
    assert!(blocked["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b.as_str().unwrap_or_default().contains("git")));
    assert!(
        validate(&schemas, index, &blocked),
        "blocked report: {blocked:#}"
    );

    // A Git identity is reported, because vault snapshot, new-skill and share commit with it.
    // It is informational: install works without one, so it must never be a blocker.
    let identity = report["prerequisites"]["optional"]["git identity"]
        .as_str()
        .expect("doctor must report whether a Git identity is available");
    assert!(
        identity.starts_with("found:") || identity.starts_with("missing"),
        "unexpected git identity status: {identity}"
    );
    assert!(
        !report["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b.as_str().unwrap_or_default().contains("identity")),
        "a missing Git identity must not be a blocker: {report:#}"
    );

    // The selected profile is reflected in the report.
    let (lead_ok, lead) = doctor_json(std::path::Path::new(&full_path), &["--profile", "lead"]);
    let _ = lead_ok;
    assert_eq!(lead["profile"], "lead");
    assert!(validate(&schemas, index, &lead), "lead report: {lead:#}");

    // Mutations of a valid report must be rejected, so the schema really constrains.
    let mut invalid = Vec::new();
    for (name, mutate) in [
        (
            "unknown status",
            Box::new(|r: &mut serde_json::Value| r["status"] = "green".into())
                as Box<dyn Fn(&mut serde_json::Value)>,
        ),
        (
            "blockers as a string",
            Box::new(|r: &mut serde_json::Value| r["blockers"] = "none".into()),
        ),
        (
            "missing profile",
            Box::new(|r: &mut serde_json::Value| {
                r.as_object_mut().unwrap().remove("profile");
            }),
        ),
        (
            "unknown profile",
            Box::new(|r: &mut serde_json::Value| r["profile"] = "admin".into()),
        ),
        (
            "repos with an object",
            Box::new(|r: &mut serde_json::Value| r["repos"] = serde_json::json!([{"name": "x"}])),
        ),
        (
            "negative count",
            Box::new(|r: &mut serde_json::Value| r["repos_in_config"] = (-1).into()),
        ),
        (
            "wsl_kernel guess",
            Box::new(|r: &mut serde_json::Value| r["wsl_kernel"] = "maybe".into()),
        ),
        (
            "herdr reported green",
            Box::new(|r: &mut serde_json::Value| r["herdr"] = "ok".into()),
        ),
        (
            "missing platform",
            Box::new(|r: &mut serde_json::Value| {
                r.as_object_mut().unwrap().remove("platform");
            }),
        ),
        (
            "required entry unknown",
            Box::new(|r: &mut serde_json::Value| {
                r["prerequisites"]["required"]["git"] = "maybe".into()
            }),
        ),
        (
            "missing a required command",
            Box::new(|r: &mut serde_json::Value| {
                r["prerequisites"]["required"]
                    .as_object_mut()
                    .unwrap()
                    .remove("curl");
            }),
        ),
        (
            "team without org",
            Box::new(|r: &mut serde_json::Value| {
                r["team"].as_object_mut().unwrap().remove("org");
            }),
        ),
        (
            "shared_home unknown",
            Box::new(|r: &mut serde_json::Value| r["shared_home"] = "unsure".into()),
        ),
    ] {
        let mut mutated = report.clone();
        mutate(&mut mutated);
        if validate(&schemas, index, &mutated) {
            invalid.push(name);
        }
    }
    assert!(
        invalid.is_empty(),
        "schema accepted mutated reports: {invalid:?}"
    );
}
