use std::{fs, process::Command};

use serde_json::Value;
use tempfile::tempdir;

fn write_json_fixture() -> tempfile::TempDir {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "fixture"
version = "0.1.0"
edition = "2024"

[package.metadata.modum]
generic_nouns = ["Repository"]
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use app::user::Repository;

mod app {
    pub mod user {
        pub struct Repository;
    }
}
"#,
    )
    .expect("write source");
    temp
}

fn write_qualified_generic_fixture() -> tempfile::TempDir {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "fixture"
version = "0.1.0"
edition = "2024"
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod response {
    pub struct Response;
}

pub fn handler() -> response::Response {
    todo!()
}
"#,
    )
    .expect("write source");
    temp
}

fn write_policy_and_advisory_fixture() -> tempfile::TempDir {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "fixture"
version = "0.1.0"
edition = "2024"
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod response {
    pub struct Response;
}

pub struct UserRepository;
pub struct UserService;
pub struct UserId;

pub fn handler() -> response::Response {
    todo!()
}
"#,
    )
    .expect("write source");
    temp
}

fn write_workspace_exclude_fixture() -> tempfile::TempDir {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("app/src")).expect("create app src");
    fs::create_dir_all(root.join("examples/high-coverage/src")).expect("create fixture src");

    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "2"
members = ["app", "examples/high-coverage"]

[workspace.metadata.modum]
exclude = ["examples/high-coverage"]
"#,
    )
    .expect("write workspace manifest");
    fs::write(
        root.join("app/Cargo.toml"),
        r#"[package]
name = "app"
version = "0.1.0"
edition = "2024"
"#,
    )
    .expect("write app manifest");
    fs::write(root.join("app/src/lib.rs"), "use http::StatusCode;\n").expect("write app source");
    fs::write(
        root.join("examples/high-coverage/Cargo.toml"),
        r#"[package]
name = "high-coverage"
version = "0.1.0"
edition = "2024"
"#,
    )
    .expect("write fixture manifest");
    fs::write(
        root.join("examples/high-coverage/src/lib.rs"),
        "use http::StatusCode;\n",
    )
    .expect("write fixture source");

    temp
}

fn write_invalid_generic_nouns_fixture() -> tempfile::TempDir {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "fixture"
version = "0.1.0"
edition = "2024"

[package.metadata.modum]
generic_nouns = [1]
"#,
    )
    .expect("write manifest");
    fs::write(root.join("src/lib.rs"), "pub struct UserRepository;\n").expect("write source");
    temp
}

fn write_invalid_scan_defaults_fixture() -> tempfile::TempDir {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "fixture"
version = "0.1.0"
edition = "2024"

[package.metadata.modum]
exclude = ["src", 1]
"#,
    )
    .expect("write manifest");
    fs::write(root.join("src/lib.rs"), "pub struct UserRepository;\n").expect("write source");
    temp
}

#[test]
fn cli_help_shows_lint_only_surface() {
    let check_help = Command::new(env!("CARGO_BIN_EXE_modum"))
        .args(["check", "--help"])
        .output()
        .expect("run modum check --help");
    assert_eq!(check_help.status.code(), Some(0));
    let check_help_out = String::from_utf8_lossy(&check_help.stdout);
    assert!(check_help_out.contains("Usage:"));
    assert!(check_help_out.contains("MODUM=off|warn|deny"));
    assert!(check_help_out.contains("modum check"));

    let top_help = Command::new(env!("CARGO_BIN_EXE_modum"))
        .arg("--help")
        .output()
        .expect("run modum --help");
    assert_eq!(top_help.status.code(), Some(0));
    let top_help_out = String::from_utf8_lossy(&top_help.stdout);
    assert!(top_help_out.contains("Commands:"));
    assert!(top_help_out.contains("check"));
    assert!(!top_help_out.contains("fix"));
}

