@doc/LLMs.md

## Project Rules

- Every feature or bug fix must be accompanied by tests covering the new or changed behavior.
- Tests live in separate `*_test.rs` files alongside the source file, linked with `#[cfg(test)] #[path = "..."] mod tests;`.
- Run `cargo test` before reporting a task as complete.
- For keybinding changes: test modifier combinations beyond the happy path — e.g. if adding Shift+X, also test Ctrl+Shift+X, Alt+Shift+X, and the same key in every `InputMode`. Modifier interactions often produce surprising fall-through behavior that only a combined test will catch.
