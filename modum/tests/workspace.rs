use std::fs;

use modum::{DiagnosticLevel, analyze_workspace};
use tempfile::tempdir;

fn write_manifest(root: &std::path::Path, extra: &str) {
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = "fixture"
version = "0.1.0"
edition = "2024"

{extra}
"#
        ),
    )
    .expect("write manifest");
}

#[test]
fn analyze_workspace_flags_flattened_generic_imports() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(
        root,
        r#"[package.metadata.modum]
generic_nouns = ["Repository"]
"#,
    );
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

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.level == DiagnosticLevel::Warning
            && diag.policy
            && diag.code.as_deref() == Some("namespace_flat_use")
    }));
}

#[test]
fn analyze_workspace_flags_storage_repository_flattened_import() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use storage::Repository;

pub mod storage {
    pub struct Repository;
}

pub fn load(repo: Repository) -> Repository {
    repo
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("namespace_flat_use")
            && diag.message.contains("storage::Repository")
    }));
}

#[test]
fn analyze_workspace_flags_broader_namespace_preserving_imports() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use http::Client;

pub mod http {
    pub struct Client;
}

pub fn connect(client: Client) -> Client {
    client
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("namespace_flat_use_preserve_module")
            && diag.message.contains("http::Client")
    }));
}

#[test]
fn analyze_workspace_flags_redundant_context_imports_even_without_configured_module() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use user::UserRepository;

pub mod user {
    pub struct UserRepository;
}

pub fn load(repo: UserRepository) -> UserRepository {
    repo
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("namespace_flat_use_redundant_leaf_context")
            && diag.message.contains("user::Repository")
    }));
}

#[test]
fn analyze_workspace_flags_redundant_leaf_context_in_public_module_files() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod user;\n").expect("write lib");
    fs::write(
        root.join("src/user.rs"),
        r#"
pub struct UserRepository;
"#,
    )
    .expect("write user module");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("api_redundant_leaf_context")
            && diag.message.contains("user::Repository")
    }));
}

#[test]
fn analyze_workspace_flags_weak_module_generic_leaf() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod storage;\n").expect("write lib");
    fs::write(
        root.join("src/storage.rs"),
        r#"
pub struct Repository;
"#,
    )
    .expect("write storage module");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("api_weak_module_generic_leaf")
            && diag.message.contains("weak module `storage`")
    }));
}

#[test]
fn analyze_workspace_flags_redundant_category_suffix() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod user {
    pub mod error {
        pub struct InvalidEmailError;
    }
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("api_redundant_category_suffix")
            && diag.message.contains("user::error::InvalidEmail")
    }));
}

#[test]
fn analyze_workspace_flags_catch_all_public_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod helpers {
    pub struct UserId;
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("api_catch_all_module")
            && diag.message.contains("catch-all API boundary")
    }));
}

#[test]
fn analyze_workspace_flags_repeated_module_segments() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod error {
    pub mod error {
        pub struct InvalidEmail;
    }
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("api_repeated_module_segment")
            && diag.message.contains("repeats `error`")
    }));
}

#[test]
fn analyze_workspace_flags_public_organizational_submodule_as_error() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/partials")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod partials;\n").expect("write lib");
    fs::write(root.join("src/partials.rs"), "pub mod error;\n").expect("write partials");
    fs::write(root.join("src/partials/error.rs"), "pub struct Error;\n").expect("write error");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.level == DiagnosticLevel::Error
            && diag.code.as_deref() == Some("api_organizational_submodule_flatten")
            && diag.message.contains("partials::error::Error")
    }));
}
