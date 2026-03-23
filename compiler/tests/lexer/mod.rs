// ===============================================================================
// QUANTALANG LEXER - INTEGRATION TESTS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// CONFIDENTIAL - Trade Secret - Patent Pending
// ===============================================================================

use quantalang::lexer::{
    tokenize, tokenize_file, Delimiter, IntBase, Keyword, LiteralKind, SourceFile, Token,
    TokenKind, Lexer, LexerConfig,
};

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

fn lex(source: &str) -> Vec<Token> {
    tokenize(source).expect("lexer should succeed")
}

fn lex_kinds(source: &str) -> Vec<TokenKind> {
    lex(source).into_iter().map(|t| t.kind).collect()
}

fn expect_single_token(source: &str, expected: TokenKind) {
    let tokens = lex(source);
    assert!(tokens.len() >= 1, "expected at least one token");
    assert_eq!(
        tokens[0].kind, expected,
        "source: {:?}, got: {:?}",
        source, tokens[0].kind
    );
}

fn expect_error(source: &str) {
    let result = tokenize(source);
    assert!(result.is_err(), "expected error for source: {:?}", source);
}

// =============================================================================
// SINGLE CHARACTER TOKEN TESTS
// =============================================================================

#[test]
fn test_delimiters() {
    expect_single_token("(", TokenKind::OpenDelim(Delimiter::Paren));
    expect_single_token(")", TokenKind::CloseDelim(Delimiter::Paren));
    expect_single_token("[", TokenKind::OpenDelim(Delimiter::Bracket));
    expect_single_token("]", TokenKind::CloseDelim(Delimiter::Bracket));
    expect_single_token("{", TokenKind::OpenDelim(Delimiter::Brace));
    expect_single_token("}", TokenKind::CloseDelim(Delimiter::Brace));
}

#[test]
fn test_punctuation() {
    expect_single_token(",", TokenKind::Comma);
    expect_single_token(";", TokenKind::Semi);
    expect_single_token(":", TokenKind::Colon);
    expect_single_token(".", TokenKind::Dot);
    expect_single_token("?", TokenKind::Question);
    expect_single_token("@", TokenKind::At);
    expect_single_token("#", TokenKind::Pound);
    expect_single_token("$", TokenKind::Dollar);
    expect_single_token("~", TokenKind::Tilde);
}

// =============================================================================
// OPERATOR TESTS
// =============================================================================

#[test]
fn test_arithmetic_operators() {
    expect_single_token("+", TokenKind::Plus);
    expect_single_token("-", TokenKind::Minus);
    expect_single_token("*", TokenKind::Star);
    expect_single_token("/", TokenKind::Slash);
    expect_single_token("%", TokenKind::Percent);
    expect_single_token("^", TokenKind::Caret);
}

#[test]
fn test_comparison_operators() {
    expect_single_token("==", TokenKind::EqEq);
    expect_single_token("!=", TokenKind::Ne);
    expect_single_token("<", TokenKind::Lt);
    expect_single_token("<=", TokenKind::Le);
    expect_single_token(">", TokenKind::Gt);
    expect_single_token(">=", TokenKind::Ge);
}

#[test]
fn test_logical_operators() {
    expect_single_token("&&", TokenKind::AndAnd);
    expect_single_token("||", TokenKind::OrOr);
    expect_single_token("!", TokenKind::Not);
}

#[test]
fn test_bitwise_operators() {
    expect_single_token("&", TokenKind::And);
    expect_single_token("|", TokenKind::Or);
    expect_single_token("<<", TokenKind::Shl);
    expect_single_token(">>", TokenKind::Shr);
}

#[test]
fn test_assignment_operators() {
    expect_single_token("=", TokenKind::Eq);
    expect_single_token("+=", TokenKind::PlusEq);
    expect_single_token("-=", TokenKind::MinusEq);
    expect_single_token("*=", TokenKind::StarEq);
    expect_single_token("/=", TokenKind::SlashEq);
    expect_single_token("%=", TokenKind::PercentEq);
    expect_single_token("^=", TokenKind::CaretEq);
    expect_single_token("&=", TokenKind::AndEq);
    expect_single_token("|=", TokenKind::OrEq);
    expect_single_token("<<=", TokenKind::ShlEq);
    expect_single_token(">>=", TokenKind::ShrEq);
}

