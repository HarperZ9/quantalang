# Type-Safe Color Spaces in Shader Code

*Accidentally mixing sRGB and linear colors is one of the most common graphics bugs. QuantaLang catches it at compile time.*

---

## The Problem: Color Spaces Are Invisible

Every color value on a computer exists in a color space. The two most common are:

- **sRGB (gamma-encoded):** What your monitor displays. Pixel values are perceptually uniform -- the difference between 0.0 and 0.1 looks about the same as the difference between 0.5 and 0.6. Image files (PNG, JPEG) store colors in sRGB. UI frameworks render in sRGB. Your eyes expect sRGB.

- **Linear (scene-referred):** What physically-based lighting math requires. Light intensity is proportional to the stored value. Doubling the value doubles the photons. All lighting calculations -- diffuse shading, specular highlights, shadow attenuation, ambient occlusion -- must happen in linear space to produce correct results.

The conversion between them is a power curve:

```
linear = sRGB ^ 2.2        (approximate)
sRGB   = linear ^ (1/2.2)  (approximate)
```

### Why This Matters

If you multiply an sRGB color by a lighting factor, you get wrong results. The math assumes linearity, but sRGB values are curved. The visual symptoms are:

- **Washed-out lighting:** Diffuse shading looks flat because the sRGB curve compresses darks.
- **Over-bright highlights:** Specular reflections blow out because the curve amplifies brights.
- **Banding in shadows:** Gradients in dark areas show visible steps because sRGB allocates fewer values there.
- **Incorrect blending:** Alpha blending two sRGB colors produces a blend that is darker than expected.

These are not hypothetical bugs. They are pervasive in real graphics code. The Khronos Group, the people behind OpenGL and Vulkan, have published multiple guides about getting color spaces right. Game engines have shipped with color space bugs for years before catching them.

### Why It Keeps Happening

The root cause is simple: `vec3(0.8, 0.4, 0.2)` looks the same whether it is sRGB or linear. The type system does not know. The compiler does not know. The only thing that knows is the programmer, and programmers forget.

In HLSL:

```hlsl
float3 albedo = tex2D(diffuseMap, uv).rgb;    // sRGB? Linear? Who knows?
float3 result = albedo * NdotL;                // Wrong if albedo is sRGB
```

In GLSL:

```glsl
vec3 albedo = texture(diffuseMap, uv).rgb;     // Same problem
vec3 result = albedo * NdotL;                  // Same bug
```

There is no type distinction between "a color in sRGB" and "a color in linear space." They are both `float3` / `vec3`. The type system is blind to the most important property of the data.

---

## QuantaLang's Solution: `with ColorSpace<T>` Annotations

QuantaLang introduces compile-time color space annotations. You tag a value with its color space, and the type checker enforces that operations respect the space.

### Declaring Color Space Intent

```rust
struct RGB {
    r: f64,
    g: f64,
    b: f64,
}

// This function REQUIRES linear-space input
fn pbr_shade(albedo: RGB with ColorSpace<Linear>, n_dot_l: f64) -> RGB with ColorSpace<Linear> {
    RGB {
        r: albedo.r * n_dot_l / PI,
        g: albedo.g * n_dot_l / PI,
        b: albedo.b * n_dot_l / PI,
    }
}
```

The `with ColorSpace<Linear>` annotation is part of the type signature. It tells the compiler and every reader of this code: this function operates in linear space. If you pass it sRGB data, something is wrong.

### What the Compiler Catches

When annotated code interacts with mismatched spaces, the type checker reports it:

```rust
fn main() {
    // Unannotated color -- compatible with any space (gradual adoption)
    let color = RGB { r: 0.8, g: 0.4, b: 0.2 };
    let result = pbr_shade(color, 0.7);  // OK: unannotated is permissive
    println!("shaded: {}", result.r);
}
```

Output:
```
shaded: 0.178254
```

