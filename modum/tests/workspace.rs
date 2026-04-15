use std::fs;

use modum::{
    AnalysisSettings, DiagnosticLevel, ScanSettings, analyze_workspace,
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
fn analyze_workspace_ignores_workspace_profile_and_runs_full_lint_set() {
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
        report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_semantic_module"))
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diag| { diag.message.contains("`metadata.modum.profile` is ignored") })
    );
}

#[test]
fn analyze_workspace_ignores_package_and_workspace_profiles_and_warns_for_both() {
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
        report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_semantic_module"))
    );
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|diag| diag.message.contains("`metadata.modum.profile` is ignored"))
            .count(),
        2
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
fn analyze_workspace_analysis_settings_no_longer_filter_lints() {
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
            ignored_diagnostic_codes: Vec::new(),
            baseline: None,
        },
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("api_candidate_semantic_module") })
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diag| { diag.message.contains("`metadata.modum.profile` is ignored") })
    );
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
            && diag.is_advisory_warning()
            && diag.message.contains("user::UserRepository")
            && diag.message.contains("user::Repository")
    }));
}

#[test]
fn analyze_workspace_keeps_adapter_context_internal_leaf_rule_as_policy() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod sqlite {
    struct SqliteAuditLogStore;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_adapter_redundant_leaf_context")
            && diag.is_policy_violation()
            && diag.message.contains("sqlite::SqliteAuditLogStore")
            && diag.message.contains("sqlite::AuditLogStore")
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
fn analyze_workspace_suppresses_internal_redundant_leaf_context_for_replay_and_trace_machinery() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod replay {
    mod machine {
        struct ReplayMachine;
    }
}

mod trace {
    mod model {
        struct ModelEncoding;
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
            .any(|diag| diag.code() == Some("internal_redundant_leaf_context"))
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("internal_adapter_redundant_leaf_context"))
    );
}

#[test]
fn analyze_workspace_suppresses_internal_redundant_leaf_context_for_parts_leafs() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod runtime {
    struct RuntimeParts;
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
fn analyze_workspace_does_not_flag_internal_redundant_leaf_context_for_role_suffix_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod access {
    struct AuthorizedRecordAccess;
}

mod actions {
    struct SectionActions;
}

mod copy {
    struct SectionCopy;
    struct LeadCopy;
}

mod section_card {
    struct CaseSectionCard;
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
fn analyze_workspace_keeps_internal_redundant_leaf_context_for_viewer_resolution_family() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod viewer {
    enum ViewerResolutionState {
        Incoming,
    }

    struct ViewerResolutionFlow;
    enum ViewerResolutionOutcome {
        Ready,
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_redundant_leaf_context")
            && diag.message.contains("viewer::ViewerResolutionState")
            && diag.message.contains("viewer::ResolutionState")
    }));
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_redundant_leaf_context")
            && diag.message.contains("viewer::ViewerResolutionFlow")
            && diag.message.contains("viewer::ResolutionFlow")
    }));
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_redundant_leaf_context")
            && diag.message.contains("viewer::ViewerResolutionOutcome")
            && diag.message.contains("viewer::ResolutionOutcome")
    }));
}

#[test]
fn analyze_workspace_flags_internal_candidate_semantic_module_for_shared_head_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod pilot_acceptance;
mod pilot_bootstrap;
mod pilot_live_ops;
mod pilot_mock_harness;
mod pilot_recovery;
mod pilot_wedge;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_candidate_semantic_module")
            && diag.message.contains("pilot_acceptance")
            && diag.message.contains("pilot_bootstrap")
            && diag.message.contains("pilot_live_ops")
            && diag
                .message
                .contains("pilot::{acceptance, bootstrap, live_ops, mock_harness, recovery, wedge}")
    }));
}

#[test]
fn analyze_workspace_flags_internal_candidate_semantic_module_for_shared_tail_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod finalize_stage;
mod inbound_stage;
mod intake_stage;
mod prepare_stage;
mod release_stage;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_candidate_semantic_module")
            && diag.message.contains("finalize_stage")
            && diag.message.contains("inbound_stage")
            && diag.message.contains("intake_stage")
            && diag
                .message
                .contains("stage::{finalize, inbound, intake, prepare, release}")
    }));
}

#[test]
fn analyze_workspace_flags_internal_candidate_semantic_module_for_private_args_family() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
struct CaseDetailArgs;
struct CaseExceptionArgs;
struct CaseAuditArgs;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_candidate_semantic_module")
            && diag.message.contains("CaseDetailArgs")
            && diag.message.contains("CaseExceptionArgs")
            && diag.message.contains("CaseAuditArgs")
            && diag
                .message
                .contains("case::args::{Audit, Detail, Exception}")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_internal_candidate_semantic_module_for_shared_head_items_without_shared_tail()
 {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
struct ReleaseOutcome;
enum ReleaseResult {}
enum ReleaseStatus {}
struct ReleaseReport;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("internal_candidate_semantic_module"))
    );
}

#[test]
fn analyze_workspace_skips_internal_candidate_semantic_module_for_bare_head_tail_member_shape() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
struct GovernanceArgs;
struct GovernanceBoardArgs;
struct GovernanceHistoryArgs;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("internal_candidate_semantic_module"))
    );
}

#[test]
fn analyze_workspace_flags_internal_candidate_semantic_module_for_two_item_head_family_in_compound_scope()
 {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod alpha_vendor_http;
mod epic_fhir;
mod epic_write_back;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_candidate_semantic_module")
            && diag.message.contains("epic_fhir")
            && diag.message.contains("epic_write_back")
            && diag.message.contains("epic::{fhir, write_back}")
    }));
}

#[test]
fn analyze_workspace_flags_internal_flat_namespace_preserving_module() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod alpha_vendor_http;
mod epic_write_back;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_flat_namespace_preserving_module")
            && diag.message.contains("alpha_vendor_http")
            && diag.message.contains("alpha_vendor::http")
    }));
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_flat_namespace_preserving_module")
            && diag.message.contains("epic_write_back")
            && diag.message.contains("epic::write_back")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_internal_flat_namespace_preserving_module_for_transport_tail() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "mod fhir_transport;\n").expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("internal_flat_namespace_preserving_module"))
    );
}

#[test]
fn analyze_workspace_flags_internal_path_shim_module_for_parent_traversal() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod flow {
    mod room {
        #[path = "../create_room_flow.rs"]
        mod create;
    }
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_path_shim_module")
            && diag.message.contains("flow::room::create")
            && diag.message.contains("../create_room_flow.rs")
    }));
}

