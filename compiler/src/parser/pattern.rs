// ===============================================================================
// QUANTALANG PARSER - PATTERN PARSING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Pattern parsing.
//!
//! This module handles parsing of all pattern expressions in QuantaLang,
//! used in match arms, let bindings, function parameters, etc.

use crate::ast::*;
use crate::lexer::{Delimiter, Keyword, TokenKind, LiteralKind, IntBase};
use super::{Parser, ParseResult, ParseError, ParseErrorKind};

impl<'a> Parser<'a> {
    /// Parse a pattern.
    pub fn parse_pattern(&mut self) -> ParseResult<Pattern> {
        self.parse_pattern_with_or()
    }

    /// Parse pattern with or alternatives.
    fn parse_pattern_with_or(&mut self) -> ParseResult<Pattern> {
        let start = self.current_span();
        let mut patterns = vec![self.parse_pattern_primary()?];

        while self.eat(&TokenKind::Or) {
            patterns.push(self.parse_pattern_primary()?);
        }

        if patterns.len() == 1 {
            Ok(patterns.pop().unwrap())
        } else {
            let span = start.merge(&patterns.last().unwrap().span);
            Ok(Pattern::new(PatternKind::Or(patterns), span))
        }
    }

    /// Parse a primary pattern (without or-alternatives).
    /// Public for use by closure parameter parsing where `|` is a delimiter.
    pub fn parse_pattern_primary(&mut self) -> ParseResult<Pattern> {
        let start = self.current_span();

        match self.current_kind().clone() {
            // =================================================================
            // WILDCARD: _
            // =================================================================
            TokenKind::Underscore => {
                self.advance();
                Ok(Pattern::new(PatternKind::Wildcard, start))
            }

            // =================================================================
            // REST: ..
            // =================================================================
            TokenKind::DotDot => {
                self.advance();
                Ok(Pattern::new(PatternKind::Rest, start))
            }

            // =================================================================
            // REFERENCE PATTERN: &pat, &mut pat
            // =================================================================
            TokenKind::And => {
                self.advance();
                let mutability = if self.eat_keyword(Keyword::Mut) {
                    Mutability::Mutable
                } else {
                    Mutability::Immutable
                };
                let inner = self.parse_pattern()?;
                let span = start.merge(&inner.span);
                Ok(Pattern::new(
                    PatternKind::Ref {
                        mutability,
                        pattern: Box::new(inner),
                    },
                    span,
                ))
            }

            TokenKind::AndAnd => {
                // &&pat is &&pat (double reference)
                self.advance();
                let inner = self.parse_pattern()?;
                let inner_span = start.merge(&inner.span);
                let inner_ref = Pattern::new(
                    PatternKind::Ref {
                        mutability: Mutability::Immutable,
                        pattern: Box::new(inner),
                    },
                    inner_span,
                );
                Ok(Pattern::new(
                    PatternKind::Ref {
                        mutability: Mutability::Immutable,
                        pattern: Box::new(inner_ref),
                    },
                    inner_span,
                ))
            }

            // =================================================================
            // BOX PATTERN: box pat
            // =================================================================
            TokenKind::Keyword(Keyword::Box) => {
                self.advance();
                let inner = self.parse_pattern()?;
                let span = start.merge(&inner.span);
                Ok(Pattern::new(PatternKind::Box(Box::new(inner)), span))
            }

            // =================================================================
            // LITERAL PATTERNS (including negative numbers)
            // =================================================================
            TokenKind::Minus => {
                self.advance();
                // Expect a numeric literal
                if let TokenKind::Literal { kind, suffix } = self.current_kind().clone() {
                    self.advance();
                    let literal = self.convert_negative_literal(&kind, suffix.as_deref())?;
                    let span = start.merge(&self.tokens[self.pos - 1].span);
                    Ok(Pattern::new(PatternKind::Literal(literal), span))
                } else {
                    Err(self.error_expected("numeric literal"))
                }
            }

            TokenKind::Literal { ref kind, ref suffix } => {
                let kind = kind.clone();
                let suffix = suffix.clone();
                self.advance();
                let literal = self.convert_pattern_literal(&kind, suffix.as_deref())?;
                let span = start;

                // Check for range patterns
                if self.check(&TokenKind::DotDot) || self.check(&TokenKind::DotDotEq) {
                    return self.parse_range_pattern(literal, start);
                }

                Ok(Pattern::new(PatternKind::Literal(literal), span))
            }

            // =================================================================
            // TUPLE PATTERN: (pat, pat, ...)
            // =================================================================
            TokenKind::OpenDelim(Delimiter::Paren) => {
                self.parse_tuple_pattern()
            }

            // =================================================================
            // SLICE PATTERN: [pat, pat, ...]
            // =================================================================
            TokenKind::OpenDelim(Delimiter::Bracket) => {
                self.parse_slice_pattern()
            }

            // =================================================================
            // MUT IDENTIFIER: mut x
            // =================================================================
            TokenKind::Keyword(Keyword::Mut) => {
                self.advance();
                let name = self.expect_ident()?;
                let span = start.merge(&name.span);

                // Check for @ subpattern
                let subpattern = if self.eat(&TokenKind::At) {
                    Some(Box::new(self.parse_pattern()?))
                } else {
                    None
                };

                Ok(Pattern::new(
                    PatternKind::Ident {
                        mutability: Mutability::Mutable,
                        name,
                        subpattern,
                    },
                    span,
                ))
            }

            // =================================================================
            // REF IDENTIFIER: ref x, ref mut x
            // =================================================================
            TokenKind::Keyword(Keyword::Ref) => {
                self.advance();
                let mutability = if self.eat_keyword(Keyword::Mut) {
                    Mutability::Mutable
                } else {
                    Mutability::Immutable
                };
                let name = self.expect_ident()?;
                let span = start.merge(&name.span);

                let subpattern = if self.eat(&TokenKind::At) {
                    Some(Box::new(self.parse_pattern()?))
                } else {
                    None
                };

                Ok(Pattern::new(
                    PatternKind::Ident {
                        mutability,
                        name,
                        subpattern,
                    },
                    span,
                ))
            }

            // =================================================================
            // PATH / IDENTIFIER / STRUCT / TUPLE STRUCT PATTERNS
            // =================================================================
            TokenKind::Ident | TokenKind::RawIdent | TokenKind::ColonColon
            | TokenKind::Keyword(Keyword::Crate | Keyword::Super | Keyword::Self_ | Keyword::SelfType)
            | TokenKind::Keyword(Keyword::Default | Keyword::Module) => {
                self.parse_path_pattern()
            }

            // =================================================================
            // ERROR
            // =================================================================
            _ => {
                // Last resort: if it's any keyword, try treating as an identifier pattern
                if matches!(self.current_kind(), TokenKind::Keyword(_)) {
                    self.parse_path_pattern()
                } else {
                    Err(self.error_expected("pattern"))
                }
            }
        }
    }

