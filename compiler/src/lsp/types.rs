// ===============================================================================
// QUANTALANG LSP TYPES
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Core LSP type definitions following the Language Server Protocol specification.

use std::collections::HashMap;

// =============================================================================
// BASIC TYPES
// =============================================================================

/// URI representing a document location.
pub type DocumentUri = String;

/// Unique request ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RequestId {
    /// Integer ID.
    Number(i64),
    /// String ID.
    String(String),
}

impl From<i64> for RequestId {
    fn from(n: i64) -> Self {
        RequestId::Number(n)
    }
}

impl From<String> for RequestId {
    fn from(s: String) -> Self {
        RequestId::String(s)
    }
}

// =============================================================================
// POSITION AND RANGE
// =============================================================================

/// Position in a text document (0-indexed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    /// Line position (0-indexed).
    pub line: u32,
    /// Character offset on a line (0-indexed, UTF-16 code units).
    pub character: u32,
}

impl Position {
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

impl PartialOrd for Position {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Position {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.line.cmp(&other.line) {
            std::cmp::Ordering::Equal => self.character.cmp(&other.character),
            ord => ord,
        }
    }
}

/// A range in a text document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Range {
    /// Start position (inclusive).
    pub start: Position,
    /// End position (exclusive).
    pub end: Position,
}

impl Range {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub fn point(pos: Position) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }

    pub fn contains(&self, pos: Position) -> bool {
        self.start <= pos && pos < self.end
    }

    pub fn overlaps(&self, other: &Range) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// A location inside a resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// The resource URI.
    pub uri: DocumentUri,
    /// The location's range.
    pub range: Range,
}

impl Location {
    pub fn new(uri: DocumentUri, range: Range) -> Self {
        Self { uri, range }
    }
}

/// Represents a link between a source and a target location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationLink {
    /// Origin selection range.
    pub origin_selection_range: Option<Range>,
    /// The target resource identifier.
    pub target_uri: DocumentUri,
    /// The full target range.
    pub target_range: Range,
    /// The range that should be selected/highlighted.
    pub target_selection_range: Range,
}

// =============================================================================
// TEXT DOCUMENT
// =============================================================================

/// Text document identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDocumentIdentifier {
    /// The text document's URI.
    pub uri: DocumentUri,
}

/// Versioned text document identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedTextDocumentIdentifier {
    /// The text document's URI.
    pub uri: DocumentUri,
    /// Version number of this document.
    pub version: i32,
}

/// Text document position params.
#[derive(Debug, Clone)]
pub struct TextDocumentPositionParams {
    /// The text document.
    pub text_document: TextDocumentIdentifier,
    /// The position inside the text document.
    pub position: Position,
}

/// A text edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// The range of the text document to be changed.
    pub range: Range,
    /// The string to be inserted.
    pub new_text: String,
}

impl TextEdit {
    pub fn new(range: Range, new_text: String) -> Self {
        Self { range, new_text }
    }

    pub fn insert(pos: Position, text: String) -> Self {
        Self {
            range: Range::point(pos),
            new_text: text,
        }
    }

    pub fn replace(range: Range, text: String) -> Self {
        Self {
            range,
            new_text: text,
        }
    }

    pub fn delete(range: Range) -> Self {
        Self {
            range,
            new_text: String::new(),
        }
    }
}

/// Annotated text edit with change annotation.
#[derive(Debug, Clone)]
pub struct AnnotatedTextEdit {
    /// The text edit.
    pub text_edit: TextEdit,
    /// Annotation ID.
    pub annotation_id: String,
}

/// Text document item.
#[derive(Debug, Clone)]
pub struct TextDocumentItem {
    /// The text document's URI.
    pub uri: DocumentUri,
    /// The language id (e.g., "quanta").
    pub language_id: String,
    /// Version number.
    pub version: i32,
    /// The content of the opened text document.
    pub text: String,
}

/// Text document content change event.
#[derive(Debug, Clone)]
pub struct TextDocumentContentChangeEvent {
    /// The range of the document that changed (None = full document).
    pub range: Option<Range>,
    /// The new text of the document/range.
    pub text: String,
}

