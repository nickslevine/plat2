use ariadne::{Report, ReportKind, Source};
use std::ops::Range;

#[derive(Debug, thiserror::Error)]
pub enum DiagnosticError {
    #[error("Syntax error: {0}")]
    Syntax(String),

    #[error("Type error: {0}")]
    Type(String),

    #[error("Runtime error: {0}")]
    Runtime(String),
}

pub fn report_error(filename: &str, source: &str, message: &str, span: (usize, usize)) {
    Report::build(ReportKind::Error, filename, span.0)
        .with_message(message)
        .with_label(
            ariadne::Label::new((filename, Range::from(span.0..span.1)))
                .with_message(message)
        )
        .finish()
        .print((filename, Source::from(source)))
        .unwrap();
}