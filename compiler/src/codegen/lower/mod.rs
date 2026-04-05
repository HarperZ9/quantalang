// ===============================================================================
// QUANTALANG CODE GENERATOR - AST TO MIR LOWERING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Lowering from AST to MIR.
//!
//! This pass transforms the type-checked AST into MIR (Mid-level IR).
//!
//! The lowering is split across several submodules:
//! - `expr`: Block, statement, and expression lowering
//! - `types`: Type lowering, const evaluation, and generic monomorphization
//! - `macros`: Closure, effect, builtin macro, and iterator chain lowering

mod expr;
mod macros;
mod types;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::ast::{self, ItemKind};
use crate::types::TypeContext;

use super::backend::CodegenResult;
use super::builder::{values, MirBuilder, MirModuleBuilder};
use super::ir::*;

/// AST to MIR lowerer.
pub struct MirLowerer<'ctx> {
    /// Type context.
    ctx: &'ctx TypeContext,
    /// Module builder.
    module: MirModuleBuilder,
    /// Variable to local mapping (per function).
    var_map: HashMap<Arc<str>, LocalId>,
    /// Loop context stack (continue_block, break_block).
    loop_stack: Vec<(BlockId, BlockId)>,
    /// Current function builder.
    current_fn: Option<MirBuilder>,
    /// Source code (for extracting token text in macro expansion).
    source: Option<Arc<str>>,
    /// Counter for generating unique closure function names.
    closure_count: u32,
    /// Impl method registry: maps (TypeName, MethodName) -> mangled function name.
    impl_methods: HashMap<(Arc<str>, Arc<str>), Arc<str>>,
    /// Generic function ASTs stored for monomorphization: name -> FnDef clone.
    generic_functions: HashMap<Arc<str>, ast::FnDef>,
    /// Generic enum ASTs stored for monomorphization: name -> EnumDef clone.
    generic_enums: HashMap<Arc<str>, ast::EnumDef>,
    /// Generic struct ASTs stored for monomorphization: name -> StructDef clone.
    generic_structs: HashMap<Arc<str>, ast::StructDef>,
    /// Set of already-generated monomorphized specializations (mangled names).
    monomorphized: HashSet<Arc<str>>,
    /// Trait definitions: maps trait name -> ordered list of method signatures.
    /// Used to generate vtable structs and dispatch.
    trait_methods: HashMap<Arc<str>, Vec<(Arc<str>, MirFnSig)>>,
    /// Trait implementations: maps (TraitName, TypeName) -> ordered method names.
    /// Used to populate vtable entries.
    trait_impls: HashMap<(Arc<str>, Arc<str>), Vec<Arc<str>>>,
    /// Effect definitions: maps effect name -> vec of (operation_name, param_types).
    effect_defs: HashMap<Arc<str>, Vec<(Arc<str>, Vec<MirType>)>>,
    /// Closure capture registry: maps closure function name -> list of
    /// (captured_var_name, captured_var_local_id) pairs.  When a call is made
    /// through a local that was assigned from a capturing closure, the extra
    /// captured values are appended as arguments.
    closure_captures: HashMap<Arc<str>, Vec<(Arc<str>, LocalId)>>,
    /// Maps a *local variable* (by its LocalId) that holds a closure function
    /// pointer to the closure's internal function name, so that `lower_call`
    /// can look up captures.
    local_closure_name: HashMap<LocalId, Arc<str>>,
    /// Generic impl blocks stored for deferred monomorphization.
    /// Maps base type name (e.g., "Option") -> list of impl defs.
    generic_impls: HashMap<Arc<str>, Vec<ast::ImplDef>>,
    /// Visibility of the current item being lowered (set by lower_function_with_vis).
    current_item_vis: Option<bool>,
    /// The current impl block's type name (for resolving `Self` in MIR lowering).
    current_impl_type: Option<Arc<str>>,
    /// Module prefix stack for inline `pub mod` blocks.
    /// When inside `pub mod foo { pub mod bar { ... } }`, this contains ["foo", "bar"].
    /// Item names are prefixed with "foo_bar_" for name mangling.
    module_prefix: Vec<Arc<str>>,
    /// Maps module short names to their mangling prefix.
    /// For `pub module std::math`, maps "math" → "std" so that
    /// `math::add()` resolves to `std_add` (not `math_add`).
    module_aliases: HashMap<Arc<str>, Arc<str>>,
    /// Maps bare type names to their full module-prefixed names.
    /// E.g., "Operator" → "tonemap_Operator" when Operator is defined
    /// inside `mod tonemap`. Used for cross-module type references.
    type_module_map: HashMap<Arc<str>, Arc<str>>,
    /// Set of tuple type names already registered as MIR type defs.
    tuple_type_defs: HashSet<Arc<str>>,
    /// Expected return type for the current expression being lowered.
    /// Set from let binding type annotations before lowering init expressions.
    /// Used by resolve_call_return_type to override the i32 fallback.
    pub(crate) expected_type: Option<MirType>,
    /// Tracks the inner type T for locals holding runtime Option<T> values.
    /// Populated from let-binding type annotations (`let x: Option<HashMap<K,V>> = ...`)
    /// and from Some(value) construction. Used by lower_runtime_option_match
    /// to bind pattern variables with the correct type instead of i32.
    pub(crate) option_inner_types: HashMap<LocalId, MirType>,
}

// =============================================================================
// Iterator chain desugaring types
// =============================================================================

/// An intermediate transform operation in an iterator chain.
pub(crate) enum IterStep<'a> {
    /// `.map(|params| body)` — transform each element.
    Map { closure: &'a ast::Expr },
    /// `.enumerate()` — prepend an index to each element.
    Enumerate,
    /// `.cloned()` — identity (no-op for Copy types).
    Cloned,
}

/// Terminal operation of an iterator chain.
pub(crate) enum IterTerminal<'a> {
    /// `.collect()` — gather results into a new Vec.
    Collect,
    /// `.fold(init, |acc, x| body)` — accumulate a single value.
    Fold {
        init: &'a ast::Expr,
        closure: &'a ast::Expr,
    }, // fields accessible via match pattern
}

/// A fully parsed iterator chain: `source.iter().<steps>.<terminal>`.
pub(crate) struct IterChain<'a> {
    /// The source expression (the thing `.iter()` is called on).
    pub(crate) source: &'a ast::Expr,
    /// Ordered list of intermediate transforms.
    pub(crate) steps: Vec<IterStep<'a>>,
    /// The terminal operation.
    pub(crate) terminal: IterTerminal<'a>,
}

impl<'ctx> MirLowerer<'ctx> {
    /// Create a new lowerer.
    pub fn new(ctx: &'ctx TypeContext) -> Self {
        Self {
            ctx,
            module: MirModuleBuilder::new("main"),
            var_map: HashMap::new(),
            loop_stack: Vec::new(),
            current_fn: None,
            source: None,
            closure_count: 0,
            impl_methods: HashMap::new(),
            generic_functions: HashMap::new(),
            generic_enums: HashMap::new(),
            generic_structs: HashMap::new(),
            monomorphized: HashSet::new(),
            trait_methods: HashMap::new(),
            trait_impls: HashMap::new(),
            effect_defs: HashMap::new(),
            closure_captures: HashMap::new(),
            local_closure_name: HashMap::new(),
            generic_impls: HashMap::new(),
            current_item_vis: None,
            current_impl_type: None,
            module_prefix: Vec::new(),
            module_aliases: HashMap::new(),
            type_module_map: HashMap::new(),
            tuple_type_defs: HashSet::new(),
            expected_type: None,
            option_inner_types: HashMap::new(),
        }
    }

    /// Create a new lowerer with source code for macro expansion.
    pub fn with_source(ctx: &'ctx TypeContext, source: Arc<str>) -> Self {
        Self {
            ctx,
            module: MirModuleBuilder::new("main"),
            var_map: HashMap::new(),
            loop_stack: Vec::new(),
            current_fn: None,
            source: Some(source),
            closure_count: 0,
            impl_methods: HashMap::new(),
            generic_functions: HashMap::new(),
            generic_enums: HashMap::new(),
            generic_structs: HashMap::new(),
            monomorphized: HashSet::new(),
            trait_methods: HashMap::new(),
            trait_impls: HashMap::new(),
            effect_defs: HashMap::new(),
            closure_captures: HashMap::new(),
            local_closure_name: HashMap::new(),
            generic_impls: HashMap::new(),
            current_item_vis: None,
            current_impl_type: None,
            module_prefix: Vec::new(),
            module_aliases: HashMap::new(),
            type_module_map: HashMap::new(),
            tuple_type_defs: HashSet::new(),
            expected_type: None,
            option_inner_types: HashMap::new(),
        }
    }

