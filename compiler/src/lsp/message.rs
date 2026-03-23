// ===============================================================================
// QUANTALANG LSP MESSAGE TYPES
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! JSON-RPC message types for LSP communication.

use super::types::*;
use std::collections::HashMap;

// =============================================================================
// JSON-RPC BASE TYPES
// =============================================================================

/// JSON-RPC version.
pub const JSONRPC_VERSION: &str = "2.0";

/// A JSON-RPC message.
#[derive(Debug, Clone)]
pub enum Message {
    /// A request message.
    Request(RequestMessage),
    /// A response message.
    Response(ResponseMessage),
    /// A notification message.
    Notification(NotificationMessage),
}

/// A request message.
#[derive(Debug, Clone)]
pub struct RequestMessage {
    /// JSON-RPC version.
    pub jsonrpc: String,
    /// The request ID.
    pub id: RequestId,
    /// The method to be invoked.
    pub method: String,
    /// The method's params.
    pub params: Option<Params>,
}

impl RequestMessage {
    pub fn new(id: impl Into<RequestId>, method: impl Into<String>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: id.into(),
            method: method.into(),
            params: None,
        }
    }

    pub fn with_params(mut self, params: Params) -> Self {
        self.params = Some(params);
        self
    }
}

/// A response message.
#[derive(Debug, Clone)]
pub struct ResponseMessage {
    /// JSON-RPC version.
    pub jsonrpc: String,
    /// The request ID.
    pub id: RequestId,
    /// The result (success).
    pub result: Option<ResponseResult>,
    /// The error (failure).
    pub error: Option<ResponseError>,
}

impl ResponseMessage {
    pub fn success(id: RequestId, result: ResponseResult) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: RequestId, error: ResponseError) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// A notification message.
#[derive(Debug, Clone)]
pub struct NotificationMessage {
    /// JSON-RPC version.
    pub jsonrpc: String,
    /// The method to be invoked.
    pub method: String,
    /// The method's params.
    pub params: Option<Params>,
}

impl NotificationMessage {
    pub fn new(method: impl Into<String>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method: method.into(),
            params: None,
        }
    }

    pub fn with_params(mut self, params: Params) -> Self {
        self.params = Some(params);
        self
    }
}

// =============================================================================
// PARAMS AND RESULTS
// =============================================================================

/// Generic params value.
#[derive(Debug, Clone)]
pub enum Params {
    /// Null params.
    None,
    /// Boolean.
    Bool(bool),
    /// Number (integer).
    Integer(i64),
    /// Number (float).
    Float(f64),
    /// String.
    String(String),
    /// Array of params.
    Array(Vec<Params>),
    /// Object with named params.
    Object(HashMap<String, Params>),
}

impl Params {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Params::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Params::Integer(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Params::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Params]> {
        match self {
            Params::Array(arr) => Some(arr),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&HashMap<String, Params>> {
        match self {
            Params::Object(obj) => Some(obj),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Params> {
        self.as_object().and_then(|obj| obj.get(key))
    }
}

impl Default for Params {
    fn default() -> Self {
        Params::None
    }
}

/// Generic response result.
#[derive(Debug, Clone)]
pub enum ResponseResult {
    /// Null result.
    Null,
    /// Boolean result.
    Bool(bool),
    /// Number result.
    Number(f64),
    /// String result.
    String(String),
    /// Array result.
    Array(Vec<ResponseResult>),
    /// Object result.
    Object(HashMap<String, ResponseResult>),
    /// Initialize result.
    Initialize(InitializeResult),
    /// Completion result.
    Completion(CompletionList),
    /// Hover result.
    Hover(Option<Hover>),
    /// Definition result.
    Definition(Vec<Location>),
    /// References result.
    References(Vec<Location>),
    /// Document symbols result.
    DocumentSymbols(Vec<DocumentSymbol>),
    /// Code actions result.
    CodeActions(Vec<CodeAction>),
    /// Text edits result.
    TextEdits(Vec<TextEdit>),
    /// Workspace edit result.
    WorkspaceEdit(WorkspaceEdit),
    /// Signature help result.
    SignatureHelp(Option<SignatureHelp>),
    /// Folding ranges result.
    FoldingRanges(Vec<FoldingRange>),
}

/// Response error.
#[derive(Debug, Clone)]
pub struct ResponseError {
    /// Error code.
    pub code: ErrorCode,
    /// Error message.
    pub message: String,
    /// Additional data.
    pub data: Option<String>,
}

impl ResponseError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn parse_error(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::ParseError, message)
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidRequest, message)
    }

    pub fn method_not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::MethodNotFound, message)
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidParams, message)
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InternalError, message)
    }
}

