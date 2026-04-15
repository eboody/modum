# Lint Reference

This is the full category-by-category lint catalog for `modum`.

Use the README for the tool's thesis, quick usage, and configuration entry points. Use this document when you want the full reference surface in one place, or use `modum --explain <code>` for one diagnostic at a time.

## Import Style

These warn when imports or re-exports flatten a namespace that should stay visible.

- `namespace_flat_use`
  Warning for flattened imports of generic nouns when there is an actionable namespace-visible call-site form that adds net context, such as `storage::Repository` or `http::StatusCode`. It also covers family leaves when parsed source shows a clear local witness that the namespace still matters at call sites, such as `viewer::ResolutionFlow` alongside `viewer::ResolutionState` and `viewer::ResolutionOutcome`. It skips cases where the only preserved form would still be redundant, such as `error::Error` or `response::Response`.
- `namespace_flat_use_preserve_module`
  Warning for flattened imports from configured namespace-preserving modules when the preserved call-site form still adds net context.
- `namespace_flat_use_redundant_leaf_context`
  Warning for flattened imports or actionable rename-heavy aliases whose leaf repeats parent context. For plain imports, this only fires when the shorter leaf would be an actionable generic noun such as `Repository`, `Error`, or `Id`. For rename aliases, this only fires when the qualified form would still preserve real context, such as `http::StatusCode` or `page::Event`.
- `namespace_redundant_qualified_generic`
  Warning for qualified call-site paths whose module only repeats a generic category already named by the leaf, such as `response::Response` or `error::Error`. When the written path resolves through an imported namespace and a nearer parent surface would read better, it can recommend that promotable parent surface for owned code even if the export does not exist yet. It doesn't invent new parent surfaces for external crates such as `std`, `core`, `serde`, or `axum`.
- `namespace_prelude_glob_import`
  Warning for `use ...::prelude::*` imports that hide the real source modules and flatten call-site context.
- `namespace_glob_preserve_module`
  Warning for glob imports from configured namespace-preserving modules such as `http::*`, when the import erases context the module name should carry at call sites.
- `namespace_family_unsupported_construct`
  Advisory warning for imports, re-exports, or type aliases where namespace-family call-site inference was skipped because the parsed source includes unsupported observation gaps such as `#[cfg]`, `macro_rules!`, other item macros, or `include!`.
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

## Public API Paths

These warn when public leaves are too generic for a weak parent, when the path repeats context it already has, or when a flat family suggests a semantic module surface.

For these surface-shape rules, shared crate-visible surfaces such as `pub(crate)` items and re-exports are treated the same way as fully public ones.

- `api_missing_parent_surface_export`
  Warning for public child modules that should also surface a readable parent alias, such as `components::Button` over `components::button::Button`, or `outcome::Toxicity` over `outcome::toxicity::Outcome`.
- `api_weak_module_generic_leaf`
- `api_redundant_leaf_context`
  Warning for public leaves that repeat semantic module context already carried by the path, such as `user::UserRepository`, or that bake a sibling semantic module into a flat public leaf when `user::Repository` already exists.
- `api_candidate_semantic_module`
  Advisory warning for public item families that suggest a semantic module surface, either through a shared head across at least three siblings like `UserRepository`, `UserService`, and `UserId`, through select 2-member high-signal pairs like `TransactionId` plus `TransactionFailure` or `HttpRequest` plus `HttpResponse`, through a shared head plus tail that points to a nested surface like `case::launch::{Inbox, Detail, Audit}`, or through a shared generic tail like `CompletedOutcome`, `RejectedOutcome`, and `toxicity::Outcome`. It works on parsed source only and doesn't see macro expansion or cfg-pruned items.
- `api_candidate_facet_module`
  Advisory warning for root modules that flatten several leaf value-plus-error families next to a broader root `Error`, such as `user::{Email, EmailError, Username, UsernameError, Error}`. It points toward owned facets like `user::email` and `user::username`, with the root keeping the cross-facet boundary.
- `api_candidate_child_facet_module`
  Advisory warning for broader public modules that appear to want one deeper child facet for a validated leaf and its failure surface, such as `chat::message` wanting `chat::message::body` or `chat::moderation` wanting `chat::moderation::reason`. It is meant for modules that already expose aggregate items like `Message`, `Item`, or `Queue`, but still keep the validated leaf flat at the same level.
- `api_boundary_wraps_child_facet_error`
  Advisory warning for broader public `Error` boundaries that still wrap a flat child companion error like `message::BodyError` or `moderation::ReasonError` even though the corresponding child module already looks like it wants a deeper owner such as `message::body` or `moderation::reason`. It points toward letting the broader boundary wrap `message::body::Error` or `moderation::reason::Error` once that child facet exists.
- `api_owned_facet_companion_error`
  Advisory warning for already-owned public facets that define a local `Error` and also rely on a same-leaf companion failure like `TextError` for a local tuple-wrapper leaf such as `Text`. It points toward one canonical facet-local failure surface like `room::name::Error` instead of exporting both `TextError` and `Error` from the same owner.
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
- `user::{Email, EmailError, Username, UsernameError, Error}`
- `chat::message::{Body, Message}` when the error surface really wants `chat::message::body::{Body, Error}`
- `chat::Error::MessageBody { source: message::BodyError }` when the child owner really wants `chat::message::body::Error`
- `room::name::{Text, Error}` with a companion `TextError`
- `CompletedOutcome`, `RejectedOutcome`, `toxicity::Outcome`
- `UserRepository` when `user::Repository` already exists
- `partials::button::Button` when the intended surface should also expose `partials::Button`
- `outcome::toxicity::Outcome` when the intended surface should also expose `outcome::Toxicity`
- `storage::Repository`
- `user::UserRepository`
- `user::error::InvalidEmailError`

Private organizational child modules are allowed to flatten their family items back to the parent surface. For example, `mod auth_shell; pub use auth_shell::{AuthShell, AuthShellVariant};` is treated as a valid parent-surface export shape.

## Boundary Modeling

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

## Module Boundaries

These catch weak or redundant public module structure.

- `api_catch_all_module`
- `api_repeated_module_segment`
- `api_path_shim_module`
  Warning for surface-visible `#[path = ...] mod ...;` shims when the attribute is being used to keep an older filesystem layout under a nicer module path. If the public path wants `request_meta`, create a real `request_meta` module file or directory instead of mounting `request_meta_flow.rs` through `#[path]`.
- `internal_path_shim_module`
  Warning for internal `#[path = ...] mod ...;` shims when the attribute is being used to fake a semantic module layout, such as `flow::room::create` mounted from `../create_room_flow.rs`. If the structure wants that module, move or rename the files so the filesystem matches the namespace.

Examples:

- `helpers`
- `error::error`
- `#[path = "request_meta_flow.rs"] mod request_meta;`
- `#[path = "../create_room_flow.rs"] mod create;`

## Structural Errors

This rule is an error, not a warning.

- `api_organizational_submodule_flatten`

Example:

- `partials::error::Error` should usually be `partials::Error`
- `response::Response` should usually be `Response`
