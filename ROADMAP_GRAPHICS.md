# QuantaLang Graphics Roadmap

**Goal: The language elite graphics programmers choose over C++ and HLSL.**

**Identity: Mathematically precise systems programming with algebraic effects — write your engine and your shaders in one language.**

---

## Current State (v0.1.1 — March 2026)

### Proven Working (48/48 end-to-end tests)
- vec2/vec3/vec4 with constructors, arithmetic (+, -, *), field access (.x, .y, .z, .w)
- Swizzling (color.xyz, color.zyx, color.rg)
- mat4 with identity, translate, scale, perspective, multiply (mat4*mat4, mat4*vec4)
- Shader math: dot, cross, normalize, length, reflect, lerp, smoothstep, mix, fract, step, clamp
- ACES filmic tone mapping (verified correct output)
- Phong lighting model (verified correct output)
- Algebraic effects controlling rendering (Render effect with handler swap)
- Structs, enums, traits, generics, closures, modules, references, Option/Result, ? operator
- C backend (48 tests verified with MSVC), LLVM IR backend, WASM backend
- SPIR-V backend (3,600+ lines) with validated shader output

### GPU Pipeline (A1-A5 Progress)
- **A1 DONE:** SPIR-V produces valid vertex + fragment shaders (pass spirv-val)
- **A2 DONE:** Vulkan FFI bindings, window creation
- **A3 DONE:** Triangle on screen — QuantaLang-generated SPIR-V shaders on GPU
- **A5 IN PROGRESS:** Uniform buffer vertex shader (MVP matrix from UBO) — SPIR-V validates
- **A5 IN PROGRESS:** Textured fragment shader (texture sampling) — SPIR-V validates
- MIR types: Texture2D, Sampler, SampledImage, ShaderBinding, BindingKind
- Shader stage attributes: #[vertex], #[fragment], #[compute] in lowerer
- SPIR-V: OpTypeImage, OpTypeSampler, OpTypeSampledImage, OpImageSampleImplicitLod
- SPIR-V: Uniform buffer blocks with OpDecorate Block/Binding/DescriptorSet/ColMajor/MatrixStride

### Compiler Stats
- 66K+ lines Rust compiler, 477+ unit tests
- 48 end-to-end test programs verified
- 4 validated SPIR-V shader files (triangle vert/frag + uniform vert + textured frag)
- C runtime with vector math, string, file I/O, effects handler

---

## Option A: Depth — GPU Execution

### A1: SPIR-V Backend Produces Valid Shaders (Weeks 1-3)

**Goal:** `quantac build shader.quanta --target=spirv` produces a `.spv` file that `spirv-val` accepts.

**Step A1.1: Validate existing SPIR-V opcodes**
- The backend already has OpTypeVector, OpTypeMatrix, OpDot, OpVectorShuffle, all GLSL.std.450 builtins
- Create unit tests that construct MIR modules with vector operations and verify the SPIR-V output
- Use spirv-val (from Vulkan SDK) to validate the binary
- Fix any encoding issues (wrong opcode lengths, missing capabilities, incorrect type IDs)

**Step A1.2: Shader entry points**
- Implement proper vertex shader entry: `OpEntryPoint Vertex %main "main" %in_pos %out_pos`
- Input/output variable decoration: `OpDecorate %in_pos Location 0`
- Built-in output: `OpDecorate %gl_Position BuiltIn Position`
- OpExecutionMode for fragment shaders (OriginUpperLeft)

**Step A1.3: Minimal vertex + fragment shader pair**
```quanta
#[vertex]
fn vs_main(position: vec3, color: vec3) -> vec4 {
    vec4(position, 1.0)  // pass-through vertex shader
}

#[fragment]
fn fs_main(color: vec3) -> vec4 {
    vec4(color, 1.0)  // solid color fragment shader
}
```
Compile to two .spv files. Validate with spirv-val. This is the milestone.

**Step A1.4: Wire --target=spirv into CLI**
- `quantac build shader.quanta --target=spirv` produces .spv
- Multi-output: vertex and fragment shaders in separate files
- Error messages specific to shader compilation ("cannot use effect in shader function")

