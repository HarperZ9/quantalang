// ===============================================================================
// QUANTALANG RUNTIME - GARBAGE COLLECTOR
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Garbage collection support for QuantaLang.
//!
//! Implements a hybrid memory management strategy:
//! - Reference counting for most objects
//! - Cycle detection for cyclic data structures
//! - Optional tracing GC for specific scenarios
//!
//! ## Design Philosophy
//!
//! QuantaLang uses a "pay for what you use" memory model:
//! - Stack-allocated values by default
//! - Reference-counted heap allocations when needed
//! - Cycle detection runs periodically or on-demand
//! - Zero-cost for non-GC code paths

use std::alloc::{alloc, dealloc, Layout};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, AtomicU32, Ordering};
use std::sync::Mutex;

// =============================================================================
// REFERENCE COUNTED TYPES
// =============================================================================

/// Reference count type.
pub type RefCount = AtomicU32;

/// Header for reference-counted objects.
#[repr(C)]
pub struct RcHeader {
    /// Strong reference count.
    strong: RefCount,
    /// Weak reference count (plus 1 for strong refs).
    weak: RefCount,
    /// Object color for cycle detection (0=white, 1=gray, 2=black, 3=purple).
    color: AtomicU32,
    /// Buffered flag for cycle detection.
    buffered: AtomicU32,
    /// Type metadata pointer.
    type_info: *const TypeInfo,
}

/// GC colors for cycle detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum GcColor {
    /// Not visited / potentially garbage.
    White = 0,
    /// Being processed.
    Gray = 1,
    /// Definitely reachable.
    Black = 2,
    /// Candidate for cycle collection.
    Purple = 3,
}

impl RcHeader {
    /// Create a new header with initial reference count of 1.
    pub fn new(type_info: *const TypeInfo) -> Self {
        Self {
            strong: AtomicU32::new(1),
            weak: AtomicU32::new(1), // +1 for strong refs
            color: AtomicU32::new(GcColor::Black as u32),
            buffered: AtomicU32::new(0),
            type_info,
        }
    }

    /// Get strong count.
    #[inline]
    pub fn strong_count(&self) -> u32 {
        self.strong.load(Ordering::Relaxed)
    }

    /// Get weak count (not including strong refs).
    #[inline]
    pub fn weak_count(&self) -> u32 {
        self.weak.load(Ordering::Relaxed).saturating_sub(1)
    }

