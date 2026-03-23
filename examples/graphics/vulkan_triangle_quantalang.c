/*
 * vulkan_triangle.c - QuantaLang Vulkan Triangle Renderer
 *
 * A standalone C program that renders a colored triangle using Vulkan + Win32.
 * No GLFW, no external dependencies beyond the Vulkan SDK and Windows API.
 *
 * Build:
 *   cl.exe /I"C:\VulkanSDK\1.4.341.1\Include" vulkan_triangle.c ^
 *       /link /LIBPATH:"C:\VulkanSDK\1.4.341.1\Lib" vulkan-1.lib user32.lib gdi32.lib
 */

#define VK_USE_PLATFORM_WIN32_KHR
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <vulkan/vulkan.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* --------------------------------------------------------------------------
 * Configuration
 * -------------------------------------------------------------------------- */
#define WINDOW_WIDTH   800
#define WINDOW_HEIGHT  600
#define WINDOW_TITLE   "QuantaLang - Vulkan Triangle"
#define MAX_FRAMES_IN_FLIGHT 2

/* --------------------------------------------------------------------------
 * Embedded SPIR-V shaders (compiled from triangle.vert / triangle.frag)
 * -------------------------------------------------------------------------- */
static const uint32_t vert_spv[] = {
    0x07230203, 0x00010000, 0x0008000b, 0x00000036, 0x00000000, 0x00020011,
    0x00000001, 0x0006000b, 0x00000001, 0x4c534c47, 0x6474732e, 0x3035342e,
    0x00000000, 0x0003000e, 0x00000000, 0x00000001, 0x0008000f, 0x00000000,
    0x00000004, 0x6e69616d, 0x00000000, 0x00000022, 0x00000026, 0x00000031,
    0x00030003, 0x00000002, 0x000001c2, 0x00040005, 0x00000004, 0x6e69616d,
    0x00000000, 0x00050005, 0x0000000c, 0x69736f70, 0x6e6f6974, 0x00000073,
    0x00040005, 0x00000017, 0x6f6c6f63, 0x00007372, 0x00060005, 0x00000020,
    0x505f6c67, 0x65567265, 0x78657472, 0x00000000, 0x00060006, 0x00000020,
    0x00000000, 0x505f6c67, 0x7469736f, 0x006e6f69, 0x00070006, 0x00000020,
    0x00000001, 0x505f6c67, 0x746e696f, 0x657a6953, 0x00000000, 0x00070006,
    0x00000020, 0x00000002, 0x435f6c67, 0x4470696c, 0x61747369, 0x0065636e,
    0x00070006, 0x00000020, 0x00000003, 0x435f6c67, 0x446c6c75, 0x61747369,
    0x0065636e, 0x00030005, 0x00000022, 0x00000000, 0x00060005, 0x00000026,
    0x565f6c67, 0x65747265, 0x646e4978, 0x00007865, 0x00050005, 0x00000031,
    0x67617266, 0x6f6c6f43, 0x00000072, 0x00030047, 0x00000020, 0x00000002,
    0x00050048, 0x00000020, 0x00000000, 0x0000000b, 0x00000000, 0x00050048,
    0x00000020, 0x00000001, 0x0000000b, 0x00000001, 0x00050048, 0x00000020,
    0x00000002, 0x0000000b, 0x00000003, 0x00050048, 0x00000020, 0x00000003,
    0x0000000b, 0x00000004, 0x00040047, 0x00000026, 0x0000000b, 0x0000002a,
    0x00040047, 0x00000031, 0x0000001e, 0x00000000, 0x00020013, 0x00000002,
    0x00030021, 0x00000003, 0x00000002, 0x00030016, 0x00000006, 0x00000020,
    0x00040017, 0x00000007, 0x00000006, 0x00000002, 0x00040015, 0x00000008,
    0x00000020, 0x00000000, 0x0004002b, 0x00000008, 0x00000009, 0x00000003,
    0x0004001c, 0x0000000a, 0x00000007, 0x00000009, 0x00040020, 0x0000000b,
    0x00000006, 0x0000000a, 0x0004003b, 0x0000000b, 0x0000000c, 0x00000006,
    0x0004002b, 0x00000006, 0x0000000d, 0x00000000, 0x0004002b, 0x00000006,
    0x0000000e, 0xbf000000, 0x0005002c, 0x00000007, 0x0000000f, 0x0000000d,
    0x0000000e, 0x0004002b, 0x00000006, 0x00000010, 0x3f000000, 0x0005002c,
    0x00000007, 0x00000011, 0x00000010, 0x00000010, 0x0005002c, 0x00000007,
    0x00000012, 0x0000000e, 0x00000010, 0x0006002c, 0x0000000a, 0x00000013,
    0x0000000f, 0x00000011, 0x00000012, 0x00040017, 0x00000014, 0x00000006,
    0x00000003, 0x0004001c, 0x00000015, 0x00000014, 0x00000009, 0x00040020,
    0x00000016, 0x00000006, 0x00000015, 0x0004003b, 0x00000016, 0x00000017,
    0x00000006, 0x0004002b, 0x00000006, 0x00000018, 0x3f800000, 0x0006002c,
    0x00000014, 0x00000019, 0x00000018, 0x0000000d, 0x0000000d, 0x0006002c,
    0x00000014, 0x0000001a, 0x0000000d, 0x00000018, 0x0000000d, 0x0006002c,
    0x00000014, 0x0000001b, 0x0000000d, 0x0000000d, 0x00000018, 0x0006002c,
    0x00000015, 0x0000001c, 0x00000019, 0x0000001a, 0x0000001b, 0x00040017,
    0x0000001d, 0x00000006, 0x00000004, 0x0004002b, 0x00000008, 0x0000001e,
    0x00000001, 0x0004001c, 0x0000001f, 0x00000006, 0x0000001e, 0x0006001e,
    0x00000020, 0x0000001d, 0x00000006, 0x0000001f, 0x0000001f, 0x00040020,
    0x00000021, 0x00000003, 0x00000020, 0x0004003b, 0x00000021, 0x00000022,
    0x00000003, 0x00040015, 0x00000023, 0x00000020, 0x00000001, 0x0004002b,
    0x00000023, 0x00000024, 0x00000000, 0x00040020, 0x00000025, 0x00000001,
    0x00000023, 0x0004003b, 0x00000025, 0x00000026, 0x00000001, 0x00040020,
    0x00000028, 0x00000006, 0x00000007, 0x00040020, 0x0000002e, 0x00000003,
    0x0000001d, 0x00040020, 0x00000030, 0x00000003, 0x00000014, 0x0004003b,
    0x00000030, 0x00000031, 0x00000003, 0x00040020, 0x00000033, 0x00000006,
    0x00000014, 0x00050036, 0x00000002, 0x00000004, 0x00000000, 0x00000003,
    0x000200f8, 0x00000005, 0x0003003e, 0x0000000c, 0x00000013, 0x0003003e,
    0x00000017, 0x0000001c, 0x0004003d, 0x00000023, 0x00000027, 0x00000026,
    0x00050041, 0x00000028, 0x00000029, 0x0000000c, 0x00000027, 0x0004003d,
    0x00000007, 0x0000002a, 0x00000029, 0x00050051, 0x00000006, 0x0000002b,
    0x0000002a, 0x00000000, 0x00050051, 0x00000006, 0x0000002c, 0x0000002a,
    0x00000001, 0x00070050, 0x0000001d, 0x0000002d, 0x0000002b, 0x0000002c,
    0x0000000d, 0x00000018, 0x00050041, 0x0000002e, 0x0000002f, 0x00000022,
    0x00000024, 0x0003003e, 0x0000002f, 0x0000002d, 0x0004003d, 0x00000023,
    0x00000032, 0x00000026, 0x00050041, 0x00000033, 0x00000034, 0x00000017,
    0x00000032, 0x0004003d, 0x00000014, 0x00000035, 0x00000034, 0x0003003e,
    0x00000031, 0x00000035, 0x000100fd, 0x00010038
};
static const size_t vert_spv_size = sizeof(vert_spv);

