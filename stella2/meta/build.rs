fn main() {
    tcw3_designer::BuildScriptConfig::new()
        .link("tcw3", tcw3_meta::DESIGNER_METADATA.into())
        .run_and_exit_on_error();
}
