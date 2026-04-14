use std::{
    env,
    fmt::Write as _,
    fs::OpenOptions,
    io::Write as _,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};

use modum::{
    AnalysisSettings, CheckMode, Diagnostic, DiagnosticClass, DiagnosticSelection, ScanSettings,
    WorkspaceReport, diagnostic_code_info, render_diagnostic_explanation,
    render_pretty_report_with_selection, run_check_with_settings, write_diagnostic_baseline,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
}

const MARKDOWN_REPORT_FILENAME_PREFIX: &str = "modum-lint-report-";
const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD_CYAN: &str = "\x1b[1;36m";
const ANSI_BOLD_RED: &str = "\x1b[1;31m";
const ANSI_BOLD_YELLOW: &str = "\x1b[1;33m";
const ANSI_BOLD_BLUE: &str = "\x1b[1;34m";
const ANSI_BOLD_MAGENTA: &str = "\x1b[1;35m";
const ANSI_BOLD_GREEN: &str = "\x1b[1;32m";
const ANSI_BOLD_WHITE: &str = "\x1b[1;37m";
const ANSI_BOLD_WHITE_ON_RED: &str = "\x1b[1;37;41m";
const ANSI_BOLD_WHITE_ON_BLUE: &str = "\x1b[1;37;44m";
const ANSI_BOLD_BLACK_ON_WHITE: &str = "\x1b[1;30;47m";
const ANSI_BOLD_BLACK_ON_CYAN: &str = "\x1b[1;30;46m";
const ANSI_BOLD_BLACK_ON_YELLOW: &str = "\x1b[1;30;43m";
const ANSI_BOLD_BLACK_ON_GREEN: &str = "\x1b[1;30;42m";
const ANSI_DIM: &str = "\x1b[2m";