#[test]
fn analyze_workspace_flags_internal_path_shim_module_for_renamed_file_stem() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod flow {
    #[path = "request_meta_flow.rs"]
    mod request_meta;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_path_shim_module")
            && diag.message.contains("flow::request_meta")
            && diag.message.contains("request_meta_flow.rs")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_natural_same_name_path_module() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
#[path = "request_meta.rs"]
mod request_meta;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("internal_path_shim_module"))
    );
}

#[test]
fn analyze_workspace_does_not_flag_natural_same_name_path_module_with_dot_prefix() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/flow")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "mod flow;\n").expect("write lib");
    fs::write(
        root.join("src/flow.rs"),
        r#"
#[path = "./flow/request_meta.rs"]
mod request_meta;
"#,
    )
    .expect("write flow");
    fs::write(root.join("src/flow/request_meta.rs"), "struct Marker;\n")
        .expect("write request_meta");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("internal_path_shim_module"))
    );
}

#[test]
fn analyze_workspace_flags_same_name_sibling_path_module_under_nested_namespace() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "mod flow;\n").expect("write lib");
    fs::write(
        root.join("src/flow.rs"),
        r#"
#[path = "request_meta.rs"]
mod request_meta;
"#,
    )
    .expect("write flow");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_path_shim_module")
            && diag.message.contains("flow::request_meta")
            && diag.message.contains("request_meta.rs")
    }));
}

#[test]
fn analyze_workspace_skips_cfg_selected_path_module() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
#[cfg(unix)]
#[path = "linux.rs"]
mod platform;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("internal_path_shim_module"))
    );
}

#[test]
fn analyze_workspace_does_not_let_unrelated_cfg_attr_hide_path_shim_module() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
#[cfg_attr(test, allow(dead_code))]
#[path = "request_meta_flow.rs"]
mod request_meta;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_path_shim_module")
            && diag.message.contains("request_meta_flow.rs")
    }));
}

#[test]
fn analyze_workspace_flags_public_path_shim_module() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod flow {
    #[path = "request_meta_flow.rs"]
    pub mod request_meta;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_path_shim_module")
            && diag.message.contains("flow::request_meta")
            && diag.message.contains("request_meta_flow.rs")
    }));
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
fn analyze_workspace_flags_internal_technical_bucket_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod dependencies {
    trait ScopeReview {}
}

mod ids {
    struct TaskId;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_catch_all_module") && diag.message.contains("dependencies")
    }));
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_catch_all_module") && diag.message.contains("ids")
    }));
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
fn analyze_workspace_suppresses_internal_organizational_module_at_private_crate_root() {
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
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("internal_organizational_submodule_flatten"))
    );
}

#[test]
fn analyze_workspace_flags_internal_organizational_module_when_nested() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod adapter {
    mod response {
        struct Payload;
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
fn analyze_workspace_prefers_promotable_parent_surface_for_alias_paths_with_redundant_canonical_leaf()
 {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use crate::workflow::inbound::source_update as source_update_boundary;

pub mod workflow {
    pub mod inbound {
        pub mod source_update {
            pub trait SourceUpdate {}
        }
    }
}

pub fn keep(boundary: &dyn source_update_boundary::SourceUpdate) -> &dyn source_update_boundary::SourceUpdate {
    boundary
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diag| diag.code() == Some("namespace_aliased_qualified_path"))
        .expect("namespace alias diagnostic");
    assert!(
        diagnostic
            .message
            .contains("source_update_boundary::SourceUpdate")
    );
    assert!(
        diagnostic
            .message
            .contains("prefer `inbound::SourceUpdate`")
    );
    assert!(diagnostic.fix.is_none());
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|diag| {
                diag.code() == Some("namespace_aliased_qualified_path")
                    && diag
                        .message
                        .contains("source_update_boundary::SourceUpdate")
            })
            .count(),
        1
    );
}

#[test]
fn analyze_workspace_prefers_promotable_parent_surface_for_redundant_same_name_namespace_paths() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use crate::workflow::inbound::source_update;

pub mod workflow {
    pub mod inbound {
        pub mod source_update {
            pub trait SourceUpdate {}
        }
    }
}

pub fn keep(boundary: &dyn source_update::SourceUpdate) -> &dyn source_update::SourceUpdate {
    boundary
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diag| diag.code() == Some("namespace_redundant_qualified_generic"))
        .expect("redundant qualified path diagnostic");
    assert!(diagnostic.message.contains("source_update::SourceUpdate"));
    assert!(
        diagnostic
            .message
            .contains("prefer `inbound::SourceUpdate`")
    );
    assert!(diagnostic.fix.is_none());
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|diag| {
                diag.code() == Some("namespace_redundant_qualified_generic")
                    && diag.message.contains("source_update::SourceUpdate")
            })
            .count(),
        1
    );
}

#[test]
fn analyze_workspace_prefers_promotable_parent_surface_for_owned_workspace_crates() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("app/src")).expect("create app src");
    fs::create_dir_all(root.join("domain/src")).expect("create domain src");
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "2"
members = ["app", "domain"]
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
    fs::write(
        root.join("domain/Cargo.toml"),
        r#"[package]
name = "domain"
version = "0.1.0"
edition = "2024"
"#,
    )
    .expect("write domain manifest");
    fs::write(
        root.join("app/src/lib.rs"),
        r#"
pub fn keep(user: domain::user::User) -> domain::user::User {
    user
}
"#,
    )
    .expect("write app source");
    fs::write(root.join("domain/src/lib.rs"), "pub mod user;\n").expect("write domain lib");
    fs::write(root.join("domain/src/user.rs"), "pub struct User;\n").expect("write domain user");

    let report = analyze_workspace(root, &[]);
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diag| {
            diag.code() == Some("namespace_redundant_qualified_generic")
                && diag.message.contains("domain::user::User")
        })
        .expect("owned workspace crate diagnostic");
    assert!(diagnostic.message.contains("prefer `domain::User`"));
    assert!(diagnostic.fix.is_none());
}

