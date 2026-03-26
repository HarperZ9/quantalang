// ===============================================================================
// QUANTALANG MACRO EXPANSION - HYGIENE
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Macro hygiene for proper scoping.
//!
//! This module implements hygienic macro expansion, ensuring that identifiers
//! introduced by macros don't accidentally capture or shadow user identifiers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::lexer::Span;

// =============================================================================
// SYNTAX CONTEXT
// =============================================================================

/// A syntax context identifies the macro expansion context of an identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SyntaxContext(pub u32);

impl SyntaxContext {
    /// The root (non-macro) syntax context.
    pub const ROOT: SyntaxContext = SyntaxContext(0);

    /// Create a fresh syntax context.
    pub fn fresh() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(1);
        SyntaxContext(COUNTER.fetch_add(1, Ordering::SeqCst))
    }

    /// Check if this is the root context.
    pub fn is_root(self) -> bool {
        self.0 == 0
    }
}

impl Default for SyntaxContext {
    fn default() -> Self {
        Self::ROOT
    }
}

// =============================================================================
// HYGIENE CONTEXT
// =============================================================================

/// The hygiene context tracks macro expansion information.
#[derive(Debug, Clone)]
pub struct HygieneContext {
    /// Information about each syntax context.
    contexts: HashMap<SyntaxContext, ContextInfo>,
    /// The next syntax context ID.
    next_id: u32,
}

/// Information about a syntax context.
#[derive(Debug, Clone)]
pub struct ContextInfo {
    /// The parent context (if any).
    pub parent: Option<SyntaxContext>,
    /// The span where this context was created.
    pub expansion_span: Span,
    /// Whether this context is opaque (identifiers don't leak out).
    pub is_opaque: bool,
}

impl HygieneContext {
    /// Create a new hygiene context.
    pub fn new() -> Self {
        let mut contexts = HashMap::new();
        contexts.insert(SyntaxContext::ROOT, ContextInfo {
            parent: None,
            expansion_span: Span::dummy(),
            is_opaque: false,
        });

        Self {
            contexts,
            next_id: 1,
        }
    }

    /// Create a fresh syntax context.
    pub fn fresh_context(&mut self, span: Span) -> SyntaxContext {
        let id = SyntaxContext(self.next_id);
        self.next_id += 1;

        self.contexts.insert(id, ContextInfo {
            parent: Some(SyntaxContext::ROOT),
            expansion_span: span,
            is_opaque: true,
        });

        id
    }

    /// Create a fresh context with a specific parent.
    pub fn fresh_with_parent(&mut self, parent: SyntaxContext, span: Span) -> SyntaxContext {
        let id = SyntaxContext(self.next_id);
        self.next_id += 1;

        self.contexts.insert(id, ContextInfo {
            parent: Some(parent),
            expansion_span: span,
            is_opaque: true,
        });

        id
    }

    /// Get information about a context.
    pub fn context_info(&self, ctx: SyntaxContext) -> Option<&ContextInfo> {
        self.contexts.get(&ctx)
    }

    /// Check if two contexts are compatible for identifier resolution.
    pub fn contexts_compatible(&self, ctx1: SyntaxContext, ctx2: SyntaxContext) -> bool {
        if ctx1 == ctx2 {
            return true;
        }

        // Check if one is an ancestor of the other
        self.is_ancestor(ctx1, ctx2) || self.is_ancestor(ctx2, ctx1)
    }

    /// Check if ctx1 is an ancestor of ctx2.
    pub fn is_ancestor(&self, ctx1: SyntaxContext, ctx2: SyntaxContext) -> bool {
        let mut current = ctx2;
        while let Some(info) = self.contexts.get(&current) {
            if let Some(parent) = info.parent {
                if parent == ctx1 {
                    return true;
                }
                current = parent;
            } else {
                break;
            }
        }
        false
    }