static const uint32_t frag_spv[] = {
    0x07230203, 0x00010000, 0x0008000b, 0x00000013, 0x00000000, 0x00020011,
    0x00000001, 0x0006000b, 0x00000001, 0x4c534c47, 0x6474732e, 0x3035342e,
    0x00000000, 0x0003000e, 0x00000000, 0x00000001, 0x0007000f, 0x00000004,
    0x00000004, 0x6e69616d, 0x00000000, 0x00000009, 0x0000000c, 0x00030010,
    0x00000004, 0x00000007, 0x00030003, 0x00000002, 0x000001c2, 0x00040005,
    0x00000004, 0x6e69616d, 0x00000000, 0x00050005, 0x00000009, 0x4374756f,
    0x726f6c6f, 0x00000000, 0x00050005, 0x0000000c, 0x67617266, 0x6f6c6f43,
    0x00000072, 0x00040047, 0x00000009, 0x0000001e, 0x00000000, 0x00040047,
    0x0000000c, 0x0000001e, 0x00000000, 0x00020013, 0x00000002, 0x00030021,
    0x00000003, 0x00000002, 0x00030016, 0x00000006, 0x00000020, 0x00040017,
    0x00000007, 0x00000006, 0x00000004, 0x00040020, 0x00000008, 0x00000003,
    0x00000007, 0x0004003b, 0x00000008, 0x00000009, 0x00000003, 0x00040017,
    0x0000000a, 0x00000006, 0x00000003, 0x00040020, 0x0000000b, 0x00000001,
    0x0000000a, 0x0004003b, 0x0000000b, 0x0000000c, 0x00000001, 0x0004002b,
    0x00000006, 0x0000000e, 0x3f800000, 0x00050036, 0x00000002, 0x00000004,
    0x00000000, 0x00000003, 0x000200f8, 0x00000005, 0x0004003d, 0x0000000a,
    0x0000000d, 0x0000000c, 0x00050051, 0x00000006, 0x0000000f, 0x0000000d,
    0x00000000, 0x00050051, 0x00000006, 0x00000010, 0x0000000d, 0x00000001,
    0x00050051, 0x00000006, 0x00000011, 0x0000000d, 0x00000002, 0x00070050,
    0x00000007, 0x00000012, 0x0000000f, 0x00000010, 0x00000011, 0x0000000e,
    0x0003003e, 0x00000009, 0x00000012, 0x000100fd, 0x00010038
};
static const size_t frag_spv_size = sizeof(frag_spv);

/* --------------------------------------------------------------------------
 * Globals
 * -------------------------------------------------------------------------- */
static HWND                  g_hwnd;
static HINSTANCE             g_hinstance;
static int                   g_running = 1;
static int                   g_framebuffer_resized = 0;

/* Vulkan handles */
static VkInstance             g_instance;
static VkSurfaceKHR          g_surface;
static VkPhysicalDevice      g_physical_device;
static VkDevice              g_device;
static VkQueue               g_graphics_queue;
static VkQueue               g_present_queue;
static uint32_t              g_graphics_family;
static uint32_t              g_present_family;

/* Swapchain */
static VkSwapchainKHR        g_swapchain;
static VkFormat              g_swapchain_format;
static VkExtent2D            g_swapchain_extent;
static uint32_t              g_swapchain_image_count;
static VkImage*              g_swapchain_images;
static VkImageView*          g_swapchain_image_views;
static VkFramebuffer*        g_swapchain_framebuffers;

/* Pipeline */
static VkRenderPass          g_render_pass;
static VkPipelineLayout      g_pipeline_layout;
static VkPipeline            g_pipeline;

/* Commands */
static VkCommandPool         g_command_pool;
static VkCommandBuffer       g_command_buffers[MAX_FRAMES_IN_FLIGHT];

