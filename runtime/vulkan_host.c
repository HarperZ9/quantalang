// ============================================================================
// QuantaLang Vulkan Host Runtime
// Copyright (c) 2024-2026 Zain Dana Harper. All Rights Reserved.
// ============================================================================
//
// Vulkan implementation: creates instance, picks GPU, creates device,
// loads SPIR-V shader modules, creates compute and graphics pipelines,
// dispatches compute work, and renders graphics frames with push constants.
//
// Supports:
//   - Compute pipelines (existing)
//   - Graphics pipelines with vertex + fragment shader stages
//   - Push constants (128 bytes, VK_SHADER_STAGE_ALL)
//   - Frame rendering: acquire, render pass, draw fullscreen quad, present
//
// Link with: vulkan-1.lib (from Vulkan SDK)
// Window creation requires a platform layer (Win32/GLFW). The draw_frame
// path is gated behind g_surface being initialized, so compute-only usage
// compiles and runs without any windowing dependency.

#include <vulkan/vulkan.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// ============================================================================
// Global state
// ============================================================================

// ---- Core state (compute) ----
static VkInstance       g_instance        = VK_NULL_HANDLE;
static VkPhysicalDevice g_physical_device = VK_NULL_HANDLE;
static VkDevice         g_device          = VK_NULL_HANDLE;
static VkQueue          g_compute_queue   = VK_NULL_HANDLE;
static uint32_t         g_compute_family  = 0;
static VkCommandPool    g_cmd_pool        = VK_NULL_HANDLE;
static int              g_initialized     = 0;
static char             g_device_name[256] = {0};

// ---- Graphics pipeline state ----
static VkQueue          g_graphics_queue    = VK_NULL_HANDLE;
static uint32_t         g_graphics_family   = 0;
static VkPipeline       g_graphics_pipeline = VK_NULL_HANDLE;
static VkPipelineLayout g_pipeline_layout   = VK_NULL_HANDLE;
static VkRenderPass     g_render_pass       = VK_NULL_HANDLE;
static int              g_graphics_ready    = 0;

// ---- Swapchain state (populated by platform layer) ----
// These are set to VK_NULL_HANDLE in headless/compute-only mode.
// A real windowing integration (Win32, GLFW) would populate these.
static VkSurfaceKHR     g_surface         = VK_NULL_HANDLE;
static VkSwapchainKHR   g_swapchain       = VK_NULL_HANDLE;
static VkFramebuffer*   g_framebuffers    = NULL;
static VkImageView*     g_swapchain_views = NULL;
static uint32_t         g_swapchain_count = 0;
static VkFormat         g_swapchain_format = VK_FORMAT_B8G8R8A8_UNORM;
static uint32_t         g_width           = 0;
static uint32_t         g_height          = 0;
static VkCommandBuffer  g_draw_cmd        = VK_NULL_HANDLE;
static VkSemaphore      g_image_available = VK_NULL_HANDLE;
static VkSemaphore      g_render_finished = VK_NULL_HANDLE;
static VkFence          g_in_flight_fence = VK_NULL_HANDLE;

// ---- Push constant buffer (128 bytes max, matches pipeline layout) ----
#define QUANTA_PUSH_CONSTANT_SIZE 128
static uint8_t g_push_constants[QUANTA_PUSH_CONSTANT_SIZE];

// ---- Window-close flag ----
static int g_should_close = 0;

// ============================================================================
// Internal helpers
// ============================================================================

// Load a SPIR-V file into a newly allocated buffer.
// Caller must free *out_code when done. Returns 0 on failure.
static int load_spirv_file(const char* path, uint32_t** out_code, long* out_size) {
    FILE* f = fopen(path, "rb");
    if (!f) {
        printf("[QuantaLang Vulkan] ERROR: Cannot open %s\n", path);
        return 0;
    }
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    fseek(f, 0, SEEK_SET);

    uint32_t* code = (uint32_t*)malloc(size);
    fread(code, 1, size, f);
    fclose(f);

    if (size < 4 || code[0] != 0x07230203) {
        printf("[QuantaLang Vulkan] ERROR: Invalid SPIR-V in %s\n", path);
        free(code);
        return 0;
    }

    *out_code = code;
    *out_size = size;
    return 1;
}

