# QuantaLang

[![crates.io](https://img.shields.io/crates/v/quantalang.svg)](https://crates.io/crates/quantalang)
[![docs.rs](https://img.shields.io/docsrs/quantalang)](https://docs.rs/quantalang)
[![VS Code Marketplace](https://img.shields.io/visual-studio-marketplace/v/HarperZ9.quantalang?label=VS%20Code)](https://marketplace.visualstudio.com/items?itemName=HarperZ9.quantalang)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**The Effects Language** — a compiled language for graphics, shaders, and systems programming.

QuantaLang compiles `.quanta` source files to **C** (primary target), **HLSL** and **GLSL** (shader output), with experimental backends for SPIR-V, LLVM IR, WebAssembly, x86-64, and ARM64.

**Landing page:** [harperz9.github.io/quantalang](https://harperz9.github.io/quantalang/)

## Install

From crates.io (recommended):

```bash
cargo install quantalang
# binary: quantac
```

Or build from source:

```bash
cd compiler
cargo build --release
```

Add `target/release/quantac` (or `target\release\quantac.exe` on Windows) to your PATH.

## Editor support

Install the **[QuantaLang VS Code extension](https://marketplace.visualstudio.com/items?itemName=HarperZ9.quantalang)** — syntax highlighting, brackets, comment toggles. Grammar source: [HarperZ9/quantalang-tmLanguage](https://github.com/HarperZ9/quantalang-tmLanguage).

## Quick Start

Create `hello.quanta`:

```
fn main() {
    println!("Hello, World!");
}
```

Compile and run:

```bash
quantac run hello.quanta
```

Or compile to C and build manually:

```bash
quantac hello.quanta -o hello.c
cc hello.c -o hello
./hello
```

## Shader Example

QuantaLang can compile shader code directly to HLSL or GLSL. Create `vignette.quanta`:

```
fn vignette(uv_x: f64, uv_y: f64, strength: f64, softness: f64) -> f64 {
    let dx = uv_x - 0.5;
    let dy = uv_y - 0.5;
    let dist = sqrt(dx * dx + dy * dy);
    let vig = smoothstep(0.5, 0.5 * softness, dist);
    1.0 - strength * (1.0 - vig)
}

#[fragment]
fn PS_Vignette(uv: vec2) -> vec4 {
    let color = tex2d(uv);
    let vig = vignette(color.x, color.y, 0.5, 0.6);
    vec4(color.x * vig, color.y * vig, color.z * vig, 1.0)
}
```

Compile to HLSL (for ReShade / DirectX):

```bash
quantac vignette.quanta --target hlsl -o vignette.fx
```

Compile to GLSL (for OpenGL / Vulkan):

```bash
quantac vignette.quanta --target glsl -o vignette.glsl
```

## CLI Commands

| Command         | Description                          |
|-----------------|--------------------------------------|
| `quantac lex`   | Tokenize a file and print tokens     |
| `quantac parse` | Parse a file and print the AST       |
| `quantac check` | Type-check a file                    |
| `quantac build` | Build a project                      |
| `quantac run`   | Compile and run a `.quanta` file     |

### Target Selection

Use `--target` to select a code generation backend:

| Target   | Flag                          | Output  | Status       |
|----------|-------------------------------|---------|--------------|
| C        | `--target c` (default)        | `.c`    | Working      |
| HLSL     | `--target hlsl`               | `.hlsl` | Working      |
| GLSL     | `--target glsl`               | `.glsl` | Working      |
| SPIR-V   | `--target spirv`              | `.spv`  | Experimental |
| LLVM IR  | `--target llvm`               | `.ll`   | Experimental |
| WASM     | `--target wasm`               | `.wasm` | Experimental |
| x86-64   | `--target x86-64`             | `.o`    | Experimental |
| ARM64    | `--target arm64`              | `.o`    | Experimental |

## Status

**132/132 test programs compile.** Full pipeline: `.quanta` → C99 → MSVC → native x86-64 executable. See [TEST_RESULTS.md](TEST_RESULTS.md) for outputs.

Programs cover: functions, recursion, structs, enums, closures, generics, traits, dynamic dispatch, algebraic effects, pattern matching, iterators, hashmaps, vector math, color science, and self-hosted compiler components.

The C backend is the primary target. HLSL/GLSL produce clean shader output. SPIR-V, LLVM, WASM, x86-64, and ARM64 backends are experimental.

## Design

See [DESIGN.md](DESIGN.md) for full architectural documentation including:
- Pipeline overview (lexer → parser → types → MIR → backends)
- Type system rationale: why bidirectional inference, why Pratt parsing, why setjmp/longjmp for effects
- MIR design: SSA with basic blocks, statement/terminator model
- Known limitations: no borrow checker, eager monomorphization, one-shot effects

## Code Quality

- **CI**: clippy (correctness) + rustfmt + `cargo test` on Linux and Windows
- **Error handling**: Parser uses `expect()` with messages, lexer has 30+ error variants for recovery, pkg layer uses full `Result<T, E>` propagation
- **Codegen unwraps**: Intentional assertions on validated AST (documented policy in `codegen/mod.rs`)
- **Tests**: 599 passing, 0 failing, 3 ignored (SPIR-V validator dependency)
  - Type inference: 54 tests (unification, bidirectional flow, effect inference, const generics)
  - Lexer: 51 tests (token types, spans, Unicode, edge cases, error recovery)
  - Parser: 85 tests (all expression/item/pattern forms, malformed programs)
  - Codegen: 195 tests across 8 backends (C backend has 24 end-to-end output verification tests)

## License

MIT License. See [LICENSE](LICENSE) for details.
