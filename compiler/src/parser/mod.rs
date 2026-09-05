// ===============================================================================
// BUILDLANG PARSER MODULE
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! # Parser
//!
//! This module implements a recursive descent parser with Pratt parsing for
//! expressions. It converts a stream of tokens into an Abstract Syntax Tree.
//!
//! ## Architecture
//!
//! - Recursive descent for statements, items, types, and patterns
//! - Pratt parsing (operator precedence climbing) for expressions
//! - Error recovery to continue parsing after errors
//! - Comprehensive span tracking for error messages
//!
//! ## Example
//!
//! ```rust,ignore
//! use buildlang::parser::{Parser, parse};
//! use buildlang::lexer::{Lexer, SourceFile};
//!
//! let source = SourceFile::new("example.bld", "fn main() { let x = 42; }");
//! let mut lexer = Lexer::new(&source);
//! let tokens = lexer.tokenize()?;
//!
//! let mut parser = Parser::new(&source, tokens);
//! let ast = parser.parse()?;
//! ```

mod error;
mod expr;
mod item;
mod pattern;
mod stmt;
mod ty;

pub use crate::ast::Module;
pub use error::{ParseError, ParseErrorKind, ParseResult};

use crate::ast::*;
use crate::lexer::{Delimiter, Keyword, SourceFile, Span, Token, TokenKind};

/// The parser state.
pub struct Parser<'a> {
    /// The source file being parsed.
    source: &'a SourceFile,
    /// The token stream.
    tokens: Vec<Token>,
    /// Current position in the token stream.
    pos: usize,
    /// Accumulated errors.
    errors: Vec<ParseError>,
    /// Restriction flags for expression parsing.
    restrictions: Restrictions,
}

/// Restrictions on expression parsing.
#[derive(Debug, Clone, Copy, Default)]
struct Restrictions {
    /// Don't parse struct literals (in ambiguous contexts).
    no_struct_literal: bool,
}

impl<'a> Parser<'a> {
    /// Create a new parser.
    pub fn new(source: &'a SourceFile, tokens: Vec<Token>) -> Self {
        Self {
            source,
            tokens,
            pos: 0,
            errors: Vec::new(),
            restrictions: Restrictions::default(),
        }
    }

    /// Parse the entire source file.
    pub fn parse(&mut self) -> ParseResult<Module> {
        self.parse_module()
    }

    /// Parse a source file (module).
    fn parse_module(&mut self) -> ParseResult<Module> {
        let start = self.current_span();

        // Parse inner attributes
        let attrs = self.parse_inner_attrs()?;

        // Parse items
        let mut items = Vec::new();
        while !self.is_eof() {
            match self.parse_item() {
                Ok(item) => items.push(item),
                Err(e) => {
                    self.errors.push(e);
                    self.recover_to_item();
                }
            }
        }

        let span = start.merge(&self.current_span());

        // Always return successfully parsed items, even when there are errors.
        // This allows the type checker to process valid items from files that
        // contain some unsupported syntax - critical for ecosystem compilation.
        if !self.errors.is_empty() {
            // Still report the first error for the caller to print
            // but DON'T discard parsed items
        }
        {
            Ok(Module::new(attrs, items, span))
        }
    }

    /// Get accumulated errors.
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    /// Take accumulated errors.
    pub fn take_errors(&mut self) -> Vec<ParseError> {
        std::mem::take(&mut self.errors)
    }

    // =========================================================================
    // TOKEN ACCESS
    // =========================================================================

