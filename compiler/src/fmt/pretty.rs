// ===============================================================================
// QUANTALANG PRETTY PRINTER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Pretty printer for generating formatted output.

use super::config::FormatConfig;

// =============================================================================
// DOCUMENT ELEMENTS
// =============================================================================

/// A pretty-printable document element.
#[derive(Debug, Clone)]
pub enum Doc {
    /// Empty document.
    Nil,
    /// A text fragment.
    Text(String),
    /// A newline (may be flattened to space).
    Line,
    /// A hard line break (never flattened).
    HardLine,
    /// A soft line break (flattens to empty).
    SoftLine,
    /// Concatenation of documents.
    Concat(Vec<Doc>),
    /// Grouped document (may be flattened).
    Group(Box<Doc>),
    /// Nested/indented document.
    Nest(usize, Box<Doc>),
    /// Aligned document (aligns to current column).
    Align(Box<Doc>),
    /// Fill (line breaking word wrapping).
    Fill(Vec<Doc>),
    /// Conditional (different based on flat mode).
    IfFlat(Box<Doc>, Box<Doc>),
    /// Line suffix (printed after line content).
    LineSuffix(String),
}

impl Doc {
    /// Create text element.
    pub fn text(s: impl Into<String>) -> Self {
        Doc::Text(s.into())
    }

    /// Create newline element.
    pub fn line() -> Self {
        Doc::Line
    }

    /// Create hard line break.
    pub fn hard_line() -> Self {
        Doc::HardLine
    }

    /// Create soft line break.
    pub fn soft_line() -> Self {
        Doc::SoftLine
    }

    /// Create concatenation.
    pub fn concat(docs: Vec<Doc>) -> Self {
        Doc::Concat(docs)
    }

    /// Create group.
    pub fn group(doc: Doc) -> Self {
        Doc::Group(Box::new(doc))
    }

    /// Create nested document.
    pub fn nest(indent: usize, doc: Doc) -> Self {
        Doc::Nest(indent, Box::new(doc))
    }

    /// Create aligned document.
    pub fn align(doc: Doc) -> Self {
        Doc::Align(Box::new(doc))
    }

    /// Create fill document.
    pub fn fill(docs: Vec<Doc>) -> Self {
        Doc::Fill(docs)
    }

    /// Create conditional document.
    pub fn if_flat(flat: Doc, break_: Doc) -> Self {
        Doc::IfFlat(Box::new(flat), Box::new(break_))
    }

    /// Join documents with separator.
    pub fn join(docs: Vec<Doc>, sep: Doc) -> Self {
        let mut result = Vec::new();
        for (i, doc) in docs.into_iter().enumerate() {
            if i > 0 {
                result.push(sep.clone());
            }
            result.push(doc);
        }
        Doc::concat(result)
    }

    /// Intersperse documents with separator.
    pub fn intersperse(docs: Vec<Doc>, sep: Doc) -> Self {
        Self::join(docs, sep)
    }

    /// Surround with brackets.
    pub fn brackets(left: &str, doc: Doc, right: &str) -> Self {
        Doc::concat(vec![Doc::text(left), doc, Doc::text(right)])
    }

    /// Surround with parentheses.
    pub fn parens(doc: Doc) -> Self {
        Self::brackets("(", doc, ")")
    }

    /// Surround with square brackets.
    pub fn square(doc: Doc) -> Self {
        Self::brackets("[", doc, "]")
    }

    /// Surround with curly braces.
    pub fn braces(doc: Doc) -> Self {
        Self::brackets("{", doc, "}")
    }
}

// =============================================================================
// PRETTY PRINTER
// =============================================================================

/// Pretty printer for converting Doc to string.
pub struct PrettyPrinter {
    /// Configuration.
    config: FormatConfig,
    /// Output buffer.
    output: String,
    /// Current column.
    column: usize,
    /// Current indentation.
    indent: usize,
    /// Line suffixes to print.
    line_suffixes: Vec<String>,
}

impl PrettyPrinter {
    /// Create a new pretty printer.
    pub fn new(config: FormatConfig) -> Self {
        Self {
            config,
            output: String::new(),
            column: 0,
            indent: 0,
            line_suffixes: Vec::new(),
        }
    }

