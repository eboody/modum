use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
};

use glob::{Pattern, glob};
use serde::Serialize;
use walkdir::WalkDir;

mod api_shape;
mod namespace;

const DEFAULT_GENERIC_NOUNS: &[&str] = &[
    "Id",
    "Repository",
    "Service",
    "Error",
    "Command",
    "Request",
    "Response",
];

const DEFAULT_WEAK_MODULES: &[&str] = &[
    "storage",
    "transport",
    "infra",
    "common",
    "misc",
    "helpers",
    "helper",
    "types",
    "util",
    "utils",
];

const DEFAULT_CATCH_ALL_MODULES: &[&str] = &[
    "common", "misc", "helpers", "helper", "types", "util", "utils",
];

const DEFAULT_ORGANIZATIONAL_MODULES: &[&str] = &["error", "errors"];

const DEFAULT_NAMESPACE_PRESERVING_MODULES: &[&str] = &[
    "auth",
    "command",
    "email",
    "error",
    "http",
    "page",
    "partials",
    "policy",
    "query",
    "repo",
    "store",
    "storage",
    "transport",
    "infra",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum DiagnosticLevel {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub file: Option<PathBuf>,
    pub line: Option<usize>,
    pub code: Option<String>,
    pub policy: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisResult {
    pub diagnostics: Vec<Diagnostic>,
}

impl AnalysisResult {
    fn empty() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceReport {
    pub scanned_files: usize,
    pub files_with_violations: usize,
    pub diagnostics: Vec<Diagnostic>,
}

impl WorkspaceReport {
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diag| diag.level == DiagnosticLevel::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diag| diag.level == DiagnosticLevel::Warning)
            .count()
    }

    pub fn policy_violation_count(&self) -> usize {
        self.diagnostics.iter().filter(|diag| diag.policy).count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckMode {
    Off,
    Warn,
    Deny,
}

impl CheckMode {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "off" => Ok(Self::Off),
            "warn" => Ok(Self::Warn),
            "deny" => Ok(Self::Deny),
            _ => Err(format!("invalid mode `{raw}`; expected off|warn|deny")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckOutcome {
    pub report: WorkspaceReport,
    pub exit_code: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NamespaceSettings {
    generic_nouns: BTreeSet<String>,
    weak_modules: BTreeSet<String>,
    catch_all_modules: BTreeSet<String>,
    organizational_modules: BTreeSet<String>,
    namespace_preserving_modules: BTreeSet<String>,
}

impl Default for NamespaceSettings {
    fn default() -> Self {
        Self {
            generic_nouns: DEFAULT_GENERIC_NOUNS
                .iter()
                .map(|noun| (*noun).to_string())
                .collect(),
            weak_modules: DEFAULT_WEAK_MODULES
                .iter()
                .map(|module| (*module).to_string())
                .collect(),
            catch_all_modules: DEFAULT_CATCH_ALL_MODULES
                .iter()
                .map(|module| (*module).to_string())
                .collect(),
            organizational_modules: DEFAULT_ORGANIZATIONAL_MODULES
                .iter()
                .map(|module| (*module).to_string())
                .collect(),
            namespace_preserving_modules: DEFAULT_NAMESPACE_PRESERVING_MODULES
                .iter()
                .map(|module| (*module).to_string())
                .collect(),
        }
    }
}

pub fn parse_check_mode(raw: &str) -> Result<CheckMode, String> {
    CheckMode::parse(raw)
}

pub fn run_check(root: &Path, include_globs: &[String], mode: CheckMode) -> CheckOutcome {
    if mode == CheckMode::Off {
        return CheckOutcome {
            report: WorkspaceReport {
                scanned_files: 0,
                files_with_violations: 0,
                diagnostics: Vec::new(),
            },
            exit_code: 0,
        };
    }

    let report = analyze_workspace(root, include_globs);
    let exit_code = check_exit_code(&report, mode);
    CheckOutcome { report, exit_code }
}

fn check_exit_code(report: &WorkspaceReport, mode: CheckMode) -> u8 {
    if report.error_count() > 0 {
        return 1;
    }

    if report.policy_violation_count() == 0 || mode == CheckMode::Warn {
        0
    } else {
        2
    }
}

pub fn analyze_file(path: &Path, src: &str) -> AnalysisResult {
    analyze_file_with_settings(path, src, &NamespaceSettings::default())
}

fn analyze_file_with_settings(
    path: &Path,
    src: &str,
    settings: &NamespaceSettings,
) -> AnalysisResult {
    let parsed = match syn::parse_file(src) {
        Ok(file) => file,
        Err(err) => {
            return AnalysisResult {
                diagnostics: vec![Diagnostic {
                    level: DiagnosticLevel::Error,
                    file: Some(path.to_path_buf()),
                    line: None,
                    code: None,
                    policy: false,
                    message: format!("failed to parse rust file: {err}"),
                }],
            };
        }
    };

    let mut result = AnalysisResult::empty();
    result
        .diagnostics
        .extend(namespace::analyze_namespace_rules(path, &parsed, settings).diagnostics);
    result
        .diagnostics
        .extend(api_shape::analyze_api_shape_rules(path, &parsed, settings).diagnostics);
    result.diagnostics.sort();
    result
}

pub fn analyze_workspace(root: &Path, include_globs: &[String]) -> WorkspaceReport {
    let mut diagnostics = Vec::new();
    let workspace_defaults = load_workspace_settings(root, &mut diagnostics);
    let rust_files = match collect_rust_files(root, include_globs) {
        Ok(files) => files,
        Err(err) => {
            diagnostics.push(Diagnostic {
                level: DiagnosticLevel::Error,
                file: None,
                line: None,
                code: None,
                policy: false,
                message: format!("failed to discover rust files: {err}"),
            });
            return WorkspaceReport {
                scanned_files: 0,
                files_with_violations: 0,
                diagnostics,
            };
        }
    };

    if rust_files.is_empty() {
        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            file: None,
            line: None,
            code: None,
            policy: false,
            message:
                "no Rust files were discovered; pass --include <path>... or run from a crate/workspace root"
                    .to_string(),
        });
    }

    let mut files_with_violations = BTreeSet::new();
    let mut package_cache = BTreeMap::new();

    for file in &rust_files {
        let src = match fs::read_to_string(file) {
            Ok(src) => src,
            Err(err) => {
                diagnostics.push(Diagnostic {
                    level: DiagnosticLevel::Error,
                    file: Some(file.clone()),
                    line: None,
                    code: None,
                    policy: false,
                    message: format!("failed to read file: {err}"),
                });
                continue;
            }
        };

        let settings = settings_for_file(root, file, &workspace_defaults, &mut package_cache);
        let analysis = analyze_file_with_settings(file, &src, &settings);
        if !analysis.diagnostics.is_empty() {
            files_with_violations.insert(file.clone());
        }
        diagnostics.extend(analysis.diagnostics);
    }

    diagnostics.sort();

    WorkspaceReport {
        scanned_files: rust_files.len(),
        files_with_violations: files_with_violations.len(),
        diagnostics,
    }
}

fn load_workspace_settings(root: &Path, diagnostics: &mut Vec<Diagnostic>) -> NamespaceSettings {
    let manifest_path = root.join("Cargo.toml");
    let Ok(manifest_src) = fs::read_to_string(&manifest_path) else {
        return NamespaceSettings::default();
    };

    let manifest: toml::Value = match toml::from_str(&manifest_src) {
        Ok(manifest) => manifest,
        Err(err) => {
            diagnostics.push(Diagnostic {
                level: DiagnosticLevel::Error,
                file: Some(manifest_path),
                line: None,
                code: None,
                policy: false,
                message: format!("failed to parse Cargo.toml for modum settings: {err}"),
            });
            return NamespaceSettings::default();
        }
    };

    parse_settings_from_manifest(
        manifest
            .get("workspace")
            .and_then(toml::Value::as_table)
            .and_then(|workspace| workspace.get("metadata"))
            .and_then(toml::Value::as_table)
            .and_then(|metadata| metadata.get("modum")),
        &manifest_path,
        diagnostics,
    )
    .unwrap_or_default()
}

fn settings_for_file(
    root: &Path,
    file: &Path,
    workspace_defaults: &NamespaceSettings,
    cache: &mut BTreeMap<PathBuf, NamespaceSettings>,
) -> NamespaceSettings {
    let Some(package_root) = find_package_root(root, file) else {
        return workspace_defaults.clone();
    };

    cache
        .entry(package_root.clone())
        .or_insert_with(|| load_package_settings(&package_root, workspace_defaults))
        .clone()
}

fn load_package_settings(root: &Path, workspace_defaults: &NamespaceSettings) -> NamespaceSettings {
    let manifest_path = root.join("Cargo.toml");
    let Ok(manifest_src) = fs::read_to_string(&manifest_path) else {
        return workspace_defaults.clone();
    };
    let Ok(manifest) = toml::from_str::<toml::Value>(&manifest_src) else {
        return workspace_defaults.clone();
    };

    parse_settings_from_manifest(
        manifest
            .get("package")
            .and_then(toml::Value::as_table)
            .and_then(|package| package.get("metadata"))
            .and_then(toml::Value::as_table)
            .and_then(|metadata| metadata.get("modum")),
        &manifest_path,
        &mut Vec::new(),
    )
    .unwrap_or_else(|| workspace_defaults.clone())
}

fn parse_settings_from_manifest(
    value: Option<&toml::Value>,
    manifest_path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<NamespaceSettings> {
    let table = value?.as_table()?;
    let mut settings = NamespaceSettings::default();

    if let Some(values) = parse_string_set_field(table, "generic_nouns", manifest_path, diagnostics)
    {
        settings.generic_nouns = values;
    }
    if let Some(values) = parse_string_set_field(table, "weak_modules", manifest_path, diagnostics)
    {
        settings.weak_modules = values;
    }
    if let Some(values) =
        parse_string_set_field(table, "catch_all_modules", manifest_path, diagnostics)
    {
        settings.catch_all_modules = values;
    }
    if let Some(values) =
        parse_string_set_field(table, "organizational_modules", manifest_path, diagnostics)
    {
        settings.organizational_modules = values;
    }
    if let Some(values) = parse_string_set_field(
        table,
        "namespace_preserving_modules",
        manifest_path,
        diagnostics,
    ) {
        settings.namespace_preserving_modules = values;
    }

    Some(settings)
}

fn parse_string_set_field(
    table: &toml::value::Table,
    key: &str,
    manifest_path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<BTreeSet<String>> {
    let value = table.get(key)?;
    let Some(array) = value.as_array() else {
        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Error,
            file: Some(manifest_path.to_path_buf()),
            line: None,
            code: None,
            policy: false,
            message: format!("`metadata.modum.{key}` must be an array of strings"),
        });
        return None;
    };

    Some(
        array
            .iter()
            .filter_map(toml::Value::as_str)
            .map(|value| value.to_string())
            .collect(),
    )
}

fn find_package_root(root: &Path, file: &Path) -> Option<PathBuf> {
    for ancestor in file.ancestors().skip(1) {
        let manifest_path = ancestor.join("Cargo.toml");
        if manifest_path.is_file()
            && let Ok(manifest_src) = fs::read_to_string(&manifest_path)
            && let Ok(manifest) = toml::from_str::<toml::Value>(&manifest_src)
            && manifest.get("package").is_some_and(toml::Value::is_table)
        {
            return Some(ancestor.to_path_buf());
        }
        if ancestor == root {
            break;
        }
    }
    None
}

pub fn render_pretty_report(report: &WorkspaceReport) -> String {
    let mut out = String::new();

    let _ = writeln!(&mut out, "modum lint report");
    let _ = writeln!(&mut out, "files scanned: {}", report.scanned_files);
    let _ = writeln!(
        &mut out,
        "files with violations: {}",
        report.files_with_violations
    );
    let _ = writeln!(
        &mut out,
        "diagnostics: {} error(s), {} warning(s)",
        report.error_count(),
        report.warning_count()
    );
    if report.policy_violation_count() > 0 {
        let _ = writeln!(
            &mut out,
            "policy violations: {}",
            report.policy_violation_count()
        );
    }

    if !report.diagnostics.is_empty() {
        let _ = writeln!(&mut out);
        let _ = writeln!(&mut out, "Diagnostics:");
        for diag in &report.diagnostics {
            let level = match diag.level {
                DiagnosticLevel::Warning => "warning",
                DiagnosticLevel::Error => "error",
            };
            let code = diag
                .code
                .as_deref()
                .map(|code| format!(" ({code})"))
                .unwrap_or_default();
            match (&diag.file, diag.line) {
                (Some(file), Some(line)) => {
                    let _ = writeln!(
                        &mut out,
                        "- [{level}{code}] {}:{line}: {}",
                        file.display(),
                        diag.message
                    );
                }
                (Some(file), None) => {
                    let _ = writeln!(
                        &mut out,
                        "- [{level}{code}] {}: {}",
                        file.display(),
                        diag.message
                    );
                }
                (None, _) => {
                    let _ = writeln!(&mut out, "- [{level}{code}] {}", diag.message);
                }
            }
        }
    }

    out
}

fn collect_rust_files(root: &Path, include_globs: &[String]) -> io::Result<Vec<PathBuf>> {
    let mut files = BTreeSet::new();
    if include_globs.is_empty() {
        for scan_root in collect_default_scan_roots(root)? {
            collect_rust_files_in_dir(&scan_root, &mut files);
        }
    } else {
        for entry in include_globs {
            let candidate = root.join(entry);
            if candidate.is_file() && is_rust_file(&candidate) {
                files.insert(candidate);
            } else if candidate.is_dir() {
                collect_rust_files_in_dir(&candidate, &mut files);
            }
        }
    }
    Ok(files.into_iter().collect())
}

fn collect_default_scan_roots(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut scan_roots = BTreeSet::new();
    let manifest_path = root.join("Cargo.toml");

    if !manifest_path.is_file() {
        add_src_root(root, &mut scan_roots);
        return Ok(scan_roots.into_iter().collect());
    }

    let manifest_src = fs::read_to_string(&manifest_path)?;
    let manifest: toml::Value = toml::from_str(&manifest_src).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse {}: {err}", manifest_path.display()),
        )
    })?;

    let root_is_package = manifest.get("package").is_some_and(toml::Value::is_table);
    if root_is_package {
        add_src_root(root, &mut scan_roots);
    }

    if let Some(workspace) = manifest.get("workspace").and_then(toml::Value::as_table) {
        let excluded = parse_workspace_patterns(workspace.get("exclude"));
        for member_pattern in parse_workspace_patterns(workspace.get("members")) {
            for member_root in resolve_workspace_member_pattern(root, &member_pattern)? {
                if is_excluded_member(root, &member_root, &excluded)? {
                    continue;
                }
                add_src_root(&member_root, &mut scan_roots);
            }
        }
    } else if !root_is_package {
        add_src_root(root, &mut scan_roots);
    }

    Ok(scan_roots.into_iter().collect())
}

fn parse_workspace_patterns(value: Option<&toml::Value>) -> Vec<String> {
    value
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(std::string::ToString::to_string)
        .collect()
}

fn resolve_workspace_member_pattern(root: &Path, pattern: &str) -> io::Result<Vec<PathBuf>> {
    let candidate = root.join(pattern);
    if !contains_glob_meta(pattern) {
        if candidate.is_dir() {
            return Ok(vec![candidate]);
        }
        if candidate
            .file_name()
            .is_some_and(|name| name == "Cargo.toml")
            && let Some(parent) = candidate.parent()
        {
            return Ok(vec![parent.to_path_buf()]);
        }
        return Ok(Vec::new());
    }

    let escaped_root = Pattern::escape(&root.to_string_lossy());
    let normalized_pattern = pattern.replace('\\', "/");
    let full_pattern = format!("{escaped_root}/{normalized_pattern}");
    let mut paths = Vec::new();
    let matches = glob(&full_pattern).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid workspace member pattern `{pattern}`: {err}"),
        )
    })?;

    for entry in matches {
        let path = entry
            .map_err(|err| io::Error::other(format!("failed to expand `{pattern}`: {err}")))?;
        if path.is_dir() {
            paths.push(path);
            continue;
        }
        if path.file_name().is_some_and(|name| name == "Cargo.toml")
            && let Some(parent) = path.parent()
        {
            paths.push(parent.to_path_buf());
        }
    }

    Ok(paths)
}