**Deliverable:** A valid SPIR-V shader pair that passes spirv-val, compiled from QuantaLang source.

---

### A2: Vulkan FFI Bindings (Weeks 3-5)

**Goal:** Call Vulkan API functions from QuantaLang through C FFI.

**Step A2.1: extern "C" block parsing**
- Parse `extern "C" { fn vkCreateInstance(...) -> VkResult; }` syntax
- The parser already handles the `extern` keyword (ast/item.rs has ExternBlock)
- Wire it through to the lowerer — extern functions skip body lowering and emit as C `extern` declarations

**Step A2.2: Vulkan type definitions**
```quanta
// Opaque handles
struct VkInstance { handle: u64 }
struct VkDevice { handle: u64 }
struct VkSwapchainKHR { handle: u64 }
struct VkCommandBuffer { handle: u64 }
struct VkPipeline { handle: u64 }

// Vulkan function bindings
extern "C" {
    fn vkCreateInstance(info: &VkInstanceCreateInfo, alloc: u64, instance: &mut VkInstance) -> i32;
    fn vkEnumeratePhysicalDevices(instance: VkInstance, count: &mut u32, devices: u64) -> i32;
    // ... minimal set needed for triangle
}
```

**Step A2.3: Minimal Vulkan bootstrap in QuantaLang**
Write the minimum Vulkan setup code in QuantaLang:
- Create instance
- Select physical device
- Create logical device
- Create swapchain
- Create render pass
- Create graphics pipeline (using the SPIR-V shaders from A1)
- Create command buffers
- Main render loop

This is ~500-800 lines of QuantaLang code. It's boilerplate, but it's QuantaLang boilerplate — proving the language can express Vulkan API patterns.

**Step A2.4: Window creation via GLFW FFI**
```quanta
extern "C" {
    fn glfwInit() -> i32;
    fn glfwCreateWindow(width: i32, height: i32, title: &str, monitor: u64, share: u64) -> u64;
    fn glfwWindowShouldClose(window: u64) -> i32;
    fn glfwPollEvents();
    fn glfwSwapBuffers(window: u64);
}
```

**Deliverable:** A QuantaLang program that opens a window using GLFW and creates a Vulkan instance.

---

### A3: Render a Triangle (Weeks 5-7)

**Goal:** The "Hello World" of graphics — a colored triangle on screen, with shader and engine code both in QuantaLang.

**Step A3.1: Combine A1 + A2**
- Compile vertex + fragment shaders to SPIR-V using `quantac --target=spirv`
- Load the SPIR-V into the Vulkan pipeline
- Submit draw commands

**Step A3.2: The demo program**
```quanta
// triangle.quanta — THE demo that proves QuantaLang is real

#[vertex]
fn vertex_shader(position: vec3, color: vec3) -> VertexOutput {
    VertexOutput {
        position: vec4(position, 1.0),
        color: color
    }
}

#[fragment]
fn fragment_shader(input: VertexOutput) -> vec4 {
    vec4(input.color, 1.0)
}

fn main() {
    // Create window and Vulkan context
    let window = create_window(800, 600, "QuantaLang Triangle");
    let renderer = create_vulkan_renderer(window);

    // Load shaders (compiled from THIS file)
    let pipeline = renderer.create_pipeline("vertex_shader.spv", "fragment_shader.spv");

    // Define triangle vertices
    let vertices = [
        vec3(-0.5, -0.5, 0.0), vec3(1.0, 0.0, 0.0),  // red
        vec3( 0.5, -0.5, 0.0), vec3(0.0, 1.0, 0.0),  // green
        vec3( 0.0,  0.5, 0.0), vec3(0.0, 0.0, 1.0),  // blue
    ];

    // Render loop
    while !window_should_close(window) {
        poll_events();
        renderer.begin_frame();
        renderer.bind_pipeline(pipeline);
        renderer.draw(vertices);
        renderer.end_frame();
    }
}
```

