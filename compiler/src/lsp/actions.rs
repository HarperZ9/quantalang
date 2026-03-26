// ===============================================================================
// QUANTALANG LSP CODE ACTIONS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Code action provider for QuantaLang.

use super::document::Document;
use super::types::*;

// =============================================================================
// CODE ACTION PROVIDER
// =============================================================================

/// Provides code actions (quick fixes, refactorings, etc.).
pub struct CodeActionProvider;

impl CodeActionProvider {
    /// Create a new code action provider.
    pub fn new() -> Self {
        Self
    }

    /// Get code actions for a range.
    pub fn provide(
        &self,
        doc: &Document,
        range: Range,
        diagnostics: &[Diagnostic],
    ) -> Vec<CodeAction> {
        let mut actions = Vec::new();

        // Generate quick fixes for diagnostics
        for diagnostic in diagnostics {
            actions.extend(self.quick_fixes_for_diagnostic(doc, diagnostic));
        }

        // Generate refactoring actions
        actions.extend(self.refactoring_actions(doc, range));

        // Generate source actions
        actions.extend(self.source_actions(doc));

        actions
    }

    /// Generate quick fixes for a diagnostic.
    fn quick_fixes_for_diagnostic(&self, doc: &Document, diagnostic: &Diagnostic) -> Vec<CodeAction> {
        let mut actions = Vec::new();
        let message = &diagnostic.message;

        // Missing semicolon
        if message.contains("expected ';'") {
            actions.push(self.add_semicolon_fix(doc, diagnostic));
        }

        // Unused variable
        if message.contains("unused variable") {
            actions.push(self.prefix_underscore_fix(doc, diagnostic));
            actions.push(self.remove_variable_fix(doc, diagnostic));
        }

        // Mismatched types
        if message.contains("mismatched types") || message.contains("type mismatch") {
            actions.extend(self.type_conversion_fixes(doc, diagnostic));
        }

        // Missing import
        if message.contains("not found in this scope") || message.contains("cannot find") {
            actions.extend(self.import_suggestions(doc, diagnostic));
        }

        // Unused import
        if message.contains("unused import") {
            actions.push(self.remove_import_fix(doc, diagnostic));
        }

        // Dead code
        if message.contains("unreachable") || message.contains("dead code") {
            actions.push(self.remove_dead_code_fix(doc, diagnostic));
        }

        actions
    }

    /// Add semicolon quick fix.
    fn add_semicolon_fix(&self, doc: &Document, diagnostic: &Diagnostic) -> CodeAction {
        let pos = diagnostic.range.end;
        let mut edit = WorkspaceEdit::new();
        edit.add_edit(doc.uri.clone(), TextEdit::insert(pos, ";".to_string()));

        CodeAction::quick_fix("Add missing semicolon")
            .with_edit(edit)
            .preferred()
    }

    /// Prefix with underscore quick fix.
    fn prefix_underscore_fix(&self, doc: &Document, diagnostic: &Diagnostic) -> CodeAction {
        let start = diagnostic.range.start;
        let mut edit = WorkspaceEdit::new();
        edit.add_edit(doc.uri.clone(), TextEdit::insert(start, "_".to_string()));

        CodeAction::quick_fix("Prefix with underscore").with_edit(edit)
    }

    /// Remove unused variable quick fix.
    fn remove_variable_fix(&self, doc: &Document, diagnostic: &Diagnostic) -> CodeAction {
        // Find the entire let statement
        let line = doc.line(diagnostic.range.start.line).unwrap_or("");
        let start = Position::new(diagnostic.range.start.line, 0);
        let end = Position::new(diagnostic.range.start.line, line.len() as u32);

        let mut edit = WorkspaceEdit::new();
        edit.add_edit(
            doc.uri.clone(),
            TextEdit::delete(Range::new(start, end)),
        );

        CodeAction::quick_fix("Remove unused variable").with_edit(edit)
    }

    /// Type conversion quick fixes.
    fn type_conversion_fixes(&self, doc: &Document, diagnostic: &Diagnostic) -> Vec<CodeAction> {
        let mut actions = Vec::new();

        // Common type conversions
        let conversions = [
            ("as i32", "Convert to i32"),
            ("as i64", "Convert to i64"),
            ("as u32", "Convert to u32"),
            ("as u64", "Convert to u64"),
            ("as usize", "Convert to usize"),
            ("as f32", "Convert to f32"),
            ("as f64", "Convert to f64"),
            (".to_string()", "Convert to String"),
            (".as_str()", "Convert to &str"),
        ];

        for (suffix, title) in conversions {
            let end = diagnostic.range.end;
            let mut edit = WorkspaceEdit::new();
            edit.add_edit(
                doc.uri.clone(),
                TextEdit::insert(end, format!(" {}", suffix)),
            );
            actions.push(CodeAction::quick_fix(title).with_edit(edit));
        }

        actions
    }

