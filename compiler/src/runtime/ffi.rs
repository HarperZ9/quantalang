// ===============================================================================
// QUANTALANG RUNTIME - FOREIGN FUNCTION INTERFACE (FFI)
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Foreign Function Interface (FFI) for C interop.
//!
//! Provides comprehensive support for calling C functions from QuantaLang
//! and exposing QuantaLang functions to C code.
//!
//! ## Features
//!
//! - Multiple calling conventions (C, stdcall, fastcall, etc.)
//! - Automatic type mapping between QuantaLang and C
//! - Struct layout compatibility (repr(C))
//! - Variadic function support
//! - Callback marshalling
//! - String conversion utilities
//! - Safe wrappers for common operations

use std::collections::HashMap;
use std::sync::Arc;

// =============================================================================
// CALLING CONVENTIONS
// =============================================================================

/// Calling conventions for FFI functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallingConvention {
    /// C calling convention (cdecl) - default for most platforms.
    C,
    /// System calling convention (platform-specific default).
    System,
    /// Windows stdcall calling convention.
    Stdcall,
    /// Windows fastcall calling convention.
    Fastcall,
    /// Windows vectorcall calling convention.
    Vectorcall,
    /// Windows thiscall calling convention (for C++ methods).
    Thiscall,
    /// AMD64 ABI (System V or Microsoft).
    Win64,
    /// System V AMD64 ABI.
    SysV64,
    /// ARM AAPCS calling convention.
    Aapcs,
    /// ARM64 calling convention.
    AArch64,
    /// WebAssembly calling convention.
    Wasm,
    /// Rust calling convention (unstable ABI).
    Rust,
    /// QuantaLang internal calling convention.
    Quanta,
}

impl CallingConvention {
    /// Get the LLVM calling convention ID.
    pub fn llvm_cc(&self) -> u32 {
        match self {
            CallingConvention::C => 0,           // ccc
            CallingConvention::Fastcall => 65,   // x86_fastcallcc
            CallingConvention::Stdcall => 64,    // x86_stdcallcc
            CallingConvention::Thiscall => 70,   // x86_thiscallcc
            CallingConvention::Vectorcall => 80, // x86_vectorcallcc
            CallingConvention::Win64 => 79,      // win64cc
            CallingConvention::SysV64 => 78,     // x86_64_sysvcc
            CallingConvention::Aapcs => 67,      // aapcscc
            CallingConvention::AArch64 => 0,     // ccc (default for AArch64)
            CallingConvention::Wasm => 0,        // ccc
            CallingConvention::System => 0,      // platform default
            CallingConvention::Rust => 0,        // no stable ABI
            CallingConvention::Quanta => 0,      // internal
        }
    }

    /// Get the string representation for LLVM IR.
    pub fn llvm_str(&self) -> &'static str {
        match self {
            CallingConvention::C => "ccc",
            CallingConvention::Fastcall => "x86_fastcallcc",
            CallingConvention::Stdcall => "x86_stdcallcc",
            CallingConvention::Thiscall => "x86_thiscallcc",
            CallingConvention::Vectorcall => "x86_vectorcallcc",
            CallingConvention::Win64 => "win64cc",
            CallingConvention::SysV64 => "x86_64_sysvcc",
            CallingConvention::Aapcs => "aapcscc",
            CallingConvention::AArch64 => "ccc",
            CallingConvention::Wasm => "ccc",
            CallingConvention::System => "ccc",
            CallingConvention::Rust => "ccc",
            CallingConvention::Quanta => "ccc",
        }
    }

    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "c" | "cdecl" => Some(CallingConvention::C),
            "system" => Some(CallingConvention::System),
            "stdcall" => Some(CallingConvention::Stdcall),
            "fastcall" => Some(CallingConvention::Fastcall),
            "vectorcall" => Some(CallingConvention::Vectorcall),
            "thiscall" => Some(CallingConvention::Thiscall),
            "win64" => Some(CallingConvention::Win64),
            "sysv64" | "sysv" => Some(CallingConvention::SysV64),
            "aapcs" => Some(CallingConvention::Aapcs),
            "aarch64" => Some(CallingConvention::AArch64),
            "wasm" => Some(CallingConvention::Wasm),
            "rust" => Some(CallingConvention::Rust),
            "quanta" => Some(CallingConvention::Quanta),
            _ => None,
        }
    }
}

