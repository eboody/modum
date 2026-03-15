# AGENTS.md

## Naming
For new or renamed modules, items, and public API paths, follow `docs/naming-guide.md`.

Default expectations:
- Prefer semantic module paths over flat prefixes.
- Do not force a single module level; use nested modules when each segment adds real meaning.
- Prefer concise public leaf names only when the parent path is already descriptive.
- If the parent path is weak or purely technical, keep more specificity in the item name or choose a better module path.
- Use `pub use ... as ...` when internal names need more context but the public API should stay shorter.
- Prefer import styles that keep the namespace visible at call sites.
