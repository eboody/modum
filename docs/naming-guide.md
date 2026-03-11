# Naming Guide

This guide defines how modules and items should be named in this repo. The goal is a clear public API, not a mechanical rewrite rule.

## Core Model

- Choose the public path first, then choose the source name.
- Use modules to express domain boundaries and meaningful facets.
- Do not force everything into exactly one module level.
- Keep public item names short when the surrounding module path already provides the missing context.
- Keep internal names more descriptive when local clarity benefits from it, then re-export a shorter public name if needed.
- Prefer import styles that leave the namespace visible at call sites.

## Module Names

- Use `lower_snake_case`.
- Name modules for stable concepts, domains, or facets.
- Add a module level only when that segment adds a real axis of meaning.
- Good module roles include `error`, `email`, `http`, `policy`, `query`, `command`, `store`, `repo`, and similar domain terms.
- Avoid catch-all names like `common`, `misc`, `helpers`, or `types` unless the boundary is genuinely shared and a more specific name would be misleading.
- Stop nesting when the extra segment only repeats the parent or mirrors file layout without improving the API.

## Item Names

- Use `PascalCase` for types and traits.
- Use `snake_case` for functions and methods.
- Use `SCREAMING_SNAKE_CASE` for constants and statics.
- Inside a descriptive module, prefer short leaf names such as `user::Repository`, `user::Id`, `user::Error`, `user::Request`.
- Generic leaf names such as `Repository`, `Error`, `Id`, and `Request` only make sense when the parent path already tells the reader which domain they belong to.
- If the parent path is weak or purely technical, keep the entity name in the leaf or move the item under a stronger module.
- Do not repeat the module name in the public leaf unless that extra prefix carries meaning that the path does not already provide.
- Prefer `user::Repository` over `user::UserRepository` when the `user` module already establishes the domain.

## Prefer And Avoid

- Prefer `user::Repository` over `user::UserRepository`.
  The module already carries the `user` context.
- Prefer `storage::UserRepository` over `storage::Repository`.
  `storage` is a technical boundary, not the domain itself.
- Prefer `user::error::InvalidEmail` over `user::InvalidEmailError`.
  The nested path makes the category explicit without stuffing it into the leaf.
- Prefer `user::email::Address` over `user::EmailAddress` when `email` is a real subdomain with more than one item.
  If `email` is only a single isolated concept, `user::EmailAddress` may still be the better shape.

## Nested Modules Are Allowed

One module level is not a hard rule. Use as many levels as the API needs, as long as each level is doing real work.

Examples:

- `user::Repository`
  The `user` module already provides the domain, so the leaf can stay generic.
- `user::error::InvalidEmail`
  Use this shape when `error` is a useful category and the leaf is a concrete variant or subtype within that category.
- `user::email::Error`
  Use this shape when `email` is the real subdomain and `Error` is that subdomain's main error type.
- `user::http::Request`
  Use this shape when transport or protocol is the extra axis that matters.

It is fine to expose both a broad domain item and a deeper specialized item:

- `user::Error`
- `user::error::InvalidEmail`

That pattern works when `user::Error` is the umbrella domain error and `user::error::*` holds narrower classified types.

The rule is not "always shorten the leaf." The rule is "shorten the leaf when the path still reads clearly."

## Re-exports And Aliases

The public API does not have to match the internal source name exactly.

Use `pub use ... as ...` when:

- a longer source name improves local readability or searchability
- generated code or macro output is clearer with a prefixed name
- the public path is cleaner with a shorter leaf name

Example:

```rust
// src/user/mod.rs
pub mod error;
mod repository;

pub use error::UserError as Error;
pub use repository::UserRepository as Repository;
```

That lets internal code keep `UserError` and `UserRepository` while the public API stays:

- `user::Error`
- `user::Repository`

That still leaves room for deeper items such as `user::error::InvalidEmail` when the nested module carries real meaning.

Use aliases to simplify the API, not to invent synonyms that hide real differences.

## Import Style

Prefer imports that preserve the module boundary:

```rust
use crate::domain::user;

fn load(repo: &user::Repository) {}
```

Prefer that over flattening the leaf import when the module name carries useful meaning:

```rust
use crate::domain::user::Repository;
```

Direct leaf imports are still acceptable for traits, macros, tests, or tight local scopes where repeated qualification is just noise. They should be the exception when they erase a useful namespace.

## Decision Rules

When adding or renaming an item, apply these checks:

1. Is the parent module descriptive enough that the leaf can be short?
2. If not, should the item keep a longer leaf name instead of becoming too generic?
3. Does another module segment add a real domain or facet boundary?
4. Does the public leaf repeat information that is already present in the path?
5. Would an internal long name plus a public re-export produce a cleaner API?
6. Will the import style preserve the shape of the namespace at call sites?

## Codex Defaults

Codex should default to these behaviors:

- Do not mechanically turn every prefixed name into exactly one `head::Tail` split.
- Consider whether `user::auth::Token`, `user::error::InvalidEmail`, or `user::http::Request` is the better public path.
- Prefer the path that reads best at call sites.
- Only shorten a leaf to `Repository`, `Error`, `Id`, or similar when the parent path already supplies the missing context.
- If a longer source name is still useful, keep it internally and re-export the shorter public name.

## Machine-Enforced Lints

`modum` enforces the subset of this guide that can be checked reliably across a workspace:

- `namespace_flat_use`
  Warning. Flags flattened imports of configured generic nouns such as `Repository` or `Error`. This covers cases like `use storage::Repository;` where the guide prefers keeping `storage::Repository` visible at call sites.
- `namespace_flat_use_preserve_module`
  Warning. Flags flattened imports from configured namespace-preserving modules such as `email`, `http`, `query`, or `storage`.
- `namespace_flat_use_redundant_leaf_context`
  Warning. Flags flattened imports such as `use user::UserRepository;` where the leaf itself repeats the parent module context.
- `namespace_flat_pub_use`
  Warning. Flags flattened public re-exports of those generic nouns.
- `namespace_flat_pub_use_preserve_module`
  Warning. Flags flattened public re-exports from configured namespace-preserving modules.
- `namespace_flat_pub_use_redundant_leaf_context`
  Warning. Flags flattened public re-exports whose leaf still repeats the parent context.
- `api_weak_module_generic_leaf`
  Warning. Flags paths such as `storage::Repository` where a weak technical module exposes a generic leaf.
- `api_redundant_leaf_context`
  Warning. Flags paths such as `user::UserRepository` or `page::ErrorPage` where the leaf repeats the parent context.
- `api_redundant_category_suffix`
  Warning. Flags paths such as `user::error::InvalidEmailError` where the parent module already provides the category.
- `api_catch_all_module`
  Warning. Flags public catch-all module names such as `helpers`, `common`, or `types`.
- `api_repeated_module_segment`
  Warning. Flags repeated public nesting such as `error::error`.
- `api_organizational_submodule_flatten`
  Error. Flags public paths such as `partials::error::Error` when the nested `error` module is just organizational and the canonical path should be `partials::Error`.

## Advisory Rules

The rest of the guide remains advisory. `modum` does not try to prove:

- which of several plausible domain decompositions reads best at call sites
- when a new module level adds enough meaning to justify itself
- when an internal long name plus a public re-export is the right tradeoff

## False-Negative Strategy

`modum` reduces false negatives in two ways:

- heuristic coverage for obviously redundant leaves such as `user::UserRepository`
- configurable coverage for module boundaries that should stay visible via `namespace_preserving_modules`

That second list should be extended per workspace for domain modules the defaults cannot infer, such as `user`, `billing`, or `tenant`.
