mod cli;
mod cmd;
mod exit;
mod filter;
mod paths;
mod repo;
mod selfmanage;
mod vault;

use std::env;
use std::ffi::OsString;
use std::process::ExitCode;

use crate::exit::Code;

fn is_fsmonitor(args: &[OsString]) -> bool {
    args.get(2).and_then(|arg| arg.to_str()) == Some("fsmonitor")
}

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().collect();

    let outcome = match args.get(1).and_then(|arg| arg.to_str()) {
        Some("filter") => filter::dispatch(args.get(2..).unwrap_or_default()),
        Some("hook") if is_fsmonitor(&args) => filter::fsmonitor(),
        _ => cli::run(),
    };

    match outcome {
        Ok(code) => ExitCode::from(code.as_u8()),
        Err(error) => {
            eprintln!("git-vault: {error:#}");
            ExitCode::from(Code::Error.as_u8())
        }
    }
}
