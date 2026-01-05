//! Configuration schema for tracey
//!
//! Config lives at `.config/tracey/config.kdl` relative to the project root.
//!
//! [impl config.format.kdl]

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
    ///
    /// [impl config.spec.name]
    #[facet(kdl::child)]
    pub name: Name,

    /// Glob pattern for markdown spec files to extract rules from
    /// e.g., "docs/spec/**/*.md"
    /// Rules will be extracted from r[rule.id] syntax in the markdown
    #[facet(kdl::child)]
    pub rules_glob: RulesGlob,

    /// Implementations of this spec (by language)
    /// Each impl block specifies which source files to scan
    #[facet(kdl::children, default)]
    pub impls: Vec<Impl>,
}

/// Configuration for a single implementation of a spec
/// Note: struct name `Impl` maps to KDL node name `impl`
#[derive(Debug, Clone, Facet)]
pub struct Impl {
    /// Language name (e.g., "rust", "swift", "typescript")
    #[facet(kdl::child)]
    pub lang: Lang,

    /// Glob patterns for source files to scan
    #[facet(kdl::children, default)]
    pub include: Vec<Include>,

    /// Glob patterns to exclude
    #[facet(kdl::children, default)]
    pub exclude: Vec<Exclude>,
}

#[derive(Debug, Clone, Facet)]
pub struct Name {
    #[facet(kdl::argument)]
    pub value: String,
}

#[derive(Debug, Clone, Facet)]
pub struct RulesGlob {
    #[facet(kdl::argument)]
    pub pattern: String,
}

#[derive(Debug, Clone, Facet)]
pub struct Lang {
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
