use std::{env, path::PathBuf, process::ExitCode};

use modum::{CheckMode, parse_check_mode, render_pretty_report, run_check};

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        None | Some("--help") | Some("-h") => {
            println!("{}", top_level_usage());
            Ok(ExitCode::from(0))
        }
        Some("check") => run_check_command(args),
        Some(other) => Err(format!("unknown command: {other}\n\n{}", top_level_usage())),
    }
}

fn run_check_command(mut args: impl Iterator<Item = String>) -> Result<ExitCode, String> {
    let mut root = env::current_dir().map_err(|err| format!("failed to get current dir: {err}"))?;
    let mut include_globs = Vec::new();
    let mut mode = env::var("MODUM")
        .ok()
        .and_then(|raw| parse_check_mode(&raw).ok())
        .unwrap_or(CheckMode::Deny);
    let mut format = OutputFormat::Text;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--root requires a path value".to_string())?;
                root = PathBuf::from(value);
            }
            "--include" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--include requires a path value".to_string())?;
                include_globs.push(value);
            }
            "--mode" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--mode requires one of: off|warn|deny".to_string())?;
                mode = parse_check_mode(&value)?;
            }
            "--format" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--format requires one of: text|json".to_string())?;
                format = parse_output_format(&value)?;
            }
            "--help" | "-h" => {
                println!("{}", check_usage());
                return Ok(ExitCode::from(0));
            }
            other => {
                return Err(format!("unknown argument: {other}\n\n{}", check_usage()));
            }
        }
    }

    if mode == CheckMode::Off {
        println!("modum check skipped (mode=off)");
        return Ok(ExitCode::from(0));
    }

    let outcome = run_check(&root, &include_globs, mode);
    match format {
        OutputFormat::Text => print!("{}", render_pretty_report(&outcome.report)),
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&outcome)
                    .map_err(|err| format!("failed to render json: {err}"))?
            );
        }
    }
    Ok(ExitCode::from(outcome.exit_code))
}

fn parse_output_format(raw: &str) -> Result<OutputFormat, String> {
    match raw {
        "text" => Ok(OutputFormat::Text),
        "json" => Ok(OutputFormat::Json),
        _ => Err(format!("invalid format `{raw}`; expected text|json")),
    }
}

fn top_level_usage() -> String {
    [
        "Usage:",
        "  modum check [options]",
        "",
        "Commands:",
        "  check    Analyze a crate or workspace and report naming-policy violations",
        "",
        "Config:",
        "  Cargo metadata: [workspace.metadata.modum] or [package.metadata.modum]",
    ]
    .join("\n")
}

fn check_usage() -> String {
    [
        "Usage:",
        "  modum check [--root <path>] [--include <path>]... [--mode off|warn|deny] [--format text|json]",
        "",
        "Examples:",
        "  modum check",
        "  modum check --mode warn",
        "  modum check --format json",
        "",
        "Environment:",
        "  MODUM=off|warn|deny (default: deny)",
    ]
    .join("\n")
}
