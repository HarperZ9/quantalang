// ===============================================================================
// QUANTALANG CODE GENERATOR
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! # Code Generation
//!
//! This module implements code generation for QuantaLang, transforming
//! type-checked AST into executable code through multiple backends.
//!
//! ## Architecture
//!
//! The code generator uses a multi-stage lowering approach:
//!
//! ```text
//! AST -> MIR (Mid-level IR) -> Backend-specific output
//! ```
//!
//! ## Unwrap Policy
//!
//! Code generation operates on ASTs that have already been parsed, resolved,
//! and type-checked. `.unwrap()` calls in codegen are assertions that the
//! type checker's guarantees hold — an unwrap failure here indicates a
//! compiler bug in an earlier phase, not malformed user input.
//!
//! This is consistent with how `rustc`, `cranelift`, and other production
//! compilers handle post-validation code generation.
//!
//! ## Supported Backends
//!
//! - **C**: Transpiles to C99 for maximum portability (production)
//! - **x86-64**: Native machine code for x86-64 processors (experimental)
//! - **ARM64**: Native machine code for ARM64/AArch64 processors (experimental)
//! - **WASM**: WebAssembly for web and edge deployment (experimental)
//! - **SPIR-V**: GPU shaders and compute kernels (experimental)
//!
//! ## Example
//!
//! ```rust,ignore
//! use quantalang::codegen::{CodeGenerator, Target};
//! use quantalang::types::TypeContext;
//!
//! let mut ctx = TypeContext::new();
//! let mut codegen = CodeGenerator::new(&ctx, Target::C);
//! let output = codegen.generate(&module)?;
//! ```

pub mod ir;
pub mod lower;
pub mod builder;
pub mod backend;
pub mod debug;
pub mod runtime;

pub use ir::*;
pub use lower::*;
pub use builder::*;
pub use backend::{Backend, Target, CodegenResult, CodegenError};

use std::sync::Arc;

use crate::ast;
use crate::types::TypeContext;

/// The main code generator.
pub struct CodeGenerator<'ctx> {
    /// Type context from type checking.
    ctx: &'ctx TypeContext,
    /// The target backend.
    target: Target,
    /// Generated MIR.
    mir: Option<MirModule>,
    /// Source code for macro expansion.
    source: Option<Arc<str>>,
    /// Generate ReShade .fx boilerplate for HLSL target.
    pub reshade: bool,
}

impl<'ctx> CodeGenerator<'ctx> {
    /// Create a new code generator.
    pub fn new(ctx: &'ctx TypeContext, target: Target) -> Self {
        Self {
            ctx,
            target,
            mir: None,
            source: None,
            reshade: false,
        }
    }

    /// Create a new code generator with source code for macro expansion.
    pub fn with_source(ctx: &'ctx TypeContext, target: Target, source: Arc<str>) -> Self {
        Self {
            ctx,
            target,
            mir: None,
            source: Some(source),
            reshade: false,
        }
    }

    /// Generate code from a type-checked module.
    pub fn generate(&mut self, module: &ast::Module) -> CodegenResult<GeneratedCode> {
        // Lower AST to MIR
        let lowerer = if let Some(ref source) = self.source {
            MirLowerer::with_source(self.ctx, source.clone())
        } else {
            MirLowerer::new(self.ctx)
        };
        let mir = lowerer.lower_module(module)?;
        self.mir = Some(mir);

        // Select backend and generate
        let mir = self.mir.as_ref().unwrap();

        match self.target {
            Target::C => {
                let mut backend = backend::c::CBackend::new();
                backend.generate(mir)
            }
            Target::X86_64 => {
                let mut backend = backend::x86_64::X86_64Backend::new();
                backend.generate(mir)
            }
            Target::Arm64 => {
                let mut backend = backend::arm64::Arm64Backend::new();
                backend.generate(mir)
            }
            Target::Wasm => {
                let mut backend = backend::wasm::WasmBackend::new();
                backend.generate(mir)
            }
            Target::SpirV => {
                let mut backend = backend::spirv::SpirvBackend::new();
                backend.generate(mir)
            }
            Target::LlvmIr => {
                let mut backend = backend::llvm::LlvmBackend::new();
                backend.generate(mir)
            }
            Target::Hlsl => {
                let mut backend = backend::hlsl::HlslBackend::new();
                let hlsl_code = if self.reshade {
                    backend.generate_reshade(mir)?
                } else {
                    backend.generate(mir)?
                };
                Ok(GeneratedCode::new(OutputFormat::Hlsl, hlsl_code.into_bytes()))
            }
            Target::Glsl => {
                let mut backend = backend::glsl::GlslBackend::new();
                let glsl_code = backend.generate(mir)?;
                Ok(GeneratedCode::new(OutputFormat::Glsl, glsl_code.into_bytes()))
            }
        }
    }