**Step A3.3: Effect-controlled rendering**
Refactor the triangle to use effects:
```quanta
effect Render {
    fn begin_frame() -> (),
    fn bind_pipeline(pipeline: u64) -> (),
    fn draw_vertices(data: u64, count: i32) -> (),
    fn end_frame() -> (),
}

// The game loop is effect-agnostic
fn game_loop(triangle: u64, pipeline: u64) ~ Render {
    perform Render.begin_frame();
    perform Render.bind_pipeline(pipeline);
    perform Render.draw_vertices(triangle, 3);
    perform Render.end_frame();
}

// The handler decides the graphics API
fn main() {
    let ctx = init_vulkan();

    handle {
        while !should_close() {
            game_loop(ctx.triangle, ctx.pipeline)
        }
    } with {
        Render.begin_frame() => |r| { vulkan_begin_frame(ctx); r },
        Render.draw_vertices(data, count) => |r| { vulkan_draw(ctx, data, count); r },
        Render.end_frame() => |r| { vulkan_end_frame(ctx); r },
    }
}
```

**Deliverable:** A QuantaLang program that renders a colored triangle on screen, with both shader code and engine code written in QuantaLang.

---

### A4: Post-Processing Pipeline (Weeks 7-9)

**Goal:** Apply the ACES tone mapping (already proven in test 39) as a GPU post-processing pass.

**Step A4.1: Full-screen quad rendering**
- Render a full-screen quad with a fragment shader
- Sample from a render target texture

**Step A4.2: ACES tone mapping as a shader**
```quanta
#[fragment]
fn tonemap_pass(uv: vec2) -> vec4 {
    let hdr_color = texture_sample(hdr_buffer, uv);
    let mapped = aces_tonemap(hdr_color.xyz);
    vec4(mapped, 1.0)
}
```

The same `aces_tonemap` function from test 39 — now running on the GPU.

**Step A4.3: Multi-pass rendering**
- Pass 1: Render scene to HDR render target
- Pass 2: Apply tone mapping
- Pass 3: Present to screen

This demonstrates QuantaLang's unique value: the SAME function compiles to both CPU (for unit testing) and GPU (for real-time rendering).

**Deliverable:** A post-processing pipeline running on the GPU, using QuantaLang shaders that were previously tested on the CPU.

---

### A5: Texture Sampling and Uniform Buffers (Weeks 9-11)

**Goal:** Read textures and receive per-frame data from the CPU.

**Step A5.1: SPIR-V texture/sampler support**
- OpTypeSampler, OpTypeSampledImage, OpImageSampleImplicitLod
- Texture coordinate interpolation from vertex to fragment

**Step A5.2: Uniform buffer support**
```quanta
#[uniform(binding = 0)]
struct CameraData {
    view: mat4,
    projection: mat4,
    position: vec3,
}

#[vertex]
fn vs_main(position: vec3, camera: CameraData) -> vec4 {
    camera.projection * camera.view * vec4(position, 1.0)
}
```

**Step A5.3: MVP transform demo**
- Rotating cube with model/view/projection matrices
- Camera controlled by uniform buffer updated from CPU each frame

**Deliverable:** A textured, lit, rotating cube rendered by QuantaLang shaders with uniform buffer input.

---

## Option B: Breadth — Language Completeness

### B1: Type System Improvements (Weeks 11-13)

**Step B1.1: Fix vec3 parameter field access**
- The type checker loses vector type info through function parameters
- `fn foo(v: vec3) -> f64 { v.x }` fails because `v` has type `?T` not `quanta_vec3`
- Fix: propagate type annotations through function parameter type resolution

**Step B1.2: String != operator**
- Currently `s != ""` generates raw struct comparison instead of `!quanta_string_eq()`
- Fix in lower_binary for Ne on QuantaString types

**Step B1.3: Generic enum/struct types**
- `Option<T>` instead of monomorphized `OptionI32`
- Requires type parameter substitution in struct/enum type definitions during lowering

**Step B1.4: Iterator trait and for-in**
- `for item in collection { ... }` desugars to iterator protocol
- Requires trait dispatch for Iterator::next()

