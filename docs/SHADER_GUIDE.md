# QuantaLang Shader Programming Guide

This guide covers writing GPU shaders in QuantaLang. If you know HLSL or GLSL, you already know 90% of the syntax -- QuantaLang just removes the ceremony and lets you test shaders on CPU before deploying to GPU.

---

## The Dual-Target Story

Every QuantaLang shader function is a normal function. There is no separate shading language. You write math, annotate an entry point, and the compiler handles the rest:

```quanta
// This function is just math. It compiles to C, LLVM, WASM, or SPIR-V.
fn aces_tonemap(x: f64) -> f64 {
    let num = x * (2.51 * x + 0.03);
    let den = x * (2.43 * x + 0.59) + 0.14;
    clamp(num / den, 0.0, 1.0)
}

// GPU entry point
#[fragment]
fn main(color: vec3) -> vec4 {
    let r = aces_tonemap(color.x);
    let g = aces_tonemap(color.y);
    let b = aces_tonemap(color.z);
    vec4(r, g, b, 1.0)
}

// CPU entry point -- test the same function with known values
fn main() {
    let result = aces_tonemap(1.0);
    println!("ACES(1.0) = {} (expect ~0.80)", result);
}
```

```bash
quantac shader.quanta -o shader.spv   # GPU: Vulkan SPIR-V
quantac shader.quanta -o shader.c     # CPU: testable C code
```

No other language can do this. Your shader math is unit-testable with `println!` and assertions, then deployed to the GPU without modification.

---

## Shader Attributes

Annotate your entry point function with a shader stage attribute:

| Attribute      | SPIR-V Execution Model | Use                              |
|---------------|------------------------|----------------------------------|
| `#[vertex]`   | Vertex                 | Transform vertices, output positions |
| `#[fragment]` | Fragment               | Compute pixel colors             |
| `#[compute]`  | GLCompute              | General-purpose GPU compute      |

```quanta
#[vertex]
fn vs_main(vertex_id: i32) -> VertexOutput { ... }

#[fragment]
fn fs_main(inputs: vec3) -> vec4 { ... }

#[compute]
fn cs_main() { ... }
```

Only the annotated function becomes the SPIR-V entry point. All other functions it calls are inlined or emitted as SPIR-V helper functions.

---

## Built-in Types

### Vectors

| Type   | Components | GPU Type     | Notes                    |
|--------|-----------|--------------|--------------------------|
| `vec2` | x, y      | OpTypeVector | 2-component float vector |
| `vec3` | x, y, z   | OpTypeVector | 3-component float vector |
| `vec4` | x, y, z, w | OpTypeVector | 4-component float vector |

Constructors:

```quanta
let a = vec2(1.0, 2.0);
let b = vec3(1.0, 0.0, 0.0);
let c = vec4(0.0, 0.5, 1.0, 1.0);
```

Field access and swizzling:

```quanta
let pos = vec3(1.0, 2.0, 3.0);
pos.x               // 1.0
pos.xy              // vec2(1.0, 2.0)
pos.zyx             // vec3(3.0, 2.0, 1.0)

// RGBA aliases work too
let color = vec4(1.0, 0.5, 0.0, 1.0);
color.rgb            // vec3(1.0, 0.5, 0.0)
color.rg             // vec2(1.0, 0.5)
```

Arithmetic operators work component-wise:

```quanta
let a = vec3(1.0, 2.0, 3.0);
let b = vec3(4.0, 5.0, 6.0);
let c = a + b;       // vec3(5.0, 7.0, 9.0)
let d = a * b;       // vec3(4.0, 10.0, 18.0)
let e = a - b;       // vec3(-3.0, -3.0, -3.0)
```

### Matrices

| Type   | Size  | GPU Type      | Notes                     |
|--------|-------|---------------|---------------------------|
| `mat4` | 4x4   | OpTypeMatrix  | Column-major 4x4 matrix   |

```quanta
let identity = mat4_identity();
let model = mat4_translate(vec3(5.0, 1.0, 3.0));
let scaled = mat4_scale(vec3(2.0, 2.0, 2.0));
let proj = mat4_perspective(45.0, 16.0 / 9.0, 0.1, 100.0);

// Matrix-matrix multiply
let mvp = proj * model;

// Matrix-vector multiply
let world_pos = model * vec4(0.0, 0.0, 0.0, 1.0);
```

### Scalar Types

