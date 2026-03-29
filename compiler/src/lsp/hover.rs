// ===============================================================================
// QUANTALANG LSP HOVER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Hover information provider for QuantaLang.

use super::document::{Document, DocumentStore};
use super::types::*;
use std::sync::Arc;

// =============================================================================
// HOVER PROVIDER
// =============================================================================

/// Provides hover information.
pub struct HoverProvider {
    /// Document store reference.
    documents: Arc<DocumentStore>,
}

impl HoverProvider {
    /// Create a new hover provider.
    pub fn new(documents: Arc<DocumentStore>) -> Self {
        Self { documents }
    }

    /// Provide hover information at a position.
    pub fn provide(&self, doc: &Document, position: Position) -> Option<Hover> {
        // Get word at position
        let (word, range) = doc.word_at(position)?;

        // Check for keyword documentation
        if let Some(content) = keyword_hover(&word) {
            return Some(Hover::new(content).with_range(range));
        }

        // Check for builtin type documentation
        if let Some(content) = builtin_type_hover(&word) {
            return Some(Hover::new(content).with_range(range));
        }

        // Check for local definitions in the document
        if let Some(content) = self.local_definition_hover(doc, &word) {
            return Some(Hover::new(content).with_range(range));
        }

        // Check for common stdlib functions
        if let Some(content) = stdlib_hover(&word) {
            return Some(Hover::new(content).with_range(range));
        }

        None
    }

    /// Get hover for local definitions.
    fn local_definition_hover(&self, doc: &Document, name: &str) -> Option<MarkupContent> {
        let content = &doc.content;

        // Search for function definition
        for line in content.lines() {
            let trimmed = line.trim();

            // Function definition
            if let Some(rest) = trimmed.strip_prefix("fn ") {
                if rest.starts_with(name) && rest[name.len()..].trim_start().starts_with('(') {
                    let end = line.find('{').unwrap_or(line.len());
                    let signature = line[..end].trim();
                    return Some(MarkupContent::markdown(format!(
                        "```quanta\n{}\n```\n\nFunction defined in this file.",
                        signature
                    )));
                }
            }

            // Struct definition
            if let Some(rest) = trimmed
                .strip_prefix("struct ")
                .or_else(|| trimmed.strip_prefix("pub struct "))
            {
                let struct_name = rest
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .next()
                    .unwrap_or("");
                if struct_name == name {
                    return Some(MarkupContent::markdown(format!(
                        "```quanta\nstruct {}\n```\n\nStruct defined in this file.",
                        name
                    )));
                }
            }

            // Enum definition
            if let Some(rest) = trimmed
                .strip_prefix("enum ")
                .or_else(|| trimmed.strip_prefix("pub enum "))
            {
                let enum_name = rest
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .next()
                    .unwrap_or("");
                if enum_name == name {
                    return Some(MarkupContent::markdown(format!(
                        "```quanta\nenum {}\n```\n\nEnum defined in this file.",
                        name
                    )));
                }
            }

            // Trait definition
            if let Some(rest) = trimmed
                .strip_prefix("trait ")
                .or_else(|| trimmed.strip_prefix("pub trait "))
            {
                let trait_name = rest
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .next()
                    .unwrap_or("");
                if trait_name == name {
                    return Some(MarkupContent::markdown(format!(
                        "```quanta\ntrait {}\n```\n\nTrait defined in this file.",
                        name
                    )));
                }
            }

            // Let binding
            if let Some(rest) = trimmed
                .strip_prefix("let ")
                .or_else(|| trimmed.strip_prefix("let mut "))
            {
                let var_end = rest
                    .find(|c: char| c == ':' || c == '=' || c == ' ')
                    .unwrap_or(rest.len());
                let var_name = rest[..var_end].trim();
                if var_name == name {
                    // Try to extract type annotation
                    if let Some(colon_pos) = rest.find(':') {
                        let type_start = colon_pos + 1;
                        let type_end = rest[type_start..]
                            .find('=')
                            .map(|i| type_start + i)
                            .unwrap_or(rest.len());
                        let type_name = rest[type_start..type_end].trim();
                        return Some(MarkupContent::markdown(format!(
                            "```quanta\nlet {}: {}\n```\n\nLocal variable.",
                            name, type_name
                        )));
                    }
                    return Some(MarkupContent::markdown(format!(
                        "```quanta\nlet {}\n```\n\nLocal variable.",
                        name
                    )));
                }
            }

            // Const binding
            if let Some(rest) = trimmed
                .strip_prefix("const ")
                .or_else(|| trimmed.strip_prefix("pub const "))
            {
                let const_end = rest.find(':').unwrap_or(rest.len());
                let const_name = rest[..const_end].trim();
                if const_name == name {
                    let eq_pos = rest.find('=').unwrap_or(rest.len());
                    let signature = &rest[..eq_pos.min(rest.len())].trim();
                    return Some(MarkupContent::markdown(format!(
                        "```quanta\nconst {}\n```\n\nConstant.",
                        signature
                    )));
                }
            }
        }

        None
    }
}

