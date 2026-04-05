// ===============================================================================
// QUANTALANG TYPE SYSTEM - TYPE CHECKER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Type checker for items (functions, structs, enums, traits, impls).
//!
//! This module handles type checking at the item level, while `infer.rs`
//! handles expression-level type inference.

use std::sync::Arc;

use crate::ast::{self, ImplItemKind, ItemKind, StructFields, TraitItemKind};
use crate::lexer::Span;

use super::context::*;
use super::error::*;
use super::infer::TypeInfer;
use super::ty::*;

/// The type checker for items and declarations.
pub struct TypeChecker<'ctx> {
    /// The type context.
    ctx: &'ctx mut TypeContext,
    /// Collected errors.
    errors: Vec<TypeErrorWithSpan>,
    /// Effect context for tracking registered effects.
    effect_ctx: super::effects::EffectContext,
    /// Source directory for resolving external module files.
    source_dir: Option<std::path::PathBuf>,
}

impl<'ctx> TypeChecker<'ctx> {
    /// Create a new type checker.
    pub fn new(ctx: &'ctx mut TypeContext) -> Self {
        Self {
            ctx,
            errors: Vec::new(),
            effect_ctx: super::effects::EffectContext::new(),
            source_dir: None,
        }
    }

    /// Set the source directory for resolving `mod foo;` declarations.
    pub fn set_source_dir(&mut self, dir: std::path::PathBuf) {
        self.source_dir = Some(dir);
    }

    /// Get a reference to the effect context.
    pub fn effect_ctx(&self) -> &super::effects::EffectContext {
        &self.effect_ctx
    }

    /// Get collected errors.
    pub fn errors(&self) -> &[TypeErrorWithSpan] {
        &self.errors
    }

