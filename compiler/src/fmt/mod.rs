// ===============================================================================
// QUANTALANG CODE FORMATTER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Code formatter for QuantaLang.
//!
//! This module provides automatic code formatting following the QuantaLang style guide:
//! - Consistent indentation (4 spaces by default)
//! - Proper spacing around operators and keywords
//! - Line length management (100 chars default)
//! - Import organization
//! - Trailing comma normalization
//!
//! ## Usage
//!
//! ```rust,ignore
//! use quantalang::fmt::{Formatter, FormatConfig};
//!
//! let config = FormatConfig::default();
//! let formatter = Formatter::new(config);
//! let formatted = formatter.format_str(source)?;
//! ```

pub mod config;
pub mod formatter;
pub mod pretty;

pub use config::*;
pub use formatter::*;
pub use pretty::*;
