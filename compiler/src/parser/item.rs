// ===============================================================================
// BUILDLANG PARSER - ITEM PARSING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! Item (top-level declaration) parsing.
//!
//! This module handles parsing of all top-level declarations:
//! - Functions
//! - Structs, Enums, Unions
//! - Traits, Impls
//! - Type aliases
//! - Constants, Statics
//! - Modules
//! - Use declarations
//! - Extern blocks
//! - Macros
//! - Effects (BuildLang extension)

use super::{ParseError, ParseErrorKind, ParseResult, Parser};
use crate::ast::*;
use crate::lexer::{Delimiter, Keyword, TokenKind};

impl<'a> Parser<'a> {
    /// Parse an item.
    pub fn parse_item(&mut self) -> ParseResult<Item> {
        let attrs = self.parse_outer_attrs()?;
        let vis = self.parse_visibility()?;
        let start = self.current_span();

        self.parse_item_kind(attrs, vis, start)
    }

    /// Parse the kind of item after visibility.
    fn parse_item_kind(
        &mut self,
        attrs: Vec<Attribute>,
        vis: Visibility,
        start: crate::lexer::Span,
    ) -> ParseResult<Item> {
        // Handle modifiers
        // Only consume `default` if followed by fn/impl/type (not when used as identifier)
        let _is_default = if self.check_keyword(Keyword::Default) {
            let next = &self.peek().kind;
            let is_modifier = matches!(
                next,
                TokenKind::Keyword(Keyword::Fn)
                    | TokenKind::Keyword(Keyword::Unsafe)
                    | TokenKind::Keyword(Keyword::Async)
                    | TokenKind::Keyword(Keyword::Const)
                    | TokenKind::Keyword(Keyword::Impl)
                    | TokenKind::Keyword(Keyword::Type)
            );
            if is_modifier {
                self.eat_keyword(Keyword::Default)
            } else {
                false
            }
        } else {
            false
        };
        let is_unsafe = self.eat_keyword(Keyword::Unsafe);
        let is_async = self.eat_keyword(Keyword::Async);
        let is_const = self.eat_keyword(Keyword::Const);

        match self.current_kind().clone() {
            // =================================================================
            // FUNCTION
            // =================================================================
            TokenKind::Keyword(Keyword::Fn) => {
                let fn_def = self.parse_fn(is_unsafe, is_async, is_const)?;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Item::new(
                    ItemKind::Function(Box::new(fn_def)),
                    vis,
                    attrs,
                    span,
                ))
            }

            // =================================================================
            // STRUCT
            // =================================================================
            TokenKind::Keyword(Keyword::Struct) => {
                let struct_def = self.parse_struct()?;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Item::new(
                    ItemKind::Struct(Box::new(struct_def)),
                    vis,
                    attrs,
                    span,
                ))
            }

