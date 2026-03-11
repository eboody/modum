use std::fs;

use modum::{CheckMode, run_check};
use tempfile::tempdir;

#[test]
fn run_check_off_skips_analysis() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub mod storage;
"#,
    )
    .expect("write source");
    fs::write(root.join("src/storage.rs"), "pub struct Repository;\n").expect("write module");

    let outcome = run_check(root, &[], CheckMode::Off);
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(outcome.report.scanned_files, 0);
    assert_eq!(outcome.report.files_with_violations, 0);
    assert!(outcome.report.diagnostics.is_empty());
}

#[test]
fn run_check_warn_allows_policy_violations_but_deny_blocks() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    fs::write(root.join("Cargo.toml"), "[package]\nname=\"fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n")
        .expect("write manifest");
    fs::write(root.join("src/lib.rs"), "pub mod storage;\n").expect("write lib");
    fs::write(root.join("src/storage.rs"), "pub struct Repository;\n").expect("write module");

    let warn = run_check(root, &[], CheckMode::Warn);
    assert_eq!(warn.exit_code, 0);
    assert_eq!(warn.report.scanned_files, 2);
    assert_eq!(warn.report.files_with_violations, 1);
    assert_eq!(warn.report.error_count(), 0);
    assert_eq!(warn.report.policy_violation_count(), 1);

    let deny = run_check(root, &[], CheckMode::Deny);
    assert_eq!(deny.exit_code, 2);
    assert_eq!(deny.report.policy_violation_count(), 1);
    assert_eq!(deny.report.error_count(), 0);
}

#[test]
fn run_check_errors_override_warn_and_deny() {
    let temp = tempdir().expect("create temp dir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("create src");
    fs::write(root.join("Cargo.toml"), "[package]\nname=\"fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n")
        .expect("write manifest");
    fs::write(root.join("src/lib.rs"), "pub struct Broken {\n").expect("write invalid file");

    let warn = run_check(root, &[], CheckMode::Warn);
    assert_eq!(warn.exit_code, 1);
    assert_eq!(warn.report.error_count(), 1);

    let deny = run_check(root, &[], CheckMode::Deny);
    assert_eq!(deny.exit_code, 1);
    assert_eq!(deny.report.error_count(), 1);
}
