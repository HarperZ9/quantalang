// ===============================================================================
// BUILDLANG PARSER - EXPRESSION PARSING (PRATT)
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! Pratt expression parser.
//!
//! This module implements a Pratt parser (top-down operator precedence) for
//! parsing expressions. Pratt parsing handles operator precedence and
//! associativity elegantly through binding power.

use super::{ParseError, ParseErrorKind, ParseResult, Parser};
use crate::ast::*;
use crate::lexer::{Delimiter, IntBase, Keyword, TokenKind};

/// Binding power for operators.
mod bp {
    /// Assignment (lowest)
    pub const ASSIGN: u8 = 1;
    /// Range
    pub const RANGE: u8 = 2;
    /// Logical OR
    pub const OR: u8 = 3;
    /// Logical AND
    pub const AND: u8 = 4;
    /// Comparison
    pub const COMPARE: u8 = 5;
    /// Bitwise OR
    pub const BIT_OR: u8 = 6;
    /// Bitwise XOR
    pub const BIT_XOR: u8 = 7;
    /// Bitwise AND
    pub const BIT_AND: u8 = 8;
    /// Shift
    pub const SHIFT: u8 = 9;
    /// Pipe operator
    pub const PIPE: u8 = 10;
    /// Addition/Subtraction
    pub const SUM: u8 = 11;
    /// Multiplication/Division
    pub const PRODUCT: u8 = 12;
    /// Type cast (as)
    pub const CAST: u8 = 13;
    /// Prefix operators
    pub const PREFIX: u8 = 14;
    /// Power `**` (right-associative). Deliberately equal to `PREFIX` so that
    /// power binds INSIDE a leading unary minus: `-2 ** 2` parses as
    /// `-(2 ** 2) == -4`, matching the Julia/Python convention `-a**b ==
    /// -(a**b)`. (If `POWER < PREFIX` it would instead give `(-2) ** 2 == 4`.)
    /// Prefix `**x` is dispatched by leading token in `parse_prefix_expr`
    /// before any binding-power comparison, so it always means double-deref and
    /// never collides with this infix power binding.
    pub const POWER: u8 = 14;
    /// Postfix operators (highest)
    pub const POSTFIX: u8 = 15;
}

impl<'a> Parser<'a> {
    /// Parse an expression.
    pub fn parse_expr(&mut self) -> ParseResult<Expr> {
        self.parse_expr_with_bp(0)
    }

    /// Parse an expression in statement position under Rust's ExprWithBlock
    /// rule. A block-like leading atom (`if`, `match`, a bare `{...}` block,
    /// `loop`/`while`/`for`, `unsafe`/`async`/`handle`) is a complete statement
    /// on its own: it does not bind a trailing infix or postfix operator, so a
    /// following `*x` / `-x` / `&x` / `.foo()` begins a NEW statement instead of
    /// being read as `block * x`, `block - x`, and so on. Without this a body
    /// like `if c { .. } \n *ptr = v;` parses the `*` as multiplication
    /// continuing the `if`, then the later `=` makes the whole `if * ptr` an
    /// invalid assignment target.
    ///
    /// Any other leading atom resumes the ordinary Pratt loop, so
    /// value-position parsing (the right side of `=`, call arguments, operands)
    /// is untouched: only a statement that STARTS with a block-like expression
    /// is affected. This mirrors the match-arm body rule; `continue_expr_with_bp`
    /// is the shared tail.
    pub fn parse_stmt_expr(&mut self) -> ParseResult<Expr> {
        let lhs = self.parse_prefix_expr()?;
        if Self::is_block_like_expr(&lhs) {
            return Ok(lhs);
        }
        self.continue_expr_with_bp(lhs, 0)
    }

    /// Parse an expression with minimum binding power.
    fn parse_expr_with_bp(&mut self, min_bp: u8) -> ParseResult<Expr> {
        // Parse prefix (atoms and unary operators)
        let lhs = self.parse_prefix_expr()?;

        // Control flow expressions (while, for, loop, if-without-value) are
        // statement-level constructs that cannot be the left operand of a
        // binary expression. Stop parsing infix/postfix after them.
        // This prevents `while ... { } -1` from being parsed as
        // `(while_result) - 1` (binary subtraction) instead of two separate
        // expressions (while statement, then -1 as implicit return).
        if Self::is_statement_expr(&lhs) {
            return Ok(lhs);
        }

        self.continue_expr_with_bp(lhs, min_bp)
    }

    /// Continue an expression from an already-parsed prefix atom, binding
    /// postfix and infix operators at or above `min_bp`. Split out of
    /// `parse_expr_with_bp` so a caller that has parsed the atom itself -- a
    /// match-arm body, which must classify the atom as block-like or value
    /// before any operator binds -- can resume the ordinary Pratt loop.
    fn continue_expr_with_bp(&mut self, mut lhs: Expr, min_bp: u8) -> ParseResult<Expr> {
        loop {
            // Try postfix operators first (highest precedence)
            if let Some(postfix_bp) = self.postfix_binding_power() {
                if postfix_bp >= min_bp {
                    lhs = self.parse_postfix_expr(lhs)?;
                    continue;
                }
            }

            // Try infix operators
            if let Some((left_bp, right_bp)) = self.infix_binding_power() {
                if left_bp < min_bp {
                    break;
                }

                lhs = self.parse_infix_expr(lhs, right_bp)?;
                continue;
            }

            break;
        }

        Ok(lhs)
    }

    /// Check if an expression is a statement-level control flow construct
    /// that should not participate in binary/postfix operations.
    fn is_statement_expr(expr: &Expr) -> bool {
        matches!(
            &expr.kind,
            ExprKind::While { .. } | ExprKind::Loop { .. } | ExprKind::For { .. }
        )
    }

    /// Check whether an expression is "block-like" (Rust's ExprWithBlock): a
    /// construct that closes on a brace and is complete on its own. As a
    /// match-arm body such an expression does not continue into trailing
    /// postfix/infix operators, so it cannot swallow the following arm's
    /// pattern, and its trailing comma is optional. A value body (anything
    /// else) binds operators normally and must be terminated by `,` or `}`.
    fn is_block_like_expr(expr: &Expr) -> bool {
        matches!(
            &expr.kind,
            ExprKind::Block(_)
                | ExprKind::If { .. }
                | ExprKind::Match { .. }
                | ExprKind::Loop { .. }
                | ExprKind::While { .. }
                | ExprKind::For { .. }
                | ExprKind::Unsafe(_)
                | ExprKind::Async { .. }
                | ExprKind::Handle { .. }
        )
    }

    /// Parse a prefix expression (atoms and unary operators).
    fn parse_prefix_expr(&mut self) -> ParseResult<Expr> {
        let start = self.current_span();

        match self.current_kind().clone() {
            // =====================================================================
            // LITERALS
            // =====================================================================
            TokenKind::Literal { kind, suffix } => {
                self.advance();
                let literal = self.convert_literal(&kind, suffix.as_deref())?;
                Ok(Expr::new(ExprKind::Literal(literal), start))
            }

            // =====================================================================
            // IDENTIFIERS AND PATHS
            // =====================================================================
            TokenKind::Ident | TokenKind::RawIdent => self.parse_path_or_struct_expr(),

            // Contextual keywords used as a plain value (a variable, field, call,
            // or path named `default`/`module`/`effect`/`auto`). Their declaration
            // forms are consumed by the item parser before expression parsing, so
            // here they are ordinary identifiers and parse as such.
            TokenKind::Keyword(_) if self.is_value_context_keyword() => {
                self.parse_path_or_struct_expr()
            }

            TokenKind::Keyword(Keyword::Self_) => {
                self.advance();
                let ident = Ident::new("self", start);
                Ok(Expr::new(ExprKind::Ident(ident), start))
            }

            TokenKind::Keyword(Keyword::SelfType) => {
                self.advance();
                let mut segments = vec![PathSegment {
                    ident: Ident::new("Self", start),
                    generics: vec![],
                }];

                // Continue parsing path segments: Self::method, Self::Type
                while self.check(&TokenKind::ColonColon) {
                    self.advance(); // consume ::
                    if self.check_ident() || self.is_contextual_keyword() {
                        let ident = self.expect_ident()?;
                        let generics = if self.check(&TokenKind::ColonColon)
                            && self.peek().kind == TokenKind::Lt
                        {
                            self.advance(); // ::
                            self.parse_generic_args()?
                        } else if self.check(&TokenKind::Lt) {
                            // Turbofish or generic args
                            self.parse_generic_args()?
                        } else {
                            vec![]
                        };
                        segments.push(PathSegment { ident, generics });
                    } else {
                        break;
                    }
                }

                let path = Path::new(
                    segments,
                    start.merge(&self.tokens[self.pos.saturating_sub(1)].span),
                );

                // Check for struct literal: Self { ... } or Self::Variant { ... }
                if !self.restrictions.no_struct_literal
                    && self.check(&TokenKind::OpenDelim(Delimiter::Brace))
                {
                    return self.parse_struct_expr(path);
                }

                // Check for function call: Self::new(...)
                if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
                    let (args, _) = self.parse_paren_comma_seq(|p| p.parse_expr())?;
                    let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                    return Ok(Expr::new(
                        ExprKind::Call {
                            func: Box::new(Expr::new(ExprKind::Path(path), start)),
                            args,
                        },
                        span,
                    ));
                }

                // Check for macro: Self!(...) - unlikely but handle it
                if self.check(&TokenKind::Not) {
                    self.advance();
                    return self.parse_macro_expr(path, start);
                }

                Ok(Expr::new(ExprKind::Path(path), start))
            }

            TokenKind::Keyword(Keyword::Crate) | TokenKind::Keyword(Keyword::Super) => {
                self.advance();
                let keyword_name = if matches!(
                    self.tokens[self.pos - 1].kind,
                    TokenKind::Keyword(Keyword::Crate)
                ) {
                    "crate"
                } else {
                    "super"
                };
                let mut segments = vec![PathSegment {
                    ident: Ident::new(keyword_name, start),
                    generics: vec![],
                }];

                // Continue: crate::module::func() or super::func()
                while self.check(&TokenKind::ColonColon) {
                    self.advance();
                    if self.check_ident() || self.is_contextual_keyword() {
                        let ident = self.expect_ident()?;
                        segments.push(PathSegment {
                            ident,
                            generics: vec![],
                        });
                    } else {
                        break;
                    }
                }

                let path = Path::new(
                    segments,
                    start.merge(&self.tokens[self.pos.saturating_sub(1)].span),
                );

                // Check for function call
                if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
                    let (args, _) = self.parse_paren_comma_seq(|p| p.parse_expr())?;
                    let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                    return Ok(Expr::new(
                        ExprKind::Call {
                            func: Box::new(Expr::new(ExprKind::Path(path), start)),
                            args,
                        },
                        span,
                    ));
                }

                Ok(Expr::new(ExprKind::Path(path), start))
            }

