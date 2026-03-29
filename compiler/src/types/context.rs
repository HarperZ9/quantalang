// ===============================================================================
// QUANTALANG TYPE SYSTEM - TYPE CONTEXT
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Type context and environment for type checking.
//!
//! The type context maintains:
//! - Variable bindings (name -> type)
//! - Type definitions (structs, enums, type aliases)
//! - Trait definitions and implementations
//! - Generic instantiations

use std::collections::HashMap;
use std::sync::Arc;

use super::ty::*;

/// The type context holds all type information during type checking.
#[derive(Debug)]
pub struct TypeContext {
    /// Stack of scopes for variable bindings.
    scopes: Vec<Scope>,

    /// Type definitions (structs, enums).
    types: HashMap<DefId, TypeDef>,

    /// Trait definitions.
    traits: HashMap<DefId, TraitDef>,

    /// Trait implementations.
    impls: Vec<TraitImpl>,

    /// Type aliases.
    aliases: HashMap<DefId, TypeAlias>,

    /// Function signatures.
    functions: HashMap<DefId, FnSig>,

    /// Next definition ID.
    next_def_id: u32,

    /// The current Self type (set when inside an impl block).
    current_self_ty: Option<Ty>,

    /// Inherent methods: (type_name, method_name) → method signature.
    /// Populated by check_inherent_impl so that lookup_method can resolve
    /// method calls on user-defined types without a trait.
    inherent_methods: HashMap<(Arc<str>, Arc<str>), TraitMethod>,

    /// Type parameter trait bounds: param_name → [trait_name, ...].
    /// Populated when entering a generic function so that lookup_method can
    /// resolve trait methods on type parameters through their bounds.
    param_trait_bounds: HashMap<Arc<str>, Vec<Arc<str>>>,

    /// Module registry: module_name -> (variable bindings, type definitions).
    /// Populated by check_mod so that `use` statements can import names.
    module_bindings: HashMap<Arc<str>, HashMap<Arc<str>, TypeScheme>>,
}

/// A scope containing variable bindings.
#[derive(Debug, Clone)]
struct Scope {
    /// Variable bindings: name -> type scheme.
    bindings: HashMap<Arc<str>, TypeScheme>,
    /// Type parameters in scope.
    type_params: HashMap<Arc<str>, Ty>,
    /// Kind of scope.
    kind: ScopeKind,
}

/// Kind of scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// Global/module scope.
    Module,
    /// Function body scope.
    Function,
    /// Block scope.
    Block,
    /// Loop scope.
    Loop,
    /// Match arm scope.
    Match,
}

impl Scope {
    fn new(kind: ScopeKind) -> Self {
        Self {
            bindings: HashMap::new(),
            type_params: HashMap::new(),
            kind,
        }
    }
}

/// A type definition (struct or enum).
#[derive(Debug, Clone)]
pub struct TypeDef {
    /// Definition ID.
    pub def_id: DefId,
    /// Name.
    pub name: Arc<str>,
    /// Generic parameters.
    pub generics: Vec<GenericParam>,
    /// Kind of type.
    pub kind: TypeDefKind,
}

/// Kind of type definition.
#[derive(Debug, Clone)]
pub enum TypeDefKind {
    /// Struct.
    Struct(StructDef),
    /// Enum.
    Enum(EnumDef),
}

/// Struct definition.
#[derive(Debug, Clone)]
pub struct StructDef {
    /// Fields (name -> type).
    pub fields: Vec<(Arc<str>, Ty)>,
    /// Is this a tuple struct?
    pub is_tuple: bool,
}

/// Enum definition.
#[derive(Debug, Clone)]
pub struct EnumDef {
    /// Variants.
    pub variants: Vec<EnumVariant>,
}

/// Enum variant.
#[derive(Debug, Clone)]
pub struct EnumVariant {
    /// Variant name.
    pub name: Arc<str>,
    /// Fields (if any).
    pub fields: Vec<(Option<Arc<str>>, Ty)>,
    /// Discriminant (if specified).
    pub discriminant: Option<i128>,
}

/// A generic parameter.
#[derive(Debug, Clone)]
pub struct GenericParam {
    /// Parameter name.
    pub name: Arc<str>,
    /// Parameter index.
    pub index: u32,
    /// Kind of parameter.
    pub kind: GenericParamKind,
}

