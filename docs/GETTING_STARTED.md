# Getting Started with QuantaLang

QuantaLang is "The Effects Language" -- a systems programming language with algebraic effects, designed for game engines and GPU shaders. One language for your engine code and your shaders: write a function once, compile it to CPU for testing and GPU for rendering.

---

## Prerequisites

- **Rust toolchain** (1.75+): [rustup.rs](https://rustup.rs)
- **C compiler** (one of): gcc, clang, or MSVC (`cl.exe` on Windows)
- **Vulkan SDK** (optional): for `spirv-val` shader validation -- [vulkan.lunarg.com](https://vulkan.lunarg.com/sdk/home)

---

## Building the Compiler

```bash
cd compiler
cargo build --release
```

The binary is at `compiler/target/release/quantac` (or `quantac.exe` on Windows). Add it to your PATH.

---

## Your First Program

Create `hello.quanta`:

```quanta
fn main() {
    println!("Hello from QuantaLang!");
}
```

Compile and run:

```bash
# Compile to C, then to native executable
quantac build hello.quanta

# Or compile to C source only
quantac hello.quanta -o hello.c
gcc hello.c -o hello -lm
./hello
```

On Windows with MSVC:

```bash
quantac hello.quanta -o hello.c
cl.exe hello.c
hello.exe
```

---

## Your First Shader

QuantaLang's defining feature: the same function compiles to both CPU and GPU. Create `shader.quanta`:

```quanta
fn aces_tonemap(x: f64) -> f64 {
    let num = x * (2.51 * x + 0.03);
    let den = x * (2.43 * x + 0.59) + 0.14;
    clamp(num / den, 0.0, 1.0)
}

#[fragment]
fn main(color: vec3) -> vec4 {
    let r = aces_tonemap(color.x);
    let g = aces_tonemap(color.y);
    let b = aces_tonemap(color.z);
    vec4(r, g, b, 1.0)
}
```

Compile to GPU (Vulkan SPIR-V):

```bash
quantac shader.quanta -o shader.spv
```

Compile to CPU (C source for testing):

```bash
quantac shader.quanta -o shader.c
```

Same file. Both targets. The `aces_tonemap` function is identical on CPU and GPU -- test on CPU, deploy to GPU.

---

## Multi-Target Compilation

QuantaLang has six code generation backends:

```bash
quantac file.quanta --target=c        # C99 source (default)
quantac file.quanta --target=llvm     # LLVM IR
quantac file.quanta --target=wasm     # WebAssembly
quantac file.quanta --target=spirv    # Vulkan SPIR-V
quantac file.quanta --target=x86-64   # x86-64 assembly
quantac file.quanta --target=arm64    # ARM64 assembly
```

The output format is also inferred from the `-o` extension:

```bash
quantac shader.quanta -o shader.spv   # infers --target=spirv
quantac shader.quanta -o shader.c     # infers --target=c
quantac shader.quanta -o shader.ll    # infers --target=llvm
```

---

## Shader Hot Reload

Watch a directory and recompile shaders on every save:

```bash
quantac watch shaders/ --target=spirv
```

Your Vulkan renderer can detect `.spv` file changes and reload without restarting.

---

## CLI Commands

```
quantac lex <file>           Tokenize and print tokens
quantac parse <file>         Parse and print AST
quantac check <file>         Type-check without compiling
quantac build [path]         Compile to C -> invoke C compiler -> native executable
quantac run <file>           Compile and run immediately
quantac fmt <file>           Format source code
quantac pkg <subcommand>     Package manager
quantac watch <path>         Watch and recompile on change
quantac lsp                  Start Language Server Protocol server
quantac repl                 Interactive REPL
quantac version              Print version
```

---

## Key Language Features

### Algebraic Effects

Effects are QuantaLang's signature feature -- like checked exceptions crossed with dependency injection. You declare what side effects a function performs, and the caller decides how to handle them.

```quanta
effect Render {
    fn draw(description: str) -> (),
}

fn render_scene() ~ Render {
    perform Render.draw("player at (5, 1, 3)")
}

fn main() {
    // Production: real Vulkan rendering
    handle {
        render_scene()
    } with {
        Render.draw(desc) => {
            println!("RENDER: {}", desc)
        },
    }
}
```

See [EFFECTS_GUIDE.md](EFFECTS_GUIDE.md) for the full effects tutorial.

### Structs and Enums

```quanta
struct Point {
    x: i32,
    y: i32,
}

fn add_points(a: Point, b: Point) -> Point {
    Point { x: a.x + b.x, y: a.y + b.y }
}

enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
}
```

### Pattern Matching

```quanta
fn area(s: Shape) -> f64 {
    match s {
        Shape::Circle(r) => 3.14159 * r * r,
        Shape::Rectangle(w, h) => w * h,
    }
}
```

### Closures with Captures

```quanta
let offset: i32 = 10;
let add_offset = |x: i32| -> i32 { x + offset };
println!("{}", add_offset(5));   // 15
```

### Traits (Static Dispatch)

```quanta
trait Shape {
    fn area(self) -> f64;
    fn name(self) -> str;
}

struct Circle {
    radius: f64,
}

impl Shape for Circle {
    fn area(self) -> f64 {
        3.14159 * self.radius * self.radius
    }
    fn name(self) -> str {
        "Circle"
    }
}
```

### Vector and Matrix Math

Built-in `vec2`, `vec3`, `vec4`, and `mat4` types with operator overloading:

```quanta
let pos = vec3(1.0, 2.0, 3.0);
let dir = normalize(pos);
let d = dot(dir, vec3(0.0, 1.0, 0.0));

let model = mat4_translate(vec3(5.0, 0.0, 3.0));
let world_pos = model * vec4(0.0, 0.0, 0.0, 1.0);

// Swizzling
let xy = pos.xy;       // vec2
let rgb = pos.xyz;     // vec3
let bgr = pos.zyx;     // vec3
```

### GLSL Built-in Functions

All GLSL.std.450 builtins are available in normal code:

```
sin  cos  tan  asin  acos  atan
pow  exp  log  sqrt  inversesqrt
abs  floor  ceil  fract  round
clamp  mix  smoothstep  step
length  distance  dot  cross  normalize  reflect
min  max
```

These functions work on both CPU and GPU targets. On CPU they compile to C `<math.h>` calls; on GPU they compile to SPIR-V GLSL.std.450 extended instructions.

---

## VS Code Extension

QuantaLang ships with a VS Code extension providing syntax highlighting:

```bash
cd editors/vscode
npm install
npm run compile
```

Then open VS Code, go to Extensions > Install from VSIX, or use the debug launch configuration to test.

---

## Next Steps

- [SHADER_GUIDE.md](SHADER_GUIDE.md) -- Write vertex, fragment, and compute shaders
- [EFFECTS_GUIDE.md](EFFECTS_GUIDE.md) -- Master algebraic effects for rendering pipelines
- `tests/programs/` -- 50 working example programs, from hello world to PBR shaders
- `tests/shaders/` -- 14 validated SPIR-V shaders
- `examples/graphics/` -- Full Vulkan triangle rendered with QuantaLang shaders