    /// Build a prefixed name for items inside inline modules.
    /// E.g., inside `mod foo { mod bar { fn baz() } }` the prefix
    /// stack is ["foo", "bar"], so `baz` becomes `foo_bar_baz`.
    fn prefixed_name(&self, name: &Arc<str>) -> Arc<str> {
        if self.module_prefix.is_empty() {
            name.clone()
        } else {
            let mut s = String::new();
            for seg in &self.module_prefix {
                s.push_str(seg);
                s.push('_');
            }
            s.push_str(name);
            Arc::from(s)
        }
    }

    /// Resolve a bare function name inside an inline module by trying the
    /// module-prefixed name first (local definition), then falling back to
    /// the unprefixed name (parent scope via `use super::*`).
    ///
    /// For example, inside `pub mod convert`, a call to `xyz_to_lab` will
    /// first check for `convert_xyz_to_lab` in the MIR module, and only
    /// fall back to `xyz_to_lab` if the prefixed version doesn't exist.
    fn resolve_fn_name(&self, bare_name: &str) -> Arc<str> {
        if !self.module_prefix.is_empty() {
            let prefixed = self.prefixed_name(&Arc::from(bare_name));
            if self.module.find_function(prefixed.as_ref()).is_some() {
                return prefixed;
            }
        }
        Arc::from(bare_name)
    }

    /// Register built-in vector math struct types so that field access
    /// (e.g. `v.x`, `v.y`, `v.z`) resolves correctly through
    /// `lookup_struct_field_type`.
    fn register_vector_types(&mut self) {
        let f64_ty = MirType::f64();

        // quanta_vec2 { x: f64, y: f64 }
        self.module.create_struct(
            Arc::from("quanta_vec2"),
            vec![
                (Some(Arc::from("x")), f64_ty.clone()),
                (Some(Arc::from("y")), f64_ty.clone()),
            ],
        );

        // quanta_vec3 { x: f64, y: f64, z: f64 }
        self.module.create_struct(
            Arc::from("quanta_vec3"),
            vec![
                (Some(Arc::from("x")), f64_ty.clone()),
                (Some(Arc::from("y")), f64_ty.clone()),
                (Some(Arc::from("z")), f64_ty.clone()),
            ],
        );

        // quanta_vec4 { x: f64, y: f64, z: f64, w: f64 }
        self.module.create_struct(
            Arc::from("quanta_vec4"),
            vec![
                (Some(Arc::from("x")), f64_ty.clone()),
                (Some(Arc::from("y")), f64_ty.clone()),
                (Some(Arc::from("z")), f64_ty.clone()),
                (Some(Arc::from("w")), f64_ty.clone()),
            ],
        );
    }

    /// Lower a module.
    pub fn lower_module(mut self, module: &ast::Module) -> CodegenResult<MirModule> {
        // Register built-in vector math types before user code
        self.register_vector_types();

        // First pass: collect type definitions and function signatures
        for item in &module.items {
            self.collect_item(item)?;
        }

        // Generate vtables for all (Trait, Type) pairs
        self.generate_vtables();

        // Second pass: lower function bodies
        for item in &module.items {
            self.lower_item(item)?;
        }

        Ok(self.module.build())
    }

    // =========================================================================
    // COLLECTION PASS
    // =========================================================================

    fn collect_item(&mut self, item: &ast::Item) -> CodegenResult<()> {
        match &item.kind {
            ItemKind::Struct(s) => {
                self.collect_struct(s)?;
                // Check for #[derive(...)] attributes and auto-generate impls
                self.process_derive_attrs(&item.attrs, &s.name, s);
                Ok(())
            }
            ItemKind::Enum(e) => self.collect_enum(e),
            ItemKind::Function(f) => self.collect_function(f),
            ItemKind::Impl(impl_def) => self.collect_impl(impl_def),
            // Effect declarations: collect operation signatures so handler
            // lowering can look up parameter types.
            ItemKind::Trait(t) => self.collect_trait(t),
            ItemKind::Effect(e) => self.collect_effect(e),
            // Extern blocks: register foreign function declarations so that
            // calls can resolve return types and the C backend emits proper
            // forward declarations.
            ItemKind::ExternBlock(eb) => self.collect_extern_block(eb),
            // Inline module: push module name onto prefix stack and
            // recursively collect all contained items.
            ItemKind::Mod(m) => self.collect_inline_mod(m),
            // Use declarations (e.g. `use super::*`) are handled implicitly
            // by the flattened naming scheme — no collection needed.
            ItemKind::Use(_) => Ok(()),
            _ => Ok(()),
        }
    }

    /// Generate vtable definitions for all (Trait, Type) implementation pairs.
    fn generate_vtables(&mut self) {
        // Copy trait_methods to the MirModule for the C backend to access
        let trait_methods_clone = self.trait_methods.clone();
        for (trait_name, methods) in &trait_methods_clone {
            self.module
                .module_mut()
                .trait_methods
                .insert(trait_name.clone(), methods.clone());
        }

        // For each (Trait, Type) impl pair, create a vtable
        let impl_pairs: Vec<_> = self.trait_impls.keys().cloned().collect();
        for (trait_name, type_name) in impl_pairs {
            if let Some(trait_methods) = self.trait_methods.get(&trait_name) {
                let methods: Vec<_> = trait_methods
                    .iter()
                    .map(|(method_name, sig)| {
                        let mangled = self
                            .impl_methods
                            .get(&(type_name.clone(), method_name.clone()))
                            .cloned()
                            .unwrap_or_else(|| Arc::from(format!("{}_{}", type_name, method_name)));
                        (method_name.clone(), mangled, sig.clone())
                    })
                    .collect();

                self.module.module_mut().vtables.push(MirVtable {
                    trait_name: trait_name.clone(),
                    type_name: type_name.clone(),
                    methods,
                });
            }
        }
    }

    fn collect_trait(&mut self, t: &ast::TraitDef) -> CodegenResult<()> {
        let trait_name: Arc<str> = t.name.name.clone();
        let mut methods = Vec::new();

        for item in &t.items {
            if let ast::TraitItemKind::Function(fndef) = &item.kind {
                let params: Vec<MirType> = fndef
                    .sig
                    .params
                    .iter()
                    .map(|p| self.lower_type_from_ast(&p.ty))
                    .collect();
                let ret = fndef
                    .sig
                    .return_ty
                    .as_ref()
                    .map(|t| self.lower_type_from_ast(t))
                    .unwrap_or(MirType::Void);

                // First param is self — replace with void* for vtable fn ptr
                let mut vtable_params = vec![MirType::Ptr(Box::new(MirType::Void))];
                vtable_params.extend(params.into_iter().skip(1));

                let fn_sig = MirFnSig::new(vtable_params, ret);
                methods.push((fndef.name.name.clone(), fn_sig));
            }
        }

        self.trait_methods.insert(trait_name, methods);
        Ok(())
    }

    fn collect_effect(&mut self, e: &ast::EffectDef) -> CodegenResult<()> {
        let effect_name: Arc<str> = Arc::from(e.name.name.as_ref());
        let ops: Vec<(Arc<str>, Vec<MirType>)> = e
            .operations
            .iter()
            .map(|op| {
                let param_types: Vec<MirType> = op
                    .params
                    .iter()
                    .map(|p| self.lower_type_from_ast(&p.ty))
                    .collect();
                (Arc::from(op.name.name.as_ref()), param_types)
            })
            .collect();
        self.effect_defs.insert(effect_name, ops);
        Ok(())
    }

