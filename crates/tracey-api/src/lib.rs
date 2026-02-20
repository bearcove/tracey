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
#[facet(rename_all = "camelCase")]
pub struct ApiSpecInfo {
    pub name: String,

    /// @tracey:ignore-next-line
    /// Prefix used in annotations (e.g., "r" for r[req.id])
    pub prefix: String,

    /// Path to spec file(s) if local
    #[facet(default)]
    pub source: Option<String>,

    /// Canonical URL for the specification (e.g., a GitHub repository)
    #[facet(default)]
    pub source_url: Option<String>,

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
    /// Raw markdown source (without r[...] marker, but with `>` prefixes for blockquote rules)
    pub raw: String,
    /// Rendered HTML (for dashboard display)
    pub html: String,
    #[facet(default)]
    pub status: Option<String>,
    #[facet(default)]
    pub level: Option<String>,
    #[facet(default)]
    pub source_file: Option<String>,
    #[facet(default)]
    pub source_line: Option<usize>,
    #[facet(default)]
    pub source_column: Option<usize>,
    /// Section slug (heading ID) that this rule belongs to
    #[facet(default)]
    pub section: Option<String>,
    /// Section title (heading text) that this rule belongs to
    #[facet(default)]
    pub section_title: Option<String>,
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

// ============================================================================
// Validation
// ============================================================================

/// r[impl validation.circular-deps]
/// r[impl validation.naming]
///
/// A validation error found in the spec or implementation.
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ValidationError {
    /// Error code for programmatic handling
    pub code: ValidationErrorCode,
    /// Human-readable error message
    pub message: String,
    /// File where the error was found (if applicable)
    #[facet(default)]
    pub file: Option<String>,
    /// Line number (if applicable)
    #[facet(default)]
    pub line: Option<usize>,
    /// Column number (if applicable)
    #[facet(default)]
    pub column: Option<usize>,
    /// Related rule IDs (for dependency errors)
    #[facet(default)]
    pub related_rules: Vec<String>,
}

/// Error codes for validation errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[facet(rename_all = "snake_case")]
#[repr(u8)]
pub enum ValidationErrorCode {
    /// Circular dependency detected in `depends` references
    CircularDependency,
    /// Requirement ID doesn't follow naming convention
    InvalidNaming,
    /// Unknown requirement ID referenced
    UnknownRequirement,
    /// Reference points to an older requirement version
    StaleRequirement,
    /// Duplicate requirement ID in the same spec
    DuplicateRequirement,
    /// Unknown prefix in reference
    UnknownPrefix,
    /// Impl annotation in test file (only verify allowed)
    ImplInTestFile,
}

/// Validation results for a spec/implementation pair
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ValidationResult {
    /// Spec name
    pub spec: String,
    /// Implementation name
    pub impl_name: String,
    /// List of validation errors found
    pub errors: Vec<ValidationError>,
    /// Number of warnings (non-fatal issues)
    pub warning_count: usize,
    /// Number of errors (fatal issues)
    pub error_count: usize,
}
