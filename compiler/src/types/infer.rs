// ===============================================================================
// QUANTALANG TYPE SYSTEM - TYPE INFERENCE
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Type inference engine.
//!
//! This module implements bidirectional type inference for expressions.
//! It combines inference (synthesizing types) with checking (verifying against expected types).

use std::sync::Arc;

use crate::ast::{self, AssignOp, BinOp, ExprKind, Literal as AstLiteral, UnaryOp};
use crate::lexer::Span;

use super::context::{ScopeKind, TypeContext, TypeDefKind};
use super::error::*;
use super::traits::{BuiltinTraits, TraitEnv, TraitResolver};
use super::ty::*;
use super::unify::Unifier;

/// Well-known type definition IDs for built-in types.
#[derive(Debug, Clone, Copy)]
pub struct WellKnownTypes {
    /// Range<T>
    pub range: Option<DefId>,
    /// RangeInclusive<T>
    pub range_inclusive: Option<DefId>,
    /// RangeFull
    pub range_full: Option<DefId>,
    /// RangeFrom<T>
    pub range_from: Option<DefId>,
    /// RangeTo<T>
    pub range_to: Option<DefId>,
    /// Option<T>
    pub option: Option<DefId>,
    /// Result<T, E>
    pub result: Option<DefId>,
}

impl Default for WellKnownTypes {
    fn default() -> Self {
        Self {
            range: None,
            range_inclusive: None,
            range_full: None,
            range_from: None,
            range_to: None,
            option: None,
            result: None,
        }
    }
}

/// The type inference engine.
pub struct TypeInfer<'ctx> {
    /// The type context.
    ctx: &'ctx mut TypeContext,
    /// The unifier for type constraints.
    unifier: Unifier,
    /// Collected errors.
    errors: Vec<TypeErrorWithSpan>,
    /// The expected return type of the current function.
    return_ty: Option<Ty>,
    /// Trait environment for resolution.
    trait_env: Option<Arc<TraitEnv>>,
    /// Built-in trait IDs.
    builtin_traits: Option<Arc<BuiltinTraits>>,
    /// Well-known type definitions.
    well_known_types: WellKnownTypes,
    /// Effect context for tracking algebraic effects.
    effect_ctx: super::effects::EffectContext,
    /// The current function's accumulated effect row.
    current_effects: super::effects::EffectRow,
    /// Whether any explicit `return` statement was found.
    has_return: bool,
    /// Borrow tracking state for the current function body.
    borrow_state: super::ty::BorrowState,
}

/// Extract variant names covered by a single pattern.
///
/// Returns `"*"` for wildcard / catch-all patterns (bare ident, `_`).
/// Returns the variant name for path-like patterns (`Some(x)`, `None`).
/// Extract the enum name from a variant pattern.
/// `Color::Red` → `"Color"`, `Shape::Circle(r)` → `"Shape"`.
/// Check if a statement references a variable by name (for NLL dead borrow detection).
fn stmt_mentions_var(stmt: &ast::Stmt, var_name: &str) -> bool {
    match &stmt.kind {
        ast::StmtKind::Expr(expr) | ast::StmtKind::Semi(expr) => expr_mentions_var(expr, var_name),
        ast::StmtKind::Local(local) => {
            if let Some(init) = &local.init {
                expr_mentions_var(&init.expr, var_name)
            } else {
                false
            }
        }
        ast::StmtKind::Item(_) => false,
        ast::StmtKind::Empty => false,
        // Macros may reference any variable — conservatively assume they do.
        // This prevents NLL from releasing borrows too early when a macro
        // like println!("{}", *r) uses a borrowed variable.
        ast::StmtKind::Macro { .. } => true,
    }
}

/// Recursively check if an expression references a variable by name.
fn expr_mentions_var(expr: &ast::Expr, var_name: &str) -> bool {
    match &expr.kind {
        ExprKind::Ident(ident) => ident.name.as_ref() == var_name,
        ExprKind::Unary { expr: inner, .. } => expr_mentions_var(inner, var_name),
        ExprKind::Binary { left, right, .. } => {
            expr_mentions_var(left, var_name) || expr_mentions_var(right, var_name)
        }
        ExprKind::Call { func, args } => {
            expr_mentions_var(func, var_name) || args.iter().any(|a| expr_mentions_var(a, var_name))
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            expr_mentions_var(receiver, var_name)
                || args.iter().any(|a| expr_mentions_var(a, var_name))
        }
        ExprKind::Field { expr: inner, .. } => expr_mentions_var(inner, var_name),
        ExprKind::Index { expr: inner, index } => {
            expr_mentions_var(inner, var_name) || expr_mentions_var(index, var_name)
        }
        ExprKind::Ref { expr: inner, .. } => expr_mentions_var(inner, var_name),
        ExprKind::Deref(inner) => expr_mentions_var(inner, var_name),
        ExprKind::Block(block) => block.stmts.iter().any(|s| stmt_mentions_var(s, var_name)),
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_mentions_var(condition, var_name)
                || then_branch
                    .stmts
                    .iter()
                    .any(|s| stmt_mentions_var(s, var_name))
                || else_branch
                    .as_ref()
                    .map_or(false, |e| expr_mentions_var(e, var_name))
        }
        ExprKind::Return(Some(inner)) => expr_mentions_var(inner, var_name),
        ExprKind::Assign { target, value, .. } => {
            expr_mentions_var(target, var_name) || expr_mentions_var(value, var_name)
        }
        ExprKind::Tuple(elems) | ExprKind::Array(elems) => {
            elems.iter().any(|e| expr_mentions_var(e, var_name))
        }
        // Macro calls may reference any variable — conservatively assume yes
        ExprKind::Macro { .. } => true,
        _ => false,
    }
}

fn extract_enum_name_from_pattern(pattern: &ast::Pattern) -> Option<String> {
    match &pattern.kind {
        ast::PatternKind::TupleStruct { path, .. }
        | ast::PatternKind::Path(path)
        | ast::PatternKind::Struct { path, .. } => {
            if path.segments.len() >= 2 {
                Some(
                    path.segments[path.segments.len() - 2]
                        .ident
                        .name
                        .to_string(),
                )
            } else {
                None
            }
        }
        ast::PatternKind::Or(patterns) => patterns.iter().find_map(extract_enum_name_from_pattern),
        _ => None,
    }
}

fn extract_covered_variants(pattern: &ast::Pattern) -> Vec<String> {
    match &pattern.kind {
        ast::PatternKind::TupleStruct { path, .. } => {
            if let Some(ident) = path.last_ident() {
                vec![ident.name.to_string()]
            } else {
                vec![]
            }
        }
        ast::PatternKind::Path(path) => {
            if let Some(ident) = path.last_ident() {
                vec![ident.name.to_string()]
            } else {
                vec![]
            }
        }
        ast::PatternKind::Struct { path, .. } => {
            if let Some(ident) = path.last_ident() {
                vec![ident.name.to_string()]
            } else {
                vec![]
            }
        }
        ast::PatternKind::Ident { .. } => {
            // A bare identifier without a subpattern is a catch-all binding.
            vec!["*".to_string()]
        }
        ast::PatternKind::Wildcard => vec!["*".to_string()],
        ast::PatternKind::Or(patterns) => {
            patterns.iter().flat_map(extract_covered_variants).collect()
        }
        _ => vec![],
    }
}

impl<'ctx> TypeInfer<'ctx> {
    /// Create a new type inference engine.
    pub fn new(ctx: &'ctx mut TypeContext) -> Self {
        Self {
            ctx,
            unifier: Unifier::new(),
            errors: Vec::new(),
            return_ty: None,
            trait_env: None,
            builtin_traits: None,
            well_known_types: WellKnownTypes::default(),
            effect_ctx: super::effects::EffectContext::new(),
            current_effects: super::effects::EffectRow::empty(),
            has_return: false,
            borrow_state: super::ty::BorrowState::new(),
        }
    }

    /// Create a new type inference engine with trait resolution support.
    pub fn with_traits(
        ctx: &'ctx mut TypeContext,
        trait_env: Arc<TraitEnv>,
        builtin_traits: Arc<BuiltinTraits>,
    ) -> Self {
        Self {
            ctx,
            unifier: Unifier::new(),
            errors: Vec::new(),
            return_ty: None,
            trait_env: Some(trait_env),
            builtin_traits: Some(builtin_traits),
            well_known_types: WellKnownTypes::default(),
            effect_ctx: super::effects::EffectContext::new(),
            current_effects: super::effects::EffectRow::empty(),
            has_return: false,
            borrow_state: super::ty::BorrowState::new(),
        }
    }

    /// Get the current accumulated effect row.
    pub fn current_effect_row(&self) -> &super::effects::EffectRow {
        &self.current_effects
    }

    /// Get a mutable reference to the effect context.
    pub fn effect_ctx_mut(&mut self) -> &mut super::effects::EffectContext {
        &mut self.effect_ctx
    }

    /// Register a user-defined effect so that `perform` and `handle` can
    /// resolve it during inference.
    pub fn register_effect(&mut self, effect: super::effects::EffectDef) {
        self.effect_ctx.register_effect(effect);
    }

    /// Set the expected return type for the current function context.
    ///
    /// When set, `infer_return` will unify the returned expression's type
    /// with this expected type, catching type mismatches inside nested
    /// control flow (e.g. `return` inside `while { if { ... } }`).
    pub fn set_return_ty(&mut self, ty: Ty) {
        self.return_ty = Some(ty);
    }

    /// Check if any `return` statement was encountered during inference.
    pub fn has_explicit_return(&self) -> bool {
        self.has_return
    }

    /// Set well-known type definitions.
    pub fn set_well_known_types(&mut self, types: WellKnownTypes) {
        self.well_known_types = types;
    }

    /// Create a Future<Output = T> type.
    fn make_future_type(&self, output: Ty) -> Ty {
        // Future is represented as a projection type: impl Future<Output = T>
        Ty::new(TyKind::Projection {
            trait_ref: Arc::from("Future"),
            item: Arc::from("Output"),
            self_ty: Box::new(Ty::fresh_var()),
            substs: vec![output],
        })
    }

    /// Create a Range<T> type.
    fn make_range_type(&self, elem_ty: Ty, inclusive: bool) -> Ty {
        let def_id = if inclusive {
            self.well_known_types.range_inclusive
        } else {
            self.well_known_types.range
        };

        if let Some(def_id) = def_id {
            Ty::adt(def_id, vec![elem_ty])
        } else {
            // Fallback: return a fresh variable with the element type constraint
            Ty::fresh_var()
        }
    }

    /// Resolve the Iterator::Item associated type.
    fn resolve_iterator_item(&self, iter_ty: &Ty) -> Ty {
        if let (Some(ref env), Some(ref builtins)) = (&self.trait_env, &self.builtin_traits) {
            let mut resolver = TraitResolver::new(env);
            if let Ok(item_ty) = resolver.resolve_assoc_type(iter_ty, builtins.iterator, "Item") {
                return item_ty;
            }
            // Try IntoIterator
            if let Ok(item_ty) =
                resolver.resolve_assoc_type(iter_ty, builtins.into_iterator, "Item")
            {
                return item_ty;
            }
        }
        // Fallback: infer from common patterns
        match &iter_ty.kind {
            TyKind::Array(elem, _) | TyKind::Slice(elem) => {
                // Iterating over array/slice yields references to elements
                Ty::reference(None, Mutability::Immutable, (**elem).clone())
            }
            TyKind::Ref(_, _, inner) => {
                // Iterating over &[T] or &Vec<T>
                match &inner.kind {
                    TyKind::Array(elem, _) | TyKind::Slice(elem) => {
                        Ty::reference(None, Mutability::Immutable, (**elem).clone())
                    }
                    _ => Ty::fresh_var(),
                }
            }
            _ => Ty::fresh_var(),
        }
    }

    /// Resolve the Try::Output associated type.
    fn resolve_try_output(&self, try_ty: &Ty) -> Ty {
        if let (Some(ref env), Some(ref builtins)) = (&self.trait_env, &self.builtin_traits) {
            let mut resolver = TraitResolver::new(env);
            if let Ok(output_ty) = resolver.resolve_assoc_type(try_ty, builtins.try_, "Output") {
                return output_ty;
            }
        }
        // Fallback: infer from common patterns (Result<T, E>, Option<T>)
        match &try_ty.kind {
            TyKind::Adt(def_id, substs) => {
                // Check if this is Option<T> or Result<T, E>
                if Some(*def_id) == self.well_known_types.option && !substs.is_empty() {
                    return substs[0].clone();
                }
                if Some(*def_id) == self.well_known_types.result && !substs.is_empty() {
                    return substs[0].clone();
                }
                Ty::fresh_var()
            }
            _ => Ty::fresh_var(),
        }
    }

    /// Resolve the Future::Output associated type.
    fn resolve_future_output(&self, future_ty: &Ty) -> Ty {
        if let (Some(ref env), Some(ref builtins)) = (&self.trait_env, &self.builtin_traits) {
            let mut resolver = TraitResolver::new(env);
            if let Ok(output_ty) = resolver.resolve_assoc_type(future_ty, builtins.future, "Output")
            {
                return output_ty;
            }
        }
        // Fallback: return fresh variable
        Ty::fresh_var()
    }

    /// Look up a method on a type.
    fn lookup_method(&self, ty: &Ty, method_name: &str) -> Option<Ty> {
        if let Some(ref env) = self.trait_env {
            let mut resolver = TraitResolver::new(env);
            if let Some((_, method_def)) = resolver.resolve_method(ty, method_name) {
                // Build function type from method signature
                let params = method_def.sig.params.clone();
                let ret = method_def.sig.return_ty.clone();
                return Some(Ty::function(params, ret));
            }
        }

        // Fallback: look up methods from trait impls registered in the TypeContext.
        // This handles `impl Trait for Type` when the full TraitEnv isn't available.
        if let Some(method) = self.ctx.lookup_trait_method(ty, method_name) {
            // Skip the first parameter (self) since method call inference
            // only counts the explicit arguments, not the receiver.
            let params: Vec<Ty> = method
                .sig
                .params
                .iter()
                .skip_while(|(name, _)| name.as_ref() == "self")
                .map(|(_, ty)| ty.clone())
                .collect();
            let ret = method.sig.ret.clone();
            return Some(Ty::function(params, ret));
        }

        // Look up inherent methods (impl Type { fn method(...) } without a trait).
        // Extract the type name and generic substitutions from the receiver.
        let (type_def_id, adt_substs) = match &ty.kind {
            TyKind::Adt(def_id, substs) => (Some(*def_id), substs.clone()),
            TyKind::Ref(_, _, inner) => {
                if let TyKind::Adt(def_id, substs) = &inner.kind {
                    (Some(*def_id), substs.clone())
                } else {
                    (None, vec![])
                }
            }
            _ => (None, vec![]),
        };
        if let Some(def_id) = type_def_id {
            if let Some(method) = self.ctx.lookup_inherent_method(def_id, method_name) {
                // If receiver has concrete substs (e.g. Foo<i32>), substitute params.
                // Otherwise, freshen params to fresh vars so the unifier can solve them.
                let apply = |ty: &Ty| -> Ty {
                    if !adt_substs.is_empty() {
                        ty.substitute_params(&adt_substs)
                    } else {
                        ty.freshen_params()
                    }
                };
                let params: Vec<Ty> = method
                    .sig
                    .params
                    .iter()
                    .skip_while(|(name, _)| name.as_ref() == "self")
                    .map(|(_, ty)| apply(ty))
                    .collect();
                let ret = apply(&method.sig.ret);
                return Some(Ty::function(params, ret));
            }
        }

        // Look up methods on type parameters through their trait bounds.
        // For `fn foo<T: Ord + Display>(x: T)`, calling `x.cmp(other)` should
        // resolve through the `Ord` trait's method definitions.
        if let TyKind::Param(ref param_name, _param_idx) = ty.kind {
            if let Some(method) = self.ctx.lookup_param_method(param_name, method_name) {
                let receiver_ty = ty.clone();
                let params: Vec<Ty> = method
                    .sig
                    .params
                    .iter()
                    .skip_while(|(name, _)| name.as_ref() == "self")
                    .map(|(_, pty)| {
                        // Substitute Self-like fresh variables with the receiver type.
                        // In trait defs, Self becomes a fresh var; we replace it with T.
                        if matches!(&pty.kind, TyKind::Var(_)) {
                            receiver_ty.clone()
                        } else {
                            pty.clone()
                        }
                    })
                    .collect();
                // For the return type: if it's a fresh variable (from Self in the
                // trait def), substitute with the receiver's param type T.
                // This ensures `T.scale(factor) -> Self` returns T, not ?Tn.
                let ret = if matches!(&method.sig.ret.kind, TyKind::Var(_)) {
                    receiver_ty
                } else {
                    method.sig.ret.clone()
                };
                return Some(Ty::function(params, ret));
            }
        }

        None
    }

