# QuantaLang for Visual Studio Code

Language support for [QuantaLang](https://github.com/quantalang/quantalang) — The Effects Language for game engines and GPU shaders.

## Features

- **Syntax highlighting** for `.quanta` files — keywords, types, effects, attributes, strings, numbers, comments, macros, operators, and more
- **Language Server Protocol (LSP)** support via the `quantac lsp` server — provides diagnostics, go-to-definition, hover info, and completions when the compiler is installed
- **Bracket matching**, auto-closing pairs, and code folding
- **Shader attribute highlighting** for `#[vertex]`, `#[fragment]`, and `#[compute]`
- **Effect system highlighting** — `effect`, `perform`, `handle`, `with`, `resume`, and the `~` effect annotation

## Installation

### From source

```bash
cd editors/vscode
npm install
npm run compile
```

Then press **F5** in VS Code to launch an Extension Development Host with the extension loaded.

### Packaging

```bash
npm install -g @vscode/vsce
vsce package
code --install-extension quantalang-0.1.0.vsix
```

## Configuration

| Setting                  | Default    | Description                          |
|--------------------------|------------|--------------------------------------|
| `quantalang.serverPath`  | `quantac`  | Path to the `quantac` compiler binary |
| `quantalang.target`      | `c`        | Default compilation target (`c`, `llvm`, `wasm`, `spirv`, `x86-64`, `arm64`) |

## Language Server

The extension connects to the QuantaLang language server (`quantac lsp`). If the compiler is not installed, syntax highlighting still works — the LSP features are optional.

Install the compiler:

```bash
# Build from source
cd compiler
cargo build --release
# The binary is at compiler/target/release/quantac
```

Then either add it to your PATH or set `quantalang.serverPath` in VS Code settings.

## Supported syntax

The grammar covers all current QuantaLang constructs:

- Control flow: `if`, `else`, `match`, `loop`, `while`, `for`, `in`, `break`, `continue`, `return`
- Declarations: `fn`, `struct`, `enum`, `trait`, `impl`, `type`, `const`, `static`, `let`, `mut`, `pub`, `mod`, `use`, `extern`
- Effects: `effect`, `handle`, `perform`, `with`, `resume`
- Types: primitive (`i32`, `f64`, `bool`, `str`, ...), GPU (`vec2`, `vec3`, `vec4`, `mat4`, ...), user-defined
- Generics: `fn identity<T>(x: T) -> T`
- Attributes: `#[vertex]`, `#[fragment]`, `#[compute]`
- Macros: `println!()`, `format!()`
- Lifetime annotations: `'a`, `'static`
- Comments: `//`, `/* */`, `///` doc comments
- Strings with format interpolation: `"Hello, {}!"`
- Number literals: decimal, hex (`0x`), binary (`0b`), octal (`0o`), floats, with type suffixes
