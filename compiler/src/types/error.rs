// ===============================================================================
// QUANTALANG TYPE SYSTEM - ERRORS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Type system error types.

use std::fmt;
use thiserror::Error;

use crate::lexer::Span;
use super::ty::*;

/// Result type for type operations.
pub type TypeResult<T> = Result<T, TypeError>;

/// A type error with location information.
#[derive(Debug, Clone)]
pub struct TypeErrorWithSpan {
    /// The error.
    pub error: TypeError,
    /// The span where the error occurred.
    pub span: Span,
    /// Optional help message.
    pub help: Option<String>,
    /// Optional notes.
    pub notes: Vec<String>,
}

impl TypeErrorWithSpan {
    /// Create a new type error with span.
    pub fn new(error: TypeError, span: Span) -> Self {
        Self {
            error,
            span,
            help: None,
            notes: Vec::new(),
        }
    }

    /// Add a help message.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Add a note.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

impl fmt::Display for TypeErrorWithSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)?;
        if let Some(help) = &self.help {
            write!(f, "\n  help: {}", help)?;
        }
        for note in &self.notes {
            write!(f, "\n  note: {}", note)?;
        }
        Ok(())
    }
}

impl std::error::Error for TypeErrorWithSpan {}

/// Type errors.
#[derive(Debug, Clone, Error)]
pub enum TypeError {
    // =========================================================================
    // UNIFICATION ERRORS
    // =========================================================================

    /// Types do not match.
    #[error("type mismatch: expected `{expected}`, found `{found}`")]
    TypeMismatch {
        expected: Ty,
        found: Ty,
    },

    /// Infinite type (occurs check failure).
    #[error("infinite type: `{var}` occurs in `{ty}`")]
    InfiniteType {
        var: TyVarId,
        ty: Ty,
    },

    /// Mutability mismatch.
    #[error("mutability mismatch: expected `{expected:?}`, found `{found:?}`")]
    MutabilityMismatch {
        expected: Mutability,
        found: Mutability,
    },

    /// Array length mismatch.
    #[error("array length mismatch: expected `{expected}`, found `{found}`")]
    ArrayLengthMismatch {
        expected: usize,
        found: usize,
    },

    /// Arity mismatch (wrong number of arguments).
    #[error("expected {expected} arguments, found {found}")]
    ArityMismatch {
        expected: usize,
        found: usize,
    },

    /// Unsafety mismatch.
    #[error("unsafety mismatch")]
    UnsafetyMismatch,

    /// Lifetime mismatch.
    #[error("lifetime mismatch: expected `{expected}`, found `{found}`")]
    LifetimeMismatch {
        expected: super::ty::Lifetime,
        found: super::ty::Lifetime,
    },

    /// ABI mismatch.
    #[error("ABI mismatch: expected `{expected}`, found `{found}`")]
    AbiMismatch {
        expected: std::sync::Arc<str>,
        found: std::sync::Arc<str>,
    },

    // =========================================================================
    // LOOKUP ERRORS
    // =========================================================================

    /// Undefined variable.
    #[error("undefined variable: `{name}`")]
    UndefinedVariable {
        name: String,
    },

    /// Undefined type.
    #[error("undefined type: `{name}`")]
    UndefinedType {
        name: String,
    },

    /// Undefined function.
    #[error("undefined function: `{name}`")]
    UndefinedFunction {
        name: String,
    },

    /// Undefined field.
    #[error("type `{ty}` has no field `{field}`")]
    UndefinedField {
        ty: Ty,
        field: String,
    },

    /// Undefined method.
    #[error("type `{ty}` has no method `{method}`")]
    UndefinedMethod {
        ty: Ty,
        method: String,
    },

    /// Undefined variant.
    #[error("enum `{enum_name}` has no variant `{variant}`")]
    UndefinedVariant {
        enum_name: String,
        variant: String,
    },

    // =========================================================================
    // EXPRESSION ERRORS
    // =========================================================================

    /// Cannot call non-function type.
    #[error("type `{ty}` is not callable")]
    NotCallable {
        ty: Ty,
    },

    /// Cannot index non-array type.
    #[error("type `{ty}` cannot be indexed")]
    NotIndexable {
        ty: Ty,
    },

