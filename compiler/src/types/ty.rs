// ===============================================================================
// QUANTALANG TYPE SYSTEM - TYPE REPRESENTATION
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Core type representation for the type system.
//!
//! Types in QuantaLang are represented as a tree structure with sharing
//! through interning. Type variables are used during inference and are
//! resolved through unification.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// A unique identifier for type variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TyVarId(pub u32);

impl TyVarId {
    /// Create a fresh type variable ID.
    pub fn fresh() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

/// A unique identifier for lifetime variables used during borrow checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LifetimeVarId(pub u32);

impl LifetimeVarId {
    pub fn fresh() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl fmt::Display for LifetimeVarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "'?{}", self.0)
    }
}

/// A lifetime in the borrow checker. Either a concrete named lifetime,
/// an inference variable, or the special 'static lifetime.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LifetimeKind {
    /// A named lifetime from source: 'a, 'b
    Named(Arc<str>),
    /// An inference variable assigned during borrow checking
    Var(LifetimeVarId),
    /// The 'static lifetime — lives for the entire program
    Static,
}

impl fmt::Display for LifetimeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LifetimeKind::Named(name) => write!(f, "'{}", name),
            LifetimeKind::Var(id) => write!(f, "{}", id),
            LifetimeKind::Static => write!(f, "'static"),
        }
    }
}

/// A constraint between two lifetimes: the left must outlive the right.
/// 'a: 'b means 'a lives at least as long as 'b.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlivesConstraint {
    /// The longer lifetime (must outlive `shorter`).
    pub longer: LifetimeKind,
    /// The shorter lifetime (must be outlived by `longer`).
    pub shorter: LifetimeKind,
}

/// Tracks borrows and lifetime constraints within a function body.
#[derive(Debug, Clone)]
pub struct BorrowState {
    /// Active borrows: variable name → (lifetime, mutability, span of borrow site)
    pub borrows: Vec<BorrowEntry>,
    /// Collected lifetime constraints from reference usage.
    pub constraints: Vec<OutlivesConstraint>,
    /// Scope depth counter for assigning scope-based lifetimes.
    scope_depth: u32,
}

/// A single active borrow.
#[derive(Debug, Clone)]
pub struct BorrowEntry {
    /// The variable being borrowed.
    pub variable: Arc<str>,
    /// The lifetime assigned to this borrow.
    pub lifetime: LifetimeKind,
    /// Whether this is a mutable borrow.
    pub is_mut: bool,
    /// The scope depth where the borrow was created.
    pub scope_depth: u32,
}

impl BorrowState {
    pub fn new() -> Self {
        Self {
            borrows: Vec::new(),
            constraints: Vec::new(),
            scope_depth: 0,
        }
    }

    /// Enter a new scope (block, loop, etc.)
    pub fn push_scope(&mut self) {
        self.scope_depth += 1;
    }

    /// Leave a scope — invalidate all borrows created in this scope.
    pub fn pop_scope(&mut self) {
        self.borrows.retain(|b| b.scope_depth < self.scope_depth);
        self.scope_depth -= 1;
    }

    /// Record a new borrow of a variable.
    pub fn add_borrow(&mut self, variable: Arc<str>, is_mut: bool) -> LifetimeKind {
        let lifetime = LifetimeKind::Var(LifetimeVarId::fresh());
        self.borrows.push(BorrowEntry {
            variable,
            lifetime: lifetime.clone(),
            is_mut,
            scope_depth: self.scope_depth,
        });
        lifetime
    }

    /// Check if a variable has an active mutable borrow.
    pub fn has_mut_borrow(&self, variable: &str) -> bool {
        self.borrows.iter().any(|b| b.variable.as_ref() == variable && b.is_mut)
    }

    /// Check if a variable has any active borrow (shared or mutable).
    pub fn has_any_borrow(&self, variable: &str) -> bool {
        self.borrows.iter().any(|b| b.variable.as_ref() == variable)
    }

