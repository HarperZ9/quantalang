// ===============================================================================
// QUANTALANG LSP COMPLETION
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Code completion provider for QuantaLang.

use super::document::{Document, DocumentStore};
use super::types::*;
use std::sync::Arc;

// =============================================================================
// COMPLETION PROVIDER
// =============================================================================

/// Provides code completion suggestions.
pub struct CompletionProvider {
    /// Document store reference.
    documents: Arc<DocumentStore>,
}

impl CompletionProvider {
    /// Create a new completion provider.
    pub fn new(documents: Arc<DocumentStore>) -> Self {
        Self { documents }
    }

    /// Provide completions at a position.
    pub fn provide(&self, doc: &Document, position: Position) -> CompletionList {
        let mut items = Vec::new();

        // Get context at position
        let context = self.get_completion_context(doc, position);

        match context {
            CompletionContext::Keyword => {
                items.extend(self.keyword_completions());
            }
            CompletionContext::Type => {
                items.extend(self.type_completions());
            }
            CompletionContext::MemberAccess(prefix) => {
                items.extend(self.member_completions(&prefix));
            }
            CompletionContext::Import => {
                items.extend(self.import_completions());
            }
            CompletionContext::Attribute => {
                items.extend(self.attribute_completions());
            }
            CompletionContext::Normal => {
                // General completions
                items.extend(self.keyword_completions());
                items.extend(self.snippet_completions());
                items.extend(self.local_completions(doc, position));
            }
        }

        CompletionList::new(items)
    }

    /// Determine completion context from position.
    fn get_completion_context(&self, doc: &Document, position: Position) -> CompletionContext {
        let line = doc.line(position.line).unwrap_or("");
        let prefix = if position.character > 0 {
            &line[..position.character as usize]
        } else {
            ""
        };

        // Check for member access (. or ::)
        if let Some(dot_pos) = prefix.rfind('.') {
            let before_dot = &prefix[..dot_pos];
            let word_start = before_dot
                .rfind(|c: char| !c.is_alphanumeric() && c != '_')
                .map(|i| i + 1)
                .unwrap_or(0);
            return CompletionContext::MemberAccess(before_dot[word_start..].to_string());
        }

        if prefix.contains("::") {
            if let Some(colon_pos) = prefix.rfind("::") {
                let before_colon = &prefix[..colon_pos];
                let word_start = before_colon
                    .rfind(|c: char| !c.is_alphanumeric() && c != '_')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                return CompletionContext::MemberAccess(before_colon[word_start..].to_string());
            }
        }

        // Check for type position (after :)
        let trimmed = prefix.trim_end();
        if trimmed.ends_with(':') && !trimmed.ends_with("::") {
            return CompletionContext::Type;
        }

        // Check for import
        if prefix.trim_start().starts_with("use ") || prefix.trim_start().starts_with("import ") {
            return CompletionContext::Import;
        }

        // Check for attribute
        if prefix.trim_start().starts_with("@[") || prefix.trim_start().starts_with("#[") {
            return CompletionContext::Attribute;
        }

        // Check if at start of line or after control keywords
        let trimmed = prefix.trim_start();
        if trimmed.is_empty()
            || trimmed.ends_with('{')
            || trimmed.ends_with(';')
            || trimmed.ends_with(')')
        {
            return CompletionContext::Keyword;
        }

        CompletionContext::Normal
    }

