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

// --- I/O initialization ---
static void __quanta_init_io(void) {
    setvbuf(stdout, NULL, _IONBF, 0);
    setvbuf(stderr, NULL, _IONBF, 0);
}

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

// f64 handle variants
static QuantaVecHandle quanta_hvec_new_f64(void) {
    QuantaVecHandle h;
    h.inner = (QuantaVec*)malloc(sizeof(QuantaVec));
    *h.inner = quanta_vec_new(sizeof(double));
    return h;
}
static void quanta_hvec_push_f64(QuantaVecHandle h, double val) { quanta_vec_push(h.inner, &val); }
static double quanta_hvec_get_f64(QuantaVecHandle h, size_t index) { return *(double*)quanta_vec_get(h.inner, index); }
static double quanta_hvec_pop_f64(QuantaVecHandle h) {
    if (h.inner->len == 0) return 0.0;
    h.inner->len--;
    return *(double*)((char*)h.inner->ptr + h.inner->len * h.inner->elem_size);
}
// i64 handle variants
static QuantaVecHandle quanta_hvec_new_i64(void) {
    QuantaVecHandle h;
    h.inner = (QuantaVec*)malloc(sizeof(QuantaVec));
    *h.inner = quanta_vec_new(sizeof(int64_t));
    return h;
}
static void quanta_hvec_push_i64(QuantaVecHandle h, int64_t val) { quanta_vec_push(h.inner, &val); }
static int64_t quanta_hvec_get_i64(QuantaVecHandle h, size_t index) { return *(int64_t*)quanta_vec_get(h.inner, index); }
static int64_t quanta_hvec_pop_i64(QuantaVecHandle h) {
    if (h.inner->len == 0) return 0;
    h.inner->len--;
    return *(int64_t*)((char*)h.inner->ptr + h.inner->len * h.inner->elem_size);
}

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

// --- Typed HashMap: string key -> f64 value (open addressing, linear probing) ---

typedef struct {
    char** keys;       // heap-allocated copies of key strings
    double* values;
    uint8_t* occupied;
    size_t cap;
    size_t len;
} QuantaStrF64Map;

typedef struct { QuantaStrF64Map* inner; } QuantaStrF64MapHandle;

static uint32_t __quanta_hash_str(const char* s) {
    uint32_t h = 2166136261u;
    for (; *s; s++) { h ^= (uint8_t)*s; h *= 16777619u; }
    return h;
}

static QuantaStrF64MapHandle quanta_hmap_new_str_f64(void) {
    QuantaStrF64MapHandle h;
    h.inner = (QuantaStrF64Map*)malloc(sizeof(QuantaStrF64Map));
    h.inner->cap = 16;
    h.inner->len = 0;
    h.inner->keys = (char**)calloc(16, sizeof(char*));
    h.inner->values = (double*)calloc(16, sizeof(double));
    h.inner->occupied = (uint8_t*)calloc(16, sizeof(uint8_t));
    return h;
}

static void __quanta_hmap_grow_str_f64(QuantaStrF64Map* m) {
    size_t old_cap = m->cap;
    char** old_keys = m->keys;
    double* old_values = m->values;
    uint8_t* old_occ = m->occupied;
    m->cap *= 2;
    m->len = 0;
    m->keys = (char**)calloc(m->cap, sizeof(char*));
    m->values = (double*)calloc(m->cap, sizeof(double));
    m->occupied = (uint8_t*)calloc(m->cap, sizeof(uint8_t));
    for (size_t i = 0; i < old_cap; i++) {
        if (old_occ[i]) {
            uint32_t hash = __quanta_hash_str(old_keys[i]);
            size_t idx = hash % m->cap;
            while (m->occupied[idx]) idx = (idx + 1) % m->cap;
            m->keys[idx] = old_keys[i]; // transfer ownership
            m->values[idx] = old_values[i];
            m->occupied[idx] = 1;
            m->len++;
        }
    }
    free(old_keys); free(old_values); free(old_occ);
}

static void quanta_hmap_insert_str_f64(QuantaStrF64MapHandle h, const char* key, double value) {
    QuantaStrF64Map* m = h.inner;
    if (m->len * 4 >= m->cap * 3) __quanta_hmap_grow_str_f64(m);
    uint32_t hash = __quanta_hash_str(key);
    size_t idx = hash % m->cap;
    while (m->occupied[idx]) {
        if (strcmp(m->keys[idx], key) == 0) { m->values[idx] = value; return; }
        idx = (idx + 1) % m->cap;
    }
    size_t klen = strlen(key);
    m->keys[idx] = (char*)malloc(klen + 1);
    memcpy(m->keys[idx], key, klen + 1);
    m->values[idx] = value;
    m->occupied[idx] = 1;
    m->len++;
}

static double quanta_hmap_get_str_f64(QuantaStrF64MapHandle h, const char* key) {
    QuantaStrF64Map* m = h.inner;
    uint32_t hash = __quanta_hash_str(key);
    size_t idx = hash % m->cap;
    size_t start = idx;
    while (m->occupied[idx]) {
        if (strcmp(m->keys[idx], key) == 0) return m->values[idx];
        idx = (idx + 1) % m->cap;
        if (idx == start) break;
    }
    return 0.0;
}