/// Get hover content for a keyword.
fn keyword_hover(word: &str) -> Option<MarkupContent> {
    let content = match word {
        "fn" => "**fn** - Function declaration\n\nDeclares a function.\n\n```quanta\nfn name(params) -> ReturnType {\n    // body\n}\n```",
        "let" => "**let** - Variable binding\n\nCreates an immutable variable binding.\n\n```quanta\nlet x = 5;\nlet y: i32 = 10;\n```",
        "mut" => "**mut** - Mutable\n\nMarks a binding or reference as mutable.\n\n```quanta\nlet mut x = 5;\nx = 10; // OK\n```",
        "const" => "**const** - Constant\n\nDeclares a compile-time constant.\n\n```quanta\nconst PI: f64 = 3.14159;\n```",
        "struct" => "**struct** - Structure\n\nDeclares a struct type.\n\n```quanta\nstruct Point {\n    x: f64,\n    y: f64,\n}\n```",
        "enum" => "**enum** - Enumeration\n\nDeclares an enum type with variants.\n\n```quanta\nenum Option<T> {\n    Some(T),\n    None,\n}\n```",
        "trait" => "**trait** - Trait\n\nDeclares a trait (interface).\n\n```quanta\ntrait Display {\n    fn display(&self) -> String;\n}\n```",
        "impl" => "**impl** - Implementation\n\nProvides implementations for types or traits.\n\n```quanta\nimpl Point {\n    fn new(x: f64, y: f64) -> Self {\n        Self { x, y }\n    }\n}\n```",
        "if" => "**if** - Conditional\n\nConditional expression or statement.\n\n```quanta\nif condition {\n    // then\n} else {\n    // else\n}\n```",
        "else" => "**else** - Else clause\n\nAlternative branch for `if` or `match`.",
        "match" => "**match** - Pattern matching\n\nPattern matching expression.\n\n```quanta\nmatch value {\n    Pattern1 => expr1,\n    Pattern2 => expr2,\n    _ => default,\n}\n```",
        "for" => "**for** - For loop\n\nIterates over an iterator.\n\n```quanta\nfor item in collection {\n    // body\n}\n```",
        "while" => "**while** - While loop\n\nLoops while condition is true.\n\n```quanta\nwhile condition {\n    // body\n}\n```",
        "loop" => "**loop** - Infinite loop\n\nLoops forever until `break`.\n\n```quanta\nloop {\n    if done {\n        break;\n    }\n}\n```",
        "break" => "**break** - Break\n\nExits a loop early.",
        "continue" => "**continue** - Continue\n\nSkips to the next iteration of a loop.",
        "return" => "**return** - Return\n\nReturns a value from a function.\n\n```quanta\nreturn value;\n```",
        "pub" => "**pub** - Public\n\nMakes an item publicly visible outside its module.",
        "mod" => "**mod** - Module\n\nDeclares or references a module.\n\n```quanta\nmod utils;\nmod inner { /* items */ }\n```",
        "use" => "**use** - Use\n\nBrings items into scope.\n\n```quanta\nuse std::collections::HashMap;\n```",
        "async" => "**async** - Async\n\nMarks a function as asynchronous.\n\n```quanta\nasync fn fetch_data() -> Data {\n    // async body\n}\n```",
        "await" => "**await** - Await\n\nWaits for an async operation to complete.\n\n```quanta\nlet data = fetch_data().await;\n```",
        "unsafe" => "**unsafe** - Unsafe\n\nMarks code as potentially unsafe.\n\n```quanta\nunsafe {\n    // unsafe operations\n}\n```",
        "where" => "**where** - Where clause\n\nAdds constraints to generic types.\n\n```quanta\nfn foo<T>(x: T) where T: Clone { }\n```",
        "type" => "**type** - Type alias\n\nCreates a type alias.\n\n```quanta\ntype Result<T> = std::result::Result<T, Error>;\n```",
        "self" => "**self** - Self reference\n\nRefers to the current instance in methods.",
        "Self" => "**Self** - Self type\n\nRefers to the implementing type in traits/impls.",
        "super" => "**super** - Super\n\nRefers to the parent module.",
        "true" => "**true** - Boolean true\n\nThe boolean value `true`.",
        "false" => "**false** - Boolean false\n\nThe boolean value `false`.",
        _ => return None,
    };
    Some(MarkupContent::markdown(content))
}

