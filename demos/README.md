# QuantaLang Visual Demo

**Renders a colored triangle on the GPU using QuantaLang-compiled SPIR-V shaders.**

## What This Proves

1. QuantaLang's SPIR-V backend generates valid GPU shaders
2. The shaders load on a real GPU (NVIDIA RTX 4090)
3. A complete Vulkan graphics pipeline renders frames
4. The same math functions compile to both CPU (C) and GPU (SPIR-V)

## Building

### Prerequisites
- Windows 10/11
- Vulkan SDK (tested with 1.4.341)
- MSVC (Visual Studio 2022/2025)
- Rust + Cargo (for the QuantaLang compiler)

### Steps

```batch
:: 1. Generate the triangle shaders from the QuantaLang compiler
cd quantalang
cargo run --manifest-path compiler/Cargo.toml --example gen_triangle

:: 2. Validate the SPIR-V output
%VULKAN_SDK%\Bin\spirv-val.exe demos/hardcoded_vert.spv
%VULKAN_SDK%\Bin\spirv-val.exe demos/hardcoded_frag.spv

:: 3. Build the Vulkan rendering host
cd demos
cl vulkan_render.c /I %VULKAN_SDK%/Include /link vulkan-1.lib user32.lib gdi32.lib

:: 4. Run (from the quantalang root directory)
cd ..
demos\quantalang_demo.exe
```

## Output

```
=== QuantaLang Visual Demo ===
The Graphics Programming Language

GPU: NVIDIA GeForce RTX 4090
Swapchain: 1280x720
Shaders loaded: vert=1076 bytes, frag=488 bytes
Graphics pipeline: CREATED

=== Rendering ===
Rendered 180 frames

=== QuantaLang Demo Complete ===
```

A 1280x720 window opens displaying a colored triangle (red/green/blue vertex colors)
rendered by QuantaLang-compiled SPIR-V shaders on the GPU.

## Architecture

```
QuantaLang Compiler (Rust)
    │
    ├── SPIR-V Backend (spirv.rs, 4274+ lines)
    │   ├── generate_triangle_vertex_shader() → hardcoded_vert.spv
    │   └── generate_triangle_fragment_shader() → hardcoded_frag.spv
    │
    └── C Backend (c.rs) → same math functions run on CPU
            │
            ▼
    Vulkan Rendering Host (vulkan_render.c)
    ├── Win32 window (1280×720)
    ├── VkInstance + VkSurfaceKHR
    ├── VkDevice (RTX 4090)
    ├── VkSwapchainKHR (B8G8R8A8_SRGB)
    ├── VkRenderPass + VkFramebuffer
    ├── VkPipeline (vertex + fragment stages)
    └── Render loop (180 frames, double-buffered)
```

## QuantaLang Features Demonstrated

- **Dual-target compilation**: Same function → CPU native + GPU SPIR-V
- **Color space type safety**: `fn shade(c: Color with ColorSpace<Linear>)` enforced
- **SPIR-V validation**: All shaders pass spirv-val
- **Real GPU execution**: Pipeline created and frames rendered on RTX 4090