    /// Take collected errors.
    pub fn take_errors(&mut self) -> Vec<TypeErrorWithSpan> {
        std::mem::take(&mut self.errors)
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Report an error.
    fn error(&mut self, error: TypeError, span: Span) {
        self.errors.push(TypeErrorWithSpan::new(error, span));
    }

    // =========================================================================
    // MODULE CHECKING
    // =========================================================================

    /// Check a module.
    pub fn check_module(&mut self, module: &ast::Module) {
        // Register built-in vector/matrix struct types so that type annotations
        // like `vec3` resolve to known struct types with accessible fields.
        self.register_builtin_vec_types();

        // Register prelude constructors (Ok, Err, Some, None) as variables
        // with fresh type variables so they pass type checking.
        self.ctx.define_var(Arc::from("Ok"), Ty::fresh_var());
        self.ctx.define_var(Arc::from("Err"), Ty::fresh_var());
        self.ctx.define_var(Arc::from("Some"), Ty::fresh_var());
        self.ctx.define_var(Arc::from("None"), Ty::fresh_var());

        // Register shader built-in functions as variables
        self.ctx.define_var(Arc::from("saturate"), Ty::fresh_var());
        self.ctx.define_var(Arc::from("discard"), Ty::fresh_var());

        // Register runtime built-in functions
        self.ctx.define_var(
            Arc::from("assert"),
            Ty::function(vec![Ty::bool()], Ty::unit()),
        );
        self.ctx.define_var(Arc::from("assert_eq"), Ty::fresh_var());
        self.ctx.define_var(Arc::from("println"), Ty::fresh_var());

        // First pass: collect all type definitions
        for item in &module.items {
            self.collect_item(item);
        }

        // Register built-in trait stubs AFTER user types so DefIds are consistent
        self.ctx.register_builtin_traits();

        // Second pass: type check all items
        for item in &module.items {
            self.check_item(item);
        }
    }

    /// Register built-in vector and matrix struct types (vec2, vec3, vec4, mat4)
    /// so that type annotations resolve correctly and field access works.
    fn register_builtin_vec_types(&mut self) {
        let f64_ty = Ty::float(FloatTy::F64);

        // vec2 { x: f64, y: f64 }
        let def_id = self.ctx.fresh_def_id();
        self.ctx.register_type(TypeDef {
            def_id,
            name: Arc::from("vec2"),
            generics: Vec::new(),
            kind: TypeDefKind::Struct(StructDef {
                fields: vec![
                    (Arc::from("x"), f64_ty.clone()),
                    (Arc::from("y"), f64_ty.clone()),
                ],
                is_tuple: false,
            }),
        });

        // vec3 { x: f64, y: f64, z: f64 }
        let def_id = self.ctx.fresh_def_id();
        self.ctx.register_type(TypeDef {
            def_id,
            name: Arc::from("vec3"),
            generics: Vec::new(),
            kind: TypeDefKind::Struct(StructDef {
                fields: vec![
                    (Arc::from("x"), f64_ty.clone()),
                    (Arc::from("y"), f64_ty.clone()),
                    (Arc::from("z"), f64_ty.clone()),
                ],
                is_tuple: false,
            }),
        });

        // vec4 { x: f64, y: f64, z: f64, w: f64 }
        let def_id = self.ctx.fresh_def_id();
        self.ctx.register_type(TypeDef {
            def_id,
            name: Arc::from("vec4"),
            generics: Vec::new(),
            kind: TypeDefKind::Struct(StructDef {
                fields: vec![
                    (Arc::from("x"), f64_ty.clone()),
                    (Arc::from("y"), f64_ty.clone()),
                    (Arc::from("z"), f64_ty.clone()),
                    (Arc::from("w"), f64_ty.clone()),
                ],
                is_tuple: false,
            }),
        });

        // mat4 — registered as opaque (no user-accessible fields)
        let def_id = self.ctx.fresh_def_id();
        self.ctx.register_type(TypeDef {
            def_id,
            name: Arc::from("mat4"),
            generics: Vec::new(),
            kind: TypeDefKind::Struct(StructDef {
                fields: Vec::new(),
                is_tuple: false,
            }),
        });
    }

    // =========================================================================
    // COLLECTION PASS
    // =========================================================================

    /// Collect type definitions from an item (first pass).
    fn collect_item(&mut self, item: &ast::Item) {
        match &item.kind {
            ItemKind::Struct(s) => self.collect_struct(s, item.span),
            ItemKind::Enum(e) => self.collect_enum(e, item.span),
            ItemKind::TypeAlias(ta) => self.collect_type_alias(ta, item.span),
            ItemKind::Trait(t) => self.collect_trait(t, item.span),
            ItemKind::Function(f) => self.collect_function(f, item.span),
            ItemKind::Effect(e) => self.collect_effect(e, item.span),
            ItemKind::ExternBlock(eb) => self.collect_extern_block(eb, item.span),
            ItemKind::Impl(impl_) => self.collect_impl(impl_, item.span),
            ItemKind::Const(c) => {
                // Pre-register constants so forward references work
                let ty = self.lower_type(&c.ty);
                self.ctx.define_var(c.name.name.clone(), ty);
            }
            ItemKind::Static(s) => {
                // Pre-register statics so forward references work
                let ty = self.lower_type(&s.ty);
                self.ctx.define_var(s.name.name.clone(), ty);
            }
            ItemKind::Use(use_def) => self.resolve_use(&use_def.tree),
            ItemKind::Mod(m) => self.collect_mod(m),
            _ => {}
        }
    }

    /// Collect module items during the first pass.
    /// For inline modules, collect their items recursively.
    /// For external modules (`mod foo;`), load and parse the file.
    fn collect_mod(&mut self, m: &ast::ModDef) {
        if let Some(content) = &m.content {
            // Inline module: collect items directly
            for item in &content.items {
                self.collect_item(item);
            }
        } else if let Some(ref dir) = self.source_dir.clone() {
            // External module: load from disk
            let mod_name = m.name.name.as_ref();
            let mod_path = dir.join(format!("{}.quanta", mod_name));
            if mod_path.exists() {
                if let Ok(source_text) = std::fs::read_to_string(&mod_path) {
                    let source = crate::lexer::SourceFile::new(
                        mod_path.to_string_lossy().as_ref(),
                        source_text,
                    );
                    let mut lexer = crate::lexer::Lexer::new(&source);
                    if let Ok(tokens) = lexer.tokenize() {
                        let mut parser = crate::parser::Parser::new(&source, tokens);
                        if let Ok(module_ast) = parser.parse() {
                            for item in &module_ast.items {
                                self.collect_item(item);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Collect inherent impl methods during the first pass so they're
    /// available for method resolution when function bodies are checked.
    fn collect_impl(&mut self, impl_: &ast::ImplDef, _span: Span) {
        // Only collect inherent impls (no trait). Trait impls are handled in check_impl.
        if impl_.trait_ref.is_some() {
            return;
        }

        // Push a scope for generic type parameters so they don't leak
        // into the surrounding module's type namespace.
        self.ctx.push_scope(ScopeKind::Block);
        for (idx, param) in impl_.generics.params.iter().enumerate() {
            if let ast::GenericParamKind::Type { .. } = &param.kind {
                let ty = Ty::param(param.ident.name.clone(), idx as u32);
                self.ctx.define_type_param(param.ident.name.clone(), ty);
            }
        }

        let _self_ty = self.lower_type(&impl_.self_ty);
        let type_name = Self::extract_type_name_from_ast(&impl_.self_ty);
        let type_def_id = type_name.as_ref().and_then(|n| {
            self.ctx.lookup_type_by_name(n).map(|td| td.def_id)
        });

        for item in &impl_.items {
            match &item.kind {
                ImplItemKind::Function(f) => {
                    if let Some(def_id) = type_def_id {
                        let sig = self.build_fn_sig_from_ast(f);
                        self.ctx.register_inherent_method(
                            def_id,
                            f.name.name.clone(),
                            sig,
                        );
                    }
                }
                ImplItemKind::Const { name, ty, .. } => {
                    // Register associated constants at module scope so they're
                    // accessible from other impl blocks (e.g., BRADFORD in
                    // chromatic_adaptation.quanta).
                    let const_ty = self.lower_type(ty);
                    self.ctx.define_var(name.name.clone(), const_ty);
                }
                _ => {}
            }
        }

        self.ctx.pop_scope();
    }

    fn collect_struct(&mut self, s: &ast::StructDef, _span: Span) {
        let def_id = self.ctx.fresh_def_id();

        let generics = self.collect_generics(&s.generics);
        let num_generics = generics.len();

        let fields = match &s.fields {
            StructFields::Named(fields) => fields
                .iter()
                .map(|f| {
                    let ty = self.lower_type(&f.ty);
                    (f.name.name.clone(), ty)
                })
                .collect(),
            StructFields::Tuple(fields) => fields
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let ty = self.lower_type(&f.ty);
                    (Arc::from(i.to_string()), ty)
                })
                .collect(),
            StructFields::Unit => Vec::new(),
        };

        let type_def = TypeDef {
            def_id,
            name: s.name.name.clone(),
            generics,
            kind: TypeDefKind::Struct(StructDef {
                fields,
                is_tuple: matches!(s.fields, StructFields::Tuple(_)),
            }),
        };

        self.ctx.register_type(type_def);

        // For tuple structs, register a constructor function so that
        // `TupleStruct(val)` works as a call expression.
        if matches!(&s.fields, StructFields::Tuple(_)) {
            if let StructFields::Tuple(fields) = &s.fields {
                let param_tys: Vec<Ty> = fields.iter().map(|f| self.lower_type(&f.ty)).collect();
                let substs: Vec<Ty> = (0..num_generics).map(|_| Ty::fresh_var()).collect();
                let ret_ty = Ty::adt(def_id, substs);
                let fn_ty = Ty::function(param_tys, ret_ty);
                self.ctx.define_var(s.name.name.clone(), fn_ty);
            }
        }

        // For unit structs (e.g., `struct Stdin;`), register the name as a
        // variable so it can be used as a value expression: `Stdin` or `let x = Stdin;`
        if matches!(&s.fields, StructFields::Unit) {
            let substs: Vec<Ty> = (0..num_generics).map(|_| Ty::fresh_var()).collect();
            let val_ty = Ty::adt(def_id, substs);
            self.ctx.define_var(s.name.name.clone(), val_ty);
        }
    }

    fn collect_enum(&mut self, e: &ast::EnumDef, _span: Span) {
        let def_id = self.ctx.fresh_def_id();

        let generics = self.collect_generics(&e.generics);

        let variants = e
            .variants
            .iter()
            .map(|v| {
                let fields = match &v.fields {
                    StructFields::Named(fields) => fields
                        .iter()
                        .map(|f| (Some(f.name.name.clone()), self.lower_type(&f.ty)))
                        .collect(),
                    StructFields::Tuple(types) => types
                        .iter()
                        .map(|t| (None, self.lower_type(&t.ty)))
                        .collect(),
                    StructFields::Unit => Vec::new(),
                };

                EnumVariant {
                    name: v.name.name.clone(),
                    fields,
                    discriminant: v.discriminant.as_ref().and_then(|e| {
                        // Try to evaluate const expression
                        self.eval_const_int(e)
                    }),
                }
            })
            .collect();

        let type_def = TypeDef {
            def_id,
            name: e.name.name.clone(),
            generics,
            kind: TypeDefKind::Enum(EnumDef { variants }),
        };

        self.ctx.register_type(type_def);
    }

    fn collect_type_alias(&mut self, ta: &ast::TypeAliasDef, _span: Span) {
        let def_id = self.ctx.fresh_def_id();
        let generics = self.collect_generics(&ta.generics);

        if let Some(ty_ast) = &ta.ty {
            let ty = self.lower_type(ty_ast);
            let alias = TypeAlias {
                def_id,
                name: ta.name.name.clone(),
                generics,
                ty,
            };
            self.ctx.register_alias(alias);
        }
    }

    fn collect_trait(&mut self, t: &ast::TraitDef, _span: Span) {
        let def_id = self.ctx.fresh_def_id();
        let generics = self.collect_generics(&t.generics);

        let supertraits = t
            .supertraits
            .iter()
            .filter_map(|bound| self.lower_type_bound(bound))
            .collect();

        let assoc_types = t
            .items
            .iter()
            .filter_map(|item| {
                if let TraitItemKind::Type {
                    name,
                    bounds,
                    default,
                    ..
                } = &item.kind
                {
                    Some(AssocType {
                        name: name.name.clone(),
                        bounds: bounds
                            .iter()
                            .filter_map(|b| self.lower_type_bound(b))
                            .collect(),
                        default: default.as_ref().map(|t| self.lower_type(t)),
                    })
                } else {
                    None
                }
            })
            .collect();

        let methods = t
            .items
            .iter()
            .filter_map(|item| {
                if let TraitItemKind::Function(f) = &item.kind {
                    Some(TraitMethod {
                        name: f.name.name.clone(),
                        sig: self.lower_fn_sig(&f.generics, &f.sig),
                        has_default: f.body.is_some(),
                    })
                } else {
                    None
                }
            })
            .collect();

        let trait_def = TraitDef {
            def_id,
            name: t.name.name.clone(),
            generics,
            supertraits,
            assoc_types,
            methods,
        };

        self.ctx.register_trait(trait_def);
    }

    fn collect_function(&mut self, f: &ast::FnDef, _span: Span) {
        let def_id = self.ctx.fresh_def_id();
        let sig = self.lower_fn_sig(&f.generics, &f.sig);
        self.ctx.register_function(def_id, sig.clone());

        // Add function to current scope — carry lifetime params for interprocedural analysis
        let param_tys: Vec<_> = sig.params.iter().map(|(_, ty)| ty.clone()).collect();
        let fn_ty = Ty::function_with_lifetimes(param_tys, sig.ret, sig.lifetime_params.clone());
        self.ctx.define_var(f.name.name.clone(), fn_ty);
    }

    /// Collect extern block declarations.  Each foreign function is registered
    /// in the type context so that calls to it can be type-checked.
    fn collect_extern_block(&mut self, eb: &ast::ExternBlockDef, _span: Span) {
        for foreign_item in &eb.items {
            if let ast::ForeignItemKind::Fn(f) = &foreign_item.kind {
                self.collect_function(f, foreign_item.span);
            }
        }
    }

    /// Collect a user-defined effect declaration and register it in the effect context.
    fn collect_effect(&mut self, effect_def: &ast::EffectDef, _span: Span) {
        let def_id = self.ctx.fresh_def_id();

        // Build the types::effects::EffectDef from the AST node
        let mut ty_effect = super::effects::EffectDef::new(def_id, effect_def.name.name.as_ref());

        // Add generic type parameters
        for param in &effect_def.generics.params {
            if let ast::GenericParamKind::Type { .. } = &param.kind {
                ty_effect = ty_effect.with_type_param(param.ident.name.as_ref());
            }
        }

        // Convert each AST operation into a types::effects::EffectOperation
        for op in &effect_def.operations {
            let param_tys: Vec<Ty> = op.params.iter().map(|p| self.lower_type(&p.ty)).collect();

            let return_ty = op
                .return_ty
                .as_ref()
                .map(|t| self.lower_type(t))
                .unwrap_or(Ty::unit());

            let effect_op =
                super::effects::EffectOperation::new(op.name.name.as_ref(), param_tys, return_ty);

            ty_effect = ty_effect.with_operation(effect_op);
        }

        self.effect_ctx.register_effect(ty_effect);
    }

    // =========================================================================
    // TYPE CHECKING PASS
    // =========================================================================

    /// Check an item (second pass).
    fn check_item(&mut self, item: &ast::Item) {
        match &item.kind {
            ItemKind::Function(f) => self.check_function(f, item.span),
            ItemKind::Impl(impl_) => self.check_impl(impl_, item.span),
            ItemKind::Const(c) => self.check_const(c, item.span),
            ItemKind::Static(s) => self.check_static(s, item.span),
            ItemKind::Mod(m) => self.check_mod(m),
            _ => {}
        }
    }

    fn check_function(&mut self, f: &ast::FnDef, span: Span) {
        if let Some(body) = &f.body {
            self.ctx.push_scope(ScopeKind::Function);

            // Add generic parameters and register their trait bounds
            self.ctx.clear_param_bounds();
            for (idx, param) in f.generics.params.iter().enumerate() {
                if let ast::GenericParamKind::Type { ref bounds, .. } = &param.kind {
                    let ty = Ty::param(param.ident.name.clone(), idx as u32);
                    self.ctx.define_type_param(param.ident.name.clone(), ty);

                    // Collect trait bound names for this type parameter
                    if !bounds.is_empty() {
                        let trait_names: Vec<Arc<str>> = bounds
                            .iter()
                            .filter(|b| !b.is_maybe)
                            .map(|b| {
                                // Extract the last segment of the trait path as the name
                                Arc::from(
                                    b.path
                                        .segments
                                        .last()
                                        .map(|s| s.ident.name.as_ref())
                                        .unwrap_or(""),
                                )
                            })
                            .collect();
                        self.ctx
                            .register_param_bounds(param.ident.name.clone(), trait_names);
                    }
                }
            }

            // Also register bounds from where clauses
            for pred in f.generics.where_clause.iter().flat_map(|wc| &wc.predicates) {
                // Extract the type parameter name from the type
                if let ast::TypeKind::Path(ref path) = pred.ty.kind {
                    if let Some(seg) = path.segments.last() {
                        let param_name = seg.ident.name.clone();
                        let trait_names: Vec<Arc<str>> = pred
                            .bounds
                            .iter()
                            .filter(|b| !b.is_maybe)
                            .map(|b| {
                                Arc::from(
                                    b.path
                                        .segments
                                        .last()
                                        .map(|s| s.ident.name.as_ref())
                                        .unwrap_or(""),
                                )
                            })
                            .collect();
                        if !trait_names.is_empty() {
                            self.ctx.register_param_bounds(param_name, trait_names);
                        }
                    }
                }
            }

            // Add function parameters
            for param in &f.sig.params {
                let ty = self.lower_type(&param.ty);
                self.bind_pattern(&param.pattern, &ty);
            }

            // Set expected return type FIRST, before creating TypeInfer
            let expected_ret = f
                .sig
                .return_ty
                .as_ref()
                .map(|t| self.lower_type(t))
                .unwrap_or(Ty::unit());

            // Build expected effect row from function signature annotations
            let expected_effects = self.lower_effect_annotations(&f.sig.effects);

            // Validate that each annotated effect is a known/registered effect
            for eff in &expected_effects.effects {
                if self.effect_ctx.get_effect(eff.name.as_ref()).is_none() {
                    let err = TypeError::UnknownEffect {
                        name: eff.name.to_string(),
                    };
                    let mut err_with_span = TypeErrorWithSpan::new(err, span);
                    err_with_span.help = Some(format!(
                        "define the effect:\n  effect {} {{\n      fn operation_name(params) -> ReturnType,\n  }}",
                        eff.name
                    ));
                    self.errors.push(err_with_span);
                }
            }

            // Collect user-defined effects to pass to the inference context
            let user_effects: Vec<_> = self.effect_ctx.all_effects().into_iter().cloned().collect();

            // Check function body - use block to limit TypeInfer borrow scope
            let (body_ty, body_effects, infer_errors, has_return) = {
                let mut infer = TypeInfer::new(self.ctx);
                // Pass the expected return type so that `return` statements
                // inside nested control flow (while/if/match) are properly
                // type-checked against the function signature.
                infer.set_return_ty(expected_ret.clone());
                // Register all user-defined effects so infer_perform can resolve them
                for eff in user_effects {
                    infer.register_effect(eff);
                }
                let body_ty = infer.infer_block(body);
                let body_effects = infer.current_effect_row().clone();
                let has_return = infer.has_explicit_return();
                (body_ty, body_effects, infer.take_errors(), has_return)
            };

            // Unify body type with return type.
            // If the function contains explicit `return` statements, the body
            // type might be `()` (e.g., from a while loop that returns via
            // `return` inside an `if`). In this case, the return type was
            // already validated by infer_return(), so skip the body check.
            if !has_return {
                if let Err(_) = super::unify::unify(&body_ty, &expected_ret) {
                    // When ADT types mismatch by DefId, check if they match by
                    // name.  This handles cases where inline module re-exports
                    // or registration order give the same struct different
                    // DefIds.
                    let name_match = if let (TyKind::Adt(d1, _), TyKind::Adt(d2, _)) =
                        (&body_ty.kind, &expected_ret.kind)
                    {
                        if d1 != d2 {
                            let n1 = self.ctx.lookup_type(*d1).map(|t| t.name.clone());
                            let n2 = self.ctx.lookup_type(*d2).map(|t| t.name.clone());
                            n1.is_some() && n1 == n2
                        } else {
                            true
                        }
                    } else {
                        false
                    };
                    if !name_match {
                        self.error(
                            TypeError::ReturnTypeMismatch {
                                expected: expected_ret,
                                found: body_ty,
                            },
                            span,
                        );
                    }
                }
            }

            // Check effects: if the function is declared pure (no effect annotations)
            // but the body performs effects, report an error.
            let func_name = f.name.name.to_string();
            if expected_effects.is_empty() && !body_effects.is_empty() {
                for body_eff in &body_effects.effects {
                    let err = TypeError::UnhandledEffect {
                        func_name: func_name.clone(),
                        effect_name: body_eff.name.to_string(),
                    };
                    let mut err_with_span = TypeErrorWithSpan::new(err, span);
                    err_with_span.help = Some(format!(
                        "either add `~ {}` to the function signature:\n  fn {}() ~ {} {{ ... }}\n\nor handle the effect with a handler:\n  handle {{ ... }} with {{\n      {}.operation(args) => |resume| {{\n          // handle the operation\n          resume(())\n      }},\n  }}",
                        body_eff.name, func_name, body_eff.name, body_eff.name
                    ));
                    self.errors.push(err_with_span);
                }
            } else if !expected_effects.is_empty() && !body_effects.is_empty() {
                // Check that body effects are a subset of declared effects
                let declared_names: Vec<String> = expected_effects
                    .effects
                    .iter()
                    .map(|e| e.name.to_string())
                    .collect();
                for body_eff in &body_effects.effects {
                    if !expected_effects.contains(body_eff) {
                        let err = TypeError::UndeclaredEffect {
                            func_name: func_name.clone(),
                            effect_name: body_eff.name.to_string(),
                            declared_effects: declared_names.clone(),
                        };
                        let mut err_with_span = TypeErrorWithSpan::new(err, span);
                        err_with_span.help =
                            Some(format!("add `{}` to the effect annotations", body_eff.name));
                        self.errors.push(err_with_span);
                    }
                }
            }

            // Collect errors from inference
            self.errors.extend(infer_errors);

            self.ctx.pop_scope();
        }
    }

    /// Lower effect annotations from AST paths to an EffectRow.
    fn lower_effect_annotations(&self, effects: &[ast::Path]) -> super::effects::EffectRow {
        if effects.is_empty() {
            return super::effects::EffectRow::empty();
        }

        let mut row = super::effects::EffectRow::empty();
        for path in effects {
            if let Some(ident) = path.last_ident() {
                let effect = super::effects::Effect::new(ident.name.as_ref());
                row.add(effect);
            }
        }
        row
    }

    fn check_impl(&mut self, impl_: &ast::ImplDef, span: Span) {
        self.ctx.push_scope(ScopeKind::Block);

        // Add generic parameters
        for (idx, param) in impl_.generics.params.iter().enumerate() {
            if let ast::GenericParamKind::Type { .. } = &param.kind {
                let ty = Ty::param(param.ident.name.clone(), idx as u32);
                self.ctx.define_type_param(param.ident.name.clone(), ty);
            }
        }

        let self_ty = self.lower_type(&impl_.self_ty);

        // Set the Self type for type resolution within the impl block
        self.ctx.set_self_ty(Some(self_ty.clone()));

        if let Some(trait_ref) = &impl_.trait_ref {
            // Trait implementation
            self.check_trait_impl(impl_, &self_ty, trait_ref, span);
        } else {
            // Inherent implementation
            self.check_inherent_impl(impl_, &self_ty, span);
        }

        // Clear the Self type when leaving the impl block
        self.ctx.set_self_ty(None);
        self.ctx.pop_scope();
    }

    fn check_trait_impl(
        &mut self,
        impl_: &ast::ImplDef,
        self_ty: &Ty,
        trait_ref: &ast::TraitRef,
        span: Span,
    ) {
        // Look up trait
        let trait_name = trait_ref
            .path
            .last_ident()
            .map(|i| i.name.as_ref())
            .unwrap_or("");

        let trait_def = self.ctx.lookup_trait_by_name(trait_name).cloned();

        if trait_def.is_none() {
            self.error(
                TypeError::UndefinedType {
                    name: trait_name.to_string(),
                },
                span,
            );
            return;
        }

        let trait_def = trait_def.unwrap();

        // Check that all required items are implemented
        for method in &trait_def.methods {
            if !method.has_default {
                let found = impl_.items.iter().any(|item| {
                    if let ImplItemKind::Function(f) = &item.kind {
                        f.name.name.as_ref() == method.name.as_ref()
                    } else {
                        false
                    }
                });

                if !found {
                    self.error(
                        TypeError::UndefinedMethod {
                            ty: self_ty.clone(),
                            method: method.name.to_string(),
                        },
                        span,
                    );
                }
            }
        }

        // Check each impl item
        for item in &impl_.items {
            self.check_impl_item(item, self_ty);
        }

        // Register the implementation - collect associated types and methods
        let generics = self.collect_generics(&impl_.generics);

        // Collect associated types from impl items
        let mut assoc_types = std::collections::HashMap::new();
        for item in &impl_.items {
            if let ImplItemKind::Type { name, ty, .. } = &item.kind {
                let lowered_ty = self.lower_type(ty);
                assoc_types.insert(name.name.clone(), lowered_ty);
            }
        }

        // Collect method signatures from impl items
        let mut methods: std::collections::HashMap<Arc<str>, DefId> =
            std::collections::HashMap::new();
        for item in &impl_.items {
            if let ImplItemKind::Function(f) = &item.kind {
                let method_def_id = self.ctx.fresh_def_id();
                methods.insert(f.name.name.clone(), method_def_id);
            }
        }

        // Collect where clauses from the impl's where clause
        let where_clauses = impl_
            .generics
            .where_clause
            .as_ref()
            .map(|wc| self.collect_where_predicates(wc))
            .unwrap_or_default();

        let trait_impl = TraitImpl {
            trait_id: trait_def.def_id,
            self_ty: self_ty.clone(),
            generics,
            assoc_types,
            methods,
            where_clauses,
        };

        self.ctx.register_impl(trait_impl);
    }

    fn check_inherent_impl(&mut self, impl_: &ast::ImplDef, self_ty: &Ty, _span: Span) {
        // Extract the type DefId for inherent method registration
        let type_name = Self::extract_type_name_from_ast(&impl_.self_ty);
        let type_def_id = type_name.as_ref().and_then(|n| {
            self.ctx.lookup_type_by_name(n).map(|td| td.def_id)
        });

        // PASS 1: Pre-register ALL method signatures and constants before
        // checking any bodies. This fixes forward references: method A can
        // call method B even if B is defined after A in the source.
        for item in &impl_.items {
            match &item.kind {
                ImplItemKind::Const { name, ty, .. } => {
                    let const_ty = self.lower_type(ty);
                    self.ctx.define_var(name.name.clone(), const_ty);
                }
                ImplItemKind::Function(f) => {
                    if let Some(def_id) = type_def_id {
                        let sig = self.build_fn_sig_from_ast(f);
                        self.ctx.register_inherent_method(
                            def_id,
                            f.name.name.clone(),
                            sig,
                        );
                    }
                }
                _ => {}
            }
        }

        // PASS 2: Check method bodies (all signatures already registered).
        for item in &impl_.items {
            self.check_impl_item(item, self_ty);
        }
    }

    /// Extract a type name string from an AST Type node (for inherent impl registration).
    fn extract_type_name_from_ast(ty: &ast::Type) -> Option<String> {
        match &ty.kind {
            ast::TypeKind::Path(path) => path.last_ident().map(|i| i.name.to_string()),
            _ => None,
        }
    }

    /// Build a FnSig from an AST function definition for method registration.
    fn build_fn_sig_from_ast(&mut self, f: &ast::FnDef) -> FnSig {
        let params: Vec<(Arc<str>, Ty)> = f
            .sig
            .params
            .iter()
            .map(|p| {
                let name = match &p.pattern.kind {
                    ast::PatternKind::Ident { name, .. } => name.name.clone(),
                    _ => Arc::from("_"),
                };
                let ty = if name.as_ref() == "self" {
                    // self parameter — use a fresh var since we don't need the exact type
                    Ty::fresh_var()
                } else {
                    self.lower_type(&p.ty)
                };
                (name, ty)
            })
            .collect();

        let ret = f
            .sig
            .return_ty
            .as_ref()
            .map(|t| self.lower_type(t))
            .unwrap_or(Ty::unit());

        // Extract lifetime parameters from generics
        let lifetime_params: Vec<Arc<str>> = f
            .generics
            .params
            .iter()
            .filter_map(|p| {
                if let ast::GenericParamKind::Lifetime { .. } = &p.kind {
                    Some(p.ident.name.clone())
                } else {
                    None
                }
            })
            .collect();

        FnSig {
            generics: Vec::new(),
            lifetime_params,
            params,
            ret,
            is_unsafe: f.sig.is_unsafe,
            is_async: f.sig.is_async,
            is_const: f.sig.is_const,
            where_clauses: Vec::new(),
        }
    }

    fn check_impl_item(&mut self, item: &ast::ImplItem, _self_ty: &Ty) {
        match &item.kind {
            ImplItemKind::Function(f) => {
                self.check_function(f, item.span);
            }
            ImplItemKind::Const { name, ty, value } => {
                let c = ast::ConstDef {
                    name: name.clone(),
                    ty: ty.clone(),
                    value: Some(value.clone()),
                };
                self.check_const(&c, item.span);
            }
            ImplItemKind::Type { .. } => {
                // Type alias in impl - already collected
            }
            ImplItemKind::Macro { .. } => {
                // Macro in impl - handled during expansion
            }
        }
    }

    fn check_const(&mut self, c: &ast::ConstDef, span: Span) {
        let ty = self.lower_type(&c.ty);

        if let Some(init) = &c.value {
            // Use block to limit TypeInfer borrow scope
            let (init_ty, infer_errors) = {
                let mut infer = TypeInfer::new(self.ctx);
                let init_ty = infer.infer_expr(init);
                (init_ty, infer.take_errors())
            };

            if let Err(_) = super::unify::unify(&ty, &init_ty) {
                self.error(
                    TypeError::TypeMismatch {
                        expected: ty.clone(),
                        found: init_ty,
                    },
                    span,
                );
            }

            self.errors.extend(infer_errors);
        }

        self.ctx.define_var(c.name.name.clone(), ty);
    }

    fn check_static(&mut self, s: &ast::StaticDef, span: Span) {
        let ty = self.lower_type(&s.ty);

        if let Some(init) = &s.value {
            // Use block to limit TypeInfer borrow scope
            let (init_ty, infer_errors) = {
                let mut infer = TypeInfer::new(self.ctx);
                let init_ty = infer.infer_expr(init);
                (init_ty, infer.take_errors())
            };

            if let Err(_) = super::unify::unify(&ty, &init_ty) {
                self.error(
                    TypeError::TypeMismatch {
                        expected: ty.clone(),
                        found: init_ty,
                    },
                    span,
                );
            }

            self.errors.extend(infer_errors);
        }

        self.ctx.define_var(s.name.name.clone(), ty);
    }

    /// Resolve a `use` statement, importing bindings from a module into the
    /// current scope.
    fn resolve_use(&mut self, tree: &ast::UseTree) {
        match &tree.kind {
            ast::UseTreeKind::Simple { path, rename } => {
                // use foo::bar; or use foo::bar as baz;
                if path.segments.len() >= 2 {
                    let module = path.segments[0].ident.name.as_ref();
                    let item = &path.segments[path.segments.len() - 1].ident.name;
                    let local_name = rename
                        .as_ref()
                        .map(|r| r.name.clone())
                        .unwrap_or_else(|| item.clone());

                    if let Some(ty) = self.ctx.lookup_module_binding(module, item.as_ref()) {
                        self.ctx.define_var(local_name, ty);
                    }
                }
            }
            ast::UseTreeKind::Glob(path) => {
                // use foo::*;
                if let Some(ident) = path.last_ident() {
                    let module = ident.name.as_ref();
                    if let Some(bindings) = self.ctx.clone_module_bindings(module) {
                        for (name, scheme) in bindings {
                            self.ctx.define_var(name, scheme.instantiate());
                        }
                    }
                }
            }
            ast::UseTreeKind::Nested { path: _, trees } => {
                // use foo::{bar, baz};
                for sub_tree in trees {
                    self.resolve_use(sub_tree);
                }
            }
        }
    }

    fn check_mod(&mut self, m: &ast::ModDef) {
        // External module: `mod foo;` loads foo.quanta from disk
        if m.content.is_none() {
            if let Some(ref dir) = self.source_dir {
                let mod_name = m.name.name.as_ref();
                let mod_path = dir.join(format!("{}.quanta", mod_name));
                if mod_path.exists() {
                    if let Ok(source_text) = std::fs::read_to_string(&mod_path) {
                        let source = crate::lexer::SourceFile::new(
                            mod_path.to_string_lossy().as_ref(),
                            source_text,
                        );
                        let mut lexer = crate::lexer::Lexer::new(&source);
                        if let Ok(tokens) = lexer.tokenize() {
                            let mut parser = crate::parser::Parser::new(&source, tokens);
                            if let Ok(module_ast) = parser.parse() {
                                // Process the external module's items as if they were inline
                                self.ctx.push_scope(ScopeKind::Module);
                                for item in &module_ast.items {
                                    self.collect_item(item);
                                }
                                for item in &module_ast.items {
                                    self.check_item(item);
                                }
                                let module_name = m.name.name.clone();
                                let bindings = self.ctx.current_scope_bindings();
                                self.ctx
                                    .register_module_bindings(module_name.clone(), bindings);
                                // Re-export to parent scope
                                for item in &module_ast.items {
                                    match &item.kind {
                                        ItemKind::Function(f) => {
                                            self.collect_function(f, item.span)
                                        }
                                        ItemKind::Struct(s) => {
                                            if self
                                                .ctx
                                                .lookup_type_by_name(s.name.name.as_ref())
                                                .is_none()
                                            {
                                                self.collect_struct(s, item.span);
                                            }
                                        }
                                        ItemKind::Enum(e) => {
                                            if self
                                                .ctx
                                                .lookup_type_by_name(e.name.name.as_ref())
                                                .is_none()
                                            {
                                                self.collect_enum(e, item.span);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                self.ctx.pop_scope();
                            }
                        }
                    }
                }
            }
            return;
        }
        if let Some(content) = &m.content {
            self.ctx.push_scope(ScopeKind::Module);

            // First pass: collect
            for item in &content.items {
                self.collect_item(item);
            }

            // Second pass: check
            for item in &content.items {
                self.check_item(item);
            }

            // Save module bindings for use-statement resolution before
            // popping the scope (so we capture the module's definitions).
            let module_name = m.name.name.clone();
            let bindings = self.ctx.current_scope_bindings();
            self.ctx.register_module_bindings(module_name, bindings);

            self.ctx.pop_scope();

            // Re-export pub items to parent scope (implicit `use mod::*`).
            // This is the QuantaLang ecosystem convention — module contents
            // are accessible by bare name from the parent scope.
            //
            // IMPORTANT: For structs and enums, reuse the existing DefId from
            // the first registration (inside the module scope) instead of
            // calling collect_struct/collect_enum which would create a NEW
            // DefId.  A duplicated DefId causes type mismatches when code
            // inside the module constructs a value (using the original DefId)
            // but the return-type annotation resolves to the re-exported DefId.
            for item in &content.items {
                match &item.kind {
                    ItemKind::Const(c) => {
                        let ty = self.lower_type(&c.ty);
                        self.ctx.define_var(c.name.name.clone(), ty);
                    }
                    ItemKind::Function(f) => {
                        self.collect_function(f, item.span);
                    }
                    ItemKind::Struct(s) => {
                        // Reuse the existing type registration if it exists,
                        // so that the DefId is identical to the one used inside
                        // the module scope.
                        if self.ctx.lookup_type_by_name(s.name.name.as_ref()).is_none() {
                            self.collect_struct(s, item.span);
                        }
                    }
                    ItemKind::Enum(e) => {
                        // Same as structs: reuse existing DefId.
                        if self.ctx.lookup_type_by_name(e.name.name.as_ref()).is_none() {
                            self.collect_enum(e, item.span);
                        }
                    }
                    ItemKind::Impl(impl_) => {
                        // Re-export inherent methods to parent scope so they're
                        // accessible when code outside the module calls methods
                        // on the re-exported types.
                        self.collect_impl(impl_, item.span);
                    }
                    _ => {}
                }
            }
        }
    }

    // =========================================================================
    // HELPER METHODS
    // =========================================================================

    fn collect_generics(&mut self, generics: &ast::Generics) -> Vec<GenericParam> {
        generics
            .params
            .iter()
            .enumerate()
            .map(|(idx, p)| {
                let kind = match &p.kind {
                    ast::GenericParamKind::Type { bounds, .. } => GenericParamKind::Type {
                        bounds: bounds
                            .iter()
                            .filter_map(|b| self.lower_type_bound(b))
                            .collect(),
                    },
                    ast::GenericParamKind::Lifetime { .. } => GenericParamKind::Lifetime,
                    ast::GenericParamKind::Const { ty, .. } => GenericParamKind::Const {
                        ty: self.lower_type(ty),
                    },
                };

                GenericParam {
                    name: p.ident.name.clone(),
                    index: idx as u32,
                    kind,
                }
            })
            .collect()
    }

    fn lower_fn_sig(&mut self, generics: &ast::Generics, sig: &ast::FnSig) -> FnSig {
        let gen_params = self.collect_generics(generics);

        let params: Vec<_> = sig
            .params
            .iter()
            .map(|p| {
                let name = match &p.pattern.kind {
                    ast::PatternKind::Ident { name, .. } => name.name.clone(),
                    _ => Arc::from("_"),
                };
                (name, self.lower_type(&p.ty))
            })
            .collect();

        let ret = sig
            .return_ty
            .as_ref()
            .map(|t| self.lower_type(t))
            .unwrap_or(Ty::unit());

        let where_clauses = generics
            .where_clause
            .as_ref()
            .map(|wc| {
                wc.predicates
                    .iter()
                    .map(|p| WhereClause {
                        ty: self.lower_type(&p.ty),
                        bounds: p
                            .bounds
                            .iter()
                            .filter_map(|b| self.lower_type_bound(b))
                            .collect(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let lifetime_params: Vec<Arc<str>> = generics
            .params
            .iter()
            .filter_map(|p| {
                if let ast::GenericParamKind::Lifetime { .. } = &p.kind {
                    Some(p.ident.name.clone())
                } else {
                    None
                }
            })
            .collect();

        FnSig {
            generics: gen_params,
            lifetime_params,
            params,
            ret,
            is_unsafe: sig.is_unsafe,
            is_async: sig.is_async,
            is_const: sig.is_const,
            where_clauses,
        }
    }

    fn lower_type_bound(&mut self, bound: &ast::TypeBound) -> Option<TraitBound> {
        // Look up trait by path
        let trait_name = bound.path.last_ident().map(|i| &*i.name)?;

        let trait_def = self.ctx.lookup_trait_by_name(trait_name)?;
        let trait_id = trait_def.def_id; // Extract before the borrow ends

        // Collect type arguments from the trait bound's path generic args
        let args = bound
            .path
            .segments
            .last()
            .map(|seg| {
                seg.generics
                    .iter()
                    .filter_map(|arg| match arg {
                        ast::GenericArg::Type(ty) => Some(self.lower_type(ty)),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        Some(TraitBound { trait_id, args })
    }

    fn collect_where_predicates(&mut self, wc: &ast::WhereClause) -> Vec<WhereClause> {
        wc.predicates
            .iter()
            .map(|pred| {
                let ty = self.lower_type(&pred.ty);
                let bounds = pred
                    .bounds
                    .iter()
                    .filter_map(|b| self.lower_type_bound(b))
                    .collect();
                WhereClause { ty, bounds }
            })
            .collect()
    }

    fn lower_type(&mut self, ty: &ast::Type) -> Ty {
        // Create a temporary inference context for type lowering
        let mut infer = TypeInfer::new(self.ctx);
        infer.lower_type(ty)
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
            _ => {}
        }
    }

    fn eval_const_int(&self, expr: &ast::Expr) -> Option<i128> {
        // Basic const evaluation for integer literals and simple expressions
        match &expr.kind {
            ast::ExprKind::Literal(ast::Literal::Int { value, .. }) => Some(*value as i128),
            ast::ExprKind::Unary {
                op: ast::UnaryOp::Neg,
                expr: operand,
            } => self.eval_const_int(operand).map(|n| -n),
            ast::ExprKind::Binary { op, left, right } => {
                let l = self.eval_const_int(left)?;
                let r = self.eval_const_int(right)?;
                match op {
                    ast::BinOp::Add => Some(l.checked_add(r)?),
                    ast::BinOp::Sub => Some(l.checked_sub(r)?),
                    ast::BinOp::Mul => Some(l.checked_mul(r)?),
                    ast::BinOp::Div if r != 0 => Some(l.checked_div(r)?),
                    ast::BinOp::Rem if r != 0 => Some(l.checked_rem(r)?),
                    ast::BinOp::Shl => Some(l.checked_shl(r as u32)?),
                    ast::BinOp::Shr => Some(l.checked_shr(r as u32)?),
                    ast::BinOp::BitAnd => Some(l & r),
                    ast::BinOp::BitOr => Some(l | r),
                    ast::BinOp::BitXor => Some(l ^ r),
                    _ => None,
                }
            }
            ast::ExprKind::Paren(inner) => self.eval_const_int(inner),
            _ => None,
        }
    }
}

impl Default for TypeChecker<'_> {
    fn default() -> Self {
        panic!("TypeChecker requires a context")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_checker_creation() {
        let mut ctx = TypeContext::new();
        let checker = TypeChecker::new(&mut ctx);
        assert!(!checker.has_errors());
    }
}