#[derive(Clone, Copy, PartialEq, Eq)]
enum CodeSpanRole {
    Problem,
    Trait,
    FamilyMarker,
    Suggestion,
}

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
    let mut pretty_text = false;

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
            "--pretty" | "-p" => {
                pretty_text = true;
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

    if format == OutputFormat::Json && pretty_text {
        return Err("--pretty is only available with text output".to_string());
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
    let plain_text_report = render_pretty_report_with_selection(&outcome.report, selection);
    if should_write_markdown_report {
        let report_path = write_markdown_report(&run_dir, &plain_text_report)?;
        eprintln!("wrote markdown report {}", report_path.display());
    }
    match format {
        OutputFormat::Text => {
            let text_report = if pretty_text {
                render_pretty_cli_report(&outcome.report, selection)
            } else {
                plain_text_report
            };
            print!("{text_report}");
        }
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

fn render_pretty_cli_report(report: &WorkspaceReport, selection: DiagnosticSelection) -> String {
    let filtered = report.filtered(selection);
    let mut out = String::new();

    let _ = writeln!(
        &mut out,
        "{} {}",
        ansi_badge("MODUM", ANSI_BOLD_BLACK_ON_CYAN),
        ansi("lint report", ANSI_BOLD_CYAN)
    );
    let _ = writeln!(
        &mut out,
        "{}",
        ansi("  ==============================", ANSI_DIM)
    );
    let _ = writeln!(&mut out);

    render_summary_row(
        &mut out,
        "Files scanned",
        &filtered.scanned_files.to_string(),
        ANSI_BOLD_BLACK_ON_WHITE,
        ANSI_BOLD_WHITE,
    );
    render_summary_row(
        &mut out,
        "Files with violations",
        &filtered.files_with_violations.to_string(),
        ANSI_BOLD_BLACK_ON_WHITE,
        ANSI_BOLD_WHITE,
    );
    render_summary_row(
        &mut out,
        "Errors",
        &filtered.error_count().to_string(),
        ANSI_BOLD_WHITE_ON_RED,
        ANSI_BOLD_RED,
    );
    render_summary_row(
        &mut out,
        "Policy warnings",
        &filtered.policy_warning_count().to_string(),
        ANSI_BOLD_BLACK_ON_YELLOW,
        ANSI_BOLD_YELLOW,
    );
    render_summary_row(
        &mut out,
        "Advisory warnings",
        &filtered.advisory_warning_count().to_string(),
        ANSI_BOLD_WHITE_ON_BLUE,
        ANSI_BOLD_BLUE,
    );
    if let Some(selection_label) = selection.report_label() {
        render_summary_row(
            &mut out,
            "Showing",
            selection_label,
            ANSI_BOLD_BLACK_ON_CYAN,
            ANSI_BOLD_WHITE,
        );
        let _ = writeln!(
            &mut out,
            "  {:<28} {}",
            "",
            ansi("(exit code still reflects the full report)", ANSI_DIM)
        );
    }

    if filtered.diagnostics.is_empty() {
        let _ = writeln!(&mut out);
        let _ = writeln!(
            &mut out,
            "{}",
            ansi("No diagnostics found.", ANSI_BOLD_GREEN)
        );
        return out;
    }

    let errors = filtered
        .diagnostics
        .iter()
        .filter(|diag| diag.is_error())
        .collect::<Vec<_>>();
    let policy = filtered
        .diagnostics
        .iter()
        .filter(|diag| diag.is_policy_warning())
        .collect::<Vec<_>>();
    let advisory = filtered
        .diagnostics
        .iter()
        .filter(|diag| diag.is_advisory_warning())
        .collect::<Vec<_>>();

    let _ = writeln!(&mut out);
    render_pretty_diagnostic_section(&mut out, "Errors", &errors, ANSI_BOLD_WHITE_ON_RED);
    render_pretty_diagnostic_section(
        &mut out,
        "Policy Diagnostics",
        &policy,
        ANSI_BOLD_BLACK_ON_YELLOW,
    );
    render_pretty_diagnostic_section(
        &mut out,
        "Advisory Diagnostics",
        &advisory,
        ANSI_BOLD_WHITE_ON_BLUE,
    );

    out
}

fn render_summary_row(
    out: &mut String,
    label: &str,
    value: &str,
    label_style: &str,
    value_style: &str,
) {
    let _ = writeln!(
        out,
        "  {} {}",
        ansi_badge(label, label_style),
        ansi(value, value_style)
    );
}

fn render_pretty_diagnostic_section(
    out: &mut String,
    title: &str,
    diagnostics: &[&Diagnostic],
    section_style: &str,
) {
    if diagnostics.is_empty() {
        return;
    }

    let _ = writeln!(
        out,
        "{} {}",
        ansi_badge(title, section_style),
        ansi(
            format!(
                "{} item{}",
                diagnostics.len(),
                if diagnostics.len() == 1 { "" } else { "s" }
            ),
            ANSI_DIM
        )
    );
    let _ = writeln!(
        out,
        "{}",
        ansi(
            "  --------------------------------------------------",
            ANSI_DIM
        )
    );
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        let code = match (diagnostic.code(), diagnostic.profile()) {
            (Some(code), Some(profile)) => {
                format!(
                    "{} {}",
                    ansi_badge(code, ANSI_BOLD_BLACK_ON_WHITE),
                    ansi_badge(profile.as_str(), ANSI_BOLD_BLACK_ON_CYAN)
                )
            }
            (Some(code), None) => ansi_badge(code, ANSI_BOLD_BLACK_ON_WHITE),
            (None, _) => ansi_badge("tool", ANSI_BOLD_BLACK_ON_WHITE),
        };
        let _ = writeln!(
            out,
            "  {} {} {}",
            ansi(format!("{:>2}.", index + 1), ANSI_DIM),
            diagnostic_kind_badge(diagnostic),
            code
        );

        if let Some(location) = diagnostic_location(diagnostic) {
            render_pretty_detail(out, "FILE", &location, ANSI_BOLD_WHITE_ON_BLUE, ANSI_DIM);
        }

        render_pretty_lint_detail(out, &diagnostic.message);

        if let Some(guidance) = diagnostic.guidance() {
            render_pretty_guidance_detail(out, "WHY", &guidance.why, ANSI_BOLD_BLACK_ON_CYAN);
            render_pretty_guidance_detail(
                out,
                "ADDRESS",
                &guidance.address,
                ANSI_BOLD_BLACK_ON_GREEN,
            );
        }

        if let Some(fix) = &diagnostic.fix {
            render_pretty_fix_detail(out, &fix.replacement);
        }

        if index + 1 != diagnostics.len() {
            let _ = writeln!(
                out,
                "{}",
                ansi(
                    "      ................................................",
                    ANSI_DIM
                )
            );
            let _ = writeln!(out);
        }
    }
    let _ = writeln!(out);
}

fn render_pretty_detail(
    out: &mut String,
    label: &str,
    value: &str,
    label_style: &str,
    value_style: &str,
) {
    let _ = writeln!(
        out,
        "      {} {}",
        ansi_badge(label, label_style),
        ansi(value, value_style)
    );
}

fn render_pretty_lint_detail(out: &mut String, message: &str) {
    let _ = writeln!(
        out,
        "      {} {}",
        ansi_badge("LINT", ANSI_BOLD_BLACK_ON_YELLOW),
        render_pretty_message(message)
    );
}

fn render_pretty_guidance_detail(out: &mut String, label: &str, message: &str, style: &str) {
    let _ = writeln!(
        out,
        "      {} {}",
        ansi_badge(label, style),
        render_pretty_message(message)
    );
}

fn render_pretty_fix_detail(out: &mut String, replacement: &str) {
    let _ = writeln!(
        out,
        "      {} {} {}",
        ansi_badge("CHANGE", ANSI_BOLD_BLACK_ON_GREEN),
        ansi("replace with", ANSI_BOLD_GREEN),
        render_inline_code_span(replacement, CodeSpanRole::Suggestion)
    );
}

fn render_pretty_message(message: &str) -> String {
    let mut out = String::new();
    let mut previous_code_role = None;
    let parts = message.split('`').collect::<Vec<_>>();

    for (index, part) in parts.iter().enumerate() {
        if index % 2 == 0 {
            if !part.is_empty() {
                out.push_str(&ansi(part, ANSI_BOLD_WHITE));
            }
            continue;
        }

        let role = code_span_role(parts[index - 1], previous_code_role);
        previous_code_role = Some(role);
        out.push_str(&render_inline_code_span(part, role));
    }

    out
}

fn code_span_role(previous_text: &str, previous_code_role: Option<CodeSpanRole>) -> CodeSpanRole {
    let context = previous_text.to_ascii_lowercase();
    let trimmed = context.trim();

    if trimmed.ends_with("prefer")
        || trimmed.ends_with("consider a semantic")
        || matches!(trimmed, "or" | "and" | "on")
            && previous_code_role == Some(CodeSpanRole::Suggestion)
    {
        return CodeSpanRole::Suggestion;
    }

    if trimmed.ends_with("manually implements both")
        || trimmed == "and" && previous_code_role == Some(CodeSpanRole::Trait)
    {
        return CodeSpanRole::Trait;
    }

    if trimmed.ends_with("share the")
        || trimmed.ends_with("share the generic")
        || trimmed == "head and" && previous_code_role == Some(CodeSpanRole::FamilyMarker)
    {
        return CodeSpanRole::FamilyMarker;
    }

    CodeSpanRole::Problem
}

fn render_inline_code_span(code: &str, role: CodeSpanRole) -> String {
    let mut out = String::new();
    out.push_str(&ansi("`", ANSI_DIM));
    out.push_str(&render_code_span_content(code, role));
    out.push_str(&ansi("`", ANSI_DIM));
    out
}

fn render_code_span_content(code: &str, role: CodeSpanRole) -> String {
    let mut out = String::new();
    let mut chars = code.char_indices().peekable();
    let mut inside_group = false;

    while let Some((start, ch)) = chars.next() {
        if ch.is_whitespace() {
            let mut end = start + ch.len_utf8();
            while let Some((next_start, next_ch)) = chars.peek().copied() {
                if !next_ch.is_whitespace() {
                    break;
                }
                chars.next();
                end = next_start + next_ch.len_utf8();
            }
            out.push_str(&code[start..end]);
            continue;
        }

        if ch == ':' && chars.next_if(|(_, next)| *next == ':').is_some() {
            out.push_str(&ansi("::", separator_style(role)));
            continue;
        }

        if is_code_ident_char(ch) {
            let mut end = start + ch.len_utf8();
            while let Some((next_start, next_ch)) = chars.peek().copied() {
                if !is_code_ident_char(next_ch) {
                    break;
                }
                chars.next();
                end = next_start + next_ch.len_utf8();
            }
            let ident = &code[start..end];
            out.push_str(&ansi(
                ident,
                code_ident_style(role, inside_group, remaining_starts_with_separator(&chars)),
            ));
            continue;
        }

        out.push_str(&ansi(
            &code[start..start + ch.len_utf8()],
            punctuation_style(role, ch),
        ));

        if ch == '{' {
            inside_group = true;
        } else if ch == '}' {
            inside_group = false;
        }
    }

    out
}

fn remaining_starts_with_separator(chars: &std::iter::Peekable<std::str::CharIndices<'_>>) -> bool {
    let mut clone = chars.clone();
    while let Some((_, ch)) = clone.next() {
        if ch.is_whitespace() {
            continue;
        }
        if ch != ':' {
            return false;
        }
        return matches!(clone.next(), Some((_, ':')));
    }
    false
}

fn is_code_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '\'')
}