static bool quanta_hmap_contains_str_f64(QuantaStrF64MapHandle h, const char* key) {
    QuantaStrF64Map* m = h.inner;
    uint32_t hash = __quanta_hash_str(key);
    size_t idx = hash % m->cap;
    size_t start = idx;
    while (m->occupied[idx]) {
        if (strcmp(m->keys[idx], key) == 0) return true;
        idx = (idx + 1) % m->cap;
        if (idx == start) break;
    }
    return false;
}

static bool quanta_hmap_remove_str_f64(QuantaStrF64MapHandle h, const char* key) {
    QuantaStrF64Map* m = h.inner;
    uint32_t hash = __quanta_hash_str(key);
    size_t idx = hash % m->cap;
    size_t start = idx;
    while (m->occupied[idx]) {
        if (strcmp(m->keys[idx], key) == 0) {
            free(m->keys[idx]);
            m->keys[idx] = NULL;
            m->occupied[idx] = 0;
            m->len--;
            return true;
        }
        idx = (idx + 1) % m->cap;
        if (idx == start) break;
    }
    return false;
}

static size_t quanta_hmap_len_str_f64(QuantaStrF64MapHandle h) { return h.inner->len; }

// --- Typed HashMap: i64 key -> f64 value ---

typedef struct {
    int64_t* keys;
    double* values;
    uint8_t* occupied;
    size_t cap;
    size_t len;
} QuantaI64F64Map;

typedef struct { QuantaI64F64Map* inner; } QuantaI64F64MapHandle;

static QuantaI64F64MapHandle quanta_hmap_new_i64_f64(void) {
    QuantaI64F64MapHandle h;
    h.inner = (QuantaI64F64Map*)malloc(sizeof(QuantaI64F64Map));
    h.inner->cap = 16;
    h.inner->len = 0;
    h.inner->keys = (int64_t*)calloc(16, sizeof(int64_t));
    h.inner->values = (double*)calloc(16, sizeof(double));
    h.inner->occupied = (uint8_t*)calloc(16, sizeof(uint8_t));
    return h;
}

static void __quanta_hmap_grow_i64_f64(QuantaI64F64Map* m) {
    size_t old_cap = m->cap;
    int64_t* old_keys = m->keys;
    double* old_values = m->values;
    uint8_t* old_occ = m->occupied;
    m->cap *= 2;
    m->len = 0;
    m->keys = (int64_t*)calloc(m->cap, sizeof(int64_t));
    m->values = (double*)calloc(m->cap, sizeof(double));
    m->occupied = (uint8_t*)calloc(m->cap, sizeof(uint8_t));
    for (size_t i = 0; i < old_cap; i++) {
        if (old_occ[i]) {
            uint32_t hash = (uint32_t)((uint64_t)old_keys[i] * 2654435761u);
            size_t idx = hash % m->cap;
            while (m->occupied[idx]) idx = (idx + 1) % m->cap;
            m->keys[idx] = old_keys[i];
            m->values[idx] = old_values[i];
            m->occupied[idx] = 1;
            m->len++;
        }
    }
    free(old_keys); free(old_values); free(old_occ);
}

