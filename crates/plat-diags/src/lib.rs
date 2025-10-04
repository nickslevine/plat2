use ariadne::{Color, Label, Report, ReportKind, Source};
use std::fmt;
use std::ops::Range;

/// A span in the source code (byte offsets)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn to_range(&self) -> Range<usize> {
        self.start..self.end
    }
}

impl From<(usize, usize)> for Span {
    fn from((start, end): (usize, usize)) -> Self {
        Self { start, end }
    }
}

/// Severity level for diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// Category of diagnostic error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Lexical,
    Syntax,
    Type,
    Visibility,
    Module,
    Runtime,
}

/// A labeled span with a message
#[derive(Debug, Clone)]
pub struct DiagnosticLabel {
    pub span: Span,
    pub message: String,
    pub color: Option<Color>,
}

impl DiagnosticLabel {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
            color: None,
        }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

/// Rich diagnostic error with location information and helpful messages
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Error severity
    pub severity: Severity,
    /// Error category
    pub category: ErrorCategory,
    /// Error code (e.g., "E001")
    pub code: Option<String>,
    /// Main error message
    pub message: String,
    /// Source filename
    pub filename: String,
    /// Primary label (the main error location)
    pub primary_label: DiagnosticLabel,
    /// Additional labels (related locations)
    pub secondary_labels: Vec<DiagnosticLabel>,
    /// Help message with suggestion
    pub help: Option<String>,
    /// Additional notes
    pub notes: Vec<String>,
}

impl Diagnostic {
    /// Create a new diagnostic error
    pub fn error(
        category: ErrorCategory,
        filename: impl Into<String>,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        let message = message.into();
        Self {
            severity: Severity::Error,
            category,
            code: None,
            filename: filename.into(),
            primary_label: DiagnosticLabel::new(span, message.clone()),
            message,
            secondary_labels: Vec::new(),
            help: None,
            notes: Vec::new(),
        }
    }

    /// Add an error code
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Set the primary label message (different from main message)
    pub fn with_label(mut self, message: impl Into<String>) -> Self {
        self.primary_label.message = message.into();
        self
    }

    /// Add a secondary label
    pub fn with_secondary_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.secondary_labels.push(DiagnosticLabel::new(span, message));
        self
    }

    /// Add a help message
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Add a note
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Report this diagnostic to stderr using Ariadne
    pub fn report(&self, source: &str) {
        let report_kind = match self.severity {
            Severity::Error => ReportKind::Error,
            Severity::Warning => ReportKind::Warning,
            Severity::Note => ReportKind::Advice,
        };

        let mut report = Report::build(report_kind, &self.filename, self.primary_label.span.start);

        // Set main message with optional error code
        if let Some(code) = &self.code {
            report = report.with_message(format!("[{}] {}", code, self.message));
        } else {
            report = report.with_message(&self.message);
        }

        // Add primary label
        let primary_color = self.primary_label.color.unwrap_or(Color::Red);
        report = report.with_label(
            Label::new((&self.filename, self.primary_label.span.to_range()))
                .with_message(&self.primary_label.message)
                .with_color(primary_color),
        );

        // Add secondary labels
        for label in &self.secondary_labels {
            let color = label.color.unwrap_or(Color::Blue);
            report = report.with_label(
                Label::new((&self.filename, label.span.to_range()))
                    .with_message(&label.message)
                    .with_color(color),
            );
        }

        // Add help message
        if let Some(help) = &self.help {
            report = report.with_help(help);
        }

        // Add notes
        for note in &self.notes {
            report = report.with_note(note);
        }

        // Print the report
        report
            .finish()
            .print((&self.filename, Source::from(source)))
            .unwrap();
    }
}

/// Legacy error type for backward compatibility
/// This will be gradually replaced by Diagnostic
#[derive(Debug)]
pub enum DiagnosticError {
    Syntax(String),
    Type(String),
    Runtime(String),
    // New variant that wraps the rich diagnostic
    Rich(Diagnostic),
}

impl DiagnosticError {
    /// Convert to a rich Diagnostic (best effort)
    pub fn to_diagnostic(&self, filename: &str, default_span: Span) -> Diagnostic {
        match self {
            DiagnosticError::Syntax(msg) => {
                Diagnostic::error(ErrorCategory::Syntax, filename, default_span, msg)
            }
            DiagnosticError::Type(msg) => {
                Diagnostic::error(ErrorCategory::Type, filename, default_span, msg)
            }
            DiagnosticError::Runtime(msg) => {
                Diagnostic::error(ErrorCategory::Runtime, filename, default_span, msg)
            }
            DiagnosticError::Rich(diag) => diag.clone(),
        }
    }
}

impl fmt::Display for DiagnosticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticError::Syntax(msg) => write!(f, "Syntax error: {}", msg),
            DiagnosticError::Type(msg) => write!(f, "Type error: {}", msg),
            DiagnosticError::Runtime(msg) => write!(f, "Runtime error: {}", msg),
            DiagnosticError::Rich(diag) => write!(f, "{}", diag.message),
        }
    }
}

