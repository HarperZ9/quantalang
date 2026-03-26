// ===============================================================================
// QUANTALANG LEXER - TOKEN DEFINITIONS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Token types and definitions for QuantaLang.
//!
//! This module defines all the token types that can be produced by the lexer,
//! including literals, keywords, operators, and delimiters.

use std::fmt;
use super::span::Span;

/// A token produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The kind of token.
    pub kind: TokenKind,
    /// The span in source code.
    pub span: Span,
}

impl Token {
    /// Create a new token.
    #[inline]
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Check if this is an EOF token.
    #[inline]
    pub fn is_eof(&self) -> bool {
        matches!(self.kind, TokenKind::Eof)
    }

    /// Check if this is a specific keyword.
    #[inline]
    pub fn is_keyword(&self, kw: Keyword) -> bool {
        matches!(&self.kind, TokenKind::Keyword(k) if *k == kw)
    }

    /// Check if this is an identifier.
    #[inline]
    pub fn is_ident(&self) -> bool {
        matches!(self.kind, TokenKind::Ident)
    }

    /// Check if this is a literal.
    #[inline]
    pub fn is_literal(&self) -> bool {
        matches!(self.kind, TokenKind::Literal { .. })
    }

    /// Check if this token can start an expression.
    pub fn can_begin_expr(&self) -> bool {
        match &self.kind {
            TokenKind::Ident
            | TokenKind::Literal { .. }
            | TokenKind::OpenDelim(_)
            | TokenKind::Not
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::And
            | TokenKind::AndAnd
            | TokenKind::Or
            | TokenKind::OrOr => true,

            TokenKind::Keyword(kw) => matches!(
                kw,
                Keyword::If
                    | Keyword::Match
                    | Keyword::Loop
                    | Keyword::While
                    | Keyword::For
                    | Keyword::Return
                    | Keyword::Break
                    | Keyword::Continue
                    | Keyword::Unsafe
                    | Keyword::Async
                    | Keyword::Move
                    | Keyword::Box
                    | Keyword::True
                    | Keyword::False
                    | Keyword::Self_
                    | Keyword::SelfType
            ),

            _ => false,
        }
    }

    /// Check if this token can start a type.
    pub fn can_begin_type(&self) -> bool {
        match &self.kind {
            TokenKind::Ident
            | TokenKind::OpenDelim(Delimiter::Paren)
            | TokenKind::OpenDelim(Delimiter::Bracket)
            | TokenKind::Not
            | TokenKind::Star
            | TokenKind::And
            | TokenKind::Question
            | TokenKind::Lt => true,

            TokenKind::Keyword(kw) => matches!(
                kw,
                Keyword::Fn
                    | Keyword::Unsafe
                    | Keyword::Extern
                    | Keyword::Dyn
                    | Keyword::Impl
                    | Keyword::SelfType
            ),

            _ => false,
        }
    }

