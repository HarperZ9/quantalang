// ===============================================================================
// QUANTALANG TYPE SYSTEM - EFFECT SYSTEM
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Effect system for tracking and controlling side effects.
//!
//! QuantaLang uses an algebraic effect system to track:
//! - I/O operations
//! - Exceptions/errors
//! - State mutations
//! - Async/await
//! - Non-determinism
//! - Custom effects
//!
//! ## Syntax
//!
//! ```quanta
//! // Function with effects
//! fn read_file(path: str) -> String with IO, Error {
//!     // ...
//! }
//!
//! // Effect handlers
//! handle {
//!     read_file("data.txt")
//! } with {
//!     IO::read(fd) => resume(mock_data),
//!     Error::throw(e) => default_value,
//! }
//!
//! // Effect polymorphism
//! fn map<A, B, E>(xs: List<A>, f: fn(A) -> B with E) -> List<B> with E {
//!     // ...
//! }
//! ```
//!
//! ## Effect Rows
//!
//! Effects are tracked as rows (sets) that can be:
//! - Empty: `{}` (pure)
//! - Concrete: `{IO, Error}`
//! - Polymorphic: `{IO | E}` (IO plus unknown effects E)

use std::collections::{HashSet, HashMap};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use super::{Ty, DefId};

/// A unique identifier for effect variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EffectVarId(pub u32);

impl EffectVarId {
    /// Create a fresh effect variable ID.
    pub fn fresh() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl fmt::Display for EffectVarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "?E{}", self.0)
    }
}

/// A single effect.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Effect {
    /// The effect name.
    pub name: Arc<str>,
    /// Type parameters for the effect.
    pub params: Vec<Ty>,
}

impl Effect {
    /// Create a new effect.
    pub fn new(name: impl Into<Arc<str>>) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
        }
    }

    /// Create an effect with type parameters.
    pub fn with_params(name: impl Into<Arc<str>>, params: Vec<Ty>) -> Self {
        Self {
            name: name.into(),
            params,
        }
    }

    /// The IO effect.
    pub fn io() -> Self {
        Self::new("IO")
    }

    /// The Error effect with error type.
    pub fn error(err_ty: Ty) -> Self {
        Self::with_params("Error", vec![err_ty])
    }

    /// The Async effect.
    pub fn async_effect() -> Self {
        Self::new("Async")
    }

    /// The State effect with state type.
    pub fn state(state_ty: Ty) -> Self {
        Self::with_params("State", vec![state_ty])
    }

    /// The NonDet effect (non-determinism).
    pub fn nondet() -> Self {
        Self::new("NonDet")
    }

    /// The Pure effect (no effects).
    pub fn pure() -> Self {
        Self::new("Pure")
    }

    /// Check if this is the IO effect.
    pub fn is_io(&self) -> bool {
        self.name.as_ref() == "IO"
    }

    /// Check if this is the Error effect.
    pub fn is_error(&self) -> bool {
        self.name.as_ref() == "Error"
    }

    /// Check if this is the Async effect.
    pub fn is_async(&self) -> bool {
        self.name.as_ref() == "Async"
    }
}

impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if !self.params.is_empty() {
            write!(f, "<")?;
            for (i, param) in self.params.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", param)?;
            }
            write!(f, ">")?;
        }
        Ok(())
    }
}

/// An effect row - a set of effects with optional tail variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectRow {
    /// Concrete effects in this row.
    pub effects: HashSet<Effect>,
    /// Optional tail variable for open rows.
    pub tail: Option<EffectVarId>,
}

impl Hash for EffectRow {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Sort effects by name for deterministic hashing
        let mut sorted: Vec<_> = self.effects.iter().collect();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        for eff in &sorted {
            eff.hash(state);
        }
        self.tail.hash(state);
    }
}

impl EffectRow {
    /// Create an empty (pure) effect row.
    pub fn empty() -> Self {
        Self {
            effects: HashSet::new(),
            tail: None,
        }
    }

    /// Create a closed row with specific effects.
    pub fn closed(effects: impl IntoIterator<Item = Effect>) -> Self {
        Self {
            effects: effects.into_iter().collect(),
            tail: None,
        }
    }

    /// Create an open row with a tail variable.
    pub fn open(effects: impl IntoIterator<Item = Effect>, tail: EffectVarId) -> Self {
        Self {
            effects: effects.into_iter().collect(),
            tail: Some(tail),
        }
    }

    /// Create a row with just a variable (fully polymorphic).
    pub fn var(var: EffectVarId) -> Self {
        Self {
            effects: HashSet::new(),
            tail: Some(var),
        }
    }

