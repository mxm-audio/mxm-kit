# Contributing

Issues and pull requests are welcome.

- **Read the `AGENTS.md` chain first** — the root, then the one in each folder you touch. They are the
  working rules: what each crate owns, what it must not do, and how it is checked.
- **One rule, written once.** The kit exists so every instrument shares one implementation of each
  rule; a per-instrument variant of a shared rule is a defect, not a feature.
- **All three platforms.** Windows, macOS and Linux; anything platform-specific is `cfg`-gated with
  every arm implemented.
- **Check before you send:**

  ```bash
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```

By contributing you agree that your contribution is licensed under the MIT licence of this
repository.