#[test]
fn analyze_workspace_prefers_promotable_parent_surface_for_owned_workspace_custom_lib_names() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("app/src")).expect("create app src");
    fs::create_dir_all(root.join("domain/src")).expect("create domain src");
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "2"
members = ["app", "domain"]
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
    fs::write(
        root.join("domain/Cargo.toml"),
        r#"[package]
name = "domain-package"
version = "0.1.0"
edition = "2024"

[lib]
name = "domain_api"
"#,
    )
    .expect("write domain manifest");
    fs::write(
        root.join("app/src/lib.rs"),
        r#"
pub fn keep(user: domain_api::user::User) -> domain_api::user::User {
    user
}
"#,
    )
    .expect("write app source");
    fs::write(root.join("domain/src/lib.rs"), "pub mod user;\n").expect("write domain lib");
    fs::write(root.join("domain/src/user.rs"), "pub struct User;\n").expect("write domain user");

    let report = analyze_workspace(root, &[]);
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diag| {
            diag.code() == Some("namespace_redundant_qualified_generic")
                && diag.message.contains("domain_api::user::User")
        })
        .expect("owned workspace custom lib diagnostic");
    assert!(diagnostic.message.contains("prefer `domain_api::User`"));
    assert!(diagnostic.fix.is_none());
}

#[test]
fn analyze_workspace_skips_promotable_parent_surface_when_ancestor_bindings_conflict() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
use crate::workflow::inbound::source_update;

pub struct SourceUpdate;

pub mod workflow {
    pub struct SourceUpdate;

    pub mod inbound {
        pub struct SourceUpdate;

        pub mod source_update {
            pub trait SourceUpdate {}
        }
    }
}

pub fn keep(boundary: &dyn source_update::SourceUpdate) -> &dyn source_update::SourceUpdate {
    boundary
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_redundant_qualified_generic")
            && diag.message.contains("source_update::SourceUpdate")
    }));
}

#[test]
fn analyze_workspace_does_not_promote_parent_surface_for_external_crates() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub fn keep(value: axum::response::Response) -> axum::response::Response {
    value
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_redundant_qualified_generic")
            && diag.message.contains("axum::response::Response")
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
fn analyze_workspace_flags_public_reexports_with_standalone_shorter_names() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod auth {
    pub struct AuthStrategy;
    pub struct NoAuth;
}

pub mod config {
    pub struct PracticeFusionConfig;
}

pub use auth::{AuthStrategy, NoAuth};
pub use config::PracticeFusionConfig;
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_pub_use_redundant_leaf_context")
            && diag.message.contains("AuthStrategy")
            && diag.message.contains("auth::Strategy")
    }));
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_pub_use_redundant_leaf_context")
            && diag.message.contains("PracticeFusionConfig")
            && diag.message.contains("config::PracticeFusion")
    }));
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_pub_use_redundant_leaf_context")
            && diag.message.contains("NoAuth")
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
fn analyze_workspace_flags_tail_family_candidate_semantic_module_for_partial_existing_namespace() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/terminal")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod terminal;\n").expect("write lib");
    fs::write(
        root.join("src/terminal.rs"),
        r#"
pub mod outcome;
pub mod toxicity;
pub struct CompletedOutcome;
pub struct RejectedOutcome;
pub enum Outcome {
    Completed(CompletedOutcome),
    Pending(outcome::Pending),
    Rejected(RejectedOutcome),
    Toxicity(toxicity::Outcome),
}
"#,
    )
    .expect("write terminal");
    fs::write(
        root.join("src/terminal/outcome.rs"),
        "pub struct Pending;\n",
    )
    .expect("write outcome");
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
                .contains("outcome::{Completed, Pending, Rejected, Toxicity}")
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
fn analyze_workspace_flags_overqualified_callsite_function_paths() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/trace_log/demo")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod trace_log;

pub fn shorten(value: &str) -> String {
    crate::trace_log::demo::chat::short_hyphenated_text(value)
}
"#,
    )
    .expect("write source");
    fs::write(root.join("src/trace_log.rs"), "pub mod demo;\n").expect("write trace_log");
    fs::write(root.join("src/trace_log/demo.rs"), "pub mod chat;\n").expect("write demo");
    fs::write(
        root.join("src/trace_log/demo/chat.rs"),
        r#"
pub fn short_hyphenated_text(value: &str) -> String {
    value.split('-').next().unwrap_or(value).to_string()
}
"#,
    )
    .expect("write chat");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_overqualified_callsite_path")
            && diag
                .message
                .contains("crate::trace_log::demo::chat::short_hyphenated_text")
            && diag
                .message
                .contains("prefer `chat::short_hyphenated_text`")
    }));
}

#[test]
fn analyze_workspace_flags_overqualified_callsite_type_paths() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/views/partials/components/portfolio")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod views;

pub fn keep(value: crate::views::partials::components::portfolio::content::CmsActionLink) -> crate::views::partials::components::portfolio::content::CmsActionLink {
    value
}
"#,
    )
    .expect("write lib");
    fs::write(root.join("src/views.rs"), "pub mod partials;\n").expect("write views");
    fs::write(root.join("src/views/partials.rs"), "pub mod components;\n").expect("write partials");
    fs::write(
        root.join("src/views/partials/components.rs"),
        "pub mod portfolio;\n",
    )
    .expect("write components");
    fs::write(
        root.join("src/views/partials/components/portfolio.rs"),
        "pub mod content;\n",
    )
    .expect("write portfolio");
    fs::write(
        root.join("src/views/partials/components/portfolio/content.rs"),
        "pub struct CmsActionLink;\n",
    )
    .expect("write content");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_overqualified_callsite_path")
            && diag
                .message
                .contains("crate::views::partials::components::portfolio::content::CmsActionLink")
            && diag.message.contains("prefer `content::CmsActionLink`")
    }));
}

#[test]
fn analyze_workspace_keeps_two_segment_visible_suffix_for_deep_generic_leaves() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/domain/chat/room")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod domain;

pub fn keep(source: domain::chat::room::name::Error) -> domain::chat::room::name::Error {
    source
}
"#,
    )
    .expect("write lib");
    fs::write(root.join("src/domain.rs"), "pub mod chat;\n").expect("write domain");
    fs::write(root.join("src/domain/chat.rs"), "pub mod room;\n").expect("write chat");
    fs::write(root.join("src/domain/chat/room.rs"), "pub mod name;\n").expect("write room");
    fs::write(
        root.join("src/domain/chat/room/name.rs"),
        "pub struct Error;\n",
    )
    .expect("write name");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_overqualified_callsite_path")
            && diag.message.contains("domain::chat::room::name::Error")
            && diag.message.contains("prefer `room::name::Error`")
    }));
}

#[test]
fn analyze_workspace_prefers_existing_namespace_binding_for_overqualified_callsites() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/app")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod app;

use app::auth;