fn contains_glob_meta(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[')
}

fn is_excluded_member(root: &Path, member_root: &Path, excluded: &[String]) -> io::Result<bool> {
    let relative = member_root
        .strip_prefix(root)
        .unwrap_or(member_root)
        .to_string_lossy()
        .replace('\\', "/");
    for pattern in excluded {
        let matcher = Pattern::new(pattern).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid workspace exclude pattern `{pattern}`: {err}"),
            )
        })?;
        if matcher.matches(&relative) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn add_src_root(root: &Path, scan_roots: &mut BTreeSet<PathBuf>) {
    let src = root.join("src");
    if src.is_dir() {
        scan_roots.insert(src);
    }
}

fn collect_rust_files_in_dir(dir: &Path, files: &mut BTreeSet<PathBuf>) {
    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if is_rust_file(path) {
            files.insert(path.to_path_buf());
        }
    }
}

fn is_rust_file(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "rs")
}

pub(crate) fn is_public(vis: &syn::Visibility) -> bool {
    !matches!(vis, syn::Visibility::Inherited)
}

pub(crate) fn unraw_ident(ident: &syn::Ident) -> String {
    let text = ident.to_string();
    text.strip_prefix("r#").unwrap_or(&text).to_string()
}

pub(crate) fn split_segments(name: &str) -> Vec<String> {
    if name.contains('_') {
        return name
            .split('_')
            .filter(|segment| !segment.is_empty())
            .map(std::string::ToString::to_string)
            .collect();
    }

    let chars: Vec<(usize, char)> = name.char_indices().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    let mut starts = vec![0usize];

    for i in 1..chars.len() {
        let prev = chars[i - 1].1;
        let curr = chars[i].1;
        let next = chars.get(i + 1).map(|(_, c)| *c);

        let lower_to_upper = prev.is_ascii_lowercase() && curr.is_ascii_uppercase();
        let acronym_to_word = prev.is_ascii_uppercase()
            && curr.is_ascii_uppercase()
            && next.map(|c| c.is_ascii_lowercase()).unwrap_or(false);

        if lower_to_upper || acronym_to_word {
            starts.push(chars[i].0);
        }
    }

    let mut out = Vec::with_capacity(starts.len());
    for (idx, start) in starts.iter().enumerate() {
        let end = if let Some(next) = starts.get(idx + 1) {
            *next
        } else {
            name.len()
        };
        let seg = &name[*start..end];
        if !seg.is_empty() {
            out.push(seg.to_string());
        }
    }

    out
}

