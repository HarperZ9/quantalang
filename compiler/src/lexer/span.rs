// ===============================================================================
// QUANTALANG LEXER - SOURCE LOCATION TRACKING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Source location types for error reporting and debugging.
//!
//! This module provides types for tracking positions in source code:
//! - `BytePos`: An absolute byte offset in the source
//! - `Position`: A human-readable line/column position
//! - `Span`: A range of source code (start to end position)
//! - `SourceFile`: A source file with its content
//! - `SourceId`: A unique identifier for a source file

use std::fmt;
use std::ops::{Add, Range};
use std::sync::Arc;

/// Unique identifier for a source file in a compilation session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SourceId(pub u32);

impl SourceId {
    /// Create a new source ID.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// The anonymous source ID (for code not from a file).
    pub const ANONYMOUS: Self = Self(0);
}

/// An absolute byte position in source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct BytePos(pub u32);

impl BytePos {
    /// Create a new byte position.
    #[inline]
    pub const fn new(pos: u32) -> Self {
        Self(pos)
    }

    /// Get the position as a usize.
    #[inline]
    pub const fn to_usize(self) -> usize {
        self.0 as usize
    }

    /// Create from a usize.
    #[inline]
    pub fn from_usize(pos: usize) -> Self {
        Self(pos as u32)
    }
}

impl Add<u32> for BytePos {
    type Output = Self;

    #[inline]
    fn add(self, rhs: u32) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl fmt::Display for BytePos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A human-readable position in source code (1-indexed line and column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Position {
    /// Line number (1-indexed).
    pub line: u32,
    /// Column number (1-indexed, in Unicode scalar values).
    pub column: u32,
    /// Absolute byte offset in the source.
    pub offset: BytePos,
}

impl Position {
    /// Create a new position.
    #[inline]
    pub const fn new(line: u32, column: u32, offset: BytePos) -> Self {
        Self {
            line,
            column,
            offset,
        }
    }

    /// Create the start position (line 1, column 1, offset 0).
    #[inline]
    pub const fn start() -> Self {
        Self {
            line: 1,
            column: 1,
            offset: BytePos(0),
        }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// A span of source code, representing a range from start to end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    /// Start byte position.
    pub start: BytePos,
    /// End byte position (exclusive).
    pub end: BytePos,
    /// The source file this span belongs to.
    pub source_id: SourceId,
}

impl Span {
    /// Create a new span.
    #[inline]
    pub const fn new(start: BytePos, end: BytePos, source_id: SourceId) -> Self {
        Self {
            start,
            end,
            source_id,
        }
    }

    /// Create a span from byte offsets.
    #[inline]
    pub const fn from_offsets(start: u32, end: u32, source_id: SourceId) -> Self {
        Self {
            start: BytePos(start),
            end: BytePos(end),
            source_id,
        }
    }

    /// Create a dummy span (for synthetic nodes).
    #[inline]
    pub const fn dummy() -> Self {
        Self {
            start: BytePos(0),
            end: BytePos(0),
            source_id: SourceId::ANONYMOUS,
        }
    }

    /// Create a span that covers a single byte position.
    #[inline]
    pub const fn point(pos: BytePos, source_id: SourceId) -> Self {
        Self {
            start: pos,
            end: BytePos(pos.0 + 1),
            source_id,
        }
    }

    /// Get the length of this span in bytes.
    #[inline]
    pub const fn len(&self) -> u32 {
        self.end.0.saturating_sub(self.start.0)
    }

    /// Check if this span is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.start.0 >= self.end.0
    }

    /// Check if this is a dummy span.
    #[inline]
    pub const fn is_dummy(&self) -> bool {
        self.start.0 == 0 && self.end.0 == 0 && self.source_id.0 == 0
    }

    /// Create a span that covers both this span and another.
    #[inline]
    pub fn merge(&self, other: &Span) -> Self {
        debug_assert_eq!(self.source_id, other.source_id);
        Self {
            start: BytePos(self.start.0.min(other.start.0)),
            end: BytePos(self.end.0.max(other.end.0)),
            source_id: self.source_id,
        }
    }

    /// Create a span from the end of this span to the start of another.
    #[inline]
    pub fn between(&self, other: &Span) -> Self {
        debug_assert_eq!(self.source_id, other.source_id);
        Self {
            start: self.end,
            end: other.start,
            source_id: self.source_id,
        }
    }

    /// Extend this span to include another.
    #[inline]
    pub fn extend_to(&mut self, other: &Span) {
        debug_assert_eq!(self.source_id, other.source_id);
        self.end = BytePos(self.end.0.max(other.end.0));
    }

    /// Convert to a byte range.
    #[inline]
    pub fn to_range(&self) -> Range<usize> {
        self.start.to_usize()..self.end.to_usize()
    }

    /// Check if this span contains a byte position.
    #[inline]
    pub fn contains(&self, pos: BytePos) -> bool {
        pos >= self.start && pos < self.end
    }

    /// Check if this span overlaps with another.
    #[inline]
    pub fn overlaps(&self, other: &Span) -> bool {
        self.source_id == other.source_id && self.start < other.end && other.start < self.end
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// A source file with its content.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Unique identifier for this file.
    pub id: SourceId,
    /// The filename (or "<anonymous>" for inline code).
    pub name: Arc<str>,
    /// The source code content.
    pub source: Arc<str>,
    /// Cached line start positions for efficient line/column lookup.
    line_starts: Vec<BytePos>,
}

impl SourceFile {
    /// Create a new source file.
    pub fn new(name: impl Into<Arc<str>>, source: impl Into<Arc<str>>) -> Self {
        let source: Arc<str> = source.into();
        let line_starts = Self::compute_line_starts(&source);
        Self {
            id: SourceId(1), // Will be assigned properly by SourceMap
            name: name.into(),
            source,
            line_starts,
        }
    }

    /// Create an anonymous source file (for inline code).
    pub fn anonymous(source: impl Into<Arc<str>>) -> Self {
        Self::new("<anonymous>", source)
    }

    /// Get the source code as a string slice.
    #[inline]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Get the filename.
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the length of the source in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.source.len()
    }

    /// Check if the source is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
    }

    /// Get the number of lines in the source.
    #[inline]
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Get the source text for a span.
    pub fn slice(&self, span: Span) -> &str {
        &self.source[span.to_range()]
    }

    /// Look up the line and column for a byte position.
    pub fn lookup_position(&self, pos: BytePos) -> Position {
        let line_index = self.lookup_line(pos);
        let line_start = self.line_starts[line_index];

        // Count Unicode scalar values for column
        let column = self.source[line_start.to_usize()..pos.to_usize()]
            .chars()
            .count() as u32
            + 1;

        Position {
            line: line_index as u32 + 1,
            column,
            offset: pos,
        }
    }

    /// Look up the line index (0-indexed) for a byte position.
    pub fn lookup_line(&self, pos: BytePos) -> usize {
        match self.line_starts.binary_search(&pos) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        }
    }