#[test]
fn test_special_operators() {
    expect_single_token("->", TokenKind::Arrow);
    expect_single_token("=>", TokenKind::FatArrow);
    expect_single_token("|>", TokenKind::Pipe);
    expect_single_token("::", TokenKind::ColonColon);
    expect_single_token("..", TokenKind::DotDot);
    expect_single_token("...", TokenKind::DotDotDot);
    expect_single_token("..=", TokenKind::DotDotEq);
}

// =============================================================================
// INTEGER LITERAL TESTS
// =============================================================================

#[test]
fn test_decimal_integers() {
    let check = |s| {
        expect_single_token(
            s,
            TokenKind::Literal {
                kind: LiteralKind::Int {
                    base: IntBase::Decimal,
                    empty_int: false,
                },
                suffix: None,
            },
        );
    };

    check("0");
    check("1");
    check("42");
    check("123456789");
    check("1_000_000");
    check("1_2_3_4");
}

#[test]
fn test_hexadecimal_integers() {
    let check = |s| {
        expect_single_token(
            s,
            TokenKind::Literal {
                kind: LiteralKind::Int {
                    base: IntBase::Hexadecimal,
                    empty_int: false,
                },
                suffix: None,
            },
        );
    };

    check("0x0");
    check("0xFF");
    check("0xDEAD_BEEF");
    check("0x123abc");
    check("0X123ABC");
}

#[test]
fn test_octal_integers() {
    let check = |s| {
        expect_single_token(
            s,
            TokenKind::Literal {
                kind: LiteralKind::Int {
                    base: IntBase::Octal,
                    empty_int: false,
                },
                suffix: None,
            },
        );
    };

    check("0o0");
    check("0o755");
    check("0o777");
    check("0O123");
}

#[test]
fn test_binary_integers() {
    let check = |s| {
        expect_single_token(
            s,
            TokenKind::Literal {
                kind: LiteralKind::Int {
                    base: IntBase::Binary,
                    empty_int: false,
                },
                suffix: None,
            },
        );
    };

    check("0b0");
    check("0b1");
    check("0b1010");
    check("0b1111_0000");
    check("0B1010");
}

#[test]
fn test_integer_suffixes() {
    let tokens = lex("42i32");
    assert!(matches!(
        &tokens[0].kind,
        TokenKind::Literal { suffix: Some(s), .. } if s.as_ref() == "i32"
    ));

    let tokens = lex("0xFFu8");
    assert!(matches!(
        &tokens[0].kind,
        TokenKind::Literal { suffix: Some(s), .. } if s.as_ref() == "u8"
    ));
}

// =============================================================================
// FLOAT LITERAL TESTS
// =============================================================================

#[test]
fn test_float_literals() {
    let check = |s| {
        expect_single_token(
            s,
            TokenKind::Literal {
                kind: LiteralKind::Float {
                    empty_exponent: false,
                },
                suffix: None,
            },
        );
    };

    check("0.0");
    check("3.14");
    check("3.14159265");
    check("1.0e10");
    check("1.0E10");
    check("1.5e-3");
    check("2.5E+10");
    check("1e10");
    check("1E10");
    check("1_000.000_001");
}

#[test]
fn test_float_suffixes() {
    let tokens = lex("3.14f32");
    assert!(matches!(
        &tokens[0].kind,
        TokenKind::Literal { suffix: Some(s), .. } if s.as_ref() == "f32"
    ));

    let tokens = lex("1.0f64");
    assert!(matches!(
        &tokens[0].kind,
        TokenKind::Literal { suffix: Some(s), .. } if s.as_ref() == "f64"
    ));
}

// =============================================================================
// STRING LITERAL TESTS
// =============================================================================

#[test]
fn test_string_literals() {
    expect_single_token(
        r#""""#,
        TokenKind::Literal {
            kind: LiteralKind::Str { terminated: true },
            suffix: None,
        },
    );

    expect_single_token(
        r#""hello""#,
        TokenKind::Literal {
            kind: LiteralKind::Str { terminated: true },
            suffix: None,
        },
    );

    expect_single_token(
        r#""hello world""#,
        TokenKind::Literal {
            kind: LiteralKind::Str { terminated: true },
            suffix: None,
        },
    );
}

