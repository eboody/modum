use std::fs;

use modum::{
    AnalysisSettings, DiagnosticLevel, LintProfile, ScanSettings, analyze_workspace,
    analyze_workspace_with_settings,
};
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
        diag.level() == DiagnosticLevel::Warning
            && diag.is_policy_violation()
            && diag.code() == Some("namespace_flat_use")
    }));
}

#[test]
fn analyze_workspace_uses_workspace_profile_defaults() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("member/src/components")).expect("create src");
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "2"
members = ["member"]

[workspace.metadata.modum]
profile = "surface"
"#,
    )
    .expect("write workspace manifest");
    fs::write(
        root.join("member/Cargo.toml"),
        r#"[package]
name = "member"
version = "0.1.0"
edition = "2024"
"#,
    )
    .expect("write member manifest");
    fs::write(
        root.join("member/src/lib.rs"),
        r#"
pub struct UserRepository;
pub struct UserService;
pub struct UserId;

pub mod components;
"#,
    )
    .expect("write lib");
    fs::write(root.join("member/src/components.rs"), "pub mod button;\n")
        .expect("write components");
    fs::write(
        root.join("member/src/components/button.rs"),
        "pub struct Button;\n",
    )
    .expect("write button");

    let report = analyze_workspace(root, &[]);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_missing_parent_surface_export"))
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_semantic_module"))
    );
}

#[test]
fn analyze_workspace_package_profile_overrides_workspace_profile() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("member/src/components")).expect("create src");
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "2"
members = ["member"]

[workspace.metadata.modum]
profile = "core"
"#,
    )
    .expect("write workspace manifest");
    fs::write(
        root.join("member/Cargo.toml"),
        r#"[package]
name = "member"
version = "0.1.0"
edition = "2024"

[package.metadata.modum]
profile = "surface"
"#,
    )
    .expect("write member manifest");
    fs::write(
        root.join("member/src/lib.rs"),
        r#"
pub struct UserRepository;
pub struct UserService;
pub struct UserId;

pub mod components;
"#,
    )
    .expect("write lib");
    fs::write(root.join("member/src/components.rs"), "pub mod button;\n")
        .expect("write components");
    fs::write(
        root.join("member/src/components/button.rs"),
        "pub struct Button;\n",
    )
    .expect("write button");

    let report = analyze_workspace(root, &[]);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_missing_parent_surface_export"))
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_semantic_module"))
    );
}

#[test]
fn analyze_workspace_package_token_family_overrides_inherit_workspace_defaults() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("member/src")).expect("create src");
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "2"
members = ["member"]

[workspace.metadata.modum]
extra_semantic_string_scalars = ["mime"]
"#,
    )
    .expect("write workspace manifest");
    fs::write(
        root.join("member/Cargo.toml"),
        r#"[package]
name = "member"
version = "0.1.0"
edition = "2024"

[package.metadata.modum]
ignored_semantic_string_scalars = ["url"]
"#,
    )
    .expect("write member manifest");
    fs::write(
        root.join("member/src/lib.rs"),
        r#"
pub fn connect(mime: String, url: String) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_semantic_string_scalar") && diag.message.contains("mime")
    }));
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_semantic_string_scalar") && diag.message.contains("url")
    }));
}

#[test]
fn analyze_workspace_cli_profile_overrides_metadata_profile() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/components")).expect("create src");
    write_manifest(
        root,
        r#"[package.metadata.modum]
profile = "strict"
"#,
    );
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

    let report = analyze_workspace_with_settings(
        root,
        &AnalysisSettings {
            scan: ScanSettings::default(),
            profile: Some(LintProfile::Core),
            ignored_diagnostic_codes: Vec::new(),
            baseline: None,
        },
    );
    assert!(report.diagnostics.is_empty());
}

#[test]
fn analyze_workspace_package_ignored_diagnostic_codes_suppress_matching_lints() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(
        root,
        r#"[package.metadata.modum]
ignored_diagnostic_codes = ["api_candidate_semantic_module"]
"#,
    );
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct UserRepository;
pub struct UserService;
pub struct UserId;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_semantic_module"))
    );
}

#[test]
fn analyze_workspace_ignored_namespace_preserving_modules_remove_defaults() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(
        root,
        r#"[package.metadata.modum]
ignored_namespace_preserving_modules = ["components"]
"#,
    );
    fs::write(
        root.join("src/lib.rs"),
        r#"
use components::Button;

mod components {
    pub struct Button;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("namespace_flat_use_preserve_module"))
    );
}

#[test]
fn analyze_workspace_extra_namespace_preserving_modules_extend_defaults() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(
        root,
        r#"[package.metadata.modum]
extra_namespace_preserving_modules = ["widgets"]
"#,
    );
    fs::write(
        root.join("src/lib.rs"),
        r#"
use widgets::Button;

mod widgets {
    pub struct Button;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_use_preserve_module")
            && diag.message.contains("widgets::Button")
    }));
}

#[test]
fn analyze_workspace_flags_internal_redundant_leaf_context() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod user {
    pub(super) struct Repository;
    struct UserRepository;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_redundant_leaf_context")
            && diag.message.contains("user::UserRepository")
            && diag.message.contains("user::Repository")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_internal_redundant_leaf_context_for_weak_shorter_leaves() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod report {
    trait ReportExt {}
}

mod bootstrap {
    struct BootstrapConfig;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("internal_redundant_leaf_context"))
    );
}

#[test]
fn analyze_workspace_does_not_flag_internal_redundant_leaf_context_for_meta_shorter_leaves() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod transition {
    struct TransitionFn;
    struct TransitionImpl;
}

mod introspection {
    struct MachineIntrospection;
}

mod state {
    struct StateModulePath;
}

mod builder {
    struct SignatureForBuilder;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("internal_redundant_leaf_context"))
    );
}

#[test]
fn analyze_workspace_flags_internal_weak_module_generic_leaf() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod helpers {
    struct Repository;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("internal_catch_all_module"))
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("internal_weak_module_generic_leaf"))
    );
}

