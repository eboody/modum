<div align="center">
  <img alt="modum logo" src="https://raw.githubusercontent.com/eboody/modum/main/modum-logo.svg" width="360">
  <p>Modum checks source-level namespace shape, alias hygiene, module naming, and surface-path heuristics across a Rust workspace.</p>
  <p>
    <a href="https://github.com/eboody/modum/actions/workflows/ci.yml"><img src="https://github.com/eboody/modum/actions/workflows/ci.yml/badge.svg?branch=main&event=push" alt="build status" /></a>
    <a href="https://crates.io/crates/modum"><img src="https://img.shields.io/crates/v/modum.svg?logo=rust" alt="crates.io" /></a>
    <a href="https://docs.rs/modum"><img src="https://docs.rs/modum/badge.svg" alt="docs.rs" /></a>
  </p>
</div>

# modum

It is a lint tool. It reports diagnostics. It does not rewrite code.
It analyzes parsed Rust source files. It does not expand macros, resolve `include!`, or prune `#[cfg]`.

## Why It Exists

`modum` exists to catch two common Rust namespace-shape problems:

- flattened imports that hide useful context at call sites
- redundant leaf names that repeat context the module path already provides

These call-site shapes are legal Rust, but they make signatures and public paths harder to scan:

```rust
pub fn handle(repo: UserRepository) -> Result<StatusCode, Error> {
    todo!()
}
```

These usually read more clearly:

```rust
pub fn handle(repo: user::Repository) -> Result<http::StatusCode, partials::Error> {
    todo!()
}
```

The same pattern shows up in public API paths:

```rust
user::Repository
user::error::InvalidEmail
partials::Error
```

instead of:

```rust
user::UserRepository
user::error::InvalidEmailError
partials::error::Error
```

The central comparison is often this:

```rust
user::Repository
UserRepository
```

In the abstract, `user::Repository` is usually better than `UserRepository`.

Why:

- The domain lives in the path, which is where Rust already gives you structure.
- The leaf can stay generic and composable: `user::Repository`, `user::Service`, `user::Id`.
- It scales better across a crate than baking the domain into every type name.

That is also why `user::Repository` is usually better than `user::UserRepository`: once the path is doing the domain work, the leaf does not need to repeat it.

The main caveat is that this only holds when `user` is a real semantic module. If the parent path is weak or technical, then a longer leaf can still be better. `UserRepository` is often clearer than `storage::Repository`.

So the rule is:

- strong semantic parent: prefer `user::Repository`
- weak or technical parent: prefer the more descriptive leaf

`modum` checks that style across an entire workspace at the parsed-source level.

## Observation Model

`modum` reads Rust source files with `syn` and reports source-level heuristics from the parsed AST.

It does not observe:

- cfg-pruned items
- macro-expanded items
- `include!`-generated items

When semantic-module family inference would depend on those constructs, `modum` skips `api_candidate_semantic_module` and emits `api_candidate_semantic_module_unsupported_construct` instead.

## Mental Model

`modum` follows four rules:

1. Keep namespace context visible at call sites.
2. Prefer a strong semantic parent with a short leaf: `user::Repository` over `UserRepository`.
3. Keep a more descriptive leaf when the parent path is weak or technical.
4. Use modules for domain boundaries, not file organization.

## Quick Usage

```bash
cargo install modum
cargo modum check --root .
cargo modum check --root . --mode warn
cargo modum check --root . --profile core
cargo modum --explain namespace_flat_use
cargo modum check --root . --show advisory
cargo modum check --root . --ignore api_candidate_semantic_module
cargo modum check --root . --write-baseline .modum-baseline.json
cargo modum check --root . --baseline .modum-baseline.json
cargo modum check --root . --exclude examples/high-coverage
cargo modum check --root . --format json
```

`cargo install modum` installs both `modum` and the Cargo subcommand `cargo-modum`, so either of these is valid:

```bash
modum check --root .
cargo modum check --root .
```

### Neovim

`modum` works well with `nvim-lint`. Use `--mode warn` so diagnostics do not fail the editor job, and use `--format json` for stable parsing.