    /// Collect extern block declarations.  Each foreign function is registered
    /// as a declaration-only `MirFunction` so that call-site return-type
    /// resolution works and the C backend emits proper forward declarations.
    fn collect_extern_block(&mut self, eb: &ast::ExternBlockDef) -> CodegenResult<()> {
        for foreign_item in &eb.items {
            if let ast::ForeignItemKind::Fn(f) = &foreign_item.kind {
                // Build parameter types.  For extern "C" functions, map `&str`
                // directly to `Ptr(i8)` (i.e. `const char*`) rather than
                // `Ptr(QuantaString)`.
                let params: Vec<MirType> = f
                    .sig
                    .params
                    .iter()
                    .map(|p| self.lower_ffi_type(&p.ty))
                    .collect();

                let ret = f
                    .sig
                    .return_ty
                    .as_ref()
                    .map(|t| self.lower_ffi_type(t))
                    .unwrap_or(MirType::Void);

                let mut sig = MirFnSig::new(params, ret);
                sig.calling_conv = CallingConv::C;
                sig.is_variadic = f.sig.params.iter().any(|_| false); // checked below

                // Check for variadic: if the last token in the AST param
                // list is `...` we won't see it as a Param; instead we rely
                // on the function signature's abi hint.  For now, detect
                // common variadic C functions by name.
                // TODO: Add proper variadic parsing support.

                let mut func = MirFunction::declaration(f.name.name.clone(), sig);
                func.is_public = true;

                // Set parameter names on the declaration so the C backend can
                // emit readable prototypes.
                for (i, param) in f.sig.params.iter().enumerate() {
                    if let ast::PatternKind::Ident { name, .. } = &param.pattern.kind {
                        let local = MirLocal {
                            id: LocalId(i as u32),
                            name: Some(name.name.clone()),
                            ty: self.lower_ffi_type(&param.ty),
                            is_mut: false,
                            is_param: true,
                            annotations: Vec::new(),
                        };
                        func.locals.push(local);
                    }
                }

                self.module.add_function(func);
            }
        }
        Ok(())
    }

    /// Collect items inside an inline `mod` block.
    ///
    /// Items are collected with their names prefixed by the module path so
    /// that `pub mod math { pub fn add(...) }` registers a function named
    /// `math_add`.  Path-based calls like `math::add(...)` are resolved to
    /// `math_add` by `lower_path`, which joins path segments with `_`.
    fn collect_inline_mod(&mut self, m: &ast::ModDef) -> CodegenResult<()> {
        if let Some(ref content) = m.content {
            self.module_prefix.push(m.name.name.clone());
            for item in &content.items {
                // Rewrite item names with the module prefix before collecting.
                let rewritten = self.rewrite_item_with_prefix(item);
                self.collect_item(&rewritten)?;
            }
            self.module_prefix.pop();
        }
        Ok(())
    }

    /// Rewrite an item's name by prepending the current module prefix.
    /// This is used to "flatten" items from inline modules into the
    /// top-level scope with mangled names (e.g. `mod_fn` for `mod::fn`).
    fn rewrite_item_with_prefix(&self, item: &ast::Item) -> ast::Item {
        if self.module_prefix.is_empty() {
            return item.clone();
        }
        let new_kind = match &item.kind {
            ItemKind::Function(f) => {
                let mut f2 = f.as_ref().clone();
                f2.name = ast::Ident {
                    name: self.prefixed_name(&f.name.name),
                    span: f.name.span,
                };
                ItemKind::Function(Box::new(f2))
            }
            ItemKind::Struct(s) => {
                let mut s2 = s.as_ref().clone();
                s2.name = ast::Ident {
                    name: self.prefixed_name(&s.name.name),
                    span: s.name.span,
                };
                ItemKind::Struct(Box::new(s2))
            }
            ItemKind::Enum(e) => {
                let mut e2 = e.as_ref().clone();
                e2.name = ast::Ident {
                    name: self.prefixed_name(&e.name.name),
                    span: e.name.span,
                };
                ItemKind::Enum(Box::new(e2))
            }
            ItemKind::Const(c) => {
                let mut c2 = c.as_ref().clone();
                c2.name = ast::Ident {
                    name: self.prefixed_name(&c.name.name),
                    span: c.name.span,
                };
                ItemKind::Const(Box::new(c2))
            }
            ItemKind::Static(s) => {
                let mut s2 = s.as_ref().clone();
                s2.name = ast::Ident {
                    name: self.prefixed_name(&s.name.name),
                    span: s.name.span,
                };
                ItemKind::Static(Box::new(s2))
            }
            ItemKind::Trait(t) => {
                let mut t2 = t.as_ref().clone();
                t2.name = ast::Ident {
                    name: self.prefixed_name(&t.name.name),
                    span: t.name.span,
                };
                ItemKind::Trait(Box::new(t2))
            }
            // Impl, Use, Mod, etc. — pass through unchanged
            _ => item.kind.clone(),
        };
        ast::Item {
            kind: new_kind,
            vis: item.vis.clone(),
            attrs: item.attrs.clone(),
            span: item.span,
            id: item.id,
        }
    }

    /// Lower an AST type for FFI usage.  This differs from `lower_type_from_ast`
    /// in that `&str` maps to `Ptr(i8)` (`const char*`) rather than
    /// `Ptr(QuantaString)`, which is what C library functions expect.
    fn lower_ffi_type(&self, ty: &ast::Type) -> MirType {
        match &ty.kind {
            ast::TypeKind::Ref { ty: inner, .. } => {
                // &str -> const char* for FFI
                if let ast::TypeKind::Path(path) = &inner.kind {
                    if let Some(ident) = path.last_ident() {
                        if ident.name.as_ref() == "str" || ident.name.as_ref() == "String" {
                            return MirType::Ptr(Box::new(MirType::i8()));
                        }
                    }
                }
                MirType::Ptr(Box::new(self.lower_ffi_type(inner)))
            }
            _ => self.lower_type_from_ast(ty),
        }
    }

    fn collect_struct(&mut self, s: &ast::StructDef) -> CodegenResult<()> {
        // If the struct has generic type parameters, store for later monomorphization
        let has_generics = s
            .generics
            .params
            .iter()
            .any(|p| matches!(p.kind, ast::GenericParamKind::Type { .. }));

        if has_generics {
            self.generic_structs.insert(s.name.name.clone(), s.clone());
            return Ok(());
        }

        let fields = match &s.fields {
            ast::StructFields::Named(fields) => fields
                .iter()
                .map(|f| {
                    let ty = self.lower_type_from_ast(&f.ty);
                    (Some(f.name.name.clone()), ty)
                })
                .collect(),
            ast::StructFields::Tuple(fields) => fields
                .iter()
                .map(|f| {
                    let ty = self.lower_type_from_ast(&f.ty);
                    (None, ty)
                })
                .collect(),
            ast::StructFields::Unit => Vec::new(),
        };

        let full_name = s.name.name.clone();
        self.module.create_struct(full_name.clone(), fields);

        // Register the bare→prefixed mapping so cross-module type
        // references can resolve (e.g., bare "Operator" → "tonemap_Operator").
        if !self.module_prefix.is_empty() {
            // Extract the bare name by stripping the prefix
            let prefix = self.module_prefix.iter()
                .map(|s| s.as_ref())
                .collect::<Vec<_>>()
                .join("_");
            let prefix_with_sep = format!("{}_", prefix);
            if let Some(bare) = full_name.strip_prefix(&prefix_with_sep) {
                self.type_module_map.insert(Arc::from(bare), full_name);
            }
        }

        Ok(())
    }

    /// Process #[derive(...)] attributes on a struct and register auto-generated methods.
    fn process_derive_attrs(
        &mut self,
        attrs: &[ast::Attribute],
        name: &ast::Ident,
        _s: &ast::StructDef,
    ) {
        for attr in attrs {
            if let Some(seg) = attr.path.segments.first() {
                if seg.ident.name.as_ref() == "derive" {
                    // Extract derive trait names from the attribute arguments
                    // For now we recognize Clone by checking the token stream
                    let has_clone = self.attr_has_derive_name(attr, "Clone");
                    if has_clone {
                        // Register a clone method: TypeName_clone(self) -> TypeName
                        // The actual body is trivial for value types — just return self.
                        let type_name = name.name.clone();
                        let method_name: Arc<str> = Arc::from(format!("{}_clone", type_name));
                        self.impl_methods
                            .insert((type_name.clone(), Arc::from("clone")), method_name.clone());
                        // The actual function will be generated in lower_item pass
                        // by checking for this registered method.
                    }
                }
            }
        }
    }

