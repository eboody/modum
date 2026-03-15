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
fn analyze_workspace_does_not_flag_preserve_module_imports_inside_same_namespace_subtree() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/components")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod components;\n").expect("write lib");
    fs::write(
        root.join("src/components.rs"),
        "pub struct NavBar;\npub mod page;\n",
    )
    .expect("write components");
    fs::write(
        root.join("src/components/page.rs"),
        r#"
use crate::components::NavBar;

pub fn render(nav: NavBar) -> NavBar {
    nav
}
"#,
    )
    .expect("write page");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("namespace_flat_use_preserve_module")
            && diag.message.contains("components::NavBar")
    }));
}

#[test]
fn analyze_workspace_allows_canonical_parent_surface_public_reexports() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/user")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod user;\n").expect("write lib");
    fs::write(
        root.join("src/user.rs"),
        r#"
pub mod error;
pub mod repository;

pub use error::{Error, RepositoryOperation, Result};
pub use repository::Repository;
"#,
    )
    .expect("write user module");
    fs::write(
        root.join("src/user/error.rs"),
        r#"
pub struct Error;
pub struct RepositoryOperation;
pub type Result<T> = core::result::Result<T, Error>;
"#,
    )
    .expect("write error module");
    fs::write(
        root.join("src/user/repository.rs"),
        "pub struct Repository;\n",
    )
    .expect("write repository module");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        matches!(
            diag.code.as_deref(),
            Some("namespace_flat_pub_use" | "namespace_flat_pub_use_preserve_module")
        )
    }));
}

#[test]
fn analyze_workspace_allows_canonical_parent_surface_public_reexports_from_internal_tree() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/repo")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod chat;\npub mod repo;\n").expect("write lib");
    fs::write(
        root.join("src/chat.rs"),
        "pub use crate::repo::chat::Repository;\n",
    )
    .expect("write chat");
    fs::write(root.join("src/repo.rs"), "pub mod chat;\n").expect("write repo");
    fs::write(root.join("src/repo/chat.rs"), "pub struct Repository;\n").expect("write repository");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        matches!(
            diag.code.as_deref(),
            Some("namespace_flat_pub_use" | "namespace_flat_pub_use_preserve_module")
        )
    }));
}

#[test]
fn analyze_workspace_flags_public_reexports_that_flatten_meaningful_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/handlers")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod handlers;\n").expect("write lib");
    fs::write(
        root.join("src/handlers.rs"),
        r#"
pub mod auth;

pub use auth::{login, login_form};
"#,
    )
    .expect("write handlers");
    fs::write(
        root.join("src/handlers/auth.rs"),
        "pub fn login() {}\npub fn login_form() {}\n",
    )
    .expect("write auth");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("namespace_flat_pub_use_preserve_module")
            && diag.message.contains("auth::login")
    }));
}

#[test]
fn analyze_workspace_flags_public_reexports_with_readable_child_module_surface() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod message {
    pub struct MessageBody;
}

pub use message::MessageBody;
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("namespace_flat_pub_use_redundant_leaf_context")
            && diag.message.contains("message::Body")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_private_child_module_family_reexports() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/components")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod components;\n").expect("write lib");
    fs::write(
        root.join("src/components.rs"),
        r#"
mod auth_shell;

pub use auth_shell::{AuthShell, AuthShellVariant};
"#,
    )
    .expect("write components");
    fs::write(
        root.join("src/components/auth_shell.rs"),
        "pub struct AuthShell;\npub struct AuthShellVariant;\n",
    )
    .expect("write auth_shell");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("namespace_flat_pub_use_redundant_leaf_context")
            && diag.message.contains("auth_shell::Variant")
    }));
}

#[test]
fn analyze_workspace_allows_preserved_parent_surface_reexports() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/partials")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod partials;\n").expect("write lib");
    fs::write(
        root.join("src/partials.rs"),
        r#"
pub mod components;

pub use components::{Button, SectionHeader};
"#,
    )
    .expect("write partials");
    fs::write(
        root.join("src/partials/components.rs"),
        "pub struct Button;\npub struct SectionHeader;\n",
    )
    .expect("write components");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("namespace_flat_pub_use_preserve_module")
            && diag.message.contains("components::SectionHeader")
    }));
}

#[test]
fn analyze_workspace_allows_private_alias_module_that_feeds_parent_surface() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/layout")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        "pub mod components;\npub mod layout;\n",
    )
    .expect("write lib");
    fs::write(
        root.join("src/components.rs"),
        "pub struct SectionHeader;\n",
    )
    .expect("write components");
    fs::write(
        root.join("src/layout.rs"),
        r#"
mod section_header;

pub use section_header::SectionHeader;
"#,
    )
    .expect("write layout");
    fs::write(
        root.join("src/layout/section_header.rs"),
        "pub use crate::components::SectionHeader;\n",
    )
    .expect("write section_header");

    let report = analyze_workspace(root, &[]);
    let alias_file = root.join("src/layout/section_header.rs");
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("namespace_flat_pub_use_preserve_module")
            && diag.file.as_deref() == Some(alias_file.as_path())
    }));
}

