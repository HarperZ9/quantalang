// ===============================================================================
// QUANTALANG LEXER - ERROR TYPES
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Error types for the lexer.
//!
//! This module provides comprehensive error types with detailed information
//! for producing high-quality error messages.

use std::fmt;
use thiserror::Error;
use super::span::Span;

/// Result type for lexer operations.
pub type LexerResult<T> = Result<T, LexerError>;

/// A lexer error with location information.
#[derive(Debug, Clone, Error)]
pub struct LexerError {
    /// The kind of error.
    pub kind: LexerErrorKind,
    /// The span where the error occurred.
    pub span: Span,
    /// Optional help message.
    pub help: Option<String>,
}

impl LexerError {
    /// Create a new lexer error.
    pub fn new(kind: LexerErrorKind, span: Span) -> Self {
        Self {
            kind,
            span,
            help: None,
        }
    }

    /// Add a help message to the error.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Get the error message.
    pub fn message(&self) -> String {
        self.kind.to_string()
    }
}

impl fmt::Display for LexerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(help) = &self.help {
            write!(f, "\n  help: {}", help)?;
        }
        Ok(())
    }
}

/// The kind of lexer error.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LexerErrorKind {
    // =========================================================================
    // CHARACTER ERRORS
    // =========================================================================

    /// An unexpected character was encountered.
    #[error("unexpected character `{0}`")]
    UnexpectedChar(char),

    /// An unexpected EOF was encountered.
    #[error("unexpected end of file")]
    UnexpectedEof,

    // =========================================================================
    // STRING ERRORS
    // =========================================================================

    /// A string literal was not terminated.
    #[error("unterminated string literal")]
    UnterminatedString,

    /// A character literal was not terminated.
    #[error("unterminated character literal")]
    UnterminatedChar,

    /// A byte string literal was not terminated.
    #[error("unterminated byte string literal")]
    UnterminatedByteString,

    /// A raw string literal was not terminated.
    #[error("unterminated raw string literal")]
    UnterminatedRawString {
        /// The number of `#` symbols expected.
        expected_hashes: u8,
    },

    /// A character literal is empty.
    #[error("empty character literal")]
    EmptyCharLiteral,

    /// A character literal contains multiple characters.
    #[error("character literal may only contain one codepoint")]
    MultipleCharsInCharLiteral,

    // =========================================================================
    // ESCAPE SEQUENCE ERRORS
    // =========================================================================

    /// An unknown escape sequence was found.
    #[error("unknown escape sequence `\\{0}`")]
    UnknownEscape(char),

    /// An invalid Unicode escape sequence.
    #[error("invalid Unicode escape sequence")]
    InvalidUnicodeEscape,

    /// Unicode escape is missing opening brace.
    #[error("Unicode escape sequence missing opening brace")]
    UnicodeEscapeMissingBrace,

    /// Unicode escape is missing closing brace.
    #[error("Unicode escape sequence missing closing brace")]
    UnicodeEscapeUnclosed,

    /// Unicode escape has too many digits.
    #[error("Unicode escape sequence has too many digits (max 6)")]
    UnicodeEscapeTooLong,

    /// Unicode escape value is not a valid codepoint.
    #[error("invalid Unicode codepoint `{0:#X}`")]
    InvalidUnicodeCodepoint(u32),

    /// An invalid hex escape sequence.
    #[error("invalid hex escape sequence")]
    InvalidHexEscape,

    /// Hex escape is out of range for a byte.
    #[error("hex escape value `{0:#X}` is out of range (max 0x7F for characters, 0xFF for bytes)")]
    HexEscapeOutOfRange(u32),

    /// An escape character at EOF.
    #[error("escape character at end of file")]
    EscapeAtEof,

    // =========================================================================
    // NUMBER ERRORS
    // =========================================================================

    /// An invalid numeric literal.
    #[error("invalid numeric literal")]
    InvalidNumber,

    /// Integer literal is too large.
    #[error("integer literal is too large")]
    IntegerOverflow,

    /// Float literal is too large or too small.
    #[error("float literal is out of range")]
    FloatOverflow,

    /// No digits after the exponent marker.
    #[error("expected digits after exponent marker")]
    EmptyExponent,

    /// No digits after radix prefix.
    #[error("expected digits after `{0}` prefix")]
    NoDigitsAfterPrefix(String),

    /// Invalid digit for the given radix.
    #[error("invalid digit `{0}` for base {1}")]
    InvalidDigit(char, u32),

    /// Float literal has unsupported base.
    #[error("float literals cannot use base {0}")]
    FloatWithBase(u32),

    /// Invalid numeric suffix.
    #[error("invalid suffix `{0}` for numeric literal")]
    InvalidNumericSuffix(String),

    /// Integer suffix used on float literal.
    #[error("integer suffix `{0}` cannot be used with float literals")]
    IntSuffixOnFloat(String),

    /// Float suffix used on integer literal with non-decimal base.
    #[error("float suffix `{0}` cannot be used with non-decimal integers")]
    FloatSuffixOnNonDecimal(String),

    /// Underscore not allowed at start of number.
    #[error("numeric literal cannot start with underscore")]
    LeadingUnderscore,

    /// Underscore not allowed at end of number.
    #[error("numeric literal cannot end with underscore")]
    TrailingUnderscore,

    /// Multiple underscores in a row.
    #[error("consecutive underscores in numeric literal")]
    ConsecutiveUnderscores,

    // =========================================================================
    // COMMENT ERRORS
    // =========================================================================

    /// A block comment was not terminated.
    #[error("unterminated block comment")]
    UnterminatedBlockComment {
        /// The nesting depth when EOF was reached.
        depth: u32,
    },

    /// A documentation comment was not properly formed.
    #[error("invalid documentation comment")]
    InvalidDocComment,

    // =========================================================================
    // IDENTIFIER ERRORS
    // =========================================================================

    /// Invalid identifier character.
    #[error("invalid character `{0}` in identifier")]
    InvalidIdentChar(char),

    /// Raw identifier syntax error.
    #[error("expected identifier after `r#`")]
    ExpectedRawIdent,

    /// Cannot use keyword as raw identifier.
    #[error("`{0}` cannot be used as a raw identifier")]
    CannotBeRawIdent(String),

    // =========================================================================
    // LIFETIME ERRORS
    // =========================================================================

    /// Lifetime must be followed by identifier.
    #[error("expected lifetime name after `'`")]
    ExpectedLifetime,

    /// Invalid lifetime name.
    #[error("invalid lifetime name")]
    InvalidLifetime,

    // =========================================================================
    // DSL ERRORS
    // =========================================================================

    /// DSL block delimiter mismatch.
    #[error("mismatched DSL block delimiters")]
    MismatchedDslDelimiters,

    /// DSL block not terminated.
    #[error("unterminated DSL block")]
    UnterminatedDslBlock,

    // =========================================================================
    // FORMAT STRING ERRORS
    // =========================================================================

    /// Format string was not terminated.
    #[error("unterminated format string literal")]
    UnterminatedFormatString,

    /// Unclosed interpolation brace in format string.
    #[error("unclosed `{{` in format string")]
    UnclosedInterpolation,

    /// Empty interpolation expression in format string.
    #[error("empty interpolation expression in format string")]
    EmptyInterpolation,

    /// Nested braces too deep in format string.
    #[error("interpolation nesting too deep (max depth: {0})")]
    InterpolationTooDeep(u32),

    /// Invalid character in format specifier.
    #[error("invalid format specifier `{0}`")]
    InvalidFormatSpecifier(String),

    // =========================================================================
    // SHEBANG ERRORS
    // =========================================================================

    /// Shebang not at start of file.
    #[error("shebang must be at the start of the file")]
    ShebangNotAtStart,

    // =========================================================================
    // ENCODING ERRORS
    // =========================================================================

    /// Invalid UTF-8 sequence.
    #[error("invalid UTF-8 sequence")]
    InvalidUtf8,

    /// NUL character in source.
    #[error("NUL character not allowed in source")]
    NulInSource,

    /// Non-ASCII character in string that requires ASCII.
    #[error("non-ASCII character in byte literal")]
    NonAsciiInByteLiteral,

    // =========================================================================
    // INTERNAL ERRORS
    // =========================================================================

    /// Internal lexer error (should not occur).
    #[error("internal lexer error: {0}")]
    Internal(String),
}

