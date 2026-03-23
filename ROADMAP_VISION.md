# QuantaLang: The Graphics Programming Language
# Vision Roadmap — Making QuantaLang World-Changing

## Context

QuantaLang has proven itself: 70K-line Rust compiler, 101 end-to-end tests, 5 Python apps ported, all 8 milestones from the original roadmap delivered. The language works.

Now it needs to become **the language elite graphics programmers demand** — the tool Pascal Gilcher, Boris Vorontsov, and every AAA shader programmer would choose over HLSL/GLSL/C++.

**The core thesis:** No language lets you write math once and have it run identically on CPU and GPU, catches color space errors at compile time, and tracks floating-point precision through a rendering pipeline. QuantaLang will be the first.

## Current State Assessment

**What we have (working):**
- SPIR-V backend: 4,274 lines, generates valid SPIR-V binaries (spirv-val passes)
- Hardcoded shader generators produce valid vertex/fragment/compute shaders
- Algebraic effects system with row polymorphism (proven end-to-end)
- TypeKind::WithEffect EXISTS in the AST (not yet wired to parser/type checker)
- Higher-kinded type infrastructure with Kind::Effect and Kind::Row
- Operator overloading, generics, multi-module compilation all working

**What's missing (the gaps):**
- End-to-end QuantaLang source → SPIR-V → runs on GPU: NOT proven
- MIR→SPIR-V has ~50% operation coverage (many ops return zero/stub)
- No Vulkan host runtime (only printf stubs)
- WithEffect type annotations: not parsed, not lowered, not checked
- No color space types, no precision tracking
- DXIL backend: does not exist

---

## Three Pillars (Sequential, Each Builds on Previous)

### Pillar 1: True Dual-Target Compilation (IMMEDIATE)
*Write once, run on CPU AND GPU — identical results*

### Pillar 2: Color Space & Precision Type System (NEXT)
*The compiler catches the bugs that take days to find*

### Pillar 3: Render Graph & Pipeline Language (FUTURE)
*Replace raw DirectX/Vulkan calls with compiler-verified constructs*

---

## Pillar 1: True Dual-Target (Q2-Q3 2027)

**Goal:** A QuantaLang function with `#[fragment]` compiles to SPIR-V, loads in Vulkan, renders pixels.

### Phase 1A: Wire the QuantaLang→SPIR-V pipeline end-to-end

**Problem:** The SPIR-V backend has hardcoded shader generators that produce valid SPIR-V, but the normal AST→MIR→SPIR-V path has ~50% operation coverage. Many MIR operations fall through to a default that returns zero.

**Deliverables:**
1. Fix MIR→SPIR-V operation coverage: implement FieldAccess, VariantField, IndexAccess, pointer stores, globals
2. Make `quantac compile shader.quanta --target=spirv` produce valid SPIR-V from QuantaLang source
3. Test: compile a real fragment shader (PBR BRDF from test 75) to SPIR-V, validate with spirv-val

**Key file:** `quantalang/compiler/src/codegen/backend/spirv.rs`
- Lines 1815-1817: pointer store stubs → implement OpStore
- Lines 1911-1915: default zero fallback → implement remaining RValues
- Line 669+: `gen_function()` entry point for compilation

### Phase 1B: Minimal Vulkan runtime

**Problem:** No host code exists to actually load and execute SPIR-V shaders.

**Deliverables:**
1. Write a minimal Vulkan host library in C (~500 lines): create instance, device, swapchain, render pass, pipeline
2. Expose as C FFI functions callable from QuantaLang: `vk_init()`, `vk_create_pipeline(spv_bytes)`, `vk_draw()`, `vk_present()`
3. Wire into the QuantaLang C runtime so a compiled program can create a window and render

**Key files:**
- New: `quantalang/runtime/vulkan_host.c` — minimal Vulkan host
- Modify: `quantalang/compiler/src/codegen/runtime.rs` — add vulkan FFI declarations

### Phase 1C: The Proof — render pixels from QuantaLang

**Deliverables:**
1. Write a complete QuantaLang program: vertex shader + fragment shader + host code
2. Compile vertex/fragment to SPIR-V, host to native via C
3. Program opens a window, renders a colored triangle, outputs a screenshot
4. Same BRDF math compiled to C produces identical output values

**Test:** `102_vulkan_triangle.quanta` — first QuantaLang program rendering real pixels via Vulkan
**Proof point:** C and SPIR-V backends produce bit-identical results for PBR functions

---

## Pillar 2: Color Space & Precision Types (Q3-Q4 2027)

**Goal:** `fn tonemap(c: LinearRGB) -> sRGB` — the compiler catches color space mixing errors.

### Phase 2A: Parse and lower WithEffect on value types

**Problem:** `TypeKind::WithEffect { ty, effects }` exists in the AST but is never parsed or lowered.

**Deliverables:**
1. Extend the parser to handle `Type with Effect1, Effect2` in parameter positions
2. Lower WithEffect to the type system as a new TyKind variant
3. Propagate through function signatures: if input is `with ColorSpace<Linear>`, output is too

