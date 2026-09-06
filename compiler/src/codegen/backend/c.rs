// ===============================================================================
// BUILDLANG CODE GENERATOR - C BACKEND
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! C code generation backend.
//!
//! Transpiles MIR to C99-compliant code for maximum portability.

// NOTE: `.unwrap()` calls in this backend are intentional assertions on
// type-checked AST invariants. Failures indicate compiler bugs in earlier
// phases, not user input errors. See codegen/mod.rs for the full unwrap policy.

use std::fmt::Write;
use std::sync::Arc;

use super::{Backend, CodegenResult, Target};
use crate::codegen::ir::*;
use crate::codegen::runtime;
use crate::codegen::{GeneratedCode, OutputFormat};

/// C backend for code generation.
pub struct CBackend {
    /// Output buffer.
    output: String,
    /// Indentation level.
    indent: usize,
    /// Temp variable counter.
    temp_counter: u32,
    /// Function parameter types - indexed by function name, stores param types.
    fn_params: std::collections::HashMap<String, Vec<MirType>>,
    /// Return type of the current function being generated.
    current_ret_ty: MirType,
    /// Name of the current function being generated (for Self resolution).
    current_fn_name: Option<String>,
    /// String table for the module currently being emitted.
    string_literals: Vec<Arc<str>>,
    /// Function-local BuildString temps that originated from string literals.
    local_string_literals: std::collections::HashMap<LocalId, u32>,
    /// Owned heap (BuildString) locals proven safe to free at every `return` of
    /// the current function (see `freeable_owned_string_locals`). Empty unless
    /// the experimental drop-insertion path is enabled.
    current_fn_freeable: Vec<LocalId>,
    /// True if the module declares a mutable global whose type could hold a heap
    /// string alias (a pointer, aggregate, Vec, Map, ...). When set, the
    /// experimental drop analysis is disabled module-wide: an owned string can be
    /// stashed into such a global (an escape the per-function MIR scan cannot see
    /// today, because that store is currently dropped by a separate lowering
    /// gap), so freeing would risk a dangling alias. Conservative: leak, never
    /// corrupt. See docs/MEMORY-PILLAR-DESIGN.md.
    module_mut_global_alias_risk: bool,
    /// Owned heap locals to free at the START of a block (block-scoped drops),
    /// keyed by the block's `bb<id>`. Bounds loop memory; disjoint from
    /// `current_fn_freeable`. Empty unless the experimental path is enabled.
    current_fn_block_frees: std::collections::HashMap<u32, Vec<LocalId>>,
    /// Increment 5: owners enrolled for a runtime `uint8_t __bl_live_N` drop
    /// flag (split-frontier / conditional-allocation death frontiers, the two
    /// shapes increments 1-4 decline). Drives the flag declaration, the
    /// def-site `= 1;` set, and the guarded Return-backstop free. Disjoint
    /// from `current_fn_freeable`/`current_fn_block_frees`. Empty unless the
    /// experimental path is enabled.
    current_fn_flag_owners: Vec<LocalId>,
    /// Increment 5: guarded frees at block START for flag owners, keyed by
    /// the block's `bb<id>`. Same key space as `current_fn_block_frees`, but
    /// emitted through the flag guard (`if (__bl_live_N) { ...; __bl_live_N =
    /// 0; }`), never merged into `current_fn_block_frees` (that map emits
    /// UNGUARDED frees). Empty unless the experimental path is enabled.
    current_fn_flag_block_frees: std::collections::HashMap<u32, Vec<LocalId>>,
    /// TEST-ONLY override for `experimental_free_enabled()`. `None` in every
    /// production path (`CBackend::new()` never sets it), so this field has
    /// no effect outside `#[cfg(test)]`: `experimental_free_enabled()` falls
    /// back to the exact `BUILDLANG_EXPERIMENTAL_FREE` env check it used
    /// before this field existed. Exists so an in-process unit test can pin
    /// the flag-guarded emission (w5-review.md Important finding 2) without
    /// setting a process-global env var, which is unsound under parallel
    /// `cargo test` (see `set_experimental_free_enabled_for_test`).
    experimental_free_override_for_test: Option<bool>,
}