pub fn keep(source: app::auth::Error) -> app::auth::Error {
    source
}
"#,
    )
    .expect("write lib");
    fs::write(root.join("src/app.rs"), "pub mod auth;\n").expect("write app");
    fs::write(root.join("src/app/auth.rs"), "pub struct Error;\n").expect("write auth");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_overqualified_callsite_path")
            && diag
                .message
                .contains("call through existing `auth` namespace")
            && diag.message.contains("prefer `auth::Error`")
    }));
}

#[test]
fn analyze_workspace_prefers_shorter_import_over_long_existing_namespace_path() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/views/partials/components/portfolio")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod views;

use crate::views::partials;

pub fn keep(
    value: crate::views::partials::components::portfolio::content::CmsActionLink,
) -> crate::views::partials::components::portfolio::content::CmsActionLink {
    value
}
"#,
    )
    .expect("write lib");
    fs::write(root.join("src/views.rs"), "pub mod partials;\n").expect("write views");
    fs::write(root.join("src/views/partials.rs"), "pub mod components;\n").expect("write partials");
    fs::write(
        root.join("src/views/partials/components.rs"),
        "pub mod portfolio;\n",
    )
    .expect("write components");
    fs::write(
        root.join("src/views/partials/components/portfolio.rs"),
        "pub mod content;\n",
    )
    .expect("write portfolio");
    fs::write(
        root.join("src/views/partials/components/portfolio/content.rs"),
        "pub struct CmsActionLink;\n",
    )
    .expect("write content");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_overqualified_callsite_path")
            && diag.message.contains("prefer `content::CmsActionLink`")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_overqualified_callsite_paths_under_weak_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/http/helpers")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod http;

pub fn keep(value: &str) -> String {
    crate::http::helpers::trimmed(value)
}
"#,
    )
    .expect("write lib");
    fs::write(root.join("src/http.rs"), "pub mod helpers;\n").expect("write http");
    fs::write(
        root.join("src/http/helpers.rs"),
        r#"
pub fn trimmed(value: &str) -> String {
    value.trim().to_string()
}
"#,
    )
    .expect("write helpers");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("namespace_overqualified_callsite_path"))
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
fn analyze_workspace_flags_flattened_imports_with_standalone_shorter_names() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod auth {
    pub struct AuthStrategy;
    pub struct NoAuth;
}

use auth::{AuthStrategy, NoAuth};

struct Demo(AuthStrategy, NoAuth);
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_use_redundant_leaf_context")
            && diag.message.contains("AuthStrategy")
            && diag.message.contains("auth::Strategy")
    }));
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_use_redundant_leaf_context")
            && diag.message.contains("NoAuth")
    }));
}

#[test]
fn analyze_workspace_flags_flattened_imports_for_compound_family_leaves() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod viewer {
    pub struct ResolutionFlow;
    pub enum ResolutionState {
        Ready,
    }
    pub enum ResolutionOutcome {
        Settled,
    }
}

use viewer::ResolutionFlow;

struct Demo(ResolutionFlow);
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_use")
            && diag.message.contains("ResolutionFlow")
            && diag.message.contains("viewer::ResolutionFlow")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_flattened_imports_for_single_compound_leaves() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod viewer {
    pub struct ResolutionFlow;
}

use viewer::ResolutionFlow;

struct Demo(ResolutionFlow);
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_use") && diag.message.contains("viewer::ResolutionFlow")
    }));
}

#[test]
fn analyze_workspace_flags_type_aliases_for_compound_family_leaves() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod viewer {
    pub struct ResolutionFlow;
    pub enum ResolutionState {
        Ready,
    }
}

type Flow = viewer::ResolutionFlow;
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_type_alias")
            && diag.message.contains("viewer::ResolutionFlow")
    }));
}

#[test]
fn analyze_workspace_flags_renamed_imports_for_compound_family_leaves() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod viewer {
    pub struct ResolutionFlow;
    pub enum ResolutionState {
        Ready,
    }
}

use viewer::ResolutionFlow as Flow;

struct Demo(Flow);
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_use")
            && diag.message.contains("ResolutionFlow")
            && diag.message.contains("viewer::ResolutionFlow")
    }));
}

#[test]
fn analyze_workspace_flags_flattened_imports_for_compound_family_leaves_in_nested_inline_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod outer {
    mod viewer {
        pub struct ResolutionFlow;
        pub enum ResolutionState {
            Ready,
        }
        pub enum ResolutionOutcome {
            Settled,
        }
    }

    use self::viewer::ResolutionFlow;

    struct Demo(ResolutionFlow);
}
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_use")
            && diag.message.contains("ResolutionFlow")
            && diag.message.contains("viewer::ResolutionFlow")
    }));
}

#[test]
fn analyze_workspace_does_not_treat_private_imported_helpers_as_family_witnesses() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod external {
    pub struct ResolutionState;
    pub struct ResolutionOutcome;
}

mod viewer {
    use crate::external::{ResolutionOutcome, ResolutionState};

    pub struct ResolutionFlow;

    pub fn helper(_: ResolutionState, _: ResolutionOutcome) {}
}

use viewer::ResolutionFlow;

struct Demo(ResolutionFlow);
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_use") && diag.message.contains("viewer::ResolutionFlow")
    }));
}

#[test]
fn analyze_workspace_reports_namespace_family_unsupported_constructs() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod viewer {
    macro_rules! family {
        () => {
            pub struct ResolutionState;
            pub struct ResolutionOutcome;
        };
    }

    pub struct ResolutionFlow;

    family!();
}

use viewer::ResolutionFlow;

struct Demo(ResolutionFlow);
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_family_unsupported_construct")
            && diag.is_advisory_warning()
            && diag.message.contains("ResolutionFlow")
    }));
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_flat_use") && diag.message.contains("viewer::ResolutionFlow")
    }));
}

#[test]
fn analyze_workspace_reports_namespace_family_unsupported_construct_for_type_aliases_with_alias_wording()
 {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod viewer {
    macro_rules! family {
        () => {
            pub struct ResolutionState;
            pub struct ResolutionOutcome;
        };
    }

    pub struct ResolutionFlow;

    family!();
}

type Flow = viewer::ResolutionFlow;
"#,
    )
    .expect("write source");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("namespace_family_unsupported_construct")
            && diag.message.contains("type alias")
            && diag.message.contains("viewer::ResolutionFlow")
    }));
}

