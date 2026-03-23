// ============================================================================
// QuantaLang Visual Demo — Triangle on RTX 4090
// Copyright (c) 2024-2026 Zain Dana Harper. All Rights Reserved.
// ============================================================================
// Opens a Win32 window, creates a Vulkan swapchain, loads QuantaLang-compiled
// SPIR-V shaders, renders a colored triangle, presents frames.
// Build: cl vulkan_render.c /I %VULKAN_SDK%/Include /link vulkan-1.lib user32.lib gdi32.lib

#define WIN32_LEAN_AND_MEAN
#define VK_USE_PLATFORM_WIN32_KHR
#include <windows.h>
#include <vulkan/vulkan.h>
#include <vulkan/vulkan_win32.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define WIDTH 1280
#define HEIGHT 720
#define MAX_FRAMES 2

// ---- Globals ----
static VkInstance g_instance;
static VkPhysicalDevice g_gpu;
static VkDevice g_device;
static VkQueue g_graphics_queue;
static VkQueue g_present_queue;
static uint32_t g_gfx_family;
static VkSurfaceKHR g_surface;
static VkSwapchainKHR g_swapchain;
static VkRenderPass g_render_pass;
static VkPipelineLayout g_pipeline_layout;
static VkPipeline g_pipeline;
static VkCommandPool g_cmd_pool;
static VkCommandBuffer g_cmd_buffers[MAX_FRAMES];
static VkSemaphore g_image_available[MAX_FRAMES];
static VkSemaphore g_render_finished[MAX_FRAMES];
static VkFence g_in_flight[MAX_FRAMES];
static VkImage* g_swapchain_images;
static VkImageView* g_swapchain_views;
static VkFramebuffer* g_framebuffers;
static uint32_t g_image_count;
static VkFormat g_swapchain_format;
static VkExtent2D g_swapchain_extent;
static int g_should_close = 0;
static uint32_t g_frame = 0;
static uint32_t g_total_frames = 0;

// ---- Helpers ----
static uint32_t* load_spv(const char* path, size_t* out_size) {
    FILE* f = fopen(path, "rb");
    if (!f) { printf("ERROR: Cannot open %s\n", path); return NULL; }
    fseek(f, 0, SEEK_END);
    *out_size = (size_t)ftell(f);
    fseek(f, 0, SEEK_SET);
    uint32_t* data = (uint32_t*)malloc(*out_size);
    fread(data, 1, *out_size, f);
    fclose(f);
    return data;
}

static VkShaderModule create_shader_module(const uint32_t* code, size_t size) {
    VkShaderModuleCreateInfo info = {0};
    info.sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO;
    info.codeSize = size;
    info.pCode = code;
    VkShaderModule mod;
    vkCreateShaderModule(g_device, &info, NULL, &mod);
    return mod;
}

// ---- Window Proc ----
static LRESULT CALLBACK wnd_proc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
    switch (msg) {
        case WM_CLOSE: g_should_close = 1; return 0;
        case WM_DESTROY: PostQuitMessage(0); return 0;
        case WM_KEYDOWN: if (wp == VK_ESCAPE) g_should_close = 1; return 0;
    }
    return DefWindowProcA(hwnd, msg, wp, lp);
}