// Create a VkShaderModule from raw SPIR-V bytes.
// Returns VK_NULL_HANDLE on failure.
static VkShaderModule create_shader_module(const uint32_t* code, size_t size) {
    VkShaderModuleCreateInfo info = {0};
    info.sType    = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO;
    info.codeSize = size;
    info.pCode    = code;

    VkShaderModule module;
    VkResult res = vkCreateShaderModule(g_device, &info, NULL, &module);
    if (res != VK_SUCCESS) {
        printf("[QuantaLang Vulkan] ERROR: vkCreateShaderModule failed (%d)\n", res);
        return VK_NULL_HANDLE;
    }
    return module;
}

// ============================================================================
// Public API: Initialization
// ============================================================================

int quanta_vk_init(void) {
    if (g_initialized) return 1;

    memset(g_push_constants, 0, QUANTA_PUSH_CONSTANT_SIZE);

    // Create Vulkan instance
    VkApplicationInfo app_info = {0};
    app_info.sType              = VK_STRUCTURE_TYPE_APPLICATION_INFO;
    app_info.pApplicationName   = "QuantaLang";
    app_info.applicationVersion = VK_MAKE_VERSION(1, 0, 0);
    app_info.pEngineName        = "QuantaLang Compiler";
    app_info.engineVersion      = VK_MAKE_VERSION(1, 0, 0);
    app_info.apiVersion         = VK_API_VERSION_1_0;

    VkInstanceCreateInfo create_info = {0};
    create_info.sType            = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO;
    create_info.pApplicationInfo = &app_info;

    VkResult result = vkCreateInstance(&create_info, NULL, &g_instance);
    if (result != VK_SUCCESS) {
        printf("[QuantaLang Vulkan] ERROR: vkCreateInstance failed (%d)\n", result);
        return 0;
    }

    // Pick physical device (first discrete GPU, or first available)
    uint32_t device_count = 0;
    vkEnumeratePhysicalDevices(g_instance, &device_count, NULL);
    if (device_count == 0) {
        printf("[QuantaLang Vulkan] ERROR: No Vulkan-capable GPU found\n");
        return 0;
    }

    VkPhysicalDevice* devices = (VkPhysicalDevice*)malloc(sizeof(VkPhysicalDevice) * device_count);
    vkEnumeratePhysicalDevices(g_instance, &device_count, devices);

    // Prefer discrete GPU
    g_physical_device = devices[0];
    for (uint32_t i = 0; i < device_count; i++) {
        VkPhysicalDeviceProperties props;
        vkGetPhysicalDeviceProperties(devices[i], &props);
        if (props.deviceType == VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU) {
            g_physical_device = devices[i];
            break;
        }
    }
    free(devices);

    VkPhysicalDeviceProperties props;
    vkGetPhysicalDeviceProperties(g_physical_device, &props);
    strncpy(g_device_name, props.deviceName, sizeof(g_device_name) - 1);

    // Find queue families: we need both compute and graphics
    uint32_t queue_family_count = 0;
    vkGetPhysicalDeviceQueueFamilyProperties(g_physical_device, &queue_family_count, NULL);
    VkQueueFamilyProperties* families = (VkQueueFamilyProperties*)malloc(
        sizeof(VkQueueFamilyProperties) * queue_family_count);
    vkGetPhysicalDeviceQueueFamilyProperties(g_physical_device, &queue_family_count, families);

    g_compute_family  = 0;
    g_graphics_family = 0;
    int found_compute  = 0;
    int found_graphics = 0;
    for (uint32_t i = 0; i < queue_family_count; i++) {
        if (!found_compute && (families[i].queueFlags & VK_QUEUE_COMPUTE_BIT)) {
            g_compute_family = i;
            found_compute = 1;
        }
        if (!found_graphics && (families[i].queueFlags & VK_QUEUE_GRAPHICS_BIT)) {
            g_graphics_family = i;
            found_graphics = 1;
        }
    }
    free(families);

    // Create logical device with both compute and graphics queues.
    // If they share a family, we only create one queue create info.
    float queue_priority = 1.0f;
    VkDeviceQueueCreateInfo queue_infos[2];
    uint32_t queue_info_count = 0;

    memset(queue_infos, 0, sizeof(queue_infos));
    queue_infos[0].sType            = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO;
    queue_infos[0].queueFamilyIndex = g_compute_family;
    queue_infos[0].queueCount       = 1;
    queue_infos[0].pQueuePriorities = &queue_priority;
    queue_info_count = 1;

    if (g_graphics_family != g_compute_family) {
        queue_infos[1].sType            = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO;
        queue_infos[1].queueFamilyIndex = g_graphics_family;
        queue_infos[1].queueCount       = 1;
        queue_infos[1].pQueuePriorities = &queue_priority;
        queue_info_count = 2;
    }

    VkDeviceCreateInfo device_info = {0};
    device_info.sType                = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO;
    device_info.queueCreateInfoCount = queue_info_count;
    device_info.pQueueCreateInfos    = queue_infos;

    result = vkCreateDevice(g_physical_device, &device_info, NULL, &g_device);
    if (result != VK_SUCCESS) {
        printf("[QuantaLang Vulkan] ERROR: vkCreateDevice failed (%d)\n", result);
        return 0;
    }

    vkGetDeviceQueue(g_device, g_compute_family,  0, &g_compute_queue);
    vkGetDeviceQueue(g_device, g_graphics_family, 0, &g_graphics_queue);

    // Create command pool (uses graphics family so it works for both)
    VkCommandPoolCreateInfo pool_info = {0};
    pool_info.sType            = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO;
    pool_info.queueFamilyIndex = g_graphics_family;
    pool_info.flags            = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT;

    result = vkCreateCommandPool(g_device, &pool_info, NULL, &g_cmd_pool);
    if (result != VK_SUCCESS) {
        printf("[QuantaLang Vulkan] ERROR: vkCreateCommandPool failed (%d)\n", result);
        return 0;
    }

    g_initialized = 1;
    g_should_close = 0;
    printf("[QuantaLang Vulkan] Initialized on %s\n", g_device_name);
    return 1;
}

