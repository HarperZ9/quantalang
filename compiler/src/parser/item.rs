// ===============================================================================
// QUANTALANG PARSER - ITEM PARSING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
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
//! - Effects (QuantaLang extension)

use crate::ast::*;
use crate::lexer::{Delimiter, Keyword, TokenKind};
use super::{Parser, ParseResult, ParseError, ParseErrorKind};

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
        let is_default = self.eat_keyword(Keyword::Default);
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
                    let extern_block = self.parse_extern_block(is_unsafe)?;
                    let span = start.merge(&self.tokens[self.pos.saturating_sub(1)].span);
                    Ok(Item::new(
                        ItemKind::ExternBlock(Box::new(extern_block)),
                        vis,
                        attrs,
                        span,
                    ))
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
            // EFFECT (QuantaLang extension)
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

        let (params, _) = self.parse_paren_comma_seq(|p| p.parse_fn_param())?;

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
    fn try_parse_self_param(&mut self, attrs: &[Attribute], start: Span) -> ParseResult<Option<Param>> {
        use crate::ast::{Pattern, PatternKind, Mutability, Type, TypeKind, Path, PathSegment};

        // Check for &self or &mut self
        if self.check(&TokenKind::And) {
            // Look ahead to see if this is &self or &mut self
            let is_ref_self = self.peek().kind == TokenKind::Keyword(Keyword::Self_);
            let is_ref_mut_self = self.peek().kind == TokenKind::Keyword(Keyword::Mut)
                && self.pos + 2 < self.tokens.len()
                && self.tokens[self.pos + 2].kind == TokenKind::Keyword(Keyword::Self_);

            if is_ref_self {
                // &self
                self.advance(); // consume &
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

                // Type is &Self
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
                        lifetime: None,
                        mutability: Mutability::Immutable,
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
            } else if is_ref_mut_self {
                // &mut self
                self.advance(); // consume &
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

                // Type is &mut Self
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
                        lifetime: None,
                        mutability: Mutability::Mutable,
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

        // Check for mut self
        if self.check_keyword(Keyword::Mut) && self.peek().kind == TokenKind::Keyword(Keyword::Self_) {
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

            // Type is Self
            let self_type_path = Path {
                segments: vec![PathSegment {
                    ident: Ident::new("Self", self_span),
                    generics: vec![],
                }],
                span: self_span,
            };
            let ty = Type {
                kind: TypeKind::Path(self_type_path),
                span: self_span,
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

        // Check for plain self
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

            // Type is Self
            let self_type_path = Path {
                segments: vec![PathSegment {
                    ident: Ident::new("Self", self_span),
                    generics: vec![],
                }],
                span: self_span,
            };
            let ty = Type {
                kind: TypeKind::Path(self_type_path),
                span: self_span,
                id: NodeId::DUMMY,
            };

            return Ok(Some(Param {
                attrs: attrs.to_vec(),
                pattern,
                ty: Box::new(ty),
                default: None,
                span: self_span,
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

        Ok(StructDef { name, generics, fields })
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

        Ok(TupleFieldDef { vis, attrs, ty: Box::new(ty), span })
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

        Ok(EnumDef { name, generics, variants })
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

        Ok(EnumVariant { attrs, name, fields, discriminant, span })
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

        let is_const = self.eat_keyword(Keyword::Const);
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
                _ => return Err(ParseError::new(
                    ParseErrorKind::InvalidType,
                    ty.span,
                )),
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

        let is_default = self.eat_keyword(Keyword::Default);
        let is_const = self.eat_keyword(Keyword::Const);
        let is_async = self.eat_keyword(Keyword::Async);
        let is_unsafe = self.eat_keyword(Keyword::Unsafe);

        let kind = match self.current_kind().clone() {
            TokenKind::Keyword(Keyword::Fn) => {
                let fn_def = self.parse_fn(is_unsafe, is_async, is_const)?;
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

        Ok(TypeAliasDef { name, generics, bounds, ty })
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

        Ok(ModDef { name, content, is_unsafe })
    }

    // =========================================================================
    // USE
    // =========================================================================

    /// Parse a use declaration.
    fn parse_use(&mut self) -> ParseResult<UseDef> {
        self.expect_keyword(Keyword::Use)?;

        let tree = self.parse_use_tree()?;

        self.expect(&TokenKind::Semi)?;

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

    /// Parse extern block.
    fn parse_extern_block(&mut self, is_unsafe: bool) -> ParseResult<ExternBlockDef> {
        let abi = if let TokenKind::Literal { .. } = self.current_kind() {
            let token_span = self.advance().span;
            Some(self.source.slice(token_span).trim_matches('"').to_string())
        } else {
            None
        };

        self.expect(&TokenKind::OpenDelim(Delimiter::Brace))?;

        let mut items = Vec::new();
        while !self.check(&TokenKind::CloseDelim(Delimiter::Brace)) && !self.is_eof() {
            items.push(self.parse_foreign_item()?);
        }

        self.expect(&TokenKind::CloseDelim(Delimiter::Brace))?;

        Ok(ExternBlockDef {
            is_unsafe,
            abi,
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

        Ok(MacroRule { pattern, body, span })
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
    // EFFECT (QuantaLang extension)
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

        Ok(EffectDef { name, generics, operations })
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
