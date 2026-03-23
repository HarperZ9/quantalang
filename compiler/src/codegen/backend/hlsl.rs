// ===============================================================================
// QUANTALANG CODE GENERATOR - HLSL BACKEND
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! HLSL code generation backend.
//!
//! Transpiles MIR to HLSL (High Level Shading Language) for use with:
//! - ReShade shader mods
//! - DirectX 11/12 shaders
//! - Unity/Unreal custom shaders
//!
//! Based on the C backend with HLSL-specific type mappings and shader semantics.

use std::fmt::Write;
use std::sync::Arc;

use super::{CodegenError, CodegenResult};
use crate::codegen::ir::*;

/// HLSL backend for code generation.
pub struct HlslBackend {
    /// Output buffer.
    output: String,
    /// Indentation level.
    indent: usize,
    /// Inlined expression strings for single-use temps (interior mutability for &self access).
    inlined_exprs: std::cell::RefCell<std::collections::HashMap<u32, String>>,
    /// Locals identified as single-use temps that will be inlined.
    single_use_temps: std::collections::HashSet<u32>,
    /// Locals whose declaration can be folded into their first assignment.
    /// (assigned exactly once, at function body top level)
    fold_decl: std::collections::HashSet<u32>,
    /// Locals that have already been declared (via folded assignment).
    declared: std::collections::HashSet<u32>,
}

