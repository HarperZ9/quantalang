---
description: Show project stats — file counts, test counts, build status
allowed-tools: Bash(find:*, cargo:*, wc:*, git:*), Grep, Glob
---

# Project Progress Report

Generate a progress report by running these checks:

## Source Stats
1. Count `.rs` files: `find . -name "*.rs" | wc -l`
2. Count lines of Rust code: `find . -name "*.rs" -exec cat {} + | wc -l`
3. Count `.quanta` example/test files: `find . -name "*.quanta" | wc -l`
4. Count modules: `find . -name "mod.rs" -o -name "lib.rs" | wc -l`

## Test Stats
5. Run `cargo test 2>&1 | tail -5` to get test pass/fail summary
6. Count `#[test]` annotations: `grep -r "#\[test\]" --include="*.rs" | wc -l`
7. Count `#[cfg(test)]` modules: `grep -r "#\[cfg(test)\]" --include="*.rs" | wc -l`

## Build Status
8. Run `cargo check 2>&1 | tail -3` — does it compile?
9. Run `cargo clippy 2>&1 | tail -5` — any warnings?

## TODOs & FIXMEs
10. Count TODOs: `grep -r "TODO" --include="*.rs" | wc -l`
11. Count FIXMEs: `grep -r "FIXME" --include="*.rs" | wc -l`
12. List unsafe blocks: `grep -rn "unsafe" --include="*.rs"`

## Git Stats
13. Current branch: `git branch --show-current`
14. Recent commits: `git log --oneline -10`
15. Uncommitted changes: `git status --short`

## Output
Present results in a clean summary table.
