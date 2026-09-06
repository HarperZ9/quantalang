// ===============================================================================
// BUILDLANG CODE GENERATOR - EXPRESSION LOWERING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! Expression, block, and statement lowering for MIR.

use std::collections::HashMap;
use std::sync::Arc;

use crate::ast::{self, BinOp as AstBinOp, ExprKind, Literal, StmtKind, UnaryOp as AstUnaryOp};

use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::values;
use crate::codegen::ir::*;
use crate::codegen::runtime;

use super::{MirLowerer, OverloadTarget};

/// Owned copy of a loop label for storage in `loop_stack`, or `None` for an
/// unlabeled loop. Kept as a free helper so every loop-lowering path records the
/// label the same way.
fn label_name(label: Option<&ast::Ident>) -> Option<String> {
    label.map(|l| l.as_str().to_string())
}

impl<'ctx> MirLowerer<'ctx> {
    // =========================================================================
    // BLOCK AND STATEMENT LOWERING
    // =========================================================================

    pub(crate) fn lower_block(&mut self, block: &ast::Block) -> CodegenResult<Option<MirValue>> {
        let mut result = None;

        for (i, stmt) in block.stmts.iter().enumerate() {
            let is_last = i == block.stmts.len() - 1;
            // Set the span cursor so every MIR statement/terminator this
            // top-level AST statement lowers to (there may be several, e.g.
            // a `let x = f(a, b);` lowers to argument temps plus the final
            // assign) is attributed to this statement's source range. This
            // is a coarser granularity than per-MIR-statement spans, but it
            // is the single choke point every statement passes through, so
            // it covers the whole function without touching the ~240
            // individual `builder.assign`/`push_*` call sites in lowering.
            if let Some(builder) = self.current_fn.as_mut() {
                builder.set_current_span(stmt.span);
            }
            result = self.lower_stmt(stmt, is_last)?;
        }

        Ok(result)
    }

    fn lower_stmt(&mut self, stmt: &ast::Stmt, is_tail: bool) -> CodegenResult<Option<MirValue>> {
        match &stmt.kind {
            StmtKind::Local(local) => {
                self.lower_local(local)?;
                Ok(None)
            }
            StmtKind::Expr(expr) => {
                let val = self.lower_expr(expr)?;
                if is_tail {
                    Ok(Some(val))
                } else {
                    Ok(None)
                }
            }
            StmtKind::Semi(expr) => {
                self.lower_expr(expr)?;
                Ok(None)
            }
            StmtKind::Item(item) => {
                self.lower_item(item)?;
                Ok(None)
            }
            StmtKind::Empty => Ok(None),
            StmtKind::Macro { path, tokens, .. } => {
                let macro_name = path
                    .segments
                    .last()
                    .map(|s| s.ident.name.as_ref())
                    .unwrap_or("");

                match macro_name {
                    "println" | "print" => {
                        self.lower_print_macro(tokens, macro_name == "println")?;
                        Ok(None)
                    }
                    "eprintln" | "eprint" => {
                        self.lower_print_macro(tokens, macro_name == "eprintln")?;
                        Ok(None)
                    }
                    "panic" => {
                        self.lower_panic_macro(tokens)?;
                        Ok(None)
                    }
                    "dbg" => {
                        self.lower_print_macro(tokens, true)?;
                        Ok(None)
                    }
                    _ => {
                        // Unknown macro -- skip with no output
                        Ok(None)
                    }
                }
            }
        }
    }