| QuantaLang | CPU          | GPU (SPIR-V)   |
|-----------|--------------|----------------|
| `f64`     | double       | OpTypeFloat 32 |
| `f32`     | float        | OpTypeFloat 32 |
| `i32`     | int32_t      | OpTypeInt 32   |
| `bool`    | int (0/1)    | OpTypeBool     |

---

## f64 to f32 Coercion

Write your shader math with `f64` for readability and CPU precision. The SPIR-V backend automatically coerces all `f64` values to `f32` for the GPU -- Vulkan 1.0 requires 32-bit floats for standard shader operations.

```quanta
// You write f64:
fn fresnel(cos_theta: f64, f0: f64) -> f64 {
    f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0)
}

// CPU target: compiles to double-precision C code
// GPU target: SPIR-V backend converts all floats to 32-bit OpTypeFloat
```

You never have to think about `f32` vs `f64` for shaders. The compiler does the right thing.

---

## Built-in Functions (GLSL.std.450)

All standard shader math functions are available. On CPU they map to `<math.h>`; on GPU they map to SPIR-V extended instructions.

**Trigonometric:**
```
sin(x)  cos(x)  tan(x)  asin(x)  acos(x)  atan(x)
```

**Exponential:**
```
pow(x, y)  exp(x)  log(x)  sqrt(x)  inversesqrt(x)
```

**Common:**
```
abs(x)  floor(x)  ceil(x)  fract(x)  round(x)
clamp(x, min, max)  mix(a, b, t)  step(edge, x)  smoothstep(lo, hi, x)
min(a, b)  max(a, b)
```

**Geometric:**
```
length(v)  distance(a, b)  dot(a, b)  cross(a, b)
normalize(v)  reflect(incident, normal)
```

Example -- Phong lighting in 4 lines:

```quanta
fn phong(normal: vec3, light_dir: vec3) -> vec3 {
    let ambient = vec3(0.1, 0.1, 0.1);
    let diff = clamp(dot(normalize(normal), normalize(light_dir)), 0.0, 1.0);
    ambient + vec3(diff, diff, diff)
}
```

---

## Vertex Shader

A vertex shader takes a `vertex_id` parameter (maps to `gl_VertexIndex`) and returns a struct with at least a `position: vec4` field:

```quanta
struct VertexOutput {
    position: vec4,
    color: vec3,
}

fn vertex_position(id: i32) -> vec4 {
    if id == 0 {
        vec4(0.0, -0.5, 0.0, 1.0)    // top center
    } else {
        if id == 1 {
            vec4(0.5, 0.5, 0.0, 1.0)  // bottom right
        } else {
            vec4(-0.5, 0.5, 0.0, 1.0) // bottom left
        }
    }
}

fn vertex_color(id: i32) -> vec3 {
    if id == 0 {
        vec3(1.0, 0.0, 0.0)           // red
    } else {
        if id == 1 {
            vec3(0.0, 1.0, 0.0)       // green
        } else {
            vec3(0.0, 0.0, 1.0)       // blue
        }
    }
}

#[vertex]
fn main(vertex_id: i32) -> VertexOutput {
    VertexOutput {
        position: vertex_position(vertex_id),
        color: vertex_color(vertex_id),
    }
}
```

The compiler:
- Maps `vertex_id` to SPIR-V `BuiltIn VertexIndex`
- Maps `position` in the return struct to `BuiltIn Position`
- Outputs `color` as `Location 0` for the fragment shader
- Decomposes the struct return into separate output variables

Compile: `quantac triangle_vert.quanta -o vert.spv`

---

## Fragment Shader

A fragment shader takes interpolated inputs from the vertex stage and returns a `vec4` color:

```quanta
fn aces_tonemap(x: f64) -> f64 {
    let num = x * (2.51 * x + 0.03);
    let den = x * (2.43 * x + 0.59) + 0.14;
    clamp(num / den, 0.0, 1.0)
}

#[fragment]
fn main(frag_color: vec3) -> vec4 {
    let r = aces_tonemap(frag_color.x);
    let g = aces_tonemap(frag_color.y);
    let b = aces_tonemap(frag_color.z);
    vec4(r, g, b, 1.0)
}
```

The compiler:
- Maps `frag_color` to `Location 0` input
- Maps the return `vec4` to `Location 0` output (framebuffer color)
- Sets `OpExecutionMode OriginUpperLeft`

Compile: `quantac triangle_frag.quanta -o frag.spv`

---

## Struct Return Types for Vertex Outputs

When a vertex shader returns a struct, the compiler decomposes it into separate SPIR-V output variables:

```quanta
struct VertexOutput {
    position: vec4,    // -> BuiltIn Position
    color: vec3,       // -> Location 0
    uv: vec2,          // -> Location 1
}
```