/* Sync */
static VkSemaphore           g_image_available[MAX_FRAMES_IN_FLIGHT];
static VkSemaphore           g_render_finished[MAX_FRAMES_IN_FLIGHT];
static VkFence               g_in_flight[MAX_FRAMES_IN_FLIGHT];
static uint32_t              g_current_frame = 0;

/* --------------------------------------------------------------------------
 * Utility macros
 * -------------------------------------------------------------------------- */
#define VK_CHECK(call)                                                       \
    do {                                                                     \
        VkResult _r = (call);                                                \
        if (_r != VK_SUCCESS) {                                              \
            fprintf(stderr, "Vulkan error %d at %s:%d\n", _r, __FILE__,     \
                    __LINE__);                                               \
            exit(1);                                                         \
        }                                                                    \
    } while (0)

/* --------------------------------------------------------------------------
 * Win32 window
 * -------------------------------------------------------------------------- */
static LRESULT CALLBACK WndProc(HWND hwnd, UINT msg, WPARAM wParam,
                                LPARAM lParam) {
    switch (msg) {
    case WM_CLOSE:
    case WM_DESTROY:
        g_running = 0;
        PostQuitMessage(0);
        return 0;
    case WM_SIZE:
        g_framebuffer_resized = 1;
        return 0;
    case WM_KEYDOWN:
        if (wParam == VK_ESCAPE) {
            g_running = 0;
            PostQuitMessage(0);
        }
        return 0;
    }
    return DefWindowProcA(hwnd, msg, wParam, lParam);
}

static void create_window(void) {
    g_hinstance = GetModuleHandleA(NULL);

    WNDCLASSA wc;
    memset(&wc, 0, sizeof(wc));
    wc.style         = CS_HREDRAW | CS_VREDRAW;
    wc.lpfnWndProc   = WndProc;
    wc.hInstance      = g_hinstance;
    wc.hCursor        = LoadCursor(NULL, IDC_ARROW);
    wc.lpszClassName  = "QuantaLangVulkanClass";

    if (!RegisterClassA(&wc)) {
        fprintf(stderr, "Failed to register window class\n");
        exit(1);
    }

    /* Adjust so the client area is exactly WINDOW_WIDTH x WINDOW_HEIGHT */
    RECT rect = {0, 0, WINDOW_WIDTH, WINDOW_HEIGHT};
    AdjustWindowRect(&rect, WS_OVERLAPPEDWINDOW, FALSE);

    g_hwnd = CreateWindowExA(
        0, "QuantaLangVulkanClass", WINDOW_TITLE, WS_OVERLAPPEDWINDOW,
        CW_USEDEFAULT, CW_USEDEFAULT,
        rect.right - rect.left, rect.bottom - rect.top,
        NULL, NULL, g_hinstance, NULL);

    if (!g_hwnd) {
        fprintf(stderr, "Failed to create window\n");
        exit(1);
    }

    ShowWindow(g_hwnd, SW_SHOW);
    UpdateWindow(g_hwnd);
}

/* --------------------------------------------------------------------------
 * Vulkan instance + surface
 * -------------------------------------------------------------------------- */
static void create_instance(void) {
    VkApplicationInfo app_info;
    memset(&app_info, 0, sizeof(app_info));
    app_info.sType              = VK_STRUCTURE_TYPE_APPLICATION_INFO;
    app_info.pApplicationName   = "QuantaLang Triangle";
    app_info.applicationVersion = VK_MAKE_VERSION(1, 0, 0);
    app_info.pEngineName        = "QuantaLang";
    app_info.engineVersion      = VK_MAKE_VERSION(1, 0, 0);
    app_info.apiVersion         = VK_API_VERSION_1_0;

    const char* extensions[] = {
        VK_KHR_SURFACE_EXTENSION_NAME,
        VK_KHR_WIN32_SURFACE_EXTENSION_NAME
    };

    VkInstanceCreateInfo ci;
    memset(&ci, 0, sizeof(ci));
    ci.sType                   = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO;
    ci.pApplicationInfo        = &app_info;
    ci.enabledExtensionCount   = 2;
    ci.ppEnabledExtensionNames = extensions;

    VK_CHECK(vkCreateInstance(&ci, NULL, &g_instance));
    printf("[vulkan] Instance created\n");
}

static void create_surface(void) {
    VkWin32SurfaceCreateInfoKHR ci;
    memset(&ci, 0, sizeof(ci));
    ci.sType     = VK_STRUCTURE_TYPE_WIN32_SURFACE_CREATE_INFO_KHR;
    ci.hinstance = g_hinstance;
    ci.hwnd      = g_hwnd;

    VK_CHECK(vkCreateWin32SurfaceKHR(g_instance, &ci, NULL, &g_surface));
    printf("[vulkan] Win32 surface created\n");
}

/* --------------------------------------------------------------------------
 * Physical device selection + queue families
 * -------------------------------------------------------------------------- */
static int find_queue_families(VkPhysicalDevice pd, uint32_t* gfx,
                               uint32_t* pres) {
    uint32_t count = 0;
    vkGetPhysicalDeviceQueueFamilyProperties(pd, &count, NULL);
    if (count == 0) return 0;

    VkQueueFamilyProperties* props =
        (VkQueueFamilyProperties*)malloc(count * sizeof(*props));
    vkGetPhysicalDeviceQueueFamilyProperties(pd, &count, props);

    int found_gfx = 0, found_pres = 0;
    for (uint32_t i = 0; i < count; i++) {
        if (props[i].queueFlags & VK_QUEUE_GRAPHICS_BIT) {
            *gfx = i;
            found_gfx = 1;
        }
        VkBool32 present_support = VK_FALSE;
        vkGetPhysicalDeviceSurfaceSupportKHR(pd, i, g_surface,
                                              &present_support);
        if (present_support) {
            *pres = i;
            found_pres = 1;
        }
        if (found_gfx && found_pres) break;
    }
    free(props);
    return found_gfx && found_pres;
}

