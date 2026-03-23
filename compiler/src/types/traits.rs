// ===============================================================================
// QUANTALANG TYPE SYSTEM - TRAIT RESOLUTION
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Trait resolution and associated types.
//!
//! This module implements trait bounds checking, impl selection, and associated
//! type projection.

use std::collections::HashMap;
use std::sync::Arc;

use super::ty::*;
use super::error::{TypeError, TypeResult};

// =============================================================================
// TRAIT DEFINITIONS
// =============================================================================

/// A unique identifier for a trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraitId(pub u32);

impl TraitId {
    /// Generate a fresh trait ID.
    pub fn fresh() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        TraitId(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

/// A trait definition.
#[derive(Debug, Clone)]
pub struct TraitDef {
    /// The trait's unique ID.
    pub id: TraitId,
    /// The trait's name.
    pub name: Arc<str>,
    /// Type parameters for the trait.
    pub type_params: Vec<TypeParam>,
    /// Associated types declared by this trait.
    pub assoc_types: Vec<AssocTypeDef>,
    /// Associated constants declared by this trait.
    pub assoc_consts: Vec<AssocConstDef>,
    /// Required methods (signatures only).
    pub required_methods: Vec<MethodSig>,
    /// Provided methods (with default implementations).
    pub provided_methods: Vec<MethodDef>,
    /// Supertraits that this trait extends.
    pub supertraits: Vec<TraitRef>,
}

/// A type parameter in a trait definition.
#[derive(Debug, Clone)]
pub struct TypeParam {
    /// The parameter's name.
    pub name: Arc<str>,
    /// Bounds on the type parameter.
    pub bounds: Vec<TraitRef>,
    /// Default type if any.
    pub default: Option<Ty>,
}

/// An associated type definition.
#[derive(Debug, Clone)]
pub struct AssocTypeDef {
    /// The associated type's name.
    pub name: Arc<str>,
    /// Bounds on the associated type.
    pub bounds: Vec<TraitRef>,
    /// Default type if any.
    pub default: Option<Ty>,
}

/// An associated constant definition.
#[derive(Debug, Clone)]
pub struct AssocConstDef {
    /// The constant's name.
    pub name: Arc<str>,
    /// The constant's type.
    pub ty: Ty,
    /// Default value expression (as string for now).
    pub default: Option<Arc<str>>,
}

/// A method signature.
#[derive(Debug, Clone)]
pub struct MethodSig {
    /// The method's name.
    pub name: Arc<str>,
    /// Whether the method is unsafe.
    pub is_unsafe: bool,
    /// Whether the method is async.
    pub is_async: bool,
    /// The receiver type (self, &self, &mut self, etc.).
    pub receiver: ReceiverKind,
    /// The method's type parameters.
    pub type_params: Vec<TypeParam>,
    /// The method's parameter types (excluding receiver).
    pub params: Vec<Ty>,
    /// The return type.
    pub return_ty: Ty,
}

/// A method with a default implementation.
#[derive(Debug, Clone)]
pub struct MethodDef {
    /// The method signature.
    pub sig: MethodSig,
    // Body would be stored elsewhere (AST reference or IR)
}

/// The kind of receiver a method takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverKind {
    /// No receiver (associated function).
    None,
    /// Takes `self` by value.
    Value,
    /// Takes `&self`.
    Ref,
    /// Takes `&mut self`.
    RefMut,
    /// Takes `Box<Self>`.
    Box,
    /// Takes `Rc<Self>`.
    Rc,
    /// Takes `Arc<Self>`.
    Arc,
    /// Takes `Pin<&Self>`.
    Pin,
    /// Takes `Pin<&mut Self>`.
    PinMut,
}

// =============================================================================
// TRAIT REFERENCES
// =============================================================================

/// A reference to a trait with type arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitRef {
    /// The trait being referenced.
    pub trait_id: TraitId,
    /// Type arguments to the trait.
    pub substs: Vec<Ty>,
}

impl TraitRef {
    /// Create a new trait reference.
    pub fn new(trait_id: TraitId, substs: Vec<Ty>) -> Self {
        Self { trait_id, substs }
    }