impl LexerErrorKind {
    /// Get a help message for this error kind.
    pub fn help(&self) -> Option<&'static str> {
        match self {
            LexerErrorKind::UnterminatedString => {
                Some("string literals must end with a closing `\"`")
            }
            LexerErrorKind::UnterminatedChar => {
                Some("character literals must end with a closing `'`")
            }
            LexerErrorKind::EmptyCharLiteral => {
                Some("use `'\\0'` for a NUL character or `\"\"` for an empty string")
            }
            LexerErrorKind::MultipleCharsInCharLiteral => {
                Some("consider using a string literal instead")
            }
            LexerErrorKind::UnknownEscape(_) => {
                Some("valid escape sequences: \\n, \\r, \\t, \\\\, \\', \\\", \\0, \\xNN, \\u{NNNN}")
            }
            LexerErrorKind::UnicodeEscapeMissingBrace => {
                Some("Unicode escapes use the format \\u{NNNN}")
            }
            LexerErrorKind::EmptyExponent => {
                Some("add digits after the exponent marker (e.g., `1e10` or `1e-5`)")
            }
            LexerErrorKind::NoDigitsAfterPrefix(_) => {
                Some("add digits after the radix prefix")
            }
            LexerErrorKind::LeadingUnderscore => {
                Some("remove the leading underscore or use an identifier")
            }
            LexerErrorKind::ConsecutiveUnderscores => {
                Some("use only a single underscore as a separator")
            }
            LexerErrorKind::NonAsciiInByteLiteral => {
                Some("use a \\xNN escape sequence for non-ASCII bytes")
            }
            _ => None,
        }
    }

    /// Check if this error is recoverable.
    pub fn is_recoverable(&self) -> bool {
        !matches!(
            self,
            LexerErrorKind::InvalidUtf8 | LexerErrorKind::Internal(_)
        )
    }
}