    fn lower_local(&mut self, local: &ast::Local) -> CodegenResult<()> {
        // Handle tuple destructuring: `let (a, b) = (expr1, expr2);`
        // We handle this before the general init lowering because we want to
        // lower each tuple element individually rather than as a single
        // aggregate value.
        if let ast::PatternKind::Tuple(patterns) = &local.pattern.kind {
            if let Some(init) = &local.init {
                // If the RHS is a tuple literal, destructure element-by-element
                // to avoid needing a named struct type for the aggregate.
                if let ExprKind::Tuple(elems) = &init.expr.kind {
                    if elems.len() == patterns.len() {
                        for (pat, elem_expr) in patterns.iter().zip(elems.iter()) {
                            if let ast::PatternKind::Ident { name, .. } = &pat.kind {
                                let val = self.lower_expr(elem_expr)?;
                                let elem_ty = self.type_of_value(&val);
                                let builder = self.current_fn.as_mut().ok_or_else(|| {
                                    CodegenError::Internal("No current function".to_string())
                                })?;
                                let local_id =
                                    builder.create_named_local(name.name.clone(), elem_ty);
                                builder.assign(local_id, MirRValue::Use(val));
                                self.var_map.insert(name.name.clone(), local_id);
                            }
                            // Wildcard patterns in tuple destructuring are silently ignored.
                        }
                        return Ok(());
                    }
                }

                // Fallback for non-literal tuples: lower the init expression
                // once, then extract fields via FieldAccess with `_0`, `_1`, etc.
                let init_v = self.lower_expr(&init.expr)?;
                let init_ty = self.type_of_value(&init_v);
                for (i, pat) in patterns.iter().enumerate() {
                    if let ast::PatternKind::Ident { name, .. } = &pat.kind {
                        // Extract element type from MirType::Tuple if available
                        let elem_ty = if let MirType::Tuple(ref elems) = init_ty {
                            elems.get(i).cloned().unwrap_or(MirType::i32())
                        } else {
                            MirType::i32()
                        };
                        let builder = self.current_fn.as_mut().ok_or_else(|| {
                            CodegenError::Internal("No current function".to_string())
                        })?;
                        let local_id =
                            builder.create_named_local(name.name.clone(), elem_ty.clone());
                        let field_name: Arc<str> = Arc::from(format!("_{}", i));
                        builder.assign(
                            local_id,
                            MirRValue::FieldAccess {
                                base: init_v.clone(),
                                field_name,
                                field_ty: elem_ty,
                            },
                        );
                        self.var_map.insert(name.name.clone(), local_id);
                    }
                }
                return Ok(());
            }
            return Ok(());
        }

        // Special case `let scratch = workgroupArray(N)`: bind the name DIRECTLY
        // to the workgroup-shared slot local instead of lowering the call to a
        // fresh slot and then array-COPYING it into the binding (which the SPIR-V
        // backend cannot express for a Workgroup-class variable). The slot local
        // carries the `Workgroup:N` annotation and is what `scratch[..]` indexes.
        if let ast::PatternKind::Ident { name, .. } = &local.pattern.kind {
            if let Some(init) = &local.init {
                if let ExprKind::Call { func, args } = &init.expr.kind {
                    if self.extract_call_name(func) == Some("workgroupArray") && args.len() == 1 {
                        let slot_val = self.lower_workgroup_array(&args[0])?;
                        if let MirValue::Local(slot) = slot_val {
                            // Give the slot the binding's source name for legible
                            // diagnostics, then alias the name to it.
                            if let Some(builder) = self.current_fn.as_mut() {
                                builder.set_local_name(slot, name.name.clone());
                            }
                            self.var_map.insert(name.name.clone(), slot);
                            return Ok(());
                        }
                    }
                }
            }
        }

        // Compute type from annotation if present
        let explicit_ty = local.ty.as_ref().map(|t| self.lower_type_from_ast(t));

        // Propagate expected type to init expression lowering so that
        // constructor calls (HashMap::new, Vec::new, etc.) can use the
        // annotation type instead of falling back to i32.
        let prev_expected = self.expected_type.take();
        if let Some(ref ty) = explicit_ty {
            self.expected_type = Some(ty.clone());
        }

        // Compute init value if present
        let init_val = if let Some(init) = &local.init {
            Some(self.lower_expr(&init.expr)?)
        } else {
            None
        };

        // Restore previous expected type
        self.expected_type = prev_expected;

        // Determine final type: explicit annotation > inferred from init > i32 fallback
        let ty = if let Some(ref t) = explicit_ty {
            t.clone()
        } else if let Some(ref val) = init_val {
            self.type_of_value(val)
        } else {
            MirType::i32() // Last-resort fallback when no annotation and no initializer
        };

        // Coerce float constants to match the declared type.
        // When `let a: f32 = 1.0;`, the literal produces f64 but we need f32.
        let init_val = init_val.map(|val| {
            if let (Some(ref exp_ty), MirValue::Const(MirConst::Float(v, _))) = (&explicit_ty, &val)
            {
                if matches!(exp_ty, MirType::Float(FloatSize::F32)) {
                    return MirValue::Const(MirConst::Float(*v, MirType::f32()));
                }
            }
            val
        });

        // Track Option<T> inner type from AST type annotation BEFORE borrowing builder.
        // When the user writes `let x: Option<HashMap<K,V>> = ...`,
        // extract the inner type so pattern matching can bind correctly.
        let option_inner = if let Some(ref ast_ty) = local.ty {
            if let ast::TypeKind::Path(ref path) = ast_ty.kind {
                let is_option = path
                    .last_ident()
                    .map(|i| i.name.as_ref() == "Option")
                    .unwrap_or(false);
                if is_option {
                    path.last_generics().and_then(|args| {
                        if let Some(ast::GenericArg::Type(ref inner_ast_ty)) = args.first() {
                            Some(self.lower_type_from_ast(inner_ast_ty))
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Track Result<Ok, Err> Ok/Err payload types from the annotation so a
        // later `match r { Ok(x) => ..., Err(e) => ... }` reads the correct
        // union slots.
        let result_ok = if let Some(ref ast_ty) = local.ty {
            self.result_ok_inner_from_ast(ast_ty)
        } else {
            None
        };
        let result_err = if let Some(ref ast_ty) = local.ty {
            self.result_err_inner_from_ast(ast_ty)
        } else {
            None
        };

        // Now borrow current_fn and use it
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        // Create local for the binding
        if let ast::PatternKind::Ident { name, .. } = &local.pattern.kind {
            let local_id = builder.create_named_local(name.name.clone(), ty.clone());
            self.var_map.insert(name.name.clone(), local_id);

            // Record the Option inner type if extracted from annotation
            if let Some(inner_ty) = option_inner {
                self.option_inner_types.insert(local_id, inner_ty);
            }
            // Record the Result Ok/Err payload types if extracted from annotation
            if let Some(ok_ty) = result_ok {
                self.result_ok_types.insert(local_id, ok_ty);
            }
            if let Some(err_ty) = result_err {
                self.result_err_types.insert(local_id, err_ty);
            }

            // Initialize if there's an init expression
            if let Some(ref val) = init_val {
                builder.assign(local_id, MirRValue::Use(val.clone()));
            }

            // If the init value came from a closure local, propagate the
            // closure-name mapping so that calls through this variable can
            // look up captures.
            if let Some(ref val) = init_val {
                if let MirValue::Local(src_id) = val {
                    if let Some(cname) = self.local_closure_name.get(src_id).cloned() {
                        self.local_closure_name.insert(local_id, cname);
                    }
                }
            }
        }

        Ok(())
    }

    // =========================================================================
    // EXPRESSION LOWERING
    // =========================================================================

    pub(crate) fn lower_expr(&mut self, expr: &ast::Expr) -> CodegenResult<MirValue> {
        match &expr.kind {
            ExprKind::Literal(lit) => self.lower_literal(lit),
            ExprKind::Ident(ident) => self.lower_ident(ident),
            ExprKind::Path(path) => self.lower_path(path),

            ExprKind::Binary { op, left, right } => self.lower_binary(*op, left, right),
            ExprKind::Unary { op, expr: inner } => self.lower_unary(*op, inner),
            ExprKind::Assign { op, target, value } => self.lower_assign(*op, target, value),

            ExprKind::Call { func, args } => self.lower_call(func, args),
            ExprKind::MethodCall {
                receiver,
                method,
                args,
                ..
            } => self.lower_method_call(receiver, method, args),

            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => self.lower_if(condition, then_branch, else_branch.as_deref()),
            ExprKind::IfLet {
                pattern,
                expr: scrutinee,
                then_branch,
                else_branch,
            } => self.lower_if_let(pattern, scrutinee, then_branch, else_branch.as_deref()),
            ExprKind::Match { scrutinee, arms } => self.lower_match(scrutinee, arms),

            ExprKind::Loop { body, label } => self.lower_loop(body, label.as_ref()),
            ExprKind::While {
                condition,
                body,
                label,
            } => self.lower_while(condition, body, label.as_ref()),
            ExprKind::WhileLet {
                pattern,
                expr: scrutinee,
                body,
                label,
            } => self.lower_while_let(pattern, scrutinee, body, label.as_ref()),
            ExprKind::For {
                pattern,
                iter,
                body,
                label,
            } => self.lower_for(pattern, iter, body, label.as_ref()),

            ExprKind::Block(block) => {
                // Explicit blocks create a new scope -- save and restore
                // var_map so `let` bindings don't leak outward.
                let saved_vars = self.var_map.clone();
                let result = self.lower_block(block)?;
                self.var_map = saved_vars;
                Ok(result.unwrap_or(values::unit()))
            }

            ExprKind::Return(value) => self.lower_return(value.as_deref()),
            ExprKind::Break { value, label } => self.lower_break(value.as_deref(), label.as_ref()),
            ExprKind::Continue { label } => self.lower_continue(label.as_ref()),

            ExprKind::Tuple(elems) => self.lower_tuple(elems),
            ExprKind::Array(elems) => self.lower_array(elems),
            ExprKind::ArrayRepeat { element, count } => self.lower_array_repeat(element, count),
            ExprKind::Index { expr: arr, index } => self.lower_index(arr, index),
            ExprKind::Field { expr: obj, field } => self.lower_field(obj, field),
            ExprKind::TupleField {
                expr: inner, index, ..
            } => self.lower_tuple_field(inner, *index),

            ExprKind::Ref {
                mutability,
                expr: inner,
            } => self.lower_ref(*mutability, inner),
            ExprKind::Deref(inner) => self.lower_deref(inner),

            ExprKind::Cast { expr: inner, ty } => self.lower_cast(inner, ty),

            ExprKind::Paren(inner) => self.lower_expr(inner),

            ExprKind::Closure {
                params,
                return_type,
                body,
                ..
            } => self.lower_closure(params, return_type.as_deref(), body),

            ExprKind::Struct { path, fields, rest } => {
                self.lower_struct_expr(path, fields, rest.as_deref())
            }

            ExprKind::Handle {
                effect,
                handlers,
                body,
            } => self.lower_handle(effect, handlers, body),
            ExprKind::Resume(value) => self.lower_resume(value.as_deref()),
            ExprKind::Perform {
                effect,
                operation,
                args,
            } => self.lower_perform(effect, operation, args),

            ExprKind::Macro { path, tokens, .. } => {
                // Expand macro expressions (println!, print!, etc.)
                let macro_name = path
                    .segments
                    .last()
                    .map(|s| s.ident.name.as_ref())
                    .unwrap_or("");

                match macro_name {
                    "println" | "print" => {
                        self.lower_print_macro(tokens, macro_name == "println")?;
                        Ok(values::unit())
                    }
                    "eprintln" | "eprint" => {
                        self.lower_print_macro(tokens, macro_name == "eprintln")?;
                        Ok(values::unit())
                    }
                    "panic" => {
                        self.lower_panic_macro(tokens)?;
                        Ok(values::unit())
                    }
                    "dbg" => {
                        self.lower_print_macro(tokens, true)?;
                        Ok(values::unit())
                    }
                    "format" => {
                        // format! builds an owned BuildString from the template
                        // and arguments (was a stub that dropped the arguments).
                        self.lower_format_macro(tokens)
                    }
                    "vec" => self.lower_vec_macro(tokens),
                    _ => {
                        // Unknown macro expression - return unit
                        Ok(values::unit())
                    }
                }
            }

            ExprKind::Try(inner) => self.lower_try(inner),

            _ => {
                // Unsupported expression - return unit
                Ok(values::unit())
            }
        }
    }

    fn lower_literal(&mut self, lit: &Literal) -> CodegenResult<MirValue> {
        match lit {
            Literal::Int { value, suffix, .. } => {
                let (ty, signed) = suffix
                    .as_ref()
                    .map(|s| match s {
                        ast::IntSuffix::I8 => (MirType::i8(), true),
                        ast::IntSuffix::I16 => (MirType::i16(), true),
                        ast::IntSuffix::I32 => (MirType::i32(), true),
                        ast::IntSuffix::I64 => (MirType::i64(), true),
                        ast::IntSuffix::I128 => (MirType::Int(IntSize::I128, true), true),
                        ast::IntSuffix::Isize => (MirType::isize(), true),
                        ast::IntSuffix::U8 => (MirType::u8(), false),
                        ast::IntSuffix::U16 => (MirType::u16(), false),
                        ast::IntSuffix::U32 => (MirType::u32(), false),
                        ast::IntSuffix::U64 => (MirType::u64(), false),
                        ast::IntSuffix::U128 => (MirType::Int(IntSize::I128, false), false),
                        ast::IntSuffix::Usize => (MirType::usize(), false),
                    })
                    .unwrap_or_else(|| {
                        // Unsuffixed: default to i32, but widen for values that
                        // exceed it so the literal is not silently truncated
                        // (mirrors the type checker's literal widening).
                        if *value > i64::MAX as u128 {
                            (MirType::Int(IntSize::I128, true), true)
                        } else if *value > i32::MAX as u128 {
                            (MirType::i64(), true)
                        } else {
                            (MirType::i32(), true)
                        }
                    });

                if signed {
                    Ok(MirValue::Const(MirConst::Int(*value as i128, ty)))
                } else {
                    Ok(MirValue::Const(MirConst::Uint(*value as u128, ty)))
                }
            }
            Literal::Float { value, suffix } => {
                let ty = suffix
                    .as_ref()
                    .map(|s| match s {
                        ast::FloatSuffix::F16 | ast::FloatSuffix::F32 => MirType::f32(),
                        ast::FloatSuffix::F64 => MirType::f64(),
                    })
                    .unwrap_or(MirType::f64());
                Ok(MirValue::Const(MirConst::Float(*value, ty)))
            }
            Literal::Bool(b) => Ok(values::bool(*b)),
            Literal::Char(c) => Ok(MirValue::Const(MirConst::Uint(*c as u128, MirType::u32()))),
            Literal::Str { value, .. } => {
                let idx = self.module.intern_string(value.clone());
                // Wrap string literal in build_string_new() to produce a BuildString
                let builder = self.current_fn.as_mut().ok_or_else(|| {
                    CodegenError::Internal("No current function for string literal".to_string())
                })?;
                let result = builder.create_local(MirType::Struct(Arc::from("BuildString")));
                let cont = builder.create_block();
                let func = MirValue::Function(Arc::from("build_string_new"));
                let str_ptr = MirValue::Const(MirConst::Str(idx));
                builder.call(func, vec![str_ptr], Some(result), cont);
                builder.switch_to_block(cont);
                Ok(values::local(result))
            }
            Literal::ByteStr { value, .. } => Ok(MirValue::Const(MirConst::ByteStr(value.clone()))),
            Literal::Byte(b) => Ok(MirValue::Const(MirConst::Uint(*b as u128, MirType::u8()))),
        }
    }

    pub(crate) fn lower_ident(&mut self, ident: &ast::Ident) -> CodegenResult<MirValue> {
        if let Some(&local) = self.var_map.get(&ident.name) {
            Ok(values::local(local))
        } else {
            // Check for math constants
            match ident.name.as_ref() {
                "PI" => Ok(MirValue::Const(MirConst::Float(
                    std::f64::consts::PI,
                    MirType::f64(),
                ))),
                "E" => Ok(MirValue::Const(MirConst::Float(
                    std::f64::consts::E,
                    MirType::f64(),
                ))),
                "TAU" => Ok(MirValue::Const(MirConst::Float(
                    std::f64::consts::TAU,
                    MirType::f64(),
                ))),
                // Might be a global or function - inside inline modules,
                // try the module-prefixed name first so local definitions
                // shadow parent-scope functions (use super::*).
                _ => {
                    // A bare reference to a unit struct (or Self inside a
                    // unit-struct impl) constructs the unit value, as in
                    // fn new() -> Self { Self }.
                    if let Some(name) = self.resolve_unit_struct_name(&ident.name) {
                        let builder = self.current_fn.as_mut().ok_or_else(|| {
                            CodegenError::Internal(
                                "No current function for unit struct value".to_string(),
                            )
                        })?;
                        let result = builder.create_local(MirType::Struct(name.clone()));
                        builder.aggregate(result, AggregateKind::Struct(name), Vec::new());
                        return Ok(values::local(result));
                    }
                    let resolved = self.resolve_fn_name(&ident.name);
                    Ok(values::global(resolved))
                }
            }
        }
    }

    /// If `name` (after resolving `Self` and the module prefix) refers to a
    /// unit struct (zero fields), return its concrete codegen name. Used so a
    /// bare value-position reference constructs the unit value.
    fn resolve_unit_struct_name(&self, name: &Arc<str>) -> Option<Arc<str>> {
        let mut candidate: Arc<str> = if name.as_ref() == "Self" {
            self.current_impl_type.clone()?
        } else {
            name.clone()
        };
        if !self.module_prefix.is_empty() {
            let prefixed = self.prefixed_name(&candidate);
            if self.module.find_type(prefixed.as_ref()).is_some() {
                candidate = prefixed;
            }
        }
        match self.module.find_type(candidate.as_ref()) {
            Some(td) => match &td.kind {
                TypeDefKind::Struct { fields, .. } if fields.is_empty() => Some(candidate),
                _ => None,
            },
            None => None,
        }
    }

    fn lower_path(&mut self, path: &ast::Path) -> CodegenResult<MirValue> {
        if path.is_simple() {
            if let Some(ident) = path.last_ident() {
                return self.lower_ident(ident);
            }
        }

        // Check for enum variant path (e.g. Shape::Unit for unit variant)
        if path.segments.len() == 2 {
            let enum_name = &path.segments[0].ident.name;
            let variant_name = &path.segments[1].ident.name;

            // For generic enums (e.g. Option::None), find the most recently
            // monomorphized specialization so the unit variant uses the correct type.
            let resolved_name = if self.generic_enums.contains_key(enum_name.as_ref()) {
                self.find_monomorphized_enum(enum_name)
                    .unwrap_or_else(|| enum_name.clone())
            } else {
                enum_name.clone()
            };

            if self.is_enum_type(&resolved_name) {
                // Unit variant construction (no payload)
                let disc = self
                    .lookup_enum_variant(&resolved_name, variant_name)
                    .map(|(d, _)| d)
                    .unwrap_or(0);

                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

                let result = builder.create_local(MirType::Struct(resolved_name.clone()));
                builder.aggregate(
                    result,
                    AggregateKind::Variant(
                        resolved_name.clone(),
                        disc as u32,
                        variant_name.clone(),
                    ),
                    Vec::new(),
                );

                return Ok(values::local(result));
            }
        }

        // Handle well-known type constants (f64::INFINITY, i32::MAX, etc.)
        if path.segments.len() == 2 {
            let type_seg = path.segments[0].ident.name.as_ref();
            let const_seg = path.segments[1].ident.name.as_ref();
            match (type_seg, const_seg) {
                ("f64", "INFINITY") => return Ok(values::global("INFINITY".to_string())),
                ("f64", "NEG_INFINITY") => return Ok(values::global("(-INFINITY)".to_string())),
                ("f64", "NAN") => return Ok(values::global("NAN".to_string())),
                ("f64", "MIN") => {
                    return Ok(values::global("(-1.7976931348623157e+308)".to_string()))
                }
                ("f64", "MAX") => return Ok(values::global("1.7976931348623157e+308".to_string())),
                ("f64", "EPSILON") => {
                    return Ok(values::global("2.2204460492503131e-16".to_string()))
                }
                ("f64", "MIN_POSITIVE") => {
                    return Ok(values::global("2.2250738585072014e-308".to_string()))
                }
                ("f32", "INFINITY") => return Ok(values::global("INFINITY".to_string())),
                ("f32", "NEG_INFINITY") => return Ok(values::global("(-INFINITY)".to_string())),
                ("f32", "NAN") => return Ok(values::global("NAN".to_string())),
                ("i32", "MIN") => return Ok(values::global("INT32_MIN".to_string())),
                ("i32", "MAX") => return Ok(values::global("INT32_MAX".to_string())),
                ("i64", "MIN") => return Ok(values::global("INT64_MIN".to_string())),
                ("i64", "MAX") => return Ok(values::global("INT64_MAX".to_string())),
                ("u32", "MAX") => return Ok(values::global("UINT32_MAX".to_string())),
                ("u64", "MAX") => return Ok(values::global("UINT64_MAX".to_string())),
                ("usize", "MAX") => return Ok(values::global("SIZE_MAX".to_string())),
                _ => {}
            }
        }

        // Complex path - treat as module-qualified reference.
        // Join segments with `_` so that `mod::func` resolves to the
        // mangled name `mod_func` produced by the module loader.
        let name = path
            .segments
            .iter()
            .map(|s| s.ident.name.as_ref())
            .collect::<Vec<_>>()
            .join("_");
        // Inside a module, check if the prefixed name exists in the module.
        // e.g., `Vec3_new` → `std_Vec3_new` when inside `mod math`.
        if !self.module_prefix.is_empty() {
            let prefixed = self.prefixed_name(&Arc::from(name.as_str()));
            if self.module.find_function(prefixed.as_ref()).is_some() {
                return Ok(values::global(prefixed.to_string()));
            }
        }
        Ok(values::global(name))
    }

    fn lower_binary(
        &mut self,
        op: AstBinOp,
        left: &ast::Expr,
        right: &ast::Expr,
    ) -> CodegenResult<MirValue> {
        // Handle special cases that don't need the builder first
        match op {
            AstBinOp::And | AstBinOp::Or => {
                // Short-circuit evaluation needed
                return self.lower_logical_op(op, left, right);
            }
            AstBinOp::Pipe => {
                // Pipe operator: x |> f => f(x)
                return self.lower_call(right, &[left.clone()]);
            }
            AstBinOp::Range | AstBinOp::RangeInclusive => {
                // Range operators - for now, just return left
                return self.lower_expr(left);
            }
            AstBinOp::Compose => {
                // Compose operator: f >> g => g(f(...))
                return self.lower_expr(right);
            }
            _ => {}
        }

        // Compute operands BEFORE borrowing current_fn
        let left_val = self.lower_expr(left)?;
        let right_val = self.lower_expr(right)?;

        // Coerce f64 constants to f32 when the other operand is f32.
        // This handles cases like `1.0 - x` where x is f32 but `1.0` defaults to f64.
        let (left_val, right_val) = {
            let lt = self.type_of_value(&left_val);
            let rt = self.type_of_value(&right_val);
            let is_f32 = |t: &MirType| matches!(t, MirType::Float(FloatSize::F32));
            let coerce_to_f32 = |v: MirValue| -> MirValue {
                if let MirValue::Const(MirConst::Float(fv, _)) = &v {
                    MirValue::Const(MirConst::Float(*fv, MirType::f32()))
                } else {
                    v
                }
            };
            if is_f32(&lt) && !is_f32(&rt) {
                (left_val, coerce_to_f32(right_val))
            } else if !is_f32(&lt) && is_f32(&rt) {
                (coerce_to_f32(left_val), right_val)
            } else {
                (left_val, right_val)
            }
        };

        // Check if operands are strings (BuildString) for special operator handling
        let left_ty = self.type_of_value(&left_val);
        let is_string_op =
            matches!(&left_ty, MirType::Struct(name) if name.as_ref() == "BuildString");

        // String concatenation: `+` on BuildString -> build_string_concat()
        if is_string_op && op == AstBinOp::Add {
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let result = builder.create_local(MirType::Struct(Arc::from("BuildString")));
            let cont = builder.create_block();
            let func = MirValue::Function(Arc::from("build_string_concat"));
            builder.call(func, vec![left_val, right_val], Some(result), cont);
            builder.switch_to_block(cont);
            return Ok(values::local(result));
        }

        // String comparison: `==` on BuildString -> build_string_eq()
        if is_string_op && op == AstBinOp::Eq {
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let result = builder.create_local(MirType::Bool);
            let cont = builder.create_block();
            let func = MirValue::Function(Arc::from("build_string_eq"));
            builder.call(func, vec![left_val, right_val], Some(result), cont);
            builder.switch_to_block(cont);
            return Ok(values::local(result));
        }

        // String inequality: `!=` on BuildString -> !build_string_eq()
        if is_string_op && op == AstBinOp::Ne {
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let eq_result = builder.create_local(MirType::Bool);
            let cont = builder.create_block();
            let func = MirValue::Function(Arc::from("build_string_eq"));
            builder.call(func, vec![left_val, right_val], Some(eq_result), cont);
            builder.switch_to_block(cont);
            // Negate the result: != is !eq
            let neg_result = builder.create_local(MirType::Bool);
            builder.unary_op(neg_result, UnaryOp::Not, values::local(eq_result));
            return Ok(values::local(neg_result));
        }

        // Broadcasting operators (I4): `.+ .- .* ./` over fixed-size `Array<T,N>`.
        // These desugar to per-element scalar MIR ops and NEVER become MIR ops or
        // reach a backend, so they must be intercepted here (before the `match op`
        // that would hit `unreachable!`). Length agreement is already guaranteed by
        // the type checker; here we only need one array operand's compile-time N.
        if matches!(
            op,
            AstBinOp::DotAdd | AstBinOp::DotSub | AstBinOp::DotMul | AstBinOp::DotDiv
        ) {
            return self.lower_broadcast(op, left_val, right_val);
        }

        // Vector arithmetic: +, -, * on build_vecN types
        if let MirType::Struct(ref name) = left_ty {
            if let Some(n) = Self::vec_component_count(name) {
                let op_suffix = match op {
                    AstBinOp::Add => Some("add"),
                    AstBinOp::Sub => Some("sub"),
                    AstBinOp::Mul => Some("mul"),
                    _ => None,
                };
                if let Some(suffix) = op_suffix {
                    let c_func = format!("build_vec{}_{}", n, suffix);
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(left_ty.clone());
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from(c_func.as_str()));
                    builder.call(func, vec![left_val, right_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }
            }
        }

        // Mat4 multiplication: mat4 * mat4 or mat4 * vec4
        if op == AstBinOp::Mul {
            if let MirType::Struct(ref name) = left_ty {
                if name.as_ref() == "build_mat4" {
                    let right_ty = self.type_of_value(&right_val);
                    let (c_func, ret_ty) = if let MirType::Struct(ref rname) = right_ty {
                        if rname.as_ref() == "build_mat4" {
                            ("build_mat4_mul", MirType::Struct(Arc::from("build_mat4")))
                        } else if rname.as_ref() == "build_vec4" {
                            (
                                "build_mat4_mul_vec4",
                                MirType::Struct(Arc::from("build_vec4")),
                            )
                        } else {
                            ("build_mat4_mul", left_ty.clone())
                        }
                    } else {
                        ("build_mat4_mul", left_ty.clone())
                    };
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(ret_ty);
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from(c_func));
                    builder.call(func, vec![left_val, right_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }
            }
        }

        // --- Generic operator overloading for user-defined types ---
        // If the left operand is a struct with a matching operator method
        // (add, sub, mul, div, eq, ne, lt, gt, le, ge), dispatch to it.
        if let MirType::Struct(ref type_name) = left_ty {
            let method_name = match op {
                AstBinOp::Add => Some("add"),
                AstBinOp::Sub => Some("sub"),
                AstBinOp::Mul => Some("mul"),
                AstBinOp::Div => Some("div"),
                AstBinOp::Rem => Some("rem"),
                AstBinOp::Eq => Some("eq"),
                AstBinOp::Ne => Some("ne"),
                AstBinOp::Lt => Some("lt"),
                AstBinOp::Gt => Some("gt"),
                AstBinOp::Le => Some("le"),
                AstBinOp::Ge => Some("ge"),
                _ => None,
            };
            if let Some(method) = method_name {
                let key = (type_name.clone(), Arc::from(method));
                if let Some(mangled_fn) = self.impl_methods.get(&key).cloned() {
                    let ret_ty = self
                        .module
                        .find_function(mangled_fn.as_ref())
                        .map(|f| f.sig.ret.clone())
                        .unwrap_or(left_ty.clone());
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(ret_ty);
                    let cont = builder.create_block();
                    let func = MirValue::Function(mangled_fn);
                    builder.call(func, vec![left_val, right_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }
            }
        }

        let mir_op = match op {
            AstBinOp::Add => BinOp::Add,
            AstBinOp::Sub => BinOp::Sub,
            AstBinOp::Mul => BinOp::Mul,
            AstBinOp::Div => BinOp::Div,
            AstBinOp::Rem => BinOp::Rem,
            AstBinOp::BitAnd => BinOp::BitAnd,
            AstBinOp::BitOr => BinOp::BitOr,
            AstBinOp::BitXor => BinOp::BitXor,
            AstBinOp::Shl => BinOp::Shl,
            AstBinOp::Shr => BinOp::Shr,
            AstBinOp::Eq => BinOp::Eq,
            AstBinOp::Ne => BinOp::Ne,
            AstBinOp::Lt => BinOp::Lt,
            AstBinOp::Le => BinOp::Le,
            AstBinOp::Gt => BinOp::Gt,
            AstBinOp::Ge => BinOp::Ge,
            AstBinOp::Pow => BinOp::Pow,
            _ => unreachable!("handled above"),
        };

        // Compute result type before borrowing the builder mutably
        let result_ty = self.binary_result_type(mir_op, &left_val, &right_val);

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        let result = builder.create_local(result_ty);
        builder.binary_op(result, mir_op, left_val, right_val);
        Ok(values::local(result))
    }

    /// Lower an elementwise broadcasting operator (`.+ .- .* ./`) over a
    /// fixed-size `Array<T,N>` into an unrolled aggregate of per-element scalar
    /// MIR ops. At least one operand is `MirType::Array(elem, N)` (guaranteed by
    /// the type checker); the other is either the same-length array or a scalar
    /// broadcast over every element. The `Dot*` op maps to the scalar MIR op and
    /// never reaches a backend.
    fn lower_broadcast(
        &mut self,
        op: AstBinOp,
        left_val: MirValue,
        right_val: MirValue,
    ) -> CodegenResult<MirValue> {
        let mir_op = match op {
            AstBinOp::DotAdd => BinOp::Add,
            AstBinOp::DotSub => BinOp::Sub,
            AstBinOp::DotMul => BinOp::Mul,
            AstBinOp::DotDiv => BinOp::Div,
            _ => unreachable!("lower_broadcast only handles Dot* operators"),
        };

        let left_ty = self.type_of_value(&left_val);
        let right_ty = self.type_of_value(&right_val);
        let left_arr = matches!(left_ty, MirType::Array(_, _));
        let right_arr = matches!(right_ty, MirType::Array(_, _));

        // Recover the compile-time length N and element type from whichever
        // operand is an array. The type checker has already required matching
        // lengths for array-array broadcasts.
        let (n, elem_ty) = match (&left_ty, &right_ty) {
            (MirType::Array(elem, n), _) => (*n, (**elem).clone()),
            (_, MirType::Array(elem, n)) => (*n, (**elem).clone()),
            _ => {
                return Err(CodegenError::Internal(
                    "broadcast operator applied to non-array operands".to_string(),
                ))
            }
        };

        let mut elem_vals = Vec::with_capacity(n as usize);
        for k in 0..n {
            let idx = MirValue::Const(MirConst::Int(k as i128, MirType::i64()));
            let lhs = self.broadcast_operand(&left_val, left_arr, &idx, &elem_ty);
            let rhs = self.broadcast_operand(&right_val, right_arr, &idx, &elem_ty);
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let elem = builder.create_local(elem_ty.clone());
            builder.binary_op(elem, mir_op, lhs, rhs);
            elem_vals.push(values::local(elem));
        }

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let result = builder.create_local(MirType::Array(Box::new(elem_ty.clone()), n));
        builder.aggregate(result, AggregateKind::Array(elem_ty), elem_vals);
        Ok(values::local(result))
    }

    /// Produce the per-element value for one operand of a broadcast at index
    /// `idx`: an `IndexAccess` read for an array operand, or the scalar itself
    /// (reused for every element) for a scalar operand.
    fn broadcast_operand(
        &mut self,
        operand: &MirValue,
        is_array: bool,
        idx: &MirValue,
        elem_ty: &MirType,
    ) -> MirValue {
        if !is_array {
            return operand.clone();
        }
        let builder = self
            .current_fn
            .as_mut()
            .expect("broadcast_operand requires a current function");
        let slot = builder.create_local(elem_ty.clone());
        builder.assign(
            slot,
            MirRValue::IndexAccess {
                base: operand.clone(),
                index: idx.clone(),
                elem_ty: elem_ty.clone(),
            },
        );
        values::local(slot)
    }

    fn lower_logical_op(
        &mut self,
        op: AstBinOp,
        left: &ast::Expr,
        right: &ast::Expr,
    ) -> CodegenResult<MirValue> {
        // Lower left expression FIRST before borrowing builder
        let left_val = self.lower_expr(left)?;

        // Now set up blocks and result
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        let result = builder.create_local(MirType::Bool);
        let eval_right = builder.create_block();
        let short_circuit = builder.create_block();
        let merge = builder.create_block();

        match op {
            AstBinOp::And => {
                // if left { evaluate right } else { short-circuit to false }
                builder.branch(left_val, eval_right, short_circuit);

                builder.switch_to_block(short_circuit);
                builder.assign_const(result, MirConst::Bool(false));
                builder.goto(merge);
            }
            AstBinOp::Or => {
                // if left { short-circuit to true } else { evaluate right }
                builder.branch(left_val, short_circuit, eval_right);

                builder.switch_to_block(short_circuit);
                builder.assign_const(result, MirConst::Bool(true));
                builder.goto(merge);
            }
            _ => unreachable!(),
        }

        // Evaluate right-hand side in eval_right block
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(eval_right);
        }

        // Lower right expression
        let right_val = self.lower_expr(right)?;

        // Assign right value to result and jump to merge
        let builder = self.current_fn.as_mut().unwrap();
        builder.assign(result, MirRValue::Use(right_val));
        builder.goto(merge);
        builder.switch_to_block(merge);

        Ok(values::local(result))
    }

    fn lower_unary(&mut self, op: AstUnaryOp, inner: &ast::Expr) -> CodegenResult<MirValue> {
        // Lower inner expression FIRST before borrowing builder
        let inner_val = self.lower_expr(inner)?;

        // Constant folding: -literal → negative constant (no local needed).
        // This prevents creating unreachable locals for fallback return values
        // like `fn find(...) -> i32 { while ... { return x; } -1 }`.
        if matches!(op, AstUnaryOp::Neg) {
            match &inner_val {
                MirValue::Const(MirConst::Int(v, ty)) => {
                    return Ok(MirValue::Const(MirConst::Int(-v, ty.clone())));
                }
                MirValue::Const(MirConst::Float(v, ty)) => {
                    return Ok(MirValue::Const(MirConst::Float(-v, ty.clone())));
                }
                _ => {}
            }
        }

        // Vector negation: -vec -> build_vecN_neg(vec)
        if matches!(op, AstUnaryOp::Neg) {
            let inner_ty = self.type_of_value(&inner_val);
            if let MirType::Struct(ref name) = inner_ty {
                if let Some(n) = Self::vec_component_count(name) {
                    let c_func = format!("build_vec{}_neg", n);
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(inner_ty.clone());
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from(c_func.as_str()));
                    builder.call(func, vec![inner_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }
            }
        }

        let mir_op = match op {
            AstUnaryOp::Neg => UnaryOp::Neg,
            AstUnaryOp::Not | AstUnaryOp::BitNot => UnaryOp::Not,
            AstUnaryOp::Deref => {
                // Dereference: emit a proper deref rvalue
                let pointee_ty = match self.type_of_value(&inner_val) {
                    MirType::Ptr(inner) => *inner,
                    _ => MirType::i32(),
                };
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let result = builder.create_local(pointee_ty.clone());
                builder.assign(
                    result,
                    MirRValue::Deref {
                        ptr: inner_val,
                        pointee_ty,
                    },
                );
                return Ok(values::local(result));
            }
            AstUnaryOp::Ref | AstUnaryOp::RefMut => {
                // Reference: emit address-of
                let inner_ty = self.type_of_value(&inner_val);
                let local = match &inner_val {
                    MirValue::Local(id) => *id,
                    _ => {
                        let builder = self.current_fn.as_mut().ok_or_else(|| {
                            CodegenError::Internal("No current function".to_string())
                        })?;
                        let temp = builder.create_local(inner_ty.clone());
                        builder.assign(temp, MirRValue::Use(inner_val));
                        temp
                    }
                };
                let is_mut = matches!(op, AstUnaryOp::RefMut);
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let result = builder.create_local(MirType::Ptr(Box::new(inner_ty)));
                builder.make_ref(result, is_mut, MirPlace::local(local));
                return Ok(values::local(result));
            }
        };

        // Compute result type before borrowing the builder mutably
        let result_ty = self.type_of_value(&inner_val);

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        let result = builder.create_local(result_ty);
        builder.unary_op(result, mir_op, inner_val);
        Ok(values::local(result))
    }

    fn lower_assign(
        &mut self,
        op: ast::AssignOp,
        target: &ast::Expr,
        value: &ast::Expr,
    ) -> CodegenResult<MirValue> {
        let val = self.lower_expr(value)?;

        // Handle dereference assignment: `*ptr = value`
        if let ExprKind::Deref(inner) = &target.kind {
            let ptr_val = self.lower_expr(inner)?;
            let ptr_local = match &ptr_val {
                MirValue::Local(id) => *id,
                _ => {
                    let ptr_ty = self.type_of_value(&ptr_val);
                    let builder = self.current_fn.as_mut().unwrap();
                    let temp = builder.create_local(ptr_ty);
                    builder.assign(temp, MirRValue::Use(ptr_val));
                    temp
                }
            };

            let builder = self.current_fn.as_mut().unwrap();
            builder.push_deref_assign(ptr_local, MirRValue::Use(val));
            return Ok(values::unit());
        }

        // Handle field assignment: `obj.field = value`
        if let ExprKind::Field { expr: obj, field } = &target.kind {
            let obj_val = self.lower_expr(obj)?;
            let obj_ty = self.type_of_value(&obj_val);

            let obj_local = match &obj_val {
                MirValue::Local(id) => *id,
                _ => {
                    let builder = self.current_fn.as_mut().unwrap();
                    let temp = builder.create_local(obj_ty.clone());
                    builder.assign(temp, MirRValue::Use(obj_val));
                    temp
                }
            };

            if obj_ty.is_pointer() {
                // Pointer-to-struct field assignment: emit ptr->field = value
                let builder = self.current_fn.as_mut().unwrap();
                builder.push_field_deref_assign(obj_local, field.name.clone(), MirRValue::Use(val));
            } else {
                // Local struct field assignment: emit base.field = value
                let builder = self.current_fn.as_mut().unwrap();
                builder.push_field_assign(obj_local, field.name.clone(), MirRValue::Use(val));
            }
            return Ok(values::unit());
        }

        // Handle indexed assignment into a Vec: `v[i] = value`. Without this the
        // write fell through every target arm and was silently dropped (the
        // element kept its old value). Dispatches to a typed runtime setter.
        if let ExprKind::Index { expr: arr, index } = &target.kind {
            let recv_val = self.lower_expr(arr)?;
            let recv_ty = self.type_of_value(&recv_val);
            if let MirType::Vec(ref elem_ty) = recv_ty {
                let suffix = match elem_ty.as_ref() {
                    MirType::Float(_) => "f64",
                    MirType::Int(IntSize::I64, _) => "i64",
                    MirType::Struct(n) if n.as_ref() == "BuildString" => "str",
                    _ => "i32",
                };
                let idx_val = self.lower_expr(index)?;
                // For a compound assignment, read the current element, apply the
                // op, then store; a plain `=` stores the value directly.
                let store_val = if op == ast::AssignOp::Assign {
                    val
                } else {
                    let get_fn =
                        MirValue::Function(Arc::from(format!("build_hvec_get_{}", suffix)));
                    let builder = self.current_fn.as_mut().unwrap();
                    let cur = builder.create_local((**elem_ty).clone());
                    let cont = builder.create_block();
                    builder.call(
                        get_fn,
                        vec![recv_val.clone(), idx_val.clone()],
                        Some(cur),
                        cont,
                    );
                    builder.switch_to_block(cont);
                    let bin_op = match op {
                        ast::AssignOp::AddAssign => BinOp::Add,
                        ast::AssignOp::SubAssign => BinOp::Sub,
                        ast::AssignOp::MulAssign => BinOp::Mul,
                        ast::AssignOp::DivAssign => BinOp::Div,
                        ast::AssignOp::RemAssign => BinOp::Rem,
                        ast::AssignOp::BitAndAssign => BinOp::BitAnd,
                        ast::AssignOp::BitOrAssign => BinOp::BitOr,
                        ast::AssignOp::BitXorAssign => BinOp::BitXor,
                        ast::AssignOp::ShlAssign => BinOp::Shl,
                        ast::AssignOp::ShrAssign => BinOp::Shr,
                        _ => BinOp::Add,
                    };
                    let combined = builder.create_local((**elem_ty).clone());
                    builder.binary_op(combined, bin_op, values::local(cur), val);
                    values::local(combined)
                };
                let set_fn = MirValue::Function(Arc::from(format!("build_hvec_set_{}", suffix)));
                let builder = self.current_fn.as_mut().unwrap();
                let cont = builder.create_block();
                builder.call(set_fn, vec![recv_val, idx_val, store_val], None, cont);
                builder.switch_to_block(cont);
                return Ok(values::unit());
            }

            // Indexed store into a slice / array / pointer base: `base[i] = v`.
            // A slice/array *parameter* has type `Ptr(Slice/Array(elem))`
            // (references pass as pointers); unwrap to the element type. Without
            // this arm the write fell through to the bare-ident logic below and
            // was silently dropped for slice parameters (e.g. a GPU kernel's
            // `out[i] = ...`).
            let elem_ty = match &recv_ty {
                MirType::Slice(elem) | MirType::Array(elem, _) => Some((**elem).clone()),
                MirType::Ptr(inner) => match inner.as_ref() {
                    MirType::Slice(elem) | MirType::Array(elem, _) => Some((**elem).clone()),
                    _ => None,
                },
                _ => None,
            };
            if let Some(elem_ty) = elem_ty {
                let idx_val = self.lower_expr(index)?;
                // Compound assignment reads the current element first.
                let store_val = if op == ast::AssignOp::Assign {
                    MirRValue::Use(val)
                } else {
                    let bin_op = match op {
                        ast::AssignOp::AddAssign => BinOp::Add,
                        ast::AssignOp::SubAssign => BinOp::Sub,
                        ast::AssignOp::MulAssign => BinOp::Mul,
                        ast::AssignOp::DivAssign => BinOp::Div,
                        ast::AssignOp::RemAssign => BinOp::Rem,
                        ast::AssignOp::BitAndAssign => BinOp::BitAnd,
                        ast::AssignOp::BitOrAssign => BinOp::BitOr,
                        ast::AssignOp::BitXorAssign => BinOp::BitXor,
                        ast::AssignOp::ShlAssign => BinOp::Shl,
                        ast::AssignOp::ShrAssign => BinOp::Shr,
                        _ => BinOp::Add,
                    };
                    let builder = self.current_fn.as_mut().unwrap();
                    let cur = builder.create_local(elem_ty.clone());
                    builder.assign(
                        cur,
                        MirRValue::IndexAccess {
                            base: recv_val.clone(),
                            index: idx_val.clone(),
                            elem_ty: elem_ty.clone(),
                        },
                    );
                    let combined = builder.create_local(elem_ty.clone());
                    builder.binary_op(combined, bin_op, values::local(cur), val);
                    MirRValue::Use(values::local(combined))
                };
                let builder = self.current_fn.as_mut().unwrap();
                builder.push_index_store(recv_val, idx_val, elem_ty, store_val);
                return Ok(values::unit());
            }
        }

        // Get the target local
        let target_local = match &target.kind {
            ExprKind::Ident(ident) => self.var_map.get(&ident.name).copied(),
            _ => None,
        };

        if let Some(local) = target_local {
            let builder = self.current_fn.as_mut().unwrap();

            if op == ast::AssignOp::Assign {
                builder.assign(local, MirRValue::Use(val));
            } else {
                // Compound assignment
                let bin_op = match op {
                    ast::AssignOp::AddAssign => BinOp::Add,
                    ast::AssignOp::SubAssign => BinOp::Sub,
                    ast::AssignOp::MulAssign => BinOp::Mul,
                    ast::AssignOp::DivAssign => BinOp::Div,
                    ast::AssignOp::RemAssign => BinOp::Rem,
                    ast::AssignOp::BitAndAssign => BinOp::BitAnd,
                    ast::AssignOp::BitOrAssign => BinOp::BitOr,
                    ast::AssignOp::BitXorAssign => BinOp::BitXor,
                    ast::AssignOp::ShlAssign => BinOp::Shl,
                    ast::AssignOp::ShrAssign => BinOp::Shr,
                    _ => BinOp::Add,
                };
                builder.binary_op(local, bin_op, values::local(local), val);
            }
        } else if let ExprKind::Ident(ident) = &target.kind {
            // Deref and field assignments were handled above, so a bare-identifier
            // target that resolves to no local is a module global / mutable static
            // (anything else would have been rejected by the type checker). MIR
            // `Assign` targets only locals, so the write is represented as a
            // `GlobalStore` (the C backend emits `NAME = value;`; the drop analysis
            // treats it as an escape). Compound assignment to a global reads the
            // current value, applies the op, and stores back.
            let name = ident.name.clone();
            let store_val = if op == ast::AssignOp::Assign {
                MirRValue::Use(val)
            } else {
                let bin_op = match op {
                    ast::AssignOp::AddAssign => BinOp::Add,
                    ast::AssignOp::SubAssign => BinOp::Sub,
                    ast::AssignOp::MulAssign => BinOp::Mul,
                    ast::AssignOp::DivAssign => BinOp::Div,
                    ast::AssignOp::RemAssign => BinOp::Rem,
                    ast::AssignOp::BitAndAssign => BinOp::BitAnd,
                    ast::AssignOp::BitOrAssign => BinOp::BitOr,
                    _ => BinOp::Add,
                };
                let val_ty = self.type_of_value(&val);
                let builder = self.current_fn.as_mut().unwrap();
                let cur = builder.create_local(val_ty.clone());
                builder.assign(cur, MirRValue::Use(MirValue::Global(name.clone())));
                let tmp = builder.create_local(val_ty);
                builder.binary_op(tmp, bin_op, values::local(cur), val);
                MirRValue::Use(values::local(tmp))
            };
            let builder = self.current_fn.as_mut().unwrap();
            builder.push_global_store(name, store_val);
        }

        Ok(values::unit())
    }

    fn lower_call(&mut self, func: &ast::Expr, args: &[ast::Expr]) -> CodegenResult<MirValue> {
        // Check for enum variant construction: Shape::Circle(5.0)
        if let Some((enum_name, variant_name)) = self.try_resolve_enum_variant_path(func) {
            return self.lower_enum_variant_construct(&enum_name, &variant_name, args);
        }

        // Check for vector constructor calls: vec2(x,y), vec3(x,y,z), vec4(x,y,z,w)
        if let Some(callee_name) = self.extract_call_name(func) {
            match callee_name {
                "vec2" if args.len() == 2 => return self.lower_vec_constructor(2, args),
                "vec3" if args.len() == 3 => return self.lower_vec_constructor(3, args),
                "vec4" if args.len() == 4 => return self.lower_vec_constructor(4, args),
                // workgroupBarrier() -> a void GPU side effect (not an rvalue):
                // synchronize the workgroup and make Workgroup-class writes
                // visible. Lowered to a WorkgroupBarrier statement the SPIR-V
                // backend turns into OpControlBarrier.
                "workgroupBarrier" if args.is_empty() => {
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    builder.push_workgroup_barrier();
                    return Ok(values::unit());
                }
                // workgroupArray(N) -> a fresh workgroup-shared scratch array of
                // N f32 elements. Represented as a MIR local of type
                // `Array(f32, N)` tagged with a `Workgroup:N` annotation the
                // SPIR-V backend reads to place it in the `Workgroup` storage
                // class. Phase 4a fixes N to the workgroup size (64); a
                // non-constant / non-64 length is rejected so the emitted array
                // length always matches the launch geometry.
                "workgroupArray" if args.len() == 1 => {
                    return self.lower_workgroup_array(&args[0]);
                }
                // texture_sample(texture, sampler, uv) -> vec4
                "texture_sample" if args.len() == 3 => {
                    let tex = self.lower_expr(&args[0])?;
                    let samp = self.lower_expr(&args[1])?;
                    let coords = self.lower_expr(&args[2])?;
                    let result_ty = MirType::Struct(Arc::from("build_vec4"));
                    let builder = self.current_fn.as_mut().unwrap();
                    let dest = builder.create_local(result_ty);
                    builder.assign(
                        dest,
                        MirRValue::TextureSample {
                            texture: tex,
                            sampler: samp,
                            coords: coords,
                        },
                    );
                    return Ok(MirValue::Local(dest));
                }
                _ => {}
            }
        }

        // Check for mat4 builtins that return specific types.
        if let Some(callee_name) = self.extract_call_name(func) {
            if let Some(dispatched) = self.try_dispatch_mat4_builtin(callee_name, args) {
                return dispatched;
            }
        }

        // Multiple dispatch FIRST: a name with 2+ definitions (generic and/or
        // concrete) is resolved by the argument-type tuple through the SAME
        // resolver the type checker used. This MUST run before the plain
        // generic-call path below: a name that mixes a generic def and a
        // concrete def is overloaded, and the checker may pick EITHER sibling
        // (concrete-beats-generic when it fits, generic otherwise). Running the
        // generic path first would always monomorphize the generic def and
        // ignore the concrete winner -> checker/codegen divergence (miscompile).
        // - Concrete winner: emit a direct call to its mangled name.
        // - Generic winner: fall through to `lower_generic_call` (monomorphized).
        if let Some(callee_name) = self.extract_call_name(func) {
            let resolved = self.resolve_fn_name(callee_name);
            if self.is_overloaded_fn(resolved.as_ref()) {
                let arg_tys: Vec<MirType> =
                    args.iter().map(|a| self.infer_single_arg_type(a)).collect();
                match self.resolve_overloaded_call_name(resolved.as_ref(), &arg_tys) {
                    Some(OverloadTarget::Concrete(mangled)) => {
                        let ret_ty = self
                            .module
                            .find_function(mangled.as_ref())
                            .map(|f| f.sig.ret.clone())
                            .unwrap_or(MirType::i32());
                        let arg_vals: Vec<_> = args
                            .iter()
                            .map(|a| self.lower_expr(a))
                            .collect::<CodegenResult<_>>()?;
                        let builder = self.current_fn.as_mut().ok_or_else(|| {
                            CodegenError::Internal("No current function".to_string())
                        })?;
                        let cont = builder.create_block();
                        if matches!(ret_ty, MirType::Void) {
                            builder.call(MirValue::Function(mangled), arg_vals, None, cont);
                            builder.switch_to_block(cont);
                            return Ok(values::unit());
                        } else {
                            let result = builder.create_local(ret_ty);
                            builder.call(MirValue::Function(mangled), arg_vals, Some(result), cont);
                            builder.switch_to_block(cont);
                            return Ok(values::local(result));
                        }
                    }
                    Some(OverloadTarget::Generic) => {
                        // The selected sibling is the generic def; monomorphize it
                        // on demand via the shared generic-call path.
                        return self.lower_generic_call(func, args);
                    }
                    // Resolution failed (checker already reported it): fall through
                    // to the generic / normal path to avoid a cascading error.
                    None => {}
                }
            }
        }

        // Check for generic function call - monomorphize if needed.
        // Inside inline modules, also check the prefixed name. This handles
        // generic-ONLY names (no concrete sibling); overloaded names that select
        // the generic def already routed here via the multi-dispatch block above.
        if let Some(fn_name) = self.extract_call_name(func) {
            let resolved = self.resolve_fn_name(fn_name);
            if self.generic_functions.contains_key(fn_name)
                || self.generic_functions.contains_key(resolved.as_ref())
            {
                return self.lower_generic_call(func, args);
            }
        }

        // Check for vector math builtins that need type-based dispatch.
        // dot/normalize/length/lerp dispatch to the correct size variant
        // based on the first argument's type.
        if let Some(callee_name) = self.extract_call_name(func) {
            if let Some(dispatched) = self.try_dispatch_vector_builtin(callee_name, args) {
                return dispatched;
            }
        }

        // Element-generic Vec builtins (vec_push/vec_get/vec_pop) dispatch to
        // the correctly-typed runtime helper based on the receiver vec's
        // element type, instead of the i32 default baked into math_builtin_to_c.
        if let Some(callee_name) = self.extract_call_name(func) {
            if let Some(dispatched) = self.try_dispatch_vec_builtin(callee_name, args) {
                return dispatched;
            }
        }

        // Check for math built-in functions and rewrite the call target to the
        // corresponding C / runtime function name - but only if there is no
        // user-defined function with the same name in the module.
        let func_val = if let Some(builtin_name) = self.try_resolve_math_builtin(func) {
            // Don't shadow a user-defined function with the same name.
            // Inside inline modules, also check the prefixed name, and resolve
            // qualified module paths (`math_utils::dot`) to their mangled
            // function name. Without the qualified-path case a user module
            // function whose last segment matches a math builtin (dot, length,
            // ...) is hijacked to the runtime builtin, which takes the wrong
            // argument type.
            let user_defined = self.call_resolves_to_user_fn(func);
            if user_defined {
                self.lower_expr(func)?
            } else {
                MirValue::Function(Arc::from(builtin_name))
            }
        } else {
            self.lower_expr(func)?
        };

        // Try to resolve the function's return type from declared signatures
        let ret_ty = self.resolve_call_return_type(func);

        let mut arg_vals: Vec<_> = args
            .iter()
            .map(|a| self.lower_expr(a))
            .collect::<CodegenResult<_>>()?;

        // ---- Coerce BuildString args to raw char* for FFI calls ----
        // When calling an extern "C" function that expects `const char*`
        // (`Ptr(i8)`), but the argument is a BuildString (from a string
        // literal), extract the `.ptr` field automatically.
        if let Some(fn_name) = self.extract_call_name(func) {
            if let Some(target_fn) = self.module.find_function(fn_name) {
                if target_fn.is_declaration() && target_fn.sig.calling_conv == CallingConv::C {
                    let param_types: Vec<MirType> = target_fn.sig.params.clone();
                    for (i, arg_val) in arg_vals.iter_mut().enumerate() {
                        if i < param_types.len() {
                            if matches!(&param_types[i], MirType::Ptr(inner) if matches!(inner.as_ref(), MirType::Int(IntSize::I8, true)))
                            {
                                let arg_ty = self.type_of_value(arg_val);
                                if let MirType::Struct(ref name) = arg_ty {
                                    if name.as_ref() == "BuildString" {
                                        if let MirValue::Local(local_id) = arg_val {
                                            let builder = self.current_fn.as_mut().unwrap();
                                            let ptr_local = builder.create_local(MirType::Ptr(
                                                Box::new(MirType::i8()),
                                            ));
                                            builder.assign(
                                                ptr_local,
                                                MirRValue::FieldAccess {
                                                    base: MirValue::Local(*local_id),
                                                    field_name: Arc::from("ptr"),
                                                    field_ty: MirType::Ptr(Box::new(MirType::i8())),
                                                },
                                            );
                                            *arg_val = values::local(ptr_local);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // ---- Coerce BuildString args to char* for builtin I/O calls ----
        // Runtime builtins like read_file, write_file, file_exists expect
        // const char* but the lowerer passes BuildString values.
        if let Some(fn_name) = self.extract_call_name(func) {
            let needs_str_coerce = matches!(
                fn_name,
                "read_file"
                    | "write_file"
                    | "file_exists"
                    | "read_bytes"
                    | "write_bytes"
                    | "append_file"
                    | "build_vk_load_shader_file"
                    | "build_vk_run_compute"
                    | "map_insert"
                    | "map_get"
                    | "map_contains"
                    | "map_remove"
                    | "list_dir"
                    | "is_dir"
                    | "file_size"
                    | "tcp_connect"
                    | "tcp_send"
                    | "getenv"
                    | "model_complete"
            );
            if needs_str_coerce {
                for arg_val in arg_vals.iter_mut() {
                    let arg_ty = self.type_of_value(arg_val);
                    if let MirType::Struct(ref name) = arg_ty {
                        if name.as_ref() == "BuildString" {
                            if let MirValue::Local(local_id) = arg_val {
                                let builder = self.current_fn.as_mut().unwrap();
                                let ptr_local =
                                    builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
                                builder.assign(
                                    ptr_local,
                                    MirRValue::FieldAccess {
                                        base: MirValue::Local(*local_id),
                                        field_name: Arc::from("ptr"),
                                        field_ty: MirType::Ptr(Box::new(MirType::i8())),
                                    },
                                );
                                *arg_val = values::local(ptr_local);
                            }
                        }
                    }
                }
            }
        }

        // ---- Append captured values for capturing closures ----
        // If the callee is a local that holds a capturing closure, look up
        // the captured variable locals and append them as extra arguments.
        if let ExprKind::Ident(ident) = &func.kind {
            if let Some(&callee_local) = self.var_map.get(&ident.name) {
                if let Some(closure_name) = self.local_closure_name.get(&callee_local).cloned() {
                    if let Some(captures) = self.closure_captures.get(&closure_name) {
                        for (_, cap_local) in captures.iter() {
                            arg_vals.push(values::local(*cap_local));
                        }
                    }
                }
            }
        }

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let cont = builder.create_block();

        // If the function returns void, don't create a result local or
        // assign the call result - just emit a void call.
        if matches!(ret_ty, MirType::Void) {
            builder.call(func_val, arg_vals, None, cont);
            builder.switch_to_block(cont);
            Ok(values::unit())
        } else {
            let result = builder.create_local(ret_ty);
            builder.call(func_val, arg_vals, Some(result), cont);
            builder.switch_to_block(cont);
            Ok(values::local(result))
        }
    }

    /// True when this call target resolves to a user-defined function in the
    /// current module, so a same-named math builtin must not shadow it. Handles
    /// bare names (via the module-prefix-aware `resolve_fn_name`) and qualified
    /// module paths like `math_utils::dot`, which the module loader mangles to
    /// `math_utils_dot`. `extract_call_name` returns `None` for a multi-segment
    /// path, so the joined name is resolved here directly.
    fn call_resolves_to_user_fn(&self, func: &ast::Expr) -> bool {
        // Bare name or single-segment path.
        if let Some(name) = self.extract_call_name(func) {
            let resolved = self.resolve_fn_name(name);
            return self.module.find_function(resolved.as_ref()).is_some();
        }
        // Qualified path: join segments with `_` to match the module loader's
        // mangling, and also try the module-prefixed form for calls made from
        // inside another inline module.
        if let ExprKind::Path(path) = &func.kind {
            if path.segments.len() > 1 {
                let joined: String = path
                    .segments
                    .iter()
                    .map(|s| s.ident.name.as_ref())
                    .collect::<Vec<_>>()
                    .join("_");
                if self.module.find_function(joined.as_str()).is_some() {
                    return true;
                }
                if !self.module_prefix.is_empty() {
                    let prefixed = self.prefixed_name(&Arc::from(joined.as_str()));
                    if self.module.find_function(prefixed.as_ref()).is_some() {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// If `func` resolves to a recognised math built-in (abs, sqrt, pow, ...),
    /// return the C function name to call instead.
    fn try_resolve_math_builtin(&self, func: &ast::Expr) -> Option<&'static str> {
        let name = match &func.kind {
            ExprKind::Ident(ident) => ident.name.as_ref(),
            ExprKind::Path(path) => {
                if let Some(ident) = path.last_ident() {
                    ident.name.as_ref()
                } else {
                    return None;
                }
            }
            _ => return None,
        };

        runtime::math_builtin_to_c(name)
    }

    /// Lower a vector constructor call: `vec2(x,y)`, `vec3(x,y,z)`, `vec4(x,y,z,w)`.
    /// Generates a struct aggregate for the corresponding `build_vecN` type.
    fn lower_vec_constructor(
        &mut self,
        components: u32,
        args: &[ast::Expr],
    ) -> CodegenResult<MirValue> {
        let struct_name = format!("build_vec{}", components);
        let mut operands = Vec::new();
        for arg in args {
            operands.push(self.lower_expr(arg)?);
        }
        let ty = MirType::Struct(Arc::from(struct_name.as_str()));
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let result = builder.create_local(ty.clone());
        builder.assign(
            result,
            MirRValue::Aggregate {
                kind: AggregateKind::Struct(Arc::from(struct_name.as_str())),
                operands,
            },
        );
        Ok(MirValue::Local(result))
    }

    /// Determine the vector component count from a struct type name.
    fn vec_component_count(struct_name: &str) -> Option<u32> {
        match struct_name {
            "build_vec2" => Some(2),
            "build_vec3" => Some(3),
            "build_vec4" => Some(4),
            _ => None,
        }
    }

    /// Dispatch the element-generic Vec builtins (`vec_push`, `vec_get`,
    /// `vec_pop`) to the correctly-typed runtime helper, keyed by the receiver
    /// vec's element type. Without this the generic names always resolved to the
    /// i32 helpers (`build_hvec_push_i32`, ...) via `math_builtin_to_c`, which
    /// truncated i64 elements to 32 bits, read f64/str vecs through the wrong
    /// accessor, and emitted a C type error for string elements. This mirrors
    /// the element-typed dispatch already used for the `v.push()`/`v.get()`
    /// method forms. The explicitly-typed names (`vec_push_i64`, `vec_get_str`,
    /// ...) are untouched; they still resolve through `math_builtin_to_c`.
    ///
    /// Returns `None` (deferring to normal handling) when the callee is not one
    /// of these builtins, when a user-defined function shadows the name, or when
    /// the first argument is not a `Vec` (which the type checker rejects for a
    /// real call, so that path is effectively unreachable for valid programs).
    fn try_dispatch_vec_builtin(
        &mut self,
        name: &str,
        args: &[ast::Expr],
    ) -> Option<CodegenResult<MirValue>> {
        let op = match name {
            "vec_push" | "vec_get" | "vec_pop" => name,
            _ => return None,
        };
        if args.is_empty() {
            return None;
        }
        // Don't shadow a user-defined function of the same name.
        let resolved = self.resolve_fn_name(name);
        if self.module.find_function(resolved.as_ref()).is_some() {
            return None;
        }

        // Lower the receiver (first arg) and read its element type.
        let recv_val = match self.lower_expr(&args[0]) {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        let recv_ty = self.type_of_value(&recv_val);
        let elem_ty = match &recv_ty {
            MirType::Vec(elem) => (**elem).clone(),
            _ => return None,
        };
        // Same element -> suffix mapping the method-call path uses, so the two
        // call forms stay in lockstep for every element type.
        let suffix: String = match &elem_ty {
            MirType::Float(_) => "f64".to_string(),
            MirType::Int(IntSize::I64, _) => "i64".to_string(),
            MirType::Struct(n) if n.as_ref() == "BuildString" => "str".to_string(),
            MirType::Struct(n) => n.to_string(),
            MirType::Vec(_) => "BuildVecHandle".to_string(),
            MirType::Map(_, _) => "BuildMapHandle".to_string(),
            _ => "i32".to_string(),
        };

        // Lower the remaining args (push value / get index).
        let mut arg_vals = vec![recv_val];
        for a in &args[1..] {
            match self.lower_expr(a) {
                Ok(v) => arg_vals.push(v),
                Err(e) => return Some(Err(e)),
            }
        }

        let (c_fn, ret_ty): (String, MirType) = match op {
            "vec_push" => (format!("build_hvec_push_{}", suffix), MirType::Void),
            "vec_get" => (format!("build_hvec_get_{}", suffix), elem_ty.clone()),
            "vec_pop" => (format!("build_hvec_pop_{}", suffix), elem_ty.clone()),
            _ => unreachable!(),
        };

        let builder = match self.current_fn.as_mut() {
            Some(b) => b,
            None => {
                return Some(Err(CodegenError::Internal(
                    "No current function".to_string(),
                )))
            }
        };
        let cont = builder.create_block();
        let func_val = MirValue::Function(Arc::from(c_fn.as_str()));
        if matches!(ret_ty, MirType::Void) {
            builder.call(func_val, arg_vals, None, cont);
            builder.switch_to_block(cont);
            Some(Ok(values::unit()))
        } else {
            let result = builder.create_local(ret_ty);
            builder.call(func_val, arg_vals, Some(result), cont);
            builder.switch_to_block(cont);
            Some(Ok(values::local(result)))
        }
    }

    /// Dispatch vector math builtins (dot, normalize, length, lerp, cross,
    /// reflect) to the correct size-specific C function based on the first
    /// argument's type.
    fn try_dispatch_vector_builtin(
        &mut self,
        name: &str,
        args: &[ast::Expr],
    ) -> Option<CodegenResult<MirValue>> {
        // Only handle known vector builtins
        match name {
            "dot" | "normalize" | "length" | "lerp" | "cross" | "reflect" => {}
            _ => return None,
        }

        // Lower the first argument to determine its type
        let first_arg = match self.lower_expr(&args[0]) {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        let first_ty = self.type_of_value(&first_arg);

        let n = if let MirType::Struct(ref sname) = first_ty {
            Self::vec_component_count(sname)
        } else {
            None
        };

        let n = match n {
            Some(n) => n,
            None => return None, // Not a vector type, fall through to normal handling
        };

        // Determine the C function name and return type
        let (c_func, ret_ty) = match name {
            "dot" => (format!("build_dot{}", n), MirType::f64()),
            "normalize" => (format!("build_normalize{}", n), first_ty.clone()),
            "length" => (format!("build_length{}", n), MirType::f64()),
            "cross" => {
                if n != 3 {
                    return None;
                }
                ("build_cross".to_string(), first_ty.clone())
            }
            "reflect" => {
                if n != 3 {
                    return None;
                }
                ("build_reflect3".to_string(), first_ty.clone())
            }
            "lerp" => (format!("build_lerp{}", n), first_ty.clone()),
            _ => return None,
        };

        // Lower remaining arguments
        let mut arg_vals = vec![first_arg];
        for arg in &args[1..] {
            match self.lower_expr(arg) {
                Ok(v) => arg_vals.push(v),
                Err(e) => return Some(Err(e)),
            }
        }

        let builder = match self.current_fn.as_mut() {
            Some(b) => b,
            None => {
                return Some(Err(CodegenError::Internal(
                    "No current function".to_string(),
                )))
            }
        };
        let result = builder.create_local(ret_ty);
        let cont = builder.create_block();
        let func = MirValue::Function(Arc::from(c_func.as_str()));
        builder.call(func, arg_vals, Some(result), cont);
        builder.switch_to_block(cont);
        Some(Ok(values::local(result)))
    }

    /// Dispatch mat4 builtins (mat4_identity, mat4_translate, mat4_scale,
    /// mat4_perspective) to the correct C runtime function with the proper
    /// return type.
    fn try_dispatch_mat4_builtin(
        &mut self,
        name: &str,
        args: &[ast::Expr],
    ) -> Option<CodegenResult<MirValue>> {
        let (c_func, ret_ty) = match name {
            "mat4_identity" => (
                "build_mat4_identity",
                MirType::Struct(Arc::from("build_mat4")),
            ),
            "mat4_translate" => (
                "build_mat4_translate",
                MirType::Struct(Arc::from("build_mat4")),
            ),
            "mat4_scale" => ("build_mat4_scale", MirType::Struct(Arc::from("build_mat4"))),
            "mat4_perspective" => (
                "build_mat4_perspective",
                MirType::Struct(Arc::from("build_mat4")),
            ),
            _ => return None,
        };

        // Lower arguments
        let mut arg_vals = Vec::new();
        for arg in args {
            match self.lower_expr(arg) {
                Ok(v) => arg_vals.push(v),
                Err(e) => return Some(Err(e)),
            }
        }

        let builder = match self.current_fn.as_mut() {
            Some(b) => b,
            None => {
                return Some(Err(CodegenError::Internal(
                    "No current function".to_string(),
                )))
            }
        };
        let result = builder.create_local(ret_ty);
        let cont = builder.create_block();
        let func = MirValue::Function(Arc::from(c_func));
        builder.call(func, arg_vals, Some(result), cont);
        builder.switch_to_block(cont);
        Some(Ok(values::local(result)))
    }

    /// Attempt to resolve the return type of a function call by looking up
    /// already-lowered function declarations in the module.
    /// Convert a MirType to a C-safe identifier suffix for monomorphized functions.
    fn type_to_c_name(ty: &MirType) -> String {
        match ty {
            MirType::Int(IntSize::I32, true) => "int32_t".to_string(),
            MirType::Int(IntSize::I64, true) => "int64_t".to_string(),
            MirType::Int(IntSize::ISize, false) => "uintptr_t".to_string(),
            MirType::Float(FloatSize::F64) => "double".to_string(),
            MirType::Float(FloatSize::F32) => "float".to_string(),
            MirType::Bool => "bool".to_string(),
            MirType::Struct(name) => name.replace("::", "_").replace(" ", "_"),
            MirType::Vec(_) => "BuildVecHandle".to_string(),
            MirType::Map(_, _) => "BuildStrF64MapHandle".to_string(),
            _ => "int32_t".to_string(),
        }
    }

    /// Best-effort static type inference for an AST expression, used to drive
    /// out i32 defaults at result-merge sites (match arms, etc.). Returns
    /// MirType::i32() as the "could not infer" sentinel, matching the
    /// surrounding lowering conventions.
    fn infer_expr_type(&self, expr: &ast::Expr) -> MirType {
        match &expr.kind {
            ExprKind::Literal(_) => self.infer_single_arg_type(expr),
            ExprKind::Paren(inner) => self.infer_expr_type(inner),
            ExprKind::Ident(ident) => {
                if let Some(&id) = self.var_map.get(&ident.name) {
                    if let Some(ref b) = self.current_fn {
                        if let Some(t) = b.local_type(id) {
                            return t;
                        }
                    }
                }
                MirType::i32()
            }
            ExprKind::Call { func, .. } => self.resolve_call_return_type(func),
            ExprKind::MethodCall {
                receiver, method, ..
            } => {
                let recv = self.infer_expr_type(receiver);
                match method.name.as_ref() {
                    "len" | "count" | "capacity" => MirType::usize(),
                    "is_some" | "is_none" | "is_ok" | "is_err" | "is_empty" | "contains"
                    | "contains_key" | "starts_with" | "ends_with" => MirType::Bool,
                    "clone" | "to_owned" => recv,
                    "get" | "get_mut" => match recv {
                        MirType::Map(_, v) => *v,
                        MirType::Vec(e) => *e,
                        _ => MirType::i32(),
                    },
                    "pop" | "pop_front" | "pop_back" | "first" | "last" | "front" | "back" => {
                        match recv {
                            MirType::Vec(e) => *e,
                            _ => MirType::i32(),
                        }
                    }
                    _ => MirType::i32(),
                }
            }
            ExprKind::Struct { path, .. } => {
                let name = path
                    .last_ident()
                    .map(|i| i.name.clone())
                    .unwrap_or_else(|| Arc::from(""));
                if let Some(q) = self.type_module_map.get(name.as_ref()) {
                    return MirType::Struct(q.clone());
                }
                MirType::Struct(name)
            }
            ExprKind::Field { expr: base, field } => {
                let mut bt = self.infer_expr_type(base);
                if let MirType::Ptr(inner) = bt {
                    bt = *inner;
                }
                if let MirType::Struct(sname) = bt {
                    if let Some(td) = self.module.find_type(sname.as_ref()) {
                        if let TypeDefKind::Struct { fields, .. } = &td.kind {
                            for (fname, fty) in fields {
                                if fname
                                    .as_deref()
                                    .map(|n| n == field.name.as_ref())
                                    .unwrap_or(false)
                                {
                                    return fty.clone();
                                }
                            }
                        }
                    }
                }
                MirType::i32()
            }
            ExprKind::If { then_branch, .. } => self.infer_block_type(then_branch),
            ExprKind::Match { arms, .. } => {
                for a in arms {
                    let t = self.infer_expr_type(&a.body);
                    if t != MirType::i32() && t != MirType::Void {
                        return t;
                    }
                }
                MirType::i32()
            }
            ExprKind::Block(b) => self.infer_block_type(b),
            _ => MirType::i32(),
        }
    }

    /// Type of a block: the type of its trailing expression statement, if any.
    fn infer_block_type(&self, block: &ast::Block) -> MirType {
        if let Some(last) = block.stmts.last() {
            if let StmtKind::Expr(ref e) = last.kind {
                return self.infer_expr_type(e);
            }
        }
        MirType::i32()
    }

    fn resolve_call_return_type(&self, func: &ast::Expr) -> MirType {
        let name = match &func.kind {
            ExprKind::Ident(ident) => {
                // Inside an inline module, try the prefixed name first so
                // that a local definition takes priority over a parent-scope
                // function imported via `use super::*`.
                Some(self.resolve_fn_name(&ident.name).to_string())
            }
            ExprKind::Path(path) => {
                // For module-qualified paths like `math::add`, join with `_`
                // to match the mangled name `math_add`.
                if path.segments.len() > 1 {
                    Some(
                        path.segments
                            .iter()
                            .map(|s| s.ident.name.as_ref())
                            .collect::<Vec<_>>()
                            .join("_"),
                    )
                } else {
                    path.last_ident()
                        .map(|i| self.resolve_fn_name(&i.name).to_string())
                }
            }
            _ => None,
        };
        if let Some(ref fn_name) = name {
            // Check module-level function declarations first.
            if let Some(func) = self.module.find_function(fn_name.as_str()) {
                return func.sig.ret.clone();
            }
            // Inside a module, also try the prefixed name (e.g., Vec3_new → std_Vec3_new)
            if !self.module_prefix.is_empty() {
                let prefixed = self.prefixed_name(&Arc::from(fn_name.as_str()));
                if let Some(func) = self.module.find_function(prefixed.as_ref()) {
                    return func.sig.ret.clone();
                }
            }
            // Check if the callee is a local variable with function pointer type
            // (e.g., a closure or higher-order function parameter).
            if let Some(&local_id) = self.var_map.get(fn_name.as_str()) {
                if let Some(ref builder) = self.current_fn {
                    if let Some(MirType::FnPtr(sig)) = builder.local_type(local_id) {
                        return sig.ret.clone();
                    }
                }
            }
        }
        // Check if it's a math/graphics builtin - these return f64
        if let Some(ref fn_name) = name {
            let is_f64_builtin = matches!(
                fn_name.as_str(),
                "sqrt"
                    | "sin"
                    | "cos"
                    | "tan"
                    | "pow"
                    | "abs"
                    | "floor"
                    | "ceil"
                    | "round"
                    | "min"
                    | "max"
                    | "clamp"
                    | "smoothstep"
                    | "mix"
                    | "fract"
                    | "step"
                    | "dot"
                    | "length"
                    | "lerp"
            );
            if is_f64_builtin {
                return MirType::f64();
            }
            // Vector constructors return struct types
            match fn_name.as_str() {
                "vec2" => return MirType::Struct(Arc::from("build_vec2")),
                "vec3" => return MirType::Struct(Arc::from("build_vec3")),
                "vec4" => return MirType::Struct(Arc::from("build_vec4")),
                "normalize" | "cross" | "reflect" => {
                    return MirType::Struct(Arc::from("build_vec3"))
                }
                // Texture sampling - returns vec4 (tex2d_depth returns f64 for single channel)
                "tex2d" | "texture_sample" => return MirType::Struct(Arc::from("build_vec4")),
                "tex2d_depth" => return MirType::f64(),
                "mat4_identity" | "mat4_translate" | "mat4_scale" | "mat4_perspective" => {
                    return MirType::Struct(Arc::from("build_mat4"));
                }
                // Vec builtins (i32 default)
                "vec_new" => return MirType::Vec(Box::new(MirType::i32())),
                "vec_get" => return MirType::i32(),
                "vec_len" => return MirType::i64(), // size_t
                "vec_pop" => return MirType::i32(),
                "vec_push" => return MirType::Void,
                // Vec builtins (f64)
                "vec_new_f64" => return MirType::Vec(Box::new(MirType::f64())),
                "vec_get_f64" => return MirType::f64(),
                "vec_pop_f64" => return MirType::f64(),
                "vec_push_f64" => return MirType::Void,
                // Vec builtins (i64)
                "vec_new_i64" => return MirType::Vec(Box::new(MirType::i64())),
                "vec_get_i64" => return MirType::i64(),
                "vec_pop_i64" => return MirType::i64(),
                "vec_push_i64" => return MirType::Void,
                // File I/O builtins
                "read_file" => return MirType::Struct(Arc::from("BuildString")),
                "write_file" => return MirType::Bool,
                "file_exists" => return MirType::Bool,
                // Binary file I/O builtins
                "read_bytes" => return MirType::Struct(Arc::from("BuildString")),
                "write_bytes" => return MirType::Bool,
                "append_file" => return MirType::Bool,
                // CLI / stdin builtins
                "args_count" => return MirType::i64(),
                "args_get" => return MirType::Struct(Arc::from("BuildString")),
                "read_line" => return MirType::Struct(Arc::from("BuildString")),
                "read_all" => return MirType::Struct(Arc::from("BuildString")),
                "stdin_is_pipe" => return MirType::Bool,
                // Process builtins
                "process_exit" => return MirType::Void,
                // Directory traversal builtins
                "list_dir" => {
                    return MirType::Vec(Box::new(MirType::Struct(Arc::from("BuildString"))))
                }
                "is_dir" => return MirType::Bool,
                "file_size" => return MirType::i64(),
                // String vec builtins
                "vec_new_str" => {
                    return MirType::Vec(Box::new(MirType::Struct(Arc::from("BuildString"))))
                }
                "vec_get_str" => return MirType::Struct(Arc::from("BuildString")),
                "vec_push_str" => return MirType::Void,
                // TCP socket builtins
                "tcp_connect" => return MirType::i64(),
                "tcp_send" => return MirType::i64(),
                "tcp_recv" => return MirType::Struct(Arc::from("BuildString")),
                "tcp_close" => return MirType::Void,
                // Environment variable builtins
                "getenv" => return MirType::Struct(Arc::from("BuildString")),
                // Clock / time builtins
                "clock_ms" | "time_unix" => return MirType::i64(),
                // Seeded random builtin
                "random_f64" => return MirType::f64(),
                // Model-call builtin
                "model_complete" => return MirType::Struct(Arc::from("BuildString")),
                // Format builtins
                "to_string_i32" | "to_string_f64" => {
                    return MirType::Struct(Arc::from("BuildString"))
                }
                // HashMap builtins (legacy i32->i32)
                "map_new_i32" => return MirType::Struct(Arc::from("BuildMapHandle")),
                "map_get_i32" => return MirType::i32(),
                "map_len_i32" => return MirType::i64(),
                "map_contains_i32" | "map_remove_i32" => return MirType::Bool,
                "map_insert_i32" => return MirType::Void,
                // HashMap builtins (str->f64, default)
                // Type::new() constructor calls (from path resolution)
                "HashMap_new" => {
                    return MirType::Map(
                        Box::new(MirType::Struct(Arc::from("BuildString"))),
                        Box::new(MirType::f64()),
                    )
                }
                "HashSet_new" => return MirType::Struct(Arc::from("HashSet")),
                // Some(x) constructs an Option; without this the dest fell back to
                // i32 and the Option struct was never built (a real C2440).
                "Some" => return MirType::Struct(Arc::from("Option")),
                // Ok(x)/Err(e) construct a Result; without this the dest fell
                // back to i32 and the Result struct was never built (the same
                // C2440 class the Option fix closed).
                "Ok" | "Err" => return MirType::Struct(Arc::from("Result")),
                // Both String constructors yield an owned heap string. Without
                // `String_from` here the dest local fell back to i32, so
                // `let s = String::from(x)` emitted `int32_t s = build_string_new(...)`,
                // a real C2440 (BuildString -> int32_t) under a C compiler.
                "String_new" | "String_from" => return MirType::Struct(Arc::from("BuildString")),
                "VecDeque_new" => return MirType::Struct(Arc::from("VecDeque")),
                "map_new" => {
                    return MirType::Map(
                        Box::new(MirType::Struct(Arc::from("BuildString"))),
                        Box::new(MirType::f64()),
                    )
                }
                "map_get" => return MirType::f64(),
                "map_len" => return MirType::i64(),
                "map_contains" | "map_remove" => return MirType::Bool,
                "map_insert" => return MirType::Void,
                // HashMap builtins (i64->f64)
                "map_new_i64" => {
                    return MirType::Map(Box::new(MirType::i64()), Box::new(MirType::f64()))
                }
                "map_get_i64" => return MirType::f64(),
                "map_len_i64" => return MirType::i64(),
                "map_contains_i64" | "map_remove_i64" => return MirType::Bool,
                "map_insert_i64" => return MirType::Void,
                _ => {}
            }
        }
        // Use expected type from let binding annotation if available,
        // otherwise fall back to i32.  This makes constructor calls like
        // HashMap::new() return the correct type when used in an annotated
        // let binding: `let mut dist: HashMap<String, f64> = HashMap::new();`
        if let Some(ref expected) = self.expected_type {
            expected.clone()
        } else {
            MirType::i32()
        }
    }

    /// Check if a path expression refers to an enum variant (e.g. Shape::Circle).
    /// Returns (enum_name, variant_name) if it does.
    fn try_resolve_enum_variant_path(&self, func: &ast::Expr) -> Option<(Arc<str>, Arc<str>)> {
        if let ExprKind::Path(path) = &func.kind {
            if path.segments.len() == 2 {
                let enum_name = &path.segments[0].ident.name;
                let variant_name = &path.segments[1].ident.name;
                // Check both concrete enums AND generic enum templates
                if self.is_enum_type(enum_name)
                    || self.generic_enums.contains_key(enum_name.as_ref())
                {
                    return Some((enum_name.clone(), variant_name.clone()));
                }
            }
        }
        None
    }

    /// Lower enum variant construction: Shape::Circle(5.0) or Option::Some(42)
    /// For generic enums, infers type from arguments and monomorphizes.
    fn lower_enum_variant_construct(
        &mut self,
        enum_name: &Arc<str>,
        variant_name: &Arc<str>,
        args: &[ast::Expr],
    ) -> CodegenResult<MirValue> {
        // Lower all argument values first
        let arg_vals: Vec<_> = args
            .iter()
            .map(|a| self.lower_expr(a))
            .collect::<CodegenResult<_>>()?;

        // Check if this is a generic enum that needs monomorphization
        let actual_enum_name = if self.generic_enums.contains_key(enum_name.as_ref()) {
            // Infer type params from the variant arguments
            let subst = self.infer_enum_generics_from_variant(enum_name, variant_name, &arg_vals);
            self.monomorphize_enum_multi(enum_name, &subst)?
        } else {
            enum_name.clone()
        };

        // Look up the variant to get its discriminant
        let disc = self
            .lookup_enum_variant(&actual_enum_name, variant_name)
            .map(|(d, _)| d)
            .unwrap_or(0);

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        let result = builder.create_local(MirType::Struct(actual_enum_name.clone()));
        builder.aggregate(
            result,
            AggregateKind::Variant(actual_enum_name.clone(), disc as u32, variant_name.clone()),
            arg_vals,
        );

        Ok(values::local(result))
    }

    /// Infer generic type parameters for an enum from the types of variant arguments.
    /// E.g., `Option::Some(42)` → infer T=i32 from the 42 argument.
    fn infer_enum_generics_from_variant(
        &self,
        enum_name: &str,
        variant_name: &str,
        arg_vals: &[MirValue],
    ) -> HashMap<Arc<str>, MirType> {
        let mut subst = HashMap::new();

        let enum_def = match self.generic_enums.get(enum_name) {
            Some(e) => e.clone(),
            None => return subst,
        };

        // Get type parameter names
        let type_param_names: Vec<Arc<str>> = enum_def
            .generics
            .params
            .iter()
            .filter_map(|p| match &p.kind {
                ast::GenericParamKind::Type { .. } => Some(p.ident.name.clone()),
                _ => None,
            })
            .collect();

        // Find the variant definition
        if let Some(variant) = enum_def
            .variants
            .iter()
            .find(|v| v.name.name.as_ref() == variant_name)
        {
            let fields = match &variant.fields {
                ast::StructFields::Tuple(fields) => {
                    fields.iter().map(|f| &f.ty).collect::<Vec<_>>()
                }
                ast::StructFields::Named(fields) => {
                    fields.iter().map(|f| &f.ty).collect::<Vec<_>>()
                }
                ast::StructFields::Unit => Vec::new(),
            };

            for (i, field_ty) in fields.iter().enumerate() {
                if let Some(val) = arg_vals.get(i) {
                    let val_ty = self.type_of_value(val);
                    // Check if this field type is a generic parameter
                    if let ast::TypeKind::Path(path) = &field_ty.kind {
                        if path.is_simple() {
                            if let Some(ident) = path.last_ident() {
                                for tp_name in &type_param_names {
                                    if ident.name.as_ref() == tp_name.as_ref() {
                                        subst.entry(tp_name.clone()).or_insert(val_ty.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fill in unbound params with i32 default
        for tp_name in &type_param_names {
            subst.entry(tp_name.clone()).or_insert(MirType::i32());
        }

        subst
    }

    fn lower_method_call(
        &mut self,
        receiver: &ast::Expr,
        method: &ast::Ident,
        args: &[ast::Expr],
    ) -> CodegenResult<MirValue> {
        // =====================================================================
        // Iterator chain lowering: detect .iter().map(closure).collect() etc.
        // and desugar to imperative loops rather than actual iterator objects.
        // =====================================================================
        let method_name = method.name.as_ref();
        if matches!(
            method_name,
            "collect" | "fold" | "sum" | "count" | "product" | "any" | "all"
        ) {
            if let Some(chain) = Self::try_parse_iter_chain(receiver, method_name, args) {
                return self.lower_iter_chain(&chain);
            }
        }

        // Lower the receiver first to determine its type
        let receiver_val = self.lower_expr(receiver)?;
        let receiver_ty = self.type_of_value(&receiver_val);

        // =================================================================
        // String method calls: s.len(), s.is_empty(), s.starts_with(),
        // s.ends_with(), s.contains(), s.to_uppercase(), s.to_lowercase(),
        // s.trim(), s.split(), s.split_whitespace(), s.lines()
        // =================================================================
        if let MirType::Struct(ref name) = receiver_ty {
            if name.as_ref() == "BuildString" {
                let method_name = method.name.as_ref();

                // --- No-arg methods returning usize ---
                if method_name == "len" {
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::usize());
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_string_len"));
                    builder.call(func, vec![receiver_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }

                // --- No-arg methods returning bool ---
                if method_name == "is_empty" {
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::Bool);
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_string_is_empty"));
                    builder.call(func, vec![receiver_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }

                // --- Identity methods that return the string itself ---
                // chars()/as_bytes()/collect() in .bld are identity ops
                // in C because strings are already byte-accessible.
                // For Ptr receivers, dereference first.
                if method_name == "chars" || method_name == "as_bytes" || method_name == "bytes" {
                    if matches!(&receiver_ty, MirType::Ptr(_)) {
                        let builder = self.current_fn.as_mut().ok_or_else(|| {
                            CodegenError::Internal("No current function".to_string())
                        })?;
                        let derefed =
                            builder.create_local(MirType::Struct(Arc::from("BuildString")));
                        builder.assign(
                            derefed,
                            MirRValue::Deref {
                                ptr: receiver_val,
                                pointee_ty: MirType::Struct(Arc::from("BuildString")),
                            },
                        );
                        return Ok(values::local(derefed));
                    }
                    return Ok(receiver_val);
                }

                // --- No-arg methods returning BuildString ---
                let no_arg_str_fn: Option<&str> = match method_name {
                    "to_uppercase" => Some("build_string_to_upper"),
                    "to_lowercase" => Some("build_string_to_lower"),
                    "trim" => Some("build_string_trim"),
                    _ => None,
                };
                if let Some(c_fn) = no_arg_str_fn {
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::Struct(Arc::from("BuildString")));
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from(c_fn));
                    builder.call(func, vec![receiver_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }

                // --- Single-arg methods returning bool (arg is BuildString) ---
                let one_arg_bool_fn: Option<&str> = match method_name {
                    "starts_with" => Some("build_string_starts_with"),
                    "ends_with" => Some("build_string_ends_with"),
                    "contains" => Some("build_string_contains"),
                    _ => None,
                };
                if let Some(c_fn) = one_arg_bool_fn {
                    let mut arg_vals = vec![receiver_val];
                    for arg in args {
                        arg_vals.push(self.lower_expr(arg)?);
                    }
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::Bool);
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from(c_fn));
                    builder.call(func, arg_vals, Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }

                // --- push_str(x): append in place ---
                // `s.push_str(x)` reassigns `s` to `build_string_concat(s, x)`.
                // String literals already lower to BuildString, so the argument
                // needs no coercion. Returns unit.
                if method_name == "push_str" && args.len() == 1 {
                    let receiver_local = match receiver_val {
                        MirValue::Local(id) => Some(id),
                        _ => None,
                    };
                    let arg_val = self.lower_expr(&args[0])?;
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::Struct(Arc::from("BuildString")));
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_string_concat"));
                    builder.call(func, vec![receiver_val, arg_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    // Write the concatenated string back to the receiver local.
                    if let Some(id) = receiver_local {
                        builder.assign(id, MirRValue::Use(values::local(result)));
                    }
                    return Ok(values::unit());
                }

                // --- split(delim) → BuildVec of BuildString ---
                if method_name == "split" {
                    let mut arg_vals = vec![receiver_val];
                    for arg in args {
                        arg_vals.push(self.lower_expr(arg)?);
                    }
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    // Result is a Vec<String> handle so .len()/for/indexing work.
                    let result = builder.create_local(MirType::Vec(Box::new(MirType::Struct(
                        Arc::from("BuildString"),
                    ))));
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_string_split_h"));
                    builder.call(func, arg_vals, Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }

                // --- split_whitespace() → Vec<String> ---
                if method_name == "split_whitespace" {
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::Vec(Box::new(MirType::Struct(
                        Arc::from("BuildString"),
                    ))));
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_string_split_ws_h"));
                    builder.call(func, vec![receiver_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }

                // --- lines() → Vec<String> ---
                if method_name == "lines" {
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::Vec(Box::new(MirType::Struct(
                        Arc::from("BuildString"),
                    ))));
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_string_lines_h"));
                    builder.call(func, vec![receiver_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }

                // --- parse_int() → i64 ---
                if method_name == "parse_int" {
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::i64());
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_string_parse_int"));
                    builder.call(func, vec![receiver_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }

                // --- parse_float() → f64 ---
                if method_name == "parse_float" {
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::f64());
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_string_parse_float"));
                    builder.call(func, vec![receiver_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }

                // --- char_at(index) → BuildString ---
                if method_name == "char_at" && args.len() == 1 {
                    let idx_val = self.lower_expr(&args[0])?;
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::Struct(Arc::from("BuildString")));
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_string_char_at"));
                    builder.call(func, vec![receiver_val, idx_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }

                // --- substring(start, len) → BuildString ---
                if method_name == "substring" && args.len() == 2 {
                    let start_val = self.lower_expr(&args[0])?;
                    let len_val = self.lower_expr(&args[1])?;
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::Struct(Arc::from("BuildString")));
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_string_substring"));
                    builder.call(
                        func,
                        vec![receiver_val, start_val, len_val],
                        Some(result),
                        cont,
                    );
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }

                // --- replace(old, new) → BuildString ---
                if method_name == "replace" && args.len() == 2 {
                    let mut arg_vals = vec![receiver_val];
                    for arg in args {
                        arg_vals.push(self.lower_expr(arg)?);
                    }
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::Struct(Arc::from("BuildString")));
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_string_replace"));
                    builder.call(func, arg_vals, Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }

                // --- repeat(count) → BuildString ---
                if method_name == "repeat" && args.len() == 1 {
                    let count_val = self.lower_expr(&args[0])?;
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::Struct(Arc::from("BuildString")));
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_string_repeat"));
                    builder.call(func, vec![receiver_val, count_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }

                // --- index_of(substr) → i64 ---
                if method_name == "index_of" && args.len() == 1 {
                    let substr_val = self.lower_expr(&args[0])?;
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::i64());
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_string_index_of"));
                    builder.call(func, vec![receiver_val, substr_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }

                // --- compare(other) → i64 ---
                if method_name == "compare" && args.len() == 1 {
                    let other_val = self.lower_expr(&args[0])?;
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::i64());
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_string_compare"));
                    builder.call(func, vec![receiver_val, other_val], Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }
            }
        }

        // Dynamic dispatch for trait objects: obj.method() → obj.vtable->method(obj.data)
        if let MirType::TraitObject(ref trait_name) = receiver_ty {
            if let Some(trait_methods) = self.trait_methods.get(trait_name).cloned() {
                // Find the method index in the vtable
                if let Some((method_idx, (_, method_sig))) = trait_methods
                    .iter()
                    .enumerate()
                    .find(|(_, (name, _))| name.as_ref() == method.name.as_ref())
                {
                    // Lower all arguments
                    let mut arg_vals = Vec::new();
                    for arg in args {
                        arg_vals.push(self.lower_expr(arg)?);
                    }

                    let ret_ty = method_sig.ret.clone();
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

                    // Extract data pointer from fat pointer: receiver.data
                    let data_ptr = builder.create_local(MirType::Ptr(Box::new(MirType::Void)));
                    builder.assign(
                        data_ptr,
                        MirRValue::FieldAccess {
                            base: receiver_val.clone(),
                            field_name: Arc::from("data"),
                            field_ty: MirType::Ptr(Box::new(MirType::Void)),
                        },
                    );

                    // Extract vtable pointer: receiver.vtable
                    let vtable_ptr_ty = MirType::Ptr(Box::new(MirType::Void));
                    let vtable_ptr = builder.create_local(vtable_ptr_ty.clone());
                    builder.assign(
                        vtable_ptr,
                        MirRValue::FieldAccess {
                            base: receiver_val,
                            field_name: Arc::from("vtable"),
                            field_ty: vtable_ptr_ty,
                        },
                    );

                    // The C backend will generate: receiver.vtable->method(receiver.data, args...)
                    // We store the method index and trait name for the C backend
                    let result = builder.create_local(ret_ty);
                    let cont = builder.create_block();

                    // Create a function value that encodes the vtable dispatch
                    let dispatch_name = Arc::from(format!(
                        "__vtable_dispatch_{}_{}_{}",
                        trait_name, method.name, method_idx
                    ));

                    // Prepend data pointer as first argument (self)
                    let mut all_args = vec![values::local(data_ptr)];
                    all_args.extend(arg_vals);

                    builder.call(
                        MirValue::Global(dispatch_name),
                        all_args,
                        Some(result),
                        cont,
                    );
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }
            }
        }

        // =====================================================================
        // Primitive float method calls: val.abs(), val.sqrt(), etc.
        // Rewrite to the equivalent C math function call.
        // =====================================================================
        if matches!(&receiver_ty, MirType::Float(_)) {
            let method_name = method.name.as_ref();
            // Map method name → C function name (single-arg, receiver is the argument)
            let simple_c_fn: Option<&str> = match method_name {
                "abs" => Some("fabs"),
                "sqrt" => Some("sqrt"),
                "cbrt" => Some("cbrt"),
                "ceil" => Some("ceil"),
                "floor" => Some("floor"),
                "round" => Some("round"),
                "trunc" => Some("trunc"),
                "signum" => None, // not a simple C call
                "recip" => None,  // not a simple C call
                "sin" => Some("sin"),
                "cos" => Some("cos"),
                "tan" => Some("tan"),
                "asin" => Some("asin"),
                "acos" => Some("acos"),
                "atan" => Some("atan"),
                "sinh" => Some("sinh"),
                "cosh" => Some("cosh"),
                "tanh" => Some("tanh"),
                "exp" => Some("exp"),
                "exp2" => Some("exp2"),
                "ln" => Some("log"),
                "log2" => Some("log2"),
                "log10" => Some("log10"),
                _ => None,
            };

            // Two-arg float methods: val.method(other) → c_fn(val, other)
            let two_arg_c_fn: Option<&str> = match method_name {
                "powi" => Some("pow"),
                "powf" => Some("pow"),
                "log" => Some("log"), // Rust log(base) - but C log is ln; handle specially
                "atan2" => Some("atan2"),
                "hypot" => Some("hypot"),
                "copysign" => Some("copysign"),
                "max" => Some("fmax"),
                "min" => Some("fmin"),
                _ => None,
            };

            // Handle simple single-arg methods: val.method() → c_fn(val)
            if let Some(c_fn) = simple_c_fn {
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let result = builder.create_local(receiver_ty.clone());
                let cont = builder.create_block();
                let func = MirValue::Function(Arc::from(c_fn));
                builder.call(func, vec![receiver_val], Some(result), cont);
                builder.switch_to_block(cont);
                return Ok(values::local(result));
            }

            // Handle two-arg methods: val.method(other) → c_fn(val, other)
            if let Some(c_fn) = two_arg_c_fn {
                let mut arg_vals = vec![receiver_val];
                for arg in args {
                    arg_vals.push(self.lower_expr(arg)?);
                }
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let result = builder.create_local(receiver_ty.clone());
                let cont = builder.create_block();
                let func = MirValue::Function(Arc::from(c_fn));
                builder.call(func, arg_vals, Some(result), cont);
                builder.switch_to_block(cont);
                return Ok(values::local(result));
            }

            // to_degrees: val * (180.0 / PI)
            if method_name == "to_degrees" {
                let factor = 180.0_f64 / std::f64::consts::PI;
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let factor_local = builder.create_local(receiver_ty.clone());
                builder.assign_const(factor_local, MirConst::Float(factor, receiver_ty.clone()));
                let result = builder.create_local(receiver_ty.clone());
                builder.binary_op(
                    result,
                    BinOp::Mul,
                    receiver_val,
                    values::local(factor_local),
                );
                return Ok(values::local(result));
            }

            // to_radians: val * (PI / 180.0)
            if method_name == "to_radians" {
                let factor = std::f64::consts::PI / 180.0_f64;
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let factor_local = builder.create_local(receiver_ty.clone());
                builder.assign_const(factor_local, MirConst::Float(factor, receiver_ty.clone()));
                let result = builder.create_local(receiver_ty.clone());
                builder.binary_op(
                    result,
                    BinOp::Mul,
                    receiver_val,
                    values::local(factor_local),
                );
                return Ok(values::local(result));
            }

            // fract: val - floor(val)
            if method_name == "fract" {
                // First compute floor(val)
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let floor_result = builder.create_local(receiver_ty.clone());
                let cont1 = builder.create_block();
                let floor_fn = MirValue::Function(Arc::from("floor"));
                builder.call(
                    floor_fn,
                    vec![receiver_val.clone()],
                    Some(floor_result),
                    cont1,
                );
                builder.switch_to_block(cont1);
                // Then compute val - floor(val)
                let result = builder.create_local(receiver_ty.clone());
                builder.binary_op(
                    result,
                    BinOp::Sub,
                    receiver_val,
                    values::local(floor_result),
                );
                return Ok(values::local(result));
            }

            // clamp(min, max): fmax(min_val, fmin(max_val, val))
            if method_name == "clamp" && args.len() == 2 {
                let min_val = self.lower_expr(&args[0])?;
                let max_val = self.lower_expr(&args[1])?;
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                // fmin(val, max)
                let clamped_high = builder.create_local(receiver_ty.clone());
                let cont1 = builder.create_block();
                let fmin_fn = MirValue::Function(Arc::from("fmin"));
                builder.call(
                    fmin_fn,
                    vec![receiver_val, max_val],
                    Some(clamped_high),
                    cont1,
                );
                builder.switch_to_block(cont1);
                // fmax(result, min)
                let result = builder.create_local(receiver_ty.clone());
                let cont2 = builder.create_block();
                let fmax_fn = MirValue::Function(Arc::from("fmax"));
                builder.call(
                    fmax_fn,
                    vec![values::local(clamped_high), min_val],
                    Some(result),
                    cont2,
                );
                builder.switch_to_block(cont2);
                return Ok(values::local(result));
            }

            // mul_add(a, b): val * a + b  (fused multiply-add)
            if method_name == "mul_add" && args.len() == 2 {
                let a_val = self.lower_expr(&args[0])?;
                let b_val = self.lower_expr(&args[1])?;
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let mul_result = builder.create_local(receiver_ty.clone());
                builder.binary_op(mul_result, BinOp::Mul, receiver_val, a_val);
                let result = builder.create_local(receiver_ty.clone());
                builder.binary_op(result, BinOp::Add, values::local(mul_result), b_val);
                return Ok(values::local(result));
            }

            // is_nan, is_infinite, is_finite, is_normal - fall through to default
            // (these would need special C codegen; not critical for now)
        }

        // Try to resolve the method via impl_methods registry.
        // Auto-deref: if receiver is &T (pointer to struct), look up on T.
        let resolved_fn_name = match &receiver_ty {
            MirType::Struct(ref type_name) => self
                .impl_methods
                .get(&(type_name.clone(), method.name.clone()))
                .cloned(),
            MirType::Ptr(inner) => {
                if let MirType::Struct(ref type_name) = **inner {
                    self.impl_methods
                        .get(&(type_name.clone(), method.name.clone()))
                        .cloned()
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(mangled_name) = resolved_fn_name {
            // Check if the method's first parameter (self) is a pointer type,
            // meaning it was declared as &self or &mut self.
            let self_is_ref = self
                .module
                .find_function(&mangled_name)
                .and_then(|f| f.sig.params.first())
                .map(|p| p.is_pointer())
                .unwrap_or(false);

            // If the method takes &self, pass &receiver instead of receiver.
            // But if receiver is already a pointer (e.g., parameter &RGB),
            // pass it directly to avoid double-indirection.
            let receiver_already_ptr = matches!(&receiver_ty, MirType::Ptr(_));

            let self_arg = if self_is_ref && !receiver_already_ptr {
                let receiver_local = match &receiver_val {
                    MirValue::Local(id) => *id,
                    _ => {
                        let builder = self.current_fn.as_mut().unwrap();
                        let temp = builder.create_local(receiver_ty.clone());
                        builder.assign(temp, MirRValue::Use(receiver_val));
                        temp
                    }
                };
                let builder = self.current_fn.as_mut().unwrap();
                let ref_local = builder.create_local(MirType::Ptr(Box::new(receiver_ty.clone())));
                builder.make_ref(ref_local, false, MirPlace::local(receiver_local));
                values::local(ref_local)
            } else {
                receiver_val
            };

            // Resolved impl method: call TypeName_methodName(receiver, args...)
            let mut arg_vals = vec![self_arg];
            for arg in args {
                arg_vals.push(self.lower_expr(arg)?);
            }

            // Resolve return type from already-lowered function
            let ret_ty = self
                .module
                .find_function(&mangled_name)
                .map(|f| f.sig.ret.clone())
                .unwrap_or(MirType::i32());

            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let result = builder.create_local(ret_ty);
            let cont = builder.create_block();
            let func = MirValue::Function(mangled_name);
            builder.call(func, arg_vals, Some(result), cont);
            builder.switch_to_block(cont);
            return Ok(values::local(result));
        }

        // &BuildString method dispatch: handle chars/as_bytes on pointer receivers.
        // These dereference the pointer and return the BuildString value.
        if let MirType::Ptr(ref inner) = receiver_ty {
            if let MirType::Struct(ref name) = inner.as_ref() {
                if name.as_ref() == "BuildString" {
                    let method_name = method.name.as_ref();
                    if method_name == "chars"
                        || method_name == "as_bytes"
                        || method_name == "bytes"
                        || method_name == "clone"
                        || method_name == "to_owned"
                    {
                        let builder = self.current_fn.as_mut().ok_or_else(|| {
                            CodegenError::Internal("No current function".to_string())
                        })?;
                        let derefed =
                            builder.create_local(MirType::Struct(Arc::from("BuildString")));
                        builder.assign(
                            derefed,
                            MirRValue::Deref {
                                ptr: receiver_val,
                                pointee_ty: MirType::Struct(Arc::from("BuildString")),
                            },
                        );
                        return Ok(values::local(derefed));
                    }
                }
            }
        }

        // Option<T> method dispatch: is_some/is_none read the has_value
        // discriminant; unwrap/unwrap_or read the typed payload slot. The
        // payload type is the tracked inner type (default i32).
        if matches!(&receiver_ty, MirType::Struct(n) if n.as_ref() == "Option") {
            let method_name = method.name.as_ref();
            let inner_ty = match &receiver_val {
                MirValue::Local(id) => self
                    .option_inner_types
                    .get(id)
                    .cloned()
                    .unwrap_or_else(MirType::i32),
                _ => MirType::i32(),
            };
            match method_name {
                "is_some" | "is_none" => {
                    let builder = self.current_fn.as_mut().unwrap();
                    let scrut = builder.create_local(MirType::Struct(Arc::from("Option")));
                    builder.assign(scrut, MirRValue::Use(receiver_val));
                    let hv = builder.create_local(MirType::Bool);
                    builder.assign(
                        hv,
                        MirRValue::FieldAccess {
                            base: values::local(scrut),
                            field_name: Arc::from("has_value"),
                            field_ty: MirType::Bool,
                        },
                    );
                    if method_name == "is_none" {
                        let neg = builder.create_local(MirType::Bool);
                        builder.unary_op(neg, UnaryOp::Not, values::local(hv));
                        return Ok(values::local(neg));
                    }
                    return Ok(values::local(hv));
                }
                "unwrap" => {
                    let builder = self.current_fn.as_mut().unwrap();
                    let scrut = builder.create_local(MirType::Struct(Arc::from("Option")));
                    builder.assign(scrut, MirRValue::Use(receiver_val));
                    let val = builder.create_local(inner_ty.clone());
                    builder.assign(
                        val,
                        MirRValue::FieldAccess {
                            base: values::local(scrut),
                            field_name: Arc::from("value"),
                            field_ty: inner_ty,
                        },
                    );
                    return Ok(values::local(val));
                }
                "unwrap_or" if args.len() == 1 => {
                    let default_val = self.lower_expr(&args[0])?;
                    let builder = self.current_fn.as_mut().unwrap();
                    let scrut = builder.create_local(MirType::Struct(Arc::from("Option")));
                    builder.assign(scrut, MirRValue::Use(receiver_val));
                    let hv = builder.create_local(MirType::Bool);
                    builder.assign(
                        hv,
                        MirRValue::FieldAccess {
                            base: values::local(scrut),
                            field_name: Arc::from("has_value"),
                            field_ty: MirType::Bool,
                        },
                    );
                    let result = builder.create_local(inner_ty.clone());
                    let some_block = builder.create_block();
                    let none_block = builder.create_block();
                    let merge_block = builder.create_block();
                    builder.branch(values::local(hv), some_block, none_block);
                    // Some: read the payload.
                    builder.switch_to_block(some_block);
                    builder.assign(
                        result,
                        MirRValue::FieldAccess {
                            base: values::local(scrut),
                            field_name: Arc::from("value"),
                            field_ty: inner_ty,
                        },
                    );
                    builder.goto(merge_block);
                    // None: use the default.
                    builder.switch_to_block(none_block);
                    builder.assign(result, MirRValue::Use(default_val));
                    builder.goto(merge_block);
                    builder.switch_to_block(merge_block);
                    return Ok(values::local(result));
                }
                _ => {}
            }
        }

        // Result<T,E> method dispatch: is_ok/is_err read the is_ok discriminant;
        // unwrap/unwrap_or read the typed ok slot; unwrap_err reads the err slot.
        if matches!(&receiver_ty, MirType::Struct(n) if n.as_ref() == "Result") {
            let method_name = method.name.as_ref();
            let ok_ty = match &receiver_val {
                MirValue::Local(id) => self
                    .result_ok_types
                    .get(id)
                    .cloned()
                    .unwrap_or_else(MirType::i32),
                _ => MirType::i32(),
            };
            let err_ty = match &receiver_val {
                MirValue::Local(id) => self
                    .result_err_types
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| MirType::Struct(Arc::from("BuildString"))),
                _ => MirType::Struct(Arc::from("BuildString")),
            };
            match method_name {
                "is_ok" | "is_err" => {
                    let builder = self.current_fn.as_mut().unwrap();
                    let scrut = builder.create_local(MirType::Struct(Arc::from("Result")));
                    builder.assign(scrut, MirRValue::Use(receiver_val));
                    let ok = builder.create_local(MirType::Bool);
                    builder.assign(
                        ok,
                        MirRValue::FieldAccess {
                            base: values::local(scrut),
                            field_name: Arc::from("is_ok"),
                            field_ty: MirType::Bool,
                        },
                    );
                    if method_name == "is_err" {
                        let neg = builder.create_local(MirType::Bool);
                        builder.unary_op(neg, UnaryOp::Not, values::local(ok));
                        return Ok(values::local(neg));
                    }
                    return Ok(values::local(ok));
                }
                "unwrap" | "unwrap_err" => {
                    let (field, ty) = if method_name == "unwrap_err" {
                        ("err", err_ty)
                    } else {
                        ("ok", ok_ty)
                    };
                    let builder = self.current_fn.as_mut().unwrap();
                    let scrut = builder.create_local(MirType::Struct(Arc::from("Result")));
                    builder.assign(scrut, MirRValue::Use(receiver_val));
                    let val = builder.create_local(ty.clone());
                    builder.assign(
                        val,
                        MirRValue::FieldAccess {
                            base: values::local(scrut),
                            field_name: Arc::from(field),
                            field_ty: ty,
                        },
                    );
                    return Ok(values::local(val));
                }
                "unwrap_or" if args.len() == 1 => {
                    let default_val = self.lower_expr(&args[0])?;
                    let builder = self.current_fn.as_mut().unwrap();
                    let scrut = builder.create_local(MirType::Struct(Arc::from("Result")));
                    builder.assign(scrut, MirRValue::Use(receiver_val));
                    let ok = builder.create_local(MirType::Bool);
                    builder.assign(
                        ok,
                        MirRValue::FieldAccess {
                            base: values::local(scrut),
                            field_name: Arc::from("is_ok"),
                            field_ty: MirType::Bool,
                        },
                    );
                    let result = builder.create_local(ok_ty.clone());
                    let ok_block = builder.create_block();
                    let err_block = builder.create_block();
                    let merge_block = builder.create_block();
                    builder.branch(values::local(ok), ok_block, err_block);
                    builder.switch_to_block(ok_block);
                    builder.assign(
                        result,
                        MirRValue::FieldAccess {
                            base: values::local(scrut),
                            field_name: Arc::from("ok"),
                            field_ty: ok_ty,
                        },
                    );
                    builder.goto(merge_block);
                    builder.switch_to_block(err_block);
                    builder.assign(result, MirRValue::Use(default_val));
                    builder.goto(merge_block);
                    builder.switch_to_block(merge_block);
                    return Ok(values::local(result));
                }
                _ => {}
            }
        }

        // Vec<T> method dispatch: map .push/.len/.get/.pop/.is_empty/.clear
        // to typed runtime functions (build_hvec_*).
        if let MirType::Vec(ref elem_ty) = receiver_ty {
            let method_name = method.name.as_ref();
            let type_suffix: String = match elem_ty.as_ref() {
                MirType::Float(_) => "f64".to_string(),
                MirType::Int(IntSize::I64, _) => "i64".to_string(),
                MirType::Struct(n) if n.as_ref() == "BuildString" => "str".to_string(),
                // Aggregate element (user struct etc.): element-sized wrapper
                // keyed by the struct name (matches the C backend's generated
                // build_hvec_*_<Struct> wrappers).
                MirType::Struct(n) => n.to_string(),
                // Nested collection element (Vec<Vec<_>>, Vec<HashMap<_,_>>): the
                // element is a handle struct; key the sized wrapper by its C type.
                MirType::Vec(_) => "BuildVecHandle".to_string(),
                MirType::Map(_, _) => "BuildMapHandle".to_string(),
                _ => "i32".to_string(),
            };

            let (runtime_fn, ret_ty): (Option<String>, MirType) = match method_name {
                "push" => (
                    Some(format!("build_hvec_push_{}", type_suffix)),
                    MirType::Void,
                ),
                "pop" => (
                    Some(format!("build_hvec_pop_{}", type_suffix)),
                    *elem_ty.clone(),
                ),
                "get" | "index" => (
                    Some(format!("build_hvec_get_{}", type_suffix)),
                    *elem_ty.clone(),
                ),
                "len" => (Some("build_hvec_len".to_string()), MirType::usize()),
                "is_empty" => {
                    // Lower as len() == 0
                    let arg_vals = vec![receiver_val];
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let len_result = builder.create_local(MirType::usize());
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from("build_hvec_len"));
                    builder.call(func, arg_vals, Some(len_result), cont);
                    builder.switch_to_block(cont);
                    let zero = builder.create_local(MirType::usize());
                    builder.assign_const(zero, MirConst::Int(0, MirType::usize()));
                    let result = builder.create_local(MirType::Bool);
                    builder.binary_op(
                        result,
                        BinOp::Eq,
                        values::local(len_result),
                        values::local(zero),
                    );
                    return Ok(values::local(result));
                }
                "clear" => (Some("build_hvec_free".to_string()), MirType::Void),
                "contains" => (
                    Some(format!("build_hvec_contains_{}", type_suffix)),
                    MirType::Bool,
                ),
                // sort() in place (ascending); string element sort is a
                // follow-up - only the numeric families have comparators.
                "sort" if matches!(type_suffix.as_str(), "i32" | "i64" | "f64") => (
                    Some(format!("build_hvec_sort_{}", type_suffix)),
                    MirType::Void,
                ),
                _ => (None, MirType::Void),
            };

            if let Some(fn_name) = runtime_fn {
                let mut arg_vals = vec![receiver_val];
                for arg in args {
                    arg_vals.push(self.lower_expr(arg)?);
                }
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let result = builder.create_local(ret_ty);
                let cont = builder.create_block();
                let func = MirValue::Function(Arc::from(fn_name.as_str()));
                builder.call(func, arg_vals, Some(result), cont);
                builder.switch_to_block(cont);
                return Ok(values::local(result));
            }
        }

        // HashMap<K,V> / Map method dispatch → typed runtime calls.
        // Select runtime function based on the Map's value type parameter.
        if let MirType::Map(ref _key_ty, ref val_ty) = receiver_ty {
            let method_name = method.name.as_ref();

            let (runtime_fn, ret_ty): (Option<String>, MirType) = match method_name {
                "insert" => {
                    // Non-f64 values go through the value-typed wrapper (which boxes
                    // values larger than the 8-byte slot, e.g. BuildString). f64
                    // values use the native str->f64 path. Previously insert always
                    // used str->f64, passing a BuildString to a `double` param (C2440).
                    let fn_name = match val_ty.as_ref() {
                        MirType::Float(_) => "build_hmap_insert_str_f64".to_string(),
                        _ => format!("build_hmap_insert_val_{}", Self::type_to_c_name(val_ty)),
                    };
                    (Some(fn_name), MirType::Void)
                }
                "get" => {
                    let val_c = match val_ty.as_ref() {
                        MirType::Float(_) => None,
                        _ => Some(format!(
                            "build_hmap_get_val_{}",
                            Self::type_to_c_name(val_ty)
                        )),
                    };
                    let fn_name = val_c.unwrap_or_else(|| "build_hmap_get_str_f64".to_string());
                    (Some(fn_name), *val_ty.clone())
                }
                "contains" | "contains_key" => (
                    Some("build_hmap_contains_str_f64".to_string()),
                    MirType::Bool,
                ),
                "len" => (Some("build_hmap_len_str_f64".to_string()), MirType::usize()),
                "remove" => (Some("build_hmap_remove_str_f64".to_string()), MirType::Void),
                // keys() -> Vec<String> (a real Vec handle, so `for k in m.keys()`,
                // indexing, and .len() all work). values() is deferred: it needs
                // the str->f64 map's insert key-coercion fix first, and the value
                // type threading to read non-f64 values correctly.
                "keys" => (
                    Some("build_hmap_keys_str_f64".to_string()),
                    MirType::Vec(Box::new(MirType::Struct(Arc::from("BuildString")))),
                ),
                "is_empty" => {
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let len_result = builder.create_local(MirType::usize());
                    let cont = builder.create_block();
                    builder.call(
                        MirValue::Function(Arc::from("build_hmap_len_str_f64")),
                        vec![receiver_val],
                        Some(len_result),
                        cont,
                    );
                    builder.switch_to_block(cont);
                    let zero = builder.create_local(MirType::usize());
                    builder.assign_const(zero, MirConst::Int(0, MirType::usize()));
                    let result = builder.create_local(MirType::Bool);
                    builder.binary_op(
                        result,
                        BinOp::Eq,
                        values::local(len_result),
                        values::local(zero),
                    );
                    return Ok(values::local(result));
                }
                "clone" => (None, receiver_ty.clone()),
                _ => (None, MirType::i32()),
            };
            if let Some(fn_name) = runtime_fn {
                let mut arg_vals = vec![receiver_val];
                for arg in args {
                    arg_vals.push(self.lower_expr(arg)?);
                }
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let result = builder.create_local(ret_ty);
                let cont = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from(fn_name.as_str())),
                    arg_vals,
                    Some(result),
                    cont,
                );
                builder.switch_to_block(cont);
                return Ok(values::local(result));
            }
        }

        // HashSet method dispatch
        if let MirType::Struct(ref name) = receiver_ty {
            if name.as_ref() == "HashSet" {
                let method_name = method.name.as_ref();
                let (runtime_fn, ret_ty): (Option<&str>, MirType) = match method_name {
                    "insert" => (Some("build_hset_insert"), MirType::Void),
                    "contains" => (Some("build_hset_contains"), MirType::Bool),
                    "len" => (None, MirType::usize()),
                    "is_empty" => (None, MirType::Bool),
                    "clone" => (None, receiver_ty.clone()),
                    _ => (None, MirType::i32()),
                };
                if let Some(fn_name) = runtime_fn {
                    let mut arg_vals = vec![receiver_val];
                    for arg in args {
                        arg_vals.push(self.lower_expr(arg)?);
                    }
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(ret_ty);
                    let cont = builder.create_block();
                    builder.call(
                        MirValue::Function(Arc::from(fn_name)),
                        arg_vals,
                        Some(result),
                        cont,
                    );
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }
            }
        }

        // clone()/to_owned() - return receiver type for safe (non-cascading) types.
        // String, primitives, Ptr, Option are safe. Collections (Vec/Map/HashSet)
        // are NOT safe because their typed return cascades through i32-typed consumers.
        {
            let method_name = method.name.as_ref();
            if method_name == "clone" || method_name == "to_owned" {
                let is_safe_clone = matches!(&receiver_ty,
                    MirType::Struct(n) if n.as_ref() == "BuildString"
                ) || matches!(
                    &receiver_ty,
                    MirType::Int(..) | MirType::Float(..) | MirType::Bool | MirType::Ptr(..)
                );
                if is_safe_clone {
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(receiver_ty.clone());
                    let cont = builder.create_block();
                    builder.call(
                        MirValue::Function(Arc::from("clone")),
                        vec![receiver_val],
                        Some(result),
                        cont,
                    );
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }
            }
        }

        // Numeric / string `.to_string()`: dispatch to the type-specific runtime
        // formatter (each returns a fresh owned BuildString), or return the
        // receiver unchanged for a String. Without this, `to_string` fell through
        // to the bare-name fallback and emitted an undefined `to_string` symbol.
        if method.name.as_ref() == "to_string" && args.is_empty() {
            let runtime = match &receiver_ty {
                MirType::Int(..) => Some("build_i64_to_string"),
                MirType::Float(..) => Some("build_f64_to_string"),
                MirType::Struct(n) if n.as_ref() == "BuildString" => {
                    // String::to_string() is identity; return the receiver.
                    return Ok(receiver_val);
                }
                _ => None,
            };
            if let Some(rt) = runtime {
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let result = builder.create_local(MirType::Struct(Arc::from("BuildString")));
                let cont = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from(rt)),
                    vec![receiver_val.clone()],
                    Some(result),
                    cont,
                );
                builder.switch_to_block(cont);
                return Ok(values::local(result));
            }
        }

        // Fallback: lower as a regular function call with receiver as first argument.
        let mut arg_vals = vec![receiver_val];
        for arg in args {
            arg_vals.push(self.lower_expr(arg)?);
        }

        let method_name = method.name.as_ref();
        let fallback_ret_ty = match method_name {
            // Boolean-returning methods
            "is_some" | "is_none" | "is_ok" | "is_err" | "is_empty" | "contains"
            | "contains_key" | "starts_with" | "ends_with" => MirType::Bool,
            // Size-returning methods
            "len" | "count" | "capacity" => MirType::usize(),
            // Void-returning mutators
            "push" | "insert" | "remove" | "clear" | "sort" | "reverse" => MirType::Void,
            // pop returns the removed element (i32 fallback)
            "pop" | "pop_front" | "pop_back" => MirType::i32(),
            _ => MirType::i32(),
        };

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let result = builder.create_local(fallback_ret_ty);
        let cont = builder.create_block();
        let func = MirValue::Function(method.name.clone());
        builder.call(func, arg_vals, Some(result), cont);
        builder.switch_to_block(cont);
        Ok(values::local(result))
    }

    /// Lower `if let PATTERN = SCRUTINEE { then } else { else }`.
    /// For a runtime Option/Result scrutinee this tests the discriminant
    /// (has_value / is_ok, negated for None / Err), binds the payload from the
    /// typed union slot in the matched branch, and runs the unmatched branch
    /// otherwise. Falls back to a plain binding for other scrutinees.
    fn lower_if_let(
        &mut self,
        pattern: &ast::Pattern,
        scrutinee: &ast::Expr,
        then_branch: &ast::Block,
        else_branch: Option<&ast::Expr>,
    ) -> CodegenResult<MirValue> {
        let scrut_val = self.lower_expr(scrutinee)?;
        let scrut_ty = self.type_of_value(&scrut_val);
        let is_option = matches!(&scrut_ty, MirType::Struct(n) if n.as_ref() == "Option");
        let is_result = matches!(&scrut_ty, MirType::Struct(n) if n.as_ref() == "Result");

        // Variant name and the (optional) bound payload identifier.
        let (variant, bind_name): (Option<Arc<str>>, Option<Arc<str>>) = match &pattern.kind {
            ast::PatternKind::TupleStruct { path, patterns } => {
                let v = path.segments.last().map(|s| s.ident.name.clone());
                let b = patterns.first().and_then(|p| match &p.kind {
                    ast::PatternKind::Ident { name, .. } => Some(name.name.clone()),
                    _ => None,
                });
                (v, b)
            }
            ast::PatternKind::Ident { name, .. } => (Some(name.name.clone()), None),
            _ => (None, None),
        };
        let variant_str = variant.as_deref();

        if is_option || is_result {
            // Resolve the payload type and the union field for the bound value.
            let (payload_ty, payload_field) = if is_option {
                (
                    self.option_inner_type_for(scrutinee, &scrut_val)
                        .unwrap_or_else(MirType::i32),
                    "value",
                )
            } else if variant_str == Some("Err") {
                (self.result_err_type_for(scrutinee, &scrut_val), "err")
            } else {
                (self.result_ok_type_for(scrutinee, &scrut_val), "ok")
            };
            let disc_field = if is_option { "has_value" } else { "is_ok" };
            // None / Err match when the discriminant is FALSE.
            let negate = matches!(variant_str, Some("None") | Some("Err"));

            let builder = self.current_fn.as_mut().unwrap();
            let scrut_local = builder.create_local(scrut_ty.clone());
            builder.assign(scrut_local, MirRValue::Use(scrut_val));
            let disc = builder.create_local(MirType::Bool);
            builder.assign(
                disc,
                MirRValue::FieldAccess {
                    base: values::local(scrut_local),
                    field_name: Arc::from(disc_field),
                    field_ty: MirType::Bool,
                },
            );
            let cond = if negate {
                let neg = builder.create_local(MirType::Bool);
                builder.unary_op(neg, UnaryOp::Not, values::local(disc));
                values::local(neg)
            } else {
                values::local(disc)
            };

            let then_block = builder.create_block();
            let else_block = builder.create_block();
            let merge_block = builder.create_block();
            builder.branch(cond, then_block, else_block);

            // Matched branch: bind the payload (when the pattern binds one).
            builder.switch_to_block(then_block);
            let saved_vars = self.var_map.clone();
            if let Some(name) = bind_name {
                let builder = self.current_fn.as_mut().unwrap();
                let bound = builder.create_named_local(name.clone(), payload_ty.clone());
                builder.assign(
                    bound,
                    MirRValue::FieldAccess {
                        base: values::local(scrut_local),
                        field_name: Arc::from(payload_field),
                        field_ty: payload_ty,
                    },
                );
                self.var_map.insert(name, bound);
            }
            let then_val = self.lower_block(then_branch)?;
            self.var_map = saved_vars.clone();
            let result_ty = then_val
                .as_ref()
                .map(|v| self.type_of_value(v))
                .unwrap_or(MirType::Void);
            let builder = self.current_fn.as_mut().unwrap();
            let result = builder.create_local(result_ty);
            if let Some(v) = then_val {
                builder.assign(result, MirRValue::Use(v));
            }
            builder.goto(merge_block);

            // Unmatched branch.
            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(else_block);
            if let Some(else_expr) = else_branch {
                let else_val = self.lower_expr(else_expr)?;
                self.var_map = saved_vars;
                let builder = self.current_fn.as_mut().unwrap();
                if !matches!(else_val, MirValue::Const(MirConst::Unit)) {
                    builder.assign(result, MirRValue::Use(else_val));
                }
            }
            let builder = self.current_fn.as_mut().unwrap();
            builder.goto(merge_block);
            builder.switch_to_block(merge_block);
            return Ok(values::local(result));
        }

        // Fallback (non-sum-type scrutinee): bind the whole value and run the
        // then-branch. Rare; preserves the previous best-effort behavior.
        if let Some(name) = bind_name.or(variant) {
            let builder = self.current_fn.as_mut().unwrap();
            let local = builder.create_named_local(name.clone(), scrut_ty.clone());
            builder.assign(local, MirRValue::Use(scrut_val));
            self.var_map.insert(name, local);
        }
        self.lower_if_unconditional(then_branch, else_branch)
    }

    /// Whether a MIR type is an aggregate (struct/enum/Option/Vec/Map/tuple/
    /// array/slice), as opposed to a primitive (int/bool/void) that an
    /// unresolved branch value such as `None` may default to.
    fn is_aggregate_mir_ty(ty: &MirType) -> bool {
        matches!(
            ty,
            MirType::Struct(_)
                | MirType::Vec(_)
                | MirType::Map(_, _)
                | MirType::Tuple(_)
                | MirType::Array(_, _)
                | MirType::Slice(_)
                | MirType::TraitObject(_)
        )
    }

    fn lower_if(
        &mut self,
        condition: &ast::Expr,
        then_branch: &ast::Block,
        else_branch: Option<&ast::Expr>,
    ) -> CodegenResult<MirValue> {
        // Lower condition FIRST before borrowing builder
        let cond_val = self.lower_expr(condition)?;

        // Set up blocks (allocate result local later once we know the type)
        let (then_block, else_block, merge_block) = {
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

            let then_block = builder.create_block();
            let else_block = builder.create_block();
            let merge_block = builder.create_block();

            builder.branch(cond_val, then_block, else_block);
            (then_block, else_block, merge_block)
        };

        // Then branch - scope `let` bindings to the branch.
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(then_block);
        }
        let saved_vars = self.var_map.clone();
        let then_val = self.lower_block(then_branch)?;
        self.var_map = saved_vars.clone();

        // Determine the if-expression's result type from the then-branch value
        let result_ty = then_val
            .as_ref()
            .map(|v| self.type_of_value(v))
            .unwrap_or(MirType::Void);

        let result = {
            let builder = self.current_fn.as_mut().unwrap();
            let result = builder.create_local(result_ty);
            if let Some(v) = then_val {
                builder.assign(result, MirRValue::Use(v));
            }
            builder.goto(merge_block);
            result
        };

        // Else branch - scope `let` bindings to the branch.
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(else_block);
        }
        if let Some(else_expr) = else_branch {
            let else_val = self.lower_expr(else_expr)?;
            self.var_map = saved_vars;
            // The result type was derived from the then-branch. If the else-branch
            // has an aggregate type (e.g. then = `None` which defaults to i32, but
            // else = `Some(x)` -> `Option`), retype the result local to the
            // aggregate so it is not declared as (and truncated to) a primitive.
            // The type checker guarantees both branches share a type, so the
            // aggregate is the correct one whenever the two MIR types disagree.
            let else_ty = self.type_of_value(&else_val);
            let builder = self.current_fn.as_mut().unwrap();
            let cur_ty = builder.local_type(result).unwrap_or(MirType::Void);
            if Self::is_aggregate_mir_ty(&else_ty) && !Self::is_aggregate_mir_ty(&cur_ty) {
                builder.retype_local(result, else_ty);
            }
            builder.assign(result, MirRValue::Use(else_val));
        }
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.goto(merge_block);
            builder.switch_to_block(merge_block);
        }

        Ok(values::local(result))
    }

    /// Simplified if-let lowering: lowers both branches.
    /// A full implementation would lower to a match with pattern destructuring;
    /// this version always takes the then branch at runtime but lowers both
    /// branches so that the else branch's code is generated (not silently dropped).
    fn lower_if_unconditional(
        &mut self,
        then_branch: &ast::Block,
        else_branch: Option<&ast::Expr>,
    ) -> CodegenResult<MirValue> {
        let then_val = self.lower_block(then_branch)?;

        // Lower the else branch if present (even though the then branch
        // is always taken, the else branch must be compiled so its code
        // is not silently lost).
        if let Some(else_expr) = else_branch {
            let _else_val = self.lower_expr(else_expr)?;
        }

        Ok(then_val.unwrap_or(values::unit()))
    }

    /// Lower `match opt { Some(x) => body1, None => body2 }` for runtime Option.
    /// The runtime Option struct has `has_value: bool` and `value: union { i64 i; double f; void* p; }`.
    /// `inner_ty_override` threads the payload type recovered at the match site
    /// (from a binding annotation or the matched call's `-> Option<T>` return)
    /// so the union slot is read with the correct type; it takes priority over
    /// the per-local table and the i32 fallback.
    fn lower_runtime_option_match(
        &mut self,
        scrutinee_val: MirValue,
        scrutinee_ty: &MirType,
        arms: &[ast::MatchArm],
        inner_ty_override: Option<MirType>,
    ) -> CodegenResult<MirValue> {
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        let is_real_option = matches!(scrutinee_ty, MirType::Struct(n) if n.as_ref() == "Option");

        let scrut_local = builder.create_local(scrutinee_ty.clone());
        builder.assign(scrut_local, MirRValue::Use(scrutinee_val));

        // Build the condition: for real Option, check has_value; for i32 fallback, check != 0
        let has_value = if is_real_option {
            let hv = builder.create_local(MirType::Bool);
            builder.assign(
                hv,
                MirRValue::FieldAccess {
                    base: values::local(scrut_local),
                    field_name: Arc::from("has_value"),
                    field_ty: MirType::Bool,
                },
            );
            hv
        } else {
            // Fallback: treat as truthy (non-zero = Some, zero = None)
            let hv = builder.create_local(MirType::Bool);
            builder.assign(hv, MirRValue::Use(MirValue::Const(MirConst::Bool(true))));
            hv
        };

        let merge_block = builder.create_block();
        let some_block = builder.create_block();
        let none_block = builder.create_block();

        // Determine result type from function return type
        let ret_ty = builder.return_type().clone();
        let result_ty = if ret_ty == MirType::Void {
            MirType::i32()
        } else {
            ret_ty
        };
        let result = builder.create_local(result_ty);

        builder.branch(values::local(has_value), some_block, none_block);

        // Process each arm
        for arm in arms {
            let is_none = matches!(&arm.pattern.kind,
                ast::PatternKind::Ident { name, .. } if name.name.as_ref() == "None"
            ) || matches!(&arm.pattern.kind,
                ast::PatternKind::TupleStruct { path, patterns } if {
                    let name = path.segments.last().map(|s| s.ident.name.as_ref()).unwrap_or("");
                    name == "None" && patterns.is_empty()
                }
            );

            let is_some = matches!(&arm.pattern.kind,
                ast::PatternKind::TupleStruct { path, .. } if {
                    let name = path.segments.last().map(|s| s.ident.name.as_ref()).unwrap_or("");
                    name == "Some"
                }
            );

            if is_none {
                let builder = self.current_fn.as_mut().unwrap();
                builder.switch_to_block(none_block);
                let body_val = self.lower_expr(&arm.body)?;
                let builder = self.current_fn.as_mut().unwrap();
                if !matches!(body_val, MirValue::Const(MirConst::Unit)) {
                    builder.assign(result, MirRValue::Use(body_val));
                }
                builder.goto(merge_block);
            } else if is_some {
                let builder = self.current_fn.as_mut().unwrap();
                builder.switch_to_block(some_block);

                // Bind the inner pattern variable.
                // For real Option: extract from .value field (i32 from union).
                // For i32 fallback: bind directly from scrutinee.
                if let ast::PatternKind::TupleStruct { patterns, .. } = &arm.pattern.kind {
                    for pat in patterns.iter() {
                        if let ast::PatternKind::Ident { name, .. } = &pat.kind {
                            let builder = self.current_fn.as_mut().unwrap();
                            let inner_ty = if is_real_option {
                                // Priority: threaded override (binding annotation
                                // or matched call's `-> Option<T>`), then the
                                // per-local table, then i32.
                                inner_ty_override
                                    .clone()
                                    .or_else(|| self.option_inner_types.get(&scrut_local).cloned())
                                    .unwrap_or_else(MirType::i32)
                            } else {
                                scrutinee_ty.clone() // i32 fallback - same type as scrutinee
                            };
                            let inner_local =
                                builder.create_named_local(name.name.clone(), inner_ty.clone());
                            if is_real_option {
                                builder.assign(
                                    inner_local,
                                    MirRValue::FieldAccess {
                                        base: values::local(scrut_local),
                                        field_name: Arc::from("value"),
                                        field_ty: inner_ty,
                                    },
                                );
                            } else {
                                // i32 fallback: just copy the scrutinee as the inner value
                                builder.assign(
                                    inner_local,
                                    MirRValue::Use(values::local(scrut_local)),
                                );
                            }
                            self.var_map.insert(name.name.clone(), inner_local);
                        }
                    }
                }

                let body_val = self.lower_expr(&arm.body)?;
                let builder = self.current_fn.as_mut().unwrap();
                if !matches!(body_val, MirValue::Const(MirConst::Unit)) {
                    builder.assign(result, MirRValue::Use(body_val));
                }
                builder.goto(merge_block);
            } else {
                // Wildcard / other - treat as default arm going to merge
                let builder = self.current_fn.as_mut().unwrap();
                builder.switch_to_block(none_block);
                let body_val = self.lower_expr(&arm.body)?;
                let builder = self.current_fn.as_mut().unwrap();
                if !matches!(body_val, MirValue::Const(MirConst::Unit)) {
                    builder.assign(result, MirRValue::Use(body_val));
                }
                builder.goto(merge_block);
            }
        }

        let builder = self.current_fn.as_mut().unwrap();
        builder.switch_to_block(merge_block);
        Ok(values::local(result))
    }

    /// Lower `match res { Ok(x) => body1, Err(e) => body2 }` for runtime Result.
    /// The runtime Result struct has `is_ok: bool`, `ok: union { i64 ok_i;
    /// double ok_f; void* ok_p; }`, and `err: BuildString`. The Ok payload type
    /// is threaded in (`ok_ty`) so the union slot is read with the correct type
    /// rather than defaulting to i32 (which would silently mis-read f64/pointer
    /// Ok payloads). The Err payload is always a BuildString in the current
    /// runtime.
    fn lower_runtime_result_match(
        &mut self,
        scrutinee_val: MirValue,
        scrutinee_ty: &MirType,
        arms: &[ast::MatchArm],
        ok_ty: MirType,
        err_ty: MirType,
    ) -> CodegenResult<MirValue> {
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        let scrut_local = builder.create_local(scrutinee_ty.clone());
        builder.assign(scrut_local, MirRValue::Use(scrutinee_val));

        // Discriminant: is_ok.
        let is_ok = builder.create_local(MirType::Bool);
        builder.assign(
            is_ok,
            MirRValue::FieldAccess {
                base: values::local(scrut_local),
                field_name: Arc::from("is_ok"),
                field_ty: MirType::Bool,
            },
        );

        let merge_block = builder.create_block();
        let ok_block = builder.create_block();
        let err_block = builder.create_block();

        // Result type for the match expression value.
        let ret_ty = builder.return_type().clone();
        let result_ty = if ret_ty == MirType::Void {
            MirType::i32()
        } else {
            ret_ty
        };
        let result = builder.create_local(result_ty);

        builder.branch(values::local(is_ok), ok_block, err_block);

        for arm in arms {
            let variant = match &arm.pattern.kind {
                ast::PatternKind::TupleStruct { path, .. } => {
                    path.segments.last().map(|s| s.ident.name.as_ref())
                }
                _ => None,
            };
            let is_ok_arm = variant == Some("Ok");
            let is_err_arm = variant == Some("Err");

            // Choose the arm's block, the bound field name, and the bound type.
            let (block, field_name, bind_ty) = if is_ok_arm {
                (ok_block, "ok", ok_ty.clone())
            } else if is_err_arm {
                (err_block, "err", err_ty.clone())
            } else {
                // Wildcard / binding arm: treat as the Err fall-through so a
                // catch-all still produces a value.
                (err_block, "err", err_ty.clone())
            };

            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(block);

            // Bind the inner pattern variable from the typed field.
            if let ast::PatternKind::TupleStruct { patterns, .. } = &arm.pattern.kind {
                for pat in patterns.iter() {
                    if let ast::PatternKind::Ident { name, .. } = &pat.kind {
                        let builder = self.current_fn.as_mut().unwrap();
                        let inner_local =
                            builder.create_named_local(name.name.clone(), bind_ty.clone());
                        builder.assign(
                            inner_local,
                            MirRValue::FieldAccess {
                                base: values::local(scrut_local),
                                field_name: Arc::from(field_name),
                                field_ty: bind_ty.clone(),
                            },
                        );
                        self.var_map.insert(name.name.clone(), inner_local);
                    }
                }
            }

            let body_val = self.lower_expr(&arm.body)?;
            let builder = self.current_fn.as_mut().unwrap();
            if !matches!(body_val, MirValue::Const(MirConst::Unit)) {
                builder.assign(result, MirRValue::Use(body_val));
            }
            builder.goto(merge_block);
        }

        let builder = self.current_fn.as_mut().unwrap();
        builder.switch_to_block(merge_block);
        Ok(values::local(result))
    }

    /// Determine the Ok payload type of a Result-valued match scrutinee.
    /// Priority: a let-binding annotation tracked per-local, then the return
    /// signature of a directly-matched call, then i32. The Err payload is
    /// always a BuildString in the current runtime.
    fn result_ok_type_for(&self, scrutinee: &ast::Expr, scrutinee_val: &MirValue) -> MirType {
        // Local tracked from a `let r: Result<Ok, Err> = ...` annotation.
        if let MirValue::Local(id) = scrutinee_val {
            if let Some(ty) = self.result_ok_types.get(id) {
                return ty.clone();
            }
        }
        // Direct `match call() { Ok(x) => ... }`: recover from the callee sig.
        if let Some(name) = self.scrutinee_callee_name(scrutinee) {
            if let Some(ty) = self.fn_result_ok_types.get(&name) {
                return ty.clone();
            }
        }
        // A user-defined `enum Result { Ok(T), Err(E) }` carries the real payload
        // types; read Ok's field directly so a matched local whose annotation and
        // callee are both unavailable is not silently defaulted to the wrong slot.
        if let Some((_, fields)) = self.lookup_enum_variant("Result", "Ok") {
            if let Some((_, ty)) = fields.first() {
                return ty.clone();
            }
        }
        MirType::i32()
    }

    /// Resolve the function/method name a match scrutinee calls, if any. A free
    /// call yields its (possibly module-qualified) name; a method call resolves
    /// the receiver's type to the mangled `Type_method` name. Used to look up
    /// the per-function Result/Option payload-type tables.
    fn scrutinee_callee_name(&self, scrutinee: &ast::Expr) -> Option<Arc<str>> {
        match &scrutinee.kind {
            ExprKind::Call { func, .. } => match &func.kind {
                ExprKind::Ident(ident) => Some(ident.name.clone()),
                ExprKind::Path(path) => path.last_ident().map(|i| i.name.clone()),
                _ => None,
            },
            ExprKind::MethodCall {
                receiver, method, ..
            } => {
                let recv_ty = self.infer_expr_type(receiver);
                let type_name = match &recv_ty {
                    MirType::Struct(n) => Some(n.clone()),
                    MirType::Ptr(inner) => match &**inner {
                        MirType::Struct(n) => Some(n.clone()),
                        _ => None,
                    },
                    _ => None,
                }?;
                self.impl_methods
                    .get(&(type_name, method.name.clone()))
                    .cloned()
            }
            _ => None,
        }
    }

    /// Determine the Err payload type of a Result-valued match scrutinee.
    /// Priority: a let-binding annotation tracked per-local, then the matched
    /// call's `Result<Ok, Err>` signature, then BuildString (the common
    /// String-error case, which keeps unannotated string-error matches working).
    fn result_err_type_for(&self, scrutinee: &ast::Expr, scrutinee_val: &MirValue) -> MirType {
        if let MirValue::Local(id) = scrutinee_val {
            if let Some(ty) = self.result_err_types.get(id) {
                return ty.clone();
            }
        }
        if let Some(name) = self.scrutinee_callee_name(scrutinee) {
            if let Some(ty) = self.fn_result_err_types.get(&name) {
                return ty.clone();
            }
        }
        // A user-defined `enum Result { Ok(T), Err(E) }` carries the real Err
        // payload type; read it directly so a scalar-error match (e.g. `Err(i32)`)
        // reads the `err_i` slot instead of dereferencing it as a boxed String.
        if let Some((_, fields)) = self.lookup_enum_variant("Result", "Err") {
            if let Some((_, ty)) = fields.first() {
                return ty.clone();
            }
        }
        MirType::Struct(Arc::from("BuildString"))
    }

    /// Determine the payload type of an Option-valued match scrutinee, when it
    /// can be recovered. Priority: a let-binding annotation tracked per-local,
    /// then the `-> Option<T>` return of a directly-matched call. Returns None
    /// when neither is available (the match then falls back to i32).
    fn option_inner_type_for(
        &self,
        scrutinee: &ast::Expr,
        scrutinee_val: &MirValue,
    ) -> Option<MirType> {
        // Local tracked from a `let o: Option<T> = ...` annotation.
        if let MirValue::Local(id) = scrutinee_val {
            if let Some(ty) = self.option_inner_types.get(id) {
                return Some(ty.clone());
            }
        }
        // Direct `match call() { Some(x) => ... }`: recover from the callee sig.
        if let Some(name) = self.scrutinee_callee_name(scrutinee) {
            if let Some(ty) = self.fn_option_inner_types.get(&name) {
                return Some(ty.clone());
            }
        }
        None
    }

    fn lower_match(
        &mut self,
        scrutinee: &ast::Expr,
        arms: &[ast::MatchArm],
    ) -> CodegenResult<MirValue> {
        // Evaluate the scrutinee once and store in a temporary.
        let scrutinee_val = self.lower_expr(scrutinee)?;
        let scrutinee_ty = self.type_of_value(&scrutinee_val);

        // If the scrutinee is a pointer to an enum (e.g. `match self` in a
        // `&self` enum method), dereference it to the enum value so the enum-tag
        // match path applies instead of an invalid struct/pointer `==`.
        let (scrutinee_val, scrutinee_ty) = match &scrutinee_ty {
            MirType::Ptr(inner) => match inner.as_ref() {
                MirType::Struct(name) if self.is_enum_type(name) => {
                    let pointee = (**inner).clone();
                    let builder = self.current_fn.as_mut().unwrap();
                    let derefed = builder.create_local(pointee.clone());
                    builder.assign(
                        derefed,
                        MirRValue::Deref {
                            ptr: scrutinee_val,
                            pointee_ty: pointee.clone(),
                        },
                    );
                    (values::local(derefed), pointee)
                }
                _ => (scrutinee_val, scrutinee_ty),
            },
            _ => (scrutinee_val, scrutinee_ty),
        };

        // Runtime Result match: `match res { Ok(x) => ..., Err(e) => ... }`
        // The runtime Result struct has `is_ok: bool`, an `ok` union, and an
        // `err: BuildString`. The Ok payload type is threaded from the binding
        // annotation or callee signature so the union slot is read correctly.
        let is_runtime_result =
            matches!(&scrutinee_ty, MirType::Struct(n) if n.as_ref() == "Result");
        if is_runtime_result {
            let ok_ty = self.result_ok_type_for(scrutinee, &scrutinee_val);
            let err_ty = self.result_err_type_for(scrutinee, &scrutinee_val);
            return self.lower_runtime_result_match(
                scrutinee_val,
                &scrutinee_ty,
                arms,
                ok_ty,
                err_ty,
            );
        }

        // Runtime Option match: `match opt { Some(x) => ..., None => ... }`
        // The runtime Option struct has fields `has_value: bool` and `value: union`.
        let is_runtime_option =
            matches!(&scrutinee_ty, MirType::Struct(n) if n.as_ref() == "Option");
        if is_runtime_option {
            let inner = self.option_inner_type_for(scrutinee, &scrutinee_val);
            return self.lower_runtime_option_match(scrutinee_val, &scrutinee_ty, arms, inner);
        }

        // Check if this is an enum match (scrutinee type is a known enum).
        let is_enum_match = if let MirType::Struct(ref name) = scrutinee_ty {
            self.is_enum_type(name)
        } else {
            false
        };

        // Pattern-based Option detection: if the arms contain Some/None
        // patterns but the scrutinee is a primitive (i32/f64/bool from a
        // fallback-typed method call), use the runtime Option match path
        // to properly declare inner binding variables.
        if !is_enum_match {
            let is_primitive_scrutinee = scrutinee_ty == MirType::i32();
            if is_primitive_scrutinee {
                let has_option_arms = arms.iter().any(|arm| {
                    matches!(&arm.pattern.kind,
                        ast::PatternKind::TupleStruct { path, .. }
                            if path.segments.last().map(|s| s.ident.name.as_ref()) == Some("Some")
                    )
                });
                if has_option_arms {
                    // Primitive (i32) scrutinee: no override; the scrutinee type
                    // itself is the payload type.
                    return self.lower_runtime_option_match(
                        scrutinee_val,
                        &scrutinee_ty,
                        arms,
                        None,
                    );
                }
            }
        }

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        let scrutinee_local = builder.create_local(scrutinee_ty.clone());
        builder.assign(scrutinee_local, MirRValue::Use(scrutinee_val));

        let merge_block = builder.create_block();

        // Pre-create one body block per arm, plus test blocks and optional
        // guard blocks.
        let mut arm_body_blocks: Vec<BlockId> = Vec::with_capacity(arms.len());
        let mut arm_test_blocks: Vec<Option<BlockId>> = Vec::with_capacity(arms.len());
        // Guard blocks: created when an arm has a guard clause.  The guard
        // block is entered after the pattern matches; if the guard evaluates
        // to false the control falls through to the next arm.
        let mut arm_guard_blocks: Vec<Option<BlockId>> = Vec::with_capacity(arms.len());

        for arm in arms {
            let builder = self.current_fn.as_mut().unwrap();
            let body_block = builder.create_block();
            arm_body_blocks.push(body_block);

            let is_wildcard_or_binding = matches!(
                arm.pattern.kind,
                ast::PatternKind::Wildcard | ast::PatternKind::Ident { .. }
            );
            // A wildcard/binding arm still needs a test block if it has a
            // guard clause, because the guard may fail.
            if is_wildcard_or_binding && arm.guard.is_none() {
                arm_test_blocks.push(None); // No comparison needed
            } else {
                let test_block = builder.create_block();
                arm_test_blocks.push(Some(test_block));
            }

            if arm.guard.is_some() {
                let guard_block = builder.create_block();
                arm_guard_blocks.push(Some(guard_block));
            } else {
                arm_guard_blocks.push(None);
            }
        }

        // Determine the result type.  For enum matches where the arms
        // produce a non-enum value, we use the enclosing function's return
        // type as a best-effort guess.  For simpler matches use the scrutinee type.
        // Prefer the type inferred from the arm bodies (drives out the i32/
        // scrutinee guesses); fall back to the enclosing return type (enum
        // matches) or the scrutinee type only when inference yields nothing.
        let mut inferred_arm_ty = MirType::i32();
        for arm in arms {
            let at = self.infer_expr_type(&arm.body);
            if at != MirType::i32() && at != MirType::Void {
                inferred_arm_ty = at;
                break;
            }
        }
        let result_ty = if inferred_arm_ty != MirType::i32() {
            inferred_arm_ty
        } else if is_enum_match {
            let builder = self
                .current_fn
                .as_ref()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let ret = builder.return_type().clone();
            if ret == MirType::Void {
                MirType::i32()
            } else {
                ret
            }
        } else {
            scrutinee_ty.clone()
        };

        let result = {
            let builder = self.current_fn.as_mut().unwrap();
            builder.create_local(result_ty)
        };

        // Jump into the first test (or body for wildcard/binding-only first arm).
        {
            let builder = self.current_fn.as_mut().unwrap();
            if let Some(Some(test_blk)) = arm_test_blocks.first() {
                builder.goto(*test_blk);
            } else if let Some(&body_blk) = arm_body_blocks.first() {
                builder.goto(body_blk);
            } else {
                builder.goto(merge_block);
            }
        }

        // Generate the chain of if-else test blocks and arm bodies.
        for (i, arm) in arms.iter().enumerate() {
            let body_block = arm_body_blocks[i];

            // Compute the fall-through target (next arm's test, body, or merge).
            let next_target = if i + 1 < arms.len() {
                arm_test_blocks[i + 1].unwrap_or(arm_body_blocks[i + 1])
            } else {
                merge_block
            };

            // --- Test block (if there is one) ---
            if let Some(test_block) = arm_test_blocks[i] {
                {
                    let builder = self.current_fn.as_mut().unwrap();
                    builder.switch_to_block(test_block);
                }

                // Generate the comparison based on pattern kind.
                let is_wildcard_or_binding = matches!(
                    arm.pattern.kind,
                    ast::PatternKind::Wildcard | ast::PatternKind::Ident { .. }
                );

                let cond_val = if is_wildcard_or_binding {
                    // Wildcard/binding patterns always match; this test block
                    // exists only because there is a guard clause.
                    values::bool(true)
                } else if is_enum_match {
                    self.lower_enum_pattern_test(&arm.pattern, scrutinee_local, &scrutinee_ty)?
                } else {
                    self.lower_pattern_test(&arm.pattern, values::local(scrutinee_local))?
                };

                // If there is a guard, jump to the guard block on pattern
                // match success; otherwise jump directly to the body.
                let on_match = arm_guard_blocks[i].unwrap_or(body_block);

                let builder = self.current_fn.as_mut().unwrap();
                builder.branch(cond_val, on_match, next_target);
            }

            // --- Guard block (if the arm has a guard clause) ---
            if let Some(guard_block) = arm_guard_blocks[i] {
                {
                    let builder = self.current_fn.as_mut().unwrap();
                    builder.switch_to_block(guard_block);
                }

                // Bind pattern variables *before* evaluating the guard so
                // that guard expressions like `x if x < 0` can reference `x`.
                if is_enum_match {
                    self.bind_enum_pattern_vars(&arm.pattern, scrutinee_local, &scrutinee_ty)?;
                } else if let ast::PatternKind::Ident { name, .. } = &arm.pattern.kind {
                    let builder = self.current_fn.as_mut().unwrap();
                    let local = builder.create_local(scrutinee_ty.clone());
                    builder.assign(local, MirRValue::Use(values::local(scrutinee_local)));
                    self.var_map.insert(name.name.clone(), local);
                } else if matches!(&arm.pattern.kind, ast::PatternKind::Struct { .. }) {
                    // Bind record-pattern fields before the guard so a guard
                    // expression can reference them (`Point { x } if x < 0`).
                    self.bind_struct_pattern_vars(&arm.pattern, scrutinee_local, &scrutinee_ty)?;
                }

                let guard_expr = arm.guard.as_ref().unwrap();
                let guard_val = self.lower_expr(guard_expr)?;

                let builder = self.current_fn.as_mut().unwrap();
                builder.branch(guard_val, body_block, next_target);
            }

            // --- Body block ---
            {
                let builder = self.current_fn.as_mut().unwrap();
                builder.switch_to_block(body_block);
            }

            // Bind pattern variables (skip if already bound in guard block).
            if arm_guard_blocks[i].is_none() {
                if is_enum_match {
                    self.bind_enum_pattern_vars(&arm.pattern, scrutinee_local, &scrutinee_ty)?;
                } else if let ast::PatternKind::Ident { name, .. } = &arm.pattern.kind {
                    let builder = self.current_fn.as_mut().unwrap();
                    let local = builder.create_local(scrutinee_ty.clone());
                    builder.assign(local, MirRValue::Use(values::local(scrutinee_local)));
                    self.var_map.insert(name.name.clone(), local);
                } else if let ast::PatternKind::TupleStruct { patterns, .. } = &arm.pattern.kind {
                    // Bind inner pattern variables from TupleStruct patterns
                    // like Some(x), Ok(val), etc. Use tracked inner type if the
                    // scrutinee is a runtime Option with a known inner type;
                    // otherwise fall back to the scrutinee type.
                    let binding_ty = self
                        .option_inner_types
                        .get(&scrutinee_local)
                        .cloned()
                        .unwrap_or_else(|| {
                            // Infer the scrutinee's value type statically (e.g.
                            // self.map.get(k) -> V); fall back to the scrutinee local
                            // type only when inference yields nothing.
                            let inferred = self.infer_expr_type(scrutinee);
                            if inferred != MirType::i32() {
                                inferred
                            } else {
                                scrutinee_ty.clone()
                            }
                        });
                    for pat in patterns.iter() {
                        if let ast::PatternKind::Ident { name, .. } = &pat.kind {
                            let builder = self.current_fn.as_mut().unwrap();
                            let local =
                                builder.create_named_local(name.name.clone(), binding_ty.clone());
                            builder.assign(local, MirRValue::Use(values::local(scrutinee_local)));
                            self.var_map.insert(name.name.clone(), local);
                        }
                    }
                } else if matches!(&arm.pattern.kind, ast::PatternKind::Struct { .. }) {
                    // Record pattern `Point { x: a, y: b }` / `Wrap { coin: c }`:
                    // bind each field by NAME to a projection of the scrutinee.
                    // (Non-enum struct match; enum struct-variants go through
                    // `bind_enum_pattern_vars` above.)
                    self.bind_struct_pattern_vars(&arm.pattern, scrutinee_local, &scrutinee_ty)?;
                }
            }

            let body_val = self.lower_expr(&arm.body)?;
            let builder = self.current_fn.as_mut().unwrap();
            // Skip assigning Unit (void) to a non-void result local -- C
            // does not allow `int x = (void)0;`.  This happens when match
            // arms contain side-effect-only expressions like println!.
            if !matches!(body_val, MirValue::Const(MirConst::Unit)) {
                builder.assign(result, MirRValue::Use(body_val));
            }
            builder.goto(merge_block);
        }

        let builder = self.current_fn.as_mut().unwrap();
        builder.switch_to_block(merge_block);

        Ok(values::local(result))
    }

    /// Generate a boolean test for enum pattern matching.
    /// Compares the scrutinee's tag against the expected variant discriminant.
    fn lower_enum_pattern_test(
        &mut self,
        pattern: &ast::Pattern,
        scrutinee_local: LocalId,
        scrutinee_ty: &MirType,
    ) -> CodegenResult<MirValue> {
        let enum_name = if let MirType::Struct(name) = scrutinee_ty {
            name.clone()
        } else {
            return Ok(values::bool(true));
        };

        match &pattern.kind {
            ast::PatternKind::TupleStruct { path, .. } => {
                // Extract variant name from path (e.g., Shape::Circle -> "Circle")
                let variant_name = path
                    .segments
                    .last()
                    .map(|s| s.ident.name.clone())
                    .unwrap_or(Arc::from(""));

                // Look up the discriminant for this variant
                let disc = self
                    .lookup_enum_variant(&enum_name, &variant_name)
                    .map(|(d, _)| d)
                    .unwrap_or(0);

                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

                // Read the tag: scrutinee.tag
                let tag_local = builder.create_local(MirType::i32());
                builder.assign(
                    tag_local,
                    MirRValue::FieldAccess {
                        base: values::local(scrutinee_local),
                        field_name: Arc::from("tag"),
                        field_ty: MirType::i32(),
                    },
                );

                // Compare: tag == expected_discriminant
                let cmp = builder.create_local(MirType::Bool);
                builder.binary_op(
                    cmp,
                    BinOp::Eq,
                    values::local(tag_local),
                    MirValue::Const(MirConst::Int(disc, MirType::i32())),
                );

                Ok(values::local(cmp))
            }

            ast::PatternKind::Path(path) => {
                // Unit variant (no payload): Shape::Unit
                let variant_name = path
                    .segments
                    .last()
                    .map(|s| s.ident.name.clone())
                    .unwrap_or(Arc::from(""));

                let disc = self
                    .lookup_enum_variant(&enum_name, &variant_name)
                    .map(|(d, _)| d)
                    .unwrap_or(0);

                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

                let tag_local = builder.create_local(MirType::i32());
                builder.assign(
                    tag_local,
                    MirRValue::FieldAccess {
                        base: values::local(scrutinee_local),
                        field_name: Arc::from("tag"),
                        field_ty: MirType::i32(),
                    },
                );

                let cmp = builder.create_local(MirType::Bool);
                builder.binary_op(
                    cmp,
                    BinOp::Eq,
                    values::local(tag_local),
                    MirValue::Const(MirConst::Int(disc, MirType::i32())),
                );

                Ok(values::local(cmp))
            }

            ast::PatternKind::Wildcard => Ok(values::bool(true)),
            ast::PatternKind::Ident { .. } => Ok(values::bool(true)),

            _ => Ok(values::bool(true)),
        }
    }

    /// Bind variables from an enum variant pattern.
    /// For `Shape::Circle(r)`, this creates a local `r` and assigns
    /// `scrutinee.data.Circle.f0` to it.
    fn bind_enum_pattern_vars(
        &mut self,
        pattern: &ast::Pattern,
        scrutinee_local: LocalId,
        scrutinee_ty: &MirType,
    ) -> CodegenResult<()> {
        let enum_name = if let MirType::Struct(name) = scrutinee_ty {
            name.clone()
        } else {
            return Ok(());
        };

        match &pattern.kind {
            ast::PatternKind::TupleStruct { path, patterns } => {
                let variant_name = path
                    .segments
                    .last()
                    .map(|s| s.ident.name.clone())
                    .unwrap_or(Arc::from(""));

                // Look up variant fields to get their types
                let variant_fields = self
                    .lookup_enum_variant(&enum_name, &variant_name)
                    .map(|(_, fields)| fields)
                    .unwrap_or_default();

                for (idx, pat) in patterns.iter().enumerate() {
                    if let ast::PatternKind::Ident { name, .. } = &pat.kind {
                        // Skip wildcard `_` patterns - don't create a local
                        if name.name.as_ref() == "_" {
                            continue;
                        }
                        let field_ty = variant_fields
                            .get(idx)
                            .map(|(_, ty)| ty.clone())
                            .unwrap_or(MirType::f64());

                        let builder = self.current_fn.as_mut().ok_or_else(|| {
                            CodegenError::Internal("No current function".to_string())
                        })?;

                        let local = builder.create_named_local(name.name.clone(), field_ty.clone());
                        builder.assign(
                            local,
                            MirRValue::VariantField {
                                base: values::local(scrutinee_local),
                                variant_name: variant_name.clone(),
                                field_index: idx as u32,
                                field_ty,
                            },
                        );
                        self.var_map.insert(name.name.clone(), local);
                    }
                    // Wildcard patterns in enum variant bindings are ignored.
                }
            }

            ast::PatternKind::Ident { name, .. } => {
                // Bind the entire scrutinee to the variable
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let local = builder.create_local(scrutinee_ty.clone());
                builder.assign(local, MirRValue::Use(values::local(scrutinee_local)));
                self.var_map.insert(name.name.clone(), local);
            }

            _ => {}
        }

        Ok(())
    }

    /// Bind variables from a STRUCT RECORD pattern: `Point { x: a, y: b }` or the
    /// shorthand `Point { x, y }`. Each field pattern binds its sub-pattern to a
    /// FIELD PROJECTION of the scrutinee (keyed by field NAME, not position —
    /// this is the record-pattern analogue of the positional `TupleStruct`
    /// binding in `bind_enum_pattern_vars`). Without this, record-pattern fields
    /// were dropped by the `_ => {}` catch-all and their names lowered to a
    /// dangling `Global("<name>")`, silently corrupting the arm body (a general
    /// correctness bug for ANY record pattern in `match`, and a soundness hole
    /// for linear matches: `match &w { Wrap { coin: c } => spend(c) }` moved the
    /// linear out of a shared borrow without being tracked).
    ///
    /// The scrutinee local may be the struct itself OR a `Ptr` to it (matching
    /// through a `&` borrow, `match &w`). `FieldAccess` on a pointer-typed base
    /// is the established auto-deref field-read idiom (see `lower_field`); the
    /// field type is looked up through the pointee. The resulting binding
    /// (`c = FieldAccess(*scrutinee)`) is a field READ through the borrowed
    /// scrutinee, which the linear checker tracks.
    fn bind_struct_pattern_vars(
        &mut self,
        pattern: &ast::Pattern,
        scrutinee_local: LocalId,
        scrutinee_ty: &MirType,
    ) -> CodegenResult<()> {
        let ast::PatternKind::Struct { fields, .. } = &pattern.kind else {
            return Ok(());
        };

        // The struct name to look up field types through, auto-dereferencing a
        // pointer scrutinee (matching a struct through a `&` borrow).
        let struct_name = match scrutinee_ty {
            MirType::Struct(name) => name.clone(),
            MirType::Ptr(inner) => match inner.as_ref() {
                MirType::Struct(name) => name.clone(),
                _ => return Ok(()),
            },
            _ => return Ok(()),
        };

        for fp in fields.iter() {
            // The bound name: the sub-pattern if it is an `Ident` (`x: a` binds
            // `a`), else the field name itself (shorthand `x` binds `x`). Skip a
            // wildcard sub-pattern (`x: _`) — it binds nothing.
            let bind_name = match &fp.pattern.kind {
                ast::PatternKind::Ident { name, .. } => {
                    if name.name.as_ref() == "_" {
                        continue;
                    }
                    name.name.clone()
                }
                ast::PatternKind::Wildcard => continue,
                // Nested destructuring sub-patterns are not yet supported here;
                // bind under the field name conservatively so at least the field
                // is projected (matches the shorthand behavior).
                _ => fp.name.name.clone(),
            };

            let field_ty = self
                .lookup_struct_field_type(&struct_name, &fp.name.name)
                .unwrap_or(MirType::i32());

            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let local = builder.create_named_local(bind_name.clone(), field_ty.clone());
            builder.assign(
                local,
                MirRValue::FieldAccess {
                    base: values::local(scrutinee_local),
                    field_name: fp.name.name.clone(),
                    field_ty,
                },
            );
            self.var_map.insert(bind_name, local);
        }

        Ok(())
    }

    /// Generate a boolean MIR value that is true when `scrutinee_val` matches
    /// `pattern`.  Supports literal, wildcard, and simple variable patterns.
    fn lower_pattern_test(
        &mut self,
        pattern: &ast::Pattern,
        scrutinee_val: MirValue,
    ) -> CodegenResult<MirValue> {
        match &pattern.kind {
            // Wildcard always matches.
            ast::PatternKind::Wildcard => Ok(values::bool(true)),

            // Variable binding always matches (binding happens in the caller).
            ast::PatternKind::Ident { .. } => Ok(values::bool(true)),

            // Literal patterns: emit scrutinee == literal.
            ast::PatternKind::Literal(lit) => {
                let lit_val = self.lower_literal(lit)?;
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let cmp = builder.create_local(MirType::Bool);
                builder.binary_op(cmp, BinOp::Eq, scrutinee_val, lit_val);
                Ok(values::local(cmp))
            }

            // Or-pattern: any sub-pattern matching is a match.
            ast::PatternKind::Or(pats) if !pats.is_empty() => {
                let first = self.lower_pattern_test(&pats[0], scrutinee_val.clone())?;
                let mut current = first;
                for pat in &pats[1..] {
                    let rhs = self.lower_pattern_test(pat, scrutinee_val.clone())?;
                    let builder = self.current_fn.as_mut().unwrap();
                    let combined = builder.create_local(MirType::Bool);
                    builder.binary_op(combined, BinOp::BitOr, current, rhs);
                    current = values::local(combined);
                }
                Ok(current)
            }

            // Path patterns (e.g. enum variants without payload) -- compare
            // as equality for now (this is a simplification).
            ast::PatternKind::Path(path) => {
                let path_val = self.lower_path(path)?;
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let cmp = builder.create_local(MirType::Bool);
                builder.binary_op(cmp, BinOp::Eq, scrutinee_val, path_val);
                Ok(values::local(cmp))
            }

            // Unsupported pattern kinds fall through to "always matches" to
            // avoid panicking.  A real implementation would need full pattern
            // compilation here.
            // Range pattern: lo..=hi → scrutinee >= lo && scrutinee <= hi
            ast::PatternKind::Range {
                start,
                end,
                inclusive,
            } => {
                let lo_val = if let Some(lo_expr) = start {
                    self.lower_expr(lo_expr)?
                } else {
                    values::i32(i32::MIN)
                };
                let hi_val = if let Some(hi_expr) = end {
                    self.lower_expr(hi_expr)?
                } else {
                    values::i32(i32::MAX)
                };

                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

                let ge = builder.create_local(MirType::Bool);
                builder.binary_op(ge, BinOp::Ge, scrutinee_val.clone(), lo_val);

                let le_op = if *inclusive { BinOp::Le } else { BinOp::Lt };
                let le = builder.create_local(MirType::Bool);
                builder.binary_op(le, le_op, scrutinee_val, hi_val);

                let result = builder.create_local(MirType::Bool);
                builder.binary_op(result, BinOp::BitAnd, values::local(ge), values::local(le));

                Ok(values::local(result))
            }

            _ => Ok(values::bool(true)),
        }
    }

    fn lower_loop(
        &mut self,
        body: &ast::Block,
        label: Option<&ast::Ident>,
    ) -> CodegenResult<MirValue> {
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        let loop_block = builder.create_block();
        let exit_block = builder.create_block();

        self.loop_stack
            .push((loop_block, exit_block, label_name(label)));

        builder.goto(loop_block);
        builder.switch_to_block(loop_block);

        // Save var_map so `let` bindings inside the loop body do not
        // shadow outer variables after the loop exits.
        let saved_vars = self.var_map.clone();
        self.lower_block(body)?;
        self.var_map = saved_vars;

        let builder = self.current_fn.as_mut().unwrap();
        builder.goto(loop_block);

        self.loop_stack.pop();

        builder.switch_to_block(exit_block);

        Ok(values::unit())
    }

    fn lower_while(
        &mut self,
        condition: &ast::Expr,
        body: &ast::Block,
        label: Option<&ast::Ident>,
    ) -> CodegenResult<MirValue> {
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let exit_block = builder.create_block();

        self.loop_stack
            .push((cond_block, exit_block, label_name(label)));

        builder.goto(cond_block);
        builder.switch_to_block(cond_block);

        let cond_val = self.lower_expr(condition)?;
        let builder = self.current_fn.as_mut().unwrap();
        builder.branch(cond_val, body_block, exit_block);

        builder.switch_to_block(body_block);
        // Save var_map so `let` bindings inside the loop body do not
        // shadow outer variables after the loop exits.
        let saved_vars = self.var_map.clone();
        self.lower_block(body)?;
        self.var_map = saved_vars;
        let builder = self.current_fn.as_mut().unwrap();
        builder.goto(cond_block);

        self.loop_stack.pop();

        builder.switch_to_block(exit_block);

        Ok(values::unit())
    }

    /// Lower `while let Some(x) = expr { body }`.
    /// Evaluates expr on each iteration, binds x from TupleStruct pattern,
    /// breaks when the pattern doesn't match (None/default).
    fn lower_while_let(
        &mut self,
        pattern: &ast::Pattern,
        scrutinee: &ast::Expr,
        body: &ast::Block,
        label: Option<&ast::Ident>,
    ) -> CodegenResult<MirValue> {
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let exit_block = builder.create_block();

        self.loop_stack
            .push((cond_block, exit_block, label_name(label)));

        builder.goto(cond_block);
        builder.switch_to_block(cond_block);

        // Evaluate the scrutinee on each iteration
        let scrut_val = self.lower_expr(scrutinee)?;
        let scrut_ty = self.type_of_value(&scrut_val);

        let builder = self.current_fn.as_mut().unwrap();
        let scrut_local = builder.create_local(scrut_ty.clone());
        builder.assign(scrut_local, MirRValue::Use(scrut_val));

        // For real Option: check has_value. For i32 fallback: always enter body.
        let is_real_option = matches!(&scrut_ty, MirType::Struct(n) if n.as_ref() == "Option");
        if is_real_option {
            let has_value = builder.create_local(MirType::Bool);
            builder.assign(
                has_value,
                MirRValue::FieldAccess {
                    base: values::local(scrut_local),
                    field_name: Arc::from("has_value"),
                    field_ty: MirType::Bool,
                },
            );
            builder.branch(values::local(has_value), body_block, exit_block);
        } else {
            // i32 fallback: always enter body (the loop will break via other means)
            builder.goto(body_block);
        }

        // Body block: bind pattern variables
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(body_block);
        }

        let saved_vars = self.var_map.clone();

        // Bind the inner variable from TupleStruct pattern (e.g., Some(p))
        if let ast::PatternKind::TupleStruct { patterns, .. } = &pattern.kind {
            for pat in patterns.iter() {
                if let ast::PatternKind::Ident { name, .. } = &pat.kind {
                    let builder = self.current_fn.as_mut().unwrap();
                    let inner_ty = if is_real_option {
                        MirType::i32()
                    } else {
                        scrut_ty.clone()
                    };
                    let inner_local =
                        builder.create_named_local(name.name.clone(), inner_ty.clone());
                    if is_real_option {
                        builder.assign(
                            inner_local,
                            MirRValue::FieldAccess {
                                base: values::local(scrut_local),
                                field_name: Arc::from("value"),
                                field_ty: inner_ty,
                            },
                        );
                    } else {
                        builder.assign(inner_local, MirRValue::Use(values::local(scrut_local)));
                    }
                    self.var_map.insert(name.name.clone(), inner_local);
                }
            }
        } else if let ast::PatternKind::Ident { name, .. } = &pattern.kind {
            let builder = self.current_fn.as_mut().unwrap();
            let local = builder.create_named_local(name.name.clone(), scrut_ty.clone());
            builder.assign(local, MirRValue::Use(values::local(scrut_local)));
            self.var_map.insert(name.name.clone(), local);
        }

        self.lower_block(body)?;
        self.var_map = saved_vars;

        let builder = self.current_fn.as_mut().unwrap();
        builder.goto(cond_block);

        self.loop_stack.pop();

        let builder = self.current_fn.as_mut().unwrap();
        builder.switch_to_block(exit_block);

        Ok(values::unit())
    }

    fn lower_for(
        &mut self,
        pattern: &ast::Pattern,
        iter: &ast::Expr,
        body: &ast::Block,
        label: Option<&ast::Ident>,
    ) -> CodegenResult<MirValue> {
        // Detect `.step_by(n)` on a range: `(start..end).step_by(n)` or `(start..=end).step_by(n)`
        if let ExprKind::MethodCall {
            receiver,
            method,
            args,
            ..
        } = &iter.kind
        {
            if method.name.as_ref() == "step_by" && args.len() == 1 {
                if let Some((start, end, inclusive)) = Self::extract_range_parts(receiver) {
                    return self.lower_for_range(
                        pattern,
                        start,
                        end,
                        inclusive,
                        body,
                        Some(&args[0]),
                        label,
                    );
                }
            }
        }

        // Detect range-based for loops: `for i in start..end` or `for i in start..=end`
        // and lower them into an explicit counted loop.
        if let ExprKind::Range {
            start,
            end,
            inclusive,
        } = &iter.kind
        {
            return self.lower_for_range(
                pattern,
                start.as_deref(),
                end.as_deref(),
                *inclusive,
                body,
                None,
                label,
            );
        }

        // Also detect binary Range/RangeInclusive operators produced by the
        // parser for `0..10` style expressions.
        if let ExprKind::Binary { op, left, right } = &iter.kind {
            if *op == AstBinOp::Range {
                return self
                    .lower_for_range(pattern, Some(left), Some(right), false, body, None, label);
            }
            if *op == AstBinOp::RangeInclusive {
                return self
                    .lower_for_range(pattern, Some(left), Some(right), true, body, None, label);
            }
        }

        // Array-based for loop: `for x in [1, 2, 3]` or `for x in arr`
        // Desugar to:
        //   let arr = <iter_expr>;
        //   let mut __idx = 0;
        //   loop_cond:
        //     if __idx >= len { goto exit }
        //     let x = arr[__idx];
        //     <body>
        //     __idx += 1;
        //     goto loop_cond
        //   exit:
        let iter_val = self.lower_expr(iter)?;
        let iter_ty = self.type_of_value(&iter_val);

        // Determine if this is an array type and extract element type + length
        let (elem_ty, arr_len) = match &iter_ty {
            MirType::Array(elem, len) => (elem.as_ref().clone(), *len),
            // `for x in v` over a Vec<T>: a runtime-length index loop. Previously
            // this fell through to the no-op loop below, silently dropping the
            // iteration (the body never ran).
            MirType::Vec(elem) => {
                let elem_ty = elem.as_ref().clone();
                return self.lower_for_vec(pattern, iter_val, elem_ty, body, label);
            }
            // `for c in s.chars()` / `for c in s`: iterate the string's bytes.
            // chars()/bytes() are identity ops, so the iterable is a BuildString.
            // Previously this fell to the no-op loop (zero iterations).
            MirType::Struct(n) if n.as_ref() == "BuildString" => {
                return self.lower_for_string(pattern, iter_val, body, label);
            }
            _ => {
                // Not an array - try iterator protocol: call .next() in a loop.
                // Requires the type to have a `next` method registered in impl_methods
                // that returns an enum with variant 0 = Some(T) and variant 1 = None.
                if let MirType::Struct(ref type_name) = iter_ty {
                    if self
                        .impl_methods
                        .contains_key(&(type_name.clone(), Arc::from("next")))
                    {
                        return self.lower_for_iterator(pattern, iter_val, &iter_ty, body, label);
                    }
                }
                // No iterator protocol - emit a no-op loop
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let exit_block = builder.create_block();
                builder.goto(exit_block);
                builder.switch_to_block(exit_block);
                return Ok(values::unit());
            }
        };

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        // Store the array in a local
        let arr_local = builder.create_local(iter_ty.clone());
        builder.assign(arr_local, MirRValue::Use(iter_val));

        // Create index counter: let mut __idx = 0;
        let idx_local = builder.create_local(MirType::i64());
        builder.assign(
            idx_local,
            MirRValue::Use(MirValue::Const(MirConst::Int(0, MirType::i64()))),
        );

        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let incr_block = builder.create_block();
        let exit_block = builder.create_block();

        self.loop_stack
            .push((incr_block, exit_block, label_name(label)));

        builder.goto(cond_block);
        builder.switch_to_block(cond_block);

        // Condition: __idx < arr_len
        let len_val = MirValue::Const(MirConst::Int(arr_len as i128, MirType::i64()));
        let cmp = builder.create_local(MirType::Bool);
        builder.binary_op(cmp, BinOp::Lt, values::local(idx_local), len_val);
        builder.branch(values::local(cmp), body_block, exit_block);

        // Body block: extract element at index and bind to pattern variable
        builder.switch_to_block(body_block);

        let elem_local = builder.create_local(elem_ty.clone());
        builder.assign(
            elem_local,
            MirRValue::IndexAccess {
                base: values::local(arr_local),
                index: values::local(idx_local),
                elem_ty: elem_ty.clone(),
            },
        );

        // Bind pattern variable and save var_map so loop-body `let`
        // bindings do not leak into the enclosing scope.
        let saved_vars = self.var_map.clone();
        self.bind_for_pattern(pattern, elem_local, &elem_ty)?;

        self.lower_block(body)?;
        self.var_map = saved_vars;

        // Goto increment block
        let builder = self.current_fn.as_mut().unwrap();
        builder.goto(incr_block);

        // Increment: __idx = __idx + 1
        builder.switch_to_block(incr_block);
        let one = MirValue::Const(MirConst::Int(1, MirType::i64()));
        let next_idx = builder.create_local(MirType::i64());
        builder.binary_op(next_idx, BinOp::Add, values::local(idx_local), one);
        builder.assign(idx_local, MirRValue::Use(values::local(next_idx)));
        builder.goto(cond_block);

        self.loop_stack.pop();

        let builder = self.current_fn.as_mut().unwrap();
        builder.switch_to_block(exit_block);

        Ok(values::unit())
    }

    /// Lower `for x in v { body }` over a `Vec<T>` to a runtime-length index loop:
    /// `len = build_hvec_len(v); for idx in 0..len { x = v[idx]; body }`. Element
    /// access uses `IndexAccess`, which the C backend lowers to the typed
    /// `build_hvec_get_<suffix>` accessor.
    fn lower_for_vec(
        &mut self,
        pattern: &ast::Pattern,
        iter_val: MirValue,
        elem_ty: MirType,
        body: &ast::Block,
        label: Option<&ast::Ident>,
    ) -> CodegenResult<MirValue> {
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        // Hold the vec handle in a local.
        let arr_local = builder.create_local(MirType::Vec(Box::new(elem_ty.clone())));
        builder.assign(arr_local, MirRValue::Use(iter_val));

        // Read the length once before the loop: `len = build_hvec_len(v)`.
        // `Call` is a terminator, so it splits into a continuation block.
        let len_local = builder.create_local(MirType::i64());
        let after_len = builder.create_block();
        builder.call(
            MirValue::Function(Arc::from("build_hvec_len")),
            vec![values::local(arr_local)],
            Some(len_local),
            after_len,
        );
        builder.switch_to_block(after_len);

        // `let mut __idx = 0;`
        let idx_local = builder.create_local(MirType::i64());
        builder.assign(
            idx_local,
            MirRValue::Use(MirValue::Const(MirConst::Int(0, MirType::i64()))),
        );

        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let incr_block = builder.create_block();
        let exit_block = builder.create_block();

        self.loop_stack
            .push((incr_block, exit_block, label_name(label)));

        let builder = self.current_fn.as_mut().unwrap();
        builder.goto(cond_block);
        builder.switch_to_block(cond_block);

        // `if __idx < len { body } else { exit }`
        let cmp = builder.create_local(MirType::Bool);
        builder.binary_op(
            cmp,
            BinOp::Lt,
            values::local(idx_local),
            values::local(len_local),
        );
        builder.branch(values::local(cmp), body_block, exit_block);

        // Body: `x = v[__idx];`
        builder.switch_to_block(body_block);
        let elem_local = builder.create_local(elem_ty.clone());
        builder.assign(
            elem_local,
            MirRValue::IndexAccess {
                base: values::local(arr_local),
                index: values::local(idx_local),
                elem_ty: elem_ty.clone(),
            },
        );

        let saved_vars = self.var_map.clone();
        self.bind_for_pattern(pattern, elem_local, &elem_ty)?;
        self.lower_block(body)?;
        self.var_map = saved_vars;

        let builder = self.current_fn.as_mut().unwrap();
        builder.goto(incr_block);

        // `__idx = __idx + 1;`
        builder.switch_to_block(incr_block);
        let one = MirValue::Const(MirConst::Int(1, MirType::i64()));
        let next_idx = builder.create_local(MirType::i64());
        builder.binary_op(next_idx, BinOp::Add, values::local(idx_local), one);
        builder.assign(idx_local, MirRValue::Use(values::local(next_idx)));
        builder.goto(cond_block);

        self.loop_stack.pop();

        let builder = self.current_fn.as_mut().unwrap();
        builder.switch_to_block(exit_block);

        Ok(values::unit())
    }

    /// Lower `for c in <string>` as a runtime-length loop over the string's
    /// bytes, binding each byte (as i32) to the pattern variable.
    fn lower_for_string(
        &mut self,
        pattern: &ast::Pattern,
        iter_val: MirValue,
        body: &ast::Block,
        label: Option<&ast::Ident>,
    ) -> CodegenResult<MirValue> {
        let byte_ty = MirType::i32();
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        let str_local = builder.create_local(MirType::Struct(Arc::from("BuildString")));
        builder.assign(str_local, MirRValue::Use(iter_val));

        // len = build_string_len(s)  (Call is a terminator -> split block).
        let len_local = builder.create_local(MirType::i64());
        let after_len = builder.create_block();
        builder.call(
            MirValue::Function(Arc::from("build_string_len")),
            vec![values::local(str_local)],
            Some(len_local),
            after_len,
        );
        builder.switch_to_block(after_len);

        let idx_local = builder.create_local(MirType::i64());
        builder.assign(
            idx_local,
            MirRValue::Use(MirValue::Const(MirConst::Int(0, MirType::i64()))),
        );

        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let incr_block = builder.create_block();
        let exit_block = builder.create_block();
        self.loop_stack
            .push((incr_block, exit_block, label_name(label)));

        let builder = self.current_fn.as_mut().unwrap();
        builder.goto(cond_block);
        builder.switch_to_block(cond_block);
        let cmp = builder.create_local(MirType::Bool);
        builder.binary_op(
            cmp,
            BinOp::Lt,
            values::local(idx_local),
            values::local(len_local),
        );
        builder.branch(values::local(cmp), body_block, exit_block);

        // Body: c = build_string_byte_at(s, idx)  (Call -> split block).
        builder.switch_to_block(body_block);
        let elem_local = builder.create_local(byte_ty.clone());
        let after_get = builder.create_block();
        builder.call(
            MirValue::Function(Arc::from("build_string_byte_at")),
            vec![values::local(str_local), values::local(idx_local)],
            Some(elem_local),
            after_get,
        );
        builder.switch_to_block(after_get);

        let saved_vars = self.var_map.clone();
        self.bind_for_pattern(pattern, elem_local, &byte_ty)?;
        self.lower_block(body)?;
        self.var_map = saved_vars;

        let builder = self.current_fn.as_mut().unwrap();
        builder.goto(incr_block);
        builder.switch_to_block(incr_block);
        let one = MirValue::Const(MirConst::Int(1, MirType::i64()));
        let next_idx = builder.create_local(MirType::i64());
        builder.binary_op(next_idx, BinOp::Add, values::local(idx_local), one);
        builder.assign(idx_local, MirRValue::Use(values::local(next_idx)));
        builder.goto(cond_block);

        self.loop_stack.pop();
        let builder = self.current_fn.as_mut().unwrap();
        builder.switch_to_block(exit_block);
        Ok(values::unit())
    }

    /// Extract range parts from an expression, unwrapping Paren and Binary wrappers.
    /// Returns `(start, end, inclusive)` if the expression is a range.
    fn extract_range_parts(
        expr: &ast::Expr,
    ) -> Option<(Option<&ast::Expr>, Option<&ast::Expr>, bool)> {
        match &expr.kind {
            ExprKind::Range {
                start,
                end,
                inclusive,
            } => Some((start.as_deref(), end.as_deref(), *inclusive)),
            ExprKind::Binary { op, left, right } if *op == AstBinOp::Range => {
                Some((Some(left.as_ref()), Some(right.as_ref()), false))
            }
            ExprKind::Binary { op, left, right } if *op == AstBinOp::RangeInclusive => {
                Some((Some(left.as_ref()), Some(right.as_ref()), true))
            }
            ExprKind::Paren(inner) => Self::extract_range_parts(inner),
            _ => None,
        }
    }

    /// Lower a range-based for loop: `for i in start..end { body }`
    ///
    /// Generates:
    /// ```text
    /// let mut i = start;     // (or 0 if start is None)
    /// loop_cond:
    ///   if i >= end { goto exit }   // (> for inclusive)
    ///   body
    ///   i = i + step           // step defaults to 1
    ///   goto loop_cond
    /// exit:
    /// ```
    fn lower_for_range(
        &mut self,
        pattern: &ast::Pattern,
        start: Option<&ast::Expr>,
        end: Option<&ast::Expr>,
        inclusive: bool,
        body: &ast::Block,
        step: Option<&ast::Expr>,
        label: Option<&ast::Ident>,
    ) -> CodegenResult<MirValue> {
        // Lower start value (default to 0)
        let start_val = if let Some(s) = start {
            self.lower_expr(s)?
        } else {
            MirValue::Const(MirConst::Int(0, MirType::i32()))
        };
        let iter_ty = self.type_of_value(&start_val);

        // Lower end value if present
        let end_val = if let Some(e) = end {
            Some(self.lower_expr(e)?)
        } else {
            None
        };

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        // Create the loop counter local and initialize it.
        let counter = builder.create_local(iter_ty.clone());
        builder.assign(counter, MirRValue::Use(start_val));

        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let incr_block = builder.create_block();
        let exit_block = builder.create_block();

        self.loop_stack
            .push((incr_block, exit_block, label_name(label)));

        builder.goto(cond_block);
        builder.switch_to_block(cond_block);

        // Condition check: counter >= end (exclusive) or counter > end (inclusive)
        if let Some(end_v) = end_val {
            let cmp_op = if inclusive { BinOp::Gt } else { BinOp::Ge };
            let cond_local = builder.create_local(MirType::Bool);
            builder.binary_op(cond_local, cmp_op, values::local(counter), end_v);
            builder.branch(values::local(cond_local), exit_block, body_block);
        } else {
            // No end bound -- infinite range, just enter body.
            builder.goto(body_block);
        }

        builder.switch_to_block(body_block);

        // Bind the loop variable and save var_map so loop-body `let`
        // bindings do not leak into the enclosing scope.
        let saved_vars = self.var_map.clone();
        if let ast::PatternKind::Ident { name, .. } = &pattern.kind {
            self.var_map.insert(name.name.clone(), counter);
        }

        self.lower_block(body)?;
        self.var_map = saved_vars;

        // Fall through to the increment block.
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.goto(incr_block);
            builder.switch_to_block(incr_block);
        }

        // Increment: counter = counter + step (default 1)
        let step_val = if let Some(step_expr) = step {
            self.lower_expr(step_expr)?
        } else {
            MirValue::Const(MirConst::Int(1, iter_ty))
        };
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.binary_op(counter, BinOp::Add, values::local(counter), step_val);
            builder.goto(cond_block);
        }

        self.loop_stack.pop();

        let builder = self.current_fn.as_mut().unwrap();
        builder.switch_to_block(exit_block);

        Ok(values::unit())
    }

    /// Lower a for-in loop using the iterator protocol.
    /// Desugars to:
    /// ```text
    /// let mut iter = <iter_val>;
    /// loop {
    ///   let __next = iter.next();
    ///   if __next.tag == 1 { break; } // None
    ///   let x = __next.data.Some.0;    // extract payload
    ///   <body>
    /// }
    /// ```
    fn lower_for_iterator(
        &mut self,
        pattern: &ast::Pattern,
        iter_val: MirValue,
        iter_ty: &MirType,
        body: &ast::Block,
        label: Option<&ast::Ident>,
    ) -> CodegenResult<MirValue> {
        let type_name = if let MirType::Struct(ref name) = iter_ty {
            name.clone()
        } else {
            return Ok(values::unit());
        };

        // Resolve the `next` method
        let next_fn_name = match self
            .impl_methods
            .get(&(type_name.clone(), Arc::from("next")))
        {
            Some(name) => name.clone(),
            None => return Ok(values::unit()),
        };

        // Get return type of next() - should be an enum (Option-like)
        let next_ret_ty = self
            .module
            .find_function(next_fn_name.as_ref())
            .map(|f| f.sig.ret.clone())
            .unwrap_or(MirType::i32());

        // Get the payload type from the enum's first variant (Some(T))
        let payload_ty = if let MirType::Struct(ref enum_name) = next_ret_ty {
            if let Some(type_def) = self.module.find_type(enum_name) {
                if let TypeDefKind::Enum { variants, .. } = &type_def.kind {
                    variants
                        .iter()
                        .find(|v| v.discriminant == 0)
                        .and_then(|v| v.fields.first())
                        .map(|(_, ty)| ty.clone())
                        .unwrap_or(MirType::i32())
                } else {
                    MirType::i32()
                }
            } else {
                MirType::i32()
            }
        } else {
            MirType::i32()
        };

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        // Store the iterator in a mutable local
        let iter_local = builder.create_local(iter_ty.clone());
        builder.assign(iter_local, MirRValue::Use(iter_val));

        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let exit_block = builder.create_block();

        self.loop_stack
            .push((cond_block, exit_block, label_name(label)));

        builder.goto(cond_block);
        builder.switch_to_block(cond_block);

        // Call iter.next()
        let next_result = builder.create_local(next_ret_ty.clone());
        let cont_after_next = builder.create_block();
        let func = MirValue::Function(next_fn_name);
        builder.call(
            func,
            vec![values::local(iter_local)],
            Some(next_result),
            cont_after_next,
        );
        builder.switch_to_block(cont_after_next);

        // Check tag: if tag == 1 (None), goto exit
        let tag_local = builder.create_local(MirType::i32());
        builder.assign(
            tag_local,
            MirRValue::FieldAccess {
                base: values::local(next_result),
                field_name: Arc::from("tag"),
                field_ty: MirType::i32(),
            },
        );
        let is_none = builder.create_local(MirType::Bool);
        builder.binary_op(
            is_none,
            BinOp::Eq,
            values::local(tag_local),
            MirValue::Const(MirConst::Int(1, MirType::i32())),
        );
        builder.branch(values::local(is_none), exit_block, body_block);

        // Body block: extract payload and bind to pattern
        builder.switch_to_block(body_block);

        // Get the variant name for discriminant 0 (Some)
        let some_variant_name = if let MirType::Struct(ref enum_name) = next_ret_ty {
            if let Some(type_def) = self.module.find_type(enum_name) {
                if let TypeDefKind::Enum { variants, .. } = &type_def.kind {
                    variants
                        .iter()
                        .find(|v| v.discriminant == 0)
                        .map(|v| v.name.clone())
                        .unwrap_or(Arc::from("Some"))
                } else {
                    Arc::from("Some")
                }
            } else {
                Arc::from("Some")
            }
        } else {
            Arc::from("Some")
        };

        let payload = builder.create_local(payload_ty.clone());
        builder.assign(
            payload,
            MirRValue::VariantField {
                base: values::local(next_result),
                variant_name: some_variant_name,
                field_index: 0,
                field_ty: payload_ty.clone(),
            },
        );

        // Bind pattern variable and save var_map so loop-body `let`
        // bindings do not leak into the enclosing scope.
        let saved_vars = self.var_map.clone();
        self.bind_for_pattern(pattern, payload, &payload_ty)?;

        self.lower_block(body)?;
        self.var_map = saved_vars;

        let builder = self.current_fn.as_mut().unwrap();
        builder.goto(cond_block);

        self.loop_stack.pop();

        let builder = self.current_fn.as_mut().unwrap();
        builder.switch_to_block(exit_block);

        Ok(values::unit())
    }

    /// Bind a for-loop pattern to a local value.  Handles Ident, Tuple,
    /// Ref, and Wildcard patterns so that `for (a, b) in ...` and
    /// `for &x in ...` declare all needed locals.
    fn bind_for_pattern(
        &mut self,
        pattern: &ast::Pattern,
        value_local: LocalId,
        value_ty: &MirType,
    ) -> CodegenResult<()> {
        match &pattern.kind {
            ast::PatternKind::Ident { name, .. } => {
                self.var_map.insert(name.name.clone(), value_local);
            }
            ast::PatternKind::Tuple(patterns) => {
                // Destructure tuple: create a local for each element and
                // extract via field access (_0, _1, ...).
                let elem_types: Vec<MirType> = if let MirType::Tuple(tys) = value_ty {
                    tys.clone()
                } else {
                    // If the type isn't a tuple, fall back to i32 for each element
                    vec![MirType::i32(); patterns.len()]
                };
                for (i, sub_pat) in patterns.iter().enumerate() {
                    let field_ty = elem_types.get(i).cloned().unwrap_or(MirType::i32());
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let field_local = builder.create_local(field_ty.clone());
                    builder.assign(
                        field_local,
                        MirRValue::FieldAccess {
                            base: values::local(value_local),
                            field_name: Arc::from(format!("_{}", i)),
                            field_ty: field_ty.clone(),
                        },
                    );
                    self.bind_for_pattern(sub_pat, field_local, &field_ty)?;
                }
            }
            ast::PatternKind::Ref { pattern: inner, .. } => {
                // &x pattern - bind the inner pattern to the same local
                // (we don't dereference in MIR for loop bindings)
                self.bind_for_pattern(inner, value_local, value_ty)?;
            }
            ast::PatternKind::Wildcard => {
                // _ - ignore the value
            }
            _ => {
                // Fallback: try to extract an ident name from the pattern
                // to avoid completely losing the binding.
            }
        }
        Ok(())
    }

    fn lower_return(&mut self, value: Option<&ast::Expr>) -> CodegenResult<MirValue> {
        // Lower value expression FIRST if present
        let ret_val = if let Some(expr) = value {
            Some(self.lower_expr(expr)?)
        } else {
            None
        };

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        if let Some(val) = ret_val {
            builder.ret(Some(val));
        } else {
            builder.ret_void();
        }

        // Create unreachable block for code after return
        let unreachable = builder.create_block();
        builder.switch_to_block(unreachable);

        Ok(values::unit())
    }

    /// Lower the `?` (try) operator.
    ///
    /// `expr?` desugars to:
    ///   match expr {
    ///       Ok(val)  | Some(val) => val,           // continue with unwrapped value
    ///       Err(e)              => return Err(e),  // early return with error
    ///       None                => return None,    // early return with None
    ///   }
    fn lower_try(&mut self, inner: &ast::Expr) -> CodegenResult<MirValue> {
        // 1. Lower the inner expression
        let inner_val = self.lower_expr(inner)?;
        let inner_ty = self.type_of_value(&inner_val);

        // Runtime Result/Option `?`: these are the runtime structs (is_ok /
        // has_value + a union slot), not user-defined tagged enums, so the
        // enum-tag path below does not apply. Unwrap the success payload as the
        // expression value, or early-return the whole value to propagate the
        // Err/None (the enclosing function returns the same runtime type, so the
        // discriminant and err/none payload carry through unchanged).
        if let MirType::Struct(ref name) = inner_ty {
            if name.as_ref() == "Result" {
                let ok_ty = self.result_ok_type_for(inner, &inner_val);
                return self.lower_try_runtime_sumtype(inner_val, &inner_ty, "is_ok", "ok", ok_ty);
            }
            if name.as_ref() == "Option" {
                let payload_ty = self
                    .option_inner_type_for(inner, &inner_val)
                    .unwrap_or_else(MirType::i32);
                return self.lower_try_runtime_sumtype(
                    inner_val,
                    &inner_ty,
                    "has_value",
                    "value",
                    payload_ty,
                );
            }
        }

        // The inner value must be an enum type (Result or Option)
        let enum_name = if let MirType::Struct(ref name) = inner_ty {
            name.clone()
        } else {
            // Not an enum -- just pass through
            return Ok(inner_val);
        };

        if !self.is_enum_type(&enum_name) {
            return Ok(inner_val);
        }

        // 2. Store the inner value in a local so we can read from it
        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function for try operator".to_string())
        })?;
        let scrutinee_local = builder.create_local(inner_ty.clone());
        builder.assign(scrutinee_local, MirRValue::Use(inner_val));

        // 3. Read the discriminant tag
        let tag_local = builder.create_local(MirType::i32());
        builder.assign(
            tag_local,
            MirRValue::FieldAccess {
                base: values::local(scrutinee_local),
                field_name: Arc::from("tag"),
                field_ty: MirType::i32(),
            },
        );

        // 4. Create basic blocks: ok_block, err_block, continue_block
        let ok_block = builder.create_block();
        let err_block = builder.create_block();
        let cont_block = builder.create_block();

        // Tag == 0 means the first variant (Ok / Some)
        let is_ok = builder.create_local(MirType::Bool);
        builder.binary_op(
            is_ok,
            BinOp::Eq,
            values::local(tag_local),
            MirValue::Const(MirConst::Int(0, MirType::i32())),
        );
        builder.branch(values::local(is_ok), ok_block, err_block);

        // 5. Ok/Some block -- extract the payload value and continue
        //    Look up variant 0 to determine the payload field type
        let variants = if let Some(type_def) = self.module.find_type(&enum_name) {
            if let TypeDefKind::Enum { variants, .. } = &type_def.kind {
                variants.clone()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let ok_variant = variants.iter().find(|v| v.discriminant == 0);
        let err_variant = variants.iter().find(|v| v.discriminant == 1);

        let ok_variant_name: Arc<str> = ok_variant
            .map(|v| v.name.clone())
            .unwrap_or(Arc::from("Ok"));

        let ok_field_ty = ok_variant
            .and_then(|v| v.fields.first())
            .map(|(_, ty)| ty.clone())
            .unwrap_or(MirType::i32());

        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(ok_block);

            // Extract the payload from variant field 0
            let unwrapped = builder.create_local(ok_field_ty.clone());
            builder.assign(
                unwrapped,
                MirRValue::VariantField {
                    base: values::local(scrutinee_local),
                    variant_name: ok_variant_name,
                    field_index: 0,
                    field_ty: ok_field_ty,
                },
            );
            builder.goto(cont_block);

            // 6. Err/None block -- construct the error variant and return early
            builder.switch_to_block(err_block);

            if let Some(err_v) = err_variant {
                if err_v.fields.is_empty() {
                    // None-like variant (no payload): return EnumName::VariantName
                    let err_result = builder.create_local(MirType::Struct(enum_name.clone()));
                    builder.aggregate(
                        err_result,
                        AggregateKind::Variant(
                            enum_name,
                            err_v.discriminant as u32,
                            err_v.name.clone(),
                        ),
                        vec![],
                    );
                    builder.ret(Some(values::local(err_result)));
                } else {
                    // Err-like variant (has payload): extract payload, reconstruct, return
                    let err_field_ty = err_v
                        .fields
                        .first()
                        .map(|(_, ty)| ty.clone())
                        .unwrap_or(MirType::i32());
                    let err_payload = builder.create_local(err_field_ty.clone());
                    builder.assign(
                        err_payload,
                        MirRValue::VariantField {
                            base: values::local(scrutinee_local),
                            variant_name: err_v.name.clone(),
                            field_index: 0,
                            field_ty: err_field_ty,
                        },
                    );
                    let err_result = builder.create_local(MirType::Struct(enum_name.clone()));
                    builder.aggregate(
                        err_result,
                        AggregateKind::Variant(
                            enum_name,
                            err_v.discriminant as u32,
                            err_v.name.clone(),
                        ),
                        vec![values::local(err_payload)],
                    );
                    builder.ret(Some(values::local(err_result)));
                }
            } else {
                // Fallback: just return the scrutinee as-is
                builder.ret(Some(values::local(scrutinee_local)));
            }

            // 7. Create unreachable block after the early return in err_block,
            //    then switch to the continuation block
            let _unreachable = builder.create_block();
            builder.switch_to_block(cont_block);

            // The result is the unwrapped value from the Ok block
            Ok(values::local(unwrapped))
        }
    }

    /// Lower `expr?` where `expr` is a runtime `Result`/`Option` struct. On the
    /// success discriminant the expression value is the unwrapped payload (read
    /// from the typed union slot, boxing-aware in the C backend); on the failure
    /// discriminant the whole value is returned early to propagate the Err/None.
    fn lower_try_runtime_sumtype(
        &mut self,
        inner_val: MirValue,
        inner_ty: &MirType,
        disc_field: &str,
        payload_field: &str,
        payload_ty: MirType,
    ) -> CodegenResult<MirValue> {
        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function for try operator".to_string())
        })?;

        let scrut = builder.create_local(inner_ty.clone());
        builder.assign(scrut, MirRValue::Use(inner_val));

        // Success discriminant (is_ok / has_value): true => unwrap, false =>
        // propagate.
        let disc = builder.create_local(MirType::Bool);
        builder.assign(
            disc,
            MirRValue::FieldAccess {
                base: values::local(scrut),
                field_name: Arc::from(disc_field),
                field_ty: MirType::Bool,
            },
        );

        let ok_block = builder.create_block();
        let fail_block = builder.create_block();
        let cont_block = builder.create_block();
        builder.branch(values::local(disc), ok_block, fail_block);

        // Success: read the payload from the typed slot.
        builder.switch_to_block(ok_block);
        let unwrapped = builder.create_local(payload_ty.clone());
        builder.assign(
            unwrapped,
            MirRValue::FieldAccess {
                base: values::local(scrut),
                field_name: Arc::from(payload_field),
                field_ty: payload_ty,
            },
        );
        builder.goto(cont_block);

        // Failure: return the whole value to propagate Err/None unchanged.
        builder.switch_to_block(fail_block);
        builder.ret(Some(values::local(scrut)));

        // An unreachable block separates the early return from the continuation.
        let _unreachable = builder.create_block();
        builder.switch_to_block(cont_block);
        Ok(values::local(unwrapped))
    }

    fn lower_break(
        &mut self,
        value: Option<&ast::Expr>,
        label: Option<&ast::Ident>,
    ) -> CodegenResult<MirValue> {
        // Break value (e.g. `break 42`) is evaluated but currently not
        // assigned to a loop result local because loops do not yet propagate
        // a result variable.  The value is lowered for side-effect correctness.
        //
        // `break 'name` targets the nearest enclosing loop whose label matches;
        // a bare `break` targets the innermost loop. An unresolved label is a
        // fail-closed error rather than a silent fall-through to the innermost
        // loop, which would compile the wrong control flow.
        let target = self.resolve_loop_target(label, "break")?;

        if let Some((_, exit_block)) = target {
            if let Some(expr) = value {
                let _val = self.lower_expr(expr)?;
            }
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            builder.goto(exit_block);
        }

        let builder = self.current_fn.as_mut().unwrap();
        let unreachable = builder.create_block();
        builder.switch_to_block(unreachable);

        Ok(values::unit())
    }

    fn lower_continue(&mut self, label: Option<&ast::Ident>) -> CodegenResult<MirValue> {
        // `continue 'name` re-enters the nearest enclosing loop whose label
        // matches; a bare `continue` re-enters the innermost loop. An unresolved
        // label fails closed (see `lower_break`).
        let target = self.resolve_loop_target(label, "continue")?;

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        if let Some((continue_block, _)) = target {
            builder.goto(continue_block);
        }

        let unreachable = builder.create_block();
        builder.switch_to_block(unreachable);

        Ok(values::unit())
    }

    /// Resolve the `(continue_block, exit_block)` target of a `break`/`continue`.
    ///
    /// With a label, the nearest enclosing loop carrying that label wins, and an
    /// unresolved label is an error (fail closed) rather than a silent fall
    /// through to the innermost loop. Without a label, the innermost loop wins;
    /// `None` means the statement is outside any loop, which the earlier
    /// type-check pass already reports, so lowering leaves it as a no-op here.
    fn resolve_loop_target(
        &self,
        label: Option<&ast::Ident>,
        kw: &str,
    ) -> CodegenResult<Option<(BlockId, BlockId)>> {
        match label {
            Some(l) => {
                let name = l.as_str();
                match self
                    .loop_stack
                    .iter()
                    .rev()
                    .find(|(_, _, lbl)| lbl.as_deref() == Some(name))
                {
                    Some((cont, exit, _)) => Ok(Some((*cont, *exit))),
                    None => Err(CodegenError::Internal(format!(
                        "{kw} to undefined loop label '{name}'"
                    ))),
                }
            }
            None => Ok(self.loop_stack.last().map(|(cont, exit, _)| (*cont, *exit))),
        }
    }

    fn lower_tuple(&mut self, elems: &[ast::Expr]) -> CodegenResult<MirValue> {
        let elem_vals: Vec<_> = elems
            .iter()
            .map(|e| self.lower_expr(e))
            .collect::<CodegenResult<_>>()?;

        if elem_vals.is_empty() {
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let result = builder.create_local(MirType::Void);
            builder.aggregate(result, AggregateKind::Tuple, elem_vals);
            return Ok(values::local(result));
        }

        // Build the proper MirType::Tuple from element types.
        let elem_tys: Vec<MirType> = elem_vals.iter().map(|v| self.type_of_value(v)).collect();
        let tuple_ty = MirType::Tuple(elem_tys.clone());

        // Register the tuple type definition (struct typedef) if not already done.
        self.ensure_tuple_type_def(&elem_tys);

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let result = builder.create_local(tuple_ty);
        builder.aggregate(result, AggregateKind::Tuple, elem_vals);

        Ok(values::local(result))
    }

    /// Register a struct typedef for a tuple type if not already registered.
    pub(crate) fn ensure_tuple_type_def(&mut self, elem_tys: &[MirType]) {
        let name = MirType::tuple_type_name(elem_tys);
        if !self.tuple_type_defs.contains(&name) {
            self.tuple_type_defs.insert(name.clone());
            let fields: Vec<(Option<Arc<str>>, MirType)> = elem_tys
                .iter()
                .enumerate()
                .map(|(i, ty)| (Some(Arc::from(format!("_{}", i))), ty.clone()))
                .collect();
            self.module.add_type(MirTypeDef {
                name,
                kind: TypeDefKind::Struct {
                    fields,
                    packed: false,
                },
            });
        }
    }

    /// Lower `[expr; count]` - array repeat expression.
    fn lower_array_repeat(
        &mut self,
        element: &ast::Expr,
        count: &ast::Expr,
    ) -> CodegenResult<MirValue> {
        // Evaluate the count as a constant
        let count_val = self
            .try_const_eval(count)
            .and_then(|c| match c {
                MirConst::Int(v, _) => Some(v as u64),
                MirConst::Uint(v, _) => Some(v as u64),
                _ => None,
            })
            .unwrap_or(4); // Default to 4 if we can't evaluate

        // Lower the element expression once to get its type
        let elem_val = self.lower_expr(element)?;
        let elem_ty = self.type_of_value(&elem_val);

        // Create N copies of the element
        let mut elem_vals = Vec::with_capacity(count_val as usize);
        elem_vals.push(elem_val);
        for _ in 1..count_val {
            elem_vals.push(self.lower_expr(element)?);
        }

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let result = builder.create_local(MirType::Array(Box::new(elem_ty.clone()), count_val));
        builder.aggregate(result, AggregateKind::Array(elem_ty), elem_vals);

        Ok(values::local(result))
    }

    fn lower_array(&mut self, elems: &[ast::Expr]) -> CodegenResult<MirValue> {
        let elem_vals: Vec<_> = elems
            .iter()
            .map(|e| self.lower_expr(e))
            .collect::<CodegenResult<_>>()?;

        // Infer element type from the first element; fall back to i32 for
        // empty array literals.
        let elem_ty = elem_vals
            .first()
            .map(|v| self.type_of_value(v))
            .unwrap_or(MirType::i32());

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let result = builder.create_local(MirType::Array(
            Box::new(elem_ty.clone()),
            elems.len() as u64,
        ));
        builder.aggregate(result, AggregateKind::Array(elem_ty), elem_vals);

        Ok(values::local(result))
    }

    /// Workgroup-shared array size, fixed to the compute workgroup X size (64)
    /// in Phase 4a. Kept in one place so the frontend annotation, the SPIR-V
    /// `OpTypeArray` length, and the launch geometry cannot drift apart.
    const WORKGROUP_ARRAY_LEN: u64 = 64;

    /// Lower `workgroupArray(N)`: a workgroup-shared scratch array of `N` f32
    /// elements. Phase 4a requires `N` to be the compile-time constant 64 (the
    /// workgroup size); the result is a fresh local of type `Array(f32, 64)`
    /// carrying a `Workgroup:64` annotation the SPIR-V backend uses to place it
    /// in the `Workgroup` storage class. Indexing this local (`scratch[lid]`)
    /// then flows through the backend's workgroup-element access-chain path
    /// rather than the SSBO path.
    fn lower_workgroup_array(&mut self, len_arg: &ast::Expr) -> CodegenResult<MirValue> {
        // The length must be the literal workgroup size (64). Accept only an
        // integer literal so the emitted array length is a compile-time constant
        // that matches the launch geometry -- a runtime length would need a
        // runtime-sized workgroup array, which SPIR-V does not allow.
        let len = match &len_arg.kind {
            ExprKind::Literal(Literal::Int { value, .. }) => *value as u64,
            _ => {
                return Err(CodegenError::Internal(
                    "workgroupArray(N) requires a constant length (Phase 4a fixes it to the \
                     workgroup size, 64)"
                        .to_string(),
                ))
            }
        };
        if len != Self::WORKGROUP_ARRAY_LEN {
            return Err(CodegenError::Internal(format!(
                "workgroupArray length must be {} (the workgroup size) in Phase 4a, got {}",
                Self::WORKGROUP_ARRAY_LEN,
                len
            )));
        }
        let elem_ty = MirType::f32();
        let arr_ty = MirType::Array(Box::new(elem_ty), len);
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let slot = builder.create_local(arr_ty);
        builder.annotate_local(slot, Arc::from(format!("Workgroup:{}", len)));
        Ok(values::local(slot))
    }

    fn lower_index(&mut self, arr: &ast::Expr, index: &ast::Expr) -> CodegenResult<MirValue> {
        let arr_val = self.lower_expr(arr)?;
        let idx_val = self.lower_expr(index)?;

        // Derive element type: if the array value has type Array(elem, _)
        // then the result is of type elem; otherwise fall back to i32.
        // A slice/array *parameter* lowers to `Ptr(Slice(elem))` /
        // `Ptr(Array(elem, _))` (references pass as pointers), so unwrap the
        // pointer THEN the collection to reach the true element type. Without
        // the double-unwrap, indexing `&[f32]` yielded `Slice<f32>` and the
        // backend mis-typed the load.
        let elem_ty = match self.type_of_value(&arr_val) {
            MirType::Array(elem, _) => *elem,
            MirType::Slice(elem) => *elem,
            MirType::Vec(elem) => *elem,
            MirType::Ptr(inner) => match *inner {
                MirType::Slice(elem) => *elem,
                MirType::Array(elem, _) => *elem,
                other => other,
            },
            _ => MirType::i32(),
        };

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        let result = builder.create_local(elem_ty.clone());
        builder.assign(
            result,
            MirRValue::IndexAccess {
                base: arr_val,
                index: idx_val,
                elem_ty,
            },
        );

        Ok(values::local(result))
    }

    /// Map a swizzle character to its component index.
    /// Supports GLSL-style swizzle sets: xyzw, rgba, stpq.
    fn swizzle_char_to_index(c: char) -> Option<u32> {
        match c {
            'x' | 'r' | 's' => Some(0),
            'y' | 'g' | 't' => Some(1),
            'z' | 'b' | 'p' => Some(2),
            'w' | 'a' | 'q' => Some(3),
            _ => None,
        }
    }

    /// Check if a field name is a valid multi-character swizzle pattern
    /// on a vector type with `max_components` components.
    fn is_swizzle_pattern(field_name: &str, max_components: u32) -> bool {
        if field_name.len() < 2 || field_name.len() > 4 {
            return false;
        }
        field_name.chars().all(|c| {
            if let Some(idx) = Self::swizzle_char_to_index(c) {
                idx < max_components
            } else {
                false
            }
        })
    }

    fn lower_field(&mut self, obj: &ast::Expr, field: &ast::Ident) -> CodegenResult<MirValue> {
        // GPU compute built-in thread index: `gl_GlobalInvocationID.x/.y/.z`.
        // Intercept before lowering `obj` (which is not a real local/global) and
        // emit a `ThreadIndex` nullary op the backends recognize. The result is a
        // u32 index (coerced to i32 for array indexing by the surrounding code).
        if let ExprKind::Ident(base_ident) = &obj.kind {
            // Map the recognized GPU compute built-in index variables to their
            // nullary op. `gl_GlobalInvocationID` -> global thread index;
            // `gl_LocalInvocationID` -> lane within the workgroup;
            // `gl_WorkgroupID` (also accept the GLSL `gl_WorkGroupID` spelling)
            // -> which workgroup. Only intercept when the name is NOT a real
            // in-scope local (a user could shadow it).
            let base = base_ident.name.as_ref();
            let builtin_ctor: Option<fn(u32) -> NullaryOp> = match base {
                "gl_GlobalInvocationID" => Some(NullaryOp::ThreadIndex),
                "gl_LocalInvocationID" => Some(NullaryOp::LocalInvocationId),
                "gl_WorkgroupID" | "gl_WorkGroupID" => Some(NullaryOp::WorkgroupId),
                _ => None,
            };
            if let Some(ctor) = builtin_ctor {
                if !self.var_map.contains_key(&base_ident.name) {
                    let component = match field.name.as_ref() {
                        "x" => 0u32,
                        "y" => 1,
                        "z" => 2,
                        other => {
                            return Err(CodegenError::Internal(format!(
                                "{} has no component `.{}`",
                                base, other
                            )))
                        }
                    };
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(MirType::u32());
                    builder.assign(
                        result,
                        MirRValue::NullaryOp(ctor(component), MirType::u32()),
                    );
                    return Ok(values::local(result));
                }
            }
        }

        let obj_val = self.lower_expr(obj)?;

        // Determine the struct type of the object so we can look up the field.
        let obj_ty = self.type_of_value(&obj_val);
        let field_name = field.name.clone();

        // Auto-deref: if the base is a pointer to a struct, look up the field
        // through the pointee type. The C backend will emit `->` for this case.
        let (effective_ty, is_ptr_deref) = match &obj_ty {
            MirType::Ptr(inner) => (inner.as_ref().clone(), true),
            other => (other.clone(), false),
        };
        let _ = is_ptr_deref; // used implicitly by FieldAccess on pointer-typed base

        // Swizzle support: multi-character field access on vector types
        // e.g. color.xyz → build_vec3_new(color.x, color.y, color.z)
        if let MirType::Struct(ref struct_name) = effective_ty {
            if let Some(max_comp) = Self::vec_component_count(struct_name) {
                if Self::is_swizzle_pattern(&field_name, max_comp) {
                    let swizzle_len = field_name.len() as u32;
                    let result_type_name = format!("build_vec{}", swizzle_len);
                    let result_ty = MirType::Struct(Arc::from(result_type_name.as_str()));
                    let component_names: Vec<&str> = vec!["x", "y", "z", "w"];

                    // Build FieldAccess for each swizzle component
                    let mut component_vals = Vec::new();
                    for c in field_name.chars() {
                        let idx = Self::swizzle_char_to_index(c).unwrap();
                        let comp_name = component_names[idx as usize];

                        let builder = self.current_fn.as_mut().ok_or_else(|| {
                            CodegenError::Internal("No current function".to_string())
                        })?;
                        let comp_local = builder.create_local(MirType::f64());
                        builder.assign(
                            comp_local,
                            MirRValue::FieldAccess {
                                base: obj_val.clone(),
                                field_name: Arc::from(comp_name),
                                field_ty: MirType::f64(),
                            },
                        );
                        component_vals.push(values::local(comp_local));
                    }

                    // Build constructor call: build_vecN_new(...)
                    let constructor = format!("build_vec{}_new", swizzle_len);
                    let builder = self
                        .current_fn
                        .as_mut()
                        .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                    let result = builder.create_local(result_ty);
                    let cont = builder.create_block();
                    let func = MirValue::Function(Arc::from(constructor.as_str()));
                    builder.call(func, component_vals, Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }
            }
        }

        // Look up the struct definition to find the field type.
        let field_ty = if let MirType::Struct(ref struct_name) = effective_ty {
            self.lookup_struct_field_type(struct_name, &field_name)
                .unwrap_or(MirType::i32())
        } else {
            MirType::i32()
        };

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        let result = builder.create_local(field_ty.clone());
        builder.assign(
            result,
            MirRValue::FieldAccess {
                base: obj_val,
                field_name,
                field_ty,
            },
        );

        Ok(values::local(result))
    }

    fn lower_tuple_field(&mut self, inner: &ast::Expr, index: u32) -> CodegenResult<MirValue> {
        let base_val = self.lower_expr(inner)?;
        let base_ty = self.type_of_value(&base_val);

        let elem_ty = if let MirType::Tuple(ref elems) = base_ty {
            elems.get(index as usize).cloned().unwrap_or(MirType::i32())
        } else {
            MirType::i32()
        };

        let field_name: Arc<str> = Arc::from(format!("_{}", index));
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let result = builder.create_local(elem_ty.clone());
        builder.assign(
            result,
            MirRValue::FieldAccess {
                base: base_val,
                field_name,
                field_ty: elem_ty,
            },
        );

        Ok(values::local(result))
    }

    fn lower_ref(
        &mut self,
        mutability: ast::Mutability,
        inner: &ast::Expr,
    ) -> CodegenResult<MirValue> {
        let inner_val = self.lower_expr(inner)?;
        let inner_ty = self.type_of_value(&inner_val);

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        // Get the local from inner value
        let local = match &inner_val {
            MirValue::Local(id) => *id,
            _ => {
                // Create a temporary with the correct type
                let temp = builder.create_local(inner_ty.clone());
                builder.assign(temp, MirRValue::Use(inner_val));
                temp
            }
        };

        let result = builder.create_local(MirType::Ptr(Box::new(inner_ty)));
        builder.make_ref(result, mutability.is_mut(), MirPlace::local(local));

        Ok(values::local(result))
    }

    fn lower_deref(&mut self, inner: &ast::Expr) -> CodegenResult<MirValue> {
        let inner_val = self.lower_expr(inner)?;

        // Derive the pointee type from the pointer's type.
        let pointee_ty = match self.type_of_value(&inner_val) {
            MirType::Ptr(inner) => *inner,
            _ => MirType::i32(), // Fallback for non-pointer derefs
        };

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        // Emit a Deref rvalue that reads through the pointer.
        let result = builder.create_local(pointee_ty.clone());
        builder.assign(
            result,
            MirRValue::Deref {
                ptr: inner_val,
                pointee_ty,
            },
        );

        Ok(values::local(result))
    }

    fn lower_cast(&mut self, inner: &ast::Expr, ty: &ast::Type) -> CodegenResult<MirValue> {
        let inner_val = self.lower_expr(inner)?;
        let target_ty = self.lower_type_from_ast(ty);
        let source_ty = self.type_of_value(&inner_val);

        // Special case: casting to a trait object (dyn Trait)
        // Creates a fat pointer: { data: &value, vtable: &TypeName_TraitName_vtable_instance }
        if let MirType::TraitObject(ref trait_name) = target_ty {
            // Get the concrete type name from the source
            let type_name = match &source_ty {
                MirType::Struct(name) => name.clone(),
                MirType::Ptr(inner) => {
                    if let MirType::Struct(name) = inner.as_ref() {
                        name.clone()
                    } else {
                        Arc::from("Unknown")
                    }
                }
                _ => Arc::from("Unknown"),
            };

            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

            // Create the fat pointer struct
            let result = builder.create_local(target_ty.clone());

            // The data pointer: take address of the value
            let data_local = match &inner_val {
                MirValue::Local(id) => *id,
                _ => {
                    let temp = builder.create_local(source_ty.clone());
                    builder.assign(temp, MirRValue::Use(inner_val));
                    temp
                }
            };

            // Create the trait object aggregate with data pointer and vtable name
            // The C backend will generate:
            //   (dyn_Shape){ .data = &value, .vtable = &TypeName_TraitName_vtable_instance }
            let data_ptr = builder.create_local(MirType::Ptr(Box::new(MirType::Void)));
            builder.make_ref(data_ptr, false, MirPlace::local(data_local));

            // Store vtable reference - the C backend generates &vtable_instance
            let vtable_name: Arc<str> =
                Arc::from(format!("{}_{}_vtable_instance", type_name, trait_name));
            let vtable_struct_ty = MirType::Struct(Arc::from(format!("{}_vtable", trait_name)));
            let vtable_ptr = builder.create_local(vtable_struct_ty);
            builder.assign(vtable_ptr, MirRValue::Use(MirValue::Global(vtable_name)));

            // Construct the fat pointer aggregate
            builder.assign(
                result,
                MirRValue::Aggregate {
                    kind: AggregateKind::Struct(Arc::from(format!("dyn_{}", trait_name))),
                    operands: vec![values::local(data_ptr), values::local(vtable_ptr)],
                },
            );

            return Ok(values::local(result));
        }

        // Regular cast
        let cast_kind = match (&source_ty, &target_ty) {
            (MirType::Int(..), MirType::Int(..)) => CastKind::IntToInt,
            (MirType::Int(..), MirType::Float(..)) => CastKind::IntToFloat,
            (MirType::Float(..), MirType::Int(..)) => CastKind::FloatToInt,
            (MirType::Float(..), MirType::Float(..)) => CastKind::FloatToFloat,
            (MirType::Ptr(_), MirType::Ptr(_)) => CastKind::PtrToPtr,
            (MirType::Ptr(_), MirType::Int(..)) => CastKind::PtrToInt,
            (MirType::Int(..), MirType::Ptr(_)) => CastKind::IntToPtr,
            _ => CastKind::Transmute,
        };

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        let result = builder.create_local(target_ty.clone());
        builder.cast(result, cast_kind, inner_val, target_ty);

        Ok(values::local(result))
    }
}