impl std::error::Error for DiagnosticError {}

// ============================================================================
// Specialized error constructors for common error types
// ============================================================================

impl Diagnostic {
    /// Create a syntax error (parsing/lexical errors)
    pub fn syntax_error(
        filename: impl Into<String>,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        Self::error(ErrorCategory::Syntax, filename, span, message)
    }

    /// Create a type error
    pub fn type_error(
        filename: impl Into<String>,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        Self::error(ErrorCategory::Type, filename, span, message)
    }

    /// Create a type mismatch error with expected and actual types
    pub fn type_mismatch(
        filename: impl Into<String>,
        span: Span,
        expected: impl fmt::Display,
        actual: impl fmt::Display,
    ) -> Self {
        Self::type_error(
            filename,
            span,
            format!("Type mismatch: expected {}, found {}", expected, actual),
        )
        .with_label(format!("expected {}, found {}", expected, actual))
        .with_help(format!("Change this to type {}", expected))
    }

    /// Create a visibility error
    pub fn visibility_error(
        filename: impl Into<String>,
        span: Span,
        item_name: impl Into<String>,
        item_kind: impl Into<String>,
    ) -> Self {
        let item_name = item_name.into();
        let item_kind = item_kind.into();
        Self::error(
            ErrorCategory::Visibility,
            filename,
            span,
            format!("Cannot access private {} '{}'", item_kind, item_name),
        )
        .with_label(format!("private {}", item_kind))
        .with_help(format!("Mark this {} as 'pub' to make it accessible", item_kind))
    }

    /// Create an undefined symbol error
    pub fn undefined_symbol(
        filename: impl Into<String>,
        span: Span,
        name: impl Into<String>,
    ) -> Self {
        let name = name.into();
        Self::type_error(filename, span, format!("Undefined symbol '{}'", name))
            .with_label(format!("not found in this scope"))
    }

    /// Create a module error
    pub fn module_error(
        filename: impl Into<String>,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        Self::error(ErrorCategory::Module, filename, span, message)
    }

    /// Create a naming convention error
    pub fn naming_convention_error(
        filename: impl Into<String>,
        span: Span,
        name: impl Into<String>,
        expected_convention: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let convention = expected_convention.into();
        Self::syntax_error(
            filename,
            span,
            format!("Name '{}' does not follow {} convention", name, convention),
        )
        .with_label(format!("should be in {}", convention))
        .with_help(format!(
            "Use {} (e.g., {})",
            convention,
            match convention.as_str() {
                "snake_case" => "my_variable",
                "TitleCase" => "MyType",
                _ => "proper_format",
            }
        ))
    }
}

// ============================================================================
// Legacy compatibility functions
// ============================================================================

// Keep the old report_error for backward compatibility
pub fn report_error(filename: &str, source: &str, message: &str, span: (usize, usize)) {
    Report::build(ReportKind::Error, filename, span.0)
        .with_message(message)
        .with_label(Label::new((filename, Range::from(span.0..span.1))).with_message(message))
        .finish()
        .print((filename, Source::from(source)))
        .unwrap();
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_creation() {
        let diag = Diagnostic::syntax_error(
            "test.plat",
            Span::new(10, 20),
            "Unexpected token",
        )
        .with_code("E001")
        .with_help("Did you forget a semicolon?");

        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.category, ErrorCategory::Syntax);
        assert_eq!(diag.code, Some("E001".to_string()));
        assert_eq!(diag.message, "Unexpected token");
        assert_eq!(diag.help, Some("Did you forget a semicolon?".to_string()));
    }

    #[test]
    fn test_type_mismatch() {
        let diag = Diagnostic::type_mismatch(
            "test.plat",
            Span::new(5, 10),
            "Int32",
            "String",
        );

        assert_eq!(diag.category, ErrorCategory::Type);
        assert!(diag.message.contains("expected Int32"));
        assert!(diag.message.contains("found String"));
        assert!(diag.help.is_some());
    }

    #[test]
    fn test_visibility_error() {
        let diag = Diagnostic::visibility_error(
            "test.plat",
            Span::new(0, 5),
            "my_field",
            "field",
        );

        assert_eq!(diag.category, ErrorCategory::Visibility);
        assert!(diag.message.contains("private"));
        assert!(diag.help.unwrap().contains("pub"));
    }

    #[test]
    fn test_multi_label_diagnostic() {
        let diag = Diagnostic::type_error(
            "test.plat",
            Span::new(10, 15),
            "Type mismatch in function call",
        )
        .with_secondary_label(Span::new(20, 25), "defined here")
        .with_secondary_label(Span::new(30, 35), "called here")
        .with_note("Function signatures must match");

        assert_eq!(diag.secondary_labels.len(), 2);
        assert_eq!(diag.notes.len(), 1);
    }
}