    /// Get keyword completions.
    fn keyword_completions(&self) -> Vec<CompletionItem> {
        vec![
            CompletionItem::keyword("fn")
                .with_detail("Function declaration")
                .with_insert_text("fn ${1:name}(${2:params}) ${3:-> ReturnType }{\n\t$0\n}")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::keyword("let")
                .with_detail("Variable binding")
                .with_insert_text("let ${1:name} = ${0:value};")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::keyword("const")
                .with_detail("Constant declaration")
                .with_insert_text("const ${1:NAME}: ${2:Type} = ${0:value};")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::keyword("struct")
                .with_detail("Struct declaration")
                .with_insert_text("struct ${1:Name} {\n\t${0:fields}\n}")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::keyword("enum")
                .with_detail("Enum declaration")
                .with_insert_text("enum ${1:Name} {\n\t${0:variants}\n}")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::keyword("trait")
                .with_detail("Trait declaration")
                .with_insert_text("trait ${1:Name} {\n\t${0:methods}\n}")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::keyword("impl")
                .with_detail("Implementation block")
                .with_insert_text("impl ${1:Type} {\n\t${0:methods}\n}")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::keyword("if")
                .with_detail("If statement")
                .with_insert_text("if ${1:condition} {\n\t$0\n}")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::keyword("else").with_detail("Else clause"),
            CompletionItem::keyword("match")
                .with_detail("Match expression")
                .with_insert_text("match ${1:value} {\n\t${2:pattern} => ${0:expr},\n}")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::keyword("for")
                .with_detail("For loop")
                .with_insert_text("for ${1:item} in ${2:iter} {\n\t$0\n}")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::keyword("while")
                .with_detail("While loop")
                .with_insert_text("while ${1:condition} {\n\t$0\n}")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::keyword("loop")
                .with_detail("Infinite loop")
                .with_insert_text("loop {\n\t$0\n}")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::keyword("return").with_detail("Return statement"),
            CompletionItem::keyword("break").with_detail("Break statement"),
            CompletionItem::keyword("continue").with_detail("Continue statement"),
            CompletionItem::keyword("pub").with_detail("Public visibility"),
            CompletionItem::keyword("mod").with_detail("Module declaration"),
            CompletionItem::keyword("use").with_detail("Use declaration"),
            CompletionItem::keyword("async").with_detail("Async function"),
            CompletionItem::keyword("await").with_detail("Await expression"),
            CompletionItem::keyword("unsafe").with_detail("Unsafe block"),
            CompletionItem::keyword("where").with_detail("Where clause"),
            CompletionItem::keyword("type").with_detail("Type alias"),
            CompletionItem::keyword("self").with_detail("Self reference"),
            CompletionItem::keyword("Self").with_detail("Self type"),
            CompletionItem::keyword("super").with_detail("Parent module"),
            CompletionItem::keyword("true").with_detail("Boolean true"),
            CompletionItem::keyword("false").with_detail("Boolean false"),
        ]
    }

    /// Get type completions.
    fn type_completions(&self) -> Vec<CompletionItem> {
        vec![
            // Primitives
            CompletionItem::type_item("i8").with_detail("8-bit signed integer"),
            CompletionItem::type_item("i16").with_detail("16-bit signed integer"),
            CompletionItem::type_item("i32").with_detail("32-bit signed integer"),
            CompletionItem::type_item("i64").with_detail("64-bit signed integer"),
            CompletionItem::type_item("i128").with_detail("128-bit signed integer"),
            CompletionItem::type_item("isize").with_detail("Pointer-sized signed integer"),
            CompletionItem::type_item("u8").with_detail("8-bit unsigned integer"),
            CompletionItem::type_item("u16").with_detail("16-bit unsigned integer"),
            CompletionItem::type_item("u32").with_detail("32-bit unsigned integer"),
            CompletionItem::type_item("u64").with_detail("64-bit unsigned integer"),
            CompletionItem::type_item("u128").with_detail("128-bit unsigned integer"),
            CompletionItem::type_item("usize").with_detail("Pointer-sized unsigned integer"),
            CompletionItem::type_item("f32").with_detail("32-bit floating point"),
            CompletionItem::type_item("f64").with_detail("64-bit floating point"),
            CompletionItem::type_item("bool").with_detail("Boolean type"),
            CompletionItem::type_item("char").with_detail("Unicode character"),
            CompletionItem::type_item("str").with_detail("String slice"),
            // Common types
            CompletionItem::type_item("String").with_detail("Owned string"),
            CompletionItem::type_item("Vec")
                .with_detail("Dynamic array")
                .with_insert_text("Vec<${1:T}>")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::type_item("Option")
                .with_detail("Optional value")
                .with_insert_text("Option<${1:T}>")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::type_item("Result")
                .with_detail("Result type")
                .with_insert_text("Result<${1:T}, ${2:E}>")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::type_item("Box")
                .with_detail("Heap allocation")
                .with_insert_text("Box<${1:T}>")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::type_item("Arc")
                .with_detail("Atomic reference counting")
                .with_insert_text("Arc<${1:T}>")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::type_item("Rc")
                .with_detail("Reference counting")
                .with_insert_text("Rc<${1:T}>")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::type_item("HashMap")
                .with_detail("Hash map")
                .with_insert_text("HashMap<${1:K}, ${2:V}>")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::type_item("HashSet")
                .with_detail("Hash set")
                .with_insert_text("HashSet<${1:T}>")
                .with_insert_text_format(InsertTextFormat::Snippet),
        ]
    }

