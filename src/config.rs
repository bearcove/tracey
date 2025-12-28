//! Configuration schema for tracey
//!
//! Config lives at `.config/tracey/config.kdl` relative to the project root.

use facet::Facet;
use facet_kdl as kdl;

/// Root configuration for tracey
#[derive(Debug, Facet)]
pub struct Config {
    /// Specifications to track coverage against
    #[facet(kdl::children, default)]
    pub specs: Vec<SpecConfig>,
}

/// Configuration for a single specification
#[derive(Debug, Facet)]
pub struct SpecConfig {
    /// Name of the spec (for display purposes)
    #[facet(kdl::child)]
    pub name: Name,

    /// URL to the spec's _rules.json manifest
    /// e.g., "https://rapace.dev/_rules.json"
    #[facet(kdl::child, default)]
    pub rules_url: Option<RulesUrl>,

    /// Path to a local _rules.json file (relative to the config file)
    /// e.g., "specs/my-spec/_rules.json"
    #[facet(kdl::child, default)]
    pub rules_file: Option<RulesFile>,

    /// Glob patterns for Rust files to scan
    /// Defaults to ["**/*.rs"] if not specified
    #[facet(kdl::children, default)]
    pub include: Vec<Include>,

    /// Glob patterns to exclude
    #[facet(kdl::children, default)]
    pub exclude: Vec<Exclude>,
}

#[derive(Debug, Facet)]
pub struct Name {
    #[facet(kdl::argument)]
    pub value: String,
}

#[derive(Debug, Facet)]
pub struct RulesUrl {
    #[facet(kdl::argument)]
    pub value: String,
}

#[derive(Debug, Facet)]
pub struct RulesFile {
    #[facet(kdl::argument)]
    pub path: String,
}

#[derive(Debug, Facet)]
pub struct Include {
    #[facet(kdl::argument)]
    pub pattern: String,
}

#[derive(Debug, Facet)]
pub struct Exclude {
    #[facet(kdl::argument)]
    pub pattern: String,
}
