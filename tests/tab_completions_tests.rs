mod common;

#[test]
fn tab_completion_tests() {
    common::run_bake_target("tab-completion-tests").expect("tab completion tests failed");
}
