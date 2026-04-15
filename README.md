<div align="center">
  <img alt="modum logo" src="https://raw.githubusercontent.com/eboody/modum/main/modum-logo.svg" width="360">
  <p>Modum is a Rust linter that catches names, module paths, and exports that make code harder to read than they need to be.</p>
  <p>
    <a href="https://github.com/eboody/modum/actions/workflows/ci.yml"><img src="https://github.com/eboody/modum/actions/workflows/ci.yml/badge.svg?branch=main&event=push" alt="build status" /></a>
    <a href="https://crates.io/crates/modum"><img src="https://img.shields.io/crates/v/modum.svg?logo=rust" alt="crates.io" /></a>
    <a href="https://docs.rs/modum"><img src="https://docs.rs/modum/badge.svg" alt="docs.rs" /></a>
  </p>
</div>

# modum

`modum` reports diagnostics. It doesn't rewrite code.
It analyzes parsed Rust source with `syn`, not compiler-resolved semantics.
It focuses first on whether module paths keep call sites readable, and it also nudges caller-facing APIs away from raw or misleading forms.

## Start Here

- Want the idea fast? Read [Why It Exists](#why-it-exists).
- Want to try it? Jump to [Quick Usage](#quick-usage).
- Want to know what it can and can't see? Read [Observation Model](#observation-model).
- Want to tune it for a real repo? Start with [Configuration](#configuration).
- Want the full lint catalog? See [docs/lint-reference.md](docs/lint-reference.md).

## Why It Exists

`modum` optimizes for APIs and call sites that read through their module paths instead of making the last name in the path carry all the context.

`modum` spends most of its time on APIs that callers actually use. The same issue can still matter internally when the structure is drifting there too.

It mostly catches two things:

- flattened imports or re-exports that hide useful context at call sites
- final item names that repeat context the path should already be carrying

The payoff is usually not one isolated rename. It is several related items collapsing into one domain module.

Codebases often drift into this over time:

```rust
pub struct UserRepository;
pub struct UserService;
pub struct UserId;
pub struct UserController;
pub struct UserDto;
pub struct UserRequest;
pub struct UserResponse;
```

That usually reads more clearly as:

```rust
pub mod user {
    pub struct Repository;
    pub struct Service;
    pub struct Id;
    pub struct Controller;
    pub struct Dto;
    pub struct Request;
    pub struct Response;
}
```

That drift also leaks into imports and public APIs:

Before:

```rust
use user::UserRepository;
use user::UserService;

pub fn handle(repo: UserRepository) -> Result<UserResponse, Error> {
    todo!()
}
```

After:

```rust
use user;

pub fn handle(repo: user::Repository) -> Result<user::Response, error::Error> {
    todo!()
}
```

That is the real move `modum` is trying to protect. The domain belongs in the path. Once the path carries that context, names like `Repository`, `Service`, `Id`, `Request`, and `Response` can stay short and composable instead of each one compensating with `User...`.

The call-site lints are strongest when the shorter name is generic or when parsed source shows a clear local group of related items. `modum` doesn't assume every short single-segment name should stay qualified.

This only works when the parent path is actually carrying real meaning. If the parent is weak or technical, the longer name can still be better:

```rust
storage::Repository
UserRepository
```

Here `UserRepository` is often clearer, because `storage` is technical and `user` names the domain.

So the rule is:

- strong parent path: prefer `user::Repository`
- weak or technical parent: keep the more descriptive name
- fix the actual structure instead of rewarding cosmetic renames that only silence a lint

Owned code and external crates are treated differently for the same reason. For code you own, `modum` can suggest a better parent path that you could create, such as re-exporting `domain::user::User` as `domain::User`. For external crates, it stays conservative and only relies on paths that already exist.

## Observation Model

`modum` reads Rust source files with `syn` and reports best-effort checks from the parsed syntax tree.

It doesn't observe:

- items removed by `#[cfg]`
- macro-expanded items
- `include!`-generated items

When that decision would depend on those constructs, `modum` skips `api_candidate_semantic_module` and emits `api_candidate_semantic_module_unsupported_construct` instead.

The call-site namespace checks are also source-level only. If keeping a path visible would depend on macro-generated or cfg-driven sibling bindings, `modum` fails closed and emits `namespace_family_unsupported_construct` instead of pretending the local group is complete.

## Quick Usage

```bash
cargo install modum
cargo modum check --root .
cargo modum check --root . --mode warn
cargo modum --explain namespace_flat_use
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

`modum` runs the full lint set by default. The main runtime opt-outs are `--ignore <code>`, a baseline, explicit scan scoping with `--include` or `--exclude`, and `--mode warn` when you want diagnostics without a failing exit code.

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

There is no profile selector anymore. `modum` runs the full lint set by default and expects opt-out tuning through ignored codes, tuned name-token lists, or a baseline.

Tuning guide:

- `generic_nouns`: generic final names like `Repository`, `Error`, or `Request`
- `namespace_preserving_modules`: modules that should stay visible at call sites, such as `http`, `email`, `partials`, or `components`
- `extra_namespace_preserving_modules` / `ignored_namespace_preserving_modules`: additive tuning for whether those modules should stay visible at call sites when defaults are close but UI or domain modules like `widgets`, `components`, `page`, or `partials` need adjustment
- `organizational_modules`: modules that should not leak into the public API, such as `error`, `request`, or `response`
- `extra_semantic_string_scalars` / `ignored_semantic_string_scalars`: token families for string-like names at API boundaries such as `email`, `url`, `path`, or your own repo-specific additions like `mime`
- `extra_semantic_numeric_scalars` / `ignored_semantic_numeric_scalars`: token families for numeric names at API boundaries such as `duration`, `timestamp`, `ttl`, or repo-specific numeric concepts
- `extra_key_value_bag_names` / `ignored_key_value_bag_names`: token families for map-like names such as `metadata`, `headers`, `params`, or repo-specific names like `labels`
- `ignored_diagnostic_codes`: exact diagnostic codes to suppress, such as `api_candidate_semantic_module`
- `baseline`: repo-root-relative path for a generated baseline file such as `.modum-baseline.json`

These tuning keys work on lowercase name tokens, not full paths.

Adoption workflow:

- start with `--mode warn`
- use `ignored_diagnostic_codes` for durable repo-specific exceptions
- use `ignored_namespace_preserving_modules = ["components", "page", "partials"]` when a UI aggregator repo intentionally flattens those modules and you don't want to replace the full default set of modules that stay visible at call sites
- generate a baseline with `modum check --write-baseline .modum-baseline.json`
- apply it in CI with `modum check --baseline .modum-baseline.json` or `metadata.modum.baseline = ".modum-baseline.json"`

## Lint Categories

The full catalog lives in [docs/lint-reference.md](docs/lint-reference.md). In the README, the important split is what each category is trying to protect:

- Import Style: keep namespace context visible at call sites and stop flattened imports or re-exports from erasing meaning that belongs in the path.
- Public API Paths: keep public paths readable by preferring parent modules that carry real meaning, avoiding repeated context in the final item name, and surfacing obvious parent aliases when a child module is doing too much naming work.
- Boundary Modeling: push caller-facing APIs away from raw strings, raw integers, raw id aliases, weak error types, and other API shapes that hide meaning inside primitives.
- Module Boundaries: catch weak catch-all modules and repeated path segments that usually signal structure drift.
- Structural Errors: block public paths like `partials::error::Error` when an organizational helper module should be flattened back to the parent path.

Use `modum --explain <code>` for one lint at a time, or open [docs/lint-reference.md](docs/lint-reference.md) when you want the full category-by-category catalog.

## What It Doesn't Check

Some naming-guide rules stay advisory because they depend too much on meaning to lint reliably from parsed source alone. `api_candidate_semantic_module` is also source-level only; if a scope relies on `#[cfg]`, item macros, or `include!`, `modum` emits `api_candidate_semantic_module_unsupported_construct` instead of pretending the inferred group is complete.

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
- use `extra_namespace_preserving_modules` or `ignored_namespace_preserving_modules` when the default list of modules that stay visible at call sites is close but not quite right for your repo
- keep `generic_nouns` aligned with the generic final names your API actually uses
- keep `organizational_modules` configured so `partials::error::Error`-style paths stay blocked

## Read Next

- [docs/lint-reference.md](docs/lint-reference.md): full lint catalog and category detail
- [docs/editor-integration.md](docs/editor-integration.md): Neovim setup and editor-facing JSON usage
- [docs/naming-guide.md](docs/naming-guide.md): naming rules that the tool follows