    /// Print a document to string.
    pub fn print(&mut self, doc: &Doc) -> String {
        self.output.clear();
        self.column = 0;
        self.indent = 0;
        self.line_suffixes.clear();

        // Calculate whether we should flatten
        let fits = self.fits(doc, self.config.max_line_length, true);
        self.print_doc(doc, fits);

        // Print any remaining line suffixes
        self.flush_line_suffixes();

        // Ensure final newline if configured
        if self.config.final_newline && !self.output.ends_with('\n') {
            self.output.push_str(self.config.newline_str());
        }

        std::mem::take(&mut self.output)
    }

    /// Check if document fits within width.
    fn fits(&self, doc: &Doc, width: usize, flat: bool) -> bool {
        self.fits_inner(doc, width as isize, flat) >= 0
    }

    fn fits_inner(&self, doc: &Doc, mut width: isize, flat: bool) -> isize {
        match doc {
            Doc::Nil => width,
            Doc::Text(s) => width - s.len() as isize,
            Doc::Line => {
                if flat {
                    width - 1 // space
                } else {
                    width // break fits
                }
            }
            Doc::HardLine => width, // hard break always fits
            Doc::SoftLine => width, // soft line flattens to nothing
            Doc::Concat(docs) => {
                for d in docs {
                    width = self.fits_inner(d, width, flat);
                    if width < 0 {
                        return width;
                    }
                }
                width
            }
            Doc::Group(d) => self.fits_inner(d, width, true),
            Doc::Nest(_, d) => self.fits_inner(d, width, flat),
            Doc::Align(d) => self.fits_inner(d, width, flat),
            Doc::Fill(docs) => {
                for d in docs {
                    width = self.fits_inner(d, width, flat);
                    if width < 0 {
                        return width;
                    }
                }
                width
            }
            Doc::IfFlat(f, b) => {
                if flat {
                    self.fits_inner(f, width, flat)
                } else {
                    self.fits_inner(b, width, flat)
                }
            }
            Doc::LineSuffix(_) => width,
        }
    }

    /// Print a document element.
    fn print_doc(&mut self, doc: &Doc, flat: bool) {
        match doc {
            Doc::Nil => {}
            Doc::Text(s) => {
                self.output.push_str(s);
                self.column += s.len();
            }
            Doc::Line => {
                if flat {
                    self.output.push(' ');
                    self.column += 1;
                } else {
                    self.print_newline();
                }
            }
            Doc::HardLine => {
                self.print_newline();
            }
            Doc::SoftLine => {
                if !flat {
                    self.print_newline();
                }
            }
            Doc::Concat(docs) => {
                for d in docs {
                    self.print_doc(d, flat);
                }
            }
            Doc::Group(d) => {
                let group_flat = flat || self.fits(d, self.config.max_line_length - self.column, true);
                self.print_doc(d, group_flat);
            }
            Doc::Nest(n, d) => {
                self.indent += n;
                self.print_doc(d, flat);
                self.indent -= n;
            }
            Doc::Align(d) => {
                let old_indent = self.indent;
                self.indent = self.column;
                self.print_doc(d, flat);
                self.indent = old_indent;
            }
            Doc::Fill(docs) => {
                for (i, d) in docs.iter().enumerate() {
                    if i > 0 {
                        // Check if we need to break
                        if self.column + self.doc_width(d) > self.config.max_line_length {
                            self.print_newline();
                        } else {
                            self.output.push(' ');
                            self.column += 1;
                        }
                    }
                    self.print_doc(d, true);
                }
            }
            Doc::IfFlat(f, b) => {
                if flat {
                    self.print_doc(f, flat);
                } else {
                    self.print_doc(b, flat);
                }
            }
            Doc::LineSuffix(s) => {
                self.line_suffixes.push(s.clone());
            }
        }
    }

    /// Print a newline with indentation.
    fn print_newline(&mut self) {
        // Flush line suffixes
        self.flush_line_suffixes();

        // Trim trailing whitespace if configured
        if self.config.trim_trailing_whitespace {
            while self.output.ends_with(' ') || self.output.ends_with('\t') {
                self.output.pop();
            }
        }

        self.output.push_str(self.config.newline_str());
        self.output.push_str(&self.config.indent_at(self.indent / self.config.indent_width));
        self.column = self.indent;
    }

