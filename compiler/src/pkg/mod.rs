// ===============================================================================
// QUANTALANG PACKAGE MANAGER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Package manager for QuantaLang (quanta-pkg).
//!
//! This module provides package management functionality including:
//! - Package manifest parsing (Quanta.toml)
//! - Dependency resolution with semver
//! - Registry interaction
//! - Package building and publishing
//!
//! ## Manifest Format
//!
//! ```toml
//! [package]
//! name = "my-package"
//! version = "1.0.0"
//! authors = ["Author Name <email@example.com>"]
//! edition = "2025"
//!
//! [dependencies]
//! serde = "1.0"
//! tokio = { version = "1.0", features = ["full"] }
//!
//! [dev-dependencies]
//! criterion = "0.4"
//! ```

pub mod lockfile;
pub mod manifest;
pub mod registry;
pub mod resolver;
pub mod version;

pub use lockfile::*;
pub use manifest::*;
pub use registry::*;
pub use resolver::*;
pub use version::*;
