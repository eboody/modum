use std::process::ExitCode;

#[path = "../cli.rs"]
mod cli;

fn main() -> ExitCode {
    cli::run_main("cargo modum", true)
}
