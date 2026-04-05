// ===============================================================================
// QUANTALANG LSP DIAGNOSTICS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Diagnostics provider for QuantaLang.
//!
//! Provides both text-pattern-based checks (brackets, syntax) and
//! real type checker diagnostics (undefined variables, type mismatches).

use super::document::{Document, DocumentStore};
use super::message::PublishDiagnosticsParams;
use super::types::*;
use std::sync::Arc;

use crate::lexer::{Lexer, SourceFile};
use crate::parser::Parser;
use crate::types::{TypeChecker, TypeContext};

// =============================================================================
// DIAGNOSTICS PROVIDER
// =============================================================================

/// Provides diagnostics for documents.
pub struct DiagnosticsProvider {
    /// Document store reference.
    documents: Arc<DocumentStore>,
}

impl DiagnosticsProvider {
    /// Create a new diagnostics provider.
    pub fn new(documents: Arc<DocumentStore>) -> Self {
        Self { documents }
    }

    /// Compute diagnostics for a document.
    pub fn compute(&self, doc: &Document) -> PublishDiagnosticsParams {
        let mut diagnostics = Vec::new();

        // Run pattern-based checks (fast, always available)
        self.check_syntax(&doc.content, &mut diagnostics);
        self.check_brackets(&doc.content, doc, &mut diagnostics);

        // Run the real type checker for semantic diagnostics
        self.check_types(&doc.content, &mut diagnostics);

        PublishDiagnosticsParams {
            uri: doc.uri.clone(),
            version: Some(doc.version),
            diagnostics,
        }
    }

