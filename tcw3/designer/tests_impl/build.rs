fn main() {
    tcw3_designer::BuildScriptConfig::new()
        .crate_name("tcw3_designer_tests_impl")
        .run_and_exit_on_error();
}
