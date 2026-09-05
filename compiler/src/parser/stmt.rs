// ===============================================================================
// BUILDLANG PARSER - STATEMENT PARSING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! Statement and block parsing.
//!
//! This module handles parsing of statements, blocks, and local bindings.

use super::{ParseError, ParseErrorKind, ParseResult, Parser};
use crate::ast::*;
use crate::lexer::{Delimiter, Keyword, TokenKind};

impl<'a> Parser<'a> {
    /// Parse a block: `{ statements... }`
    pub fn parse_block(&mut self) -> ParseResult<Block> {
        let start = self.expect(&TokenKind::OpenDelim(Delimiter::Brace))?.span;

        let mut stmts = Vec::new();

        while !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) && !self.is_eof() {
            match self.parse_stmt() {
                Ok(stmt) => stmts.push(stmt),
                Err(e) => {
                    self.errors.push(e);
                    self.recover_to_stmt();
                }
            }
        }

        let end = self.expect(&TokenKind::CloseDelim(Delimiter::Brace))?.span;
        let span = start.merge(&end);

        Ok(Block {
            stmts,
            span,
            id: NodeId::DUMMY,
        })
    }

    /// Parse a statement.
    pub fn parse_stmt(&mut self) -> ParseResult<Stmt> {
        let attrs = self.parse_outer_attrs()?;
        let start = self.current_span();

        // Check for item statements
        if self.is_item_start() {
            let item = self.parse_item()?;
            let span = item.span;
            return Ok(Stmt::new(StmtKind::Item(Box::new(item)), span));
        }

        match self.current_kind().clone() {
            // =================================================================
            // LET STATEMENT
            // =================================================================
            TokenKind::Keyword(Keyword::Let) => self.parse_let_stmt(attrs),

            // =================================================================
            // SEMICOLON (empty statement)
            // =================================================================
            TokenKind::Semi => {
                self.advance();
                Ok(Stmt::new(StmtKind::Empty, start))
            }

            // =================================================================
            // EXPRESSION STATEMENT
            // =================================================================
            _ => self.parse_expr_stmt(attrs),
        }
    }

    /// Parse a let statement: `let pattern: type = expr;`
    fn parse_let_stmt(&mut self, attrs: Vec<Attribute>) -> ParseResult<Stmt> {
        let start = self.expect_keyword(Keyword::Let)?;

        let pattern = self.parse_pattern()?;

        let ty = if self.eat(&TokenKind::Colon) {
            Some(Box::new(self.parse_type()?))
        } else {
            None
        };

        let init = if self.eat(&TokenKind::Eq) {
            let value = self.parse_expr()?;

            // Check for else branch (let-else)
            let diverge = if self.eat_keyword(Keyword::Else) {
                let block = self.parse_block()?;
                let block_span = block.span;
                let expr = Expr {
                    kind: ExprKind::Block(Box::new(block)),
                    span: block_span,
                    id: NodeId::DUMMY,
                    attrs: Vec::new(),
                };
                Some(Box::new(expr))
            } else {
                None
            };

            Some(LocalInit {
                expr: Box::new(value),
                diverge,
            })
        } else {
            None
        };

        let end = self.expect(&TokenKind::Semi)?.span;
        let span = start.merge(&end);

        let local = Local {
            attrs,
            pattern,
            ty,
            init,
            span,
            id: NodeId::DUMMY,
        };

        Ok(Stmt::new(StmtKind::Local(Box::new(local)), span))
    }

    /// Parse an expression statement.
    fn parse_expr_stmt(&mut self, _attrs: Vec<Attribute>) -> ParseResult<Stmt> {
        // Statement position: a block-like leading expression (`if`, `match`,
        // `{...}`, a loop, `unsafe`/`async`/`handle`) terminates at its closing
        // brace and does not bind a trailing operator, so `if c { .. }` on one
        // line and `*ptr = v;` on the next parse as two statements.
        let expr = self.parse_stmt_expr()?;
        let start = expr.span;

        // Check if this is a block expression that doesn't need semicolon
        let needs_semi = !self.expr_is_complete(&expr);

        if self.eat(&TokenKind::Semi) {
            // Expression with semicolon - value is discarded
            let span = start.merge(&self.tokens[self.pos - 1].span);
            Ok(Stmt::new(StmtKind::Semi(Box::new(expr)), span))
        } else if needs_semi {
            // Check if we're at the end of a block
            if self.check(&TokenKind::CloseDelim(Delimiter::Brace)) {
                // Expression is the final expression of the block
                Ok(Stmt::new(StmtKind::Expr(Box::new(expr)), start))
            } else {
                // Missing semicolon
                Err(ParseError::new(
                    ParseErrorKind::ExpectedSemicolon,
                    self.current_span(),
                ))
            }
        } else {
            // Block expression, no semicolon needed
            Ok(Stmt::new(StmtKind::Expr(Box::new(expr)), start))
        }
    }

    /// Check if an expression is "complete" (doesn't require a semicolon).
    fn expr_is_complete(&self, expr: &Expr) -> bool {
        matches!(
            expr.kind,
            ExprKind::If { .. }
                | ExprKind::IfLet { .. }
                | ExprKind::Match { .. }
                | ExprKind::Loop { .. }
                | ExprKind::While { .. }
                | ExprKind::WhileLet { .. }
                | ExprKind::For { .. }
                | ExprKind::Block(_)
                | ExprKind::Unsafe(_)
                | ExprKind::Async { .. }
        )
    }

    /// Check if the current token starts an item.
    fn is_item_start(&self) -> bool {
        // Check for visibility first
        if self.check_keyword(Keyword::Pub) {
            return true;
        }

        // Check for outer attributes
        if self.check(&TokenKind::Pound) && !matches!(self.peek().kind, TokenKind::Not) {
            // Could be item with attributes
            return true;
        }

        match self.current_kind() {
            TokenKind::Keyword(kw) => match kw {
                // unsafe/async are only item starts when followed by fn/impl/trait/mod
                // (e.g., `unsafe fn`, `unsafe impl`). Standalone `unsafe { ... }` blocks
                // are expressions, not items.
                Keyword::Unsafe | Keyword::Async => {
                    matches!(
                        self.peek().kind,
                        TokenKind::Keyword(Keyword::Fn)
                            | TokenKind::Keyword(Keyword::Impl)
                            | TokenKind::Keyword(Keyword::Trait)
                            | TokenKind::Keyword(Keyword::Mod)
                            | TokenKind::Keyword(Keyword::Extern)
                    )
                }
                _ => matches!(
                    kw,
                    Keyword::Fn
                        | Keyword::Struct
                        | Keyword::Enum
                        | Keyword::Trait
                        | Keyword::Impl
                        | Keyword::Type
                        | Keyword::Const
                        | Keyword::Static
                        | Keyword::Mod
                        | Keyword::Use
                        | Keyword::Extern
                        | Keyword::Effect
                        | Keyword::Macro
                ),
            },
            _ => false,
        }
    }

    /// Parse multiple statements until end of block.
    pub fn parse_stmts(&mut self) -> ParseResult<Vec<Stmt>> {
        let mut stmts = Vec::new();

        while !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) && !self.is_eof() {
            match self.parse_stmt() {
                Ok(stmt) => stmts.push(stmt),
                Err(e) => {
                    self.errors.push(e);
                    self.recover_to_stmt();
                }
            }
        }

        Ok(stmts)
    }

    /// Parse an optional block or expression.
    pub fn parse_block_or_expr(&mut self) -> ParseResult<Expr> {
        if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            let block = self.parse_block()?;
            let span = block.span;
            Ok(Expr::new(ExprKind::Block(Box::new(block)), span))
        } else {
            self.parse_expr()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{Lexer, SourceFile as LexerSourceFile};

    fn parse_stmt_from_str(s: &str) -> ParseResult<Stmt> {
        let source = LexerSourceFile::new("test.bld", format!("fn test() {{ {} }}", s));
        let mut lexer = Lexer::new(&source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(&source, tokens);
        // Skip fn test() {
        parser.advance(); // fn
        parser.advance(); // test
        parser.advance(); // (
        parser.advance(); // )
        parser.advance(); // {
        parser.parse_stmt()
    }

    #[test]
    fn test_let_stmt() {
        // Basic let
        let result = parse_stmt_from_str("let x = 42;");
        assert!(result.is_ok());

        // Let with type
        let result = parse_stmt_from_str("let x: i32 = 42;");
        assert!(result.is_ok());

        // Let without initializer
        let result = parse_stmt_from_str("let x: i32;");
        assert!(result.is_ok());
    }

    #[test]
    fn test_expr_stmt() {
        let result = parse_stmt_from_str("x + 1;");
        assert!(result.is_ok());
    }

    /// Parse a whole function body and return the parser's collected errors.
    fn parse_fn_body_errors(body: &str) -> Vec<String> {
        let source = LexerSourceFile::new("test.bld", format!("fn test() {{ {} }}", body));
        let mut lexer = Lexer::new(&source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(&source, tokens);
        let _ = parser.parse();
        parser.errors().iter().map(|e| e.message()).collect()
    }

    #[test]
    fn block_like_statement_does_not_swallow_next_deref_assignment() {
        // A block-like statement (`if`, `match`, bare block, loop, `unsafe`)
        // is complete at its closing brace: a following line that begins with
        // a prefix operator (`*ptr = v`, `*acc += n`, `-x`) is a SEPARATE
        // statement, not a trailing operand of the block. Before the
        // ExprWithBlock fix the `*` bound as multiplication continuing the
        // block, and the later `=`/`+=` made the block the invalid left side
        // of an assignment ("invalid left-hand side of assignment"). Each case
        // is the shape found in the corpus (oracle/lib, nexus/diagnostics).
        let bodies = [
            "if sum > 0.0 { normalize(); }\n*run_length_probs = new_probs;",
            "if a > b { record(); }\n*self.usage.entry(k).or_insert(0) += size;",
            "match tag { A => one(), B => two() }\n*out = done;",
            "unsafe { touch(); }\n*p = q;",
            "{ let t = 1; use_it(t); }\n*p = q;",
        ];
        for body in bodies {
            let errors = parse_fn_body_errors(body);
            assert!(
                errors.is_empty(),
                "block-like statement followed by a deref-assignment should parse \
                 as two statements, got errors for {body:?}: {errors:?}"
            );
        }
    }

    #[test]
    fn assignment_with_block_like_right_side_still_binds() {
        // The fix is scoped to statement position: a block-like expression on
        // the RIGHT of `=` (value position) is unaffected and still parses as
        // the assignment's value. This guards against a fix that stopped
        // operator binding everywhere.
        let errors = parse_fn_body_errors("let x = if c { 1 } else { 2 };\nuse_it(x);");
        assert!(
            errors.is_empty(),
            "a block-like expression in value position must still parse, got: {errors:?}"
        );
    }

    #[test]
    fn if_let_block_statement_needs_no_semicolon() {
        // A non-tail `if let { ... }` is a block-expression statement, so no
        // trailing `;` is required -- the same as a plain `if { ... }`. A second
        // statement follows the first, so the first is not in tail position.
        let errors = parse_fn_body_errors(
            "if let Some(x) = opt { use_it(x); }\n\
             if let Some(y) = other { use_it(y); }\n\
             done();",
        );
        assert!(
            errors.is_empty(),
            "consecutive `if let` statements should parse without a semicolon, got: {errors:?}"
        );
    }

    #[test]
    fn plain_if_block_statement_needs_no_semicolon() {
        // Control: the plain `if` case that already worked, guarding the shared
        // `expr_is_complete` path against regression.
        let errors = parse_fn_body_errors(
            "if a { one(); }\n\
             if b { two(); }\n\
             done();",
        );
        assert!(errors.is_empty(), "consecutive `if` statements should parse, got: {errors:?}");
    }

    #[test]
    fn turbofish_on_path_call_parses() {
        // `path::seg::<T>()` turbofish on a plain `::`-path (not a `.method`
        // call and not `Self::`) was rejected with `expected identifier, found
        // `<``, because the path loop ate the `::` and then demanded an ident.
        let errors = parse_fn_body_errors(
            "let n = std::mem::size_of::<i32>();\n\
             let r = rand::random::<f32>();\n\
             let v = Vec::<u8>::new();",
        );
        assert!(errors.is_empty(), "turbofish path calls should parse, got: {errors:?}");
    }
}