            // =================================================================
            // ENUM
            // =================================================================
            TokenKind::Keyword(Keyword::Enum) => {
                let enum_def = self.parse_enum()?;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Item::new(
                    ItemKind::Enum(Box::new(enum_def)),
                    vis,
                    attrs,
                    span,
                ))
            }

            // =================================================================
            // TRAIT
            // =================================================================
            TokenKind::Keyword(Keyword::Trait) => {
                let trait_def = self.parse_trait(is_unsafe)?;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Item::new(
                    ItemKind::Trait(Box::new(trait_def)),
                    vis,
                    attrs,
                    span,
                ))
            }

            TokenKind::Keyword(Keyword::Auto) => {
                self.advance();
                self.expect_keyword(Keyword::Trait)?;
                let mut trait_def = self.parse_trait_inner(is_unsafe)?;
                trait_def.is_auto = true;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Item::new(
                    ItemKind::Trait(Box::new(trait_def)),
                    vis,
                    attrs,
                    span,
                ))
            }

            // =================================================================
            // IMPL
            // =================================================================
            TokenKind::Keyword(Keyword::Impl) => {
                let impl_def = self.parse_impl(is_unsafe)?;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Item::new(
                    ItemKind::Impl(Box::new(impl_def)),
                    vis,
                    attrs,
                    span,
                ))
            }

            // =================================================================
            // TYPE ALIAS
            // =================================================================
            TokenKind::Keyword(Keyword::Type) => {
                let type_alias = self.parse_type_alias()?;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Item::new(
                    ItemKind::TypeAlias(Box::new(type_alias)),
                    vis,
                    attrs,
                    span,
                ))
            }

            // =================================================================
            // CONST (if not already consumed as modifier)
            // =================================================================
            TokenKind::Keyword(Keyword::Const) if !is_const => {
                let const_def = self.parse_const()?;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Item::new(
                    ItemKind::Const(Box::new(const_def)),
                    vis,
                    attrs,
                    span,
                ))
            }

            // =================================================================
            // STATIC
            // =================================================================
            TokenKind::Keyword(Keyword::Static) => {
                let static_def = self.parse_static()?;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Item::new(
                    ItemKind::Static(Box::new(static_def)),
                    vis,
                    attrs,
                    span,
                ))
            }

            // =================================================================
            // MODULE
            // =================================================================
            TokenKind::Keyword(Keyword::Mod) => {
                let mod_def = self.parse_mod(is_unsafe)?;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Item::new(
                    ItemKind::Mod(Box::new(mod_def)),
                    vis,
                    attrs,
                    span,
                ))
            }

            // =================================================================
            // MODULE DECLARATION (BuildLang ecosystem: `module std::math`)
            // =================================================================
            TokenKind::Keyword(Keyword::Module) => {
                let mod_def = self.parse_module_decl(is_unsafe)?;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Item::new(
                    ItemKind::Mod(Box::new(mod_def)),
                    vis,
                    attrs,
                    span,
                ))
            }

            // =================================================================
            // USE
            // =================================================================
            TokenKind::Keyword(Keyword::Use) => {
                let use_def = self.parse_use()?;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Item::new(
                    ItemKind::Use(Box::new(use_def)),
                    vis,
                    attrs,
                    span,
                ))
            }

            // =================================================================
            // EXTERN
            // =================================================================
            TokenKind::Keyword(Keyword::Extern) => {
                self.advance();
                if self.check_keyword(Keyword::Crate) {
                    let extern_crate = self.parse_extern_crate()?;
                    let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                    Ok(Item::new(
                        ItemKind::ExternCrate(Box::new(extern_crate)),
                        vis,
                        attrs,
                        span,
                    ))
                } else {
                    // Optional ABI string after `extern`, consumed here so we can
                    // tell `extern "C" fn ...` (a C-ABI function definition, i.e.
                    // an export) apart from `extern "C" { ... }` (an extern block).
                    let abi = if let TokenKind::Literal { .. } = self.current_kind() {
                        let token_span = self.advance().span;
                        Some(self.source.slice(token_span).trim_matches('"').to_string())
                    } else {
                        None
                    };

                    if self.check_keyword(Keyword::Fn) {
                        let mut fn_def = self.parse_fn(is_unsafe, false, false)?;
                        fn_def.sig.abi = abi;
                        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                        Ok(Item::new(
                            ItemKind::Function(Box::new(fn_def)),
                            vis,
                            attrs,
                            span,
                        ))
                    } else {
                        let extern_block = self.parse_extern_block(is_unsafe, abi)?;
                        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                        Ok(Item::new(
                            ItemKind::ExternBlock(Box::new(extern_block)),
                            vis,
                            attrs,
                            span,
                        ))
                    }
                }
            }

            // =================================================================
            // MACRO RULES
            // =================================================================
            TokenKind::Keyword(Keyword::Macro) => {
                self.advance();
                if self.check_ident() && self.source.slice(self.current().span) == "rules" {
                    self.advance();
                    self.expect(&TokenKind::Not)?;
                    let macro_rules = self.parse_macro_rules()?;
                    let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                    Ok(Item::new(
                        ItemKind::MacroRules(Box::new(macro_rules)),
                        vis,
                        attrs,
                        span,
                    ))
                } else {
                    let macro_def = self.parse_macro_def()?;
                    let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                    Ok(Item::new(
                        ItemKind::Macro(Box::new(macro_def)),
                        vis,
                        attrs,
                        span,
                    ))
                }
            }

            // =================================================================
            // EFFECT (BuildLang extension)
            // =================================================================
            TokenKind::Keyword(Keyword::Effect) => {
                let effect_def = self.parse_effect()?;
                let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                Ok(Item::new(
                    ItemKind::Effect(Box::new(effect_def)),
                    vis,
                    attrs,
                    span,
                ))
            }

            // =================================================================
            // ERROR
            // =================================================================
            _ => {
                if is_const {
                    // const fn or const item
                    if self.check_keyword(Keyword::Fn) {
                        let fn_def = self.parse_fn(is_unsafe, is_async, true)?;
                        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                        return Ok(Item::new(
                            ItemKind::Function(Box::new(fn_def)),
                            vis,
                            attrs,
                            span,
                        ));
                    }
                    let const_def = self.parse_const_inner()?;
                    let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                    Ok(Item::new(
                        ItemKind::Const(Box::new(const_def)),
                        vis,
                        attrs,
                        span,
                    ))
                } else {
                    Err(self.error_expected("item"))
                }
            }
        }
    }

    // =========================================================================
    // FUNCTION
    // =========================================================================

    /// Parse a function definition.
    fn parse_fn(&mut self, is_unsafe: bool, is_async: bool, is_const: bool) -> ParseResult<FnDef> {
        self.expect_keyword(Keyword::Fn)?;

        let name = self.expect_ident()?;
        let generics = self.parse_generics()?;

        // Parse the parameter list, detecting a trailing C-style `...` variadic
        // marker (e.g. `printf(fmt: &str, ...)`). The `...` must be the last
        // entry and is not itself a parameter.
        self.expect(&TokenKind::OpenDelim(Delimiter::Paren))?;
        let mut params = Vec::new();
        let mut is_variadic = false;
        while !self.check(&TokenKind::CloseDelim(Delimiter::Paren)) {
            if self.check(&TokenKind::DotDotDot) {
                self.advance(); // consume `...`
                is_variadic = true;
                break;
            }
            params.push(self.parse_fn_param()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::CloseDelim(Delimiter::Paren))?;

        let return_ty = if self.eat(&TokenKind::Arrow) {
            Some(Box::new(self.parse_type()?))
        } else {
            None
        };

        // Parse effects annotation
        let effects = if self.eat(&TokenKind::Tilde) {
            let mut effects = Vec::new();
            loop {
                effects.push(self.parse_path()?);
                if !self.eat(&TokenKind::Plus) {
                    break;
                }
            }
            effects
        } else {
            Vec::new()
        };

        // Parse where clause if not already parsed
        let generics = if generics.where_clause.is_none() && self.check_keyword(Keyword::Where) {
            Generics {
                params: generics.params,
                where_clause: Some(self.parse_where_clause()?),
                span: generics.span,
            }
        } else {
            generics
        };

        let body = if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            Some(Box::new(self.parse_block()?))
        } else {
            self.expect(&TokenKind::Semi)?;
            None
        };

        let abi = None; // TODO: Handle extern fn

        Ok(FnDef {
            name,
            generics,
            sig: FnSig {
                is_unsafe,
                is_async,
                is_const,
                abi,
                params,
                is_variadic,
                return_ty,
                effects,
            },
            body,
        })
    }

    /// Parse a function parameter.
    fn parse_fn_param(&mut self) -> ParseResult<Param> {
        let attrs = self.parse_outer_attrs()?;
        let start = self.current_span();

        // Check for self parameter variants: self, &self, &mut self, mut self
        if let Some(param) = self.try_parse_self_param(&attrs, start)? {
            return Ok(param);
        }

        let pattern = self.parse_pattern()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.parse_type()?;

        let default = if self.eat(&TokenKind::Eq) {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);

        Ok(Param {
            attrs,
            pattern,
            ty: Box::new(ty),
            default,
            span,
        })
    }

    /// Try to parse a self parameter (self, &self, &mut self, mut self).
    /// Returns None if the current position doesn't start a self parameter.
    fn try_parse_self_param(
        &mut self,
        attrs: &[Attribute],
        start: Span,
    ) -> ParseResult<Option<Param>> {
        use crate::ast::{Mutability, Path, PathSegment, Pattern, PatternKind, Type, TypeKind};

        // Check for a reference self receiver: `&self`, `&'a self`,
        // `&mut self`, or `&'a mut self`. Look past the `&`, an optional
        // lifetime, and an optional `mut` to confirm `self` before consuming
        // anything; otherwise this is an ordinary reference-typed parameter
        // (`&Frustum`, `&'a T`) and we fall through to the pattern parser.
        if self.check(&TokenKind::And) {
            let mut offset = 1;
            let has_lifetime = self.peek_n(offset).kind == TokenKind::Lifetime;
            if has_lifetime {
                offset += 1;
            }
            let has_mut = self.peek_n(offset).kind == TokenKind::Keyword(Keyword::Mut);
            if has_mut {
                offset += 1;
            }

            if self.peek_n(offset).kind == TokenKind::Keyword(Keyword::Self_) {
                self.advance(); // consume &

                // An explicit receiver lifetime (`&'a self`); the corpus pairs
                // it with a `<'a>` generic on the method.
                let lifetime = if has_lifetime {
                    Some(self.expect_lifetime()?)
                } else {
                    None
                };

                let mutability = if has_mut {
                    self.advance(); // consume mut
                    Mutability::Mutable
                } else {
                    Mutability::Immutable
                };

                let self_span = self.current_span();
                self.advance(); // consume self

                let self_ident = Ident::new("self", self_span);
                let pattern = Pattern::new(
                    PatternKind::Ident {
                        mutability,
                        name: self_ident,
                        subpattern: None,
                    },
                    self_span,
                );

                // Type is `&Self` / `&'a Self` / `&mut Self` / `&'a mut Self`.
                let self_type_path = Path {
                    segments: vec![PathSegment {
                        ident: Ident::new("Self", self_span),
                        generics: vec![],
                    }],
                    span: self_span,
                };
                let inner_ty = Type {
                    kind: TypeKind::Path(self_type_path),
                    span: self_span,
                    id: NodeId::DUMMY,
                };
                let ty = Type {
                    kind: TypeKind::Ref {
                        lifetime,
                        mutability,
                        ty: Box::new(inner_ty),
                    },
                    span: start.merge(&self_span),
                    id: NodeId::DUMMY,
                };

                return Ok(Some(Param {
                    attrs: attrs.to_vec(),
                    pattern,
                    ty: Box::new(ty),
                    default: None,
                    span: start.merge(&self_span),
                }));
            }
        }

        // Check for mut self or mut self: Type
        if self.check_keyword(Keyword::Mut)
            && self.peek().kind == TokenKind::Keyword(Keyword::Self_)
        {
            self.advance(); // consume mut
            let self_span = self.current_span();
            self.advance(); // consume self

            let self_ident = Ident::new("self", self_span);
            let pattern = Pattern::new(
                PatternKind::Ident {
                    mutability: Mutability::Mutable,
                    name: self_ident,
                    subpattern: None,
                },
                self_span,
            );

            // Check for explicit type: `mut self: Self`, `mut self: &mut Self`
            let ty = if self.eat(&TokenKind::Colon) {
                self.parse_type()?
            } else {
                let self_type_path = Path {
                    segments: vec![PathSegment {
                        ident: Ident::new("Self", self_span),
                        generics: vec![],
                    }],
                    span: self_span,
                };
                Type {
                    kind: TypeKind::Path(self_type_path),
                    span: self_span,
                    id: NodeId::DUMMY,
                }
            };

            let end_span = self.tokens[self.pos.saturating_sub(1)].span;
            return Ok(Some(Param {
                attrs: attrs.to_vec(),
                pattern,
                ty: Box::new(ty),
                default: None,
                span: start.merge(&end_span),
            }));
        }

        // Check for plain self or self: ExplicitType
        if self.check_keyword(Keyword::Self_) {
            let self_span = self.current_span();
            self.advance(); // consume self

            let self_ident = Ident::new("self", self_span);
            let pattern = Pattern::new(
                PatternKind::Ident {
                    mutability: Mutability::Immutable,
                    name: self_ident,
                    subpattern: None,
                },
                self_span,
            );

            // Check for explicit type annotation: `self: &Self`, `self: Self`, etc.
            let ty = if self.eat(&TokenKind::Colon) {
                // Explicit self type - parse the type
                self.parse_type()?
            } else {
                // Implicit self type - default to Self
                let self_type_path = Path {
                    segments: vec![PathSegment {
                        ident: Ident::new("Self", self_span),
                        generics: vec![],
                    }],
                    span: self_span,
                };
                Type {
                    kind: TypeKind::Path(self_type_path),
                    span: self_span,
                    id: NodeId::DUMMY,
                }
            };

            let end_span = self.tokens[self.pos.saturating_sub(1)].span;
            return Ok(Some(Param {
                attrs: attrs.to_vec(),
                pattern,
                ty: Box::new(ty),
                default: None,
                span: start.merge(&end_span),
            }));
        }

        Ok(None)
    }

    // =========================================================================
    // STRUCT
    // =========================================================================

    /// Parse a struct definition.
    fn parse_struct(&mut self) -> ParseResult<StructDef> {
        self.expect_keyword(Keyword::Struct)?;

        let name = self.expect_ident()?;
        let generics = self.parse_generics()?;

        let fields = if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            // Named fields
            let (fields, _) = self.parse_brace_comma_seq(|p| p.parse_struct_field())?;
            StructFields::Named(fields)
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
            // Tuple struct
            let (fields, _) = self.parse_paren_comma_seq(|p| p.parse_tuple_field())?;
            self.expect(&TokenKind::Semi)?;
            StructFields::Tuple(fields)
        } else {
            // Unit struct
            self.expect(&TokenKind::Semi)?;
            StructFields::Unit
        };

        Ok(StructDef {
            name,
            generics,
            fields,
        })
    }

    /// Parse a struct field.
    fn parse_struct_field(&mut self) -> ParseResult<FieldDef> {
        let attrs = self.parse_outer_attrs()?;
        let vis = self.parse_visibility()?;
        let start = self.current_span();

        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.parse_type()?;

        let default = if self.eat(&TokenKind::Eq) {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);

        Ok(FieldDef {
            vis,
            attrs,
            name,
            ty: Box::new(ty),
            default,
            span,
        })
    }

    /// Parse a tuple struct field.
    fn parse_tuple_field(&mut self) -> ParseResult<TupleFieldDef> {
        let attrs = self.parse_outer_attrs()?;
        let vis = self.parse_visibility()?;
        let start = self.current_span();

        let ty = self.parse_type()?;
        let span = start.merge(&ty.span);

        Ok(TupleFieldDef {
            vis,
            attrs,
            ty: Box::new(ty),
            span,
        })
    }

    // =========================================================================
    // ENUM
    // =========================================================================

    /// Parse an enum definition.
    fn parse_enum(&mut self) -> ParseResult<EnumDef> {
        self.expect_keyword(Keyword::Enum)?;

        let name = self.expect_ident()?;
        let generics = self.parse_generics()?;

        let (variants, _) = self.parse_brace_comma_seq(|p| p.parse_enum_variant())?;

        Ok(EnumDef {
            name,
            generics,
            variants,
        })
    }

    /// Parse an enum variant.
    fn parse_enum_variant(&mut self) -> ParseResult<EnumVariant> {
        let attrs = self.parse_outer_attrs()?;
        let start = self.current_span();

        let name = self.expect_ident()?;

        let fields = if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            let (fields, _) = self.parse_brace_comma_seq(|p| p.parse_struct_field())?;
            StructFields::Named(fields)
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
            let (fields, _) = self.parse_paren_comma_seq(|p| p.parse_tuple_field())?;
            StructFields::Tuple(fields)
        } else {
            StructFields::Unit
        };

        let discriminant = if self.eat(&TokenKind::Eq) {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);

        Ok(EnumVariant {
            attrs,
            name,
            fields,
            discriminant,
            span,
        })
    }

    // =========================================================================
    // TRAIT
    // =========================================================================

    /// Parse a trait definition.
    fn parse_trait(&mut self, is_unsafe: bool) -> ParseResult<TraitDef> {
        self.expect_keyword(Keyword::Trait)?;
        self.parse_trait_inner(is_unsafe)
    }

    /// Parse trait definition after `trait` keyword.
    fn parse_trait_inner(&mut self, is_unsafe: bool) -> ParseResult<TraitDef> {
        let name = self.expect_ident()?;
        let generics = self.parse_generics()?;

        let supertraits = if self.eat(&TokenKind::Colon) {
            self.parse_type_bounds()?
        } else {
            Vec::new()
        };

        // Parse where clause
        let generics = if generics.where_clause.is_none() && self.check_keyword(Keyword::Where) {
            Generics {
                params: generics.params,
                where_clause: Some(self.parse_where_clause()?),
                span: generics.span,
            }
        } else {
            generics
        };

        self.expect(&TokenKind::OpenDelim(Delimiter::Brace))?;

        let mut items = Vec::new();
        while !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) && !self.is_eof() {
            items.push(self.parse_trait_item()?);
        }

        self.expect(&TokenKind::CloseDelim(Delimiter::Brace))?;

        Ok(TraitDef {
            name,
            is_unsafe,
            is_auto: false,
            generics,
            supertraits,
            items,
        })
    }

    /// Parse a trait item.
    fn parse_trait_item(&mut self) -> ParseResult<TraitItem> {
        let attrs = self.parse_outer_attrs()?;
        let start = self.current_span();

        // Only consume `const` as a modifier when it is followed by `fn`
        // (`const fn foo();`). A bare `const NAME: TYPE;` begins an associated
        // const declaration and must be left for the `Const` arm below; eating
        // it here unconditionally left that arm unreachable, so every associated
        // const in a trait failed with "expected trait item". Mirrors the impl
        // item parser.
        let is_const = if self.check_keyword(Keyword::Const) {
            if matches!(&self.peek().kind, TokenKind::Keyword(Keyword::Fn)) {
                self.eat_keyword(Keyword::Const)
            } else {
                false
            }
        } else {
            false
        };
        let is_async = self.eat_keyword(Keyword::Async);
        let is_unsafe = self.eat_keyword(Keyword::Unsafe);

        let kind = match self.current_kind().clone() {
            TokenKind::Keyword(Keyword::Fn) => {
                let fn_def = self.parse_fn(is_unsafe, is_async, is_const)?;
                TraitItemKind::Function(Box::new(fn_def))
            }

            TokenKind::Keyword(Keyword::Type) => {
                self.advance();
                let name = self.expect_ident()?;
                let generics = self.parse_generics()?;

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

                self.expect(&TokenKind::Semi)?;

                TraitItemKind::Type {
                    name,
                    generics,
                    bounds,
                    default,
                }
            }

            TokenKind::Keyword(Keyword::Const) if !is_const => {
                self.advance();
                let name = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let ty = self.parse_type()?;

                let default = if self.eat(&TokenKind::Eq) {
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };

                self.expect(&TokenKind::Semi)?;

                TraitItemKind::Const {
                    name,
                    ty: Box::new(ty),
                    default,
                }
            }

            _ => return Err(self.error_expected("trait item")),
        };

        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);

        Ok(TraitItem {
            attrs,
            kind,
            span,
            id: NodeId::DUMMY,
        })
    }

    // =========================================================================
    // IMPL
    // =========================================================================

    /// Parse an impl block.
    fn parse_impl(&mut self, is_unsafe: bool) -> ParseResult<ImplDef> {
        self.expect_keyword(Keyword::Impl)?;

        let generics = self.parse_generics()?;

        // Check for negative impl
        let is_negative = self.eat(&TokenKind::Not);

        // Parse the type (or trait + for + type)
        let ty = self.parse_type()?;

        let (trait_ref, self_ty) = if self.eat_keyword(Keyword::For) {
            // This is `impl Trait for Type`
            let trait_path = match &ty.kind {
                TypeKind::Path(p) => p.clone(),
                _ => return Err(ParseError::new(ParseErrorKind::InvalidType, ty.span)),
            };
            let trait_ref = TraitRef {
                path: trait_path,
                is_negative,
            };
            let self_ty = self.parse_type()?;
            (Some(trait_ref), Box::new(self_ty))
        } else {
            // This is `impl Type`
            (None, Box::new(ty))
        };

        // Parse where clause
        let generics = if generics.where_clause.is_none() && self.check_keyword(Keyword::Where) {
            Generics {
                params: generics.params,
                where_clause: Some(self.parse_where_clause()?),
                span: generics.span,
            }
        } else {
            generics
        };

        self.expect(&TokenKind::OpenDelim(Delimiter::Brace))?;

        let mut items = Vec::new();
        while !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) && !self.is_eof() {
            items.push(self.parse_impl_item()?);
        }

        self.expect(&TokenKind::CloseDelim(Delimiter::Brace))?;

        Ok(ImplDef {
            is_unsafe,
            is_negative,
            generics,
            trait_ref,
            self_ty,
            items,
        })
    }

    /// Parse an impl item.
    fn parse_impl_item(&mut self) -> ParseResult<ImplItem> {
        let attrs = self.parse_outer_attrs()?;
        let vis = self.parse_visibility()?;
        let start = self.current_span();

        let is_default = if self.check_keyword(Keyword::Default) {
            let next = &self.peek().kind;
            if matches!(
                next,
                TokenKind::Keyword(Keyword::Fn) | TokenKind::Keyword(Keyword::Type)
            ) {
                self.eat_keyword(Keyword::Default)
            } else {
                false
            }
        } else {
            false
        };

        // Only consume const as modifier if followed by fn (const fn)
        let is_const = if self.check_keyword(Keyword::Const) {
            let next = &self.peek().kind;
            if matches!(next, TokenKind::Keyword(Keyword::Fn)) {
                self.eat_keyword(Keyword::Const)
            } else {
                false
            }
        } else {
            false
        };

        let is_async = self.eat_keyword(Keyword::Async);
        let is_unsafe = self.eat_keyword(Keyword::Unsafe);

        // `extern "ABI"` on a method mirrors the free-item form (see the
        // `Keyword::Extern` arm in `parse_item`): consume the keyword and the
        // optional ABI string so `extern "stdcall" fn ...` inside an impl body
        // parses the same as at the top level. Only a function may follow;
        // `extern crate` / `extern { ... }` are free-item forms, not impl items.
        let abi = if self.check_keyword(Keyword::Extern) {
            self.advance();
            let abi = if let TokenKind::Literal { .. } = self.current_kind() {
                let token_span = self.advance().span;
                Some(self.source.slice(token_span).trim_matches('"').to_string())
            } else {
                None
            };
            if !self.check_keyword(Keyword::Fn) {
                return Err(self.error_expected("`fn` after `extern` in impl item"));
            }
            abi
        } else {
            None
        };

        let kind = match self.current_kind().clone() {
            TokenKind::Keyword(Keyword::Fn) => {
                let mut fn_def = self.parse_fn(is_unsafe, is_async, is_const)?;
                fn_def.sig.abi = abi;
                ImplItemKind::Function(Box::new(fn_def))
            }

            TokenKind::Keyword(Keyword::Type) => {
                self.advance();
                let name = self.expect_ident()?;
                let generics = self.parse_generics()?;
                self.expect(&TokenKind::Eq)?;
                let ty = self.parse_type()?;
                self.expect(&TokenKind::Semi)?;

                ImplItemKind::Type {
                    name,
                    generics,
                    ty: Box::new(ty),
                }
            }

            TokenKind::Keyword(Keyword::Const) if !is_const => {
                self.advance();
                let name = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let ty = self.parse_type()?;
                self.expect(&TokenKind::Eq)?;
                let value = self.parse_expr()?;
                self.expect(&TokenKind::Semi)?;

                ImplItemKind::Const {
                    name,
                    ty: Box::new(ty),
                    value: Box::new(value),
                }
            }

            _ => return Err(self.error_expected("impl item")),
        };

        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);

        Ok(ImplItem {
            vis,
            attrs,
            is_default,
            kind,
            span,
            id: NodeId::DUMMY,
        })
    }

    // =========================================================================
    // TYPE ALIAS
    // =========================================================================

    /// Parse a type alias.
    fn parse_type_alias(&mut self) -> ParseResult<TypeAliasDef> {
        self.expect_keyword(Keyword::Type)?;

        let name = self.expect_ident()?;
        let generics = self.parse_generics()?;

        let bounds = if self.eat(&TokenKind::Colon) {
            self.parse_type_bounds()?
        } else {
            Vec::new()
        };

        let ty = if self.eat(&TokenKind::Eq) {
            Some(Box::new(self.parse_type()?))
        } else {
            None
        };

        self.expect(&TokenKind::Semi)?;

        Ok(TypeAliasDef {
            name,
            generics,
            bounds,
            ty,
        })
    }

    // =========================================================================
    // CONST / STATIC
    // =========================================================================

    /// Parse a const definition.
    fn parse_const(&mut self) -> ParseResult<ConstDef> {
        self.expect_keyword(Keyword::Const)?;
        self.parse_const_inner()
    }

    /// Parse const after keyword.
    fn parse_const_inner(&mut self) -> ParseResult<ConstDef> {
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.parse_type()?;

        let value = if self.eat(&TokenKind::Eq) {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        self.expect(&TokenKind::Semi)?;

        Ok(ConstDef {
            name,
            ty: Box::new(ty),
            value,
        })
    }

    /// Parse a static definition.
    fn parse_static(&mut self) -> ParseResult<StaticDef> {
        self.expect_keyword(Keyword::Static)?;

        let mutability = if self.eat_keyword(Keyword::Mut) {
            Mutability::Mutable
        } else {
            Mutability::Immutable
        };

        let name = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.parse_type()?;

        let value = if self.eat(&TokenKind::Eq) {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };

        self.expect(&TokenKind::Semi)?;

        Ok(StaticDef {
            name,
            mutability,
            ty: Box::new(ty),
            value,
        })
    }

    // =========================================================================
    // MODULE
    // =========================================================================

    /// Parse a module.
    fn parse_mod(&mut self, is_unsafe: bool) -> ParseResult<ModDef> {
        self.expect_keyword(Keyword::Mod)?;

        let name = self.expect_ident()?;

        let content = if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            let brace_start = self.expect(&TokenKind::OpenDelim(Delimiter::Brace))?.span;

            let attrs = self.parse_inner_attrs()?;

            let mut items = Vec::new();
            while !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) && !self.is_eof() {
                match self.parse_item() {
                    Ok(item) => items.push(item),
                    Err(e) => {
                        self.errors.push(e);
                        self.recover_to_item();
                    }
                }
            }

            let brace_end = self.expect(&TokenKind::CloseDelim(Delimiter::Brace))?.span;
            let span = brace_start.merge(&brace_end);

            Some(ModContent { attrs, items, span })
        } else {
            self.expect(&TokenKind::Semi)?;
            None
        };

        Ok(ModDef {
            name,
            content,
            is_unsafe,
            is_file_module: false,
        })
    }

    /// Parse a `module` declaration (BuildLang ecosystem convention).
    ///
    /// Syntax: `module name` or `module path::to::name`
    ///
    /// This is a file-level module declaration used by .bld ecosystem files.
    /// It declares the module path for the current file. Unlike `mod`, it does
    /// not have a body or require a semicolon - the rest of the file IS the body.
    fn parse_module_decl(&mut self, is_unsafe: bool) -> ParseResult<ModDef> {
        self.expect_keyword(Keyword::Module)?;

        // Parse the module name (may be a path like `std::math`)
        let name = self.expect_ident()?;

        // Consume any `::segment` path components (e.g., `std::math::trig`)
        while self.eat(&TokenKind::ColonColon) {
            let _segment = self.expect_ident()?;
        }

        // Check for braced body: `module name { ... items ... }`
        let content = if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            let brace_start = self.expect(&TokenKind::OpenDelim(Delimiter::Brace))?.span;
            let attrs = self.parse_inner_attrs()?;
            let mut items = Vec::new();
            while !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) && !self.is_eof() {
                match self.parse_item() {
                    Ok(item) => items.push(item),
                    Err(e) => {
                        self.errors.push(e);
                        self.recover_to_item();
                    }
                }
            }
            let brace_end = self.expect(&TokenKind::CloseDelim(Delimiter::Brace))?.span;
            Some(ModContent {
                attrs,
                items,
                span: brace_start.merge(&brace_end),
            })
        } else {
            // Optional semicolon (ecosystem files often omit it)
            self.eat(&TokenKind::Semi);
            None
        };

        Ok(ModDef {
            name,
            content,
            is_unsafe,
            is_file_module: true,
        })
    }

    // =========================================================================
    // USE
    // =========================================================================

    /// Parse a use declaration.
    fn parse_use(&mut self) -> ParseResult<UseDef> {
        self.expect_keyword(Keyword::Use)?;

        let tree = self.parse_use_tree()?;

        // Semicolon is optional (ecosystem files often omit it)
        self.eat(&TokenKind::Semi);

        Ok(UseDef { tree })
    }

    /// Parse a use tree.
    fn parse_use_tree(&mut self) -> ParseResult<UseTree> {
        let start = self.current_span();

        // Check for leading ::
        let has_leading = self.eat(&TokenKind::ColonColon);

        // Check for glob at start
        if self.eat(&TokenKind::Star) {
            let span = start.merge(&self.tokens[self.pos - 1].span);
            let path = if has_leading {
                Path::new(Vec::new(), start)
            } else {
                Path::new(Vec::new(), start)
            };
            return Ok(UseTree {
                kind: UseTreeKind::Glob(path),
                span,
            });
        }

        // Check for nested at start: {a, b}
        if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            let (trees, brace_span) = self.parse_brace_comma_seq(|p| p.parse_use_tree())?;
            let span = start.merge(&brace_span);
            return Ok(UseTree {
                kind: UseTreeKind::Nested {
                    path: Path::new(Vec::new(), start),
                    trees,
                },
                span,
            });
        }

        // Parse the path prefix
        let mut segments = Vec::new();

        // Handle self, crate, super as first segment
        if self.check_keyword(Keyword::Self_) {
            let kw_span = self.advance().span;
            segments.push(PathSegment::simple(Ident::new("self", kw_span)));
        } else if self.check_keyword(Keyword::Crate) {
            let kw_span = self.advance().span;
            segments.push(PathSegment::simple(Ident::new("crate", kw_span)));
        } else if self.check_keyword(Keyword::Super) {
            let kw_span = self.advance().span;
            segments.push(PathSegment::simple(Ident::new("super", kw_span)));
        } else {
            let ident = self.expect_ident()?;
            segments.push(PathSegment::simple(ident));
        }

        // Continue parsing path segments
        while self.eat(&TokenKind::ColonColon) {
            // Check for glob: path::*
            if self.eat(&TokenKind::Star) {
                let span = start.merge(&self.tokens[self.pos - 1].span);
                return Ok(UseTree {
                    kind: UseTreeKind::Glob(Path::new(segments, span)),
                    span,
                });
            }

            // Check for nested: path::{a, b}
            if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
                let path_span = start.merge(&self.tokens[self.pos - 1].span);
                let (trees, brace_span) = self.parse_brace_comma_seq(|p| p.parse_use_tree())?;
                let span = start.merge(&brace_span);
                return Ok(UseTree {
                    kind: UseTreeKind::Nested {
                        path: Path::new(segments, path_span),
                        trees,
                    },
                    span,
                });
            }

            // Handle self in middle of path
            if self.check_keyword(Keyword::Self_) {
                let kw_span = self.advance().span;
                segments.push(PathSegment::simple(Ident::new("self", kw_span)));
            } else {
                let ident = self.expect_ident()?;
                segments.push(PathSegment::simple(ident));
            }
        }

        // Check for rename: path as name
        let rename = if self.eat_keyword(Keyword::As) {
            Some(self.expect_ident()?)
        } else {
            None
        };

        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
        let path = Path::new(segments, span);

        Ok(UseTree {
            kind: UseTreeKind::Simple { path, rename },
            span,
        })
    }

    // =========================================================================
    // EXTERN
    // =========================================================================

    /// Parse extern crate.
    fn parse_extern_crate(&mut self) -> ParseResult<ExternCrateDef> {
        self.expect_keyword(Keyword::Crate)?;

        let name = self.expect_ident()?;

        let rename = if self.eat_keyword(Keyword::As) {
            Some(self.expect_ident()?)
        } else {
            None
        };

        self.expect(&TokenKind::Semi)?;

        Ok(ExternCrateDef { name, rename })
    }

    /// Parse extern block. The optional ABI string is parsed by the caller (so
    /// it can disambiguate `extern "C" fn ...` from a block) and passed in.
    fn parse_extern_block(
        &mut self,
        is_unsafe: bool,
        abi: Option<String>,
    ) -> ParseResult<ExternBlockDef> {
        // Optional `link "lib"` and `header "path"` clauses, in any order,
        // naming the library to link and the C header that backs the block.
        // Both are contextual keywords: unambiguous here because an extern
        // block body opens with `{`, never a bare identifier.
        let mut link = None;
        let mut header = None;
        loop {
            if !self.check_ident() {
                break;
            }
            let is_link = self.source.slice(self.current_span()) == "link";
            let is_header = self.source.slice(self.current_span()) == "header";
            if !is_link && !is_header {
                break;
            }
            self.advance(); // consume `link` or `header`
            let value = if let TokenKind::Literal { .. } = self.current_kind() {
                let token_span = self.advance().span;
                self.source.slice(token_span).trim_matches('"').to_string()
            } else {
                return Err(self.error_expected("string literal after extern block clause"));
            };
            if is_link {
                link = Some(value);
            } else {
                header = Some(value);
            }
        }

        self.expect(&TokenKind::OpenDelim(Delimiter::Brace))?;

        let mut items = Vec::new();
        while !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) && !self.is_eof() {
            items.push(self.parse_foreign_item()?);
        }

        self.expect(&TokenKind::CloseDelim(Delimiter::Brace))?;

        Ok(ExternBlockDef {
            is_unsafe,
            abi,
            header,
            link,
            items,
        })
    }

    /// Parse a foreign item.
    fn parse_foreign_item(&mut self) -> ParseResult<ForeignItem> {
        let attrs = self.parse_outer_attrs()?;
        let vis = self.parse_visibility()?;
        let start = self.current_span();

        let kind = match self.current_kind().clone() {
            TokenKind::Keyword(Keyword::Fn) => {
                let fn_def = self.parse_fn(false, false, false)?;
                ForeignItemKind::Fn(Box::new(fn_def))
            }

            TokenKind::Keyword(Keyword::Static) => {
                self.advance();
                let mutability = if self.eat_keyword(Keyword::Mut) {
                    Mutability::Mutable
                } else {
                    Mutability::Immutable
                };
                let name = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let ty = self.parse_type()?;
                self.expect(&TokenKind::Semi)?;

                ForeignItemKind::Static {
                    name,
                    mutability,
                    ty: Box::new(ty),
                }
            }

            TokenKind::Keyword(Keyword::Type) => {
                self.advance();
                let name = self.expect_ident()?;
                let bounds = if self.eat(&TokenKind::Colon) {
                    self.parse_type_bounds()?
                } else {
                    Vec::new()
                };
                self.expect(&TokenKind::Semi)?;

                ForeignItemKind::Type { name, bounds }
            }

            _ => return Err(self.error_expected("foreign item")),
        };

        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);

        Ok(ForeignItem {
            vis,
            attrs,
            kind,
            span,
            id: NodeId::DUMMY,
        })
    }

    // =========================================================================
    // MACRO
    // =========================================================================

    /// Parse macro_rules! definition.
    fn parse_macro_rules(&mut self) -> ParseResult<MacroRulesDef> {
        let name = self.expect_ident()?;

        let (rules, _) = if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            self.parse_brace_comma_seq(|p| p.parse_macro_rule())?
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
            let result = self.parse_paren_comma_seq(|p| p.parse_macro_rule())?;
            self.expect(&TokenKind::Semi)?;
            result
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Bracket)) {
            let result = self.parse_bracket_comma_seq(|p| p.parse_macro_rule())?;
            self.expect(&TokenKind::Semi)?;
            result
        } else {
            return Err(self.error_expected("macro body"));
        };

        Ok(MacroRulesDef { name, rules })
    }

    /// Parse a single macro rule.
    fn parse_macro_rule(&mut self) -> ParseResult<MacroRule> {
        let start = self.current_span();

        // Parse pattern
        let pattern = self.parse_token_trees_until(Delimiter::Paren)?;

        self.expect(&TokenKind::FatArrow)?;

        // Parse body
        let body = if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            self.parse_token_trees_until(Delimiter::Brace)?
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Paren)) {
            self.parse_token_trees_until(Delimiter::Paren)?
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Bracket)) {
            self.parse_token_trees_until(Delimiter::Bracket)?
        } else {
            return Err(self.error_expected("macro rule body"));
        };

        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);

        Ok(MacroRule {
            pattern,
            body,
            span,
        })
    }

    /// Parse a macro definition (macro 2.0).
    fn parse_macro_def(&mut self) -> ParseResult<MacroDef> {
        let name = if self.check_ident() {
            Some(self.expect_ident()?)
        } else {
            None
        };

        let body = self.parse_token_trees_until(Delimiter::Brace)?;

        Ok(MacroDef { name, body })
    }

    // =========================================================================
    // EFFECT (BuildLang extension)
    // =========================================================================

    /// Parse an effect definition.
    fn parse_effect(&mut self) -> ParseResult<EffectDef> {
        self.expect_keyword(Keyword::Effect)?;

        let name = self.expect_ident()?;
        let generics = self.parse_generics()?;

        self.expect(&TokenKind::OpenDelim(Delimiter::Brace))?;

        let mut operations = Vec::new();
        while !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) && !self.is_eof() {
            operations.push(self.parse_effect_operation()?);
        }

        self.expect(&TokenKind::CloseDelim(Delimiter::Brace))?;

        Ok(EffectDef {
            name,
            generics,
            operations,
        })
    }

    /// Parse an effect operation.
    fn parse_effect_operation(&mut self) -> ParseResult<EffectOperation> {
        let attrs = self.parse_outer_attrs()?;
        let start = self.current_span();

        self.expect_keyword(Keyword::Fn)?;
        let name = self.expect_ident()?;

        let (params, _) = self.parse_paren_comma_seq(|p| p.parse_fn_param())?;

        let return_ty = if self.eat(&TokenKind::Arrow) {
            Some(Box::new(self.parse_type()?))
        } else {
            None
        };

        // Accept either `;` or `,` as operation terminator
        if !self.eat(&TokenKind::Semi) {
            self.eat(&TokenKind::Comma);
        }

        let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);

        Ok(EffectOperation {
            attrs,
            name,
            params,
            return_ty,
            span,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{Lexer, SourceFile as LexerSourceFile};

    fn parse_item_str(s: &str) -> ParseResult<Item> {
        let source = LexerSourceFile::new("test.bld", s.to_string());
        let mut lexer = Lexer::new(&source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(&source, tokens);
        parser.parse_item()
    }

    #[test]
    fn fn_no_params() {
        let item = parse_item_str("fn foo() -> i32 { 42 }").unwrap();
        match &item.kind {
            ItemKind::Function(f) => {
                assert_eq!(f.name.as_str(), "foo");
                assert!(f.sig.params.is_empty());
                assert!(f.sig.return_ty.is_some());
                assert!(f.body.is_some());
            }
            other => panic!("expected Function, got {:?}", other),
        }
    }

    #[test]
    fn fn_with_params() {
        let item = parse_item_str("fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();
        match &item.kind {
            ItemKind::Function(f) => {
                assert_eq!(f.name.as_str(), "add");
                assert_eq!(f.sig.params.len(), 2);
                assert!(f.sig.return_ty.is_some());
                assert!(f.body.is_some());
            }
            other => panic!("expected Function, got {:?}", other),
        }
    }

    #[test]
    fn reference_self_receiver_with_lifetime_parses() {
        // `&'a self` and `&'a mut self` are reference receivers carrying an
        // explicit lifetime; the corpus pairs them with a `<'a>` method
        // generic on iterator-returning methods. Confirm all four reference
        // receiver shapes parse and that the lifetime and mutability land on
        // the receiver's `&Self` type. `&self`/`&mut self` are checked here
        // too so the refactor that added the lifetime forms is pinned as
        // behaviour-preserving.
        let cases = [
            ("fn f<'a>(&'a self) {}", true, Mutability::Immutable),
            ("fn f<'a>(&'a mut self) {}", true, Mutability::Mutable),
            ("fn f(&self) {}", false, Mutability::Immutable),
            ("fn f(&mut self) {}", false, Mutability::Mutable),
        ];
        for (src, has_lifetime, mutability) in cases {
            let item =
                parse_item_str(src).unwrap_or_else(|e| panic!("`{src}` should parse: {e:?}"));
            let f = match &item.kind {
                ItemKind::Function(f) => f,
                other => panic!("`{src}` parsed as {other:?}, expected Function"),
            };
            assert_eq!(f.sig.params.len(), 1, "`{src}` should have one (self) param");
            match &f.sig.params[0].ty.kind {
                TypeKind::Ref {
                    lifetime,
                    mutability: m,
                    ..
                } => {
                    assert_eq!(
                        lifetime.is_some(),
                        has_lifetime,
                        "`{src}` receiver lifetime presence"
                    );
                    assert_eq!(*m, mutability, "`{src}` receiver mutability");
                }
                other => panic!("`{src}` receiver type was {other:?}, expected Ref"),
            }
        }
    }

    #[test]
    fn struct_with_fields() {
        let item = parse_item_str("struct Point { x: f64, y: f64 }").unwrap();
        match &item.kind {
            ItemKind::Struct(s) => {
                assert_eq!(s.name.as_str(), "Point");
                match &s.fields {
                    StructFields::Named(fields) => {
                        assert_eq!(fields.len(), 2);
                        assert_eq!(fields[0].name.as_str(), "x");
                        assert_eq!(fields[1].name.as_str(), "y");
                    }
                    other => panic!("expected Named fields, got {:?}", other),
                }
            }
            other => panic!("expected Struct, got {:?}", other),
        }
    }

    #[test]
    fn extern_block_header_clause() {
        // `header "path"` after the ABI names the backing C header for native FFI.
        let item =
            parse_item_str("extern \"C\" header \"sqlite3.h\" { fn sqlite3_libversion() -> i32; }")
                .unwrap();
        match &item.kind {
            ItemKind::ExternBlock(eb) => {
                assert_eq!(eb.header.as_deref(), Some("sqlite3.h"));
                assert_eq!(eb.abi.as_deref(), Some("C"));
            }
            other => panic!("expected ExternBlock, got {:?}", other),
        }
    }

    #[test]
    fn extern_block_header_angle_form_preserved() {
        // The angle-bracket form is kept verbatim so the backend can emit `<...>`.
        let item =
            parse_item_str("extern \"C\" header \"<sqlite3.h>\" { fn f() -> i32; }").unwrap();
        match &item.kind {
            ItemKind::ExternBlock(eb) => assert_eq!(eb.header.as_deref(), Some("<sqlite3.h>")),
            other => panic!("expected ExternBlock, got {:?}", other),
        }
    }

    #[test]
    fn extern_block_without_header_is_none() {
        let item = parse_item_str("extern \"C\" { fn foo(); }").unwrap();
        match &item.kind {
            ItemKind::ExternBlock(eb) => assert_eq!(eb.header, None),
            other => panic!("expected ExternBlock, got {:?}", other),
        }
    }

    #[test]
    fn extern_block_header_without_abi() {
        let item = parse_item_str("extern header \"mylib.h\" { fn g(); }").unwrap();
        match &item.kind {
            ItemKind::ExternBlock(eb) => {
                assert_eq!(eb.header.as_deref(), Some("mylib.h"));
                assert_eq!(eb.abi, None);
            }
            other => panic!("expected ExternBlock, got {:?}", other),
        }
    }

    #[test]
    fn extern_variadic_fn_parses() {
        let item = parse_item_str("extern \"C\" { fn printf(fmt: &str, ...) -> i32; }").unwrap();
        match &item.kind {
            ItemKind::ExternBlock(eb) => match &eb.items[0].kind {
                ForeignItemKind::Fn(f) => {
                    assert!(f.sig.is_variadic, "printf should be variadic");
                    assert_eq!(f.sig.params.len(), 1, "the `...` is not a normal param");
                }
                other => panic!("expected Fn, got {:?}", other),
            },
            other => panic!("expected ExternBlock, got {:?}", other),
        }
    }

    #[test]
    fn non_variadic_fn_is_not_variadic() {
        let item = parse_item_str("fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();
        match &item.kind {
            ItemKind::Function(f) => assert!(!f.sig.is_variadic),
            other => panic!("expected Function, got {:?}", other),
        }
    }

    #[test]
    fn extern_block_link_clause() {
        // `link "lib"` names the library to link, so `buildc build` can pass -llib.
        let item = parse_item_str("extern \"C\" link \"sqlite3\" { fn s() -> i32; }").unwrap();
        match &item.kind {
            ItemKind::ExternBlock(eb) => {
                assert_eq!(eb.link.as_deref(), Some("sqlite3"));
                assert_eq!(eb.header, None);
            }
            other => panic!("expected ExternBlock, got {:?}", other),
        }
    }

    #[test]
    fn extern_block_link_and_header_any_order() {
        // Both clauses may appear, in either order, after the ABI.
        let lh = parse_item_str(
            "extern \"C\" link \"sqlite3\" header \"<sqlite3.h>\" { fn s() -> i32; }",
        )
        .unwrap();
        match &lh.kind {
            ItemKind::ExternBlock(eb) => {
                assert_eq!(eb.link.as_deref(), Some("sqlite3"));
                assert_eq!(eb.header.as_deref(), Some("<sqlite3.h>"));
            }
            other => panic!("expected ExternBlock, got {:?}", other),
        }

        let hl = parse_item_str(
            "extern \"C\" header \"<sqlite3.h>\" link \"sqlite3\" { fn s() -> i32; }",
        )
        .unwrap();
        match &hl.kind {
            ItemKind::ExternBlock(eb) => {
                assert_eq!(eb.link.as_deref(), Some("sqlite3"));
                assert_eq!(eb.header.as_deref(), Some("<sqlite3.h>"));
            }
            other => panic!("expected ExternBlock, got {:?}", other),
        }
    }

    #[test]
    fn extern_block_without_link_is_none() {
        let item = parse_item_str("extern \"C\" { fn foo(); }").unwrap();
        match &item.kind {
            ItemKind::ExternBlock(eb) => assert_eq!(eb.link, None),
            other => panic!("expected ExternBlock, got {:?}", other),
        }
    }

    #[test]
    fn extern_c_fn_definition_parses_as_function() {
        // `extern "C" fn ... { ... }` is a function definition (a C-ABI export),
        // not an extern block.
        let item = parse_item_str("extern \"C\" fn exported_add(a: i32, b: i32) -> i32 { a + b }")
            .unwrap();
        match &item.kind {
            ItemKind::Function(f) => {
                assert_eq!(f.name.as_str(), "exported_add");
                assert_eq!(f.sig.abi.as_deref(), Some("C"));
                assert!(f.body.is_some());
            }
            other => panic!("expected Function, got {:?}", other),
        }
    }

    #[test]
    fn extern_block_still_parses_after_extern_fn_support() {
        // Regression: `extern "C" { ... }` (with clauses) must still parse as a
        // block, not be mistaken for a function definition.
        let item = parse_item_str("extern \"C\" header \"<m.h>\" { fn f() -> i32; }").unwrap();
        match &item.kind {
            ItemKind::ExternBlock(eb) => {
                assert_eq!(eb.header.as_deref(), Some("<m.h>"));
                assert_eq!(eb.items.len(), 1);
            }
            other => panic!("expected ExternBlock, got {:?}", other),
        }
    }

    #[test]
    fn extern_abi_method_in_impl_parses() {
        // `extern "ABI" fn` is accepted as an impl method, mirroring the
        // free-item form; the ABI is recorded on the method signature.
        let item = parse_item_str(
            "impl HookManager { extern \"stdcall\" fn present(&self, flags: u32) -> u32 { flags } }",
        )
        .unwrap();
        match &item.kind {
            ItemKind::Impl(imp) => {
                assert_eq!(imp.items.len(), 1);
                match &imp.items[0].kind {
                    ImplItemKind::Function(f) => {
                        assert_eq!(f.name.as_str(), "present");
                        assert_eq!(f.sig.abi.as_deref(), Some("stdcall"));
                        assert!(f.body.is_some());
                    }
                    other => panic!("expected impl Function, got {:?}", other),
                }
            }
            other => panic!("expected Impl, got {:?}", other),
        }
    }

    #[test]
    fn plain_method_in_impl_has_no_abi() {
        // Regression: an ordinary method must still parse and carry no ABI, so
        // the new `extern` branch does not clobber the common case.
        let item = parse_item_str("impl Foo { fn bar(&self) -> u32 { 0 } }").unwrap();
        match &item.kind {
            ItemKind::Impl(imp) => match &imp.items[0].kind {
                ImplItemKind::Function(f) => {
                    assert_eq!(f.name.as_str(), "bar");
                    assert_eq!(f.sig.abi, None);
                }
                other => panic!("expected impl Function, got {:?}", other),
            },
            other => panic!("expected Impl, got {:?}", other),
        }
    }

    #[test]
    fn enum_with_variants() {
        let item = parse_item_str("enum Option<T> { Some(T), None }").unwrap();
        match &item.kind {
            ItemKind::Enum(e) => {
                assert_eq!(e.name.as_str(), "Option");
                assert!(!e.generics.params.is_empty());
                assert_eq!(e.variants.len(), 2);
                assert_eq!(e.variants[0].name.as_str(), "Some");
                assert_eq!(e.variants[1].name.as_str(), "None");
            }
            other => panic!("expected Enum, got {:?}", other),
        }
    }

    #[test]
    fn trait_definition() {
        let item = parse_item_str("trait Display { fn fmt(&self) -> str; }").unwrap();
        match &item.kind {
            ItemKind::Trait(t) => {
                assert_eq!(t.name.as_str(), "Display");
                assert_eq!(t.items.len(), 1);
                match &t.items[0].kind {
                    TraitItemKind::Function(f) => {
                        assert_eq!(f.name.as_str(), "fmt");
                        assert!(f.body.is_none(), "trait method should be bodyless");
                    }
                    other => panic!("expected trait Function item, got {:?}", other),
                }
            }
            other => panic!("expected Trait, got {:?}", other),
        }
    }

    #[test]
    fn trait_associated_const() {
        // A bare `const NAME: TYPE;` and a defaulted `const NAME: TYPE = val;`
        // are both associated const declarations; `const fn` stays a function.
        let item = parse_item_str(
            "trait Hash { const OUTPUT_SIZE: usize; const SEED: u64 = 0; const fn reset(); }",
        )
        .unwrap();
        match &item.kind {
            ItemKind::Trait(t) => {
                assert_eq!(t.items.len(), 3, "three trait items");
                match &t.items[0].kind {
                    TraitItemKind::Const { name, default, .. } => {
                        assert_eq!(name.as_str(), "OUTPUT_SIZE");
                        assert!(default.is_none(), "no default on the first const");
                    }
                    other => panic!("expected associated Const, got {:?}", other),
                }
                match &t.items[1].kind {
                    TraitItemKind::Const { name, default, .. } => {
                        assert_eq!(name.as_str(), "SEED");
                        assert!(default.is_some(), "second const carries a default");
                    }
                    other => panic!("expected defaulted Const, got {:?}", other),
                }
                match &t.items[2].kind {
                    TraitItemKind::Function(f) => assert_eq!(f.name.as_str(), "reset"),
                    other => panic!("`const fn` should stay a Function, got {:?}", other),
                }
            }
            other => panic!("expected Trait, got {:?}", other),
        }
    }

    #[test]
    fn impl_block() {
        let item = parse_item_str(
            "impl Point { fn new(x: f64, y: f64) -> Point { Point { x: x, y: y } } }",
        )
        .unwrap();
        match &item.kind {
            ItemKind::Impl(imp) => {
                assert!(imp.trait_ref.is_none(), "inherent impl has no trait ref");
                assert!(!imp.items.is_empty());
                match &imp.items[0].kind {
                    ImplItemKind::Function(f) => {
                        assert_eq!(f.name.as_str(), "new");
                        assert_eq!(f.sig.params.len(), 2);
                    }
                    other => panic!("expected impl Function item, got {:?}", other),
                }
            }
            other => panic!("expected Impl, got {:?}", other),
        }
    }

    #[test]
    fn generic_function() {
        let item = parse_item_str("fn identity<T>(x: T) -> T { x }").unwrap();
        match &item.kind {
            ItemKind::Function(f) => {
                assert_eq!(f.name.as_str(), "identity");
                assert_eq!(f.generics.params.len(), 1);
                assert_eq!(f.sig.params.len(), 1);
                assert!(f.sig.return_ty.is_some());
            }
            other => panic!("expected Function, got {:?}", other),
        }
    }

    // Extract the generic arguments on the last path segment of a function's
    // return type, so an associated-type-binding assertion can read them.
    fn return_type_generics(item: &Item) -> Vec<crate::ast::GenericArg> {
        let ret = match &item.kind {
            ItemKind::Function(f) => f.sig.return_ty.as_ref().expect("return type"),
            other => panic!("expected Function, got {:?}", other),
        };
        match &ret.kind {
            crate::ast::TypeKind::Path(p) => p.segments.last().unwrap().generics.clone(),
            other => panic!("expected Path type, got {:?}", other),
        }
    }

    #[test]
    fn associated_type_binding_parses_as_assoc_type() {
        // `Item = i32` in `Iterator<Item = i32>` is an associated-type
        // binding, not a positional type argument. It must parse as
        // `GenericArg::AssocType` carrying the bound name and type.
        let item = parse_item_str("fn f() -> Iterator<Item = i32> { 0 }").unwrap();
        let generics = return_type_generics(&item);
        assert_eq!(generics.len(), 1, "one binding, no positional args");
        match &generics[0] {
            crate::ast::GenericArg::AssocType { name, ty } => {
                assert_eq!(name.as_str(), "Item");
                assert!(matches!(&ty.kind, crate::ast::TypeKind::Path(_)));
            }
            other => panic!("expected AssocType binding, got {:?}", other),
        }
    }

    #[test]
    fn binding_and_positional_args_coexist() {
        // A positional type argument and a binding parse side by side, each
        // into its own `GenericArg` kind. This guards the disambiguation: a
        // bare `i32` stays a `Type`, only `Output = bool` becomes a binding.
        let item = parse_item_str("fn f() -> Op<i32, Output = bool> { 0 }").unwrap();
        let generics = return_type_generics(&item);
        assert_eq!(generics.len(), 2);
        assert!(
            matches!(&generics[0], crate::ast::GenericArg::Type(_)),
            "leading `i32` must stay a positional type argument"
        );
        match &generics[1] {
            crate::ast::GenericArg::AssocType { name, .. } => assert_eq!(name.as_str(), "Output"),
            other => panic!("expected AssocType binding, got {:?}", other),
        }
    }

    #[test]
    fn multiple_associated_type_bindings_parse() {
        // Two comma-separated bindings, the shape corpus trait bounds use
        // (`SerializeSeq<Ok = ..., Error = ...>`). Each binding parses
        // independently into its own kind, and the right-hand side accepts a
        // multi-segment path (`io::Error`) as well as a plain name. (A
        // `Self::`-qualified right-hand side is a separate, pre-existing gap
        // in type-position `Self` projection, not exercised here.)
        let item =
            parse_item_str("fn f() -> Seq<Ok = String, Error = io::Error> { 0 }").unwrap();
        let generics = return_type_generics(&item);
        assert_eq!(generics.len(), 2, "two bindings, no positional args");
        for (arg, expected) in generics.iter().zip(["Ok", "Error"]) {
            match arg {
                crate::ast::GenericArg::AssocType { name, ty } => {
                    assert_eq!(name.as_str(), expected);
                    assert!(matches!(&ty.kind, crate::ast::TypeKind::Path(_)));
                }
                other => panic!("expected AssocType binding, got {:?}", other),
            }
        }
    }

    #[test]
    fn self_projection_parses_in_type_position() {
        // `Self::Output` in type position is a two-segment path (`Self`, then
        // `Output`). The `Self` type arm used to consume only `Self` and leave
        // the `::` dangling, so `Poll<Self::Output>` and `Option<Self::Item>`
        // failed to parse. The projection must now parse to the full path.
        let item = parse_item_str("fn f() -> Self::Output { 0 }").unwrap();
        let ret = match &item.kind {
            ItemKind::Function(f) => f.sig.return_ty.as_ref().expect("return type"),
            other => panic!("expected Function, got {:?}", other),
        };
        match &ret.kind {
            crate::ast::TypeKind::Path(p) => {
                let names: Vec<_> = p.segments.iter().map(|s| s.ident.as_str()).collect();
                assert_eq!(names, ["Self", "Output"]);
            }
            other => panic!("expected Path type, got {:?}", other),
        }
    }

    #[test]
    fn self_projection_carries_generics_on_the_head() {
        // A projection nested inside a generic argument list, the corpus shape
        // (`Poll<Self::Output>`). The outer `Poll` gets one type argument, and
        // that argument is the two-segment `Self::Output` path.
        let item = parse_item_str("fn f() -> Poll<Self::Output> { 0 }").unwrap();
        let generics = return_type_generics(&item);
        assert_eq!(generics.len(), 1);
        match &generics[0] {
            crate::ast::GenericArg::Type(ty) => match &ty.kind {
                crate::ast::TypeKind::Path(p) => {
                    let names: Vec<_> = p.segments.iter().map(|s| s.ident.as_str()).collect();
                    assert_eq!(names, ["Self", "Output"]);
                }
                other => panic!("expected Path argument, got {:?}", other),
            },
            other => panic!("expected Type argument, got {:?}", other),
        }
    }

    #[test]
    fn pub_function() {
        let item = parse_item_str("pub fn greet() {}").unwrap();
        assert!(matches!(&item.vis, Visibility::Public(_)));
        assert!(matches!(&item.kind, ItemKind::Function(_)));
    }

    #[test]
    fn unit_struct() {
        let item = parse_item_str("struct Unit;").unwrap();
        match &item.kind {
            ItemKind::Struct(s) => {
                assert_eq!(s.name.as_str(), "Unit");
                assert!(matches!(&s.fields, StructFields::Unit));
            }
            other => panic!("expected Struct(Unit), got {:?}", other),
        }
    }

    #[test]
    fn tuple_struct() {
        let item = parse_item_str("struct Pair(i32, i32);").unwrap();
        match &item.kind {
            ItemKind::Struct(s) => {
                assert_eq!(s.name.as_str(), "Pair");
                match &s.fields {
                    StructFields::Tuple(fields) => assert_eq!(fields.len(), 2),
                    other => panic!("expected Tuple fields, got {:?}", other),
                }
            }
            other => panic!("expected Struct, got {:?}", other),
        }
    }
}
