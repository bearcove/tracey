//! Protocol definitions for the tracey daemon RPC service.
//!
//! This crate defines the `TraceyDaemon` service trait using roam's `#[service]`
//! macro. The daemon exposes this service over a Unix socket, and bridges
//! (HTTP, MCP, LSP) connect as clients.

use facet::Facet;
use roam::Tx;
use tracey_core::RuleId;

// Re-export API types for convenience
pub use tracey_api::*;

/// Protocol version — bump this whenever any RPC method is added, removed, or changed.
/// The daemon writes this into its PID file; connectors compare it before connecting
/// to detect stale daemons running an incompatible build.
pub const PROTOCOL_VERSION: u32 = 4;

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
    pub id: RuleId,
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

/// Request for stale references query
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct StaleRequest {
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

/// Response for stale references query
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct StaleResponse {
    pub spec: String,
    pub impl_name: String,
    pub total_rules: usize,
    pub stale_count: usize,
    /// Flat list of stale entries (sorted by file, then line)
    pub refs: Vec<StaleEntry>,
}

/// A single stale reference entry
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct StaleEntry {
    /// Current rule ID (latest version in the spec)
    pub current_id: RuleId,
    /// File containing the stale reference
    pub file: String,
    /// Line number of the stale reference
    pub line: usize,
    /// The rule ID referenced in code (older version)
    pub reference_id: RuleId,
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
    /// Rules with at least one exact implementation reference (not stale).
    pub covered_rules: usize,
    /// Rules where any reference is stale (points to an older rule version).
    /// Not included in covered_rules. covered_rules + stale_rules + uncovered = total.
    pub stale_rules: usize,
    pub verified_rules: usize,
}

/// Information about a specific rule
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct RuleInfo {
    pub id: RuleId,
    /// Raw markdown source (without r[...] marker, but with `>` prefixes for blockquote rules)
    pub raw: String,
    pub html: String,
    #[facet(default)]
    pub source_file: Option<String>,
    #[facet(default)]
    pub source_line: Option<usize>,
    /// Coverage across all implementations
    pub coverage: Vec<RuleCoverage>,
    /// Diff from the previous rule version (N-1 → N), if version > 1 and git history is available.
    /// Inline markdown format: ~~removed~~ / **added**.
    #[facet(default)]
    pub version_diff: Option<String>,
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
    /// "rule" or "source"
    pub kind: String,
    /// For rule: rule ID, for source: file path
    pub id: String,
    /// Line number (0 for rules)
    #[facet(default)]
    pub line: usize,
    /// Raw content (line content or rule text)
    #[facet(default)]
    pub content: Option<String>,
    /// HTML with highlighted matches
    #[facet(default)]
    pub highlighted: Option<String>,
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

/// Response for health check query.
///
/// This provides visibility into daemon internals for monitoring.
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct HealthResponse {
    /// Current data version
    pub version: u64,

    /// Whether the file watcher is active
    pub watcher_active: bool,

    /// Error message if watcher failed (None if healthy)
    #[facet(default)]
    pub watcher_error: Option<String>,

    /// Error message if config file has errors (None if healthy)
    #[facet(default)]
    pub config_error: Option<String>,

    /// Timestamp of last file change event (millis since epoch)
    #[facet(default)]
    pub watcher_last_event_ms: Option<u64>,

    /// Count of file change events received
    pub watcher_event_count: u64,

    /// Directories currently being watched
    pub watched_directories: Vec<String>,

    /// Daemon uptime in seconds
    pub uptime_secs: u64,
}

/// Summary of what changed in a rebuild
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct DeltaSummary {
    /// Rules that became covered
    pub newly_covered: Vec<CoverageChange>,
    /// Rules that became uncovered
    pub newly_uncovered: Vec<RuleId>,
}

/// A change in coverage status
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct CoverageChange {
    pub rule_id: RuleId,
    pub file: String,
    pub line: usize,
}

// ============================================================================
// LSP Support Types
// ============================================================================

/// Position in a file (0-indexed line and column)
#[derive(Debug, Clone, Facet)]
pub struct LspPosition {
    pub path: String,
    pub line: u32,
    pub character: u32,
}

/// A location in a file
#[derive(Debug, Clone, Facet)]
pub struct LspLocation {
    pub path: String,
    pub line: u32,
    pub character: u32,
}

/// A code reference location for hover links
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct HoverRef {
    /// Relative file path
    pub file: String,
    /// Line number (1-indexed)
    pub line: usize,
}

/// Hover information for a requirement reference
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct HoverInfo {
    /// Rule ID
    pub rule_id: RuleId,
    /// Raw markdown source (without r[...] marker)
    pub raw: String,
    /// Spec name this rule belongs to
    pub spec_name: String,
    /// Spec source URL (if configured)
    #[facet(default)]
    pub spec_url: Option<String>,
    /// Source file where the rule is defined
    #[facet(default)]
    pub source_file: Option<String>,
    /// Number of impl references
    pub impl_count: usize,
    /// Number of verify references
    pub verify_count: usize,
    /// Implementation references (file:line)
    #[facet(default)]
    pub impl_refs: Vec<HoverRef>,
    /// Verification references (file:line)
    #[facet(default)]
    pub verify_refs: Vec<HoverRef>,
    /// Range of the reference (for highlighting)
    pub range_start_line: u32,
    pub range_start_char: u32,
    pub range_end_line: u32,
    pub range_end_char: u32,
    /// Diff between a previous rule version and the current one (inline markdown format).
    /// Present for tail annotations (current version, version > 1) and stale annotations.
    #[facet(default)]
    pub version_diff: Option<String>,
}

/// A completion item
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct LspCompletionItem {
    /// The label to display
    pub label: String,
    /// Kind: "verb" or "rule"
    pub kind: String,
    /// Detail text (spec name or verb description)
    #[facet(default)]
    pub detail: Option<String>,
    /// Documentation (rule text)
    #[facet(default)]
    pub documentation: Option<String>,
    /// Text to insert (may include trailing space)
    #[facet(default)]
    pub insert_text: Option<String>,
}

/// A diagnostic (error/warning)
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct LspDiagnostic {
    /// Severity: "error", "warning", "info", "hint"
    pub severity: String,
    /// Error code
    pub code: String,
    /// Message
    pub message: String,
    /// Range
    pub start_line: u32,
    pub start_char: u32,
    pub end_line: u32,
    pub end_char: u32,
}

/// Diagnostics for a single file (used in workspace diagnostics)
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct LspFileDiagnostics {
    /// File path (relative to project root)
    pub path: String,
    /// Diagnostics for this file
    pub diagnostics: Vec<LspDiagnostic>,
}

/// A document symbol (requirement reference)
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct LspSymbol {
    /// Symbol name (rule ID)
    pub name: String,
    /// Kind: "definition", "impl", "verify", etc.
    pub kind: String,
    /// Source file path for the symbol, relative to project root when available
    #[facet(default)]
    pub path: Option<String>,
    /// Range
    pub start_line: u32,
    pub start_char: u32,
    pub end_line: u32,
    pub end_char: u32,
}

/// A semantic token
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct LspSemanticToken {
    pub line: u32,
    pub start_char: u32,
    pub length: u32,
    /// Token type index
    pub token_type: u32,
    /// Token modifiers bitmask
    pub modifiers: u32,
}

/// A code lens
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct LspCodeLens {
    /// Range (line where the lens appears)
    pub line: u32,
    pub start_char: u32,
    pub end_char: u32,
    /// Title to display
    pub title: String,
    /// Command name
    pub command: String,
    /// Command arguments (JSON)
    #[facet(default)]
    pub arguments: Vec<String>,
}

/// An inlay hint
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct LspInlayHint {
    /// Position (end of reference)
    pub line: u32,
    pub character: u32,
    /// Label text
    pub label: String,
}

/// Rename preparation result
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct PrepareRenameResult {
    /// Range of the text to be renamed
    pub start_line: u32,
    pub start_char: u32,
    pub end_line: u32,
    pub end_char: u32,
    /// Current text (for display)
    pub placeholder: String,
}

/// A text edit for rename
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct LspTextEdit {
    pub path: String,
    pub start_line: u32,
    pub start_char: u32,
    pub end_line: u32,
    pub end_char: u32,
    pub new_text: String,
}

/// A code action
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct LspCodeAction {
    /// Title
    pub title: String,
    /// Kind: "quickfix", "source", etc.
    pub kind: String,
    /// Command name
    pub command: String,
    /// Command arguments
    #[facet(default)]
    pub arguments: Vec<String>,
    /// Is this the preferred action?
    #[facet(default)]
    pub is_preferred: bool,
}

/// Request for inlay hints
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct InlayHintsRequest {
    pub path: String,
    pub content: String,
    pub start_line: u32,
    pub end_line: u32,
}

/// Request to add config pattern
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct ConfigPatternRequest {
    #[facet(default)]
    pub spec: Option<String>,
    #[facet(default)]
    pub impl_name: Option<String>,
    pub pattern: String,
}

/// Request for LSP operations that need path, content, and position
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct LspPositionRequest {
    pub path: String,
    pub content: String,
    pub line: u32,
    pub character: u32,
}

/// Request for LSP references
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct LspReferencesRequest {
    pub path: String,
    pub content: String,
    pub line: u32,
    pub character: u32,
    pub include_declaration: bool,
}

/// Request for LSP rename
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct LspRenameRequest {
    pub path: String,
    pub content: String,
    pub line: u32,
    pub character: u32,
    pub new_name: String,
}

/// Request for LSP operations that need path and content
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
pub struct LspDocumentRequest {
    pub path: String,
    pub content: String,
}

// ============================================================================
// TraceyDaemon service definition
// ============================================================================

/// The tracey daemon RPC service.
///
/// This service is exposed by the daemon over a Unix socket. Bridges (HTTP, MCP, LSP)
/// connect as clients and translate their protocols to/from these RPC calls.
#[roam::service]
pub trait TraceyDaemon {
    // === Core Queries ===

    /// Get coverage status for all specs/impls
    async fn status(&self) -> StatusResponse;

    /// Get uncovered rules (rules without implementation references)
    async fn uncovered(&self, req: UncoveredRequest) -> UncoveredResponse;

    /// Get untested rules (rules with impl but no verify references)
    async fn untested(&self, req: UntestedRequest) -> UntestedResponse;

    /// Get stale references (code pointing to older rule versions)
    async fn stale(&self, req: StaleRequest) -> StaleResponse;

    /// Get unmapped code (code units without requirement references)
    async fn unmapped(&self, req: UnmappedRequest) -> UnmappedResponse;

    /// Get details for a specific rule by ID
    async fn rule(&self, rule_id: RuleId) -> Option<RuleInfo>;

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

    /// Get daemon health status
    async fn health(&self) -> HealthResponse;

    /// Request the daemon to shut down gracefully
    async fn shutdown(&self);

    /// Subscribe to data updates (streaming)
    ///
    /// The daemon will send `DataUpdate` messages through the Tx channel
    /// whenever the dashboard data is rebuilt.
    async fn subscribe(&self, updates: Tx<DataUpdate>);

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
    async fn search(&self, query: String, limit: u32) -> Vec<SearchResult>;

    /// Update a byte range in a file (for inline editing)
    async fn update_file_range(&self, req: UpdateFileRangeRequest) -> Result<(), UpdateError>;

    // === LSP Support ===

    /// Check if a path is a test file (for LSP diagnostics)
    ///
    /// Returns true if the path matches the test_include patterns for any implementation.
    async fn is_test_file(&self, path: String) -> bool;

    /// Get hover info for a position in a file
    async fn lsp_hover(&self, req: LspPositionRequest) -> Option<HoverInfo>;

    /// Get definition location for a reference at a position
    async fn lsp_definition(&self, req: LspPositionRequest) -> Vec<LspLocation>;

    /// Get implementation locations for a reference at a position
    async fn lsp_implementation(&self, req: LspPositionRequest) -> Vec<LspLocation>;

    /// Get all references to a requirement
    async fn lsp_references(&self, req: LspReferencesRequest) -> Vec<LspLocation>;

    /// Get completions for a position
    async fn lsp_completions(&self, req: LspPositionRequest) -> Vec<LspCompletionItem>;

    /// Get diagnostics for a file
    async fn lsp_diagnostics(&self, req: LspDocumentRequest) -> Vec<LspDiagnostic>;

    /// Get diagnostics for all files in the workspace
    ///
    /// This returns diagnostics for all spec files and implementation files
    /// known to tracey, without requiring the files to be opened.
    async fn lsp_workspace_diagnostics(&self) -> Vec<LspFileDiagnostics>;

    /// Get document symbols (requirement references) in a file
    async fn lsp_document_symbols(&self, req: LspDocumentRequest) -> Vec<LspSymbol>;

    /// Search workspace for requirement IDs
    async fn lsp_workspace_symbols(&self, query: String) -> Vec<LspSymbol>;

    /// Get semantic tokens for syntax highlighting
    async fn lsp_semantic_tokens(&self, req: LspDocumentRequest) -> Vec<LspSemanticToken>;

    /// Get code lens items
    async fn lsp_code_lens(&self, req: LspDocumentRequest) -> Vec<LspCodeLens>;

    /// Get inlay hints for a range
    async fn lsp_inlay_hints(&self, req: InlayHintsRequest) -> Vec<LspInlayHint>;

    /// Prepare rename (check if renaming is valid)
    async fn lsp_prepare_rename(&self, req: LspPositionRequest) -> Option<PrepareRenameResult>;

    /// Execute rename
    async fn lsp_rename(&self, req: LspRenameRequest) -> Vec<LspTextEdit>;

    /// Get code actions for a position
    async fn lsp_code_actions(&self, req: LspPositionRequest) -> Vec<LspCodeAction>;

    /// Get document highlight ranges (same requirement references)
    async fn lsp_document_highlight(&self, req: LspPositionRequest) -> Vec<LspLocation>;

    // === Validation ===

    /// Validate the spec and implementation for errors
    ///
    /// Returns validation errors such as circular dependencies, naming violations,
    /// and unknown references.
    async fn validate(&self, req: ValidateRequest) -> ValidationResult;

    // === Config Modification (for MCP) ===

    /// Add an exclude pattern to an implementation
    async fn config_add_exclude(&self, req: ConfigPatternRequest) -> Result<(), String>;

    /// Add an include pattern to an implementation
    async fn config_add_include(&self, req: ConfigPatternRequest) -> Result<(), String>;
}