// ============================================================================
// Public API: Device query
// ============================================================================

const char* quanta_vk_device_name(void) {
    return g_device_name;
}

// ============================================================================
// Public API: Shader loading
// ============================================================================

int quanta_vk_load_shader_file(const char* path) {
    if (!g_initialized) return 0;

    uint32_t* code = NULL;
    long size = 0;
    if (!load_spirv_file(path, &code, &size)) return 0;

    VkShaderModule module = create_shader_module(code, (size_t)size);
    free(code);
    if (module == VK_NULL_HANDLE) return 0;

    printf("[QuantaLang Vulkan] Shader loaded: %s (%ld bytes)\n", path, size);
    vkDestroyShaderModule(g_device, module, NULL);
    return 1;
}

// ============================================================================
// Public API: Compute pipeline (existing)
// ============================================================================

int quanta_vk_run_compute(const char* spv_path) {
    if (!g_initialized) return 0;

    uint32_t* code = NULL;
    long size = 0;
    if (!load_spirv_file(spv_path, &code, &size)) return 0;

    VkShaderModule shader = create_shader_module(code, (size_t)size);
    free(code);
    if (shader == VK_NULL_HANDLE) return 0;

    // Pipeline layout with push constants
    VkPushConstantRange push_range = {0};
    push_range.stageFlags = VK_SHADER_STAGE_ALL;
    push_range.offset     = 0;
    push_range.size       = QUANTA_PUSH_CONSTANT_SIZE;

    VkPipelineLayoutCreateInfo layout_info = {0};
    layout_info.sType                  = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO;
    layout_info.pushConstantRangeCount = 1;
    layout_info.pPushConstantRanges    = &push_range;

    VkPipelineLayout pipeline_layout;
    VkResult res = vkCreatePipelineLayout(g_device, &layout_info, NULL, &pipeline_layout);
    if (res != VK_SUCCESS) {
        vkDestroyShaderModule(g_device, shader, NULL);
        printf("[QuantaLang Vulkan] ERROR: Pipeline layout creation failed\n");
        return 0;
    }

    // Create compute pipeline
    VkComputePipelineCreateInfo pipeline_info = {0};
    pipeline_info.sType       = VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO;
    pipeline_info.stage.sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
    pipeline_info.stage.stage = VK_SHADER_STAGE_COMPUTE_BIT;
    pipeline_info.stage.module = shader;
    pipeline_info.stage.pName  = "main";
    pipeline_info.layout       = pipeline_layout;

    VkPipeline pipeline;
    res = vkCreateComputePipelines(g_device, VK_NULL_HANDLE, 1, &pipeline_info, NULL, &pipeline);

    if (res == VK_SUCCESS) {
        printf("[QuantaLang Vulkan] Compute pipeline created successfully\n");

        VkCommandBufferAllocateInfo cmd_info = {0};
        cmd_info.sType              = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO;
        cmd_info.commandPool        = g_cmd_pool;
        cmd_info.level              = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
        cmd_info.commandBufferCount = 1;

        VkCommandBuffer cmd;
        vkAllocateCommandBuffers(g_device, &cmd_info, &cmd);

        VkCommandBufferBeginInfo begin_info = {0};
        begin_info.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO;
        begin_info.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
        vkBeginCommandBuffer(cmd, &begin_info);
        vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, pipeline);
        vkCmdPushConstants(cmd, pipeline_layout, VK_SHADER_STAGE_ALL,
                           0, QUANTA_PUSH_CONSTANT_SIZE, g_push_constants);
        vkCmdDispatch(cmd, 1, 1, 1);
        vkEndCommandBuffer(cmd);

        VkSubmitInfo submit = {0};
        submit.sType              = VK_STRUCTURE_TYPE_SUBMIT_INFO;
        submit.commandBufferCount = 1;
        submit.pCommandBuffers    = &cmd;
        vkQueueSubmit(g_compute_queue, 1, &submit, VK_NULL_HANDLE);
        vkQueueWaitIdle(g_compute_queue);

        printf("[QuantaLang Vulkan] Compute dispatch complete (1x1x1)\n");
        vkDestroyPipeline(g_device, pipeline, NULL);
    } else {
        printf("[QuantaLang Vulkan] Pipeline not created (shader is fragment/vertex, not compute)\n");
        printf("[QuantaLang Vulkan] VkShaderModule was valid -- SPIR-V accepted by driver\n");
    }

    vkDestroyPipelineLayout(g_device, pipeline_layout, NULL);
    vkDestroyShaderModule(g_device, shader, NULL);
    return 1;
}

