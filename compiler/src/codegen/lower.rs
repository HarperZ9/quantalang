// ===============================================================================
// QUANTALANG CODE GENERATOR - AST TO MIR LOWERING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Lowering from AST to MIR.
//!
//! This pass transforms the type-checked AST into MIR (Mid-level IR).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::ast::{self, ExprKind, StmtKind, ItemKind, Literal, BinOp as AstBinOp, UnaryOp as AstUnaryOp};
use crate::types::TypeContext;

use super::ir::*;
use super::builder::{MirBuilder, MirModuleBuilder, values};
use super::backend::{CodegenError, CodegenResult};

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
        }
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
            _ => Ok(()),
        }
    }

    /// Generate vtable definitions for all (Trait, Type) implementation pairs.
    fn generate_vtables(&mut self) {
        // Copy trait_methods to the MirModule for the C backend to access
        let trait_methods_clone = self.trait_methods.clone();
        for (trait_name, methods) in &trait_methods_clone {
            self.module.module_mut().trait_methods.insert(trait_name.clone(), methods.clone());
        }

        // For each (Trait, Type) impl pair, create a vtable
        let impl_pairs: Vec<_> = self.trait_impls.keys().cloned().collect();
        for (trait_name, type_name) in impl_pairs {
            if let Some(trait_methods) = self.trait_methods.get(&trait_name) {
                let methods: Vec<_> = trait_methods.iter().map(|(method_name, sig)| {
                    let mangled = self.impl_methods
                        .get(&(type_name.clone(), method_name.clone()))
                        .cloned()
                        .unwrap_or_else(|| Arc::from(format!("{}_{}", type_name, method_name)));
                    (method_name.clone(), mangled, sig.clone())
                }).collect();

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
                let params: Vec<MirType> = fndef.sig.params.iter().map(|p| {
                    self.lower_type_from_ast(&p.ty)
                }).collect();
                let ret = fndef.sig.return_ty.as_ref()
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
        let ops: Vec<(Arc<str>, Vec<MirType>)> = e.operations.iter().map(|op| {
            let param_types: Vec<MirType> = op.params.iter()
                .map(|p| self.lower_type_from_ast(&p.ty))
                .collect();
            (Arc::from(op.name.name.as_ref()), param_types)
        }).collect();
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
                let params: Vec<MirType> = f.sig.params.iter().map(|p| {
                    self.lower_ffi_type(&p.ty)
                }).collect();

                let ret = f.sig.return_ty.as_ref()
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
        let has_generics = s.generics.params.iter().any(|p| {
            matches!(p.kind, ast::GenericParamKind::Type { .. })
        });

        if has_generics {
            self.generic_structs.insert(s.name.name.clone(), s.clone());
            return Ok(());
        }

        let fields = match &s.fields {
            ast::StructFields::Named(fields) => {
                fields.iter().map(|f| {
                    let ty = self.lower_type_from_ast(&f.ty);
                    (Some(f.name.name.clone()), ty)
                }).collect()
            }
            ast::StructFields::Tuple(fields) => {
                fields.iter().map(|f| {
                    let ty = self.lower_type_from_ast(&f.ty);
                    (None, ty)
                }).collect()
            }
            ast::StructFields::Unit => Vec::new(),
        };

        self.module.create_struct(s.name.name.clone(), fields);
        Ok(())
    }

    /// Process #[derive(...)] attributes on a struct and register auto-generated methods.
    fn process_derive_attrs(&mut self, attrs: &[ast::Attribute], name: &ast::Ident, _s: &ast::StructDef) {
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
                        self.impl_methods.insert(
                            (type_name.clone(), Arc::from("clone")),
                            method_name.clone(),
                        );
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
        let has_generics = e.generics.params.iter().any(|p| {
            matches!(p.kind, ast::GenericParamKind::Type { .. })
        });

        if has_generics {
            self.generic_enums.insert(e.name.name.clone(), e.clone());
            return Ok(());
        }

        let variants: Vec<_> = e.variants.iter().enumerate().map(|(i, v)| {
            let fields = match &v.fields {
                ast::StructFields::Named(fields) => {
                    fields.iter().map(|f| {
                        (Some(f.name.name.clone()), self.lower_type_from_ast(&f.ty))
                    }).collect()
                }
                ast::StructFields::Tuple(fields) => {
                    fields.iter().map(|f| {
                        (None, self.lower_type_from_ast(&f.ty))
                    }).collect()
                }
                ast::StructFields::Unit => Vec::new(),
            };

            MirEnumVariant {
                name: v.name.name.clone(),
                discriminant: i as i128,
                fields,
            }
        }).collect();

        self.module.create_enum(e.name.name.clone(), MirType::i32(), variants);
        Ok(())
    }

    /// Monomorphize a generic enum for a specific concrete type (single-param shorthand).
    /// E.g., `Option<T>` + `i32` → `Option_i32` with `Some(i32)` variant.
    fn monomorphize_enum(&mut self, enum_name: &str, concrete_ty: &MirType) -> CodegenResult<Arc<str>> {
        // Build single-param substitution map and delegate to multi-param version
        let enum_def = self.generic_enums.get(enum_name).cloned();
        if let Some(ref e) = enum_def {
            let type_param = e.generics.params.iter()
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
    fn monomorphize_enum_multi(&mut self, enum_name: &str, subst: &HashMap<Arc<str>, MirType>) -> CodegenResult<Arc<str>> {
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
        let variants: Vec<_> = enum_def.variants.iter().enumerate().map(|(i, v)| {
            let fields = match &v.fields {
                ast::StructFields::Named(fields) => {
                    fields.iter().map(|f| {
                        let ty = self.substitute_type_from_ast(&f.ty, subst);
                        (Some(f.name.name.clone()), ty)
                    }).collect()
                }
                ast::StructFields::Tuple(fields) => {
                    fields.iter().map(|f| {
                        let ty = self.substitute_type_from_ast(&f.ty, subst);
                        (None, ty)
                    }).collect()
                }
                ast::StructFields::Unit => Vec::new(),
            };

            MirEnumVariant {
                name: v.name.name.clone(),
                discriminant: i as i128,
                fields,
            }
        }).collect();

        self.module.create_enum(mangled_name.clone(), MirType::i32(), variants);

        // Also monomorphize any impl blocks for this generic enum
        self.monomorphize_impl_methods(enum_name, &mangled_name, subst)?;

        Ok(mangled_name)
    }

    /// Monomorphize a generic struct for specific concrete types.
    /// E.g., `Pair<T> { first: T, second: T }` + `{T: i32}` → `Pair_i32`.
    fn monomorphize_struct(&mut self, struct_name: &str, subst: &HashMap<Arc<str>, MirType>) -> CodegenResult<Arc<str>> {
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
            ast::StructFields::Named(fields) => {
                fields.iter().map(|f| {
                    let ty = self.substitute_type_from_ast(&f.ty, subst);
                    (Some(f.name.name.clone()), ty)
                }).collect()
            }
            ast::StructFields::Tuple(fields) => {
                fields.iter().map(|f| {
                    let ty = self.substitute_type_from_ast(&f.ty, subst);
                    (None, ty)
                }).collect()
            }
            ast::StructFields::Unit => Vec::new(),
        };

        self.module.create_struct(mangled_name.clone(), fields);

        // Also monomorphize any impl blocks for this generic struct
        self.monomorphize_impl_methods(struct_name, &mangled_name, subst)?;

        Ok(mangled_name)
    }

    /// Resolve an AST type using a substitution map for generic type parameters.
    /// Falls back to `lower_type_from_ast` for non-generic types.
    fn substitute_type_from_ast(&self, ty: &ast::Type, subst: &HashMap<Arc<str>, MirType>) -> MirType {
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
                                ident.name.as_ref(), generic_args, subst,
                            );
                            if !inner_subst.is_empty() {
                                let mangled = Self::mangle_generic_name(ident.name.as_ref(), &inner_subst);
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
                let length = self.try_const_eval(len)
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
            enum_def.generics.params.iter()
                .filter_map(|p| match &p.kind {
                    ast::GenericParamKind::Type { .. } => Some(p.ident.name.clone()),
                    _ => None,
                })
                .collect()
        } else if let Some(struct_def) = self.generic_structs.get(type_name) {
            struct_def.generics.params.iter()
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
                    result.insert(param_name.clone(), self.substitute_type_from_ast(arg_ty, outer_subst));
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
        entries.sort_by_key(|(k, _)| k.clone());
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
                    let mangled_fn_name: Arc<str> = Arc::from(
                        format!("{}_{}", mangled_type_name, method_name)
                    );

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
            self.generic_functions.insert(f.name.name.clone(), f.clone());
            return Ok(());
        }

        // Register a forward declaration so that resolve_call_return_type
        // can find the function's return type even before its body is lowered.
        // Skip `main` since its return type is special-cased during lowering.
        if f.name.name.as_ref() != "main" {
            let params: Vec<MirType> = f.sig.params.iter().map(|p| {
                self.lower_type_from_ast(&p.ty)
            }).collect();
            let ret = f.sig.return_ty.as_ref()
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
        let has_impl_generics = impl_def.generics.params.iter().any(|p| {
            matches!(p.kind, ast::GenericParamKind::Type { .. })
        });
        let is_generic_type = self.generic_enums.contains_key(type_name.as_ref())
            || self.generic_structs.contains_key(type_name.as_ref());

        if has_impl_generics || is_generic_type {
            self.generic_impls.entry(type_name.clone())
                .or_insert_with(Vec::new)
                .push(impl_def.clone());
            return Ok(());
        }

        // Track trait implementations for vtable generation
        let trait_name = impl_def.trait_ref.as_ref().map(|tr| {
            tr.path.last_ident().map(|i| i.name.clone()).unwrap_or(Arc::from("Unknown"))
        });

        let mut impl_method_names = Vec::new();

        for impl_item in &impl_def.items {
            if let ast::ImplItemKind::Function(f) = &impl_item.kind {
                let method_name = f.name.name.clone();
                let mangled: Arc<str> = Arc::from(format!("{}_{}", type_name, method_name));
                self.impl_methods.insert(
                    (type_name.clone(), method_name.clone()),
                    mangled.clone(),
                );
                impl_method_names.push(mangled);
            }
        }

        // Register trait impl for vtable generation
        if let Some(ref tname) = trait_name {
            self.trait_impls.insert(
                (tname.clone(), type_name.clone()),
                impl_method_names,
            );
        }

        Ok(())
    }

    /// Extract a type name from an AST Type node (used for impl block self_ty).
    fn resolve_type_name(&self, ty: &ast::Type) -> Arc<str> {
        match &ty.kind {
            ast::TypeKind::Path(path) => {
                path.last_ident()
                    .map(|i| i.name.clone())
                    .unwrap_or(Arc::from("Unknown"))
            }
            _ => Arc::from("Unknown"),
        }
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
            _ => Ok(()),
        }
    }

    /// Check if any attribute has the given name.
    fn has_attribute(attrs: &[ast::Attribute], name: &str) -> bool {
        attrs.iter().any(|attr| {
            attr.path.segments.first()
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
                                            if let ast::TokenTree::Token(ref num_tok) = tokens[i + 2] {
                                                if let crate::lexer::TokenKind::Literal { .. } = &num_tok.kind {
                                                    // Extract numeric value from source via span
                                                    if let Some(ref src) = self.source {
                                                        let start = num_tok.span.start.to_usize();
                                                        let end = num_tok.span.end.to_usize();
                                                        if end <= src.len() {
                                                            if let Ok(n) = src[start..end].parse::<u32>() {
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
    fn lower_function_with_vis(&mut self, f: &ast::FnDef, attrs: &[ast::Attribute], vis: &ast::Visibility) -> CodegenResult<()> {
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
        let params: Vec<_> = f.sig.params.iter().map(|p| {
            self.lower_type_from_ast(&p.ty)
        }).collect();

        let is_main = f.name.name.as_ref() == "main";
        let has_shader_stage = Self::extract_shader_stage(attrs).is_some();

        let ret = if is_main && f.sig.return_ty.is_none() && !has_shader_stage {
            // C requires main to return int (but not shader main)
            MirType::i32()
        } else {
            f.sig.return_ty.as_ref()
                .map(|t| self.lower_type_from_ast(t))
                .unwrap_or(MirType::Void)
        };

        let sig = MirFnSig::new(params, ret);

        if let Some(body) = &f.body {
            // Create function builder
            let mut builder = MirBuilder::new(f.name.name.clone(), sig);
            self.var_map.clear();

            // Map parameters to locals and set their names
            for (i, param) in f.sig.params.iter().enumerate() {
                if let ast::PatternKind::Ident { name, .. } = &param.pattern.kind {
                    let local_id = builder.param_local(i);
                    builder.set_param_name(i, name.name.clone());
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
                    builder.ret(Some(val));
                }
            } else {
                builder.ret_void();
            }

            let mut func = builder.build();
            if is_main {
                func.linkage = Linkage::External;  // main must not be static
            }
            func.is_public = is_main || self.current_item_vis.unwrap_or(true);

            // Set shader stage from attributes (#[vertex], #[fragment], #[compute])
            func.shader_stage = Self::extract_shader_stage(attrs);
            if func.shader_stage.is_some() {
                func.is_public = true; // shader entry points must be public
            }

            self.module.add_function(func);
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
        let has_impl_generics = impl_def.generics.params.iter().any(|p| {
            matches!(p.kind, ast::GenericParamKind::Type { .. })
        });
        let is_generic_type = self.generic_enums.contains_key(type_name.as_ref())
            || self.generic_structs.contains_key(type_name.as_ref());

        if has_impl_generics || is_generic_type {
            return Ok(());
        }

        for impl_item in &impl_def.items {
            if let ast::ImplItemKind::Function(f) = &impl_item.kind {
                self.lower_impl_method(&type_name, f)?;
            }
        }
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

        let ret = f.sig.return_ty.as_ref()
            .map(|t| self.lower_type_from_ast(t))
            .unwrap_or(MirType::Void);

        let sig = MirFnSig::new(params, ret);

        if let Some(body) = &f.body {
            let mut builder = MirBuilder::new(mangled_name.clone(), sig);
            self.var_map.clear();

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
        }

        Ok(())
    }

    /// Lower a monomorphized generic impl method. Unlike lower_impl_method,
    /// this uses the function name directly (already mangled) and the self type
    /// is the already-mangled type name.
    fn lower_generic_impl_method(&mut self, type_name: &Arc<str>, f: &ast::FnDef) -> CodegenResult<()> {
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

        let ret = f.sig.return_ty.as_ref()
            .map(|t| self.lower_type_from_ast(t))
            .unwrap_or(MirType::Void);

        let sig = MirFnSig::new(params, ret);

        if let Some(body) = &f.body {
            let mut builder = MirBuilder::new(fn_name.clone(), sig);
            self.var_map.clear();

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
        }

        Ok(())
    }

    /// Generate methods from #[derive(...)] attributes on a struct.
    fn generate_derive_methods(&mut self, name: &ast::Ident, attrs: &[ast::Attribute]) -> CodegenResult<()> {
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
            MirValue::Global(_) | MirValue::Function(_) => MirType::i32(),
        }
    }

    /// Determine the result type of a binary operation given its operator and
    /// left operand type.  Comparisons always produce Bool; arithmetic and
    /// bitwise ops propagate the operand type.
    fn binary_result_type(&self, op: BinOp, left_val: &MirValue) -> MirType {
        match op {
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                MirType::Bool
            }
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
    fn lookup_enum_variant(&self, enum_name: &str, variant_name: &str) -> Option<(i128, Vec<(Option<Arc<str>>, MirType)>)> {
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
        self.monomorphized.iter()
            .find(|name| name.starts_with(&prefix) && self.is_enum_type(name))
            .cloned()
    }

    // =========================================================================
    // BLOCK AND STATEMENT LOWERING
    // =========================================================================

    fn lower_block(&mut self, block: &ast::Block) -> CodegenResult<Option<MirValue>> {
        let mut result = None;

        for (i, stmt) in block.stmts.iter().enumerate() {
            let is_last = i == block.stmts.len() - 1;
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
                let macro_name = path.segments.last()
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
                                let local_id = builder.create_named_local(name.name.clone(), elem_ty);
                                builder.assign(local_id, MirRValue::Use(val));
                                self.var_map.insert(name.name.clone(), local_id);
                            }
                            // Wildcard patterns in tuple destructuring are silently ignored.
                        }
                        return Ok(());
                    }
                }

                // Fallback for non-literal tuples: lower the init expression
                // once, then extract fields via FieldAccess with `f0`, `f1`, etc.
                let init_v = self.lower_expr(&init.expr)?;
                for (i, pat) in patterns.iter().enumerate() {
                    if let ast::PatternKind::Ident { name, .. } = &pat.kind {
                        let elem_ty = MirType::i32();
                        let builder = self.current_fn.as_mut().ok_or_else(|| {
                            CodegenError::Internal("No current function".to_string())
                        })?;
                        let local_id = builder.create_named_local(name.name.clone(), elem_ty.clone());
                        let field_name: Arc<str> = Arc::from(format!("f{}", i));
                        builder.assign(local_id, MirRValue::FieldAccess {
                            base: init_v.clone(),
                            field_name,
                            field_ty: elem_ty,
                        });
                        self.var_map.insert(name.name.clone(), local_id);
                    }
                }
                return Ok(());
            }
            return Ok(());
        }

        // Compute type from annotation if present
        let explicit_ty = local.ty.as_ref()
            .map(|t| self.lower_type_from_ast(t));

        // Compute init value if present
        let init_val = if let Some(init) = &local.init {
            Some(self.lower_expr(&init.expr)?)
        } else {
            None
        };

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
            if let (Some(ref exp_ty), MirValue::Const(MirConst::Float(v, _))) = (&explicit_ty, &val) {
                if matches!(exp_ty, MirType::Float(FloatSize::F32)) {
                    return MirValue::Const(MirConst::Float(*v, MirType::f32()));
                }
            }
            val
        });

        // Now borrow current_fn and use it
        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

        // Create local for the binding
        if let ast::PatternKind::Ident { name, .. } = &local.pattern.kind {
            let local_id = builder.create_named_local(name.name.clone(), ty.clone());
            self.var_map.insert(name.name.clone(), local_id);

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

    fn lower_expr(&mut self, expr: &ast::Expr) -> CodegenResult<MirValue> {
        match &expr.kind {
            ExprKind::Literal(lit) => self.lower_literal(lit),
            ExprKind::Ident(ident) => self.lower_ident(ident),
            ExprKind::Path(path) => self.lower_path(path),

            ExprKind::Binary { op, left, right } => {
                self.lower_binary(*op, left, right)
            }
            ExprKind::Unary { op, expr: inner } => {
                self.lower_unary(*op, inner)
            }
            ExprKind::Assign { op, target, value } => {
                self.lower_assign(*op, target, value)
            }

            ExprKind::Call { func, args } => {
                self.lower_call(func, args)
            }
            ExprKind::MethodCall { receiver, method, args, .. } => {
                self.lower_method_call(receiver, method, args)
            }

            ExprKind::If { condition, then_branch, else_branch } => {
                self.lower_if(condition, then_branch, else_branch.as_deref())
            }
            ExprKind::Match { scrutinee, arms } => {
                self.lower_match(scrutinee, arms)
            }

            ExprKind::Loop { body, label } => {
                self.lower_loop(body, label.as_ref())
            }
            ExprKind::While { condition, body, label } => {
                self.lower_while(condition, body, label.as_ref())
            }
            ExprKind::For { pattern, iter, body, label } => {
                self.lower_for(pattern, iter, body, label.as_ref())
            }

            ExprKind::Block(block) => {
                let result = self.lower_block(block)?;
                Ok(result.unwrap_or(values::unit()))
            }

            ExprKind::Return(value) => {
                self.lower_return(value.as_deref())
            }
            ExprKind::Break { value, label } => {
                self.lower_break(value.as_deref(), label.as_ref())
            }
            ExprKind::Continue { label } => {
                self.lower_continue(label.as_ref())
            }

            ExprKind::Tuple(elems) => self.lower_tuple(elems),
            ExprKind::Array(elems) => self.lower_array(elems),
            ExprKind::Index { expr: arr, index } => {
                self.lower_index(arr, index)
            }
            ExprKind::Field { expr: obj, field } => {
                self.lower_field(obj, field)
            }

            ExprKind::Ref { mutability, expr: inner } => {
                self.lower_ref(*mutability, inner)
            }
            ExprKind::Deref(inner) => {
                self.lower_deref(inner)
            }

            ExprKind::Cast { expr: inner, ty } => {
                self.lower_cast(inner, ty)
            }

            ExprKind::Paren(inner) => self.lower_expr(inner),

            ExprKind::Closure { params, return_type, body, .. } => {
                self.lower_closure(params, return_type.as_deref(), body)
            }

            ExprKind::Struct { path, fields, rest } => {
                self.lower_struct_expr(path, fields, rest.as_deref())
            }

            ExprKind::Handle { effect, handlers, body } => {
                self.lower_handle(effect, handlers, body)
            }
            ExprKind::Resume(value) => {
                self.lower_resume(value.as_deref())
            }
            ExprKind::Perform { effect, operation, args } => {
                self.lower_perform(effect, operation, args)
            }

            ExprKind::Macro { path, tokens, .. } => {
                // Expand macro expressions (println!, print!, etc.)
                let macro_name = path.segments.last()
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
                        // format! returns a string - for now return a string constant
                        let s = self.extract_string_from_tokens(tokens);
                        Ok(MirValue::Const(MirConst::Str(
                            self.module.intern_string(s),
                        )))
                    }
                    _ => {
                        // Unknown macro expression - return unit
                        Ok(values::unit())
                    }
                }
            }

            ExprKind::Try(inner) => {
                self.lower_try(inner)
            }

            _ => {
                // Unsupported expression - return unit
                Ok(values::unit())
            }
        }
    }

    fn lower_literal(&mut self, lit: &Literal) -> CodegenResult<MirValue> {
        match lit {
            Literal::Int { value, suffix, .. } => {
                let (ty, signed) = suffix.as_ref().map(|s| match s {
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
                }).unwrap_or((MirType::i32(), true));

                if signed {
                    Ok(MirValue::Const(MirConst::Int(*value as i128, ty)))
                } else {
                    Ok(MirValue::Const(MirConst::Uint(*value as u128, ty)))
                }
            }
            Literal::Float { value, suffix } => {
                let ty = suffix.as_ref().map(|s| match s {
                    ast::FloatSuffix::F16 | ast::FloatSuffix::F32 => MirType::f32(),
                    ast::FloatSuffix::F64 => MirType::f64(),
                }).unwrap_or(MirType::f64());
                Ok(MirValue::Const(MirConst::Float(*value, ty)))
            }
            Literal::Bool(b) => Ok(values::bool(*b)),
            Literal::Char(c) => {
                Ok(MirValue::Const(MirConst::Uint(*c as u128, MirType::u32())))
            }
            Literal::Str { value, .. } => {
                let idx = self.module.intern_string(value.clone());
                // Wrap string literal in quanta_string_new() to produce a QuantaString
                let builder = self.current_fn.as_mut().ok_or_else(|| {
                    CodegenError::Internal("No current function for string literal".to_string())
                })?;
                let result = builder.create_local(MirType::Struct(Arc::from("QuantaString")));
                let cont = builder.create_block();
                let func = MirValue::Function(Arc::from("quanta_string_new"));
                let str_ptr = MirValue::Const(MirConst::Str(idx));
                builder.call(func, vec![str_ptr], Some(result), cont);
                builder.switch_to_block(cont);
                Ok(values::local(result))
            }
            Literal::ByteStr { value, .. } => {
                Ok(MirValue::Const(MirConst::ByteStr(value.clone())))
            }
            Literal::Byte(b) => {
                Ok(MirValue::Const(MirConst::Uint(*b as u128, MirType::u8())))
            }
        }
    }

    fn lower_ident(&mut self, ident: &ast::Ident) -> CodegenResult<MirValue> {
        if let Some(&local) = self.var_map.get(&ident.name) {
            Ok(values::local(local))
        } else {
            // Check for math constants
            match ident.name.as_ref() {
                "PI" => Ok(MirValue::Const(MirConst::Float(std::f64::consts::PI, MirType::f64()))),
                "E" => Ok(MirValue::Const(MirConst::Float(std::f64::consts::E, MirType::f64()))),
                "TAU" => Ok(MirValue::Const(MirConst::Float(std::f64::consts::TAU, MirType::f64()))),
                // Might be a global or function
                _ => Ok(values::global(ident.name.clone()))
            }
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
                let disc = self.lookup_enum_variant(&resolved_name, variant_name)
                    .map(|(d, _)| d)
                    .unwrap_or(0);

                let builder = self.current_fn.as_mut().ok_or_else(|| {
                    CodegenError::Internal("No current function".to_string())
                })?;

                let result = builder.create_local(MirType::Struct(resolved_name.clone()));
                builder.aggregate(
                    result,
                    AggregateKind::Variant(resolved_name.clone(), disc as u32, variant_name.clone()),
                    Vec::new(),
                );

                return Ok(values::local(result));
            }
        }

        // Complex path - treat as module-qualified reference.
        // Join segments with `_` so that `mod::func` resolves to the
        // mangled name `mod_func` produced by the module loader.
        let name = path.segments.iter()
            .map(|s| s.ident.name.as_ref())
            .collect::<Vec<_>>()
            .join("_");
        Ok(values::global(name))
    }

    fn lower_binary(&mut self, op: AstBinOp, left: &ast::Expr, right: &ast::Expr) -> CodegenResult<MirValue> {
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
                } else { v }
            };
            if is_f32(&lt) && !is_f32(&rt) {
                (left_val, coerce_to_f32(right_val))
            } else if !is_f32(&lt) && is_f32(&rt) {
                (coerce_to_f32(left_val), right_val)
            } else {
                (left_val, right_val)
            }
        };

        // Check if operands are strings (QuantaString) for special operator handling
        let left_ty = self.type_of_value(&left_val);
        let is_string_op = matches!(&left_ty, MirType::Struct(name) if name.as_ref() == "QuantaString");

        // String concatenation: `+` on QuantaString -> quanta_string_concat()
        if is_string_op && op == AstBinOp::Add {
            let builder = self.current_fn.as_mut().ok_or_else(|| {
                CodegenError::Internal("No current function".to_string())
            })?;
            let result = builder.create_local(MirType::Struct(Arc::from("QuantaString")));
            let cont = builder.create_block();
            let func = MirValue::Function(Arc::from("quanta_string_concat"));
            builder.call(func, vec![left_val, right_val], Some(result), cont);
            builder.switch_to_block(cont);
            return Ok(values::local(result));
        }

        // String comparison: `==` on QuantaString -> quanta_string_eq()
        if is_string_op && op == AstBinOp::Eq {
            let builder = self.current_fn.as_mut().ok_or_else(|| {
                CodegenError::Internal("No current function".to_string())
            })?;
            let result = builder.create_local(MirType::Bool);
            let cont = builder.create_block();
            let func = MirValue::Function(Arc::from("quanta_string_eq"));
            builder.call(func, vec![left_val, right_val], Some(result), cont);
            builder.switch_to_block(cont);
            return Ok(values::local(result));
        }

        // String inequality: `!=` on QuantaString -> !quanta_string_eq()
        if is_string_op && op == AstBinOp::Ne {
            let builder = self.current_fn.as_mut().ok_or_else(|| {
                CodegenError::Internal("No current function".to_string())
            })?;
            let eq_result = builder.create_local(MirType::Bool);
            let cont = builder.create_block();
            let func = MirValue::Function(Arc::from("quanta_string_eq"));
            builder.call(func, vec![left_val, right_val], Some(eq_result), cont);
            builder.switch_to_block(cont);
            // Negate the result: != is !eq
            let neg_result = builder.create_local(MirType::Bool);
            builder.unary_op(neg_result, UnaryOp::Not, values::local(eq_result));
            return Ok(values::local(neg_result));
        }

        // Vector arithmetic: +, -, * on quanta_vecN types
        if let MirType::Struct(ref name) = left_ty {
            if let Some(n) = Self::vec_component_count(name) {
                let op_suffix = match op {
                    AstBinOp::Add => Some("add"),
                    AstBinOp::Sub => Some("sub"),
                    AstBinOp::Mul => Some("mul"),
                    _ => None,
                };
                if let Some(suffix) = op_suffix {
                    let c_func = format!("quanta_vec{}_{}", n, suffix);
                    let builder = self.current_fn.as_mut().ok_or_else(|| {
                        CodegenError::Internal("No current function".to_string())
                    })?;
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
                if name.as_ref() == "quanta_mat4" {
                    let right_ty = self.type_of_value(&right_val);
                    let (c_func, ret_ty) = if let MirType::Struct(ref rname) = right_ty {
                        if rname.as_ref() == "quanta_mat4" {
                            ("quanta_mat4_mul", MirType::Struct(Arc::from("quanta_mat4")))
                        } else if rname.as_ref() == "quanta_vec4" {
                            ("quanta_mat4_mul_vec4", MirType::Struct(Arc::from("quanta_vec4")))
                        } else {
                            ("quanta_mat4_mul", left_ty.clone())
                        }
                    } else {
                        ("quanta_mat4_mul", left_ty.clone())
                    };
                    let builder = self.current_fn.as_mut().ok_or_else(|| {
                        CodegenError::Internal("No current function".to_string())
                    })?;
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
                AstBinOp::Eq  => Some("eq"),
                AstBinOp::Ne  => Some("ne"),
                AstBinOp::Lt  => Some("lt"),
                AstBinOp::Gt  => Some("gt"),
                AstBinOp::Le  => Some("le"),
                AstBinOp::Ge  => Some("ge"),
                _ => None,
            };
            if let Some(method) = method_name {
                let key = (type_name.clone(), Arc::from(method));
                if let Some(mangled_fn) = self.impl_methods.get(&key).cloned() {
                    let ret_ty = self.module.find_function(mangled_fn.as_ref())
                        .map(|f| f.sig.ret.clone())
                        .unwrap_or(left_ty.clone());
                    let builder = self.current_fn.as_mut().ok_or_else(|| {
                        CodegenError::Internal("No current function".to_string())
                    })?;
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
        let result_ty = self.binary_result_type(mir_op, &left_val);

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

        let result = builder.create_local(result_ty);
        builder.binary_op(result, mir_op, left_val, right_val);
        Ok(values::local(result))
    }

    fn lower_logical_op(&mut self, op: AstBinOp, left: &ast::Expr, right: &ast::Expr) -> CodegenResult<MirValue> {
        // Lower left expression FIRST before borrowing builder
        let left_val = self.lower_expr(left)?;

        // Now set up blocks and result
        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

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

        // Vector negation: -vec -> quanta_vecN_neg(vec)
        if matches!(op, AstUnaryOp::Neg) {
            let inner_ty = self.type_of_value(&inner_val);
            if let MirType::Struct(ref name) = inner_ty {
                if let Some(n) = Self::vec_component_count(name) {
                    let c_func = format!("quanta_vec{}_neg", n);
                    let builder = self.current_fn.as_mut().ok_or_else(|| {
                        CodegenError::Internal("No current function".to_string())
                    })?;
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
                let builder = self.current_fn.as_mut().ok_or_else(|| {
                    CodegenError::Internal("No current function".to_string())
                })?;
                let result = builder.create_local(pointee_ty.clone());
                builder.assign(result, MirRValue::Deref {
                    ptr: inner_val,
                    pointee_ty,
                });
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
                let builder = self.current_fn.as_mut().ok_or_else(|| {
                    CodegenError::Internal("No current function".to_string())
                })?;
                let result = builder.create_local(MirType::Ptr(Box::new(inner_ty)));
                builder.make_ref(result, is_mut, MirPlace::local(local));
                return Ok(values::local(result));
            }
        };

        // Compute result type before borrowing the builder mutably
        let result_ty = self.type_of_value(&inner_val);

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

        let result = builder.create_local(result_ty);
        builder.unary_op(result, mir_op, inner_val);
        Ok(values::local(result))
    }

    fn lower_assign(&mut self, op: ast::AssignOp, target: &ast::Expr, value: &ast::Expr) -> CodegenResult<MirValue> {
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

        // Handle field assignment through a pointer: `self.field = value` where self is &mut T
        if let ExprKind::Field { expr: obj, field } = &target.kind {
            let obj_val = self.lower_expr(obj)?;
            let obj_ty = self.type_of_value(&obj_val);

            if obj_ty.is_pointer() {
                // This is a pointer-to-struct field assignment: emit ptr->field = value
                let obj_local = match &obj_val {
                    MirValue::Local(id) => *id,
                    _ => {
                        let builder = self.current_fn.as_mut().unwrap();
                        let temp = builder.create_local(obj_ty);
                        builder.assign(temp, MirRValue::Use(obj_val));
                        temp
                    }
                };
                let builder = self.current_fn.as_mut().unwrap();
                builder.push_field_deref_assign(obj_local, field.name.clone(), MirRValue::Use(val));
                return Ok(values::unit());
            }
        }

        // Get the target local
        let target_local = match &target.kind {
            ExprKind::Ident(ident) => {
                self.var_map.get(&ident.name).copied()
            }
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
                // texture_sample(texture, sampler, uv) -> vec4
                "texture_sample" if args.len() == 3 => {
                    let tex = self.lower_expr(&args[0])?;
                    let samp = self.lower_expr(&args[1])?;
                    let coords = self.lower_expr(&args[2])?;
                    let result_ty = MirType::Struct(Arc::from("quanta_vec4"));
                    let builder = self.current_fn.as_mut().unwrap();
                    let dest = builder.create_local(result_ty);
                    builder.assign(dest, MirRValue::TextureSample {
                        texture: tex,
                        sampler: samp,
                        coords: coords,
                    });
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

        // Check for generic function call — monomorphize if needed.
        if let Some(fn_name) = self.extract_call_name(func) {
            if self.generic_functions.contains_key(fn_name) {
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

        // Check for math built-in functions and rewrite the call target to the
        // corresponding C / runtime function name — but only if there is no
        // user-defined function with the same name in the module.
        let func_val = if let Some(builtin_name) = self.try_resolve_math_builtin(func) {
            // Don't shadow a user-defined function with the same name.
            let fn_name = self.extract_call_name(func);
            let user_defined = fn_name
                .and_then(|n| self.module.find_function(n))
                .is_some();
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

        let mut arg_vals: Vec<_> = args.iter()
            .map(|a| self.lower_expr(a))
            .collect::<CodegenResult<_>>()?;

        // ---- Coerce QuantaString args to raw char* for FFI calls ----
        // When calling an extern "C" function that expects `const char*`
        // (`Ptr(i8)`), but the argument is a QuantaString (from a string
        // literal), extract the `.ptr` field automatically.
        if let Some(fn_name) = self.extract_call_name(func) {
            if let Some(target_fn) = self.module.find_function(fn_name) {
                if target_fn.is_declaration() && target_fn.sig.calling_conv == CallingConv::C {
                    let param_types: Vec<MirType> = target_fn.sig.params.clone();
                    for (i, arg_val) in arg_vals.iter_mut().enumerate() {
                        if i < param_types.len() {
                            if matches!(&param_types[i], MirType::Ptr(inner) if matches!(inner.as_ref(), MirType::Int(IntSize::I8, true))) {
                                let arg_ty = self.type_of_value(arg_val);
                                if let MirType::Struct(ref name) = arg_ty {
                                    if name.as_ref() == "QuantaString" {
                                        if let MirValue::Local(local_id) = arg_val {
                                            let builder = self.current_fn.as_mut().unwrap();
                                            let ptr_local = builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
                                            builder.assign(ptr_local, MirRValue::FieldAccess {
                                                base: MirValue::Local(*local_id),
                                                field_name: Arc::from("ptr"),
                                                field_ty: MirType::Ptr(Box::new(MirType::i8())),
                                            });
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

        // ---- Coerce QuantaString args to char* for builtin I/O calls ----
        // Runtime builtins like read_file, write_file, file_exists expect
        // const char* but the lowerer passes QuantaString values.
        if let Some(fn_name) = self.extract_call_name(func) {
            let needs_str_coerce = matches!(fn_name,
                "read_file" | "write_file" | "file_exists" |
                "quanta_vk_load_shader_file" | "quanta_vk_run_compute"
            );
            if needs_str_coerce {
                for arg_val in arg_vals.iter_mut() {
                    let arg_ty = self.type_of_value(arg_val);
                    if let MirType::Struct(ref name) = arg_ty {
                        if name.as_ref() == "QuantaString" {
                            if let MirValue::Local(local_id) = arg_val {
                                let builder = self.current_fn.as_mut().unwrap();
                                let ptr_local = builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
                                builder.assign(ptr_local, MirRValue::FieldAccess {
                                    base: MirValue::Local(*local_id),
                                    field_name: Arc::from("ptr"),
                                    field_ty: MirType::Ptr(Box::new(MirType::i8())),
                                });
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

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;
        let cont = builder.create_block();

        // If the function returns void, don't create a result local or
        // assign the call result — just emit a void call.
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

        super::runtime::math_builtin_to_c(name)
    }

    /// Lower a vector constructor call: `vec2(x,y)`, `vec3(x,y,z)`, `vec4(x,y,z,w)`.
    /// Generates a struct aggregate for the corresponding `quanta_vecN` type.
    fn lower_vec_constructor(&mut self, components: u32, args: &[ast::Expr]) -> CodegenResult<MirValue> {
        let struct_name = format!("quanta_vec{}", components);
        let mut operands = Vec::new();
        for arg in args {
            operands.push(self.lower_expr(arg)?);
        }
        let ty = MirType::Struct(Arc::from(struct_name.as_str()));
        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;
        let result = builder.create_local(ty.clone());
        builder.assign(result, MirRValue::Aggregate {
            kind: AggregateKind::Struct(Arc::from(struct_name.as_str())),
            operands,
        });
        Ok(MirValue::Local(result))
    }

    /// Determine the vector component count from a struct type name.
    fn vec_component_count(struct_name: &str) -> Option<u32> {
        match struct_name {
            "quanta_vec2" => Some(2),
            "quanta_vec3" => Some(3),
            "quanta_vec4" => Some(4),
            _ => None,
        }
    }

    /// Dispatch vector math builtins (dot, normalize, length, lerp, cross,
    /// reflect) to the correct size-specific C function based on the first
    /// argument's type.
    fn try_dispatch_vector_builtin(&mut self, name: &str, args: &[ast::Expr]) -> Option<CodegenResult<MirValue>> {
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
            "dot" => (format!("quanta_dot{}", n), MirType::f64()),
            "normalize" => (format!("quanta_normalize{}", n), first_ty.clone()),
            "length" => (format!("quanta_length{}", n), MirType::f64()),
            "cross" => {
                if n != 3 { return None; }
                ("quanta_cross".to_string(), first_ty.clone())
            }
            "reflect" => {
                if n != 3 { return None; }
                ("quanta_reflect3".to_string(), first_ty.clone())
            }
            "lerp" => (format!("quanta_lerp{}", n), first_ty.clone()),
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
            None => return Some(Err(CodegenError::Internal("No current function".to_string()))),
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
    fn try_dispatch_mat4_builtin(&mut self, name: &str, args: &[ast::Expr]) -> Option<CodegenResult<MirValue>> {
        let (c_func, ret_ty) = match name {
            "mat4_identity" => ("quanta_mat4_identity", MirType::Struct(Arc::from("quanta_mat4"))),
            "mat4_translate" => ("quanta_mat4_translate", MirType::Struct(Arc::from("quanta_mat4"))),
            "mat4_scale" => ("quanta_mat4_scale", MirType::Struct(Arc::from("quanta_mat4"))),
            "mat4_perspective" => ("quanta_mat4_perspective", MirType::Struct(Arc::from("quanta_mat4"))),
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
            None => return Some(Err(CodegenError::Internal("No current function".to_string()))),
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
    fn resolve_call_return_type(&self, func: &ast::Expr) -> MirType {
        let name = match &func.kind {
            ExprKind::Ident(ident) => Some(ident.name.to_string()),
            ExprKind::Path(path) => {
                // For module-qualified paths like `math::add`, join with `_`
                // to match the mangled name `math_add`.
                if path.segments.len() > 1 {
                    Some(path.segments.iter()
                        .map(|s| s.ident.name.as_ref())
                        .collect::<Vec<_>>()
                        .join("_"))
                } else {
                    path.last_ident().map(|i| i.name.to_string())
                }
            }
            _ => None,
        };
        if let Some(ref fn_name) = name {
            // Check module-level function declarations first.
            if let Some(func) = self.module.find_function(fn_name.as_str()) {
                return func.sig.ret.clone();
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
        // Check if it's a math/graphics builtin — these return f64
        if let Some(ref fn_name) = name {
            let is_f64_builtin = matches!(fn_name.as_str(),
                "sqrt" | "sin" | "cos" | "tan" | "pow" | "abs" |
                "floor" | "ceil" | "round" | "min" | "max" |
                "clamp" | "smoothstep" | "mix" | "fract" | "step" |
                "dot" | "length" | "lerp"
            );
            if is_f64_builtin {
                return MirType::f64();
            }
            // Vector constructors return struct types
            match fn_name.as_str() {
                "vec2" => return MirType::Struct(Arc::from("quanta_vec2")),
                "vec3" => return MirType::Struct(Arc::from("quanta_vec3")),
                "vec4" => return MirType::Struct(Arc::from("quanta_vec4")),
                "normalize" | "cross" | "reflect" => return MirType::Struct(Arc::from("quanta_vec3")),
                // Texture sampling — returns vec4 (tex2d_depth returns f64 for single channel)
                "tex2d" | "texture_sample" => return MirType::Struct(Arc::from("quanta_vec4")),
                "tex2d_depth" => return MirType::f64(),
                "mat4_identity" | "mat4_translate" | "mat4_scale" | "mat4_perspective" => {
                    return MirType::Struct(Arc::from("quanta_mat4"));
                }
                // Vec builtins
                "vec_new" => return MirType::Struct(Arc::from("QuantaVecHandle")),
                "vec_get" => return MirType::i32(),
                "vec_len" => return MirType::i64(), // size_t
                "vec_pop" => return MirType::i32(),
                "vec_push" => return MirType::Void,
                // File I/O builtins
                "read_file" => return MirType::Struct(Arc::from("QuantaString")),
                "write_file" => return MirType::Bool,
                "file_exists" => return MirType::Bool,
                // Format builtins
                "to_string_i32" | "to_string_f64" => return MirType::Struct(Arc::from("QuantaString")),
                // HashMap builtins
                "map_new" => return MirType::Struct(Arc::from("QuantaMapHandle")),
                "map_get" => return MirType::i32(),
                "map_len" => return MirType::i64(),
                "map_contains" | "map_remove" => return MirType::Bool,
                "map_insert" => return MirType::Void,
                _ => {}
            }
        }
        // Fallback when we cannot resolve the callee
        MirType::i32()
    }

    /// Check if a path expression refers to an enum variant (e.g. Shape::Circle).
    /// Returns (enum_name, variant_name) if it does.
    fn try_resolve_enum_variant_path(&self, func: &ast::Expr) -> Option<(Arc<str>, Arc<str>)> {
        if let ExprKind::Path(path) = &func.kind {
            if path.segments.len() == 2 {
                let enum_name = &path.segments[0].ident.name;
                let variant_name = &path.segments[1].ident.name;
                // Check both concrete enums AND generic enum templates
                if self.is_enum_type(enum_name) || self.generic_enums.contains_key(enum_name.as_ref()) {
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
        let arg_vals: Vec<_> = args.iter()
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
        let disc = self.lookup_enum_variant(&actual_enum_name, variant_name)
            .map(|(d, _)| d)
            .unwrap_or(0);

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

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
        let type_param_names: Vec<Arc<str>> = enum_def.generics.params.iter()
            .filter_map(|p| match &p.kind {
                ast::GenericParamKind::Type { .. } => Some(p.ident.name.clone()),
                _ => None,
            })
            .collect();

        // Find the variant definition
        if let Some(variant) = enum_def.variants.iter().find(|v| v.name.name.as_ref() == variant_name) {
            let fields = match &variant.fields {
                ast::StructFields::Tuple(fields) => fields.iter().map(|f| &f.ty).collect::<Vec<_>>(),
                ast::StructFields::Named(fields) => fields.iter().map(|f| &f.ty).collect::<Vec<_>>(),
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
        // Lower the receiver first to determine its type
        let receiver_val = self.lower_expr(receiver)?;
        let receiver_ty = self.type_of_value(&receiver_val);

        // Check if this is a string method (len on QuantaString)
        if let MirType::Struct(ref name) = receiver_ty {
            if name.as_ref() == "QuantaString" && method.name.as_ref() == "len" {
                let builder = self.current_fn.as_mut().ok_or_else(|| {
                    CodegenError::Internal("No current function".to_string())
                })?;
                let result = builder.create_local(MirType::usize());
                let cont = builder.create_block();
                let func = MirValue::Function(Arc::from("quanta_string_len"));
                builder.call(func, vec![receiver_val], Some(result), cont);
                builder.switch_to_block(cont);
                return Ok(values::local(result));
            }
        }

        // Dynamic dispatch for trait objects: obj.method() → obj.vtable->method(obj.data)
        if let MirType::TraitObject(ref trait_name) = receiver_ty {
            if let Some(trait_methods) = self.trait_methods.get(trait_name).cloned() {
                // Find the method index in the vtable
                if let Some((method_idx, (_, method_sig))) = trait_methods.iter().enumerate()
                    .find(|(_, (name, _))| name.as_ref() == method.name.as_ref())
                {
                    // Lower all arguments
                    let mut arg_vals = Vec::new();
                    for arg in args {
                        arg_vals.push(self.lower_expr(arg)?);
                    }

                    let ret_ty = method_sig.ret.clone();
                    let builder = self.current_fn.as_mut().ok_or_else(|| {
                        CodegenError::Internal("No current function".to_string())
                    })?;

                    // Extract data pointer from fat pointer: receiver.data
                    let data_ptr = builder.create_local(MirType::Ptr(Box::new(MirType::Void)));
                    builder.assign(data_ptr, MirRValue::FieldAccess {
                        base: receiver_val.clone(),
                        field_name: Arc::from("data"),
                        field_ty: MirType::Ptr(Box::new(MirType::Void)),
                    });

                    // Extract vtable pointer: receiver.vtable
                    let vtable_ptr_ty = MirType::Ptr(Box::new(MirType::Void));
                    let vtable_ptr = builder.create_local(vtable_ptr_ty.clone());
                    builder.assign(vtable_ptr, MirRValue::FieldAccess {
                        base: receiver_val,
                        field_name: Arc::from("vtable"),
                        field_ty: vtable_ptr_ty,
                    });

                    // The C backend will generate: receiver.vtable->method(receiver.data, args...)
                    // We store the method index and trait name for the C backend
                    let result = builder.create_local(ret_ty);
                    let cont = builder.create_block();

                    // Create a function value that encodes the vtable dispatch
                    let dispatch_name = Arc::from(format!(
                        "__vtable_dispatch_{}_{}_{}", trait_name, method.name, method_idx
                    ));

                    // Prepend data pointer as first argument (self)
                    let mut all_args = vec![values::local(data_ptr)];
                    all_args.extend(arg_vals);

                    builder.call(MirValue::Global(dispatch_name), all_args, Some(result), cont);
                    builder.switch_to_block(cont);
                    return Ok(values::local(result));
                }
            }
        }

        // Try to resolve the method via impl_methods registry
        let resolved_fn_name = if let MirType::Struct(ref type_name) = receiver_ty {
            self.impl_methods.get(&(type_name.clone(), method.name.clone())).cloned()
        } else {
            None
        };

        if let Some(mangled_name) = resolved_fn_name {
            // Check if the method's first parameter (self) is a pointer type,
            // meaning it was declared as &self or &mut self.
            let self_is_ref = self.module.find_function(&mangled_name)
                .and_then(|f| f.sig.params.first())
                .map(|p| p.is_pointer())
                .unwrap_or(false);

            // If the method takes &self, pass &receiver instead of receiver.
            let self_arg = if self_is_ref {
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
            let ret_ty = self.module.find_function(&mangled_name)
                .map(|f| f.sig.ret.clone())
                .unwrap_or(MirType::i32());

            let builder = self.current_fn.as_mut().ok_or_else(|| {
                CodegenError::Internal("No current function".to_string())
            })?;
            let result = builder.create_local(ret_ty);
            let cont = builder.create_block();
            let func = MirValue::Function(mangled_name);
            builder.call(func, arg_vals, Some(result), cont);
            builder.switch_to_block(cont);
            return Ok(values::local(result));
        }

        // Fallback: lower as a regular function call with receiver as first argument
        let mut arg_vals = vec![receiver_val];
        for arg in args {
            arg_vals.push(self.lower_expr(arg)?);
        }

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;
        let result = builder.create_local(MirType::i32());
        let cont = builder.create_block();
        let func = MirValue::Function(method.name.clone());
        builder.call(func, arg_vals, Some(result), cont);
        builder.switch_to_block(cont);
        Ok(values::local(result))
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
            let builder = self.current_fn.as_mut().ok_or_else(|| {
                CodegenError::Internal("No current function".to_string())
            })?;

            let then_block = builder.create_block();
            let else_block = builder.create_block();
            let merge_block = builder.create_block();

            builder.branch(cond_val, then_block, else_block);
            (then_block, else_block, merge_block)
        };

        // Then branch
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(then_block);
        }
        let then_val = self.lower_block(then_branch)?;

        // Determine the if-expression's result type from the then-branch value
        let result_ty = then_val.as_ref()
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

        // Else branch
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(else_block);
        }
        if let Some(else_expr) = else_branch {
            let else_val = self.lower_expr(else_expr)?;
            let builder = self.current_fn.as_mut().unwrap();
            builder.assign(result, MirRValue::Use(else_val));
        }
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.goto(merge_block);
            builder.switch_to_block(merge_block);
        }

        Ok(values::local(result))
    }

    fn lower_match(&mut self, scrutinee: &ast::Expr, arms: &[ast::MatchArm]) -> CodegenResult<MirValue> {
        // Evaluate the scrutinee once and store in a temporary.
        let scrutinee_val = self.lower_expr(scrutinee)?;
        let scrutinee_ty = self.type_of_value(&scrutinee_val);

        // Check if this is an enum match (scrutinee type is a known enum).
        let is_enum_match = if let MirType::Struct(ref name) = scrutinee_ty {
            self.is_enum_type(name)
        } else {
            false
        };

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

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
        let result_ty = if is_enum_match {
            let builder = self.current_fn.as_ref().ok_or_else(|| {
                CodegenError::Internal("No current function".to_string())
            })?;
            let ret = builder.return_type().clone();
            if ret == MirType::Void { MirType::i32() } else { ret }
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
                    self.lower_enum_pattern_test(
                        &arm.pattern,
                        scrutinee_local,
                        &scrutinee_ty,
                    )?
                } else {
                    self.lower_pattern_test(
                        &arm.pattern,
                        values::local(scrutinee_local),
                    )?
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
                let variant_name = path.segments.last()
                    .map(|s| s.ident.name.clone())
                    .unwrap_or(Arc::from(""));

                // Look up the discriminant for this variant
                let disc = self.lookup_enum_variant(&enum_name, &variant_name)
                    .map(|(d, _)| d)
                    .unwrap_or(0);

                let builder = self.current_fn.as_mut().ok_or_else(|| {
                    CodegenError::Internal("No current function".to_string())
                })?;

                // Read the tag: scrutinee.tag
                let tag_local = builder.create_local(MirType::i32());
                builder.assign(tag_local, MirRValue::FieldAccess {
                    base: values::local(scrutinee_local),
                    field_name: Arc::from("tag"),
                    field_ty: MirType::i32(),
                });

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
                let variant_name = path.segments.last()
                    .map(|s| s.ident.name.clone())
                    .unwrap_or(Arc::from(""));

                let disc = self.lookup_enum_variant(&enum_name, &variant_name)
                    .map(|(d, _)| d)
                    .unwrap_or(0);

                let builder = self.current_fn.as_mut().ok_or_else(|| {
                    CodegenError::Internal("No current function".to_string())
                })?;

                let tag_local = builder.create_local(MirType::i32());
                builder.assign(tag_local, MirRValue::FieldAccess {
                    base: values::local(scrutinee_local),
                    field_name: Arc::from("tag"),
                    field_ty: MirType::i32(),
                });

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
                let variant_name = path.segments.last()
                    .map(|s| s.ident.name.clone())
                    .unwrap_or(Arc::from(""));

                // Look up variant fields to get their types
                let variant_fields = self.lookup_enum_variant(&enum_name, &variant_name)
                    .map(|(_, fields)| fields)
                    .unwrap_or_default();

                for (idx, pat) in patterns.iter().enumerate() {
                    if let ast::PatternKind::Ident { name, .. } = &pat.kind {
                        // Skip wildcard `_` patterns — don't create a local
                        if name.name.as_ref() == "_" {
                            continue;
                        }
                        let field_ty = variant_fields.get(idx)
                            .map(|(_, ty)| ty.clone())
                            .unwrap_or(MirType::f64());

                        let builder = self.current_fn.as_mut().ok_or_else(|| {
                            CodegenError::Internal("No current function".to_string())
                        })?;

                        let local = builder.create_named_local(name.name.clone(), field_ty.clone());
                        builder.assign(local, MirRValue::VariantField {
                            base: values::local(scrutinee_local),
                            variant_name: variant_name.clone(),
                            field_index: idx as u32,
                            field_ty,
                        });
                        self.var_map.insert(name.name.clone(), local);
                    }
                    // Wildcard patterns in enum variant bindings are ignored.
                }
            }

            ast::PatternKind::Ident { name, .. } => {
                // Bind the entire scrutinee to the variable
                let builder = self.current_fn.as_mut().ok_or_else(|| {
                    CodegenError::Internal("No current function".to_string())
                })?;
                let local = builder.create_local(scrutinee_ty.clone());
                builder.assign(local, MirRValue::Use(values::local(scrutinee_local)));
                self.var_map.insert(name.name.clone(), local);
            }

            _ => {}
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
                let builder = self.current_fn.as_mut().ok_or_else(|| {
                    CodegenError::Internal("No current function".to_string())
                })?;
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
                let builder = self.current_fn.as_mut().ok_or_else(|| {
                    CodegenError::Internal("No current function".to_string())
                })?;
                let cmp = builder.create_local(MirType::Bool);
                builder.binary_op(cmp, BinOp::Eq, scrutinee_val, path_val);
                Ok(values::local(cmp))
            }

            // Unsupported pattern kinds fall through to "always matches" to
            // avoid panicking.  A real implementation would need full pattern
            // compilation here.
            _ => Ok(values::bool(true)),
        }
    }

    fn lower_loop(&mut self, body: &ast::Block, _label: Option<&ast::Ident>) -> CodegenResult<MirValue> {
        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

        let loop_block = builder.create_block();
        let exit_block = builder.create_block();

        self.loop_stack.push((loop_block, exit_block));

        builder.goto(loop_block);
        builder.switch_to_block(loop_block);

        self.lower_block(body)?;

        let builder = self.current_fn.as_mut().unwrap();
        builder.goto(loop_block);

        self.loop_stack.pop();

        builder.switch_to_block(exit_block);

        Ok(values::unit())
    }

    fn lower_while(&mut self, condition: &ast::Expr, body: &ast::Block, _label: Option<&ast::Ident>) -> CodegenResult<MirValue> {
        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let exit_block = builder.create_block();

        self.loop_stack.push((cond_block, exit_block));

        builder.goto(cond_block);
        builder.switch_to_block(cond_block);

        let cond_val = self.lower_expr(condition)?;
        let builder = self.current_fn.as_mut().unwrap();
        builder.branch(cond_val, body_block, exit_block);

        builder.switch_to_block(body_block);
        self.lower_block(body)?;
        let builder = self.current_fn.as_mut().unwrap();
        builder.goto(cond_block);

        self.loop_stack.pop();

        builder.switch_to_block(exit_block);

        Ok(values::unit())
    }

    fn lower_for(&mut self, pattern: &ast::Pattern, iter: &ast::Expr, body: &ast::Block, _label: Option<&ast::Ident>) -> CodegenResult<MirValue> {
        // Detect range-based for loops: `for i in start..end` or `for i in start..=end`
        // and lower them into an explicit counted loop.
        if let ExprKind::Range { start, end, inclusive } = &iter.kind {
            return self.lower_for_range(pattern, start.as_deref(), end.as_deref(), *inclusive, body);
        }

        // Also detect binary Range/RangeInclusive operators produced by the
        // parser for `0..10` style expressions.
        if let ExprKind::Binary { op, left, right } = &iter.kind {
            if *op == AstBinOp::Range {
                return self.lower_for_range(pattern, Some(left), Some(right), false, body);
            }
            if *op == AstBinOp::RangeInclusive {
                return self.lower_for_range(pattern, Some(left), Some(right), true, body);
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
            _ => {
                // Not an array — try iterator protocol: call .next() in a loop.
                // Requires the type to have a `next` method registered in impl_methods
                // that returns an enum with variant 0 = Some(T) and variant 1 = None.
                if let MirType::Struct(ref type_name) = iter_ty {
                    if self.impl_methods.contains_key(&(type_name.clone(), Arc::from("next"))) {
                        return self.lower_for_iterator(pattern, iter_val, &iter_ty, body);
                    }
                }
                // No iterator protocol — emit a no-op loop
                let builder = self.current_fn.as_mut().ok_or_else(|| {
                    CodegenError::Internal("No current function".to_string())
                })?;
                let exit_block = builder.create_block();
                builder.goto(exit_block);
                builder.switch_to_block(exit_block);
                return Ok(values::unit());
            }
        };

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

        // Store the array in a local
        let arr_local = builder.create_local(iter_ty.clone());
        builder.assign(arr_local, MirRValue::Use(iter_val));

        // Create index counter: let mut __idx = 0;
        let idx_local = builder.create_local(MirType::i64());
        builder.assign(idx_local, MirRValue::Use(MirValue::Const(MirConst::Int(0, MirType::i64()))));

        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let incr_block = builder.create_block();
        let exit_block = builder.create_block();

        self.loop_stack.push((incr_block, exit_block));

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
        builder.assign(elem_local, MirRValue::IndexAccess {
            base: values::local(arr_local),
            index: values::local(idx_local),
            elem_ty: elem_ty.clone(),
        });

        // Bind pattern variable
        if let ast::PatternKind::Ident { name, .. } = &pattern.kind {
            self.var_map.insert(name.name.clone(), elem_local);
        }

        self.lower_block(body)?;

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

    /// Lower a range-based for loop: `for i in start..end { body }`
    ///
    /// Generates:
    /// ```text
    /// let mut i = start;     // (or 0 if start is None)
    /// loop_cond:
    ///   if i >= end { goto exit }   // (> for inclusive)
    ///   body
    ///   i = i + 1
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

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

        // Create the loop counter local and initialize it.
        let counter = builder.create_local(iter_ty.clone());
        builder.assign(counter, MirRValue::Use(start_val));

        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let incr_block = builder.create_block();
        let exit_block = builder.create_block();

        self.loop_stack.push((incr_block, exit_block));

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

        // Bind the loop variable so the body can reference it.
        if let ast::PatternKind::Ident { name, .. } = &pattern.kind {
            self.var_map.insert(name.name.clone(), counter);
        }

        self.lower_block(body)?;

        // Fall through to the increment block.
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.goto(incr_block);
            builder.switch_to_block(incr_block);
        }

        // Increment: counter = counter + 1
        {
            let builder = self.current_fn.as_mut().unwrap();
            let one = MirValue::Const(MirConst::Int(1, iter_ty));
            builder.binary_op(counter, BinOp::Add, values::local(counter), one);
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
    ) -> CodegenResult<MirValue> {
        let type_name = if let MirType::Struct(ref name) = iter_ty {
            name.clone()
        } else {
            return Ok(values::unit());
        };

        // Resolve the `next` method
        let next_fn_name = match self.impl_methods.get(&(type_name.clone(), Arc::from("next"))) {
            Some(name) => name.clone(),
            None => return Ok(values::unit()),
        };

        // Get return type of next() — should be an enum (Option-like)
        let next_ret_ty = self.module.find_function(next_fn_name.as_ref())
            .map(|f| f.sig.ret.clone())
            .unwrap_or(MirType::i32());

        // Get the payload type from the enum's first variant (Some(T))
        let payload_ty = if let MirType::Struct(ref enum_name) = next_ret_ty {
            if let Some(type_def) = self.module.find_type(enum_name) {
                if let TypeDefKind::Enum { variants, .. } = &type_def.kind {
                    variants.iter()
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

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

        // Store the iterator in a mutable local
        let iter_local = builder.create_local(iter_ty.clone());
        builder.assign(iter_local, MirRValue::Use(iter_val));

        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let exit_block = builder.create_block();

        self.loop_stack.push((cond_block, exit_block));

        builder.goto(cond_block);
        builder.switch_to_block(cond_block);

        // Call iter.next()
        let next_result = builder.create_local(next_ret_ty.clone());
        let cont_after_next = builder.create_block();
        let func = MirValue::Function(next_fn_name);
        builder.call(func, vec![values::local(iter_local)], Some(next_result), cont_after_next);
        builder.switch_to_block(cont_after_next);

        // Check tag: if tag == 1 (None), goto exit
        let tag_local = builder.create_local(MirType::i32());
        builder.assign(tag_local, MirRValue::FieldAccess {
            base: values::local(next_result),
            field_name: Arc::from("tag"),
            field_ty: MirType::i32(),
        });
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
                    variants.iter()
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
        builder.assign(payload, MirRValue::VariantField {
            base: values::local(next_result),
            variant_name: some_variant_name,
            field_index: 0,
            field_ty: payload_ty,
        });

        // Bind pattern variable
        if let ast::PatternKind::Ident { name, .. } = &pattern.kind {
            self.var_map.insert(name.name.clone(), payload);
        }

        self.lower_block(body)?;

        let builder = self.current_fn.as_mut().unwrap();
        builder.goto(cond_block);

        self.loop_stack.pop();

        let builder = self.current_fn.as_mut().unwrap();
        builder.switch_to_block(exit_block);

        Ok(values::unit())
    }

    fn lower_return(&mut self, value: Option<&ast::Expr>) -> CodegenResult<MirValue> {
        // Lower value expression FIRST if present
        let ret_val = if let Some(expr) = value {
            Some(self.lower_expr(expr)?)
        } else {
            None
        };

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

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
        builder.assign(tag_local, MirRValue::FieldAccess {
            base: values::local(scrutinee_local),
            field_name: Arc::from("tag"),
            field_ty: MirType::i32(),
        });

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
            builder.assign(unwrapped, MirRValue::VariantField {
                base: values::local(scrutinee_local),
                variant_name: ok_variant_name,
                field_index: 0,
                field_ty: ok_field_ty,
            });
            builder.goto(cont_block);

            // 6. Err/None block -- construct the error variant and return early
            builder.switch_to_block(err_block);

            if let Some(err_v) = err_variant {
                if err_v.fields.is_empty() {
                    // None-like variant (no payload): return EnumName::VariantName
                    let err_result = builder.create_local(MirType::Struct(enum_name.clone()));
                    builder.aggregate(
                        err_result,
                        AggregateKind::Variant(enum_name, err_v.discriminant as u32, err_v.name.clone()),
                        vec![],
                    );
                    builder.ret(Some(values::local(err_result)));
                } else {
                    // Err-like variant (has payload): extract payload, reconstruct, return
                    let err_field_ty = err_v.fields.first()
                        .map(|(_, ty)| ty.clone())
                        .unwrap_or(MirType::i32());
                    let err_payload = builder.create_local(err_field_ty.clone());
                    builder.assign(err_payload, MirRValue::VariantField {
                        base: values::local(scrutinee_local),
                        variant_name: err_v.name.clone(),
                        field_index: 0,
                        field_ty: err_field_ty,
                    });
                    let err_result = builder.create_local(MirType::Struct(enum_name.clone()));
                    builder.aggregate(
                        err_result,
                        AggregateKind::Variant(enum_name, err_v.discriminant as u32, err_v.name.clone()),
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

    fn lower_break(&mut self, value: Option<&ast::Expr>, _label: Option<&ast::Ident>) -> CodegenResult<MirValue> {
        // Break value (e.g. `break 42`) is evaluated but currently not
        // assigned to a loop result local because loops do not yet propagate
        // a result variable.  The value is lowered for side-effect correctness.
        if let Some((_, exit_block)) = self.loop_stack.last().copied() {
            if let Some(expr) = value {
                let _val = self.lower_expr(expr)?;
            }
            let builder = self.current_fn.as_mut().ok_or_else(|| {
                CodegenError::Internal("No current function".to_string())
            })?;
            builder.goto(exit_block);
        }

        let builder = self.current_fn.as_mut().unwrap();
        let unreachable = builder.create_block();
        builder.switch_to_block(unreachable);

        Ok(values::unit())
    }

    fn lower_continue(&mut self, _label: Option<&ast::Ident>) -> CodegenResult<MirValue> {
        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

        if let Some((continue_block, _)) = self.loop_stack.last().copied() {
            builder.goto(continue_block);
        }

        let unreachable = builder.create_block();
        builder.switch_to_block(unreachable);

        Ok(values::unit())
    }

    fn lower_tuple(&mut self, elems: &[ast::Expr]) -> CodegenResult<MirValue> {
        let elem_vals: Vec<_> = elems.iter()
            .map(|e| self.lower_expr(e))
            .collect::<CodegenResult<_>>()?;

        // Use Void for empty tuples (unit), otherwise use a Struct placeholder
        // since MIR does not have a first-class tuple type.
        let tuple_ty = if elem_vals.is_empty() {
            MirType::Void
        } else {
            // Represent tuples as anonymous struct types.  A full implementation
            // would register a proper tuple type; for now this prevents the
            // backend from seeing a Void-typed local that actually holds data.
            MirType::Struct(Arc::from("tuple"))
        };

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;
        let result = builder.create_local(tuple_ty);
        builder.aggregate(result, AggregateKind::Tuple, elem_vals);

        Ok(values::local(result))
    }

    fn lower_array(&mut self, elems: &[ast::Expr]) -> CodegenResult<MirValue> {
        let elem_vals: Vec<_> = elems.iter()
            .map(|e| self.lower_expr(e))
            .collect::<CodegenResult<_>>()?;

        // Infer element type from the first element; fall back to i32 for
        // empty array literals.
        let elem_ty = elem_vals.first()
            .map(|v| self.type_of_value(v))
            .unwrap_or(MirType::i32());

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;
        let result = builder.create_local(MirType::Array(Box::new(elem_ty.clone()), elems.len() as u64));
        builder.aggregate(result, AggregateKind::Array(elem_ty), elem_vals);

        Ok(values::local(result))
    }

    fn lower_index(&mut self, arr: &ast::Expr, index: &ast::Expr) -> CodegenResult<MirValue> {
        let arr_val = self.lower_expr(arr)?;
        let idx_val = self.lower_expr(index)?;

        // Derive element type: if the array value has type Array(elem, _)
        // then the result is of type elem; otherwise fall back to i32.
        let elem_ty = match self.type_of_value(&arr_val) {
            MirType::Array(elem, _) => *elem,
            MirType::Slice(elem) => *elem,
            MirType::Ptr(inner) => *inner,
            _ => MirType::i32(),
        };

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

        let result = builder.create_local(elem_ty.clone());
        builder.assign(result, MirRValue::IndexAccess {
            base: arr_val,
            index: idx_val,
            elem_ty,
        });

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
        // e.g. color.xyz → quanta_vec3_new(color.x, color.y, color.z)
        if let MirType::Struct(ref struct_name) = effective_ty {
            if let Some(max_comp) = Self::vec_component_count(struct_name) {
                if Self::is_swizzle_pattern(&field_name, max_comp) {
                    let swizzle_len = field_name.len() as u32;
                    let result_type_name = format!("quanta_vec{}", swizzle_len);
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
                        builder.assign(comp_local, MirRValue::FieldAccess {
                            base: obj_val.clone(),
                            field_name: Arc::from(comp_name),
                            field_ty: MirType::f64(),
                        });
                        component_vals.push(values::local(comp_local));
                    }

                    // Build constructor call: quanta_vecN_new(...)
                    let constructor = format!("quanta_vec{}_new", swizzle_len);
                    let builder = self.current_fn.as_mut().ok_or_else(|| {
                        CodegenError::Internal("No current function".to_string())
                    })?;
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

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

        let result = builder.create_local(field_ty.clone());
        builder.assign(result, MirRValue::FieldAccess {
            base: obj_val,
            field_name,
            field_ty,
        });

        Ok(values::local(result))
    }

    fn lower_ref(&mut self, mutability: ast::Mutability, inner: &ast::Expr) -> CodegenResult<MirValue> {
        let inner_val = self.lower_expr(inner)?;
        let inner_ty = self.type_of_value(&inner_val);

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

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

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

        // Emit a Deref rvalue that reads through the pointer.
        let result = builder.create_local(pointee_ty.clone());
        builder.assign(result, MirRValue::Deref {
            ptr: inner_val,
            pointee_ty,
        });

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

            let builder = self.current_fn.as_mut().ok_or_else(|| {
                CodegenError::Internal("No current function".to_string())
            })?;

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

            // Store vtable reference — the C backend generates &vtable_instance
            let vtable_name: Arc<str> = Arc::from(format!(
                "{}_{}_vtable_instance", type_name, trait_name
            ));
            let vtable_struct_ty = MirType::Struct(Arc::from(format!("{}_vtable", trait_name)));
            let vtable_ptr = builder.create_local(vtable_struct_ty);
            builder.assign(vtable_ptr, MirRValue::Use(MirValue::Global(vtable_name)));

            // Construct the fat pointer aggregate
            builder.assign(result, MirRValue::Aggregate {
                kind: AggregateKind::Struct(Arc::from(format!("dyn_{}", trait_name))),
                operands: vec![values::local(data_ptr), values::local(vtable_ptr)],
            });

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

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

        let result = builder.create_local(target_ty.clone());
        builder.cast(result, cast_kind, inner_val, target_ty);

        Ok(values::local(result))
    }

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
                if !param_names.contains(&ident.name)
                    && !seen.contains(&ident.name)
                {
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
            ExprKind::If { condition, then_branch, else_branch } => {
                Self::collect_free_vars_inner(condition, param_names, env_vars, found, seen, source);
                for stmt in &then_branch.stmts {
                    if let StmtKind::Expr(e) | StmtKind::Semi(e) = &stmt.kind {
                        Self::collect_free_vars_inner(e, param_names, env_vars, found, seen, source);
                    }
                }
                if let Some(e) = else_branch {
                    Self::collect_free_vars_inner(e, param_names, env_vars, found, seen, source);
                }
            }
            ExprKind::Block(block) | ExprKind::Unsafe(block) => {
                for stmt in &block.stmts {
                    if let StmtKind::Expr(e) | StmtKind::Semi(e) = &stmt.kind {
                        Self::collect_free_vars_inner(e, param_names, env_vars, found, seen, source);
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
                Self::collect_free_vars_inner(scrutinee, param_names, env_vars, found, seen, source);
                for arm in arms {
                    Self::collect_free_vars_inner(&arm.body, param_names, env_vars, found, seen, source);
                    if let Some(guard) = &arm.guard {
                        Self::collect_free_vars_inner(guard, param_names, env_vars, found, seen, source);
                    }
                }
            }
            ExprKind::Macro { tokens, .. } => {
                // Scan token trees for identifiers
                for tt in tokens {
                    Self::collect_free_vars_in_token_tree(tt, param_names, env_vars, found, seen, source);
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
                    Self::collect_free_vars_in_token_tree(inner, param_names, env_vars, found, seen, source);
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
    fn lower_closure(
        &mut self,
        params: &[ast::ClosureParam],
        return_type: Option<&ast::Type>,
        body: &ast::Expr,
    ) -> CodegenResult<MirValue> {
        let closure_id = self.closure_count;
        self.closure_count += 1;
        let closure_name: Arc<str> = Arc::from(format!("__closure_{}", closure_id));

        // ---- Detect captured variables (lambda lifting) ----
        let param_names: HashSet<Arc<str>> = params.iter()
            .filter_map(|p| {
                if let ast::PatternKind::Ident { name, .. } = &p.pattern.kind {
                    Some(name.name.clone())
                } else {
                    None
                }
            })
            .collect();

        let captures = Self::collect_free_vars(
            body,
            &param_names,
            &self.var_map,
            self.source.as_deref(),
        );

        // ---- Build the MIR signature ----
        // Declared params first, then captured-variable params appended.
        let mut mir_params: Vec<MirType> = params.iter().map(|p| {
            p.ty.as_ref()
                .map(|t| self.lower_type_from_ast(t))
                .unwrap_or(MirType::i32())
        }).collect();

        // Resolve captured variable types from the enclosing builder.
        let capture_types: Vec<MirType> = captures.iter().map(|(_name, local_id)| {
            if let Some(ref builder) = self.current_fn {
                builder.local_type(*local_id).unwrap_or(MirType::i32())
            } else {
                MirType::i32()
            }
        }).collect();

        mir_params.extend(capture_types.iter().cloned());

        let mir_ret = return_type
            .map(|t| self.lower_type_from_ast(t))
            .unwrap_or(MirType::i32());

        let sig = MirFnSig::new(mir_params.clone(), mir_ret.clone());
        // The fn-ptr type must use the *full* parameter list (visible +
        // captured) so that the C declaration matches the call sites, which
        // append captured values as extra arguments.
        let fn_ptr_ty = MirType::FnPtr(Box::new(MirFnSig::new(mir_params.clone(), mir_ret.clone())));

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

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function for closure".to_string())
        })?;
        let result = builder.create_local(fn_ptr_ty);
        builder.assign(result, MirRValue::Use(MirValue::Function(closure_name.clone())));

        // Track that this local holds the given closure so we can find
        // its captures later when calling through this local.
        self.local_closure_name.insert(result, closure_name);

        Ok(values::local(result))
    }

    fn lower_struct_expr(
        &mut self,
        path: &ast::Path,
        fields: &[ast::FieldExpr],
        _rest: Option<&ast::Expr>,
    ) -> CodegenResult<MirValue> {
        // Lower all field values FIRST before borrowing the builder.
        let field_vals: Vec<_> = fields.iter()
            .map(|f| {
                if let Some(val) = &f.value {
                    self.lower_expr(val)
                } else {
                    // Field shorthand: `name` means `name: name`
                    self.lower_ident(&f.name)
                }
            })
            .collect::<CodegenResult<_>>()?;

        let raw_name = path.last_ident()
            .map(|i| i.name.clone())
            .unwrap_or(Arc::from(""));

        // Check if this is a generic struct that needs monomorphization
        let struct_name = if self.generic_structs.contains_key(raw_name.as_ref()) {
            // Try to resolve from explicit generic args on the path
            let generic_args = path.last_generics().unwrap_or(&[]);
            if !generic_args.is_empty() {
                let empty_subst = HashMap::new();
                let subst = self.resolve_generic_args_with_subst(
                    raw_name.as_ref(), generic_args, &empty_subst,
                );
                self.monomorphize_struct(raw_name.as_ref(), &subst)?
            } else {
                // Infer generic params from field values
                let subst = self.infer_struct_generics_from_fields(raw_name.as_ref(), &field_vals, fields);
                self.monomorphize_struct(raw_name.as_ref(), &subst)?
            }
        } else {
            raw_name
        };

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;
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
        let type_param_names: Vec<Arc<str>> = struct_def.generics.params.iter()
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
            "IO"     => 1,
            "Error"  => 2,
            "Async"  => 3,
            "State"  => 4,
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
    fn lower_handle(
        &mut self,
        effect: &ast::Path,
        handlers: &[ast::EffectHandler],
        body: &ast::Block,
    ) -> CodegenResult<MirValue> {
        // Resolve the effect name and its integer ID.
        let effect_name = effect.segments.iter()
            .map(|s| s.ident.name.as_ref())
            .collect::<Vec<_>>()
            .join("::");
        let eid = Self::effect_id(&effect_name);

        // --- Allocate locals ---------------------------------------------------
        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

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
        let push_block  = builder.create_labeled_block("effect_push");
        let body_block  = builder.create_labeled_block("effect_body");
        let merge_block = builder.create_labeled_block("effect_merge");

        // Create a block for each handler clause.
        let handler_blocks: Vec<BlockId> = handlers.iter().enumerate().map(|(i, _)| {
            let builder = self.current_fn.as_mut().unwrap();
            builder.create_labeled_block(format!("effect_handler_{}", i))
        }).collect();

        // --- Emit: push handler -------------------------------------------------
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.goto(push_block);
            builder.switch_to_block(push_block);

            // quanta_push_handler(&handler, effect_id)
            let push_fn = MirValue::Function(Arc::from("quanta_push_handler"));
            // Take address of the handler struct so the C call gets a pointer.
            let handler_ptr_local = builder.create_local(
                MirType::Ptr(Box::new(MirType::Struct(Arc::from("QuantaHandler")))),
            );
            builder.assign(handler_ptr_local, MirRValue::AddressOf {
                is_mut: true,
                place: MirPlace::local(handler_local),
            });
            let eid_val = MirValue::Const(MirConst::Int(eid as i128, MirType::i32()));
            let cont = builder.create_block();
            builder.call(push_fn, vec![MirValue::Local(handler_ptr_local), eid_val], None, cont);
            builder.switch_to_block(cont);

            // setjmp(handler.env) — pass the handler local directly;
            // the C backend will emit `.env` when it sees a setjmp call
            // with a QuantaHandler-typed argument.
            let setjmp_fn = MirValue::Function(Arc::from("setjmp"));
            let cont2 = builder.create_block();
            builder.call(setjmp_fn, vec![MirValue::Local(handler_local)], Some(setjmp_result), cont2);
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
                let targets: Vec<(MirConst, BlockId)> = handler_blocks.iter().enumerate().map(|(i, &blk)| {
                    (MirConst::Int((i as i128) + 1, MirType::i32()), blk)
                }).collect();
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
                    let ty = param.ty.as_ref()
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
    fn lower_resume(&mut self, value: Option<&ast::Expr>) -> CodegenResult<MirValue> {
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
    fn lower_perform(
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

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;

        // Allocate a result slot on the stack.  Use an unnamed local to
        // avoid duplicate C declarations when perform is called multiple
        // times with the same effect/operation.
        let result_local = builder.create_local(MirType::i32());

        // Store the argument value into a local so we can take its address.
        let arg_local = builder.create_local(arg_ty.clone());
        builder.assign(arg_local, MirRValue::Use(arg_val));

        // Take address of arg and result for the void* parameters.
        let arg_ptr = builder.create_local(MirType::Ptr(Box::new(arg_ty)));
        builder.assign(arg_ptr, MirRValue::AddressOf {
            is_mut: false,
            place: MirPlace::local(arg_local),
        });
        let result_ptr = builder.create_local(MirType::Ptr(Box::new(MirType::i32())));
        builder.assign(result_ptr, MirRValue::AddressOf {
            is_mut: true,
            place: MirPlace::local(result_local),
        });

        // quanta_perform(effect_id, op_id, &arg, &result)
        let perform_fn = MirValue::Function(Arc::from("quanta_perform"));
        let eid_val = MirValue::Const(MirConst::Int(eid as i128, MirType::i32()));
        let op_val  = MirValue::Const(MirConst::Int(op_id as i128, MirType::i32()));

        let cont = builder.create_block();
        builder.call(
            perform_fn,
            vec![eid_val, op_val, MirValue::Local(arg_ptr), MirValue::Local(result_ptr)],
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

    fn lower_print_macro(&mut self, tokens: &[ast::TokenTree], newline: bool) -> CodegenResult<()> {
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
                            let ptr_local = builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
                            if let MirValue::Local(local_id) = val {
                                builder.assign(ptr_local, MirRValue::FieldAccess {
                                    base: MirValue::Local(local_id),
                                    field_name: Arc::from("ptr"),
                                    field_ty: MirType::Ptr(Box::new(MirType::i8())),
                                });
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
                        let local_ty = self.current_fn.as_ref()
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
        let arg_values: Vec<MirValue> = arg_values.into_iter()
            .take(placeholder_count)
            .collect();

        let builder = self.current_fn.as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function for macro".into()))?;

        // Create a local for the format string pointer
        let fmt_local = builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
        builder.assign(fmt_local, MirRValue::Use(MirValue::Const(MirConst::Str(str_idx))));

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
        use crate::lexer::{TokenKind, Delimiter};

        let source = match self.source {
            Some(ref s) => s,
            None => return Vec::new(),
        };

        // Flatten delimited groups to get a flat list of tokens (the macro
        // parser emits flat token sequences, not nested Delimited nodes).
        let flat: Vec<&ast::TokenTree> = tokens.iter()
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
                        if current_arg_start.is_none() { current_arg_start = Some(s); }
                        if e > current_arg_end { current_arg_end = e; }
                    }
                    TokenKind::CloseDelim(Delimiter::Paren)
                    | TokenKind::CloseDelim(Delimiter::Bracket) => {
                        paren_depth -= 1;
                        let s = tok.span.start.to_usize();
                        let e = tok.span.end.to_usize();
                        if current_arg_start.is_none() { current_arg_start = Some(s); }
                        if e > current_arg_end { current_arg_end = e; }
                    }
                    TokenKind::Comma if paren_depth == 0 => {
                        // Top-level comma: flush current argument
                        if let Some(start) = current_arg_start {
                            if current_arg_end > start && current_arg_end <= source.len() {
                                let text = source[start..current_arg_end].trim().to_string();
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
                        if current_arg_start.is_none() { current_arg_start = Some(s); }
                        if e > current_arg_end { current_arg_end = e; }
                    }
                }
            }
        }

        // Flush any remaining argument
        if let Some(start) = current_arg_start {
            if current_arg_end > start && current_arg_end <= source.len() {
                let text = source[start..current_arg_end].trim().to_string();
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
            Some(MirType::Struct(name)) if name.as_ref() == "QuantaString" => {
                "%s".to_string()
            }
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
        }.to_string()
    }

    fn lower_panic_macro(&mut self, tokens: &[ast::TokenTree]) -> CodegenResult<()> {
        // Print the panic message first
        self.lower_print_macro(tokens, true)?;

        let builder = self.current_fn.as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function for macro".into()))?;

        // Call abort() after printing
        builder.abort();

        // Create an unreachable continuation block for any code after panic
        let unreachable_block = builder.create_block();
        builder.switch_to_block(unreachable_block);

        Ok(())
    }

    fn extract_string_from_tokens(&self, tokens: &[ast::TokenTree]) -> String {
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
                                    let raw = &source[start..end];
                                    // Strip surrounding quotes
                                    let content = raw.trim_start_matches('"').trim_end_matches('"');
                                    return content.to_string();
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
        let flat: Vec<&ast::TokenTree> = tokens.iter()
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
                                let name = &source[start..end];
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

    // =========================================================================
    // TYPE LOWERING
    // =========================================================================

    fn lower_type_from_ast(&self, ty: &ast::Type) -> MirType {
        match &ty.kind {
            ast::TypeKind::Never => MirType::Never,
            ast::TypeKind::Infer => MirType::i32(), // Inference placeholder: i32 is a safe default
            ast::TypeKind::Tuple(elems) => {
                if elems.is_empty() {
                    MirType::Void
                } else {
                    // Tuples with elements are represented as anonymous structs.
                    // A full implementation would register a structural tuple
                    // type; using Struct("tuple") keeps the pipeline working.
                    MirType::Struct(Arc::from("tuple"))
                }
            }
            ast::TypeKind::Array { elem, len } => {
                let elem_ty = self.lower_type_from_ast(elem);
                // Try to evaluate the length as a literal integer; default to
                // 0 when the expression is too complex for const evaluation.
                let length = self.try_const_eval(len)
                    .and_then(|c| match c {
                        MirConst::Int(v, _) => Some(v as u64),
                        MirConst::Uint(v, _) => Some(v as u64),
                        _ => None,
                    })
                    .unwrap_or(0);
                MirType::Array(Box::new(elem_ty), length)
            }
            ast::TypeKind::Slice(elem) => {
                MirType::Slice(Box::new(self.lower_type_from_ast(elem)))
            }
            ast::TypeKind::Ptr { ty: inner, .. } => {
                MirType::Ptr(Box::new(self.lower_type_from_ast(inner)))
            }
            ast::TypeKind::Ref { ty: inner, .. } => {
                MirType::Ptr(Box::new(self.lower_type_from_ast(inner)))
            }
            ast::TypeKind::Path(path) => {
                self.lower_type_path(path)
            }
            ast::TypeKind::BareFn { params, return_ty, .. } => {
                let mir_params: Vec<MirType> = params.iter()
                    .map(|p| self.lower_type_from_ast(&p.ty))
                    .collect();
                let mir_ret = return_ty.as_ref()
                    .map(|t| self.lower_type_from_ast(t))
                    .unwrap_or(MirType::Void);
                MirType::FnPtr(Box::new(MirFnSig::new(mir_params, mir_ret)))
            }
            ast::TypeKind::FnTrait { params, return_ty, .. } => {
                let mir_params: Vec<MirType> = params.iter()
                    .map(|p| self.lower_type_from_ast(p))
                    .collect();
                let mir_ret = return_ty.as_ref()
                    .map(|t| self.lower_type_from_ast(t))
                    .unwrap_or(MirType::Void);
                MirType::FnPtr(Box::new(MirFnSig::new(mir_params, mir_ret)))
            }
            ast::TypeKind::TraitObject { bounds, .. } => {
                // dyn Trait → MirType::TraitObject("TraitName")
                if let Some(first_bound) = bounds.first() {
                    let name = first_bound.path.last_ident()
                        .map(|i| i.name.clone())
                        .unwrap_or(Arc::from("Unknown"));
                    MirType::TraitObject(name)
                } else {
                    MirType::TraitObject(Arc::from("Unknown"))
                }
            }
            ast::TypeKind::WithEffect { ty: inner, .. } => {
                // `with` annotations are compile-time metadata — the runtime
                // type is the base type. Lower through to the inner type.
                self.lower_type_from_ast(inner)
            }
            _ => MirType::i32(),
        }
    }

    fn lower_type_path(&self, path: &ast::Path) -> MirType {
        if let Some(ident) = path.last_ident() {
            // Check for generic type arguments: Option<i32>, Result<i32, str>, Pair<f64>
            if let Some(generic_args) = path.last_generics() {
                if !generic_args.is_empty() {
                    let type_name = ident.name.as_ref();
                    // Check if this is a known generic enum or struct
                    let is_generic_enum = self.generic_enums.contains_key(type_name);
                    let is_generic_struct = self.generic_structs.contains_key(type_name);

                    if is_generic_enum || is_generic_struct {
                        // Resolve the generic args to concrete types
                        let empty_subst = HashMap::new();
                        let subst = self.resolve_generic_args_with_subst(
                            type_name, generic_args, &empty_subst,
                        );
                        if !subst.is_empty() {
                            let mangled = Self::mangle_generic_name(type_name, &subst);
                            return MirType::Struct(mangled);
                        }
                    }
                }
            }

            match ident.name.as_ref() {
                "i8" => MirType::i8(),
                "i16" => MirType::i16(),
                "i32" => MirType::i32(),
                "i64" => MirType::i64(),
                "i128" => MirType::Int(IntSize::I128, true),
                "isize" => MirType::isize(),
                "u8" => MirType::u8(),
                "u16" => MirType::u16(),
                "u32" => MirType::u32(),
                "u64" => MirType::u64(),
                "u128" => MirType::Int(IntSize::I128, false),
                "usize" => MirType::usize(),
                "f32" => MirType::f32(),
                "f64" => MirType::f64(),
                "bool" => MirType::Bool,
                "char" => MirType::u32(),
                "str" | "String" => MirType::Struct(Arc::from("QuantaString")),
                "vec2" => MirType::Struct(Arc::from("quanta_vec2")),
                "vec3" => MirType::Struct(Arc::from("quanta_vec3")),
                "vec4" => MirType::Struct(Arc::from("quanta_vec4")),
                "mat4" => MirType::Struct(Arc::from("quanta_mat4")),
                name => MirType::Struct(Arc::from(name)),
            }
        } else {
            MirType::i32()
        }
    }

    // =========================================================================
    // CONST EVALUATION
    // =========================================================================

    fn try_const_eval(&self, expr: &ast::Expr) -> Option<MirConst> {
        match &expr.kind {
            ExprKind::Literal(lit) => match lit {
                Literal::Int { value, .. } => Some(MirConst::Int(*value as i128, MirType::i32())),
                Literal::Float { value, .. } => Some(MirConst::Float(*value, MirType::f64())),
                Literal::Bool(b) => Some(MirConst::Bool(*b)),
                Literal::Char(c) => Some(MirConst::Uint(*c as u128, MirType::u32())),
                _ => None,
            },
            _ => None,
        }
    }

    // =========================================================================
    // GENERICS MONOMORPHIZATION
    // =========================================================================

    /// Check whether a function definition has type-level generic parameters
    /// (ignoring lifetime-only generics).
    fn fn_has_type_generics(&self, f: &ast::FnDef) -> bool {
        f.generics.params.iter().any(|p| {
            matches!(p.kind, ast::GenericParamKind::Type { .. })
        })
    }

    /// Extract the simple function name from a call expression, if it is
    /// a plain identifier or single-segment path.
    fn extract_call_name<'a>(&self, func: &'a ast::Expr) -> Option<&'a str> {
        match &func.kind {
            ExprKind::Ident(ident) => Some(ident.name.as_ref()),
            ExprKind::Path(path) => {
                if path.segments.len() == 1 {
                    Some(path.segments[0].ident.name.as_ref())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Generate a mangling suffix for a MirType, used to form monomorphized
    /// function names like `identity_i32` or `max_f64`.
    fn mangle_type(ty: &MirType) -> String {
        match ty {
            MirType::Bool => "bool".to_string(),
            MirType::Void => "void".to_string(),
            MirType::Never => "never".to_string(),
            MirType::Int(size, signed) => {
                let prefix = if *signed { "i" } else { "u" };
                let bits = match size {
                    IntSize::I8 => "8",
                    IntSize::I16 => "16",
                    IntSize::I32 => "32",
                    IntSize::I64 => "64",
                    IntSize::I128 => "128",
                    IntSize::ISize => "size",
                };
                format!("{}{}", prefix, bits)
            }
            MirType::Float(size) => match size {
                FloatSize::F32 => "f32".to_string(),
                FloatSize::F64 => "f64".to_string(),
            },
            MirType::Ptr(inner) => format!("ptr_{}", Self::mangle_type(inner)),
            MirType::Array(inner, len) => format!("arr_{}_{}", Self::mangle_type(inner), len),
            MirType::Slice(inner) => format!("slice_{}", Self::mangle_type(inner)),
            MirType::Struct(name) => name.to_string(),
            MirType::FnPtr(_) => "fnptr".to_string(),
            MirType::Vector(inner, lanes) => format!("vec_{}x{}", Self::mangle_type(inner), lanes),
            MirType::Texture2D(inner) => format!("tex2d_{}", Self::mangle_type(inner)),
            MirType::Sampler => "sampler".to_string(),
            MirType::SampledImage(inner) => format!("sampledimg_{}", Self::mangle_type(inner)),
            MirType::TraitObject(name) => format!("dyn_{}", name),
        }
    }

    /// Infer the concrete MirType for the first type parameter by examining the
    /// first argument at the call site.  Returns the MirType inferred from the
    /// first argument's literal or local variable type.
    fn infer_type_from_args(&self, args: &[ast::Expr]) -> MirType {
        if let Some(first_arg) = args.first() {
            match &first_arg.kind {
                ExprKind::Literal(lit) => match lit {
                    Literal::Int { suffix, .. } => {
                        suffix.as_ref().map(|s| match s {
                            ast::IntSuffix::I8 => MirType::i8(),
                            ast::IntSuffix::I16 => MirType::i16(),
                            ast::IntSuffix::I32 => MirType::i32(),
                            ast::IntSuffix::I64 => MirType::i64(),
                            ast::IntSuffix::I128 => MirType::Int(IntSize::I128, true),
                            ast::IntSuffix::Isize => MirType::isize(),
                            ast::IntSuffix::U8 => MirType::u8(),
                            ast::IntSuffix::U16 => MirType::u16(),
                            ast::IntSuffix::U32 => MirType::u32(),
                            ast::IntSuffix::U64 => MirType::u64(),
                            ast::IntSuffix::U128 => MirType::Int(IntSize::I128, false),
                            ast::IntSuffix::Usize => MirType::usize(),
                        }).unwrap_or(MirType::i32())
                    }
                    Literal::Float { suffix, .. } => {
                        suffix.as_ref().map(|s| match s {
                            ast::FloatSuffix::F16 | ast::FloatSuffix::F32 => MirType::f32(),
                            ast::FloatSuffix::F64 => MirType::f64(),
                        }).unwrap_or(MirType::f64())
                    }
                    Literal::Bool(_) => MirType::Bool,
                    Literal::Char(_) => MirType::u32(),
                    Literal::Str { .. } => MirType::Ptr(Box::new(MirType::i8())),
                    _ => MirType::i32(),
                },
                ExprKind::Ident(ident) => {
                    // Look up the variable's type from the var_map
                    if let Some(&local_id) = self.var_map.get(&ident.name) {
                        if let Some(ref builder) = self.current_fn {
                            if let Some(ty) = builder.local_type(local_id) {
                                return ty;
                            }
                        }
                    }
                    MirType::i32()
                }
                _ => {
                    // For complex expressions, lower the argument and infer
                    // from the result — but since we can't lower here without
                    // side effects, fall back to i32.
                    MirType::i32()
                }
            }
        } else {
            MirType::i32()
        }
    }

    /// Substitute all occurrences of the generic type parameter in an AST Type
    /// node with a concrete type path.  Returns a new cloned Type with
    /// substitutions applied.
    fn substitute_type_in_ast_type(
        ty: &ast::Type,
        param_name: &str,
        concrete_name: &str,
    ) -> ast::Type {
        let new_kind = match &ty.kind {
            ast::TypeKind::Path(path) => {
                if path.is_simple() {
                    if let Some(ident) = path.last_ident() {
                        if ident.name.as_ref() == param_name {
                            // Replace T with the concrete type
                            let new_ident = ast::Ident {
                                name: Arc::from(concrete_name),
                                span: ident.span,
                            };
                            let seg = ast::PathSegment::from_ident(new_ident);
                            ast::TypeKind::Path(ast::Path::new(vec![seg], path.span))
                        } else {
                            ty.kind.clone()
                        }
                    } else {
                        ty.kind.clone()
                    }
                } else {
                    ty.kind.clone()
                }
            }
            ast::TypeKind::Ref { lifetime, mutability, ty: inner } => {
                ast::TypeKind::Ref {
                    lifetime: lifetime.clone(),
                    mutability: *mutability,
                    ty: Box::new(Self::substitute_type_in_ast_type(inner, param_name, concrete_name)),
                }
            }
            ast::TypeKind::Ptr { mutability, ty: inner } => {
                ast::TypeKind::Ptr {
                    mutability: *mutability,
                    ty: Box::new(Self::substitute_type_in_ast_type(inner, param_name, concrete_name)),
                }
            }
            ast::TypeKind::Slice(inner) => {
                ast::TypeKind::Slice(Box::new(
                    Self::substitute_type_in_ast_type(inner, param_name, concrete_name),
                ))
            }
            ast::TypeKind::Array { elem, len } => {
                ast::TypeKind::Array {
                    elem: Box::new(Self::substitute_type_in_ast_type(elem, param_name, concrete_name)),
                    len: len.clone(),
                }
            }
            ast::TypeKind::Tuple(elems) => {
                ast::TypeKind::Tuple(
                    elems.iter()
                        .map(|e| Self::substitute_type_in_ast_type(e, param_name, concrete_name))
                        .collect(),
                )
            }
            _ => ty.kind.clone(),
        };

        ast::Type {
            kind: new_kind,
            span: ty.span,
            id: ty.id,
        }
    }

    /// Create a monomorphized (specialized) copy of a generic FnDef by
    /// replacing its single type parameter with a concrete type.
    fn monomorphize_fndef(
        f: &ast::FnDef,
        param_name: &str,
        concrete_name: &str,
        mangled_fn_name: Arc<str>,
    ) -> ast::FnDef {
        // Build new params with substituted types
        let new_params: Vec<ast::Param> = f.sig.params.iter().map(|p| {
            ast::Param {
                attrs: p.attrs.clone(),
                pattern: p.pattern.clone(),
                ty: Box::new(Self::substitute_type_in_ast_type(&p.ty, param_name, concrete_name)),
                default: p.default.clone(),
                span: p.span,
            }
        }).collect();

        // Build new return type
        let new_return_ty = f.sig.return_ty.as_ref().map(|rt| {
            Box::new(Self::substitute_type_in_ast_type(rt, param_name, concrete_name))
        });

        ast::FnDef {
            name: ast::Ident {
                name: mangled_fn_name,
                span: f.name.span,
            },
            generics: ast::Generics::empty(), // No longer generic
            sig: ast::FnSig {
                is_unsafe: f.sig.is_unsafe,
                is_async: f.sig.is_async,
                is_const: f.sig.is_const,
                abi: f.sig.abi.clone(),
                params: new_params,
                return_ty: new_return_ty,
                effects: f.sig.effects.clone(),
            },
            body: f.body.clone(),
        }
    }

    /// Map a MirType to the QuantaLang source-level type name used for AST
    /// substitution (e.g. MirType::i32() -> "i32", MirType::f64() -> "f64").
    fn mir_type_to_quanta_name(ty: &MirType) -> &'static str {
        match ty {
            MirType::Bool => "bool",
            MirType::Int(IntSize::I8, true) => "i8",
            MirType::Int(IntSize::I16, true) => "i16",
            MirType::Int(IntSize::I32, true) => "i32",
            MirType::Int(IntSize::I64, true) => "i64",
            MirType::Int(IntSize::I128, true) => "i128",
            MirType::Int(IntSize::ISize, true) => "isize",
            MirType::Int(IntSize::I8, false) => "u8",
            MirType::Int(IntSize::I16, false) => "u16",
            MirType::Int(IntSize::I32, false) => "u32",
            MirType::Int(IntSize::I64, false) => "u64",
            MirType::Int(IntSize::I128, false) => "u128",
            MirType::Int(IntSize::ISize, false) => "usize",
            MirType::Float(FloatSize::F32) => "f32",
            MirType::Float(FloatSize::F64) => "f64",
            _ => "i32", // Fallback for complex types
        }
    }

    /// Lower a call to a generic function.  This infers the concrete type from
    /// the call-site arguments, monomorphizes the function if it has not been
    /// generated yet, and emits the call to the mangled specialization.
    fn lower_generic_call(
        &mut self,
        func: &ast::Expr,
        args: &[ast::Expr],
    ) -> CodegenResult<MirValue> {
        let fn_name_str = self.extract_call_name(func)
            .ok_or_else(|| CodegenError::Internal("generic call without name".to_string()))?;
        let fn_name: Arc<str> = Arc::from(fn_name_str);

        // Retrieve the generic FnDef to build the substitution map.
        let generic_fndef = self.generic_functions.get(&fn_name)
            .ok_or_else(|| CodegenError::Internal(
                format!("generic function {} not found", fn_name),
            ))?
            .clone();

        // Collect all type parameter names in declaration order.
        let type_param_names: Vec<Arc<str>> = generic_fndef.generics.params.iter()
            .filter_map(|p| match &p.kind {
                ast::GenericParamKind::Type { .. } => Some(p.ident.name.clone()),
                _ => None,
            })
            .collect();

        // Build multi-param substitution map by inferring types from arguments.
        let subst = self.infer_subst_from_args(&generic_fndef, &type_param_names, args);
        let mangled_name = Self::mangle_generic_name(fn_name.as_ref(), &subst);

        // Monomorphize on demand: generate the specialization if we haven't already.
        if !self.monomorphized.contains(&mangled_name) {
            self.monomorphized.insert(mangled_name.clone());

            // Build a monomorphized FnDef with all type parameters replaced.
            let specialized = Self::monomorphize_fndef_multi(
                &generic_fndef,
                &subst,
                mangled_name.clone(),
            );

            // Save the current function context — lower_function will
            // overwrite current_fn / var_map for the specialization.
            let saved_fn = self.current_fn.take();
            let saved_vars = std::mem::take(&mut self.var_map);

            // Lower the specialized function as a normal (non-generic) function.
            self.lower_function(&specialized, &[])?;

            // Restore the caller's function context.
            self.current_fn = saved_fn;
            self.var_map = saved_vars;
        }

        // Now emit the call to the monomorphized function.
        let func_val = MirValue::Function(mangled_name.clone());

        // Resolve the return type from the now-lowered specialization.
        let ret_ty = self.module.find_function(mangled_name.as_ref())
            .map(|f| f.sig.ret.clone())
            .unwrap_or(MirType::i32());

        let arg_vals: Vec<_> = args.iter()
            .map(|a| self.lower_expr(a))
            .collect::<CodegenResult<_>>()?;

        let builder = self.current_fn.as_mut().ok_or_else(|| {
            CodegenError::Internal("No current function".to_string())
        })?;
        let result = builder.create_local(ret_ty);
        let cont = builder.create_block();

        builder.call(func_val, arg_vals, Some(result), cont);
        builder.switch_to_block(cont);

        Ok(values::local(result))
    }

    /// Infer a full substitution map by matching generic function params against
    /// call-site argument types.  Walks each parameter type to bind all generic
    /// type parameters, not just the first.
    fn infer_subst_from_args(
        &self,
        fndef: &ast::FnDef,
        type_param_names: &[Arc<str>],
        args: &[ast::Expr],
    ) -> HashMap<Arc<str>, MirType> {
        let mut subst = HashMap::new();

        for (i, param) in fndef.sig.params.iter().enumerate() {
            if let Some(arg_expr) = args.get(i) {
                let arg_ty = self.infer_single_arg_type(arg_expr);
                // If the parameter type is a simple generic param name, bind it
                if let ast::TypeKind::Path(path) = &param.ty.kind {
                    if path.is_simple() {
                        if let Some(ident) = path.last_ident() {
                            for tp_name in type_param_names {
                                if ident.name.as_ref() == tp_name.as_ref() {
                                    subst.entry(tp_name.clone()).or_insert(arg_ty.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fill in any unbound params with i32 default
        for tp_name in type_param_names {
            subst.entry(tp_name.clone()).or_insert(MirType::i32());
        }

        subst
    }

    /// Infer the MirType for a single expression (used by subst inference).
    fn infer_single_arg_type(&self, expr: &ast::Expr) -> MirType {
        match &expr.kind {
            ExprKind::Literal(lit) => match lit {
                Literal::Int { suffix, .. } => {
                    suffix.as_ref().map(|s| match s {
                        ast::IntSuffix::I8 => MirType::i8(),
                        ast::IntSuffix::I16 => MirType::i16(),
                        ast::IntSuffix::I32 => MirType::i32(),
                        ast::IntSuffix::I64 => MirType::i64(),
                        ast::IntSuffix::I128 => MirType::Int(IntSize::I128, true),
                        ast::IntSuffix::Isize => MirType::isize(),
                        ast::IntSuffix::U8 => MirType::u8(),
                        ast::IntSuffix::U16 => MirType::u16(),
                        ast::IntSuffix::U32 => MirType::u32(),
                        ast::IntSuffix::U64 => MirType::u64(),
                        ast::IntSuffix::U128 => MirType::Int(IntSize::I128, false),
                        ast::IntSuffix::Usize => MirType::usize(),
                    }).unwrap_or(MirType::i32())
                }
                Literal::Float { suffix, .. } => {
                    suffix.as_ref().map(|s| match s {
                        ast::FloatSuffix::F16 | ast::FloatSuffix::F32 => MirType::f32(),
                        ast::FloatSuffix::F64 => MirType::f64(),
                    }).unwrap_or(MirType::f64())
                }
                Literal::Bool(_) => MirType::Bool,
                Literal::Char(_) => MirType::u32(),
                Literal::Str { .. } => MirType::Struct(Arc::from("QuantaString")),
                _ => MirType::i32(),
            },
            ExprKind::Ident(ident) => {
                if let Some(&local_id) = self.var_map.get(&ident.name) {
                    if let Some(ref builder) = self.current_fn {
                        if let Some(ty) = builder.local_type(local_id) {
                            return ty;
                        }
                    }
                }
                MirType::i32()
            }
            _ => MirType::i32(),
        }
    }

    /// Monomorphize a FnDef using a multi-parameter substitution map.
    fn monomorphize_fndef_multi(
        f: &ast::FnDef,
        subst: &HashMap<Arc<str>, MirType>,
        mangled_fn_name: Arc<str>,
    ) -> ast::FnDef {
        // Build new params with all type parameters substituted
        let new_params: Vec<ast::Param> = f.sig.params.iter().map(|p| {
            ast::Param {
                attrs: p.attrs.clone(),
                pattern: p.pattern.clone(),
                ty: Box::new(Self::substitute_type_in_ast_type_multi(&p.ty, subst)),
                default: p.default.clone(),
                span: p.span,
            }
        }).collect();

        // Build new return type
        let new_return_ty = f.sig.return_ty.as_ref().map(|rt| {
            Box::new(Self::substitute_type_in_ast_type_multi(rt, subst))
        });

        ast::FnDef {
            name: ast::Ident {
                name: mangled_fn_name,
                span: f.name.span,
            },
            generics: ast::Generics::empty(), // No longer generic
            sig: ast::FnSig {
                is_unsafe: f.sig.is_unsafe,
                is_async: f.sig.is_async,
                is_const: f.sig.is_const,
                abi: f.sig.abi.clone(),
                params: new_params,
                return_ty: new_return_ty,
                effects: f.sig.effects.clone(),
            },
            body: f.body.clone(),
        }
    }

    /// Substitute all generic type parameters in an AST Type using a multi-param map.
    fn substitute_type_in_ast_type_multi(
        ty: &ast::Type,
        subst: &HashMap<Arc<str>, MirType>,
    ) -> ast::Type {
        let new_kind = match &ty.kind {
            ast::TypeKind::Path(path) => {
                if path.is_simple() {
                    if let Some(ident) = path.last_ident() {
                        // Check if this ident is any of the type params
                        if let Some(concrete_ty) = subst.get(&ident.name) {
                            let concrete_name = Self::mir_type_to_quanta_name(concrete_ty);
                            let new_ident = ast::Ident {
                                name: Arc::from(concrete_name),
                                span: ident.span,
                            };
                            let seg = ast::PathSegment::from_ident(new_ident);
                            return ast::Type::new(
                                ast::TypeKind::Path(ast::Path::new(vec![seg], path.span)),
                                ty.span,
                            );
                        }
                    }
                }
                ty.kind.clone()
            }
            ast::TypeKind::Ref { lifetime, mutability, ty: inner } => {
                ast::TypeKind::Ref {
                    lifetime: lifetime.clone(),
                    mutability: *mutability,
                    ty: Box::new(Self::substitute_type_in_ast_type_multi(inner, subst)),
                }
            }
            ast::TypeKind::Ptr { mutability, ty: inner } => {
                ast::TypeKind::Ptr {
                    mutability: *mutability,
                    ty: Box::new(Self::substitute_type_in_ast_type_multi(inner, subst)),
                }
            }
            ast::TypeKind::Slice(inner) => {
                ast::TypeKind::Slice(Box::new(
                    Self::substitute_type_in_ast_type_multi(inner, subst),
                ))
            }
            ast::TypeKind::Array { elem, len } => {
                ast::TypeKind::Array {
                    elem: Box::new(Self::substitute_type_in_ast_type_multi(elem, subst)),
                    len: len.clone(),
                }
            }
            _ => ty.kind.clone(),
        };
        ast::Type::new(new_kind, ty.span)
    }
}

/// Determine item visibility.  The AST currently does not carry a
/// visibility modifier on the Ident node, so all items are assumed
/// public.  When the parser / AST is extended with `pub` tracking,
/// this function should inspect the actual modifier.
fn item_visibility(_name: &ast::Ident) -> Option<bool> {
    Some(true)
}
