// ===============================================================================
// QUANTALANG TYPE SYSTEM - BUILT-IN TYPES AND TRAITS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Built-in types and traits for QuantaLang.
//!
//! This module defines the core built-in types (primitives, collections) and
//! fundamental traits (Copy, Clone, Debug, etc.) that are implicitly available.

use std::sync::Arc;

use super::ty::*;
use super::context::*;

/// IDs for built-in traits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinTraitId {
    /// The `Copy` trait - types that can be copied bitwise.
    Copy,
    /// The `Clone` trait - types that can be explicitly cloned.
    Clone,
    /// The `Sized` trait - types with a known size at compile time.
    Sized,
    /// The `Send` trait - types safe to send between threads.
    Send,
    /// The `Sync` trait - types safe to share between threads.
    Sync,
    /// The `Drop` trait - types with custom destructors.
    Drop,
    /// The `Default` trait - types with default values.
    Default,
    /// The `Debug` trait - types that can be formatted for debugging.
    Debug,
    /// The `Display` trait - types that can be displayed to users.
    Display,
    /// The `PartialEq` trait - types that can be compared for equality.
    PartialEq,
    /// The `Eq` trait - types with reflexive equality.
    Eq,
    /// The `PartialOrd` trait - types with partial ordering.
    PartialOrd,
    /// The `Ord` trait - types with total ordering.
    Ord,
    /// The `Hash` trait - types that can be hashed.
    Hash,
    /// The `Iterator` trait - types that produce sequences.
    Iterator,
    /// The `IntoIterator` trait - types that can be converted to iterators.
    IntoIterator,
    /// The `FromIterator` trait - types that can be constructed from iterators.
    FromIterator,
    /// The `Add` trait - types that support addition.
    Add,
    /// The `Sub` trait - types that support subtraction.
    Sub,
    /// The `Mul` trait - types that support multiplication.
    Mul,
    /// The `Div` trait - types that support division.
    Div,
    /// The `Rem` trait - types that support remainder.
    Rem,
    /// The `Neg` trait - types that support negation.
    Neg,
    /// The `Not` trait - types that support logical/bitwise not.
    Not,
    /// The `Index` trait - types that support indexing.
    Index,
    /// The `IndexMut` trait - types that support mutable indexing.
    IndexMut,
    /// The `Deref` trait - types that can be dereferenced.
    Deref,
    /// The `DerefMut` trait - types that can be mutably dereferenced.
    DerefMut,
    /// The `Fn` trait - callable types.
    Fn,
    /// The `FnMut` trait - callable types that may mutate state.
    FnMut,
    /// The `FnOnce` trait - callable types that consume themselves.
    FnOnce,
    /// The `Future` trait - asynchronous computations.
    Future,
    /// The `Try` trait - types that support the `?` operator.
    Try,
    /// The `From` trait - types that can be constructed from another type.
    From,
    /// The `Into` trait - types that can be converted into another type.
    Into,
    /// The `AsRef` trait - cheap reference-to-reference conversions.
    AsRef,
    /// The `AsMut` trait - cheap mutable reference conversions.
    AsMut,
}

/// Manager for built-in types and traits.
pub struct Builtins {
    /// Definition IDs for built-in traits.
    trait_ids: std::collections::HashMap<BuiltinTraitId, DefId>,
}

impl Builtins {
    /// Create a new builtins manager and register everything in the context.
    pub fn new(ctx: &mut TypeContext) -> Self {
        let mut builtins = Self {
            trait_ids: std::collections::HashMap::new(),
        };

        builtins.register_traits(ctx);
        builtins.register_primitive_impls(ctx);

        builtins
    }

    /// Get the DefId for a built-in trait.
    pub fn trait_id(&self, id: BuiltinTraitId) -> Option<DefId> {
        self.trait_ids.get(&id).copied()
    }

