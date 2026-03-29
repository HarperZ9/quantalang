// ===============================================================================
// QUANTALANG AST MODULE
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! # Abstract Syntax Tree
//!
//! This module defines the AST node types for QuantaLang. The AST is produced
//! by the parser and consumed by later stages (type checking, code generation).
//!
//! ## Design Principles
//!
//! - Every node carries a `Span` for error reporting
//! - Nodes are designed for both analysis and transformation
//! - Expression nodes support the full Pratt parsing operator set
//! - Pattern nodes mirror expression structure where applicable

mod expr;
mod item;
mod operators;
mod pattern;
mod stmt;
mod ty;

pub use expr::*;
pub use item::*;
pub use operators::*;
pub use pattern::*;
pub use stmt::*;
pub use ty::*;

pub use crate::lexer::Span;
use std::sync::Arc;

/// A unique identifier for AST nodes (for later passes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

impl NodeId {
    /// A dummy node ID for synthetic nodes.
    pub const DUMMY: Self = Self(u32::MAX);

    /// Create a new node ID.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::DUMMY
    }
}

/// An interned identifier (variable name, function name, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ident {
    /// The name of the identifier.
    pub name: Arc<str>,
    /// The span where this identifier appears.
    pub span: Span,
}

impl Ident {
    /// Create a new identifier.
    pub fn new(name: impl Into<Arc<str>>, span: Span) -> Self {
        Self {
            name: name.into(),
            span,
        }
    }

    /// Create a dummy identifier (for synthetic nodes).
    pub fn dummy(name: impl Into<Arc<str>>) -> Self {
        Self {
            name: name.into(),
            span: Span::dummy(),
        }
    }

    /// Get the name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Display for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl AsRef<str> for Ident {
    fn as_ref(&self) -> &str {
        &self.name
    }
}

/// A path like `std::collections::HashMap`.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    /// The segments of the path.
    pub segments: Vec<PathSegment>,
    /// The span of the entire path.
    pub span: Span,
}

impl Path {
    /// Create a new path.
    pub fn new(segments: Vec<PathSegment>, span: Span) -> Self {
        Self { segments, span }
    }

    /// Create a single-segment path from an identifier.
    pub fn from_ident(ident: Ident) -> Self {
        let span = ident.span;
        Self {
            segments: vec![PathSegment::from_ident(ident)],
            span,
        }
    }

    /// Check if this path has a single segment.
    pub fn is_simple(&self) -> bool {
        self.segments.len() == 1 && self.segments[0].generics.is_empty()
    }

    /// Get the last segment's identifier.
    pub fn last_ident(&self) -> Option<&Ident> {
        self.segments.last().map(|s| &s.ident)
    }

    /// Get the last segment's generic arguments.
    pub fn last_generics(&self) -> Option<&[GenericArg]> {
        self.segments.last().map(|s| s.generics.as_slice())
    }
}

/// A segment in a path (e.g., `HashMap<K, V>`).
#[derive(Debug, Clone, PartialEq)]
pub struct PathSegment {
    /// The identifier.
    pub ident: Ident,
    /// Generic arguments (if any).
    pub generics: Vec<GenericArg>,
}

impl PathSegment {
    /// Create a segment from an identifier.
    pub fn from_ident(ident: Ident) -> Self {
        Self {
            ident,
            generics: Vec::new(),
        }
    }

    /// Create a simple segment (alias for from_ident).
    pub fn simple(ident: Ident) -> Self {
        Self::from_ident(ident)
    }

    /// Create a segment with generic arguments.
    pub fn with_generics(ident: Ident, generics: Vec<GenericArg>) -> Self {
        Self { ident, generics }
    }
}

/// A generic argument in a path.
#[derive(Debug, Clone, PartialEq)]
pub enum GenericArg {
    /// A type argument.
    Type(Box<Type>),
    /// A lifetime argument.
    Lifetime(Lifetime),
    /// A const argument.
    Const(Box<Expr>),
}

/// A lifetime like `'a` or `'static`.
#[derive(Debug, Clone, PartialEq)]
pub struct Lifetime {
    /// The name (without the leading `'`).
    pub name: Ident,
    /// The span including the `'`.
    pub span: Span,
}

impl Lifetime {
    /// Create a new lifetime.
    pub fn new(name: Ident, span: Span) -> Self {
        Self { name, span }
    }

    /// Check if this is the static lifetime.
    pub fn is_static(&self) -> bool {
        self.name.as_str() == "static"
    }

    /// Check if this is the anonymous lifetime `'_`.
    pub fn is_anonymous(&self) -> bool {
        self.name.as_str() == "_"
    }
}

/// Visibility of an item.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Visibility {
    /// Private (default).
    #[default]
    Private,
    /// Public to all.
    Public(Span),
    /// Public within the crate.
    Crate(Span),
    /// Public to the parent module.
    Super(Span),
    /// Public to a specific path.
    Restricted { path: Path, span: Span },
}

impl Visibility {
    /// Check if this is public.
    pub fn is_public(&self) -> bool {
        matches!(self, Visibility::Public(_))
    }