    /// Check if this token can start a pattern.
    pub fn can_begin_pattern(&self) -> bool {
        match &self.kind {
            TokenKind::Ident
            | TokenKind::Literal { .. }
            | TokenKind::OpenDelim(Delimiter::Paren)
            | TokenKind::OpenDelim(Delimiter::Bracket)
            | TokenKind::And
            | TokenKind::Minus
            | TokenKind::DotDot
            | TokenKind::DotDotDot
            | TokenKind::DotDotEq => true,

            TokenKind::Keyword(kw) => matches!(
                kw,
                Keyword::Ref
                    | Keyword::Mut
                    | Keyword::True
                    | Keyword::False
                    | Keyword::Box
            ),

            _ => false,
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

/// The kind of a token.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // =========================================================================
    // LITERALS
    // =========================================================================

    /// A literal value (integer, float, string, char, etc.)
    Literal {
        kind: LiteralKind,
        /// For numeric literals, the suffix (e.g., i32, f64)
        suffix: Option<Box<str>>,
    },

    // =========================================================================
    // IDENTIFIERS
    // =========================================================================

    /// An identifier (variable name, function name, etc.)
    Ident,

    /// A lifetime identifier ('a, 'static, etc.)
    Lifetime,

    /// A raw identifier (r#name)
    RawIdent,

    /// The underscore wildcard `_`
    Underscore,

    // =========================================================================
    // KEYWORDS
    // =========================================================================

    /// A keyword
    Keyword(Keyword),

    // =========================================================================
    // OPERATORS - ARITHMETIC
    // =========================================================================

    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `^`
    Caret,

    // =========================================================================
    // OPERATORS - BITWISE
    // =========================================================================

    /// `&`
    And,
    /// `|`
    Or,
    /// `~`
    Tilde,
    /// `<<`
    Shl,
    /// `>>`
    Shr,

    // =========================================================================
    // OPERATORS - LOGICAL
    // =========================================================================

    /// `&&`
    AndAnd,
    /// `||`
    OrOr,
    /// `!`
    Not,

    // =========================================================================
    // OPERATORS - COMPARISON
    // =========================================================================

    /// `==`
    EqEq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,

    // =========================================================================
    // OPERATORS - ASSIGNMENT
    // =========================================================================

    /// `=`
    Eq,
    /// `+=`
    PlusEq,
    /// `-=`
    MinusEq,
    /// `*=`
    StarEq,
    /// `/=`
    SlashEq,
    /// `%=`
    PercentEq,
    /// `^=`
    CaretEq,
    /// `&=`
    AndEq,
    /// `|=`
    OrEq,
    /// `<<=`
    ShlEq,
    /// `>>=`
    ShrEq,

    // =========================================================================
    // OPERATORS - SPECIAL
    // =========================================================================

    /// `->`
    Arrow,
    /// `=>`
    FatArrow,
    /// `|>`
    Pipe,
    /// `::`
    ColonColon,
    /// `..`
    DotDot,
    /// `...`
    DotDotDot,
    /// `..=`
    DotDotEq,

    // =========================================================================
    // DELIMITERS
    // =========================================================================

    /// Opening delimiter: `(`, `[`, `{`
    OpenDelim(Delimiter),
    /// Closing delimiter: `)`, `]`, `}`
    CloseDelim(Delimiter),

    // =========================================================================
    // PUNCTUATION
    // =========================================================================

    /// `.`
    Dot,
    /// `,`
    Comma,
    /// `:`
    Colon,
    /// `;`
    Semi,
    /// `?`
    Question,
    /// `@`
    At,
    /// `#`
    Pound,
    /// `$`
    Dollar,

    // =========================================================================
    // SPECIAL TOKENS
    // =========================================================================

    /// End of file
    Eof,

    /// Whitespace (only emitted if preserving whitespace)
    Whitespace,

    /// Comment (only emitted if preserving comments)
    Comment {
        /// Whether this is a documentation comment (/// or //!)
        is_doc: bool,
        /// Whether this is an inner doc comment (//! or /*!)
        is_inner: bool,
    },

    /// A DSL block (sql!{...}, regex!{...}, etc.)
    DslBlock {
        /// The DSL name (e.g., "sql", "regex")
        name: Box<str>,
    },

    /// Unknown/invalid character
    Unknown,
}

impl TokenKind {
    /// Get the "glue" representation for two-character tokens.
    pub fn glue(&self, next: &TokenKind) -> Option<TokenKind> {
        match (self, next) {
            // Two-character operators
            (TokenKind::Plus, TokenKind::Eq) => Some(TokenKind::PlusEq),
            (TokenKind::Minus, TokenKind::Eq) => Some(TokenKind::MinusEq),
            (TokenKind::Minus, TokenKind::Gt) => Some(TokenKind::Arrow),
            (TokenKind::Star, TokenKind::Eq) => Some(TokenKind::StarEq),
            (TokenKind::Slash, TokenKind::Eq) => Some(TokenKind::SlashEq),
            (TokenKind::Percent, TokenKind::Eq) => Some(TokenKind::PercentEq),
            (TokenKind::Caret, TokenKind::Eq) => Some(TokenKind::CaretEq),
            (TokenKind::And, TokenKind::And) => Some(TokenKind::AndAnd),
            (TokenKind::And, TokenKind::Eq) => Some(TokenKind::AndEq),
            (TokenKind::Or, TokenKind::Or) => Some(TokenKind::OrOr),
            (TokenKind::Or, TokenKind::Eq) => Some(TokenKind::OrEq),
            (TokenKind::Or, TokenKind::Gt) => Some(TokenKind::Pipe),
            (TokenKind::Eq, TokenKind::Eq) => Some(TokenKind::EqEq),
            (TokenKind::Eq, TokenKind::Gt) => Some(TokenKind::FatArrow),
            (TokenKind::Not, TokenKind::Eq) => Some(TokenKind::Ne),
            (TokenKind::Lt, TokenKind::Eq) => Some(TokenKind::Le),
            (TokenKind::Lt, TokenKind::Lt) => Some(TokenKind::Shl),
            (TokenKind::Gt, TokenKind::Eq) => Some(TokenKind::Ge),
            (TokenKind::Gt, TokenKind::Gt) => Some(TokenKind::Shr),
            (TokenKind::Colon, TokenKind::Colon) => Some(TokenKind::ColonColon),
            (TokenKind::Dot, TokenKind::Dot) => Some(TokenKind::DotDot),

            // Three-character operators (need special handling)
            (TokenKind::DotDot, TokenKind::Dot) => Some(TokenKind::DotDotDot),
            (TokenKind::DotDot, TokenKind::Eq) => Some(TokenKind::DotDotEq),
            (TokenKind::Shl, TokenKind::Eq) => Some(TokenKind::ShlEq),
            (TokenKind::Shr, TokenKind::Eq) => Some(TokenKind::ShrEq),

            _ => None,
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Literal { kind, suffix } => {
                write!(f, "{}", kind)?;
                if let Some(s) = suffix {
                    write!(f, "{}", s)?;
                }
                Ok(())
            }
            TokenKind::Ident => write!(f, "identifier"),
            TokenKind::Lifetime => write!(f, "lifetime"),
            TokenKind::RawIdent => write!(f, "raw identifier"),
            TokenKind::Keyword(kw) => write!(f, "`{}`", kw),
            TokenKind::Plus => write!(f, "`+`"),
            TokenKind::Minus => write!(f, "`-`"),
            TokenKind::Star => write!(f, "`*`"),
            TokenKind::Slash => write!(f, "`/`"),
            TokenKind::Percent => write!(f, "`%`"),
            TokenKind::Caret => write!(f, "`^`"),
            TokenKind::And => write!(f, "`&`"),
            TokenKind::Or => write!(f, "`|`"),
            TokenKind::Tilde => write!(f, "`~`"),
            TokenKind::Shl => write!(f, "`<<`"),
            TokenKind::Shr => write!(f, "`>>`"),
            TokenKind::AndAnd => write!(f, "`&&`"),
            TokenKind::OrOr => write!(f, "`||`"),
            TokenKind::Not => write!(f, "`!`"),
            TokenKind::EqEq => write!(f, "`==`"),
            TokenKind::Ne => write!(f, "`!=`"),
            TokenKind::Lt => write!(f, "`<`"),
            TokenKind::Le => write!(f, "`<=`"),
            TokenKind::Gt => write!(f, "`>`"),
            TokenKind::Ge => write!(f, "`>=`"),
            TokenKind::Eq => write!(f, "`=`"),
            TokenKind::PlusEq => write!(f, "`+=`"),
            TokenKind::MinusEq => write!(f, "`-=`"),
            TokenKind::StarEq => write!(f, "`*=`"),
            TokenKind::SlashEq => write!(f, "`/=`"),
            TokenKind::PercentEq => write!(f, "`%=`"),
            TokenKind::CaretEq => write!(f, "`^=`"),
            TokenKind::AndEq => write!(f, "`&=`"),
            TokenKind::OrEq => write!(f, "`|=`"),
            TokenKind::ShlEq => write!(f, "`<<=`"),
            TokenKind::ShrEq => write!(f, "`>>=`"),
            TokenKind::Arrow => write!(f, "`->`"),
            TokenKind::FatArrow => write!(f, "`=>`"),
            TokenKind::Pipe => write!(f, "`|>`"),
            TokenKind::ColonColon => write!(f, "`::`"),
            TokenKind::DotDot => write!(f, "`..`"),
            TokenKind::DotDotDot => write!(f, "`...`"),
            TokenKind::DotDotEq => write!(f, "`..=`"),
            TokenKind::OpenDelim(d) => write!(f, "`{}`", d.open_char()),
            TokenKind::CloseDelim(d) => write!(f, "`{}`", d.close_char()),
            TokenKind::Dot => write!(f, "`.`"),
            TokenKind::Comma => write!(f, "`,`"),
            TokenKind::Colon => write!(f, "`:`"),
            TokenKind::Semi => write!(f, "`;`"),
            TokenKind::Question => write!(f, "`?`"),
            TokenKind::At => write!(f, "`@`"),
            TokenKind::Pound => write!(f, "`#`"),
            TokenKind::Dollar => write!(f, "`$`"),
            TokenKind::Eof => write!(f, "end of file"),
            TokenKind::Whitespace => write!(f, "whitespace"),
            TokenKind::Comment { is_doc, .. } => {
                if *is_doc {
                    write!(f, "doc comment")
                } else {
                    write!(f, "comment")
                }
            }
            TokenKind::DslBlock { name } => write!(f, "`{}!`", name),
            TokenKind::Unknown => write!(f, "unknown"),
            TokenKind::Underscore => write!(f, "`_`"),
        }
    }
}

/// Delimiter types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Delimiter {
    /// Parentheses: `(` `)`
    Paren,
    /// Square brackets: `[` `]`
    Bracket,
    /// Curly braces: `{` `}`
    Brace,
}

impl Delimiter {
    /// Get the opening character for this delimiter.
    #[inline]
    pub const fn open_char(&self) -> char {
        match self {
            Delimiter::Paren => '(',
            Delimiter::Bracket => '[',
            Delimiter::Brace => '{',
        }
    }

