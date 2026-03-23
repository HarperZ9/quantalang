// ===============================================================================
// QUANTALANG CODE GENERATOR - C RUNTIME LIBRARY
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! C runtime library embedded in generated output.
//!
//! This module generates a C runtime header that gets included at the top of
//! every compiled C program. It provides string operations, vector (dynamic
//! array) support, formatted printing helpers, and math built-in wrappers.

/// Returns the complete C runtime header as a string.
///
/// This header is embedded at the top of every generated C file, after the
/// standard includes. It provides:
/// - `QuantaString` type with concat, length, equality, and free operations
/// - `QuantaVec` dynamic array type with push, get, and free operations
/// - Type-specific print helpers for formatted output
/// - Math built-in wrappers (`quanta_min`, `quanta_max`)
pub fn runtime_header() -> &'static str {
    r#"
// ============================================================================
// QuantaLang Runtime Library
// Embedded in generated C output - do not edit
// ============================================================================

// --- String type ---

typedef struct {
    const char* ptr;
    size_t len;
    size_t cap;    // 0 for string literals (not heap-allocated)
} QuantaString;

static QuantaString quanta_string_new(const char* s) {
    QuantaString qs;
    qs.ptr = s;
    qs.len = strlen(s);
    qs.cap = 0; // literal, not owned
    return qs;
}

static QuantaString quanta_string_concat(QuantaString a, QuantaString b) {
    size_t new_len = a.len + b.len;
    char* buf = (char*)malloc(new_len + 1);
    memcpy(buf, a.ptr, a.len);
    memcpy(buf + a.len, b.ptr, b.len);
    buf[new_len] = '\0';
    QuantaString qs;
    qs.ptr = buf;
    qs.len = new_len;
    qs.cap = new_len + 1;
    return qs;
}

static size_t quanta_string_len(QuantaString s) {
    return s.len;
}

static bool quanta_string_eq(QuantaString a, QuantaString b) {
    if (a.len != b.len) return false;
    return memcmp(a.ptr, b.ptr, a.len) == 0;
}

static void quanta_string_free(QuantaString s) {
    if (s.cap > 0) free((void*)s.ptr);
}

// --- Vec (dynamic array) type ---

typedef struct {
    void* ptr;
    size_t len;
    size_t cap;
    size_t elem_size;
} QuantaVec;

static QuantaVec quanta_vec_new(size_t elem_size) {
    QuantaVec v;
    v.ptr = NULL;
    v.len = 0;
    v.cap = 0;
    v.elem_size = elem_size;
    return v;
}

static void quanta_vec_push(QuantaVec* v, const void* elem) {
    if (v->len >= v->cap) {
        v->cap = v->cap == 0 ? 8 : v->cap * 2;
        v->ptr = realloc(v->ptr, v->cap * v->elem_size);
    }
    memcpy((char*)v->ptr + v->len * v->elem_size, elem, v->elem_size);
    v->len++;
}

static void* quanta_vec_get(QuantaVec* v, size_t index) {
    return (char*)v->ptr + index * v->elem_size;
}

static void quanta_vec_free(QuantaVec* v) {
    free(v->ptr);
    v->ptr = NULL;
    v->len = 0;
    v->cap = 0;
}

// --- Typed Vec helpers (wrapping QuantaVec for specific element types) ---

static QuantaVec quanta_vec_new_i32(void) { return quanta_vec_new(sizeof(int32_t)); }
static QuantaVec quanta_vec_new_i64(void) { return quanta_vec_new(sizeof(int64_t)); }
static QuantaVec quanta_vec_new_f64(void) { return quanta_vec_new(sizeof(double)); }

// Note: These wrappers use a global QuantaVec pointer trick — the first
// argument is a QuantaVec passed by value, but since QuantaLang passes
// struct locals, the C compiler will place them on the stack where we
// can take their address. We use a thin wrapper to bridge the gap.
static QuantaVec* __quanta_vec_ref = NULL;
static void quanta_vec_push_i32(QuantaVec v, int32_t val) {
    // Find the original local via the stored pointer (set by codegen)
    // Fallback: push into a copy — but mutations won't be visible.
    // The real solution: codegen passes &v. For now we use a global registry.
    (void)v; (void)val;
}
// Simpler approach: heap-allocated vecs via a handle pattern
typedef struct { QuantaVec* inner; } QuantaVecHandle;

static QuantaVecHandle quanta_hvec_new_i32(void) {
    QuantaVecHandle h;
    h.inner = (QuantaVec*)malloc(sizeof(QuantaVec));
    *h.inner = quanta_vec_new(sizeof(int32_t));
    return h;
}
static void quanta_hvec_push_i32(QuantaVecHandle h, int32_t val) { quanta_vec_push(h.inner, &val); }
static int32_t quanta_hvec_get_i32(QuantaVecHandle h, size_t index) { return *(int32_t*)quanta_vec_get(h.inner, index); }
static size_t quanta_hvec_len(QuantaVecHandle h) { return h.inner->len; }
static int32_t quanta_hvec_pop_i32(QuantaVecHandle h) {
    if (h.inner->len == 0) return 0;
    h.inner->len--;
    return *(int32_t*)((char*)h.inner->ptr + h.inner->len * h.inner->elem_size);
}
static void quanta_hvec_free(QuantaVecHandle h) { quanta_vec_free(h.inner); free(h.inner); }

