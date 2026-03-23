// ===============================================================================
// QUANTALANG PARSER - STATEMENT PARSING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Statement and block parsing.
//!
//! This module handles parsing of statements, blocks, and local bindings.

use crate::ast::*;
use crate::lexer::{Delimiter, Keyword, TokenKind};
use super::{Parser, ParseResult, ParseError, ParseErrorKind};

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

        Ok(Block { stmts, span, id: NodeId::DUMMY })
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
            TokenKind::Keyword(Keyword::Let) => {
                self.parse_let_stmt(attrs)
            }

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
            _ => {
                self.parse_expr_stmt(attrs)
            }
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
    fn parse_expr_stmt(&mut self, attrs: Vec<Attribute>) -> ParseResult<Stmt> {
        let expr = self.parse_expr()?;
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
                Err(ParseError::new(ParseErrorKind::ExpectedSemicolon, self.current_span()))
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
            TokenKind::Keyword(kw) => matches!(
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
                    | Keyword::Unsafe
                    | Keyword::Async
                    | Keyword::Effect
                    | Keyword::Macro
            ),
            _ => false,
        }
    }

    /// Parse multiple statements until end of block.
    pub fn parse_stmts(&mut self) -> ParseResult<Vec<Stmt>> {
        let mut stmts = Vec::new();

        while !self.check(&TokenKind::CloseDelim(Delimiter::Brace))
            && !self.is_eof()
        {
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
        let source = LexerSourceFile::new("test.quanta", format!("fn test() {{ {} }}", s));
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
}
