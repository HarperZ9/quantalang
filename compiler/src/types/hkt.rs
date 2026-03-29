// ===============================================================================
// QUANTALANG TYPE SYSTEM - HIGHER-KINDED TYPES
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Higher-kinded types (HKT) for QuantaLang.
//!
//! This module implements a kind system that allows type constructors to be
//! used as first-class citizens. For example:
//!
//! ```quanta
//! // Functor trait with higher-kinded self type
//! trait Functor<F: * -> *> {
//!     fn map<A, B>(self: F<A>, f: fn(A) -> B) -> F<B>;
//! }
//!
//! // Monad trait
//! trait Monad<M: * -> *> {
//!     fn pure<A>(value: A) -> M<A>;
//!     fn flatMap<A, B>(self: M<A>, f: fn(A) -> M<B>) -> M<B>;
//! }
//! ```
//!
//! ## Kind System
//!
//! Kinds classify types:
//! - `*` (Type): The kind of concrete types like `i32`, `String`
//! - `* -> *`: Type constructors taking one type (like `Vec`, `Option`)
//! - `* -> * -> *`: Type constructors taking two types (like `Result`, `HashMap`)
//! - `Lifetime`: The kind of lifetimes
//! - `Const T`: The kind of const values of type T

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use super::{DefId, Ty, TyKind};

/// A unique identifier for kind variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KindVarId(pub u32);

impl KindVarId {
    /// Create a fresh kind variable ID.
    pub fn fresh() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl fmt::Display for KindVarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "?K{}", self.0)
    }
}

/// A kind in the kind system.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Kind {
    /// The kind of concrete types (also written as `*` or `Type`).
    Type,

    /// A type constructor kind: `K1 -> K2`.
    /// For example, `Vec` has kind `* -> *`.
    Arrow(Box<Kind>, Box<Kind>),

    /// The kind of lifetimes.
    Lifetime,

    /// The kind of const values of a particular type.
    Const(Box<Ty>),

    /// A kind variable (for kind inference).
    Var(KindVarId),

    /// A row kind for effect rows.
    Row,

    /// An effect kind.
    Effect,

    /// Error kind (for error recovery).
    Error,
}

impl Kind {
    /// The kind of concrete types.
    pub fn ty() -> Self {
        Kind::Type
    }

    /// A unary type constructor kind (* -> *).
    pub fn unary() -> Self {
        Kind::Arrow(Box::new(Kind::Type), Box::new(Kind::Type))
    }

    /// A binary type constructor kind (* -> * -> *).
    pub fn binary() -> Self {
        Kind::Arrow(
            Box::new(Kind::Type),
            Box::new(Kind::Arrow(Box::new(Kind::Type), Box::new(Kind::Type))),
        )
    }

    /// Create an n-ary type constructor kind.
    pub fn nary(n: usize) -> Self {
        if n == 0 {
            Kind::Type
        } else {
            Kind::Arrow(Box::new(Kind::Type), Box::new(Kind::nary(n - 1)))
        }
    }

    /// Create a fresh kind variable.
    pub fn fresh_var() -> Self {
        Kind::Var(KindVarId::fresh())
    }

    /// Check if this is a type kind.
    pub fn is_type(&self) -> bool {
        matches!(self, Kind::Type)
    }

    /// Check if this is a type constructor kind.
    pub fn is_constructor(&self) -> bool {
        matches!(self, Kind::Arrow(_, _))
    }

    /// Get the arity of a type constructor kind.
    pub fn arity(&self) -> usize {
        match self {
            Kind::Arrow(_, result) => 1 + result.arity(),
            _ => 0,
        }
    }

    /// Apply a kind to another, returning the result kind.
    pub fn apply(&self, _arg: &Kind) -> Option<Kind> {
        match self {
            Kind::Arrow(param, result) => {
                // In a full implementation, we'd check that arg matches param
                let _ = param;
                Some((**result).clone())
            }
            _ => None,
        }
    }

