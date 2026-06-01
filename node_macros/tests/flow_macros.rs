#[test]
fn flow_macros_compile_and_fail_as_expected() {
    let t = trybuild::TestCases::new();
    t.pass("tests/flow_macros/pass/*.rs");
    t.compile_fail("tests/flow_macros/fail/*.rs");
}