### B2: Standard Library Expansion (Weeks 13-15)

**Step B2.1: Vec<T> backed by QuantaVec runtime**
- Push, pop, get, set, len, capacity
- For-in iteration

**Step B2.2: HashMap<K,V>**
- Basic hash map with string keys (simplest useful case)
- Get, set, contains, remove

**Step B2.3: I/O improvements**
- File read/write working end-to-end (runtime functions exist, need testing)
- stdin/stdout/stderr handles
- Buffered I/O

### B3: Multi-Shot Continuations (Weeks 15-18)

**Step B3.1: Replace setjmp/longjmp with segmented stacks or CPS**
- Current one-shot model: only first perform in a handle block fires
- Multi-shot: generators, async streams, backtracking search all become possible
- Approach: CPS transform in the MIR → C backend, or embed libmprompt

**Step B3.2: Async/await as effect sugar**
```quanta
// async/await is just syntax sugar for the Async effect
async fn fetch_data(url: str) -> str {
    // desugars to: perform Async.yield_() between IO operations
}
```

**Step B3.3: Effect polymorphism in codegen**
```quanta
fn map<A, B, E>(items: Vec<A>, f: fn(A) ~ E -> B) ~ E -> Vec<B>
```
- The type system already supports open effect rows
- Codegen needs to handle effect row variables

### B4: Package Ecosystem (Weeks 18-20)

**Step B4.1: Wire package manager to CLI**
- `quantac pkg init` — create Quanta.toml
- `quantac pkg add json` — add dependency
- `quantac pkg build` — build with dependencies

**Step B4.2: Module system improvements**
- Nested modules: `mod rendering::pipeline`
- pub/private visibility
- use glob imports: `use math::*`

**Step B4.3: LSP completion**
- Wire the LSP server properly
- VS Code extension with syntax highlighting
- Go-to-definition, autocomplete, inline errors

### B5: Error Messages and Diagnostics (Weeks 20-22)

**Step B5.1: Source location in errors**
- "error at shader.quanta:15:8: undefined variable 'texCoord'"
- Underline the exact span in the source

**Step B5.2: Suggestion engine**
- "did you mean 'vec3'?" for typos
- "function requires effect ~IO, add it to the signature" for unhandled effects

**Step B5.3: Warning system**
- Unused variables, unreachable code
- Performance warnings for shader code (branching in fragment shaders, etc.)

---

## Option C: The Demo — Showcase Application

### C1: Spinning Cube Demo (Weeks 22-24)

**Goal:** One program. One file. Opens a window. Renders a spinning, lit, textured cube. Shader and engine code both in QuantaLang. Effects control the rendering pipeline.

```quanta
// cube_demo.quanta — The QuantaLang Graphics Showcase
//
// This single file contains:
// - Vertex and fragment shaders (compile to SPIR-V for GPU)
// - Phong lighting with specular highlights
// - ACES tone mapping post-process
// - Matrix transforms (model/view/projection)
// - Effect-controlled rendering (swap Vulkan/mock/profiler)
// - All in one language, all type-checked together

#[vertex]
fn vs_main(pos: vec3, normal: vec3, uv: vec2, mvp: mat4) -> VertexOutput {
    VertexOutput {
        position: mvp * vec4(pos, 1.0),
        world_normal: normal,
        tex_coord: uv,
    }
}

#[fragment]
fn fs_main(input: VertexOutput) -> vec4 {
    let light = normalize(vec3(1.0, 1.0, 0.5));
    let color = phong_lighting(input.world_normal, light, vec3(0.0, 0.0, 1.0), 64.0);
    let mapped = aces_tonemap(color);
    vec4(mapped, 1.0)
}

effect Render {
    fn clear(color: vec4) -> (),
    fn draw_mesh(mesh: u64, transform: mat4) -> (),
    fn present() -> (),
}

fn main() {
    let window = create_window(1280, 720, "QuantaLang Cube");
    let ctx = init_vulkan(window);
    let cube = load_cube_mesh(ctx);
    let mut angle: f64 = 0.0;

    handle {
        while !should_close(window) {
            angle = angle + 0.01;
            let model = mat4_rotate_y(angle);
            let view = mat4_look_at(vec3(0.0, 2.0, 5.0), vec3(0.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0));
            let proj = mat4_perspective(0.785, 1.78, 0.1, 100.0);
            let mvp = proj * view * model;

            perform Render.clear(vec4(0.1, 0.1, 0.15, 1.0));
            perform Render.draw_mesh(cube, mvp);
            perform Render.present();
            poll_events(window);
        }
    } with {
        Render.clear(c) => |r| { vk_clear(ctx, c); r },
        Render.draw_mesh(m, t) => |r| { vk_draw(ctx, m, t); r },
        Render.present() => |r| { vk_present(ctx); r },
    }
}
```

