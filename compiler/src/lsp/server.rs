// ===============================================================================
// QUANTALANG LSP SERVER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Main LSP server implementation.

use super::completion::CompletionProvider;
use super::diagnostics::DiagnosticsProvider;
use super::document::DocumentStore;
use super::hover::HoverProvider;
use super::message::*;
use super::symbols::SymbolProvider;
use super::transport::*;
use super::types::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// =============================================================================
// SERVER STATE
// =============================================================================

/// Server state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    /// Not initialized.
    Uninitialized,
    /// Initializing.
    Initializing,
    /// Running.
    Running,
    /// Shutting down.
    ShuttingDown,
    /// Shut down.
    Shutdown,
}

// =============================================================================
// LSP SERVER
// =============================================================================

/// The QuantaLang Language Server.
pub struct LanguageServer {
    /// Server state.
    state: ServerState,
    /// Shutdown flag.
    shutdown: AtomicBool,
    /// Document store.
    documents: Arc<DocumentStore>,
    /// Client capabilities.
    client_capabilities: Option<ClientCapabilities>,
    /// Root URI.
    root_uri: Option<DocumentUri>,
    /// Completion provider.
    completion: CompletionProvider,
    /// Hover provider.
    hover: HoverProvider,
    /// Diagnostics provider.
    diagnostics: DiagnosticsProvider,
    /// Symbol provider.
    symbols: SymbolProvider,
}

impl LanguageServer {
    /// Create a new language server.
    pub fn new() -> Self {
        let documents = Arc::new(DocumentStore::new());
        Self {
            state: ServerState::Uninitialized,
            shutdown: AtomicBool::new(false),
            documents: documents.clone(),
            client_capabilities: None,
            root_uri: None,
            completion: CompletionProvider::new(documents.clone()),
            hover: HoverProvider::new(documents.clone()),
            diagnostics: DiagnosticsProvider::new(documents.clone()),
            symbols: SymbolProvider::new(documents.clone()),
        }
    }

    /// Get server state.
    pub fn state(&self) -> ServerState {
        self.state
    }

    /// Check if server should shutdown.
    pub fn should_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    /// Get document store.
    pub fn documents(&self) -> &Arc<DocumentStore> {
        &self.documents
    }

    // =========================================================================
    // LIFECYCLE
    // =========================================================================