    /// Increment strong count.
    #[inline]
    pub fn inc_strong(&self) {
        self.strong.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement strong count. Returns true if this was the last strong ref.
    #[inline]
    pub fn dec_strong(&self) -> bool {
        self.strong.fetch_sub(1, Ordering::Release) == 1
    }

    /// Increment weak count.
    #[inline]
    pub fn inc_weak(&self) {
        self.weak.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement weak count. Returns true if this was the last weak ref.
    #[inline]
    pub fn dec_weak(&self) -> bool {
        self.weak.fetch_sub(1, Ordering::Release) == 1
    }

    /// Get the color.
    #[inline]
    pub fn color(&self) -> GcColor {
        match self.color.load(Ordering::Relaxed) {
            0 => GcColor::White,
            1 => GcColor::Gray,
            2 => GcColor::Black,
            _ => GcColor::Purple,
        }
    }

    /// Set the color.
    #[inline]
    pub fn set_color(&self, color: GcColor) {
        self.color.store(color as u32, Ordering::Relaxed);
    }

    /// Check if buffered for cycle detection.
    #[inline]
    pub fn is_buffered(&self) -> bool {
        self.buffered.load(Ordering::Relaxed) != 0
    }

    /// Set buffered flag.
    #[inline]
    pub fn set_buffered(&self, buffered: bool) {
        self.buffered.store(buffered as u32, Ordering::Relaxed);
    }
}

// =============================================================================
// TYPE METADATA
// =============================================================================

/// Type information for GC.
#[repr(C)]
pub struct TypeInfo {
    /// Type name.
    pub name: &'static str,
    /// Size in bytes.
    pub size: usize,
    /// Alignment in bytes.
    pub align: usize,
    /// Drop function (destructor).
    pub drop_fn: Option<unsafe fn(*mut u8)>,
    /// Trace function for cycle detection.
    pub trace_fn: Option<unsafe fn(*mut u8, &mut dyn FnMut(*mut RcHeader))>,
    /// Number of reference fields (for simple cycle detection).
    pub ref_field_count: usize,
    /// Offsets of reference fields.
    pub ref_field_offsets: &'static [usize],
}

impl TypeInfo {
    /// Create type info for a type with no references.
    pub const fn leaf(name: &'static str, size: usize, align: usize) -> Self {
        Self {
            name,
            size,
            align,
            drop_fn: None,
            trace_fn: None,
            ref_field_count: 0,
            ref_field_offsets: &[],
        }
    }

    /// Create type info with a drop function.
    pub const fn with_drop(
        name: &'static str,
        size: usize,
        align: usize,
        drop_fn: unsafe fn(*mut u8),
    ) -> Self {
        Self {
            name,
            size,
            align,
            drop_fn: Some(drop_fn),
            trace_fn: None,
            ref_field_count: 0,
            ref_field_offsets: &[],
        }
    }

    /// Check if this type can contain cycles.
    pub fn can_cycle(&self) -> bool {
        self.ref_field_count > 0 || self.trace_fn.is_some()
    }
}

// =============================================================================
// ALLOCATION
// =============================================================================

/// Allocate a new reference-counted object.
///
/// Returns a pointer to the header, followed by the object data.
pub fn rc_alloc(type_info: &'static TypeInfo) -> NonNull<RcHeader> {
    let header_size = std::mem::size_of::<RcHeader>();
    let header_align = std::mem::align_of::<RcHeader>();

    // Calculate total size with proper alignment
    let data_offset = (header_size + type_info.align - 1) & !(type_info.align - 1);
    let total_size = data_offset + type_info.size;
    let total_align = header_align.max(type_info.align);

    let layout = Layout::from_size_align(total_size, total_align)
        .expect("Invalid layout");

    // Allocate memory
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        std::alloc::handle_alloc_error(layout);
    }

    // Initialize header
    let header = ptr as *mut RcHeader;
    unsafe {
        header.write(RcHeader::new(type_info));
    }

    // Zero-initialize data
    let data_ptr = unsafe { ptr.add(data_offset) };
    unsafe {
        std::ptr::write_bytes(data_ptr, 0, type_info.size);
    }

    NonNull::new(header).expect("Allocation returned null")
}

/// Get the data pointer from a header pointer.
pub fn rc_data<T>(header: NonNull<RcHeader>) -> NonNull<T> {
    let type_info = unsafe { &*(*header.as_ptr()).type_info };
    let header_size = std::mem::size_of::<RcHeader>();
    let data_offset = (header_size + type_info.align - 1) & !(type_info.align - 1);

    let data_ptr = unsafe { (header.as_ptr() as *mut u8).add(data_offset) as *mut T };
    NonNull::new(data_ptr).expect("Data pointer is null")
}

/// Increment reference count.
pub fn rc_inc(header: NonNull<RcHeader>) {
    unsafe { (*header.as_ptr()).inc_strong() };
}

/// Decrement reference count and potentially free.
pub fn rc_dec(header: NonNull<RcHeader>) {
    let hdr = unsafe { &*header.as_ptr() };

    if hdr.dec_strong() {
        // This was the last strong reference
        let type_info = unsafe { &*hdr.type_info };

        // Call destructor if present
        if let Some(drop_fn) = type_info.drop_fn {
            let data_ptr = rc_data::<u8>(header);
            unsafe { drop_fn(data_ptr.as_ptr()) };
        }

        // Check if we can free immediately or need cycle detection
        if type_info.can_cycle() {
            // Add to candidate buffer for cycle detection
            CYCLE_COLLECTOR.add_candidate(header);
        }

        // Decrement weak count (from strong refs)
        rc_dec_weak(header);
    }
}

/// Decrement weak reference count.
pub fn rc_dec_weak(header: NonNull<RcHeader>) {
    let hdr = unsafe { &*header.as_ptr() };

    if hdr.dec_weak() {
        // No more weak references, free memory
        let type_info = unsafe { &*hdr.type_info };

        let header_size = std::mem::size_of::<RcHeader>();
        let header_align = std::mem::align_of::<RcHeader>();
        let data_offset = (header_size + type_info.align - 1) & !(type_info.align - 1);
        let total_size = data_offset + type_info.size;
        let total_align = header_align.max(type_info.align);

        let layout = Layout::from_size_align(total_size, total_align)
            .expect("Invalid layout");

        unsafe {
            dealloc(header.as_ptr() as *mut u8, layout);
        }
    }
}

// =============================================================================
// CYCLE DETECTION
// =============================================================================

/// Cycle collector using Bacon-Rajan algorithm.
pub struct CycleCollector {
    /// Candidate buffer (purple nodes).
    candidates: Mutex<Vec<NonNull<RcHeader>>>,
    /// Current roots being processed.
    roots: Mutex<Vec<NonNull<RcHeader>>>,
    /// Statistics.
    stats: GcStats,
}

// Safety: The NonNull pointers are only accessed through the Mutex, which provides synchronization.
// The RcHeader pointers are valid as long as the refcount is > 0.
unsafe impl Send for CycleCollector {}
unsafe impl Sync for CycleCollector {}

/// GC statistics.
#[derive(Default)]
pub struct GcStats {
    /// Total allocations.
    pub allocations: AtomicUsize,
    /// Total deallocations.
    pub deallocations: AtomicUsize,
    /// Cycle collections performed.
    pub cycle_collections: AtomicUsize,
    /// Cycles broken.
    pub cycles_broken: AtomicUsize,
    /// Current live objects.
    pub live_objects: AtomicUsize,
}

impl CycleCollector {
    /// Create a new cycle collector.
    pub const fn new() -> Self {
        Self {
            candidates: Mutex::new(Vec::new()),
            roots: Mutex::new(Vec::new()),
            stats: GcStats {
                allocations: AtomicUsize::new(0),
                deallocations: AtomicUsize::new(0),
                cycle_collections: AtomicUsize::new(0),
                cycles_broken: AtomicUsize::new(0),
                live_objects: AtomicUsize::new(0),
            },
        }
    }