    /// Check if this kind contains any kind variables.
    pub fn has_vars(&self) -> bool {
        match self {
            Kind::Var(_) => true,
            Kind::Arrow(k1, k2) => k1.has_vars() || k2.has_vars(),
            Kind::Const(ty) => ty.has_vars(),
            _ => false,
        }
    }

    /// Substitute kind variables.
    pub fn substitute(&self, subst: &KindSubstitution) -> Kind {
        match self {
            Kind::Var(id) => {
                if let Some(kind) = subst.get(*id) {
                    kind.substitute(subst)
                } else {
                    self.clone()
                }
            }
            Kind::Arrow(k1, k2) => Kind::Arrow(
                Box::new(k1.substitute(subst)),
                Box::new(k2.substitute(subst)),
            ),
            _ => self.clone(),
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Type => write!(f, "*"),
            Kind::Arrow(k1, k2) => {
                // Parenthesize left side if it's also an arrow
                if matches!(**k1, Kind::Arrow(_, _)) {
                    write!(f, "({}) -> {}", k1, k2)
                } else {
                    write!(f, "{} -> {}", k1, k2)
                }
            }
            Kind::Lifetime => write!(f, "Lifetime"),
            Kind::Const(ty) => write!(f, "Const {}", ty),
            Kind::Var(id) => write!(f, "{}", id),
            Kind::Row => write!(f, "Row"),
            Kind::Effect => write!(f, "Effect"),
            Kind::Error => write!(f, "{{error}}"),
        }
    }
}

/// A substitution for kind variables.
#[derive(Debug, Clone, Default)]
pub struct KindSubstitution {
    map: HashMap<KindVarId, Kind>,
}

impl KindSubstitution {
    /// Create an empty substitution.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a mapping.
    pub fn insert(&mut self, var: KindVarId, kind: Kind) {
        self.map.insert(var, kind);
    }

    /// Get the kind for a variable.
    pub fn get(&self, var: KindVarId) -> Option<&Kind> {
        self.map.get(&var)
    }

    /// Check if a variable is bound.
    pub fn contains(&self, var: KindVarId) -> bool {
        self.map.contains_key(&var)
    }
}

/// A higher-kinded type parameter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HKTParam {
    /// Parameter name.
    pub name: Arc<str>,
    /// Parameter index.
    pub index: u32,
    /// The kind of this parameter.
    pub kind: Kind,
}

impl HKTParam {
    /// Create a new HKT parameter.
    pub fn new(name: impl Into<Arc<str>>, index: u32, kind: Kind) -> Self {
        Self {
            name: name.into(),
            index,
            kind,
        }
    }

    /// Create a type parameter (kind *).
    pub fn type_param(name: impl Into<Arc<str>>, index: u32) -> Self {
        Self::new(name, index, Kind::Type)
    }

    /// Create a unary type constructor parameter (kind * -> *).
    pub fn constructor_param(name: impl Into<Arc<str>>, index: u32) -> Self {
        Self::new(name, index, Kind::unary())
    }
}

impl fmt::Display for HKTParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.kind.is_type() {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{}: {}", self.name, self.kind)
        }
    }
}

/// A type constructor - a type that takes type arguments.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeConstructor {
    /// The definition ID.
    pub def_id: DefId,
    /// The name.
    pub name: Arc<str>,
    /// Type parameters with their kinds.
    pub params: Vec<HKTParam>,
    /// The kind of the fully-applied type.
    pub result_kind: Kind,
}

impl TypeConstructor {
    /// Create a new type constructor.
    pub fn new(def_id: DefId, name: impl Into<Arc<str>>, params: Vec<HKTParam>) -> Self {
        Self {
            def_id,
            name: name.into(),
            params,
            result_kind: Kind::Type,
        }
    }

    /// Get the kind of this type constructor.
    pub fn kind(&self) -> Kind {
        self.params
            .iter()
            .rev()
            .fold(self.result_kind.clone(), |acc, param| {
                Kind::Arrow(Box::new(param.kind.clone()), Box::new(acc))
            })
    }