    /// Get member completions for a type.
    fn member_completions(&self, prefix: &str) -> Vec<CompletionItem> {
        // Common methods for different types
        match prefix {
            "String" | "str" => vec![
                CompletionItem::method("len").with_detail("fn() -> usize"),
                CompletionItem::method("is_empty").with_detail("fn() -> bool"),
                CompletionItem::method("chars").with_detail("fn() -> Chars"),
                CompletionItem::method("bytes").with_detail("fn() -> Bytes"),
                CompletionItem::method("trim").with_detail("fn() -> &str"),
                CompletionItem::method("split").with_detail("fn(&str) -> Split"),
                CompletionItem::method("contains").with_detail("fn(&str) -> bool"),
                CompletionItem::method("starts_with").with_detail("fn(&str) -> bool"),
                CompletionItem::method("ends_with").with_detail("fn(&str) -> bool"),
                CompletionItem::method("replace").with_detail("fn(&str, &str) -> String"),
                CompletionItem::method("to_uppercase").with_detail("fn() -> String"),
                CompletionItem::method("to_lowercase").with_detail("fn() -> String"),
                CompletionItem::method("push_str").with_detail("fn(&mut self, &str)"),
                CompletionItem::method("push").with_detail("fn(&mut self, char)"),
            ],
            "Vec" => vec![
                CompletionItem::method("len").with_detail("fn() -> usize"),
                CompletionItem::method("is_empty").with_detail("fn() -> bool"),
                CompletionItem::method("push").with_detail("fn(&mut self, T)"),
                CompletionItem::method("pop").with_detail("fn(&mut self) -> Option<T>"),
                CompletionItem::method("first").with_detail("fn() -> Option<&T>"),
                CompletionItem::method("last").with_detail("fn() -> Option<&T>"),
                CompletionItem::method("get").with_detail("fn(usize) -> Option<&T>"),
                CompletionItem::method("iter").with_detail("fn() -> Iter<T>"),
                CompletionItem::method("iter_mut").with_detail("fn() -> IterMut<T>"),
                CompletionItem::method("clear").with_detail("fn(&mut self)"),
                CompletionItem::method("contains").with_detail("fn(&T) -> bool"),
                CompletionItem::method("sort").with_detail("fn(&mut self)"),
                CompletionItem::method("reverse").with_detail("fn(&mut self)"),
            ],
            "Option" => vec![
                CompletionItem::method("is_some").with_detail("fn() -> bool"),
                CompletionItem::method("is_none").with_detail("fn() -> bool"),
                CompletionItem::method("unwrap").with_detail("fn() -> T"),
                CompletionItem::method("unwrap_or").with_detail("fn(T) -> T"),
                CompletionItem::method("unwrap_or_else").with_detail("fn(FnOnce() -> T) -> T"),
                CompletionItem::method("map").with_detail("fn(FnOnce(T) -> U) -> Option<U>"),
                CompletionItem::method("and_then")
                    .with_detail("fn(FnOnce(T) -> Option<U>) -> Option<U>"),
                CompletionItem::method("ok_or").with_detail("fn(E) -> Result<T, E>"),
                CompletionItem::method("expect").with_detail("fn(&str) -> T"),
            ],
            "Result" => vec![
                CompletionItem::method("is_ok").with_detail("fn() -> bool"),
                CompletionItem::method("is_err").with_detail("fn() -> bool"),
                CompletionItem::method("ok").with_detail("fn() -> Option<T>"),
                CompletionItem::method("err").with_detail("fn() -> Option<E>"),
                CompletionItem::method("unwrap").with_detail("fn() -> T"),
                CompletionItem::method("unwrap_err").with_detail("fn() -> E"),
                CompletionItem::method("unwrap_or").with_detail("fn(T) -> T"),
                CompletionItem::method("map").with_detail("fn(FnOnce(T) -> U) -> Result<U, E>"),
                CompletionItem::method("map_err").with_detail("fn(FnOnce(E) -> F) -> Result<T, F>"),
                CompletionItem::method("and_then")
                    .with_detail("fn(FnOnce(T) -> Result<U, E>) -> Result<U, E>"),
                CompletionItem::method("expect").with_detail("fn(&str) -> T"),
            ],
            _ => vec![
                // Generic methods available on most types
                CompletionItem::method("clone").with_detail("fn() -> Self"),
                CompletionItem::method("to_string").with_detail("fn() -> String"),
            ],
        }
    }

