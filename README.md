# QuantaLang

**The Effects Language** — a compiled language for graphics, shaders, and systems programming.

QuantaLang compiles `.quanta` source files to **C** (production-ready), **HLSL** and **GLSL** (shader output), with experimental backends for SPIR-V, LLVM IR, WebAssembly, x86-64, and ARM64.

## Installation

```bash
cd compiler
cargo build --release
```

Add `target/release/quantac` (or `target\release\quantac.exe` on Windows) to your PATH.

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
| C        | `--target c` (default)        | `.c`    | Stable       |
| HLSL     | `--target hlsl`               | `.hlsl` | Stable       |
| GLSL     | `--target glsl`               | `.glsl` | Stable       |
| SPIR-V   | `--target spirv`              | `.spv`  | Experimental |
| LLVM IR  | `--target llvm`               | `.ll`   | Experimental |
| WASM     | `--target wasm`               | `.wasm` | Experimental |
| x86-64   | `--target x86-64`             | `.o`    | Experimental |
| ARM64    | `--target arm64`              | `.o`    | Experimental |

## Status

**Alpha.** The C backend is stable and produces correct, portable output. The shader backends (HLSL and GLSL) produce clean output suitable for ReShade and OpenGL/Vulkan respectively. SPIR-V output passes `spirv-val` validation. The LLVM, WASM, x86-64, and ARM64 backends are experimental and under active development.

## License

MIT License. See [LICENSE](LICENSE) for details.
