// ===============================================================================
// QUANTALANG AST - ITEMS (TOP-LEVEL DECLARATIONS)
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Item AST nodes.
//!
//! Items are top-level declarations: functions, structs, enums, traits, etc.

use super::{
    Attribute, Block, Expr, Generics, Ident, Mutability, NodeId, Path, Pattern, Type, TypeBound,
    Visibility,
};
use crate::lexer::Span;

/// An item node.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// The kind of item.
    pub kind: ItemKind,
    /// Visibility.
    pub vis: Visibility,
    /// Attributes on this item.
    pub attrs: Vec<Attribute>,
    /// The span of this item.
    pub span: Span,
    /// Node ID.
    pub id: NodeId,
}

impl Item {
    /// Create a new item.
    pub fn new(kind: ItemKind, vis: Visibility, attrs: Vec<Attribute>, span: Span) -> Self {
        Self {
            kind,
            vis,
            attrs,
            span,
            id: NodeId::DUMMY,
        }
    }

    /// Get the name of this item (if it has one).
    pub fn name(&self) -> Option<&Ident> {
        match &self.kind {
            ItemKind::Function(f) => Some(&f.name),
            ItemKind::Struct(s) => Some(&s.name),
            ItemKind::Enum(e) => Some(&e.name),
            ItemKind::Trait(t) => Some(&t.name),
            ItemKind::TypeAlias(t) => Some(&t.name),
            ItemKind::Const(c) => Some(&c.name),
            ItemKind::Static(s) => Some(&s.name),
            ItemKind::Mod(m) => Some(&m.name),
            ItemKind::Impl(_) => None,
            ItemKind::Use(_) => None,
            ItemKind::ExternCrate(e) => Some(&e.name),
            ItemKind::ExternBlock(_) => None,
            ItemKind::Macro(m) => m.name.as_ref(),
            ItemKind::MacroRules(m) => Some(&m.name),
            ItemKind::Effect(e) => Some(&e.name),
        }
    }
}

/// The kind of item.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    /// Function definition.
    Function(Box<FnDef>),
    /// Struct definition.
    Struct(Box<StructDef>),
    /// Enum definition.
    Enum(Box<EnumDef>),
    /// Trait definition.
    Trait(Box<TraitDef>),
    /// Impl block.
    Impl(Box<ImplDef>),
    /// Type alias.
    TypeAlias(Box<TypeAliasDef>),
    /// Constant.
    Const(Box<ConstDef>),
    /// Static.
    Static(Box<StaticDef>),
    /// Module.
    Mod(Box<ModDef>),
    /// Use declaration.
    Use(Box<UseDef>),
    /// Extern crate.
    ExternCrate(Box<ExternCrateDef>),
    /// Extern block.
    ExternBlock(Box<ExternBlockDef>),
    /// Macro definition.
    Macro(Box<MacroDef>),
    /// Macro rules.
    MacroRules(Box<MacroRulesDef>),
    /// Effect definition (QuantaLang extension).
    Effect(Box<EffectDef>),
}

/// Function definition.
#[derive(Debug, Clone, PartialEq)]
pub struct FnDef {
    /// Function name.
    pub name: Ident,
    /// Generic parameters.
    pub generics: Generics,
    /// Function signature.
    pub sig: FnSig,
    /// Function body (None for declarations).
    pub body: Option<Box<Block>>,
}

/// Function signature.
#[derive(Debug, Clone, PartialEq)]
pub struct FnSig {
    /// Whether this is unsafe.
    pub is_unsafe: bool,
    /// Whether this is async.
    pub is_async: bool,
    /// Whether this is const.
    pub is_const: bool,
    /// Extern ABI (if any).
    pub abi: Option<String>,
    /// Parameters.
    pub params: Vec<Param>,
    /// Return type (None for unit).
    pub return_ty: Option<Box<Type>>,
    /// Effect annotations.
    pub effects: Vec<Path>,
}

impl Default for FnSig {
    fn default() -> Self {
        Self {
            is_unsafe: false,
            is_async: false,
            is_const: false,
            abi: None,
            params: Vec::new(),
            return_ty: None,
            effects: Vec::new(),
        }
    }
}

/// Function parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// Attributes.
    pub attrs: Vec<Attribute>,
    /// The pattern.
    pub pattern: Pattern,
    /// The type.
    pub ty: Box<Type>,
    /// Default value.
    pub default: Option<Box<Expr>>,
    /// Span.
    pub span: Span,
}

/// Struct definition.
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    /// Struct name.
    pub name: Ident,
    /// Generic parameters.
    pub generics: Generics,
    /// Struct fields.
    pub fields: StructFields,
}