impl CBackend {
    /// Create a new C backend.
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            temp_counter: 0,
            fn_params: std::collections::HashMap::new(),
            current_ret_ty: MirType::Void,
            current_fn_name: None,
            string_literals: Vec::new(),
            local_string_literals: std::collections::HashMap::new(),
            current_fn_freeable: Vec::new(),
            module_mut_global_alias_risk: false,
            current_fn_block_frees: std::collections::HashMap::new(),
            current_fn_flag_owners: Vec::new(),
            current_fn_flag_block_frees: std::collections::HashMap::new(),
            experimental_free_override_for_test: None,
        }
    }

    /// Whether the experimental deterministic-free path is enabled. Off by
    /// default so the verified baseline (corpus c-execution, all current
    /// programs) keeps the existing leak-but-correct behavior; enabled by
    /// setting `BUILDLANG_EXPERIMENTAL_FREE` while the analysis is matured and
    /// proven under AddressSanitizer. `experimental_free_override_for_test`,
    /// when set, takes precedence over the env var; every production path
    /// leaves it `None`, so production behavior is byte-for-byte unchanged.
    fn experimental_free_enabled(&self) -> bool {
        self.experimental_free_override_for_test
            .unwrap_or_else(|| std::env::var_os("BUILDLANG_EXPERIMENTAL_FREE").is_some())
    }

    /// TEST-ONLY: force `experimental_free_enabled()` to `enabled`, bypassing
    /// the `BUILDLANG_EXPERIMENTAL_FREE` environment variable. Lets an
    /// in-process unit test exercise the flag-guarded emission path without
    /// setting a process-global env var -- unsafe under parallel `cargo
    /// test`, and the repo's established idiom is to never do so (see
    /// `docs/superpowers/plans/2026-07-29-drop-flags.md`'s "Test idiom
    /// note"). No effect on any production path: nothing outside `#[cfg(test)]`
    /// can call this, and `CBackend::new()` always leaves the override `None`.
    #[cfg(test)]
    fn set_experimental_free_enabled_for_test(&mut self, enabled: bool) {
        self.experimental_free_override_for_test = Some(enabled);
    }

    /// Runtime functions that return a FRESH, solely-owned heap `BuildString`
    /// (`cap > 0`, from `malloc`). TRUST SURFACE: every entry must allocate a
    /// buffer that nothing else owns or aliases. Verified against `runtime.rs`,
    /// each listed function `malloc`s and copies. Deliberately EXCLUDES
    /// `build_string_new`/`String_from` (they return `cap = 0` wrappers, so
    /// there is nothing to free) and the container-alias getters
    /// `build_hvec_get_str`/`build_hmap_get_str_str` (their result aliases the
    /// container, so freeing it would double-free).
    fn allocates_owned_string(name: &str) -> bool {
        matches!(
            name,
            "build_string_concat"
                | "build_format_str"
                | "build_format_i32"
                | "build_format_f64"
                | "build_i32_to_string"
                | "build_f64_to_string"
        )
    }

    /// Runtime functions that READ a `BuildString` (or its `.ptr`) argument
    /// during the call and never retain a pointer to it past the call, never
    /// free it, and never return an alias of it. TRUST SURFACE for the escape
    /// check: a use of an owned local as an argument to one of these does not
    /// block freeing the local. Excludes `String_from` (returns a `cap = 0`
    /// wrapper that ALIASES its argument's buffer) and `build_string_free`
    /// (it frees). When in doubt, leave a function OFF: the local then leaks,
    /// which is safe. This is a CLOSED list (no name-prefix wildcard): every
    /// entry is audited line-by-line against `runtime.rs` to read-but-never-
    /// retain its string argument; adding a new runtime function does NOT
    /// auto-trust it.
    fn borrows_string_arg(name: &str) -> bool {
        matches!(
            name,
            "printf"
                | "fprintf"
                | "sprintf"
                | "snprintf"
                | "puts"
                | "fputs"
                | "fwrite"
                | "build_string_len"
                | "build_string_eq"
                | "build_string_concat"
                | "build_format_str"
                | "build_print_str"
                | "build_print_string"
                | "build_eprint_str"
                | "build_eprint_string"
        )
    }

    /// The runtime suffix for an element-typed `Vec` handle (`build_hvec_*_<suffix>`
    /// and `build_hvec_new_<suffix>`), chosen from the Vec's element type.
    fn hvec_elem_suffix(elem: &MirType) -> String {
        match elem {
            MirType::Struct(n) if n.as_ref() == "BuildString" => "str".to_string(),
            MirType::Int(IntSize::I64, _) => "i64".to_string(),
            MirType::Int(..) => "i32".to_string(),
            MirType::Float(..) => "f64".to_string(),
            // Aggregate element (a user struct, vector type, etc.): use a
            // monomorphized, element-sized wrapper keyed by the struct name.
            MirType::Struct(n) => n.to_string(),
            // Nested collection element: keyed by its C handle type name.
            MirType::Vec(_) => "BuildVecHandle".to_string(),
            MirType::Map(_, _) => "BuildMapHandle".to_string(),
            _ => "i32".to_string(),
        }
    }

    /// True when a Vec element type needs a monomorphized element-sized wrapper
    /// (rather than one of the built-in i32/i64/f64/str handle families).
    fn vec_elem_needs_sized_wrapper(elem: &MirType) -> bool {
        matches!(elem, MirType::Struct(n) if n.as_ref() != "BuildString")
            || matches!(elem, MirType::Vec(_) | MirType::Map(_, _))
    }

    /// The directly-named callee of a `Call`, if any.
    fn callee_name(func: &MirValue) -> Option<&str> {
        crate::codegen::analysis::cfg::callee_name(func)
    }

    /// Whether a type could hold (or embed) a heap string buffer or a pointer
    /// into one. Used to decide if a mutable global is a stash hazard for the
    /// drop analysis. Conservative: only pure scalars, SIMD vectors, GPU
    /// resources, and function pointers are treated as alias-free; everything
    /// else (pointers, arrays, slices, structs, trait objects, Vec, Map, tuples)
    /// is assumed capable of aliasing a heap string.
    fn ty_can_hold_heap_string_alias(ty: &MirType) -> bool {
        !matches!(
            ty,
            MirType::Void
                | MirType::Bool
                | MirType::Int(..)
                | MirType::Float(..)
                | MirType::Never
                | MirType::Vector(..)
                | MirType::Sampler
                | MirType::Texture2D(..)
                | MirType::SampledImage(..)
                | MirType::FnPtr(..)
        )
    }

    /// Find owned heap `BuildString` locals that are sound to free at every
    /// `return`. The rule (see docs/MEMORY-PILLAR-DESIGN.md, "second increment"):
    /// a local is freed iff it OWNS a fresh heap buffer (it is the dest of an
    /// allocating Call, or move-acquired from another owner), its defining block
    /// DOMINATES every reachable return (definite init), it is NOT moved-from
    /// (ownership not transferred away - the alias guard against double-free),
    /// it has exactly one definition, and it does NOT ESCAPE. Conservative by
    /// construction: any risky use anywhere excludes the local (it then leaks,
    /// which is safe). Soundness rests on the escape scan being COMPLETE.
    fn freeable_owned_string_locals(&self, func: &MirFunction) -> Vec<LocalId> {
        // Module-wide guard: if a mutable global could hold a heap string alias,
        // an owner could be stashed there (an escape invisible to this
        // per-function scan), so reclaim nothing in the whole module.
        if self.module_mut_global_alias_risk {
            return Vec::new();
        }
        let blocks = match &func.blocks {
            Some(b) if !b.is_empty() => b,
            _ => return Vec::new(),
        };

        let is_owned_buildstring = |id: LocalId| -> bool {
            func.locals.iter().any(|l| {
                l.id == id
                    && !l.is_param
                    && matches!(l.ty, MirType::Struct(ref n) if n.as_ref() == "BuildString")
            })
        };

        // 1a. Alloc-defined owners: dest of a Call to a known allocating fn.
        //     Track the defining block index for the definite-init check.
        let mut owner_def: std::collections::HashMap<LocalId, usize> =
            std::collections::HashMap::new();
        for (bi, block) in blocks.iter().enumerate() {
            if let Some(MirTerminator::Call {
                func: callee,
                dest: Some(d),
                ..
            }) = &block.terminator
            {
                if is_owned_buildstring(*d)
                    && Self::callee_name(callee)
                        .map(Self::allocates_owned_string)
                        .unwrap_or(false)
                {
                    owner_def.entry(*d).or_insert(bi);
                }
            }
        }

        // 1b. Move-acquired owners (to a fixpoint): `dest = Use(Local src)`
        //     where `src` is an owner transfers ownership to `dest` and marks
        //     `src` moved-from (so it is never freed: the alias guard).
        let mut moved_from: std::collections::HashSet<LocalId> = std::collections::HashSet::new();
        loop {
            let mut changed = false;
            for (bi, block) in blocks.iter().enumerate() {
                for stmt in &block.stmts {
                    if let MirStmtKind::Assign {
                        dest,
                        value: MirRValue::Use(MirValue::Local(src)),
                    } = &stmt.kind
                    {
                        if owner_def.contains_key(src) {
                            if moved_from.insert(*src) {
                                changed = true;
                            }
                            if is_owned_buildstring(*dest) && !owner_def.contains_key(dest) {
                                owner_def.insert(*dest, bi);
                                changed = true;
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }

        // 1c. Alias guard against MULTIPLE move-acquirers. A source moved into
        //     more than one destination (an uncaught use-after-move such as
        //     `let p = c; let q = c;`) makes those destinations ALIAS the same
        //     heap buffer, so freeing more than one is a double-free. The
        //     moved-from guard alone assumes one acquirer per source, which the
        //     front end does not currently enforce at codegen. Taint every
        //     destination of any multiply-moved source and propagate along move
        //     edges (a tainted owner's own acquirers alias it too); tainted
        //     owners are never freed (the buffer leaks, which is safe).
        let mut acquirers: std::collections::HashMap<LocalId, Vec<LocalId>> =
            std::collections::HashMap::new();
        for block in blocks {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign {
                    dest,
                    value: MirRValue::Use(MirValue::Local(src)),
                } = &stmt.kind
                {
                    if owner_def.contains_key(src) && owner_def.contains_key(dest) {
                        acquirers.entry(*src).or_default().push(*dest);
                    }
                }
            }
        }
        let mut tainted: std::collections::HashSet<LocalId> = std::collections::HashSet::new();
        let mut worklist: Vec<LocalId> = Vec::new();
        for dests in acquirers.values() {
            if dests.len() > 1 {
                for d in dests {
                    if tainted.insert(*d) {
                        worklist.push(*d);
                    }
                }
            }
        }
        while let Some(n) = worklist.pop() {
            if let Some(dests) = acquirers.get(&n) {
                for d in dests {
                    if tainted.insert(*d) {
                        worklist.push(*d);
                    }
                }
            }
        }

        // 2. Reachable returns and the dominator sets.
        let dom = Self::compute_dominators(blocks);
        let reachable = Self::reachable_blocks(blocks);
        let return_blocks: Vec<usize> = blocks
            .iter()
            .enumerate()
            .filter(|(i, b)| {
                reachable[*i] && matches!(b.terminator, Some(MirTerminator::Return(_)))
            })
            .map(|(i, _)| i)
            .collect();

        // 3. Definition counts (exactly-one-def guard against reassignment).
        let def_count = |id: LocalId| -> usize {
            let mut n = 0usize;
            for block in blocks {
                for stmt in &block.stmts {
                    if matches!(&stmt.kind, MirStmtKind::Assign { dest, .. } if *dest == id) {
                        n += 1;
                    }
                }
                if matches!(&block.terminator, Some(MirTerminator::Call { dest: Some(d), .. }) if *d == id)
                {
                    n += 1;
                }
            }
            n
        };

        let mut out = Vec::new();
        for (&id, &def_bi) in &owner_def {
            if moved_from.contains(&id) {
                continue;
            }
            if tainted.contains(&id) {
                continue;
            }
            if def_count(id) != 1 {
                continue;
            }
            // Definite init: the defining block dominates every reachable return.
            if !return_blocks.iter().all(|&rb| dom[rb].contains(&def_bi)) {
                continue;
            }
            if Self::owned_string_escapes(id, blocks) {
                continue;
            }
            out.push(id);
        }
        // Deterministic order for reproducible codegen (receipts).
        out.sort_by_key(|id| id.0);
        out
    }

    /// Owned heap `BuildString` candidates passing every soundness gate EXCEPT
    /// the drop-PLACEMENT gate (definite-init for function-exit, isolated-edge for
    /// block-scoped). Returns `(local, defining-block-index)`. Recomputes the
    /// ownership/move/taint/one-def/escape gates independently of
    /// `freeable_owned_string_locals` (which stays byte-identical and verified) so
    /// the block-scoped pass adds zero risk to the function-exit path. Empty if the
    /// module-wide mutable-global guard is active.
    fn sound_owned_candidates(&self, func: &MirFunction) -> Vec<(LocalId, usize)> {
        if self.module_mut_global_alias_risk {
            return Vec::new();
        }
        let blocks = match &func.blocks {
            Some(b) if !b.is_empty() => b,
            _ => return Vec::new(),
        };
        let is_owned_buildstring = |id: LocalId| -> bool {
            func.locals.iter().any(|l| {
                l.id == id
                    && !l.is_param
                    && matches!(l.ty, MirType::Struct(ref n) if n.as_ref() == "BuildString")
            })
        };
        let mut owner_def: std::collections::HashMap<LocalId, usize> =
            std::collections::HashMap::new();
        for (bi, block) in blocks.iter().enumerate() {
            if let Some(MirTerminator::Call {
                func: callee,
                dest: Some(d),
                ..
            }) = &block.terminator
            {
                if is_owned_buildstring(*d)
                    && Self::callee_name(callee)
                        .map(Self::allocates_owned_string)
                        .unwrap_or(false)
                {
                    owner_def.entry(*d).or_insert(bi);
                }
            }
        }
        let mut moved_from: std::collections::HashSet<LocalId> = std::collections::HashSet::new();
        loop {
            let mut changed = false;
            for (bi, block) in blocks.iter().enumerate() {
                for stmt in &block.stmts {
                    if let MirStmtKind::Assign {
                        dest,
                        value: MirRValue::Use(MirValue::Local(src)),
                    } = &stmt.kind
                    {
                        if owner_def.contains_key(src) {
                            if moved_from.insert(*src) {
                                changed = true;
                            }
                            if is_owned_buildstring(*dest) && !owner_def.contains_key(dest) {
                                owner_def.insert(*dest, bi);
                                changed = true;
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut acquirers: std::collections::HashMap<LocalId, Vec<LocalId>> =
            std::collections::HashMap::new();
        for block in blocks {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign {
                    dest,
                    value: MirRValue::Use(MirValue::Local(src)),
                } = &stmt.kind
                {
                    if owner_def.contains_key(src) && owner_def.contains_key(dest) {
                        acquirers.entry(*src).or_default().push(*dest);
                    }
                }
            }
        }
        let mut tainted: std::collections::HashSet<LocalId> = std::collections::HashSet::new();
        let mut worklist: Vec<LocalId> = Vec::new();
        for dests in acquirers.values() {
            if dests.len() > 1 {
                for d in dests {
                    if tainted.insert(*d) {
                        worklist.push(*d);
                    }
                }
            }
        }
        while let Some(n) = worklist.pop() {
            if let Some(dests) = acquirers.get(&n) {
                for d in dests {
                    if tainted.insert(*d) {
                        worklist.push(*d);
                    }
                }
            }
        }
        let def_count = |id: LocalId| -> usize {
            let mut n = 0usize;
            for block in blocks {
                for stmt in &block.stmts {
                    if matches!(&stmt.kind, MirStmtKind::Assign { dest, .. } if *dest == id) {
                        n += 1;
                    }
                }
                if matches!(&block.terminator, Some(MirTerminator::Call { dest: Some(d), .. }) if *d == id)
                {
                    n += 1;
                }
            }
            n
        };
        let mut out = Vec::new();
        for (&id, &def_bi) in &owner_def {
            if moved_from.contains(&id) || tainted.contains(&id) {
                continue;
            }
            if def_count(id) != 1 {
                continue;
            }
            if Self::owned_string_escapes(id, blocks) {
                continue;
            }
            out.push((id, def_bi));
        }
        out.sort_by_key(|(id, _)| id.0);
        out
    }

    /// The move-source chain of `id`: every local whose buffer `id` ultimately
    /// acquired through a chain of `dest = Use(src)` moves (`id`'s immediate
    /// source, its source, ...). These locals alias `id`'s heap buffer, so a
    /// `.ptr` borrow taken off ANY of them must also be confined to `id`'s block
    /// before `id` is block-scoped freed. Closes the move-source borrow gap the
    /// adversarial audit flagged (latent: not currently source-reachable).
    fn move_source_chain(id: LocalId, blocks: &[MirBlock]) -> Vec<LocalId> {
        crate::codegen::analysis::cfg::move_source_chain(id, blocks)
    }

    /// Owned heap locals to free at the START of a block (block-scoped drops),
    /// keyed by the C `bb<id>` block id where the free is emitted. This reclaims
    /// loop-body allocations the function-exit pass cannot (their def does not
    /// dominate the return), bounding loop peak memory.
    ///
    /// ADDITIVE and DISJOINT from `fn_exit` (the function-exit free set), so no
    /// buffer is freed twice. The placement is the subtle part (see
    /// docs/MEMORY-PILLAR-DESIGN.md, third increment): a `.ptr` borrow of the owner
    /// is typically consumed by the defining block's TERMINATOR (e.g. `printf`), so
    /// freeing at end-of-statements would be a use-after-free. Instead free at the
    /// START of the defining block's successor `S`, only on an ISOLATED edge
    /// (`B` has one successor `S`, `S` has one predecessor `B`), with `L`, its move
    /// sources, and all their `.ptr` borrow temps confined to `B`.
    fn block_scoped_freeable(
        &self,
        func: &MirFunction,
        fn_exit: &[LocalId],
    ) -> std::collections::HashMap<u32, Vec<LocalId>> {
        let mut map: std::collections::HashMap<u32, Vec<LocalId>> =
            std::collections::HashMap::new();
        let blocks = match &func.blocks {
            Some(b) if !b.is_empty() => b,
            _ => return map,
        };
        let fn_exit_set: std::collections::HashSet<LocalId> = fn_exit.iter().copied().collect();
        let id_to_index = Self::block_id_index(blocks);
        // The entry block runs once at function start BEFORE any predecessor, so a
        // free placed at its start would free a not-yet-allocated local. Never
        // target it, even if a back-edge gives it a single CFG predecessor.
        let entry = id_to_index.get(&0).copied().unwrap_or(0);
        let mut pred_count = vec![0usize; blocks.len()];
        for b in blocks {
            for s in Self::terminator_successors(&b.terminator, &id_to_index) {
                if s < pred_count.len() {
                    pred_count[s] += 1;
                }
            }
        }
        for (id, def_bi) in self.sound_owned_candidates(func) {
            if fn_exit_set.contains(&id) {
                continue;
            }
            // `id`, its move sources, and all their `.ptr` borrow temps must be
            // confined to `def_bi`.
            if !Self::live_range_confined_to_block(id, def_bi, blocks) {
                continue;
            }
            // Isolated edge `def_bi -> S`: free at the start of `S`, after the
            // defining block's terminator has consumed any borrow.
            let succs = Self::terminator_successors(&blocks[def_bi].terminator, &id_to_index);
            if succs.len() != 1 {
                continue;
            }
            let s = succs[0];
            if s == def_bi || s == entry || pred_count[s] != 1 {
                continue;
            }
            map.entry(blocks[s].id.0).or_default().push(id);
        }
        for v in map.values_mut() {
            v.sort_by_key(|id| id.0);
        }
        map
    }

    /// True if `id`, every local in its move-source chain, and every `.ptr`/field
    /// borrow temp derived from any of them are USED only within block index `b`.
    /// (`sound_owned_candidates` already guarantees a `.ptr` temp flows only to
    /// non-retaining borrow calls and is never copied, so confining those uses to
    /// `b` confines the whole buffer live range to `b`: it is dead in every
    /// successor.)
    fn live_range_confined_to_block(id: LocalId, b: usize, blocks: &[MirBlock]) -> bool {
        // The locals that alias `id`'s buffer: `id` plus its move sources.
        let mut aliases = vec![id];
        aliases.extend(Self::move_source_chain(id, blocks));
        // Collect borrow temps `T = <alias>.<field>`; any created outside `b`
        // already breaks confinement.
        let mut borrows: Vec<LocalId> = Vec::new();
        for (bi, block) in blocks.iter().enumerate() {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign {
                    dest,
                    value:
                        MirRValue::FieldAccess {
                            base: MirValue::Local(base),
                            ..
                        },
                } = &stmt.kind
                {
                    if aliases.contains(base) {
                        if bi != b {
                            return false;
                        }
                        borrows.push(*dest);
                    }
                }
            }
        }
        let confined = |x: LocalId| -> bool {
            for (bi, block) in blocks.iter().enumerate() {
                if bi == b {
                    continue;
                }
                let used = block
                    .stmts
                    .iter()
                    .any(|s| Self::stmt_uses_local(&s.kind, x))
                    || Self::terminator_uses_local(&block.terminator, x);
                if used {
                    return false;
                }
            }
            true
        };
        // `id` itself must be confined; the move sources are moved-from (dead after
        // their move) so we only need their BORROWS confined, which is enforced
        // above. The borrow temps must not leak out of `b`.
        confined(id) && borrows.iter().all(|t| confined(*t))
    }

    /// True if statement `kind` USES `x` (reads it). The `Assign` dest is a
    /// definition, not a use, so it is excluded.
    fn stmt_uses_local(kind: &MirStmtKind, x: LocalId) -> bool {
        crate::codegen::analysis::cfg::stmt_uses_local(kind, x)
    }

    /// True if a terminator USES `x` (any appearance as a value or place). A
    /// `Call` dest is a definition and is not represented here.
    fn terminator_uses_local(term: &Option<MirTerminator>, x: LocalId) -> bool {
        crate::codegen::analysis::cfg::terminator_uses_local(term, x)
    }

    /// Blocks reachable from the entry (`BlockId(0)` if present, else index 0).
    fn reachable_blocks(blocks: &[MirBlock]) -> Vec<bool> {
        crate::codegen::analysis::cfg::reachable_blocks(blocks)
    }

    /// Map each block's `BlockId` value to its index in `blocks`.
    fn block_id_index(blocks: &[MirBlock]) -> std::collections::HashMap<u32, usize> {
        crate::codegen::analysis::cfg::block_id_index(blocks)
    }

    /// Successor block indices of a terminator, resolved through `id_to_index`.
    fn terminator_successors(
        term: &Option<MirTerminator>,
        id_to_index: &std::collections::HashMap<u32, usize>,
    ) -> Vec<usize> {
        crate::codegen::analysis::cfg::terminator_successors(term, id_to_index)
    }

    /// Iterative dominator sets: `dom[i]` is the set of block indices that
    /// dominate block `i`. `X` dominates `Y` iff `X` is in `dom[Y]`. Only
    /// REACHABLE predecessors are intersected: the MIR lowering routinely emits
    /// unreachable blocks, and an unreachable predecessor (with `dom = {itself}`)
    /// would otherwise erase a join's true dominators. That erasure is fail-safe
    /// (it only shrinks dom-sets, so dominance can spuriously FAIL, never
    /// spuriously hold), but it silently suppresses most sound frees, so it is
    /// fixed here.
    fn compute_dominators(blocks: &[MirBlock]) -> Vec<std::collections::HashSet<usize>> {
        crate::codegen::analysis::cfg::compute_dominators(blocks)
    }

    /// True if owned `BuildString` local `id` ESCAPES, i.e. appears anywhere
    /// other than (a) as an argument to a borrow-call, or (b) as the base of a
    /// field read whose destination pointer temp does not itself escape. A miss
    /// here frees a live or aliased value, so this scan is COMPLETE over every
    /// statement and terminator.
    fn owned_string_escapes(id: LocalId, blocks: &[MirBlock]) -> bool {
        for block in blocks {
            for stmt in &block.stmts {
                match &stmt.kind {
                    MirStmtKind::Assign { dest, value } => match value {
                        // Borrowed field read `T = id.<field>`: sound iff the
                        // destination temp `T` does not itself escape.
                        MirRValue::FieldAccess {
                            base: MirValue::Local(b),
                            ..
                        } if *b == id => {
                            if Self::ptr_temp_escapes(*dest, blocks) {
                                return true;
                            }
                        }
                        // Any other rvalue mentioning `id` (move, cast, binop,
                        // aggregate, ref, ...) lets `id` flow out: escape.
                        other => {
                            if Self::rvalue_mentions(other, id) {
                                return true;
                            }
                        }
                    },
                    MirStmtKind::DerefAssign { ptr, value } => {
                        if *ptr == id || Self::rvalue_mentions(value, id) {
                            return true;
                        }
                    }
                    MirStmtKind::FieldDerefAssign { ptr, value, .. } => {
                        if *ptr == id || Self::rvalue_mentions(value, id) {
                            return true;
                        }
                    }
                    MirStmtKind::FieldAssign { base, value, .. } => {
                        if *base == id || Self::rvalue_mentions(value, id) {
                            return true;
                        }
                    }
                    // Storing `id` (or a value derived from it) into a module
                    // global lets it outlive the function: a hard escape.
                    MirStmtKind::GlobalStore { value, .. } => {
                        if Self::rvalue_mentions(value, id) {
                            return true;
                        }
                    }
                    // Storing `id` into a slice/buffer element lets it outlive
                    // the function through the referenced storage: a hard escape.
                    MirStmtKind::IndexStore {
                        base, index, value, ..
                    } => {
                        if matches!(base, MirValue::Local(l) if *l == id)
                            || matches!(index, MirValue::Local(l) if *l == id)
                            || Self::rvalue_mentions(value, id)
                        {
                            return true;
                        }
                    }
                    MirStmtKind::StorageLive(_)
                    | MirStmtKind::StorageDead(_)
                    | MirStmtKind::Nop
                    | MirStmtKind::WorkgroupBarrier => {}
                }
            }
            if Self::terminator_lets_local_escape(&block.terminator, id) {
                return true;
            }
        }
        false
    }

    /// True if a pointer temp `t` (the destination of an `id.ptr` field read)
    /// escapes: any use other than as an argument to a borrow-call lets the
    /// borrowed pointer outlive `id`, which would dangle once `id` is freed.
    fn ptr_temp_escapes(t: LocalId, blocks: &[MirBlock]) -> bool {
        for block in blocks {
            for stmt in &block.stmts {
                match &stmt.kind {
                    MirStmtKind::Assign { value, .. } => {
                        if Self::rvalue_mentions(value, t) {
                            return true;
                        }
                    }
                    MirStmtKind::DerefAssign { ptr, value } => {
                        if *ptr == t || Self::rvalue_mentions(value, t) {
                            return true;
                        }
                    }
                    MirStmtKind::FieldDerefAssign { ptr, value, .. } => {
                        if *ptr == t || Self::rvalue_mentions(value, t) {
                            return true;
                        }
                    }
                    MirStmtKind::FieldAssign { base, value, .. } => {
                        if *base == t || Self::rvalue_mentions(value, t) {
                            return true;
                        }
                    }
                    MirStmtKind::GlobalStore { value, .. } => {
                        if Self::rvalue_mentions(value, t) {
                            return true;
                        }
                    }
                    MirStmtKind::IndexStore {
                        base, index, value, ..
                    } => {
                        if matches!(base, MirValue::Local(l) if *l == t)
                            || matches!(index, MirValue::Local(l) if *l == t)
                            || Self::rvalue_mentions(value, t)
                        {
                            return true;
                        }
                    }
                    MirStmtKind::StorageLive(_)
                    | MirStmtKind::StorageDead(_)
                    | MirStmtKind::Nop
                    | MirStmtKind::WorkgroupBarrier => {}
                }
            }
            if Self::terminator_lets_local_escape(&block.terminator, t) {
                return true;
            }
        }
        false
    }

    /// True if a terminator lets `id` escape: returned, used as a branch/assert
    /// condition or switch value, dropped, called through, or passed by value to
    /// a callee that is not a known borrow. A `Call` argument to a borrow-call is
    /// the one non-escaping appearance.
    fn terminator_lets_local_escape(term: &Option<MirTerminator>, id: LocalId) -> bool {
        let is = |val: &MirValue| matches!(val, MirValue::Local(l) if *l == id);
        match term {
            Some(MirTerminator::Call { func, args, .. }) => {
                if is(func) {
                    return true;
                }
                let borrows = Self::callee_name(func)
                    .map(Self::borrows_string_arg)
                    .unwrap_or(false);
                args.iter().any(is) && !borrows
            }
            Some(MirTerminator::Return(Some(val))) => is(val),
            Some(MirTerminator::If { cond, .. }) => is(cond),
            Some(MirTerminator::Switch { value, .. }) => is(value),
            Some(MirTerminator::Assert { cond, .. }) => is(cond),
            Some(MirTerminator::Drop { place, .. }) => {
                place.local == id
                    || place
                        .projections
                        .iter()
                        .any(|pr| matches!(pr, PlaceProjection::Index(l) if *l == id))
            }
            _ => false,
        }
    }

    /// True if `id` is mentioned anywhere in an rvalue, as a value or a place.
    /// The exhaustive match makes the compiler enforce completeness: a new
    /// `MirRValue` variant will not compile until handled here.
    fn rvalue_mentions(r: &MirRValue, id: LocalId) -> bool {
        crate::codegen::analysis::cfg::rvalue_mentions(r, id)
    }

    /// Write indentation.
    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
    }

    /// Write a line with indentation.
    fn writeln(&mut self, s: &str) {
        self.write_indent();
        self.output.push_str(s);
        self.output.push('\n');
    }

    /// Generate a fresh temp name.
    fn fresh_temp(&mut self) -> String {
        let name = format!("__tmp{}", self.temp_counter);
        self.temp_counter += 1;
        name
    }

    // =========================================================================
    // CODE GENERATION
    // =========================================================================

    fn generate_module(&mut self, module: &MirModule) -> CodegenResult<()> {
        self.string_literals = module.strings.clone();
        // A mutable global that could hold a heap string alias disables the
        // experimental drop analysis module-wide (see the field docs).
        self.module_mut_global_alias_risk = module
            .globals
            .iter()
            .any(|g| g.is_mut && Self::ty_can_hold_heap_string_alias(&g.ty));

        // Header comment
        self.output.push_str("// Generated by BuildLang Compiler\n");
        self.output.push_str("// Do not edit manually\n\n");

        // Expose POSIX declarations used by the embedded runtime when the C
        // compiler is invoked in strict C99/C11 modes on glibc-based systems.
        self.output.push_str("#ifndef _WIN32\n");
        self.output.push_str("#ifndef _POSIX_C_SOURCE\n");
        self.output.push_str("#define _POSIX_C_SOURCE 200809L\n");
        self.output.push_str("#endif\n");
        self.output.push_str("#ifndef _DEFAULT_SOURCE\n");
        self.output.push_str("#define _DEFAULT_SOURCE\n");
        self.output.push_str("#endif\n");
        self.output.push_str("#endif\n\n");

        // Prevent Windows API name collisions
        self.output.push_str("#ifdef _WIN32\n");
        self.output.push_str("#define WIN32_LEAN_AND_MEAN\n");
        self.output.push_str("#define NOGDI\n");
        self.output.push_str("#endif\n\n");

        // Standard includes
        self.output.push_str("#include <stdint.h>\n");
        self.output.push_str("#include <stdbool.h>\n");
        self.output.push_str("#include <stddef.h>\n");
        self.output.push_str("#include <stdio.h>\n");
        self.output.push_str("#include <stdlib.h>\n");
        self.output.push_str("#include <string.h>\n");
        self.output.push_str("#include <math.h>\n");
        self.output.push_str("#include <ctype.h>\n");
        self.output.push_str("#include <time.h>\n");
        self.output.push_str("#include <assert.h>\n");
        self.output.push_str("#include <stdarg.h>\n");
        self.output.push('\n');

        // FFI headers named by extern blocks' `header "..."` clauses. These let
        // BuildLang call into any C-ABI library natively: the header supplies
        // the authoritative prototypes, types, and macros. Emitted sorted and
        // de-duplicated so the output stays reproducible for receipts.
        let mut ffi_headers: Vec<&str> = module
            .functions
            .iter()
            .filter_map(|f| f.link_header.as_deref())
            .chain(
                module
                    .globals
                    .iter()
                    .filter_map(|g| g.link_header.as_deref()),
            )
            .collect();
        ffi_headers.sort_unstable();
        ffi_headers.dedup();
        if !ffi_headers.is_empty() {
            self.output.push_str("// Foreign library headers\n");
            for header in ffi_headers {
                if header.starts_with('<') {
                    // Verbatim angle-bracket form, e.g. <sqlite3.h>.
                    self.output.push_str(&format!("#include {header}\n"));
                } else {
                    // Quoted form for local/relative headers, e.g. "mylib.h".
                    self.output.push_str(&format!("#include \"{header}\"\n"));
                }
            }
            self.output.push('\n');
        }

        // Libraries named by `link "..."` clauses. Linking happens at compile
        // time, so record the requirement as a greppable note for anyone
        // inspecting the emitted C (e.g. via `buildc build --emit c`). The
        // build driver passes these to the C compiler automatically.
        let mut link_libs: Vec<&str> = module
            .functions
            .iter()
            .filter_map(|f| f.link_lib.as_deref())
            .chain(module.globals.iter().filter_map(|g| g.link_lib.as_deref()))
            .collect();
        link_libs.sort_unstable();
        link_libs.dedup();
        if !link_libs.is_empty() {
            for lib in link_libs {
                self.output.push_str(&format!("// buildc-link: {lib}\n"));
            }
            self.output.push('\n');
        }

        // Undefine known Windows API macros that collide with common type names
        self.output.push_str("#ifdef _WIN32\n");
        self.output.push_str("#undef DeviceCapabilities\n");
        self.output.push_str("#undef Rectangle\n");
        self.output.push_str("#undef CreateWindow\n");
        self.output.push_str("#undef GetMessage\n");
        self.output.push_str("#undef SendMessage\n");
        self.output.push_str("#undef LoadImage\n");
        self.output.push_str("#undef DrawText\n");
        self.output.push_str("#undef GetObject\n");
        self.output.push_str("#endif\n\n");

        // Embedded runtime library
        self.output.push_str(runtime::runtime_header());
        self.output.push('\n');

        // Type definitions
        let mut all_types = module.types.clone();
        all_types.extend(Self::synthesize_tuple_typedefs(module));
        self.generate_type_definitions(&all_types)?;

        // Vtable TYPE declarations (before forward declarations so dyn_* types are available)
        self.generate_vtable_types(module)?;

        // String table
        self.generate_string_table(&module.strings)?;

        // Global variables
        self.generate_globals(&module.globals)?;

        // Forward declarations
        self.generate_forward_declarations(&module.functions)?;

        // Vtable INSTANCES (after forward declarations so function names are known)
        self.generate_vtable_instances(module)?;

        // Build function parameter type index for auto-ref at call sites
        for func in &module.functions {
            self.fn_params
                .insert(func.name.to_string(), func.sig.params.clone());
        }

        // Generate monomorphized HashMap wrappers for non-f64 value types.
        // Scan all function locals to discover Map types used in the module.
        {
            // Collect (C type, is-scalar) for each non-f64 map value type. Scalars
            // (<= 8 bytes) ride in the map's native 8-byte slot; non-scalars (e.g.
            // BuildString, 24 bytes) are BOXED (a heap pointer rides in the slot)
            // so the full value round-trips instead of being truncated.
            let mut map_val_types: std::collections::HashMap<String, bool> =
                std::collections::HashMap::new();
            for func in &module.functions {
                for local in &func.locals {
                    if let MirType::Map(_, ref val_ty) = local.ty {
                        let val_c = self.type_to_c(val_ty);
                        if val_c != "double" {
                            let is_scalar = matches!(
                                val_ty.as_ref(),
                                MirType::Int(..)
                                    | MirType::Float(..)
                                    | MirType::Bool
                                    | MirType::Ptr(..)
                            );
                            map_val_types.insert(val_c, is_scalar);
                        }
                    }
                }
            }
            if !map_val_types.is_empty() {
                self.output
                    .push_str("// Monomorphized HashMap wrappers for non-f64 value types\n");
                // Deterministic order for reproducible codegen (receipts).
                let mut entries: Vec<(&String, &bool)> = map_val_types.iter().collect();
                entries.sort_by(|a, b| a.0.cmp(b.0));
                // The key is a `BuildString` (string-keyed map family), passed by
                // value by the method dispatch; the wrappers use its `.ptr`. Taking
                // `BuildString key` avoids a key coercion the generated wrappers
                // would otherwise need.
                for (val_c, &is_scalar) in entries {
                    let safe_name = val_c.replace("*", "ptr").replace(" ", "_");
                    // get
                    write!(self.output, "static {val_c} build_hmap_get_val_{safe_name}(BuildStrF64MapHandle h, BuildString key) {{\n").unwrap();
                    write!(self.output, "    if (!build_hmap_contains_str_f64(h, key.ptr)) {{ {val_c} __z; memset(&__z, 0, sizeof(__z)); return __z; }}\n").unwrap();
                    self.output
                        .push_str("    double __d = build_hmap_get_str_f64(h, key.ptr);\n");
                    if is_scalar {
                        write!(
                            self.output,
                            "    {val_c} __v; memset(&__v, 0, sizeof(__v));\n"
                        )
                        .unwrap();
                        self.output.push_str("    memcpy(&__v, &__d, sizeof(__d) < sizeof(__v) ? sizeof(__d) : sizeof(__v));\n    return __v;\n}\n");
                    } else {
                        write!(
                            self.output,
                            "    {val_c}* __b; memcpy(&__b, &__d, sizeof(__b));\n"
                        )
                        .unwrap();
                        write!(self.output, "    if (!__b) {{ {val_c} __z; memset(&__z, 0, sizeof(__z)); return __z; }}\n    return *__b;\n}}\n").unwrap();
                    }
                    // insert
                    write!(self.output, "static void build_hmap_insert_val_{safe_name}(BuildStrF64MapHandle h, BuildString key, {val_c} value) {{\n").unwrap();
                    self.output.push_str("    double __d = 0;\n");
                    if is_scalar {
                        self.output.push_str("    memcpy(&__d, &value, sizeof(value) < sizeof(__d) ? sizeof(value) : sizeof(__d));\n");
                    } else {
                        write!(self.output, "    {val_c}* __b = ({val_c}*)malloc(sizeof({val_c})); *__b = value; memcpy(&__d, &__b, sizeof(__b));\n").unwrap();
                    }
                    self.output
                        .push_str("    build_hmap_insert_str_f64(h, key.ptr, __d);\n}\n\n");
                }
            }
        }

        // Generate monomorphized Vec element wrappers for aggregate element
        // types (user structs etc.). The built-in i32/i64/f64/str families cover
        // scalars and strings; an aggregate element rides in the size-aware
        // generic BuildVec via a per-type wrapper so `Vec<P>` push/get/pop work.
        {
            let mut vec_elem_types: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for func in &module.functions {
                for local in &func.locals {
                    if let MirType::Vec(ref elem) = local.ty {
                        if Self::vec_elem_needs_sized_wrapper(elem) {
                            vec_elem_types.insert(self.type_to_c(elem));
                        }
                    }
                }
            }
            if !vec_elem_types.is_empty() {
                self.output.push_str(
                    "// Monomorphized Vec element wrappers for aggregate element types\n",
                );
                // Deterministic order for reproducible codegen (receipts).
                let mut entries: Vec<&String> = vec_elem_types.iter().collect();
                entries.sort();
                for elem_c in entries {
                    let suffix = elem_c.replace('*', "ptr").replace(' ', "_");
                    write!(self.output, "static BuildVecHandle build_hvec_new_{suffix}(void) {{ BuildVecHandle h; h.inner = (BuildVec*)malloc(sizeof(BuildVec)); *h.inner = build_vec_new(sizeof({elem_c})); return h; }}\n").unwrap();
                    write!(self.output, "static void build_hvec_push_{suffix}(BuildVecHandle h, {elem_c} val) {{ build_vec_push(h.inner, &val); }}\n").unwrap();
                    write!(self.output, "static {elem_c} build_hvec_get_{suffix}(BuildVecHandle h, size_t index) {{ return *({elem_c}*)build_vec_get(h.inner, index); }}\n").unwrap();
                    write!(self.output, "static {elem_c} build_hvec_pop_{suffix}(BuildVecHandle h) {{ {elem_c} __z; memset(&__z, 0, sizeof(__z)); if (h.inner->len == 0) return __z; h.inner->len--; return *({elem_c}*)((char*)h.inner->ptr + h.inner->len * h.inner->elem_size); }}\n").unwrap();
                }
                self.output.push('\n');
            }
        }

        // Function definitions
        for func in &module.functions {
            if !func.is_declaration() {
                self.generate_function(func)?;
            }
        }

        Ok(())
    }

    /// Synthesize a MirTypeDef for every tuple type referenced anywhere in the
    /// module (function signatures, locals, and type-def fields/variants) that
    /// was not already registered. Tuple types appear by value as params, locals,
    /// and fields, but only literals and return types were registered during
    /// lowering, so the rest reached the backend with no typedef. The topological
    /// emitter (collect_type_deps) orders each after its element types.
    fn synthesize_tuple_typedefs(module: &MirModule) -> Vec<MirTypeDef> {
        use std::collections::HashSet;
        fn visit(
            ty: &MirType,
            existing: &HashSet<String>,
            seen: &mut HashSet<String>,
            out: &mut Vec<MirTypeDef>,
        ) {
            match ty {
                MirType::Tuple(elems) => {
                    if !elems.is_empty() {
                        let name = MirType::tuple_type_name(elems);
                        let key = name.to_string();
                        if !existing.contains(&key) && seen.insert(key) {
                            let fields: Vec<(Option<Arc<str>>, MirType)> = elems
                                .iter()
                                .enumerate()
                                .map(|(i, t)| (Some(Arc::from(format!("_{}", i))), t.clone()))
                                .collect();
                            out.push(MirTypeDef {
                                name,
                                kind: TypeDefKind::Struct {
                                    fields,
                                    packed: false,
                                },
                            });
                        }
                    }
                    for e in elems {
                        visit(e, existing, seen, out);
                    }
                }
                MirType::Array(inner, _)
                | MirType::Slice(inner)
                | MirType::Ptr(inner)
                | MirType::Vec(inner) => visit(inner, existing, seen, out),
                MirType::Map(k, v) => {
                    visit(k, existing, seen, out);
                    visit(v, existing, seen, out);
                }
                MirType::FnPtr(sig) => {
                    for prm in &sig.params {
                        visit(prm, existing, seen, out);
                    }
                    visit(&sig.ret, existing, seen, out);
                }
                _ => {}
            }
        }
        let existing: HashSet<String> = module.types.iter().map(|t| t.name.to_string()).collect();
        let mut seen: HashSet<String> = HashSet::new();
        let mut out: Vec<MirTypeDef> = Vec::new();
        for td in &module.types {
            match &td.kind {
                TypeDefKind::Struct { fields, .. } => {
                    for (_, ft) in fields {
                        visit(ft, &existing, &mut seen, &mut out);
                    }
                }
                TypeDefKind::Enum { variants, .. } => {
                    for v in variants {
                        for (_, ft) in &v.fields {
                            visit(ft, &existing, &mut seen, &mut out);
                        }
                    }
                }
                TypeDefKind::Union { variants } => {
                    for (_, vt) in variants {
                        visit(vt, &existing, &mut seen, &mut out);
                    }
                }
            }
        }
        for f in &module.functions {
            for prm in &f.sig.params {
                visit(prm, &existing, &mut seen, &mut out);
            }
            visit(&f.sig.ret, &existing, &mut seen, &mut out);
            for l in &f.locals {
                visit(&l.ty, &existing, &mut seen, &mut out);
            }
        }
        out
    }

    fn generate_type_definitions(&mut self, types: &[MirTypeDef]) -> CodegenResult<()> {
        if types.is_empty() {
            return Ok(());
        }

        // Pre-emit typedefs for tuple types used in struct fields.
        // (f32, f32) → typedef struct Tuple_f32_f32 { float field0; float field1; } Tuple_f32_f32;
        let type_names: std::collections::HashSet<&str> =
            types.iter().map(|t| t.name.as_ref()).collect();
        let mut emitted_tuples = std::collections::HashSet::new();
        for ty in types {
            if let TypeDefKind::Struct { fields, .. } = &ty.kind {
                for (_, field_ty) in fields {
                    if let MirType::Struct(name) = field_ty {
                        if name.starts_with("Tuple_")
                            && !type_names.contains(name.as_ref())
                            && !emitted_tuples.contains(name.as_ref())
                        {
                            emitted_tuples.insert(name.to_string());
                            // Parse the tuple element types from the mangled name
                            let parts: Vec<&str> = name[6..].split('_').collect();
                            write!(self.output, "typedef struct {} {{\n", name).unwrap();
                            for (i, part) in parts.iter().enumerate() {
                                let c_type = match *part {
                                    "f32" => "float",
                                    "f64" => "double",
                                    "i32" => "int32_t",
                                    "i64" => "int64_t",
                                    "u32" => "uint32_t",
                                    "u64" => "uint64_t",
                                    "bool" => "bool",
                                    _ => "int32_t",
                                };
                                write!(self.output, "    {} field{};\n", c_type, i).unwrap();
                            }
                            write!(self.output, "}} {};\n\n", name).unwrap();
                        }
                    }
                    // A tuple field may instead be carried as MirType::Tuple
                    // (not Struct("Tuple_..")); emit its typedef too, deriving
                    // field C types from the element types directly.
                    if let MirType::Tuple(elems) = field_ty {
                        // Only all-primitive tuples are safe to pre-emit here:
                        // a tuple of named structs would reference a struct not
                        // yet defined at this point (emit ordering). Those remain
                        // a known gap for the topological type emitter.
                        let all_prim = elems.iter().all(|e| {
                            matches!(e, MirType::Int(..) | MirType::Float(..) | MirType::Bool)
                        });
                        if !elems.is_empty() && all_prim {
                            let name = MirType::tuple_type_name(elems);
                            if !type_names.contains(name.as_ref())
                                && !emitted_tuples.contains(name.as_ref())
                            {
                                emitted_tuples.insert(name.to_string());
                                let field_ctypes: Vec<String> =
                                    elems.iter().map(|e| self.type_to_c(e)).collect();
                                write!(self.output, "typedef struct {} {{\n", name).unwrap();
                                for (i, ct) in field_ctypes.iter().enumerate() {
                                    write!(self.output, "    {} field{};\n", ct, i).unwrap();
                                }
                                write!(self.output, "}} {};\n\n", name).unwrap();
                            }
                        }
                    }
                }
            }
        }

        // Runtime-provided types that must not be re-emitted.
        const RUNTIME_TYPES: &[&str] = &[
            "build_vec2",
            "build_vec3",
            "build_vec4",
            "build_mat4",
            "VecDeque",
            "HashSet",
            "Option",
            "Result",
        ];

        // Emit forward declarations for struct/union/enum types.
        // Enums are emitted as tagged structs, so they get struct forward
        // declarations.  The body later uses `struct X { ... };` (not
        // `typedef struct X { ... } X;`) to avoid MSVC redefinition errors.
        self.output.push_str("// Forward declarations\n");
        for ty in types {
            if RUNTIME_TYPES.contains(&ty.name.as_ref()) {
                continue;
            }
            match &ty.kind {
                TypeDefKind::Struct { .. } | TypeDefKind::Enum { .. } => {
                    write!(self.output, "typedef struct {} {};\n", ty.name, ty.name).unwrap();
                }
                TypeDefKind::Union { .. } => {
                    write!(self.output, "typedef union {} {};\n", ty.name, ty.name).unwrap();
                }
            }
        }
        self.output.push('\n');

        // Topological sort: emit types in dependency order.
        // A type must be emitted after all types it references by value.
        let type_names: std::collections::HashSet<&str> =
            types.iter().map(|t| t.name.as_ref()).collect();

        let mut emitted: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for rt in RUNTIME_TYPES {
            emitted.insert(rt);
        }

        // Recursively collect named-type dependencies from a MirType.
        // Only value types (not behind a pointer) require ordering.
        fn collect_type_deps(
            mir_ty: &MirType,
            type_names: &std::collections::HashSet<&str>,
            out: &mut Vec<String>,
        ) {
            match mir_ty {
                MirType::Struct(name) => {
                    if type_names.contains(name.as_ref()) {
                        out.push(name.to_string());
                    }
                }
                MirType::Array(inner, _) | MirType::Slice(inner) => {
                    collect_type_deps(inner, type_names, out);
                }
                MirType::Tuple(elems) => {
                    // A tuple used by value (e.g. as a struct field) depends on its
                    // own typedef being emitted first; register that dependency.
                    let tname = MirType::tuple_type_name(elems);
                    if type_names.contains(tname.as_ref()) {
                        out.push(tname.to_string());
                    } else {
                        for e in elems {
                            collect_type_deps(e, type_names, out);
                        }
                    }
                }
                // Vec/Map/Ptr are behind pointers - no ordering needed
                _ => {}
            }
        }

        // Collect value dependencies for each type
        let deps: std::collections::HashMap<&str, Vec<String>> = types
            .iter()
            .map(|ty| {
                let mut d = Vec::new();
                match &ty.kind {
                    TypeDefKind::Struct { fields, .. } => {
                        for (_, ft) in fields {
                            collect_type_deps(ft, &type_names, &mut d);
                        }
                    }
                    TypeDefKind::Enum { variants, .. } => {
                        for v in variants {
                            for (_, ft) in &v.fields {
                                collect_type_deps(ft, &type_names, &mut d);
                            }
                        }
                    }
                    TypeDefKind::Union { variants } => {
                        for (_, vt) in variants {
                            collect_type_deps(vt, &type_names, &mut d);
                        }
                    }
                }
                (ty.name.as_ref(), d)
            })
            .collect();

        // Emit types in dependency order (simple iterative approach)
        self.output
            .push_str("// Type definitions (dependency-ordered)\n");
        let mut remaining: Vec<&MirTypeDef> = types
            .iter()
            .filter(|t| !RUNTIME_TYPES.contains(&t.name.as_ref()))
            .collect();
        let max_passes = remaining.len() + 1;
        for _ in 0..max_passes {
            if remaining.is_empty() {
                break;
            }
            let mut next_remaining = Vec::new();
            for ty in &remaining {
                let type_deps = deps.get(ty.name.as_ref()).cloned().unwrap_or_default();
                if type_deps.iter().all(|d| emitted.contains(d.as_str())) {
                    self.emit_type_def(ty);
                    emitted.insert(ty.name.as_ref());
                } else {
                    next_remaining.push(*ty);
                }
            }
            if next_remaining.len() == remaining.len() {
                // Circular dependency - emit remaining in original order
                for ty in &next_remaining {
                    self.emit_type_def(ty);
                }
                break;
            }
            remaining = next_remaining;
        }

        Ok(())
    }

    fn emit_type_def(&mut self, ty: &MirTypeDef) {
        match &ty.kind {
            TypeDefKind::Struct { fields, packed } => {
                if *packed {
                    self.output.push_str("#pragma pack(push, 1)\n");
                }
                // Use struct tag only (not typedef) since forward decl already typedef'd
                write!(self.output, "struct {} {{\n", ty.name).unwrap();
                self.indent += 1;
                // C requires at least one member in a struct
                if fields.is_empty() {
                    self.write_indent();
                    self.output.push_str("char _pad;\n");
                }
                for (i, (name, field_ty)) in fields.iter().enumerate() {
                    self.write_indent();
                    let field_name = name
                        .as_ref()
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| format!("field{}", i));
                    // Escape C reserved words in field names
                    let field_name = Self::escape_c_keyword(&field_name);
                    if matches!(field_ty, MirType::Array(_, _)) {
                        write!(
                            self.output,
                            "{};\n",
                            self.fmt_array_decl(field_ty, &field_name)
                        )
                        .unwrap();
                    } else if let MirType::FnPtr(ref sig) = field_ty {
                        // Function pointer fields need special syntax:
                        // ret_type (*field_name)(param_types)
                        let ret = self.type_to_c(&sig.ret);
                        let params: Vec<_> = sig.params.iter().map(|p| self.type_to_c(p)).collect();
                        write!(
                            self.output,
                            "{} (*{})({}){}\n",
                            ret,
                            field_name,
                            params.join(", "),
                            ";"
                        )
                        .unwrap();
                    } else {
                        write!(
                            self.output,
                            "{} {};\n",
                            self.type_to_c(field_ty),
                            field_name
                        )
                        .unwrap();
                    }
                }
                self.indent -= 1;
                self.output.push_str("};\n\n");
                if *packed {
                    self.output.push_str("#pragma pack(pop)\n");
                }
            }
            TypeDefKind::Union { variants } => {
                write!(self.output, "union {} {{\n", ty.name).unwrap();
                self.indent += 1;
                for (name, var_ty) in variants {
                    self.write_indent();
                    write!(self.output, "{} {};\n", self.type_to_c(var_ty), name).unwrap();
                }
                self.indent -= 1;
                self.output.push_str("};\n\n");
            }
            TypeDefKind::Enum {
                discriminant_ty: _,
                variants,
            } => {
                // Generate enum discriminants
                write!(self.output, "typedef enum {{\n").unwrap();
                self.indent += 1;
                for variant in variants {
                    self.write_indent();
                    write!(
                        self.output,
                        "{}_{} = {},\n",
                        ty.name, variant.name, variant.discriminant
                    )
                    .unwrap();
                }
                self.indent -= 1;
                write!(self.output, "}} {}_Tag;\n\n", ty.name).unwrap();

                // Generate tagged union (forward decl already typedef'd)
                write!(self.output, "struct {} {{\n", ty.name).unwrap();
                self.indent += 1;
                self.write_indent();
                write!(self.output, "{}_Tag tag;\n", ty.name).unwrap();
                self.write_indent();
                self.output.push_str("union {\n");
                self.indent += 1;
                for variant in variants {
                    if !variant.fields.is_empty() {
                        self.write_indent();
                        self.output.push_str("struct {\n");
                        self.indent += 1;
                        for (i, (fname, fty)) in variant.fields.iter().enumerate() {
                            self.write_indent();
                            let field_name = fname
                                .as_ref()
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| format!("f{}", i));
                            let field_name = Self::escape_c_keyword(&field_name);
                            if matches!(fty, MirType::Array(_, _)) {
                                write!(self.output, "{};\n", self.fmt_array_decl(fty, &field_name))
                                    .unwrap();
                            } else if let MirType::FnPtr(ref sig) = fty {
                                let ret = self.type_to_c(&sig.ret);
                                let params: Vec<_> =
                                    sig.params.iter().map(|p| self.type_to_c(p)).collect();
                                write!(
                                    self.output,
                                    "{} (*{})({}){}\n",
                                    ret,
                                    field_name,
                                    params.join(", "),
                                    ";"
                                )
                                .unwrap();
                            } else {
                                write!(self.output, "{} {};\n", self.type_to_c(fty), field_name)
                                    .unwrap();
                            }
                        }
                        self.indent -= 1;
                        self.write_indent();
                        write!(self.output, "}} {};\n", variant.name).unwrap();
                    } else {
                        // Unit variant: add empty struct so it can be
                        // referenced in designated initializers
                        self.write_indent();
                        // Placeholder field name must not be a reserved C identifier (e.g. a
                        // variant named Bool would yield _Bool, the C99 boolean keyword).
                        // It is pure padding, never referenced.
                        let placeholder = format!("_{}", variant.name);
                        let placeholder = if Self::is_c_reserved(&placeholder) {
                            format!("{}_", placeholder)
                        } else {
                            placeholder
                        };
                        write!(self.output, "char {};\n", placeholder).unwrap();
                    }
                }
                self.indent -= 1;
                self.write_indent();
                self.output.push_str("} data;\n");
                self.indent -= 1;
                self.output.push_str("};\n\n");
            }
        }
    }

    fn generate_vtable_types(&mut self, module: &MirModule) -> CodegenResult<()> {
        if module.trait_methods.is_empty() {
            return Ok(());
        }

        self.output
            .push_str("// Vtable types for dynamic dispatch\n");

        for (trait_name, methods) in &module.trait_methods {
            write!(self.output, "typedef struct {}_vtable {{\n", trait_name).unwrap();
            for (method_name, sig) in methods {
                let ret = self.type_to_c(&sig.ret);
                let params: Vec<String> = sig.params.iter().map(|p| self.type_to_c(p)).collect();
                write!(
                    self.output,
                    "    {} (*{})({});\n",
                    ret,
                    method_name,
                    params.join(", ")
                )
                .unwrap();
            }
            write!(self.output, "}} {}_vtable;\n\n", trait_name).unwrap();

            write!(self.output, "typedef struct dyn_{} {{\n", trait_name).unwrap();
            write!(self.output, "    void* data;\n").unwrap();
            write!(self.output, "    {}_vtable* vtable;\n", trait_name).unwrap();
            write!(self.output, "}} dyn_{};\n\n", trait_name).unwrap();
        }

        Ok(())
    }

    fn generate_vtable_instances(&mut self, module: &MirModule) -> CodegenResult<()> {
        if module.vtables.is_empty() {
            return Ok(());
        }

        // The vtable method sigs carry `void*` as the self param (that is the
        // C-level type stored in the function-pointer table), so they cannot say
        // whether the concrete method takes `self` by value or `&self` by
        // reference. Read that from the concrete function's real first parameter
        // instead, keyed by its mangled name.
        let concrete_self_by_ref: std::collections::HashMap<&str, bool> = module
            .functions
            .iter()
            .map(|f| {
                (
                    f.name.as_ref(),
                    f.sig.params.first().map(|p| p.is_pointer()).unwrap_or(false),
                )
            })
            .collect();

        // Generate wrapper functions that dereference void* to concrete type
        for vtable in &module.vtables {
            for (method_name, mangled_fn, sig) in &vtable.methods {
                let ret = self.type_to_c(&sig.ret);
                let wrapper_name = format!(
                    "__vtable_wrap_{}_{}_{}",
                    vtable.type_name, vtable.trait_name, method_name
                );

                // Generate: ret wrapper(void* self, ...) { return concrete(<self>, ...); }
                // The receiver is passed as a pointer when the method takes
                // `&self`/`&mut self` (a pointer self param), or dereferenced to a
                // value when it takes `self` by value. The vtable sig always
                // reports the self param as `void*`, so consult the concrete
                // function's real first parameter to decide.
                let self_is_ref = concrete_self_by_ref
                    .get(mangled_fn.as_ref())
                    .copied()
                    .unwrap_or_else(|| {
                        sig.params.first().map(|p| p.is_pointer()).unwrap_or(false)
                    });
                let self_arg = if self_is_ref {
                    format!("({}*)__self", vtable.type_name)
                } else {
                    format!("(*({}*)__self)", vtable.type_name)
                };
                let mut wrapper_params = vec!["void* __self".to_string()];
                let mut call_args = vec![self_arg];
                for (i, param) in sig.params.iter().skip(1).enumerate() {
                    let param_ty = self.type_to_c(param);
                    wrapper_params.push(format!("{} __arg{}", param_ty, i));
                    call_args.push(format!("__arg{}", i));
                }

                write!(
                    self.output,
                    "static {} {}({}) {{ return {}({}); }}\n",
                    ret,
                    wrapper_name,
                    wrapper_params.join(", "),
                    mangled_fn,
                    call_args.join(", ")
                )
                .unwrap();
            }
        }
        self.output.push('\n');

        // Generate vtable instances using wrapper functions
        for vtable in &module.vtables {
            write!(
                self.output,
                "static {}_vtable {}_{}_vtable_instance = {{\n",
                vtable.trait_name, vtable.type_name, vtable.trait_name
            )
            .unwrap();

            for (method_name, _, sig) in &vtable.methods {
                let ret = self.type_to_c(&sig.ret);
                let params: Vec<String> = sig.params.iter().map(|p| self.type_to_c(p)).collect();
                let wrapper_name = format!(
                    "__vtable_wrap_{}_{}_{}",
                    vtable.type_name, vtable.trait_name, method_name
                );
                write!(
                    self.output,
                    "    .{} = ({} (*)({})){},\n",
                    method_name,
                    ret,
                    params.join(", "),
                    wrapper_name
                )
                .unwrap();
            }
            write!(self.output, "}};\n\n").unwrap();
        }

        Ok(())
    }

    fn generate_string_table(&mut self, strings: &[Arc<str>]) -> CodegenResult<()> {
        if strings.is_empty() {
            return Ok(());
        }

        self.output.push_str("// String table\n");
        for (i, s) in strings.iter().enumerate() {
            let escaped = self.escape_string(s);
            write!(
                self.output,
                "static const char* __str{} = \"{}\";\n",
                i, escaped
            )
            .unwrap();
        }
        self.output.push('\n');

        Ok(())
    }

    fn generate_globals(&mut self, globals: &[MirGlobal]) -> CodegenResult<()> {
        if globals.is_empty() {
            return Ok(());
        }

        self.output.push_str("// Global variables\n");
        for global in globals {
            let c_type = self.type_to_c(&global.ty);

            // A foreign `static` is an external declaration, never a definition.
            // If a header backs it, the header declares it and we emit nothing;
            // otherwise emit a bare `extern` declaration to reference the symbol.
            if global.is_extern_decl {
                if global.link_header.is_none() {
                    self.output
                        .push_str(&format!("extern {} {};\n", c_type, global.name));
                }
                continue;
            }

            let mut decl = if global.is_mut {
                format!("{} {}", c_type, global.name)
            } else {
                format!("const {} {}", c_type, global.name)
            };

            if let Some(init) = &global.init {
                decl.push_str(" = ");
                decl.push_str(&self.const_to_c_static(init));
            }

            decl.push_str(";\n");
            self.output.push_str(&decl);
        }
        self.output.push('\n');

        Ok(())
    }

    fn generate_forward_declarations(&mut self, functions: &[MirFunction]) -> CodegenResult<()> {
        // Separate extern "C" declarations from regular forward declarations.
        let extern_fns: Vec<_> = functions
            .iter()
            .filter(|f| f.is_declaration() && f.sig.calling_conv == CallingConv::C)
            .collect();
        let regular_fns: Vec<_> = functions
            .iter()
            .filter(|f| !(f.is_declaration() && f.sig.calling_conv == CallingConv::C))
            .collect();

        // Extern "C" functions: skip declarations for standard C library
        // functions that are already available through the included headers.
        // For non-standard FFI functions, emit a proper extern declaration.
        if !extern_fns.is_empty() {
            let std_c_fns = [
                "printf",
                "fprintf",
                "sprintf",
                "snprintf",
                "scanf",
                "sscanf",
                "puts",
                "putchar",
                "getchar",
                "gets",
                "fgets",
                "fputs",
                "fopen",
                "fclose",
                "fread",
                "fwrite",
                "fseek",
                "ftell",
                "rewind",
                "malloc",
                "calloc",
                "realloc",
                "free",
                "memcpy",
                "memset",
                "memmove",
                "memcmp",
                "strlen",
                "strcpy",
                "strncpy",
                "strcat",
                "strncat",
                "strcmp",
                "strncmp",
                "atoi",
                "atof",
                "atol",
                "strtol",
                "strtod",
                "abs",
                "labs",
                "div",
                "rand",
                "srand",
                "exit",
                "abort",
                "atexit",
                "qsort",
                "bsearch",
                "sin",
                "cos",
                "tan",
                "asin",
                "acos",
                "atan",
                "atan2",
                "sinh",
                "cosh",
                "tanh",
                "sqrt",
                "cbrt",
                "pow",
                "exp",
                "exp2",
                "log",
                "log10",
                "log2",
                "ceil",
                "floor",
                "round",
                "trunc",
                "fabs",
                "fmod",
                "fmax",
                "fmin",
                "hypot",
                "copysign",
                "sinf",
                "cosf",
                "tanf",
                "sqrtf",
                "powf",
                "expf",
                "logf",
                "ceilf",
                "floorf",
                "roundf",
                "fabsf",
                "fmodf",
                "time",
                "clock",
                "isalpha",
                "isdigit",
                "isalnum",
                "isspace",
                "toupper",
                "tolower",
                // BuildLang runtime-provided functions (graphics stub, file I/O)
                "build_gfx_init",
                "build_gfx_load_shader",
                "build_gfx_create_pipeline",
                "build_gfx_begin_frame",
                "build_gfx_clear",
                "build_gfx_draw",
                "build_gfx_end_frame",
                "build_gfx_should_close",
                "build_gfx_shutdown",
                // Directory traversal (provided by runtime)
                "build_list_dir",
                "build_is_dir",
                "build_file_size",
                // String vec handle (provided by runtime)
                "build_hvec_new_str",
                "build_hvec_push_str",
                "build_hvec_get_str",
                // TCP socket functions (provided by runtime)
                "build_tcp_connect",
                "build_tcp_send",
                "build_tcp_recv",
                "build_tcp_close",
            ];

            let non_std: Vec<_> = extern_fns
                .iter()
                // Skip standard-library functions (provided by the standard
                // includes) and header-backed functions (provided by their
                // declared `header "..."` include), so we never synthesize a
                // prototype that could conflict with the real one.
                .filter(|f| !std_c_fns.contains(&f.name.as_ref()) && f.link_header.is_none())
                .collect();

            if !non_std.is_empty() {
                self.output.push_str("// Extern function declarations\n");
                for func in non_std {
                    self.output.push_str("extern ");
                    self.generate_function_signature(func)?;
                    self.output.push_str(";\n");
                }
                self.output.push('\n');
            }
        }

        // Regular forward declarations for BuildLang-defined functions.
        // Skip declaration-only functions (no body) - they have no param info
        // and would generate incorrect `func(void)` forward declarations.
        self.output.push_str("// Forward declarations\n");
        for func in regular_fns {
            if !func.is_declaration() {
                self.generate_function_signature(func)?;
                self.output.push_str(";\n");
            }
        }
        self.output.push('\n');

        Ok(())
    }

    fn generate_function(&mut self, func: &MirFunction) -> CodegenResult<()> {
        self.current_ret_ty = func.sig.ret.clone();
        self.current_fn_name = Some(func.name.to_string());
        self.local_string_literals.clear();
        self.current_fn_freeable = if self.experimental_free_enabled() {
            self.freeable_owned_string_locals(func)
        } else {
            Vec::new()
        };
        self.current_fn_block_frees = if self.experimental_free_enabled() {
            self.block_scoped_freeable(func, &self.current_fn_freeable)
        } else {
            std::collections::HashMap::new()
        };
        self.current_fn_flag_owners = Vec::new();
        self.current_fn_flag_block_frees = std::collections::HashMap::new();
        if self.experimental_free_enabled() {
            // Increment 4: multi-block live ranges the block-scoped (single-block)
            // rule declines. Disjoint from both prior sets.
            let fn_exit_set: std::collections::HashSet<LocalId> =
                self.current_fn_freeable.iter().copied().collect();
            let block_scoped_set: std::collections::HashSet<LocalId> = self
                .current_fn_block_frees
                .values()
                .flatten()
                .copied()
                .collect();
            let candidates = self.sound_owned_candidates(func);
            let extra = crate::codegen::analysis::drops::multi_block_freeable(
                func,
                &candidates,
                &fn_exit_set,
                &block_scoped_set,
            );
            let multi_block_set: std::collections::HashSet<LocalId> =
                extra.values().flatten().copied().collect();
            for (bb, ids) in extra {
                let slot = self.current_fn_block_frees.entry(bb).or_default();
                for id in ids {
                    if !slot.contains(&id) {
                        slot.push(id);
                    }
                }
                slot.sort_by_key(|id| id.0);
            }

            // Increment 5: split-frontier / conditional-allocation drop flags.
            // `claimed` is every owner already freed by increments 1-4
            // (fn-exit, block-scoped, and increment 4's multi-block set):
            // disjointness, so no owner is freed by two mechanisms. Reuses
            // `candidates` (the same escape-filtered ownership/move/taint/
            // one-def gates) computed above for increment 4.
            let claimed: std::collections::HashSet<LocalId> = fn_exit_set
                .iter()
                .copied()
                .chain(block_scoped_set.iter().copied())
                .chain(multi_block_set.iter().copied())
                .collect();
            let flag_frees = crate::codegen::analysis::flags::split_frontier_flag_frees(
                func,
                &candidates,
                &claimed,
            );
            self.current_fn_flag_owners = flag_frees.owners;
            self.current_fn_flag_block_frees = flag_frees.block_frees;
        }
        self.generate_function_signature(func)?;
        self.output.push_str(" {\n");
        self.indent += 1;

        // For main(), initialize I/O and command-line args before anything else
        if func.name.as_ref() == "main" {
            self.write_indent();
            self.output.push_str("__build_init_io();\n");
            self.write_indent();
            self.output.push_str("build_args_init(argc, argv);\n");
        }

        // Dead local elimination: collect all referenced local IDs from the
        // function body. Only declare locals that are actually used.
        let used_locals = Self::collect_used_locals(func);

        // Generate local declarations (skip dead locals)
        for local in &func.locals {
            if !local.is_param {
                // Skip void-typed locals -- C does not allow `void x;`
                if matches!(local.ty, MirType::Void) {
                    continue;
                }
                // Skip locals that are never referenced in the function body
                if !used_locals.contains(&local.id) {
                    continue;
                }
                self.write_indent();
                let name = self.local_name(local.id, &func.locals);
                // Arrays need special C declaration syntax: `type name[size]`
                // Function pointers need: `ret (*name)(params)`
                if matches!(local.ty, MirType::Array(_, _)) {
                    write!(self.output, "{};\n", self.fmt_array_decl(&local.ty, &name)).unwrap();
                } else if let MirType::FnPtr(ref sig) = local.ty {
                    let ret = self.type_to_c(&sig.ret);
                    let params: Vec<_> = sig.params.iter().map(|p| self.type_to_c(p)).collect();
                    write!(self.output, "{} (*{})({});\n", ret, name, params.join(", ")).unwrap();
                } else {
                    write!(self.output, "{} {};\n", self.type_to_c(&local.ty), name).unwrap();
                }
            }
        }

        // Increment 5: declare a runtime drop flag for each enrolled owner,
        // initialized to 0 (not yet allocated). Keyed by the owner's numeric
        // LocalId so it cannot collide with another flag; the `__bl_` prefix
        // matches existing emitted-runtime naming (`__build_init_io`).
        // Residual collision with a user identifier literally named
        // `__bl_live_N` is accepted as negligible (see docs/MEMORY-PILLAR-DESIGN.md).
        // Soundness invariant (verbatim, also in analysis::flags and the
        // guarded-free emission below): at every point in the emitted C,
        // `__bl_live_N == 1` implies the owner currently holds a fresh
        // allocated buffer that no free site has released.
        let flag_owners = self.current_fn_flag_owners.clone();
        for owner in flag_owners {
            self.write_indent();
            writeln!(self.output, "uint8_t __bl_live_{} = 0;", owner.0).unwrap();
        }

        // Emit parameter aliases when MIR renames differ from C signature names.
        // The C signature uses the original name (e.g., `x`) but the MIR body
        // uses a disambiguated name (e.g., `x_0`). Emit `type x_0 = x;` so the
        // body can reference the parameter by its MIR name.
        for local in &func.locals {
            if local.is_param && !matches!(local.ty, MirType::Void) {
                let mir_name = self.local_name(local.id, &func.locals);
                if let Some(ref orig) = local.name {
                    let orig_str = orig.to_string();
                    let c_name = if Self::is_c_reserved(&orig_str) {
                        format!("_{}", orig_str)
                    } else {
                        orig_str
                    };
                    if mir_name != c_name {
                        self.write_indent();
                        write!(
                            self.output,
                            "{} {} = {};\n",
                            self.type_to_c(&local.ty),
                            mir_name,
                            c_name
                        )
                        .unwrap();
                    }
                }
            }
        }

        if !func.locals.iter().filter(|l| !l.is_param).next().is_none() {
            self.output.push('\n');
        }

        // Generate basic blocks (all labels emitted; trivial goto→label pairs
        // are cleaned up by eliminate_trivial_gotos after generation).
        if let Some(blocks) = &func.blocks {
            for (i, block) in blocks.iter().enumerate() {
                // Generate label (except for entry block)
                if i > 0 || block.label.is_some() {
                    let label = block
                        .label
                        .as_ref()
                        .map(|l| l.to_string())
                        .unwrap_or_else(|| format!("bb{}", block.id.0));
                    write!(self.output, "{}:\n", label).unwrap();
                }

                // Block-scoped drops: free owned heap locals at the START of this
                // block, after the predecessor's terminator has consumed any
                // borrow. Disjoint from the function-exit set; empty unless the
                // experimental path is enabled.
                if let Some(frees) = self.current_fn_block_frees.get(&block.id.0).cloned() {
                    for id in frees {
                        let name = self.local_name(id, &func.locals);
                        self.write_indent();
                        writeln!(self.output, "build_string_free({});", name).unwrap();
                    }
                }

                // Increment 5: guarded frees at the split-frontier / conditional-
                // allocation death sites `analysis::flags` computed. Unlike the
                // unguarded loop above, each of these is wrapped in the owner's
                // `__bl_live_N` flag test (see `emit_flag_guarded_free`).
                if let Some(flag_frees) = self.current_fn_flag_block_frees.get(&block.id.0).cloned()
                {
                    for id in flag_frees {
                        self.emit_flag_guarded_free(id, &func.locals);
                    }
                }

                // Generate statements
                for stmt in &block.stmts {
                    self.generate_statement(stmt, &func.locals)?;
                }

                // Generate terminator
                if let Some(term) = &block.terminator {
                    self.generate_terminator(term, &func.locals, blocks)?;
                } else if block.stmts.is_empty() && (i > 0 || block.label.is_some()) {
                    self.write_indent();
                    self.output.push_str("(void)0;\n");
                }
            }
        }

        self.indent -= 1;
        self.output.push_str("}\n\n");

        // Post-process optimizations on generated C output:
        // 1. Trivial goto elimination: remove goto→label pairs for sequential blocks
        // 2. Copy propagation: inline single-use temporaries
        self.eliminate_trivial_gotos();
        // Copy propagation needs MIR-level dataflow analysis to be correct.
        // Text-based approach fails on reassigned temps and cross-block values.
        // Deferred to MIR optimization pass.
        // self.propagate_copies();

        Ok(())
    }

    /// Remove `goto bbN;\nbbN:\n` pairs from the output where the goto targets
    /// the immediately following label. Preserves the label if other jumps
    /// reference it; removes both if the label has only one predecessor.
    fn eliminate_trivial_gotos(&mut self) {
        use std::collections::HashSet;
        // Find all goto targets in the output to know which labels are multi-referenced
        let mut multi_ref_labels: HashSet<String> = HashSet::new();
        let mut label_refs: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for line in self.output.lines() {
            let trimmed = line.trim();
            if let Some(target) = trimmed.strip_prefix("goto ") {
                if let Some(label) = target.strip_suffix(';') {
                    *label_refs.entry(label.to_string()).or_insert(0) += 1;
                }
            }
        }
        for (label, count) in &label_refs {
            if *count > 1 {
                multi_ref_labels.insert(label.clone());
            }
        }

        let old = std::mem::take(&mut self.output);
        let lines: Vec<&str> = old.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim();
            // Check for `goto bbN;` followed by `bbN:`
            if let Some(target) = trimmed.strip_prefix("goto ") {
                if let Some(label) = target.strip_suffix(';') {
                    let expected_label = format!("{}:", label);
                    if i + 1 < lines.len() && lines[i + 1].trim() == expected_label {
                        // Skip the goto; keep the label only if multi-referenced
                        if multi_ref_labels.contains(label) {
                            // Keep the label, skip the goto
                            i += 1;
                            continue;
                        } else {
                            // Skip both goto and label
                            i += 2;
                            continue;
                        }
                    }
                }
            }
            self.output.push_str(lines[i]);
            self.output.push('\n');
            i += 1;
        }
    }

    /// Copy propagation: inline single-use temporaries.
    /// When `_N = expr;` appears and `_N` is used exactly once on the next line
    /// as a simple value (not an lvalue), replace the use with expr and remove
    /// the assignment.
    ///
    /// `_1 = add(3, 4);  result = _1;` → `result = add(3, 4);`
    /// `_3 = __str0;  printf(_3, x);` → `printf(__str0, x);`
    fn propagate_copies(&mut self) {
        // Run multiple passes until no more changes (cascading copies)
        for _ in 0..3 {
            if !self.propagate_copies_pass() {
                break;
            }
        }
    }

    fn propagate_copies_pass(&mut self) -> bool {
        let old = std::mem::take(&mut self.output);
        let lines: Vec<&str> = old.lines().collect();
        let mut result = Vec::with_capacity(lines.len());
        let mut changed = false;
        let mut skip_next = false;

        for i in 0..lines.len() {
            if skip_next {
                skip_next = false;
                continue;
            }

            let trimmed = lines[i].trim();

            // Match pattern: `_N = expr;` where _N is a MIR temporary
            if let Some((temp_name, expr)) = Self::parse_temp_assign(trimmed) {
                // Check if the temp is used exactly once in the NEXT line
                if i + 1 < lines.len() {
                    let next = lines[i + 1];
                    let next_trimmed = next.trim();
                    let occurrences = Self::count_ident_occurrences(next_trimmed, &temp_name);

                    // Skip if the next line REASSIGNS the same temp (lvalue use)
                    let next_is_reassign = next_trimmed.starts_with(&format!("{} = ", temp_name))
                        || next_trimmed.starts_with(&format!("{} =", temp_name));
                    // Inline if: used exactly once on next line, not as lvalue,
                    // and expr is safe to inline (no side effects when reordered)
                    if occurrences == 1 && !next_is_reassign && Self::is_safe_to_inline(&expr) {
                        let inlined = Self::replace_ident(next, &temp_name, &expr);
                        result.push(inlined);
                        skip_next = true;
                        changed = true;
                        continue;
                    }
                }
            }

            result.push(lines[i].to_string());
        }

        self.output = result.join("\n");
        if !self.output.is_empty() {
            self.output.push('\n');
        }
        changed
    }

    /// Parse `_N = expr;` or `name = expr;` for MIR temporaries.
    /// Returns (temp_name, expr) if the line is a simple assignment to a temp.
    fn parse_temp_assign(line: &str) -> Option<(String, String)> {
        // Must start with _ and a digit (MIR temporary like _1, _23)
        let parts: Vec<&str> = line.splitn(2, " = ").collect();
        if parts.len() != 2 {
            return None;
        }
        let name = parts[0].trim();
        let expr = parts[1].trim().strip_suffix(';')?.trim();

        // Only inline MIR temporaries (_N), not user-named variables
        if !name.starts_with('_') || name.len() < 2 {
            return None;
        }
        if !name[1..]
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            return None;
        }

        Some((name.to_string(), expr.to_string()))
    }

    /// Count occurrences of an identifier in a line (whole-word match).
    fn count_ident_occurrences(line: &str, ident: &str) -> usize {
        let mut count = 0;
        let ident_bytes = ident.as_bytes();
        let line_bytes = line.as_bytes();
        let ilen = ident_bytes.len();

        for pos in 0..line_bytes.len() {
            if pos + ilen > line_bytes.len() {
                break;
            }
            if &line_bytes[pos..pos + ilen] == ident_bytes {
                // Check word boundary before
                let before_ok = pos == 0
                    || !line_bytes[pos - 1].is_ascii_alphanumeric() && line_bytes[pos - 1] != b'_';
                // Check word boundary after
                let after_ok = pos + ilen >= line_bytes.len()
                    || !line_bytes[pos + ilen].is_ascii_alphanumeric()
                        && line_bytes[pos + ilen] != b'_';
                if before_ok && after_ok {
                    count += 1;
                }
            }
        }
        count
    }

    /// Check if an expression is safe to inline. Only inline trivial values:
    /// variables, constants, string literals, and field access. NOT expressions
    /// with operators or function calls.
    fn is_safe_to_inline(expr: &str) -> bool {
        // Only inline: identifiers, __strN, numeric literals, field access (x.y)
        // Reject: function calls, operators, casts, complex expressions
        if expr.contains('(')
            || expr.contains('+')
            || expr.contains('-')
            || expr.contains('*')
            || expr.contains('/')
            || expr.contains('?')
            || expr.contains('{')
            || expr.contains('[')
        {
            return false;
        }
        true
    }

    /// Replace an identifier with an expression, respecting word boundaries.
    fn replace_ident(line: &str, ident: &str, replacement: &str) -> String {
        let mut result = String::with_capacity(line.len() + replacement.len());
        let bytes = line.as_bytes();
        let ident_bytes = ident.as_bytes();
        let ilen = ident_bytes.len();
        let mut pos = 0;

        while pos < bytes.len() {
            if pos + ilen <= bytes.len() && &bytes[pos..pos + ilen] == ident_bytes {
                let before_ok =
                    pos == 0 || (!bytes[pos - 1].is_ascii_alphanumeric() && bytes[pos - 1] != b'_');
                let after_ok = pos + ilen >= bytes.len()
                    || (!bytes[pos + ilen].is_ascii_alphanumeric() && bytes[pos + ilen] != b'_');
                if before_ok && after_ok {
                    result.push_str(replacement);
                    pos += ilen;
                    continue;
                }
            }
            result.push(bytes[pos] as char);
            pos += 1;
        }
        result
    }

    /// Generate a C header (`.h`) declaring this module's `extern "C"` exports
    /// so C, and any C-ABI language, can call them. Emits an include guard, the
    /// integer/bool/size typedefs the prototypes use, and a C++ linkage guard.
    /// Functions are de-duplicated by name (lowering registers both a forward
    /// declaration and a definition for each function).
    pub fn generate_c_header(&self, module: &MirModule) -> String {
        let mut out = String::new();
        out.push_str("// Generated by BuildLang Compiler - C export header\n");
        out.push_str("// Do not edit manually\n");
        out.push_str("#pragma once\n\n");
        out.push_str("#include <stdint.h>\n");
        out.push_str("#include <stdbool.h>\n");
        out.push_str("#include <stddef.h>\n\n");
        out.push_str("#ifdef __cplusplus\nextern \"C\" {\n#endif\n\n");

        let mut seen: Vec<&str> = Vec::new();
        for func in &module.functions {
            if func.is_c_export && !seen.contains(&func.name.as_ref()) {
                seen.push(func.name.as_ref());
                out.push_str(&self.c_callable_prototype(func));
                out.push_str(";\n");
            }
        }

        out.push_str("\n#ifdef __cplusplus\n}\n#endif\n");
        out
    }

    /// Render a C prototype string `ret name(params)` for a function, with no
    /// linkage qualifier and no trailing `;`. Mirrors the definition signature
    /// so the header and the emitted `.c` agree on the symbol.
    fn c_callable_prototype(&self, func: &MirFunction) -> String {
        let mut s = String::new();
        let array_ret_out = self.array_ret_out_param(&func.sig.ret);
        let ret = if array_ret_out.is_some() {
            "void".to_string()
        } else {
            self.type_to_c(&func.sig.ret)
        };
        let name = Self::user_fn_emit_name(func.name.as_ref());
        s.push_str(&format!("{} {}(", ret, name));

        let params: Vec<_> = func.locals.iter().filter(|l| l.is_param).collect();
        if params.is_empty() && array_ret_out.is_none() {
            s.push_str("void");
        } else {
            for (i, param) in params.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                let pname = param
                    .name
                    .as_ref()
                    .map(|n| {
                        let raw = n.to_string();
                        if Self::is_c_reserved(&raw) {
                            format!("_{}", raw)
                        } else {
                            raw
                        }
                    })
                    .unwrap_or_else(|| format!("arg{}", i));
                if matches!(param.ty, MirType::Array(_, _)) {
                    s.push_str(&self.fmt_array_decl(&param.ty, &pname));
                } else if let MirType::FnPtr(ref fsig) = param.ty {
                    let r = self.type_to_c(&fsig.ret);
                    let fp: Vec<_> = fsig.params.iter().map(|p| self.type_to_c(p)).collect();
                    s.push_str(&format!("{} (*{})({})", r, pname, fp.join(", ")));
                } else {
                    s.push_str(&format!("{} {}", self.type_to_c(&param.ty), pname));
                }
            }
            if func.sig.is_variadic {
                s.push_str(", ...");
            }
            if let Some(ref out_param) = array_ret_out {
                if !params.is_empty() {
                    s.push_str(", ");
                }
                s.push_str(out_param);
            }
        }
        s.push(')');
        s
    }

    fn generate_function_signature(&mut self, func: &MirFunction) -> CodegenResult<()> {
        // A fixed-size `Array<T,N>` return is lowered to a `void` function with a
        // trailing pointer-to-array out-parameter: C cannot return or assign an
        // array by value. The result is `memcpy`d into `*__buildret_out`.
        let array_ret_out = self.array_ret_out_param(&func.sig.ret);
        let ret_type = if array_ret_out.is_some() {
            "void".to_string()
        } else {
            self.type_to_c(&func.sig.ret)
        };

        // Linkage
        match func.linkage {
            Linkage::Internal => self.output.push_str("static "),
            Linkage::External => {}
            Linkage::Weak => self.output.push_str("__attribute__((weak)) "),
            Linkage::LinkOnce => self.output.push_str("static inline "),
        }

        // Escape user-defined function names that conflict with C macros/stdlib.
        let func_name = Self::user_fn_emit_name(func.name.as_ref());
        write!(self.output, "{} {}(", ret_type, func_name).unwrap();

        // For main(), always emit (int argc, char** argv) signature
        if func.name.as_ref() == "main" {
            self.output.push_str("int argc, char** argv)");
            return Ok(());
        }

        // Parameters
        let params: Vec<_> = func.locals.iter().filter(|l| l.is_param).collect();

        if params.is_empty() && array_ret_out.is_none() {
            self.output.push_str("void");
        } else {
            for (i, param) in params.iter().enumerate() {
                if i > 0 {
                    self.output.push_str(", ");
                }
                let name = param
                    .name
                    .as_ref()
                    .map(|n| {
                        let s = n.to_string();
                        if Self::is_c_reserved(&s) {
                            format!("_{}", s)
                        } else {
                            s
                        }
                    })
                    .unwrap_or_else(|| format!("arg{}", i));
                // Arrays need special C parameter syntax: `type name[size]`
                // Function pointers need: `ret (*name)(params)`
                if matches!(param.ty, MirType::Array(_, _)) {
                    write!(self.output, "{}", self.fmt_array_decl(&param.ty, &name)).unwrap();
                } else if let MirType::FnPtr(ref sig) = param.ty {
                    let ret = self.type_to_c(&sig.ret);
                    let fn_params: Vec<_> = sig.params.iter().map(|p| self.type_to_c(p)).collect();
                    write!(self.output, "{} (*{})({})", ret, name, fn_params.join(", ")).unwrap();
                } else {
                    write!(self.output, "{} {}", self.type_to_c(&param.ty), name).unwrap();
                }
            }
            if func.sig.is_variadic {
                self.output.push_str(", ...");
            }
            // Trailing out-parameter for an array return type.
            if let Some(ref out_param) = array_ret_out {
                if !params.is_empty() {
                    self.output.push_str(", ");
                }
                self.output.push_str(out_param);
            }
        }

        self.output.push(')');

        Ok(())
    }

    fn generate_statement(&mut self, stmt: &MirStmt, locals: &[MirLocal]) -> CodegenResult<()> {
        match &stmt.kind {
            MirStmtKind::Assign { dest, value } => {
                self.record_string_literal_assignment(*dest, value);
                // Skip assignments to void-typed locals (these come from
                // statement-level if/else that don't produce values).
                if let Some(local) = locals.get(dest.0 as usize) {
                    if matches!(local.ty, MirType::Void) {
                        return Ok(());
                    }
                }
                let dest_name = self.local_name(*dest, locals);
                // Check if dest is an array type and value is an aggregate
                let is_array_dest = locals
                    .get(dest.0 as usize)
                    .map(|l| matches!(l.ty, MirType::Array(_, _)))
                    .unwrap_or(false);

                if is_array_dest {
                    if let MirRValue::Aggregate { operands, .. } = value {
                        // Emit element-by-element assignment for array initialization.
                        // If elements are arrays themselves (nested arrays like [[f64; 4]; 4]),
                        // use memcpy since C doesn't allow direct array assignment.
                        for (i, op) in operands.iter().enumerate() {
                            self.write_indent();
                            let val = self.value_to_c(op, locals);
                            // Check if the operand is a local with array type
                            let is_array_elem = if let MirValue::Local(local_id) = op {
                                locals
                                    .get(local_id.0 as usize)
                                    .map(|l| matches!(l.ty, MirType::Array(_, _)))
                                    .unwrap_or(false)
                            } else {
                                false
                            };
                            if is_array_elem {
                                write!(
                                    self.output,
                                    "memcpy({}[{}], {}, sizeof({}));\n",
                                    dest_name, i, val, val
                                )
                                .unwrap();
                            } else {
                                write!(self.output, "{}[{}] = {};\n", dest_name, i, val).unwrap();
                            }
                        }
                    } else if let MirRValue::Use(src_val) = value {
                        // Array-to-array copy: use memcpy since C does not
                        // allow direct array assignment.
                        self.write_indent();
                        let src = self.value_to_c(src_val, locals);
                        write!(
                            self.output,
                            "memcpy({}, {}, sizeof({}));\n",
                            dest_name, src, dest_name
                        )
                        .unwrap();
                    } else {
                        self.write_indent();
                        let rvalue = self.rvalue_to_c(value, locals)?;
                        write!(
                            self.output,
                            "memcpy({}, &({}), sizeof({}));\n",
                            dest_name, rvalue, dest_name
                        )
                        .unwrap();
                    }
                } else if let MirRValue::FieldAccess {
                    base, field_name, ..
                } = value
                {
                    // Special case: loading from handler_data needs cast+deref.
                    // handler_data holds a `void*[N]` argv array; slot `i` points
                    // to the perform argument for parameter `i`. The field marker
                    // is `handler_data#<i>` (plain `handler_data` means index 0).
                    let hd_index = if field_name.as_ref() == "handler_data" {
                        Some(0usize)
                    } else {
                        field_name
                            .as_ref()
                            .strip_prefix("handler_data#")
                            .and_then(|s| s.parse::<usize>().ok())
                    };
                    if let Some(idx) = hd_index {
                        let base_str = self.value_to_c(base, locals);
                        let dest_type = locals
                            .get(dest.0 as usize)
                            .map(|l| self.type_to_c(&l.ty))
                            .unwrap_or_else(|| "int32_t".to_string());
                        self.write_indent();
                        write!(
                            self.output,
                            "{} = *({}*)((void**){}.handler_data)[{}];\n",
                            dest_name, dest_type, base_str, idx
                        )
                        .unwrap();
                    } else {
                        self.write_indent();
                        let rvalue = self.rvalue_to_c(value, locals)?;
                        write!(self.output, "{} = {};\n", dest_name, rvalue).unwrap();
                    }
                } else if let MirRValue::Aggregate {
                    kind: AggregateKind::Struct(_),
                    operands,
                } = value
                {
                    // Check if any operand is an array - if so, use memcpy
                    // because C compound literals can't initialize array fields
                    // from array variables.
                    let has_array_operand = operands.iter().any(|op| {
                        if let MirValue::Local(id) = op {
                            locals
                                .get(id.0 as usize)
                                .map(|l| matches!(l.ty, MirType::Array(_, _)))
                                .unwrap_or(false)
                        } else {
                            false
                        }
                    });
                    if has_array_operand && operands.len() == 1 {
                        // Single array field → memcpy the array into the struct
                        let src = self.value_to_c(&operands[0], locals);
                        self.write_indent();
                        write!(
                            self.output,
                            "memcpy(&{}, &{}, sizeof({}));\n",
                            dest_name, src, dest_name
                        )
                        .unwrap();
                    } else {
                        self.write_indent();
                        let rvalue = self.rvalue_to_c(value, locals)?;
                        write!(self.output, "{} = {};\n", dest_name, rvalue).unwrap();
                    }
                } else if let MirRValue::Use(src_val) = value {
                    // Detect type mismatch between dest and src for Use assignments.
                    // When one is a struct and the other is a primitive (or different
                    // struct), use memcpy to avoid C type errors.
                    let dest_ty = locals.get(dest.0 as usize).map(|l| &l.ty);
                    let src_ty = match src_val {
                        MirValue::Local(id) => locals.get(id.0 as usize).map(|l| &l.ty),
                        _ => None,
                    };
                    let needs_cast = if let (Some(dt), Some(st)) = (dest_ty, src_ty) {
                        let dt_is_struct = matches!(
                            dt,
                            MirType::Struct(_)
                                | MirType::Vec(_)
                                | MirType::Map(_, _)
                                | MirType::Tuple(_)
                        );
                        let st_is_struct = matches!(
                            st,
                            MirType::Struct(_)
                                | MirType::Vec(_)
                                | MirType::Map(_, _)
                                | MirType::Tuple(_)
                        );
                        (dt_is_struct || st_is_struct) && dt != st
                    } else {
                        false
                    };
                    if needs_cast {
                        let src_str = self.value_to_c(src_val, locals);
                        self.write_indent();
                        write!(
                            self.output,
                            "memcpy(&{}, &{}, sizeof({}) < sizeof({}) ? sizeof({}) : sizeof({}));\n",
                            dest_name, src_str, dest_name, src_str, dest_name, src_str
                        ).unwrap();
                    } else {
                        self.emit_typed_assign(dest_name.clone(), value, *dest, locals)?;
                    }
                } else {
                    self.emit_typed_assign(dest_name.clone(), value, *dest, locals)?;
                }

                // Increment 5: def-site flag set, move-acquire case.
                // `sound_owned_candidates`'s def_count == 1 guarantee makes
                // any Assign to a flag owner its unique def; the candidate
                // gates only admit allocating-Call and `Use(Local)` (move-
                // acquire) defs, so an Assign reaching here for a flag owner
                // is always the move-acquire of a fresh buffer whose source
                // is moved-from and therefore excluded from every free set.
                if self.current_fn_flag_owners.contains(dest) {
                    self.write_indent();
                    writeln!(self.output, "__bl_live_{} = 1;", dest.0).unwrap();
                }
            }
            MirStmtKind::DerefAssign { ptr, value } => {
                let ptr_name = self.local_name(*ptr, locals);
                self.write_indent();
                let rvalue = self.rvalue_to_c(value, locals)?;
                write!(self.output, "*{} = {};\n", ptr_name, rvalue).unwrap();
            }
            MirStmtKind::FieldDerefAssign {
                ptr,
                field_name,
                value,
            } => {
                let ptr_name = self.local_name(*ptr, locals);
                self.write_indent();
                let rvalue = self.rvalue_to_c(value, locals)?;
                let field = Self::escape_c_keyword(field_name);
                write!(self.output, "{}->{} = {};\n", ptr_name, field, rvalue).unwrap();
            }
            MirStmtKind::FieldAssign {
                base,
                field_name,
                value,
            } => {
                let base_name = self.local_name(*base, locals);
                self.write_indent();
                let rvalue = self.rvalue_to_c(value, locals)?;
                let field = Self::escape_c_keyword(field_name);
                write!(self.output, "{}.{} = {};\n", base_name, field, rvalue).unwrap();
            }
            MirStmtKind::GlobalStore { name, value } => {
                // `GLOBAL = value;` - the C global identifier matches the
                // definition and the read path (value_to_c on MirValue::Global).
                self.write_indent();
                let rvalue = self.rvalue_to_c(value, locals)?;
                write!(self.output, "{} = {};\n", name, rvalue).unwrap();
            }
            MirStmtKind::IndexStore {
                base,
                index,
                elem_ty,
                value,
            } => {
                let base_str = self.value_to_c(base, locals);
                let index_str = self.value_to_c(index, locals);
                self.write_indent();
                let rvalue = self.rvalue_to_c(value, locals)?;
                // Heap Vec elements go through the typed setter; a slice/array/
                // pointer base (the GPU compute case) is a plain C subscript.
                let base_is_vec = match base {
                    MirValue::Local(id) => locals
                        .get(id.0 as usize)
                        .map(|l| matches!(l.ty, MirType::Vec(_)))
                        .unwrap_or(false),
                    _ => false,
                };
                if base_is_vec {
                    let suffix = match elem_ty {
                        MirType::Float(_) => "f64",
                        MirType::Int(IntSize::I64, _) => "i64",
                        MirType::Struct(n) if n.as_ref() == "BuildString" => "str",
                        _ => "i32",
                    };
                    write!(
                        self.output,
                        "build_hvec_set_{}({}, {}, {});\n",
                        suffix, base_str, index_str, rvalue
                    )
                    .unwrap();
                } else if let Some(target) =
                    Self::slice_element_access(base, &base_str, &index_str, locals)
                {
                    // Slice / pointer-to-slice store: `base->ptr[i] = value`.
                    write!(self.output, "{} = {};\n", target, rvalue).unwrap();
                } else {
                    write!(self.output, "{}[{}] = {};\n", base_str, index_str, rvalue).unwrap();
                }
            }
            MirStmtKind::StorageLive(_) | MirStmtKind::StorageDead(_) => {
                // No-op in C
            }
            MirStmtKind::WorkgroupBarrier => {
                // The C path is the single-threaded scalar cross-check; a
                // workgroup barrier synchronizes GPU invocations and has no
                // meaning there, so it emits nothing.
            }
            MirStmtKind::Nop => {
                self.writeln(";");
            }
        }

        Ok(())
    }

    /// Resolve a block ID to its goto label. If the block has a named label,
    /// use that; otherwise fall back to `bb{id}`.
    fn block_label(&self, id: &BlockId, blocks: &[MirBlock]) -> String {
        blocks
            .iter()
            .find(|b| b.id == *id)
            .and_then(|b| b.label.as_ref())
            .map(|l| l.to_string())
            .unwrap_or_else(|| format!("bb{}", id.0))
    }

    fn generate_terminator(
        &mut self,
        term: &MirTerminator,
        locals: &[MirLocal],
        blocks: &[MirBlock],
    ) -> CodegenResult<()> {
        match term {
            MirTerminator::Goto(target) => {
                self.write_indent();
                write!(self.output, "goto {};\n", self.block_label(target, blocks)).unwrap();
            }
            MirTerminator::If {
                cond,
                then_block,
                else_block,
            } => {
                self.write_indent();
                let cond_str = self.value_to_c(cond, locals);
                write!(
                    self.output,
                    "if ({}) goto {}; else goto {};\n",
                    cond_str,
                    self.block_label(then_block, blocks),
                    self.block_label(else_block, blocks)
                )
                .unwrap();
            }
            MirTerminator::Switch {
                value,
                targets,
                default,
            } => {
                self.write_indent();
                let val_str = self.value_to_c(value, locals);
                write!(self.output, "switch ({}) {{\n", val_str).unwrap();
                self.indent += 1;
                for (const_val, target) in targets {
                    self.write_indent();
                    write!(
                        self.output,
                        "case {}: goto {};\n",
                        self.const_to_c(const_val),
                        self.block_label(target, blocks)
                    )
                    .unwrap();
                }
                self.write_indent();
                write!(
                    self.output,
                    "default: goto {};\n",
                    self.block_label(default, blocks)
                )
                .unwrap();
                self.indent -= 1;
                self.writeln("}");
            }
            MirTerminator::Call {
                func,
                args,
                dest,
                target,
                ..
            } => {
                self.write_indent();
                let func_str = self.value_to_c(func, locals);
                if let Some(dest_local) = dest {
                    if func_str == "build_string_new" {
                        if let Some(MirValue::Const(MirConst::Str(idx))) = args.first() {
                            self.local_string_literals.insert(*dest_local, *idx);
                        } else {
                            self.local_string_literals.remove(dest_local);
                        }
                    } else {
                        self.local_string_literals.remove(dest_local);
                    }
                }

                // Handle vtable dispatch: __vtable_dispatch_TraitName_methodName_idx
                // args[0] = data pointer (void*), args[1..] = method arguments
                // The vtable pointer was stored in a local by the lowerer
                if func_str.starts_with("__vtable_dispatch_") {
                    let suffix = func_str.strip_prefix("__vtable_dispatch_").unwrap();
                    let parts: Vec<&str> = suffix.rsplitn(2, '_').collect();
                    if parts.len() == 2 {
                        let _method_idx: usize = parts[0].parse().unwrap_or(0);
                        let trait_and_method = parts[1];
                        let method_name = trait_and_method
                            .rsplit('_')
                            .next()
                            .unwrap_or(trait_and_method);

                        let args_str: Vec<_> =
                            args.iter().map(|a| self.value_to_c(a, locals)).collect();

                        // Find the vtable local - it's the local right after the data pointer
                        // that was assigned from a FieldAccess with field_name "vtable"
                        // For now, look for a local with void* type near the data pointer
                        let vtable_local_name = if args.len() > 0 {
                            if let MirValue::Local(data_id) = &args[0] {
                                // Vtable local is typically data_id + 1 (allocated together)
                                let vtable_id = LocalId(data_id.0 + 1);
                                self.local_name(vtable_id, locals)
                            } else {
                                "/* vtable */".to_string()
                            }
                        } else {
                            "/* vtable */".to_string()
                        };

                        // Generate: dest = ((TraitName_vtable*)vtable_ptr)->method(data_ptr, args...)
                        if let Some(dest_local) = dest {
                            let dest_name = self.local_name(*dest_local, locals);
                            write!(
                                self.output,
                                "{} = ((void*){} != NULL) ? ",
                                dest_name, vtable_local_name
                            )
                            .unwrap();
                            // Cast vtable to correct type and call through function pointer
                            let trait_name =
                                trait_and_method.rsplitn(2, '_').last().unwrap_or("Unknown");
                            write!(
                                self.output,
                                "(({}_vtable*){})->{}({}) : 0;\n",
                                trait_name,
                                vtable_local_name,
                                method_name,
                                args_str.join(", ")
                            )
                            .unwrap();
                        }

                        if let Some(target) = target {
                            self.write_indent();
                            write!(self.output, "goto bb{};\n", target.0).unwrap();
                        }

                        return Ok(());
                    }
                }

                // Map intrinsic_* calls to C standard library functions.
                let func_str = match func_str.as_str() {
                    "intrinsic_trunc" => "trunc".to_string(),
                    "intrinsic_exp2" => "exp2".to_string(),
                    "intrinsic_asin" => "asin".to_string(),
                    "intrinsic_acos" => "acos".to_string(),
                    "intrinsic_atan" => "atan".to_string(),
                    "intrinsic_atan2" => "atan2".to_string(),
                    "intrinsic_sinh" => "sinh".to_string(),
                    "intrinsic_cosh" => "cosh".to_string(),
                    "intrinsic_tanh" => "tanh".to_string(),
                    "intrinsic_cbrt" => "cbrt".to_string(),
                    "intrinsic_log" => "log".to_string(),
                    "intrinsic_log2" => "log2".to_string(),
                    "intrinsic_log10" => "log10".to_string(),
                    "intrinsic_fabs" => "fabs".to_string(),
                    "intrinsic_sqrt" => "sqrt".to_string(),
                    "intrinsic_ceil" => "ceil".to_string(),
                    "intrinsic_floor" => "floor".to_string(),
                    "intrinsic_round" => "round".to_string(),
                    "intrinsic_pow" => "pow".to_string(),
                    "intrinsic_fmin" => "fmin".to_string(),
                    "intrinsic_fmax" => "fmax".to_string(),
                    "intrinsic_copysign" => "copysign".to_string(),
                    "intrinsic_hypot" => "hypot".to_string(),
                    // Constructor calls → runtime functions
                    "HashMap_new" => "build_hmap_new_str_f64".to_string(),
                    "HashSet_new" => "build_hset_new".to_string(),
                    "VecDeque_new" => "build_vdeque_new".to_string(),
                    // println/print without ! - keep name, handle below
                    _ => func_str,
                };

                // Future: type-dispatch for map builtins (i32 vs str→f64) based on
                // argument types. Currently map_get/map_insert default to str→f64.

                // println/print (without !) → printf with BuildString handling
                if matches!(
                    func_str.as_str(),
                    "println" | "print" | "eprintln" | "eprint"
                ) {
                    self.emit_print_call(func_str.as_str(), args, *target, blocks, locals)?;
                    return Ok(());
                }

                // Vec::new() → element-typed runtime constructor. The element
                // type lives on the dest local's MIR `Vec<Elem>` type (the C type
                // name erases it to BuildVecHandle, but MIR preserves it). Without
                // this, Vec::new lowered to an undefined `Vec_new` symbol.
                if func_str == "Vec_new" && args.is_empty() {
                    if let Some(dest_local) = dest {
                        let suffix = locals
                            .get(dest_local.0 as usize)
                            .and_then(|l| match &l.ty {
                                MirType::Vec(elem) => Some(Self::hvec_elem_suffix(elem)),
                                _ => None,
                            })
                            .unwrap_or_else(|| "i32".to_string());
                        let dest_name = self.local_name(*dest_local, locals);
                        write!(
                            self.output,
                            "{} = build_hvec_new_{}();\n",
                            dest_name, suffix
                        )
                        .unwrap();
                    }
                    if let Some(target) = target {
                        self.write_indent();
                        write!(self.output, "goto bb{};\n", target.0).unwrap();
                    }
                    return Ok(());
                }

                // String::new() → build_string_new("")
                if func_str == "String_new" && args.is_empty() {
                    if let Some(dest_local) = dest {
                        let dest_name = self.local_name(*dest_local, locals);
                        write!(self.output, "{} = build_string_new(\"\");\n", dest_name).unwrap();
                    }
                    if let Some(target) = target {
                        self.write_indent();
                        write!(self.output, "goto bb{};\n", target.0).unwrap();
                    }
                    return Ok(());
                }

                // String::from(s) → build_string_new(<s as const char*>): copy the
                // &str into a freshly allocated owned String. A &str literal/value
                // is a BuildString here, whose char buffer is its `.ptr` field;
                // anything else is already a `const char*`. (Without this,
                // String::from lowered to an undefined `String_from` symbol.)
                if func_str == "String_from" && args.len() == 1 {
                    if let Some(dest_local) = dest {
                        let dest_name = self.local_name(*dest_local, locals);
                        let arg = self.value_to_c(&args[0], locals);
                        let arg_is_string = matches!(&args[0],
                            MirValue::Local(l) if locals.iter().any(|loc| loc.id == *l
                                && matches!(loc.ty, MirType::Struct(ref n) if n.as_ref() == "BuildString")));
                        let cptr = if arg_is_string {
                            format!("{}.ptr", arg)
                        } else {
                            arg
                        };
                        write!(self.output, "{} = build_string_new({});\n", dest_name, cptr)
                            .unwrap();
                    }
                    if let Some(target) = target {
                        self.write_indent();
                        write!(self.output, "goto bb{};\n", target.0).unwrap();
                    }
                    return Ok(());
                }

                // clone(x) → direct copy (x is value-typed in C)
                if func_str == "clone" && args.len() == 1 {
                    let arg_str = self.value_to_c(&args[0], locals);
                    if let Some(dest_local) = dest {
                        let dest_name = self.local_name(*dest_local, locals);
                        write!(self.output, "{} = {};\n", dest_name, arg_str).unwrap();
                    }
                    if let Some(target) = target {
                        self.write_indent();
                        write!(self.output, "goto bb{};\n", target.0).unwrap();
                    }
                    return Ok(());
                }

                // Runtime None/Some: emit zero-initialized Option struct.
                if func_str == "None" && args.is_empty() {
                    if let Some(dest_local) = dest {
                        let dest_name = self.local_name(*dest_local, locals);
                        write!(self.output, "{}.has_value = false;\n", dest_name,).unwrap();
                    }
                    if let Some(target) = target {
                        self.write_indent();
                        write!(self.output, "goto bb{};\n", target.0).unwrap();
                    }
                    return Ok(());
                }

                // Runtime Some(x): construct an Option with the payload in the
                // typed 8-byte union slot (`.value.i` integer/bool, `.value.f`
                // float, `.value.p` pointer). Previously Some lowered to an
                // undefined `Some(x)` call into an i32-typed dest (a C2440).
                if func_str == "Some" && args.len() == 1 {
                    if let Some(dest_local) = dest {
                        let dest_name = self.local_name(*dest_local, locals);
                        let arg = self.value_to_c(&args[0], locals);
                        let arg_ty = self.sumtype_arg_type(&args[0], locals);
                        let boxed = arg_ty
                            .as_ref()
                            .map(Self::payload_needs_boxing)
                            .unwrap_or(false);
                        write!(self.output, "{}.has_value = true;\n", dest_name).unwrap();
                        self.write_indent();
                        if boxed {
                            // Payload >8 bytes: box it (malloc + copy) and store
                            // the pointer in the .p slot.
                            let ct = self.type_to_c(arg_ty.as_ref().unwrap());
                            write!(
                                self.output,
                                "{{ {ct}* __opt_box = ({ct}*)malloc(sizeof({ct})); \
                                 *__opt_box = {arg}; {dest}.value.p = (void*)__opt_box; }}\n",
                                ct = ct,
                                arg = arg,
                                dest = dest_name
                            )
                            .unwrap();
                        } else {
                            let (slot, cast) = match &args[0] {
                                MirValue::Local(id) => {
                                    match locals.get(id.0 as usize).map(|l| &l.ty) {
                                        Some(MirType::Float(_)) => ("f", "(double)"),
                                        Some(MirType::Ptr(_)) => ("p", "(void*)"),
                                        _ => ("i", "(int64_t)"),
                                    }
                                }
                                MirValue::Const(MirConst::Float(..)) => ("f", "(double)"),
                                _ => ("i", "(int64_t)"),
                            };
                            write!(
                                self.output,
                                "{}.value.{} = {}({});\n",
                                dest_name, slot, cast, arg
                            )
                            .unwrap();
                        }
                    }
                    if let Some(target) = target {
                        self.write_indent();
                        write!(self.output, "goto bb{};\n", target.0).unwrap();
                    }
                    return Ok(());
                }

                // Runtime Ok(x): construct a Result with is_ok=true and the
                // payload in the typed 8-byte ok union slot (`.ok.ok_i`/`.ok_f`/
                // `.ok_p`). Mirrors the Some(x) Option construction. Previously
                // Ok lowered to an undefined `Ok(x)` call into an i32 dest.
                if func_str == "Ok" && args.len() == 1 {
                    if let Some(dest_local) = dest {
                        let dest_name = self.local_name(*dest_local, locals);
                        let arg = self.value_to_c(&args[0], locals);
                        let arg_ty = self.sumtype_arg_type(&args[0], locals);
                        let boxed = arg_ty
                            .as_ref()
                            .map(Self::payload_needs_boxing)
                            .unwrap_or(false);
                        write!(self.output, "{}.is_ok = true;\n", dest_name).unwrap();
                        self.write_indent();
                        if boxed {
                            // Payload >8 bytes: box it and store the pointer in
                            // the ok_p slot.
                            let ct = self.type_to_c(arg_ty.as_ref().unwrap());
                            write!(
                                self.output,
                                "{{ {ct}* __ok_box = ({ct}*)malloc(sizeof({ct})); \
                                 *__ok_box = {arg}; {dest}.ok.ok_p = (void*)__ok_box; }}\n",
                                ct = ct,
                                arg = arg,
                                dest = dest_name
                            )
                            .unwrap();
                        } else {
                            let (slot, cast) = match &args[0] {
                                MirValue::Local(id) => {
                                    match locals.get(id.0 as usize).map(|l| &l.ty) {
                                        Some(MirType::Float(_)) => ("ok_f", "(double)"),
                                        Some(MirType::Ptr(_)) => ("ok_p", "(void*)"),
                                        _ => ("ok_i", "(int64_t)"),
                                    }
                                }
                                MirValue::Const(MirConst::Float(..)) => ("ok_f", "(double)"),
                                _ => ("ok_i", "(int64_t)"),
                            };
                            write!(
                                self.output,
                                "{}.ok.{} = {}({});\n",
                                dest_name, slot, cast, arg
                            )
                            .unwrap();
                        }
                    }
                    if let Some(target) = target {
                        self.write_indent();
                        write!(self.output, "goto bb{};\n", target.0).unwrap();
                    }
                    return Ok(());
                }

                // Runtime Err(e): construct a Result with is_ok=false and the
                // error message in the `err` BuildString field. The common Err
                // payload is a String/BuildString; a bare string literal is
                // wrapped into a BuildString. Previously Err lowered to an
                // undefined `Err(e)` call into an i32 dest.
                if func_str == "Err" && args.len() == 1 {
                    if let Some(dest_local) = dest {
                        let dest_name = self.local_name(*dest_local, locals);
                        let arg = self.value_to_c(&args[0], locals);
                        write!(self.output, "{}.is_ok = false;\n", dest_name).unwrap();
                        self.write_indent();
                        // A raw string literal is a `const char*`: wrap it into a
                        // BuildString and box it into the err_p slot.
                        if let MirValue::Const(MirConst::Str(_)) = &args[0] {
                            write!(
                                self.output,
                                "{{ BuildString* __err_box = (BuildString*)malloc(sizeof(BuildString)); \
                                 *__err_box = build_string_new({arg}); {dest}.err.err_p = (void*)__err_box; }}\n",
                                arg = arg,
                                dest = dest_name
                            )
                            .unwrap();
                        } else {
                            let arg_ty = self.sumtype_arg_type(&args[0], locals);
                            let boxed = arg_ty
                                .as_ref()
                                .map(Self::payload_needs_boxing)
                                .unwrap_or(false);
                            if boxed {
                                // Err payload >8 bytes (e.g. String): box it.
                                let ct = self.type_to_c(arg_ty.as_ref().unwrap());
                                write!(
                                    self.output,
                                    "{{ {ct}* __err_box = ({ct}*)malloc(sizeof({ct})); \
                                     *__err_box = {arg}; {dest}.err.err_p = (void*)__err_box; }}\n",
                                    ct = ct,
                                    arg = arg,
                                    dest = dest_name
                                )
                                .unwrap();
                            } else {
                                let (slot, cast) = match &args[0] {
                                    MirValue::Local(id) => {
                                        match locals.get(id.0 as usize).map(|l| &l.ty) {
                                            Some(MirType::Float(_)) => ("err_f", "(double)"),
                                            Some(MirType::Ptr(_)) => ("err_p", "(void*)"),
                                            _ => ("err_i", "(int64_t)"),
                                        }
                                    }
                                    MirValue::Const(MirConst::Float(..)) => ("err_f", "(double)"),
                                    _ => ("err_i", "(int64_t)"),
                                };
                                write!(
                                    self.output,
                                    "{}.err.{} = {}({});\n",
                                    dest_name, slot, cast, arg
                                )
                                .unwrap();
                            }
                        }
                    }
                    if let Some(target) = target {
                        self.write_indent();
                        write!(self.output, "goto bb{};\n", target.0).unwrap();
                    }
                    return Ok(());
                }

                // Unit struct constructors: Stdin, Stdout, Stderr - emit zero-init.
                const UNIT_STRUCTS: &[(&str, &str)] = &[
                    ("Stdin", "io_Stdin"),
                    ("Stdout", "io_Stdout"),
                    ("Stderr", "io_Stderr"),
                ];
                if args.is_empty() {
                    if let Some((_, c_name)) = UNIT_STRUCTS.iter().find(|(ql, _)| func_str == *ql) {
                        if let Some(dest_local) = dest {
                            let dest_name = self.local_name(*dest_local, locals);
                            write!(self.output, "{} = ({}){{ 0 }};\n", dest_name, c_name,).unwrap();
                        }
                        if let Some(target) = target {
                            self.write_indent();
                            write!(self.output, "goto bb{};\n", target.0).unwrap();
                        }
                        return Ok(());
                    }
                }

                // Special-case setjmp: when passed a BuildHandler local,
                // emit `setjmp(handler.env)` instead of `setjmp(handler)`.
                //
                // Special-case printf: bool arguments used with %s must be
                // converted to "true"/"false" strings via a ternary.
                // Look up target function's parameter types for auto-ref.
                let target_params = self.fn_params.get(func_str.as_str()).cloned();
                let is_printf = func_str == "printf";
                let args_str: Vec<_> = args
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        let s = self.value_to_c(a, locals);
                        // Auto-coerce arguments based on parameter types.
                        if let Some(ref params) = target_params {
                            if let Some(param_ty) = params.get(i) {
                                let arg_is_string = if let MirValue::Local(id) = a {
                                    locals.get(id.0 as usize)
                                        .map(|l| matches!(l.ty, MirType::Struct(ref n) if n.as_ref() == "BuildString"))
                                        .unwrap_or(false)
                                } else { false };

                                // BuildString → &BuildString (auto-ref for &String params)
                                if let MirType::Ptr(ref inner) = param_ty {
                                    if let MirType::Struct(ref pname) = inner.as_ref() {
                                        if pname.as_ref() == "BuildString" && arg_is_string {
                                            return format!("&{}", s);
                                        }
                                    }
                                    // BuildString → const char* (extract .ptr)
                                    if let MirType::Int(IntSize::I8, _) = inner.as_ref() {
                                        if arg_is_string {
                                            return format!("{}.ptr", s);
                                        }
                                    }
                                }
                            }
                        }
                        if func_str == "setjmp" {
                            if let MirValue::Local(id) = a {
                                if let Some(local) = locals.get(id.0 as usize) {
                                    if let MirType::Struct(ref name) = local.ty {
                                        if name.as_ref() == "BuildHandler" {
                                            return format!("{}.env", s);
                                        }
                                    }
                                }
                            }
                        }
                        // For printf, convert bool args to "true"/"false" strings
                        if is_printf {
                            if let MirValue::Local(id) = a {
                                if let Some(local) = locals.get(id.0 as usize) {
                                    if matches!(local.ty, MirType::Bool) {
                                        return format!("{} ? \"true\" : \"false\"", s);
                                    }
                                }
                            }
                            if let MirValue::Const(MirConst::Bool(_)) = a {
                                return format!("{} ? \"true\" : \"false\"", s);
                            }
                        }
                        s
                    })
                    .collect();

                if let Some(dest_local) = dest {
                    // Skip assignment for void-returning functions.
                    // Check both the dest type and known void functions
                    // (assert returns void but MIR may type the dest as i32).
                    let dest_is_void = locals
                        .get(dest_local.0 as usize)
                        .map(|l| matches!(l.ty, MirType::Void))
                        .unwrap_or(false);
                    // Array return: the callee is lowered to a void function with
                    // a trailing pointer-to-array out-parameter. Pass `&dest` so
                    // the result is written directly into the caller's array
                    // local (C cannot assign a call result to an array variable).
                    let dest_is_array = locals
                        .get(dest_local.0 as usize)
                        .map(|l| matches!(l.ty, MirType::Array(_, _)))
                        .unwrap_or(false);
                    let fn_returns_void = matches!(
                        func_str.as_str(),
                        "assert" | "free" | "exit" | "abort" | "process_exit"
                    );
                    if dest_is_array && !fn_returns_void {
                        let dest_name = self.local_name(*dest_local, locals);
                        let mut all_args = args_str.clone();
                        all_args.push(format!("&{}", dest_name));
                        write!(self.output, "{}({});\n", func_str, all_args.join(", ")).unwrap();
                    } else if dest_is_void || fn_returns_void {
                        write!(self.output, "{}({});\n", func_str, args_str.join(", ")).unwrap();
                    } else {
                        let dest_name = self.local_name(*dest_local, locals);
                        write!(
                            self.output,
                            "{} = {}({});\n",
                            dest_name,
                            func_str,
                            args_str.join(", ")
                        )
                        .unwrap();
                    }
                } else {
                    write!(self.output, "{}({});\n", func_str, args_str.join(", ")).unwrap();
                }

                // Increment 5: def-site flag set for an allocating-Call dest.
                // `sound_owned_candidates`'s def_count == 1 guarantee makes
                // this Call the owner's unique def, so the set is
                // unconditional once dest is a flag owner. Read confirmation
                // (recorded in the commit body): every special-case early
                // exit above this generic tail (vtable dispatch, the
                // `intrinsic_*` remap, `Vec_new`/`String_new`/`String_from`/
                // `clone`/`None`/`Some`/`Ok`/`Err`/unit-struct constructors)
                // keys on a `func_str` that can never equal one of the closed
                // 6-name `allocates_owned_string` list (`build_string_concat`,
                // `build_format_str`, `build_format_i32`, `build_format_f64`,
                // `build_i32_to_string`, `build_f64_to_string`), so this
                // generic tail is the ONLY path where `dest = callee(...)`
                // for an allocating callee. A missed set fails SAFE (flag
                // stays 0, buffer leaks); a spurious set is impossible
                // because the emission is keyed on `dest == flag owner`,
                // which `sound_owned_candidates` only admits for a def_count
                // == 1 owner.
                if let Some(dest_local) = dest {
                    if self.current_fn_flag_owners.contains(dest_local) {
                        self.write_indent();
                        writeln!(self.output, "__bl_live_{} = 1;", dest_local.0).unwrap();
                    }
                }

                if let Some(target_block) = target {
                    self.write_indent();
                    write!(
                        self.output,
                        "goto {};\n",
                        self.block_label(target_block, blocks)
                    )
                    .unwrap();
                }
            }
            MirTerminator::Return(value) => {
                // Free owned heap locals proven safe to drop at function exit
                // (experimental, opt-in; the set is empty unless enabled).
                if !self.current_fn_freeable.is_empty() {
                    let names: Vec<String> = self
                        .current_fn_freeable
                        .iter()
                        .map(|id| self.local_name(*id, locals))
                        .collect();
                    for name in names {
                        self.write_indent();
                        writeln!(self.output, "build_string_free({});", name).unwrap();
                    }
                }
                // Increment 5: the Return backstop. Every flag-managed owner
                // gets a guarded free here too, alongside the unguarded
                // function-exit frees above (disjoint owner sets, so this
                // never re-frees an increment-1-4 owner). This reclaims paths
                // that bypass every frontier site (early return, break out of
                // a loop mid-iteration) and is sound unconditionally: nothing
                // executes after the block's statements at a Return
                // terminator, and non-escape (already gated by
                // `sound_owned_candidates`) means no pointer derived from the
                // owner survives the function.
                if !self.current_fn_flag_owners.is_empty() {
                    let owners = self.current_fn_flag_owners.clone();
                    for owner in owners {
                        self.emit_flag_guarded_free(owner, locals);
                    }
                }
                // Flush stdout before returning to ensure all output is visible,
                // especially when running as a child process on Windows.
                self.write_indent();
                self.output.push_str("fflush(stdout);\n");
                self.write_indent();
                // Array return: the caller passed a pointer-to-array out-param;
                // copy the result array into it and return `void`. C cannot
                // return an array by value.
                if let Some(val) = value {
                    if matches!(self.current_ret_ty, MirType::Array(_, _)) {
                        let val_str = self.value_to_c(val, locals);
                        write!(
                            self.output,
                            "memcpy({out}, {val}, sizeof(*{out}));\n",
                            out = Self::ARRAY_RET_OUT_PARAM,
                            val = val_str
                        )
                        .unwrap();
                        self.write_indent();
                        self.output.push_str("return;\n");
                        return Ok(());
                    }
                }
                if let Some(val) = value {
                    let val_str = self.value_to_c(val, locals);
                    // Check for type mismatch between return value and function
                    // return type.  Use memcpy cast for struct/primitive mismatches.
                    let val_ty = match val {
                        MirValue::Local(id) => locals.get(id.0 as usize).map(|l| &l.ty),
                        _ => None,
                    };
                    let ret_is_struct = matches!(
                        &self.current_ret_ty,
                        MirType::Struct(_)
                            | MirType::Vec(_)
                            | MirType::Map(_, _)
                            | MirType::Tuple(_)
                    );
                    let val_is_struct = val_ty
                        .map(|t| {
                            matches!(
                                t,
                                MirType::Struct(_)
                                    | MirType::Vec(_)
                                    | MirType::Map(_, _)
                                    | MirType::Tuple(_)
                            )
                        })
                        .unwrap_or(false);
                    let types_mismatch = val_ty.map(|t| t != &self.current_ret_ty).unwrap_or(false);
                    if types_mismatch
                        && (ret_is_struct || val_is_struct)
                        && self.current_ret_ty != MirType::Void
                    {
                        let ret_c = self.type_to_c(&self.current_ret_ty);
                        write!(self.output, "{{ {} _cast; memset(&_cast, 0, sizeof(_cast)); memcpy(&_cast, &{}, sizeof({}) < sizeof(_cast) ? sizeof({}) : sizeof(_cast)); return _cast; }}\n",
                            ret_c, val_str, val_str, val_str).unwrap();
                    } else {
                        write!(self.output, "return {};\n", val_str).unwrap();
                    }
                } else {
                    self.output.push_str("return;\n");
                }
            }
            MirTerminator::Unreachable => {
                self.writeln("__builtin_unreachable();");
            }
            MirTerminator::Abort => {
                self.writeln("abort();");
            }
            MirTerminator::Assert {
                cond,
                expected,
                msg,
                target,
                ..
            } => {
                self.write_indent();
                let cond_str = self.value_to_c(cond, locals);
                if *expected {
                    write!(self.output,
                        "if (!{}) {{ fprintf(stderr, \"Assertion failed: %s\\n\", \"{}\"); abort(); }}\n",
                        cond_str, msg
                    ).unwrap();
                } else {
                    write!(self.output,
                        "if ({}) {{ fprintf(stderr, \"Assertion failed: %s\\n\", \"{}\"); abort(); }}\n",
                        cond_str, msg
                    ).unwrap();
                }
                self.write_indent();
                write!(self.output, "goto {};\n", self.block_label(target, blocks)).unwrap();
            }
            MirTerminator::Drop {
                place: _, target, ..
            } => {
                // No explicit drop in C
                self.write_indent();
                write!(self.output, "goto {};\n", self.block_label(target, blocks)).unwrap();
            }
            MirTerminator::Resume => {
                self.writeln("// resume unwinding");
            }
        }

        Ok(())
    }

    // =========================================================================
    // TYPE AND VALUE CONVERSION
    // =========================================================================

    /// True when a sum-type payload of this type does not fit the 8-byte
    /// Option/Result union slot and must be boxed (malloc + store the pointer in
    /// the `.p` / `.ok_p` slot). Scalars and pointers fit inline; aggregates
    /// (BuildString and other structs, tuples, arrays, collection handles) are
    /// boxed. Boxing round-trips any value; it is correctness-safe even for an
    /// 8-byte handle.
    fn payload_needs_boxing(ty: &MirType) -> bool {
        !matches!(
            ty,
            MirType::Int(..)
                | MirType::Float(..)
                | MirType::Bool
                | MirType::Ptr(..)
                | MirType::FnPtr(..)
                | MirType::Void
                | MirType::Never
        )
    }

    fn type_to_c(&self, ty: &MirType) -> String {
        match ty {
            MirType::Void => "void".to_string(),
            MirType::Bool => "bool".to_string(),
            MirType::Int(size, signed) => {
                let prefix = if *signed { "" } else { "u" };
                match size {
                    IntSize::I8 => format!("{}int8_t", prefix),
                    IntSize::I16 => format!("{}int16_t", prefix),
                    IntSize::I32 => format!("{}int32_t", prefix),
                    IntSize::I64 => format!("{}int64_t", prefix),
                    IntSize::I128 => format!("__int128_t"), // GCC extension
                    IntSize::ISize => format!("{}intptr_t", prefix),
                }
            }
            MirType::Float(size) => match size {
                FloatSize::F32 => "float".to_string(),
                FloatSize::F64 => "double".to_string(),
            },
            MirType::Ptr(inner) => {
                format!("{}*", self.type_to_c(inner))
            }
            MirType::Array(elem, len) => {
                // C doesn't allow array types directly in most contexts
                // This is handled specially in declarations
                format!("{}[{}]", self.type_to_c(elem), len)
            }
            MirType::Slice(elem) => {
                // Slice as fat pointer struct
                format!("struct {{ {}* ptr; size_t len; }}", self.type_to_c(elem))
            }
            MirType::Struct(name) => {
                // Resolve unresolved Self to the enclosing function's impl type.
                // E.g., in function "Point_new", Self → Point.
                if name.as_ref() == "Self" {
                    if let Some(ref fn_name) = self.current_fn_name {
                        if let Some(idx) = fn_name.rfind('_') {
                            return fn_name[..idx].to_string();
                        }
                    }
                }
                name.to_string()
            }
            MirType::FnPtr(sig) => {
                let ret = self.type_to_c(&sig.ret);
                let params: Vec<_> = sig.params.iter().map(|p| self.type_to_c(p)).collect();
                format!("{} (*)({})", ret, params.join(", "))
            }
            MirType::Never => "void".to_string(), // Never returns
            MirType::Vector(elem, lanes) => {
                // Use GCC/Clang vector extension
                let elem_ty = self.type_to_c(elem);
                format!("{} __attribute__((vector_size({})))", elem_ty, lanes * 4)
            }
            MirType::Texture2D(_) => "void*".to_string(), // Opaque GPU handle
            MirType::Sampler => "void*".to_string(),      // Opaque GPU handle
            MirType::SampledImage(_) => "void*".to_string(), // Opaque GPU handle
            MirType::TraitObject(name) => format!("dyn_{}", name), // vtable struct
            MirType::Vec(_) => "BuildVecHandle".to_string(),
            // The handle-struct name depends on key/value types. Default map is
            // str->f64 (BuildStrF64MapHandle), which also backs the value-typed
            // str-key families (build_hmap_*_val_*). The i64->f64 family has its
            // own handle. The i32->i32 family never flows through here: map_new_i32
            // returns Struct("BuildMapHandle") directly. Without this split, a
            // `map_new_i64()` local was declared BuildStrF64MapHandle while the
            // runtime returned BuildI64F64MapHandle, a real C2440.
            MirType::Map(ref k, ref v) => match (k.as_ref(), v.as_ref()) {
                (MirType::Int(IntSize::I64, _), MirType::Float(..)) => {
                    "BuildI64F64MapHandle".to_string()
                }
                _ => "BuildStrF64MapHandle".to_string(),
            },
            MirType::Tuple(ref elems) => {
                if elems.is_empty() {
                    "void".to_string()
                } else {
                    MirType::tuple_type_name(elems).to_string()
                }
            }
        }
    }

    /// Recursively extract the base (non-array) element type and all dimension
    /// sizes from a (possibly nested) `MirType::Array`.
    ///
    /// For `Array(Array(f64, 3), 3)` this returns `(f64, [3, 3])`.
    /// The dimensions are in *outer-to-inner* order so they can be appended
    /// directly after the variable name: `double name[3][3]`.
    fn array_base_and_dims<'a>(&self, ty: &'a MirType) -> (&'a MirType, Vec<u64>) {
        let mut current = ty;
        let mut dims = Vec::new();
        while let MirType::Array(ref elem, len) = *current {
            dims.push(len);
            current = elem;
        }
        (current, dims)
    }

    /// Format an array declaration: `base_type name[d1][d2]...`.
    /// Returns the string assuming the caller will append `;\n` or similar.
    fn fmt_array_decl(&self, ty: &MirType, name: &str) -> String {
        let (base, dims) = self.array_base_and_dims(ty);
        let base_c = self.type_to_c(base);
        let dim_str: String = dims.iter().map(|d| format!("[{}]", d)).collect();
        format!("{} {}{}", base_c, name, dim_str)
    }

    /// The synthetic out-parameter name used to return a fixed-size `Array<T,N>`
    /// by value. C cannot return an array type or assign to one, so an
    /// array-returning function is lowered to a `void` function that takes a
    /// trailing pointer-to-array out-parameter and `memcpy`s the result into it.
    const ARRAY_RET_OUT_PARAM: &'static str = "__buildret_out";

    /// Format the trailing out-parameter declaration for an array return type.
    /// For a return type `[f64; 3]` this yields `double (*__buildret_out)[3]`
    /// (a pointer to `double[3]`); for `[[f64; 4]; 4]`, `double (*__buildret_out)[4][4]`.
    /// Returns `None` for non-array return types.
    fn array_ret_out_param(&self, ret_ty: &MirType) -> Option<String> {
        if !matches!(ret_ty, MirType::Array(_, _)) {
            return None;
        }
        let (base, dims) = self.array_base_and_dims(ret_ty);
        let base_c = self.type_to_c(base);
        let dim_str: String = dims.iter().map(|d| format!("[{}]", d)).collect();
        Some(format!(
            "{} (*{}){}",
            base_c,
            Self::ARRAY_RET_OUT_PARAM,
            dim_str
        ))
    }

    fn value_to_c(&self, value: &MirValue, locals: &[MirLocal]) -> String {
        match value {
            MirValue::Local(id) => self.local_name(*id, locals),
            MirValue::Const(c) => self.const_to_c(c),
            MirValue::Global(name) => match name.as_ref() {
                "None" => return "((Option){ .has_value = false })".to_string(),
                "Stdin" => return "((io_Stdin){ 0 })".to_string(),
                "Stdout" => return "((io_Stdout){ 0 })".to_string(),
                "Stderr" => return "((io_Stderr){ 0 })".to_string(),
                // A function reference in value position (a direct call's callee)
                // must get the same stdlib-collision escape as its definition.
                other
                    if !Self::is_runtime_or_builtin_fn(other)
                        && Self::is_c_stdlib_collision(other) =>
                {
                    format!("_{}", other)
                }
                _ => name.to_string(),
            },
            MirValue::Function(name) => {
                // Map well-known value-position names to C struct literals.
                // These appear when the MIR uses a function name as a value
                // (e.g., `return None;`, `current = None;`).
                match name.as_ref() {
                    "None" => return "((Option){ .has_value = false })".to_string(),
                    "Stdin" => return "((io_Stdin){ 0 })".to_string(),
                    "Stdout" => return "((io_Stdout){ 0 })".to_string(),
                    "Stderr" => return "((io_Stderr){ 0 })".to_string(),
                    _ => {}
                }
                // Escape user-defined function names that conflict with C
                // reserved words or macros - but never escape runtime helpers
                // (build_*), standard C math/stdlib functions, or other
                // known builtins.  These are already correct as-is.
                if Self::is_runtime_or_builtin_fn(name) {
                    name.to_string()
                } else if Self::is_c_reserved(name) || Self::is_c_stdlib_collision(name) {
                    format!("_{}", name)
                } else {
                    name.to_string()
                }
            }
        }
    }

    fn const_to_c(&self, c: &MirConst) -> String {
        match c {
            MirConst::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            MirConst::Int(v, ty) => match ty {
                MirType::Int(IntSize::I64, _) => format!("{}LL", v),
                MirType::Int(IntSize::I128, _) => format!("((__int128){})", v),
                _ => v.to_string(),
            },
            MirConst::Uint(v, ty) => match ty {
                MirType::Int(IntSize::I64, _) => format!("{}ULL", v),
                _ => format!("{}U", v),
            },
            MirConst::Float(v, ty) => {
                // Ensure float constants always have a decimal point in C
                // so that `1.0 / 3.0` doesn't become `1 / 3` (integer division).
                let s = format!("{}", v);
                let needs_dot = !s.contains('.') && !s.contains('e') && !s.contains('E');
                match ty {
                    MirType::Float(FloatSize::F32) => {
                        if needs_dot {
                            format!("{}.0f", s)
                        } else {
                            format!("{}f", s)
                        }
                    }
                    _ => {
                        if needs_dot {
                            format!("{}.0", s)
                        } else {
                            s
                        }
                    }
                }
            }
            MirConst::Str(idx) => format!("__str{}", idx),
            MirConst::ByteStr(bytes) => {
                let escaped: String = bytes.iter().map(|b| format!("\\x{:02x}", b)).collect();
                format!("\"{}\"", escaped)
            }
            MirConst::Null(_) => "NULL".to_string(),
            MirConst::Unit => "((void)0)".to_string(),
            MirConst::Zeroed(ty) => {
                // Zero initializer
                format!("(({}){{}})", self.type_to_c(ty))
            }
            MirConst::Undef(_) => "/* undef */ 0".to_string(),
            MirConst::Struct(name, fields) => {
                let field_strs: Vec<String> = fields.iter().map(|f| self.const_to_c(f)).collect();
                format!("({}){{ {} }}", name, field_strs.join(", "))
            }
        }
    }

    /// Emit a constant as a C static initializer (file-scope `const`/global).
    /// Aggregate constants become plain brace-init lists `{ ... }` rather than
    /// compound literals `(T){ ... }`: C requires the initializer of an object
    /// with static storage duration to be a constant expression, and a compound
    /// literal is not one, so MSVC rejects `const Color W = (Color){...};` with
    /// C2099. Nested structs recurse the same way; scalars and strings are
    /// already constant expressions and defer to `const_to_c`.
    fn const_to_c_static(&self, c: &MirConst) -> String {
        match c {
            MirConst::Struct(_, fields) => {
                let field_strs: Vec<String> =
                    fields.iter().map(|f| self.const_to_c_static(f)).collect();
                format!("{{ {} }}", field_strs.join(", "))
            }
            MirConst::Zeroed(_) => "{0}".to_string(),
            _ => self.const_to_c(c),
        }
    }

    /// If `base` indexes a slice (fat pointer `{ptr, len}`) or a pointer-to-slice
    /// (`&[T]` -> `Ptr(Slice)`), return the correct element access. A slice value
    /// is `base.ptr[index]`; a pointer-to-slice (a slice/array *parameter*) is
    /// `base->ptr[index]`. Returns `None` for non-slice bases (caller emits a
    /// plain subscript). Without this, indexing a `&[f32]` parameter emitted
    /// `base[index]`, which subscripts the fat-pointer struct itself (a type
    /// error), so compute kernels using slice params never compiled on the C
    /// path.
    fn slice_element_access(
        base: &MirValue,
        base_str: &str,
        index_str: &str,
        locals: &[MirLocal],
    ) -> Option<String> {
        let ty = match base {
            MirValue::Local(id) => &locals.get(id.0 as usize)?.ty,
            _ => return None,
        };
        match ty {
            MirType::Slice(_) => Some(format!("{}.ptr[{}]", base_str, index_str)),
            MirType::Ptr(inner) if matches!(inner.as_ref(), MirType::Slice(_)) => {
                Some(format!("{}->ptr[{}]", base_str, index_str))
            }
            _ => None,
        }
    }

    fn rvalue_to_c(&self, rvalue: &MirRValue, locals: &[MirLocal]) -> CodegenResult<String> {
        Ok(match rvalue {
            MirRValue::Use(value) => self.value_to_c(value, locals),
            MirRValue::BinaryOp { op, left, right } => {
                let l = self.value_to_c(left, locals);
                let r = self.value_to_c(right, locals);
                if *op == BinOp::Pow {
                    return Ok(format!("pow({}, {})", l, r));
                }
                // C's `%` is integer-only. A float remainder must lower to the
                // C library `fmod`/`fmodf`, which matches Rust's `%` on floats
                // (5.5 % 2.0 == 1.5). Without this the backend emitted `(a % b)`
                // on doubles, which gcc rejects outright.
                if *op == BinOp::Rem {
                    let float_size = |v: &MirValue| -> Option<FloatSize> {
                        match v {
                            MirValue::Const(MirConst::Float(_, MirType::Float(sz))) => Some(*sz),
                            MirValue::Local(id) => match locals.get(id.0 as usize).map(|loc| &loc.ty) {
                                Some(MirType::Float(sz)) => Some(*sz),
                                _ => None,
                            },
                            _ => None,
                        }
                    };
                    if let Some(sz) = float_size(left).or_else(|| float_size(right)) {
                        let f = match sz {
                            FloatSize::F32 => "fmodf",
                            FloatSize::F64 => "fmod",
                        };
                        return Ok(format!("{}({}, {})", f, l, r));
                    }
                }
                // String comparison: use strcmp when either operand is BuildString
                if *op == BinOp::Eq || *op == BinOp::Ne {
                    let is_string = |v: &MirValue| -> bool {
                        match v {
                            MirValue::Local(id) => locals
                                .get(id.0 as usize)
                                .map(|loc| matches!(loc.ty, MirType::Struct(ref n) if n.as_ref() == "BuildString"))
                                .unwrap_or(false),
                            _ => false,
                        }
                    };
                    if is_string(left) && is_string(right) {
                        let cmp = format!("strcmp({}.ptr, {}.ptr)", l, r);
                        return Ok(if *op == BinOp::Eq {
                            format!("({} == 0)", cmp)
                        } else {
                            format!("({} != 0)", cmp)
                        });
                    }
                }
                // Shift-count masking. Rust wraps a shift count to the left
                // operand's bit width (release semantics): `a << b` shifts by
                // `b % bits`. C promotes a sub-`int` operand to 32-bit `int`
                // before shifting, so a narrow type (i8/i16/u8/u16) shifted by
                // a count at or past its width used the un-masked 32-bit count
                // and printed a wrong value (255u8 >> 9 gave 0 instead of 127,
                // 1u8 << 8 gave 0 instead of 1). Mask the count to the operand
                // width for every integer width: for i32/i64 this equals the
                // count-masking x86 already performs, so those stay correct
                // while the narrow widths are fixed and the emitted C no longer
                // depends on a target-specific shift rule. `<<`/`>>` bind
                // tighter than `&`, so the masked count is fully parenthesized.
                if *op == BinOp::Shl || *op == BinOp::Shr {
                    let int_bits = |v: &MirValue| -> Option<u32> {
                        let ty = match v {
                            MirValue::Const(MirConst::Int(_, t))
                            | MirValue::Const(MirConst::Uint(_, t)) => t,
                            MirValue::Local(id) => &locals.get(id.0 as usize)?.ty,
                            _ => return None,
                        };
                        match ty {
                            MirType::Int(size, _) => Some(match size {
                                IntSize::I8 => 8,
                                IntSize::I16 => 16,
                                IntSize::I32 => 32,
                                IntSize::I64 | IntSize::ISize => 64,
                                IntSize::I128 => 128,
                            }),
                            _ => None,
                        }
                    };
                    if let Some(bits) = int_bits(left) {
                        let op_str = self.binop_to_c(*op);
                        return Ok(format!("({} {} (({}) & {}))", l, op_str, r, bits - 1));
                    }
                }
                let op_str = self.binop_to_c(*op);
                format!("({} {} {})", l, op_str, r)
            }
            MirRValue::UnaryOp { op, operand } => {
                let v = self.value_to_c(operand, locals);
                let op_str = match op {
                    UnaryOp::Not => "!",
                    UnaryOp::BitNot => "~",
                    UnaryOp::Neg => "-",
                };
                format!("({}{})", op_str, v)
            }
            MirRValue::Ref { is_mut: _, place } => {
                let local_name = self.local_name(place.local, locals);
                format!("&{}", local_name)
            }
            MirRValue::AddressOf { is_mut: _, place } => {
                let local_name = self.local_name(place.local, locals);
                format!("&{}", local_name)
            }
            MirRValue::Cast {
                kind: CastKind::FloatToInt,
                value,
                ty,
            } => {
                // A raw C cast of a float outside the target integer's range,
                // or of NaN, is undefined behavior. Route through a saturating
                // helper so the result matches Rust's `as` (clamp to min/max,
                // NaN -> 0). The outer cast pins the exact target C type, which
                // covers isize/usize/i128 whose helper returns a wider integer.
                let v = self.value_to_c(value, locals);
                let t = self.type_to_c(ty);
                let helper = match ty {
                    MirType::Int(size, signed) => match (size, signed) {
                        (IntSize::I8, true) => "bl_f2i_i8",
                        (IntSize::I8, false) => "bl_f2i_u8",
                        (IntSize::I16, true) => "bl_f2i_i16",
                        (IntSize::I16, false) => "bl_f2i_u16",
                        (IntSize::I32, true) => "bl_f2i_i32",
                        (IntSize::I32, false) => "bl_f2i_u32",
                        (IntSize::I64, true) | (IntSize::ISize, true) => "bl_f2i_i64",
                        (IntSize::I64, false) | (IntSize::ISize, false) => "bl_f2i_u64",
                        (IntSize::I128, _) => "bl_f2i_i128",
                    },
                    _ => "",
                };
                if helper.is_empty() {
                    format!("(({}){})", t, v)
                } else {
                    format!("(({}){}({}))", t, helper, v)
                }
            }
            MirRValue::Cast { kind: _, value, ty } => {
                let v = self.value_to_c(value, locals);
                let t = self.type_to_c(ty);
                format!("(({}){})", t, v)
            }
            MirRValue::Aggregate { kind, operands } => {
                let vals: Vec<_> = operands
                    .iter()
                    .map(|o| self.value_to_c(o, locals))
                    .collect();
                match kind {
                    AggregateKind::Array(_) => format!("{{ {} }}", vals.join(", ")),
                    AggregateKind::Tuple => {
                        // Determine element types to build the typedef name.
                        let elem_tys: Vec<MirType> = operands
                            .iter()
                            .map(|op| match op {
                                MirValue::Local(id) => locals
                                    .get(id.0 as usize)
                                    .map(|l| l.ty.clone())
                                    .unwrap_or(MirType::i32()),
                                MirValue::Const(c) => match c {
                                    MirConst::Bool(_) => MirType::Bool,
                                    MirConst::Int(_, ty) => ty.clone(),
                                    MirConst::Uint(_, ty) => ty.clone(),
                                    MirConst::Float(_, ty) => ty.clone(),
                                    _ => MirType::i32(),
                                },
                                _ => MirType::i32(),
                            })
                            .collect();
                        if elem_tys.is_empty() {
                            format!("{{ {} }}", vals.join(", "))
                        } else {
                            let name = MirType::tuple_type_name(&elem_tys);
                            format!("({}){{ {} }}", name, vals.join(", "))
                        }
                    }
                    AggregateKind::Struct(name) => {
                        if name.starts_with("dyn_") && vals.len() == 2 {
                            // Fat pointer: { data = (void*)&obj, vtable = &vtable_instance }
                            format!(
                                "({}){{ .data = {}, .vtable = ({}_vtable*)&{} }}",
                                name,
                                vals[0],
                                name.strip_prefix("dyn_").unwrap_or("Unknown"),
                                vals[1]
                            )
                        } else if vals.is_empty() {
                            // Unit struct (no fields): portable zero-init.
                            format!("({}){{0}}", name)
                        } else {
                            format!("({}){{ {} }}", name, vals.join(", "))
                        }
                    }
                    AggregateKind::Variant(name, disc, variant_name) => {
                        // Option/Result are prelude builtins with a fixed field
                        // layout (`.has_value`/`.value`, `.is_ok`/`.ok`/`.err`)
                        // that the runtime methods and match-lowering read. A
                        // path-qualified construction (`Option::Some(x)`) reaches
                        // this generic aggregate path when the program also
                        // declares `enum Option`; emit the builtin shape so it
                        // agrees with those readers instead of the generic
                        // `.tag`/`.data` shape (which the Option typedef lacks).
                        let is_builtin_sum = matches!(name.as_ref(), "Option" | "Result");
                        if is_builtin_sum {
                            // Pick the 8-byte union slot from the payload type.
                            let slot = match operands.first() {
                                Some(MirValue::Local(id)) => {
                                    match locals.get(id.0 as usize).map(|l| &l.ty) {
                                        Some(MirType::Float(_)) => "f",
                                        Some(MirType::Ptr(_)) => "p",
                                        _ => "i",
                                    }
                                }
                                Some(MirValue::Const(MirConst::Float(..))) => "f",
                                _ => "i",
                            };
                            let cast = match slot {
                                "f" => "(double)",
                                "p" => "(void*)",
                                _ => "(int64_t)",
                            };
                            match (name.as_ref(), variant_name.as_ref()) {
                                ("Option", "Some") if !vals.is_empty() => format!(
                                    "((Option){{ .has_value = true, .value = {{ .{} = {}{} }} }})",
                                    slot, cast, vals[0]
                                ),
                                ("Option", "Some") => {
                                    "((Option){ .has_value = true })".to_string()
                                }
                                ("Option", _) => "((Option){ .has_value = false })".to_string(),
                                ("Result", "Ok") if !vals.is_empty() => format!(
                                    "((Result){{ .is_ok = true, .ok = {{ .ok_{} = {}{} }} }})",
                                    slot, cast, vals[0]
                                ),
                                ("Result", "Ok") => "((Result){ .is_ok = true })".to_string(),
                                ("Result", _) if !vals.is_empty() => format!(
                                    "((Result){{ .is_ok = false, .err = {{ .err_{} = {}{} }} }})",
                                    slot, cast, vals[0]
                                ),
                                ("Result", _) => "((Result){ .is_ok = false })".to_string(),
                                _ => unreachable!(),
                            }
                        } else if vals.is_empty() {
                            // Unit variant - no data fields
                            format!("({}){{ .tag = {} }}", name, disc)
                        } else {
                            format!(
                                "({}){{ .tag = {}, .data = {{ .{} = {{ {} }} }} }}",
                                name,
                                disc,
                                variant_name,
                                vals.join(", ")
                            )
                        }
                    }
                    AggregateKind::Closure(_) => format!("{{ {} }}", vals.join(", ")),
                }
            }
            MirRValue::Repeat { value, count } => {
                let v = self.value_to_c(value, locals);
                // C doesn't have array repeat, use designated initializers
                format!("{{ [0 ... {}] = {} }}", count - 1, v)
            }
            MirRValue::Discriminant(place) => {
                let local_name = self.local_name(place.local, locals);
                format!("{}.tag", local_name)
            }
            MirRValue::Len(place) => {
                let local_name = self.local_name(place.local, locals);
                format!("{}.len", local_name)
            }
            MirRValue::NullaryOp(op, ty) => match op {
                NullaryOp::SizeOf => format!("sizeof({})", self.type_to_c(ty)),
                NullaryOp::AlignOf => format!("_Alignof({})", self.type_to_c(ty)),
                // The GPU compute thread index reads an ambient per-invocation
                // variable. On the CPU cross-check path the dispatch driver
                // defines `buildc_gl_global_invocation_{x,y,z}` as it loops over
                // the grid; on GPU the SPIR-V backend wires GlobalInvocationId.
                NullaryOp::ThreadIndex(component) => {
                    let axis = match component {
                        0 => "x",
                        1 => "y",
                        _ => "z",
                    };
                    format!("buildc_gl_global_invocation_{}", axis)
                }
                // The LOCAL invocation and WORKGROUP indices likewise read
                // ambient per-invocation variables the (future Phase-4b) dispatch
                // driver defines as it loops the grid. The SPIR-V backend wires
                // the real LocalInvocationId / WorkgroupId built-ins.
                NullaryOp::LocalInvocationId(component) => {
                    let axis = match component {
                        0 => "x",
                        1 => "y",
                        _ => "z",
                    };
                    format!("buildc_gl_local_invocation_{}", axis)
                }
                NullaryOp::WorkgroupId(component) => {
                    let axis = match component {
                        0 => "x",
                        1 => "y",
                        _ => "z",
                    };
                    format!("buildc_gl_workgroup_{}", axis)
                }
            },
            MirRValue::FieldAccess {
                base,
                field_name,
                field_ty,
            } => {
                let base_str = self.value_to_c(base, locals);
                // Option payload read: `opt.value` is an 8-byte union; read the
                // typed slot (`.i`/`.f`/`.p`) and cast back to the payload type.
                let base_is_option = matches!(base, MirValue::Local(id)
                    if locals.get(id.0 as usize)
                        .map(|l| matches!(&l.ty, MirType::Struct(n) if n.as_ref() == "Option"))
                        .unwrap_or(false));
                if base_is_option && field_name.as_ref() == "value" {
                    // Boxed payload (>8 bytes): the .p slot holds a malloc'd
                    // pointer; deref it back to the payload type.
                    if Self::payload_needs_boxing(field_ty) {
                        let ct = self.type_to_c(field_ty);
                        return Ok(format!("(*({}*){}.value.p)", ct, base_str));
                    }
                    let slot = match field_ty {
                        MirType::Float(_) => "f",
                        MirType::Ptr(_) => "p",
                        _ => "i",
                    };
                    return Ok(format!(
                        "({}){}.value.{}",
                        self.type_to_c(field_ty),
                        base_str,
                        slot
                    ));
                }
                // Result Ok payload read: `res.ok` is an 8-byte union; read the
                // typed slot (`.ok_i`/`.ok_f`/`.ok_p`) and cast to the payload
                // type. (`.err` is a plain BuildString field handled below.)
                let base_is_result = matches!(base, MirValue::Local(id)
                    if locals.get(id.0 as usize)
                        .map(|l| matches!(&l.ty, MirType::Struct(n) if n.as_ref() == "Result"))
                        .unwrap_or(false));
                if base_is_result && field_name.as_ref() == "ok" {
                    // Boxed Ok payload (>8 bytes): deref the malloc'd pointer.
                    if Self::payload_needs_boxing(field_ty) {
                        let ct = self.type_to_c(field_ty);
                        return Ok(format!("(*({}*){}.ok.ok_p)", ct, base_str));
                    }
                    let slot = match field_ty {
                        MirType::Float(_) => "ok_f",
                        MirType::Ptr(_) => "ok_p",
                        _ => "ok_i",
                    };
                    return Ok(format!(
                        "({}){}.ok.{}",
                        self.type_to_c(field_ty),
                        base_str,
                        slot
                    ));
                }
                // Result Err payload read: `res.err` is a typed union, symmetric
                // to `ok` (boxed for >8-byte payloads such as String).
                if base_is_result && field_name.as_ref() == "err" {
                    if Self::payload_needs_boxing(field_ty) {
                        let ct = self.type_to_c(field_ty);
                        return Ok(format!("(*({}*){}.err.err_p)", ct, base_str));
                    }
                    let slot = match field_ty {
                        MirType::Float(_) => "err_f",
                        MirType::Ptr(_) => "err_p",
                        _ => "err_i",
                    };
                    return Ok(format!(
                        "({}){}.err.{}",
                        self.type_to_c(field_ty),
                        base_str,
                        slot
                    ));
                }
                // If the base value is a pointer type, use -> instead of .
                let is_ptr = match base {
                    MirValue::Local(id) => locals
                        .get(id.0 as usize)
                        .map(|l| l.ty.is_pointer())
                        .unwrap_or(false),
                    _ => false,
                };
                let field = Self::escape_c_keyword(field_name);
                if is_ptr {
                    format!("{}->{}", base_str, field)
                } else {
                    format!("{}.{}", base_str, field)
                }
            }
            MirRValue::VariantField {
                base,
                variant_name,
                field_index,
                ..
            } => {
                let base_str = self.value_to_c(base, locals);
                format!("{}.data.{}.f{}", base_str, variant_name, field_index)
            }
            MirRValue::IndexAccess {
                base,
                index,
                elem_ty,
            } => {
                let base_str = self.value_to_c(base, locals);
                let index_str = self.value_to_c(index, locals);
                // For Vec types, use typed runtime getter instead of raw subscript.
                let base_is_vec = match base {
                    MirValue::Local(id) => locals
                        .get(id.0 as usize)
                        .map(|l| matches!(l.ty, MirType::Vec(_)))
                        .unwrap_or(false),
                    _ => false,
                };
                if base_is_vec {
                    // Scalar/string elements use the typed getters; aggregate elements
                    // (structs, tuples) use the generic pointer getter + cast/deref so
                    // Vec<struct> / Vec<tuple> indexing reads the element by value.
                    match elem_ty {
                        MirType::Float(_) => {
                            format!("build_hvec_get_f64({}, {})", base_str, index_str)
                        }
                        MirType::Int(IntSize::I64, _) => {
                            format!("build_hvec_get_i64({}, {})", base_str, index_str)
                        }
                        MirType::Struct(n) if n.as_ref() == "BuildString" => {
                            format!("build_hvec_get_str({}, {})", base_str, index_str)
                        }
                        MirType::Int(..) | MirType::Bool => {
                            format!("build_hvec_get_i32({}, {})", base_str, index_str)
                        }
                        other => {
                            let ct = self.type_to_c(other);
                            format!(
                                "(*({}*)build_vec_get({}.inner, {}))",
                                ct, base_str, index_str
                            )
                        }
                    }
                } else {
                    // BuildString indexing: access .ptr[index] for byte access
                    let base_is_string = match base {
                        MirValue::Local(id) => locals
                            .get(id.0 as usize)
                            .map(|l| matches!(l.ty, MirType::Struct(ref n) if n.as_ref() == "BuildString"))
                            .unwrap_or(false),
                        _ => false,
                    };
                    if base_is_string {
                        format!("((uint8_t*){}.ptr)[{}]", base_str, index_str)
                    } else if let Some(access) =
                        Self::slice_element_access(base, &base_str, &index_str, locals)
                    {
                        access
                    } else {
                        format!("{}[{}]", base_str, index_str)
                    }
                }
            }
            MirRValue::Deref { ptr, .. } => {
                let ptr_str = self.value_to_c(ptr, locals);
                format!("(*{})", ptr_str)
            }
            MirRValue::TextureSample { .. } => {
                // GPU-only operation; emit a zero placeholder in C
                "/* texture_sample: GPU-only */ 0".to_string()
            }
        })
    }

    fn record_string_literal_assignment(&mut self, dest: LocalId, value: &MirRValue) {
        if let MirRValue::Use(MirValue::Const(MirConst::Str(idx))) = value {
            self.local_string_literals.insert(dest, *idx);
        } else {
            self.local_string_literals.remove(&dest);
        }
    }

    fn print_format_literal(&self, value: &MirValue) -> Option<&str> {
        let idx = match value {
            MirValue::Const(MirConst::Str(idx)) => Some(*idx),
            MirValue::Local(id) => self.local_string_literals.get(id).copied(),
            _ => None,
        }?;
        self.string_literals.get(idx as usize).map(|s| s.as_ref())
    }

    fn value_mir_type<'a>(
        &self,
        value: &'a MirValue,
        locals: &'a [MirLocal],
    ) -> Option<&'a MirType> {
        match value {
            MirValue::Local(id) => locals.get(id.0 as usize).map(|local| &local.ty),
            MirValue::Const(MirConst::Bool(_)) => Some(&MirType::Bool),
            MirValue::Const(MirConst::Int(_, ty))
            | MirValue::Const(MirConst::Uint(_, ty))
            | MirValue::Const(MirConst::Float(_, ty))
            | MirValue::Const(MirConst::Null(ty))
            | MirValue::Const(MirConst::Zeroed(ty))
            | MirValue::Const(MirConst::Undef(ty)) => Some(ty),
            MirValue::Const(MirConst::Str(_)) => None,
            _ => None,
        }
    }

    /// Resolve the MIR type of a value passed to a sum-type constructor
    /// (`Ok`/`Err`/`Some`), for deciding whether the payload must be boxed.
    /// Beyond locals, this recognizes the `None` value (a `Global`/`Function`
    /// literal that is an `Option`), so `Ok(None)` / `Some(None)` box correctly
    /// instead of casting the Option struct into the scalar slot.
    fn sumtype_arg_type(&self, value: &MirValue, locals: &[MirLocal]) -> Option<MirType> {
        match value {
            MirValue::Local(id) => locals.get(id.0 as usize).map(|l| l.ty.clone()),
            MirValue::Global(n) | MirValue::Function(n) if n.as_ref() == "None" => {
                Some(MirType::Struct(Arc::from("Option")))
            }
            _ => None,
        }
    }

    fn value_is_build_string(&self, value: &MirValue, locals: &[MirLocal]) -> bool {
        matches!(
            self.value_mir_type(value, locals),
            Some(MirType::Struct(name)) if name.as_ref() == "BuildString"
        ) || matches!(value, MirValue::Const(MirConst::Str(_)))
    }

    fn printf_specifier_for_value(&self, value: &MirValue, locals: &[MirLocal]) -> &'static str {
        match self.value_mir_type(value, locals) {
            Some(MirType::Int(IntSize::I64 | IntSize::I128 | IntSize::ISize, true)) => "%lld",
            Some(MirType::Int(IntSize::I64 | IntSize::I128 | IntSize::ISize, false)) => "%llu",
            Some(MirType::Int(_, true)) => "%d",
            Some(MirType::Int(_, false)) => "%u",
            // Floats print via a runtime formatter that renders Rust's Display
            // (shortest round-tripping decimal), passed through printf as "%s".
            Some(MirType::Float(_)) => "%s",
            Some(MirType::Bool) => "%s",
            Some(MirType::Ptr(_)) => "%p",
            Some(MirType::Struct(name)) if name.as_ref() == "BuildString" => "%s",
            _ => "%s",
        }
    }

    fn print_arg_to_c(&self, value: &MirValue, locals: &[MirLocal]) -> String {
        let rendered = self.value_to_c(value, locals);
        if matches!(value, MirValue::Const(MirConst::Bool(_)))
            || matches!(self.value_mir_type(value, locals), Some(MirType::Bool))
        {
            format!("{} ? \"true\" : \"false\"", rendered)
        } else if self.value_is_build_string(value, locals) {
            match value {
                MirValue::Const(MirConst::Str(idx)) => format!("__str{}", idx),
                _ => format!("{}.ptr", rendered),
            }
        } else if let Some(MirType::Float(size)) = self.value_mir_type(value, locals) {
            // Render floats with the runtime shortest-round-trip formatter so
            // printed output matches Rust's Display instead of %g's 6 digits.
            match size {
                FloatSize::F32 => format!("bl_fmt_f32({})", rendered),
                FloatSize::F64 => format!("bl_fmt_f64({})", rendered),
            }
        } else {
            rendered
        }
    }

    fn build_format_to_c_printf(
        &self,
        format: &str,
        args: &[MirValue],
        locals: &[MirLocal],
    ) -> String {
        let mut out = String::new();
        let mut arg_index = 0usize;
        let mut chars = format.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '{' {
                match chars.peek().copied() {
                    Some('}') => {
                        chars.next();
                        if let Some(arg) = args.get(arg_index) {
                            out.push_str(self.printf_specifier_for_value(arg, locals));
                        } else {
                            out.push_str("{}");
                        }
                        arg_index += 1;
                    }
                    Some('{') => {
                        chars.next();
                        out.push('{');
                    }
                    _ => out.push(ch),
                }
            } else if ch == '}' {
                if chars.peek() == Some(&'}') {
                    chars.next();
                }
                out.push('}');
            } else if ch == '%' {
                out.push_str("%%");
            } else {
                out.push(ch);
            }
        }

        out
    }

    fn emit_print_call(
        &mut self,
        func_name: &str,
        args: &[MirValue],
        target: Option<BlockId>,
        blocks: &[MirBlock],
        locals: &[MirLocal],
    ) -> CodegenResult<()> {
        let is_err = matches!(func_name, "eprintln" | "eprint");
        let newline = matches!(func_name, "println" | "eprintln");

        if args.is_empty() {
            let output = if newline { "\\n" } else { "" };
            if is_err {
                write!(self.output, "fprintf(stderr, \"{}\");\n", output).unwrap();
            } else {
                write!(self.output, "printf(\"{}\");\n", output).unwrap();
            }
        } else if let Some(format) = self.print_format_literal(&args[0]) {
            let mut c_format = self.build_format_to_c_printf(format, &args[1..], locals);
            if newline {
                c_format.push('\n');
            }
            let escaped_format = self.escape_string(&c_format);
            let rendered_args = args
                .iter()
                .skip(1)
                .map(|arg| self.print_arg_to_c(arg, locals))
                .collect::<Vec<_>>();
            if is_err {
                write!(self.output, "fprintf(stderr, \"{}\"", escaped_format).unwrap();
            } else {
                write!(self.output, "printf(\"{}\"", escaped_format).unwrap();
            }
            if !rendered_args.is_empty() {
                write!(self.output, ", {}", rendered_args.join(", ")).unwrap();
            }
            self.output.push_str(");\n");
        } else {
            let format = if newline {
                format!("{}\\n", self.printf_specifier_for_value(&args[0], locals))
            } else {
                self.printf_specifier_for_value(&args[0], locals)
                    .to_string()
            };
            let arg = self.print_arg_to_c(&args[0], locals);
            if is_err {
                write!(self.output, "fprintf(stderr, \"{}\", {});\n", format, arg).unwrap();
            } else {
                write!(self.output, "printf(\"{}\", {});\n", format, arg).unwrap();
            }
        }

        if let Some(target_block) = target {
            self.write_indent();
            write!(
                self.output,
                "goto {};\n",
                self.block_label(&target_block, blocks)
            )
            .unwrap();
        }

        Ok(())
    }

    /// Emit an assignment, using memcpy for struct/primitive type mismatches.
    fn emit_typed_assign(
        &mut self,
        dest_name: String,
        value: &MirRValue,
        dest_id: LocalId,
        locals: &[MirLocal],
    ) -> CodegenResult<()> {
        // Check for type mismatch that needs memcpy
        let dest_ty = locals.get(dest_id.0 as usize).map(|l| &l.ty);
        let src_ty = if let MirRValue::Use(MirValue::Local(id)) = value {
            locals.get(id.0 as usize).map(|l| &l.ty)
        } else if let MirRValue::FieldAccess { field_ty, .. } = value {
            Some(field_ty)
        } else {
            None
        };
        let needs_cast = if let (Some(dt), Some(st)) = (dest_ty, src_ty) {
            let is_struct = |t: &MirType| {
                matches!(
                    t,
                    MirType::Struct(_) | MirType::Vec(_) | MirType::Map(_, _) | MirType::Tuple(_)
                )
            };
            (is_struct(dt) || is_struct(st)) && dt != st
        } else {
            false
        };
        if needs_cast {
            if let MirRValue::Use(src_val) = value {
                let src_str = self.value_to_c(src_val, locals);
                self.write_indent();
                write!(
                    self.output,
                    "memcpy(&{}, &{}, sizeof({}) < sizeof({}) ? sizeof({}) : sizeof({}));\n",
                    dest_name, src_str, dest_name, src_str, dest_name, src_str
                )
                .unwrap();
            } else {
                // For non-Use rvalues, fall back to direct assignment
                // (the type mismatch is from FieldAccess or other non-local sources)
                let rvalue = self.rvalue_to_c(value, locals)?;
                self.write_indent();
                write!(self.output, "{} = {};\n", dest_name, rvalue).unwrap();
            }
        } else {
            self.write_indent();
            let rvalue = self.rvalue_to_c(value, locals)?;
            write!(self.output, "{} = {};\n", dest_name, rvalue).unwrap();
        }
        Ok(())
    }

    fn binop_to_c(&self, op: BinOp) -> &'static str {
        match op {
            BinOp::Add | BinOp::AddChecked | BinOp::AddWrapping | BinOp::AddSaturating => "+",
            BinOp::Sub | BinOp::SubChecked | BinOp::SubWrapping | BinOp::SubSaturating => "-",
            BinOp::Mul | BinOp::MulChecked | BinOp::MulWrapping => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            // Pow needs special handling with pow() function call, not an operator
            BinOp::Pow => "/* pow */+",
        }
    }

    fn local_name(&self, id: LocalId, locals: &[MirLocal]) -> String {
        locals
            .get(id.0 as usize)
            .and_then(|l| l.name.as_ref())
            .map(|n| {
                let name = n.to_string();
                // Escape C reserved words/types by prefixing with underscore
                let base = if Self::is_c_reserved(&name) {
                    format!("_{}", name)
                } else {
                    name.clone()
                };
                // Disambiguate when multiple locals share the same user name
                // (e.g. pattern bindings `v` in different match arms).  Check
                // if any *other* local has the same name; if so, append the
                // local ID to make the C declaration unique.
                let has_dup = locals.iter().any(|other| {
                    other.id != id && other.name.as_ref().map(|s| s.as_ref()) == Some(n.as_ref())
                });
                if has_dup {
                    format!("{}_{}", base, id.0)
                } else {
                    base
                }
            })
            .unwrap_or_else(|| format!("_{}", id.0))
    }

    /// Increment 5: emit a guarded free for a flag-managed owner:
    /// `if (__bl_live_N) { build_string_free(<name>); __bl_live_N = 0; }`.
    /// Soundness invariant (verbatim, also in `analysis::flags` and the flag
    /// declaration above): at every point in the emitted C, `__bl_live_N ==
    /// 1` implies the owner currently holds a fresh allocated buffer that no
    /// free site has released. The guard is what makes ALLOCATED sound
    /// (double free, uninitialized free); it does NOT guard liveness -- the
    /// call site is responsible for only reaching this at a buffer-dead-at-
    /// entry point (the UAF guard lives in `analysis::flags`'s site rule and
    /// the unconditional Return backstop). The clear (`= 0`) after the free
    /// is load-bearing: without it, a later guarded free on the same path
    /// (e.g. a frontier site followed by the Return backstop) would double-free.
    fn emit_flag_guarded_free(&mut self, owner: LocalId, locals: &[MirLocal]) {
        let name = self.local_name(owner, locals);
        self.write_indent();
        writeln!(
            self.output,
            "if (__bl_live_{0}) {{ build_string_free({1}); __bl_live_{0} = 0; }}",
            owner.0, name
        )
        .unwrap();
    }

    /// Check if a name conflicts with C reserved words or standard type names.
    fn is_c_reserved(name: &str) -> bool {
        matches!(
            name,
            // C keywords
            "auto" | "break" | "case" | "char" | "const" | "continue" |
            "default" | "do" | "double" | "else" | "enum" | "extern" |
            "float" | "for" | "goto" | "if" | "inline" | "int" | "long" |
            "register" | "restrict" | "return" | "short" | "signed" |
            "sizeof" | "static" | "struct" | "switch" | "typedef" |
            "union" | "unsigned" | "void" | "volatile" | "while" |
            // C99/C11 keywords
            "_Alignas" | "_Alignof" | "_Atomic" | "_Bool" | "_Complex" |
            "_Generic" | "_Imaginary" | "_Noreturn" | "_Static_assert" |
            "_Thread_local" |
            // Common standard library identifiers
            "bool" | "true" | "false" | "NULL" |
            // Standard types from stdint.h
            "int8_t" | "int16_t" | "int32_t" | "int64_t" |
            "uint8_t" | "uint16_t" | "uint32_t" | "uint64_t" |
            "size_t" | "ptrdiff_t" | "intptr_t" | "uintptr_t" |
            // printf itself
            "printf" | "fprintf" | "sprintf" |
            // Common macros / functions from stdlib that may collide
            "min" | "max" | "abs" |
            // Win16 legacy keywords (defined as macros in windef.h)
            "near" | "far"
        )
    }

    /// Escape a struct/field identifier that collides with a reserved C word by
    /// appending an underscore. Delegates to `is_c_reserved` so the definition
    /// site and every access site share one list; a narrower hand-maintained set
    /// here let MSVC-reserved names such as `near`/`far` (Win16 legacy keywords)
    /// leak through as `double near;`, which MSVC rejects with C2208.
    fn escape_c_keyword(name: &str) -> String {
        if Self::is_c_reserved(name) {
            format!("{}_", name)
        } else {
            name.to_string()
        }
    }

    /// Collect block IDs that need labels (targeted by non-sequential jumps).
    /// A block needs a label if ANY block other than its immediate predecessor
    /// has a jump (goto/if/switch/call) targeting it.
    fn collect_needed_labels(blocks: &[MirBlock]) -> std::collections::HashSet<BlockId> {
        use crate::codegen::ir::*;
        let mut needed = std::collections::HashSet::new();

        for (i, block) in blocks.iter().enumerate() {
            let next_id = blocks.get(i + 1).map(|b| b.id);
            if let Some(ref term) = block.terminator {
                match term {
                    MirTerminator::Goto(target) => {
                        // Only need label if NOT the next sequential block
                        if Some(*target) != next_id {
                            needed.insert(*target);
                        }
                    }
                    MirTerminator::If {
                        then_block,
                        else_block,
                        ..
                    } => {
                        needed.insert(*then_block);
                        needed.insert(*else_block);
                    }
                    MirTerminator::Switch {
                        targets, default, ..
                    } => {
                        for (_, target) in targets {
                            needed.insert(*target);
                        }
                        needed.insert(*default);
                    }
                    MirTerminator::Call { target, .. } => {
                        if let Some(t) = target {
                            // Call continuation: need label if not next block
                            if Some(*t) != next_id {
                                needed.insert(*t);
                            }
                        }
                    }
                    MirTerminator::Assert { target, unwind, .. } => {
                        needed.insert(*target);
                        if let Some(u) = unwind {
                            needed.insert(*u);
                        }
                    }
                    _ => {}
                }
            }
        }
        needed
    }

    /// Collect all LocalId values that are referenced in the function body.
    /// Used for dead local elimination - locals not in this set can be skipped.
    fn collect_used_locals(func: &MirFunction) -> std::collections::HashSet<LocalId> {
        use crate::codegen::ir::*;
        let mut used = std::collections::HashSet::new();

        let blocks = match &func.blocks {
            Some(blocks) => blocks,
            None => return used,
        };

        fn collect_val(val: &MirValue, used: &mut std::collections::HashSet<LocalId>) {
            if let MirValue::Local(id) = val {
                used.insert(*id);
            }
        }
        fn collect_place(place: &MirPlace, used: &mut std::collections::HashSet<LocalId>) {
            used.insert(place.local);
        }

        for block in blocks {
            for stmt in &block.stmts {
                match &stmt.kind {
                    MirStmtKind::Assign { dest, value } => {
                        used.insert(*dest);
                        match value {
                            MirRValue::Use(v) => collect_val(v, &mut used),
                            MirRValue::BinaryOp { left, right, .. } => {
                                collect_val(left, &mut used);
                                collect_val(right, &mut used);
                            }
                            MirRValue::UnaryOp { operand, .. } => {
                                collect_val(operand, &mut used);
                            }
                            MirRValue::Ref { place, .. } | MirRValue::AddressOf { place, .. } => {
                                collect_place(place, &mut used);
                            }
                            MirRValue::Cast { value, .. } => collect_val(value, &mut used),
                            MirRValue::Aggregate { operands, .. } => {
                                for op in operands {
                                    collect_val(op, &mut used);
                                }
                            }
                            MirRValue::FieldAccess { base, .. }
                            | MirRValue::VariantField { base, .. } => {
                                collect_val(base, &mut used);
                            }
                            MirRValue::IndexAccess { base, index, .. } => {
                                collect_val(base, &mut used);
                                collect_val(index, &mut used);
                            }
                            MirRValue::Repeat { value, .. } => collect_val(value, &mut used),
                            MirRValue::Discriminant(p) | MirRValue::Len(p) => {
                                collect_place(p, &mut used);
                            }
                            MirRValue::TextureSample {
                                texture,
                                sampler,
                                coords,
                            } => {
                                collect_val(texture, &mut used);
                                collect_val(sampler, &mut used);
                                collect_val(coords, &mut used);
                            }
                            _ => {}
                        }
                    }
                    MirStmtKind::DerefAssign { ptr, value } => {
                        used.insert(*ptr);
                        match value {
                            MirRValue::Use(v) => collect_val(v, &mut used),
                            _ => {}
                        }
                    }
                    MirStmtKind::StorageLive(id) | MirStmtKind::StorageDead(id) => {
                        used.insert(*id);
                    }
                    _ => {}
                }
            }
            if let Some(ref term) = block.terminator {
                match term {
                    MirTerminator::Return(v) => {
                        if let Some(v) = v {
                            collect_val(v, &mut used);
                        }
                    }
                    MirTerminator::If { cond, .. } => collect_val(cond, &mut used),
                    MirTerminator::Switch { value, .. } => collect_val(value, &mut used),
                    MirTerminator::Call {
                        func: f,
                        args,
                        dest,
                        ..
                    } => {
                        collect_val(f, &mut used);
                        for a in args {
                            collect_val(a, &mut used);
                        }
                        if let Some(d) = dest {
                            used.insert(*d);
                        }
                    }
                    MirTerminator::Assert { cond, .. } => collect_val(cond, &mut used),
                    _ => {}
                }
            }
        }

        used
    }

    /// True when a user-defined function name collides with a C standard-library
    /// function that is NOT one of our recognized math/runtime builtins. Such a
    /// name must be emitted with a leading underscore at the definition, forward
    /// declaration, and every call site, or the C compiler reports a redefinition
    /// (or silently binds calls to the libc symbol). Names that are intentional
    /// builtins (abs, exit, malloc, ...) are deliberately excluded.
    fn is_c_stdlib_collision(name: &str) -> bool {
        matches!(
            name,
            "div"
                | "ldiv"
                | "lldiv"
                | "labs"
                | "llabs"
                | "system"
                | "remove"
                | "rename"
                | "getenv"
                | "putenv"
                | "rand"
                | "srand"
                | "qsort"
                | "bsearch"
                | "atexit"
                | "abort"
                | "atoi"
                | "atol"
                | "atof"
                | "strtol"
                | "strtod"
                | "toupper"
                | "tolower"
        )
    }

    /// Emit name for a user-defined function: prefixed with `_` when it collides
    /// with a C macro (min/max/abs) or stdlib function (div, system, ...).
    fn user_fn_emit_name(name: &str) -> String {
        if matches!(name, "min" | "max" | "abs") || Self::is_c_stdlib_collision(name) {
            format!("_{}", name)
        } else {
            name.to_string()
        }
    }

    fn is_runtime_or_builtin_fn(name: &str) -> bool {
        // Runtime helpers all start with "build_"
        if name.starts_with("build_") {
            return true;
        }
        // Standard C math / stdlib functions used by the builtin system
        matches!(
            name,
            "sqrt"
                | "cbrt"
                | "sin"
                | "cos"
                | "tan"
                | "pow"
                | "fabs"
                | "asin"
                | "acos"
                | "atan"
                | "atan2"
                | "sinh"
                | "cosh"
                | "tanh"
                | "exp"
                | "exp2"
                | "log"
                | "log2"
                | "log10"
                | "floor"
                | "ceil"
                | "round"
                | "trunc"
                | "fmax"
                | "fmin"
                | "fmod"
                | "hypot"
                | "copysign"
                | "abs"
                | "printf"
                | "fprintf"
                | "sprintf"
                | "setjmp"
                | "longjmp"
                | "malloc"
                | "realloc"
                | "free"
                | "memcpy"
                | "memcmp"
                | "strlen"
                | "fopen"
                | "fclose"
                | "fread"
                | "fwrite"
                | "fseek"
                | "ftell"
                | "exit"
                | "abort"
        )
    }

    fn escape_string(&self, s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                '\n' => result.push_str("\\n"),
                '\r' => result.push_str("\\r"),
                '\t' => result.push_str("\\t"),
                '\\' => result.push_str("\\\\"),
                '"' => result.push_str("\\\""),
                c if c.is_ascii_control() => {
                    result.push_str(&format!("\\x{:02x}", c as u8));
                }
                c => result.push(c),
            }
        }
        result
    }
}