    /// Get all active borrows of a variable.
    pub fn borrows_of(&self, variable: &str) -> Vec<&BorrowEntry> {
        self.borrows.iter().filter(|b| b.variable.as_ref() == variable).collect()
    }

    /// Add a constraint: `longer` must outlive `shorter`.
    pub fn add_outlives(&mut self, longer: LifetimeKind, shorter: LifetimeKind) {
        self.constraints.push(OutlivesConstraint { longer, shorter });
    }

    /// Current scope depth.
    pub fn current_scope_depth(&self) -> u32 {
        self.scope_depth
    }
}

impl fmt::Display for TyVarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display as ?T0, ?T1, etc.
        write!(f, "?T{}", self.0)
    }
}

/// A type in the QuantaLang type system.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ty {
    /// The kind of type.
    pub kind: TyKind,
    /// Optional type annotations (e.g., `with ColorSpace<Linear>`).
    /// These are compile-time metadata for color space safety, precision tracking, etc.
    /// An empty vec means no annotations (the common case).
    pub annotations: Vec<Arc<str>>,
}

impl Ty {
    /// Create a new type.
    pub fn new(kind: TyKind) -> Self {
        Self { kind, annotations: Vec::new() }
    }

    /// Create a type with annotations (e.g., `with ColorSpace<Linear>`).
    pub fn with_annotations(kind: TyKind, annotations: Vec<Arc<str>>) -> Self {
        Self { kind, annotations }
    }

    /// Check if this type has a specific annotation.
    pub fn has_annotation(&self, name: &str) -> bool {
        self.annotations.iter().any(|a| a.as_ref() == name)
    }

    /// Create a type variable.
    pub fn var(id: TyVarId) -> Self {
        Self::new(TyKind::Var(id))
    }

    /// Create a fresh type variable.
    pub fn fresh_var() -> Self {
        Self::var(TyVarId::fresh())
    }

    /// Create an integer type.
    pub fn int(int_ty: IntTy) -> Self {
        Self::new(TyKind::Int(int_ty))
    }

    /// Create a float type.
    pub fn float(float_ty: FloatTy) -> Self {
        Self::new(TyKind::Float(float_ty))
    }

    /// Create a boolean type.
    pub fn bool() -> Self {
        Self::new(TyKind::Bool)
    }

    /// Create a char type.
    pub fn char() -> Self {
        Self::new(TyKind::Char)
    }

    /// Create a string slice type.
    pub fn str() -> Self {
        Self::new(TyKind::Str)
    }

    /// Create the unit type.
    pub fn unit() -> Self {
        Self::new(TyKind::Tuple(Vec::new()))
    }

    /// Create the never type.
    pub fn never() -> Self {
        Self::new(TyKind::Never)
    }

    /// Create a tuple type.
    pub fn tuple(elements: Vec<Ty>) -> Self {
        Self::new(TyKind::Tuple(elements))
    }

    /// Create an array type.
    pub fn array(elem: Ty, len: usize) -> Self {
        Self::new(TyKind::Array(Box::new(elem), len))
    }

    /// Create a slice type.
    pub fn slice(elem: Ty) -> Self {
        Self::new(TyKind::Slice(Box::new(elem)))
    }

    /// Create a reference type.
    pub fn reference(lifetime: Option<Lifetime>, mutability: Mutability, ty: Ty) -> Self {
        Self::new(TyKind::Ref(lifetime, mutability, Box::new(ty)))
    }

    /// Create a raw pointer type.
    pub fn ptr(mutability: Mutability, ty: Ty) -> Self {
        Self::new(TyKind::Ptr(mutability, Box::new(ty)))
    }

    /// Create a function type (pure, no effects).
    pub fn function(params: Vec<Ty>, ret: Ty) -> Self {
        Self::new(TyKind::Fn(FnTy {
            params,
            ret: Box::new(ret),
            is_unsafe: false,
            abi: None,
            effects: super::effects::EffectRow::empty(),
        }))
    }