#[test]
fn test_string_escapes() {
    let cases = [
        r#""hello\nworld""#,
        r#""tab\there""#,
        r#""quote\"here""#,
        r#""backslash\\here""#,
        r#""\x41\x42\x43""#,
        r#""\u{0041}\u{1F600}""#,
    ];

    for case in cases {
        expect_single_token(
            case,
            TokenKind::Literal {
                kind: LiteralKind::Str { terminated: true },
                suffix: None,
            },
        );
    }
}

#[test]
fn test_multiline_strings() {
    let s = "\"line1\nline2\nline3\"";
    expect_single_token(
        s,
        TokenKind::Literal {
            kind: LiteralKind::Str { terminated: true },
            suffix: None,
        },
    );
}

#[test]
fn test_unterminated_string() {
    expect_error(r#""unterminated"#);
}

// =============================================================================
// RAW STRING TESTS
// =============================================================================

#[test]
fn test_raw_strings() {
    expect_single_token(
        r##"r"raw""##,
        TokenKind::Literal {
            kind: LiteralKind::RawStr { n_hashes: Some(0) },
            suffix: None,
        },
    );

    expect_single_token(
        r###"r#"raw with "quotes""#"###,
        TokenKind::Literal {
            kind: LiteralKind::RawStr { n_hashes: Some(1) },
            suffix: None,
        },
    );

    expect_single_token(
        r####"r##"even more ##"## raw"##"####,
        TokenKind::Literal {
            kind: LiteralKind::RawStr { n_hashes: Some(2) },
            suffix: None,
        },
    );
}

// =============================================================================
// CHARACTER LITERAL TESTS
// =============================================================================

#[test]
fn test_char_literals() {
    expect_single_token(
        "'a'",
        TokenKind::Literal {
            kind: LiteralKind::Char { terminated: true },
            suffix: None,
        },
    );

    expect_single_token(
        "'\\n'",
        TokenKind::Literal {
            kind: LiteralKind::Char { terminated: true },
            suffix: None,
        },
    );

    expect_single_token(
        "'\\x41'",
        TokenKind::Literal {
            kind: LiteralKind::Char { terminated: true },
            suffix: None,
        },
    );

    expect_single_token(
        "'\\u{1F600}'",
        TokenKind::Literal {
            kind: LiteralKind::Char { terminated: true },
            suffix: None,
        },
    );
}

#[test]
fn test_empty_char_literal() {
    expect_error("''");
}

// =============================================================================
// BYTE LITERAL TESTS
// =============================================================================

#[test]
fn test_byte_literals() {
    expect_single_token(
        "b'a'",
        TokenKind::Literal {
            kind: LiteralKind::Byte { terminated: true },
            suffix: None,
        },
    );

    expect_single_token(
        "b'\\xFF'",
        TokenKind::Literal {
            kind: LiteralKind::Byte { terminated: true },
            suffix: None,
        },
    );
}

#[test]
fn test_byte_strings() {
    expect_single_token(
        r#"b"hello""#,
        TokenKind::Literal {
            kind: LiteralKind::ByteStr { terminated: true },
            suffix: None,
        },
    );

    expect_single_token(
        r#"b"\xFF\x00""#,
        TokenKind::Literal {
            kind: LiteralKind::ByteStr { terminated: true },
            suffix: None,
        },
    );
}

// =============================================================================
// LIFETIME TESTS
// =============================================================================

#[test]
fn test_lifetimes() {
    expect_single_token("'a", TokenKind::Lifetime);
    expect_single_token("'static", TokenKind::Lifetime);
    expect_single_token("'lifetime", TokenKind::Lifetime);
    expect_single_token("'_", TokenKind::Lifetime);
}

// =============================================================================
// KEYWORD TESTS
// =============================================================================

#[test]
fn test_declaration_keywords() {
    expect_single_token("fn", TokenKind::Keyword(Keyword::Fn));
    expect_single_token("struct", TokenKind::Keyword(Keyword::Struct));
    expect_single_token("enum", TokenKind::Keyword(Keyword::Enum));
    expect_single_token("trait", TokenKind::Keyword(Keyword::Trait));
    expect_single_token("impl", TokenKind::Keyword(Keyword::Impl));
    expect_single_token("type", TokenKind::Keyword(Keyword::Type));
    expect_single_token("const", TokenKind::Keyword(Keyword::Const));
    expect_single_token("static", TokenKind::Keyword(Keyword::Static));
    expect_single_token("let", TokenKind::Keyword(Keyword::Let));
    expect_single_token("mut", TokenKind::Keyword(Keyword::Mut));
    expect_single_token("pub", TokenKind::Keyword(Keyword::Pub));
    expect_single_token("mod", TokenKind::Keyword(Keyword::Mod));
    expect_single_token("use", TokenKind::Keyword(Keyword::Use));
}

#[test]
fn test_control_flow_keywords() {
    expect_single_token("if", TokenKind::Keyword(Keyword::If));
    expect_single_token("else", TokenKind::Keyword(Keyword::Else));
    expect_single_token("match", TokenKind::Keyword(Keyword::Match));
    expect_single_token("loop", TokenKind::Keyword(Keyword::Loop));
    expect_single_token("while", TokenKind::Keyword(Keyword::While));
    expect_single_token("for", TokenKind::Keyword(Keyword::For));
    expect_single_token("in", TokenKind::Keyword(Keyword::In));
    expect_single_token("break", TokenKind::Keyword(Keyword::Break));
    expect_single_token("continue", TokenKind::Keyword(Keyword::Continue));
    expect_single_token("return", TokenKind::Keyword(Keyword::Return));
}

#[test]
fn test_boolean_keywords() {
    expect_single_token(
        "true",
        TokenKind::Literal {
            kind: LiteralKind::Bool(true),
            suffix: None,
        },
    );
    expect_single_token(
        "false",
        TokenKind::Literal {
            kind: LiteralKind::Bool(false),
            suffix: None,
        },
    );
}

#[test]
fn test_quanta_specific_keywords() {
    expect_single_token("ai", TokenKind::Keyword(Keyword::AI));
    expect_single_token("neural", TokenKind::Keyword(Keyword::Neural));
    expect_single_token("infer", TokenKind::Keyword(Keyword::Infer));
    expect_single_token("effect", TokenKind::Keyword(Keyword::Effect));
    expect_single_token("handle", TokenKind::Keyword(Keyword::Handle));
    expect_single_token("with", TokenKind::Keyword(Keyword::With));
}

// =============================================================================
// IDENTIFIER TESTS
// =============================================================================

#[test]
fn test_identifiers() {
    expect_single_token("foo", TokenKind::Ident);
    expect_single_token("bar123", TokenKind::Ident);
    expect_single_token("_underscore", TokenKind::Ident);
    expect_single_token("__double", TokenKind::Ident);
    expect_single_token("CamelCase", TokenKind::Ident);
    expect_single_token("snake_case", TokenKind::Ident);
    expect_single_token("SCREAMING_SNAKE", TokenKind::Ident);
}

#[test]
fn test_unicode_identifiers() {
    expect_single_token("", TokenKind::Ident);
    expect_single_token("cafe", TokenKind::Ident);
    expect_single_token("", TokenKind::Ident);
    expect_single_token("", TokenKind::Ident);
    expect_single_token("nombre", TokenKind::Ident);
}

#[test]
fn test_raw_identifiers() {
    expect_single_token("r#fn", TokenKind::RawIdent);
    expect_single_token("r#type", TokenKind::RawIdent);
    expect_single_token("r#match", TokenKind::RawIdent);
}

// =============================================================================
// COMMENT TESTS
// =============================================================================

#[test]
fn test_line_comments_skipped() {
    let tokens = lex("// comment\nlet");
    assert_eq!(tokens.len(), 2); // let, EOF
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(Keyword::Let)));
}