pub(crate) fn normalize_segment(segment: &str) -> String {
    segment.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{
        CheckMode, Diagnostic, DiagnosticLevel, NamespaceSettings, WorkspaceReport,
        check_exit_code, parse_check_mode, split_segments,
    };

    #[test]
    fn splits_pascal_camel_snake_and_acronyms() {
        assert_eq!(split_segments("WhatEver"), vec!["What", "Ever"]);
        assert_eq!(split_segments("whatEver"), vec!["what", "Ever"]);
        assert_eq!(split_segments("what_ever"), vec!["what", "ever"]);
        assert_eq!(split_segments("HTTPServer"), vec!["HTTP", "Server"]);
    }

    #[test]
    fn parses_check_modes() {
        assert_eq!(parse_check_mode("off"), Ok(CheckMode::Off));
        assert_eq!(parse_check_mode("warn"), Ok(CheckMode::Warn));
        assert_eq!(parse_check_mode("deny"), Ok(CheckMode::Deny));
    }

    #[test]
    fn rejects_invalid_check_mode() {
        let err = parse_check_mode("strict").unwrap_err();
        assert!(err.contains("expected off|warn|deny"));
    }

    #[test]
    fn check_exit_code_follows_warn_and_deny_semantics() {
        let clean = WorkspaceReport {
            scanned_files: 1,
            files_with_violations: 0,
            diagnostics: Vec::new(),
        };
        assert_eq!(check_exit_code(&clean, CheckMode::Warn), 0);
        assert_eq!(check_exit_code(&clean, CheckMode::Deny), 0);

        let with_policy = WorkspaceReport {
            scanned_files: 1,
            files_with_violations: 1,
            diagnostics: vec![Diagnostic {
                level: DiagnosticLevel::Warning,
                file: None,
                line: None,
                code: Some("lint".to_string()),
                policy: true,
                message: "warning".to_string(),
            }],
        };
        assert_eq!(check_exit_code(&with_policy, CheckMode::Warn), 0);
        assert_eq!(check_exit_code(&with_policy, CheckMode::Deny), 2);

        let with_error = WorkspaceReport {
            scanned_files: 1,
            files_with_violations: 1,
            diagnostics: vec![Diagnostic {
                level: DiagnosticLevel::Error,
                file: None,
                line: None,
                code: None,
                policy: false,
                message: "error".to_string(),
            }],
        };
        assert_eq!(check_exit_code(&with_error, CheckMode::Warn), 1);
        assert_eq!(check_exit_code(&with_error, CheckMode::Deny), 1);
    }

    #[test]
    fn namespace_settings_defaults_cover_generic_nouns_and_weak_modules() {
        let settings = NamespaceSettings::default();
        assert!(settings.generic_nouns.contains("Repository"));
        assert!(settings.generic_nouns.contains("Id"));
        assert!(settings.weak_modules.contains("storage"));
        assert!(settings.catch_all_modules.contains("helpers"));
        assert!(settings.organizational_modules.contains("error"));
        assert!(settings.namespace_preserving_modules.contains("email"));
    }
}
