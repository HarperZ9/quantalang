// ===============================================================================
// QUANTALANG LEXER - CHARACTER CURSOR
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Character cursor for lexical analysis.
//!
//! This module provides a cursor that iterates over characters in source code
//! with proper Unicode handling and position tracking.

use super::span::{BytePos, SourceId};
use std::str::Chars;

/// End-of-file character marker.
pub const EOF_CHAR: char = '\0';

/// A cursor over the source code that tracks position.
#[derive(Debug, Clone)]
pub struct Cursor<'a> {
    /// The source code being scanned.
    source: &'a str,
    /// Iterator over characters.
    chars: Chars<'a>,
    /// The source file ID.
    source_id: SourceId,
    /// Number of bytes consumed from the input.
    bytes_consumed: u32,
    /// Initial length for calculating consumed bytes.
    initial_len: usize,
}

impl<'a> Cursor<'a> {
    /// Create a new cursor over the source code.
    pub fn new(source: &'a str, source_id: SourceId) -> Self {
        Self {
            source,
            chars: source.chars(),
            source_id,
            bytes_consumed: 0,
            initial_len: source.len(),
        }
    }

    /// Get the source ID.
    #[inline]
    pub fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// Get the current byte position.
    #[inline]
    pub fn pos(&self) -> BytePos {
        BytePos(self.bytes_consumed)
    }

    /// Get the number of bytes remaining.
    #[inline]
    pub fn remaining_len(&self) -> usize {
        self.chars.as_str().len()
    }

    /// Check if we've reached the end of the source.
    #[inline]
    pub fn is_eof(&self) -> bool {
        self.chars.as_str().is_empty()
    }

    /// Peek at the first character without consuming it.
    /// Returns `EOF_CHAR` if at end of input.
    #[inline]
    pub fn first(&self) -> char {
        self.chars.clone().next().unwrap_or(EOF_CHAR)
    }

    /// Peek at the second character without consuming it.
    /// Returns `EOF_CHAR` if at end of input.
    #[inline]
    pub fn second(&self) -> char {
        let mut chars = self.chars.clone();
        chars.next();
        chars.next().unwrap_or(EOF_CHAR)
    }

    /// Peek at the third character without consuming it.
    /// Returns `EOF_CHAR` if at end of input.
    #[inline]
    pub fn third(&self) -> char {
        let mut chars = self.chars.clone();
        chars.next();
        chars.next();
        chars.next().unwrap_or(EOF_CHAR)
    }

    /// Peek at the nth character (0-indexed) without consuming it.
    pub fn peek_nth(&self, n: usize) -> char {
        let mut chars = self.chars.clone();
        for _ in 0..n {
            if chars.next().is_none() {
                return EOF_CHAR;
            }
        }
        chars.next().unwrap_or(EOF_CHAR)
    }

    /// Peek at the remaining source as a string slice.
    #[inline]
    pub fn remaining(&self) -> &'a str {
        self.chars.as_str()
    }

    /// Check if the remaining source starts with the given prefix.
    #[inline]
    pub fn starts_with(&self, prefix: &str) -> bool {
        self.chars.as_str().starts_with(prefix)
    }

    /// Consume and return the next character.
    /// Returns `None` if at end of input.
    pub fn bump(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        self.bytes_consumed += c.len_utf8() as u32;
        Some(c)
    }

    /// Consume the next character, returning it.
    /// Returns `EOF_CHAR` if at end of input.
    #[inline]
    pub fn bump_or_eof(&mut self) -> char {
        self.bump().unwrap_or(EOF_CHAR)
    }

    /// Consume a character if it matches the expected character.
    /// Returns `true` if consumed, `false` otherwise.
    #[inline]
    pub fn eat(&mut self, expected: char) -> bool {
        if self.first() == expected {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consume characters while the predicate returns true.
    pub fn eat_while<P>(&mut self, mut predicate: P)
    where
        P: FnMut(char) -> bool,
    {
        while predicate(self.first()) && !self.is_eof() {
            self.bump();
        }
    }

    /// Consume the next n characters.
    pub fn bump_n(&mut self, n: usize) {
        for _ in 0..n {
            if self.bump().is_none() {
                break;
            }
        }
    }

    /// Consume characters until the predicate returns true.
    pub fn eat_until<P>(&mut self, mut predicate: P)
    where
        P: FnMut(char) -> bool,
    {
        while !predicate(self.first()) && !self.is_eof() {
            self.bump();
        }
    }

    /// Get the source text from `start` to the current position.
    pub fn slice_from(&self, start: BytePos) -> &'a str {
        let start_idx = start.to_usize();
        let end_idx = self.pos().to_usize();
        &self.source[start_idx..end_idx]
    }

    /// Get the entire source.
    #[inline]
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// Reset the consumed byte counter (for starting a new token).
    #[inline]
    pub fn reset_consumed(&mut self) {
        self.bytes_consumed = (self.initial_len - self.remaining_len()) as u32;
    }

    /// Get bytes consumed since the last token start.
    #[inline]
    pub fn token_len(&self) -> u32 {
        self.bytes_consumed
    }

    /// Create a savepoint that can be used to restore the cursor.
    pub fn savepoint(&self) -> CursorSavepoint<'a> {
        CursorSavepoint {
            chars: self.chars.clone(),
            bytes_consumed: self.bytes_consumed,
        }
    }

    /// Restore from a savepoint.
    pub fn restore(&mut self, savepoint: CursorSavepoint<'a>) {
        self.chars = savepoint.chars;
        self.bytes_consumed = savepoint.bytes_consumed;
    }
}