This is the demo that changes minds. One file. Shader + engine. CPU-testable lighting. GPU-accelerated rendering. Effect-swappable backend.

### C2: ReShade-Style Post-Processing Framework (Weeks 24-28)

**Goal:** Build a QuantaLang version of ReShade's post-processing framework — the kind of tool Pascal Gilcher works with daily.

- Multiple configurable post-processing effects
- Each effect is a QuantaLang module with a fragment shader
- Effects chain: scene → bloom → tone map → color grade → FXAA → output
- UI for enabling/disabling effects (using effects for UI state!)
- Hot reload: change a shader file, see it update immediately

### C3: Minimal Game Engine (Weeks 28-36)

**Goal:** A small but complete game engine written entirely in QuantaLang.

- Scene graph with entities, components, transforms
- Forward rendering with point/directional lights
- Material system with shader permutations
- Asset loading (OBJ meshes, PNG textures via FFI)
- Input handling (keyboard/mouse via GLFW FFI)
- Physics (simple AABB collision)
- Audio (via FFI to miniaudio or similar)

Everything uses effects:
```quanta
effect Physics { fn step(dt: f64) -> (), fn raycast(origin: vec3, dir: vec3) -> Hit }
effect Audio { fn play(sound: str) -> (), fn set_volume(vol: f64) -> () }
effect Input { fn is_key_pressed(key: i32) -> bool, fn mouse_pos() -> vec2 }
```

Swap handlers for testing. Record effects for replay. Profile by timing handlers.

---

## Timeline Summary

| Phase | Weeks | Milestone |
|-------|-------|-----------|
| **A1** | 1-3 | SPIR-V backend produces valid shader pairs |
| **A2** | 3-5 | Vulkan FFI bindings, window creation |
| **A3** | 5-7 | **Triangle on screen** — both shader and engine in QuantaLang |
| **A4** | 7-9 | Post-processing (ACES tone mapping on GPU) |
| **A5** | 9-11 | Textures, uniform buffers, MVP transforms |
| **B1** | 11-13 | Type system fixes, generic types |
| **B2** | 13-15 | Vec, HashMap, I/O stdlib |
| **B3** | 15-18 | Multi-shot continuations, async/await |
| **B4** | 18-20 | Package manager, module improvements, LSP |
| **B5** | 20-22 | Error messages, warnings, diagnostics |
| **C1** | 22-24 | **Spinning cube demo** — the showcase |
| **C2** | 24-28 | ReShade-style post-processing framework |
| **C3** | 28-36 | Minimal game engine |

---

## Success Criteria

**For Pascal Gilcher:** "I can write my ReShade shaders in QuantaLang, test them on CPU, compile to SPIR-V, and hot reload them. The swizzling and math feel native. Effects let me swap between quality presets without recompiling."

**For Boris Vorontsov:** "I can build ENB-level post-processing in QuantaLang with zero-overhead abstractions. The memory layout control is precise. The SPIR-V output is optimal. I can inject effects into any game's rendering pipeline."

**For every game programmer:** "I write my engine and my shaders in the same language. I test rendering on CPU. I debug shader math with printf. The effect system replaces my dependency injection framework, my event system, and my command pattern — all at once."

---

*QuantaLang: Mathematically precise. GPU-native. Effect-controlled. One language for everything.*