    /// Create a function type with an effect row.
    pub fn function_with_effects(params: Vec<Ty>, ret: Ty, effects: super::effects::EffectRow) -> Self {
        Self::new(TyKind::Fn(FnTy {
            params,
            ret: Box::new(ret),
            is_unsafe: false,
            abi: None,
            effects,
        }))
    }

    /// Create an ADT (struct/enum) type.
    pub fn adt(def_id: DefId, substs: Vec<Ty>) -> Self {
        Self::new(TyKind::Adt(def_id, substs))
    }

    /// Create a type parameter.
    pub fn param(name: impl Into<Arc<str>>, index: u32) -> Self {
        Self::new(TyKind::Param(name.into(), index))
    }

    /// Create an error type (for error recovery).
    pub fn error() -> Self {
        Self::new(TyKind::Error)
    }

    /// Create a String type (as a well-known type placeholder).
    /// In a full implementation, this would use a real DefId for String.
    pub fn string() -> Self {
        // For now, represent String as a fresh type variable
        // TODO: Use proper String ADT when type context is available
        Self::fresh_var()
    }

    /// Create a Vec<T> type (as a well-known type placeholder).
    /// In a full implementation, this would use a real DefId for Vec.
    pub fn vec(_elem: Ty) -> Self {
        // For now, represent Vec<T> as a fresh type variable
        // TODO: Use proper Vec ADT when type context is available
        Self::fresh_var()
    }

    /// Check if this is a type variable.
    pub fn is_var(&self) -> bool {
        matches!(self.kind, TyKind::Var(_))
    }

    /// Check if this is the unit type.
    pub fn is_unit(&self) -> bool {
        matches!(&self.kind, TyKind::Tuple(elems) if elems.is_empty())
    }

    /// Check if this is the never type.
    pub fn is_never(&self) -> bool {
        matches!(self.kind, TyKind::Never)
    }

    /// Check if this is an error type.
    pub fn is_error(&self) -> bool {
        matches!(self.kind, TyKind::Error)
    }

    /// Check if this type contains any type variables.
    pub fn has_vars(&self) -> bool {
        match &self.kind {
            TyKind::Var(_) => true,
            TyKind::Int(_) | TyKind::Float(_) | TyKind::Bool | TyKind::Char
            | TyKind::Str | TyKind::Never | TyKind::Error => false,
            TyKind::Tuple(elems) => elems.iter().any(|t| t.has_vars()),
            TyKind::Array(elem, _) => elem.has_vars(),
            TyKind::Slice(elem) => elem.has_vars(),
            TyKind::Ref(_, _, ty) => ty.has_vars(),
            TyKind::Ptr(_, ty) => ty.has_vars(),
            TyKind::Fn(fn_ty) => {
                fn_ty.params.iter().any(|t| t.has_vars()) || fn_ty.ret.has_vars()
            }
            TyKind::Adt(_, substs) => substs.iter().any(|t| t.has_vars()),
            TyKind::Param(_, _) => false,
            TyKind::Projection { .. } => true, // Projections need resolution
            TyKind::Infer(_) => true,
            TyKind::TraitObject(_) => false,
        }
    }

    /// Collect all free type variables in this type.
    pub fn free_vars(&self) -> HashSet<TyVarId> {
        let mut vars = HashSet::new();
        self.collect_vars(&mut vars);
        vars
    }

