# Changelog

All notable changes to QuantaLang will be documented in this file.

## [1.0.0] - 2026-03-22

### Language Features
- Generics with trait bounds and where clauses
- Pattern matching with exhaustiveness checking
- Closures with capture semantics
- Algebraic effects and effect handlers
- Built-in color space types (sRGB, Linear, ACES, Oklab, HSL, HSV)
- Ownership and borrowing system
- Module system with visibility controls
- Macro system with hygiene

### Compiler
- C backend (stable, primary target)
- HLSL shader output
- GLSL shader output
- SPIR-V binary shader output
- x86-64 native backend (experimental)
- AArch64 native backend (experimental)
- WASM backend (experimental)
- LLVM IR backend (experimental)
- 8 total code generation backends

### Tooling
- LSP server with completion, hover, and diagnostics
- VS Code extension with syntax highlighting and LSP integration
- CLI (`quantac`) with lex, parse, check, build, and run subcommands
- Package manager (`quanta pkg`) with dependency resolution
- Code formatter (`quanta fmt`)

### Known Limitations
- Non-C backends (x86-64, AArch64, WASM, LLVM) are experimental and may not support all language features
- Package manager is not connected to a live registry
- Formatter is not wired into the CLI pipeline
