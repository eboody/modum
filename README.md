# modum

`modum` is a proc-macro that rewrites a named Rust item into `module::item` form.

## Quick examples

```rust
use modum::modum;

#[modum]
pub struct PascalCase;

let _ = pascal::Case;
```

```rust
use modum::modum;

#[modum]
pub fn myFunction() -> u32 {
    1
}

let _ = my::function();
```

## Naming rules

- First segment becomes the module name (lowercase/snake):
  - `HTTPServer` -> `http::Server`
- Remaining segments become the tail item.
- Tail casing is item-kind-aware:
  - `struct`, `enum`, `trait`, `type`, `union` => `PascalCase`
  - `fn` => `snake_case`
  - `const`, `static` => `SCREAMING_SNAKE_CASE`

Single-segment names are rejected (for example `Foo` or `foo`).