#[test]
fn analyze_workspace_flags_internal_repeated_module_segment() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod user {
    mod user {
        struct Repository;
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("internal_repeated_module_segment"))
    );
}

#[test]
fn analyze_workspace_flags_internal_organizational_module() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod response {
    struct Payload;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("internal_organizational_submodule_flatten"))
    );
}

#[test]
fn analyze_workspace_flags_flattening_type_aliases() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod user {
    pub struct Repository;
}

type Repository = user::Repository;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("namespace_flat_type_alias"))
    );
}

#[test]
fn analyze_workspace_flags_namespace_preserving_type_aliases() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod http {
    pub struct Client;
}

type Client = http::Client;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("namespace_flat_type_alias_preserve_module"))
    );
}

#[test]
fn analyze_workspace_flags_redundant_type_alias_context() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod user {
    pub struct Repository;
}

type UserRepository = user::Repository;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_type_alias_redundant_leaf_context")
            && diag.message.contains("user::Repository")
    }));
}

#[test]
fn analyze_workspace_flags_missing_parent_surface_export_for_crate_visible_child_module() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/components")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub(crate) mod components;\n").expect("write lib");
    fs::write(root.join("src/components.rs"), "pub(crate) mod button;\n")
        .expect("write components");
    fs::write(
        root.join("src/components/button.rs"),
        "pub(crate) struct Button;\n",
    )
    .expect("write button");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_missing_parent_surface_export")
            && diag.message.contains("components::Button")
    }));
}

#[test]
fn analyze_workspace_flags_candidate_semantic_module_for_crate_visible_family() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub(crate) struct UserRepository;
pub(crate) struct UserService;
pub(crate) struct UserId;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_semantic_module"))
    );
}

#[test]
fn analyze_workspace_flags_stringly_protocol_parameters() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub fn transition(next_state: &str, gate_kind: String) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_stringly_protocol_parameter")
            && diag.message.contains("next_state")
            && diag.message.contains("gate_kind")
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
        diag.code() == Some("namespace_flat_use") && diag.message.contains("storage::Repository")
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
        diag.code() == Some("namespace_flat_use_preserve_module")
            && diag.message.contains("http::Client")
    }));
}

#[test]
fn analyze_workspace_prefers_trace_namespace_for_shortened_entry_leaf() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use trace::TraceEntry;

pub mod trace {
    pub struct TraceEntry;
}

pub fn keep(entry: TraceEntry) -> TraceEntry {
    entry
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_use_redundant_leaf_context")
            && diag.message.contains("trace::Entry")
    }));
}

#[test]
fn analyze_workspace_prefers_write_back_namespace_for_shortened_submission_leaf() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use write_back::WriteBackSubmission;

pub mod write_back {
    pub struct WriteBackSubmission;
}

pub fn keep(submission: WriteBackSubmission) -> WriteBackSubmission {
    submission
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_use_redundant_leaf_context")
            && diag.message.contains("write_back::Submission")
    }));
}

#[test]
fn analyze_workspace_flags_flattened_namespace_alias_paths() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use domain::write_back as write_back_domain;

pub mod domain {
    pub mod write_back {
        pub enum Submission {
            Accepted,
            Rejected,
        }
    }
}

pub fn keep(submission: write_back_domain::Submission) -> write_back_domain::Submission {
    match submission {
        write_back_domain::Submission::Accepted => write_back_domain::Submission::Rejected,
        write_back_domain::Submission::Rejected => write_back_domain::Submission::Accepted,
    }
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_aliased_qualified_path")
            && diag.message.contains("write_back_domain::Submission")
            && diag.message.contains("domain::write_back::Submission")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_non_flattening_namespace_alias_paths() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use domain::write_back as outbound;

pub mod domain {
    pub mod write_back {
        pub struct Submission;
    }
}

pub fn keep(submission: outbound::Submission) -> outbound::Submission {
    submission
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("namespace_aliased_qualified_path") })
    );
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
        diag.code() == Some("namespace_flat_use_preserve_module")
            && diag.message.contains("components::NavBar")
    }));
}

#[test]
fn analyze_workspace_allows_semantic_child_module_namespace_under_preserved_parent() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod components;
mod page;
"#,
    )
    .expect("write lib");
    fs::write(
        root.join("src/components.rs"),
        r#"
pub use tab_set::Component as TabSet;

pub mod tab_set {
    pub struct Component;
    pub struct ContentProps;

    pub mod content {
        pub struct Content;
    }
}
"#,
    )
    .expect("write components");
    fs::write(
        root.join("src/page.rs"),
        r#"
use crate::components::tab_set;

pub fn render(props: tab_set::ContentProps) -> tab_set::Component {
    let _ = core::mem::size_of::<tab_set::content::Content>();
    todo!()
}
"#,
    )
    .expect("write page");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_use_preserve_module")
            && diag.message.contains("components::tab_set")
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
            diag.code(),
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
            diag.code(),
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
        diag.code() == Some("namespace_flat_pub_use_preserve_module")
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
        diag.code() == Some("namespace_flat_pub_use_redundant_leaf_context")
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
        diag.code() == Some("namespace_flat_pub_use_redundant_leaf_context")
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
        diag.code() == Some("namespace_flat_pub_use_preserve_module")
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
        diag.code() == Some("namespace_flat_pub_use_preserve_module")
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
        diag.code() == Some("api_missing_parent_surface_export")
            && diag.message.contains("components::Button")
            && diag.message.contains("components::button::Button")
    }));
}

#[test]
fn analyze_workspace_flags_missing_parent_surface_export_for_generic_child_main_item() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/outcome")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod outcome;\n").expect("write lib");
    fs::write(root.join("src/outcome.rs"), "pub mod toxicity;\n").expect("write outcome");
    fs::write(
        root.join("src/outcome/toxicity.rs"),
        "pub struct Outcome;\n",
    )
    .expect("write toxicity");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_missing_parent_surface_export")
            && diag.message.contains("outcome::Toxicity")
            && diag.message.contains("outcome::toxicity::Outcome")
    }));
}