#[test]
fn test_block_comments_skipped() {
    let tokens = lex("/* comment */ let");
    assert_eq!(tokens.len(), 2); // let, EOF
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(Keyword::Let)));
}

#[test]
fn test_nested_block_comments() {
    let tokens = lex("/* outer /* inner */ still outer */ let");
    assert_eq!(tokens.len(), 2); // let, EOF
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(Keyword::Let)));
}

#[test]
fn test_unterminated_block_comment() {
    expect_error("/* unterminated");
}

// =============================================================================
// DSL BLOCK TESTS
// =============================================================================

#[test]
fn test_sql_dsl() {
    expect_single_token(
        "sql! { SELECT * FROM users }",
        TokenKind::DslBlock { name: "sql".into() },
    );
}

#[test]
fn test_regex_dsl() {
    expect_single_token(
        "regex! { [a-z]+ }",
        TokenKind::DslBlock { name: "regex".into() },
    );
}

#[test]
fn test_json_dsl() {
    expect_single_token(
        r#"json! { "key": "value" }"#,
        TokenKind::DslBlock { name: "json".into() },
    );
}

#[test]
fn test_nested_dsl_delimiters() {
    expect_single_token(
        "sql! { SELECT { nested } FROM users }",
        TokenKind::DslBlock { name: "sql".into() },
    );
}