    /// Register all built-in traits.
    fn register_traits(&mut self, ctx: &mut TypeContext) {
        // Copy trait
        self.register_trait(ctx, BuiltinTraitId::Copy, "Copy", vec![]);

        // Clone trait (Clone: Clone requirement is implicit)
        self.register_trait(ctx, BuiltinTraitId::Clone, "Clone", vec![]);

        // Sized trait (marker trait)
        self.register_trait(ctx, BuiltinTraitId::Sized, "Sized", vec![]);

        // Send and Sync (marker traits)
        self.register_trait(ctx, BuiltinTraitId::Send, "Send", vec![]);
        self.register_trait(ctx, BuiltinTraitId::Sync, "Sync", vec![]);

        // Drop trait
        self.register_trait(ctx, BuiltinTraitId::Drop, "Drop", vec![]);

        // Default trait
        self.register_trait(ctx, BuiltinTraitId::Default, "Default", vec![]);

        // Debug and Display
        self.register_trait(ctx, BuiltinTraitId::Debug, "Debug", vec![]);
        self.register_trait(ctx, BuiltinTraitId::Display, "Display", vec![]);

        // Comparison traits
        self.register_trait(ctx, BuiltinTraitId::PartialEq, "PartialEq", vec![]);
        self.register_trait(ctx, BuiltinTraitId::Eq, "Eq", vec![
            BuiltinTraitId::PartialEq,
        ]);
        self.register_trait(ctx, BuiltinTraitId::PartialOrd, "PartialOrd", vec![
            BuiltinTraitId::PartialEq,
        ]);
        self.register_trait(ctx, BuiltinTraitId::Ord, "Ord", vec![
            BuiltinTraitId::Eq,
            BuiltinTraitId::PartialOrd,
        ]);

        // Hash trait
        self.register_trait(ctx, BuiltinTraitId::Hash, "Hash", vec![]);

        // Iterator traits
        self.register_trait(ctx, BuiltinTraitId::Iterator, "Iterator", vec![]);
        self.register_trait(ctx, BuiltinTraitId::IntoIterator, "IntoIterator", vec![]);
        self.register_trait(ctx, BuiltinTraitId::FromIterator, "FromIterator", vec![]);

        // Arithmetic traits
        self.register_trait(ctx, BuiltinTraitId::Add, "Add", vec![]);
        self.register_trait(ctx, BuiltinTraitId::Sub, "Sub", vec![]);
        self.register_trait(ctx, BuiltinTraitId::Mul, "Mul", vec![]);
        self.register_trait(ctx, BuiltinTraitId::Div, "Div", vec![]);
        self.register_trait(ctx, BuiltinTraitId::Rem, "Rem", vec![]);
        self.register_trait(ctx, BuiltinTraitId::Neg, "Neg", vec![]);
        self.register_trait(ctx, BuiltinTraitId::Not, "Not", vec![]);

        // Indexing traits
        self.register_trait(ctx, BuiltinTraitId::Index, "Index", vec![]);
        self.register_trait(ctx, BuiltinTraitId::IndexMut, "IndexMut", vec![
            BuiltinTraitId::Index,
        ]);

        // Deref traits
        self.register_trait(ctx, BuiltinTraitId::Deref, "Deref", vec![]);
        self.register_trait(ctx, BuiltinTraitId::DerefMut, "DerefMut", vec![
            BuiltinTraitId::Deref,
        ]);

        // Callable traits
        self.register_trait(ctx, BuiltinTraitId::FnOnce, "FnOnce", vec![]);
        self.register_trait(ctx, BuiltinTraitId::FnMut, "FnMut", vec![
            BuiltinTraitId::FnOnce,
        ]);
        self.register_trait(ctx, BuiltinTraitId::Fn, "Fn", vec![
            BuiltinTraitId::FnMut,
        ]);

        // Async traits
        self.register_trait(ctx, BuiltinTraitId::Future, "Future", vec![]);

        // Try trait
        self.register_trait(ctx, BuiltinTraitId::Try, "Try", vec![]);

        // Conversion traits
        self.register_trait(ctx, BuiltinTraitId::From, "From", vec![]);
        self.register_trait(ctx, BuiltinTraitId::Into, "Into", vec![]);
        self.register_trait(ctx, BuiltinTraitId::AsRef, "AsRef", vec![]);
        self.register_trait(ctx, BuiltinTraitId::AsMut, "AsMut", vec![]);
    }

    fn register_trait(
        &mut self,
        ctx: &mut TypeContext,
        id: BuiltinTraitId,
        name: &str,
        supertraits: Vec<BuiltinTraitId>,
    ) {
        let def_id = ctx.fresh_def_id();
        self.trait_ids.insert(id, def_id);

        let supertrait_bounds: Vec<_> = supertraits.iter()
            .filter_map(|st| {
                self.trait_ids.get(st).map(|&trait_id| TraitBound {
                    trait_id,
                    args: Vec::new(),
                })
            })
            .collect();

        let trait_def = TraitDef {
            def_id,
            name: Arc::from(name),
            generics: Vec::new(),
            supertraits: supertrait_bounds,
            assoc_types: Vec::new(),
            methods: Vec::new(),
        };

        ctx.register_trait(trait_def);
    }