/// Error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Parse error.
    ParseError = -32700,
    /// Invalid request.
    InvalidRequest = -32600,
    /// Method not found.
    MethodNotFound = -32601,
    /// Invalid params.
    InvalidParams = -32602,
    /// Internal error.
    InternalError = -32603,

    // LSP specific errors
    /// Server not initialized.
    ServerNotInitialized = -32002,
    /// Unknown error code.
    UnknownErrorCode = -32001,

    /// Request cancelled.
    RequestCancelled = -32800,
    /// Content modified.
    ContentModified = -32801,
    /// Server cancelled.
    ServerCancelled = -32802,
    /// Request failed.
    RequestFailed = -32803,
}

// =============================================================================
// LIFECYCLE MESSAGES
// =============================================================================

/// Initialize request params.
#[derive(Debug, Clone)]
pub struct InitializeParams {
    /// Process ID of the parent process.
    pub process_id: Option<i32>,
    /// Root path (deprecated).
    pub root_path: Option<String>,
    /// Root URI.
    pub root_uri: Option<DocumentUri>,
    /// Client capabilities.
    pub capabilities: ClientCapabilities,
    /// Initialization options.
    pub initialization_options: Option<Params>,
    /// Trace setting.
    pub trace: Option<TraceValue>,
    /// Workspace folders.
    pub workspace_folders: Option<Vec<WorkspaceFolder>>,
}

/// Initialize result.
#[derive(Debug, Clone)]
pub struct InitializeResult {
    /// Server capabilities.
    pub capabilities: ServerCapabilities,
    /// Server info.
    pub server_info: Option<ServerInfo>,
}

/// Server info.
#[derive(Debug, Clone)]
pub struct ServerInfo {
    /// Server name.
    pub name: String,
    /// Server version.
    pub version: Option<String>,
}

/// Client capabilities.
#[derive(Debug, Clone, Default)]
pub struct ClientCapabilities {
    /// Text document capabilities.
    pub text_document: Option<TextDocumentClientCapabilities>,
    /// Workspace capabilities.
    pub workspace: Option<WorkspaceClientCapabilities>,
    /// Window capabilities.
    pub window: Option<WindowClientCapabilities>,
    /// General capabilities.
    pub general: Option<GeneralClientCapabilities>,
}

/// Text document client capabilities.
#[derive(Debug, Clone, Default)]
pub struct TextDocumentClientCapabilities {
    /// Synchronization capabilities.
    pub synchronization: Option<SynchronizationCapabilities>,
    /// Completion capabilities.
    pub completion: Option<CompletionClientCapabilities>,
    /// Hover capabilities.
    pub hover: Option<HoverClientCapabilities>,
}

/// Synchronization capabilities.
#[derive(Debug, Clone, Default)]
pub struct SynchronizationCapabilities {
    /// Dynamic registration.
    pub dynamic_registration: bool,
    /// Will save support.
    pub will_save: bool,
    /// Will save wait until support.
    pub will_save_wait_until: bool,
    /// Did save support.
    pub did_save: bool,
}

/// Completion client capabilities.
#[derive(Debug, Clone, Default)]
pub struct CompletionClientCapabilities {
    /// Dynamic registration.
    pub dynamic_registration: bool,
    /// Snippet support.
    pub snippet_support: bool,
    /// Commit characters support.
    pub commit_characters_support: bool,
    /// Documentation format.
    pub documentation_format: Vec<MarkupKind>,
    /// Preselect support.
    pub preselect_support: bool,
}

/// Hover client capabilities.
#[derive(Debug, Clone, Default)]
pub struct HoverClientCapabilities {
    /// Dynamic registration.
    pub dynamic_registration: bool,
    /// Content format.
    pub content_format: Vec<MarkupKind>,
}

/// Workspace client capabilities.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceClientCapabilities {
    /// Apply edit support.
    pub apply_edit: bool,
    /// Workspace edit capabilities.
    pub workspace_edit: Option<WorkspaceEditClientCapabilities>,
    /// Did change configuration support.
    pub did_change_configuration: Option<DidChangeConfigurationCapabilities>,
}

/// Workspace edit capabilities.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceEditClientCapabilities {
    /// Document changes support.
    pub document_changes: bool,
}

/// Did change configuration capabilities.
#[derive(Debug, Clone, Default)]
pub struct DidChangeConfigurationCapabilities {
    /// Dynamic registration.
    pub dynamic_registration: bool,
}

/// Window client capabilities.
#[derive(Debug, Clone, Default)]
pub struct WindowClientCapabilities {
    /// Work done progress support.
    pub work_done_progress: bool,
    /// Show message support.
    pub show_message: Option<ShowMessageRequestCapabilities>,
}

/// Show message request capabilities.
#[derive(Debug, Clone, Default)]
pub struct ShowMessageRequestCapabilities {
    /// Message action item support.
    pub message_action_item: Option<MessageActionItemCapabilities>,
}

