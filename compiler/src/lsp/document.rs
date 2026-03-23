// ===============================================================================
// QUANTALANG LSP DOCUMENT STORE
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Document management for the LSP server.

use super::types::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// =============================================================================
// DOCUMENT
// =============================================================================

/// A document in the store.
#[derive(Debug, Clone)]
pub struct Document {
    /// The document URI.
    pub uri: DocumentUri,
    /// Language ID (e.g., "quanta").
    pub language_id: String,
    /// Version number.
    pub version: i32,
    /// The document content.
    pub content: String,
    /// Line offsets for fast position lookup.
    line_offsets: Vec<usize>,
}

impl Document {
    /// Create a new document.
    pub fn new(uri: DocumentUri, language_id: String, version: i32, content: String) -> Self {
        let line_offsets = compute_line_offsets(&content);
        Self {
            uri,
            language_id,
            version,
            content,
            line_offsets,
        }
    }

    /// Update the document content.
    pub fn update(&mut self, version: i32, content: String) {
        self.version = version;
        self.content = content;
        self.line_offsets = compute_line_offsets(&self.content);
    }

    /// Apply incremental changes.
    pub fn apply_changes(&mut self, version: i32, changes: &[TextDocumentContentChangeEvent]) {
        self.version = version;

        for change in changes {
            if let Some(range) = change.range {
                // Incremental change
                let start = self.offset_at(range.start);
                let end = self.offset_at(range.end);

                if start <= self.content.len() && end <= self.content.len() && start <= end {
                    self.content.replace_range(start..end, &change.text);
                }
            } else {
                // Full content change
                self.content = change.text.clone();
            }
        }

        self.line_offsets = compute_line_offsets(&self.content);
    }

    /// Get byte offset at a position.
    pub fn offset_at(&self, position: Position) -> usize {
        let line = position.line as usize;
        if line >= self.line_offsets.len() {
            return self.content.len();
        }

        let line_start = self.line_offsets[line];
        let line_end = if line + 1 < self.line_offsets.len() {
            self.line_offsets[line + 1]
        } else {
            self.content.len()
        };

        // Convert character offset (UTF-16 code units) to byte offset
        let line_content = &self.content[line_start..line_end];
        let mut byte_offset = 0;
        let mut utf16_offset = 0;
        let target_utf16 = position.character as usize;

        for c in line_content.chars() {
            if utf16_offset >= target_utf16 {
                break;
            }
            utf16_offset += c.len_utf16();
            byte_offset += c.len_utf8();
        }

        line_start + byte_offset
    }

    /// Get position at byte offset.
    pub fn position_at(&self, offset: usize) -> Position {
        let offset = offset.min(self.content.len());

        // Binary search for the line
        let line = match self.line_offsets.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        };

        let line_start = self.line_offsets[line];
        let line_content = &self.content[line_start..offset];

        // Count UTF-16 code units
        let character: u32 = line_content.chars().map(|c| c.len_utf16() as u32).sum();

        Position {
            line: line as u32,
            character,
        }
    }

    /// Get line count.
    pub fn line_count(&self) -> usize {
        self.line_offsets.len()
    }

    /// Get a line's content.
    pub fn line(&self, line: u32) -> Option<&str> {
        let line = line as usize;
        if line >= self.line_offsets.len() {
            return None;
        }

        let start = self.line_offsets[line];
        let end = if line + 1 < self.line_offsets.len() {
            self.line_offsets[line + 1]
        } else {
            self.content.len()
        };

        // Strip trailing newline
        let line_content = &self.content[start..end];
        Some(line_content.trim_end_matches(&['\r', '\n'][..]))
    }

    /// Get word at position.
    pub fn word_at(&self, position: Position) -> Option<(String, Range)> {
        let offset = self.offset_at(position);
        if offset >= self.content.len() {
            return None;
        }

        // Find word boundaries
        let bytes = self.content.as_bytes();

        // Find start
        let mut start = offset;
        while start > 0 && is_word_char(bytes[start - 1]) {
            start -= 1;
        }

        // Find end
        let mut end = offset;
        while end < bytes.len() && is_word_char(bytes[end]) {
            end += 1;
        }

        if start == end {
            return None;
        }

        let word = self.content[start..end].to_string();
        let start_pos = self.position_at(start);
        let end_pos = self.position_at(end);

        Some((word, Range::new(start_pos, end_pos)))
    }

    /// Get text in a range.
    pub fn text_in_range(&self, range: Range) -> &str {
        let start = self.offset_at(range.start);
        let end = self.offset_at(range.end);
        &self.content[start..end]
    }
}