#[test]
fn analyze_workspace_flags_tail_family_candidate_semantic_module_and_suppresses_weaker_child_export_hint()
 {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/terminal")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod terminal;\n").expect("write lib");
    fs::write(
        root.join("src/terminal.rs"),
        r#"
pub struct CompletedOutcome;
pub struct RejectedOutcome;
pub mod toxicity;
pub enum Outcome {
    Completed(CompletedOutcome),
    Rejected(RejectedOutcome),
    Toxicity(toxicity::Outcome),
}
"#,
    )
    .expect("write terminal");
    fs::write(
        root.join("src/terminal/toxicity.rs"),
        "pub struct Outcome;\n",
    )
    .expect("write toxicity");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module")
            && !diag.is_policy_violation()
            && diag.message.contains("CompletedOutcome")
            && diag.message.contains("RejectedOutcome")
            && diag.message.contains("toxicity::Outcome")
            && diag
                .message
                .contains("outcome::{Completed, Rejected, Toxicity}")
    }));
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_missing_parent_surface_export")
            && diag.message.contains("terminal::Toxicity")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_missing_parent_surface_export_for_generic_child_when_module_has_multiple_public_items()
 {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/outcome")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod outcome;\n").expect("write lib");
    fs::write(root.join("src/outcome.rs"), "pub mod toxicity;\n").expect("write outcome");
    fs::write(
        root.join("src/outcome/toxicity.rs"),
        "pub struct Outcome;\npub struct Evidence;\n",
    )
    .expect("write toxicity");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_missing_parent_surface_export")
            && diag.message.contains("outcome::Toxicity")
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
        diag.code() == Some("api_missing_parent_surface_export")
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
            .any(|diag| { diag.code() == Some("api_missing_parent_surface_export") })
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
        diag.code() == Some("namespace_flat_use_redundant_leaf_context")
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
            .any(|diag| { diag.code() == Some("namespace_flat_use_preserve_module") })
    );
}

#[test]
fn analyze_workspace_does_not_flag_flattened_imports_when_only_visible_surface_is_redundant() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod response;
mod handler;
"#,
    )
    .expect("write lib");
    fs::write(root.join("src/response.rs"), "pub struct Response;\n").expect("write response");
    fs::write(
        root.join("src/handler.rs"),
        r#"
use crate::response::Response;

pub fn keep(value: Response) -> Response {
    value
}
"#,
    )
    .expect("write handler");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        matches!(
            diag.code(),
            Some("namespace_flat_use") | Some("namespace_flat_use_preserve_module")
        )
    }));
}

#[test]
fn analyze_workspace_does_not_flag_flattened_external_generic_imports_without_net_context() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use std::error::Error;

pub fn source(error: &dyn Error) -> &(dyn Error + '_) {
    error
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        matches!(
            diag.code(),
            Some("namespace_flat_use") | Some("namespace_flat_use_preserve_module")
        )
    }));
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
        diag.code() == Some("namespace_parent_surface") && diag.message.contains("crate::Error")
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
        diag.code() == Some("namespace_parent_surface") && diag.message.contains("user::Result")
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
            .any(|diag| { diag.code() == Some("namespace_parent_surface") })
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
            diag.code(),
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
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("namespace_flat_use_redundant_leaf_context") })
    );
}

#[test]
fn analyze_workspace_flags_redundant_qualified_generic_callsite_paths() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod response {
    pub struct Response;
}

pub fn keep(value: response::Response) -> response::Response {
    value
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_redundant_qualified_generic")
            && diag.message.contains("response::Response")
            && diag.message.contains("prefer `Response`")
    }));
}

#[test]
fn analyze_workspace_prefers_canonical_crate_surface_for_qualified_generic_paths() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod error;
pub use error::Error;

pub fn keep(value: crate::error::Error) -> crate::error::Error {
    value
}
"#,
    )
    .expect("write source");
    fs::write(root.join("src/error.rs"), "pub struct Error;\n").expect("write error");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_redundant_qualified_generic")
            && diag.message.contains("crate::error::Error")
            && diag.message.contains("prefer `crate::Error`")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_meaningful_qualified_callsite_paths() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod http {
    pub struct StatusCode;
}

mod url {
    pub struct Url;
}

pub fn keep(status: http::StatusCode, url: url::Url) -> (http::StatusCode, url::Url) {
    (status, url)
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("namespace_redundant_qualified_generic") })
    );
}

#[test]
fn analyze_workspace_does_not_flag_non_actionable_external_redundant_context_imports() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use syn::parse::ParseStream;

pub fn keep(_input: ParseStream<'_>) {}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("namespace_flat_use_redundant_leaf_context") })
    );
}

#[test]
fn analyze_workspace_does_not_flag_non_actionable_external_renamed_imports() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use syn::Path as SynPath;

pub fn keep(_path: SynPath) {}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("namespace_flat_use_redundant_leaf_context") })
    );
}

#[test]
fn analyze_workspace_flags_actionable_external_renamed_imports() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use http::StatusCode as HttpStatusCode;

pub fn keep(code: HttpStatusCode) -> HttpStatusCode {
    code
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_use_redundant_leaf_context")
            && diag.message.contains("http::StatusCode")
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
        diag.code() == Some("namespace_flat_use_redundant_leaf_context")
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
        diag.code() == Some("namespace_flat_use_redundant_leaf_context")
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
        diag.code() == Some("api_redundant_leaf_context")
            && diag.message.contains("user::Repository")
    }));
}

#[test]
fn analyze_workspace_flags_public_root_generic_response_module_as_error() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod response;\n").expect("write lib");
    fs::write(root.join("src/response.rs"), "pub struct Response;\n").expect("write response");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_organizational_submodule_flatten")
            && diag.message.contains("response::Response")
            && diag.message.contains("prefer `Response`")
    }));
}

#[test]
fn analyze_workspace_flags_nested_generic_response_module_with_final_parent_surface() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/auth")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod auth;\n").expect("write lib");
    fs::write(root.join("src/auth.rs"), "pub mod response;\n").expect("write auth");
    fs::write(root.join("src/auth/response.rs"), "pub struct Response;\n").expect("write response");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_organizational_submodule_flatten")
            && diag.message.contains("auth::response::Response")
            && diag.message.contains("prefer `auth::Response`")
    }));
}