/// A savepoint for restoring cursor state.
#[derive(Debug, Clone)]
pub struct CursorSavepoint<'a> {
    chars: Chars<'a>,
    bytes_consumed: u32,
}

/// Check if a character is a valid start of an identifier (UAX #31).
pub fn is_id_start(c: char) -> bool {
    unicode_xid::UnicodeXID::is_xid_start(c) || c == '_'
}

/// Check if a character can continue an identifier (UAX #31).
pub fn is_id_continue(c: char) -> bool {
    unicode_xid::UnicodeXID::is_xid_continue(c)
}

/// Check if a character is ASCII whitespace.
#[inline]
pub fn is_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0B' | '\x0C')
}

/// Check if a character is a decimal digit.
#[inline]
pub fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

/// Check if a character is a hexadecimal digit.
#[inline]
pub fn is_hex_digit(c: char) -> bool {
    c.is_ascii_hexdigit()
}

/// Check if a character is an octal digit.
#[inline]
pub fn is_octal_digit(c: char) -> bool {
    matches!(c, '0'..='7')
}

/// Check if a character is a binary digit.
#[inline]
pub fn is_binary_digit(c: char) -> bool {
    matches!(c, '0' | '1')
}

/// Check if a character is valid in a number (including underscore separator).
#[inline]
pub fn is_digit_char(c: char, radix: u32) -> bool {
    c == '_' || c.is_digit(radix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_basic() {
        let mut cursor = Cursor::new("hello", SourceId(0));

        assert_eq!(cursor.first(), 'h');
        assert_eq!(cursor.second(), 'e');
        assert_eq!(cursor.bump(), Some('h'));
        assert_eq!(cursor.first(), 'e');
        assert_eq!(cursor.pos().0, 1);
    }

    #[test]
    fn test_cursor_unicode() {
        let mut cursor = Cursor::new("你", SourceId(0));

        assert_eq!(cursor.first(), '你');
        assert_eq!(cursor.bump(), Some('你'));
        // Check UTF-8 encoding: 你 is 3 bytes
        assert_eq!(cursor.pos().0, 3);
    }

    #[test]
    fn test_cursor_eof() {
        let mut cursor = Cursor::new("ab", SourceId(0));

        assert_eq!(cursor.bump(), Some('a'));
        assert_eq!(cursor.bump(), Some('b'));
        assert_eq!(cursor.bump(), None);
        assert!(cursor.is_eof());
        assert_eq!(cursor.first(), EOF_CHAR);
    }

    #[test]
    fn test_eat_while() {
        let mut cursor = Cursor::new("aaab", SourceId(0));
        cursor.eat_while(|c| c == 'a');
        assert_eq!(cursor.first(), 'b');
        assert_eq!(cursor.pos().0, 3);
    }

    #[test]
    fn test_savepoint() {
        let mut cursor = Cursor::new("hello", SourceId(0));
        cursor.bump();
        cursor.bump();

        let save = cursor.savepoint();
        cursor.bump();
        cursor.bump();
        assert_eq!(cursor.first(), 'o');

        cursor.restore(save);
        assert_eq!(cursor.first(), 'l');
    }

    #[test]
    fn test_id_start() {
        assert!(is_id_start('a'));
        assert!(is_id_start('Z'));
        assert!(is_id_start('_'));
        assert!(is_id_start('你')); // Chinese character
        assert!(!is_id_start('0'));
        assert!(!is_id_start('-'));
    }

    #[test]
    fn test_id_continue() {
        assert!(is_id_continue('a'));
        assert!(is_id_continue('0'));
        assert!(is_id_continue('_'));
        assert!(!is_id_continue('-'));
        assert!(!is_id_continue(' '));
    }
}
