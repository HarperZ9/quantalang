// ===============================================================================
// QUANTALANG PARSER - TYPE PARSING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Type parsing.
//!
//! This module handles parsing of all type expressions in QuantaLang.

use crate::ast::*;
use crate::lexer::{Delimiter, Keyword, TokenKind};
use super::{Parser, ParseResult, ParseError, ParseErrorKind};

impl<'a> Parser<'a> {
    /// Parse a type.
    pub fn parse_type(&mut self) -> ParseResult<Type> {
        self.parse_type_with_bounds(true)
    }

    /// Parse a type, optionally allowing bounds.
    fn parse_type_with_bounds(&mut self, allow_bounds: bool) -> ParseResult<Type> {
        let start = self.current_span();

        // Parse the primary type
        let mut ty = self.parse_type_primary()?;

        // Parse type bounds if allowed (for dyn Trait + ...)
        if allow_bounds && self.check(&TokenKind::Plus) {
            if let TypeKind::TraitObject { ref mut bounds, .. } = ty.kind {
                while self.eat(&TokenKind::Plus) {
                    let bound = self.parse_type_bound()?;
                    bounds.push(bound);
                }
                ty.span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
            }
        }

        // Parse `with` annotations: `Type with ColorSpace<Linear>`
        // This creates a WithEffect AST node wrapping the base type.
        // Only consume one annotation to avoid ambiguity with parameter lists.
        if self.eat_keyword(Keyword::With) {
            let mut effects = Vec::new();
            let effect_path = self.parse_path()?;
            effects.push(effect_path);
            // Only continue with comma-separated annotations if the next
            // token after comma is NOT an identifier followed by colon
            // (which would indicate the next function parameter).
            while self.check(&TokenKind::Comma) {
                // Peek ahead: if comma is followed by ident + colon, it's a
                // parameter separator, not another annotation.
                if self.pos + 2 < self.tokens.len() {
                    let after_comma = &self.tokens[self.pos + 1].kind;
                    let after_that = &self.tokens[self.pos + 2].kind;
                    if matches!(after_comma, TokenKind::Ident) && matches!(after_that, TokenKind::Colon) {
                        break; // Next item is a parameter, not an annotation
                    }
                }
                self.advance(); // consume comma
                let next_effect = self.parse_path()?;
                effects.push(next_effect);
            }
            let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
            ty = Type::new(TypeKind::WithEffect { ty: Box::new(ty), effects }, span);
        }

        Ok(ty)
    }

    /// Parse a primary type (without trailing bounds).
    fn parse_type_primary(&mut self) -> ParseResult<Type> {
        let start = self.current_span();

        match self.current_kind().clone() {
            // =================================================================
            // NEVER TYPE: !
            // =================================================================
            TokenKind::Not => {
                self.advance();
                Ok(Type::new(TypeKind::Never, start))
            }

            // =================================================================
            // INFER TYPE: _
            // =================================================================
            TokenKind::Underscore => {
                self.advance();
                Ok(Type::new(TypeKind::Infer, start))
            }

            // =================================================================
            // TUPLE / PARENTHESIZED / UNIT
            // =================================================================
            TokenKind::OpenDelim(Delimiter::Paren) => {
                self.parse_tuple_or_paren_type()
            }

            // =================================================================
            // ARRAY / SLICE: [T] or [T; N]
            // =================================================================
            TokenKind::OpenDelim(Delimiter::Bracket) => {
                self.parse_array_or_slice_type()
            }

            // =================================================================
            // REFERENCE: &T, &mut T, &'a T
            // =================================================================
            TokenKind::And => {
                self.parse_ref_type(false)
            }

            TokenKind::AndAnd => {
                // &&T is &&T (double reference)
                self.advance();
                let inner = self.parse_type()?;
                let inner_span = start.merge(&inner.span);
                let inner_ref = Type::new(
                    TypeKind::Ref {
                        lifetime: None,
                        mutability: Mutability::Immutable,
                        ty: Box::new(inner),
                    },
                    inner_span,
                );
                Ok(Type::new(
                    TypeKind::Ref {
                        lifetime: None,
                        mutability: Mutability::Immutable,
                        ty: Box::new(inner_ref),
                    },
                    inner_span,
                ))
            }

            // =================================================================
            // POINTER: *const T, *mut T
            // =================================================================
            TokenKind::Star => {
                self.parse_ptr_type()
            }

            // =================================================================
            // FN TYPE: fn(T) -> U
            // =================================================================
            TokenKind::Keyword(Keyword::Fn) => {
                self.parse_bare_fn_type(false, false)
            }

            TokenKind::Keyword(Keyword::Unsafe) => {
                self.advance();
                if self.check_keyword(Keyword::Extern) || self.check_keyword(Keyword::Fn) {
                    self.parse_bare_fn_type(true, false)
                } else {
                    Err(self.error_expected("`fn` or `extern`"))
                }
            }

            TokenKind::Keyword(Keyword::Extern) => {
                self.advance();
                let _abi = if let TokenKind::Literal { .. } = self.current_kind() {
                    let token_span = self.advance().span;
                    Some(self.source.slice(token_span).trim_matches('"').to_string())
                } else {
                    None
                };
                self.parse_bare_fn_type(false, true)
            }

            // =================================================================
            // IMPL TRAIT: impl Trait
            // =================================================================
            TokenKind::Keyword(Keyword::Impl) => {
                self.advance();
                let bounds = self.parse_type_bounds()?;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Type::new(TypeKind::ImplTrait { bounds }, span))
            }

            // =================================================================
            // DYN TRAIT: dyn Trait
            // =================================================================
            TokenKind::Keyword(Keyword::Dyn) => {
                self.advance();
                let bounds = self.parse_type_bounds()?;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Type::new(
                    TypeKind::TraitObject {
                        bounds,
                        lifetime: None,
                    },
                    span,
                ))
            }