/// Collection of lexer errors.
#[derive(Debug, Clone, Default)]
pub struct LexerErrors {
    errors: Vec<LexerError>,
}

impl LexerErrors {
    /// Create an empty error collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an error to the collection.
    pub fn push(&mut self, error: LexerError) {
        self.errors.push(error);
    }

    /// Create and add an error.
    pub fn emit(&mut self, kind: LexerErrorKind, span: Span) {
        self.push(LexerError::new(kind, span));
    }

    /// Check if there are any errors.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Get the number of errors.
    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// Get the errors as a slice.
    pub fn errors(&self) -> &[LexerError] {
        &self.errors
    }

    /// Take ownership of the errors.
    pub fn into_errors(self) -> Vec<LexerError> {
        self.errors
    }

    /// Iterate over the errors.
    pub fn iter(&self) -> impl Iterator<Item = &LexerError> {
        self.errors.iter()
    }
}

impl IntoIterator for LexerErrors {
    type Item = LexerError;
    type IntoIter = std::vec::IntoIter<LexerError>;

    fn into_iter(self) -> Self::IntoIter {
        self.errors.into_iter()
    }
}

impl<'a> IntoIterator for &'a LexerErrors {
    type Item = &'a LexerError;
    type IntoIter = std::slice::Iter<'a, LexerError>;

    fn into_iter(self) -> Self::IntoIter {
        self.errors.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = LexerError::new(
            LexerErrorKind::UnexpectedChar('$'),
            Span::dummy(),
        );
        assert!(err.to_string().contains("unexpected character"));
    }

    #[test]
    fn test_error_with_help() {
        let err = LexerError::new(
            LexerErrorKind::UnterminatedString,
            Span::dummy(),
        )
        .with_help("add a closing quote");

        assert!(err.help.is_some());
        assert!(err.to_string().contains("help"));
    }

    #[test]
    fn test_error_collection() {
        let mut errors = LexerErrors::new();
        assert!(errors.is_empty());

        errors.emit(LexerErrorKind::UnexpectedChar('$'), Span::dummy());
        errors.emit(LexerErrorKind::UnterminatedString, Span::dummy());

        assert_eq!(errors.len(), 2);
    }
}