#[test]
fn analyze_workspace_collapse_redundant_leaf_context_to_final_public_path() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod response;\n").expect("write lib");
    fs::write(
        root.join("src/response.rs"),
        r#"
pub struct ResponseResponse;
"#,
    )
    .expect("write response");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_redundant_category_suffix")
            && diag.message.contains("ResponseResponse")
            && diag.message.contains("prefer `Response`")
            && !diag.message.contains("prefer `response::Response`")
    }));
}

#[test]
fn analyze_workspace_flags_prefixed_public_root_item_when_semantic_module_surface_exists() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod user;

pub struct UserRepository;
"#,
    )
    .expect("write lib");
    fs::write(
        root.join("src/user.rs"),
        r#"
pub struct Repository;
"#,
    )
    .expect("write user");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_redundant_leaf_context")
            && diag.message.contains("user::Repository")
            && diag.message.contains("UserRepository")
    }));
}

#[test]
fn analyze_workspace_flags_prefixed_public_root_alias_when_semantic_module_surface_exists() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod generated;
pub mod user;

pub use generated::UserRepository;
"#,
    )
    .expect("write lib");
    fs::write(
        root.join("src/generated.rs"),
        "pub struct UserRepository;\n",
    )
    .expect("write gen");
    fs::write(
        root.join("src/user.rs"),
        r#"
pub struct Repository;
"#,
    )
    .expect("write user");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_redundant_leaf_context")
            && diag.message.contains("user::Repository")
            && diag.message.contains("UserRepository")
    }));
}

#[test]
fn analyze_workspace_flags_tail_suffixed_public_root_item_when_semantic_module_surface_exists() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod outcome;

pub struct CompletedOutcome;
"#,
    )
    .expect("write lib");
    fs::write(
        root.join("src/outcome.rs"),
        r#"
pub struct Completed;
"#,
    )
    .expect("write outcome");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_redundant_leaf_context")
            && diag.message.contains("outcome::Completed")
            && diag.message.contains("CompletedOutcome")
    }));
}

#[test]
fn analyze_workspace_flags_candidate_semantic_module_for_same_head_family() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct UserRepository;
pub struct UserService;
pub struct UserId;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module")
            && !diag.is_policy_violation()
            && diag.message.contains("UserRepository")
            && diag.message.contains("UserService")
            && diag.message.contains("UserId")
            && diag.message.contains("user::{Id, Repository, Service}")
    }));
}

#[test]
fn analyze_workspace_flags_candidate_semantic_module_for_compound_shared_head_family() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct WriteBackReady;
pub struct WriteBackPrepared;
pub struct WriteBackSubmitted;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module")
            && diag.message.contains("WriteBackReady")
            && diag.message.contains("WriteBackPrepared")
            && diag.message.contains("WriteBackSubmitted")
            && diag
                .message
                .contains("write_back::{Prepared, Ready, Submitted}")
            && !diag.message.contains("write::{Back")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_candidate_semantic_module_for_weak_head_family() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct ToSqlQuery;
pub struct ToQueryValue;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_candidate_semantic_module") })
    );
}

#[test]
fn analyze_workspace_does_not_flag_candidate_semantic_module_for_two_item_head_family() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct ConfigurationError;
pub struct ConfigurationErrorKind;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_candidate_semantic_module") })
    );
}

#[test]
fn analyze_workspace_does_not_flag_candidate_semantic_module_for_meta_family() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct DiagnosticClass;
pub struct DiagnosticCodeInfo;
pub struct DiagnosticFix;
pub struct DiagnosticFixKind;
pub struct DiagnosticLevel;
pub struct DiagnosticSelection;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_semantic_module"))
    );
}

#[test]
fn analyze_workspace_dedupes_candidate_semantic_module_for_same_file_head() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct QueryError;
pub struct QueryResult;
pub struct QueryValue;

pub mod nested {
    pub struct QueryBuilder;
    pub struct QueryType;
    pub struct QueryPredicate;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|diag| {
                diag.code() == Some("api_candidate_semantic_module")
                    && diag.message.contains("query::{")
            })
            .count(),
        1
    );
}

#[test]
fn analyze_workspace_does_not_flag_candidate_semantic_module_inside_hidden_internal_module() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
#[doc(hidden)]
pub mod __private {
    pub struct UserRepository;
    pub struct UserService;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_candidate_semantic_module") })
    );
}

#[test]
fn analyze_workspace_skips_candidate_semantic_module_for_cfg_guarded_public_items() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
#[cfg(any())]
pub struct UserRepository;
pub struct UserService;
pub struct UserId;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module_unsupported_construct")
            && diag.message.contains("`#[cfg]`")
    }));
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_candidate_semantic_module") })
    );
}

#[test]
fn analyze_workspace_skips_candidate_semantic_module_for_macro_generated_public_items() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
macro_rules! declare_user_family {
    () => {
        pub struct UserRepository;
        pub struct UserService;
        pub struct UserId;
    };
}

declare_user_family!();
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module_unsupported_construct")
            && diag.message.contains("`macro_rules!`")
            && diag.message.contains("`item macro`")
    }));
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_candidate_semantic_module") })
    );
}

#[test]
fn analyze_workspace_skips_candidate_semantic_module_when_public_child_module_uses_include() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    let terminal_file = root.join("src/terminal.rs");
    fs::create_dir_all(root.join("src/terminal")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod terminal;\n").expect("write lib");
    fs::write(
        &terminal_file,
        r#"
pub struct CompletedOutcome;
pub struct RejectedOutcome;
pub mod toxicity;
pub struct Outcome;
"#,
    )
    .expect("write terminal");
    fs::write(
        root.join("src/terminal/toxicity.rs"),
        "include!(\"toxicity_generated.rs\");\n",
    )
    .expect("write toxicity");
    fs::write(
        root.join("src/terminal/toxicity_generated.rs"),
        "pub struct Outcome;\n",
    )
    .expect("write generated");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module_unsupported_construct")
            && diag.file.as_deref() == Some(terminal_file.as_path())
            && diag.message.contains("`include!`")
    }));
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_candidate_semantic_module") })
    );
}