            // =================================================================
            // SELF TYPE
            // =================================================================
            TokenKind::Keyword(Keyword::SelfType) => {
                self.advance();
                let path = Path::from_ident(Ident::new("Self", start));
                Ok(Type::new(TypeKind::Path(path), start))
            }

            // =================================================================
            // FN TRAITS: Fn, FnMut, FnOnce
            // =================================================================
            TokenKind::Keyword(Keyword::Fn) => {
                self.parse_fn_trait_type(FnTraitKind::Fn)
            }

            // =================================================================
            // PATH TYPES (including primitives)
            // =================================================================
            TokenKind::Ident | TokenKind::RawIdent | TokenKind::ColonColon
            | TokenKind::Keyword(Keyword::Crate | Keyword::Super | Keyword::Self_) => {
                let path = self.parse_path()?;
                let span = path.span;
                // Primitive types like i32, f64 are parsed as path types
                Ok(Type::new(TypeKind::Path(path), span))
            }

            // =================================================================
            // LIFETIME (for bare lifetime in bounds)
            // =================================================================
            TokenKind::Lifetime => {
                let lifetime = self.expect_lifetime()?;
                let span = lifetime.span;
                // This is typically used in bounds context
                Err(ParseError::new(ParseErrorKind::ExpectedType, span))
            }

            // =================================================================
            // MACRO INVOCATION
            // =================================================================
            TokenKind::Pound => {
                // Could be an attribute, but we don't expect attributes here
                Err(self.error_expected("type"))
            }