impl HlslBackend {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            inlined_exprs: std::cell::RefCell::new(std::collections::HashMap::new()),
            single_use_temps: std::collections::HashSet::new(),
            fold_decl: std::collections::HashSet::new(),
            declared: std::collections::HashSet::new(),
        }
    }

    /// Count how many times each local is used across all blocks.
    fn compute_use_counts(blocks: &[MirBlock]) -> std::collections::HashMap<u32, u32> {
        let mut counts = std::collections::HashMap::new();

        fn count_value(val: &MirValue, counts: &mut std::collections::HashMap<u32, u32>) {
            if let MirValue::Local(id) = val {
                *counts.entry(id.0).or_insert(0) += 1;
            }
        }

        fn count_rvalue(rv: &MirRValue, counts: &mut std::collections::HashMap<u32, u32>) {
            match rv {
                MirRValue::Use(v) => count_value(v, counts),
                MirRValue::BinaryOp { left, right, .. } => {
                    count_value(left, counts);
                    count_value(right, counts);
                }
                MirRValue::UnaryOp { operand, .. } => count_value(operand, counts),
                MirRValue::Aggregate { operands, .. } => {
                    for op in operands { count_value(op, counts); }
                }
                MirRValue::FieldAccess { base, .. } => count_value(base, counts),
                MirRValue::Cast { value, .. } => count_value(value, counts),
                _ => {}
            }
        }

        for block in blocks {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign { value, .. } = &stmt.kind {
                    count_rvalue(value, &mut counts);
                }
            }
            if let Some(term) = &block.terminator {
                match term {
                    MirTerminator::Return(Some(v)) => count_value(v, &mut counts),
                    MirTerminator::If { cond, .. } => count_value(cond, &mut counts),
                    MirTerminator::Call { args, .. } => {
                        for a in args { count_value(a, &mut counts); }
                    }
                    _ => {}
                }
            }
        }
        counts
    }

    /// Identify single-use temporaries that can be inlined.
    /// A local is inlineable iff: assigned exactly once, used exactly once, is a temp, and is pure.
    fn compute_inlineable_temps(
        blocks: &[MirBlock],
        locals: &[MirLocal],
        use_counts: &std::collections::HashMap<u32, u32>,
    ) -> std::collections::HashSet<u32> {
        // Count how many times each local is ASSIGNED (must be exactly 1 for safe inlining)
        let mut assign_counts: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        for block in blocks {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign { dest, .. } = &stmt.kind {
                    *assign_counts.entry(dest.0).or_insert(0) += 1;
                }
            }
            if let Some(MirTerminator::Call { dest: Some(dest_id), .. }) = &block.terminator {
                *assign_counts.entry(dest_id.0).or_insert(0) += 1;
            }
        }

        let mut inlineable = std::collections::HashSet::new();
        for block in blocks {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign { dest, value } = &stmt.kind {
                    let use_count = use_counts.get(&dest.0).copied().unwrap_or(0);
                    let def_count = assign_counts.get(&dest.0).copied().unwrap_or(0);
                    if use_count == 1 && def_count == 1 {
                        // Check it's a compiler temp (name starts with _)
                        let name = locals.iter()
                            .find(|l| l.id == *dest)
                            .and_then(|l| l.name.as_ref());
                        let is_temp = match name {
                            Some(n) => n.starts_with('_') || n.as_ref().chars().next().map_or(true, |c| c == '_'),
                            None => true, // unnamed locals are temps
                        };
                        // Only inline pure expressions (not used for control flow side effects)
                        let is_pure = matches!(value,
                            MirRValue::Use(_) | MirRValue::BinaryOp { .. } |
                            MirRValue::UnaryOp { .. } | MirRValue::FieldAccess { .. } |
                            MirRValue::Cast { .. } | MirRValue::Aggregate { .. }
                        );
                        if is_temp && is_pure {
                            inlineable.insert(dest.0);
                        }
                    }
                }
            }
            // Also check Call terminator destinations (single-use call results)
            if let Some(MirTerminator::Call { dest: Some(dest_id), .. }) = &block.terminator {
                let use_count = use_counts.get(&dest_id.0).copied().unwrap_or(0);
                let def_count = assign_counts.get(&dest_id.0).copied().unwrap_or(0);
                if use_count == 1 && def_count == 1 {
                    let name = locals.iter()
                        .find(|l| l.id == *dest_id)
                        .and_then(|l| l.name.as_ref());
                    let is_temp = match name {
                        Some(n) => n.starts_with('_') || n.as_ref().starts_with('_'),
                        None => true,
                    };
                    if is_temp {
                        inlineable.insert(dest_id.0);
                    }
                }
            }
        }
        inlineable
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
    }

    fn writeln(&mut self, s: &str) {
        self.write_indent();
        self.output.push_str(s);
        self.output.push('\n');
    }

    /// Generate HLSL from a MIR module.
    pub fn generate(&mut self, module: &MirModule) -> CodegenResult<String> {
        self.output.clear();

        // Header
        self.output.push_str("// Generated by QuantaLang Compiler\n");
        self.output.push_str("// Target: HLSL (DirectX / ReShade)\n");
        self.output.push_str("// Do not edit manually\n\n");

        // Uniform declarations
        self.generate_uniforms(&module.uniforms);

        // Struct definitions
        for ty in &module.types {
            self.generate_struct(ty)?;
        }

        // Function definitions
        for func in &module.functions {
            if !func.is_declaration() {
                self.generate_function(func)?;
            }
        }

        Ok(self.output.clone())
    }

    /// Generate a complete ReShade .fx effect file.
    /// Includes ReShade.fxh, generates all shader functions, and adds
    /// a technique/pass block. If a #[fragment] function exists, it's
    /// wired as the pixel shader; otherwise a template is generated.
    pub fn generate_reshade(&mut self, module: &MirModule) -> CodegenResult<String> {
        self.output.clear();

        // ReShade header
        self.output.push_str("// Generated by QuantaLang Compiler\n");
        self.output.push_str("// Target: ReShade Effect (.fx)\n");
        self.output.push_str("// Do not edit generated functions — regenerate from .quanta source\n\n");
        self.output.push_str("#include \"ReShade.fxh\"\n\n");

        // Uniform declarations with ReShade UI annotations
        self.generate_reshade_uniforms(&module.uniforms);

        // Struct definitions
        for ty in &module.types {
            self.generate_struct(ty)?;
        }

        // Function definitions
        for func in &module.functions {
            if !func.is_declaration() {
                self.generate_function(func)?;
            }
        }

        // Check for explicit fragment shader
        let fragment_fn = module.functions.iter()
            .find(|f| matches!(f.shader_stage, Some(ShaderStage::Fragment)));

        if let Some(frag) = fragment_fn {
            // Wire the fragment function as the pixel shader
            self.writeln(&format!("technique Quanta_{} {{", frag.name));
            self.indent += 1;
            self.writeln("pass {");
            self.indent += 1;
            self.writeln("VertexShader = PostProcessVS;");
            self.writeln(&format!("PixelShader = {};", frag.name));
            self.indent -= 1;
            self.writeln("}");
            self.indent -= 1;
            self.writeln("}\n");
        } else {
            // Generate pixel shader template + technique
            self.output.push_str("// =============================================================\n");
            self.output.push_str("// ReShade Integration — customize the pixel shader below\n");
            self.output.push_str("// =============================================================\n\n");
            self.output.push_str("float4 PS_Main(float4 pos : SV_Position, float2 texcoord : TEXCOORD) : SV_Target {\n");
            self.output.push_str("    float3 color = tex2D(ReShade::BackBuffer, texcoord).rgb;\n");
            self.output.push_str("    // TODO: Apply your QuantaLang functions to 'color' here\n");
            self.output.push_str("    return float4(color, 1.0);\n");
            self.output.push_str("}\n\n");
            self.output.push_str("technique QuantaShader {\n");
            self.output.push_str("    pass {\n");
            self.output.push_str("        VertexShader = PostProcessVS;\n");
            self.output.push_str("        PixelShader = PS_Main;\n");
            self.output.push_str("    }\n");
            self.output.push_str("}\n");
        }

        Ok(self.output.clone())
    }

    /// Check if a block (or its descendants) contains a back-edge to the target block.
    /// Used to detect while loop patterns in the MIR control flow graph.
    /// Strip redundant outer parentheses from an expression string.
    fn strip_outer_parens(s: &str) -> &str {
        let t = s.trim();
        if t.len() < 2 || !t.starts_with('(') || !t.ends_with(')') {
            return t;
        }
        let mut depth = 0i32;
        for (i, c) in t.chars().enumerate() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && i < t.len() - 1 {
                        return t; // closing paren isn't the last — not a simple wrapper
                    }
                }
                _ => {}
            }
        }
        if depth == 0 { &t[1..t.len()-1] } else { t }
    }

    /// Check if a block (or its descendants) contains a back-edge to the target block.
    /// Only follows FORWARD edges (block ID > target ID) to avoid false positives
    /// from outer loop iterations reaching inner blocks through the outer back-edge.
    fn has_back_edge(from: BlockId, target: BlockId, all_blocks: &[MirBlock]) -> bool {
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![from];
        while let Some(id) = stack.pop() {
            if visited.contains(&id.0) { continue; }
            visited.insert(id.0);
            if let Some(block) = all_blocks.iter().find(|b| b.id == id) {
                let follow = |t: &BlockId, stack: &mut Vec<BlockId>| {
                    if *t == target { return true; }
                    // Only follow forward edges to avoid tracing through outer loops
                    if t.0 > target.0 { stack.push(*t); }
                    false
                };
                match &block.terminator {
                    Some(MirTerminator::Goto(t)) => {
                        if follow(t, &mut stack) { return true; }
                    }
                    Some(MirTerminator::Call { target: Some(t), .. }) => {
                        if follow(t, &mut stack) { return true; }
                    }
                    Some(MirTerminator::If { then_block, else_block, .. }) => {
                        if follow(then_block, &mut stack) { return true; }
                        if follow(else_block, &mut stack) { return true; }
                    }
                    _ => {}
                }
            }
        }
        false
    }

    /// Emit plain HLSL uniform declarations.
    fn generate_uniforms(&mut self, uniforms: &[MirUniform]) {
        for u in uniforms {
            let ty_str = self.type_to_hlsl(&u.ty);
            if let Some(ref default) = u.default {
                self.writeln(&format!("uniform {} {} = {};", ty_str, u.name, self.const_to_hlsl(default)));
            } else {
                self.writeln(&format!("uniform {} {};", ty_str, u.name));
            }
        }
        if !uniforms.is_empty() {
            self.output.push('\n');
        }
    }

    /// Emit ReShade-annotated uniform declarations with UI slider metadata.
    fn generate_reshade_uniforms(&mut self, uniforms: &[MirUniform]) {
        for u in uniforms {
            let ty_str = self.type_to_hlsl(&u.ty);
            let default_str = u.default.as_ref()
                .map(|d| self.const_to_hlsl(d))
                .unwrap_or_else(|| "0".to_string());

            // Infer smart slider ranges from default value
            let (ui_min, ui_max, ui_step) = if let Some(MirConst::Float(v, _)) = &u.default {
                let abs_v = v.abs();
                if abs_v < 0.01 {
                    ("0.0", "0.1", "0.001")
                } else if abs_v < 0.1 {
                    ("0.0", "1.0", "0.005")
                } else if abs_v <= 1.0 {
                    ("0.0", "2.0", "0.01")
                } else if abs_v <= 10.0 {
                    ("0.0", "20.0", "0.1")
                } else {
                    ("0.0", "100.0", "1.0")
                }
            } else if let Some(MirConst::Int(_, _)) = &u.default {
                ("0", "100", "1")
            } else {
                ("0.0", "1.0", "0.01")
            };

            self.writeln(&format!("uniform {} {} <", ty_str, u.name));
            self.indent += 1;
            self.writeln("ui_type = \"slider\";");
            self.writeln(&format!("ui_min = {}; ui_max = {}; ui_step = {};", ui_min, ui_max, ui_step));
            self.writeln(&format!("ui_label = \"{}\";", u.name));
            self.indent -= 1;
            self.writeln(&format!("> = {};", default_str));
            self.output.push('\n');
        }
    }

    fn generate_struct(&mut self, ty: &MirTypeDef) -> CodegenResult<()> {
        // Skip built-in vector types — HLSL has native float2/3/4
        match ty.name.as_ref() {
            "quanta_vec2" | "quanta_vec3" | "quanta_vec4" | "quanta_mat4" => return Ok(()),
            _ => {}
        }
        match &ty.kind {
            TypeDefKind::Struct { fields, .. } => {
                self.writeln(&format!("struct {} {{", ty.name));
                self.indent += 1;
                for (i, (name, field_ty)) in fields.iter().enumerate() {
                    let fname = name.as_ref()
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| format!("field_{}", i));
                    self.writeln(&format!("{} {};", self.type_to_hlsl(field_ty), fname));
                }
                self.indent -= 1;
                self.writeln("};\n");
            }
            TypeDefKind::Enum { variants, discriminant_ty } => {
                // Enums as tagged unions (same as C backend)
                let disc_ty = self.type_to_hlsl(discriminant_ty);
                self.writeln(&format!("struct {} {{", ty.name));
                self.indent += 1;
                self.writeln(&format!("{} tag;", disc_ty));
                // Union of variant data
                if variants.iter().any(|v| !v.fields.is_empty()) {
                    self.writeln("// variant data");
                }
                self.indent -= 1;
                self.writeln("};\n");
            }
            _ => {} // Union types not supported in HLSL
        }
        Ok(())
    }

    fn generate_function(&mut self, func: &MirFunction) -> CodegenResult<()> {
        // Skip non-shader main (test harness with printf/println)
        if func.name.as_ref() == "main" && func.shader_stage.is_none() {
            return Ok(());
        }

        let ret_ty = self.type_to_hlsl(&func.sig.ret);
        let params: Vec<String> = func.sig.params.iter().enumerate().map(|(i, ty)| {
            let param_name = func.locals.iter()
                .find(|l| l.is_param && l.id.0 == i as u32)
                .and_then(|l| l.name.as_ref())
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("p{}", i));

            // Add shader semantics for entry points
            let semantic = if func.shader_stage.is_some() || func.name.as_ref() == "main" {
                match param_name.as_str() {
                    "vertex_id" | "gl_VertexIndex" => " : SV_VertexID",
                    "instance_id" => " : SV_InstanceID",
                    // Fragment shader: vec2 params get TEXCOORD semantic
                    "uv" | "texcoord" | "tex_coord" | "coord" =>
                        if matches!(func.shader_stage, Some(ShaderStage::Fragment)) { " : TEXCOORD" } else { "" },
                    _ => "",
                }
            } else {
                ""
            };

            format!("{} {}{}", self.type_to_hlsl(ty), param_name, semantic)
        }).collect();

        // Return semantic for shader entry points
        let ret_semantic = if func.shader_stage.is_some() || func.name.as_ref() == "main" {
            match func.shader_stage {
                Some(ShaderStage::Vertex) => " : SV_Position",
                Some(ShaderStage::Fragment) => " : SV_Target0",
                _ => "",
            }
        } else {
            ""
        };

        // Fragment shaders: auto-add SV_Position parameter if not present
        let mut final_params = params.clone();
        if matches!(func.shader_stage, Some(ShaderStage::Fragment)) {
            let has_pos = func.sig.params.iter().any(|ty| matches!(ty, MirType::Struct(n) if n.as_ref() == "quanta_vec4"));
            if !has_pos {
                final_params.insert(0, "float4 pos : SV_Position".to_string());
            }
        }

        self.writeln(&format!("{} {}({}){} {{",
            ret_ty, func.name, final_params.join(", "), ret_semantic));
        self.indent += 1;

        // Deep expression inlining: identify single-use temps and inline them
        self.inlined_exprs.borrow_mut().clear();
        self.single_use_temps.clear();
        self.fold_decl.clear();
        self.declared.clear();
        if let Some(blocks) = &func.blocks {
            let use_counts = Self::compute_use_counts(blocks);
            self.single_use_temps = Self::compute_inlineable_temps(blocks, &func.locals, &use_counts);

            // Identify locals that can have declaration folded into first assignment.
            // Compute the "entry chain" — blocks reachable from block 0 via linear
            // Call/Goto continuations (no branches). Assignments in this chain are top-level.
            let mut entry_chain: std::collections::HashSet<usize> = std::collections::HashSet::new();
            entry_chain.insert(0);
            let mut chain_idx = 0usize;
            loop {
                if chain_idx >= blocks.len() { break; }
                match &blocks[chain_idx].terminator {
                    Some(MirTerminator::Call { target: Some(t), .. }) => {
                        if let Some(idx) = blocks.iter().position(|b| b.id == *t) {
                            entry_chain.insert(idx);
                            chain_idx = idx;
                            continue;
                        }
                    }
                    Some(MirTerminator::Goto(t)) => {
                        if let Some(idx) = blocks.iter().position(|b| b.id == *t) {
                            if idx > chain_idx {
                                entry_chain.insert(idx);
                                chain_idx = idx;
                                continue;
                            }
                        }
                    }
                    _ => {}
                }
                break;
            }

            let mut assign_counts: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
            let mut entry_assigned: std::collections::HashSet<u32> = std::collections::HashSet::new();
            for (bi, block) in blocks.iter().enumerate() {
                let in_entry = entry_chain.contains(&bi);
                for stmt in &block.stmts {
                    if let MirStmtKind::Assign { dest, .. } = &stmt.kind {
                        *assign_counts.entry(dest.0).or_insert(0) += 1;
                        if in_entry { entry_assigned.insert(dest.0); }
                    }
                }
                if let Some(MirTerminator::Call { dest: Some(dest_id), .. }) = &block.terminator {
                    *assign_counts.entry(dest_id.0).or_insert(0) += 1;
                    if in_entry { entry_assigned.insert(dest_id.0); }
                }
            }
            for local in &func.locals {
                if local.is_param { continue; }
                if matches!(local.ty, MirType::Void) { continue; }
                if self.single_use_temps.contains(&local.id.0) { continue; }
                let name = self.local_name(local.id, &func.locals);
                if name == "_ret" { continue; }
                let count = assign_counts.get(&local.id.0).copied().unwrap_or(0);
                // Fold declaration for ANY single-assignment variable
                // (HLSL allows declarations at any point in any scope)
                if count == 1 {
                    self.fold_decl.insert(local.id.0);
                }
            }
        }

        // Local variable declarations — only for locals that can't be folded
        for local in &func.locals {
            if !local.is_param {
                if matches!(local.ty, MirType::Void) { continue; }
                let name = self.local_name(local.id, &func.locals);
                if name == "_ret" { continue; }
                if self.single_use_temps.contains(&local.id.0) { continue; }
                if self.fold_decl.contains(&local.id.0) { continue; }
                let ty_str = self.type_to_hlsl(&local.ty);
                self.writeln(&format!("{} {};", ty_str, name));
            }
        }

        // Generate function body from basic blocks
        if let Some(blocks) = &func.blocks {
            // Emit blocks with structured control flow reconstruction
            let mut emitted: std::collections::HashSet<u32> = std::collections::HashSet::new();
            for block in blocks {
                if !emitted.contains(&block.id.0) {
                    self.generate_block_structured(block, blocks, func, &mut emitted)?;
                }
            }

            // Safety return for non-void functions where else-path falls through
            // (e.g., if/else where then returns early but else assigns and drops out)
            if !matches!(func.sig.ret, MirType::Void) {
                let last_is_return = self.output.lines().rev()
                    .find(|l| !l.trim().is_empty())
                    .map_or(false, |l| l.trim().starts_with("return"));
                if !last_is_return {
                    if let Some(ret_val) = self.find_return_value(blocks, &func.locals) {
                        self.writeln(&format!("return {};", ret_val));
                    }
                }
            }
        }

        self.indent -= 1;
        self.writeln("}\n");
        Ok(())
    }

    fn generate_block_structured(
        &mut self,
        block: &MirBlock,
        all_blocks: &[MirBlock],
        func: &MirFunction,
        emitted: &mut std::collections::HashSet<u32>,
    ) -> CodegenResult<()> {
        if emitted.contains(&block.id.0) {
            return Ok(());
        }
        emitted.insert(block.id.0);

        // Emit statements with deep expression inlining
        for stmt in &block.stmts {
            if let MirStmtKind::Assign { dest, value } = &stmt.kind {
                if self.single_use_temps.contains(&dest.0) {
                    // Don't emit — store the expression for inlining at use site
                    let expr_str = self.rvalue_to_hlsl(value, func);
                    self.inlined_exprs.borrow_mut().insert(dest.0, expr_str);
                    continue;
                }
            }
            self.generate_statement(stmt, func)?;
        }

        // Handle terminator with structured control flow
        if let Some(term) = &block.terminator {
            match term {
                MirTerminator::If { cond, then_block, else_block } => {
                    // Detect while loop pattern: then-branch loops back to current block
                    let is_while = Self::has_back_edge(*then_block, block.id, all_blocks);
                    let cond_raw = self.value_to_hlsl(cond, &func.locals);
                    let cond_str = Self::strip_outer_parens(&cond_raw);

                    if is_while {
                        self.writeln(&format!("while ({}) {{", cond_str));
                        self.indent += 1;
                        if let Some(tb) = all_blocks.iter().find(|b| b.id == *then_block) {
                            self.generate_block_structured(tb, all_blocks, func, emitted)?;
                        }
                        self.indent -= 1;
                        self.writeln("}");
                        if let Some(eb) = all_blocks.iter().find(|b| b.id == *else_block) {
                            self.generate_block_structured(eb, all_blocks, func, emitted)?;
                        }
                    } else {
                        self.writeln(&format!("if ({}) {{", cond_str));
                        self.indent += 1;
                        if let Some(tb) = all_blocks.iter().find(|b| b.id == *then_block) {
                            self.generate_block_structured(tb, all_blocks, func, emitted)?;
                        }
                        self.indent -= 1;
                        self.writeln("} else {");
                        self.indent += 1;
                        if let Some(eb) = all_blocks.iter().find(|b| b.id == *else_block) {
                            self.generate_block_structured(eb, all_blocks, func, emitted)?;
                        }
                        self.indent -= 1;
                        self.writeln("}");
                    }
                }
                MirTerminator::Goto(target) => {
                    // Follow the goto by emitting the target block inline
                    if let Some(tb) = all_blocks.iter().find(|b| b.id == *target) {
                        self.generate_block_structured(tb, all_blocks, func, emitted)?;
                    }
                }
                MirTerminator::Call { func: callee, args, dest, target, .. } => {
                    let call_expr = self.generate_call_expr(callee, args, &func.locals);
                    if let Some(dest_id) = dest {
                        if self.single_use_temps.contains(&dest_id.0) {
                            // Inline: store call expression for later substitution
                            self.inlined_exprs.borrow_mut().insert(dest_id.0, call_expr);
                        } else {
                            let dest_name = self.local_name(*dest_id, &func.locals);
                            // Fold declaration with Call result assignment
                            if self.fold_decl.contains(&dest_id.0) && !self.declared.contains(&dest_id.0) {
                                let dest_ty = func.locals.iter().find(|l| l.id == *dest_id).map(|l| &l.ty);
                                if let Some(ty) = dest_ty {
                                    let ty_str = self.type_to_hlsl(ty);
                                    self.writeln(&format!("{} {} = {};", ty_str, dest_name, call_expr));
                                    self.declared.insert(dest_id.0);
                                } else {
                                    self.writeln(&format!("{} = {};", dest_name, call_expr));
                                }
                            } else {
                                self.writeln(&format!("{} = {};", dest_name, call_expr));
                            }
                        }
                    } else {
                        self.writeln(&format!("{};", call_expr));
                    }
                    if let Some(target_id) = target {
                        if let Some(tb) = all_blocks.iter().find(|b| b.id == *target_id) {
                            self.generate_block_structured(tb, all_blocks, func, emitted)?;
                        }
                    }
                }
                MirTerminator::Return(Some(val)) => {
                    // Peephole: "temp = expr; return temp;" → "return expr;"
                    let val_str = self.value_to_hlsl(val, &func.locals);
                    // Find the last non-empty line and check if it assigns to the return value
                    let mut folded = false;
                    let search_pat = format!("{} = ", val_str);
                    // Trim trailing whitespace to find actual last content
                    let trimmed_len = self.output.trim_end().len();
                    if trimmed_len > 0 {
                        // Find start of last line
                        let last_nl = self.output[..trimmed_len].rfind('\n').unwrap_or(0);
                        let line_start = if last_nl == 0 { 0 } else { last_nl + 1 };
                        let last_line = self.output[line_start..trimmed_len].trim();
                        if last_line.contains(&search_pat) && last_line.ends_with(';') {
                            if let Some(eq_pos) = last_line.find(" = ") {
                                let rhs = &last_line[eq_pos+3..last_line.len()-1];
                                let rhs_owned = rhs.to_string();
                                self.output.truncate(line_start);
                                self.writeln(&format!("return {};", rhs_owned));
                                folded = true;
                            }
                        }
                    }
                    if !folded {
                        self.writeln(&format!("return {};", val_str));
                    }
                }
                _ => {
                    self.generate_terminator(term, func)?;
                }
            }
        }
        Ok(())
    }

    fn generate_statement(&mut self, stmt: &MirStmt, func: &MirFunction) -> CodegenResult<()> {
        match &stmt.kind {
            MirStmtKind::Assign { dest, value } => {
                // Skip void-typed assignments (unit values, no-ops)
                let dest_ty = func.locals.iter().find(|l| l.id == *dest).map(|l| &l.ty);
                if matches!(dest_ty, Some(MirType::Void)) { return Ok(()); }
                let dest_name = self.local_name(*dest, &func.locals);
                let val_str = self.rvalue_to_hlsl(value, func);
                // Fold declaration with first assignment if eligible
                if self.fold_decl.contains(&dest.0) && !self.declared.contains(&dest.0) {
                    let ty_str = self.type_to_hlsl(dest_ty.unwrap());
                    self.writeln(&format!("{} {} = {};", ty_str, dest_name, val_str));
                    self.declared.insert(dest.0);
                } else {
                    self.writeln(&format!("{} = {};", dest_name, val_str));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn generate_terminator(&mut self, term: &MirTerminator, func: &MirFunction) -> CodegenResult<()> {
        match term {
            MirTerminator::Return(Some(val)) => {
                let val_str = self.value_to_hlsl(val, &func.locals);
                self.writeln(&format!("return {};", val_str));
            }
            MirTerminator::Return(None) => {
                self.writeln("return;");
            }
            MirTerminator::Goto(_) | MirTerminator::If { .. } => {
                // Handled by generate_block_structured
            }
            MirTerminator::Call { func: callee, args, dest, .. } => {
                let call_expr = self.generate_call_expr(callee, args, &func.locals);
                if let Some(dest_id) = dest {
                    let dest_name = self.local_name(*dest_id, &func.locals);
                    self.writeln(&format!("{} = {};", dest_name, call_expr));
                } else {
                    self.writeln(&format!("{};", call_expr));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn rvalue_to_hlsl(&self, rvalue: &MirRValue, func: &MirFunction) -> String {
        match rvalue {
            MirRValue::Use(val) => self.value_to_hlsl(val, &func.locals),
            MirRValue::BinaryOp { op, left, right } => {
                let l = self.value_to_hlsl(left, &func.locals);
                let r = self.value_to_hlsl(right, &func.locals);
                let op_str = match op {
                    BinOp::Add => "+", BinOp::Sub => "-",
                    BinOp::Mul => "*", BinOp::Div => "/",
                    BinOp::Rem => "%",
                    BinOp::Eq => "==", BinOp::Ne => "!=",
                    BinOp::Lt => "<", BinOp::Le => "<=",
                    BinOp::Gt => ">", BinOp::Ge => ">=",
                    BinOp::BitAnd => "&", BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::Shl => "<<", BinOp::Shr => ">>",
                    BinOp::Pow => "pow", // handled as function call
                    _ => "??",
                };
                if matches!(op, BinOp::Pow) {
                    format!("pow({}, {})", l, r)
                } else {
                    format!("({} {} {})", l, op_str, r)
                }
            }
            MirRValue::UnaryOp { op, operand } => {
                let v = self.value_to_hlsl(operand, &func.locals);
                match op {
                    UnaryOp::Neg => format!("(-{})", v),
                    UnaryOp::Not => format!("(!{})", v),
                }
            }
            MirRValue::Aggregate { kind, operands } => {
                match kind {
                    AggregateKind::Struct(name) => {
                        // HLSL struct initialization
                        let fields: Vec<String> = operands.iter()
                            .map(|o| self.value_to_hlsl(o, &func.locals))
                            .collect();

                        // Map quanta_vec types to HLSL constructors
                        match name.as_ref() {
                            "quanta_vec2" => format!("float2({})", fields.join(", ")),
                            "quanta_vec3" => format!("float3({})", fields.join(", ")),
                            "quanta_vec4" => format!("float4({})", fields.join(", ")),
                            _ => format!("{{ {} }}", fields.join(", ")),
                        }
                    }
                    _ => "0".to_string(),
                }
            }
            MirRValue::FieldAccess { base, field_name, .. } => {
                let b = self.value_to_hlsl(base, &func.locals);
                format!("{}.{}", b, field_name)
            }
            MirRValue::Cast { value, ty, .. } => {
                let v = self.value_to_hlsl(value, &func.locals);
                let t = self.type_to_hlsl(ty);
                format!("({})({})", t, v)
            }
            _ => "0".to_string(),
        }
    }

    fn value_to_hlsl(&self, value: &MirValue, locals: &[MirLocal]) -> String {
        match value {
            MirValue::Local(id) => {
                // Check for inlined expression (single-use temp)
                if let Some(expr) = self.inlined_exprs.borrow().get(&id.0) {
                    return expr.clone();
                }
                self.local_name(*id, locals)
            }
            MirValue::Const(c) => self.const_to_hlsl(c),
            MirValue::Global(name) | MirValue::Function(name) => {
                // Map C runtime functions to HLSL equivalents
                match name.as_ref() {
                    // Math
                    "fabs" => "abs".to_string(),
                    // Vector math
                    "quanta_dot2" | "quanta_dot3" | "quanta_dot4" => "dot".to_string(),
                    "quanta_normalize2" | "quanta_normalize3" | "quanta_normalize4" => "normalize".to_string(),
                    "quanta_length2" | "quanta_length3" | "quanta_length4" => "length".to_string(),
                    "quanta_cross" => "cross".to_string(),
                    "quanta_reflect3" => "reflect".to_string(),
                    // Interpolation
                    "quanta_mix" | "quanta_lerp2" | "quanta_lerp3" | "quanta_lerp4" => "lerp".to_string(),
                    // Shader math
                    "quanta_clampf" | "quanta_clamp3" => "clamp".to_string(),
                    "quanta_smoothstep" => "smoothstep".to_string(),
                    "quanta_fract" => "frac".to_string(),
                    "quanta_step" => "step".to_string(),
                    // Min/max
                    "quanta_min_f64" | "quanta_min_f32" | "quanta_min_i32" | "quanta_min_i64" => "min".to_string(),
                    "quanta_max_f64" | "quanta_max_f32" | "quanta_max_i32" | "quanta_max_i64" => "max".to_string(),
                    // Texture sampling
                    "texture_sample" | "quanta_texture_sample" => "tex2D".to_string(),
                    // Vector constructors (mapped by aggregate, but handle if called as function)
                    "quanta_vec2_new" => "float2".to_string(),
                    "quanta_vec3_new" => "float3".to_string(),
                    "quanta_vec4_new" => "float4".to_string(),
                    // Matrix
                    "quanta_mat4_mul" | "quanta_mat4_mul_vec4" => "mul".to_string(),
                    // Strip quanta_ prefix for functions that map directly
                    _ => name.to_string(),
                }
            }
        }
    }

    fn const_to_hlsl(&self, c: &MirConst) -> String {
        match c {
            MirConst::Bool(true) => "true".to_string(),
            MirConst::Bool(false) => "false".to_string(),
            MirConst::Int(v, _) => format!("{}", v),
            MirConst::Uint(v, _) => format!("{}u", v),
            MirConst::Float(v, ty) => {
                let suffix = if matches!(ty, MirType::Float(FloatSize::F32)) { "f" } else { "" };
                if v.fract() == 0.0 { format!("{}.0{}", v, suffix) }
                else { format!("{}{}", v, suffix) }
            }
            MirConst::Unit => "0".to_string(),
            _ => "0".to_string(),
        }
    }

    /// Convert MIR type to HLSL type name.
    fn type_to_hlsl(&self, ty: &MirType) -> String {
        match ty {
            MirType::Void => "void".to_string(),
            MirType::Bool => "bool".to_string(),
            MirType::Int(size, signed) => {
                match (size, signed) {
                    (IntSize::I32, true) => "int".to_string(),
                    (IntSize::I32, false) => "uint".to_string(),
                    _ => "int".to_string(), // HLSL primarily uses int/uint
                }
            }
            MirType::Float(FloatSize::F32) => "float".to_string(),
            MirType::Float(FloatSize::F64) => "float".to_string(), // HLSL uses float (no double in SM5)
            MirType::Struct(name) => {
                // Map QuantaLang built-in types to HLSL types
                match name.as_ref() {
                    "quanta_vec2" => "float2".to_string(),
                    "quanta_vec3" => "float3".to_string(),
                    "quanta_vec4" => "float4".to_string(),
                    "quanta_mat4" => "float4x4".to_string(),
                    "QuantaString" => "string".to_string(), // Not really valid in HLSL
                    _ => name.to_string(),
                }
            }
            MirType::Ptr(_) => "int".to_string(), // No pointers in HLSL
            MirType::Array(elem, len) => {
                format!("{}[{}]", self.type_to_hlsl(elem), len)
            }
            MirType::Texture2D(_) => "Texture2D".to_string(),
            MirType::Sampler => "SamplerState".to_string(),
            _ => "float".to_string(),
        }
    }

    /// Generate a function call expression, with special handling for intrinsics.
    fn generate_call_expr(&self, callee: &MirValue, args: &[MirValue], locals: &[MirLocal]) -> String {
        // Check for tex2d — insert backbuffer as first argument
        let callee_name = match callee {
            MirValue::Function(name) | MirValue::Global(name) => Some(name.as_ref()),
            _ => None,
        };
        let callee_str = self.value_to_hlsl(callee, locals);
        let arg_strs: Vec<String> = args.iter()
            .map(|a| self.value_to_hlsl(a, locals))
            .collect();

        match callee_name {
            Some("tex2d") => {
                // tex2d(uv) → tex2D(ReShade::BackBuffer, uv)
                format!("tex2D(ReShade::BackBuffer, {})", arg_strs.join(", "))
            }
            Some("tex2d_depth") => {
                // tex2d_depth(uv) → ReShade::GetLinearizedDepth(uv)
                format!("ReShade::GetLinearizedDepth({})", arg_strs.join(", "))
            }
            _ => {
                format!("{}({})", callee_str, arg_strs.join(", "))
            }
        }
    }

    /// Find the return value expression from any Return terminator in the function.
    fn find_return_value(&self, blocks: &[MirBlock], locals: &[MirLocal]) -> Option<String> {
        for block in blocks {
            if let Some(MirTerminator::Return(Some(val))) = &block.terminator {
                return Some(self.value_to_hlsl(val, locals));
            }
        }
        None
    }

    fn local_name(&self, id: LocalId, locals: &[MirLocal]) -> String {
        locals.iter()
            .find(|l| l.id == id)
            .and_then(|l| l.name.as_ref())
            .map(|n| {
                // Escape HLSL reserved words
                match n.as_ref() {
                    "input" | "output" | "register" | "texture" | "sampler" =>
                        format!("_{}", n),
                    _ => n.to_string(),
                }
            })
            .unwrap_or_else(|| format!("_{}", id.0))
    }
}