// =============================================================================
// DIAGNOSTICS
// =============================================================================

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    /// Reports an error.
    Error = 1,
    /// Reports a warning.
    Warning = 2,
    /// Reports an information.
    Information = 3,
    /// Reports a hint.
    Hint = 4,
}

/// Diagnostic tag for additional metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DiagnosticTag {
    /// Unused or unnecessary code.
    Unnecessary = 1,
    /// Deprecated code.
    Deprecated = 2,
}

/// Related information for a diagnostic.
#[derive(Debug, Clone)]
pub struct DiagnosticRelatedInformation {
    /// Location of related info.
    pub location: Location,
    /// The message.
    pub message: String,
}

/// A diagnostic (error, warning, etc.).
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The range at which the message applies.
    pub range: Range,
    /// Severity (error, warning, etc.).
    pub severity: Option<DiagnosticSeverity>,
    /// Diagnostic code.
    pub code: Option<DiagnosticCode>,
    /// Human-readable string for the source.
    pub source: Option<String>,
    /// The diagnostic's message.
    pub message: String,
    /// Tags for the diagnostic.
    pub tags: Vec<DiagnosticTag>,
    /// Related information.
    pub related_information: Vec<DiagnosticRelatedInformation>,
}

/// Diagnostic code (can be number or string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticCode {
    Number(i32),
    String(String),
}

impl Diagnostic {
    pub fn error(range: Range, message: impl Into<String>) -> Self {
        Self {
            range,
            severity: Some(DiagnosticSeverity::Error),
            code: None,
            source: Some("quantalang".to_string()),
            message: message.into(),
            tags: Vec::new(),
            related_information: Vec::new(),
        }
    }

    pub fn warning(range: Range, message: impl Into<String>) -> Self {
        Self {
            range,
            severity: Some(DiagnosticSeverity::Warning),
            code: None,
            source: Some("quantalang".to_string()),
            message: message.into(),
            tags: Vec::new(),
            related_information: Vec::new(),
        }
    }

    pub fn hint(range: Range, message: impl Into<String>) -> Self {
        Self {
            range,
            severity: Some(DiagnosticSeverity::Hint),
            code: None,
            source: Some("quantalang".to_string()),
            message: message.into(),
            tags: Vec::new(),
            related_information: Vec::new(),
        }
    }

    pub fn with_code(mut self, code: impl Into<DiagnosticCode>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_tag(mut self, tag: DiagnosticTag) -> Self {
        self.tags.push(tag);
        self
    }
}

impl From<i32> for DiagnosticCode {
    fn from(n: i32) -> Self {
        DiagnosticCode::Number(n)
    }
}

impl From<String> for DiagnosticCode {
    fn from(s: String) -> Self {
        DiagnosticCode::String(s)
    }
}

// =============================================================================
// COMPLETION
// =============================================================================

/// Completion item kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompletionItemKind {
    Text = 1,
    Method = 2,
    Function = 3,
    Constructor = 4,
    Field = 5,
    Variable = 6,
    Class = 7,
    Interface = 8,
    Module = 9,
    Property = 10,
    Unit = 11,
    Value = 12,
    Enum = 13,
    Keyword = 14,
    Snippet = 15,
    Color = 16,
    File = 17,
    Reference = 18,
    Folder = 19,
    EnumMember = 20,
    Constant = 21,
    Struct = 22,
    Event = 23,
    Operator = 24,
    TypeParameter = 25,
}

/// Completion item tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompletionItemTag {
    Deprecated = 1,
}

/// Insert text format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum InsertTextFormat {
    /// Plain text.
    PlainText = 1,
    /// Snippet syntax.
    Snippet = 2,
}

/// Completion item.
#[derive(Debug, Clone)]
pub struct CompletionItem {
    /// Label for display.
    pub label: String,
    /// The kind of completion.
    pub kind: Option<CompletionItemKind>,
    /// Tags for this item.
    pub tags: Vec<CompletionItemTag>,
    /// Detail information.
    pub detail: Option<String>,
    /// Documentation.
    pub documentation: Option<MarkupContent>,
    /// Deprecated flag.
    pub deprecated: bool,
    /// Pre-select this item.
    pub preselect: bool,
    /// Sort text for ordering.
    pub sort_text: Option<String>,
    /// Filter text for filtering.
    pub filter_text: Option<String>,
    /// Text to insert.
    pub insert_text: Option<String>,
    /// Insert text format.
    pub insert_text_format: Option<InsertTextFormat>,
    /// Text edit for the completion.
    pub text_edit: Option<TextEdit>,
    /// Additional edits.
    pub additional_text_edits: Vec<TextEdit>,
    /// Characters that trigger completion commit.
    pub commit_characters: Vec<String>,
    /// Custom data for resolution.
    pub data: Option<String>,
}