    /// Create a fresh effect variable row.
    pub fn fresh_var() -> Self {
        Self::var(EffectVarId::fresh())
    }

    /// Check if this row is empty (pure).
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty() && self.tail.is_none()
    }

    /// Check if this row is closed.
    pub fn is_closed(&self) -> bool {
        self.tail.is_none()
    }

    /// Check if this row is open (has a tail variable).
    pub fn is_open(&self) -> bool {
        self.tail.is_some()
    }

    /// Check if this row contains a specific effect.
    pub fn contains(&self, effect: &Effect) -> bool {
        self.effects.contains(effect)
    }

    /// Check if this row contains IO.
    pub fn has_io(&self) -> bool {
        self.effects.iter().any(|e| e.is_io())
    }

    /// Check if this row contains Error.
    pub fn has_error(&self) -> bool {
        self.effects.iter().any(|e| e.is_error())
    }

    /// Check if this row contains Async.
    pub fn has_async(&self) -> bool {
        self.effects.iter().any(|e| e.is_async())
    }

    /// Add an effect to this row.
    pub fn add(&mut self, effect: Effect) {
        self.effects.insert(effect);
    }

    /// Remove an effect from this row.
    pub fn remove(&mut self, effect: &Effect) -> bool {
        self.effects.remove(effect)
    }

    /// Merge two rows.
    pub fn merge(&self, other: &EffectRow) -> EffectRow {
        let effects: HashSet<_> = self.effects.union(&other.effects).cloned().collect();
        let tail = match (self.tail, other.tail) {
            (Some(v1), Some(v2)) if v1 == v2 => Some(v1),
            (Some(v), None) | (None, Some(v)) => Some(v),
            (Some(_), Some(_)) => Some(EffectVarId::fresh()), // Different vars, need fresh
            (None, None) => None,
        };

        EffectRow { effects, tail }
    }

    /// Substitute effect variables.
    pub fn substitute(&self, subst: &EffectSubstitution) -> EffectRow {
        let mut result = EffectRow {
            effects: self.effects.clone(),
            tail: None,
        };

        if let Some(var) = self.tail {
            if let Some(row) = subst.get(var) {
                // Merge with substituted row
                result.effects.extend(row.effects.clone());
                result.tail = row.tail;
            } else {
                result.tail = Some(var);
            }
        }

        result
    }
}

impl fmt::Display for EffectRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return write!(f, "Pure");
        }

        let effects: Vec<_> = self.effects.iter().map(|e| format!("{}", e)).collect();

        if let Some(tail) = self.tail {
            if effects.is_empty() {
                write!(f, "{}", tail)
            } else {
                write!(f, "{} | {}", effects.join(", "), tail)
            }
        } else {
            write!(f, "{}", effects.join(", "))
        }
    }
}

/// A substitution for effect variables.
#[derive(Debug, Clone, Default)]
pub struct EffectSubstitution {
    map: HashMap<EffectVarId, EffectRow>,
}

impl EffectSubstitution {
    /// Create an empty substitution.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a mapping.
    pub fn insert(&mut self, var: EffectVarId, row: EffectRow) {
        self.map.insert(var, row);
    }

    /// Get the row for a variable.
    pub fn get(&self, var: EffectVarId) -> Option<&EffectRow> {
        self.map.get(&var)
    }

    /// Check if a variable is bound.
    pub fn contains(&self, var: EffectVarId) -> bool {
        self.map.contains_key(&var)
    }

    /// Compose two substitutions.
    pub fn compose(&self, other: &EffectSubstitution) -> EffectSubstitution {
        let mut result = EffectSubstitution::new();

        for (var, row) in &other.map {
            result.insert(*var, row.substitute(self));
        }

        for (var, row) in &self.map {
            if !result.contains(*var) {
                result.insert(*var, row.clone());
            }
        }

        result
    }
}

/// An effect definition.
#[derive(Debug, Clone)]
pub struct EffectDef {
    /// Definition ID.
    pub def_id: DefId,
    /// Effect name.
    pub name: Arc<str>,
    /// Type parameters.
    pub type_params: Vec<Arc<str>>,
    /// Operations defined by this effect.
    pub operations: Vec<EffectOperation>,
}

impl EffectDef {
    /// Create a new effect definition.
    pub fn new(def_id: DefId, name: impl Into<Arc<str>>) -> Self {
        Self {
            def_id,
            name: name.into(),
            type_params: Vec::new(),
            operations: Vec::new(),
        }
    }