impl Default for CallingConvention {
    fn default() -> Self {
        CallingConvention::C
    }
}

// =============================================================================
// FFI TYPES
// =============================================================================

/// C-compatible primitive types.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CType {
    /// void
    Void,
    /// char (signed)
    Char,
    /// unsigned char
    UChar,
    /// short
    Short,
    /// unsigned short
    UShort,
    /// int
    Int,
    /// unsigned int
    UInt,
    /// long
    Long,
    /// unsigned long
    ULong,
    /// long long
    LongLong,
    /// unsigned long long
    ULongLong,
    /// float
    Float,
    /// double
    Double,
    /// long double
    LongDouble,
    /// size_t
    SizeT,
    /// ssize_t
    SSizeT,
    /// intptr_t
    IntPtrT,
    /// uintptr_t
    UIntPtrT,
    /// ptrdiff_t
    PtrDiffT,
    /// bool (_Bool in C99)
    Bool,
    /// int8_t
    Int8,
    /// uint8_t
    UInt8,
    /// int16_t
    Int16,
    /// uint16_t
    UInt16,
    /// int32_t
    Int32,
    /// uint32_t
    UInt32,
    /// int64_t
    Int64,
    /// uint64_t
    UInt64,
    /// int128_t (extension)
    Int128,
    /// uint128_t (extension)
    UInt128,
    /// Pointer to another type.
    Ptr(Box<CType>),
    /// Const pointer.
    ConstPtr(Box<CType>),
    /// Fixed-size array.
    Array(Box<CType>, usize),
    /// Function pointer.
    FnPtr(Box<CFunctionSignature>),
    /// Named struct.
    Struct(Arc<str>),
    /// Named union.
    Union(Arc<str>),
    /// Named enum.
    Enum(Arc<str>),
    /// Opaque type (forward declaration).
    Opaque(Arc<str>),
}

impl CType {
    /// Get the size in bytes for this type on the target platform.
    pub fn size(&self, ptr_size: usize) -> Option<usize> {
        match self {
            CType::Void => Some(0),
            CType::Char | CType::UChar | CType::Bool | CType::Int8 | CType::UInt8 => Some(1),
            CType::Short | CType::UShort | CType::Int16 | CType::UInt16 => Some(2),
            CType::Int | CType::UInt | CType::Int32 | CType::UInt32 | CType::Float => Some(4),
            CType::Long | CType::ULong => {
                // Platform-dependent: 4 on Windows, 8 on Unix 64-bit
                if ptr_size == 8 {
                    Some(8) // Assume Unix-like
                } else {
                    Some(4)
                }
            }
            CType::LongLong | CType::ULongLong | CType::Int64 | CType::UInt64 | CType::Double => {
                Some(8)
            }
            CType::LongDouble => Some(16), // x86 extended precision
            CType::Int128 | CType::UInt128 => Some(16),
            CType::SizeT | CType::SSizeT | CType::IntPtrT | CType::UIntPtrT | CType::PtrDiffT => {
                Some(ptr_size)
            }
            CType::Ptr(_) | CType::ConstPtr(_) | CType::FnPtr(_) => Some(ptr_size),
            CType::Array(elem, count) => elem.size(ptr_size).map(|s| s * count),
            CType::Struct(_) | CType::Union(_) | CType::Enum(_) | CType::Opaque(_) => None, // Need type info
        }
    }

