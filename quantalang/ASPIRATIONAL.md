# Self-Hosted QuantaLang — Compilation Target

This directory contains a complete self-hosted compiler, standard library, and toolchain
written in QuantaLang itself. **Most of this code cannot yet be compiled by the current
Rust-based compiler.**

It serves as the **compilation target** — as the Rust compiler gains features (trait dispatch,
module imports, standard library), more of this code becomes compilable. The goal is
self-hosting: a QuantaLang compiler that compiles itself.

## Current Compilability

The Rust compiler (in `../compiler/`) currently supports:
- Variables, functions, if/else, loops, match, recursion
- Structs, enums, strings, arrays, methods, closures, generics
- Algebraic effects (handle/perform/resume)
- 18/18 end-to-end test programs verified

Files in this directory that use only those features could potentially be compiled now.
Files that use trait dispatch, module imports, or standard library functions cannot yet.

## Contents
- `src/` — Self-hosted compiler (lexer, parser, AST, type checker, codegen)
- `stdlib/` — Standard library (core, alloc, std modules)
- `tests/` — Test suite for the self-hosted compiler
- `examples/` — Example programs including effect demonstrations
- `docs/` — Language documentation and manifesto

## Why Keep It Here?
This code is not dead weight — it's the specification for what QuantaLang should become.
As the Rust compiler gains features, we progressively compile more of these files,
working toward the ultimate goal: self-hosting.
