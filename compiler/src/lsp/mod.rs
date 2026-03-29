// ===============================================================================
// QUANTALANG LANGUAGE SERVER PROTOCOL
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Language Server Protocol implementation for QuantaLang.
//!
//! This module provides full LSP support including:
//! - Text document synchronization
//! - Code completion
//! - Hover information
//! - Go to definition
//! - Find references
//! - Document symbols
//! - Diagnostics
//! - Code actions
//! - Formatting
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                         LSP Server                               │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐             │
//! │  │   Message   │  │  Document   │  │   Symbol    │             │
//! │  │  Transport  │  │   Store     │  │   Index     │             │
//! │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘             │
//! │         │                │                │                     │
//! │         ▼                ▼                ▼                     │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │                    Request Handler                       │   │
//! │  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐   │   │
//! │  │  │Completion│ │  Hover   │ │ GoToDef  │ │ Diagnose │   │   │
//! │  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘   │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

pub mod actions;
pub mod completion;
pub mod definition;
pub mod diagnostics;
pub mod document;
pub mod hover;
pub mod message;
pub mod server;
pub mod symbols;
pub mod transport;
pub mod types;

pub use document::*;
pub use message::*;
pub use server::*;
pub use transport::*;
pub use types::*;