    /// Get the alignment in bytes for this type.
    pub fn align(&self, ptr_size: usize) -> Option<usize> {
        match self {
            CType::Void => Some(1),
            CType::Char | CType::UChar | CType::Bool | CType::Int8 | CType::UInt8 => Some(1),
            CType::Short | CType::UShort | CType::Int16 | CType::UInt16 => Some(2),
            CType::Int | CType::UInt | CType::Int32 | CType::UInt32 | CType::Float => Some(4),
            CType::Long | CType::ULong => {
                if ptr_size == 8 {
                    Some(8)
                } else {
                    Some(4)
                }
            }
            CType::LongLong | CType::ULongLong | CType::Int64 | CType::UInt64 | CType::Double => {
                Some(8)
            }
            CType::LongDouble => Some(16),
            CType::Int128 | CType::UInt128 => Some(16),
            CType::SizeT | CType::SSizeT | CType::IntPtrT | CType::UIntPtrT | CType::PtrDiffT => {
                Some(ptr_size)
            }
            CType::Ptr(_) | CType::ConstPtr(_) | CType::FnPtr(_) => Some(ptr_size),
            CType::Array(elem, _) => elem.align(ptr_size),
            CType::Struct(_) | CType::Union(_) | CType::Enum(_) | CType::Opaque(_) => None,
        }
    }

    /// Check if this is a pointer type.
    pub fn is_pointer(&self) -> bool {
        matches!(self, CType::Ptr(_) | CType::ConstPtr(_) | CType::FnPtr(_))
    }

    /// Check if this is an integer type.
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            CType::Char
                | CType::UChar
                | CType::Short
                | CType::UShort
                | CType::Int
                | CType::UInt
                | CType::Long
                | CType::ULong
                | CType::LongLong
                | CType::ULongLong
                | CType::Bool
                | CType::Int8
                | CType::UInt8
                | CType::Int16
                | CType::UInt16
                | CType::Int32
                | CType::UInt32
                | CType::Int64
                | CType::UInt64
                | CType::Int128
                | CType::UInt128
                | CType::SizeT
                | CType::SSizeT
                | CType::IntPtrT
                | CType::UIntPtrT
                | CType::PtrDiffT
        )
    }

    /// Check if this is a floating-point type.
    pub fn is_float(&self) -> bool {
        matches!(self, CType::Float | CType::Double | CType::LongDouble)
    }

    /// Check if this is a signed type.
    pub fn is_signed(&self) -> bool {
        matches!(
            self,
            CType::Char
                | CType::Short
                | CType::Int
                | CType::Long
                | CType::LongLong
                | CType::Int8
                | CType::Int16
                | CType::Int32
                | CType::Int64
                | CType::Int128
                | CType::SSizeT
                | CType::IntPtrT
                | CType::PtrDiffT
                | CType::Float
                | CType::Double
                | CType::LongDouble
        )
    }

    /// Convert to LLVM IR type string.
    pub fn llvm_type(&self, ptr_size: usize) -> String {
        match self {
            CType::Void => "void".to_string(),
            CType::Char | CType::UChar | CType::Bool | CType::Int8 | CType::UInt8 => {
                "i8".to_string()
            }
            CType::Short | CType::UShort | CType::Int16 | CType::UInt16 => "i16".to_string(),
            CType::Int | CType::UInt | CType::Int32 | CType::UInt32 => "i32".to_string(),
            CType::Long | CType::ULong => {
                if ptr_size == 8 {
                    "i64".to_string()
                } else {
                    "i32".to_string()
                }
            }
            CType::LongLong | CType::ULongLong | CType::Int64 | CType::UInt64 => "i64".to_string(),
            CType::Int128 | CType::UInt128 => "i128".to_string(),
            CType::Float => "float".to_string(),
            CType::Double => "double".to_string(),
            CType::LongDouble => "x86_fp80".to_string(),
            CType::SizeT | CType::SSizeT | CType::IntPtrT | CType::UIntPtrT | CType::PtrDiffT => {
                format!("i{}", ptr_size * 8)
            }
            CType::Ptr(inner) | CType::ConstPtr(inner) => {
                format!("{}*", inner.llvm_type(ptr_size))
            }
            CType::FnPtr(_) => "ptr".to_string(),
            CType::Array(elem, count) => {
                format!("[{} x {}]", count, elem.llvm_type(ptr_size))
            }
            CType::Struct(name) => format!("%struct.{}", name),
            CType::Union(name) => format!("%union.{}", name),
            CType::Enum(name) => format!("%enum.{}", name),
            CType::Opaque(name) => format!("%{}", name),
        }
    }
}