    /// Check if a derive attribute contains a specific trait name.
    fn attr_has_derive_name(&self, attr: &ast::Attribute, target: &str) -> bool {
        if let ast::AttrArgs::Delimited(tokens) = &attr.args {
            for tok in tokens {
                if let ast::TokenTree::Token(t) = tok {
                    if let crate::lexer::TokenKind::Ident = &t.kind {
                        // Check token text via source span
                        if let Some(ref src) = self.source {
                            let start = t.span.start.to_usize();
                            let end = t.span.end.to_usize();
                            if end <= src.len() && &src[start..end] == target {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    fn collect_enum(&mut self, e: &ast::EnumDef) -> CodegenResult<()> {
        // If the enum has generic type parameters, store for later monomorphization
        let has_generics = e
            .generics
            .params
            .iter()
            .any(|p| matches!(p.kind, ast::GenericParamKind::Type { .. }));

        if has_generics {
            self.generic_enums.insert(e.name.name.clone(), e.clone());
            return Ok(());
        }

        let variants: Vec<_> = e
            .variants
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let fields = match &v.fields {
                    ast::StructFields::Named(fields) => fields
                        .iter()
                        .map(|f| (Some(f.name.name.clone()), self.lower_type_from_ast(&f.ty)))
                        .collect(),
                    ast::StructFields::Tuple(fields) => fields
                        .iter()
                        .map(|f| (None, self.lower_type_from_ast(&f.ty)))
                        .collect(),
                    ast::StructFields::Unit => Vec::new(),
                };

                MirEnumVariant {
                    name: v.name.name.clone(),
                    discriminant: i as i128,
                    fields,
                }
            })
            .collect();

        let full_name = e.name.name.clone();
        self.module
            .create_enum(full_name.clone(), MirType::i32(), variants);

        // Register bare→prefixed mapping for cross-module resolution
        if !self.module_prefix.is_empty() {
            let prefix = self.module_prefix.iter()
                .map(|s| s.as_ref())
                .collect::<Vec<_>>()
                .join("_");
            let prefix_with_sep = format!("{}_", prefix);
            if let Some(bare) = full_name.strip_prefix(&prefix_with_sep) {
                self.type_module_map.insert(Arc::from(bare), full_name);
            }
        }

        Ok(())
    }

    /// Monomorphize a generic enum for a specific concrete type (single-param shorthand).
    /// E.g., `Option<T>` + `i32` → `Option_i32` with `Some(i32)` variant.
    fn monomorphize_enum(
        &mut self,
        enum_name: &str,
        concrete_ty: &MirType,
    ) -> CodegenResult<Arc<str>> {
        // Build single-param substitution map and delegate to multi-param version
        let enum_def = self.generic_enums.get(enum_name).cloned();
        if let Some(ref e) = enum_def {
            let type_param = e
                .generics
                .params
                .iter()
                .find_map(|p| match &p.kind {
                    ast::GenericParamKind::Type { .. } => Some(p.ident.name.clone()),
                    _ => None,
                })
                .unwrap_or(Arc::from("T"));
            let mut subst = HashMap::new();
            subst.insert(type_param, concrete_ty.clone());
            return self.monomorphize_enum_multi(enum_name, &subst);
        }
        Ok(Arc::from(enum_name))
    }

    /// Monomorphize a generic enum with a multi-parameter substitution map.
    /// E.g., `Result<T, E>` + `{T: i32, E: QuantaString}` → `Result_E_QuantaString_T_i32`.
    fn monomorphize_enum_multi(
        &mut self,
        enum_name: &str,
        subst: &HashMap<Arc<str>, MirType>,
    ) -> CodegenResult<Arc<str>> {
        let mangled_name = Self::mangle_generic_name(enum_name, subst);

        if self.monomorphized.contains(&mangled_name) {
            return Ok(mangled_name);
        }
        self.monomorphized.insert(mangled_name.clone());

        let enum_def = self.generic_enums.get(enum_name).cloned();
        let enum_def = match enum_def {
            Some(e) => e,
            None => return Ok(Arc::from(enum_name)), // Not generic, use as-is
        };

        // Build monomorphized variants using the substitution map
        let variants: Vec<_> = enum_def
            .variants
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let fields = match &v.fields {
                    ast::StructFields::Named(fields) => fields
                        .iter()
                        .map(|f| {
                            let ty = self.substitute_type_from_ast(&f.ty, subst);
                            (Some(f.name.name.clone()), ty)
                        })
                        .collect(),
                    ast::StructFields::Tuple(fields) => fields
                        .iter()
                        .map(|f| {
                            let ty = self.substitute_type_from_ast(&f.ty, subst);
                            (None, ty)
                        })
                        .collect(),
                    ast::StructFields::Unit => Vec::new(),
                };

                MirEnumVariant {
                    name: v.name.name.clone(),
                    discriminant: i as i128,
                    fields,
                }
            })
            .collect();

        self.module
            .create_enum(mangled_name.clone(), MirType::i32(), variants);

        // Also monomorphize any impl blocks for this generic enum
        self.monomorphize_impl_methods(enum_name, &mangled_name, subst)?;

        Ok(mangled_name)
    }

    /// Monomorphize a generic struct for specific concrete types.
    /// E.g., `Pair<T> { first: T, second: T }` + `{T: i32}` → `Pair_i32`.
    fn monomorphize_struct(
        &mut self,
        struct_name: &str,
        subst: &HashMap<Arc<str>, MirType>,
    ) -> CodegenResult<Arc<str>> {
        let mangled_name = Self::mangle_generic_name(struct_name, subst);

        if self.monomorphized.contains(&mangled_name) {
            return Ok(mangled_name);
        }
        self.monomorphized.insert(mangled_name.clone());

        let struct_def = self.generic_structs.get(struct_name).cloned();
        let struct_def = match struct_def {
            Some(s) => s,
            None => return Ok(Arc::from(struct_name)), // Not generic, use as-is
        };

        let fields = match &struct_def.fields {
            ast::StructFields::Named(fields) => fields
                .iter()
                .map(|f| {
                    let ty = self.substitute_type_from_ast(&f.ty, subst);
                    (Some(f.name.name.clone()), ty)
                })
                .collect(),
            ast::StructFields::Tuple(fields) => fields
                .iter()
                .map(|f| {
                    let ty = self.substitute_type_from_ast(&f.ty, subst);
                    (None, ty)
                })
                .collect(),
            ast::StructFields::Unit => Vec::new(),
        };

        self.module.create_struct(mangled_name.clone(), fields);

        // Also monomorphize any impl blocks for this generic struct
        self.monomorphize_impl_methods(struct_name, &mangled_name, subst)?;

        Ok(mangled_name)
    }

    /// Resolve an AST type using a substitution map for generic type parameters.
    /// Falls back to `lower_type_from_ast` for non-generic types.
    fn substitute_type_from_ast(
        &self,
        ty: &ast::Type,
        subst: &HashMap<Arc<str>, MirType>,
    ) -> MirType {
        match &ty.kind {
            ast::TypeKind::Path(path) => {
                if path.is_simple() {
                    if let Some(ident) = path.last_ident() {
                        // Check if this type name is a generic parameter to substitute
                        if let Some(concrete) = subst.get(&ident.name) {
                            return concrete.clone();
                        }
                    }
                }
                // Check for generic type references like Option<T> in field types
                if let Some(ident) = path.last_ident() {
                    if let Some(generic_args) = path.last_generics() {
                        if !generic_args.is_empty() {
                            let inner_subst = self.resolve_generic_args_with_subst(
                                ident.name.as_ref(),
                                generic_args,
                                subst,
                            );
                            if !inner_subst.is_empty() {
                                let mangled =
                                    Self::mangle_generic_name(ident.name.as_ref(), &inner_subst);
                                return MirType::Struct(mangled);
                            }
                        }
                    }
                }
                self.lower_type_from_ast(ty)
            }
            ast::TypeKind::Ref { ty: inner, .. } | ast::TypeKind::Ptr { ty: inner, .. } => {
                MirType::Ptr(Box::new(self.substitute_type_from_ast(inner, subst)))
            }
            ast::TypeKind::Array { elem, len } => {
                let elem_ty = self.substitute_type_from_ast(elem, subst);
                let length = self
                    .try_const_eval(len)
                    .and_then(|c| match c {
                        MirConst::Int(v, _) => Some(v as u64),
                        MirConst::Uint(v, _) => Some(v as u64),
                        _ => None,
                    })
                    .unwrap_or(0);
                MirType::Array(Box::new(elem_ty), length)
            }
            ast::TypeKind::Slice(inner) => {
                MirType::Slice(Box::new(self.substitute_type_from_ast(inner, subst)))
            }
            _ => self.lower_type_from_ast(ty),
        }
    }

    /// Resolve generic arguments from a path using an existing substitution map.
    /// E.g., given `Option<T>` and subst `{T: i32}`, returns `{T: i32}` for Option's params.
    fn resolve_generic_args_with_subst(
        &self,
        type_name: &str,
        generic_args: &[ast::GenericArg],
        outer_subst: &HashMap<Arc<str>, MirType>,
    ) -> HashMap<Arc<str>, MirType> {
        let mut result = HashMap::new();

        // Get the type parameter names from the generic definition
        let param_names: Vec<Arc<str>> = if let Some(enum_def) = self.generic_enums.get(type_name) {
            enum_def
                .generics
                .params
                .iter()
                .filter_map(|p| match &p.kind {
                    ast::GenericParamKind::Type { .. } => Some(p.ident.name.clone()),
                    _ => None,
                })
                .collect()
        } else if let Some(struct_def) = self.generic_structs.get(type_name) {
            struct_def
                .generics
                .params
                .iter()
                .filter_map(|p| match &p.kind {
                    ast::GenericParamKind::Type { .. } => Some(p.ident.name.clone()),
                    _ => None,
                })
                .collect()
        } else {
            return result;
        };

        for (i, arg) in generic_args.iter().enumerate() {
            if let ast::GenericArg::Type(arg_ty) = arg {
                if let Some(param_name) = param_names.get(i) {
                    result.insert(
                        param_name.clone(),
                        self.substitute_type_from_ast(arg_ty, outer_subst),
                    );
                }
            }
        }

        result
    }

    /// Generate a mangled name from a base name and a substitution map.
    /// E.g., ("Result", {T: i32, E: QuantaString}) → "Result_i32_QuantaString"
    fn mangle_generic_name(base: &str, subst: &HashMap<Arc<str>, MirType>) -> Arc<str> {
        let mut parts = vec![base.to_string()];
        // Sort by param name for deterministic mangling
        let mut entries: Vec<_> = subst.iter().collect();
        entries.sort_by_key(|(k, _)| (*k).clone());
        for (_, ty) in entries {
            parts.push(Self::mangle_type(ty));
        }
        Arc::from(parts.join("_"))
    }

    /// Monomorphize all impl blocks for a generic type using the given substitution map.
    /// For each method in the impl, generates a specialized version like `Option_T_i32_unwrap_or`.
    fn monomorphize_impl_methods(
        &mut self,
        base_type_name: &str,
        mangled_type_name: &Arc<str>,
        subst: &HashMap<Arc<str>, MirType>,
    ) -> CodegenResult<()> {
        let impl_defs = match self.generic_impls.get(base_type_name) {
            Some(impls) => impls.clone(),
            None => return Ok(()),
        };

        for impl_def in &impl_defs {
            for impl_item in &impl_def.items {
                if let ast::ImplItemKind::Function(f) = &impl_item.kind {
                    let method_name = f.name.name.clone();
                    let mangled_fn_name: Arc<str> =
                        Arc::from(format!("{}_{}", mangled_type_name, method_name));

                    // Register in impl_methods so method calls resolve
                    self.impl_methods.insert(
                        (mangled_type_name.clone(), method_name.clone()),
                        mangled_fn_name.clone(),
                    );

                    // Skip if already generated
                    if self.monomorphized.contains(&mangled_fn_name) {
                        continue;
                    }
                    self.monomorphized.insert(mangled_fn_name.clone());

                    // Build a specialized FnDef with type params replaced.
                    // We need to also replace `self` type references with the
                    // mangled type, and the function name is already mangled.
                    let mut specialized = Self::monomorphize_fndef_multi(
                        f,
                        subst,
                        f.name.name.clone(), // Keep original name; we rename below
                    );
                    // Override the name to our mangled fn name
                    specialized.name = ast::Ident {
                        name: mangled_fn_name.clone(),
                        span: f.name.span,
                    };

                    // Save context, lower as impl method, restore.
                    // lower_impl_method will NOT re-mangle; it uses the
                    // fn name from the def, but prepends type_name_.
                    // So we use lower_function directly and manually handle
                    // the self param type.
                    let saved_fn = self.current_fn.take();
                    let saved_vars = std::mem::take(&mut self.var_map);

                    self.lower_generic_impl_method(mangled_type_name, &specialized)?;

                    self.current_fn = saved_fn;
                    self.var_map = saved_vars;
                }
            }
        }

        Ok(())
    }

    fn collect_function(&mut self, f: &ast::FnDef) -> CodegenResult<()> {
        // If the function has generic type parameters, store it for later
        // monomorphization instead of lowering it immediately.
        if self.fn_has_type_generics(f) {
            self.generic_functions
                .insert(f.name.name.clone(), f.clone());
            return Ok(());
        }

        // Register a forward declaration so that resolve_call_return_type
        // can find the function's return type even before its body is lowered.
        // Skip `main` since its return type is special-cased during lowering.
        if f.name.name.as_ref() != "main" {
            let params: Vec<MirType> = f
                .sig
                .params
                .iter()
                .map(|p| self.lower_type_from_ast(&p.ty))
                .collect();
            let ret = f
                .sig
                .return_ty
                .as_ref()
                .map(|t| self.lower_type_from_ast(t))
                .unwrap_or(MirType::Void);
            let sig = MirFnSig::new(params, ret);
            self.module.declare_function(f.name.name.clone(), sig);
        }

        Ok(())
    }

    /// Collect impl block methods, registering them as `TypeName_methodName` functions.
    fn collect_impl(&mut self, impl_def: &ast::ImplDef) -> CodegenResult<()> {
        let type_name = self.resolve_type_name(&impl_def.self_ty);

        // Check if this is an impl on a generic type (e.g., impl<T> Option<T> { ... })
        // If so, store for deferred monomorphization.
        let has_impl_generics = impl_def
            .generics
            .params
            .iter()
            .any(|p| matches!(p.kind, ast::GenericParamKind::Type { .. }));
        let is_generic_type = self.generic_enums.contains_key(type_name.as_ref())
            || self.generic_structs.contains_key(type_name.as_ref());

        if has_impl_generics || is_generic_type {
            self.generic_impls
                .entry(type_name.clone())
                .or_insert_with(Vec::new)
                .push(impl_def.clone());
            return Ok(());
        }

        // Track trait implementations for vtable generation
        let trait_name = impl_def.trait_ref.as_ref().map(|tr| {
            tr.path
                .last_ident()
                .map(|i| i.name.clone())
                .unwrap_or(Arc::from("Unknown"))
        });

        let mut impl_method_names = Vec::new();

        for impl_item in &impl_def.items {
            if let ast::ImplItemKind::Function(f) = &impl_item.kind {
                let method_name = f.name.name.clone();
                let mangled: Arc<str> = Arc::from(format!("{}_{}", type_name, method_name));
                self.impl_methods
                    .insert((type_name.clone(), method_name.clone()), mangled.clone());
                impl_method_names.push(mangled.clone());

                // Forward-declare the method signature so resolve_call_return_type
                // can find the correct return type during lowering.
                let mut params = Vec::new();
                for param in &f.sig.params {
                    let is_self = matches!(
                        &param.pattern.kind,
                        ast::PatternKind::Ident { name, .. } if name.name.as_ref() == "self"
                    );
                    if is_self {
                        if matches!(param.ty.kind, ast::TypeKind::Ref { .. }) {
                            params.push(MirType::Ptr(Box::new(MirType::Struct(type_name.clone()))));
                        } else {
                            params.push(MirType::Struct(type_name.clone()));
                        }
                    } else {
                        params.push(self.lower_type_from_ast(&param.ty));
                    }
                }
                let ret = f
                    .sig
                    .return_ty
                    .as_ref()
                    .map(|t| self.lower_type_from_ast(t))
                    .unwrap_or(MirType::Void);
                self.module
                    .declare_function(mangled, MirFnSig::new(params, ret));
            }
        }

        // Register trait impl for vtable generation
        if let Some(ref tname) = trait_name {
            self.trait_impls
                .insert((tname.clone(), type_name.clone()), impl_method_names);
        }

        Ok(())
    }

    /// Extract a type name from an AST Type node (used for impl block self_ty).
    fn resolve_type_name(&self, ty: &ast::Type) -> Arc<str> {
        let bare_name = match &ty.kind {
            ast::TypeKind::Path(path) => path
                .last_ident()
                .map(|i| i.name.clone())
                .unwrap_or(Arc::from("Unknown")),
            _ => Arc::from("Unknown"),
        };
        // Inside a module, use the prefixed name for user-defined types
        // to match struct typedefs (e.g., `Vec3` -> `std_Vec3`).
        // Do NOT prefix builtins (Vec, HashMap, String, etc.).
        if !self.module_prefix.is_empty() {
            let name = bare_name.as_ref();
            let is_builtin = matches!(
                name,
                "Vec"
                    | "HashMap"
                    | "HashSet"
                    | "BTreeMap"
                    | "BTreeSet"
                    | "String"
                    | "Option"
                    | "Result"
                    | "Box"
            );
            if !is_builtin {
                let prefixed = self.prefixed_name(&bare_name);
                if self.module.find_type(prefixed.as_ref()).is_some() {
                    return prefixed;
                }
            }
        }
        bare_name
    }

    // =========================================================================
    // LOWERING PASS
    // =========================================================================

    fn lower_item(&mut self, item: &ast::Item) -> CodegenResult<()> {
        match &item.kind {
            ItemKind::Function(f) => self.lower_function_with_vis(f, &item.attrs, &item.vis),
            ItemKind::Struct(s) => {
                // Generate derive'd methods (e.g., clone)
                self.generate_derive_methods(&s.name, &item.attrs)
            }
            ItemKind::Static(s) => {
                // Check for #[uniform] attribute
                if Self::has_attribute(&item.attrs, "uniform") {
                    self.extract_uniform_from_static(s);
                }
                self.lower_static(s)
            }
            ItemKind::Const(c) => {
                // Check for #[uniform] attribute
                if Self::has_attribute(&item.attrs, "uniform") {
                    self.extract_uniform_from_const(c);
                }
                self.lower_const(c)
            }
            ItemKind::Impl(impl_def) => self.lower_impl(impl_def),
            // Effect declarations produce no code -- handled at type-check time
            // and dispatched via effect_id at runtime.
            ItemKind::Effect(_) => Ok(()),
            // Inline module: push module name onto prefix stack and
            // recursively lower all contained items.
            ItemKind::Mod(m) => self.lower_inline_mod(m, &item.attrs),
            // Use declarations — no-op in lowering (name resolution is
            // handled by the prefix + join scheme).
            ItemKind::Use(_) => Ok(()),
            _ => Ok(()),
        }
    }

    /// Lower items inside an inline `mod` block.
    ///
    /// Similar to `collect_inline_mod`, this pushes the module name onto
    /// the prefix stack and rewrites item names before lowering.
    fn lower_inline_mod(
        &mut self,
        m: &ast::ModDef,
        _attrs: &[ast::Attribute],
    ) -> CodegenResult<()> {
        if let Some(ref content) = m.content {
            self.module_prefix.push(m.name.name.clone());
            for item in &content.items {
                let rewritten = self.rewrite_item_with_prefix(item);
                self.lower_item(&rewritten)?;
            }
            self.module_prefix.pop();
        }
        Ok(())
    }

    /// Check if any attribute has the given name.
    fn has_attribute(attrs: &[ast::Attribute], name: &str) -> bool {
        attrs.iter().any(|attr| {
            attr.path
                .segments
                .first()
                .map_or(false, |seg| seg.ident.name.as_ref() == name)
        })
    }

    /// Extract a uniform declaration from a const item with #[uniform].
    fn extract_uniform_from_const(&mut self, c: &ast::ConstDef) {
        let ty = self.lower_type_from_ast(&c.ty);
        let default = c.value.as_ref().and_then(|e| self.try_const_eval(e));
        self.module.module_mut().uniforms.push(MirUniform {
            name: c.name.name.clone(),
            ty,
            default,
        });
    }

    /// Extract a uniform declaration from a static item with #[uniform].
    fn extract_uniform_from_static(&mut self, s: &ast::StaticDef) {
        let ty = self.lower_type_from_ast(&s.ty);
        let default = s.value.as_ref().and_then(|e| self.try_const_eval(e));
        self.module.module_mut().uniforms.push(MirUniform {
            name: s.name.name.clone(),
            ty,
            default,
        });
    }

    /// Extract shader stage from function attributes (#[vertex], #[fragment], #[compute]).
    fn extract_shader_stage(attrs: &[ast::Attribute]) -> Option<ShaderStage> {
        for attr in attrs {
            if let Some(seg) = attr.path.segments.first() {
                match seg.ident.name.as_ref() {
                    "vertex" => return Some(ShaderStage::Vertex),
                    "fragment" => return Some(ShaderStage::Fragment),
                    "compute" => return Some(ShaderStage::Compute),
                    _ => {}
                }
            }
        }
        None
    }

    /// Extract uniform binding info from attributes: #[uniform(binding = N, set = M)]
    fn extract_uniform_binding(&self, attrs: &[ast::Attribute]) -> Option<(u32, u32)> {
        for attr in attrs {
            if let Some(seg) = attr.path.segments.first() {
                if seg.ident.name.as_ref() == "uniform" {
                    let mut binding = 0u32;
                    let mut set = 0u32;
                    if let ast::AttrArgs::Delimited(tokens) = &attr.args {
                        // Parse binding = N, set = M from token stream
                        let mut i = 0;
                        while i < tokens.len() {
                            if let ast::TokenTree::Token(tok) = &tokens[i] {
                                if let crate::lexer::TokenKind::Ident = &tok.kind {
                                    // Extract identifier name from source via span
                                    let name_str = if let Some(ref src) = self.source {
                                        let start = tok.span.start.to_usize();
                                        let end = tok.span.end.to_usize();
                                        if end <= src.len() {
                                            Some(src[start..end].to_string())
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    };
                                    // Look for "= N" after the identifier
                                    if let Some(name_str) = name_str {
                                        if i + 2 < tokens.len() {
                                            if let ast::TokenTree::Token(ref num_tok) =
                                                tokens[i + 2]
                                            {
                                                if let crate::lexer::TokenKind::Literal { .. } =
                                                    &num_tok.kind
                                                {
                                                    // Extract numeric value from source via span
                                                    if let Some(ref src) = self.source {
                                                        let start = num_tok.span.start.to_usize();
                                                        let end = num_tok.span.end.to_usize();
                                                        if end <= src.len() {
                                                            if let Ok(n) =
                                                                src[start..end].parse::<u32>()
                                                            {
                                                                match name_str.as_str() {
                                                                    "binding" => binding = n,
                                                                    "set" => set = n,
                                                                    _ => {}
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            i += 1;
                        }
                    }
                    return Some((set, binding));
                }
            }
        }
        None
    }

    /// Lower a function with explicit visibility from the Item node.
    fn lower_function_with_vis(
        &mut self,
        f: &ast::FnDef,
        attrs: &[ast::Attribute],
        vis: &ast::Visibility,
    ) -> CodegenResult<()> {
        self.current_item_vis = Some(vis.is_public());
        let result = self.lower_function(f, attrs);
        self.current_item_vis = None;
        result
    }

    fn lower_function(&mut self, f: &ast::FnDef, attrs: &[ast::Attribute]) -> CodegenResult<()> {
        // Skip generic functions — they are monomorphized on demand at call sites.
        if self.fn_has_type_generics(f) {
            return Ok(());
        }

        // Build function signature
        let params: Vec<_> = f
            .sig
            .params
            .iter()
            .map(|p| self.lower_type_from_ast(&p.ty))
            .collect();

        let is_main = f.name.name.as_ref() == "main";
        let has_shader_stage = Self::extract_shader_stage(attrs).is_some();

        let ret = if is_main && f.sig.return_ty.is_none() && !has_shader_stage {
            // C requires main to return int (but not shader main)
            MirType::i32()
        } else {
            f.sig
                .return_ty
                .as_ref()
                .map(|t| self.lower_type_from_ast(t))
                .unwrap_or(MirType::Void)
        };

        // Register tuple type defs for any tuple types in the signature.
        if let MirType::Tuple(ref elems) = ret {
            self.ensure_tuple_type_def(elems);
        }

        let sig = MirFnSig::new(params, ret);
        let fn_ret_ty = sig.ret.clone();

        if let Some(body) = &f.body {
            // Save the current function context so that nested function
            // definitions (e.g. `fn helper()` inside another function body)
            // do not clobber the enclosing function's builder and var_map.
            let saved_fn = self.current_fn.take();
            let saved_vars = std::mem::take(&mut self.var_map);

            // Create function builder
            let mut builder = MirBuilder::new(f.name.name.clone(), sig);

            // Map parameters to locals and set their names + annotations
            for (i, param) in f.sig.params.iter().enumerate() {
                if let ast::PatternKind::Ident { name, .. } = &param.pattern.kind {
                    let local_id = builder.param_local(i);
                    builder.set_param_name(i, name.name.clone());
                    // Extract type annotations (e.g., `with ColorSpace<Linear>`)
                    let annotations = Self::extract_type_annotations(&param.ty);
                    if !annotations.is_empty() {
                        builder.set_param_annotations(i, annotations);
                    }
                    self.var_map.insert(name.name.clone(), local_id);
                }
            }

            self.current_fn = Some(builder);

            // Lower function body
            let result = self.lower_block(body)?;

            // Add return if needed
            let mut builder = self.current_fn.take().unwrap();
            if is_main && !has_shader_stage {
                // main returns 0 (success) in C — but not for shader entry points
                let zero = MirValue::Const(MirConst::Int(0, MirType::i32()));
                builder.ret(Some(zero));
            } else if f.sig.return_ty.is_some() {
                if let Some(val) = result {
                    // If the result is a local that might not be declared
                    // (created in a block unreachable from the while loop exit),
                    // ensure the local exists by re-creating it if needed.
                    if let MirValue::Local(id) = &val {
                        if builder.local_type(*id).is_none() {
                            let ret_ty = fn_ret_ty.clone();
                            let ret_local = builder.create_local(ret_ty.clone());
                            let default = match &ret_ty {
                                MirType::Int(_, _) => MirValue::Const(MirConst::Int(0, ret_ty)),
                                MirType::Float(_) => MirValue::Const(MirConst::Float(0.0, ret_ty)),
                                _ => MirValue::Const(MirConst::Int(0, MirType::i32())),
                            };
                            builder.assign(ret_local, MirRValue::Use(default));
                            builder.ret(Some(MirValue::Local(ret_local)));
                        } else {
                            builder.ret(Some(val));
                        }
                    } else {
                        builder.ret(Some(val));
                    }
                }
            } else {
                builder.ret_void();
            }

            let mut func = builder.build();
            if is_main {
                func.linkage = Linkage::External; // main must not be static
            }
            func.is_public = is_main || self.current_item_vis.unwrap_or(true);

            // Set shader stage from attributes (#[vertex], #[fragment], #[compute])
            func.shader_stage = Self::extract_shader_stage(attrs);
            if func.shader_stage.is_some() {
                func.is_public = true; // shader entry points must be public
            }

            self.module.add_function(func);

            // Restore the enclosing function context (if any).
            self.current_fn = saved_fn;
            self.var_map = saved_vars;
        } else {
            // Declaration only
            self.module.declare_function(f.name.name.clone(), sig);
        }

        Ok(())
    }

    /// Lower all methods in an impl block as free functions with mangled names.
    fn lower_impl(&mut self, impl_def: &ast::ImplDef) -> CodegenResult<()> {
        let type_name = self.resolve_type_name(&impl_def.self_ty);

        // Skip generic impls — they are lowered when the type is monomorphized
        let has_impl_generics = impl_def
            .generics
            .params
            .iter()
            .any(|p| matches!(p.kind, ast::GenericParamKind::Type { .. }));
        let is_generic_type = self.generic_enums.contains_key(type_name.as_ref())
            || self.generic_structs.contains_key(type_name.as_ref());

        if has_impl_generics || is_generic_type {
            return Ok(());
        }

        // Set current impl type so Self resolves correctly in type lowering
        self.current_impl_type = Some(type_name.clone());

        // PASS 1: Forward-declare ALL method signatures before lowering bodies.
        // This fixes forward references: method A can call method B even if B
        // is defined after A in the source, because B's signature is already known.
        for impl_item in &impl_def.items {
            if let ast::ImplItemKind::Function(f) = &impl_item.kind {
                self.declare_impl_method(&type_name, f)?;
            }
        }

        // PASS 2: Lower method bodies (signatures already declared).
        for impl_item in &impl_def.items {
            if let ast::ImplItemKind::Function(f) = &impl_item.kind {
                self.lower_impl_method(&type_name, f)?;
            }
        }

        self.current_impl_type = None;
        Ok(())
    }

    /// Forward-declare an impl method's signature without lowering the body.
    /// This enables forward references within the same impl block.
    fn declare_impl_method(&mut self, type_name: &Arc<str>, f: &ast::FnDef) -> CodegenResult<()> {
        let mangled_name: Arc<str> = Arc::from(format!("{}_{}", type_name, f.name.name));

        // Build parameter types
        let mut params = Vec::new();
        for param in &f.sig.params {
            let is_self = matches!(
                &param.pattern.kind,
                ast::PatternKind::Ident { name, .. } if name.name.as_ref() == "self"
            );
            if is_self {
                if matches!(param.ty.kind, ast::TypeKind::Ref { .. }) {
                    params.push(MirType::Ptr(Box::new(MirType::Struct(type_name.clone()))));
                } else {
                    params.push(MirType::Struct(type_name.clone()));
                }
            } else {
                params.push(self.lower_type_from_ast(&param.ty));
            }
        }

        let ret = f
            .sig
            .return_ty
            .as_ref()
            .map(|t| self.lower_type_from_ast(t))
            .unwrap_or(MirType::Void);

        let sig = MirFnSig::new(params, ret);
        self.module.declare_function(mangled_name, sig);
        Ok(())
    }

    /// Lower a single impl method as a free function.
    /// The method `fn area(self) -> f64` on type `Rectangle` becomes
    /// `fn Rectangle_area(self: Rectangle) -> f64`.
    fn lower_impl_method(&mut self, type_name: &Arc<str>, f: &ast::FnDef) -> CodegenResult<()> {
        let mangled_name: Arc<str> = Arc::from(format!("{}_{}", type_name, f.name.name));

        // Build parameter types: `self` becomes the struct type, `&self`/`&mut self`
        // becomes a pointer to the struct type, others are lowered normally.
        let mut params = Vec::new();
        for param in &f.sig.params {
            let is_self = matches!(&param.pattern.kind,
                ast::PatternKind::Ident { name, .. } if name.name.as_ref() == "self"
            );
            if is_self {
                // Check if the type is a reference (&Self or &mut Self)
                if matches!(param.ty.kind, ast::TypeKind::Ref { .. }) {
                    params.push(MirType::Ptr(Box::new(MirType::Struct(type_name.clone()))));
                } else {
                    params.push(MirType::Struct(type_name.clone()));
                }
            } else {
                params.push(self.lower_type_from_ast(&param.ty));
            }
        }

        let ret = f
            .sig
            .return_ty
            .as_ref()
            .map(|t| self.lower_type_from_ast(t))
            .unwrap_or(MirType::Void);

        let sig = MirFnSig::new(params, ret);

        if let Some(body) = &f.body {
            // Save enclosing function context for nested method definitions.
            let saved_fn = self.current_fn.take();
            let saved_vars = std::mem::take(&mut self.var_map);

            let mut builder = MirBuilder::new(mangled_name.clone(), sig);

            // Map parameters (including self) to locals
            for (i, param) in f.sig.params.iter().enumerate() {
                if let ast::PatternKind::Ident { name, .. } = &param.pattern.kind {
                    let local_id = builder.param_local(i);
                    builder.set_param_name(i, name.name.clone());
                    self.var_map.insert(name.name.clone(), local_id);
                }
            }

            self.current_fn = Some(builder);

            let result = self.lower_block(body)?;

            let mut builder = self.current_fn.take().unwrap();
            if f.sig.return_ty.is_some() {
                if let Some(val) = result {
                    builder.ret(Some(val));
                }
            } else {
                builder.ret_void();
            }

            let mut func = builder.build();
            func.is_public = true;
            self.module.add_function(func);

            // Restore enclosing function context.
            self.current_fn = saved_fn;
            self.var_map = saved_vars;
        }

        Ok(())
    }

    /// Lower a monomorphized generic impl method. Unlike lower_impl_method,
    /// this uses the function name directly (already mangled) and the self type
    /// is the already-mangled type name.
    fn lower_generic_impl_method(
        &mut self,
        type_name: &Arc<str>,
        f: &ast::FnDef,
    ) -> CodegenResult<()> {
        // The function name is already the final mangled name (e.g., Option_i32_unwrap_or)
        let fn_name = f.name.name.clone();

        let mut params = Vec::new();
        for param in &f.sig.params {
            let is_self = matches!(&param.pattern.kind,
                ast::PatternKind::Ident { name, .. } if name.name.as_ref() == "self"
            );
            if is_self {
                if matches!(param.ty.kind, ast::TypeKind::Ref { .. }) {
                    params.push(MirType::Ptr(Box::new(MirType::Struct(type_name.clone()))));
                } else {
                    params.push(MirType::Struct(type_name.clone()));
                }
            } else {
                params.push(self.lower_type_from_ast(&param.ty));
            }
        }

        let ret = f
            .sig
            .return_ty
            .as_ref()
            .map(|t| self.lower_type_from_ast(t))
            .unwrap_or(MirType::Void);

        let sig = MirFnSig::new(params, ret);

        if let Some(body) = &f.body {
            // Save enclosing function context for nested definitions.
            let saved_fn = self.current_fn.take();
            let saved_vars = std::mem::take(&mut self.var_map);

            let mut builder = MirBuilder::new(fn_name.clone(), sig);

            for (i, param) in f.sig.params.iter().enumerate() {
                if let ast::PatternKind::Ident { name, .. } = &param.pattern.kind {
                    let local_id = builder.param_local(i);
                    builder.set_param_name(i, name.name.clone());
                    self.var_map.insert(name.name.clone(), local_id);
                }
            }

            self.current_fn = Some(builder);
            let result = self.lower_block(body)?;

            let mut builder = self.current_fn.take().unwrap();
            if f.sig.return_ty.is_some() {
                if let Some(val) = result {
                    builder.ret(Some(val));
                }
            } else {
                builder.ret_void();
            }

            let mut func = builder.build();
            func.is_public = true;
            self.module.add_function(func);

            // Restore enclosing function context.
            self.current_fn = saved_fn;
            self.var_map = saved_vars;
        }

        Ok(())
    }

    /// Generate methods from #[derive(...)] attributes on a struct.
    fn generate_derive_methods(
        &mut self,
        name: &ast::Ident,
        _attrs: &[ast::Attribute],
    ) -> CodegenResult<()> {
        let type_name = name.name.clone();
        let clone_key = (type_name.clone(), Arc::from("clone"));

        // Only generate if the clone method was registered during collection
        if self.impl_methods.contains_key(&clone_key) {
            let fn_name = self.impl_methods.get(&clone_key).unwrap().clone();

            // Skip if already generated
            if self.monomorphized.contains(&fn_name) {
                return Ok(());
            }
            self.monomorphized.insert(fn_name.clone());

            // Generate: fn TypeName_clone(self: TypeName) -> TypeName { return self; }
            let struct_ty = MirType::Struct(type_name.clone());
            let sig = MirFnSig::new(vec![struct_ty.clone()], struct_ty.clone());
            let mut builder = MirBuilder::new(fn_name, sig);

            let self_local = builder.param_local(0);
            builder.set_param_name(0, Arc::from("self"));
            builder.ret(Some(values::local(self_local)));

            let mut func = builder.build();
            func.is_public = true;
            self.module.add_function(func);
        }

        Ok(())
    }

    fn lower_static(&mut self, s: &ast::StaticDef) -> CodegenResult<()> {
        let ty = self.lower_type_from_ast(&s.ty);
        let init = s.value.as_ref().and_then(|e| self.try_const_eval(e));

        let mut global = MirGlobal::new(s.name.name.clone(), ty);
        global.init = init;
        global.is_mut = s.mutability.is_mut();

        self.module.add_global(global);
        Ok(())
    }

    fn lower_const(&mut self, c: &ast::ConstDef) -> CodegenResult<()> {
        let ty = self.lower_type_from_ast(&c.ty);
        let init = c.value.as_ref().and_then(|e| self.try_const_eval(e));

        let mut global = MirGlobal::new(c.name.name.clone(), ty);
        global.init = init;
        global.is_mut = false;

        self.module.add_global(global);
        Ok(())
    }

    // =========================================================================
    // TYPE INFERENCE HELPERS
    // =========================================================================

    /// Infer the MIR type of a value based on its representation.
    /// When the AST does not carry resolved type annotations into the lowering
    /// phase, we derive a best-effort type from the value itself.
    fn type_of_value(&self, val: &MirValue) -> MirType {
        match val {
            MirValue::Const(c) => match c {
                MirConst::Bool(_) => MirType::Bool,
                MirConst::Int(_, ty) => ty.clone(),
                MirConst::Uint(_, ty) => ty.clone(),
                MirConst::Float(_, ty) => ty.clone(),
                MirConst::Str(_) => MirType::Ptr(Box::new(MirType::i8())),
                MirConst::ByteStr(_) => MirType::Ptr(Box::new(MirType::u8())),
                MirConst::Null(ty) => MirType::Ptr(Box::new(ty.clone())),
                MirConst::Unit => MirType::Void,
                MirConst::Zeroed(ty) => ty.clone(),
                MirConst::Undef(ty) => ty.clone(),
                MirConst::Struct(name, _) => MirType::Struct(name.clone()),
            },
            MirValue::Local(id) => {
                // Look up the local's declared type in the current function builder
                if let Some(ref builder) = self.current_fn {
                    if let Some(local) = builder.local_type(*id) {
                        return local;
                    }
                }
                MirType::i32()
            }
            MirValue::Global(name) => {
                // Look up the global's type from the module
                if let Some(global) = self.module.find_global(name) {
                    global.ty.clone()
                } else {
                    MirType::i32()
                }
            }
            MirValue::Function(_) => MirType::i32(),
        }
    }

    /// Determine the result type of a binary operation given its operator and
    /// left operand type.  Comparisons always produce Bool; arithmetic and
    /// bitwise ops propagate the operand type.
    fn binary_result_type(&self, op: BinOp, left_val: &MirValue) -> MirType {
        match op {
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => MirType::Bool,
            _ => self.type_of_value(left_val),
        }
    }

    /// Look up a struct field type by struct name and field name.
    fn lookup_struct_field_type(&self, struct_name: &str, field_name: &str) -> Option<MirType> {
        if let Some(type_def) = self.module.find_type(struct_name) {
            if let TypeDefKind::Struct { fields, .. } = &type_def.kind {
                for (fname, fty) in fields {
                    if let Some(name) = fname {
                        if name.as_ref() == field_name {
                            return Some(fty.clone());
                        }
                    }
                }
            }
        }
        None
    }

    /// Look up an enum variant by enum name and variant name.
    /// Returns (discriminant, variant_fields) if found.
    fn lookup_enum_variant(
        &self,
        enum_name: &str,
        variant_name: &str,
    ) -> Option<(i128, Vec<(Option<Arc<str>>, MirType)>)> {
        if let Some(type_def) = self.module.find_type(enum_name) {
            if let TypeDefKind::Enum { variants, .. } = &type_def.kind {
                for v in variants {
                    if v.name.as_ref() == variant_name {
                        return Some((v.discriminant, v.fields.clone()));
                    }
                }
            }
        }
        None
    }

    /// Check if a name refers to a known enum type.
    fn is_enum_type(&self, name: &str) -> bool {
        if let Some(type_def) = self.module.find_type(name) {
            matches!(type_def.kind, TypeDefKind::Enum { .. })
        } else {
            false
        }
    }

    /// Find the most recently monomorphized specialization of a generic enum.
    /// E.g., if "Option" has been monomorphized as "Option_T_i32", return that.
    fn find_monomorphized_enum(&self, base_name: &str) -> Option<Arc<str>> {
        let prefix = format!("{}_", base_name);
        // Search only registered enum types, not all monomorphized names
        // (which also includes functions like Option_i32_unwrap_or)
        self.monomorphized
            .iter()
            .find(|name| name.starts_with(&prefix) && self.is_enum_type(name))
            .cloned()
    }
}

/// Determine item visibility.  The AST currently does not carry a
/// visibility modifier on the Ident node, so all items are assumed
/// public.  When the parser / AST is extended with `pub` tracking,
/// this function should inspect the actual modifier.
fn item_visibility(_name: &ast::Ident) -> Option<bool> {
    Some(true)
}