    fn collect_vars(&self, vars: &mut HashSet<TyVarId>) {
        match &self.kind {
            TyKind::Var(id) => {
                vars.insert(*id);
            }
            TyKind::Tuple(elems) => {
                for elem in elems {
                    elem.collect_vars(vars);
                }
            }
            TyKind::Array(elem, _) | TyKind::Slice(elem) => {
                elem.collect_vars(vars);
            }
            TyKind::Ref(_, _, ty) | TyKind::Ptr(_, ty) => {
                ty.collect_vars(vars);
            }
            TyKind::Fn(fn_ty) => {
                for param in &fn_ty.params {
                    param.collect_vars(vars);
                }
                fn_ty.ret.collect_vars(vars);
            }
            TyKind::Adt(_, substs) => {
                for subst in substs {
                    subst.collect_vars(vars);
                }
            }
            TyKind::Projection { self_ty, substs, .. } => {
                self_ty.collect_vars(vars);
                for subst in substs {
                    subst.collect_vars(vars);
                }
            }
            TyKind::Infer(infer) => {
                vars.insert(infer.var);
            }
            _ => {}
        }
    }

    /// Substitute type variables according to a substitution.
    pub fn substitute(&self, subst: &Substitution) -> Ty {
        let mut result = match &self.kind {
            TyKind::Var(id) => {
                if let Some(ty) = subst.get(*id) {
                    ty.substitute(subst)
                } else {
                    return self.clone();
                }
            }
            TyKind::Tuple(elems) => {
                Ty::tuple(elems.iter().map(|t| t.substitute(subst)).collect())
            }
            TyKind::Array(elem, len) => {
                Ty::array(elem.substitute(subst), *len)
            }
            TyKind::Slice(elem) => {
                Ty::slice(elem.substitute(subst))
            }
            TyKind::Ref(lifetime, mutability, ty) => {
                Ty::reference(lifetime.clone(), *mutability, ty.substitute(subst))
            }
            TyKind::Ptr(mutability, ty) => {
                Ty::ptr(*mutability, ty.substitute(subst))
            }
            TyKind::Fn(fn_ty) => {
                Ty::new(TyKind::Fn(FnTy {
                    params: fn_ty.params.iter().map(|t| t.substitute(subst)).collect(),
                    ret: Box::new(fn_ty.ret.substitute(subst)),
                    is_unsafe: fn_ty.is_unsafe,
                    abi: fn_ty.abi.clone(),
                    effects: fn_ty.effects.clone(),
                }))
            }
            TyKind::Adt(def_id, substs) => {
                Ty::adt(*def_id, substs.iter().map(|t| t.substitute(subst)).collect())
            }
            TyKind::Projection { trait_ref, item, self_ty, substs } => {
                Ty::new(TyKind::Projection {
                    trait_ref: trait_ref.clone(),
                    item: item.clone(),
                    self_ty: Box::new(self_ty.substitute(subst)),
                    substs: substs.iter().map(|t| t.substitute(subst)).collect(),
                })
            }
            TyKind::Infer(infer) => {
                if let Some(ty) = subst.get(infer.var) {
                    ty.substitute(subst)
                } else {
                    self.clone()
                }
            }
            _ => return self.clone(),
        };
        // Preserve annotations from the original type through substitution
        if !self.annotations.is_empty() && result.annotations.is_empty() {
            result.annotations = self.annotations.clone();
        }
        result
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            TyKind::Var(id) => write!(f, "{}", id),
            TyKind::Int(int_ty) => write!(f, "{}", int_ty),
            TyKind::Float(float_ty) => write!(f, "{}", float_ty),
            TyKind::Bool => write!(f, "bool"),
            TyKind::Char => write!(f, "char"),
            TyKind::Str => write!(f, "str"),
            TyKind::Never => write!(f, "!"),
            TyKind::Tuple(elems) => {
                if elems.is_empty() {
                    write!(f, "()")
                } else {
                    write!(f, "(")?;
                    for (i, elem) in elems.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", elem)?;
                    }
                    if elems.len() == 1 {
                        write!(f, ",")?;
                    }
                    write!(f, ")")
                }
            }
            TyKind::Array(elem, len) => write!(f, "[{}; {}]", elem, len),
            TyKind::Slice(elem) => write!(f, "[{}]", elem),
            TyKind::Ref(lifetime, mutability, ty) => {
                write!(f, "&")?;
                if let Some(lt) = lifetime {
                    write!(f, "{} ", lt)?;
                }
                if *mutability == Mutability::Mutable {
                    write!(f, "mut ")?;
                }
                write!(f, "{}", ty)
            }
            TyKind::Ptr(mutability, ty) => {
                write!(f, "*")?;
                if *mutability == Mutability::Mutable {
                    write!(f, "mut ")?;
                } else {
                    write!(f, "const ")?;
                }
                write!(f, "{}", ty)
            }
            TyKind::Fn(fn_ty) => {
                if fn_ty.is_unsafe {
                    write!(f, "unsafe ")?;
                }
                if let Some(abi) = &fn_ty.abi {
                    write!(f, "extern \"{}\" ", abi)?;
                }
                write!(f, "fn(")?;
                for (i, param) in fn_ty.params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", param)?;
                }
                write!(f, ")")?;
                if !fn_ty.ret.is_unit() {
                    write!(f, " -> {}", fn_ty.ret)?;
                }
                if !fn_ty.effects.is_empty() {
                    write!(f, " ~ {}", fn_ty.effects)?;
                }
                Ok(())
            }
            TyKind::Adt(def_id, substs) => {
                write!(f, "{}", def_id)?;
                if !substs.is_empty() {
                    write!(f, "<")?;
                    for (i, subst) in substs.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", subst)?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            TyKind::Param(name, _) => write!(f, "{}", name),
            TyKind::Projection { trait_ref, item, .. } => {
                write!(f, "<{} as {}>::{}", trait_ref, trait_ref, item)
            }
            TyKind::Infer(infer) => write!(f, "?{}", infer.var.0),
            TyKind::TraitObject(bounds) => {
                write!(f, "dyn {}", bounds.iter().map(|b| b.as_ref()).collect::<Vec<_>>().join(" + "))
            }
            TyKind::Error => write!(f, "{{error}}"),
        }?;
        // Append color space / precision annotations if present
        if !self.annotations.is_empty() {
            write!(f, " with ")?;
            for (i, ann) in self.annotations.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                // Format "ColorSpace:Linear" as "ColorSpace<Linear>"
                if let Some(colon_pos) = ann.find(':') {
                    let (cat, val) = ann.split_at(colon_pos);
                    write!(f, "{}<{}>", cat, &val[1..])?;
                } else {
                    write!(f, "{}", ann)?;
                }
            }
        }
        Ok(())
    }
}