    /// Get the closing character for this delimiter.
    #[inline]
    pub const fn close_char(&self) -> char {
        match self {
            Delimiter::Paren => ')',
            Delimiter::Bracket => ']',
            Delimiter::Brace => '}',
        }
    }

    /// Create from an opening character.
    pub fn from_open_char(c: char) -> Option<Self> {
        match c {
            '(' => Some(Delimiter::Paren),
            '[' => Some(Delimiter::Bracket),
            '{' => Some(Delimiter::Brace),
            _ => None,
        }
    }

    /// Create from a closing character.
    pub fn from_close_char(c: char) -> Option<Self> {
        match c {
            ')' => Some(Delimiter::Paren),
            ']' => Some(Delimiter::Bracket),
            '}' => Some(Delimiter::Brace),
            _ => None,
        }
    }
}

/// The kind of a literal.
#[derive(Debug, Clone, PartialEq)]
pub enum LiteralKind {
    /// An integer literal: `42`, `0xFF`, `0b1010`, `0o755`
    Int {
        /// The base of the integer (decimal, hex, binary, octal)
        base: IntBase,
        /// Whether the value is too large to fit in any integer type
        empty_int: bool,
    },

    /// A floating-point literal: `3.14`, `1e10`, `2.5e-3`
    Float {
        /// Whether the literal is empty (just `.` or `e`)
        empty_exponent: bool,
    },