static void quanta_hmap_insert_i64_f64(QuantaI64F64MapHandle h, int64_t key, double value) {
    QuantaI64F64Map* m = h.inner;
    if (m->len * 4 >= m->cap * 3) __quanta_hmap_grow_i64_f64(m);
    uint32_t hash = (uint32_t)((uint64_t)key * 2654435761u);
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

static double quanta_hmap_get_i64_f64(QuantaI64F64MapHandle h, int64_t key) {
    QuantaI64F64Map* m = h.inner;
    uint32_t hash = (uint32_t)((uint64_t)key * 2654435761u);
    size_t idx = hash % m->cap;
    size_t start = idx;
    while (m->occupied[idx]) {
        if (m->keys[idx] == key) return m->values[idx];
        idx = (idx + 1) % m->cap;
        if (idx == start) break;
    }
    return 0.0;
}

static bool quanta_hmap_contains_i64_f64(QuantaI64F64MapHandle h, int64_t key) {
    QuantaI64F64Map* m = h.inner;
    uint32_t hash = (uint32_t)((uint64_t)key * 2654435761u);
    size_t idx = hash % m->cap;
    size_t start = idx;
    while (m->occupied[idx]) {
        if (m->keys[idx] == key) return true;
        idx = (idx + 1) % m->cap;
        if (idx == start) break;
    }
    return false;
}

static bool quanta_hmap_remove_i64_f64(QuantaI64F64MapHandle h, int64_t key) {
    QuantaI64F64Map* m = h.inner;
    uint32_t hash = (uint32_t)((uint64_t)key * 2654435761u);
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

static size_t quanta_hmap_len_i64_f64(QuantaI64F64MapHandle h) { return h.inner->len; }

// --- Print formatting helpers ---
// Use snprintf + __quanta_write for reliable output under MinTTY/git-bash.

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
static double quanta_saturate(double x) { return x < 0.0 ? 0.0 : (x > 1.0 ? 1.0 : x); }

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

// --- Binary file I/O ---

static bool quanta_write_bytes(const char* path, const char* data, int64_t len) {
    FILE* f = fopen(path, "wb");
    if (!f) return false;
    fwrite(data, 1, (size_t)len, f);
    fclose(f);
    return true;
}

static QuantaString quanta_read_bytes(const char* path) {
    FILE* f = fopen(path, "rb");
    if (!f) return quanta_string_new("");
    fseek(f, 0, SEEK_END);
    long sz = ftell(f);
    fseek(f, 0, SEEK_SET);
    char* buf = (char*)malloc(sz + 1);
    fread(buf, 1, sz, f);
    buf[sz] = 0;
    fclose(f);
    QuantaString s;
    s.ptr = buf;
    s.len = (int64_t)sz;
    s.cap = (int64_t)sz + 1;
    return s;
}

static bool quanta_append_file(const char* path, const char* data) {
    FILE* f = fopen(path, "ab");
    if (!f) return false;
    size_t len = strlen(data);
    fwrite(data, 1, len, f);
    fclose(f);
    return true;
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

// --- String method helpers ---

static bool quanta_string_is_empty(QuantaString s) {
    return s.len == 0;
}

static bool quanta_string_starts_with(QuantaString s, QuantaString prefix) {
    if (prefix.len > s.len) return false;
    return memcmp(s.ptr, prefix.ptr, prefix.len) == 0;
}

static bool quanta_string_ends_with(QuantaString s, QuantaString suffix) {
    if (suffix.len > s.len) return false;
    return memcmp(s.ptr + s.len - suffix.len, suffix.ptr, suffix.len) == 0;
}

static bool quanta_string_contains(QuantaString s, QuantaString substr) {
    if (substr.len == 0) return true;
    if (substr.len > s.len) return false;
    for (size_t i = 0; i <= s.len - substr.len; i++) {
        if (memcmp(s.ptr + i, substr.ptr, substr.len) == 0) return true;
    }
    return false;
}

static QuantaString quanta_string_to_upper(QuantaString s) {
    char* buf = (char*)malloc(s.len + 1);
    for (size_t i = 0; i < s.len; i++) buf[i] = (char)toupper((unsigned char)s.ptr[i]);
    buf[s.len] = '\0';
    QuantaString qs; qs.ptr = buf; qs.len = s.len; qs.cap = s.len + 1;
    return qs;
}

static QuantaString quanta_string_to_lower(QuantaString s) {
    char* buf = (char*)malloc(s.len + 1);
    for (size_t i = 0; i < s.len; i++) buf[i] = (char)tolower((unsigned char)s.ptr[i]);
    buf[s.len] = '\0';
    QuantaString qs; qs.ptr = buf; qs.len = s.len; qs.cap = s.len + 1;
    return qs;
}

static QuantaString quanta_string_trim(QuantaString s) {
    size_t start = 0;
    while (start < s.len && isspace((unsigned char)s.ptr[start])) start++;
    size_t end = s.len;
    while (end > start && isspace((unsigned char)s.ptr[end - 1])) end--;
    size_t new_len = end - start;
    char* buf = (char*)malloc(new_len + 1);
    memcpy(buf, s.ptr + start, new_len);
    buf[new_len] = '\0';
    QuantaString qs; qs.ptr = buf; qs.len = new_len; qs.cap = new_len + 1;
    return qs;
}

static QuantaVec quanta_string_split(QuantaString s, QuantaString delim) {
    QuantaVec v = quanta_vec_new(sizeof(QuantaString));
    if (delim.len == 0) {
        quanta_vec_push(&v, &s);
        return v;
    }
    size_t start = 0;
    for (size_t i = 0; i <= s.len - delim.len; i++) {
        if (memcmp(s.ptr + i, delim.ptr, delim.len) == 0) {
            size_t part_len = i - start;
            char* buf = (char*)malloc(part_len + 1);
            memcpy(buf, s.ptr + start, part_len);
            buf[part_len] = '\0';
            QuantaString part; part.ptr = buf; part.len = part_len; part.cap = part_len + 1;
            quanta_vec_push(&v, &part);
            start = i + delim.len;
            i = start - 1; // loop will increment
        }
    }
    // trailing part
    size_t part_len = s.len - start;
    char* buf = (char*)malloc(part_len + 1);
    memcpy(buf, s.ptr + start, part_len);
    buf[part_len] = '\0';
    QuantaString part; part.ptr = buf; part.len = part_len; part.cap = part_len + 1;
    quanta_vec_push(&v, &part);
    return v;
}

static QuantaVec quanta_string_split_ws(QuantaString s) {
    QuantaVec v = quanta_vec_new(sizeof(QuantaString));
    size_t i = 0;
    while (i < s.len) {
        while (i < s.len && isspace((unsigned char)s.ptr[i])) i++;
        if (i >= s.len) break;
        size_t start = i;
        while (i < s.len && !isspace((unsigned char)s.ptr[i])) i++;
        size_t part_len = i - start;
        char* buf = (char*)malloc(part_len + 1);
        memcpy(buf, s.ptr + start, part_len);
        buf[part_len] = '\0';
        QuantaString part; part.ptr = buf; part.len = part_len; part.cap = part_len + 1;
        quanta_vec_push(&v, &part);
    }
    return v;
}

static QuantaVec quanta_string_lines(QuantaString s) {
    QuantaString delim; delim.ptr = "\n"; delim.len = 1; delim.cap = 0;
    return quanta_string_split(s, delim);
}

// --- Command-line arguments ---

#ifdef _WIN32
#include <io.h>
#define QUANTA_ISATTY _isatty
#define QUANTA_FILENO _fileno
#else
#include <unistd.h>
#define QUANTA_ISATTY isatty
#define QUANTA_FILENO fileno
#endif

static QuantaVec QUANTA_ARGS;
static int QUANTA_ARGC = 0;

static QuantaString quanta_string_from_cstr(const char* s) {
    size_t len = strlen(s);
    char* buf = (char*)malloc(len + 1);
    memcpy(buf, s, len + 1);
    QuantaString qs; qs.ptr = buf; qs.len = len; qs.cap = len + 1;
    return qs;
}

static void quanta_args_init(int argc, char** argv) {
    QUANTA_ARGC = argc;
    QUANTA_ARGS = quanta_vec_new(sizeof(QuantaString));
    for (int i = 0; i < argc; i++) {
        QuantaString s = quanta_string_from_cstr(argv[i]);
        quanta_vec_push(&QUANTA_ARGS, &s);
    }
}

static int64_t quanta_args_count(void) {
    return (int64_t)QUANTA_ARGC;
}

static QuantaString quanta_args_get(int64_t index) {
    if (index < 0 || index >= QUANTA_ARGC) {
        return quanta_string_new("");
    }
    return *(QuantaString*)quanta_vec_get(&QUANTA_ARGS, (size_t)index);
}

// --- Stdin reading ---

static QuantaString quanta_read_line(void) {
    char buf[4096];
    if (fgets(buf, sizeof(buf), stdin) == NULL) {
        return quanta_string_new("");
    }
    return quanta_string_from_cstr(buf);
}

static QuantaString quanta_read_all(void) {
    size_t cap = 4096;
    size_t len = 0;
    char* buf = (char*)malloc(cap);
    while (1) {
        size_t n = fread(buf + len, 1, cap - len, stdin);
        len += n;
        if (n == 0) break;
        if (len >= cap) {
            cap *= 2;
            buf = (char*)realloc(buf, cap);
        }
    }
    buf[len] = '\0';
    QuantaString qs; qs.ptr = buf; qs.len = len; qs.cap = cap;
    return qs;
}

static bool quanta_stdin_is_pipe(void) {
    return !QUANTA_ISATTY(QUANTA_FILENO(stdin));
}

// --- Extended string operations ---

static int64_t quanta_string_parse_int(QuantaString s) {
    return (int64_t)atoll(s.ptr);
}

static double quanta_string_parse_float(QuantaString s) {
    return atof(s.ptr);
}

static QuantaString quanta_string_char_at(QuantaString s, int64_t idx) {
    if (idx < 0 || (size_t)idx >= s.len) {
        return quanta_string_new("");
    }
    char* buf = (char*)malloc(2);
    buf[0] = s.ptr[idx];
    buf[1] = '\0';
    QuantaString qs; qs.ptr = buf; qs.len = 1; qs.cap = 2;
    return qs;
}

static QuantaString quanta_string_substring(QuantaString s, int64_t start, int64_t slen) {
    if (start < 0) start = 0;
    if ((size_t)start >= s.len || slen <= 0) {
        return quanta_string_from_cstr("");
    }
    size_t actual = (size_t)slen;
    if ((size_t)start + actual > s.len) actual = s.len - (size_t)start;
    char* buf = (char*)malloc(actual + 1);
    memcpy(buf, s.ptr + start, actual);
    buf[actual] = '\0';
    QuantaString qs; qs.ptr = buf; qs.len = actual; qs.cap = actual + 1;
    return qs;
}

static QuantaString quanta_string_replace(QuantaString s, QuantaString old_s, QuantaString new_s) {
    if (old_s.len == 0) {
        return quanta_string_from_cstr(s.ptr);
    }
    // Count occurrences
    size_t count = 0;
    const char* p = s.ptr;
    while ((p = strstr(p, old_s.ptr)) != NULL) {
        count++;
        p += old_s.len;
    }
    if (count == 0) {
        return quanta_string_from_cstr(s.ptr);
    }
    size_t new_len = s.len + count * (new_s.len - old_s.len);
    char* buf = (char*)malloc(new_len + 1);
    char* dst = buf;
    const char* src = s.ptr;
    while ((p = strstr(src, old_s.ptr)) != NULL) {
        size_t chunk = (size_t)(p - src);
        memcpy(dst, src, chunk);
        dst += chunk;
        memcpy(dst, new_s.ptr, new_s.len);
        dst += new_s.len;
        src = p + old_s.len;
    }
    // Copy remainder
    size_t rem = s.len - (size_t)(src - s.ptr);
    memcpy(dst, src, rem);
    dst[rem] = '\0';
    QuantaString qs; qs.ptr = buf; qs.len = new_len; qs.cap = new_len + 1;
    return qs;
}

static QuantaString quanta_string_repeat(QuantaString s, int64_t count) {
    if (count <= 0 || s.len == 0) {
        return quanta_string_from_cstr("");
    }
    size_t new_len = s.len * (size_t)count;
    char* buf = (char*)malloc(new_len + 1);
    for (int64_t i = 0; i < count; i++) {
        memcpy(buf + i * s.len, s.ptr, s.len);
    }
    buf[new_len] = '\0';
    QuantaString qs; qs.ptr = buf; qs.len = new_len; qs.cap = new_len + 1;
    return qs;
}

static int64_t quanta_string_index_of(QuantaString s, QuantaString substr) {
    if (substr.len == 0) return 0;
    if (substr.len > s.len) return -1;
    const char* p = strstr(s.ptr, substr.ptr);
    if (p == NULL) return -1;
    return (int64_t)(p - s.ptr);
}

static int64_t quanta_string_compare(QuantaString a, QuantaString b) {
    int c = strcmp(a.ptr, b.ptr);
    return (int64_t)(c < 0 ? -1 : (c > 0 ? 1 : 0));
}

// --- Process exit ---

static void quanta_process_exit(int64_t code) {
    exit((int)code);
}

// --- stderr output helper ---

static void quanta_eprint_str(const char* v) { fprintf(stderr, "%s", v); }
static void quanta_eprint_string(QuantaString v) { fprintf(stderr, "%.*s", (int)v.len, v.ptr); }

// --- stdout initialization ---
// Disable output buffering so printf output is visible immediately,
// especially when running as a child process (e.g., quantac run).
#ifdef _MSC_VER
#pragma section(".CRT$XCU", read)
static void __quanta_init_stdio(void) { setvbuf(stdout, NULL, _IONBF, 0); }
__declspec(allocate(".CRT$XCU")) static void (*__quanta_init_ptr)(void) = __quanta_init_stdio;
#else
__attribute__((constructor)) static void __quanta_init_stdio(void) { setvbuf(stdout, NULL, _IONBF, 0); }
#endif

// --- String Vec handle variants ---

static QuantaVecHandle quanta_hvec_new_str(void) {
    QuantaVecHandle h;
    h.inner = (QuantaVec*)malloc(sizeof(QuantaVec));
    *h.inner = quanta_vec_new(sizeof(QuantaString));
    return h;
}
static void quanta_hvec_push_str(QuantaVecHandle h, QuantaString val) { quanta_vec_push(h.inner, &val); }
static QuantaString quanta_hvec_get_str(QuantaVecHandle h, size_t index) { return *(QuantaString*)quanta_vec_get(h.inner, index); }

// --- TCP socket support (includes must come before windows.h) ---

#ifdef _WIN32
#include <winsock2.h>
#include <ws2tcpip.h>
#pragma comment(lib, "ws2_32.lib")
#include <windows.h>
#else
#include <sys/socket.h>
#include <netinet/in.h>
#include <netdb.h>
#include <unistd.h>
#define closesocket close
#endif

// --- Directory traversal ---

#ifdef _WIN32
// windows.h already included above
#else
#include <dirent.h>
#include <sys/stat.h>
#endif

static QuantaVecHandle quanta_list_dir(const char* path) {
    QuantaVecHandle v = quanta_hvec_new_str();
    #ifdef _WIN32
    char search_path[1024];
    snprintf(search_path, sizeof(search_path), "%s\\*", path);
    WIN32_FIND_DATAA fd;
    HANDLE h = FindFirstFileA(search_path, &fd);
    if (h != INVALID_HANDLE_VALUE) {
        do {
            if (strcmp(fd.cFileName, ".") != 0 && strcmp(fd.cFileName, "..") != 0) {
                quanta_hvec_push_str(v, quanta_string_from_cstr(fd.cFileName));
            }
        } while (FindNextFileA(h, &fd));
        FindClose(h);
    }
    #else
    DIR* d = opendir(path);
    if (d) {
        struct dirent* ent;
        while ((ent = readdir(d)) != NULL) {
            if (strcmp(ent->d_name, ".") != 0 && strcmp(ent->d_name, "..") != 0) {
                quanta_hvec_push_str(v, quanta_string_from_cstr(ent->d_name));
            }
        }
        closedir(d);
    }
    #endif
    return v;
}

static bool quanta_is_dir(const char* path) {
    #ifdef _WIN32
    DWORD attr = GetFileAttributesA(path);
    return (attr != INVALID_FILE_ATTRIBUTES && (attr & FILE_ATTRIBUTE_DIRECTORY));
    #else
    struct stat st;
    return (stat(path, &st) == 0 && S_ISDIR(st.st_mode));
    #endif
}

static int64_t quanta_file_size(const char* path) {
    #ifdef _WIN32
    WIN32_FILE_ATTRIBUTE_DATA fad;
    if (GetFileAttributesExA(path, GetFileExInfoStandard, &fad)) {
        return ((int64_t)fad.nFileSizeHigh << 32) | fad.nFileSizeLow;
    }
    return -1;
    #else
    struct stat st;
    return (stat(path, &st) == 0) ? st.st_size : -1;
    #endif
}

// --- TCP socket functions ---

#ifdef _WIN32
static void quanta_net_init(void) {
    static int initialized = 0;
    if (!initialized) {
        WSADATA wsa;
        WSAStartup(MAKEWORD(2, 2), &wsa);
        initialized = 1;
    }
}
#else
static void quanta_net_init(void) {}
#endif

// Connect to host:port, returns socket fd (-1 on error)
static int64_t quanta_tcp_connect(const char* host, int64_t port) {
    quanta_net_init();
    struct addrinfo hints, *res;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;

    char port_str[16];
    snprintf(port_str, sizeof(port_str), "%lld", (long long)port);

    if (getaddrinfo(host, port_str, &hints, &res) != 0) return -1;

    int64_t sock = (int64_t)socket(res->ai_family, res->ai_socktype, res->ai_protocol);
    if (sock < 0) { freeaddrinfo(res); return -1; }

    if (connect((int)sock, res->ai_addr, (int)res->ai_addrlen) != 0) {
        closesocket((int)sock);
        freeaddrinfo(res);
        return -1;
    }
    freeaddrinfo(res);
    return sock;
}

// Send data on socket
static int64_t quanta_tcp_send(int64_t sock, const char* data) {
    return send((int)sock, data, (int)strlen(data), 0);
}

// Receive data from socket (up to 64KB)
static QuantaString quanta_tcp_recv(int64_t sock) {
    char buf[65536];
    int total = 0;
    int n;
    while (total < 65000) {
        n = recv((int)sock, buf + total, sizeof(buf) - total - 1, 0);
        if (n <= 0) break;
        total += n;
    }
    buf[total] = '\0';
    return quanta_string_from_cstr(buf);
}

// Close socket
static void quanta_tcp_close(int64_t sock) {
    closesocket((int)sock);
}

// --- Environment variable access ---

static QuantaString quanta_getenv(const char* name) {
    const char* val = getenv(name);
    if (val == NULL) return quanta_string_from_cstr("");
    return quanta_string_from_cstr(val);
}

// --- Clock / time builtins ---

static int64_t quanta_clock_ms(void) {
    #ifdef _WIN32
    LARGE_INTEGER freq, count;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&count);
    return (int64_t)(count.QuadPart * 1000 / freq.QuadPart);
    #else
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
    #endif
}

static int64_t quanta_time_unix(void) {
    return (int64_t)time(NULL);
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
    "read_bytes", "write_bytes", "append_file",
    // Vector math builtins
    "dot", "cross", "normalize", "length", "reflect", "lerp",
    // Mat4 builtins
    "mat4_identity", "mat4_translate", "mat4_scale", "mat4_perspective",
    // Shader math builtins
    "clamp", "smoothstep", "mix", "fract", "step", "saturate",
    // Vec builtins
    "vec_new", "vec_push", "vec_get", "vec_len", "vec_pop",
    "vec_new_f64", "vec_push_f64", "vec_get_f64", "vec_pop_f64",
    "vec_new_i64", "vec_push_i64", "vec_get_i64", "vec_pop_i64",
    // Format builtins
    "to_string_i32", "to_string_f64",
    // HashMap builtins (default str->f64)
    "map_new", "map_insert", "map_get", "map_contains", "map_len", "map_remove",
    // HashMap builtins (legacy i32->i32)
    "map_new_i32", "map_insert_i32", "map_get_i32", "map_contains_i32", "map_len_i32", "map_remove_i32",
    // HashMap builtins (i64->f64)
    "map_new_i64", "map_insert_i64", "map_get_i64", "map_contains_i64", "map_len_i64", "map_remove_i64",
    // Vulkan runtime builtins
    "quanta_vk_init", "quanta_vk_load_shader_file", "quanta_vk_run_compute",
    "quanta_vk_shutdown", "quanta_vk_create_graphics_pipeline",
    "quanta_vk_set_push_constant_f32", "quanta_vk_draw_frame",
    "quanta_vk_should_close", "quanta_vk_request_close", "quanta_vk_device_name",
    // CLI / stdin builtins
    "args_count", "args_get",
    "read_line", "read_all", "stdin_is_pipe",
    // Process builtins
    "process_exit",
    // Directory traversal builtins
    "list_dir", "is_dir", "file_size",
    // String vec builtins
    "vec_new_str", "vec_push_str", "vec_get_str",
    // TCP socket builtins
    "tcp_connect", "tcp_send", "tcp_recv", "tcp_close",
    // Environment variable builtins
    "getenv",
    // Clock / time builtins
    "clock_ms", "time_unix",
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
        "read_bytes"  => Some("quanta_read_bytes"),
        "write_bytes" => Some("quanta_write_bytes"),
        "append_file" => Some("quanta_append_file"),
        "vec_new"     => Some("quanta_hvec_new_i32"),
        "vec_push"    => Some("quanta_hvec_push_i32"),
        "vec_get"     => Some("quanta_hvec_get_i32"),
        "vec_len"     => Some("quanta_hvec_len"),
        "vec_pop"     => Some("quanta_hvec_pop_i32"),
        "vec_new_f64"  => Some("quanta_hvec_new_f64"),
        "vec_push_f64" => Some("quanta_hvec_push_f64"),
        "vec_get_f64"  => Some("quanta_hvec_get_f64"),
        "vec_pop_f64"  => Some("quanta_hvec_pop_f64"),
        "vec_new_i64"  => Some("quanta_hvec_new_i64"),
        "vec_push_i64" => Some("quanta_hvec_push_i64"),
        "vec_get_i64"  => Some("quanta_hvec_get_i64"),
        "vec_pop_i64"  => Some("quanta_hvec_pop_i64"),
        "to_string_i32" => Some("quanta_i32_to_string"),
        "to_string_f64" => Some("quanta_f64_to_string"),
        // HashMap builtins (legacy i32->i32)
        "map_new_i32"      => Some("quanta_map_new"),
        "map_insert_i32"   => Some("quanta_map_insert"),
        "map_get_i32"      => Some("quanta_map_get"),
        "map_contains_i32" => Some("quanta_map_contains"),
        "map_len_i32"      => Some("quanta_map_len"),
        "map_remove_i32"   => Some("quanta_map_remove"),
        // HashMap builtins — default to str->f64 (most common use case)
        "map_new"      => Some("quanta_hmap_new_str_f64"),
        "map_insert"   => Some("quanta_hmap_insert_str_f64"),
        "map_get"      => Some("quanta_hmap_get_str_f64"),
        "map_contains" => Some("quanta_hmap_contains_str_f64"),
        "map_len"      => Some("quanta_hmap_len_str_f64"),
        "map_remove"   => Some("quanta_hmap_remove_str_f64"),
        // HashMap builtins (i64->f64)
        "map_new_i64"      => Some("quanta_hmap_new_i64_f64"),
        "map_insert_i64"   => Some("quanta_hmap_insert_i64_f64"),
        "map_get_i64"      => Some("quanta_hmap_get_i64_f64"),
        "map_contains_i64" => Some("quanta_hmap_contains_i64_f64"),
        "map_len_i64"      => Some("quanta_hmap_len_i64_f64"),
        "map_remove_i64"   => Some("quanta_hmap_remove_i64_f64"),
        // Vulkan runtime builtins
        "quanta_vk_init" => Some("quanta_vk_init"),
        "quanta_vk_load_shader_file" => Some("quanta_vk_load_shader_file"),
        "quanta_vk_run_compute" => Some("quanta_vk_run_compute"),
        "quanta_vk_shutdown" => Some("quanta_vk_shutdown"),
        "quanta_vk_create_graphics_pipeline" => Some("quanta_vk_create_graphics_pipeline"),
        "quanta_vk_set_push_constant_f32" => Some("quanta_vk_set_push_constant_f32"),
        "quanta_vk_draw_frame" => Some("quanta_vk_draw_frame"),
        "quanta_vk_should_close" => Some("quanta_vk_should_close"),
        "quanta_vk_request_close" => Some("quanta_vk_request_close"),
        "quanta_vk_device_name" => Some("quanta_vk_device_name"),
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
        "saturate"   => Some("quanta_saturate"),
        "smoothstep" => Some("quanta_smoothstep"),
        "mix"        => Some("quanta_mix"),
        "fract"      => Some("quanta_fract"),
        "step"       => Some("quanta_step"),
        // CLI / stdin builtins
        "args_count"   => Some("quanta_args_count"),
        "args_get"     => Some("quanta_args_get"),
        "read_line"    => Some("quanta_read_line"),
        "read_all"     => Some("quanta_read_all"),
        "stdin_is_pipe" => Some("quanta_stdin_is_pipe"),
        // Process builtins
        "process_exit" => Some("quanta_process_exit"),
        // Directory traversal builtins
        "list_dir"  => Some("quanta_list_dir"),
        "is_dir"    => Some("quanta_is_dir"),
        "file_size" => Some("quanta_file_size"),
        // String vec builtins
        "vec_new_str"  => Some("quanta_hvec_new_str"),
        "vec_push_str" => Some("quanta_hvec_push_str"),
        "vec_get_str"  => Some("quanta_hvec_get_str"),
        // TCP socket builtins
        "tcp_connect" => Some("quanta_tcp_connect"),
        "tcp_send"    => Some("quanta_tcp_send"),
        "tcp_recv"    => Some("quanta_tcp_recv"),
        "tcp_close"   => Some("quanta_tcp_close"),
        // Environment variable builtins
        "getenv" => Some("quanta_getenv"),
        // Clock / time builtins
        "clock_ms"  => Some("quanta_clock_ms"),
        "time_unix" => Some("quanta_time_unix"),
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
    fn test_runtime_header_contains_string_methods() {
        let header = runtime_header();
        assert!(header.contains("quanta_string_is_empty"));
        assert!(header.contains("quanta_string_starts_with"));
        assert!(header.contains("quanta_string_ends_with"));
        assert!(header.contains("quanta_string_contains"));
        assert!(header.contains("quanta_string_to_upper"));
        assert!(header.contains("quanta_string_to_lower"));
        assert!(header.contains("quanta_string_trim"));
        assert!(header.contains("quanta_string_split"));
        assert!(header.contains("quanta_string_split_ws"));
        assert!(header.contains("quanta_string_lines"));
    }

    #[test]
    fn test_runtime_header_contains_vec_type() {
        let header = runtime_header();
        assert!(header.contains("QuantaVec"));
        assert!(header.contains("quanta_vec_new"));
        assert!(header.contains("quanta_vec_push"));
        assert!(header.contains("quanta_vec_get"));
        assert!(header.contains("quanta_vec_free"));
        // f64/i64 handle variants
        assert!(header.contains("quanta_hvec_new_f64"));
        assert!(header.contains("quanta_hvec_push_f64"));
        assert!(header.contains("quanta_hvec_get_f64"));
        assert!(header.contains("quanta_hvec_new_i64"));
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
        assert_eq!(MATH_BUILTINS.len(), 96);
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
        assert!(header.contains("quanta_read_bytes"));
        assert!(header.contains("quanta_write_bytes"));
        assert!(header.contains("quanta_append_file"));
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
        assert_eq!(math_builtin_to_c("read_bytes"), Some("quanta_read_bytes"));
        assert_eq!(math_builtin_to_c("write_bytes"), Some("quanta_write_bytes"));
        assert_eq!(math_builtin_to_c("append_file"), Some("quanta_append_file"));
    }

    #[test]
    fn test_runtime_header_contains_typed_hashmap() {
        let header = runtime_header();
        // str->f64 typed hashmap
        assert!(header.contains("QuantaStrF64Map"));
        assert!(header.contains("QuantaStrF64MapHandle"));
        assert!(header.contains("quanta_hmap_new_str_f64"));
        assert!(header.contains("quanta_hmap_insert_str_f64"));
        assert!(header.contains("quanta_hmap_get_str_f64"));
        assert!(header.contains("quanta_hmap_contains_str_f64"));
        assert!(header.contains("quanta_hmap_remove_str_f64"));
        assert!(header.contains("quanta_hmap_len_str_f64"));
        // i64->f64 typed hashmap
        assert!(header.contains("QuantaI64F64Map"));
        assert!(header.contains("QuantaI64F64MapHandle"));
        assert!(header.contains("quanta_hmap_new_i64_f64"));
        assert!(header.contains("quanta_hmap_insert_i64_f64"));
        assert!(header.contains("quanta_hmap_get_i64_f64"));
        assert!(header.contains("quanta_hmap_contains_i64_f64"));
        assert!(header.contains("quanta_hmap_remove_i64_f64"));
        assert!(header.contains("quanta_hmap_len_i64_f64"));
    }

    #[test]
    fn test_hashmap_builtin_lookup() {
        // Default str->f64 builtins
        assert_eq!(math_builtin_to_c("map_new"), Some("quanta_hmap_new_str_f64"));
        assert_eq!(math_builtin_to_c("map_insert"), Some("quanta_hmap_insert_str_f64"));
        assert_eq!(math_builtin_to_c("map_get"), Some("quanta_hmap_get_str_f64"));
        assert_eq!(math_builtin_to_c("map_contains"), Some("quanta_hmap_contains_str_f64"));
        assert_eq!(math_builtin_to_c("map_remove"), Some("quanta_hmap_remove_str_f64"));
        assert_eq!(math_builtin_to_c("map_len"), Some("quanta_hmap_len_str_f64"));
        // i64->f64 builtins
        assert_eq!(math_builtin_to_c("map_new_i64"), Some("quanta_hmap_new_i64_f64"));
        assert_eq!(math_builtin_to_c("map_insert_i64"), Some("quanta_hmap_insert_i64_f64"));
        assert_eq!(math_builtin_to_c("map_get_i64"), Some("quanta_hmap_get_i64_f64"));
        // Legacy i32->i32 builtins
        assert_eq!(math_builtin_to_c("map_new_i32"), Some("quanta_map_new"));
        assert_eq!(math_builtin_to_c("map_insert_i32"), Some("quanta_map_insert"));
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

    #[test]
    fn test_runtime_header_contains_cli_args() {
        let header = runtime_header();
        assert!(header.contains("QUANTA_ARGS"));
        assert!(header.contains("quanta_args_init"));
        assert!(header.contains("quanta_args_count"));
        assert!(header.contains("quanta_args_get"));
        assert!(header.contains("quanta_string_from_cstr"));
    }

    #[test]
    fn test_runtime_header_contains_stdin() {
        let header = runtime_header();
        assert!(header.contains("quanta_read_line"));
        assert!(header.contains("quanta_read_all"));
        assert!(header.contains("quanta_stdin_is_pipe"));
    }

    #[test]
    fn test_runtime_header_contains_extended_string_ops() {
        let header = runtime_header();
        assert!(header.contains("quanta_string_parse_int"));
        assert!(header.contains("quanta_string_parse_float"));
        assert!(header.contains("quanta_string_char_at"));
        assert!(header.contains("quanta_string_substring"));
        assert!(header.contains("quanta_string_replace"));
        assert!(header.contains("quanta_string_repeat"));
        assert!(header.contains("quanta_string_index_of"));
        assert!(header.contains("quanta_string_compare"));
    }

    #[test]
    fn test_cli_builtin_lookup() {
        assert_eq!(math_builtin_to_c("args_count"), Some("quanta_args_count"));
        assert_eq!(math_builtin_to_c("args_get"), Some("quanta_args_get"));
        assert_eq!(math_builtin_to_c("read_line"), Some("quanta_read_line"));
        assert_eq!(math_builtin_to_c("read_all"), Some("quanta_read_all"));
        assert_eq!(math_builtin_to_c("stdin_is_pipe"), Some("quanta_stdin_is_pipe"));
        assert_eq!(math_builtin_to_c("process_exit"), Some("quanta_process_exit"));
    }

    #[test]
    fn test_runtime_header_contains_dir_traversal() {
        let header = runtime_header();
        assert!(header.contains("quanta_list_dir"));
        assert!(header.contains("quanta_is_dir"));
        assert!(header.contains("quanta_file_size"));
        assert!(header.contains("quanta_hvec_new_str"));
        assert!(header.contains("quanta_hvec_push_str"));
        assert!(header.contains("quanta_hvec_get_str"));
    }

    #[test]
    fn test_clock_builtin_lookup() {
        assert_eq!(math_builtin_to_c("clock_ms"), Some("quanta_clock_ms"));
        assert_eq!(math_builtin_to_c("time_unix"), Some("quanta_time_unix"));
    }

    #[test]
    fn test_runtime_header_contains_clock() {
        let header = runtime_header();
        assert!(header.contains("quanta_clock_ms"));
        assert!(header.contains("quanta_time_unix"));
    }

    #[test]
    fn test_dir_builtin_lookup() {
        assert_eq!(math_builtin_to_c("list_dir"), Some("quanta_list_dir"));
        assert_eq!(math_builtin_to_c("is_dir"), Some("quanta_is_dir"));
        assert_eq!(math_builtin_to_c("file_size"), Some("quanta_file_size"));
        assert_eq!(math_builtin_to_c("vec_new_str"), Some("quanta_hvec_new_str"));
        assert_eq!(math_builtin_to_c("vec_push_str"), Some("quanta_hvec_push_str"));
        assert_eq!(math_builtin_to_c("vec_get_str"), Some("quanta_hvec_get_str"));
    }
}