#[test]
fn analyze_workspace_does_not_treat_hidden_module_as_semantic_surface() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
#[doc(hidden)]
pub mod user {
    pub struct Repository;
}

pub struct UserRepository;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_redundant_leaf_context") })
    );
}

#[test]
fn analyze_workspace_flags_manual_enum_string_helper_methods() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub enum Scenario {
    HappyPath,
    Failure,
}

impl Scenario {
    pub fn label(&self) -> &'static str {
        match self {
            Self::HappyPath => "happy-path",
            Self::Failure => "failure",
        }
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_manual_enum_string_helper")
            && diag.message.contains("Scenario")
            && diag.message.contains("`label`")
            && diag.message.contains("Display")
    }));
}

#[test]
fn analyze_workspace_flags_manual_display_impls_for_public_enums() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub enum Mode {
    Warn,
    Deny,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Warn => f.write_str("warn"),
            Self::Deny => f.write_str("deny"),
        }
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_manual_enum_string_helper")
            && diag.message.contains("manual `Display` impl")
            && diag.message.contains("Mode")
    }));
}

#[test]
fn analyze_workspace_flags_free_enum_string_helper_functions_for_borrowed_inputs() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub enum Scenario {
    HappyPath,
    Failure,
}

pub fn scenario_label(value: &Scenario) -> &'static str {
    match value {
        Scenario::HappyPath => "happy-path",
        Scenario::Failure => "failure",
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_manual_enum_string_helper")
            && diag.message.contains("scenario_label")
            && diag.message.contains("Scenario")
            && diag.message.contains("Display")
    }));
}

#[test]
fn analyze_workspace_flags_parallel_enum_metadata_helpers() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    Crossing,
    OutboundScope,
    WriteBack,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceTerm {
    CRoute,
    ReleaseGate,
}

impl Id {
    pub fn label(self) -> &'static str {
        match self {
            Self::Crossing => "Crossing",
            Self::OutboundScope => "Outbound scope",
            Self::WriteBack => "Write-back",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Crossing => "crossing::Flow",
            Self::OutboundScope => "outbound_scope::Flow",
            Self::WriteBack => "write_back::Flow",
        }
    }

    pub fn source_term(self) -> Option<SourceTerm> {
        match self {
            Self::Crossing => Some(SourceTerm::CRoute),
            Self::OutboundScope => Some(SourceTerm::ReleaseGate),
            Self::WriteBack => None,
        }
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_parallel_enum_metadata_helper")
            && diag.message.contains("Id")
            && diag.message.contains("descriptor")
    }));
}

#[test]
fn analyze_workspace_flags_stringly_model_scaffold_structs() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct MachineDescriptor {
    pub state_path: String,
    pub kind_label: String,
    pub next_machine: String,
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_stringly_model_scaffold")
            && diag.message.contains("MachineDescriptor")
            && diag.message.contains("state_path")
            && diag.message.contains("typed enums")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_plain_text_structs_as_model_scaffolds() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct ContactCard {
    pub first_name: String,
    pub last_name: String,
    pub email: String,
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_stringly_model_scaffold") })
    );
}

#[test]
fn analyze_workspace_flags_stringly_protocol_collections() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub static LEGAL_STATES: &[&str] = &["draft", "published"];
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_stringly_protocol_collection")
            && diag.message.contains("LEGAL_STATES")
            && !diag.is_policy_violation()
    }));
}

#[test]
fn analyze_workspace_does_not_flag_generic_string_collections() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub static TAGS: &[&str] = &["blue", "green"];
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_stringly_protocol_collection") })
    );
}

#[test]
fn analyze_workspace_flags_boolean_protocol_decisions() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct UserId;

pub fn review(approved: bool, actor: UserId) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_boolean_protocol_decision")
            && diag.message.contains("approved")
            && diag.message.contains("review")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_runtime_toggle_bools_as_protocol_decisions() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub fn render(pretty: bool) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_boolean_protocol_decision") })
    );
}

#[test]
fn analyze_workspace_flags_prelude_glob_imports() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use http::prelude::*;

pub fn handle() {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_prelude_glob_import")
            && diag.message.contains("http::prelude::*")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_relative_prelude_glob_imports() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use crate::util::prelude::*;

mod util {
    pub mod prelude {
        pub struct Helper;
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("namespace_prelude_glob_import"))
    );
}

#[test]
fn analyze_workspace_flags_glob_imports_that_flatten_preserved_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use http::*;

pub fn handle() {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_glob_preserve_module") && diag.message.contains("http::*")
    }));
}

#[test]
fn analyze_workspace_flags_raw_string_error_surfaces() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Report {
    pub error: String,
}

pub fn parse() -> Result<(), String> {
    Ok(())
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_string_error_surface")
            && (diag.message.contains("Report") || diag.message.contains("parse"))
    }));
}

#[test]
fn analyze_workspace_does_not_flag_parse_passthrough_string_error_surfaces() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub enum Mode {
    Warn,
    Deny,
}

impl Mode {
    pub fn parse(raw: &str) -> Result<Self, String> {
        raw.parse()
    }
}

pub fn parse_mode(raw: &str) -> Result<Mode, String> {
    Mode::parse(raw)
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_string_error_surface"))
    );
}

#[test]
fn analyze_workspace_does_not_treat_generic_detail_fields_as_error_surfaces() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Draft {
    pub detail: String,
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_string_error_surface"))
    );
}

#[test]
fn analyze_workspace_flags_reason_fields_on_error_like_structs() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Rejected {
    pub reason: String,
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_string_error_surface") && diag.message.contains("Rejected")
    }));
}

#[test]
fn analyze_workspace_flags_anyhow_error_leakage() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Failure {
    pub source: anyhow::Error,
}

pub fn load() -> anyhow::Result<()> {
    Ok(())
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_anyhow_error_surface")
            && (diag.message.contains("Failure") || diag.message.contains("load"))
    }));
}

