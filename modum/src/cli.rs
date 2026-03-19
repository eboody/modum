use std::{env, path::PathBuf, process::ExitCode};

use modum::{
    CheckMode, DiagnosticSelection, ScanSettings, parse_check_mode,
    render_pretty_report_with_selection, run_check_with_scan_settings,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputSelection {
    All,
    Policy,
    Advisory,
}

pub fn run_main(command_prefix: &'static str, strip_subcommand_name: bool) -> ExitCode {
    match run(command_prefix, strip_subcommand_name) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

fn run(command_prefix: &'static str, strip_subcommand_name: bool) -> Result<ExitCode, String> {
    let args = normalize_args(env::args().skip(1).collect(), strip_subcommand_name);
    let mut args = args.into_iter();

    match args.next().as_deref() {
        None | Some("--help") | Some("-h") => {
            println!("{}", top_level_usage(command_prefix));
            Ok(ExitCode::from(0))
        }
        Some("check") => run_check_command(args, command_prefix),
        Some(other) => Err(format!(
            "unknown command: {other}\n\n{}",
            top_level_usage(command_prefix)
        )),
    }
}

fn normalize_args(mut args: Vec<String>, strip_subcommand_name: bool) -> Vec<String> {
    if strip_subcommand_name && matches!(args.first().map(String::as_str), Some("modum")) {
        args.remove(0);
    }
    args
}

fn run_check_command(
    mut args: impl Iterator<Item = String>,
    command_prefix: &'static str,
) -> Result<ExitCode, String> {
    let mut root = env::current_dir().map_err(|err| format!("failed to get current dir: {err}"))?;
    let mut scan_settings = ScanSettings::default();
    let mut mode = env::var("MODUM")
        .ok()
        .and_then(|raw| parse_check_mode(&raw).ok())
        .unwrap_or(CheckMode::Deny);
    let mut format = OutputFormat::Text;
    let mut selection = OutputSelection::All;

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
                scan_settings.include.push(value);
            }
            "--exclude" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--exclude requires a path or glob value".to_string())?;
                scan_settings.exclude.push(value);
            }
            "--mode" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--mode requires one of: off|warn|deny".to_string())?;
                mode = parse_check_mode(&value)?;
            }
            "--show" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--show requires one of: all|policy|advisory".to_string())?;
                selection = parse_output_selection(&value)?;
            }
            "--format" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--format requires one of: text|json".to_string())?;
                format = parse_output_format(&value)?;
            }
            "--help" | "-h" => {
                println!("{}", check_usage(command_prefix));
                return Ok(ExitCode::from(0));
            }
            other => {
                return Err(format!(
                    "unknown argument: {other}\n\n{}",
                    check_usage(command_prefix)
                ));
            }
        }
    }

    if mode == CheckMode::Off {
        println!("modum check skipped (mode=off)");
        return Ok(ExitCode::from(0));
    }

    if format == OutputFormat::Json && selection != OutputSelection::All {
        return Err(
            "--show is only available with text output; json already includes `policy` and `fix` metadata"
                .to_string(),
        );
    }

    let outcome = run_check_with_scan_settings(&root, &scan_settings, mode);
    match format {
        OutputFormat::Text => print!(
            "{}",
            render_pretty_report_with_selection(&outcome.report, selection.into())
        ),
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

fn parse_output_selection(raw: &str) -> Result<OutputSelection, String> {
    match raw {
        "all" => Ok(OutputSelection::All),
        "policy" => Ok(OutputSelection::Policy),
        "advisory" => Ok(OutputSelection::Advisory),
        _ => Err(format!(
            "invalid show mode `{raw}`; expected all|policy|advisory"
        )),
    }
}

impl From<OutputSelection> for DiagnosticSelection {
    fn from(value: OutputSelection) -> Self {
        match value {
            OutputSelection::All => DiagnosticSelection::All,
            OutputSelection::Policy => DiagnosticSelection::Policy,
            OutputSelection::Advisory => DiagnosticSelection::Advisory,
        }
    }
}

fn top_level_usage(command_prefix: &'static str) -> String {
    [
        "Usage:",
        &format!("  {command_prefix} check [options]"),
        "",
        "Commands:",
        "  check    Analyze a crate or workspace and report naming-policy violations",
        "",
        "Config:",
        "  Cargo metadata: [workspace.metadata.modum] or [package.metadata.modum]",
    ]
    .join("\n")
}

fn check_usage(command_prefix: &'static str) -> String {
    [
        "Usage:",
        &format!(
            "  {} check [--root <path>] [--include <path-or-glob>]... [--exclude <path-or-glob>]... [--show all|policy|advisory] [--mode off|warn|deny] [--format text|json]",
            command_prefix
        ),
        "",
        "Examples:",
        &format!("  {command_prefix} check"),
        &format!("  {command_prefix} check --mode warn"),
        &format!("  {command_prefix} check --exclude examples/high-coverage/**"),
        &format!("  {command_prefix} check --show advisory"),
        &format!("  {command_prefix} check --format json"),
        "",
        "Environment:",
        "  MODUM=off|warn|deny (default: deny)",
    ]
    .join("\n")
}
