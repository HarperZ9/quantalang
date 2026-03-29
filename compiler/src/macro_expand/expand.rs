// ===============================================================================
// QUANTALANG MACRO EXPANSION - EXPANSION ENGINE
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Macro expansion engine.
//!
//! This module implements the expansion of macro invocations to AST nodes.

use crate::lexer::{Span, Token, TokenKind};

use super::hygiene::{HygieneContext, SyntaxContext};
use super::{
    match_macro_pattern, tokens_to_tree, Binding, BindingValue, Bindings, ExpansionElement,
    MacroContext, MacroError, MacroExpansion, MacroResult, RepetitionKind, TokenTree,
};

// =============================================================================
// MACRO EXPANDER
// =============================================================================

/// The macro expansion engine.
pub struct MacroExpander<'ctx> {
    /// The macro context.
    ctx: &'ctx MacroContext,
    /// The hygiene context.
    hygiene: HygieneContext,
    /// Current expansion depth.
    depth: u32,
    /// Maximum expansion depth.
    max_depth: u32,
}

impl<'ctx> MacroExpander<'ctx> {
    /// Create a new macro expander.
    pub fn new(ctx: &'ctx MacroContext) -> Self {
        Self {
            ctx,
            hygiene: HygieneContext::new(),
            depth: 0,
            max_depth: 128,
        }
    }

    /// Set the maximum expansion depth.
    pub fn with_max_depth(mut self, max_depth: u32) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Expand a macro invocation.
    pub fn expand(
        &mut self,
        name: &str,
        input: &[Token],
        span: Span,
    ) -> MacroResult<Vec<TokenTree>> {
        // Check recursion limit
        if self.depth >= self.max_depth {
            return Err(MacroError::RecursionLimit {
                limit: self.max_depth,
            });
        }

        // Look up the macro
        let macro_def = self
            .ctx
            .lookup_macro(name)
            .ok_or_else(|| MacroError::MacroNotFound {
                name: name.to_string(),
            })?;

        // Convert input to token trees
        let input_trees = tokens_to_tree(input);

        // Try each rule
        for rule in &macro_def.rules {
            if let Ok(bindings) = match_macro_pattern(&rule.pattern, &input_trees) {
                // Create a new syntax context for this expansion
                let syntax_ctx = self.hygiene.fresh_context(span);

                // Expand the template
                self.depth += 1;
                let result = self.expand_template(&rule.expansion, &bindings, syntax_ctx);
                self.depth -= 1;

                return result;
            }
        }

        // No rule matched
        Err(MacroError::NoMatchingRule {
            name: name.to_string(),
        })
    }

    /// Expand a macro template.
    fn expand_template(
        &mut self,
        template: &MacroExpansion,
        bindings: &Bindings,
        syntax_ctx: SyntaxContext,
    ) -> MacroResult<Vec<TokenTree>> {
        let mut result = Vec::new();

        for element in &template.elements {
            let expanded = self.expand_element(element, bindings, syntax_ctx)?;
            result.extend(expanded);
        }

        Ok(result)
    }

    /// Expand a single template element.
    fn expand_element(
        &mut self,
        element: &ExpansionElement,
        bindings: &Bindings,
        syntax_ctx: SyntaxContext,
    ) -> MacroResult<Vec<TokenTree>> {
        match element {
            ExpansionElement::Token(kind, span) => {
                // Apply hygiene to the token
                let token = Token {
                    kind: kind.clone(),
                    span: self.hygiene.apply_context(*span, syntax_ctx),
                };
                Ok(vec![TokenTree::Token(token)])
            }
            ExpansionElement::MetaVar(name) => self.expand_metavar(name, bindings),
            ExpansionElement::Repetition {
                elements,
                separator,
                repetition,
            } => self.expand_repetition(
                elements,
                separator.as_ref(),
                *repetition,
                bindings,
                syntax_ctx,
            ),
            ExpansionElement::Delimited {
                delimiter,
                elements,
                span,
            } => {
                let inner = self.expand_elements(elements, bindings, syntax_ctx)?;
                Ok(vec![TokenTree::Delimited {
                    delimiter: *delimiter,
                    open_span: *span,
                    tokens: inner,
                    close_span: *span,
                }])
            }
        }
    }

    /// Expand multiple elements.
    fn expand_elements(
        &mut self,
        elements: &[ExpansionElement],
        bindings: &Bindings,
        syntax_ctx: SyntaxContext,
    ) -> MacroResult<Vec<TokenTree>> {
        let mut result = Vec::new();
        for element in elements {
            result.extend(self.expand_element(element, bindings, syntax_ctx)?);
        }
        Ok(result)
    }

