// ===============================================================================
// QUANTALANG CODE GENERATOR - WASM BACKEND
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! WebAssembly (WASM) code generation backend.
//!
//! Generates WebAssembly binary format for browser and WASI targets.
//!
//! ## Features
//!
//! - Full WASI preview1 support
//! - Proper MIR to WASM instruction translation
//! - Memory management with heap allocator
//! - String and data section handling
//! - Control flow (blocks, loops, branches)
//! - Function calls and indirect calls
//! - Stack-based instruction generation

use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Arc;

use super::{Backend, Target, CodegenError, CodegenResult};
use crate::codegen::ir::*;
use crate::codegen::{GeneratedCode, OutputFormat};

// =============================================================================
// WASI SYSCALL DEFINITIONS
// =============================================================================

/// WASI error codes.
#[repr(u16)]
#[derive(Debug, Clone, Copy)]
pub enum WasiErrno {
    Success = 0,
    Toobig = 1,
    Acces = 2,
    Addrinuse = 3,
    Addrnotavail = 4,
    Afnosupport = 5,
    Again = 6,
    Already = 7,
    Badf = 8,
    Badmsg = 9,
    Busy = 10,
    Canceled = 11,
    Child = 12,
    Connaborted = 13,
    Connrefused = 14,
    Connreset = 15,
    Deadlk = 16,
    Destaddrreq = 17,
    Dom = 18,
    Dquot = 19,
    Exist = 20,
    Fault = 21,
    Fbig = 22,
    Hostunreach = 23,
    Idrm = 24,
    Ilseq = 25,
    Inprogress = 26,
    Intr = 27,
    Inval = 28,
    Io = 29,
    Isconn = 30,
    Isdir = 31,
    Loop = 32,
    Mfile = 33,
    Mlink = 34,
    Msgsize = 35,
    Multihop = 36,
    Nametoolong = 37,
    Netdown = 38,
    Netreset = 39,
    Netunreach = 40,
    Nfile = 41,
    Nobufs = 42,
    Nodev = 43,
    Noent = 44,
    Noexec = 45,
    Nolck = 46,
    Nolink = 47,
    Nomem = 48,
    Nomsg = 49,
    Noprotoopt = 50,
    Nospc = 51,
    Nosys = 52,
    Notconn = 53,
    Notdir = 54,
    Notempty = 55,
    Notrecoverable = 56,
    Notsock = 57,
    Notsup = 58,
    Notty = 59,
    Nxio = 60,
    Overflow = 61,
    Ownerdead = 62,
    Perm = 63,
    Pipe = 64,
    Proto = 65,
    Protonosupport = 66,
    Prototype = 67,
    Range = 68,
    Rofs = 69,
    Spipe = 70,
    Srch = 71,
    Stale = 72,
    Timedout = 73,
    Txtbsy = 74,
    Xdev = 75,
    Notcapable = 76,
}

/// WASI file descriptor rights.
#[derive(Debug, Clone, Copy)]
pub struct WasiRights(pub u64);

impl WasiRights {
    pub const FD_DATASYNC: u64 = 1 << 0;
    pub const FD_READ: u64 = 1 << 1;
    pub const FD_SEEK: u64 = 1 << 2;
    pub const FD_FDSTAT_SET_FLAGS: u64 = 1 << 3;
    pub const FD_SYNC: u64 = 1 << 4;
    pub const FD_TELL: u64 = 1 << 5;
    pub const FD_WRITE: u64 = 1 << 6;
    pub const FD_ADVISE: u64 = 1 << 7;
    pub const FD_ALLOCATE: u64 = 1 << 8;
    pub const PATH_CREATE_DIRECTORY: u64 = 1 << 9;
    pub const PATH_CREATE_FILE: u64 = 1 << 10;
    pub const PATH_LINK_SOURCE: u64 = 1 << 11;
    pub const PATH_LINK_TARGET: u64 = 1 << 12;
    pub const PATH_OPEN: u64 = 1 << 13;
    pub const FD_READDIR: u64 = 1 << 14;
    pub const PATH_READLINK: u64 = 1 << 15;
    pub const PATH_RENAME_SOURCE: u64 = 1 << 16;
    pub const PATH_RENAME_TARGET: u64 = 1 << 17;
    pub const PATH_FILESTAT_GET: u64 = 1 << 18;
    pub const PATH_FILESTAT_SET_SIZE: u64 = 1 << 19;
    pub const PATH_FILESTAT_SET_TIMES: u64 = 1 << 20;
    pub const FD_FILESTAT_GET: u64 = 1 << 21;
    pub const FD_FILESTAT_SET_SIZE: u64 = 1 << 22;
    pub const FD_FILESTAT_SET_TIMES: u64 = 1 << 23;
    pub const PATH_SYMLINK: u64 = 1 << 24;
    pub const PATH_REMOVE_DIRECTORY: u64 = 1 << 25;
    pub const PATH_UNLINK_FILE: u64 = 1 << 26;
    pub const POLL_FD_READWRITE: u64 = 1 << 27;
    pub const SOCK_SHUTDOWN: u64 = 1 << 28;
    pub const SOCK_ACCEPT: u64 = 1 << 29;
}

/// WASI clock identifiers.
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum WasiClockId {
    Realtime = 0,
    Monotonic = 1,
    ProcessCputimeId = 2,
    ThreadCputimeId = 3,
}

// =============================================================================
// WASM BACKEND
// =============================================================================

/// WASM backend for code generation.
pub struct WasmBackend {
    /// Output buffer (WAT text format).
    output: String,
    /// Whether to target WASI.
    wasi: bool,
    /// Memory pages to allocate (1 page = 64KB).
    memory_pages: u32,
    /// Maximum memory pages.
    max_memory_pages: Option<u32>,
    /// Current indentation level.
    indent: u32,
    /// Local variable counter.
    local_counter: u32,
    /// Block label counter.
    block_counter: u32,
    /// Mapping from LocalId to WASM local names.
    local_names: HashMap<LocalId, String>,
    /// Mapping from BlockId to WASM block labels.
    block_labels: HashMap<BlockId, String>,
    /// Heap pointer location in memory.
    heap_base: u32,
    /// Enable bulk memory operations.
    bulk_memory: bool,
    /// Enable SIMD.
    simd: bool,
    /// Enable threads.
    threads: bool,
}

impl WasmBackend {
    /// Create a new WASM backend.
    pub fn new() -> Self {
        Self {
            output: String::new(),
            wasi: false,
            memory_pages: 1,
            max_memory_pages: None,
            indent: 0,
            local_counter: 0,
            block_counter: 0,
            local_names: HashMap::new(),
            block_labels: HashMap::new(),
            heap_base: 65536, // Start heap after first page
            bulk_memory: false,
            simd: false,
            threads: false,
        }
    }

    /// Create a WASM backend targeting WASI.
    pub fn with_wasi() -> Self {
        let mut backend = Self::new();
        backend.wasi = true;
        backend.memory_pages = 4; // 256KB for WASI
        backend
    }

    /// Set initial memory pages.
    pub fn with_memory(mut self, pages: u32) -> Self {
        self.memory_pages = pages;
        self
    }

    /// Set maximum memory pages.
    pub fn with_max_memory(mut self, pages: u32) -> Self {
        self.max_memory_pages = Some(pages);
        self
    }

    /// Enable bulk memory operations.
    pub fn with_bulk_memory(mut self) -> Self {
        self.bulk_memory = true;
        self
    }

    /// Enable SIMD operations.
    pub fn with_simd(mut self) -> Self {
        self.simd = true;
        self
    }

    /// Enable threading support.
    pub fn with_threads(mut self) -> Self {
        self.threads = true;
        self
    }

    /// Reset per-function state.
    fn reset_function_state(&mut self) {
        self.local_counter = 0;
        self.block_counter = 0;
        self.local_names.clear();
        self.block_labels.clear();
    }