The `position` field is special -- it maps to `gl_Position` (BuiltIn Position). All other fields get sequential `Location` decorations starting at 0.

---

## Shader Hot Reload Workflow

During development, use the watch command to recompile shaders on save:

```bash
# Terminal 1: watch and recompile
quantac watch shaders/ --target=spirv

# Terminal 2: your Vulkan app
./my_engine
```

Your engine detects `.spv` timestamp changes and reloads pipeline state. Typical workflow:

1. Edit `shaders/postprocess.quanta` in your editor
2. `quantac watch` detects the change, recompiles to `postprocess.spv`
3. Your engine sees the new `.spv`, recreates the pipeline
4. New shader is live in < 1 second

---

## Example: Cook-Torrance PBR BRDF

A full physically-based rendering shader. The same code used in Unreal Engine and Frostbite, written in QuantaLang:

```quanta
fn pi() -> f64 { 3.14159265358979 }

// Fresnel-Schlick: F(h,v) = F0 + (1-F0)(1-h.v)^5
fn fresnel_schlick(cos_theta: f64, f0: f64) -> f64 {
    f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0)
}

// GGX normal distribution
fn distribution_ggx(n_dot_h: f64, roughness: f64) -> f64 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denom_term = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    a2 / (pi() * denom_term * denom_term)
}

// Schlick-GGX geometry (single direction)
fn geometry_schlick_ggx(n_dot_v: f64, roughness: f64) -> f64 {
    let r = roughness + 1.0;
    let k = r * r / 8.0;
    n_dot_v / (n_dot_v * (1.0 - k) + k)
}

// Smith geometry (both directions)
fn geometry_smith(n_dot_v: f64, n_dot_l: f64, roughness: f64) -> f64 {
    geometry_schlick_ggx(n_dot_v, roughness) *
    geometry_schlick_ggx(n_dot_l, roughness)
}

// Full Cook-Torrance specular BRDF
fn cook_torrance(
    n_dot_h: f64, n_dot_v: f64, n_dot_l: f64, h_dot_v: f64,
    roughness: f64, f0: f64
) -> f64 {
    let d = distribution_ggx(n_dot_h, roughness);
    let f = fresnel_schlick(h_dot_v, f0);
    let g = geometry_smith(n_dot_v, n_dot_l, roughness);
    (d * f * g) / (4.0 * n_dot_v * n_dot_l + 0.0001)
}

// Combined PBR: Lambertian diffuse + Cook-Torrance specular
fn pbr_lighting(
    n_dot_h: f64, n_dot_v: f64, n_dot_l: f64, h_dot_v: f64,
    albedo: f64, roughness: f64, metallic: f64
) -> f64 {
    let f0 = 0.04 * (1.0 - metallic) + albedo * metallic;
    let specular = cook_torrance(n_dot_h, n_dot_v, n_dot_l, h_dot_v, roughness, f0);
    let ks = fresnel_schlick(h_dot_v, f0);
    let kd = (1.0 - ks) * (1.0 - metallic);
    let diffuse = kd * albedo / pi();
    (diffuse + specular) * n_dot_l
}

fn aces_tonemap(x: f64) -> f64 {
    clamp(x * (2.51 * x + 0.03) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0)
}

fn linear_to_srgb(x: f64) -> f64 {
    clamp(pow(x, 1.0 / 2.2), 0.0, 1.0)
}

// --- GPU entry point ---
#[fragment]
fn fs_pbr(inputs: vec3) -> vec4 {
    let n_dot_l = clamp(inputs.x, 0.0, 1.0);
    let n_dot_h = clamp(inputs.y, 0.0, 1.0);
    let h_dot_v = clamp(inputs.z, 0.0, 1.0);
    let n_dot_v = 0.8;

    let lit = pbr_lighting(n_dot_h, n_dot_v, n_dot_l, h_dot_v, 0.8, 0.4, 0.0);
    let final_val = linear_to_srgb(aces_tonemap(lit));
    vec4(final_val, final_val, final_val, 1.0)
}

// --- CPU entry point (testing) ---
fn main() {
    let f = fresnel_schlick(1.0, 0.04);
    println!("Fresnel head-on: {} (expect ~0.04)", f);

    let f_grazing = fresnel_schlick(0.0, 0.04);
    println!("Fresnel grazing: {} (expect ~1.0)", f_grazing);

    let result = pbr_lighting(0.9, 0.8, 0.7, 0.85, 0.8, 0.4, 0.0);
    println!("PBR dielectric: {}", result);
}
```