/// The kind of a type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TyKind {
    // =========================================================================
    // TYPE VARIABLES
    // =========================================================================

    /// A type variable (for inference).
    Var(TyVarId),

    /// An inference variable with constraints.
    Infer(InferTy),

    // =========================================================================
    // PRIMITIVE TYPES
    // =========================================================================

    /// Integer types.
    Int(IntTy),

    /// Floating-point types.
    Float(FloatTy),

    /// Boolean type.
    Bool,

    /// Character type.
    Char,

    /// String slice type.
    Str,

    /// The never type (diverging).
    Never,

    // =========================================================================
    // COMPOUND TYPES
    // =========================================================================

    /// Tuple type.
    Tuple(Vec<Ty>),

    /// Array type with known size.
    Array(Box<Ty>, usize),

    /// Slice type.
    Slice(Box<Ty>),

    /// Reference type.
    Ref(Option<Lifetime>, Mutability, Box<Ty>),

    /// Raw pointer type.
    Ptr(Mutability, Box<Ty>),

    /// Function type.
    Fn(FnTy),

    // =========================================================================
    // USER-DEFINED TYPES
    // =========================================================================

    /// Algebraic data type (struct, enum, union).
    Adt(DefId, Vec<Ty>),

    /// Type parameter.
    Param(Arc<str>, u32),

    /// Associated type projection: `<T as Trait>::Item`
    Projection {
        trait_ref: Arc<str>,
        item: Arc<str>,
        self_ty: Box<Ty>,
        substs: Vec<Ty>,
    },

    // =========================================================================
    // TRAIT OBJECTS
    // =========================================================================

    /// Trait object: `dyn Trait`
    TraitObject(Vec<Arc<str>>),

    // =========================================================================
    // SPECIAL
    // =========================================================================

    /// Error type (for error recovery).
    Error,
}

