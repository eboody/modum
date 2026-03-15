<div align="center">
  <img alt="modum logo" src="https://raw.githubusercontent.com/eboody/modum/main/modum-logo.svg" width="360">
  <p>Modum enforces consistent module naming, import style, and public API paths across a Rust workspace.</p>
  <p>
    <a href="https://github.com/eboody/modum/actions/workflows/ci.yml"><img src="https://github.com/eboody/modum/actions/workflows/ci.yml/badge.svg?branch=main&event=push" alt="build status" /></a>
    <a href="https://crates.io/crates/modum"><img src="https://img.shields.io/crates/v/modum.svg?logo=rust" alt="crates.io" /></a>
    <a href="https://docs.rs/modum"><img src="https://docs.rs/modum/badge.svg" alt="docs.rs" /></a>
  </p>
</div>

# modum

It is a lint tool. It reports diagnostics. It does not rewrite code.

## Why It Exists

These API shapes are legal Rust, but they add noise:

```rust
use http::Client;
use user::UserRepository;

pub use partials::error::Error;
```

These read more clearly:

```rust
use http;
use user;

pub use partials::Error;
```

And these public paths usually read better:

```text
user::Repository
user::error::InvalidEmail
partials::Error
```

instead of:

```text
user::UserRepository
user::error::InvalidEmailError
partials::error::Error
```

`modum` enforces that style across an entire workspace.

## Mental Model

`modum` follows three rules:

1. Keep namespace context visible at call sites.
2. Keep public paths meaningful without redundancy.
3. Use modules for domain boundaries, not file organization.

## Quick Usage

```bash
cargo run -p modum -- check --root .
cargo run -p modum -- check --root . --mode warn
cargo run -p modum -- check --root . --format json
```

If you already built the binary:

```bash
target/debug/modum check --root .
```

Environment:

```bash
MODUM=off|warn|deny
```

Default mode is `deny`.

## Exit Behavior

- `0`: clean, or warnings allowed via `--mode warn`
- `2`: warning-level policy violations found in `deny` mode
- `1`: hard errors, including parse/configuration failures and error-level policy violations such as `api_organizational_submodule_flatten`

## Configuration

Configure the lints in any workspace with Cargo metadata:

```toml
[workspace.metadata.modum]
generic_nouns = ["Id", "Repository", "Service", "Error", "Command", "Request", "Response"]
weak_modules = ["storage", "transport", "infra", "common", "misc", "helpers", "helper", "types", "util", "utils"]
catch_all_modules = ["common", "misc", "helpers", "helper", "types", "util", "utils"]
organizational_modules = ["error", "errors"]
namespace_preserving_modules = ["auth", "command", "components", "email", "error", "http", "page", "partials", "policy", "query", "repo", "store", "storage", "transport", "infra"]
```

Use `[package.metadata.modum]` inside a member crate to override workspace defaults for that package.

Tuning guide:

- `generic_nouns`: generic leaves like `Repository`, `Error`, or `Request`
- `namespace_preserving_modules`: modules that should stay visible at call sites, such as `http`, `email`, `partials`, or `components`
- `organizational_modules`: modules that should not leak into the public API surface, such as `error`

## Lint Categories

### Import Style

These warn when imports or re-exports flatten a namespace that should stay visible.

- `namespace_flat_use`
- `namespace_flat_use_preserve_module`
- `namespace_flat_use_redundant_leaf_context`
- `namespace_parent_surface`
- `namespace_flat_pub_use`
- `namespace_flat_pub_use_preserve_module`
- `namespace_flat_pub_use_redundant_leaf_context`

Examples:

- `use storage::Repository;`
- `use http::Client;`
- `use user::UserRepository;`
- `use crate::error::Error;` inside a crate whose root surface already exposes `Error`
- `pub use auth::{login, logout};`

Canonical parent-surface re-exports are allowed. `pub use error::{Error, Result};` is valid when that is how a module intentionally exposes `module::Error` and `module::Result`. The same applies to broader UI surfaces such as exposing both `components::Button` and `partials::Button`.

### Public API Paths

These warn when public leaves are too generic for a weak parent, or when the path repeats context it already has.

- `api_missing_parent_surface_export`
- `api_weak_module_generic_leaf`
- `api_redundant_leaf_context`
- `api_redundant_category_suffix`

Examples:

- `partials::button::Button` when the intended surface should also expose `partials::Button`
- `storage::Repository`
- `user::UserRepository`
- `user::error::InvalidEmailError`

Private organizational child modules are allowed to flatten their family items back to the parent surface. For example, `mod auth_shell; pub use auth_shell::{AuthShell, AuthShellVariant};` is treated as a valid parent-surface export shape.

### Module Boundaries

These catch weak or redundant public module structure.

- `api_catch_all_module`
- `api_repeated_module_segment`

Examples:

- `helpers`
- `error::error`

### Structural Errors

This is an error-level rule, not a warning.

- `api_organizational_submodule_flatten`

Example:

- `partials::error::Error` should usually be `partials::Error`

## What It Does Not Check

Some naming-guide rules stay advisory because they are too semantic to lint reliably without compiler-grade context.

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
- keep `generic_nouns` aligned with the generic leaves your API actually uses
- keep `organizational_modules` configured so `partials::error::Error`-style paths stay blocked
