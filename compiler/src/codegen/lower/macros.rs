// ===============================================================================
// BUILDLANG CODE GENERATOR - MACRO AND CLOSURE LOWERING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! Closure lowering, effect lowering, builtin macro expansion, and iterator
//! chain desugaring for MIR.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::ast::{self, ExprKind, StmtKind};

use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::{values, MirBuilder};
use crate::codegen::ir::*;

use super::{IterChain, IterStep, IterTerminal, MirLowerer};

impl<'ctx> MirLowerer<'ctx> {
    // =========================================================================
    // CLOSURE LOWERING
    // =========================================================================

    /// Collect free variables in an expression that are defined in the
    /// enclosing scope (i.e., present in `env_vars`) but are NOT among
    /// the closure's own `param_names`.  Returns a de-duplicated list of
    /// (variable_name, local_id_in_enclosing_scope).
    fn collect_free_vars(
        expr: &ast::Expr,
        param_names: &HashSet<Arc<str>>,
        env_vars: &HashMap<Arc<str>, LocalId>,
        source: Option<&str>,
    ) -> Vec<(Arc<str>, LocalId)> {
        let mut found: Vec<(Arc<str>, LocalId)> = Vec::new();
        let mut seen: HashSet<Arc<str>> = HashSet::new();
        Self::collect_free_vars_inner(expr, param_names, env_vars, &mut found, &mut seen, source);
        found
    }