impl CompletionItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            kind: None,
            tags: Vec::new(),
            detail: None,
            documentation: None,
            deprecated: false,
            preselect: false,
            sort_text: None,
            filter_text: None,
            insert_text: None,
            insert_text_format: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
            commit_characters: Vec::new(),
            data: None,
        }
    }

    pub fn keyword(label: impl Into<String>) -> Self {
        Self::new(label).with_kind(CompletionItemKind::Keyword)
    }

    pub fn function(label: impl Into<String>) -> Self {
        Self::new(label).with_kind(CompletionItemKind::Function)
    }

    pub fn variable(label: impl Into<String>) -> Self {
        Self::new(label).with_kind(CompletionItemKind::Variable)
    }

    pub fn field(label: impl Into<String>) -> Self {
        Self::new(label).with_kind(CompletionItemKind::Field)
    }

    pub fn method(label: impl Into<String>) -> Self {
        Self::new(label).with_kind(CompletionItemKind::Method)
    }

    pub fn type_item(label: impl Into<String>) -> Self {
        Self::new(label).with_kind(CompletionItemKind::Class)
    }

    pub fn snippet(label: impl Into<String>, snippet: impl Into<String>) -> Self {
        Self::new(label)
            .with_kind(CompletionItemKind::Snippet)
            .with_insert_text(snippet)
            .with_insert_text_format(InsertTextFormat::Snippet)
    }

    pub fn with_kind(mut self, kind: CompletionItemKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_documentation(mut self, docs: impl Into<String>) -> Self {
        self.documentation = Some(MarkupContent::markdown(docs));
        self
    }

    pub fn with_insert_text(mut self, text: impl Into<String>) -> Self {
        self.insert_text = Some(text.into());
        self
    }

    pub fn with_insert_text_format(mut self, format: InsertTextFormat) -> Self {
        self.insert_text_format = Some(format);
        self
    }

    pub fn with_sort_text(mut self, text: impl Into<String>) -> Self {
        self.sort_text = Some(text.into());
        self
    }

    pub fn deprecated(mut self) -> Self {
        self.deprecated = true;
        self.tags.push(CompletionItemTag::Deprecated);
        self
    }

    pub fn preselect(mut self) -> Self {
        self.preselect = true;
        self
    }
}

/// Completion list.
#[derive(Debug, Clone)]
pub struct CompletionList {
    /// Is this list incomplete?
    pub is_incomplete: bool,
    /// The completion items.
    pub items: Vec<CompletionItem>,
}

impl CompletionList {
    pub fn new(items: Vec<CompletionItem>) -> Self {
        Self {
            is_incomplete: false,
            items,
        }
    }

    pub fn incomplete(items: Vec<CompletionItem>) -> Self {
        Self {
            is_incomplete: true,
            items,
        }
    }
}

// =============================================================================
// HOVER
// =============================================================================

/// Markup content.
#[derive(Debug, Clone)]
pub struct MarkupContent {
    /// The type of the Markup.
    pub kind: MarkupKind,
    /// The content.
    pub value: String,
}

/// Markup kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkupKind {
    /// Plain text.
    PlainText,
    /// Markdown.
    Markdown,
}

impl MarkupContent {
    pub fn plain_text(value: impl Into<String>) -> Self {
        Self {
            kind: MarkupKind::PlainText,
            value: value.into(),
        }
    }

    pub fn markdown(value: impl Into<String>) -> Self {
        Self {
            kind: MarkupKind::Markdown,
            value: value.into(),
        }
    }
}

/// Hover response.
#[derive(Debug, Clone)]
pub struct Hover {
    /// The hover's content.
    pub contents: MarkupContent,
    /// Optional range.
    pub range: Option<Range>,
}

impl Hover {
    pub fn new(contents: MarkupContent) -> Self {
        Self {
            contents,
            range: None,
        }
    }

