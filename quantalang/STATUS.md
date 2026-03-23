# Status: quantalang/ (Self-Hosted Compiler + Standard Library)

Last audited: 2026-03-21

## Working
- Nothing in this directory can be compiled or executed. The QuantaLang compiler (written in Rust) cannot compile these `.quanta` files. There is no self-hosting capability.

## Partial
- None.

## Aspirational

### Self-Hosted Compiler (`src/`, ~231,688 lines across many files)
- **Lexer** (`lexer.quanta`): Token definitions, keyword lists, literal types. Written in QuantaLang syntax.
- **Parser** (`parser.quanta`): Recursive descent parser with `ParseError` types and `ParseResult`.
- **AST** (`ast.quanta`): AST node definitions.
- **Types** (`types.quanta`): Type system definitions.
- **HIR** (`hir.quanta`): High-level IR definitions.
- **MIR** (`mir.quanta`): Mid-level IR definitions.
- **Codegen** (`codegen.quanta`): Code generation with `Target` enum (x86_64, Aarch64, Riscv64).
- **Codegen x86_64** (`codegen_x86_64.quanta`): x86_64 register definitions, instruction selection, register allocation.
- **Codegen AArch64** (`codegen_aarch64.quanta`): ARM64 code generation.
- **Codegen WASM** (`codegen_wasm.quanta`, `codegen_wasm_mir.quanta`): WebAssembly code generation.
- **Driver** (`driver.quanta`): CLI driver and build system.
- **LSP** (`lsp/`): Language server in QuantaLang (5 files: protocol, types, documents, analysis, server).
- **Package Manager** (`pkg/`): Package management in QuantaLang (7 files: manifest, version, lockfile, resolver, registry, cli).
- **Formatter** (`fmt/`): Code formatter in QuantaLang (5 files: config, printer, visitor, cli).
- **Test Framework** (`test/`): Test runner in QuantaLang (6 files: config, discovery, executor, reporter, cli).
- **Build System** (`build/`): Build system in QuantaLang (6 files: config, graph, cache, compiler, executor).
- **Linter** (`lint/`): Linter in QuantaLang (multiple files).
- **Doc Generator** (`doc/`): Documentation generator (6 files: parser, model, html, markdown, cli).
- **Main** (`main.quanta`): Entry point -- 12 lines, calls `driver::main()`.
- **Lib** (`lib.quanta`): Library root re-exporting all modules.

### Self-Hosted Standard Library (`stdlib/`, ~28,649 lines)
- **Core** (`stdlib/core/`, 9 files): `intrinsics`, `marker`, `primitives`, `option`, `iter`, `cmp`, `ops`, `mem`, `ptr`, `lib`. Modeled after Rust's core library. Includes `Option<T>`, `Result<T,E>`, `Iterator` trait, comparison traits, operator overloading traits, memory/pointer primitives.
- **Alloc** (`stdlib/alloc/`, 6 files): `alloc`, `boxed`, `vec`, `string`, `rc`, `lib`. Heap allocation, `Box<T>`, `Vec<T>`, `String`, `Rc<T>`.
- **Std** (`stdlib/std/`, 9 files): `fs`, `thread`, `sync`, `net`, `time`, `process`, `path`, `env`, `error`, plus `sys/linux.quanta`. OS-level abstractions.

### Test Suite (`tests/`, ~8,230 lines)
- 7 test files: `basic_tests`, `control_tests`, `function_tests`, `type_tests`, `memory_tests`, `codegen_tests`, plus framework/runner infrastructure. **None of these can be executed** since the compiler cannot compile QuantaLang.

## Not Started
- Self-hosting: the Rust-based compiler cannot parse/compile these `.quanta` files.
- Bootstrap chain: no path from Rust compiler to self-hosted compiler.
- Any execution of `.quanta` source files.

## Honest Assessment
This directory contains a **massive, detailed vision** of what a self-hosted QuantaLang ecosystem would look like -- compiler, standard library, package manager, formatter, linter, test framework, build system, doc generator, and LSP server, all written in QuantaLang. The code is syntactically consistent and architecturally coherent. However, **none of it can be compiled or executed**. The Rust-based QuantaLang compiler does not support compiling `.quanta` files that use this module system, import syntax, or standard library. This entire directory is aspirational architecture -- a target to build toward, not working software.

The `Quanta.toml` manifest claims 21,785 lines and lists module line counts, but these are self-reported and the code has grown well beyond that (231,688+ lines in `src/` alone, 28,649 in `stdlib/`).