            // =====================================================================
            // UNARY OPERATORS
            // =====================================================================
            TokenKind::Minus => {
                self.advance();
                let expr = self.parse_expr_with_bp(bp::PREFIX)?;
                let span = start.merge(&expr.span);
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnaryOp::Neg,
                        expr: Box::new(expr),
                    },
                    span,
                ))
            }

            TokenKind::Not => {
                self.advance();
                let expr = self.parse_expr_with_bp(bp::PREFIX)?;
                let span = start.merge(&expr.span);
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(expr),
                    },
                    span,
                ))
            }

            TokenKind::Tilde => {
                self.advance();
                let expr = self.parse_expr_with_bp(bp::PREFIX)?;
                let span = start.merge(&expr.span);
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnaryOp::BitNot,
                        expr: Box::new(expr),
                    },
                    span,
                ))
            }

            TokenKind::Star => {
                self.advance();
                let expr = self.parse_expr_with_bp(bp::PREFIX)?;
                let span = start.merge(&expr.span);
                Ok(Expr::new(ExprKind::Deref(Box::new(expr)), span))
            }

            // Prefix `**x` = double dereference `*(*x)`. Before I2, `**` lexed
            // as two `Star` tokens, so `**ptr` was two nested prefix derefs;
            // now that `**` is a single `StarStar` token we must reproduce that
            // meaning in prefix position (infix `a ** b` is power). This split
            // is what keeps the change backward compatible.
            TokenKind::StarStar => {
                self.advance();
                let expr = self.parse_expr_with_bp(bp::PREFIX)?;
                let span = start.merge(&expr.span);
                let inner = Expr::new(ExprKind::Deref(Box::new(expr)), span);
                Ok(Expr::new(ExprKind::Deref(Box::new(inner)), span))
            }

            TokenKind::And => {
                self.advance();
                let mutability = if self.eat_keyword(Keyword::Mut) {
                    Mutability::Mutable
                } else {
                    Mutability::Immutable
                };
                let expr = self.parse_expr_with_bp(bp::PREFIX)?;
                let span = start.merge(&expr.span);
                Ok(Expr::new(
                    ExprKind::Ref {
                        mutability,
                        expr: Box::new(expr),
                    },
                    span,
                ))
            }

            TokenKind::AndAnd => {
                // && is sugar for & &
                self.advance();
                let inner = self.parse_expr_with_bp(bp::PREFIX)?;
                let inner_span = start.merge(&inner.span);
                let inner_ref = Expr::new(
                    ExprKind::Ref {
                        mutability: Mutability::Immutable,
                        expr: Box::new(inner),
                    },
                    inner_span,
                );
                Ok(Expr::new(
                    ExprKind::Ref {
                        mutability: Mutability::Immutable,
                        expr: Box::new(inner_ref),
                    },
                    inner_span,
                ))
            }

            // =====================================================================
            // GROUPED / TUPLE / UNIT
            // =====================================================================
            TokenKind::OpenDelim(Delimiter::Paren) => self.parse_paren_expr(),

            // =====================================================================
            // ARRAY
            // =====================================================================
            TokenKind::OpenDelim(Delimiter::Bracket) => self.parse_array_expr(),

            // =====================================================================
            // BLOCK
            // =====================================================================
            TokenKind::OpenDelim(Delimiter::Brace) => {
                let block = self.parse_block()?;
                let span = block.span;
                Ok(Expr::new(ExprKind::Block(Box::new(block)), span))
            }

            // =====================================================================
            // CONTROL FLOW
            // =====================================================================
            TokenKind::Keyword(Keyword::If) => self.parse_if_expr(),
            TokenKind::Keyword(Keyword::Match) => self.parse_match_expr(),
            TokenKind::Keyword(Keyword::Loop) => self.parse_loop_expr(),
            TokenKind::Keyword(Keyword::While) => self.parse_while_expr(),
            TokenKind::Keyword(Keyword::For) => self.parse_for_expr(),

            // =====================================================================
            // EFFECT SYSTEM
            // =====================================================================
            TokenKind::Keyword(Keyword::Handle) => {
                // `handle { ... } with { ... }` is the effect-handler expression,
                // which always has a `{` after `handle`. But `handle` is also a
                // common identifier (e.g. a function name); when it is not
                // followed by `{`, parse it as an identifier so `handle(x)` is a
                // normal call. Without this, `let r = handle(x);` was parsed as a
                // handler expression and swallowed the following statements.
                if matches!(self.peek().kind, TokenKind::OpenDelim(Delimiter::Brace)) {
                    self.parse_handle_expr()
                } else {
                    self.advance();
                    let ident = Ident::new("handle", start);
                    Ok(Expr::new(ExprKind::Ident(ident), start))
                }
            }
            TokenKind::Keyword(Keyword::Resume) => self.parse_resume_expr(),
            TokenKind::Keyword(Keyword::Perform) => self.parse_perform_expr(),

            // =====================================================================
            // JUMPS
            // =====================================================================
            TokenKind::Keyword(Keyword::Return) => self.parse_return_expr(),
            TokenKind::Keyword(Keyword::Break) => self.parse_break_expr(),
            TokenKind::Keyword(Keyword::Continue) => self.parse_continue_expr(),

            // =====================================================================
            // CLOSURES
            // =====================================================================
            // `|params|` and `||` (zero params) both route through the closure
            // parser, which handles the `OrOr` empty-list case itself.
            TokenKind::Or | TokenKind::OrOr => self.parse_closure_expr(false, false),

            TokenKind::Keyword(Keyword::Move) => {
                self.advance();
                self.parse_closure_expr(true, false)
            }

            TokenKind::Keyword(Keyword::Async) => {
                self.advance();
                // Eat an optional `move` first: it captures for both the async
                // block (`async move { .. }`) and the async closure
                // (`async move || ..`). The block-vs-closure decision is made on
                // the token that follows it, so `move` must be consumed before
                // the `{` check rather than inside the block branch (where it
                // was unreachable and left `async move { .. }` to fall through to
                // the closure parser and fail with `expected |, found {`).
                let is_move = self.eat_keyword(Keyword::Move);
                if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
                    // async { ... } or async move { ... }
                    let block = self.parse_block()?;
                    let span = start.merge(&block.span);
                    Ok(Expr::new(
                        ExprKind::Async {
                            is_move,
                            body: Box::new(block),
                        },
                        span,
                    ))
                } else {
                    // async || ... or async move || ...
                    self.parse_closure_expr(is_move, true)
                }
            }

            // =====================================================================
            // UNSAFE
            // =====================================================================
            TokenKind::Keyword(Keyword::Unsafe) => {
                self.advance();
                let block = self.parse_block()?;
                let span = start.merge(&block.span);
                Ok(Expr::new(ExprKind::Unsafe(Box::new(block)), span))
            }

            // =====================================================================
            // RANGE (prefix form)
            // =====================================================================
            TokenKind::DotDot => {
                self.advance();
                let end = if self.can_begin_expr() {
                    Some(Box::new(self.parse_expr_with_bp(bp::RANGE)?))
                } else {
                    None
                };
                let span = if let Some(ref e) = end {
                    start.merge(&e.span)
                } else {
                    start
                };
                Ok(Expr::new(
                    ExprKind::Range {
                        start: None,
                        end,
                        inclusive: false,
                    },
                    span,
                ))
            }

            TokenKind::DotDotEq => {
                self.advance();
                let end = Box::new(self.parse_expr_with_bp(bp::RANGE)?);
                let span = start.merge(&end.span);
                Ok(Expr::new(
                    ExprKind::Range {
                        start: None,
                        end: Some(end),
                        inclusive: true,
                    },
                    span,
                ))
            }

            // =====================================================================
            // MACROS / DSL
            // =====================================================================
            TokenKind::DslBlock { ref name } => {
                let name = name.clone();
                self.advance();
                let path = Path::from_ident(Ident::new(name.as_ref(), start));
                // For now, store as macro with empty tokens (content is in the DSL block)
                Ok(Expr::new(
                    ExprKind::Macro {
                        path,
                        delimiter: Delimiter::Brace,
                        tokens: Vec::new(),
                    },
                    start,
                ))
            }

            // =====================================================================
            // ERROR
            // =====================================================================
            _ => Err(self.error_expected("expression")),
        }
    }

    /// Parse postfix expressions.
    fn parse_postfix_expr(&mut self, lhs: Expr) -> ParseResult<Expr> {
        let start = lhs.span;

        match self.current_kind().clone() {
            // Function call: expr(args)
            TokenKind::OpenDelim(Delimiter::Paren) => {
                let (args, args_span) = self.parse_paren_comma_seq(|p| p.parse_expr())?;
                let span = start.merge(&args_span);
                Ok(Expr::new(
                    ExprKind::Call {
                        func: Box::new(lhs),
                        args,
                    },
                    span,
                ))
            }

            // Index: expr[index]
            TokenKind::OpenDelim(Delimiter::Bracket) => {
                self.advance();
                let index = self.parse_expr()?;
                let end = self
                    .expect(&TokenKind::CloseDelim(Delimiter::Bracket))?
                    .span;
                let span = start.merge(&end);
                Ok(Expr::new(
                    ExprKind::Index {
                        expr: Box::new(lhs),
                        index: Box::new(index),
                    },
                    span,
                ))
            }

            // Field access / method call: expr.field or expr.method(args)
            TokenKind::Dot => {
                self.advance();

                // Check for tuple field access (expr.0)
                if let TokenKind::Literal {
                    kind: crate::lexer::LiteralKind::Int { .. },
                    ..
                } = self.current_kind()
                {
                    let token_span = self.advance().span;
                    let field_str = self.source.slice(token_span);
                    let index: u32 = field_str.parse().map_err(|_| {
                        ParseError::new(ParseErrorKind::InvalidExpression, token_span)
                    })?;
                    let span = start.merge(&token_span);
                    return Ok(Expr::new(
                        ExprKind::TupleField {
                            expr: Box::new(lhs),
                            index,
                            span: token_span,
                        },
                        span,
                    ));
                }

                // Check for .await
                if self.check_keyword(Keyword::Await) {
                    let end = self.advance().span;
                    let span = start.merge(&end);
                    return Ok(Expr::new(ExprKind::Await(Box::new(lhs)), span));
                }

                let field = self.expect_ident()?;

                // Check for method call - either `method(...)` or `method::<T>(...)`
                let is_turbofish =
                    self.check(&TokenKind::ColonColon) && self.peek().kind == TokenKind::Lt;
                if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) || is_turbofish {
                    // Method call with optional turbofish
                    let generics = if self.check(&TokenKind::ColonColon) {
                        self.advance();
                        self.parse_generic_args()?
                    } else {
                        Vec::new()
                    };

                    let (args, args_span) = self.parse_paren_comma_seq(|p| p.parse_expr())?;
                    let span = start.merge(&args_span);

                    Ok(Expr::new(
                        ExprKind::MethodCall {
                            receiver: Box::new(lhs),
                            method: field,
                            generics,
                            args,
                        },
                        span,
                    ))
                } else {
                    // Field access
                    let span = start.merge(&field.span);
                    Ok(Expr::new(
                        ExprKind::Field {
                            expr: Box::new(lhs),
                            field,
                        },
                        span,
                    ))
                }
            }

            // Try operator: expr?
            TokenKind::Question => {
                let end = self.advance().span;
                let span = start.merge(&end);
                Ok(Expr::new(ExprKind::Try(Box::new(lhs)), span))
            }

            _ => Err(self.error_unexpected()),
        }
    }

    /// Parse infix expressions.
    fn parse_infix_expr(&mut self, lhs: Expr, right_bp: u8) -> ParseResult<Expr> {
        let start = lhs.span;
        let op_span = self.current_span();

        // Check for assignment operators first
        if let Some(assign_op) = self.try_parse_assign_op() {
            if !lhs.is_place() {
                return Err(ParseError::new(
                    ParseErrorKind::InvalidAssignTarget,
                    lhs.span,
                ));
            }
            let rhs = self.parse_expr_with_bp(right_bp)?;
            let span = start.merge(&rhs.span);
            return Ok(Expr::new(
                ExprKind::Assign {
                    op: assign_op,
                    target: Box::new(lhs),
                    value: Box::new(rhs),
                },
                span,
            ));
        }

        // Check for type cast
        if self.check_keyword(Keyword::As) {
            self.advance();
            let ty = self.parse_type()?;
            let span = start.merge(&ty.span);
            return Ok(Expr::new(
                ExprKind::Cast {
                    expr: Box::new(lhs),
                    ty: Box::new(ty),
                },
                span,
            ));
        }

        // Check for range operators
        if self.check(&TokenKind::DotDot) {
            self.advance();
            let end = if self.can_begin_expr()
                && !matches!(
                    self.current_kind(),
                    TokenKind::CloseDelim(_) | TokenKind::Comma | TokenKind::Semi
                ) {
                Some(Box::new(self.parse_expr_with_bp(right_bp)?))
            } else {
                None
            };
            let span = if let Some(ref e) = end {
                start.merge(&e.span)
            } else {
                start.merge(&op_span)
            };
            return Ok(Expr::new(
                ExprKind::Range {
                    start: Some(Box::new(lhs)),
                    end,
                    inclusive: false,
                },
                span,
            ));
        }

        if self.check(&TokenKind::DotDotEq) {
            self.advance();
            let end = Box::new(self.parse_expr_with_bp(right_bp)?);
            let span = start.merge(&end.span);
            return Ok(Expr::new(
                ExprKind::Range {
                    start: Some(Box::new(lhs)),
                    end: Some(end),
                    inclusive: true,
                },
                span,
            ));
        }

        // Binary operators
        let op = self.parse_binary_op()?;
        let rhs = self.parse_expr_with_bp(right_bp)?;
        let span = start.merge(&rhs.span);

        Ok(Expr::new(
            ExprKind::Binary {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            },
            span,
        ))
    }

    /// Get the binding power for postfix operators.
    fn postfix_binding_power(&self) -> Option<u8> {
        match self.current_kind() {
            TokenKind::OpenDelim(Delimiter::Paren) => Some(bp::POSTFIX),
            TokenKind::OpenDelim(Delimiter::Bracket) => Some(bp::POSTFIX),
            TokenKind::Dot => Some(bp::POSTFIX),
            TokenKind::Question => Some(bp::POSTFIX),
            _ => None,
        }
    }

    /// Get the binding power for infix operators.
    fn infix_binding_power(&self) -> Option<(u8, u8)> {
        match self.current_kind() {
            // Assignment (right-associative)
            TokenKind::Eq => Some((bp::ASSIGN, bp::ASSIGN)),
            TokenKind::PlusEq
            | TokenKind::MinusEq
            | TokenKind::StarEq
            | TokenKind::SlashEq
            | TokenKind::PercentEq
            | TokenKind::CaretEq
            | TokenKind::AndEq
            | TokenKind::OrEq
            | TokenKind::ShlEq
            | TokenKind::ShrEq => Some((bp::ASSIGN, bp::ASSIGN)),

            // Range
            TokenKind::DotDot | TokenKind::DotDotEq => Some((bp::RANGE, bp::RANGE + 1)),

            // Logical OR
            TokenKind::OrOr => Some((bp::OR, bp::OR + 1)),

            // Logical AND
            TokenKind::AndAnd => Some((bp::AND, bp::AND + 1)),

            // Comparison (non-associative, but we use left-assoc here)
            TokenKind::EqEq
            | TokenKind::Ne
            | TokenKind::Lt
            | TokenKind::Le
            | TokenKind::Gt
            | TokenKind::Ge => Some((bp::COMPARE, bp::COMPARE + 1)),

            // Bitwise OR
            TokenKind::Or => Some((bp::BIT_OR, bp::BIT_OR + 1)),

            // Bitwise XOR
            TokenKind::Caret => Some((bp::BIT_XOR, bp::BIT_XOR + 1)),

            // Bitwise AND
            TokenKind::And => Some((bp::BIT_AND, bp::BIT_AND + 1)),

            // Shift
            TokenKind::Shl | TokenKind::Shr => Some((bp::SHIFT, bp::SHIFT + 1)),

            // Pipe
            TokenKind::Pipe => Some((bp::PIPE, bp::PIPE + 1)),

            // Addition/Subtraction (and elementwise broadcasts, same level)
            TokenKind::Plus | TokenKind::Minus | TokenKind::DotPlus | TokenKind::DotMinus => {
                Some((bp::SUM, bp::SUM + 1))
            }

            // Multiplication/Division (and elementwise broadcasts, same level)
            TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::DotStar
            | TokenKind::DotSlash => Some((bp::PRODUCT, bp::PRODUCT + 1)),

            // Power (right-associative: (bp, bp) instead of the left-assoc
            // (bp, bp+1) convention). Binds tighter than product.
            TokenKind::StarStar => Some((bp::POWER, bp::POWER)),

            // Type cast
            TokenKind::Keyword(Keyword::As) => Some((bp::CAST, bp::CAST + 1)),

            _ => None,
        }
    }

    /// Try to parse an assignment operator.
    fn try_parse_assign_op(&mut self) -> Option<AssignOp> {
        let op = match self.current_kind() {
            TokenKind::Eq => AssignOp::Assign,
            TokenKind::PlusEq => AssignOp::AddAssign,
            TokenKind::MinusEq => AssignOp::SubAssign,
            TokenKind::StarEq => AssignOp::MulAssign,
            TokenKind::SlashEq => AssignOp::DivAssign,
            TokenKind::PercentEq => AssignOp::RemAssign,
            TokenKind::AndEq => AssignOp::BitAndAssign,
            TokenKind::OrEq => AssignOp::BitOrAssign,
            TokenKind::CaretEq => AssignOp::BitXorAssign,
            TokenKind::ShlEq => AssignOp::ShlAssign,
            TokenKind::ShrEq => AssignOp::ShrAssign,
            _ => return None,
        };
        self.advance();
        Some(op)
    }

    /// Parse a binary operator.
    fn parse_binary_op(&mut self) -> ParseResult<BinOp> {
        let op = match self.current_kind() {
            TokenKind::Plus => BinOp::Add,
            TokenKind::Minus => BinOp::Sub,
            TokenKind::Star => BinOp::Mul,
            TokenKind::StarStar => BinOp::Pow,
            TokenKind::Slash => BinOp::Div,
            TokenKind::Percent => BinOp::Rem,
            TokenKind::DotPlus => BinOp::DotAdd,
            TokenKind::DotMinus => BinOp::DotSub,
            TokenKind::DotStar => BinOp::DotMul,
            TokenKind::DotSlash => BinOp::DotDiv,
            TokenKind::And => BinOp::BitAnd,
            TokenKind::Or => BinOp::BitOr,
            TokenKind::Caret => BinOp::BitXor,
            TokenKind::Shl => BinOp::Shl,
            TokenKind::Shr => BinOp::Shr,
            TokenKind::AndAnd => BinOp::And,
            TokenKind::OrOr => BinOp::Or,
            TokenKind::EqEq => BinOp::Eq,
            TokenKind::Ne => BinOp::Ne,
            TokenKind::Lt => BinOp::Lt,
            TokenKind::Le => BinOp::Le,
            TokenKind::Gt => BinOp::Gt,
            TokenKind::Ge => BinOp::Ge,
            TokenKind::Pipe => BinOp::Pipe,
            _ => return Err(self.error_expected("binary operator")),
        };
        self.advance();
        Ok(op)
    }

    /// Check if the current token can begin an expression.
    fn can_begin_expr(&self) -> bool {
        matches!(
            self.current_kind(),
            TokenKind::Ident
                | TokenKind::RawIdent
                | TokenKind::Literal { .. }
                | TokenKind::OpenDelim(_)
                | TokenKind::Minus
                | TokenKind::Not
                | TokenKind::Star
                | TokenKind::StarStar
                | TokenKind::And
                | TokenKind::AndAnd
                | TokenKind::Or
                | TokenKind::OrOr
                | TokenKind::DotDot
                | TokenKind::DotDotEq
                | TokenKind::Keyword(
                    Keyword::If
                        | Keyword::Match
                        | Keyword::Loop
                        | Keyword::While
                        | Keyword::For
                        | Keyword::Return
                        | Keyword::Break
                        | Keyword::Continue
                        | Keyword::Move
                        | Keyword::Async
                        | Keyword::Unsafe
                        | Keyword::Handle
                        | Keyword::Resume
                        | Keyword::Perform
                        | Keyword::Self_
                        | Keyword::SelfType
                        | Keyword::Crate
                        | Keyword::Super
                )
                | TokenKind::DslBlock { .. }
        )
    }

    // =========================================================================
    // SPECIFIC EXPRESSION PARSERS
    // =========================================================================

    /// Convert a literal token to AST literal.
    pub(crate) fn convert_literal(
        &self,
        kind: &crate::lexer::LiteralKind,
        suffix: Option<&str>,
    ) -> ParseResult<Literal> {
        use crate::lexer::LiteralKind as LK;

        match kind {
            LK::Int { base, .. } => {
                let span = self.tokens[self.pos - 1].span;
                let text = self.source.slice(span);
                // Remove prefix and underscores
                let text = match base {
                    IntBase::Decimal => text.replace('_', ""),
                    IntBase::Hexadecimal => text[2..].replace('_', ""),
                    IntBase::Octal => text[2..].replace('_', ""),
                    IntBase::Binary => text[2..].replace('_', ""),
                };
                // Remove suffix
                let text = suffix
                    .map_or(text.as_str(), |s| &text[..text.len() - s.len()])
                    .to_string();
                let value = u128::from_str_radix(&text, base.radix()).unwrap_or(0);
                let int_suffix = suffix.and_then(IntSuffix::from_str);
                Ok(Literal::Int {
                    value,
                    suffix: int_suffix,
                    base: *base,
                })
            }
            LK::Float { .. } => {
                let span = self.tokens[self.pos - 1].span;
                let text = self.source.slice(span).replace('_', "");
                let text = suffix.map_or(text.as_str(), |s| &text[..text.len() - s.len()]);
                let value: f64 = text.parse().unwrap_or(0.0);
                let float_suffix = suffix.and_then(FloatSuffix::from_str);
                Ok(Literal::Float {
                    value,
                    suffix: float_suffix,
                })
            }
            LK::Char { .. } => {
                let span = self.tokens[self.pos - 1].span;
                let text = self.source.slice(span);
                let c = self.parse_char_content(&text[1..text.len() - 1])?;
                Ok(Literal::Char(c))
            }
            LK::Byte { .. } => {
                let span = self.tokens[self.pos - 1].span;
                let text = self.source.slice(span);
                let c = self.parse_char_content(&text[2..text.len() - 1])?;
                Ok(Literal::Byte(c as u8))
            }
            LK::Str { .. } => {
                let span = self.tokens[self.pos - 1].span;
                let text = self.source.slice(span);
                let content = self.parse_string_content(&text[1..text.len() - 1])?;
                Ok(Literal::Str {
                    value: content,
                    is_raw: false,
                })
            }
            LK::ByteStr { .. } => {
                let span = self.tokens[self.pos - 1].span;
                let text = self.source.slice(span);
                let content = self.parse_string_content(&text[2..text.len() - 1])?;
                Ok(Literal::ByteStr {
                    value: content.into_bytes(),
                    is_raw: false,
                })
            }
            LK::RawStr { .. } => {
                let span = self.tokens[self.pos - 1].span;
                let text = self.source.slice(span);
                // Find the actual content between r#..."..."#
                let start = text.find('"').unwrap() + 1;
                let end = text.rfind('"').unwrap();
                Ok(Literal::Str {
                    value: text[start..end].to_string(),
                    is_raw: true,
                })
            }
            LK::RawByteStr { .. } => {
                let span = self.tokens[self.pos - 1].span;
                let text = self.source.slice(span);
                let start = text.find('"').unwrap() + 1;
                let end = text.rfind('"').unwrap();
                Ok(Literal::ByteStr {
                    value: text[start..end].as_bytes().to_vec(),
                    is_raw: true,
                })
            }
            LK::Bool(b) => Ok(Literal::Bool(*b)),
            LK::CStr { .. } => {
                // Treat C strings like regular strings for now
                let span = self.tokens[self.pos - 1].span;
                let text = self.source.slice(span);
                let content = self.parse_string_content(&text[2..text.len() - 1])?;
                Ok(Literal::Str {
                    value: content,
                    is_raw: false,
                })
            }
            LK::FormatStr { .. } => {
                // Treat format strings like regular strings for now
                let span = self.tokens[self.pos - 1].span;
                let text = self.source.slice(span);
                // Remove f" prefix and " suffix
                let content = if text.starts_with("f\"") {
                    self.parse_string_content(&text[2..text.len() - 1])?
                } else {
                    text.to_string()
                };
                Ok(Literal::Str {
                    value: content,
                    is_raw: false,
                })
            }
        }
    }

    /// Parse escape sequences in a string.
    fn parse_string_content(&self, s: &str) -> ParseResult<String> {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => result.push('\n'),
                    Some('r') => result.push('\r'),
                    Some('t') => result.push('\t'),
                    Some('\\') => result.push('\\'),
                    Some('\'') => result.push('\''),
                    Some('"') => result.push('"'),
                    Some('0') => result.push('\0'),
                    Some('x') => {
                        let hi = chars.next().unwrap_or('0');
                        let lo = chars.next().unwrap_or('0');
                        let hex = format!("{}{}", hi, lo);
                        let val = u8::from_str_radix(&hex, 16).unwrap_or(0);
                        result.push(val as char);
                    }
                    Some('u') => {
                        chars.next(); // {
                        let mut hex = String::new();
                        while let Some(&c) = chars.peek() {
                            if c == '}' {
                                chars.next();
                                break;
                            }
                            hex.push(chars.next().unwrap());
                        }
                        let val = u32::from_str_radix(&hex, 16).unwrap_or(0);
                        if let Some(c) = char::from_u32(val) {
                            result.push(c);
                        }
                    }
                    Some(c) => result.push(c),
                    None => {}
                }
            } else {
                result.push(c);
            }
        }

        Ok(result)
    }

    /// Parse a character literal content.
    fn parse_char_content(&self, s: &str) -> ParseResult<char> {
        let content = self.parse_string_content(s)?;
        content
            .chars()
            .next()
            .ok_or_else(|| ParseError::new(ParseErrorKind::InvalidExpression, self.current_span()))
    }

    /// Parse path or struct expression.
    fn parse_path_or_struct_expr(&mut self) -> ParseResult<Expr> {
        let path = self.parse_path_in_expr()?;
        let start = path.span;

        // Check for struct literal
        if !self.restrictions.no_struct_literal
            && self.check(&TokenKind::OpenDelim(Delimiter::Brace))
        {
            return self.parse_struct_expr(path);
        }

        // Check for macro invocation
        if self.check(&TokenKind::Not) {
            self.advance();
            return self.parse_macro_expr(path, start);
        }

        // Simple path or identifier
        if path.is_simple() {
            let ident = path.last_ident().unwrap().clone();
            Ok(Expr::new(ExprKind::Ident(ident), start))
        } else {
            Ok(Expr::new(ExprKind::Path(path), start))
        }
    }

    /// Parse struct literal expression.
    fn parse_struct_expr(&mut self, path: Path) -> ParseResult<Expr> {
        let start = path.span;
        self.expect(&TokenKind::OpenDelim(Delimiter::Brace))?;

        let mut fields = Vec::new();
        let mut rest = None;

        while !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) && !self.is_eof() {
            // A field may carry outer attributes, e.g. `#[cfg(unix)] ino`.
            // Parse them before the `..rest` check so a bare `..` is still
            // recognized after any leading attributes.
            let attrs = self.parse_outer_attrs()?;

            // Check for ..rest
            if self.check(&TokenKind::DotDot) {
                self.advance();
                rest = Some(Box::new(self.parse_expr()?));
                break;
            }

            let field_start = self.current_span();
            let name = self.expect_ident()?;

            let value = if self.eat(&TokenKind::Colon) {
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            };

            let field_span = if let Some(ref v) = value {
                field_start.merge(&v.span)
            } else {
                name.span
            };

            fields.push(FieldExpr {
                name,
                value,
                attrs,
                span: field_span,
            });

            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        let end = self.expect(&TokenKind::CloseDelim(Delimiter::Brace))?.span;
        let span = start.merge(&end);

        Ok(Expr::new(ExprKind::Struct { path, fields, rest }, span))
    }

    /// Parse macro invocation expression.
    fn parse_macro_expr(&mut self, path: Path, start: crate::lexer::Span) -> ParseResult<Expr> {
        let (delimiter, tokens) = if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
            (
                Delimiter::Paren,
                self.parse_token_trees_until(Delimiter::Paren)?,
            )
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Bracket)) {
            (
                Delimiter::Bracket,
                self.parse_token_trees_until(Delimiter::Bracket)?,
            )
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            (
                Delimiter::Brace,
                self.parse_token_trees_until(Delimiter::Brace)?,
            )
        } else {
            return Err(self.error_expected("macro delimiter"));
        };

        let span = start.merge(&self.tokens[self.pos - 1].span);
        Ok(Expr::new(
            ExprKind::Macro {
                path,
                delimiter,
                tokens,
            },
            span,
        ))
    }

    /// Parse parenthesized expression (or tuple or unit).
    fn parse_paren_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect(&TokenKind::OpenDelim(Delimiter::Paren))?.span;

        // Unit: ()
        if self.check(&TokenKind::CloseDelim(Delimiter::Paren)) {
            let end = self.advance().span;
            return Ok(Expr::new(ExprKind::Tuple(Vec::new()), start.merge(&end)));
        }

        let first = self.parse_expr()?;

        // Check for tuple
        if self.check(&TokenKind::Comma) {
            self.advance();
            let mut elements = vec![first];

            while !self.check(&TokenKind::CloseDelim(Delimiter::Paren)) && !self.is_eof() {
                elements.push(self.parse_expr()?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }

            let end = self.expect(&TokenKind::CloseDelim(Delimiter::Paren))?.span;
            let span = start.merge(&end);
            return Ok(Expr::new(ExprKind::Tuple(elements), span));
        }

        // Parenthesized expression
        let end = self.expect(&TokenKind::CloseDelim(Delimiter::Paren))?.span;
        let span = start.merge(&end);
        Ok(Expr::new(ExprKind::Paren(Box::new(first)), span))
    }

    /// Parse array expression.
    fn parse_array_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect(&TokenKind::OpenDelim(Delimiter::Bracket))?.span;

        // Empty array
        if self.check(&TokenKind::CloseDelim(Delimiter::Bracket)) {
            let end = self.advance().span;
            return Ok(Expr::new(ExprKind::Array(Vec::new()), start.merge(&end)));
        }

        // An element may carry outer attributes, e.g. `#[cfg(windows)] path`.
        // `ExprKind::Array` holds bare expressions, so the attributes are
        // parsed and dropped rather than attached.
        let _ = self.parse_outer_attrs()?;
        let first = self.parse_expr()?;

        // Repeat: [expr; count]
        if self.check(&TokenKind::Semi) {
            self.advance();
            let count = self.parse_expr()?;
            let end = self
                .expect(&TokenKind::CloseDelim(Delimiter::Bracket))?
                .span;
            let span = start.merge(&end);
            return Ok(Expr::new(
                ExprKind::ArrayRepeat {
                    element: Box::new(first),
                    count: Box::new(count),
                },
                span,
            ));
        }

        // Normal array
        let mut elements = vec![first];

        if self.eat(&TokenKind::Comma) {
            while !self.check(&TokenKind::CloseDelim(Delimiter::Bracket)) && !self.is_eof() {
                let _ = self.parse_outer_attrs()?;
                elements.push(self.parse_expr()?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }

        let end = self
            .expect(&TokenKind::CloseDelim(Delimiter::Bracket))?
            .span;
        let span = start.merge(&end);
        Ok(Expr::new(ExprKind::Array(elements), span))
    }

    /// Parse if expression (including `if let`).
    fn parse_if_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect_keyword(Keyword::If)?;

        // Check for `if let Pattern = expr { ... }`
        if self.check_keyword(Keyword::Let) {
            return self.parse_if_let_expr(start);
        }

        // Don't allow struct literals in condition (ambiguous with block)
        let old_restrictions = self.restrictions;
        self.restrictions.no_struct_literal = true;
        let condition = self.parse_expr()?;
        self.restrictions = old_restrictions;

        let then_branch = self.parse_block()?;

        let else_branch = if self.eat_keyword(Keyword::Else) {
            if self.check_keyword(Keyword::If) {
                Some(Box::new(self.parse_if_expr()?))
            } else {
                let block = self.parse_block()?;
                Some(Box::new(Expr::new(
                    ExprKind::Block(Box::new(block.clone())),
                    block.span,
                )))
            }
        } else {
            None
        };

        let span = if let Some(ref e) = else_branch {
            start.merge(&e.span)
        } else {
            start.merge(&then_branch.span)
        };

        Ok(Expr::new(
            ExprKind::If {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch,
            },
            span,
        ))
    }

    /// Parse `if let Pattern = expr { ... } else { ... }`
    fn parse_if_let_expr(&mut self, start: crate::lexer::Span) -> ParseResult<Expr> {
        self.expect_keyword(Keyword::Let)?;
        let pattern = self.parse_pattern()?;
        self.expect(&TokenKind::Eq)?;

        let old_restrictions = self.restrictions;
        self.restrictions.no_struct_literal = true;
        let expr = self.parse_expr()?;
        self.restrictions = old_restrictions;

        let then_branch = self.parse_block()?;

        let else_branch = if self.eat_keyword(Keyword::Else) {
            if self.check_keyword(Keyword::If) {
                Some(Box::new(self.parse_if_expr()?))
            } else {
                let block = self.parse_block()?;
                Some(Box::new(Expr::new(
                    ExprKind::Block(Box::new(block.clone())),
                    block.span,
                )))
            }
        } else {
            None
        };

        let span = if let Some(ref e) = else_branch {
            start.merge(&e.span)
        } else {
            start.merge(&then_branch.span)
        };

        Ok(Expr::new(
            ExprKind::IfLet {
                pattern: Box::new(pattern),
                expr: Box::new(expr),
                then_branch: Box::new(then_branch),
                else_branch,
            },
            span,
        ))
    }

    /// Parse match expression.
    fn parse_match_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect_keyword(Keyword::Match)?;

        let old_restrictions = self.restrictions;
        self.restrictions.no_struct_literal = true;
        let scrutinee = self.parse_expr()?;
        self.restrictions = old_restrictions;

        self.expect(&TokenKind::OpenDelim(Delimiter::Brace))?;

        let mut arms = Vec::new();

        while !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) && !self.is_eof() {
            let attrs = self.parse_outer_attrs()?;
            let arm_start = self.current_span();

            let pattern = self.parse_pattern()?;

            let guard = if self.eat_keyword(Keyword::If) {
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            };

            self.expect(&TokenKind::FatArrow)?;

            // Parse the arm body under Rust's ExprWithBlock / ExprWithoutBlock
            // rule. Parse the prefix atom first, then classify it: a block-like
            // body (block, if, match, loop/while/for, unsafe/async/handle
            // block) ends at its closing brace and does NOT bind trailing
            // postfix/infix operators, so it cannot consume the following arm's
            // pattern -- its trailing comma is optional. A value body resumes
            // the ordinary Pratt loop so operators bind (`x + 1`, `f()?`, `a.b`)
            // and must be terminated by `,` or the closing `}`.
            let atom = self.parse_prefix_expr()?;
            let body_is_block = Self::is_block_like_expr(&atom);
            let body = if body_is_block {
                atom
            } else {
                self.continue_expr_with_bp(atom, 0)?
            };

            let arm_span = arm_start.merge(&body.span);

            arms.push(MatchArm {
                attrs,
                pattern,
                guard,
                body: Box::new(body),
                span: arm_span,
            });

            // Comma is required after a value body and optional after a
            // block-like body; a closing `}` ends the arm list in either case.
            if !self.eat(&TokenKind::Comma)
                && !self.check(&TokenKind::CloseDelim(Delimiter::Brace))
                && !body_is_block
            {
                return Err(self.error_expected("`,` or `}`"));
            }
        }

        let end = self.expect(&TokenKind::CloseDelim(Delimiter::Brace))?.span;
        let span = start.merge(&end);

        Ok(Expr::new(
            ExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms,
            },
            span,
        ))
    }

    /// Parse loop expression.
    fn parse_loop_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect_keyword(Keyword::Loop)?;
        let body = self.parse_block()?;
        let span = start.merge(&body.span);

        Ok(Expr::new(
            ExprKind::Loop {
                body: Box::new(body),
                label: None,
            },
            span,
        ))
    }

    /// Parse while expression.
    fn parse_while_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect_keyword(Keyword::While)?;

        // Check for while let
        if self.check_keyword(Keyword::Let) {
            return self.parse_while_let_expr(start);
        }

        let old_restrictions = self.restrictions;
        self.restrictions.no_struct_literal = true;
        let condition = self.parse_expr()?;
        self.restrictions = old_restrictions;

        let body = self.parse_block()?;
        let span = start.merge(&body.span);

        Ok(Expr::new(
            ExprKind::While {
                condition: Box::new(condition),
                body: Box::new(body),
                label: None,
            },
            span,
        ))
    }

    /// Parse while let expression.
    fn parse_while_let_expr(&mut self, start: crate::lexer::Span) -> ParseResult<Expr> {
        self.expect_keyword(Keyword::Let)?;
        let pattern = self.parse_pattern()?;
        self.expect(&TokenKind::Eq)?;

        let old_restrictions = self.restrictions;
        self.restrictions.no_struct_literal = true;
        let expr = self.parse_expr()?;
        self.restrictions = old_restrictions;

        let body = self.parse_block()?;
        let span = start.merge(&body.span);

        Ok(Expr::new(
            ExprKind::WhileLet {
                pattern: Box::new(pattern),
                expr: Box::new(expr),
                body: Box::new(body),
                label: None,
            },
            span,
        ))
    }

    /// Parse for expression.
    fn parse_for_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect_keyword(Keyword::For)?;
        let pattern = self.parse_pattern()?;
        self.expect_keyword(Keyword::In)?;

        let old_restrictions = self.restrictions;
        self.restrictions.no_struct_literal = true;
        let iter = self.parse_expr()?;
        self.restrictions = old_restrictions;

        let body = self.parse_block()?;
        let span = start.merge(&body.span);

        Ok(Expr::new(
            ExprKind::For {
                pattern: Box::new(pattern),
                iter: Box::new(iter),
                body: Box::new(body),
                label: None,
            },
            span,
        ))
    }

    /// Parse return expression.
    fn parse_return_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect_keyword(Keyword::Return)?;

        let value = if self.can_begin_expr() {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        let span = if let Some(ref v) = value {
            start.merge(&v.span)
        } else {
            start
        };

        Ok(Expr::new(ExprKind::Return(value), span))
    }

    /// Parse break expression.
    fn parse_break_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect_keyword(Keyword::Break)?;

        let label = if self.check_lifetime() {
            let lt = self.expect_lifetime()?;
            Some(lt.name)
        } else {
            None
        };

        let value = if self.can_begin_expr() {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        let span = if let Some(ref v) = value {
            start.merge(&v.span)
        } else if let Some(ref l) = label {
            start.merge(&l.span)
        } else {
            start
        };

        Ok(Expr::new(ExprKind::Break { label, value }, span))
    }

    /// Parse continue expression.
    fn parse_continue_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect_keyword(Keyword::Continue)?;

        let label = if self.check_lifetime() {
            let lt = self.expect_lifetime()?;
            Some(lt.name)
        } else {
            None
        };

        let span = if let Some(ref l) = label {
            start.merge(&l.span)
        } else {
            start
        };

        Ok(Expr::new(ExprKind::Continue { label }, span))
    }

    /// Parse closure expression.
    fn parse_closure_expr(&mut self, is_move: bool, is_async: bool) -> ParseResult<Expr> {
        let start = self.current_span();

        // `||` lexes as a single `OrOr` token, so a zero-parameter closure never
        // presents two separate `|`s. Consume it as an empty parameter list. This
        // covers bare `|| body`, `move || body`, and `async || body` uniformly,
        // including an explicit `-> T` return type.
        if self.eat(&TokenKind::OrOr) {
            let return_type = if self.eat(&TokenKind::Arrow) {
                Some(Box::new(self.parse_type()?))
            } else {
                None
            };
            return self.parse_closure_body(Vec::new(), return_type, start, is_move, is_async);
        }

        self.expect(&TokenKind::Or)?;

        let mut params = Vec::new();
        while !self.check(&TokenKind::Or) && !self.is_eof() {
            let param_start = self.current_span();
            // Use parse_pattern_primary (not parse_pattern) to avoid
            // consuming `|` as an or-pattern - in closures, `|` is the
            // parameter list delimiter, not a pattern combinator.
            let pattern = self.parse_pattern_primary()?;
            let ty = if self.eat(&TokenKind::Colon) {
                Some(Box::new(self.parse_type()?))
            } else {
                None
            };
            let param_span = if let Some(ref t) = ty {
                param_start.merge(&t.span)
            } else {
                pattern.span
            };
            params.push(ClosureParam {
                pattern,
                ty,
                span: param_span,
            });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        self.expect(&TokenKind::Or)?;

        let return_type = if self.eat(&TokenKind::Arrow) {
            Some(Box::new(self.parse_type()?))
        } else {
            None
        };

        self.parse_closure_body(params, return_type, start, is_move, is_async)
    }

    /// Parse closure body.
    fn parse_closure_body(
        &mut self,
        params: Vec<ClosureParam>,
        return_type: Option<Box<Type>>,
        start: crate::lexer::Span,
        is_move: bool,
        is_async: bool,
    ) -> ParseResult<Expr> {
        let body = self.parse_expr()?;
        let span = start.merge(&body.span);

        Ok(Expr::new(
            ExprKind::Closure {
                is_move,
                is_async,
                params,
                return_type,
                body: Box::new(body),
            },
            span,
        ))
    }

    // =========================================================================
    // EFFECT SYSTEM PARSERS
    // =========================================================================

    /// Parse a handle expression:
    ///
    /// ```build
    /// handle {
    ///     body_expression
    /// } with {
    ///     Effect.operation(params) => |resume| { handler_body },
    /// }
    /// ```
    fn parse_handle_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect_keyword(Keyword::Handle)?;

        // Parse the body block
        let body = self.parse_block()?;

        // Expect `with`
        self.expect_keyword(Keyword::With)?;

        // Parse the handler block: { clauses }
        self.expect(&TokenKind::OpenDelim(Delimiter::Brace))?;

        let mut handlers = Vec::new();
        let mut effect_path: Option<Path> = None;

        while !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) && !self.is_eof() {
            let clause_start = self.current_span();

            // Parse Effect.operation(params) => |resume| { body }
            // First, parse the effect name (identifier)
            let effect_name = self.expect_ident()?;

            // Set the effect path from the first clause if not yet set
            if effect_path.is_none() {
                effect_path = Some(Path::from_ident(effect_name.clone()));
            }

            // Expect `.`
            self.expect(&TokenKind::Dot)?;

            // Parse operation name
            let operation = self.expect_ident()?;

            // Parse optional parameter list
            let params = if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
                let (params, _) = self.parse_paren_comma_seq(|p| {
                    let pat_start = p.current_span();
                    let pattern = p.parse_pattern()?;
                    let ty = if p.eat(&TokenKind::Colon) {
                        Some(Box::new(p.parse_type()?))
                    } else {
                        None
                    };
                    let param_span = if let Some(ref t) = ty {
                        pat_start.merge(&t.span)
                    } else {
                        pattern.span
                    };
                    Ok(ClosureParam {
                        pattern,
                        ty,
                        span: param_span,
                    })
                })?;
                params
            } else {
                Vec::new()
            };

            // Expect `=>`
            self.expect(&TokenKind::FatArrow)?;

            // Parse handler body: `|resume_name| { body }` or just an expression.
            // The `|...|` part is a resume parameter - we parse it specially because
            // `resume` is a keyword and the normal closure parser rejects keywords
            // as parameter names.
            let handler_body = if self.check(&TokenKind::Or) {
                // Consume `|`
                self.advance();
                // Accept any identifier or keyword as the resume parameter name
                // (resume is a keyword, so we can't use expect_ident)
                self.advance();
                // Consume `|`
                self.expect(&TokenKind::Or)?;
                // Parse the body block
                self.parse_expr()?
            } else {
                self.parse_expr()?
            };

            let clause_span = clause_start.merge(&handler_body.span);

            handlers.push(EffectHandler {
                operation,
                params,
                body: Box::new(handler_body),
                span: clause_span,
            });

            // Comma is optional after block expression bodies (Rust convention)
            if !self.eat(&TokenKind::Comma) {
                if !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) {
                    // Check if this looks like the start of another arm - if so, allow missing comma
                    let looks_like_next_arm = self.check_ident()
                        || self.check(&TokenKind::Underscore)
                        || self.check(&TokenKind::Pound)
                        || self.check(&TokenKind::At)
                        || self.check_keyword(Keyword::Self_)
                        || self.check_keyword(Keyword::SelfType)
                        || self.check(&TokenKind::Or);
                    if !looks_like_next_arm {
                        return Err(self.error_expected("`,` or `}`"));
                    }
                }
            }
        }

        let end = self.expect(&TokenKind::CloseDelim(Delimiter::Brace))?.span;
        let span = start.merge(&end);

        // Use the effect path from the first handler clause, or a dummy path
        let effect = effect_path.unwrap_or_else(|| Path::from_ident(Ident::new("Unknown", start)));

        Ok(Expr::new(
            ExprKind::Handle {
                effect,
                handlers,
                body: Box::new(body),
            },
            span,
        ))
    }

    /// Parse a resume expression: `resume(value)` or `resume`
    fn parse_resume_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect_keyword(Keyword::Resume)?;

        // Check for `resume(value)` (call-like syntax)
        let value = if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
            self.advance();
            if self.check(&TokenKind::CloseDelim(Delimiter::Paren)) {
                // resume() => resume with unit
                let end = self.advance().span;
                return Ok(Expr::new(ExprKind::Resume(None), start.merge(&end)));
            }
            let val = self.parse_expr()?;
            self.expect(&TokenKind::CloseDelim(Delimiter::Paren))?;
            Some(Box::new(val))
        } else if self.can_begin_expr() {
            // resume value (without parens)
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        let span = if let Some(ref v) = value {
            start.merge(&v.span)
        } else {
            start
        };

        Ok(Expr::new(ExprKind::Resume(value), span))
    }

    /// Parse a perform expression: `perform Effect.operation(args)`
    fn parse_perform_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect_keyword(Keyword::Perform)?;

        // Parse Effect name
        let effect = self.expect_ident()?;

        // Expect `.`
        self.expect(&TokenKind::Dot)?;

        // Parse operation name
        let operation = self.expect_ident()?;

        // Parse arguments
        let (args, args_span) = self.parse_paren_comma_seq(|p| p.parse_expr())?;

        let span = start.merge(&args_span);

        Ok(Expr::new(
            ExprKind::Perform {
                effect,
                operation,
                args,
            },
            span,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{Lexer, SourceFile as LexerSourceFile};

    /// Parse a standalone expression by wrapping it in `fn test() { EXPR; }`.
    fn parse_expr_str(s: &str) -> ParseResult<Expr> {
        let source = LexerSourceFile::new("test.bld", format!("fn test() {{ {}; }}", s));
        let mut lexer = Lexer::new(&source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(&source, tokens);
        parser.advance(); // fn
        parser.advance(); // test
        parser.advance(); // (
        parser.advance(); // )
        parser.advance(); // {
        parser.parse_expr()
    }

    /// Parse a standalone type from its source text.
    fn parse_type_str(s: &str) -> ParseResult<Type> {
        let source = LexerSourceFile::new("test.bld", s.to_string());
        let mut lexer = Lexer::new(&source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(&source, tokens);
        parser.parse_type()
    }

    // =========================================================================
    // CONTEXTUAL KEYWORDS IN VALUE POSITION
    // =========================================================================

    #[test]
    fn value_context_keywords_parse_as_identifiers() {
        // `default`/`module`/`effect`/`auto` are keywords only in item
        // position; used as a plain value each is an ordinary identifier.
        // Their declaration forms are consumed by the item parser before an
        // expression is ever parsed, so here they must route through the same
        // path/identifier parser as any other name.
        for word in ["default", "module", "effect", "auto"] {
            let expr = parse_expr_str(word)
                .unwrap_or_else(|e| panic!("`{word}` should parse as a value expression: {e:?}"));
            match &expr.kind {
                ExprKind::Ident(id) => {
                    assert_eq!(id.as_str(), word, "identifier name should be preserved")
                }
                other => panic!("`{word}` parsed as {other:?}, expected ExprKind::Ident"),
            }
        }
    }

    #[test]
    fn path_root_keywords_parse_in_type_position() {
        // `super`/`crate`/`self`/`Self` are legal as the FIRST segment of a
        // qualified TYPE path (`super::T`, `crate::m::Foo`, `self::Bar`,
        // `Self::Assoc`). The expression parser has its own primary arms for
        // these roots, but the shared path parser rejected them, so a
        // `super::`-qualified return type, field type, or bound failed to
        // parse while its expression-position twin (`super::CONST`) did not.
        for (src, root) in [
            ("super::DecodeError", "super"),
            ("crate::codec::Frame", "crate"),
            ("self::Bar", "self"),
            ("Self::Assoc", "Self"),
        ] {
            let ty = parse_type_str(src)
                .unwrap_or_else(|e| panic!("`{src}` should parse as a type path: {e:?}"));
            match &ty.kind {
                TypeKind::Path(path) => assert_eq!(
                    path.segments.first().unwrap().ident.as_str(),
                    root,
                    "`{src}` first path segment should be `{root}`"
                ),
                other => panic!("`{src}` parsed as {other:?}, expected TypeKind::Path"),
            }
        }
    }

    // =========================================================================
    // ZERO-PARAMETER CLOSURES
    // =========================================================================

    #[test]
    fn zero_param_closures_parse_in_every_flavor() {
        // `||` is a single `OrOr` token, so the empty parameter list must be
        // accepted by the closure parser directly -- not only by the bare
        // prefix path. `move ||` and `async ||` route through
        // `parse_closure_expr`, and an explicit `-> T` must still parse.
        for src in [
            "|| 1",
            "move || 1",
            "async || 1",
            "|| -> i32 { 1 }",
            "move || -> i32 { 1 }",
        ] {
            let expr = parse_expr_str(src)
                .unwrap_or_else(|e| panic!("`{src}` should parse as a closure: {e:?}"));
            match &expr.kind {
                ExprKind::Closure { params, .. } => {
                    assert!(params.is_empty(), "`{src}` should have zero params")
                }
                other => panic!("`{src}` parsed as {other:?}, expected ExprKind::Closure"),
            }
        }
    }

    #[test]
    fn async_move_block_parses_as_an_async_block_not_a_closure() {
        // `async move { .. }` is an async block that captures by move, distinct
        // from a move closure. The block-vs-closure decision is made on the
        // token after an optional `move`, so `move` must be consumed before the
        // `{` check. Before the fix the async arm tested for `{` first and let
        // `async move { .. }` fall through to the closure parser, which failed
        // with `expected |, found {`.
        let block_cases = [("async { 1 }", false), ("async move { 1 }", true)];
        for (src, want_move) in block_cases {
            let expr = parse_expr_str(src)
                .unwrap_or_else(|e| panic!("`{src}` should parse as an async block: {e:?}"));
            match &expr.kind {
                ExprKind::Async { is_move, .. } => assert_eq!(
                    *is_move, want_move,
                    "`{src}` should have is_move={want_move}"
                ),
                other => panic!("`{src}` parsed as {other:?}, expected ExprKind::Async"),
            }
        }

        // The closure forms are unaffected: `async ||` and `async move ||` stay
        // async closures, with the move flag tracking the keyword.
        let closure_cases = [("async || 1", false), ("async move || 1", true)];
        for (src, want_move) in closure_cases {
            let expr = parse_expr_str(src)
                .unwrap_or_else(|e| panic!("`{src}` should parse as an async closure: {e:?}"));
            match &expr.kind {
                ExprKind::Closure {
                    is_async, is_move, ..
                } => {
                    assert!(*is_async, "`{src}` should be an async closure");
                    assert_eq!(*is_move, want_move, "`{src}` should have is_move={want_move}");
                }
                other => panic!("`{src}` parsed as {other:?}, expected ExprKind::Closure"),
            }
        }
    }

    #[test]
    fn outer_attributes_are_accepted_on_struct_fields_and_array_elements() {
        // `#[cfg(..)]` and friends may prefix a struct-literal field or an
        // array element. The struct field keeps its attributes; the array
        // element's are parsed and dropped (the AST holds bare expressions).
        let s = parse_expr_str("Foo { #[cfg(unix)] ino, path: p }")
            .unwrap_or_else(|e| panic!("attributed struct field should parse: {e:?}"));
        match &s.kind {
            ExprKind::Struct { fields, .. } => {
                assert_eq!(fields.len(), 2, "both fields present");
                assert_eq!(fields[0].attrs.len(), 1, "first field keeps its attribute");
                assert!(fields[1].attrs.is_empty(), "second field has no attribute");
            }
            other => panic!("parsed as {other:?}, expected ExprKind::Struct"),
        }

        let a = parse_expr_str("[#[cfg(windows)] a, b, #[cfg(unix)] c]")
            .unwrap_or_else(|e| panic!("attributed array element should parse: {e:?}"));
        match &a.kind {
            ExprKind::Array(elems) => assert_eq!(elems.len(), 3, "all three elements present"),
            other => panic!("parsed as {other:?}, expected ExprKind::Array"),
        }
    }

    // =========================================================================
    // MATCH-ARM BODIES: ExprWithBlock / ExprWithoutBlock comma rule
    // =========================================================================

    fn match_arms(expr: &Expr) -> &[MatchArm] {
        match &expr.kind {
            ExprKind::Match { arms, .. } => arms,
            other => panic!("expected a match expression, got {other:?}"),
        }
    }

    #[test]
    fn block_bodied_arm_does_not_swallow_next_tuple_pattern() {
        // A block-like body ends at its `}`; the arm that follows may begin
        // with `(` (a tuple/or pattern) and must NOT be consumed as a call on
        // the block. Before the ExprWithBlock rule the block body greedily
        // parsed `(a, _) | (_, a)` as `block(a, _) | (_, a)` and then failed at
        // the `=>` of that next arm. Reaching two arms proves they stayed
        // separate -- a merge would have been a parse error, not two arms.
        let src = "match (x, x) { \
            (a, b) if a != b => { a + b } \
            (a, _) | (_, a) => a \
        }";
        let expr = parse_expr_str(src)
            .unwrap_or_else(|e| panic!("comma-optional block arm should parse: {e:?}"));
        let arms = match_arms(&expr);
        assert_eq!(arms.len(), 2, "both arms must survive as separate arms");
        assert!(
            matches!(&arms[0].body.kind, ExprKind::Block(_)),
            "first arm body stays a block, not an over-consumed call expression"
        );
    }

    #[test]
    fn block_bodied_arm_allows_comma_less_literal_next_arm() {
        // The following arm may also start with a literal. The block body does
        // not bind it and no trailing comma is required after the block.
        let src = "match x { 0 => { 1 } 1 => 2, _ => 3 }";
        let expr = parse_expr_str(src)
            .unwrap_or_else(|e| panic!("comma-optional block arm should parse: {e:?}"));
        let arms = match_arms(&expr);
        assert_eq!(arms.len(), 3, "three arms, none merged");
    }

    #[test]
    fn value_bodied_arm_requires_a_trailing_comma() {
        // A value (ExprWithoutBlock) body must be terminated by `,` or `}`.
        // Omitting the comma between two value arms is a hard error; the parser
        // fails closed rather than silently accepting it, matching Rust.
        let src = "match x { 0 => 1 _ => 2 }";
        assert!(
            parse_expr_str(src).is_err(),
            "comma-less value arms must be rejected"
        );
    }

    #[test]
    fn block_like_arm_body_does_not_bind_a_trailing_operator() {
        // An `if/else` arm body is block-like: it terminates at the final `}`
        // and does not absorb a trailing `+ 1`. The leftover `+` then fails to
        // parse as the next arm's pattern, so the match is rejected rather than
        // silently re-associating the operator onto the if-expression.
        let src = "match x { _ => if x > 0 { 1 } else { 2 } + 1 }";
        assert!(
            parse_expr_str(src).is_err(),
            "a block-like body must not bind a trailing operator"
        );
    }

    // =========================================================================
    // OPERATOR PRECEDENCE (core Pratt parser correctness)
    // =========================================================================

    #[test]
    fn mul_binds_tighter_than_add() {
        // 1 + 2 * 3 => Add(1, Mul(2, 3))
        let expr = parse_expr_str("1 + 2 * 3").unwrap();
        match &expr.kind {
            ExprKind::Binary {
                op: BinOp::Add,
                left,
                right,
            } => {
                assert!(matches!(
                    &left.kind,
                    ExprKind::Literal(Literal::Int { value: 1, .. })
                ));
                assert!(matches!(
                    &right.kind,
                    ExprKind::Binary { op: BinOp::Mul, .. }
                ));
            }
            other => panic!("expected Add(1, Mul(2,3)), got {:?}", other),
        }
    }

    #[test]
    fn add_is_left_associative() {
        // 1 + 2 + 3 => Add(Add(1, 2), 3)
        let expr = parse_expr_str("1 + 2 + 3").unwrap();
        match &expr.kind {
            ExprKind::Binary {
                op: BinOp::Add,
                left,
                right,
            } => {
                assert!(matches!(
                    &left.kind,
                    ExprKind::Binary { op: BinOp::Add, .. }
                ));
                assert!(matches!(
                    &right.kind,
                    ExprKind::Literal(Literal::Int { value: 3, .. })
                ));
            }
            other => panic!("expected Add(Add(1,2), 3), got {:?}", other),
        }
    }

    #[test]
    fn comparison_binds_looser_than_arithmetic() {
        // a + b == c * d => Eq(Add(a,b), Mul(c,d))
        let expr = parse_expr_str("a + b == c * d").unwrap();
        match &expr.kind {
            ExprKind::Binary {
                op: BinOp::Eq,
                left,
                right,
            } => {
                assert!(matches!(
                    &left.kind,
                    ExprKind::Binary { op: BinOp::Add, .. }
                ));
                assert!(matches!(
                    &right.kind,
                    ExprKind::Binary { op: BinOp::Mul, .. }
                ));
            }
            other => panic!("expected Eq(Add(_,_), Mul(_,_)), got {:?}", other),
        }
    }

    #[test]
    fn logical_and_binds_tighter_than_or() {
        // a || b && c => Or(a, And(b, c))
        let expr = parse_expr_str("a || b && c").unwrap();
        match &expr.kind {
            ExprKind::Binary {
                op: BinOp::Or,
                left,
                right,
            } => {
                assert!(matches!(&left.kind, ExprKind::Ident(_) | ExprKind::Path(_)));
                assert!(matches!(
                    &right.kind,
                    ExprKind::Binary { op: BinOp::And, .. }
                ));
            }
            other => panic!("expected Or(a, And(b,c)), got {:?}", other),
        }
    }

    #[test]
    fn unary_minus_binds_tighter_than_binary() {
        // -a + b => Add(Neg(a), b)
        let expr = parse_expr_str("-a + b").unwrap();
        match &expr.kind {
            ExprKind::Binary {
                op: BinOp::Add,
                left,
                right,
            } => {
                assert!(matches!(
                    &left.kind,
                    ExprKind::Unary {
                        op: UnaryOp::Neg,
                        ..
                    }
                ));
                assert!(matches!(
                    &right.kind,
                    ExprKind::Ident(_) | ExprKind::Path(_)
                ));
            }
            other => panic!("expected Add(Neg(a), b), got {:?}", other),
        }
    }

    #[test]
    fn method_call_chains_left() {
        // a.b().c() => MethodCall(MethodCall(a, b, []), c, [])
        let expr = parse_expr_str("a.b().c()").unwrap();
        match &expr.kind {
            ExprKind::MethodCall {
                receiver, method, ..
            } => {
                assert_eq!(method.as_str(), "c");
                assert!(
                    matches!(&receiver.kind, ExprKind::MethodCall { method: inner_m, .. } if inner_m.as_str() == "b")
                );
            }
            other => panic!("expected MethodCall(MethodCall(a,b),c), got {:?}", other),
        }
    }

    #[test]
    fn index_postfix_binds_tight() {
        // a[0].b => Field(Index(a, 0), b)
        let expr = parse_expr_str("a[0].b").unwrap();
        match &expr.kind {
            ExprKind::Field { expr: inner, field } => {
                assert_eq!(field.as_str(), "b");
                assert!(matches!(&inner.kind, ExprKind::Index { .. }));
            }
            other => panic!("expected Field(Index(a,0), b), got {:?}", other),
        }
    }

    #[test]
    fn assignment_is_right_associative() {
        // a = b = c => Assign(a, Assign(b, c))
        let expr = parse_expr_str("a = b = c").unwrap();
        match &expr.kind {
            ExprKind::Assign {
                op: AssignOp::Assign,
                target,
                value,
            } => {
                assert!(matches!(
                    &target.kind,
                    ExprKind::Ident(_) | ExprKind::Path(_)
                ));
                assert!(matches!(
                    &value.kind,
                    ExprKind::Assign {
                        op: AssignOp::Assign,
                        ..
                    }
                ));
            }
            other => panic!("expected Assign(a, Assign(b, c)), got {:?}", other),
        }
    }

    #[test]
    fn sub_is_left_associative() {
        // 10 - 3 - 2 => Sub(Sub(10, 3), 2)
        let expr = parse_expr_str("10 - 3 - 2").unwrap();
        match &expr.kind {
            ExprKind::Binary {
                op: BinOp::Sub,
                left,
                right,
            } => {
                assert!(matches!(
                    &left.kind,
                    ExprKind::Binary { op: BinOp::Sub, .. }
                ));
                assert!(matches!(
                    &right.kind,
                    ExprKind::Literal(Literal::Int { value: 2, .. })
                ));
            }
            other => panic!("expected Sub(Sub(10,3), 2), got {:?}", other),
        }
    }

    #[test]
    fn mixed_mul_div_left_associative() {
        // 6 * 2 / 3 => Div(Mul(6, 2), 3)
        let expr = parse_expr_str("6 * 2 / 3").unwrap();
        match &expr.kind {
            ExprKind::Binary {
                op: BinOp::Div,
                left,
                right,
            } => {
                assert!(matches!(
                    &left.kind,
                    ExprKind::Binary { op: BinOp::Mul, .. }
                ));
                assert!(matches!(
                    &right.kind,
                    ExprKind::Literal(Literal::Int { value: 3, .. })
                ));
            }
            other => panic!("expected Div(Mul(6,2), 3), got {:?}", other),
        }
    }

    #[test]
    fn bitwise_and_binds_tighter_than_bitwise_or() {
        // a | b & c => BitOr(a, BitAnd(b, c))
        let expr = parse_expr_str("a | b & c").unwrap();
        match &expr.kind {
            ExprKind::Binary {
                op: BinOp::BitOr,
                left,
                right,
            } => {
                assert!(matches!(&left.kind, ExprKind::Ident(_) | ExprKind::Path(_)));
                assert!(matches!(
                    &right.kind,
                    ExprKind::Binary {
                        op: BinOp::BitAnd,
                        ..
                    }
                ));
            }
            other => panic!("expected BitOr(a, BitAnd(b,c)), got {:?}", other),
        }
    }

    // =========================================================================
    // EXPRESSION FORMS
    // =========================================================================

    #[test]
    fn if_else_parses() {
        let expr = parse_expr_str("if x { 1 } else { 2 }").unwrap();
        match &expr.kind {
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                assert!(matches!(
                    &condition.kind,
                    ExprKind::Ident(_) | ExprKind::Path(_)
                ));
                assert!(!then_branch.stmts.is_empty() || then_branch.stmts.is_empty());
                assert!(else_branch.is_some());
            }
            other => panic!("expected If {{ .. }} else {{ .. }}, got {:?}", other),
        }
    }

    #[test]
    fn closure_parses() {
        let expr = parse_expr_str("|x, y| x + y").unwrap();
        match &expr.kind {
            ExprKind::Closure { params, body, .. } => {
                assert_eq!(params.len(), 2);
                assert!(matches!(
                    &body.kind,
                    ExprKind::Binary { op: BinOp::Add, .. }
                ));
            }
            other => panic!("expected Closure, got {:?}", other),
        }
    }

    #[test]
    fn nested_function_call() {
        // f(g(x), h(y, z))
        let expr = parse_expr_str("f(g(x), h(y, z))").unwrap();
        match &expr.kind {
            ExprKind::Call { func, args } => {
                assert!(matches!(&func.kind, ExprKind::Ident(_) | ExprKind::Path(_)));
                assert_eq!(args.len(), 2);
                // First arg: g(x) - a Call
                assert!(matches!(&args[0].kind, ExprKind::Call { .. }));
                // Second arg: h(y, z) - a Call with 2 args
                match &args[1].kind {
                    ExprKind::Call {
                        args: inner_args, ..
                    } => assert_eq!(inner_args.len(), 2),
                    other => panic!("expected Call for h(y,z), got {:?}", other),
                }
            }
            other => panic!(
                "expected Call(f, [Call(g,..), Call(h,..)]), got {:?}",
                other
            ),
        }
    }

    #[test]
    fn array_literal() {
        let expr = parse_expr_str("[1, 2, 3]").unwrap();
        match &expr.kind {
            ExprKind::Array(elems) => assert_eq!(elems.len(), 3),
            other => panic!("expected Array with 3 elements, got {:?}", other),
        }
    }

    #[test]
    fn range_expression() {
        let expr = parse_expr_str("0..10").unwrap();
        match &expr.kind {
            ExprKind::Range {
                start,
                end,
                inclusive,
            } => {
                assert!(start.is_some());
                assert!(end.is_some());
                assert!(!inclusive);
            }
            other => panic!("expected Range(0, 10, exclusive), got {:?}", other),
        }
    }

    #[test]
    fn unary_not() {
        let expr = parse_expr_str("!flag").unwrap();
        match &expr.kind {
            ExprKind::Unary {
                op: UnaryOp::Not,
                expr: inner,
            } => {
                assert!(matches!(
                    &inner.kind,
                    ExprKind::Ident(_) | ExprKind::Path(_)
                ));
            }
            other => panic!("expected Not(flag), got {:?}", other),
        }
    }

    #[test]
    fn parenthesized_overrides_precedence() {
        // (1 + 2) * 3 => Mul(Paren(Add(1,2)), 3) or Mul(Add(1,2), 3)
        let expr = parse_expr_str("(1 + 2) * 3").unwrap();
        match &expr.kind {
            ExprKind::Binary {
                op: BinOp::Mul,
                left,
                right,
            } => {
                // The LHS is the parenthesized add (may be wrapped in Paren)
                let inner = match &left.kind {
                    ExprKind::Paren(inner) => &inner.kind,
                    other => other,
                };
                assert!(matches!(inner, ExprKind::Binary { op: BinOp::Add, .. }));
                assert!(matches!(
                    &right.kind,
                    ExprKind::Literal(Literal::Int { value: 3, .. })
                ));
            }
            other => panic!("expected Mul(Add(1,2), 3), got {:?}", other),
        }
    }
}
