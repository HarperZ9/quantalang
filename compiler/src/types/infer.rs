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

use crate::ast::{self, ExprKind, Literal as AstLiteral, BinOp, UnaryOp, AssignOp};
use crate::lexer::Span;

use super::ty::*;
use super::context::{TypeContext, ScopeKind, TypeDefKind};
use super::unify::Unifier;
use super::error::*;
use super::traits::{TraitEnv, TraitResolver, BuiltinTraits};
use super::const_generics::ConstValue;

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
            if let Ok(item_ty) = resolver.resolve_assoc_type(iter_ty, builtins.into_iterator, "Item") {
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
            if let Ok(output_ty) = resolver.resolve_assoc_type(future_ty, builtins.future, "Output") {
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
            let params: Vec<Ty> = method.sig.params.iter()
                .skip_while(|(name, _)| name.as_ref() == "self")
                .map(|(_, ty)| ty.clone())
                .collect();
            let ret = method.sig.ret.clone();
            return Some(Ty::function(params, ret));
        }

        // Look up inherent methods (impl Type { fn method(...) } without a trait).
        // Extract the type name from the DefId to query the inherent_methods registry.
        let type_name = match &ty.kind {
            TyKind::Adt(def_id, _) => {
                self.ctx.lookup_type(*def_id).map(|td| td.name.to_string())
            }
            _ => None,
        };
        if let Some(ref tname) = type_name {
            if let Some(method) = self.ctx.lookup_inherent_method(tname, method_name) {
                let params: Vec<Ty> = method.sig.params.iter()
                    .skip_while(|(name, _)| name.as_ref() == "self")
                    .map(|(_, ty)| ty.clone())
                    .collect();
                let ret = method.sig.ret.clone();
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
        self.unifier.unify(t1, t2).map_err(|e| {
            self.error(e.clone(), span);
            e
        })
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
            ExprKind::TupleField { expr: inner, index, .. } => {
                self.infer_tuple_field(inner, *index, expr.span)
            }
            ExprKind::Index { expr: inner, index } => {
                self.infer_index(inner, index, expr.span)
            }

            ExprKind::Call { func, args } => self.infer_call(func, args, expr.span),
            ExprKind::MethodCall { receiver, method, generics: _, args } => {
                self.infer_method_call(receiver, method, args, expr.span)
            }

            ExprKind::If { condition, then_branch, else_branch } => {
                self.infer_if(condition, then_branch, else_branch.as_deref(), expr.span)
            }
            ExprKind::Match { scrutinee, arms } => {
                self.infer_match(scrutinee, arms, expr.span)
            }
            ExprKind::Loop { body, .. } => self.infer_loop(body),
            ExprKind::While { condition, body, .. } => {
                self.infer_while(condition, body)
            }
            ExprKind::For { pattern, iter, body, .. } => {
                self.infer_for(pattern, iter, body, expr.span)
            }

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

            ExprKind::Closure { params, return_type, body, .. } => {
                self.infer_closure(params, return_type.as_deref(), body, expr.span)
            }

            ExprKind::Cast { expr: inner, ty } => self.infer_cast(inner, ty, expr.span),
            ExprKind::Ref { mutability, expr: inner } => {
                self.infer_ref(*mutability, inner)
            }
            ExprKind::Deref(inner) => self.infer_deref(inner, expr.span),

            ExprKind::Range { start, end, inclusive } => {
                self.infer_range(start.as_deref(), end.as_deref(), *inclusive, expr.span)
            }

            ExprKind::Try(inner) => self.infer_try(inner, expr.span),
            ExprKind::Await(inner) => self.infer_await(inner, expr.span),

            ExprKind::Struct { path, fields, rest } => {
                self.infer_struct(path, fields, rest.as_deref(), expr.span)
            }

            ExprKind::Paren(inner) => self.infer_expr(inner),
            ExprKind::Error => Ty::error(),

            // While let loops
            ExprKind::WhileLet { pattern, expr, body, .. } => {
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
            ExprKind::Handle { effect, body, handlers } => {
                self.infer_handle(effect, body, handlers, expr.span)
            }

            ExprKind::Resume(value) => {
                // Resume transfers control back to the handler's continuation.
                // The value passed to resume must match the operation's return type.
                if let Some(val) = value {
                    let _ = self.infer_expr(val);
                }
                // Resume itself diverges from the handler clause's perspective.
                Ty::never()
            }

            ExprKind::Perform { effect, operation, args } => {
                self.infer_perform(effect, operation, args, expr.span)
            }
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
                // String literals have type &'static str
                Ty::reference(
                    Some(Lifetime::static_lifetime()),
                    Mutability::Immutable,
                    Ty::str(),
                )
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
            let is_builtin = matches!(name,
                // Math builtins
                "sqrt" | "sin" | "cos" | "tan" | "pow" | "abs" |
                "log" | "log2" | "log10" | "exp" | "atan2" |
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
                // Vec builtins
                "vec_new" | "vec_push" | "vec_get" | "vec_len" | "vec_pop" |
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
                self.error(TypeError::UndefinedVariable {
                    name: ident.name.to_string(),
                }, span);
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

        // Check for associated function: Type::func (2-segment path)
        let segments: Vec<&str> = path.segments.iter()
            .map(|s| s.ident.name.as_ref())
            .collect();
        if segments.len() == 2 {
            let type_name = segments[0];
            let func_name = segments[1];
            // Look up as an inherent method/associated function
            if let Some(method) = self.ctx.lookup_inherent_method(type_name, func_name) {
                let param_tys: Vec<Ty> = method.sig.params.iter().map(|(_, ty)| ty.clone()).collect();
                return Ty::function(param_tys, method.sig.ret.clone());
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
                // Dereference: *T -> value of T
                Ty::fresh_var()
            }
            UnaryOp::Ref => {
                // Reference: &T
                Ty::fresh_var()
            }
            UnaryOp::RefMut => {
                // Mutable reference: &mut T
                Ty::fresh_var()
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

    fn infer_assign(&mut self, _op: AssignOp, target: &ast::Expr, value: &ast::Expr, span: Span) -> Ty {
        let target_ty = self.infer_expr(target);
        let value_ty = self.infer_expr(value);
        let _ = self.unify(&target_ty, &value_ty, span);
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
                            self.error(TypeError::UndefinedField {
                                ty: expr_ty,
                                field: field_name.to_string(),
                            }, span);
                            return Ty::error();
                        }
                        TypeDefKind::Enum(_) => {
                            self.error(TypeError::UndefinedField {
                                ty: expr_ty,
                                field: field.name.to_string(),
                            }, span);
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
            TyKind::Error => Ty::error(),
            _ => {
                self.error(TypeError::UndefinedField {
                    ty: expr_ty,
                    field: field.name.to_string(),
                }, span);
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
                    self.error(TypeError::UndefinedField {
                        ty: expr_ty,
                        field: index.to_string(),
                    }, span);
                    Ty::error()
                }
            }
            TyKind::Error => Ty::error(),
            _ => {
                self.error(TypeError::UndefinedField {
                    ty: expr_ty,
                    field: index.to_string(),
                }, span);
                Ty::error()
            }
        }
    }

    fn infer_index(&mut self, expr: &ast::Expr, index: &ast::Expr, span: Span) -> Ty {
        let expr_ty = self.infer_expr(expr);
        let index_ty = self.infer_expr(index);
        let expr_ty = self.apply(&expr_ty);

        // Check that index type is usize (or can be unified with usize)
        let _ = self.unify(&index_ty, &Ty::int(IntTy::Usize), span);

        match &expr_ty.kind {
            TyKind::Array(elem, _) | TyKind::Slice(elem) => (**elem).clone(),
            TyKind::Ref(_, _, inner) => {
                // Auto-deref for indexing on references to arrays/slices
                match &inner.kind {
                    TyKind::Array(elem, _) | TyKind::Slice(elem) => (**elem).clone(),
                    _ => {
                        self.error(TypeError::NotIndexable { ty: expr_ty }, span);
                        Ty::error()
                    }
                }
            }
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
                    self.error(TypeError::ArityMismatch {
                        expected: fn_ty.params.len(),
                        found: args.len(),
                    }, span);
                }

                for (param, arg) in fn_ty.params.iter().zip(args.iter()) {
                    let arg_ty = self.infer_expr(arg);
                    let _ = self.unify(param, &arg_ty, span);
                }

                // Propagate callee's effects to caller's effect context
                if !fn_ty.effects.is_empty() {
                    self.current_effects = self.current_effects.merge(&fn_ty.effects);
                }

                (*fn_ty.ret).clone()
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
        let effect_name = effect.last_ident()
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
            let defined_ops: Vec<_> = effect_def.operations.iter()
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
            let op_match = def.operations.iter().find(|op| op.name.as_ref() == operation_name);
            (
                op_match.map(|op| (op.params.clone(), op.return_ty.clone())),
                def.operations.len(),
            )
        });

        if let Some((op_data, _op_count)) = effect_lookup {
            if let Some((param_tys, return_ty)) = op_data {
                // Check argument count
                if param_tys.len() != args.len() {
                    self.error(TypeError::ArityMismatch {
                        expected: param_tys.len(),
                        found: args.len(),
                    }, span);
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
        if let TyKind::TraitObject(ref bounds) = receiver_ty.kind {
            // Trait objects have all methods defined by the trait — silently accept
            // The codegen handles the vtable dispatch
            return Ty::error(); // Return error type as placeholder (codegen resolves return type)
        }

        // Try to look up method in impl blocks using trait resolver
        if let Some(method_ty) = self.lookup_method(&receiver_ty, method.name.as_ref()) {
            // Unify with expected function type
            match &method_ty.kind {
                TyKind::Fn(fn_ty) => {
                    // Check arity (method params don't include self)
                    if fn_ty.params.len() != arg_tys.len() {
                        self.error(TypeError::ArityMismatch {
                            expected: fn_ty.params.len(),
                            found: arg_tys.len(),
                        }, span);
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

        // Check for common built-in methods
        let method_name = method.name.as_ref();
        match method_name {
            "len" => {
                // .len() on arrays, slices, strings returns usize
                match &receiver_ty.kind {
                    TyKind::Array(_, _) | TyKind::Slice(_) | TyKind::Str => {
                        return Ty::int(IntTy::Usize);
                    }
                    TyKind::Ref(_, _, inner) => {
                        match &inner.kind {
                            TyKind::Array(_, _) | TyKind::Slice(_) | TyKind::Str => {
                                return Ty::int(IntTy::Usize);
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            "is_empty" => {
                // .is_empty() returns bool
                return Ty::bool();
            }
            "clone" => {
                // .clone() returns Self
                return receiver_ty.clone();
            }
            "to_string" | "to_owned" => {
                // These return String (represented as owned str for now)
                return Ty::str();
            }
            "iter" | "iter_mut" | "into_iter" => {
                // Returns an iterator - we'll use a fresh variable
                return Ty::fresh_var();
            }
            "unwrap" | "expect" => {
                // For Option<T> and Result<T, E>, returns T
                if let TyKind::Adt(def_id, substs) = &receiver_ty.kind {
                    if (Some(*def_id) == self.well_known_types.option ||
                        Some(*def_id) == self.well_known_types.result) && !substs.is_empty() {
                        return substs[0].clone();
                    }
                }
            }
            "map" | "and_then" | "or_else" => {
                // These take a function and return the same wrapper type
                if !arg_tys.is_empty() {
                    // Return type depends on the closure's return type
                    return Ty::fresh_var();
                }
            }
            _ => {}
        }

        // Method not found - report error but return fresh variable to continue inference
        self.error(TypeError::UndefinedMethod {
            ty: receiver_ty,
            method: method.name.to_string(),
        }, span);
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

        result_ty
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

        let mut result_ty = Ty::unit();

        for stmt in &block.stmts {
            result_ty = self.infer_stmt(stmt);
        }

        self.ctx.pop_scope();
        result_ty
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
            ast::StmtKind::Item(_) => {
                // Items are handled separately
                Ty::unit()
            }
            ast::StmtKind::Empty => Ty::unit(),
            ast::StmtKind::Macro { path, tokens: _, is_semi } => {
                // Macro invocations as statements
                // For well-known macros, we can infer their result type
                // For unknown macros, we use unit if semicolon-terminated,
                // otherwise a fresh type variable

                let macro_name = path.segments.last()
                    .map(|s| s.ident.as_str())
                    .unwrap_or("");

                match macro_name {
                    // Diagnostic macros always return unit
                    "println" | "print" | "eprintln" | "eprint" |
                    "dbg" | "debug" | "log" | "trace" | "warn" | "error" |
                    "assert" | "assert_eq" | "assert_ne" | "debug_assert" |
                    "debug_assert_eq" | "debug_assert_ne" => Ty::unit(),

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
            self.infer_expr(expr)
        } else {
            Ty::unit()
        };

        if let Some(expected) = self.return_ty.clone() {
            let _ = self.unify(&value_ty, &expected, span);
        }

        self.has_return = true;
        Ty::never()
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

        let param_tys: Vec<Ty> = params.iter().map(|p| {
            let ty = if let Some(ty_ast) = &p.ty {
                self.lower_type(ty_ast)
            } else {
                Ty::fresh_var()
            };
            self.bind_pattern(&p.pattern, &ty);
            ty
        }).collect();

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
        let mut_ = match mutability {
            ast::Mutability::Mutable => Mutability::Mutable,
            ast::Mutability::Immutable => Mutability::Immutable,
        };
        Ty::reference(None, mut_, inner_ty)
    }

    fn infer_deref(&mut self, expr: &ast::Expr, span: Span) -> Ty {
        let expr_ty = self.infer_expr(expr);
        let expr_ty = self.apply(&expr_ty);

        match &expr_ty.kind {
            TyKind::Ref(_, _, inner) | TyKind::Ptr(_, inner) => (**inner).clone(),
            TyKind::Error => Ty::error(),
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
        let struct_info = self.ctx.lookup_type_by_name(struct_name).and_then(|type_def| {
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
                        self.error(TypeError::UndefinedVariable {
                            name: field_name.to_string(),
                        }, span);
                        Ty::error()
                    }
                };

                // Find expected field type and unify
                if let Some((_, expected_ty)) = struct_fields.iter().find(|(n, _)| n.as_ref() == field_name) {
                    let _ = self.unify(expected_ty, &value_ty, span);
                } else {
                    self.error(TypeError::UndefinedField {
                        ty: Ty::adt(def_id, substs.clone()),
                        field: field_name.to_string(),
                    }, span);
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
                if let TyKind::Tuple(elem_tys) = &ty.kind {
                    for (pat, elem_ty) in patterns.iter().zip(elem_tys.iter()) {
                        self.bind_pattern(pat, elem_ty);
                    }
                }
            }
            ast::PatternKind::Struct { path, fields, .. } => {
                // Look up struct definition to get field types
                let struct_name = path.last_ident().map(|i| &*i.name);

                // Extract field types before recursive calls to avoid borrow conflicts
                let field_types: Vec<_> = fields.iter().map(|field| {
                    let field_name = field.name.as_ref();

                    // Try to get field type from ADT type
                    if let TyKind::Adt(def_id, _substs) = &ty.kind {
                        if let Some(type_def) = self.ctx.lookup_type(*def_id) {
                            if let TypeDefKind::Struct(struct_def) = &type_def.kind {
                                if let Some((_, t)) = struct_def.fields.iter()
                                    .find(|(n, _)| n.as_ref() == field_name) {
                                    return t.clone();
                                }
                            }
                        }
                    }
                    // Try by name
                    if let Some(name) = struct_name {
                        if let Some(type_def) = self.ctx.lookup_type_by_name(name) {
                            if let TypeDefKind::Struct(struct_def) = &type_def.kind {
                                if let Some((_, t)) = struct_def.fields.iter()
                                    .find(|(n, _)| n.as_ref() == field_name) {
                                    return t.clone();
                                }
                            }
                        }
                    }
                    Ty::fresh_var()
                }).collect();

                // Now bind patterns with the extracted types
                for (field, field_ty) in fields.iter().zip(field_types.iter()) {
                    self.bind_pattern(&field.pattern, field_ty);
                }
            }
            ast::PatternKind::TupleStruct { path: _, patterns, .. } => {
                // Extract field types before recursive calls to avoid borrow conflicts
                let field_types: Vec<_> = patterns.iter().enumerate().map(|(i, _)| {
                    if let TyKind::Adt(def_id, _) = &ty.kind {
                        if let Some(type_def) = self.ctx.lookup_type(*def_id) {
                            if let TypeDefKind::Struct(struct_def) = &type_def.kind {
                                if struct_def.is_tuple {
                                    return struct_def.fields.get(i)
                                        .map(|(_, t)| t.clone())
                                        .unwrap_or_else(Ty::fresh_var);
                                }
                            }
                        }
                    }
                    Ty::fresh_var()
                }).collect();

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
            ast::TypeKind::Slice(elem) => {
                Ty::slice(self.lower_type(elem))
            }
            ast::TypeKind::Ref { lifetime, mutability, ty: inner } => {
                let lt = lifetime.as_ref().map(|l| Lifetime::new(l.name.name.as_ref()));
                let mut_ = match mutability {
                    ast::Mutability::Mutable => Mutability::Mutable,
                    ast::Mutability::Immutable => Mutability::Immutable,
                };
                Ty::reference(lt, mut_, self.lower_type(inner))
            }
            ast::TypeKind::Ptr { mutability, ty: inner } => {
                let mut_ = match mutability {
                    ast::Mutability::Mutable => Mutability::Mutable,
                    ast::Mutability::Immutable => Mutability::Immutable,
                };
                Ty::ptr(mut_, self.lower_type(inner))
            }
            ast::TypeKind::BareFn { params, return_ty, is_unsafe, .. } => {
                let param_tys: Vec<_> = params.iter().map(|p| self.lower_type(&p.ty)).collect();
                let ret = return_ty.as_ref().map(|t| self.lower_type(t)).unwrap_or(Ty::unit());
                Ty::new(TyKind::Fn(FnTy {
                    params: param_tys,
                    ret: Box::new(ret),
                    is_unsafe: *is_unsafe,
                    abi: None,
                    effects: super::effects::EffectRow::empty(),
                }))
            }
            ast::TypeKind::Path(path) => {
                self.lower_type_path(path)
            }
            ast::TypeKind::TraitObject { bounds, .. } => {
                let trait_names: Vec<Arc<str>> = bounds.iter()
                    .map(|b| b.path.last_ident()
                        .map(|i| i.name.clone())
                        .unwrap_or(Arc::from("Unknown")))
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
                    generics.iter().filter_map(|arg| {
                        if let ast::GenericArg::Type(ty) = arg {
                            Some(self.lower_type(&ty))
                        } else {
                            None
                        }
                    }).collect()
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