    /// Handle initialize request.
    pub fn initialize(&mut self, params: InitializeParams) -> InitializeResult {
        self.state = ServerState::Initializing;
        self.client_capabilities = Some(params.capabilities);
        self.root_uri = params.root_uri;

        InitializeResult {
            capabilities: ServerCapabilities::full(),
            server_info: Some(ServerInfo {
                name: "quantalang-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        }
    }

    /// Handle initialized notification.
    pub fn initialized(&mut self) {
        self.state = ServerState::Running;
    }

    /// Handle shutdown request.
    pub fn shutdown(&mut self) {
        self.state = ServerState::ShuttingDown;
        self.shutdown.store(true, Ordering::Release);
    }

    /// Handle exit notification.
    pub fn exit(&mut self) {
        self.state = ServerState::Shutdown;
    }

    // =========================================================================
    // TEXT DOCUMENT
    // =========================================================================

    /// Handle didOpen notification.
    pub fn did_open(
        &mut self,
        params: DidOpenTextDocumentParams,
    ) -> Option<PublishDiagnosticsParams> {
        let doc = self.documents.open(params.text_document);
        Some(self.diagnostics.compute(&doc))
    }

    /// Handle didChange notification.
    pub fn did_change(
        &mut self,
        params: DidChangeTextDocumentParams,
    ) -> Option<PublishDiagnosticsParams> {
        let doc = self.documents.update(
            &params.text_document.uri,
            params.text_document.version,
            &params.content_changes,
        )?;
        Some(self.diagnostics.compute(&doc))
    }

    /// Handle didSave notification.
    pub fn did_save(
        &mut self,
        params: DidSaveTextDocumentParams,
    ) -> Option<PublishDiagnosticsParams> {
        let doc = self.documents.get(&params.text_document.uri)?;
        Some(self.diagnostics.compute(&doc))
    }

    /// Handle didClose notification.
    pub fn did_close(&mut self, params: DidCloseTextDocumentParams) {
        self.documents.close(&params.text_document.uri);
    }

    // =========================================================================
    // LANGUAGE FEATURES
    // =========================================================================

    /// Handle completion request.
    pub fn completion(&self, params: CompletionParams) -> Option<CompletionList> {
        let doc = self
            .documents
            .get(&params.text_document_position.text_document.uri)?;
        Some(
            self.completion
                .provide(&doc, params.text_document_position.position),
        )
    }

    /// Handle hover request.
    pub fn hover(&self, params: TextDocumentPositionParams) -> Option<Hover> {
        let doc = self.documents.get(&params.text_document.uri)?;
        self.hover.provide(&doc, params.position)
    }

    /// Handle definition request.
    pub fn definition(&self, params: TextDocumentPositionParams) -> Vec<Location> {
        let Some(doc) = self.documents.get(&params.text_document.uri) else {
            return Vec::new();
        };

        // Get the word at the cursor position
        let Some((word, _word_range)) = doc.word_at(params.position) else {
            return Vec::new();
        };

        // Search document symbols for a matching definition
        let doc_symbols = self.symbols.document_symbols(&doc);
        let mut locations = Vec::new();

        self.find_definition_in_symbols(&word, &doc_symbols, &doc.uri, &mut locations);

        // If no symbol found in current document, search workspace
        if locations.is_empty() {
            for uri in self.documents.uris() {
                if uri == doc.uri {
                    continue;
                }
                if let Some(other_doc) = self.documents.get(&uri) {
                    let other_symbols = self.symbols.document_symbols(&other_doc);
                    self.find_definition_in_symbols(&word, &other_symbols, &uri, &mut locations);
                }
            }
        }

        locations
    }

    /// Find definition matching name in symbol tree.
    fn find_definition_in_symbols(
        &self,
        name: &str,
        symbols: &[DocumentSymbol],
        uri: &str,
        locations: &mut Vec<Location>,
    ) {
        for symbol in symbols {
            if symbol.name == name {
                locations.push(Location::new(uri.to_string(), symbol.selection_range));
            }
            // Recurse into children
            self.find_definition_in_symbols(name, &symbol.children, uri, locations);
        }
    }

    /// Handle references request.
    pub fn references(&self, params: TextDocumentPositionParams) -> Vec<Location> {
        let Some(doc) = self.documents.get(&params.text_document.uri) else {
            return Vec::new();
        };

        // Get the word at the cursor position
        let Some((word, _word_range)) = doc.word_at(params.position) else {
            return Vec::new();
        };

        let mut locations = Vec::new();

        // Find all occurrences in current document
        self.find_references_in_document(&word, &doc, &mut locations);

        // Search all workspace documents
        for uri in self.documents.uris() {
            if uri == doc.uri {
                continue; // Already searched
            }
            if let Some(other_doc) = self.documents.get(&uri) {
                self.find_references_in_document(&word, &other_doc, &mut locations);
            }
        }

        locations
    }

    /// Find all references to a word in a document.
    fn find_references_in_document(
        &self,
        word: &str,
        doc: &super::document::Document,
        locations: &mut Vec<Location>,
    ) {
        let content = &doc.content;
        let mut offset = 0;

        while let Some(pos) = content[offset..].find(word) {
            let abs_pos = offset + pos;

            // Check word boundaries to avoid matching substrings
            let before_ok = abs_pos == 0
                || !content.as_bytes()[abs_pos - 1].is_ascii_alphanumeric()
                    && content.as_bytes()[abs_pos - 1] != b'_';
            let after_pos = abs_pos + word.len();
            let after_ok = after_pos >= content.len()
                || !content.as_bytes()[after_pos].is_ascii_alphanumeric()
                    && content.as_bytes()[after_pos] != b'_';

            if before_ok && after_ok {
                let start = doc.position_at(abs_pos);
                let end = doc.position_at(after_pos);
                locations.push(Location::new(doc.uri.clone(), Range::new(start, end)));
            }

            offset = abs_pos + 1; // Move past this occurrence
        }
    }

    /// Handle documentSymbol request.
    pub fn document_symbol(&self, uri: &DocumentUri) -> Vec<DocumentSymbol> {
        let Some(doc) = self.documents.get(uri) else {
            return Vec::new();
        };
        self.symbols.document_symbols(&doc)
    }

    /// Handle code action request.
    pub fn code_action(&self, params: CodeActionParams) -> Vec<CodeAction> {
        let Some(doc) = self.documents.get(&params.text_document.uri) else {
            return Vec::new();
        };

        let mut actions = Vec::new();

        // Generate quick fixes for diagnostics
        for diagnostic in &params.context.diagnostics {
            if let Some(fix) = self.generate_quick_fix(&doc, diagnostic) {
                actions.push(fix);
            }
        }

        actions
    }

    /// Handle formatting request.
    pub fn format(&self, params: DocumentFormattingParams) -> Vec<TextEdit> {
        let Some(doc) = self.documents.get(&params.text_document.uri) else {
            return Vec::new();
        };

        // TODO: Use actual formatter
        // For now, just trim trailing whitespace
        let mut edits = Vec::new();
        for (line_num, line) in doc.content.lines().enumerate() {
            let trimmed = line.trim_end();
            if trimmed.len() < line.len() {
                let start = Position::new(line_num as u32, trimmed.len() as u32);
                let end = Position::new(line_num as u32, line.len() as u32);
                edits.push(TextEdit::delete(Range::new(start, end)));
            }
        }

        edits
    }

    /// Handle rename request.
    pub fn rename(&self, params: RenameParams) -> Option<WorkspaceEdit> {
        let doc = self
            .documents
            .get(&params.text_document_position.text_document.uri)?;
        let (word, _range) = doc.word_at(params.text_document_position.position)?;

        // Find all occurrences in the document
        let mut edit = WorkspaceEdit::new();
        let content = &doc.content;
        let mut offset = 0;

        while let Some(pos) = content[offset..].find(&word) {
            let abs_pos = offset + pos;
            // Check word boundaries
            let before_ok =
                abs_pos == 0 || !content.as_bytes()[abs_pos - 1].is_ascii_alphanumeric();
            let after_pos = abs_pos + word.len();
            let after_ok = after_pos >= content.len()
                || !content.as_bytes()[after_pos].is_ascii_alphanumeric();

            if before_ok && after_ok {
                let start = doc.position_at(abs_pos);
                let end = doc.position_at(after_pos);
                edit.add_edit(
                    doc.uri.clone(),
                    TextEdit::replace(Range::new(start, end), params.new_name.clone()),
                );
            }

            offset = abs_pos + word.len();
        }

        Some(edit)
    }

    /// Handle folding range request.
    pub fn folding_range(&self, uri: &DocumentUri) -> Vec<FoldingRange> {
        let Some(doc) = self.documents.get(uri) else {
            return Vec::new();
        };

        let mut ranges = Vec::new();
        let mut brace_stack: Vec<u32> = Vec::new();
        let mut comment_start: Option<u32> = None;

        let lines: Vec<&str> = doc.content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            let line_num = line_num as u32;
            let trimmed = line.trim_start();

            // Track brace nesting for code folding
            for c in line.chars() {
                if c == '{' {
                    brace_stack.push(line_num);
                } else if c == '}' {
                    if let Some(start_line) = brace_stack.pop() {
                        if line_num > start_line {
                            ranges.push(FoldingRange {
                                start_line,
                                start_character: None,
                                end_line: line_num,
                                end_character: None,
                                kind: None,
                            });
                        }
                    }
                }
            }

            // Track consecutive comment blocks
            let is_comment = trimmed.starts_with("//");
            if is_comment {
                if comment_start.is_none() {
                    comment_start = Some(line_num);
                }
            } else {
                // End of comment block - check if we had consecutive comments
                if let Some(start) = comment_start {
                    if line_num > start + 1 {
                        // At least 2 consecutive comment lines
                        ranges.push(FoldingRange {
                            start_line: start,
                            start_character: None,
                            end_line: line_num - 1,
                            end_character: None,
                            kind: Some(FoldingRangeKind::Comment),
                        });
                    }
                    comment_start = None;
                }
            }
        }

        // Handle comment block at end of file
        if let Some(start) = comment_start {
            let end = lines.len() as u32 - 1;
            if end > start {
                ranges.push(FoldingRange {
                    start_line: start,
                    start_character: None,
                    end_line: end,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Comment),
                });
            }
        }

        ranges
    }

    // =========================================================================
    // HELPERS
    // =========================================================================

    /// Generate a quick fix for a diagnostic.
    fn generate_quick_fix(
        &self,
        doc: &super::document::Document,
        diagnostic: &Diagnostic,
    ) -> Option<CodeAction> {
        let message = &diagnostic.message;

        // Missing semicolon fix
        if message.contains("expected ';'") {
            let mut action = CodeAction::quick_fix("Add missing semicolon");
            let pos = diagnostic.range.end;
            let mut edit = WorkspaceEdit::new();
            edit.add_edit(doc.uri.clone(), TextEdit::insert(pos, ";".to_string()));
            action.edit = Some(edit);
            action.is_preferred = true;
            return Some(action);
        }

        // Unused variable fix
        if message.contains("unused variable") {
            let mut action = CodeAction::quick_fix("Prefix with underscore");
            let start = diagnostic.range.start;
            let mut edit = WorkspaceEdit::new();
            edit.add_edit(doc.uri.clone(), TextEdit::insert(start, "_".to_string()));
            action.edit = Some(edit);
            return Some(action);
        }

        // Import suggestion
        if message.contains("not found in this scope") {
            if let Some((word, _)) = doc.word_at(diagnostic.range.start) {
                // Suggest common imports
                let import_suggestions = suggest_import(&word);
                if !import_suggestions.is_empty() {
                    let suggestion = &import_suggestions[0];
                    let mut action = CodeAction::quick_fix(format!("Import {}", suggestion));
                    let mut edit = WorkspaceEdit::new();
                    edit.add_edit(
                        doc.uri.clone(),
                        TextEdit::insert(Position::new(0, 0), format!("use {};\n", suggestion)),
                    );
                    action.edit = Some(edit);
                    return Some(action);
                }
            }
        }

        None
    }
}

impl Default for LanguageServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Suggest imports for a name.
fn suggest_import(name: &str) -> Vec<String> {
    // Common stdlib imports
    let suggestions: Vec<(&str, &str)> = vec![
        ("HashMap", "std::collections::HashMap"),
        ("HashSet", "std::collections::HashSet"),
        ("Vec", "std::vec::Vec"),
        ("String", "std::string::String"),
        ("Arc", "std::sync::Arc"),
        ("Mutex", "std::sync::Mutex"),
        ("Rc", "std::rc::Rc"),
        ("RefCell", "std::cell::RefCell"),
        ("Path", "std::path::Path"),
        ("PathBuf", "std::path::PathBuf"),
        ("File", "std::fs::File"),
    ];

    suggestions
        .into_iter()
        .filter(|(n, _)| *n == name)
        .map(|(_, path)| path.to_string())
        .collect()
}

// =============================================================================
// SERVER RUNNER
// =============================================================================

/// Run the language server with stdio transport.
pub fn run_server() -> Result<(), TransportError> {
    let transport = StdioTransport::new();
    let mut server = LanguageServer::new();

    loop {
        let raw_msg = transport.recv()?;

        // Parse and handle message
        let response = handle_raw_message(&mut server, &raw_msg.content);

        if let Some(response_content) = response {
            transport.send(RawMessage::new(response_content))?;
        }

        // Check for exit
        if server.should_shutdown() {
            break;
        }
    }

    Ok(())
}

/// Extract a JSON string value for a given key (simplified parser).
fn extract_json_string(content: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let pos = content.find(&pattern)?;
    let rest = &content[pos + pattern.len()..];
    // Skip optional whitespace and colon
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    let rest = &rest[1..];
    // Find closing quote (handling escaped quotes)
    let mut end = 0;
    let bytes = rest.as_bytes();
    while end < bytes.len() {
        if bytes[end] == b'"' {
            return Some(
                rest[..end]
                    .replace("\\\"", "\"")
                    .replace("\\\\", "\\")
                    .replace("\\n", "\n")
                    .replace("\\t", "\t"),
            );
        }
        if bytes[end] == b'\\' {
            end += 1; // skip escaped char
        }
        end += 1;
    }
    None
}

/// Extract a JSON number value for a given key (simplified parser).
fn extract_json_number(content: &str, key: &str) -> Option<i64> {
    let pattern = format!("\"{}\"", key);
    let pos = content.find(&pattern)?;
    let rest = &content[pos + pattern.len()..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();
    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '-')
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Extract text document URI from JSON content.
fn extract_uri(content: &str) -> Option<String> {
    extract_json_string(content, "uri")
}

/// Extract position (line, character) from JSON content.
fn extract_position(content: &str) -> Option<Position> {
    // Find "position" object, then extract line and character within it
    let pos_idx = content.find("\"position\"")?;
    let rest = &content[pos_idx..];
    let line = extract_json_number(rest, "line")? as u32;
    let character = extract_json_number(rest, "character")? as u32;
    Some(Position::new(line, character))
}

/// Build a JSON response with result.
fn build_response(id: String, result: String) -> String {
    JsonObjectBuilder::new()
        .field_str("jsonrpc", "2.0")
        .field("id", id)
        .field("result", result)
        .build()
}

/// Build a JSON notification (no id).
fn build_notification(method: &str, params: String) -> String {
    JsonObjectBuilder::new()
        .field_str("jsonrpc", "2.0")
        .field_str("method", method)
        .field("params", params)
        .build()
}

/// Build diagnostics notification JSON.
fn build_diagnostics_notification(params: &PublishDiagnosticsParams) -> String {
    let mut diag_array = JsonArrayBuilder::new();
    for d in &params.diagnostics {
        let severity = match d.severity {
            Some(DiagnosticSeverity::Error) => 1,
            Some(DiagnosticSeverity::Warning) => 2,
            Some(DiagnosticSeverity::Information) => 3,
            Some(DiagnosticSeverity::Hint) => 4,
            None => 1,
        };
        diag_array = diag_array.item(
            JsonObjectBuilder::new()
                .field("range", build_range_json(&d.range))
                .field_number("severity", severity)
                .field_str("message", &d.message)
                .field_str("source", d.source.as_deref().unwrap_or("quantalang"))
                .build(),
        );
    }
    let params_json = JsonObjectBuilder::new()
        .field_str("uri", &params.uri)
        .field("diagnostics", diag_array.build())
        .build();
    build_notification("textDocument/publishDiagnostics", params_json)
}

/// Build range JSON.
fn build_range_json(range: &Range) -> String {
    JsonObjectBuilder::new()
        .field(
            "start",
            JsonObjectBuilder::new()
                .field_number("line", range.start.line)
                .field_number("character", range.start.character)
                .build(),
        )
        .field(
            "end",
            JsonObjectBuilder::new()
                .field_number("line", range.end.line)
                .field_number("character", range.end.character)
                .build(),
        )
        .build()
}

/// Build location JSON.
fn build_location_json(loc: &Location) -> String {
    JsonObjectBuilder::new()
        .field_str("uri", &loc.uri)
        .field("range", build_range_json(&loc.range))
        .build()
}

/// Handle a raw JSON message and return a response (and optionally a notification to send after).
fn handle_raw_message(server: &mut LanguageServer, content: &str) -> Option<String> {
    let id = extract_id(content);

    // =========================================================================
    // LIFECYCLE
    // =========================================================================

    if content.contains("\"method\":\"initialize\"") {
        let root_uri = extract_json_string(content, "rootUri");
        let params = InitializeParams {
            process_id: None,
            root_path: None,
            root_uri,
            capabilities: ClientCapabilities::default(),
            initialization_options: None,
            trace: None,
            workspace_folders: None,
        };
        let result = server.initialize(params);
        return Some(build_response(
            id.unwrap_or_else(|| "1".to_string()),
            build_initialize_result(&result),
        ));
    }

    if content.contains("\"method\":\"initialized\"") {
        server.initialized();
        return None;
    }

    if content.contains("\"method\":\"shutdown\"") {
        server.shutdown();
        return Some(build_response(
            id.unwrap_or_else(|| "1".to_string()),
            JsonBuilder::null(),
        ));
    }

    if content.contains("\"method\":\"exit\"") {
        server.exit();
        return None;
    }

    // =========================================================================
    // TEXT DOCUMENT SYNC
    // =========================================================================

    if content.contains("\"method\":\"textDocument/didOpen\"") {
        let uri = extract_json_string(content, "uri").unwrap_or_default();
        let language_id =
            extract_json_string(content, "languageId").unwrap_or_else(|| "quanta".to_string());
        let version = extract_json_number(content, "version").unwrap_or(0) as i32;
        let text = extract_json_string(content, "text").unwrap_or_default();
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri,
                language_id,
                version,
                text,
            },
        };
        if let Some(diag) = server.did_open(params) {
            return Some(build_diagnostics_notification(&diag));
        }
        return None;
    }