    fn collect_free_vars_inner(
        expr: &ast::Expr,
        param_names: &HashSet<Arc<str>>,
        env_vars: &HashMap<Arc<str>, LocalId>,
        found: &mut Vec<(Arc<str>, LocalId)>,
        seen: &mut HashSet<Arc<str>>,
        source: Option<&str>,
    ) {
        match &expr.kind {
            ExprKind::Ident(ident) => {
                if !param_names.contains(&ident.name) && !seen.contains(&ident.name) {
                    if let Some(&local_id) = env_vars.get(&ident.name) {
                        seen.insert(ident.name.clone());
                        found.push((ident.name.clone(), local_id));
                    }
                }
            }
            ExprKind::Binary { left, right, .. } => {
                Self::collect_free_vars_inner(left, param_names, env_vars, found, seen, source);
                Self::collect_free_vars_inner(right, param_names, env_vars, found, seen, source);
            }
            ExprKind::Unary { expr: inner, .. }
            | ExprKind::Paren(inner)
            | ExprKind::Ref { expr: inner, .. }
            | ExprKind::Deref(inner)
            | ExprKind::Return(Some(inner))
            | ExprKind::Try(inner)
            | ExprKind::Await(inner)
            | ExprKind::Cast { expr: inner, .. } => {
                Self::collect_free_vars_inner(inner, param_names, env_vars, found, seen, source);
            }
            ExprKind::Call { func, args } => {
                Self::collect_free_vars_inner(func, param_names, env_vars, found, seen, source);
                for a in args {
                    Self::collect_free_vars_inner(a, param_names, env_vars, found, seen, source);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                Self::collect_free_vars_inner(receiver, param_names, env_vars, found, seen, source);
                for a in args {
                    Self::collect_free_vars_inner(a, param_names, env_vars, found, seen, source);
                }
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                Self::collect_free_vars_inner(
                    condition,
                    param_names,
                    env_vars,
                    found,
                    seen,
                    source,
                );
                for stmt in &then_branch.stmts {
                    if let StmtKind::Expr(e) | StmtKind::Semi(e) = &stmt.kind {
                        Self::collect_free_vars_inner(
                            e,
                            param_names,
                            env_vars,
                            found,
                            seen,
                            source,
                        );
                    }
                }
                if let Some(e) = else_branch {
                    Self::collect_free_vars_inner(e, param_names, env_vars, found, seen, source);
                }
            }
            ExprKind::Block(block) | ExprKind::Unsafe(block) => {
                for stmt in &block.stmts {
                    if let StmtKind::Expr(e) | StmtKind::Semi(e) = &stmt.kind {
                        Self::collect_free_vars_inner(
                            e,
                            param_names,
                            env_vars,
                            found,
                            seen,
                            source,
                        );
                    }
                }
            }
            ExprKind::Tuple(elems) | ExprKind::Array(elems) => {
                for e in elems {
                    Self::collect_free_vars_inner(e, param_names, env_vars, found, seen, source);
                }
            }
            ExprKind::Index { expr: arr, index } => {
                Self::collect_free_vars_inner(arr, param_names, env_vars, found, seen, source);
                Self::collect_free_vars_inner(index, param_names, env_vars, found, seen, source);
            }
            ExprKind::Field { expr: obj, .. } => {
                Self::collect_free_vars_inner(obj, param_names, env_vars, found, seen, source);
            }
            ExprKind::Assign { target, value, .. } => {
                Self::collect_free_vars_inner(target, param_names, env_vars, found, seen, source);
                Self::collect_free_vars_inner(value, param_names, env_vars, found, seen, source);
            }
            ExprKind::Match { scrutinee, arms } => {
                Self::collect_free_vars_inner(
                    scrutinee,
                    param_names,
                    env_vars,
                    found,
                    seen,
                    source,
                );
                for arm in arms {
                    Self::collect_free_vars_inner(
                        &arm.body,
                        param_names,
                        env_vars,
                        found,
                        seen,
                        source,
                    );
                    if let Some(guard) = &arm.guard {
                        Self::collect_free_vars_inner(
                            guard,
                            param_names,
                            env_vars,
                            found,
                            seen,
                            source,
                        );
                    }
                }
            }
            ExprKind::Macro { tokens, .. } => {
                // Scan token trees for identifiers
                for tt in tokens {
                    Self::collect_free_vars_in_token_tree(
                        tt,
                        param_names,
                        env_vars,
                        found,
                        seen,
                        source,
                    );
                }
            }
            _ => {}
        }
    }

    fn collect_free_vars_in_token_tree(
        tt: &ast::TokenTree,
        param_names: &HashSet<Arc<str>>,
        env_vars: &HashMap<Arc<str>, LocalId>,
        found: &mut Vec<(Arc<str>, LocalId)>,
        seen: &mut HashSet<Arc<str>>,
        source: Option<&str>,
    ) {
        match tt {
            ast::TokenTree::Token(token) => {
                // Check if this token is an identifier that references an
                // enclosing variable.  We need the source text to recover the
                // identifier string because Token stores only a span.
                if let crate::lexer::TokenKind::Ident = &token.kind {
                    if let Some(src) = source {
                        let start = token.span.start.to_usize();
                        let end = token.span.end.to_usize();
                        if end <= src.len() {
                            let name: Arc<str> = Arc::from(&src[start..end]);
                            if !param_names.contains(&name) && !seen.contains(&name) {
                                if let Some(&local_id) = env_vars.get(&name) {
                                    seen.insert(name.clone());
                                    found.push((name, local_id));
                                }
                            }
                        }
                    }
                }
            }
            ast::TokenTree::Delimited { tokens, .. } => {
                for inner in tokens {
                    Self::collect_free_vars_in_token_tree(
                        inner,
                        param_names,
                        env_vars,
                        found,
                        seen,
                        source,
                    );
                }
            }
        }
    }

    /// Lower a closure expression into a static function + function pointer.
    ///
    /// **Capturing closures (lambda lifting)**: if the closure body references
    /// variables from the enclosing scope, those variables are added as extra
    /// trailing parameters to the generated `__closure_N` function.  At the
    /// call site the captured values are automatically appended.
    ///
    /// NOTE: this approach works for closures that are called locally or passed
    /// to functions that invoke them in the same compilation unit.  Returning a
    /// capturing closure from a function is not yet supported because the
    /// function-pointer signature would differ from the declared type.
    pub(crate) fn lower_closure(
        &mut self,
        params: &[ast::ClosureParam],
        return_type: Option<&ast::Type>,
        body: &ast::Expr,
    ) -> CodegenResult<MirValue> {
        let closure_id = self.closure_count;
        self.closure_count += 1;
        let closure_name: Arc<str> = Arc::from(format!("__closure_{}", closure_id));

        // ---- Detect captured variables (lambda lifting) ----
        let param_names: HashSet<Arc<str>> = params
            .iter()
            .filter_map(|p| {
                if let ast::PatternKind::Ident { name, .. } = &p.pattern.kind {
                    Some(name.name.clone())
                } else {
                    None
                }
            })
            .collect();

        let captures =
            Self::collect_free_vars(body, &param_names, &self.var_map, self.source.as_deref());

        // ---- Build the MIR signature ----
        // Declared params first, then captured-variable params appended.
        let mut mir_params: Vec<MirType> = params
            .iter()
            .map(|p| {
                p.ty.as_ref()
                    .map(|t| self.lower_type_from_ast(t))
                    .unwrap_or(MirType::i32())
            })
            .collect();

        // Resolve captured variable types from the enclosing builder.
        let capture_types: Vec<MirType> = captures
            .iter()
            .map(|(_name, local_id)| {
                if let Some(ref builder) = self.current_fn {
                    builder.local_type(*local_id).unwrap_or(MirType::i32())
                } else {
                    MirType::i32()
                }
            })
            .collect();

        mir_params.extend(capture_types.iter().cloned());

        let mir_ret = return_type
            .map(|t| self.lower_type_from_ast(t))
            .unwrap_or(MirType::i32());

        let sig = MirFnSig::new(mir_params.clone(), mir_ret.clone());
        // The fn-ptr type must use the *full* parameter list (visible +
        // captured) so that the C declaration matches the call sites, which
        // append captured values as extra arguments.
        let fn_ptr_ty =
            MirType::FnPtr(Box::new(MirFnSig::new(mir_params.clone(), mir_ret.clone())));

        // Save current function state
        let saved_fn = self.current_fn.take();
        let saved_vars = std::mem::take(&mut self.var_map);

        let mut closure_builder = MirBuilder::with_linear_type_names(
            closure_name.clone(),
            sig,
            Arc::new(self.linear_type_names.clone()),
        );

        // Map declared params
        for (i, param) in params.iter().enumerate() {
            if let ast::PatternKind::Ident { name, .. } = &param.pattern.kind {
                let local_id = closure_builder.param_local(i);
                closure_builder.set_param_name(i, name.name.clone());
                self.var_map.insert(name.name.clone(), local_id);
            }
        }

        // Map captured-variable params (appended after declared params)
        for (ci, (cap_name, _)) in captures.iter().enumerate() {
            let param_idx = params.len() + ci;
            let local_id = closure_builder.param_local(param_idx);
            closure_builder.set_param_name(param_idx, cap_name.clone());
            self.var_map.insert(cap_name.clone(), local_id);
        }

        self.current_fn = Some(closure_builder);

        let body_val = self.lower_expr(body)?;

        let mut closure_builder = self.current_fn.take().unwrap();
        if mir_ret != MirType::Void {
            closure_builder.ret(Some(body_val));
        } else {
            closure_builder.ret_void();
        }

        let mut closure_func = closure_builder.build();
        closure_func.linkage = Linkage::Internal;
        closure_func.is_public = false;

        self.module.add_function(closure_func);

        // Restore the enclosing function state
        self.current_fn = saved_fn;
        self.var_map = saved_vars;

        // Register captures so that lower_call can append extra args.
        if !captures.is_empty() {
            self.closure_captures.insert(closure_name.clone(), captures);
        }

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function for closure".to_string()))?;
        let result = builder.create_local(fn_ptr_ty);
        builder.assign(
            result,
            MirRValue::Use(MirValue::Function(closure_name.clone())),
        );

        // Track that this local holds the given closure so we can find
        // its captures later when calling through this local.
        self.local_closure_name.insert(result, closure_name);

        Ok(values::local(result))
    }

    pub(crate) fn lower_struct_expr(
        &mut self,
        path: &ast::Path,
        fields: &[ast::FieldExpr],
        _rest: Option<&ast::Expr>,
    ) -> CodegenResult<MirValue> {
        // Lower all field values FIRST before borrowing the builder.
        let field_vals: Vec<_> = fields
            .iter()
            .map(|f| {
                if let Some(val) = &f.value {
                    self.lower_expr(val)
                } else {
                    // Field shorthand: `name` means `name: name`
                    self.lower_ident(&f.name)
                }
            })
            .collect::<CodegenResult<_>>()?;

        let mut raw_name = path
            .last_ident()
            .map(|i| i.name.clone())
            .unwrap_or(Arc::from(""));

        // Resolve Self to concrete type name
        if raw_name.as_ref() == "Self" {
            if let Some(ref impl_ty) = self.current_impl_type {
                raw_name = impl_ty.clone();
            }
        }

        // Inside inline modules, try the prefixed struct name first so that
        // types defined in the current module are resolved correctly.
        if !self.module_prefix.is_empty() {
            let prefixed = self.prefixed_name(&raw_name);
            if self.module.find_type(prefixed.as_ref()).is_some() {
                raw_name = prefixed;
            }
        }

        // Check if this is a generic struct that needs monomorphization
        let struct_name = if self.generic_structs.contains_key(raw_name.as_ref()) {
            // Try to resolve from explicit generic args on the path
            let generic_args = path.last_generics().unwrap_or(&[]);
            if !generic_args.is_empty() {
                let empty_subst = HashMap::new();
                let subst = self.resolve_generic_args_with_subst(
                    raw_name.as_ref(),
                    generic_args,
                    &empty_subst,
                );
                self.monomorphize_struct(raw_name.as_ref(), &subst)?
            } else {
                // Infer generic params from field values
                let subst =
                    self.infer_struct_generics_from_fields(raw_name.as_ref(), &field_vals, fields);
                self.monomorphize_struct(raw_name.as_ref(), &subst)?
            }
        } else {
            raw_name
        };

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let result = builder.create_local(MirType::Struct(struct_name.clone()));
        builder.aggregate(result, AggregateKind::Struct(struct_name), field_vals);

        Ok(values::local(result))
    }

    /// Infer generic type parameters for a struct from the types of field values
    /// at the construction site.
    fn infer_struct_generics_from_fields(
        &self,
        struct_name: &str,
        field_vals: &[MirValue],
        field_exprs: &[ast::FieldExpr],
    ) -> HashMap<Arc<str>, MirType> {
        let mut subst = HashMap::new();

        let struct_def = match self.generic_structs.get(struct_name) {
            Some(s) => s.clone(),
            None => return subst,
        };

        // Get type parameter names
        let type_param_names: Vec<Arc<str>> = struct_def
            .generics
            .params
            .iter()
            .filter_map(|p| match &p.kind {
                ast::GenericParamKind::Type { .. } => Some(p.ident.name.clone()),
                _ => None,
            })
            .collect();

        // Match field types against field values
        if let ast::StructFields::Named(def_fields) = &struct_def.fields {
            for (i, def_field) in def_fields.iter().enumerate() {
                if let Some(val) = field_vals.get(i) {
                    let val_ty = self.type_of_value(val);
                    // Check if this field's type is a generic parameter
                    if let ast::TypeKind::Path(path) = &def_field.ty.kind {
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

        // Also try matching by field name from field_exprs
        if let ast::StructFields::Named(def_fields) = &struct_def.fields {
            for field_expr in field_exprs {
                let field_name = &field_expr.name.name;
                if let Some(def_field) = def_fields.iter().find(|f| &f.name.name == field_name) {
                    if let ast::TypeKind::Path(path) = &def_field.ty.kind {
                        if path.is_simple() {
                            if let Some(ident) = path.last_ident() {
                                for tp_name in &type_param_names {
                                    if ident.name.as_ref() == tp_name.as_ref() {
                                        // Already handled above via positional matching
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fill in any unbound params with i32 default
        for tp_name in &type_param_names {
            subst.entry(tp_name.clone()).or_insert(MirType::i32());
        }

        subst
    }

    // =========================================================================
    // EFFECT LOWERING
    // =========================================================================

    /// Compute a deterministic integer ID for a named effect.
    ///
    /// Built-in effects get fixed IDs; user-defined effects are hashed into a
    /// range starting at 100 so they never collide with the built-in set.
    fn effect_id(effect_name: &str) -> i32 {
        match effect_name {
            "IO" => 1,
            "Error" => 2,
            "Async" => 3,
            "State" => 4,
            "NonDet" => 5,
            _ => {
                // Hash the name for user-defined effects
                let mut hash: i32 = 0;
                for b in effect_name.bytes() {
                    hash = hash.wrapping_mul(31).wrapping_add(b as i32);
                }
                hash.abs() + 100 // offset to avoid collision with built-ins
            }
        }
    }

    /// Look up the type of a parameter in an effect operation definition.
    ///
    /// `effect_name` - the resolved effect name (e.g. "Console")
    /// `op_name`     - the operation name (e.g. "log")
    /// `param_idx`   - zero-based parameter index
    ///
    /// Returns `None` if the effect or operation is not found, or if the index
    /// is out of range.
    fn lookup_effect_param_type(
        &self,
        effect_name: &str,
        op_name: &str,
        param_idx: usize,
    ) -> Option<MirType> {
        let ops = self.effect_defs.get(effect_name)?;
        for (name, param_types) in ops {
            if name.as_ref() == op_name {
                return param_types.get(param_idx).cloned();
            }
        }
        None
    }

    /// Lower an `ExprKind::Handle` expression.
    ///
    /// ```text
    /// handle { body } with { Effect.op(params) => handler_body, ... }
    /// ```
    ///
    /// Generates MIR that:
    /// 1. Allocates a `BuildHandler` on the stack (as a struct local).
    /// 2. Calls `build_push_handler(&handler, effect_id)`.
    /// 3. Calls `setjmp(handler.env)`:
    ///    - If the result is 0  -> execute the body normally, then pop the handler.
    ///    - If the result is N  -> dispatch to handler clause N-1.
    /// 4. Pops the handler on every exit path.
    pub(crate) fn lower_handle(
        &mut self,
        effect: &ast::Path,
        handlers: &[ast::EffectHandler],
        body: &ast::Block,
    ) -> CodegenResult<MirValue> {
        // Resolve the effect name and its integer ID.
        let effect_name = effect
            .segments
            .iter()
            .map(|s| s.ident.name.as_ref())
            .collect::<Vec<_>>()
            .join("::");
        let eid = Self::effect_id(&effect_name);

        // --- Allocate locals ---------------------------------------------------
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        // The handler struct is opaque at the MIR level; the C backend will emit
        // a `BuildHandler` declaration for it.  We represent it as an i8 array
        // large enough to hold the C struct (the runtime defines the real type).
        let handler_local = builder.create_named_local(
            format!("__handler_{}", effect_name),
            MirType::Struct(Arc::from("BuildHandler")),
        );

        // The local that receives the setjmp return value (0 = normal, N = op N-1).
        let setjmp_result = builder.create_local(MirType::i32());

        // The final result of the handle expression.
        let handle_result = builder.create_local(MirType::i32());

        // --- Create blocks ------------------------------------------------------
        let push_block = builder.create_labeled_block("effect_push");
        let body_block = builder.create_labeled_block("effect_body");
        let merge_block = builder.create_labeled_block("effect_merge");

        // Create a block for each handler clause.
        let handler_blocks: Vec<BlockId> = handlers
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let builder = self.current_fn.as_mut().unwrap();
                builder.create_labeled_block(format!("effect_handler_{}", i))
            })
            .collect();

        // --- Emit: push handler -------------------------------------------------
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.goto(push_block);
            builder.switch_to_block(push_block);

            // build_push_handler(&handler, effect_id)
            let push_fn = MirValue::Function(Arc::from("build_push_handler"));
            // Take address of the handler struct so the C call gets a pointer.
            let handler_ptr_local = builder.create_local(MirType::Ptr(Box::new(MirType::Struct(
                Arc::from("BuildHandler"),
            ))));
            builder.assign(
                handler_ptr_local,
                MirRValue::AddressOf {
                    is_mut: true,
                    place: MirPlace::local(handler_local),
                },
            );
            let eid_val = MirValue::Const(MirConst::Int(eid as i128, MirType::i32()));
            let cont = builder.create_block();
            builder.call(
                push_fn,
                vec![MirValue::Local(handler_ptr_local), eid_val],
                None,
                cont,
            );
            builder.switch_to_block(cont);

            // setjmp(handler.env) - pass the handler local directly;
            // the C backend will emit `.env` when it sees a setjmp call
            // with a BuildHandler-typed argument.
            let setjmp_fn = MirValue::Function(Arc::from("setjmp"));
            let cont2 = builder.create_block();
            builder.call(
                setjmp_fn,
                vec![MirValue::Local(handler_local)],
                Some(setjmp_result),
                cont2,
            );
            builder.switch_to_block(cont2);

            // Branch: if setjmp_result == 0 -> body, else dispatch
            let zero = MirValue::Const(MirConst::Int(0, MirType::i32()));
            let is_normal = builder.create_local(MirType::Bool);
            builder.binary_op(is_normal, BinOp::Eq, MirValue::Local(setjmp_result), zero);

            // Build switch targets for handler dispatch.
            // setjmp returns op_id + 1, so handler clause i fires when result == i+1.
            let default_block = if handler_blocks.is_empty() {
                merge_block
            } else {
                handler_blocks[0]
            };

            if handler_blocks.len() <= 1 {
                // Simple: either body or first (only) handler
                builder.branch(MirValue::Local(is_normal), body_block, default_block);
            } else {
                // Multi-handler: first check if normal, then switch on op_id.
                let dispatch_block = builder.create_labeled_block("effect_dispatch");
                builder.branch(MirValue::Local(is_normal), body_block, dispatch_block);

                builder.switch_to_block(dispatch_block);
                let targets: Vec<(MirConst, BlockId)> = handler_blocks
                    .iter()
                    .enumerate()
                    .map(|(i, &blk)| (MirConst::Int((i as i128) + 1, MirType::i32()), blk))
                    .collect();
                builder.switch(MirValue::Local(setjmp_result), targets, merge_block);
            }
        }

        // --- Emit: body (normal path, setjmp returned 0) ------------------------
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(body_block);
        }
        let body_val = self.lower_block(body)?;
        {
            let builder = self.current_fn.as_mut().unwrap();
            if let Some(ref v) = body_val {
                // Don't assign void/unit values to the handle result.
                if !matches!(v, MirValue::Const(MirConst::Unit)) {
                    builder.assign(handle_result, MirRValue::Use(v.clone()));
                }
            }
            // Pop the handler after the body completes normally.
            let pop_fn = MirValue::Function(Arc::from("build_pop_handler"));
            let cont = builder.create_block();
            builder.call(pop_fn, vec![], None, cont);
            builder.switch_to_block(cont);
            builder.goto(merge_block);
        }

        // --- Emit: handler clauses ----------------------------------------------
        for (i, handler) in handlers.iter().enumerate() {
            {
                let builder = self.current_fn.as_mut().unwrap();
                builder.switch_to_block(handler_blocks[i]);
            }

            // Map handler parameters to locals so the handler body can reference
            // them.  For the setjmp model the parameter data is available via
            // `handler.handler_data`; at the MIR level we just create named
            // locals that the C backend will initialise from `handler_data`.
            for (param_idx, param) in handler.params.iter().enumerate() {
                if let ast::PatternKind::Ident { name, .. } = &param.pattern.kind {
                    // Determine parameter type: use explicit annotation if present,
                    // otherwise look up the effect operation's parameter types
                    // from the collected effect definitions.
                    let op_name = handler.operation.name.as_ref();
                    let ty = param
                        .ty
                        .as_ref()
                        .map(|t| self.lower_type_from_ast(t))
                        .unwrap_or_else(|| {
                            self.lookup_effect_param_type(&effect_name, op_name, param_idx)
                                .unwrap_or(MirType::i32())
                        });
                    let builder = self.current_fn.as_mut().unwrap();
                    let local = builder.create_named_local(name.name.clone(), ty.clone());
                    self.var_map.insert(name.name.clone(), local);

                    // Load argument `param_idx` from handler_data.
                    // The perform site stores a `void*[N]` argv array in
                    // handler_data, where argv[i] points to argument i. The
                    // `handler_data#<i>` field marker tells the C backend which
                    // argument to read; it renders as:
                    //   msg = *(ParamType*)((void**)__handler_EffectName.handler_data)[i]
                    let handler_data_field = MirRValue::FieldAccess {
                        base: MirValue::Local(handler_local),
                        field_name: Arc::from(format!("handler_data#{}", param_idx)),
                        field_ty: MirType::Ptr(Box::new(MirType::Void)),
                    };
                    builder.assign(local, handler_data_field);
                }
            }

            // Lower the handler body expression.
            let handler_val = self.lower_expr(&handler.body)?;
            {
                let builder = self.current_fn.as_mut().unwrap();
                // Don't assign void/unit values to the handle result.
                // Also skip if the handler_val is a local that isn't declared in
                // the current function (can happen with resume return values).
                let should_assign = !matches!(handler_val, MirValue::Const(MirConst::Unit))
                    && !matches!(handler_val, MirValue::Const(MirConst::Bool(_)))
                    && match &handler_val {
                        MirValue::Local(lid) => builder.local_exists(*lid),
                        _ => true,
                    };
                if should_assign {
                    builder.assign(handle_result, MirRValue::Use(handler_val));
                }
                // Pop handler after handling.
                let pop_fn = MirValue::Function(Arc::from("build_pop_handler"));
                let cont = builder.create_block();
                builder.call(pop_fn, vec![], None, cont);
                builder.switch_to_block(cont);
                builder.goto(merge_block);
            }
        }

        // --- Merge block --------------------------------------------------------
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(merge_block);
        }

        Ok(values::local(handle_result))
    }

    /// Lower an `ExprKind::Resume` expression.
    ///
    /// In the setjmp/longjmp one-shot model, `resume(value)` stores the resume
    /// value into the handler result local and returns from the handler clause.
    /// Because the handler clause already ends with an assignment to the
    /// handle-expression result followed by a goto to the merge block, the
    /// simplest lowering is to evaluate the resume value and return it as the
    /// handler clause's value.
    pub(crate) fn lower_resume(&mut self, value: Option<&ast::Expr>) -> CodegenResult<MirValue> {
        if let Some(expr) = value {
            self.lower_expr(expr)
        } else {
            Ok(values::unit())
        }
    }

    /// Lower an `ExprKind::Perform` expression.
    ///
    /// ```text
    /// perform Effect.op(arg1, arg2, ...)
    /// ```
    ///
    /// Generates a call to `build_perform(effect_id, op_id, arg_ptr, result_ptr)`
    /// which longjmps to the nearest matching handler.  The first argument is
    /// passed via the `arg` pointer; the result pointer is set up so the handler
    /// can write a return value back to the perform-site (for the one-shot model
    /// this is not used, but the slot is allocated for future coroutine support).
    pub(crate) fn lower_perform(
        &mut self,
        effect: &ast::Ident,
        operation: &ast::Ident,
        args: &[ast::Expr],
    ) -> CodegenResult<MirValue> {
        let eid = Self::effect_id(effect.name.as_ref());

        // Compute a simple operation index from the operation name.
        // In a full implementation this would look up the effect declaration to
        // find the canonical index; here we use a hash so that different op
        // names within the same effect get distinct IDs.
        let op_id: i32 = {
            let mut h: i32 = 0;
            for b in operation.name.bytes() {
                h = h.wrapping_mul(31).wrapping_add(b as i32);
            }
            h.abs()
        };

        // Lower every argument. Each is marshalled into a `void*[N]` argv
        // array whose i-th slot holds the address of argument i; the handler
        // reads argument i back via `((void**)handler_data)[i]`. Passing the
        // whole argv (not just the first arg) is what lets a multi-parameter
        // operation see all of its arguments. A zero-argument operation passes
        // a null `arg` pointer, which its handler never dereferences.
        let arg_vals: Vec<MirValue> = args
            .iter()
            .map(|a| self.lower_expr(a))
            .collect::<CodegenResult<Vec<_>>>()?;

        // Compute each argument's type before borrowing current_fn mutably.
        let arg_tys: Vec<MirType> = arg_vals.iter().map(|v| self.type_of_value(v)).collect();

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        // Allocate a result slot on the stack.  Use an unnamed local to
        // avoid duplicate C declarations when perform is called multiple
        // times with the same effect/operation.
        let result_local = builder.create_local(MirType::i32());

        // Build the argv array of pointers-to-arguments. Each argument value is
        // stored into its own local so its address is stable, then written into
        // `argv[i]`; a `T*` slot value decays to the `void*` element type in C.
        let n = arg_vals.len();
        let arg_ptr_value: MirValue = if n == 0 {
            MirValue::Const(MirConst::Null(MirType::Void))
        } else {
            let argv = builder.create_local(MirType::Array(
                Box::new(MirType::Ptr(Box::new(MirType::Void))),
                n as u64,
            ));
            for (i, (val, ty)) in arg_vals.into_iter().zip(arg_tys.into_iter()).enumerate() {
                let arg_local = builder.create_local(ty);
                builder.assign(arg_local, MirRValue::Use(val));
                builder.push_index_store(
                    MirValue::Local(argv),
                    MirValue::Const(MirConst::Int(i as i128, MirType::i32())),
                    MirType::Ptr(Box::new(MirType::Void)),
                    MirRValue::AddressOf {
                        is_mut: false,
                        place: MirPlace::local(arg_local),
                    },
                );
            }
            MirValue::Local(argv)
        };

        // Take the address of the result slot for the void* result parameter.
        let result_ptr = builder.create_local(MirType::Ptr(Box::new(MirType::i32())));
        builder.assign(
            result_ptr,
            MirRValue::AddressOf {
                is_mut: true,
                place: MirPlace::local(result_local),
            },
        );

        // build_perform(effect_id, op_id, argv, &result)
        let perform_fn = MirValue::Function(Arc::from("build_perform"));
        let eid_val = MirValue::Const(MirConst::Int(eid as i128, MirType::i32()));
        let op_val = MirValue::Const(MirConst::Int(op_id as i128, MirType::i32()));

        let cont = builder.create_block();
        builder.call(
            perform_fn,
            vec![eid_val, op_val, arg_ptr_value, MirValue::Local(result_ptr)],
            None,
            cont,
        );
        builder.switch_to_block(cont);

        // In practice build_perform never returns (it longjmps), but at the MIR
        // level we model the continuation so the CFG remains well-formed.  The
        // result local is available if a future coroutine-based implementation
        // stores a return value there.
        Ok(values::local(result_local))
    }

    // =========================================================================
    // BUILTIN MACRO LOWERING
    // =========================================================================

    /// Lower `vec![a, b, c]` (literal) and `vec![val; count]` (repeat) macros.
    ///
    /// Expansion:
    ///   vec![a, b, c]     =>  let v = vec_new_T(); vec_push_T(v, a); vec_push_T(v, b); vec_push_T(v, c); v
    ///   vec![val; count]  =>  let v = vec_new_T(); for i in 0..count { vec_push_T(v, val); } v
    ///
    /// The element type is inferred from the first argument expression.
    pub(crate) fn lower_vec_macro(&mut self, tokens: &[ast::TokenTree]) -> CodegenResult<MirValue> {
        // When a `Vec<T>` type is expected (from a let annotation or another
        // bidirectional hint), lower each element with `T` as its expected type
        // and build the vec at `T`. Without this an annotated
        // `let v: Vec<i64> = vec![1, 2, 3]` infers `i32` from the bare literals,
        // stores 4-byte-strided elements through the i32 push helper, and every
        // `v[i]` reads at the i64 width the annotation requires -> the read
        // spans two elements: a silent wrong answer with no diagnostic.
        let expected_elem: Option<MirType> = match &self.expected_type {
            Some(MirType::Vec(elem)) => Some((**elem).clone()),
            _ => None,
        };

        // Check if tokens have valid spans matching our source file.
        // When the macro expander expands nested vec! calls, the inner
        // tokens get synthetic spans that don't match self.source.
        // In that case, fall back to creating an empty vec (the macro
        // expander already handled the expansion at a higher level).
        if let Some(ref source) = self.source {
            let has_valid_spans = tokens.iter().any(|t| {
                if let ast::TokenTree::Token(tok) = t {
                    let s = tok.span.start.to_usize();
                    let e = tok.span.end.to_usize();
                    e > s
                        && e <= source.len()
                        && source.is_char_boundary(s)
                        && source.is_char_boundary(e)
                        && {
                            // Verify the span actually points to code, not a comment
                            let text = &source[s..e];
                            // For an Ident token, the text should be alphanumeric
                            match &tok.kind {
                                crate::lexer::TokenKind::Ident => {
                                    text.chars().all(|c| c.is_alphanumeric() || c == '_')
                                }
                                _ => true,
                            }
                        }
                } else {
                    true // Delimited groups are fine
                }
            });

            if !has_valid_spans && !tokens.is_empty() {
                // Tokens have synthetic spans - this vec! was already expanded
                // by the macro expander. Create an empty vec as placeholder,
                // at the expected element type when one is known (else the f64
                // default this path has always used).
                let placeholder_elem = expected_elem.clone().unwrap_or_else(MirType::f64);
                let (new_fn_name, _) = Self::vec_fn_names_for_type(&placeholder_elem)?;
                let new_fn = MirValue::Function(Arc::from(new_fn_name));
                let vec_ty = MirType::Vec(Box::new(placeholder_elem));
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let vec_local = builder.create_local(vec_ty);
                let cont = builder.create_block();
                builder.call(new_fn, vec![], Some(vec_local), cont);
                builder.switch_to_block(cont);
                return Ok(MirValue::Local(vec_local));
            }
        }

        // Split macro tokens into argument groups at top-level commas/semicolons.
        let all_groups = self.split_vec_macro_token_groups(tokens);

        if all_groups.is_empty() {
            // vec![] with no args -- empty vec at the expected element type,
            // else i32.
            let empty_elem = expected_elem.clone().unwrap_or_else(MirType::i32);
            let (new_fn_name, _) = Self::vec_fn_names_for_type(&empty_elem)?;
            let new_fn = MirValue::Function(Arc::from(new_fn_name));
            let vec_ty = MirType::Vec(Box::new(empty_elem));
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let vec_local = builder.create_local(vec_ty);
            let cont = builder.create_block();
            builder.call(new_fn, vec![], Some(vec_local), cont);
            builder.switch_to_block(cont);
            return Ok(MirValue::Local(vec_local));
        }

        // Check for repeat syntax: vec![val; count]
        let is_repeat = self.detect_vec_repeat_syntax(tokens);

        if is_repeat && all_groups.len() == 2 {
            // vec![val; count] -- repeat form
            let val_tokens = all_groups[0].clone();
            let count_tokens = all_groups[1].clone();

            // Lower the value at the expected element type so a bare literal
            // adopts the annotated width; fall back to the inferred type.
            let prev_expected = self.expected_type.take();
            self.expected_type = expected_elem.clone();
            let val = self.parse_and_lower_token_group(val_tokens);
            self.expected_type = prev_expected;
            let val = val?;
            let elem_ty = expected_elem
                .clone()
                .unwrap_or_else(|| self.type_of_value(&val));
            let (new_fn_name, push_fn_name) = Self::vec_fn_names_for_type(&elem_ty)?;
            let vec_ty = MirType::Vec(Box::new(elem_ty));

            // Create the vec
            let new_fn = MirValue::Function(Arc::from(new_fn_name));
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let vec_local = builder.create_local(vec_ty);
            let cont = builder.create_block();
            builder.call(new_fn, vec![], Some(vec_local), cont);
            builder.switch_to_block(cont);

            // Parse and lower the count expression
            let count_val = self.parse_and_lower_token_group(count_tokens)?;

            // Emit a counted loop: for i in 0..count { vec_push(v, val) }
            // We implement this as a simple while loop in MIR:
            //   let i = 0; while (i < count) { vec_push(v, val); i = i + 1; }
            let i_local = {
                let builder = self.current_fn.as_mut().unwrap();
                let i = builder.create_local(MirType::i64());
                builder.assign(
                    i,
                    MirRValue::Use(MirValue::Const(MirConst::Int(0, MirType::i64()))),
                );
                i
            };

            // Create loop blocks
            let (loop_header, loop_body, loop_exit) = {
                let builder = self.current_fn.as_mut().unwrap();
                let header = builder.create_block();
                let body = builder.create_block();
                let exit = builder.create_block();
                (header, body, exit)
            };

            // Jump to loop header
            {
                let builder = self.current_fn.as_mut().unwrap();
                builder.goto(loop_header);
                builder.switch_to_block(loop_header);
            }

            // Condition: i < count
            let cond = {
                let builder = self.current_fn.as_mut().unwrap();
                let cond = builder.create_local(MirType::Bool);
                builder.assign(
                    cond,
                    MirRValue::BinaryOp {
                        op: BinOp::Lt,
                        left: MirValue::Local(i_local),
                        right: count_val.clone(),
                    },
                );
                cond
            };

            // Branch on condition
            {
                let builder = self.current_fn.as_mut().unwrap();
                builder.branch(MirValue::Local(cond), loop_body, loop_exit);
                builder.switch_to_block(loop_body);
            }

            // Push the value
            let push_fn = MirValue::Function(Arc::from(push_fn_name));
            {
                let builder = self.current_fn.as_mut().unwrap();
                let cont2 = builder.create_block();
                builder.call(push_fn, vec![MirValue::Local(vec_local), val], None, cont2);
                builder.switch_to_block(cont2);
            }

            // Increment i
            {
                let builder = self.current_fn.as_mut().unwrap();
                let incremented = builder.create_local(MirType::i64());
                builder.assign(
                    incremented,
                    MirRValue::BinaryOp {
                        op: BinOp::Add,
                        left: MirValue::Local(i_local),
                        right: MirValue::Const(MirConst::Int(1, MirType::i64())),
                    },
                );
                builder.assign(i_local, MirRValue::Use(MirValue::Local(incremented)));
                builder.goto(loop_header);
            }

            // Switch to exit block
            {
                let builder = self.current_fn.as_mut().unwrap();
                builder.switch_to_block(loop_exit);
            }

            Ok(MirValue::Local(vec_local))
        } else {
            // vec![a, b, c] -- literal form
            // Lower the first argument at the expected element type (so a bare
            // literal adopts the annotated width), else infer from its value.
            let first_tokens = all_groups[0].clone();
            let prev_expected = self.expected_type.take();
            self.expected_type = expected_elem.clone();
            let first_val = self.parse_and_lower_token_group(first_tokens);
            self.expected_type = prev_expected;
            let first_val = first_val?;
            let elem_ty = expected_elem
                .clone()
                .unwrap_or_else(|| self.type_of_value(&first_val));
            let (new_fn_name, push_fn_name) = Self::vec_fn_names_for_type(&elem_ty)?;
            let vec_ty = MirType::Vec(Box::new(elem_ty));

            // Create the vec
            let new_fn = MirValue::Function(Arc::from(new_fn_name));
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let vec_local = builder.create_local(vec_ty);
            let cont = builder.create_block();
            builder.call(new_fn, vec![], Some(vec_local), cont);
            builder.switch_to_block(cont);

            // Push the first element
            let push_fn_val = MirValue::Function(Arc::from(push_fn_name.as_str()));
            {
                let builder = self.current_fn.as_mut().unwrap();
                let cont2 = builder.create_block();
                builder.call(
                    push_fn_val,
                    vec![MirValue::Local(vec_local), first_val],
                    None,
                    cont2,
                );
                builder.switch_to_block(cont2);
            }

            // Push remaining elements, each lowered at the expected element type.
            for group in &all_groups[1..] {
                let prev_expected = self.expected_type.take();
                self.expected_type = expected_elem.clone();
                let val = self.parse_and_lower_token_group(group.clone());
                self.expected_type = prev_expected;
                let val = val?;
                let push_fn_val = MirValue::Function(Arc::from(push_fn_name.as_str()));
                let builder = self.current_fn.as_mut().unwrap();
                let cont2 = builder.create_block();
                builder.call(
                    push_fn_val,
                    vec![MirValue::Local(vec_local), val],
                    None,
                    cont2,
                );
                builder.switch_to_block(cont2);
            }

            Ok(MirValue::Local(vec_local))
        }
    }

    /// Map a vec element type to its runtime build/push helper pair, or reject
    /// the element type when the HVec runtime cannot store it.
    ///
    /// Two storage strategies back a `Vec<T>`. Scalars and strings ride a
    /// built-in `build_hvec_*_<suffix>` family: f64 (also f32), i64 (also
    /// isize), i32 (the narrow integer widths, u32, char, and bool), and str.
    /// A user struct, a nested vector, or a map rides a monomorphized,
    /// element-sized wrapper the C backend emits per element type
    /// (`vec_elem_needs_sized_wrapper` in codegen/backend/c.rs), keyed by the
    /// struct name or the nested handle's C type. Selecting the wrapper suffix
    /// here keeps `vec![Point { .. }]` in step with `v.push(Point { .. })`,
    /// which the vec method accessors already lower through the same wrapper.
    ///
    /// A tuple or an array element has no representation in either strategy: no
    /// scalar family fits it and the backend emits no wrapper for it. Reject it
    /// rather than fall through to the i32 family, which for an array element
    /// silently miscompiled (the array decayed to a pointer truncated to
    /// `int32_t` under `-Wint-conversion`, warn-only) and for a tuple leaked a
    /// raw C type error. A vector of a tuple or array element is an honest gap;
    /// it needs a wrapper that stores and projects the aggregate, the same path
    /// structs and nested vectors already take.
    ///
    /// Keep the supported suffixes in sync with `hvec_elem_suffix` and
    /// `vec_elem_needs_sized_wrapper` in codegen/backend/c.rs and with the vec
    /// method and free-function accessor tables in codegen/lower/expr.rs, so a
    /// `vec![...]` builds the same typed handle those accessors later read.
    fn vec_fn_names_for_type(elem_ty: &MirType) -> CodegenResult<(String, String)> {
        let suffix: String = match elem_ty {
            MirType::Float(FloatSize::F64) | MirType::Float(FloatSize::F32) => "f64".to_string(),
            MirType::Int(IntSize::I64, _) | MirType::Int(IntSize::ISize, _) => "i64".to_string(),
            MirType::Struct(n) if n.as_ref() == "BuildString" => "str".to_string(),
            // The narrow integer widths, u32, char (lowered to u32), and bool all
            // pass to the i32 helper's `int32_t` parameter by value, so their
            // build and read strides agree.
            MirType::Int(_, _) | MirType::Bool => "i32".to_string(),
            // A user struct, a nested vector, or a map rides the element-sized
            // wrapper the C backend generates; the suffix is the struct name or
            // the nested handle's C type, matching hvec_elem_suffix.
            MirType::Struct(n) => n.to_string(),
            MirType::Vec(_) => "BuildVecHandle".to_string(),
            MirType::Map(_, _) => "BuildMapHandle".to_string(),
            _ => {
                return Err(CodegenError::Unsupported(format!(
                    "a vector of `{elem_ty}` elements is not supported yet: the Vec \
                     runtime stores scalar (integer, float, bool, char), string, \
                     struct, nested-vector, and map elements, not a tuple or array \
                     element"
                )))
            }
        };
        Ok((
            format!("build_hvec_new_{suffix}"),
            format!("build_hvec_push_{suffix}"),
        ))
    }

    /// Extract all argument source texts from vec! macro tokens.
    /// Unlike print macros, vec! has no format string to skip.
    /// Split vec! macro tokens into argument token groups at top-level
    /// commas or semicolons. Returns flattened token lists suitable for
    /// passing directly to the parser, avoiding span-based source text
    /// extraction which breaks when tokens have cross-source spans.
    fn split_vec_macro_token_groups(
        &self,
        tokens: &[ast::TokenTree],
    ) -> Vec<Vec<crate::lexer::Token>> {
        use crate::lexer::TokenKind;

        // Unwrap outermost Delimited group if present
        let inner: &[ast::TokenTree] = if tokens.len() == 1 {
            if let ast::TokenTree::Delimited {
                tokens: ref inner, ..
            } = tokens[0]
            {
                inner
            } else {
                tokens
            }
        } else {
            tokens
        };

        let mut groups: Vec<Vec<crate::lexer::Token>> = Vec::new();
        let mut current: Vec<crate::lexer::Token> = Vec::new();
        let mut depth: i32 = 0;

        // Recursively flatten a TokenTree into a Vec<Token>
        fn flatten_tree(tree: &ast::TokenTree, out: &mut Vec<crate::lexer::Token>) {
            match tree {
                ast::TokenTree::Token(tok) => out.push(tok.clone()),
                ast::TokenTree::Delimited {
                    delimiter,
                    tokens: inner,
                    span,
                } => {
                    out.push(crate::lexer::Token::new(
                        TokenKind::OpenDelim(*delimiter),
                        *span,
                    ));
                    for t in inner {
                        flatten_tree(t, out);
                    }
                    out.push(crate::lexer::Token::new(
                        TokenKind::CloseDelim(*delimiter),
                        *span,
                    ));
                }
            }
        }

        for token in inner {
            match token {
                ast::TokenTree::Token(tok) => match &tok.kind {
                    TokenKind::OpenDelim(_) => {
                        depth += 1;
                        current.push(tok.clone());
                    }
                    TokenKind::CloseDelim(_) => {
                        depth -= 1;
                        current.push(tok.clone());
                    }
                    TokenKind::Comma | TokenKind::Semi if depth == 0 => {
                        if !current.is_empty() {
                            groups.push(std::mem::take(&mut current));
                        }
                    }
                    _ => {
                        current.push(tok.clone());
                    }
                },
                ast::TokenTree::Delimited { .. } => {
                    // Flatten the delimited group into individual tokens
                    // so the parser can handle it
                    flatten_tree(token, &mut current);
                }
            }
        }

        if !current.is_empty() {
            groups.push(current);
        }

        groups
    }

    /// Parse a token group as an expression and lower it.
    fn parse_and_lower_token_group(
        &mut self,
        tokens: Vec<crate::lexer::Token>,
    ) -> CodegenResult<MirValue> {
        use crate::lexer::SourceFile;
        use crate::parser::Parser;

        // Build source text. First try span-based extraction from source,
        // then validate it by re-tokenizing. If validation fails, use
        // reconstructed text from token kinds.
        let source_text = self.extract_source_for_tokens(&tokens);

        let sf = SourceFile::anonymous(source_text.as_str());
        match crate::lexer::tokenize(source_text.as_str()) {
            Ok(new_tokens) => {
                let mut parser = Parser::new(&sf, new_tokens);
                match parser.parse_expr() {
                    Ok(expr) => {
                        // The parsed expression's token spans are relative to
                        // `source_text` (the anonymous re-tokenized string), not
                        // the original file. Point self.source at it while we
                        // lower, so a nested macro that re-extracts source by
                        // span reads the right text. Without this, an inner
                        // vec! inside a Vec<Vec<_>> literal sliced the original
                        // file at a stale offset and pushed a garbage element
                        // (an unrelated identifier, then the 0 parse-failure
                        // default). Restore the previous source afterward.
                        let prev_source = self.source.replace(Arc::from(source_text.as_str()));
                        let result = self.lower_expr(&expr);
                        self.source = prev_source;
                        result
                    }
                    Err(_) => {
                        // Parsing failed - tokens likely have synthetic spans.
                        // Return a sensible default (0 for numerics).
                        Ok(MirValue::Const(MirConst::Int(0, MirType::i64())))
                    }
                }
            }
            Err(_) => {
                // Tokenization failed - return default
                Ok(MirValue::Const(MirConst::Int(0, MirType::i64())))
            }
        }
    }

    /// Extract source text for a token group. Tries span-based extraction first,
    /// validates by checking token count matches, falls back to reconstruction.
    fn extract_source_for_tokens(&self, tokens: &[crate::lexer::Token]) -> String {
        if let Some(ref src) = self.source {
            let first_start = tokens.first().map(|t| t.span.start.to_usize()).unwrap_or(0);
            let last_end = tokens.last().map(|t| t.span.end.to_usize()).unwrap_or(0);
            if last_end > first_start
                && last_end <= src.len()
                && src.is_char_boundary(first_start)
                && src.is_char_boundary(last_end)
            {
                let candidate = src[first_start..last_end].to_string();
                // Validate: re-tokenize and check it produces a similar token count
                if let Ok(re_tokens) = crate::lexer::tokenize(candidate.as_str()) {
                    // If re-tokenization gives roughly the same number of tokens, use it
                    if re_tokens.len() >= tokens.len().saturating_sub(2)
                        && re_tokens.len() <= tokens.len() + 2
                    {
                        return candidate;
                    }
                }
            }
        }
        // Fallback: reconstruct from token kinds
        self.reconstruct_source_from_tokens(tokens)
    }

    /// Reconstruct source text from token kinds when spans are unreliable.
    fn reconstruct_source_from_tokens(&self, tokens: &[crate::lexer::Token]) -> String {
        use crate::lexer::{Delimiter, LiteralKind, TokenKind};

        let mut parts = Vec::new();
        for tok in tokens {
            let text = match &tok.kind {
                TokenKind::Ident | TokenKind::RawIdent | TokenKind::Lifetime => {
                    // Try to extract from source
                    if let Some(ref src) = self.source {
                        let s = tok.span.start.to_usize();
                        let e = tok.span.end.to_usize();
                        if e > s
                            && e <= src.len()
                            && src.is_char_boundary(s)
                            && src.is_char_boundary(e)
                        {
                            src[s..e].to_string()
                        } else {
                            "_".to_string()
                        }
                    } else {
                        "_".to_string()
                    }
                }
                TokenKind::Literal { kind, suffix: _ } => {
                    if let Some(ref src) = self.source {
                        let s = tok.span.start.to_usize();
                        let e = tok.span.end.to_usize();
                        if e > s && e <= src.len() {
                            src[s..e].to_string()
                        } else {
                            match kind {
                                LiteralKind::Int { .. } => "0".to_string(),
                                LiteralKind::Float { .. } => "0.0".to_string(),
                                LiteralKind::Str { .. } => "\"\"".to_string(),
                                LiteralKind::Char { .. } => "'\\0'".to_string(),
                                _ => "0".to_string(),
                            }
                        }
                    } else {
                        "0".to_string()
                    }
                }
                TokenKind::Keyword(kw) => format!("{:?}", kw).to_lowercase(),
                TokenKind::Plus => "+".to_string(),
                TokenKind::Minus => "-".to_string(),
                TokenKind::Star => "*".to_string(),
                TokenKind::Slash => "/".to_string(),
                TokenKind::Percent => "%".to_string(),
                TokenKind::And => "&".to_string(),
                TokenKind::Or => "|".to_string(),
                TokenKind::Caret => "^".to_string(),
                TokenKind::Not => "!".to_string(),
                TokenKind::Dot => ".".to_string(),
                TokenKind::DotDot => "..".to_string(),
                TokenKind::DotDotEq => "..=".to_string(),
                TokenKind::Comma => ",".to_string(),
                TokenKind::Semi => ";".to_string(),
                TokenKind::Colon => ":".to_string(),
                TokenKind::ColonColon => "::".to_string(),
                TokenKind::Eq => "=".to_string(),
                TokenKind::EqEq => "==".to_string(),
                TokenKind::Ne => "!=".to_string(),
                TokenKind::Lt => "<".to_string(),
                TokenKind::Gt => ">".to_string(),
                TokenKind::Le => "<=".to_string(),
                TokenKind::Ge => ">=".to_string(),
                TokenKind::OpenDelim(Delimiter::Paren) => "(".to_string(),
                TokenKind::OpenDelim(Delimiter::Bracket) => "[".to_string(),
                TokenKind::OpenDelim(Delimiter::Brace) => "{".to_string(),
                TokenKind::CloseDelim(Delimiter::Paren) => ")".to_string(),
                TokenKind::CloseDelim(Delimiter::Bracket) => "]".to_string(),
                TokenKind::CloseDelim(Delimiter::Brace) => "}".to_string(),
                TokenKind::Arrow => "->".to_string(),
                TokenKind::FatArrow => "=>".to_string(),
                TokenKind::Underscore => "_".to_string(),
                TokenKind::Tilde => "~".to_string(),
                TokenKind::At => "@".to_string(),
                TokenKind::Pound => "#".to_string(),
                TokenKind::Question => "?".to_string(),
                TokenKind::Shl => "<<".to_string(),
                TokenKind::Shr => ">>".to_string(),
                _ => {
                    // Fallback: try span extraction
                    if let Some(ref src) = self.source {
                        let s = tok.span.start.to_usize();
                        let e = tok.span.end.to_usize();
                        if e > s
                            && e <= src.len()
                            && src.is_char_boundary(s)
                            && src.is_char_boundary(e)
                        {
                            src[s..e].to_string()
                        } else {
                            " ".to_string()
                        }
                    } else {
                        " ".to_string()
                    }
                }
            };
            parts.push(text);
        }
        parts.join(" ")
    }

    /// Detect whether the token stream contains a semicolon at the top level,
    /// indicating the `vec![val; count]` repeat syntax.
    fn detect_vec_repeat_syntax(&self, tokens: &[ast::TokenTree]) -> bool {
        use crate::lexer::{Delimiter, TokenKind};

        // Unwrap outermost Delimited if present, don't flatten
        let inner: &[ast::TokenTree] = if tokens.len() == 1 {
            if let ast::TokenTree::Delimited {
                tokens: ref inner, ..
            } = tokens[0]
            {
                inner
            } else {
                tokens
            }
        } else {
            tokens
        };

        let mut depth: i32 = 0;
        for token in inner {
            match token {
                ast::TokenTree::Delimited { .. } => {
                    // Nested delimited group - skip it entirely (don't
                    // look inside for semicolons).
                }
                ast::TokenTree::Token(tok) => match &tok.kind {
                    TokenKind::OpenDelim(Delimiter::Paren)
                    | TokenKind::OpenDelim(Delimiter::Bracket)
                    | TokenKind::OpenDelim(Delimiter::Brace) => depth += 1,
                    TokenKind::CloseDelim(Delimiter::Paren)
                    | TokenKind::CloseDelim(Delimiter::Bracket)
                    | TokenKind::CloseDelim(Delimiter::Brace) => depth -= 1,
                    TokenKind::Semi if depth == 0 => return true,
                    _ => {}
                },
            }
        }
        false
    }

    /// Shared format-macro processing for `print!`/`println!`/`format!`: parse
    /// the format string and arguments, convert `{}`/`{:?}`/`{:.N}` placeholders
    /// to C printf specifiers, intern the C format string, and return its string
    /// index plus the lowered argument values (BuildString args reduced to their
    /// `.ptr`, trimmed to the placeholder count).
    fn prepare_format_call(
        &mut self,
        tokens: &[ast::TokenTree],
        newline: bool,
    ) -> CodegenResult<(u32, Vec<MirValue>)> {
        // Extract the format string from the macro tokens.
        let format_str = self.extract_string_from_tokens(tokens);

        // Extract argument source text from tokens and parse + lower each one
        // as a full expression through the normal lowering pipeline.
        let arg_source_texts = self.extract_arg_source_texts(tokens);

        // Parse and lower each argument expression, collecting the MIR values
        // and their resolved types.
        let mut arg_values: Vec<MirValue> = Vec::new();
        let mut arg_types: Vec<Option<MirType>> = Vec::new();

        // Pre-scan the format string so we know, per placeholder position,
        // whether it is a plain `{}` (default Display). A plain float placeholder
        // is rendered through the shortest-round-trip formatter instead of C's
        // lossy `%g`, so Display matches Rust. Explicit specs like `{:.3}` keep
        // their `%.3f` path.
        let plain_flags = Self::placeholder_is_plain(&format_str);

        for (arg_index, arg_src) in arg_source_texts.iter().enumerate() {
            match self.parse_and_lower_macro_arg(arg_src) {
                Ok(val) => {
                    let ty = self.type_of_value(&val);
                    // Plain `{}` on a float: convert to a shortest-round-trip
                    // string (0.1 + 0.2 -> 0.30000000000000004, 1234567.0 ->
                    // 1234567) rather than emitting C's 6-significant-digit %g.
                    if plain_flags.get(arg_index).copied().unwrap_or(false) {
                        if let MirType::Float(size) = ty {
                            let conv = match size {
                                FloatSize::F32 => "build_f32_to_string",
                                FloatSize::F64 => "build_f64_to_string",
                            };
                            let builder = self.current_fn.as_mut().unwrap();
                            let sdest =
                                builder.create_local(MirType::Struct(Arc::from("BuildString")));
                            let cont = builder.create_block();
                            builder.call(
                                MirValue::Function(Arc::from(conv)),
                                vec![val],
                                Some(sdest),
                                cont,
                            );
                            builder.switch_to_block(cont);
                            let ptr_local =
                                builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
                            builder.assign(
                                ptr_local,
                                MirRValue::FieldAccess {
                                    base: MirValue::Local(sdest),
                                    field_name: Arc::from("ptr"),
                                    field_ty: MirType::Ptr(Box::new(MirType::i8())),
                                },
                            );
                            arg_types.push(Some(MirType::Ptr(Box::new(MirType::i8()))));
                            arg_values.push(MirValue::Local(ptr_local));
                            continue;
                        }
                    }
                    // For BuildString values, extract .ptr for printf
                    if let MirType::Struct(ref name) = ty {
                        if name.as_ref() == "BuildString" {
                            let builder = self.current_fn.as_mut().unwrap();
                            let ptr_local =
                                builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
                            if let MirValue::Local(local_id) = val {
                                builder.assign(
                                    ptr_local,
                                    MirRValue::FieldAccess {
                                        base: MirValue::Local(local_id),
                                        field_name: Arc::from("ptr"),
                                        field_ty: MirType::Ptr(Box::new(MirType::i8())),
                                    },
                                );
                            }
                            arg_types.push(Some(MirType::Ptr(Box::new(MirType::i8()))));
                            arg_values.push(MirValue::Local(ptr_local));
                            continue;
                        }
                    }
                    arg_types.push(Some(ty));
                    arg_values.push(val);
                }
                Err(_) => {
                    // Fallback: try the old identifier-based lookup
                    let arg_name = arg_src.trim();
                    if let Some(&local_id) = self.var_map.get(arg_name) {
                        let local_ty = self
                            .current_fn
                            .as_ref()
                            .and_then(|b| b.local_type(local_id));
                        arg_types.push(local_ty);
                        arg_values.push(MirValue::Local(local_id));
                    } else {
                        arg_types.push(None);
                    }
                }
            }
        }

        // Convert {} / {:?} / {:.N} placeholders to C printf format specifiers.
        let mut c_fmt = String::new();
        let mut placeholder_count = 0;
        let mut chars = format_str.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '{' {
                if chars.peek() == Some(&'{') {
                    // Escaped literal brace: {{ -> {
                    chars.next();
                    c_fmt.push('{');
                } else if chars.peek() == Some(&'}') {
                    // Simple placeholder: {}
                    chars.next(); // consume '}'
                    let specifier = self.format_specifier_for_type(
                        arg_types.get(placeholder_count).and_then(|t| t.as_ref()),
                        None,
                    );
                    c_fmt.push_str(&specifier);
                    placeholder_count += 1;
                } else if chars.peek() == Some(&':') {
                    // Extended placeholder: {:?} or {:.N}
                    chars.next(); // consume ':'
                    let mut fmt_spec = String::new();
                    while let Some(&c) = chars.peek() {
                        if c == '}' {
                            chars.next();
                            break;
                        }
                        fmt_spec.push(c);
                        chars.next();
                    }
                    if fmt_spec == "?" {
                        // Debug format {:?} - print type name + value
                        let ty = arg_types.get(placeholder_count).and_then(|t| t.as_ref());
                        let type_name = self.type_debug_name(ty);
                        c_fmt.push_str(&type_name);
                        c_fmt.push('(');
                        c_fmt.push_str(&self.format_specifier_for_type(ty, None));
                        c_fmt.push(')');
                    } else if fmt_spec.starts_with('.') {
                        // Precision format {:.N} for floats
                        let precision = &fmt_spec[1..];
                        let specifier = self.format_specifier_for_type(
                            arg_types.get(placeholder_count).and_then(|t| t.as_ref()),
                            Some(precision),
                        );
                        c_fmt.push_str(&specifier);
                    } else {
                        // Unknown format spec, fall back to %d
                        c_fmt.push_str("%d");
                    }
                    placeholder_count += 1;
                } else {
                    c_fmt.push(ch);
                }
            } else if ch == '}' {
                if chars.peek() == Some(&'}') {
                    // Escaped literal brace: }} -> }
                    chars.next();
                    c_fmt.push('}');
                } else {
                    c_fmt.push(ch);
                }
            } else if ch == '%' {
                // Escape literal % for C printf: % -> %%
                c_fmt.push_str("%%");
            } else {
                c_fmt.push(ch);
            }
        }
        if newline {
            c_fmt.push('\n');
        }

        // Intern the C format string
        let str_idx = self.module.intern_string(c_fmt);

        // Trim arg_values to the number of placeholders we actually found.
        let arg_values: Vec<MirValue> = arg_values.into_iter().take(placeholder_count).collect();
        Ok((str_idx, arg_values))
    }

    /// `print!`/`println!`/`eprint!`/`dbg!`: format the arguments and write the
    /// result to stdout via printf.
    pub(crate) fn lower_print_macro(
        &mut self,
        tokens: &[ast::TokenTree],
        newline: bool,
    ) -> CodegenResult<()> {
        let (str_idx, arg_values) = self.prepare_format_call(tokens, newline)?;
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function for macro".into()))?;
        let fmt_local = builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
        builder.assign(
            fmt_local,
            MirRValue::Use(MirValue::Const(MirConst::Str(str_idx))),
        );
        let continue_block = builder.create_block();
        let printf_fn = MirValue::Function(Arc::from("printf"));
        let mut call_args = vec![MirValue::Local(fmt_local)];
        call_args.extend(arg_values);
        builder.call(printf_fn, call_args, None, continue_block);
        builder.switch_to_block(continue_block);
        Ok(())
    }

    /// `format!`: build an owned `BuildString` from the format string and
    /// arguments via the variadic `build_sprintf` runtime function (no trailing
    /// newline; returns the string instead of printing it).
    pub(crate) fn lower_format_macro(
        &mut self,
        tokens: &[ast::TokenTree],
    ) -> CodegenResult<MirValue> {
        let (str_idx, arg_values) = self.prepare_format_call(tokens, false)?;
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function for macro".into()))?;
        let fmt_local = builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
        builder.assign(
            fmt_local,
            MirRValue::Use(MirValue::Const(MirConst::Str(str_idx))),
        );
        let result = builder.create_local(MirType::Struct(Arc::from("BuildString")));
        let continue_block = builder.create_block();
        let sprintf_fn = MirValue::Function(Arc::from("build_sprintf"));
        let mut call_args = vec![MirValue::Local(fmt_local)];
        call_args.extend(arg_values);
        builder.call(sprintf_fn, call_args, Some(result), continue_block);
        builder.switch_to_block(continue_block);
        Ok(values::local(result))
    }

    /// Extract the source text of each argument expression in a macro call,
    /// after the format string.  Returns a Vec of source-text strings, one
    /// per argument.  Uses token spans to find comma-separated argument
    /// boundaries in the original source.
    fn extract_arg_source_texts(&self, tokens: &[ast::TokenTree]) -> Vec<String> {
        use crate::lexer::{Delimiter, TokenKind};

        let source = match self.source {
            Some(ref s) => s,
            None => return Vec::new(),
        };

        // Unwrap the outermost Delimited group if present, but do NOT flatten
        // nested groups - their spans must stay intact for correct source extraction.
        let inner: &[ast::TokenTree] = if tokens.len() == 1 {
            if let ast::TokenTree::Delimited {
                tokens: ref inner, ..
            } = tokens[0]
            {
                inner
            } else {
                tokens
            }
        } else {
            tokens
        };

        // Walk the token list, tracking delimiter depth to distinguish
        // top-level argument-separating commas from commas inside nested expressions.
        let mut args = Vec::new();
        let mut past_format_string = false;
        let mut depth: i32 = 0;
        let mut current_arg_start: Option<usize> = None; // byte offset in source
        let mut current_arg_end: usize = 0;

        for token in inner {
            match token {
                ast::TokenTree::Delimited { span, .. } => {
                    if !past_format_string {
                        continue;
                    }
                    // Use the full span of the delimited group to extend
                    // the current argument range without flattening.
                    let s = span.start.to_usize();
                    let e = span.end.to_usize();
                    if current_arg_start.is_none() {
                        current_arg_start = Some(s);
                    }
                    if e > current_arg_end {
                        current_arg_end = e;
                    }
                }
                ast::TokenTree::Token(tok) => {
                    if !past_format_string {
                        if let TokenKind::Literal { kind, .. } = &tok.kind {
                            if matches!(kind, crate::lexer::LiteralKind::Str { .. }) {
                                past_format_string = true;
                            }
                        }
                        continue;
                    }

                    // Track parenthesis/bracket/brace depth
                    match &tok.kind {
                        TokenKind::OpenDelim(Delimiter::Paren)
                        | TokenKind::OpenDelim(Delimiter::Bracket)
                        | TokenKind::OpenDelim(Delimiter::Brace) => {
                            depth += 1;
                            let s = tok.span.start.to_usize();
                            let e = tok.span.end.to_usize();
                            if current_arg_start.is_none() {
                                current_arg_start = Some(s);
                            }
                            if e > current_arg_end {
                                current_arg_end = e;
                            }
                        }
                        TokenKind::CloseDelim(Delimiter::Paren)
                        | TokenKind::CloseDelim(Delimiter::Bracket)
                        | TokenKind::CloseDelim(Delimiter::Brace) => {
                            depth -= 1;
                            let s = tok.span.start.to_usize();
                            let e = tok.span.end.to_usize();
                            if current_arg_start.is_none() {
                                current_arg_start = Some(s);
                            }
                            if e > current_arg_end {
                                current_arg_end = e;
                            }
                        }
                        TokenKind::Comma if depth == 0 => {
                            // Top-level comma: flush current argument
                            if let Some(start) = current_arg_start {
                                if current_arg_end > start && current_arg_end <= source.len() {
                                    let text = source
                                        .get(start..current_arg_end)
                                        .unwrap_or("")
                                        .trim()
                                        .to_string();
                                    if !text.is_empty() {
                                        args.push(text);
                                    }
                                }
                                current_arg_start = None;
                                current_arg_end = 0;
                            }
                        }
                        _ => {
                            // Extend current argument span
                            let s = tok.span.start.to_usize();
                            let e = tok.span.end.to_usize();
                            if current_arg_start.is_none() {
                                current_arg_start = Some(s);
                            }
                            if e > current_arg_end {
                                current_arg_end = e;
                            }
                        }
                    }
                }
            }
        }

        // Flush any remaining argument
        if let Some(start) = current_arg_start {
            if current_arg_end > start && current_arg_end <= source.len() {
                let text = source
                    .get(start..current_arg_end)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !text.is_empty() {
                    args.push(text);
                }
            }
        }

        args
    }

    /// Parse a source text fragment as an expression, then lower it through
    /// the normal expression-lowering pipeline.
    fn parse_and_lower_macro_arg(&mut self, src: &str) -> CodegenResult<MirValue> {
        use crate::lexer::SourceFile;
        use crate::parser::Parser;

        // Create a mini source file for the expression fragment
        let sf = SourceFile::anonymous(src);
        let tokens = crate::lexer::tokenize(src).map_err(|e| {
            CodegenError::Internal(format!("Failed to tokenize macro arg '{}': {:?}", src, e))
        })?;
        let mut parser = Parser::new(&sf, tokens);

        let expr = parser.parse_expr().map_err(|e| {
            CodegenError::Internal(format!("Failed to parse macro arg '{}': {:?}", src, e))
        })?;

        self.lower_expr(&expr)
    }

    /// Walk a Rust-style format string and report, for each placeholder in
    /// order, whether it is a plain `{}` (default Display). Escaped braces
    /// (`{{`, `}}`) are skipped and every `{:...}` form reports `false`. The
    /// brace bookkeeping mirrors the placeholder loop in `prepare_format_call`,
    /// so index `i` here lines up with placeholder `i` there.
    fn placeholder_is_plain(format_str: &str) -> Vec<bool> {
        let mut kinds = Vec::new();
        let mut chars = format_str.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '{' {
                match chars.peek() {
                    Some('{') => {
                        chars.next();
                    }
                    Some('}') => {
                        chars.next();
                        kinds.push(true);
                    }
                    Some(':') => {
                        chars.next();
                        while let Some(&c) = chars.peek() {
                            chars.next();
                            if c == '}' {
                                break;
                            }
                        }
                        kinds.push(false);
                    }
                    _ => {}
                }
            } else if ch == '}' && chars.peek() == Some(&'}') {
                chars.next();
            }
        }
        kinds
    }

    /// Pick the correct printf format specifier based on the MIR type.
    fn format_specifier_for_type(&self, ty: Option<&MirType>, precision: Option<&str>) -> String {
        match ty {
            Some(MirType::Int(IntSize::I64, true)) => "%lld".to_string(),
            Some(MirType::Int(IntSize::I64, false)) => "%llu".to_string(),
            Some(MirType::Int(_, true)) => "%d".to_string(),
            Some(MirType::Int(_, false)) => "%u".to_string(),
            Some(MirType::Float(FloatSize::F32)) | Some(MirType::Float(FloatSize::F64)) => {
                if let Some(prec) = precision {
                    format!("%.{}f", prec)
                } else {
                    "%g".to_string()
                }
            }
            Some(MirType::Bool) => "%s".to_string(), // printed via ternary in C
            Some(MirType::Ptr(_)) => "%s".to_string(), // assume string pointer
            Some(MirType::Struct(name)) if name.as_ref() == "BuildString" => "%s".to_string(),
            _ => "%d".to_string(), // default for integers (most common)
        }
    }

    /// Return a short debug type name for {:?} format.
    fn type_debug_name(&self, ty: Option<&MirType>) -> String {
        match ty {
            Some(MirType::Int(IntSize::I8, true)) => "i8",
            Some(MirType::Int(IntSize::I16, true)) => "i16",
            Some(MirType::Int(IntSize::I32, true)) => "i32",
            Some(MirType::Int(IntSize::I64, true)) => "i64",
            Some(MirType::Int(IntSize::I8, false)) => "u8",
            Some(MirType::Int(IntSize::I16, false)) => "u16",
            Some(MirType::Int(IntSize::I32, false)) => "u32",
            Some(MirType::Int(IntSize::I64, false)) => "u64",
            Some(MirType::Float(FloatSize::F32)) => "f32",
            Some(MirType::Float(FloatSize::F64)) => "f64",
            Some(MirType::Bool) => "bool",
            Some(MirType::Ptr(_)) => "str",
            Some(MirType::Struct(name)) => return name.to_string(),
            _ => "i32",
        }
        .to_string()
    }

    pub(crate) fn lower_panic_macro(&mut self, tokens: &[ast::TokenTree]) -> CodegenResult<()> {
        // Print the panic message first
        self.lower_print_macro(tokens, true)?;

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function for macro".into()))?;

        // Call abort() after printing
        builder.abort();

        // Create an unreachable continuation block for any code after panic
        let unreachable_block = builder.create_block();
        builder.switch_to_block(unreachable_block);

        Ok(())
    }

    pub(crate) fn extract_string_from_tokens(&self, tokens: &[ast::TokenTree]) -> String {
        use crate::lexer::TokenKind;

        for token in tokens {
            match token {
                ast::TokenTree::Token(tok) => {
                    if let TokenKind::Literal { kind, .. } = &tok.kind {
                        if matches!(kind, crate::lexer::LiteralKind::Str { .. }) {
                            // Try to extract the string content from source via span
                            if let Some(ref source) = self.source {
                                let start = tok.span.start.to_usize();
                                let end = tok.span.end.to_usize();
                                if start < source.len() && end <= source.len() && start < end {
                                    if let Some(raw) = source.get(start..end) {
                                        // Strip surrounding quotes
                                        let content =
                                            raw.trim_start_matches('"').trim_end_matches('"');
                                        return content.to_string();
                                    }
                                }
                            }
                            // Fallback: return empty string if source not available
                            return String::new();
                        }
                    }
                }
                ast::TokenTree::Delimited { tokens: inner, .. } => {
                    let result = self.extract_string_from_tokens(inner);
                    if !result.is_empty() {
                        return result;
                    }
                }
            }
        }
        String::new()
    }

    fn extract_arg_idents_from_tokens(&self, tokens: &[ast::TokenTree]) -> Vec<String> {
        use crate::lexer::TokenKind;

        let mut args = Vec::new();
        let mut past_first_comma = false;
        let mut current_arg = String::new();

        // Flatten delimited groups
        let flat: Vec<&ast::TokenTree> = tokens
            .iter()
            .flat_map(|t| match t {
                ast::TokenTree::Delimited { tokens: inner, .. } => inner.iter().collect::<Vec<_>>(),
                other => vec![other],
            })
            .collect();

        for token in &flat {
            if let ast::TokenTree::Token(tok) = token {
                match &tok.kind {
                    TokenKind::Comma => {
                        if past_first_comma && !current_arg.is_empty() {
                            args.push(std::mem::take(&mut current_arg));
                        }
                        past_first_comma = true;
                    }
                    TokenKind::Ident if past_first_comma => {
                        // Extract identifier name from source via span
                        if let Some(ref source) = self.source {
                            let start = tok.span.start.to_usize();
                            let end = tok.span.end.to_usize();
                            if start < source.len() && end <= source.len() {
                                let name = source.get(start..end).unwrap_or("");
                                if current_arg.is_empty() {
                                    current_arg = name.to_string();
                                } else {
                                    // Appending after a dot
                                    current_arg.push_str(name);
                                }
                            }
                        }
                    }
                    TokenKind::Dot if past_first_comma && !current_arg.is_empty() => {
                        // Part of a field access expression: ident.field
                        current_arg.push('.');
                    }
                    _ => {}
                }
            }
        }
        // Flush any remaining argument
        if past_first_comma && !current_arg.is_empty() {
            args.push(current_arg);
        }
        args
    }

    // =================================================================
    // Iterator chain lowering: .iter().map(|x| ...).collect() → loop
    // =================================================================

    /// Try to parse a method call chain as an iterator pipeline.
    /// Returns `None` if the chain doesn't start with `.iter()`.
    ///
    /// Walks backwards from the terminal (`collect` or `fold`) through
    /// nested `MethodCall` receivers until it finds `.iter()`.
    pub(crate) fn try_parse_iter_chain<'a>(
        terminal_receiver: &'a ast::Expr,
        terminal_name: &str,
        terminal_args: &'a [ast::Expr],
    ) -> Option<IterChain<'a>> {
        let terminal = match terminal_name {
            "collect" => IterTerminal::Collect,
            "fold" if terminal_args.len() == 2 => IterTerminal::Fold {
                init: &terminal_args[0],
                closure: &terminal_args[1],
            },
            "sum" if terminal_args.is_empty() => IterTerminal::Sum,
            "count" if terminal_args.is_empty() => IterTerminal::Count,
            "product" if terminal_args.is_empty() => IterTerminal::Product,
            "any" if terminal_args.len() == 1 => IterTerminal::Any {
                closure: &terminal_args[0],
            },
            "all" if terminal_args.len() == 1 => IterTerminal::All {
                closure: &terminal_args[0],
            },
            _ => return None,
        };

        // Walk backwards through the MethodCall chain.
        let mut steps: Vec<IterStep<'a>> = Vec::new();
        let mut current = terminal_receiver;

        loop {
            match &current.kind {
                ExprKind::MethodCall {
                    receiver,
                    method,
                    args,
                    ..
                } => {
                    let name = method.name.as_ref();
                    match name {
                        "iter" => {
                            // Found the base - `receiver` is the source vec.
                            steps.reverse();
                            return Some(IterChain {
                                source: receiver,
                                steps,
                                terminal,
                            });
                        }
                        "map" if args.len() == 1 => {
                            steps.push(IterStep::Map { closure: &args[0] });
                            current = receiver;
                        }
                        "filter" if args.len() == 1 => {
                            steps.push(IterStep::Filter { closure: &args[0] });
                            current = receiver;
                        }
                        "enumerate" if args.is_empty() => {
                            steps.push(IterStep::Enumerate);
                            current = receiver;
                        }
                        "cloned" if args.is_empty() => {
                            steps.push(IterStep::Cloned);
                            current = receiver;
                        }
                        "rev" if args.is_empty() => {
                            steps.push(IterStep::Rev);
                            current = receiver;
                        }
                        "take" if args.len() == 1 => {
                            steps.push(IterStep::Take { count: &args[0] });
                            current = receiver;
                        }
                        "skip" if args.len() == 1 => {
                            steps.push(IterStep::Skip { count: &args[0] });
                            current = receiver;
                        }
                        _ => return None, // Unknown intermediate method
                    }
                }
                _ => return None, // Chain doesn't lead to a MethodCall
            }
        }
    }

    /// Lower a fully parsed iterator chain into an imperative loop.
    ///
    /// For `.collect()` terminals, produces:
    /// ```text
    /// let result = vec_new_T();
    /// for i in 0..vec_len(source) {
    ///     let elem = vec_get_T(source, i);
    ///     // apply each step transform
    ///     vec_push_T(result, final_elem);
    /// }
    /// result
    /// ```
    ///
    /// For `.fold(init, |acc, x| body)` terminals, produces:
    /// ```text
    /// let acc = init;
    /// for i in 0..vec_len(source) {
    ///     let elem = vec_get_T(source, i);
    ///     // apply each step transform
    ///     acc = body(acc, elem);
    /// }
    /// acc
    /// ```
    pub(crate) fn lower_iter_chain(&mut self, chain: &IterChain<'_>) -> CodegenResult<MirValue> {
        // 1. Lower the source vec expression.
        let source_val = self.lower_expr(chain.source)?;
        let source_ty = self.type_of_value(&source_val);

        // Determine the element type from the Vec type.
        let elem_ty = match &source_ty {
            MirType::Vec(inner) => inner.as_ref().clone(),
            _ => MirType::f64(), // Fallback; most spectrum usage is f64
        };

        // Select the correct runtime function names for the element type.
        let (get_fn_name, len_fn_name) = Self::vec_get_len_fn_names(&elem_ty);

        // Store source in a local so we can reference it in the loop.
        let source_local = {
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let loc = builder.create_local(source_ty.clone());
            builder.assign(loc, MirRValue::Use(source_val));
            loc
        };

        // 2. Get the length: len = vec_len(source)
        let len_local = {
            let builder = self.current_fn.as_mut().unwrap();
            let len = builder.create_local(MirType::i64());
            let cont = builder.create_block();
            builder.call(
                MirValue::Function(Arc::from(len_fn_name)),
                vec![values::local(source_local)],
                Some(len),
                cont,
            );
            builder.switch_to_block(cont);
            len
        };

        // 3. Check for enumerate step - affects how many closure params we
        //    bind (index + element vs just element).
        let has_enumerate = chain.steps.iter().any(|s| matches!(s, IterStep::Enumerate));
        // `.rev()` iterates the source in reverse: start at len-1, step down,
        // exit when the (signed) index drops below 0.
        let reversed = chain.steps.iter().any(|s| matches!(s, IterStep::Rev));

        // 4. Determine result element type by walking the steps.
        //    After map closures, the type may change based on the closure's
        //    return type annotation.  For now we infer from the closure.
        let output_elem_ty = self.infer_chain_output_type(&elem_ty, &chain.steps);
        let (new_fn_name, push_fn_name) = Self::vec_fn_names_for_type(&output_elem_ty)?;

        // 5. Set up the result value depending on terminal type.
        let (result_local, _is_collect) = match &chain.terminal {
            IterTerminal::Collect => {
                // Create the output vec.
                let builder = self.current_fn.as_mut().unwrap();
                let vec_ty = MirType::Vec(Box::new(output_elem_ty.clone()));
                let result = builder.create_local(vec_ty);
                let cont = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from(new_fn_name)),
                    vec![],
                    Some(result),
                    cont,
                );
                builder.switch_to_block(cont);
                (result, true)
            }
            IterTerminal::Fold { init, .. } => {
                // Lower the initial accumulator value.
                let init_val = self.lower_expr(init)?;
                let init_ty = self.type_of_value(&init_val);
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(init_ty);
                builder.assign(acc, MirRValue::Use(init_val));
                (acc, false)
            }
            IterTerminal::Sum => {
                // Accumulator of the output element type, initialized to zero.
                let zero = match &output_elem_ty {
                    MirType::Float(_) => MirConst::Float(0.0, output_elem_ty.clone()),
                    _ => MirConst::Int(0, output_elem_ty.clone()),
                };
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(output_elem_ty.clone());
                builder.assign(acc, MirRValue::Use(MirValue::Const(zero)));
                (acc, false)
            }
            IterTerminal::Count => {
                // i64 counter initialized to zero; the element value is ignored.
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(MirType::i64());
                builder.assign(
                    acc,
                    MirRValue::Use(MirValue::Const(MirConst::Int(0, MirType::i64()))),
                );
                (acc, false)
            }
            IterTerminal::Product => {
                // Accumulator of the output element type, initialized to one.
                let one = match &output_elem_ty {
                    MirType::Float(_) => MirConst::Float(1.0, output_elem_ty.clone()),
                    _ => MirConst::Int(1, output_elem_ty.clone()),
                };
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(output_elem_ty.clone());
                builder.assign(acc, MirRValue::Use(MirValue::Const(one)));
                (acc, false)
            }
            IterTerminal::Any { .. } => {
                // bool accumulator, false until a matching element is seen.
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(MirType::Bool);
                builder.assign(acc, MirRValue::Use(MirValue::Const(MirConst::Bool(false))));
                (acc, false)
            }
            IterTerminal::All { .. } => {
                // bool accumulator, true until a non-matching element is seen.
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(MirType::Bool);
                builder.assign(acc, MirRValue::Use(MirValue::Const(MirConst::Bool(true))));
                (acc, false)
            }
        };

        // Per-step counters for take/skip, created before the loop so they
        // persist across iterations. Indexed by step position; None for steps
        // that don't need a counter. Counting yielded/skipped elements (rather
        // than checking the source index) makes take/skip compose correctly with
        // each other and with filter.
        let step_counters: Vec<Option<LocalId>> = {
            let builder = self.current_fn.as_mut().unwrap();
            chain
                .steps
                .iter()
                .map(|s| match s {
                    IterStep::Take { .. } | IterStep::Skip { .. } => {
                        let c = builder.create_local(MirType::i64());
                        builder.assign(
                            c,
                            MirRValue::Use(MirValue::Const(MirConst::Int(0, MirType::i64()))),
                        );
                        Some(c)
                    }
                    _ => None,
                })
                .collect()
        };

        // 6. Create the loop: for i in 0..len { ... }
        let idx_local = {
            let builder = self.current_fn.as_mut().unwrap();
            let idx = builder.create_local(MirType::i64());
            if reversed {
                // idx = len - 1
                builder.binary_op(
                    idx,
                    BinOp::Sub,
                    values::local(len_local),
                    MirValue::Const(MirConst::Int(1, MirType::i64())),
                );
            } else {
                builder.assign(
                    idx,
                    MirRValue::Use(MirValue::Const(MirConst::Int(0, MirType::i64()))),
                );
            }
            idx
        };

        let (cond_block, body_block, incr_block, exit_block) = {
            let builder = self.current_fn.as_mut().unwrap();
            let cond = builder.create_block();
            let body = builder.create_block();
            let incr = builder.create_block();
            let exit = builder.create_block();
            builder.goto(cond);
            builder.switch_to_block(cond);
            (cond, body, incr, exit)
        };

        // Condition: forward `idx < len`, reversed `idx >= 0`.
        {
            let builder = self.current_fn.as_mut().unwrap();
            let cmp = builder.create_local(MirType::Bool);
            if reversed {
                builder.binary_op(
                    cmp,
                    BinOp::Ge,
                    values::local(idx_local),
                    MirValue::Const(MirConst::Int(0, MirType::i64())),
                );
            } else {
                builder.binary_op(
                    cmp,
                    BinOp::Lt,
                    values::local(idx_local),
                    values::local(len_local),
                );
            }
            builder.branch(values::local(cmp), body_block, exit_block);
            builder.switch_to_block(body_block);
        }

        // 7. Loop body: get element, apply transforms.
        let elem_local = {
            let builder = self.current_fn.as_mut().unwrap();
            let elem = builder.create_local(elem_ty.clone());
            let cont = builder.create_block();
            builder.call(
                MirValue::Function(Arc::from(get_fn_name)),
                vec![values::local(source_local), values::local(idx_local)],
                Some(elem),
                cont,
            );
            builder.switch_to_block(cont);
            elem
        };

        // Apply each step transform to produce the final value.
        let mut current_val = values::local(elem_local);
        for (step_idx, step) in chain.steps.iter().enumerate() {
            match step {
                IterStep::Map { closure } => {
                    current_val = self.lower_iter_map_inline(
                        closure,
                        current_val,
                        if has_enumerate {
                            Some(values::local(idx_local))
                        } else {
                            None
                        },
                    )?;
                }
                IterStep::Filter { closure } => {
                    // Evaluate the predicate on the current element; if it does
                    // not hold, skip straight to the increment block (drop this
                    // element from the rest of the pipeline and the terminal).
                    let keep = self.lower_iter_map_inline(
                        closure,
                        current_val.clone(),
                        if has_enumerate {
                            Some(values::local(idx_local))
                        } else {
                            None
                        },
                    )?;
                    let builder = self.current_fn.as_mut().unwrap();
                    let keep_block = builder.create_block();
                    builder.branch(keep, keep_block, incr_block);
                    builder.switch_to_block(keep_block);
                }
                IterStep::Enumerate => {
                    // enumerate doesn't change the value; it just means
                    // subsequent map closures get (index, elem).  The index
                    // is passed via the idx_local when lowering map closures.
                }
                IterStep::Cloned => {
                    // No-op for Copy types.
                }
                IterStep::Rev => {
                    // Direction is handled by the loop bounds; per-element no-op.
                }
                IterStep::Skip { count } => {
                    // skip(n): while the skip counter < n, increment it and drop
                    // the element (goto increment); afterwards pass through. The
                    // counter (not the source index) makes this compose with
                    // filter and other skips.
                    let n_val = self.lower_expr(count)?;
                    let ctr = step_counters[step_idx].expect("skip counter");
                    let builder = self.current_fn.as_mut().unwrap();
                    let skipping = builder.create_local(MirType::Bool);
                    builder.binary_op(skipping, BinOp::Lt, values::local(ctr), n_val);
                    let skip_block = builder.create_block();
                    let pass_block = builder.create_block();
                    builder.branch(values::local(skipping), skip_block, pass_block);
                    // Skip: counter += 1, drop this element.
                    builder.switch_to_block(skip_block);
                    let nc = builder.create_local(MirType::i64());
                    builder.binary_op(
                        nc,
                        BinOp::Add,
                        values::local(ctr),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    builder.assign(ctr, MirRValue::Use(values::local(nc)));
                    builder.goto(incr_block);
                    builder.switch_to_block(pass_block);
                }
                IterStep::Take { count } => {
                    // take(n): once the take counter reaches n, exit the loop;
                    // otherwise increment it and pass the element through. The
                    // counter (not the source index) makes this compose.
                    let n_val = self.lower_expr(count)?;
                    let ctr = step_counters[step_idx].expect("take counter");
                    let builder = self.current_fn.as_mut().unwrap();
                    let done = builder.create_local(MirType::Bool);
                    builder.binary_op(done, BinOp::Ge, values::local(ctr), n_val);
                    let take_block = builder.create_block();
                    builder.branch(values::local(done), exit_block, take_block);
                    // Within the limit: counter += 1, pass through.
                    builder.switch_to_block(take_block);
                    let nc = builder.create_local(MirType::i64());
                    builder.binary_op(
                        nc,
                        BinOp::Add,
                        values::local(ctr),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    builder.assign(ctr, MirRValue::Use(values::local(nc)));
                }
            }
        }

        // 8. Terminal: push to result vec OR update accumulator.
        match &chain.terminal {
            IterTerminal::Collect => {
                let builder = self.current_fn.as_mut().unwrap();
                let cont = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from(push_fn_name)),
                    vec![values::local(result_local), current_val],
                    None,
                    cont,
                );
                builder.switch_to_block(cont);
            }
            IterTerminal::Fold { closure, .. } => {
                // Inline the fold closure: acc = closure(acc, current_val)
                let new_acc =
                    self.lower_iter_fold_inline(closure, values::local(result_local), current_val)?;
                let builder = self.current_fn.as_mut().unwrap();
                builder.assign(result_local, MirRValue::Use(new_acc));
            }
            IterTerminal::Sum => {
                // acc = acc + current_val
                let val_ty = self.type_of_value(&current_val);
                let builder = self.current_fn.as_mut().unwrap();
                let next = builder.create_local(val_ty);
                builder.binary_op(next, BinOp::Add, values::local(result_local), current_val);
                builder.assign(result_local, MirRValue::Use(values::local(next)));
            }
            IterTerminal::Count => {
                // acc = acc + 1 (the element value is unused)
                let builder = self.current_fn.as_mut().unwrap();
                let next = builder.create_local(MirType::i64());
                builder.binary_op(
                    next,
                    BinOp::Add,
                    values::local(result_local),
                    MirValue::Const(MirConst::Int(1, MirType::i64())),
                );
                builder.assign(result_local, MirRValue::Use(values::local(next)));
            }
            IterTerminal::Product => {
                // acc = acc * current_val
                let val_ty = self.type_of_value(&current_val);
                let builder = self.current_fn.as_mut().unwrap();
                let next = builder.create_local(val_ty);
                builder.binary_op(next, BinOp::Mul, values::local(result_local), current_val);
                builder.assign(result_local, MirRValue::Use(values::local(next)));
            }
            IterTerminal::Any { closure } => {
                // acc = acc || pred(elem)  (bitwise-or on bool is correct here)
                let pred = self.lower_iter_map_inline(
                    closure,
                    current_val,
                    if has_enumerate {
                        Some(values::local(idx_local))
                    } else {
                        None
                    },
                )?;
                let builder = self.current_fn.as_mut().unwrap();
                let next = builder.create_local(MirType::Bool);
                builder.binary_op(next, BinOp::BitOr, values::local(result_local), pred);
                builder.assign(result_local, MirRValue::Use(values::local(next)));
            }
            IterTerminal::All { closure } => {
                // acc = acc && pred(elem)  (bitwise-and on bool is correct here)
                let pred = self.lower_iter_map_inline(
                    closure,
                    current_val,
                    if has_enumerate {
                        Some(values::local(idx_local))
                    } else {
                        None
                    },
                )?;
                let builder = self.current_fn.as_mut().unwrap();
                let next = builder.create_local(MirType::Bool);
                builder.binary_op(next, BinOp::BitAnd, values::local(result_local), pred);
                builder.assign(result_local, MirRValue::Use(values::local(next)));
            }
        }

        // 9. Increment and loop back.
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.goto(incr_block);
            builder.switch_to_block(incr_block);
            let next_idx = builder.create_local(MirType::i64());
            builder.binary_op(
                next_idx,
                if reversed { BinOp::Sub } else { BinOp::Add },
                values::local(idx_local),
                MirValue::Const(MirConst::Int(1, MirType::i64())),
            );
            builder.assign(idx_local, MirRValue::Use(values::local(next_idx)));
            builder.goto(cond_block);
        }

        // 10. Switch to exit block and return the result.
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(exit_block);
        }

        Ok(values::local(result_local))
    }