#[test]
fn analyze_workspace_flags_semantic_string_scalar_boundaries() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub fn connect(email: &str, url: String, locale: String) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_semantic_string_scalar")
            && diag.message.contains("email")
            && diag.message.contains("url")
    }));
}

#[test]
fn analyze_workspace_prefers_protocol_parameter_warning_over_semantic_scalar_for_file_paths() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub fn load(file_path: &str) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_stringly_protocol_parameter"))
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_semantic_string_scalar"))
    );
}

#[test]
fn analyze_workspace_does_not_flag_render_helpers_for_semantic_string_returns() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub fn format_url() -> String {
    String::new()
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_semantic_string_scalar"))
    );
}

#[test]
fn analyze_workspace_extra_semantic_string_scalars_extend_defaults() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(
        root,
        r#"[package.metadata.modum]
extra_semantic_string_scalars = ["mime"]
"#,
    );
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub fn parse(mime: String) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_semantic_string_scalar") && diag.message.contains("mime")
    }));
}

#[test]
fn analyze_workspace_ignored_semantic_string_scalars_remove_defaults() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(
        root,
        r#"[package.metadata.modum]
ignored_semantic_string_scalars = ["url"]
"#,
    );
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub fn connect(url: String, email: String) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_semantic_string_scalar") && diag.message.contains("email")
    }));
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_semantic_string_scalar") && diag.message.contains("url")
    }));
}

#[test]
fn analyze_workspace_surface_profile_keeps_surface_scalar_lints_and_hides_strict_id_lints() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub type UserId = String;

pub fn connect(email: String) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace_with_settings(
        root,
        &AnalysisSettings {
            scan: ScanSettings::default(),
            profile: Some(LintProfile::Surface),
            ignored_diagnostic_codes: Vec::new(),
            baseline: None,
        },
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_semantic_string_scalar"))
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_raw_id_surface"))
    );
}

#[test]
fn analyze_workspace_flags_semantic_numeric_scalar_boundaries() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub fn schedule(duration: u64, timestamp: i64, port: u16) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_semantic_numeric_scalar")
            && diag.message.contains("duration")
            && diag.message.contains("port")
    }));
}

#[test]
fn analyze_workspace_flags_raw_key_value_bag_surfaces() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use std::collections::{BTreeMap, HashMap};

pub struct Request {
    pub headers: HashMap<String, String>,
}

pub fn update(metadata: BTreeMap<String, String>) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_raw_key_value_bag")
            && (diag.message.contains("headers") || diag.message.contains("metadata"))
    }));
}

#[test]
fn analyze_workspace_does_not_treat_labels_as_key_value_bags_by_default() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use std::collections::HashMap;

pub fn labels() -> HashMap<String, String> {
    HashMap::new()
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_raw_key_value_bag"))
    );
}

#[test]
fn analyze_workspace_extra_key_value_bag_names_extend_defaults() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(
        root,
        r#"[package.metadata.modum]
extra_key_value_bag_names = ["labels"]
"#,
    );
    fs::write(
        root.join("src/lib.rs"),
        r#"
use std::collections::HashMap;

pub fn labels() -> HashMap<String, String> {
    HashMap::new()
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_raw_key_value_bag") && diag.message.contains("labels")
    }));
}

#[test]
fn analyze_workspace_flags_boolean_flag_clusters() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Options {
    pub include_archived: bool,
    pub include_deleted: bool,
}

pub fn render(include_archived: bool, include_deleted: bool) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_boolean_flag_cluster")
            && (diag.message.contains("Options") || diag.message.contains("render"))
    }));
}

#[test]
fn analyze_workspace_does_not_flag_runtime_toggle_boolean_clusters() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct RenderOptions {
    pub pretty: bool,
    pub color: bool,
}

pub fn render(pretty: bool, color: bool) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_boolean_flag_cluster"))
    );
}

#[test]
fn analyze_workspace_flags_integer_protocol_boundaries() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub fn transition(status: u8, gate_kind: i32, phase: usize) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_integer_protocol_parameter")
            && diag.message.contains("status")
            && diag.message.contains("gate_kind")
    }));
}

#[test]
fn analyze_workspace_flags_raw_id_surfaces() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub type UserId = String;

pub struct Session {
    pub request_id: u64,
}

pub fn load(user_id: String) -> u64 {
    1
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_raw_id_surface")
            && (diag.message.contains("UserId")
                || diag.message.contains("request_id")
                || diag.message.contains("user_id"))
    }));
}

#[test]
fn analyze_workspace_flags_manual_flag_sets() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub const FLAG_READ: u32 = 1;
pub const FLAG_WRITE: u32 = 2;
pub const FLAG_EXECUTE: u32 = 4;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_manual_flag_set")
            && diag.message.contains("FLAG_READ")
            && diag.message.contains("FLAG_WRITE")
    }));
}

#[test]
fn analyze_workspace_flags_raw_bitmask_boundaries() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Access {
    pub permissions: u32,
}

pub fn update(flags: u64) {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_manual_flag_set")
            && (diag.message.contains("permissions") || diag.message.contains("flags"))
    }));
}

#[test]
fn analyze_workspace_flags_repeated_named_flag_checks_in_public_methods() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Access {
    permissions: u32,
}

impl Access {
    const FLAG_READ: u32 = 1;
    const FLAG_WRITE: u32 = 2;

    pub fn allows_read_and_write(&self) -> bool {
        self.permissions & Self::FLAG_READ != 0 && self.permissions & Self::FLAG_WRITE != 0
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_manual_flag_set")
            && diag.message.contains("Access::allows_read_and_write")
            && diag.message.contains("FLAG_READ")
            && diag.message.contains("FLAG_WRITE")
    }));
}

#[test]
fn analyze_workspace_flags_repeated_named_flag_assembly_in_public_methods() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Access {
    permissions: u32,
}

const FLAG_READ: u32 = 1;
const FLAG_WRITE: u32 = 2;

