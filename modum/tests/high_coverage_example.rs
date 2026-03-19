use std::{collections::BTreeSet, path::PathBuf};

use modum::analyze_workspace;

#[test]
fn high_coverage_example_emits_expected_lints() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/high-coverage");
    let report = analyze_workspace(&root, &[]);

    assert_eq!(report.error_count(), 1);

    let codes = report
        .diagnostics
        .iter()
        .filter_map(|diag| diag.code().map(str::to_owned))
        .collect::<BTreeSet<_>>();

    assert_eq!(
        codes,
        BTreeSet::from([
            "api_catch_all_module".to_string(),
            "api_missing_parent_surface_export".to_string(),
            "api_organizational_submodule_flatten".to_string(),
            "api_redundant_category_suffix".to_string(),
            "api_redundant_leaf_context".to_string(),
            "api_repeated_module_segment".to_string(),
            "api_weak_module_generic_leaf".to_string(),
            "namespace_flat_pub_use".to_string(),
            "namespace_flat_use".to_string(),
            "namespace_flat_use_preserve_module".to_string(),
            "namespace_flat_use_redundant_leaf_context".to_string(),
        ])
    );
}