/// C function signature.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CFunctionSignature {
    /// Return type.
    pub return_type: CType,
    /// Parameter types.
    pub params: Vec<CType>,
    /// Parameter names (optional).
    pub param_names: Vec<Option<Arc<str>>>,
    /// Is variadic?
    pub is_variadic: bool,
    /// Calling convention.
    pub calling_conv: CallingConvention,
}

impl CFunctionSignature {
    /// Create a new function signature.
    pub fn new(return_type: CType, params: Vec<CType>) -> Self {
        let param_names = vec![None; params.len()];
        Self {
            return_type,
            params,
            param_names,
            is_variadic: false,
            calling_conv: CallingConvention::C,
        }
    }

    /// Set variadic.
    pub fn variadic(mut self) -> Self {
        self.is_variadic = true;
        self
    }

    /// Set calling convention.
    pub fn with_calling_conv(mut self, cc: CallingConvention) -> Self {
        self.calling_conv = cc;
        self
    }

    /// Set parameter names.
    pub fn with_param_names(mut self, names: Vec<Option<Arc<str>>>) -> Self {
        self.param_names = names;
        self
    }
}

// =============================================================================
// STRUCT LAYOUT
// =============================================================================

/// Field in a C struct.
#[derive(Debug, Clone)]
pub struct CStructField {
    /// Field name.
    pub name: Arc<str>,
    /// Field type.
    pub ty: CType,
    /// Offset in bytes from struct start.
    pub offset: usize,
    /// Bit offset (for bit fields).
    pub bit_offset: Option<u8>,
    /// Bit width (for bit fields).
    pub bit_width: Option<u8>,
}

/// C struct definition with computed layout.
#[derive(Debug, Clone)]
pub struct CStructDef {
    /// Struct name.
    pub name: Arc<str>,
    /// Fields.
    pub fields: Vec<CStructField>,
    /// Total size in bytes.
    pub size: usize,
    /// Alignment in bytes.
    pub align: usize,
    /// Is packed (no padding)?
    pub packed: bool,
}

impl CStructDef {
    /// Create a new struct definition and compute layout.
    pub fn new(
        name: impl Into<Arc<str>>,
        fields: Vec<(Arc<str>, CType)>,
        packed: bool,
        ptr_size: usize,
    ) -> Self {
        let name = name.into();
        let mut computed_fields = Vec::new();
        let mut offset = 0usize;
        let mut max_align = 1usize;

        for (field_name, field_ty) in fields {
            let field_align = if packed {
                1
            } else {
                field_ty.align(ptr_size).unwrap_or(1)
            };
            let field_size = field_ty.size(ptr_size).unwrap_or(0);

            // Align offset
            if !packed {
                offset = (offset + field_align - 1) & !(field_align - 1);
            }

            computed_fields.push(CStructField {
                name: field_name,
                ty: field_ty,
                offset,
                bit_offset: None,
                bit_width: None,
            });

            offset += field_size;
            max_align = max_align.max(field_align);
        }

        // Final struct size alignment
        if !packed {
            offset = (offset + max_align - 1) & !(max_align - 1);
        }

        Self {
            name,
            fields: computed_fields,
            size: offset,
            align: if packed { 1 } else { max_align },
            packed,
        }
    }

    /// Get a field by name.
    pub fn get_field(&self, name: &str) -> Option<&CStructField> {
        self.fields.iter().find(|f| f.name.as_ref() == name)
    }

    /// Generate LLVM IR type definition.
    pub fn llvm_type_def(&self, ptr_size: usize) -> String {
        let fields: Vec<String> = self
            .fields
            .iter()
            .map(|f| f.ty.llvm_type(ptr_size))
            .collect();

        if self.packed {
            format!("%struct.{} = type <{{ {} }}>", self.name, fields.join(", "))
        } else {
            format!("%struct.{} = type {{ {} }}", self.name, fields.join(", "))
        }
    }
}

// =============================================================================
// EXTERN FUNCTION DECLARATION
// =============================================================================

