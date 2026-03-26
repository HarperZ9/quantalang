// ===============================================================================
// QUANTALANG MACRO EXPANSION - PATTERN MATCHING
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Macro pattern matching and binding.
//!
//! This module implements the pattern matching algorithm for macro_rules! macros.

use std::collections::HashMap;
use std::sync::Arc;

use crate::lexer::{TokenKind, Delimiter, Keyword};

use super::{
    MacroPattern, PatternElement, MetaVarKind, RepetitionKind,
    TokenTree, MacroError, MacroResult,
};

// =============================================================================
// MACRO BINDINGS
// =============================================================================

/// A bound value from pattern matching.
#[derive(Debug, Clone)]
pub enum Binding {
    /// A single value.
    Single(BindingValue),
    /// Multiple values from a repetition.
    Repeated(Vec<Binding>),
}

/// A single bound value.
#[derive(Debug, Clone)]
pub enum BindingValue {
    /// A token tree.
    TokenTree(TokenTree),
    /// Multiple token trees.
    TokenTrees(Vec<TokenTree>),
}

impl Binding {
    /// Get as a single value.
    pub fn as_single(&self) -> Option<&BindingValue> {
        match self {
            Binding::Single(v) => Some(v),
            _ => None,
        }
    }

    /// Get as repeated values.
    pub fn as_repeated(&self) -> Option<&[Binding]> {
        match self {
            Binding::Repeated(v) => Some(v),
            _ => None,
        }
    }

    /// Get the number of repetitions (1 for single values).
    pub fn count(&self) -> usize {
        match self {
            Binding::Single(_) => 1,
            Binding::Repeated(v) => v.len(),
        }
    }
}

/// Collection of bindings from pattern matching.
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    pub(crate) bindings: HashMap<Arc<str>, Binding>,
}

impl Bindings {
    /// Create empty bindings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a binding.
    pub fn insert(&mut self, name: Arc<str>, binding: Binding) {
        self.bindings.insert(name, binding);
    }

    /// Get a binding.
    pub fn get(&self, name: &str) -> Option<&Binding> {
        self.bindings.get(name)
    }

    /// Merge another set of bindings into this one.
    pub fn merge(&mut self, other: Bindings) {
        self.bindings.extend(other.bindings);
    }
}

// =============================================================================
// PATTERN MATCHER
// =============================================================================

/// The macro pattern matcher.
pub struct PatternMatcher<'a> {
    /// The input token trees.
    input: &'a [TokenTree],
    /// Current position in the input.
    pos: usize,
}

impl<'a> PatternMatcher<'a> {
    /// Create a new pattern matcher.
    pub fn new(input: &'a [TokenTree]) -> Self {
        Self { input, pos: 0 }
    }

    /// Match a pattern against the input.
    pub fn match_pattern(&mut self, pattern: &MacroPattern) -> MacroResult<Bindings> {
        let mut bindings = Bindings::new();

        for element in &pattern.elements {
            self.match_element(element, &mut bindings)?;
        }

        // Check that all input was consumed
        if self.pos < self.input.len() {
            return Err(MacroError::UnexpectedToken {
                expected: "end of macro input".to_string(),
                found: self.current_kind().clone(),
            });
        }

        Ok(bindings)
    }

    /// Match a single pattern element.
    fn match_element(&mut self, element: &PatternElement, bindings: &mut Bindings) -> MacroResult<()> {
        match element {
            PatternElement::Token(kind) => {
                self.expect_token(kind)?;
            }
            PatternElement::MetaVar { name, kind } => {
                let value = self.match_metavar(*kind)?;
                bindings.insert(name.clone(), Binding::Single(value));
            }
            PatternElement::Repetition { elements, separator, repetition } => {
                self.match_repetition(elements, separator.as_ref(), *repetition, bindings)?;
            }
            PatternElement::Delimited { delimiter, elements } => {
                self.match_delimited(*delimiter, elements, bindings)?;
            }
        }
        Ok(())
    }

