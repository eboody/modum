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

Apply a net-context test before preserving a qualifier. If the leaf already says the generic category clearly and the qualifier mostly repeats it, the qualified form is noise rather than clarity. Paths like `response::Response` or `error::Error` should usually be shortened at call sites.

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
- Treat lint suggestions as candidates, not commands; if qualification adds no net meaning, prefer the shorter call-site shape and fix the lint.
- Only shorten a leaf to `Repository`, `Error`, `Id`, or similar when the parent path already supplies the missing context.
- If a longer source name is still useful, keep it internally and re-export the shorter public name.

## Machine-Enforced Lints

`modum` enforces the subset of this guide that can be checked reliably across a workspace:

- `namespace_flat_use`
  Warning. Flags flattened imports of generic nouns such as `Repository` or `Error` when there is an actionable namespace-visible call-site form that adds net context, such as `storage::Repository` or `http::StatusCode`. It skips cases where the only preserved form would still be redundant, such as `error::Error` or `response::Response`.
- `namespace_flat_use_preserve_module`
  Warning. Flags flattened imports from configured namespace-preserving modules such as `email`, `http`, `query`, or `storage` when the preserved call-site form still adds net context.
- `namespace_flat_use_redundant_leaf_context`
  Warning. Flags flattened imports or actionable rename-heavy aliases such as `use user::UserRepository;` or `use playwright::api::page::Event as PageEvent;` where the child module already supplies the missing context. For plain imports, this only applies when the shorter leaf would land on an actionable generic noun such as `Repository`, `Error`, or `Id`. For rename aliases, this only applies when the preserved qualifier still adds real context at call sites.
- `namespace_redundant_qualified_generic`
  Warning. Flags qualified call-site paths such as `response::Response` or `error::Error` when the qualifier only repeats a generic category that the leaf already names clearly.
- `namespace_aliased_qualified_path`
  Warning. Flags qualified call-site paths such as `write_back_domain::Submission` when a renamed snake_case namespace alias has flattened a semantic path like `domain::write_back` into a technical local synonym. If the resolved canonical path is itself redundant, it suggests the final readable surface instead.
- `namespace_parent_surface`
  Warning. Flags imports that reach into an internal child module even though the containing parent surface already exposes the same readable item, such as preferring `http::Error` over `error::Error`.
- `namespace_flat_pub_use`
  Warning. Flags flattened public re-exports of those generic nouns when they bypass a meaningful child facet. It does not flag module-root re-exports that intentionally create the canonical parent surface, such as `pub use error::Error;` for `user::Error`.
- `namespace_flat_pub_use_preserve_module`
  Warning. Flags flattened public re-exports from configured namespace-preserving modules when the child facet should stay visible. It does not flag broader parent-surface exports such as exposing both `components::Button` and `partials::Button`, and it skips private alias modules that only exist to feed a parent surface.
- Semantic child module namespaces such as `tab_set::ContentProps` are already carrying context at call sites. Do not force a broader parent like `components::tab_set::ContentProps` unless that broader parent adds net meaning.
- `namespace_flat_pub_use_redundant_leaf_context`
  Warning. Flags flattened public re-exports whose leaf still repeats the parent context. It does not flag family re-exports from private organizational child modules when flattening them is how the parent surface is intentionally shaped.
- `api_missing_parent_surface_export`
  Warning. Flags public child modules that expose a same-name main item such as `button::Button`, or a sole generic main item such as `toxicity::Outcome`, without also re-exporting the readable parent surface. It does not treat the crate root as automatically readable enough for domain items like `domain::User`.
- `api_weak_module_generic_leaf`
  Warning. Flags paths such as `storage::Repository` where a weak technical module exposes a generic leaf.
- `api_redundant_leaf_context`
  Warning. Flags paths such as `user::UserRepository` or `page::ErrorPage` where the leaf repeats the parent context, and also flags flat public leaves such as `UserRepository` when a sibling semantic module surface like `user::Repository` already exists.
- `api_candidate_semantic_module`
  Advisory warning. Flags sibling public items such as `UserRepository`, `UserService`, and `UserId` when at least three shared-head items suggest a semantic module surface like `user::{Repository, Service, Id}`. When those siblings also share a common tail, such as `CaseInboxLaunch`, `CaseDetailLaunch`, and `CaseAuditLaunch`, it can suggest a nested surface like `case::launch::{Inbox, Detail, Audit}`. It also flags families such as `CompletedOutcome`, `RejectedOutcome`, and `toxicity::Outcome` when their shared generic tail suggests a semantic module surface like `outcome::{Completed, Rejected, Toxicity}`. It skips weak promoted heads like `to`, `has`, `open`, and `rolled`, and it skips hidden or internal module scopes. This is a parsed-source heuristic, not a macro-expanded or cfg-pruned authority surface.
