# Building ReShade Effects with QuantaLang

*Stop writing raw HLSL for your game mods. QuantaLang gives you type safety, cross-target compilation, and clean ReShade output -- from a single source file.*

---

## What is ReShade?

[ReShade](https://reshade.me/) is a post-processing injector for games. It hooks into DirectX, OpenGL, or Vulkan rendering pipelines and lets you apply screen-space effects -- ambient occlusion, depth of field, bloom, color grading, film grain -- to any game, even games that shipped without them.

The modding community has built thousands of ReShade presets that transform how games look. Under the hood, every ReShade effect is a pixel shader written in HLSL-flavored `.fx` files.

### Why Modders Use ReShade

- **Universal:** Works with almost any game using DirectX 9/10/11/12, OpenGL, or Vulkan
- **No game modding required:** Injects at the driver level, not the game code
- **Instant feedback:** Edit a shader, press reload, see the result in-game
- **Community presets:** Share configurations as `.ini` files

### The Pain Point

Writing ReShade effects means writing raw HLSL with ReShade-specific boilerplate: `#include "ReShade.fxh"`, uniform annotations with `ui_type` metadata, `technique` blocks, `tex2D(ReShade::BackBuffer, uv)` calls, and DX semantics like `SV_Position` and `TEXCOORD`.

This boilerplate is repetitive, error-prone, and obscures the actual shader math. QuantaLang eliminates it.

---

## How QuantaLang Maps to ReShade

QuantaLang provides shader-specific attributes and builtins that compile directly to the patterns ReShade expects. Here is how the concepts map:

| QuantaLang | ReShade Output |
|------------|---------------|
| `#[fragment] fn PS_Name(uv: vec2) -> vec4` | `float4 PS_Name(float4 pos : SV_Position, float2 uv : TEXCOORD) : SV_Target0` |
| `#[uniform] const name: f64 = val;` | `uniform float name < ui_type = "slider"; ... > = val;` |
| `tex2d(uv)` | `tex2D(ReShade::BackBuffer, uv)` |
| `tex2d_depth(uv)` | `ReShade::GetLinearizedDepth(uv)` |
| `vec2(x, y)` | `float2(x, y)` |
| `vec4(r, g, b, a)` | `float4(r, g, b, a)` |
| `smoothstep()`, `clamp()`, `sin()`, `cos()` | Same names (HLSL intrinsics) |

The compiler also automatically generates:
- The `#include "ReShade.fxh"` header
- The `technique` block binding your pixel shader
- `PostProcessVS` as the vertex shader (standard ReShade full-screen pass)
- UI slider annotations for every `#[uniform]` variable

You write the math. QuantaLang writes the boilerplate.

---

## Walk-Through: Building an SSAO Effect

Screen-Space Ambient Occlusion (SSAO) is one of the most impactful post-processing effects. It simulates the soft shadows that form in crevices and corners where ambient light is partially blocked by nearby geometry.

Let us build one from scratch in QuantaLang.

### Step 1: Define Uniforms

Uniforms are parameters that ReShade exposes as sliders in its UI overlay. In QuantaLang, mark them with `#[uniform]`:

```rust
#[uniform]
const ao_radius: f64 = 0.005;

#[uniform]
const ao_intensity: f64 = 2.0;

#[uniform]
const ao_bias: f64 = 0.001;

#[uniform]
const ao_samples: f64 = 16.0;
```

The compiler translates each of these into ReShade's annotated uniform syntax:

```hlsl
uniform float ao_radius <
    ui_type = "slider";
    ui_min = 0.0; ui_max = 0.1; ui_step = 0.001;
    ui_label = "ao_radius";
> = 0.005;
```

Players can now adjust the AO radius, intensity, bias, and sample count live in-game.

### Step 2: Write Helper Functions

SSAO needs pseudo-random noise to distribute sample points. Standard noise functions work in QuantaLang exactly as you would expect:

```rust
fn hash(n: f64) -> f64 {
    let x = sin(n) * 43758.5453;
    fract(x)
}

fn hash2(a: f64, b: f64) -> f64 {
    hash(a * 127.1 + b * 311.7)
}
```

These compile to identical HLSL math -- no translation overhead.

### Step 3: Implement the AO Kernel

The core algorithm samples points in a disc around each pixel, reads the depth buffer, and accumulates occlusion where nearby geometry blocks light:

```rust
fn compute_ao(uv_x: f64, uv_y: f64) -> f64 {
    let center_depth = tex2d_depth(vec2(uv_x, uv_y));

    if center_depth > 0.999 {
        1.0  // Sky pixels: no occlusion
    } else {
        let mut occlusion = 0.0;
        let mut i = 0.0;
        while i < ao_samples {
            let angle = hash2(uv_x * 100.0 + i, uv_y * 100.0) * 6.2831853;
            let dist = hash2(i * 7.3, uv_x + uv_y) * ao_radius;

            let sample_x = uv_x + cos(angle) * dist;
            let sample_y = uv_y + sin(angle) * dist;
            let sample_depth = tex2d_depth(vec2(sample_x, sample_y));
            let diff = center_depth - sample_depth;

            if diff > ao_bias {
                let range = smoothstep(0.0, 1.0, ao_radius / abs(diff));
                occlusion = occlusion + range;
            }
            i = i + 1.0;
        }

        let ao = 1.0 - (occlusion / ao_samples) * ao_intensity;
        clamp(ao, 0.0, 1.0)
    }
}
```

Key points:
- `tex2d_depth()` reads the linearized depth buffer -- the compiler emits `ReShade::GetLinearizedDepth()`
- Early exit for sky pixels (depth near 1.0) avoids wasted work
- The disc sampling uses a pseudo-random angle and radius per sample for noise-free results

### Step 4: Write the Fragment Shader

The entry point reads the scene color, computes AO, and multiplies:

```rust
#[fragment]
fn PS_SSAO(uv: vec2) -> vec4 {
    let color = tex2d(uv);
    let ao = compute_ao(uv.x, uv.y);
    vec4(color.x * ao, color.y * ao, color.z * ao, 1.0)
}
```

### Step 5: Compile

```bash
quantac ssao.quanta --target hlsl -o ssao.fx
```

### The Generated .fx File

The compiler produces a complete, drop-in ReShade effect:

```hlsl
// Generated by QuantaLang Compiler
// Target: ReShade Effect (.fx)

#include "ReShade.fxh"

uniform float ao_radius <
    ui_type = "slider";
    ui_min = 0.0; ui_max = 0.1; ui_step = 0.001;
    ui_label = "ao_radius";
> = 0.005;

uniform float ao_intensity <
    ui_type = "slider";
    ui_min = 0.0; ui_max = 20.0; ui_step = 0.1;
    ui_label = "ao_intensity";
> = 2.0;

// ... (bias and samples uniforms) ...

float hash(float n) {
    float x = (sin(n) * 43758.5453);
    return frac(x);
}

// ... (helper functions) ...

float compute_ao(float uv_x, float uv_y) {
    float center_depth = ReShade::GetLinearizedDepth(float2(uv_x, uv_y));
    if (center_depth > 0.999) {
        return 1.0;
    } else {
        // ... (disc sampling loop) ...
    }
}

float4 PS_SSAO(float4 pos : SV_Position, float2 uv : TEXCOORD) : SV_Target0 {
    float4 color = tex2D(ReShade::BackBuffer, uv);
    float ao = compute_ao(uv.x, uv.y);
    return float4((color.x * ao), (color.y * ao), (color.z * ao), 1.0);
}

technique Quanta_PS_SSAO {
    pass {
        VertexShader = PostProcessVS;
        PixelShader = PS_SSAO;
    }
}
```

Copy `ssao.fx` into your ReShade `Shaders` folder, enable it in the overlay, and you have ambient occlusion in any game.

---

## The Five Shader Demos

The QuantaLang `demos/` directory ships with five fully working ReShade effects, each demonstrating different shader techniques:

### 1. SSAO (`ssao.quanta`)

Screen-Space Ambient Occlusion. Disc-based sampling with depth comparison and range-weighted occlusion. 4 uniforms, ~83 lines of QuantaLang. The effect described in detail above.

### 2. Bloom (`bloom.quanta`)

Two-pass bright extraction and Gaussian blur. Pass 1 extracts pixels above a luminance threshold. Pass 2 applies a 9-tap cross-shaped Gaussian blur. 2 uniforms (threshold, intensity), ~139 lines.

```rust
#[uniform]
const threshold: f64 = 0.7;

#[uniform]
const intensity: f64 = 1.0;

fn luminance(r: f64, g: f64, b: f64) -> f64 {
    r * 0.2126 + g * 0.7152 + b * 0.0722
}

// ... extraction and blur functions ...

#[fragment]
fn PS_Bloom(uv: vec2) -> vec4 {
    let color = tex2d(uv);
    // Extract, blur, composite
    // ...
}
```

### 3. Depth of Field (`depth_of_field.quanta`)

Circle-of-confusion based blur that simulates camera focus. 8-tap disc kernel for smooth bokeh. 2 uniforms (focus_distance, aperture), ~83 lines.

```rust
#[uniform]
const focus_distance: f64 = 0.3;

#[uniform]
const aperture: f64 = 0.01;

fn circle_of_confusion(depth: f64) -> f64 {
    let diff = abs(depth - focus_distance);
    let coc = diff * aperture / focus_distance;
    clamp(coc, 0.0, 0.02)
}
```

### 4. Chromatic Aberration (`chromatic_aberration.quanta`)

Simulates lens fringing by radially offsetting RGB channels. Red shifts outward, blue shifts inward, green stays centered. 1 uniform (strength), ~57 lines.

```rust
#[uniform]
const strength: f64 = 0.005;

#[fragment]
fn PS_ChromaticAberration(uv: vec2) -> vec4 {
    let red_sample = tex2d(vec2(offset_outward(uv)));
    let green_sample = tex2d(uv);
    let blue_sample = tex2d(vec2(offset_inward(uv)));
    vec4(red_sample.x, green_sample.y, blue_sample.z, 1.0)
}
```

### 5. Film Grain (`film_grain.quanta`)

Pseudo-random noise overlay for a cinematic film look. Quantized UV noise with time-based animation. 3 uniforms (grain_amount, grain_size, frame_time), ~62 lines.

```rust
#[uniform]
const grain_amount: f64 = 0.08;

#[uniform]
const grain_size: f64 = 1.0;

#[uniform]
const frame_time: f64 = 0.0;
```

---

## From QuantaLang to In-Game: The Workflow

Here is the complete workflow for developing a ReShade effect with QuantaLang:

```
1. Write effect.quanta
   |
2. quantac effect.quanta --target hlsl -o effect.fx
   |
3. Copy effect.fx to <Game>/reshade-shaders/Shaders/
   |
4. Open game -> ReShade overlay -> Enable effect
   |
5. Tweak uniforms in the ReShade UI
   |
6. Edit .quanta source -> Recompile -> Reload in ReShade
```

Steps 2-6 are a fast iteration loop. The compiler runs in milliseconds, ReShade reloads shaders on demand, and the uniform sliders give you instant parameter feedback.

---

## Why Not Just Write HLSL Directly?

You can. People do. But QuantaLang gives you several advantages:

**Cross-target output.** If a game uses Vulkan instead of DirectX, recompile to GLSL. Same source, different target. No rewrite.

**Type-safe uniforms.** The `#[uniform]` attribute is type-checked. If you reference a uniform that does not exist, the compiler catches it before you load the shader in-game.

**Readable source.** Compare QuantaLang's `tex2d(uv)` to ReShade's `tex2D(ReShade::BackBuffer, uv)`. Compare `#[fragment]` to manually writing semantics. The QuantaLang version is shorter and expresses intent more clearly.

**CPU execution for testing.** Use `quantac run` to execute shader math on the CPU. Validate your algorithm with print statements before you ever load it in a game.

```bash
# Test the math without a GPU
quantac run ssao.quanta
```

**One source of truth.** No drift between HLSL and GLSL versions. No forgotten bug fixes in one target but not the other.

---

## Get Started

```bash
git clone https://github.com/HarperZ9/quantalang.git
cd quantalang
cargo build --release

# Compile all five demos to .fx
for f in demos/ssao.quanta demos/bloom.quanta demos/depth_of_field.quanta \
         demos/chromatic_aberration.quanta demos/film_grain.quanta; do
    ./target/release/quantac "$f" --target hlsl -o "${f%.quanta}.fx"
done
```

Drop the `.fx` files into your ReShade shader folder and start tweaking.

---

*QuantaLang is open source under the MIT license. GitHub: [HarperZ9/quantalang](https://github.com/HarperZ9/quantalang)*
