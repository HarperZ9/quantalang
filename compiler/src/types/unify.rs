// ===============================================================================
// BUILDLANG TYPE SYSTEM - UNIFICATION
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! Type unification algorithm.
//!
//! Unification finds a substitution that makes two types equal.
//! This is the core algorithm for Hindley-Milner type inference.

use super::error::{TypeError, TypeResult};
use super::ty::*;

/// Unifier for type inference.
#[derive(Debug)]
pub struct Unifier {
    /// The current substitution.
    subst: Substitution,
}

impl Unifier {
    /// Create a new unifier.
    pub fn new() -> Self {
        Self {
            subst: Substitution::new(),
        }
    }

    /// Create a unifier with an initial substitution.
    pub fn with_subst(subst: Substitution) -> Self {
        Self { subst }
    }

    /// Get the current substitution.
    pub fn substitution(&self) -> &Substitution {
        &self.subst
    }

    /// Take the substitution.
    pub fn into_substitution(self) -> Substitution {
        self.subst
    }

    /// Apply the current substitution to a type.
    pub fn apply(&self, ty: &Ty) -> Ty {
        ty.substitute(&self.subst)
    }

    /// Unify two types, updating the substitution.
    ///
    /// Reference and pointer pointees unify invariantly: a concrete integer
    /// or float width difference behind `&`, `&mut`, or `*` is a type error,
    /// not a silent coercion. In value position, integer width coercion stays
    /// permissive so array indexing with an i32 variable still checks.
    pub fn unify(&mut self, t1: &Ty, t2: &Ty) -> TypeResult<()> {
        self.unify_mode(t1, t2, false)
    }

    /// Unify with an explicit variance mode. `invariant` is true when the two
    /// types sit behind a reference or pointer, where a width coercion would
    /// let a store or load reach the wrong number of bytes.
    fn unify_mode(&mut self, t1: &Ty, t2: &Ty, invariant: bool) -> TypeResult<()> {
        let t1 = self.apply(t1);
        let t2 = self.apply(t2);

        self.unify_impl(&t1, &t2, invariant)
    }