    /// Write indented line.
    fn emit_line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.output.push_str("  ");
        }
        self.output.push_str(s);
        self.output.push('\n');
    }

    /// Write without newline.
    fn emit(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.output.push_str("  ");
        }
        self.output.push_str(s);
    }

    /// Generate a fresh local name.
    fn fresh_local(&mut self) -> String {
        let name = format!("$__tmp{}", self.local_counter);
        self.local_counter += 1;
        name
    }

    /// Generate a fresh block label.
    fn fresh_block_label(&mut self) -> String {
        let label = format!("$block{}", self.block_counter);
        self.block_counter += 1;
        label
    }

    // =========================================================================
    // TYPE EMISSION
    // =========================================================================

    /// Emit a WASM type.
    fn emit_type(&self, ty: &MirType) -> &'static str {
        match ty {
            MirType::Void => "i32", // WASM doesn't have void, use i32 as placeholder
            MirType::Bool => "i32",
            MirType::Int(size, _) => match size {
                IntSize::I8 | IntSize::I16 | IntSize::I32 => "i32",
                IntSize::I64 | IntSize::I128 | IntSize::ISize => "i64",
            },
            MirType::Float(size) => match size {
                FloatSize::F32 => "f32",
                FloatSize::F64 => "f64",
            },
            MirType::Ptr(_) => "i32", // wasm32 pointers are 32-bit
            MirType::FnPtr(_) => "i32", // Function pointers are table indices
            MirType::Array(_, _) => "i32", // Arrays are memory pointers
            MirType::Slice(_) => "i32", // Slices are fat pointers (ptr + len)
            MirType::Struct(_) => "i32", // Structs are memory pointers
            MirType::Never => "i32", // Never returns, but needs a type
            MirType::Vector(_, _) => "v128", // WASM SIMD 128-bit vector
            // Opaque GPU types — represent as i32 pointer handles in WASM
            MirType::Texture2D(_) | MirType::Sampler | MirType::SampledImage(_) => "i32",
            MirType::TraitObject(_) => "i32", // wasm pointer
        }
    }

    /// Get the byte size of a type.
    fn type_size(&self, ty: &MirType) -> u32 {
        match ty {
            MirType::Void => 0,
            MirType::Bool => 1,
            MirType::Int(size, _) => match size {
                IntSize::I8 => 1,
                IntSize::I16 => 2,
                IntSize::I32 | IntSize::ISize => 4, // wasm32
                IntSize::I64 => 8,
                IntSize::I128 => 16,
            },
            MirType::Float(size) => match size {
                FloatSize::F32 => 4,
                FloatSize::F64 => 8,
            },
            MirType::Ptr(_) | MirType::FnPtr(_) => 4,
            MirType::Array(elem, count) => self.type_size(elem) * (*count as u32),
            MirType::Slice(_) => 8, // ptr + len
            MirType::Struct(_) => 4, // Placeholder
            MirType::Never => 0,
            MirType::Vector(elem, lanes) => self.type_size(elem) * lanes,
            // Opaque GPU types — pointer-sized handles in wasm32
            MirType::Texture2D(_) | MirType::Sampler | MirType::SampledImage(_) => 4,
            MirType::TraitObject(_) => 8, // two i32 pointers: data ptr + vtable ptr
        }
    }

    // =========================================================================
    // MODULE GENERATION
    // =========================================================================

    /// Generate module header.
    fn gen_module_header(&mut self, module: &MirModule) {
        self.emit_line(";; Generated by QuantaLang Compiler");
        self.emit_line(&format!(";; Module: {}", module.name));
        self.emit_line(";; Target: WebAssembly");
        if self.wasi {
            self.emit_line(";; WASI: snapshot_preview1");
        }
        self.output.push('\n');
        self.emit_line("(module");
        self.indent += 1;
    }

    /// Generate WASI imports.
    fn gen_wasi_imports(&mut self) {
        self.emit_line(";; =======================================================");
        self.emit_line(";; WASI Imports (wasi_snapshot_preview1)");
        self.emit_line(";; =======================================================");
        self.output.push('\n');

        // Process control
        self.emit_line(";; Process Control");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"proc_exit\" (func $__wasi_proc_exit (param i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"args_get\" (func $__wasi_args_get (param i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"args_sizes_get\" (func $__wasi_args_sizes_get (param i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"environ_get\" (func $__wasi_environ_get (param i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"environ_sizes_get\" (func $__wasi_environ_sizes_get (param i32 i32) (result i32)))");
        self.output.push('\n');

        // Clock
        self.emit_line(";; Clock");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"clock_res_get\" (func $__wasi_clock_res_get (param i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"clock_time_get\" (func $__wasi_clock_time_get (param i32 i64 i32) (result i32)))");
        self.output.push('\n');

        // File descriptors
        self.emit_line(";; File Descriptors");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_advise\" (func $__wasi_fd_advise (param i32 i64 i64 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_allocate\" (func $__wasi_fd_allocate (param i32 i64 i64) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_close\" (func $__wasi_fd_close (param i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_datasync\" (func $__wasi_fd_datasync (param i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_fdstat_get\" (func $__wasi_fd_fdstat_get (param i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_fdstat_set_flags\" (func $__wasi_fd_fdstat_set_flags (param i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_fdstat_set_rights\" (func $__wasi_fd_fdstat_set_rights (param i32 i64 i64) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_filestat_get\" (func $__wasi_fd_filestat_get (param i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_filestat_set_size\" (func $__wasi_fd_filestat_set_size (param i32 i64) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_filestat_set_times\" (func $__wasi_fd_filestat_set_times (param i32 i64 i64 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_pread\" (func $__wasi_fd_pread (param i32 i32 i32 i64 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_prestat_get\" (func $__wasi_fd_prestat_get (param i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_prestat_dir_name\" (func $__wasi_fd_prestat_dir_name (param i32 i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_pwrite\" (func $__wasi_fd_pwrite (param i32 i32 i32 i64 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_read\" (func $__wasi_fd_read (param i32 i32 i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_readdir\" (func $__wasi_fd_readdir (param i32 i32 i32 i64 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_renumber\" (func $__wasi_fd_renumber (param i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_seek\" (func $__wasi_fd_seek (param i32 i64 i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_sync\" (func $__wasi_fd_sync (param i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_tell\" (func $__wasi_fd_tell (param i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"fd_write\" (func $__wasi_fd_write (param i32 i32 i32 i32) (result i32)))");
        self.output.push('\n');

        // Path operations
        self.emit_line(";; Path Operations");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"path_create_directory\" (func $__wasi_path_create_directory (param i32 i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"path_filestat_get\" (func $__wasi_path_filestat_get (param i32 i32 i32 i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"path_filestat_set_times\" (func $__wasi_path_filestat_set_times (param i32 i32 i32 i32 i64 i64 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"path_link\" (func $__wasi_path_link (param i32 i32 i32 i32 i32 i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"path_open\" (func $__wasi_path_open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"path_readlink\" (func $__wasi_path_readlink (param i32 i32 i32 i32 i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"path_remove_directory\" (func $__wasi_path_remove_directory (param i32 i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"path_rename\" (func $__wasi_path_rename (param i32 i32 i32 i32 i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"path_symlink\" (func $__wasi_path_symlink (param i32 i32 i32 i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"path_unlink_file\" (func $__wasi_path_unlink_file (param i32 i32 i32) (result i32)))");
        self.output.push('\n');

        // Polling
        self.emit_line(";; Polling");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"poll_oneoff\" (func $__wasi_poll_oneoff (param i32 i32 i32 i32) (result i32)))");
        self.output.push('\n');

        // Random
        self.emit_line(";; Random");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"random_get\" (func $__wasi_random_get (param i32 i32) (result i32)))");
        self.output.push('\n');

        // Scheduling
        self.emit_line(";; Scheduling");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"sched_yield\" (func $__wasi_sched_yield (result i32)))");
        self.output.push('\n');

        // Sockets (optional, for socket-capable runtimes)
        self.emit_line(";; Sockets (optional)");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"sock_accept\" (func $__wasi_sock_accept (param i32 i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"sock_recv\" (func $__wasi_sock_recv (param i32 i32 i32 i32 i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"sock_send\" (func $__wasi_sock_send (param i32 i32 i32 i32 i32) (result i32)))");
        self.emit_line("(import \"wasi_snapshot_preview1\" \"sock_shutdown\" (func $__wasi_sock_shutdown (param i32 i32) (result i32)))");
        self.output.push('\n');
    }

    /// Generate memory section.
    fn gen_memory(&mut self) {
        self.emit_line(";; =======================================================");
        self.emit_line(";; Memory");
        self.emit_line(";; =======================================================");

        if let Some(max) = self.max_memory_pages {
            self.emit_line(&format!(
                "(memory (export \"memory\") {} {})",
                self.memory_pages, max
            ));
        } else {
            self.emit_line(&format!(
                "(memory (export \"memory\") {})",
                self.memory_pages
            ));
        }

        // Heap pointer global
        self.emit_line(&format!(
            "(global $__heap_base (mut i32) (i32.const {}))",
            self.heap_base
        ));
        self.emit_line("(global $__stack_pointer (mut i32) (i32.const 65536))");
        self.output.push('\n');
    }

    /// Generate data section.
    fn gen_data_section(&mut self, module: &MirModule) {
        if module.strings.is_empty() {
            return;
        }

        self.emit_line(";; =======================================================");
        self.emit_line(";; Data Section");
        self.emit_line(";; =======================================================");

        let mut offset = 0u32;
        for (i, s) in module.strings.iter().enumerate() {
            let escaped = self.escape_string(s);
            self.emit_line(&format!(
                "(data (i32.const {}) \"{}\\00\")",
                offset, escaped
            ));
            self.emit_line(&format!(
                "(global $__str{} i32 (i32.const {}))",
                i, offset
            ));
            offset += s.len() as u32 + 1; // +1 for null terminator
        }
        self.output.push('\n');
    }

    /// Escape a string for WAT.
    fn escape_string(&self, s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                '\n' => result.push_str("\\n"),
                '\r' => result.push_str("\\r"),
                '\t' => result.push_str("\\t"),
                '"' => result.push_str("\\\""),
                '\\' => result.push_str("\\\\"),
                c if c.is_ascii_graphic() || c == ' ' => result.push(c),
                c => {
                    for b in c.to_string().as_bytes() {
                        write!(&mut result, "\\{:02x}", b).unwrap();
                    }
                }
            }
        }
        result
    }

    /// Generate global variables.
    fn gen_globals(&mut self, module: &MirModule) {
        if module.globals.is_empty() {
            return;
        }

        self.emit_line(";; =======================================================");
        self.emit_line(";; Globals");
        self.emit_line(";; =======================================================");

        for global in &module.globals {
            let wasm_type = self.emit_type(&global.ty);
            let default_val = match &global.ty {
                MirType::Float(FloatSize::F32) => "f32.const 0.0",
                MirType::Float(FloatSize::F64) => "f64.const 0.0",
                MirType::Int(IntSize::I64, _) | MirType::Int(IntSize::I128, _) => "i64.const 0",
                _ => "i32.const 0",
            };

            if global.is_mut {
                self.emit_line(&format!(
                    "(global ${} (mut {}) ({}))",
                    global.name, wasm_type, default_val
                ));
            } else {
                self.emit_line(&format!(
                    "(global ${} {} ({}))",
                    global.name, wasm_type, default_val
                ));
            }
        }
        self.output.push('\n');
    }

    /// Generate table for indirect calls.
    fn gen_table(&mut self, module: &MirModule) {
        let func_count = module.functions.len();
        if func_count > 0 {
            self.emit_line(";; =======================================================");
            self.emit_line(";; Function Table");
            self.emit_line(";; =======================================================");
            self.emit_line(&format!("(table (export \"__indirect_function_table\") {} funcref)", func_count));

            // Populate table with function references
            let mut elem_str = String::from("(elem (i32.const 0)");
            for func in &module.functions {
                if !func.is_declaration() {
                    elem_str.push_str(&format!(" ${}", func.name));
                }
            }
            elem_str.push(')');
            self.emit_line(&elem_str);
            self.output.push('\n');
        }
    }

    /// Generate type declarations for function signatures.
    fn gen_types(&mut self, module: &MirModule) {
        self.emit_line(";; =======================================================");
        self.emit_line(";; Type Declarations");
        self.emit_line(";; =======================================================");

        // Generate a type for each unique signature
        let mut sig_map: HashMap<String, usize> = HashMap::new();
        let mut type_idx = 0;

        for func in &module.functions {
            let sig_str = self.signature_string(&func.sig);
            if !sig_map.contains_key(&sig_str) {
                sig_map.insert(sig_str.clone(), type_idx);

                let mut type_def = format!("(type $t{} (func", type_idx);

                // Parameters
                if !func.sig.params.is_empty() {
                    type_def.push_str(" (param");
                    for param in &func.sig.params {
                        type_def.push_str(&format!(" {}", self.emit_type(param)));
                    }
                    type_def.push(')');
                }

                // Result
                if func.sig.ret != MirType::Void {
                    type_def.push_str(&format!(" (result {})", self.emit_type(&func.sig.ret)));
                }

                type_def.push_str("))");
                self.emit_line(&type_def);

                type_idx += 1;
            }
        }
        self.output.push('\n');
    }

    /// Generate signature string for deduplication.
    fn signature_string(&self, sig: &MirFnSig) -> String {
        let mut s = String::new();
        for p in &sig.params {
            s.push_str(self.emit_type(p));
            s.push(',');
        }
        s.push_str("->");
        s.push_str(self.emit_type(&sig.ret));
        s
    }

    // =========================================================================
    // FUNCTION GENERATION
    // =========================================================================

    /// Generate a function.
    fn gen_function(&mut self, func: &MirFunction) -> CodegenResult<()> {
        self.reset_function_state();

        if func.is_declaration() {
            // External function declaration (already imported)
            return Ok(());
        }

        // Function header
        let mut header = format!("(func ${}", func.name);

        // Export public functions
        if func.is_public {
            header.push_str(&format!(" (export \"{}\")", func.name));
        }

        // Parameters
        for (i, param) in func.sig.params.iter().enumerate() {
            let name = format!("$arg{}", i);
            header.push_str(&format!(" (param {} {})", name, self.emit_type(param)));
        }

        // Result
        if func.sig.ret != MirType::Void {
            header.push_str(&format!(" (result {})", self.emit_type(&func.sig.ret)));
        }

        self.emit_line(&header);
        self.indent += 1;

        // Local variables
        self.gen_locals(func)?;

        // Function body
        self.gen_function_body(func)?;

        self.indent -= 1;
        self.emit_line(")");
        self.output.push('\n');

        Ok(())
    }

    /// Generate local variable declarations.
    fn gen_locals(&mut self, func: &MirFunction) -> CodegenResult<()> {
        for local in &func.locals {
            if local.is_param {
                // Parameters are already declared in function signature
                let name = format!("$arg{}", local.id.0);
                self.local_names.insert(local.id, name);
            } else {
                let name = if let Some(n) = &local.name {
                    format!("$local_{}", n)
                } else {
                    format!("$local{}", local.id.0)
                };
                self.local_names.insert(local.id, name.clone());
                self.emit_line(&format!("(local {} {})", name, self.emit_type(&local.ty)));
            }
        }

        // If any block has a Switch terminator, declare a scratch local
        // to hold the discriminant value during the case chain.
        let has_switch = func.blocks.as_ref().map_or(false, |blocks| {
            blocks.iter().any(|b| matches!(&b.terminator, Some(MirTerminator::Switch { .. })))
        });
        if has_switch {
            self.emit_line("(local $__switch_val i32)");
        }

        Ok(())
    }

    /// Generate function body from MIR blocks.
    ///
    /// Uses a "nested block" strategy for structured control flow:
    /// each MIR basic block gets a WASM `block` label so that forward
    /// `Goto` terminators can branch with `br`.  A top-level `loop`
    /// wraps everything so that backward branches are also possible.
    fn gen_function_body(&mut self, func: &MirFunction) -> CodegenResult<()> {
        let blocks = match &func.blocks {
            Some(blocks) => blocks,
            None => return Ok(()),
        };

        if blocks.is_empty() {
            // Empty function - just return default
            if func.sig.ret != MirType::Void {
                self.gen_default_value(&func.sig.ret);
            }
            return Ok(());
        }

        // Generate block labels
        for block in blocks {
            let label = format!("$bb{}", block.id.0);
            self.block_labels.insert(block.id, label);
        }

        let num_blocks = blocks.len();

        // ---------- Structured control-flow wrapper ----------
        //
        // Strategy: wrap the entire body in a `loop` so backward
        // branches work, and nest `block` labels for each MIR BB
        // so that forward branches (br $bbN) skip to the right place.
        //
        // Layout (for N blocks):
        //
        //   (loop $loop_top
        //     (block $bb1          ;; br $bb1 = skip to bb1
        //       (block $bb2        ;; br $bb2 = skip to bb2
        //         ...
        //         ;; bb0 code
        //         ;; terminator for bb0
        //       )  ;; $bb1 target
        //       ;; bb1 code
        //     )  ;; $bb2 target
        //     ;; bb2 code
        //     ...
        //     br $loop_top  ;; restart loop (needed for backward br)
        //   )

        if num_blocks == 1 {
            // Simple case: single block, no control flow wrapping needed.
            let block = &blocks[0];
            let label = self.block_labels.get(&block.id).cloned().unwrap_or_default();
            self.emit_line(&format!(";; Block {}", label));
            for stmt in &block.stmts {
                self.gen_stmt(stmt, func)?;
            }
            if let Some(term) = &block.terminator {
                self.gen_terminator(term, func, true)?;
            }
            return Ok(());
        }

        // Multi-block function: emit structured wrapper.
        self.emit_line("(block $fn_exit");
        self.indent += 1;
        self.emit_line("(loop $loop_top");
        self.indent += 1;

        // Open nested blocks for bb1 .. bb(N-1).
        // (bb0 is emitted at the innermost level)
        for i in (1..num_blocks).rev() {
            let label = self.block_labels.get(&BlockId(i as u32)).cloned().unwrap_or_default();
            self.emit_line(&format!("(block {}", label));
            self.indent += 1;
        }

        // Now emit each basic block's code, closing the nesting after each.
        for (i, block) in blocks.iter().enumerate() {
            let is_last = i == num_blocks - 1;
            let label = self.block_labels.get(&block.id).cloned().unwrap_or_default();
            self.emit_line(&format!(";; Block {}", label));

            for stmt in &block.stmts {
                self.gen_stmt(stmt, func)?;
            }
            if let Some(term) = &block.terminator {
                self.gen_terminator(term, func, is_last)?;
            }

            // Close the `block` that was opened for this BB (except for the last one).
            if i < num_blocks - 1 {
                self.indent -= 1;
                let next_label = self.block_labels.get(&BlockId((i + 1) as u32)).cloned().unwrap_or_default();
                self.emit_line(&format!(") ;; end {}", next_label));
            }
        }

        // Close the loop and outer block.
        self.emit_line("br $loop_top");
        self.indent -= 1;
        self.emit_line(") ;; end $loop_top");
        self.indent -= 1;
        self.emit_line(") ;; end $fn_exit");

        Ok(())
    }

    /// Generate a statement.
    fn gen_stmt(&mut self, stmt: &MirStmt, func: &MirFunction) -> CodegenResult<()> {
        match &stmt.kind {
            MirStmtKind::Assign { dest, value } => {
                self.gen_rvalue(value, func)?;
                let dest_name = self.local_names.get(dest)
                    .ok_or_else(|| CodegenError::Internal(format!("Unknown local: {:?}", dest)))?
                    .clone();
                self.emit_line(&format!("local.set {}", dest_name));
            }
            MirStmtKind::DerefAssign { ptr, value } => {
                // WASM linear memory store: load addr, compute value, store
                let ptr_name = self.local_names.get(ptr)
                    .ok_or_else(|| CodegenError::Internal(format!("Unknown local: {:?}", ptr)))?
                    .clone();
                self.gen_rvalue(value, func)?;
                self.emit_line(&format!("local.get {}", ptr_name));
                self.emit_line("i64.store");
            }
            MirStmtKind::FieldDerefAssign { ptr, field_name: _, value } => {
                let ptr_name = self.local_names.get(ptr)
                    .ok_or_else(|| CodegenError::Internal(format!("Unknown local: {:?}", ptr)))?
                    .clone();
                self.gen_rvalue(value, func)?;
                self.emit_line(&format!("local.get {}", ptr_name));
                self.emit_line("i64.store");
            }
            MirStmtKind::StorageLive(_local) => {
                // No-op in WASM
            }
            MirStmtKind::StorageDead(_local) => {
                // No-op in WASM
            }
            MirStmtKind::Nop => {
                // No-op
            }
        }

        Ok(())
    }

    /// Generate an rvalue (push result onto stack).
    fn gen_rvalue(&mut self, rvalue: &MirRValue, func: &MirFunction) -> CodegenResult<()> {
        match rvalue {
            MirRValue::Use(value) => {
                self.gen_value(value, func)?;
            }
            MirRValue::BinaryOp { op, left, right } => {
                // Push operands
                self.gen_value(left, func)?;
                self.gen_value(right, func)?;

                // Get the type for proper instruction selection
                let ty = self.infer_value_type(left, func)?;
                let instr = self.wasm_binop(*op, &ty);
                self.emit_line(&instr);
            }
            MirRValue::UnaryOp { op, operand } => {
                let ty = self.infer_value_type(operand, func)?;

                match op {
                    UnaryOp::Neg => {
                        if ty.is_float() {
                            self.gen_value(operand, func)?;
                            let wasm_ty = self.emit_type(&ty);
                            self.emit_line(&format!("{}.neg", wasm_ty));
                        } else {
                            // Integer negation: 0 - operand
                            let wasm_ty = self.emit_type(&ty);
                            self.emit_line(&format!("{}.const 0", wasm_ty));
                            self.gen_value(operand, func)?;
                            self.emit_line(&format!("{}.sub", wasm_ty));
                        }
                    }
                    UnaryOp::Not => {
                        self.gen_value(operand, func)?;
                        if ty == MirType::Bool {
                            // Boolean not: xor with 1
                            self.emit_line("i32.const 1");
                            self.emit_line("i32.xor");
                        } else {
                            // Bitwise not: xor with -1
                            let wasm_ty = self.emit_type(&ty);
                            self.emit_line(&format!("{}.const -1", wasm_ty));
                            self.emit_line(&format!("{}.xor", wasm_ty));
                        }
                    }
                }
            }
            MirRValue::Ref { place, .. } | MirRValue::AddressOf { place, .. } => {
                // Get address of place
                self.gen_place_addr(place, func)?;
            }
            MirRValue::Cast { kind, value, ty } => {
                self.gen_value(value, func)?;
                let from_ty = self.infer_value_type(value, func)?;
                self.gen_cast(*kind, &from_ty, ty)?;
            }
            MirRValue::Aggregate { kind, operands } => {
                // Allocate space on heap and store values
                let size = operands.len() as u32 * 4; // Simplified
                self.emit_line(&format!("global.get $__heap_base"));
                self.emit_line(&format!("global.get $__heap_base"));
                self.emit_line(&format!("i32.const {}", size));
                self.emit_line("i32.add");
                self.emit_line("global.set $__heap_base");
                // Store operands at allocated memory (simplified)
                // In a real implementation, we'd calculate proper offsets
            }
            MirRValue::Repeat { value, count } => {
                // Allocate array and fill with value
                let ty = self.infer_value_type(value, func)?;
                let elem_size = self.type_size(&ty);
                let total_size = elem_size * (*count as u32);

                // Allocate memory
                self.emit_line("global.get $__heap_base");
                self.emit_line(&format!("global.get $__heap_base"));
                self.emit_line(&format!("i32.const {}", total_size));
                self.emit_line("i32.add");
                self.emit_line("global.set $__heap_base");
            }
            MirRValue::Discriminant(place) => {
                // Load discriminant from enum
                self.gen_place_addr(place, func)?;
                self.emit_line("i32.load");
            }
            MirRValue::Len(place) => {
                // Load length from slice (second word of fat pointer)
                self.gen_place_addr(place, func)?;
                self.emit_line("i32.const 4");
                self.emit_line("i32.add");
                self.emit_line("i32.load");
            }
            MirRValue::NullaryOp(op, ty) => {
                match op {
                    NullaryOp::SizeOf => {
                        let size = self.type_size(ty);
                        self.emit_line(&format!("i32.const {}", size));
                    }
                    NullaryOp::AlignOf => {
                        // Simplified alignment
                        let align = std::cmp::min(self.type_size(ty), 8);
                        self.emit_line(&format!("i32.const {}", align));
                    }
                }
            }
            MirRValue::FieldAccess { base, field_name, field_ty } => {
                // Struct field access: base is a pointer to the struct in memory.
                // Load the base pointer, add the field offset, then load the value.
                // We use a simplified fixed-size field layout (4 bytes per field).
                self.emit_line(&format!(";; field access .{}", field_name));
                self.gen_value(base, func)?;
                // Compute field offset: for now, use a hash-based index as a
                // rough placeholder.  In a real implementation the struct layout
                // would be looked up from MirTypeDef.
                let field_offset = field_name.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32)) % 64;
                let elem_size = self.type_size(field_ty);
                self.emit_line(&format!("i32.const {} ;; offset of .{}", field_offset * elem_size, field_name));
                self.emit_line("i32.add");
                // Load the field value with the appropriate instruction.
                let load_instr = match self.emit_type(field_ty) {
                    "f32" => "f32.load",
                    "f64" => "f64.load",
                    "i64" => "i64.load",
                    _ => "i32.load",
                };
                self.emit_line(load_instr);
            }
            MirRValue::VariantField { base, variant_name, field_index, field_ty } => {
                // Enum variant field access: skip discriminant (4 bytes), then
                // index into the payload area.
                self.emit_line(&format!(";; variant field {}.{}", variant_name, field_index));
                self.gen_value(base, func)?;
                let elem_size = self.type_size(field_ty);
                // Offset = 4 (discriminant) + field_index * elem_size
                let offset = 4 + (*field_index) * elem_size;
                self.emit_line(&format!("i32.const {}", offset));
                self.emit_line("i32.add");
                let load_instr = match self.emit_type(field_ty) {
                    "f32" => "f32.load",
                    "f64" => "f64.load",
                    "i64" => "i64.load",
                    _ => "i32.load",
                };
                self.emit_line(load_instr);
            }
            MirRValue::IndexAccess { base, index, elem_ty } => {
                // Array index access: base_ptr + index * elem_size, then load.
                self.gen_value(base, func)?;
                self.gen_value(index, func)?;
                let elem_size = self.type_size(elem_ty);
                self.emit_line(&format!("i32.const {} ;; element size", elem_size));
                self.emit_line("i32.mul");
                self.emit_line("i32.add");
                let load_instr = match self.emit_type(elem_ty) {
                    "f32" => "f32.load",
                    "f64" => "f64.load",
                    "i64" => "i64.load",
                    _ => "i32.load",
                };
                self.emit_line(load_instr);
            }
            MirRValue::Deref { ptr, pointee_ty } => {
                self.gen_value(ptr, func)?;
                let load_instr = match self.emit_type(pointee_ty) {
                    "f32" => "f32.load",
                    "f64" => "f64.load",
                    "i64" => "i64.load",
                    _ => "i32.load",
                };
                self.emit_line(load_instr);
            }
            MirRValue::TextureSample { .. } => {
                // GPU-only operation; push zero placeholder in WASM
                self.emit_line(";; texture_sample: GPU-only, not supported in WASM");
                self.emit_line("i32.const 0");
            }
        }

        Ok(())
    }

    /// Generate a value (push onto stack).
    fn gen_value(&mut self, value: &MirValue, func: &MirFunction) -> CodegenResult<()> {
        match value {
            MirValue::Local(id) => {
                let name = self.local_names.get(id)
                    .ok_or_else(|| CodegenError::Internal(format!("Unknown local: {:?}", id)))?
                    .clone();
                self.emit_line(&format!("local.get {}", name));
            }
            MirValue::Const(c) => {
                self.gen_const(c)?;
            }
            MirValue::Global(name) => {
                self.emit_line(&format!("global.get ${}", name));
            }
            MirValue::Function(name) => {
                // Function reference - push table index
                // In real impl, we'd look up the function's table index
                self.emit_line(&format!("i32.const 0 ;; func ${}", name));
            }
        }

        Ok(())
    }

    /// Generate a constant value.
    fn gen_const(&mut self, c: &MirConst) -> CodegenResult<()> {
        match c {
            MirConst::Bool(b) => {
                self.emit_line(&format!("i32.const {}", if *b { 1 } else { 0 }));
            }
            MirConst::Int(v, ty) => {
                let wasm_ty = self.emit_type(ty);
                self.emit_line(&format!("{}.const {}", wasm_ty, v));
            }
            MirConst::Uint(v, ty) => {
                let wasm_ty = self.emit_type(ty);
                self.emit_line(&format!("{}.const {}", wasm_ty, v));
            }
            MirConst::Float(v, ty) => {
                let wasm_ty = self.emit_type(ty);
                self.emit_line(&format!("{}.const {}", wasm_ty, v));
            }
            MirConst::Str(idx) => {
                self.emit_line(&format!("global.get $__str{}", idx));
            }
            MirConst::ByteStr(_) => {
                // Push address of byte string in data section
                self.emit_line("i32.const 0 ;; bytestr");
            }
            MirConst::Null(_) => {
                self.emit_line("i32.const 0");
            }
            MirConst::Unit => {
                // Unit is represented as i32 0
                self.emit_line("i32.const 0");
            }
            MirConst::Zeroed(_) => {
                self.emit_line("i32.const 0");
            }
            MirConst::Undef(_) => {
                self.emit_line("i32.const 0 ;; undef");
            }
        }

        Ok(())
    }

    /// Generate a place address.
    fn gen_place_addr(&mut self, place: &MirPlace, func: &MirFunction) -> CodegenResult<()> {
        // Start with the local's address
        let name = self.local_names.get(&place.local)
            .ok_or_else(|| CodegenError::Internal(format!("Unknown local: {:?}", place.local)))?
            .clone();
        self.emit_line(&format!("local.get {}", name));

        // Apply projections
        for proj in &place.projections {
            match proj {
                PlaceProjection::Deref => {
                    self.emit_line("i32.load");
                }
                PlaceProjection::Field(idx, _ty) => {
                    self.emit_line(&format!("i32.const {}", idx * 4));
                    self.emit_line("i32.add");
                }
                PlaceProjection::Index(idx_local) => {
                    let idx_name = self.local_names.get(idx_local)
                        .ok_or_else(|| CodegenError::Internal(format!("Unknown local: {:?}", idx_local)))?
                        .clone();
                    self.emit_line(&format!("local.get {}", idx_name));
                    self.emit_line("i32.const 4");
                    self.emit_line("i32.mul");
                    self.emit_line("i32.add");
                }
                PlaceProjection::ConstantIndex { offset, from_end } => {
                    if *from_end {
                        // Need array length - simplified
                        self.emit_line(&format!("i32.const {}", offset * 4));
                        self.emit_line("i32.sub");
                    } else {
                        self.emit_line(&format!("i32.const {}", offset * 4));
                        self.emit_line("i32.add");
                    }
                }
                PlaceProjection::Subslice { from, .. } => {
                    self.emit_line(&format!("i32.const {}", from * 4));
                    self.emit_line("i32.add");
                }
                PlaceProjection::Downcast(_) => {
                    // Skip discriminant (4 bytes)
                    self.emit_line("i32.const 4");
                    self.emit_line("i32.add");
                }
            }
        }

        Ok(())
    }

    /// Emit a branch to the target block using structured control flow.
    ///
    /// For a forward branch (target > current), emit `br $bbN`.
    /// For a backward branch (target == 0), emit `br $loop_top`.
    /// For a branch to self (target == current), emit `br $loop_top` for simplicity.
    fn emit_br_to(&mut self, target: BlockId) {
        if target.0 == 0 {
            // Backward branch to the start of the function.
            self.emit_line("br $loop_top ;; back to bb0");
        } else {
            // Forward branch to the target block label.
            let label = self.block_labels.get(&target)
                .cloned()
                .unwrap_or_else(|| format!("$bb{}", target.0));
            self.emit_line(&format!("br {} ;; goto bb{}", label, target.0));
        }
    }

    /// Generate a terminator.
    fn gen_terminator(&mut self, term: &MirTerminator, func: &MirFunction, _is_last: bool) -> CodegenResult<()> {
        match term {
            MirTerminator::Goto(target) => {
                self.emit_br_to(*target);
            }
            MirTerminator::If { cond, then_block, else_block } => {
                self.gen_value(cond, func)?;
                self.emit_line("(if");
                self.indent += 1;
                self.emit_line("(then");
                self.indent += 1;
                self.emit_br_to(*then_block);
                self.indent -= 1;
                self.emit_line(")");
                self.emit_line("(else");
                self.indent += 1;
                self.emit_br_to(*else_block);
                self.indent -= 1;
                self.emit_line(")");
                self.indent -= 1;
                self.emit_line(")");
            }
            MirTerminator::Switch { value, targets, default } => {
                // Emit switch as a chain of if-then-br comparisons.
                // We use a dedicated local ($__switch_val) to avoid re-evaluating
                // the switch discriminant for each case.
                self.emit_line(";; switch");
                self.gen_value(value, func)?;
                // Store discriminant in the function's existing __switch_val
                // local (declared in gen_locals if a switch is present).
                self.emit_line("local.set $__switch_val");
                for (const_val, target) in targets {
                    let val_str = match const_val {
                        MirConst::Int(v, _) => v.to_string(),
                        MirConst::Uint(v, _) => v.to_string(),
                        _ => "0".to_string(),
                    };
                    self.emit_line("local.get $__switch_val");
                    self.emit_line(&format!("i32.const {}", val_str));
                    self.emit_line("i32.eq");
                    self.emit_line("(if");
                    self.indent += 1;
                    self.emit_line("(then");
                    self.indent += 1;
                    self.emit_br_to(*target);
                    self.indent -= 1;
                    self.emit_line(")");
                    self.indent -= 1;
                    self.emit_line(")");
                }
                // Default case -- unconditional branch.
                self.emit_br_to(*default);
            }
            MirTerminator::Call { func: callee, args, dest, target, .. } => {
                // Push arguments
                for arg in args {
                    self.gen_value(arg, func)?;
                }

                // Call function
                match callee {
                    MirValue::Function(name) => {
                        self.emit_line(&format!("call ${}", name));
                    }
                    _ => {
                        // Indirect call
                        self.gen_value(callee, func)?;
                        self.emit_line("call_indirect");
                    }
                }

                // Store result
                if let Some(dest_local) = dest {
                    let dest_name = self.local_names.get(dest_local)
                        .ok_or_else(|| CodegenError::Internal(format!("Unknown local: {:?}", dest_local)))?
                        .clone();
                    self.emit_line(&format!("local.set {}", dest_name));
                }

                // Continue to target block
                if let Some(target) = target {
                    self.emit_br_to(*target);
                }
            }
            MirTerminator::Return(value) => {
                if let Some(val) = value {
                    self.gen_value(val, func)?;
                }
                self.emit_line("return");
            }
            MirTerminator::Unreachable => {
                self.emit_line("unreachable");
            }
            MirTerminator::Drop { target, .. } => {
                // Drop is a no-op in WASM (no destructor support yet),
                // but we still need to branch to the continuation block.
                self.emit_line(";; drop (no-op)");
                self.emit_br_to(*target);
            }
            MirTerminator::Assert { cond, expected, msg, target, .. } => {
                self.gen_value(cond, func)?;
                if !*expected {
                    self.emit_line("i32.eqz");
                }
                self.emit_line("(if");
                self.indent += 1;
                self.emit_line("(then)");
                self.emit_line(&format!("(else ;; assert failed: {}", msg));
                self.emit_line("  unreachable");
                self.emit_line(")");
                self.indent -= 1;
                self.emit_line(")");
                self.emit_br_to(*target);
            }
            MirTerminator::Resume => {
                self.emit_line(";; resume unwinding");
                self.emit_line("unreachable");
            }
            MirTerminator::Abort => {
                if self.wasi {
                    self.emit_line("i32.const 1");
                    self.emit_line("call $__wasi_proc_exit");
                }
                self.emit_line("unreachable");
            }
        }

        Ok(())
    }

    /// Generate a cast instruction.
    fn gen_cast(&mut self, kind: CastKind, from: &MirType, to: &MirType) -> CodegenResult<()> {
        let from_wasm = self.emit_type(from);
        let to_wasm = self.emit_type(to);

        match kind {
            CastKind::IntToInt => {
                // WASM only has i32 and i64, so we need wrap/extend
                match (from_wasm, to_wasm) {
                    ("i32", "i64") => {
                        if from.is_signed() {
                            self.emit_line("i64.extend_i32_s");
                        } else {
                            self.emit_line("i64.extend_i32_u");
                        }
                    }
                    ("i64", "i32") => {
                        self.emit_line("i32.wrap_i64");
                    }
                    _ => {} // Same size, no conversion needed
                }
            }
            CastKind::FloatToFloat => {
                match (from_wasm, to_wasm) {
                    ("f32", "f64") => {
                        self.emit_line("f64.promote_f32");
                    }
                    ("f64", "f32") => {
                        self.emit_line("f32.demote_f64");
                    }
                    _ => {}
                }
            }
            CastKind::IntToFloat => {
                let convert = if from.is_signed() {
                    format!("{}.convert_i{}_s", to_wasm, if from_wasm == "i64" { "64" } else { "32" })
                } else {
                    format!("{}.convert_i{}_u", to_wasm, if from_wasm == "i64" { "64" } else { "32" })
                };
                self.emit_line(&convert);
            }
            CastKind::FloatToInt => {
                let trunc = if to.is_signed() {
                    format!("{}.trunc_{}_s", to_wasm, from_wasm)
                } else {
                    format!("{}.trunc_{}_u", to_wasm, from_wasm)
                };
                self.emit_line(&trunc);
            }
            CastKind::PtrToInt | CastKind::IntToPtr | CastKind::PtrToPtr | CastKind::FnToPtr => {
                // Pointers and ints are both i32 in wasm32
            }
            CastKind::Transmute => {
                // Reinterpret bits
                match (from_wasm, to_wasm) {
                    ("f32", "i32") => self.emit_line("i32.reinterpret_f32"),
                    ("i32", "f32") => self.emit_line("f32.reinterpret_i32"),
                    ("f64", "i64") => self.emit_line("i64.reinterpret_f64"),
                    ("i64", "f64") => self.emit_line("f64.reinterpret_i64"),
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Generate WASM binary operation instruction.
    fn wasm_binop(&self, op: BinOp, ty: &MirType) -> String {
        let wasm_ty = self.emit_type(ty);
        let is_float = ty.is_float();
        let is_signed = ty.is_signed();

        match op {
            BinOp::Add => format!("{}.add", wasm_ty),
            BinOp::Sub => format!("{}.sub", wasm_ty),
            BinOp::Mul => format!("{}.mul", wasm_ty),
            BinOp::Div => {
                if is_float {
                    format!("{}.div", wasm_ty)
                } else if is_signed {
                    format!("{}.div_s", wasm_ty)
                } else {
                    format!("{}.div_u", wasm_ty)
                }
            }
            BinOp::Rem => {
                if is_float {
                    // WASM doesn't have float rem, need runtime
                    format!(";; TODO: {}.rem", wasm_ty)
                } else if is_signed {
                    format!("{}.rem_s", wasm_ty)
                } else {
                    format!("{}.rem_u", wasm_ty)
                }
            }
            BinOp::BitAnd => format!("{}.and", wasm_ty),
            BinOp::BitOr => format!("{}.or", wasm_ty),
            BinOp::BitXor => format!("{}.xor", wasm_ty),
            BinOp::Shl => format!("{}.shl", wasm_ty),
            BinOp::Shr => {
                if is_signed {
                    format!("{}.shr_s", wasm_ty)
                } else {
                    format!("{}.shr_u", wasm_ty)
                }
            }
            BinOp::Eq => {
                if is_float {
                    format!("{}.eq", wasm_ty)
                } else {
                    format!("{}.eq", wasm_ty)
                }
            }
            BinOp::Ne => {
                if is_float {
                    format!("{}.ne", wasm_ty)
                } else {
                    format!("{}.ne", wasm_ty)
                }
            }
            BinOp::Lt => {
                if is_float {
                    format!("{}.lt", wasm_ty)
                } else if is_signed {
                    format!("{}.lt_s", wasm_ty)
                } else {
                    format!("{}.lt_u", wasm_ty)
                }
            }
            BinOp::Le => {
                if is_float {
                    format!("{}.le", wasm_ty)
                } else if is_signed {
                    format!("{}.le_s", wasm_ty)
                } else {
                    format!("{}.le_u", wasm_ty)
                }
            }
            BinOp::Gt => {
                if is_float {
                    format!("{}.gt", wasm_ty)
                } else if is_signed {
                    format!("{}.gt_s", wasm_ty)
                } else {
                    format!("{}.gt_u", wasm_ty)
                }
            }
            BinOp::Ge => {
                if is_float {
                    format!("{}.ge", wasm_ty)
                } else if is_signed {
                    format!("{}.ge_s", wasm_ty)
                } else {
                    format!("{}.ge_u", wasm_ty)
                }
            }
            BinOp::AddChecked | BinOp::AddWrapping | BinOp::AddSaturating => format!("{}.add", wasm_ty),
            BinOp::SubChecked | BinOp::SubWrapping | BinOp::SubSaturating => format!("{}.sub", wasm_ty),
            BinOp::MulChecked | BinOp::MulWrapping => format!("{}.mul", wasm_ty),
            BinOp::Pow => {
                // WASM doesn't have a native power instruction
                // For floats, could use wasm-intrinsics when available
                // For integers, call the runtime power function
                format!("call $__quanta_pow_{}", wasm_ty)
            }
        }
    }

    /// Infer the type of a value.
    fn infer_value_type(&self, value: &MirValue, func: &MirFunction) -> CodegenResult<MirType> {
        match value {
            MirValue::Local(id) => {
                func.locals.iter()
                    .find(|l| l.id == *id)
                    .map(|l| l.ty.clone())
                    .ok_or_else(|| CodegenError::Internal(format!("Unknown local: {:?}", id)))
            }
            MirValue::Const(c) => {
                match c {
                    MirConst::Bool(_) => Ok(MirType::Bool),
                    MirConst::Int(_, ty) => Ok(ty.clone()),
                    MirConst::Uint(_, ty) => Ok(ty.clone()),
                    MirConst::Float(_, ty) => Ok(ty.clone()),
                    MirConst::Str(_) => Ok(MirType::Ptr(Box::new(MirType::Int(IntSize::I8, false)))),
                    MirConst::ByteStr(_) => Ok(MirType::Ptr(Box::new(MirType::Int(IntSize::I8, false)))),
                    MirConst::Null(ty) => Ok(ty.clone()),
                    MirConst::Unit => Ok(MirType::Void),
                    MirConst::Zeroed(ty) => Ok(ty.clone()),
                    MirConst::Undef(ty) => Ok(ty.clone()),
                }
            }
            MirValue::Global(_) => Ok(MirType::i32()),
            MirValue::Function(_) => Ok(MirType::i32()),
        }
    }

    /// Generate a default value for a type.
    fn gen_default_value(&mut self, ty: &MirType) {
        match ty {
            MirType::Float(FloatSize::F32) => self.emit_line("f32.const 0.0"),
            MirType::Float(FloatSize::F64) => self.emit_line("f64.const 0.0"),
            MirType::Int(IntSize::I64, _) | MirType::Int(IntSize::I128, _) => {
                self.emit_line("i64.const 0")
            }
            _ => self.emit_line("i32.const 0"),
        }
    }

    /// Generate WASI _start entry point.
    fn gen_wasi_start(&mut self, module: &MirModule) {
        let has_main = module.functions.iter().any(|f| f.name.as_ref() == "main");

        if has_main {
            self.emit_line(";; =======================================================");
            self.emit_line(";; WASI Entry Point");
            self.emit_line(";; =======================================================");
            self.emit_line("(func (export \"_start\")");
            self.indent += 1;
            self.emit_line("call $main");
            self.emit_line("drop ;; ignore return value");
            self.indent -= 1;
            self.emit_line(")");
            self.output.push('\n');
        }
    }

    /// Generate WASI helper functions.
    fn gen_wasi_helpers(&mut self) {
        self.emit_line(";; =======================================================");
        self.emit_line(";; WASI Helper Functions");
        self.emit_line(";; =======================================================");
        self.output.push('\n');

        // print function - write to stdout
        self.emit_line("(func $__quanta_print (param $ptr i32) (param $len i32)");
        self.indent += 1;
        self.emit_line("(local $iov i32)");
        self.emit_line("(local $written i32)");
        self.emit_line(";; Build iovec at heap");
        self.emit_line("global.get $__heap_base");
        self.emit_line("local.set $iov");
        self.emit_line(";; iov.buf = ptr");
        self.emit_line("local.get $iov");
        self.emit_line("local.get $ptr");
        self.emit_line("i32.store");
        self.emit_line(";; iov.len = len");
        self.emit_line("local.get $iov");
        self.emit_line("i32.const 4");
        self.emit_line("i32.add");
        self.emit_line("local.get $len");
        self.emit_line("i32.store");
        self.emit_line(";; fd_write(stdout=1, iov, 1, &written)");
        self.emit_line("i32.const 1 ;; stdout");
        self.emit_line("local.get $iov");
        self.emit_line("i32.const 1 ;; iovs_len");
        self.emit_line("local.get $iov");
        self.emit_line("i32.const 8");
        self.emit_line("i32.add");
        self.emit_line("call $__wasi_fd_write");
        self.emit_line("drop");
        self.indent -= 1;
        self.emit_line(")");
        self.output.push('\n');

        // println function - print with newline
        self.emit_line("(func $__quanta_println (param $ptr i32) (param $len i32)");
        self.indent += 1;
        self.emit_line("local.get $ptr");
        self.emit_line("local.get $len");
        self.emit_line("call $__quanta_print");
        self.emit_line(";; Print newline");
        self.emit_line("i32.const 1024 ;; newline address");
        self.emit_line("i32.const 1");
        self.emit_line("call $__quanta_print");
        self.indent -= 1;
        self.emit_line(")");
        self.output.push('\n');

        // Newline data
        self.emit_line("(data (i32.const 1024) \"\\n\")");
        self.output.push('\n');

        // Memory allocator (bump allocator)
        self.emit_line(";; Simple bump allocator");
        self.emit_line("(func $__quanta_alloc (param $size i32) (result i32)");
        self.indent += 1;
        self.emit_line("(local $ptr i32)");
        self.emit_line(";; Get current heap pointer");
        self.emit_line("global.get $__heap_base");
        self.emit_line("local.set $ptr");
        self.emit_line(";; Align size to 8 bytes");
        self.emit_line("local.get $size");
        self.emit_line("i32.const 7");
        self.emit_line("i32.add");
        self.emit_line("i32.const -8 ;; 0xFFFFFFF8");
        self.emit_line("i32.and");
        self.emit_line(";; Bump heap pointer");
        self.emit_line("global.get $__heap_base");
        self.emit_line("i32.add");
        self.emit_line("global.set $__heap_base");
        self.emit_line(";; Return old pointer");
        self.emit_line("local.get $ptr");
        self.indent -= 1;
        self.emit_line(")");
        self.output.push('\n');

        // Free (no-op for bump allocator)
        self.emit_line("(func $__quanta_free (param $ptr i32)");
        self.indent += 1;
        self.emit_line(";; No-op for bump allocator");
        self.emit_line("nop");
        self.indent -= 1;
        self.emit_line(")");
        self.output.push('\n');

        // Memory copy
        if self.bulk_memory {
            self.emit_line("(func $__quanta_memcpy (param $dest i32) (param $src i32) (param $len i32)");
            self.indent += 1;
            self.emit_line("local.get $dest");
            self.emit_line("local.get $src");
            self.emit_line("local.get $len");
            self.emit_line("memory.copy");
            self.indent -= 1;
            self.emit_line(")");
        } else {
            self.emit_line("(func $__quanta_memcpy (param $dest i32) (param $src i32) (param $len i32)");
            self.indent += 1;
            self.emit_line("(local $i i32)");
            self.emit_line("(block $done");
            self.indent += 1;
            self.emit_line("(loop $loop");
            self.indent += 1;
            self.emit_line("local.get $i");
            self.emit_line("local.get $len");
            self.emit_line("i32.ge_u");
            self.emit_line("br_if $done");
            self.emit_line(";; dest[i] = src[i]");
            self.emit_line("local.get $dest");
            self.emit_line("local.get $i");
            self.emit_line("i32.add");
            self.emit_line("local.get $src");
            self.emit_line("local.get $i");
            self.emit_line("i32.add");
            self.emit_line("i32.load8_u");
            self.emit_line("i32.store8");
            self.emit_line(";; i++");
            self.emit_line("local.get $i");
            self.emit_line("i32.const 1");
            self.emit_line("i32.add");
            self.emit_line("local.set $i");
            self.emit_line("br $loop");
            self.indent -= 1;
            self.emit_line(")");
            self.indent -= 1;
            self.emit_line(")");
            self.indent -= 1;
            self.emit_line(")");
        }
        self.output.push('\n');

        // Memory set
        if self.bulk_memory {
            self.emit_line("(func $__quanta_memset (param $dest i32) (param $val i32) (param $len i32)");
            self.indent += 1;
            self.emit_line("local.get $dest");
            self.emit_line("local.get $val");
            self.emit_line("local.get $len");
            self.emit_line("memory.fill");
            self.indent -= 1;
            self.emit_line(")");
        } else {
            self.emit_line("(func $__quanta_memset (param $dest i32) (param $val i32) (param $len i32)");
            self.indent += 1;
            self.emit_line("(local $i i32)");
            self.emit_line("(block $done");
            self.indent += 1;
            self.emit_line("(loop $loop");
            self.indent += 1;
            self.emit_line("local.get $i");
            self.emit_line("local.get $len");
            self.emit_line("i32.ge_u");
            self.emit_line("br_if $done");
            self.emit_line(";; dest[i] = val");
            self.emit_line("local.get $dest");
            self.emit_line("local.get $i");
            self.emit_line("i32.add");
            self.emit_line("local.get $val");
            self.emit_line("i32.store8");
            self.emit_line(";; i++");
            self.emit_line("local.get $i");
            self.emit_line("i32.const 1");
            self.emit_line("i32.add");
            self.emit_line("local.set $i");
            self.emit_line("br $loop");
            self.indent -= 1;
            self.emit_line(")");
            self.indent -= 1;
            self.emit_line(")");
            self.indent -= 1;
            self.emit_line(")");
        }
        self.output.push('\n');

        // Exit function
        self.emit_line("(func $__quanta_exit (param $code i32)");
        self.indent += 1;
        self.emit_line("local.get $code");
        self.emit_line("call $__wasi_proc_exit");
        self.indent -= 1;
        self.emit_line(")");
        self.output.push('\n');

        // Abort function
        self.emit_line("(func $__quanta_abort");
        self.indent += 1;
        self.emit_line("i32.const 134 ;; SIGABRT");
        self.emit_line("call $__wasi_proc_exit");
        self.indent -= 1;
        self.emit_line(")");
        self.output.push('\n');

        // Get args count
        self.emit_line("(func $__quanta_args_count (result i32)");
        self.indent += 1;
        self.emit_line("(local $argc i32)");
        self.emit_line("(local $argv_buf_size i32)");
        self.emit_line(";; Get args sizes");
        self.emit_line("global.get $__heap_base ;; &argc");
        self.emit_line("global.get $__heap_base");
        self.emit_line("i32.const 4");
        self.emit_line("i32.add ;; &argv_buf_size");
        self.emit_line("call $__wasi_args_sizes_get");
        self.emit_line("drop");
        self.emit_line(";; Return argc");
        self.emit_line("global.get $__heap_base");
        self.emit_line("i32.load");
        self.indent -= 1;
        self.emit_line(")");
        self.output.push('\n');

        // Get current time (nanoseconds since epoch)
        self.emit_line("(func $__quanta_time_now (result i64)");
        self.indent += 1;
        self.emit_line("(local $time_ptr i32)");
        self.emit_line(";; Allocate 8 bytes for timestamp");
        self.emit_line("global.get $__heap_base");
        self.emit_line("local.set $time_ptr");
        self.emit_line(";; Get realtime clock");
        self.emit_line("i32.const 0 ;; CLOCK_REALTIME");
        self.emit_line("i64.const 1 ;; precision: 1ns");
        self.emit_line("local.get $time_ptr");
        self.emit_line("call $__wasi_clock_time_get");
        self.emit_line("drop");
        self.emit_line(";; Return timestamp");
        self.emit_line("local.get $time_ptr");
        self.emit_line("i64.load");
        self.indent -= 1;
        self.emit_line(")");
        self.output.push('\n');

        // Random bytes
        self.emit_line("(func $__quanta_random (param $buf i32) (param $len i32) (result i32)");
        self.indent += 1;
        self.emit_line("local.get $buf");
        self.emit_line("local.get $len");
        self.emit_line("call $__wasi_random_get");
        self.indent -= 1;
        self.emit_line(")");
        self.output.push('\n');

        // Stderr print
        self.emit_line("(func $__quanta_eprint (param $ptr i32) (param $len i32)");
        self.indent += 1;
        self.emit_line("(local $iov i32)");
        self.emit_line("(local $written i32)");
        self.emit_line("global.get $__heap_base");
        self.emit_line("local.set $iov");
        self.emit_line("local.get $iov");
        self.emit_line("local.get $ptr");
        self.emit_line("i32.store");
        self.emit_line("local.get $iov");
        self.emit_line("i32.const 4");
        self.emit_line("i32.add");
        self.emit_line("local.get $len");
        self.emit_line("i32.store");
        self.emit_line("i32.const 2 ;; stderr");
        self.emit_line("local.get $iov");
        self.emit_line("i32.const 1");
        self.emit_line("local.get $iov");
        self.emit_line("i32.const 8");
        self.emit_line("i32.add");
        self.emit_line("call $__wasi_fd_write");
        self.emit_line("drop");
        self.indent -= 1;
        self.emit_line(")");
        self.output.push('\n');
    }
}

impl Default for WasmBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for WasmBackend {
    fn generate(&mut self, mir: &MirModule) -> CodegenResult<GeneratedCode> {
        self.output.clear();

        // Generate module structure
        self.gen_module_header(mir);

        // WASI imports
        if self.wasi {
            self.gen_wasi_imports();
        }

        // Memory
        self.gen_memory();

        // Types
        self.gen_types(mir);

        // Data section
        self.gen_data_section(mir);

        // Globals
        self.gen_globals(mir);

        // Table for indirect calls
        self.gen_table(mir);

        // WASI helpers
        if self.wasi {
            self.gen_wasi_helpers();
        }

        // Functions
        self.emit_line(";; =======================================================");
        self.emit_line(";; Functions");
        self.emit_line(";; =======================================================");
        self.output.push('\n');

        for func in &mir.functions {
            self.gen_function(func)?;
        }

        // WASI _start
        if self.wasi {
            self.gen_wasi_start(mir);
        }

        // Close module
        self.indent -= 1;
        self.emit_line(")");

        Ok(GeneratedCode::new(
            OutputFormat::Wat,
            self.output.as_bytes().to_vec(),
        ))
    }

    fn target(&self) -> Target {
        Target::Wasm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_simple_module() -> MirModule {
        let mut module = MirModule::new("test");

        let sig = MirFnSig::new(vec![MirType::i32(), MirType::i32()], MirType::i32());
        let mut func = MirFunction::new("add", sig);
        func.is_public = true;

        // Add parameters
        func.add_local(MirLocal {
            id: LocalId(0),
            name: Some("a".into()),
            ty: MirType::i32(),
            is_mut: false,
            is_param: true,
            annotations: Vec::new(),
        });
        func.add_local(MirLocal {
            id: LocalId(1),
            name: Some("b".into()),
            ty: MirType::i32(),
            is_mut: false,
            is_param: true,
            annotations: Vec::new(),
        });
        func.add_local(MirLocal {
            id: LocalId(2),
            name: Some("result".into()),
            ty: MirType::i32(),
            is_mut: true,
            is_param: false,
            annotations: Vec::new(),
        });

        let mut block = MirBlock::new(BlockId::ENTRY);
        block.push_stmt(MirStmt::assign(
            LocalId(2),
            MirRValue::BinaryOp {
                op: BinOp::Add,
                left: MirValue::Local(LocalId(0)),
                right: MirValue::Local(LocalId(1)),
            },
        ));
        block.set_terminator(MirTerminator::Return(Some(MirValue::Local(LocalId(2)))));
        func.add_block(block);

        module.add_function(func);
        module
    }

    #[test]
    fn test_wasm_backend_new() {
        let backend = WasmBackend::new();
        assert!(!backend.wasi);
        assert_eq!(backend.memory_pages, 1);
    }

    #[test]
    fn test_wasm_backend_with_wasi() {
        let backend = WasmBackend::with_wasi();
        assert!(backend.wasi);
        assert_eq!(backend.memory_pages, 4);
    }

    #[test]
    fn test_wasm_backend_with_options() {
        let backend = WasmBackend::new()
            .with_memory(8)
            .with_max_memory(64)
            .with_bulk_memory()
            .with_simd();

        assert_eq!(backend.memory_pages, 8);
        assert_eq!(backend.max_memory_pages, Some(64));
        assert!(backend.bulk_memory);
        assert!(backend.simd);
    }

    #[test]
    fn test_emit_type() {
        let backend = WasmBackend::new();

        assert_eq!(backend.emit_type(&MirType::Bool), "i32");
        assert_eq!(backend.emit_type(&MirType::i32()), "i32");
        assert_eq!(backend.emit_type(&MirType::i64()), "i64");
        assert_eq!(backend.emit_type(&MirType::f32()), "f32");
        assert_eq!(backend.emit_type(&MirType::f64()), "f64");
        assert_eq!(backend.emit_type(&MirType::Ptr(Box::new(MirType::i32()))), "i32");
    }

    #[test]
    fn test_type_size() {
        let backend = WasmBackend::new();

        assert_eq!(backend.type_size(&MirType::Bool), 1);
        assert_eq!(backend.type_size(&MirType::i8()), 1);
        assert_eq!(backend.type_size(&MirType::i16()), 2);
        assert_eq!(backend.type_size(&MirType::i32()), 4);
        assert_eq!(backend.type_size(&MirType::i64()), 8);
        assert_eq!(backend.type_size(&MirType::f32()), 4);
        assert_eq!(backend.type_size(&MirType::f64()), 8);
    }

    #[test]
    fn test_wasm_binop() {
        let backend = WasmBackend::new();

        assert_eq!(backend.wasm_binop(BinOp::Add, &MirType::i32()), "i32.add");
        assert_eq!(backend.wasm_binop(BinOp::Sub, &MirType::i64()), "i64.sub");
        assert_eq!(backend.wasm_binop(BinOp::Mul, &MirType::f32()), "f32.mul");
        assert_eq!(backend.wasm_binop(BinOp::Div, &MirType::i32()), "i32.div_s");
        assert_eq!(backend.wasm_binop(BinOp::Div, &MirType::u32()), "i32.div_u");
        assert_eq!(backend.wasm_binop(BinOp::Div, &MirType::f64()), "f64.div");
    }

    #[test]
    fn test_escape_string() {
        let backend = WasmBackend::new();

        assert_eq!(backend.escape_string("hello"), "hello");
        assert_eq!(backend.escape_string("hello\nworld"), "hello\\nworld");
        assert_eq!(backend.escape_string("tab\there"), "tab\\there");
        assert_eq!(backend.escape_string("quote\"here"), "quote\\\"here");
    }

    #[test]
    fn test_generate_simple_module() {
        let module = create_simple_module();
        let mut backend = WasmBackend::new();

        let result = backend.generate(&module);
        assert!(result.is_ok());

        let code = result.unwrap();
        let output = code.as_string().unwrap();

        assert!(output.contains("(module"));
        assert!(output.contains("(func $add"));
        assert!(output.contains("(export \"add\")"));
        assert!(output.contains("(memory"));
    }

    #[test]
    fn test_generate_wasi_module() {
        let module = create_simple_module();
        let mut backend = WasmBackend::with_wasi();

        let result = backend.generate(&module);
        assert!(result.is_ok());

        let code = result.unwrap();
        let output = code.as_string().unwrap();

        assert!(output.contains("wasi_snapshot_preview1"));
        assert!(output.contains("$__wasi_fd_write"));
        assert!(output.contains("$__wasi_proc_exit"));
    }

    #[test]
    fn test_backend_target() {
        let backend = WasmBackend::new();
        assert_eq!(backend.target(), Target::Wasm);
    }

    #[test]
    fn test_wasi_helpers() {
        let mut module = MirModule::new("test");

        let sig = MirFnSig::new(vec![], MirType::i32());
        let mut func = MirFunction::new("main", sig);
        func.is_public = true;

        let mut block = MirBlock::new(BlockId::ENTRY);
        block.set_terminator(MirTerminator::Return(Some(MirValue::Const(MirConst::Int(0, MirType::i32())))));
        func.add_block(block);

        module.add_function(func);

        let mut backend = WasmBackend::with_wasi();
        let result = backend.generate(&module);
        assert!(result.is_ok());

        let code = result.unwrap();
        let output = code.as_string().unwrap();

        assert!(output.contains("$__quanta_print"));
        assert!(output.contains("$__quanta_println"));
        assert!(output.contains("(export \"_start\")"));
    }
}
