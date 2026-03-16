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
