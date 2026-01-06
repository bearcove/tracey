//! tracey-core - Core library for spec coverage analysis
//!
//! This crate provides the building blocks for:
//! - Extracting requirement references from source code (Rust, Swift, TypeScript, and more)
//! - Computing coverage statistics
//!
//! # Features
//!
//! - `walk` - Enable [`WalkSources`] for gitignore-aware directory walking (brings in `ignore`)
//! - `parallel` - Enable parallel extraction (brings in `rayon`)
//!
//! For markdown requirement extraction, use the `bearmark` crate directly.
//!
//! # Extracting Requirement References from Source Code
//!
//! tracey recognizes requirement references in comments using `//` or `/* */` syntax.
//! This works with Rust, Swift, TypeScript, JavaScript, Go, C/C++, and many other languages.
//!
//! See [`SUPPORTED_EXTENSIONS`] for the full list of supported file types.
//!
//! ```text
//! // r[impl channel.id.parity] - implementation reference
//! // r[verify error.handling] - test/verification reference
//! // [req.id] - basic reference (legacy syntax)
//! ```
//!
//! Extract references using [`Rules::extract`]:
//!
//! ```ignore
//! use tracey_core::{Rules, WalkSources};
//!
//! // Scan Rust files for requirement references
//! let rules = Rules::extract(
//!     WalkSources::new(".")
//!         .include(["**/*.rs"])
//!         .exclude(["target/**"])
//! )?;
//!
//! println!("Found {} requirement references", rules.len());
//! ```
//!
//! # Extracting Requirements from Markdown
//!
//! Requirements are defined in markdown using the `r[req.id]` syntax:
//!
//! ```markdown
//! r[channel.id.allocation]
//! Channel IDs MUST be allocated sequentially starting from 0.
//! ```
//!
//! Requirements can also include metadata attributes:
//!
//! ```markdown
//! r[channel.id.allocation status=stable level=must since=1.0]
//! Channel IDs MUST be allocated sequentially starting from 0.
//!
//! r[experimental.feature status=draft]
//! This feature is under development.
//!
//! r[old.behavior status=deprecated until=3.0]
//! This behavior is deprecated and will be removed.
//! ```
//!
//! Extract requirements using bearmark's render function:
//!
//! ```ignore
//! use bearmark::{render, RenderOptions};
//!
//! let markdown = r#"
//! # My Spec
//!
//! r[my.req.id] This requirement defines important behavior.
//! "#;
//!
//! // Render markdown to extract requirements with HTML content
//! let doc = render(markdown, &RenderOptions::default()).await.unwrap();
//! assert_eq!(doc.rules.len(), 1);
//! assert_eq!(doc.rules[0].id, "my.req.id");
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
//!         .add("foo.rs", "// r[impl test.req]")
//!         .add("bar.rs", "// r[verify other.req]")
//! ).unwrap();
//!
//! assert_eq!(rules.len(), 2);
//! ```

mod coverage;
mod lexer;
mod sources;
mod spec;

#[cfg(feature = "reverse")]
pub mod code_units;

pub use coverage::CoverageReport;
pub use lexer::{ParseWarning, RefVerb, RuleReference, Rules, SourceSpan, WarningKind};
pub use sources::{
    MemorySources, PathSources, SUPPORTED_EXTENSIONS, Sources, is_supported_extension,
};
pub use spec::ReqDefinition;

#[cfg(feature = "walk")]
pub use sources::WalkSources;
