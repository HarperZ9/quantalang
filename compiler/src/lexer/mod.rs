// ===============================================================================
// QUANTALANG LEXER MODULE
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! # Lexical Analysis
//!
//! This module provides the lexer (tokenizer) for QuantaLang. It converts source
//! code into a stream of tokens that can be consumed by the parser.
//!
//! ## Features
//!
//! - Full Unicode identifier support (UAX #31)
//! - All numeric literal formats (decimal, hex, octal, binary, float, scientific)
//! - String literals with escape sequences and Unicode escapes
//! - Raw strings with configurable delimiter count
//! - Byte strings and byte literals
//! - Character literals with escape sequences
//! - Line, block, and documentation comments
//! - Nested block comments
//! - DSL block recognition (sql!, regex!, math!, etc.)
//! - Lifetime annotations
//! - Comprehensive error reporting with spans
//!
//! ## Example
//!
//! ```rust,ignore
//! use quantalang::lexer::{Lexer, SourceFile};
//!
//! let source = SourceFile::new("example.quanta", "let x = 42;");
//! let mut lexer = Lexer::new(&source);
//! let tokens = lexer.tokenize()?;
//!
//! for token in &tokens {
//!     println!("{:?}", token);
//! }
//! ```

mod token;
mod span;
mod cursor;
mod scanner;
mod error;

pub use token::{
    Token, TokenKind, Keyword, Delimiter, LiteralKind, IntBase, InterpolatedPart,
    NumericSuffixKind, NumericSuffixValidation, validate_numeric_suffix,
    is_valid_int_suffix, is_valid_float_suffix, is_integer_only_suffix,
    INTEGER_SUFFIXES, FLOAT_SUFFIXES, NUMERIC_SUFFIXES,
    DocComment, DocCommentKind, DocComments,
};
pub use span::{Span, Position, SourceFile, SourceId, BytePos};
pub use scanner::Lexer;
pub use error::{LexerError, LexerErrorKind, LexerResult};

/// Convenience function to tokenize source code
pub fn tokenize(source: &str) -> LexerResult<Vec<Token>> {
    let file = SourceFile::anonymous(source);
    let mut lexer = Lexer::new(&file);
    lexer.tokenize()
}

/// Convenience function to tokenize source code from a file
pub fn tokenize_file(filename: &str, source: &str) -> LexerResult<Vec<Token>> {
    let file = SourceFile::new(filename, source);
    let mut lexer = Lexer::new(&file);
    lexer.tokenize()
}

/// Convenience function to tokenize source code and extract doc comments
pub fn tokenize_with_docs(source: &str) -> LexerResult<(Vec<Token>, DocComments)> {
    let file = SourceFile::anonymous(source);
    let mut lexer = Lexer::new(&file);
    lexer.tokenize_with_docs()
}

/// Convenience function to tokenize source code from a file and extract doc comments
pub fn tokenize_file_with_docs(filename: &str, source: &str) -> LexerResult<(Vec<Token>, DocComments)> {
    let file = SourceFile::new(filename, source);
    let mut lexer = Lexer::new(&file);
    lexer.tokenize_with_docs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokenization() {
        let tokens = tokenize("let x = 42").unwrap();
        assert!(tokens.len() >= 4); // let, x, =, 42, EOF
    }
}
