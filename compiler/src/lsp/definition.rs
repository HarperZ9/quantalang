// ===============================================================================
// QUANTALANG LSP DEFINITION
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Go to definition provider for QuantaLang.

use super::document::{Document, DocumentStore};
use super::types::*;
use std::sync::Arc;

// =============================================================================
// DEFINITION PROVIDER
// =============================================================================

/// Provides go-to-definition functionality.
pub struct DefinitionProvider {
    /// Document store reference.
    documents: Arc<DocumentStore>,
}

impl DefinitionProvider {
    /// Create a new definition provider.
    pub fn new(documents: Arc<DocumentStore>) -> Self {
        Self { documents }
    }

    /// Find definition of symbol at position.
    pub fn find_definition(&self, doc: &Document, position: Position) -> Vec<Location> {
        let Some((word, _range)) = doc.word_at(position) else {
            return Vec::new();
        };

        let mut locations = Vec::new();

        // Search in current document
        if let Some(loc) = self.find_definition_in_doc(doc, &word) {
            locations.push(loc);
        }

        // Search in other open documents
        for uri in self.documents.uris() {
            if uri != doc.uri {
                if let Some(other_doc) = self.documents.get(&uri) {
                    if let Some(loc) = self.find_definition_in_doc(&other_doc, &word) {
                        locations.push(loc);
                    }
                }
            }
        }

        locations
    }

    /// Find definition in a single document.
    fn find_definition_in_doc(&self, doc: &Document, name: &str) -> Option<Location> {
        let content = &doc.content;

        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            // Function definition
            if let Some(loc) = self.match_function_def(doc, line, line_num, name) {
                return Some(loc);
            }

            // Struct definition
            if let Some(loc) = self.match_struct_def(doc, trimmed, line, line_num, name) {
                return Some(loc);
            }

            // Enum definition
            if let Some(loc) = self.match_enum_def(doc, trimmed, line, line_num, name) {
                return Some(loc);
            }

            // Trait definition
            if let Some(loc) = self.match_trait_def(doc, trimmed, line, line_num, name) {
                return Some(loc);
            }

            // Const definition
            if let Some(loc) = self.match_const_def(doc, trimmed, line, line_num, name) {
                return Some(loc);
            }

            // Type alias
            if let Some(loc) = self.match_type_alias(doc, trimmed, line, line_num, name) {
                return Some(loc);
            }
        }

