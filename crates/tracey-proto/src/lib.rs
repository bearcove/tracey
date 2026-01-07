//! Protocol definitions for the tracey daemon RPC service.
//!
//! This crate defines the `TraceyDaemon` service trait using roam's `#[service]`
//! macro. The daemon exposes this service over a Unix socket, and bridges
//! (HTTP, MCP, LSP) connect as clients.

use facet::Facet;
use roam::Pull;
use roam::prelude::*;

// Re-export API types for convenience
pub use tracey_api::*;

// ============================================================================
// Request/Response types for the TraceyDaemon service
// ============================================================================

/// Request for uncovered rules query
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct UncoveredRequest {
    /// Spec name (optional if only one spec configured)
    #[facet(default)]
    pub spec: Option<String>,
    /// Implementation name (optional if only one impl configured)
    #[facet(default)]
    pub impl_name: Option<String>,
    /// Filter rules by ID prefix (case-insensitive)
    #[facet(default)]
    pub prefix: Option<String>,
}

/// Response for uncovered rules query
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct UncoveredResponse {
    pub spec: String,
    pub impl_name: String,
    pub total_rules: usize,
    pub uncovered_count: usize,
    /// Rules grouped by section
    pub by_section: Vec<SectionRules>,
}

/// Rules within a section
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct SectionRules {
    pub section: String,
    pub rules: Vec<RuleRef>,
}

/// Reference to a rule
#[derive(Debug, Clone, Facet)]
pub struct RuleRef {
    pub id: String,
    #[facet(default)]
    pub text: Option<String>,
}

/// Request for untested rules query
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct UntestedRequest {
    #[facet(default)]
    pub spec: Option<String>,
    #[facet(default)]
    pub impl_name: Option<String>,
    #[facet(default)]
    pub prefix: Option<String>,
}

/// Response for untested rules query
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct UntestedResponse {
    pub spec: String,
    pub impl_name: String,
    pub total_rules: usize,
    pub untested_count: usize,
    pub by_section: Vec<SectionRules>,
}

/// Request for unmapped code query
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct UnmappedRequest {
    #[facet(default)]
    pub spec: Option<String>,
    #[facet(default)]
    pub impl_name: Option<String>,
    /// Path to zoom into (directory or file)
    #[facet(default)]
    pub path: Option<String>,
}

/// Response for unmapped code query
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct UnmappedResponse {
    pub spec: String,
    pub impl_name: String,
    pub total_units: usize,
    pub unmapped_count: usize,
    /// Tree view or file details depending on path
    pub entries: Vec<UnmappedEntry>,
}

/// Entry in unmapped code tree
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct UnmappedEntry {
    pub path: String,
    pub is_dir: bool,
    pub total_units: usize,
    pub unmapped_units: usize,
    /// Code units if this is a file and detailed view requested
    #[facet(default)]
    pub units: Vec<UnmappedUnit>,
}

/// An unmapped code unit
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct UnmappedUnit {
    pub kind: String,
    #[facet(default)]
    pub name: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
}

/// Coverage status response
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct StatusResponse {
    pub impls: Vec<ImplStatus>,
}

/// Status for a single spec/impl combination
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ImplStatus {
    pub spec: String,
    pub impl_name: String,
    pub total_rules: usize,
    pub covered_rules: usize,
    pub verified_rules: usize,
}

/// Information about a specific rule
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct RuleInfo {
    pub id: String,
    pub text: String,
    pub html: String,
    #[facet(default)]
    pub source_file: Option<String>,
    #[facet(default)]
    pub source_line: Option<usize>,
    /// Coverage across all implementations
    pub coverage: Vec<RuleCoverage>,
}

/// Coverage of a rule in a specific implementation
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct RuleCoverage {
    pub spec: String,
    pub impl_name: String,
    pub impl_refs: Vec<ApiCodeRef>,
    pub verify_refs: Vec<ApiCodeRef>,
}

/// Response from reload command
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ReloadResponse {
    pub version: u64,
    pub rebuild_time_ms: u64,
}

/// Request for file content
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct FileRequest {
    pub spec: String,
    pub impl_name: String,
    pub path: String,
}

/// Search result item
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct SearchResult {
    /// "rule" or "file"
    pub kind: String,
    pub id: String,
    #[facet(default)]
    pub text: Option<String>,
    #[facet(default)]
    pub path: Option<String>,
    pub score: f32,
}

