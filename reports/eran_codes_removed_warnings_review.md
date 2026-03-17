# Removed Warning Review

Compared:

- old report: `/home/eran/code/eran_codes/modum-lint-report.txt`
- new report: `/tmp/eran_codes_modum_after.txt`

Removed warnings: `76`

Lint counts:

- `namespace_flat_use_redundant_leaf_context`: `42`
- `namespace_flat_pub_use_redundant_leaf_context`: `19`
- `namespace_flat_use`: `8`
- `namespace_flat_use_preserve_module`: `7`

Important caveat:

- The old report file is from March 11, 2026.
- The new run is from March 12, 2026.
- A few removed lines may be due to source drift in `eran_codes` between runs, not just linter behavior.

## Confirmed Good Removals

These removed warnings should stay removed. The old rule was producing nonsense suggestions.

### Relative-import noise

Count: `8`

Examples:

- `prefer super::Error` in `app/src/chat/join_room_flow.rs`
- `prefer super::Error` in `app/src/user/register_user_flow.rs`
- `prefer super::Error` in `src/config.rs`

Assessment:

- These were tautologies.
- `super` is not a meaningful public namespace.
- The warning was not actionable.

### Non-binding imports and keywords

Count: `5`

Examples:

- `use std::error::Error as _;` -> `prefer error::Error`
- `auth::self`

Assessment:

- `_` imports are trait-enablement imports, not namespace-shaping imports.
- `self` is not a leaf binding.
- These were parser/analysis bugs, not policy signals.

### Mechanical stdlib / external-name shortening

Count: `26`

Examples:

- `IntoResponse` -> `response::Into`
- `FromStr` -> `str::From`
- `SystemTime` -> `time::System`
- `AtomicUsize` -> `atomic::Usize`
- `HeaderName` -> `header::Name`
- `PathBuf` -> `path::Buf`
- `RefCell` -> `cell::Ref`

Assessment:

- These were exactly the kind of mechanical rewrites the naming guide says to avoid.
- The shortened forms are worse than the originals.
- These should remain suppressed.

## Removed Warnings That Are Reasonable To Leave Off For Now

These removals are not as clear-cut as the cases above, but the old rule was still too aggressive. It was inventing shorter names without enough evidence that the shorter path was actually canonical or clearer.

### Re-export overlap guesses

Count: `19`

Examples:

- `MessageBody` -> `message::Body`
- `RoomName` -> `room::Name`
- `SseRegistry` -> `sse::Registry`
- `DemoState` -> `state::Demo`
- `AuthShellVariant` -> `auth_shell::Variant`
- `StatusCardItem` -> `status_card::Item`

Assessment:

- Some of these might be good API changes.
- Some are just style preferences.
- The previous lint was too eager because it treated any prefix/suffix overlap as redundant.
- Keeping these removed is acceptable until there is a stronger signal than string overlap.

### Descriptive alias guesses

Count: `16`

Examples:

- `domain_chat` -> `domain::chat`
- `SseHandle` -> `sse::Handle`
- `TraceLayer` -> `trace::Layer`
- `PageEvent` -> `page::Event`
- `InfraConfig` -> `config::Infra`
- `WorkCaseContent` -> `content::WorkCase`

Assessment:

- These are closer to real readability issues than the stdlib cases.
- The problem is that the old lint still guessed the replacement mechanically.
- Some of these likely belong in a future lint, but not under the old heuristic.

## What This Audit Suggests Next

The removals point to a missing lint, not just a bad one.

The missing rule is:

- when a parent module or crate already defines the canonical readable surface, prefer that canonical surface over reaching into an internal child module

Examples of the intended shape:

- prefer `http::Error` over `error::Error`
- prefer `http::Result` over `error::Result`
- prefer `user::Error` over `error::Error`

That is different from the removed heuristic.

Why it needs a separate lint:

- it depends on explicit exported module surfaces
- it should follow real parent-module structure, not string chopping
- it needs to know whether the parent actually exposes the canonical item

## Bottom Line

The removed warnings split into three buckets:

- definitely bad and should stay gone: relative imports, `_`/`self`, stdlib/external mechanical shortening
- ambiguous and fine to leave off for now: many public re-export overlap guesses
- promising candidates for a new rule: internal imports that bypass a canonical parent surface such as `http::Error`

Raw removed warning list:

- `/tmp/modum_removed_warnings.txt`