    /// Parse a path-based pattern (identifier, path, struct, tuple struct).
    fn parse_path_pattern(&mut self) -> ParseResult<Pattern> {
        let start = self.current_span();

        // Try to parse as simple identifier first
        let is_simple_ident = (self.check_ident() || self.is_contextual_keyword())
            && !matches!(self.peek().kind, TokenKind::ColonColon | TokenKind::OpenDelim(_));

        if is_simple_ident && !matches!(self.peek().kind, TokenKind::OpenDelim(_)) {
            let name = self.expect_ident()?;
            let span = name.span;

            // Check for @ subpattern
            let subpattern = if self.eat(&TokenKind::At) {
                Some(Box::new(self.parse_pattern()?))
            } else {
                None
            };

            // Check for range pattern
            if self.check(&TokenKind::DotDot) || self.check(&TokenKind::DotDotEq) {
                let path = Path::from_ident(name);
                return self.parse_range_pattern_from_path(path, start);
            }

            return Ok(Pattern::new(
                PatternKind::Ident {
                    mutability: Mutability::Immutable,
                    name,
                    subpattern,
                },
                span,
            ));
        }

        // Parse as path
        let path = self.parse_path()?;

        // Check what follows the path
        match self.current_kind() {
            // Struct pattern: Path { field: pat, ... }
            TokenKind::OpenDelim(Delimiter::Brace) => {
                self.parse_struct_pattern(path, start)
            }

            // Tuple struct pattern: Path(pat, ...)
            TokenKind::OpenDelim(Delimiter::Paren) => {
                self.parse_tuple_struct_pattern(path, start)
            }

            // Range pattern
            TokenKind::DotDot | TokenKind::DotDotEq => {
                self.parse_range_pattern_from_path(path, start)
            }

            // Plain path pattern (enum variant without fields)
            _ => {
                // If it's a simple path with one segment, treat as identifier
                if path.is_simple() {
                    let name = path.last_ident().unwrap().clone();
                    let subpattern = if self.eat(&TokenKind::At) {
                        Some(Box::new(self.parse_pattern()?))
                    } else {
                        None
                    };
                    Ok(Pattern::new(
                        PatternKind::Ident {
                            mutability: Mutability::Immutable,
                            name,
                            subpattern,
                        },
                        path.span,
                    ))
                } else {
                    Ok(Pattern::new(PatternKind::Path(path.clone()), path.span))
                }
            }
        }
    }