impl Default for CBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for CBackend {
    fn generate(&mut self, mir: &MirModule) -> CodegenResult<GeneratedCode> {
        self.output.clear();
        self.temp_counter = 0;

        self.generate_module(mir)?;

        // Collect the libraries named by `link "..."` clauses, sorted and
        // de-duplicated, so the build driver can pass them to the C compiler.
        let mut link_libraries: Vec<String> = mir
            .functions
            .iter()
            .filter_map(|f| f.link_lib.as_deref())
            .chain(mir.globals.iter().filter_map(|g| g.link_lib.as_deref()))
            .map(str::to_string)
            .collect();
        link_libraries.sort_unstable();
        link_libraries.dedup();

        Ok(
            GeneratedCode::new(OutputFormat::CSource, self.output.as_bytes().to_vec())
                .with_link_libraries(link_libraries),
        )
    }

    fn target(&self) -> Target {
        Target::C
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::builder::{values, MirBuilder, MirModuleBuilder};
    use std::sync::Arc;

    // =========================================================================
    // C BACKEND TESTS
    // =========================================================================

    #[test]
    fn test_c_backend_simple() {
        let mut module_builder = MirModuleBuilder::new("test");

        // Build: fn add(a: i32, b: i32) -> i32 { a + b }
        let sig = MirFnSig::new(vec![MirType::i32(), MirType::i32()], MirType::i32());
        let mut builder = MirBuilder::new("add", sig);

        let a = builder.param_local(0);
        let b = builder.param_local(1);
        let result = builder.create_local(MirType::i32());

        builder.binary_op(result, BinOp::Add, values::local(a), values::local(b));
        builder.ret(Some(values::local(result)));

        module_builder.add_function(builder.build());
        let module = module_builder.build();

        let mut backend = CBackend::new();
        let output = backend.generate(&module).unwrap();

        let code = output.as_string().unwrap();
        assert!(code.contains("int32_t add("));
        assert!(code.contains("return"));
    }

    #[test]
    fn test_c_backend_void_function() {
        let mut module_builder = MirModuleBuilder::new("test");

        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut builder = MirBuilder::new("noop", sig);
        builder.ret_void();

        module_builder.add_function(builder.build());
        let module = module_builder.build();

        let mut backend = CBackend::new();
        let output = backend.generate(&module).unwrap();

        let code = output.as_string().unwrap();
        assert!(code.contains("void noop(void)"));
        assert!(code.contains("return;"));
    }

    #[test]
    fn test_c_backend_global_variable() {
        let mut module_builder = MirModuleBuilder::new("test");

        let mut global = MirGlobal::new("MY_CONST", MirType::i32());
        global.init = Some(MirConst::Int(42, MirType::i32()));
        module_builder.add_global(global);

        // Add a dummy function so we have something to generate
        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut builder = MirBuilder::new("main", sig);
        builder.ret_void();
        module_builder.add_function(builder.build());

        let module = module_builder.build();

        let mut backend = CBackend::new();
        let output = backend.generate(&module).unwrap();

        let code = output.as_string().unwrap();
        assert!(code.contains("MY_CONST"));
    }

    #[test]
    fn test_c_backend_string_table() {
        let mut module_builder = MirModuleBuilder::new("test");

        module_builder.intern_string("hello");
        module_builder.intern_string("world");

        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut builder = MirBuilder::new("main", sig);
        builder.ret_void();
        module_builder.add_function(builder.build());

        let module = module_builder.build();

        let mut backend = CBackend::new();
        let output = backend.generate(&module).unwrap();

        let code = output.as_string().unwrap();
        assert!(code.contains("__str0"));
        assert!(code.contains("hello"));
    }

    #[test]
    fn test_c_backend_branching() {
        let mut module_builder = MirModuleBuilder::new("test");

        let sig = MirFnSig::new(vec![MirType::Bool], MirType::i32());
        let mut builder = MirBuilder::new("branch_test", sig);

        let cond = builder.param_local(0);
        let then_block = builder.create_block();
        let else_block = builder.create_block();

        builder.branch(values::local(cond), then_block, else_block);

        builder.switch_to_block(then_block);
        builder.ret(Some(values::i32(1)));

        builder.switch_to_block(else_block);
        builder.ret(Some(values::i32(0)));

        module_builder.add_function(builder.build());
        let module = module_builder.build();

        let mut backend = CBackend::new();
        let output = backend.generate(&module).unwrap();

        let code = output.as_string().unwrap();
        assert!(code.contains("if ("));
        assert!(code.contains("goto"));
    }

    #[test]
    fn test_c_backend_all_binary_ops() {
        let mut module_builder = MirModuleBuilder::new("test");

        let ops = [
            BinOp::Add,
            BinOp::Sub,
            BinOp::Mul,
            BinOp::Div,
            BinOp::Rem,
            BinOp::BitAnd,
            BinOp::BitOr,
            BinOp::BitXor,
            BinOp::Eq,
            BinOp::Ne,
            BinOp::Lt,
            BinOp::Le,
            BinOp::Gt,
            BinOp::Ge,
        ];

        for (i, op) in ops.iter().enumerate() {
            let sig = MirFnSig::new(vec![MirType::i32(), MirType::i32()], MirType::i32());
            let mut builder = MirBuilder::new(format!("op_{}", i), sig);

            let a = builder.param_local(0);
            let b = builder.param_local(1);
            let result = builder.create_local(MirType::i32());

            builder.binary_op(result, *op, values::local(a), values::local(b));
            builder.ret(Some(values::local(result)));

            module_builder.add_function(builder.build());
        }

        let module = module_builder.build();

        let mut backend = CBackend::new();
        let output = backend.generate(&module).unwrap();
        let code = output.as_string().unwrap();

        // Verify it generated without errors
        assert!(code.contains("op_0"));
        assert!(code.contains("op_13"));
    }

    #[test]
    fn test_c_backend_type_to_c() {
        let backend = CBackend::new();

        assert_eq!(backend.type_to_c(&MirType::Void), "void");
        assert_eq!(backend.type_to_c(&MirType::Bool), "bool");
        assert_eq!(backend.type_to_c(&MirType::i8()), "int8_t");
        assert_eq!(backend.type_to_c(&MirType::u8()), "uint8_t");
        assert_eq!(backend.type_to_c(&MirType::i32()), "int32_t");
        assert_eq!(backend.type_to_c(&MirType::u32()), "uint32_t");
        assert_eq!(backend.type_to_c(&MirType::i64()), "int64_t");
        assert_eq!(backend.type_to_c(&MirType::f32()), "float");
        assert_eq!(backend.type_to_c(&MirType::f64()), "double");
        assert_eq!(backend.type_to_c(&MirType::isize()), "intptr_t");
        assert_eq!(backend.type_to_c(&MirType::usize()), "uintptr_t");
    }

    #[test]
    fn test_c_backend_struct_type() {
        let mut module_builder = MirModuleBuilder::new("test");

        module_builder.create_struct(
            "Point",
            vec![
                (Some(Arc::from("x")), MirType::i32()),
                (Some(Arc::from("y")), MirType::i32()),
            ],
        );

        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut builder = MirBuilder::new("main", sig);
        builder.ret_void();
        module_builder.add_function(builder.build());

        let module = module_builder.build();

        let mut backend = CBackend::new();
        let output = backend.generate(&module).unwrap();

        let code = output.as_string().unwrap();
        assert!(code.contains("typedef struct Point"));
        assert!(code.contains("int32_t x;"));
        assert!(code.contains("int32_t y;"));
    }

    #[test]
    fn test_c_backend_target() {
        let backend = CBackend::new();
        assert_eq!(backend.target(), Target::C);
    }

    fn bs(id: u32, name: &str) -> MirLocal {
        MirLocal {
            id: LocalId(id),
            name: Some(Arc::from(name)),
            ty: MirType::Struct(Arc::from("BuildString")),
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        }
    }

    fn freeable_string_local(referenced: bool) -> MirFunction {
        // A function with one non-param BuildString local defined by an
        // allocating Call (build_string_concat allocates a fresh heap buffer)
        // in the entry block. If `referenced`, the local is returned (escapes);
        // otherwise it is never used.
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(bs(0, "s"));
        let mut entry = MirBlock::new(BlockId(0));
        entry.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut exit = MirBlock::new(BlockId(1));
        exit.terminator = Some(MirTerminator::Return(if referenced {
            Some(MirValue::Local(LocalId(0)))
        } else {
            None
        }));
        func.blocks = Some(vec![entry, exit]);
        func
    }

    #[test]
    fn freeable_owned_string_local_is_detected_when_unused() {
        let backend = CBackend::new();
        let func = freeable_string_local(false);
        assert_eq!(
            backend.freeable_owned_string_locals(&func),
            vec![LocalId(0)],
            "an unused owned BuildString local defined in the entry block should be freeable"
        );
    }

    #[test]
    fn referenced_owned_string_local_is_not_freed() {
        let backend = CBackend::new();
        let func = freeable_string_local(true);
        assert!(
            backend.freeable_owned_string_locals(&func).is_empty(),
            "a returned (referenced) BuildString local must not be freed"
        );
    }

    #[test]
    fn parameter_string_local_is_not_freed() {
        // A BuildString PARAMETER is owned by the caller; the callee must never
        // free it even if it appears unused in the body.
        let backend = CBackend::new();
        let mut func = freeable_string_local(false);
        func.locals[0].is_param = true;
        assert!(
            backend.freeable_owned_string_locals(&func).is_empty(),
            "a parameter must not be freed by the callee"
        );
    }

    #[test]
    fn cap0_wrapper_local_is_not_freed() {
        // build_string_new returns a cap=0 wrapper (no heap), so it is NOT in the
        // allocating set: such a local owns nothing and must not be in the free
        // set (freeing is a no-op, but the analysis must not treat it as owned).
        let backend = CBackend::new();
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(bs(0, "s"));
        let mut entry = MirBlock::new(BlockId(0));
        entry.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_new")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut exit = MirBlock::new(BlockId(1));
        exit.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![entry, exit]);
        assert!(
            backend.freeable_owned_string_locals(&func).is_empty(),
            "a cap=0 build_string_new wrapper is not owned heap and must not be freed"
        );
    }