```lua
local lint = require("lint")

lint.linters.modum = {
  cmd = "modum",
  stdin = false,
  stream = "stdout",
  args = { "check", "--root", vim.fn.getcwd(), "--mode", "warn", "--format", "json" },
  parser = function(output, bufnr)
    if output == "" then
      return {}
    end

    local decoded = vim.json.decode(output)
    local current_file = vim.api.nvim_buf_get_name(bufnr)
    local diagnostics = {}

    for _, item in ipairs(((decoded or {}).report or {}).diagnostics or {}) do
      if item.file == current_file then
        diagnostics[#diagnostics + 1] = {
          bufnr = bufnr,
          lnum = math.max((item.line or 1) - 1, 0),
          col = 0,
          severity = item.level == "Error"
            and vim.diagnostic.severity.ERROR
            or vim.diagnostic.severity.WARN,
          source = "modum",
          code = item.code,
          message = item.message,
        }
      end
    end

    return diagnostics
  end,
}

lint.linters_by_ft.rust = { "modum" }
```

If you edit multiple crates from one Neovim session, replace `vim.fn.getcwd()` with your workspace root resolver. `modum` is workspace-oriented, so it is usually better to run it on save than on every `InsertLeave`.

If you are developing `modum` itself:

```bash
cargo run -p modum -- check --root .
```

Environment:

```bash
MODUM=off|warn|deny
```

Default mode is `deny`.

## Output

Text output groups diagnostics into `Errors`, `Policy Diagnostics`, and `Advisory Diagnostics`.

Use `--show policy` or `--show advisory` when you want to focus one side of the report without changing exit behavior. The exit code still reflects the full report.

Use `--profile core`, `--profile surface`, or `--profile strict` to choose how opinionated the lint set should be. `strict` is the default.

Use `--ignore <code>` for one-off opt-outs in local runs, and `--write-baseline <path>` plus `--baseline <path>` when you want to ratchet down an existing repo without fixing every warning at once.

Text output includes the diagnostic code profile, and direct rewrite-style fixes show a short `fix:` hint inline.

JSON output keeps the full diagnostic list and includes:

- `profile`: the minimum lint profile that includes the diagnostic
- `policy`: whether the diagnostic counts as a policy violation
- `fix`: optional autofix metadata when the rewrite is a direct path replacement, such as `response::Response` to `Response`

You can explain any code without running analysis:

```bash
modum --explain namespace_flat_use
cargo modum --explain api_candidate_semantic_module
```

## CI Usage

Use `modum` the same way you would use `clippy` or `cargo-deny`: run it as a normal command in CI, not from `build.rs`.

```yaml
- run: cargo install modum
- run: cargo modum check --root .
```

For large repos that are adopting `modum` incrementally:

```yaml
- run: cargo install modum
- run: cargo modum check --root . --baseline .modum-baseline.json
```

## Exit Behavior

- `0`: clean, or warnings allowed via `--mode warn`
- `2`: warning-level policy violations found in `deny` mode
- `1`: hard errors, including parse/configuration failures and error-level policy violations such as `api_organizational_submodule_flatten`

## Configuration

Configure the lints in any workspace with Cargo metadata:

```toml
[workspace.metadata.modum]
profile = "strict"
include = ["src", "crates/*/src"]
exclude = ["examples/high-coverage"]
generic_nouns = ["Id", "Repository", "Service", "Error", "Command", "Request", "Response", "Outcome"]
weak_modules = ["storage", "transport", "infra", "common", "misc", "helpers", "helper", "types", "util", "utils"]
catch_all_modules = ["common", "misc", "helpers", "helper", "types", "util", "utils"]
organizational_modules = ["error", "errors", "request", "response"]
namespace_preserving_modules = ["auth", "command", "components", "email", "error", "http", "page", "partials", "policy", "query", "repo", "store", "storage", "transport", "infra"]
extra_namespace_preserving_modules = ["widgets"]
ignored_namespace_preserving_modules = ["components"]
extra_semantic_string_scalars = ["mime"]
ignored_semantic_string_scalars = ["url"]
extra_semantic_numeric_scalars = ["epoch"]
ignored_semantic_numeric_scalars = ["port"]
extra_key_value_bag_names = ["labels"]
ignored_key_value_bag_names = ["tags"]
ignored_diagnostic_codes = ["api_candidate_semantic_module"]
baseline = ".modum-baseline.json"
```

Use `[package.metadata.modum]` inside a member crate to override workspace defaults for that package. Package settings inherit the workspace defaults first, then apply only the keys you set locally.

`include` and `exclude` are optional scan defaults. CLI `--include` overrides metadata `include`, and CLI `--exclude` adds to metadata `exclude`.

`ignored_diagnostic_codes` is additive across workspace, package, and CLI `--ignore` values. Use it for durable repo-level exceptions.

`baseline` is a repo-root-relative JSON file of existing coded diagnostics. Matching baseline entries are filtered out after normal analysis. A metadata baseline is optional until the file exists; an explicit CLI `--baseline <path>` requires the file to exist.