static void pick_physical_device(void) {
    uint32_t count = 0;
    vkEnumeratePhysicalDevices(g_instance, &count, NULL);
    if (count == 0) {
        fprintf(stderr, "No Vulkan-capable GPU found\n");
        exit(1);
    }

    VkPhysicalDevice* devices =
        (VkPhysicalDevice*)malloc(count * sizeof(*devices));
    vkEnumeratePhysicalDevices(g_instance, &count, devices);

    for (uint32_t i = 0; i < count; i++) {
        uint32_t gfx = 0, pres = 0;
        if (find_queue_families(devices[i], &gfx, &pres)) {
            /* Check swapchain extension support */
            uint32_t ext_count = 0;
            vkEnumerateDeviceExtensionProperties(devices[i], NULL, &ext_count,
                                                  NULL);
            VkExtensionProperties* exts =
                (VkExtensionProperties*)malloc(ext_count * sizeof(*exts));
            vkEnumerateDeviceExtensionProperties(devices[i], NULL, &ext_count,
                                                  exts);
            int has_swapchain = 0;
            for (uint32_t j = 0; j < ext_count; j++) {
                if (strcmp(exts[j].extensionName,
                           VK_KHR_SWAPCHAIN_EXTENSION_NAME) == 0) {
                    has_swapchain = 1;
                    break;
                }
            }
            free(exts);

            if (has_swapchain) {
                g_physical_device = devices[i];
                g_graphics_family = gfx;
                g_present_family  = pres;

                VkPhysicalDeviceProperties props;
                vkGetPhysicalDeviceProperties(devices[i], &props);
                printf("[vulkan] Using GPU: %s\n", props.deviceName);
                free(devices);
                return;
            }
        }
    }

    fprintf(stderr, "No suitable GPU found\n");
    free(devices);
    exit(1);
}

/* --------------------------------------------------------------------------
 * Logical device + queues
 * -------------------------------------------------------------------------- */
static void create_device(void) {
    float priority = 1.0f;

    /* We may need 1 or 2 queue create infos depending on whether graphics
       and present families differ. */
    uint32_t unique_families[2];
    uint32_t unique_count = 1;
    unique_families[0] = g_graphics_family;
    if (g_present_family != g_graphics_family) {
        unique_families[1] = g_present_family;
        unique_count = 2;
    }

    VkDeviceQueueCreateInfo queue_cis[2];
    memset(queue_cis, 0, sizeof(queue_cis));
    for (uint32_t i = 0; i < unique_count; i++) {
        queue_cis[i].sType            = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO;
        queue_cis[i].queueFamilyIndex = unique_families[i];
        queue_cis[i].queueCount       = 1;
        queue_cis[i].pQueuePriorities = &priority;
    }

    const char* dev_extensions[] = {VK_KHR_SWAPCHAIN_EXTENSION_NAME};

    VkPhysicalDeviceFeatures features;
    memset(&features, 0, sizeof(features));

    VkDeviceCreateInfo ci;
    memset(&ci, 0, sizeof(ci));
    ci.sType                   = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO;
    ci.queueCreateInfoCount    = unique_count;
    ci.pQueueCreateInfos       = queue_cis;
    ci.enabledExtensionCount   = 1;
    ci.ppEnabledExtensionNames = dev_extensions;
    ci.pEnabledFeatures        = &features;

    VK_CHECK(vkCreateDevice(g_physical_device, &ci, NULL, &g_device));

    vkGetDeviceQueue(g_device, g_graphics_family, 0, &g_graphics_queue);
    vkGetDeviceQueue(g_device, g_present_family, 0, &g_present_queue);
    printf("[vulkan] Logical device created\n");
}

/* --------------------------------------------------------------------------
 * Swapchain
 * -------------------------------------------------------------------------- */
static VkSurfaceFormatKHR choose_surface_format(void) {
    uint32_t count = 0;
    vkGetPhysicalDeviceSurfaceFormatsKHR(g_physical_device, g_surface, &count,
                                         NULL);
    VkSurfaceFormatKHR* formats =
        (VkSurfaceFormatKHR*)malloc(count * sizeof(*formats));
    vkGetPhysicalDeviceSurfaceFormatsKHR(g_physical_device, g_surface, &count,
                                         formats);

    VkSurfaceFormatKHR chosen = formats[0];
    for (uint32_t i = 0; i < count; i++) {
        if (formats[i].format == VK_FORMAT_B8G8R8A8_SRGB &&
            formats[i].colorSpace == VK_COLOR_SPACE_SRGB_NONLINEAR_KHR) {
            chosen = formats[i];
            break;
        }
        if (formats[i].format == VK_FORMAT_B8G8R8A8_UNORM) {
            chosen = formats[i];
            /* keep looking for SRGB */
        }
    }
    free(formats);
    return chosen;
}

static VkExtent2D choose_extent(const VkSurfaceCapabilitiesKHR* caps) {
    if (caps->currentExtent.width != 0xFFFFFFFF) {
        return caps->currentExtent;
    }

    RECT r;
    GetClientRect(g_hwnd, &r);
    VkExtent2D ext;
    ext.width  = (uint32_t)(r.right - r.left);
    ext.height = (uint32_t)(r.bottom - r.top);

    if (ext.width < caps->minImageExtent.width)
        ext.width = caps->minImageExtent.width;
    if (ext.width > caps->maxImageExtent.width)
        ext.width = caps->maxImageExtent.width;
    if (ext.height < caps->minImageExtent.height)
        ext.height = caps->minImageExtent.height;
    if (ext.height > caps->maxImageExtent.height)
        ext.height = caps->maxImageExtent.height;

    return ext;
}