The color space system is designed for **gradual adoption**:

- **Unannotated code compiles without errors.** Existing code does not break. You do not need to annotate your entire codebase to start using color spaces.
- **Annotated code enforces correctness.** Once you add `with ColorSpace<Linear>` to a function signature, the compiler holds you to it.
- **Mixing annotated and unannotated is allowed.** Unannotated values are treated as compatible with any space. This lets you adopt the system one function at a time.

### The Strictness Spectrum

QuantaLang's color space checking operates on a spectrum from permissive to strict:

```
Level 1: No annotations           -> No checking (legacy behavior)
Level 2: Some functions annotated  -> Partial checking (gradual adoption)
Level 3: Full pipeline annotated   -> Complete checking (maximum safety)
```

At Level 3, the compiler can trace color data through the entire pipeline and catch every mismatch.

---

## The Annotation Pipeline

Here is how color space annotations flow through the compiler:

```
Source (.quanta)
  |
  fn pbr_shade(albedo: RGB with ColorSpace<Linear>, ...) -> RGB with ColorSpace<Linear>
  |
  v
Parser
  |
  Parses `with ColorSpace<Linear>` as a type annotation qualifier
  |
  v
Type Checker
  |
  Records color space metadata on each typed expression
  Checks function call arguments against parameter annotations
  Reports mismatches as compile-time errors
  |
  v
Shader Output
  |
  Emits color space as comments for documentation:
  // ColorSpace: Linear
  float3 pbr_shade(float3 albedo, float NdotL) { ... }
```

### What Gets Generated

When you compile an annotated shader to HLSL or GLSL, the color space information appears as documentation comments in the output. The GPU does not have a type system for color spaces -- no shader language does. But the generated code carries the annotations as comments so that anyone reading the shader output understands the intent.

```hlsl
// Generated by QuantaLang Compiler
// ColorSpace annotation: albedo is Linear
float3 pbr_shade(float3 albedo, float n_dot_l) {
    return float3(
        albedo.x * n_dot_l / 3.14159,
        albedo.y * n_dot_l / 3.14159,
        albedo.z * n_dot_l / 3.14159
    );
}
```

The real value is not in the output comments -- it is in the compile-time check that runs before the output is ever generated.

---

## Practical Example: A Color-Correct Pipeline

Here is how you would structure a color-correct post-processing pipeline in QuantaLang:

### Step 1: Define the Conversion Functions

```rust
fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        pow((c + 0.055) / 1.055, 2.4)
    }
}

fn linear_to_srgb(c: f64) -> f64 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * pow(c, 1.0 / 2.4) - 0.055
    }
}
```

### Step 2: Annotate the Shading Pipeline

```rust
fn shade_pixel(
    albedo: RGB with ColorSpace<Linear>,
    normal: vec3,
    light_dir: vec3,
    light_color: RGB with ColorSpace<Linear>,
) -> RGB with ColorSpace<Linear> {
    let n_dot_l = clamp(dot(normal, light_dir), 0.0, 1.0);
    RGB {
        r: albedo.r * light_color.r * n_dot_l,
        g: albedo.g * light_color.g * n_dot_l,
        b: albedo.b * light_color.b * n_dot_l,
    }
}
```

### Step 3: Convert at the Boundaries

The key discipline is: convert sRGB to linear at input, do all math in linear, convert back to sRGB at output.

```rust
#[fragment]
fn PS_ColorCorrectShading(uv: vec2) -> vec4 {
    // Read from texture (sRGB)
    let tex_color = tex2d(uv);

    // Convert to linear for math
    let linear_r = srgb_to_linear(tex_color.x);
    let linear_g = srgb_to_linear(tex_color.y);
    let linear_b = srgb_to_linear(tex_color.z);
    let linear_albedo = RGB { r: linear_r, g: linear_g, b: linear_b };

    // Shade in linear space
    let normal = vec3(0.0, 1.0, 0.0);
    let light_dir = vec3(0.5, 0.7, 0.3);
    let light_color = RGB { r: 1.0, g: 0.95, b: 0.9 };
    let shaded = shade_pixel(linear_albedo, normal, light_dir, light_color);

    // Convert back to sRGB for display
    let out_r = linear_to_srgb(shaded.r);
    let out_g = linear_to_srgb(shaded.g);
    let out_b = linear_to_srgb(shaded.b);

    vec4(out_r, out_g, out_b, 1.0)
}
```