    /// Get import completions.
    fn import_completions(&self) -> Vec<CompletionItem> {
        vec![
            CompletionItem::new("std")
                .with_kind(CompletionItemKind::Module)
                .with_detail("Standard library"),
            CompletionItem::new("std::collections")
                .with_kind(CompletionItemKind::Module)
                .with_detail("Collection types"),
            CompletionItem::new("std::io")
                .with_kind(CompletionItemKind::Module)
                .with_detail("I/O operations"),
            CompletionItem::new("std::fs")
                .with_kind(CompletionItemKind::Module)
                .with_detail("Filesystem operations"),
            CompletionItem::new("std::sync")
                .with_kind(CompletionItemKind::Module)
                .with_detail("Synchronization primitives"),
            CompletionItem::new("std::thread")
                .with_kind(CompletionItemKind::Module)
                .with_detail("Threading"),
            CompletionItem::new("std::time")
                .with_kind(CompletionItemKind::Module)
                .with_detail("Time handling"),
        ]
    }

    /// Get attribute completions.
    fn attribute_completions(&self) -> Vec<CompletionItem> {
        vec![
            CompletionItem::new("derive")
                .with_kind(CompletionItemKind::Keyword)
                .with_detail("Derive macro")
                .with_insert_text("derive(${1:Clone, Debug})]")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::new("test")
                .with_kind(CompletionItemKind::Keyword)
                .with_detail("Test function"),
            CompletionItem::new("inline")
                .with_kind(CompletionItemKind::Keyword)
                .with_detail("Inline hint"),
            CompletionItem::new("cfg")
                .with_kind(CompletionItemKind::Keyword)
                .with_detail("Conditional compilation")
                .with_insert_text("cfg(${1:condition})]")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::new("allow")
                .with_kind(CompletionItemKind::Keyword)
                .with_detail("Allow lint")
                .with_insert_text("allow(${1:lint})]")
                .with_insert_text_format(InsertTextFormat::Snippet),
            CompletionItem::new("deny")
                .with_kind(CompletionItemKind::Keyword)
                .with_detail("Deny lint"),
            CompletionItem::new("deprecated")
                .with_kind(CompletionItemKind::Keyword)
                .with_detail("Mark as deprecated"),
            CompletionItem::new("must_use")
                .with_kind(CompletionItemKind::Keyword)
                .with_detail("Must use return value"),
            CompletionItem::new("repr")
                .with_kind(CompletionItemKind::Keyword)
                .with_detail("Type representation")
                .with_insert_text("repr(${1:C})]")
                .with_insert_text_format(InsertTextFormat::Snippet),
        ]
    }

    /// Get snippet completions.
    fn snippet_completions(&self) -> Vec<CompletionItem> {
        vec![
            CompletionItem::snippet("main", "fn main() {\n\t$0\n}").with_detail("Main function"),
            CompletionItem::snippet("test", "#[test]\nfn ${1:test_name}() {\n\t$0\n}")
                .with_detail("Test function"),
            CompletionItem::snippet("println", "println!(\"${1:}\");$0")
                .with_detail("Print line macro"),
            CompletionItem::snippet("print", "print!(\"${1:}\");$0").with_detail("Print macro"),
            CompletionItem::snippet("format", "format!(\"${1:}\"${2:, args})$0")
                .with_detail("Format string"),
            CompletionItem::snippet("vec", "vec![${1:items}]$0").with_detail("Vector literal"),
            CompletionItem::snippet("todo", "todo!(\"${1:}\")$0").with_detail("Todo macro"),
            CompletionItem::snippet("unimplemented", "unimplemented!(\"${1:}\")$0")
                .with_detail("Unimplemented macro"),
            CompletionItem::snippet("assert", "assert!(${1:condition});$0")
                .with_detail("Assert macro"),
            CompletionItem::snippet("assert_eq", "assert_eq!(${1:left}, ${2:right});$0")
                .with_detail("Assert equality"),
            CompletionItem::snippet("dbg", "dbg!(${1:&value})$0").with_detail("Debug print"),
        ]
    }

