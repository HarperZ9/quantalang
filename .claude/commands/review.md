---
description: Review Rust code for safety, correctness, and best practices
allowed-tools: Read, Grep, Glob, Bash(git diff:*)
---

# Code Review

## Context
- Current diff: `git diff HEAD`
- Staged changes: `git diff --cached`
- Current branch: `git branch --show-current`

## Review Checklist (Rust)

1. **Safety** — No unjustified `unsafe` blocks, no undefined behavior, proper bounds checking
2. **Error Handling** — No `.unwrap()` in library/production code, use `Result<T, E>` and `?` operator, meaningful error types
3. **Lifetimes** — Correct lifetime annotations, no unnecessary `'static`, no dangling references
4. **Clippy** — No clippy warnings, follow idiomatic Rust patterns
5. **Performance** — No unnecessary allocations, prefer `&str` over `String` where possible, use iterators over index loops
6. **Traits** — Proper trait design, derive common traits (Debug, Clone, PartialEq), implement Display for user-facing types
7. **Concurrency** — No data races, proper Send/Sync bounds, safe shared state
8. **Quality Gates** — Files <= 500 lines, functions <= 60 lines, modules properly organized

## Output Format

For each issue found:
- **File**: path/to/file.rs:line
- **Severity**: CRITICAL | WARNING | INFO
- **Issue**: Description
- **Fix**: Suggested change
