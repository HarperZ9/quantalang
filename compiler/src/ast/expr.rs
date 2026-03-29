// ===============================================================================
// QUANTALANG AST - EXPRESSIONS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Expression AST nodes.
//!
//! Expressions are the core of QuantaLang - everything that produces a value.

use super::{
    AssignOp, Attribute, BinOp, Block, Ident, Mutability, NodeId, Path, Pattern, Type, UnaryOp,
};
use crate::lexer::Span;

/// An expression node.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    /// The kind of expression.
    pub kind: ExprKind,
    /// The span of this expression.
    pub span: Span,
    /// Node ID for this expression.
    pub id: NodeId,
    /// Attributes on this expression.
    pub attrs: Vec<Attribute>,
}

impl Expr {
    /// Create a new expression.
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self {
            kind,
            span,
            id: NodeId::DUMMY,
            attrs: Vec::new(),
        }
    }

    /// Create with attributes.
    pub fn with_attrs(kind: ExprKind, span: Span, attrs: Vec<Attribute>) -> Self {
        Self {
            kind,
            span,
            id: NodeId::DUMMY,
            attrs,
        }
    }

    /// Check if this expression is a place expression (can be assigned to).
    pub fn is_place(&self) -> bool {
        matches!(
            self.kind,
            ExprKind::Ident(_)
                | ExprKind::Field { .. }
                | ExprKind::TupleField { .. }
                | ExprKind::Index { .. }
                | ExprKind::Deref(_)
                | ExprKind::Path(_)
        )
    }

    /// Check if this expression requires a semicolon when used as a statement.
    pub fn requires_semi(&self) -> bool {
        !matches!(
            self.kind,
            ExprKind::If { .. }
                | ExprKind::Match { .. }
                | ExprKind::Loop { .. }
                | ExprKind::While { .. }
                | ExprKind::For { .. }
                | ExprKind::Block(_)
                | ExprKind::Unsafe(_)
                | ExprKind::Async { .. }
                | ExprKind::Handle { .. }
        )
    }
}