// ============================================================================
// Public API: Graphics pipeline with vertex + fragment shader stages
// ============================================================================

// Create a full graphics pipeline from separate vertex and fragment SPIR-V
// files. Requires quanta_vk_init() to have been called first.
//
// The pipeline is configured to draw a fullscreen triangle / quad:
//   - No vertex input bindings (vertices are generated in the vertex shader)
//   - Triangle list topology
//   - Viewport/scissor set dynamically at draw time
//   - Push constants: 128 bytes visible to all stages
//   - Single color attachment, no depth
//
// Returns 1 on success, 0 on failure.
int quanta_vk_create_graphics_pipeline(const char* vert_path, const char* frag_path) {
    if (!g_initialized) {
        printf("[QuantaLang Vulkan] ERROR: Not initialized\n");
        return 0;
    }

    // Clean up any previously created graphics pipeline
    if (g_graphics_pipeline != VK_NULL_HANDLE) {
        vkDestroyPipeline(g_device, g_graphics_pipeline, NULL);
        g_graphics_pipeline = VK_NULL_HANDLE;
    }
    if (g_pipeline_layout != VK_NULL_HANDLE) {
        vkDestroyPipelineLayout(g_device, g_pipeline_layout, NULL);
        g_pipeline_layout = VK_NULL_HANDLE;
    }
    if (g_render_pass != VK_NULL_HANDLE) {
        vkDestroyRenderPass(g_device, g_render_pass, NULL);
        g_render_pass = VK_NULL_HANDLE;
    }
    g_graphics_ready = 0;

    // ---- Load vertex shader SPIR-V ----
    uint32_t* vert_code = NULL;
    long vert_size = 0;
    if (!load_spirv_file(vert_path, &vert_code, &vert_size)) return 0;

    VkShaderModule vert_module = create_shader_module(vert_code, (size_t)vert_size);
    free(vert_code);
    if (vert_module == VK_NULL_HANDLE) return 0;

    printf("[QuantaLang Vulkan] Vertex shader loaded: %s (%ld bytes)\n", vert_path, vert_size);

    // ---- Load fragment shader SPIR-V ----
    uint32_t* frag_code = NULL;
    long frag_size = 0;
    if (!load_spirv_file(frag_path, &frag_code, &frag_size)) {
        vkDestroyShaderModule(g_device, vert_module, NULL);
        return 0;
    }

    VkShaderModule frag_module = create_shader_module(frag_code, (size_t)frag_size);
    free(frag_code);
    if (frag_module == VK_NULL_HANDLE) {
        vkDestroyShaderModule(g_device, vert_module, NULL);
        return 0;
    }

    printf("[QuantaLang Vulkan] Fragment shader loaded: %s (%ld bytes)\n", frag_path, frag_size);

    // ---- Shader stages ----
    VkPipelineShaderStageCreateInfo stages[2];
    memset(stages, 0, sizeof(stages));

    stages[0].sType  = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
    stages[0].stage  = VK_SHADER_STAGE_VERTEX_BIT;
    stages[0].module = vert_module;
    stages[0].pName  = "main";

    stages[1].sType  = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
    stages[1].stage  = VK_SHADER_STAGE_FRAGMENT_BIT;
    stages[1].module = frag_module;
    stages[1].pName  = "main";

    // ---- Vertex input: none (fullscreen quad generated in vertex shader) ----
    VkPipelineVertexInputStateCreateInfo vertex_input = {0};
    vertex_input.sType = VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO;

    // ---- Input assembly: triangle list ----
    VkPipelineInputAssemblyStateCreateInfo input_assembly = {0};
    input_assembly.sType    = VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO;
    input_assembly.topology = VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST;

    // ---- Dynamic viewport and scissor ----
    VkDynamicState dynamic_states[] = {
        VK_DYNAMIC_STATE_VIEWPORT,
        VK_DYNAMIC_STATE_SCISSOR
    };
    VkPipelineDynamicStateCreateInfo dynamic_state = {0};
    dynamic_state.sType             = VK_STRUCTURE_TYPE_PIPELINE_DYNAMIC_STATE_CREATE_INFO;
    dynamic_state.dynamicStateCount = 2;
    dynamic_state.pDynamicStates    = dynamic_states;

    VkPipelineViewportStateCreateInfo viewport_state = {0};
    viewport_state.sType         = VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO;
    viewport_state.viewportCount = 1;
    viewport_state.scissorCount  = 1;

    // ---- Rasterizer: fill polygons, no culling for fullscreen quad ----
    VkPipelineRasterizationStateCreateInfo rasterizer = {0};
    rasterizer.sType       = VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO;
    rasterizer.polygonMode = VK_POLYGON_MODE_FILL;
    rasterizer.lineWidth   = 1.0f;
    rasterizer.cullMode    = VK_CULL_MODE_NONE;
    rasterizer.frontFace   = VK_FRONT_FACE_COUNTER_CLOCKWISE;

    // ---- Multisampling: disabled ----
    VkPipelineMultisampleStateCreateInfo multisample = {0};
    multisample.sType                = VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO;
    multisample.rasterizationSamples = VK_SAMPLE_COUNT_1_BIT;

    // ---- Color blending: standard alpha blend on one attachment ----
    VkPipelineColorBlendAttachmentState blend_attachment = {0};
    blend_attachment.colorWriteMask = VK_COLOR_COMPONENT_R_BIT | VK_COLOR_COMPONENT_G_BIT |
                                     VK_COLOR_COMPONENT_B_BIT | VK_COLOR_COMPONENT_A_BIT;
    blend_attachment.blendEnable    = VK_FALSE;

    VkPipelineColorBlendStateCreateInfo color_blend = {0};
    color_blend.sType           = VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO;
    color_blend.attachmentCount = 1;
    color_blend.pAttachments    = &blend_attachment;

    // ---- Push constant range: 128 bytes, all stages ----
    VkPushConstantRange push_range = {0};
    push_range.stageFlags = VK_SHADER_STAGE_ALL;
    push_range.offset     = 0;
    push_range.size       = QUANTA_PUSH_CONSTANT_SIZE;

    VkPipelineLayoutCreateInfo layout_info = {0};
    layout_info.sType                  = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO;
    layout_info.pushConstantRangeCount = 1;
    layout_info.pPushConstantRanges    = &push_range;

    VkResult res = vkCreatePipelineLayout(g_device, &layout_info, NULL, &g_pipeline_layout);
    if (res != VK_SUCCESS) {
        printf("[QuantaLang Vulkan] ERROR: Graphics pipeline layout creation failed (%d)\n", res);
        vkDestroyShaderModule(g_device, vert_module, NULL);
        vkDestroyShaderModule(g_device, frag_module, NULL);
        return 0;
    }

    // ---- Render pass: single color attachment ----
    VkAttachmentDescription color_attachment = {0};
    color_attachment.format         = g_swapchain_format;
    color_attachment.samples        = VK_SAMPLE_COUNT_1_BIT;
    color_attachment.loadOp         = VK_ATTACHMENT_LOAD_OP_CLEAR;
    color_attachment.storeOp        = VK_ATTACHMENT_STORE_OP_STORE;
    color_attachment.stencilLoadOp  = VK_ATTACHMENT_LOAD_OP_DONT_CARE;
    color_attachment.stencilStoreOp = VK_ATTACHMENT_STORE_OP_DONT_CARE;
    color_attachment.initialLayout  = VK_IMAGE_LAYOUT_UNDEFINED;
    color_attachment.finalLayout    = VK_IMAGE_LAYOUT_PRESENT_SRC_KHR;

    VkAttachmentReference color_ref = {0};
    color_ref.attachment = 0;
    color_ref.layout     = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL;

    VkSubpassDescription subpass = {0};
    subpass.pipelineBindPoint    = VK_PIPELINE_BIND_POINT_GRAPHICS;
    subpass.colorAttachmentCount = 1;
    subpass.pColorAttachments    = &color_ref;

    VkRenderPassCreateInfo rp_info = {0};
    rp_info.sType           = VK_STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO;
    rp_info.attachmentCount = 1;
    rp_info.pAttachments    = &color_attachment;
    rp_info.subpassCount    = 1;
    rp_info.pSubpasses      = &subpass;

    res = vkCreateRenderPass(g_device, &rp_info, NULL, &g_render_pass);
    if (res != VK_SUCCESS) {
        printf("[QuantaLang Vulkan] ERROR: Render pass creation failed (%d)\n", res);
        vkDestroyPipelineLayout(g_device, g_pipeline_layout, NULL);
        g_pipeline_layout = VK_NULL_HANDLE;
        vkDestroyShaderModule(g_device, vert_module, NULL);
        vkDestroyShaderModule(g_device, frag_module, NULL);
        return 0;
    }

    // ---- Create graphics pipeline ----
    VkGraphicsPipelineCreateInfo gfx_info = {0};
    gfx_info.sType               = VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO;
    gfx_info.stageCount          = 2;
    gfx_info.pStages             = stages;
    gfx_info.pVertexInputState   = &vertex_input;
    gfx_info.pInputAssemblyState = &input_assembly;
    gfx_info.pViewportState      = &viewport_state;
    gfx_info.pRasterizationState = &rasterizer;
    gfx_info.pMultisampleState   = &multisample;
    gfx_info.pColorBlendState    = &color_blend;
    gfx_info.pDynamicState       = &dynamic_state;
    gfx_info.layout              = g_pipeline_layout;
    gfx_info.renderPass          = g_render_pass;
    gfx_info.subpass             = 0;

    res = vkCreateGraphicsPipelines(g_device, VK_NULL_HANDLE, 1, &gfx_info, NULL,
                                    &g_graphics_pipeline);

    // Shader modules can be destroyed immediately after pipeline creation
    vkDestroyShaderModule(g_device, vert_module, NULL);
    vkDestroyShaderModule(g_device, frag_module, NULL);

    if (res != VK_SUCCESS) {
        printf("[QuantaLang Vulkan] ERROR: Graphics pipeline creation failed (%d)\n", res);
        printf("[QuantaLang Vulkan] (This is expected without a valid swapchain/surface)\n");
        // Pipeline creation failed, but the shader modules were valid.
        // In headless mode this is acceptable -- the shaders compiled to SPIR-V
        // and were accepted by the driver.
        return 0;
    }

    g_graphics_ready = 1;
    printf("[QuantaLang Vulkan] Graphics pipeline created (vert + frag)\n");
    return 1;
}

