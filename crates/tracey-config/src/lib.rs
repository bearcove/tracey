//! Configuration schema for tracey
//!
//! r[impl config.format.styx]
//! r[impl config.schema]
//!
//! Config lives at `.config/tracey/config.styx` relative to the project root.

use facet::Facet;

/// Root configuration for tracey
#[derive(Debug, Clone, Default, Facet)]
pub struct Config {
    /// Specifications to track coverage against
    #[facet(default)]
    pub specs: Vec<SpecConfig>,
}

/// Configuration for a single specification
#[derive(Debug, Clone, Facet)]
pub struct SpecConfig {
    /// Name of the spec (for display purposes)
    /// r[impl config.spec.name]
    pub name: String,

    /// Deprecated: prefix is now inferred from requirement markers in spec files.
    ///
    /// If present in config, tracey will report an error and ask you to remove it.
    #[facet(default)]
    pub prefix: Option<String>,

    /// Canonical URL for the specification (e.g., a GitHub repository)
    /// r[impl config.spec.source-url]
    #[facet(default)]
    pub source_url: Option<String>,

    /// Glob patterns for markdown spec files containing requirement definitions
    /// e.g., "docs/spec/**/*.md"
    /// r[impl config.spec.include]
    #[facet(default)]
    pub include: Vec<String>,

    /// Implementations of this spec (by language)
    /// Each impl block specifies which source files to scan
    #[facet(default)]
    pub impls: Vec<Impl>,
}

/// Configuration for a single implementation of a spec
#[derive(Debug, Clone, Facet)]
pub struct Impl {
    /// Name of this implementation (e.g., "main", "core", "frontend")
    /// r[impl config.impl.name]
    pub name: String,

    /// Glob patterns for source files to scan
    /// r[impl config.impl.include]
    #[facet(default)]
    pub include: Vec<String>,

    /// Glob patterns to exclude
    /// r[impl config.impl.exclude]
    #[facet(default)]
    pub exclude: Vec<String>,

    /// Glob patterns for test files (only verify annotations allowed)
    /// r[impl config.impl.test_include]
    #[facet(default)]
    pub test_include: Vec<String>,
}