fn code_ident_style(
    role: CodeSpanRole,
    inside_group: bool,
    followed_by_separator: bool,
) -> &'static str {
    match role {
        CodeSpanRole::Problem => ANSI_BOLD_RED,
        CodeSpanRole::Trait => ANSI_BOLD_BLUE,
        CodeSpanRole::FamilyMarker => ANSI_BOLD_MAGENTA,
        CodeSpanRole::Suggestion if followed_by_separator => ANSI_BOLD_CYAN,
        CodeSpanRole::Suggestion if inside_group => ANSI_BOLD_GREEN,
        CodeSpanRole::Suggestion => ANSI_BOLD_GREEN,
    }
}

fn separator_style(role: CodeSpanRole) -> &'static str {
    match role {
        CodeSpanRole::Problem => ANSI_BOLD_RED,
        CodeSpanRole::Trait => ANSI_BOLD_BLUE,
        CodeSpanRole::FamilyMarker => ANSI_BOLD_MAGENTA,
        CodeSpanRole::Suggestion => ANSI_BOLD_YELLOW,
    }
}

fn punctuation_style(role: CodeSpanRole, ch: char) -> &'static str {
    match role {
        CodeSpanRole::Problem => ANSI_BOLD_RED,
        CodeSpanRole::Trait => ANSI_BOLD_BLUE,
        CodeSpanRole::FamilyMarker => ANSI_BOLD_MAGENTA,
        CodeSpanRole::Suggestion
            if matches!(ch, '{' | '}' | '(' | ')' | '[' | ']' | '<' | '>' | ',') =>
        {
            ANSI_BOLD_MAGENTA
        }
        CodeSpanRole::Suggestion => ANSI_BOLD_WHITE,
    }
}

