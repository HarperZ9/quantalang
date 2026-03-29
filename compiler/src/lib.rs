// ===============================================================================
// QUANTALANG COMPILER
// "The Language That Evolves"
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================
//
// QuantaLang is a multi-paradigm systems programming language featuring:
// - Strong static typing with gradual inference
// - Memory safety without garbage collection (ownership + borrowing)
// - First-class effects system
// - Embedded DSLs (SQL, Regex, Math, Finance, Graphics)
// - Universal compilation (x86-64, ARM64, WASM, SPIR-V, C, JS)
// - @ai decorator, neural types (@ai decorator, neural types)
// - Self-evolution through Axiom integration
//
// ===============================================================================

#![warn(rust_2018_idioms)]
#![allow(missing_docs)]
#![allow(dead_code)]
#![allow(ambiguous_glob_reexports)]
#![deny(unsafe_op_in_unsafe_fn)]

//! # QuantaLang Compiler
//!
//! This crate provides the complete QuantaLang compiler implementation including:
//! - Lexical analysis (tokenization)
//! - Parsing (recursive descent with Pratt parsing for expressions)
//! - Type checking (Hindley-Milner with extensions)
//! - Code generation (LLVM IR, WASM, SPIR-V)
//!
//! ## Example
//!
//! ```rust,ignore
//! use quantalang::{compile, CompileOptions, Target};
//!
//! let source = r#"
//!     fn main() {
//!         println("Hello, QuantaLang!")
//!     }
//! "#;
//!
//! let output = compile(source, Target::Native, CompileOptions::default())?;
//! ```

pub mod lexer;
pub mod ast;
pub mod parser;
pub mod types;
pub mod macro_expand;
pub mod codegen;
pub mod runtime;
pub mod lsp;
pub mod fmt;
pub mod pkg;

/// The compiler version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The language version
pub const LANGUAGE_VERSION: (u32, u32, u32) = (1, 0, 0);

/// Author information
pub const AUTHOR: &str = "Zain Dana Harper";

/// Copyright notice
pub const COPYRIGHT: &str = "Copyright (c) 2022-2026 Zain Dana Harper. MIT License.";

// Re-export commonly used types
pub use lexer::{Lexer, Token, TokenKind, Span, SourceFile};
pub use codegen::{CodeGenerator, Target, GeneratedCode, OutputFormat};
pub use runtime::{Executor, Task, TaskId, Poll, Future, Channel, Semaphore};
pub use lsp::{LanguageServer, DocumentStore, run_server};
pub use fmt::{Formatter, FormatConfig, FormatError};
pub use pkg::{Manifest, Version, VersionReq, Registry, Resolver, Lockfile};