/// Kind of generic parameter.
#[derive(Debug, Clone)]
pub enum GenericParamKind {
    /// Type parameter with optional bounds.
    Type { bounds: Vec<TraitBound> },
    /// Lifetime parameter.
    Lifetime,
    /// Const parameter.
    Const { ty: Ty },
}

/// A trait bound.
#[derive(Debug, Clone)]
pub struct TraitBound {
    /// The trait being bounded.
    pub trait_id: DefId,
    /// Type arguments for the trait.
    pub args: Vec<Ty>,
}

/// A trait definition.
#[derive(Debug, Clone)]
pub struct TraitDef {
    /// Definition ID.
    pub def_id: DefId,
    /// Name.
    pub name: Arc<str>,
    /// Generic parameters.
    pub generics: Vec<GenericParam>,
    /// Super traits.
    pub supertraits: Vec<TraitBound>,
    /// Associated types.
    pub assoc_types: Vec<AssocType>,
    /// Methods.
    pub methods: Vec<TraitMethod>,
}

/// An associated type in a trait.
#[derive(Debug, Clone)]
pub struct AssocType {
    /// Name.
    pub name: Arc<str>,
    /// Bounds.
    pub bounds: Vec<TraitBound>,
    /// Default type (if any).
    pub default: Option<Ty>,
}

/// A method in a trait.
#[derive(Debug, Clone)]
pub struct TraitMethod {
    /// Name.
    pub name: Arc<str>,
    /// Signature.
    pub sig: FnSig,
    /// Has default implementation?
    pub has_default: bool,
}

/// A trait implementation.
#[derive(Debug, Clone)]
pub struct TraitImpl {
    /// The trait being implemented.
    pub trait_id: DefId,
    /// The type implementing the trait.
    pub self_ty: Ty,
    /// Generic parameters on the impl.
    pub generics: Vec<GenericParam>,
    /// Associated type values.
    pub assoc_types: HashMap<Arc<str>, Ty>,
    /// Method implementations.
    pub methods: HashMap<Arc<str>, DefId>,
    /// Where clauses.
    pub where_clauses: Vec<WhereClause>,
}

/// A where clause.
#[derive(Debug, Clone)]
pub struct WhereClause {
    /// The type being constrained.
    pub ty: Ty,
    /// The bounds.
    pub bounds: Vec<TraitBound>,
}

/// A type alias.
#[derive(Debug, Clone)]
pub struct TypeAlias {
    /// Definition ID.
    pub def_id: DefId,
    /// Name.
    pub name: Arc<str>,
    /// Generic parameters.
    pub generics: Vec<GenericParam>,
    /// The aliased type.
    pub ty: Ty,
}

/// A function signature.
#[derive(Debug, Clone)]
pub struct FnSig {
    /// Generic parameters.
    pub generics: Vec<GenericParam>,
    /// Lifetime parameters (e.g., 'a, 'b in `fn foo<'a, 'b>(...)`)
    pub lifetime_params: Vec<Arc<str>>,
    /// Parameter types (with names).
    pub params: Vec<(Arc<str>, Ty)>,
    /// Return type.
    pub ret: Ty,
    /// Is unsafe?
    pub is_unsafe: bool,
    /// Is async?
    pub is_async: bool,
    /// Is const?
    pub is_const: bool,
    /// Where clauses.
    pub where_clauses: Vec<WhereClause>,
}

impl TypeContext {
    /// Create a new type context.
    pub fn new() -> Self {
        let mut ctx = Self {
            scopes: vec![Scope::new(ScopeKind::Module)],
            types: HashMap::new(),
            traits: HashMap::new(),
            impls: Vec::new(),
            aliases: HashMap::new(),
            functions: HashMap::new(),
            next_def_id: 0,
            current_self_ty: None,
            inherent_methods: HashMap::new(),
            param_trait_bounds: HashMap::new(),
            module_bindings: HashMap::new(),
        };
        ctx.init_builtins();
        ctx
    }

    /// Initialize built-in types.
    fn init_builtins(&mut self) {
        // Trait stubs are registered lazily during check_module to avoid
        // shifting DefIds for user types. See register_builtin_traits().
    }

