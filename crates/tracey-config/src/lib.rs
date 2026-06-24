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

    /// Glob patterns for spec files containing requirement definitions
    /// e.g., "docs/spec/**/*.md" or "docs/spec/**/*.typ"
    /// r[impl config.spec.include]
    #[facet(default)]
    pub include: Vec<String>,

    /// Per-format render options. Only set when a spec format you use needs
    /// configuration (currently only typst).
    #[facet(default)]
    pub format: FormatConfig,

    /// Implementations of this spec (by language)
    /// Each impl block specifies which source files to scan
    #[facet(default)]
    pub impls: Vec<Impl>,
}

/// Per-format render options for a [`SpecConfig`].
///
/// One field per spec backend that needs configuration. `tracey-config` cannot
/// reference `tracey-core` (would invert the dependency direction), so the
/// schema lives here and the `crates/tracey` glue converts each field into the
/// matching `tracey_core::*Config` at build time. Adding a backend with config
/// = +1 field here + +1 arm in `tracey::data::build_spec_configs`.
#[derive(Debug, Clone, Default, Facet)]
pub struct FormatConfig {
    #[facet(default)]
    pub typst: TypstFormatConfig,
}

/// Typst-backend options. Mirrors `tracey_core::TypstConfig` (string paths;
/// resolved against the project root at load time).
#[derive(Debug, Clone, Default, Facet)]
pub struct TypstFormatConfig {
    /// Directory containing vendored Typst packages, laid out as
    /// `<path>/<namespace>/<name>/<version>/` (e.g.
    /// `vendor/typst-packages/preview/cetz/0.2.0/`). Relative paths resolve
    /// against the project root. When set, package imports look here before
    /// falling back to the system typst cache. Tracey never downloads packages.
    #[facet(default)]
    pub package_path: Option<String>,
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
