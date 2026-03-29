// ===============================================================================
// QUANTALANG RUNTIME SUPPORT
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Runtime support for QuantaLang programs.
//!
//! This module provides runtime infrastructure including:
//! - Foreign Function Interface (FFI) for C interop
//! - Garbage collection (reference counting + cycle detection)
//! - Async runtime with work-stealing scheduler
//!
//! ## FFI Example
//!
//! ```quanta
//! extern "C" {
//!     fn printf(format: *const i8, ...) -> i32;
//!     fn malloc(size: usize) -> *mut u8;
//!     fn free(ptr: *mut u8);
//! }
//!
//! @[link(name = "mylib")]
//! extern "C" {
//!     fn my_function(x: i32) -> i32;
//! }
//! ```
//!
//! ## Memory Management
//!
//! ```quanta
//! // Automatic reference counting
//! let obj = Box::new(MyStruct { value: 42 });
//!
//! // Manual memory via FFI
//! unsafe {
//!     let ptr = malloc(1024);
//!     // ... use memory ...
//!     free(ptr);
//! }
//! ```

pub mod async_rt;
pub mod ffi;
pub mod gc;

pub use async_rt::*;
pub use ffi::*;
pub use gc::*;