/// Message action item capabilities.
#[derive(Debug, Clone, Default)]
pub struct MessageActionItemCapabilities {
    /// Additional properties support.
    pub additional_properties_support: bool,
}

/// General client capabilities.
#[derive(Debug, Clone, Default)]
pub struct GeneralClientCapabilities {
    /// Stale request support.
    pub stale_request_support: Option<StaleRequestSupportCapabilities>,
    /// Regular expressions.
    pub regular_expressions: Option<RegularExpressionsCapabilities>,
    /// Markdown support.
    pub markdown: Option<MarkdownClientCapabilities>,
}

/// Stale request support capabilities.
#[derive(Debug, Clone, Default)]
pub struct StaleRequestSupportCapabilities {
    /// Cancel.
    pub cancel: bool,
    /// Retry on content modified.
    pub retry_on_content_modified: Vec<String>,
}

/// Regular expressions capabilities.
#[derive(Debug, Clone, Default)]
pub struct RegularExpressionsCapabilities {
    /// Engine.
    pub engine: String,
    /// Version.
    pub version: Option<String>,
}

/// Markdown capabilities.
#[derive(Debug, Clone, Default)]
pub struct MarkdownClientCapabilities {
    /// Parser.
    pub parser: String,
    /// Version.
    pub version: Option<String>,
}

/// Trace value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceValue {
    /// Off.
    Off,
    /// Messages only.
    Messages,
    /// Verbose.
    Verbose,
}

/// Workspace folder.
#[derive(Debug, Clone)]
pub struct WorkspaceFolder {
    /// The URI.
    pub uri: DocumentUri,
    /// The name.
    pub name: String,
}

// =============================================================================
// TEXT DOCUMENT MESSAGES
// =============================================================================

/// Did open text document params.
#[derive(Debug, Clone)]
pub struct DidOpenTextDocumentParams {
    /// The document that was opened.
    pub text_document: TextDocumentItem,
}

/// Did change text document params.
#[derive(Debug, Clone)]
pub struct DidChangeTextDocumentParams {
    /// The document that did change.
    pub text_document: VersionedTextDocumentIdentifier,
    /// The content changes.
    pub content_changes: Vec<TextDocumentContentChangeEvent>,
}

/// Did save text document params.
#[derive(Debug, Clone)]
pub struct DidSaveTextDocumentParams {
    /// The document that was saved.
    pub text_document: TextDocumentIdentifier,
    /// The content (if includeText was set).
    pub text: Option<String>,
}

/// Did close text document params.
#[derive(Debug, Clone)]
pub struct DidCloseTextDocumentParams {
    /// The document that was closed.
    pub text_document: TextDocumentIdentifier,
}

/// Publish diagnostics params.
#[derive(Debug, Clone)]
pub struct PublishDiagnosticsParams {
    /// The URI.
    pub uri: DocumentUri,
    /// The version number (optional).
    pub version: Option<i32>,
    /// The diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

impl PublishDiagnosticsParams {
    pub fn new(uri: DocumentUri, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            uri,
            version: None,
            diagnostics,
        }
    }
}

// =============================================================================
// COMPLETION MESSAGES
// =============================================================================

/// Completion params.
#[derive(Debug, Clone)]
pub struct CompletionParams {
    /// Text document position.
    pub text_document_position: TextDocumentPositionParams,
    /// Completion context.
    pub context: Option<CompletionContext>,
}

/// Completion context.
#[derive(Debug, Clone)]
pub struct CompletionContext {
    /// How completion was triggered.
    pub trigger_kind: CompletionTriggerKind,
    /// The trigger character.
    pub trigger_character: Option<String>,
}

/// Completion trigger kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompletionTriggerKind {
    /// Invoked by user.
    Invoked = 1,
    /// Triggered by trigger character.
    TriggerCharacter = 2,
    /// Re-triggered for incomplete completions.
    TriggerForIncompleteCompletions = 3,
}

// =============================================================================
// CODE ACTION MESSAGES
// =============================================================================

/// Code action params.
#[derive(Debug, Clone)]
pub struct CodeActionParams {
    /// The document.
    pub text_document: TextDocumentIdentifier,
    /// The range to get actions for.
    pub range: Range,
    /// Context carrying additional information.
    pub context: CodeActionContext,
}

/// Code action context.
#[derive(Debug, Clone)]
pub struct CodeActionContext {
    /// Diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Requested code action kinds.
    pub only: Option<Vec<String>>,
    /// The reason why code actions were requested.
    pub trigger_kind: Option<CodeActionTriggerKind>,
}

/// Code action trigger kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CodeActionTriggerKind {
    /// Invoked by user.
    Invoked = 1,
    /// Automatic.
    Automatic = 2,
}

// =============================================================================
// FORMATTING MESSAGES
// =============================================================================