    /// Cannot dereference non-pointer type.
    #[error("type `{ty}` cannot be dereferenced")]
    NotDereferenceable {
        ty: Ty,
    },

    /// Cannot borrow variable as mutable while it is already borrowed.
    #[error("cannot borrow `{variable}` as mutable because it is already borrowed")]
    AlreadyBorrowed {
        variable: String,
    },

    /// Cannot borrow variable as mutable more than once at a time.
    #[error("cannot borrow `{variable}` as mutable more than once at a time")]
    DoubleMutableBorrow {
        variable: String,
    },

    /// Cannot use a moved value.
    #[error("use of moved value: `{variable}`")]
    UseAfterMove {
        variable: String,
    },

    /// Reference to local variable escapes function scope.
    #[error("cannot return reference to local variable `{variable}`")]
    ReferenceEscapesScope {
        variable: String,
    },

    /// Invalid binary operation.
    #[error("cannot apply binary operator `{op}` to types `{left}` and `{right}`")]
    InvalidBinaryOp {
        op: String,
        left: Ty,
        right: Ty,
    },

    /// Invalid unary operation.
    #[error("cannot apply unary operator `{op}` to type `{ty}`")]
    InvalidUnaryOp {
        op: String,
        ty: Ty,
    },

    /// Invalid assignment target.
    #[error("invalid assignment target")]
    InvalidAssignTarget,

    /// Cannot assign to immutable binding.
    #[error("cannot assign to immutable variable `{name}`")]
    ImmutableAssignment {
        name: String,
    },

    // =========================================================================
    // PATTERN ERRORS
    // =========================================================================

    /// Pattern type mismatch.
    #[error("pattern type mismatch: expected `{expected}`, found `{found}`")]
    PatternMismatch {
        expected: Ty,
        found: Ty,
    },

    /// Refutable pattern in irrefutable position.
    #[error("refutable pattern in irrefutable position")]
    RefutablePattern,

    /// Non-exhaustive patterns.
    #[error("non-exhaustive patterns")]
    NonExhaustivePatterns,

    /// Non-exhaustive match (missing specific enum variants).
    #[error("non-exhaustive match: missing variants {}", missing_variants.join(", "))]
    NonExhaustiveMatch {
        missing_variants: Vec<String>,
    },

    // =========================================================================
    // CONTROL FLOW ERRORS
    // =========================================================================

    /// Break outside of loop.
    #[error("`break` outside of loop")]
    BreakOutsideLoop,

    /// Continue outside of loop.
    #[error("`continue` outside of loop")]
    ContinueOutsideLoop,

    /// Return outside of function.
    #[error("`return` outside of function")]
    ReturnOutsideFunction,

    /// Missing return type.
    #[error("function returns `{found}` but expected `{expected}`")]
    ReturnTypeMismatch {
        expected: Ty,
        found: Ty,
    },

    // =========================================================================
    // TRAIT ERRORS
    // =========================================================================

    /// Trait not implemented.
    #[error("the trait bound `{ty}: {trait_id:?}` is not satisfied")]
    TraitNotImplemented {
        ty: Ty,
        trait_id: super::traits::TraitId,
    },

    /// Ambiguous trait resolution.
    #[error("multiple implementations of trait `{trait_name}` for type `{ty}`")]
    AmbiguousImpl {
        trait_name: String,
        ty: Ty,
    },

    /// Associated type not found.
    #[error("associated type `{name}` not found in trait `{trait_name}`")]
    AssocTypeNotFound {
        name: String,
        trait_name: String,
    },

    /// Associated type not defined in impl.
    #[error("associated type `{assoc_name}` not defined")]
    AssociatedTypeNotDefined {
        assoc_name: String,
    },

    /// Internal error.
    #[error("internal error: {0}")]
    InternalError(String),

    // =========================================================================
    // GENERICS ERRORS
    // =========================================================================

    /// Wrong number of type arguments.
    #[error("expected {expected} type arguments, found {found}")]
    WrongTypeArgCount {
        expected: usize,
        found: usize,
    },

    /// Bound not satisfied.
    #[error("type `{ty}` does not satisfy bound `{bound}`")]
    BoundNotSatisfied {
        ty: Ty,
        bound: String,
    },