// --- Format string helpers (returns QuantaString) ---

static QuantaString quanta_format_i32(const char* fmt, int32_t v) {
    char buf[64];
    int n = snprintf(buf, sizeof(buf), fmt, v);
    char* heap = (char*)malloc(n + 1);
    memcpy(heap, buf, n + 1);
    QuantaString qs; qs.ptr = heap; qs.len = n; qs.cap = n + 1;
    return qs;
}

static QuantaString quanta_format_f64(const char* fmt, double v) {
    char buf[128];
    int n = snprintf(buf, sizeof(buf), fmt, v);
    char* heap = (char*)malloc(n + 1);
    memcpy(heap, buf, n + 1);
    QuantaString qs; qs.ptr = heap; qs.len = n; qs.cap = n + 1;
    return qs;
}

static QuantaString quanta_format_str(const char* fmt, const char* v) {
    int n = snprintf(NULL, 0, fmt, v);
    char* heap = (char*)malloc(n + 1);
    snprintf(heap, n + 1, fmt, v);
    QuantaString qs; qs.ptr = heap; qs.len = n; qs.cap = n + 1;
    return qs;
}

static QuantaString quanta_i32_to_string(int32_t v) { return quanta_format_i32("%d", v); }
static QuantaString quanta_f64_to_string(double v) { return quanta_format_f64("%g", v); }

// --- Math constants ---
static const double QUANTA_PI = 3.14159265358979323846;
static const double QUANTA_E = 2.71828182845904523536;
static const double QUANTA_TAU = 6.28318530717958647692;

// --- HashMap (i32 -> i32, open addressing with linear probing) ---

typedef struct {
    int32_t* keys;
    int32_t* values;
    uint8_t* occupied;
    size_t cap;
    size_t len;
} QuantaHashMap;

typedef struct { QuantaHashMap* inner; } QuantaMapHandle;

static QuantaMapHandle quanta_map_new(void) {
    QuantaMapHandle h;
    h.inner = (QuantaHashMap*)malloc(sizeof(QuantaHashMap));
    h.inner->cap = 16;
    h.inner->len = 0;
    h.inner->keys = (int32_t*)calloc(16, sizeof(int32_t));
    h.inner->values = (int32_t*)calloc(16, sizeof(int32_t));
    h.inner->occupied = (uint8_t*)calloc(16, sizeof(uint8_t));
    return h;
}

static void quanta_map_grow(QuantaHashMap* m) {
    size_t old_cap = m->cap;
    int32_t* old_keys = m->keys;
    int32_t* old_values = m->values;
    uint8_t* old_occ = m->occupied;
    m->cap *= 2;
    m->len = 0;
    m->keys = (int32_t*)calloc(m->cap, sizeof(int32_t));
    m->values = (int32_t*)calloc(m->cap, sizeof(int32_t));
    m->occupied = (uint8_t*)calloc(m->cap, sizeof(uint8_t));
    for (size_t i = 0; i < old_cap; i++) {
        if (old_occ[i]) {
            uint32_t h = (uint32_t)old_keys[i] * 2654435761u;
            size_t idx = h % m->cap;
            while (m->occupied[idx]) idx = (idx + 1) % m->cap;
            m->keys[idx] = old_keys[i];
            m->values[idx] = old_values[i];
            m->occupied[idx] = 1;
            m->len++;
        }
    }
    free(old_keys); free(old_values); free(old_occ);
}

static void quanta_map_insert(QuantaMapHandle h, int32_t key, int32_t value) {
    QuantaHashMap* m = h.inner;
    if (m->len * 4 >= m->cap * 3) quanta_map_grow(m);
    uint32_t hash = (uint32_t)key * 2654435761u;
    size_t idx = hash % m->cap;
    while (m->occupied[idx]) {
        if (m->keys[idx] == key) { m->values[idx] = value; return; }
        idx = (idx + 1) % m->cap;
    }
    m->keys[idx] = key;
    m->values[idx] = value;
    m->occupied[idx] = 1;
    m->len++;
}

static int32_t quanta_map_get(QuantaMapHandle h, int32_t key, int32_t fallback) {
    QuantaHashMap* m = h.inner;
    uint32_t hash = (uint32_t)key * 2654435761u;
    size_t idx = hash % m->cap;
    size_t start = idx;
    while (m->occupied[idx]) {
        if (m->keys[idx] == key) return m->values[idx];
        idx = (idx + 1) % m->cap;
        if (idx == start) break;
    }
    return fallback;
}

