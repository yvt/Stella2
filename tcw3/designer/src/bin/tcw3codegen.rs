use std::env;

use tcw3_designer::BuildScriptConfig;

fn main() {
    let args: Vec<_> = env::args_os().collect();

    if args.len() != 4 {
        eprintln!("Usage: tcw3codegen INPUT.tcwdl CRATENAME OUTPUT.rs");
        std::process::exit(1);
    }

    BuildScriptConfig::new()
        .root_source_file(&args[1])
        .crate_name(args[2].to_str().expect("Crate name contains invalid UTF-8"))
        .out_source_file(&args[3])
        .run_and_exit_on_error();
}