    /// Add a candidate for cycle detection.
    pub fn add_candidate(&self, header: NonNull<RcHeader>) {
        let hdr = unsafe { &*header.as_ptr() };

        // Only add if not already buffered
        if hdr.is_buffered() {
            return;
        }

        hdr.set_buffered(true);
        hdr.set_color(GcColor::Purple);

        // Mutex poisoning only occurs on thread panic
        let mut candidates = self.candidates.lock().unwrap();
        candidates.push(header);

        // Trigger collection if buffer is large
        if candidates.len() > 1000 {
            drop(candidates);
            self.collect();
        }
    }

    /// Run cycle collection.
    pub fn collect(&self) {
        self.stats.cycle_collections.fetch_add(1, Ordering::Relaxed);

        // Phase 1: Mark roots
        self.mark_roots();

        // Phase 2: Scan roots
        self.scan_roots();

        // Phase 3: Collect white nodes
        self.collect_white();
    }

    /// Mark candidates as potential roots.
    fn mark_roots(&self) {
        // Mutex poisoning only occurs on thread panic
        let mut candidates = self.candidates.lock().unwrap();
        let mut roots = self.roots.lock().unwrap();

        for &header in candidates.iter() {
            let hdr = unsafe { &*header.as_ptr() };

            if hdr.color() == GcColor::Purple {
                self.mark_gray(header);
                roots.push(header);
            } else {
                hdr.set_buffered(false);
                if hdr.color() == GcColor::Black && hdr.strong_count() == 0 {
                    // Already collected
                }
            }
        }

        candidates.clear();
    }

    /// Mark a node gray and trace its children.
    fn mark_gray(&self, header: NonNull<RcHeader>) {
        let hdr = unsafe { &*header.as_ptr() };

        if hdr.color() != GcColor::Gray {
            hdr.set_color(GcColor::Gray);

            // Trace children
            let type_info = unsafe { &*hdr.type_info };
            if let Some(trace_fn) = type_info.trace_fn {
                let data_ptr = rc_data::<u8>(header);
                unsafe {
                    trace_fn(data_ptr.as_ptr(), &mut |child| {
                        let child_hdr = &*child;
                        child_hdr.strong.fetch_sub(1, Ordering::Relaxed);
                        self.mark_gray(NonNull::new_unchecked(child));
                    });
                }
            }
        }
    }

    /// Scan roots to find garbage.
    fn scan_roots(&self) {
        // Mutex poisoning only occurs on thread panic
        let roots = self.roots.lock().unwrap();

        for &header in roots.iter() {
            self.scan(header);
        }
    }

    /// Scan a node.
    fn scan(&self, header: NonNull<RcHeader>) {
        let hdr = unsafe { &*header.as_ptr() };

        if hdr.color() == GcColor::Gray {
            if hdr.strong_count() > 0 {
                self.scan_black(header);
            } else {
                hdr.set_color(GcColor::White);

                // Scan children
                let type_info = unsafe { &*hdr.type_info };
                if let Some(trace_fn) = type_info.trace_fn {
                    let data_ptr = rc_data::<u8>(header);
                    unsafe {
                        trace_fn(data_ptr.as_ptr(), &mut |child| {
                            self.scan(NonNull::new_unchecked(child));
                        });
                    }
                }
            }
        }
    }