    /// Check if all parameters are of kind *.
    pub fn is_simple(&self) -> bool {
        self.params.iter().all(|p| p.kind.is_type())
    }

    /// Get the arity (number of type parameters).
    pub fn arity(&self) -> usize {
        self.params.len()
    }
}

/// A partially applied type constructor.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PartialApp {
    /// The type constructor.
    pub constructor: Arc<TypeConstructor>,
    /// Applied type arguments.
    pub args: Vec<Ty>,
}

impl PartialApp {
    /// Create a new partial application.
    pub fn new(constructor: Arc<TypeConstructor>, args: Vec<Ty>) -> Self {
        Self { constructor, args }
    }

    /// Get the remaining kind after application.
    pub fn remaining_kind(&self) -> Kind {
        let full_kind = self.constructor.kind();
        let mut current = full_kind;

        for _ in &self.args {
            if let Kind::Arrow(_, result) = current {
                current = *result;
            } else {
                return Kind::Error;
            }
        }

        current
    }

    /// Check if fully applied.
    pub fn is_fully_applied(&self) -> bool {
        self.args.len() == self.constructor.arity()
    }

    /// Apply one more type argument.
    pub fn apply(&self, arg: Ty) -> Self {
        let mut args = self.args.clone();
        args.push(arg);
        Self {
            constructor: self.constructor.clone(),
            args,
        }
    }
}

/// Kind checking context.
#[derive(Debug, Default)]
pub struct KindContext {
    /// Type parameter kinds.
    type_params: HashMap<Arc<str>, Kind>,
    /// Type constructor kinds.
    constructors: HashMap<DefId, Arc<TypeConstructor>>,
    /// Kind substitution from unification.
    subst: KindSubstitution,
}

impl KindContext {
    /// Create a new kind context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a type parameter.
    pub fn register_param(&mut self, name: Arc<str>, kind: Kind) {
        self.type_params.insert(name, kind);
    }

    /// Register a type constructor.
    pub fn register_constructor(&mut self, tc: TypeConstructor) {
        self.constructors.insert(tc.def_id, Arc::new(tc));
    }

    /// Get the kind of a type parameter.
    pub fn param_kind(&self, name: &str) -> Option<&Kind> {
        self.type_params.get(name)
    }

    /// Get a type constructor.
    pub fn get_constructor(&self, def_id: DefId) -> Option<&Arc<TypeConstructor>> {
        self.constructors.get(&def_id)
    }

    /// Infer the kind of a type.
    pub fn infer_kind(&mut self, ty: &Ty) -> Result<Kind, KindError> {
        match &ty.kind {
            // Primitive types have kind *
            TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Bool
            | TyKind::Char
            | TyKind::Str
            | TyKind::Never => Ok(Kind::Type),

            // Compound types have kind *
            TyKind::Tuple(_)
            | TyKind::Array(_, _)
            | TyKind::Slice(_)
            | TyKind::Ref(_, _, _)
            | TyKind::Ptr(_, _)
            | TyKind::Fn(_) => Ok(Kind::Type),

            // Type variables default to kind *
            TyKind::Var(_) | TyKind::Infer(_) => Ok(Kind::Type),

            // Type parameters have their declared kind
            TyKind::Param(name, _) => self
                .param_kind(name)
                .cloned()
                .ok_or_else(|| KindError::UnboundTypeParam(name.to_string())),

            // ADTs: check constructor and arguments
            TyKind::Adt(def_id, args) => {
                // Clone the constructor info to avoid borrow conflict
                let tc_info = self
                    .get_constructor(*def_id)
                    .map(|tc| (tc.arity(), tc.name.to_string(), tc.params.clone()));

                if let Some((arity, name, params)) = tc_info {
                    // Check arity
                    if args.len() != arity {
                        return Err(KindError::ArityMismatch {
                            expected: arity,
                            found: args.len(),
                            name,
                        });
                    }

                    // Check each argument has the right kind
                    for (arg, param) in args.iter().zip(&params) {
                        let arg_kind = self.infer_kind(arg)?;
                        self.unify_kinds(&arg_kind, &param.kind)?;
                    }

                    Ok(Kind::Type)
                } else {
                    // Unknown constructor, assume kind *
                    Ok(Kind::Type)
                }
            }

            // Projections have kind *
            TyKind::Projection { .. } => Ok(Kind::Type),

            // Trait objects have kind *
            TyKind::TraitObject(_) => Ok(Kind::Type),

            // Error types have error kind
            TyKind::Error => Ok(Kind::Error),
        }
    }