/// Struct field variants.
#[derive(Debug, Clone, PartialEq)]
pub enum StructFields {
    /// Named fields: `struct Point { x: i32, y: i32 }`
    Named(Vec<FieldDef>),
    /// Tuple fields: `struct Point(i32, i32);`
    Tuple(Vec<TupleFieldDef>),
    /// Unit struct: `struct Unit;`
    Unit,
}

/// Named field definition.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    /// Visibility.
    pub vis: Visibility,
    /// Attributes.
    pub attrs: Vec<Attribute>,
    /// Field name.
    pub name: Ident,
    /// Field type.
    pub ty: Box<Type>,
    /// Default value.
    pub default: Option<Box<Expr>>,
    /// Span.
    pub span: Span,
}

/// Tuple field definition.
#[derive(Debug, Clone, PartialEq)]
pub struct TupleFieldDef {
    /// Visibility.
    pub vis: Visibility,
    /// Attributes.
    pub attrs: Vec<Attribute>,
    /// Field type.
    pub ty: Box<Type>,
    /// Span.
    pub span: Span,
}

/// Enum definition.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    /// Enum name.
    pub name: Ident,
    /// Generic parameters.
    pub generics: Generics,
    /// Variants.
    pub variants: Vec<EnumVariant>,
}

/// Enum variant.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    /// Attributes.
    pub attrs: Vec<Attribute>,
    /// Variant name.
    pub name: Ident,
    /// Variant fields.
    pub fields: StructFields,
    /// Discriminant expression.
    pub discriminant: Option<Box<Expr>>,
    /// Span.
    pub span: Span,
}

/// Trait definition.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitDef {
    /// Trait name.
    pub name: Ident,
    /// Whether this is unsafe.
    pub is_unsafe: bool,
    /// Whether this is auto.
    pub is_auto: bool,
    /// Generic parameters.
    pub generics: Generics,
    /// Supertraits.
    pub supertraits: Vec<TypeBound>,
    /// Trait items.
    pub items: Vec<TraitItem>,
}

/// Item in a trait definition.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitItem {
    /// Attributes.
    pub attrs: Vec<Attribute>,
    /// The kind of trait item.
    pub kind: TraitItemKind,
    /// Span.
    pub span: Span,
    /// Node ID.
    pub id: NodeId,
}

/// Kind of trait item.
#[derive(Debug, Clone, PartialEq)]
pub enum TraitItemKind {
    /// Associated function/method.
    Function(Box<FnDef>),
    /// Associated type.
    Type {
        name: Ident,
        generics: Generics,
        bounds: Vec<TypeBound>,
        default: Option<Box<Type>>,
    },
    /// Associated constant.
    Const {
        name: Ident,
        ty: Box<Type>,
        default: Option<Box<Expr>>,
    },
    /// Macro invocation.
    Macro {
        path: Path,
        tokens: Vec<super::TokenTree>,
    },
}

/// Impl block.
#[derive(Debug, Clone, PartialEq)]
pub struct ImplDef {
    /// Whether this is unsafe.
    pub is_unsafe: bool,
    /// Whether this is negative (`impl !Trait for Type`).
    pub is_negative: bool,
    /// Generic parameters.
    pub generics: Generics,
    /// Trait being implemented (None for inherent impl).
    pub trait_ref: Option<TraitRef>,
    /// Type being implemented for.
    pub self_ty: Box<Type>,
    /// Impl items.
    pub items: Vec<ImplItem>,
}

/// Trait reference in an impl.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitRef {
    /// The trait path.
    pub path: Path,
    /// Whether this is a `!Trait` bound.
    pub is_negative: bool,
}

/// Item in an impl block.
#[derive(Debug, Clone, PartialEq)]
pub struct ImplItem {
    /// Visibility.
    pub vis: Visibility,
    /// Attributes.
    pub attrs: Vec<Attribute>,
    /// Whether this is a default impl.
    pub is_default: bool,
    /// The kind of impl item.
    pub kind: ImplItemKind,
    /// Span.
    pub span: Span,
    /// Node ID.
    pub id: NodeId,
}

/// Kind of impl item.
#[derive(Debug, Clone, PartialEq)]
pub enum ImplItemKind {
    /// Associated function/method.
    Function(Box<FnDef>),
    /// Associated type.
    Type {
        name: Ident,
        generics: Generics,
        ty: Box<Type>,
    },
    /// Associated constant.
    Const {
        name: Ident,
        ty: Box<Type>,
        value: Box<Expr>,
    },
    /// Macro invocation.
    Macro {
        path: Path,
        tokens: Vec<super::TokenTree>,
    },
}

