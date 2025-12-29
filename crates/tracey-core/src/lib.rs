//! tracey-core - Core library for spec coverage analysis
//!
//! This crate provides the building blocks for:
//! - Extracting rule references from source code (Rust, Swift, TypeScript, and more)
//! - Extracting rule definitions from markdown spec documents
//! - Computing coverage against a spec manifest
//!
//! # Features
//!
//! - `walk` - Enable [`WalkSources`] for gitignore-aware directory walking (brings in `ignore`)
//! - `parallel` - Enable parallel extraction (brings in `rayon`)
//! - `fetch` - Enable [`SpecManifest::fetch`] for HTTP fetching (brings in `ureq`)
//! - `markdown` - Enable [`markdown`] module for extracting rules from spec documents
//!
//! # Extracting Rule References from Source Code
//!
//! tracey recognizes rule references in comments using `//` or `/* */` syntax.
//! This works with Rust, Swift, TypeScript, JavaScript, Go, C/C++, and many other languages.
//!
//! See [`SUPPORTED_EXTENSIONS`] for the full list of supported file types.
//!
//! ```rust
//! // [impl channel.id.parity] - implementation reference
//! // [verify error.handling] - test/verification reference
//! // [rule.id] - basic reference (legacy syntax)
//! ```
//!
//! Extract references using [`Rules::extract`]:
//!
//! ```ignore
//! use tracey_core::{Rules, WalkSources, SpecManifest, CoverageReport};
//!
//! // Scan Rust files for rule references
//! let rules = Rules::extract(
//!     WalkSources::new(".")
//!         .include(["**/*.rs"])
//!         .exclude(["target/**"])
//! )?;
//!
//! // Load spec manifest and compute coverage
//! let manifest = SpecManifest::load("spec/_rules.json")?;
//! let report = CoverageReport::compute("my-spec", &manifest, &rules);
//! println!("Coverage: {:.1}%", report.coverage_percent());
//! ```
//!
//! # Extracting Rules from Markdown (feature: `markdown`)
//!
//! Rules are defined in markdown using the `r[rule.id]` syntax:
//!
//! ```markdown
//! r[channel.id.allocation]
//! Channel IDs MUST be allocated sequentially starting from 0.
//! ```
//!
//! Extract rules and generate manifests:
//!
//! ```
//! use tracey_core::markdown::{MarkdownProcessor, RulesManifest};
//!
//! let markdown = r#"
//! # My Spec
//!
//! r[my.rule.id]
//! This rule defines important behavior.
//! "#;
//!
//! // Extract rules from markdown
//! let result = MarkdownProcessor::process(markdown).unwrap();
//! assert_eq!(result.rules.len(), 1);
//! assert_eq!(result.rules[0].id, "my.rule.id");
//!
//! // Generate _rules.json manifest
//! let manifest = RulesManifest::from_rules(&result.rules, "/spec");
//! println!("{}", manifest.to_json());
//!
//! // The transformed output has HTML anchors for rendering
//! assert!(result.output.contains("id=\"r-my.rule.id\""));
//! ```
//!
//! # In-Memory Sources (for testing/WASM)
//!
//! Use [`MemorySources`] when you don't want to hit the filesystem:
//!
//! ```
//! use tracey_core::{Rules, MemorySources, Sources};
//!
//! let rules = Rules::extract(
//!     MemorySources::new()
//!         .add("foo.rs", "// [impl test.rule]")
//!         .add("bar.rs", "// [verify other.rule]")
//! ).unwrap();
//!
//! assert_eq!(rules.len(), 2);
//! ```

mod coverage;
mod lexer;
#[cfg(feature = "markdown")]
pub mod markdown;
mod sources;
mod spec;

pub use coverage::CoverageReport;
pub use lexer::{ParseWarning, RefVerb, RuleReference, Rules, SourceSpan, WarningKind};
pub use sources::{
    MemorySources, PathSources, SUPPORTED_EXTENSIONS, Sources, is_supported_extension,
};
pub use spec::{RuleInfo, SpecManifest};

#[cfg(feature = "walk")]
pub use sources::WalkSources;