    /// Get the generated MIR (for debugging/inspection).
    pub fn mir(&self) -> Option<&MirModule> {
        self.mir.as_ref()
    }
}

/// Generated code output.
#[derive(Debug)]
pub struct GeneratedCode {
    /// The output format.
    pub format: OutputFormat,
    /// The generated code/data.
    pub data: Vec<u8>,
    /// Optional debug information.
    pub debug_info: Option<DebugInfo>,
}

impl GeneratedCode {
    /// Create new generated code.
    pub fn new(format: OutputFormat, data: Vec<u8>) -> Self {
        Self {
            format,
            data,
            debug_info: None,
        }
    }

    /// Add debug information.
    pub fn with_debug_info(mut self, debug_info: DebugInfo) -> Self {
        self.debug_info = Some(debug_info);
        self
    }

    /// Get the code as a string (for text formats).
    pub fn as_string(&self) -> Option<String> {
        match self.format {
            OutputFormat::CSource | OutputFormat::Assembly | OutputFormat::Wat | OutputFormat::LlvmIr | OutputFormat::Hlsl | OutputFormat::Glsl => {
                String::from_utf8(self.data.clone()).ok()
            }
            _ => None,
        }
    }
}

/// Output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// C source code.
    CSource,
    /// Assembly source.
    Assembly,
    /// Object file.
    Object,
    /// Executable.
    Executable,
    /// WebAssembly binary.
    Wasm,
    /// WebAssembly text format (WAT).
    Wat,
    /// SPIR-V binary.
    SpirV,
    /// LLVM IR text format.
    LlvmIr,
    /// HLSL source code.
    Hlsl,
    /// GLSL source code.
    Glsl,
}

/// Debug information for generated code.
#[derive(Debug, Clone)]
pub struct DebugInfo {
    /// Source file mappings.
    pub source_maps: Vec<SourceMap>,
}

/// Source map entry.
#[derive(Debug, Clone)]
pub struct SourceMap {
    /// Generated code offset.
    pub generated_offset: usize,
    /// Original source file.
    pub source_file: String,
    /// Original line number.
    pub line: u32,
    /// Original column.
    pub column: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // GeneratedCode Tests
    // =========================================================================

    #[test]
    fn test_generated_code_new() {
        let code = GeneratedCode::new(
            OutputFormat::CSource,
            b"int main() { return 0; }".to_vec(),
        );
        assert_eq!(code.format, OutputFormat::CSource);
        assert_eq!(code.data, b"int main() { return 0; }");
        assert!(code.debug_info.is_none());
    }

    #[test]
    fn test_generated_code_as_string() {
        let code = GeneratedCode::new(
            OutputFormat::CSource,
            b"int main() { return 0; }".to_vec(),
        );
        assert_eq!(code.as_string(), Some("int main() { return 0; }".to_string()));
    }

    #[test]
    fn test_generated_code_as_string_assembly() {
        let code = GeneratedCode::new(
            OutputFormat::Assembly,
            b"mov rax, 42\nret".to_vec(),
        );
        assert_eq!(code.as_string(), Some("mov rax, 42\nret".to_string()));
    }

    #[test]
    fn test_generated_code_as_string_wat() {
        let code = GeneratedCode::new(
            OutputFormat::Wat,
            b"(module (func (export \"main\")))".to_vec(),
        );
        assert_eq!(code.as_string(), Some("(module (func (export \"main\")))".to_string()));
    }

    #[test]
    fn test_generated_code_as_string_binary_formats() {
        // Binary formats should return None
        let wasm = GeneratedCode::new(OutputFormat::Wasm, vec![0x00, 0x61, 0x73, 0x6d]);
        assert!(wasm.as_string().is_none());

        let object = GeneratedCode::new(OutputFormat::Object, vec![0x7f, 0x45, 0x4c, 0x46]);
        assert!(object.as_string().is_none());

        let executable = GeneratedCode::new(OutputFormat::Executable, vec![0x4d, 0x5a]);
        assert!(executable.as_string().is_none());

        let spirv = GeneratedCode::new(OutputFormat::SpirV, vec![0x03, 0x02, 0x23, 0x07]);
        assert!(spirv.as_string().is_none());
    }

    #[test]
    fn test_generated_code_as_string_invalid_utf8() {
        let code = GeneratedCode::new(
            OutputFormat::CSource,
            vec![0xff, 0xfe, 0x00, 0x01], // Invalid UTF-8
        );
        assert!(code.as_string().is_none());
    }

