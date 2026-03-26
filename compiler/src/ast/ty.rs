// ===============================================================================
// QUANTALANG AST - TYPES
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Type AST nodes.
//!
//! Types describe the shape of values in QuantaLang.

use crate::lexer::Span;
use super::{Expr, Ident, Lifetime, Mutability, NodeId, Path, TypeBound};

/// A type node.
#[derive(Debug, Clone, PartialEq)]
pub struct Type {
    /// The kind of type.
    pub kind: TypeKind,
    /// The span of this type.
    pub span: Span,
    /// Node ID.
    pub id: NodeId,
}

impl Type {
    /// Create a new type.
    pub fn new(kind: TypeKind, span: Span) -> Self {
        Self {
            kind,
            span,
            id: NodeId::DUMMY,
        }
    }

    /// Create an inferred type placeholder.
    pub fn inferred(span: Span) -> Self {
        Self::new(TypeKind::Infer, span)
    }

    /// Create a unit type.
    pub fn unit(span: Span) -> Self {
        Self::new(TypeKind::Tuple(Vec::new()), span)
    }

    /// Create a never type.
    pub fn never(span: Span) -> Self {
        Self::new(TypeKind::Never, span)
    }

    /// Check if this is the unit type.
    pub fn is_unit(&self) -> bool {
        matches!(&self.kind, TypeKind::Tuple(v) if v.is_empty())
    }

    /// Check if this is the never type.
    pub fn is_never(&self) -> bool {
        matches!(self.kind, TypeKind::Never)
    }

    /// Check if this is an inferred type.
    pub fn is_inferred(&self) -> bool {
        matches!(self.kind, TypeKind::Infer)
    }
}

/// The kind of type.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeKind {
    // =========================================================================
    // NAMED TYPES
    // =========================================================================

    /// A path type: `std::io::Read`, `Option<T>`
    Path(Path),

    // =========================================================================
    // PRIMITIVE TYPES
    // =========================================================================

    /// The never type: `!`
    Never,

    /// Type to be inferred: `_`
    Infer,

    // =========================================================================
    // COMPOUND TYPES
    // =========================================================================

    /// A tuple type: `(A, B, C)`, `()` for unit
    Tuple(Vec<Type>),

    /// An array type: `[T; N]`
    Array {
        elem: Box<Type>,
        len: Box<Expr>,
    },

    /// A slice type: `[T]`
    Slice(Box<Type>),

    // =========================================================================
    // POINTER TYPES
    // =========================================================================

    /// A reference type: `&T`, `&'a T`, `&mut T`
    Ref {
        lifetime: Option<Lifetime>,
        mutability: Mutability,
        ty: Box<Type>,
    },

    /// A raw pointer: `*const T`, `*mut T`
    Ptr {
        mutability: Mutability,
        ty: Box<Type>,
    },

    // =========================================================================
    // FUNCTION TYPES
    // =========================================================================

    /// A bare function type: `fn(A, B) -> C`
    BareFn {
        is_unsafe: bool,
        is_extern: bool,
        abi: Option<String>,
        params: Vec<BareFnParam>,
        return_ty: Option<Box<Type>>,
        is_variadic: bool,
    },

    /// A closure/function trait: `Fn(A) -> B`, `FnMut(A)`, `FnOnce()`
    FnTrait {
        kind: FnTraitKind,
        params: Vec<Type>,
        return_ty: Option<Box<Type>>,
    },

    // =========================================================================
    // TRAIT OBJECTS
    // =========================================================================

    /// A trait object: `dyn Trait`, `dyn Trait + 'a`
    TraitObject {
        bounds: Vec<TypeBound>,
        lifetime: Option<Lifetime>,
    },

    /// An impl trait: `impl Trait`, `impl Trait + 'a`
    ImplTrait {
        bounds: Vec<TypeBound>,
    },

    // =========================================================================
    // SPECIAL
    // =========================================================================

    /// Parenthesized type (for span preservation)
    Paren(Box<Type>),

    /// Type macro: `my_type!(args)`
    Macro {
        path: Path,
        tokens: Vec<super::TokenTree>,
    },

    /// Placeholder for error recovery
    Error,

    // =========================================================================
    // QUANTALANG EXTENSIONS
    // =========================================================================

    /// Effect annotation: `T with Effect`
    WithEffect {
        ty: Box<Type>,
        effects: Vec<Path>,
    },

    /// Neural type: `Neural<Input, Output>`
    Neural {
        input: Box<Type>,
        output: Box<Type>,
    },

    /// Optional type shorthand: `T?` (sugar for `Option<T>`)
    Optional(Box<Type>),

    /// Result type shorthand: `T!E` (sugar for `Result<T, E>`)
    Result {
        ok: Box<Type>,
        err: Box<Type>,
    },
}

