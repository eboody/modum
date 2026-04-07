use std::{
    env,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};

use modum::{
    AnalysisSettings, CheckMode, DiagnosticSelection, ScanSettings, diagnostic_code_info,
    render_diagnostic_explanation, render_pretty_report_with_selection, run_check_with_settings,
    write_diagnostic_baseline,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
}

const MARKDOWN_REPORT_FILENAME_PREFIX: &str = "modum-lint-report-";

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            _ => Err(format!("invalid format `{raw}`; expected text|json")),
        }
    }
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
        Some("--explain") => run_explain_command(args, command_prefix),
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
    let run_dir = env::current_dir().map_err(|err| format!("failed to get current dir: {err}"))?;
    let mut root = run_dir.clone();
    let mut scan_settings = ScanSettings::default();
    let mut explain_code = None;
    let mut profile = None;
    let mut ignored_diagnostic_codes = Vec::new();
    let mut mode = env::var("MODUM")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(CheckMode::Deny);
    let mut format = OutputFormat::Text;
    let mut selection = DiagnosticSelection::All;
    let mut baseline = None;
    let mut write_baseline = None;
    let mut should_write_markdown_report = false;

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
            "--profile" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--profile requires one of: core|surface|strict".to_string())?;
                profile = Some(
                    value
                        .parse()
                        .map_err(|err: String| format!("--profile {err}"))?,
                );
            }
            "--ignore" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--ignore requires a diagnostic code".to_string())?;
                if diagnostic_code_info(&value).is_none() {
                    return Err(format!("--ignore unknown diagnostic code `{value}`"));
                }
                ignored_diagnostic_codes.push(value);
            }
            "--explain" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--explain requires a diagnostic code".to_string())?;
                explain_code = Some(value);
            }
            "--mode" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--mode requires one of: off|warn|deny".to_string())?;
                mode = value.parse()?;
            }
            "--show" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--show requires one of: all|policy|advisory".to_string())?;
                selection = value.parse()?;
            }
            "--format" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--format requires one of: text|json".to_string())?;
                format = value.parse()?;
            }
            "--baseline" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--baseline requires a path value".to_string())?;
                baseline = Some(PathBuf::from(value));
            }
            "--write-baseline" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--write-baseline requires a path value".to_string())?;
                write_baseline = Some(PathBuf::from(value));
            }
            "--write-markdown-report" | "-w" => {
                should_write_markdown_report = true;
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

    if let Some(code) = explain_code.as_deref() {
        print_explanation(code)?;
        return Ok(ExitCode::from(0));
    }

    if mode == CheckMode::Off {
        println!("modum check skipped (mode=off)");
        return Ok(ExitCode::from(0));
    }

    if baseline.is_some() && write_baseline.is_some() {
        return Err(
            "--baseline and --write-baseline cannot be used together; write a fresh baseline first, then apply it in a separate run"
                .to_string(),
        );
    }

    if format == OutputFormat::Json && selection != DiagnosticSelection::All {
        return Err(
            "--show is only available with text output; json already includes `policy` and `fix` metadata"
                .to_string(),
        );
    }

    let outcome = run_check_with_settings(
        &root,
        &AnalysisSettings {
            scan: scan_settings,
            profile,
            ignored_diagnostic_codes,
            baseline,
        },
        mode,
    );
    if let Some(path) = write_baseline {
        let count = write_diagnostic_baseline(&root, &path, &outcome.report)
            .map_err(|err| format!("failed to write baseline: {err}"))?;
        eprintln!(
            "wrote baseline {} ({count} coded diagnostics)",
            path.display()
        );
    }
    let text_report = render_pretty_report_with_selection(&outcome.report, selection);
    if should_write_markdown_report {
        let report_path = write_markdown_report(&run_dir, &text_report)?;
        eprintln!("wrote markdown report {}", report_path.display());
    }
    match format {
        OutputFormat::Text => print!("{text_report}"),
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

fn run_explain_command(
    mut args: impl Iterator<Item = String>,
    command_prefix: &'static str,
) -> Result<ExitCode, String> {
    let Some(code) = args.next() else {
        return Err(format!(
            "--explain requires a diagnostic code\n\n{}",
            top_level_usage(command_prefix)
        ));
    };
    print_explanation(&code)?;
    Ok(ExitCode::from(0))
}

fn print_explanation(code: &str) -> Result<(), String> {
    let rendered = render_diagnostic_explanation(code)
        .ok_or_else(|| format!("unknown diagnostic code `{code}`"))?;
    println!("{rendered}");
    Ok(())
}

fn top_level_usage(command_prefix: &'static str) -> String {
    [
        "Usage:",
        &format!("  {command_prefix} check [options]"),
        &format!("  {command_prefix} --explain <code>"),
        "",
        "Commands:",
        "  check    Analyze a crate or workspace and report naming-policy violations",
        "",
        "Config:",
        "  Cargo metadata: [workspace.metadata.modum] or [package.metadata.modum]",
    ]
    .join("\n")
}

fn write_markdown_report(run_dir: &Path, text_report: &str) -> Result<PathBuf, String> {
    let markdown = format!(
        "# modum lint report\n\n```text\n{}```\n",
        text_report.trim_end()
    );
    let timestamp_secs = current_timestamp_secs()?;

    for collision_index in 0.. {
        let report_path = run_dir.join(markdown_report_filename(timestamp_secs, collision_index));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&report_path)
        {
            Ok(mut file) => {
                file.write_all(markdown.as_bytes()).map_err(|err| {
                    format!(
                        "failed to write markdown report {}: {err}",
                        report_path.display()
                    )
                })?;
                return Ok(report_path);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(format!(
                    "failed to create markdown report {}: {err}",
                    report_path.display()
                ));
            }
        }
    }

    unreachable!("collision index iterator is unbounded")
}

fn current_timestamp_secs() -> Result<u64, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("failed to get current timestamp: {err}"))?;
    Ok(timestamp.as_secs())
}

fn markdown_report_filename(timestamp_secs: u64, collision_index: usize) -> String {
    if collision_index == 0 {
        return format!("{MARKDOWN_REPORT_FILENAME_PREFIX}{timestamp_secs}.md");
    }

    format!(
        "{MARKDOWN_REPORT_FILENAME_PREFIX}{timestamp_secs}-{}.md",
        collision_index + 1
    )
}

fn check_usage(command_prefix: &'static str) -> String {
    [
        "Usage:",
        &format!(
            "  {} check [--root <path>] [--include <path-or-glob>]... [--exclude <path-or-glob>]... [--profile core|surface|strict] [--ignore <code>]... [--baseline <path>] [--write-baseline <path>] [--write-markdown-report|-w] [--show all|policy|advisory] [--mode off|warn|deny] [--format text|json] [--explain <code>]",
            command_prefix
        ),
        "",
        "Examples:",
        &format!("  {command_prefix} check"),
        &format!("  {command_prefix} check --mode warn"),
        &format!("  {command_prefix} check --profile core"),
        &format!("  {command_prefix} check -w"),
        &format!("  {command_prefix} check --ignore api_candidate_semantic_module"),
        &format!("  {command_prefix} check --write-baseline .modum-baseline.json"),
        &format!("  {command_prefix} check --baseline .modum-baseline.json"),
        &format!("  {command_prefix} --explain namespace_flat_use"),
        &format!("  {command_prefix} check --exclude examples/high-coverage/**"),
        &format!("  {command_prefix} check --show advisory"),
        &format!("  {command_prefix} check --format json"),
        "",
        "Environment:",
        "  MODUM=off|warn|deny (default: deny)",
    ]
    .join("\n")
}