static void create_swapchain(void) {
    VkSurfaceCapabilitiesKHR caps;
    vkGetPhysicalDeviceSurfaceCapabilitiesKHR(g_physical_device, g_surface,
                                               &caps);

    VkSurfaceFormatKHR fmt = choose_surface_format();
    VkExtent2D extent      = choose_extent(&caps);

    uint32_t image_count = caps.minImageCount + 1;
    if (caps.maxImageCount > 0 && image_count > caps.maxImageCount)
        image_count = caps.maxImageCount;

    VkSwapchainCreateInfoKHR ci;
    memset(&ci, 0, sizeof(ci));
    ci.sType            = VK_STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO_KHR;
    ci.surface          = g_surface;
    ci.minImageCount    = image_count;
    ci.imageFormat      = fmt.format;
    ci.imageColorSpace  = fmt.colorSpace;
    ci.imageExtent      = extent;
    ci.imageArrayLayers = 1;
    ci.imageUsage       = VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT;

    uint32_t families[] = {g_graphics_family, g_present_family};
    if (g_graphics_family != g_present_family) {
        ci.imageSharingMode      = VK_SHARING_MODE_CONCURRENT;
        ci.queueFamilyIndexCount = 2;
        ci.pQueueFamilyIndices   = families;
    } else {
        ci.imageSharingMode = VK_SHARING_MODE_EXCLUSIVE;
    }

    ci.preTransform   = caps.currentTransform;
    ci.compositeAlpha = VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR;
    ci.presentMode    = VK_PRESENT_MODE_FIFO_KHR;
    ci.clipped        = VK_TRUE;
    ci.oldSwapchain   = VK_NULL_HANDLE;

    VK_CHECK(vkCreateSwapchainKHR(g_device, &ci, NULL, &g_swapchain));

    g_swapchain_format = fmt.format;
    g_swapchain_extent = extent;

    /* Get images */
    vkGetSwapchainImagesKHR(g_device, g_swapchain, &g_swapchain_image_count,
                             NULL);
    g_swapchain_images =
        (VkImage*)malloc(g_swapchain_image_count * sizeof(VkImage));
    vkGetSwapchainImagesKHR(g_device, g_swapchain, &g_swapchain_image_count,
                             g_swapchain_images);

    /* Create image views */
    g_swapchain_image_views =
        (VkImageView*)malloc(g_swapchain_image_count * sizeof(VkImageView));

    for (uint32_t i = 0; i < g_swapchain_image_count; i++) {
        VkImageViewCreateInfo iv_ci;
        memset(&iv_ci, 0, sizeof(iv_ci));
        iv_ci.sType    = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO;
        iv_ci.image    = g_swapchain_images[i];
        iv_ci.viewType = VK_IMAGE_VIEW_TYPE_2D;
        iv_ci.format   = g_swapchain_format;
        iv_ci.components.r = VK_COMPONENT_SWIZZLE_IDENTITY;
        iv_ci.components.g = VK_COMPONENT_SWIZZLE_IDENTITY;
        iv_ci.components.b = VK_COMPONENT_SWIZZLE_IDENTITY;
        iv_ci.components.a = VK_COMPONENT_SWIZZLE_IDENTITY;
        iv_ci.subresourceRange.aspectMask     = VK_IMAGE_ASPECT_COLOR_BIT;
        iv_ci.subresourceRange.baseMipLevel   = 0;
        iv_ci.subresourceRange.levelCount     = 1;
        iv_ci.subresourceRange.baseArrayLayer = 0;
        iv_ci.subresourceRange.layerCount     = 1;

        VK_CHECK(vkCreateImageView(g_device, &iv_ci, NULL,
                                    &g_swapchain_image_views[i]));
    }

    printf("[vulkan] Swapchain created: %ux%u, %u images\n",
           extent.width, extent.height, g_swapchain_image_count);
}

/* --------------------------------------------------------------------------
 * Render pass
 * -------------------------------------------------------------------------- */
static void create_render_pass(void) {
    VkAttachmentDescription color_attach;
    memset(&color_attach, 0, sizeof(color_attach));
    color_attach.format         = g_swapchain_format;
    color_attach.samples        = VK_SAMPLE_COUNT_1_BIT;
    color_attach.loadOp         = VK_ATTACHMENT_LOAD_OP_CLEAR;
    color_attach.storeOp        = VK_ATTACHMENT_STORE_OP_STORE;
    color_attach.stencilLoadOp  = VK_ATTACHMENT_LOAD_OP_DONT_CARE;
    color_attach.stencilStoreOp = VK_ATTACHMENT_STORE_OP_DONT_CARE;
    color_attach.initialLayout  = VK_IMAGE_LAYOUT_UNDEFINED;
    color_attach.finalLayout    = VK_IMAGE_LAYOUT_PRESENT_SRC_KHR;

    VkAttachmentReference color_ref;
    color_ref.attachment = 0;
    color_ref.layout     = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL;

    VkSubpassDescription subpass;
    memset(&subpass, 0, sizeof(subpass));
    subpass.pipelineBindPoint    = VK_PIPELINE_BIND_POINT_GRAPHICS;
    subpass.colorAttachmentCount = 1;
    subpass.pColorAttachments    = &color_ref;

    VkSubpassDependency dep;
    memset(&dep, 0, sizeof(dep));
    dep.srcSubpass    = VK_SUBPASS_EXTERNAL;
    dep.dstSubpass    = 0;
    dep.srcStageMask  = VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT;
    dep.srcAccessMask = 0;
    dep.dstStageMask  = VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT;
    dep.dstAccessMask = VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT;

    VkRenderPassCreateInfo ci;
    memset(&ci, 0, sizeof(ci));
    ci.sType           = VK_STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO;
    ci.attachmentCount = 1;
    ci.pAttachments    = &color_attach;
    ci.subpassCount    = 1;
    ci.pSubpasses      = &subpass;
    ci.dependencyCount = 1;
    ci.pDependencies   = &dep;

    VK_CHECK(vkCreateRenderPass(g_device, &ci, NULL, &g_render_pass));
    printf("[vulkan] Render pass created\n");
}

/* --------------------------------------------------------------------------
 * Shader modules
 * -------------------------------------------------------------------------- */
