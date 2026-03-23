# QuantaLang Project Status

Last audited: 2026-03-21

## Identity
The Effects Language -- algebraic effects as a first-class feature.

## What Works (verified, tested, compiles)
- **Lexer**: Complete Unicode-aware tokenizer with comprehensive token types, spans, error recovery. 59 unit tests + 51 integration tests.
- **Parser**: Full recursive descent with Pratt parsing for expressions. Handles functions, structs, enums, match, if/else, loops, effects, generics, patterns. 4 unit tests + 85 integration tests.
- **Type Checker**: Hindley-Milner inference, effect tracking, unification, trait resolution, const generics, higher-kinded types. Unit tests across multiple files.
- **C Backend**: Generates valid C99 from QuantaLang source. Handles structs, unions, globals, string tables, branching, all binary/unary ops. 11 unit tests. This is the only backend wired into the CLI.
- **Effects**: Parse -> type check -> codegen pipeline (setjmp/longjmp C runtime).
- **Programs that compile**: Variables, functions, if/else, loops, match, recursion, arithmetic, effects -- all compile to C and execute via `quantac build`.
- **Auto-compile**: `quantac build` discovers and invokes system C compiler (gcc/clang/MSVC).
- **CLI subcommands**: `lex`, `parse`, `check`, `build`, `run`, `repl`, `version`.
- **MIR pipeline**: Full MIR builder (codegen/builder.rs, 29 tests), MIR IR (codegen/ir.rs, 31 tests), debug info (codegen/debug.rs, 24 tests), embedded C runtime (codegen/runtime.rs, 7 tests).
- **Macro expansion**: Builtin macros, pattern matching, hygiene. Unit tests present.
- **591 total `#[test]` annotations** (455 in compiler/src/, 136 in compiler/tests/).

## What's Partial (has real code, not fully connected)
- **x86-64 Backend** (1615 lines, 22 tests): Generates assembly and machine code from MIR. Implements `Backend` trait. Not wired into CLI -- no `--target` flag.
- **ARM64 Backend** (1629 lines, 21 tests): Generates assembly and machine code from MIR. Implements `Backend` trait. Not wired into CLI.
- **WASM Backend** (1866 lines, 11 tests): Generates WebAssembly binary from MIR with WASI support. Implements `Backend` trait. Not wired into CLI. No end-to-end .wasm execution test.
- **LLVM Backend** (1915 lines, 11 tests): Generates LLVM IR text from MIR. Implements `Backend` trait. Not wired into CLI. Requires external LLVM tools.
- **SPIR-V Backend** (1898 lines, 7 tests): Generates SPIR-V binary for Vulkan compute. Implements `Backend` trait. Not wired into CLI. No Vulkan validation test.
- **x86-64 Instruction Encoder** (2058 lines, 38 tests): Encodes x86-64 instructions to binary machine code. Works in isolation but no linker/loader to produce executables.
- **ARM64 Instruction Encoder** (2161 lines, 32 tests): Encodes ARM64 instructions to binary. Same limitation.
- **LSP Server** (6448 lines, 24 tests): Full LSP implementation with completion, hover, diagnostics, go-to-definition, symbols, code actions. JSON dispatch uses manual string matching (not serde_json). Only lifecycle messages are dispatched in the server loop. `run_server()` exists but no `quantac lsp` subcommand. Cannot serve a real VS Code session beyond initialize/shutdown.
- **Formatter** (1631 lines, 11 tests): Code formatter with configurable style (indentation, line length, brace style, trailing commas, import organization). Pretty printer with document algebra. Not wired into CLI -- no `quantac fmt` subcommand.
- **Package Manager** (3354 lines, 24 tests): Manifest parsing (Quanta.toml), semver version handling, lockfile generation, dependency resolution, registry client (targets registry.quantalang.org). Not wired into CLI. No registry exists.
- **Runtime: FFI** (1038 lines, 7 tests): Calling convention definitions, type layout, ABI classification. Not used by any code generation backend.
- **Runtime: GC** (786 lines, 4 tests): Reference counting with cycle detection design. Not linked into compiled programs.
- **Runtime: Async** (1216 lines, 6 tests): Work-stealing scheduler design. Not linked into compiled programs. No async/await syntax support.

## What's Aspirational (architecture exists, doesn't function)
- **Self-hosted compiler** (quantalang/src/, 231,688 lines): Complete compiler written in QuantaLang (lexer, parser, AST, types, HIR, MIR, codegen for x86_64/AArch64/WASM, driver, LSP, package manager, formatter, linter, test framework, build system, doc generator). **Cannot be compiled or executed.** The Rust compiler does not support the `.quanta` module system, import syntax, or standard library used by this code.
- **Self-hosted stdlib** (quantalang/stdlib/, 28,649 lines): Core library (Option, Result, Iterator, primitives, memory, pointers), Alloc library (Box, Vec, String, Rc), Std library (fs, thread, sync, net, time, process). Modeled after Rust's standard library. **Cannot be compiled or executed.**
- **Self-hosted test suite** (quantalang/tests/, 8,230 lines): Test framework and test cases for the self-hosted compiler. **Cannot be executed.**

## Honest Line Counts
- Compiler (Rust, `compiler/src/`): 61,695 lines -- STATUS: working core (lexer, parser, types, C backend), partial other backends/tools
- Integration Tests (Rust, `compiler/tests/`): 1,522 lines -- STATUS: working
- Self-hosted compiler (QuantaLang, `quantalang/src/`): 231,688 lines -- STATUS: aspirational, cannot compile
- Self-hosted stdlib (QuantaLang, `quantalang/stdlib/`): 28,649 lines -- STATUS: aspirational, cannot compile
- Self-hosted tests (QuantaLang, `quantalang/tests/`): 8,230 lines -- STATUS: aspirational, cannot execute

## What the CLI Actually Does Today
```
quantac lex <file>       # Tokenize and print tokens
quantac parse <file>     # Parse and print AST
quantac check <file>     # Type-check
quantac build [path]     # Compile to C, invoke C compiler, produce executable
quantac run <file>       # Compile and run
quantac repl             # Interactive REPL
quantac version          # Print version
```

There are no `lsp`, `fmt`, `pkg`, `test`, `doc`, or `lint` subcommands, despite those modules existing in the codebase.

## Summary
QuantaLang has a **working compiler core** (lexer -> parser -> type checker -> MIR -> C backend -> executable) with 591 tests. It can compile and run real programs with variables, functions, control flow, pattern matching, recursion, and algebraic effects. Five additional backends (x86-64, ARM64, WASM, LLVM, SPIR-V) contain real, tested code but are not accessible from the CLI. The LSP, formatter, and package manager are implemented but not wired into the CLI. The self-hosted compiler and standard library (268,567 lines of `.quanta` code) represent an ambitious long-term vision but cannot be compiled or executed today.
