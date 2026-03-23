// ===============================================================================
// QUANTALANG FORMATTER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Main formatter implementation for QuantaLang source code.

use super::config::{FormatConfig, BraceStyle, TrailingComma, ImportStyle};
use super::pretty::{Doc, PrettyPrinter};

// =============================================================================
// FORMATTER
// =============================================================================

/// QuantaLang source code formatter.
pub struct Formatter {
    /// Configuration.
    config: FormatConfig,
}

impl Formatter {
    /// Create a new formatter with the given configuration.
    pub fn new(config: FormatConfig) -> Self {
        Self { config }
    }

    /// Create a formatter with default configuration.
    pub fn default_formatter() -> Self {
        Self::new(FormatConfig::default())
    }

    /// Format source code string.
    pub fn format_str(&self, source: &str) -> Result<String, FormatError> {
        let mut output = String::with_capacity(source.len());
        let lines: Vec<&str> = source.lines().collect();

        let mut i = 0;
        let mut in_block_comment = false;
        let mut blank_count = 0;

        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim();

            // Handle blank lines
            if trimmed.is_empty() {
                blank_count += 1;
                if blank_count <= self.config.max_blank_lines {
                    output.push_str(self.config.newline_str());
                }
                i += 1;
                continue;
            }
            blank_count = 0;

            // Handle block comments
            if in_block_comment {
                output.push_str(&self.format_comment_line(line));
                output.push_str(self.config.newline_str());
                if trimmed.ends_with("*/") {
                    in_block_comment = false;
                }
                i += 1;
                continue;
            }

            if trimmed.starts_with("/*") {
                in_block_comment = !trimmed.ends_with("*/");
                output.push_str(&self.format_comment_line(line));
                output.push_str(self.config.newline_str());
                i += 1;
                continue;
            }

            // Handle line comments
            if trimmed.starts_with("//") {
                output.push_str(&self.format_comment_line(line));
                output.push_str(self.config.newline_str());
                i += 1;
                continue;
            }

            // Format the line based on its type
            let (formatted, consumed) = self.format_construct(&lines, i)?;
            output.push_str(&formatted);
            i += consumed;
        }

        // Ensure final newline if configured
        if self.config.final_newline && !output.ends_with('\n') {
            output.push_str(self.config.newline_str());
        }