static VkShaderModule create_shader_module(const uint32_t* code,
                                           size_t size) {
    VkShaderModuleCreateInfo ci;
    memset(&ci, 0, sizeof(ci));
    ci.sType    = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO;
    ci.codeSize = size;
    ci.pCode    = code;

    VkShaderModule mod;
    VK_CHECK(vkCreateShaderModule(g_device, &ci, NULL, &mod));
    return mod;
}

/* --------------------------------------------------------------------------
 * Graphics pipeline
 * -------------------------------------------------------------------------- */
static void create_pipeline(void) {
    VkShaderModule vert_mod = create_shader_module(vert_spv, vert_spv_size);
    VkShaderModule frag_mod = create_shader_module(frag_spv, frag_spv_size);

    VkPipelineShaderStageCreateInfo stages[2];
    memset(stages, 0, sizeof(stages));

    stages[0].sType  = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
    stages[0].stage  = VK_SHADER_STAGE_VERTEX_BIT;
    stages[0].module = vert_mod;
    stages[0].pName  = "main";

    stages[1].sType  = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
    stages[1].stage  = VK_SHADER_STAGE_FRAGMENT_BIT;
    stages[1].module = frag_mod;
    stages[1].pName  = "main";

    /* Vertex input: none (hardcoded in shader) */
    VkPipelineVertexInputStateCreateInfo vertex_input;
    memset(&vertex_input, 0, sizeof(vertex_input));
    vertex_input.sType = VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO;

    /* Input assembly */
    VkPipelineInputAssemblyStateCreateInfo input_asm;
    memset(&input_asm, 0, sizeof(input_asm));
    input_asm.sType    = VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO;
    input_asm.topology = VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST;

    /* Viewport + scissor (dynamic) */
    VkViewport viewport;
    viewport.x        = 0.0f;
    viewport.y        = 0.0f;
    viewport.width    = (float)g_swapchain_extent.width;
    viewport.height   = (float)g_swapchain_extent.height;
    viewport.minDepth = 0.0f;
    viewport.maxDepth = 1.0f;

    VkRect2D scissor;
    scissor.offset.x = 0;
    scissor.offset.y = 0;
    scissor.extent   = g_swapchain_extent;

    VkPipelineViewportStateCreateInfo viewport_state;
    memset(&viewport_state, 0, sizeof(viewport_state));
    viewport_state.sType         = VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO;
    viewport_state.viewportCount = 1;
    viewport_state.pViewports    = &viewport;
    viewport_state.scissorCount  = 1;
    viewport_state.pScissors     = &scissor;

    /* Rasterizer */
    VkPipelineRasterizationStateCreateInfo raster;
    memset(&raster, 0, sizeof(raster));
    raster.sType       = VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO;
    raster.polygonMode = VK_POLYGON_MODE_FILL;
    raster.lineWidth   = 1.0f;
    raster.cullMode    = VK_CULL_MODE_BACK_BIT;
    raster.frontFace   = VK_FRONT_FACE_CLOCKWISE;

    /* Multisampling */
    VkPipelineMultisampleStateCreateInfo multisample;
    memset(&multisample, 0, sizeof(multisample));
    multisample.sType                = VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO;
    multisample.rasterizationSamples = VK_SAMPLE_COUNT_1_BIT;

    /* Color blend */
    VkPipelineColorBlendAttachmentState blend_attach;
    memset(&blend_attach, 0, sizeof(blend_attach));
    blend_attach.colorWriteMask = VK_COLOR_COMPONENT_R_BIT |
                                  VK_COLOR_COMPONENT_G_BIT |
                                  VK_COLOR_COMPONENT_B_BIT |
                                  VK_COLOR_COMPONENT_A_BIT;
    blend_attach.blendEnable = VK_FALSE;

    VkPipelineColorBlendStateCreateInfo blend;
    memset(&blend, 0, sizeof(blend));
    blend.sType           = VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO;
    blend.attachmentCount = 1;
    blend.pAttachments    = &blend_attach;

    /* Dynamic state: viewport and scissor */
    VkDynamicState dyn_states[] = {VK_DYNAMIC_STATE_VIEWPORT,
                                   VK_DYNAMIC_STATE_SCISSOR};
    VkPipelineDynamicStateCreateInfo dynamic;
    memset(&dynamic, 0, sizeof(dynamic));
    dynamic.sType             = VK_STRUCTURE_TYPE_PIPELINE_DYNAMIC_STATE_CREATE_INFO;
    dynamic.dynamicStateCount = 2;
    dynamic.pDynamicStates    = dyn_states;

    /* Pipeline layout (empty - no descriptors) */
    VkPipelineLayoutCreateInfo layout_ci;
    memset(&layout_ci, 0, sizeof(layout_ci));
    layout_ci.sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO;

    VK_CHECK(vkCreatePipelineLayout(g_device, &layout_ci, NULL,
                                     &g_pipeline_layout));

    /* Create the pipeline */
    VkGraphicsPipelineCreateInfo pipe_ci;
    memset(&pipe_ci, 0, sizeof(pipe_ci));
    pipe_ci.sType               = VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO;
    pipe_ci.stageCount          = 2;
    pipe_ci.pStages             = stages;
    pipe_ci.pVertexInputState   = &vertex_input;
    pipe_ci.pInputAssemblyState = &input_asm;
    pipe_ci.pViewportState      = &viewport_state;
    pipe_ci.pRasterizationState = &raster;
    pipe_ci.pMultisampleState   = &multisample;
    pipe_ci.pColorBlendState    = &blend;
    pipe_ci.pDynamicState       = &dynamic;
    pipe_ci.layout              = g_pipeline_layout;
    pipe_ci.renderPass          = g_render_pass;
    pipe_ci.subpass             = 0;

    VK_CHECK(vkCreateGraphicsPipelines(g_device, VK_NULL_HANDLE, 1, &pipe_ci,
                                        NULL, &g_pipeline));

    vkDestroyShaderModule(g_device, vert_mod, NULL);
    vkDestroyShaderModule(g_device, frag_mod, NULL);
    printf("[vulkan] Graphics pipeline created\n");
}