    pub fn with_range(mut self, range: Range) -> Self {
        self.range = Some(range);
        self
    }
}

// =============================================================================
// SYMBOLS
// =============================================================================

/// Symbol kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SymbolKind {
    File = 1,
    Module = 2,
    Namespace = 3,
    Package = 4,
    Class = 5,
    Method = 6,
    Property = 7,
    Field = 8,
    Constructor = 9,
    Enum = 10,
    Interface = 11,
    Function = 12,
    Variable = 13,
    Constant = 14,
    String = 15,
    Number = 16,
    Boolean = 17,
    Array = 18,
    Object = 19,
    Key = 20,
    Null = 21,
    EnumMember = 22,
    Struct = 23,
    Event = 24,
    Operator = 25,
    TypeParameter = 26,
}

/// Symbol tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SymbolTag {
    Deprecated = 1,
}

/// Document symbol (hierarchical).
#[derive(Debug, Clone)]
pub struct DocumentSymbol {
    /// Name of the symbol.
    pub name: String,
    /// Detail information.
    pub detail: Option<String>,
    /// The kind of symbol.
    pub kind: SymbolKind,
    /// Tags for this symbol.
    pub tags: Vec<SymbolTag>,
    /// Range that encloses the symbol.
    pub range: Range,
    /// Range for selection/highlighting.
    pub selection_range: Range,
    /// Children of this symbol.
    pub children: Vec<DocumentSymbol>,
}

impl DocumentSymbol {
    pub fn new(
        name: impl Into<String>,
        kind: SymbolKind,
        range: Range,
        selection_range: Range,
    ) -> Self {
        Self {
            name: name.into(),
            detail: None,
            kind,
            tags: Vec::new(),
            range,
            selection_range,
            children: Vec::new(),
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_children(mut self, children: Vec<DocumentSymbol>) -> Self {
        self.children = children;
        self
    }
}

/// Symbol information (flat).
#[derive(Debug, Clone)]
pub struct SymbolInformation {
    /// Name of the symbol.
    pub name: String,
    /// The kind of symbol.
    pub kind: SymbolKind,
    /// Tags for this symbol.
    pub tags: Vec<SymbolTag>,
    /// Location of this symbol.
    pub location: Location,
    /// Container name.
    pub container_name: Option<String>,
}

// =============================================================================
// CODE ACTIONS
// =============================================================================

/// Code action kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeActionKind(pub String);

impl CodeActionKind {
    pub const EMPTY: &'static str = "";
    pub const QUICK_FIX: &'static str = "quickfix";
    pub const REFACTOR: &'static str = "refactor";
    pub const REFACTOR_EXTRACT: &'static str = "refactor.extract";
    pub const REFACTOR_INLINE: &'static str = "refactor.inline";
    pub const REFACTOR_REWRITE: &'static str = "refactor.rewrite";
    pub const SOURCE: &'static str = "source";
    pub const SOURCE_ORGANIZE_IMPORTS: &'static str = "source.organizeImports";
    pub const SOURCE_FIX_ALL: &'static str = "source.fixAll";

    pub fn quick_fix() -> Self {
        Self(Self::QUICK_FIX.to_string())
    }

    pub fn refactor() -> Self {
        Self(Self::REFACTOR.to_string())
    }

    pub fn refactor_extract() -> Self {
        Self(Self::REFACTOR_EXTRACT.to_string())
    }

    pub fn source() -> Self {
        Self(Self::SOURCE.to_string())
    }

    pub fn organize_imports() -> Self {
        Self(Self::SOURCE_ORGANIZE_IMPORTS.to_string())
    }
}

/// Code action.
#[derive(Debug, Clone)]
pub struct CodeAction {
    /// Title of the code action.
    pub title: String,
    /// Kind of the code action.
    pub kind: Option<CodeActionKind>,
    /// Diagnostics this action resolves.
    pub diagnostics: Vec<Diagnostic>,
    /// Is this a preferred action?
    pub is_preferred: bool,
    /// Workspace edit to apply.
    pub edit: Option<WorkspaceEdit>,
    /// Command to execute.
    pub command: Option<Command>,
    /// Custom data.
    pub data: Option<String>,
}

impl CodeAction {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            kind: None,
            diagnostics: Vec::new(),
            is_preferred: false,
            edit: None,
            command: None,
            data: None,
        }
    }

