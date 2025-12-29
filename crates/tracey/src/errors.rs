//! Miette-based error reporting with syntax highlighting

// The facet-error derive generates pattern matches that don't use opaque fields
#![allow(unused_variables, unused_assignments)]

use facet::Facet;
use facet_miette as diagnostic;
use miette::{Diagnostic, NamedSource, SourceSpan};
use std::path::Path;
use tracey_core::{ParseWarning, WarningKind};

/// Parse warning errors
#[derive(Debug, Facet)]
#[facet(derive(Error, facet_miette::Diagnostic))]
#[repr(u8)]
pub enum ParseError {
    /// Unknown verb '{verb}'
    #[facet(diagnostic::code = "tracey::unknown_verb")]
    #[facet(diagnostic::help = "Valid verbs are: define, impl, verify, depends, related")]
    UnknownVerb {
        verb: String,

        #[facet(opaque, diagnostic::source_code)]
        src: NamedSource<String>,

        #[facet(opaque, diagnostic::label = "this verb is not recognized")]
        span: SourceSpan,
    },

    /// Malformed rule reference
    #[facet(diagnostic::code = "tracey::malformed_reference")]
    #[facet(
        diagnostic::help = "Rule references should be in the format [verb rule.id] or [rule.id]"
    )]
    MalformedReference {
        #[facet(opaque, diagnostic::source_code)]
        src: NamedSource<String>,

        #[facet(opaque, diagnostic::label = "invalid syntax")]
        span: SourceSpan,
    },
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
        WarningKind::UnknownVerb(verb) => Some(Box::new(ParseError::UnknownVerb {
            verb: verb.clone(),
            src,
            span,
        })),
        WarningKind::MalformedReference => {
            Some(Box::new(ParseError::MalformedReference { src, span }))
        }
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
