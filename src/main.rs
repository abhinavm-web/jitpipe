#![deny(clippy::all)]

use clap::Parser;
use std::process::ExitCode;

use jitpipe::cli::JitPipeArgs;
use jitpipe::run_jitpipe;

fn main() -> ExitCode {
    let args = JitPipeArgs::parse();
    match run_jitpipe(args) {
        Ok(code) => {
            if code == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(code as u8)
            }
        }
        Err(err) => {
            eprintln!("jitpipe: {}", err);
            ExitCode::from(2)
        }
    }
}
