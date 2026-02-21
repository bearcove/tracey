//! tracey-core - Core library for spec coverage analysis
//!
//! This crate provides the building blocks for:
//! - Extracting requirement references from source code (Rust, Swift, TypeScript, and more)
//! - Computing coverage statistics

mod coverage;
mod lexer;
mod markdown;
mod rule_id;
mod sources;
mod spec;

#[cfg(feature = "reverse")]
pub mod code_units;

pub use coverage::CoverageReport;
pub use lexer::{ParseWarning, RefVerb, ReqReference, Reqs, SourceSpan, WarningKind};
pub use rule_id::{
    RuleId, RuleIdMatch, classify_reference_for_rule, classify_reference_for_rule_str,
    parse_rule_id,
};
pub use sources::{
    ExtractionResult, MemorySources, PathSources, SUPPORTED_EXTENSIONS, Sources,
    is_supported_extension,
};
pub use spec::ReqDefinition;

#[cfg(feature = "walk")]
pub use sources::WalkSources;
