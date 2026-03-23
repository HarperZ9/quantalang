---
description: Refactor Rust code with formatting, linting, and size gates
allowed-tools: Read, Write, Edit, Bash(cargo:*, git:*), Grep, Glob
---

# Refactor Workflow

## Pre-flight
1. Run `cargo fmt --check` to see formatting issues
2. Run `cargo clippy -- -W clippy::all` to see lint warnings
3. Identify files exceeding size gates

## Size Gates
- **Files**: <= 500 lines (split into modules if larger)
- **Functions**: <= 60 lines (extract helpers if larger)
- **Structs**: <= 15 fields (consider grouping into sub-structs)
- **Impl blocks**: <= 20 methods (split into trait impls or separate impl blocks)

## Refactor Checklist
1. Run `cargo fmt` to fix formatting
2. Fix all clippy warnings
3. Replace `.unwrap()` with proper error handling (`?`, `.expect()` with context, or match)
4. Replace `.clone()` where borrowing would work
5. Convert `for i in 0..vec.len()` to iterator patterns
6. Extract repeated code into shared functions or traits
7. Ensure public items have doc comments (`///`)
8. Verify `use` statements are organized (std, external crates, local)

## Post-flight
1. Run `cargo check` — must compile
2. Run `cargo test` — no regressions
3. Run `cargo clippy` — zero warnings
4. Run `git diff --stat` to summarize changes
