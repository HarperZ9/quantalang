// ===============================================================================
// QUANTALANG MACRO EXPANSION
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! # Macro Expansion
//!
//! This module implements macro expansion for QuantaLang, including:
//! - Declarative macros (macro_rules!)
//! - Procedural macros
//! - Built-in macros
//!
//! ## Architecture
//!
//! The macro system consists of:
//! - `pattern`: Macro pattern matching and binding
//! - `expand`: Macro expansion engine
//! - `hygiene`: Hygienic macro scoping
//! - `builtins`: Built-in macros (println!, vec!, etc.)
//!
//! ## Example
//!
//! ```rust,ignore
//! use quantalang::macro_expand::{MacroExpander, MacroContext};
//! use quantalang::ast::Expr;
//!
//! let mut ctx = MacroContext::new();
//! let mut expander = MacroExpander::new(&mut ctx);
//! let expanded = expander.expand_expr(&macro_invocation)?;
//! ```

mod pattern;
mod expand;
mod hygiene;
mod builtins;

pub use pattern::*;
pub use expand::*;
pub use hygiene::*;
pub use builtins::*;

use std::collections::HashMap;
use std::sync::Arc;

use crate::lexer::{Token, TokenKind, Span, Delimiter};
use thiserror::Error;

// =============================================================================
// TOKEN TREE
// =============================================================================

/// A token tree represents either a single token or a delimited group of tokens.
#[derive(Debug, Clone)]
pub enum TokenTree {
    /// A single token.
    Token(Token),
    /// A delimited group of token trees: `(...)`, `[...]`, or `{...}`.
    Delimited {
        /// The opening delimiter.
        delimiter: Delimiter,
        /// The span of the opening delimiter.
        open_span: Span,
        /// The contained token trees.
        tokens: Vec<TokenTree>,
        /// The span of the closing delimiter.
        close_span: Span,
    },
}

impl TokenTree {
    /// Get the span of this token tree.
    pub fn span(&self) -> Span {
        match self {
            TokenTree::Token(t) => t.span,
            TokenTree::Delimited { open_span, close_span, .. } => {
                open_span.merge(close_span)
            }
        }
    }

    /// Check if this is a token.
    pub fn is_token(&self) -> bool {
        matches!(self, TokenTree::Token(_))
    }

    /// Check if this is a delimited group.
    pub fn is_delimited(&self) -> bool {
        matches!(self, TokenTree::Delimited { .. })
    }

    /// Get the token if this is a token.
    pub fn as_token(&self) -> Option<&Token> {
        match self {
            TokenTree::Token(t) => Some(t),
            _ => None,
        }
    }

    /// Get the inner tokens if this is a delimited group.
    pub fn as_delimited(&self) -> Option<(&Delimiter, &[TokenTree])> {
        match self {
            TokenTree::Delimited { delimiter, tokens, .. } => Some((delimiter, tokens)),
            _ => None,
        }
    }
}

/// Convert a slice of tokens to a token tree.
pub fn tokens_to_tree(tokens: &[Token]) -> Vec<TokenTree> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i].kind {
            TokenKind::OpenDelim(delim) => {
                let open_span = tokens[i].span;
                let delim = *delim;
                i += 1;

                // Find matching close delimiter
                let (inner, close_idx) = collect_until_close(&tokens[i..], delim);
                let inner_trees = tokens_to_tree(&inner);

                let close_span = if i + close_idx < tokens.len() {
                    tokens[i + close_idx].span
                } else {
                    open_span
                };

                result.push(TokenTree::Delimited {
                    delimiter: delim,
                    open_span,
                    tokens: inner_trees,
                    close_span,
                });

                i += close_idx + 1;
            }
            TokenKind::CloseDelim(_) => {
                // Unmatched close delimiter - skip
                i += 1;
            }
            TokenKind::Eof => {
                // Skip EOF tokens - they shouldn't be part of token trees
                i += 1;
            }
            _ => {
                result.push(TokenTree::Token(tokens[i].clone()));
                i += 1;
            }
        }
    }

    result
}