    /// Parse struct pattern: Path { field: pat, field, .. }
    fn parse_struct_pattern(&mut self, path: Path, start: crate::lexer::Span) -> ParseResult<Pattern> {
        self.expect(&TokenKind::OpenDelim(Delimiter::Brace))?;

        let mut fields = Vec::new();
        let mut rest: Option<crate::lexer::Span> = None;

        while !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) && !self.is_eof() {
            // Check for ..
            if self.check(&TokenKind::DotDot) {
                let dot_span = self.advance().span;
                rest = Some(dot_span);
                break;
            }

            let field_start = self.current_span();

            // Check for ref/mut modifiers
            let _by_ref = self.eat_keyword(Keyword::Ref);
            let mutability = if self.eat_keyword(Keyword::Mut) {
                Mutability::Mutable
            } else {
                Mutability::Immutable
            };

            let name = self.expect_ident()?;

            // Check for : pattern
            let (pattern, is_shorthand) = if self.eat(&TokenKind::Colon) {
                (self.parse_pattern()?, false)
            } else {
                // Shorthand: just field name
                (Pattern::new(
                    PatternKind::Ident {
                        mutability,
                        name: name.clone(),
                        subpattern: None,
                    },
                    name.span,
                ), true)
            };

            let field_span = field_start.merge(&pattern.span);
            fields.push(FieldPattern {
                attrs: Vec::new(),
                name,
                pattern,
                is_shorthand,
                span: field_span,
            });

            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        let end = self.expect(&TokenKind::CloseDelim(Delimiter::Brace))?.span;
        let span = start.merge(&end);

        Ok(Pattern::new(PatternKind::Struct { path, fields, rest }, span))
    }

    /// Parse tuple struct pattern: Path(pat, pat, ...)
    fn parse_tuple_struct_pattern(&mut self, path: Path, start: crate::lexer::Span) -> ParseResult<Pattern> {
        let (patterns, paren_span) = self.parse_paren_comma_seq(|p| p.parse_pattern())?;
        let span = start.merge(&paren_span);

        Ok(Pattern::new(PatternKind::TupleStruct { path, patterns }, span))
    }