/// Parameter in a bare function type.
#[derive(Debug, Clone, PartialEq)]
pub struct BareFnParam {
    /// Optional parameter name.
    pub name: Option<Ident>,
    /// The parameter type.
    pub ty: Box<Type>,
    /// Span.
    pub span: Span,
}

/// Kind of function trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FnTraitKind {
    /// `Fn` - callable by shared reference
    Fn,
    /// `FnMut` - callable by mutable reference
    FnMut,
    /// `FnOnce` - callable by value (consumes)
    FnOnce,
}

impl FnTraitKind {
    /// Get the trait name.
    pub fn as_str(&self) -> &'static str {
        match self {
            FnTraitKind::Fn => "Fn",
            FnTraitKind::FnMut => "FnMut",
            FnTraitKind::FnOnce => "FnOnce",
        }
    }
}

/// Primitive type names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveType {
    // Integers
    I8, I16, I32, I64, I128, Isize,
    U8, U16, U32, U64, U128, Usize,
    // Floats
    F16, F32, F64,
    // Other
    Bool,
    Char,
    Str,
}

impl PrimitiveType {
    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "i8" => Some(PrimitiveType::I8),
            "i16" => Some(PrimitiveType::I16),
            "i32" => Some(PrimitiveType::I32),
            "i64" => Some(PrimitiveType::I64),
            "i128" => Some(PrimitiveType::I128),
            "isize" => Some(PrimitiveType::Isize),
            "u8" => Some(PrimitiveType::U8),
            "u16" => Some(PrimitiveType::U16),
            "u32" => Some(PrimitiveType::U32),
            "u64" => Some(PrimitiveType::U64),
            "u128" => Some(PrimitiveType::U128),
            "usize" => Some(PrimitiveType::Usize),
            "f16" => Some(PrimitiveType::F16),
            "f32" => Some(PrimitiveType::F32),
            "f64" => Some(PrimitiveType::F64),
            "bool" => Some(PrimitiveType::Bool),
            "char" => Some(PrimitiveType::Char),
            "str" => Some(PrimitiveType::Str),
            _ => None,
        }
    }

    /// Get the type name.
    pub fn as_str(&self) -> &'static str {
        match self {
            PrimitiveType::I8 => "i8",
            PrimitiveType::I16 => "i16",
            PrimitiveType::I32 => "i32",
            PrimitiveType::I64 => "i64",
            PrimitiveType::I128 => "i128",
            PrimitiveType::Isize => "isize",
            PrimitiveType::U8 => "u8",
            PrimitiveType::U16 => "u16",
            PrimitiveType::U32 => "u32",
            PrimitiveType::U64 => "u64",
            PrimitiveType::U128 => "u128",
            PrimitiveType::Usize => "usize",
            PrimitiveType::F16 => "f16",
            PrimitiveType::F32 => "f32",
            PrimitiveType::F64 => "f64",
            PrimitiveType::Bool => "bool",
            PrimitiveType::Char => "char",
            PrimitiveType::Str => "str",
        }
    }

    /// Check if this is an integer type.
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            PrimitiveType::I8
                | PrimitiveType::I16
                | PrimitiveType::I32
                | PrimitiveType::I64
                | PrimitiveType::I128
                | PrimitiveType::Isize
                | PrimitiveType::U8
                | PrimitiveType::U16
                | PrimitiveType::U32
                | PrimitiveType::U64
                | PrimitiveType::U128
                | PrimitiveType::Usize
        )
    }

    /// Check if this is a signed integer type.
    pub fn is_signed(&self) -> bool {
        matches!(
            self,
            PrimitiveType::I8
                | PrimitiveType::I16
                | PrimitiveType::I32
                | PrimitiveType::I64
                | PrimitiveType::I128
                | PrimitiveType::Isize
        )
    }

    /// Check if this is a float type.
    pub fn is_float(&self) -> bool {
        matches!(self, PrimitiveType::F16 | PrimitiveType::F32 | PrimitiveType::F64)
    }
}