/// Integer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntTy {
    I8, I16, I32, I64, I128, Isize,
    U8, U16, U32, U64, U128, Usize,
}

impl IntTy {
    /// Check if this is a signed integer type.
    pub fn is_signed(&self) -> bool {
        matches!(self, IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::I128 | IntTy::Isize)
    }

    /// Get the bit width (None for isize/usize).
    pub fn bit_width(&self) -> Option<u32> {
        match self {
            IntTy::I8 | IntTy::U8 => Some(8),
            IntTy::I16 | IntTy::U16 => Some(16),
            IntTy::I32 | IntTy::U32 => Some(32),
            IntTy::I64 | IntTy::U64 => Some(64),
            IntTy::I128 | IntTy::U128 => Some(128),
            IntTy::Isize | IntTy::Usize => None,
        }
    }
}

impl fmt::Display for IntTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntTy::I8 => write!(f, "i8"),
            IntTy::I16 => write!(f, "i16"),
            IntTy::I32 => write!(f, "i32"),
            IntTy::I64 => write!(f, "i64"),
            IntTy::I128 => write!(f, "i128"),
            IntTy::Isize => write!(f, "isize"),
            IntTy::U8 => write!(f, "u8"),
            IntTy::U16 => write!(f, "u16"),
            IntTy::U32 => write!(f, "u32"),
            IntTy::U64 => write!(f, "u64"),
            IntTy::U128 => write!(f, "u128"),
            IntTy::Usize => write!(f, "usize"),
        }
    }
}

/// Floating-point types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatTy {
    F16, F32, F64,
}

impl fmt::Display for FloatTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FloatTy::F16 => write!(f, "f16"),
            FloatTy::F32 => write!(f, "f32"),
            FloatTy::F64 => write!(f, "f64"),
        }
    }
}

/// Mutability marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mutability {
    Immutable,
    Mutable,
}

/// A lifetime.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Lifetime {
    pub name: Arc<str>,
}

impl Lifetime {
    pub fn new(name: impl Into<Arc<str>>) -> Self {
        Self { name: name.into() }
    }

    pub fn static_lifetime() -> Self {
        Self::new("'static")
    }

    pub fn anonymous() -> Self {
        Self::new("'_")
    }
}

impl fmt::Display for Lifetime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Function type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FnTy {
    /// Parameter types.
    pub params: Vec<Ty>,
    /// Return type.
    pub ret: Box<Ty>,
    /// Whether this is an unsafe function.
    pub is_unsafe: bool,
    /// ABI (e.g., "C", "Rust").
    pub abi: Option<Arc<str>>,
    /// Algebraic effect row for this function.
    pub effects: super::effects::EffectRow,
}

/// A definition ID for ADTs, traits, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefId {
    /// Module/crate index.
    pub krate: u32,
    /// Local definition index.
    pub index: u32,
}

impl DefId {
    pub fn new(krate: u32, index: u32) -> Self {
        Self { krate, index }
    }

    /// A dummy DefId for testing.
    pub const DUMMY: Self = Self { krate: 0, index: u32::MAX };
}

impl fmt::Display for DefId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DefId({}, {})", self.krate, self.index)
    }
}

/// An inference type variable with additional constraints.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InferTy {
    /// The underlying type variable.
    pub var: TyVarId,
    /// The kind of inference.
    pub kind: InferKind,
}