        Ok(output)
    }

    /// Format a construct starting at line index.
    fn format_construct(&self, lines: &[&str], start: usize) -> Result<(String, usize), FormatError> {
        let line = lines[start];
        let trimmed = line.trim();

        // Use statement
        if trimmed.starts_with("use ") {
            return Ok((self.format_use_statement(trimmed)?, 1));
        }

        // Function definition
        if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ")
            || trimmed.starts_with("async fn ") || trimmed.starts_with("pub async fn ")
        {
            return self.format_function(lines, start);
        }

        // Struct definition
        if trimmed.starts_with("struct ") || trimmed.starts_with("pub struct ") {
            return self.format_struct(lines, start);
        }

        // Enum definition
        if trimmed.starts_with("enum ") || trimmed.starts_with("pub enum ") {
            return self.format_enum(lines, start);
        }

        // Trait definition
        if trimmed.starts_with("trait ") || trimmed.starts_with("pub trait ") {
            return self.format_trait(lines, start);
        }

        // Impl block
        if trimmed.starts_with("impl ") || trimmed.starts_with("impl<") {
            return self.format_impl(lines, start);
        }

        // Const/static
        if trimmed.starts_with("const ") || trimmed.starts_with("pub const ")
            || trimmed.starts_with("static ") || trimmed.starts_with("pub static ")
        {
            return Ok((self.format_const_static(trimmed)?, 1));
        }

        // Type alias
        if trimmed.starts_with("type ") || trimmed.starts_with("pub type ") {
            return Ok((self.format_type_alias(trimmed)?, 1));
        }

        // Module declaration
        if trimmed.starts_with("mod ") || trimmed.starts_with("pub mod ") {
            if trimmed.contains('{') {
                return self.format_module(lines, start);
            }
            return Ok((self.format_mod_decl(trimmed)?, 1));
        }

        // Attribute
        if trimmed.starts_with("#[") || trimmed.starts_with("@[") {
            return Ok((self.format_attribute(trimmed)?, 1));
        }

        // Default: format as a statement
        Ok((self.format_statement(trimmed)?, 1))
    }

    /// Format a use statement.
    fn format_use_statement(&self, line: &str) -> Result<String, FormatError> {
        let mut output = String::new();

        // Normalize spacing
        let normalized = line
            .replace("use  ", "use ")
            .replace(" ::", "::")
            .replace(":: ", "::")
            .replace("{ ", "{")
            .replace(" }", "}")
            .replace(" ,", ",")
            .replace(",  ", ", ");

        output.push_str(&normalized);
        output.push_str(self.config.newline_str());
        Ok(output)
    }

    /// Format a function.
    fn format_function(&self, lines: &[&str], start: usize) -> Result<(String, usize), FormatError> {
        let mut output = String::new();
        let line = lines[start].trim();

        // Find the signature parts
        let sig_end = if let Some(brace_pos) = line.find('{') {
            brace_pos
        } else {
            line.len()
        };

        let signature = &line[..sig_end].trim();
        output.push_str(&self.format_function_signature(signature)?);

        // Handle brace style
        if line.contains('{') {
            match self.config.brace_style {
                BraceStyle::SameLine | BraceStyle::PreferSameLine => {
                    output.push_str(" {");
                }
                BraceStyle::NextLine => {
                    output.push_str(self.config.newline_str());
                    output.push('{');
                }
            }
            output.push_str(self.config.newline_str());

            // Format body
            let (body, end_line) = self.format_block_body(lines, start)?;
            output.push_str(&body);
            output.push('}');
            output.push_str(self.config.newline_str());

            Ok((output, end_line - start + 1))
        } else {
            // Declaration only (trait method)
            output.push_str(self.config.newline_str());
            Ok((output, 1))
        }
    }

    /// Format a function signature.
    fn format_function_signature(&self, sig: &str) -> Result<String, FormatError> {
        let mut output = String::new();

        // Parse parts
        let parts: Vec<&str> = sig.splitn(2, '(').collect();
        if parts.len() < 2 {
            return Ok(sig.to_string());
        }

        let prefix = parts[0].trim();
        let rest = parts[1];

        output.push_str(prefix);
        output.push('(');

        // Find params and return type
        if let Some(paren_end) = rest.find(')') {
            let params = &rest[..paren_end];
            let after_params = &rest[paren_end + 1..];

            // Format parameters
            output.push_str(&self.format_params(params)?);
            output.push(')');

            // Return type
            if let Some(arrow_pos) = after_params.find("->") {
                output.push_str(" -> ");
                let ret_type = after_params[arrow_pos + 2..].trim();
                output.push_str(ret_type);
            }

            // Where clause
            if let Some(where_pos) = after_params.find("where") {
                output.push_str(self.config.newline_str());
                output.push_str(&self.config.indent_str());
                output.push_str(&after_params[where_pos..].trim());
            }
        }

        Ok(output)
    }

    /// Format function parameters.
    fn format_params(&self, params: &str) -> Result<String, FormatError> {
        if params.trim().is_empty() {
            return Ok(String::new());
        }

        let params: Vec<&str> = params.split(',').collect();
        let total_len: usize = params.iter().map(|p| p.trim().len()).sum::<usize>()
            + params.len() * 2; // commas and spaces

        // Check if fits on one line
        if total_len < self.config.max_line_length - 20 {
            Ok(params
                .iter()
                .map(|p| p.trim())
                .collect::<Vec<_>>()
                .join(", "))
        } else {
            // Multi-line
            let mut output = String::new();
            output.push_str(self.config.newline_str());
            for (i, param) in params.iter().enumerate() {
                output.push_str(&self.config.indent_str());
                output.push_str(param.trim());
                if i < params.len() - 1 {
                    output.push(',');
                } else if matches!(self.config.trailing_comma, TrailingComma::Always | TrailingComma::Multiline) {
                    output.push(',');
                }
                output.push_str(self.config.newline_str());
            }
            Ok(output)
        }
    }

    /// Format a struct.
    fn format_struct(&self, lines: &[&str], start: usize) -> Result<(String, usize), FormatError> {
        let mut output = String::new();
        let line = lines[start].trim();

        // Get struct header
        let header_end = line.find('{').unwrap_or(line.len());
        let header = &line[..header_end].trim();
        output.push_str(header);

        if line.contains('{') {
            match self.config.brace_style {
                BraceStyle::SameLine | BraceStyle::PreferSameLine => {
                    output.push_str(" {");
                }
                BraceStyle::NextLine => {
                    output.push_str(self.config.newline_str());
                    output.push('{');
                }
            }
            output.push_str(self.config.newline_str());

            // Format fields
            let (body, end_line) = self.format_struct_fields(lines, start)?;
            output.push_str(&body);
            output.push('}');
            output.push_str(self.config.newline_str());

            Ok((output, end_line - start + 1))
        } else if line.contains('(') {
            // Tuple struct
            output.push_str(self.config.newline_str());
            Ok((output, 1))
        } else {
            // Unit struct
            output.push(';');
            output.push_str(self.config.newline_str());
            Ok((output, 1))
        }
    }

    /// Format struct fields.
    fn format_struct_fields(&self, lines: &[&str], start: usize) -> Result<(String, usize), FormatError> {
        let mut output = String::new();
        let mut depth = 0;
        let mut end_line = start;

        for i in start..lines.len() {
            let line = lines[i];
            for c in line.chars() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        end_line = i;
                        return Ok((output, end_line));
                    }
                }
            }

            if i > start {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('}') {
                    output.push_str(&self.config.indent_str());
                    output.push_str(&self.format_field(trimmed)?);
                    output.push_str(self.config.newline_str());
                }
            }
        }

        Ok((output, end_line))
    }

    /// Format a struct field.
    fn format_field(&self, field: &str) -> Result<String, FormatError> {
        // Normalize spacing around colon
        let normalized = field
            .replace(" :", ":")
            .replace(":  ", ": ")
            .replace(": ", ": ");

        // Handle trailing comma
        let trimmed = normalized.trim_end_matches(',').trim();
        let mut output = trimmed.to_string();

        // Add trailing comma if configured
        if matches!(self.config.trailing_comma, TrailingComma::Always | TrailingComma::Multiline) {
            output.push(',');
        }

        Ok(output)
    }

    /// Format an enum.
    fn format_enum(&self, lines: &[&str], start: usize) -> Result<(String, usize), FormatError> {
        let mut output = String::new();
        let line = lines[start].trim();

        let header_end = line.find('{').unwrap_or(line.len());
        let header = &line[..header_end].trim();
        output.push_str(header);

        if line.contains('{') {
            match self.config.brace_style {
                BraceStyle::SameLine | BraceStyle::PreferSameLine => {
                    output.push_str(" {");
                }
                BraceStyle::NextLine => {
                    output.push_str(self.config.newline_str());
                    output.push('{');
                }
            }
            output.push_str(self.config.newline_str());

            let (body, end_line) = self.format_enum_variants(lines, start)?;
            output.push_str(&body);
            output.push('}');
            output.push_str(self.config.newline_str());

            Ok((output, end_line - start + 1))
        } else {
            output.push_str(self.config.newline_str());
            Ok((output, 1))
        }
    }

    /// Format enum variants.
    fn format_enum_variants(&self, lines: &[&str], start: usize) -> Result<(String, usize), FormatError> {
        let mut output = String::new();
        let mut depth = 0;
        let mut end_line = start;

        for i in start..lines.len() {
            let line = lines[i];
            for c in line.chars() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        end_line = i;
                        return Ok((output, end_line));
                    }
                }
            }

            if i > start && depth == 1 {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('}') {
                    output.push_str(&self.config.indent_str());
                    output.push_str(&self.format_variant(trimmed)?);
                    output.push_str(self.config.newline_str());
                }
            }
        }

        Ok((output, end_line))
    }

    /// Format an enum variant.
    fn format_variant(&self, variant: &str) -> Result<String, FormatError> {
        let trimmed = variant.trim_end_matches(',').trim();
        let mut output = trimmed.to_string();

        if matches!(self.config.trailing_comma, TrailingComma::Always | TrailingComma::Multiline) {
            output.push(',');
        }

        Ok(output)
    }

    /// Format a trait.
    fn format_trait(&self, lines: &[&str], start: usize) -> Result<(String, usize), FormatError> {
        // Similar to struct but with method formatting
        self.format_impl_like(lines, start, "trait")
    }

    /// Format an impl block.
    fn format_impl(&self, lines: &[&str], start: usize) -> Result<(String, usize), FormatError> {
        self.format_impl_like(lines, start, "impl")
    }

    /// Format trait-like or impl-like block.
    fn format_impl_like(&self, lines: &[&str], start: usize, kind: &str) -> Result<(String, usize), FormatError> {
        let mut output = String::new();
        let line = lines[start].trim();

        let header_end = line.find('{').unwrap_or(line.len());
        let header = &line[..header_end].trim();
        output.push_str(header);

        if line.contains('{') {
            match self.config.brace_style {
                BraceStyle::SameLine | BraceStyle::PreferSameLine => {
                    output.push_str(" {");
                }
                BraceStyle::NextLine => {
                    output.push_str(self.config.newline_str());
                    output.push('{');
                }
            }
            output.push_str(self.config.newline_str());

            let (body, end_line) = self.format_block_body(lines, start)?;
            output.push_str(&body);
            output.push('}');
            output.push_str(self.config.newline_str());

            Ok((output, end_line - start + 1))
        } else {
            output.push_str(self.config.newline_str());
            Ok((output, 1))
        }
    }

    /// Format a block body.
    fn format_block_body(&self, lines: &[&str], start: usize) -> Result<(String, usize), FormatError> {
        let mut output = String::new();
        let mut depth = 0;
        let mut end_line = start;

        for i in start..lines.len() {
            let line = lines[i];
            let mut in_string = false;

            for c in line.chars() {
                if c == '"' {
                    in_string = !in_string;
                } else if !in_string {
                    if c == '{' {
                        depth += 1;
                    } else if c == '}' {
                        depth -= 1;
                        if depth == 0 {
                            end_line = i;
                            return Ok((output, end_line));
                        }
                    }
                }
            }

            if i > start {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('}') {
                    output.push_str(&self.config.indent_str());
                    output.push_str(&self.format_statement(trimmed)?);
                    output.push_str(self.config.newline_str());
                }
            }
        }

        Ok((output, end_line))
    }

    /// Format a module.
    fn format_module(&self, lines: &[&str], start: usize) -> Result<(String, usize), FormatError> {
        self.format_impl_like(lines, start, "mod")
    }

    /// Format a module declaration.
    fn format_mod_decl(&self, line: &str) -> Result<String, FormatError> {
        let mut output = line.trim().to_string();
        if !output.ends_with(';') {
            output.push(';');
        }
        output.push_str(self.config.newline_str());
        Ok(output)
    }

    /// Format a const or static declaration.
    fn format_const_static(&self, line: &str) -> Result<String, FormatError> {
        let normalized = line
            .replace(" :", ":")
            .replace(":  ", ": ")
            .replace(" =", " =")
            .replace("=  ", "= ");

        let mut output = normalized.trim().to_string();
        if !output.ends_with(';') {
            output.push(';');
        }
        output.push_str(self.config.newline_str());
        Ok(output)
    }

    /// Format a type alias.
    fn format_type_alias(&self, line: &str) -> Result<String, FormatError> {
        let normalized = line
            .replace(" =", " =")
            .replace("=  ", "= ");

        let mut output = normalized.trim().to_string();
        if !output.ends_with(';') {
            output.push(';');
        }
        output.push_str(self.config.newline_str());
        Ok(output)
    }

    /// Format an attribute.
    fn format_attribute(&self, line: &str) -> Result<String, FormatError> {
        let mut output = line.trim().to_string();
        output.push_str(self.config.newline_str());
        Ok(output)
    }

    /// Format a statement.
    fn format_statement(&self, line: &str) -> Result<String, FormatError> {
        let mut output = String::new();

        // Normalize operator spacing
        let normalized = self.normalize_operators(line);

        output.push_str(&normalized);

        Ok(output)
    }

    /// Format a comment line.
    fn format_comment_line(&self, line: &str) -> String {
        if self.config.trim_trailing_whitespace {
            line.trim_end().to_string()
        } else {
            line.to_string()
        }
    }

    /// Normalize spacing around operators.
    fn normalize_operators(&self, s: &str) -> String {
        if !self.config.normalize_spacing {
            return s.to_string();
        }

        let mut result = s.to_string();

        // Binary operators that need spaces
        let operators = [
            ("==", " == "),
            ("!=", " != "),
            ("<=", " <= "),
            (">=", " >= "),
            ("&&", " && "),
            ("||", " || "),
            ("+=", " += "),
            ("-=", " -= "),
            ("*=", " *= "),
            ("/=", " /= "),
            ("->", " -> "),
            ("=>", " => "),
        ];

        for (op, spaced) in operators {
            result = result.replace(op, spaced);
        }

        // Clean up extra spaces
        while result.contains("  ") {
            result = result.replace("  ", " ");
        }

        result
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self::default_formatter()
    }
}

// =============================================================================
// FORMAT ERROR
// =============================================================================

/// Formatting error.
#[derive(Debug)]
pub enum FormatError {
    /// Syntax error in source.
    SyntaxError(String),
    /// IO error.
    IoError(std::io::Error),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::SyntaxError(msg) => write!(f, "syntax error: {}", msg),
            FormatError::IoError(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for FormatError {}

impl From<std::io::Error> for FormatError {
    fn from(err: std::io::Error) -> Self {
        FormatError::IoError(err)
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_simple_fn() {
        let formatter = Formatter::default();
        let input = "fn foo(){}";
        let result = formatter.format_str(input).unwrap();
        assert!(result.contains("fn foo()"));
        assert!(result.contains("{"));
    }

    #[test]
    fn test_format_use() {
        let formatter = Formatter::default();
        let input = "use  std::collections::HashMap;";
        let result = formatter.format_str(input).unwrap();
        assert_eq!(result.trim(), "use std::collections::HashMap;");
    }

    #[test]
    fn test_normalize_operators() {
        let formatter = Formatter::default();
        let input = "x==y";
        let result = formatter.normalize_operators(input);
        assert_eq!(result, "x == y");
    }
}