    /// Import suggestion quick fixes.
    fn import_suggestions(&self, doc: &Document, diagnostic: &Diagnostic) -> Vec<CodeAction> {
        let mut actions = Vec::new();

        // Extract the name from diagnostic range
        let name = doc.text_in_range(diagnostic.range);

        // Common imports
        let suggestions = get_import_suggestions(&name);

        for suggestion in suggestions {
            let import_line = format!("use {};\n", suggestion);
            let mut edit = WorkspaceEdit::new();
            edit.add_edit(
                doc.uri.clone(),
                TextEdit::insert(Position::new(0, 0), import_line),
            );
            actions.push(
                CodeAction::quick_fix(format!("Import {}", suggestion))
                    .with_edit(edit),
            );
        }

        actions
    }

    /// Remove unused import quick fix.
    fn remove_import_fix(&self, doc: &Document, diagnostic: &Diagnostic) -> CodeAction {
        let line = diagnostic.range.start.line;
        let _line_text = doc.line(line).unwrap_or("");
        let start = Position::new(line, 0);
        // Include the newline
        let end = Position::new(line + 1, 0);

        let mut edit = WorkspaceEdit::new();
        edit.add_edit(doc.uri.clone(), TextEdit::delete(Range::new(start, end)));

        CodeAction::quick_fix("Remove unused import")
            .with_edit(edit)
            .preferred()
    }

    /// Remove dead code quick fix.
    fn remove_dead_code_fix(&self, doc: &Document, diagnostic: &Diagnostic) -> CodeAction {
        let mut edit = WorkspaceEdit::new();
        edit.add_edit(doc.uri.clone(), TextEdit::delete(diagnostic.range));

        CodeAction::quick_fix("Remove dead code").with_edit(edit)
    }

    /// Generate refactoring actions.
    fn refactoring_actions(&self, doc: &Document, range: Range) -> Vec<CodeAction> {
        let mut actions = Vec::new();

        // Only offer refactorings if there's a selection
        if range.start != range.end {
            let selected_text = doc.text_in_range(range);

            // Extract variable
            if !selected_text.trim().is_empty() && !selected_text.contains('\n') {
                actions.push(self.extract_variable_action(doc, range, &selected_text));
            }

            // Extract function
            if selected_text.contains('\n') || selected_text.len() > 50 {
                actions.push(self.extract_function_action(doc, range));
            }
        }

        // Inline variable (if cursor is on a variable)
        // This would require more context analysis

        actions
    }

    /// Extract variable refactoring.
    fn extract_variable_action(&self, doc: &Document, range: Range, text: &str) -> CodeAction {
        // Create a placeholder for the new variable
        let var_name = "extracted";
        let let_stmt = format!("let {} = {};\n    ", var_name, text.trim());

        let mut edit = WorkspaceEdit::new();

        // Insert let statement before the line
        let line_start = Position::new(range.start.line, 0);
        let indent = doc.line(range.start.line)
            .map(|l| l.len() - l.trim_start().len())
            .unwrap_or(0);
        let indented_let = format!("{}{}", " ".repeat(indent), let_stmt);
        edit.add_edit(doc.uri.clone(), TextEdit::insert(line_start, indented_let));

        // Replace selection with variable name
        edit.add_edit(doc.uri.clone(), TextEdit::replace(range, var_name.to_string()));

        CodeAction::new("Extract to variable")
            .with_kind(CodeActionKind::refactor_extract())
            .with_edit(edit)
    }

    /// Extract function refactoring.
    fn extract_function_action(&self, doc: &Document, range: Range) -> CodeAction {
        let selected_text = doc.text_in_range(range);
        let fn_body = selected_text.trim();

        // Create a new function
        let fn_def = format!(
            "\nfn extracted() {{\n    {}\n}}\n",
            fn_body.replace('\n', "\n    ")
        );

        let mut edit = WorkspaceEdit::new();

        // Insert function at end of file
        let last_line = doc.line_count().saturating_sub(1) as u32;
        let end_pos = Position::new(last_line, doc.line(last_line).map(|l| l.len() as u32).unwrap_or(0));
        edit.add_edit(doc.uri.clone(), TextEdit::insert(end_pos, fn_def));

        // Replace selection with function call
        edit.add_edit(doc.uri.clone(), TextEdit::replace(range, "extracted()".to_string()));

        CodeAction::new("Extract to function")
            .with_kind(CodeActionKind::refactor_extract())
            .with_edit(edit)
    }