    /// Internal unification implementation.
    fn unify_impl(&mut self, t1: &Ty, t2: &Ty, invariant: bool) -> TypeResult<()> {
        // If types are equal (including annotations), we're done
        if t1 == t2 {
            return Ok(());
        }

        // Check color space / annotation compatibility.
        // If BOTH types have annotations, they must match.
        // If only one has annotations, the unannotated type is compatible
        // (allows mixing annotated APIs with unannotated code).
        if !t1.annotations.is_empty() && !t2.annotations.is_empty() {
            // Both have annotations - check for conflicts
            // Extract the category (e.g., "ColorSpace") and value (e.g., "Linear")
            for ann1 in &t1.annotations {
                for ann2 in &t2.annotations {
                    if let (Some(cat1), Some(cat2)) =
                        (ann1.split(':').next(), ann2.split(':').next())
                    {
                        if cat1 == cat2 && ann1 != ann2 {
                            // Same category, different value - color space mismatch!
                            return Err(TypeError::TypeMismatch {
                                expected: t1.clone(),
                                found: t2.clone(),
                            });
                        }
                    }
                }
            }
        }

        match (&t1.kind, &t2.kind) {
            // Type variable on the left
            (TyKind::Var(v1), _) => {
                self.bind(*v1, t2.clone())?;
                Ok(())
            }

            // Type variable on the right
            (_, TyKind::Var(v2)) => {
                self.bind(*v2, t1.clone())?;
                Ok(())
            }

            // Inference variables
            (TyKind::Infer(infer1), _) => {
                self.bind(infer1.var, t2.clone())?;
                Ok(())
            }
            (_, TyKind::Infer(infer2)) => {
                self.bind(infer2.var, t1.clone())?;
                Ok(())
            }

            // Error type unifies with anything (for error recovery)
            (TyKind::Error, _) | (_, TyKind::Error) => Ok(()),

            // Never type can unify with any type (subtype of everything)
            (TyKind::Never, _) | (_, TyKind::Never) => Ok(()),

            // Primitive types must be equal
            (TyKind::Int(i1), TyKind::Int(i2)) if i1 == i2 => Ok(()),
            // Value-position integer width coercion (e.g. i32 <-> usize for
            // array indexing) stays permissive so common programs check.
            // Behind a reference or pointer the widths must match exactly: a
            // differing width falls through to the mismatch error below, since
            // an 8-byte store through a 4-byte place corrupts adjacent memory.
            (TyKind::Int(_), TyKind::Int(_)) if !invariant => Ok(()),
            // Float-to-float coercion (f32 <-> f64) stays allowed for
            // ecosystem compatibility, exactly as before. A unit dimension
            // (`f64<m/s>`, experimental) is the one thing that still hard
            // fails here: two DIFFERENT `Some` dimensions never unify.
            // Equal dimensions, or at least one `None` (unconstrained,
            // compatible with any unit), unify same as an unannotated float.
            (TyKind::Float(f1), TyKind::Float(f2)) => {
                // Behind a reference or pointer, f32 and f64 are different
                // widths and must not coerce; fall through to the error.
                if invariant && f1 != f2 {
                    return Err(TypeError::TypeMismatch {
                        expected: t1.clone(),
                        found: t2.clone(),
                    });
                }
                match (&t1.unit_dim, &t2.unit_dim) {
                    (Some(a), Some(b)) if a != b => Err(TypeError::UnitMismatch {
                        expected: a.to_canonical_string(),
                        found: b.to_canonical_string(),
                    }),
                    _ => Ok(()),
                }
            }
            (TyKind::Bool, TyKind::Bool) => Ok(()),
            (TyKind::Char, TyKind::Char) => Ok(()),
            (TyKind::Str, TyKind::Str) => Ok(()),

            // String coercion: `str` and `&str` / `&'static str` are
            // interchangeable in BuildLang (both map to BuildString).
            (TyKind::Str, TyKind::Ref(_, _, inner)) | (TyKind::Ref(_, _, inner), TyKind::Str)
                if inner.kind == TyKind::Str =>
            {
                Ok(())
            }

            // Reference coercion: `&T` unifies with `T` (auto-deref).
            // Only for concrete types (ADT, primitives), not for Never/Error.
            // Excludes Ref-vs-Ref: when both sides are references, fall through
            // to the Ref/Ref arm which checks lifetime compatibility.
            (TyKind::Ref(_, _, inner), other)
                if !invariant
                    && !matches!(
                        other,
                        TyKind::Never | TyKind::Error | TyKind::Var(_) | TyKind::Ref(_, _, _)
                    ) =>
            {
                self.unify_impl(inner, t2, invariant)
            }
            (other, TyKind::Ref(_, _, inner))
                if !invariant
                    && !matches!(
                        other,
                        TyKind::Never | TyKind::Error | TyKind::Var(_) | TyKind::Ref(_, _, _)
                    ) =>
            {
                self.unify_impl(t1, inner, invariant)
            }

            // Tuples: must have same length and unify element-wise
            (TyKind::Tuple(elems1), TyKind::Tuple(elems2)) => {
                if elems1.len() != elems2.len() {
                    return Err(TypeError::TypeMismatch {
                        expected: t1.clone(),
                        found: t2.clone(),
                    });
                }
                for (e1, e2) in elems1.iter().zip(elems2.iter()) {
                    self.unify_mode(e1, e2, invariant)?;
                }
                Ok(())
            }

            // Arrays: same element type and length
            (TyKind::Array(elem1, len1), TyKind::Array(elem2, len2)) => {
                if len1 != len2 {
                    return Err(TypeError::ArrayLengthMismatch {
                        expected: *len1,
                        found: *len2,
                    });
                }
                self.unify_mode(elem1, elem2, invariant)
            }

            // Slices: same element type
            (TyKind::Slice(elem1), TyKind::Slice(elem2)) => {
                self.unify_mode(elem1, elem2, invariant)
            }

            // Array-to-slice coercion: [T; N] unifies with [T]
            // This allows passing fixed-size arrays where slices are expected.
            (TyKind::Array(elem1, _), TyKind::Slice(elem2)) => {
                self.unify_mode(elem1, elem2, invariant)
            }
            (TyKind::Slice(elem1), TyKind::Array(elem2, _)) => {
                self.unify_mode(elem1, elem2, invariant)
            }

            // References: same mutability and unified pointee
            (TyKind::Ref(lt1, mut1, ty1), TyKind::Ref(lt2, mut2, ty2)) => {
                if mut1 != mut2 {
                    return Err(TypeError::MutabilityMismatch {
                        expected: *mut1,
                        found: *mut2,
                    });
                }
                // Lifetime unification: lifetimes unify if they're identical,
                // or if either is elided (None), allowing inference to proceed
                match (lt1, lt2) {
                    (Some(l1), Some(l2)) if l1 != l2 => {
                        return Err(TypeError::LifetimeMismatch {
                            expected: l1.clone(),
                            found: l2.clone(),
                        });
                    }
                    // Elided lifetimes or matching lifetimes are acceptable
                    _ => {}
                }
                // A mutable or shared reference pins its pointee width: reading
                // or writing through it moves a fixed number of bytes, so the
                // pointee unifies invariantly.
                self.unify_mode(ty1, ty2, true)
            }

            // Pointers: same mutability and unified pointee
            (TyKind::Ptr(mut1, ty1), TyKind::Ptr(mut2, ty2)) => {
                if mut1 != mut2 {
                    return Err(TypeError::MutabilityMismatch {
                        expected: *mut1,
                        found: *mut2,
                    });
                }
                // Same reasoning as references: the pointee width is fixed.
                self.unify_mode(ty1, ty2, true)
            }

            // Functions: unify parameters and return type
            (TyKind::Fn(fn1), TyKind::Fn(fn2)) => {
                if fn1.params.len() != fn2.params.len() {
                    return Err(TypeError::ArityMismatch {
                        expected: fn1.params.len(),
                        found: fn2.params.len(),
                    });
                }
                if fn1.is_unsafe != fn2.is_unsafe {
                    return Err(TypeError::UnsafetyMismatch);
                }
                // ABI matching: ABIs must be compatible for function pointers
                // None (default Build ABI) is compatible with explicit "build"
                // Different explicit ABIs are incompatible
                match (&fn1.abi, &fn2.abi) {
                    (None, None) => {}
                    (None, Some(a)) | (Some(a), None) if &**a == "build" => {}
                    (Some(a1), Some(a2)) if a1 == a2 => {}
                    (Some(a1), Some(a2)) => {
                        return Err(TypeError::AbiMismatch {
                            expected: a1.clone(),
                            found: a2.clone(),
                        });
                    }
                    _ => {}
                }
                for (p1, p2) in fn1.params.iter().zip(fn2.params.iter()) {
                    self.unify_mode(p1, p2, invariant)?;
                }
                self.unify_mode(&fn1.ret, &fn2.ret, invariant)?;
                // Effect rows are part of the callable contract. Allowing an
                // effectful function to unify with a pure function type erases
                // the capability gate at callback and assignment boundaries.
                if fn1.effects != fn2.effects {
                    return Err(TypeError::TypeMismatch {
                        expected: t1.clone(),
                        found: t2.clone(),
                    });
                }
                Ok(())
            }

            // ADTs: same definition and unified type arguments
            (TyKind::Adt(def1, args1), TyKind::Adt(def2, args2)) => {
                if def1 != def2 {
                    return Err(TypeError::TypeMismatch {
                        expected: t1.clone(),
                        found: t2.clone(),
                    });
                }
                if args1.len() != args2.len() {
                    return Err(TypeError::TypeMismatch {
                        expected: t1.clone(),
                        found: t2.clone(),
                    });
                }
                for (a1, a2) in args1.iter().zip(args2.iter()) {
                    self.unify_mode(a1, a2, invariant)?;
                }
                Ok(())
            }

            // Type parameters: must be identical
            (TyKind::Param(n1, i1), TyKind::Param(n2, i2)) => {
                if n1 == n2 && i1 == i2 {
                    Ok(())
                } else {
                    Err(TypeError::TypeMismatch {
                        expected: t1.clone(),
                        found: t2.clone(),
                    })
                }
            }

            // Associated type projections: same trait item and unified inputs.
            (
                TyKind::Projection {
                    trait_ref: trait_ref1,
                    item: item1,
                    self_ty: self_ty1,
                    substs: substs1,
                },
                TyKind::Projection {
                    trait_ref: trait_ref2,
                    item: item2,
                    self_ty: self_ty2,
                    substs: substs2,
                },
            ) => {
                if trait_ref1 != trait_ref2 || item1 != item2 || substs1.len() != substs2.len() {
                    return Err(TypeError::TypeMismatch {
                        expected: t1.clone(),
                        found: t2.clone(),
                    });
                }
                self.unify_mode(self_ty1, self_ty2, invariant)?;
                for (subst1, subst2) in substs1.iter().zip(substs2.iter()) {
                    self.unify_mode(subst1, subst2, invariant)?;
                }
                Ok(())
            }

            // All other combinations are mismatches
            _ => Err(TypeError::TypeMismatch {
                expected: t1.clone(),
                found: t2.clone(),
            }),
        }
    }