    /// Mark a node and its children as reachable.
    fn scan_black(&self, header: NonNull<RcHeader>) {
        let hdr = unsafe { &*header.as_ptr() };
        hdr.set_color(GcColor::Black);

        // Restore children's reference counts
        let type_info = unsafe { &*hdr.type_info };
        if let Some(trace_fn) = type_info.trace_fn {
            let data_ptr = rc_data::<u8>(header);
            unsafe {
                trace_fn(data_ptr.as_ptr(), &mut |child| {
                    let child_hdr = &*child;
                    child_hdr.strong.fetch_add(1, Ordering::Relaxed);
                    if child_hdr.color() != GcColor::Black {
                        self.scan_black(NonNull::new_unchecked(child));
                    }
                });
            }
        }
    }

    /// Collect white (garbage) nodes.
    fn collect_white(&self) {
        // Mutex poisoning only occurs on thread panic
        let mut roots = self.roots.lock().unwrap();

        for &header in roots.iter() {
            self.collect_white_node(header);
        }

        roots.clear();
    }

    /// Collect a white node.
    fn collect_white_node(&self, header: NonNull<RcHeader>) {
        let hdr = unsafe { &*header.as_ptr() };

        if hdr.color() == GcColor::White && !hdr.is_buffered() {
            hdr.set_color(GcColor::Black);

            // Collect children first
            let type_info = unsafe { &*hdr.type_info };
            if let Some(trace_fn) = type_info.trace_fn {
                let data_ptr = rc_data::<u8>(header);
                unsafe {
                    trace_fn(data_ptr.as_ptr(), &mut |child| {
                        self.collect_white_node(NonNull::new_unchecked(child));
                    });
                }
            }

            // Free this node
            self.stats.cycles_broken.fetch_add(1, Ordering::Relaxed);
            rc_dec_weak(header);
        }
    }

    /// Get statistics.
    pub fn stats(&self) -> &GcStats {
        &self.stats
    }
}

// Global cycle collector
static CYCLE_COLLECTOR: CycleCollector = CycleCollector::new();

/// Force a garbage collection cycle.
pub fn gc_collect() {
    CYCLE_COLLECTOR.collect();
}

/// Get GC statistics.
pub fn gc_stats() -> &'static GcStats {
    CYCLE_COLLECTOR.stats()
}

// =============================================================================
// ARENA ALLOCATOR
// =============================================================================

/// Arena allocator for batch allocations.
pub struct Arena {
    /// Current chunk.
    chunks: Vec<Vec<u8>>,
    /// Current position in the last chunk.
    pos: usize,
    /// Chunk size.
    chunk_size: usize,
}

impl Arena {
    /// Create a new arena with the given chunk size.
    pub fn new(chunk_size: usize) -> Self {
        Self {
            chunks: vec![vec![0u8; chunk_size]],
            pos: 0,
            chunk_size,
        }
    }

    /// Allocate memory from the arena.
    pub fn alloc(&mut self, size: usize, align: usize) -> *mut u8 {
        // Align position
        let aligned_pos = (self.pos + align - 1) & !(align - 1);

        // Check if we need a new chunk
        if aligned_pos + size > self.chunk_size {
            let chunk_size = self.chunk_size.max(size);
            self.chunks.push(vec![0u8; chunk_size]);
            self.pos = 0;
            return self.alloc(size, align);
        }

        // Safe: chunks is never empty — constructor and alloc both guarantee at least one chunk
        let ptr = self.chunks.last_mut().unwrap().as_mut_ptr();
        let result = unsafe { ptr.add(aligned_pos) };
        self.pos = aligned_pos + size;
        result
    }

    /// Allocate and initialize a value.
    pub fn alloc_value<T>(&mut self, value: T) -> &mut T {
        let size = std::mem::size_of::<T>();
        let align = std::mem::align_of::<T>();
        let ptr = self.alloc(size, align) as *mut T;
        unsafe {
            ptr.write(value);
            &mut *ptr
        }
    }

    /// Reset the arena, keeping allocated chunks.
    pub fn reset(&mut self) {
        self.pos = 0;
        // Keep only the first chunk
        self.chunks.truncate(1);
    }

