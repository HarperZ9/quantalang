// ===============================================================================
// QUANTALANG CODE GENERATOR - LLVM BACKEND
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! LLVM IR code generation backend.
//!
//! This module generates LLVM IR textual representation from MIR.
//! The output can be consumed by LLVM tools (llc, opt, clang) to produce
//! native code or further optimized IR.
//!
//! ## Features
//!
//! - Full MIR to LLVM IR translation
//! - SSA form preservation
//! - Type mapping (MIR types to LLVM types)
//! - Function and calling convention support
//! - Global variables and string literals
//! - All arithmetic, comparison, and logical operations
//! - Control flow (branches, switches, function calls)
//! - Memory operations (alloca, load, store, GEP)
//! - Aggregate types (structs, arrays)

use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Arc;

use super::{Backend, CodegenError, CodegenResult, Target};
use crate::codegen::ir::*;
use crate::codegen::{GeneratedCode, OutputFormat};

// =============================================================================
// LLVM BACKEND
// =============================================================================

/// LLVM IR code generation backend.
pub struct LlvmBackend {
    /// Target triple (e.g., "x86_64-unknown-linux-gnu").
    target_triple: String,
    /// Data layout string.
    data_layout: String,
    /// Optimization level (0-3).
    opt_level: u32,
    /// Enable debug info.
    debug_info: bool,
    /// Output buffer.
    output: String,
    /// Value name counter for SSA.
    value_counter: u32,
    /// Block label counter.
    block_counter: u32,
    /// String literal counter.
    string_counter: u32,
    /// Mapping from LocalId to LLVM value names.
    local_names: HashMap<LocalId, String>,
    /// Mapping from string indices to global names.
    string_globals: HashMap<u32, String>,
    /// Current function being generated.
    current_function: Option<Arc<str>>,
    /// Cached type definitions from the module (for field lookups).
    type_defs: Vec<MirTypeDef>,
}

impl LlvmBackend {
    /// Create a new LLVM backend with default settings.
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        let default_triple = "x86_64-pc-windows-msvc";
        #[cfg(target_os = "linux")]
        let default_triple = "x86_64-unknown-linux-gnu";
        #[cfg(target_os = "macos")]
        let default_triple = "aarch64-apple-darwin";
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        let default_triple = "x86_64-unknown-linux-gnu";

        #[cfg(target_os = "windows")]
        let default_data_layout = "e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128";
        #[cfg(target_os = "macos")]
        let default_data_layout = "e-m:o-i64:64-i128:128-n32:64-S128-Fn32";
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let default_data_layout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128";