    /// Bind a type variable to a type.
    fn bind(&mut self, var: TyVarId, ty: Ty) -> TypeResult<()> {
        // Check if already bound
        if let Some(existing) = self.subst.get(var) {
            return self.unify(&existing.clone(), &ty);
        }

        // Apply current substitution to resolve already-bound variables
        // before the occurs check. Without this, the occurs check can
        // false-positive when ?T appears inside &?U and ?U is already
        // bound to the same struct type (common with auto-deref on
        // reference parameters in functions returning struct literals).
        let resolved = self.apply(&ty);

        // Occurs check: prevent infinite types
        if self.occurs_in(var, &resolved) {
            return Err(TypeError::InfiniteType { var, ty: resolved });
        }

        // Add the binding
        self.subst.insert(var, resolved);
        Ok(())
    }

    /// Check if a type variable occurs in a type (for occurs check).
    fn occurs_in(&self, var: TyVarId, ty: &Ty) -> bool {
        match &ty.kind {
            TyKind::Var(v) if *v == var => true,
            TyKind::Var(v) => {
                if let Some(bound) = self.subst.get(*v) {
                    self.occurs_in(var, bound)
                } else {
                    false
                }
            }
            TyKind::Infer(infer) if infer.var == var => true,
            TyKind::Infer(infer) => {
                if let Some(bound) = self.subst.get(infer.var) {
                    self.occurs_in(var, bound)
                } else {
                    false
                }
            }
            TyKind::Tuple(elems) => elems.iter().any(|t| self.occurs_in(var, t)),
            TyKind::Array(elem, _) | TyKind::Slice(elem) => self.occurs_in(var, elem),
            TyKind::Ref(_, _, ty) | TyKind::Ptr(_, ty) => self.occurs_in(var, ty),
            TyKind::Fn(fn_ty) => {
                fn_ty.params.iter().any(|t| self.occurs_in(var, t))
                    || self.occurs_in(var, &fn_ty.ret)
            }
            TyKind::Adt(_, args) => args.iter().any(|t| self.occurs_in(var, t)),
            TyKind::Projection {
                self_ty, substs, ..
            } => self.occurs_in(var, self_ty) || substs.iter().any(|t| self.occurs_in(var, t)),
            _ => false,
        }
    }
}