    /// Match a metavariable.
    fn match_metavar(&mut self, kind: MetaVarKind) -> MacroResult<BindingValue> {
        if self.pos >= self.input.len() {
            return Err(MacroError::UnexpectedToken {
                expected: format!("{:?}", kind),
                found: TokenKind::Eof,
            });
        }

        match kind {
            MetaVarKind::TokenTree => {
                let tt = self.input[self.pos].clone();
                self.pos += 1;
                Ok(BindingValue::TokenTree(tt))
            }
            MetaVarKind::Ident => {
                self.expect_ident()?;
                let tt = self.input[self.pos - 1].clone();
                Ok(BindingValue::TokenTree(tt))
            }
            MetaVarKind::Literal => {
                self.expect_literal()?;
                let tt = self.input[self.pos - 1].clone();
                Ok(BindingValue::TokenTree(tt))
            }
            MetaVarKind::Lifetime => {
                self.expect_lifetime()?;
                let tt = self.input[self.pos - 1].clone();
                Ok(BindingValue::TokenTree(tt))
            }
            MetaVarKind::Expr | MetaVarKind::Type | MetaVarKind::Path
            | MetaVarKind::Pat | MetaVarKind::Stmt | MetaVarKind::Block
            | MetaVarKind::Item | MetaVarKind::Meta | MetaVarKind::Vis => {
                // For complex fragments, collect tokens until a delimiter or end
                let trees = self.collect_fragment(kind)?;
                Ok(BindingValue::TokenTrees(trees))
            }
        }
    }

