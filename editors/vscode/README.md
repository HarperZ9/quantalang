# QuantaLang for Visual Studio Code

Language support for [QuantaLang](https://github.com/HarperZ9/quantalang) -- syntax highlighting, snippets, and bracket matching for `.quanta` files.

## Features

- **Syntax highlighting** -- keywords, types, effects, attributes, strings, numbers, comments, macros, operators, and more
- **Language Server Protocol (LSP)** support via `quantac lsp` -- diagnostics, go-to-definition, hover info, and completions when the compiler is installed
- **Bracket matching**, auto-closing pairs, and code folding
- **19 snippets** for common constructs (functions, structs, enums, traits, loops, tests, shaders, effects)
- **Shader attribute highlighting** for `#[vertex]`, `#[fragment]`, and `#[compute]`
- **Effect system highlighting** -- `effect`, `perform`, `handle`, `with`, `resume`, and the `~` effect annotation

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
code --install-extension quantalang-1.0.0.vsix
```

## Configuration

| Setting                  | Default    | Description                          |
|--------------------------|------------|--------------------------------------|
| `quantalang.serverPath`  | `quantac`  | Path to the `quantac` compiler binary |
| `quantalang.target`      | `c`        | Default compilation target (`c`, `llvm`, `wasm`, `spirv`, `x86-64`, `arm64`) |

## Language Server

The extension connects to the QuantaLang language server (`quantac lsp`). If the compiler is not installed, syntax highlighting still works -- the LSP features are optional.

Install the compiler:

```bash
cd compiler
cargo build --release
# The binary is at compiler/target/release/quantac
```

Then either add it to your PATH or set `quantalang.serverPath` in VS Code settings.

## Snippets

| Prefix     | Description                      |
|------------|----------------------------------|
| `fn`       | Function definition              |
| `fnm`      | Method with `&self`              |
| `st`       | Struct definition                |
| `impl`     | Implementation block             |
| `enum`     | Enum definition                  |
| `trait`    | Trait definition                 |
| `for`      | For loop over range              |
| `match`    | Match expression                 |
| `while`    | While loop                       |
| `let`      | Let binding                      |
| `letm`     | Mutable let binding              |
| `test`     | Test function with `#[test]`     |
| `mod`      | Module definition                |
| `vec`      | `vec!` macro                     |
| `println`  | `println!` macro                 |
| `effect`   | Effect definition                |
| `fragment` | Fragment shader entry point      |
| `vertex`   | Vertex shader entry point        |
| `uniform`  | Shader uniform constant          |

## Supported Syntax

The grammar covers all current QuantaLang constructs:

- Control flow: `if`, `else`, `match`, `loop`, `while`, `for`, `in`, `break`, `continue`, `return`
- Declarations: `fn`, `struct`, `enum`, `trait`, `impl`, `type`, `const`, `static`, `let`, `mut`, `pub`, `mod`, `use`, `extern`
- Effects: `effect`, `handle`, `perform`, `with`, `resume`
- Types: primitive (`i32`, `f64`, `bool`, `str`, ...), GPU (`vec2`, `vec3`, `vec4`, `mat4`, ...), user-defined
- Generics: `fn identity<T>(x: T) -> T`
- Attributes: `#[vertex]`, `#[fragment]`, `#[compute]`, `#[test]`
- Macros: `println!()`, `format!()`, `vec![]`
- Lifetime annotations: `'a`, `'static`
- Comments: `//`, `/* */`, `///` doc comments
- Strings with format interpolation: `"Hello, {}!"`
- Number literals: decimal, hex (`0x`), binary (`0b`), octal (`0o`), floats, with type suffixes
- Operators: `+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `&&`, `||`, `!`, `&`, `|`, `^`, `<<`, `>>`, `..`, `..=`, `->`, `=>`, `~`, `?`
