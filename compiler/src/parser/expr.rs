// ===============================================================================
// QUANTALANG PARSER - EXPRESSION PARSING (PRATT)
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Pratt expression parser.
//!
//! This module implements a Pratt parser (top-down operator precedence) for
//! parsing expressions. Pratt parsing handles operator precedence and
//! associativity elegantly through binding power.

use crate::ast::*;
use crate::lexer::{Delimiter, Keyword, TokenKind, IntBase};
use super::{Parser, ParseResult, ParseError, ParseErrorKind};

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
    /// Postfix operators (highest)
    pub const POSTFIX: u8 = 15;
}

impl<'a> Parser<'a> {
    /// Parse an expression.
    pub fn parse_expr(&mut self) -> ParseResult<Expr> {
        self.parse_expr_with_bp(0)
    }

    /// Parse an expression with minimum binding power.
    fn parse_expr_with_bp(&mut self, min_bp: u8) -> ParseResult<Expr> {
        // Parse prefix (atoms and unary operators)
        let mut lhs = self.parse_prefix_expr()?;

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

            TokenKind::Ident | TokenKind::RawIdent => {
                self.parse_path_or_struct_expr()
            }

            TokenKind::Keyword(Keyword::Self_) => {
                self.advance();
                let ident = Ident::new("self", start);
                Ok(Expr::new(ExprKind::Ident(ident), start))
            }

            TokenKind::Keyword(Keyword::SelfType) => {
                self.advance();
                let path = Path::from_ident(Ident::new("Self", start));
                Ok(Expr::new(ExprKind::Path(path), start))
            }

            TokenKind::Keyword(Keyword::Crate) => {
                self.advance();
                let path = Path::from_ident(Ident::new("crate", start));
                Ok(Expr::new(ExprKind::Path(path), start))
            }

            TokenKind::Keyword(Keyword::Super) => {
                self.advance();
                let path = Path::from_ident(Ident::new("super", start));
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

            TokenKind::OpenDelim(Delimiter::Paren) => {
                self.parse_paren_expr()
            }

            // =====================================================================
            // ARRAY
            // =====================================================================

            TokenKind::OpenDelim(Delimiter::Bracket) => {
                self.parse_array_expr()
            }

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

            TokenKind::Keyword(Keyword::Handle) => self.parse_handle_expr(),
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

            TokenKind::Or => self.parse_closure_expr(false, false),
            TokenKind::OrOr => {
                // || - closure with no params
                self.advance();
                self.parse_closure_body(Vec::new(), None, start, false, false)
            }

            TokenKind::Keyword(Keyword::Move) => {
                self.advance();
                self.parse_closure_expr(true, false)
            }

            TokenKind::Keyword(Keyword::Async) => {
                self.advance();
                if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
                    // async { ... }
                    let is_move = self.eat_keyword(Keyword::Move);
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
                    // async || or async move ||
                    let is_move = self.eat_keyword(Keyword::Move);
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
                let end = self.expect(&TokenKind::CloseDelim(Delimiter::Bracket))?.span;
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
                if let TokenKind::Literal { kind: crate::lexer::LiteralKind::Int { .. }, .. } = self.current_kind() {
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

                // Check for method call
                if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
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
            let end = if self.can_begin_expr() && !matches!(self.current_kind(), TokenKind::CloseDelim(_) | TokenKind::Comma | TokenKind::Semi) {
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
            TokenKind::PlusEq | TokenKind::MinusEq | TokenKind::StarEq
            | TokenKind::SlashEq | TokenKind::PercentEq | TokenKind::CaretEq
            | TokenKind::AndEq | TokenKind::OrEq | TokenKind::ShlEq | TokenKind::ShrEq => {
                Some((bp::ASSIGN, bp::ASSIGN))
            }

            // Range
            TokenKind::DotDot | TokenKind::DotDotEq => Some((bp::RANGE, bp::RANGE + 1)),

            // Logical OR
            TokenKind::OrOr => Some((bp::OR, bp::OR + 1)),

            // Logical AND
            TokenKind::AndAnd => Some((bp::AND, bp::AND + 1)),

            // Comparison (non-associative, but we use left-assoc here)
            TokenKind::EqEq | TokenKind::Ne | TokenKind::Lt | TokenKind::Le
            | TokenKind::Gt | TokenKind::Ge => Some((bp::COMPARE, bp::COMPARE + 1)),

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

            // Addition/Subtraction
            TokenKind::Plus | TokenKind::Minus => Some((bp::SUM, bp::SUM + 1)),

            // Multiplication/Division
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => {
                Some((bp::PRODUCT, bp::PRODUCT + 1))
            }

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
            TokenKind::Slash => BinOp::Div,
            TokenKind::Percent => BinOp::Rem,
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
                let text = suffix.map_or(text.as_str(), |s| &text[..text.len() - s.len()]).to_string();
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
        content.chars().next().ok_or_else(|| {
            ParseError::new(ParseErrorKind::InvalidExpression, self.current_span())
        })
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
                attrs: Vec::new(),
                span: field_span,
            });

            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        let end = self.expect(&TokenKind::CloseDelim(Delimiter::Brace))?.span;
        let span = start.merge(&end);

        Ok(Expr::new(
            ExprKind::Struct { path, fields, rest },
            span,
        ))
    }

    /// Parse macro invocation expression.
    fn parse_macro_expr(&mut self, path: Path, start: crate::lexer::Span) -> ParseResult<Expr> {
        let (delimiter, tokens) = if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
            (Delimiter::Paren, self.parse_token_trees_until(Delimiter::Paren)?)
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Bracket)) {
            (Delimiter::Bracket, self.parse_token_trees_until(Delimiter::Bracket)?)
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            (Delimiter::Brace, self.parse_token_trees_until(Delimiter::Brace)?)
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

        let first = self.parse_expr()?;

        // Repeat: [expr; count]
        if self.check(&TokenKind::Semi) {
            self.advance();
            let count = self.parse_expr()?;
            let end = self.expect(&TokenKind::CloseDelim(Delimiter::Bracket))?.span;
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
                elements.push(self.parse_expr()?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }

        let end = self.expect(&TokenKind::CloseDelim(Delimiter::Bracket))?.span;
        let span = start.merge(&end);
        Ok(Expr::new(ExprKind::Array(elements), span))
    }

    /// Parse if expression.
    fn parse_if_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect_keyword(Keyword::If)?;

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
            let body = self.parse_expr()?;

            let arm_span = arm_start.merge(&body.span);

            arms.push(MatchArm {
                attrs,
                pattern,
                guard,
                body: Box::new(body),
                span: arm_span,
            });

            // Comma is optional before closing brace
            if !self.eat(&TokenKind::Comma) {
                if !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) {
                    return Err(self.error_expected("`,` or `}`"));
                }
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

        self.expect(&TokenKind::Or)?;

        let mut params = Vec::new();
        while !self.check(&TokenKind::Or) && !self.is_eof() {
            let param_start = self.current_span();
            let pattern = self.parse_pattern()?;
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
    /// ```quanta
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
                    Ok(ClosureParam { pattern, ty, span: param_span })
                })?;
                params
            } else {
                Vec::new()
            };

            // Expect `=>`
            self.expect(&TokenKind::FatArrow)?;

            // Parse handler body: `|resume_name| { body }` or just an expression.
            // The `|...|` part is a resume parameter — we parse it specially because
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

            // Comma is optional before closing brace
            if !self.eat(&TokenKind::Comma) {
                if !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) {
                    return Err(self.error_expected("`,` or `}`"));
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
                return Ok(Expr::new(
                    ExprKind::Resume(None),
                    start.merge(&end),
                ));
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