    /// Add a type parameter.
    pub fn with_type_param(mut self, name: impl Into<Arc<str>>) -> Self {
        self.type_params.push(name.into());
        self
    }

    /// Add an operation.
    pub fn with_operation(mut self, op: EffectOperation) -> Self {
        self.operations.push(op);
        self
    }
}

/// An operation within an effect.
#[derive(Debug, Clone)]
pub struct EffectOperation {
    /// Operation name.
    pub name: Arc<str>,
    /// Parameter types.
    pub params: Vec<Ty>,
    /// Return type.
    pub return_ty: Ty,
}

impl EffectOperation {
    /// Create a new operation.
    pub fn new(name: impl Into<Arc<str>>, params: Vec<Ty>, return_ty: Ty) -> Self {
        Self {
            name: name.into(),
            params,
            return_ty,
        }
    }
}

/// An effect handler.
#[derive(Debug, Clone)]
pub struct EffectHandler {
    /// The effect being handled.
    pub effect: Effect,
    /// Handler clauses for each operation.
    pub clauses: Vec<HandlerClause>,
    /// Return clause type (input type -> output type).
    pub return_clause: Option<(Ty, Ty)>,
}

/// A clause in an effect handler.
#[derive(Debug, Clone)]
pub struct HandlerClause {
    /// Operation name.
    pub operation: Arc<str>,
    /// Parameter names.
    pub param_names: Vec<Arc<str>>,
    /// Whether this clause resumes.
    pub resumes: bool,
}

impl HandlerClause {
    /// Create a new handler clause.
    pub fn new(operation: impl Into<Arc<str>>, param_names: Vec<Arc<str>>) -> Self {
        Self {
            operation: operation.into(),
            param_names,
            resumes: true,
        }
    }

    /// Set whether this clause resumes.
    pub fn with_resume(mut self, resumes: bool) -> Self {
        self.resumes = resumes;
        self
    }
}

/// Effectful function type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectfulFn {
    /// Parameter types.
    pub params: Vec<Ty>,
    /// Return type.
    pub return_ty: Ty,
    /// Effect row.
    pub effects: EffectRow,
}

impl EffectfulFn {
    /// Create a new effectful function type.
    pub fn new(params: Vec<Ty>, return_ty: Ty, effects: EffectRow) -> Self {
        Self {
            params,
            return_ty,
            effects,
        }
    }

    /// Create a pure function (no effects).
    pub fn pure(params: Vec<Ty>, return_ty: Ty) -> Self {
        Self::new(params, return_ty, EffectRow::empty())
    }

    /// Check if this function is pure.
    pub fn is_pure(&self) -> bool {
        self.effects.is_empty()
    }
}

impl fmt::Display for EffectfulFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fn(")?;
        for (i, param) in self.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", param)?;
        }
        write!(f, ") -> {}", self.return_ty)?;

        if !self.effects.is_empty() {
            write!(f, " with {}", self.effects)?;
        }

        Ok(())
    }
}

/// Effect inference context.
#[derive(Debug, Default)]
pub struct EffectContext {
    /// Effect definitions.
    effects: HashMap<Arc<str>, EffectDef>,
    /// Current effect substitution.
    subst: EffectSubstitution,
    /// Effect constraints.
    constraints: Vec<EffectConstraint>,
}

impl EffectContext {
    /// Create a new effect context.
    pub fn new() -> Self {
        let mut ctx = Self::default();
        ctx.register_builtin_effects();
        ctx
    }

    /// Register built-in effects.
    fn register_builtin_effects(&mut self) {
        // IO effect
        let io = EffectDef::new(DefId::new(0, 0), "IO")
            .with_operation(EffectOperation::new(
                "print",
                vec![Ty::str()],
                Ty::unit(),
            ))
            .with_operation(EffectOperation::new(
                "read_line",
                vec![],
                Ty::str(),
            ));
        self.register_effect(io);

        // Error effect
        let error = EffectDef::new(DefId::new(0, 1), "Error")
            .with_type_param("E")
            .with_operation(EffectOperation::new(
                "throw",
                vec![Ty::param("E", 0)],
                Ty::never(),
            ))
            .with_operation(EffectOperation::new(
                "catch",
                vec![],
                Ty::param("E", 0),
            ));
        self.register_effect(error);

        // Async effect
        let async_eff = EffectDef::new(DefId::new(0, 2), "Async")
            .with_operation(EffectOperation::new(
                "await",
                vec![Ty::param("T", 0)],
                Ty::param("T", 0),
            ))
            .with_operation(EffectOperation::new(
                "spawn",
                vec![],
                Ty::unit(),
            ));
        self.register_effect(async_eff);

        // State effect
        let state = EffectDef::new(DefId::new(0, 3), "State")
            .with_type_param("S")
            .with_operation(EffectOperation::new(
                "get",
                vec![],
                Ty::param("S", 0),
            ))
            .with_operation(EffectOperation::new(
                "put",
                vec![Ty::param("S", 0)],
                Ty::unit(),
            ))
            .with_operation(EffectOperation::new(
                "modify",
                vec![Ty::function(vec![Ty::param("S", 0)], Ty::param("S", 0))],
                Ty::unit(),
            ));
        self.register_effect(state);

        // NonDet effect
        let nondet = EffectDef::new(DefId::new(0, 4), "NonDet")
            .with_operation(EffectOperation::new(
                "choose",
                vec![Ty::param("T", 0), Ty::param("T", 0)],
                Ty::param("T", 0),
            ))
            .with_operation(EffectOperation::new(
                "fail",
                vec![],
                Ty::never(),
            ));
        self.register_effect(nondet);
    }

