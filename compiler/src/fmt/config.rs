// ===============================================================================
// QUANTALANG FORMATTER CONFIGURATION
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Formatter configuration and options.

// =============================================================================
// FORMAT CONFIG
// =============================================================================

/// Formatter configuration.
#[derive(Debug, Clone)]
pub struct FormatConfig {
    /// Maximum line length before wrapping.
    pub max_line_length: usize,
    /// Indentation width in spaces.
    pub indent_width: usize,
    /// Use tabs instead of spaces.
    pub use_tabs: bool,
    /// Tab width (for display purposes).
    pub tab_width: usize,
    /// Put trailing commas in multi-line constructs.
    pub trailing_comma: TrailingComma,
    /// Newline style.
    pub newline: NewlineStyle,
    /// Brace style.
    pub brace_style: BraceStyle,
    /// How to format imports.
    pub import_style: ImportStyle,
    /// How to format arrays/slices.
    pub array_style: ArrayStyle,
    /// How to format function calls.
    pub fn_call_style: FnCallStyle,
    /// Whether to format doc comments.
    pub format_doc_comments: bool,
    /// Whether to format strings.
    pub format_strings: bool,
    /// Whether to normalize spacing.
    pub normalize_spacing: bool,
    /// Whether to remove trailing whitespace.
    pub trim_trailing_whitespace: bool,
    /// Whether to ensure final newline.
    pub final_newline: bool,
    /// Blank lines before items.
    pub blank_lines_before_items: usize,
    /// Maximum consecutive blank lines.
    pub max_blank_lines: usize,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            max_line_length: 100,
            indent_width: 4,
            use_tabs: false,
            tab_width: 4,
            trailing_comma: TrailingComma::Multiline,
            newline: NewlineStyle::Unix,
            brace_style: BraceStyle::SameLine,
            import_style: ImportStyle::Merged,
            array_style: ArrayStyle::Visual,
            fn_call_style: FnCallStyle::Visual,
            format_doc_comments: true,
            format_strings: false,
            normalize_spacing: true,
            trim_trailing_whitespace: true,
            final_newline: true,
            blank_lines_before_items: 1,
            max_blank_lines: 2,
        }
    }
}

impl FormatConfig {
    /// Create a compact configuration (minimal whitespace).
    pub fn compact() -> Self {
        Self {
            max_line_length: 120,
            indent_width: 2,
            trailing_comma: TrailingComma::Never,
            blank_lines_before_items: 0,
            max_blank_lines: 1,
            ..Default::default()
        }
    }

    /// Create a wide configuration (more whitespace).
    pub fn wide() -> Self {
        Self {
            max_line_length: 80,
            indent_width: 4,
            trailing_comma: TrailingComma::Always,
            blank_lines_before_items: 2,
            max_blank_lines: 3,
            ..Default::default()
        }
    }

    /// Get the indentation string for one level.
    pub fn indent_str(&self) -> String {
        if self.use_tabs {
            "\t".to_string()
        } else {
            " ".repeat(self.indent_width)
        }
    }

    /// Get indentation for a given depth.
    pub fn indent_at(&self, depth: usize) -> String {
        self.indent_str().repeat(depth)
    }

    /// Get the newline string.
    pub fn newline_str(&self) -> &'static str {
        match self.newline {
            NewlineStyle::Unix => "\n",
            NewlineStyle::Windows => "\r\n",
            NewlineStyle::Native => {
                #[cfg(windows)]
                {
                    "\r\n"
                }
                #[cfg(not(windows))]
                {
                    "\n"
                }
            }
        }
    }
}

// =============================================================================
// OPTIONS
// =============================================================================

/// Trailing comma style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrailingComma {
    /// Never add trailing commas.
    Never,
    /// Always add trailing commas.
    Always,
    /// Add trailing commas only in multi-line constructs.
    Multiline,
}

/// Newline style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewlineStyle {
    /// Unix style (LF).
    Unix,
    /// Windows style (CRLF).
    Windows,
    /// Use platform native.
    Native,
}

/// Brace style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BraceStyle {
    /// Opening brace on same line as declaration.
    SameLine,
    /// Opening brace on its own line.
    NextLine,
    /// Like SameLine, but else/elif on same line as closing brace.
    PreferSameLine,
}

/// Import formatting style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportStyle {
    /// Keep imports as-is.
    Preserve,
    /// Merge imports from the same crate.
    Merged,
    /// One import per line.
    Separate,
}

/// Array/slice formatting style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayStyle {
    /// Visual alignment (elements aligned with opening bracket).
    Visual,
    /// Block style (elements indented from bracket).
    Block,
}

