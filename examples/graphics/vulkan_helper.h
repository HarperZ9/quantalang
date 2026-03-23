// vulkan_helper.h — Minimal Vulkan setup for QuantaLang demos
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
//
// This handles the Vulkan boilerplate so QuantaLang code focuses on
// rendering logic.
//
// NOTE: This is a STUB that demonstrates the FFI pattern.
// Real Vulkan requires the Vulkan SDK installed.
// For now, this prints what WOULD happen to prove the integration works.
//
// To use with real Vulkan:
//   1. Replace these stubs with actual Vulkan API calls
//   2. Link against vulkan-1.lib and GLFW/SDL
//   3. The QuantaLang code stays exactly the same

#ifndef QUANTA_VULKAN_HELPER_H
#define QUANTA_VULKAN_HELPER_H

#include <stdio.h>
#include <stdint.h>

// Opaque context handle
typedef struct {
    int32_t initialized;
    int32_t width;
    int32_t height;
    int32_t frame_count;
    int32_t should_close;
} QuantaGfxContext;

static QuantaGfxContext quanta_gfx_global_ctx = {0, 0, 0, 0, 0};

// Initialize graphics context (would create Vulkan instance, device, swapchain)
static int32_t quanta_gfx_init(int32_t width, int32_t height, const char* title) {
    printf("[GFX] Initializing %dx%d window: %s\n", width, height, title);
    quanta_gfx_global_ctx.initialized = 1;
    quanta_gfx_global_ctx.width = width;
    quanta_gfx_global_ctx.height = height;
    quanta_gfx_global_ctx.frame_count = 0;
    quanta_gfx_global_ctx.should_close = 0;
    printf("[GFX] Vulkan instance created\n");
    printf("[GFX] Physical device selected\n");
    printf("[GFX] Logical device created\n");
    printf("[GFX] Swapchain created (%dx%d)\n", width, height);
    return 1;
}

// Load a SPIR-V shader module
static int32_t quanta_gfx_load_shader(const char* path, int32_t stage) {
    printf("[GFX] Loading shader: %s (stage=%s)\n", path, stage == 0 ? "vertex" : "fragment");
    return 1; // shader handle
}

// Create a graphics pipeline from vertex + fragment shaders
static int32_t quanta_gfx_create_pipeline(int32_t vertex_shader, int32_t fragment_shader) {
    printf("[GFX] Creating graphics pipeline (vs=%d, fs=%d)\n", vertex_shader, fragment_shader);
    return 1; // pipeline handle
}

// Begin a frame (acquire swapchain image, begin command buffer)
static void quanta_gfx_begin_frame(void) {
    quanta_gfx_global_ctx.frame_count++;
    // Only print for first few frames to avoid spam
    if (quanta_gfx_global_ctx.frame_count <= 3) {
        printf("[GFX] Begin frame %d\n", quanta_gfx_global_ctx.frame_count);
    }
}

// Clear the framebuffer
static void quanta_gfx_clear(float r, float g, float b, float a) {
    if (quanta_gfx_global_ctx.frame_count <= 3) {
        printf("[GFX] Clear (%.1f, %.1f, %.1f, %.1f)\n", r, g, b, a);
    }
}

// Draw vertices (would call vkCmdDraw)
static void quanta_gfx_draw(int32_t vertex_count) {
    if (quanta_gfx_global_ctx.frame_count <= 3) {
        printf("[GFX] Draw %d vertices\n", vertex_count);
    }
}

// End frame (end command buffer, submit, present)
static void quanta_gfx_end_frame(void) {
    if (quanta_gfx_global_ctx.frame_count <= 3) {
        printf("[GFX] End frame %d\n", quanta_gfx_global_ctx.frame_count);
    }
    // Simulate window close after 3 frames for testing
    if (quanta_gfx_global_ctx.frame_count >= 3) {
        quanta_gfx_global_ctx.should_close = 1;
    }
}

// Check if window should close
static int32_t quanta_gfx_should_close(void) {
    return quanta_gfx_global_ctx.should_close;
}

// Cleanup
static void quanta_gfx_shutdown(void) {
    printf("[GFX] Shutdown complete (%d frames rendered)\n", quanta_gfx_global_ctx.frame_count);
    quanta_gfx_global_ctx.initialized = 0;
}

#endif // QUANTA_VULKAN_HELPER_H