Profile guide:

- `core`: internal namespace readability, including private type naming, type-alias hygiene, internal module-boundary rules, and glob/prelude pressure when imports flatten preserved namespaces
- `surface`: `core` plus caller-facing path shaping and typed boundary nudges for public and shared crate-visible surfaces, including semantic scalar boundaries and `anyhow` leakage
- `strict`: `surface` plus the heavier advisory heuristics, including semantic-module family suggestions, raw string error surfaces, raw ids, raw key-value bags, bool clusters, manual flag sets, and API-shape taste rules

Profile precedence:

- CLI `--profile` overrides package and workspace metadata
- `[package.metadata.modum] profile = "..."` overrides workspace metadata for that crate
- `[workspace.metadata.modum] profile = "..."` sets the workspace default
- if no profile is set anywhere, `strict` is used

Tuning guide:

- `generic_nouns`: generic leaves like `Repository`, `Error`, or `Request`
- `namespace_preserving_modules`: modules that should stay visible at call sites, such as `http`, `email`, `partials`, or `components`
- `extra_namespace_preserving_modules` / `ignored_namespace_preserving_modules`: additive tuning for preserve-module pressure when defaults are close but UI or domain modules like `widgets`, `components`, `page`, or `partials` need adjustment
- `organizational_modules`: modules that should not leak into the public API surface, such as `error`, `request`, or `response`
- `extra_semantic_string_scalars` / `ignored_semantic_string_scalars`: token families for string-like boundary names such as `email`, `url`, `path`, or your own repo-specific additions like `mime`
- `extra_semantic_numeric_scalars` / `ignored_semantic_numeric_scalars`: token families for numeric boundary names such as `duration`, `timestamp`, `ttl`, or repo-specific numeric concepts
- `extra_key_value_bag_names` / `ignored_key_value_bag_names`: token families for string bag names such as `metadata`, `headers`, `params`, or repo-specific names like `labels`
- `ignored_diagnostic_codes`: exact diagnostic codes to suppress, such as `api_candidate_semantic_module`
- `baseline`: repo-root-relative path for a generated baseline file such as `.modum-baseline.json`

These tuning keys work on lowercase name tokens, not full paths.

Adoption workflow:

- start with `--profile core` or `--mode warn`
- use `ignored_diagnostic_codes` for durable repo-specific exceptions
- use `ignored_namespace_preserving_modules = ["components", "page", "partials"]` when a UI aggregator repo intentionally flattens those modules and you do not want to replace the full preserve-module default set
- generate a baseline with `modum check --write-baseline .modum-baseline.json`
- apply it in CI with `modum check --baseline .modum-baseline.json` or `metadata.modum.baseline = ".modum-baseline.json"`

## Lint Categories

### Import Style

These warn when imports or re-exports flatten a namespace that should stay visible.

- `namespace_flat_use`
  Warning for flattened imports of generic nouns when there is an actionable namespace-visible call-site form that adds net context, such as `storage::Repository` or `http::StatusCode`. It skips cases where the only preserved form would still be redundant, such as `error::Error` or `response::Response`.
- `namespace_flat_use_preserve_module`
  Warning for flattened imports from configured namespace-preserving modules when the preserved call-site form still adds net context.
- `namespace_flat_use_redundant_leaf_context`
  Warning for flattened imports or actionable rename-heavy aliases whose leaf repeats parent context. For plain imports, this only fires when the shorter leaf would be an actionable generic noun such as `Repository`, `Error`, or `Id`. For rename aliases, this only fires when the qualified form would still preserve real context, such as `http::StatusCode` or `page::Event`.
- `namespace_redundant_qualified_generic`
  Warning for qualified call-site paths whose module only repeats a generic category already named by the leaf, such as `response::Response` or `error::Error`.
- `namespace_prelude_glob_import`
  Warning for `use ...::prelude::*` imports that hide the real source modules and flatten call-site context.
- `namespace_glob_preserve_module`
  Warning for glob imports from configured namespace-preserving modules such as `http::*`, when the import erases context the module name should carry at call sites.
- `namespace_parent_surface`
- `namespace_flat_pub_use`
- `namespace_flat_pub_use_preserve_module`
- `namespace_flat_pub_use_redundant_leaf_context`

Examples:

- `use storage::Repository;`
- `use http::Client;`
- `use user::UserRepository;`
- `response::Response`
- `use crate::error::Error;` inside a crate whose root surface already exposes `Error`
- `pub use auth::{login, logout};`
- `use http::prelude::*;`
- `use http::*;`

