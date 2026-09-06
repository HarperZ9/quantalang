// ===============================================================================
// BUILDLANG TYPE SYSTEM - TYPE CHECKER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! Type checker for items (functions, structs, enums, traits, impls).
//!
//! This module handles type checking at the item level, while `infer.rs`
//! handles expression-level type inference.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::ast::{self, ImplItemKind, ItemKind, StructFields, TraitItemKind};
use crate::lexer::{SourceId, Span};

use super::context::*;
use super::error::*;
use super::infer::TypeInfer;
use super::ty::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionEffectSummary {
    pub function: String,
    pub declared_effects: Vec<String>,
    pub observed_capabilities: BTreeMap<String, BTreeSet<String>>,
    pub propagated_effects: BTreeMap<String, BTreeSet<String>>,
}

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
    /// Canonicalized paths of `mod foo;` files currently on the load stack.
    /// A file that is already being loaded must not be loaded again, or a
    /// `mod NAME;` chain that comes back to itself would recurse until the
    /// stack overflows. Fail closed: a repeated path is a cycle diagnostic,
    /// not a crash. Paths are removed after their subtree finishes so a
    /// diamond (two siblings importing the same leaf) is not a false cycle.
    loading_modules: HashSet<PathBuf>,
    /// Cache of `use` module path (`a::b::c`) -> the set of top-level item
    /// names that module file exports (`None` when no backing file was found or
    /// it failed to parse). Reading a module's export names is read-only: it
    /// never mutates the type context, so binding an imported leaf as an opaque
    /// value can never perturb the consumer's own types the way collecting the
    /// module's items would. The cache keeps each module parsed at most once.
    use_module_exports: HashMap<Arc<str>, Option<HashSet<Arc<str>>>>,
    /// Source text for span-backed token evidence during inference.
    source_text: Option<Arc<str>>,
    /// Source ID associated with `source_text`.
    source_id: Option<SourceId>,
    /// Per-function declared/observed effect evidence for receipts.
    function_effect_summaries: Vec<FunctionEffectSummary>,
}

impl<'ctx> TypeChecker<'ctx> {
    /// Create a new type checker.
    pub fn new(ctx: &'ctx mut TypeContext) -> Self {
        Self {
            ctx,
            errors: Vec::new(),
            effect_ctx: super::effects::EffectContext::new(),
            source_dir: None,
            loading_modules: HashSet::new(),
            use_module_exports: HashMap::new(),
            source_text: None,
            source_id: None,
            function_effect_summaries: Vec::new(),
        }
    }

    /// Set the source directory for resolving `mod foo;` declarations.
    pub fn set_source_dir(&mut self, dir: std::path::PathBuf) {
        self.source_dir = Some(dir);
    }

    /// Set source text for span-backed token evidence.
    pub fn set_source_text(&mut self, source_text: impl Into<Arc<str>>) {
        self.source_text = Some(source_text.into());
        self.source_id = None;
    }

    /// Set source file data for span-backed token evidence.
    pub fn set_source_file(&mut self, source_file: &crate::lexer::SourceFile) {
        self.source_text = Some(source_file.source.clone());
        self.source_id = Some(source_file.id);
    }

    /// Get a reference to the effect context.
    pub fn effect_ctx(&self) -> &super::effects::EffectContext {
        &self.effect_ctx
    }

    /// Get collected errors.
    pub fn errors(&self) -> &[TypeErrorWithSpan] {
        &self.errors
    }