impl Default for Unifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Unify two types and return the resulting substitution.
pub fn unify(t1: &Ty, t2: &Ty) -> TypeResult<Substitution> {
    let mut unifier = Unifier::new();
    unifier.unify(t1, t2)?;
    Ok(unifier.into_substitution())
}

/// Unify two types with an existing substitution.
pub fn unify_with(subst: Substitution, t1: &Ty, t2: &Ty) -> TypeResult<Substitution> {
    let mut unifier = Unifier::with_subst(subst);
    unifier.unify(t1, t2)?;
    Ok(unifier.into_substitution())
}

/// Check if two types can be unified.
pub fn can_unify(t1: &Ty, t2: &Ty) -> bool {
    unify(t1, t2).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unify_same_types() {
        let t = Ty::int(IntTy::I32);
        let subst = unify(&t, &t).unwrap();
        assert!(subst.is_empty());
    }

    #[test]
    fn test_unify_var_with_concrete() {
        let v = TyVarId::fresh();
        let var = Ty::var(v);
        let concrete = Ty::int(IntTy::I32);

        let subst = unify(&var, &concrete).unwrap();
        assert_eq!(subst.get(v), Some(&concrete));
    }

    #[test]
    fn test_unify_tuples() {
        let v = TyVarId::fresh();
        let t1 = Ty::tuple(vec![Ty::var(v), Ty::bool()]);
        let t2 = Ty::tuple(vec![Ty::int(IntTy::I32), Ty::bool()]);

        let subst = unify(&t1, &t2).unwrap();
        assert_eq!(subst.get(v), Some(&Ty::int(IntTy::I32)));
    }

    #[test]
    fn test_unify_different_lengths() {
        let t1 = Ty::tuple(vec![Ty::int(IntTy::I32)]);
        let t2 = Ty::tuple(vec![Ty::int(IntTy::I32), Ty::bool()]);

        assert!(unify(&t1, &t2).is_err());
    }

    #[test]
    fn test_occurs_check() {
        let v = TyVarId::fresh();
        let var = Ty::var(v);
        // Try to unify ?T with (?T, bool) - should fail
        let t = Ty::tuple(vec![var.clone(), Ty::bool()]);

        assert!(unify(&var, &t).is_err());
    }

    #[test]
    fn test_unify_functions() {
        let v1 = TyVarId::fresh();
        let v2 = TyVarId::fresh();

        let t1 = Ty::function(vec![Ty::var(v1)], Ty::var(v2));
        let t2 = Ty::function(vec![Ty::int(IntTy::I32)], Ty::bool());

        let subst = unify(&t1, &t2).unwrap();
        assert_eq!(subst.get(v1), Some(&Ty::int(IntTy::I32)));
        assert_eq!(subst.get(v2), Some(&Ty::bool()));
    }

    #[test]
    fn test_unify_rejects_erased_function_effects() {
        let pure = Ty::function(vec![Ty::str()], Ty::str());
        let file_effect =
            super::super::effects::EffectRow::closed([super::super::effects::Effect::new(
                "FileSystem",
            )]);
        let effectful = Ty::function_with_effects(vec![Ty::str()], Ty::str(), file_effect);

        let result = unify(&pure, &effectful);

        assert!(
            result.is_err(),
            "effectful functions must not unify with pure function types"
        );
    }

    #[test]
    fn test_unify_never() {
        // Never type can unify with anything
        let never = Ty::never();
        let concrete = Ty::int(IntTy::I32);

        assert!(unify(&never, &concrete).is_ok());
    }

    #[test]
    fn test_transitive_unification() {
        let v1 = TyVarId::fresh();
        let v2 = TyVarId::fresh();

        let mut unifier = Unifier::new();
        unifier.unify(&Ty::var(v1), &Ty::var(v2)).unwrap();
        unifier.unify(&Ty::var(v2), &Ty::int(IntTy::I32)).unwrap();

        let result = unifier.apply(&Ty::var(v1));
        assert_eq!(result, Ty::int(IntTy::I32));
    }

    #[test]
    fn test_ref_lifetime_mismatch_is_error() {
        let t1 = Ty::reference(
            Some(Lifetime::new("a")),
            Mutability::Immutable,
            Ty::int(IntTy::I32),
        );
        let t2 = Ty::reference(
            Some(Lifetime::new("b")),
            Mutability::Immutable,
            Ty::int(IntTy::I32),
        );
        let result = unify(&t1, &t2);
        assert!(
            result.is_err(),
            "expected LifetimeMismatch error for 'a vs 'b"
        );
    }

    #[test]
    fn test_ref_same_lifetime_ok() {
        let t1 = Ty::reference(
            Some(Lifetime::new("a")),
            Mutability::Immutable,
            Ty::int(IntTy::I32),
        );
        let t2 = Ty::reference(
            Some(Lifetime::new("a")),
            Mutability::Immutable,
            Ty::int(IntTy::I32),
        );
        assert!(unify(&t1, &t2).is_ok());
    }

    #[test]
    fn test_ref_elided_lifetime_ok() {
        let t1 = Ty::reference(None, Mutability::Immutable, Ty::int(IntTy::I32));
        let t2 = Ty::reference(
            Some(Lifetime::new("a")),
            Mutability::Immutable,
            Ty::int(IntTy::I32),
        );
        assert!(
            unify(&t1, &t2).is_ok(),
            "elided lifetime should unify with named"
        );
    }
}