#[test]
fn analyze_workspace_groups_macro_leaf_error_namespace_boundary_and_reports_facet_candidate() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/user")).expect("create user module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod user;\n").expect("write lib");
    fs::write(
        root.join("src/user/mod.rs"),
        r#"
macro_rules! declare_supporting_items {
    () => {
        const _: () = ();
    };
}

declare_supporting_items!();

pub mod error;
pub use error::Error;

pub struct Username(String);
pub struct Email(String);
"#,
    )
    .expect("write user mod");
    fs::write(
        root.join("src/user/error.rs"),
        r#"
use crate::user::{EmailError, UsernameError};

pub enum Error {
    Username { source: UsernameError },
    Email { source: EmailError },
}
"#,
    )
    .expect("write user error");

    let report = analyze_workspace(root, &[]);
    let namespace_diags = report
        .diagnostics
        .iter()
        .filter(|diag| diag.code() == Some("namespace_family_unsupported_construct"))
        .collect::<Vec<_>>();
    assert_eq!(namespace_diags.len(), 1);
    assert!(
        namespace_diags[0]
            .message
            .contains("`EmailError`, `UsernameError`")
    );
    assert!(namespace_diags[0].message.contains("owning facet"));
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_facet_module")
            && diag.message.contains("user::Error")
            && diag.message.contains("user::email")
            && diag.message.contains("user::username")
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
fn analyze_workspace_flags_public_redundant_leaf_context_for_standalone_shorter_names() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod auth {
    pub struct AuthStrategy;
    pub struct NoAuth;
}

pub mod client {
    pub struct PracticeFusionClient;
}

pub mod config {
    pub struct EpicConfig;
}

pub mod factory {
    pub struct EhrClientFactory;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_redundant_leaf_context")
            && diag.message.contains("auth::AuthStrategy")
            && diag.message.contains("auth::Strategy")
    }));
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_redundant_leaf_context")
            && diag.message.contains("client::PracticeFusionClient")
            && diag.message.contains("client::PracticeFusion")
    }));
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_redundant_leaf_context")
            && diag.message.contains("config::EpicConfig")
            && diag.message.contains("config::Epic")
    }));
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_redundant_leaf_context")
            && diag.message.contains("factory::EhrClientFactory")
            && diag.message.contains("factory::EhrClient")
    }));
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_redundant_leaf_context") && diag.message.contains("auth::NoAuth")
    }));
}

#[test]
fn analyze_workspace_keeps_public_redundant_leaf_context_for_semantic_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod stmt {
    pub struct ExprStmt;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_redundant_leaf_context")
            && diag.message.contains("stmt::ExprStmt")
            && diag.message.contains("stmt::Expr")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_fragmentary_category_suffix_shortening() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/http")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod http;\n").expect("write lib");
    fs::write(root.join("src/http.rs"), "pub mod response;\n").expect("write http");
    fs::write(
        root.join("src/http/response.rs"),
        r#"
pub struct IntoResponse;
"#,
    )
    .expect("write response");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_redundant_category_suffix")
            && diag.message.contains("IntoResponse")
    }));
}

#[test]
fn analyze_workspace_does_not_flag_internal_fragmentary_redundant_leaf_context() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod test {
    struct TestResult;
}

mod body {
    struct BoxBody;
}

mod query {
    struct OptionalQuery;
}

mod error {
    struct IntoError;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        (diag.code() == Some("internal_redundant_leaf_context")
            && (diag.message.contains("body::BoxBody")
                || diag.message.contains("query::OptionalQuery")))
            || (diag.code() == Some("internal_redundant_category_suffix")
                && diag.message.contains("IntoError"))
    }));
}

#[test]
fn analyze_workspace_flags_internal_redundant_leaf_context_for_standalone_shorter_names() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
mod auth {
    struct AuthStrategy;
}

mod config {
    struct EpicConfig;
}

mod factory {
    struct EhrClientFactory;
}

mod mock {
    struct MockEhrClient;
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_redundant_leaf_context")
            && diag.message.contains("auth::AuthStrategy")
            && diag.message.contains("auth::Strategy")
    }));
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_redundant_leaf_context")
            && diag.message.contains("config::EpicConfig")
            && diag.message.contains("config::Epic")
    }));
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_redundant_leaf_context")
            && diag.message.contains("factory::EhrClientFactory")
            && diag.message.contains("factory::EhrClient")
    }));
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_adapter_redundant_leaf_context")
            && diag.message.contains("mock::MockEhrClient")
            && diag.message.contains("mock::EhrClient")
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
fn analyze_workspace_allows_longer_internal_reexport_when_shorter_surface_is_also_exposed() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/replay")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod replay;
"#,
    )
    .expect("write lib");
    fs::write(
        root.join("src/replay.rs"),
        r#"
pub(crate) mod machine;

pub use machine::rebuild;
pub(crate) use machine::rebuild_machine;
"#,
    )
    .expect("write replay");
    fs::write(
        root.join("src/replay/machine.rs"),
        r#"
pub fn rebuild() {}
pub(crate) fn rebuild_machine() {}
"#,
    )
    .expect("write machine");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_redundant_leaf_context")
            && diag.message.contains("rebuild_machine")
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
fn analyze_workspace_flags_candidate_semantic_module_for_partial_existing_head_namespace() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod user;

pub struct UserRepository;
pub struct UserService;
pub struct UserId;
"#,
    )
    .expect("write lib");
    fs::write(
        root.join("src/user.rs"),
        r#"
pub struct Profile;
pub struct Repository;
"#,
    )
    .expect("write user");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module")
            && !diag.is_policy_violation()
            && diag.message.contains("UserRepository")
            && diag.message.contains("UserService")
            && diag.message.contains("UserId")
            && diag
                .message
                .contains("user::{Id, Profile, Repository, Service}")
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
fn analyze_workspace_flags_candidate_semantic_module_for_public_shared_head_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod audit;
pub mod audit_path;
pub mod audit_semantics;
"#,
    )
    .expect("write lib");
    fs::write(root.join("src/audit.rs"), "pub struct Record;\n").expect("write audit");
    fs::write(root.join("src/audit_path.rs"), "pub struct Boundary;\n").expect("write audit path");
    fs::write(
        root.join("src/audit_semantics.rs"),
        "pub struct Snapshot;\n",
    )
    .expect("write audit semantics");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module")
            && diag.message.contains("public sibling modules")
            && diag.message.contains("audit_path")
            && diag.message.contains("audit_semantics")
            && diag.message.contains("audit::{path, semantics}")
    }));
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("internal_candidate_semantic_module")
            && diag.message.contains("audit::{path, semantics}")
    }));
}

