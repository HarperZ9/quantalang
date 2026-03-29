// ===============================================================================
// QUANTALANG CODE GENERATOR - BACKENDS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Code generation backends.
//!
//! Each backend takes MIR and produces target-specific output.

pub mod arm64;
pub mod arm64_enc;
pub mod c;
pub mod glsl;
pub mod hlsl;
pub mod llvm;
pub mod spirv;
pub mod wasm;
pub mod x86_64;
pub mod x86_64_enc;

use std::fmt;
use thiserror::Error;

use super::ir::MirModule;
use super::GeneratedCode;

/// Target platform for code generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// C transpilation (portable).
    C,
    /// x86-64 native code.
    X86_64,
    /// ARM64/AArch64 native code.
    Arm64,
    /// WebAssembly.
    Wasm,
    /// SPIR-V for GPU.
    SpirV,
    /// LLVM IR (for optimization and multi-target).
    LlvmIr,
    /// HLSL for DirectX / ReShade.
    Hlsl,
    /// GLSL for OpenGL / Vulkan shader source.
    Glsl,
}

impl Target {
    /// Get the default file extension for this target.
    pub fn extension(&self) -> &'static str {
        match self {
            Target::C => "c",
            Target::X86_64 | Target::Arm64 => "o",
            Target::Wasm => "wasm",
            Target::SpirV => "spv",
            Target::LlvmIr => "ll",
            Target::Hlsl => "hlsl",
            Target::Glsl => "glsl",
        }
    }

    /// Get the pointer size in bits for this target.
    pub fn pointer_size(&self) -> u32 {
        match self {
            Target::C => 64, // Assume 64-bit
            Target::X86_64 => 64,
            Target::Arm64 => 64,
            Target::Wasm => 32, // wasm32
            Target::SpirV => 64,
            Target::LlvmIr => 64,              // Default to 64-bit
            Target::Hlsl | Target::Glsl => 32, // GPU
        }
    }

    /// Check if this is a native code target.
    pub fn is_native(&self) -> bool {
        matches!(self, Target::X86_64 | Target::Arm64)
    }

    /// Check if this is an intermediate representation target.
    pub fn is_ir(&self) -> bool {
        matches!(self, Target::LlvmIr)
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::C => write!(f, "c"),
            Target::X86_64 => write!(f, "x86_64"),
            Target::Arm64 => write!(f, "arm64"),
            Target::Wasm => write!(f, "wasm"),
            Target::SpirV => write!(f, "spirv"),
            Target::LlvmIr => write!(f, "llvm-ir"),
            Target::Hlsl => write!(f, "hlsl"),
            Target::Glsl => write!(f, "glsl"),
        }
    }
}

/// Code generation error.
#[derive(Debug, Clone, Error)]
pub enum CodegenError {
    /// Internal error.
    #[error("internal error: {0}")]
    Internal(String),

    /// Unsupported feature.
    #[error("unsupported feature: {0}")]
    Unsupported(String),

    /// Invalid MIR.
    #[error("invalid MIR: {0}")]
    InvalidMir(String),

    /// Type error.
    #[error("type error: {0}")]
    TypeError(String),

    /// Missing function.
    #[error("missing function: {0}")]
    MissingFunction(String),

    /// Missing type.
    #[error("missing type: {0}")]
    MissingType(String),
}

/// Result type for code generation.
pub type CodegenResult<T> = Result<T, CodegenError>;

/// Trait for code generation backends.
pub trait Backend {
    /// Generate code from MIR.
    fn generate(&mut self, mir: &MirModule) -> CodegenResult<GeneratedCode>;

    /// Get the target for this backend.
    fn target(&self) -> Target;
}
