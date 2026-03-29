// ===============================================================================
// QUANTALANG CODE GENERATOR - MACRO AND CLOSURE LOWERING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
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

        let mut closure_builder = MirBuilder::new(closure_name.clone(), sig);

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
    /// 1. Allocates a `QuantaHandler` on the stack (as a struct local).
    /// 2. Calls `quanta_push_handler(&handler, effect_id)`.
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
        // a `QuantaHandler` declaration for it.  We represent it as an i8 array
        // large enough to hold the C struct (the runtime defines the real type).
        let handler_local = builder.create_named_local(
            format!("__handler_{}", effect_name),
            MirType::Struct(Arc::from("QuantaHandler")),
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

            // quanta_push_handler(&handler, effect_id)
            let push_fn = MirValue::Function(Arc::from("quanta_push_handler"));
            // Take address of the handler struct so the C call gets a pointer.
            let handler_ptr_local = builder.create_local(MirType::Ptr(Box::new(MirType::Struct(
                Arc::from("QuantaHandler"),
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

            // setjmp(handler.env) — pass the handler local directly;
            // the C backend will emit `.env` when it sees a setjmp call
            // with a QuantaHandler-typed argument.
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
            let pop_fn = MirValue::Function(Arc::from("quanta_pop_handler"));
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

                    // Load the argument from handler_data.
                    // The perform site stores a pointer to the argument in handler_data.
                    // We generate a FieldAccess that the C backend renders as:
                    //   msg = *(ParamType*)__handler_EffectName.handler_data
                    let handler_data_field = MirRValue::FieldAccess {
                        base: MirValue::Local(handler_local),
                        field_name: Arc::from("handler_data"),
                        field_ty: MirType::Ptr(Box::new(MirType::Void)),
                    };
                    // Store the void* into a temp, then cast and deref.
                    // Simpler approach: use a special marker that the C backend
                    // can detect. We'll use FieldAccess with field "handler_data"
                    // and let the C backend emit the cast+deref.
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
                let pop_fn = MirValue::Function(Arc::from("quanta_pop_handler"));
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
    /// Generates a call to `quanta_perform(effect_id, op_id, arg_ptr, result_ptr)`
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

        // Lower the first argument (if any) — it becomes the `arg` pointer.
        let arg_val = if let Some(first) = args.first() {
            self.lower_expr(first)?
        } else {
            MirValue::Const(MirConst::Null(MirType::Void))
        };

        // Compute the argument type before borrowing current_fn mutably.
        let arg_ty = self.type_of_value(&arg_val);

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        // Allocate a result slot on the stack.  Use an unnamed local to
        // avoid duplicate C declarations when perform is called multiple
        // times with the same effect/operation.
        let result_local = builder.create_local(MirType::i32());

        // Store the argument value into a local so we can take its address.
        let arg_local = builder.create_local(arg_ty.clone());
        builder.assign(arg_local, MirRValue::Use(arg_val));

        // Take address of arg and result for the void* parameters.
        let arg_ptr = builder.create_local(MirType::Ptr(Box::new(arg_ty)));
        builder.assign(
            arg_ptr,
            MirRValue::AddressOf {
                is_mut: false,
                place: MirPlace::local(arg_local),
            },
        );
        let result_ptr = builder.create_local(MirType::Ptr(Box::new(MirType::i32())));
        builder.assign(
            result_ptr,
            MirRValue::AddressOf {
                is_mut: true,
                place: MirPlace::local(result_local),
            },
        );

        // quanta_perform(effect_id, op_id, &arg, &result)
        let perform_fn = MirValue::Function(Arc::from("quanta_perform"));
        let eid_val = MirValue::Const(MirConst::Int(eid as i128, MirType::i32()));
        let op_val = MirValue::Const(MirConst::Int(op_id as i128, MirType::i32()));

        let cont = builder.create_block();
        builder.call(
            perform_fn,
            vec![
                eid_val,
                op_val,
                MirValue::Local(arg_ptr),
                MirValue::Local(result_ptr),
            ],
            None,
            cont,
        );
        builder.switch_to_block(cont);

        // In practice quanta_perform never returns (it longjmps), but at the MIR
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
        // Extract all argument source texts from the macro tokens.
        // Unlike print macros, vec! has no format string, so we extract
        // ALL tokens as arguments (including the first one).
        let all_args = self.extract_vec_macro_args(tokens);

        if all_args.is_empty() {
            // vec![] with no args -- create empty i32 vec
            let new_fn = MirValue::Function(Arc::from("quanta_hvec_new_i32"));
            let vec_ty = MirType::Vec(Box::new(MirType::i32()));
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
        // We detect this by looking for a semicolon in the raw token list.
        let is_repeat = self.detect_vec_repeat_syntax(tokens);

        if is_repeat && all_args.len() == 2 {
            // vec![val; count] -- repeat form
            let val_src = &all_args[0];
            let count_src = &all_args[1];

            // Parse and lower the value expression to infer the element type
            let val = self.parse_and_lower_macro_arg(val_src)?;
            let elem_ty = self.type_of_value(&val);
            let (new_fn_name, push_fn_name) = Self::vec_fn_names_for_type(&elem_ty);
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
            let count_val = self.parse_and_lower_macro_arg(count_src)?;

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
            // Parse and lower the first argument to infer the element type
            let first_val = self.parse_and_lower_macro_arg(&all_args[0])?;
            let elem_ty = self.type_of_value(&first_val);
            let (new_fn_name, push_fn_name) = Self::vec_fn_names_for_type(&elem_ty);
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
            let push_fn_val = MirValue::Function(Arc::from(push_fn_name));
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

            // Push remaining elements
            for arg_src in &all_args[1..] {
                let val = self.parse_and_lower_macro_arg(arg_src)?;
                let push_fn_val = MirValue::Function(Arc::from(push_fn_name));
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

    /// Select the correct C-level vec_new / vec_push function names based on element type.
    /// Returns the C runtime function names (not the QuantaLang builtin names).
    fn vec_fn_names_for_type(elem_ty: &MirType) -> (&'static str, &'static str) {
        match elem_ty {
            MirType::Float(FloatSize::F64) | MirType::Float(FloatSize::F32) => {
                ("quanta_hvec_new_f64", "quanta_hvec_push_f64")
            }
            MirType::Int(IntSize::I64, _) | MirType::Int(IntSize::ISize, _) => {
                ("quanta_hvec_new_i64", "quanta_hvec_push_i64")
            }
            _ => {
                // Default to i32 for everything else
                ("quanta_hvec_new_i32", "quanta_hvec_push_i32")
            }
        }
    }

    /// Extract all argument source texts from vec! macro tokens.
    /// Unlike print macros, vec! has no format string to skip.
    fn extract_vec_macro_args(&self, tokens: &[ast::TokenTree]) -> Vec<String> {
        use crate::lexer::{Delimiter, TokenKind};

        let source = match self.source {
            Some(ref s) => s,
            None => return Vec::new(),
        };

        // Flatten delimited groups
        let flat: Vec<&ast::TokenTree> = tokens
            .iter()
            .flat_map(|t| match t {
                ast::TokenTree::Delimited { tokens: inner, .. } => inner.iter().collect::<Vec<_>>(),
                other => vec![other],
            })
            .collect();

        let mut args = Vec::new();
        let mut paren_depth: i32 = 0;
        let mut current_arg_start: Option<usize> = None;
        let mut current_arg_end: usize = 0;

        for token in &flat {
            if let ast::TokenTree::Token(tok) = token {
                match &tok.kind {
                    TokenKind::OpenDelim(Delimiter::Paren)
                    | TokenKind::OpenDelim(Delimiter::Bracket) => {
                        paren_depth += 1;
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
                    | TokenKind::CloseDelim(Delimiter::Bracket) => {
                        paren_depth -= 1;
                        let s = tok.span.start.to_usize();
                        let e = tok.span.end.to_usize();
                        if current_arg_start.is_none() {
                            current_arg_start = Some(s);
                        }
                        if e > current_arg_end {
                            current_arg_end = e;
                        }
                    }
                    TokenKind::Comma if paren_depth == 0 => {
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
                    TokenKind::Semi if paren_depth == 0 => {
                        // Semicolon in vec![val; count] -- treat like comma
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

        // Flush remaining
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

    /// Detect whether the token stream contains a semicolon at the top level,
    /// indicating the `vec![val; count]` repeat syntax.
    fn detect_vec_repeat_syntax(&self, tokens: &[ast::TokenTree]) -> bool {
        use crate::lexer::{Delimiter, TokenKind};

        let flat: Vec<&ast::TokenTree> = tokens
            .iter()
            .flat_map(|t| match t {
                ast::TokenTree::Delimited { tokens: inner, .. } => inner.iter().collect::<Vec<_>>(),
                other => vec![other],
            })
            .collect();

        let mut paren_depth: i32 = 0;
        for token in &flat {
            if let ast::TokenTree::Token(tok) = token {
                match &tok.kind {
                    TokenKind::OpenDelim(Delimiter::Paren)
                    | TokenKind::OpenDelim(Delimiter::Bracket) => paren_depth += 1,
                    TokenKind::CloseDelim(Delimiter::Paren)
                    | TokenKind::CloseDelim(Delimiter::Bracket) => paren_depth -= 1,
                    TokenKind::Semi if paren_depth == 0 => return true,
                    _ => {}
                }
            }
        }
        false
    }

    pub(crate) fn lower_print_macro(
        &mut self,
        tokens: &[ast::TokenTree],
        newline: bool,
    ) -> CodegenResult<()> {
        // Extract the format string from the macro tokens.
        let format_str = self.extract_string_from_tokens(tokens);

        // Extract argument source text from tokens and parse + lower each one
        // as a full expression through the normal lowering pipeline.
        let arg_source_texts = self.extract_arg_source_texts(tokens);

        // Parse and lower each argument expression, collecting the MIR values
        // and their resolved types.
        let mut arg_values: Vec<MirValue> = Vec::new();
        let mut arg_types: Vec<Option<MirType>> = Vec::new();

        for arg_src in &arg_source_texts {
            match self.parse_and_lower_macro_arg(arg_src) {
                Ok(val) => {
                    let ty = self.type_of_value(&val);
                    // For QuantaString values, extract .ptr for printf
                    if let MirType::Struct(ref name) = ty {
                        if name.as_ref() == "QuantaString" {
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

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function for macro".into()))?;

        // Create a local for the format string pointer
        let fmt_local = builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
        builder.assign(
            fmt_local,
            MirRValue::Use(MirValue::Const(MirConst::Str(str_idx))),
        );

        // Create a continuation block for after the call
        let continue_block = builder.create_block();

        // Call printf with format string + arguments
        let printf_fn = MirValue::Function(Arc::from("printf"));
        let mut call_args = vec![MirValue::Local(fmt_local)];
        call_args.extend(arg_values);
        builder.call(printf_fn, call_args, None, continue_block);

        // Switch to the continuation block
        builder.switch_to_block(continue_block);

        Ok(())
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

        // Flatten delimited groups to get a flat list of tokens (the macro
        // parser emits flat token sequences, not nested Delimited nodes).
        let flat: Vec<&ast::TokenTree> = tokens
            .iter()
            .flat_map(|t| match t {
                ast::TokenTree::Delimited { tokens: inner, .. } => inner.iter().collect::<Vec<_>>(),
                other => vec![other],
            })
            .collect();

        // Walk the flat token list, tracking parenthesis depth to distinguish
        // top-level argument-separating commas from commas inside function calls.
        let mut args = Vec::new();
        let mut past_format_string = false;
        let mut paren_depth: i32 = 0;
        let mut current_arg_start: Option<usize> = None; // byte offset in source
        let mut current_arg_end: usize = 0;

        for token in &flat {
            if let ast::TokenTree::Token(tok) = token {
                if !past_format_string {
                    if let TokenKind::Literal { kind, .. } = &tok.kind {
                        if matches!(kind, crate::lexer::LiteralKind::Str { .. }) {
                            past_format_string = true;
                        }
                    }
                    continue;
                }

                // Track parenthesis/bracket depth
                match &tok.kind {
                    TokenKind::OpenDelim(Delimiter::Paren)
                    | TokenKind::OpenDelim(Delimiter::Bracket) => {
                        paren_depth += 1;
                        // Extend current arg span
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
                    | TokenKind::CloseDelim(Delimiter::Bracket) => {
                        paren_depth -= 1;
                        let s = tok.span.start.to_usize();
                        let e = tok.span.end.to_usize();
                        if current_arg_start.is_none() {
                            current_arg_start = Some(s);
                        }
                        if e > current_arg_end {
                            current_arg_end = e;
                        }
                    }
                    TokenKind::Comma if paren_depth == 0 => {
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
            Some(MirType::Struct(name)) if name.as_ref() == "QuantaString" => "%s".to_string(),
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
                            // Found the base — `receiver` is the source vec.
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
                        "enumerate" if args.is_empty() => {
                            steps.push(IterStep::Enumerate);
                            current = receiver;
                        }
                        "cloned" if args.is_empty() => {
                            steps.push(IterStep::Cloned);
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

        // 3. Check for enumerate step — affects how many closure params we
        //    bind (index + element vs just element).
        let has_enumerate = chain.steps.iter().any(|s| matches!(s, IterStep::Enumerate));

        // 4. Determine result element type by walking the steps.
        //    After map closures, the type may change based on the closure's
        //    return type annotation.  For now we infer from the closure.
        let output_elem_ty = self.infer_chain_output_type(&elem_ty, &chain.steps);
        let (new_fn_name, push_fn_name) = Self::vec_fn_names_for_type(&output_elem_ty);

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
        };

        // 6. Create the loop: for i in 0..len { ... }
        let idx_local = {
            let builder = self.current_fn.as_mut().unwrap();
            let idx = builder.create_local(MirType::i64());
            builder.assign(
                idx,
                MirRValue::Use(MirValue::Const(MirConst::Int(0, MirType::i64()))),
            );
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

        // Condition: idx < len
        {
            let builder = self.current_fn.as_mut().unwrap();
            let cmp = builder.create_local(MirType::Bool);
            builder.binary_op(
                cmp,
                BinOp::Lt,
                values::local(idx_local),
                values::local(len_local),
            );
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
        for step in &chain.steps {
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
                IterStep::Enumerate => {
                    // enumerate doesn't change the value; it just means
                    // subsequent map closures get (index, elem).  The index
                    // is passed via the idx_local when lowering map closures.
                }
                IterStep::Cloned => {
                    // No-op for Copy types.
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
        }

        // 9. Increment and loop back.
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.goto(incr_block);
            builder.switch_to_block(incr_block);
            let next_idx = builder.create_local(MirType::i64());
            builder.binary_op(
                next_idx,
                BinOp::Add,
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
            // Not a closure — shouldn't happen, but fall back to lowering as-is.
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
                IterStep::Enumerate | IterStep::Cloned => {
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
                ("quanta_hvec_get_f64", "quanta_hvec_len")
            }
            MirType::Int(IntSize::I64, _) | MirType::Int(IntSize::ISize, _) => {
                ("quanta_hvec_get_i64", "quanta_hvec_len")
            }
            _ => ("quanta_hvec_get_i32", "quanta_hvec_len"),
        }
    }
}
