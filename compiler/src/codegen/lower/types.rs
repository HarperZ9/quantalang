// ===============================================================================
// QUANTALANG CODE GENERATOR - TYPE LOWERING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Type lowering, const evaluation, and generic monomorphization for MIR.

use std::collections::HashMap;
use std::sync::Arc;

use crate::ast::{self, ExprKind, Literal};

use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::values;
use crate::codegen::ir::*;

use super::MirLowerer;

impl<'ctx> MirLowerer<'ctx> {
    // =========================================================================
    // TYPE LOWERING
    // =========================================================================

    pub(crate) fn lower_type_from_ast(&self, ty: &ast::Type) -> MirType {
        match &ty.kind {
            ast::TypeKind::Never => MirType::Never,
            ast::TypeKind::Infer => MirType::i32(), // Inference placeholder: i32 is a safe default
            ast::TypeKind::Tuple(elems) => {
                if elems.is_empty() {
                    MirType::Void
                } else {
                    let elem_tys: Vec<MirType> =
                        elems.iter().map(|t| self.lower_type_from_ast(t)).collect();
                    MirType::Tuple(elem_tys)
                }
            }
            ast::TypeKind::Array { elem, len } => {
                let elem_ty = self.lower_type_from_ast(elem);
                // Try to evaluate the length as a literal integer; default to
                // 0 when the expression is too complex for const evaluation.
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
            ast::TypeKind::Slice(elem) => MirType::Slice(Box::new(self.lower_type_from_ast(elem))),
            ast::TypeKind::Ptr { ty: inner, .. } => {
                MirType::Ptr(Box::new(self.lower_type_from_ast(inner)))
            }
            ast::TypeKind::Ref { ty: inner, .. } => {
                MirType::Ptr(Box::new(self.lower_type_from_ast(inner)))
            }
            ast::TypeKind::Path(path) => self.lower_type_path(path),
            ast::TypeKind::BareFn {
                params, return_ty, ..
            } => {
                let mir_params: Vec<MirType> = params
                    .iter()
                    .map(|p| self.lower_type_from_ast(&p.ty))
                    .collect();
                let mir_ret = return_ty
                    .as_ref()
                    .map(|t| self.lower_type_from_ast(t))
                    .unwrap_or(MirType::Void);
                MirType::FnPtr(Box::new(MirFnSig::new(mir_params, mir_ret)))
            }
            ast::TypeKind::FnTrait {
                params, return_ty, ..
            } => {
                let mir_params: Vec<MirType> =
                    params.iter().map(|p| self.lower_type_from_ast(p)).collect();
                let mir_ret = return_ty
                    .as_ref()
                    .map(|t| self.lower_type_from_ast(t))
                    .unwrap_or(MirType::Void);
                MirType::FnPtr(Box::new(MirFnSig::new(mir_params, mir_ret)))
            }
            ast::TypeKind::TraitObject { bounds, .. } => {
                // dyn Trait → MirType::TraitObject("TraitName")
                if let Some(first_bound) = bounds.first() {
                    let name = first_bound
                        .path
                        .last_ident()
                        .map(|i| i.name.clone())
                        .unwrap_or(Arc::from("Unknown"));
                    MirType::TraitObject(name)
                } else {
                    MirType::TraitObject(Arc::from("Unknown"))
                }
            }
            ast::TypeKind::WithEffect {
                ty: inner,
                effects: _,
            } => {
                // `with` annotations are compile-time metadata — the runtime
                // type is the base type. Lower through to the inner type.
                // Effects are preserved via extract_type_annotations() for shader output.
                self.lower_type_from_ast(inner)
            }
            _ => MirType::i32(),
        }
    }

    /// Extract type annotations from an AST type (e.g., `f64 with ColorSpace<Linear>`).
    /// Returns a list of annotation strings like `["ColorSpace:Linear"]`.
    pub(crate) fn extract_type_annotations(ty: &ast::Type) -> Vec<Arc<str>> {
        match &ty.kind {
            ast::TypeKind::WithEffect { effects, .. } => {
                effects
                    .iter()
                    .map(|path| {
                        // Format effect path as "Category:Value" (e.g., "ColorSpace:Linear")
                        let segments: Vec<&str> = path
                            .segments
                            .iter()
                            .map(|seg| seg.ident.name.as_ref())
                            .collect();
                        Arc::from(segments.join(":"))
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    fn lower_type_path(&self, path: &ast::Path) -> MirType {
        if let Some(ident) = path.last_ident() {
            // Check for generic type arguments: Option<i32>, Result<i32, str>, Pair<f64>
            if let Some(generic_args) = path.last_generics() {
                if !generic_args.is_empty() {
                    let type_name = ident.name.as_ref();

                    // Special-case Vec<T>: resolve to MirType::Vec(element_type)
                    if type_name == "Vec" {
                        if let Some(ast::GenericArg::Type(arg_ty)) = generic_args.first() {
                            let elem_ty = self.lower_type_from_ast(arg_ty);
                            return MirType::Vec(Box::new(elem_ty));
                        }
                        // Vec without type arg defaults to Vec<i32>
                        return MirType::Vec(Box::new(MirType::i32()));
                    }

                    // Special-case HashMap<K, V>: resolve to MirType::Map(key, value)
                    if type_name == "HashMap" {
                        let key_ty =
                            if let Some(ast::GenericArg::Type(arg_ty)) = generic_args.first() {
                                self.lower_type_from_ast(arg_ty)
                            } else {
                                MirType::Struct(Arc::from("QuantaString"))
                            };
                        let val_ty =
                            if let Some(ast::GenericArg::Type(arg_ty)) = generic_args.get(1) {
                                self.lower_type_from_ast(arg_ty)
                            } else {
                                MirType::f64()
                            };
                        return MirType::Map(Box::new(key_ty), Box::new(val_ty));
                    }

                    // Check if this is a known generic enum or struct
                    let is_generic_enum = self.generic_enums.contains_key(type_name);
                    let is_generic_struct = self.generic_structs.contains_key(type_name);

                    if is_generic_enum || is_generic_struct {
                        // Resolve the generic args to concrete types
                        let empty_subst = HashMap::new();
                        let subst = self.resolve_generic_args_with_subst(
                            type_name,
                            generic_args,
                            &empty_subst,
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
                // Resolve Self to the current impl's concrete type name
                "Self" => {
                    if let Some(ref impl_ty) = self.current_impl_type {
                        MirType::Struct(impl_ty.clone())
                    } else {
                        MirType::Struct(Arc::from("Self"))
                    }
                }
                name => {
                    // Inside inline modules, use the prefixed struct name
                    // for types defined in the current module scope.
                    // This ensures consistent naming: if the struct typedef
                    // is emitted as `std_Vec3`, function return types must
                    // also use `std_Vec3`, not bare `Vec3`.
                    if !self.module_prefix.is_empty() {
                        let prefixed = self.prefixed_name(&Arc::from(name));
                        if self.module.find_type(prefixed.as_ref()).is_some() {
                            return MirType::Struct(prefixed);
                        }
                    }
                    MirType::Struct(Arc::from(name))
                }
            }
        } else {
            MirType::i32()
        }
    }

    // =========================================================================
    // CONST EVALUATION
    // =========================================================================

    pub(crate) fn try_const_eval(&self, expr: &ast::Expr) -> Option<MirConst> {
        match &expr.kind {
            ExprKind::Literal(lit) => match lit {
                Literal::Int { value, .. } => Some(MirConst::Int(*value as i128, MirType::i32())),
                Literal::Float { value, .. } => Some(MirConst::Float(*value, MirType::f64())),
                Literal::Bool(b) => Some(MirConst::Bool(*b)),
                Literal::Char(c) => Some(MirConst::Uint(*c as u128, MirType::u32())),
                _ => None,
            },
            ExprKind::Struct { path, fields, .. } => {
                let struct_name = path
                    .last_ident()
                    .map(|i| i.name.clone())
                    .unwrap_or(Arc::from(""));
                let mut field_consts = Vec::new();
                for f in fields {
                    let val_expr = f.value.as_ref()?;
                    let c = self.try_const_eval(val_expr)?;
                    field_consts.push(c);
                }
                Some(MirConst::Struct(struct_name, field_consts))
            }
            ExprKind::Unary {
                op: ast::UnaryOp::Neg,
                expr: inner,
            } => {
                // Support negative literals in const context: -0.5, -1
                match self.try_const_eval(inner)? {
                    MirConst::Int(v, ty) => Some(MirConst::Int(-v, ty)),
                    MirConst::Float(v, ty) => Some(MirConst::Float(-v, ty)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    // =========================================================================
    // GENERICS MONOMORPHIZATION
    // =========================================================================

    /// Check whether a function definition has type-level generic parameters
    /// (ignoring lifetime-only generics).
    pub(crate) fn fn_has_type_generics(&self, f: &ast::FnDef) -> bool {
        f.generics
            .params
            .iter()
            .any(|p| matches!(p.kind, ast::GenericParamKind::Type { .. }))
    }

    /// Extract the simple function name from a call expression, if it is
    /// a plain identifier or single-segment path.
    pub(crate) fn extract_call_name<'a>(&self, func: &'a ast::Expr) -> Option<&'a str> {
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
    pub(crate) fn mangle_type(ty: &MirType) -> String {
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
            MirType::Vec(inner) => format!("Vec_{}", Self::mangle_type(inner)),
            MirType::Tuple(elems) => {
                let parts: Vec<String> = elems.iter().map(|e| Self::mangle_type(e)).collect();
                format!("tuple_{}", parts.join("_"))
            }
            MirType::Map(key, val) => {
                format!("Map_{}_{}", Self::mangle_type(key), Self::mangle_type(val))
            }
        }
    }

    /// Infer the concrete MirType for the first type parameter by examining the
    /// first argument at the call site.  Returns the MirType inferred from the
    /// first argument's literal or local variable type.
    fn infer_type_from_args(&self, args: &[ast::Expr]) -> MirType {
        if let Some(first_arg) = args.first() {
            match &first_arg.kind {
                ExprKind::Literal(lit) => match lit {
                    Literal::Int { suffix, .. } => suffix
                        .as_ref()
                        .map(|s| match s {
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
                        })
                        .unwrap_or(MirType::i32()),
                    Literal::Float { suffix, .. } => suffix
                        .as_ref()
                        .map(|s| match s {
                            ast::FloatSuffix::F16 | ast::FloatSuffix::F32 => MirType::f32(),
                            ast::FloatSuffix::F64 => MirType::f64(),
                        })
                        .unwrap_or(MirType::f64()),
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
            ast::TypeKind::Ref {
                lifetime,
                mutability,
                ty: inner,
            } => ast::TypeKind::Ref {
                lifetime: lifetime.clone(),
                mutability: *mutability,
                ty: Box::new(Self::substitute_type_in_ast_type(
                    inner,
                    param_name,
                    concrete_name,
                )),
            },
            ast::TypeKind::Ptr {
                mutability,
                ty: inner,
            } => ast::TypeKind::Ptr {
                mutability: *mutability,
                ty: Box::new(Self::substitute_type_in_ast_type(
                    inner,
                    param_name,
                    concrete_name,
                )),
            },
            ast::TypeKind::Slice(inner) => ast::TypeKind::Slice(Box::new(
                Self::substitute_type_in_ast_type(inner, param_name, concrete_name),
            )),
            ast::TypeKind::Array { elem, len } => ast::TypeKind::Array {
                elem: Box::new(Self::substitute_type_in_ast_type(
                    elem,
                    param_name,
                    concrete_name,
                )),
                len: len.clone(),
            },
            ast::TypeKind::Tuple(elems) => ast::TypeKind::Tuple(
                elems
                    .iter()
                    .map(|e| Self::substitute_type_in_ast_type(e, param_name, concrete_name))
                    .collect(),
            ),
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
        let new_params: Vec<ast::Param> = f
            .sig
            .params
            .iter()
            .map(|p| ast::Param {
                attrs: p.attrs.clone(),
                pattern: p.pattern.clone(),
                ty: Box::new(Self::substitute_type_in_ast_type(
                    &p.ty,
                    param_name,
                    concrete_name,
                )),
                default: p.default.clone(),
                span: p.span,
            })
            .collect();

        // Build new return type
        let new_return_ty = f.sig.return_ty.as_ref().map(|rt| {
            Box::new(Self::substitute_type_in_ast_type(
                rt,
                param_name,
                concrete_name,
            ))
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
    pub(crate) fn lower_generic_call(
        &mut self,
        func: &ast::Expr,
        args: &[ast::Expr],
    ) -> CodegenResult<MirValue> {
        let fn_name_str = self
            .extract_call_name(func)
            .ok_or_else(|| CodegenError::Internal("generic call without name".to_string()))?;
        let fn_name: Arc<str> = Arc::from(fn_name_str);

        // Retrieve the generic FnDef to build the substitution map.
        let generic_fndef = self
            .generic_functions
            .get(&fn_name)
            .ok_or_else(|| {
                CodegenError::Internal(format!("generic function {} not found", fn_name))
            })?
            .clone();

        // Collect all type parameter names in declaration order.
        let type_param_names: Vec<Arc<str>> = generic_fndef
            .generics
            .params
            .iter()
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
            let specialized =
                Self::monomorphize_fndef_multi(&generic_fndef, &subst, mangled_name.clone());

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
        let ret_ty = self
            .module
            .find_function(mangled_name.as_ref())
            .map(|f| f.sig.ret.clone())
            .unwrap_or(MirType::i32());

        let arg_vals: Vec<_> = args
            .iter()
            .map(|a| self.lower_expr(a))
            .collect::<CodegenResult<_>>()?;

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
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
                Literal::Int { suffix, .. } => suffix
                    .as_ref()
                    .map(|s| match s {
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
                    })
                    .unwrap_or(MirType::i32()),
                Literal::Float { suffix, .. } => suffix
                    .as_ref()
                    .map(|s| match s {
                        ast::FloatSuffix::F16 | ast::FloatSuffix::F32 => MirType::f32(),
                        ast::FloatSuffix::F64 => MirType::f64(),
                    })
                    .unwrap_or(MirType::f64()),
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
    pub(crate) fn monomorphize_fndef_multi(
        f: &ast::FnDef,
        subst: &HashMap<Arc<str>, MirType>,
        mangled_fn_name: Arc<str>,
    ) -> ast::FnDef {
        // Build new params with all type parameters substituted
        let new_params: Vec<ast::Param> = f
            .sig
            .params
            .iter()
            .map(|p| ast::Param {
                attrs: p.attrs.clone(),
                pattern: p.pattern.clone(),
                ty: Box::new(Self::substitute_type_in_ast_type_multi(&p.ty, subst)),
                default: p.default.clone(),
                span: p.span,
            })
            .collect();

        // Build new return type
        let new_return_ty = f
            .sig
            .return_ty
            .as_ref()
            .map(|rt| Box::new(Self::substitute_type_in_ast_type_multi(rt, subst)));

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
            ast::TypeKind::Ref {
                lifetime,
                mutability,
                ty: inner,
            } => ast::TypeKind::Ref {
                lifetime: lifetime.clone(),
                mutability: *mutability,
                ty: Box::new(Self::substitute_type_in_ast_type_multi(inner, subst)),
            },
            ast::TypeKind::Ptr {
                mutability,
                ty: inner,
            } => ast::TypeKind::Ptr {
                mutability: *mutability,
                ty: Box::new(Self::substitute_type_in_ast_type_multi(inner, subst)),
            },
            ast::TypeKind::Slice(inner) => ast::TypeKind::Slice(Box::new(
                Self::substitute_type_in_ast_type_multi(inner, subst),
            )),
            ast::TypeKind::Array { elem, len } => ast::TypeKind::Array {
                elem: Box::new(Self::substitute_type_in_ast_type_multi(elem, subst)),
                len: len.clone(),
            },
            _ => ty.kind.clone(),
        };
        ast::Type::new(new_kind, ty.span)
    }
}
