use std::{fs, process::Command};

use serde_json::Value;
use tempfile::tempdir;

fn strip_ansi_codes(raw: &str) -> String {
    let mut stripped = String::new();
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            stripped.push(ch);
            continue;
        }

        if chars.next_if_eq(&'[').is_none() {
            continue;
        }

        for next in chars.by_ref() {
            if ('@'..='~').contains(&next) {
                break;
            }
        }
    }

    stripped
}

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

fn write_pretty_highlight_fixture() -> tempfile::TempDir {
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
pub struct RequestError {
    pub message: String,
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RequestError {}

pub fn load() -> std::result::Result<(), RequestError> {
    todo!()
}

pub mod workflow {
    pub mod inbound {
        pub mod source_update {
            pub trait SourceUpdate {}
        }
    }
}

pub fn keep(
    boundary: &dyn workflow::inbound::source_update::SourceUpdate,
) -> &dyn workflow::inbound::source_update::SourceUpdate {
    boundary
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

fn write_source_family_fixture() -> tempfile::TempDir {
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
pub struct SourceRuntime;
pub struct SourceUpdate;
pub struct SourceVendor;
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

fn write_invalid_profile_metadata_fixture() -> tempfile::TempDir {
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
profile = "default"
"#,
    )
    .expect("write manifest");
    fs::write(root.join("src/lib.rs"), "pub struct UserRepository;\n").expect("write source");
    temp
}

fn write_invalid_ignored_diagnostic_codes_fixture() -> tempfile::TempDir {
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
ignored_diagnostic_codes = ["not_a_real_code"]
"#,
    )
    .expect("write manifest");
    fs::write(root.join("src/lib.rs"), "pub struct UserRepository;\n").expect("write source");
    temp
}

fn write_invalid_semantic_scalar_metadata_fixture() -> tempfile::TempDir {
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
ignored_semantic_string_scalars = [1]
"#,
    )
    .expect("write manifest");
    fs::write(root.join("src/lib.rs"), "pub fn connect(url: String) {}\n").expect("write source");
    temp
}

fn write_invalid_namespace_preserving_override_metadata_fixture() -> tempfile::TempDir {
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
ignored_namespace_preserving_modules = [1]
"#,
    )
    .expect("write manifest");
    fs::write(root.join("src/lib.rs"), "use components::Button;\n").expect("write source");
    temp
}

fn write_surface_and_strict_fixture() -> tempfile::TempDir {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/components")).expect("create src");
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
pub struct UserRepository;
pub struct UserService;
pub struct UserId;

pub mod components;
"#,
    )
    .expect("write lib");
    fs::write(root.join("src/components.rs"), "pub mod button;\n").expect("write components");
    fs::write(
        root.join("src/components/button.rs"),
        "pub struct Button;\n",
    )
    .expect("write button");
    temp
}

fn write_profile_metadata_fixture(profile: &str) -> tempfile::TempDir {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/components")).expect("create src");
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = "fixture"
version = "0.1.0"
edition = "2024"

[package.metadata.modum]
profile = "{profile}"
"#
        ),
    )
    .expect("write manifest");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct UserRepository;
pub struct UserService;
pub struct UserId;

pub mod components;
"#,
    )
    .expect("write lib");
    fs::write(root.join("src/components.rs"), "pub mod button;\n").expect("write components");
    fs::write(
        root.join("src/components/button.rs"),
        "pub struct Button;\n",
    )
    .expect("write button");
    temp
}