    /// Register common trait stubs so ecosystem code can use `impl Default for X` etc.
    /// Called during check_module AFTER user types are collected.
    pub fn register_builtin_traits(&mut self) {
        let common_traits = [
            "Default", "Display", "Debug", "Clone", "Copy",
            "PartialEq", "Eq", "PartialOrd", "Ord", "Hash",
            "Send", "Sync", "Sized", "Drop",
            "Iterator", "IntoIterator", "FromIterator",
            "From", "Into", "TryFrom", "TryInto",
            "AsRef", "AsMut", "Deref", "DerefMut",
            "Add", "Sub", "Mul", "Div", "Rem", "Neg",
            "Serialize", "Deserialize",
        ];
        for name in &common_traits {
            // Only register if not already defined by user code
            if self.lookup_trait_by_name(name).is_none() {
                let def_id = self.fresh_def_id();
                self.register_trait(TraitDef {
                    def_id,
                    name: Arc::from(*name),
                    generics: Vec::new(),
                    supertraits: Vec::new(),
                    methods: Vec::new(),
                    assoc_types: Vec::new(),
                });
            }
        }
    }

    /// Generate a fresh definition ID.
    pub fn fresh_def_id(&mut self) -> DefId {
        let id = DefId::new(0, self.next_def_id);
        self.next_def_id += 1;
        id
    }

    // =========================================================================
    // SCOPE MANAGEMENT
    // =========================================================================

    /// Push a new scope.
    pub fn push_scope(&mut self, kind: ScopeKind) {
        self.scopes.push(Scope::new(kind));
    }

    /// Pop the current scope.
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Get the current scope kind.
    pub fn current_scope_kind(&self) -> ScopeKind {
        self.scopes.last().map(|s| s.kind).unwrap_or(ScopeKind::Module)
    }

    /// Check if we're inside a loop.
    pub fn in_loop(&self) -> bool {
        self.scopes.iter().any(|s| s.kind == ScopeKind::Loop)
    }

    /// Check if we're inside a function.
    pub fn in_function(&self) -> bool {
        self.scopes.iter().any(|s| s.kind == ScopeKind::Function)
    }

    // =========================================================================
    // SELF TYPE
    // =========================================================================

    /// Set the current Self type (used when entering an impl block).
    pub fn set_self_ty(&mut self, ty: Option<Ty>) {
        self.current_self_ty = ty;
    }

    /// Get the current Self type.
    pub fn get_self_ty(&self) -> Option<&Ty> {
        self.current_self_ty.as_ref()
    }

    // =========================================================================
    // VARIABLE BINDINGS
    // =========================================================================

    /// Define a variable in the current scope.
    pub fn define_var(&mut self, name: impl Into<Arc<str>>, ty: Ty) {
        let name = name.into();
        if let Some(scope) = self.scopes.last_mut() {
            scope.bindings.insert(name, TypeScheme::mono(ty));
        }
    }

    /// Define a variable with a type scheme (polymorphic).
    pub fn define_var_scheme(&mut self, name: impl Into<Arc<str>>, scheme: TypeScheme) {
        let name = name.into();
        if let Some(scope) = self.scopes.last_mut() {
            scope.bindings.insert(name, scheme);
        }
    }

