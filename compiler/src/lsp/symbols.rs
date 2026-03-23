// ===============================================================================
// QUANTALANG LSP SYMBOLS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Document symbol provider for QuantaLang.

use super::document::{Document, DocumentStore};
use super::types::*;
use std::sync::Arc;

// =============================================================================
// SYMBOL PROVIDER
// =============================================================================

/// Provides document and workspace symbols.
pub struct SymbolProvider {
    /// Document store reference.
    documents: Arc<DocumentStore>,
}

impl SymbolProvider {
    /// Create a new symbol provider.
    pub fn new(documents: Arc<DocumentStore>) -> Self {
        Self { documents }
    }

    /// Get document symbols (hierarchical).
    pub fn document_symbols(&self, doc: &Document) -> Vec<DocumentSymbol> {
        let mut symbols = Vec::new();
        let lines: Vec<&str> = doc.content.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            if let Some((symbol, end_line)) = self.parse_symbol(&lines, i, doc) {
                symbols.push(symbol);
                i = end_line + 1;
            } else {
                i += 1;
            }
        }

        symbols
    }

    /// Parse a symbol starting at a line.
    fn parse_symbol(&self, lines: &[&str], start: usize, doc: &Document) -> Option<(DocumentSymbol, usize)> {
        let line = lines.get(start)?;
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
            return None;
        }

        // Parse different symbol types
        if let Some(symbol) = self.parse_function(lines, start, doc) {
            return Some(symbol);
        }
        if let Some(symbol) = self.parse_struct(lines, start, doc) {
            return Some(symbol);
        }
        if let Some(symbol) = self.parse_enum(lines, start, doc) {
            return Some(symbol);
        }
        if let Some(symbol) = self.parse_trait(lines, start, doc) {
            return Some(symbol);
        }
        if let Some(symbol) = self.parse_impl(lines, start, doc) {
            return Some(symbol);
        }
        if let Some(symbol) = self.parse_const(lines, start, doc) {
            return Some(symbol);
        }
        if let Some(symbol) = self.parse_type_alias(lines, start, doc) {
            return Some(symbol);
        }
        if let Some(symbol) = self.parse_module(lines, start, doc) {
            return Some(symbol);
        }

        None
    }

    /// Parse a function definition.
    fn parse_function(&self, lines: &[&str], start: usize, doc: &Document) -> Option<(DocumentSymbol, usize)> {
        let line = lines[start];
        let trimmed = line.trim();

        // Check for function pattern
        let rest = trimmed.strip_prefix("pub ")
            .or_else(|| trimmed.strip_prefix("async "))
            .or_else(|| trimmed.strip_prefix("pub async "))
            .unwrap_or(trimmed);

        let rest = rest.strip_prefix("fn ")?;

        // Extract function name
        let paren_pos = rest.find('(')?;
        let name = rest[..paren_pos].trim().to_string();

        // Get signature (up to opening brace)
        let brace_pos = line.find('{');
        let detail = if let Some(pos) = brace_pos {
            Some(line[..pos].trim().to_string())
        } else {
            Some(line.trim().to_string())
        };

        // Find end of function
        let end_line = if brace_pos.is_some() {
            self.find_block_end(lines, start)
        } else {
            start
        };

        let name_start = line.find(&name).unwrap_or(0) as u32;

        let symbol = DocumentSymbol::new(
            &name,
            SymbolKind::Function,
            Range::new(
                Position::new(start as u32, 0),
                Position::new(end_line as u32, lines[end_line].len() as u32),
            ),
            Range::new(
                Position::new(start as u32, name_start),
                Position::new(start as u32, name_start + name.len() as u32),
            ),
        )
        .with_detail(detail.unwrap_or_default());

        Some((symbol, end_line))
    }

    /// Parse a struct definition.
    fn parse_struct(&self, lines: &[&str], start: usize, doc: &Document) -> Option<(DocumentSymbol, usize)> {
        let line = lines[start];
        let trimmed = line.trim();

        let rest = trimmed.strip_prefix("pub ")
            .unwrap_or(trimmed)
            .strip_prefix("struct ")?;

        // Extract struct name
        let name_end = rest.find(|c: char| c == '<' || c == '{' || c == '(' || c == ' ')
            .unwrap_or(rest.len());
        let name = rest[..name_end].trim().to_string();

        // Find end of struct
        let end_line = if line.contains('{') {
            self.find_block_end(lines, start)
        } else {
            start
        };

        // Parse fields as children
        let mut children = Vec::new();
        if line.contains('{') {
            for i in (start + 1)..end_line {
                if let Some(field) = self.parse_field(lines[i], i as u32) {
                    children.push(field);
                }
            }
        }

        let name_start = line.find(&name).unwrap_or(0) as u32;

        let symbol = DocumentSymbol::new(
            &name,
            SymbolKind::Struct,
            Range::new(
                Position::new(start as u32, 0),
                Position::new(end_line as u32, lines[end_line].len() as u32),
            ),
            Range::new(
                Position::new(start as u32, name_start),
                Position::new(start as u32, name_start + name.len() as u32),
            ),
        )
        .with_children(children);

        Some((symbol, end_line))
    }

    /// Parse a struct field.
    fn parse_field(&self, line: &str, line_num: u32) -> Option<DocumentSymbol> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed == "}" {
            return None;
        }

        let colon_pos = trimmed.find(':')?;
        let name = trimmed[..colon_pos].trim();
        let name = name.strip_prefix("pub ").unwrap_or(name);

        if name.is_empty() {
            return None;
        }

        let name_start = line.find(name).unwrap_or(0) as u32;

        Some(DocumentSymbol::new(
            name,
            SymbolKind::Field,
            Range::new(
                Position::new(line_num, 0),
                Position::new(line_num, line.len() as u32),
            ),
            Range::new(
                Position::new(line_num, name_start),
                Position::new(line_num, name_start + name.len() as u32),
            ),
        ))
    }

    /// Parse an enum definition.
    fn parse_enum(&self, lines: &[&str], start: usize, doc: &Document) -> Option<(DocumentSymbol, usize)> {
        let line = lines[start];
        let trimmed = line.trim();

        let rest = trimmed.strip_prefix("pub ")
            .unwrap_or(trimmed)
            .strip_prefix("enum ")?;

        let name_end = rest.find(|c: char| c == '<' || c == '{' || c == ' ')
            .unwrap_or(rest.len());
        let name = rest[..name_end].trim().to_string();

        let end_line = if line.contains('{') {
            self.find_block_end(lines, start)
        } else {
            start
        };

        // Parse variants as children
        let mut children = Vec::new();
        if line.contains('{') {
            for i in (start + 1)..end_line {
                if let Some(variant) = self.parse_enum_variant(lines[i], i as u32) {
                    children.push(variant);
                }
            }
        }

        let name_start = line.find(&name).unwrap_or(0) as u32;

        let symbol = DocumentSymbol::new(
            &name,
            SymbolKind::Enum,
            Range::new(
                Position::new(start as u32, 0),
                Position::new(end_line as u32, lines[end_line].len() as u32),
            ),
            Range::new(
                Position::new(start as u32, name_start),
                Position::new(start as u32, name_start + name.len() as u32),
            ),
        )
        .with_children(children);

        Some((symbol, end_line))
    }

    /// Parse an enum variant.
    fn parse_enum_variant(&self, line: &str, line_num: u32) -> Option<DocumentSymbol> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed == "}" {
            return None;
        }

        let name_end = trimmed.find(|c: char| c == '(' || c == '{' || c == ',' || c == ' ')
            .unwrap_or(trimmed.len());
        let name = trimmed[..name_end].trim();

        if name.is_empty() {
            return None;
        }

        let name_start = line.find(name).unwrap_or(0) as u32;

        Some(DocumentSymbol::new(
            name,
            SymbolKind::EnumMember,
            Range::new(
                Position::new(line_num, 0),
                Position::new(line_num, line.len() as u32),
            ),
            Range::new(
                Position::new(line_num, name_start),
                Position::new(line_num, name_start + name.len() as u32),
            ),
        ))
    }

    /// Parse a trait definition.
    fn parse_trait(&self, lines: &[&str], start: usize, doc: &Document) -> Option<(DocumentSymbol, usize)> {
        let line = lines[start];
        let trimmed = line.trim();

        let rest = trimmed.strip_prefix("pub ")
            .unwrap_or(trimmed)
            .strip_prefix("trait ")?;

        let name_end = rest.find(|c: char| c == '<' || c == '{' || c == ':' || c == ' ')
            .unwrap_or(rest.len());
        let name = rest[..name_end].trim().to_string();

        let end_line = if line.contains('{') {
            self.find_block_end(lines, start)
        } else {
            start
        };

        // Parse methods as children
        let mut children = Vec::new();
        if line.contains('{') {
            let mut i = start + 1;
            while i < end_line {
                if let Some((method, method_end)) = self.parse_function(lines, i, doc) {
                    children.push(method);
                    i = method_end + 1;
                } else {
                    i += 1;
                }
            }
        }

        let name_start = line.find(&name).unwrap_or(0) as u32;

        let symbol = DocumentSymbol::new(
            &name,
            SymbolKind::Interface,
            Range::new(
                Position::new(start as u32, 0),
                Position::new(end_line as u32, lines[end_line].len() as u32),
            ),
            Range::new(
                Position::new(start as u32, name_start),
                Position::new(start as u32, name_start + name.len() as u32),
            ),
        )
        .with_children(children);

        Some((symbol, end_line))
    }

    /// Parse an impl block.
    fn parse_impl(&self, lines: &[&str], start: usize, doc: &Document) -> Option<(DocumentSymbol, usize)> {
        let line = lines[start];
        let trimmed = line.trim();

        let rest = trimmed.strip_prefix("impl")?;

        // Extract type name
        let brace_pos = rest.find('{').unwrap_or(rest.len());
        let impl_part = rest[..brace_pos].trim();

        // Handle "impl Trait for Type" and "impl Type"
        let name = if impl_part.contains(" for ") {
            let parts: Vec<&str> = impl_part.split(" for ").collect();
            if parts.len() == 2 {
                format!("impl {} for {}", parts[0].trim(), parts[1].trim())
            } else {
                format!("impl {}", impl_part)
            }
        } else {
            format!("impl {}", impl_part.trim_start_matches('<').split('>').last().unwrap_or(impl_part).trim())
        };

        let end_line = if line.contains('{') {
            self.find_block_end(lines, start)
        } else {
            start
        };

        // Parse methods as children
        let mut children = Vec::new();
        if line.contains('{') {
            let mut i = start + 1;
            while i < end_line {
                if let Some((method, method_end)) = self.parse_function(lines, i, doc) {
                    children.push(method);
                    i = method_end + 1;
                } else {
                    i += 1;
                }
            }
        }

        let symbol = DocumentSymbol::new(
            &name,
            SymbolKind::Class,
            Range::new(
                Position::new(start as u32, 0),
                Position::new(end_line as u32, lines[end_line].len() as u32),
            ),
            Range::new(
                Position::new(start as u32, 0),
                Position::new(start as u32, line.len() as u32),
            ),
        )
        .with_children(children);

        Some((symbol, end_line))
    }

    /// Parse a const definition.
    fn parse_const(&self, lines: &[&str], start: usize, doc: &Document) -> Option<(DocumentSymbol, usize)> {
        let line = lines[start];
        let trimmed = line.trim();

        let rest = trimmed.strip_prefix("pub ")
            .unwrap_or(trimmed)
            .strip_prefix("const ")?;

        let colon_pos = rest.find(':')?;
        let name = rest[..colon_pos].trim().to_string();

        let name_start = line.find(&name).unwrap_or(0) as u32;

        let symbol = DocumentSymbol::new(
            &name,
            SymbolKind::Constant,
            Range::new(
                Position::new(start as u32, 0),
                Position::new(start as u32, line.len() as u32),
            ),
            Range::new(
                Position::new(start as u32, name_start),
                Position::new(start as u32, name_start + name.len() as u32),
            ),
        );

        Some((symbol, start))
    }

    /// Parse a type alias.
    fn parse_type_alias(&self, lines: &[&str], start: usize, doc: &Document) -> Option<(DocumentSymbol, usize)> {
        let line = lines[start];
        let trimmed = line.trim();

        let rest = trimmed.strip_prefix("pub ")
            .unwrap_or(trimmed)
            .strip_prefix("type ")?;

        let eq_pos = rest.find('=')?;
        let name_part = rest[..eq_pos].trim();
        let name_end = name_part.find('<').unwrap_or(name_part.len());
        let name = name_part[..name_end].trim().to_string();

        let name_start = line.find(&name).unwrap_or(0) as u32;

        let symbol = DocumentSymbol::new(
            &name,
            SymbolKind::TypeParameter,
            Range::new(
                Position::new(start as u32, 0),
                Position::new(start as u32, line.len() as u32),
            ),
            Range::new(
                Position::new(start as u32, name_start),
                Position::new(start as u32, name_start + name.len() as u32),
            ),
        );

        Some((symbol, start))
    }

    /// Parse a module declaration.
    fn parse_module(&self, lines: &[&str], start: usize, doc: &Document) -> Option<(DocumentSymbol, usize)> {
        let line = lines[start];
        let trimmed = line.trim();

        let rest = trimmed.strip_prefix("pub ")
            .unwrap_or(trimmed)
            .strip_prefix("mod ")?;

        // Check if it's an inline module or a declaration
        if rest.contains('{') {
            let name_end = rest.find(|c: char| c == '{' || c == ' ')
                .unwrap_or(rest.len());
            let name = rest[..name_end].trim().to_string();

            let end_line = self.find_block_end(lines, start);

            // Parse children
            let mut children = Vec::new();
            let mut i = start + 1;
            while i < end_line {
                if let Some((child, child_end)) = self.parse_symbol(lines, i, doc) {
                    children.push(child);
                    i = child_end + 1;
                } else {
                    i += 1;
                }
            }

            let name_start = line.find(&name).unwrap_or(0) as u32;

            let symbol = DocumentSymbol::new(
                &name,
                SymbolKind::Module,
                Range::new(
                    Position::new(start as u32, 0),
                    Position::new(end_line as u32, lines[end_line].len() as u32),
                ),
                Range::new(
                    Position::new(start as u32, name_start),
                    Position::new(start as u32, name_start + name.len() as u32),
                ),
            )
            .with_children(children);

            Some((symbol, end_line))
        } else {
            // Module declaration (mod name;)
            let name_end = rest.find(';').unwrap_or(rest.len());
            let name = rest[..name_end].trim().to_string();

            let name_start = line.find(&name).unwrap_or(0) as u32;

            let symbol = DocumentSymbol::new(
                &name,
                SymbolKind::Module,
                Range::new(
                    Position::new(start as u32, 0),
                    Position::new(start as u32, line.len() as u32),
                ),
                Range::new(
                    Position::new(start as u32, name_start),
                    Position::new(start as u32, name_start + name.len() as u32),
                ),
            );

            Some((symbol, start))
        }
    }

    /// Find the end of a brace-delimited block.
    fn find_block_end(&self, lines: &[&str], start: usize) -> usize {
        let mut depth = 0;

        for i in start..lines.len() {
            for c in lines[i].chars() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        return i;
                    }
                }
            }
        }

        lines.len().saturating_sub(1)
    }

    /// Get workspace symbols matching a query.
    pub fn workspace_symbols(&self, query: &str) -> Vec<SymbolInformation> {
        let mut symbols = Vec::new();
        let query_lower = query.to_lowercase();

        for uri in self.documents.uris() {
            if let Some(doc) = self.documents.get(&uri) {
                let doc_symbols = self.document_symbols(&doc);
                self.collect_matching_symbols(&doc_symbols, &uri, &query_lower, &mut symbols);
            }
        }

        symbols
    }

    /// Collect symbols matching a query recursively.
    fn collect_matching_symbols(
        &self,
        symbols: &[DocumentSymbol],
        uri: &str,
        query: &str,
        result: &mut Vec<SymbolInformation>,
    ) {
        for symbol in symbols {
            if symbol.name.to_lowercase().contains(query) {
                result.push(SymbolInformation {
                    name: symbol.name.clone(),
                    kind: symbol.kind,
                    tags: symbol.tags.clone(),
                    location: Location::new(uri.to_string(), symbol.selection_range),
                    container_name: None,
                });
            }

            // Recurse into children
            self.collect_matching_symbols(&symbol.children, uri, query, result);
        }
    }
}