/// External function declaration.
#[derive(Debug, Clone)]
pub struct ExternFunction {
    /// Function name.
    pub name: Arc<str>,
    /// Mangled name (for C++ or decorated symbols).
    pub mangled_name: Option<Arc<str>>,
    /// Function signature.
    pub signature: CFunctionSignature,
    /// Library to link.
    pub library: Option<Arc<str>>,
    /// Is weak linkage?
    pub weak: bool,
    /// Documentation.
    pub doc: Option<Arc<str>>,
}

impl ExternFunction {
    /// Create a new extern function.
    pub fn new(name: impl Into<Arc<str>>, signature: CFunctionSignature) -> Self {
        Self {
            name: name.into(),
            mangled_name: None,
            signature,
            library: None,
            weak: false,
            doc: None,
        }
    }

    /// Set the mangled name.
    pub fn with_mangled_name(mut self, name: impl Into<Arc<str>>) -> Self {
        self.mangled_name = Some(name.into());
        self
    }

    /// Set the library to link.
    pub fn with_library(mut self, lib: impl Into<Arc<str>>) -> Self {
        self.library = Some(lib.into());
        self
    }

    /// Set weak linkage.
    pub fn weak(mut self) -> Self {
        self.weak = true;
        self
    }

    /// Get the symbol name to use.
    pub fn symbol_name(&self) -> &str {
        self.mangled_name.as_ref().unwrap_or(&self.name).as_ref()
    }

    /// Generate LLVM declaration.
    pub fn llvm_declaration(&self, ptr_size: usize) -> String {
        let ret_ty = self.signature.return_type.llvm_type(ptr_size);
        let params: Vec<String> = self
            .signature
            .params
            .iter()
            .map(|p| p.llvm_type(ptr_size))
            .collect();

        let variadic = if self.signature.is_variadic {
            ", ..."
        } else {
            ""
        };
        let linkage = if self.weak { "extern_weak " } else { "" };
        let cc = self.signature.calling_conv.llvm_str();

        format!(
            "declare {} {} {} @{}({}{})",
            linkage,
            cc,
            ret_ty,
            self.symbol_name(),
            params.join(", "),
            variadic
        )
    }
}

// =============================================================================
// FFI CONTEXT
// =============================================================================

/// FFI context holding all external declarations.
#[derive(Debug, Default)]
pub struct FfiContext {
    /// External functions.
    pub functions: HashMap<Arc<str>, ExternFunction>,
    /// Struct definitions.
    pub structs: HashMap<Arc<str>, CStructDef>,
    /// Type aliases.
    pub typedefs: HashMap<Arc<str>, CType>,
    /// Libraries to link.
    pub libraries: Vec<LibraryLink>,
    /// Pointer size for target.
    pub ptr_size: usize,
}

/// Library link specification.
#[derive(Debug, Clone)]
pub struct LibraryLink {
    /// Library name.
    pub name: Arc<str>,
    /// Search path (optional).
    pub path: Option<Arc<str>>,
    /// Link kind.
    pub kind: LinkKind,
}

/// Library link kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// Dynamic library (.so, .dll, .dylib).
    Dynamic,
    /// Static library (.a, .lib).
    Static,
    /// Framework (macOS).
    Framework,
}

impl FfiContext {
    /// Create a new FFI context.
    pub fn new(ptr_size: usize) -> Self {
        Self {
            functions: HashMap::new(),
            structs: HashMap::new(),
            typedefs: HashMap::new(),
            libraries: Vec::new(),
            ptr_size,
        }
    }

    /// Add an external function.
    pub fn add_function(&mut self, func: ExternFunction) {
        self.functions.insert(func.name.clone(), func);
    }

    /// Add a struct definition.
    pub fn add_struct(&mut self, def: CStructDef) {
        self.structs.insert(def.name.clone(), def);
    }

    /// Add a type alias.
    pub fn add_typedef(&mut self, name: impl Into<Arc<str>>, ty: CType) {
        self.typedefs.insert(name.into(), ty);
    }