    /// Clear the arena, freeing all chunks.
    pub fn clear(&mut self) {
        self.chunks.clear();
        self.chunks.push(vec![0u8; self.chunk_size]);
        self.pos = 0;
    }

    /// Get total allocated bytes.
    pub fn allocated_bytes(&self) -> usize {
        if self.chunks.is_empty() {
            0
        } else {
            (self.chunks.len() - 1) * self.chunk_size + self.pos
        }
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        // Chunks are automatically dropped
    }
}

// =============================================================================
// MEMORY POOL
// =============================================================================

/// Fixed-size memory pool for frequent allocations.
pub struct MemoryPool<T> {
    /// Free list.
    free_list: Mutex<Vec<NonNull<T>>>,
    /// Total capacity.
    capacity: usize,
    /// Currently allocated.
    allocated: AtomicUsize,
}

impl<T> MemoryPool<T> {
    /// Create a new memory pool with pre-allocated capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            free_list: Mutex::new(Vec::with_capacity(capacity)),
            capacity,
            allocated: AtomicUsize::new(0),
        }
    }

    /// Allocate from the pool.
    pub fn alloc(&self) -> NonNull<T> {
        // Try to get from free list
        {
            // Mutex poisoning only occurs on thread panic
            let mut free_list = self.free_list.lock().unwrap();
            if let Some(ptr) = free_list.pop() {
                self.allocated.fetch_add(1, Ordering::Relaxed);
                return ptr;
            }
        }

        // Allocate new
        let layout = Layout::new::<T>();
        let ptr = unsafe { alloc(layout) as *mut T };
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }

        self.allocated.fetch_add(1, Ordering::Relaxed);
        NonNull::new(ptr).expect("Allocation returned null")
    }

    /// Return to the pool.
    pub fn free(&self, ptr: NonNull<T>) {
        // Mutex poisoning only occurs on thread panic
        let mut free_list = self.free_list.lock().unwrap();

        // Only keep up to capacity in free list
        if free_list.len() < self.capacity {
            free_list.push(ptr);
        } else {
            // Actually deallocate
            let layout = Layout::new::<T>();
            unsafe {
                dealloc(ptr.as_ptr() as *mut u8, layout);
            }
        }

        self.allocated.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get current allocation count.
    pub fn allocated(&self) -> usize {
        self.allocated.load(Ordering::Relaxed)
    }
}

impl<T> Drop for MemoryPool<T> {
    fn drop(&mut self) {
        // Mutex poisoning only occurs on thread panic
        let free_list = self.free_list.lock().unwrap();
        let layout = Layout::new::<T>();

        for ptr in free_list.iter() {
            unsafe {
                dealloc(ptr.as_ptr() as *mut u8, layout);
            }
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_TYPE_INFO: TypeInfo = TypeInfo::leaf("TestType", 8, 8);

    #[test]
    fn test_rc_header() {
        let header = RcHeader::new(&TEST_TYPE_INFO);
        assert_eq!(header.strong_count(), 1);
        assert_eq!(header.weak_count(), 0);
        assert_eq!(header.color(), GcColor::Black);
        assert!(!header.is_buffered());
    }

    #[test]
    fn test_rc_alloc() {
        let header = rc_alloc(&TEST_TYPE_INFO);
        let hdr = unsafe { &*header.as_ptr() };

        assert_eq!(hdr.strong_count(), 1);

        rc_inc(header);
        assert_eq!(hdr.strong_count(), 2);

        rc_dec(header);
        assert_eq!(hdr.strong_count(), 1);

        rc_dec(header);
        // Header should be freed now
    }

    #[test]
    fn test_arena() {
        let mut arena = Arena::new(1024);

        let ptr1 = arena.alloc(100, 8);
        let ptr2 = arena.alloc(200, 16);

        assert!(!ptr1.is_null());
        assert!(!ptr2.is_null());
        assert_ne!(ptr1, ptr2);

        let val = arena.alloc_value(42u64);
        assert_eq!(*val, 42);

        arena.reset();
        assert_eq!(arena.pos, 0);
    }

    #[test]
    fn test_memory_pool() {
        let pool: MemoryPool<u64> = MemoryPool::new(10);

        let ptr1 = pool.alloc();
        let ptr2 = pool.alloc();

        assert_eq!(pool.allocated(), 2);

        pool.free(ptr1);
        assert_eq!(pool.allocated(), 1);

        let ptr3 = pool.alloc();
        assert_eq!(pool.allocated(), 2);

        pool.free(ptr2);
        pool.free(ptr3);
        assert_eq!(pool.allocated(), 0);
    }
}