    /// Inline-lower a `.map()` closure call: evaluate the closure body
    /// with the closure's parameter bound to `elem_val`.
    ///
    /// Instead of emitting a real closure function + call, we directly
    /// lower the closure body expression in the current function scope
    /// with the parameter variable mapped to `elem_val`.
    fn lower_iter_map_inline(
        &mut self,
        closure_expr: &ast::Expr,
        elem_val: MirValue,
        index_val: Option<MirValue>,
    ) -> CodegenResult<MirValue> {
        if let ExprKind::Closure { params, body, .. } = &closure_expr.kind {
            // Save the current var_map entries that will be shadowed.
            let mut saved: Vec<(Arc<str>, Option<LocalId>)> = Vec::new();

            if params.len() == 2 && index_val.is_some() {
                // enumerate-style: |i, x| body
                // First param = index, second param = element
                if let ast::PatternKind::Ident { name, .. } = &params[0].pattern.kind {
                    let old = self.var_map.get(&name.name).copied();
                    saved.push((name.name.clone(), old));

                    let idx_val = index_val.unwrap();
                    let param_ty = params[0]
                        .ty
                        .as_ref()
                        .map(|t| self.lower_type_from_ast(t))
                        .unwrap_or(MirType::i64());
                    let builder = self.current_fn.as_mut().unwrap();
                    let local = builder.create_local(param_ty);
                    builder.assign(local, MirRValue::Use(idx_val));
                    self.var_map.insert(name.name.clone(), local);
                }
                if let ast::PatternKind::Ident { name, .. } = &params[1].pattern.kind {
                    let old = self.var_map.get(&name.name).copied();
                    saved.push((name.name.clone(), old));

                    let param_ty = params[1]
                        .ty
                        .as_ref()
                        .map(|t| self.lower_type_from_ast(t))
                        .unwrap_or(MirType::f64());
                    let builder = self.current_fn.as_mut().unwrap();
                    let local = builder.create_local(param_ty);
                    builder.assign(local, MirRValue::Use(elem_val));
                    self.var_map.insert(name.name.clone(), local);
                }
            } else if params.len() == 1
                && index_val.is_some()
                && matches!(&params[0].pattern.kind, ast::PatternKind::Tuple(t) if t.len() == 2)
            {
                // enumerate-style with a tuple param: |(i, x)| body.
                // First tuple element = index, second = element.
                if let ast::PatternKind::Tuple(elems) = &params[0].pattern.kind {
                    if let ast::PatternKind::Ident { name, .. } = &elems[0].kind {
                        let old = self.var_map.get(&name.name).copied();
                        saved.push((name.name.clone(), old));
                        let builder = self.current_fn.as_mut().unwrap();
                        let local = builder.create_local(MirType::i64());
                        builder.assign(local, MirRValue::Use(index_val.clone().unwrap()));
                        self.var_map.insert(name.name.clone(), local);
                    }
                    if let ast::PatternKind::Ident { name, .. } = &elems[1].kind {
                        let old = self.var_map.get(&name.name).copied();
                        saved.push((name.name.clone(), old));
                        let elem_ty = self.type_of_value(&elem_val);
                        let builder = self.current_fn.as_mut().unwrap();
                        let local = builder.create_local(elem_ty);
                        builder.assign(local, MirRValue::Use(elem_val));
                        self.var_map.insert(name.name.clone(), local);
                    }
                }
            } else if let Some(first_param) = params.first() {
                // Single-param: |x| body
                if let ast::PatternKind::Ident { name, .. } = &first_param.pattern.kind {
                    let old = self.var_map.get(&name.name).copied();
                    saved.push((name.name.clone(), old));

                    let param_ty = first_param
                        .ty
                        .as_ref()
                        .map(|t| self.lower_type_from_ast(t))
                        .unwrap_or(MirType::f64());
                    let builder = self.current_fn.as_mut().unwrap();
                    let local = builder.create_local(param_ty);
                    builder.assign(local, MirRValue::Use(elem_val));
                    self.var_map.insert(name.name.clone(), local);
                }
            }

            // Lower the closure body inline.
            let result = self.lower_expr(body)?;

            // Restore shadowed var_map entries.
            for (name, old) in saved {
                if let Some(id) = old {
                    self.var_map.insert(name, id);
                } else {
                    self.var_map.remove(&name);
                }
            }

            Ok(result)
        } else {
            // Not a closure - shouldn't happen, but fall back to lowering as-is.
            self.lower_expr(closure_expr)
        }
    }

