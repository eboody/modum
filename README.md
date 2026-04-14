<div align="center">
  <img alt="modum logo" src="https://raw.githubusercontent.com/eboody/modum/main/modum-logo.svg" width="360">
  <p>Modum checks source-level namespace shape, public surface paths, and caller-facing boundary heuristics across a Rust workspace.</p>
  <p>
    <a href="https://github.com/eboody/modum/actions/workflows/ci.yml"><img src="https://github.com/eboody/modum/actions/workflows/ci.yml/badge.svg?branch=main&event=push" alt="build status" /></a>
    <a href="https://crates.io/crates/modum"><img src="https://img.shields.io/crates/v/modum.svg?logo=rust" alt="crates.io" /></a>
    <a href="https://docs.rs/modum"><img src="https://docs.rs/modum/badge.svg" alt="docs.rs" /></a>
  </p>
</div>

# modum

It is a lint tool. It reports diagnostics. It doesn't rewrite code.
It analyzes parsed Rust source files. It doesn't expand macros, resolve `include!`, or prune `#[cfg]`.
It centers namespace shape first, and it also nudges caller-facing boundary modeling when a public surface gets less truthful through flattened paths, raw scalars, or weak module structure.

## Start Here

- Want the idea fast? Read [Why It Exists](#why-it-exists) and [Mental Model](#mental-model).
- Want to try it? Jump to [Quick Usage](#quick-usage).
- Want to tune it for a real repo? Start with [Configuration](#configuration) and the profile guide.
- Want the full lint catalog? See [docs/lint-reference.md](docs/lint-reference.md).

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

That is also why `user::Repository` is usually better than `user::UserRepository`: once the path is doing the domain work, the leaf doesn't need to repeat it.

The main caveat is that this only holds when `user` is a real semantic module. If the parent path is weak or technical, then a longer leaf can still be better. `UserRepository` is often clearer than `storage::Repository`.

So the rule is:

- strong semantic parent: prefer `user::Repository`
- weak or technical parent: prefer the more descriptive leaf

`modum` checks that style across an entire workspace at the parsed-source level.

## What It Is For

`modum` exists to make Rust code read through its paths and surfaces instead of compensating prefixes, suffixes, and flattened aliases.

- Put meaning in module paths when the path is where that meaning belongs.
- Keep leaves short when a strong parent path already carries the domain.
- Keep leaves specific when the parent path is weak or technical.
- Fix the actual structure instead of rewarding cosmetic renames that only silence a lint.

The target is clearer paths and a truer API shape, not one universal naming aesthetic and not shorter names for their own sake. A lint is only good when following it makes the path clearer and the surface more truthful.

That is also why owned code and external crates are treated differently. For code you own, `modum` can suggest a better parent surface that you could create, such as re-exporting `domain::user::User` as `domain::User`. For external crates, it stays conservative and only relies on surfaces that already exist.

## Observation Model

`modum` reads Rust source files with `syn` and reports source-level heuristics from the parsed AST.

It doesn't observe:

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

## Editor Integration

For editor setup, see [docs/editor-integration.md](docs/editor-integration.md). The short version is:

- use `--mode warn` so diagnostics don't fail the editor job
- use `--format json` for stable parsing
- resolve the workspace root explicitly if one editor session spans several crates

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
- use `ignored_namespace_preserving_modules = ["components", "page", "partials"]` when a UI aggregator repo intentionally flattens those modules and you don't want to replace the full preserve-module default set
- generate a baseline with `modum check --write-baseline .modum-baseline.json`
- apply it in CI with `modum check --baseline .modum-baseline.json` or `metadata.modum.baseline = ".modum-baseline.json"`

## Lint Categories

The full catalog lives in [docs/lint-reference.md](docs/lint-reference.md). In the README, the important split is what each category is trying to protect:

- Import Style: keep namespace context visible at call sites and stop flattened imports or re-exports from erasing meaning that belongs in the path.
- Public API Paths: keep public surfaces honest by preferring strong semantic parents, avoiding repeated leaf context, and surfacing obvious parent aliases when a child module is doing too much naming work.
- Boundary Modeling: push caller-facing APIs away from raw strings, raw integers, raw id aliases, weak error surfaces, and other boundary shapes that leak semantics into primitives.
- Module Boundaries: catch weak catch-all modules and repeated path segments that usually signal structure drift.
- Structural Errors: block public paths like `partials::error::Error` when an organizational child module should be flattened back to the parent surface.

Use `modum --explain <code>` for one lint at a time, or open [docs/lint-reference.md](docs/lint-reference.md) when you want the full category-by-category catalog.

## What It Doesn't Check

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

The broader import-style lints only inspect module-scope `use` items. They don't scan local block imports inside functions or tight test scopes, because those scopes often benefit from flatter imports.

To reduce false negatives:

- extend `namespace_preserving_modules` for domain modules like `user`, `billing`, or `tenant`
- use `extra_namespace_preserving_modules` or `ignored_namespace_preserving_modules` when the default preserve-module set is close but not quite right for your repo
- keep `generic_nouns` aligned with the generic leaves your API actually uses
- keep `organizational_modules` configured so `partials::error::Error`-style paths stay blocked

## Read Next

- [docs/lint-reference.md](docs/lint-reference.md): full lint catalog and category detail
- [docs/editor-integration.md](docs/editor-integration.md): Neovim setup and editor-facing JSON usage
- [docs/naming-guide.md](docs/naming-guide.md): naming rules that shape the tool's heuristics