    /// Create a trait reference with no type arguments.
    pub fn simple(trait_id: TraitId) -> Self {
        Self { trait_id, substs: Vec::new() }
    }
}

/// A trait bound with optional associated type constraints.
#[derive(Debug, Clone)]
pub struct TraitBound {
    /// The trait reference.
    pub trait_ref: TraitRef,
    /// Associated type constraints (e.g., `Iterator<Item = u32>`).
    pub assoc_type_constraints: Vec<AssocTypeConstraint>,
}

/// A constraint on an associated type.
#[derive(Debug, Clone)]
pub struct AssocTypeConstraint {
    /// The associated type's name.
    pub name: Arc<str>,
    /// The type it should equal.
    pub ty: Ty,
}

// =============================================================================
// IMPL DEFINITIONS
// =============================================================================

/// A unique identifier for an impl block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImplId(pub u32);

impl ImplId {
    /// Generate a fresh impl ID.
    pub fn fresh() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        ImplId(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

/// An impl block.
#[derive(Debug, Clone)]
pub struct ImplDef {
    /// The impl's unique ID.
    pub id: ImplId,
    /// Type parameters for the impl.
    pub type_params: Vec<TypeParam>,
    /// The trait being implemented (None for inherent impl).
    pub trait_ref: Option<TraitRef>,
    /// The type this impl is for (Self type).
    pub self_ty: Ty,
    /// Where clause predicates.
    pub where_predicates: Vec<WherePredicate>,
    /// Associated type definitions.
    pub assoc_types: HashMap<Arc<str>, Ty>,
    /// Associated const definitions.
    pub assoc_consts: HashMap<Arc<str>, (Ty, Option<Arc<str>>)>,
    /// Method implementations.
    pub methods: HashMap<Arc<str>, MethodDef>,
    /// Whether this is a negative impl (e.g., `impl !Send for Foo`).
    pub is_negative: bool,
}

/// A where predicate.
#[derive(Debug, Clone)]
pub struct WherePredicate {
    /// The type being constrained.
    pub ty: Ty,
    /// The trait bounds on the type.
    pub bounds: Vec<TraitBound>,
}

// =============================================================================
// TRAIT ENVIRONMENT
// =============================================================================

/// The trait environment containing all trait and impl definitions.
#[derive(Debug, Clone, Default)]
pub struct TraitEnv {
    /// All trait definitions by ID.
    pub traits: HashMap<TraitId, TraitDef>,
    /// Trait definitions by name (for lookup).
    pub trait_names: HashMap<Arc<str>, TraitId>,
    /// All impl definitions by ID.
    pub impls: HashMap<ImplId, ImplDef>,
    /// Impls indexed by trait (for trait impl lookup).
    pub impls_for_trait: HashMap<TraitId, Vec<ImplId>>,
    /// Inherent impls indexed by type (for method resolution).
    pub inherent_impls: HashMap<DefId, Vec<ImplId>>,
}

impl TraitEnv {
    /// Create a new empty trait environment.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a trait definition.
    pub fn register_trait(&mut self, def: TraitDef) {
        let id = def.id;
        let name = def.name.clone();
        self.traits.insert(id, def);
        self.trait_names.insert(name, id);
    }

    /// Register an impl definition.
    pub fn register_impl(&mut self, def: ImplDef) {
        let id = def.id;
        if let Some(ref trait_ref) = def.trait_ref {
            self.impls_for_trait
                .entry(trait_ref.trait_id)
                .or_default()
                .push(id);
        } else {
            // Inherent impl
            if let TyKind::Adt(def_id, _) = &def.self_ty.kind {
                self.inherent_impls
                    .entry(*def_id)
                    .or_default()
                    .push(id);
            }
        }
        self.impls.insert(id, def);
    }

    /// Look up a trait by name.
    pub fn lookup_trait(&self, name: &str) -> Option<&TraitDef> {
        self.trait_names.get(name).and_then(|id| self.traits.get(id))
    }

    /// Look up a trait by ID.
    pub fn get_trait(&self, id: TraitId) -> Option<&TraitDef> {
        self.traits.get(&id)
    }

    /// Get all impls for a trait.
    pub fn impls_for(&self, trait_id: TraitId) -> impl Iterator<Item = &ImplDef> {
        self.impls_for_trait
            .get(&trait_id)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|id| self.impls.get(id))
    }

    /// Get inherent impls for a type.
    pub fn inherent_impls_for(&self, def_id: DefId) -> impl Iterator<Item = &ImplDef> {
        self.inherent_impls
            .get(&def_id)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|id| self.impls.get(id))
    }
}