    /// Unify two kinds.
    pub fn unify_kinds(&mut self, k1: &Kind, k2: &Kind) -> Result<(), KindError> {
        let k1 = k1.substitute(&self.subst);
        let k2 = k2.substitute(&self.subst);

        match (&k1, &k2) {
            // Same kind
            (Kind::Type, Kind::Type) => Ok(()),
            (Kind::Lifetime, Kind::Lifetime) => Ok(()),
            (Kind::Row, Kind::Row) => Ok(()),
            (Kind::Effect, Kind::Effect) => Ok(()),
            (Kind::Error, _) | (_, Kind::Error) => Ok(()),

            // Arrow kinds
            (Kind::Arrow(a1, r1), Kind::Arrow(a2, r2)) => {
                self.unify_kinds(a1, a2)?;
                self.unify_kinds(r1, r2)
            }

            // Const kinds
            (Kind::Const(t1), Kind::Const(t2)) if t1 == t2 => Ok(()),

            // Kind variables
            (Kind::Var(v), k) | (k, Kind::Var(v)) => {
                if self.occurs_check(*v, k) {
                    Err(KindError::InfiniteKind(*v))
                } else {
                    self.subst.insert(*v, k.clone());
                    Ok(())
                }
            }

            // Mismatch
            _ => Err(KindError::Mismatch {
                expected: k1.clone(),
                found: k2.clone(),
            }),
        }
    }

    /// Check if a kind variable occurs in a kind.
    fn occurs_check(&self, var: KindVarId, kind: &Kind) -> bool {
        match kind {
            Kind::Var(v) => *v == var,
            Kind::Arrow(k1, k2) => self.occurs_check(var, k1) || self.occurs_check(var, k2),
            _ => false,
        }
    }

    /// Apply the current substitution to a kind.
    pub fn apply_subst(&self, kind: &Kind) -> Kind {
        kind.substitute(&self.subst)
    }
}

/// Kind checking errors.
#[derive(Debug, Clone)]
pub enum KindError {
    /// Kind mismatch.
    Mismatch { expected: Kind, found: Kind },

    /// Wrong number of type arguments.
    ArityMismatch {
        expected: usize,
        found: usize,
        name: String,
    },

    /// Unbound type parameter.
    UnboundTypeParam(String),

    /// Infinite kind (occurs check failure).
    InfiniteKind(KindVarId),

    /// Cannot apply a non-constructor kind.
    NotAConstructor(Kind),
}

impl fmt::Display for KindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KindError::Mismatch { expected, found } => {
                write!(f, "kind mismatch: expected {}, found {}", expected, found)
            }
            KindError::ArityMismatch {
                expected,
                found,
                name,
            } => {
                write!(
                    f,
                    "wrong number of type arguments for '{}': expected {}, found {}",
                    name, expected, found
                )
            }
            KindError::UnboundTypeParam(name) => {
                write!(f, "unbound type parameter: {}", name)
            }
            KindError::InfiniteKind(var) => {
                write!(f, "infinite kind: {} occurs in its own definition", var)
            }
            KindError::NotAConstructor(kind) => {
                write!(f, "expected a type constructor, found kind {}", kind)
            }
        }
    }
}

