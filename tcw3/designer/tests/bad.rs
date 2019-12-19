macro_rules! should_error {
    ($name:ident, $path:literal) => {
        #[test]
        fn $name() {
            let _ = env_logger::try_init();
            let mut out_diag = Vec::<u8>::new();
            let mut out_stream = Vec::new();
            let e = tcw3_designer::BuildScriptConfig::new()
                .root_source_file(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/bad/", $path))
                .out_source_stream(&mut out_stream)
                .out_diag_stream(&mut out_diag)
                .crate_name("designer_test")
                .run();
            eprintln!("{}", std::str::from_utf8(&out_diag).unwrap());
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

should_error!(comp_path_external, "comp_path_external.tcwdl");
should_error!(comp_path_super, "comp_path_super.tcwdl");
should_error!(const_uninitable, "const_uninitable.tcwdl");
should_error!(const_watch, "const_watch.tcwdl");
should_error!(input_circular, "input_circular.tcwdl");
should_error!(input_circular2, "input_circular2.tcwdl");
should_error!(input_circular_objinit, "input_circular_objinit.tcwdl");
should_error!(input_circular_this, "input_circular_this.tcwdl");
should_error!(input_field_not_comp, "input_field_not_comp.tcwdl");
should_error!(input_field_not_comp2, "input_field_not_comp2.tcwdl");
should_error!(input_field_not_comp3, "input_field_not_comp3.tcwdl");
should_error!(input_field_unknown, "input_field_unknown.tcwdl");
should_error!(objinit_comp_unknown, "objinit_comp_unknown.tcwdl");
should_error!(objinit_explicit_type, "objinit_explicit_type.tcwdl");
should_error!(objinit_settable, "objinit_settable.tcwdl");
should_error!(objinit_field_dupe, "objinit_field_dupe.tcwdl");
should_error!(objinit_field_unknown, "objinit_field_unknown.tcwdl");
should_error!(
    objinit_field_wrong_field_ty,
    "objinit_field_wrong_field_ty.tcwdl"
);
should_error!(objinit_field_wrong_ty, "objinit_field_wrong_ty.tcwdl");
should_error!(prop_uninitable, "prop_uninitable.tcwdl");
should_error!(use_dupe, "use_dupe.tcwdl");
should_error!(use_self, "use_self.tcwdl");
should_error!(use_super, "use_super.tcwdl");
should_error!(use_unknown, "use_unknown.tcwdl");
