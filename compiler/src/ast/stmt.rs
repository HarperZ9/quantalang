// ===============================================================================
// QUANTALANG AST - STATEMENTS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Statement AST nodes.
//!
//! Statements are the building blocks of imperative code.

use crate::lexer::Span;
use super::{Attribute, Expr, Item, NodeId, Pattern, Type};

/// A statement node.
#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    /// The kind of statement.
    pub kind: StmtKind,
    /// The span of this statement.
    pub span: Span,
    /// Node ID.
    pub id: NodeId,
}

impl Stmt {
    /// Create a new statement.
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Self {
            kind,
            span,
            id: NodeId::DUMMY,
        }
    }

    /// Create a let statement.
    pub fn local(local: Local) -> Self {
        let span = local.span;
        Self::new(StmtKind::Local(Box::new(local)), span)
    }

    /// Create an expression statement (with semicolon).
    pub fn expr(expr: Expr) -> Self {
        let span = expr.span;
        Self::new(StmtKind::Expr(Box::new(expr)), span)
    }

    /// Create a semi statement (expression followed by semicolon).
    pub fn semi(expr: Expr) -> Self {
        let span = expr.span;
        Self::new(StmtKind::Semi(Box::new(expr)), span)
    }

    /// Create an item statement.
    pub fn item(item: Item) -> Self {
        let span = item.span;
        Self::new(StmtKind::Item(Box::new(item)), span)
    }

    /// Create an empty statement (just a semicolon).
    pub fn empty(span: Span) -> Self {
        Self::new(StmtKind::Empty, span)
    }

    /// Check if this statement has a trailing semicolon.
    pub fn has_semi(&self) -> bool {
        matches!(
            self.kind,
            StmtKind::Local(_) | StmtKind::Semi(_) | StmtKind::Empty
        )
    }
}

/// The kind of statement.
#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// A local variable binding: `let x = 1;`
    Local(Box<Local>),

    /// An expression without trailing semicolon (used as block value).
    Expr(Box<Expr>),

    /// An expression with trailing semicolon.
    Semi(Box<Expr>),

    /// An item declaration.
    Item(Box<Item>),

    /// An empty statement (just `;`).
    Empty,

    /// A macro invocation statement.
    Macro {
        path: super::Path,
        tokens: Vec<super::TokenTree>,
        is_semi: bool,
    },
}

/// A local variable binding (`let` statement).
#[derive(Debug, Clone, PartialEq)]
pub struct Local {
    /// Attributes on this local.
    pub attrs: Vec<Attribute>,
    /// The pattern being bound.
    pub pattern: Pattern,
    /// Optional type annotation.
    pub ty: Option<Box<Type>>,
    /// Optional initializer expression.
    pub init: Option<LocalInit>,
    /// Span.
    pub span: Span,
    /// Node ID.
    pub id: NodeId,
}

impl Local {
    /// Create a new local binding.
    pub fn new(
        pattern: Pattern,
        ty: Option<Box<Type>>,
        init: Option<LocalInit>,
        span: Span,
    ) -> Self {
        Self {
            attrs: Vec::new(),
            pattern,
            ty,
            init,
            span,
            id: NodeId::DUMMY,
        }
    }

    /// Create with attributes.
    pub fn with_attrs(
        attrs: Vec<Attribute>,
        pattern: Pattern,
        ty: Option<Box<Type>>,
        init: Option<LocalInit>,
        span: Span,
    ) -> Self {
        Self {
            attrs,
            pattern,
            ty,
            init,
            span,
            id: NodeId::DUMMY,
        }
    }
}

/// Initializer for a local binding.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalInit {
    /// The initializer expression.
    pub expr: Box<Expr>,
    /// Optional diverging expression (`else` branch for `let-else`).
    pub diverge: Option<Box<Expr>>,
}

impl LocalInit {
    /// Create a simple initializer.
    pub fn simple(expr: Expr) -> Self {
        Self {
            expr: Box::new(expr),
            diverge: None,
        }
    }

    /// Create a let-else initializer.
    pub fn with_else(expr: Expr, diverge: Expr) -> Self {
        Self {
            expr: Box::new(expr),
            diverge: Some(Box::new(diverge)),
        }
    }
}
