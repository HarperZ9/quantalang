// ===============================================================================
// QUANTALANG MACRO EXPANSION - BUILT-IN MACROS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Built-in macros.
//!
//! This module provides built-in macros like `println!`, `vec!`, `format!`, etc.


use crate::lexer::{TokenKind, Span, Delimiter, Keyword, LiteralKind};

use super::{
    MacroContext, MacroDef, MacroRule, MacroPattern, PatternElement,
    MacroExpansion, ExpansionElement, MetaVarKind, RepetitionKind, MacroId,
};

// =============================================================================
// BUILT-IN MACRO REGISTRATION
// =============================================================================

/// Register all built-in macros in the context.
pub fn register_builtins(ctx: &mut MacroContext) {
    // Printing and formatting
    register_println(ctx);
    register_print(ctx);
    register_eprintln(ctx);
    register_eprint(ctx);
    register_format(ctx);
    register_format_args(ctx);

    // Assertions
    register_assert(ctx);
    register_assert_eq(ctx);
    register_assert_ne(ctx);
    register_debug_assert(ctx);

    // Collections
    register_vec(ctx);

    // Debugging
    register_dbg(ctx);
    register_todo(ctx);
    register_unimplemented(ctx);
    register_unreachable(ctx);

    // Code generation
    register_stringify(ctx);
    register_concat(ctx);
    register_include(ctx);
    register_include_str(ctx);
    register_include_bytes(ctx);

    // Environment
    register_env(ctx);
    register_option_env(ctx);
    register_cfg(ctx);

    // Compile-time
    register_compile_error(ctx);
    register_line(ctx);
    register_column(ctx);
    register_file(ctx);
    register_module_path(ctx);
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Create a simple macro with a single rule.
fn simple_macro(
    name: &str,
    pattern: Vec<PatternElement>,
    expansion: Vec<ExpansionElement>,
) -> MacroDef {
    MacroDef {
        id: MacroId::fresh(),
        name: name.into(),
        rules: vec![MacroRule {
            pattern: MacroPattern { elements: pattern },
            expansion: MacroExpansion { elements: expansion },
            span: Span::dummy(),
        }],
        is_exported: true,
        span: Span::dummy(),
    }
}

/// Create a pattern for `($($arg:tt)*)`.
fn variadic_tt_pattern() -> Vec<PatternElement> {
    vec![PatternElement::Repetition {
        elements: vec![PatternElement::MetaVar {
            name: "arg".into(),
            kind: MetaVarKind::TokenTree,
        }],
        separator: None,
        repetition: RepetitionKind::ZeroOrMore,
    }]
}

/// Create an expansion for `$($arg)*`.
fn variadic_tt_expansion() -> Vec<ExpansionElement> {
    vec![ExpansionElement::Repetition {
        elements: vec![ExpansionElement::MetaVar("arg".into())],
        separator: None,
        repetition: RepetitionKind::ZeroOrMore,
    }]
}

// =============================================================================
// PRINTING MACROS
// =============================================================================

fn register_println(ctx: &mut MacroContext) {
    // println!() -> std::io::_print(format_args!("\n"))
    // println!($fmt:expr) -> std::io::_print(format_args!(concat!($fmt, "\n")))
    // println!($fmt:expr, $($arg:tt)*) -> std::io::_print(format_args!(concat!($fmt, "\n"), $($arg)*))

    let def = MacroDef {
        id: MacroId::fresh(),
        name: "println".into(),
        rules: vec![
            // println!()
            MacroRule {
                pattern: MacroPattern { elements: vec![] },
                expansion: MacroExpansion {
                    elements: vec![
                        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // std
                        ExpansionElement::Token(TokenKind::ColonColon, Span::dummy()),
                        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // io
                        ExpansionElement::Token(TokenKind::ColonColon, Span::dummy()),
                        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // writeln
                        ExpansionElement::Token(TokenKind::Not, Span::dummy()),
                        ExpansionElement::Delimited {
                            delimiter: Delimiter::Paren,
                            elements: vec![
                                ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // stdout
                            ],
                            span: Span::dummy(),
                        },
                    ],
                },
                span: Span::dummy(),
            },
            // println!($($arg:tt)*)
            MacroRule {
                pattern: MacroPattern {
                    elements: variadic_tt_pattern(),
                },
                expansion: MacroExpansion {
                    elements: vec![
                        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // print_macro
                        ExpansionElement::Token(TokenKind::Not, Span::dummy()),
                        ExpansionElement::Delimited {
                            delimiter: Delimiter::Paren,
                            elements: variadic_tt_expansion(),
                            span: Span::dummy(),
                        },
                    ],
                },
                span: Span::dummy(),
            },
        ],
        is_exported: true,
        span: Span::dummy(),
    };
    ctx.register_macro(def);
}

fn register_print(ctx: &mut MacroContext) {
    let def = simple_macro("print", variadic_tt_pattern(), vec![
        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // print_impl
        ExpansionElement::Token(TokenKind::Not, Span::dummy()),
        ExpansionElement::Delimited {
            delimiter: Delimiter::Paren,
            elements: variadic_tt_expansion(),
            span: Span::dummy(),
        },
    ]);
    ctx.register_macro(def);
}

fn register_eprintln(ctx: &mut MacroContext) {
    let def = simple_macro("eprintln", variadic_tt_pattern(), vec![
        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // eprintln_impl
        ExpansionElement::Token(TokenKind::Not, Span::dummy()),
        ExpansionElement::Delimited {
            delimiter: Delimiter::Paren,
            elements: variadic_tt_expansion(),
            span: Span::dummy(),
        },
    ]);
    ctx.register_macro(def);
}

fn register_eprint(ctx: &mut MacroContext) {
    let def = simple_macro("eprint", variadic_tt_pattern(), vec![
        ExpansionElement::Token(TokenKind::Ident, Span::dummy()),
        ExpansionElement::Token(TokenKind::Not, Span::dummy()),
        ExpansionElement::Delimited {
            delimiter: Delimiter::Paren,
            elements: variadic_tt_expansion(),
            span: Span::dummy(),
        },
    ]);
    ctx.register_macro(def);
}

fn register_format(ctx: &mut MacroContext) {
    let def = simple_macro("format", variadic_tt_pattern(), vec![
        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // String
        ExpansionElement::Token(TokenKind::ColonColon, Span::dummy()),
        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // from
        ExpansionElement::Delimited {
            delimiter: Delimiter::Paren,
            elements: vec![
                ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // format_args
                ExpansionElement::Token(TokenKind::Not, Span::dummy()),
                ExpansionElement::Delimited {
                    delimiter: Delimiter::Paren,
                    elements: variadic_tt_expansion(),
                    span: Span::dummy(),
                },
            ],
            span: Span::dummy(),
        },
    ]);
    ctx.register_macro(def);
}

fn register_format_args(ctx: &mut MacroContext) {
    // format_args! is a compiler intrinsic
    let def = simple_macro("format_args", variadic_tt_pattern(), variadic_tt_expansion());
    ctx.register_macro(def);
}

// =============================================================================
// ASSERTION MACROS
// =============================================================================

fn register_assert(ctx: &mut MacroContext) {
    let def = MacroDef {
        id: MacroId::fresh(),
        name: "assert".into(),
        rules: vec![
            // assert!($cond:expr)
            MacroRule {
                pattern: MacroPattern {
                    elements: vec![PatternElement::MetaVar {
                        name: "cond".into(),
                        kind: MetaVarKind::Expr,
                    }],
                },
                expansion: MacroExpansion {
                    elements: vec![
                        ExpansionElement::Token(TokenKind::Keyword(Keyword::If), Span::dummy()),
                        ExpansionElement::Token(TokenKind::Not, Span::dummy()),
                        ExpansionElement::Delimited {
                            delimiter: Delimiter::Paren,
                            elements: vec![ExpansionElement::MetaVar("cond".into())],
                            span: Span::dummy(),
                        },
                        ExpansionElement::Delimited {
                            delimiter: Delimiter::Brace,
                            elements: vec![
                                ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // panic
                                ExpansionElement::Token(TokenKind::Not, Span::dummy()),
                                ExpansionElement::Delimited {
                                    delimiter: Delimiter::Paren,
                                    elements: vec![
                                        ExpansionElement::Token(
                                            TokenKind::Literal {
                                                kind: LiteralKind::Str { terminated: true },
                                                suffix: None,
                                            },
                                            Span::dummy(),
                                        ),
                                    ],
                                    span: Span::dummy(),
                                },
                            ],
                            span: Span::dummy(),
                        },
                    ],
                },
                span: Span::dummy(),
            },
            // assert!($cond:expr, $($arg:tt)*)
            MacroRule {
                pattern: MacroPattern {
                    elements: vec![
                        PatternElement::MetaVar {
                            name: "cond".into(),
                            kind: MetaVarKind::Expr,
                        },
                        PatternElement::Token(TokenKind::Comma),
                        PatternElement::Repetition {
                            elements: vec![PatternElement::MetaVar {
                                name: "arg".into(),
                                kind: MetaVarKind::TokenTree,
                            }],
                            separator: None,
                            repetition: RepetitionKind::ZeroOrMore,
                        },
                    ],
                },
                expansion: MacroExpansion {
                    elements: vec![
                        ExpansionElement::Token(TokenKind::Keyword(Keyword::If), Span::dummy()),
                        ExpansionElement::Token(TokenKind::Not, Span::dummy()),
                        ExpansionElement::Delimited {
                            delimiter: Delimiter::Paren,
                            elements: vec![ExpansionElement::MetaVar("cond".into())],
                            span: Span::dummy(),
                        },
                        ExpansionElement::Delimited {
                            delimiter: Delimiter::Brace,
                            elements: vec![
                                ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // panic
                                ExpansionElement::Token(TokenKind::Not, Span::dummy()),
                                ExpansionElement::Delimited {
                                    delimiter: Delimiter::Paren,
                                    elements: vec![ExpansionElement::Repetition {
                                        elements: vec![ExpansionElement::MetaVar("arg".into())],
                                        separator: None,
                                        repetition: RepetitionKind::ZeroOrMore,
                                    }],
                                    span: Span::dummy(),
                                },
                            ],
                            span: Span::dummy(),
                        },
                    ],
                },
                span: Span::dummy(),
            },
        ],
        is_exported: true,
        span: Span::dummy(),
    };
    ctx.register_macro(def);
}

fn register_assert_eq(ctx: &mut MacroContext) {
    let def = simple_macro("assert_eq",
        vec![
            PatternElement::MetaVar { name: "left".into(), kind: MetaVarKind::Expr },
            PatternElement::Token(TokenKind::Comma),
            PatternElement::MetaVar { name: "right".into(), kind: MetaVarKind::Expr },
        ],
        vec![
            ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // assert_eq_impl
            ExpansionElement::Token(TokenKind::Not, Span::dummy()),
            ExpansionElement::Delimited {
                delimiter: Delimiter::Paren,
                elements: vec![
                    ExpansionElement::MetaVar("left".into()),
                    ExpansionElement::Token(TokenKind::Comma, Span::dummy()),
                    ExpansionElement::MetaVar("right".into()),
                ],
                span: Span::dummy(),
            },
        ],
    );
    ctx.register_macro(def);
}

fn register_assert_ne(ctx: &mut MacroContext) {
    let def = simple_macro("assert_ne",
        vec![
            PatternElement::MetaVar { name: "left".into(), kind: MetaVarKind::Expr },
            PatternElement::Token(TokenKind::Comma),
            PatternElement::MetaVar { name: "right".into(), kind: MetaVarKind::Expr },
        ],
        vec![
            ExpansionElement::Token(TokenKind::Ident, Span::dummy()),
            ExpansionElement::Token(TokenKind::Not, Span::dummy()),
            ExpansionElement::Delimited {
                delimiter: Delimiter::Paren,
                elements: vec![
                    ExpansionElement::MetaVar("left".into()),
                    ExpansionElement::Token(TokenKind::Comma, Span::dummy()),
                    ExpansionElement::MetaVar("right".into()),
                ],
                span: Span::dummy(),
            },
        ],
    );
    ctx.register_macro(def);
}

fn register_debug_assert(ctx: &mut MacroContext) {
    let def = simple_macro("debug_assert", variadic_tt_pattern(), vec![
        ExpansionElement::Token(TokenKind::Keyword(Keyword::If), Span::dummy()),
        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // cfg!
        ExpansionElement::Token(TokenKind::Not, Span::dummy()),
        ExpansionElement::Delimited {
            delimiter: Delimiter::Paren,
            elements: vec![ExpansionElement::Token(TokenKind::Ident, Span::dummy())], // debug_assertions
            span: Span::dummy(),
        },
        ExpansionElement::Delimited {
            delimiter: Delimiter::Brace,
            elements: vec![
                ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // assert
                ExpansionElement::Token(TokenKind::Not, Span::dummy()),
                ExpansionElement::Delimited {
                    delimiter: Delimiter::Paren,
                    elements: variadic_tt_expansion(),
                    span: Span::dummy(),
                },
            ],
            span: Span::dummy(),
        },
    ]);
    ctx.register_macro(def);
}

// =============================================================================
// COLLECTION MACROS
// =============================================================================

fn register_vec(ctx: &mut MacroContext) {
    let def = MacroDef {
        id: MacroId::fresh(),
        name: "vec".into(),
        rules: vec![
            // vec![]
            MacroRule {
                pattern: MacroPattern { elements: vec![] },
                expansion: MacroExpansion {
                    elements: vec![
                        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // Vec
                        ExpansionElement::Token(TokenKind::ColonColon, Span::dummy()),
                        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // new
                        ExpansionElement::Delimited {
                            delimiter: Delimiter::Paren,
                            elements: vec![],
                            span: Span::dummy(),
                        },
                    ],
                },
                span: Span::dummy(),
            },
            // vec![$elem; $n]
            MacroRule {
                pattern: MacroPattern {
                    elements: vec![
                        PatternElement::MetaVar { name: "elem".into(), kind: MetaVarKind::Expr },
                        PatternElement::Token(TokenKind::Semi),
                        PatternElement::MetaVar { name: "n".into(), kind: MetaVarKind::Expr },
                    ],
                },
                expansion: MacroExpansion {
                    elements: vec![
                        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // vec
                        ExpansionElement::Token(TokenKind::ColonColon, Span::dummy()),
                        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // from_elem
                        ExpansionElement::Delimited {
                            delimiter: Delimiter::Paren,
                            elements: vec![
                                ExpansionElement::MetaVar("elem".into()),
                                ExpansionElement::Token(TokenKind::Comma, Span::dummy()),
                                ExpansionElement::MetaVar("n".into()),
                            ],
                            span: Span::dummy(),
                        },
                    ],
                },
                span: Span::dummy(),
            },
            // vec![$($elem:expr),* $(,)?]
            MacroRule {
                pattern: MacroPattern {
                    elements: vec![
                        PatternElement::Repetition {
                            elements: vec![PatternElement::MetaVar {
                                name: "elem".into(),
                                kind: MetaVarKind::Expr,
                            }],
                            separator: Some(TokenKind::Comma),
                            repetition: RepetitionKind::ZeroOrMore,
                        },
                        PatternElement::Repetition {
                            elements: vec![PatternElement::Token(TokenKind::Comma)],
                            separator: None,
                            repetition: RepetitionKind::ZeroOrOne,
                        },
                    ],
                },
                expansion: MacroExpansion {
                    elements: vec![
                        ExpansionElement::Delimited {
                            delimiter: Delimiter::Brace,
                            elements: vec![
                                ExpansionElement::Token(TokenKind::Keyword(Keyword::Let), Span::dummy()),
                                ExpansionElement::Token(TokenKind::Keyword(Keyword::Mut), Span::dummy()),
                                ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // v
                                ExpansionElement::Token(TokenKind::Eq, Span::dummy()),
                                ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // Vec
                                ExpansionElement::Token(TokenKind::ColonColon, Span::dummy()),
                                ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // new
                                ExpansionElement::Delimited {
                                    delimiter: Delimiter::Paren,
                                    elements: vec![],
                                    span: Span::dummy(),
                                },
                                ExpansionElement::Token(TokenKind::Semi, Span::dummy()),
                                ExpansionElement::Repetition {
                                    elements: vec![
                                        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // v
                                        ExpansionElement::Token(TokenKind::Dot, Span::dummy()),
                                        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // push
                                        ExpansionElement::Delimited {
                                            delimiter: Delimiter::Paren,
                                            elements: vec![ExpansionElement::MetaVar("elem".into())],
                                            span: Span::dummy(),
                                        },
                                        ExpansionElement::Token(TokenKind::Semi, Span::dummy()),
                                    ],
                                    separator: None,
                                    repetition: RepetitionKind::ZeroOrMore,
                                },
                                ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // v
                            ],
                            span: Span::dummy(),
                        },
                    ],
                },
                span: Span::dummy(),
            },
        ],
        is_exported: true,
        span: Span::dummy(),
    };
    ctx.register_macro(def);
}

// =============================================================================
// DEBUGGING MACROS
// =============================================================================

fn register_dbg(ctx: &mut MacroContext) {
    let def = simple_macro("dbg", variadic_tt_pattern(), variadic_tt_expansion());
    ctx.register_macro(def);
}

fn register_todo(ctx: &mut MacroContext) {
    let def = simple_macro("todo", variadic_tt_pattern(), vec![
        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // panic
        ExpansionElement::Token(TokenKind::Not, Span::dummy()),
        ExpansionElement::Delimited {
            delimiter: Delimiter::Paren,
            elements: variadic_tt_expansion(),
            span: Span::dummy(),
        },
    ]);
    ctx.register_macro(def);
}

fn register_unimplemented(ctx: &mut MacroContext) {
    let def = simple_macro("unimplemented", variadic_tt_pattern(), vec![
        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // panic
        ExpansionElement::Token(TokenKind::Not, Span::dummy()),
        ExpansionElement::Delimited {
            delimiter: Delimiter::Paren,
            elements: variadic_tt_expansion(),
            span: Span::dummy(),
        },
    ]);
    ctx.register_macro(def);
}

fn register_unreachable(ctx: &mut MacroContext) {
    let def = simple_macro("unreachable", variadic_tt_pattern(), vec![
        ExpansionElement::Token(TokenKind::Ident, Span::dummy()), // unreachable_impl
        ExpansionElement::Token(TokenKind::Not, Span::dummy()),
        ExpansionElement::Delimited {
            delimiter: Delimiter::Paren,
            elements: variadic_tt_expansion(),
            span: Span::dummy(),
        },
    ]);
    ctx.register_macro(def);
}

// =============================================================================
// CODE GENERATION MACROS
// =============================================================================

fn register_stringify(ctx: &mut MacroContext) {
    let def = simple_macro("stringify", variadic_tt_pattern(), variadic_tt_expansion());
    ctx.register_macro(def);
}

fn register_concat(ctx: &mut MacroContext) {
    let def = simple_macro("concat", variadic_tt_pattern(), variadic_tt_expansion());
    ctx.register_macro(def);
}

fn register_include(ctx: &mut MacroContext) {
    let def = simple_macro("include",
        vec![PatternElement::MetaVar { name: "file".into(), kind: MetaVarKind::Literal }],
        vec![ExpansionElement::MetaVar("file".into())],
    );
    ctx.register_macro(def);
}

fn register_include_str(ctx: &mut MacroContext) {
    let def = simple_macro("include_str",
        vec![PatternElement::MetaVar { name: "file".into(), kind: MetaVarKind::Literal }],
        vec![ExpansionElement::MetaVar("file".into())],
    );
    ctx.register_macro(def);
}

fn register_include_bytes(ctx: &mut MacroContext) {
    let def = simple_macro("include_bytes",
        vec![PatternElement::MetaVar { name: "file".into(), kind: MetaVarKind::Literal }],
        vec![ExpansionElement::MetaVar("file".into())],
    );
    ctx.register_macro(def);
}

// =============================================================================
// ENVIRONMENT MACROS
// =============================================================================

fn register_env(ctx: &mut MacroContext) {
    let def = simple_macro("env",
        vec![PatternElement::MetaVar { name: "name".into(), kind: MetaVarKind::Literal }],
        vec![ExpansionElement::MetaVar("name".into())],
    );
    ctx.register_macro(def);
}

fn register_option_env(ctx: &mut MacroContext) {
    let def = simple_macro("option_env",
        vec![PatternElement::MetaVar { name: "name".into(), kind: MetaVarKind::Literal }],
        vec![ExpansionElement::MetaVar("name".into())],
    );
    ctx.register_macro(def);
}

fn register_cfg(ctx: &mut MacroContext) {
    let def = simple_macro("cfg", variadic_tt_pattern(), vec![
        ExpansionElement::Token(
            TokenKind::Literal { kind: LiteralKind::Bool(true), suffix: None },
            Span::dummy(),
        ),
    ]);
    ctx.register_macro(def);
}

// =============================================================================
// COMPILE-TIME MACROS
// =============================================================================

fn register_compile_error(ctx: &mut MacroContext) {
    let def = simple_macro("compile_error",
        vec![PatternElement::MetaVar { name: "msg".into(), kind: MetaVarKind::Literal }],
        vec![ExpansionElement::MetaVar("msg".into())],
    );
    ctx.register_macro(def);
}

fn register_line(ctx: &mut MacroContext) {
    let def = simple_macro("line", vec![], vec![
        ExpansionElement::Token(
            TokenKind::Literal {
                kind: LiteralKind::Int { base: crate::lexer::IntBase::Decimal, empty_int: false },
                suffix: None,
            },
            Span::dummy(),
        ),
    ]);
    ctx.register_macro(def);
}

fn register_column(ctx: &mut MacroContext) {
    let def = simple_macro("column", vec![], vec![
        ExpansionElement::Token(
            TokenKind::Literal {
                kind: LiteralKind::Int { base: crate::lexer::IntBase::Decimal, empty_int: false },
                suffix: None,
            },
            Span::dummy(),
        ),
    ]);
    ctx.register_macro(def);
}

fn register_file(ctx: &mut MacroContext) {
    let def = simple_macro("file", vec![], vec![
        ExpansionElement::Token(
            TokenKind::Literal { kind: LiteralKind::Str { terminated: true }, suffix: None },
            Span::dummy(),
        ),
    ]);
    ctx.register_macro(def);
}

fn register_module_path(ctx: &mut MacroContext) {
    let def = simple_macro("module_path", vec![], vec![
        ExpansionElement::Token(
            TokenKind::Literal { kind: LiteralKind::Str { terminated: true }, suffix: None },
            Span::dummy(),
        ),
    ]);
    ctx.register_macro(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_builtins() {
        let mut ctx = MacroContext::new();
        register_builtins(&mut ctx);

        // Check that common macros are registered
        assert!(ctx.lookup_macro("println").is_some());
        assert!(ctx.lookup_macro("print").is_some());
        assert!(ctx.lookup_macro("format").is_some());
        assert!(ctx.lookup_macro("vec").is_some());
        assert!(ctx.lookup_macro("assert").is_some());
        assert!(ctx.lookup_macro("assert_eq").is_some());
        assert!(ctx.lookup_macro("dbg").is_some());
        assert!(ctx.lookup_macro("todo").is_some());
    }
}
