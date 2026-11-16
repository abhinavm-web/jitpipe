#![deny(clippy::all)]

use clap::Parser;
use std::process::ExitCode;

use jitpipe::cli::SgrepArgs;
use jitpipe::run_sgrep;

fn main() -> ExitCode {
    let args = SgrepArgs::parse();
    match run_sgrep(args) {
        Ok(code) => {
            if code == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(code as u8)
            }
        }
        Err(err) => {
            eprintln!("sgrep: {}", err);
            ExitCode::from(2)
        }
    }
}