#[test]
fn analyze_workspace_flags_missing_parent_surface_export_for_public_child_module() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/components")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod components;\n").expect("write lib");
    fs::write(root.join("src/components.rs"), "pub mod button;\n").expect("write components");
    fs::write(
        root.join("src/components/button.rs"),
        "pub struct Button;\npub struct Variant;\n",
    )
    .expect("write button");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("api_missing_parent_surface_export")
            && diag.message.contains("components::Button")
            && diag.message.contains("components::button::Button")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_missing_parent_surface_export_at_weak_crate_root() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod user;\n").expect("write lib");
    fs::write(root.join("src/user.rs"), "pub struct User;\n").expect("write user");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("api_missing_parent_surface_export")
            && diag.message.contains("user::User")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_missing_parent_surface_export_when_parent_already_has_it() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/components")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod components;\n").expect("write lib");
    fs::write(
        root.join("src/components.rs"),
        "pub mod button;\npub use button::Button;\n",
    )
    .expect("write components");
    fs::write(
        root.join("src/components/button.rs"),
        "pub struct Button;\n",
    )
    .expect("write button");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code.as_deref() == Some("api_missing_parent_surface_export") })
    );
}

#[test]
fn analyze_workspace_flags_renamed_imports_that_hide_a_readable_module_surface() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use playwright::api::page::Event as PageEvent;

pub fn keep(event: PageEvent) -> PageEvent {
    event
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("namespace_flat_use_redundant_leaf_context")
            && diag.message.contains("page::Event")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_trait_enabling_underscore_imports() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
#[cfg(test)]
mod tests {
    use std::error::Error as _;
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code.as_deref() == Some("namespace_flat_use_preserve_module") })
    );
}

#[test]
fn analyze_workspace_flags_canonical_parent_surface_at_crate_root() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        "mod error;\npub mod request;\npub use error::Error;\n",
    )
    .expect("write lib");
    fs::write(root.join("src/error.rs"), "pub struct Error;\n").expect("write error");
    fs::write(
        root.join("src/request.rs"),
        r#"
use crate::error::Error;

pub fn keep(error: Error) -> Error {
    error
}
"#,
    )
    .expect("write request");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("namespace_parent_surface")
            && diag.message.contains("fixture::Error")
    }));
}

#[test]
fn analyze_workspace_flags_canonical_parent_surface_in_nested_module() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/user")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod user;\n").expect("write lib");
    fs::write(
        root.join("src/user.rs"),
        r#"
pub mod error;
pub mod repository;

pub use error::Result;
"#,
    )
    .expect("write user module");
    fs::write(
        root.join("src/user/error.rs"),
        "pub type Result<T> = core::result::Result<T, ()>;\n",
    )
    .expect("write user error");
    fs::write(
        root.join("src/user/repository.rs"),
        r#"
use super::error::Result;

pub fn keep<T>(value: Result<T>) -> Result<T> {
    value
}
"#,
    )
    .expect("write repository");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("namespace_parent_surface")
            && diag.message.contains("user::Result")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_parent_surface_when_parent_does_not_export_it() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/user")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod user;\n").expect("write lib");
    fs::write(
        root.join("src/user.rs"),
        r#"
pub mod error;
pub mod repository;
"#,
    )
    .expect("write user module");
    fs::write(
        root.join("src/user/error.rs"),
        "pub type Result<T> = core::result::Result<T, ()>;\n",
    )
    .expect("write user error");
    fs::write(
        root.join("src/user/repository.rs"),
        r#"
use super::error::Result;

pub fn keep<T>(value: Result<T>) -> Result<T> {
    value
}
"#,
    )
    .expect("write repository");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code.as_deref() == Some("namespace_parent_surface") })
    );
}

#[test]
fn analyze_workspace_does_not_flag_relative_group_imports_without_semantic_parent() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod parent {
    pub struct Error;

    mod child {
        use super::Error;
    }
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        matches!(
            diag.code.as_deref(),
            Some("namespace_flat_use") | Some("namespace_flat_use_redundant_leaf_context")
        )
    }));
}

#[test]
fn analyze_workspace_does_not_mechanically_shorten_std_names() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use std::{
    str::FromStr,
    sync::atomic::AtomicUsize,
    time::SystemTime,
};

pub fn sample() {
    let _ = std::mem::size_of::<AtomicUsize>();
    let _ = SystemTime::UNIX_EPOCH;
    let _ = <u8 as FromStr>::from_str("1");
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report.diagnostics.iter().any(|diag| {
            diag.code.as_deref() == Some("namespace_flat_use_redundant_leaf_context")
        })
    );
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
fn analyze_workspace_flags_relative_imports_with_real_parent_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod room {
    pub struct RoomId;
}

mod message {
    use super::room::RoomId;

    pub fn load(id: RoomId) -> RoomId {
        id
    }
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code.as_deref() == Some("namespace_flat_use_redundant_leaf_context")
            && diag.message.contains("room::Id")
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