    /// Inline-lower a `.fold()` closure: `|acc, x| body`.
    fn lower_iter_fold_inline(
        &mut self,
        closure_expr: &ast::Expr,
        acc_val: MirValue,
        elem_val: MirValue,
    ) -> CodegenResult<MirValue> {
        if let ExprKind::Closure { params, body, .. } = &closure_expr.kind {
            let mut saved: Vec<(Arc<str>, Option<LocalId>)> = Vec::new();

            // Bind acc parameter.
            if let Some(acc_param) = params.first() {
                if let ast::PatternKind::Ident { name, .. } = &acc_param.pattern.kind {
                    let old = self.var_map.get(&name.name).copied();
                    saved.push((name.name.clone(), old));

                    let param_ty = acc_param
                        .ty
                        .as_ref()
                        .map(|t| self.lower_type_from_ast(t))
                        .unwrap_or(MirType::f64());
                    let builder = self.current_fn.as_mut().unwrap();
                    let local = builder.create_local(param_ty);
                    builder.assign(local, MirRValue::Use(acc_val));
                    self.var_map.insert(name.name.clone(), local);
                }
            }

            // Bind element parameter.
            if params.len() >= 2 {
                if let ast::PatternKind::Ident { name, .. } = &params[1].pattern.kind {
                    let old = self.var_map.get(&name.name).copied();
                    saved.push((name.name.clone(), old));

                    let param_ty = params[1]
                        .ty
                        .as_ref()
                        .map(|t| self.lower_type_from_ast(t))
                        .unwrap_or(MirType::f64());
                    let builder = self.current_fn.as_mut().unwrap();
                    let local = builder.create_local(param_ty);
                    builder.assign(local, MirRValue::Use(elem_val));
                    self.var_map.insert(name.name.clone(), local);
                }
            }

            let result = self.lower_expr(body)?;

            for (name, old) in saved {
                if let Some(id) = old {
                    self.var_map.insert(name, id);
                } else {
                    self.var_map.remove(&name);
                }
            }

            Ok(result)
        } else {
            self.lower_expr(closure_expr)
        }
    }