- `api_candidate_semantic_module_unsupported_construct`
  Advisory warning. Flags scopes where semantic-module family inference was skipped because the parsed source includes unsupported observation gaps such as `#[cfg]`, `macro_rules!`, other item macros, or `include!`. `modum` emits this instead of `api_candidate_semantic_module` when the raw source is too weak to justify that heuristic.
- `api_manual_enum_string_helper`
  Advisory warning. Flags public enum string surfaces that are spelled manually, including non-const methods like `label()` or `as_str()`, free helpers like `scenario_label(&Scenario)`, and manual `Display` impls that only map variants to string literals.
- `api_ad_hoc_parse_helper`
  Advisory warning. Flags public enum parse helpers such as free `parse_*` functions and inherent methods like `Mode::parse(&str) -> Result<Self, _>` when `FromStr` or `TryFrom<&str>` would be a more standard boundary surface. It skips helpers when the enum already implements `FromStr` in the same scope.
- `api_parallel_enum_metadata_helper`
  Advisory warning. Flags public enums that expose several parallel metadata helpers such as `label()`, `code()`, and `source_term()` over repeated `match self` blocks, when a typed descriptor surface would model that metadata more cleanly.
- `api_strum_serialize_all_candidate`
  Warning. Flags per-variant `strum` string attributes that could be replaced by one enum-level `serialize_all` rule without changing the external strings.
- `api_builder_candidate`
  Warning. Flags public constructors or workflow entrypoints that take several positional weak parameters and would likely read better as a builder or typed options struct. It skips functions already marked with a builder surface.
- `api_repeated_parameter_cluster`
  Warning. Flags repeated public constructor or workflow signatures that reuse the same ordered named parameter cluster across entrypoints, when a shared options type or `bon` builder would likely avoid duplicating the call shape.
- `api_optional_parameter_builder`
  Warning. Flags builder-shaped public entrypoints that take positional `Option<_>` parameters and would likely read better as a `bon` builder, so callers can omit unset values instead of passing `None`.
- `api_defaulted_optional_parameter`
  Warning. Flags builder-shaped public entrypoints that immediately default positional `Option<_>` parameters, when a `bon` builder would let callers omit those values entirely.
- `api_standalone_builder_surface`
  Advisory warning. Flags public `with_*` or `set_*` free-function families that collectively behave like a builder surface for one type.
- `api_boolean_protocol_decision`
  Warning. Flags public `bool` parameters or fields that encode a domain or protocol decision rather than a runtime toggle.
- `api_forwarding_compat_wrapper`
  Warning. Flags explicit conversion helpers such as `to_*` or `into_*` methods that only forward to an existing `From` conversion already present in the crate.
- `api_stringly_protocol_collection`
  Advisory warning. Flags public const or static collections that enumerate protocol, state, transition, artifact, gate, or step values as raw strings instead of typed enums or descriptor maps.
- `api_stringly_model_scaffold`
  Advisory warning. Flags public structs that carry several semantic descriptor fields as raw strings, such as `state_path`, `kind_label`, and `next_machine`, when those concepts would likely read better as typed enums, newtypes, or a focused descriptor type. It is intentionally narrower than a generic "too many strings" check and targets obvious modeling scaffolds rather than ordinary text payloads.
- `api_redundant_category_suffix`
  Warning. Flags paths such as `user::error::InvalidEmailError` where the parent module already provides the category.
- `api_catch_all_module`
  Warning. Flags public catch-all module names such as `helpers`, `common`, or `types`.
- `api_repeated_module_segment`
  Warning. Flags repeated public nesting such as `error::error`.
- `api_organizational_submodule_flatten`
  Error. Flags public paths such as `partials::error::Error` or `response::Response` when the nested module is just organizational and the canonical path should drop that category layer.

## Advisory Rules

The rest of the guide remains advisory. `modum` does not try to prove:

- which of several plausible domain decompositions reads best at call sites
- when a new module level adds enough meaning to justify itself
- when an internal long name plus a public re-export is the right tradeoff

## False-Negative Strategy

`modum` reduces false negatives in two ways:

- heuristic coverage for obviously redundant leaves such as `user::UserRepository`
- configurable coverage for module boundaries that should stay visible via `namespace_preserving_modules`

That second list should be extended per workspace for domain modules the defaults cannot infer, such as `user`, `billing`, or `tenant`. The defaults intentionally favor semantic surfaces like `partials`, `page`, and `components` over broader container namespaces like `views`.

Observation model:
- `modum` reads parsed Rust source files.
- It does not expand macros, resolve `include!`, or prune `#[cfg]`.
- When those constructs would weaken semantic-module family inference, it emits `api_candidate_semantic_module_unsupported_construct` instead of presenting `api_candidate_semantic_module` as complete.