    /// Parse tuple pattern: (pat, pat, ...)
    fn parse_tuple_pattern(&mut self) -> ParseResult<Pattern> {
        let start = self.expect(&TokenKind::OpenDelim(Delimiter::Paren))?.span;

        // Empty tuple: ()
        if self.check(&TokenKind::CloseDelim(Delimiter::Paren)) {
            let end = self.advance().span;
            return Ok(Pattern::new(PatternKind::Tuple(Vec::new()), start.merge(&end)));
        }

        let mut patterns = Vec::new();

        loop {
            patterns.push(self.parse_pattern()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
            // Allow trailing comma
            if self.check(&TokenKind::CloseDelim(Delimiter::Paren)) {
                break;
            }
        }

        let end = self.expect(&TokenKind::CloseDelim(Delimiter::Paren))?.span;
        let span = start.merge(&end);

        // Single element tuple needs trailing comma, otherwise it's just parenthesized
        Ok(Pattern::new(PatternKind::Tuple(patterns), span))
    }

    /// Parse slice pattern: [pat, pat, ...]
    fn parse_slice_pattern(&mut self) -> ParseResult<Pattern> {
        let start = self.expect(&TokenKind::OpenDelim(Delimiter::Bracket))?.span;

        let mut patterns = Vec::new();

        while !self.check(&TokenKind::CloseDelim(Delimiter::Bracket)) && !self.is_eof() {
            patterns.push(self.parse_pattern()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        let end = self.expect(&TokenKind::CloseDelim(Delimiter::Bracket))?.span;
        let span = start.merge(&end);

        Ok(Pattern::new(PatternKind::Slice(patterns), span))
    }

    /// Parse range pattern from literal: 1..10 or 1..=10
    fn parse_range_pattern(&mut self, start_lit: Literal, start_span: crate::lexer::Span) -> ParseResult<Pattern> {
        use crate::ast::{Expr, ExprKind, NodeId};

        let start_pattern = Pattern::new(PatternKind::Literal(start_lit.clone()), start_span);

        let inclusive = if self.eat(&TokenKind::DotDotEq) {
            true
        } else if self.eat(&TokenKind::DotDot) {
            false
        } else {
            return Ok(start_pattern);
        };

        // Convert literal to expression for range start
        let start_expr = Expr {
            kind: ExprKind::Literal(start_lit),
            span: start_span,
            id: NodeId::DUMMY,
            attrs: Vec::new(),
        };

        // Parse end and convert to expression
        let end_expr = if self.can_begin_pattern() {
            let end_pat = self.parse_pattern_primary()?;
            // Convert pattern to expression (only works for literals and paths)
            Some(Box::new(self.pattern_to_expr(&end_pat)?))
        } else {
            None
        };

        let span = if let Some(ref end) = end_expr {
            start_span.merge(&end.span)
        } else {
            start_span
        };

        Ok(Pattern::new(
            PatternKind::Range {
                start: Some(Box::new(start_expr)),
                end: end_expr,
                inclusive,
            },
            span,
        ))
    }

    /// Parse range pattern from path.
    fn parse_range_pattern_from_path(&mut self, path: Path, start_span: crate::lexer::Span) -> ParseResult<Pattern> {
        use crate::ast::{Expr, ExprKind, NodeId};

        let start_pattern = Pattern::new(PatternKind::Path(path.clone()), start_span);

        let inclusive = if self.eat(&TokenKind::DotDotEq) {
            true
        } else if self.eat(&TokenKind::DotDot) {
            false
        } else {
            return Ok(start_pattern);
        };

        // Convert path to expression for range start
        let start_expr = Expr {
            kind: ExprKind::Path(path),
            span: start_span,
            id: NodeId::DUMMY,
            attrs: Vec::new(),
        };

        let end_expr = if self.can_begin_pattern() {
            let end_pat = self.parse_pattern_primary()?;
            Some(Box::new(self.pattern_to_expr(&end_pat)?))
        } else {
            None
        };

        let span = if let Some(ref end) = end_expr {
            start_span.merge(&end.span)
        } else {
            start_span
        };

        Ok(Pattern::new(
            PatternKind::Range {
                start: Some(Box::new(start_expr)),
                end: end_expr,
                inclusive,
            },
            span,
        ))
    }

    /// Check if current token can begin a pattern.
    fn can_begin_pattern(&self) -> bool {
        matches!(
            self.current_kind(),
            TokenKind::Ident
                | TokenKind::RawIdent
                | TokenKind::Literal { .. }
                | TokenKind::Minus
                | TokenKind::Underscore
                | TokenKind::OpenDelim(_)
                | TokenKind::And
                | TokenKind::AndAnd
                | TokenKind::DotDot
                | TokenKind::ColonColon
                | TokenKind::Keyword(
                    Keyword::Mut
                        | Keyword::Ref
                        | Keyword::Box
                        | Keyword::Const
                        | Keyword::Crate
                        | Keyword::Super
                        | Keyword::Self_
                        | Keyword::SelfType
                )
        )
    }

    /// Convert a literal token to a pattern literal.
    fn convert_pattern_literal(
        &self,
        kind: &LiteralKind,
        suffix: Option<&str>,
    ) -> ParseResult<Literal> {
        // Reuse the expression literal conversion
        self.convert_literal(kind, suffix)
    }

    /// Convert a negative literal.
    fn convert_negative_literal(
        &self,
        kind: &LiteralKind,
        suffix: Option<&str>,
    ) -> ParseResult<Literal> {
        match kind {
            LiteralKind::Int { base, .. } => {
                let span = self.tokens[self.pos - 1].span;
                let text = self.source.slice(span);
                let text = match base {
                    IntBase::Decimal => text.replace('_', ""),
                    IntBase::Hexadecimal => text[2..].replace('_', ""),
                    IntBase::Octal => text[2..].replace('_', ""),
                    IntBase::Binary => text[2..].replace('_', ""),
                };
                let text = suffix.map_or(text.as_str(), |s| &text[..text.len() - s.len()]).to_string();
                let value = u128::from_str_radix(&text, base.radix()).unwrap_or(0);
                // Store as negative value (wrapping)
                let int_suffix = suffix.and_then(IntSuffix::from_str);
                Ok(Literal::Int {
                    value: (-(value as i128)) as u128,
                    suffix: int_suffix,
                    base: *base,
                })
            }
            LiteralKind::Float { .. } => {
                let span = self.tokens[self.pos - 1].span;
                let text = self.source.slice(span).replace('_', "");
                let text = suffix.map_or(text.as_str(), |s| &text[..text.len() - s.len()]);
                let value: f64 = text.parse().unwrap_or(0.0);
                let float_suffix = suffix.and_then(FloatSuffix::from_str);
                Ok(Literal::Float {
                    value: -value,
                    suffix: float_suffix,
                })
            }
            _ => Err(ParseError::new(
                ParseErrorKind::InvalidPattern,
                self.current_span(),
            )),
        }
    }

    /// Convert a pattern to an expression (for range patterns).
    /// Only works for literal and path patterns.
    fn pattern_to_expr(&self, pattern: &Pattern) -> ParseResult<crate::ast::Expr> {
        use crate::ast::{Expr, ExprKind, NodeId};

        match &pattern.kind {
            PatternKind::Literal(lit) => Ok(Expr {
                kind: ExprKind::Literal(lit.clone()),
                span: pattern.span,
                id: NodeId::DUMMY,
                attrs: Vec::new(),
            }),
            PatternKind::Path(path) => Ok(Expr {
                kind: ExprKind::Path(path.clone()),
                span: pattern.span,
                id: NodeId::DUMMY,
                attrs: Vec::new(),
            }),
            _ => Err(ParseError::new(
                ParseErrorKind::InvalidPattern,
                pattern.span,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{Lexer, SourceFile as LexerSourceFile};

    /// Parse a pattern by wrapping it in `fn test() { let PATTERN = x; }`.
    fn parse_pattern_str(s: &str) -> ParseResult<Pattern> {
        let source = LexerSourceFile::new(
            "test.quanta",
            format!("fn test() {{ let {} = x; }}", s),
        );
        let mut lexer = Lexer::new(&source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(&source, tokens);
        // Skip: fn(0) test(1) ((2) )(3) {(4) let(5) → now at pattern
        for _ in 0..6 {
            parser.advance();
        }
        parser.parse_pattern()
    }

    #[test]
    fn wildcard_pattern() {
        // The lexer produces Ident for `_` (not TokenKind::Underscore),
        // so the pattern parser treats it as an identifier binding.
        let pat = parse_pattern_str("_").unwrap();
        match &pat.kind {
            PatternKind::Ident { name, .. } => assert_eq!(name.as_str(), "_"),
            PatternKind::Wildcard => {} // accepted if lexer changes
            other => panic!("expected Ident(_) or Wildcard, got {:?}", other),
        }
    }

    #[test]
    fn binding_pattern() {
        let pat = parse_pattern_str("x").unwrap();
        match &pat.kind {
            PatternKind::Ident { name, mutability, .. } => {
                assert_eq!(name.as_str(), "x");
                assert_eq!(*mutability, Mutability::Immutable);
            }
            other => panic!("expected Ident pattern, got {:?}", other),
        }
    }

    #[test]
    fn tuple_destructure() {
        let pat = parse_pattern_str("(a, b, c)").unwrap();
        match &pat.kind {
            PatternKind::Tuple(patterns) => {
                assert_eq!(patterns.len(), 3);
                assert!(matches!(&patterns[0].kind, PatternKind::Ident { name, .. } if name.as_str() == "a"));
                assert!(matches!(&patterns[1].kind, PatternKind::Ident { name, .. } if name.as_str() == "b"));
                assert!(matches!(&patterns[2].kind, PatternKind::Ident { name, .. } if name.as_str() == "c"));
            }
            other => panic!("expected Tuple pattern, got {:?}", other),
        }
    }

    #[test]
    fn struct_destructure() {
        let pat = parse_pattern_str("Point { x, y }").unwrap();
        match &pat.kind {
            PatternKind::Struct { path, fields, rest } => {
                assert_eq!(path.segments.last().unwrap().ident.as_str(), "Point");
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name.as_str(), "x");
                assert!(fields[0].is_shorthand);
                assert_eq!(fields[1].name.as_str(), "y");
                assert!(rest.is_none());
            }
            other => panic!("expected Struct pattern, got {:?}", other),
        }
    }

    #[test]
    fn enum_pattern() {
        let pat = parse_pattern_str("Some(value)").unwrap();
        match &pat.kind {
            PatternKind::TupleStruct { path, patterns } => {
                assert_eq!(path.segments.last().unwrap().ident.as_str(), "Some");
                assert_eq!(patterns.len(), 1);
                assert!(matches!(&patterns[0].kind, PatternKind::Ident { name, .. } if name.as_str() == "value"));
            }
            other => panic!("expected TupleStruct pattern, got {:?}", other),
        }
    }

    #[test]
    fn nested_pattern() {
        let pat = parse_pattern_str("Some((a, b))").unwrap();
        match &pat.kind {
            PatternKind::TupleStruct { path, patterns } => {
                assert_eq!(path.segments.last().unwrap().ident.as_str(), "Some");
                assert_eq!(patterns.len(), 1);
                assert!(matches!(&patterns[0].kind, PatternKind::Tuple(_)));
            }
            other => panic!("expected TupleStruct(Some, [Tuple]), got {:?}", other),
        }
    }

    #[test]
    fn literal_pattern() {
        let pat = parse_pattern_str("42").unwrap();
        match &pat.kind {
            PatternKind::Literal(Literal::Int { value: 42, .. }) => {}
            other => panic!("expected Literal(42), got {:?}", other),
        }
    }

    #[test]
    fn or_pattern() {
        let pat = parse_pattern_str("A | B").unwrap();
        match &pat.kind {
            PatternKind::Or(alternatives) => {
                assert_eq!(alternatives.len(), 2);
            }
            other => panic!("expected Or pattern, got {:?}", other),
        }
    }

    #[test]
    fn mut_binding() {
        let pat = parse_pattern_str("mut x").unwrap();
        match &pat.kind {
            PatternKind::Ident { name, mutability, .. } => {
                assert_eq!(name.as_str(), "x");
                assert_eq!(*mutability, Mutability::Mutable);
            }
            other => panic!("expected mut Ident, got {:?}", other),
        }
    }

    #[test]
    fn slice_pattern() {
        let pat = parse_pattern_str("[a, b, c]").unwrap();
        match &pat.kind {
            PatternKind::Slice(patterns) => assert_eq!(patterns.len(), 3),
            other => panic!("expected Slice pattern, got {:?}", other),
        }
    }
}