    pub fn quick_fix(title: impl Into<String>) -> Self {
        Self::new(title).with_kind(CodeActionKind::quick_fix())
    }

    pub fn refactor(title: impl Into<String>) -> Self {
        Self::new(title).with_kind(CodeActionKind::refactor())
    }

    pub fn with_kind(mut self, kind: CodeActionKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_edit(mut self, edit: WorkspaceEdit) -> Self {
        self.edit = Some(edit);
        self
    }

    pub fn preferred(mut self) -> Self {
        self.is_preferred = true;
        self
    }
}

// =============================================================================
// WORKSPACE EDIT
// =============================================================================

/// Workspace edit for multi-file changes.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceEdit {
    /// Map of document URI to text edits.
    pub changes: HashMap<DocumentUri, Vec<TextEdit>>,
    /// Document changes (versioned).
    pub document_changes: Vec<TextDocumentEdit>,
}

impl WorkspaceEdit {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_edit(&mut self, uri: DocumentUri, edit: TextEdit) {
        self.changes.entry(uri).or_default().push(edit);
    }

    pub fn with_edit(mut self, uri: DocumentUri, edit: TextEdit) -> Self {
        self.add_edit(uri, edit);
        self
    }
}

/// Text document edit with version.
#[derive(Debug, Clone)]
pub struct TextDocumentEdit {
    /// The text document to change.
    pub text_document: VersionedTextDocumentIdentifier,
    /// The edits to apply.
    pub edits: Vec<TextEdit>,
}

// =============================================================================
// COMMAND
// =============================================================================

/// A command representing an action.
#[derive(Debug, Clone)]
pub struct Command {
    /// Title of the command.
    pub title: String,
    /// The identifier of the actual command.
    pub command: String,
    /// Arguments to the command.
    pub arguments: Vec<String>,
}

impl Command {
    pub fn new(title: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            command: command.into(),
            arguments: Vec::new(),
        }
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.arguments = args;
        self
    }
}

// =============================================================================
// SERVER CAPABILITIES
// =============================================================================

/// Text document sync kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TextDocumentSyncKind {
    /// Documents should not be synced.
    None = 0,
    /// Documents are synced by sending the full content.
    Full = 1,
    /// Documents are synced by sending incremental changes.
    Incremental = 2,
}

/// Server capabilities.
#[derive(Debug, Clone, Default)]
pub struct ServerCapabilities {
    /// Text document sync mode.
    pub text_document_sync: Option<TextDocumentSyncKind>,
    /// Completion support.
    pub completion_provider: Option<CompletionOptions>,
    /// Hover support.
    pub hover_provider: bool,
    /// Signature help support.
    pub signature_help_provider: Option<SignatureHelpOptions>,
    /// Go to definition support.
    pub definition_provider: bool,
    /// Go to type definition support.
    pub type_definition_provider: bool,
    /// Go to implementation support.
    pub implementation_provider: bool,
    /// Find references support.
    pub references_provider: bool,
    /// Document highlight support.
    pub document_highlight_provider: bool,
    /// Document symbol support.
    pub document_symbol_provider: bool,
    /// Workspace symbol support.
    pub workspace_symbol_provider: bool,
    /// Code action support.
    pub code_action_provider: Option<CodeActionOptions>,
    /// Code lens support.
    pub code_lens_provider: Option<CodeLensOptions>,
    /// Document formatting support.
    pub document_formatting_provider: bool,
    /// Document range formatting support.
    pub document_range_formatting_provider: bool,
    /// Rename support.
    pub rename_provider: Option<RenameOptions>,
    /// Folding range support.
    pub folding_range_provider: bool,
    /// Semantic tokens support.
    pub semantic_tokens_provider: Option<SemanticTokensOptions>,
}