    #[test]
    fn test_generated_code_with_debug_info() {
        let debug_info = DebugInfo {
            source_maps: vec![
                SourceMap {
                    generated_offset: 0,
                    source_file: "main.qta".to_string(),
                    line: 1,
                    column: 0,
                },
            ],
        };

        let code = GeneratedCode::new(OutputFormat::CSource, b"int main() {}".to_vec())
            .with_debug_info(debug_info);

        assert!(code.debug_info.is_some());
        let info = code.debug_info.unwrap();
        assert_eq!(info.source_maps.len(), 1);
        assert_eq!(info.source_maps[0].source_file, "main.qta");
    }

    #[test]
    fn test_generated_code_empty() {
        let code = GeneratedCode::new(OutputFormat::CSource, vec![]);
        assert!(code.data.is_empty());
        assert_eq!(code.as_string(), Some(String::new()));
    }

    // =========================================================================
    // OutputFormat Tests
    // =========================================================================

    #[test]
    fn test_output_format_equality() {
        assert_eq!(OutputFormat::CSource, OutputFormat::CSource);
        assert_eq!(OutputFormat::Assembly, OutputFormat::Assembly);
        assert_eq!(OutputFormat::Object, OutputFormat::Object);
        assert_eq!(OutputFormat::Executable, OutputFormat::Executable);
        assert_eq!(OutputFormat::Wasm, OutputFormat::Wasm);
        assert_eq!(OutputFormat::Wat, OutputFormat::Wat);
        assert_eq!(OutputFormat::SpirV, OutputFormat::SpirV);
    }

    #[test]
    fn test_output_format_inequality() {
        assert_ne!(OutputFormat::CSource, OutputFormat::Assembly);
        assert_ne!(OutputFormat::Wasm, OutputFormat::Wat);
        assert_ne!(OutputFormat::Object, OutputFormat::Executable);
    }

    #[test]
    fn test_output_format_clone() {
        let format = OutputFormat::CSource;
        let cloned = format.clone();
        assert_eq!(format, cloned);
    }

    #[test]
    fn test_output_format_copy() {
        let format = OutputFormat::Assembly;
        let copied = format;
        assert_eq!(format, copied); // format still usable because Copy
    }

    #[test]
    fn test_output_format_debug() {
        assert_eq!(format!("{:?}", OutputFormat::CSource), "CSource");
        assert_eq!(format!("{:?}", OutputFormat::Assembly), "Assembly");
        assert_eq!(format!("{:?}", OutputFormat::Object), "Object");
        assert_eq!(format!("{:?}", OutputFormat::Executable), "Executable");
        assert_eq!(format!("{:?}", OutputFormat::Wasm), "Wasm");
        assert_eq!(format!("{:?}", OutputFormat::Wat), "Wat");
        assert_eq!(format!("{:?}", OutputFormat::SpirV), "SpirV");
    }

    // =========================================================================
    // DebugInfo Tests
    // =========================================================================

    #[test]
    fn test_debug_info_new() {
        let debug_info = DebugInfo {
            source_maps: vec![],
        };
        assert!(debug_info.source_maps.is_empty());
    }

    #[test]
    fn test_debug_info_with_source_maps() {
        let debug_info = DebugInfo {
            source_maps: vec![
                SourceMap {
                    generated_offset: 0,
                    source_file: "lib.qta".to_string(),
                    line: 1,
                    column: 0,
                },
                SourceMap {
                    generated_offset: 100,
                    source_file: "lib.qta".to_string(),
                    line: 10,
                    column: 4,
                },
                SourceMap {
                    generated_offset: 200,
                    source_file: "util.qta".to_string(),
                    line: 5,
                    column: 8,
                },
            ],
        };
        assert_eq!(debug_info.source_maps.len(), 3);
    }

    #[test]
    fn test_debug_info_clone() {
        let debug_info = DebugInfo {
            source_maps: vec![
                SourceMap {
                    generated_offset: 42,
                    source_file: "test.qta".to_string(),
                    line: 5,
                    column: 10,
                },
            ],
        };
        let cloned = debug_info.clone();
        assert_eq!(cloned.source_maps.len(), 1);
        assert_eq!(cloned.source_maps[0].generated_offset, 42);
    }

    // =========================================================================
    // SourceMap Tests
    // =========================================================================

    #[test]
    fn test_source_map_new() {
        let map = SourceMap {
            generated_offset: 256,
            source_file: "main.qta".to_string(),
            line: 42,
            column: 8,
        };
        assert_eq!(map.generated_offset, 256);
        assert_eq!(map.source_file, "main.qta");
        assert_eq!(map.line, 42);
        assert_eq!(map.column, 8);
    }

    #[test]
    fn test_source_map_clone() {
        let map = SourceMap {
            generated_offset: 100,
            source_file: "test.qta".to_string(),
            line: 1,
            column: 0,
        };
        let cloned = map.clone();
        assert_eq!(cloned.generated_offset, map.generated_offset);
        assert_eq!(cloned.source_file, map.source_file);
        assert_eq!(cloned.line, map.line);
        assert_eq!(cloned.column, map.column);
    }