fn write_metadata_baseline_fixture() -> tempfile::TempDir {
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
baseline = ".modum-baseline.json"
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct UserRepository;
pub struct UserService;
pub struct UserId;
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
    assert!(check_help_out.contains("--profile core|surface|strict"));
    assert!(check_help_out.contains("--ignore <code>"));
    assert!(check_help_out.contains("--baseline <path>"));
    assert!(check_help_out.contains("--write-baseline <path>"));
    assert!(check_help_out.contains("--write-markdown-report|-w"));
    assert!(check_help_out.contains("--pretty|-p"));
    assert!(check_help_out.contains("check -w"));
    assert!(check_help_out.contains("check -p"));
    assert!(check_help_out.contains("--explain <code>"));

    let top_help = Command::new(env!("CARGO_BIN_EXE_modum"))
        .arg("--help")
        .output()
        .expect("run modum --help");
    assert_eq!(top_help.status.code(), Some(0));
    let top_help_out = String::from_utf8_lossy(&top_help.stdout);
    assert!(top_help_out.contains("Commands:"));
    assert!(top_help_out.contains("check"));
    assert!(top_help_out.contains("--explain <code>"));
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
    assert!(check_help_out.contains("--profile core|surface|strict"));
    assert!(check_help_out.contains("--ignore <code>"));
    assert!(check_help_out.contains("--baseline <path>"));
    assert!(check_help_out.contains("--write-baseline <path>"));
    assert!(check_help_out.contains("--write-markdown-report|-w"));
    assert!(check_help_out.contains("--pretty|-p"));
    assert!(check_help_out.contains("check -w"));
    assert!(check_help_out.contains("check -p"));
    assert!(check_help_out.contains("--explain <code>"));

    let top_help = Command::new(env!("CARGO_BIN_EXE_cargo-modum"))
        .args(["modum", "--help"])
        .output()
        .expect("run cargo-modum modum --help");
    assert_eq!(top_help.status.code(), Some(0));
    let top_help_out = String::from_utf8_lossy(&top_help.stdout);
    assert!(top_help_out.contains("Commands:"));
    assert!(top_help_out.contains("check"));
    assert!(top_help_out.contains("--explain <code>"));
    assert!(!top_help_out.contains("fix"));
}

#[test]
fn cli_json_output_reports_policy_diagnostics() {
    let temp = write_json_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
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
    assert_eq!(
        json["report"]["diagnostics"][0]["profile"].as_str(),
        Some("core")
    );
}

#[test]
fn cargo_subcommand_json_output_reports_policy_diagnostics() {
    let temp = write_json_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_cargo-modum"))
        .current_dir(root)
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
        .current_dir(root)
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
    let diag = json["report"]["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .find(|diag| diag["code"].as_str() == Some("namespace_redundant_qualified_generic"))
        .expect("qualified generic diagnostic");
    assert_eq!(
        diag["code"].as_str(),
        Some("namespace_redundant_qualified_generic")
    );
    assert_eq!(diag["policy"].as_bool(), Some(true));
    assert_eq!(diag["fix"]["kind"].as_str(), Some("replace_path"));
    assert_eq!(diag["fix"]["replacement"].as_str(), Some("Response"));
    assert!(diag["guidance"]["why"]
        .as_str()
        .is_some_and(|text| text.contains("generic category")));
    assert!(diag["guidance"]["address"]
        .as_str()
        .is_some_and(|text| text.contains("direct rewrite is `Response`")));
}

#[test]
fn cli_pretty_flag_requires_text_output() {
    let temp = write_json_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--format",
            "json",
            "-p",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--pretty is only available with text output"));
}

#[test]
fn cli_text_output_can_filter_to_advisories() {
    let temp = write_policy_and_advisory_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
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
fn cli_text_output_shows_profile_and_fix_hint() {
    let temp = write_qualified_generic_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args(["check", "--root", root.to_str().expect("utf8 root")])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(2));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("namespace_redundant_qualified_generic, core"));
    assert!(stdout.contains("[fix: Response]"));
    assert!(stdout.contains("why: The qualifier repeats a generic category"));
    assert!(stdout.contains("address: Use the nearer parent surface"));
}

#[test]
fn cli_pretty_output_adds_formatting_and_color() {
    let temp = write_policy_and_advisory_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args(["check", "--root", root.to_str().expect("utf8 root"), "-p"])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(2));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\u{1b}[1;30;43m"));
    assert!(stdout.contains("\u{1b}[1;30;42m"));
    assert!(stdout.contains("\u{1b}[1;37;44m"));

    let stripped = strip_ansi_codes(&stdout);
    assert!(stripped.contains("Files scanned"));
    assert!(stripped.contains("Policy Diagnostics"));
    assert!(stripped.contains("Advisory Diagnostics"));
    assert!(stripped.contains("namespace_redundant_qualified_generic"));
    assert!(stripped.contains("api_candidate_semantic_module"));
    assert!(stripped.contains("LINT"));
    assert!(stripped.contains("WHY"));
    assert!(stripped.contains("ADDRESS"));
    assert!(stripped.contains("CHANGE"));
    assert!(stripped.contains("replace with"));
    assert!(stripped.contains("FILE"));
}