impl ServerCapabilities {
    pub fn full() -> Self {
        Self {
            text_document_sync: Some(TextDocumentSyncKind::Incremental),
            completion_provider: Some(CompletionOptions {
                trigger_characters: vec![".".to_string(), ":".to_string()],
                resolve_provider: true,
            }),
            hover_provider: true,
            signature_help_provider: Some(SignatureHelpOptions {
                trigger_characters: vec!["(".to_string(), ",".to_string()],
                retrigger_characters: vec![",".to_string()],
            }),
            definition_provider: true,
            type_definition_provider: true,
            implementation_provider: true,
            references_provider: true,
            document_highlight_provider: true,
            document_symbol_provider: true,
            workspace_symbol_provider: true,
            code_action_provider: Some(CodeActionOptions {
                code_action_kinds: vec![
                    CodeActionKind::QUICK_FIX.to_string(),
                    CodeActionKind::REFACTOR.to_string(),
                    CodeActionKind::SOURCE_ORGANIZE_IMPORTS.to_string(),
                ],
                resolve_provider: false,
            }),
            code_lens_provider: Some(CodeLensOptions {
                resolve_provider: true,
            }),
            document_formatting_provider: true,
            document_range_formatting_provider: true,
            rename_provider: Some(RenameOptions {
                prepare_provider: true,
            }),
            folding_range_provider: true,
            semantic_tokens_provider: None, // Would need full semantic token config
        }
    }
}

/// Completion options.
#[derive(Debug, Clone)]
pub struct CompletionOptions {
    /// Characters that trigger completion.
    pub trigger_characters: Vec<String>,
    /// Server provides resolve support.
    pub resolve_provider: bool,
}

/// Signature help options.
#[derive(Debug, Clone)]
pub struct SignatureHelpOptions {
    /// Characters that trigger signature help.
    pub trigger_characters: Vec<String>,
    /// Characters that re-trigger signature help.
    pub retrigger_characters: Vec<String>,
}

/// Code action options.
#[derive(Debug, Clone)]
pub struct CodeActionOptions {
    /// Supported code action kinds.
    pub code_action_kinds: Vec<String>,
    /// Server provides resolve support.
    pub resolve_provider: bool,
}

/// Code lens options.
#[derive(Debug, Clone)]
pub struct CodeLensOptions {
    /// Server provides resolve support.
    pub resolve_provider: bool,
}

/// Rename options.
#[derive(Debug, Clone)]
pub struct RenameOptions {
    /// Server supports prepare rename.
    pub prepare_provider: bool,
}

/// Semantic tokens options.
#[derive(Debug, Clone)]
pub struct SemanticTokensOptions {
    /// The legend.
    pub legend: SemanticTokensLegend,
    /// Range support.
    pub range: bool,
    /// Full document support.
    pub full: bool,
}

/// Semantic tokens legend.
#[derive(Debug, Clone)]
pub struct SemanticTokensLegend {
    /// Token types.
    pub token_types: Vec<String>,
    /// Token modifiers.
    pub token_modifiers: Vec<String>,
}

// =============================================================================
// FOLDING RANGE
// =============================================================================

/// Folding range kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldingRangeKind {
    /// Comment.
    Comment,
    /// Import.
    Imports,
    /// Region.
    Region,
}

/// A folding range.
#[derive(Debug, Clone)]
pub struct FoldingRange {
    /// Start line (0-based).
    pub start_line: u32,
    /// Start character.
    pub start_character: Option<u32>,
    /// End line (0-based).
    pub end_line: u32,
    /// End character.
    pub end_character: Option<u32>,
    /// Kind of folding range.
    pub kind: Option<FoldingRangeKind>,
}

// =============================================================================
// SIGNATURE HELP
// =============================================================================

/// Signature information.
#[derive(Debug, Clone)]
pub struct SignatureInformation {
    /// Label of this signature.
    pub label: String,
    /// Documentation.
    pub documentation: Option<MarkupContent>,
    /// Parameters of this signature.
    pub parameters: Vec<ParameterInformation>,
    /// Active parameter.
    pub active_parameter: Option<u32>,
}

/// Parameter information.
#[derive(Debug, Clone)]
pub struct ParameterInformation {
    /// Label of this parameter.
    pub label: String,
    /// Documentation.
    pub documentation: Option<MarkupContent>,
}

/// Signature help.
#[derive(Debug, Clone)]
pub struct SignatureHelp {
    /// One or more signatures.
    pub signatures: Vec<SignatureInformation>,
    /// Active signature.
    pub active_signature: Option<u32>,
    /// Active parameter.
    pub active_parameter: Option<u32>,
}