**Key files:**
- `quantalang/compiler/src/parser/ty.rs` — parse `with` after type
- `quantalang/compiler/src/types/ty.rs` — add TyKind::Annotated { base, annotations }
- `quantalang/compiler/src/types/infer.rs` — propagate annotations through unification

### Phase 2B: Color space effect definitions

**Deliverables:**
1. Define built-in color space effects: `ColorSpace<Linear>`, `ColorSpace<sRGB>`, `ColorSpace<ACEScg>`, `ColorSpace<P3>`
2. Define conversion functions that change the color space annotation
3. Compiler error when mixing: `let wrong: sRGB = my_linear_color;`

**Syntax:**
```quanta
effect ColorSpace<S> {}

type LinearRGB = vec3 with ColorSpace<Linear>;
type sRGBColor = vec3 with ColorSpace<sRGB>;

fn srgb_to_linear(c: sRGBColor) -> LinearRGB { ... }

// COMPILE ERROR: expected LinearRGB, got sRGBColor
let result = pbr_shade(my_srgb_color);
```

### Phase 2C: Precision annotations

**Deliverables:**
1. `#[precision(bits = 23)]` on function parameters and returns
2. Compiler tracks precision loss through arithmetic chains
3. Warning when precision drops below a threshold

**This is a research-level feature** — start simple with annotations, work toward full tracking.

---

## Pillar 3: Render Graph Language (2028+)

**Goal:** Render passes are first-class compiler-verified constructs.

This pillar depends on Pillars 1+2 being complete. Design only — no implementation yet.

**Vision:**
```quanta
render_graph deferred_pipeline {
    pass gbuffer {
        vertex: gbuffer_vertex,
        fragment: gbuffer_fragment,
        outputs: [albedo: RGBA8, normal: RGB16F, depth: D32F],
    }
    pass lighting {
        compute: tiled_lighting,
        inputs: [gbuffer.albedo, gbuffer.normal, gbuffer.depth],
        outputs: [hdr: RGBA16F],
    }
    pass tonemap {
        fragment: aces_tonemap,
        inputs: [lighting.hdr],
        outputs: [display: sRGB_RGBA8],
    }
}
```

The compiler verifies:
- All inputs satisfied by previous pass outputs
- No circular dependencies
- Format compatibility between connected passes
- Color space correctness at each stage
- Memory layout optimization

---

## Implementation Priority

**Do first (Pillar 1A):** Fix MIR→SPIR-V operation coverage
- This is the single most impactful change
- Currently ~50% of MIR operations are handled
- Most gaps are mechanical — implement the same pattern for each missing op
- Estimated: 200-300 lines of Rust additions to spirv.rs

**Do second (Pillar 1B):** Minimal Vulkan host
- ~500 lines of C
- Standard boilerplate (every Vulkan tutorial covers this)
- Wire as FFI functions in the QuantaLang runtime

**Do third (Pillar 1C):** The proof
- Write the triangle demo in QuantaLang
- This is the moment QuantaLang becomes real for graphics programmers

**Then (Pillar 2A-2C):** Color space types
- Parser changes: small
- Type system changes: medium
- Precision tracking: large (research-level)

---

## Success Criteria

| Milestone | Metric |
|-----------|--------|
| Pillar 1A | `quantac compile shader.quanta --target=spirv` produces valid SPIR-V from QuantaLang source |
| Pillar 1B | A C program linked with the Vulkan host lib renders a triangle using QuantaLang-compiled SPIR-V |
| Pillar 1C | A pure QuantaLang program (no C) opens a window and renders pixels via Vulkan |
| Pillar 2A | `fn foo(c: vec3 with ColorSpace<Linear>)` parses, type-checks, and compiles |
| Pillar 2B | Compiler error when passing sRGB color to function expecting Linear |
| Pillar 2C | Precision warning after 10-pass pipeline |

## Files to Modify

**Pillar 1:**
- `compiler/src/codegen/backend/spirv.rs` — MIR op coverage (primary work)
- `compiler/src/codegen/runtime.rs` — Vulkan FFI declarations
- `compiler/src/main.rs` — ensure --target=spirv works end-to-end
- New: `runtime/vulkan_host.c` — minimal Vulkan host

**Pillar 2:**
- `compiler/src/parser/ty.rs` — parse `with` annotations
- `compiler/src/types/ty.rs` — TyKind::Annotated
- `compiler/src/types/infer.rs` — annotation propagation
- `compiler/src/types/effects.rs` — color space effect definitions

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| SPIR-V MIR coverage too complex | Focus on shader-relevant ops only (no closures, no HashMap in shaders) |
| Vulkan host code is large | Use minimal triangle-only subset, expand later |
| Color space types affect all type inference | Make annotations optional — existing code unaffected |
| Precision tracking is research-level | Start with simple annotations, defer full tracking |
