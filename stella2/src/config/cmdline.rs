//! Command-line argument parsing
use std::{
    env::{args_os, ArgsOs},
    ffi::OsString,
    path::PathBuf,
};

/// A lightweight instant messaging client.
#[derive(Default)]
pub struct Args {
    /// the path to a custom profile directory
    pub profile: Option<PathBuf>,
}

impl Args {
    pub fn from_env_or_exit() -> Self {
        let mut this = Self::default();

        let mut args = args_os();
        if args.next().is_none() {
            return this;
        }

        while let Some(hdr_os) = args.next() {
            // The representation of an `OsStr` is opaque, so we can't search for hyphens without
            // converting it to `str`. However, it implements `PartialEq<str>`, so we can check for
            // an exact match without doing the conversion.
            let handler_info = HANDLER_TABLE.iter().find(|p| hdr_os == p.0);

            if let Some((hdr, handler)) = handler_info {
                handler.handle(&mut this, hdr, &mut args);
            } else {
                if let Some(hdr) = hdr_os.to_str() {
                    eprintln!("error: Found an unexpected argument '{}'", hdr);
                } else {
                    eprintln!("error: Found an unexpected argument ");
                }
                std::process::exit(1);
            }
        }

        this
    }
}

static HANDLER_TABLE: &[(&str, &(dyn ArgHandler<Args> + Send + Sync))] = &[
    ("-h", &(handle_help as fn(&mut Args))),
    ("--help", &(handle_help as fn(&mut Args))),
    ("--profile", &(handle_profile as fn(&mut Args, OsString))),
];

fn display_help_and_exit() -> ! {
    // TODO: Display the message in a window when running on Windows
    println!(
        "Stella 2
A lightweight instant messaging client.

USAGE:
    stella2 [OPTIONS]

FLAGS:
    -h, --help       display help information

OPTIONS:
    --profile <PROFILE>    the path to a custom profile directory"
    );
    std::process::exit(0);
}

trait ArgHandler<Ctx> {
    fn handle(&self, ctx: &mut Ctx, arg_hdr: &str, args_iter: &mut ArgsOs);
}

impl<Ctx> ArgHandler<Ctx> for fn(&mut Ctx) {
    fn handle(&self, ctx: &mut Ctx, _arg_hdr: &str, _args_iter: &mut ArgsOs) {
        self(ctx);
    }
}

impl<Ctx> ArgHandler<Ctx> for fn(&mut Ctx, OsString) {
    fn handle(&self, ctx: &mut Ctx, arg_hdr: &str, args_iter: &mut ArgsOs) {
        if let Some(value) = args_iter.next() {
            self(ctx, value);
        } else {
            eprintln!("error: The argument '{}' requires a value", arg_hdr);
            std::process::exit(1);
        }
    }
}

fn handle_help<T>(_: &mut T) {
    display_help_and_exit();
}

fn handle_profile(args: &mut Args, value: OsString) {
    args.profile = Some(value.into());
}