#[test]
fn analyze_workspace_flags_candidate_semantic_module_for_public_shared_tail_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod case_review;
pub mod field_review;
pub mod scope_review;
"#,
    )
    .expect("write lib");
    fs::write(root.join("src/case_review.rs"), "pub struct Case;\n").expect("write case review");
    fs::write(root.join("src/field_review.rs"), "pub struct Field;\n").expect("write field review");
    fs::write(root.join("src/scope_review.rs"), "pub struct Scope;\n").expect("write scope review");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module")
            && diag.message.contains("public sibling modules")
            && diag.message.contains("share the `Review` tail")
            && diag.message.contains("review::{case, field, scope}")
    }));
}

#[test]
fn analyze_workspace_prefers_dominant_compound_head_for_public_shared_head_family() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct SourceUpdateAdapter;
pub struct SourceUpdateApiSchema;
pub struct SourceUpdateAuth;
pub struct SourceAuthorizationHeader;
pub struct SourceUpdateConfig;
pub struct SourceEndpoint;
pub struct SourceUpdateMode;
pub struct SourceUpdateRequest;
pub struct SourceUpdateResponse;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module")
            && diag.message.contains("SourceUpdateAdapter")
            && diag.message.contains("SourceUpdateResponse")
            && diag.message.contains(
                "source_update::{Adapter, ApiSchema, Auth, Config, Mode, Request, Response}",
            )
            && !diag.message.contains("source::{AuthorizationHeader")
    }));
}

#[test]
fn analyze_workspace_prefers_nested_candidate_semantic_module_for_shared_head_and_tail_family() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct CaseInboxLaunch;
pub struct CaseDetailLaunch;
pub struct CaseAuditLaunch;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module")
            && diag
                .message
                .contains("share the `Case` head and `Launch` tail")
            && diag
                .message
                .contains("case::launch::{Audit, Detail, Inbox}")
            && !diag.message.contains("case::{AuditLaunch")
    }));
}

#[test]
fn analyze_workspace_keeps_bare_head_tail_member_alongside_nested_candidate_semantic_module() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct GovernanceLaunch;
pub struct GovernanceBoardLaunch;
pub struct GovernanceHistoryLaunch;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module")
            && diag
                .message
                .contains("share the `Governance` head and `Launch` tail")
            && diag
                .message
                .contains("governance::{Launch, launch::{Board, History}}")
            && !diag.message.contains("governance::{BoardLaunch")
    }));
}

#[test]
fn analyze_workspace_skips_candidate_semantic_module_for_compile_test_scaffolding() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod compile_test;\n").expect("write lib");
    fs::write(
        root.join("src/compile_test.rs"),
        r#"
pub struct ReleaseCandidate;
pub struct ReleaseApproved;
pub struct ReleaseReleased;
"#,
    )
    .expect("write compile test");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_semantic_module"))
    );
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
fn analyze_workspace_flags_candidate_semantic_module_for_high_signal_two_item_head_family() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub struct TransactionId;
pub enum TransactionFailure {
    Failed,
}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module")
            && diag.message.contains("TransactionId")
            && diag.message.contains("TransactionFailure")
            && diag.message.contains("transaction::{Failure, Id}")
    }));
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
fn analyze_workspace_does_not_nest_candidate_semantic_module_inside_parent_that_already_carries_the_head()
 {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    let scope_review_file = root.join("src/scope_review.rs");
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod scope_review;\n").expect("write lib");
    fs::write(
        &scope_review_file,
        r#"
pub type ScopeReviewResult = std::result::Result<ScopeDecision, Error>;

pub enum ScopeDecision {
    Approved,
    Rejected,
}

pub trait ScopeReview {}

pub struct Error;
"#,
    )
    .expect("write scope review");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module")
            && diag.file.as_deref() == Some(scope_review_file.as_path())
    }));
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
fn analyze_workspace_skips_candidate_semantic_module_for_cfg_guarded_high_signal_two_item_head_family()
 {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
#[cfg(any())]
pub struct TransactionId;
pub enum TransactionFailure {
    Failed,
}
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
fn analyze_workspace_skips_candidate_semantic_module_for_cfg_guarded_public_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod audit {}
#[cfg(any())]
pub mod audit_path {}
pub mod audit_semantics {}
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
fn analyze_workspace_does_not_flag_candidate_semantic_module_unsupported_construct_for_unrelated_cfg_guarded_public_modules()
 {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
#[cfg(any())]
pub mod user_profile {}
pub mod audit_log {}
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(!report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_semantic_module_unsupported_construct")
    }));
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
fn analyze_workspace_does_not_report_candidate_facet_module_for_decode_wrapper_error_family() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/user")).expect("create user module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod user;\n").expect("write lib");
    fs::write(
        root.join("src/user/mod.rs"),
        r#"
pub mod error;
pub use error::Error;

pub struct Username(String);
pub struct Email(String);
"#,
    )
    .expect("write user mod");
    fs::write(
        root.join("src/user/error.rs"),
        r#"
use crate::user::{EmailError, UsernameError};

pub enum Error {
    DecodeUsername { source: UsernameError },
    DecodeEmail { source: EmailError },
}
"#,
    )
    .expect("write user error");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_facet_module"))
    );
}

#[test]
fn analyze_workspace_does_not_report_candidate_facet_module_for_private_root_module() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/user")).expect("create user module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "mod user;\n").expect("write lib");
    fs::write(
        root.join("src/user/mod.rs"),
        r#"
pub mod error;
pub use error::Error;

pub struct Username(String);
pub struct Email(String);
"#,
    )
    .expect("write user mod");
    fs::write(
        root.join("src/user/error.rs"),
        r#"
use crate::user::{EmailError, UsernameError};

pub enum Error {
    Username { source: UsernameError },
    Email { source: EmailError },
}
"#,
    )
    .expect("write user error");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_facet_module"))
    );
}

#[test]
fn analyze_workspace_does_not_report_candidate_facet_module_for_scalar_wrapper_bundle() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/value")).expect("create value module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod value;\n").expect("write lib");
    fs::write(
        root.join("src/value/mod.rs"),
        r#"
pub mod error;
pub use error::Error;

pub struct ExternalId(String);
pub struct Label(String);
pub struct DetailText(String);
"#,
    )
    .expect("write value mod");
    fs::write(
        root.join("src/value/error.rs"),
        r#"
use crate::value::{DetailTextError, ExternalIdError, LabelError};

pub enum Error {
    ExternalId { source: ExternalIdError },
    Label { source: LabelError },
    DetailText { source: DetailTextError },
}
"#,
    )
    .expect("write value error");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_facet_module"))
    );
}