/// The kind of expression.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    // =========================================================================
    // LITERALS
    // =========================================================================
    /// A literal value.
    Literal(Literal),

    // =========================================================================
    // IDENTIFIERS AND PATHS
    // =========================================================================
    /// An identifier: `x`, `foo`
    Ident(Ident),

    /// A path: `std::io::Read`
    Path(Path),

    // =========================================================================
    // COMPOUND EXPRESSIONS
    // =========================================================================
    /// An array literal: `[1, 2, 3]`
    Array(Vec<Expr>),

    /// An array with repeated element: `[0; 10]`
    ArrayRepeat {
        element: Box<Expr>,
        count: Box<Expr>,
    },

    /// A tuple: `(a, b, c)`
    Tuple(Vec<Expr>),

    /// A struct literal: `Point { x: 1, y: 2 }`
    Struct {
        path: Path,
        fields: Vec<FieldExpr>,
        rest: Option<Box<Expr>>,
    },

    // =========================================================================
    // OPERATORS
    // =========================================================================
    /// A unary operation: `-x`, `!b`, `*ptr`
    Unary { op: UnaryOp, expr: Box<Expr> },

    /// A binary operation: `a + b`, `x && y`
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// An assignment: `x = 1`, `x += 1`
    Assign {
        op: AssignOp,
        target: Box<Expr>,
        value: Box<Expr>,
    },

    // =========================================================================
    // ACCESS
    // =========================================================================
    /// Field access: `point.x`
    Field { expr: Box<Expr>, field: Ident },

    /// Tuple field access: `tuple.0`
    TupleField {
        expr: Box<Expr>,
        index: u32,
        span: Span,
    },

    /// Index access: `array[0]`
    Index { expr: Box<Expr>, index: Box<Expr> },

    /// Dereference: `*ptr`
    Deref(Box<Expr>),

    /// Reference: `&x`, `&mut x`
    Ref {
        mutability: Mutability,
        expr: Box<Expr>,
    },

    // =========================================================================
    // CALLS
    // =========================================================================
    /// Function call: `foo(1, 2)`
    Call { func: Box<Expr>, args: Vec<Expr> },

    /// Method call: `x.foo(1, 2)`
    MethodCall {
        receiver: Box<Expr>,
        method: Ident,
        generics: Vec<super::GenericArg>,
        args: Vec<Expr>,
    },

    // =========================================================================
    // CONTROL FLOW
    // =========================================================================
    /// If expression: `if cond { ... } else { ... }`
    If {
        condition: Box<Expr>,
        then_branch: Box<Block>,
        else_branch: Option<Box<Expr>>,
    },

    /// If let expression: `if let Pat = expr { ... } else { ... }`
    IfLet {
        pattern: Box<Pattern>,
        expr: Box<Expr>,
        then_branch: Box<Block>,
        else_branch: Option<Box<Expr>>,
    },

    /// Match expression: `match x { ... }`
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },

    /// Infinite loop: `loop { ... }`
    Loop {
        body: Box<Block>,
        label: Option<Ident>,
    },

    /// While loop: `while cond { ... }`
    While {
        condition: Box<Expr>,
        body: Box<Block>,
        label: Option<Ident>,
    },

    /// While let loop: `while let Pat = expr { ... }`
    WhileLet {
        pattern: Box<Pattern>,
        expr: Box<Expr>,
        body: Box<Block>,
        label: Option<Ident>,
    },

    /// For loop: `for x in iter { ... }`
    For {
        pattern: Box<Pattern>,
        iter: Box<Expr>,
        body: Box<Block>,
        label: Option<Ident>,
    },

    // =========================================================================
    // JUMPS
    // =========================================================================
    /// Return: `return`, `return x`
    Return(Option<Box<Expr>>),

    /// Break: `break`, `break x`, `break 'label x`
    Break {
        label: Option<Ident>,
        value: Option<Box<Expr>>,
    },

    /// Continue: `continue`, `continue 'label`
    Continue { label: Option<Ident> },

    // =========================================================================
    // CLOSURES
    // =========================================================================
    /// Closure: `|x, y| x + y`, `move |x| x * 2`
    Closure {
        is_move: bool,
        is_async: bool,
        params: Vec<ClosureParam>,
        return_type: Option<Box<Type>>,
        body: Box<Expr>,
    },

    // =========================================================================
    // BLOCKS
    // =========================================================================
    /// Block expression: `{ ... }`
    Block(Box<Block>),

    /// Unsafe block: `unsafe { ... }`
    Unsafe(Box<Block>),

    /// Async block: `async { ... }`, `async move { ... }`
    Async { is_move: bool, body: Box<Block> },

    // =========================================================================
    // TYPE OPERATIONS
    // =========================================================================
    /// Type cast: `x as i32`
    Cast { expr: Box<Expr>, ty: Box<Type> },

    /// Type ascription: `x: i32`
    TypeAscription { expr: Box<Expr>, ty: Box<Type> },

    // =========================================================================
    // ERROR HANDLING
    // =========================================================================
    /// Try operator: `x?`
    Try(Box<Expr>),

    // =========================================================================
    // ASYNC
    // =========================================================================
    /// Await: `x.await`
    Await(Box<Expr>),

    // =========================================================================
    // RANGES
    // =========================================================================
    /// Range: `..`, `a..`, `..b`, `a..b`
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        inclusive: bool,
    },

    // =========================================================================
    // MACROS
    // =========================================================================
    /// Macro invocation: `println!(...)`
    Macro {
        path: Path,
        delimiter: crate::lexer::Delimiter,
        tokens: Vec<super::TokenTree>,
    },

    // =========================================================================
    // QUANTALANG EXTENSIONS
    // =========================================================================
    /// AI query: `@ai("prompt")`
    AIQuery {
        prompt: Box<Expr>,
        options: Vec<(Ident, Expr)>,
    },

    /// AI inference: `expr @infer Type`
    AIInfer { expr: Box<Expr>, ty: Box<Type> },

    /// Effect handler: `handle effect { ... }`
    Handle {
        effect: Path,
        handlers: Vec<EffectHandler>,
        body: Box<Block>,
    },

    /// Resume from effect: `resume value`
    Resume(Option<Box<Expr>>),

    /// Perform an effect operation: `perform IO.read("/etc/hosts")`
    Perform {
        /// The effect name (e.g., `IO`).
        effect: Ident,
        /// The operation name (e.g., `read`).
        operation: Ident,
        /// Arguments to the operation.
        args: Vec<Expr>,
    },

    // =========================================================================
    // SPECIAL
    // =========================================================================
    /// Parenthesized expression (for span tracking)
    Paren(Box<Expr>),

    /// Placeholder for error recovery
    Error,
}