    /// Look up a variable.
    pub fn lookup_var(&self, name: &str) -> Option<Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(scheme) = scope.bindings.get(name) {
                return Some(scheme.instantiate());
            }
        }
        None
    }

    /// Look up a variable's type scheme.
    pub fn lookup_var_scheme(&self, name: &str) -> Option<&TypeScheme> {
        for scope in self.scopes.iter().rev() {
            if let Some(scheme) = scope.bindings.get(name) {
                return Some(scheme);
            }
        }
        None
    }

    // =========================================================================
    // TYPE PARAMETERS
    // =========================================================================

    /// Define a type parameter in the current scope.
    pub fn define_type_param(&mut self, name: impl Into<Arc<str>>, ty: Ty) {
        let name = name.into();
        if let Some(scope) = self.scopes.last_mut() {
            scope.type_params.insert(name, ty);
        }
    }

    /// Look up a type parameter.
    pub fn lookup_type_param(&self, name: &str) -> Option<&Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.type_params.get(name) {
                return Some(ty);
            }
        }
        None
    }

    // =========================================================================
    // TYPE DEFINITIONS
    // =========================================================================

    /// Register a type definition.
    pub fn register_type(&mut self, def: TypeDef) {
        self.types.insert(def.def_id, def);
    }

    /// Look up a type definition by ID.
    pub fn lookup_type(&self, def_id: DefId) -> Option<&TypeDef> {
        self.types.get(&def_id)
    }

    /// Look up a type definition by name.
    pub fn lookup_type_by_name(&self, name: &str) -> Option<&TypeDef> {
        self.types.values().find(|t| t.name.as_ref() == name)
    }

    // =========================================================================
    // TRAIT DEFINITIONS
    // =========================================================================

    /// Register a trait definition.
    pub fn register_trait(&mut self, def: TraitDef) {
        self.traits.insert(def.def_id, def);
    }

    /// Look up a trait definition.
    pub fn lookup_trait(&self, def_id: DefId) -> Option<&TraitDef> {
        self.traits.get(&def_id)
    }

    /// Look up a trait by name.
    pub fn lookup_trait_by_name(&self, name: &str) -> Option<&TraitDef> {
        self.traits.values().find(|t| t.name.as_ref() == name)
    }

    // =========================================================================
    // TRAIT IMPLEMENTATIONS
    // =========================================================================

    /// Register a trait implementation.
    pub fn register_impl(&mut self, impl_: TraitImpl) {
        self.impls.push(impl_);
    }

    /// Find implementations of a trait for a type.
    pub fn find_impls(&self, trait_id: DefId, _self_ty: &Ty) -> Vec<&TraitImpl> {
        self.impls
            .iter()
            .filter(|impl_| impl_.trait_id == trait_id)
            // TODO: proper type matching with unification
            .collect()
    }

    /// Look up a method available on a type via trait implementations.
    ///
    /// Searches all registered trait impls whose `self_ty` matches the given
    /// type and returns the trait method signature if found.
    pub fn lookup_trait_method(&self, self_ty: &Ty, method_name: &str) -> Option<&TraitMethod> {
        // Find trait impls whose self_ty matches
        for impl_ in &self.impls {
            let matches = match (&impl_.self_ty.kind, &self_ty.kind) {
                (TyKind::Adt(d1, _), TyKind::Adt(d2, _)) => d1 == d2,
                _ => false,
            };
            if matches && impl_.methods.contains_key(method_name) {
                // Look up the method signature from the trait definition
                if let Some(trait_def) = self.traits.get(&impl_.trait_id) {
                    if let Some(method) = trait_def.methods.iter().find(|m| m.name.as_ref() == method_name) {
                        return Some(method);
                    }
                }
            }
        }
        None
    }

    // =========================================================================
    // INHERENT METHODS
    // =========================================================================

    /// Register an inherent method on a user-defined type.
    /// This is used for `impl Type { fn method(...) }` without a trait.
    pub fn register_inherent_method(&mut self, type_name: Arc<str>, method_name: Arc<str>, sig: FnSig) {
        self.inherent_methods.insert(
            (type_name, method_name.clone()),
            TraitMethod {
                name: method_name,
                sig,
                has_default: false,
            },
        );
    }

    /// Look up an inherent method on a type by name.
    pub fn lookup_inherent_method(&self, type_name: &str, method_name: &str) -> Option<&TraitMethod> {
        self.inherent_methods.get(&(Arc::from(type_name), Arc::from(method_name)))
    }

    /// Find the type name that has an inherent method with the given name.
    /// Used as a fallback when DefId-based type lookup fails.
    pub fn lookup_type_by_name_containing_method(&self, method_name: &str) -> Option<String> {
        let method_arc = Arc::from(method_name);
        for (type_name, mname) in self.inherent_methods.keys() {
            if mname == &method_arc {
                return Some(type_name.to_string());
            }
        }
        None
    }

    // =========================================================================
    // TYPE PARAMETER TRAIT BOUNDS
    // =========================================================================

    /// Register trait bounds for a type parameter (e.g., `T: Clone + Debug`).
    pub fn register_param_bounds(&mut self, param_name: Arc<str>, trait_names: Vec<Arc<str>>) {
        self.param_trait_bounds.insert(param_name, trait_names);
    }

    /// Clear all type parameter trait bounds (call when leaving a generic scope).
    pub fn clear_param_bounds(&mut self) {
        self.param_trait_bounds.clear();
    }

    /// Look up a method on a type parameter by searching its trait bounds.
    ///
    /// For `T: Ord + Display`, this searches the `Ord` and `Display` trait
    /// definitions for the named method and returns the first match.
    pub fn lookup_param_method(&self, param_name: &str, method_name: &str) -> Option<&TraitMethod> {
        let bounds = self.param_trait_bounds.get(param_name)?;
        for trait_name in bounds {
            // Find the trait definition by name
            if let Some(trait_def) = self.lookup_trait_by_name(trait_name) {
                // Search the trait's methods
                if let Some(method) = trait_def.methods.iter().find(|m| m.name.as_ref() == method_name) {
                    return Some(method);
                }
            }
        }
        None
    }

    // =========================================================================
    // MODULE BINDINGS
    // =========================================================================

    /// Register a module's exported bindings for use-statement resolution.
    pub fn register_module_bindings(&mut self, name: Arc<str>, bindings: HashMap<Arc<str>, TypeScheme>) {
        self.module_bindings.insert(name, bindings);
    }

    /// Look up a binding in a named module.
    pub fn lookup_module_binding(&self, module: &str, name: &str) -> Option<Ty> {
        self.module_bindings.get(module)?
            .get(name)
            .map(|scheme| scheme.instantiate())
    }

    /// Return a clone of the current scope's variable bindings.
    pub fn current_scope_bindings(&self) -> HashMap<Arc<str>, TypeScheme> {
        self.scopes.last()
            .map(|s| s.bindings.clone())
            .unwrap_or_default()
    }

    /// Return a clone of a named module's bindings (for glob imports).
    pub fn clone_module_bindings(&self, module: &str) -> Option<HashMap<Arc<str>, TypeScheme>> {
        self.module_bindings.get(module).cloned()
    }

    // =========================================================================
    // TYPE ALIASES
    // =========================================================================

    /// Register a type alias.
    pub fn register_alias(&mut self, alias: TypeAlias) {
        self.aliases.insert(alias.def_id, alias);
    }

    /// Look up a type alias.
    pub fn lookup_alias(&self, def_id: DefId) -> Option<&TypeAlias> {
        self.aliases.get(&def_id)
    }

    // =========================================================================
    // FUNCTIONS
    // =========================================================================

    /// Register a function signature.
    pub fn register_function(&mut self, def_id: DefId, sig: FnSig) {
        self.functions.insert(def_id, sig);
    }

    /// Look up a function signature.
    pub fn lookup_function(&self, def_id: DefId) -> Option<&FnSig> {
        self.functions.get(&def_id)
    }

    // =========================================================================
    // GENERALIZATION
    // =========================================================================

    /// Generalize a type to a type scheme by quantifying over free variables
    /// that are not in the environment.
    pub fn generalize(&self, ty: &Ty) -> TypeScheme {
        let free_in_ty = ty.free_vars();
        let free_in_env = self.free_vars_in_env();

        let vars: Vec<_> = free_in_ty
            .difference(&free_in_env)
            .cloned()
            .collect();

        TypeScheme::poly(vars, ty.clone())
    }

    /// Collect all free type variables in the environment.
    fn free_vars_in_env(&self) -> std::collections::HashSet<TyVarId> {
        let mut vars = std::collections::HashSet::new();
        for scope in &self.scopes {
            for scheme in scope.bindings.values() {
                // Don't include bound variables from the scheme
                let free = scheme.ty.free_vars();
                for var in free {
                    if !scheme.vars.contains(&var) {
                        vars.insert(var);
                    }
                }
            }
        }
        vars
    }
}

impl Default for TypeContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_management() {
        let mut ctx = TypeContext::new();

        ctx.define_var("x", Ty::int(IntTy::I32));
        assert!(ctx.lookup_var("x").is_some());

        ctx.push_scope(ScopeKind::Block);
        ctx.define_var("y", Ty::bool());
        assert!(ctx.lookup_var("x").is_some());
        assert!(ctx.lookup_var("y").is_some());

        ctx.pop_scope();
        assert!(ctx.lookup_var("x").is_some());
        assert!(ctx.lookup_var("y").is_none()); // y is out of scope
    }

    #[test]
    fn test_generalization() {
        let ctx = TypeContext::new();

        let v = TyVarId::fresh();
        let ty = Ty::function(vec![Ty::var(v)], Ty::var(v));

        let scheme = ctx.generalize(&ty);
        assert_eq!(scheme.vars.len(), 1);
    }
}