/* --------------------------------------------------------------------------
 * Framebuffers
 * -------------------------------------------------------------------------- */
static void create_framebuffers(void) {
    g_swapchain_framebuffers =
        (VkFramebuffer*)malloc(g_swapchain_image_count * sizeof(VkFramebuffer));

    for (uint32_t i = 0; i < g_swapchain_image_count; i++) {
        VkFramebufferCreateInfo ci;
        memset(&ci, 0, sizeof(ci));
        ci.sType           = VK_STRUCTURE_TYPE_FRAMEBUFFER_CREATE_INFO;
        ci.renderPass      = g_render_pass;
        ci.attachmentCount = 1;
        ci.pAttachments    = &g_swapchain_image_views[i];
        ci.width           = g_swapchain_extent.width;
        ci.height          = g_swapchain_extent.height;
        ci.layers          = 1;

        VK_CHECK(vkCreateFramebuffer(g_device, &ci, NULL,
                                      &g_swapchain_framebuffers[i]));
    }
    printf("[vulkan] Framebuffers created\n");
}

/* --------------------------------------------------------------------------
 * Command pool + buffers
 * -------------------------------------------------------------------------- */
static void create_command_pool(void) {
    VkCommandPoolCreateInfo ci;
    memset(&ci, 0, sizeof(ci));
    ci.sType            = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO;
    ci.flags            = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT;
    ci.queueFamilyIndex = g_graphics_family;

    VK_CHECK(vkCreateCommandPool(g_device, &ci, NULL, &g_command_pool));

    VkCommandBufferAllocateInfo ai;
    memset(&ai, 0, sizeof(ai));
    ai.sType              = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO;
    ai.commandPool        = g_command_pool;
    ai.level              = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
    ai.commandBufferCount = MAX_FRAMES_IN_FLIGHT;

    VK_CHECK(vkAllocateCommandBuffers(g_device, &ai, g_command_buffers));
    printf("[vulkan] Command pool + buffers created\n");
}

/* --------------------------------------------------------------------------
 * Sync objects
 * -------------------------------------------------------------------------- */
static void create_sync_objects(void) {
    VkSemaphoreCreateInfo sem_ci;
    memset(&sem_ci, 0, sizeof(sem_ci));
    sem_ci.sType = VK_STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO;

    VkFenceCreateInfo fence_ci;
    memset(&fence_ci, 0, sizeof(fence_ci));
    fence_ci.sType = VK_STRUCTURE_TYPE_FENCE_CREATE_INFO;
    fence_ci.flags = VK_FENCE_CREATE_SIGNALED_BIT;

    for (int i = 0; i < MAX_FRAMES_IN_FLIGHT; i++) {
        VK_CHECK(vkCreateSemaphore(g_device, &sem_ci, NULL,
                                    &g_image_available[i]));
        VK_CHECK(vkCreateSemaphore(g_device, &sem_ci, NULL,
                                    &g_render_finished[i]));
        VK_CHECK(vkCreateFence(g_device, &fence_ci, NULL,
                                &g_in_flight[i]));
    }
    printf("[vulkan] Sync objects created\n");
}

/* --------------------------------------------------------------------------
 * Record command buffer
 * -------------------------------------------------------------------------- */
static void record_command_buffer(VkCommandBuffer cmd, uint32_t image_idx) {
    VkCommandBufferBeginInfo begin;
    memset(&begin, 0, sizeof(begin));
    begin.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO;
    VK_CHECK(vkBeginCommandBuffer(cmd, &begin));

    VkClearValue clear;
    clear.color.float32[0] = 0.01f;  /* near-black background */
    clear.color.float32[1] = 0.01f;
    clear.color.float32[2] = 0.02f;
    clear.color.float32[3] = 1.0f;

    VkRenderPassBeginInfo rp;
    memset(&rp, 0, sizeof(rp));
    rp.sType             = VK_STRUCTURE_TYPE_RENDER_PASS_BEGIN_INFO;
    rp.renderPass        = g_render_pass;
    rp.framebuffer       = g_swapchain_framebuffers[image_idx];
    rp.renderArea.offset.x = 0;
    rp.renderArea.offset.y = 0;
    rp.renderArea.extent = g_swapchain_extent;
    rp.clearValueCount   = 1;
    rp.pClearValues      = &clear;

    vkCmdBeginRenderPass(cmd, &rp, VK_SUBPASS_CONTENTS_INLINE);
    vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, g_pipeline);

    /* Set dynamic viewport and scissor */
    VkViewport vp;
    vp.x        = 0.0f;
    vp.y        = 0.0f;
    vp.width    = (float)g_swapchain_extent.width;
    vp.height   = (float)g_swapchain_extent.height;
    vp.minDepth = 0.0f;
    vp.maxDepth = 1.0f;
    vkCmdSetViewport(cmd, 0, 1, &vp);

    VkRect2D sc;
    sc.offset.x = 0;
    sc.offset.y = 0;
    sc.extent   = g_swapchain_extent;
    vkCmdSetScissor(cmd, 0, 1, &sc);

    /* Draw the triangle: 3 vertices, 1 instance */
    vkCmdDraw(cmd, 3, 1, 0, 0);

    vkCmdEndRenderPass(cmd);
    VK_CHECK(vkEndCommandBuffer(cmd));
}

/* --------------------------------------------------------------------------
 * Swapchain recreation (for window resize)
 * -------------------------------------------------------------------------- */
static void cleanup_swapchain(void) {
    for (uint32_t i = 0; i < g_swapchain_image_count; i++) {
        vkDestroyFramebuffer(g_device, g_swapchain_framebuffers[i], NULL);
        vkDestroyImageView(g_device, g_swapchain_image_views[i], NULL);
    }
    free(g_swapchain_framebuffers);
    free(g_swapchain_image_views);
    free(g_swapchain_images);
    vkDestroySwapchainKHR(g_device, g_swapchain, NULL);
}