    /// A character literal: `'a'`, `'\n'`, `'\u{1F600}'`
    Char {
        /// Whether the literal was properly terminated
        terminated: bool,
    },

    /// A byte literal: `b'a'`, `b'\xFF'`
    Byte {
        /// Whether the literal was properly terminated
        terminated: bool,
    },

    /// A string literal: `"hello"`, `"line\nbreak"`
    Str {
        /// Whether the literal was properly terminated
        terminated: bool,
    },

    /// A byte string literal: `b"hello"`, `b"\xFF\x00"`
    ByteStr {
        /// Whether the literal was properly terminated
        terminated: bool,
    },

    /// A raw string literal: `r"raw"`, `r#"raw with "quotes""#`
    RawStr {
        /// The number of `#` symbols used
        n_hashes: Option<u8>,
    },

    /// A raw byte string literal: `br"raw"`, `br#"raw"#`
    RawByteStr {
        /// The number of `#` symbols used
        n_hashes: Option<u8>,
    },

    /// A C-style string literal (for FFI): `c"hello"`
    CStr {
        /// Whether the literal was properly terminated
        terminated: bool,
    },

    /// An interpolated/format string literal: `f"Hello, {name}!"`
    FormatStr {
        /// Whether the literal was properly terminated
        terminated: bool,
        /// The parts of the interpolated string
        parts: Vec<InterpolatedPart>,
    },

    /// A boolean literal: `true`, `false`
    Bool(bool),
}

/// Token types for string interpolation.
#[derive(Debug, Clone, PartialEq)]
pub enum InterpolatedPart {
    /// A literal string segment: the text between interpolations.
    Literal(String),
    /// An interpolation expression: the content inside `{...}`.
    Expr(String),
}

impl fmt::Display for LiteralKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LiteralKind::Int { .. } => write!(f, "integer"),
            LiteralKind::Float { .. } => write!(f, "float"),
            LiteralKind::Char { .. } => write!(f, "char"),
            LiteralKind::Byte { .. } => write!(f, "byte"),
            LiteralKind::Str { .. } => write!(f, "string"),
            LiteralKind::ByteStr { .. } => write!(f, "byte string"),
            LiteralKind::RawStr { .. } => write!(f, "raw string"),
            LiteralKind::RawByteStr { .. } => write!(f, "raw byte string"),
            LiteralKind::CStr { .. } => write!(f, "C string"),
            LiteralKind::FormatStr { .. } => write!(f, "format string"),
            LiteralKind::Bool(b) => write!(f, "{}", b),
        }
    }
}

/// The base of an integer literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntBase {
    /// Decimal: `42`
    Decimal,
    /// Hexadecimal: `0xFF`
    Hexadecimal,
    /// Octal: `0o755`
    Octal,
    /// Binary: `0b1010`
    Binary,
}

impl IntBase {
    /// Get the radix for this base.
    #[inline]
    pub const fn radix(&self) -> u32 {
        match self {
            IntBase::Decimal => 10,
            IntBase::Hexadecimal => 16,
            IntBase::Octal => 8,
            IntBase::Binary => 2,
        }
    }

    /// Get the prefix for this base.
    #[inline]
    pub const fn prefix(&self) -> &'static str {
        match self {
            IntBase::Decimal => "",
            IntBase::Hexadecimal => "0x",
            IntBase::Octal => "0o",
            IntBase::Binary => "0b",
        }
    }
}

// =============================================================================
// NUMERIC SUFFIX VALIDATION
// =============================================================================

/// Valid integer type suffixes.
pub const INTEGER_SUFFIXES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize",
    "u8", "u16", "u32", "u64", "u128", "usize",
];

