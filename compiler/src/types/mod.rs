// ===============================================================================
// QUANTALANG TYPE SYSTEM
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! # Type System
//!
//! This module implements QuantaLang's type system, featuring:
//! - Hindley-Milner type inference with extensions
//! - Bidirectional type checking
//! - Trait-based polymorphism
//! - Higher-kinded types (HKT)
//! - Algebraic effect system
//! - Const generics
//!
//! ## Architecture
//!
//! The type system consists of several components:
//! - `ty`: Core type representation
//! - `context`: Type environment and scoping
//! - `infer`: Type inference engine
//! - `unify`: Unification algorithm
//! - `check`: Type checking passes
//! - `traits`: Trait resolution
//! - `hkt`: Higher-kinded types and kind system
//! - `effects`: Algebraic effect system
//! - `const_generics`: Compile-time constant values as type parameters
//!
//! ## Example
//!
//! ```rust,ignore
//! use quantalang::types::{TypeChecker, TypeContext};
//! use quantalang::ast::Module;
//!
//! let mut ctx = TypeContext::new();
//! let mut checker = TypeChecker::new(&mut ctx);
//! checker.check_module(&module)?;
//! ```

mod ty;
mod context;
mod infer;
mod unify;
mod check;
mod error;
mod builtins;
mod traits;
mod hkt;
mod effects;
mod const_generics;

pub use ty::*;
pub use context::*;
pub use infer::*;
pub use unify::*;
pub use check::*;
pub use error::*;
pub use builtins::*;
pub use traits::*;
pub use hkt::*;
pub use effects::*;
pub use const_generics::*;