    #[test]
    fn test_source_map_debug() {
        let map = SourceMap {
            generated_offset: 0,
            source_file: "x.qta".to_string(),
            line: 1,
            column: 0,
        };
        let debug = format!("{:?}", map);
        assert!(debug.contains("SourceMap"));
        assert!(debug.contains("generated_offset"));
        assert!(debug.contains("x.qta"));
    }

    // =========================================================================
    // CodeGenerator Tests
    // =========================================================================

    #[test]
    fn test_code_generator_new() {
        let ctx = TypeContext::new();
        let codegen = CodeGenerator::new(&ctx, Target::C);
        assert_eq!(codegen.target, Target::C);
        assert!(codegen.mir.is_none());
    }

    #[test]
    fn test_code_generator_new_all_targets() {
        let ctx = TypeContext::new();

        let cg_c = CodeGenerator::new(&ctx, Target::C);
        assert_eq!(cg_c.target, Target::C);

        let cg_x86 = CodeGenerator::new(&ctx, Target::X86_64);
        assert_eq!(cg_x86.target, Target::X86_64);

        let cg_arm = CodeGenerator::new(&ctx, Target::Arm64);
        assert_eq!(cg_arm.target, Target::Arm64);

        let cg_wasm = CodeGenerator::new(&ctx, Target::Wasm);
        assert_eq!(cg_wasm.target, Target::Wasm);

        let cg_spirv = CodeGenerator::new(&ctx, Target::SpirV);
        assert_eq!(cg_spirv.target, Target::SpirV);
    }

    #[test]
    fn test_code_generator_mir_initially_none() {
        let ctx = TypeContext::new();
        let codegen = CodeGenerator::new(&ctx, Target::C);
        assert!(codegen.mir().is_none());
    }

    // =========================================================================
    // Target Tests
    // =========================================================================

    #[test]
    fn test_target_variants() {
        let targets = [
            Target::C,
            Target::X86_64,
            Target::Arm64,
            Target::Wasm,
            Target::SpirV,
        ];
        assert_eq!(targets.len(), 5);
    }

    #[test]
    fn test_target_equality() {
        assert_eq!(Target::C, Target::C);
        assert_eq!(Target::X86_64, Target::X86_64);
        assert_ne!(Target::C, Target::X86_64);
        assert_ne!(Target::Wasm, Target::SpirV);
    }

    // =========================================================================
    // Integration Tests
    // =========================================================================

    #[test]
    fn test_full_pipeline_empty_module() {
        // Test the full pipeline with an empty module
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let result = codegen.generate(&module);
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.format, OutputFormat::CSource);

        // Check that MIR was generated
        assert!(codegen.mir().is_some());
    }

    #[test]
    fn test_full_pipeline_c_backend() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let result = codegen.generate(&module);
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.format, OutputFormat::CSource);

        // C output should be valid UTF-8
        assert!(generated.as_string().is_some());
    }

    #[test]
    fn test_full_pipeline_x86_64_backend() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::X86_64);

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let result = codegen.generate(&module);
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.format, OutputFormat::Assembly);

        // Assembly output should be valid UTF-8
        assert!(generated.as_string().is_some());
    }

    #[test]
    fn test_full_pipeline_arm64_backend() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::Arm64);

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let result = codegen.generate(&module);
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.format, OutputFormat::Assembly);
    }

    #[test]
    fn test_full_pipeline_wasm_backend() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::Wasm);

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let result = codegen.generate(&module);
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.format, OutputFormat::Wat);

        // WAT output should be valid UTF-8
        assert!(generated.as_string().is_some());
    }

    #[test]
    fn test_full_pipeline_spirv_backend() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::SpirV);

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let result = codegen.generate(&module);
        assert!(result.is_ok());

        let generated = result.unwrap();
        assert_eq!(generated.format, OutputFormat::SpirV);

        // SPIR-V is binary, so as_string should return None
        assert!(generated.as_string().is_none());
    }

    #[test]
    fn test_mir_accessible_after_generation() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);

        assert!(codegen.mir().is_none());

        let module = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };

        let _ = codegen.generate(&module);

        // MIR should now be accessible
        let mir = codegen.mir();
        assert!(mir.is_some());
    }

    #[test]
    fn test_multiple_generations_overwrite_mir() {
        let ctx = TypeContext::new();
        let mut codegen = CodeGenerator::new(&ctx, Target::C);

        let module1 = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };
        let _ = codegen.generate(&module1);
        assert!(codegen.mir().is_some());

        let module2 = ast::Module {
            attrs: vec![],
            items: vec![],
            span: ast::Span::dummy(),
        };
        let _ = codegen.generate(&module2);
        assert!(codegen.mir().is_some());
    }
}
