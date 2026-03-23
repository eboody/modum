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
mod diagnostic;
mod namespace;

pub use diagnostic::{
    Diagnostic, DiagnosticClass, DiagnosticCodeInfo, DiagnosticFix, DiagnosticFixKind,
    DiagnosticLevel, DiagnosticSelection, LintProfile, diagnostic_code_info,
};

const DEFAULT_GENERIC_NOUNS: &[&str] = &[
    "Id",
    "Repository",
    "Service",
    "Error",
    "Command",
    "Request",
    "Response",
    "Outcome",
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

const DEFAULT_ORGANIZATIONAL_MODULES: &[&str] = &["error", "errors", "request", "response"];

const DEFAULT_NAMESPACE_PRESERVING_MODULES: &[&str] = &[
    "auth",
    "command",
    "components",
    "email",
    "error",
    "http",
    "page",
    "partials",
    "policy",
    "query",
    "repo",
    "store",
    "trace",
    "storage",
    "transport",
    "infra",
    "write_back",
];

const DEFAULT_SEMANTIC_STRING_SCALARS: &[&str] =
    &["email", "url", "uri", "path", "locale", "currency", "ip"];

const DEFAULT_SEMANTIC_NUMERIC_SCALARS: &[&str] =
    &["duration", "timeout", "ttl", "timestamp", "port"];

const DEFAULT_KEY_VALUE_BAG_NAMES: &[&str] = &[
    "metadata",
    "attribute",
    "attributes",
    "header",
    "headers",
    "param",
    "params",
    "tag",
    "tags",
];

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
            .filter(|diag| diag.is_error())
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diag| !diag.is_error())
            .count()
    }

    pub fn policy_warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diag| diag.is_policy_warning())
            .count()
    }

    pub fn advisory_warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diag| diag.is_advisory_warning())
            .count()
    }

    pub fn policy_violation_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diag| diag.is_policy_violation())
            .count()
    }

    pub fn filtered(&self, selection: DiagnosticSelection) -> Self {
        let diagnostics = self
            .diagnostics
            .iter()
            .filter(|diag| selection.includes(diag))
            .cloned()
            .collect::<Vec<_>>();
        let files_with_violations = diagnostics
            .iter()
            .filter_map(|diag| diag.file.as_ref())
            .collect::<BTreeSet<_>>()
            .len();

        Self {
            scanned_files: self.scanned_files,
            files_with_violations,
            diagnostics,
        }
    }

    pub fn filtered_by_profile(&self, profile: LintProfile) -> Self {
        let diagnostics = self
            .diagnostics
            .iter()
            .filter(|diag| diag.included_in_profile(profile))
            .cloned()
            .collect::<Vec<_>>();
        let files_with_violations = diagnostics
            .iter()
            .filter_map(|diag| diag.file.as_ref())
            .collect::<BTreeSet<_>>()
            .len();

        Self {
            scanned_files: self.scanned_files,
            files_with_violations,
            diagnostics,
        }
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
        raw.parse()
    }
}

