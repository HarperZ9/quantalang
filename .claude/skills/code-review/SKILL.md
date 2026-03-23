---
name: code-review
description: Perform a thorough Rust code review on the QuantaLang compiler
tools: Read, Grep, Glob, Bash(git diff:*, cargo:*)
---

# Code Review Skill (Rust / QuantaLang)

You are reviewing Rust code for the QuantaLang compiler project. This is a multi-phase compiler with lexer, parser, type checker, and code generator stages.

## Trigger
User asks to review code, a PR, or recent changes.

## Process

### 1. Gather Context
- Run `git diff HEAD` or `git diff --cached` to see changes
- Run `git log --oneline -5` to understand recent work
- Identify which compiler phase(s) the changes affect

### 2. Review by Priority

#### P0 — Safety & Soundness
- [ ] No unjustified `unsafe` blocks
- [ ] No undefined behavior (UB)
- [ ] No unsound type coercions in the type checker
- [ ] Error handling uses `Result`/`?`, not `.unwrap()` in lib code
- [ ] No panics in code paths reachable by user input

#### P1 — Correctness
- [ ] Pattern matches are exhaustive (no catch-all `_` hiding bugs)
- [ ] Lifetime annotations are correct
- [ ] AST visitors handle all node types
- [ ] Parser error recovery doesn't skip important tokens
- [ ] Code generation output matches language semantics

#### P2 — Performance
- [ ] No unnecessary `String` allocations (use `&str` or `Cow`)
- [ ] No unnecessary `.clone()` calls
- [ ] Iterator chains preferred over indexed loops
- [ ] Large enums use `Box` for variant data where appropriate
- [ ] No O(n^2) algorithms where O(n) or O(n log n) would work

#### P3 — Idiomatic Rust
- [ ] `Result`/`Option` used properly (no sentinel values)
- [ ] Traits designed for extensibility
- [ ] Derive macros used where appropriate (Debug, Clone, PartialEq)
- [ ] `impl Display` for user-facing types
- [ ] Public APIs have `///` doc comments

#### P4 — Maintainability
- [ ] No dead code
- [ ] Clear naming conventions
- [ ] Files <= 500 lines
- [ ] Functions <= 60 lines
- [ ] Related code grouped into modules

### 3. Output Format

```
## Code Review: [summary of what was reviewed]

### Critical (must fix before merge)
- File:line — Issue — Fix

### Warnings (should fix)
- File:line — Issue — Fix

### Suggestions (nice to have)
- File:line — Issue — Fix

### Looks Good
- [List of things done well]

**Verdict: APPROVE | REQUEST CHANGES | NEEDS DISCUSSION**
```