Compile:

```bash
quantac pbr_brdf.quanta -o pbr_brdf.spv    # GPU
quantac pbr_brdf.quanta -o pbr_brdf.c      # CPU test harness
```

---

## Example: Volumetric Fog Ray Marching

Height-based fog with Beer-Lambert extinction and Henyey-Greenstein phase scattering. The kind of shader Pascal Gilcher writes for ReShade:

```quanta
fn pi() -> f64 { 3.14159265358979 }

// Beer-Lambert extinction: transmittance = e^(-density * distance)
fn beer_lambert(density: f64, distance: f64) -> f64 {
    pow(2.718281828, 0.0 - density * distance)
}

// Henyey-Greenstein phase function
// g > 0: forward scattering (sun glow through fog)
// g = 0: isotropic
// g < 0: backward scattering
fn henyey_greenstein(cos_theta: f64, g: f64) -> f64 {
    let g2 = g * g;
    let denom = 1.0 + g2 - 2.0 * g * cos_theta;
    (1.0 - g2) / (4.0 * pi() * pow(denom, 1.5))
}

// Exponential height fog -- density falls off with altitude
fn height_fog_density(y: f64, base_density: f64, falloff: f64) -> f64 {
    base_density * pow(2.718281828, 0.0 - falloff * clamp(y, 0.0, 100.0))
}

// Ray march: accumulate extinction and in-scattering along a ray
fn raymarch_fog(
    ray_start_y: f64, ray_end_y: f64, ray_length: f64,
    light_cos_angle: f64,
    base_density: f64, falloff: f64, scatter_g: f64,
    num_steps: i32
) -> f64 {
    let step_size = ray_length / 16.0;
    let mut total_transmittance = 1.0;
    let mut in_scatter = 0.0;

    let mut i: i32 = 0;
    while i < 16 {
        let t = (i as f64 + 0.5) / 16.0;
        let sample_y = ray_start_y + (ray_end_y - ray_start_y) * t;
        let density = height_fog_density(sample_y, base_density, falloff);
        let step_extinction = beer_lambert(density, step_size);
        let phase = henyey_greenstein(light_cos_angle, scatter_g);

        in_scatter = in_scatter + total_transmittance * density * phase * step_size;
        total_transmittance = total_transmittance * step_extinction;
        i = i + 1;
    }
    in_scatter
}

fn aces(x: f64) -> f64 {
    clamp(x * (2.51 * x + 0.03) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0)
}

#[fragment]
fn fs_fog(ray_params: vec3) -> vec4 {
    let fog = raymarch_fog(
        ray_params.x, ray_params.y, 50.0,
        ray_params.z,
        0.05, 0.1, 0.7, 16
    );
    let mapped = aces(fog * 5.0);
    vec4(mapped, mapped, mapped, 1.0)
}

fn main() {
    let toward_sun = raymarch_fog(0.0, 5.0, 50.0, 0.9, 0.05, 0.1, 0.7, 16);
    let away = raymarch_fog(0.0, 5.0, 50.0, -0.5, 0.05, 0.1, 0.7, 16);
    println!("Fog toward sun: {} (bright, forward scattering)", toward_sun);
    println!("Fog away: {} (dim, less scattering)", away);
}
```

---

## Tips for C++/HLSL Programmers

| HLSL / GLSL              | QuantaLang                        |
|--------------------------|-----------------------------------|
| `float4`                 | `vec4`                            |
| `float3x3`              | (not yet -- use `mat4`)           |
| `SV_Position`           | `position: vec4` in return struct |
| `SV_VertexID`           | `vertex_id: i32` parameter        |
| `SV_Target0`            | return `vec4` from fragment       |
| `cbuffer`               | uniform buffer (in progress)      |
| `Texture2D.Sample()`    | texture sampling (in progress)    |
| `#include`              | functions are just functions      |
| separate `.hlsl` files  | same `.quanta` file for CPU + GPU |
| `[numthreads(8,8,1)]`   | `#[compute]`                      |

Key differences from HLSL/GLSL:
- **No separate shader language.** Your shader functions are normal functions.
- **Type inference.** Use `let` -- the compiler figures out the type.
- **`f64` everywhere.** Write `f64` for clarity; the GPU backend coerces to `f32`.
- **Algebraic effects.** Swap rendering backends without changing shader code. See [EFFECTS_GUIDE.md](EFFECTS_GUIDE.md).
- **Pattern matching.** Use `match` instead of chains of `if/else`.