impl Access {
    pub fn elevated(&self) -> u32 {
        self.permissions | FLAG_READ | FLAG_WRITE
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_manual_flag_set")
            && diag.message.contains("Access::elevated")
            && diag.message.contains("FLAG_READ")
            && diag.message.contains("FLAG_WRITE")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_single_named_flag_check_in_public_methods() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Access {
    permissions: u32,
}

impl Access {
    const FLAG_READ: u32 = 1;

    pub fn allows_read(&self) -> bool {
        self.permissions & Self::FLAG_READ != 0
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_manual_flag_set") && diag.message.contains("Access::allows_read")
    }));
}

#[test]
fn analyze_workspace_flags_manual_error_surfaces() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
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
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_manual_error_surface") && diag.message.contains("RequestError")
    }));
}

#[test]
fn analyze_workspace_flags_parallel_enum_metadata_helpers_with_two_strong_helpers() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub enum Outcome {
    Accepted,
    Rejected,
}

impl Outcome {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }

    pub fn color(&self) -> &'static str {
        match self {
            Self::Accepted => "green",
            Self::Rejected => "red",
        }
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_parallel_enum_metadata_helper")
            && diag.message.contains("code")
            && diag.message.contains("color")
    }));
}

#[test]
fn analyze_workspace_flags_builder_candidates_for_configuration_heavy_entrypoints() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Workflow;

pub fn start(vendor: String, approved: bool, retries: usize, release_gate: bool) -> Workflow {
    Workflow
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_builder_candidate")
            && diag.message.contains("start")
            && diag.message.contains("builder")
    }));
}

#[test]
fn analyze_workspace_flags_repeated_parameter_clusters() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct TaskId;
pub struct Kind;
pub struct AuditRecord;

pub struct Outcome;
pub struct Rejected;

impl Outcome {
    pub fn rejected(
        task_id: TaskId,
        kind: Kind,
        reason: String,
        audit_record: AuditRecord,
    ) -> Self {
        let _ = (task_id, kind, reason, audit_record);
        Self
    }
}

impl Rejected {
    pub fn from_audit(
        task_id: TaskId,
        kind: Kind,
        reason: String,
        audit_record: AuditRecord,
    ) -> Self {
        let _ = (task_id, kind, reason, audit_record);
        Self
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_repeated_parameter_cluster")
            && diag.message.contains("Outcome::rejected")
            && diag.message.contains("Rejected::from_audit")
            && diag.message.contains("task_id")
            && diag.message.contains("bon")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_distinct_domain_constructors_as_builder_candidates() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct UserId;
pub struct UserRepo;
pub struct Policy;

pub struct Session;

impl Session {
    pub fn new(id: UserId, repo: UserRepo, policy: Policy) -> Self {
        Self
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_builder_candidate") })
    );
}

#[test]
fn analyze_workspace_does_not_flag_repeated_clusters_of_strong_domain_types() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct UserId;
pub struct UserRepo;
pub struct Policy;

pub struct Session;
pub struct Login;

impl Session {
    pub fn new(id: UserId, repo: UserRepo, policy: Policy) -> Self {
        let _ = (id, repo, policy);
        Self
    }
}

impl Login {
    pub fn create(id: UserId, repo: UserRepo, policy: Policy) -> Self {
        let _ = (id, repo, policy);
        Self
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_repeated_parameter_cluster") })
    );
}

#[test]
fn analyze_workspace_flags_optional_parameter_builders() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Request;

impl Request {
    pub fn new(id: String, label: Option<String>) -> Self {
        let _ = (id, label);
        Self
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_optional_parameter_builder")
            && diag.message.contains("new")
            && diag.message.contains("label")
            && diag.message.contains("bon")
    }));
}

#[test]
fn analyze_workspace_flags_defaulted_optional_parameters() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Request;

impl Request {
    pub fn new(id: String, label: Option<String>) -> Self {
        let _ = (id, label.unwrap_or_default());
        Self
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_defaulted_optional_parameter")
            && diag.message.contains("label")
            && diag.message.contains("bon")
    }));
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_optional_parameter_builder") })
    );
}

#[test]
fn analyze_workspace_flags_maybe_some_callsites() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Request;
pub struct Builder;

impl Request {
    pub fn builder() -> Builder {
        Builder
    }
}

impl Builder {
    pub fn label(self, _label: String) -> Self {
        self
    }

    pub fn maybe_label(self, _label: Option<String>) -> Self {
        self
    }

    pub fn maybe_note(self, _note: Option<String>) -> Self {
        self
    }
}

pub fn build() {
    let _ = Request::builder()
        .maybe_label(Some("ready".to_string()))
        .maybe_note(std::option::Option::Some("set".to_string()));
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    let maybe_some = report
        .diagnostics
        .iter()
        .filter(|diag| diag.code() == Some("callsite_maybe_some"))
        .collect::<Vec<_>>();
    assert_eq!(maybe_some.len(), 2);
    assert!(
        maybe_some
            .iter()
            .any(|diag| diag.message.contains("maybe_label") && diag.message.contains("label"))
    );
    assert!(
        maybe_some
            .iter()
            .any(|diag| diag.message.contains("maybe_note") && diag.message.contains("note"))
    );
}

#[test]
fn analyze_workspace_does_not_flag_maybe_some_for_real_option_forwarding() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Request;
pub struct Builder;

impl Request {
    pub fn builder() -> Builder {
        Builder
    }
}

impl Builder {
    pub fn label(self, _label: String) -> Self {
        self
    }

    pub fn maybe_label(self, _label: Option<String>) -> Self {
        self
    }

    pub fn with_label(self, _label: Option<String>) -> Self {
        self
    }
}

pub fn build(label: Option<String>) {
    let _ = Request::builder()
        .maybe_label(label)
        .with_label(Some("ready".to_string()))
        .label("direct".to_string());
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("callsite_maybe_some"))
    );
}

#[test]
fn analyze_workspace_flags_optional_parameter_builders_for_free_functions() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Workflow;
pub struct Task;
pub struct Audit;