    /// Match a repetition pattern.
    fn match_repetition(
        &mut self,
        elements: &[PatternElement],
        separator: Option<&TokenKind>,
        repetition: RepetitionKind,
        bindings: &mut Bindings,
    ) -> MacroResult<()> {
        let mut all_bindings: HashMap<Arc<str>, Vec<Binding>> = HashMap::new();
        let mut count = 0;

        loop {
            // Check if we should stop
            let can_match = self.can_match_elements(elements);
            if !can_match {
                break;
            }

            // Match the elements
            let mut iter_bindings = Bindings::new();
            let start_pos = self.pos;

            let matched = self.try_match_elements(elements, &mut iter_bindings);
            if !matched {
                self.pos = start_pos;
                break;
            }

            // Collect bindings
            for (name, binding) in iter_bindings.bindings {
                all_bindings.entry(name).or_default().push(binding);
            }

            count += 1;

            // Handle separator
            if let Some(sep) = separator {
                if self.check_token(sep) {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }

        // Check repetition requirements
        match repetition {
            RepetitionKind::ZeroOrMore => {}
            RepetitionKind::OneOrMore => {
                if count == 0 {
                    return Err(MacroError::UnexpectedToken {
                        expected: "at least one repetition".to_string(),
                        found: self.current_kind().clone(),
                    });
                }
            }
            RepetitionKind::ZeroOrOne => {
                if count > 1 {
                    return Err(MacroError::UnexpectedToken {
                        expected: "at most one repetition".to_string(),
                        found: self.current_kind().clone(),
                    });
                }
            }
        }

        // Add collected bindings
        for (name, values) in all_bindings {
            bindings.insert(name, Binding::Repeated(values));
        }

        Ok(())
    }

    /// Match a delimited group.
    fn match_delimited(
        &mut self,
        delimiter: Delimiter,
        elements: &[PatternElement],
        bindings: &mut Bindings,
    ) -> MacroResult<()> {
        if self.pos >= self.input.len() {
            return Err(MacroError::UnexpectedToken {
                expected: format!("{:?}", delimiter),
                found: TokenKind::Eof,
            });
        }

        match &self.input[self.pos] {
            TokenTree::Delimited { delimiter: d, tokens, .. } if *d == delimiter => {
                self.pos += 1;

                // Match inner elements
                let mut inner_matcher = PatternMatcher::new(tokens);
                for element in elements {
                    inner_matcher.match_element(element, bindings)?;
                }

                Ok(())
            }
            _ => Err(MacroError::UnexpectedToken {
                expected: format!("{:?}", delimiter),
                found: self.current_kind().clone(),
            }),
        }
    }

    /// Try to match elements, returning false if it fails.
    fn try_match_elements(&mut self, elements: &[PatternElement], bindings: &mut Bindings) -> bool {
        for element in elements {
            if self.match_element(element, bindings).is_err() {
                return false;
            }
        }
        true
    }

    /// Check if we can potentially match the elements.
    fn can_match_elements(&self, elements: &[PatternElement]) -> bool {
        if self.pos >= self.input.len() {
            return false;
        }

        if let Some(first) = elements.first() {
            self.can_match_element(first)
        } else {
            true
        }
    }

    /// Check if we can potentially match an element.
    fn can_match_element(&self, element: &PatternElement) -> bool {
        if self.pos >= self.input.len() {
            return false;
        }

        match element {
            PatternElement::Token(kind) => self.check_token(kind),
            PatternElement::MetaVar { kind, .. } => self.can_match_metavar(*kind),
            PatternElement::Repetition { .. } => true,
            PatternElement::Delimited { delimiter, .. } => {
                matches!(&self.input[self.pos], TokenTree::Delimited { delimiter: d, .. } if *d == *delimiter)
            }
        }
    }

    /// Check if we can match a metavariable kind.
    fn can_match_metavar(&self, kind: MetaVarKind) -> bool {
        if self.pos >= self.input.len() {
            return false;
        }

        match kind {
            MetaVarKind::TokenTree => true,
            MetaVarKind::Ident => matches!(
                self.current_kind(),
                TokenKind::Ident | TokenKind::RawIdent | TokenKind::Keyword(_)
            ),
            MetaVarKind::Literal => matches!(self.current_kind(), TokenKind::Literal { .. }),
            MetaVarKind::Lifetime => matches!(self.current_kind(), TokenKind::Lifetime),
            _ => true, // Complex fragments can start with many tokens
        }
    }

    /// Collect tokens for a complex fragment.
    fn collect_fragment(&mut self, kind: MetaVarKind) -> MacroResult<Vec<TokenTree>> {
        let mut trees = Vec::new();

        // Collect until we hit something that definitely ends the fragment
        while self.pos < self.input.len() {
            if self.at_fragment_end(kind) {
                break;
            }
            trees.push(self.input[self.pos].clone());
            self.pos += 1;
        }

        if trees.is_empty() {
            return Err(MacroError::UnexpectedToken {
                expected: format!("{:?}", kind),
                found: self.current_kind().clone(),
            });
        }

        Ok(trees)
    }

    /// Check if we're at the end of a fragment.
    fn at_fragment_end(&self, kind: MetaVarKind) -> bool {
        if self.pos >= self.input.len() {
            return true;
        }

        let token = self.current_kind();

        match kind {
            MetaVarKind::Expr | MetaVarKind::Stmt => {
                // Expressions/statements end at ; , => and closing delimiters
                matches!(token,
                    TokenKind::Semi | TokenKind::Comma | TokenKind::FatArrow
                    | TokenKind::CloseDelim(_)
                )
            }
            MetaVarKind::Type | MetaVarKind::Path => {
                // Types/paths end at , ; = > and closing delimiters
                matches!(token,
                    TokenKind::Comma | TokenKind::Semi | TokenKind::Eq
                    | TokenKind::Gt | TokenKind::CloseDelim(_)
                )
            }
            MetaVarKind::Pat => {
                // Patterns end at = | if and closing delimiters
                matches!(token,
                    TokenKind::Eq | TokenKind::Or | TokenKind::CloseDelim(_)
                ) || matches!(token, TokenKind::Keyword(Keyword::If))
            }
            _ => {
                // Default: end at common delimiters
                matches!(token,
                    TokenKind::Semi | TokenKind::Comma | TokenKind::CloseDelim(_)
                )
            }
        }
    }

    /// Expect a specific token.
    fn expect_token(&mut self, expected: &TokenKind) -> MacroResult<()> {
        if !self.check_token(expected) {
            return Err(MacroError::UnexpectedToken {
                expected: format!("{:?}", expected),
                found: self.current_kind().clone(),
            });
        }
        self.pos += 1;
        Ok(())
    }

    /// Expect an identifier.
    fn expect_ident(&mut self) -> MacroResult<()> {
        match self.current_kind() {
            TokenKind::Ident | TokenKind::RawIdent => {
                self.pos += 1;
                Ok(())
            }
            _ => Err(MacroError::UnexpectedToken {
                expected: "identifier".to_string(),
                found: self.current_kind().clone(),
            }),
        }
    }

    /// Expect a literal.
    fn expect_literal(&mut self) -> MacroResult<()> {
        match self.current_kind() {
            TokenKind::Literal { .. } => {
                self.pos += 1;
                Ok(())
            }
            _ => Err(MacroError::UnexpectedToken {
                expected: "literal".to_string(),
                found: self.current_kind().clone(),
            }),
        }
    }

    /// Expect a lifetime.
    fn expect_lifetime(&mut self) -> MacroResult<()> {
        match self.current_kind() {
            TokenKind::Lifetime => {
                self.pos += 1;
                Ok(())
            }
            _ => Err(MacroError::UnexpectedToken {
                expected: "lifetime".to_string(),
                found: self.current_kind().clone(),
            }),
        }
    }

    /// Check if the current token matches.
    fn check_token(&self, expected: &TokenKind) -> bool {
        if self.pos >= self.input.len() {
            return false;
        }
        match &self.input[self.pos] {
            TokenTree::Token(t) => &t.kind == expected,
            _ => false,
        }
    }

    /// Get the current token kind.
    fn current_kind(&self) -> TokenKind {
        if self.pos >= self.input.len() {
            TokenKind::Eof
        } else {
            match &self.input[self.pos] {
                TokenTree::Token(t) => t.kind.clone(),
                TokenTree::Delimited { delimiter, .. } => TokenKind::OpenDelim(*delimiter),
            }
        }
    }
}

/// Match a pattern against token trees.
pub fn match_macro_pattern(pattern: &MacroPattern, input: &[TokenTree]) -> MacroResult<Bindings> {
    let mut matcher = PatternMatcher::new(input);
    matcher.match_pattern(pattern)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{SourceFile, Lexer, Token};
    use crate::macro_expand::tokens_to_tree;

    fn lex(source: &str) -> Vec<Token> {
        let file = SourceFile::anonymous(source);
        let mut lexer = Lexer::new(&file);
        lexer.tokenize().unwrap()
    }

    fn make_pattern(elements: Vec<PatternElement>) -> MacroPattern {
        MacroPattern { elements }
    }

    #[test]
    fn test_match_literal_token() {
        let tokens = lex("foo");
        let trees = tokens_to_tree(&tokens);

        let pattern = make_pattern(vec![
            PatternElement::Token(TokenKind::Ident),
        ]);

        let bindings = match_macro_pattern(&pattern, &trees).unwrap();
        assert!(bindings.bindings.is_empty());
    }

    #[test]
    fn test_match_metavar_ident() {
        let tokens = lex("foo");
        let trees = tokens_to_tree(&tokens);

        let pattern = make_pattern(vec![
            PatternElement::MetaVar {
                name: "x".into(),
                kind: MetaVarKind::Ident,
            },
        ]);

        let bindings = match_macro_pattern(&pattern, &trees).unwrap();
        assert!(bindings.get("x").is_some());
    }

    #[test]
    fn test_match_metavar_tt() {
        let tokens = lex("(1 + 2)");
        let trees = tokens_to_tree(&tokens);

        let pattern = make_pattern(vec![
            PatternElement::MetaVar {
                name: "e".into(),
                kind: MetaVarKind::TokenTree,
            },
        ]);

        let bindings = match_macro_pattern(&pattern, &trees).unwrap();
        assert!(bindings.get("e").is_some());
    }
}