#[test]
fn cli_pretty_output_highlights_problem_and_recommended_code_spans() {
    let temp = write_pretty_highlight_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "-p",
            "--mode",
            "warn",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\u{1b}[1;31mRequestError\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;34mDisplay\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;34mError\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;31msource_update\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;31mSourceUpdate\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;36minbound\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;33m::\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;32mSourceUpdate\u{1b}[0m"));
}

#[test]
fn cli_pretty_output_syntax_highlights_semantic_surface_suggestion() {
    let temp = write_source_family_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "-p",
            "--mode",
            "warn",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\u{1b}[1;31mSourceRuntime\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;31mSourceUpdate\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;31mSourceVendor\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;35mSource\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;36msource\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;33m::\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;35m{\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;32mRuntime\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;32mUpdate\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;32mVendor\u{1b}[0m"));
    assert!(stdout.contains("\u{1b}[1;35m}\u{1b}[0m"));
}

#[test]
fn cli_pretty_output_respects_advisory_filter() {
    let temp = write_policy_and_advisory_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "-p",
            "--show",
            "advisory",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(2));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stripped = strip_ansi_codes(&stdout);
    assert!(stripped.contains("Showing"));
    assert!(stripped.contains("advisory diagnostics and errors only"));
    assert!(stripped.contains("(exit code still reflects the full report)"));
    assert!(stripped.contains("Advisory Diagnostics"));
    assert!(!stripped.contains("Policy Diagnostics"));
    assert!(!stripped.contains("namespace_redundant_qualified_generic"));
}

#[test]
fn cli_explain_prints_profile_and_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .args(["--explain", "namespace_flat_use"])
        .output()
        .expect("run modum --explain");
    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("namespace_flat_use"));
    assert!(stdout.contains("profile: core"));
    assert!(stdout.contains("Flattened imports hide useful namespace context"));
    assert!(stdout.contains("why:"));
    assert!(stdout.contains("address:"));
    assert!(stdout.contains("suppression: use `--ignore namespace_flat_use`"));
    assert!(stdout.contains("write-baseline .modum-baseline.json"));
}

#[test]
fn cli_explain_semantic_string_scalar_includes_fix_and_tuning_guidance() {
    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .args(["--explain", "api_semantic_string_scalar"])
        .output()
        .expect("run modum --explain");
    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("api_semantic_string_scalar"));
    assert!(stdout.contains("typed boundary"));
    assert!(stdout.contains("extra_semantic_string_scalars"));
    assert!(stdout.contains("ignored_semantic_string_scalars"));
    assert!(stdout.contains("ignored_diagnostic_codes"));
    assert!(!stdout.contains("thiserror"));
}

#[test]
fn cli_explain_preserve_module_includes_tuning_guidance() {
    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .args(["--explain", "namespace_flat_use_preserve_module"])
        .output()
        .expect("run modum --explain");
    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("namespace_flat_use_preserve_module"));
    assert!(stdout.contains("extra_namespace_preserving_modules"));
    assert!(stdout.contains("ignored_namespace_preserving_modules"));
    assert!(stdout.contains("ignored_diagnostic_codes"));
}

#[test]
fn cli_explain_maybe_some_includes_fix_guidance() {
    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .args(["--explain", "callsite_maybe_some"])
        .output()
        .expect("run modum --explain");
    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("callsite_maybe_some"));
    assert!(stdout.contains("profile: strict"));
    assert!(stdout.contains("non-`maybe_` setter"));
    assert!(stdout.contains("ignored_diagnostic_codes"));
}

#[test]
fn cli_ignore_suppresses_matching_diagnostic_code() {
    let temp = write_policy_and_advisory_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--ignore",
            "api_candidate_semantic_module",
            "--format",
            "json",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(2));

    let json: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    let diagnostics = json["report"]["diagnostics"]
        .as_array()
        .expect("diagnostics");
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag["code"].as_str() == Some("namespace_redundant_qualified_generic"))
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag["code"].as_str() == Some("api_candidate_semantic_module"))
    );
}