    /// Infer the output element type of an iterator chain by examining
    /// the steps.  For `.map()` closures with a return type annotation,
    /// use that.  For closures whose body is a simple identifier matching
    /// a parameter, use that parameter's annotated type.  Otherwise,
    /// propagate the input element type.
    fn infer_chain_output_type(&self, input_ty: &MirType, steps: &[IterStep<'_>]) -> MirType {
        let mut ty = input_ty.clone();
        for step in steps {
            match step {
                IterStep::Map { closure } => {
                    if let ExprKind::Closure {
                        return_type,
                        params,
                        body,
                        ..
                    } = &closure.kind
                    {
                        if let Some(ret_ty) = return_type {
                            ty = self.lower_type_from_ast(ret_ty);
                        } else {
                            // Try to infer from the closure body: if the body
                            // is a simple identifier matching a parameter, use
                            // that parameter's type annotation.
                            if let ExprKind::Ident(body_ident) = &body.kind {
                                for param in params {
                                    if let ast::PatternKind::Ident { name, .. } =
                                        &param.pattern.kind
                                    {
                                        if name.name.as_ref() == body_ident.name.as_ref() {
                                            if let Some(pt) = &param.ty {
                                                ty = self.lower_type_from_ast(pt);
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                            // For binary expressions (e.g., |x| x * 2.0),
                            // the type is preserved from the single param.
                            else if params.len() == 1 {
                                if let Some(p) = params.first() {
                                    if let Some(pt) = &p.ty {
                                        ty = self.lower_type_from_ast(pt);
                                    }
                                }
                            }
                        }
                    }
                }
                IterStep::Filter { .. }
                | IterStep::Enumerate
                | IterStep::Cloned
                | IterStep::Rev
                | IterStep::Take { .. }
                | IterStep::Skip { .. } => {
                    // These don't change the element type.
                }
            }
        }
        ty
    }

    /// Select the correct C runtime function names for vec get/len
    /// based on element type.
    fn vec_get_len_fn_names(elem_ty: &MirType) -> (&'static str, &'static str) {
        match elem_ty {
            MirType::Float(FloatSize::F64) | MirType::Float(FloatSize::F32) => {
                ("build_hvec_get_f64", "build_hvec_len")
            }
            MirType::Int(IntSize::I64, _) | MirType::Int(IntSize::ISize, _) => {
                ("build_hvec_get_i64", "build_hvec_len")
            }
            _ => ("build_hvec_get_i32", "build_hvec_len"),
        }
    }
}
