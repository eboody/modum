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

`modum` exists to catch two common Rust API-shape problems:

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

`modum` enforces that style across an entire workspace.

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

## CI Usage

Use `modum` the same way you would use `clippy` or `cargo-deny`: run it as a normal command in CI, not from `build.rs`.

```yaml
- run: cargo install modum
- run: cargo modum check --root .
```

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
organizational_modules = ["error", "errors", "request", "response"]
namespace_preserving_modules = ["auth", "command", "components", "email", "error", "http", "page", "partials", "policy", "query", "repo", "store", "storage", "transport", "infra"]
```

Use `[package.metadata.modum]` inside a member crate to override workspace defaults for that package.

Tuning guide:

- `generic_nouns`: generic leaves like `Repository`, `Error`, or `Request`
- `namespace_preserving_modules`: modules that should stay visible at call sites, such as `http`, `email`, `partials`, or `components`
- `organizational_modules`: modules that should not leak into the public API surface, such as `error`, `request`, or `response`

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

Canonical parent-surface re-exports are allowed. `pub use error::{Error, Result};` is valid when that is how a module intentionally exposes `module::Error` and `module::Result`. The same applies to broader UI surfaces such as exposing both `components::Button` and `partials::Button`.

A semantic child module namespace can also stay flat when it is already doing the call-site naming work. For example, `use components::tab_set;` with call sites like `tab_set::ContentProps` should not be forced into `components::tab_set::ContentProps`.

### Public API Paths

These warn when public leaves are too generic for a weak parent, when the path repeats context it already has, or when a flat family suggests a semantic module surface.

- `api_missing_parent_surface_export`
- `api_weak_module_generic_leaf`
- `api_redundant_leaf_context`
  Warning for public leaves that repeat semantic module context already carried by the path, such as `user::UserRepository`, or that bake a sibling semantic module into a flat public leaf when `user::Repository` already exists.
- `api_candidate_semantic_module`
  Advisory warning for public item families such as `UserRepository`, `UserService`, and `UserId` that share a semantic head under one parent and suggest a module surface like `user::{Repository, Service, Id}`.
- `api_redundant_category_suffix`

Examples:

- `UserRepository`, `UserService`, `UserId`
- `UserRepository` when `user::Repository` already exists
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

This rule is an error, not a warning.

- `api_organizational_submodule_flatten`

Example:

- `partials::error::Error` should usually be `partials::Error`
- `response::Response` should usually be `Response`

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