    /// Get per-function effect summaries collected during the latest module check.
    pub fn function_effect_summaries(&self) -> &[FunctionEffectSummary] {
        &self.function_effect_summaries
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
        self.function_effect_summaries.clear();

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

        // Linear containment: a non-`#[linear]` aggregate may not hold a linear
        // field (it would launder the resource past no-cloning). Runs after all
        // types are collected so every field type's linearity is known.
        self.validate_linear_containment(module);

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

        // mat4 - registered as opaque (no user-accessible fields)
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

        // uvec3 { x: u32, y: u32, z: u32 } - the type of GPU compute built-in
        // thread-index variables (`gl_GlobalInvocationID`, etc.). Components are
        // unsigned 32-bit so `gl_GlobalInvocationID.x` yields a usable index.
        let u32_ty = Ty::int(IntTy::U32);
        let def_id = self.ctx.fresh_def_id();
        self.ctx.register_type(TypeDef {
            def_id,
            name: Arc::from("uvec3"),
            generics: Vec::new(),
            kind: TypeDefKind::Struct(StructDef {
                fields: vec![
                    (Arc::from("x"), u32_ty.clone()),
                    (Arc::from("y"), u32_ty.clone()),
                    (Arc::from("z"), u32_ty.clone()),
                ],
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
            ItemKind::Struct(s) => {
                self.collect_struct(s, item.span, Self::has_linear_attr(&item.attrs))
            }
            ItemKind::Enum(e) => {
                self.collect_enum(e, item.span, Self::has_linear_attr(&item.attrs))
            }
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
        } else if m.is_file_module {
            // File-level `module NAME` header: the rest of the file already IS
            // the body, so there is nothing to load. Loading `NAME.bld` here
            // would re-open the file that declared the header (its own stem)
            // and recurse until the stack overflows.
        } else if let Some(ref dir) = self.source_dir.clone() {
            // External submodule: `mod NAME;` loads NAME.bld from disk.
            let mod_name = m.name.name.as_ref();
            let mod_path = dir.join(format!("{}.bld", mod_name));
            if mod_path.exists() {
                let key = std::fs::canonicalize(&mod_path).unwrap_or_else(|_| mod_path.clone());
                if !self.loading_modules.insert(key.clone()) {
                    // Already on the load stack: a cycle. Fail closed with a
                    // diagnostic instead of recursing into a stack overflow.
                    self.error(
                        TypeError::ModuleImportCycle {
                            path: mod_path.display().to_string(),
                            module: mod_name.to_string(),
                        },
                        m.name.span,
                    );
                    return;
                }
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
                self.loading_modules.remove(&key);
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
        let type_def_id = type_name
            .as_ref()
            .and_then(|n| self.ctx.lookup_type_by_name(n).map(|td| td.def_id));

        for item in &impl_.items {
            match &item.kind {
                ImplItemKind::Function(f) => {
                    if let Some(def_id) = type_def_id {
                        let sig = self.build_fn_sig_from_ast(f);
                        self.ctx
                            .register_inherent_method(def_id, f.name.name.clone(), sig);
                    }
                }
                ImplItemKind::Const { name, ty, .. } => {
                    // Register associated constants at module scope so they're
                    // accessible from other impl blocks (e.g., BRADFORD in
                    // chromatic_adaptation.bld).
                    let const_ty = self.lower_type(ty);
                    self.ctx.define_var(name.name.clone(), const_ty);
                }
                _ => {}
            }
        }

        self.ctx.pop_scope();
    }

    /// Whether an item's attribute list contains the `#[linear]` marker.
    fn has_linear_attr(attrs: &[ast::Attribute]) -> bool {
        attrs.iter().any(|attr| {
            attr.path
                .segments
                .first()
                .map_or(false, |seg| seg.ident.name.as_ref() == "linear")
        })
    }

    /// Enforce the linear containment rule: a non-`#[linear]` struct or enum
    /// must not have a field whose type is `#[linear]`. Otherwise the linear
    /// value could be read out of the untracked aggregate repeatedly,
    /// laundering it past no-cloning.
    fn validate_linear_containment(&mut self, module: &ast::Module) {
        for item in &module.items {
            if Self::has_linear_attr(&item.attrs) {
                continue; // a linear aggregate is itself move-tracked
            }
            match &item.kind {
                ItemKind::Struct(s) => {
                    let container = s.name.name.to_string();
                    self.check_fields_not_linear(&container, &s.fields, item.span);
                }
                ItemKind::Enum(e) => {
                    let container = e.name.name.to_string();
                    for variant in &e.variants {
                        self.check_fields_not_linear(&container, &variant.fields, item.span);
                    }
                }
                _ => {}
            }
        }
    }

    /// Report any field of `fields` whose type resolves to a `#[linear]` type.
    fn check_fields_not_linear(&mut self, container: &str, fields: &ast::StructFields, span: Span) {
        let raw: Vec<(String, &ast::Type)> = match fields {
            StructFields::Named(fs) => fs
                .iter()
                .map(|f| (f.name.name.to_string(), f.ty.as_ref()))
                .collect(),
            StructFields::Tuple(fs) => fs
                .iter()
                .enumerate()
                .map(|(i, f)| (i.to_string(), f.ty.as_ref()))
                .collect(),
            StructFields::Unit => Vec::new(),
        };
        for (field_name, field_ty_ast) in raw {
            let ty = self.lower_type(field_ty_ast);
            if let TyKind::Adt(def_id, _) = &ty.kind {
                if self.ctx.is_linear_def(*def_id) {
                    let field_type = self
                        .ctx
                        .lookup_type(*def_id)
                        .map(|d| d.name.to_string())
                        .unwrap_or_else(|| "?".to_string());
                    self.errors.push(TypeErrorWithSpan::new(
                        TypeError::LinearFieldInNonLinearType {
                            container: container.to_string(),
                            field: field_name,
                            field_type,
                        },
                        span,
                    ));
                }
            }
        }
    }

    fn collect_struct(&mut self, s: &ast::StructDef, _span: Span, is_linear: bool) {
        let def_id = self.ctx.fresh_def_id();
        if is_linear {
            self.ctx.mark_linear(def_id);
        }

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

    fn collect_enum(&mut self, e: &ast::EnumDef, _span: Span, is_linear: bool) {
        let def_id = self.ctx.fresh_def_id();
        if is_linear {
            self.ctx.mark_linear(def_id);
        }

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

        // Add function to current scope - carry lifetime params for interprocedural analysis
        let param_tys: Vec<_> = sig.params.iter().map(|(_, ty)| ty.clone()).collect();
        let effects = self.lower_effect_annotations(&f.sig.effects);
        let fn_ty = Ty::function_with_effects_and_lifetimes(
            param_tys.clone(),
            sig.ret.clone(),
            effects,
            sig.lifetime_params.clone(),
        );

        // Multiple dispatch: append this definition to the name's candidate set.
        // The FIRST definition of a name is also bound as a plain variable so a
        // bare value reference (fn pointer) still resolves and the single-def
        // fast path in `infer_call` is byte-for-byte identical to before.
        // Subsequent same-name definitions do NOT overwrite that binding (which
        // is what silently clobbered overloads previously); they only extend the
        // candidate set, and the call site resolves by the argument-type tuple.
        let is_first = self.ctx.method_candidates(f.name.name.as_ref()).is_none();
        self.ctx.register_method_candidate(
            f.name.name.clone(),
            MethodCandidate {
                def_id,
                param_tys,
                ret: sig.ret.clone(),
                sig,
            },
        );
        if is_first {
            self.ctx.define_var(f.name.name.clone(), fn_ty);
        }
    }

    /// Collect extern block declarations. Foreign functions and statics are
    /// registered in the type context so access can be type-checked.
    fn collect_extern_block(&mut self, eb: &ast::ExternBlockDef, _span: Span) {
        for foreign_item in &eb.items {
            match &foreign_item.kind {
                ast::ForeignItemKind::Fn(f) => {
                    let def_id = self.ctx.fresh_def_id();
                    let sig = self.lower_fn_sig(&f.generics, &f.sig);
                    self.ctx.register_function(def_id, sig.clone());

                    let param_tys: Vec<_> = sig.params.iter().map(|(_, ty)| ty.clone()).collect();
                    let effect_name =
                        super::capabilities::capability_effect_for_call(f.name.name.as_ref())
                            .unwrap_or(super::capabilities::FOREIGN);
                    let effects = super::effects::EffectRow::closed([super::effects::Effect::new(
                        effect_name,
                    )]);
                    let fn_ty = Ty::function_with_effects_and_lifetimes(
                        param_tys,
                        sig.ret.clone(),
                        effects,
                        sig.lifetime_params.clone(),
                    )
                    .with_variadic(f.sig.is_variadic);
                    self.ctx.define_var(f.name.name.clone(), fn_ty);
                    self.ctx.register_foreign_function(f.name.name.clone());
                }
                ast::ForeignItemKind::Static { name, ty, .. } => {
                    let static_ty = self.lower_type(ty);
                    self.ctx.define_var(name.name.clone(), static_ty);
                    self.ctx.register_foreign_static(name.name.clone());
                }
                _ => {}
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

    /// Register the trait bounds declared by a set of generics (both inline
    /// `<T: Trait>` bounds and `where T: Trait` predicates) so that method
    /// resolution can find trait methods on those type parameters. Bounds
    /// accumulate onto whatever is already registered (see
    /// `TypeContext::register_param_bounds`), so an enclosing impl's bounds and
    /// a method's own bounds compose rather than clobber each other.
    fn register_generic_param_bounds(&mut self, generics: &ast::Generics) {
        for param in &generics.params {
            if let ast::GenericParamKind::Type { ref bounds, .. } = &param.kind {
                if !bounds.is_empty() {
                    let trait_names = Self::bound_trait_names(bounds);
                    if !trait_names.is_empty() {
                        self.ctx
                            .register_param_bounds(param.ident.name.clone(), trait_names);
                    }
                }
            }
        }

        for pred in generics.where_clause.iter().flat_map(|wc| &wc.predicates) {
            if let ast::TypeKind::Path(ref path) = pred.ty.kind {
                if let Some(seg) = path.segments.last() {
                    let trait_names = Self::bound_trait_names(&pred.bounds);
                    if !trait_names.is_empty() {
                        self.ctx
                            .register_param_bounds(seg.ident.name.clone(), trait_names);
                    }
                }
            }
        }
    }

    /// Extract the trait names from a list of bounds, dropping `?Sized`-style
    /// maybe bounds (the last path segment is used as the trait's name).
    fn bound_trait_names(bounds: &[ast::TypeBound]) -> Vec<Arc<str>> {
        bounds
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
            .collect()
    }

    fn check_function(&mut self, f: &ast::FnDef, span: Span) {
        if let Some(body) = &f.body {
            // Generic functions are checked per-instantiation at
            // monomorphization; the abstract body cannot resolve
            // type-parameter method calls or closure-param returns.
            let is_generic = f
                .generics
                .params
                .iter()
                .any(|p| matches!(p.kind, ast::GenericParamKind::Type { .. }));
            self.ctx.push_scope(ScopeKind::Function);

            // Layer this function's own type-parameter bounds on top of any
            // bounds already in scope (e.g. from an enclosing `impl<T: Bound>`),
            // and restore the prior set when the function is done so sibling
            // items are not affected. Without preserving the enclosing bounds, a
            // trait-impl method body could not resolve trait methods on the
            // impl's type parameters (e.g. `self[i].cmp(..)` in `impl<T: Ord>
            // Ord for [T]`).
            let saved_param_bounds = self.ctx.param_bounds_snapshot();

            // Add generic parameters and register their trait bounds.
            for (idx, param) in f.generics.params.iter().enumerate() {
                if let ast::GenericParamKind::Type { .. } = &param.kind {
                    let ty = Ty::param(param.ident.name.clone(), idx as u32);
                    self.ctx.define_type_param(param.ident.name.clone(), ty);
                }
            }
            self.register_generic_param_bounds(&f.generics);

            // Add function parameters. Capture linear-typed Ident params so we
            // can hand them to the body inferer for no-cloning tracking (the
            // inferer is created fresh below and does not see these bindings).
            let mut param_bindings: Vec<(String, Ty)> = Vec::new();
            for param in &f.sig.params {
                let ty = self.lower_type(&param.ty);
                self.bind_pattern(&param.pattern, &ty);
                if let ast::PatternKind::Ident { name, .. } = &param.pattern.kind {
                    param_bindings.push((name.name.as_ref().to_string(), ty));
                }
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
            let (
                body_ty,
                body_effects,
                capability_sources,
                propagated_effect_sources,
                infer_errors,
                has_return,
            ) = {
                let mut infer = if let Some(source_text) = &self.source_text {
                    TypeInfer::with_source_text(self.ctx, source_text.clone(), self.source_id)
                } else {
                    TypeInfer::new(self.ctx)
                };
                // Pass the expected return type so that `return` statements
                // inside nested control flow (while/if/match) are properly
                // type-checked against the function signature.
                infer.set_return_ty(expected_ret.clone());
                // Hand linear-typed parameters to the inferer so consuming a
                // linear parameter more than once is rejected (no-cloning).
                for (name, ty) in &param_bindings {
                    infer.register_linear_param(name, ty);
                }
                // Register all user-defined effects so infer_perform can resolve them
                for eff in user_effects {
                    infer.register_effect(eff);
                }
                let body_ty = infer.infer_block(body);
                let body_effects = infer.current_effect_row().clone();
                let capability_sources = infer.capability_sources().clone();
                let propagated_effect_sources = infer.propagated_effect_sources().clone();
                let has_return = infer.has_explicit_return();
                (
                    body_ty,
                    body_effects,
                    capability_sources,
                    propagated_effect_sources,
                    infer.take_errors(),
                    has_return,
                )
            };

            // Unify body type with return type.
            // If the function contains explicit `return` statements, the body
            // type might be `()` (e.g., from a while loop that returns via
            // `return` inside an `if`). In this case, the return type was
            // already validated by infer_return(), so skip the body check.
            if !has_return && !is_generic {
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
            let mut declared_effects: Vec<String> = expected_effects
                .effects
                .iter()
                .map(|effect| effect.name.to_string())
                .collect();
            declared_effects.sort();
            self.function_effect_summaries.push(FunctionEffectSummary {
                function: func_name.clone(),
                declared_effects,
                observed_capabilities: capability_sources.clone(),
                propagated_effects: propagated_effect_sources.clone(),
            });

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
                    if let Some(sources) = capability_sources.get(body_eff.name.as_ref()) {
                        err_with_span.notes.push(format!(
                            "capability `{}` was triggered by ambient call(s): {}",
                            body_eff.name,
                            sources.iter().cloned().collect::<Vec<_>>().join(", ")
                        ));
                    }
                    if let Some(sources) = propagated_effect_sources.get(body_eff.name.as_ref()) {
                        err_with_span.notes.push(format!(
                            "capability `{}` was propagated by effectful call(s): {}",
                            body_eff.name,
                            sources.iter().cloned().collect::<Vec<_>>().join(", ")
                        ));
                    }
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
                        if let Some(sources) = capability_sources.get(body_eff.name.as_ref()) {
                            err_with_span.notes.push(format!(
                                "capability `{}` was triggered by ambient call(s): {}",
                                body_eff.name,
                                sources.iter().cloned().collect::<Vec<_>>().join(", ")
                            ));
                        }
                        if let Some(sources) = propagated_effect_sources.get(body_eff.name.as_ref())
                        {
                            err_with_span.notes.push(format!(
                                "capability `{}` was propagated by effectful call(s): {}",
                                body_eff.name,
                                sources.iter().cloned().collect::<Vec<_>>().join(", ")
                            ));
                        }
                        self.errors.push(err_with_span);
                    }
                }
            }

            // Collect errors from inference. For generic functions, defer body
            // type errors to monomorphization (the concrete instantiation at each
            // call site is the real check).
            if !is_generic {
                self.errors.extend(infer_errors);
            }

            // Restore the type-parameter bounds that were in scope on entry,
            // dropping this function's own bounds.
            self.ctx.restore_param_bounds(saved_param_bounds);

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

        // Register the impl's type-parameter trait bounds so method bodies can
        // resolve trait methods on the impl's type parameters. For
        // `impl<T: Ord> Ord for [T]`, this makes `self[i].cmp(..)` (where
        // `self[i]: T`) resolve `cmp` through the `Ord` bound. The bounds are
        // restored on exit so sibling items are unaffected.
        let saved_param_bounds = self.ctx.param_bounds_snapshot();
        self.register_generic_param_bounds(&impl_.generics);

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
        self.ctx.restore_param_bounds(saved_param_bounds);
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

        // Register the implementation BEFORE checking method bodies so that a
        // method can call sibling methods of the same trait impl (including
        // required methods and defaulted methods). Without this pre-pass,
        // `self.cmp(other)` inside `Ord::max_by` would not resolve because the
        // impl was not yet visible to method resolution.
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

        // Check each impl item (bodies can now resolve sibling trait methods).
        for item in &impl_.items {
            self.check_impl_item(item, self_ty);
        }
    }

    fn check_inherent_impl(&mut self, impl_: &ast::ImplDef, self_ty: &Ty, _span: Span) {
        // Extract the type DefId for inherent method registration
        let type_name = Self::extract_type_name_from_ast(&impl_.self_ty);
        let type_def_id = type_name
            .as_ref()
            .and_then(|n| self.ctx.lookup_type_by_name(n).map(|td| td.def_id));

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
                        self.ctx
                            .register_inherent_method(def_id, f.name.name.clone(), sig);
                    }
                }
                _ => {}
            }
        }

        // PASS 2: Check method bodies (all signatures already registered).
        // Methods of a generic impl (impl<T> ...) are checked per-instantiation
        // at monomorphization; their bodies cannot resolve the abstract type
        // parameters (method/field access on T), so defer the bodies here.
        let impl_is_generic = impl_
            .generics
            .params
            .iter()
            .any(|p| matches!(p.kind, ast::GenericParamKind::Type { .. }));
        for item in &impl_.items {
            if impl_is_generic && matches!(item.kind, ImplItemKind::Function(_)) {
                continue;
            }
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
                    // self parameter - use a fresh var since we don't need the exact type
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
            effects: self.lower_effect_annotations(&f.sig.effects),
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
        self.resolve_use_prefixed(tree, &[]);
    }

    /// Resolve a `use` tree carrying the module-path prefix accumulated from
    /// enclosing nested groups, so `use foundation::math::{sign, cbrt}` binds
    /// `sign`/`cbrt` from the `foundation::math` module rather than losing the
    /// `foundation::math` prefix (which the flat handler used to drop).
    fn resolve_use_prefixed(&mut self, tree: &ast::UseTree, prefix: &[Arc<str>]) {
        match &tree.kind {
            ast::UseTreeKind::Simple { path, rename } => {
                let mut segs: Vec<Arc<str>> = prefix.to_vec();
                segs.extend(path.segments.iter().map(|s| s.ident.name.clone()));
                if segs.len() < 2 || Self::is_reserved_use_root(&segs[0]) {
                    return;
                }
                let item = segs[segs.len() - 1].clone();
                // `use a::b::self` names the module itself, not a value binding.
                if item.as_ref() == "self" {
                    return;
                }
                let module_segs = &segs[..segs.len() - 1];
                // First, a module registered by an in-tree `mod NAME;` (real
                // signatures). Fall back to the file-backed export set, binding
                // the imported name as an opaque value so it stops reading as
                // "undefined" without importing a concrete signature that could
                // collide with the consumer's own same-named type.
                let local_name = rename
                    .as_ref()
                    .map(|r| r.name.clone())
                    .unwrap_or_else(|| item.clone());
                if let Some(ty) = self
                    .ctx
                    .lookup_module_binding(module_segs[0].as_ref(), item.as_ref())
                {
                    self.ctx.define_var(local_name, ty);
                    return;
                }
                if self.module_exports_name(module_segs, item.as_ref()) {
                    self.ctx.define_var(local_name, Ty::fresh_var());
                }
            }
            ast::UseTreeKind::Glob(path) => {
                let mut segs: Vec<Arc<str>> = prefix.to_vec();
                segs.extend(path.segments.iter().map(|s| s.ident.name.clone()));
                if segs.is_empty() || Self::is_reserved_use_root(&segs[0]) {
                    return;
                }
                // Real bindings from an in-tree `mod NAME;` win; otherwise bind
                // every file-backed export name as an opaque value.
                if let Some(bindings) = self
                    .ctx
                    .clone_module_bindings(segs[segs.len() - 1].as_ref())
                {
                    for (name, scheme) in bindings {
                        self.ctx.define_var(name, scheme.instantiate());
                    }
                    return;
                }
                if let Some(names) = self.module_export_set(&segs) {
                    for name in names {
                        self.ctx.define_var(name, Ty::fresh_var());
                    }
                }
            }
            ast::UseTreeKind::Nested { path, trees } => {
                let mut new_prefix: Vec<Arc<str>> = prefix.to_vec();
                new_prefix.extend(path.segments.iter().map(|s| s.ident.name.clone()));
                for sub_tree in trees {
                    self.resolve_use_prefixed(sub_tree, &new_prefix);
                }
            }
        }
    }

    /// Roots that name the standard library or a path relative to the current
    /// crate rather than an on-disk sibling module file. These are left to the
    /// existing prelude/name resolution instead of a filesystem load.
    fn is_reserved_use_root(seg: &str) -> bool {
        matches!(seg, "std" | "core" | "crate" | "self" | "super")
    }

    /// Whether the module backing `module_segs` exports a top-level item named
    /// `name`. Thin membership test over [`Self::module_export_set`].
    fn module_exports_name(&mut self, module_segs: &[Arc<str>], name: &str) -> bool {
        self.module_export_set(module_segs)
            .is_some_and(|names| names.contains(name))
    }

    /// The set of top-level item names the `.bld` file backing a `use a::b::c`
    /// path import exports. Read-only: the module file is lexed and parsed to
    /// harvest its item names, and NOTHING is written into the type context, so
    /// no type is registered, no `DefId` is minted, and no diagnostic is
    /// produced. This keeps `use` provably additive: binding an imported leaf as
    /// a fresh var stops it reading as "undefined" without importing a concrete
    /// signature that could collide with the consumer's own same-named type.
    /// Cached per module path (`None` cached for a missing/unparseable file).
    fn module_export_set(&mut self, module_segs: &[Arc<str>]) -> Option<HashSet<Arc<str>>> {
        if module_segs.is_empty() {
            return None;
        }
        let key: Arc<str> = Arc::from(
            module_segs
                .iter()
                .map(|s| s.as_ref())
                .collect::<Vec<_>>()
                .join("::"),
        );
        if let Some(cached) = self.use_module_exports.get(&key) {
            return cached.clone();
        }
        let names = self
            .resolve_use_module_file(module_segs)
            .and_then(|path| Self::collect_export_names(&path));
        self.use_module_exports.insert(key, names.clone());
        names
    }

    /// Resolve module path segments to an on-disk `.bld` file, searching the
    /// source directory and its ancestors so a file under `tests/` can reach a
    /// `foundation/` tree sitting beside the corpus root.
    fn resolve_use_module_file(&self, module_segs: &[Arc<str>]) -> Option<PathBuf> {
        let start = self.source_dir.as_ref()?;
        let mut cur: Option<&std::path::Path> = Some(start.as_path());
        for _ in 0..8 {
            let root = cur?;
            if let Some(f) = Self::use_module_candidate(root, module_segs) {
                return Some(f);
            }
            cur = root.parent();
        }
        None
    }

    /// Build candidate files for `segs` under `root` in preference order,
    /// returning the first that exists. Directory segments may be spelled with
    /// underscores in the import but hyphens on disk (`field_tensor` ->
    /// `field-tensor/`), so both spellings are tried per segment.
    fn use_module_candidate(root: &std::path::Path, segs: &[Arc<str>]) -> Option<PathBuf> {
        let n = segs.len();
        let mut dir = root.to_path_buf();
        for seg in &segs[..n - 1] {
            let as_is = dir.join(seg.as_ref());
            let hyphen = dir.join(seg.replace('_', "-"));
            if as_is.is_dir() {
                dir = as_is;
            } else if hyphen.is_dir() {
                dir = hyphen;
            } else {
                return None;
            }
        }
        let last = &segs[n - 1];
        let last_hyphen = last.replace('_', "-");
        let mut cands: Vec<PathBuf> = vec![
            dir.join(format!("{}.bld", last)),
            dir.join(last.as_ref()).join("lib.bld"),
            dir.join(last.as_ref()).join("mod.bld"),
        ];
        if last_hyphen != last.as_ref() {
            cands.push(dir.join(format!("{}.bld", last_hyphen)));
            cands.push(dir.join(&last_hyphen).join("lib.bld"));
        }
        cands.into_iter().find(|c| c.is_file())
    }

    /// Lex and parse `path` READ-ONLY and return the set of top-level item names
    /// it exports. Touches nothing on `self`: no type context, no error list, no
    /// module stack. Returns `None` when the file cannot be read, lexed, or
    /// parsed, so an unresolved import stays silent for the existing
    /// name-resolution path rather than inventing an error.
    fn collect_export_names(path: &std::path::Path) -> Option<HashSet<Arc<str>>> {
        let source_text = std::fs::read_to_string(path).ok()?;
        let source = crate::lexer::SourceFile::new(path.to_string_lossy().as_ref(), source_text);
        let mut lexer = crate::lexer::Lexer::new(&source);
        let tokens = lexer.tokenize().ok()?;
        let mut parser = crate::parser::Parser::new(&source, tokens);
        let module_ast = parser.parse().ok()?;
        let mut names: HashSet<Arc<str>> = HashSet::new();
        for item in &module_ast.items {
            match &item.kind {
                ItemKind::Function(f) => {
                    names.insert(f.name.name.clone());
                }
                ItemKind::Struct(s) => {
                    names.insert(s.name.name.clone());
                }
                ItemKind::Enum(e) => {
                    names.insert(e.name.name.clone());
                }
                ItemKind::Const(c) => {
                    names.insert(c.name.name.clone());
                }
                ItemKind::Static(s) => {
                    names.insert(s.name.name.clone());
                }
                ItemKind::TypeAlias(ta) => {
                    names.insert(ta.name.name.clone());
                }
                ItemKind::Trait(t) => {
                    names.insert(t.name.name.clone());
                }
                ItemKind::Effect(e) => {
                    names.insert(e.name.name.clone());
                }
                ItemKind::Mod(m) => {
                    names.insert(m.name.name.clone());
                }
                _ => {}
            }
        }
        Some(names)
    }

    fn check_mod(&mut self, m: &ast::ModDef) {
        // External module: `mod foo;` loads foo.bld from disk
        if m.content.is_none() {
            if m.is_file_module {
                // File-level `module NAME` header: the rest of the file already
                // IS the body, so there is nothing to load. Loading `NAME.bld`
                // here would re-open the declaring file and recurse forever.
                return;
            }
            if let Some(ref dir) = self.source_dir {
                let mod_name = m.name.name.as_ref();
                let mod_path = dir.join(format!("{}.bld", mod_name));
                if mod_path.exists() {
                    let key = std::fs::canonicalize(&mod_path).unwrap_or_else(|_| mod_path.clone());
                    if !self.loading_modules.insert(key.clone()) {
                        // Already on the load stack: fail closed with a cycle
                        // diagnostic instead of overflowing the stack.
                        self.error(
                            TypeError::ModuleImportCycle {
                                path: mod_path.display().to_string(),
                                module: mod_name.to_string(),
                            },
                            m.name.span,
                        );
                        return;
                    }
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
                                                self.collect_struct(
                                                    s,
                                                    item.span,
                                                    Self::has_linear_attr(&item.attrs),
                                                );
                                            }
                                        }
                                        ItemKind::Enum(e) => {
                                            if self
                                                .ctx
                                                .lookup_type_by_name(e.name.name.as_ref())
                                                .is_none()
                                            {
                                                self.collect_enum(
                                                    e,
                                                    item.span,
                                                    Self::has_linear_attr(&item.attrs),
                                                );
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                self.ctx.pop_scope();
                            }
                        }
                    }
                    self.loading_modules.remove(&key);
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
            // This is the BuildLang ecosystem convention - module contents
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
                            self.collect_struct(s, item.span, Self::has_linear_attr(&item.attrs));
                        }
                    }
                    ItemKind::Enum(e) => {
                        // Same as structs: reuse existing DefId.
                        if self.ctx.lookup_type_by_name(e.name.name.as_ref()).is_none() {
                            self.collect_enum(e, item.span, Self::has_linear_attr(&item.attrs));
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
            effects: self.lower_effect_annotations(&sig.effects),
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

    fn check_source(source: &str) -> Vec<TypeErrorWithSpan> {
        let source_file = crate::lexer::SourceFile::new("capability_test.bld", source);
        let mut lexer = crate::lexer::Lexer::new(&source_file);
        let tokens = lexer.tokenize().expect("tokenize capability fixture");
        let mut parser = crate::parser::Parser::new(&source_file, tokens);
        let module = parser.parse().expect("parse capability fixture");

        let mut ctx = TypeContext::new();
        let mut checker = TypeChecker::new(&mut ctx);
        checker.set_source_file(&source_file);
        checker.check_module(&module);
        checker.take_errors()
    }

    #[test]
    fn variadic_extern_call_with_extra_args_typechecks() {
        let errors = check_source(
            "extern \"C\" { fn my_printf(fmt: &str, ...) -> i32; }\n\
             fn main() ~ Foreign { my_printf(\"%d %d\", 1, 2); }",
        );
        assert!(
            !errors
                .iter()
                .any(|e| matches!(e.error, TypeError::ArityMismatch { .. })),
            "a variadic extern call with extra args must not raise ArityMismatch: {:?}",
            errors
        );
    }

    #[test]
    fn non_variadic_call_with_extra_args_still_errors() {
        let errors = check_source(
            "extern \"C\" { fn takes_one(x: i32) -> i32; }\n\
             fn main() ~ Foreign { takes_one(1, 2); }",
        );
        assert!(
            errors
                .iter()
                .any(|e| matches!(e.error, TypeError::ArityMismatch { .. })),
            "a non-variadic call with extra args must still raise ArityMismatch: {:?}",
            errors
        );
    }

    #[test]
    fn call_through_fn_pointer_array_element_is_callable() {
        // Iterating an array of function pointers binds each element by
        // reference, so the callee is a `&fn(..) -> _`. Calling it must
        // auto-deref to the underlying function rather than report the type
        // as NotCallable. Regression for benchmarks.bld's
        // `for bench in [bench_a, bench_b, ..] { let r = bench(); }`.
        let errors = check_source(
            "fn a() -> i32 { 1 }\n\
             fn b() -> i32 { 2 }\n\
             fn run() -> i32 {\n\
                 let mut total = 0;\n\
                 for f in [a, b] {\n\
                     total = total + f();\n\
                 }\n\
                 total\n\
             }",
        );
        assert!(
            !errors
                .iter()
                .any(|e| matches!(e.error, TypeError::NotCallable { .. })),
            "calling an element of a function-pointer array must not be NotCallable: {:?}",
            errors
        );
    }

    #[test]
    fn call_through_plain_reference_to_fn_is_callable() {
        // A bare `&fn(..)` binding (not via an array) is equally callable:
        // `let g = &f; g()` auto-derefs to call `f`.
        let errors = check_source(
            "fn f() -> i32 { 7 }\n\
             fn run() -> i32 {\n\
                 let g = &f;\n\
                 g()\n\
             }",
        );
        assert!(
            !errors
                .iter()
                .any(|e| matches!(e.error, TypeError::NotCallable { .. })),
            "calling through a reference to a function must not be NotCallable: {:?}",
            errors
        );
    }

    #[test]
    fn call_on_reference_to_non_function_still_errors() {
        // Auto-deref applies only when the reference's target is a function.
        // `(&i32)()` must still be reported as not callable (fail closed).
        let errors = check_source(
            "fn run() -> i32 {\n\
                 let x = 5;\n\
                 let r = &x;\n\
                 r()\n\
             }",
        );
        assert!(
            errors
                .iter()
                .any(|e| matches!(e.error, TypeError::NotCallable { .. })),
            "calling a reference to a non-function must still be NotCallable: {:?}",
            errors
        );
    }

    // =========================================================================
    // LINEAR TYPES (no-cloning / no-double-spend) -- opt-in via `#[linear]`.
    //
    // A value whose nominal type is marked `#[linear]` may be *consumed*
    // (used by value) at most once. This is the no-cloning rule for qubits,
    // the no-double-spend rule for on-chain assets, and resource-handle
    // safety for fin-sec settlement obligations -- one type-system feature
    // serving all three. Borrows (`&q`) do not consume. Ordinary types are
    // unaffected (copy-like reuse is preserved).
    // =========================================================================

    const LINEAR_PRELUDE: &str = "#[linear]\nstruct Qubit { id: i64 }\n\
         fn consume(q: Qubit) -> i64 { q.id }\n\
         fn observe(q: &Qubit) -> i64 { 0 }\n";

    fn has_linear_use_after_move(errors: &[TypeErrorWithSpan]) -> bool {
        errors
            .iter()
            .any(|e| matches!(e.error, TypeError::LinearUseAfterMove { .. }))
    }

    #[test]
    fn linear_single_use_is_ok() {
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; \
             let a = consume(q); println(\"{{}}\", a); }}"
        ));
        assert!(
            !has_linear_use_after_move(&errors),
            "using a linear value exactly once must be clean: {errors:#?}"
        );
    }

    #[test]
    fn linear_value_used_twice_via_call_errors() {
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; \
             let a = consume(q); let b = consume(q); println(\"{{}}\", a + b); }}"
        ));
        assert!(
            has_linear_use_after_move(&errors),
            "consuming a linear value twice must be rejected (no-cloning): {errors:#?}"
        );
    }

    #[test]
    fn linear_value_moved_via_let_then_used_errors() {
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; \
             let q2 = q; let a = consume(q); println(\"{{}}\", a); }}"
        ));
        assert!(
            has_linear_use_after_move(&errors),
            "a linear value moved via `let` then used again must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_value_can_be_borrowed_without_consuming() {
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; \
             let a = observe(&q); let b = observe(&q); let c = consume(q); \
             println(\"{{}}\", a + b + c); }}"
        ));
        assert!(
            !has_linear_use_after_move(&errors),
            "borrowing a linear value must not consume it: {errors:#?}"
        );
    }

    #[test]
    fn ordinary_value_can_still_be_reused() {
        // Backward-compat guard: a non-`#[linear]` struct keeps copy-like reuse.
        let errors = check_source(
            "struct Coin { value: i64 }\n\
             fn spend(c: Coin) -> i64 { c.value }\n\
             fn main() ~ Console { let coin = Coin { value: 1 }; \
             let a = spend(coin); let b = spend(coin); println(\"{}\", a + b); }",
        );
        assert!(
            !has_linear_use_after_move(&errors),
            "ordinary (non-linear) values must remain freely reusable: {errors:#?}"
        );
    }

    #[test]
    fn linear_value_consumed_in_branch_then_used_after_errors() {
        // Conservative soundness: consumed on *any* path -> poisoned afterward.
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; \
             let cond = true; if cond {{ let a = consume(q); println(\"{{}}\", a); }} \
             let b = consume(q); println(\"{{}}\", b); }}"
        ));
        assert!(
            has_linear_use_after_move(&errors),
            "a linear value consumed in a branch then used after must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_value_consumed_in_loop_body_errors() {
        // A loop body may run more than once; consuming an outer linear there
        // is a potential double-use and must be rejected.
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; \
             let mut i = 0; while i < 2 {{ let a = consume(q); i = i + 1; }} }}"
        ));
        assert!(
            has_linear_use_after_move(&errors),
            "consuming an outer linear value inside a loop must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_param_single_use_is_ok() {
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn forward(q: Qubit) -> i64 {{ consume(q) }}\n\
             fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; println(\"{{}}\", forward(q)); }}"
        ));
        assert!(
            !has_linear_use_after_move(&errors),
            "using a linear parameter exactly once must be clean: {errors:#?}"
        );
    }

    #[test]
    fn linear_param_consumed_twice_errors() {
        // A linear function *parameter* must be subject to no-cloning too.
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn waste(q: Qubit) -> i64 {{ let a = consume(q); \
             let b = consume(q); a + b }}\n\
             fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; println(\"{{}}\", waste(q)); }}"
        ));
        assert!(
            has_linear_use_after_move(&errors),
            "consuming a linear parameter twice must be rejected: {errors:#?}"
        );
    }

    fn has_linear_containment(errors: &[TypeErrorWithSpan]) -> bool {
        errors
            .iter()
            .any(|e| matches!(e.error, TypeError::LinearFieldInNonLinearType { .. }))
    }

    #[test]
    fn nonlinear_struct_with_linear_field_is_rejected() {
        // A non-linear type cannot hold a linear field, or the linear value
        // could be laundered (read out repeatedly) past no-cloning.
        let errors = check_source(
            "#[linear]\nstruct Coin { value: i64 }\n\
             struct Wallet { coin: Coin }\n",
        );
        assert!(
            has_linear_containment(&errors),
            "a non-linear struct holding a linear field must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_struct_with_linear_field_is_allowed() {
        // Marking the container `#[linear]` makes the whole aggregate tracked.
        let errors = check_source(
            "#[linear]\nstruct Coin { value: i64 }\n\
             #[linear]\nstruct Wallet { coin: Coin }\n",
        );
        assert!(
            !has_linear_containment(&errors),
            "a linear container may hold a linear field: {errors:#?}"
        );
    }

    // -- Soundness: parse-only constructs must be rejected loudly, never
    //    silently discarded downstream. --

    #[test]
    fn let_else_is_rejected_not_silently_discarded() {
        // let-else parses, but neither the checker nor codegen handles the
        // diverge block: codegen would silently DROP the else path (a silent
        // miscompile). The checker must reject it with UnsupportedConstruct.
        let errors = check_source("fn main() { let Some(x) = opt else { return; }; }");
        assert!(
            errors
                .iter()
                .any(|e| matches!(e.error, TypeError::UnsupportedConstruct { .. })),
            "let-else must be rejected loudly, not silently miscompiled: {errors:#?}"
        );

        // A plain let (no diverge block) is unaffected.
        let errors = check_source("fn main() { let x = 1; }");
        assert!(
            !errors
                .iter()
                .any(|e| matches!(e.error, TypeError::UnsupportedConstruct { .. })),
            "a plain let must not trip the let-else rejection: {errors:#?}"
        );
    }

    // -- Soundness: a linear value may not enter a position the move-analysis
    //    cannot follow (aggregates, generics, closures, deref). These programs
    //    are the confirmed bypasses from the adversarial soundness pass; each
    //    must now be REJECTED with some linear error. --

    fn has_any_linear_error(errors: &[TypeErrorWithSpan]) -> bool {
        errors.iter().any(|e| {
            matches!(
                e.error,
                TypeError::LinearUseAfterMove { .. }
                    | TypeError::LinearFieldInNonLinearType { .. }
                    | TypeError::LinearInUnsupportedPosition { .. }
            )
        })
    }

    #[test]
    fn linear_value_in_tuple_is_rejected() {
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; \
             let t = (q, 0); let a = t.0; let b = t.0; \
             let x = consume(a); let y = consume(b); println(\"{{}}\", x + y); }}"
        ));
        assert!(
            has_any_linear_error(&errors),
            "storing a linear value in a tuple must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_value_in_array_is_rejected() {
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; \
             let arr = [q]; let a = arr[0]; let b = arr[0]; \
             let x = consume(a); let y = consume(b); println(\"{{}}\", x + y); }}"
        ));
        assert!(
            has_any_linear_error(&errors),
            "storing a linear value in an array must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_value_in_array_repeat_is_rejected() {
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; \
             let arr = [q; 3]; let a = arr[0]; let b = arr[1]; \
             let x = consume(a); let y = consume(b); println(\"{{}}\", x + y); }}"
        ));
        assert!(
            has_any_linear_error(&errors),
            "array-repeat of a linear value (a literal clone) must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_value_through_generic_fn_is_rejected() {
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn thru<T>(x: T) -> T {{ x }}\n\
             fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; let c = thru(q); \
             let a = consume(c); let b = consume(c); println(\"{{}}\", a + b); }}"
        ));
        assert!(
            has_any_linear_error(&errors),
            "passing a linear value through a generic parameter must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_value_moved_out_of_reference_is_rejected() {
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; \
             let r = &q; let a = *r; let b = *r; \
             let x = consume(a); let y = consume(b); println(\"{{}}\", x + y); }}"
        ));
        assert!(
            has_any_linear_error(&errors),
            "moving a linear value out of a reference (deref-copy) must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_value_in_generic_struct_is_rejected() {
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}struct Holder<T> {{ item: T }}\n\
             fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; \
             let h = Holder {{ item: q }}; let a = h.item; let b = h.item; \
             let x = consume(a); let y = consume(b); println(\"{{}}\", x + y); }}"
        ));
        assert!(
            has_any_linear_error(&errors),
            "storing a linear value in a generic struct field must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_value_in_option_is_rejected() {
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; \
             let opt = Some(q); println(\"made an option\"); }}"
        ));
        assert!(
            has_any_linear_error(&errors),
            "wrapping a linear value in Option (a generic container) must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_consumed_then_revived_by_inner_shadow_errors() {
        // An inner-block binding that shadows the (already consumed) outer
        // linear must not revive the outer slot when the block exits.
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; \
             let a = consume(q); \
             {{ let q = Qubit {{ id: 2 }}; let _ = observe(&q); }} \
             let b = consume(q); println(\"{{}}\", a + b); }}"
        ));
        assert!(
            has_any_linear_error(&errors),
            "an inner shadow must not revive a consumed outer linear: {errors:#?}"
        );
    }