/// Collect tokens until the matching close delimiter.
fn collect_until_close(tokens: &[Token], open_delim: Delimiter) -> (Vec<Token>, usize) {
    let mut result = Vec::new();
    let mut depth = 1;
    let mut i = 0;

    while i < tokens.len() && depth > 0 {
        match &tokens[i].kind {
            TokenKind::OpenDelim(d) if *d == open_delim => {
                depth += 1;
                result.push(tokens[i].clone());
            }
            TokenKind::CloseDelim(d) if *d == open_delim => {
                depth -= 1;
                if depth > 0 {
                    result.push(tokens[i].clone());
                }
            }
            _ => {
                result.push(tokens[i].clone());
            }
        }
        i += 1;
    }

    (result, i.saturating_sub(1))
}

// =============================================================================
// MACRO DEFINITION
// =============================================================================

/// A unique identifier for a macro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacroId(pub u32);

impl MacroId {
    /// Generate a fresh macro ID.
    pub fn fresh() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        MacroId(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

/// A macro definition.
#[derive(Debug, Clone)]
pub struct MacroDef {
    /// The macro's unique ID.
    pub id: MacroId,
    /// The macro's name.
    pub name: Arc<str>,
    /// The macro rules.
    pub rules: Vec<MacroRule>,
    /// Whether this macro is exported.
    pub is_exported: bool,
    /// The span of the macro definition.
    pub span: Span,
}

/// A single macro rule (pattern => expansion).
#[derive(Debug, Clone)]
pub struct MacroRule {
    /// The pattern to match.
    pub pattern: MacroPattern,
    /// The expansion template.
    pub expansion: MacroExpansion,
    /// The span of this rule.
    pub span: Span,
}

/// A macro pattern.
#[derive(Debug, Clone)]
pub struct MacroPattern {
    /// The pattern elements.
    pub elements: Vec<PatternElement>,
}

/// An element of a macro pattern.
#[derive(Debug, Clone)]
pub enum PatternElement {
    /// A literal token to match.
    Token(TokenKind),
    /// A metavariable: `$name:kind`.
    MetaVar {
        name: Arc<str>,
        kind: MetaVarKind,
    },
    /// A repetition: `$(...)*` or `$(...)+` or `$(...)?`.
    Repetition {
        elements: Vec<PatternElement>,
        separator: Option<TokenKind>,
        repetition: RepetitionKind,
    },
    /// A delimited group.
    Delimited {
        delimiter: Delimiter,
        elements: Vec<PatternElement>,
    },
}

/// The kind of a metavariable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaVarKind {
    /// `$x:expr` - expression
    Expr,
    /// `$x:ty` - type
    Type,
    /// `$x:ident` - identifier
    Ident,
    /// `$x:path` - path
    Path,
    /// `$x:pat` - pattern
    Pat,
    /// `$x:stmt` - statement
    Stmt,
    /// `$x:block` - block
    Block,
    /// `$x:item` - item
    Item,
    /// `$x:meta` - attribute content
    Meta,
    /// `$x:tt` - token tree
    TokenTree,
    /// `$x:literal` - literal
    Literal,
    /// `$x:lifetime` - lifetime
    Lifetime,
    /// `$x:vis` - visibility
    Vis,
}

impl MetaVarKind {
    /// Parse a metavariable kind from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "expr" => Some(Self::Expr),
            "ty" => Some(Self::Type),
            "ident" => Some(Self::Ident),
            "path" => Some(Self::Path),
            "pat" | "pat_param" => Some(Self::Pat),
            "stmt" => Some(Self::Stmt),
            "block" => Some(Self::Block),
            "item" => Some(Self::Item),
            "meta" => Some(Self::Meta),
            "tt" => Some(Self::TokenTree),
            "literal" | "lit" => Some(Self::Literal),
            "lifetime" => Some(Self::Lifetime),
            "vis" => Some(Self::Vis),
            _ => None,
        }
    }
}

/// The kind of repetition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepetitionKind {
    /// `*` - zero or more
    ZeroOrMore,
    /// `+` - one or more
    OneOrMore,
    /// `?` - zero or one
    ZeroOrOne,
}

