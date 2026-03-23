// ===============================================================================
// QUANTALANG LEXER - SCANNER IMPLEMENTATION
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Main lexer implementation.
//!
//! This module contains the `Lexer` struct which tokenizes QuantaLang source code
//! into a stream of tokens.

use super::cursor::{
    is_binary_digit, is_digit, is_hex_digit, is_id_continue, is_id_start, is_octal_digit,
    is_whitespace, Cursor, EOF_CHAR,
};
use super::error::{LexerError, LexerErrorKind, LexerErrors, LexerResult};
use super::span::{BytePos, SourceFile, SourceId, Span};
use super::token::{
    is_dsl_name, is_integer_only_suffix, is_valid_float_suffix, is_valid_int_suffix,
    validate_numeric_suffix, Delimiter, DocComment, DocCommentKind, DocComments,
    IntBase, InterpolatedPart, Keyword, LiteralKind, NumericSuffixKind,
    NumericSuffixValidation, Token, TokenKind,
};

/// Configuration options for the lexer.
#[derive(Debug, Clone, Default)]
pub struct LexerConfig {
    /// Whether to preserve whitespace tokens.
    pub preserve_whitespace: bool,
    /// Whether to preserve comment tokens.
    pub preserve_comments: bool,
    /// Whether to allow shebang at the start of the file.
    pub allow_shebang: bool,
}