    #[test]
    fn linear_field_read_through_reference_is_rejected() {
        // Moving a linear field out from behind a borrow (`w.coin` where
        // `w: &Wrap`) does not consume `w`, so it could be repeated.
        let errors = check_source(
            "#[linear]\nstruct Coin { value: i64 }\n\
             #[linear]\nstruct Wrap { coin: Coin }\n\
             fn take(w: &Wrap) -> Coin { w.coin }\n\
             fn spend(c: Coin) -> i64 { c.value }\n\
             fn main() ~ Console { let coin = Coin { value: 1 }; \
             let w = Wrap { coin: coin }; \
             let a = spend(take(&w)); let b = spend(take(&w)); \
             println(\"{} {}\", a, b); }",
        );
        assert!(
            has_any_linear_error(&errors),
            "moving a linear field out through a reference must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_shorthand_field_init_consumes() {
        // `Wallet { coin }` shorthand moves `coin`; using it again is an error.
        let errors = check_source(
            "#[linear]\nstruct Coin { value: i64 }\n\
             #[linear]\nstruct Wallet { coin: Coin }\n\
             fn spend(c: Coin) -> i64 { c.value }\n\
             fn main() ~ Console { let coin = Coin { value: 1 }; \
             let w = Wallet { coin }; let a = spend(coin); println(\"{}\", a); }",
        );
        assert!(
            has_any_linear_error(&errors),
            "a shorthand field init must consume the moved linear local: {errors:#?}"
        );
    }

    #[test]
    fn linear_consumed_in_while_condition_errors() {
        // A while condition is re-evaluated each iteration; consuming an outer
        // linear there is a potential repeat-consume.
        let errors = check_source(
            "#[linear]\nstruct Coin { value: i64 }\n\
             fn drain(c: Coin) -> bool { c.value > 0 }\n\
             fn main() ~ Console { let coin = Coin { value: 1 }; \
             while drain(coin) { println(\"loop\"); } }",
        );
        assert!(
            has_any_linear_error(&errors),
            "consuming a linear value in a while condition must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_method_call_on_borrowed_receiver_is_rejected() {
        // `(&coin).burn()` where `burn(self)` would move the linear out of a
        // borrow; the self-kind isn't visible, so it is conservatively rejected.
        let errors = check_source(
            "#[linear]\nstruct Coin { value: i64 }\n\
             trait Burnable { fn burn(self) -> i64; }\n\
             impl Burnable for Coin { fn burn(self) -> i64 { self.value } }\n\
             fn spend(c: Coin) -> i64 { c.value }\n\
             fn main() ~ Console { let coin = Coin { value: 1 }; \
             let a = (&coin).burn(); let b = spend(coin); println(\"{} {}\", a, b); }",
        );
        assert!(
            has_any_linear_error(&errors),
            "a method call on a borrowed linear receiver must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_value_in_builtin_collection_is_rejected() {
        // Moving a linear value into a builtin Vec lets it be read out twice.
        let errors = check_source(
            "#[linear]\nstruct Coin { value: i64 }\n\
             fn spend(c: Coin) -> i64 { c.value }\n\
             fn main() ~ Console { let coin = Coin { value: 1 }; \
             let v = vec_new(); vec_push(v, coin); \
             let a = spend(vec_get(v, 0)); let b = spend(vec_get(v, 0)); \
             println(\"{} {}\", a, b); }",
        );
        assert!(
            has_any_linear_error(&errors),
            "storing a linear value in a builtin collection must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn linear_value_captured_in_closure_is_rejected() {
        let errors = check_source(&format!(
            "{LINEAR_PRELUDE}fn main() ~ Console {{ let q = Qubit {{ id: 1 }}; \
             let f = || consume(q); let a = f(); let b = f(); println(\"{{}}\", a + b); }}"
        ));
        assert!(
            has_any_linear_error(&errors),
            "capturing+consuming a linear value in a closure must be rejected: {errors:#?}"
        );
    }

    #[test]
    fn capability_ambient_file_call_requires_filesystem_effect() {
        let errors = check_source(r#"fn main() { read_file("ops.txt"); }"#);

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UnhandledEffect { effect_name, .. } if effect_name == "FileSystem"
            )),
            "expected FileSystem effect error, got {errors:#?}"
        );
        assert!(
            errors
                .iter()
                .any(|err| err.notes.iter().any(|note| note.contains("read_file"))),
            "expected diagnostic note naming read_file, got {errors:#?}"
        );
    }

    #[test]
    fn capability_declared_filesystem_effect_allows_file_call() {
        let errors = check_source(r#"fn main() ~ FileSystem { read_file("ops.txt"); }"#);

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    #[test]
    fn capability_c_runtime_filesystem_call_requires_filesystem_effect() {
        let errors = check_source(
            r#"
            extern "C" {
                fn build_file_exists(path: &str) -> bool;
            }

            fn main() {
                build_file_exists("ops.txt");
            }
            "#,
        );

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UnhandledEffect { effect_name, .. } if effect_name == "FileSystem"
            )),
            "expected FileSystem effect error, got {errors:#?}"
        );
        assert!(
            errors.iter().any(|err| err
                .notes
                .iter()
                .any(|note| note.contains("build_file_exists"))),
            "expected diagnostic note naming build_file_exists, got {errors:#?}"
        );
    }

    #[test]
    fn capability_declared_filesystem_effect_allows_c_runtime_file_call() {
        let errors = check_source(
            r#"
            extern "C" {
                fn build_file_exists(path: &str) -> bool;
            }

            fn main() ~ FileSystem {
                build_file_exists("ops.txt");
            }
            "#,
        );

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    #[test]
    fn capability_wrong_declared_effect_does_not_allow_file_call() {
        let errors = check_source(r#"fn main() ~ Network { read_file("ops.txt"); }"#);

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UndeclaredEffect { effect_name, .. } if effect_name == "FileSystem"
            )),
            "expected undeclared FileSystem error, got {errors:#?}"
        );
    }

