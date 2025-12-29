//! tracey-core - Core library for spec coverage analysis
//!
//! This crate provides the building blocks for extracting rule references
//! from Rust source code and computing coverage against a spec manifest.
//!
//! # Features
//!
//! - `walk` - Enable [`WalkSources`] for gitignore-aware directory walking (brings in `ignore`)
//! - `parallel` - Enable parallel extraction (brings in `rayon`)
//! - `fetch` - Enable [`SpecManifest::fetch`] for HTTP fetching (brings in `ureq`)
//!
//! # Example
//!
//! ```ignore
//! use tracey_core::{Rules, WalkSources, SpecManifest, CoverageReport};
//!
//! let rules = Rules::extract(
//!     WalkSources::new(".")
//!         .include(["**/*.rs"])
//!         .exclude(["target/**"])
//! )?;
//!
//! let manifest = SpecManifest::load("spec/_rules.json")?;
//! let report = CoverageReport::compute("my-spec", &manifest, &rules);
//! ```

mod coverage;
mod lexer;
#[cfg(feature = "markdown")]
pub mod markdown;
mod sources;
mod spec;

pub use coverage::CoverageReport;
pub use lexer::{ParseWarning, RefVerb, RuleReference, Rules, SourceSpan, WarningKind};
pub use sources::{MemorySources, PathSources, Sources};
pub use spec::{RuleInfo, SpecManifest};

#[cfg(feature = "walk")]
pub use sources::WalkSources;