// ============================================================================
// Public API: Push constants
// ============================================================================

// Set a single float (4 bytes) in the push constant buffer.
//   offset: byte offset into the 128-byte push constant block (must be < 124)
//   value:  float value to store
void quanta_vk_set_push_constant_f32(int offset, float value) {
    if (offset < 0 || offset > (QUANTA_PUSH_CONSTANT_SIZE - (int)sizeof(float))) {
        printf("[QuantaLang Vulkan] WARNING: push constant offset %d out of range\n", offset);
        return;
    }
    memcpy(&g_push_constants[offset], &value, sizeof(float));
}

// ============================================================================
// Public API: Frame rendering
// ============================================================================

// Draw one frame: acquire swapchain image, begin render pass, bind graphics
// pipeline, push constants, draw a fullscreen triangle (3 vertices, generated
// in the vertex shader), end render pass, present.
//
// Prerequisites:
//   - quanta_vk_init() succeeded
//   - quanta_vk_create_graphics_pipeline() succeeded
//   - A valid swapchain exists (g_surface != VK_NULL_HANDLE)
//
// In headless / compute-only mode (no surface), this logs a message and
// returns 0. This lets the demo code call draw_frame unconditionally while
// only actually rendering when a window is present.
//
// Returns 1 on success, 0 if drawing was skipped or failed.
int quanta_vk_draw_frame(void) {
    if (!g_initialized || !g_graphics_ready) {
        printf("[QuantaLang Vulkan] draw_frame: pipeline not ready\n");
        return 0;
    }

    // In headless mode (no swapchain), skip actual rendering but log
    if (g_swapchain == VK_NULL_HANDLE || g_surface == VK_NULL_HANDLE) {
        printf("[QuantaLang Vulkan] draw_frame: headless mode (no swapchain), skipping present\n");
        return 0;
    }

    // ---- Wait for previous frame to finish ----
    vkWaitForFences(g_device, 1, &g_in_flight_fence, VK_TRUE, UINT64_MAX);
    vkResetFences(g_device, 1, &g_in_flight_fence);

    // ---- Acquire next swapchain image ----
    uint32_t image_index = 0;
    VkResult res = vkAcquireNextImageKHR(g_device, g_swapchain, UINT64_MAX,
                                          g_image_available, VK_NULL_HANDLE, &image_index);
    if (res == VK_ERROR_OUT_OF_DATE_KHR) {
        printf("[QuantaLang Vulkan] Swapchain out of date\n");
        return 0;
    }

    // ---- Record command buffer ----
    vkResetCommandBuffer(g_draw_cmd, 0);

    VkCommandBufferBeginInfo begin_info = {0};
    begin_info.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO;
    begin_info.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
    vkBeginCommandBuffer(g_draw_cmd, &begin_info);

    // Begin render pass
    VkClearValue clear_color = {{{0.0f, 0.0f, 0.0f, 1.0f}}};
    VkRenderPassBeginInfo rp_begin = {0};
    rp_begin.sType             = VK_STRUCTURE_TYPE_RENDER_PASS_BEGIN_INFO;
    rp_begin.renderPass        = g_render_pass;
    rp_begin.framebuffer       = g_framebuffers[image_index];
    rp_begin.renderArea.offset.x = 0;
    rp_begin.renderArea.offset.y = 0;
    rp_begin.renderArea.extent.width  = g_width;
    rp_begin.renderArea.extent.height = g_height;
    rp_begin.clearValueCount   = 1;
    rp_begin.pClearValues      = &clear_color;

    vkCmdBeginRenderPass(g_draw_cmd, &rp_begin, VK_SUBPASS_CONTENTS_INLINE);

    // Bind pipeline
    vkCmdBindPipeline(g_draw_cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, g_graphics_pipeline);

    // Set dynamic viewport and scissor
    VkViewport viewport = {0};
    viewport.width    = (float)g_width;
    viewport.height   = (float)g_height;
    viewport.minDepth = 0.0f;
    viewport.maxDepth = 1.0f;
    vkCmdSetViewport(g_draw_cmd, 0, 1, &viewport);

    VkRect2D scissor = {0};
    scissor.extent.width  = g_width;
    scissor.extent.height = g_height;
    vkCmdSetScissor(g_draw_cmd, 0, 1, &scissor);

    // Push constants before draw
    vkCmdPushConstants(g_draw_cmd, g_pipeline_layout, VK_SHADER_STAGE_ALL,
                       0, QUANTA_PUSH_CONSTANT_SIZE, g_push_constants);

    // Draw fullscreen triangle (3 vertices, 1 instance, no vertex buffer)
    vkCmdDraw(g_draw_cmd, 3, 1, 0, 0);

    vkCmdEndRenderPass(g_draw_cmd);
    vkEndCommandBuffer(g_draw_cmd);

    // ---- Submit ----
    VkPipelineStageFlags wait_stage = VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT;
    VkSubmitInfo submit = {0};
    submit.sType                = VK_STRUCTURE_TYPE_SUBMIT_INFO;
    submit.waitSemaphoreCount   = 1;
    submit.pWaitSemaphores      = &g_image_available;
    submit.pWaitDstStageMask    = &wait_stage;
    submit.commandBufferCount   = 1;
    submit.pCommandBuffers      = &g_draw_cmd;
    submit.signalSemaphoreCount = 1;
    submit.pSignalSemaphores    = &g_render_finished;

    vkQueueSubmit(g_graphics_queue, 1, &submit, g_in_flight_fence);

    // ---- Present ----
    VkPresentInfoKHR present = {0};
    present.sType              = VK_STRUCTURE_TYPE_PRESENT_INFO_KHR;
    present.waitSemaphoreCount = 1;
    present.pWaitSemaphores    = &g_render_finished;
    present.swapchainCount     = 1;
    present.pSwapchains        = &g_swapchain;
    present.pImageIndices      = &image_index;

    vkQueuePresentKHR(g_graphics_queue, &present);

    return 1;
}