    #[test]
    fn capability_console_macro_requires_console_effect() {
        let errors = check_source(r#"fn main() { println!("ops"); }"#);

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UnhandledEffect { effect_name, .. } if effect_name == "Console"
            )),
            "expected Console effect error, got {errors:#?}"
        );
        assert!(
            errors
                .iter()
                .any(|err| err.notes.iter().any(|note| note.contains("println!"))),
            "expected diagnostic note naming println!, got {errors:#?}"
        );
    }

    #[test]
    fn capability_declared_console_effect_allows_console_macro() {
        let errors = check_source(r#"fn main() ~ Console { println!("ops"); }"#);

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    #[test]
    fn capability_direct_console_call_requires_console_effect() {
        let errors = check_source(r#"fn main() { println("ops"); }"#);

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UnhandledEffect { effect_name, .. } if effect_name == "Console"
            )),
            "expected Console effect error, got {errors:#?}"
        );
        assert!(
            errors
                .iter()
                .any(|err| err.notes.iter().any(|note| note.contains("println"))),
            "expected diagnostic note naming println, got {errors:#?}"
        );
    }

    #[test]
    fn capability_declared_console_effect_allows_direct_console_call() {
        let errors = check_source(r#"fn main() ~ Console { println("ops"); }"#);

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    #[test]
    fn capability_wrong_declared_effect_does_not_allow_console_macro() {
        let errors = check_source(r#"fn main() ~ Network { println!("ops"); }"#);

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UndeclaredEffect { effect_name, .. } if effect_name == "Console"
            )),
            "expected undeclared Console error, got {errors:#?}"
        );
    }

    #[test]
    fn capability_gpu_runtime_call_requires_gpu_effect() {
        let errors = check_source(r#"fn main() { build_vk_init(); }"#);

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UnhandledEffect { effect_name, .. } if effect_name == "Gpu"
            )),
            "expected Gpu effect error, got {errors:#?}"
        );
        assert!(
            errors
                .iter()
                .any(|err| err.notes.iter().any(|note| note.contains("build_vk_init"))),
            "expected diagnostic note naming build_vk_init, got {errors:#?}"
        );
    }

    #[test]
    fn capability_declared_gpu_effect_allows_gpu_runtime_call() {
        let errors = check_source(r#"fn main() ~ Gpu { build_vk_init(); }"#);

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    #[test]
    fn capability_c_runtime_graphics_call_requires_gpu_effect() {
        let errors = check_source(
            r#"
            extern "C" {
                fn build_gfx_init(width: i32, height: i32, title: &str) -> i32;
            }

            fn main() {
                build_gfx_init(800, 600, "BuildLang Triangle");
            }
            "#,
        );

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UnhandledEffect { effect_name, .. } if effect_name == "Gpu"
            )),
            "expected Gpu effect error, got {errors:#?}"
        );
        assert!(
            errors
                .iter()
                .any(|err| err.notes.iter().any(|note| note.contains("build_gfx_init"))),
            "expected diagnostic note naming build_gfx_init, got {errors:#?}"
        );
    }

    #[test]
    fn capability_declared_gpu_effect_allows_c_runtime_graphics_call() {
        let errors = check_source(
            r#"
            extern "C" {
                fn build_gfx_init(width: i32, height: i32, title: &str) -> i32;
            }

            fn main() ~ Gpu {
                build_gfx_init(800, 600, "BuildLang Triangle");
            }
            "#,
        );

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    #[test]
    fn capability_wrong_declared_effect_does_not_allow_gpu_runtime_call() {
        let errors = check_source(r#"fn main() ~ FileSystem { build_vk_init(); }"#);

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UndeclaredEffect { effect_name, .. } if effect_name == "Gpu"
            )),
            "expected undeclared Gpu error, got {errors:#?}"
        );
    }

    #[test]
    fn capability_foreign_call_requires_foreign_effect() {
        let errors = check_source(
            r#"
            extern "C" { fn touch(); }
            fn main() { touch(); }
            "#,
        );

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UnhandledEffect { effect_name, .. } if effect_name == "Foreign"
            )),
            "expected Foreign effect error, got {errors:#?}"
        );
        assert!(
            errors
                .iter()
                .any(|err| err.notes.iter().any(|note| note.contains("touch"))),
            "expected diagnostic note naming touch, got {errors:#?}"
        );
    }

    #[test]
    fn capability_foreign_static_requires_foreign_effect() {
        let errors = check_source(
            r#"
            extern "C" { static BUILD_ERRNO: i32; }
            fn main() { let code = BUILD_ERRNO; }
            "#,
        );

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UnhandledEffect { effect_name, .. } if effect_name == "Foreign"
            )),
            "expected Foreign effect error, got {errors:#?}"
        );
        assert!(
            errors
                .iter()
                .any(|err| err.notes.iter().any(|note| note.contains("BUILD_ERRNO"))),
            "expected diagnostic note naming BUILD_ERRNO, got {errors:#?}"
        );
    }

    #[test]
    fn capability_declared_foreign_effect_allows_foreign_static() {
        let errors = check_source(
            r#"
            extern "C" { static BUILD_ERRNO: i32; }
            fn main() ~ Foreign { let code = BUILD_ERRNO; }
            "#,
        );

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    #[test]
    fn check_summary_records_foreign_calls_as_direct_capabilities() {
        let source = r#"
            extern "C" { fn touch(); }
            fn main() ~ Foreign { touch(); }
        "#;
        let source_file = crate::lexer::SourceFile::new("foreign_summary_test.bld", source);
        let mut lexer = crate::lexer::Lexer::new(&source_file);
        let tokens = lexer.tokenize().expect("tokenize foreign summary fixture");
        let mut parser = crate::parser::Parser::new(&source_file, tokens);
        let module = parser.parse().expect("parse foreign summary fixture");

        let mut ctx = TypeContext::new();
        let mut checker = TypeChecker::new(&mut ctx);
        checker.check_module(&module);

        let summaries = checker.function_effect_summaries();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].function, "main");
        assert_eq!(summaries[0].declared_effects, vec!["Foreign"]);
        assert_eq!(
            summaries[0]
                .observed_capabilities
                .get("Foreign")
                .expect("Foreign capability should be observed")
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["touch"]
        );
        assert!(
            summaries[0].propagated_effects.is_empty(),
            "direct extern call should not be recorded as propagated: {summaries:#?}"
        );
    }

    #[test]
    fn check_summary_records_declared_effects_and_capability_sources() {
        let source = r#"fn main() ~ Console { println!("ops"); }"#;
        let source_file = crate::lexer::SourceFile::new("summary_test.bld", source);
        let mut lexer = crate::lexer::Lexer::new(&source_file);
        let tokens = lexer.tokenize().expect("tokenize summary fixture");
        let mut parser = crate::parser::Parser::new(&source_file, tokens);
        let module = parser.parse().expect("parse summary fixture");

        let mut ctx = TypeContext::new();
        let mut checker = TypeChecker::new(&mut ctx);
        checker.check_module(&module);

        let summaries = checker.function_effect_summaries();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].function, "main");
        assert_eq!(summaries[0].declared_effects, vec!["Console"]);
        assert_eq!(
            summaries[0]
                .observed_capabilities
                .get("Console")
                .expect("Console capability should be observed")
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["println!"]
        );
    }

    #[test]
    fn check_summary_separates_direct_and_propagated_capabilities() {
        let source = r#"
            fn load_config() ~ FileSystem {
                read_file("ops.txt");
            }

            fn main() ~ FileSystem {
                load_config();
            }
        "#;
        let source_file = crate::lexer::SourceFile::new("summary_test.bld", source);
        let mut lexer = crate::lexer::Lexer::new(&source_file);
        let tokens = lexer.tokenize().expect("tokenize summary fixture");
        let mut parser = crate::parser::Parser::new(&source_file, tokens);
        let module = parser.parse().expect("parse summary fixture");

        let mut ctx = TypeContext::new();
        let mut checker = TypeChecker::new(&mut ctx);
        checker.check_module(&module);
        assert!(
            checker.errors().is_empty(),
            "expected clean type check, got {:#?}",
            checker.errors()
        );

        let summaries = checker.function_effect_summaries();
        let load_config = summaries
            .iter()
            .find(|summary| summary.function == "load_config")
            .expect("load_config summary");
        assert_eq!(
            load_config
                .observed_capabilities
                .get("FileSystem")
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["read_file".to_string()]
        );
        assert!(
            load_config.propagated_effects.is_empty(),
            "direct boundary should not report propagated callees"
        );

        let main = summaries
            .iter()
            .find(|summary| summary.function == "main")
            .expect("main summary");
        assert!(
            main.observed_capabilities.is_empty(),
            "caller should not report callee helper as direct IO"
        );
        assert_eq!(
            main.propagated_effects
                .get("FileSystem")
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["load_config".to_string()]
        );
    }

    #[test]
    fn check_summary_records_console_macro_as_direct_not_propagated() {
        let source = r#"fn main() ~ Console { println!("ops"); }"#;
        let source_file = crate::lexer::SourceFile::new("summary_test.bld", source);
        let mut lexer = crate::lexer::Lexer::new(&source_file);
        let tokens = lexer.tokenize().expect("tokenize summary fixture");
        let mut parser = crate::parser::Parser::new(&source_file, tokens);
        let module = parser.parse().expect("parse summary fixture");

        let mut ctx = TypeContext::new();
        let mut checker = TypeChecker::new(&mut ctx);
        checker.check_module(&module);
        assert!(
            checker.errors().is_empty(),
            "expected clean type check, got {:#?}",
            checker.errors()
        );

        let main = checker
            .function_effect_summaries()
            .iter()
            .find(|summary| summary.function == "main")
            .expect("main summary");
        assert_eq!(
            main.observed_capabilities
                .get("Console")
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["println!".to_string()]
        );
        assert!(
            main.propagated_effects.is_empty(),
            "macro capability should remain direct provenance"
        );
    }

    #[test]
    fn check_summaries_reset_between_modules() {
        let first = r#"fn main() ~ Console { println!("ops"); }"#;
        let second = r#"fn helper() {}"#;

        let parse_module = |name: &str, source: &str| {
            let source_file = crate::lexer::SourceFile::new(name, source);
            let mut lexer = crate::lexer::Lexer::new(&source_file);
            let tokens = lexer.tokenize().expect("tokenize summary fixture");
            let mut parser = crate::parser::Parser::new(&source_file, tokens);
            parser.parse().expect("parse summary fixture")
        };

        let mut ctx = TypeContext::new();
        let mut checker = TypeChecker::new(&mut ctx);
        checker.check_module(&parse_module("first.bld", first));
        assert_eq!(checker.function_effect_summaries().len(), 1);

        checker.check_module(&parse_module("second.bld", second));
        let summaries = checker.function_effect_summaries();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].function, "helper");
        assert!(summaries[0].declared_effects.is_empty());
        assert!(summaries[0].observed_capabilities.is_empty());
        assert!(summaries[0].propagated_effects.is_empty());
    }

    #[test]
    fn test_type_checker_creation() {
        let mut ctx = TypeContext::new();
        let checker = TypeChecker::new(&mut ctx);
        assert!(!checker.has_errors());
    }

    // =========================================================================
    // SELF-HOSTING BRICK 1: trait default methods, reference receivers, and
    // closures as method parameters. These patterns are required by the
    // self-hosted `core::cmp` (PartialEq::ne default) and `core::option`
    // (closure-taking combinators) modules.
    // =========================================================================

    /// A trait default method (`ne` calling `eq`) must resolve at a call site
    /// even when the `impl` only provides the required method. This is the
    /// exact shape of `PartialEq::ne` in `stdlib/core/cmp.bld`.
    #[test]
    fn trait_default_method_resolves_with_reference_receiver() {
        let errors = check_source(
            r#"
            trait MyEq {
                fn eq(&self, other: &Self) -> bool;
                fn ne(&self, other: &Self) -> bool {
                    !self.eq(other)
                }
            }

            struct P { x: i32 }

            impl MyEq for P {
                fn eq(&self, other: &P) -> bool {
                    self.x == other.x
                }
            }

            fn check() -> bool {
                let a = P { x: 1 };
                let b = P { x: 2 };
                a.ne(&b)
            }
            "#,
        );

        assert!(
            !errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UndefinedMethod { method, .. } if method == "ne"
            )),
            "default trait method `ne` should resolve, got {errors:#?}"
        );
        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    /// A by-value default method (`greet` calling `name`) must also resolve.
    #[test]
    fn trait_default_method_resolves_by_value() {
        let errors = check_source(
            r#"
            trait Greet {
                fn name(self) -> i32;
                fn greet(self) -> i32 {
                    self.name() + 1
                }
            }

            struct P { x: i32 }

            impl Greet for P {
                fn name(self) -> i32 { self.x }
            }

            fn check() -> i32 {
                let a = P { x: 1 };
                a.greet()
            }
            "#,
        );

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    /// A default method that does not call back into the trait should still
    /// resolve when the impl omits it.
    #[test]
    fn trait_default_method_standalone_resolves() {
        let errors = check_source(
            r#"
            trait Greet {
                fn base(&self) -> i32;
                fn doubled(&self) -> i32 {
                    42
                }
            }

            struct P { x: i32 }

            impl Greet for P {
                fn base(&self) -> i32 { self.x }
            }

            fn check() -> i32 {
                let a = P { x: 1 };
                a.doubled()
            }
            "#,
        );

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    /// An impl that overrides a default method must still resolve to the
    /// overriding signature (no regression from the default-method change).
    #[test]
    fn trait_default_method_override_resolves() {
        let errors = check_source(
            r#"
            trait Greet {
                fn base(&self) -> i32;
                fn doubled(&self) -> i32 { 0 }
            }

            struct P { x: i32 }

            impl Greet for P {
                fn base(&self) -> i32 { self.x }
                fn doubled(&self) -> i32 { self.x * 2 }
            }

            fn check() -> i32 {
                let a = P { x: 5 };
                a.doubled()
            }
            "#,
        );

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    /// Generic `&self` reference receivers in a generic impl, with one method
    /// calling another, mirror `Option::is_none` calling `Option::is_some`.
    #[test]
    fn generic_reference_receiver_methods_resolve() {
        let errors = check_source(
            r#"
            enum Opt<T> {
                None,
                Some(T),
            }

            impl<T> Opt<T> {
                fn is_some(&self) -> bool {
                    match self {
                        Opt::Some(_) => true,
                        Opt::None => false,
                    }
                }
                fn is_none(&self) -> bool {
                    !self.is_some()
                }
            }

            fn check() -> bool {
                let s: Opt<i32> = Opt::None;
                s.is_none()
            }
            "#,
        );

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    /// End-to-end shape of the self-hosted `core::cmp` traits: `PartialEq`
    /// with a defaulted `ne`, an `Ord`-style supertrait, reference receivers,
    /// and a default method that chains through the required method.
    #[test]
    fn cmp_style_trait_hierarchy_checks() {
        let errors = check_source(
            r#"
            trait PartialEq {
                fn eq(&self, other: &Self) -> bool;
                fn ne(&self, other: &Self) -> bool {
                    !self.eq(other)
                }
            }

            trait Ord: PartialEq {
                fn cmp(&self, other: &Self) -> i32;
                fn max_by(self, other: Self) -> Self;
            }

            struct N { v: i32 }

            impl PartialEq for N {
                fn eq(&self, other: &N) -> bool {
                    self.v == other.v
                }
            }

            impl Ord for N {
                fn cmp(&self, other: &N) -> i32 {
                    if self.v < other.v {
                        -1
                    } else if self.ne(other) {
                        1
                    } else {
                        0
                    }
                }
                fn max_by(self, other: N) -> N {
                    if self.cmp(&other) < 0 { other } else { self }
                }
            }

            fn check() -> i32 {
                let a = N { v: 1 };
                let b = N { v: 2 };
                let m = a.max_by(b);
                m.v
            }
            "#,
        );

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    /// Soundness boundary: a trait default method must NOT resolve on a type
    /// that does not implement the trait. The default-method fix only applies
    /// when a matching impl exists.
    #[test]
    fn trait_default_method_does_not_resolve_without_impl() {
        let errors = check_source(
            r#"
            trait MyEq {
                fn eq(&self, other: &Self) -> bool;
                fn ne(&self, other: &Self) -> bool {
                    !self.eq(other)
                }
            }

            struct P { x: i32 }
            struct Q { y: i32 }

            impl MyEq for P {
                fn eq(&self, other: &P) -> bool { self.x == other.x }
            }

            fn check() -> bool {
                let a = Q { y: 1 };
                let b = Q { y: 2 };
                a.ne(&b)
            }
            "#,
        );

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UndefinedMethod { method, .. } if method == "ne"
            )),
            "default method `ne` must not resolve on a non-implementing type, got {errors:#?}"
        );
    }

    /// Closures passed as method parameters (both `impl Trait` and an explicit
    /// `F: FnOnce` bound), chained, mirror `Option::map` / `Option::and_then`.
    #[test]
    fn closures_as_method_parameters_resolve() {
        let errors = check_source(
            r#"
            enum Opt<T> {
                None,
                Some(T),
            }

            impl<T> Opt<T> {
                fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Opt<U> {
                    match self {
                        Opt::Some(x) => Opt::Some(f(x)),
                        Opt::None => Opt::None,
                    }
                }
                fn is_some_and(self, f: impl FnOnce(T) -> bool) -> bool {
                    match self {
                        Opt::Some(x) => f(x),
                        Opt::None => false,
                    }
                }
                fn unwrap_or(self, fallback: T) -> T {
                    match self {
                        Opt::Some(x) => x,
                        Opt::None => fallback,
                    }
                }
            }

            fn check() -> i32 {
                let s = Opt::Some(10);
                s.map(|x| x + 1).unwrap_or(0)
            }
            "#,
        );

        assert!(errors.is_empty(), "expected no errors, got {errors:#?}");
    }

    // ── Trait-method resolution on a generic type parameter via its bound ──
    //
    // These tests cover self-hosting brick 2: a call `x.method()` where `x: T`
    // and `T` carries a trait bound providing `method` (inline, where-clause, or
    // from an enclosing `impl<T: Trait>`) must resolve through that bound.
    //
    // Known remaining gaps (intentionally NOT addressed by this brick):
    //   1. Generic *inherent*-impl method bodies are deferred and never
    //      type-checked (see `check_inherent_impl`, the `impl_is_generic`
    //      skip). Method calls on `T` there are not checked at all, so they
    //      neither resolve nor error until monomorphization.
    //   2. Method resolution on an array slice `self[..]` typed as the concrete
    //      slice `[T]` inside `impl Ord for [T; N]` (stdlib `core::cmp`,
    //      `self[..].cmp(&other[..])`) still fails with `[T] has no method cmp`.
    //      That is concrete-slice-type resolution, distinct from the
    //      type-parameter-bound resolution fixed here.

    #[test]
    fn impl_method_resolves_trait_method_on_bounded_type_param() {
        // Inside `impl<T: Foo> Foo for Wrap<T>`, a call `inner.bar()` where
        // `inner: T` must resolve `bar` through the impl-level bound `T: Foo`.
        let errors = check_source(
            r#"
            trait Foo {
                fn bar(&self) -> i32;
            }

            struct Wrap<T>(T);

            impl<T: Foo> Foo for Wrap<T> {
                fn bar(&self) -> i32 {
                    let inner: T = make();
                    inner.bar()
                }
            }

            fn make<T>() -> T {
                make()
            }
            "#,
        );

        assert!(
            !errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UndefinedMethod { method, .. } if method == "bar"
            )),
            "expected `bar` to resolve through impl bound `T: Foo`, got {errors:#?}"
        );
    }

    #[test]
    fn impl_method_resolves_trait_method_on_slice_element() {
        // Inside `impl<T: Foo> Foo for [T]`, indexing yields a `T`; calling a
        // bound trait method on it must resolve through `T: Foo`.
        let errors = check_source(
            r#"
            trait Foo {
                fn bar(&self) -> i32;
            }

            impl<T: Foo> Foo for [T] {
                fn bar(&self) -> i32 {
                    self[0].bar()
                }
            }
            "#,
        );

        assert!(
            !errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UndefinedMethod { method, .. } if method == "bar"
            )),
            "expected `bar` to resolve on `[T]` element through bound `T: Foo`, got {errors:#?}"
        );
    }

    #[test]
    fn impl_method_resolves_trait_method_via_where_clause_bound() {
        // The bound is supplied through a where-clause instead of inline.
        let errors = check_source(
            r#"
            trait Foo {
                fn bar(&self) -> i32;
            }

            struct Wrap<T>(T);

            impl<T> Foo for Wrap<T>
            where
                T: Foo,
            {
                fn bar(&self) -> i32 {
                    let inner: T = make();
                    inner.bar()
                }
            }

            fn make<T>() -> T {
                make()
            }
            "#,
        );

        assert!(
            !errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UndefinedMethod { method, .. } if method == "bar"
            )),
            "expected `bar` to resolve through where-clause bound `T: Foo`, got {errors:#?}"
        );
    }

    #[test]
    fn unbounded_type_param_method_call_still_errors() {
        // Negative case in a CHECKED context: a trait-impl method body (which
        // is type-checked, unlike deferred generic-function bodies) calls a
        // method that none of `T`'s bounds provide. `T: Other` exists but does
        // not supply `missing`, so resolution must still reject the call.
        let errors = check_source(
            r#"
            trait Foo {
                fn bar(&self) -> i32;
            }

            trait Other {
                fn other(&self) -> i32;
            }

            struct Wrap<T>(T);

            impl<T: Other> Foo for Wrap<T> {
                fn bar(&self) -> i32 {
                    let inner: T = make();
                    inner.missing()
                }
            }

            fn make<T>() -> T {
                make()
            }
            "#,
        );

        assert!(
            errors.iter().any(|err| matches!(
                &err.error,
                TypeError::UndefinedMethod { method, .. } if method == "missing"
            )),
            "expected `missing` to be unresolved when no bound provides it, got {errors:#?}"
        );
    }

    // =========================================================================
    // UNIT-ANNOTATED NUMERIC TYPES (`f64<m/s>`, experimental) -- W6 checker
    // slice one. All dimension math below is the shipped `units` algebra
    // (units.rs, unmodified); these tests only prove it is correctly wired
    // into the checker's unifier and `infer_binary`.
    // =========================================================================

    mod unit_annotated_types {
        use super::*;

        fn unit_errors(errors: &[TypeErrorWithSpan]) -> Vec<&TypeError> {
            errors
                .iter()
                .map(|e| &e.error)
                .filter(|e| {
                    matches!(
                        e,
                        TypeError::UnitMismatch { .. } | TypeError::UnitOperationMismatch { .. }
                    )
                })
                .collect()
        }

        #[test]
        fn mismatched_add_refused_with_operation_wording() {
            let errors = check_source(
                "fn main() { let d: f64<m> = 3.0; let t: f64<s> = 1.0; let x = d + t; }",
            );
            assert_eq!(errors.len(), 1, "expected exactly one error: {errors:#?}");
            match &errors[0].error {
                TypeError::UnitOperationMismatch {
                    operation,
                    left,
                    right,
                } => {
                    assert_eq!(*operation, "add");
                    assert_eq!(left, "m");
                    assert_eq!(right, "s");
                }
                other => panic!("expected UnitOperationMismatch, got {other:?}"),
            }
            assert_eq!(
                errors[0].error.to_string(),
                "unit mismatch: cannot add `m` and `s` (dimensions differ)"
            );
        }

        #[test]
        fn mismatched_subtract_refused_with_operation_wording() {
            let errors = check_source(
                "fn main() { let d: f64<m> = 3.0; let t: f64<s> = 1.0; let x = d - t; }",
            );
            let unit_errs = unit_errors(&errors);
            assert_eq!(unit_errs.len(), 1, "{errors:#?}");
            match unit_errs[0] {
                TypeError::UnitOperationMismatch { operation, .. } => {
                    assert_eq!(*operation, "subtract")
                }
                other => panic!("expected UnitOperationMismatch, got {other:?}"),
            }
        }

        #[test]
        fn mismatched_compare_refused_with_operation_wording() {
            let errors = check_source(
                "fn main() { let d: f64<m> = 3.0; let t: f64<s> = 1.0; let x = d < t; }",
            );
            let unit_errs = unit_errors(&errors);
            assert_eq!(unit_errs.len(), 1, "{errors:#?}");
            match unit_errs[0] {
                TypeError::UnitOperationMismatch { operation, .. } => {
                    assert_eq!(*operation, "compare")
                }
                other => panic!("expected UnitOperationMismatch, got {other:?}"),
            }
        }

        #[test]
        fn mismatched_assign_refused() {
            let errors =
                check_source("fn main() { let mut d: f64<m> = 3.0; let t: f64<s> = 1.0; d = t; }");
            let unit_errs = unit_errors(&errors);
            assert_eq!(unit_errs.len(), 1, "{errors:#?}");
            match unit_errs[0] {
                TypeError::UnitMismatch { expected, found } => {
                    assert_eq!(expected, "m");
                    assert_eq!(found, "s");
                }
                other => panic!("expected UnitMismatch, got {other:?}"),
            }
        }

        #[test]
        fn mismatched_let_annotation_refused() {
            let errors = check_source(
                "fn main() { let d: f64<m> = 3.0; let t: f64<s> = 1.0; \
                 let bad: f64<m> = d / t; }",
            );
            let unit_errs = unit_errors(&errors);
            assert_eq!(unit_errs.len(), 1, "{errors:#?}");
            match unit_errs[0] {
                TypeError::UnitMismatch { expected, found } => {
                    assert_eq!(expected, "m");
                    assert_eq!(found, "m/s");
                }
                other => panic!("expected UnitMismatch, got {other:?}"),
            }
        }

        #[test]
        fn mismatched_argument_refused() {
            let errors = check_source(
                "fn go(v: f64<m/s>) { } \
                 fn main() { let d: f64<m> = 3.0; go(d); }",
            );
            let unit_errs = unit_errors(&errors);
            assert_eq!(
                unit_errs.len(),
                1,
                "the unify backstop must catch a unit mismatch through a call \
                 argument: {errors:#?}"
            );
            assert!(matches!(unit_errs[0], TypeError::UnitMismatch { .. }));
        }

        #[test]
        fn derived_division_dimension_accepted() {
            let errors = check_source(
                "fn main() { let d: f64<m> = 3.0; let t: f64<s> = 1.0; \
                 let v: f64<m/s> = d / t; }",
            );
            assert!(unit_errors(&errors).is_empty(), "{errors:#?}");
        }

        #[test]
        fn unannotated_product_stays_unconstrained_weak_mode() {
            // `None * None` must stay `None` (never invent a dimensionless
            // unit where neither operand wrote one): the product must still
            // mix freely with an explicitly dimensioned quantity, exactly
            // like the unannotated-mixing rule for Add.
            let errors = check_source(
                "fn main() { \
                    let a: f64 = 1.0; let b: f64 = 2.0; let p = a * b; \
                    let d: f64<m> = 3.0; let s = p + d; \
                 }",
            );
            assert!(unit_errors(&errors).is_empty(), "{errors:#?}");
        }

        #[test]
        fn derived_multiplication_named_unit_equivalence_accepted() {
            // kg*m/s^2 (force) times m (length) is J (energy): proves
            // `multiply` and named-derived-unit equivalence end to end.
            let errors = check_source(
                "fn main() { let f: f64<kg*m/s^2> = 1.0; let len: f64<m> = 2.0; \
                 let e: f64<J> = f * len; }",
            );
            assert!(unit_errors(&errors).is_empty(), "{errors:#?}");
        }

        #[test]
        fn same_dimension_add_rem_compare_accepted() {
            let errors = check_source(
                "fn main() { \
                    let d: f64<m> = 3.0; \
                    let sum: f64<m> = d + d; \
                    let r: f64<m> = d % d; \
                    let b: bool = d < d; \
                 }",
            );
            assert!(unit_errors(&errors).is_empty(), "{errors:#?}");
        }

        #[test]
        fn scalar_times_unit_keeps_the_unit() {
            let errors =
                check_source("fn main() { let d: f64<m> = 3.0; let p: f64<m> = 2.0 * d; }");
            assert!(unit_errors(&errors).is_empty(), "{errors:#?}");
        }

        #[test]
        fn unannotated_mixing_is_allowed_weak_mode() {
            // An unannotated float is UNCONSTRAINED (compatible with any
            // unit); full dimension variables are a deferred follow-up, not
            // this slice.
            let errors =
                check_source("fn main() { let d: f64<m> = 3.0; let p: f64 = 1.0; let s = d + p; }");
            assert!(unit_errors(&errors).is_empty(), "{errors:#?}");
        }

        #[test]
        fn pow_on_unit_carrying_operand_is_loudly_unsupported() {
            let errors = check_source("fn main() { let d: f64<m> = 3.0; let bad = d ** 2.0; }");
            assert!(
                errors.iter().any(|e| matches!(
                    &e.error,
                    TypeError::UnsupportedConstruct { construct, .. }
                        if construct.contains("unit-carrying")
                )),
                "expected UnsupportedConstruct for `**` on a unit-carrying \
                 operand: {errors:#?}"
            );
        }

        #[test]
        fn unary_minus_preserves_the_unit() {
            let errors = check_source("fn main() { let d: f64<m> = 3.0; let neg: f64<m> = -d; }");
            assert!(unit_errors(&errors).is_empty(), "{errors:#?}");
        }

        #[test]
        fn cast_to_unit_annotated_float_reannotates() {
            // `infer_cast` resolves its target via `lower_type` (the exact
            // path `WithUnit` lowering hooks into), so `x as f64<m>` is a
            // deliberate re-annotation escape hatch: the cast's result
            // carries the annotated dimension from that point on. If casts
            // were instead unclaimed (target ignored), `d` would stay an
            // unconstrained plain float and this would produce no error.
            let errors = check_source(
                "fn main() { let p: f64 = 1.0; let d = p as f64<m>; let bad: f64<s> = d; }",
            );
            assert!(
                errors
                    .iter()
                    .any(|e| matches!(&e.error, TypeError::UnitMismatch { .. })),
                "the cast must have attached `m`, so binding it to `f64<s>` \
                 should mismatch: {errors:#?}"
            );
        }

        #[test]
        fn negative_exponent_form_normalizes_and_unifies() {
            // `m*s^-1` and `m/s` are the same dimension; an argument-passing
            // boundary must accept either spelling for the other.
            let errors = check_source(
                "fn go(v: f64<m/s>) { } \
                 fn main() { let d: f64<m*s^-1> = 3.0; go(d); }",
            );
            assert!(unit_errors(&errors).is_empty(), "{errors:#?}");
        }

        // =====================================================================
        // TASK 5: Ty-EQUALITY AUDIT (the diffuse risk)
        //
        // `unit_dim` now participates in `Ty`'s derived `Eq`/`Hash`, so any
        // Ty-keyed lookup or equality-driven classification could newly
        // distinguish `f64<m>` from `f64`. The audit (see the commit body)
        // found ONE real hazard: `infer.rs`'s `dispatch_position_match`
        // (multiple-dispatch overload scoring) compared full `Ty` equality
        // for its Exact-match check, but codegen's mangled overload names
        // come from `MirType`, which has no unit slot. Two candidates
        // differing ONLY by unit annotation would therefore mangle to the
        // SAME C name -- a probed program confirmed the checker uniquely
        // resolving such a call while codegen emitted two colliding
        // `go_f64` definitions. Mitigated by stripping `unit_dim` at that
        // one comparison boundary, keeping the checker and codegen decision
        // procedures in agreement (see `dispatch_position_match`'s doc
        // comment). Every other Ty-keyed site audited (dispatch.rs itself
        // is MirType/generic, not Ty-keyed; traits.rs and context.rs's
        // `HashMap<Arc<str>, Ty>` tables are string-keyed, `Ty` only ever
        // the value) was safe as-is: no code change.
        // =====================================================================

        #[test]
        fn overload_set_differing_only_by_unit_is_ambiguous_not_silently_accepted() {
            let errors = check_source(
                "fn go(v: f64<m>) -> f64 { v } \
                 fn go(v: f64<s>) -> f64 { v + 1.0 } \
                 fn main() { let d: f64<m> = 3.0; let r = go(d); }",
            );
            assert!(
                errors
                    .iter()
                    .any(|e| matches!(&e.error, TypeError::AmbiguousMethod { .. })),
                "expected AmbiguousMethod (matching codegen's unit-blindness at the \
                 mangled name), not a silent unique resolution that a real C \
                 compiler would then reject as a duplicate definition: {errors:#?}"
            );
        }

        #[test]
        fn method_call_on_annotated_float_receiver_resolves() {
            // `.abs()` is dimension-preserving (identity on magnitude): a unit
            // riding on the receiver must not break method resolution, and
            // the result keeps the receiver's dimension (inherent float
            // methods return `receiver_ty` unchanged; see infer.rs's FLOAT
            // METHODS arm).
            let errors =
                check_source("fn main() { let d: f64<m> = -3.0; let a: f64<m> = d.abs(); }");
            assert!(unit_errors(&errors).is_empty(), "{errors:#?}");
        }

        #[test]
        fn annotated_float_survives_a_generic_function() {
            let errors = check_source(
                "fn identity<T>(x: T) -> T { x } \
                 fn main() { let d: f64<m> = 3.0; let r: f64<m> = identity(d); }",
            );
            assert!(unit_errors(&errors).is_empty(), "{errors:#?}");
        }

        // No negative (mismatch-catching) counterpart: the checker does not
        // thread a generic function's return type back to enforce anything
        // against the caller for ANY type, unit-carrying or not (confirmed:
        // the identical gap reproduces for a plain, unit-free
        // `let bad: i32 = identity(3.0);`, zero diagnostics). Pre-existing,
        // orthogonal to units, and out of scope for this slice ("if the
        // checker supports it" per the plan's Task 5 bullet -- it does not,
        // for generics in general). The positive test above is the honest
        // claim: units are not lost passing through, nothing stronger.

        // =====================================================================
        // Review follow-up: `.sqrt()`/`.cbrt()`/`.powi()`/`.powf()` on a
        // unit-carrying receiver used to silently return the receiver's
        // unchanged (and therefore wrong) dimension. Now a loud
        // `UnsupportedConstruct` refusal, matching `**`'s principle.
        // =====================================================================

        #[test]
        fn sqrt_on_unit_carrying_receiver_is_loudly_unsupported() {
            let errors =
                check_source("fn main() { let area: f64<m*m> = 9.0; let side = area.sqrt(); }");
            assert!(
                errors.iter().any(|e| matches!(
                    &e.error,
                    TypeError::UnsupportedConstruct { construct, .. }
                        if construct.contains("unit-carrying") && construct.contains("sqrt")
                )),
                "expected UnsupportedConstruct for `.sqrt()` on a unit-carrying \
                 receiver: {errors:#?}"
            );
        }

        #[test]
        fn cbrt_on_unit_carrying_receiver_is_loudly_unsupported() {
            let errors =
                check_source("fn main() { let vol: f64<m*m*m> = 27.0; let side = vol.cbrt(); }");
            assert!(
                errors.iter().any(|e| matches!(
                    &e.error,
                    TypeError::UnsupportedConstruct { construct, .. }
                        if construct.contains("unit-carrying") && construct.contains("cbrt")
                )),
                "expected UnsupportedConstruct for `.cbrt()` on a unit-carrying \
                 receiver: {errors:#?}"
            );
        }

        #[test]
        fn powi_and_powf_on_unit_carrying_receiver_are_loudly_unsupported() {
            let errors = check_source(
                "fn main() { \
                    let d: f64<m> = 3.0; \
                    let a = d.powi(2); \
                    let b = d.powf(2.0); \
                 }",
            );
            let refusals: Vec<_> = errors
                .iter()
                .filter(|e| matches!(&e.error, TypeError::UnsupportedConstruct { construct, .. } if construct.contains("unit-carrying")))
                .collect();
            assert_eq!(
                refusals.len(),
                2,
                "expected UnsupportedConstruct for both `.powi()` and `.powf()` \
                 on a unit-carrying receiver: {errors:#?}"
            );
        }

        #[test]
        fn sqrt_on_unannotated_receiver_is_unaffected() {
            // `unit_dim: None` must keep matching the identity-methods arm:
            // the refusal is guarded on `unit_dim.is_some()` and must not
            // fire for a plain, unit-free float.
            let errors = check_source("fn main() { let x: f64 = 9.0; let y: f64 = x.sqrt(); }");
            assert!(unit_errors(&errors).is_empty(), "{errors:#?}");
            assert!(
                !errors
                    .iter()
                    .any(|e| matches!(&e.error, TypeError::UnsupportedConstruct { .. })),
                "unannotated `.sqrt()` must not be refused: {errors:#?}"
            );
        }

        #[test]
        fn identity_shaped_methods_still_preserve_the_unit() {
            // `.abs()`, `.floor()`, `.ceil()`, `.round()`, `.trunc()`,
            // `.fract()` genuinely preserve dimension (the numeric value
            // changes, the unit it is expressed in does not) and must stay
            // untouched by the sqrt/cbrt/powi/powf refusal.
            let errors = check_source(
                "fn main() { \
                    let d: f64<m> = -3.7; \
                    let a: f64<m> = d.abs(); \
                    let b: f64<m> = d.floor(); \
                    let c: f64<m> = d.ceil(); \
                    let e: f64<m> = d.round(); \
                    let f: f64<m> = d.trunc(); \
                    let g: f64<m> = d.fract(); \
                 }",
            );
            assert!(unit_errors(&errors).is_empty(), "{errors:#?}");
            assert!(
                !errors
                    .iter()
                    .any(|e| matches!(&e.error, TypeError::UnsupportedConstruct { .. })),
                "identity-shaped methods must not be refused: {errors:#?}"
            );
        }
    }
}