fn diagnostic_location(diagnostic: &Diagnostic) -> Option<String> {
    diagnostic.file.as_ref().map(|file| match diagnostic.line {
        Some(line) => format!("{}:{line}", file.display()),
        None => file.display().to_string(),
    })
}

fn diagnostic_kind_badge(diagnostic: &Diagnostic) -> String {
    match &diagnostic.class {
        DiagnosticClass::ToolError | DiagnosticClass::PolicyError { .. } => {
            ansi_badge("ERROR", ANSI_BOLD_WHITE_ON_RED)
        }
        DiagnosticClass::ToolWarning => ansi_badge("WARNING", ANSI_BOLD_BLACK_ON_YELLOW),
        DiagnosticClass::PolicyWarning { .. } => ansi_badge("POLICY", ANSI_BOLD_BLACK_ON_YELLOW),
        DiagnosticClass::AdvisoryWarning { .. } => ansi_badge("ADVISORY", ANSI_BOLD_WHITE_ON_BLUE),
    }
}

fn ansi_badge(text: impl std::fmt::Display, style: &str) -> String {
    ansi(format!(" {text} "), style)
}

fn ansi(text: impl std::fmt::Display, style: &str) -> String {
    format!("{style}{text}{ANSI_RESET}")
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
            "  {} check [--root <path>] [--include <path-or-glob>]... [--exclude <path-or-glob>]... [--profile core|surface|strict] [--ignore <code>]... [--baseline <path>] [--write-baseline <path>] [--write-markdown-report|-w] [--pretty|-p] [--show all|policy|advisory] [--mode off|warn|deny] [--format text|json] [--explain <code>]",
            command_prefix
        ),
        "",
        "Examples:",
        &format!("  {command_prefix} check"),
        &format!("  {command_prefix} check --mode warn"),
        &format!("  {command_prefix} check --profile core"),
        &format!("  {command_prefix} check -w"),
        &format!("  {command_prefix} check -p"),
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