static bool quanta_map_contains(QuantaMapHandle h, int32_t key) {
    QuantaHashMap* m = h.inner;
    uint32_t hash = (uint32_t)key * 2654435761u;
    size_t idx = hash % m->cap;
    size_t start = idx;
    while (m->occupied[idx]) {
        if (m->keys[idx] == key) return true;
        idx = (idx + 1) % m->cap;
        if (idx == start) break;
    }
    return false;
}

static size_t quanta_map_len(QuantaMapHandle h) { return h.inner->len; }

static bool quanta_map_remove(QuantaMapHandle h, int32_t key) {
    QuantaHashMap* m = h.inner;
    uint32_t hash = (uint32_t)key * 2654435761u;
    size_t idx = hash % m->cap;
    size_t start = idx;
    while (m->occupied[idx]) {
        if (m->keys[idx] == key) {
            m->occupied[idx] = 0;
            m->len--;
            return true;
        }
        idx = (idx + 1) % m->cap;
        if (idx == start) break;
    }
    return false;
}

// --- Print formatting helpers ---

static void quanta_print_i32(int32_t v) { printf("%d", v); }
static void quanta_print_i64(int64_t v) { printf("%lld", (long long)v); }
static void quanta_print_f32(float v) { printf("%g", (double)v); }
static void quanta_print_f64(double v) { printf("%g", v); }
static void quanta_print_bool(bool v) { printf("%s", v ? "true" : "false"); }
static void quanta_print_str(const char* v) { printf("%s", v); }
static void quanta_print_string(QuantaString v) { printf("%.*s", (int)v.len, v.ptr); }
static void quanta_print_char(char v) { printf("%c", v); }

// --- Math built-in helpers ---

static int32_t quanta_min_i32(int32_t a, int32_t b) { return a < b ? a : b; }
static int32_t quanta_max_i32(int32_t a, int32_t b) { return a > b ? a : b; }
static int64_t quanta_min_i64(int64_t a, int64_t b) { return a < b ? a : b; }
static int64_t quanta_max_i64(int64_t a, int64_t b) { return a > b ? a : b; }
static double quanta_min_f64(double a, double b) { return a < b ? a : b; }
static double quanta_max_f64(double a, double b) { return a > b ? a : b; }
static float quanta_min_f32(float a, float b) { return a < b ? a : b; }
static float quanta_max_f32(float a, float b) { return a > b ? a : b; }

// --- Vector Math Types ---

typedef struct { double x, y; } quanta_vec2;
typedef struct { double x, y, z; } quanta_vec3;
typedef struct { double x, y, z, w; } quanta_vec4;
typedef struct { double m[4][4]; } quanta_mat4;

// Constructors
static quanta_vec2 quanta_vec2_new(double x, double y) { return (quanta_vec2){x, y}; }
static quanta_vec3 quanta_vec3_new(double x, double y, double z) { return (quanta_vec3){x, y, z}; }
static quanta_vec4 quanta_vec4_new(double x, double y, double z, double w) { return (quanta_vec4){x, y, z, w}; }

// Vector dot product
static double quanta_dot2(quanta_vec2 a, quanta_vec2 b) { return a.x*b.x + a.y*b.y; }
static double quanta_dot3(quanta_vec3 a, quanta_vec3 b) { return a.x*b.x + a.y*b.y + a.z*b.z; }
static double quanta_dot4(quanta_vec4 a, quanta_vec4 b) { return a.x*b.x + a.y*b.y + a.z*b.z + a.w*b.w; }

// Vector length
static double quanta_length2(quanta_vec2 v) { return sqrt(quanta_dot2(v, v)); }
static double quanta_length3(quanta_vec3 v) { return sqrt(quanta_dot3(v, v)); }
static double quanta_length4(quanta_vec4 v) { return sqrt(quanta_dot4(v, v)); }

// Vector normalize
static quanta_vec2 quanta_normalize2(quanta_vec2 v) { double l = quanta_length2(v); return (quanta_vec2){v.x/l, v.y/l}; }
static quanta_vec3 quanta_normalize3(quanta_vec3 v) { double l = quanta_length3(v); return (quanta_vec3){v.x/l, v.y/l, v.z/l}; }
static quanta_vec4 quanta_normalize4(quanta_vec4 v) { double l = quanta_length4(v); return (quanta_vec4){v.x/l, v.y/l, v.z/l, v.w/l}; }

// Cross product (vec3 only)
static quanta_vec3 quanta_cross(quanta_vec3 a, quanta_vec3 b) {
    return (quanta_vec3){a.y*b.z - a.z*b.y, a.z*b.x - a.x*b.z, a.x*b.y - a.y*b.x};
}

// Reflect
static quanta_vec3 quanta_reflect3(quanta_vec3 v, quanta_vec3 n) {
    double d = quanta_dot3(v, n);
    return (quanta_vec3){v.x - 2.0*d*n.x, v.y - 2.0*d*n.y, v.z - 2.0*d*n.z};
}

