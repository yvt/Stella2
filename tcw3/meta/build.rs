fn main() {
    tcw3_designer::BuildScriptConfig::new()
        .tcw3_path("crate")
        .designer_runtime_path("crate::designer_runtime")
        .run_and_exit_on_error();
}