    /// Run the full lexer → parser → type checker pipeline and convert
    /// errors to LSP diagnostics with accurate source positions.
    fn check_types(&self, content: &str, diagnostics: &mut Vec<Diagnostic>) {
        let source_file = SourceFile::new("buffer", content.to_string());

        // Lex
        let mut lexer = Lexer::new(&source_file);
        let tokens = match lexer.tokenize() {
            Ok(tokens) => tokens,
            Err(e) => {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 1,
                        },
                    },
                    severity: Some(DiagnosticSeverity::Error),
                    code: None,
                    source: Some("quantalang".to_string()),
                    message: format!("Lexer error: {}", e),
                    tags: Vec::new(),
                    related_information: Vec::new(),
                });
                return;
            }
        };

        // Parse
        let mut parser = Parser::new(&source_file, tokens);
        let ast = match parser.parse() {
            Ok(ast) => ast,
            Err(e) => {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 1,
                        },
                    },
                    severity: Some(DiagnosticSeverity::Error),
                    code: None,
                    source: Some("quantalang".to_string()),
                    message: format!("Parse error: {}", e),
                    tags: Vec::new(),
                    related_information: Vec::new(),
                });
                return;
            }
        };

        // Report parse errors with positions
        for err in parser.errors() {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 0, character: 1 },
                },
                severity: Some(DiagnosticSeverity::Error),
                code: None,
                source: Some("quantalang".to_string()),
                message: format!("{}", err),
                tags: Vec::new(),
                related_information: Vec::new(),
            });
        }

        // Type check
        let mut ctx = TypeContext::new();
        let mut checker = TypeChecker::new(&mut ctx);
        checker.check_module(&ast);

        // Convert type errors to diagnostics with source positions
        for err in checker.errors() {
            let span = err.span;
            let (start_line, start_col, end_line, end_col) =
                if span.start.to_usize() < content.len() {
                    let start_pos = source_file.lookup_position(span.start);
                    let end_pos = source_file.lookup_position(span.end);
                    (
                        start_pos.line.saturating_sub(1),
                        start_pos.column.saturating_sub(1),
                        end_pos.line.saturating_sub(1),
                        end_pos.column.saturating_sub(1),
                    )
                } else {
                    (0, 0, 0, 1)
                };

            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: start_line,
                        character: start_col,
                    },
                    end: Position {
                        line: end_line,
                        character: end_col,
                    },
                },
                severity: Some(DiagnosticSeverity::Error),
                code: None,
                source: Some("quantalang".to_string()),
                message: format!("{}", err),
                tags: Vec::new(),
                related_information: Vec::new(),
            });
        }
    }

    /// Check for syntax issues.
    fn check_syntax(&self, content: &str, diagnostics: &mut Vec<Diagnostic>) {
        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num as u32;
            let trimmed = line.trim();

            // Check for missing semicolons after statements
            if self.needs_semicolon(trimmed)
                && !trimmed.ends_with(';')
                && !trimmed.ends_with('{')
                && !trimmed.ends_with('}')
                && !trimmed.ends_with(',')
            {
                let col = line.len() as u32;
                diagnostics.push(
                    Diagnostic::error(
                        Range::point(Position::new(line_num, col)),
                        "expected ';' at end of statement",
                    )
                    .with_code(1001),
                );
            }

            // Check for double semicolons
            if trimmed.contains(";;") {
                if let Some(pos) = line.find(";;") {
                    diagnostics.push(
                        Diagnostic::warning(
                            Range::new(
                                Position::new(line_num, pos as u32),
                                Position::new(line_num, pos as u32 + 2),
                            ),
                            "unnecessary double semicolon",
                        )
                        .with_tag(DiagnosticTag::Unnecessary),
                    );
                }
            }

            // Check for trailing whitespace
            if line.ends_with(' ') || line.ends_with('\t') {
                let trimmed_len = line.trim_end().len() as u32;
                diagnostics.push(
                    Diagnostic::hint(
                        Range::new(
                            Position::new(line_num, trimmed_len),
                            Position::new(line_num, line.len() as u32),
                        ),
                        "trailing whitespace",
                    )
                    .with_tag(DiagnosticTag::Unnecessary),
                );
            }
        }
    }

    /// Check if a line needs a semicolon.
    fn needs_semicolon(&self, line: &str) -> bool {
        // Lines that typically need semicolons
        let patterns = ["let ", "return ", "break", "continue", ") =", "= "];

        for pattern in &patterns {
            if line.contains(pattern) {
                // But not if it's a function or control flow
                if !line.starts_with("fn ")
                    && !line.starts_with("if ")
                    && !line.starts_with("else")
                    && !line.starts_with("match ")
                    && !line.starts_with("for ")
                    && !line.starts_with("while ")
                    && !line.starts_with("loop")
                    && !line.starts_with("//")
                    && !line.starts_with("/*")
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check for bracket matching.
    fn check_brackets(&self, content: &str, _doc: &Document, diagnostics: &mut Vec<Diagnostic>) {
        let mut stack: Vec<(char, Position)> = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num as u32;
            let mut in_string = false;
            let mut in_char = false;
            let mut escape_next = false;

            for (col, c) in line.chars().enumerate() {
                let col = col as u32;

                if escape_next {
                    escape_next = false;
                    continue;
                }

                if c == '\\' {
                    escape_next = true;
                    continue;
                }

                if c == '"' && !in_char {
                    in_string = !in_string;
                    continue;
                }

                if c == '\'' && !in_string {
                    in_char = !in_char;
                    continue;
                }

                if in_string || in_char {
                    continue;
                }

                match c {
                    '(' | '[' | '{' => {
                        stack.push((c, Position::new(line_num, col)));
                    }
                    ')' => {
                        if let Some((open, _)) = stack.pop() {
                            if open != '(' {
                                diagnostics.push(Diagnostic::error(
                                    Range::point(Position::new(line_num, col)),
                                    format!(
                                        "mismatched bracket: expected closing for '{}', found ')'",
                                        open
                                    ),
                                ));
                            }
                        } else {
                            diagnostics.push(Diagnostic::error(
                                Range::point(Position::new(line_num, col)),
                                "unmatched closing parenthesis",
                            ));
                        }
                    }
                    ']' => {
                        if let Some((open, _)) = stack.pop() {
                            if open != '[' {
                                diagnostics.push(Diagnostic::error(
                                    Range::point(Position::new(line_num, col)),
                                    format!(
                                        "mismatched bracket: expected closing for '{}', found ']'",
                                        open
                                    ),
                                ));
                            }
                        } else {
                            diagnostics.push(Diagnostic::error(
                                Range::point(Position::new(line_num, col)),
                                "unmatched closing bracket",
                            ));
                        }
                    }
                    '}' => {
                        if let Some((open, _)) = stack.pop() {
                            if open != '{' {
                                diagnostics.push(Diagnostic::error(
                                    Range::point(Position::new(line_num, col)),
                                    format!(
                                        "mismatched bracket: expected closing for '{}', found '}}'",
                                        open
                                    ),
                                ));
                            }
                        } else {
                            diagnostics.push(Diagnostic::error(
                                Range::point(Position::new(line_num, col)),
                                "unmatched closing brace",
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }

        // Report unclosed brackets
        for (open, pos) in stack {
            let expected = match open {
                '(' => ')',
                '[' => ']',
                '{' => '}',
                _ => '?',
            };
            diagnostics.push(Diagnostic::error(
                Range::point(pos),
                format!("unclosed '{}', expected '{}'", open, expected),
            ));
        }
    }

    /// Check for common coding issues.
    fn check_common_issues(
        &self,
        content: &str,
        _doc: &Document,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num as u32;
            let trimmed = line.trim();

            // Check for TODO/FIXME/HACK comments
            for marker in &["TODO", "FIXME", "HACK", "XXX"] {
                if let Some(pos) = line.to_uppercase().find(marker) {
                    if line[..pos].contains("//") || line[..pos].contains("/*") {
                        diagnostics.push(Diagnostic::hint(
                            Range::new(
                                Position::new(line_num, pos as u32),
                                Position::new(line_num, pos as u32 + marker.len() as u32),
                            ),
                            format!("{} comment", marker),
                        ));
                    }
                }
            }

            // Check for deprecated patterns
            if trimmed.contains("unwrap()") {
                if let Some(pos) = line.find("unwrap()") {
                    diagnostics.push(Diagnostic::hint(
                        Range::new(
                            Position::new(line_num, pos as u32),
                            Position::new(line_num, pos as u32 + 8),
                        ),
                        "consider using 'expect' or '?' instead of 'unwrap'",
                    ));
                }
            }

            // Check for panic! in non-test code
            if trimmed.contains("panic!(") && !self.is_in_test_function(content, line_num as usize)
            {
                if let Some(pos) = line.find("panic!(") {
                    diagnostics.push(Diagnostic::warning(
                        Range::new(
                            Position::new(line_num, pos as u32),
                            Position::new(line_num, pos as u32 + 7),
                        ),
                        "consider using Result instead of panic!",
                    ));
                }
            }

            // Check for hardcoded values that might be magic numbers
            self.check_magic_numbers(line, line_num, diagnostics);
        }
    }

    /// Check if a line is inside a test function.
    fn is_in_test_function(&self, content: &str, line_num: usize) -> bool {
        let lines: Vec<&str> = content.lines().collect();
        for i in (0..line_num).rev() {
            let trimmed = lines[i].trim();
            if trimmed.starts_with("fn ") {
                // Check if there's a #[test] attribute above
                if i > 0 && lines[i - 1].trim().contains("#[test]") {
                    return true;
                }
                return false;
            }
        }
        false
    }

    /// Check for magic numbers.
    fn check_magic_numbers(&self, line: &str, line_num: u32, diagnostics: &mut Vec<Diagnostic>) {
        // Skip comments
        if line.trim().starts_with("//") {
            return;
        }

        // Look for numeric literals that might be magic numbers
        let allowed_numbers = ["0", "1", "2", "-1", "0.0", "1.0"];

        let mut chars = line.chars().peekable();
        let mut col = 0;

        while let Some(c) = chars.next() {
            if c.is_ascii_digit() {
                let start = col;
                let mut num_str = String::new();
                num_str.push(c);

                while let Some(&next) = chars.peek() {
                    if next.is_ascii_digit() || next == '.' || next == '_' {
                        num_str.push(chars.next().unwrap());
                        col += 1;
                    } else {
                        break;
                    }
                }

                // Skip if it's an allowed number
                let clean_num: String = num_str.chars().filter(|c| *c != '_').collect();
                if !allowed_numbers.contains(&clean_num.as_str()) {
                    // Only flag larger numbers
                    if let Ok(n) = clean_num.parse::<i64>() {
                        if n.abs() > 10 {
                            diagnostics.push(Diagnostic::hint(
                                Range::new(
                                    Position::new(line_num, start as u32),
                                    Position::new(line_num, col as u32 + 1),
                                ),
                                "consider extracting magic number to a named constant",
                            ));
                        }
                    }
                }
            }
            col += 1;
        }
    }

    /// Check for unused variables.
    fn check_unused_variables(
        &self,
        content: &str,
        _doc: &Document,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Simple unused variable detection
        // In real implementation, this would use proper scope analysis

        let mut declared_vars: Vec<(String, Position)> = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num as u32;
            let trimmed = line.trim();

            // Find let bindings
            if let Some(rest) = trimmed
                .strip_prefix("let ")
                .or_else(|| trimmed.strip_prefix("let mut "))
            {
                let var_end = rest
                    .find(|c: char| c == ':' || c == '=' || c == ' ')
                    .unwrap_or(rest.len());
                let var_name = rest[..var_end].trim().to_string();

                if !var_name.starts_with('_') && !var_name.is_empty() {
                    if let Some(col) = line.find(&var_name) {
                        declared_vars.push((var_name, Position::new(line_num, col as u32)));
                    }
                }
            }
        }

        // Check if variables are used
        for (var_name, pos) in declared_vars {
            let mut used = false;
            let var_line = pos.line as usize;

            for (line_num, line) in content.lines().enumerate() {
                if line_num <= var_line {
                    continue;
                }

                // Check if variable is used (simple word boundary check)
                let line_without_strings = remove_strings(line);
                if contains_word(&line_without_strings, &var_name) {
                    used = true;
                    break;
                }
            }

            if !used {
                diagnostics.push(
                    Diagnostic::warning(
                        Range::new(
                            pos,
                            Position::new(pos.line, pos.character + var_name.len() as u32),
                        ),
                        format!("unused variable: {}", var_name),
                    )
                    .with_tag(DiagnosticTag::Unnecessary)
                    .with_code(1002),
                );
            }
        }
    }
}

/// Remove string literals from a line.
fn remove_strings(line: &str) -> String {
    let mut result = String::new();
    let mut in_string = false;
    let mut escape_next = false;

    for c in line.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if c == '\\' {
            escape_next = true;
            continue;
        }
        if c == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string {
            result.push(c);
        }
    }
    result
}

/// Check if a line contains a word as a whole word.
fn contains_word(line: &str, word: &str) -> bool {
    let bytes = line.as_bytes();
    let word_bytes = word.as_bytes();

    let mut i = 0;
    while i <= bytes.len().saturating_sub(word_bytes.len()) {
        if &bytes[i..i + word_bytes.len()] == word_bytes {
            // Check word boundaries
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
            let after_ok = i + word_bytes.len() >= bytes.len()
                || (!bytes[i + word_bytes.len()].is_ascii_alphanumeric()
                    && bytes[i + word_bytes.len()] != b'_');

            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_word() {
        assert!(contains_word("let x = foo;", "foo"));
        assert!(!contains_word("let foobar = 1;", "foo"));
        assert!(contains_word("foo + bar", "foo"));
        assert!(contains_word("foo + bar", "bar"));
    }

    #[test]
    fn test_remove_strings() {
        assert_eq!(remove_strings(r#"let x = "hello";"#), "let x = ;");
        assert_eq!(remove_strings(r#"let x = "foo\"bar";"#), "let x = ;");
    }
}