#[test]
fn cli_write_baseline_then_apply_it() {
    let temp = write_policy_and_advisory_fixture();
    let root = temp.path();
    let baseline_path = root.join(".modum/baseline.json");

    let write_output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--format",
            "json",
            "--write-baseline",
            ".modum/baseline.json",
        ])
        .output()
        .expect("run modum check with baseline write");
    assert_eq!(write_output.status.code(), Some(2));
    assert!(baseline_path.is_file());
    let stderr = String::from_utf8_lossy(&write_output.stderr);
    assert!(stderr.contains("wrote baseline .modum/baseline.json (2 coded diagnostics)"));

    let baseline: Value =
        serde_json::from_slice(&fs::read(&baseline_path).expect("read baseline")).expect("json");
    assert_eq!(baseline["version"].as_u64(), Some(1));
    assert_eq!(baseline["diagnostics"].as_array().map(Vec::len), Some(2));

    let apply_output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--format",
            "json",
            "--baseline",
            ".modum/baseline.json",
        ])
        .output()
        .expect("run modum check with baseline");
    assert_eq!(apply_output.status.code(), Some(0));

    let json: Value = serde_json::from_slice(&apply_output.stdout).expect("parse json");
    assert_eq!(
        json["report"]["diagnostics"].as_array().map(Vec::len),
        Some(0)
    );
}

#[test]
fn cli_metadata_baseline_path_is_used_when_present() {
    let temp = write_metadata_baseline_fixture();
    let root = temp.path();

    let write_output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--format",
            "json",
            "--write-baseline",
            ".modum-baseline.json",
        ])
        .output()
        .expect("run modum check with baseline write");
    assert_eq!(write_output.status.code(), Some(0));

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--format",
            "json",
        ])
        .output()
        .expect("run modum check with metadata baseline");
    assert_eq!(output.status.code(), Some(0));

    let json: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    assert_eq!(
        json["report"]["diagnostics"].as_array().map(Vec::len),
        Some(0)
    );
}

#[test]
fn cli_does_not_write_markdown_report_without_flag() {
    let temp = write_policy_and_advisory_fixture();
    let root = temp.path();
    let run_dir = tempdir().expect("create run dir");

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(run_dir.path())
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
    assert!(
        !fs::read_dir(run_dir.path())
            .expect("read run dir")
            .map(|entry| entry.expect("dir entry").path())
            .any(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with("modum-lint-report-") && name.ends_with(".md")
                    })
            })
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("wrote markdown report"));

    let _: Value = serde_json::from_slice(&output.stdout).expect("parse json");
}

#[test]
fn cli_writes_markdown_report_in_invocation_directory() {
    let temp = write_policy_and_advisory_fixture();
    let root = temp.path();
    let run_dir = tempdir().expect("create run dir");

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(run_dir.path())
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "-w",
            "--format",
            "json",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(2));
    let reports = fs::read_dir(run_dir.path())
        .expect("read run dir")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("modum-lint-report-") && name.ends_with(".md"))
        })
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 1);
    let report_name = reports[0]
        .file_name()
        .and_then(|name| name.to_str())
        .expect("report file name");
    let timestamp_stem = report_name
        .strip_prefix("modum-lint-report-")
        .and_then(|name| name.strip_suffix(".md"))
        .expect("timestamped report name");
    assert!(!timestamp_stem.is_empty());
    assert!(timestamp_stem.chars().all(|ch| ch.is_ascii_digit()));
    assert!(
        !fs::read_dir(root)
            .expect("read root dir")
            .map(|entry| entry.expect("dir entry").path())
            .any(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with("modum-lint-report-") && name.ends_with(".md")
                    })
            })
    );

    let markdown = fs::read_to_string(&reports[0]).expect("read markdown report");
    assert!(markdown.starts_with("# modum lint report\n\n```text\n"));
    assert!(markdown.contains("Policy Diagnostics:"));
    assert!(markdown.contains("Advisory Diagnostics:"));
    assert!(markdown.contains("namespace_redundant_qualified_generic"));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("wrote markdown report"));
    assert!(stderr.contains("modum-lint-report-"));
    assert!(stderr.contains(".md"));

    let _: Value = serde_json::from_slice(&output.stdout).expect("parse json");
}

#[test]
fn cli_pretty_output_keeps_markdown_report_plain() {
    let temp = write_policy_and_advisory_fixture();
    let root = temp.path();
    let run_dir = tempdir().expect("create run dir");

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(run_dir.path())
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "-p",
            "-w",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(2));

    let reports = fs::read_dir(run_dir.path())
        .expect("read run dir")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("modum-lint-report-") && name.ends_with(".md"))
        })
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 1);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\u{1b}["));

    let markdown = fs::read_to_string(&reports[0]).expect("read markdown report");
    assert!(!markdown.contains("\u{1b}["));
    assert!(markdown.contains("Policy Diagnostics:"));
    assert!(markdown.contains("Advisory Diagnostics:"));
}