    // =========================================================================
    // OTHER ERRORS
    // =========================================================================

    /// Duplicate definition.
    #[error("duplicate definition: `{name}`")]
    DuplicateDefinition {
        name: String,
    },

    /// Type annotation required.
    #[error("type annotations needed")]
    TypeAnnotationNeeded,

    /// Unsafe operation outside unsafe block.
    #[error("unsafe operation outside of `unsafe` block")]
    UnsafeOutsideUnsafe,

    /// Internal error.
    #[error("internal type error: {0}")]
    Internal(String),

    // =========================================================================
    // EFFECT ERRORS
    // =========================================================================

    /// Unknown effect: the effect has not been declared.
    #[error("unknown effect `{name}`")]
    UnknownEffect {
        name: String,
    },

    /// Unhandled effect: a function performs an effect but does not declare it.
    #[error("function `{func_name}` performs effect `{effect_name}` but does not declare it")]
    UnhandledEffect {
        func_name: String,
        effect_name: String,
    },

    /// Undeclared effect: a function body performs an effect not in the signature.
    #[error("function performs undeclared effect `{effect_name}`")]
    UndeclaredEffect {
        func_name: String,
        effect_name: String,
        declared_effects: Vec<String>,
    },

    /// Unknown effect operation.
    #[error("unknown operation `{operation}` in effect `{effect_name}`")]
    UnknownEffectOperation {
        effect_name: String,
        operation: String,
    },

    /// Missing handler clause: a handle block does not cover all operations.
    #[error("handler for effect `{effect_name}` is missing operation `{operation}`")]
    MissingHandlerClause {
        effect_name: String,
        operation: String,
    },
}

impl TypeError {
    /// Check if this is a fatal error that should stop type checking.
    pub fn is_fatal(&self) -> bool {
        matches!(self, TypeError::Internal(_))
    }

    /// Get a suggested fix for this error.
    pub fn suggestion(&self) -> Option<String> {
        match self {
            TypeError::TypeMismatch { expected, .. } => {
                Some(format!("consider adding a type annotation: `: {}`", expected))
            }
            TypeError::ImmutableAssignment { name } => {
                Some(format!("consider making `{}` mutable: `let mut {}`", name, name))
            }
            TypeError::TypeAnnotationNeeded => {
                Some("consider adding a type annotation".to_string())
            }
            TypeError::UnsafeOutsideUnsafe => {
                Some("consider wrapping in an `unsafe` block".to_string())
            }
            TypeError::UnknownEffect { name } => {
                Some(format!(
                    "define the effect:\n  effect {} {{\n      fn operation_name(params) -> ReturnType,\n  }}",
                    name
                ))
            }
            TypeError::UnhandledEffect { func_name, effect_name } => {
                Some(format!(
                    "either add `~ {}` to the function signature:\n  fn {}() ~ {} {{ ... }}\n\nor handle the effect with a handler:\n  handle {{ ... }} with {{\n      {}.operation(args) => |resume| {{\n          // handle the operation\n          resume(())\n      }},\n  }}",
                    effect_name, func_name, effect_name, effect_name
                ))
            }
            TypeError::UndeclaredEffect { func_name: _, effect_name, declared_effects } => {
                let declared = declared_effects.join(", ");
                Some(format!(
                    "add `{}` to the effect annotations: ~ {}, {}",
                    effect_name, declared, effect_name
                ))
            }
            TypeError::UnknownEffectOperation { effect_name, operation } => {
                Some(format!(
                    "effect `{}` does not define operation `{}`; check available operations",
                    effect_name, operation
                ))
            }
            TypeError::MissingHandlerClause { effect_name, operation } => {
                Some(format!(
                    "add a handler clause for `{}`:\n  {}.{}(params) => |resume| {{\n      // handle the {} operation\n      resume(())\n  }},",
                    operation, effect_name, operation, operation
                ))
            }
            TypeError::NonExhaustiveMatch { missing_variants } => {
                Some(format!(
                    "add arms for the missing variants: {}, or add a wildcard `_` arm",
                    missing_variants.join(", ")
                ))
            }
            _ => None,
        }
    }
}