// =============================================================================
// TRAIT RESOLUTION
// =============================================================================

/// The trait resolver.
pub struct TraitResolver<'env> {
    /// The trait environment.
    env: &'env TraitEnv,
    /// Cache of resolved trait impls.
    cache: HashMap<(Ty, TraitId), Option<ImplId>>,
}

impl<'env> TraitResolver<'env> {
    /// Create a new trait resolver.
    pub fn new(env: &'env TraitEnv) -> Self {
        Self {
            env,
            cache: HashMap::new(),
        }
    }

    /// Check if a type implements a trait.
    pub fn implements(&mut self, ty: &Ty, trait_id: TraitId) -> bool {
        self.resolve_impl(ty, trait_id).is_some()
    }

    /// Resolve which impl (if any) provides a trait for a type.
    pub fn resolve_impl(&mut self, ty: &Ty, trait_id: TraitId) -> Option<ImplId> {
        // Check cache first
        let key = (ty.clone(), trait_id);
        if let Some(cached) = self.cache.get(&key) {
            return *cached;
        }

        // Try to find a matching impl
        let result = self.find_impl(ty, trait_id);
        self.cache.insert(key, result);
        result
    }

    /// Find an impl that provides a trait for a type.
    fn find_impl(&self, ty: &Ty, trait_id: TraitId) -> Option<ImplId> {
        for impl_def in self.env.impls_for(trait_id) {
            if self.impl_applies(impl_def, ty) {
                return Some(impl_def.id);
            }
        }
        None
    }

    /// Check if an impl applies to a type.
    fn impl_applies(&self, impl_def: &ImplDef, ty: &Ty) -> bool {
        // Try to unify the impl's self type with the given type
        self.types_unify(&impl_def.self_ty, ty)
    }

    /// Check if two types can unify (simplified version).
    fn types_unify(&self, t1: &Ty, t2: &Ty) -> bool {
        // Simplified unification for impl selection
        match (&t1.kind, &t2.kind) {
            (TyKind::Var(_), _) | (_, TyKind::Var(_)) => true,
            (TyKind::Infer(_), _) | (_, TyKind::Infer(_)) => true,
            (TyKind::Error, _) | (_, TyKind::Error) => true,
            (TyKind::Never, _) | (_, TyKind::Never) => true,

            (TyKind::Int(i1), TyKind::Int(i2)) => i1 == i2,
            (TyKind::Float(f1), TyKind::Float(f2)) => f1 == f2,
            (TyKind::Bool, TyKind::Bool) => true,
            (TyKind::Char, TyKind::Char) => true,
            (TyKind::Str, TyKind::Str) => true,

            (TyKind::Tuple(e1), TyKind::Tuple(e2)) => {
                e1.len() == e2.len() && e1.iter().zip(e2.iter()).all(|(a, b)| self.types_unify(a, b))
            }
            (TyKind::Array(e1, l1), TyKind::Array(e2, l2)) => {
                l1 == l2 && self.types_unify(e1, e2)
            }
            (TyKind::Slice(e1), TyKind::Slice(e2)) => self.types_unify(e1, e2),
            (TyKind::Ref(_, m1, t1), TyKind::Ref(_, m2, t2)) => {
                m1 == m2 && self.types_unify(t1, t2)
            }
            (TyKind::Ptr(m1, t1), TyKind::Ptr(m2, t2)) => {
                m1 == m2 && self.types_unify(t1, t2)
            }
            (TyKind::Fn(f1), TyKind::Fn(f2)) => {
                f1.params.len() == f2.params.len()
                    && f1.is_unsafe == f2.is_unsafe
                    && f1.params.iter().zip(f2.params.iter()).all(|(a, b)| self.types_unify(a, b))
                    && self.types_unify(&f1.ret, &f2.ret)
            }
            (TyKind::Adt(d1, a1), TyKind::Adt(d2, a2)) => {
                d1 == d2 && a1.len() == a2.len()
                    && a1.iter().zip(a2.iter()).all(|(a, b)| self.types_unify(a, b))
            }
            (TyKind::Param(n1, _), TyKind::Param(n2, _)) => n1 == n2,

            // Type parameter on one side can match anything (simplified)
            (TyKind::Param(_, _), _) | (_, TyKind::Param(_, _)) => true,

            _ => false,
        }
    }