#[test]
fn analyze_workspace_does_not_report_candidate_facet_module_for_cfg_guarded_root_family_members() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/user")).expect("create user module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod user;\n").expect("write lib");
    fs::write(
        root.join("src/user/mod.rs"),
        r#"
pub mod error;
pub use error::Error;

#[cfg(any())]
pub struct Username(String);
pub struct Email(String);
"#,
    )
    .expect("write user mod");
    fs::write(
        root.join("src/user/error.rs"),
        r#"
use crate::user::{EmailError, UsernameError};

pub enum Error {
    Username { source: UsernameError },
    Email { source: EmailError },
}
"#,
    )
    .expect("write user error");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_facet_module"))
    );
}

#[test]
fn analyze_workspace_reports_candidate_child_facet_module_for_broader_message_owner() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/chat")).expect("create chat module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod chat;\n").expect("write lib");
    fs::write(root.join("src/chat/mod.rs"), "pub mod message;\n").expect("write chat mod");
    fs::write(
        root.join("src/chat/message.rs"),
        r#"
#[nutype(
    sanitize(trim),
    validate(not_empty, len_char_max = 1000),
    derive(Debug, Clone, PartialEq, Eq, AsRef)
)]
pub struct Body(String);

pub struct Message {
    pub body: Body,
}
"#,
    )
    .expect("write message module");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_child_facet_module")
            && diag.message.contains("chat::message")
            && diag.message.contains("chat::message::Body")
            && diag.message.contains("chat::message::body")
            && diag.message.contains("`Message`")
    }));
}

#[test]
fn analyze_workspace_reports_candidate_child_facet_module_for_trait_and_struct_owner() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/chat")).expect("create chat module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod chat;\n").expect("write lib");
    fs::write(root.join("src/chat/mod.rs"), "pub mod moderation;\n").expect("write chat mod");
    fs::write(
        root.join("src/chat/moderation.rs"),
        r#"
#[nutype(validate(len_char_max = 200))]
pub struct Reason(String);

pub struct Item {
    pub reason: Reason,
}

pub trait Queue {
    fn enqueue(&self, reason: &Reason);
}
"#,
    )
    .expect("write moderation module");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_candidate_child_facet_module")
            && diag.message.contains("chat::moderation")
            && diag.message.contains("chat::moderation::Reason")
            && diag.message.contains("chat::moderation::reason")
            && diag.message.contains("`Item`")
            && diag.message.contains("`Queue`")
    }));
}

#[test]
fn analyze_workspace_does_not_report_candidate_child_facet_module_for_leaf_only_owner() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/chat")).expect("create chat module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod chat;\n").expect("write lib");
    fs::write(root.join("src/chat/mod.rs"), "pub mod client;\n").expect("write chat mod");
    fs::write(
        root.join("src/chat/client.rs"),
        r#"
#[nutype(validate(not_empty))]
pub struct Id(String);
"#,
    )
    .expect("write client module");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_child_facet_module"))
    );
}

#[test]
fn analyze_workspace_does_not_report_candidate_child_facet_module_for_helper_function_only() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/chat")).expect("create chat module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod chat;\n").expect("write lib");
    fs::write(root.join("src/chat/mod.rs"), "pub mod message;\n").expect("write chat mod");
    fs::write(
        root.join("src/chat/message.rs"),
        r#"
#[nutype(validate(not_empty))]
pub struct Body(String);

pub fn parse(body: Body) -> Body {
    body
}
"#,
    )
    .expect("write message module");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_child_facet_module"))
    );
}

#[test]
fn analyze_workspace_does_not_report_candidate_child_facet_module_for_multiple_validated_leaves() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/chat")).expect("create chat module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod chat;\n").expect("write lib");
    fs::write(root.join("src/chat/mod.rs"), "pub mod message;\n").expect("write chat mod");
    fs::write(
        root.join("src/chat/message.rs"),
        r#"
#[nutype(validate(not_empty))]
pub struct Body(String);

#[nutype(validate(not_empty))]
pub struct Summary(String);

pub struct Message {
    pub body: Body,
    pub summary: Summary,
}
"#,
    )
    .expect("write message module");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_child_facet_module"))
    );
}

#[test]
fn analyze_workspace_does_not_report_candidate_child_facet_module_when_owner_already_has_error() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/chat/room")).expect("create room module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod chat;\n").expect("write lib");
    fs::write(root.join("src/chat/mod.rs"), "pub mod room;\n").expect("write chat mod");
    fs::write(root.join("src/chat/room.rs"), "pub mod name;\n").expect("write room mod");
    fs::write(
        root.join("src/chat/room/name.rs"),
        r#"
#[nutype(validate(not_empty))]
pub struct Text(String);

pub struct Name {
    pub text: Text,
}

pub enum Error {
    Invalid { source: TextError },
}
"#,
    )
    .expect("write name module");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_candidate_child_facet_module"))
    );
}

#[test]
fn analyze_workspace_reports_boundary_wraps_child_facet_error_for_public_boundary_error() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/chat")).expect("create chat module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod chat;\n").expect("write lib");
    fs::write(
        root.join("src/chat/mod.rs"),
        "pub mod error;\npub mod message;\npub use error::Error;\n",
    )
    .expect("write chat mod");
    fs::write(
        root.join("src/chat/message.rs"),
        r#"
#[nutype(validate(not_empty))]
pub struct Body(String);

pub struct Message {
    pub body: Body,
}
"#,
    )
    .expect("write message module");
    fs::write(
        root.join("src/chat/error.rs"),
        r#"
use crate::chat::message;

pub enum Error {
    MessageBody { source: message::BodyError },
}
"#,
    )
    .expect("write error module");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_boundary_wraps_child_facet_error")
            && diag.message.contains("chat::Error")
            && diag.message.contains("MessageBody")
            && diag.message.contains("chat::message::BodyError")
            && diag.message.contains("chat::message::body::Error")
    }));
}

#[test]
fn analyze_workspace_reports_boundary_wraps_child_facet_error_for_imported_flat_error_leaf() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/chat")).expect("create chat module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod chat;\n").expect("write lib");
    fs::write(
        root.join("src/chat/mod.rs"),
        "pub mod error;\npub mod message;\npub use error::Error;\n",
    )
    .expect("write chat mod");
    fs::write(
        root.join("src/chat/message.rs"),
        r#"
#[nutype(validate(not_empty))]
pub struct Body(String);

pub struct Message {
    pub body: Body,
}
"#,
    )
    .expect("write message module");
    fs::write(
        root.join("src/chat/error.rs"),
        r#"
use crate::chat::message::BodyError;

pub enum Error {
    MessageBody { source: BodyError },
}
"#,
    )
    .expect("write error module");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_boundary_wraps_child_facet_error")
            && diag.message.contains("chat::Error")
            && diag.message.contains("MessageBody")
            && diag.message.contains("chat::message::BodyError")
            && diag.message.contains("chat::message::body::Error")
    }));
}