// =============================================================================
// WHITESPACE HANDLING TESTS
// =============================================================================

#[test]
fn test_whitespace_ignored() {
    let tokens = lex("  let   x   =   42  ");
    assert_eq!(tokens.len(), 5); // let, x, =, 42, EOF
}

#[test]
fn test_newlines_ignored() {
    let tokens = lex("let\nx\n=\n42");
    assert_eq!(tokens.len(), 5); // let, x, =, 42, EOF
}

#[test]
fn test_tabs_ignored() {
    let tokens = lex("let\t\tx\t=\t42");
    assert_eq!(tokens.len(), 5); // let, x, =, 42, EOF
}

// =============================================================================
// FULL PROGRAM TESTS
// =============================================================================

#[test]
fn test_simple_function() {
    let tokens = lex("fn main() { let x = 42; }");
    let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();

    assert!(matches!(kinds[0], TokenKind::Keyword(Keyword::Fn)));
    assert!(matches!(kinds[1], TokenKind::Ident));
    assert!(matches!(kinds[2], TokenKind::OpenDelim(Delimiter::Paren)));
    assert!(matches!(kinds[3], TokenKind::CloseDelim(Delimiter::Paren)));
    assert!(matches!(kinds[4], TokenKind::OpenDelim(Delimiter::Brace)));
    assert!(matches!(kinds[5], TokenKind::Keyword(Keyword::Let)));
    assert!(matches!(kinds[6], TokenKind::Ident));
    assert!(matches!(kinds[7], TokenKind::Eq));
    assert!(matches!(kinds[8], TokenKind::Literal { .. }));
    assert!(matches!(kinds[9], TokenKind::Semi));
    assert!(matches!(kinds[10], TokenKind::CloseDelim(Delimiter::Brace)));
    assert!(matches!(kinds[11], TokenKind::Eof));
}

#[test]
fn test_struct_definition() {
    let tokens = lex("pub struct Point { x: f32, y: f32 }");
    assert!(tokens.len() > 10);
}

#[test]
fn test_generic_function() {
    let tokens = lex("fn swap<T>(a: &mut T, b: &mut T) { }");
    assert!(tokens.len() > 10);
}

#[test]
fn test_closure() {
    let tokens = lex("|x, y| x + y");
    assert!(matches!(tokens[0].kind, TokenKind::Or));
    assert!(matches!(tokens[1].kind, TokenKind::Ident));
}

#[test]
fn test_match_expression() {
    let tokens = lex("match x { Some(v) => v, None => 0 }");
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(Keyword::Match)));
}

// =============================================================================
// SPAN TESTS
// =============================================================================

#[test]
fn test_span_accuracy() {
    let source = "let x = 42";
    let file = SourceFile::anonymous(source);
    let mut lexer = Lexer::new(&file);
    let tokens = lexer.tokenize().unwrap();

    // "let" should span bytes 0-3
    assert_eq!(tokens[0].span.start.0, 0);
    assert_eq!(tokens[0].span.end.0, 3);

    // "x" should span bytes 4-5
    assert_eq!(tokens[1].span.start.0, 4);
    assert_eq!(tokens[1].span.end.0, 5);
}

#[test]
fn test_unicode_span() {
    let source = "let  = 42"; // 3-byte character
    let file = SourceFile::anonymous(source);
    let mut lexer = Lexer::new(&file);
    let tokens = lexer.tokenize().unwrap();

    // "" should span 3 bytes
    assert_eq!(tokens[1].span.len(), 3);
}

// =============================================================================
// ERROR RECOVERY TESTS
// =============================================================================

#[test]
fn test_recover_after_bad_char() {
    // The lexer should be able to recover and continue after an error
    let file = SourceFile::anonymous("let ` x = 42");
    let mut lexer = Lexer::new(&file);
    let result = lexer.tokenize();

    // Should error on the backtick
    assert!(result.is_err());

    // But errors collection should have info
    assert!(!lexer.errors().is_empty());
}