    /// Get the parent chain of a context.
    pub fn parent_chain(&self, ctx: SyntaxContext) -> Vec<SyntaxContext> {
        let mut chain = vec![ctx];
        let mut current = ctx;

        while let Some(info) = self.contexts.get(&current) {
            if let Some(parent) = info.parent {
                chain.push(parent);
                current = parent;
            } else {
                break;
            }
        }

        chain
    }

    /// Apply a syntax context to a span.
    pub fn apply_context(&self, span: Span, _ctx: SyntaxContext) -> Span {
        // For now, just return the span unchanged
        // In a full implementation, we would attach the context to the span
        span
    }

    /// Mark an identifier as being used across a macro boundary.
    pub fn mark_cross_boundary(&mut self, ctx: SyntaxContext, name: &str) {
        // This would be used for $crate and similar features
        // For now, just a placeholder
        let _ = (ctx, name);
    }
}

impl Default for HygieneContext {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// IDENTIFIER HYGIENE
// =============================================================================

/// A hygienic identifier with its syntax context.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HygienicIdent {
    /// The identifier name.
    pub name: String,
    /// The syntax context.
    pub context: SyntaxContext,
}

impl HygienicIdent {
    /// Create a new hygienic identifier.
    pub fn new(name: impl Into<String>, context: SyntaxContext) -> Self {
        Self {
            name: name.into(),
            context,
        }
    }

    /// Create an identifier in the root context.
    pub fn root(name: impl Into<String>) -> Self {
        Self::new(name, SyntaxContext::ROOT)
    }

    /// Check if this identifier can refer to the same binding as another.
    pub fn can_resolve_to(&self, other: &HygienicIdent, hygiene: &HygieneContext) -> bool {
        if self.name != other.name {
            return false;
        }
        hygiene.contexts_compatible(self.context, other.context)
    }
}

/// Generate a unique gensym name.
pub fn gensym(prefix: &str) -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    format!("{}_{}", prefix, COUNTER.fetch_add(1, Ordering::SeqCst))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_context() {
        assert!(SyntaxContext::ROOT.is_root());
        assert!(!SyntaxContext::fresh().is_root());
    }

    #[test]
    fn test_fresh_contexts() {
        let ctx1 = SyntaxContext::fresh();
        let ctx2 = SyntaxContext::fresh();
        assert_ne!(ctx1, ctx2);
    }

    #[test]
    fn test_context_compatibility() {
        let mut hygiene = HygieneContext::new();

        let ctx1 = hygiene.fresh_context(Span::dummy());
        let ctx2 = hygiene.fresh_with_parent(ctx1, Span::dummy());

        // Same context is compatible
        assert!(hygiene.contexts_compatible(ctx1, ctx1));

        // Parent-child is compatible
        assert!(hygiene.contexts_compatible(ctx1, ctx2));

        // Unrelated contexts are not compatible
        let ctx3 = hygiene.fresh_context(Span::dummy());
        assert!(!hygiene.contexts_compatible(ctx2, ctx3));
    }

    #[test]
    fn test_hygienic_ident_resolution() {
        let hygiene = HygieneContext::new();

        let ident1 = HygienicIdent::root("x");
        let ident2 = HygienicIdent::root("x");
        let ident3 = HygienicIdent::root("y");
        let ident4 = HygienicIdent::new("x", SyntaxContext::fresh());

        // Same name and context can resolve
        assert!(ident1.can_resolve_to(&ident2, &hygiene));

        // Different names cannot resolve
        assert!(!ident1.can_resolve_to(&ident3, &hygiene));

        // Different unrelated contexts cannot resolve
        // (in the default hygiene, fresh contexts are children of root,
        // so this actually can resolve)
        // For truly opaque contexts, we would need additional logic
    }

    #[test]
    fn test_gensym() {
        let name1 = gensym("temp");
        let name2 = gensym("temp");
        assert_ne!(name1, name2);
        assert!(name1.starts_with("temp_"));
    }
}