/// Valid floating-point type suffixes.
pub const FLOAT_SUFFIXES: &[&str] = &["f32", "f64"];

/// All valid numeric suffixes.
pub const NUMERIC_SUFFIXES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize",
    "u8", "u16", "u32", "u64", "u128", "usize",
    "f32", "f64",
];

/// Numeric suffix kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericSuffixKind {
    /// Signed integer suffix (i8, i16, i32, i64, i128, isize).
    SignedInt,
    /// Unsigned integer suffix (u8, u16, u32, u64, u128, usize).
    UnsignedInt,
    /// Floating-point suffix (f32, f64).
    Float,
}

/// Result of validating a numeric suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumericSuffixValidation {
    /// No suffix present.
    None,
    /// Valid suffix found.
    Valid {
        /// The suffix kind.
        kind: NumericSuffixKind,
        /// The bit size (or 0 for size variants).
        bits: u16,
        /// Whether this is a pointer-sized type (isize, usize).
        is_size: bool,
    },
    /// Invalid suffix.
    Invalid(String),
}

/// Validate a numeric suffix.
pub fn validate_numeric_suffix(suffix: Option<&str>) -> NumericSuffixValidation {
    let suffix = match suffix {
        Some(s) => s,
        None => return NumericSuffixValidation::None,
    };

    match suffix {
        // Signed integers
        "i8" => NumericSuffixValidation::Valid {
            kind: NumericSuffixKind::SignedInt,
            bits: 8,
            is_size: false,
        },
        "i16" => NumericSuffixValidation::Valid {
            kind: NumericSuffixKind::SignedInt,
            bits: 16,
            is_size: false,
        },
        "i32" => NumericSuffixValidation::Valid {
            kind: NumericSuffixKind::SignedInt,
            bits: 32,
            is_size: false,
        },
        "i64" => NumericSuffixValidation::Valid {
            kind: NumericSuffixKind::SignedInt,
            bits: 64,
            is_size: false,
        },
        "i128" => NumericSuffixValidation::Valid {
            kind: NumericSuffixKind::SignedInt,
            bits: 128,
            is_size: false,
        },
        "isize" => NumericSuffixValidation::Valid {
            kind: NumericSuffixKind::SignedInt,
            bits: 0,
            is_size: true,
        },
        // Unsigned integers
        "u8" => NumericSuffixValidation::Valid {
            kind: NumericSuffixKind::UnsignedInt,
            bits: 8,
            is_size: false,
        },
        "u16" => NumericSuffixValidation::Valid {
            kind: NumericSuffixKind::UnsignedInt,
            bits: 16,
            is_size: false,
        },
        "u32" => NumericSuffixValidation::Valid {
            kind: NumericSuffixKind::UnsignedInt,
            bits: 32,
            is_size: false,
        },
        "u64" => NumericSuffixValidation::Valid {
            kind: NumericSuffixKind::UnsignedInt,
            bits: 64,
            is_size: false,
        },
        "u128" => NumericSuffixValidation::Valid {
            kind: NumericSuffixKind::UnsignedInt,
            bits: 128,
            is_size: false,
        },
        "usize" => NumericSuffixValidation::Valid {
            kind: NumericSuffixKind::UnsignedInt,
            bits: 0,
            is_size: true,
        },
        // Floating-point
        "f32" => NumericSuffixValidation::Valid {
            kind: NumericSuffixKind::Float,
            bits: 32,
            is_size: false,
        },
        "f64" => NumericSuffixValidation::Valid {
            kind: NumericSuffixKind::Float,
            bits: 64,
            is_size: false,
        },
        // Invalid
        _ => NumericSuffixValidation::Invalid(suffix.to_string()),
    }
}

/// Check if a suffix is valid for an integer literal.
pub fn is_valid_int_suffix(suffix: &str) -> bool {
    INTEGER_SUFFIXES.contains(&suffix) || FLOAT_SUFFIXES.contains(&suffix)
}

/// Check if a suffix is valid for a float literal.
pub fn is_valid_float_suffix(suffix: &str) -> bool {
    FLOAT_SUFFIXES.contains(&suffix)
}

/// Check if a suffix is an integer-only suffix.
pub fn is_integer_only_suffix(suffix: &str) -> bool {
    INTEGER_SUFFIXES.contains(&suffix)
}

// =============================================================================
// DOCUMENTATION COMMENT TYPES
// =============================================================================

/// A documentation comment extracted from source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocComment {
    /// The kind of documentation comment.
    pub kind: DocCommentKind,
    /// The content of the documentation comment (without the `///` or `//!` prefix).
    pub content: String,
    /// The span of the comment in source.
    pub span: Span,
}

impl DocComment {
    /// Create a new documentation comment.
    pub fn new(kind: DocCommentKind, content: String, span: Span) -> Self {
        Self { kind, content, span }
    }

