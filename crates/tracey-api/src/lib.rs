//! API types for the tracey dashboard
//!
//! This crate contains only the JSON API type definitions used by the tracey
//! HTTP server and dashboard. These types are used to generate TypeScript
//! definitions via facet-typescript.

use facet::Facet;

/// Git status for a file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum GitStatus {
    /// File has uncommitted changes
    Dirty,
    /// File has staged changes
    Staged,
    /// File is clean (no changes)
    Clean,
    /// Not in a git repo or error checking
    Unknown,
}

/// Project configuration info
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ApiConfig {
    pub project_root: String,
    pub specs: Vec<ApiSpecInfo>,
}

#[derive(Debug, Clone, Facet)]
pub struct ApiSpecInfo {
    pub name: String,
    /// Prefix used in annotations (e.g., "r" for r[req.id])
    pub prefix: String,
    /// Path to spec file(s) if local
    #[facet(default)]
    pub source: Option<String>,
    /// Available implementations for this spec
    pub implementations: Vec<String>,
}

/// Forward traceability: rules with their code references
#[derive(Debug, Clone, Facet)]
pub struct ApiForwardData {
    pub specs: Vec<ApiSpecForward>,
}

#[derive(Debug, Clone, Facet)]
pub struct ApiSpecForward {
    pub name: String,
    pub rules: Vec<ApiRule>,
}

#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ApiRule {
    pub id: String,
    pub html: String,
    #[facet(default)]
    pub status: Option<String>,
    #[facet(default)]
    pub level: Option<String>,
    #[facet(default)]
    pub source_file: Option<String>,
    #[facet(default)]
    pub source_line: Option<usize>,
    pub impl_refs: Vec<ApiCodeRef>,
    pub verify_refs: Vec<ApiCodeRef>,
    pub depends_refs: Vec<ApiCodeRef>,
}

#[derive(Debug, Clone, Facet)]
pub struct ApiCodeRef {
    pub file: String,
    pub line: usize,
}

/// Reverse traceability: file tree with coverage info
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ApiReverseData {
    /// Total code units across all files
    pub total_units: usize,
    /// Code units with at least one rule reference
    pub covered_units: usize,
    /// File tree with coverage info
    pub files: Vec<ApiFileEntry>,
}

#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ApiFileEntry {
    pub path: String,
    /// Number of code units in this file
    pub total_units: usize,
    /// Number of covered code units
    pub covered_units: usize,
}

/// Single file with full coverage details
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ApiFileData {
    pub path: String,
    pub content: String,
    /// Syntax-highlighted HTML content
    pub html: String,
    /// Code units in this file with their coverage
    pub units: Vec<ApiCodeUnit>,
}

#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ApiCodeUnit {
    pub kind: String,
    #[facet(default)]
    pub name: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    /// Rule references found in this code unit's comments
    pub rule_refs: Vec<String>,
}

/// A section of a spec (one source file)
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct SpecSection {
    /// Source file path
    pub source_file: String,
    /// Rendered HTML content
    pub html: String,
    /// Weight for ordering (from frontmatter)
    pub weight: i32,
}

/// Coverage counts for an outline entry
#[derive(Debug, Clone, Default, Facet)]
#[facet(rename_all = "camelCase")]
pub struct OutlineCoverage {
    /// Number of rules with implementation refs
    pub impl_count: usize,
    /// Number of rules with verification refs
    pub verify_count: usize,
    /// Total number of rules
    pub total: usize,
}

/// An entry in the spec outline (heading with coverage info)
#[derive(Debug, Clone, Facet)]
pub struct OutlineEntry {
    /// Heading text
    pub title: String,
    /// Slug for linking
    pub slug: String,
    /// Heading level (1-6)
    pub level: u8,
    /// Direct coverage (rules directly under this heading)
    pub coverage: OutlineCoverage,
    /// Aggregated coverage (includes all nested rules)
    pub aggregated: OutlineCoverage,
}

/// Spec content (may span multiple files)
#[derive(Debug, Clone, Facet)]
pub struct ApiSpecData {
    pub name: String,
    /// Sections ordered by weight
    pub sections: Vec<SpecSection>,
    /// Outline with coverage info
    pub outline: Vec<OutlineEntry>,
}