impl std::str::FromStr for CheckMode {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScanSettings {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnalysisSettings {
    pub scan: ScanSettings,
    pub profile: Option<LintProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NamespaceSettings {
    generic_nouns: BTreeSet<String>,
    weak_modules: BTreeSet<String>,
    catch_all_modules: BTreeSet<String>,
    organizational_modules: BTreeSet<String>,
    namespace_preserving_modules: BTreeSet<String>,
    semantic_string_scalars: BTreeSet<String>,
    semantic_numeric_scalars: BTreeSet<String>,
    key_value_bag_names: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PackageSettings {
    namespace: NamespaceSettings,
    profile: Option<LintProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSettings {
    namespace: NamespaceSettings,
    profile: LintProfile,
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
            semantic_string_scalars: DEFAULT_SEMANTIC_STRING_SCALARS
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
            semantic_numeric_scalars: DEFAULT_SEMANTIC_NUMERIC_SCALARS
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
            key_value_bag_names: DEFAULT_KEY_VALUE_BAG_NAMES
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
        }
    }
}

pub fn parse_check_mode(raw: &str) -> Result<CheckMode, String> {
    CheckMode::parse(raw)
}

pub fn parse_lint_profile(raw: &str) -> Result<LintProfile, String> {
    raw.parse()
}

pub fn render_diagnostic_explanation(code: &str) -> Option<String> {
    let info = diagnostic_code_info(code)?;
    let mut rendered = format!(
        "{code}\nprofile: {}\nsummary: {}",
        info.profile.as_str(),
        info.summary,
    );

    if let Some(details) = diagnostic_explanation_details(code) {
        for detail in details {
            let _ = write!(rendered, "\n{detail}");
        }
    }

    Some(rendered)
}

pub fn run_check(root: &Path, include_globs: &[String], mode: CheckMode) -> CheckOutcome {
    run_check_with_scan_settings(
        root,
        &ScanSettings {
            include: include_globs.to_vec(),
            exclude: Vec::new(),
        },
        mode,
    )
}

pub fn run_check_with_scan_settings(
    root: &Path,
    scan_settings: &ScanSettings,
    mode: CheckMode,
) -> CheckOutcome {
    run_check_with_settings(
        root,
        &AnalysisSettings {
            scan: scan_settings.clone(),
            profile: None,
        },
        mode,
    )
}

pub fn run_check_with_settings(
    root: &Path,
    settings: &AnalysisSettings,
    mode: CheckMode,
) -> CheckOutcome {
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

    let report = analyze_workspace_with_settings(root, settings);
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
                diagnostics: vec![Diagnostic::error(
                    Some(path.to_path_buf()),
                    None,
                    format!("failed to parse rust file: {err}"),
                )],
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
    analyze_workspace_with_scan_settings(
        root,
        &ScanSettings {
            include: include_globs.to_vec(),
            exclude: Vec::new(),
        },
    )
}

pub fn analyze_workspace_with_scan_settings(
    root: &Path,
    cli_scan_settings: &ScanSettings,
) -> WorkspaceReport {
    analyze_workspace_with_settings(
        root,
        &AnalysisSettings {
            scan: cli_scan_settings.clone(),
            profile: None,
        },
    )
}

pub fn analyze_workspace_with_settings(
    root: &Path,
    cli_settings: &AnalysisSettings,
) -> WorkspaceReport {
    let mut diagnostics = Vec::new();
    let workspace_defaults = load_workspace_settings(root, &mut diagnostics);
    let repo_profile = load_repo_profile(root, &mut diagnostics);
    let repo_scan_settings = load_repo_scan_settings(root, &mut diagnostics);
    let effective_scan_settings = effective_scan_settings(&repo_scan_settings, &cli_settings.scan);
    let rust_files = match collect_rust_files(
        root,
        &effective_scan_settings.include,
        &effective_scan_settings.exclude,
    ) {
        Ok(files) => files,
        Err(err) => {
            diagnostics.push(Diagnostic::error(
                None,
                None,
                format!("failed to discover rust files: {err}"),
            ));
            return WorkspaceReport {
                scanned_files: 0,
                files_with_violations: 0,
                diagnostics,
            };
        }
    };

    if rust_files.is_empty() {
        diagnostics.push(Diagnostic::warning(
            None,
            None,
            "no Rust files were discovered; pass --include <path>... or run from a crate/workspace root",
        ));
    }

    let mut files_with_violations = BTreeSet::new();
    let mut package_cache = BTreeMap::new();

    for file in &rust_files {
        let src = match fs::read_to_string(file) {
            Ok(src) => src,
            Err(err) => {
                diagnostics.push(Diagnostic::error(
                    Some(file.clone()),
                    None,
                    format!("failed to read file: {err}"),
                ));
                continue;
            }
        };

        let settings = settings_for_file(
            root,
            file,
            &workspace_defaults,
            repo_profile,
            cli_settings.profile,
            &mut package_cache,
            &mut diagnostics,
        );
        let mut analysis = analyze_file_with_settings(file, &src, &settings.namespace);
        analysis
            .diagnostics
            .retain(|diag| diag.included_in_profile(settings.profile));
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

fn effective_scan_settings(
    repo_defaults: &ScanSettings,
    cli_overrides: &ScanSettings,
) -> ScanSettings {
    let include = if cli_overrides.include.is_empty() {
        repo_defaults.include.clone()
    } else {
        cli_overrides.include.clone()
    };
    let mut exclude = repo_defaults.exclude.clone();
    exclude.extend(cli_overrides.exclude.iter().cloned());
    ScanSettings { include, exclude }
}

fn load_workspace_settings(root: &Path, diagnostics: &mut Vec<Diagnostic>) -> NamespaceSettings {
    let manifest_path = root.join("Cargo.toml");
    let Ok(manifest_src) = fs::read_to_string(&manifest_path) else {
        return NamespaceSettings::default();
    };

    let manifest: toml::Value = match toml::from_str(&manifest_src) {
        Ok(manifest) => manifest,
        Err(err) => {
            diagnostics.push(Diagnostic::error(
                Some(manifest_path),
                None,
                format!("failed to parse Cargo.toml for modum settings: {err}"),
            ));
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
        &NamespaceSettings::default(),
        &manifest_path,
        diagnostics,
    )
    .unwrap_or_default()
}

fn load_repo_scan_settings(root: &Path, diagnostics: &mut Vec<Diagnostic>) -> ScanSettings {
    let manifest_path = root.join("Cargo.toml");
    let Ok(manifest_src) = fs::read_to_string(&manifest_path) else {
        return ScanSettings::default();
    };

    let manifest: toml::Value = match toml::from_str(&manifest_src) {
        Ok(manifest) => manifest,
        Err(err) => {
            diagnostics.push(Diagnostic::error(
                Some(manifest_path),
                None,
                format!("failed to parse Cargo.toml for modum settings: {err}"),
            ));
            return ScanSettings::default();
        }
    };

    parse_scan_settings_from_manifest(
        manifest
            .get("workspace")
            .and_then(toml::Value::as_table)
            .and_then(|workspace| workspace.get("metadata"))
            .and_then(toml::Value::as_table)
            .and_then(|metadata| metadata.get("modum"))
            .or_else(|| {
                manifest
                    .get("package")
                    .and_then(toml::Value::as_table)
                    .and_then(|package| package.get("metadata"))
                    .and_then(toml::Value::as_table)
                    .and_then(|metadata| metadata.get("modum"))
            }),
        &manifest_path,
        diagnostics,
    )
    .unwrap_or_default()
}

fn load_repo_profile(root: &Path, diagnostics: &mut Vec<Diagnostic>) -> Option<LintProfile> {
    let manifest_path = root.join("Cargo.toml");
    let Ok(manifest_src) = fs::read_to_string(&manifest_path) else {
        return None;
    };

    let manifest: toml::Value = match toml::from_str(&manifest_src) {
        Ok(manifest) => manifest,
        Err(err) => {
            diagnostics.push(Diagnostic::error(
                Some(manifest_path),
                None,
                format!("failed to parse Cargo.toml for modum settings: {err}"),
            ));
            return None;
        }
    };

    parse_profile_from_manifest(
        manifest
            .get("workspace")
            .and_then(toml::Value::as_table)
            .and_then(|workspace| workspace.get("metadata"))
            .and_then(toml::Value::as_table)
            .and_then(|metadata| metadata.get("modum"))
            .or_else(|| {
                manifest
                    .get("package")
                    .and_then(toml::Value::as_table)
                    .and_then(|package| package.get("metadata"))
                    .and_then(toml::Value::as_table)
                    .and_then(|metadata| metadata.get("modum"))
            }),
        &manifest_path,
        diagnostics,
    )
}

fn settings_for_file(
    root: &Path,
    file: &Path,
    workspace_defaults: &NamespaceSettings,
    repo_profile: Option<LintProfile>,
    cli_profile: Option<LintProfile>,
    cache: &mut BTreeMap<PathBuf, PackageSettings>,
    diagnostics: &mut Vec<Diagnostic>,
) -> FileSettings {
    let Some(package_root) = find_package_root(root, file) else {
        return FileSettings {
            namespace: workspace_defaults.clone(),
            profile: resolve_profile(cli_profile, None, repo_profile),
        };
    };

    if let Some(settings) = cache.get(&package_root) {
        return FileSettings {
            namespace: settings.namespace.clone(),
            profile: resolve_profile(cli_profile, settings.profile, repo_profile),
        };
    }

    let settings = load_package_settings(&package_root, workspace_defaults, diagnostics);
    cache.insert(package_root, settings.clone());
    FileSettings {
        namespace: settings.namespace,
        profile: resolve_profile(cli_profile, settings.profile, repo_profile),
    }
}

fn load_package_settings(
    root: &Path,
    workspace_defaults: &NamespaceSettings,
    diagnostics: &mut Vec<Diagnostic>,
) -> PackageSettings {
    let manifest_path = root.join("Cargo.toml");
    let Ok(manifest_src) = fs::read_to_string(&manifest_path) else {
        return PackageSettings {
            namespace: workspace_defaults.clone(),
            profile: None,
        };
    };

    let manifest = match toml::from_str::<toml::Value>(&manifest_src) {
        Ok(manifest) => manifest,
        Err(err) => {
            diagnostics.push(Diagnostic::error(
                Some(manifest_path),
                None,
                format!("failed to parse Cargo.toml for modum settings: {err}"),
            ));
            return PackageSettings {
                namespace: workspace_defaults.clone(),
                profile: None,
            };
        }
    };

    let metadata = manifest
        .get("package")
        .and_then(toml::Value::as_table)
        .and_then(|package| package.get("metadata"))
        .and_then(toml::Value::as_table)
        .and_then(|metadata| metadata.get("modum"));

    let namespace =
        parse_settings_from_manifest(metadata, workspace_defaults, &manifest_path, diagnostics)
            .unwrap_or_else(|| workspace_defaults.clone());
    let profile = parse_profile_from_manifest(metadata, &manifest_path, diagnostics);

    PackageSettings { namespace, profile }
}

fn resolve_profile(
    cli_profile: Option<LintProfile>,
    package_profile: Option<LintProfile>,
    repo_profile: Option<LintProfile>,
) -> LintProfile {
    cli_profile
        .or(package_profile)
        .or(repo_profile)
        .unwrap_or_default()
}

fn parse_settings_from_manifest(
    value: Option<&toml::Value>,
    base: &NamespaceSettings,
    manifest_path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<NamespaceSettings> {
    let table = value?.as_table()?;
    let mut settings = base.clone();

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
    apply_token_family_overrides(
        &mut settings.semantic_string_scalars,
        table,
        "extra_semantic_string_scalars",
        "ignored_semantic_string_scalars",
        manifest_path,
        diagnostics,
    );
    apply_token_family_overrides(
        &mut settings.semantic_numeric_scalars,
        table,
        "extra_semantic_numeric_scalars",
        "ignored_semantic_numeric_scalars",
        manifest_path,
        diagnostics,
    );
    apply_token_family_overrides(
        &mut settings.key_value_bag_names,
        table,
        "extra_key_value_bag_names",
        "ignored_key_value_bag_names",
        manifest_path,
        diagnostics,
    );

    Some(settings)
}

fn parse_scan_settings_from_manifest(
    value: Option<&toml::Value>,
    manifest_path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ScanSettings> {
    let table = value?.as_table()?;
    let mut settings = ScanSettings::default();

    if let Some(values) = parse_string_list_field(table, "include", manifest_path, diagnostics) {
        settings.include = values;
    }
    if let Some(values) = parse_string_list_field(table, "exclude", manifest_path, diagnostics) {
        settings.exclude = values;
    }

    Some(settings)
}

fn parse_profile_from_manifest(
    value: Option<&toml::Value>,
    manifest_path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<LintProfile> {
    let table = value?.as_table()?;
    let raw = parse_string_field(table, "profile", manifest_path, diagnostics)?;
    match raw.parse() {
        Ok(profile) => Some(profile),
        Err(err) => {
            diagnostics.push(Diagnostic::error(
                Some(manifest_path.to_path_buf()),
                None,
                format!("`metadata.modum.profile` {err}"),
            ));
            None
        }
    }
}

fn parse_string_field(
    table: &toml::value::Table,
    key: &str,
    manifest_path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    let value = table.get(key)?;
    let Some(value) = value.as_str() else {
        diagnostics.push(Diagnostic::error(
            Some(manifest_path.to_path_buf()),
            None,
            format!("`metadata.modum.{key}` must be a string"),
        ));
        return None;
    };

    Some(value.to_string())
}

fn parse_string_set_field(
    table: &toml::value::Table,
    key: &str,
    manifest_path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<BTreeSet<String>> {
    Some(
        parse_string_values_field(table, key, manifest_path, diagnostics)?
            .into_iter()
            .collect(),
    )
}

fn parse_string_list_field(
    table: &toml::value::Table,
    key: &str,
    manifest_path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<String>> {
    parse_string_values_field(table, key, manifest_path, diagnostics)
}

fn parse_string_values_field(
    table: &toml::value::Table,
    key: &str,
    manifest_path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<String>> {
    let value = table.get(key)?;
    let Some(array) = value.as_array() else {
        diagnostics.push(Diagnostic::error(
            Some(manifest_path.to_path_buf()),
            None,
            format!("`metadata.modum.{key}` must be an array of strings"),
        ));
        return None;
    };

    let mut values = Vec::with_capacity(array.len());
    for (index, value) in array.iter().enumerate() {
        let Some(value) = value.as_str() else {
            diagnostics.push(Diagnostic::error(
                Some(manifest_path.to_path_buf()),
                None,
                format!("`metadata.modum.{key}[{index}]` must be a string"),
            ));
            return None;
        };
        values.push(value.to_string());
    }

    Some(values)
}

fn parse_normalized_string_set_field(
    table: &toml::value::Table,
    key: &str,
    manifest_path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<BTreeSet<String>> {
    Some(
        parse_string_values_field(table, key, manifest_path, diagnostics)?
            .into_iter()
            .map(|value| normalize_segment(&value))
            .collect(),
    )
}

fn apply_token_family_overrides(
    target: &mut BTreeSet<String>,
    table: &toml::value::Table,
    extra_key: &str,
    ignored_key: &str,
    manifest_path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(values) =
        parse_normalized_string_set_field(table, extra_key, manifest_path, diagnostics)
    {
        target.extend(values);
    }
    if let Some(values) =
        parse_normalized_string_set_field(table, ignored_key, manifest_path, diagnostics)
    {
        for value in values {
            target.remove(&value);
        }
    }
}

fn diagnostic_explanation_details(code: &str) -> Option<&'static [&'static str]> {
    match code {
        "namespace_prelude_glob_import" => Some(&[
            "why: prelude globs make it harder to see which module gives a name its meaning.",
            "typical fixes: import the specific items you need or keep the preserving module visible at call sites.",
        ]),
        "namespace_glob_preserve_module" => Some(&[
            "why: broad globs from modules like `http`, `error`, or `query` erase context that often helps readers scan call sites.",
            "typical fixes: import only the concrete items you need or keep the module qualifier in local code.",
        ]),
        "api_anyhow_error_surface" => Some(&[
            "why: `anyhow` works well internally, but caller-facing boundaries usually read better when the crate owns the error type and variants.",
            "typical fixes: return a crate-owned error enum or newtype and convert internal failures into that boundary type.",
        ]),
        "api_string_error_surface" => Some(&[
            "why: raw string errors lose structure, variant names, and machine-readable context at the boundary.",
            "typical fixes: model the boundary error as an enum, a focused struct, or another typed error value with named data.",
        ]),
        "api_semantic_string_scalar" => Some(&[
            "why: names like `email`, `url`, `path`, or `locale` usually carry domain rules that a plain `String` cannot express.",
            "typical fixes: parse at the boundary into a domain newtype or another focused typed value.",
            "repo tuning: use `metadata.modum.extra_semantic_string_scalars` or `metadata.modum.ignored_semantic_string_scalars` to adjust the token family.",
        ]),
        "api_semantic_numeric_scalar" => Some(&[
            "why: names like `duration`, `timestamp`, or `port` often want units or domain semantics, not a bare integer.",
            "typical fixes: use a typed duration, timestamp, port, or small domain newtype at the boundary.",
            "repo tuning: use `metadata.modum.extra_semantic_numeric_scalars` or `metadata.modum.ignored_semantic_numeric_scalars` to adjust the token family.",
        ]),
        "api_raw_key_value_bag" => Some(&[
            "why: bags like `metadata`, `headers`, or `params` often accrete hidden contracts that are easier to understand once they are typed.",
            "typical fixes: introduce a focused options struct, metadata type, or dedicated collection wrapper.",
            "repo tuning: use `metadata.modum.extra_key_value_bag_names` or `metadata.modum.ignored_key_value_bag_names` to adjust the token family.",
        ]),
        "api_boolean_flag_cluster" => Some(&[
            "why: several booleans together usually encode modes or policy choices that are easier to name explicitly.",
            "typical fixes: group the behavior into a typed options struct, an enum, or a smaller decision object.",
        ]),
        "api_manual_flag_set" => Some(&[
            "why: parallel flag constants and repeated raw bitmask checks usually mean the boundary is modeling a flags type by hand.",
            "typical fixes: introduce a focused typed flags surface or small domain wrapper instead of exposing raw integer masks.",
        ]),
        "api_raw_id_surface" => Some(&[
            "why: ids often carry validation, formatting, or cross-system meaning that is easy to lose when they stay as bare strings or integers.",
            "typical fixes: introduce a small id newtype and parse or validate at the boundary.",
        ]),
        _ => None,
    }
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
    render_pretty_report_with_selection(report, DiagnosticSelection::All)
}

pub fn render_pretty_report_with_selection(
    report: &WorkspaceReport,
    selection: DiagnosticSelection,
) -> String {
    let filtered = report.filtered(selection);
    let mut out = String::new();

    let _ = writeln!(&mut out, "modum lint report");
    let _ = writeln!(&mut out, "files scanned: {}", filtered.scanned_files);
    let _ = writeln!(
        &mut out,
        "files with violations: {}",
        filtered.files_with_violations
    );
    let _ = writeln!(
        &mut out,
        "diagnostics: {} error(s), {} policy warning(s), {} advisory warning(s)",
        filtered.error_count(),
        filtered.policy_warning_count(),
        filtered.advisory_warning_count()
    );
    if let Some(selection_label) = selection.report_label() {
        let _ = writeln!(
            &mut out,
            "showing: {selection_label} (exit code still reflects the full report)"
        );
    }
    if filtered.policy_violation_count() > 0 {
        let _ = writeln!(
            &mut out,
            "policy violations: {}",
            filtered.policy_violation_count()
        );
    }
    if filtered.advisory_warning_count() > 0 {
        let _ = writeln!(
            &mut out,
            "advisories: {}",
            filtered.advisory_warning_count()
        );
    }

    if !filtered.diagnostics.is_empty() {
        let _ = writeln!(&mut out);
        render_diagnostic_section(
            &mut out,
            "Errors:",
            filtered.diagnostics.iter().filter(|diag| diag.is_error()),
        );
        render_diagnostic_section(
            &mut out,
            "Policy Diagnostics:",
            filtered
                .diagnostics
                .iter()
                .filter(|diag| diag.is_policy_warning()),
        );
        render_diagnostic_section(
            &mut out,
            "Advisory Diagnostics:",
            filtered
                .diagnostics
                .iter()
                .filter(|diag| diag.is_advisory_warning()),
        );
    }

    out
}

fn render_diagnostic_section<'a>(
    out: &mut String,
    title: &str,
    diagnostics: impl Iterator<Item = &'a Diagnostic>,
) {
    let diagnostics = diagnostics.collect::<Vec<_>>();
    if diagnostics.is_empty() {
        return;
    }

    let _ = writeln!(out, "{title}");
    for diag in diagnostics {
        let level = match diag.level() {
            DiagnosticLevel::Warning => "warning",
            DiagnosticLevel::Error => "error",
        };
        let code = match (diag.code(), diag.profile()) {
            (Some(code), Some(profile)) => format!(" ({code}, {})", profile.as_str()),
            (Some(code), None) => format!(" ({code})"),
            (None, _) => String::new(),
        };
        let fix = diag
            .fix
            .as_ref()
            .map(|fix| format!(" [fix: {}]", fix.replacement))
            .unwrap_or_default();
        match (&diag.file, diag.line) {
            (Some(file), Some(line)) => {
                let _ = writeln!(
                    out,
                    "- [{level}{code}] {}:{line}: {}{fix}",
                    file.display(),
                    diag.message
                );
            }
            (Some(file), None) => {
                let _ = writeln!(
                    out,
                    "- [{level}{code}] {}: {}{fix}",
                    file.display(),
                    diag.message
                );
            }
            (None, _) => {
                let _ = writeln!(out, "- [{level}{code}] {}{fix}", diag.message);
            }
        }
    }
    let _ = writeln!(out);
}

fn collect_rust_files(
    root: &Path,
    include_globs: &[String],
    exclude_globs: &[String],
) -> io::Result<Vec<PathBuf>> {
    let mut files = BTreeSet::new();
    if include_globs.is_empty() {
        for scan_root in collect_default_scan_roots(root)? {
            collect_rust_files_in_dir(&scan_root, &mut files);
        }
    } else {
        for entry in include_globs {
            collect_rust_files_for_entry(root, entry, &mut files)?;
        }
    }

    let mut filtered = Vec::with_capacity(files.len());
    for path in files {
        if !is_excluded_path(root, &path, exclude_globs)? {
            filtered.push(path);
        }
    }

    Ok(filtered)
}

fn collect_rust_files_for_entry(
    root: &Path,
    entry: &str,
    files: &mut BTreeSet<PathBuf>,
) -> io::Result<()> {
    let candidate = root.join(entry);
    if !contains_glob_meta(entry) {
        if candidate.is_file() && is_rust_file(&candidate) {
            files.insert(candidate);
        } else if candidate.is_dir() {
            collect_rust_files_in_dir(&candidate, files);
        }
        return Ok(());
    }

    let escaped_root = Pattern::escape(&root.to_string_lossy());
    let normalized_pattern = entry.replace('\\', "/");
    let full_pattern = format!("{escaped_root}/{normalized_pattern}");
    let matches = glob(&full_pattern).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid include pattern `{entry}`: {err}"),
        )
    })?;

    for matched in matches {
        let path = matched
            .map_err(|err| io::Error::other(format!("failed to expand `{entry}`: {err}")))?;
        if path.is_file() && is_rust_file(&path) {
            files.insert(path);
        } else if path.is_dir() {
            collect_rust_files_in_dir(&path, files);
        }
    }

    Ok(())
}

fn is_excluded_path(root: &Path, path: &Path, exclude_globs: &[String]) -> io::Result<bool> {
    if exclude_globs.is_empty() {
        return Ok(false);
    }

    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    for pattern in exclude_globs {
        if contains_glob_meta(pattern) {
            let matcher = Pattern::new(pattern).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("invalid exclude pattern `{pattern}`: {err}"),
                )
            })?;
            if matcher.matches(&relative) {
                return Ok(true);
            }
            continue;
        }

        let normalized = pattern.trim_end_matches('/').replace('\\', "/");
        if relative == normalized || relative.starts_with(&format!("{normalized}/")) {
            return Ok(true);
        }
    }
    Ok(false)
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

#[derive(Clone, Copy)]
pub(crate) enum NameStyle {
    Pascal,
    Snake,
    ScreamingSnake,
}

pub(crate) fn detect_name_style(name: &str) -> NameStyle {
    if name.contains('_') {
        if name
            .chars()
            .filter(|ch| ch.is_ascii_alphabetic())
            .all(|ch| ch.is_ascii_uppercase())
        {
            NameStyle::ScreamingSnake
        } else {
            NameStyle::Snake
        }
    } else {
        NameStyle::Pascal
    }
}

pub(crate) fn render_segments(segments: &[String], style: NameStyle) -> String {
    match style {
        NameStyle::Pascal => segments
            .iter()
            .map(|segment| {
                let lower = segment.to_ascii_lowercase();
                let mut chars = lower.chars();
                let Some(first) = chars.next() else {
                    return String::new();
                };
                let mut rendered = String::new();
                rendered.push(first.to_ascii_uppercase());
                rendered.extend(chars);
                rendered
            })
            .collect::<Vec<_>>()
            .join(""),
        NameStyle::Snake => segments
            .iter()
            .map(|segment| segment.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join("_"),
        NameStyle::ScreamingSnake => segments
            .iter()
            .map(|segment| segment.to_ascii_uppercase())
            .collect::<Vec<_>>()
            .join("_"),
    }
}

pub(crate) fn inferred_file_module_path(path: &Path) -> Vec<String> {
    let components = path
        .iter()
        .map(|component| component.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let rel = if let Some(src_idx) = components.iter().rposition(|component| component == "src") {
        &components[src_idx + 1..]
    } else {
        &components[..]
    };

    if rel.is_empty() || rel.first().is_some_and(|component| component == "bin") {
        return Vec::new();
    }

    let mut module_path = Vec::new();
    for (idx, component) in rel.iter().enumerate() {
        let is_last = idx + 1 == rel.len();
        if is_last {
            match component.as_str() {
                "lib.rs" | "main.rs" | "mod.rs" => {}
                other => {
                    if let Some(stem) = other.strip_suffix(".rs") {
                        module_path.push(stem.to_string());
                    }
                }
            }
            continue;
        }

        module_path.push(component.to_string());
    }

    module_path
}

pub(crate) fn source_root(path: &Path) -> Option<PathBuf> {
    let mut root = PathBuf::new();
    for component in path.components() {
        root.push(component.as_os_str());
        if component.as_os_str() == "src" {
            return Some(root);
        }
    }
    None
}

pub(crate) fn parent_module_files(src_root: &Path, prefix: &[String]) -> Vec<PathBuf> {
    if prefix.is_empty() {
        return vec![src_root.join("lib.rs"), src_root.join("main.rs")];
    }

    let joined = prefix.join("/");
    vec![
        src_root.join(format!("{joined}.rs")),
        src_root.join(joined).join("mod.rs"),
    ]
}

pub(crate) fn replace_path_fix(replacement: impl Into<String>) -> DiagnosticFix {
    DiagnosticFix {
        kind: DiagnosticFixKind::ReplacePath,
        replacement: replacement.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CheckMode, Diagnostic, DiagnosticSelection, LintProfile, NamespaceSettings,
        WorkspaceReport, check_exit_code, parse_check_mode, parse_lint_profile, split_segments,
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
    fn check_mode_supports_standard_parsing() {
        assert_eq!("off".parse::<CheckMode>(), Ok(CheckMode::Off));
        assert_eq!("warn".parse::<CheckMode>(), Ok(CheckMode::Warn));
        assert_eq!("deny".parse::<CheckMode>(), Ok(CheckMode::Deny));
    }

    #[test]
    fn rejects_invalid_check_mode() {
        let err = parse_check_mode("strict").unwrap_err();
        assert!(err.contains("expected off|warn|deny"));
    }

    #[test]
    fn lint_profile_supports_standard_parsing() {
        assert_eq!(parse_lint_profile("core"), Ok(LintProfile::Core));
        assert_eq!(parse_lint_profile("surface"), Ok(LintProfile::Surface));
        assert_eq!(parse_lint_profile("strict"), Ok(LintProfile::Strict));
    }

    #[test]
    fn rejects_invalid_lint_profile() {
        let err = parse_lint_profile("default").unwrap_err();
        assert!(err.contains("expected core|surface|strict"));
    }

    #[test]
    fn diagnostic_selection_supports_standard_parsing() {
        assert_eq!(
            "all".parse::<DiagnosticSelection>(),
            Ok(DiagnosticSelection::All)
        );
        assert_eq!(
            "policy".parse::<DiagnosticSelection>(),
            Ok(DiagnosticSelection::Policy)
        );
        assert_eq!(
            "advisory".parse::<DiagnosticSelection>(),
            Ok(DiagnosticSelection::Advisory)
        );
    }

    #[test]
    fn rejects_invalid_diagnostic_selection() {
        let err = "warnings".parse::<DiagnosticSelection>().unwrap_err();
        assert!(err.contains("expected all|policy|advisory"));
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
            diagnostics: vec![Diagnostic::policy(None, None, "lint", "warning")],
        };
        assert_eq!(check_exit_code(&with_policy, CheckMode::Warn), 0);
        assert_eq!(check_exit_code(&with_policy, CheckMode::Deny), 2);

        let with_error = WorkspaceReport {
            scanned_files: 1,
            files_with_violations: 1,
            diagnostics: vec![Diagnostic::error(None, None, "error")],
        };
        assert_eq!(check_exit_code(&with_error, CheckMode::Warn), 1);
        assert_eq!(check_exit_code(&with_error, CheckMode::Deny), 1);
    }

    #[test]
    fn namespace_settings_defaults_cover_generic_nouns_and_weak_modules() {
        let settings = NamespaceSettings::default();
        assert!(settings.generic_nouns.contains("Repository"));
        assert!(settings.generic_nouns.contains("Id"));
        assert!(settings.generic_nouns.contains("Outcome"));
        assert!(settings.weak_modules.contains("storage"));
        assert!(settings.catch_all_modules.contains("helpers"));
        assert!(settings.organizational_modules.contains("error"));
        assert!(settings.organizational_modules.contains("request"));
        assert!(settings.organizational_modules.contains("response"));
        assert!(settings.namespace_preserving_modules.contains("email"));
        assert!(settings.namespace_preserving_modules.contains("components"));
        assert!(settings.namespace_preserving_modules.contains("partials"));
        assert!(settings.namespace_preserving_modules.contains("trace"));
        assert!(settings.namespace_preserving_modules.contains("write_back"));
        assert!(!settings.namespace_preserving_modules.contains("views"));
        assert!(!settings.namespace_preserving_modules.contains("handlers"));
    }

    #[test]
    fn workspace_report_can_filter_policy_and_advisory_diagnostics() {
        let report = WorkspaceReport {
            scanned_files: 2,
            files_with_violations: 2,
            diagnostics: vec![
                Diagnostic::policy(Some("src/policy.rs".into()), Some(1), "policy", "policy"),
                Diagnostic::advisory(
                    Some("src/advisory.rs".into()),
                    Some(2),
                    "advisory",
                    "advisory",
                ),
                Diagnostic::error(Some("src/error.rs".into()), Some(3), "error"),
            ],
        };

        let policy_only = report.filtered(DiagnosticSelection::Policy);
        assert_eq!(policy_only.files_with_violations, 2);
        assert_eq!(policy_only.error_count(), 1);
        assert_eq!(policy_only.policy_warning_count(), 1);
        assert_eq!(policy_only.advisory_warning_count(), 0);

        let advisory_only = report.filtered(DiagnosticSelection::Advisory);
        assert_eq!(advisory_only.files_with_violations, 2);
        assert_eq!(advisory_only.error_count(), 1);
        assert_eq!(advisory_only.policy_warning_count(), 0);
        assert_eq!(advisory_only.advisory_warning_count(), 1);
    }

    #[test]
    fn workspace_report_can_filter_diagnostics_by_profile() {
        let report = WorkspaceReport {
            scanned_files: 3,
            files_with_violations: 3,
            diagnostics: vec![
                Diagnostic::policy(
                    Some("src/core.rs".into()),
                    Some(1),
                    "namespace_flat_use",
                    "core",
                ),
                Diagnostic::policy(
                    Some("src/surface.rs".into()),
                    Some(2),
                    "api_missing_parent_surface_export",
                    "surface",
                ),
                Diagnostic::advisory(
                    Some("src/strict.rs".into()),
                    Some(3),
                    "api_candidate_semantic_module",
                    "strict",
                ),
                Diagnostic::error(Some("Cargo.toml".into()), None, "config"),
            ],
        };

        let core = report.filtered_by_profile(LintProfile::Core);
        assert_eq!(core.files_with_violations, 2);
        assert_eq!(core.diagnostics.len(), 2);
        assert!(
            core.diagnostics
                .iter()
                .any(|diag| diag.code() == Some("namespace_flat_use"))
        );
        assert!(core.diagnostics.iter().any(|diag| diag.code().is_none()));

        let surface = report.filtered_by_profile(LintProfile::Surface);
        assert_eq!(surface.files_with_violations, 3);
        assert_eq!(surface.diagnostics.len(), 3);
        assert!(
            surface
                .diagnostics
                .iter()
                .any(|diag| diag.code() == Some("api_missing_parent_surface_export"))
        );
        assert!(
            !surface
                .diagnostics
                .iter()
                .any(|diag| diag.code() == Some("api_candidate_semantic_module"))
        );
    }
}