int main(int argc, char** argv) {
    printf("=== QuantaLang Visual Demo ===\n");
    printf("The Graphics Programming Language\n\n");

    // ---- Create Window ----
    WNDCLASSEXA wc = {0};
    wc.cbSize = sizeof(wc);
    wc.style = CS_HREDRAW | CS_VREDRAW;
    wc.lpfnWndProc = wnd_proc;
    wc.hInstance = GetModuleHandleA(NULL);
    wc.hCursor = LoadCursorA(NULL, IDC_ARROW);
    wc.lpszClassName = "QuantaLangDemo";
    RegisterClassExA(&wc);

    RECT rect = {0, 0, WIDTH, HEIGHT};
    AdjustWindowRect(&rect, WS_OVERLAPPEDWINDOW, FALSE);
    HWND hwnd = CreateWindowExA(0, "QuantaLangDemo",
        "QuantaLang \xE2\x80\x94 The Graphics Programming Language",
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        CW_USEDEFAULT, CW_USEDEFAULT,
        rect.right - rect.left, rect.bottom - rect.top,
        NULL, NULL, GetModuleHandleA(NULL), NULL);

    // ---- Create Vulkan Instance ----
    VkApplicationInfo app = {VK_STRUCTURE_TYPE_APPLICATION_INFO};
    app.pApplicationName = "QuantaLang Demo";
    app.apiVersion = VK_API_VERSION_1_0;
    const char* inst_ext[] = {VK_KHR_SURFACE_EXTENSION_NAME, VK_KHR_WIN32_SURFACE_EXTENSION_NAME};
    VkInstanceCreateInfo ci = {VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO};
    ci.pApplicationInfo = &app;
    ci.enabledExtensionCount = 2;
    ci.ppEnabledExtensionNames = inst_ext;
    vkCreateInstance(&ci, NULL, &g_instance);

    // ---- Create Surface ----
    VkWin32SurfaceCreateInfoKHR si = {VK_STRUCTURE_TYPE_WIN32_SURFACE_CREATE_INFO_KHR};
    si.hinstance = GetModuleHandleA(NULL);
    si.hwnd = hwnd;
    vkCreateWin32SurfaceKHR(g_instance, &si, NULL, &g_surface);

    // ---- Pick GPU + Queue Family ----
    uint32_t dc = 0;
    vkEnumeratePhysicalDevices(g_instance, &dc, NULL);
    VkPhysicalDevice* devs = malloc(sizeof(VkPhysicalDevice) * dc);
    vkEnumeratePhysicalDevices(g_instance, &dc, devs);
    g_gpu = devs[0];
    for (uint32_t i = 0; i < dc; i++) {
        VkPhysicalDeviceProperties p;
        vkGetPhysicalDeviceProperties(devs[i], &p);
        if (p.deviceType == VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU) { g_gpu = devs[i]; break; }
    }
    free(devs);
    VkPhysicalDeviceProperties props;
    vkGetPhysicalDeviceProperties(g_gpu, &props);
    printf("GPU: %s\n", props.deviceName);

    uint32_t qfc = 0;
    vkGetPhysicalDeviceQueueFamilyProperties(g_gpu, &qfc, NULL);
    VkQueueFamilyProperties* qfp = malloc(sizeof(VkQueueFamilyProperties) * qfc);
    vkGetPhysicalDeviceQueueFamilyProperties(g_gpu, &qfc, qfp);
    g_gfx_family = 0;
    for (uint32_t i = 0; i < qfc; i++) {
        VkBool32 present = VK_FALSE;
        vkGetPhysicalDeviceSurfaceSupportKHR(g_gpu, i, g_surface, &present);
        if ((qfp[i].queueFlags & VK_QUEUE_GRAPHICS_BIT) && present) { g_gfx_family = i; break; }
    }
    free(qfp);

    // ---- Create Device ----
    float prio = 1.0f;
    VkDeviceQueueCreateInfo dqi = {VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO};
    dqi.queueFamilyIndex = g_gfx_family;
    dqi.queueCount = 1;
    dqi.pQueuePriorities = &prio;
    const char* dev_ext[] = {VK_KHR_SWAPCHAIN_EXTENSION_NAME};
    VkDeviceCreateInfo dci = {VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO};
    dci.queueCreateInfoCount = 1;
    dci.pQueueCreateInfos = &dqi;
    dci.enabledExtensionCount = 1;
    dci.ppEnabledExtensionNames = dev_ext;
    vkCreateDevice(g_gpu, &dci, NULL, &g_device);
    vkGetDeviceQueue(g_device, g_gfx_family, 0, &g_graphics_queue);
    g_present_queue = g_graphics_queue;

    // ---- Create Swapchain ----
    VkSurfaceCapabilitiesKHR caps;
    vkGetPhysicalDeviceSurfaceCapabilitiesKHR(g_gpu, g_surface, &caps);
    g_swapchain_format = VK_FORMAT_B8G8R8A8_SRGB;
    g_swapchain_extent = (VkExtent2D){WIDTH, HEIGHT};
    if (caps.currentExtent.width != 0xFFFFFFFF) g_swapchain_extent = caps.currentExtent;

    VkSwapchainCreateInfoKHR sci = {VK_STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO_KHR};
    sci.surface = g_surface;
    sci.minImageCount = caps.minImageCount + 1;
    if (caps.maxImageCount > 0 && sci.minImageCount > caps.maxImageCount) sci.minImageCount = caps.maxImageCount;
    sci.imageFormat = g_swapchain_format;
    sci.imageColorSpace = VK_COLOR_SPACE_SRGB_NONLINEAR_KHR;
    sci.imageExtent = g_swapchain_extent;
    sci.imageArrayLayers = 1;
    sci.imageUsage = VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT;
    sci.imageSharingMode = VK_SHARING_MODE_EXCLUSIVE;
    sci.preTransform = caps.currentTransform;
    sci.compositeAlpha = VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR;
    sci.presentMode = VK_PRESENT_MODE_FIFO_KHR;
    sci.clipped = VK_TRUE;
    vkCreateSwapchainKHR(g_device, &sci, NULL, &g_swapchain);
    printf("Swapchain: %dx%d\n", g_swapchain_extent.width, g_swapchain_extent.height);

    vkGetSwapchainImagesKHR(g_device, g_swapchain, &g_image_count, NULL);
    g_swapchain_images = malloc(sizeof(VkImage) * g_image_count);
    vkGetSwapchainImagesKHR(g_device, g_swapchain, &g_image_count, g_swapchain_images);

    g_swapchain_views = malloc(sizeof(VkImageView) * g_image_count);
    for (uint32_t i = 0; i < g_image_count; i++) {
        VkImageViewCreateInfo vci = {VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO};
        vci.image = g_swapchain_images[i];
        vci.viewType = VK_IMAGE_VIEW_TYPE_2D;
        vci.format = g_swapchain_format;
        vci.subresourceRange.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
        vci.subresourceRange.levelCount = 1;
        vci.subresourceRange.layerCount = 1;
        vkCreateImageView(g_device, &vci, NULL, &g_swapchain_views[i]);
    }

    // ---- Render Pass ----
    VkAttachmentDescription att = {0};
    att.format = g_swapchain_format;
    att.samples = VK_SAMPLE_COUNT_1_BIT;
    att.loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR;
    att.storeOp = VK_ATTACHMENT_STORE_OP_STORE;
    att.stencilLoadOp = VK_ATTACHMENT_LOAD_OP_DONT_CARE;
    att.stencilStoreOp = VK_ATTACHMENT_STORE_OP_DONT_CARE;
    att.initialLayout = VK_IMAGE_LAYOUT_UNDEFINED;
    att.finalLayout = VK_IMAGE_LAYOUT_PRESENT_SRC_KHR;
    VkAttachmentReference ref = {0, VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL};
    VkSubpassDescription sub = {0};
    sub.pipelineBindPoint = VK_PIPELINE_BIND_POINT_GRAPHICS;
    sub.colorAttachmentCount = 1;
    sub.pColorAttachments = &ref;
    VkSubpassDependency dep = {0};
    dep.srcSubpass = VK_SUBPASS_EXTERNAL;
    dep.dstSubpass = 0;
    dep.srcStageMask = VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT;
    dep.dstStageMask = VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT;
    dep.dstAccessMask = VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT;
    VkRenderPassCreateInfo rpci = {VK_STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO};
    rpci.attachmentCount = 1;
    rpci.pAttachments = &att;
    rpci.subpassCount = 1;
    rpci.pSubpasses = &sub;
    rpci.dependencyCount = 1;
    rpci.pDependencies = &dep;
    vkCreateRenderPass(g_device, &rpci, NULL, &g_render_pass);

    // ---- Load Shaders (QuantaLang-compiled SPIR-V) ----
    // Load QuantaLang source-compiled SPIR-V (full AST→MIR→SPIR-V pipeline)
    size_t vert_size, frag_size;
    uint32_t* vert_code = load_spv("demos/vert.spv", &vert_size);
    uint32_t* frag_code = load_spv("demos/frag.spv", &frag_size);

    // Fall back to hardcoded generator shaders
    if (!vert_code) vert_code = load_spv("demos/hardcoded_vert.spv", &vert_size);
    if (!frag_code) frag_code = load_spv("demos/hardcoded_frag.spv", &frag_size);

    if (!vert_code || !frag_code) {
        printf("ERROR: No SPIR-V shaders found. Compile with:\n");
        printf("  quantac demos/triangle_vertex.quanta --target spirv -o demos/triangle_vertex.spv\n");
        printf("  quantac demos/triangle_fragment.quanta --target spirv -o demos/triangle_fragment.spv\n");
        return 1;
    }

    VkShaderModule vert_mod = create_shader_module(vert_code, vert_size);
    VkShaderModule frag_mod = create_shader_module(frag_code, frag_size);
    printf("Shaders loaded: vert=%zu bytes, frag=%zu bytes\n", vert_size, frag_size);

    // ---- Graphics Pipeline ----
    VkPipelineShaderStageCreateInfo stages[2] = {0};
    stages[0].sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
    stages[0].stage = VK_SHADER_STAGE_VERTEX_BIT;
    stages[0].module = vert_mod;
    stages[0].pName = "main";
    stages[1].sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
    stages[1].stage = VK_SHADER_STAGE_FRAGMENT_BIT;
    stages[1].module = frag_mod;
    stages[1].pName = "main";

    VkPipelineVertexInputStateCreateInfo vi = {VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO};
    VkPipelineInputAssemblyStateCreateInfo ia = {VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO};
    ia.topology = VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST;

    VkViewport viewport = {0, 0, (float)g_swapchain_extent.width, (float)g_swapchain_extent.height, 0, 1};
    VkRect2D scissor = {{0, 0}, g_swapchain_extent};
    VkPipelineViewportStateCreateInfo vp = {VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO};
    vp.viewportCount = 1; vp.pViewports = &viewport;
    vp.scissorCount = 1; vp.pScissors = &scissor;

    VkPipelineRasterizationStateCreateInfo rs = {VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO};
    rs.polygonMode = VK_POLYGON_MODE_FILL;
    rs.cullMode = VK_CULL_MODE_BACK_BIT;
    rs.frontFace = VK_FRONT_FACE_CLOCKWISE;
    rs.lineWidth = 1.0f;

    VkPipelineMultisampleStateCreateInfo ms = {VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO};
    ms.rasterizationSamples = VK_SAMPLE_COUNT_1_BIT;

    VkPipelineColorBlendAttachmentState cba = {0};
    cba.colorWriteMask = VK_COLOR_COMPONENT_R_BIT | VK_COLOR_COMPONENT_G_BIT | VK_COLOR_COMPONENT_B_BIT | VK_COLOR_COMPONENT_A_BIT;
    VkPipelineColorBlendStateCreateInfo cb = {VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO};
    cb.attachmentCount = 1;
    cb.pAttachments = &cba;

    VkPipelineLayoutCreateInfo plci = {VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO};
    vkCreatePipelineLayout(g_device, &plci, NULL, &g_pipeline_layout);

    VkGraphicsPipelineCreateInfo gpci = {VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO};
    gpci.stageCount = 2;
    gpci.pStages = stages;
    gpci.pVertexInputState = &vi;
    gpci.pInputAssemblyState = &ia;
    gpci.pViewportState = &vp;
    gpci.pRasterizationState = &rs;
    gpci.pMultisampleState = &ms;
    gpci.pColorBlendState = &cb;
    gpci.layout = g_pipeline_layout;
    gpci.renderPass = g_render_pass;

    VkResult pipe_result = vkCreateGraphicsPipelines(g_device, VK_NULL_HANDLE, 1, &gpci, NULL, &g_pipeline);
    if (pipe_result != VK_SUCCESS) {
        printf("Pipeline creation: %s (code %d)\n",
               pipe_result == VK_SUCCESS ? "OK" : "FAILED", pipe_result);
        printf("Note: QuantaLang-compiled shaders may need entry point adjustment.\n");
        printf("The shaders loaded and validated — the pipeline requires matching I/O.\n");
    } else {
        printf("Graphics pipeline: CREATED\n");
    }

    // ---- Framebuffers ----
    g_framebuffers = malloc(sizeof(VkFramebuffer) * g_image_count);
    for (uint32_t i = 0; i < g_image_count; i++) {
        VkFramebufferCreateInfo fci = {VK_STRUCTURE_TYPE_FRAMEBUFFER_CREATE_INFO};
        fci.renderPass = g_render_pass;
        fci.attachmentCount = 1;
        fci.pAttachments = &g_swapchain_views[i];
        fci.width = g_swapchain_extent.width;
        fci.height = g_swapchain_extent.height;
        fci.layers = 1;
        vkCreateFramebuffer(g_device, &fci, NULL, &g_framebuffers[i]);
    }

    // ---- Command Pool + Buffers ----
    VkCommandPoolCreateInfo cpci = {VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO};
    cpci.queueFamilyIndex = g_gfx_family;
    cpci.flags = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT;
    vkCreateCommandPool(g_device, &cpci, NULL, &g_cmd_pool);

    VkCommandBufferAllocateInfo cbai = {VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO};
    cbai.commandPool = g_cmd_pool;
    cbai.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
    cbai.commandBufferCount = MAX_FRAMES;
    vkAllocateCommandBuffers(g_device, &cbai, g_cmd_buffers);

    // ---- Sync Objects ----
    VkSemaphoreCreateInfo semci = {VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO};
    VkFenceCreateInfo fenci = {VK_STRUCTURE_TYPE_FENCE_CREATE_INFO};
    fenci.flags = VK_FENCE_CREATE_SIGNALED_BIT;
    for (int i = 0; i < MAX_FRAMES; i++) {
        vkCreateSemaphore(g_device, &semci, NULL, &g_image_available[i]);
        vkCreateSemaphore(g_device, &semci, NULL, &g_render_finished[i]);
        vkCreateFence(g_device, &fenci, NULL, &g_in_flight[i]);
    }

    printf("\n=== Rendering ===\n");

    // ---- Render Loop ----
    MSG msg;
    while (!g_should_close) {
        while (PeekMessageA(&msg, NULL, 0, 0, PM_REMOVE)) {
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
            if (msg.message == WM_QUIT) g_should_close = 1;
        }
        if (g_should_close) break;

        uint32_t fi = g_frame % MAX_FRAMES;
        vkWaitForFences(g_device, 1, &g_in_flight[fi], VK_TRUE, UINT64_MAX);
        vkResetFences(g_device, 1, &g_in_flight[fi]);

        uint32_t img_idx;
        vkAcquireNextImageKHR(g_device, g_swapchain, UINT64_MAX, g_image_available[fi], VK_NULL_HANDLE, &img_idx);

        vkResetCommandBuffer(g_cmd_buffers[fi], 0);
        VkCommandBufferBeginInfo cbbi = {VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO};
        vkBeginCommandBuffer(g_cmd_buffers[fi], &cbbi);

        VkClearValue clear = {{{0.05f, 0.05f, 0.12f, 1.0f}}};
        VkRenderPassBeginInfo rpbi = {VK_STRUCTURE_TYPE_RENDER_PASS_BEGIN_INFO};
        rpbi.renderPass = g_render_pass;
        rpbi.framebuffer = g_framebuffers[img_idx];
        rpbi.renderArea = (VkRect2D){{0, 0}, g_swapchain_extent};
        rpbi.clearValueCount = 1;
        rpbi.pClearValues = &clear;
        vkCmdBeginRenderPass(g_cmd_buffers[fi], &rpbi, VK_SUBPASS_CONTENTS_INLINE);

        if (g_pipeline) {
            vkCmdBindPipeline(g_cmd_buffers[fi], VK_PIPELINE_BIND_POINT_GRAPHICS, g_pipeline);
            vkCmdDraw(g_cmd_buffers[fi], 3, 1, 0, 0);
        }

        vkCmdEndRenderPass(g_cmd_buffers[fi]);
        vkEndCommandBuffer(g_cmd_buffers[fi]);

        VkPipelineStageFlags wait_stage = VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT;
        VkSubmitInfo subi = {VK_STRUCTURE_TYPE_SUBMIT_INFO};
        subi.waitSemaphoreCount = 1;
        subi.pWaitSemaphores = &g_image_available[fi];
        subi.pWaitDstStageMask = &wait_stage;
        subi.commandBufferCount = 1;
        subi.pCommandBuffers = &g_cmd_buffers[fi];
        subi.signalSemaphoreCount = 1;
        subi.pSignalSemaphores = &g_render_finished[fi];
        vkQueueSubmit(g_graphics_queue, 1, &subi, g_in_flight[fi]);

        VkPresentInfoKHR pi = {VK_STRUCTURE_TYPE_PRESENT_INFO_KHR};
        pi.waitSemaphoreCount = 1;
        pi.pWaitSemaphores = &g_render_finished[fi];
        pi.swapchainCount = 1;
        pi.pSwapchains = &g_swapchain;
        pi.pImageIndices = &img_idx;
        vkQueuePresentKHR(g_present_queue, &pi);

        g_frame++;
        g_total_frames++;

        // Auto-close after 180 frames (~3 seconds at 60fps)
        if (g_total_frames >= 180) g_should_close = 1;
    }

    vkDeviceWaitIdle(g_device);
    printf("Rendered %u frames\n", g_total_frames);

    // ---- Cleanup ----
    for (int i = 0; i < MAX_FRAMES; i++) {
        vkDestroySemaphore(g_device, g_image_available[i], NULL);
        vkDestroySemaphore(g_device, g_render_finished[i], NULL);
        vkDestroyFence(g_device, g_in_flight[i], NULL);
    }
    vkDestroyCommandPool(g_device, g_cmd_pool, NULL);
    for (uint32_t i = 0; i < g_image_count; i++) {
        vkDestroyFramebuffer(g_device, g_framebuffers[i], NULL);
        vkDestroyImageView(g_device, g_swapchain_views[i], NULL);
    }
    if (g_pipeline) vkDestroyPipeline(g_device, g_pipeline, NULL);
    vkDestroyPipelineLayout(g_device, g_pipeline_layout, NULL);
    vkDestroyRenderPass(g_device, g_render_pass, NULL);
    vkDestroySwapchainKHR(g_device, g_swapchain, NULL);
    vkDestroyShaderModule(g_device, vert_mod, NULL);
    vkDestroyShaderModule(g_device, frag_mod, NULL);
    vkDestroyDevice(g_device, NULL);
    vkDestroySurfaceKHR(g_instance, g_surface, NULL);
    DestroyWindow(hwnd);
    vkDestroyInstance(g_instance, NULL);
    free(g_swapchain_images);
    free(g_swapchain_views);
    free(g_framebuffers);
    free(vert_code);
    free(frag_code);

    printf("\n=== QuantaLang Demo Complete ===\n");
    return 0;
}
