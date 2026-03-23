// ===============================================================================
// QUANTALANG AST - PATTERNS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Pattern AST nodes.
//!
//! Patterns are used in match expressions, let bindings, and function parameters.

use crate::lexer::Span;
use super::{Attribute, Expr, Ident, Literal, Mutability, NodeId, Path, Type};

/// A pattern node.
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    /// The kind of pattern.
    pub kind: PatternKind,
    /// The span of this pattern.
    pub span: Span,
    /// Node ID.
    pub id: NodeId,
}

impl Pattern {
    /// Create a new pattern.
    pub fn new(kind: PatternKind, span: Span) -> Self {
        Self {
            kind,
            span,
            id: NodeId::DUMMY,
        }
    }

    /// Create a wildcard pattern.
    pub fn wildcard(span: Span) -> Self {
        Self::new(PatternKind::Wildcard, span)
    }

    /// Create an identifier pattern.
    pub fn ident(name: Ident, mutability: Mutability) -> Self {
        let span = name.span;
        Self::new(
            PatternKind::Ident {
                mutability,
                name,
                subpattern: None,
            },
            span,
        )
    }

    /// Check if this pattern is irrefutable (always matches).
    pub fn is_irrefutable(&self) -> bool {
        match &self.kind {
            PatternKind::Wildcard => true,
            PatternKind::Ident { subpattern: None, .. } => true,
            PatternKind::Ident { subpattern: Some(p), .. } => p.is_irrefutable(),
            PatternKind::Tuple(patterns) => patterns.iter().all(|p| p.is_irrefutable()),
            PatternKind::Ref { pattern, .. } => pattern.is_irrefutable(),
            PatternKind::Paren(p) => p.is_irrefutable(),
            _ => false,
        }
    }

    /// Check if this pattern binds any variables.
    pub fn binds_variables(&self) -> bool {
        match &self.kind {
            PatternKind::Wildcard | PatternKind::Rest => false,
            PatternKind::Ident { .. } => true,
            PatternKind::Literal(_) | PatternKind::Path(_) => false,
            PatternKind::Tuple(patterns) | PatternKind::Slice(patterns) => {
                patterns.iter().any(|p| p.binds_variables())
            }
            PatternKind::Struct { fields, rest, .. } => {
                rest.is_some() || fields.iter().any(|f| f.pattern.binds_variables())
            }
            PatternKind::TupleStruct { patterns, .. } => {
                patterns.iter().any(|p| p.binds_variables())
            }
            PatternKind::Or(patterns) => patterns.iter().any(|p| p.binds_variables()),
            PatternKind::Ref { pattern, .. } | PatternKind::Box(pattern) | PatternKind::Paren(pattern) => {
                pattern.binds_variables()
            }
            PatternKind::Range { .. } => false,
            PatternKind::Macro { .. } | PatternKind::Error => false,
        }
    }
}

/// The kind of pattern.
#[derive(Debug, Clone, PartialEq)]
pub enum PatternKind {
    // =========================================================================
    // BASIC PATTERNS
    // =========================================================================

    /// Wildcard pattern: `_`
    Wildcard,

    /// Rest pattern: `..`
    Rest,

    /// Identifier pattern: `x`, `mut x`, `ref x`, `x @ pattern`
    Ident {
        mutability: Mutability,
        name: Ident,
        subpattern: Option<Box<Pattern>>,
    },

    /// Literal pattern: `42`, `"hello"`, `true`
    Literal(Literal),

    // =========================================================================
    // PATH PATTERNS
    // =========================================================================

    /// Path pattern: `None`, `Some`, `Enum::Variant`
    Path(Path),

    // =========================================================================
    // COMPOUND PATTERNS
    // =========================================================================

    /// Tuple pattern: `(a, b, c)`
    Tuple(Vec<Pattern>),

    /// Slice pattern: `[a, b, c]`, `[first, .., last]`
    Slice(Vec<Pattern>),

    /// Struct pattern: `Point { x, y }`, `Point { x: a, .. }`
    Struct {
        path: Path,
        fields: Vec<FieldPattern>,
        rest: Option<Span>,
    },

    /// Tuple struct pattern: `Some(x)`, `Point(x, y)`
    TupleStruct {
        path: Path,
        patterns: Vec<Pattern>,
    },

    // =========================================================================
    // COMPOSITE PATTERNS
    // =========================================================================

    /// Or pattern: `A | B | C`
    Or(Vec<Pattern>),

    /// Reference pattern: `&x`, `&mut x`
    Ref {
        mutability: Mutability,
        pattern: Box<Pattern>,
    },

    /// Box pattern: `box x`
    Box(Box<Pattern>),

    // =========================================================================
    // RANGE PATTERNS
    // =========================================================================

    /// Range pattern: `0..10`, `'a'..='z'`, `..10`
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        inclusive: bool,
    },

    // =========================================================================
    // SPECIAL
    // =========================================================================

    /// Parenthesized pattern (for span preservation)
    Paren(Box<Pattern>),

    /// Macro pattern
    Macro {
        path: Path,
        tokens: Vec<super::TokenTree>,
    },

    /// Placeholder for error recovery
    Error,
}

/// A field in a struct pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldPattern {
    /// Attributes on this field.
    pub attrs: Vec<Attribute>,
    /// The field name.
    pub name: Ident,
    /// The pattern (None for shorthand `x` meaning `x: x`)
    pub pattern: Pattern,
    /// Whether this uses shorthand syntax.
    pub is_shorthand: bool,
    /// Span.
    pub span: Span,
}

impl FieldPattern {
    /// Create a shorthand field pattern like `x` (meaning `x: x`).
    pub fn shorthand(name: Ident) -> Self {
        let span = name.span;
        Self {
            attrs: Vec::new(),
            pattern: Pattern::ident(name.clone(), Mutability::Immutable),
            name,
            is_shorthand: true,
            span,
        }
    }

    /// Create an explicit field pattern like `x: pattern`.
    pub fn explicit(name: Ident, pattern: Pattern) -> Self {
        let span = name.span.merge(&pattern.span);
        Self {
            attrs: Vec::new(),
            name,
            pattern,
            is_shorthand: false,
            span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Span;

    #[test]
    fn test_irrefutable() {
        let span = Span::dummy();

        // Wildcard is irrefutable
        assert!(Pattern::wildcard(span).is_irrefutable());

        // Simple ident is irrefutable
        let ident = Ident::dummy("x");
        assert!(Pattern::ident(ident, Mutability::Immutable).is_irrefutable());

        // Literal is refutable
        let lit = Pattern::new(PatternKind::Literal(Literal::Int {
            value: 42,
            suffix: None,
            base: crate::lexer::IntBase::Decimal,
        }), span);
        assert!(!lit.is_irrefutable());
    }
}