    /// Get the current token.
    fn current(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| self.tokens.last().expect("tokens should not be empty"))
    }

    /// Get the current token kind.
    fn current_kind(&self) -> &TokenKind {
        &self.current().kind
    }

    /// Get the current span.
    fn current_span(&self) -> Span {
        self.current().span
    }

    /// Peek at the next token.
    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos + 1)
            .unwrap_or_else(|| self.tokens.last().expect("tokens should not be empty"))
    }

    /// Peek at the nth next token.
    fn peek_n(&self, n: usize) -> &Token {
        self.tokens
            .get(self.pos + n)
            .unwrap_or_else(|| self.tokens.last().expect("tokens should not be empty"))
    }

    /// Check if at end of file.
    fn is_eof(&self) -> bool {
        matches!(self.current_kind(), TokenKind::Eof)
    }

    /// Check if the current token matches a kind.
    fn check(&self, kind: &TokenKind) -> bool {
        self.current_kind() == kind
    }

    /// Check if the current token is a keyword.
    fn check_keyword(&self, kw: Keyword) -> bool {
        matches!(self.current_kind(), TokenKind::Keyword(k) if *k == kw)
    }

    /// Check if the current token is an identifier.
    fn check_ident(&self) -> bool {
        matches!(self.current_kind(), TokenKind::Ident | TokenKind::RawIdent)
    }

    /// Check if the current token is a lifetime.
    fn check_lifetime(&self) -> bool {
        matches!(self.current_kind(), TokenKind::Lifetime)
    }

    /// Advance to the next token.
    fn advance(&mut self) -> &Token {
        if !self.is_eof() {
            self.pos += 1;
        }
        self.tokens.get(self.pos - 1).unwrap()
    }

    /// Consume the current token if it matches.
    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Consume a keyword.
    fn eat_keyword(&mut self, kw: Keyword) -> bool {
        if self.check_keyword(kw) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Expect and consume a token, or error.
    fn expect(&mut self, kind: &TokenKind) -> ParseResult<&Token> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(self.error_expected(format!("{}", kind)))
        }
    }

    /// Expect and consume a keyword, or error.
    fn expect_keyword(&mut self, kw: Keyword) -> ParseResult<Span> {
        if self.check_keyword(kw) {
            Ok(self.advance().span)
        } else {
            Err(self.error_expected(format!("`{}`", kw)))
        }
    }

    /// Expect and consume an identifier.
    ///
    /// Also accepts contextual keywords that can be used as identifiers
    /// (e.g., `default`, `module`) when they appear in identifier position.
    fn expect_ident(&mut self) -> ParseResult<Ident> {
        if self.check_ident() {
            let is_raw = matches!(self.current_kind(), TokenKind::RawIdent);
            let token_span = self.advance().span;
            let name = self.source.slice(token_span);
            // Strip r# prefix for raw identifiers
            let name = if is_raw { &name[2..] } else { name };
            Ok(Ident::new(name, token_span))
        } else if self.is_contextual_keyword() {
            // Allow contextual keywords as identifiers
            let token_span = self.advance().span;
            let name = self.source.slice(token_span);
            Ok(Ident::new(name, token_span))
        } else {
            Err(self.error_expected("identifier"))
        }
    }

    /// Check if the current token is a keyword that can be used as an identifier
    /// in certain contexts (struct fields, variable names, type paths, etc.).
    fn is_contextual_keyword(&self) -> bool {
        matches!(
            self.current_kind(),
            TokenKind::Keyword(Keyword::Default)
                | TokenKind::Keyword(Keyword::Module)
                | TokenKind::Keyword(Keyword::SelfType)
                | TokenKind::Keyword(Keyword::Handle)
                | TokenKind::Keyword(Keyword::Effect)
                | TokenKind::Keyword(Keyword::Auto)
        )
    }

    /// Check if the current token is a contextual keyword that can begin an
    /// expression as a plain value (variable, field access, call, path).
    ///
    /// These keywords carry a declaration meaning only in item position
    /// (`default impl`, `auto trait`, `module m { .. }`, `effect E { .. }`),
    /// which the item parser consumes before an expression is ever parsed. When
    /// one of them reaches expression parsing it is being used as an ordinary
    /// identifier, so it routes to the same path/struct-expression parser as any
    /// identifier. `handle`, `resume`, and `perform` are deliberately excluded:
    /// they have their own expression forms.
    fn is_value_context_keyword(&self) -> bool {
        matches!(
            self.current_kind(),
            TokenKind::Keyword(Keyword::Default)
                | TokenKind::Keyword(Keyword::Module)
                | TokenKind::Keyword(Keyword::Effect)
                | TokenKind::Keyword(Keyword::Auto)
        )
    }

    /// If the current token is a path-root keyword (`self`, `Self`, `super`,
    /// `crate`), consume it and return it as an `Ident` carrying the keyword's
    /// spelling. These are legal only as the FIRST segment of a path
    /// (`super::T`, `crate::m::f`, `self::x`, `Self::Assoc`); a later segment
    /// still routes through `expect_ident`. Returns `None` without advancing
    /// when the current token is not a path root, so the caller falls back to
    /// the ordinary identifier rule.
    fn eat_path_root_keyword(&mut self) -> Option<Ident> {
        let name = match self.current_kind() {
            TokenKind::Keyword(Keyword::Self_) => "self",
            TokenKind::Keyword(Keyword::SelfType) => "Self",
            TokenKind::Keyword(Keyword::Super) => "super",
            TokenKind::Keyword(Keyword::Crate) => "crate",
            _ => return None,
        };
        let span = self.advance().span;
        Some(Ident::new(name, span))
    }

    /// Expect a lifetime.
    fn expect_lifetime(&mut self) -> ParseResult<Lifetime> {
        if self.check_lifetime() {
            let token_span = self.advance().span;
            let name_str = self.source.slice(token_span);
            // Strip the leading '
            let name = Ident::new(&name_str[1..], token_span);
            Ok(Lifetime::new(name, token_span))
        } else {
            Err(self.error_expected("lifetime"))
        }
    }

    // =========================================================================
    // DELIMITED SEQUENCES
    // =========================================================================

    /// Parse a delimited sequence.
    fn parse_delimited<T, F>(
        &mut self,
        open: Delimiter,
        close: Delimiter,
        sep: &TokenKind,
        mut parse_elem: F,
    ) -> ParseResult<(Vec<T>, Span)>
    where
        F: FnMut(&mut Self) -> ParseResult<T>,
    {
        let open_span = self.expect(&TokenKind::OpenDelim(open))?.span;

        let mut items = Vec::new();

        while !self.check(&TokenKind::CloseDelim(close)) && !self.is_eof() {
            items.push(parse_elem(self)?);

            if !self.eat(sep) {
                break;
            }
        }

        let close_span = self.expect(&TokenKind::CloseDelim(close))?.span;
        let span = open_span.merge(&close_span);

        Ok((items, span))
    }

    /// Parse a comma-separated list in parentheses.
    fn parse_paren_comma_seq<T, F>(&mut self, parse_elem: F) -> ParseResult<(Vec<T>, Span)>
    where
        F: FnMut(&mut Self) -> ParseResult<T>,
    {
        self.parse_delimited(
            Delimiter::Paren,
            Delimiter::Paren,
            &TokenKind::Comma,
            parse_elem,
        )
    }

    /// Parse a comma-separated list in brackets.
    fn parse_bracket_comma_seq<T, F>(&mut self, parse_elem: F) -> ParseResult<(Vec<T>, Span)>
    where
        F: FnMut(&mut Self) -> ParseResult<T>,
    {
        self.parse_delimited(
            Delimiter::Bracket,
            Delimiter::Bracket,
            &TokenKind::Comma,
            parse_elem,
        )
    }

    /// Parse a comma-separated list in braces.
    fn parse_brace_comma_seq<T, F>(&mut self, parse_elem: F) -> ParseResult<(Vec<T>, Span)>
    where
        F: FnMut(&mut Self) -> ParseResult<T>,
    {
        self.parse_delimited(
            Delimiter::Brace,
            Delimiter::Brace,
            &TokenKind::Comma,
            parse_elem,
        )
    }

    // =========================================================================
    // ATTRIBUTES
    // =========================================================================

    /// Parse outer attributes (`#[...]` or `@[...]` or `@name(...)`).
    fn parse_outer_attrs(&mut self) -> ParseResult<Vec<Attribute>> {
        let mut attrs = Vec::new();
        loop {
            if self.check(&TokenKind::Pound) && !self.is_eof() {
                // Check if it's an outer attribute (not #!)
                if matches!(self.peek().kind, TokenKind::Not) {
                    break;
                }
                attrs.push(self.parse_attribute(false)?);
            } else if self.check(&TokenKind::At) && !self.is_eof() {
                // BuildLang ecosystem `@` attribute syntax
                attrs.push(self.parse_at_attribute()?);
            } else {
                break;
            }
        }
        Ok(attrs)
    }

    /// Parse a `@` prefixed attribute (BuildLang ecosystem convention).
    ///
    /// Supports:
    /// - `@derive(Clone, Debug)` → equivalent to `#[derive(Clone, Debug)]`
    /// - `@[repr(u8)]` → equivalent to `#[repr(u8)]`
    /// - `@group(0) @binding(1)` → shader annotations
    fn parse_at_attribute(&mut self) -> ParseResult<Attribute> {
        let start = self.expect(&TokenKind::At)?.span;

        // @[...] form - same as #[...]
        if self.check(&TokenKind::OpenDelim(Delimiter::Bracket)) {
            self.advance(); // consume [
            let path = self.parse_path()?;

            let args = if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
                let tokens = self.parse_token_trees_until(Delimiter::Paren)?;
                AttrArgs::Delimited(tokens)
            } else {
                AttrArgs::Empty
            };

            let end = self
                .expect(&TokenKind::CloseDelim(Delimiter::Bracket))?
                .span;
            let span = start.merge(&end);

            return Ok(Attribute {
                is_inner: false,
                path,
                args,
                span,
            });
        }

        // @name or @name(args...) form
        let path = self.parse_path()?;

        let args = if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
            let tokens = self.parse_token_trees_until(Delimiter::Paren)?;
            AttrArgs::Delimited(tokens)
        } else {
            AttrArgs::Empty
        };

        let end_span = self.tokens[self.pos.saturating_sub(1)].span;
        let span = start.merge(&end_span);

        // Consume optional trailing ] if present (from @derive(...)] pattern)
        self.eat(&TokenKind::CloseDelim(Delimiter::Bracket));

        Ok(Attribute {
            is_inner: false,
            path,
            args,
            span,
        })
    }

    /// Parse inner attributes.
    fn parse_inner_attrs(&mut self) -> ParseResult<Vec<Attribute>> {
        let mut attrs = Vec::new();
        while self.check(&TokenKind::Pound) {
            if !matches!(self.peek().kind, TokenKind::Not) {
                break;
            }
            attrs.push(self.parse_attribute(true)?);
        }
        Ok(attrs)
    }

    /// Parse a single attribute.
    fn parse_attribute(&mut self, is_inner: bool) -> ParseResult<Attribute> {
        let start = self.expect(&TokenKind::Pound)?.span;

        if is_inner {
            self.expect(&TokenKind::Not)?;
        }

        self.expect(&TokenKind::OpenDelim(Delimiter::Bracket))?;

        let path = self.parse_path()?;

        let args = if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
            let tokens = self.parse_token_trees_until(Delimiter::Paren)?;
            AttrArgs::Delimited(tokens)
        } else if self.eat(&TokenKind::Eq) {
            let expr = self.parse_expr()?;
            AttrArgs::Eq(Box::new(expr))
        } else {
            AttrArgs::Empty
        };

        let end = self
            .expect(&TokenKind::CloseDelim(Delimiter::Bracket))?
            .span;
        let span = start.merge(&end);

        Ok(Attribute {
            path,
            args,
            is_inner,
            span,
        })
    }

    /// Parse token trees until a closing delimiter.
    fn parse_token_trees_until(&mut self, close: Delimiter) -> ParseResult<Vec<TokenTree>> {
        self.expect(&TokenKind::OpenDelim(close))?;

        let mut trees = Vec::new();
        let mut depth = 1;

        while depth > 0 && !self.is_eof() {
            let token = self.current().clone();

            match &token.kind {
                TokenKind::OpenDelim(d) => {
                    if *d == close {
                        depth += 1;
                    }
                    self.advance();
                    trees.push(TokenTree::Token(token));
                }
                TokenKind::CloseDelim(d) => {
                    if *d == close {
                        depth -= 1;
                        if depth == 0 {
                            self.advance();
                            break;
                        }
                    }
                    self.advance();
                    trees.push(TokenTree::Token(token));
                }
                _ => {
                    self.advance();
                    trees.push(TokenTree::Token(token));
                }
            }
        }

        Ok(trees)
    }

    // =========================================================================
    // VISIBILITY
    // =========================================================================

    /// Parse visibility specifier.
    fn parse_visibility(&mut self) -> ParseResult<Visibility> {
        if !self.check_keyword(Keyword::Pub) {
            return Ok(Visibility::Private);
        }

        let start = self.advance().span;

        if !self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
            return Ok(Visibility::Public(start));
        }

        self.advance(); // (

        let vis = if self.check_keyword(Keyword::Crate) {
            self.advance();
            Visibility::Crate(start)
        } else if self.check_keyword(Keyword::Super) {
            self.advance();
            Visibility::Super(start)
        } else if self.check_keyword(Keyword::Self_) {
            self.advance();
            Visibility::Private
        } else if self.eat_keyword(Keyword::In) {
            let path = self.parse_path()?;
            let end = self.expect(&TokenKind::CloseDelim(Delimiter::Paren))?.span;
            return Ok(Visibility::Restricted {
                path,
                span: start.merge(&end),
            });
        } else {
            return Err(self.error_expected("visibility restriction"));
        };

        self.expect(&TokenKind::CloseDelim(Delimiter::Paren))?;
        Ok(vis)
    }

    // =========================================================================
    // PATHS
    // =========================================================================

    /// Parse a path.
    fn parse_path(&mut self) -> ParseResult<Path> {
        self.parse_path_inner(false)
    }

    /// Parse a path in an expression context, where bare `<` should NOT be
    /// treated as opening a generic argument list (since it is ambiguous with
    /// the less-than comparison operator).  Turbofish `::< >` is still allowed.
    pub(crate) fn parse_path_in_expr(&mut self) -> ParseResult<Path> {
        self.parse_path_inner(true)
    }

    fn parse_path_inner(&mut self, expr_context: bool) -> ParseResult<Path> {
        let start = self.current_span();
        let mut segments = Vec::new();

        // Handle leading ::
        if self.eat(&TokenKind::ColonColon) {
            // Global path
        }

        loop {
            // A path-root keyword (`self`, `Self`, `super`, `crate`) is legal
            // only as the first segment; every later segment is an ordinary
            // identifier. The expression parser has its own primary-expression
            // arms for these roots, so this path is reached from type, pattern,
            // and use-tree contexts, where `super::T` / `crate::m::f` had been
            // rejected by `expect_ident`.
            let ident = if segments.is_empty() {
                match self.eat_path_root_keyword() {
                    Some(root) => root,
                    None => self.expect_ident()?,
                }
            } else {
                self.expect_ident()?
            };
            // In type context a bare `<` opens a generic argument list. In
            // expression context a bare `<` is ambiguous with the less-than
            // operator, so generics there come only from a turbofish `::<...>`
            // (handled just below).
            let mut generics = if !expr_context && self.check(&TokenKind::Lt) {
                self.parse_generic_args()?
            } else {
                Vec::new()
            };

            // Turbofish `::<...>` binds its generics to the segment just parsed,
            // e.g. `size_of::<i32>()` or `Vec::<u8>::new()`. Consume the `::<`
            // here so the path loop does not then demand an identifier after the
            // `::`, which was the cause of `expected identifier, found `<``.
            if expr_context
                && self.check(&TokenKind::ColonColon)
                && self.peek().kind == TokenKind::Lt
            {
                self.advance(); // consume `::`
                generics = self.parse_generic_args()?;
            }

            segments.push(PathSegment::with_generics(ident, generics));

            if !self.eat(&TokenKind::ColonColon) {
                break;
            }
        }

        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
        Ok(Path::new(segments, span))
    }

    /// Parse generic arguments in a path.
    fn parse_generic_args(&mut self) -> ParseResult<Vec<GenericArg>> {
        self.expect(&TokenKind::Lt)?;

        let mut args = Vec::new();

        while !self.check(&TokenKind::Gt)
            && !self.check(&TokenKind::Shr) // >> can close nested generics
            && !self.is_eof()
        {
            if self.check_lifetime() {
                let lifetime = self.expect_lifetime()?;
                args.push(GenericArg::Lifetime(lifetime));
            } else if (self.check_ident() || self.is_contextual_keyword())
                && matches!(self.peek().kind, TokenKind::Eq)
            {
                // Associated type binding: `Item = T` in `Iterator<Item = T>`.
                // Inside a call-site generic argument list, `IDENT = TYPE` is
                // only ever a binding -- no other production puts `=` after a
                // name here -- so this never shadows a real type argument.
                let name = self.expect_ident()?;
                self.expect(&TokenKind::Eq)?;
                let ty = self.parse_type()?;
                args.push(GenericArg::AssocType {
                    name,
                    ty: Box::new(ty),
                });
            } else {
                let ty = self.parse_type()?;
                args.push(GenericArg::Type(Box::new(ty)));
            }

            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        self.expect_closing_angle()?;
        Ok(args)
    }

    /// Expect a closing `>` for generic arguments.
    /// Handles the `>>` → `>` + `>` split for nested generics like `Vec<Vec<T>>`.
    fn expect_closing_angle(&mut self) -> ParseResult<Span> {
        if self.check(&TokenKind::Gt) {
            Ok(self.advance().span)
        } else if self.check(&TokenKind::Shr) {
            // `>>` - consume as one `>` and replace the remaining `>` by
            // adjusting the token to Gt. We do this by advancing and then
            // inserting a virtual Gt token. Simpler: just replace the current
            // Shr token with Gt and consume it (the second > is "free").
            let span = self.current_span();
            // Replace >> with > (consume one >, leave one >)
            self.tokens[self.pos].kind = TokenKind::Gt;
            Ok(span)
        } else {
            Err(self.error_expected("`>`"))
        }
    }

    // =========================================================================
    // GENERICS
    // =========================================================================

    /// Parse generic parameters.
    fn parse_generics(&mut self) -> ParseResult<Generics> {
        if !self.check(&TokenKind::Lt) {
            return Ok(Generics::empty());
        }

        let start = self.advance().span;
        let mut params = Vec::new();

        while !self.check(&TokenKind::Gt) && !self.check(&TokenKind::Shr) && !self.is_eof() {
            let attrs = self.parse_outer_attrs()?;

            let param = if self.check_lifetime() {
                self.parse_lifetime_param(attrs)?
            } else if self.check_keyword(Keyword::Const) {
                self.parse_const_param(attrs)?
            } else {
                self.parse_type_param(attrs)?
            };

            params.push(param);

            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        let end = self.expect_closing_angle()?;
        let span = start.merge(&end);

        let where_clause = if self.check_keyword(Keyword::Where) {
            Some(self.parse_where_clause()?)
        } else {
            None
        };

        Ok(Generics {
            params,
            where_clause,
            span,
        })
    }

    /// Parse a type parameter.
    fn parse_type_param(&mut self, attrs: Vec<Attribute>) -> ParseResult<GenericParam> {
        let ident = self.expect_ident()?;
        let span = ident.span;

        let bounds = if self.eat(&TokenKind::Colon) {
            self.parse_type_bounds()?
        } else {
            Vec::new()
        };

        let default = if self.eat(&TokenKind::Eq) {
            Some(Box::new(self.parse_type()?))
        } else {
            None
        };

        Ok(GenericParam {
            ident,
            kind: GenericParamKind::Type { bounds, default },
            attrs,
            span,
        })
    }

    /// Parse a lifetime parameter.
    fn parse_lifetime_param(&mut self, attrs: Vec<Attribute>) -> ParseResult<GenericParam> {
        let lifetime = self.expect_lifetime()?;
        let span = lifetime.span;

        let bounds = if self.eat(&TokenKind::Colon) {
            let mut bounds = Vec::new();
            loop {
                bounds.push(self.expect_lifetime()?);
                if !self.eat(&TokenKind::Plus) {
                    break;
                }
            }
            bounds
        } else {
            Vec::new()
        };

        Ok(GenericParam {
            ident: lifetime.name,
            kind: GenericParamKind::Lifetime { bounds },
            attrs,
            span,
        })
    }

    /// Parse a const parameter.
    fn parse_const_param(&mut self, attrs: Vec<Attribute>) -> ParseResult<GenericParam> {
        let start = self.expect_keyword(Keyword::Const)?;
        let ident = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.parse_type()?;

        let default = if self.eat(&TokenKind::Eq) {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);

        Ok(GenericParam {
            ident,
            kind: GenericParamKind::Const {
                ty: Box::new(ty),
                default,
            },
            attrs,
            span,
        })
    }

    /// Parse type bounds (e.g., `Clone + Debug + 'static`).
    fn parse_type_bounds(&mut self) -> ParseResult<Vec<TypeBound>> {
        let mut bounds = Vec::new();

        loop {
            let is_maybe = self.eat(&TokenKind::Question);

            // Handle lifetime bounds like 'static, 'a
            if self.check_lifetime() {
                let lifetime = self.expect_lifetime()?;
                let path = Path::from_ident(lifetime.name);
                bounds.push(TypeBound {
                    path,
                    is_maybe,
                    span: lifetime.span,
                });
            } else {
                let path = self.parse_path()?;
                let span = path.span;
                // Fn-trait sugar: `Fn(A, B) -> C` (also FnMut/FnOnce). Consume the
                // parenthesized argument types and optional return type so the bound
                // parses; without this the unparsed `(...) -> T` derails the generic-
                // param list and truncates the impl block, leaking later methods.
                if self.eat(&TokenKind::OpenDelim(Delimiter::Paren)) {
                    if !self.check(&TokenKind::CloseDelim(Delimiter::Paren)) {
                        loop {
                            let _ = self.parse_type()?;
                            if !self.eat(&TokenKind::Comma) {
                                break;
                            }
                        }
                    }
                    self.expect(&TokenKind::CloseDelim(Delimiter::Paren))?;
                    if self.eat(&TokenKind::Arrow) {
                        let _ = self.parse_type()?;
                    }
                }
                bounds.push(TypeBound {
                    path,
                    is_maybe,
                    span,
                });
            }

            if !self.eat(&TokenKind::Plus) {
                break;
            }
        }

        Ok(bounds)
    }

    /// Parse a where clause.
    fn parse_where_clause(&mut self) -> ParseResult<WhereClause> {
        let start = self.expect_keyword(Keyword::Where)?;
        let mut predicates = Vec::new();

        loop {
            let ty = self.parse_type()?;
            self.expect(&TokenKind::Colon)?;
            let bounds = self.parse_type_bounds()?;
            let span = ty.span.merge(&self.tokens[self.pos.saturating_sub(1)].span);

            predicates.push(WherePredicate {
                ty: Box::new(ty),
                bounds,
                span,
            });

            if !self.eat(&TokenKind::Comma) {
                break;
            }

            // Check for end of where clause
            if self.check(&TokenKind::OpenDelim(Delimiter::Brace))
                || self.check(&TokenKind::Semi)
                || self.check(&TokenKind::Eq)
            {
                break;
            }
        }

        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
        Ok(WhereClause { predicates, span })
    }

    // =========================================================================
    // ERROR HANDLING
    // =========================================================================

    /// Create an error for unexpected token.
    fn error_unexpected(&self) -> ParseError {
        ParseError::new(
            ParseErrorKind::UnexpectedToken {
                found: format!("{}", self.current_kind()),
            },
            self.current_span(),
        )
    }

    /// Create an error for expected something.
    fn error_expected(&self, expected: impl Into<String>) -> ParseError {
        ParseError::new(
            ParseErrorKind::Expected {
                expected: expected.into(),
                found: format!("{}", self.current_kind()),
            },
            self.current_span(),
        )
    }

    /// Recover to the next item boundary after a parse error.
    /// Tracks brace depth to correctly skip past nested blocks.
    fn recover_to_item(&mut self) {
        let mut brace_depth: i32 = 0;
        while !self.is_eof() {
            match self.current_kind() {
                // Track brace depth to avoid stopping inside a nested block
                TokenKind::OpenDelim(Delimiter::Brace) => {
                    brace_depth += 1;
                    self.advance();
                    continue;
                }
                TokenKind::CloseDelim(Delimiter::Brace) => {
                    if brace_depth > 0 {
                        brace_depth -= 1;
                        self.advance();
                        continue;
                    } else {
                        // At top level - consume the } and stop
                        self.advance();
                        break;
                    }
                }
                // Item-starting tokens - only stop if at top brace level
                TokenKind::Keyword(
                    Keyword::Fn
                    | Keyword::Struct
                    | Keyword::Enum
                    | Keyword::Trait
                    | Keyword::Impl
                    | Keyword::Type
                    | Keyword::Const
                    | Keyword::Static
                    | Keyword::Mod
                    | Keyword::Module
                    | Keyword::Use
                    | Keyword::Pub
                    | Keyword::Extern
                    | Keyword::Effect
                    | Keyword::Unsafe
                    | Keyword::Async,
                ) if brace_depth == 0 => break,
                // Attributes also start items
                TokenKind::Pound | TokenKind::At if brace_depth == 0 => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// Recover to the next statement.
    fn recover_to_stmt(&mut self) {
        while !self.is_eof() {
            match self.current_kind() {
                TokenKind::Semi => {
                    self.advance();
                    break;
                }
                TokenKind::CloseDelim(Delimiter::Brace) => break,
                TokenKind::Keyword(
                    Keyword::Let
                    | Keyword::If
                    | Keyword::While
                    | Keyword::For
                    | Keyword::Loop
                    | Keyword::Match
                    | Keyword::Return
                    | Keyword::Break
                    | Keyword::Continue,
                ) => break,
                _ => {
                    self.advance();
                }
            }
        }
    }
}

/// Parse source code into an AST.
pub fn parse(source: &SourceFile, tokens: Vec<Token>) -> ParseResult<Module> {
    let mut parser = Parser::new(source, tokens);
    parser.parse()
}

/// Convenience function to lex and parse source code.
pub fn parse_source(name: &str, source: &str) -> ParseResult<Module> {
    let source_file = SourceFile::new(name, source);
    let mut lexer = crate::lexer::Lexer::new(&source_file);
    let tokens = lexer
        .tokenize()
        .map_err(|e| ParseError::new(ParseErrorKind::LexerError(e.to_string()), e.span))?;
    parse(&source_file, tokens)
}