Canonical parent-surface re-exports are allowed. `pub use error::{Error, Result};` is valid when that is how a module intentionally exposes `module::Error` and `module::Result`. The same applies to broader UI surfaces such as exposing both `components::Button` and `partials::Button`.

A semantic child module namespace can also stay flat when it is already doing the call-site naming work. For example, `use components::tab_set;` with call sites like `tab_set::ContentProps` should not be forced into `components::tab_set::ContentProps`.

### Public API Paths

These warn when public leaves are too generic for a weak parent, when the path repeats context it already has, or when a flat family suggests a semantic module surface.

For these surface-shape rules, shared crate-visible surfaces such as `pub(crate)` items and re-exports are treated the same way as fully public ones.

- `api_missing_parent_surface_export`
  Warning for public child modules that should also surface a readable parent alias, such as `components::Button` over `components::button::Button`, or `outcome::Toxicity` over `outcome::toxicity::Outcome`.
- `api_weak_module_generic_leaf`
- `api_redundant_leaf_context`
  Warning for public leaves that repeat semantic module context already carried by the path, such as `user::UserRepository`, or that bake a sibling semantic module into a flat public leaf when `user::Repository` already exists.
- `api_candidate_semantic_module`
  Advisory warning for public item families that suggest a semantic module surface, either through a shared head across at least three siblings like `UserRepository`, `UserService`, and `UserId`, or through a shared generic tail like `CompletedOutcome`, `RejectedOutcome`, and `toxicity::Outcome`. It works on parsed source only and does not see macro expansion or cfg-pruned items.
- `api_candidate_semantic_module_unsupported_construct`
  Advisory warning for scopes where semantic-module family inference was skipped because the parsed source includes unsupported observation gaps such as `#[cfg]`, `macro_rules!`, other item macros, or `include!`.
- `api_manual_enum_string_helper`
  Advisory warning for public enum string surfaces that are spelled manually, including bespoke non-const methods such as `label()` or `as_str()`, free helpers such as `scenario_label(&Scenario)`, and manual `Display` impls that only map variants to string literals.
- `api_ad_hoc_parse_helper`
  Advisory warning for public enum parse helpers, including free `parse_*` functions and inherent methods such as `Mode::parse(&str) -> Result<Self, _>`, when `FromStr` or `TryFrom<&str>` would be a better standard boundary.
- `api_parallel_enum_metadata_helper`
  Advisory warning for public enums that expose several parallel metadata helpers like `label()`, `code()`, and `source_term()` over repeated `match self` blocks, when a typed descriptor surface would model that metadata more cleanly.
- `api_strum_serialize_all_candidate`
  Warning for per-variant `strum` string attributes that could be replaced by one enum-level `serialize_all` rule without changing the external strings.
- `api_builder_candidate`
  Warning for public constructors or workflow entrypoints that take several positional weak parameters and would read better as a builder or typed options struct. It skips functions already marked with a builder surface.
- `api_repeated_parameter_cluster`
  Warning for repeated public constructor or workflow signatures that reuse the same ordered named parameter cluster across entrypoints, when a shared options type or `bon` builder would avoid duplicating the call shape.
- `api_optional_parameter_builder`
  Warning for builder-shaped public entrypoints that take positional `Option<_>` parameters and would read better as a `bon` builder, so callers can omit unset values instead of passing `None`.
- `api_defaulted_optional_parameter`
  Warning for builder-shaped public entrypoints that immediately default positional `Option<_>` parameters, when a `bon` builder would let callers omit those values entirely.
- `callsite_maybe_some`
  Advisory warning for `maybe_*` method calls that pass `Some(...)` directly, which usually defeats the point of having paired `x(...)` and `maybe_x(...)` builder setters.
- `api_standalone_builder_surface`
  Advisory warning for families of public `with_*` or `set_*` free functions that collectively behave like a builder surface for one type.
- `api_boolean_protocol_decision`
  Warning for public `bool` parameters or fields that encode a domain or protocol decision rather than a runtime toggle.
- `api_forwarding_compat_wrapper`
  Warning for explicit conversion helpers such as `to_*` or `into_*` methods that only forward to an existing `From` conversion already present in the crate.
- `api_stringly_protocol_collection`
  Advisory warning for public const or static collections that enumerate protocol, state, transition, artifact, gate, or step values as raw strings instead of typed enums or descriptor maps.
