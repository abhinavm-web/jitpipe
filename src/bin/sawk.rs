#![deny(clippy::all)]

use clap::Parser;
use std::process::ExitCode;

use jitpipe::cli::SawkArgs;
use jitpipe::run_sawk;

fn main() -> ExitCode {
    let args = SawkArgs::parse();
    match run_sawk(args) {
        Ok(code) => {
            if code == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(code as u8)
            }
        }
        Err(err) => {
            eprintln!("sawk: {}", err);
            ExitCode::from(2)
        }
    }
}