/// A macro expansion template.
#[derive(Debug, Clone)]
pub struct MacroExpansion {
    /// The expansion elements.
    pub elements: Vec<ExpansionElement>,
}

/// An element of a macro expansion.
#[derive(Debug, Clone)]
pub enum ExpansionElement {
    /// A literal token to emit.
    Token(TokenKind, Span),
    /// A metavariable reference: `$name`.
    MetaVar(Arc<str>),
    /// A repetition: `$(...)*`.
    Repetition {
        elements: Vec<ExpansionElement>,
        separator: Option<TokenKind>,
        repetition: RepetitionKind,
    },
    /// A delimited group.
    Delimited {
        delimiter: Delimiter,
        elements: Vec<ExpansionElement>,
        span: Span,
    },
}

// =============================================================================
// MACRO CONTEXT
// =============================================================================

/// The macro context containing all macro definitions.
#[derive(Debug, Clone, Default)]
pub struct MacroContext {
    /// All macro definitions by ID.
    pub macros: HashMap<MacroId, MacroDef>,
    /// Macro definitions by name.
    pub macro_names: HashMap<Arc<str>, MacroId>,
    /// Scope stack for macro hygiene.
    scopes: Vec<MacroScope>,
}

/// A macro scope for hygiene.
#[derive(Debug, Clone, Default)]
pub struct MacroScope {
    /// Local macro definitions in this scope.
    local_macros: HashMap<Arc<str>, MacroId>,
}

impl MacroContext {
    /// Create a new empty macro context.
    pub fn new() -> Self {
        let mut ctx = Self::default();
        ctx.scopes.push(MacroScope::default());
        ctx
    }

    /// Register a macro definition.
    pub fn register_macro(&mut self, def: MacroDef) {
        let id = def.id;
        let name = def.name.clone();
        self.macros.insert(id, def);
        self.macro_names.insert(name, id);
    }

    /// Look up a macro by name.
    pub fn lookup_macro(&self, name: &str) -> Option<&MacroDef> {
        // Check local scopes first (reverse order)
        for scope in self.scopes.iter().rev() {
            if let Some(id) = scope.local_macros.get(name) {
                return self.macros.get(id);
            }
        }
        // Check global
        self.macro_names.get(name).and_then(|id| self.macros.get(id))
    }

    /// Push a new scope.
    pub fn push_scope(&mut self) {
        self.scopes.push(MacroScope::default());
    }

    /// Pop the current scope.
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Define a local macro in the current scope.
    pub fn define_local(&mut self, name: Arc<str>, id: MacroId) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.local_macros.insert(name, id);
        }
    }
}

// =============================================================================
// MACRO ERRORS
// =============================================================================

/// Errors that can occur during macro expansion.
#[derive(Debug, Clone, Error)]
pub enum MacroError {
    /// Macro not found.
    #[error("macro `{name}` not found")]
    MacroNotFound { name: String },

    /// No matching rule.
    #[error("no rules matched for macro `{name}`")]
    NoMatchingRule { name: String },

    /// Metavariable not found.
    #[error("metavariable `{name}` not found")]
    MetaVarNotFound { name: String },

    /// Invalid metavariable kind.
    #[error("invalid metavariable kind `{kind}`")]
    InvalidMetaVarKind { kind: String },

    /// Repetition mismatch.
    #[error("repetition mismatch: `{name}` does not repeat with `{other}`")]
    RepetitionMismatch { name: String, other: String },

    /// Unexpected token.
    #[error("unexpected token: expected `{expected}`, found `{found:?}`")]
    UnexpectedToken { expected: String, found: TokenKind },

    /// Recursion limit exceeded.
    #[error("macro recursion limit ({limit}) exceeded")]
    RecursionLimit { limit: u32 },

    /// Parse error during expansion.
    #[error("parse error: {message}")]
    ParseError { message: String },
}

/// Result type for macro operations.
pub type MacroResult<T> = Result<T, MacroError>;
