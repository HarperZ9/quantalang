# Status: lsp/

Last audited: 2026-03-21

## Working
- **Transport** (`transport.rs`, 468 lines): Stdio-based LSP transport with Content-Length header parsing, raw message send/receive. 5 unit tests.
- **Document Store** (`document.rs`, 528 lines): Tracks open documents, versions, content changes. Supports `didOpen`, `didChange`, `didClose`. 4 unit tests.
- **Types** (`types.rs`, 1104 lines): Full LSP type definitions (Position, Range, Location, Diagnostic, CompletionItem, etc.).
- **Message Types** (`message.rs`, 771 lines): Request/response/notification message structures for LSP protocol.
- **Diagnostics** (`diagnostics.rs`, 487 lines): Syntax checking, bracket matching, common issue detection, unused variable detection. 4 unit tests.
- **Completion** (`completion.rs`, 568 lines): Keyword and builtin type completion suggestions. Has 2 `todo!()` calls for context-aware completion. 4 unit tests.
- **Hover** (`hover.rs`, 260 lines): Keyword documentation, builtin type docs, local definition lookup. Has 2 `todo!()` calls.
- **Symbols** (`symbols.rs`, 600 lines): Document symbol extraction (functions, structs, enums, etc.).
- **Definition** (`definition.rs`, 436 lines): Go-to-definition via symbol search across documents.
- **Code Actions** (`actions.rs`, 428 lines): Quick fixes and refactoring suggestions.
- **Server** (`server.rs`, 741 lines): Main server with lifecycle management, request routing.

## Partial
- **Server runner** (`run_server()` in `server.rs`): The `run_server()` function exists and creates a stdio transport loop. However, the JSON parsing is manual string matching (`content.contains("\"method\":\"initialize\"")`), not real JSON parsing (no serde_json). Only `initialize`, `initialized`, `shutdown`, and `exit` methods are dispatched in the runner -- all other capabilities (completion, hover, diagnostics, definition, symbols, actions) have provider implementations but **are not wired into the message dispatch loop**. The server can technically start but cannot handle real VS Code requests beyond lifecycle events.

## Aspirational
- Full VS Code extension integration: no `--lsp` CLI subcommand exists in `quantac`. The `run_server()` function is exported from the library but never called from `main.rs`.
- Real JSON parsing: the server uses manual string matching, not proper JSON deserialization.
- Semantic analysis integration: diagnostics are text-pattern-based (bracket matching, unused variables by regex), not driven by the actual lexer/parser/type-checker pipeline.

## Not Started
- No VS Code extension package.
- No integration with the compiler's type checker for semantic diagnostics.
- No `quantac lsp` CLI subcommand.

## Honest Assessment
Total: 6,448 lines across 12 files, 24 unit tests. The LSP module has real, substantial implementations for all major language server capabilities (completion, hover, definition, diagnostics, symbols, actions). However, it is **not connected to the CLI**, uses **manual string matching instead of JSON parsing**, and only dispatches lifecycle messages in the actual server loop. The provider implementations work in isolation (unit-tested) but cannot be reached by a real LSP client. You cannot connect VS Code to this server and get working completions or diagnostics.
