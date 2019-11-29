use std::env;

use tcw3_designer::BuildScriptConfig;

fn main() {
    let args: Vec<_> = env::args_os().collect();

    if args.len() != 3 {
        eprintln!("Usage: tcw3codegen INPUT.tcwdl OUTPUT.rs");
        std::process::exit(1);
    }

    BuildScriptConfig::new()
        .root_source_file(&args[1])
        .out_source_file(&args[2])
        .run_and_exit_on_error();
}