    /// Check if this is an inner doc comment (`//!` or `/*!`).
    pub fn is_inner(&self) -> bool {
        matches!(self.kind, DocCommentKind::InnerLine | DocCommentKind::InnerBlock)
    }

    /// Check if this is an outer doc comment (`///` or `/**`).
    pub fn is_outer(&self) -> bool {
        matches!(self.kind, DocCommentKind::OuterLine | DocCommentKind::OuterBlock)
    }

    /// Check if this is a line comment.
    pub fn is_line(&self) -> bool {
        matches!(self.kind, DocCommentKind::OuterLine | DocCommentKind::InnerLine)
    }

    /// Check if this is a block comment.
    pub fn is_block(&self) -> bool {
        matches!(self.kind, DocCommentKind::OuterBlock | DocCommentKind::InnerBlock)
    }
}

/// The kind of documentation comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocCommentKind {
    /// Outer line comment: `/// doc`
    OuterLine,
    /// Inner line comment: `//! doc`
    InnerLine,
    /// Outer block comment: `/** doc */`
    OuterBlock,
    /// Inner block comment: `/*! doc */`
    InnerBlock,
}

impl DocCommentKind {
    /// Get the prefix string for this kind of doc comment.
    pub const fn prefix(&self) -> &'static str {
        match self {
            DocCommentKind::OuterLine => "///",
            DocCommentKind::InnerLine => "//!",
            DocCommentKind::OuterBlock => "/**",
            DocCommentKind::InnerBlock => "/*!",
        }
    }
}

/// A collection of documentation comments for a single item.
#[derive(Debug, Clone, Default)]
pub struct DocComments {
    /// The collected documentation comments.
    comments: Vec<DocComment>,
}

impl DocComments {
    /// Create an empty collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from a vector of doc comments.
    pub fn from_vec(comments: Vec<DocComment>) -> Self {
        Self { comments }
    }

    /// Add a documentation comment.
    pub fn push(&mut self, comment: DocComment) {
        self.comments.push(comment);
    }