    /// Get the start position of a line (0-indexed).
    pub fn line_start(&self, line: usize) -> Option<BytePos> {
        self.line_starts.get(line).copied()
    }

    /// Get the source text for a line (0-indexed).
    pub fn line_source(&self, line: usize) -> Option<&str> {
        let start = self.line_starts.get(line)?.to_usize();
        let end = self
            .line_starts
            .get(line + 1)
            .map(|p| p.to_usize())
            .unwrap_or(self.source.len());

        // Trim trailing newline
        let text = &self.source[start..end];
        Some(text.trim_end_matches('\n').trim_end_matches('\r'))
    }

    /// Get start and end positions for a span.
    pub fn span_to_positions(&self, span: Span) -> (Position, Position) {
        (
            self.lookup_position(span.start),
            self.lookup_position(span.end),
        )
    }

    /// Compute line start positions from source.
    fn compute_line_starts(source: &str) -> Vec<BytePos> {
        let mut starts = vec![BytePos(0)];

        for (i, c) in source.char_indices() {
            if c == '\n' {
                starts.push(BytePos::from_usize(i + 1));
            }
        }

        starts
    }
}

impl PartialEq for SourceFile {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for SourceFile {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_lookup() {
        let source = SourceFile::new("test.quanta", "line 1\nline 2\nline 3");

        let pos0 = source.lookup_position(BytePos(0));
        assert_eq!(pos0.line, 1);
        assert_eq!(pos0.column, 1);

        let pos7 = source.lookup_position(BytePos(7));
        assert_eq!(pos7.line, 2);
        assert_eq!(pos7.column, 1);
    }

    #[test]
    fn test_line_source() {
        let source = SourceFile::new("test.quanta", "line 1\nline 2\nline 3");

        assert_eq!(source.line_source(0), Some("line 1"));
        assert_eq!(source.line_source(1), Some("line 2"));
        assert_eq!(source.line_source(2), Some("line 3"));
    }

    #[test]
    fn test_span_merge() {
        let s1 = Span::from_offsets(0, 10, SourceId(1));
        let s2 = Span::from_offsets(5, 15, SourceId(1));
        let merged = s1.merge(&s2);

        assert_eq!(merged.start.0, 0);
        assert_eq!(merged.end.0, 15);
    }

    #[test]
    fn test_unicode_column() {
        let source = SourceFile::new("test.quanta", "let x = 'a'");
        let pos = source.lookup_position(BytePos(8));
        assert_eq!(pos.column, 9); // Position of 'a'
    }
}