    if content.contains("\"method\":\"textDocument/didChange\"") {
        let uri = extract_uri(content).unwrap_or_default();
        let version = extract_json_number(content, "version").unwrap_or(0) as i32;
        // Extract full text from contentChanges (simplified: assumes full sync)
        let text = extract_json_string(content, "text").unwrap_or_default();
        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier { uri, version },
            content_changes: vec![TextDocumentContentChangeEvent { range: None, text }],
        };
        if let Some(diag) = server.did_change(params) {
            return Some(build_diagnostics_notification(&diag));
        }
        return None;
    }

    if content.contains("\"method\":\"textDocument/didSave\"") {
        let uri = extract_uri(content).unwrap_or_default();
        let params = DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier { uri },
            text: None,
        };
        if let Some(diag) = server.did_save(params) {
            return Some(build_diagnostics_notification(&diag));
        }
        return None;
    }

    if content.contains("\"method\":\"textDocument/didClose\"") {
        let uri = extract_uri(content).unwrap_or_default();
        server.did_close(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri },
        });
        return None;
    }

    // =========================================================================
    // LANGUAGE FEATURES (requests — require id)
    // =========================================================================

    if content.contains("\"method\":\"textDocument/completion\"") {
        let uri = extract_uri(content).unwrap_or_default();
        let position = extract_position(content).unwrap_or(Position::new(0, 0));
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position,
            },
            context: None,
        };
        let result = server.completion(params);
        let result_json = match result {
            Some(list) => {
                let mut items = JsonArrayBuilder::new();
                for item in &list.items {
                    let mut obj = JsonObjectBuilder::new()
                        .field_str("label", &item.label)
                        .field_number("kind", item.kind.map(|k| k as i32).unwrap_or(1));
                    if let Some(ref detail) = item.detail {
                        obj = obj.field_str("detail", detail);
                    }
                    if let Some(ref doc) = item.documentation {
                        obj = obj.field_str("documentation", &doc.value);
                    }
                    if let Some(ref insert) = item.insert_text {
                        obj = obj.field_str("insertText", insert);
                    }
                    items = items.item(obj.build());
                }
                JsonObjectBuilder::new()
                    .field_bool("isIncomplete", list.is_incomplete)
                    .field("items", items.build())
                    .build()
            }
            None => JsonBuilder::null(),
        };
        return Some(build_response(
            id.unwrap_or_else(|| "1".to_string()),
            result_json,
        ));
    }

    if content.contains("\"method\":\"textDocument/hover\"") {
        let uri = extract_uri(content).unwrap_or_default();
        let position = extract_position(content).unwrap_or(Position::new(0, 0));
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position,
        };
        let result = server.hover(params);
        let result_json = match result {
            Some(hover) => {
                let kind_str = match hover.contents.kind {
                    MarkupKind::PlainText => "plaintext",
                    MarkupKind::Markdown => "markdown",
                };
                let mut obj = JsonObjectBuilder::new().field(
                    "contents",
                    JsonObjectBuilder::new()
                        .field_str("kind", kind_str)
                        .field_str("value", &hover.contents.value)
                        .build(),
                );
                if let Some(ref range) = hover.range {
                    obj = obj.field("range", build_range_json(range));
                }
                obj.build()
            }
            None => JsonBuilder::null(),
        };
        return Some(build_response(
            id.unwrap_or_else(|| "1".to_string()),
            result_json,
        ));
    }

    if content.contains("\"method\":\"textDocument/definition\"") {
        let uri = extract_uri(content).unwrap_or_default();
        let position = extract_position(content).unwrap_or(Position::new(0, 0));
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position,
        };
        let locations = server.definition(params);
        let mut arr = JsonArrayBuilder::new();
        for loc in &locations {
            arr = arr.item(build_location_json(loc));
        }
        return Some(build_response(
            id.unwrap_or_else(|| "1".to_string()),
            arr.build(),
        ));
    }

    if content.contains("\"method\":\"textDocument/references\"") {
        let uri = extract_uri(content).unwrap_or_default();
        let position = extract_position(content).unwrap_or(Position::new(0, 0));
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position,
        };
        let locations = server.references(params);
        let mut arr = JsonArrayBuilder::new();
        for loc in &locations {
            arr = arr.item(build_location_json(loc));
        }
        return Some(build_response(
            id.unwrap_or_else(|| "1".to_string()),
            arr.build(),
        ));
    }

    if content.contains("\"method\":\"textDocument/documentSymbol\"") {
        let uri = extract_uri(content).unwrap_or_default();
        let symbols = server.document_symbol(&uri);
        let result = build_symbols_json(&symbols);
        return Some(build_response(
            id.unwrap_or_else(|| "1".to_string()),
            result,
        ));
    }

    if content.contains("\"method\":\"textDocument/formatting\"") {
        let uri = extract_uri(content).unwrap_or_default();
        let params = DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri },
            options: FormattingOptions::default(),
        };
        let edits = server.format(params);
        let mut arr = JsonArrayBuilder::new();
        for edit in &edits {
            arr = arr.item(
                JsonObjectBuilder::new()
                    .field("range", build_range_json(&edit.range))
                    .field_str("newText", &edit.new_text)
                    .build(),
            );
        }
        return Some(build_response(
            id.unwrap_or_else(|| "1".to_string()),
            arr.build(),
        ));
    }

    if content.contains("\"method\":\"textDocument/foldingRange\"") {
        let uri = extract_uri(content).unwrap_or_default();
        let ranges = server.folding_range(&uri);
        let mut arr = JsonArrayBuilder::new();
        for r in &ranges {
            let mut obj = JsonObjectBuilder::new()
                .field_number("startLine", r.start_line)
                .field_number("endLine", r.end_line);
            if let Some(kind) = &r.kind {
                let kind_str = match kind {
                    FoldingRangeKind::Comment => "comment",
                    FoldingRangeKind::Imports => "imports",
                    FoldingRangeKind::Region => "region",
                };
                obj = obj.field_str("kind", kind_str);
            }
            arr = arr.item(obj.build());
        }
        return Some(build_response(
            id.unwrap_or_else(|| "1".to_string()),
            arr.build(),
        ));
    }

    // =========================================================================
    // UNKNOWN METHOD
    // =========================================================================

    if content.contains("\"id\":") {
        let response = JsonObjectBuilder::new()
            .field_str("jsonrpc", "2.0")
            .field("id", id.unwrap_or_else(|| "1".to_string()))
            .field(
                "error",
                JsonObjectBuilder::new()
                    .field_number("code", -32601)
                    .field_str("message", "Method not found")
                    .build(),
            )
            .build();
        return Some(response);
    }

    None
}