/// A literal value.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// Integer literal.
    Int {
        value: u128,
        suffix: Option<IntSuffix>,
        base: super::super::lexer::IntBase,
    },
    /// Float literal.
    Float {
        value: f64,
        suffix: Option<FloatSuffix>,
    },
    /// String literal.
    Str { value: String, is_raw: bool },
    /// Byte string literal.
    ByteStr { value: Vec<u8>, is_raw: bool },
    /// Character literal.
    Char(char),
    /// Byte literal.
    Byte(u8),
    /// Boolean literal.
    Bool(bool),
}

/// Integer suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntSuffix {
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,
}

impl IntSuffix {
    /// Parse from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "i8" => Some(IntSuffix::I8),
            "i16" => Some(IntSuffix::I16),
            "i32" => Some(IntSuffix::I32),
            "i64" => Some(IntSuffix::I64),
            "i128" => Some(IntSuffix::I128),
            "isize" => Some(IntSuffix::Isize),
            "u8" => Some(IntSuffix::U8),
            "u16" => Some(IntSuffix::U16),
            "u32" => Some(IntSuffix::U32),
            "u64" => Some(IntSuffix::U64),
            "u128" => Some(IntSuffix::U128),
            "usize" => Some(IntSuffix::Usize),
            _ => None,
        }
    }
}

/// Float suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatSuffix {
    F16,
    F32,
    F64,
}

impl FloatSuffix {
    /// Parse from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "f16" => Some(FloatSuffix::F16),
            "f32" => Some(FloatSuffix::F32),
            "f64" => Some(FloatSuffix::F64),
            _ => None,
        }
    }
}

/// A struct field expression: `x: 1` or `x` (shorthand).
#[derive(Debug, Clone, PartialEq)]
pub struct FieldExpr {
    /// The field name.
    pub name: Ident,
    /// The field value (None for shorthand).
    pub value: Option<Box<Expr>>,
    /// Attributes.
    pub attrs: Vec<Attribute>,
    /// Span.
    pub span: Span,
}

/// A match arm: `Pattern if guard => expr`
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    /// Attributes on this arm.
    pub attrs: Vec<Attribute>,
    /// The pattern to match.
    pub pattern: Pattern,
    /// Optional guard expression.
    pub guard: Option<Box<Expr>>,
    /// The body expression.
    pub body: Box<Expr>,
    /// Span.
    pub span: Span,
}

/// A closure parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct ClosureParam {
    /// The pattern.
    pub pattern: Pattern,
    /// Optional type annotation.
    pub ty: Option<Box<Type>>,
    /// Span.
    pub span: Span,
}

/// An effect handler.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectHandler {
    /// The effect operation being handled.
    pub operation: Ident,
    /// Parameters for the operation.
    pub params: Vec<ClosureParam>,
    /// Handler body.
    pub body: Box<Expr>,
    /// Span.
    pub span: Span,
}

/// Expression precedence for parsing.
impl Expr {
    /// Get the precedence of this expression for disambiguation.
    pub fn precedence(&self) -> u8 {
        match &self.kind {
            ExprKind::Closure { .. } => 0,
            ExprKind::Assign { .. } => 1,
            ExprKind::Range { .. } => 2,
            ExprKind::Binary { op, .. } => op.precedence(),
            ExprKind::Unary { .. } => 25,
            ExprKind::Cast { .. } | ExprKind::TypeAscription { .. } => 26,
            ExprKind::Call { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::Field { .. }
            | ExprKind::TupleField { .. }
            | ExprKind::Index { .. }
            | ExprKind::Try(_)
            | ExprKind::Await(_) => 27,
            _ => 28, // Atoms have highest precedence
        }
    }
}
