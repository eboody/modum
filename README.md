# modum

**modum** is a proc-macro attribute that rewrites one named Rust item into a generated module path:

`NameLikeThis` -> `name::LikeThis` (or `name::like_this` / `name::LIKE_THIS` depending on item kind).

## Why Use modum?
- **Reduce top-level namespace noise** by grouping related items under lightweight modules.
- **Keep call-sites readable** with stable `module::item` access patterns.
- **Make naming conventions explicit** across types, functions, and constants.
- **Stay compile-time only**: no runtime cost, just generated Rust.

## The Problem modum Solves

In medium/large crates, top-level items pile up quickly:
- similar prefixes repeated across many items (`UserCreate`, `UserDelete`, `UserList`, ...)
- naming collisions between constants/functions/types
- uneven style drift between Pascal/camel/snake naming sources

`modum` turns the first semantic segment into a namespace module, so names become easier to scan and group:

- `UserCreate` -> `user::Create`
- `userCreate` -> `user::create` (for `fn`)
- `USER_TOTAL` -> `user::TOTAL` (for `const`/`static`)

## How modum Works

`#[modum]`:
1. Parses the item identifier into segments (`PascalCase`, `camelCase`, or `snake_case`).
2. Uses the **first segment** as the generated module name (lowercase).
3. Uses the **remaining segments** as the inner item name.
4. Re-emits your item as:

```rust
pub mod <head> {
    pub <item renamed to tail>;
}
```

The original top-level item name is removed. The generated module path is the API.

## Table of Contents
- [Quick Start](#quick-start)
- [Naming Model](#naming-model)
- [Supported Items](#supported-items)
- [Examples](#examples)
- [Common Errors and Tips](#common-errors-and-tips)
- [API Reference](#api-reference)
- [Limitations](#limitations)

## Quick Start

```rust
use modum::modum;

#[modum]
pub struct WhatEver;

#[modum]
pub fn myFunction() -> u32 {
    7
}

#[modum]
pub const STATE_TOTAL: usize = 22;

fn demo() {
    let _ = what::Ever;
    let _ = my::function();
    let _ = state::TOTAL;
}
```

## Naming Model

### Input Styles
- `PascalCase`
- `camelCase`
- `snake_case`

### Split Rule
- First segment -> module name (lowercase)
- Remaining segments -> tail item name

### Tail Casing by Item Kind
- `struct`, `enum`, `trait`, `type`, `union` -> `PascalCase`
- `fn` -> `snake_case`
- `const`, `static` -> `SCREAMING_SNAKE_CASE`

### Examples
- `HTTPServer` (`struct`) -> `http::Server`
- `myHTTPServer` (`fn`) -> `my::http_server`
- `state_total` (`const`) -> `state::TOTAL`
- `STATE_TOTAL` (`const`) -> `state::TOTAL`

## Supported Items

`#[modum]` supports:
- `struct`
- `enum`
- `trait`
- `type`
- `union`
- `fn` (free function)
- `const`
- `static`

Unsupported item kinds fail with a compile-time error.

## Examples

### Structs and Enums

```rust
use modum::modum;

#[modum]
pub struct ReviewState;

#[modum]
pub enum HTTPStatus {
    Ok,
    Err,
}

fn demo() {
    let _ = review::State;
    let _ = http::Status::Ok;
}
```

### Functions

```rust
use modum::modum;

#[modum]
pub fn myFunction() -> u32 {
    1
}

fn demo() {
    let _ = my::function();
}
```

### Const and Static

```rust
use modum::modum;

#[modum]
pub const STATE_TOTAL: usize = 22;

#[modum]
pub static METRIC_SUM: usize = 99;

fn demo() {
    let _ = state::TOTAL;
    let _ = metric::SUM;
}
```

### Traits and Type Aliases

```rust
use modum::modum;

#[modum]
pub trait RequestHandler {
    fn handle(&self) -> u8;
}

#[modum]
pub type AppList<T> = Vec<T>;

struct Worker;

impl request::Handler for Worker {
    fn handle(&self) -> u8 {
        3
    }
}
```

## Common Errors and Tips

- **Single-segment names are rejected**:
  - `Foo` or `foo` do not have enough segments.
  - Use at least two segments, like `FooBar`, `foo_bar`, `fooBar`.

- **Macro ordering with derives/attr macros matters**:
  - Prefer placing `#[modum]` above `#[derive(...)]` and similar attribute macros.
  - Example:
    ```rust
    #[modum]
    #[derive(Debug, Clone)]
    pub struct MetaData;
    ```

- **Duplicate generated module names will conflict**:
  - Two items whose first segment is the same in the same scope both generate the same module.
  - Example: `app_value` and `app_state` both try to create `mod app`.

- **Original top-level name no longer exists**:
  - After rewrite, use `module::Item` only.

## API Reference

### `#[modum]`

Apply to one supported named item.

```rust
use modum::modum;

#[modum]
pub struct PascalCase;
```

Generated shape:

```rust
pub mod pascal {
    pub struct Case;
}
```

Rules:
- No macro arguments are accepted (`#[modum(...)]` is invalid).
- Item attributes (other than `#[modum]`) are preserved.
- Visibility is preserved on both generated module and inner item.

## Limitations

- Only supported on the item kinds listed above.
- Does not currently merge multiple transformed items into one generated module.
- Renaming/moving into a module can affect privacy and path resolution in edge cases.

## License

MIT