    /// Resolve an associated type.
    pub fn resolve_assoc_type(
        &mut self,
        ty: &Ty,
        trait_id: TraitId,
        assoc_name: &str,
    ) -> TypeResult<Ty> {
        let impl_id = self.resolve_impl(ty, trait_id).ok_or_else(|| {
            TypeError::TraitNotImplemented {
                ty: ty.clone(),
                trait_id,
            }
        })?;

        let impl_def = self.env.impls.get(&impl_id).ok_or_else(|| {
            TypeError::InternalError("impl not found".into())
        })?;

        impl_def.assoc_types.get(assoc_name).cloned().ok_or_else(|| {
            TypeError::AssociatedTypeNotDefined {
                assoc_name: assoc_name.to_string(),
            }
        })
    }

    /// Resolve a method.
    pub fn resolve_method(
        &mut self,
        ty: &Ty,
        method_name: &str,
    ) -> Option<(ImplId, &'env MethodDef)> {
        // First check inherent impls
        if let TyKind::Adt(def_id, _) = &ty.kind {
            for impl_def in self.env.inherent_impls_for(*def_id) {
                if let Some(method) = impl_def.methods.get(method_name) {
                    return Some((impl_def.id, method));
                }
            }
        }

        // Then check trait impls
        for impl_def in self.env.impls.values() {
            if self.impl_applies(impl_def, ty) {
                if let Some(method) = impl_def.methods.get(method_name) {
                    return Some((impl_def.id, method));
                }
            }
        }

        None
    }
}

// =============================================================================
// BUILT-IN TRAITS
// =============================================================================

/// Well-known built-in trait IDs.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinTraits {
    pub copy: TraitId,
    pub clone: TraitId,
    pub sized: TraitId,
    pub send: TraitId,
    pub sync: TraitId,
    pub drop: TraitId,
    pub fn_: TraitId,
    pub fn_mut: TraitId,
    pub fn_once: TraitId,
    pub iterator: TraitId,
    pub into_iterator: TraitId,
    pub future: TraitId,
    pub try_: TraitId,
    pub add: TraitId,
    pub sub: TraitId,
    pub mul: TraitId,
    pub div: TraitId,
    pub neg: TraitId,
    pub not: TraitId,
    pub eq: TraitId,
    pub partial_eq: TraitId,
    pub ord: TraitId,
    pub partial_ord: TraitId,
    pub hash: TraitId,
    pub debug: TraitId,
    pub display: TraitId,
    pub default: TraitId,
    pub from: TraitId,
    pub into: TraitId,
    pub deref: TraitId,
    pub deref_mut: TraitId,
    pub index: TraitId,
    pub index_mut: TraitId,
}