    /// Expand a metavariable reference.
    fn expand_metavar(&mut self, name: &str, bindings: &Bindings) -> MacroResult<Vec<TokenTree>> {
        let binding = bindings
            .get(name)
            .ok_or_else(|| MacroError::MetaVarNotFound {
                name: name.to_string(),
            })?;

        match binding {
            Binding::Single(value) => match value {
                BindingValue::TokenTree(tt) => Ok(vec![tt.clone()]),
                BindingValue::TokenTrees(tts) => Ok(tts.clone()),
            },
            Binding::Repeated(_) => {
                // This shouldn't happen outside of a repetition context
                Err(MacroError::RepetitionMismatch {
                    name: name.to_string(),
                    other: "non-repeated context".to_string(),
                })
            }
        }
    }

    /// Expand a repetition.
    fn expand_repetition(
        &mut self,
        elements: &[ExpansionElement],
        separator: Option<&TokenKind>,
        _repetition: RepetitionKind,
        bindings: &Bindings,
        syntax_ctx: SyntaxContext,
    ) -> MacroResult<Vec<TokenTree>> {
        // Find a repeating metavar to determine the count
        let count = self.find_repetition_count(elements, bindings)?;

        let mut result = Vec::new();

        for i in 0..count {
            if i > 0 {
                if let Some(sep) = separator {
                    let sep_token = Token {
                        kind: sep.clone(),
                        span: Span::dummy(),
                    };
                    result.push(TokenTree::Token(sep_token));
                }
            }

            // Create bindings for this iteration
            let iter_bindings = self.extract_iteration(bindings, i);

            // Expand elements with iteration bindings
            result.extend(self.expand_elements(elements, &iter_bindings, syntax_ctx)?);
        }

        Ok(result)
    }

    /// Find the repetition count from a set of bindings.
    fn find_repetition_count(
        &self,
        elements: &[ExpansionElement],
        bindings: &Bindings,
    ) -> MacroResult<usize> {
        for element in elements {
            if let Some(count) = self.element_repetition_count(element, bindings) {
                return Ok(count);
            }
        }
        // No repeating metavar found - default to 0
        Ok(0)
    }

    /// Get the repetition count for an element.
    fn element_repetition_count(
        &self,
        element: &ExpansionElement,
        bindings: &Bindings,
    ) -> Option<usize> {
        match element {
            ExpansionElement::MetaVar(name) => {
                if let Some(binding) = bindings.get(name) {
                    match binding {
                        Binding::Repeated(v) => Some(v.len()),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            ExpansionElement::Delimited { elements, .. } => {
                for e in elements {
                    if let Some(count) = self.element_repetition_count(e, bindings) {
                        return Some(count);
                    }
                }
                None
            }
            ExpansionElement::Repetition { elements, .. } => {
                for e in elements {
                    if let Some(count) = self.element_repetition_count(e, bindings) {
                        return Some(count);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Extract bindings for a single iteration.
    fn extract_iteration(&self, bindings: &Bindings, index: usize) -> Bindings {
        let mut result = Bindings::new();

        for (name, binding) in &bindings.bindings {
            let iter_binding = match binding {
                Binding::Single(v) => Binding::Single(v.clone()),
                Binding::Repeated(v) => {
                    if index < v.len() {
                        v[index].clone()
                    } else {
                        // Should not happen if counts are consistent
                        Binding::Single(BindingValue::TokenTrees(Vec::new()))
                    }
                }
            };
            result.insert(name.clone(), iter_binding);
        }

        result
    }
}

/// Expand a macro invocation.
pub fn expand_macro(
    ctx: &MacroContext,
    name: &str,
    input: &[Token],
    span: Span,
) -> MacroResult<Vec<TokenTree>> {
    let mut expander = MacroExpander::new(ctx);
    expander.expand(name, input, span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{Lexer, SourceFile};

    fn lex(source: &str) -> Vec<Token> {
        let file = SourceFile::anonymous(source);
        let mut lexer = Lexer::new(&file);
        lexer.tokenize().unwrap()
    }

    #[test]
    fn test_expand_simple() {
        // Create a simple macro that just returns its input
        let mut ctx = MacroContext::new();

        use super::super::{
            MacroDef, MacroId, MacroPattern, MacroRule, MetaVarKind, PatternElement,
        };

        let rule = MacroRule {
            pattern: MacroPattern {
                elements: vec![PatternElement::MetaVar {
                    name: "e".into(),
                    kind: MetaVarKind::TokenTree,
                }],
            },
            expansion: MacroExpansion {
                elements: vec![ExpansionElement::MetaVar("e".into())],
            },
            span: Span::dummy(),
        };

        let def = MacroDef {
            id: MacroId::fresh(),
            name: "identity".into(),
            rules: vec![rule],
            is_exported: false,
            span: Span::dummy(),
        };

        ctx.register_macro(def);

        // Expand
        let input = lex("42");
        let result = expand_macro(&ctx, "identity", &input, Span::dummy()).unwrap();

        assert_eq!(result.len(), 1); // Just "42" (EOF is filtered out)
    }
}