    #[test]
    fn move_acquired_owner_is_freed_and_source_is_not() {
        // The alias guard: `_0 = concat(...); s = _0; printf(s.ptr)`. `_0` and `s`
        // alias the same heap buffer via the move, so freeing BOTH double-frees.
        // The move marks `_0` moved-from (excluded); `s` (the sole live owner) is
        // freed. Mirrors the real lowering of `let s = a + b; println!("{}", s)`.
        let backend = CBackend::new();
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(bs(0, "_t")); // concat result
        func.locals.push(bs(1, "s")); // move-acquires the buffer
        func.locals.push(MirLocal {
            id: LocalId(2),
            name: Some(Arc::from("p")),
            ty: MirType::i64(), // a borrowed .ptr temp (non-BuildString)
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        });
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        ));
        b1.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(1)),
                field_name: Arc::from("ptr"),
                field_ty: MirType::i64(),
            },
        ));
        b1.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("printf")),
            args: vec![MirValue::Local(LocalId(2))],
            dest: None,
            target: Some(BlockId(2)),
            unwind: None,
        });
        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1, b2]);
        assert_eq!(
            backend.freeable_owned_string_locals(&func),
            vec![LocalId(1)],
            "the move destination `s` is freed; the moved-from source `_t` is not (no double-free)"
        );
    }

    #[test]
    fn multi_move_acquirers_are_not_freed() {
        // The adversarial double-free: `_0 = concat(...); c = _0; p = c; q = c`.
        // `c` is moved into BOTH `p` and `q`, so they alias one heap buffer.
        // Freeing both is a double-free; the alias guard must free NEITHER.
        let backend = CBackend::new();
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(bs(0, "_t")); // concat result
        func.locals.push(bs(1, "c"));
        func.locals.push(bs(2, "p"));
        func.locals.push(bs(3, "q"));
        func.locals.push(MirLocal {
            id: LocalId(4),
            name: Some(Arc::from("pp")),
            ty: MirType::i64(),
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        });
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Use(MirValue::Local(LocalId(0))),
        )); // c = _t
        b1.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Use(MirValue::Local(LocalId(1))),
        )); // p = c
        b1.stmts.push(MirStmt::assign(
            LocalId(3),
            MirRValue::Use(MirValue::Local(LocalId(1))),
        )); // q = c  (second acquirer of c)
        b1.stmts.push(MirStmt::assign(
            LocalId(4),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(2)),
                field_name: Arc::from("ptr"),
                field_ty: MirType::i64(),
            },
        ));
        b1.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("printf")),
            args: vec![MirValue::Local(LocalId(4))],
            dest: None,
            target: Some(BlockId(2)),
            unwind: None,
        });
        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1, b2]);
        assert!(
            backend.freeable_owned_string_locals(&func).is_empty(),
            "two move-acquirers of one buffer must not be freed (double-free guard)"
        );
    }

    #[test]
    fn mutable_global_alias_risk_disables_freeing() {
        // If the module declares a mutable global that could hold a heap string
        // alias, an owner could be stashed there (an escape the per-function scan
        // cannot see), so the whole module reclaims nothing.
        let mut backend = CBackend::new();
        backend.module_mut_global_alias_risk = true;
        let func = freeable_string_local(false);
        assert!(
            backend.freeable_owned_string_locals(&func).is_empty(),
            "a module with an aliasable mutable global must free nothing"
        );
        // Sanity: the same func IS freeable when there is no such global.
        let safe = CBackend::new();
        assert_eq!(safe.freeable_owned_string_locals(&func), vec![LocalId(0)]);
    }

    #[test]
    fn global_store_of_owner_is_an_escape() {
        // Storing an owned string into a module global is a hard escape, caught by
        // owned_string_escapes even with the module-wide guard OFF (fresh backend).
        // This is the coupling that makes narrowing the guard sound: a value
        // stashed into a global is never freed.
        let backend = CBackend::new(); // module_mut_global_alias_risk = false
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(bs(0, "c"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: vec![],
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.stmts.push(MirStmt::new(MirStmtKind::GlobalStore {
            name: Arc::from("G"),
            value: MirRValue::Use(MirValue::Local(LocalId(0))),
        }));
        b1.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1]);
        assert!(
            backend.freeable_owned_string_locals(&func).is_empty(),
            "an owner stored into a global must not be freed (escape coupling)"
        );
    }

    #[test]
    fn ty_alias_risk_classifies_scalars_safe_and_pointers_risky() {
        assert!(!CBackend::ty_can_hold_heap_string_alias(&MirType::i32()));
        assert!(!CBackend::ty_can_hold_heap_string_alias(&MirType::Bool));
        assert!(!CBackend::ty_can_hold_heap_string_alias(&MirType::f64()));
        assert!(CBackend::ty_can_hold_heap_string_alias(&MirType::Ptr(
            Box::new(MirType::i8())
        )));
        assert!(CBackend::ty_can_hold_heap_string_alias(&MirType::Struct(
            Arc::from("BuildString")
        )));
        assert!(CBackend::ty_can_hold_heap_string_alias(&MirType::Vec(
            Box::new(MirType::Struct(Arc::from("BuildString")))
        )));
    }

    #[test]
    fn alloc_defined_string_printed_is_freed() {
        // `_0 = concat(...); p = _0.ptr; printf(p)`. Single owner, borrowed only
        // by a non-retaining print, never moved or returned: freeable.
        let backend = CBackend::new();
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(bs(0, "s"));
        func.locals.push(MirLocal {
            id: LocalId(1),
            name: Some(Arc::from("p")),
            ty: MirType::i64(),
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        });
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(0)),
                field_name: Arc::from("ptr"),
                field_ty: MirType::i64(),
            },
        ));
        b1.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("printf")),
            args: vec![MirValue::Local(LocalId(1))],
            dest: None,
            target: Some(BlockId(2)),
            unwind: None,
        });
        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1, b2]);
        assert_eq!(
            backend.freeable_owned_string_locals(&func),
            vec![LocalId(0)],
            "an allocated string only read by a non-retaining print is freeable"
        );
    }

    #[test]
    fn string_stored_into_aggregate_is_not_freed() {
        // `_0 = concat(...); _1 = (_0)` stores the owned string into an aggregate
        // that outlives the local: it escapes and must NOT be freed.
        let backend = CBackend::new();
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(bs(0, "s"));
        func.locals.push(MirLocal {
            id: LocalId(1),
            name: Some(Arc::from("tup")),
            ty: MirType::Struct(Arc::from("Tup")),
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        });
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::Aggregate {
                kind: AggregateKind::Tuple,
                operands: vec![MirValue::Local(LocalId(0))],
            },
        ));
        b1.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1]);
        assert!(
            backend.freeable_owned_string_locals(&func).is_empty(),
            "a string stored into an aggregate escapes and must not be freed"
        );
    }

    #[test]
    fn escaping_ptr_temp_blocks_free() {
        // `_0 = concat(...); p = _0.ptr; return p`. The borrowed pointer escapes
        // (returned), so freeing `_0` would dangle it: `_0` must NOT be freed.
        let backend = CBackend::new();
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::i64()));
        func.locals.push(bs(0, "s"));
        func.locals.push(MirLocal {
            id: LocalId(1),
            name: Some(Arc::from("p")),
            ty: MirType::i64(),
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        });
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(0)),
                field_name: Arc::from("ptr"),
                field_ty: MirType::i64(),
            },
        ));
        b1.terminator = Some(MirTerminator::Return(Some(MirValue::Local(LocalId(1)))));
        func.blocks = Some(vec![b0, b1]);
        assert!(
            backend.freeable_owned_string_locals(&func).is_empty(),
            "an escaping borrowed .ptr temp must block freeing its owner"
        );
    }

    fn i64_local(id: u32, name: &str) -> MirLocal {
        MirLocal {
            id: LocalId(id),
            name: Some(Arc::from(name)),
            ty: MirType::i64(),
            is_mut: false,
            is_param: false,
            annotations: Vec::new(),
        }
    }

    // bb0 `if c -> bb1/bb4`; bb1 `concat -> _1 -> bb2`; bb2 `s=_1; p=s.ptr; <term>`;
    // bb3; bb4 `goto bb5`; bb5 `return`. `s` (local 2) is never function-exit
    // freeable (bb2 does not dominate the return: bb5 is also reachable via bb4).
    fn confined_owner_func(borrow_escapes: bool) -> MirFunction {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(MirLocal {
            id: LocalId(0),
            name: Some(Arc::from("c")),
            ty: MirType::Bool,
            is_mut: false,
            is_param: true,
            annotations: Vec::new(),
        });
        func.locals.push(bs(1, "_t"));
        func.locals.push(bs(2, "s"));
        func.locals.push(i64_local(3, "p"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(0)),
            then_block: BlockId(1),
            else_block: BlockId(4),
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: vec![],
            dest: Some(LocalId(1)),
            target: Some(BlockId(2)),
            unwind: None,
        });
        let mut b2 = MirBlock::new(BlockId(2));
        b2.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Use(MirValue::Local(LocalId(1))),
        ));
        b2.stmts.push(MirStmt::assign(
            LocalId(3),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(2)),
                field_name: Arc::from("ptr"),
                field_ty: MirType::i64(),
            },
        ));
        let mut b3 = MirBlock::new(BlockId(3));
        if borrow_escapes {
            b2.terminator = Some(MirTerminator::Goto(BlockId(3)));
            b3.terminator = Some(MirTerminator::Call {
                func: MirValue::Function(Arc::from("printf")),
                args: vec![MirValue::Local(LocalId(3))],
                dest: None,
                target: Some(BlockId(5)),
                unwind: None,
            });
        } else {
            b2.terminator = Some(MirTerminator::Call {
                func: MirValue::Function(Arc::from("printf")),
                args: vec![MirValue::Local(LocalId(3))],
                dest: None,
                target: Some(BlockId(3)),
                unwind: None,
            });
            b3.terminator = Some(MirTerminator::Goto(BlockId(5)));
        }
        let mut b4 = MirBlock::new(BlockId(4));
        b4.terminator = Some(MirTerminator::Goto(BlockId(5)));
        let mut b5 = MirBlock::new(BlockId(5));
        b5.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1, b2, b3, b4, b5]);
        func
    }

    #[test]
    fn block_scoped_frees_confined_owner_at_successor_start() {
        let backend = CBackend::new();
        let func = confined_owner_func(false);
        let fn_exit = backend.freeable_owned_string_locals(&func);
        assert!(
            !fn_exit.contains(&LocalId(2)),
            "s must not be function-exit freeable: its block does not dominate the return"
        );
        let map = backend.block_scoped_freeable(&func, &fn_exit);
        assert_eq!(
            map.get(&3).map(|v| v.as_slice()),
            Some(&[LocalId(2)][..]),
            "confined owner `s` must be freed at the start of bb3: {map:?}"
        );
    }

    #[test]
    fn block_scoped_skips_owner_whose_borrow_escapes_its_block() {
        let backend = CBackend::new();
        let func = confined_owner_func(true);
        let fn_exit = backend.freeable_owned_string_locals(&func);
        let map = backend.block_scoped_freeable(&func, &fn_exit);
        assert!(
            map.values().all(|v| !v.contains(&LocalId(2))),
            "an owner whose .ptr borrow outlives its block must not be block-scoped freed: {map:?}"
        );
    }

    #[test]
    fn conditional_alloc_not_dominating_return_is_not_freed() {
        // `_0` is allocated only on the then-branch, which does not dominate the
        // return, so `_0` is not definitely initialized at the free site: skip it.
        let backend = CBackend::new();
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(MirLocal {
            id: LocalId(1),
            name: Some(Arc::from("c")),
            ty: MirType::Bool,
            is_mut: false,
            is_param: true,
            annotations: Vec::new(),
        });
        func.locals.push(bs(0, "s"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(1)),
            then_block: BlockId(1),
            else_block: BlockId(2),
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(2)),
            unwind: None,
        });
        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1, b2]);
        assert!(
            backend.freeable_owned_string_locals(&func).is_empty(),
            "a conditionally-allocated string is not definitely initialized at return"
        );
    }

    // Shape exercising increment 4 specifically (disjoint from increments 1-3
    // through the REAL backend helpers, not just the standalone analysis call):
    // bb0: if c -> bb1 else bb4
    // bb1: _0 = alloc() -> bb2                    (owner defined bb1, NOT bb0)
    // bb2: _1 = _0.ptr ; printf(_1) -> bb3         (owner used bb2, not bb1)
    // bb3: return
    // bb4: return                                  (a return NOT dominated by bb1)
    //
    // fn_exit (increments 1-2) declines `_0`: bb4 is a sibling return not
    // dominated by bb1, so `_0` is not definite-init at every return.
    // block-scoped (increment 3) declines `_0` too: `live_range_confined_to_block`
    // requires the `.ptr` borrow confined to the DEFINING block (bb1), but the
    // borrow happens in bb2, so confinement fails.
    // Only increment 4 catches it: the buffer dies at bb3's entry, bb2 is the
    // sole (terminal) predecessor of bb3, and bb1 (the def block) dominates bb3
    // -> free `_0` at the start of bb3.
    fn multi_block_owner_func() -> MirFunction {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(MirLocal {
            id: LocalId(9),
            name: Some(Arc::from("c")),
            ty: MirType::Bool,
            is_mut: false,
            is_param: true,
            annotations: Vec::new(),
        });
        func.locals.push(bs(0, "_0"));
        func.locals.push(i64_local(1, "_1"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(9)),
            then_block: BlockId(1),
            else_block: BlockId(4),
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(2)),
            unwind: None,
        });
        let mut b2 = MirBlock::new(BlockId(2));
        b2.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(0)),
                field_name: Arc::from("ptr"),
                field_ty: MirType::i64(),
            },
        ));
        b2.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("printf")),
            args: vec![MirValue::Local(LocalId(1))],
            dest: None,
            target: Some(BlockId(3)),
            unwind: None,
        });
        let mut b3 = MirBlock::new(BlockId(3));
        b3.terminator = Some(MirTerminator::Return(None));
        let mut b4 = MirBlock::new(BlockId(4));
        b4.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1, b2, b3, b4]);
        func
    }

    #[test]
    fn increment4_multi_block_owner_is_scheduled_for_free() {
        let backend = CBackend::new();
        let func = multi_block_owner_func();
        let fn_exit_vec = backend.freeable_owned_string_locals(&func);
        assert!(
            !fn_exit_vec.contains(&LocalId(0)),
            "_0 must NOT be fn-exit freeable: bb1 does not dominate the sibling return bb4: {fn_exit_vec:?}"
        );
        let fn_exit: std::collections::HashSet<LocalId> = fn_exit_vec.into_iter().collect();
        let bscoped_map =
            backend.block_scoped_freeable(&func, &backend.freeable_owned_string_locals(&func));
        let bscoped: std::collections::HashSet<LocalId> =
            bscoped_map.values().flatten().copied().collect();
        assert!(
            !bscoped.contains(&LocalId(0)),
            "_0 must NOT be block-scoped freeable either, so increment 4 is what claims it: {bscoped_map:?}"
        );
        let candidates = backend.sound_owned_candidates(&func);
        let extra = crate::codegen::analysis::drops::multi_block_freeable(
            &func,
            &candidates,
            &fn_exit,
            &bscoped,
        );
        assert_eq!(
            extra.get(&3).map(|v| v.as_slice()),
            Some(&[LocalId(0)][..]),
            "increment-4 must schedule _0 to be freed at the start of bb3: {extra:?}"
        );
    }

    // Multi-hop escape guard: owner `_0` is allocated, then its `.ptr` is copied
    // multi-hop (`_1 = _0.ptr; _2 = _1;`), re-mentioning `_1` outside a direct
    // borrow-call argument position. `owned_string_escapes` must reject `_0` via
    // `ptr_temp_escapes` (the `_2 = _1` assign mentions `_1`), so
    // `sound_owned_candidates` must NOT include `_0`, and therefore
    // `multi_block_freeable` must never schedule it for a free. This is the
    // end-to-end proof that a multi-hop-aliased owner is never freed.
    fn multi_hop_escaping_owner_func() -> MirFunction {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(bs(0, "_0"));
        func.locals.push(i64_local(1, "_1"));
        func.locals.push(i64_local(2, "_2"));
        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(1)),
            unwind: None,
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(0)),
                field_name: Arc::from("ptr"),
                field_ty: MirType::i64(),
            },
        ));
        // Multi-hop copy: `_2 = _1`. `_1` is re-mentioned by this rvalue, so
        // `ptr_temp_escapes(_1, ..)` is true, so `_0` escapes.
        b1.stmts.push(MirStmt::assign(
            LocalId(2),
            MirRValue::Use(MirValue::Local(LocalId(1))),
        ));
        b1.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("printf")),
            args: vec![MirValue::Local(LocalId(2))],
            dest: None,
            target: Some(BlockId(2)),
            unwind: None,
        });
        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1, b2]);
        func
    }

    #[test]
    fn multi_hop_ptr_copy_owner_is_excluded_from_sound_candidates_and_never_freed() {
        let backend = CBackend::new();
        let func = multi_hop_escaping_owner_func();
        let candidates = backend.sound_owned_candidates(&func);
        assert!(
            !candidates.iter().any(|(id, _)| *id == LocalId(0)),
            "a multi-hop-aliased owner must be excluded from sound_owned_candidates: {candidates:?}"
        );
        let extra = crate::codegen::analysis::drops::multi_block_freeable(
            &func,
            &candidates,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        );
        assert!(
            extra.is_empty(),
            "a multi-hop-aliased owner must never be scheduled for a free: {extra:?}"
        );
        // Increment 5: a multi-hop-aliased owner is excluded from
        // `sound_owned_candidates` (asserted above), so it can never even be
        // OFFERED to `split_frontier_flag_frees` in production -- but confirm
        // the function is inert on an empty candidate list too (defense in
        // depth: the escape gate is upstream of every drop mechanism).
        let flag_frees = crate::codegen::analysis::flags::split_frontier_flag_frees(
            &func,
            &candidates,
            &std::collections::HashSet::new(),
        );
        assert!(
            !flag_frees.owners.contains(&LocalId(0)),
            "a multi-hop-aliased owner must never be enrolled as a flag owner: {:?}",
            flag_frees.owners
        );
    }

    // Increment 5: exact-text emission test for `emit_flag_guarded_free`.
    // Pins the guard/free/clear text (all on the owner's real local name) so
    // a change to the emitted C is caught here, not just by the C-diff
    // fixture evidence.
    #[test]
    fn emit_flag_guarded_free_produces_exact_guard_text() {
        let mut backend = CBackend::new();
        let locals = vec![bs(0, "s")];
        backend.emit_flag_guarded_free(LocalId(0), &locals);
        assert_eq!(
            backend.output,
            "if (__bl_live_0) { build_string_free(s); __bl_live_0 = 0; }\n"
        );
    }

    // Increment 5, w5-review.md Important finding 2: the exact-text test
    // above pins ONLY `emit_flag_guarded_free`'s guard/free/clear text. It
    // does not touch the flag DECLARATION (c.rs ~line 2056, `uint8_t
    // __bl_live_N = 0;`), the def-site `= 1;` set, or the fact that the
    // Return backstop specifically (not just any frontier site) emits the
    // same guard text. Before this test, only the manual C-diff protocol
    // recorded in `docs/MEMORY-PILLAR-DESIGN.md` pinned those three
    // emissions -- so the proven memory-corruption mutation (flip the
    // declaration's `= 0` to `= 1`; w5-report.md Task 6 mutation 6, ASan
    // bad-free) was invisible to `cargo test`. This test compiles the real
    // shipped fixture `compiler/tests/mem/split_frontier_loop.bld` through
    // the actual front-end pipeline (lex/parse/typecheck/lower, the same
    // steps `buildc build` runs) and then through the real
    // `CBackend::generate_function` path, with `BUILDLANG_EXPERIMENTAL_FREE`
    // forced on via the test-only override -- never a process env var
    // (unsafe under parallel `cargo test`; see the "Test idiom note" in
    // `docs/superpowers/plans/2026-07-29-drop-flags.md`).
    #[test]
    fn real_fixture_pins_flag_declaration_set_and_return_backstop() {
        let source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/mem/split_frontier_loop.bld"
        ));
        let source_file = crate::lexer::SourceFile::new("split_frontier_loop.bld", source);
        let mut lexer = crate::lexer::Lexer::new(&source_file);
        let tokens = lexer
            .tokenize()
            .expect("lexing the real fixture should succeed");
        let mut parser = crate::parser::Parser::new(&source_file, tokens);
        let ast = parser
            .parse()
            .expect("parsing the real fixture should succeed");
        assert!(
            parser.errors().is_empty(),
            "unexpected parser errors: {:?}",
            parser.errors()
        );

        let mut ctx = crate::types::TypeContext::new();
        let mut checker = crate::types::TypeChecker::new(&mut ctx);
        checker.set_source_file(&source_file);
        checker.check_module(&ast);
        assert!(
            !checker.has_errors(),
            "unexpected type errors: {:?}",
            checker.errors()
        );

        let mir =
            crate::codegen::lower::MirLowerer::with_source(&ctx, Arc::from(source_file.source()))
                .lower_module(&ast)
                .expect("lowering the real fixture should succeed");

        let mut backend = CBackend::new();
        backend.set_experimental_free_enabled_for_test(true);
        let output = backend.generate(&mir).expect("C generation should succeed");
        let code = output.as_string().expect("C output should be a string");

        // Exactly one flag owner (`s`) in this fixture: locate its
        // declaration and extract the LocalId suffix. The suffix itself is
        // allowed to drift with unrelated lowering changes; the exact text
        // around it is what this test pins.
        let decl_marker = "uint8_t __bl_live_";
        assert_eq!(
            code.matches(decl_marker).count(),
            1,
            "this fixture allocates exactly one flag owner (`s`); a different count means the fixture's shape changed:\n{code}"
        );
        let decl_start = code.find(decl_marker).unwrap();
        let after_prefix = &code[decl_start + decl_marker.len()..];
        let digits_len = after_prefix
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .count();
        assert!(
            digits_len > 0,
            "expected a numeric LocalId after __bl_live_: {after_prefix}"
        );
        let owner_id = &after_prefix[..digits_len];

        // 1) The declaration, initialized to 0 (c.rs ~line 2056). This is
        //    the exact text the initializer-flip mutation corrupts by
        //    changing `= 0` to `= 1`.
        let decl_line = format!("uint8_t __bl_live_{owner_id} = 0;");
        assert!(
            code.contains(&decl_line),
            "expected the flag declaration initialized to 0:\n{decl_line}\nfull output:\n{code}"
        );

        // 2) The def-site set, immediately after the owner's unique
        //    move-acquire (`s = _N;`) -- c.rs's Assign arm (~line 2717).
        let set_line = format!("__bl_live_{owner_id} = 1;");
        assert!(
            code.contains(&set_line),
            "expected the def-site flag set:\n{set_line}\nfull output:\n{code}"
        );

        // 3) The guarded free/clear text. Both arm-continuation frontier
        //    sites (bb10, bb11 per the design doc's recorded C-diff) and the
        //    Return backstop emit this exact text (the guard is what makes a
        //    later hit on the same path a safe no-op) -- three occurrences.
        let guard_line = format!(
            "if (__bl_live_{owner_id}) {{ build_string_free(s); __bl_live_{owner_id} = 0; }}"
        );
        let guard_occurrences = code.matches(guard_line.as_str()).count();
        assert_eq!(
            guard_occurrences, 3,
            "expected 3 occurrences of the guarded free/clear (two frontier sites + the Return backstop), found {guard_occurrences}:\n{guard_line}\nfull output:\n{code}"
        );

        // The Return backstop specifically sits directly before
        // `fflush(stdout);` (c.rs ~line 3470-3476), distinguishing it from
        // the two frontier-site occurrences above.
        let lines: Vec<&str> = code.lines().collect();
        let fflush_idx = lines
            .iter()
            .position(|l| l.trim() == "fflush(stdout);")
            .expect("expected an fflush(stdout); line at the fixture's single return");
        assert!(
            fflush_idx > 0 && lines[fflush_idx - 1].trim() == guard_line,
            "expected the guarded Return backstop directly before fflush(stdout); got: {:?}\nfull output:\n{code}",
            lines.get(fflush_idx.wrapping_sub(1))
        );
    }

    // Increment 5 disjointness: a fn with a CONDITIONALLY ALLOCATED owner
    // (`analysis::flags`'s `conditional_alloc_enrolls_with_no_dominance`
    // shape: the allocation itself lives inside one arm of an `if`, so the
    // def block does NOT dominate the join/return) must yield an owner set
    // from `split_frontier_flag_frees` that is disjoint from every prior
    // increment's set on the SAME candidate list. This is deliberately NOT
    // the unconditional-allocation split-frontier shape
    // (`drops.rs::declines_split_death_frontier`): on that CFG the def block
    // DOES dominate the single return, so `freeable_owned_string_locals`
    // (fn-exit, which only requires definite-init + no escape, not uniform
    // per-arm USE) legitimately claims the owner -- proving fn-exit's rule
    // is about ALLOCATION being definite, not about the buffer being used on
    // every arm. Conditional allocation is the shape that defeats fn-exit's
    // dominance check outright.
    // bb0: if cond -> bb1 else bb2
    // bb1: _0 = alloc() -> bb1b
    // bb1b: _1 = _0.ptr ; printf(_1) -> bb3     (used on this arm)
    // bb2: -> bb3                                (never allocated on this arm)
    // bb3: return                                (join; def does not dominate it)
    fn conditional_alloc_owner_func() -> MirFunction {
        let mut func = MirFunction::new("f", MirFnSig::new(vec![], MirType::Void));
        func.locals.push(i64_local(9, "cond"));
        func.locals.push(bs(0, "_0"));
        func.locals.push(i64_local(1, "_1"));

        let mut b0 = MirBlock::new(BlockId(0));
        b0.terminator = Some(MirTerminator::If {
            cond: MirValue::Local(LocalId(9)),
            then_block: BlockId(1),
            else_block: BlockId(2),
        });
        let mut b1 = MirBlock::new(BlockId(1));
        b1.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("build_string_concat")),
            args: Vec::new(),
            dest: Some(LocalId(0)),
            target: Some(BlockId(4)),
            unwind: None,
        });
        let mut b1b = MirBlock::new(BlockId(4));
        b1b.stmts.push(MirStmt::assign(
            LocalId(1),
            MirRValue::FieldAccess {
                base: MirValue::Local(LocalId(0)),
                field_name: Arc::from("ptr"),
                field_ty: MirType::i64(),
            },
        ));
        b1b.terminator = Some(MirTerminator::Call {
            func: MirValue::Function(Arc::from("printf")),
            args: vec![MirValue::Local(LocalId(1))],
            dest: None,
            target: Some(BlockId(3)),
            unwind: None,
        });
        let mut b2 = MirBlock::new(BlockId(2));
        b2.terminator = Some(MirTerminator::Goto(BlockId(3)));
        let mut b3 = MirBlock::new(BlockId(3));
        b3.terminator = Some(MirTerminator::Return(None));
        func.blocks = Some(vec![b0, b1, b1b, b2, b3]);
        func
    }

    #[test]
    fn increment5_owner_set_is_disjoint_from_increments_1_4() {
        let backend = CBackend::new();
        let func = conditional_alloc_owner_func();

        let fn_exit = backend.freeable_owned_string_locals(&func);
        assert!(
            !fn_exit.contains(&LocalId(0)),
            "fn-exit must decline a conditionally allocated owner (def does not dominate the return): {fn_exit:?}"
        );
        let fn_exit_set: std::collections::HashSet<LocalId> = fn_exit.into_iter().collect();

        let block_scoped = backend.block_scoped_freeable(&func, &[]);
        assert!(
            block_scoped.values().flatten().all(|id| *id != LocalId(0)),
            "block-scoped must decline (live range spans blocks): {block_scoped:?}"
        );
        let block_scoped_set: std::collections::HashSet<LocalId> =
            block_scoped.values().flatten().copied().collect();

        let candidates = backend.sound_owned_candidates(&func);
        let multi_block = crate::codegen::analysis::drops::multi_block_freeable(
            &func,
            &candidates,
            &fn_exit_set,
            &block_scoped_set,
        );
        assert!(
            multi_block.values().flatten().all(|id| *id != LocalId(0)),
            "increment 4 must decline (its site rule requires def to dominate the site): {multi_block:?}"
        );
        let multi_block_set: std::collections::HashSet<LocalId> =
            multi_block.values().flatten().copied().collect();

        let claimed: std::collections::HashSet<LocalId> = fn_exit_set
            .iter()
            .copied()
            .chain(block_scoped_set.iter().copied())
            .chain(multi_block_set.iter().copied())
            .collect();
        let flag_frees = crate::codegen::analysis::flags::split_frontier_flag_frees(
            &func,
            &candidates,
            &claimed,
        );
        assert_eq!(
            flag_frees.owners,
            vec![LocalId(0)],
            "increment 5 must claim the owner every prior increment declined: {:?}",
            flag_frees.owners
        );
        assert_eq!(
            flag_frees.block_frees.get(&3).map(|v| v.as_slice()),
            Some(&[LocalId(0)][..]),
            "the guarded site lands at the join bb3, with no dominance requirement: {:?}",
            flag_frees.block_frees
        );

        // Pairwise disjoint: the same owner never appears in two sets.
        assert!(!fn_exit_set.contains(&LocalId(0)));
        assert!(!block_scoped_set.contains(&LocalId(0)));
        assert!(!multi_block_set.contains(&LocalId(0)));
        assert!(flag_frees.owners.contains(&LocalId(0)));
    }

    #[test]
    fn test_c_backend_includes() {
        let module_builder = MirModuleBuilder::new("test");
        let module = module_builder.build();

        let mut backend = CBackend::new();
        let output = backend.generate(&module).unwrap();

        let code = output.as_string().unwrap();
        assert!(code.contains("#include <stdint.h>"));
        assert!(code.contains("#include <stdbool.h>"));
        assert!(code.contains("#include <stddef.h>"));
        assert!(code.contains("#include <math.h>"));
        // Runtime library should be embedded
        assert!(code.contains("BuildString"));
        assert!(code.contains("BuildVec"));
    }

    #[test]
    fn test_c_backend_struct_field_access() {
        let mut module_builder = MirModuleBuilder::new("test");

        module_builder.create_struct(
            "Point",
            vec![
                (Some(Arc::from("x")), MirType::i32()),
                (Some(Arc::from("y")), MirType::i32()),
            ],
        );

        let sig = MirFnSig::new(vec![MirType::Struct(Arc::from("Point"))], MirType::i32());
        let mut builder = MirBuilder::new("get_x", sig);
        builder.set_param_name(0, "p");

        let result = builder.create_local(MirType::i32());
        builder.assign(
            result,
            MirRValue::FieldAccess {
                base: values::local(LocalId(0)),
                field_name: Arc::from("x"),
                field_ty: MirType::i32(),
            },
        );
        builder.ret(Some(values::local(result)));

        module_builder.add_function(builder.build());
        let module = module_builder.build();

        let mut backend = CBackend::new();
        let output = backend.generate(&module).unwrap();

        let code = output.as_string().unwrap();
        assert!(code.contains("p.x"), "Expected 'p.x' in:\n{}", code);
        assert!(code.contains("Point p"), "Expected 'Point p' in:\n{}", code);
    }

    #[test]
    fn test_c_backend_struct_aggregate() {
        let mut module_builder = MirModuleBuilder::new("test");

        module_builder.create_struct(
            "Point",
            vec![
                (Some(Arc::from("x")), MirType::i32()),
                (Some(Arc::from("y")), MirType::i32()),
            ],
        );

        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut builder = MirBuilder::new("main", sig);

        let p = builder.create_named_local(Arc::from("p"), MirType::Struct(Arc::from("Point")));
        builder.aggregate(
            p,
            AggregateKind::Struct(Arc::from("Point")),
            vec![values::i32(3), values::i32(4)],
        );
        builder.ret_void();

        module_builder.add_function(builder.build());
        let module = module_builder.build();

        let mut backend = CBackend::new();
        let output = backend.generate(&module).unwrap();

        let code = output.as_string().unwrap();
        assert!(
            code.contains("(Point){ 3, 4 }"),
            "Expected struct aggregate in:\n{}",
            code
        );
    }

    #[test]
    fn test_c_backend_enum_type() {
        let mut module_builder = MirModuleBuilder::new("test");

        module_builder.create_enum(
            "Shape",
            MirType::i32(),
            vec![
                MirEnumVariant {
                    name: Arc::from("Circle"),
                    discriminant: 0,
                    fields: vec![(None, MirType::f64())],
                },
                MirEnumVariant {
                    name: Arc::from("Rectangle"),
                    discriminant: 1,
                    fields: vec![(None, MirType::f64()), (None, MirType::f64())],
                },
            ],
        );

        let sig = MirFnSig::new(vec![], MirType::Void);
        let mut builder = MirBuilder::new("main", sig);
        builder.ret_void();

        module_builder.add_function(builder.build());
        let module = module_builder.build();

        let mut backend = CBackend::new();
        let output = backend.generate(&module).unwrap();

        let code = output.as_string().unwrap();
        assert!(
            code.contains("Shape_Circle = 0"),
            "Expected enum tag in:\n{}",
            code
        );
        assert!(
            code.contains("Shape_Rectangle = 1"),
            "Expected enum tag in:\n{}",
            code
        );
        assert!(
            code.contains("Shape_Tag"),
            "Expected tag type in:\n{}",
            code
        );
        assert!(
            code.contains("typedef struct Shape Shape;"),
            "Expected forward declaration in:\n{}",
            code
        );
        assert!(
            code.contains("struct Shape {"),
            "Expected struct definition in:\n{}",
            code
        );
    }

    #[test]
    fn test_c_backend_variant_field_access() {
        let mut module_builder = MirModuleBuilder::new("test");

        module_builder.create_enum(
            "Shape",
            MirType::i32(),
            vec![MirEnumVariant {
                name: Arc::from("Circle"),
                discriminant: 0,
                fields: vec![(None, MirType::f64())],
            }],
        );

        let sig = MirFnSig::new(vec![MirType::Struct(Arc::from("Shape"))], MirType::f64());
        let mut builder = MirBuilder::new("get_radius", sig);
        builder.set_param_name(0, "s");

        let result = builder.create_local(MirType::f64());
        builder.assign(
            result,
            MirRValue::VariantField {
                base: values::local(LocalId(0)),
                variant_name: Arc::from("Circle"),
                field_index: 0,
                field_ty: MirType::f64(),
            },
        );
        builder.ret(Some(values::local(result)));

        module_builder.add_function(builder.build());
        let module = module_builder.build();

        let mut backend = CBackend::new();
        let output = backend.generate(&module).unwrap();

        let code = output.as_string().unwrap();
        assert!(
            code.contains("s.data.Circle.f0"),
            "Expected variant field access in:\n{}",
            code
        );
    }

    #[test]
    fn test_c_backend_escape_string() {
        let backend = CBackend::new();

        assert_eq!(backend.escape_string("hello"), "hello");
        assert_eq!(backend.escape_string("hello\nworld"), "hello\\nworld");
        assert_eq!(backend.escape_string("tab\there"), "tab\\there");
        assert_eq!(backend.escape_string("quote\"here"), "quote\\\"here");
        assert_eq!(backend.escape_string("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn test_e2e_struct_codegen() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn get_x(p: Point) -> i32 {
    p.x
}

fn main() {
    let p = Point { x: 3, y: 4 };
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen.generate(&module).expect("Failed to generate");
        let code = output.as_string().unwrap();

        // Verify struct type definition (forward decl + definition)
        assert!(
            code.contains("struct Point {"),
            "Missing struct definition in:\n{}",
            code
        );
        assert!(code.contains("int32_t x;"), "Missing field x in:\n{}", code);
        assert!(code.contains("int32_t y;"), "Missing field y in:\n{}", code);
        assert!(
            code.contains("};") && code.contains("struct Point {"),
            "Missing struct definition in:\n{}",
            code
        );

        // Verify struct literal
        assert!(
            code.contains("(Point){ 3, 4 }"),
            "Missing struct literal in:\n{}",
            code
        );

        // Verify field access
        assert!(code.contains("p.x"), "Missing field access in:\n{}", code);

        // Verify function signature uses struct type
        assert!(
            code.contains("Point p"),
            "Missing struct param in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_enum_codegen() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
enum Shape {
    Circle(f64),
    Rect(f64, f64),
}

fn area(s: Shape) -> f64 {
    match s {
        Shape::Circle(r) => 3.14 * r * r,
        Shape::Rect(w, h) => w * h,
    }
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen.generate(&module).expect("Failed to generate");
        let code = output.as_string().unwrap();

        // Verify enum type definition
        assert!(
            code.contains("Shape_Circle = 0"),
            "Missing Circle tag in:\n{}",
            code
        );
        assert!(
            code.contains("Shape_Rect = 1"),
            "Missing Rect tag in:\n{}",
            code
        );
        assert!(code.contains("Shape_Tag"), "Missing tag type in:\n{}", code);
        assert!(
            code.contains("typedef struct Shape Shape;"),
            "Missing enum forward decl in:\n{}",
            code
        );
        assert!(
            code.contains("struct Shape {"),
            "Missing enum struct body in:\n{}",
            code
        );
        assert!(code.contains("union"), "Missing union in:\n{}", code);

        // Verify variant field access in match
        assert!(
            code.contains(".data.Circle.f0"),
            "Missing variant field access in:\n{}",
            code
        );
        assert!(
            code.contains(".data.Rect.f0"),
            "Missing Rect field0 access in:\n{}",
            code
        );
        assert!(
            code.contains(".data.Rect.f1"),
            "Missing Rect field1 access in:\n{}",
            code
        );

        // Verify tag comparison
        assert!(code.contains(".tag"), "Missing tag access in:\n{}", code);
    }

    #[test]
    fn test_e2e_ref_self_method() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn new(x: f64, y: f64) -> Self {
        Point { x: x, y: y }
    }

    fn magnitude(&self) -> f64 {
        sqrt(self.x * self.x + self.y * self.y)
    }

    fn scale(&mut self, factor: f64) {
        self.x = self.x * factor;
        self.y = self.y * factor;
    }
}

fn main() {
    let mut p = Point::new(3.0, 4.0);
    let mag = p.magnitude();
    p.scale(2.0);
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen.generate(&module).expect("Failed to generate");
        let code = output.as_string().unwrap();

        // &self method should take a pointer parameter
        assert!(code.contains("Point*"), "Expected 'Point*' in:\n{}", code);
        // field access through pointer should use ->
        assert!(
            code.contains("->x") || code.contains("-> x"),
            "Expected '->x' (pointer field access) in:\n{}",
            code
        );
        // Method call should pass &p
        assert!(
            code.contains("&p") || code.contains("& p"),
            "Expected '&p' (address-of) in method call in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_ref_self_distance() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn distance(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        sqrt(dx * dx + dy * dy)
    }
}

fn main() {
    let p = Point { x: 3.0, y: 4.0 };
    let q = Point { x: 0.0, y: 0.0 };
    let dist = p.distance(&q);
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen.generate(&module).expect("Failed to generate");
        let code = output.as_string().unwrap();

        // &self method takes pointer
        assert!(
            code.contains("Point*"),
            "Expected 'Point*' param in:\n{}",
            code
        );
        // Field access through pointer should use ->
        assert!(code.contains("->x"), "Expected '->x' in:\n{}", code);
    }

    #[test]
    fn test_e2e_mut_self_method() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn scale(&mut self, factor: f64) {
        self.x = self.x * factor;
        self.y = self.y * factor;
    }
}

fn main() {
    let mut p = Point { x: 3.0, y: 4.0 };
    p.scale(2.0);
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen.generate(&module).expect("Failed to generate");
        let code = output.as_string().unwrap();

        // &mut self should generate pointer parameter
        assert!(
            code.contains("Point*"),
            "Expected 'Point*' param in:\n{}",
            code
        );
        // Field assignment through pointer: self->x = ...
        assert!(
            code.contains("->x =") || code.contains("->x="),
            "Expected '->x =' (pointer field assign) in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_primitive_float_methods() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn main() {
    let x: f64 = -3.7;
    let y: f64 = 2.0;

    let a = x.abs();
    let f = x.floor();
    let c = x.ceil();
    let s = (4.0).sqrt();
    let p = y.powi(3);
    let mx = x.max(y);
    let mn = x.min(y);
    let pi = 3.14159265358979323846;
    let deg = pi.to_degrees();
    let rad = (180.0).to_radians();
    let sv = (0.0).sin();
    let cv = (0.0).cos();
    let ev = (1.0).exp();
    let lv = (1.0).ln();
    let fv = (3.7).fract();
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // Verify primitive method calls are lowered to correct C functions
        assert!(
            code.contains("fabs("),
            "Expected fabs() for .abs() in:\n{}",
            code
        );
        assert!(
            code.contains("floor("),
            "Expected floor() for .floor() in:\n{}",
            code
        );
        assert!(
            code.contains("ceil("),
            "Expected ceil() for .ceil() in:\n{}",
            code
        );
        assert!(
            code.contains("sqrt("),
            "Expected sqrt() for .sqrt() in:\n{}",
            code
        );
        assert!(
            code.contains("pow("),
            "Expected pow() for .powi() in:\n{}",
            code
        );
        assert!(
            code.contains("fmax("),
            "Expected fmax() for .max() in:\n{}",
            code
        );
        assert!(
            code.contains("fmin("),
            "Expected fmin() for .min() in:\n{}",
            code
        );
        assert!(
            code.contains("sin("),
            "Expected sin() for .sin() in:\n{}",
            code
        );
        assert!(
            code.contains("cos("),
            "Expected cos() for .cos() in:\n{}",
            code
        );
        assert!(
            code.contains("exp("),
            "Expected exp() for .exp() in:\n{}",
            code
        );
        assert!(
            code.contains("log("),
            "Expected log() for .ln() in:\n{}",
            code
        );

        // to_degrees and to_radians lower to multiplication by constants
        // They should NOT produce a function call to "to_degrees" or "to_radians"
        assert!(
            !code.contains("to_degrees("),
            "to_degrees should be inlined, not a call in:\n{}",
            code
        );
        assert!(
            !code.contains("to_radians("),
            "to_radians should be inlined, not a call in:\n{}",
            code
        );

        // fract should use floor() and subtraction
        // Verify floor appears (used by both .floor() and .fract())
        let floor_count = code.matches("floor(").count();
        assert!(
            floor_count >= 2,
            "Expected at least 2 floor() calls (for .floor() and .fract()) in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_inline_module_basic() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
pub mod math {
    pub fn add(a: i32, b: i32) -> i32 {
        a + b
    }
}

fn main() {
    let result = math::add(3, 4);
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // The function inside `mod math` should be emitted as `math_add`
        assert!(
            code.contains("math_add"),
            "Expected math_add function in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_inline_module_with_const() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
pub mod config {
    pub const MAX_SIZE: i32 = 100;
}

fn main() {
    let x = config::MAX_SIZE;
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // The const inside `mod config` should be emitted as `config_MAX_SIZE`
        assert!(
            code.contains("config_MAX_SIZE"),
            "Expected config_MAX_SIZE global in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_inline_module_with_use_super() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn helper() -> i32 {
    42
}

pub mod inner {
    use super::*;

    pub fn call_helper() -> i32 {
        helper()
    }
}

fn main() {
    let x = inner::call_helper();
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // inner::call_helper should generate inner_call_helper
        assert!(
            code.contains("inner_call_helper"),
            "Expected inner_call_helper function in:\n{}",
            code
        );
        // The body should call helper() (the parent-scope function)
        assert!(
            code.contains("helper("),
            "Expected call to helper() in inner_call_helper body:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_nested_inline_modules() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
pub mod outer {
    pub mod inner {
        pub fn deep_fn() -> i32 {
            99
        }
    }
}

fn main() {
    let x = outer::inner::deep_fn();
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // Nested modules should produce outer_inner_deep_fn
        assert!(
            code.contains("outer_inner_deep_fn"),
            "Expected outer_inner_deep_fn function in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_struct_const() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
struct Color {
    r: f64,
    g: f64,
    b: f64,
}

const WHITE: Color = Color { r: 1.0, g: 1.0, b: 1.0 };
const BLACK: Color = Color { r: 0.0, g: 0.0, b: 0.0 };

fn main() {
    let w = WHITE;
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // Verify struct constant globals are emitted with initializers.
        // Static-storage globals use a brace-init list `{ ... }`, NOT a compound
        // literal `(Color){ ... }`: a compound literal is not a constant
        // expression, so MSVC rejects it as a file-scope initializer with C2099.
        // See `const_to_c_static`. Do not reintroduce the `(Color)` cast here.
        assert!(
            code.contains("const Color WHITE = {"),
            "Expected const Color WHITE global in:\n{}",
            code
        );
        assert!(
            code.contains("const Color BLACK = {"),
            "Expected const Color BLACK global in:\n{}",
            code
        );

        // Verify field values are present in the initializer
        assert!(
            code.contains("1") && code.contains("0"),
            "Expected field values in struct const initializer:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_vec_macro_literal() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn main() {
    let v = vec![1, 2, 3];
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // vec![1, 2, 3] should expand to vec_new + 3x vec_push
        assert!(
            code.contains("build_hvec_new_i32"),
            "Expected build_hvec_new_i32 call in:\n{}",
            code
        );
        assert!(
            code.contains("build_hvec_push_i32"),
            "Expected build_hvec_push_i32 calls in:\n{}",
            code
        );
        let push_count = code.matches("build_hvec_push_i32").count();
        assert!(
            push_count >= 3,
            "Expected at least 3 push calls, got {} in:\n{}",
            push_count,
            code
        );
    }

    #[test]
    fn test_e2e_vec_macro_repeat() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn main() {
    let zeros = vec![0.0; 5];
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // vec![0.0; 5] should use f64 variant and have a loop
        assert!(
            code.contains("build_hvec_new_f64"),
            "Expected build_hvec_new_f64 call in:\n{}",
            code
        );
        assert!(
            code.contains("build_hvec_push_f64"),
            "Expected build_hvec_push_f64 call in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_vec_type_annotation() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn main() {
    let v: Vec<f64> = vec![1.0, 2.0, 3.0];
    let len = vec_len(v);
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // Vec<f64> type annotation should produce BuildVecHandle
        assert!(
            code.contains("BuildVecHandle"),
            "Expected BuildVecHandle type in:\n{}",
            code
        );
        assert!(
            code.contains("build_hvec_new_f64"),
            "Expected build_hvec_new_f64 in:\n{}",
            code
        );
    }

    // =========================================================================
    // Iterator chain lowering tests
    // =========================================================================

    #[test]
    fn test_e2e_iter_map_collect() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn main() {
    let v = vec![1.0, 2.0, 3.0];
    let doubled = v.iter().map(|x: f64| x * 2.0).collect();
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // Iterator chain should lower to a loop with vec_new + get + push
        assert!(
            code.contains("build_hvec_new_f64"),
            "Expected build_hvec_new_f64 for collect result in:\n{}",
            code
        );
        assert!(
            code.contains("build_hvec_get_f64"),
            "Expected build_hvec_get_f64 for element access in:\n{}",
            code
        );
        assert!(
            code.contains("build_hvec_push_f64"),
            "Expected build_hvec_push_f64 for collect push in:\n{}",
            code
        );
        assert!(
            code.contains("build_hvec_len"),
            "Expected build_hvec_len for loop bound in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_iter_fold() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn main() {
    let v = vec![1.0, 2.0, 3.0];
    let sum = v.iter().fold(0.0, |acc: f64, x: f64| acc + x);
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // fold should lower to a loop with accumulator but NO vec_new/push
        assert!(
            code.contains("build_hvec_get_f64"),
            "Expected build_hvec_get_f64 for element access in:\n{}",
            code
        );
        assert!(
            code.contains("build_hvec_len"),
            "Expected build_hvec_len for loop bound in:\n{}",
            code
        );
        // fold shouldn't create a new output vec (only the source vec![...] + runtime definition)
        let new_count = code.matches("build_hvec_new_f64").count();
        // 1 for the runtime header definition + 1 for source vec = 2 total
        assert!(
            new_count == 2,
            "Expected exactly 2 build_hvec_new_f64 (runtime def + source), got {} in:\n{}",
            new_count,
            code
        );
        // Also verify no push calls (fold accumulates, doesn't push)
        // Count only calls in main(), not the runtime definition
        let push_count = code.matches("build_hvec_push_f64(").count();
        // 3 pushes for vec![1.0, 2.0, 3.0] + 1 runtime def = 4
        assert!(
            push_count <= 4,
            "Fold should not add extra push calls, got {} in:\n{}",
            push_count,
            code
        );
    }

    #[test]
    fn test_e2e_iter_map_fold_chain() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn main() {
    let v = vec![1.0, 2.0, 3.0];
    let sum_sq = v.iter().map(|x: f64| x * x).fold(0.0, |acc: f64, x: f64| acc + x);
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // map + fold chain: should have get + len but only 1 new (for source)
        assert!(
            code.contains("build_hvec_get_f64"),
            "Expected build_hvec_get_f64 in:\n{}",
            code
        );
        assert!(
            code.contains("build_hvec_len"),
            "Expected build_hvec_len in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_iter_enumerate_map_collect() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn main() {
    let v = vec![10.0, 20.0, 30.0];
    let indices = v.iter().enumerate().map(|i: i64, x: f64| i).collect();
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // enumerate + map + collect: should have the loop infrastructure
        assert!(
            code.contains("build_hvec_len"),
            "Expected build_hvec_len for loop bound in:\n{}",
            code
        );
        assert!(
            code.contains("build_hvec_get_f64"),
            "Expected build_hvec_get_f64 in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_inclusive_range_for_loop() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn main() {
    let mut sum = 0;
    for i in 0..=5 {
        sum = sum + i;
    }
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // Inclusive range: exit condition uses > (not >=)
        assert!(
            code.contains("> 5)"),
            "Expected `> 5)` for inclusive range exit in:\n{}",
            code
        );
        // Should have an increment by 1
        assert!(
            code.contains("+ 1)"),
            "Expected increment by 1 in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_range_step_by() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn main() {
    let mut count = 0;
    for i in (0..10).step_by(2) {
        count = count + 1;
    }
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // Exclusive range: exit condition uses >=
        assert!(
            code.contains(">= 10)"),
            "Expected `>= 10)` for exclusive range exit in:\n{}",
            code
        );
        // step_by(2): increment by 2 instead of 1
        assert!(
            code.contains("+ 2)"),
            "Expected increment by 2 for step_by(2) in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_inclusive_range_step_by() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn main() {
    let mut vals = vec![];
    for i in (0..=10).step_by(3) {
        vec_push(vals, i);
    }
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // Inclusive range: exit condition uses >
        assert!(
            code.contains("> 10)"),
            "Expected `> 10)` for inclusive range exit in:\n{}",
            code
        );
        // step_by(3): increment by 3
        assert!(
            code.contains("+ 3)"),
            "Expected increment by 3 for step_by(3) in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_main_argc_argv() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn main() {
    let n = args_count();
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        // main gets argc/argv signature
        assert!(
            code.contains("main(int argc, char** argv)"),
            "Missing argc/argv signature in:\n{}",
            code
        );
        // args init is called
        assert!(
            code.contains("build_args_init(argc, argv)"),
            "Missing args init in:\n{}",
            code
        );
        // args_count builtin is called
        assert!(
            code.contains("build_args_count"),
            "Missing args_count call in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_string_parse_methods() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn main() {
    let s = "Hello, World!";
    let idx = s.index_of("World");
    let sub = s.substring(0, 5);
    let r = s.replace("World", "BuildLang");
    let n = "42".parse_int();
    let f = "3.14".parse_float();
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        assert!(
            code.contains("build_string_index_of"),
            "Missing index_of call in:\n{}",
            code
        );
        assert!(
            code.contains("build_string_substring"),
            "Missing substring call in:\n{}",
            code
        );
        assert!(
            code.contains("build_string_replace"),
            "Missing replace call in:\n{}",
            code
        );
        assert!(
            code.contains("build_string_parse_int"),
            "Missing parse_int call in:\n{}",
            code
        );
        assert!(
            code.contains("build_string_parse_float"),
            "Missing parse_float call in:\n{}",
            code
        );
    }

    #[test]
    fn test_e2e_stdin_builtins() {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let source = r#"
fn main() {
    let piped = stdin_is_pipe();
    if piped {
        let input = read_all();
    }
}
"#;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        let code = output.as_string().unwrap();

        assert!(
            code.contains("build_stdin_is_pipe"),
            "Missing stdin_is_pipe call in:\n{}",
            code
        );
        assert!(
            code.contains("build_read_all"),
            "Missing read_all call in:\n{}",
            code
        );
    }

    // =========================================================================
    // SNAPSHOT TESTS (insta)
    // =========================================================================

    /// Helper: parse source → type-check → generate C code string.
    fn snapshot_codegen(source: &str) -> String {
        use crate::codegen::{CodeGenerator, Target};
        use crate::parser::parse_source;
        use crate::types::TypeContext;

        let module = parse_source("test.bld", source).expect("Failed to parse");
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::with_source(&ctx, Target::C, Arc::from(source));
        let output = codegen
            .generate(&module)
            .expect("Failed to generate C code");
        output.as_string().unwrap().to_string()
    }

    #[test]
    fn test_function_style_println_formats_arguments_and_literals() {
        let source = r#"
fn main() {
    let v: i32 = 4;
    println("{}", v);
    println("100% ready");
    println("{}");
}
"#;
        let code = snapshot_codegen(source);
        assert!(
            code.contains("printf(\"%d\\n\","),
            "function-style println should lower format args to printf specifiers:\n{}",
            code
        );
        assert!(
            code.contains("printf(\"100%% ready\\n\");"),
            "literal percent signs must be escaped for printf:\n{}",
            code
        );
        assert!(
            code.contains("printf(\"{}\\n\");"),
            "missing format args should keep braces literal instead of emitting a printf specifier:\n{}",
            code
        );
    }

    #[test]
    fn test_snapshot_hello() {
        let source = r#"
fn main() {
    println!("Hello");
}
"#;
        let code = snapshot_codegen(source);
        insta::assert_snapshot!(code);
    }

    #[test]
    fn test_snapshot_arithmetic() {
        let source = r#"
fn main() {
    let x = 3 + 4 * 5;
    println!("{}", x);
}
"#;
        let code = snapshot_codegen(source);
        insta::assert_snapshot!(code);
    }

    #[test]
    fn test_snapshot_struct() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let p = Point { x: 1, y: 2 };
}
"#;
        let code = snapshot_codegen(source);
        insta::assert_snapshot!(code);
    }

    #[test]
    fn test_snapshot_if_else() {
        let source = r#"
fn main() {
    let x = 10;
    if x > 5 {
        println!("big");
    } else {
        println!("small");
    }
}
"#;
        let code = snapshot_codegen(source);
        insta::assert_snapshot!(code);
    }

    #[test]
    fn test_snapshot_recursion() {
        let source = r#"
fn factorial(n: i32) -> i32 {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}

fn main() {
    println!("{}", factorial(5));
}
"#;
        let code = snapshot_codegen(source);
        insta::assert_snapshot!(code);
    }
}
