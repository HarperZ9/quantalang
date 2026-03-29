// ===============================================================================
// QUANTALANG PARSER - ERROR TYPES
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Error types for the parser.

use crate::lexer::Span;
use std::fmt;
use thiserror::Error;

/// Result type for parser operations.
pub type ParseResult<T> = Result<T, ParseError>;

/// A parser error with location information.
#[derive(Debug, Clone, Error)]
pub struct ParseError {
    /// The kind of error.
    pub kind: ParseErrorKind,
    /// The span where the error occurred.
    pub span: Span,
    /// Optional help message.
    pub help: Option<String>,
    /// Optional notes.
    pub notes: Vec<String>,
}

impl ParseError {
    /// Create a new parser error.
    pub fn new(kind: ParseErrorKind, span: Span) -> Self {
        Self {
            kind,
            span,
            help: None,
            notes: Vec::new(),
        }
    }

    /// Add a help message.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Add a note.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Get the error message.
    pub fn message(&self) -> String {
        self.kind.to_string()
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(help) = &self.help {
            write!(f, "\n  help: {}", help)?;
        }
        for note in &self.notes {
            write!(f, "\n  note: {}", note)?;
        }
        Ok(())
    }
}

/// The kind of parser error.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseErrorKind {
    // =========================================================================
    // TOKEN ERRORS
    // =========================================================================
    /// Unexpected token.
    #[error("unexpected token: {found}")]
    UnexpectedToken { found: String },

    /// Expected a specific token.
    #[error("expected {expected}, found {found}")]
    Expected { expected: String, found: String },

    /// Unexpected end of file.
    #[error("unexpected end of file")]
    UnexpectedEof,

    // =========================================================================
    // EXPRESSION ERRORS
    // =========================================================================
    /// Invalid expression.
    #[error("invalid expression")]
    InvalidExpression,

    /// Invalid left-hand side of assignment.
    #[error("invalid left-hand side of assignment")]
    InvalidAssignTarget,

    /// Missing operand.
    #[error("expected operand")]
    MissingOperand,

    /// Unclosed delimiter.
    #[error("unclosed {delimiter}")]
    UnclosedDelimiter { delimiter: String },

    /// Mismatched delimiter.
    #[error("mismatched {expected}, found {found}")]
    MismatchedDelimiter { expected: String, found: String },

    // =========================================================================
    // STATEMENT ERRORS
    // =========================================================================
    /// Invalid statement.
    #[error("invalid statement")]
    InvalidStatement,

    /// Expected semicolon.
    #[error("expected `;`")]
    ExpectedSemicolon,

    // =========================================================================
    // ITEM ERRORS
    // =========================================================================
    /// Invalid item.
    #[error("invalid item declaration")]
    InvalidItem,

    /// Duplicate modifier.
    #[error("duplicate `{modifier}` modifier")]
    DuplicateModifier { modifier: String },

    /// Conflicting modifiers.
    #[error("`{first}` and `{second}` are mutually exclusive")]
    ConflictingModifiers { first: String, second: String },

    // =========================================================================
    // TYPE ERRORS
    // =========================================================================
    /// Invalid type.
    #[error("invalid type")]
    InvalidType,

    /// Expected type.
    #[error("expected type")]
    ExpectedType,

    // =========================================================================
    // PATTERN ERRORS
    // =========================================================================
    /// Invalid pattern.
    #[error("invalid pattern")]
    InvalidPattern,

    /// Expected pattern.
    #[error("expected pattern")]
    ExpectedPattern,

    /// Rest pattern must be last.
    #[error("`..` must be at the end of the pattern")]
    RestPatternNotLast,

    // =========================================================================
    // GENERIC ERRORS
    // =========================================================================
    /// Invalid generic parameter.
    #[error("invalid generic parameter")]
    InvalidGenericParam,

    /// Invalid where clause.
    #[error("invalid where clause")]
    InvalidWhereClause,

    // =========================================================================
    // ATTRIBUTE ERRORS
    // =========================================================================
    /// Invalid attribute.
    #[error("invalid attribute")]
    InvalidAttribute,

    /// Inner attribute not allowed here.
    #[error("inner attribute not allowed in this context")]
    InnerAttributeNotAllowed,

    // =========================================================================
    // MACRO ERRORS
    // =========================================================================
    /// Invalid macro invocation.
    #[error("invalid macro invocation")]
    InvalidMacroInvocation,

    // =========================================================================
    // LEXER ERRORS
    // =========================================================================
    /// Lexer error.
    #[error("lexer error: {0}")]
    LexerError(String),

    // =========================================================================
    // INTERNAL ERRORS
    // =========================================================================
    /// Internal parser error.
    #[error("internal parser error: {0}")]
    Internal(String),
}

impl ParseErrorKind {
    /// Get a suggested fix for this error.
    pub fn suggestion(&self) -> Option<&'static str> {
        match self {
            ParseErrorKind::ExpectedSemicolon => Some("add a `;` at the end of the statement"),
            ParseErrorKind::UnclosedDelimiter { .. } => Some("add the missing closing delimiter"),
            ParseErrorKind::InvalidAssignTarget => {
                Some("only variables, fields, and dereferences can be assigned to")
            }
            _ => None,
        }
    }

    /// Check if this error is recoverable.
    pub fn is_recoverable(&self) -> bool {
        !matches!(
            self,
            ParseErrorKind::UnexpectedEof | ParseErrorKind::Internal(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = ParseError::new(
            ParseErrorKind::Expected {
                expected: "identifier".to_string(),
                found: "`+`".to_string(),
            },
            Span::dummy(),
        );
        assert!(err.to_string().contains("expected"));
        assert!(err.to_string().contains("identifier"));
    }

    #[test]
    fn test_error_with_help() {
        let err = ParseError::new(ParseErrorKind::InvalidAssignTarget, Span::dummy())
            .with_help("try using a variable name");

        assert!(err.help.is_some());
    }
}