        Self {
            target_triple: default_triple.into(),
            data_layout: default_data_layout.into(),
            opt_level: 0,
            debug_info: false,
            output: String::new(),
            value_counter: 0,
            block_counter: 0,
            string_counter: 0,
            local_names: HashMap::new(),
            string_globals: HashMap::new(),
            current_function: None,
            type_defs: Vec::new(),
        }
    }

    /// Set the target triple.
    pub fn with_target_triple(mut self, triple: impl Into<String>) -> Self {
        self.target_triple = triple.into();
        self
    }

    /// Set the data layout.
    pub fn with_data_layout(mut self, layout: impl Into<String>) -> Self {
        self.data_layout = layout.into();
        self
    }

    /// Set optimization level.
    pub fn with_opt_level(mut self, level: u32) -> Self {
        self.opt_level = level.min(3);
        self
    }

    /// Enable debug info generation.
    pub fn with_debug_info(mut self, enable: bool) -> Self {
        self.debug_info = enable;
        self
    }

    /// Generate a fresh SSA value name.
    fn fresh_value(&mut self) -> String {
        let name = format!("%{}", self.value_counter);
        self.value_counter += 1;
        name
    }

    /// Generate a fresh block label for synthesized blocks (assert failures, etc.).
    /// Uses a distinct prefix to avoid collisions with MIR block labels (bb0, bb1, ...).
    fn fresh_block(&mut self) -> String {
        let name = format!("_synth{}", self.block_counter);
        self.block_counter += 1;
        name
    }

    /// Reset per-function state.
    fn reset_function_state(&mut self) {
        self.value_counter = 0;
        self.local_names.clear();
    }

    // =========================================================================
    // MODULE GENERATION
    // =========================================================================

    /// Generate the module header.
    fn gen_module_header(&mut self, module: &MirModule) {
        writeln!(&mut self.output, "; QuantaLang LLVM IR Output").unwrap();
        writeln!(&mut self.output, "; Module: {}", module.name).unwrap();
        writeln!(&mut self.output).unwrap();
        writeln!(&mut self.output, "source_filename = \"{}\"", module.name).unwrap();
        writeln!(&mut self.output, "target datalayout = \"{}\"", self.data_layout).unwrap();
        writeln!(&mut self.output, "target triple = \"{}\"", self.target_triple).unwrap();
        writeln!(&mut self.output).unwrap();
    }

    /// Generate string literal globals.
    fn gen_string_literals(&mut self, module: &MirModule) {
        for (idx, s) in module.strings.iter().enumerate() {
            let global_name = format!("@.str.{}", idx);
            self.string_globals.insert(idx as u32, global_name.clone());

            // Escape the string for LLVM
            let escaped = self.escape_string(s);
            let len = s.len() + 1; // Include null terminator

            writeln!(
                &mut self.output,
                "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\", align 1",
                global_name, len, escaped
            ).unwrap();
        }

        if !module.strings.is_empty() {
            writeln!(&mut self.output).unwrap();
        }
    }

    /// Escape a string for LLVM IR.
    fn escape_string(&self, s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                '\n' => result.push_str("\\0A"),
                '\r' => result.push_str("\\0D"),
                '\t' => result.push_str("\\09"),
                '"' => result.push_str("\\22"),
                '\\' => result.push_str("\\5C"),
                c if c.is_ascii_graphic() || c == ' ' => result.push(c),
                c => {
                    for b in c.to_string().as_bytes() {
                        write!(&mut result, "\\{:02X}", b).unwrap();
                    }
                }
            }
        }
        result
    }

    /// Generate global variables.
    fn gen_globals(&mut self, module: &MirModule) -> CodegenResult<()> {
        for global in &module.globals {
            let ty = self.llvm_type(&global.ty)?;
            let linkage = self.llvm_linkage(global.linkage);

            let init = if let Some(init) = &global.init {
                self.llvm_const(init)?
            } else {
                format!("zeroinitializer")
            };

            let mutability = if global.is_mut { "" } else { "constant " };
            let align = self.type_align(&global.ty);

            writeln!(
                &mut self.output,
                "@{} = {} {}{} {}, align {}",
                global.name,
                linkage,
                mutability,
                ty,
                init,
                align
            ).unwrap();
        }

        if !module.globals.is_empty() {
            writeln!(&mut self.output).unwrap();
        }

        Ok(())
    }

    /// Generate type definitions.
    fn gen_type_defs(&mut self, module: &MirModule) -> CodegenResult<()> {
        for typedef in &module.types {
            match &typedef.kind {
                TypeDefKind::Struct { fields, packed } => {
                    let fields_str: Vec<String> = fields
                        .iter()
                        .map(|(_, ty)| self.llvm_type(ty))
                        .collect::<Result<Vec<_>, _>>()?;

                    let body = if *packed {
                        format!("<{{ {} }}>", fields_str.join(", "))
                    } else {
                        format!("{{ {} }}", fields_str.join(", "))
                    };

                    writeln!(&mut self.output, "%{} = type {}", typedef.name, body).unwrap();
                }
                TypeDefKind::Union { variants } => {
                    // Unions are represented as the largest variant
                    let max_size = variants
                        .iter()
                        .map(|(_, ty)| self.type_size(ty))
                        .max()
                        .unwrap_or(0);

                    writeln!(
                        &mut self.output,
                        "%{} = type {{ [{} x i8] }}",
                        typedef.name, max_size
                    ).unwrap();
                }
                TypeDefKind::Enum { discriminant_ty, variants } => {
                    // Enums: discriminant + union of payloads
                    let disc_ty = self.llvm_type(discriminant_ty)?;

                    let max_payload_size = variants
                        .iter()
                        .map(|v| {
                            v.fields.iter().map(|(_, ty)| self.type_size(ty)).sum::<u64>()
                        })
                        .max()
                        .unwrap_or(0);

                    writeln!(
                        &mut self.output,
                        "%{} = type {{ {}, [{} x i8] }}",
                        typedef.name, disc_ty, max_payload_size
                    ).unwrap();
                }
            }
        }

        if !module.types.is_empty() {
            writeln!(&mut self.output).unwrap();
        }

        Ok(())
    }

    /// Generate external declarations.
    fn gen_externals(&mut self, module: &MirModule) -> CodegenResult<()> {
        for ext in &module.externals {
            match &ext.kind {
                ExternalKind::Function(sig) => {
                    let ret_ty = self.llvm_type(&sig.ret)?;
                    let params: Vec<String> = sig.params
                        .iter()
                        .map(|p| self.llvm_type(p))
                        .collect::<Result<Vec<_>, _>>()?;

                    let variadic = if sig.is_variadic { ", ..." } else { "" };

                    writeln!(
                        &mut self.output,
                        "declare {} @{}({}{})",
                        ret_ty, ext.name, params.join(", "), variadic
                    ).unwrap();
                }
                ExternalKind::Global(ty) => {
                    let llvm_ty = self.llvm_type(ty)?;
                    writeln!(
                        &mut self.output,
                        "@{} = external global {}",
                        ext.name, llvm_ty
                    ).unwrap();
                }
            }
        }

        if !module.externals.is_empty() {
            writeln!(&mut self.output).unwrap();
        }

        Ok(())
    }

    /// Generate LLVM intrinsics declarations.
    fn gen_intrinsics(&mut self) {
        writeln!(&mut self.output, "; LLVM Intrinsics").unwrap();

        // Memory intrinsics
        writeln!(&mut self.output, "declare void @llvm.memcpy.p0.p0.i64(ptr nocapture writeonly, ptr nocapture readonly, i64, i1 immarg) nounwind").unwrap();
        writeln!(&mut self.output, "declare void @llvm.memmove.p0.p0.i64(ptr nocapture writeonly, ptr nocapture readonly, i64, i1 immarg) nounwind").unwrap();
        writeln!(&mut self.output, "declare void @llvm.memset.p0.i64(ptr nocapture writeonly, i8, i64, i1 immarg) nounwind").unwrap();

        // Math intrinsics (f32)
        writeln!(&mut self.output, "declare float @llvm.sqrt.f32(float) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare float @llvm.sin.f32(float) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare float @llvm.cos.f32(float) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare float @llvm.pow.f32(float, float) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare float @llvm.exp.f32(float) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare float @llvm.log.f32(float) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare float @llvm.fabs.f32(float) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare float @llvm.floor.f32(float) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare float @llvm.ceil.f32(float) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare float @llvm.round.f32(float) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare float @llvm.fma.f32(float, float, float) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare float @llvm.minnum.f32(float, float) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare float @llvm.maxnum.f32(float, float) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare float @llvm.copysign.f32(float, float) nounwind readnone").unwrap();

        // Math intrinsics (f64)
        writeln!(&mut self.output, "declare double @llvm.sqrt.f64(double) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare double @llvm.sin.f64(double) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare double @llvm.cos.f64(double) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare double @llvm.pow.f64(double, double) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare double @llvm.exp.f64(double) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare double @llvm.log.f64(double) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare double @llvm.fabs.f64(double) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare double @llvm.floor.f64(double) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare double @llvm.ceil.f64(double) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare double @llvm.round.f64(double) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare double @llvm.fma.f64(double, double, double) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare double @llvm.minnum.f64(double, double) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare double @llvm.maxnum.f64(double, double) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare double @llvm.copysign.f64(double, double) nounwind readnone").unwrap();

        // Bit manipulation intrinsics
        writeln!(&mut self.output, "declare i8 @llvm.ctpop.i8(i8) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i16 @llvm.ctpop.i16(i16) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i32 @llvm.ctpop.i32(i32) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i64 @llvm.ctpop.i64(i64) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i32 @llvm.ctlz.i32(i32, i1 immarg) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i64 @llvm.ctlz.i64(i64, i1 immarg) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i32 @llvm.cttz.i32(i32, i1 immarg) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i64 @llvm.cttz.i64(i64, i1 immarg) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i32 @llvm.bswap.i32(i32) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i64 @llvm.bswap.i64(i64) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i32 @llvm.bitreverse.i32(i32) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i64 @llvm.bitreverse.i64(i64) nounwind readnone").unwrap();

        // Overflow-checking arithmetic
        writeln!(&mut self.output, "declare {{i32, i1}} @llvm.sadd.with.overflow.i32(i32, i32) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare {{i64, i1}} @llvm.sadd.with.overflow.i64(i64, i64) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare {{i32, i1}} @llvm.uadd.with.overflow.i32(i32, i32) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare {{i64, i1}} @llvm.uadd.with.overflow.i64(i64, i64) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare {{i32, i1}} @llvm.ssub.with.overflow.i32(i32, i32) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare {{i64, i1}} @llvm.ssub.with.overflow.i64(i64, i64) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare {{i32, i1}} @llvm.smul.with.overflow.i32(i32, i32) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare {{i64, i1}} @llvm.smul.with.overflow.i64(i64, i64) nounwind readnone").unwrap();

        // Saturating arithmetic
        writeln!(&mut self.output, "declare i32 @llvm.sadd.sat.i32(i32, i32) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i64 @llvm.sadd.sat.i64(i64, i64) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i32 @llvm.uadd.sat.i32(i32, i32) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i64 @llvm.uadd.sat.i64(i64, i64) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i32 @llvm.ssub.sat.i32(i32, i32) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i64 @llvm.ssub.sat.i64(i64, i64) nounwind readnone").unwrap();

        // Lifetime intrinsics
        writeln!(&mut self.output, "declare void @llvm.lifetime.start.p0(i64 immarg, ptr nocapture) nounwind").unwrap();
        writeln!(&mut self.output, "declare void @llvm.lifetime.end.p0(i64 immarg, ptr nocapture) nounwind").unwrap();

        // Debug intrinsics
        writeln!(&mut self.output, "declare void @llvm.dbg.declare(metadata, metadata, metadata) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare void @llvm.dbg.value(metadata, metadata, metadata) nounwind readnone").unwrap();

        // Trap and unreachable
        writeln!(&mut self.output, "declare void @llvm.trap() cold noreturn nounwind").unwrap();
        writeln!(&mut self.output, "declare void @llvm.debugtrap() nounwind").unwrap();

        // Stack operations
        writeln!(&mut self.output, "declare ptr @llvm.stacksave.p0() nounwind").unwrap();
        writeln!(&mut self.output, "declare void @llvm.stackrestore.p0(ptr) nounwind").unwrap();

        // Assume and expect (for optimization hints)
        writeln!(&mut self.output, "declare void @llvm.assume(i1) nounwind").unwrap();
        writeln!(&mut self.output, "declare i1 @llvm.expect.i1(i1, i1) nounwind readnone").unwrap();
        writeln!(&mut self.output, "declare i64 @llvm.expect.i64(i64, i64) nounwind readnone").unwrap();

        // Prefetch
        writeln!(&mut self.output, "declare void @llvm.prefetch.p0(ptr, i32 immarg, i32 immarg, i32 immarg) nounwind").unwrap();

        writeln!(&mut self.output).unwrap();
    }

    /// Generate atomic fence instruction.
    fn gen_atomic_fence(&mut self, ordering: &str) {
        writeln!(&mut self.output, "  fence {}", ordering).unwrap();
    }

    /// Generate atomic load instruction.
    fn gen_atomic_load(&mut self, dest: &str, ptr: &str, ty: &str, ordering: &str) {
        writeln!(
            &mut self.output,
            "  {} = load atomic {}, ptr {} {}",
            dest, ty, ptr, ordering
        ).unwrap();
    }

    /// Generate atomic store instruction.
    fn gen_atomic_store(&mut self, val: &str, ptr: &str, ty: &str, ordering: &str) {
        writeln!(
            &mut self.output,
            "  store atomic {} {}, ptr {} {}",
            ty, val, ptr, ordering
        ).unwrap();
    }

    /// Generate atomic compare-and-exchange instruction.
    fn gen_atomic_cmpxchg(&mut self, dest: &str, ptr: &str, cmp: &str, new: &str, ty: &str,
                          success_ordering: &str, failure_ordering: &str) {
        writeln!(
            &mut self.output,
            "  {} = cmpxchg ptr {}, {} {}, {} {} {} {}",
            dest, ptr, ty, cmp, ty, new, success_ordering, failure_ordering
        ).unwrap();
    }

    /// Generate atomic read-modify-write instruction.
    fn gen_atomic_rmw(&mut self, dest: &str, op: &str, ptr: &str, val: &str, ty: &str, ordering: &str) {
        writeln!(
            &mut self.output,
            "  {} = atomicrmw {} ptr {}, {} {} {}",
            dest, op, ptr, ty, val, ordering
        ).unwrap();
    }

    // =========================================================================
    // VECTOR OPERATIONS
    // =========================================================================

    /// Generate a vector splat (broadcast scalar to all lanes).
    #[allow(dead_code)]
    fn gen_vector_splat(&mut self, dest: &str, scalar: &str, vec_ty: &str, lanes: u32) {
        // First insert into undef, then shuffle to broadcast
        let tmp1 = self.fresh_value();
        writeln!(
            &mut self.output,
            "  {} = insertelement {} undef, {} {}, i32 0",
            tmp1, vec_ty, scalar.split_whitespace().next().unwrap_or("i32"), scalar
        ).unwrap();

        // Create shuffle mask of all zeros to broadcast lane 0
        let mask: Vec<String> = (0..lanes).map(|_| "i32 0".to_string()).collect();
        writeln!(
            &mut self.output,
            "  {} = shufflevector {} {}, {} undef, <{} x i32> <{}>",
            dest, vec_ty, tmp1, vec_ty, lanes, mask.join(", ")
        ).unwrap();
    }

    /// Generate vector element extraction.
    #[allow(dead_code)]
    fn gen_vector_extract(&mut self, dest: &str, vec: &str, vec_ty: &str, idx: &str, elem_ty: &str) {
        writeln!(
            &mut self.output,
            "  {} = extractelement {} {}, i32 {}",
            dest, vec_ty, vec, idx
        ).unwrap();
        let _ = elem_ty; // Used for type checking in caller
    }

    /// Generate vector element insertion.
    #[allow(dead_code)]
    fn gen_vector_insert(&mut self, dest: &str, vec: &str, vec_ty: &str, val: &str, idx: &str) {
        writeln!(
            &mut self.output,
            "  {} = insertelement {} {}, {} {}, i32 {}",
            dest, vec_ty, vec,
            val.split_whitespace().next().unwrap_or("i32"), val, idx
        ).unwrap();
    }

    /// Generate vector shuffle.
    #[allow(dead_code)]
    fn gen_vector_shuffle(&mut self, dest: &str, v1: &str, v2: &str, vec_ty: &str, mask: &[i32]) {
        let mask_str: Vec<String> = mask.iter().map(|&i| {
            if i < 0 { "undef".to_string() } else { format!("i32 {}", i) }
        }).collect();
        let lanes = mask.len();
        writeln!(
            &mut self.output,
            "  {} = shufflevector {} {}, {} {}, <{} x i32> <{}>",
            dest, vec_ty, v1, vec_ty, v2, lanes, mask_str.join(", ")
        ).unwrap();
    }

    /// Generate vector arithmetic operation.
    #[allow(dead_code)]
    fn gen_vector_arith(&mut self, dest: &str, op: &str, v1: &str, v2: &str, vec_ty: &str) {
        writeln!(
            &mut self.output,
            "  {} = {} {} {}, {}",
            dest, op, vec_ty, v1, v2
        ).unwrap();
    }

    /// Generate vector comparison.
    #[allow(dead_code)]
    fn gen_vector_cmp(&mut self, dest: &str, pred: &str, v1: &str, v2: &str, vec_ty: &str, is_float: bool) {
        let op = if is_float { "fcmp" } else { "icmp" };
        writeln!(
            &mut self.output,
            "  {} = {} {} {} {}, {}",
            dest, op, pred, vec_ty, v1, v2
        ).unwrap();
    }

    /// Generate vector select (blend).
    #[allow(dead_code)]
    fn gen_vector_select(&mut self, dest: &str, cond: &str, v1: &str, v2: &str, cond_ty: &str, vec_ty: &str) {
        writeln!(
            &mut self.output,
            "  {} = select {} {}, {} {}, {} {}",
            dest, cond_ty, cond, vec_ty, v1, vec_ty, v2
        ).unwrap();
    }

    /// Generate vector reduction (horizontal operation).
    #[allow(dead_code)]
    fn gen_vector_reduce(&mut self, dest: &str, op: &str, vec: &str, vec_ty: &str, scalar_ty: &str) {
        // LLVM reduction intrinsics
        let intrinsic = match op {
            "add" => "llvm.vector.reduce.add",
            "mul" => "llvm.vector.reduce.mul",
            "and" => "llvm.vector.reduce.and",
            "or" => "llvm.vector.reduce.or",
            "xor" => "llvm.vector.reduce.xor",
            "smax" => "llvm.vector.reduce.smax",
            "smin" => "llvm.vector.reduce.smin",
            "umax" => "llvm.vector.reduce.umax",
            "umin" => "llvm.vector.reduce.umin",
            "fadd" => "llvm.vector.reduce.fadd",
            "fmul" => "llvm.vector.reduce.fmul",
            "fmax" => "llvm.vector.reduce.fmax",
            "fmin" => "llvm.vector.reduce.fmin",
            _ => panic!("Unknown reduction op: {}", op),
        };

        writeln!(
            &mut self.output,
            "  {} = call {} @{}.{}({} {})",
            dest, scalar_ty, intrinsic, vec_ty.replace('<', "v").replace('>', "").replace(" x ", ""),
            vec_ty, vec
        ).unwrap();
    }

    /// Generate FMA (fused multiply-add) vector operation.
    #[allow(dead_code)]
    fn gen_vector_fma(&mut self, dest: &str, a: &str, b: &str, c: &str, vec_ty: &str) {
        writeln!(
            &mut self.output,
            "  {} = call {} @llvm.fma.{}({} {}, {} {}, {} {})",
            dest, vec_ty, vec_ty.replace('<', "v").replace('>', "").replace(" x ", ""),
            vec_ty, a, vec_ty, b, vec_ty, c
        ).unwrap();
    }

    // =========================================================================
    // FUNCTION GENERATION
    // =========================================================================

    /// Generate a function.
    fn gen_function(&mut self, func: &MirFunction) -> CodegenResult<()> {
        self.reset_function_state();
        self.current_function = Some(func.name.clone());

        let ret_ty = self.llvm_type(&func.sig.ret)?;
        let linkage = self.llvm_linkage(func.linkage);
        let calling_conv = self.llvm_calling_conv(func.sig.calling_conv);

        // Generate parameter list
        let mut params = Vec::new();
        for (i, param_ty) in func.sig.params.iter().enumerate() {
            let ty = self.llvm_type(param_ty)?;
            let name = format!("%arg{}", i);
            params.push(format!("{} {}", ty, name));
        }

        let variadic = if func.sig.is_variadic { ", ..." } else { "" };

        if func.is_declaration() {
            // External declaration
            writeln!(
                &mut self.output,
                "declare {} {} @{}({}{}) {}",
                calling_conv, ret_ty, func.name, params.join(", "), variadic,
                if func.sig.is_variadic { "" } else { "nounwind" }
            ).unwrap();
            writeln!(&mut self.output).unwrap();
            return Ok(());
        }

        // Function definition
        write!(
            &mut self.output,
            "define {} {} @{}({}{}) {}",
            linkage, ret_ty, func.name, params.join(", "), variadic,
            calling_conv
        ).unwrap();

        // Function attributes
        if self.opt_level > 0 {
            write!(&mut self.output, " #0").unwrap();
        }

        writeln!(&mut self.output, " {{").unwrap();

        // Generate locals as alloca
        self.gen_locals(func)?;

        // Store parameters to locals
        self.gen_param_stores(func)?;

        // Generate basic blocks
        if let Some(blocks) = &func.blocks {
            for block in blocks {
                self.gen_block(block, func)?;
            }
        }

        writeln!(&mut self.output, "}}").unwrap();
        writeln!(&mut self.output).unwrap();

        self.current_function = None;
        Ok(())
    }

    /// Generate local variable allocations.
    fn gen_locals(&mut self, func: &MirFunction) -> CodegenResult<()> {
        // Entry block label
        writeln!(&mut self.output, "entry:").unwrap();

        for local in &func.locals {
            if local.is_param {
                continue; // Parameters handled separately
            }

            let ty = self.llvm_type(&local.ty)?;
            let local_align = self.type_align(&local.ty);
            let name = format!("%local{}", local.id.0);
            self.local_names.insert(local.id, name.clone());

            writeln!(
                &mut self.output,
                "  {} = alloca {}, align {}",
                name, ty, local_align
            ).unwrap();
        }

        Ok(())
    }

    /// Generate parameter stores.
    fn gen_param_stores(&mut self, func: &MirFunction) -> CodegenResult<()> {
        for (i, local) in func.locals.iter().filter(|l| l.is_param).enumerate() {
            let ty = self.llvm_type(&local.ty)?;
            let local_align = self.type_align(&local.ty);
            let local_name = format!("%local{}", local.id.0);
            self.local_names.insert(local.id, local_name.clone());

            // Alloca for parameter
            writeln!(
                &mut self.output,
                "  {} = alloca {}, align {}",
                local_name, ty, local_align
            ).unwrap();

            // Store parameter value
            writeln!(
                &mut self.output,
                "  store {} %arg{}, ptr {}, align {}",
                ty, i, local_name, local_align
            ).unwrap();
        }

        // Jump to first real block if we have blocks
        if func.blocks.as_ref().map(|b| !b.is_empty()).unwrap_or(false) {
            writeln!(&mut self.output, "  br label %bb0").unwrap();
        }

        Ok(())
    }

    /// Generate a basic block.
    fn gen_block(&mut self, block: &MirBlock, func: &MirFunction) -> CodegenResult<()> {
        // Block label
        let label = block.label.as_ref()
            .map(|l| l.to_string())
            .unwrap_or_else(|| format!("bb{}", block.id.0));

        writeln!(&mut self.output).unwrap();
        writeln!(&mut self.output, "{}:", label).unwrap();

        // Generate statements
        for stmt in &block.stmts {
            self.gen_stmt(stmt, func)?;
        }

        // Generate terminator
        if let Some(term) = &block.terminator {
            self.gen_terminator(term, func)?;
        } else {
            // Unreachable if no terminator
            writeln!(&mut self.output, "  unreachable").unwrap();
        }

        Ok(())
    }

    /// Generate a statement.
    fn gen_stmt(&mut self, stmt: &MirStmt, func: &MirFunction) -> CodegenResult<()> {
        match &stmt.kind {
            MirStmtKind::Assign { dest, value } => {
                self.gen_assign(*dest, value, func)?;
            }
            MirStmtKind::DerefAssign { .. } | MirStmtKind::FieldDerefAssign { .. } => {
                // TODO: implement pointer store for LLVM
            }
            MirStmtKind::StorageLive(local) => {
                let name = self.get_local_name(*local)?;
                writeln!(&mut self.output, "  ; storage_live {}", name).unwrap();
            }
            MirStmtKind::StorageDead(local) => {
                let name = self.get_local_name(*local)?;
                writeln!(&mut self.output, "  ; storage_dead {}", name).unwrap();
            }
            MirStmtKind::Nop => {
                // No-op, nothing to generate
            }
        }
        Ok(())
    }

    /// Generate an assignment.
    fn gen_assign(&mut self, dest: LocalId, rvalue: &MirRValue, func: &MirFunction) -> CodegenResult<()> {
        let dest_name = self.get_local_name(dest)?;
        let dest_ty = self.get_local_type(dest, func)?;
        let llvm_ty = self.llvm_type(&dest_ty)?;
        let dest_align = self.type_align(&dest_ty);

        match rvalue {
            MirRValue::Use(value) => {
                let val = self.gen_value(value, func)?;
                writeln!(
                    &mut self.output,
                    "  store {} {}, ptr {}, align {}",
                    llvm_ty, val, dest_name, dest_align
                ).unwrap();
            }
            MirRValue::BinaryOp { op, left, right } => {
                let left_val = self.gen_value(left, func)?;
                let right_val = self.gen_value(right, func)?;
                let result = self.fresh_value();

                let instr = self.llvm_binop(*op, &dest_ty)?;
                let dest_align = self.type_align(&dest_ty);
                writeln!(
                    &mut self.output,
                    "  {} = {} {} {}, {}",
                    result, instr, llvm_ty, left_val, right_val
                ).unwrap();

                writeln!(
                    &mut self.output,
                    "  store {} {}, ptr {}, align {}",
                    llvm_ty, result, dest_name, dest_align
                ).unwrap();
            }
            MirRValue::UnaryOp { op, operand } => {
                let operand_val = self.gen_value(operand, func)?;
                let dest_align = self.type_align(&dest_ty);
                let result = self.fresh_value();

                match op {
                    UnaryOp::Neg => {
                        if dest_ty.is_float() {
                            writeln!(
                                &mut self.output,
                                "  {} = fneg {} {}",
                                result, llvm_ty, operand_val
                            ).unwrap();
                        } else {
                            writeln!(
                                &mut self.output,
                                "  {} = sub {} 0, {}",
                                result, llvm_ty, operand_val
                            ).unwrap();
                        }
                    }
                    UnaryOp::Not => {
                        if dest_ty == MirType::Bool {
                            writeln!(
                                &mut self.output,
                                "  {} = xor i1 {}, true",
                                result, operand_val
                            ).unwrap();
                        } else {
                            writeln!(
                                &mut self.output,
                                "  {} = xor {} {}, -1",
                                result, llvm_ty, operand_val
                            ).unwrap();
                        }
                    }
                }

                writeln!(
                    &mut self.output,
                    "  store {} {}, ptr {}, align {}",
                    llvm_ty, result, dest_name, dest_align
                ).unwrap();
            }
            MirRValue::Ref { is_mut: _, place } => {
                let ptr = self.gen_place_addr(place, func)?;
                writeln!(
                    &mut self.output,
                    "  store ptr {}, ptr {}, align 8",
                    ptr, dest_name
                ).unwrap();
            }
            MirRValue::AddressOf { is_mut: _, place } => {
                let ptr = self.gen_place_addr(place, func)?;
                writeln!(
                    &mut self.output,
                    "  store ptr {}, ptr {}, align 8",
                    ptr, dest_name
                ).unwrap();
            }
            MirRValue::Cast { kind, value, ty } => {
                let val = self.gen_value(value, func)?;
                let from_ty = self.infer_value_type(value, func)?;
                let to_ty = self.llvm_type(ty)?;
                let from_llvm = self.llvm_type(&from_ty)?;
                let ty_align = self.type_align(ty);
                let result = self.fresh_value();

                let instr = self.llvm_cast(*kind, &from_ty, ty)?;
                writeln!(
                    &mut self.output,
                    "  {} = {} {} {} to {}",
                    result, instr, from_llvm, val, to_ty
                ).unwrap();

                writeln!(
                    &mut self.output,
                    "  store {} {}, ptr {}, align {}",
                    to_ty, result, dest_name, ty_align
                ).unwrap();
            }
            MirRValue::Aggregate { kind, operands } => {
                self.gen_aggregate(dest, dest_name.clone(), kind, operands, func)?;
            }
            MirRValue::Repeat { value, count } => {
                // Array repeat [value; count]
                let val = self.gen_value(value, func)?;
                let elem_ty = self.infer_value_type(value, func)?;
                let elem_llvm = self.llvm_type(&elem_ty)?;
                let elem_align = self.type_align(&elem_ty);

                for i in 0..*count {
                    let gep = self.fresh_value();
                    writeln!(
                        &mut self.output,
                        "  {} = getelementptr inbounds {}, ptr {}, i64 {}",
                        gep, elem_llvm, dest_name, i
                    ).unwrap();
                    writeln!(
                        &mut self.output,
                        "  store {} {}, ptr {}, align {}",
                        elem_llvm, val, gep, elem_align
                    ).unwrap();
                }
            }
            MirRValue::Discriminant(place) => {
                let ptr = self.gen_place_addr(place, func)?;
                let dest_align = self.type_align(&dest_ty);
                let disc_ptr = self.fresh_value();
                let disc_val = self.fresh_value();

                writeln!(
                    &mut self.output,
                    "  {} = getelementptr inbounds {{ i32, [0 x i8] }}, ptr {}, i32 0, i32 0",
                    disc_ptr, ptr
                ).unwrap();
                writeln!(
                    &mut self.output,
                    "  {} = load i32, ptr {}, align 4",
                    disc_val, disc_ptr
                ).unwrap();
                writeln!(
                    &mut self.output,
                    "  store {} {}, ptr {}, align {}",
                    llvm_ty, disc_val, dest_name, dest_align
                ).unwrap();
            }
            MirRValue::Len(place) => {
                // For slices: load the length from fat pointer
                let ptr = self.gen_place_addr(place, func)?;
                let len_ptr = self.fresh_value();
                let len_val = self.fresh_value();

                writeln!(
                    &mut self.output,
                    "  {} = getelementptr inbounds {{ ptr, i64 }}, ptr {}, i32 0, i32 1",
                    len_ptr, ptr
                ).unwrap();
                writeln!(
                    &mut self.output,
                    "  {} = load i64, ptr {}, align 8",
                    len_val, len_ptr
                ).unwrap();
                writeln!(
                    &mut self.output,
                    "  store i64 {}, ptr {}, align 8",
                    len_val, dest_name
                ).unwrap();
            }
            MirRValue::NullaryOp(op, ty) => {
                let result = match op {
                    NullaryOp::SizeOf => self.type_size(ty),
                    NullaryOp::AlignOf => self.type_align(ty) as u64,
                };
                let dest_align = self.type_align(&dest_ty);
                writeln!(
                    &mut self.output,
                    "  store {} {}, ptr {}, align {}",
                    llvm_ty, result, dest_name, dest_align
                ).unwrap();
            }
            MirRValue::FieldAccess { base, field_name, field_ty } => {
                // Struct field access: GEP into the struct, then load the field value
                let base_val = self.gen_place_addr_from_value(base, func)?;
                let base_ty = self.infer_value_type(base, func)?;
                let field_llvm_ty = self.llvm_type(field_ty)?;
                let field_align = self.type_align(field_ty);

                // Determine the struct name and field index
                let struct_name = match &base_ty {
                    MirType::Struct(name) => name.clone(),
                    _ => {
                        // Fallback: treat base as a pointer and use field index 0
                        let field_ptr = self.fresh_value();
                        let field_val = self.fresh_value();
                        writeln!(
                            &mut self.output,
                            "  {} = getelementptr inbounds i8, ptr {}, i32 0",
                            field_ptr, base_val
                        ).unwrap();
                        writeln!(
                            &mut self.output,
                            "  {} = load {}, ptr {}, align {}",
                            field_val, field_llvm_ty, field_ptr, field_align
                        ).unwrap();
                        writeln!(
                            &mut self.output,
                            "  store {} {}, ptr {}, align {}",
                            llvm_ty, field_val, dest_name, dest_align
                        ).unwrap();
                        return Ok(());
                    }
                };

                let field_idx = self.find_struct_field_index(&struct_name, field_name)?;

                let field_ptr = self.fresh_value();
                let field_val = self.fresh_value();

                writeln!(
                    &mut self.output,
                    "  {} = getelementptr inbounds %{}, ptr {}, i32 0, i32 {}",
                    field_ptr, struct_name, base_val, field_idx
                ).unwrap();
                writeln!(
                    &mut self.output,
                    "  {} = load {}, ptr {}, align {}",
                    field_val, field_llvm_ty, field_ptr, field_align
                ).unwrap();
                writeln!(
                    &mut self.output,
                    "  store {} {}, ptr {}, align {}",
                    llvm_ty, field_val, dest_name, dest_align
                ).unwrap();
            }
            MirRValue::VariantField { base, variant_name: _, field_index, field_ty } => {
                // Tagged union variant field access:
                // 1. Get base address
                // 2. GEP past the discriminant to the payload area (field index 1)
                // 3. GEP into the payload at the field offset
                // 4. Load the field value
                let base_val = self.gen_place_addr_from_value(base, func)?;
                let base_ty = self.infer_value_type(base, func)?;
                let field_llvm_ty = self.llvm_type(field_ty)?;
                let field_align = self.type_align(field_ty);

                let enum_ty_name = match &base_ty {
                    MirType::Struct(name) => format!("%{}", name),
                    _ => "{ i32, [0 x i8] }".to_string(),
                };

                // GEP to payload area (index 1 in the enum struct)
                let payload_ptr = self.fresh_value();
                writeln!(
                    &mut self.output,
                    "  {} = getelementptr inbounds {}, ptr {}, i32 0, i32 1",
                    payload_ptr, enum_ty_name, base_val
                ).unwrap();

                // GEP to the specific field within the payload (byte offset)
                let field_ptr = self.fresh_value();
                let field_offset = *field_index as u64 * 8; // Simplified offset, matching aggregate layout
                writeln!(
                    &mut self.output,
                    "  {} = getelementptr inbounds i8, ptr {}, i64 {}",
                    field_ptr, payload_ptr, field_offset
                ).unwrap();

                // Load the field value
                let field_val = self.fresh_value();
                writeln!(
                    &mut self.output,
                    "  {} = load {}, ptr {}, align {}",
                    field_val, field_llvm_ty, field_ptr, field_align
                ).unwrap();

                writeln!(
                    &mut self.output,
                    "  store {} {}, ptr {}, align {}",
                    llvm_ty, field_val, dest_name, dest_align
                ).unwrap();
            }
            MirRValue::IndexAccess { base, index, elem_ty } => {
                // Array index access: GEP into the array, then load the element
                let base_val = self.gen_place_addr_from_value(base, func)?;
                let idx_val = self.gen_value(index, func)?;
                let elem_llvm_ty = self.llvm_type(elem_ty)?;
                let elem_align = self.type_align(elem_ty);

                let elem_ptr = self.fresh_value();
                writeln!(
                    &mut self.output,
                    "  {} = getelementptr inbounds {}, ptr {}, i64 {}",
                    elem_ptr, elem_llvm_ty, base_val, idx_val
                ).unwrap();

                let elem_val = self.fresh_value();
                writeln!(
                    &mut self.output,
                    "  {} = load {}, ptr {}, align {}",
                    elem_val, elem_llvm_ty, elem_ptr, elem_align
                ).unwrap();

                writeln!(
                    &mut self.output,
                    "  store {} {}, ptr {}, align {}",
                    llvm_ty, elem_val, dest_name, dest_align
                ).unwrap();
            }
            MirRValue::Deref { ptr, pointee_ty } => {
                let ptr_val = self.gen_value(ptr, func)?;
                let pointee_llvm_ty = self.llvm_type(pointee_ty)?;
                let pointee_align = self.type_align(pointee_ty);
                let loaded = self.fresh_value();
                writeln!(
                    &mut self.output,
                    "  {} = load {}, ptr {}, align {}",
                    loaded, pointee_llvm_ty, ptr_val, pointee_align
                ).unwrap();
                writeln!(
                    &mut self.output,
                    "  store {} {}, ptr {}, align {}",
                    llvm_ty, loaded, dest_name, dest_align
                ).unwrap();
            }
            MirRValue::TextureSample { .. } => {
                // GPU-only operation; store zeroed placeholder in LLVM IR
                writeln!(
                    &mut self.output,
                    "  ; texture_sample: GPU-only, not supported in LLVM backend"
                ).unwrap();
                writeln!(
                    &mut self.output,
                    "  store {} zeroinitializer, ptr {}, align {}",
                    llvm_ty, dest_name, dest_align
                ).unwrap();
            }
        }

        Ok(())
    }

    /// Generate aggregate initialization.
    fn gen_aggregate(
        &mut self,
        _dest: LocalId,
        dest_name: String,
        kind: &AggregateKind,
        operands: &[MirValue],
        func: &MirFunction,
    ) -> CodegenResult<()> {
        match kind {
            AggregateKind::Array(elem_ty) => {
                let elem_llvm = self.llvm_type(elem_ty)?;
                let elem_align = self.type_align(elem_ty);
                for (i, op) in operands.iter().enumerate() {
                    let val = self.gen_value(op, func)?;
                    let gep = self.fresh_value();
                    writeln!(
                        &mut self.output,
                        "  {} = getelementptr inbounds {}, ptr {}, i64 {}",
                        gep, elem_llvm, dest_name, i
                    ).unwrap();
                    writeln!(
                        &mut self.output,
                        "  store {} {}, ptr {}, align {}",
                        elem_llvm, val, gep, elem_align
                    ).unwrap();
                }
            }
            AggregateKind::Tuple => {
                // Tuple: store each element at offset
                for (i, op) in operands.iter().enumerate() {
                    let val = self.gen_value(op, func)?;
                    let ty = self.infer_value_type(op, func)?;
                    let llvm_ty = self.llvm_type(&ty)?;
                    let align = self.type_align(&ty);
                    let gep = self.fresh_value();

                    // For tuples, use byte offset
                    writeln!(
                        &mut self.output,
                        "  {} = getelementptr inbounds i8, ptr {}, i64 {}",
                        gep, dest_name, i * 8 // Simplified offset
                    ).unwrap();
                    writeln!(
                        &mut self.output,
                        "  store {} {}, ptr {}, align {}",
                        llvm_ty, val, gep, align
                    ).unwrap();
                }
            }
            AggregateKind::Struct(name) => {
                // Struct: use GEP with struct type
                for (i, op) in operands.iter().enumerate() {
                    let val = self.gen_value(op, func)?;
                    let ty = self.infer_value_type(op, func)?;
                    let llvm_ty = self.llvm_type(&ty)?;
                    let align = self.type_align(&ty);
                    let gep = self.fresh_value();

                    writeln!(
                        &mut self.output,
                        "  {} = getelementptr inbounds %{}, ptr {}, i32 0, i32 {}",
                        gep, name, dest_name, i
                    ).unwrap();
                    writeln!(
                        &mut self.output,
                        "  store {} {}, ptr {}, align {}",
                        llvm_ty, val, gep, align
                    ).unwrap();
                }
            }
            AggregateKind::Variant(name, discriminant, _variant_name) => {
                // Enum variant: store discriminant, then fields
                let disc_ptr = self.fresh_value();
                writeln!(
                    &mut self.output,
                    "  {} = getelementptr inbounds %{}, ptr {}, i32 0, i32 0",
                    disc_ptr, name, dest_name
                ).unwrap();
                writeln!(
                    &mut self.output,
                    "  store i32 {}, ptr {}, align 4",
                    discriminant, disc_ptr
                ).unwrap();

                // Store fields in payload area
                let payload_ptr = self.fresh_value();
                writeln!(
                    &mut self.output,
                    "  {} = getelementptr inbounds %{}, ptr {}, i32 0, i32 1",
                    payload_ptr, name, dest_name
                ).unwrap();

                for (i, op) in operands.iter().enumerate() {
                    let val = self.gen_value(op, func)?;
                    let ty = self.infer_value_type(op, func)?;
                    let llvm_ty = self.llvm_type(&ty)?;
                    let align = self.type_align(&ty);
                    let field_ptr = self.fresh_value();

                    writeln!(
                        &mut self.output,
                        "  {} = getelementptr inbounds i8, ptr {}, i64 {}",
                        field_ptr, payload_ptr, i * 8
                    ).unwrap();
                    writeln!(
                        &mut self.output,
                        "  store {} {}, ptr {}, align {}",
                        llvm_ty, val, field_ptr, align
                    ).unwrap();
                }
            }
            AggregateKind::Closure(name) => {
                // Closure: similar to struct
                for (i, op) in operands.iter().enumerate() {
                    let val = self.gen_value(op, func)?;
                    let ty = self.infer_value_type(op, func)?;
                    let llvm_ty = self.llvm_type(&ty)?;
                    let align = self.type_align(&ty);
                    let gep = self.fresh_value();

                    writeln!(
                        &mut self.output,
                        "  {} = getelementptr inbounds %{}, ptr {}, i32 0, i32 {}",
                        gep, name, dest_name, i
                    ).unwrap();
                    writeln!(
                        &mut self.output,
                        "  store {} {}, ptr {}, align {}",
                        llvm_ty, val, gep, align
                    ).unwrap();
                }
            }
        }

        Ok(())
    }

    /// Generate a terminator.
    fn gen_terminator(&mut self, term: &MirTerminator, func: &MirFunction) -> CodegenResult<()> {
        match term {
            MirTerminator::Goto(target) => {
                writeln!(&mut self.output, "  br label %bb{}", target.0).unwrap();
            }
            MirTerminator::If { cond, then_block, else_block } => {
                let cond_val = self.gen_value(cond, func)?;
                writeln!(
                    &mut self.output,
                    "  br i1 {}, label %bb{}, label %bb{}",
                    cond_val, then_block.0, else_block.0
                ).unwrap();
            }
            MirTerminator::Switch { value, targets, default } => {
                let val = self.gen_value(value, func)?;
                let ty = self.infer_value_type(value, func)?;
                let llvm_ty = self.llvm_type(&ty)?;

                write!(
                    &mut self.output,
                    "  switch {} {}, label %bb{} [",
                    llvm_ty, val, default.0
                ).unwrap();

                for (const_val, target) in targets {
                    let cv = self.llvm_const(const_val)?;
                    write!(
                        &mut self.output,
                        "\n    {} {}, label %bb{}",
                        llvm_ty, cv, target.0
                    ).unwrap();
                }

                writeln!(&mut self.output, "\n  ]").unwrap();
            }
            MirTerminator::Call { func: callee, args, dest, target, unwind: _ } => {
                let callee_val = self.gen_value(callee, func)?;

                let mut arg_strs = Vec::new();
                for arg in args {
                    let val = self.gen_value(arg, func)?;
                    let ty = self.infer_value_type(arg, func)?;
                    let llvm_ty = self.llvm_type(&ty)?;
                    arg_strs.push(format!("{} {}", llvm_ty, val));
                }

                if let Some(dest_local) = dest {
                    let dest_name = self.get_local_name(*dest_local)?;
                    let dest_ty = self.get_local_type(*dest_local, func)?;
                    let ret_ty = self.llvm_type(&dest_ty)?;
                    let dest_align = self.type_align(&dest_ty);
                    let result = self.fresh_value();

                    writeln!(
                        &mut self.output,
                        "  {} = call {} {}({})",
                        result, ret_ty, callee_val, arg_strs.join(", ")
                    ).unwrap();

                    writeln!(
                        &mut self.output,
                        "  store {} {}, ptr {}, align {}",
                        ret_ty, result, dest_name, dest_align
                    ).unwrap();
                } else {
                    // No destination: call and discard the return value.
                    // Use void for truly void functions, otherwise emit
                    // the call with the correct return type and drop the result.
                    let is_void_fn = match callee {
                        MirValue::Function(name) => {
                            // For implicitly declared functions (printf, etc.) or
                            // declared externals, we can't easily look up the return type here.
                            // For safety, emit `call i32` if it's a known C function,
                            // otherwise `call void`.
                            match name.as_ref() {
                                "printf" | "fprintf" | "sprintf" | "snprintf" |
                                "puts" | "putchar" | "getchar" => false,
                                _ => true,
                            }
                        }
                        _ => true,
                    };

                    if is_void_fn {
                        writeln!(
                            &mut self.output,
                            "  call void {}({})",
                            callee_val, arg_strs.join(", ")
                        ).unwrap();
                    } else {
                        // Emit call with i32 return type and discard the result
                        let _unused = self.fresh_value();
                        writeln!(
                            &mut self.output,
                            "  {} = call i32 {}({})",
                            _unused, callee_val, arg_strs.join(", ")
                        ).unwrap();
                    }
                }

                if let Some(target_block) = target {
                    writeln!(&mut self.output, "  br label %bb{}", target_block.0).unwrap();
                }
            }
            MirTerminator::Return(value) => {
                if let Some(val) = value {
                    let ret_val = self.gen_value(val, func)?;
                    let ret_ty = self.llvm_type(&func.sig.ret)?;
                    writeln!(&mut self.output, "  ret {} {}", ret_ty, ret_val).unwrap();
                } else {
                    writeln!(&mut self.output, "  ret void").unwrap();
                }
            }
            MirTerminator::Unreachable => {
                writeln!(&mut self.output, "  unreachable").unwrap();
            }
            MirTerminator::Drop { place: _, target, unwind: _ } => {
                // Drop is a no-op in LLVM IR (handled by runtime)
                writeln!(&mut self.output, "  br label %bb{}", target.0).unwrap();
            }
            MirTerminator::Assert { cond, expected, msg, target, unwind: _ } => {
                let cond_val = self.gen_value(cond, func)?;
                let fail_block = self.fresh_block();

                if *expected {
                    writeln!(
                        &mut self.output,
                        "  br i1 {}, label %bb{}, label %{}",
                        cond_val, target.0, fail_block
                    ).unwrap();
                } else {
                    writeln!(
                        &mut self.output,
                        "  br i1 {}, label %{}, label %bb{}",
                        cond_val, fail_block, target.0
                    ).unwrap();
                }

                // Generate fail block
                writeln!(&mut self.output).unwrap();
                writeln!(&mut self.output, "{}:", fail_block).unwrap();
                writeln!(
                    &mut self.output,
                    "  ; assertion failed: {}",
                    msg
                ).unwrap();
                writeln!(&mut self.output, "  call void @llvm.trap()").unwrap();
                writeln!(&mut self.output, "  unreachable").unwrap();
            }
            MirTerminator::Resume => {
                writeln!(&mut self.output, "  resume {{ ptr, i32 }} undef").unwrap();
            }
            MirTerminator::Abort => {
                writeln!(&mut self.output, "  call void @llvm.trap()").unwrap();
                writeln!(&mut self.output, "  unreachable").unwrap();
            }
        }

        Ok(())
    }

    // =========================================================================
    // VALUE AND PLACE GENERATION
    // =========================================================================

    /// Generate a value expression.
    fn gen_value(&mut self, value: &MirValue, func: &MirFunction) -> CodegenResult<String> {
        match value {
            MirValue::Local(id) => {
                let name = self.get_local_name(*id)?;
                let ty = self.get_local_type(*id, func)?;
                let llvm_ty = self.llvm_type(&ty)?;
                let align = self.type_align(&ty);
                let loaded = self.fresh_value();

                writeln!(
                    &mut self.output,
                    "  {} = load {}, ptr {}, align {}",
                    loaded, llvm_ty, name, align
                ).unwrap();

                Ok(loaded)
            }
            MirValue::Const(c) => self.llvm_const(c),
            MirValue::Global(name) => Ok(format!("@{}", name)),
            MirValue::Function(name) => Ok(format!("@{}", name)),
        }
    }

    /// Generate a place address.
    fn gen_place_addr(&mut self, place: &MirPlace, func: &MirFunction) -> CodegenResult<String> {
        let mut addr = self.get_local_name(place.local)?;
        let mut current_ty = self.get_local_type(place.local, func)?;

        for proj in &place.projections {
            match proj {
                PlaceProjection::Deref => {
                    let loaded = self.fresh_value();
                    writeln!(
                        &mut self.output,
                        "  {} = load ptr, ptr {}, align 8",
                        loaded, addr
                    ).unwrap();
                    addr = loaded;

                    // Update type to pointee
                    if let MirType::Ptr(inner) = current_ty {
                        current_ty = *inner;
                    }
                }
                PlaceProjection::Field(idx, field_ty) => {
                    let gep = self.fresh_value();
                    let struct_ty = self.llvm_type(&current_ty)?;

                    writeln!(
                        &mut self.output,
                        "  {} = getelementptr inbounds {}, ptr {}, i32 0, i32 {}",
                        gep, struct_ty, addr, idx
                    ).unwrap();

                    addr = gep;
                    current_ty = field_ty.clone();
                }
                PlaceProjection::Index(idx_local) => {
                    let idx = self.gen_value(&MirValue::Local(*idx_local), func)?;
                    let gep = self.fresh_value();

                    if let MirType::Array(elem_ty, _) = &current_ty {
                        let elem_llvm = self.llvm_type(elem_ty)?;
                        writeln!(
                            &mut self.output,
                            "  {} = getelementptr inbounds {}, ptr {}, i64 {}",
                            gep, elem_llvm, addr, idx
                        ).unwrap();
                        current_ty = (**elem_ty).clone();
                    }

                    addr = gep;
                }
                PlaceProjection::ConstantIndex { offset, from_end } => {
                    let gep = self.fresh_value();
                    let idx = if *from_end {
                        format!("-{}", offset)
                    } else {
                        offset.to_string()
                    };

                    if let MirType::Array(elem_ty, _) = &current_ty {
                        let elem_llvm = self.llvm_type(elem_ty)?;
                        writeln!(
                            &mut self.output,
                            "  {} = getelementptr inbounds {}, ptr {}, i64 {}",
                            gep, elem_llvm, addr, idx
                        ).unwrap();
                        current_ty = (**elem_ty).clone();
                    }

                    addr = gep;
                }
                PlaceProjection::Subslice { from, to: _, from_end: _ } => {
                    let gep = self.fresh_value();
                    writeln!(
                        &mut self.output,
                        "  {} = getelementptr inbounds i8, ptr {}, i64 {}",
                        gep, addr, from
                    ).unwrap();
                    addr = gep;
                }
                PlaceProjection::Downcast(variant_idx) => {
                    // For enums: skip discriminant and get payload
                    let payload = self.fresh_value();
                    writeln!(
                        &mut self.output,
                        "  {} = getelementptr inbounds {{ i32, [0 x i8] }}, ptr {}, i32 0, i32 1",
                        payload, addr
                    ).unwrap();
                    writeln!(
                        &mut self.output,
                        "  ; downcast to variant {}",
                        variant_idx
                    ).unwrap();
                    addr = payload;
                }
            }
        }

        Ok(addr)
    }

    /// Get the address of a value for use in GEP operations.
    /// For locals, returns the alloca pointer directly (no load).
    /// For globals, returns the global symbol.
    fn gen_place_addr_from_value(&mut self, value: &MirValue, _func: &MirFunction) -> CodegenResult<String> {
        match value {
            MirValue::Local(id) => self.get_local_name(*id),
            MirValue::Global(name) => Ok(format!("@{}", name)),
            _ => Err(CodegenError::Internal(
                "FieldAccess/IndexAccess base must be a local or global".into()
            )),
        }
    }

    // =========================================================================
    // TYPE CONVERSION
    // =========================================================================

    /// Convert MIR type to LLVM type string.
    fn llvm_type(&self, ty: &MirType) -> CodegenResult<String> {
        match ty {
            MirType::Void => Ok("void".into()),
            MirType::Bool => Ok("i1".into()),
            MirType::Int(size, _signed) => {
                let bits = match size {
                    IntSize::I8 => 8,
                    IntSize::I16 => 16,
                    IntSize::I32 => 32,
                    IntSize::I64 => 64,
                    IntSize::I128 => 128,
                    IntSize::ISize => 64, // Assuming 64-bit
                };
                Ok(format!("i{}", bits))
            }
            MirType::Float(size) => {
                match size {
                    FloatSize::F32 => Ok("float".into()),
                    FloatSize::F64 => Ok("double".into()),
                }
            }
            MirType::Ptr(_) => Ok("ptr".into()),
            MirType::Array(elem, len) => {
                let elem_ty = self.llvm_type(elem)?;
                Ok(format!("[{} x {}]", len, elem_ty))
            }
            MirType::Slice(_) => {
                // Fat pointer: { ptr, i64 }
                Ok("{ ptr, i64 }".into())
            }
            MirType::Struct(name) => Ok(format!("%{}", name)),
            MirType::FnPtr(sig) => {
                let ret = self.llvm_type(&sig.ret)?;
                let params: Vec<String> = sig.params
                    .iter()
                    .map(|p| self.llvm_type(p))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("ptr")) // LLVM uses opaque pointers
            }
            MirType::Never => Ok("void".into()),
            MirType::Vector(elem, lanes) => {
                let elem_ty = self.llvm_type(elem)?;
                Ok(format!("<{} x {}>", lanes, elem_ty))
            }
            // Opaque GPU types — represent as opaque pointers in LLVM
            MirType::Texture2D(_) | MirType::Sampler | MirType::SampledImage(_) => Ok("ptr".into()),
            MirType::TraitObject(_) => Ok("{ ptr, ptr }".to_string()), // data ptr + vtable ptr
        }
    }

    /// Get type size in bytes.
    fn type_size(&self, ty: &MirType) -> u64 {
        match ty {
            MirType::Void => 0,
            MirType::Bool => 1,
            MirType::Int(size, _) => {
                match size {
                    IntSize::I8 => 1,
                    IntSize::I16 => 2,
                    IntSize::I32 => 4,
                    IntSize::I64 | IntSize::ISize => 8,
                    IntSize::I128 => 16,
                }
            }
            MirType::Float(size) => {
                match size {
                    FloatSize::F32 => 4,
                    FloatSize::F64 => 8,
                }
            }
            MirType::Ptr(_) | MirType::FnPtr(_) => 8,
            MirType::Array(elem, len) => self.type_size(elem) * len,
            MirType::Slice(_) => 16, // Fat pointer
            MirType::Struct(_) => 8, // Placeholder
            MirType::Never => 0,
            MirType::Vector(elem, lanes) => self.type_size(elem) * (*lanes as u64),
            // Opaque GPU types — pointer-sized
            MirType::Texture2D(_) | MirType::Sampler | MirType::SampledImage(_) => 8,
            MirType::TraitObject(_) => 16, // fat pointer: data ptr + vtable ptr
        }
    }

    /// Get type alignment.
    fn type_align(&self, ty: &MirType) -> u32 {
        match ty {
            MirType::Void | MirType::Never => 1,
            MirType::Bool => 1,
            MirType::Int(size, _) => {
                match size {
                    IntSize::I8 => 1,
                    IntSize::I16 => 2,
                    IntSize::I32 => 4,
                    IntSize::I64 | IntSize::ISize | IntSize::I128 => 8,
                }
            }
            MirType::Float(size) => {
                match size {
                    FloatSize::F32 => 4,
                    FloatSize::F64 => 8,
                }
            }
            MirType::Ptr(_) | MirType::FnPtr(_) | MirType::Slice(_) => 8,
            MirType::Array(elem, _) => self.type_align(elem),
            MirType::Struct(_) => 8, // Placeholder
            MirType::Vector(elem, lanes) => {
                // Vector alignment is typically the full vector size, up to 32 bytes
                let size = self.type_size(elem) * (*lanes as u64);
                (size as u32).min(32)
            }
            // Opaque GPU types — pointer-aligned
            MirType::Texture2D(_) | MirType::Sampler | MirType::SampledImage(_) => 8,
            MirType::TraitObject(_) => 8, // pointer-aligned
        }
    }

    /// Convert MIR constant to LLVM constant string.
    fn llvm_const(&self, c: &MirConst) -> CodegenResult<String> {
        match c {
            MirConst::Bool(b) => Ok(if *b { "true" } else { "false" }.into()),
            MirConst::Int(v, _) => Ok(v.to_string()),
            MirConst::Uint(v, _) => Ok(v.to_string()),
            MirConst::Float(v, ty) => {
                match ty {
                    MirType::Float(FloatSize::F32) => {
                        // Convert to hex representation for precision
                        let bits = (*v as f32).to_bits();
                        Ok(format!("0x{:08X}", bits))
                    }
                    MirType::Float(FloatSize::F64) => {
                        let bits = v.to_bits();
                        Ok(format!("0x{:016X}", bits))
                    }
                    _ => Ok(v.to_string()),
                }
            }
            MirConst::Str(idx) => {
                if let Some(global) = self.string_globals.get(idx) {
                    Ok(global.clone())
                } else {
                    Ok("null".into())
                }
            }
            MirConst::ByteStr(bytes) => {
                let mut s = String::from("c\"");
                for b in bytes {
                    if b.is_ascii_graphic() && *b != b'"' && *b != b'\\' {
                        s.push(*b as char);
                    } else {
                        write!(&mut s, "\\{:02X}", b).unwrap();
                    }
                }
                s.push_str("\\00\"");
                Ok(s)
            }
            MirConst::Null(_) => Ok("null".into()),
            MirConst::Unit => Ok("void".into()),
            MirConst::Zeroed(_) => Ok("zeroinitializer".into()),
            MirConst::Undef(_) => Ok("undef".into()),
        }
    }

    /// Convert binary operator to LLVM instruction.
    fn llvm_binop(&self, op: BinOp, ty: &MirType) -> CodegenResult<String> {
        let is_float = ty.is_float();
        let is_signed = ty.is_signed();

        let instr = match op {
            BinOp::Add => if is_float { "fadd" } else { "add" },
            BinOp::Sub => if is_float { "fsub" } else { "sub" },
            BinOp::Mul => if is_float { "fmul" } else { "mul" },
            BinOp::Div => {
                if is_float { "fdiv" }
                else if is_signed { "sdiv" }
                else { "udiv" }
            }
            BinOp::Rem => {
                if is_float { "frem" }
                else if is_signed { "srem" }
                else { "urem" }
            }
            BinOp::BitAnd => "and",
            BinOp::BitOr => "or",
            BinOp::BitXor => "xor",
            BinOp::Shl => "shl",
            BinOp::Shr => if is_signed { "ashr" } else { "lshr" },
            BinOp::Eq => if is_float { "fcmp oeq" } else { "icmp eq" },
            BinOp::Ne => if is_float { "fcmp one" } else { "icmp ne" },
            BinOp::Lt => {
                if is_float { "fcmp olt" }
                else if is_signed { "icmp slt" }
                else { "icmp ult" }
            }
            BinOp::Le => {
                if is_float { "fcmp ole" }
                else if is_signed { "icmp sle" }
                else { "icmp ule" }
            }
            BinOp::Gt => {
                if is_float { "fcmp ogt" }
                else if is_signed { "icmp sgt" }
                else { "icmp ugt" }
            }
            BinOp::Ge => {
                if is_float { "fcmp oge" }
                else if is_signed { "icmp sge" }
                else { "icmp uge" }
            }
            BinOp::AddChecked | BinOp::AddWrapping | BinOp::AddSaturating => "add",
            BinOp::SubChecked | BinOp::SubWrapping | BinOp::SubSaturating => "sub",
            BinOp::MulChecked | BinOp::MulWrapping => "mul",
            // Pow requires llvm.pow intrinsic, but for integer we use repeated mul
            BinOp::Pow => if is_float { "call @llvm.pow" } else { "mul" },
        };

        Ok(instr.into())
    }

    /// Convert cast kind to LLVM instruction.
    fn llvm_cast(&self, kind: CastKind, from: &MirType, to: &MirType) -> CodegenResult<String> {
        let instr = match kind {
            CastKind::IntToInt => {
                let from_bits = self.type_size(from) * 8;
                let to_bits = self.type_size(to) * 8;
                if to_bits > from_bits {
                    if from.is_signed() { "sext" } else { "zext" }
                } else if to_bits < from_bits {
                    "trunc"
                } else {
                    "bitcast"
                }
            }
            CastKind::FloatToFloat => {
                let from_bits = self.type_size(from);
                let to_bits = self.type_size(to);
                if to_bits > from_bits { "fpext" } else { "fptrunc" }
            }
            CastKind::IntToFloat => {
                if from.is_signed() { "sitofp" } else { "uitofp" }
            }
            CastKind::FloatToInt => {
                if to.is_signed() { "fptosi" } else { "fptoui" }
            }
            CastKind::PtrToInt => "ptrtoint",
            CastKind::IntToPtr => "inttoptr",
            CastKind::PtrToPtr => "bitcast",
            CastKind::FnToPtr => "bitcast",
            CastKind::Transmute => "bitcast",
        };

        Ok(instr.into())
    }

    /// Convert linkage to LLVM linkage string.
    fn llvm_linkage(&self, linkage: Linkage) -> &'static str {
        match linkage {
            Linkage::Internal => "internal",
            Linkage::External => "external",
            Linkage::Weak => "weak",
            Linkage::LinkOnce => "linkonce_odr",
        }
    }

    /// Convert calling convention to LLVM string.
    fn llvm_calling_conv(&self, conv: CallingConv) -> &'static str {
        match conv {
            CallingConv::Quanta => "",
            CallingConv::C => "ccc",
            CallingConv::Fast => "fastcc",
            CallingConv::Cold => "coldcc",
        }
    }

    // =========================================================================
    // HELPER METHODS
    // =========================================================================

    /// Get the LLVM name for a local.
    fn get_local_name(&self, id: LocalId) -> CodegenResult<String> {
        self.local_names.get(&id)
            .cloned()
            .ok_or_else(|| CodegenError::Internal(format!("Unknown local: {}", id)))
    }

    /// Get the type of a local.
    fn get_local_type(&self, id: LocalId, func: &MirFunction) -> CodegenResult<MirType> {
        func.locals.iter()
            .find(|l| l.id == id)
            .map(|l| l.ty.clone())
            .ok_or_else(|| CodegenError::Internal(format!("Unknown local type: {}", id)))
    }

    /// Infer the type of a value.
    fn infer_value_type(&self, value: &MirValue, func: &MirFunction) -> CodegenResult<MirType> {
        match value {
            MirValue::Local(id) => self.get_local_type(*id, func),
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
            MirValue::Global(_) => Ok(MirType::Ptr(Box::new(MirType::Void))),
            MirValue::Function(_) => Ok(MirType::Ptr(Box::new(MirType::Void))),
        }
    }

    /// Look up the field index for a struct type by field name.
    /// Returns the 0-based index into the struct's fields, or an error.
    fn find_struct_field_index(&self, struct_name: &str, field_name: &str) -> CodegenResult<u32> {
        for td in &self.type_defs {
            if td.name.as_ref() == struct_name {
                if let TypeDefKind::Struct { fields, .. } = &td.kind {
                    for (i, (name_opt, _ty)) in fields.iter().enumerate() {
                        if let Some(n) = name_opt {
                            if n.as_ref() == field_name {
                                return Ok(i as u32);
                            }
                        }
                    }
                    return Err(CodegenError::Internal(format!(
                        "Field '{}' not found in struct '{}'", field_name, struct_name
                    )));
                }
            }
        }
        Err(CodegenError::Internal(format!(
            "Struct type '{}' not found in module type definitions", struct_name
        )))
    }

    /// Scan the MIR module for function calls that reference functions not
    /// defined in the module or already declared as externals, and emit
    /// `declare` statements for them. This handles cases like `printf` which
    /// the MIR lowerer uses directly without adding to externals.
    fn gen_implicit_declares(&mut self, mir: &MirModule) {
        use std::collections::HashSet;

        // Collect names of all defined functions and declared externals
        let mut known: HashSet<&str> = HashSet::new();
        for f in &mir.functions {
            known.insert(&f.name);
        }
        for ext in &mir.externals {
            known.insert(&ext.name);
        }

        // Collect names referenced via MirValue::Function / Call terminators
        let mut needed: HashSet<String> = HashSet::new();
        for func in &mir.functions {
            if let Some(blocks) = &func.blocks {
                for block in blocks {
                    if let Some(MirTerminator::Call { func: callee, .. }) = &block.terminator {
                        if let MirValue::Function(name) = callee {
                            if !known.contains(name.as_ref())
                                && !name.starts_with("llvm.")
                            {
                                needed.insert(name.to_string());
                            }
                        }
                    }
                }
            }
        }

        if !needed.is_empty() {
            writeln!(&mut self.output, "; Implicit external declarations").unwrap();
            for name in &needed {
                // Default to variadic C function signature for printf-like functions
                writeln!(
                    &mut self.output,
                    "declare i32 @{}(ptr, ...) nounwind",
                    name
                ).unwrap();
            }
            writeln!(&mut self.output).unwrap();
        }
    }

    /// Generate function attributes.
    fn gen_attributes(&mut self) {
        if self.opt_level > 0 {
            writeln!(&mut self.output).unwrap();
            writeln!(&mut self.output, "attributes #0 = {{ nounwind }}").unwrap();
        }
    }
}