    /// Check if the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.comments.is_empty()
    }

    /// Get the number of comments.
    pub fn len(&self) -> usize {
        self.comments.len()
    }

    /// Get the comments as a slice.
    pub fn comments(&self) -> &[DocComment] {
        &self.comments
    }

    /// Get only outer doc comments.
    pub fn outer_comments(&self) -> Vec<&DocComment> {
        self.comments.iter().filter(|c| c.is_outer()).collect()
    }

    /// Get only inner doc comments.
    pub fn inner_comments(&self) -> Vec<&DocComment> {
        self.comments.iter().filter(|c| c.is_inner()).collect()
    }

    /// Combine all comments into a single documentation string.
    /// Lines are joined with newlines.
    pub fn to_doc_string(&self) -> String {
        self.comments
            .iter()
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Combine only outer comments into a documentation string.
    pub fn to_outer_doc_string(&self) -> String {
        self.comments
            .iter()
            .filter(|c| c.is_outer())
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Combine only inner comments into a documentation string.
    pub fn to_inner_doc_string(&self) -> String {
        self.comments
            .iter()
            .filter(|c| c.is_inner())
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Iterate over the comments.
    pub fn iter(&self) -> impl Iterator<Item = &DocComment> {
        self.comments.iter()
    }
}

impl IntoIterator for DocComments {
    type Item = DocComment;
    type IntoIter = std::vec::IntoIter<DocComment>;

    fn into_iter(self) -> Self::IntoIter {
        self.comments.into_iter()
    }
}

impl<'a> IntoIterator for &'a DocComments {
    type Item = &'a DocComment;
    type IntoIter = std::slice::Iter<'a, DocComment>;

    fn into_iter(self) -> Self::IntoIter {
        self.comments.iter()
    }
}

/// Keywords in QuantaLang.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Keyword {
    // =========================================================================
    // DECLARATION KEYWORDS
    // =========================================================================

    /// `fn`
    Fn,
    /// `struct`
    Struct,
    /// `enum`
    Enum,
    /// `trait`
    Trait,
    /// `impl`
    Impl,
    /// `type`
    Type,
    /// `const`
    Const,
    /// `static`
    Static,
    /// `let`
    Let,
    /// `mut`
    Mut,
    /// `pub`
    Pub,
    /// `mod`
    Mod,
    /// `module` (QuantaLang ecosystem module declaration)
    Module,
    /// `use`
    Use,
    /// `as`
    As,
    /// `crate`
    Crate,
    /// `super`
    Super,
    /// `self`
    Self_,
    /// `Self`
    SelfType,

    // =========================================================================
    // CONTROL FLOW KEYWORDS
    // =========================================================================

    /// `if`
    If,
    /// `else`
    Else,
    /// `match`
    Match,
    /// `loop`
    Loop,
    /// `while`
    While,
    /// `for`
    For,
    /// `in`
    In,
    /// `break`
    Break,
    /// `continue`
    Continue,
    /// `return`
    Return,

    // =========================================================================
    // BOOLEAN KEYWORDS
    // =========================================================================

    /// `true`
    True,
    /// `false`
    False,

    // =========================================================================
    // TYPE KEYWORDS
    // =========================================================================

    /// `where`
    Where,
    /// `dyn`
    Dyn,
    /// `typeof`
    Typeof,
    /// `sizeof`
    Sizeof,

    // =========================================================================
    // MEMORY & SAFETY KEYWORDS
    // =========================================================================

    /// `ref`
    Ref,
    /// `move`
    Move,
    /// `box`
    Box,
    /// `unsafe`
    Unsafe,
    /// `extern`
    Extern,

    // =========================================================================
    // ASYNC KEYWORDS
    // =========================================================================

    /// `async`
    Async,
    /// `await`
    Await,

    // =========================================================================
    // EFFECT SYSTEM KEYWORDS
    // =========================================================================

    /// `with`
    With,
    /// `effect`
    Effect,
    /// `handle`
    Handle,
    /// `resume`
    Resume,
    /// `perform`
    Perform,

    // =========================================================================
    // AI CONSTRUCT KEYWORDS
    // =========================================================================

    /// `ai`
    AI,
    /// `neural`
    Neural,
    /// `infer`
    Infer,

    // =========================================================================
    // MACRO KEYWORDS
    // =========================================================================

    /// `macro`
    Macro,
    /// `macro_rules`
    MacroRules,

    // =========================================================================
    // RESERVED KEYWORDS (for future use)
    // =========================================================================

    /// `abstract`
    Abstract,
    /// `become`
    Become,
    /// `do`
    Do,
    /// `final`
    Final,
    /// `override`
    Override,
    /// `priv`
    Priv,
    /// `try`
    Try,
    /// `yield`
    Yield,
    /// `union`
    Union,
    /// `default`
    Default,
    /// `auto`
    Auto,
}

impl Keyword {
    /// Get the keyword string.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Keyword::Fn => "fn",
            Keyword::Struct => "struct",
            Keyword::Enum => "enum",
            Keyword::Trait => "trait",
            Keyword::Impl => "impl",
            Keyword::Type => "type",
            Keyword::Const => "const",
            Keyword::Static => "static",
            Keyword::Let => "let",
            Keyword::Mut => "mut",
            Keyword::Pub => "pub",
            Keyword::Mod => "mod",
            Keyword::Module => "module",
            Keyword::Use => "use",
            Keyword::As => "as",
            Keyword::Crate => "crate",
            Keyword::Super => "super",
            Keyword::Self_ => "self",
            Keyword::SelfType => "Self",
            Keyword::If => "if",
            Keyword::Else => "else",
            Keyword::Match => "match",
            Keyword::Loop => "loop",
            Keyword::While => "while",
            Keyword::For => "for",
            Keyword::In => "in",
            Keyword::Break => "break",
            Keyword::Continue => "continue",
            Keyword::Return => "return",
            Keyword::True => "true",
            Keyword::False => "false",
            Keyword::Where => "where",
            Keyword::Dyn => "dyn",
            Keyword::Typeof => "typeof",
            Keyword::Sizeof => "sizeof",
            Keyword::Ref => "ref",
            Keyword::Move => "move",
            Keyword::Box => "box",
            Keyword::Unsafe => "unsafe",
            Keyword::Extern => "extern",
            Keyword::Async => "async",
            Keyword::Await => "await",
            Keyword::With => "with",
            Keyword::Effect => "effect",
            Keyword::Handle => "handle",
            Keyword::Resume => "resume",
            Keyword::Perform => "perform",
            Keyword::AI => "ai",
            Keyword::Neural => "neural",
            Keyword::Infer => "infer",
            Keyword::Macro => "macro",
            Keyword::MacroRules => "macro_rules",
            Keyword::Abstract => "abstract",
            Keyword::Become => "become",
            Keyword::Do => "do",
            Keyword::Final => "final",
            Keyword::Override => "override",
            Keyword::Priv => "priv",
            Keyword::Try => "try",
            Keyword::Yield => "yield",
            Keyword::Union => "union",
            Keyword::Default => "default",
            Keyword::Auto => "auto",
        }
    }

    /// Parse a keyword from a string.
    pub fn from_str(s: &str) -> Option<Keyword> {
        match s {
            "fn" => Some(Keyword::Fn),
            "struct" => Some(Keyword::Struct),
            "enum" => Some(Keyword::Enum),
            "trait" => Some(Keyword::Trait),
            "impl" => Some(Keyword::Impl),
            "type" => Some(Keyword::Type),
            "const" => Some(Keyword::Const),
            "static" => Some(Keyword::Static),
            "let" => Some(Keyword::Let),
            "mut" => Some(Keyword::Mut),
            "pub" => Some(Keyword::Pub),
            "mod" => Some(Keyword::Mod),
            "module" => Some(Keyword::Module),
            "use" => Some(Keyword::Use),
            "as" => Some(Keyword::As),
            "crate" => Some(Keyword::Crate),
            "super" => Some(Keyword::Super),
            "self" => Some(Keyword::Self_),
            "Self" => Some(Keyword::SelfType),
            "if" => Some(Keyword::If),
            "else" => Some(Keyword::Else),
            "match" => Some(Keyword::Match),
            "loop" => Some(Keyword::Loop),
            "while" => Some(Keyword::While),
            "for" => Some(Keyword::For),
            "in" => Some(Keyword::In),
            "break" => Some(Keyword::Break),
            "continue" => Some(Keyword::Continue),
            "return" => Some(Keyword::Return),
            "true" => Some(Keyword::True),
            "false" => Some(Keyword::False),
            "where" => Some(Keyword::Where),
            "dyn" => Some(Keyword::Dyn),
            "typeof" => Some(Keyword::Typeof),
            "sizeof" => Some(Keyword::Sizeof),
            "ref" => Some(Keyword::Ref),
            "move" => Some(Keyword::Move),
            "box" => Some(Keyword::Box),
            "unsafe" => Some(Keyword::Unsafe),
            "extern" => Some(Keyword::Extern),
            "async" => Some(Keyword::Async),
            "await" => Some(Keyword::Await),
            "with" => Some(Keyword::With),
            "effect" => Some(Keyword::Effect),
            "handle" => Some(Keyword::Handle),
            "resume" => Some(Keyword::Resume),
            "perform" => Some(Keyword::Perform),
            "ai" => Some(Keyword::AI),
            "neural" => Some(Keyword::Neural),
            "infer" => Some(Keyword::Infer),
            "macro" => Some(Keyword::Macro),
            "macro_rules" => Some(Keyword::MacroRules),
            "abstract" => Some(Keyword::Abstract),
            "become" => Some(Keyword::Become),
            "do" => Some(Keyword::Do),
            "final" => Some(Keyword::Final),
            "override" => Some(Keyword::Override),
            "priv" => Some(Keyword::Priv),
            "try" => Some(Keyword::Try),
            "yield" => Some(Keyword::Yield),
            "union" => Some(Keyword::Union),
            "default" => Some(Keyword::Default),
            "auto" => Some(Keyword::Auto),
            _ => None,
        }
    }

    /// Check if this keyword is reserved for future use.
    pub const fn is_reserved(&self) -> bool {
        matches!(
            self,
            Keyword::Abstract
                | Keyword::Become
                | Keyword::Do
                | Keyword::Final
                | Keyword::Override
                | Keyword::Priv
                | Keyword::Try
                | Keyword::Yield
        )
    }

    /// Check if this keyword can be used as an identifier with raw syntax (r#keyword).
    pub const fn can_be_raw(&self) -> bool {
        !matches!(self, Keyword::Crate | Keyword::Self_ | Keyword::SelfType | Keyword::Super)
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// DSL block names recognized by the lexer.
pub const DSL_NAMES: &[&str] = &[
    "sql",
    "regex",
    "math",
    "finance",
    "glsl",
    "hlsl",
    "shell",
    "json",
    "xml",
    "html",
    "css",
    "graphql",
];

/// Check if a name is a recognized DSL.
pub fn is_dsl_name(name: &str) -> bool {
    DSL_NAMES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_roundtrip() {
        for kw in [
            Keyword::Fn,
            Keyword::Struct,
            Keyword::Let,
            Keyword::If,
            Keyword::Match,
        ] {
            let s = kw.as_str();
            assert_eq!(Keyword::from_str(s), Some(kw));
        }
    }

    #[test]
    fn test_delimiter_chars() {
        assert_eq!(Delimiter::Paren.open_char(), '(');
        assert_eq!(Delimiter::Paren.close_char(), ')');
        assert_eq!(Delimiter::from_open_char('('), Some(Delimiter::Paren));
        assert_eq!(Delimiter::from_close_char(')'), Some(Delimiter::Paren));
    }

    #[test]
    fn test_token_glue() {
        assert_eq!(
            TokenKind::Plus.glue(&TokenKind::Eq),
            Some(TokenKind::PlusEq)
        );
        assert_eq!(
            TokenKind::Eq.glue(&TokenKind::Gt),
            Some(TokenKind::FatArrow)
        );
        assert_eq!(
            TokenKind::Minus.glue(&TokenKind::Gt),
            Some(TokenKind::Arrow)
        );
    }
}
