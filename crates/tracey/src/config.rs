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

    /// URL to the spec's _rules.json manifest
    /// e.g., `https://rapace.dev/_rules.json`
    #[facet(kdl::child, default)]
    pub rules_url: Option<RulesUrl>,

    /// Path to a local _rules.json file (relative to the config file)
    /// e.g., "specs/my-spec/_rules.json"
    #[facet(kdl::child, default)]
    pub rules_file: Option<RulesFile>,

    /// Glob pattern for markdown spec files to extract rules from
    /// e.g., "docs/spec/**/*.md"
    /// Rules will be extracted from r[rule.id] syntax in the markdown
    #[facet(kdl::child, default)]
    pub rules_glob: Option<RulesGlob>,

    /// Glob patterns for Rust files to scan
    /// Defaults to ["**/*.rs"] if not specified
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
pub struct RulesUrl {
    #[facet(kdl::argument)]
    pub value: String,
}

#[derive(Debug, Clone, Facet)]
pub struct RulesFile {
    #[facet(kdl::argument)]
    pub path: String,
}

#[derive(Debug, Clone, Facet)]
pub struct RulesGlob {
    #[facet(kdl::argument)]
    pub pattern: String,
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
