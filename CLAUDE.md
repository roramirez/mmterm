@doc/LLMs.md

## Project Rules

- Every feature or bug fix must be accompanied by tests covering the new or changed behavior.
- Tests live in separate `*_test.rs` files alongside the source file, linked with `#[cfg(test)] #[path = "..."] mod tests;`.
- Run `cargo test` before reporting a task as complete.