/// The main lexer struct.
pub struct Lexer<'a> {
    /// The source file being lexed.
    source: &'a SourceFile,
    /// The character cursor.
    cursor: Cursor<'a>,
    /// Configuration options.
    config: LexerConfig,
    /// The start position of the current token.
    token_start: BytePos,
    /// Accumulated errors.
    errors: LexerErrors,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given source file.
    pub fn new(source: &'a SourceFile) -> Self {
        Self {
            source,
            cursor: Cursor::new(source.source(), source.id),
            config: LexerConfig::default(),
            token_start: BytePos(0),
            errors: LexerErrors::new(),
        }
    }

    /// Create a new lexer with custom configuration.
    pub fn with_config(source: &'a SourceFile, config: LexerConfig) -> Self {
        Self {
            source,
            cursor: Cursor::new(source.source(), source.id),
            config,
            token_start: BytePos(0),
            errors: LexerErrors::new(),
        }
    }

    /// Get the source file.
    #[inline]
    pub fn source(&self) -> &'a SourceFile {
        self.source
    }

    /// Tokenize the entire source code.
    pub fn tokenize(&mut self) -> LexerResult<Vec<Token>> {
        let mut tokens = Vec::new();

        // Handle optional shebang
        if self.config.allow_shebang && self.cursor.first() == '#' && self.cursor.second() == '!' {
            self.skip_shebang();
        }

        loop {
            match self.next_token() {
                Ok(token) => {
                    let is_eof = token.is_eof();
                    tokens.push(token);
                    if is_eof {
                        break;
                    }
                }
                Err(err) => {
                    self.errors.push(err);
                    // Try to recover by skipping the problematic character
                    if !self.cursor.is_eof() {
                        self.cursor.bump();
                    } else {
                        break;
                    }
                }
            }
        }

        if self.errors.is_empty() {
            Ok(tokens)
        } else {
            // Return first error; full error list is available via errors()
            Err(self.errors.errors()[0].clone())
        }
    }

    /// Get accumulated errors.
    pub fn errors(&self) -> &LexerErrors {
        &self.errors
    }

    /// Tokenize and extract documentation comments.
    /// Returns both tokens and doc comments.
    pub fn tokenize_with_docs(&mut self) -> LexerResult<(Vec<Token>, DocComments)> {
        // Create a new lexer config that preserves comments
        let original_config = self.config.clone();
        self.config.preserve_comments = true;

        let mut tokens = Vec::new();
        let mut doc_comments = DocComments::new();

        // Handle optional shebang
        if self.config.allow_shebang && self.cursor.first() == '#' && self.cursor.second() == '!' {
            self.skip_shebang();
        }

        loop {
            match self.next_token() {
                Ok(token) => {
                    let is_eof = token.is_eof();

                    // Extract doc comment if this is one
                    if let TokenKind::Comment { is_doc: true, is_inner } = token.kind {
                        if let Some(doc) = self.extract_doc_comment(&token, is_inner) {
                            doc_comments.push(doc);
                        }
                    } else if !matches!(token.kind, TokenKind::Comment { .. }) {
                        // Only add non-comment tokens (keep doc comments separate)
                        tokens.push(token);
                    }

                    if is_eof {
                        break;
                    }
                }
                Err(err) => {
                    self.errors.push(err);
                    if !self.cursor.is_eof() {
                        self.cursor.bump();
                    } else {
                        break;
                    }
                }
            }
        }

        // Restore original config
        self.config = original_config;

        if self.errors.is_empty() {
            Ok((tokens, doc_comments))
        } else {
            Err(self.errors.errors()[0].clone())
        }
    }

    /// Extract a DocComment from a comment token.
    fn extract_doc_comment(&self, token: &Token, is_inner: bool) -> Option<DocComment> {
        let source = self.source.source();
        let start = token.span.start.to_usize();
        let end = token.span.end.to_usize();

        if start >= source.len() || end > source.len() {
            return None;
        }

        let text = &source[start..end];

        // Determine comment kind and extract content
        let (kind, content) = if text.starts_with("///") {
            let content = text.strip_prefix("///").unwrap_or(text);
            let content = content.strip_prefix(' ').unwrap_or(content);
            (DocCommentKind::OuterLine, content.trim_end().to_string())
        } else if text.starts_with("//!") {
            let content = text.strip_prefix("//!").unwrap_or(text);
            let content = content.strip_prefix(' ').unwrap_or(content);
            (DocCommentKind::InnerLine, content.trim_end().to_string())
        } else if text.starts_with("/**") && !text.starts_with("/***") {
            // Block doc comment (outer)
            let content = text
                .strip_prefix("/**")
                .and_then(|s| s.strip_suffix("*/"))
                .unwrap_or(text);
            (DocCommentKind::OuterBlock, self.clean_block_doc_content(content))
        } else if text.starts_with("/*!") {
            // Block doc comment (inner)
            let content = text
                .strip_prefix("/*!")
                .and_then(|s| s.strip_suffix("*/"))
                .unwrap_or(text);
            (DocCommentKind::InnerBlock, self.clean_block_doc_content(content))
        } else {
            return None;
        };

        Some(DocComment::new(kind, content, token.span))
    }

    /// Clean up block doc comment content by removing leading asterisks and normalizing indentation.
    fn clean_block_doc_content(&self, content: &str) -> String {
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() {
            return String::new();
        }

        // Find minimum indentation (ignoring empty lines)
        let mut min_indent = usize::MAX;
        for line in &lines {
            let trimmed = line.trim_start();
            if !trimmed.is_empty() {
                let indent = line.len() - trimmed.len();
                // Also account for leading asterisk
                let effective_start = if trimmed.starts_with('*') {
                    indent + 1 + if trimmed.len() > 1 && trimmed.chars().nth(1) == Some(' ') { 1 } else { 0 }
                } else {
                    indent
                };
                if effective_start < min_indent {
                    min_indent = effective_start;
                }
            }
        }

        if min_indent == usize::MAX {
            min_indent = 0;
        }

        // Process each line
        let cleaned: Vec<String> = lines
            .iter()
            .map(|line| {
                let trimmed = line.trim_start();
                if trimmed.is_empty() {
                    String::new()
                } else if trimmed.starts_with('*') {
                    // Remove leading asterisk and optional space
                    let after_star = trimmed.strip_prefix('*').unwrap_or(trimmed);
                    after_star.strip_prefix(' ').unwrap_or(after_star).to_string()
                } else {
                    // Just remove the common indentation
                    if line.len() >= min_indent {
                        line[min_indent..].to_string()
                    } else {
                        line.trim().to_string()
                    }
                }
            })
            .collect();

        // Remove leading/trailing empty lines
        let mut start = 0;
        let mut end = cleaned.len();

        while start < end && cleaned[start].is_empty() {
            start += 1;
        }
        while end > start && cleaned[end - 1].is_empty() {
            end -= 1;
        }

        cleaned[start..end].join("\n")
    }

    /// Scan the next token from the source.
    pub fn next_token(&mut self) -> LexerResult<Token> {
        // Skip whitespace and comments (unless preserving them)
        if !self.config.preserve_whitespace && !self.config.preserve_comments {
            self.skip_trivia();
        } else if !self.config.preserve_whitespace {
            // When preserving comments but not whitespace, still skip whitespace
            while is_whitespace(self.cursor.first()) {
                self.cursor.bump();
            }
        }

        // Mark token start
        self.token_start = self.cursor.pos();

        // Check for EOF
        if self.cursor.is_eof() {
            return Ok(self.make_token(TokenKind::Eof));
        }

        // Handle whitespace if preserving
        if self.config.preserve_whitespace && is_whitespace(self.cursor.first()) {
            return Ok(self.scan_whitespace());
        }

        // Handle comments if preserving
        if self.config.preserve_comments {
            if self.cursor.first() == '/' {
                match self.cursor.second() {
                    '/' => return self.scan_line_comment(),
                    '*' => return self.scan_block_comment(),
                    _ => {}
                }
            }
        }

        // Scan the next token
        let c = self.cursor.bump_or_eof();
        self.scan_token(c)
    }

    /// Scan a token starting with the given character.
    fn scan_token(&mut self, c: char) -> LexerResult<Token> {
        let kind = match c {
            // Single-character tokens
            '(' => TokenKind::OpenDelim(Delimiter::Paren),
            ')' => TokenKind::CloseDelim(Delimiter::Paren),
            '[' => TokenKind::OpenDelim(Delimiter::Bracket),
            ']' => TokenKind::CloseDelim(Delimiter::Bracket),
            '{' => TokenKind::OpenDelim(Delimiter::Brace),
            '}' => TokenKind::CloseDelim(Delimiter::Brace),
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semi,
            '~' => TokenKind::Tilde,
            '?' => TokenKind::Question,
            '@' => TokenKind::At,
            '#' => TokenKind::Pound,
            '$' => TokenKind::Dollar,

            // Multi-character operators
            '+' => self.scan_plus(),
            '-' => self.scan_minus(),
            '*' => self.scan_star(),
            '/' => self.scan_slash()?,
            '%' => self.scan_percent(),
            '^' => self.scan_caret(),
            '&' => self.scan_and(),
            '|' => self.scan_pipe(),
            '!' => self.scan_bang(),
            '=' => self.scan_eq(),
            '<' => self.scan_lt(),
            '>' => self.scan_gt(),
            ':' => self.scan_colon(),
            '.' => self.scan_dot()?,

            // String and character literals
            '"' => self.scan_string()?,
            '\'' => self.scan_char_or_lifetime()?,

            // Numbers
            '0'..='9' => return self.scan_number(c),

            // Identifiers (with raw string/byte string handling)
            'r' if self.cursor.first() == '#' || self.cursor.first() == '"' => {
                return self.scan_raw_string_or_ident();
            }
            'b' if self.cursor.first() == '"' || self.cursor.first() == '\'' || self.cursor.first() == 'r' => {
                return self.scan_byte_string_or_ident();
            }
            'c' if self.cursor.first() == '"' => {
                return self.scan_c_string();
            }
            // Format string (f"...")
            'f' if self.cursor.first() == '"' => {
                return self.scan_format_string();
            }

            // Regular identifiers
            c if is_id_start(c) => return self.scan_identifier(c),

            // Unknown character
            c => {
                return Err(LexerError::new(
                    LexerErrorKind::UnexpectedChar(c),
                    self.current_span(),
                ));
            }
        };

        Ok(self.make_token(kind))
    }

    // =========================================================================
    // OPERATOR SCANNING
    // =========================================================================

    fn scan_plus(&mut self) -> TokenKind {
        if self.cursor.eat('=') {
            TokenKind::PlusEq
        } else {
            TokenKind::Plus
        }
    }

    fn scan_minus(&mut self) -> TokenKind {
        if self.cursor.eat('=') {
            TokenKind::MinusEq
        } else if self.cursor.eat('>') {
            TokenKind::Arrow
        } else {
            TokenKind::Minus
        }
    }

    fn scan_star(&mut self) -> TokenKind {
        if self.cursor.eat('=') {
            TokenKind::StarEq
        } else {
            TokenKind::Star
        }
    }

    fn scan_slash(&mut self) -> LexerResult<TokenKind> {
        if self.cursor.eat('=') {
            Ok(TokenKind::SlashEq)
        } else if self.cursor.first() == '/' {
            // Line comment - should have been skipped in trivia
            self.skip_line_comment();
            self.next_token().map(|t| t.kind)
        } else if self.cursor.first() == '*' {
            // Block comment - should have been skipped in trivia
            self.skip_block_comment()?;
            self.next_token().map(|t| t.kind)
        } else {
            Ok(TokenKind::Slash)
        }
    }

    fn scan_percent(&mut self) -> TokenKind {
        if self.cursor.eat('=') {
            TokenKind::PercentEq
        } else {
            TokenKind::Percent
        }
    }

    fn scan_caret(&mut self) -> TokenKind {
        if self.cursor.eat('=') {
            TokenKind::CaretEq
        } else {
            TokenKind::Caret
        }
    }

    fn scan_and(&mut self) -> TokenKind {
        if self.cursor.eat('&') {
            TokenKind::AndAnd
        } else if self.cursor.eat('=') {
            TokenKind::AndEq
        } else {
            TokenKind::And
        }
    }

    fn scan_pipe(&mut self) -> TokenKind {
        if self.cursor.eat('|') {
            TokenKind::OrOr
        } else if self.cursor.eat('=') {
            TokenKind::OrEq
        } else if self.cursor.eat('>') {
            TokenKind::Pipe
        } else {
            TokenKind::Or
        }
    }

    fn scan_bang(&mut self) -> TokenKind {
        if self.cursor.eat('=') {
            TokenKind::Ne
        } else {
            TokenKind::Not
        }
    }

    fn scan_eq(&mut self) -> TokenKind {
        if self.cursor.eat('=') {
            TokenKind::EqEq
        } else if self.cursor.eat('>') {
            TokenKind::FatArrow
        } else {
            TokenKind::Eq
        }
    }

    fn scan_lt(&mut self) -> TokenKind {
        if self.cursor.eat('<') {
            if self.cursor.eat('=') {
                TokenKind::ShlEq
            } else {
                TokenKind::Shl
            }
        } else if self.cursor.eat('=') {
            TokenKind::Le
        } else {
            TokenKind::Lt
        }
    }

    fn scan_gt(&mut self) -> TokenKind {
        if self.cursor.eat('>') {
            if self.cursor.eat('=') {
                TokenKind::ShrEq
            } else {
                TokenKind::Shr
            }
        } else if self.cursor.eat('=') {
            TokenKind::Ge
        } else {
            TokenKind::Gt
        }
    }

    fn scan_colon(&mut self) -> TokenKind {
        if self.cursor.eat(':') {
            TokenKind::ColonColon
        } else {
            TokenKind::Colon
        }
    }

    fn scan_dot(&mut self) -> LexerResult<TokenKind> {
        if self.cursor.eat('.') {
            if self.cursor.eat('.') {
                Ok(TokenKind::DotDotDot)
            } else if self.cursor.eat('=') {
                Ok(TokenKind::DotDotEq)
            } else {
                Ok(TokenKind::DotDot)
            }
        } else if is_digit(self.cursor.first()) {
            // Could be a float like .5 - treat as Dot for now, parser handles
            Ok(TokenKind::Dot)
        } else {
            Ok(TokenKind::Dot)
        }
    }

    // =========================================================================
    // STRING SCANNING
    // =========================================================================

    fn scan_string(&mut self) -> LexerResult<TokenKind> {
        let terminated = self.scan_string_content('"')?;
        Ok(TokenKind::Literal {
            kind: LiteralKind::Str { terminated },
            suffix: self.scan_literal_suffix(),
        })
    }

    fn scan_string_content(&mut self, quote: char) -> LexerResult<bool> {
        loop {
            match self.cursor.first() {
                c if c == quote => {
                    self.cursor.bump();
                    return Ok(true);
                }
                '\\' => {
                    self.cursor.bump();
                    self.scan_escape()?;
                }
                '\n' | '\r' => {
                    // Allow newlines in strings (multiline strings)
                    self.cursor.bump();
                }
                EOF_CHAR => {
                    return Err(LexerError::new(
                        LexerErrorKind::UnterminatedString,
                        self.current_span(),
                    ));
                }
                _ => {
                    self.cursor.bump();
                }
            }
        }
    }

    fn scan_escape(&mut self) -> LexerResult<()> {
        let c = self.cursor.bump_or_eof();
        match c {
            'n' | 'r' | 't' | '\\' | '"' | '\'' | '0' => Ok(()),
            'x' => {
                // \xNN - two hex digits
                for _ in 0..2 {
                    let d = self.cursor.bump_or_eof();
                    if !is_hex_digit(d) && d != EOF_CHAR {
                        return Err(LexerError::new(
                            LexerErrorKind::InvalidHexEscape,
                            self.current_span(),
                        ));
                    }
                }
                Ok(())
            }
            'u' => {
                // \u{NNNN} - Unicode escape
                if !self.cursor.eat('{') {
                    return Err(LexerError::new(
                        LexerErrorKind::UnicodeEscapeMissingBrace,
                        self.current_span(),
                    ));
                }

                let mut count = 0;
                loop {
                    let d = self.cursor.first();
                    if d == '}' {
                        self.cursor.bump();
                        break;
                    }
                    if d == EOF_CHAR {
                        return Err(LexerError::new(
                            LexerErrorKind::UnicodeEscapeUnclosed,
                            self.current_span(),
                        ));
                    }
                    if !is_hex_digit(d) {
                        return Err(LexerError::new(
                            LexerErrorKind::InvalidUnicodeEscape,
                            self.current_span(),
                        ));
                    }
                    self.cursor.bump();
                    count += 1;
                    if count > 6 {
                        return Err(LexerError::new(
                            LexerErrorKind::UnicodeEscapeTooLong,
                            self.current_span(),
                        ));
                    }
                }
                Ok(())
            }
            '\n' | '\r' => {
                // Line continuation - skip whitespace on next line
                self.cursor.eat_while(is_whitespace);
                Ok(())
            }
            EOF_CHAR => Err(LexerError::new(
                LexerErrorKind::EscapeAtEof,
                self.current_span(),
            )),
            c => Err(LexerError::new(
                LexerErrorKind::UnknownEscape(c),
                self.current_span(),
            )),
        }
    }

    // =========================================================================
    // CHARACTER AND LIFETIME SCANNING
    // =========================================================================

    fn scan_char_or_lifetime(&mut self) -> LexerResult<TokenKind> {
        // Check if this is a lifetime or a character literal
        let first = self.cursor.first();

        // If it's an identifier character, check for lifetime
        if is_id_start(first) || first == '_' {
            // Could be a lifetime like 'a or 'static
            let save = self.cursor.savepoint();
            self.cursor.bump();
            self.cursor.eat_while(is_id_continue);

            // If NOT followed by ', it's a lifetime
            if self.cursor.first() != '\'' {
                return Ok(TokenKind::Lifetime);
            }

            // It's followed by ' - restore and treat as char
            self.cursor.restore(save);
        }

        // Scan character literal
        self.scan_char_literal()
    }

    fn scan_char_literal(&mut self) -> LexerResult<TokenKind> {
        let first = self.cursor.first();

        if first == '\'' {
            // Empty char literal
            self.cursor.bump();
            return Err(LexerError::new(
                LexerErrorKind::EmptyCharLiteral,
                self.current_span(),
            ));
        }

        if first == '\\' {
            self.cursor.bump();
            self.scan_escape()?;
        } else if first == EOF_CHAR {
            return Err(LexerError::new(
                LexerErrorKind::UnterminatedChar,
                self.current_span(),
            ));
        } else {
            self.cursor.bump();
        }

        // Expect closing '
        if !self.cursor.eat('\'') {
            // Check if there are more characters before '
            if self.cursor.first() != '\'' && self.cursor.first() != EOF_CHAR {
                // Multiple characters
                self.cursor.eat_until(|c| c == '\'');
                if self.cursor.eat('\'') {
                    return Err(LexerError::new(
                        LexerErrorKind::MultipleCharsInCharLiteral,
                        self.current_span(),
                    ));
                }
            }
            return Err(LexerError::new(
                LexerErrorKind::UnterminatedChar,
                self.current_span(),
            ));
        }

        Ok(TokenKind::Literal {
            kind: LiteralKind::Char { terminated: true },
            suffix: self.scan_literal_suffix(),
        })
    }

    // =========================================================================
    // RAW STRING SCANNING
    // =========================================================================

    fn scan_raw_string_or_ident(&mut self) -> LexerResult<Token> {
        // Already consumed 'r', check for raw string or raw identifier
        if self.cursor.first() == '#' && self.cursor.second() != '!' {
            // Could be raw string (r#"..."#) or raw identifier (r#ident)
            let mut hash_count = 0u8;
            while self.cursor.eat('#') {
                hash_count = hash_count.saturating_add(1);
            }

            if self.cursor.first() == '"' {
                // Raw string
                self.cursor.bump();
                return self.scan_raw_string_content(hash_count);
            } else if is_id_start(self.cursor.first()) {
                // Raw identifier
                return self.scan_raw_identifier();
            } else {
                // Just identifier 'r' followed by '#'s
                // This is actually a syntax error, but we'll let parser handle it
                return Ok(self.make_token(TokenKind::Ident));
            }
        }

        if self.cursor.first() == '"' {
            // r"..." - raw string with no hashes
            self.cursor.bump();
            return self.scan_raw_string_content(0);
        }

        // Just the identifier 'r'
        self.scan_identifier('r')
    }

    fn scan_raw_string_content(&mut self, hash_count: u8) -> LexerResult<Token> {
        loop {
            match self.cursor.first() {
                '"' => {
                    self.cursor.bump();
                    // Count closing hashes
                    let mut closing = 0u8;
                    while closing < hash_count && self.cursor.eat('#') {
                        closing += 1;
                    }
                    if closing == hash_count {
                        // Successfully terminated
                        let kind = TokenKind::Literal {
                            kind: LiteralKind::RawStr {
                                n_hashes: Some(hash_count),
                            },
                            suffix: self.scan_literal_suffix(),
                        };
                        return Ok(self.make_token(kind));
                    }
                    // Not enough hashes, continue scanning
                }
                EOF_CHAR => {
                    return Err(LexerError::new(
                        LexerErrorKind::UnterminatedRawString {
                            expected_hashes: hash_count,
                        },
                        self.current_span(),
                    ));
                }
                _ => {
                    self.cursor.bump();
                }
            }
        }
    }

    fn scan_raw_identifier(&mut self) -> LexerResult<Token> {
        // Already consumed 'r#', scan identifier
        let start_char = self.cursor.bump_or_eof();
        if !is_id_start(start_char) {
            return Err(LexerError::new(
                LexerErrorKind::ExpectedRawIdent,
                self.current_span(),
            ));
        }

        self.cursor.eat_while(is_id_continue);

        // Check if it's a keyword that can be raw
        let name = self.cursor.slice_from(self.token_start);
        // Strip the "r#" prefix
        let ident_part = &name[2..];
        if let Some(kw) = Keyword::from_str(ident_part) {
            if !kw.can_be_raw() {
                return Err(LexerError::new(
                    LexerErrorKind::CannotBeRawIdent(ident_part.to_string()),
                    self.current_span(),
                ));
            }
        }

        Ok(self.make_token(TokenKind::RawIdent))
    }

    // =========================================================================
    // BYTE STRING SCANNING
    // =========================================================================

    fn scan_byte_string_or_ident(&mut self) -> LexerResult<Token> {
        // Already consumed 'b'
        match self.cursor.first() {
            '"' => {
                self.cursor.bump();
                self.scan_byte_string()
            }
            '\'' => {
                self.cursor.bump();
                self.scan_byte_literal()
            }
            'r' => {
                self.cursor.bump();
                self.scan_raw_byte_string()
            }
            _ => self.scan_identifier('b'),
        }
    }

    fn scan_byte_string(&mut self) -> LexerResult<Token> {
        let terminated = self.scan_byte_string_content()?;
        let suffix = self.scan_literal_suffix();
        Ok(self.make_token(TokenKind::Literal {
            kind: LiteralKind::ByteStr { terminated },
            suffix,
        }))
    }

    fn scan_byte_string_content(&mut self) -> LexerResult<bool> {
        loop {
            match self.cursor.first() {
                '"' => {
                    self.cursor.bump();
                    return Ok(true);
                }
                '\\' => {
                    self.cursor.bump();
                    self.scan_byte_escape()?;
                }
                c if c.is_ascii() && c != '\r' => {
                    self.cursor.bump();
                }
                '\r' if self.cursor.second() == '\n' => {
                    self.cursor.bump();
                    self.cursor.bump();
                }
                EOF_CHAR => {
                    return Err(LexerError::new(
                        LexerErrorKind::UnterminatedByteString,
                        self.current_span(),
                    ));
                }
                c => {
                    return Err(LexerError::new(
                        LexerErrorKind::NonAsciiInByteLiteral,
                        self.current_span(),
                    ));
                }
            }
        }
    }

    fn scan_byte_escape(&mut self) -> LexerResult<()> {
        let c = self.cursor.bump_or_eof();
        match c {
            'n' | 'r' | 't' | '\\' | '"' | '\'' | '0' => Ok(()),
            'x' => {
                // \xNN - must be valid ASCII
                for _ in 0..2 {
                    let d = self.cursor.bump_or_eof();
                    if !is_hex_digit(d) {
                        return Err(LexerError::new(
                            LexerErrorKind::InvalidHexEscape,
                            self.current_span(),
                        ));
                    }
                }
                Ok(())
            }
            EOF_CHAR => Err(LexerError::new(
                LexerErrorKind::EscapeAtEof,
                self.current_span(),
            )),
            c => Err(LexerError::new(
                LexerErrorKind::UnknownEscape(c),
                self.current_span(),
            )),
        }
    }

    fn scan_byte_literal(&mut self) -> LexerResult<Token> {
        let first = self.cursor.first();

        if first == '\'' {
            self.cursor.bump();
            return Err(LexerError::new(
                LexerErrorKind::EmptyCharLiteral,
                self.current_span(),
            ));
        }

        if first == '\\' {
            self.cursor.bump();
            self.scan_byte_escape()?;
        } else if !first.is_ascii() {
            return Err(LexerError::new(
                LexerErrorKind::NonAsciiInByteLiteral,
                self.current_span(),
            ));
        } else {
            self.cursor.bump();
        }

        if !self.cursor.eat('\'') {
            return Err(LexerError::new(
                LexerErrorKind::UnterminatedChar,
                self.current_span(),
            ));
        }

        let suffix = self.scan_literal_suffix();
        Ok(self.make_token(TokenKind::Literal {
            kind: LiteralKind::Byte { terminated: true },
            suffix,
        }))
    }

    fn scan_raw_byte_string(&mut self) -> LexerResult<Token> {
        // Already consumed 'br'
        let mut hash_count = 0u8;
        while self.cursor.eat('#') {
            hash_count = hash_count.saturating_add(1);
        }

        if !self.cursor.eat('"') {
            // Just identifier
            return self.scan_identifier('b');
        }

        // Scan raw byte string content
        loop {
            match self.cursor.first() {
                '"' => {
                    self.cursor.bump();
                    let mut closing = 0u8;
                    while closing < hash_count && self.cursor.eat('#') {
                        closing += 1;
                    }
                    if closing == hash_count {
                        let suffix = self.scan_literal_suffix();
                        return Ok(self.make_token(TokenKind::Literal {
                            kind: LiteralKind::RawByteStr {
                                n_hashes: Some(hash_count),
                            },
                            suffix,
                        }));
                    }
                }
                c if !c.is_ascii() => {
                    return Err(LexerError::new(
                        LexerErrorKind::NonAsciiInByteLiteral,
                        self.current_span(),
                    ));
                }
                EOF_CHAR => {
                    return Err(LexerError::new(
                        LexerErrorKind::UnterminatedRawString {
                            expected_hashes: hash_count,
                        },
                        self.current_span(),
                    ));
                }
                _ => {
                    self.cursor.bump();
                }
            }
        }
    }

    // =========================================================================
    // C STRING SCANNING
    // =========================================================================

    fn scan_c_string(&mut self) -> LexerResult<Token> {
        // Already consumed 'c'
        self.cursor.bump(); // consume '"'

        let terminated = self.scan_string_content('"')?;
        let suffix = self.scan_literal_suffix();
        Ok(self.make_token(TokenKind::Literal {
            kind: LiteralKind::CStr { terminated },
            suffix,
        }))
    }

    // =========================================================================
    // FORMAT STRING SCANNING (STRING INTERPOLATION)
    // =========================================================================

    /// Scan a format string literal: `f"Hello, {name}!"`
    ///
    /// Format strings support:
    /// - `{expr}` - interpolation of an expression
    /// - `{expr:fmt}` - interpolation with format specifier
    /// - `{{` - escaped open brace (literal `{`)
    /// - `}}` - escaped close brace (literal `}`)
    fn scan_format_string(&mut self) -> LexerResult<Token> {
        // Already consumed 'f'
        self.cursor.bump(); // consume '"'

        let mut parts: Vec<InterpolatedPart> = Vec::new();
        let mut current_literal = String::new();

        loop {
            match self.cursor.first() {
                '"' => {
                    // End of format string
                    self.cursor.bump();

                    // Add any remaining literal part
                    if !current_literal.is_empty() {
                        parts.push(InterpolatedPart::Literal(current_literal));
                    }

                    let suffix = self.scan_literal_suffix();
                    return Ok(self.make_token(TokenKind::Literal {
                        kind: LiteralKind::FormatStr {
                            terminated: true,
                            parts,
                        },
                        suffix,
                    }));
                }
                '{' => {
                    self.cursor.bump();

                    if self.cursor.first() == '{' {
                        // Escaped brace: {{ becomes {
                        self.cursor.bump();
                        current_literal.push('{');
                    } else {
                        // Interpolation expression
                        // Save current literal part
                        if !current_literal.is_empty() {
                            parts.push(InterpolatedPart::Literal(current_literal));
                            current_literal = String::new();
                        }

                        // Scan the interpolation expression
                        let expr = self.scan_interpolation_expr()?;
                        parts.push(InterpolatedPart::Expr(expr));
                    }
                }
                '}' => {
                    self.cursor.bump();

                    if self.cursor.first() == '}' {
                        // Escaped brace: }} becomes }
                        self.cursor.bump();
                        current_literal.push('}');
                    } else {
                        // Unmatched closing brace - error
                        return Err(LexerError::new(
                            LexerErrorKind::UnexpectedChar('}'),
                            self.current_span(),
                        ));
                    }
                }
                '\\' => {
                    // Handle escape sequences
                    self.cursor.bump();
                    let escaped = self.scan_format_escape()?;
                    current_literal.push(escaped);
                }
                '\n' | '\r' => {
                    // Allow newlines in format strings (multiline)
                    current_literal.push(self.cursor.bump_or_eof());
                }
                EOF_CHAR => {
                    return Err(LexerError::new(
                        LexerErrorKind::UnterminatedFormatString,
                        self.current_span(),
                    ));
                }
                c => {
                    current_literal.push(c);
                    self.cursor.bump();
                }
            }
        }
    }

    /// Scan an interpolation expression: the content inside `{...}`.
    /// Handles nested braces for expressions like `{map[key]}` or `{obj.method()}`.
    fn scan_interpolation_expr(&mut self) -> LexerResult<String> {
        let mut expr = String::new();
        let mut brace_depth = 1u32;
        let max_depth = 16u32;

        // Check for empty interpolation
        if self.cursor.first() == '}' {
            return Err(LexerError::new(
                LexerErrorKind::EmptyInterpolation,
                self.current_span(),
            ));
        }

        loop {
            match self.cursor.first() {
                '{' => {
                    brace_depth += 1;
                    if brace_depth > max_depth {
                        return Err(LexerError::new(
                            LexerErrorKind::InterpolationTooDeep(max_depth),
                            self.current_span(),
                        ));
                    }
                    expr.push(self.cursor.bump_or_eof());
                }
                '}' => {
                    brace_depth -= 1;
                    if brace_depth == 0 {
                        self.cursor.bump(); // consume closing '}'
                        return Ok(expr);
                    }
                    expr.push(self.cursor.bump_or_eof());
                }
                '"' => {
                    // String literal inside expression - scan it carefully
                    expr.push(self.cursor.bump_or_eof());
                    self.scan_expr_string_literal(&mut expr)?;
                }
                '\'' => {
                    // Character literal or lifetime
                    expr.push(self.cursor.bump_or_eof());
                    // Simple handling: scan until closing quote
                    while self.cursor.first() != '\'' && self.cursor.first() != EOF_CHAR {
                        if self.cursor.first() == '\\' {
                            expr.push(self.cursor.bump_or_eof());
                        }
                        expr.push(self.cursor.bump_or_eof());
                    }
                    if self.cursor.first() == '\'' {
                        expr.push(self.cursor.bump_or_eof());
                    }
                }
                EOF_CHAR => {
                    return Err(LexerError::new(
                        LexerErrorKind::UnclosedInterpolation,
                        self.current_span(),
                    ));
                }
                c => {
                    expr.push(c);
                    self.cursor.bump();
                }
            }
        }
    }

    /// Scan a string literal inside an interpolation expression.
    /// This handles nested strings like `{map["key"]}`.
    fn scan_expr_string_literal(&mut self, expr: &mut String) -> LexerResult<()> {
        loop {
            match self.cursor.first() {
                '"' => {
                    expr.push(self.cursor.bump_or_eof());
                    return Ok(());
                }
                '\\' => {
                    expr.push(self.cursor.bump_or_eof());
                    if !self.cursor.is_eof() {
                        expr.push(self.cursor.bump_or_eof());
                    }
                }
                EOF_CHAR => {
                    return Err(LexerError::new(
                        LexerErrorKind::UnterminatedString,
                        self.current_span(),
                    ));
                }
                c => {
                    expr.push(c);
                    self.cursor.bump();
                }
            }
        }
    }

    /// Scan escape sequences in format strings.
    /// Similar to scan_escape but returns the actual character.
    fn scan_format_escape(&mut self) -> LexerResult<char> {
        let c = self.cursor.bump_or_eof();
        match c {
            'n' => Ok('\n'),
            'r' => Ok('\r'),
            't' => Ok('\t'),
            '\\' => Ok('\\'),
            '"' => Ok('"'),
            '\'' => Ok('\''),
            '0' => Ok('\0'),
            'x' => {
                // \xNN - two hex digits
                let mut value = 0u32;
                for _ in 0..2 {
                    let d = self.cursor.bump_or_eof();
                    if let Some(digit) = d.to_digit(16) {
                        value = value * 16 + digit;
                    } else {
                        return Err(LexerError::new(
                            LexerErrorKind::InvalidHexEscape,
                            self.current_span(),
                        ));
                    }
                }
                char::from_u32(value).ok_or_else(|| {
                    LexerError::new(
                        LexerErrorKind::InvalidUnicodeCodepoint(value),
                        self.current_span(),
                    )
                })
            }
            'u' => {
                // \u{NNNN} - Unicode escape
                if !self.cursor.eat('{') {
                    return Err(LexerError::new(
                        LexerErrorKind::UnicodeEscapeMissingBrace,
                        self.current_span(),
                    ));
                }

                let mut value = 0u32;
                let mut count = 0;
                loop {
                    let d = self.cursor.first();
                    if d == '}' {
                        self.cursor.bump();
                        break;
                    }
                    if d == EOF_CHAR {
                        return Err(LexerError::new(
                            LexerErrorKind::UnicodeEscapeUnclosed,
                            self.current_span(),
                        ));
                    }
                    if let Some(digit) = d.to_digit(16) {
                        value = value * 16 + digit;
                        self.cursor.bump();
                        count += 1;
                        if count > 6 {
                            return Err(LexerError::new(
                                LexerErrorKind::UnicodeEscapeTooLong,
                                self.current_span(),
                            ));
                        }
                    } else {
                        return Err(LexerError::new(
                            LexerErrorKind::InvalidUnicodeEscape,
                            self.current_span(),
                        ));
                    }
                }

                char::from_u32(value).ok_or_else(|| {
                    LexerError::new(
                        LexerErrorKind::InvalidUnicodeCodepoint(value),
                        self.current_span(),
                    )
                })
            }
            '\n' | '\r' => {
                // Line continuation - skip whitespace on next line
                self.cursor.eat_while(is_whitespace);
                Ok(' ') // Replace with space
            }
            EOF_CHAR => Err(LexerError::new(
                LexerErrorKind::EscapeAtEof,
                self.current_span(),
            )),
            c => Err(LexerError::new(
                LexerErrorKind::UnknownEscape(c),
                self.current_span(),
            )),
        }
    }

    // =========================================================================
    // NUMBER SCANNING
    // =========================================================================

    fn scan_number(&mut self, first: char) -> LexerResult<Token> {
        let (base, empty_int) = if first == '0' {
            match self.cursor.first() {
                'x' | 'X' => {
                    self.cursor.bump();
                    let empty = !self.scan_digits(16);
                    (IntBase::Hexadecimal, empty)
                }
                'o' | 'O' => {
                    self.cursor.bump();
                    let empty = !self.scan_digits(8);
                    (IntBase::Octal, empty)
                }
                'b' | 'B' => {
                    self.cursor.bump();
                    let empty = !self.scan_digits(2);
                    (IntBase::Binary, empty)
                }
                // Could be 0, 0.5, 0e10, etc.
                '0'..='9' | '.' | 'e' | 'E' => {
                    self.scan_digits(10);
                    (IntBase::Decimal, false)
                }
                _ => (IntBase::Decimal, false),
            }
        } else {
            self.scan_digits(10);
            (IntBase::Decimal, false)
        };

        // Check for float
        let is_float = if base == IntBase::Decimal {
            self.scan_float_part()?
        } else {
            false
        };

        // Scan and validate suffix
        let suffix = self.scan_literal_suffix();

        // Validate the suffix if present
        if let Some(ref s) = suffix {
            self.validate_number_suffix(s, is_float, base)?;
        }

        let kind = if is_float {
            LiteralKind::Float {
                empty_exponent: false,
            }
        } else {
            LiteralKind::Int { base, empty_int }
        };

        Ok(self.make_token(TokenKind::Literal { kind, suffix }))
    }

    /// Validate a numeric literal suffix.
    fn validate_number_suffix(
        &self,
        suffix: &str,
        is_float: bool,
        base: IntBase,
    ) -> LexerResult<()> {
        match validate_numeric_suffix(Some(suffix)) {
            NumericSuffixValidation::None => Ok(()),
            NumericSuffixValidation::Valid { kind, .. } => {
                // Check for integer suffix on float literal
                if is_float && kind != NumericSuffixKind::Float {
                    return Err(LexerError::new(
                        LexerErrorKind::IntSuffixOnFloat(suffix.to_string()),
                        self.current_span(),
                    ));
                }

                // Check for float suffix on non-decimal integer
                if !is_float && base != IntBase::Decimal && kind == NumericSuffixKind::Float {
                    return Err(LexerError::new(
                        LexerErrorKind::FloatSuffixOnNonDecimal(suffix.to_string()),
                        self.current_span(),
                    ));
                }

                Ok(())
            }
            NumericSuffixValidation::Invalid(s) => Err(LexerError::new(
                LexerErrorKind::InvalidNumericSuffix(s),
                self.current_span(),
            )),
        }
    }

    fn scan_digits(&mut self, radix: u32) -> bool {
        let mut has_digits = false;

        loop {
            let c = self.cursor.first();
            if c == '_' {
                self.cursor.bump();
            } else if c.is_digit(radix) {
                self.cursor.bump();
                has_digits = true;
            } else {
                break;
            }
        }

        has_digits
    }

    fn scan_float_part(&mut self) -> LexerResult<bool> {
        let mut is_float = false;

        // Check for decimal point
        if self.cursor.first() == '.' && self.cursor.second() != '.' && !is_id_start(self.cursor.second()) {
            self.cursor.bump();
            is_float = true;
            self.scan_digits(10);
        }

        // Check for exponent
        if self.cursor.first() == 'e' || self.cursor.first() == 'E' {
            self.cursor.bump();
            is_float = true;

            // Optional sign
            if self.cursor.first() == '+' || self.cursor.first() == '-' {
                self.cursor.bump();
            }

            if !self.scan_digits(10) {
                return Err(LexerError::new(
                    LexerErrorKind::EmptyExponent,
                    self.current_span(),
                ));
            }
        }

        Ok(is_float)
    }

    fn scan_literal_suffix(&mut self) -> Option<Box<str>> {
        if is_id_start(self.cursor.first()) {
            let start = self.cursor.pos();
            self.cursor.eat_while(is_id_continue);
            Some(self.cursor.slice_from(start).into())
        } else {
            None
        }
    }

    // =========================================================================
    // IDENTIFIER SCANNING
    // =========================================================================

    fn scan_identifier(&mut self, first: char) -> LexerResult<Token> {
        self.cursor.eat_while(is_id_continue);

        let text = self.cursor.slice_from(self.token_start);

        // Check for DSL macro invocation (e.g., sql!)
        if self.cursor.first() == '!' && is_dsl_name(text) {
            return self.scan_dsl_block(text.to_string());
        }

        // Check for keyword
        if let Some(kw) = Keyword::from_str(text) {
            // Handle true/false as boolean literals
            if kw == Keyword::True {
                return Ok(self.make_token(TokenKind::Literal {
                    kind: LiteralKind::Bool(true),
                    suffix: None,
                }));
            }
            if kw == Keyword::False {
                return Ok(self.make_token(TokenKind::Literal {
                    kind: LiteralKind::Bool(false),
                    suffix: None,
                }));
            }
            return Ok(self.make_token(TokenKind::Keyword(kw)));
        }

        Ok(self.make_token(TokenKind::Ident))
    }

    // =========================================================================
    // DSL BLOCK SCANNING
    // =========================================================================

    fn scan_dsl_block(&mut self, name: String) -> LexerResult<Token> {
        self.cursor.bump(); // consume '!'

        // Determine delimiter
        let (open, close) = match self.cursor.first() {
            '{' => ('{', '}'),
            '(' => ('(', ')'),
            '[' => ('[', ']'),
            c => {
                return Err(LexerError::new(
                    LexerErrorKind::UnexpectedChar(c),
                    self.current_span(),
                ));
            }
        };

        self.cursor.bump(); // consume opening delimiter

        let mut depth = 1u32;

        loop {
            let c = self.cursor.first();
            if c == open {
                depth += 1;
                self.cursor.bump();
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    self.cursor.bump();
                    break;
                }
                self.cursor.bump();
            } else if c == EOF_CHAR {
                return Err(LexerError::new(
                    LexerErrorKind::UnterminatedDslBlock,
                    self.current_span(),
                ));
            } else {
                self.cursor.bump();
            }
        }

        Ok(self.make_token(TokenKind::DslBlock { name: name.into() }))
    }

    // =========================================================================
    // WHITESPACE AND COMMENT HANDLING
    // =========================================================================

    fn skip_trivia(&mut self) {
        loop {
            let c = self.cursor.first();
            if is_whitespace(c) {
                self.cursor.bump();
            } else if c == '/' {
                match self.cursor.second() {
                    '/' => {
                        self.skip_line_comment();
                    }
                    '*' => {
                        if let Err(e) = self.skip_block_comment() {
                            self.errors.push(e);
                        }
                    }
                    _ => break,
                }
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        self.cursor.eat_until(|c| c == '\n');
        self.cursor.eat('\n');
    }

    fn skip_block_comment(&mut self) -> LexerResult<()> {
        self.cursor.bump(); // '/'
        self.cursor.bump(); // '*'

        let mut depth = 1u32;

        loop {
            match (self.cursor.first(), self.cursor.second()) {
                ('*', '/') => {
                    self.cursor.bump();
                    self.cursor.bump();
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                ('/', '*') => {
                    self.cursor.bump();
                    self.cursor.bump();
                    depth += 1;
                }
                (EOF_CHAR, _) => {
                    return Err(LexerError::new(
                        LexerErrorKind::UnterminatedBlockComment { depth },
                        self.current_span(),
                    ));
                }
                _ => {
                    self.cursor.bump();
                }
            }
        }
    }

    fn scan_whitespace(&mut self) -> Token {
        self.cursor.eat_while(is_whitespace);
        self.make_token(TokenKind::Whitespace)
    }

    fn scan_line_comment(&mut self) -> LexerResult<Token> {
        self.cursor.bump(); // first '/'
        self.cursor.bump(); // second '/'

        let is_doc = self.cursor.first() == '/' || self.cursor.first() == '!';
        let is_inner = self.cursor.first() == '!';

        self.cursor.eat_until(|c| c == '\n');

        Ok(self.make_token(TokenKind::Comment { is_doc, is_inner }))
    }

    fn scan_block_comment(&mut self) -> LexerResult<Token> {
        self.cursor.bump(); // '/'
        self.cursor.bump(); // '*'

        let is_doc = self.cursor.first() == '*' || self.cursor.first() == '!';
        let is_inner = self.cursor.first() == '!';

        let mut depth = 1u32;

        loop {
            match (self.cursor.first(), self.cursor.second()) {
                ('*', '/') => {
                    self.cursor.bump();
                    self.cursor.bump();
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                ('/', '*') => {
                    self.cursor.bump();
                    self.cursor.bump();
                    depth += 1;
                }
                (EOF_CHAR, _) => {
                    return Err(LexerError::new(
                        LexerErrorKind::UnterminatedBlockComment { depth },
                        self.current_span(),
                    ));
                }
                _ => {
                    self.cursor.bump();
                }
            }
        }

        Ok(self.make_token(TokenKind::Comment { is_doc, is_inner }))
    }

    fn skip_shebang(&mut self) {
        self.cursor.eat_until(|c| c == '\n');
        self.cursor.eat('\n');
    }

    // =========================================================================
    // HELPER METHODS
    // =========================================================================

    fn make_token(&self, kind: TokenKind) -> Token {
        Token::new(kind, self.current_span())
    }

    fn current_span(&self) -> Span {
        Span::new(self.token_start, self.cursor.pos(), self.cursor.source_id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(source: &str) -> Vec<Token> {
        let file = SourceFile::anonymous(source);
        let mut lexer = Lexer::new(&file);
        lexer.tokenize().unwrap()
    }

    fn lex_one(source: &str) -> TokenKind {
        let tokens = lex(source);
        tokens[0].kind.clone()
    }

    #[test]
    fn test_operators() {
        assert!(matches!(lex_one("+"), TokenKind::Plus));
        assert!(matches!(lex_one("-"), TokenKind::Minus));
        assert!(matches!(lex_one("*"), TokenKind::Star));
        assert!(matches!(lex_one("/"), TokenKind::Slash));
        assert!(matches!(lex_one("+="), TokenKind::PlusEq));
        assert!(matches!(lex_one("->"), TokenKind::Arrow));
        assert!(matches!(lex_one("=>"), TokenKind::FatArrow));
        assert!(matches!(lex_one("::"), TokenKind::ColonColon));
        assert!(matches!(lex_one(".."), TokenKind::DotDot));
        assert!(matches!(lex_one("..."), TokenKind::DotDotDot));
        assert!(matches!(lex_one("..="), TokenKind::DotDotEq));
    }

    #[test]
    fn test_numbers() {
        assert!(matches!(
            lex_one("42"),
            TokenKind::Literal {
                kind: LiteralKind::Int {
                    base: IntBase::Decimal,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            lex_one("0xFF"),
            TokenKind::Literal {
                kind: LiteralKind::Int {
                    base: IntBase::Hexadecimal,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            lex_one("0b1010"),
            TokenKind::Literal {
                kind: LiteralKind::Int {
                    base: IntBase::Binary,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            lex_one("3.14"),
            TokenKind::Literal {
                kind: LiteralKind::Float { .. },
                ..
            }
        ));
        assert!(matches!(
            lex_one("1e10"),
            TokenKind::Literal {
                kind: LiteralKind::Float { .. },
                ..
            }
        ));
    }

    #[test]
    fn test_strings() {
        assert!(matches!(
            lex_one(r#""hello""#),
            TokenKind::Literal {
                kind: LiteralKind::Str { terminated: true },
                ..
            }
        ));
    }

    #[test]
    fn test_raw_strings() {
        assert!(matches!(
            lex_one(r##"r#"raw"#"##),
            TokenKind::Literal {
                kind: LiteralKind::RawStr { .. },
                ..
            }
        ));
    }

    #[test]
    fn test_keywords() {
        assert!(matches!(lex_one("fn"), TokenKind::Keyword(Keyword::Fn)));
        assert!(matches!(lex_one("let"), TokenKind::Keyword(Keyword::Let)));
        assert!(matches!(lex_one("if"), TokenKind::Keyword(Keyword::If)));
        assert!(matches!(lex_one("match"), TokenKind::Keyword(Keyword::Match)));
    }

    #[test]
    fn test_booleans() {
        assert!(matches!(
            lex_one("true"),
            TokenKind::Literal {
                kind: LiteralKind::Bool(true),
                ..
            }
        ));
        assert!(matches!(
            lex_one("false"),
            TokenKind::Literal {
                kind: LiteralKind::Bool(false),
                ..
            }
        ));
    }

    #[test]
    fn test_identifiers() {
        assert!(matches!(lex_one("foo"), TokenKind::Ident));
        assert!(matches!(lex_one("_bar"), TokenKind::Ident));
        assert!(matches!(lex_one("baz123"), TokenKind::Ident));
    }

    #[test]
    fn test_lifetimes() {
        assert!(matches!(lex_one("'a"), TokenKind::Lifetime));
        assert!(matches!(lex_one("'static"), TokenKind::Lifetime));
    }

    #[test]
    fn test_delimiters() {
        assert!(matches!(
            lex_one("("),
            TokenKind::OpenDelim(Delimiter::Paren)
        ));
        assert!(matches!(
            lex_one(")"),
            TokenKind::CloseDelim(Delimiter::Paren)
        ));
        assert!(matches!(
            lex_one("{"),
            TokenKind::OpenDelim(Delimiter::Brace)
        ));
    }

    #[test]
    fn test_full_tokenization() {
        let tokens = lex("fn main() { let x = 42; }");
        // fn, main, (, ), {, let, x, =, 42, ;, }, EOF
        assert_eq!(tokens.len(), 12);
    }

    #[test]
    fn test_comments_skipped() {
        let tokens = lex("// comment\nlet x = 1");
        assert!(matches!(tokens[0].kind, TokenKind::Keyword(Keyword::Let)));
    }

    #[test]
    fn test_nested_block_comments() {
        let tokens = lex("/* outer /* inner */ outer */ let x = 1");
        assert!(matches!(tokens[0].kind, TokenKind::Keyword(Keyword::Let)));
    }

    #[test]
    fn test_unicode_identifier() {
        // Greek letters
        assert!(matches!(lex_one("α"), TokenKind::Ident));
        assert!(matches!(lex_one("αβγ"), TokenKind::Ident));
        // ASCII identifier
        assert!(matches!(lex_one("cafe"), TokenKind::Ident));
    }

    // =========================================================================
    // FORMAT STRING TESTS
    // =========================================================================

    #[test]
    fn test_format_string_simple() {
        let kind = lex_one(r#"f"hello""#);
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::FormatStr { terminated, parts },
                ..
            } => {
                assert!(terminated);
                assert_eq!(parts.len(), 1);
                match &parts[0] {
                    InterpolatedPart::Literal(s) => assert_eq!(s, "hello"),
                    _ => panic!("expected literal part"),
                }
            }
            _ => panic!("expected format string, got {:?}", kind),
        }
    }

    #[test]
    fn test_format_string_with_interpolation() {
        let kind = lex_one(r#"f"Hello, {name}!""#);
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::FormatStr { terminated, parts },
                ..
            } => {
                assert!(terminated);
                assert_eq!(parts.len(), 3);
                match &parts[0] {
                    InterpolatedPart::Literal(s) => assert_eq!(s, "Hello, "),
                    _ => panic!("expected literal part"),
                }
                match &parts[1] {
                    InterpolatedPart::Expr(e) => assert_eq!(e, "name"),
                    _ => panic!("expected expr part"),
                }
                match &parts[2] {
                    InterpolatedPart::Literal(s) => assert_eq!(s, "!"),
                    _ => panic!("expected literal part"),
                }
            }
            _ => panic!("expected format string, got {:?}", kind),
        }
    }

    #[test]
    fn test_format_string_multiple_interpolations() {
        let kind = lex_one(r#"f"{a} + {b} = {c}""#);
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::FormatStr { parts, .. },
                ..
            } => {
                assert_eq!(parts.len(), 5);
                // {a}, " + ", {b}, " = ", {c}
                assert!(matches!(&parts[0], InterpolatedPart::Expr(e) if e == "a"));
                assert!(matches!(&parts[1], InterpolatedPart::Literal(s) if s == " + "));
                assert!(matches!(&parts[2], InterpolatedPart::Expr(e) if e == "b"));
                assert!(matches!(&parts[3], InterpolatedPart::Literal(s) if s == " = "));
                assert!(matches!(&parts[4], InterpolatedPart::Expr(e) if e == "c"));
            }
            _ => panic!("expected format string"),
        }
    }

    #[test]
    fn test_format_string_escaped_braces() {
        let kind = lex_one(r#"f"Use {{x}} for braces""#);
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::FormatStr { parts, .. },
                ..
            } => {
                assert_eq!(parts.len(), 1);
                match &parts[0] {
                    InterpolatedPart::Literal(s) => assert_eq!(s, "Use {x} for braces"),
                    _ => panic!("expected literal part"),
                }
            }
            _ => panic!("expected format string"),
        }
    }

    #[test]
    fn test_format_string_nested_braces() {
        let kind = lex_one(r#"f"{map[key]}""#);
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::FormatStr { parts, .. },
                ..
            } => {
                assert_eq!(parts.len(), 1);
                match &parts[0] {
                    InterpolatedPart::Expr(e) => assert_eq!(e, "map[key]"),
                    _ => panic!("expected expr part"),
                }
            }
            _ => panic!("expected format string"),
        }
    }

    #[test]
    fn test_format_string_with_nested_string() {
        let kind = lex_one(r#"f"{dict["key"]}""#);
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::FormatStr { parts, .. },
                ..
            } => {
                assert_eq!(parts.len(), 1);
                match &parts[0] {
                    InterpolatedPart::Expr(e) => assert_eq!(e, r#"dict["key"]"#),
                    _ => panic!("expected expr part"),
                }
            }
            _ => panic!("expected format string"),
        }
    }

    #[test]
    fn test_format_string_with_escape_sequences() {
        let kind = lex_one(r#"f"line1\nline2""#);
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::FormatStr { parts, .. },
                ..
            } => {
                assert_eq!(parts.len(), 1);
                match &parts[0] {
                    InterpolatedPart::Literal(s) => assert_eq!(s, "line1\nline2"),
                    _ => panic!("expected literal part"),
                }
            }
            _ => panic!("expected format string"),
        }
    }

    #[test]
    fn test_format_string_only_expression() {
        let kind = lex_one(r#"f"{value}""#);
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::FormatStr { parts, .. },
                ..
            } => {
                assert_eq!(parts.len(), 1);
                match &parts[0] {
                    InterpolatedPart::Expr(e) => assert_eq!(e, "value"),
                    _ => panic!("expected expr part"),
                }
            }
            _ => panic!("expected format string"),
        }
    }

    #[test]
    fn test_format_string_complex_expression() {
        let kind = lex_one(r#"f"{obj.method(arg1, arg2)}""#);
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::FormatStr { parts, .. },
                ..
            } => {
                assert_eq!(parts.len(), 1);
                match &parts[0] {
                    InterpolatedPart::Expr(e) => assert_eq!(e, "obj.method(arg1, arg2)"),
                    _ => panic!("expected expr part"),
                }
            }
            _ => panic!("expected format string"),
        }
    }

    // =========================================================================
    // NUMERIC SUFFIX TESTS
    // =========================================================================

    #[test]
    fn test_integer_with_valid_suffixes() {
        // Signed integers
        for suffix in &["i8", "i16", "i32", "i64", "i128", "isize"] {
            let source = format!("42{}", suffix);
            let kind = lex_one(&source);
            match kind {
                TokenKind::Literal {
                    kind: LiteralKind::Int { .. },
                    suffix: Some(s),
                } => assert_eq!(&*s, *suffix),
                _ => panic!("expected integer with suffix {}", suffix),
            }
        }

        // Unsigned integers
        for suffix in &["u8", "u16", "u32", "u64", "u128", "usize"] {
            let source = format!("42{}", suffix);
            let kind = lex_one(&source);
            match kind {
                TokenKind::Literal {
                    kind: LiteralKind::Int { .. },
                    suffix: Some(s),
                } => assert_eq!(&*s, *suffix),
                _ => panic!("expected integer with suffix {}", suffix),
            }
        }
    }

    #[test]
    fn test_float_with_valid_suffixes() {
        for suffix in &["f32", "f64"] {
            let source = format!("3.14{}", suffix);
            let kind = lex_one(&source);
            match kind {
                TokenKind::Literal {
                    kind: LiteralKind::Float { .. },
                    suffix: Some(s),
                } => assert_eq!(&*s, *suffix),
                _ => panic!("expected float with suffix {}", suffix),
            }
        }
    }

    #[test]
    fn test_integer_with_float_suffix() {
        // Float suffix on decimal integer is valid (implicit conversion)
        let kind = lex_one("42f64");
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::Int { .. },
                suffix: Some(s),
            } => assert_eq!(&*s, "f64"),
            _ => panic!("expected integer with f64 suffix"),
        }
    }

    #[test]
    fn test_hex_with_float_suffix_error() {
        // Float suffix on non-decimal integer should error
        // Note: 0xFFf32 is actually a valid hex number (f,3,2 are hex digits)
        // So we test with octal where f is not a valid digit
        let file = SourceFile::anonymous("0o77f32");
        let mut lexer = Lexer::new(&file);
        let result = lexer.tokenize();
        assert!(result.is_err());
    }

    #[test]
    fn test_float_with_int_suffix_error() {
        // Integer suffix on float literal should error
        let file = SourceFile::anonymous("3.14i32");
        let mut lexer = Lexer::new(&file);
        let result = lexer.tokenize();
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_suffix_error() {
        // Invalid suffix should error
        let file = SourceFile::anonymous("42foo");
        let mut lexer = Lexer::new(&file);
        let result = lexer.tokenize();
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_with_valid_int_suffix() {
        let kind = lex_one("0xFFu8");
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::Int { base: IntBase::Hexadecimal, .. },
                suffix: Some(s),
            } => assert_eq!(&*s, "u8"),
            _ => panic!("expected hex integer with u8 suffix"),
        }
    }

    #[test]
    fn test_binary_with_valid_int_suffix() {
        let kind = lex_one("0b1010i32");
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::Int { base: IntBase::Binary, .. },
                suffix: Some(s),
            } => assert_eq!(&*s, "i32"),
            _ => panic!("expected binary integer with i32 suffix"),
        }
    }

    #[test]
    fn test_octal_with_valid_int_suffix() {
        let kind = lex_one("0o755u32");
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::Int { base: IntBase::Octal, .. },
                suffix: Some(s),
            } => assert_eq!(&*s, "u32"),
            _ => panic!("expected octal integer with u32 suffix"),
        }
    }

    #[test]
    fn test_scientific_notation_with_suffix() {
        let kind = lex_one("1e10f64");
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::Float { .. },
                suffix: Some(s),
            } => assert_eq!(&*s, "f64"),
            _ => panic!("expected float with f64 suffix"),
        }
    }

    #[test]
    fn test_no_suffix() {
        let kind = lex_one("42");
        match kind {
            TokenKind::Literal {
                kind: LiteralKind::Int { .. },
                suffix: None,
            } => {}
            _ => panic!("expected integer without suffix"),
        }
    }

    // =========================================================================
    // DOC COMMENT EXTRACTION TESTS
    // =========================================================================

    fn lex_with_docs(source: &str) -> (Vec<Token>, DocComments) {
        let file = SourceFile::anonymous(source);
        let mut lexer = Lexer::new(&file);
        lexer.tokenize_with_docs().unwrap()
    }

    #[test]
    fn test_outer_line_doc_comment() {
        let (_, docs) = lex_with_docs("/// This is a doc comment\nfn foo() {}");
        assert_eq!(docs.len(), 1);
        assert!(docs.comments()[0].is_outer());
        assert!(docs.comments()[0].is_line());
        assert_eq!(docs.comments()[0].content, "This is a doc comment");
    }

    #[test]
    fn test_inner_line_doc_comment() {
        let (_, docs) = lex_with_docs("//! Module documentation\nfn foo() {}");
        assert_eq!(docs.len(), 1);
        assert!(docs.comments()[0].is_inner());
        assert!(docs.comments()[0].is_line());
        assert_eq!(docs.comments()[0].content, "Module documentation");
    }

    #[test]
    fn test_multiple_doc_comments() {
        let source = "/// First line\n/// Second line\nfn foo() {}";
        let (_, docs) = lex_with_docs(source);
        assert_eq!(docs.len(), 2);
        assert_eq!(docs.comments()[0].content, "First line");
        assert_eq!(docs.comments()[1].content, "Second line");
        assert_eq!(docs.to_doc_string(), "First line\nSecond line");
    }

    #[test]
    fn test_mixed_inner_outer_docs() {
        let source = "//! Module doc\n/// Function doc\nfn foo() {}";
        let (_, docs) = lex_with_docs(source);
        assert_eq!(docs.len(), 2);
        assert_eq!(docs.inner_comments().len(), 1);
        assert_eq!(docs.outer_comments().len(), 1);
        assert_eq!(docs.to_inner_doc_string(), "Module doc");
        assert_eq!(docs.to_outer_doc_string(), "Function doc");
    }

    #[test]
    fn test_doc_comment_preserves_tokens() {
        let (tokens, docs) = lex_with_docs("/// Doc\nfn foo() {}");
        // Tokens should not include doc comments
        assert!(!tokens.iter().any(|t| matches!(t.kind, TokenKind::Comment { .. })));
        // Should have fn, foo, (, ), {, }, EOF
        assert!(tokens.len() >= 7);
        // But we should have extracted the doc
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn test_doc_comments_kind() {
        let (_, docs) = lex_with_docs("/// outer\n//! inner");
        assert_eq!(docs.comments()[0].kind, DocCommentKind::OuterLine);
        assert_eq!(docs.comments()[1].kind, DocCommentKind::InnerLine);
    }

    #[test]
    fn test_empty_doc_collection() {
        let (_, docs) = lex_with_docs("fn foo() {}");
        assert!(docs.is_empty());
        assert_eq!(docs.to_doc_string(), "");
    }

    #[test]
    fn test_doc_comment_no_space() {
        // Doc comment without space after ///
        let (_, docs) = lex_with_docs("///No space\nfn foo() {}");
        assert_eq!(docs.comments()[0].content, "No space");
    }
}