    /// Register built-in trait implementations for primitive types.
    fn register_primitive_impls(&mut self, ctx: &mut TypeContext) {
        // All primitive types implement Copy, Clone, Sized, Send, Sync
        let copy_clone_traits = vec![
            BuiltinTraitId::Copy,
            BuiltinTraitId::Clone,
            BuiltinTraitId::Sized,
            BuiltinTraitId::Send,
            BuiltinTraitId::Sync,
            BuiltinTraitId::Default,
            BuiltinTraitId::Debug,
            BuiltinTraitId::PartialEq,
            BuiltinTraitId::Eq,
            BuiltinTraitId::PartialOrd,
            BuiltinTraitId::Ord,
            BuiltinTraitId::Hash,
        ];

        // Integer types
        let int_types = vec![
            Ty::int(IntTy::I8),
            Ty::int(IntTy::I16),
            Ty::int(IntTy::I32),
            Ty::int(IntTy::I64),
            Ty::int(IntTy::I128),
            Ty::int(IntTy::Isize),
            Ty::int(IntTy::U8),
            Ty::int(IntTy::U16),
            Ty::int(IntTy::U32),
            Ty::int(IntTy::U64),
            Ty::int(IntTy::U128),
            Ty::int(IntTy::Usize),
        ];

        for ty in &int_types {
            for trait_id in &copy_clone_traits {
                self.register_impl(ctx, *trait_id, ty.clone());
            }

            // Integer-specific traits
            self.register_impl(ctx, BuiltinTraitId::Add, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Sub, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Mul, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Div, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Rem, ty.clone());
        }

        // Signed integers also implement Neg
        let signed_int_types = vec![
            Ty::int(IntTy::I8),
            Ty::int(IntTy::I16),
            Ty::int(IntTy::I32),
            Ty::int(IntTy::I64),
            Ty::int(IntTy::I128),
            Ty::int(IntTy::Isize),
        ];

        for ty in &signed_int_types {
            self.register_impl(ctx, BuiltinTraitId::Neg, ty.clone());
        }

        // Float types
        let float_types = vec![
            Ty::float(FloatTy::F32),
            Ty::float(FloatTy::F64),
        ];

        for ty in &float_types {
            self.register_impl(ctx, BuiltinTraitId::Copy, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Clone, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Sized, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Send, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Sync, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Default, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Debug, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::PartialEq, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::PartialOrd, ty.clone());

            // Arithmetic
            self.register_impl(ctx, BuiltinTraitId::Add, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Sub, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Mul, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Div, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Rem, ty.clone());
            self.register_impl(ctx, BuiltinTraitId::Neg, ty.clone());
        }

        // Bool type
        let bool_ty = Ty::bool();
        for trait_id in &copy_clone_traits {
            self.register_impl(ctx, *trait_id, bool_ty.clone());
        }
        self.register_impl(ctx, BuiltinTraitId::Not, bool_ty.clone());
        self.register_impl(ctx, BuiltinTraitId::Display, bool_ty.clone());

        // Char type
        let char_ty = Ty::char();
        for trait_id in &copy_clone_traits {
            self.register_impl(ctx, *trait_id, char_ty.clone());
        }
        self.register_impl(ctx, BuiltinTraitId::Display, char_ty.clone());

        // Unit type
        let unit_ty = Ty::unit();
        self.register_impl(ctx, BuiltinTraitId::Copy, unit_ty.clone());
        self.register_impl(ctx, BuiltinTraitId::Clone, unit_ty.clone());
        self.register_impl(ctx, BuiltinTraitId::Sized, unit_ty.clone());
        self.register_impl(ctx, BuiltinTraitId::Send, unit_ty.clone());
        self.register_impl(ctx, BuiltinTraitId::Sync, unit_ty.clone());
        self.register_impl(ctx, BuiltinTraitId::Default, unit_ty.clone());
        self.register_impl(ctx, BuiltinTraitId::Debug, unit_ty.clone());
        self.register_impl(ctx, BuiltinTraitId::PartialEq, unit_ty.clone());
        self.register_impl(ctx, BuiltinTraitId::Eq, unit_ty.clone());
        self.register_impl(ctx, BuiltinTraitId::PartialOrd, unit_ty.clone());
        self.register_impl(ctx, BuiltinTraitId::Ord, unit_ty.clone());
        self.register_impl(ctx, BuiltinTraitId::Hash, unit_ty.clone());

        // Never type
        let never_ty = Ty::never();
        self.register_impl(ctx, BuiltinTraitId::Sized, never_ty.clone());
        // Never type implements all traits (it can never be constructed)
    }