impl std::error::Error for KindError {}

/// Built-in type constructors.
pub fn builtin_constructors() -> Vec<TypeConstructor> {
    vec![
        // Option<T>: * -> *
        TypeConstructor::new(
            DefId::new(0, 0),
            "Option",
            vec![HKTParam::type_param("T", 0)],
        ),
        // Result<T, E>: * -> * -> *
        TypeConstructor::new(
            DefId::new(0, 1),
            "Result",
            vec![HKTParam::type_param("T", 0), HKTParam::type_param("E", 1)],
        ),
        // Vec<T>: * -> *
        TypeConstructor::new(DefId::new(0, 2), "Vec", vec![HKTParam::type_param("T", 0)]),
        // HashMap<K, V>: * -> * -> *
        TypeConstructor::new(
            DefId::new(0, 3),
            "HashMap",
            vec![HKTParam::type_param("K", 0), HKTParam::type_param("V", 1)],
        ),
        // Box<T>: * -> *
        TypeConstructor::new(DefId::new(0, 4), "Box", vec![HKTParam::type_param("T", 0)]),
        // Rc<T>: * -> *
        TypeConstructor::new(DefId::new(0, 5), "Rc", vec![HKTParam::type_param("T", 0)]),
        // Arc<T>: * -> *
        TypeConstructor::new(DefId::new(0, 6), "Arc", vec![HKTParam::type_param("T", 0)]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kind_display() {
        assert_eq!(format!("{}", Kind::Type), "*");
        assert_eq!(format!("{}", Kind::unary()), "* -> *");
        assert_eq!(format!("{}", Kind::binary()), "* -> * -> *");
        assert_eq!(format!("{}", Kind::nary(3)), "* -> * -> * -> *");
    }

    #[test]
    fn test_kind_arity() {
        assert_eq!(Kind::Type.arity(), 0);
        assert_eq!(Kind::unary().arity(), 1);
        assert_eq!(Kind::binary().arity(), 2);
        assert_eq!(Kind::nary(5).arity(), 5);
    }

    #[test]
    fn test_type_constructor() {
        let tc = TypeConstructor::new(
            DefId::new(0, 0),
            "Result",
            vec![HKTParam::type_param("T", 0), HKTParam::type_param("E", 1)],
        );

        assert_eq!(tc.arity(), 2);
        assert_eq!(format!("{}", tc.kind()), "* -> * -> *");
    }

    #[test]
    fn test_kind_unification() {
        let mut ctx = KindContext::new();

        // Unify * with *
        assert!(ctx.unify_kinds(&Kind::Type, &Kind::Type).is_ok());

        // Unify (* -> *) with (* -> *)
        assert!(ctx.unify_kinds(&Kind::unary(), &Kind::unary()).is_ok());

        // Unify variable with *
        let v = Kind::fresh_var();
        assert!(ctx.unify_kinds(&v, &Kind::Type).is_ok());

        // Mismatch
        let mut ctx2 = KindContext::new();
        assert!(ctx2.unify_kinds(&Kind::Type, &Kind::unary()).is_err());
    }

    #[test]
    fn test_partial_application() {
        let tc = Arc::new(TypeConstructor::new(
            DefId::new(0, 0),
            "Result",
            vec![HKTParam::type_param("T", 0), HKTParam::type_param("E", 1)],
        ));

        let partial = PartialApp::new(tc.clone(), vec![]);
        assert!(!partial.is_fully_applied());
        assert_eq!(format!("{}", partial.remaining_kind()), "* -> * -> *");

        let partial2 = partial.apply(Ty::int(super::super::IntTy::I32));
        assert!(!partial2.is_fully_applied());
        assert_eq!(format!("{}", partial2.remaining_kind()), "* -> *");

        let partial3 = partial2.apply(Ty::str());
        assert!(partial3.is_fully_applied());
        assert_eq!(format!("{}", partial3.remaining_kind()), "*");
    }
}
