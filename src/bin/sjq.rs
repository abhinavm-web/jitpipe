#![deny(clippy::all)]

use clap::Parser;
use std::process::ExitCode;

use jitpipe::cli::SjqArgs;
use jitpipe::run_sjq;

fn main() -> ExitCode {
    let args = SjqArgs::parse();
    match run_sjq(args) {
        Ok(code) => {
            if code == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(code as u8)
            }
        }
        Err(err) => {
            eprintln!("sjq: {}", err);
            ExitCode::from(2)
        }
    }
}