#[test]
fn cargo_subcommand_help_shows_lint_only_surface() {
    let check_help = Command::new(env!("CARGO_BIN_EXE_cargo-modum"))
        .args(["modum", "check", "--help"])
        .output()
        .expect("run cargo-modum modum check --help");
    assert_eq!(check_help.status.code(), Some(0));
    let check_help_out = String::from_utf8_lossy(&check_help.stdout);
    assert!(check_help_out.contains("Usage:"));
    assert!(check_help_out.contains("MODUM=off|warn|deny"));
    assert!(check_help_out.contains("cargo modum check"));

    let top_help = Command::new(env!("CARGO_BIN_EXE_cargo-modum"))
        .args(["modum", "--help"])
        .output()
        .expect("run cargo-modum modum --help");
    assert_eq!(top_help.status.code(), Some(0));
    let top_help_out = String::from_utf8_lossy(&top_help.stdout);
    assert!(top_help_out.contains("Commands:"));
    assert!(top_help_out.contains("check"));
    assert!(!top_help_out.contains("fix"));
}

#[test]
fn cli_json_output_reports_policy_diagnostics() {
    let temp = write_json_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--format",
            "json",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(2));

    let json: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    assert_eq!(json["exit_code"].as_u64(), Some(2));
    assert_eq!(
        json["report"]["diagnostics"][0]["code"].as_str(),
        Some("namespace_flat_use")
    );
}

#[test]
fn cargo_subcommand_json_output_reports_policy_diagnostics() {
    let temp = write_json_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_cargo-modum"))
        .args([
            "modum",
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--format",
            "json",
        ])
        .output()
        .expect("run cargo-modum modum check");
    assert_eq!(output.status.code(), Some(2));

    let json: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    assert_eq!(json["exit_code"].as_u64(), Some(2));
    assert_eq!(
        json["report"]["diagnostics"][0]["code"].as_str(),
        Some("namespace_flat_use")
    );
}

#[test]
fn cli_json_output_includes_fix_metadata_for_direct_path_rewrites() {
    let temp = write_qualified_generic_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--format",
            "json",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(2));

    let json: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    let diag = &json["report"]["diagnostics"][0];
    assert_eq!(
        diag["code"].as_str(),
        Some("namespace_redundant_qualified_generic")
    );
    assert_eq!(diag["policy"].as_bool(), Some(true));
    assert_eq!(diag["fix"]["kind"].as_str(), Some("replace_path"));
    assert_eq!(diag["fix"]["replacement"].as_str(), Some("Response"));
}

#[test]
fn cli_text_output_can_filter_to_advisories() {
    let temp = write_policy_and_advisory_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--show",
            "advisory",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(2));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("showing: advisory diagnostics and errors only"));
    assert!(stdout.contains("Advisory Diagnostics:"));
    assert!(stdout.contains("api_candidate_semantic_module"));
    assert!(!stdout.contains("Policy Diagnostics:"));
    assert!(!stdout.contains("namespace_redundant_qualified_generic"));
}

#[test]
fn cli_respects_workspace_metadata_exclude_defaults() {
    let temp = write_workspace_exclude_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--format",
            "json",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(2));

    let json: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    assert_eq!(json["report"]["scanned_files"].as_u64(), Some(1));
    assert_eq!(
        json["report"]["diagnostics"].as_array().map(Vec::len),
        Some(1)
    );
    let file = json["report"]["diagnostics"][0]["file"]
        .as_str()
        .expect("diagnostic file");
    assert!(file.ends_with("/app/src/lib.rs"));
    assert!(!file.contains("examples/high-coverage"));
}

#[test]
fn cli_invalid_exclude_pattern_reports_error() {
    let temp = write_json_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--exclude",
            "[",
            "--format",
            "json",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(1));

    let json: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    assert_eq!(json["exit_code"].as_u64(), Some(1));
    assert!(
        json["report"]["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|diag| {
                diag["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("invalid exclude pattern `[`"))
            })
    );
}

#[test]
fn cli_invalid_generic_nouns_metadata_reports_error() {
    let temp = write_invalid_generic_nouns_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--format",
            "json",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(1));

    let json: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    assert_eq!(json["exit_code"].as_u64(), Some(1));
    assert!(
        json["report"]["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|diag| {
                diag["message"].as_str().is_some_and(|message| {
                    message.contains("`metadata.modum.generic_nouns[0]` must be a string")
                })
            })
    );
}

#[test]
fn cli_invalid_scan_default_metadata_reports_error() {
    let temp = write_invalid_scan_defaults_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--format",
            "json",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(1));

    let json: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    assert_eq!(json["exit_code"].as_u64(), Some(1));
    assert!(
        json["report"]["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|diag| {
                diag["message"].as_str().is_some_and(|message| {
                    message.contains("`metadata.modum.exclude[1]` must be a string")
                })
            })
    );
}