/// Build document symbols JSON recursively.
fn build_symbols_json(symbols: &[DocumentSymbol]) -> String {
    let mut arr = JsonArrayBuilder::new();
    for sym in symbols {
        let mut obj = JsonObjectBuilder::new()
            .field_str("name", &sym.name)
            .field_number("kind", sym.kind as i32)
            .field("range", build_range_json(&sym.range))
            .field("selectionRange", build_range_json(&sym.selection_range));
        if let Some(ref detail) = sym.detail {
            obj = obj.field_str("detail", detail);
        }
        if !sym.children.is_empty() {
            obj = obj.field("children", build_symbols_json(&sym.children));
        }
        arr = arr.item(obj.build());
    }
    arr.build()
}

/// Extract request ID from JSON (very simplified).
fn extract_id(content: &str) -> Option<String> {
    if let Some(pos) = content.find("\"id\":") {
        let rest = &content[pos + 5..];
        let rest = rest.trim_start();

        if rest.starts_with('"') {
            // String ID
            if let Some(end) = rest[1..].find('"') {
                return Some(format!("\"{}\"", &rest[1..1 + end]));
            }
        } else {
            // Number ID
            let end = rest
                .find(|c: char| !c.is_ascii_digit() && c != '-')
                .unwrap_or(rest.len());
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// Build initialize result JSON.
fn build_initialize_result(result: &InitializeResult) -> String {
    let _caps = &result.capabilities;

    let mut builder = JsonObjectBuilder::new().field(
        "capabilities",
        JsonObjectBuilder::new()
            .field_number("textDocumentSync", 2) // Incremental
            .field(
                "completionProvider",
                JsonObjectBuilder::new()
                    .field(
                        "triggerCharacters",
                        JsonArrayBuilder::new()
                            .item(JsonBuilder::string("."))
                            .item(JsonBuilder::string(":"))
                            .build(),
                    )
                    .field_bool("resolveProvider", true)
                    .build(),
            )
            .field_bool("hoverProvider", true)
            .field_bool("definitionProvider", true)
            .field_bool("referencesProvider", true)
            .field_bool("documentSymbolProvider", true)
            .field_bool("documentFormattingProvider", true)
            .field_bool("renameProvider", true)
            .field_bool("foldingRangeProvider", true)
            .build(),
    );

    if let Some(ref info) = result.server_info {
        builder = builder.field(
            "serverInfo",
            JsonObjectBuilder::new()
                .field_str("name", &info.name)
                .field_str_if_some("version", info.version.as_deref())
                .build(),
        );
    }

    builder.build()
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_lifecycle() {
        let mut server = LanguageServer::new();
        assert_eq!(server.state(), ServerState::Uninitialized);

        let params = InitializeParams {
            process_id: Some(1234),
            root_path: None,
            root_uri: Some("file:///workspace".to_string()),
            capabilities: ClientCapabilities::default(),
            initialization_options: None,
            trace: None,
            workspace_folders: None,
        };

        let result = server.initialize(params);
        assert!(result.server_info.is_some());
        assert_eq!(server.state(), ServerState::Initializing);

        server.initialized();
        assert_eq!(server.state(), ServerState::Running);

        server.shutdown();
        assert!(server.should_shutdown());
        assert_eq!(server.state(), ServerState::ShuttingDown);

        server.exit();
        assert_eq!(server.state(), ServerState::Shutdown);
    }

    #[test]
    fn test_extract_id() {
        assert_eq!(
            extract_id(r#"{"id":1,"method":"test"}"#),
            Some("1".to_string())
        );
        assert_eq!(
            extract_id(r#"{"id":"abc","method":"test"}"#),
            Some("\"abc\"".to_string())
        );
    }
}
