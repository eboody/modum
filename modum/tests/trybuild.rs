#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass_struct_pascal.rs");
    t.pass("tests/ui/pass_enum_acronym.rs");
    t.pass("tests/ui/pass_trait_type_union.rs");
    t.pass("tests/ui/pass_fn_camel.rs");
    t.pass("tests/ui/pass_fn_snake.rs");
    t.pass("tests/ui/pass_const_static.rs");
    t.pass("tests/ui/pass_nested_module.rs");
    t.pass("tests/ui/pass_keywords_raw_idents.rs");
    t.pass("tests/ui/pass_visibility_generics.rs");

    t.compile_fail("tests/ui/fail_single_segment_pascal.rs");
    t.compile_fail("tests/ui/fail_single_segment_snake.rs");
    t.compile_fail("tests/ui/fail_unsupported_impl.rs");
    t.compile_fail("tests/ui/fail_unsupported_mod.rs");
}
