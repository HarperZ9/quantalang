# QuantaLang Architecture

## Overview

QuantaLang -- "The Effects Language" -- has a working Rust-based compiler and a large body
of aspirational self-hosted code. This document explains what each directory contains and
what actually works today.

## `compiler/` -- The Real Compiler (Rust)

This is the working implementation. It compiles QuantaLang source to C99, invokes a system
C compiler (gcc/clang/MSVC), and produces native executables.

**Pipeline:** Lexer -> Parser -> Type Checker -> MIR -> C Backend -> Executable

What works end-to-end:
- Variables, functions, control flow (if/else, while, match)
- Structs, enums, pattern matching, recursion
- Algebraic effects (define, perform, handle) via setjmp/longjmp C runtime
- 591 tests across unit and integration suites

What exists but is not wired into the CLI:
- x86-64, ARM64, WASM, LLVM, SPIR-V backends (tested in isolation)
- LSP server, code formatter, package manager (implemented but no subcommands)

Key paths:
- `compiler/src/lexer/` -- tokenizer
- `compiler/src/parser/` -- recursive descent + Pratt parsing
- `compiler/src/types/` -- Hindley-Milner inference, effect tracking
- `compiler/src/codegen/` -- MIR builder, C backend, native backends
- `compiler/src/lsp/` -- language server
- `compiler/src/fmt/` -- code formatter
- `compiler/src/pkg/` -- package manager
- `compiler/tests/` -- integration tests

CLI today:
```
quantac lex <file>       # Tokenize and print tokens
quantac parse <file>     # Parse and print AST
quantac check <file>     # Type-check
quantac build [path]     # Compile to C -> native executable
quantac run <file>       # Compile and run
quantac repl             # Interactive REPL
quantac version          # Print version
```

## `future/` -- Aspirational Self-Hosted Compiler

**The self-hosted compiler in `future/` is a design document expressed as code. It
represents the future vision but cannot be compiled by the current compiler.**

This directory contains ~268K lines of `.quanta` code: a complete self-hosted compiler,
standard library, and test suite. None of it compiles or executes. The Rust compiler does
not yet support the module system, import syntax, generics, or standard library that this
code relies on.

Key paths:
- `future/self-hosted-compiler/` -- self-hosted compiler (lexer, parser, AST, HIR, MIR,
  codegen for x86_64/AArch64/WASM, driver, LSP, package manager, formatter, linter,
  doc generator, test framework, build system)
- `future/stdlib/` -- standard library (core, alloc, std -- modeled after Rust's stdlib)
- `future/tests/` -- test suite for the self-hosted compiler
- `future/release/` -- release packaging

This code is valuable as a specification and roadmap. It defines what QuantaLang's syntax,
standard library, and tooling should look like when the language is capable of self-hosting.

## `quantalang/quantalang/` -- Examples and Documentation

What remains in this directory:
- `examples/` -- example programs (effects demos, HTTP server, CLI tool, concurrency)
- `docs/` -- language specification, guides, tutorials, API docs
- `scripts/` -- installer
- `website/` -- project website
- `STATUS.md`, `ASPIRATIONAL.md` -- status tracking

## `tests/` -- Working Test Programs

Test programs that compile and run with the current Rust compiler. Each `.quanta` file has
a matching `.expected` file with expected output.

Programs (all verified working):
- `01_hello.quanta` through `17_higher_order.quanta`
- Covers: hello world, variables, functions, if/else, while loops, recursion, nested
  functions, arithmetic, boolean logic, multiple returns, structs, enums, strings, arrays,
  methods, closures, higher-order functions

Run all tests: `./tests/run_tests.sh`

## `src/` -- Top-Level Source

Contains additional QuantaLang source files (lexer, parser, stdlib, VM) at the project
root level. Like `quantalang/quantalang/`, these are aspirational and do not compile with
the current compiler.

## Line Counts

| Directory | Lines | Language | Status |
|---|---|---|---|
| `compiler/src/` | ~61,700 | Rust | Working core, partial tools |
| `compiler/tests/` | ~1,500 | Rust | Working |
| `tests/programs/` | 17 programs | QuantaLang | Working (compile and run) |
| `future/self-hosted-compiler/` | ~231,700 | QuantaLang | Aspirational |
| `future/stdlib/` | ~28,600 | QuantaLang | Aspirational |
| `future/tests/` | ~8,200 | QuantaLang | Aspirational |