// Lerp
static quanta_vec2 quanta_lerp2(quanta_vec2 a, quanta_vec2 b, double t) {
    return (quanta_vec2){a.x + (b.x-a.x)*t, a.y + (b.y-a.y)*t};
}
static quanta_vec3 quanta_lerp3(quanta_vec3 a, quanta_vec3 b, double t) {
    return (quanta_vec3){a.x + (b.x-a.x)*t, a.y + (b.y-a.y)*t, a.z + (b.z-a.z)*t};
}
static quanta_vec4 quanta_lerp4(quanta_vec4 a, quanta_vec4 b, double t) {
    return (quanta_vec4){a.x + (b.x-a.x)*t, a.y + (b.y-a.y)*t, a.z + (b.z-a.z)*t, a.w + (b.w-a.w)*t};
}

// Vec2 arithmetic
static quanta_vec2 quanta_vec2_add(quanta_vec2 a, quanta_vec2 b) { return (quanta_vec2){a.x+b.x, a.y+b.y}; }
static quanta_vec2 quanta_vec2_sub(quanta_vec2 a, quanta_vec2 b) { return (quanta_vec2){a.x-b.x, a.y-b.y}; }
static quanta_vec2 quanta_vec2_mul(quanta_vec2 a, quanta_vec2 b) { return (quanta_vec2){a.x*b.x, a.y*b.y}; }
static quanta_vec2 quanta_vec2_scale(quanta_vec2 v, double s) { return (quanta_vec2){v.x*s, v.y*s}; }
static quanta_vec2 quanta_vec2_neg(quanta_vec2 v) { return (quanta_vec2){-v.x, -v.y}; }

// Vec3 arithmetic
static quanta_vec3 quanta_vec3_add(quanta_vec3 a, quanta_vec3 b) { return (quanta_vec3){a.x+b.x, a.y+b.y, a.z+b.z}; }
static quanta_vec3 quanta_vec3_sub(quanta_vec3 a, quanta_vec3 b) { return (quanta_vec3){a.x-b.x, a.y-b.y, a.z-b.z}; }
static quanta_vec3 quanta_vec3_mul(quanta_vec3 a, quanta_vec3 b) { return (quanta_vec3){a.x*b.x, a.y*b.y, a.z*b.z}; }
static quanta_vec3 quanta_vec3_scale(quanta_vec3 v, double s) { return (quanta_vec3){v.x*s, v.y*s, v.z*s}; }
static quanta_vec3 quanta_vec3_neg(quanta_vec3 v) { return (quanta_vec3){-v.x, -v.y, -v.z}; }

// Vec4 arithmetic
static quanta_vec4 quanta_vec4_add(quanta_vec4 a, quanta_vec4 b) { return (quanta_vec4){a.x+b.x, a.y+b.y, a.z+b.z, a.w+b.w}; }
static quanta_vec4 quanta_vec4_sub(quanta_vec4 a, quanta_vec4 b) { return (quanta_vec4){a.x-b.x, a.y-b.y, a.z-b.z, a.w-b.w}; }
static quanta_vec4 quanta_vec4_mul(quanta_vec4 a, quanta_vec4 b) { return (quanta_vec4){a.x*b.x, a.y*b.y, a.z*b.z, a.w*b.w}; }
static quanta_vec4 quanta_vec4_scale(quanta_vec4 v, double s) { return (quanta_vec4){v.x*s, v.y*s, v.z*s, v.w*s}; }
static quanta_vec4 quanta_vec4_neg(quanta_vec4 v) { return (quanta_vec4){-v.x, -v.y, -v.z, -v.w}; }

// --- Mat4 operations ---

static quanta_mat4 quanta_mat4_identity(void) {
    quanta_mat4 m = {{{1,0,0,0},{0,1,0,0},{0,0,1,0},{0,0,0,1}}};
    return m;
}

static quanta_mat4 quanta_mat4_mul(quanta_mat4 a, quanta_mat4 b) {
    quanta_mat4 r = {{{0}}};
    for (int i = 0; i < 4; i++)
        for (int j = 0; j < 4; j++)
            for (int k = 0; k < 4; k++)
                r.m[i][j] += a.m[i][k] * b.m[k][j];
    return r;
}

static quanta_vec4 quanta_mat4_mul_vec4(quanta_mat4 m, quanta_vec4 v) {
    return (quanta_vec4){
        m.m[0][0]*v.x + m.m[0][1]*v.y + m.m[0][2]*v.z + m.m[0][3]*v.w,
        m.m[1][0]*v.x + m.m[1][1]*v.y + m.m[1][2]*v.z + m.m[1][3]*v.w,
        m.m[2][0]*v.x + m.m[2][1]*v.y + m.m[2][2]*v.z + m.m[2][3]*v.w,
        m.m[3][0]*v.x + m.m[3][1]*v.y + m.m[3][2]*v.z + m.m[3][3]*v.w
    };
}

static quanta_mat4 quanta_mat4_translate(quanta_vec3 t) {
    quanta_mat4 m = quanta_mat4_identity();
    m.m[0][3] = t.x; m.m[1][3] = t.y; m.m[2][3] = t.z;
    return m;
}