    /// Get local completions from the document.
    fn local_completions(&self, doc: &Document, _position: Position) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let content = &doc.content;

        // Simple pattern matching for local variables and functions
        // In a real implementation, this would use the AST
        for line in content.lines() {
            // Match function definitions
            if let Some(fn_match) = extract_fn_name(line) {
                items.push(
                    CompletionItem::function(&fn_match.name)
                        .with_detail(&fn_match.signature)
                        .with_sort_text(format!("0_{}", fn_match.name)),
                );
            }

            // Match let bindings
            if let Some(var_name) = extract_let_binding(line) {
                items.push(
                    CompletionItem::variable(&var_name).with_sort_text(format!("1_{}", var_name)),
                );
            }

            // Match struct definitions
            if let Some(struct_name) = extract_struct_name(line) {
                items.push(
                    CompletionItem::new(&struct_name)
                        .with_kind(CompletionItemKind::Struct)
                        .with_sort_text(format!("2_{}", struct_name)),
                );
            }
        }

        items
    }
}

/// Completion context.
enum CompletionContext {
    /// At keyword position.
    Keyword,
    /// At type position.
    Type,
    /// Member access (after . or ::).
    MemberAccess(String),
    /// In import statement.
    Import,
    /// In attribute.
    Attribute,
    /// Normal context.
    Normal,
}

/// Function match result.
struct FnMatch {
    name: String,
    signature: String,
}

/// Extract function name from a line.
fn extract_fn_name(line: &str) -> Option<FnMatch> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("fn ") {
        if let Some(paren_pos) = rest.find('(') {
            let name = rest[..paren_pos].trim().to_string();
            if !name.is_empty() && is_valid_identifier(&name) {
                let end = rest.find('{').unwrap_or(rest.len());
                let signature = format!("fn {}", &rest[..end].trim());
                return Some(FnMatch { name, signature });
            }
        }
    }
    None
}

/// Extract let binding name.
fn extract_let_binding(line: &str) -> Option<String> {
    let trimmed = line.trim();
    // Try "let mut " first since "let " is a prefix of "let mut "
    let rest = trimmed
        .strip_prefix("let mut ")
        .or_else(|| trimmed.strip_prefix("let "))?;

    let end = rest
        .find(|c: char| c == ':' || c == '=' || c == ' ')
        .unwrap_or(rest.len());
    let name = rest[..end].trim().to_string();

    if !name.is_empty() && is_valid_identifier(&name) {
        Some(name)
    } else {
        None
    }
}

/// Extract struct name.
fn extract_struct_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("struct ")
        .or_else(|| trimmed.strip_prefix("pub struct "))?;

    let end = rest
        .find(|c: char| c == '<' || c == '{' || c == '(' || c == ' ')
        .unwrap_or(rest.len());
    let name = rest[..end].trim().to_string();

    if !name.is_empty() && is_valid_identifier(&name) {
        Some(name)
    } else {
        None
    }
}

/// Check if a string is a valid identifier.
fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => chars.all(|c| c.is_alphanumeric() || c == '_'),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_fn_name() {
        let result = extract_fn_name("fn hello_world(x: i32) -> i32 {");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.name, "hello_world");
    }

    #[test]
    fn test_extract_let_binding() {
        assert_eq!(extract_let_binding("let x = 5;"), Some("x".to_string()));
        assert_eq!(
            extract_let_binding("let mut y: i32 = 10;"),
            Some("y".to_string())
        );
    }

    #[test]
    fn test_extract_struct_name() {
        assert_eq!(extract_struct_name("struct Foo {"), Some("Foo".to_string()));
        assert_eq!(
            extract_struct_name("pub struct Bar<T> {"),
            Some("Bar".to_string())
        );
    }
}