    /// Evaluate a const expression to get its value (for array sizes, etc.)
    fn eval_const_expr(&self, expr: &ast::Expr) -> Option<usize> {
        match &expr.kind {
            ExprKind::Literal(lit) => match lit {
                AstLiteral::Int { value, .. } => {
                    // Convert the integer value to usize
                    (*value).try_into().ok()
                }
                _ => None,
            },
            ExprKind::Ident(ident) => {
                // Check if it's a const variable in scope
                // For now, we don't support const lookup
                let _ = ident;
                None
            }
            ExprKind::Binary { op, left, right } => {
                let left_val = self.eval_const_expr(left)?;
                let right_val = self.eval_const_expr(right)?;
                match op {
                    BinOp::Add => Some(left_val + right_val),
                    BinOp::Sub => left_val.checked_sub(right_val),
                    BinOp::Mul => Some(left_val * right_val),
                    BinOp::Div => {
                        if right_val != 0 {
                            Some(left_val / right_val)
                        } else {
                            None
                        }
                    }
                    BinOp::Rem => {
                        if right_val != 0 {
                            Some(left_val % right_val)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            ExprKind::Paren(inner) => self.eval_const_expr(inner),
            _ => None,
        }
    }

    /// Get collected errors.
    pub fn errors(&self) -> &[TypeErrorWithSpan] {
        &self.errors
    }

    /// Take collected errors.
    pub fn take_errors(&mut self) -> Vec<TypeErrorWithSpan> {
        std::mem::take(&mut self.errors)
    }

    /// Report an error.
    fn error(&mut self, error: TypeError, span: Span) {
        self.errors.push(TypeErrorWithSpan::new(error, span));
    }

    /// Apply the current substitution to a type.
    pub fn apply(&self, ty: &Ty) -> Ty {
        self.unifier.apply(ty)
    }

    /// Unify two types.
    fn unify(&mut self, t1: &Ty, t2: &Ty, span: Span) -> TypeResult<()> {
        match self.unifier.unify(t1, t2) {
            Ok(()) => Ok(()),
            Err(TypeError::TypeMismatch {
                ref expected,
                ref found,
            }) => {
                // When ADT types mismatch by DefId, check if they match by name.
                // This handles cases where the same struct gets different DefIds
                // due to registration order (e.g., vec3 from builtins vs user code).
                if let (TyKind::Adt(d1, _), TyKind::Adt(d2, _)) = (&expected.kind, &found.kind) {
                    if d1 != d2 {
                        let name1 = self.ctx.lookup_type(*d1).map(|t| t.name.clone());
                        let name2 = self.ctx.lookup_type(*d2).map(|t| t.name.clone());
                        if name1.is_some() && name1 == name2 {
                            return Ok(()); // Same type by name — allow it
                        }
                    }
                }
                let e = TypeError::TypeMismatch {
                    expected: expected.clone(),
                    found: found.clone(),
                };
                self.error(e.clone(), span);
                Err(e)
            }
            Err(e) => {
                self.error(e.clone(), span);
                Err(e)
            }
        }
    }

    // =========================================================================
    // EXPRESSION INFERENCE
    // =========================================================================

    /// Infer the type of an expression.
    pub fn infer_expr(&mut self, expr: &ast::Expr) -> Ty {
        match &expr.kind {
            ExprKind::Literal(lit) => self.infer_literal(lit),
            ExprKind::Ident(ident) => self.infer_ident(ident, expr.span),
            ExprKind::Path(path) => self.infer_path(path, expr.span),

            ExprKind::Tuple(elems) => self.infer_tuple(elems),
            ExprKind::Array(elems) => self.infer_array(elems, expr.span),
            ExprKind::ArrayRepeat { element, count } => {
                self.infer_array_repeat(element, count, expr.span)
            }

            ExprKind::Unary { op, expr: inner } => self.infer_unary(*op, inner),
            ExprKind::Binary { op, left, right } => self.infer_binary(*op, left, right, expr.span),
            ExprKind::Assign { op, target, value } => {
                self.infer_assign(*op, target, value, expr.span)
            }

            ExprKind::Field { expr: inner, field } => self.infer_field(inner, field, expr.span),
            ExprKind::TupleField {
                expr: inner, index, ..
            } => self.infer_tuple_field(inner, *index, expr.span),
            ExprKind::Index { expr: inner, index } => self.infer_index(inner, index, expr.span),

            ExprKind::Call { func, args } => self.infer_call(func, args, expr.span),
            ExprKind::MethodCall {
                receiver,
                method,
                generics: _,
                args,
            } => self.infer_method_call(receiver, method, args, expr.span),

            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => self.infer_if(condition, then_branch, else_branch.as_deref(), expr.span),
            ExprKind::IfLet {
                pattern,
                expr: scrutinee,
                then_branch,
                else_branch,
            } => {
                // if let Pattern = expr { then } else { else }
                let scrutinee_ty = self.infer_expr(scrutinee);
                self.check_pattern(pattern, &scrutinee_ty);
                let then_ty = self.infer_block(then_branch);
                if let Some(else_expr) = else_branch.as_deref() {
                    let else_ty = self.infer_expr(else_expr);
                    let _ = self.unify(&then_ty, &else_ty, expr.span);
                    self.apply(&then_ty)
                } else {
                    Ty::unit()
                }
            }
            ExprKind::Match { scrutinee, arms } => self.infer_match(scrutinee, arms, expr.span),
            ExprKind::Loop { body, .. } => self.infer_loop(body),
            ExprKind::While {
                condition, body, ..
            } => self.infer_while(condition, body),
            ExprKind::For {
                pattern,
                iter,
                body,
                ..
            } => self.infer_for(pattern, iter, body, expr.span),

            ExprKind::Block(block) => self.infer_block(block),
            ExprKind::Unsafe(block) => self.infer_block(block),
            ExprKind::Async { body, .. } => {
                // Async blocks return impl Future<Output = T>
                let body_ty = self.infer_block(body);
                // Wrap the body type in a Future projection
                self.make_future_type(body_ty)
            }

            ExprKind::Return(value) => self.infer_return(value.as_deref(), expr.span),
            ExprKind::Break { value, .. } => self.infer_break(value.as_deref(), expr.span),
            ExprKind::Continue { .. } => self.infer_continue(expr.span),

            ExprKind::Closure {
                params,
                return_type,
                body,
                ..
            } => self.infer_closure(params, return_type.as_deref(), body, expr.span),

            ExprKind::Cast { expr: inner, ty } => self.infer_cast(inner, ty, expr.span),
            ExprKind::Ref {
                mutability,
                expr: inner,
            } => self.infer_ref(*mutability, inner),
            ExprKind::Deref(inner) => self.infer_deref(inner, expr.span),

            ExprKind::Range {
                start,
                end,
                inclusive,
            } => self.infer_range(start.as_deref(), end.as_deref(), *inclusive, expr.span),

            ExprKind::Try(inner) => self.infer_try(inner, expr.span),
            ExprKind::Await(inner) => self.infer_await(inner, expr.span),

            ExprKind::Struct { path, fields, rest } => {
                self.infer_struct(path, fields, rest.as_deref(), expr.span)
            }

            ExprKind::Paren(inner) => self.infer_expr(inner),
            ExprKind::Error => Ty::error(),

            // While let loops
            ExprKind::WhileLet {
                pattern,
                expr,
                body,
                ..
            } => {
                let expr_ty = self.infer_expr(expr);
                self.ctx.push_scope(ScopeKind::Loop);
                self.check_pattern(pattern, &expr_ty);
                let _ = self.infer_block(body);
                self.ctx.pop_scope();
                Ty::unit()
            }

            // Type ascription: `expr: Type`
            ExprKind::TypeAscription { expr: inner, ty } => {
                let inferred = self.infer_expr(inner);
                let expected = self.lower_type(ty);
                let _ = self.unify(&inferred, &expected, expr.span);
                self.apply(&expected)
            }

            // Macro invocations - return fresh var (macro expansion happens earlier)
            ExprKind::Macro { .. } => Ty::fresh_var(),

            // QuantaLang AI extensions
            ExprKind::AIQuery { prompt, options } => {
                // AI queries return a String or structured response
                let _ = self.infer_expr(prompt);
                for (_, opt_expr) in options {
                    let _ = self.infer_expr(opt_expr);
                }
                Ty::str()
            }

            ExprKind::AIInfer { expr: inner, ty } => {
                // AI inference attempts to produce a value of the given type
                let _ = self.infer_expr(inner);
                self.lower_type(ty)
            }

            // Effect handling
            ExprKind::Handle {
                effect,
                body,
                handlers,
            } => self.infer_handle(effect, body, handlers, expr.span),

            ExprKind::Resume(value) => {
                // Resume transfers control back to the handler's continuation.
                // The value passed to resume must match the operation's return type.
                if let Some(val) = value {
                    let _ = self.infer_expr(val);
                }
                // Resume itself diverges from the handler clause's perspective.
                Ty::never()
            }

            ExprKind::Perform {
                effect,
                operation,
                args,
            } => self.infer_perform(effect, operation, args, expr.span),
        }
    }

    /// Check an expression against an expected type.
    pub fn check_expr(&mut self, expr: &ast::Expr, expected: &Ty) -> Ty {
        let inferred = self.infer_expr(expr);
        if let Err(_) = self.unify(&inferred, expected, expr.span) {
            // Error already recorded
        }
        self.apply(&inferred)
    }

    // =========================================================================
    // LITERAL INFERENCE
    // =========================================================================

    fn infer_literal(&mut self, lit: &AstLiteral) -> Ty {
        match lit {
            AstLiteral::Int { suffix, .. } => {
                if let Some(suffix) = suffix {
                    match suffix {
                        ast::IntSuffix::I8 => Ty::int(IntTy::I8),
                        ast::IntSuffix::I16 => Ty::int(IntTy::I16),
                        ast::IntSuffix::I32 => Ty::int(IntTy::I32),
                        ast::IntSuffix::I64 => Ty::int(IntTy::I64),
                        ast::IntSuffix::I128 => Ty::int(IntTy::I128),
                        ast::IntSuffix::Isize => Ty::int(IntTy::Isize),
                        ast::IntSuffix::U8 => Ty::int(IntTy::U8),
                        ast::IntSuffix::U16 => Ty::int(IntTy::U16),
                        ast::IntSuffix::U32 => Ty::int(IntTy::U32),
                        ast::IntSuffix::U64 => Ty::int(IntTy::U64),
                        ast::IntSuffix::U128 => Ty::int(IntTy::U128),
                        ast::IntSuffix::Usize => Ty::int(IntTy::Usize),
                    }
                } else {
                    // Integer literal without suffix - create inference variable
                    Ty::new(TyKind::Infer(InferTy {
                        var: TyVarId::fresh(),
                        kind: InferKind::Int,
                    }))
                }
            }
            AstLiteral::Float { suffix, .. } => {
                if let Some(suffix) = suffix {
                    match suffix {
                        ast::FloatSuffix::F16 => Ty::float(FloatTy::F32), // Map f16 to f32
                        ast::FloatSuffix::F32 => Ty::float(FloatTy::F32),
                        ast::FloatSuffix::F64 => Ty::float(FloatTy::F64),
                    }
                } else {
                    // Float literal without suffix - create inference variable
                    Ty::new(TyKind::Infer(InferTy {
                        var: TyVarId::fresh(),
                        kind: InferKind::Float,
                    }))
                }
            }
            AstLiteral::Bool(_) => Ty::bool(),
            AstLiteral::Char(_) => Ty::char(),
            AstLiteral::Byte(_) => Ty::int(IntTy::U8),
            AstLiteral::Str { .. } => {
                // String literals have type str (owned).
                // At the C level all strings are QuantaString, so using the
                // owned str type lets method calls like char_at, substring,
                // contains, etc. pass type-checking without a workaround.
                Ty::str()
            }
            AstLiteral::ByteStr { value, .. } => {
                // Byte string literals have type &'static [u8; N]
                Ty::reference(
                    Some(Lifetime::static_lifetime()),
                    Mutability::Immutable,
                    Ty::array(Ty::int(IntTy::U8), value.len()),
                )
            }
        }
    }

    // =========================================================================
    // IDENTIFIER AND PATH INFERENCE
    // =========================================================================

    fn infer_ident(&mut self, ident: &ast::Ident, span: Span) -> Ty {
        if let Some(ty) = self.ctx.lookup_var(ident.name.as_ref()) {
            ty
        } else {
            // Check if this is a known builtin (math functions, vector constructors, I/O)
            let name = ident.name.as_ref();
            let is_builtin = matches!(
                name,
                // Math builtins
                "sqrt" | "sin" | "cos" | "tan" | "pow" | "abs" |
                "sinh" | "cosh" | "tanh" | "asin" | "acos" | "atan" |
                "log" | "log2" | "log10" | "exp" | "exp2" | "atan2" |
                "floor" | "ceil" | "round" | "min" | "max" |
                // Vector constructors
                "vec2" | "vec3" | "vec4" | "mat4" |
                // Vector math builtins
                "dot" | "cross" | "normalize" | "length" | "reflect" | "lerp" |
                // Matrix builtins
                "mat4_identity" | "mat4_translate" | "mat4_scale" | "mat4_perspective" |
                // Shader math builtins
                "clamp" | "smoothstep" | "mix" | "fract" | "step" |
                // Texture sampling builtins
                "texture_sample" | "tex2d" | "tex2d_depth" |
                // I/O builtins
                "read_file" | "write_file" | "file_exists" | "exit" |
                // CLI / stdin builtins
                "args_count" | "args_get" |
                "read_line" | "read_all" | "stdin_is_pipe" |
                // Process builtins
                "process_exit" |
                // Directory traversal builtins
                "list_dir" | "is_dir" | "file_size" |
                // String vec builtins
                "vec_new_str" | "vec_push_str" | "vec_get_str" |
                // TCP socket builtins
                "tcp_connect" | "tcp_send" | "tcp_recv" | "tcp_close" |
                // Environment variable builtins
                "getenv" |
                // Clock / time builtins
                "clock_ms" | "time_unix" |
                // Vec builtins
                "vec_new" | "vec_push" | "vec_get" | "vec_len" | "vec_pop" |
                "vec_new_f64" | "vec_push_f64" | "vec_get_f64" | "vec_pop_f64" |
                "vec_new_i64" | "vec_push_i64" | "vec_get_i64" | "vec_pop_i64" |
                // Format builtins
                "to_string_i32" | "to_string_f64" |
                // HashMap builtins
                "map_new" | "map_insert" | "map_get" | "map_contains" | "map_len" | "map_remove" |
                // Vulkan runtime builtins
                "quanta_vk_init" | "quanta_vk_load_shader_file" | "quanta_vk_run_compute" | "quanta_vk_shutdown" |
                // Math constants
                "PI" | "E" | "TAU"
            );
            if is_builtin {
                // Return a generic function type — the lowerer handles the actual dispatch
                Ty::error() // Silently accept; codegen resolves these
            } else {
                self.error(
                    TypeError::UndefinedVariable {
                        name: ident.name.to_string(),
                    },
                    span,
                );
                Ty::error()
            }
        }
    }

    fn infer_path(&mut self, path: &ast::Path, span: Span) -> Ty {
        // For simple paths, treat as variable lookup
        if path.is_simple() {
            if let Some(ident) = path.last_ident() {
                return self.infer_ident(ident, span);
            }
        }

        // Handle qualified paths (e.g., std::vec::Vec, Type::method)

        // Handle standard library paths (std::iter::repeat, std::mem::replace, etc.)
        let segments: Vec<&str> = path
            .segments
            .iter()
            .map(|s| s.ident.name.as_ref())
            .collect();
        if segments.len() >= 3 && segments[0] == "std" {
            let full_path = segments.join("::");
            match full_path.as_str() {
                "std::iter::repeat" => {
                    // repeat(value) -> impl Iterator<Item=T>
                    return Ty::function(vec![Ty::fresh_var()], Ty::fresh_var());
                }
                "std::mem::replace" => {
                    // replace(&mut T, T) -> T
                    let t = Ty::fresh_var();
                    return Ty::function(vec![t.clone(), t.clone()], t);
                }
                "std::mem::swap" => {
                    return Ty::function(vec![Ty::fresh_var(), Ty::fresh_var()], Ty::unit());
                }
                "std::mem::size_of" | "std::mem::align_of" => {
                    return Ty::function(vec![], Ty::int(IntTy::Usize));
                }
                "std::mem::drop" | "std::mem::forget" => {
                    return Ty::function(vec![Ty::fresh_var()], Ty::unit());
                }
                "std::ptr::null" | "std::ptr::null_mut" => {
                    return Ty::fresh_var();
                }
                "std::ptr::write"
                | "std::ptr::read"
                | "std::ptr::copy"
                | "std::ptr::copy_nonoverlapping" => {
                    return Ty::function(vec![Ty::fresh_var(), Ty::fresh_var()], Ty::unit());
                }
                "std::cmp::min" | "std::cmp::max" => {
                    let t = Ty::fresh_var();
                    return Ty::function(vec![t.clone(), t.clone()], t);
                }
                "std::cmp::Ordering::Less"
                | "std::cmp::Ordering::Equal"
                | "std::cmp::Ordering::Greater" => {
                    return Ty::fresh_var();
                }
                _ => {
                    // Unknown std:: path — return fresh var
                    return Ty::fresh_var();
                }
            }
        }

        // Check for associated function: Type::func (2-segment path)
        if segments.len() == 2 {
            let mut type_name = segments[0];
            let func_name = segments[1];

            // Resolve "Self" to the actual type name in the current impl context
            let resolved_name;
            if type_name == "Self" {
                if let Some(self_ty) = self.ctx.get_self_ty() {
                    if let TyKind::Adt(def_id, _) = &self_ty.kind {
                        if let Some(td) = self.ctx.lookup_type(*def_id) {
                            resolved_name = td.name.to_string();
                            type_name = &resolved_name;
                        }
                    }
                }
            }

            // Handle well-known type constants (f64::INFINITY, f32::NAN, etc.)
            match (type_name, func_name) {
                (
                    "f64",
                    "INFINITY" | "NEG_INFINITY" | "NAN" | "MIN" | "MAX" | "MIN_POSITIVE"
                    | "EPSILON",
                ) => {
                    return Ty::float(FloatTy::F64);
                }
                (
                    "f32",
                    "INFINITY" | "NEG_INFINITY" | "NAN" | "MIN" | "MAX" | "MIN_POSITIVE"
                    | "EPSILON",
                ) => {
                    return Ty::float(FloatTy::F32);
                }
                ("i32", "MIN" | "MAX") => return Ty::int(IntTy::I32),
                ("i64", "MIN" | "MAX") => return Ty::int(IntTy::I64),
                ("u32", "MIN" | "MAX") => return Ty::int(IntTy::U32),
                ("u64", "MIN" | "MAX") => return Ty::int(IntTy::U64),
                ("usize", "MIN" | "MAX") => return Ty::int(IntTy::Usize),
                ("isize", "MIN" | "MAX") => return Ty::int(IntTy::Isize),
                _ => {}
            }

            // Look up as an inherent method/associated function (by type name → DefId)
            if let Some(method) = self.ctx.lookup_inherent_method_by_name(type_name, func_name) {
                // Freshen generic params so the unifier can solve them from context.
                // e.g., SimpleMap::new() returns SimpleMap<K,V> → SimpleMap<?0,?1>
                let param_tys: Vec<Ty> = method
                    .sig
                    .params
                    .iter()
                    .map(|(_, ty)| ty.freshen_params())
                    .collect();
                return Ty::function(param_tys, method.sig.ret.freshen_params());
            }

            // For module-qualified paths (e.g., convert::xyz_to_lab), the first
            // segment is a module name, not a type.  Looking up just the bare
            // function name via `lookup_var_scheme` can resolve to a DIFFERENT
            // overload (e.g., a local 1-param `xyz_to_lab` shadows the module's
            // 2-param version).  Only fall back to bare-name lookup when the
            // first segment is a known type (Type::method pattern).
            let first_is_type = self.ctx.lookup_type_by_name(type_name).is_some()
                || self.ctx.lookup_inherent_method_by_name(type_name, "new").is_some();
            if first_is_type {
                if let Some(scheme) = self.ctx.lookup_var_scheme(func_name) {
                    return scheme.instantiate();
                }
            } else {
                // Module-qualified call: return a fresh type variable so that
                // type inference resolves it from the call-site arguments
                // rather than accidentally picking a wrong local overload.
                return Ty::fresh_var();
            }
        }

        // Try to resolve the last segment as a type or function
        if let Some(last) = path.last_ident() {
            let name = last.name.as_ref();

            // Check if it's a type in the context
            if let Some(type_def) = self.ctx.lookup_type_by_name(name) {
                let def_id = type_def.def_id;
                // Return the type with fresh type parameters if generic
                let substs: Vec<Ty> = type_def.generics.iter().map(|_| Ty::fresh_var()).collect();
                return Ty::adt(def_id, substs);
            }

            // Check if it's a static/const function
            if let Some(scheme) = self.ctx.lookup_var_scheme(name) {
                return scheme.instantiate();
            }
        }

        // Fallback: return fresh variable (will be constrained by usage)
        Ty::fresh_var()
    }

    // =========================================================================
    // COMPOUND EXPRESSION INFERENCE
    // =========================================================================

    fn infer_tuple(&mut self, elems: &[ast::Expr]) -> Ty {
        let elem_tys: Vec<_> = elems.iter().map(|e| self.infer_expr(e)).collect();
        Ty::tuple(elem_tys)
    }

    fn infer_array(&mut self, elems: &[ast::Expr], span: Span) -> Ty {
        if elems.is_empty() {
            // Empty array - need element type from context
            let elem_ty = Ty::fresh_var();
            return Ty::array(elem_ty, 0);
        }

        let first_ty = self.infer_expr(&elems[0]);
        for elem in &elems[1..] {
            let elem_ty = self.infer_expr(elem);
            let _ = self.unify(&first_ty, &elem_ty, span);
        }

        Ty::array(self.apply(&first_ty), elems.len())
    }

    fn infer_array_repeat(&mut self, element: &ast::Expr, count: &ast::Expr, span: Span) -> Ty {
        let elem_ty = self.infer_expr(element);

        // Evaluate count as const expression
        let size = self.eval_const_expr(count).unwrap_or_else(|| {
            // If we can't evaluate it, check that the count has an integer type
            let count_ty = self.infer_expr(count);
            let _ = self.unify(&count_ty, &Ty::int(IntTy::Usize), span);
            0 // Placeholder for unknown const
        });

        Ty::array(elem_ty, size)
    }

    // =========================================================================
    // OPERATOR INFERENCE
    // =========================================================================

    fn infer_unary(&mut self, op: UnaryOp, expr: &ast::Expr) -> Ty {
        let inner_ty = self.infer_expr(expr);

        match op {
            UnaryOp::Neg => {
                // Negation works on numeric types
                inner_ty
            }
            UnaryOp::Not => {
                // Logical not on bool, bitwise not on integers
                inner_ty
            }
            UnaryOp::BitNot => {
                // Bitwise not on integers
                inner_ty
            }
            UnaryOp::Deref => {
                // Dereference: *ref → T (strip one reference layer)
                let resolved = self.apply(&inner_ty);
                match &resolved.kind {
                    TyKind::Ref(_, _, pointee) => (**pointee).clone(),
                    TyKind::Ptr(_, pointee) => (**pointee).clone(),
                    // If the type is an unresolved variable, return a fresh var
                    // that will be constrained later through unification
                    TyKind::Var(_) | TyKind::Infer(_) => Ty::fresh_var(),
                    // Dereferencing a non-reference type is an error
                    _ => {
                        self.error(
                            TypeError::NotDereferenceable {
                                ty: resolved.clone(),
                            },
                            expr.span,
                        );
                        Ty::error()
                    }
                }
            }
            UnaryOp::Ref => {
                // Reference: &expr → &T where T = typeof(expr)
                // Track the borrow if the inner expression is a variable
                if let ExprKind::Ident(ident) = &expr.kind {
                    let var_name = ident.name.as_ref();
                    // Cannot take shared reference while mutably borrowed
                    if self.borrow_state.has_mut_borrow(var_name) {
                        self.error(
                            TypeError::AlreadyBorrowed {
                                variable: var_name.to_string(),
                            },
                            expr.span,
                        );
                    }
                    self.borrow_state
                        .add_borrow(Arc::from(var_name), None, false);
                }
                Ty::reference(None, Mutability::Immutable, inner_ty)
            }
            UnaryOp::RefMut => {
                // Mutable reference: &mut expr → &mut T
                if let ExprKind::Ident(ident) = &expr.kind {
                    let var_name = ident.name.as_ref();
                    // Cannot take mutable reference while any borrow is active
                    if self.borrow_state.has_any_borrow(var_name) {
                        self.error(
                            TypeError::DoubleMutableBorrow {
                                variable: var_name.to_string(),
                            },
                            expr.span,
                        );
                    }
                    self.borrow_state
                        .add_borrow(Arc::from(var_name), None, true);
                }
                Ty::reference(None, Mutability::Mutable, inner_ty)
            }
        }
    }

    fn infer_binary(&mut self, op: BinOp, left: &ast::Expr, right: &ast::Expr, span: Span) -> Ty {
        let left_ty = self.infer_expr(left);
        let right_ty = self.infer_expr(right);

        match op {
            // Arithmetic operators
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                let _ = self.unify(&left_ty, &right_ty, span);
                self.apply(&left_ty)
            }

            // Comparison operators
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let _ = self.unify(&left_ty, &right_ty, span);
                Ty::bool()
            }

            // Logical operators
            BinOp::And | BinOp::Or => {
                let _ = self.unify(&left_ty, &Ty::bool(), span);
                let _ = self.unify(&right_ty, &Ty::bool(), span);
                Ty::bool()
            }

            // Bitwise operators
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                let _ = self.unify(&left_ty, &right_ty, span);
                self.apply(&left_ty)
            }

            // Pipe operator (function application)
            BinOp::Pipe => {
                // x |> f is equivalent to f(x)
                // The right side should be a function that takes the left side as argument
                let ret_ty = Ty::fresh_var();
                let expected_fn = Ty::function(vec![left_ty.clone()], ret_ty.clone());
                let _ = self.unify(&right_ty, &expected_fn, span);
                self.apply(&ret_ty)
            }

            // Power operator
            BinOp::Pow => {
                let _ = self.unify(&left_ty, &right_ty, span);
                self.apply(&left_ty)
            }

            // Range operators
            BinOp::Range | BinOp::RangeInclusive => {
                let _ = self.unify(&left_ty, &right_ty, span);
                let elem_ty = self.apply(&left_ty);
                // Returns a Range<T> or RangeInclusive<T> type
                self.make_range_type(elem_ty, op == BinOp::RangeInclusive)
            }

            // Compose operator
            BinOp::Compose => {
                // f >> g => g(f(x))
                Ty::fresh_var()
            }
        }
    }

    fn infer_assign(
        &mut self,
        _op: AssignOp,
        target: &ast::Expr,
        value: &ast::Expr,
        span: Span,
    ) -> Ty {
        let target_ty = self.infer_expr(target);
        let value_ty = self.infer_expr(value);
        let _ = self.unify(&target_ty, &value_ty, span);

        // Borrow check: if assigning a reference to a named variable,
        // track the borrow (same as check_borrow_at_binding for let).
        if let ExprKind::Ident(ident) = &target.kind {
            self.check_borrow_at_binding(ident.name.as_ref(), value, &value_ty, span);
        }

        Ty::unit()
    }

    // =========================================================================
    // ACCESS INFERENCE
    // =========================================================================

    fn infer_field(&mut self, expr: &ast::Expr, field: &ast::Ident, span: Span) -> Ty {
        let expr_ty = self.infer_expr(expr);
        let expr_ty = self.apply(&expr_ty);

        match &expr_ty.kind {
            TyKind::Adt(def_id, substs) => {
                // Look up field type in struct definition
                if let Some(type_def) = self.ctx.lookup_type(*def_id) {
                    match &type_def.kind {
                        TypeDefKind::Struct(struct_def) => {
                            let field_name = field.name.as_ref();
                            for (fname, fty) in &struct_def.fields {
                                if fname.as_ref() == field_name {
                                    // Apply generic substitutions to field type
                                    let mut field_ty = fty.clone();
                                    if !substs.is_empty() && !type_def.generics.is_empty() {
                                        let mut _subst = Substitution::new();
                                        for (i, _param) in type_def.generics.iter().enumerate() {
                                            if i < substs.len() {
                                                if let TyKind::Param(_, idx) = &fty.kind {
                                                    if *idx as usize == i {
                                                        field_ty = substs[i].clone();
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    return field_ty;
                                }
                            }
                            // Field not found
                            self.error(
                                TypeError::UndefinedField {
                                    ty: expr_ty,
                                    field: field_name.to_string(),
                                },
                                span,
                            );
                            return Ty::error();
                        }
                        TypeDefKind::Enum(_) => {
                            self.error(
                                TypeError::UndefinedField {
                                    ty: expr_ty,
                                    field: field.name.to_string(),
                                },
                                span,
                            );
                            return Ty::error();
                        }
                    }
                }
                // Type definition not found - return fresh var
                Ty::fresh_var()
            }
            TyKind::Ref(_, _, inner) => {
                // Auto-deref for field access on references
                self.infer_field_on_type(inner, field, span)
            }
            // Inference variables — allow field access, return fresh var
            TyKind::Var(_) | TyKind::Infer(_) => Ty::fresh_var(),
            // Never type — suppress cascading errors
            TyKind::Never => Ty::never(),
            TyKind::Error => Ty::error(),
            _ => {
                self.error(
                    TypeError::UndefinedField {
                        ty: expr_ty,
                        field: field.name.to_string(),
                    },
                    span,
                );
                Ty::error()
            }
        }
    }

    /// Helper to look up a field on a type (without wrapping in an expression).
    fn infer_field_on_type(&mut self, ty: &Ty, field: &ast::Ident, span: Span) -> Ty {
        match &ty.kind {
            TyKind::Adt(def_id, _substs) => {
                if let Some(type_def) = self.ctx.lookup_type(*def_id) {
                    if let TypeDefKind::Struct(struct_def) = &type_def.kind {
                        let field_name = field.name.as_ref();
                        for (fname, fty) in &struct_def.fields {
                            if fname.as_ref() == field_name {
                                return fty.clone();
                            }
                        }
                    }
                }
                Ty::fresh_var()
            }
            TyKind::Ref(_, _, inner) => self.infer_field_on_type(inner, field, span),
            _ => Ty::fresh_var(),
        }
    }

    fn infer_tuple_field(&mut self, expr: &ast::Expr, index: u32, span: Span) -> Ty {
        let expr_ty = self.infer_expr(expr);
        let expr_ty = self.apply(&expr_ty);

        match &expr_ty.kind {
            TyKind::Tuple(elems) => {
                if (index as usize) < elems.len() {
                    elems[index as usize].clone()
                } else {
                    self.error(
                        TypeError::UndefinedField {
                            ty: expr_ty,
                            field: index.to_string(),
                        },
                        span,
                    );
                    Ty::error()
                }
            }
            // Tuple structs: access fields by numeric index (e.g. `val.0`)
            TyKind::Adt(def_id, _substs) => {
                if let Some(type_def) = self.ctx.lookup_type(*def_id) {
                    if let TypeDefKind::Struct(struct_def) = &type_def.kind {
                        if struct_def.is_tuple {
                            let idx_str = index.to_string();
                            for (fname, fty) in &struct_def.fields {
                                if fname.as_ref() == idx_str {
                                    return fty.clone();
                                }
                            }
                        }
                    }
                }
                self.error(
                    TypeError::UndefinedField {
                        ty: expr_ty,
                        field: index.to_string(),
                    },
                    span,
                );
                Ty::error()
            }
            TyKind::Var(_) | TyKind::Infer(_) => Ty::fresh_var(),
            TyKind::Error => Ty::error(),
            _ => {
                self.error(
                    TypeError::UndefinedField {
                        ty: expr_ty,
                        field: index.to_string(),
                    },
                    span,
                );
                Ty::error()
            }
        }
    }

    fn infer_index(&mut self, expr: &ast::Expr, index: &ast::Expr, span: Span) -> Ty {
        let expr_ty = self.infer_expr(expr);
        let index_ty = self.infer_expr(index);
        let expr_ty = self.apply(&expr_ty);

        // Detect if index is a range expression (a..b, a..=b, ..b, a..)
        let is_range = matches!(&index.kind, ast::ExprKind::Range { .. });

        // Check that index type is usize (or can be unified with usize)
        // Skip for range expressions which have their own type
        if !is_range {
            let _ = self.unify(&index_ty, &Ty::int(IntTy::Usize), span);
        }

        match &expr_ty.kind {
            TyKind::Array(elem, _) | TyKind::Slice(elem) => {
                if is_range {
                    Ty::slice((**elem).clone())
                } else {
                    (**elem).clone()
                }
            }
            TyKind::Str => {
                if is_range {
                    Ty::str()
                } else {
                    Ty::int(IntTy::U8)
                }
            }
            TyKind::Ref(_, _, inner) => match &inner.kind {
                TyKind::Array(elem, _) | TyKind::Slice(elem) => {
                    if is_range {
                        Ty::slice((**elem).clone())
                    } else {
                        (**elem).clone()
                    }
                }
                TyKind::Str => {
                    if is_range {
                        Ty::str()
                    } else {
                        Ty::int(IntTy::U8)
                    }
                }
                // ADT through reference (&Vec<T>, &mut HashMap<K,V>, etc.)
                TyKind::Adt(_, substs) => {
                    if !substs.is_empty() {
                        if is_range {
                            Ty::slice(substs[0].clone())
                        } else {
                            substs[0].clone()
                        }
                    } else {
                        Ty::fresh_var()
                    }
                }
                TyKind::Var(_) | TyKind::Infer(_) => Ty::fresh_var(),
                _ => {
                    self.error(TypeError::NotIndexable { ty: expr_ty }, span);
                    Ty::error()
                }
            },
            // ADT types (Vec, HashMap, etc.)
            TyKind::Adt(_, substs) => {
                if !substs.is_empty() {
                    if is_range {
                        // Range indexing on Vec<T> returns [T] (slice)
                        Ty::slice(substs[0].clone())
                    } else {
                        substs[0].clone()
                    }
                } else {
                    Ty::fresh_var()
                }
            }
            // Inference variables — allow indexing, return fresh var
            TyKind::Var(_) | TyKind::Infer(_) => Ty::fresh_var(),
            // Never type — suppress cascading
            TyKind::Never => Ty::never(),
            TyKind::Error => Ty::error(),
            _ => {
                self.error(TypeError::NotIndexable { ty: expr_ty }, span);
                Ty::error()
            }
        }
    }

    // =========================================================================
    // CALL INFERENCE
    // =========================================================================

    fn infer_call(&mut self, func: &ast::Expr, args: &[ast::Expr], span: Span) -> Ty {
        let func_ty = self.infer_expr(func);
        let func_ty = self.apply(&func_ty);

        match &func_ty.kind {
            TyKind::Fn(fn_ty) => {
                if fn_ty.params.len() != args.len() {
                    self.error(
                        TypeError::ArityMismatch {
                            expected: fn_ty.params.len(),
                            found: args.len(),
                        },
                        span,
                    );
                }

                for (param, arg) in fn_ty.params.iter().zip(args.iter()) {
                    let arg_ty = self.infer_expr(arg);
                    let _ = self.unify(param, &arg_ty, span);
                }

                // Propagate callee's effects to caller's effect context
                if !fn_ty.effects.is_empty() {
                    self.current_effects = self.current_effects.merge(&fn_ty.effects);
                }

                // Interprocedural lifetime: if the function returns a reference,
                // track which argument variables the return borrows from.
                let ret = (*fn_ty.ret).clone();
                if let TyKind::Ref(ref ret_lt, _, _) = &ret.kind {
                    if !fn_ty.lifetime_params.is_empty() {
                        // Lifetime-guided: only track args whose param lifetime
                        // matches the return type's lifetime parameter.
                        let ret_lt_name = ret_lt.as_ref().map(|l| &l.name);
                        for (param_ty, arg) in fn_ty.params.iter().zip(args.iter()) {
                            if let TyKind::Ref(Some(ref param_lt), _, _) = &param_ty.kind {
                                let shares_lifetime = match ret_lt_name {
                                    Some(name) => param_lt.name == *name,
                                    None => true, // elided return: conservative
                                };
                                if shares_lifetime {
                                    if let ExprKind::Ref { expr: inner, .. } = &arg.kind {
                                        if let ExprKind::Ident(ident) = &inner.kind {
                                            self.borrow_state.add_borrow(
                                                Arc::from(ident.name.as_ref()),
                                                None,
                                                false,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // No explicit lifetime params: apply elision rules.
                        // Count reference parameters to decide which args propagate.
                        let ref_param_indices: Vec<usize> = fn_ty
                            .params
                            .iter()
                            .enumerate()
                            .filter(|(_, p)| matches!(&p.kind, TyKind::Ref(_, _, _)))
                            .map(|(i, _)| i)
                            .collect();

                        if ref_param_indices.len() == 1 {
                            // Elision rule 1: single ref param → return borrows from it
                            let idx = ref_param_indices[0];
                            if let Some(arg) = args.get(idx) {
                                if let ExprKind::Ref { expr: inner, .. } = &arg.kind {
                                    if let ExprKind::Ident(ident) = &inner.kind {
                                        self.borrow_state.add_borrow(
                                            Arc::from(ident.name.as_ref()),
                                            None,
                                            false,
                                        );
                                    }
                                }
                            }
                        } else {
                            // Multiple ref params or none: conservative fallback
                            for arg in args {
                                if let ExprKind::Ref { expr: inner, .. } = &arg.kind {
                                    if let ExprKind::Ident(ident) = &inner.kind {
                                        self.borrow_state.add_borrow(
                                            Arc::from(ident.name.as_ref()),
                                            None,
                                            false,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                ret
            }
            TyKind::Var(_) | TyKind::Infer(_) => {
                // Unknown function type - create fresh types for params and return
                let param_tys: Vec<_> = args.iter().map(|a| self.infer_expr(a)).collect();
                let ret_ty = Ty::fresh_var();
                let fn_ty = Ty::function(param_tys, ret_ty.clone());
                let _ = self.unify(&func_ty, &fn_ty, span);
                ret_ty
            }
            TyKind::Error => Ty::error(),
            // Unit type — cascading from a failed function lookup; suppress error
            TyKind::Tuple(elems) if elems.is_empty() => Ty::fresh_var(),
            _ => {
                self.error(TypeError::NotCallable { ty: func_ty }, span);
                Ty::error()
            }
        }
    }

    // =========================================================================
    // EFFECT INFERENCE
    // =========================================================================

    /// Infer the type of a handle expression.
    ///
    /// A handle expression wraps a body that may perform effects, and provides
    /// handlers that intercept those effects. The handled effects are removed
    /// from the body's effect row.
    fn infer_handle(
        &mut self,
        effect: &ast::Path,
        body: &ast::Block,
        handlers: &[ast::EffectHandler],
        span: Span,
    ) -> Ty {
        // Determine which effect is being handled from the path
        let effect_name = effect
            .last_ident()
            .map(|i| i.name.as_ref().to_string())
            .unwrap_or_default();

        // Save the current effect row
        let saved_effects = self.current_effects.clone();

        // Infer the body with a fresh effect context
        self.current_effects = super::effects::EffectRow::empty();
        let body_ty = self.infer_block(body);
        let body_effects = self.current_effects.clone();

        // Check that handlers cover the operations of the handled effect
        if let Some(effect_def) = self.effect_ctx.get_effect(&effect_name) {
            let defined_ops: Vec<_> = effect_def
                .operations
                .iter()
                .map(|op| op.name.as_ref().to_string())
                .collect();

            // Collect which operations the handlers cover
            let mut handled_ops: Vec<String> = Vec::new();

            for handler in handlers {
                let handler_op = handler.operation.name.as_ref();

                // Check that this operation exists in the effect definition
                if !defined_ops.iter().any(|op| op == handler_op) {
                    self.error(
                        TypeError::UnknownEffectOperation {
                            effect_name: effect_name.clone(),
                            operation: handler_op.to_string(),
                        },
                        handler.span,
                    );
                } else {
                    handled_ops.push(handler_op.to_string());
                }

                // Infer the handler body type
                let _ = self.infer_expr(&handler.body);
            }

            // Check for missing handler clauses: all operations must be handled
            for op_name in &defined_ops {
                if !handled_ops.iter().any(|h| h == op_name) {
                    let err = TypeError::MissingHandlerClause {
                        effect_name: effect_name.clone(),
                        operation: op_name.clone(),
                    };
                    let mut err_with_span = TypeErrorWithSpan::new(err, span);
                    err_with_span.help = Some(format!(
                        "add a handler clause for `{}`:\n  {}.{}(params) => |resume| {{\n      // handle the {} operation\n      resume(())\n  }},",
                        op_name, effect_name, op_name, op_name
                    ));
                    self.errors.push(err_with_span);
                }
            }
        } else {
            // Unknown effect -- report the error and still infer handler bodies
            let err = TypeError::UnknownEffect {
                name: effect_name.clone(),
            };
            let mut err_with_span = TypeErrorWithSpan::new(err, span);
            err_with_span.help = Some(format!(
                "define the effect:\n  effect {} {{\n      fn operation_name(params) -> ReturnType,\n  }}",
                effect_name
            ));
            self.errors.push(err_with_span);
            for handler in handlers {
                let _ = self.infer_expr(&handler.body);
            }
        }

        // Remove the handled effect from the body's effect row
        let handled_effect = super::effects::Effect::new(effect_name.as_str());
        let mut remaining_effects = body_effects;
        remaining_effects.remove(&handled_effect);

        // Restore and merge: caller gets body's remaining (unhandled) effects
        self.current_effects = saved_effects.merge(&remaining_effects);

        body_ty
    }

    /// Infer the type of a perform expression.
    ///
    /// `perform Effect.operation(args)` invokes an effect operation,
    /// adding the effect to the current context and returning the
    /// operation's return type.
    fn infer_perform(
        &mut self,
        effect_ident: &ast::Ident,
        operation_ident: &ast::Ident,
        args: &[ast::Expr],
        span: Span,
    ) -> Ty {
        let effect_name = effect_ident.name.as_ref();
        let operation_name = operation_ident.name.as_ref();

        // Add this effect to the current context
        let effect = super::effects::Effect::new(effect_name);
        self.current_effects.add(effect);

        // Look up the effect definition to find the operation's return type.
        // Clone data we need to avoid borrow conflicts with self.
        let effect_lookup = self.effect_ctx.get_effect(effect_name).map(|def| {
            let op_match = def
                .operations
                .iter()
                .find(|op| op.name.as_ref() == operation_name);
            (
                op_match.map(|op| (op.params.clone(), op.return_ty.clone())),
                def.operations.len(),
            )
        });

        if let Some((op_data, _op_count)) = effect_lookup {
            if let Some((param_tys, return_ty)) = op_data {
                // Check argument count
                if param_tys.len() != args.len() {
                    self.error(
                        TypeError::ArityMismatch {
                            expected: param_tys.len(),
                            found: args.len(),
                        },
                        span,
                    );
                }

                // Type-check arguments against operation parameters
                for (param_ty, arg) in param_tys.iter().zip(args.iter()) {
                    let arg_ty = self.infer_expr(arg);
                    let _ = self.unify(param_ty, &arg_ty, span);
                }

                return return_ty;
            } else {
                let err = TypeError::UnknownEffectOperation {
                    effect_name: effect_name.to_string(),
                    operation: operation_name.to_string(),
                };
                self.error(err, span);
            }
        } else {
            let err = TypeError::UnknownEffect {
                name: effect_name.to_string(),
            };
            let mut err_with_span = TypeErrorWithSpan::new(err, span);
            err_with_span.help = Some(format!(
                "define the effect:\n  effect {} {{\n      fn operation_name(params) -> ReturnType,\n  }}",
                effect_name
            ));
            self.errors.push(err_with_span);
            // Still infer argument types even for unknown effects
            for arg in args {
                let _ = self.infer_expr(arg);
            }
        }

        Ty::fresh_var()
    }

    fn infer_method_call(
        &mut self,
        receiver: &ast::Expr,
        method: &ast::Ident,
        args: &[ast::Expr],
        span: Span,
    ) -> Ty {
        let receiver_ty = self.infer_expr(receiver);
        let receiver_ty = self.apply(&receiver_ty);

        // Infer argument types
        let arg_tys: Vec<_> = args.iter().map(|a| self.infer_expr(a)).collect();

        // Check for trait object method calls: dyn Trait has all trait methods
        if let TyKind::TraitObject(_bounds) = receiver_ty.kind {
            return Ty::error();
        }

        // Error type — silently accept any method call to prevent cascading errors
        if matches!(&receiver_ty.kind, TyKind::Error) {
            return Ty::error();
        }

        // Try to look up method in impl blocks using trait resolver.
        // Auto-deref: if receiver is &T or &mut T, also try looking up on T.
        let derefed_ty = match &receiver_ty.kind {
            TyKind::Ref(_, _, inner) => Some(inner.as_ref().clone()),
            TyKind::Ptr(_, inner) => Some(inner.as_ref().clone()),
            _ => None,
        };
        let method_result = self
            .lookup_method(&receiver_ty, method.name.as_ref())
            .or_else(|| {
                derefed_ty
                    .as_ref()
                    .and_then(|dt| self.lookup_method(dt, method.name.as_ref()))
            });

        if let Some(method_ty) = method_result {
            // Unify with expected function type
            match &method_ty.kind {
                TyKind::Fn(fn_ty) => {
                    // Check arity (method params don't include self)
                    if fn_ty.params.len() != arg_tys.len() {
                        self.error(
                            TypeError::ArityMismatch {
                                expected: fn_ty.params.len(),
                                found: arg_tys.len(),
                            },
                            span,
                        );
                    }
                    // Unify argument types
                    for (param, arg) in fn_ty.params.iter().zip(arg_tys.iter()) {
                        let _ = self.unify(param, arg, span);
                    }
                    return (*fn_ty.ret).clone();
                }
                _ => {}
            }
        }

        // Check for common built-in methods.
        // This handles methods on primitive types, standard library types,
        // and well-known patterns that don't require explicit trait impls.
        let method_name = method.name.as_ref();

        // Helper: check if type is an integer or float type
        let is_int = match &receiver_ty.kind {
            TyKind::Int(_) => true,
            TyKind::Ref(_, _, inner) => matches!(&inner.kind, TyKind::Int(_)),
            _ => false,
        };
        let is_float = match &receiver_ty.kind {
            TyKind::Float(_) => true,
            TyKind::Ref(_, _, inner) => matches!(&inner.kind, TyKind::Float(_)),
            _ => false,
        };
        let is_numeric = is_int || is_float;

        match method_name {
            // =================================================================
            // COLLECTION / SEQUENCE METHODS
            // =================================================================
            "len" | "capacity" => {
                match &receiver_ty.kind {
                    TyKind::Array(_, _) | TyKind::Slice(_) | TyKind::Str => {
                        return Ty::int(IntTy::Usize);
                    }
                    TyKind::Ref(_, _, inner) => match &inner.kind {
                        TyKind::Array(_, _) | TyKind::Slice(_) | TyKind::Str => {
                            return Ty::int(IntTy::Usize);
                        }
                        _ => {}
                    },
                    _ => {}
                }
                // Fallthrough: also works on Vec, HashMap, String, etc.
                return Ty::int(IntTy::Usize);
            }
            "is_empty" => return Ty::bool(),
            "contains" | "starts_with" | "ends_with" => return Ty::bool(),
            "clone" | "copy" => return receiver_ty.clone(),
            "to_string" | "to_owned" => return Ty::str(),
            "as_str" | "as_ref" => return Ty::str(),
            "parse_int" => return Ty::int(IntTy::I64),
            "parse_float" => return Ty::float(FloatTy::F64),

            // Collection mutators — return unit
            "push" | "push_str" | "push_back" | "push_front" | "insert" | "extend"
            | "extend_from_slice" | "copy_from_slice" | "clear" | "truncate" | "sort"
            | "sort_by" | "sort_unstable" | "reverse" | "reserve" | "shrink_to_fit" | "retain"
            | "dedup" | "set" | "store" => {
                return Ty::unit();
            }

            // Collection accessors — return element or fresh var
            "pop" | "pop_front" | "pop_back" => return Ty::fresh_var(),
            "remove" | "swap_remove" => return Ty::fresh_var(),
            "get" | "get_mut" | "entry" | "first" | "last" | "front" | "back" => {
                return Ty::fresh_var();
            }
            "split" | "splitn" | "rsplit" | "split_once" | "rsplit_once" | "split_whitespace"
            | "split_at" | "chunks" | "chunks_mut" | "chunks_exact" | "windows" | "lines"
            | "drain" | "splice" => {
                return Ty::fresh_var();
            }
            "binary_search" | "binary_search_by" | "binary_search_by_key" => {
                return Ty::fresh_var();
            }
            "join" => return Ty::str(),
            "with_capacity" | "from_slice" | "to_vec" | "into_vec" | "into_boxed_slice" => {
                return Ty::fresh_var();
            }

            // =================================================================
            // ITERATOR METHODS
            // =================================================================
            "iter" | "iter_mut" | "into_iter" | "values" | "keys" | "chars" | "bytes"
            | "as_bytes" => {
                return Ty::fresh_var();
            }
            "map" | "filter" | "filter_map" | "flat_map" | "and_then" | "or_else" | "map_err"
            | "take" | "skip" | "take_while" | "skip_while" | "chain" | "zip" | "enumerate"
            | "peekable" | "cycle" | "step_by" | "inspect" | "fuse" | "scan" => {
                return Ty::fresh_var();
            }
            "collect" | "fold" | "reduce" => return Ty::fresh_var(),
            "sum" | "product" => return Ty::fresh_var(),
            "count" => return Ty::int(IntTy::Usize),
            "any" | "all" => return Ty::bool(),
            "find" | "find_map" | "position" | "rposition" => return Ty::fresh_var(),
            "next" | "peek" | "next_back" if !is_numeric => {
                return Ty::fresh_var();
            }
            "min" | "max" | "min_by" | "max_by" | "min_by_key" | "max_by_key" if !is_numeric => {
                return Ty::fresh_var();
            }
            "for_each" => return Ty::unit(),

            // =================================================================
            // OPTION / RESULT METHODS
            // =================================================================
            "unwrap" | "expect" => {
                if let TyKind::Adt(def_id, substs) = &receiver_ty.kind {
                    if (Some(*def_id) == self.well_known_types.option
                        || Some(*def_id) == self.well_known_types.result)
                        && !substs.is_empty()
                    {
                        return substs[0].clone();
                    }
                }
                return Ty::fresh_var();
            }
            "unwrap_or" | "unwrap_or_else" | "unwrap_or_default" => {
                if let TyKind::Adt(def_id, substs) = &receiver_ty.kind {
                    if (Some(*def_id) == self.well_known_types.option
                        || Some(*def_id) == self.well_known_types.result)
                        && !substs.is_empty()
                    {
                        return substs[0].clone();
                    }
                }
                return Ty::fresh_var();
            }
            "ok" | "err" => return Ty::fresh_var(), // Result → Option conversion
            "ok_or" | "ok_or_else" => return Ty::fresh_var(),
            "is_some" | "is_none" | "is_ok" | "is_err" => return Ty::bool(),
            "as_mut" => return Ty::fresh_var(),

            // =================================================================
            // INTEGER METHODS
            // =================================================================
            "leading_zeros" | "trailing_zeros" | "count_ones" | "count_zeros" | "leading_ones"
            | "trailing_ones"
                if is_int =>
            {
                return Ty::int(IntTy::U32);
            }
            "wrapping_add" | "wrapping_sub" | "wrapping_mul" | "wrapping_div" | "wrapping_neg"
            | "wrapping_shl" | "wrapping_shr" | "saturating_add" | "saturating_sub"
            | "saturating_mul" | "checked_add" | "checked_sub" | "checked_mul" | "checked_div"
            | "overflowing_add" | "overflowing_sub" | "overflowing_mul" | "rotate_left"
            | "rotate_right" | "swap_bytes" | "reverse_bits"
                if is_int =>
            {
                return receiver_ty.clone();
            }
            "to_le_bytes" | "to_be_bytes" | "to_ne_bytes" if is_int => {
                return Ty::fresh_var(); // returns [u8; N]
            }
            "from_le_bytes" | "from_be_bytes" | "from_ne_bytes" if is_int => {
                return receiver_ty.clone();
            }
            "pow" if is_int => return receiver_ty.clone(),
            "abs" if is_int => return receiver_ty.clone(),
            "checked_neg" | "checked_abs" if is_int => return Ty::fresh_var(),
            "to_le" | "to_be" | "to_ne" if is_int => return receiver_ty.clone(),
            "is_power_of_two" if is_int => return Ty::bool(),
            "next_power_of_two" if is_int => return receiver_ty.clone(),
            "partial_cmp" | "cmp" if is_int => return Ty::fresh_var(),

            // =================================================================
            // FLOAT METHODS
            // =================================================================
            "abs" | "sqrt" | "cbrt" | "ceil" | "floor" | "round" | "trunc" | "fract" | "signum"
            | "recip" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh"
            | "tanh" | "asinh" | "acosh" | "atanh" | "exp" | "exp2" | "ln" | "log2" | "log10"
            | "to_radians" | "to_degrees"
                if is_float =>
            {
                return receiver_ty.clone();
            }
            "powi" | "powf" | "log" | "atan2" | "hypot" | "copysign" | "clamp" | "min" | "max"
            | "mul_add" | "div_euclid" | "rem_euclid"
                if is_float =>
            {
                return receiver_ty.clone();
            }
            "is_nan" | "is_infinite" | "is_finite" | "is_normal" | "is_sign_positive"
            | "is_sign_negative"
                if is_float =>
            {
                return Ty::bool();
            }
            "sin_cos" if is_float => {
                return Ty::tuple(vec![receiver_ty.clone(), receiver_ty.clone()]);
            }
            "partial_cmp" if is_float => {
                return Ty::fresh_var(); // Returns Option<Ordering>
            }
            "total_cmp" if is_float => {
                return Ty::fresh_var(); // Returns Ordering
            }
            "to_bits" if is_float => return Ty::int(IntTy::U64),
            "to_int_unchecked" if is_float => return Ty::fresh_var(),
            "classify" if is_float => return Ty::fresh_var(),

            // Numeric methods that work on any numeric type
            "abs" | "min" | "max" | "clamp" if is_numeric => {
                return receiver_ty.clone();
            }

            // =================================================================
            // STRING METHODS
            // =================================================================
            "trim" | "trim_start" | "trim_end" | "trim_matches" | "trim_start_matches"
            | "trim_end_matches" | "to_uppercase" | "to_lowercase" | "to_ascii_uppercase"
            | "to_ascii_lowercase" | "replace" | "replacen" | "char_at" | "substring" => {
                return Ty::str();
            }
            "index_of" => return Ty::int(IntTy::I64),
            "repeat" => return Ty::str(),
            "parse" => return Ty::fresh_var(),
            "rfind" if matches!(&receiver_ty.kind, TyKind::Str | TyKind::Ref(_, _, _)) => {
                return Ty::fresh_var();
            }
            "is_ascii"
            | "is_ascii_alphanumeric"
            | "is_ascii_alphabetic"
            | "is_ascii_digit"
            | "is_ascii_lowercase"
            | "is_ascii_uppercase"
            | "is_ascii_whitespace"
            | "is_ascii_punctuation"
            | "is_char_boundary"
            | "is_alphanumeric"
            | "is_alphabetic"
            | "is_numeric"
            | "is_whitespace"
            | "is_lowercase"
            | "is_uppercase"
            | "is_control"
            | "is_digit"
            | "is_ascii_control"
            | "is_ascii_graphic"
            | "is_ascii_hexdigit" => {
                return Ty::bool();
            }
            "len_utf8" | "len_utf16" => {
                return Ty::int(IntTy::Usize);
            }
            "to_ascii_lowercase" | "to_ascii_uppercase" | "to_lowercase" | "to_uppercase" => {
                return Ty::char();
            }
            "char_indices" | "chars" | "bytes" => {
                return Ty::fresh_var(); // Iterator — return fresh var for now
            }
            "components" | "to_path_buf" => {
                return Ty::fresh_var(); // Path methods
            }
            "is_dir" | "is_file" | "is_absolute" | "is_relative" | "exists" => {
                return Ty::bool();
            }
            "map_or" | "unwrap_or_else" | "and_then" | "or_else" => {
                return Ty::fresh_var(); // Combinator methods
            }

            // =================================================================
            // FORMATTING & I/O
            // =================================================================
            "write_str" | "write_fmt" | "write" | "write_all" | "flush" => {
                return Ty::fresh_var();
            }
            "read" | "read_exact" | "read_to_string" | "read_to_end" | "read_line" => {
                return Ty::fresh_var();
            }

            // =================================================================
            // CONVERSION / UTILITY
            // =================================================================
            "into" | "from" | "try_into" | "try_from" => return Ty::fresh_var(),
            "default" => return Ty::fresh_var(),
            "hash" => return Ty::unit(),
            "fmt" | "display" | "debug" => return Ty::fresh_var(),
            "serialize" | "deserialize" => return Ty::fresh_var(),

            // =================================================================
            // CONCURRENCY / SYNC
            // =================================================================
            "lock" | "try_lock" | "try_read" | "try_write" => {
                return Ty::fresh_var();
            }
            "send" | "recv" | "try_send" | "try_recv" => return Ty::fresh_var(),
            "spawn" => return Ty::fresh_var(),
            "load" | "fetch_add" | "fetch_sub" | "compare_exchange" | "compare_and_swap"
            | "swap" => {
                return Ty::fresh_var();
            }

            // =================================================================
            // TIME
            // =================================================================
            "elapsed" | "duration_since" => return Ty::fresh_var(),
            "as_secs" | "as_millis" | "as_micros" | "as_nanos" => {
                return Ty::int(IntTy::U64);
            }
            "as_secs_f64" | "as_secs_f32" => return Ty::float(FloatTy::F64),

            // =================================================================
            // MATH / MATRIX
            // =================================================================
            "inverse" | "transpose" | "determinant" | "adjugate" | "multiply" | "transform"
            | "apply" | "normalize" | "cross" | "reflect" | "lerp" | "slerp" | "nlerp" => {
                return receiver_ty.clone();
            }
            "length" | "magnitude" | "norm" | "dot" | "distance" => {
                return Ty::float(FloatTy::F64);
            }

            _ => {}
        }

        // For unresolved inference variables (?T or &?T), allow any method call
        // and return a fresh variable. This prevents cascading errors
        // when the concrete type isn't known yet.
        if matches!(&receiver_ty.kind, TyKind::Var(_) | TyKind::Infer(_)) {
            return Ty::fresh_var();
        }
        if let Some(ref dt) = derefed_ty {
            if matches!(&dt.kind, TyKind::Var(_) | TyKind::Infer(_)) {
                return Ty::fresh_var();
            }
        }

        // Method not found - report error but return fresh variable to continue inference
        self.error(
            TypeError::UndefinedMethod {
                ty: receiver_ty,
                method: method.name.to_string(),
            },
            span,
        );
        Ty::fresh_var()
    }

    // =========================================================================
    // CONTROL FLOW INFERENCE
    // =========================================================================

    fn infer_if(
        &mut self,
        condition: &ast::Expr,
        then_branch: &ast::Block,
        else_branch: Option<&ast::Expr>,
        span: Span,
    ) -> Ty {
        let cond_ty = self.infer_expr(condition);
        let _ = self.unify(&cond_ty, &Ty::bool(), span);

        let then_ty = self.infer_block(then_branch);

        if let Some(else_expr) = else_branch {
            let else_ty = self.infer_expr(else_expr);
            let _ = self.unify(&then_ty, &else_ty, span);
            self.apply(&then_ty)
        } else {
            // if without else returns unit
            let _ = self.unify(&then_ty, &Ty::unit(), span);
            Ty::unit()
        }
    }

    fn infer_match(&mut self, scrutinee: &ast::Expr, arms: &[ast::MatchArm], span: Span) -> Ty {
        let scrutinee_ty = self.infer_expr(scrutinee);

        let mut result_ty = Ty::fresh_var();

        for arm in arms {
            // Type check pattern against scrutinee
            self.check_pattern(&arm.pattern, &scrutinee_ty);

            // Type check guard if present
            if let Some(guard) = &arm.guard {
                let guard_ty = self.infer_expr(guard);
                let _ = self.unify(&guard_ty, &Ty::bool(), span);
            }

            // Infer body type
            let body_ty = self.infer_expr(&arm.body);
            let _ = self.unify(&result_ty, &body_ty, span);
            result_ty = self.apply(&result_ty);
        }

        // Exhaustiveness checking: determine the enum type from either
        // the resolved scrutinee or the arm patterns, then verify all
        // variants are covered.
        let enum_info = self.resolve_enum_from_match(arms, &scrutinee_ty);
        if let Some(all_variants) = enum_info {
            let mut covered: Vec<String> = Vec::new();
            let mut has_wildcard = false;

            for arm in arms {
                let variants = extract_covered_variants(&arm.pattern);
                for v in &variants {
                    if v == "*" {
                        has_wildcard = true;
                    } else {
                        covered.push(v.clone());
                    }
                }
            }

            if !has_wildcard {
                let missing: Vec<String> = all_variants
                    .iter()
                    .filter(|v| !covered.contains(v))
                    .cloned()
                    .collect();

                if !missing.is_empty() {
                    self.error(
                        TypeError::NonExhaustiveMatch {
                            missing_variants: missing,
                        },
                        span,
                    );
                }
            }
        }

        result_ty
    }

    /// Resolve the enum type from match arm patterns.
    /// When the scrutinee is an unresolved type variable, we extract the enum
    /// name from the first variant pattern (e.g., `Color::Red` tells us the
    /// enum is `Color`) and look it up in the type context.
    fn resolve_enum_from_match(
        &self,
        arms: &[ast::MatchArm],
        scrutinee_ty: &Ty,
    ) -> Option<Vec<String>> {
        // First try: resolved scrutinee type
        let resolved = self.apply(scrutinee_ty);
        if let TyKind::Adt(def_id, _) = &resolved.kind {
            if let Some(type_def) = self.ctx.lookup_type(*def_id) {
                if let TypeDefKind::Enum(enum_def) = &type_def.kind {
                    return Some(
                        enum_def
                            .variants
                            .iter()
                            .map(|v| v.name.to_string())
                            .collect(),
                    );
                }
            }
        }

        // Second try: extract enum name from the pattern paths
        for arm in arms {
            if let Some(enum_name) = extract_enum_name_from_pattern(&arm.pattern) {
                if let Some(type_def) = self.ctx.lookup_type_by_name(&enum_name) {
                    if let TypeDefKind::Enum(enum_def) = &type_def.kind {
                        return Some(
                            enum_def
                                .variants
                                .iter()
                                .map(|v| v.name.to_string())
                                .collect(),
                        );
                    }
                }
            }
        }

        None
    }

    fn infer_loop(&mut self, body: &ast::Block) -> Ty {
        self.ctx.push_scope(ScopeKind::Loop);
        let _ = self.infer_block(body);
        self.ctx.pop_scope();

        // Loop returns never (unless break with value)
        Ty::never()
    }

    fn infer_while(&mut self, condition: &ast::Expr, body: &ast::Block) -> Ty {
        let cond_ty = self.infer_expr(condition);
        let _ = self.unify(&cond_ty, &Ty::bool(), condition.span);

        self.ctx.push_scope(ScopeKind::Loop);
        let _ = self.infer_block(body);
        self.ctx.pop_scope();

        Ty::unit()
    }

    fn infer_for(
        &mut self,
        pattern: &ast::Pattern,
        iter: &ast::Expr,
        body: &ast::Block,
        _span: Span,
    ) -> Ty {
        let iter_ty = self.infer_expr(iter);
        let iter_ty = self.apply(&iter_ty);

        // Resolve Iterator trait to get Item type
        let item_ty = self.resolve_iterator_item(&iter_ty);

        self.ctx.push_scope(ScopeKind::Loop);
        self.check_pattern(pattern, &item_ty);
        let _ = self.infer_block(body);
        self.ctx.pop_scope();

        Ty::unit()
    }

    // =========================================================================
    // BLOCK INFERENCE
    // =========================================================================

    pub fn infer_block(&mut self, block: &ast::Block) -> Ty {
        self.ctx.push_scope(ScopeKind::Block);
        self.borrow_state.push_scope();

        let mut result_ty = Ty::unit();

        for (i, stmt) in block.stmts.iter().enumerate() {
            // NLL: before processing each statement, release borrows whose
            // binding variables have no further uses in remaining statements.
            let remaining = &block.stmts[i + 1..];
            self.release_dead_borrows(remaining);

            result_ty = self.infer_stmt(stmt);
        }

        let dying_borrows = self.borrow_state.pop_scope();

        // Check for dangling references: if a dying borrow's binding
        // is still alive in an outer scope, the reference outlives its source.
        for borrow in &dying_borrows {
            if let Some(ref binding_name) = borrow.binding {
                // Check if the binding variable exists in an outer scope
                // (it was declared before this block and will outlive it)
                if self.ctx.lookup_var(binding_name).is_some() {
                    // The binding survives but the borrowed variable dies
                    // → dangling reference
                    self.error(
                        TypeError::ReferenceEscapesScope {
                            variable: borrow.variable.to_string(),
                        },
                        block.span,
                    );
                }
            }
        }

        self.ctx.pop_scope();
        result_ty
    }

    /// Release borrows whose binding variables are not referenced in any
    /// of the remaining statements. This implements simplified Non-Lexical
    /// Lifetimes: a borrow dies at its last use, not at scope end.
    ///
    /// Only considers borrows with a named binding (e.g., `let r = &x` where
    /// binding="r"). Temporary borrows (function arguments) have no binding
    /// and are never tracked past their creation.
    fn release_dead_borrows(&mut self, remaining_stmts: &[ast::Stmt]) {
        // Collect (index, binding_name) for borrows that have named bindings
        let entries: Vec<(usize, Arc<str>)> = self
            .borrow_state
            .borrows
            .iter()
            .enumerate()
            .filter_map(|(i, b)| b.binding.as_ref().map(|name| (i, name.clone())))
            .collect();

        let mut to_remove = Vec::new();
        for (idx, binding_name) in &entries {
            // A borrow is dead if the binding variable (e.g., `r`) is not
            // referenced in any remaining statement
            let is_used = remaining_stmts
                .iter()
                .any(|s| stmt_mentions_var(s, binding_name));
            if !is_used {
                to_remove.push(*idx);
            }
        }

        // Remove in reverse order to preserve indices
        for idx in to_remove.into_iter().rev() {
            if idx < self.borrow_state.borrows.len() {
                self.borrow_state.borrows.remove(idx);
            }
        }
    }

    fn infer_stmt(&mut self, stmt: &ast::Stmt) -> Ty {
        match &stmt.kind {
            ast::StmtKind::Local(local) => {
                self.infer_local(local);
                Ty::unit()
            }
            ast::StmtKind::Expr(expr) => self.infer_expr(expr),
            ast::StmtKind::Semi(expr) => {
                let _ = self.infer_expr(expr);
                Ty::unit()
            }
            ast::StmtKind::Item(item) => {
                // Nested items (functions, structs, etc.) — register in local scope
                match &item.kind {
                    ast::ItemKind::Function(f) => {
                        // Register nested function as a local variable
                        let param_tys: Vec<Ty> = f
                            .sig
                            .params
                            .iter()
                            .map(|p| self.lower_type(&p.ty))
                            .collect();
                        let ret_ty = f
                            .sig
                            .return_ty
                            .as_ref()
                            .map(|t| self.lower_type(t))
                            .unwrap_or(Ty::unit());
                        let fn_ty = Ty::function(param_tys, ret_ty);
                        self.ctx.define_var(f.name.name.clone(), fn_ty);
                    }
                    ast::ItemKind::Const(c) => {
                        // Register local const as a variable
                        let ty = self.lower_type(&c.ty);
                        if let Some(value) = &c.value {
                            let _ = self.infer_expr(value);
                        }
                        self.ctx.define_var(c.name.name.clone(), ty);
                    }
                    _ => {}
                }
                Ty::unit()
            }
            ast::StmtKind::Empty => Ty::unit(),
            ast::StmtKind::Macro {
                path,
                tokens: _,
                is_semi,
            } => {
                // Macro invocations as statements
                // For well-known macros, we can infer their result type
                // For unknown macros, we use unit if semicolon-terminated,
                // otherwise a fresh type variable

                let macro_name = path.segments.last().map(|s| s.ident.as_str()).unwrap_or("");

                match macro_name {
                    // Diagnostic macros always return unit
                    "println" | "print" | "eprintln" | "eprint" | "dbg" | "debug" | "log"
                    | "trace" | "warn" | "error" | "assert" | "assert_eq" | "assert_ne"
                    | "debug_assert" | "debug_assert_eq" | "debug_assert_ne" => Ty::unit(),

                    // Panic/unreachable return never type
                    "panic" | "unreachable" | "unimplemented" | "todo" => Ty::never(),

                    // Format macros return String
                    "format" | "format_args" => Ty::string(),

                    // Vec macro returns Vec<T> with fresh type var
                    "vec" => {
                        let elem_ty = Ty::fresh_var();
                        Ty::vec(elem_ty)
                    }

                    // Other macros - type depends on semicolon
                    _ => {
                        if *is_semi {
                            // Semicolon-terminated statement - result is discarded
                            Ty::unit()
                        } else {
                            // No semicolon - macro result is the block value
                            Ty::fresh_var()
                        }
                    }
                }
            }
        }
    }

    fn infer_local(&mut self, local: &ast::Local) {
        let ty = if let Some(ty_ast) = &local.ty {
            self.lower_type(ty_ast)
        } else {
            Ty::fresh_var()
        };

        if let Some(init) = &local.init {
            let init_ty = self.infer_expr(&init.expr);
            let _ = self.unify(&ty, &init_ty, local.span);

            // Borrow check: if binding a reference, track the borrow
            let var_name = match &local.pattern.kind {
                ast::PatternKind::Ident { name, .. } => Some(name.name.as_ref().to_string()),
                _ => None,
            };
            if let Some(ref name) = var_name {
                self.check_borrow_at_binding(name, &init.expr, &ty, local.span);
            }
        }

        // Bind pattern variables
        self.bind_pattern(&local.pattern, &self.apply(&ty));
    }

    // =========================================================================
    // JUMP INFERENCE
    // =========================================================================

    fn infer_return(&mut self, value: Option<&ast::Expr>, span: Span) -> Ty {
        if !self.ctx.in_function() {
            self.error(TypeError::ReturnOutsideFunction, span);
            return Ty::never();
        }

        let value_ty = if let Some(expr) = value {
            let ty = self.infer_expr(expr);

            // Borrow check: if returning a reference, verify it doesn't
            // point to a local variable (which would be destroyed on return).
            if let Some(return_expr) = value {
                self.check_return_reference(return_expr, &ty, span);
            }

            ty
        } else {
            Ty::unit()
        };

        if let Some(expected) = self.return_ty.clone() {
            let _ = self.unify(&value_ty, &expected, span);
        }

        self.has_return = true;
        Ty::never()
    }

    /// Check that a returned reference doesn't point to a local variable.
    fn check_return_reference(&mut self, expr: &ast::Expr, ty: &Ty, span: Span) {
        let resolved = self.apply(ty);
        if !matches!(&resolved.kind, TyKind::Ref(_, _, _)) {
            return; // Not returning a reference — nothing to check
        }

        // If the expression is &local_var, that's always a bug
        match &expr.kind {
            ExprKind::Ref { expr: inner, .. } => {
                if let ExprKind::Ident(ident) = &inner.kind {
                    self.error(
                        TypeError::ReferenceEscapesScope {
                            variable: ident.name.to_string(),
                        },
                        span,
                    );
                }
            }
            ExprKind::Unary {
                op: UnaryOp::Ref | UnaryOp::RefMut,
                expr: inner,
            } => {
                if let ExprKind::Ident(ident) = &inner.kind {
                    self.error(
                        TypeError::ReferenceEscapesScope {
                            variable: ident.name.to_string(),
                        },
                        span,
                    );
                }
            }
            _ => {}
        }
    }

    fn infer_break(&mut self, value: Option<&ast::Expr>, span: Span) -> Ty {
        if !self.ctx.in_loop() {
            self.error(TypeError::BreakOutsideLoop, span);
        }

        if let Some(expr) = value {
            let _ = self.infer_expr(expr);
        }

        Ty::never()
    }

    fn infer_continue(&mut self, span: Span) -> Ty {
        if !self.ctx.in_loop() {
            self.error(TypeError::ContinueOutsideLoop, span);
        }
        Ty::never()
    }

    // =========================================================================
    // CLOSURE INFERENCE
    // =========================================================================

    fn infer_closure(
        &mut self,
        params: &[ast::ClosureParam],
        return_type: Option<&ast::Type>,
        body: &ast::Expr,
        _span: Span,
    ) -> Ty {
        self.ctx.push_scope(ScopeKind::Function);

        let param_tys: Vec<Ty> = params
            .iter()
            .map(|p| {
                let ty = if let Some(ty_ast) = &p.ty {
                    self.lower_type(ty_ast)
                } else {
                    Ty::fresh_var()
                };
                self.bind_pattern(&p.pattern, &ty);
                ty
            })
            .collect();

        let expected_ret = return_type.map(|t| self.lower_type(t));
        let old_return_ty = self.return_ty.take();
        self.return_ty = expected_ret.clone();

        let body_ty = self.infer_expr(body);

        if let Some(expected) = &expected_ret {
            let _ = self.unify(&body_ty, expected, body.span);
        }

        self.return_ty = old_return_ty;
        self.ctx.pop_scope();

        Ty::function(param_tys, self.apply(&body_ty))
    }

    // =========================================================================
    // TYPE OPERATIONS
    // =========================================================================

    fn infer_cast(&mut self, expr: &ast::Expr, ty: &ast::Type, _span: Span) -> Ty {
        let _ = self.infer_expr(expr);
        self.lower_type(ty)
    }

    fn infer_ref(&mut self, mutability: ast::Mutability, expr: &ast::Expr) -> Ty {
        let inner_ty = self.infer_expr(expr);
        // Borrow checking happens at the let-binding site (infer_local),
        // not here. Temporary references (&x passed directly to a function)
        // are consumed immediately and don't need tracking.
        let mut_ = match mutability {
            ast::Mutability::Mutable => Mutability::Mutable,
            ast::Mutability::Immutable => Mutability::Immutable,
        };
        Ty::reference(None, mut_, inner_ty)
    }

    /// Check borrow rules when a reference is stored in a let binding.
    /// Called from infer_local when the initializer produces a reference type.
    fn check_borrow_at_binding(
        &mut self,
        var_name: &str,
        init_expr: &ast::Expr,
        ty: &Ty,
        span: crate::lexer::Span,
    ) {
        let resolved = self.apply(ty);
        let is_ref = matches!(&resolved.kind, TyKind::Ref(_, _, _));
        if !is_ref {
            return;
        }

        let is_mut = matches!(&resolved.kind, TyKind::Ref(_, Mutability::Mutable, _));

        // Extract borrowed variable names from the initializer.
        // Case 1: Direct reference — let r = &x;
        // Case 2: Function call returning reference — let r = pick(&x, &y);
        let borrowed_vars: Vec<String> = match &init_expr.kind {
            ExprKind::Ref { expr: inner, .. } => {
                if let ExprKind::Ident(ident) = &inner.kind {
                    vec![ident.name.as_ref().to_string()]
                } else {
                    vec![]
                }
            }
            ExprKind::Call { func, args } => {
                // Interprocedural: resolve the callee to check for lifetime params.
                let func_ty = self.infer_expr(func);
                let func_ty = self.apply(&func_ty);
                if let TyKind::Fn(ref fn_ty) = &func_ty.kind {
                    if !fn_ty.lifetime_params.is_empty() {
                        // Lifetime-guided: only borrow from args whose param
                        // lifetime matches the return type's lifetime.
                        let ret_lt_name = if let TyKind::Ref(Some(ref lt), _, _) = &fn_ty.ret.kind
                        {
                            Some(lt.name.clone())
                        } else {
                            None
                        };
                        fn_ty
                            .params
                            .iter()
                            .zip(args.iter())
                            .filter_map(|(param_ty, arg)| {
                                if let TyKind::Ref(Some(ref param_lt), _, _) = &param_ty.kind {
                                    let matches = ret_lt_name
                                        .as_ref()
                                        .map(|n| param_lt.name == *n)
                                        .unwrap_or(true);
                                    if matches {
                                        if let ExprKind::Ref { expr: inner, .. } = &arg.kind {
                                            if let ExprKind::Ident(ident) = &inner.kind {
                                                return Some(ident.name.as_ref().to_string());
                                            }
                                        }
                                    }
                                }
                                None
                            })
                            .collect()
                    } else {
                        // No lifetime params: apply elision rules
                        let ref_param_indices: Vec<usize> = fn_ty
                            .params
                            .iter()
                            .enumerate()
                            .filter(|(_, p)| matches!(&p.kind, TyKind::Ref(_, _, _)))
                            .map(|(i, _)| i)
                            .collect();

                        if ref_param_indices.len() == 1 {
                            // Single ref param → only that arg is a borrow source
                            let idx = ref_param_indices[0];
                            args.get(idx)
                                .and_then(|arg| {
                                    if let ExprKind::Ref { expr: inner, .. } = &arg.kind {
                                        if let ExprKind::Ident(ident) = &inner.kind {
                                            return Some(ident.name.as_ref().to_string());
                                        }
                                    }
                                    None
                                })
                                .into_iter()
                                .collect()
                        } else {
                            // Multiple ref params: conservative fallback
                            args.iter()
                                .filter_map(|arg| {
                                    if let ExprKind::Ref { expr: inner, .. } = &arg.kind {
                                        if let ExprKind::Ident(ident) = &inner.kind {
                                            return Some(ident.name.as_ref().to_string());
                                        }
                                    }
                                    None
                                })
                                .collect()
                        }
                    }
                } else {
                    // Not a known function type: heuristic fallback
                    args.iter()
                        .filter_map(|arg| {
                            if let ExprKind::Ref { expr: inner, .. } = &arg.kind {
                                if let ExprKind::Ident(ident) = &inner.kind {
                                    return Some(ident.name.as_ref().to_string());
                                }
                            }
                            None
                        })
                        .collect()
                }
            }
            ExprKind::MethodCall { args, .. } => {
                // Method calls: use heuristic for now (lifetime-aware method
                // resolution is deferred to Phase 2).
                args.iter()
                    .filter_map(|arg| {
                        if let ExprKind::Ref { expr: inner, .. } = &arg.kind {
                            if let ExprKind::Ident(ident) = &inner.kind {
                                return Some(ident.name.as_ref().to_string());
                            }
                        }
                        None
                    })
                    .collect()
            }
            _ => vec![],
        };

        for source_var in &borrowed_vars {
            if is_mut {
                if self.borrow_state.has_any_borrow(source_var) {
                    self.error(
                        TypeError::DoubleMutableBorrow {
                            variable: source_var.clone(),
                        },
                        span,
                    );
                }
            } else {
                if self.borrow_state.has_mut_borrow(source_var) {
                    self.error(
                        TypeError::AlreadyBorrowed {
                            variable: source_var.clone(),
                        },
                        span,
                    );
                }
            }
            self.borrow_state.add_borrow(
                Arc::from(source_var.as_str()),
                Some(Arc::from(var_name)),
                is_mut,
            );
        }
    }

    fn infer_deref(&mut self, expr: &ast::Expr, span: Span) -> Ty {
        let expr_ty = self.infer_expr(expr);
        let expr_ty = self.apply(&expr_ty);

        match &expr_ty.kind {
            TyKind::Ref(_, _, inner) | TyKind::Ptr(_, inner) => (**inner).clone(),
            // Inference variables — allow deref, return fresh var
            TyKind::Var(_) | TyKind::Infer(_) => Ty::fresh_var(),
            TyKind::Error => Ty::error(),
            // Primitives: deref is an error (not a reference type).
            // Pattern-bound variables from &self should use auto-deref, not *.
            TyKind::Int(_) | TyKind::Float(_) | TyKind::Bool | TyKind::Char => {
                self.error(TypeError::NotDereferenceable { ty: expr_ty }, span);
                Ty::error()
            }
            _ => {
                self.error(TypeError::NotDereferenceable { ty: expr_ty }, span);
                Ty::error()
            }
        }
    }

    // =========================================================================
    // OTHER INFERENCE
    // =========================================================================

    fn infer_range(
        &mut self,
        start: Option<&ast::Expr>,
        end: Option<&ast::Expr>,
        inclusive: bool,
        span: Span,
    ) -> Ty {
        let elem_ty = if let Some(start) = start {
            self.infer_expr(start)
        } else if let Some(end) = end {
            self.infer_expr(end)
        } else {
            Ty::fresh_var()
        };

        if let (Some(start), Some(end)) = (start, end) {
            let start_ty = self.infer_expr(start);
            let end_ty = self.infer_expr(end);
            let _ = self.unify(&start_ty, &end_ty, span);
        }

        // Return Range<T> or RangeInclusive<T> type
        self.make_range_type(self.apply(&elem_ty), inclusive)
    }

    fn infer_try(&mut self, expr: &ast::Expr, _span: Span) -> Ty {
        let expr_ty = self.infer_expr(expr);
        let expr_ty = self.apply(&expr_ty);

        // Resolve Try trait to get the Output type (e.g., T from Result<T, E> or Option<T>)
        self.resolve_try_output(&expr_ty)
    }

    fn infer_await(&mut self, expr: &ast::Expr, _span: Span) -> Ty {
        let expr_ty = self.infer_expr(expr);
        let expr_ty = self.apply(&expr_ty);

        // Resolve Future trait to get Output type
        self.resolve_future_output(&expr_ty)
    }

    fn infer_struct(
        &mut self,
        path: &ast::Path,
        fields: &[ast::FieldExpr],
        rest: Option<&ast::Expr>,
        span: Span,
    ) -> Ty {
        // Look up struct definition by path
        let struct_name = path.last_ident().map(|i| i.name.as_ref()).unwrap_or("");

        // Extract struct info before mutable borrows to avoid borrow conflicts
        let struct_info = self
            .ctx
            .lookup_type_by_name(struct_name)
            .and_then(|type_def| {
                if let TypeDefKind::Struct(struct_def) = &type_def.kind {
                    Some((
                        type_def.def_id,
                        type_def.generics.len(),
                        struct_def.fields.clone(),
                    ))
                } else {
                    None
                }
            });

        if let Some((def_id, generics_len, struct_fields)) = struct_info {
            // Create fresh type parameters for generics
            let substs: Vec<Ty> = (0..generics_len).map(|_| Ty::fresh_var()).collect();

            // Check each field
            for field in fields {
                let field_name = field.name.as_ref();
                let value_ty = if let Some(value) = &field.value {
                    self.infer_expr(value)
                } else {
                    // Shorthand field: `field` means `field: field`
                    if let Some(var_ty) = self.ctx.lookup_var(field_name) {
                        var_ty
                    } else {
                        self.error(
                            TypeError::UndefinedVariable {
                                name: field_name.to_string(),
                            },
                            span,
                        );
                        Ty::error()
                    }
                };

                // Find expected field type and unify
                if let Some((_, expected_ty)) =
                    struct_fields.iter().find(|(n, _)| n.as_ref() == field_name)
                {
                    let _ = self.unify(expected_ty, &value_ty, span);
                } else {
                    self.error(
                        TypeError::UndefinedField {
                            ty: Ty::adt(def_id, substs.clone()),
                            field: field_name.to_string(),
                        },
                        span,
                    );
                }
            }

            // Handle struct update syntax (..other)
            if let Some(rest_expr) = rest {
                let rest_ty = self.infer_expr(rest_expr);
                let struct_ty = Ty::adt(def_id, substs.clone());
                let _ = self.unify(&rest_ty, &struct_ty, span);
            }

            return Ty::adt(def_id, substs);
        }

        // Fallback: infer field types but return fresh variable
        for field in fields {
            if let Some(value) = &field.value {
                let _ = self.infer_expr(value);
            }
        }
        if let Some(rest_expr) = rest {
            let _ = self.infer_expr(rest_expr);
        }
        Ty::fresh_var()
    }

    // =========================================================================
    // PATTERN CHECKING
    // =========================================================================

    fn check_pattern(&mut self, pattern: &ast::Pattern, expected: &Ty) {
        // Bind pattern variables and check types
        self.bind_pattern(pattern, expected);
    }

    fn bind_pattern(&mut self, pattern: &ast::Pattern, ty: &Ty) {
        match &pattern.kind {
            ast::PatternKind::Wildcard => {}
            ast::PatternKind::Ident { name, .. } => {
                self.ctx.define_var(name.name.clone(), ty.clone());
            }
            ast::PatternKind::Tuple(patterns) => {
                match &ty.kind {
                    TyKind::Tuple(elem_tys) => {
                        for (pat, elem_ty) in patterns.iter().zip(elem_tys.iter()) {
                            self.bind_pattern(pat, elem_ty);
                        }
                    }
                    // For inference variables or unknown types, generate fresh
                    // vars for each pattern element so variables get bound.
                    _ => {
                        for pat in patterns {
                            self.bind_pattern(pat, &Ty::fresh_var());
                        }
                    }
                }
            }
            ast::PatternKind::Struct { path, fields, .. } => {
                // Look up struct definition to get field types
                let struct_name = path.last_ident().map(|i| &*i.name);

                // Extract field types before recursive calls to avoid borrow conflicts
                let field_types: Vec<_> = fields
                    .iter()
                    .map(|field| {
                        let field_name = field.name.as_ref();

                        // Try to get field type from ADT type
                        if let TyKind::Adt(def_id, _substs) = &ty.kind {
                            if let Some(type_def) = self.ctx.lookup_type(*def_id) {
                                if let TypeDefKind::Struct(struct_def) = &type_def.kind {
                                    if let Some((_, t)) = struct_def
                                        .fields
                                        .iter()
                                        .find(|(n, _)| n.as_ref() == field_name)
                                    {
                                        return t.clone();
                                    }
                                }
                            }
                        }
                        // Try by name
                        if let Some(name) = struct_name {
                            if let Some(type_def) = self.ctx.lookup_type_by_name(name) {
                                if let TypeDefKind::Struct(struct_def) = &type_def.kind {
                                    if let Some((_, t)) = struct_def
                                        .fields
                                        .iter()
                                        .find(|(n, _)| n.as_ref() == field_name)
                                    {
                                        return t.clone();
                                    }
                                }
                            }
                        }
                        Ty::fresh_var()
                    })
                    .collect();

                // Now bind patterns with the extracted types
                for (field, field_ty) in fields.iter().zip(field_types.iter()) {
                    self.bind_pattern(&field.pattern, field_ty);
                }
            }
            ast::PatternKind::TupleStruct {
                path: _, patterns, ..
            } => {
                // Extract field types before recursive calls to avoid borrow conflicts
                let field_types: Vec<_> = patterns
                    .iter()
                    .enumerate()
                    .map(|(i, _)| {
                        if let TyKind::Adt(def_id, _) = &ty.kind {
                            if let Some(type_def) = self.ctx.lookup_type(*def_id) {
                                if let TypeDefKind::Struct(struct_def) = &type_def.kind {
                                    if struct_def.is_tuple {
                                        return struct_def
                                            .fields
                                            .get(i)
                                            .map(|(_, t)| t.clone())
                                            .unwrap_or_else(Ty::fresh_var);
                                    }
                                }
                            }
                        }
                        Ty::fresh_var()
                    })
                    .collect();

                // Now bind patterns with the extracted types
                for (pattern, field_ty) in patterns.iter().zip(field_types.iter()) {
                    self.bind_pattern(pattern, field_ty);
                }
            }
            ast::PatternKind::Slice(patterns) => {
                let elem_ty = if let TyKind::Slice(elem) | TyKind::Array(elem, _) = &ty.kind {
                    (**elem).clone()
                } else {
                    Ty::fresh_var()
                };
                for pat in patterns {
                    self.bind_pattern(pat, &elem_ty);
                }
            }
            ast::PatternKind::Or(patterns) => {
                for pat in patterns {
                    self.bind_pattern(pat, ty);
                }
            }
            ast::PatternKind::Ref { pattern, .. } => {
                if let TyKind::Ref(_, _, inner) = &ty.kind {
                    self.bind_pattern(pattern, inner);
                } else {
                    // When a `&x` pattern is used but the type isn't Ref
                    // (e.g. iterators yielding owned values), bind the inner
                    // pattern to the type directly — the `&` just dereferences.
                    self.bind_pattern(pattern, ty);
                }
            }
            _ => {}
        }
    }

    // =========================================================================
    // TYPE LOWERING
    // =========================================================================

    /// Lower an AST type to an internal type.
    pub fn lower_type(&mut self, ty: &ast::Type) -> Ty {
        match &ty.kind {
            ast::TypeKind::Never => Ty::never(),
            ast::TypeKind::Infer => Ty::fresh_var(),
            ast::TypeKind::Tuple(elems) => {
                Ty::tuple(elems.iter().map(|t| self.lower_type(t)).collect())
            }
            ast::TypeKind::Array { elem, len } => {
                let elem_ty = self.lower_type(elem);
                // Evaluate const length
                let size = self.eval_const_expr(len).unwrap_or(0);
                Ty::array(elem_ty, size)
            }
            ast::TypeKind::Slice(elem) => Ty::slice(self.lower_type(elem)),
            ast::TypeKind::Ref {
                lifetime,
                mutability,
                ty: inner,
            } => {
                let lt = lifetime
                    .as_ref()
                    .map(|l| Lifetime::new(l.name.name.as_ref()));
                let mut_ = match mutability {
                    ast::Mutability::Mutable => Mutability::Mutable,
                    ast::Mutability::Immutable => Mutability::Immutable,
                };
                Ty::reference(lt, mut_, self.lower_type(inner))
            }
            ast::TypeKind::Ptr {
                mutability,
                ty: inner,
            } => {
                let mut_ = match mutability {
                    ast::Mutability::Mutable => Mutability::Mutable,
                    ast::Mutability::Immutable => Mutability::Immutable,
                };
                Ty::ptr(mut_, self.lower_type(inner))
            }
            ast::TypeKind::BareFn {
                params,
                return_ty,
                is_unsafe,
                ..
            } => {
                let param_tys: Vec<_> = params.iter().map(|p| self.lower_type(&p.ty)).collect();
                let ret = return_ty
                    .as_ref()
                    .map(|t| self.lower_type(t))
                    .unwrap_or(Ty::unit());
                Ty::new(TyKind::Fn(FnTy {
                    params: param_tys,
                    ret: Box::new(ret),
                    is_unsafe: *is_unsafe,
                    abi: None,
                    effects: super::effects::EffectRow::empty(),
                    lifetime_params: Vec::new(),
                }))
            }
            ast::TypeKind::Path(path) => self.lower_type_path(path),
            ast::TypeKind::TraitObject { bounds, .. } => {
                let trait_names: Vec<Arc<str>> = bounds
                    .iter()
                    .map(|b| {
                        b.path
                            .last_ident()
                            .map(|i| i.name.clone())
                            .unwrap_or(Arc::from("Unknown"))
                    })
                    .collect();
                Ty::new(TyKind::TraitObject(trait_names))
            }
            ast::TypeKind::WithEffect { ty: inner, effects } => {
                // Lower the base type with color space annotations stored on the Ty.
                // During unification, annotated types must match their annotations.
                let mut base_ty = self.lower_type(inner);
                // Extract annotation strings from effect paths
                for effect in effects {
                    if let Some(ident) = effect.last_ident() {
                        if let Some(generics) = effect.last_generics() {
                            // Generic annotation: ColorSpace<Linear> → "ColorSpace:Linear"
                            for g in generics {
                                if let ast::GenericArg::Type(arg_ty) = g {
                                    if let ast::TypeKind::Path(p) = &arg_ty.kind {
                                        if let Some(arg_id) = p.last_ident() {
                                            let ann = format!("{}:{}", ident.name, arg_id.name);
                                            base_ty.annotations.push(Arc::from(ann.as_str()));
                                        }
                                    }
                                }
                            }
                        } else {
                            // Simple annotation: Pure → "Pure"
                            base_ty.annotations.push(ident.name.clone());
                        }
                    }
                }
                base_ty
            }
            _ => Ty::fresh_var(),
        }
    }

    fn lower_type_path(&mut self, path: &ast::Path) -> Ty {
        // Check if it's a primitive type
        if let Some(ident) = path.last_ident() {
            let name = ident.name.as_ref();

            // Check for Self type first (resolves to the impl's self type)
            if name == "Self" {
                if let Some(self_ty) = self.ctx.get_self_ty() {
                    return self_ty.clone();
                }
            }

            // Check for primitive types
            if let Some(ty) = self.lookup_primitive(name) {
                return ty;
            }

            // Check type parameters in scope
            if let Some(ty) = self.ctx.lookup_type_param(name) {
                return ty.clone();
            }

            // Look up type definition by name
            if let Some(type_def) = self.ctx.lookup_type_by_name(name) {
                let def_id = type_def.def_id;
                // Get type arguments from path if present
                let substs: Vec<Ty> = if let Some(generics) = path.last_generics() {
                    generics
                        .iter()
                        .filter_map(|arg| {
                            if let ast::GenericArg::Type(ty) = arg {
                                Some(self.lower_type(&ty))
                            } else {
                                None
                            }
                        })
                        .collect()
                } else {
                    // Fresh type variables for unspecified generics
                    type_def.generics.iter().map(|_| Ty::fresh_var()).collect()
                };
                return Ty::adt(def_id, substs);
            }
        }

        // Fallback: return fresh variable for unknown types
        Ty::fresh_var()
    }

    fn lower_primitive(&mut self, prim: &ast::PrimitiveType) -> Ty {
        match prim {
            ast::PrimitiveType::Bool => Ty::bool(),
            ast::PrimitiveType::Char => Ty::char(),
            ast::PrimitiveType::Str => Ty::str(),
            ast::PrimitiveType::I8 => Ty::int(IntTy::I8),
            ast::PrimitiveType::I16 => Ty::int(IntTy::I16),
            ast::PrimitiveType::I32 => Ty::int(IntTy::I32),
            ast::PrimitiveType::I64 => Ty::int(IntTy::I64),
            ast::PrimitiveType::I128 => Ty::int(IntTy::I128),
            ast::PrimitiveType::Isize => Ty::int(IntTy::Isize),
            ast::PrimitiveType::U8 => Ty::int(IntTy::U8),
            ast::PrimitiveType::U16 => Ty::int(IntTy::U16),
            ast::PrimitiveType::U32 => Ty::int(IntTy::U32),
            ast::PrimitiveType::U64 => Ty::int(IntTy::U64),
            ast::PrimitiveType::U128 => Ty::int(IntTy::U128),
            ast::PrimitiveType::Usize => Ty::int(IntTy::Usize),
            ast::PrimitiveType::F16 => Ty::float(FloatTy::F16),
            ast::PrimitiveType::F32 => Ty::float(FloatTy::F32),
            ast::PrimitiveType::F64 => Ty::float(FloatTy::F64),
        }
    }

    fn lookup_primitive(&self, name: &str) -> Option<Ty> {
        match name {
            "bool" => Some(Ty::bool()),
            "char" => Some(Ty::char()),
            "str" => Some(Ty::str()),
            "i8" => Some(Ty::int(IntTy::I8)),
            "i16" => Some(Ty::int(IntTy::I16)),
            "i32" => Some(Ty::int(IntTy::I32)),
            "i64" => Some(Ty::int(IntTy::I64)),
            "i128" => Some(Ty::int(IntTy::I128)),
            "isize" => Some(Ty::int(IntTy::Isize)),
            "u8" => Some(Ty::int(IntTy::U8)),
            "u16" => Some(Ty::int(IntTy::U16)),
            "u32" => Some(Ty::int(IntTy::U32)),
            "u64" => Some(Ty::int(IntTy::U64)),
            "u128" => Some(Ty::int(IntTy::U128)),
            "usize" => Some(Ty::int(IntTy::Usize)),
            "f32" => Some(Ty::float(FloatTy::F32)),
            "f64" => Some(Ty::float(FloatTy::F64)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::effects::{Effect, EffectRow};
    use super::super::unify::{self, Unifier};
    use super::*;
    use crate::ast::{self, ExprKind, Literal as AstLiteral, NodeId};
    use crate::lexer::Span;

    // =====================================================================
    // Test helpers
    // =====================================================================

    fn dummy_span() -> Span {
        Span::dummy()
    }

    fn make_expr(kind: ExprKind) -> ast::Expr {
        ast::Expr {
            kind,
            span: dummy_span(),
            id: NodeId::DUMMY,
            attrs: Vec::new(),
        }
    }

    fn int_lit(value: u128, suffix: Option<ast::IntSuffix>) -> AstLiteral {
        AstLiteral::Int {
            value,
            suffix,
            base: crate::lexer::IntBase::Decimal,
        }
    }

    fn float_lit(value: f64, suffix: Option<ast::FloatSuffix>) -> AstLiteral {
        AstLiteral::Float { value, suffix }
    }

    // =====================================================================
    // 1. Literal inference
    // =====================================================================

    #[test]
    fn unsuffixed_int_gets_infer_int() {
        // Unsuffixed integer literals must produce InferKind::Int so that
        // later unification can default them to i32 or coerce to context.
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);
        let ty = infer.infer_literal(&int_lit(42, None));

        match &ty.kind {
            TyKind::Infer(it) => assert_eq!(it.kind, InferKind::Int),
            other => panic!("expected Infer(Int), got {:?}", other),
        }
    }

    #[test]
    fn suffixed_int_i32() {
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);
        let ty = infer.infer_literal(&int_lit(42, Some(ast::IntSuffix::I32)));
        assert_eq!(ty, Ty::int(IntTy::I32));
    }

    #[test]
    fn suffixed_int_u64() {
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);
        let ty = infer.infer_literal(&int_lit(0, Some(ast::IntSuffix::U64)));
        assert_eq!(ty, Ty::int(IntTy::U64));
    }

    #[test]
    fn unsuffixed_float_gets_infer_float() {
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);
        let ty = infer.infer_literal(&float_lit(3.14, None));

        match &ty.kind {
            TyKind::Infer(it) => assert_eq!(it.kind, InferKind::Float),
            other => panic!("expected Infer(Float), got {:?}", other),
        }
    }

    #[test]
    fn suffixed_float_f32() {
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);
        let ty = infer.infer_literal(&float_lit(1.0, Some(ast::FloatSuffix::F32)));
        assert_eq!(ty, Ty::float(FloatTy::F32));
    }

    #[test]
    fn bool_literal() {
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);
        assert_eq!(infer.infer_literal(&AstLiteral::Bool(true)), Ty::bool());
        assert_eq!(infer.infer_literal(&AstLiteral::Bool(false)), Ty::bool());
    }

    #[test]
    fn char_literal() {
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);
        assert_eq!(infer.infer_literal(&AstLiteral::Char('x')), Ty::char());
    }

    #[test]
    fn str_literal_is_owned_str() {
        // QuantaLang strings are owned; string literals get `str` not `&str`.
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);
        let ty = infer.infer_literal(&AstLiteral::Str {
            value: "hello".into(),
            is_raw: false,
        });
        assert_eq!(ty, Ty::str());
    }

    #[test]
    fn byte_literal_is_u8() {
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);
        assert_eq!(
            infer.infer_literal(&AstLiteral::Byte(b'A')),
            Ty::int(IntTy::U8)
        );
    }

    #[test]
    fn byte_string_is_ref_u8_array() {
        // b"hello" should be &'static [u8; 5]
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);
        let ty = infer.infer_literal(&AstLiteral::ByteStr {
            value: b"hello".to_vec(),
            is_raw: false,
        });
        let expected = Ty::reference(
            Some(Lifetime::static_lifetime()),
            Mutability::Immutable,
            Ty::array(Ty::int(IntTy::U8), 5),
        );
        assert_eq!(ty, expected);
    }

    // =====================================================================
    // 2. Unification properties
    // =====================================================================

    #[test]
    fn unify_reflexivity_primitives() {
        // Reflexivity: unify(T, T) must succeed for any concrete T.
        // This is the identity law of the unification relation.
        for ty in &[
            Ty::int(IntTy::I32),
            Ty::bool(),
            Ty::char(),
            Ty::str(),
            Ty::float(FloatTy::F64),
            Ty::unit(),
            Ty::never(),
        ] {
            let subst = unify::unify(ty, ty).unwrap();
            assert!(
                subst.is_empty(),
                "reflexive unify of {} produced bindings",
                ty
            );
        }
    }

    #[test]
    fn unify_reflexivity_compound() {
        // Reflexivity for compound types: tuples, arrays, functions.
        let tuple = Ty::tuple(vec![Ty::int(IntTy::I32), Ty::bool()]);
        assert!(unify::unify(&tuple, &tuple).unwrap().is_empty());

        let arr = Ty::array(Ty::int(IntTy::I32), 3);
        assert!(unify::unify(&arr, &arr).unwrap().is_empty());

        let func = Ty::function(vec![Ty::int(IntTy::I32)], Ty::bool());
        assert!(unify::unify(&func, &func).unwrap().is_empty());
    }

    #[test]
    fn unify_symmetry() {
        // Symmetry: if unify(A, B) binds ?a -> T, then unify(B, A) must
        // produce the same resolved type. Order must not matter.
        let v = TyVarId::fresh();
        let var = Ty::var(v);
        let concrete = Ty::int(IntTy::I64);

        let s1 = unify::unify(&var, &concrete).unwrap();
        assert_eq!(s1.get(v), Some(&concrete));

        let v2 = TyVarId::fresh();
        let var2 = Ty::var(v2);
        let s2 = unify::unify(&concrete, &var2).unwrap();
        assert_eq!(s2.get(v2), Some(&concrete));
    }

    #[test]
    fn unify_symmetry_two_vars() {
        // When unifying two vars, direction should not matter for resolution.
        let a = TyVarId::fresh();
        let b = TyVarId::fresh();

        let mut u = Unifier::new();
        u.unify(&Ty::var(a), &Ty::var(b)).unwrap();
        u.unify(&Ty::var(b), &Ty::int(IntTy::I32)).unwrap();

        // Both must resolve to i32 regardless of which was unified first.
        assert_eq!(u.apply(&Ty::var(a)), Ty::int(IntTy::I32));
        assert_eq!(u.apply(&Ty::var(b)), Ty::int(IntTy::I32));
    }

    #[test]
    fn occurs_check_prevents_infinite_type() {
        // Occurs check: unifying ?a with a type that contains ?a must fail.
        // Without this, the substitution {?a -> (?a, bool)} would loop forever.
        let v = TyVarId::fresh();
        let var = Ty::var(v);
        let cyclic = Ty::tuple(vec![var.clone(), Ty::bool()]);

        let result = unify::unify(&var, &cyclic);
        assert!(
            result.is_err(),
            "occurs check should reject ?a ~ (?a, bool)"
        );
    }

    #[test]
    fn occurs_check_nested() {
        // Nested occurs check: ?a ~ fn(?a) -> bool
        let v = TyVarId::fresh();
        let var = Ty::var(v);
        let func = Ty::function(vec![var.clone()], Ty::bool());

        assert!(unify::unify(&var, &func).is_err());
    }

    #[test]
    fn occurs_check_through_array() {
        let v = TyVarId::fresh();
        let var = Ty::var(v);
        let arr = Ty::array(var.clone(), 1);

        assert!(unify::unify(&var, &arr).is_err());
    }

    #[test]
    fn never_is_bottom_type() {
        // Never (!) is the bottom type: it unifies with everything.
        // This is essential for soundness of diverging expressions.
        let never = Ty::never();
        for ty in &[
            Ty::int(IntTy::I32),
            Ty::bool(),
            Ty::str(),
            Ty::tuple(vec![Ty::int(IntTy::I32)]),
            Ty::function(vec![], Ty::unit()),
        ] {
            assert!(
                unify::unify(&never, ty).is_ok(),
                "! should unify with {}",
                ty
            );
            assert!(
                unify::unify(ty, &never).is_ok(),
                "{} should unify with !",
                ty
            );
        }
    }

    #[test]
    fn error_type_absorbs() {
        // Error type unifies with anything for error recovery.
        let err = Ty::error();
        assert!(unify::unify(&err, &Ty::int(IntTy::I32)).is_ok());
        assert!(unify::unify(&Ty::bool(), &err).is_ok());
    }

    #[test]
    fn infer_var_unifies_like_regular_var() {
        // InferKind::Int variables bind to concrete types through unification,
        // just like plain Var — they just carry fallback info.
        let infer_ty = Ty::new(TyKind::Infer(InferTy {
            var: TyVarId::fresh(),
            kind: InferKind::Int,
        }));
        let concrete = Ty::int(IntTy::I32);
        assert!(unify::unify(&infer_ty, &concrete).is_ok());
    }

    // =====================================================================
    // 3. Bidirectional flow
    // =====================================================================

    #[test]
    fn check_expr_constrains_literal() {
        // Checking mode: check_expr(expr, expected) should unify the
        // inferred type with the expected type. A bare integer literal
        // checked against i64 should resolve without error.
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);

        let expr = make_expr(ExprKind::Literal(int_lit(42, None)));
        let result = infer.check_expr(&expr, &Ty::int(IntTy::I64));

        // Result should be compatible with i64 (InferKind::Int unifies with any int)
        assert!(
            infer.errors().is_empty(),
            "check_expr should not produce errors"
        );
        // The resolved type might still be Infer(Int) or i64 depending on
        // whether the unifier collapsed it, but no error is the key property.
        let _ = result;
    }

    #[test]
    fn check_expr_rejects_type_mismatch() {
        // Checking mode must reject impossible coercions: bool vs i32.
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);

        let expr = make_expr(ExprKind::Literal(AstLiteral::Bool(true)));
        let _ = infer.check_expr(&expr, &Ty::int(IntTy::I32));

        assert!(
            !infer.errors().is_empty(),
            "bool checked against i32 should error"
        );
    }

    #[test]
    fn if_else_branches_unify() {
        // if-else is a classic bidirectional case: both branches must
        // have the same type. We test this by building an if-else where
        // both branches are bool literals — the result should be bool.
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);

        let cond = make_expr(ExprKind::Literal(AstLiteral::Bool(true)));
        let then_expr = make_expr(ExprKind::Literal(AstLiteral::Bool(true)));
        let else_expr = make_expr(ExprKind::Literal(AstLiteral::Bool(false)));

        let then_block = ast::Block {
            stmts: vec![ast::Stmt::new(
                ast::StmtKind::Expr(Box::new(then_expr)),
                dummy_span(),
            )],
            span: dummy_span(),
            id: NodeId::DUMMY,
        };

        let result = infer.infer_if(&cond, &then_block, Some(&else_expr), dummy_span());
        assert!(infer.errors().is_empty());
        assert_eq!(result, Ty::bool());
    }

    #[test]
    fn if_without_else_is_unit() {
        // if without else must return unit, since the else path implicitly
        // yields (). The then-branch is also unified with unit.
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);

        let cond = make_expr(ExprKind::Literal(AstLiteral::Bool(true)));
        let then_expr = make_expr(ExprKind::Literal(AstLiteral::Bool(true)));
        let then_block = ast::Block {
            stmts: vec![ast::Stmt::new(
                ast::StmtKind::Semi(Box::new(then_expr)),
                dummy_span(),
            )],
            span: dummy_span(),
            id: NodeId::DUMMY,
        };

        let result = infer.infer_if(&cond, &then_block, None, dummy_span());
        assert_eq!(result, Ty::unit());
    }

    #[test]
    fn if_else_mismatched_branches_errors() {
        // If then-branch is bool and else-branch is i32, unification should
        // fail and produce a type error.
        let mut ctx = TypeContext::new();
        let mut infer = TypeInfer::new(&mut ctx);

        let cond = make_expr(ExprKind::Literal(AstLiteral::Bool(true)));
        let then_expr = make_expr(ExprKind::Literal(AstLiteral::Bool(true)));
        let else_expr = make_expr(ExprKind::Literal(int_lit(1, Some(ast::IntSuffix::I32))));

        let then_block = ast::Block {
            stmts: vec![ast::Stmt::new(
                ast::StmtKind::Expr(Box::new(then_expr)),
                dummy_span(),
            )],
            span: dummy_span(),
            id: NodeId::DUMMY,
        };

        let _ = infer.infer_if(&cond, &then_block, Some(&else_expr), dummy_span());
        assert!(
            !infer.errors().is_empty(),
            "mismatched if-else branches must error"
        );
    }

    // =====================================================================
    // 4. Function call inference
    // =====================================================================

    #[test]
    fn call_infers_return_type_from_signature() {
        // Given a function fn(i32) -> bool in scope, calling it with an
        // i32 argument should produce bool.
        let mut ctx = TypeContext::new();
        ctx.define_var(
            "is_positive",
            Ty::function(vec![Ty::int(IntTy::I32)], Ty::bool()),
        );

        let mut infer = TypeInfer::new(&mut ctx);

        let func = make_expr(ExprKind::Ident(ast::Ident::dummy("is_positive")));
        let arg = make_expr(ExprKind::Literal(int_lit(5, Some(ast::IntSuffix::I32))));
        let result = infer.infer_call(&func, &[arg], dummy_span());

        assert!(infer.errors().is_empty());
        assert_eq!(result, Ty::bool());
    }

    #[test]
    fn call_generic_instantiation_from_args() {
        // A generic identity function fn(?T) -> ?T should have its type
        // parameter instantiated from the argument type.
        let v = TyVarId::fresh();
        let scheme = TypeScheme::poly(vec![v], Ty::function(vec![Ty::var(v)], Ty::var(v)));

        let mut ctx = TypeContext::new();
        ctx.define_var_scheme("identity", scheme);

        let mut infer = TypeInfer::new(&mut ctx);

        let func = make_expr(ExprKind::Ident(ast::Ident::dummy("identity")));
        let arg = make_expr(ExprKind::Literal(AstLiteral::Bool(true)));
        let result = infer.infer_call(&func, &[arg], dummy_span());

        assert!(infer.errors().is_empty());
        // The return type should resolve to bool after instantiation + unification
        let resolved = infer.apply(&result);
        assert_eq!(resolved, Ty::bool());
    }

    #[test]
    fn call_arity_mismatch() {
        let mut ctx = TypeContext::new();
        ctx.define_var("f", Ty::function(vec![Ty::int(IntTy::I32)], Ty::bool()));

        let mut infer = TypeInfer::new(&mut ctx);

        let func = make_expr(ExprKind::Ident(ast::Ident::dummy("f")));
        // Pass two args to a one-param function
        let arg1 = make_expr(ExprKind::Literal(int_lit(1, Some(ast::IntSuffix::I32))));
        let arg2 = make_expr(ExprKind::Literal(int_lit(2, Some(ast::IntSuffix::I32))));
        let _ = infer.infer_call(&func, &[arg1, arg2], dummy_span());

        assert!(!infer.errors().is_empty(), "wrong arity should error");
    }

    #[test]
    fn call_infers_var_function_from_usage() {
        // When the callee is an unresolved type variable, inference should
        // construct a function type from the call-site arguments and unify.
        let a = TyVarId::fresh();
        let mut ctx = TypeContext::new();
        ctx.define_var("mystery", Ty::var(a));

        let mut infer = TypeInfer::new(&mut ctx);

        let func = make_expr(ExprKind::Ident(ast::Ident::dummy("mystery")));
        let arg = make_expr(ExprKind::Literal(AstLiteral::Bool(true)));
        let ret = infer.infer_call(&func, &[arg], dummy_span());

        assert!(infer.errors().is_empty());
        // The variable should now be bound to fn(bool) -> ?ret
        let resolved = infer.apply(&Ty::var(a));
        match &resolved.kind {
            TyKind::Fn(fn_ty) => {
                assert_eq!(fn_ty.params.len(), 1);
                assert_eq!(fn_ty.params[0], Ty::bool());
            }
            _ => panic!("expected function type, got {}", resolved),
        }
        let _ = ret;
    }

    #[test]
    fn call_propagates_callee_effects() {
        // When calling a function with effects, those effects should be
        // accumulated into the caller's effect row.
        let io_row = EffectRow::closed(vec![Effect::io()]);
        let fn_ty = Ty::function_with_effects(vec![Ty::str()], Ty::unit(), io_row);

        let mut ctx = TypeContext::new();
        ctx.define_var("print", fn_ty);

        let mut infer = TypeInfer::new(&mut ctx);
        assert!(infer.current_effect_row().is_empty());

        let func = make_expr(ExprKind::Ident(ast::Ident::dummy("print")));
        let arg = make_expr(ExprKind::Literal(AstLiteral::Str {
            value: "hi".into(),
            is_raw: false,
        }));
        let _ = infer.infer_call(&func, &[arg], dummy_span());

        assert!(infer.current_effect_row().has_io());
    }

    // =====================================================================
    // 5. Effect inference
    // =====================================================================

    #[test]
    fn pure_function_has_empty_effects() {
        // A pure function (no `with` clause) should have an empty effect row.
        let fn_ty = Ty::function(vec![Ty::int(IntTy::I32)], Ty::int(IntTy::I32));
        match &fn_ty.kind {
            TyKind::Fn(f) => assert!(f.effects.is_empty()),
            _ => panic!("expected Fn"),
        }
    }

    #[test]
    fn io_function_has_io_effect() {
        let io_row = EffectRow::closed(vec![Effect::io()]);
        let fn_ty = Ty::function_with_effects(vec![Ty::str()], Ty::unit(), io_row.clone());
        match &fn_ty.kind {
            TyKind::Fn(f) => {
                assert!(f.effects.has_io());
                assert!(!f.effects.is_empty());
            }
            _ => panic!("expected Fn"),
        }
    }

    #[test]
    fn effect_accumulation_across_calls() {
        // Calling two functions with different effects should accumulate
        // both effects in the caller's row.
        let io_row = EffectRow::closed(vec![Effect::io()]);
        let err_row = EffectRow::closed(vec![Effect::error(Ty::str())]);

        let mut ctx = TypeContext::new();
        ctx.define_var("read", Ty::function_with_effects(vec![], Ty::str(), io_row));
        ctx.define_var(
            "parse",
            Ty::function_with_effects(vec![Ty::str()], Ty::int(IntTy::I32), err_row),
        );

        let mut infer = TypeInfer::new(&mut ctx);

        let read_fn = make_expr(ExprKind::Ident(ast::Ident::dummy("read")));
        let _ = infer.infer_call(&read_fn, &[], dummy_span());

        let parse_fn = make_expr(ExprKind::Ident(ast::Ident::dummy("parse")));
        let arg = make_expr(ExprKind::Literal(AstLiteral::Str {
            value: "42".into(),
            is_raw: false,
        }));
        let _ = infer.infer_call(&parse_fn, &[arg], dummy_span());

        let effects = infer.current_effect_row();
        assert!(effects.has_io(), "should have IO from read()");
        assert!(effects.has_error(), "should have Error from parse()");
    }

    #[test]
    fn effect_row_merge_is_union() {
        // Merging two effect rows should produce the set union.
        let row1 = EffectRow::closed(vec![Effect::io()]);
        let row2 = EffectRow::closed(vec![Effect::error(Ty::str())]);
        let merged = row1.merge(&row2);

        assert!(merged.has_io());
        assert!(merged.has_error());
    }

    #[test]
    fn empty_effect_row_is_pure() {
        let row = EffectRow::empty();
        assert!(row.is_empty());
        assert!(!row.has_io());
        assert!(row.is_closed());
    }

    // =====================================================================
    // Additional properties
    // =====================================================================

    #[test]
    fn transitive_unification_chain() {
        // If ?a ~ ?b and ?b ~ ?c and ?c ~ i32, then ?a must resolve to i32.
        let a = TyVarId::fresh();
        let b = TyVarId::fresh();
        let c = TyVarId::fresh();

        let mut u = Unifier::new();
        u.unify(&Ty::var(a), &Ty::var(b)).unwrap();
        u.unify(&Ty::var(b), &Ty::var(c)).unwrap();
        u.unify(&Ty::var(c), &Ty::int(IntTy::I32)).unwrap();

        assert_eq!(u.apply(&Ty::var(a)), Ty::int(IntTy::I32));
        assert_eq!(u.apply(&Ty::var(b)), Ty::int(IntTy::I32));
        assert_eq!(u.apply(&Ty::var(c)), Ty::int(IntTy::I32));
    }

    #[test]
    fn unify_tuple_element_wise() {
        // Unifying (i32, ?a) with (i32, bool) must bind ?a -> bool.
        let a = TyVarId::fresh();
        let t1 = Ty::tuple(vec![Ty::int(IntTy::I32), Ty::var(a)]);
        let t2 = Ty::tuple(vec![Ty::int(IntTy::I32), Ty::bool()]);

        let subst = unify::unify(&t1, &t2).unwrap();
        assert_eq!(subst.get(a), Some(&Ty::bool()));
    }

    #[test]
    fn unify_function_binds_return() {
        // Unifying fn(?a) -> ?b with fn(i32) -> bool should bind both.
        let a = TyVarId::fresh();
        let b = TyVarId::fresh();

        let t1 = Ty::function(vec![Ty::var(a)], Ty::var(b));
        let t2 = Ty::function(vec![Ty::int(IntTy::I32)], Ty::bool());

        let subst = unify::unify(&t1, &t2).unwrap();
        assert_eq!(subst.get(a), Some(&Ty::int(IntTy::I32)));
        assert_eq!(subst.get(b), Some(&Ty::bool()));
    }

    #[test]
    fn unify_array_requires_same_length() {
        let a1 = Ty::array(Ty::int(IntTy::I32), 3);
        let a2 = Ty::array(Ty::int(IntTy::I32), 5);
        assert!(unify::unify(&a1, &a2).is_err(), "[i32; 3] != [i32; 5]");

        // Same length should succeed
        let a3 = Ty::array(Ty::int(IntTy::I32), 3);
        assert!(unify::unify(&a1, &a3).is_ok());
    }

    #[test]
    fn type_scheme_instantiation_is_fresh() {
        // Two instantiations of the same scheme must produce independent
        // type variables so they can unify to different concrete types.
        let v = TyVarId::fresh();
        let scheme = TypeScheme::poly(vec![v], Ty::function(vec![Ty::var(v)], Ty::var(v)));

        let inst1 = scheme.instantiate();
        let inst2 = scheme.instantiate();

        assert_ne!(inst1, inst2, "each instantiation should use fresh vars");

        // But both should have the same *shape*: fn(?a) -> ?a
        match (&inst1.kind, &inst2.kind) {
            (TyKind::Fn(f1), TyKind::Fn(f2)) => {
                assert_eq!(f1.params.len(), 1);
                assert_eq!(f2.params.len(), 1);
                // Param and return should be the same var within each instance
                assert_eq!(f1.params[0], *f1.ret);
                assert_eq!(f2.params[0], *f2.ret);
                // But different between instances
                assert_ne!(f1.params[0], f2.params[0]);
            }
            _ => panic!("expected function types"),
        }
    }

    // =====================================================================
    // Exhaustiveness checking helpers
    // =====================================================================

    #[test]
    fn extract_wildcard_covers_all() {
        let pat = ast::Pattern::wildcard(dummy_span());
        let covered = extract_covered_variants(&pat);
        assert_eq!(covered, vec!["*"]);
    }

    #[test]
    fn extract_ident_covers_all() {
        let pat = ast::Pattern::ident(ast::Ident::dummy("x"), ast::Mutability::Immutable);
        let covered = extract_covered_variants(&pat);
        assert_eq!(covered, vec!["*"]);
    }

    #[test]
    fn extract_path_variant() {
        let path = ast::Path::from_ident(ast::Ident::dummy("None"));
        let pat = ast::Pattern::new(ast::PatternKind::Path(path), dummy_span());
        let covered = extract_covered_variants(&pat);
        assert_eq!(covered, vec!["None"]);
    }

    #[test]
    fn extract_tuple_struct_variant() {
        let path = ast::Path::from_ident(ast::Ident::dummy("Some"));
        let pat = ast::Pattern::new(
            ast::PatternKind::TupleStruct {
                path,
                patterns: vec![ast::Pattern::wildcard(dummy_span())],
            },
            dummy_span(),
        );
        let covered = extract_covered_variants(&pat);
        assert_eq!(covered, vec!["Some"]);
    }

    #[test]
    fn extract_or_pattern_collects_all() {
        let p1 = ast::Pattern::new(
            ast::PatternKind::Path(ast::Path::from_ident(ast::Ident::dummy("A"))),
            dummy_span(),
        );
        let p2 = ast::Pattern::new(
            ast::PatternKind::Path(ast::Path::from_ident(ast::Ident::dummy("B"))),
            dummy_span(),
        );
        let pat = ast::Pattern::new(ast::PatternKind::Or(vec![p1, p2]), dummy_span());
        let covered = extract_covered_variants(&pat);
        assert_eq!(covered, vec!["A", "B"]);
    }

    #[test]
    fn extract_struct_pattern_variant() {
        let path = ast::Path::from_ident(ast::Ident::dummy("Point"));
        let pat = ast::Pattern::new(
            ast::PatternKind::Struct {
                path,
                fields: vec![],
                rest: None,
            },
            dummy_span(),
        );
        let covered = extract_covered_variants(&pat);
        assert_eq!(covered, vec!["Point"]);
    }

    // =====================================================================
    // Borrow checker tests
    // =====================================================================

    #[test]
    fn borrow_state_tracks_borrows() {
        let mut state = super::super::ty::BorrowState::new();
        let _lt = state.add_borrow(Arc::from("x"), None, false);
        assert!(state.has_any_borrow("x"));
        assert!(!state.has_mut_borrow("x"));
    }

    #[test]
    fn borrow_state_tracks_mut_borrows() {
        let mut state = super::super::ty::BorrowState::new();
        state.add_borrow(Arc::from("x"), None, true);
        assert!(state.has_any_borrow("x"));
        assert!(state.has_mut_borrow("x"));
    }

    #[test]
    fn borrow_state_scope_expiry() {
        let mut state = super::super::ty::BorrowState::new();
        state.push_scope();
        state.add_borrow(Arc::from("x"), None, true);
        assert!(state.has_mut_borrow("x"));
        state.pop_scope();
        assert!(!state.has_any_borrow("x"));
    }

    #[test]
    fn borrow_state_outer_persists_through_inner() {
        let mut state = super::super::ty::BorrowState::new();
        state.add_borrow(Arc::from("x"), None, true);
        state.push_scope();
        // Borrow from outer scope still visible in inner scope
        assert!(state.has_mut_borrow("x"));
        state.pop_scope();
        // Still active after inner scope ends
        assert!(state.has_mut_borrow("x"));
    }

    #[test]
    fn borrow_state_multiple_shared_ok() {
        let mut state = super::super::ty::BorrowState::new();
        state.add_borrow(Arc::from("x"), None, false);
        state.add_borrow(Arc::from("x"), None, false);
        state.add_borrow(Arc::from("x"), None, false);
        assert!(state.has_any_borrow("x"));
        assert!(!state.has_mut_borrow("x"));
        assert_eq!(state.borrows_of("x").len(), 3);
    }

    #[test]
    fn borrow_state_nested_scope_expiry() {
        let mut state = super::super::ty::BorrowState::new();
        state.push_scope(); // depth 1
        state.add_borrow(Arc::from("x"), None, true);
        state.push_scope(); // depth 2
        state.add_borrow(Arc::from("y"), None, false);
        assert!(state.has_mut_borrow("x"));
        assert!(state.has_any_borrow("y"));
        state.pop_scope(); // back to 1: y's borrow dies
        assert!(state.has_mut_borrow("x"));
        assert!(!state.has_any_borrow("y"));
        state.pop_scope(); // back to 0: x's borrow dies
        assert!(!state.has_any_borrow("x"));
    }

    #[test]
    fn lifetime_var_id_is_fresh() {
        let a = super::super::ty::LifetimeVarId::fresh();
        let b = super::super::ty::LifetimeVarId::fresh();
        assert_ne!(a, b);
    }

    #[test]
    fn lifetime_kind_display() {
        let named = super::super::ty::LifetimeKind::Named(Arc::from("a"));
        assert_eq!(format!("{}", named), "'a");
        let stat = super::super::ty::LifetimeKind::Static;
        assert_eq!(format!("{}", stat), "'static");
    }

    // =====================================================================
    // Interprocedural lifetime analysis tests
    // =====================================================================

    #[test]
    fn fn_ty_with_lifetimes_constructor() {
        use super::super::ty::{FnTy, Ty, TyKind};
        let fn_ty = Ty::function_with_lifetimes(
            vec![Ty::reference(
                Some(super::super::ty::Lifetime::new("a")),
                super::super::ty::Mutability::Immutable,
                Ty::int(super::super::ty::IntTy::I32),
            )],
            Ty::reference(
                Some(super::super::ty::Lifetime::new("a")),
                super::super::ty::Mutability::Immutable,
                Ty::int(super::super::ty::IntTy::I32),
            ),
            vec![Arc::from("a")],
        );
        if let TyKind::Fn(ref ft) = &fn_ty.kind {
            assert_eq!(ft.lifetime_params.len(), 1);
            assert_eq!(ft.lifetime_params[0].as_ref(), "a");
        } else {
            panic!("expected FnTy");
        }
    }

    #[test]
    fn fn_ty_lifetime_params_preserved_through_substitute() {
        use super::super::ty::{FnTy, Substitution, Ty, TyKind};
        let fn_ty = Ty::function_with_lifetimes(
            vec![Ty::fresh_var()],
            Ty::int(super::super::ty::IntTy::I32),
            vec![Arc::from("a"), Arc::from("b")],
        );
        // Substitute with an empty substitution — lifetime params must survive
        let subst = Substitution::new();
        let result = fn_ty.substitute(&subst);
        if let TyKind::Fn(ref ft) = &result.kind {
            assert_eq!(ft.lifetime_params.len(), 2);
            assert_eq!(ft.lifetime_params[0].as_ref(), "a");
            assert_eq!(ft.lifetime_params[1].as_ref(), "b");
        } else {
            panic!("expected FnTy");
        }
    }

    #[test]
    fn fn_ty_lifetime_params_preserved_through_freshen() {
        use super::super::ty::{FnTy, Ty, TyKind};
        let fn_ty = Ty::function_with_lifetimes(
            vec![Ty::param(Arc::from("T"), 0)],
            Ty::param(Arc::from("T"), 0),
            vec![Arc::from("a")],
        );
        let freshened = fn_ty.freshen_params();
        if let TyKind::Fn(ref ft) = &freshened.kind {
            assert_eq!(ft.lifetime_params.len(), 1);
            assert_eq!(ft.lifetime_params[0].as_ref(), "a");
        } else {
            panic!("expected FnTy");
        }
    }

    #[test]
    fn fn_ty_display_with_lifetimes() {
        use super::super::ty::Ty;
        let fn_ty = Ty::function_with_lifetimes(
            vec![Ty::int(super::super::ty::IntTy::I32)],
            Ty::bool(),
            vec![Arc::from("a"), Arc::from("b")],
        );
        let display = format!("{}", fn_ty);
        assert!(display.starts_with("for<'a, 'b>"));
        assert!(display.contains("fn("));
    }

    #[test]
    fn fn_ty_default_has_no_lifetime_params() {
        use super::super::ty::{Ty, TyKind};
        let fn_ty = Ty::function(
            vec![Ty::int(super::super::ty::IntTy::I32)],
            Ty::bool(),
        );
        if let TyKind::Fn(ref ft) = &fn_ty.kind {
            assert!(ft.lifetime_params.is_empty());
        } else {
            panic!("expected FnTy");
        }
    }
}