impl Default for LlvmBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for LlvmBackend {
    fn generate(&mut self, mir: &MirModule) -> CodegenResult<GeneratedCode> {
        self.output.clear();
        self.string_globals.clear();
        self.block_counter = 0;
        self.string_counter = 0;
        self.type_defs = mir.types.clone();

        // Generate module structure
        self.gen_module_header(mir);
        self.gen_type_defs(mir)?;
        self.gen_string_literals(mir);
        self.gen_globals(mir)?;
        self.gen_externals(mir)?;
        self.gen_intrinsics();
        self.gen_implicit_declares(mir);

        // Generate functions
        for func in &mir.functions {
            self.gen_function(func)?;
        }

        // Generate attributes and intrinsics
        self.gen_attributes();

        Ok(GeneratedCode::new(
            OutputFormat::LlvmIr,
            self.output.as_bytes().to_vec(),
        ))
    }

    fn target(&self) -> Target {
        Target::LlvmIr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_module() -> MirModule {
        let mut module = MirModule::new("test");

        // Add a simple function
        let sig = MirFnSig::new(vec![MirType::i32()], MirType::i32());
        let mut func = MirFunction::new("add_one", sig);
        func.linkage = Linkage::External;
        func.is_public = true;

        // Add parameter local
        let param = MirLocal {
            id: LocalId(0),
            name: Some("x".into()),
            ty: MirType::i32(),
            is_mut: false,
            is_param: true,
        };
        func.add_local(param);

        // Add result local
        let result = MirLocal {
            id: LocalId(1),
            name: Some("result".into()),
            ty: MirType::i32(),
            is_mut: true,
            is_param: false,
        };
        func.add_local(result);

        // Entry block
        let mut entry = MirBlock::new(BlockId::ENTRY);
        entry.push_stmt(MirStmt::new(MirStmtKind::Assign {
            dest: LocalId(1),
            value: MirRValue::BinaryOp {
                op: BinOp::Add,
                left: MirValue::Local(LocalId(0)),
                right: MirValue::Const(MirConst::Int(1, MirType::i32())),
            },
        }));
        entry.set_terminator(MirTerminator::Return(Some(MirValue::Local(LocalId(1)))));
        func.add_block(entry);

        module.add_function(func);
        module
    }

    #[test]
    fn test_llvm_backend_new() {
        let backend = LlvmBackend::new();
        // Target triple is platform-dependent
        #[cfg(target_os = "windows")]
        assert_eq!(backend.target_triple, "x86_64-pc-windows-msvc");
        #[cfg(target_os = "linux")]
        assert_eq!(backend.target_triple, "x86_64-unknown-linux-gnu");
        assert_eq!(backend.opt_level, 0);
    }

    #[test]
    fn test_llvm_backend_with_options() {
        let backend = LlvmBackend::new()
            .with_target_triple("aarch64-apple-darwin")
            .with_opt_level(2)
            .with_debug_info(true);

        assert_eq!(backend.target_triple, "aarch64-apple-darwin");
        assert_eq!(backend.opt_level, 2);
        assert!(backend.debug_info);
    }

    #[test]
    fn test_llvm_type_conversion() {
        let backend = LlvmBackend::new();

        assert_eq!(backend.llvm_type(&MirType::Void).unwrap(), "void");
        assert_eq!(backend.llvm_type(&MirType::Bool).unwrap(), "i1");
        assert_eq!(backend.llvm_type(&MirType::i8()).unwrap(), "i8");
        assert_eq!(backend.llvm_type(&MirType::i16()).unwrap(), "i16");
        assert_eq!(backend.llvm_type(&MirType::i32()).unwrap(), "i32");
        assert_eq!(backend.llvm_type(&MirType::i64()).unwrap(), "i64");
        assert_eq!(backend.llvm_type(&MirType::f32()).unwrap(), "float");
        assert_eq!(backend.llvm_type(&MirType::f64()).unwrap(), "double");
        assert_eq!(backend.llvm_type(&MirType::Ptr(Box::new(MirType::i32()))).unwrap(), "ptr");
    }

    #[test]
    fn test_llvm_const_conversion() {
        let backend = LlvmBackend::new();

        assert_eq!(backend.llvm_const(&MirConst::Bool(true)).unwrap(), "true");
        assert_eq!(backend.llvm_const(&MirConst::Bool(false)).unwrap(), "false");
        assert_eq!(backend.llvm_const(&MirConst::Int(42, MirType::i32())).unwrap(), "42");
        assert_eq!(backend.llvm_const(&MirConst::Uint(100, MirType::u64())).unwrap(), "100");
        assert_eq!(backend.llvm_const(&MirConst::Null(MirType::Ptr(Box::new(MirType::Void)))).unwrap(), "null");
    }

    #[test]
    fn test_escape_string() {
        let backend = LlvmBackend::new();

        assert_eq!(backend.escape_string("hello"), "hello");
        assert_eq!(backend.escape_string("hello\nworld"), "hello\\0Aworld");
        assert_eq!(backend.escape_string("tab\there"), "tab\\09here");
        assert_eq!(backend.escape_string("quote\"here"), "quote\\22here");
    }

    #[test]
    fn test_type_size() {
        let backend = LlvmBackend::new();

        assert_eq!(backend.type_size(&MirType::Bool), 1);
        assert_eq!(backend.type_size(&MirType::i8()), 1);
        assert_eq!(backend.type_size(&MirType::i16()), 2);
        assert_eq!(backend.type_size(&MirType::i32()), 4);
        assert_eq!(backend.type_size(&MirType::i64()), 8);
        assert_eq!(backend.type_size(&MirType::f32()), 4);
        assert_eq!(backend.type_size(&MirType::f64()), 8);
        assert_eq!(backend.type_size(&MirType::Ptr(Box::new(MirType::i32()))), 8);
    }

    #[test]
    fn test_type_align() {
        let backend = LlvmBackend::new();

        assert_eq!(backend.type_align(&MirType::Bool), 1);
        assert_eq!(backend.type_align(&MirType::i8()), 1);
        assert_eq!(backend.type_align(&MirType::i16()), 2);
        assert_eq!(backend.type_align(&MirType::i32()), 4);
        assert_eq!(backend.type_align(&MirType::i64()), 8);
        assert_eq!(backend.type_align(&MirType::Ptr(Box::new(MirType::i32()))), 8);
    }

    #[test]
    fn test_llvm_binop() {
        let backend = LlvmBackend::new();

        // Integer operations
        assert_eq!(backend.llvm_binop(BinOp::Add, &MirType::i32()).unwrap(), "add");
        assert_eq!(backend.llvm_binop(BinOp::Sub, &MirType::i32()).unwrap(), "sub");
        assert_eq!(backend.llvm_binop(BinOp::Mul, &MirType::i32()).unwrap(), "mul");
        assert_eq!(backend.llvm_binop(BinOp::Div, &MirType::i32()).unwrap(), "sdiv");
        assert_eq!(backend.llvm_binop(BinOp::Div, &MirType::u32()).unwrap(), "udiv");

        // Float operations
        assert_eq!(backend.llvm_binop(BinOp::Add, &MirType::f32()).unwrap(), "fadd");
        assert_eq!(backend.llvm_binop(BinOp::Sub, &MirType::f64()).unwrap(), "fsub");

        // Comparisons
        assert_eq!(backend.llvm_binop(BinOp::Eq, &MirType::i32()).unwrap(), "icmp eq");
        assert_eq!(backend.llvm_binop(BinOp::Lt, &MirType::i32()).unwrap(), "icmp slt");
        assert_eq!(backend.llvm_binop(BinOp::Lt, &MirType::u32()).unwrap(), "icmp ult");
        assert_eq!(backend.llvm_binop(BinOp::Eq, &MirType::f64()).unwrap(), "fcmp oeq");
    }

    #[test]
    fn test_llvm_linkage() {
        let backend = LlvmBackend::new();

        assert_eq!(backend.llvm_linkage(Linkage::Internal), "internal");
        assert_eq!(backend.llvm_linkage(Linkage::External), "external");
        assert_eq!(backend.llvm_linkage(Linkage::Weak), "weak");
        assert_eq!(backend.llvm_linkage(Linkage::LinkOnce), "linkonce_odr");
    }

    #[test]
    fn test_generate_simple_module() {
        let module = create_test_module();
        let mut backend = LlvmBackend::new();

        let result = backend.generate(&module);
        assert!(result.is_ok());

        let code = result.unwrap();
        let output = String::from_utf8(code.data).unwrap();

        // Check header
        assert!(output.contains("QuantaLang LLVM IR Output"));
        assert!(output.contains("Module: test"));
        assert!(output.contains("target triple"));

        // Check function
        assert!(output.contains("define external i32 @add_one"));
        assert!(output.contains("add i32"));
        assert!(output.contains("ret i32"));
    }

    #[test]
    fn test_backend_target() {
        let backend = LlvmBackend::new();
        assert_eq!(backend.target(), Target::LlvmIr);
    }
}