/// Function call formatting style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FnCallStyle {
    /// Visual alignment (args aligned with opening paren).
    Visual,
    /// Block style (args indented from function name).
    Block,
}

// =============================================================================
// CONFIG FILE PARSING
// =============================================================================

/// Parse a configuration from a TOML-like format.
///
/// Example format:
/// ```toml
/// max_line_length = 100
/// indent_width = 4
/// use_tabs = false
/// trailing_comma = "multiline"
/// brace_style = "same_line"
/// ```
pub fn parse_config(content: &str) -> Result<FormatConfig, ConfigError> {
    let mut config = FormatConfig::default();

    for line in content.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }

        // Parse key = value
        let parts: Vec<&str> = line.splitn(2, '=').collect();
        if parts.len() != 2 {
            continue;
        }

        let key = parts[0].trim();
        let value = parts[1].trim().trim_matches('"');

        match key {
            "max_line_length" => {
                config.max_line_length = value
                    .parse()
                    .map_err(|_| ConfigError::InvalidValue(key.to_string(), value.to_string()))?;
            }
            "indent_width" => {
                config.indent_width = value
                    .parse()
                    .map_err(|_| ConfigError::InvalidValue(key.to_string(), value.to_string()))?;
            }
            "use_tabs" => {
                config.use_tabs = value == "true";
            }
            "tab_width" => {
                config.tab_width = value
                    .parse()
                    .map_err(|_| ConfigError::InvalidValue(key.to_string(), value.to_string()))?;
            }
            "trailing_comma" => {
                config.trailing_comma = match value {
                    "never" => TrailingComma::Never,
                    "always" => TrailingComma::Always,
                    "multiline" => TrailingComma::Multiline,
                    _ => {
                        return Err(ConfigError::InvalidValue(
                            key.to_string(),
                            value.to_string(),
                        ))
                    }
                };
            }
            "newline" => {
                config.newline = match value {
                    "unix" | "lf" => NewlineStyle::Unix,
                    "windows" | "crlf" => NewlineStyle::Windows,
                    "native" => NewlineStyle::Native,
                    _ => {
                        return Err(ConfigError::InvalidValue(
                            key.to_string(),
                            value.to_string(),
                        ))
                    }
                };
            }
            "brace_style" => {
                config.brace_style = match value {
                    "same_line" => BraceStyle::SameLine,
                    "next_line" => BraceStyle::NextLine,
                    "prefer_same_line" => BraceStyle::PreferSameLine,
                    _ => {
                        return Err(ConfigError::InvalidValue(
                            key.to_string(),
                            value.to_string(),
                        ))
                    }
                };
            }
            "import_style" => {
                config.import_style = match value {
                    "preserve" => ImportStyle::Preserve,
                    "merged" => ImportStyle::Merged,
                    "separate" => ImportStyle::Separate,
                    _ => {
                        return Err(ConfigError::InvalidValue(
                            key.to_string(),
                            value.to_string(),
                        ))
                    }
                };
            }
            "format_doc_comments" => {
                config.format_doc_comments = value == "true";
            }
            "trim_trailing_whitespace" => {
                config.trim_trailing_whitespace = value == "true";
            }
            "final_newline" => {
                config.final_newline = value == "true";
            }
            "blank_lines_before_items" => {
                config.blank_lines_before_items = value
                    .parse()
                    .map_err(|_| ConfigError::InvalidValue(key.to_string(), value.to_string()))?;
            }
            "max_blank_lines" => {
                config.max_blank_lines = value
                    .parse()
                    .map_err(|_| ConfigError::InvalidValue(key.to_string(), value.to_string()))?;
            }
            _ => {
                // Unknown key - ignore or warn
            }
        }
    }

    Ok(config)
}

/// Configuration error.
#[derive(Debug)]
pub enum ConfigError {
    /// Invalid value for a configuration key.
    InvalidValue(String, String),
    /// Unknown configuration key.
    UnknownKey(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InvalidValue(key, value) => {
                write!(f, "invalid value '{}' for key '{}'", value, key)
            }
            ConfigError::UnknownKey(key) => {
                write!(f, "unknown configuration key '{}'", key)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let content = r#"
            max_line_length = 80
            indent_width = 2
            use_tabs = false
            trailing_comma = "always"
        "#;

        let config = parse_config(content).unwrap();
        assert_eq!(config.max_line_length, 80);
        assert_eq!(config.indent_width, 2);
        assert!(!config.use_tabs);
        assert_eq!(config.trailing_comma, TrailingComma::Always);
    }
}
