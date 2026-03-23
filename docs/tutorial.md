# Getting Started with QuantaLang

QuantaLang is "The Effects Language" -- a single language that compiles to C, HLSL, GLSL, and SPIR-V. Write your logic once, deploy it as a native executable or a GPU shader for DirectX, OpenGL, or Vulkan.

This tutorial walks you through installation, your first program, the language fundamentals, and your first shader.

---

## 1. Installation

### Prerequisites

- **Rust toolchain** (1.75+): [https://rustup.rs](https://rustup.rs)
- **A C compiler**: GCC, Clang, or MSVC (for compiling generated C output)
- **Git**

### Build from Source

```bash
git clone https://github.com/HarperZ9/quantalang.git
cd quantalang
cargo build --release
```

The compiler binary is produced at `target/release/quantac` (or `quantac.exe` on Windows).

### Add to PATH

**Linux / macOS:**
```bash
export PATH="$PATH:$(pwd)/target/release"
```

**Windows (PowerShell):**
```powershell
$env:PATH += ";$(Get-Location)\target\release"
```

### Verify Installation

```bash
quantac version
```

Expected output:
```
QuantaLang compiler v1.0.0
```

You are ready to go.

---

## 2. Hello, World!

Create a file named `hello.quanta`:

```rust
fn main() {
    println!("Hello, World!");
}
```

### Run Directly

```bash
quantac run hello.quanta
```

Output:
```
Hello, World!
```

The `run` command compiles to C, invokes your system C compiler, and executes the result in one step.

### Build an Executable

```bash
quantac build hello.quanta
```

This produces:
1. `hello.c` -- the generated C source
2. `hello.exe` (Windows) or `hello` (Linux/macOS) -- the compiled binary

Run it directly:
```bash
./hello
Hello, World!
```

### Inspect the Generated C

The generated C includes QuantaLang's embedded runtime (strings, vectors, math types) followed by your compiled code. The `main()` function in `hello.c` looks like this:

```c
static const char* __str0 = "Hello, World!\n";

int32_t main(void) {
    printf(__str0);
    return 0;
}
```

Clean, readable, no runtime dependencies beyond the C standard library.

---

## 3. Language Basics

### Variables

Variables are declared with `let`. Types are inferred but can be annotated explicitly.

```rust
fn main() {
    let x = 42;
    let name = "QuantaLang";
    let pi: f64 = 3.14159;
    let flag: bool = true;

    println!("x = {}", x);
    println!("name = {}", name);
    println!("pi = {}", pi);
    println!("flag = {}", flag);
}
```

Output:
```
x = 42
name = QuantaLang
pi = 3.14159
flag = true
```

Mutable variables use `let mut`:

```rust
fn main() {
    let mut count = 0;
    count = count + 1;
    count = count + 1;
    println!("count = {}", count);
}
```

Output:
```
count = 2
```

### Functions

Functions are declared with `fn`. Parameters require type annotations. The return type follows `->`.

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn multiply(a: i32, b: i32) -> i32 {
    a * b
}

fn main() {
    let result = add(3, 4);
    println!("3 + 4 = {}", result);

    let product = multiply(6, 7);
    println!("6 * 7 = {}", product);
}
```

Output:
```
3 + 4 = 7
6 * 7 = 42
```

The last expression in a function body is the implicit return value -- no `return` keyword needed.

### Structs

Structs group related data with named fields.

```rust
struct Point {
    x: i32,
    y: i32,
}

fn add_points(a: Point, b: Point) -> Point {
    Point { x: a.x + b.x, y: a.y + b.y }
}

fn main() {
    let p1 = Point { x: 3, y: 4 };
    let p2 = Point { x: 10, y: 20 };
    let sum = add_points(p1, p2);
    println!("Sum: ({}, {})", sum.x, sum.y);
}
```

Output:
```
Sum: (13, 24)
```

### Enums and Pattern Matching

Enums define types with multiple variants. The `match` expression destructures them.

```rust
enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
}

fn area(s: Shape) -> f64 {
    match s {
        Shape::Circle(r) => 3.14159 * r * r,
        Shape::Rectangle(w, h) => w * h,
    }
}

fn main() {
    let c = Shape::Circle(5.0);
    let r = Shape::Rectangle(3.0, 4.0);
    println!("Circle area: {}", area(c));
    println!("Rectangle area: {}", area(r));
}
```

Output:
```
Circle area: 78.5397
Rectangle area: 12
```

### Control Flow

```rust
fn main() {
    // If/else
    let x = 10;
    if x > 5 {
        println!("x is large");
    } else {
        println!("x is small");
    }

    // While loops
    let mut i = 0;
    while i < 5 {
        println!("i = {}", i);
        i = i + 1;
    }
}
```

Output:
```
x is large
i = 0
i = 1
i = 2
i = 3
i = 4
```

---

## 4. Your First Shader

This is where QuantaLang shines. You write one `.quanta` file and compile it to any shader target.

### Write a Vignette Effect

Create `vignette.quanta`:

```rust
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

Key attributes:
- `#[fragment]` marks a function as a pixel/fragment shader entry point
- `tex2d()` samples the back buffer texture
- `vec2`, `vec4` are built-in GPU vector types

### Compile to HLSL (DirectX / ReShade)

```bash
quantac vignette.quanta --target hlsl -o vignette.fx
```

Generated HLSL output:

```hlsl
// Generated by QuantaLang Compiler
// Target: HLSL (DirectX / ReShade)

float vignette(float uv_x, float uv_y, float strength, float softness) {
    float dx = (uv_x - 0.5);
    float dy = (uv_y - 0.5);
    float dist = sqrt(((dx * dx) + (dy * dy)));
    float vig = smoothstep(0.5, (0.5 * softness), dist);
    return (1.0 - (strength * (1.0 - vig)));
}

float4 PS_Vignette(float4 pos : SV_Position, float2 uv : TEXCOORD) : SV_Target0 {
    float4 color = tex2D(ReShade::BackBuffer, uv);
    float vig = vignette(color.x, color.y, 0.5, 0.6);
    return float4((color.x * vig), (color.y * vig), (color.z * vig), 1.0);
}
```

### Compile to GLSL (OpenGL / Vulkan)

```bash
quantac vignette.quanta --target glsl -o vignette.glsl
```

Generated GLSL output:

```glsl
// Generated by QuantaLang Compiler
// Target: GLSL (OpenGL / Vulkan)

#version 450

double vignette(double uv_x, double uv_y, double strength, double softness) {
    double dx = (uv_x - 0.5);
    double dy = (uv_y - 0.5);
    double dist = sqrt(((dx * dx) + (dy * dy)));
    double vig = smoothstep(0.5, (0.5 * softness), dist);
    return (1.0 - (strength * (1.0 - vig)));
}

vec4 PS_Vignette(vec2 uv) {
    vec4 color = texture(backbuffer, uv);
    double vig = vignette(color.x, color.y, 0.5, 0.6);
    return vec4((color.x * vig), (color.y * vig), (color.z * vig), 1.0);
}
```

### Compile to a ReShade .fx Effect

```bash
quantac vignette.quanta --target hlsl -o vignette.fx
```

The `.fx` output automatically includes the `ReShade.fxh` header and generates a `technique` block:

```hlsl
#include "ReShade.fxh"

// ... shader functions ...

technique Quanta_PS_Vignette {
    pass {
        VertexShader = PostProcessVS;
        PixelShader = PS_Vignette;
    }
}
```

Drop this file into your ReShade `Shaders` folder and it works immediately.

---

## 5. VS Code Extension

QuantaLang ships with a VS Code extension for syntax highlighting, code snippets, and LSP support.

### Install

```bash
code --install-extension editors/quantalang-0.1.0.vsix
```

### Features

- **Syntax highlighting** for `.quanta` files -- keywords, types, attributes, shader builtins
- **Snippets** -- type `fn`, `struct`, `match`, `#[fragment]` to expand common patterns
- **LSP integration** -- powered by `quantac lsp`, providing:
  - Go to definition
  - Hover type information
  - Diagnostics (errors and warnings inline)
  - Completion suggestions

### Start the LSP Manually

If you need to run the language server outside VS Code:

```bash
quantac lsp
```

The LSP communicates over stdin/stdout using the Language Server Protocol.

---

## 6. Next Steps

### Explore the Shader Demos

The `demos/` directory contains 19 complete shader effects you can study and modify:

| Demo | Description |
|------|-------------|
| `vignette_shader.quanta` | Screen-edge darkening |
| `ssao.quanta` | Screen-space ambient occlusion |
| `bloom.quanta` | Two-pass bright extraction + blur |
| `depth_of_field.quanta` | Disc-kernel bokeh blur |
| `chromatic_aberration.quanta` | RGB channel separation |
| `film_grain.quanta` | Cinematic noise overlay |
| `color_grading.quanta` | Color correction LUT |
| `edge_detect.quanta` | Sobel edge detection |
| `cinematic.quanta` | Combined cinematic pipeline |
| `reshade_tonemap.quanta` | HDR tone mapping |

Compile any of them:

```bash
quantac bloom.quanta --target hlsl -o bloom.fx
quantac ssao.quanta --target glsl -o ssao.glsl
```

### Run the Test Suite

The `tests/programs/` directory contains 117 integration tests with `.expected` output files:

```bash
quantac run tests/programs/01_hello.quanta
quantac run tests/programs/03_functions.quanta
quantac run tests/programs/11_structs.quanta
quantac run tests/programs/12_enums.quanta
```

### CLI Reference

The `quantac` compiler exposes these commands:

| Command | Description |
|---------|-------------|
| `quantac run <file>` | Compile and execute immediately |
| `quantac build <file>` | Compile to C and produce a native binary |
| `quantac lex <file>` | Show the token stream |
| `quantac parse <file>` | Show the AST |
| `quantac check <file>` | Run the type checker only |
| `quantac compile <file> --target <t>` | Compile to a specific backend |
| `quantac lsp` | Start the language server |
| `quantac fmt <file>` | Format a `.quanta` source file |
| `quantac pkg <cmd>` | Package manager (init, add, resolve, search) |

Supported `--target` values: `c`, `hlsl`, `glsl`, `spirv`, `llvm`, `wasm`, `x86-64`, `arm64`.

### Cross-Target Compilation

To verify that a shader compiles to all targets, use the cross-target harness:

```bash
tests/cross_target.sh
```

This runs 81 compilations (27 shaders x 3 targets) with zero expected failures.

---

## Summary

You now know how to:

1. Build and install the QuantaLang compiler
2. Write and run programs that compile to native executables via C
3. Use variables, functions, structs, enums, and pattern matching
4. Write shader effects with `#[fragment]` and compile to HLSL, GLSL, or ReShade `.fx`
5. Set up VS Code with syntax highlighting and LSP support

QuantaLang is one language for CPU and GPU. Write once, target everywhere.
