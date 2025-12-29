//! Miette-based error reporting with syntax highlighting

use miette::{Diagnostic, NamedSource, SourceSpan};
use std::path::Path;
use thiserror::Error;
use tracey_core::{ParseWarning, WarningKind};

/// Error for unknown verb in rule reference
#[derive(Debug, Error, Diagnostic)]
#[error("Unknown verb '{verb}'")]
#[diagnostic(
    code(tracey::unknown_verb),
    help("Valid verbs are: define, impl, verify, depends, related")
)]
pub struct UnknownVerbError {
    pub verb: String,

    #[source_code]
    pub src: NamedSource<String>,

    #[label("this verb is not recognized")]
    pub span: SourceSpan,
}

/// Error for malformed rule reference
#[derive(Debug, Error, Diagnostic)]
#[error("Malformed rule reference")]
#[diagnostic(
    code(tracey::malformed_reference),
    help("Rule references should be in the format [verb rule.id] or [rule.id]")
)]
pub struct MalformedReferenceError {
    #[source_code]
    pub src: NamedSource<String>,

    #[label("invalid syntax")]
    pub span: SourceSpan,
}

/// Convert a ParseWarning into a miette diagnostic
pub fn warning_to_diagnostic(
    warning: &ParseWarning,
    source_cache: &impl Fn(&Path) -> Option<String>,
) -> Option<Box<dyn Diagnostic + Send + Sync + 'static>> {
    let content = source_cache(&warning.file)?;
    let src = NamedSource::new(warning.file.display().to_string(), content);
    let span = SourceSpan::new(warning.span.offset.into(), warning.span.length);

    match &warning.kind {
        WarningKind::UnknownVerb(verb) => Some(Box::new(UnknownVerbError {
            verb: verb.clone(),
            src,
            span,
        })),
        WarningKind::MalformedReference => Some(Box::new(MalformedReferenceError { src, span })),
    }
}

/// Print warnings using miette
pub fn print_warnings(warnings: &[ParseWarning], source_cache: &impl Fn(&Path) -> Option<String>) {
    for warning in warnings {
        if let Some(diagnostic) = warning_to_diagnostic(warning, source_cache) {
            eprintln!("{:?}", miette::Report::new_boxed(diagnostic));
        }
    }
}
