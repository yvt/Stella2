macro_rules! should_error {
    ($name:ident, $path:literal) => {
        #[test]
        fn $name() {
            let _ = env_logger::try_init();
            let mut out_stream = Vec::new();
            let e = tcw3_designer::BuildScriptConfig::new()
                .root_source_file(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/bad/", $path))
                .out_source_stream(&mut out_stream)
                .crate_name("designer_test")
                .run();
            if !out_stream.is_empty() {
                println!("output:");
                println!("```rust");
                println!("{}", std::str::from_utf8(&out_stream).unwrap());
                println!("```");
            }
            assert!(e.is_err(), "codegen did not fail");
        }
    };
}

should_error!(const_uninitable, "const_uninitable.tcwdl");
should_error!(objinit_comp_unknown, "objinit_comp_unknown.tcwdl");
should_error!(objinit_explicit_type, "objinit_explicit_type.tcwdl");
should_error!(objinit_settable, "objinit_settable.tcwdl");
should_error!(objinit_field_dupe, "objinit_field_dupe.tcwdl");
should_error!(objinit_field_unknown, "objinit_field_unknown.tcwdl");
should_error!(objinit_field_wrong_field_ty, "objinit_field_wrong_field_ty.tcwdl");
should_error!(objinit_field_wrong_ty, "objinit_field_wrong_ty.tcwdl");
should_error!(prop_uninitable, "prop_uninitable.tcwdl");
should_error!(use_super, "use_super.tcwdl");