#[test]
fn analyze_workspace_reports_boundary_wraps_child_facet_error_for_aliased_imported_flat_error_leaf()
{
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/chat")).expect("create chat module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod chat;\n").expect("write lib");
    fs::write(
        root.join("src/chat/mod.rs"),
        "pub mod error;\npub mod message;\npub use error::Error;\n",
    )
    .expect("write chat mod");
    fs::write(
        root.join("src/chat/message.rs"),
        r#"
#[nutype(validate(not_empty))]
pub struct Body(String);

pub struct Message {
    pub body: Body,
}
"#,
    )
    .expect("write message module");
    fs::write(
        root.join("src/chat/error.rs"),
        r#"
use crate::chat::message::BodyError as ChatBodyError;

pub enum Error {
    MessageBody { source: ChatBodyError },
}
"#,
    )
    .expect("write error module");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_boundary_wraps_child_facet_error")
            && diag.message.contains("chat::Error")
            && diag.message.contains("MessageBody")
            && diag.message.contains("chat::message::BodyError")
            && diag.message.contains("chat::message::body::Error")
    }));
}

#[test]
fn analyze_workspace_reports_boundary_wraps_child_facet_error_for_super_child_path() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/chat")).expect("create chat module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod chat;\n").expect("write lib");
    fs::write(
        root.join("src/chat/mod.rs"),
        "pub mod error;\npub mod moderation;\npub use error::Error;\n",
    )
    .expect("write chat mod");
    fs::write(
        root.join("src/chat/moderation.rs"),
        r#"
#[nutype(validate(not_empty))]
pub struct Reason(String);

pub struct Item {
    pub reason: Reason,
}
"#,
    )
    .expect("write moderation module");
    fs::write(
        root.join("src/chat/error.rs"),
        r#"
pub enum Error {
    InvalidModerationReason {
        source: super::moderation::ReasonError,
    },
}
"#,
    )
    .expect("write error module");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_boundary_wraps_child_facet_error")
            && diag.message.contains("chat::Error")
            && diag.message.contains("InvalidModerationReason")
            && diag.message.contains("chat::moderation::ReasonError")
            && diag.message.contains("chat::moderation::reason::Error")
    }));
}

#[test]
fn analyze_workspace_does_not_report_boundary_wraps_child_facet_error_for_private_decode_enum() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/chat")).expect("create chat module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod chat;\n").expect("write lib");
    fs::write(
        root.join("src/chat/mod.rs"),
        "pub mod error;\npub mod message;\npub use error::Error;\n",
    )
    .expect("write chat mod");
    fs::write(
        root.join("src/chat/message.rs"),
        r#"
#[nutype(validate(not_empty))]
pub struct Body(String);

pub struct Message {
    pub body: Body,
}
"#,
    )
    .expect("write message module");
    fs::write(
        root.join("src/chat/error.rs"),
        r#"
enum Decode {
    MessageBody { source: super::message::BodyError },
}
"#,
    )
    .expect("write error module");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_boundary_wraps_child_facet_error"))
    );
}

#[test]
fn analyze_workspace_reports_owned_facet_companion_error_for_nested_leaf_owner() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/chat/room")).expect("create room module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod chat;\n").expect("write lib");
    fs::write(root.join("src/chat/mod.rs"), "pub mod room;\n").expect("write chat mod");
    fs::write(root.join("src/chat/room.rs"), "pub mod name;\n").expect("write room mod");
    fs::write(
        root.join("src/chat/room/name.rs"),
        r#"
pub struct Text(String);

pub enum Error {
    Invalid { source: TextError },
}
"#,
    )
    .expect("write name module");

    let report = analyze_workspace(root, &[]);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.code() == Some("api_owned_facet_companion_error")
            && diag.message.contains("chat::room::name")
            && diag.message.contains("TextError")
            && diag.message.contains("chat::room::name::Error")
    }));
}

#[test]
fn analyze_workspace_does_not_report_owned_facet_companion_error_for_root_module_surface() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod value;\n").expect("write lib");
    fs::write(
        root.join("src/value.rs"),
        r#"
pub struct Text(String);

pub enum Error {
    Invalid { source: TextError },
}
"#,
    )
    .expect("write value module");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_owned_facet_companion_error"))
    );
}

#[test]
fn analyze_workspace_does_not_report_owned_facet_companion_error_for_imported_companion_name() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src/chat/room")).expect("create room module");
    write_manifest(root, "");
    fs::write(root.join("src/lib.rs"), "pub mod chat;\npub mod support;\n").expect("write lib");
    fs::write(root.join("src/chat/mod.rs"), "pub mod room;\n").expect("write chat mod");
    fs::write(root.join("src/chat/room.rs"), "pub mod config;\n").expect("write room mod");
    fs::write(
        root.join("src/chat/room/config.rs"),
        r#"
use crate::support::ConfigError;

pub struct Config(String);

pub enum Error {
    Invalid { source: ConfigError },
}
"#,
    )
    .expect("write config module");
    fs::write(
        root.join("src/support.rs"),
        r#"
pub struct ConfigError;
"#,
    )
    .expect("write support module");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.code() == Some("api_owned_facet_companion_error"))
    );
}

#[test]
fn analyze_workspace_skips_internal_candidate_semantic_module_for_macro_generated_internal_items() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
macro_rules! declare_case_family {
    () => {
        struct CaseAuditArgs;
    };
}

declare_case_family!();

struct CaseDetailArgs;
struct CaseHistoryArgs;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("internal_candidate_semantic_module") })
    );
}

#[test]
fn analyze_workspace_skips_internal_candidate_semantic_module_for_cfg_guarded_internal_modules() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    write_manifest(root, "");
    fs::write(
        root.join("src/lib.rs"),
        r#"
#[cfg(any())]
mod epic_fhir;
mod epic_write_back;
mod alpha_vendor_http;
"#,
    )
    .expect("write lib");

    let report = analyze_workspace(root, &[]);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| { diag.code() == Some("internal_candidate_semantic_module") })
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
fn analyze_workspace_reports_surface_and_strict_boundary_lints_by_default() {
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
        report
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