/// Kind of type inference.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InferKind {
    /// General type variable.
    Type,
    /// Integer type (defaults to i32).
    Int,
    /// Float type (defaults to f64).
    Float,
}

/// A substitution mapping type variables to types.
#[derive(Debug, Clone, Default)]
pub struct Substitution {
    map: HashMap<TyVarId, Ty>,
}

impl Substitution {
    /// Create an empty substitution.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a mapping.
    pub fn insert(&mut self, var: TyVarId, ty: Ty) {
        self.map.insert(var, ty);
    }

    /// Get the type for a variable.
    pub fn get(&self, var: TyVarId) -> Option<&Ty> {
        self.map.get(&var)
    }

    /// Check if a variable is bound.
    pub fn contains(&self, var: TyVarId) -> bool {
        self.map.contains_key(&var)
    }

    /// Compose two substitutions (self ∘ other).
    pub fn compose(&self, other: &Substitution) -> Substitution {
        let mut result = Substitution::new();

        // Apply self to all types in other
        for (var, ty) in &other.map {
            result.insert(*var, ty.substitute(self));
        }

        // Add mappings from self that aren't in other
        for (var, ty) in &self.map {
            if !result.contains(*var) {
                result.insert(*var, ty.clone());
            }
        }

        result
    }

    /// Iterate over all mappings.
    pub fn iter(&self) -> impl Iterator<Item = (&TyVarId, &Ty)> {
        self.map.iter()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// A type scheme (polymorphic type).
#[derive(Debug, Clone)]
pub struct TypeScheme {
    /// Bound type variables.
    pub vars: Vec<TyVarId>,
    /// The underlying type.
    pub ty: Ty,
}

impl TypeScheme {
    /// Create a monomorphic type scheme.
    pub fn mono(ty: Ty) -> Self {
        Self { vars: Vec::new(), ty }
    }

    /// Create a polymorphic type scheme.
    pub fn poly(vars: Vec<TyVarId>, ty: Ty) -> Self {
        Self { vars, ty }
    }

    /// Instantiate this scheme with fresh type variables.
    pub fn instantiate(&self) -> Ty {
        if self.vars.is_empty() {
            return self.ty.clone();
        }

        let mut subst = Substitution::new();
        for var in &self.vars {
            subst.insert(*var, Ty::fresh_var());
        }
        self.ty.substitute(&subst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fresh_var() {
        let v1 = TyVarId::fresh();
        let v2 = TyVarId::fresh();
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_type_display() {
        assert_eq!(format!("{}", Ty::int(IntTy::I32)), "i32");
        assert_eq!(format!("{}", Ty::bool()), "bool");
        assert_eq!(format!("{}", Ty::unit()), "()");
        assert_eq!(format!("{}", Ty::never()), "!");
        assert_eq!(format!("{}", Ty::tuple(vec![Ty::int(IntTy::I32), Ty::bool()])), "(i32, bool)");
        assert_eq!(format!("{}", Ty::array(Ty::int(IntTy::I32), 10)), "[i32; 10]");
        assert_eq!(format!("{}", Ty::slice(Ty::int(IntTy::I32))), "[i32]");
    }

    #[test]
    fn test_substitution() {
        let v1 = TyVarId::fresh();
        let ty = Ty::tuple(vec![Ty::var(v1), Ty::bool()]);

        let mut subst = Substitution::new();
        subst.insert(v1, Ty::int(IntTy::I32));

        let result = ty.substitute(&subst);
        assert_eq!(format!("{}", result), "(i32, bool)");
    }

    #[test]
    fn test_type_scheme() {
        let v1 = TyVarId::fresh();
        let scheme = TypeScheme::poly(
            vec![v1],
            Ty::function(vec![Ty::var(v1)], Ty::var(v1)),
        );

        let ty1 = scheme.instantiate();
        let ty2 = scheme.instantiate();

        // Each instantiation should have different variables
        assert_ne!(ty1, ty2);
    }
}