/// Type alias definition.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeAliasDef {
    /// Type name.
    pub name: Ident,
    /// Generic parameters.
    pub generics: Generics,
    /// Bounds (for associated types).
    pub bounds: Vec<TypeBound>,
    /// The aliased type.
    pub ty: Option<Box<Type>>,
}

/// Constant definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstDef {
    /// Constant name.
    pub name: Ident,
    /// Type.
    pub ty: Box<Type>,
    /// Value.
    pub value: Option<Box<Expr>>,
}

/// Static definition.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticDef {
    /// Static name.
    pub name: Ident,
    /// Mutability.
    pub mutability: Mutability,
    /// Type.
    pub ty: Box<Type>,
    /// Value.
    pub value: Option<Box<Expr>>,
}

/// Module definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ModDef {
    /// Module name.
    pub name: Ident,
    /// Module content (None for `mod foo;`).
    pub content: Option<ModContent>,
    /// Whether this is unsafe.
    pub is_unsafe: bool,
}

/// Module content.
#[derive(Debug, Clone, PartialEq)]
pub struct ModContent {
    /// Inner attributes.
    pub attrs: Vec<Attribute>,
    /// Items.
    pub items: Vec<Item>,
    /// Span.
    pub span: Span,
}

/// Use declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct UseDef {
    /// The use tree.
    pub tree: UseTree,
}

/// Use tree.
#[derive(Debug, Clone, PartialEq)]
pub struct UseTree {
    /// The kind of use tree.
    pub kind: UseTreeKind,
    /// Span.
    pub span: Span,
}

/// Kind of use tree.
#[derive(Debug, Clone, PartialEq)]
pub enum UseTreeKind {
    /// Simple path: `use std::io;`
    Simple { path: Path, rename: Option<Ident> },
    /// Glob: `use std::io::*;`
    Glob(Path),
    /// Nested: `use std::{io, fs};`
    Nested { path: Path, trees: Vec<UseTree> },
}

/// Extern crate.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternCrateDef {
    /// Crate name.
    pub name: Ident,
    /// Rename.
    pub rename: Option<Ident>,
}

/// Extern block.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternBlockDef {
    /// Whether this is unsafe.
    pub is_unsafe: bool,
    /// ABI.
    pub abi: Option<String>,
    /// Items.
    pub items: Vec<ForeignItem>,
}

/// Item in an extern block.
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignItem {
    /// Visibility.
    pub vis: Visibility,
    /// Attributes.
    pub attrs: Vec<Attribute>,
    /// The kind of foreign item.
    pub kind: ForeignItemKind,
    /// Span.
    pub span: Span,
    /// Node ID.
    pub id: NodeId,
}

/// Kind of foreign item.
#[derive(Debug, Clone, PartialEq)]
pub enum ForeignItemKind {
    /// Foreign function.
    Fn(Box<FnDef>),
    /// Foreign static.
    Static {
        name: Ident,
        mutability: Mutability,
        ty: Box<Type>,
    },
    /// Foreign type.
    Type { name: Ident, bounds: Vec<TypeBound> },
    /// Macro invocation.
    Macro {
        path: Path,
        tokens: Vec<super::TokenTree>,
    },
}

/// Macro definition.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroDef {
    /// Macro name.
    pub name: Option<Ident>,
    /// Macro body.
    pub body: Vec<super::TokenTree>,
}

/// Macro rules definition.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroRulesDef {
    /// Macro name.
    pub name: Ident,
    /// Rules.
    pub rules: Vec<MacroRule>,
}

/// A macro rule.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroRule {
    /// Pattern.
    pub pattern: Vec<super::TokenTree>,
    /// Body.
    pub body: Vec<super::TokenTree>,
    /// Span.
    pub span: Span,
}

/// Effect definition (QuantaLang extension).
#[derive(Debug, Clone, PartialEq)]
pub struct EffectDef {
    /// Effect name.
    pub name: Ident,
    /// Generic parameters.
    pub generics: Generics,
    /// Operations.
    pub operations: Vec<EffectOperation>,
}

/// An operation in an effect.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectOperation {
    /// Attributes.
    pub attrs: Vec<Attribute>,
    /// Operation name.
    pub name: Ident,
    /// Parameters.
    pub params: Vec<Param>,
    /// Return type.
    pub return_ty: Option<Box<Type>>,
    /// Span.
    pub span: Span,
}