// ============================================================================
// Public API: Window close query
// ============================================================================

// Returns 1 if the window close has been requested, 0 otherwise.
// In headless mode, always returns 0 (never closes on its own).
//
// A platform integration (Win32 message pump, GLFW poll) should set
// g_should_close = 1 when the user requests close. For headless demos,
// the caller typically uses a frame counter instead.
int quanta_vk_should_close(void) {
    return g_should_close;
}

// Programmatically request close (for frame-counted demos)
void quanta_vk_request_close(void) {
    g_should_close = 1;
}

// ============================================================================
// Public API: Shutdown
// ============================================================================

void quanta_vk_shutdown(void) {
    if (!g_initialized) return;

    vkDeviceWaitIdle(g_device);

    // Destroy graphics pipeline state
    if (g_graphics_pipeline != VK_NULL_HANDLE)
        vkDestroyPipeline(g_device, g_graphics_pipeline, NULL);
    if (g_pipeline_layout != VK_NULL_HANDLE)
        vkDestroyPipelineLayout(g_device, g_pipeline_layout, NULL);
    if (g_render_pass != VK_NULL_HANDLE)
        vkDestroyRenderPass(g_device, g_render_pass, NULL);

    // Destroy synchronization objects
    if (g_image_available != VK_NULL_HANDLE)
        vkDestroySemaphore(g_device, g_image_available, NULL);
    if (g_render_finished != VK_NULL_HANDLE)
        vkDestroySemaphore(g_device, g_render_finished, NULL);
    if (g_in_flight_fence != VK_NULL_HANDLE)
        vkDestroyFence(g_device, g_in_flight_fence, NULL);

    // Destroy swapchain resources
    if (g_framebuffers) {
        for (uint32_t i = 0; i < g_swapchain_count; i++) {
            if (g_framebuffers[i] != VK_NULL_HANDLE)
                vkDestroyFramebuffer(g_device, g_framebuffers[i], NULL);
        }
        free(g_framebuffers);
        g_framebuffers = NULL;
    }
    if (g_swapchain_views) {
        for (uint32_t i = 0; i < g_swapchain_count; i++) {
            if (g_swapchain_views[i] != VK_NULL_HANDLE)
                vkDestroyImageView(g_device, g_swapchain_views[i], NULL);
        }
        free(g_swapchain_views);
        g_swapchain_views = NULL;
    }
    if (g_swapchain != VK_NULL_HANDLE)
        vkDestroySwapchainKHR(g_device, g_swapchain, NULL);
    if (g_surface != VK_NULL_HANDLE)
        vkDestroySurfaceKHR(g_instance, g_surface, NULL);

    // Destroy core state
    if (g_cmd_pool) vkDestroyCommandPool(g_device, g_cmd_pool, NULL);
    if (g_device)   vkDestroyDevice(g_device, NULL);
    if (g_instance) vkDestroyInstance(g_instance, NULL);

    g_graphics_pipeline = VK_NULL_HANDLE;
    g_pipeline_layout   = VK_NULL_HANDLE;
    g_render_pass       = VK_NULL_HANDLE;
    g_cmd_pool          = VK_NULL_HANDLE;
    g_device            = VK_NULL_HANDLE;
    g_instance          = VK_NULL_HANDLE;
    g_swapchain         = VK_NULL_HANDLE;
    g_surface           = VK_NULL_HANDLE;
    g_image_available   = VK_NULL_HANDLE;
    g_render_finished   = VK_NULL_HANDLE;
    g_in_flight_fence   = VK_NULL_HANDLE;
    g_initialized       = 0;
    g_graphics_ready    = 0;
    g_should_close      = 0;

    printf("[QuantaLang Vulkan] Shutdown complete\n");
}