    /// Generate source actions (organize imports, etc.).
    fn source_actions(&self, doc: &Document) -> Vec<CodeAction> {
        let mut actions = Vec::new();

        // Organize imports
        if let Some(edit) = self.organize_imports(doc) {
            actions.push(
                CodeAction::new("Organize imports")
                    .with_kind(CodeActionKind::organize_imports())
                    .with_edit(edit),
            );
        }

        // Add missing derive
        actions.push(
            CodeAction::new("Add #[derive(Debug)]")
                .with_kind(CodeActionKind::source()),
        );

        actions
    }

    /// Organize imports in a document.
    fn organize_imports(&self, doc: &Document) -> Option<WorkspaceEdit> {
        let content = &doc.content;
        let lines: Vec<&str> = content.lines().collect();

        // Find all use statements
        let mut imports: Vec<(usize, String)> = Vec::new();
        let mut first_import_line: Option<usize> = None;
        let mut last_import_line: Option<usize> = None;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("use ") {
                if first_import_line.is_none() {
                    first_import_line = Some(i);
                }
                last_import_line = Some(i);
                imports.push((i, line.to_string()));
            }
        }

        if imports.len() <= 1 {
            return None;
        }

        // Sort imports
        imports.sort_by(|a, b| {
            let a_trimmed = a.1.trim();
            let b_trimmed = b.1.trim();

            // std comes first, then external crates, then local
            let a_priority = import_priority(a_trimmed);
            let b_priority = import_priority(b_trimmed);

            a_priority.cmp(&b_priority).then_with(|| a_trimmed.cmp(b_trimmed))
        });

        let first = first_import_line?;
        let last = last_import_line?;

        // Build new import block
        let mut new_imports = String::new();
        for (_, import) in &imports {
            new_imports.push_str(import.trim());
            new_imports.push('\n');
        }

        let mut edit = WorkspaceEdit::new();
        edit.add_edit(
            doc.uri.clone(),
            TextEdit::replace(
                Range::new(
                    Position::new(first as u32, 0),
                    Position::new(last as u32 + 1, 0),
                ),
                new_imports,
            ),
        );

        Some(edit)
    }
}

impl Default for CodeActionProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Get import priority for sorting.
fn import_priority(import: &str) -> u8 {
    if import.contains("std::") {
        0 // std first
    } else if import.contains("crate::") || import.contains("self::") || import.contains("super::") {
        2 // local last
    } else {
        1 // external crates middle
    }
}

/// Get import suggestions for a name.
fn get_import_suggestions(name: &str) -> Vec<String> {
    let suggestions: Vec<(&str, &str)> = vec![
        ("HashMap", "std::collections::HashMap"),
        ("HashSet", "std::collections::HashSet"),
        ("BTreeMap", "std::collections::BTreeMap"),
        ("BTreeSet", "std::collections::BTreeSet"),
        ("VecDeque", "std::collections::VecDeque"),
        ("LinkedList", "std::collections::LinkedList"),
        ("BinaryHeap", "std::collections::BinaryHeap"),
        ("Vec", "std::vec::Vec"),
        ("String", "std::string::String"),
        ("Arc", "std::sync::Arc"),
        ("Mutex", "std::sync::Mutex"),
        ("RwLock", "std::sync::RwLock"),
        ("Rc", "std::rc::Rc"),
        ("RefCell", "std::cell::RefCell"),
        ("Cell", "std::cell::Cell"),
        ("Path", "std::path::Path"),
        ("PathBuf", "std::path::PathBuf"),
        ("File", "std::fs::File"),
        ("Read", "std::io::Read"),
        ("Write", "std::io::Write"),
        ("BufReader", "std::io::BufReader"),
        ("BufWriter", "std::io::BufWriter"),
        ("Duration", "std::time::Duration"),
        ("Instant", "std::time::Instant"),
        ("SystemTime", "std::time::SystemTime"),
        ("thread", "std::thread"),
        ("fmt", "std::fmt"),
        ("Display", "std::fmt::Display"),
        ("Debug", "std::fmt::Debug"),
        ("Default", "std::default::Default"),
        ("Clone", "std::clone::Clone"),
        ("Copy", "std::marker::Copy"),
        ("Send", "std::marker::Send"),
        ("Sync", "std::marker::Sync"),
        ("Iterator", "std::iter::Iterator"),
        ("IntoIterator", "std::iter::IntoIterator"),
        ("FromIterator", "std::iter::FromIterator"),
    ];

    suggestions
        .into_iter()
        .filter(|(n, _)| *n == name)
        .map(|(_, path)| path.to_string())
        .collect()
}
