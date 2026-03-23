# Getting Started with QuantaLang

Write shaders in QuantaLang. Compile to HLSL, GLSL, SPIR-V, or C. Drop into ReShade.

## Install

```bash
# Build from source (requires Rust toolchain)
cd quantalang/compiler
cargo build --release

# Add to PATH
export PATH="$PWD/target/release:$PATH"

# Verify
quantac version
```

## Hello Shader (5 minutes)

Create `hello.quanta`:

```quanta
#[uniform]
const brightness: f64 = 1.0;

fn adjust(c: f64) -> f64 {
    c * brightness
}

#[fragment]
fn PS_Hello(uv: vec2) -> vec4 {
    let color = tex2d(uv);
    vec4(adjust(color.x), adjust(color.y), adjust(color.z), 1.0)
}
```

Compile to ReShade:

```bash
quantac hello.quanta --target hlsl -o hello.fx
```

Drop `hello.fx` into `reshade-shaders/Shaders/`. Open ReShade. Enable "Quanta_PS_Hello". Adjust the brightness slider.

## What Just Happened

| QuantaLang | Generated HLSL |
|------------|---------------|
| `#[uniform] const brightness: f64 = 1.0;` | `uniform float brightness < ui_type = "slider"; ... > = 1.0;` |
| `#[fragment] fn PS_Hello(uv: vec2) -> vec4` | `float4 PS_Hello(float4 pos : SV_Position, float2 uv : TEXCOORD) : SV_Target0` |
| `tex2d(uv)` | `tex2D(ReShade::BackBuffer, uv)` |
| `vec4(r, g, b, 1.0)` | `float4(r, g, b, 1.0)` |
| (auto-generated) | `technique Quanta_PS_Hello { pass { ... } }` |

## Language Basics

### Types

```quanta
let x: i32 = 42;        // 32-bit integer
let y: f64 = 3.14;      // 64-bit float (maps to float in HLSL)
let v: vec4 = vec4(1.0, 0.0, 0.0, 1.0);  // RGBA color
let b: bool = true;
```

### Functions

```quanta
fn add(a: f64, b: f64) -> f64 {
    a + b    // last expression is the return value
}
```

### Control Flow

```quanta
if x > 0.5 {
    1.0
} else {
    0.0
}

while i < 16.0 {
    // loop body
    i = i + 1.0;
}

for j in 0..10 {
    // counted loop
}
```

### Structs

```quanta
struct Color {
    r: f64,
    g: f64,
    b: f64,
}

impl Color {
    fn luminance(self) -> f64 {
        self.r * 0.2126 + self.g * 0.7152 + self.b * 0.0722
    }
}
```

## Shader Features

### Uniforms (ReShade Sliders)

```quanta
#[uniform]
const exposure: f64 = 0.0;

#[uniform]
const saturation: f64 = 1.0;
```

These become adjustable sliders in ReShade's UI.

### Texture Sampling

```quanta
let color = tex2d(uv);              // Sample backbuffer
let depth = tex2d_depth(uv);        // Sample depth buffer
```

### Fragment Shaders

```quanta
#[fragment]
fn PS_MyEffect(uv: vec2) -> vec4 {
    // uv.x, uv.y = screen coordinates (0..1)
    // Return: output color as vec4
    let color = tex2d(uv);
    vec4(color.x, color.y, color.z, 1.0)
}
```

The compiler auto-generates:
- `SV_Position` parameter
- `TEXCOORD` semantic on `uv`
- `SV_Target0` return semantic
- ReShade `technique` + `pass` block

### Shader Math Intrinsics

All standard shader math functions are available:

```quanta
sin(x)  cos(x)  tan(x)  sqrt(x)  pow(x, y)  abs(x)  exp(x)
floor(x)  ceil(x)  round(x)  fract(x)  min(a, b)  max(a, b)
clamp(x, lo, hi)  smoothstep(edge0, edge1, x)  mix(a, b, t)
dot(a, b)  cross(a, b)  normalize(v)  length(v)  reflect(i, n)
```

### Color Space Safety

```quanta
fn tonemap(c: vec3 with ColorSpace<Linear>) -> vec3 with ColorSpace<sRGB> {
    // The compiler enforces: input must be Linear, output is sRGB.
    // Passing sRGB to a function expecting Linear = compile error.
}
```

## Cross-Target Compilation

Same source, every target:

```bash
quantac shader.quanta --target hlsl -o shader.fx      # ReShade / DirectX
quantac shader.quanta --target glsl -o shader.glsl     # OpenGL / Vulkan
quantac shader.quanta --target spirv -o shader.spv      # Vulkan binary
quantac shader.quanta --target c -o shader.c            # CPU validation
```

## VS Code Extension

Install the QuantaLang extension for:
- Syntax highlighting (keywords, types, shader intrinsics)
- Code snippets (`fn`, `fragment`, `uniform`, `vignette`, `hash`)
- LSP diagnostics (when `quantac` is in PATH)

## Example: Complete SSAO Shader

See `demos/ssao.quanta` — 85 lines of QuantaLang that compile to a production ReShade SSAO effect with depth sampling, random kernel loop, occlusion computation, and adjustable uniforms.

```bash
quantac demos/ssao.quanta --target hlsl -o ssao.fx
```
