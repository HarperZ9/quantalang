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

mod builtins;
mod check;
mod const_generics;
mod context;
mod effects;
mod error;
mod hkt;
mod infer;
mod traits;
mod ty;
mod unify;

pub use builtins::*;
pub use check::*;
pub use const_generics::*;
pub use context::*;
pub use effects::*;
pub use error::*;
pub use hkt::*;
pub use infer::*;
pub use traits::*;
pub use ty::*;
pub use unify::*;