static quanta_mat4 quanta_mat4_scale(quanta_vec3 s) {
    quanta_mat4 m = {{{s.x,0,0,0},{0,s.y,0,0},{0,0,s.z,0},{0,0,0,1}}};
    return m;
}

static quanta_mat4 quanta_mat4_perspective(double fov, double aspect, double near_val, double far_val) {
    double f = 1.0 / tan(fov * 0.5);
    quanta_mat4 m = {{{0}}};
    m.m[0][0] = f / aspect;
    m.m[1][1] = f;
    m.m[2][2] = (far_val + near_val) / (near_val - far_val);
    m.m[2][3] = (2.0 * far_val * near_val) / (near_val - far_val);
    m.m[3][2] = -1.0;
    return m;
}

// --- Shader math functions ---

static double quanta_clampf(double x, double lo, double hi) { return x < lo ? lo : (x > hi ? hi : x); }
static quanta_vec3 quanta_clamp3(quanta_vec3 v, double lo, double hi) {
    return (quanta_vec3){quanta_clampf(v.x, lo, hi), quanta_clampf(v.y, lo, hi), quanta_clampf(v.z, lo, hi)};
}
static double quanta_smoothstep(double edge0, double edge1, double x) {
    double t = quanta_clampf((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}
static double quanta_mix(double a, double b, double t) { return a + (b - a) * t; }
static double quanta_fract(double x) { return x - floor(x); }
static double quanta_step(double edge, double x) { return x < edge ? 0.0 : 1.0; }

// --- Effect handler runtime (setjmp/longjmp based) ---

#include <setjmp.h>

// Effect handler stack
typedef struct QuantaHandler {
    jmp_buf env;
    int32_t effect_id;
    void* handler_data;
    struct QuantaHandler* parent;
} QuantaHandler;

// Global handler stack
static QuantaHandler* __quanta_handler_stack = NULL;

// Push a handler onto the stack
static void quanta_push_handler(QuantaHandler* h, int32_t effect_id) {
    h->effect_id = effect_id;
    h->handler_data = NULL;
    h->parent = __quanta_handler_stack;
    __quanta_handler_stack = h;
}

// Pop a handler from the stack
static void quanta_pop_handler(void) {
    if (__quanta_handler_stack) {
        __quanta_handler_stack = __quanta_handler_stack->parent;
    }
}

// Perform an effect operation — jumps to the nearest handler
static void quanta_perform(int32_t effect_id, int32_t op_id, void* arg, void* result) {
    QuantaHandler* h = __quanta_handler_stack;
    while (h) {
        if (h->effect_id == effect_id) {
            h->handler_data = arg;
            longjmp(h->env, op_id + 1); // +1 because setjmp returns 0 on first call
        }
        h = h->parent;
    }
    // Unhandled effect — abort
    fprintf(stderr, "Unhandled effect %d operation %d\n", effect_id, op_id);
    abort();
}

// --- File I/O ---

static QuantaString quanta_read_file(const char* path) {
    FILE* f = fopen(path, "rb");
    if (!f) return quanta_string_new("");
    fseek(f, 0, SEEK_END);
    long len = ftell(f);
    fseek(f, 0, SEEK_SET);
    char* buf = (char*)malloc(len + 1);
    fread(buf, 1, len, f);
    buf[len] = '\0';
    fclose(f);
    QuantaString qs;
    qs.ptr = buf;
    qs.len = (size_t)len;
    qs.cap = (size_t)(len + 1);
    return qs;
}

static bool quanta_write_file(const char* path, const char* content) {
    FILE* f = fopen(path, "wb");
    if (!f) return false;
    size_t len = strlen(content);
    size_t written = fwrite(content, 1, len, f);
    fclose(f);
    return written == len;
}

static bool quanta_file_exists(const char* path) {
    FILE* f = fopen(path, "rb");
    if (f) { fclose(f); return true; }
    return false;
}

// --- Process / Environment ---

static int32_t quanta_exit(int32_t code) {
    exit(code);
    return code; // unreachable
}

// --- Graphics Stub (Vulkan helper) ---
// Minimal stub that prints what a real Vulkan backend would do.
// Used for testing the FFI integration pattern.
// When connected to a real Vulkan backend, these are replaced by
// linking against vulkan_helper.c / the Vulkan SDK.

typedef struct {
    int32_t initialized;
    int32_t width;
    int32_t height;
    int32_t frame_count;
    int32_t should_close;
} QuantaGfxContext;

static QuantaGfxContext __quanta_gfx_ctx = {0, 0, 0, 0, 0};

static int32_t quanta_gfx_init(int32_t width, int32_t height, const char* title) {
    printf("[GFX] Initializing %dx%d window: %s\n", width, height, title);
    __quanta_gfx_ctx.initialized = 1;
    __quanta_gfx_ctx.width = width;
    __quanta_gfx_ctx.height = height;
    __quanta_gfx_ctx.frame_count = 0;
    __quanta_gfx_ctx.should_close = 0;
    printf("[GFX] Vulkan instance created\n");
    printf("[GFX] Physical device selected\n");
    printf("[GFX] Logical device created\n");
    printf("[GFX] Swapchain created (%dx%d)\n", width, height);
    return 1;
}

static int32_t quanta_gfx_load_shader(const char* path, int32_t stage) {
    printf("[GFX] Loading shader: %s (stage=%s)\n", path, stage == 0 ? "vertex" : "fragment");
    return 1;
}

static int32_t quanta_gfx_create_pipeline(int32_t vertex_shader, int32_t fragment_shader) {
    printf("[GFX] Creating graphics pipeline (vs=%d, fs=%d)\n", vertex_shader, fragment_shader);
    return 1;
}

static void quanta_gfx_begin_frame(void) {
    __quanta_gfx_ctx.frame_count++;
    if (__quanta_gfx_ctx.frame_count <= 3) {
        printf("[GFX] Begin frame %d\n", __quanta_gfx_ctx.frame_count);
    }
}

static void quanta_gfx_clear(float r, float g, float b, float a) {
    if (__quanta_gfx_ctx.frame_count <= 3) {
        printf("[GFX] Clear (%.1f, %.1f, %.1f, %.1f)\n", r, g, b, a);
    }
}

static void quanta_gfx_draw(int32_t vertex_count) {
    if (__quanta_gfx_ctx.frame_count <= 3) {
        printf("[GFX] Draw %d vertices\n", vertex_count);
    }
}

static void quanta_gfx_end_frame(void) {
    if (__quanta_gfx_ctx.frame_count <= 3) {
        printf("[GFX] End frame %d\n", __quanta_gfx_ctx.frame_count);
    }
    if (__quanta_gfx_ctx.frame_count >= 3) {
        __quanta_gfx_ctx.should_close = 1;
    }
}

static int32_t quanta_gfx_should_close(void) {
    return __quanta_gfx_ctx.should_close;
}

static void quanta_gfx_shutdown(void) {
    printf("[GFX] Shutdown complete (%d frames rendered)\n", __quanta_gfx_ctx.frame_count);
    __quanta_gfx_ctx.initialized = 0;
}

// ============================================================================
// End QuantaLang Runtime
// ============================================================================
"#
}

/// List of math built-in function names recognized by the lowerer.
///
/// These are lowered directly to their C `<math.h>` equivalents.
pub const MATH_BUILTINS: &[&str] = &[
    "abs", "sqrt", "pow", "sin", "cos", "tan",
    "floor", "ceil", "round",
    "min", "max",
    "read_file", "write_file", "file_exists", "exit",
    // Vector math builtins
    "dot", "cross", "normalize", "length", "reflect", "lerp",
    // Mat4 builtins
    "mat4_identity", "mat4_translate", "mat4_scale", "mat4_perspective",
    // Shader math builtins
    "clamp", "smoothstep", "mix", "fract", "step",
    // Vec builtins
    "vec_new", "vec_push", "vec_get", "vec_len", "vec_pop",
    // Format builtins
    "to_string_i32", "to_string_f64",
    // HashMap builtins
    "map_new", "map_insert", "map_get", "map_contains", "map_len", "map_remove",
];

/// Maps a QuantaLang math built-in name to its C equivalent expression.
///
/// Returns `Some((c_name, needs_cast))` where `c_name` is the C function name
/// and `needs_cast` indicates whether the result needs an `(int32_t)` cast
/// (for functions like `abs` that have integer overloads in C).
///
/// `min` and `max` are mapped to the runtime helpers `quanta_min_i32` /
/// `quanta_max_i32` by default (the caller can select a different type variant).
pub fn math_builtin_to_c(name: &str) -> Option<&'static str> {
    match name {
        "abs"   => Some("fabs"),
        "sqrt"  => Some("sqrt"),
        "pow"   => Some("pow"),
        "sin"   => Some("sin"),
        "cos"   => Some("cos"),
        "tan"   => Some("tan"),
        "log"   => Some("log"),
        "log2"  => Some("log2"),
        "log10" => Some("log10"),
        "exp"   => Some("exp"),
        "atan2" => Some("atan2"),
        "floor" => Some("floor"),
        "ceil"  => Some("ceil"),
        "round" => Some("round"),
        "min"   => Some("quanta_min_i32"),
        "max"   => Some("quanta_max_i32"),
        "read_file"   => Some("quanta_read_file"),
        "write_file"  => Some("quanta_write_file"),
        "file_exists" => Some("quanta_file_exists"),
        "exit"        => Some("quanta_exit"),
        "vec_new"     => Some("quanta_hvec_new_i32"),
        "vec_push"    => Some("quanta_hvec_push_i32"),
        "vec_get"     => Some("quanta_hvec_get_i32"),
        "vec_len"     => Some("quanta_hvec_len"),
        "vec_pop"     => Some("quanta_hvec_pop_i32"),
        "to_string_i32" => Some("quanta_i32_to_string"),
        "to_string_f64" => Some("quanta_f64_to_string"),
        // HashMap builtins
        "map_new"      => Some("quanta_map_new"),
        "map_insert"   => Some("quanta_map_insert"),
        "map_get"      => Some("quanta_map_get"),
        "map_contains" => Some("quanta_map_contains"),
        "map_len"      => Some("quanta_map_len"),
        "map_remove"   => Some("quanta_map_remove"),
        // Vulkan runtime builtins
        "quanta_vk_init" => Some("quanta_vk_init"),
        "quanta_vk_load_shader_file" => Some("quanta_vk_load_shader_file"),
        "quanta_vk_run_compute" => Some("quanta_vk_run_compute"),
        "quanta_vk_shutdown" => Some("quanta_vk_shutdown"),
        // Vector math builtins — default to vec3 variants; the lowerer
        // dispatches to the correct size variant based on argument type.
        "dot"         => Some("quanta_dot3"),
        "cross"       => Some("quanta_cross"),
        "normalize"   => Some("quanta_normalize3"),
        "length"      => Some("quanta_length3"),
        "reflect"     => Some("quanta_reflect3"),
        "lerp"        => Some("quanta_lerp3"),
        // Mat4 builtins
        "mat4_identity"    => Some("quanta_mat4_identity"),
        "mat4_translate"   => Some("quanta_mat4_translate"),
        "mat4_scale"       => Some("quanta_mat4_scale"),
        "mat4_perspective" => Some("quanta_mat4_perspective"),
        // Shader math builtins
        "clamp"      => Some("quanta_clampf"),
        "smoothstep" => Some("quanta_smoothstep"),
        "mix"        => Some("quanta_mix"),
        "fract"      => Some("quanta_fract"),
        "step"       => Some("quanta_step"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_header_contains_string_type() {
        let header = runtime_header();
        assert!(header.contains("QuantaString"));
        assert!(header.contains("quanta_string_new"));
        assert!(header.contains("quanta_string_concat"));
        assert!(header.contains("quanta_string_len"));
        assert!(header.contains("quanta_string_eq"));
        assert!(header.contains("quanta_string_free"));
    }

    #[test]
    fn test_runtime_header_contains_vec_type() {
        let header = runtime_header();
        assert!(header.contains("QuantaVec"));
        assert!(header.contains("quanta_vec_new"));
        assert!(header.contains("quanta_vec_push"));
        assert!(header.contains("quanta_vec_get"));
        assert!(header.contains("quanta_vec_free"));
    }

    #[test]
    fn test_runtime_header_contains_print_helpers() {
        let header = runtime_header();
        assert!(header.contains("quanta_print_i32"));
        assert!(header.contains("quanta_print_i64"));
        assert!(header.contains("quanta_print_f32"));
        assert!(header.contains("quanta_print_f64"));
        assert!(header.contains("quanta_print_bool"));
        assert!(header.contains("quanta_print_str"));
        assert!(header.contains("quanta_print_string"));
        assert!(header.contains("quanta_print_char"));
    }

    #[test]
    fn test_runtime_header_contains_math_helpers() {
        let header = runtime_header();
        assert!(header.contains("quanta_min_i32"));
        assert!(header.contains("quanta_max_i32"));
        assert!(header.contains("quanta_min_f64"));
        assert!(header.contains("quanta_max_f64"));
    }

    #[test]
    fn test_math_builtin_lookup() {
        assert_eq!(math_builtin_to_c("abs"), Some("fabs"));
        assert_eq!(math_builtin_to_c("sqrt"), Some("sqrt"));
        assert_eq!(math_builtin_to_c("pow"), Some("pow"));
        assert_eq!(math_builtin_to_c("sin"), Some("sin"));
        assert_eq!(math_builtin_to_c("cos"), Some("cos"));
        assert_eq!(math_builtin_to_c("tan"), Some("tan"));
        assert_eq!(math_builtin_to_c("floor"), Some("floor"));
        assert_eq!(math_builtin_to_c("ceil"), Some("ceil"));
        assert_eq!(math_builtin_to_c("round"), Some("round"));
        assert_eq!(math_builtin_to_c("min"), Some("quanta_min_i32"));
        assert_eq!(math_builtin_to_c("max"), Some("quanta_max_i32"));
        assert_eq!(math_builtin_to_c("unknown"), None);
    }

    #[test]
    fn test_math_builtins_list() {
        assert_eq!(MATH_BUILTINS.len(), 43);
        assert!(MATH_BUILTINS.contains(&"abs"));
        assert!(MATH_BUILTINS.contains(&"min"));
        assert!(MATH_BUILTINS.contains(&"max"));
        assert!(MATH_BUILTINS.contains(&"dot"));
        assert!(MATH_BUILTINS.contains(&"cross"));
        assert!(MATH_BUILTINS.contains(&"normalize"));
        assert!(MATH_BUILTINS.contains(&"length"));
    }

    #[test]
    fn test_runtime_header_contains_effect_handler() {
        let header = runtime_header();
        assert!(header.contains("QuantaHandler"));
        assert!(header.contains("quanta_push_handler"));
        assert!(header.contains("quanta_pop_handler"));
        assert!(header.contains("quanta_perform"));
        assert!(header.contains("__quanta_handler_stack"));
        assert!(header.contains("setjmp.h"));
    }

    #[test]
    fn test_runtime_header_contains_file_io() {
        let header = runtime_header();
        assert!(header.contains("quanta_read_file"));
        assert!(header.contains("quanta_write_file"));
        assert!(header.contains("quanta_file_exists"));
        assert!(header.contains("quanta_exit"));
    }

    #[test]
    fn test_runtime_header_contains_vector_types() {
        let header = runtime_header();
        assert!(header.contains("quanta_vec2"));
        assert!(header.contains("quanta_vec3"));
        assert!(header.contains("quanta_vec4"));
        assert!(header.contains("quanta_mat4"));
        assert!(header.contains("quanta_vec3_new"));
        assert!(header.contains("quanta_dot3"));
        assert!(header.contains("quanta_length3"));
        assert!(header.contains("quanta_normalize3"));
        assert!(header.contains("quanta_cross"));
        assert!(header.contains("quanta_vec3_add"));
        assert!(header.contains("quanta_vec3_sub"));
        assert!(header.contains("quanta_vec3_mul"));
        assert!(header.contains("quanta_vec3_scale"));
        assert!(header.contains("quanta_vec3_neg"));
    }

    #[test]
    fn test_vector_builtin_lookup() {
        assert_eq!(math_builtin_to_c("dot"), Some("quanta_dot3"));
        assert_eq!(math_builtin_to_c("cross"), Some("quanta_cross"));
        assert_eq!(math_builtin_to_c("normalize"), Some("quanta_normalize3"));
        assert_eq!(math_builtin_to_c("length"), Some("quanta_length3"));
        assert_eq!(math_builtin_to_c("reflect"), Some("quanta_reflect3"));
        assert_eq!(math_builtin_to_c("lerp"), Some("quanta_lerp3"));
    }

    #[test]
    fn test_mat4_builtin_lookup() {
        assert_eq!(math_builtin_to_c("mat4_identity"), Some("quanta_mat4_identity"));
        assert_eq!(math_builtin_to_c("mat4_translate"), Some("quanta_mat4_translate"));
        assert_eq!(math_builtin_to_c("mat4_scale"), Some("quanta_mat4_scale"));
        assert_eq!(math_builtin_to_c("mat4_perspective"), Some("quanta_mat4_perspective"));
    }

    #[test]
    fn test_shader_math_builtin_lookup() {
        assert_eq!(math_builtin_to_c("clamp"), Some("quanta_clampf"));
        assert_eq!(math_builtin_to_c("smoothstep"), Some("quanta_smoothstep"));
        assert_eq!(math_builtin_to_c("mix"), Some("quanta_mix"));
        assert_eq!(math_builtin_to_c("fract"), Some("quanta_fract"));
        assert_eq!(math_builtin_to_c("step"), Some("quanta_step"));
    }

    #[test]
    fn test_runtime_header_contains_mat4_ops() {
        let header = runtime_header();
        assert!(header.contains("quanta_mat4_identity"));
        assert!(header.contains("quanta_mat4_mul"));
        assert!(header.contains("quanta_mat4_mul_vec4"));
        assert!(header.contains("quanta_mat4_translate"));
        assert!(header.contains("quanta_mat4_scale"));
        assert!(header.contains("quanta_mat4_perspective"));
    }

    #[test]
    fn test_runtime_header_contains_shader_math() {
        let header = runtime_header();
        assert!(header.contains("quanta_clampf"));
        assert!(header.contains("quanta_clamp3"));
        assert!(header.contains("quanta_smoothstep"));
        assert!(header.contains("quanta_mix"));
        assert!(header.contains("quanta_fract"));
        assert!(header.contains("quanta_step"));
    }

    #[test]
    fn test_io_builtin_lookup() {
        assert_eq!(math_builtin_to_c("read_file"), Some("quanta_read_file"));
        assert_eq!(math_builtin_to_c("write_file"), Some("quanta_write_file"));
        assert_eq!(math_builtin_to_c("file_exists"), Some("quanta_file_exists"));
        assert_eq!(math_builtin_to_c("exit"), Some("quanta_exit"));
    }

    #[test]
    fn test_runtime_header_contains_graphics_stub() {
        let header = runtime_header();
        assert!(header.contains("QuantaGfxContext"));
        assert!(header.contains("quanta_gfx_init"));
        assert!(header.contains("quanta_gfx_load_shader"));
        assert!(header.contains("quanta_gfx_create_pipeline"));
        assert!(header.contains("quanta_gfx_begin_frame"));
        assert!(header.contains("quanta_gfx_clear"));
        assert!(header.contains("quanta_gfx_draw"));
        assert!(header.contains("quanta_gfx_end_frame"));
        assert!(header.contains("quanta_gfx_should_close"));
        assert!(header.contains("quanta_gfx_shutdown"));
    }
}