    /// Add a library to link.
    pub fn add_library(&mut self, name: impl Into<Arc<str>>, kind: LinkKind) {
        self.libraries.push(LibraryLink {
            name: name.into(),
            path: None,
            kind,
        });
    }

    /// Resolve a typedef chain to its base type.
    pub fn resolve_type(&self, ty: &CType) -> CType {
        match ty {
            CType::Opaque(name) => {
                if let Some(resolved) = self.typedefs.get(name) {
                    self.resolve_type(resolved)
                } else {
                    ty.clone()
                }
            }
            _ => ty.clone(),
        }
    }

    /// Generate all LLVM declarations.
    pub fn llvm_declarations(&self) -> String {
        let mut output = String::new();

        // Struct definitions
        for def in self.structs.values() {
            output.push_str(&def.llvm_type_def(self.ptr_size));
            output.push('\n');
        }

        if !self.structs.is_empty() {
            output.push('\n');
        }

        // Function declarations
        for func in self.functions.values() {
            output.push_str(&func.llvm_declaration(self.ptr_size));
            output.push('\n');
        }

        output
    }

    /// Add standard C library functions.
    pub fn add_libc(&mut self) {
        // Memory functions
        self.add_function(ExternFunction::new(
            "malloc",
            CFunctionSignature::new(CType::Ptr(Box::new(CType::Void)), vec![CType::SizeT]),
        ));

        self.add_function(ExternFunction::new(
            "calloc",
            CFunctionSignature::new(
                CType::Ptr(Box::new(CType::Void)),
                vec![CType::SizeT, CType::SizeT],
            ),
        ));

        self.add_function(ExternFunction::new(
            "realloc",
            CFunctionSignature::new(
                CType::Ptr(Box::new(CType::Void)),
                vec![CType::Ptr(Box::new(CType::Void)), CType::SizeT],
            ),
        ));

        self.add_function(ExternFunction::new(
            "free",
            CFunctionSignature::new(CType::Void, vec![CType::Ptr(Box::new(CType::Void))]),
        ));

        self.add_function(ExternFunction::new(
            "memcpy",
            CFunctionSignature::new(
                CType::Ptr(Box::new(CType::Void)),
                vec![
                    CType::Ptr(Box::new(CType::Void)),
                    CType::ConstPtr(Box::new(CType::Void)),
                    CType::SizeT,
                ],
            ),
        ));

        self.add_function(ExternFunction::new(
            "memmove",
            CFunctionSignature::new(
                CType::Ptr(Box::new(CType::Void)),
                vec![
                    CType::Ptr(Box::new(CType::Void)),
                    CType::ConstPtr(Box::new(CType::Void)),
                    CType::SizeT,
                ],
            ),
        ));

        self.add_function(ExternFunction::new(
            "memset",
            CFunctionSignature::new(
                CType::Ptr(Box::new(CType::Void)),
                vec![CType::Ptr(Box::new(CType::Void)), CType::Int, CType::SizeT],
            ),
        ));

        self.add_function(ExternFunction::new(
            "memcmp",
            CFunctionSignature::new(
                CType::Int,
                vec![
                    CType::ConstPtr(Box::new(CType::Void)),
                    CType::ConstPtr(Box::new(CType::Void)),
                    CType::SizeT,
                ],
            ),
        ));

        // String functions
        self.add_function(ExternFunction::new(
            "strlen",
            CFunctionSignature::new(CType::SizeT, vec![CType::ConstPtr(Box::new(CType::Char))]),
        ));

        self.add_function(ExternFunction::new(
            "strcpy",
            CFunctionSignature::new(
                CType::Ptr(Box::new(CType::Char)),
                vec![
                    CType::Ptr(Box::new(CType::Char)),
                    CType::ConstPtr(Box::new(CType::Char)),
                ],
            ),
        ));

        self.add_function(ExternFunction::new(
            "strncpy",
            CFunctionSignature::new(
                CType::Ptr(Box::new(CType::Char)),
                vec![
                    CType::Ptr(Box::new(CType::Char)),
                    CType::ConstPtr(Box::new(CType::Char)),
                    CType::SizeT,
                ],
            ),
        ));

        self.add_function(ExternFunction::new(
            "strcmp",
            CFunctionSignature::new(
                CType::Int,
                vec![
                    CType::ConstPtr(Box::new(CType::Char)),
                    CType::ConstPtr(Box::new(CType::Char)),
                ],
            ),
        ));

        self.add_function(ExternFunction::new(
            "strncmp",
            CFunctionSignature::new(
                CType::Int,
                vec![
                    CType::ConstPtr(Box::new(CType::Char)),
                    CType::ConstPtr(Box::new(CType::Char)),
                    CType::SizeT,
                ],
            ),
        ));

        // I/O functions
        self.add_function(ExternFunction::new(
            "printf",
            CFunctionSignature::new(CType::Int, vec![CType::ConstPtr(Box::new(CType::Char))])
                .variadic(),
        ));

        self.add_function(ExternFunction::new(
            "sprintf",
            CFunctionSignature::new(
                CType::Int,
                vec![
                    CType::Ptr(Box::new(CType::Char)),
                    CType::ConstPtr(Box::new(CType::Char)),
                ],
            )
            .variadic(),
        ));

        self.add_function(ExternFunction::new(
            "snprintf",
            CFunctionSignature::new(
                CType::Int,
                vec![
                    CType::Ptr(Box::new(CType::Char)),
                    CType::SizeT,
                    CType::ConstPtr(Box::new(CType::Char)),
                ],
            )
            .variadic(),
        ));

        self.add_function(ExternFunction::new(
            "puts",
            CFunctionSignature::new(CType::Int, vec![CType::ConstPtr(Box::new(CType::Char))]),
        ));

        self.add_function(ExternFunction::new(
            "putchar",
            CFunctionSignature::new(CType::Int, vec![CType::Int]),
        ));

        // Math functions
        self.add_function(ExternFunction::new(
            "sin",
            CFunctionSignature::new(CType::Double, vec![CType::Double]),
        ));

        self.add_function(ExternFunction::new(
            "cos",
            CFunctionSignature::new(CType::Double, vec![CType::Double]),
        ));

        self.add_function(ExternFunction::new(
            "tan",
            CFunctionSignature::new(CType::Double, vec![CType::Double]),
        ));

        self.add_function(ExternFunction::new(
            "sqrt",
            CFunctionSignature::new(CType::Double, vec![CType::Double]),
        ));

        self.add_function(ExternFunction::new(
            "pow",
            CFunctionSignature::new(CType::Double, vec![CType::Double, CType::Double]),
        ));

        self.add_function(ExternFunction::new(
            "exp",
            CFunctionSignature::new(CType::Double, vec![CType::Double]),
        ));

        self.add_function(ExternFunction::new(
            "log",
            CFunctionSignature::new(CType::Double, vec![CType::Double]),
        ));

        self.add_function(ExternFunction::new(
            "log10",
            CFunctionSignature::new(CType::Double, vec![CType::Double]),
        ));

        self.add_function(ExternFunction::new(
            "fabs",
            CFunctionSignature::new(CType::Double, vec![CType::Double]),
        ));

        self.add_function(ExternFunction::new(
            "floor",
            CFunctionSignature::new(CType::Double, vec![CType::Double]),
        ));

        self.add_function(ExternFunction::new(
            "ceil",
            CFunctionSignature::new(CType::Double, vec![CType::Double]),
        ));

        // Process functions
        self.add_function(ExternFunction::new(
            "exit",
            CFunctionSignature::new(CType::Void, vec![CType::Int]),
        ));

        self.add_function(ExternFunction::new(
            "abort",
            CFunctionSignature::new(CType::Void, vec![]),
        ));

        self.add_function(ExternFunction::new(
            "getenv",
            CFunctionSignature::new(
                CType::Ptr(Box::new(CType::Char)),
                vec![CType::ConstPtr(Box::new(CType::Char))],
            ),
        ));
    }
}

