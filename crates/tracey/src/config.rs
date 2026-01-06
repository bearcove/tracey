//! Configuration schema for tracey
//!
//! r[impl config.format.kdl]
//! r[impl config.schema]
//!
//! Config lives at `.config/tracey/config.kdl` relative to the project root.

use facet::Facet;
use facet_kdl as kdl;

/// Root configuration for tracey
#[derive(Debug, Clone, Facet)]
pub struct Config {
    /// Specifications to track coverage against
    #[facet(kdl::children, default)]
    pub specs: Vec<SpecConfig>,
}

/// Configuration for a single specification
#[derive(Debug, Clone, Facet)]
pub struct SpecConfig {
    /// Name of the spec (for display purposes)
    /// r[impl config.spec.name]
    #[facet(kdl::child)]
    pub name: Name,

    /// Prefix used to identify this spec in annotations (e.g., "r" for r[req.id])
    /// r[impl config.spec.prefix]
    #[facet(kdl::child)]
    pub prefix: Prefix,

    /// Glob patterns for markdown spec files containing requirement definitions
    /// e.g., "docs/spec/**/*.md"
    /// r[impl config.spec.include]
    #[facet(kdl::children, default)]
    pub include: Vec<Include>,

    /// Implementations of this spec (by language)
    /// Each impl block specifies which source files to scan
    #[facet(kdl::children, default)]
    pub impls: Vec<Impl>,
}

/// Configuration for a single implementation of a spec
/// Note: struct name `Impl` maps to KDL node name `impl`
#[derive(Debug, Clone, Facet)]
pub struct Impl {
    /// Name of this implementation (e.g., "main", "core", "frontend")
    /// r[impl config.impl.name]
    #[facet(kdl::child)]
    pub name: ImplName,

    /// Glob patterns for source files to scan
    /// r[impl config.impl.include]
    #[facet(kdl::children, default)]
    pub include: Vec<Include>,

    /// Glob patterns to exclude
    /// r[impl config.impl.exclude]
    #[facet(kdl::children, default)]
    pub exclude: Vec<Exclude>,
}

#[derive(Debug, Clone, Facet)]
pub struct Name {
    #[facet(kdl::argument)]
    pub value: String,
}

#[derive(Debug, Clone, Facet)]
pub struct Prefix {
    #[facet(kdl::argument)]
    pub value: String,
}

#[derive(Debug, Clone, Facet)]
pub struct ImplName {
    #[facet(kdl::argument)]
    pub value: String,
}

#[derive(Debug, Clone, Facet)]
pub struct Include {
    #[facet(kdl::argument)]
    pub pattern: String,
}

#[derive(Debug, Clone, Facet)]
pub struct Exclude {
    #[facet(kdl::argument)]
    pub pattern: String,
}
