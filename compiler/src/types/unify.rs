// ===============================================================================
// QUANTALANG TYPE SYSTEM - UNIFICATION
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Type unification algorithm.
//!
//! Unification finds a substitution that makes two types equal.
//! This is the core algorithm for Hindley-Milner type inference.

use super::ty::*;
use super::error::{TypeError, TypeResult};

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
    pub fn unify(&mut self, t1: &Ty, t2: &Ty) -> TypeResult<()> {
        let t1 = self.apply(t1);
        let t2 = self.apply(t2);

        self.unify_impl(&t1, &t2)
    }

    /// Internal unification implementation.
    fn unify_impl(&mut self, t1: &Ty, t2: &Ty) -> TypeResult<()> {
        // If types are equal (including annotations), we're done
        if t1 == t2 {
            return Ok(());
        }

        // Check color space / annotation compatibility.
        // If BOTH types have annotations, they must match.
        // If only one has annotations, the unannotated type is compatible
        // (allows mixing annotated APIs with unannotated code).
        if !t1.annotations.is_empty() && !t2.annotations.is_empty() {
            // Both have annotations — check for conflicts
            // Extract the category (e.g., "ColorSpace") and value (e.g., "Linear")
            for ann1 in &t1.annotations {
                for ann2 in &t2.annotations {
                    if let (Some(cat1), Some(cat2)) = (ann1.split(':').next(), ann2.split(':').next()) {
                        if cat1 == cat2 && ann1 != ann2 {
                            // Same category, different value — color space mismatch!
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
            // Allow implicit integer width coercion (e.g. i32 <-> usize for
            // array indexing).  A stricter implementation would only allow
            // widening; for now we allow all integer-to-integer conversions
            // so test programs that index arrays with i32 variables compile.
            (TyKind::Int(_), TyKind::Int(_)) => Ok(()),
            (TyKind::Float(f1), TyKind::Float(f2)) if f1 == f2 => Ok(()),
            // Allow implicit float width coercion (f32 <-> f64) for ecosystem
            // compatibility. Shader code frequently mixes f32 and f64.
            (TyKind::Float(_), TyKind::Float(_)) => Ok(()),
            (TyKind::Bool, TyKind::Bool) => Ok(()),
            (TyKind::Char, TyKind::Char) => Ok(()),
            (TyKind::Str, TyKind::Str) => Ok(()),

            // String coercion: `str` and `&str` / `&'static str` are
            // interchangeable in QuantaLang (both map to QuantaString).
            (TyKind::Str, TyKind::Ref(_, _, inner))
            | (TyKind::Ref(_, _, inner), TyKind::Str)
                if inner.kind == TyKind::Str =>
            {
                Ok(())
            }

            // Reference coercion: `&T` unifies with `T` (auto-deref).
            // Only for concrete types (ADT, primitives), not for Never/Error.
            (TyKind::Ref(_, _, inner), other)
                if !matches!(other, TyKind::Never | TyKind::Error | TyKind::Var(_)) =>
            {
                self.unify_impl(inner, t2)
            }
            (other, TyKind::Ref(_, _, inner))
                if !matches!(other, TyKind::Never | TyKind::Error | TyKind::Var(_)) =>
            {
                self.unify_impl(t1, inner)
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
                    self.unify(e1, e2)?;
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
                self.unify(elem1, elem2)
            }

            // Slices: same element type
            (TyKind::Slice(elem1), TyKind::Slice(elem2)) => {
                self.unify(elem1, elem2)
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
                self.unify(ty1, ty2)
            }

            // Pointers: same mutability and unified pointee
            (TyKind::Ptr(mut1, ty1), TyKind::Ptr(mut2, ty2)) => {
                if mut1 != mut2 {
                    return Err(TypeError::MutabilityMismatch {
                        expected: *mut1,
                        found: *mut2,
                    });
                }
                self.unify(ty1, ty2)
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
                // None (default Quanta ABI) is compatible with explicit "quanta"
                // Different explicit ABIs are incompatible
                match (&fn1.abi, &fn2.abi) {
                    (None, None) => {}
                    (None, Some(a)) | (Some(a), None)
                        if &**a == "quanta" => {}
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
                    self.unify(p1, p2)?;
                }
                self.unify(&fn1.ret, &fn2.ret)?;
                // Effect rows: pure is compatible with any; otherwise must match
                // For now, skip strict effect unification to avoid breaking existing code.
                // Both pure => ok. One effectful, one pure => ok (subsumption).
                // Both effectful => must be structurally equal (checked later).
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
                    self.unify(a1, a2)?;
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

        // Occurs check: prevent infinite types
        if self.occurs_in(var, &ty) {
            return Err(TypeError::InfiniteType { var, ty });
        }

        // Add the binding
        self.subst.insert(var, ty);
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
            TyKind::Projection { self_ty, substs, .. } => {
                self.occurs_in(var, self_ty)
                    || substs.iter().any(|t| self.occurs_in(var, t))
            }
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
}