    /// Get the span of the visibility modifier.
    pub fn span(&self) -> Option<Span> {
        match self {
            Visibility::Private => None,
            Visibility::Public(span) => Some(*span),
            Visibility::Crate(span) => Some(*span),
            Visibility::Super(span) => Some(*span),
            Visibility::Restricted { span, .. } => Some(*span),
        }
    }
}

/// Mutability marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Mutability {
    /// Immutable (default).
    #[default]
    Immutable,
    /// Mutable.
    Mutable,
}

impl Mutability {
    /// Check if mutable.
    pub fn is_mut(&self) -> bool {
        matches!(self, Mutability::Mutable)
    }
}

/// An attribute like `#[derive(Debug)]` or `#![no_std]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    /// The path of the attribute.
    pub path: Path,
    /// The arguments/body of the attribute.
    pub args: AttrArgs,
    /// Whether this is an inner attribute (`#!`).
    pub is_inner: bool,
    /// The span of the entire attribute.
    pub span: Span,
}

/// Attribute arguments.
#[derive(Debug, Clone, PartialEq)]
pub enum AttrArgs {
    /// No arguments: `#[test]`
    Empty,
    /// Parenthesized tokens: `#[derive(Debug, Clone)]`
    Delimited(Vec<TokenTree>),
    /// Equals sign and expression: `#[path = "foo.rs"]`
    Eq(Box<Expr>),
}

/// A token tree for macro/attribute arguments.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenTree {
    /// A single token.
    Token(crate::lexer::Token),
    /// A delimited group.
    Delimited {
        delimiter: crate::lexer::Delimiter,
        tokens: Vec<TokenTree>,
        span: Span,
    },
}

/// A parsed module (source file AST).
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    /// Inner attributes.
    pub attrs: Vec<Attribute>,
    /// Top-level items.
    pub items: Vec<Item>,
    /// The span of the entire file.
    pub span: Span,
}

impl Module {
    /// Create a new module.
    pub fn new(attrs: Vec<Attribute>, items: Vec<Item>, span: Span) -> Self {
        Self { attrs, items, span }
    }
}

/// A block of statements.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The statements in the block.
    /// The final statement may be an expression statement without semicolon,
    /// which serves as the block's return value.
    pub stmts: Vec<Stmt>,
    /// The span of the block including braces.
    pub span: Span,
    /// Node ID for this block.
    pub id: NodeId,
}

impl Block {
    /// Create a new block.
    pub fn new(stmts: Vec<Stmt>, span: Span) -> Self {
        Self {
            stmts,
            span,
            id: NodeId::DUMMY,
        }
    }

    /// Check if the block is empty.
    pub fn is_empty(&self) -> bool {
        self.stmts.is_empty()
    }

    /// Get the trailing expression (the block's value) if any.
    pub fn tail_expr(&self) -> Option<&Expr> {
        match self.stmts.last() {
            Some(stmt) => match &stmt.kind {
                StmtKind::Expr(expr) => Some(expr),
                _ => None,
            },
            None => None,
        }
    }
}

/// Generic parameter definition.
#[derive(Debug, Clone, PartialEq)]
pub struct GenericParam {
    /// The parameter identifier.
    pub ident: Ident,
    /// The kind of generic parameter.
    pub kind: GenericParamKind,
    /// Attributes on this parameter.
    pub attrs: Vec<Attribute>,
    /// The span.
    pub span: Span,
}

/// Kind of generic parameter.
#[derive(Debug, Clone, PartialEq)]
pub enum GenericParamKind {
    /// A type parameter: `T` or `T: Trait`.
    Type {
        bounds: Vec<TypeBound>,
        default: Option<Box<Type>>,
    },
    /// A lifetime parameter: `'a` or `'a: 'b`.
    Lifetime { bounds: Vec<Lifetime> },
    /// A const parameter: `const N: usize`.
    Const {
        ty: Box<Type>,
        default: Option<Box<Expr>>,
    },
}

/// A bound on a type like `T: Clone + Debug`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeBound {
    /// The trait path.
    pub path: Path,
    /// Whether this is a `?Sized` style bound.
    pub is_maybe: bool,
    /// The span.
    pub span: Span,
}

/// Where clause.
#[derive(Debug, Clone, PartialEq)]
pub struct WhereClause {
    /// The predicates.
    pub predicates: Vec<WherePredicate>,
    /// The span.
    pub span: Span,
}

/// A predicate in a where clause.
#[derive(Debug, Clone, PartialEq)]
pub struct WherePredicate {
    /// The type being constrained.
    pub ty: Box<Type>,
    /// The bounds.
    pub bounds: Vec<TypeBound>,
    /// The span.
    pub span: Span,
}

/// Generics on an item.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Generics {
    /// Generic parameters.
    pub params: Vec<GenericParam>,
    /// Where clause.
    pub where_clause: Option<WhereClause>,
    /// The span of the `<...>` part.
    pub span: Span,
}

impl Generics {
    /// Create empty generics.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Check if there are no generic parameters.
    pub fn is_empty(&self) -> bool {
        self.params.is_empty() && self.where_clause.is_none()
    }
}