        None
    }

    /// Match function definition.
    fn match_function_def(
        &self,
        doc: &Document,
        line: &str,
        line_num: usize,
        name: &str,
    ) -> Option<Location> {
        let trimmed = line.trim();
        let rest = trimmed
            .strip_prefix("pub ")
            .or_else(|| trimmed.strip_prefix("async "))
            .or_else(|| trimmed.strip_prefix("pub async "))
            .unwrap_or(trimmed);

        let rest = rest.strip_prefix("fn ")?;
        let paren_pos = rest.find('(')?;
        let fn_name = rest[..paren_pos].trim();

        if fn_name == name {
            let col = line.find(name)?;
            return Some(Location::new(
                doc.uri.clone(),
                Range::new(
                    Position::new(line_num as u32, col as u32),
                    Position::new(line_num as u32, (col + name.len()) as u32),
                ),
            ));
        }
        None
    }

    /// Match struct definition.
    fn match_struct_def(
        &self,
        doc: &Document,
        trimmed: &str,
        line: &str,
        line_num: usize,
        name: &str,
    ) -> Option<Location> {
        let rest = trimmed
            .strip_prefix("pub ")
            .unwrap_or(trimmed)
            .strip_prefix("struct ")?;

        let name_end = rest
            .find(|c: char| c == '<' || c == '{' || c == '(' || c == ' ')
            .unwrap_or(rest.len());
        let struct_name = rest[..name_end].trim();

        if struct_name == name {
            let col = line.find(name)?;
            return Some(Location::new(
                doc.uri.clone(),
                Range::new(
                    Position::new(line_num as u32, col as u32),
                    Position::new(line_num as u32, (col + name.len()) as u32),
                ),
            ));
        }
        None
    }

    /// Match enum definition.
    fn match_enum_def(
        &self,
        doc: &Document,
        trimmed: &str,
        line: &str,
        line_num: usize,
        name: &str,
    ) -> Option<Location> {
        let rest = trimmed
            .strip_prefix("pub ")
            .unwrap_or(trimmed)
            .strip_prefix("enum ")?;

        let name_end = rest
            .find(|c: char| c == '<' || c == '{' || c == ' ')
            .unwrap_or(rest.len());
        let enum_name = rest[..name_end].trim();

        if enum_name == name {
            let col = line.find(name)?;
            return Some(Location::new(
                doc.uri.clone(),
                Range::new(
                    Position::new(line_num as u32, col as u32),
                    Position::new(line_num as u32, (col + name.len()) as u32),
                ),
            ));
        }
        None
    }

    /// Match trait definition.
    fn match_trait_def(
        &self,
        doc: &Document,
        trimmed: &str,
        line: &str,
        line_num: usize,
        name: &str,
    ) -> Option<Location> {
        let rest = trimmed
            .strip_prefix("pub ")
            .unwrap_or(trimmed)
            .strip_prefix("trait ")?;

        let name_end = rest
            .find(|c: char| c == '<' || c == '{' || c == ':' || c == ' ')
            .unwrap_or(rest.len());
        let trait_name = rest[..name_end].trim();

        if trait_name == name {
            let col = line.find(name)?;
            return Some(Location::new(
                doc.uri.clone(),
                Range::new(
                    Position::new(line_num as u32, col as u32),
                    Position::new(line_num as u32, (col + name.len()) as u32),
                ),
            ));
        }
        None
    }

    /// Match const definition.
    fn match_const_def(
        &self,
        doc: &Document,
        trimmed: &str,
        line: &str,
        line_num: usize,
        name: &str,
    ) -> Option<Location> {
        let rest = trimmed
            .strip_prefix("pub ")
            .unwrap_or(trimmed)
            .strip_prefix("const ")?;

        let colon_pos = rest.find(':')?;
        let const_name = rest[..colon_pos].trim();

        if const_name == name {
            let col = line.find(name)?;
            return Some(Location::new(
                doc.uri.clone(),
                Range::new(
                    Position::new(line_num as u32, col as u32),
                    Position::new(line_num as u32, (col + name.len()) as u32),
                ),
            ));
        }
        None
    }

    /// Match type alias.
    fn match_type_alias(
        &self,
        doc: &Document,
        trimmed: &str,
        line: &str,
        line_num: usize,
        name: &str,
    ) -> Option<Location> {
        let rest = trimmed
            .strip_prefix("pub ")
            .unwrap_or(trimmed)
            .strip_prefix("type ")?;

        let eq_pos = rest.find('=')?;
        let name_part = rest[..eq_pos].trim();
        let name_end = name_part.find('<').unwrap_or(name_part.len());
        let type_name = name_part[..name_end].trim();

        if type_name == name {
            let col = line.find(name)?;
            return Some(Location::new(
                doc.uri.clone(),
                Range::new(
                    Position::new(line_num as u32, col as u32),
                    Position::new(line_num as u32, (col + name.len()) as u32),
                ),
            ));
        }
        None
    }

    /// Find all references to a symbol.
    pub fn find_references(
        &self,
        doc: &Document,
        position: Position,
        include_definition: bool,
    ) -> Vec<Location> {
        let Some((word, _range)) = doc.word_at(position) else {
            return Vec::new();
        };

        let mut locations = Vec::new();

        // Search in all open documents
        for uri in self.documents.uris() {
            if let Some(search_doc) = self.documents.get(&uri) {
                self.find_references_in_doc(&search_doc, &word, include_definition, &mut locations);
            }
        }

        locations
    }

    /// Find references in a document.
    fn find_references_in_doc(
        &self,
        doc: &Document,
        name: &str,
        include_definition: bool,
        locations: &mut Vec<Location>,
    ) {
        let content = &doc.content;

        for (line_num, line) in content.lines().enumerate() {
            // Skip if this is a definition and we don't want definitions
            if !include_definition {
                let trimmed = line.trim();
                if trimmed.starts_with("fn ")
                    || trimmed.starts_with("pub fn ")
                    || trimmed.starts_with("struct ")
                    || trimmed.starts_with("pub struct ")
                    || trimmed.starts_with("enum ")
                    || trimmed.starts_with("pub enum ")
                    || trimmed.starts_with("trait ")
                    || trimmed.starts_with("pub trait ")
                {
                    // Check if this line defines the symbol
                    if line.contains(&format!("fn {}", name))
                        || line.contains(&format!("struct {} ", name))
                        || line.contains(&format!("struct {}<", name))
                        || line.contains(&format!("struct {}{{", name))
                        || line.contains(&format!("enum {} ", name))
                        || line.contains(&format!("enum {}<", name))
                        || line.contains(&format!("enum {}{{", name))
                        || line.contains(&format!("trait {} ", name))
                        || line.contains(&format!("trait {}<", name))
                        || line.contains(&format!("trait {}{{", name))
                    {
                        continue;
                    }
                }
            }

            // Find all occurrences of the word
            let mut search_pos = 0;
            while let Some(pos) = line[search_pos..].find(name) {
                let abs_pos = search_pos + pos;

                // Check word boundaries
                let before_ok = abs_pos == 0
                    || !line.as_bytes()[abs_pos - 1].is_ascii_alphanumeric()
                        && line.as_bytes()[abs_pos - 1] != b'_';
                let after_pos = abs_pos + name.len();
                let after_ok = after_pos >= line.len()
                    || !line.as_bytes()[after_pos].is_ascii_alphanumeric()
                        && line.as_bytes()[after_pos] != b'_';

                if before_ok && after_ok {
                    locations.push(Location::new(
                        doc.uri.clone(),
                        Range::new(
                            Position::new(line_num as u32, abs_pos as u32),
                            Position::new(line_num as u32, after_pos as u32),
                        ),
                    ));
                }

                search_pos = abs_pos + name.len();
            }
        }
    }

    /// Find type definition (for go to type definition).
    pub fn find_type_definition(&self, doc: &Document, position: Position) -> Vec<Location> {
        // For now, this is the same as find_definition
        // In a full implementation, this would track through variable types
        self.find_definition(doc, position)
    }

    /// Find implementations of a trait or type.
    pub fn find_implementations(&self, doc: &Document, position: Position) -> Vec<Location> {
        let Some((word, _range)) = doc.word_at(position) else {
            return Vec::new();
        };

        let mut locations = Vec::new();

        // Search for impl blocks
        for uri in self.documents.uris() {
            if let Some(search_doc) = self.documents.get(&uri) {
                self.find_impl_blocks(&search_doc, &word, &mut locations);
            }
        }

        locations
    }

    /// Find impl blocks for a type or trait.
    fn find_impl_blocks(&self, doc: &Document, name: &str, locations: &mut Vec<Location>) {
        let content = &doc.content;

        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            if let Some(rest) = trimmed.strip_prefix("impl") {
                // Check if this implements the type/trait
                let implements_type = rest.contains(&format!(" {} ", name))
                    || rest.contains(&format!(" {}<", name))
                    || rest.contains(&format!(" {}{{", name))
                    || rest.contains(&format!(" for {} ", name))
                    || rest.contains(&format!(" for {}<", name))
                    || rest.contains(&format!(" for {}{{", name))
                    || rest.trim().starts_with(name);

                if implements_type {
                    if let Some(col) = line.find("impl") {
                        locations.push(Location::new(
                            doc.uri.clone(),
                            Range::new(
                                Position::new(line_num as u32, col as u32),
                                Position::new(line_num as u32, line.len() as u32),
                            ),
                        ));
                    }
                }
            }
        }
    }
}