/// Check if a byte is part of a word.
fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Compute line offsets for a string.
fn compute_line_offsets(content: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    for (i, c) in content.char_indices() {
        if c == '\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}

// =============================================================================
// DOCUMENT STORE
// =============================================================================

/// Thread-safe document store.
#[derive(Debug)]
pub struct DocumentStore {
    /// Documents by URI.
    documents: RwLock<HashMap<DocumentUri, Arc<Document>>>,
}

impl DocumentStore {
    /// Create a new document store.
    pub fn new() -> Self {
        Self {
            documents: RwLock::new(HashMap::new()),
        }
    }

    /// Open a document.
    pub fn open(&self, item: TextDocumentItem) -> Arc<Document> {
        let doc = Arc::new(Document::new(
            item.uri.clone(),
            item.language_id,
            item.version,
            item.text,
        ));
        self.documents.write().unwrap().insert(item.uri, doc.clone());
        doc
    }

    /// Update a document.
    pub fn update(
        &self,
        uri: &DocumentUri,
        version: i32,
        changes: &[TextDocumentContentChangeEvent],
    ) -> Option<Arc<Document>> {
        let mut docs = self.documents.write().unwrap();
        if let Some(doc) = docs.get_mut(uri) {
            let mut new_doc = (**doc).clone();
            new_doc.apply_changes(version, changes);
            let new_doc = Arc::new(new_doc);
            docs.insert(uri.clone(), new_doc.clone());
            Some(new_doc)
        } else {
            None
        }
    }

    /// Close a document.
    pub fn close(&self, uri: &DocumentUri) -> Option<Arc<Document>> {
        self.documents.write().unwrap().remove(uri)
    }

    /// Get a document.
    pub fn get(&self, uri: &DocumentUri) -> Option<Arc<Document>> {
        self.documents.read().unwrap().get(uri).cloned()
    }

    /// Check if a document exists.
    pub fn contains(&self, uri: &DocumentUri) -> bool {
        self.documents.read().unwrap().contains_key(uri)
    }

    /// Get all document URIs.
    pub fn uris(&self) -> Vec<DocumentUri> {
        self.documents.read().unwrap().keys().cloned().collect()
    }

    /// Get document count.
    pub fn len(&self) -> usize {
        self.documents.read().unwrap().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.documents.read().unwrap().is_empty()
    }
}

impl Default for DocumentStore {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// ROPE (for large documents)
// =============================================================================

/// A simple rope implementation for efficient text editing.
/// Uses a tree of chunks for O(log n) modifications.
#[derive(Debug, Clone)]
pub struct Rope {
    /// Root node.
    root: RopeNode,
}

#[derive(Debug, Clone)]
enum RopeNode {
    /// Leaf node containing actual text.
    Leaf(String),
    /// Internal node with left and right children.
    Branch {
        left: Box<RopeNode>,
        right: Box<RopeNode>,
        len: usize,
    },
}

impl Rope {
    /// Maximum leaf size.
    const MAX_LEAF_SIZE: usize = 512;

    /// Create a new rope from a string.
    pub fn new(text: &str) -> Self {
        Self {
            root: Self::build_tree(text),
        }
    }

    fn build_tree(text: &str) -> RopeNode {
        if text.len() <= Self::MAX_LEAF_SIZE {
            RopeNode::Leaf(text.to_string())
        } else {
            let mid = text.len() / 2;
            // Find a safe split point (don't split UTF-8 sequences)
            let mid = text
                .char_indices()
                .take_while(|(i, _)| *i < mid)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(0);

            let (left, right) = text.split_at(mid);
            RopeNode::Branch {
                left: Box::new(Self::build_tree(left)),
                right: Box::new(Self::build_tree(right)),
                len: text.len(),
            }
        }
    }

    /// Get the total length.
    pub fn len(&self) -> usize {
        self.root.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get a character at an index.
    pub fn char_at(&self, index: usize) -> Option<char> {
        self.root.char_at(index)
    }

    /// Get a slice of the rope.
    pub fn slice(&self, start: usize, end: usize) -> String {
        let mut result = String::with_capacity(end - start);
        self.root.collect_range(start, end, &mut result);
        result
    }

    /// Insert text at a position.
    pub fn insert(&mut self, pos: usize, text: &str) {
        let current = self.slice(0, self.len());
        let mut new_content = String::with_capacity(current.len() + text.len());
        new_content.push_str(&current[..pos.min(current.len())]);
        new_content.push_str(text);
        if pos < current.len() {
            new_content.push_str(&current[pos..]);
        }
        self.root = Self::build_tree(&new_content);
    }

    /// Delete a range.
    pub fn delete(&mut self, start: usize, end: usize) {
        let current = self.slice(0, self.len());
        let mut new_content = String::with_capacity(current.len() - (end - start));
        new_content.push_str(&current[..start.min(current.len())]);
        if end < current.len() {
            new_content.push_str(&current[end..]);
        }
        self.root = Self::build_tree(&new_content);
    }

    /// Convert to string.
    pub fn to_string(&self) -> String {
        self.slice(0, self.len())
    }
}

impl RopeNode {
    fn len(&self) -> usize {
        match self {
            RopeNode::Leaf(s) => s.len(),
            RopeNode::Branch { len, .. } => *len,
        }
    }

    fn char_at(&self, index: usize) -> Option<char> {
        match self {
            RopeNode::Leaf(s) => s.chars().nth(index),
            RopeNode::Branch { left, right, .. } => {
                let left_len = left.len();
                if index < left_len {
                    left.char_at(index)
                } else {
                    right.char_at(index - left_len)
                }
            }
        }
    }

    fn collect_range(&self, start: usize, end: usize, result: &mut String) {
        match self {
            RopeNode::Leaf(s) => {
                let s_len = s.len();
                if start < s_len && end > 0 {
                    let actual_start = start.min(s_len);
                    let actual_end = end.min(s_len);
                    result.push_str(&s[actual_start..actual_end]);
                }
            }
            RopeNode::Branch { left, right, .. } => {
                let left_len = left.len();
                if start < left_len {
                    left.collect_range(start, end.min(left_len), result);
                }
                if end > left_len {
                    let right_start = if start > left_len { start - left_len } else { 0 };
                    right.collect_range(right_start, end - left_len, result);
                }
            }
        }
    }
}

impl From<&str> for Rope {
    fn from(s: &str) -> Self {
        Rope::new(s)
    }
}

impl std::fmt::Display for Rope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_offset_at() {
        let doc = Document::new(
            "file:///test.quanta".to_string(),
            "quanta".to_string(),
            1,
            "hello\nworld\n".to_string(),
        );

        assert_eq!(doc.offset_at(Position::new(0, 0)), 0);
        assert_eq!(doc.offset_at(Position::new(0, 5)), 5);
        assert_eq!(doc.offset_at(Position::new(1, 0)), 6);
        assert_eq!(doc.offset_at(Position::new(1, 5)), 11);
    }

    #[test]
    fn test_document_position_at() {
        let doc = Document::new(
            "file:///test.quanta".to_string(),
            "quanta".to_string(),
            1,
            "hello\nworld\n".to_string(),
        );

        assert_eq!(doc.position_at(0), Position::new(0, 0));
        assert_eq!(doc.position_at(5), Position::new(0, 5));
        assert_eq!(doc.position_at(6), Position::new(1, 0));
        assert_eq!(doc.position_at(11), Position::new(1, 5));
    }

    #[test]
    fn test_document_word_at() {
        let doc = Document::new(
            "file:///test.quanta".to_string(),
            "quanta".to_string(),
            1,
            "fn hello_world() {}".to_string(),
        );

        let (word, range) = doc.word_at(Position::new(0, 5)).unwrap();
        assert_eq!(word, "hello_world");
        assert_eq!(range.start, Position::new(0, 3));
        assert_eq!(range.end, Position::new(0, 14));
    }

    #[test]
    fn test_rope() {
        let mut rope = Rope::new("hello world");
        assert_eq!(rope.len(), 11);
        assert_eq!(rope.slice(0, 5), "hello");
        assert_eq!(rope.slice(6, 11), "world");

        rope.insert(6, "beautiful ");
        assert_eq!(rope.to_string(), "hello beautiful world");

        rope.delete(6, 16);
        assert_eq!(rope.to_string(), "hello world");
    }
}