pub fn start(task: Task, audit: Option<Audit>) -> Workflow {
    let _ = (task, audit);
    Workflow
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_optional_parameter_builder")
            && diag.message.contains("start")
            && diag.message.contains("audit")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_optional_parameters_for_non_builderish_helpers() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Report;
pub struct Theme;

pub fn summarize(report: Report, theme: Option<Theme>) -> Report {
    let _ = theme;
    report
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_optional_parameter_builder") })
    );
}

#[test]
fn analyze_workspace_does_not_flag_builder_annotated_entrypoints() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "bon = \"3\"");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use bon::bon;

pub struct Workflow;

#[bon]
impl Workflow {
    #[builder]
    pub fn start(
        vendor: String,
        approved: bool,
        retries: usize,
        release_gate: Option<bool>,
    ) -> Self {
        let _ = (vendor, approved, retries, release_gate);
        Self
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        matches!(
            diag.code(),
            Some("api_builder_candidate" | "api_optional_parameter_builder")
        )
    }));
}

#[test]
fn analyze_workspace_flags_standalone_builder_surfaces() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Request;
pub struct Vendor;
pub struct Scope;
pub struct Release;

pub fn with_vendor(request: Request, _vendor: Vendor) -> Request {
    request
}

pub fn with_scope(request: Request, _scope: Scope) -> Request {
    request
}

pub fn with_release(request: Request, _release: Release) -> Request {
    request
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_standalone_builder_surface")
            && diag.message.contains("Request")
            && diag.message.contains("with_vendor")
    }));
}

#[test]
fn analyze_workspace_flags_forwarding_compat_wrappers() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct Request;
pub struct Action;

impl From<Request> for Action {
    fn from(_value: Request) -> Self {
        Self
    }
}

impl Request {
    pub fn to_action(self) -> Action {
        Action::from(self)
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_forwarding_compat_wrapper")
            && diag.message.contains("to_action")
            && diag.message.contains("From<Request> for Action")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_noun_convenience_wrappers_for_from_impls() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct App;
pub struct Request;

impl From<&App> for Request {
    fn from(_value: &App) -> Self {
        Self
    }
}

impl App {
    pub fn request(&self) -> Request {
        self.into()
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_forwarding_compat_wrapper") })
    );
}

#[test]
fn analyze_workspace_flags_strum_serialize_all_candidates() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use strum::{AsRefStr, Display, EnumString};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, AsRefStr)]
pub enum Scenario {
    #[strum(serialize = "happy-path")]
    HappyPath,
    #[strum(serialize = "ttl-expired")]
    TtlExpired,
    #[strum(serialize = "policy-denied")]
    PolicyDenied,
    #[strum(serialize = "audit-failure")]
    AuditFailure,
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_strum_serialize_all_candidate")
            && diag.message.contains("serialize_all")
            && diag.message.contains("kebab-case")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_strum_aliases_as_serialize_all_candidates() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use strum::{AsRefStr, Display, EnumString};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, AsRefStr)]
pub enum Scenario {
    #[strum(serialize = "c-route", serialize = "C-Route")]
    CRoute,
    #[strum(serialize = "release-gate")]
    ReleaseGate,
    #[strum(serialize = "audit-access")]
    AuditAccess,
    #[strum(serialize = "write-back")]
    WriteBack,
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_strum_serialize_all_candidate") })
    );
}

#[test]
fn analyze_workspace_flags_ad_hoc_parse_helpers_for_public_enums() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub enum Mode {
    Warn,
    Deny,
}

pub fn parse_mode(raw: &str) -> Result<Mode, String> {
    match raw {
        "warn" => Ok(Mode::Warn),
        "deny" => Ok(Mode::Deny),
        _ => Err(format!("unknown mode: {raw}")),
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_ad_hoc_parse_helper")
            && diag.message.contains("parse_mode")
            && diag.message.contains("FromStr")
    }));
}

#[test]
fn analyze_workspace_flags_inherent_enum_parse_helpers_returning_self() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub enum Mode {
    Warn,
    Deny,
}

impl Mode {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "warn" => Ok(Self::Warn),
            "deny" => Ok(Self::Deny),
            _ => Err(format!("unknown mode: {raw}")),
        }
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_ad_hoc_parse_helper")
            && diag.message.contains("Mode")
            && diag.message.contains("`parse`")
            && diag.message.contains("FromStr")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_parse_helpers_when_enum_already_implements_from_str() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub enum Mode {
    Warn,
    Deny,
}

impl std::str::FromStr for Mode {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "warn" => Ok(Self::Warn),
            "deny" => Ok(Self::Deny),
            _ => Err(format!("unknown mode: {raw}")),
        }
    }
}

pub fn parse_mode(raw: &str) -> Result<Mode, String> {
    raw.parse()
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_ad_hoc_parse_helper") })
    );
}

#[test]
fn analyze_workspace_does_not_flag_inherent_parse_helpers_when_enum_already_implements_from_str() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub enum Mode {
    Warn,
    Deny,
}

impl std::str::FromStr for Mode {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "warn" => Ok(Self::Warn),
            "deny" => Ok(Self::Deny),
            _ => Err(format!("unknown mode: {raw}")),
        }
    }
}

impl Mode {
    pub fn parse(raw: &str) -> Result<Self, String> {
        raw.parse()
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_ad_hoc_parse_helper") })
    );
}

#[test]
fn analyze_workspace_does_not_flag_candidate_semantic_module_when_surface_already_exists() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod user;

pub struct UserRepository;
pub struct UserId;
"#,
    )
    .expect("write lib");
    fs::write(
        root.join("src/user.rs"),
        r#"
pub struct Repository;
pub struct Id;
"#,
    )
    .expect("write user");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_candidate_semantic_module") })
    );
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
        diag.code() == Some("api_weak_module_generic_leaf")
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
        diag.code() == Some("api_redundant_category_suffix")
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
        diag.code() == Some("api_catch_all_module")
            && diag.message.contains("catch-all public module")
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
        diag.code() == Some("api_repeated_module_segment")
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
        diag.level() == DiagnosticLevel::Error
            && diag.code() == Some("api_organizational_submodule_flatten")
            && diag.message.contains("partials::error::Error")
    }));
}