/// Get hover content for a builtin type.
fn builtin_type_hover(word: &str) -> Option<MarkupContent> {
    let content = match word {
        "i8" => "**i8** - 8-bit signed integer\n\nRange: -128 to 127",
        "i16" => "**i16** - 16-bit signed integer\n\nRange: -32,768 to 32,767",
        "i32" => "**i32** - 32-bit signed integer\n\nRange: -2,147,483,648 to 2,147,483,647",
        "i64" => "**i64** - 64-bit signed integer\n\nRange: -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807",
        "i128" => "**i128** - 128-bit signed integer",
        "isize" => "**isize** - Pointer-sized signed integer\n\nSize depends on target architecture.",
        "u8" => "**u8** - 8-bit unsigned integer\n\nRange: 0 to 255",
        "u16" => "**u16** - 16-bit unsigned integer\n\nRange: 0 to 65,535",
        "u32" => "**u32** - 32-bit unsigned integer\n\nRange: 0 to 4,294,967,295",
        "u64" => "**u64** - 64-bit unsigned integer\n\nRange: 0 to 18,446,744,073,709,551,615",
        "u128" => "**u128** - 128-bit unsigned integer",
        "usize" => "**usize** - Pointer-sized unsigned integer\n\nSize depends on target architecture. Used for indexing.",
        "f32" => "**f32** - 32-bit floating point\n\nIEEE 754 single precision.",
        "f64" => "**f64** - 64-bit floating point\n\nIEEE 754 double precision.",
        "bool" => "**bool** - Boolean type\n\nEither `true` or `false`.",
        "char" => "**char** - Unicode scalar value\n\n4 bytes, represents a Unicode scalar value.",
        "str" => "**str** - String slice\n\nUTF-8 encoded string slice. Usually seen as `&str`.",
        "String" => "**String** - Owned string\n\nA growable, UTF-8 encoded string.\n\n```quanta\nlet s = String::from(\"hello\");\nlet s = \"hello\".to_string();\n```",
        "Vec" => "**Vec<T>** - Vector\n\nA growable array type.\n\n```quanta\nlet v: Vec<i32> = vec![1, 2, 3];\n```",
        "Option" => "**Option<T>** - Optional value\n\nRepresents either `Some(T)` or `None`.\n\n```quanta\nlet x: Option<i32> = Some(5);\nlet y: Option<i32> = None;\n```",
        "Result" => "**Result<T, E>** - Result type\n\nRepresents either `Ok(T)` or `Err(E)`.\n\n```quanta\nlet x: Result<i32, Error> = Ok(5);\nlet y: Result<i32, Error> = Err(error);\n```",
        "Box" => "**Box<T>** - Heap allocation\n\nA pointer to data allocated on the heap.\n\n```quanta\nlet b = Box::new(5);\n```",
        "Arc" => "**Arc<T>** - Atomic reference counting\n\nThread-safe reference-counted pointer.\n\n```quanta\nlet a = Arc::new(data);\n```",
        "Rc" => "**Rc<T>** - Reference counting\n\nSingle-threaded reference-counted pointer.\n\n```quanta\nlet r = Rc::new(data);\n```",
        "HashMap" => "**HashMap<K, V>** - Hash map\n\nA hash table providing O(1) average lookup.\n\n```quanta\nlet mut map = HashMap::new();\nmap.insert(\"key\", \"value\");\n```",
        "HashSet" => "**HashSet<T>** - Hash set\n\nA set implemented with a hash table.\n\n```quanta\nlet mut set = HashSet::new();\nset.insert(item);\n```",
        _ => return None,
    };
    Some(MarkupContent::markdown(content))
}

/// Get hover content for stdlib functions.
fn stdlib_hover(word: &str) -> Option<MarkupContent> {
    let content = match word {
        "println" => "**println!** - Print line macro\n\nPrints formatted text to stdout with a newline.\n\n```quanta\nprintln!(\"Hello, {}!\", name);\n```",
        "print" => "**print!** - Print macro\n\nPrints formatted text to stdout without a newline.\n\n```quanta\nprint!(\"Loading...\");\n```",
        "format" => "**format!** - Format macro\n\nCreates a formatted String.\n\n```quanta\nlet s = format!(\"x = {}, y = {}\", x, y);\n```",
        "vec" => "**vec!** - Vector macro\n\nCreates a Vec with the given elements.\n\n```quanta\nlet v = vec![1, 2, 3];\n```",
        "panic" => "**panic!** - Panic macro\n\nTerminates the program with an error message.\n\n```quanta\npanic!(\"Something went wrong!\");\n```",
        "assert" => "**assert!** - Assert macro\n\nPanics if the condition is false.\n\n```quanta\nassert!(x > 0);\n```",
        "assert_eq" => "**assert_eq!** - Assert equality macro\n\nPanics if the two values are not equal.\n\n```quanta\nassert_eq!(a, b);\n```",
        "dbg" => "**dbg!** - Debug macro\n\nPrints the value and returns it.\n\n```quanta\nlet x = dbg!(calculate());\n```",
        "todo" => "**todo!** - Todo macro\n\nMarks code as not yet implemented. Panics at runtime.\n\n```quanta\nfn not_done() {\n    todo!(\"implement this\")\n}\n```",
        "unimplemented" => "**unimplemented!** - Unimplemented macro\n\nMarks code that should never be reached.\n\n```quanta\nmatch impossible {\n    _ => unimplemented!(),\n}\n```",
        "unreachable" => "**unreachable!** - Unreachable macro\n\nIndicates unreachable code paths.\n\n```quanta\nif false {\n    unreachable!()\n}\n```",
        _ => return None,
    };
    Some(MarkupContent::markdown(content))
}