    fn register_impl(&mut self, ctx: &mut TypeContext, trait_id: BuiltinTraitId, self_ty: Ty) {
        if let Some(&def_id) = self.trait_ids.get(&trait_id) {
            let impl_ = TraitImpl {
                trait_id: def_id,
                self_ty,
                generics: Vec::new(),
                assoc_types: std::collections::HashMap::new(),
                methods: std::collections::HashMap::new(),
                where_clauses: Vec::new(),
            };
            ctx.register_impl(impl_);
        }
    }
}

/// Check if a type is `Copy`.
pub fn is_copy(ctx: &TypeContext, builtins: &Builtins, ty: &Ty) -> bool {
    if let Some(copy_id) = builtins.trait_id(BuiltinTraitId::Copy) {
        !ctx.find_impls(copy_id, ty).is_empty()
    } else {
        // Fallback: primitives are always Copy
        matches!(ty.kind,
            TyKind::Int(_) | TyKind::Float(_) | TyKind::Bool | TyKind::Char |
            TyKind::Ptr(_, _) | TyKind::Never
        )
    }
}

/// Check if a type is `Sized`.
pub fn is_sized(ty: &Ty) -> bool {
    !matches!(ty.kind, TyKind::Slice(_) | TyKind::Str)
}

/// Check if a type is `Send`.
pub fn is_send(ctx: &TypeContext, builtins: &Builtins, ty: &Ty) -> bool {
    if let Some(send_id) = builtins.trait_id(BuiltinTraitId::Send) {
        !ctx.find_impls(send_id, ty).is_empty()
    } else {
        // Most types are Send by default
        true
    }
}

/// Check if a type is `Sync`.
pub fn is_sync(ctx: &TypeContext, builtins: &Builtins, ty: &Ty) -> bool {
    if let Some(sync_id) = builtins.trait_id(BuiltinTraitId::Sync) {
        !ctx.find_impls(sync_id, ty).is_empty()
    } else {
        // Most types are Sync by default
        true
    }
}

/// Get the default value for a type (if it implements Default).
pub fn default_value_repr(ty: &Ty) -> Option<String> {
    match &ty.kind {
        TyKind::Int(int_ty) => Some(match int_ty {
            IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::I128 | IntTy::Isize |
            IntTy::U8 | IntTy::U16 | IntTy::U32 | IntTy::U64 | IntTy::U128 | IntTy::Usize => "0".to_string(),
        }),
        TyKind::Float(_) => Some("0.0".to_string()),
        TyKind::Bool => Some("false".to_string()),
        TyKind::Char => Some("'\\0'".to_string()),
        TyKind::Tuple(elems) if elems.is_empty() => Some("()".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtins_registration() {
        let mut ctx = TypeContext::new();
        let builtins = Builtins::new(&mut ctx);

        // Check that Copy trait is registered
        assert!(builtins.trait_id(BuiltinTraitId::Copy).is_some());

        // Check that i32 implements Copy
        let i32_ty = Ty::int(IntTy::I32);
        assert!(is_copy(&ctx, &builtins, &i32_ty));
    }

    #[test]
    fn test_is_sized() {
        assert!(is_sized(&Ty::int(IntTy::I32)));
        assert!(is_sized(&Ty::bool()));
        assert!(!is_sized(&Ty::str()));
        assert!(!is_sized(&Ty::slice(Ty::int(IntTy::U8))));
    }

    #[test]
    fn test_default_values() {
        assert_eq!(default_value_repr(&Ty::int(IntTy::I32)), Some("0".to_string()));
        assert_eq!(default_value_repr(&Ty::bool()), Some("false".to_string()));
        assert_eq!(default_value_repr(&Ty::float(FloatTy::F64)), Some("0.0".to_string()));
    }
}
