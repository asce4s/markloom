---
name: Rust Best Practices
description: "Use when writing or reviewing Rust code. Covers ownership idioms, cargo tooling, error handling with Result/Option, module layout, and testing conventions."
---

# Rust Best Practices

Rust trades runtime cost for compile-time safety. The borrow checker is not an obstacle -- it is the design tool. Code that compiles is memory-safe by construction.

## Idioms & Conventions

- **Prefer borrowing over cloning.** Accept `&str` instead of `String`, `&[T]` instead of `Vec<T>` in function parameters. Clone only when ownership is genuinely needed.
- **Use `impl Trait` in argument position** to accept any type satisfying a bound without forcing callers into generics: `fn process(reader: impl Read)`.
- **Leverage pattern matching exhaustively.** Match all enum variants explicitly; avoid `_ =>` catch-alls on enums you control so the compiler flags new variants.
- **Builder pattern for complex construction.** Use `Default::default()` plus method chaining instead of constructors with many parameters.
- **Newtype pattern for type safety.** Wrap primitives (`struct UserId(u64)`) to prevent mixing up IDs, indices, and counts.
- **Lifetimes: elision first.** Only annotate lifetimes when the compiler asks. When you must, keep them on the struct, not scattered across methods.

## Tooling

| Tool | Purpose |
|---|---|
| `cargo build` / `cargo run` | Build and run |
| `cargo clippy` | Lint -- treat warnings as errors in CI (`-- -D warnings`) |
| `cargo fmt` / `rustfmt` | Format -- enforce in CI with `--check` |
| `cargo doc --open` | Generate and browse docs |
| `cargo audit` | Check dependencies for known vulnerabilities |
| `rust-analyzer` | IDE support (LSP) |

## Project Structure

```
src/
  main.rs          # Binary entry point
  lib.rs           # Library root (re-exports public API)
  config.rs        # Configuration types
  db/
    mod.rs         # Module declaration
    client.rs      # Database client
  api/
    mod.rs
    routes.rs
  services/
    mod.rs
tests/              # Integration tests (separate compilation unit)
benches/            # Benchmarks
```

Declare modules in `mod.rs` or use the file-as-module convention (`db.rs` + `db/` directory). Pick one style per project.

## Error Handling with Result/Option

- **Use `?` for propagation.** Never `.unwrap()` in library code.
- **Define domain error enums** with `thiserror` for libraries, `anyhow` for applications.
- **`Option` is not an error.** Use it for "might not exist" cases. Convert to `Result` at API boundaries with `.ok_or_else(|| ...)`.
- **Map errors at boundaries:** `map_err` to convert between error types rather than leaking implementation details.

```rust
#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("not found: {0}")]
    NotFound(String),
}
```

## Testing

- **Unit tests live in the same file** inside `#[cfg(test)] mod tests { ... }`. This gives access to private functions.
- **Integration tests go in `tests/`.** Each file is a separate crate -- test only the public API.
- **Use `#[should_panic]`** for expected failure cases, `#[ignore]` for slow tests.
- **Test fixtures:** use `tempfile` for filesystem isolation, `wiremock` for HTTP mocking.
- **Run with:** `cargo test`, `cargo test -- --nocapture` for stdout, `cargo test test_name` for a single test.

## Common Pitfalls

| Pitfall | Fix |
|---|---|
| Fighting the borrow checker by cloning everything | Restructure to use references; split borrows across scopes |
| Using `unwrap()` in production code | Use `?`, `expect("reason")`, or proper error types |
| Blocking async runtime with sync I/O | Use `tokio::task::spawn_blocking` for CPU/IO-bound work |
| Unused `Result` -- silently ignoring errors | Compiler warns; never `let _ = fallible_call()` without justification |
| Over-generic code | Start concrete, generalize only when you have 2+ call sites |

## Related Framework Skills

| Framework | Official Skill? | Status |
|-----------|:---:|--------|
| **Axum** | No | Gap — consider creating `axum-patterns` |
| **Actix Web** | No | Gap |
| **Tokio** | No | Gap — async runtime patterns |

## Attribution
**Original** — Datatide, MIT licensed.