            // =================================================================
            // ERROR
            // =================================================================
            _ => Err(self.error_expected("type")),
        }
    }

    /// Parse reference type: &T, &mut T, &'a T, &'a mut T
    fn parse_ref_type(&mut self, _is_double: bool) -> ParseResult<Type> {
        let start = self.expect(&TokenKind::And)?.span;

        let lifetime = if self.check_lifetime() {
            Some(self.expect_lifetime()?)
        } else {
            None
        };

        let mutability = if self.eat_keyword(Keyword::Mut) {
            Mutability::Mutable
        } else {
            Mutability::Immutable
        };

        let ty = self.parse_type()?;
        let span = start.merge(&ty.span);

        Ok(Type::new(
            TypeKind::Ref {
                lifetime,
                mutability,
                ty: Box::new(ty),
            },
            span,
        ))
    }

    /// Parse pointer type: *const T, *mut T
    fn parse_ptr_type(&mut self) -> ParseResult<Type> {
        let start = self.expect(&TokenKind::Star)?.span;

        let mutability = if self.eat_keyword(Keyword::Mut) {
            Mutability::Mutable
        } else if self.eat_keyword(Keyword::Const) {
            Mutability::Immutable
        } else {
            return Err(self.error_expected("`const` or `mut`"));
        };

        let ty = self.parse_type()?;
        let span = start.merge(&ty.span);

        Ok(Type::new(
            TypeKind::Ptr {
                mutability,
                ty: Box::new(ty),
            },
            span,
        ))
    }

    /// Parse tuple or parenthesized type.
    fn parse_tuple_or_paren_type(&mut self) -> ParseResult<Type> {
        let start = self.expect(&TokenKind::OpenDelim(Delimiter::Paren))?.span;

        // Unit type: ()
        if self.check(&TokenKind::CloseDelim(Delimiter::Paren)) {
            let end = self.advance().span;
            return Ok(Type::new(TypeKind::Tuple(Vec::new()), start.merge(&end)));
        }

        let first = self.parse_type()?;

        // Check for tuple (needs comma)
        if self.check(&TokenKind::Comma) {
            self.advance();
            let mut elements = vec![first];

            while !self.check(&TokenKind::CloseDelim(Delimiter::Paren)) && !self.is_eof() {
                elements.push(self.parse_type()?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }

            let end = self.expect(&TokenKind::CloseDelim(Delimiter::Paren))?.span;
            return Ok(Type::new(TypeKind::Tuple(elements), start.merge(&end)));
        }

        // Parenthesized type
        let end = self.expect(&TokenKind::CloseDelim(Delimiter::Paren))?.span;
        let span = start.merge(&end);

        // Just return the inner type with updated span
        Ok(Type::new(first.kind, span))
    }

    /// Parse array or slice type: [T] or [T; N]
    fn parse_array_or_slice_type(&mut self) -> ParseResult<Type> {
        let start = self.expect(&TokenKind::OpenDelim(Delimiter::Bracket))?.span;

        let elem_ty = self.parse_type()?;

        if self.eat(&TokenKind::Semi) {
            // Array: [T; N]
            let len = self.parse_expr()?;
            let end = self.expect(&TokenKind::CloseDelim(Delimiter::Bracket))?.span;
            let span = start.merge(&end);

            Ok(Type::new(
                TypeKind::Array {
                    elem: Box::new(elem_ty),
                    len: Box::new(len),
                },
                span,
            ))
        } else {
            // Slice: [T]
            let end = self.expect(&TokenKind::CloseDelim(Delimiter::Bracket))?.span;
            let span = start.merge(&end);

            Ok(Type::new(TypeKind::Slice(Box::new(elem_ty)), span))
        }
    }

    /// Parse bare function type: fn(T, U) -> V
    fn parse_bare_fn_type(&mut self, is_unsafe: bool, has_extern: bool) -> ParseResult<Type> {
        let start = self.current_span();

        let abi = if has_extern && !self.check_keyword(Keyword::Fn) {
            if let TokenKind::Literal { .. } = self.current_kind() {
                let token_span = self.advance().span;
                Some(self.source.slice(token_span).trim_matches('"').to_string())
            } else {
                Some("C".to_string())
            }
        } else if !has_extern && self.eat_keyword(Keyword::Extern) {
            if let TokenKind::Literal { .. } = self.current_kind() {
                let token_span = self.advance().span;
                Some(self.source.slice(token_span).trim_matches('"').to_string())
            } else {
                Some("C".to_string())
            }
        } else {
            None
        };

        self.expect_keyword(Keyword::Fn)?;

        let (params, _) = self.parse_paren_comma_seq(|p| p.parse_bare_fn_param())?;

        let return_ty = if self.eat(&TokenKind::Arrow) {
            Some(Box::new(self.parse_type()?))
        } else {
            None
        };

        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);

        Ok(Type::new(
            TypeKind::BareFn {
                is_unsafe,
                is_extern: abi.is_some(),
                abi,
                params,
                return_ty,
                is_variadic: false,
            },
            span,
        ))
    }

    /// Parse a bare function parameter.
    fn parse_bare_fn_param(&mut self) -> ParseResult<BareFnParam> {
        let start = self.current_span();

        // Check for named parameter: name: Type
        let name = if self.check_ident() && matches!(self.peek().kind, TokenKind::Colon) {
            let ident = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            Some(ident)
        } else {
            None
        };

        let ty = self.parse_type()?;
        let span = start.merge(&ty.span);

        Ok(BareFnParam {
            name,
            ty: Box::new(ty),
            span,
        })
    }

    /// Parse Fn trait type: Fn(T) -> U
    fn parse_fn_trait_type(&mut self, kind: FnTraitKind) -> ParseResult<Type> {
        let start = self.current_span();
        self.advance(); // Skip Fn/FnMut/FnOnce keyword

        let (params, _) = self.parse_paren_comma_seq(|p| p.parse_type())?;

        let return_ty = if self.eat(&TokenKind::Arrow) {
            Some(Box::new(self.parse_type()?))
        } else {
            None
        };

        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);

        Ok(Type::new(
            TypeKind::FnTrait {
                kind,
                params,
                return_ty,
            },
            span,
        ))
    }

    /// Parse a type bound.
    fn parse_type_bound(&mut self) -> ParseResult<TypeBound> {
        let is_maybe = self.eat(&TokenKind::Question);

        // Handle lifetime bounds
        if self.check_lifetime() {
            let lifetime = self.expect_lifetime()?;
            // Convert lifetime to a type bound - this is a bit of a hack
            let path = Path::from_ident(lifetime.name);
            return Ok(TypeBound {
                path,
                is_maybe,
                span: lifetime.span,
            });
        }

        let path = self.parse_path()?;
        let span = path.span;

        Ok(TypeBound { path, is_maybe, span })
    }
}
