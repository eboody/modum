# Modum Lint Misinterpretation Catalog

This document inventories ways a person or agent can technically "fix" a `modum` warning while undermining the point of the lint.

It is based on one example run against `eran_codes`, but the catalog is phrased as a general anti-pattern list rather than a workspace-specific prescription.

Run used:

```bash
target/debug/modum check --root ~/code/eran_codes --mode warn -p
```

Observation point:

- one parsed-source `modum` report
- manual review of representative files behind the reported lint families

This is a broad working catalog. It aims to cover the main bad-fix patterns without claiming to be exhaustive across all Rust workspaces.

## Shared Misinterpretation Patterns

These patterns repeat across many lint families. The per-lint sections below reference them by id so the catalog stays consolidated.

- `P01 Ignore Instead Of Fixing`
  Blanket-ignore the lint, baseline it away, or treat it as "style noise" without deciding whether the structure is actually lying.
- `P02 Cosmetic Leaf Rename`
  Rename only the item or type leaf while keeping the same weak path and the same structural problem.
- `P03 Cosmetic Parent Rename`
  Rename only the parent module while keeping it just as weak or organizational as before.
- `P04 Token Surgery`
  Mechanically drop or add the repeated token without checking whether the resulting name is still a real standalone name.
- `P05 Local Alias Patch`
  Silence one import or call site with `use ... as ...`, a local alias, or a one-off renamed import instead of fixing the owning surface.
- `P06 Caller Patch Instead Of Owner Fix`
  Adjust one use site, one re-export, or one wrapper layer while leaving the owning module or public surface unchanged.
- `P07 Duplicate Surface`
  Add the suggested shape as a second path but keep the old path equally canonical, so the codebase now has two truths instead of one.
- `P08 Weak Bucket Substitution`
  Move the code into another weak bucket like `common`, `shared`, `helpers`, `types`, `models`, `runtime`, `ops`, or `impl`.
- `P09 Fragmentary Leaf Collapse`
  Produce a shorter leaf that is only a leftover fragment, modifier, or generic noun rather than a complete name.
- `P10 Cargo-Cult Nesting`
  Add another module level because the lint suggested nesting, but choose the wrong hierarchy or create nesting that carries no new meaning.
- `P11 Cargo-Cult Flattening`
  Flatten everything to a shallower parent even when the child facet was the real semantic signal that should have stayed visible.
- `P12 Partial Family Fix`
  Fix one item, one sibling, or one import site in a family while leaving the rest in the old shape, making the surface less consistent.
- `P13 Tautological Wrapper Or Alias`
  Introduce a type alias or wrapper that preserves the same raw contract, such as `type ClientId = String` or a tuple struct with no validation or semantics.
- `P14 Parse Or Validate Later`
  Keep the raw boundary as-is and defer the real modeling work deeper into the workflow.
- `P15 Hide The Raw Contract`
  Move the same raw field or parameter into a config struct, helper, options bag, builder, or wrapper object and call it fixed.
- `P16 Trade One Raw Scalar For Another`
  Change `String` to `Box<str>`, `Arc<str>`, `SecretString`, `Cow<'_, str>`, `u64` to `usize`, or one primitive width to another without adding semantic modeling.
- `P17 Treat A Prompt As A Command`
  Read an advisory or design prompt as a mandatory rewrite instead of a suggestion that still needs human judgment.
- `P18 Misread An Analysis Boundary`
  Treat an unsupported-construct diagnostic as a rewrite request rather than a warning that the tool can't see that scope authoritatively.
- `P19 Overfit To The Current Corpus`
  Cargo-cult the exact names or wrappers seen in one workspace instead of following the more general shape the lint is trying to enforce.
- `P20 Rename Around The Reason`
  Satisfy the literal text of the lint while preserving the same underlying lie the lint was trying to point out.

## Lint-Family Catalog

Each section captures the spirit of the lint, representative examples from the `eran_codes` run, and the ways that lint is most likely to be "fixed" incorrectly.

### `internal_catch_all_module`

Representative example:

- `service` in `crates/libs/sensitive-runtime/src/lib.rs`

Spirit:

- A bucket module is making item names carry meaning the path should carry.

Likely misinterpretations:

- `P03` Rename `service` to another weak bucket like `helpers`, `runtime`, `core`, `types`, or `ops`.
- `P08` Split the bucket into several other weak buckets instead of naming the real domain or facet boundary.
- `P20` Keep the weak module and make item names even longer so the module can stay semantically empty.
- `P11` Flatten everything into the parent even if the parent is now carrying unrelated concerns.
- `P07` Add a better semantic module but keep the original weak bucket as an equally valid entrypoint.

### `internal_flat_namespace_preserving_module`

Representative examples:

- `trace_log::filter_query -> trace_log::filter::query`
- `sensitive::record_store -> sensitive::record::store`
- `sensitive::token_store -> sensitive::token::store`

Spirit:

- The flat compound module name is hiding a child facet that should stay visible in the path.

Likely misinterpretations:

- `P02` Keep the flat module and compensate by renaming items inside it to longer compound names.
- `P10` Add nesting on the wrong axis, such as `trace_log::query::filter`, just because two tokens exist.
- `P06` Add a nested re-export or alias while keeping the flat compound module as the real home.
- `P08` Insert a weak middle layer like `helpers`, `detail`, or `impl` instead of the semantic parent the lint is pointing to.
- `P12` Fix only one of several sibling compound modules, leaving the family split between flat and nested shapes.

### `internal_organizational_submodule_flatten`

Representative examples:

- `errors`
- `request`
- `response`

Spirit:

- Pure category modules make names carry the burden instead of the path.

Likely misinterpretations:

- `P03` Rename `response` to `responses`, `wire`, `dto`, or `models` and keep the same category burden.
- `P02` Keep `response` and only shorten or lengthen the items inside it.
- `P07` Add parent re-exports but keep the category module as the real path everyone still uses.
- `P11` Flatten unrelated families together just to remove the category segment.
- `P08` Replace one organizational bucket with another organizational bucket.

### `internal_redundant_category_suffix`

Representative examples:

- `response::TokenRefreshResponse -> response::TokenRefresh`
- `response::RecordsPageResponse -> response::RecordsPage`
- `response::RemoteErrorResponse -> response::RemoteError`

Spirit:

- The parent path already carries the repeated category, so the leaf should not repeat it unless the shorter leaf would become unclear.

Likely misinterpretations:

- `P04` Blindly strip the suffix everywhere, even when the resulting leaf becomes vague or collides with another concept.
- `P05` Keep the original item and add a local alias or import rename instead of changing the owning item or surface.
- `P03` Weaken the parent module and then use the long suffix as compensation.
- `P07` Add the short name as a second alias but keep the long form as an equally endorsed surface.
- `P20` Rename the item to another longer category-bearing variant like `TokenRefreshPayload` instead of answering the path problem.

### `namespace_flat_use_preserve_module`

Representative examples:

- `error::BindSensitiveProviderListenerSnafu`
- `error::ParseSensitiveProviderBaseUrlSnafu`

Spirit:

- The child module is a real facet, so flattening the import hides useful role information at the call site.

Likely misinterpretations:

- `P05` Keep the flat import and add a local alias that bakes the missing facet back into the leaf.
- `P11` Replace the explicit preserved import with a glob or broader prelude import that hides the facet even more.
- `P06` Add a flat re-export one level up instead of importing through the real facet.
- `P20` Rename the imported item to carry `Error`, `Query`, or `Http` in the leaf while the path stays flat.
- `P12` Fix only a couple of imports and leave the rest of the file or module flattened.

### `namespace_flat_use_redundant_leaf_context`

Representative example:

- `ViewerResolutionOutcome -> viewer::ResolutionOutcome`

Spirit:

- The imported leaf is carrying context the child module path could already carry.

Likely misinterpretations:

- `P02` Rename the defining item to the shorter leaf but keep importing it flat.
- `P05` Use `as ResolutionOutcome` on the flat import and leave the missing path context hidden.
- `P09` Shorten the leaf to something fragmentary or overly generic without checking whether the path is now clear enough.
- `P08` Introduce a weak module just to host the shorter leaf.
- `P12` Fix one import site while other flat imports of the same family keep the old shape.

### `namespace_parent_surface`

Representative examples:

- `response::RemoteErrorResponse -> sensitive_boundary::RemoteErrorResponse`
- `response::RecordsPageResponse -> sensitive_boundary::RecordsPageResponse`
- `response::TokenRefreshRequest -> sensitive_boundary::TokenRefreshRequest`
- `response::TokenRefreshResponse -> sensitive_boundary::TokenRefreshResponse`

Spirit:

- A readable parent surface already exists, so callers should not keep reaching through an implementation child module.

Likely misinterpretations:

- `P06` Add the parent re-export but keep all real callers on the child path.
- `P07` Treat both parent and child paths as equally canonical forever.
- `P11` Move the item physically into the parent module and lose the child implementation organization.
- `P05` Fix only one import site with an alias instead of moving callers to the parent surface.
- `P20` Rename the child item or child module so the bypass is less obvious while the same bypass still exists.

### `namespace_redundant_qualified_generic`

Representative examples:

- `domain::user::User -> domain::User`
- `crate::views::partials::button::Button -> views::Button`

Spirit:

- The qualifier is carrying context the nearer parent surface could carry more cleanly.

Likely misinterpretations:

- `P05` Silence the lint with a local `use ... as User` instead of creating or using the real parent surface.
- `P07` Add a shallow alias or wrapper path but keep the old longer path equally canonical.
- `P02` Rename `User` to `DomainUser` or something similar so the longer qualified path can stay.
- `P11` Drop all qualification and use bare `User`, losing the semantic path entirely.
- `P19` Apply the same logic to external crates by inventing local aliases that mimic parent surfaces the crate doesn't actually expose.
- `P20` Create a "fix" that preserves the same shape, such as `domain::users::User` or `views::partials::Button`.

### `api_candidate_semantic_module`

Representative examples:

- `provider::{BoundaryMeta, Client, FailureKind, Operation, Records, Token}`
- `runtime::{HeartbeatStatus, Owner, StatusProof, StatusRecord}`
- `demo::{chat, db}`
- `sandbox::{BaseUrl, ClientId, HttpConfig}`
- `token_refresh::{Request, Response}`

Spirit:

- A public family looks like it wants one shared semantic module surface rather than repeating the family marker in each leaf.

Likely misinterpretations:

- `P17` Treat the advisory as an automatic rewrite even when the resulting paths aren't clearly better.
- `P10` Cargo-cult the suggested module extraction while keeping weak or awkward inner names.
- `P12` Extract only part of the family, leaving the public surface split between flat and modular forms.
- `P20` Create the module but keep names like `token_refresh::TokenRefreshRequest` or `sandbox::SandboxClientId`.
- `P08` Replace the family with a weak module like `types`, `shared`, or `data` instead of the semantic module the lint is pointing toward.
- `P07` Add the semantic module as a second path but leave the old flat family equally blessed.
- `P19` Rename unrelated items just to manufacture a family that fits the heuristic.

### `internal_candidate_semantic_module`

Representative examples:

- `flow::{create_room, join_room, list_messages, moderate_message, post_message}`
- `input::{moderation, post}`
- `post::{flow, input}`
- `card::{result, status}`
- `local_tab::{panel, root}`

Spirit:

- An internal sibling family looks like it wants one shared semantic module surface instead of repeated heads or tails.

Likely misinterpretations:

- `P17` Treat the advisory as mandatory instead of a design prompt.
- `P10` Extract a module layer everywhere any family resemblance exists.
- `P12` Restructure one corner of the family and leave the rest inconsistent.
- `P20` Keep the repeated token in the inner leaves, such as `flow::LoginFlow`.
- `P08` Replace the repeated family with an organizational bucket rather than a semantic one.
- `P07` Add the candidate module as an alternate path while internal callers still use the old flat names.

### `api_candidate_semantic_module_unsupported_construct`

Representative examples:

- scopes containing item macros in `domain/src/lib.rs`, `domain/src/sensitive/mod.rs`, `domain/src/user/mod.rs`, and `http/src/request.rs`

Spirit:

- The tool hit an analysis boundary. It says the current pass couldn't see enough to judge this scope cleanly, so it shouldn't be read as a rewrite recommendation.

Likely misinterpretations:

- `P18` Treat the warning as if the code is structurally wrong rather than partially unseen.
- `P17` Apply the semantic-module extraction anyway without checking the expanded or cfg-sensitive surface.
- `P01` Suppress the warning and then treat the absence of a family lint as proof no family exists.
- `P19` Force the inferred family from nearby patterns in this corpus without checking what the macro or include actually expands to.
- `P20` Rewrite macros, includes, or cfg-driven definitions into hand-written source just so the current pass can see them, even if that makes the real code worse.

### `api_raw_id_surface`

Representative example:

- `SensitiveSandbox.client_id`

Spirit:

- A caller-facing id should usually be a focused boundary type, not a raw string or integer.

Likely misinterpretations:

- `P02` Rename `client_id` to something more specific while keeping it raw.
- `P13` Introduce `type ClientId = String` or a tuple wrapper with no validation, parsing, or semantic distinction.
- `P15` Move the raw id into another config or options object and call the boundary fixed.
- `P14` Keep the raw id at the boundary and only parse it later in the workflow.
- `P16` Change the storage wrapper to `SecretString`, `Box<str>`, or another string container without adding an id type.
- `P20` Use a generic catch-all id wrapper like `Id` or `Identifier` that erases which system or domain the id belongs to.
- `P19` Reuse a repo type that happens to have `Id` in the name but models a different domain or protocol.

### `api_semantic_numeric_scalar`

Representative examples:

- `provider_stub_port`
- `timeout_secs`
- `default_provider_port`

Spirit:

- The boundary name already encodes unit or domain meaning, so a raw integer is too weak for the API shape.

Likely misinterpretations:

- `P02` Rename `timeout_secs` to `timeout` or `provider_stub_port` to `port_number` while keeping the raw integer.
- `P13` Introduce `type TimeoutSecs = u64` or `type Port = u16` without any stronger modeling.
- `P15` Hide the raw number inside a helper or config wrapper but keep the same caller-facing contract.
- `P16` Change one integer width to another and treat that as typed modeling.
- `P14` Parse or convert later instead of exposing the stronger type at the boundary.
- `P20` Add doc comments about units while leaving the raw scalar surface unchanged.
- `P19` Pick a stronger type that doesn't actually match the intended meaning, such as any numeric wrapper just because it is available.

### `api_semantic_string_scalar`

Representative example:

- `SensitiveSandbox.base_url`

Spirit:

- The boundary name suggests a domain concept like URL, email, or path, so a raw string is too weak.

Likely misinterpretations:

- `P02` Rename `base_url` to `endpoint` or another nearby word while keeping `String`.
- `P13` Introduce `type BaseUrl = String` or a wrapper that never validates or parses.
- `P15` Move the raw string into another boundary object and call the modeling fixed.
- `P14` Parse or validate only after the public boundary, leaving the boundary itself stringly typed.
- `P16` Switch to another string container and treat that as a typed boundary.
- `P20` Keep the raw contract and only rename the constructor, field, or docs around it.
- `P19` Grab the wrong focused type instead of the actual URL/path/email boundary the code wants.

### `internal_redundant_leaf_context`

Representative examples:

- `viewer::ViewerResolutionState -> viewer::ResolutionState`
- `viewer::ViewerResolutionFlow -> viewer::ResolutionFlow`
- `viewer::ViewerResolutionOutcome -> viewer::ResolutionOutcome`
- `config::SandboxHttpConfig -> config::SandboxHttp`

Spirit:

- The parent path is already carrying context the leaf is repeating, so the leaf can often get shorter if the shorter path still reads clearly.

Likely misinterpretations:

- `P04` Blindly strip the repeated token without checking whether the shorter leaf is now vague or collides with another concept.
- `P09` Collapse the leaf to a fragment or overly generic noun.
- `P03` Weaken the parent and keep the shorter leaf, making the overall path less informative.
- `P05` Fix one import or local alias while leaving the owning item name unchanged.
- `P12` Shorten one member of a family but leave adjacent members long.
- `P10` Add a bogus extra module level only to justify the shorter leaf.

## Cross-Lint Failure Modes Worth Calling Out Explicitly

These are recurring ways an agent can "satisfy" many different lints while still violating `modum`'s point.

- `Mechanical symmetry`
  Apply the same kind of rename or nesting move everywhere because it worked once, without checking whether the parent path is actually stronger afterward.
- `Surface drift`
  Add a nicer surface but leave the old surface just as endorsed, so callers never converge.
- `Path laundering`
  Move the same semantics into aliases, wrappers, or helper objects so the raw or repetitive shape is still present, only one layer further away.
- `Prompt confusion`
  Treat advisories and unsupported-analysis warnings as if they were mandatory structural rewrites.
- `One-hop thinking`
  Fix the exact line that triggered the warning instead of deciding what the canonical owning surface should be for the whole family.
- `Repo-local cargo culting`
  Copy the exact wrapper, module name, or surface from the current corpus instead of applying the more general rule to whatever codebase `modum` is linting.