    /// Register an effect definition.
    pub fn register_effect(&mut self, effect: EffectDef) {
        self.effects.insert(effect.name.clone(), effect);
    }

    /// Get an effect definition.
    pub fn get_effect(&self, name: &str) -> Option<&EffectDef> {
        self.effects.get(name)
    }

    /// Get all registered effect definitions.
    pub fn all_effects(&self) -> Vec<&EffectDef> {
        self.effects.values().collect()
    }

    /// Add an effect constraint.
    pub fn add_constraint(&mut self, constraint: EffectConstraint) {
        self.constraints.push(constraint);
    }

    /// Unify two effect rows.
    pub fn unify_rows(&mut self, r1: &EffectRow, r2: &EffectRow) -> Result<(), EffectError> {
        let r1 = r1.substitute(&self.subst);
        let r2 = r2.substitute(&self.subst);

        match (r1.tail, r2.tail) {
            // Both closed
            (None, None) => {
                if r1.effects == r2.effects {
                    Ok(())
                } else {
                    Err(EffectError::Mismatch {
                        expected: r1.clone(),
                        found: r2.clone(),
                    })
                }
            }

            // One open, one closed
            (Some(v), None) => {
                // Check that r2 contains all effects from r1
                if r1.effects.is_subset(&r2.effects) {
                    let diff: HashSet<_> = r2.effects.difference(&r1.effects).cloned().collect();
                    self.subst.insert(v, EffectRow::closed(diff));
                    Ok(())
                } else {
                    Err(EffectError::Mismatch {
                        expected: r1.clone(),
                        found: r2.clone(),
                    })
                }
            }

            (None, Some(v)) => {
                // Check that r1 contains all effects from r2
                if r2.effects.is_subset(&r1.effects) {
                    let diff: HashSet<_> = r1.effects.difference(&r2.effects).cloned().collect();
                    self.subst.insert(v, EffectRow::closed(diff));
                    Ok(())
                } else {
                    Err(EffectError::Mismatch {
                        expected: r1.clone(),
                        found: r2.clone(),
                    })
                }
            }

            // Both open
            (Some(v1), Some(v2)) if v1 == v2 => {
                // Same variable, check concrete effects match
                if r1.effects == r2.effects {
                    Ok(())
                } else {
                    Err(EffectError::Mismatch {
                        expected: r1.clone(),
                        found: r2.clone(),
                    })
                }
            }

            (Some(v1), Some(v2)) => {
                // Different variables, create fresh variable for the join
                let fresh = EffectVarId::fresh();
                let union: HashSet<_> = r1.effects.union(&r2.effects).cloned().collect();

                // v1 = r1.effects + fresh
                let r1_diff: HashSet<_> = union.difference(&r1.effects).cloned().collect();
                self.subst.insert(v1, EffectRow::open(r1_diff, fresh));

                // v2 = r2.effects + fresh
                let r2_diff: HashSet<_> = union.difference(&r2.effects).cloned().collect();
                self.subst.insert(v2, EffectRow::open(r2_diff, fresh));

                Ok(())
            }
        }
    }

    /// Check if effects are subsumed (r1 ⊆ r2).
    pub fn subsumes(&self, r1: &EffectRow, r2: &EffectRow) -> bool {
        let r1 = r1.substitute(&self.subst);
        let r2 = r2.substitute(&self.subst);

        // All effects in r1 must be in r2
        if !r1.effects.is_subset(&r2.effects) {
            return false;
        }

        // If r1 is open, r2 must also be open (or contain all possible effects)
        match (r1.tail, r2.tail) {
            (None, _) => true,
            (Some(_), None) => false, // r1 is open but r2 is closed
            (Some(v1), Some(v2)) => v1 == v2, // Same tail variable
        }
    }