/// Request to update a file range (for inline editing)
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct UpdateFileRangeRequest {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub content: String,
    pub file_hash: String,
}

/// Error from file update
#[derive(Debug, Clone, Facet)]
pub struct UpdateError {
    pub message: String,
}

/// Request for validation
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ValidateRequest {
    /// Spec name (optional if only one spec configured)
    #[facet(default)]
    pub spec: Option<String>,
    /// Implementation name (optional if only one impl configured)
    #[facet(default)]
    pub impl_name: Option<String>,
}

/// Notification of data update (sent via streaming)
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct DataUpdate {
    pub version: u64,
    #[facet(default)]
    pub delta: Option<DeltaSummary>,
}

/// Summary of what changed in a rebuild
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct DeltaSummary {
    /// Rules that became covered
    pub newly_covered: Vec<CoverageChange>,
    /// Rules that became uncovered
    pub newly_uncovered: Vec<String>,
}

/// A change in coverage status
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct CoverageChange {
    pub rule_id: String,
    pub file: String,
    pub line: usize,
}

// ============================================================================
// TraceyDaemon service definition
// ============================================================================

/// The tracey daemon RPC service.
///
/// This service is exposed by the daemon over a Unix socket. Bridges (HTTP, MCP, LSP)
/// connect as clients and translate their protocols to/from these RPC calls.
#[service]
pub trait TraceyDaemon {
    // === Core Queries ===

    /// Get coverage status for all specs/impls
    async fn status(&self) -> StatusResponse;

    /// Get uncovered rules (rules without implementation references)
    async fn uncovered(&self, req: UncoveredRequest) -> UncoveredResponse;

    /// Get untested rules (rules with impl but no verify references)
    async fn untested(&self, req: UntestedRequest) -> UntestedResponse;

    /// Get unmapped code (code units without requirement references)
    async fn unmapped(&self, req: UnmappedRequest) -> UnmappedResponse;

    /// Get details for a specific rule by ID
    async fn rule(&self, rule_id: String) -> Option<RuleInfo>;

    // === Configuration ===

    /// Get current configuration
    async fn config(&self) -> ApiConfig;

    // === VFS Overlay (for LSP) ===

    /// Notify that a file was opened with the given content
    async fn vfs_open(&self, path: String, content: String);

    /// Notify that file content changed (unsaved edits)
    async fn vfs_change(&self, path: String, content: String);

    /// Notify that a file was closed (remove from overlay)
    async fn vfs_close(&self, path: String);

    // === Control ===

    /// Force a rebuild of the dashboard data
    async fn reload(&self) -> ReloadResponse;

    /// Get current data version
    async fn version(&self) -> u64;

    /// Subscribe to data updates (streaming)
    ///
    /// The daemon will send `DataUpdate` messages through the pull stream
    /// whenever the dashboard data is rebuilt.
    async fn subscribe(&self, updates: Pull<DataUpdate>);

    // === Dashboard Data ===

    /// Get forward traceability data (rules → code references)
    async fn forward(&self, spec: String, impl_name: String) -> Option<ApiSpecForward>;

    /// Get reverse traceability data (files → coverage)
    async fn reverse(&self, spec: String, impl_name: String) -> Option<ApiReverseData>;

    /// Get file content with syntax highlighting and code units
    async fn file(&self, req: FileRequest) -> Option<ApiFileData>;

    /// Get rendered spec content with outline
    async fn spec_content(&self, spec: String, impl_name: String) -> Option<ApiSpecData>;

    /// Search rules and files
    async fn search(&self, query: String, limit: usize) -> Vec<SearchResult>;

    /// Update a byte range in a file (for inline editing)
    async fn update_file_range(&self, req: UpdateFileRangeRequest) -> Result<(), UpdateError>;

    // === LSP Support ===

    /// Check if a path is a test file (for LSP diagnostics)
    ///
    /// Returns true if the path matches the test_include patterns for any implementation.
    async fn is_test_file(&self, path: String) -> bool;

    // === Validation ===

    /// Validate the spec and implementation for errors
    ///
    /// Returns validation errors such as circular dependencies, naming violations,
    /// and unknown references.
    async fn validate(&self, req: ValidateRequest) -> ValidationResult;
}