// =============================================================================
// STRING UTILITIES
// =============================================================================

/// String encoding for FFI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringEncoding {
    /// UTF-8 (default for Rust/modern systems).
    Utf8,
    /// UTF-16 LE (Windows wide strings).
    Utf16Le,
    /// UTF-16 BE.
    Utf16Be,
    /// UTF-32 LE.
    Utf32Le,
    /// ASCII (7-bit).
    Ascii,
    /// Latin-1 (ISO-8859-1).
    Latin1,
}

/// Convert a Rust string to a null-terminated C string.
pub fn to_c_string(s: &str) -> Vec<u8> {
    let mut bytes = s.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

/// Convert a C string pointer to a Rust string (unsafe).
///
/// # Safety
/// The pointer must be valid and point to a null-terminated string.
pub unsafe fn from_c_string(ptr: *const i8) -> String {
    if ptr.is_null() {
        return String::new();
    }

    let mut len = 0;
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }

    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
    String::from_utf8_lossy(slice).into_owned()
}

/// Convert a UTF-16 string to UTF-8.
pub fn utf16_to_utf8(utf16: &[u16]) -> String {
    String::from_utf16_lossy(utf16)
}

/// Convert a UTF-8 string to UTF-16.
pub fn utf8_to_utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calling_convention_llvm() {
        assert_eq!(CallingConvention::C.llvm_str(), "ccc");
        assert_eq!(CallingConvention::Stdcall.llvm_str(), "x86_stdcallcc");
        assert_eq!(CallingConvention::Fastcall.llvm_str(), "x86_fastcallcc");
    }

    #[test]
    fn test_ctype_size() {
        assert_eq!(CType::Int8.size(8), Some(1));
        assert_eq!(CType::Int32.size(8), Some(4));
        assert_eq!(CType::Int64.size(8), Some(8));
        assert_eq!(CType::Ptr(Box::new(CType::Void)).size(8), Some(8));
        assert_eq!(CType::Ptr(Box::new(CType::Void)).size(4), Some(4));
    }

    #[test]
    fn test_struct_layout() {
        // struct { int32 a; int64 b; int8 c; }
        let fields = vec![
            (Arc::from("a"), CType::Int32),
            (Arc::from("b"), CType::Int64),
            (Arc::from("c"), CType::Int8),
        ];

        let def = CStructDef::new("TestStruct", fields, false, 8);

        assert_eq!(def.fields[0].offset, 0); // a at 0
        assert_eq!(def.fields[1].offset, 8); // b at 8 (aligned to 8)
        assert_eq!(def.fields[2].offset, 16); // c at 16
        assert_eq!(def.size, 24); // padded to alignment 8
        assert_eq!(def.align, 8);
    }

    #[test]
    fn test_packed_struct_layout() {
        let fields = vec![
            (Arc::from("a"), CType::Int32),
            (Arc::from("b"), CType::Int64),
            (Arc::from("c"), CType::Int8),
        ];

        let def = CStructDef::new("PackedStruct", fields, true, 8);

        assert_eq!(def.fields[0].offset, 0); // a at 0
        assert_eq!(def.fields[1].offset, 4); // b at 4 (no alignment)
        assert_eq!(def.fields[2].offset, 12); // c at 12
        assert_eq!(def.size, 13); // no padding
        assert_eq!(def.align, 1);
    }

    #[test]
    fn test_extern_function() {
        let sig = CFunctionSignature::new(CType::Int, vec![CType::ConstPtr(Box::new(CType::Char))])
            .variadic();

        let func = ExternFunction::new("printf", sig);
        let decl = func.llvm_declaration(8);

        assert!(decl.contains("declare"));
        assert!(decl.contains("@printf"));
        assert!(decl.contains("..."));
    }

    #[test]
    fn test_to_c_string() {
        let s = "Hello";
        let c_str = to_c_string(s);
        assert_eq!(c_str, vec![72, 101, 108, 108, 111, 0]);
    }

    #[test]
    fn test_ffi_context_libc() {
        let mut ctx = FfiContext::new(8);
        ctx.add_libc();

        assert!(ctx.functions.contains_key("malloc"));
        assert!(ctx.functions.contains_key("free"));
        assert!(ctx.functions.contains_key("printf"));
        assert!(ctx.functions.contains_key("strlen"));
    }
}