- `api_stringly_model_scaffold`
  Advisory warning for public structs that carry several semantic descriptor fields as raw strings, such as `state_path`, `kind_label`, and `next_machine`, when those concepts would read better as typed enums, newtypes, or a focused descriptor type.
- `api_redundant_category_suffix`

Examples:

- `UserRepository`, `UserService`, `UserId`
- `CompletedOutcome`, `RejectedOutcome`, `toxicity::Outcome`
- `UserRepository` when `user::Repository` already exists
- `partials::button::Button` when the intended surface should also expose `partials::Button`
- `outcome::toxicity::Outcome` when the intended surface should also expose `outcome::Toxicity`
- `storage::Repository`
- `user::UserRepository`
- `user::error::InvalidEmailError`

Private organizational child modules are allowed to flatten their family items back to the parent surface. For example, `mod auth_shell; pub use auth_shell::{AuthShell, AuthShellVariant};` is treated as a valid parent-surface export shape.

### Boundary Modeling

These advisories push caller-facing types and signatures away from raw strings, raw integers, raw key-value bags, manual flag bits, and catch-all error surfaces.

- `api_anyhow_error_surface`
  Advisory warning for public or shared surfaces that expose `anyhow::Error` or `anyhow::Result` instead of a crate-owned typed error boundary.
- `api_string_error_surface`
  Advisory warning for public or shared surfaces that return `Result<_, String>` or store error text in raw string fields.
- `api_manual_error_surface`
  Advisory warning for public error types that manually expose both `Display` and `Error`, when the boundary may want a smaller focused error surface instead of more formatting boilerplate.
- `api_semantic_string_scalar`
  Advisory warning for caller-facing names like `email`, `url`, `path`, `locale`, or `currency` when they stay raw `String` or `&str`.
- `api_semantic_numeric_scalar`
  Advisory warning for caller-facing names like `duration`, `timestamp`, or `port` when they stay raw primitive integers.
- `api_raw_key_value_bag`
  Advisory warning for caller-facing `HashMap<String, String>`, `BTreeMap<String, String>`, or `Vec<(String, String)>` bags such as `metadata`, `headers`, `params`, or `tags`.
- `api_boolean_flag_cluster`
  Advisory warning for public structs or entrypoints that carry several booleans which jointly shape behavior.
- `api_integer_protocol_parameter`
  Advisory warning for protocol-like names such as `status`, `kind`, `mode`, or `phase` when they stay raw integers.
- `api_raw_id_surface`
  Advisory warning for raw id aliases, fields, parameters, or returns such as `UserId = String` or `request_id: u64`.
- `api_manual_flag_set`
  Advisory warning for parallel public `FLAG_*` integer constants, raw `flags` and `permissions` bit-mask boundaries, or repeated named bitmask checks and assembly in caller-facing code that suggest a typed flags surface.

### Module Boundaries

These catch weak or redundant public module structure.

- `api_catch_all_module`
- `api_repeated_module_segment`

Examples:

- `helpers`
- `error::error`

### Structural Errors

This rule is an error, not a warning.

- `api_organizational_submodule_flatten`

Example:

- `partials::error::Error` should usually be `partials::Error`
- `response::Response` should usually be `Response`

## What It Does Not Check

Some naming-guide rules stay advisory because they are too semantic to lint reliably without compiler-grade context. `api_candidate_semantic_module` is also source-level only; if a scope relies on `#[cfg]`, item macros, or `include!`, `modum` emits `api_candidate_semantic_module_unsupported_construct` instead of pretending the inferred family is complete.

Examples:

- choosing the best public path among several plausible domain decompositions
- deciding when an internal long name plus `pub use ... as ...` is the right tradeoff
- deciding whether a new module level adds real meaning or only mirrors the file tree in edge cases

## Scope

Default discovery:

- package root: scans `<root>/src`
- workspace root: scans each member crate's `src`

Override discovery with `--include`:

```bash
modum check --root . --include crates/api/src --include crates/domain/src
```

## False Positives And False Negatives

The broader import-style lints only inspect module-scope `use` items. They do not scan local block imports inside functions or tight test scopes, because those scopes often benefit from flatter imports.

To reduce false negatives:

- extend `namespace_preserving_modules` for domain modules like `user`, `billing`, or `tenant`
- use `extra_namespace_preserving_modules` or `ignored_namespace_preserving_modules` when the default preserve-module set is close but not quite right for your repo
- keep `generic_nouns` aligned with the generic leaves your API actually uses
- keep `organizational_modules` configured so `partials::error::Error`-style paths stay blocked
