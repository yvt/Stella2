use regex::Regex;

macro_rules! should_error {
    ($name:ident, $path:literal) => {
        #[test]
        fn $name() {
            run_should_error(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/bad/", $path));
        }
    };
}

fn run_should_error(source_path: &str) {
    let _ = env_logger::try_init();
    let mut out_diag = Vec::<u8>::new();
    let mut out_stream = Vec::new();
    let e = tcw3_designer::BuildScriptConfig::new()
        .root_source_file(source_path)
        .out_source_stream(&mut out_stream)
        .out_diag_stream(&mut out_diag)
        .crate_name("designer_test")
        .run();
    let out_diag = std::str::from_utf8(&out_diag).unwrap();
    eprintln!("{}", out_diag);
    if !out_stream.is_empty() {
        println!("output:");
        println!("```rust");
        println!("{}", std::str::from_utf8(&out_stream).unwrap());
        println!("```");
    }
    assert!(e.is_err(), "codegen did not fail");

    // Extract error messages
    lazy_static::lazy_static! {
        static ref RE: Regex = Regex::new(r#"(?m)error: (.+?)\s*--> .*:(\d+):\d+$"#)
            .unwrap();
    }

    let errors: Vec<(&str, usize)> = RE
        .captures_iter(out_diag)
        .map(|caps| (caps.get(1).unwrap().as_str(), caps[2].parse().unwrap()))
        .collect();

    // Look for annotations
    let source = std::fs::read_to_string(source_path).unwrap();
    let mut has_annotation = false;

    for (line_i, line) in (1..).zip(source.lines()) {
        let line = line.trim();
        if line.starts_with("//~^ ERROR") {
            has_annotation = true;

            let target_line_i = line_i - 1;
            let needle = &line[11..];

            let found_matching_error = errors
                .iter()
                .any(|(msg, line_i)| *line_i == target_line_i && msg.contains(needle));

            if !found_matching_error {
                panic!("missing: '{}'", needle);
            }
        }
    }

    assert!(has_annotation, "error annotation not found");
}

// TODO: `comp_path_external`
should_error!(comp_path_unknown, "comp_path_unknown.tcwdl");
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
should_error!(input_inline_unsyntactic, "input_inline_unsyntactic.tcwdl");
should_error!(objinit_comp_unknown, "objinit_comp_unknown.tcwdl");
should_error!(objinit_explicit_type, "objinit_explicit_type.tcwdl");
should_error!(objinit_settable, "objinit_settable.tcwdl");
should_error!(objinit_subexpr, "objinit_subexpr.tcwdl");
should_error!(objinit_field_dupe, "objinit_field_dupe.tcwdl");
should_error!(
    objinit_field_short_badinput,
    "objinit_field_short_badinput.tcwdl"
);
should_error!(objinit_field_unknown, "objinit_field_unknown.tcwdl");
should_error!(objinit_field_wrong_ty, "objinit_field_wrong_ty.tcwdl");
should_error!(prop_uninitable, "prop_uninitable.tcwdl");
should_error!(prop_unsettable, "prop_unsettable.tcwdl");
should_error!(use_dupe, "use_dupe.tcwdl");
should_error!(use_self, "use_self.tcwdl");
should_error!(use_super, "use_super.tcwdl");
should_error!(use_unknown, "use_unknown.tcwdl");
should_error!(watch_nonnullary, "watch_nonnullary.tcwdl");