Without annotations, someone might skip the `srgb_to_linear` step and pass the texture color directly to `shade_pixel`. The result would compile and run, but the shading would be wrong. With annotations, the compiler checks the color space at every function boundary.

---

## Why This Matters Beyond Shaders

Color space bugs are not limited to games and real-time graphics. They affect:

### Photography and Video

Photo editors that apply curves, levels, or color balance in sRGB instead of linear produce results that do not match the photographer's intent. Skin tones shift. Highlights clip asymmetrically. Gradients band.

### Medical Imaging

Diagnostic displays are calibrated to specific color spaces (DICOM GSDF). Applying image processing in the wrong space can change the apparent brightness of features, potentially affecting diagnosis.

### Color Grading

Film colorists work in specific color spaces (ACEScg, ACES 2065-1, DCI-P3). Accidentally mixing spaces produces color shifts that are difficult to diagnose because the result "looks close enough" until you compare side-by-side.

### Display Calibration

QuantaLang's sibling project, Calibrate Pro, is a display calibration tool that measures and corrects color accuracy. Color space correctness is not optional in that domain -- a miscalibrated conversion function means every subsequent measurement is wrong.

In all of these fields, the bug is the same: a color value passes through a function that assumes a different space than the one the value actually occupies. The type system should catch this. QuantaLang's does.

---

## Comparison with Other Approaches

### Existing Shader Languages (HLSL, GLSL, MSL)

No color space concept. `float3` is `float3`. The programmer is fully responsible for tracking spaces manually.

### Rust (on CPU)

Rust's type system could encode color spaces via newtype wrappers:

```rust
struct Linear(f32);
struct SRGB(f32);
```

But this is a manual convention. No shader language supports it on the GPU side.

### QuantaLang

Color spaces are part of the type annotation system. They are checked at compile time across the full pipeline -- CPU code and shader output alike. The annotations follow the data through function calls, assignments, and return values.

```rust
// Caught at compile time, not at 2 AM during a color grading session
fn process(color: RGB with ColorSpace<Linear>) -> RGB with ColorSpace<Linear> {
    // The type checker knows this function's contract
}
```

---

## Current State and Roadmap

Color space annotations in QuantaLang are functional today:

- The `with ColorSpace<T>` syntax parses and type-checks.
- The compiler enforces annotations at function boundaries.
- Unannotated code remains fully compatible (gradual adoption).
- Integration test `107_colorspace_mismatch.quanta` validates the feature end-to-end.

Future directions:

- **Automatic conversion insertion:** The compiler could automatically insert `srgb_to_linear()` when an sRGB value flows into a linear-annotated parameter.
- **Additional color spaces:** Support for `DisplayP3`, `ACEScg`, `Rec2020`, and custom spaces.
- **Shader output annotations:** Richer comments or metadata in the generated HLSL/GLSL that downstream tools can consume.

---

## Try It

```bash
git clone https://github.com/HarperZ9/quantalang.git
cd quantalang
cargo build --release

# Run the color space test
./target/release/quantac run tests/programs/107_colorspace_mismatch.quanta
```

Expected output:
```
unannotated: 0.178254
color space checking: ACTIVE
```

The type checker is live. Your colors are safe.

---

*QuantaLang is open source under the MIT license. GitHub: [HarperZ9/quantalang](https://github.com/HarperZ9/quantalang)*