    /// Flush line suffixes.
    fn flush_line_suffixes(&mut self) {
        for suffix in std::mem::take(&mut self.line_suffixes) {
            self.output.push_str(&suffix);
        }
    }

    /// Estimate the width of a document.
    fn doc_width(&self, doc: &Doc) -> usize {
        match doc {
            Doc::Nil => 0,
            Doc::Text(s) => s.len(),
            Doc::Line | Doc::HardLine | Doc::SoftLine => 0,
            Doc::Concat(docs) => docs.iter().map(|d| self.doc_width(d)).sum(),
            Doc::Group(d) | Doc::Nest(_, d) | Doc::Align(d) => self.doc_width(d),
            Doc::Fill(docs) => docs.iter().map(|d| self.doc_width(d)).sum(),
            Doc::IfFlat(f, _) => self.doc_width(f),
            Doc::LineSuffix(s) => s.len(),
        }
    }
}

// =============================================================================
// BUILDER HELPERS
// =============================================================================

/// Builder for constructing documents.
pub struct DocBuilder {
    docs: Vec<Doc>,
}

impl DocBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self { docs: Vec::new() }
    }

    /// Add text.
    pub fn text(mut self, s: impl Into<String>) -> Self {
        self.docs.push(Doc::text(s));
        self
    }

    /// Add newline.
    pub fn line(mut self) -> Self {
        self.docs.push(Doc::line());
        self
    }

    /// Add hard line.
    pub fn hard_line(mut self) -> Self {
        self.docs.push(Doc::hard_line());
        self
    }

    /// Add soft line.
    pub fn soft_line(mut self) -> Self {
        self.docs.push(Doc::soft_line());
        self
    }

    /// Add space.
    pub fn space(mut self) -> Self {
        self.docs.push(Doc::text(" "));
        self
    }

    /// Add document.
    pub fn doc(mut self, doc: Doc) -> Self {
        self.docs.push(doc);
        self
    }

    /// Add nested document.
    pub fn nest(mut self, indent: usize, f: impl FnOnce(DocBuilder) -> DocBuilder) -> Self {
        let nested = f(DocBuilder::new()).build();
        self.docs.push(Doc::nest(indent, nested));
        self
    }

    /// Add grouped document.
    pub fn group(mut self, f: impl FnOnce(DocBuilder) -> DocBuilder) -> Self {
        let grouped = f(DocBuilder::new()).build();
        self.docs.push(Doc::group(grouped));
        self
    }

    /// Build the final document.
    pub fn build(self) -> Doc {
        Doc::concat(self.docs)
    }
}

impl Default for DocBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_text() {
        let config = FormatConfig::default();
        let mut printer = PrettyPrinter::new(config);
        let doc = Doc::text("hello world");
        let result = printer.print(&doc);
        assert_eq!(result.trim(), "hello world");
    }

    #[test]
    fn test_concat() {
        let config = FormatConfig::default();
        let mut printer = PrettyPrinter::new(config);
        let doc = Doc::concat(vec![
            Doc::text("hello"),
            Doc::text(" "),
            Doc::text("world"),
        ]);
        let result = printer.print(&doc);
        assert_eq!(result.trim(), "hello world");
    }

    #[test]
    fn test_group_fits() {
        let config = FormatConfig::default();
        let mut printer = PrettyPrinter::new(config);
        let doc = Doc::group(Doc::concat(vec![
            Doc::text("fn"),
            Doc::text("("),
            Doc::text("x"),
            Doc::text(")"),
        ]));
        let result = printer.print(&doc);
        assert_eq!(result.trim(), "fn(x)");
    }

    #[test]
    fn test_nest() {
        let mut config = FormatConfig::default();
        config.final_newline = false;
        let mut printer = PrettyPrinter::new(config);
        let doc = Doc::concat(vec![
            Doc::text("{"),
            Doc::nest(
                4,
                Doc::concat(vec![Doc::hard_line(), Doc::text("body")]),
            ),
            Doc::hard_line(),
            Doc::text("}"),
        ]);
        let result = printer.print(&doc);
        assert!(result.contains("    body"));
    }
}