impl BuiltinTraits {
    /// Create the built-in traits and register them in the environment.
    pub fn new(env: &mut TraitEnv) -> Self {
        let mut create_trait = |name: &str, assoc_types: Vec<&str>| {
            let id = TraitId::fresh();
            let def = TraitDef {
                id,
                name: name.into(),
                type_params: Vec::new(),
                assoc_types: assoc_types.into_iter().map(|n| AssocTypeDef {
                    name: n.into(),
                    bounds: Vec::new(),
                    default: None,
                }).collect(),
                assoc_consts: Vec::new(),
                required_methods: Vec::new(),
                provided_methods: Vec::new(),
                supertraits: Vec::new(),
            };
            env.register_trait(def);
            id
        };

        Self {
            copy: create_trait("Copy", vec![]),
            clone: create_trait("Clone", vec![]),
            sized: create_trait("Sized", vec![]),
            send: create_trait("Send", vec![]),
            sync: create_trait("Sync", vec![]),
            drop: create_trait("Drop", vec![]),
            fn_: create_trait("Fn", vec!["Output"]),
            fn_mut: create_trait("FnMut", vec!["Output"]),
            fn_once: create_trait("FnOnce", vec!["Output"]),
            iterator: create_trait("Iterator", vec!["Item"]),
            into_iterator: create_trait("IntoIterator", vec!["Item", "IntoIter"]),
            future: create_trait("Future", vec!["Output"]),
            try_: create_trait("Try", vec!["Output", "Residual"]),
            add: create_trait("Add", vec!["Output"]),
            sub: create_trait("Sub", vec!["Output"]),
            mul: create_trait("Mul", vec!["Output"]),
            div: create_trait("Div", vec!["Output"]),
            neg: create_trait("Neg", vec!["Output"]),
            not: create_trait("Not", vec!["Output"]),
            eq: create_trait("Eq", vec![]),
            partial_eq: create_trait("PartialEq", vec![]),
            ord: create_trait("Ord", vec![]),
            partial_ord: create_trait("PartialOrd", vec![]),
            hash: create_trait("Hash", vec![]),
            debug: create_trait("Debug", vec![]),
            display: create_trait("Display", vec![]),
            default: create_trait("Default", vec![]),
            from: create_trait("From", vec![]),
            into: create_trait("Into", vec![]),
            deref: create_trait("Deref", vec!["Target"]),
            deref_mut: create_trait("DerefMut", vec![]),
            index: create_trait("Index", vec!["Output"]),
            index_mut: create_trait("IndexMut", vec![]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_registration() {
        let mut env = TraitEnv::new();
        let id = TraitId::fresh();
        let def = TraitDef {
            id,
            name: "TestTrait".into(),
            type_params: Vec::new(),
            assoc_types: Vec::new(),
            assoc_consts: Vec::new(),
            required_methods: Vec::new(),
            provided_methods: Vec::new(),
            supertraits: Vec::new(),
        };
        env.register_trait(def);

        assert!(env.lookup_trait("TestTrait").is_some());
        assert!(env.get_trait(id).is_some());
    }

    #[test]
    fn test_impl_resolution() {
        let mut env = TraitEnv::new();

        // Create a trait
        let trait_id = TraitId::fresh();
        let trait_def = TraitDef {
            id: trait_id,
            name: "Display".into(),
            type_params: Vec::new(),
            assoc_types: Vec::new(),
            assoc_consts: Vec::new(),
            required_methods: Vec::new(),
            provided_methods: Vec::new(),
            supertraits: Vec::new(),
        };
        env.register_trait(trait_def);

        // Create an impl for i32
        let impl_id = ImplId::fresh();
        let impl_def = ImplDef {
            id: impl_id,
            type_params: Vec::new(),
            trait_ref: Some(TraitRef::simple(trait_id)),
            self_ty: Ty::int(IntTy::I32),
            where_predicates: Vec::new(),
            assoc_types: HashMap::new(),
            assoc_consts: HashMap::new(),
            methods: HashMap::new(),
            is_negative: false,
        };
        env.register_impl(impl_def);

        // Resolve
        let mut resolver = TraitResolver::new(&env);
        assert!(resolver.implements(&Ty::int(IntTy::I32), trait_id));
        assert!(!resolver.implements(&Ty::bool(), trait_id));
    }

    #[test]
    fn test_builtin_traits() {
        let mut env = TraitEnv::new();
        let builtins = BuiltinTraits::new(&mut env);

        assert!(env.get_trait(builtins.copy).is_some());
        assert!(env.get_trait(builtins.clone).is_some());
        assert!(env.get_trait(builtins.iterator).is_some());

        let iterator = env.get_trait(builtins.iterator).unwrap();
        assert_eq!(iterator.assoc_types.len(), 1);
        assert_eq!(iterator.assoc_types[0].name.as_ref(), "Item");
    }
}