static void recreate_swapchain(void) {
    /* Handle minimization: wait until window has nonzero size */
    RECT r;
    GetClientRect(g_hwnd, &r);
    while ((r.right - r.left) == 0 || (r.bottom - r.top) == 0) {
        MSG msg;
        if (PeekMessageA(&msg, NULL, 0, 0, PM_REMOVE)) {
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }
        GetClientRect(g_hwnd, &r);
        if (!g_running) return;
    }

    vkDeviceWaitIdle(g_device);
    cleanup_swapchain();
    create_swapchain();
    create_framebuffers();
    g_framebuffer_resized = 0;
}

/* --------------------------------------------------------------------------
 * Draw frame
 * -------------------------------------------------------------------------- */
static void draw_frame(void) {
    vkWaitForFences(g_device, 1, &g_in_flight[g_current_frame], VK_TRUE,
                    UINT64_MAX);

    uint32_t image_idx;
    VkResult result = vkAcquireNextImageKHR(
        g_device, g_swapchain, UINT64_MAX,
        g_image_available[g_current_frame], VK_NULL_HANDLE, &image_idx);

    if (result == VK_ERROR_OUT_OF_DATE_KHR) {
        recreate_swapchain();
        return;
    }
    if (result != VK_SUCCESS && result != VK_SUBOPTIMAL_KHR) {
        fprintf(stderr, "Failed to acquire swapchain image\n");
        return;
    }

    vkResetFences(g_device, 1, &g_in_flight[g_current_frame]);
    vkResetCommandBuffer(g_command_buffers[g_current_frame], 0);
    record_command_buffer(g_command_buffers[g_current_frame], image_idx);

    VkPipelineStageFlags wait_stage =
        VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT;

    VkSubmitInfo submit;
    memset(&submit, 0, sizeof(submit));
    submit.sType                = VK_STRUCTURE_TYPE_SUBMIT_INFO;
    submit.waitSemaphoreCount   = 1;
    submit.pWaitSemaphores      = &g_image_available[g_current_frame];
    submit.pWaitDstStageMask    = &wait_stage;
    submit.commandBufferCount   = 1;
    submit.pCommandBuffers      = &g_command_buffers[g_current_frame];
    submit.signalSemaphoreCount = 1;
    submit.pSignalSemaphores    = &g_render_finished[g_current_frame];

    VK_CHECK(vkQueueSubmit(g_graphics_queue, 1, &submit,
                            g_in_flight[g_current_frame]));

    VkPresentInfoKHR present;
    memset(&present, 0, sizeof(present));
    present.sType              = VK_STRUCTURE_TYPE_PRESENT_INFO_KHR;
    present.waitSemaphoreCount = 1;
    present.pWaitSemaphores    = &g_render_finished[g_current_frame];
    present.swapchainCount     = 1;
    present.pSwapchains        = &g_swapchain;
    present.pImageIndices      = &image_idx;

    result = vkQueuePresentKHR(g_present_queue, &present);

    if (result == VK_ERROR_OUT_OF_DATE_KHR || result == VK_SUBOPTIMAL_KHR ||
        g_framebuffer_resized) {
        g_framebuffer_resized = 0;
        recreate_swapchain();
    } else if (result != VK_SUCCESS) {
        fprintf(stderr, "Failed to present\n");
    }

    g_current_frame = (g_current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
}

/* --------------------------------------------------------------------------
 * Cleanup
 * -------------------------------------------------------------------------- */
static void cleanup(void) {
    vkDeviceWaitIdle(g_device);

    for (int i = 0; i < MAX_FRAMES_IN_FLIGHT; i++) {
        vkDestroySemaphore(g_device, g_image_available[i], NULL);
        vkDestroySemaphore(g_device, g_render_finished[i], NULL);
        vkDestroyFence(g_device, g_in_flight[i], NULL);
    }

    vkDestroyCommandPool(g_device, g_command_pool, NULL);

    for (uint32_t i = 0; i < g_swapchain_image_count; i++) {
        vkDestroyFramebuffer(g_device, g_swapchain_framebuffers[i], NULL);
        vkDestroyImageView(g_device, g_swapchain_image_views[i], NULL);
    }
    free(g_swapchain_framebuffers);
    free(g_swapchain_image_views);
    free(g_swapchain_images);

    vkDestroyPipeline(g_device, g_pipeline, NULL);
    vkDestroyPipelineLayout(g_device, g_pipeline_layout, NULL);
    vkDestroyRenderPass(g_device, g_render_pass, NULL);
    vkDestroySwapchainKHR(g_device, g_swapchain, NULL);
    vkDestroyDevice(g_device, NULL);
    vkDestroySurfaceKHR(g_instance, g_surface, NULL);
    vkDestroyInstance(g_instance, NULL);
    DestroyWindow(g_hwnd);

    printf("[vulkan] Cleanup complete\n");
}

/* --------------------------------------------------------------------------
 * Entry point
 * -------------------------------------------------------------------------- */
int main(void) {
    printf("=== QuantaLang Vulkan Triangle Renderer ===\n\n");

    create_window();
    create_instance();
    create_surface();
    pick_physical_device();
    create_device();
    create_swapchain();
    create_render_pass();
    create_pipeline();
    create_framebuffers();
    create_command_pool();
    create_sync_objects();

    printf("\n[main] Entering render loop (press ESC or close window to exit)\n\n");

    /* Main message + render loop */
    while (g_running) {
        MSG msg;
        while (PeekMessageA(&msg, NULL, 0, 0, PM_REMOVE)) {
            if (msg.message == WM_QUIT) {
                g_running = 0;
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageA(&msg);
        }
        if (g_running) {
            draw_frame();
        }
    }

    cleanup();
    printf("\n=== Done ===\n");
    return 0;
}