    /// Apply current substitution to a row.
    pub fn apply_subst(&self, row: &EffectRow) -> EffectRow {
        row.substitute(&self.subst)
    }
}

/// An effect constraint.
#[derive(Debug, Clone)]
pub enum EffectConstraint {
    /// r1 = r2
    Equal(EffectRow, EffectRow),
    /// r1 ⊆ r2
    Subsumes(EffectRow, EffectRow),
    /// Effect must be handled
    MustHandle(Effect),
}

/// Effect errors.
#[derive(Debug, Clone)]
pub enum EffectError {
    /// Effect row mismatch.
    Mismatch { expected: EffectRow, found: EffectRow },

    /// Unhandled effect.
    Unhandled(Effect),

    /// Unknown effect.
    UnknownEffect(String),

    /// Unknown operation.
    UnknownOperation { effect: String, operation: String },

    /// Invalid handler.
    InvalidHandler(String),
}

impl fmt::Display for EffectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EffectError::Mismatch { expected, found } => {
                write!(f, "effect mismatch: expected {{{}}}, found {{{}}}", expected, found)
            }
            EffectError::Unhandled(effect) => {
                write!(f, "unhandled effect: {}", effect)
            }
            EffectError::UnknownEffect(name) => {
                write!(f, "unknown effect: {}", name)
            }
            EffectError::UnknownOperation { effect, operation } => {
                write!(f, "unknown operation '{}' in effect '{}'", operation, effect)
            }
            EffectError::InvalidHandler(msg) => {
                write!(f, "invalid handler: {}", msg)
            }
        }
    }
}

impl std::error::Error for EffectError {}

/// Built-in effect definitions.
pub fn builtin_effects() -> Vec<EffectDef> {
    let ctx = EffectContext::new();
    ctx.effects.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effect_display() {
        assert_eq!(format!("{}", Effect::io()), "IO");
        assert_eq!(format!("{}", Effect::async_effect()), "Async");

        let error = Effect::error(Ty::str());
        assert_eq!(format!("{}", error), "Error<str>");
    }

    #[test]
    fn test_effect_row() {
        let empty = EffectRow::empty();
        assert!(empty.is_empty());
        assert!(empty.is_closed());
        assert_eq!(format!("{}", empty), "Pure");

        let io_row = EffectRow::closed(vec![Effect::io()]);
        assert!(!io_row.is_empty());
        assert!(io_row.has_io());
        assert_eq!(format!("{}", io_row), "IO");

        let multi = EffectRow::closed(vec![Effect::io(), Effect::async_effect()]);
        assert!(multi.has_io());
        assert!(multi.has_async());
    }

    #[test]
    fn test_effect_row_merge() {
        let r1 = EffectRow::closed(vec![Effect::io()]);
        let r2 = EffectRow::closed(vec![Effect::async_effect()]);

        let merged = r1.merge(&r2);
        assert!(merged.has_io());
        assert!(merged.has_async());
        assert!(merged.is_closed());
    }

    #[test]
    fn test_open_effect_row() {
        let var = EffectVarId::fresh();
        let open = EffectRow::open(vec![Effect::io()], var);

        assert!(open.is_open());
        assert!(open.has_io());
    }

    #[test]
    fn test_effect_unification() {
        let mut ctx = EffectContext::new();

        // Unify two identical closed rows
        let r1 = EffectRow::closed(vec![Effect::io()]);
        let r2 = EffectRow::closed(vec![Effect::io()]);
        assert!(ctx.unify_rows(&r1, &r2).is_ok());

        // Unify different closed rows should fail
        let mut ctx2 = EffectContext::new();
        let r3 = EffectRow::closed(vec![Effect::io()]);
        let r4 = EffectRow::closed(vec![Effect::async_effect()]);
        assert!(ctx2.unify_rows(&r3, &r4).is_err());
    }

    #[test]
    fn test_effectful_fn() {
        let pure_fn = EffectfulFn::pure(
            vec![Ty::int(super::super::IntTy::I32)],
            Ty::bool(),
        );
        assert!(pure_fn.is_pure());

        let io_fn = EffectfulFn::new(
            vec![Ty::str()],
            Ty::unit(),
            EffectRow::closed(vec![Effect::io()]),
        );
        assert!(!io_fn.is_pure());
        assert!(io_fn.effects.has_io());
    }
}