/// Document formatting params.
#[derive(Debug, Clone)]
pub struct DocumentFormattingParams {
    /// The document.
    pub text_document: TextDocumentIdentifier,
    /// Formatting options.
    pub options: FormattingOptions,
}

/// Document range formatting params.
#[derive(Debug, Clone)]
pub struct DocumentRangeFormattingParams {
    /// The document.
    pub text_document: TextDocumentIdentifier,
    /// The range to format.
    pub range: Range,
    /// Formatting options.
    pub options: FormattingOptions,
}

/// Formatting options.
#[derive(Debug, Clone)]
pub struct FormattingOptions {
    /// Tab size.
    pub tab_size: u32,
    /// Insert spaces instead of tabs.
    pub insert_spaces: bool,
    /// Trim trailing whitespace.
    pub trim_trailing_whitespace: bool,
    /// Insert final newline.
    pub insert_final_newline: bool,
    /// Trim final newlines.
    pub trim_final_newlines: bool,
}

impl Default for FormattingOptions {
    fn default() -> Self {
        Self {
            tab_size: 4,
            insert_spaces: true,
            trim_trailing_whitespace: true,
            insert_final_newline: true,
            trim_final_newlines: true,
        }
    }
}

// =============================================================================
// RENAME MESSAGES
// =============================================================================

/// Rename params.
#[derive(Debug, Clone)]
pub struct RenameParams {
    /// Text document position.
    pub text_document_position: TextDocumentPositionParams,
    /// The new name.
    pub new_name: String,
}

/// Prepare rename params.
#[derive(Debug, Clone)]
pub struct PrepareRenameParams {
    /// The text document.
    pub text_document: TextDocumentIdentifier,
    /// The position.
    pub position: Position,
}

/// Prepare rename result.
#[derive(Debug, Clone)]
pub struct PrepareRenameResult {
    /// The range to rename.
    pub range: Range,
    /// Placeholder text.
    pub placeholder: String,
}

// =============================================================================
// LSP METHOD NAMES
// =============================================================================

/// LSP method names.
pub mod methods {
    // Lifecycle
    pub const INITIALIZE: &str = "initialize";
    pub const INITIALIZED: &str = "initialized";
    pub const SHUTDOWN: &str = "shutdown";
    pub const EXIT: &str = "exit";

    // Text Document
    pub const DID_OPEN: &str = "textDocument/didOpen";
    pub const DID_CHANGE: &str = "textDocument/didChange";
    pub const DID_SAVE: &str = "textDocument/didSave";
    pub const DID_CLOSE: &str = "textDocument/didClose";

    // Language Features
    pub const COMPLETION: &str = "textDocument/completion";
    pub const COMPLETION_RESOLVE: &str = "completionItem/resolve";
    pub const HOVER: &str = "textDocument/hover";
    pub const SIGNATURE_HELP: &str = "textDocument/signatureHelp";
    pub const DEFINITION: &str = "textDocument/definition";
    pub const TYPE_DEFINITION: &str = "textDocument/typeDefinition";
    pub const IMPLEMENTATION: &str = "textDocument/implementation";
    pub const REFERENCES: &str = "textDocument/references";
    pub const DOCUMENT_HIGHLIGHT: &str = "textDocument/documentHighlight";
    pub const DOCUMENT_SYMBOL: &str = "textDocument/documentSymbol";
    pub const CODE_ACTION: &str = "textDocument/codeAction";
    pub const CODE_LENS: &str = "textDocument/codeLens";
    pub const CODE_LENS_RESOLVE: &str = "codeLens/resolve";
    pub const FORMATTING: &str = "textDocument/formatting";
    pub const RANGE_FORMATTING: &str = "textDocument/rangeFormatting";
    pub const RENAME: &str = "textDocument/rename";
    pub const PREPARE_RENAME: &str = "textDocument/prepareRename";
    pub const FOLDING_RANGE: &str = "textDocument/foldingRange";
    pub const SEMANTIC_TOKENS_FULL: &str = "textDocument/semanticTokens/full";

    // Workspace
    pub const WORKSPACE_SYMBOL: &str = "workspace/symbol";
    pub const EXECUTE_COMMAND: &str = "workspace/executeCommand";
    pub const APPLY_EDIT: &str = "workspace/applyEdit";

    // Notifications (server -> client)
    pub const PUBLISH_DIAGNOSTICS: &str = "textDocument/publishDiagnostics";
    pub const SHOW_MESSAGE: &str = "window/showMessage";
    pub const LOG_MESSAGE: &str = "window/logMessage";

    // Progress
    pub const WORK_DONE_PROGRESS_CREATE: &str = "window/workDoneProgress/create";
    pub const PROGRESS: &str = "$/progress";

    // Cancellation
    pub const CANCEL_REQUEST: &str = "$/cancelRequest";
}