#[test]
fn cli_writes_distinct_timestamped_markdown_reports_across_runs() {
    let temp = write_policy_and_advisory_fixture();
    let root = temp.path();
    let run_dir = tempdir().expect("create run dir");

    for _ in 0..2 {
        let output = Command::new(env!("CARGO_BIN_EXE_modum"))
            .current_dir(run_dir.path())
            .args([
                "check",
                "--root",
                root.to_str().expect("utf8 root"),
                "--write-markdown-report",
                "--format",
                "json",
            ])
            .output()
            .expect("run modum check");
        assert_eq!(output.status.code(), Some(2));
    }

    let reports = fs::read_dir(run_dir.path())
        .expect("read run dir")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("modum-lint-report-") && name.ends_with(".md"))
        })
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 2);

    let first_name = reports[0]
        .file_name()
        .and_then(|name| name.to_str())
        .expect("first report file name");
    let second_name = reports[1]
        .file_name()
        .and_then(|name| name.to_str())
        .expect("second report file name");
    assert_ne!(first_name, second_name);
    assert!(first_name.starts_with("modum-lint-report-"));
    assert!(second_name.starts_with("modum-lint-report-"));
}

#[test]
fn cli_profile_core_hides_strict_advisories() {
    let temp = write_policy_and_advisory_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--profile",
            "core",
            "--format",
            "json",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(2));

    let json: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    let diagnostics = json["report"]["diagnostics"]
        .as_array()
        .expect("diagnostics");
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag["code"].as_str() == Some("namespace_redundant_qualified_generic"))
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag["code"].as_str() == Some("api_candidate_semantic_module"))
    );
}

#[test]
fn cli_profile_surface_includes_surface_and_hides_strict_advisories() {
    let temp = write_surface_and_strict_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--profile",
            "surface",
            "--format",
            "json",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(2));

    let json: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    let diagnostics = json["report"]["diagnostics"]
        .as_array()
        .expect("diagnostics");
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag["code"].as_str() == Some("api_missing_parent_surface_export"))
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag["code"].as_str() == Some("api_candidate_semantic_module"))
    );
}

#[test]
fn cli_invalid_profile_reports_error() {
    let temp = write_json_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--profile",
            "default",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--profile invalid profile `default`; expected core|surface|strict"));
}

#[test]
fn cli_invalid_ignore_reports_error() {
    let temp = write_json_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--ignore",
            "not_a_real_code",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--ignore unknown diagnostic code `not_a_real_code`"));
}

#[test]
fn cli_respects_workspace_metadata_exclude_defaults() {
    let temp = write_workspace_exclude_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
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
        .current_dir(root)
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
        .current_dir(root)
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
        .current_dir(root)
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

#[test]
fn cli_invalid_profile_metadata_reports_error() {
    let temp = write_invalid_profile_metadata_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
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
                    message.contains(
                        "`metadata.modum.profile` invalid profile `default`; expected core|surface|strict"
                    )
                })
            })
    );
}

#[test]
fn cli_invalid_ignored_diagnostic_codes_metadata_reports_error() {
    let temp = write_invalid_ignored_diagnostic_codes_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
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
                    message.contains(
                        "`metadata.modum.ignored_diagnostic_codes[0]` unknown diagnostic code `not_a_real_code`",
                    )
                })
            })
    );
}

#[test]
fn cli_invalid_semantic_scalar_metadata_reports_error() {
    let temp = write_invalid_semantic_scalar_metadata_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
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
                    message.contains(
                        "`metadata.modum.ignored_semantic_string_scalars[0]` must be a string",
                    )
                })
            })
    );
}

#[test]
fn cli_invalid_namespace_preserving_override_metadata_reports_error() {
    let temp = write_invalid_namespace_preserving_override_metadata_fixture();
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
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
                    message.contains(
                        "`metadata.modum.ignored_namespace_preserving_modules[0]` must be a string",
                    )
                })
            })
    );
}

#[test]
fn cli_package_metadata_profile_sets_default_filter() {
    let temp = write_profile_metadata_fixture("core");
    let root = temp.path();

    let output = Command::new(env!("CARGO_BIN_EXE_modum"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            root.to_str().expect("utf8 root"),
            "--format",
            "json",
        ])
        .output()
        .expect("run modum check");
    assert_eq!(output.status.code(), Some(0));

    let json: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    let diagnostics = json["report"]["diagnostics"]
        .as_array()
        .expect("diagnostics");
    assert!(diagnostics.is_empty());
}
