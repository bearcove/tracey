//! Dashboard data building.
//!
//! This module contains the core data structures and functions for building
//! the `DashboardData` that powers the tracey dashboard, MCP, and LSP.

#![allow(dead_code)]

use eyre::Result;
use owo_colors::OwoColorize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};
use tracey_core::code_units::CodeUnit;
use tracey_core::is_supported_extension;
use tracey_core::{
    ParseWarning, RefVerb, ReqDefinition, ReqReference, Reqs, RuleId, RuleIdMatch,
    classify_reference_for_rule, parse_rule_id,
};
use tracing::info;

// Markdown rendering
use marq::{
    AasvgHandler, ArboriumHandler, CompareHandler, InlineCodeHandler, PikruHandler, RenderOptions,
    ReqHandler, parse_frontmatter, render,
};

use crate::config::Config;
use crate::rule_suggestions::suggest_similar_rule_ids;
use crate::search;

// ============================================================================
// JSON API Types
// ============================================================================

// Re-export API types from tracey-api crate
pub use tracey_api::{
    ApiCodeRef, ApiCodeUnit, ApiConfig, ApiFileData, ApiFileEntry, ApiForwardData, ApiReverseData,
    ApiRule, ApiSpecData, ApiSpecForward, ApiSpecInfo, ApiStaleRef, GitStatus, OutlineCoverage,
    OutlineEntry, SpecSection, ValidationError, ValidationErrorCode, ValidationResult,
};
use tracey_proto::{LspDiagnostic, LspFileDiagnostics};

// ============================================================================
// Core Types
// ============================================================================

/// Key for implementation-specific data: (spec_name, impl_name)
pub type ImplKey = (String, String);

/// Computed dashboard data that gets rebuilt on file changes
pub struct DashboardData {
    pub config: ApiConfig,
    /// Forward data per implementation: (spec_name, impl_name) -> data
    pub forward_by_impl: BTreeMap<ImplKey, ApiSpecForward>,
    /// Reverse data per implementation: (spec_name, impl_name) -> data
    pub reverse_by_impl: BTreeMap<ImplKey, ApiReverseData>,
    /// Code units per implementation for file API
    pub code_units_by_impl: BTreeMap<ImplKey, BTreeMap<PathBuf, Vec<CodeUnit>>>,
    /// Spec content per implementation (coverage info varies by impl)
    pub specs_content_by_impl: BTreeMap<ImplKey, ApiSpecData>,
    /// Spec include patterns by spec name
    pub spec_includes_by_name: BTreeMap<String, Vec<String>>,
    /// Source files for full-text index construction
    pub search_files: BTreeMap<PathBuf, String>,
    /// Parsed requirement references and warnings by source file, captured during rebuild.
    /// Includes all prefixes found in scanned files (not filtered to a spec prefix).
    pub source_reqs_by_file: BTreeMap<PathBuf, Reqs>,
    /// Rules for full-text index construction
    pub search_rules: Vec<search::RuleEntry>,
    /// Precomputed validation diagnostics by (spec, impl).
    pub validation_by_impl: BTreeMap<ImplKey, ValidationResult>,
    /// Precomputed workspace diagnostics for LSP publishing.
    pub workspace_diagnostics: Vec<LspFileDiagnostics>,
    /// Version number (incremented only when content actually changes)
    pub version: u64,
    /// Hash of forward + reverse JSON for change detection
    pub content_hash: u64,
    /// Delta from previous build (what changed)
    pub delta: crate::server::Delta,
    /// Files matched by test_include patterns (only verify allowed)
    /// r[impl config.impl.test_include]
    pub test_files: std::collections::HashSet<PathBuf>,
}

#[derive(Default)]
pub struct BuildCache {
    source_files: HashMap<PathBuf, CachedSourceFile>,
    impl_scan_paths: HashMap<ImplScanKey, CachedScanPaths>,
    spec_scan_paths: HashMap<SpecScanKey, CachedScanPaths>,
    markdown_files: HashMap<PathBuf, CachedMarkdownFile>,
}

#[derive(Clone)]
struct CachedSourceFile {
    content_hash: u64,
    file_len: u64,
    modified_nanos: Option<u128>,
    content: String,
    refs: Vec<ReqReference>,
    parse_warnings: Vec<ParseWarning>,
    code_units: Vec<CodeUnit>,
}

#[derive(Default)]
struct CacheStats {
    metadata_hits: usize,
    hash_hits: usize,
    misses: usize,
    reparsed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ImplScanKey {
    project_root: PathBuf,
    include: Vec<String>,
    exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SpecScanKey {
    project_root: PathBuf,
    include: Vec<String>,
}

#[derive(Default, Clone)]
struct CachedScanPaths {
    files: BTreeSet<PathBuf>,
}

#[derive(Clone)]
struct CachedMarkdownFile {
    content_hash: u64,
    file_len: u64,
    modified_nanos: Option<u128>,
    extracted_rules: Vec<crate::ExtractedRule>,
}

/// Escape HTML special characters
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

// ============================================================================
// Rule Handler
// ============================================================================

/// Coverage status for a rule
#[derive(Debug, Clone)]
struct RuleCoverage {
    status: &'static str, // "covered", "partial", "stale", "uncovered"
    impl_refs: Vec<ApiCodeRef>,
    verify_refs: Vec<ApiCodeRef>,
}

/// Custom rule handler that renders rules with coverage status and refs
struct TraceyRuleHandler {
    coverage: BTreeMap<String, RuleCoverage>,
    /// Current source file being rendered (shared with rendering loop)
    current_source_file: Arc<Mutex<String>>,
    /// Spec name for URL generation
    spec_name: String,
    /// Implementation name for URL generation
    impl_name: String,
    /// Project root for absolute paths
    project_root: PathBuf,
    /// Git status for files
    git_status: HashMap<String, GitStatus>,
}

impl TraceyRuleHandler {
    fn new(
        coverage: BTreeMap<String, RuleCoverage>,
        current_source_file: Arc<Mutex<String>>,
        spec_name: String,
        impl_name: String,
        project_root: PathBuf,
        git_status: HashMap<String, GitStatus>,
    ) -> Self {
        Self {
            coverage,
            current_source_file,
            spec_name,
            impl_name,
            project_root,
            git_status,
        }
    }
}

/// @tracey:ignore-next-line
/// Custom inline code handler that transforms `r[rule.id]` into clickable links.
struct TraceyInlineCodeHandler {
    /// Spec name for URL generation
    spec_name: String,
    /// Implementation name for URL generation
    impl_name: String,
}

impl TraceyInlineCodeHandler {
    fn new(spec_name: String, impl_name: String) -> Self {
        Self {
            spec_name,
            impl_name,
        }
    }
}

impl InlineCodeHandler for TraceyInlineCodeHandler {
    fn render(&self, code: &str) -> Option<String> {
        let code = code.trim();

        // @tracey:ignore-next-line
        // Match r[rule.id] pattern
        if !code.starts_with("r[") || !code.ends_with(']') {
            return None;
        }

        // @tracey:ignore-next-line
        // Extract rule.id from r[rule.id]
        let rule_id = &code[2..code.len() - 1];

        // Validate it looks like a rule ID (alphanumeric, dots, dashes, underscores)
        if rule_id.is_empty()
            || !rule_id
                .chars()
                .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_' || c == '+')
        {
            return None;
        }

        // Generate link: /{spec}/{impl}/spec#r-{rule_id}
        // The anchor format matches what TraceyRuleHandler generates (r-{rule_id})
        let anchor = format!("r-{}", rule_id);
        Some(format!(
            r#"<code><a href="/{}/{}/spec#{}" class="rule-ref">{}</a></code>"#,
            self.spec_name,
            self.impl_name,
            anchor,
            html_escape(code)
        ))
    }
}

/// Get devicon class for a file path based on extension
fn devicon_class(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?;
    match ext {
        // Systems languages
        "rs" => Some("devicon-rust-original"),
        "go" => Some("devicon-go-plain"),
        "zig" => Some("devicon-zig-original"),
        "c" => Some("devicon-c-plain"),
        "h" => Some("devicon-c-plain"),
        "cpp" | "cc" | "cxx" => Some("devicon-cplusplus-plain"),
        "hpp" | "hh" | "hxx" => Some("devicon-cplusplus-plain"),
        // Web/JS ecosystem
        "js" | "mjs" | "cjs" => Some("devicon-javascript-plain"),
        "ts" | "mts" | "cts" => Some("devicon-typescript-plain"),
        "jsx" => Some("devicon-javascript-plain"),
        "tsx" => Some("devicon-typescript-plain"),
        "vue" => Some("devicon-vuejs-plain"),
        "svelte" => Some("devicon-svelte-plain"),
        // Mobile
        "swift" => Some("devicon-swift-plain"),
        "kt" | "kts" => Some("devicon-kotlin-plain"),
        "dart" => Some("devicon-dart-plain"),
        // JVM
        "java" => Some("devicon-java-plain"),
        "scala" => Some("devicon-scala-plain"),
        "clj" | "cljs" | "cljc" => Some("devicon-clojure-plain"),
        "groovy" => Some("devicon-groovy-plain"),
        // Scripting
        "py" => Some("devicon-python-plain"),
        "rb" => Some("devicon-ruby-plain"),
        "php" => Some("devicon-php-plain"),
        "lua" => Some("devicon-lua-plain"),
        "pl" | "pm" => Some("devicon-perl-plain"),
        "r" => Some("devicon-r-plain"),
        "jl" => Some("devicon-julia-plain"),
        // Functional
        "hs" | "lhs" => Some("devicon-haskell-plain"),
        "ml" | "mli" => Some("devicon-ocaml-plain"),
        "ex" | "exs" => Some("devicon-elixir-plain"),
        "erl" | "hrl" => Some("devicon-erlang-plain"),
        "fs" | "fsi" | "fsx" => Some("devicon-fsharp-plain"),
        // Shell
        "sh" | "bash" | "zsh" => Some("devicon-bash-plain"),
        "ps1" | "psm1" => Some("devicon-powershell-plain"),
        // Config/data
        "json" => Some("devicon-json-plain"),
        "yaml" | "yml" => Some("devicon-yaml-plain"),
        "toml" => Some("devicon-toml-plain"),
        "xml" => Some("devicon-xml-plain"),
        "sql" => Some("devicon-postgresql-plain"),
        // Web
        "html" | "htm" => Some("devicon-html5-plain"),
        "css" => Some("devicon-css3-plain"),
        "scss" | "sass" => Some("devicon-sass-original"),
        // Docs
        "md" | "markdown" => Some("devicon-markdown-original"),
        _ => None,
    }
}

// r[impl markdown.html.div] - rule wrapped in <div class="rule-container">
// r[impl markdown.html.anchor] - div has id="r-{rule.id}"
// r[impl markdown.html.link] - rule-badge links to the rule
impl ReqHandler for TraceyRuleHandler {
    fn start<'a>(
        &'a self,
        rule: &'a ReqDefinition,
    ) -> Pin<Box<dyn Future<Output = marq::Result<String>> + Send + 'a>> {
        Box::pin(async move {
            let rule_id = rule.id.to_string();
            let coverage = self.coverage.get(&rule_id);
            let status = coverage.map(|c| c.status).unwrap_or("uncovered");

            // Insert <wbr> after dots for better line breaking
            let display_id = rule_id.replace('.', ".<wbr>");

            // Get current source file for this rule (make it absolute)
            let relative_source = self.current_source_file.lock().unwrap().clone();
            let absolute_source = self.project_root.join(&relative_source);
            let source_file = absolute_source.display().to_string();

            // Build the badges that pierce the top border
            let mut badges_html = String::new();

            // r[impl dashboard.editing.copy.button]
            // r[impl dashboard.links.req-links]
            // Segmented badge group: copy button + requirement ID
            badges_html.push_str(&format!(
                r#"<div class="req-badge-group"><button class="req-badge req-copy req-segment-left" data-req-id="{}" title="Copy requirement ID"><svg class="req-copy-icon" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg></button><a class="req-badge req-id req-segment-right" href="/{}/{}/spec#r--{}" data-rule="{}" data-source-file="{}" data-source-line="{}" title="{}">{}</a></div>"#,
                &rule_id,
                self.spec_name, self.impl_name, &rule_id, &rule_id, source_file, rule.line, &rule_id, display_id
            ));

            // Implementation badge
            if let Some(cov) = coverage {
                if !cov.impl_refs.is_empty() {
                    let r = &cov.impl_refs[0];
                    let filename = r.file.rsplit('/').next().unwrap_or(&r.file);
                    let icon = devicon_class(&r.file)
                        .map(|c| format!(r#"<i class="{c}"></i> "#))
                        .unwrap_or_default();
                    let count_suffix = if cov.impl_refs.len() > 1 {
                        format!(" +{}", cov.impl_refs.len() - 1)
                    } else {
                        String::new()
                    };
                    // Serialize all refs as JSON for popup (manual, no serde)
                    let all_refs_json = cov
                        .impl_refs
                        .iter()
                        .map(|r| {
                            format!(
                                r#"{{"file":"{}","line":{}}}"#,
                                r.file.replace('\\', "\\\\").replace('"', "\\\""),
                                r.line
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    let all_refs_json = format!("[{}]", all_refs_json).replace('"', "&quot;");
                    // r[impl dashboard.links.impl-refs]
                    badges_html.push_str(&format!(
                        r#"<a class="req-badge req-impl" href="/{}/{}/sources/{}:{}" data-file="{}" data-line="{}" data-all-refs="{}" title="Implementation: {}:{}">{icon}{}:{}{}</a>"#,
                        self.spec_name, self.impl_name, r.file, r.line, r.file, r.line, all_refs_json, r.file, r.line, filename, r.line, count_suffix
                    ));
                }

                // r[impl dashboard.links.verify-refs]
                if !cov.verify_refs.is_empty() {
                    let r = &cov.verify_refs[0];
                    let filename = r.file.rsplit('/').next().unwrap_or(&r.file);
                    let icon = devicon_class(&r.file)
                        .map(|c| format!(r#"<i class="{c}"></i> "#))
                        .unwrap_or_default();
                    let count_suffix = if cov.verify_refs.len() > 1 {
                        format!(" +{}", cov.verify_refs.len() - 1)
                    } else {
                        String::new()
                    };
                    // Serialize all refs as JSON for popup (manual, no serde)
                    let all_refs_json = cov
                        .verify_refs
                        .iter()
                        .map(|r| {
                            format!(
                                r#"{{"file":"{}","line":{}}}"#,
                                r.file.replace('\\', "\\\\").replace('"', "\\\""),
                                r.line
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    let all_refs_json = format!("[{}]", all_refs_json).replace('"', "&quot;");
                    badges_html.push_str(&format!(
                        r#"<a class="req-badge req-test" href="/{}/{}/sources/{}:{}" data-file="{}" data-line="{}" data-all-refs="{}" title="Test: {}:{}">{icon}{}:{}{}</a>"#,
                        self.spec_name, self.impl_name, r.file, r.line, r.file, r.line, all_refs_json, r.file, r.line, filename, r.line, count_suffix
                    ));
                }
            }

            // r[impl dashboard.editing.byte-range.attribute]
            // r[impl dashboard.editing.badge.display]
            // r[impl dashboard.editing.badge.appearance]
            // Edit badge - separate group on the right
            let edit_badge_html = format!(
                r#"<button class="req-badge req-edit" data-br="{}-{}" data-source-file="{}" title="Edit this requirement"><svg class="req-edit-icon" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"></path><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"></path></svg> Edit</button>"#,
                rule.span.offset,
                rule.span.offset + rule.span.length,
                source_file
            );

            // Render the opening of the req container
            Ok(format!(
                r#"<div class="req-container req-{status}" id="{anchor}" data-br="{br_start}-{br_end}">
<div class="req-badges-left">{badges}</div>
<div class="req-badges-right">{edit_badge}</div>
<div class="req-content">"#,
                status = status,
                anchor = rule.anchor_id,
                br_start = rule.span.offset,
                br_end = rule.span.offset + rule.span.length,
                badges = badges_html,
                edit_badge = edit_badge_html,
            ))
        })
    }

    fn end<'a>(
        &'a self,
        _rule: &'a ReqDefinition,
    ) -> Pin<Box<dyn Future<Output = marq::Result<String>> + Send + 'a>> {
        Box::pin(async move {
            // Close the rule container
            Ok("</div>\n</div>".to_string())
        })
    }
}

// ============================================================================
// Git Status
// ============================================================================

/// Get git status for all files in the repository
/// Returns a map of file path -> GitStatus
fn get_git_status(project_root: &Path) -> HashMap<String, GitStatus> {
    let mut status_map = HashMap::new();

    // Run git status --porcelain to get file statuses
    let output = match std::process::Command::new("git")
        .arg("status")
        .arg("--porcelain")
        .current_dir(project_root)
        .output()
    {
        Ok(output) => output,
        Err(_) => return status_map, // Not a git repo or git not available
    };

    if !output.status.success() {
        return status_map;
    }

    let status_text = String::from_utf8_lossy(&output.stdout);
    for line in status_text.lines() {
        if line.len() < 4 {
            continue;
        }

        // Git status --porcelain format: "XY filename"
        // X = index status, Y = working tree status
        let index_status = line.chars().next().unwrap_or(' ');
        let worktree_status = line.chars().nth(1).unwrap_or(' ');
        let filename = line[3..].trim().to_string();

        let status = if worktree_status != ' ' && worktree_status != '?' {
            // Working tree has changes (dirty)
            GitStatus::Dirty
        } else if index_status != ' ' && index_status != '?' {
            // Changes staged in index
            GitStatus::Staged
        } else {
            // Clean or unknown
            GitStatus::Clean
        };

        status_map.insert(filename, status);
    }

    status_map
}

// ============================================================================
// Data Building
// ============================================================================

/// Compute relative path from `from` to `to`, preserving ../ for cross-workspace paths
fn compute_relative_path(from: &Path, to: &Path) -> String {
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();

    let mut common_len = 0;
    for (a, b) in from_components.iter().zip(to_components.iter()) {
        if a == b {
            common_len += 1;
        } else {
            break;
        }
    }

    // Build relative path: ../ for each component in from after common, then to components
    let mut result = PathBuf::new();
    for _ in common_len..from_components.len() {
        result.push("..");
    }
    for component in &to_components[common_len..] {
        result.push(component);
    }

    result.display().to_string()
}

/// File content overlay - maps absolute paths to content
/// Used by LSP to provide VFS content for open files
pub type FileOverlay = std::collections::HashMap<PathBuf, String>;

/// Read a file, checking the overlay first, then falling back to disk
///
/// r[impl daemon.vfs.priority]
async fn read_file_with_overlay(path: &Path, overlay: &FileOverlay) -> std::io::Result<String> {
    // Check overlay first (for open files in LSP)
    if let Some(content) = overlay.get(path) {
        return Ok(content.clone());
    }
    // Try canonicalized path too
    if let Ok(canonical) = path.canonicalize()
        && let Some(content) = overlay.get(&canonical)
    {
        return Ok(content.clone());
    }
    // Fall back to disk (async)
    tokio::fs::read_to_string(path).await
}

fn file_modified_nanos(modified: SystemTime) -> Option<u128> {
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_nanos())
}

fn compute_content_hash(content: &str) -> u64 {
    simple_hash(content)
}

fn compute_column_for_content(content: &str, byte_offset: usize) -> usize {
    let before = &content[..byte_offset.min(content.len())];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    before[line_start..].chars().count() + 1
}

fn extract_marker_prefix_from_content(
    content: &str,
    marker_span: marq::SourceSpan,
) -> Option<String> {
    let start = marker_span.offset;
    let end = start.checked_add(marker_span.length)?;
    let marker = content.get(start..end)?;
    let bracket = marker.find('[')?;
    let prefix = marker[..bracket].trim();
    if prefix.is_empty() {
        return None;
    }
    Some(prefix.to_string())
}

async fn get_cached_source_file(
    path: &Path,
    overlay: &FileOverlay,
    cache: &mut BuildCache,
    stats: &mut CacheStats,
) -> std::io::Result<CachedSourceFile> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    let overlay_content = overlay
        .get(path)
        .or_else(|| overlay.get(&canonical))
        .cloned();

    if let Some(content) = overlay_content {
        let content_hash = compute_content_hash(&content);
        if let Some(entry) = cache.source_files.get(&canonical)
            && entry.content_hash == content_hash
        {
            stats.hash_hits += 1;
            return Ok(entry.clone());
        }

        let reqs = Reqs::extract_from_content(&canonical, &content);
        let code_units = tracey_core::code_units::extract(&canonical, &content).units;
        let parsed = CachedSourceFile {
            content_hash,
            file_len: content.len() as u64,
            modified_nanos: None,
            content,
            refs: reqs.references,
            parse_warnings: reqs.warnings,
            code_units,
        };
        stats.misses += 1;
        stats.reparsed += 1;
        cache.source_files.insert(canonical, parsed.clone());
        return Ok(parsed);
    }

    let metadata = tokio::fs::metadata(&canonical).await?;
    let file_len = metadata.len();
    let modified_nanos = metadata.modified().ok().and_then(file_modified_nanos);

    if let Some(entry) = cache.source_files.get(&canonical)
        && entry.file_len == file_len
        && entry.modified_nanos == modified_nanos
    {
        stats.metadata_hits += 1;
        return Ok(entry.clone());
    }

    let content = read_file_with_overlay(&canonical, overlay).await?;
    let content_hash = compute_content_hash(&content);

    if let Some(entry) = cache.source_files.get(&canonical)
        && entry.content_hash == content_hash
    {
        let mut updated = entry.clone();
        updated.file_len = file_len;
        updated.modified_nanos = modified_nanos;
        cache.source_files.insert(canonical, updated.clone());
        stats.hash_hits += 1;
        return Ok(updated);
    }

    let reqs = Reqs::extract_from_content(&canonical, &content);
    let code_units = tracey_core::code_units::extract(&canonical, &content).units;
    let parsed = CachedSourceFile {
        content_hash,
        file_len,
        modified_nanos,
        content,
        refs: reqs.references,
        parse_warnings: reqs.warnings,
        code_units,
    };
    stats.misses += 1;
    stats.reparsed += 1;
    cache.source_files.insert(canonical, parsed.clone());
    Ok(parsed)
}

#[derive(Clone)]
struct ScanRootPattern {
    root: PathBuf,
    pattern: String,
}

fn build_scan_roots(
    project_root: &Path,
    include: &[String],
) -> (Vec<ScanRootPattern>, Vec<String>) {
    let mut roots = Vec::new();
    let mut warnings = Vec::new();

    if include.is_empty() {
        roots.push(ScanRootPattern {
            root: project_root.to_path_buf(),
            pattern: "**/*".to_string(),
        });
        return (roots, warnings);
    }

    for pattern in include {
        if pattern.starts_with("../") {
            let base_path =
                if let Some(wildcard_pos) = pattern.find("**").or_else(|| pattern.find('*')) {
                    pattern[..wildcard_pos].trim_end_matches('/')
                } else {
                    pattern.as_str()
                };
            let resolved_path = project_root.join(base_path);
            if !resolved_path.exists() {
                warnings.push(format!(
                    "Warning: Cross-workspace path not found: {}\n  Pattern: {}",
                    base_path, pattern
                ));
                continue;
            }
            let adjusted_pattern = if let Some(suffix) = pattern.strip_prefix(base_path) {
                suffix.trim_start_matches('/').to_string()
            } else {
                pattern.to_string()
            };
            roots.push(ScanRootPattern {
                root: resolved_path,
                pattern: adjusted_pattern,
            });
        } else {
            roots.push(ScanRootPattern {
                root: project_root.to_path_buf(),
                pattern: pattern.clone(),
            });
        }
    }

    (roots, warnings)
}

fn path_matches_root_pattern(path: &Path, root_pattern: &ScanRootPattern) -> bool {
    let Ok(relative) = path.strip_prefix(&root_pattern.root) else {
        return false;
    };
    let relative_str = relative.to_string_lossy();
    glob_match(&relative_str, &root_pattern.pattern)
}

fn path_matches_any_root(path: &Path, roots: &[ScanRootPattern]) -> bool {
    roots.iter().any(|r| path_matches_root_pattern(path, r))
}

fn path_matches_excludes(path: &Path, roots: &[ScanRootPattern], exclude: &[String]) -> bool {
    roots.iter().any(|r| {
        let Ok(relative) = path.strip_prefix(&r.root) else {
            return false;
        };
        let relative_str = relative.to_string_lossy();
        exclude
            .iter()
            .any(|pattern| glob_match(&relative_str, pattern))
    })
}

fn full_walk_for_roots(
    roots: &[ScanRootPattern],
    include_supported_ext_only: bool,
    include_markdown_only: bool,
    exclude: &[String],
) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    for root_pattern in roots {
        let walker = ignore::WalkBuilder::new(&root_pattern.root)
            .follow_links(true)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            let Some(ft) = entry.file_type() else {
                continue;
            };
            if !ft.is_file() {
                continue;
            }
            if include_markdown_only && path.extension().is_none_or(|ext| ext != "md") {
                continue;
            }
            if include_supported_ext_only
                && path
                    .extension()
                    .is_none_or(|ext| !is_supported_extension(ext))
            {
                continue;
            }
            if !path_matches_root_pattern(path, root_pattern) {
                continue;
            }
            if path_matches_excludes(path, roots, exclude) {
                continue;
            }
            let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            out.insert(canonical);
        }
    }
    out
}

fn update_cached_scan_paths(
    existing: &mut CachedScanPaths,
    roots: &[ScanRootPattern],
    changed_files: &[PathBuf],
    include_supported_ext_only: bool,
    include_markdown_only: bool,
    exclude: &[String],
) {
    for changed in changed_files {
        let canonical = changed
            .canonicalize()
            .unwrap_or_else(|_| changed.to_path_buf());
        let exists = canonical.exists();
        let ext_ok = if include_markdown_only {
            canonical.extension().is_some_and(|ext| ext == "md")
        } else if include_supported_ext_only {
            canonical.extension().is_some_and(is_supported_extension)
        } else {
            true
        };
        let included = ext_ok
            && path_matches_any_root(&canonical, roots)
            && !path_matches_excludes(&canonical, roots, exclude);

        if exists && included {
            existing.files.insert(canonical);
        } else {
            existing.files.remove(&canonical);
        }
    }
}

fn get_cached_impl_scan_paths(
    project_root: &Path,
    include: &[String],
    exclude: &[String],
    changed_files: &[PathBuf],
    cache: &mut BuildCache,
) -> (BTreeSet<PathBuf>, Vec<String>, bool) {
    let key = ImplScanKey {
        project_root: project_root.to_path_buf(),
        include: include.to_vec(),
        exclude: exclude.to_vec(),
    };
    let (roots, warnings) = build_scan_roots(project_root, include);
    let entry = cache.impl_scan_paths.entry(key).or_default();
    let did_full_walk;
    if entry.files.is_empty() {
        entry.files = full_walk_for_roots(&roots, true, false, exclude);
        did_full_walk = true;
    } else if !changed_files.is_empty() {
        update_cached_scan_paths(entry, &roots, changed_files, true, false, exclude);
        did_full_walk = false;
    } else {
        entry.files = full_walk_for_roots(&roots, true, false, exclude);
        did_full_walk = true;
    }
    (entry.files.clone(), warnings, did_full_walk)
}

fn get_cached_spec_scan_paths(
    project_root: &Path,
    include: &[String],
    changed_files: &[PathBuf],
    cache: &mut BuildCache,
) -> (BTreeSet<PathBuf>, Vec<String>, bool) {
    let key = SpecScanKey {
        project_root: project_root.to_path_buf(),
        include: include.to_vec(),
    };
    let (roots, warnings) = build_scan_roots(project_root, include);
    let entry = cache.spec_scan_paths.entry(key).or_default();
    let did_full_walk;
    if entry.files.is_empty() {
        entry.files = full_walk_for_roots(&roots, false, true, &[]);
        did_full_walk = true;
    } else if !changed_files.is_empty() {
        update_cached_scan_paths(entry, &roots, changed_files, false, true, &[]);
        did_full_walk = false;
    } else {
        entry.files = full_walk_for_roots(&roots, false, true, &[]);
        did_full_walk = true;
    }
    (entry.files.clone(), warnings, did_full_walk)
}

async fn extract_markdown_rules_cached(
    project_root: &Path,
    path: &Path,
    overlay: &FileOverlay,
    cache: &mut BuildCache,
    quiet: bool,
    stats: &mut CacheStats,
) -> Result<Vec<crate::ExtractedRule>> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let overlay_content = overlay
        .get(path)
        .or_else(|| overlay.get(&canonical))
        .cloned();
    let overlay_is_present = overlay_content.is_some();

    let (content, file_len, modified_nanos) = if let Some(content) = overlay_content {
        (content.clone(), content.len() as u64, None)
    } else {
        let metadata = tokio::fs::metadata(&canonical).await.ok();
        let file_len = metadata.as_ref().map_or(0, std::fs::Metadata::len);
        let modified_nanos = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(file_modified_nanos);
        (
            read_file_with_overlay(&canonical, overlay).await?,
            file_len,
            modified_nanos,
        )
    };

    let content_hash = compute_content_hash(&content);
    if let Some(entry) = cache.markdown_files.get(&canonical) {
        if !overlay_is_present
            && entry.file_len == file_len
            && entry.modified_nanos == modified_nanos
        {
            stats.metadata_hits += 1;
            return Ok(entry.extracted_rules.clone());
        }
        if entry.content_hash == content_hash {
            let updated = CachedMarkdownFile {
                content_hash,
                file_len,
                modified_nanos,
                extracted_rules: entry.extracted_rules.clone(),
            };
            cache.markdown_files.insert(canonical, updated.clone());
            stats.hash_hits += 1;
            return Ok(updated.extracted_rules);
        }
    }

    let relative_display = if let Ok(rel) = canonical.strip_prefix(project_root) {
        rel.display().to_string()
    } else {
        compute_relative_path(project_root, &canonical)
    };

    let doc = render(&content, &RenderOptions::default())
        .await
        .map_err(|e| eyre::eyre!("Failed to process {}: {}", canonical.display(), e))?;

    if !quiet && !doc.reqs.is_empty() {
        eprintln!(
            "   {} {} requirements from {}",
            "Found".green(),
            doc.reqs.len(),
            relative_display
        );
    }

    let mut extracted = Vec::new();
    if !doc.reqs.is_empty() {
        use marq::DocElement;
        let mut rule_sections: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
        let mut current_section: Option<(String, String)> = None;
        for element in &doc.elements {
            match element {
                DocElement::Heading(h) => current_section = Some((h.id.clone(), h.title.clone())),
                DocElement::Req(r) => {
                    if let Some((slug, title)) = &current_section {
                        rule_sections
                            .insert(r.id.to_string(), (Some(slug.clone()), Some(title.clone())));
                    }
                }
                DocElement::Paragraph(_) => {}
            }
        }

        for req in doc.reqs {
            let column = Some(compute_column_for_content(&content, req.span.offset));
            let prefix =
                extract_marker_prefix_from_content(&content, req.marker_span).ok_or_else(|| {
                    eyre::eyre!(
                        "Failed to determine requirement marker prefix in {} at line {}",
                        relative_display,
                        req.line
                    )
                })?;
            let (section, section_title) = rule_sections
                .remove(&req.id.to_string())
                .unwrap_or((None, None));
            extracted.push(crate::ExtractedRule {
                def: req,
                source_file: relative_display.clone(),
                prefix,
                column,
                section,
                section_title,
            });
        }
    }

    cache.markdown_files.insert(
        canonical,
        CachedMarkdownFile {
            content_hash,
            file_len,
            modified_nanos,
            extracted_rules: extracted.clone(),
        },
    );
    stats.misses += 1;
    stats.reparsed += 1;
    Ok(extracted)
}

async fn load_rules_from_includes_cached(
    project_root: &Path,
    include_patterns: &[String],
    overlay: &FileOverlay,
    cache: &mut BuildCache,
    quiet: bool,
    changed_files: &[PathBuf],
    stats: &mut CacheStats,
) -> Result<(Vec<crate::ExtractedRule>, bool)> {
    let (mut spec_paths, _warnings, did_full_walk) =
        get_cached_spec_scan_paths(project_root, include_patterns, changed_files, cache);
    let (spec_roots, _) = build_scan_roots(project_root, include_patterns);
    for overlay_path in overlay.keys() {
        if overlay_path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        if path_matches_any_root(overlay_path, &spec_roots) {
            spec_paths.insert(overlay_path.clone());
        }
    }

    let mut all_rules = Vec::new();
    let mut seen_ids: BTreeSet<String> = BTreeSet::new();
    for path in spec_paths {
        let extracted =
            extract_markdown_rules_cached(project_root, &path, overlay, cache, quiet, stats)
                .await?;
        for rule in extracted {
            let id = rule.def.id.to_string();
            if seen_ids.contains(&id) {
                eyre::bail!(
                    "Duplicate requirement '{}' found in {}",
                    rule.def.id.red(),
                    rule.source_file
                );
            }
            seen_ids.insert(id);
            all_rules.push(rule);
        }
    }
    Ok((all_rules, did_full_walk))
}

async fn scan_impl_files(
    project_root: &Path,
    include: &[String],
    exclude: &[String],
    overlay: &FileOverlay,
    cache: &mut BuildCache,
    changed_files: &[PathBuf],
    stats: &mut CacheStats,
) -> (
    Vec<ReqReference>,
    Vec<ParseWarning>,
    Vec<String>,
    BTreeMap<PathBuf, Vec<CodeUnit>>,
    BTreeMap<PathBuf, String>,
    BTreeMap<PathBuf, Reqs>,
    bool,
) {
    let (mut files, warnings, did_full_walk) =
        get_cached_impl_scan_paths(project_root, include, exclude, changed_files, cache);
    let (impl_roots, _) = build_scan_roots(project_root, include);
    for overlay_path in overlay.keys() {
        if overlay_path
            .extension()
            .is_none_or(|ext| !is_supported_extension(ext))
        {
            continue;
        }
        if path_matches_any_root(overlay_path, &impl_roots)
            && !path_matches_excludes(overlay_path, &impl_roots, exclude)
        {
            files.insert(overlay_path.clone());
        }
    }
    let mut refs = Vec::new();
    let mut parse_warnings = Vec::new();
    let mut code_units_by_file: BTreeMap<PathBuf, Vec<CodeUnit>> = BTreeMap::new();
    let mut file_contents: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut reqs_by_file: BTreeMap<PathBuf, Reqs> = BTreeMap::new();
    for path in files {
        if let Ok(parsed) = get_cached_source_file(&path, overlay, cache, stats).await {
            reqs_by_file.insert(
                path.clone(),
                Reqs {
                    references: parsed.refs.clone(),
                    warnings: parsed.parse_warnings.clone(),
                },
            );
            refs.extend(parsed.refs);
            parse_warnings.extend(parsed.parse_warnings);
            if !parsed.code_units.is_empty() {
                code_units_by_file.insert(path.clone(), parsed.code_units);
            }
            file_contents.insert(path, parsed.content);
        }
    }
    (
        refs,
        parse_warnings,
        warnings,
        code_units_by_file,
        file_contents,
        reqs_by_file,
        did_full_walk,
    )
}

struct ImplComputedOutput {
    impl_name: String,
    api_rules: Vec<ApiRule>,
    all_search_rules: Vec<search::RuleEntry>,
    impl_code_units: BTreeMap<PathBuf, Vec<CodeUnit>>,
    reverse_data: ApiReverseData,
    refs_len: usize,
    code_files: usize,
    total_units: usize,
    covered_units: usize,
    forward_elapsed_ms: u128,
    reverse_elapsed_ms: u128,
    elapsed_ms: u128,
}

const STALE_IMPLEMENTATION_MUST_CHANGE_PREFIX: &str = "Implementation must be changed to match updated rule text — and ONLY ONCE THAT'S DONE must the code annotation be bumped";

#[derive(Debug, Clone, PartialEq, Eq)]
enum KnownRuleMatch {
    Exact,
    Stale(RuleId),
    Missing,
}

fn classify_reference_against_known_rules(
    reference_id: &RuleId,
    known_rule_ids: &[RuleId],
) -> KnownRuleMatch {
    let mut stale_target: Option<RuleId> = None;

    for rule_id in known_rule_ids {
        match classify_reference_for_rule(rule_id, reference_id) {
            RuleIdMatch::Exact => return KnownRuleMatch::Exact,
            RuleIdMatch::Stale => {
                stale_target = Some(rule_id.clone());
            }
            RuleIdMatch::NoMatch => {}
        }
    }

    if let Some(rule_id) = stale_target {
        KnownRuleMatch::Stale(rule_id)
    } else {
        KnownRuleMatch::Missing
    }
}

fn stale_diagnostic_message_short(
    reference_rule_id: &RuleId,
    current_rule: Option<&ApiRule>,
) -> String {
    let mut message = String::from(STALE_IMPLEMENTATION_MUST_CHANGE_PREFIX);
    if let Some(current_rule) = current_rule {
        message.push_str(&format!(
            ". Reference '{}' is stale; current rule is '{}'.",
            reference_rule_id, current_rule.id
        ));
    } else {
        message.push_str(". The referenced annotation is stale, but the latest matching rule could not be loaded.");
    }
    message
}

fn unknown_rule_message_with_context(
    prefix: &str,
    verb: &RefVerb,
    reference_id: &RuleId,
    known_rule_ids: &[RuleId],
) -> String {
    let suggestions = suggest_similar_rule_ids(reference_id, known_rule_ids, 3);
    let reference = format!("{}[{} {}]", prefix, verb, reference_id);
    if suggestions.is_empty() {
        format!("Unknown rule reference {}", reference)
    } else {
        format!(
            "Unknown rule reference {} (did you mean: {})",
            reference,
            suggestions
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn is_valid_rule_id(id: &RuleId) -> bool {
    let base_id = &id.base;
    for segment in base_id.split('.') {
        if segment.is_empty() {
            return false;
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return false;
        }
        if !segment
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase())
        {
            return false;
        }
    }
    true
}

fn detect_circular_dependencies(forward_data: &ApiSpecForward) -> Vec<Vec<RuleId>> {
    use std::collections::{HashMap, HashSet};

    let mut graph: HashMap<RuleId, Vec<RuleId>> = HashMap::new();
    for rule in &forward_data.rules {
        graph.entry(rule.id.clone()).or_default();
    }

    let mut cycles = Vec::new();
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut path = Vec::new();

    fn dfs(
        node: &RuleId,
        graph: &HashMap<RuleId, Vec<RuleId>>,
        visited: &mut HashSet<RuleId>,
        rec_stack: &mut HashSet<RuleId>,
        path: &mut Vec<RuleId>,
        cycles: &mut Vec<Vec<RuleId>>,
    ) {
        visited.insert(node.clone());
        rec_stack.insert(node.clone());
        path.push(node.clone());

        if let Some(neighbors) = graph.get(node) {
            for neighbor in neighbors {
                if !visited.contains(neighbor) {
                    dfs(neighbor, graph, visited, rec_stack, path, cycles);
                } else if rec_stack.contains(neighbor) {
                    let cycle_start = path.iter().position(|n| n == neighbor).unwrap_or(0);
                    let mut cycle: Vec<RuleId> = path[cycle_start..].to_vec();
                    cycle.push(neighbor.clone());
                    cycles.push(cycle);
                }
            }
        }

        path.pop();
        rec_stack.remove(node);
    }

    for node in graph.keys().cloned().collect::<Vec<_>>() {
        if !visited.contains(&node) {
            dfs(
                &node,
                &graph,
                &mut visited,
                &mut rec_stack,
                &mut path,
                &mut cycles,
            );
        }
    }

    cycles
}

fn span_to_range(content: &str, offset: usize, length: usize) -> (u32, u32, u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;
    let mut start_line = 0u32;
    let mut start_col = 0u32;
    let mut found_start = false;

    for (i, c) in content.char_indices() {
        if i == offset {
            start_line = line;
            start_col = col;
            found_start = true;
        }
        if i == offset + length {
            return (start_line, start_col, line, col);
        }
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    if !found_start {
        (line, col, line, col)
    } else {
        (start_line, start_col, line, col)
    }
}

fn compute_validation_by_impl(
    abs_root: &Path,
    config: &ApiConfig,
    forward_by_impl: &BTreeMap<ImplKey, ApiSpecForward>,
    reverse_by_impl: &BTreeMap<ImplKey, ApiReverseData>,
    source_reqs_by_file: &BTreeMap<PathBuf, Reqs>,
    test_files: &std::collections::HashSet<PathBuf>,
) -> BTreeMap<ImplKey, ValidationResult> {
    let mut out = BTreeMap::new();
    let known_prefixes: std::collections::HashSet<&str> =
        config.specs.iter().map(|s| s.prefix.as_str()).collect();

    for (impl_key, forward_data) in forward_by_impl {
        let (spec, impl_name) = impl_key;
        let mut errors = Vec::new();

        let known_rule_ids: Vec<RuleId> = forward_data.rules.iter().map(|r| r.id.clone()).collect();
        let rules_by_id: HashMap<RuleId, &ApiRule> = forward_data
            .rules
            .iter()
            .map(|rule| (rule.id.clone(), rule))
            .collect();

        let mut seen_ids: HashMap<RuleId, (&Option<String>, Option<usize>)> = HashMap::new();
        let mut seen_bases: HashMap<String, (&RuleId, &Option<String>, Option<usize>)> =
            HashMap::new();

        for rule in &forward_data.rules {
            if let Some((prev_file, prev_line)) = seen_ids.get(&rule.id) {
                errors.push(ValidationError {
                    code: ValidationErrorCode::DuplicateRequirement,
                    message: format!(
                        "Duplicate rule ID '{}' (first defined at {}:{})",
                        rule.id,
                        prev_file.as_deref().unwrap_or("?"),
                        prev_line.unwrap_or(0)
                    ),
                    file: rule.source_file.clone(),
                    line: rule.source_line,
                    column: rule.source_column,
                    related_rules: vec![rule.id.clone()],
                    reference_rule_id: None,
                });
            } else {
                seen_ids.insert(rule.id.clone(), (&rule.source_file, rule.source_line));
            }

            if let Some((prev_rule_id, prev_file, prev_line)) =
                seen_bases.get(rule.id.base.as_str())
            {
                errors.push(ValidationError {
                    code: ValidationErrorCode::DuplicateRequirement,
                    message: format!(
                        "Duplicate rule base '{}' across versions ('{}' and '{}') in same spec (first defined at {}:{})",
                        rule.id.base,
                        prev_rule_id,
                        rule.id,
                        prev_file.as_deref().unwrap_or("?"),
                        prev_line.unwrap_or(0)
                    ),
                    file: rule.source_file.clone(),
                    line: rule.source_line,
                    column: rule.source_column,
                    related_rules: vec![(*prev_rule_id).clone(), rule.id.clone()],
                    reference_rule_id: None,
                });
            } else {
                seen_bases.insert(
                    rule.id.base.clone(),
                    (&rule.id, &rule.source_file, rule.source_line),
                );
            }
        }

        for rule in &forward_data.rules {
            if !is_valid_rule_id(&rule.id) {
                errors.push(ValidationError {
                    code: ValidationErrorCode::InvalidNaming,
                    message: format!(
                        "Rule ID '{}' doesn't follow naming convention (use dot-separated lowercase segments)",
                        rule.id
                    ),
                    file: rule.source_file.clone(),
                    line: rule.source_line,
                    column: rule.source_column,
                    related_rules: vec![],
                    reference_rule_id: None,
                });
            }

            for impl_ref in &rule.impl_refs {
                let ref_path = abs_root.join(&impl_ref.file);
                if test_files.contains(&ref_path) {
                    errors.push(ValidationError {
                        code: ValidationErrorCode::ImplInTestFile,
                        message: format!(
                            "Test file contains impl annotation for '{}' - test files may only contain verify annotations",
                            rule.id
                        ),
                        file: Some(impl_ref.file.clone()),
                        line: Some(impl_ref.line),
                        column: None,
                        related_rules: vec![rule.id.clone()],
                        reference_rule_id: None,
                    });
                }
            }
        }

        if let Some(reverse_data) = reverse_by_impl.get(impl_key) {
            let current_spec_prefix: Option<&str> = config
                .specs
                .iter()
                .find(|s| s.name == *spec)
                .map(|s| s.prefix.as_str());

            let known_rule_ids_for_prefix: Vec<RuleId> = if let Some(prefix) = current_spec_prefix {
                let spec_names: std::collections::HashSet<&str> = config
                    .specs
                    .iter()
                    .filter(|s| s.prefix == prefix)
                    .map(|s| s.name.as_str())
                    .collect();

                forward_by_impl
                    .iter()
                    .filter(|((spec_name, _), _)| spec_names.contains(spec_name.as_str()))
                    .flat_map(|(_, forward)| forward.rules.iter().map(|r| r.id.clone()))
                    .collect()
            } else {
                Vec::new()
            };

            let mut available_prefixes: Vec<_> = known_prefixes.iter().copied().collect();
            available_prefixes.sort_unstable();
            let available_prefixes_joined = available_prefixes.join(", ");

            for file_entry in &reverse_data.files {
                let file_path = abs_root.join(&file_entry.path);
                let canonical = file_path
                    .canonicalize()
                    .unwrap_or_else(|_| file_path.clone());
                let Some(reqs) = source_reqs_by_file
                    .get(&canonical)
                    .or_else(|| source_reqs_by_file.get(&file_path))
                else {
                    continue;
                };

                for reference in &reqs.references {
                    if !known_prefixes.contains(reference.prefix.as_str()) {
                        errors.push(ValidationError {
                            code: ValidationErrorCode::UnknownPrefix,
                            message: format!(
                                "Unknown prefix '{}' - available prefixes: {}",
                                reference.prefix, available_prefixes_joined
                            ),
                            file: Some(file_entry.path.clone()),
                            line: Some(reference.line),
                            column: None,
                            related_rules: vec![],
                            reference_rule_id: None,
                        });
                    } else if current_spec_prefix == Some(reference.prefix.as_str()) {
                        match classify_reference_against_known_rules(
                            &reference.req_id,
                            &known_rule_ids,
                        ) {
                            KnownRuleMatch::Exact => {}
                            KnownRuleMatch::Stale(current_rule_id) => {
                                let message = stale_diagnostic_message_short(
                                    &reference.req_id,
                                    rules_by_id.get(&current_rule_id).copied(),
                                );
                                errors.push(ValidationError {
                                    code: ValidationErrorCode::StaleRequirement,
                                    message,
                                    file: Some(file_entry.path.clone()),
                                    line: Some(reference.line),
                                    column: None,
                                    related_rules: vec![current_rule_id],
                                    reference_rule_id: Some(reference.req_id.clone()),
                                });
                            }
                            KnownRuleMatch::Missing => {
                                match classify_reference_against_known_rules(
                                    &reference.req_id,
                                    &known_rule_ids_for_prefix,
                                ) {
                                    KnownRuleMatch::Exact | KnownRuleMatch::Stale(_) => {}
                                    KnownRuleMatch::Missing => {
                                        let message = unknown_rule_message_with_context(
                                            &reference.prefix,
                                            &reference.verb,
                                            &reference.req_id,
                                            &known_rule_ids_for_prefix,
                                        );
                                        errors.push(ValidationError {
                                            code: ValidationErrorCode::UnknownRequirement,
                                            message,
                                            file: Some(file_entry.path.clone()),
                                            line: Some(reference.line),
                                            column: None,
                                            related_rules: vec![],
                                            reference_rule_id: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        for cycle in detect_circular_dependencies(forward_data) {
            errors.push(ValidationError {
                code: ValidationErrorCode::CircularDependency,
                message: format!(
                    "Circular dependency detected: {}",
                    cycle
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" -> ")
                ),
                file: None,
                line: None,
                column: None,
                related_rules: cycle,
                reference_rule_id: None,
            });
        }

        let error_count = errors.len();
        out.insert(
            impl_key.clone(),
            ValidationResult {
                spec: spec.clone(),
                impl_name: impl_name.clone(),
                errors,
                warning_count: 0,
                error_count,
            },
        );
    }

    out
}

fn compute_workspace_diagnostics(
    abs_root: &Path,
    config: &ApiConfig,
    forward_by_impl: &BTreeMap<ImplKey, ApiSpecForward>,
    source_reqs_by_file: &BTreeMap<PathBuf, Reqs>,
    file_contents: &BTreeMap<PathBuf, String>,
    test_files: &std::collections::HashSet<PathBuf>,
) -> Vec<LspFileDiagnostics> {
    let known_prefixes: std::collections::HashSet<&str> =
        config.specs.iter().map(|s| s.prefix.as_str()).collect();
    if known_prefixes.is_empty() {
        return Vec::new();
    }

    let mut known_rules_by_prefix: HashMap<&str, Vec<RuleId>> = HashMap::new();
    let mut rules_by_id: HashMap<RuleId, ApiRule> = HashMap::new();
    for spec_cfg in &config.specs {
        let rule_ids = known_rules_by_prefix
            .entry(spec_cfg.prefix.as_str())
            .or_default();
        for ((spec_name, _), forward_data) in forward_by_impl {
            if spec_name == &spec_cfg.name {
                for rule in &forward_data.rules {
                    rule_ids.push(rule.id.clone());
                    rules_by_id
                        .entry(rule.id.clone())
                        .or_insert_with(|| rule.clone());
                }
            }
        }
    }

    let mut out = Vec::new();
    for (path, reqs) in source_reqs_by_file {
        let Some(content) = file_contents.get(path) else {
            continue;
        };
        let is_test = test_files.contains(path);
        let mut diagnostics = Vec::new();

        for reference in &reqs.references {
            let (start_line, start_char, end_line, end_char) =
                span_to_range(content, reference.span.offset, reference.span.length);

            if !known_prefixes.contains(reference.prefix.as_str()) {
                diagnostics.push(LspDiagnostic {
                    severity: "error".to_string(),
                    code: "unknown-prefix".to_string(),
                    message: format!("Unknown prefix: '{}'", reference.prefix),
                    start_line,
                    start_char,
                    end_line,
                    end_char,
                });
                continue;
            }

            let known_for_prefix = known_rules_by_prefix
                .get(reference.prefix.as_str())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            match classify_reference_against_known_rules(&reference.req_id, known_for_prefix) {
                KnownRuleMatch::Exact => {}
                KnownRuleMatch::Stale(current_rule_id) => {
                    let message = stale_diagnostic_message_short(
                        &reference.req_id,
                        rules_by_id.get(&current_rule_id),
                    );
                    diagnostics.push(LspDiagnostic {
                        severity: "warning".to_string(),
                        code: "stale".to_string(),
                        message,
                        start_line,
                        start_char,
                        end_line,
                        end_char,
                    });
                }
                KnownRuleMatch::Missing => {
                    let message = unknown_rule_message_with_context(
                        &reference.prefix,
                        &reference.verb,
                        &reference.req_id,
                        known_for_prefix,
                    );
                    diagnostics.push(LspDiagnostic {
                        severity: "warning".to_string(),
                        code: "orphaned".to_string(),
                        message,
                        start_line,
                        start_char,
                        end_line,
                        end_char,
                    });
                }
            }

            if is_test && reference.verb == RefVerb::Impl {
                diagnostics.push(LspDiagnostic {
                    severity: "warning".to_string(),
                    code: "impl-in-test".to_string(),
                    message: "Implementation reference in test file (use 'verify' instead)"
                        .to_string(),
                    start_line,
                    start_char,
                    end_line,
                    end_char,
                });
            }
        }

        for warning in &reqs.warnings {
            let (start_line, start_char, end_line, end_char) =
                span_to_range(content, warning.span.offset, warning.span.length);
            let message = match &warning.kind {
                tracey_core::WarningKind::UnknownVerb(verb) => {
                    format!("Unknown verb: '{}'", verb)
                }
                tracey_core::WarningKind::MalformedReference => "Malformed reference".to_string(),
            };

            diagnostics.push(LspDiagnostic {
                severity: "warning".to_string(),
                code: "parse-warning".to_string(),
                message,
                start_line,
                start_char,
                end_line,
                end_char,
            });
        }

        if diagnostics.is_empty() {
            continue;
        }

        let rel_path = path
            .strip_prefix(abs_root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| compute_relative_path(abs_root, path));
        out.push(LspFileDiagnostics {
            path: rel_path,
            diagnostics,
        });
    }

    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn compute_impl_output(
    abs_root: &Path,
    _spec_name: &str,
    impl_name: String,
    inferred_prefix: &str,
    extracted_rules: &[crate::ExtractedRule],
    refs: Vec<ReqReference>,
    impl_code_units: BTreeMap<PathBuf, Vec<CodeUnit>>,
) -> ImplComputedOutput {
    let impl_start = Instant::now();
    let forward_start = Instant::now();
    struct IndexedRef {
        verb: RefVerb,
        req_id: RuleId,
        code_ref: ApiCodeRef,
        relative_file: String,
        line: usize,
    }
    let mut indexed_refs: Vec<IndexedRef> = Vec::new();
    let mut refs_by_base: HashMap<String, Vec<usize>> = HashMap::new();
    for r in &refs {
        if r.prefix != inferred_prefix {
            continue;
        }
        let canonical_ref = r.file.canonicalize().unwrap_or_else(|_| r.file.clone());
        let relative_display = if let Ok(rel) = canonical_ref.strip_prefix(abs_root) {
            rel.display().to_string()
        } else {
            compute_relative_path(abs_root, &canonical_ref)
        };
        let idx = indexed_refs.len();
        indexed_refs.push(IndexedRef {
            verb: r.verb,
            req_id: r.req_id.clone(),
            code_ref: ApiCodeRef {
                file: relative_display.clone(),
                line: r.line,
            },
            relative_file: relative_display,
            line: r.line,
        });
        refs_by_base
            .entry(r.req_id.base.clone())
            .or_default()
            .push(idx);
    }

    let mut api_rules = Vec::new();
    for extracted in extracted_rules {
        let Some(rule_id) = parse_rule_id(&extracted.def.id.to_string()) else {
            continue;
        };
        let mut impl_refs = Vec::new();
        let mut verify_refs = Vec::new();
        let mut depends_refs = Vec::new();
        let mut stale_refs = Vec::new();

        let candidate_idxs = refs_by_base
            .get(&rule_id.base)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        for idx in candidate_idxs {
            let entry = &indexed_refs[*idx];
            match classify_reference_for_rule(&rule_id, &entry.req_id) {
                RuleIdMatch::Exact => match entry.verb {
                    RefVerb::Impl | RefVerb::Define => impl_refs.push(entry.code_ref.clone()),
                    RefVerb::Verify => verify_refs.push(entry.code_ref.clone()),
                    RefVerb::Depends | RefVerb::Related => {
                        depends_refs.push(entry.code_ref.clone())
                    }
                },
                RuleIdMatch::Stale => match entry.verb {
                    RefVerb::Impl | RefVerb::Define => {
                        impl_refs.push(entry.code_ref.clone());
                        stale_refs.push(ApiStaleRef {
                            file: entry.relative_file.clone(),
                            line: entry.line,
                            reference_id: entry.req_id.clone(),
                        });
                    }
                    RefVerb::Verify => {
                        verify_refs.push(entry.code_ref.clone());
                        stale_refs.push(ApiStaleRef {
                            file: entry.relative_file.clone(),
                            line: entry.line,
                            reference_id: entry.req_id.clone(),
                        });
                    }
                    RefVerb::Depends | RefVerb::Related => {}
                },
                RuleIdMatch::NoMatch => {}
            }
        }

        api_rules.push(ApiRule {
            id: rule_id,
            raw: extracted.def.raw.clone(),
            html: extracted.def.html.clone(),
            status: extracted
                .def
                .metadata
                .status
                .map(|s| s.as_str().to_string()),
            level: extracted.def.metadata.level.map(|l| l.as_str().to_string()),
            source_file: Some(extracted.source_file.clone()),
            source_line: Some(extracted.def.line),
            source_column: extracted.column,
            section: extracted.section.clone(),
            section_title: extracted.section_title.clone(),
            impl_refs,
            verify_refs,
            depends_refs,
            is_stale: !stale_refs.is_empty(),
            stale_refs,
        });
    }
    api_rules.sort_by(|a, b| a.id.cmp(&b.id));
    let all_search_rules = api_rules
        .iter()
        .map(|r| search::RuleEntry {
            id: r.id.to_string(),
            raw: r.raw.clone(),
        })
        .collect::<Vec<_>>();
    let forward_elapsed_ms = forward_start.elapsed().as_millis();

    let reverse_start = Instant::now();
    let mut total_units = 0;
    let mut covered_units = 0;
    let mut file_entries = Vec::new();
    for (path, units) in &impl_code_units {
        let relative_display = if let Ok(rel) = path.strip_prefix(abs_root) {
            rel.display().to_string()
        } else {
            compute_relative_path(abs_root, path)
        };
        let file_total = units.len();
        let file_covered = units.iter().filter(|u| !u.req_refs.is_empty()).count();
        total_units += file_total;
        covered_units += file_covered;
        file_entries.push(ApiFileEntry {
            path: relative_display,
            total_units: file_total,
            covered_units: file_covered,
        });
    }
    file_entries.sort_by(|a, b| a.path.cmp(&b.path));
    let reverse_elapsed_ms = reverse_start.elapsed().as_millis();

    ImplComputedOutput {
        impl_name,
        api_rules,
        all_search_rules,
        code_files: impl_code_units.len(),
        impl_code_units,
        reverse_data: ApiReverseData {
            total_units,
            covered_units,
            files: file_entries,
        },
        refs_len: refs.len(),
        total_units,
        covered_units,
        forward_elapsed_ms,
        reverse_elapsed_ms,
        elapsed_ms: impl_start.elapsed().as_millis(),
    }
}

pub async fn build_dashboard_data(
    project_root: &Path,
    config: &Config,
    version: u64,
    quiet: bool,
) -> Result<DashboardData> {
    let mut cache = BuildCache::default();
    build_dashboard_data_with_overlay_and_cache(
        project_root,
        config,
        version,
        quiet,
        &FileOverlay::new(),
        &mut cache,
        &[],
    )
    .await
}

pub async fn build_dashboard_data_with_overlay(
    project_root: &Path,
    config: &Config,
    version: u64,
    quiet: bool,
    overlay: &FileOverlay,
) -> Result<DashboardData> {
    let mut cache = BuildCache::default();
    build_dashboard_data_with_overlay_and_cache(
        project_root,
        config,
        version,
        quiet,
        overlay,
        &mut cache,
        &[],
    )
    .await
}

pub async fn build_dashboard_data_with_overlay_and_cache(
    project_root: &Path,
    config: &Config,
    version: u64,
    quiet: bool,
    overlay: &FileOverlay,
    cache: &mut BuildCache,
    changed_files: &[PathBuf],
) -> Result<DashboardData> {
    let build_start = Instant::now();
    let abs_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let mut cache_stats = CacheStats::default();

    let mut api_config = ApiConfig {
        project_root: abs_root.display().to_string(),
        specs: Vec::new(),
    };

    let mut forward_by_impl: BTreeMap<ImplKey, ApiSpecForward> = BTreeMap::new();
    let mut reverse_by_impl: BTreeMap<ImplKey, ApiReverseData> = BTreeMap::new();
    let mut code_units_by_impl: BTreeMap<ImplKey, BTreeMap<PathBuf, Vec<CodeUnit>>> =
        BTreeMap::new();
    let specs_content_by_impl: BTreeMap<ImplKey, ApiSpecData> = BTreeMap::new();
    let mut spec_includes_by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut all_file_contents: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut all_source_reqs_by_file: BTreeMap<PathBuf, Reqs> = BTreeMap::new();
    let mut all_search_rules: Vec<search::RuleEntry> = Vec::new();
    let mut total_extracted_rules = 0usize;
    let mut total_source_refs = 0usize;
    let mut total_code_files = 0usize;
    let mut total_code_units = 0usize;
    let total_impls: usize = config.specs.iter().map(|s| s.impls.len()).sum();

    info!(
        "dashboard build start version={} specs={} impls={} overlay_files={}",
        version,
        config.specs.len(),
        total_impls,
        overlay.len()
    );

    // r[impl config.impl.test_include]
    // Collect all test file patterns and find matching files
    let test_files_start = Instant::now();
    let mut test_files: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    for spec_config in &config.specs {
        for impl_config in &spec_config.impls {
            let test_patterns: Vec<&str> = impl_config
                .test_include
                .iter()
                .map(|t| t.as_str())
                .collect();
            if !test_patterns.is_empty() {
                // Walk files and match against test patterns
                let walker = ignore::WalkBuilder::new(project_root)
                    .follow_links(true)
                    .hidden(false)
                    .git_ignore(true)
                    .build();
                for entry in walker.flatten() {
                    let Some(ft) = entry.file_type() else {
                        continue;
                    };
                    if ft.is_file() {
                        let path = entry.path();
                        if let Ok(relative) = path.strip_prefix(project_root) {
                            let relative_str = relative.to_string_lossy();
                            for pattern in &test_patterns {
                                if glob_match(&relative_str, pattern) {
                                    test_files.insert(path.to_path_buf());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    info!(
        "dashboard build test file scan done test_files={} elapsed_ms={}",
        test_files.len(),
        test_files_start.elapsed().as_millis()
    );

    for spec_config in &config.specs {
        let spec_start = Instant::now();
        let spec_name = &spec_config.name;
        let include_patterns: Vec<String> = spec_config.include.to_vec();

        if let Some(prefix) = &spec_config.prefix {
            // r[impl config.spec.prefix+2]
            return Err(eyre::eyre!(
                "Spec '{}' uses deprecated `prefix {}` in config.\n\n\
                 Remove the `prefix` field from this spec config. Tracey now infers prefixes \
                 directly from requirement markers in spec files (for example `r[...]`).",
                spec_name,
                prefix
            ));
        }

        // Validate that spec has at least one implementation
        if spec_config.impls.is_empty() {
            return Err(eyre::eyre!(
                "Spec '{}' has no implementations defined.\n\n\
                Add at least one impl block to your config:\n\n\
                spec {{\n    \
                    name \"{}\"\n    \
                    include \"docs/spec/**/*.md\"\n\n    \
                    impl {{\n        \
                        name \"main\"\n        \
                        include \"src/**/*.rs\"\n    \
                    }}\n\
                }}",
                spec_name,
                spec_name
            ));
        }

        // Extract requirements directly from markdown files (shared across impls)
        if !quiet {
            eprintln!(
                "   {} requirements from {:?}",
                "Extracting".green(),
                include_patterns
            );
        }
        let (extracted_rules, spec_walk_full_scan) = load_rules_from_includes_cached(
            project_root,
            &include_patterns,
            overlay,
            cache,
            quiet,
            changed_files,
            &mut cache_stats,
        )
        .await?;
        total_extracted_rules += extracted_rules.len();

        let unique_prefixes: BTreeSet<String> =
            extracted_rules.iter().map(|r| r.prefix.clone()).collect();
        let inferred_prefix = match unique_prefixes.len() {
            0 => {
                return Err(eyre::eyre!(
                    "Spec '{}' has no requirement definitions, so tracey cannot infer its marker prefix.",
                    spec_name
                ));
            }
            1 => unique_prefixes.into_iter().next().unwrap(),
            _ => {
                let prefixes = unique_prefixes.into_iter().collect::<Vec<_>>().join(", ");
                return Err(eyre::eyre!(
                    "Spec '{}' uses multiple requirement marker prefixes ({}). \
                     Use a single prefix per spec.",
                    spec_name,
                    prefixes
                ));
            }
        };
        info!(
            "dashboard build spec extracted spec={} rules={} inferred_prefix={} includes={} walk_full_scan={} elapsed_ms={}",
            spec_name,
            extracted_rules.len(),
            inferred_prefix,
            include_patterns.len(),
            spec_walk_full_scan,
            spec_start.elapsed().as_millis()
        );

        api_config.specs.push(ApiSpecInfo {
            name: spec_name.clone(),
            prefix: inferred_prefix.clone(),
            source: Some(include_patterns.join(", ")),
            source_url: spec_config.source_url.clone(),
            implementations: spec_config.impls.iter().map(|i| i.name.clone()).collect(),
        });
        spec_includes_by_name.insert(spec_name.clone(), include_patterns.clone());

        // Build data for each implementation
        struct ImplComputeTaskMeta {
            impl_key: ImplKey,
            impl_name: String,
            warning_count: usize,
            scan_elapsed_ms: u128,
            impl_walk_full_scan: bool,
        }
        let mut impl_compute_tasks = Vec::new();
        let mut impl_compute_meta = Vec::new();

        for impl_config in &spec_config.impls {
            let scan_start = Instant::now();
            let impl_name = impl_config.name.clone();
            if !quiet {
                eprintln!("   {} {} implementation", "Scanning".green(), impl_name);
            }
            let include: Vec<String> = if impl_config.include.is_empty() {
                vec!["**/*.rs".to_string()]
            } else {
                impl_config.include.to_vec()
            };
            let exclude: Vec<String> = impl_config.exclude.to_vec();
            let impl_key: ImplKey = (spec_name.clone(), impl_name.clone());
            let (
                refs,
                parse_warnings,
                scan_warnings,
                impl_code_units,
                impl_file_contents,
                impl_source_reqs_by_file,
                impl_walk_full_scan,
            ) = scan_impl_files(
                project_root,
                &include,
                &exclude,
                overlay,
                cache,
                changed_files,
                &mut cache_stats,
            )
            .await;
            let warning_count = scan_warnings.len();
            let scan_elapsed_ms = scan_start.elapsed().as_millis();

            for warning in &scan_warnings {
                if !quiet {
                    eprintln!("{}", warning.yellow());
                }
            }
            total_source_refs += refs.len();
            for (path, content) in impl_file_contents {
                all_file_contents.insert(path, content);
            }
            for (path, reqs) in impl_source_reqs_by_file {
                all_source_reqs_by_file.entry(path).or_insert(reqs);
            }
            if !parse_warnings.is_empty() {
                info!(
                    "dashboard build impl parse warnings spec={} impl={} count={}",
                    spec_name,
                    impl_name,
                    parse_warnings.len()
                );
            }

            let abs_root_cloned = abs_root.clone();
            let spec_name_cloned = spec_name.clone();
            let inferred_prefix_cloned = inferred_prefix.clone();
            let extracted_rules_cloned = extracted_rules.clone();
            let impl_name_cloned = impl_name.clone();
            impl_compute_tasks.push(tokio::task::spawn_blocking(move || {
                compute_impl_output(
                    &abs_root_cloned,
                    &spec_name_cloned,
                    impl_name_cloned,
                    &inferred_prefix_cloned,
                    &extracted_rules_cloned,
                    refs,
                    impl_code_units,
                )
            }));
            impl_compute_meta.push(ImplComputeTaskMeta {
                impl_key,
                impl_name,
                warning_count,
                scan_elapsed_ms,
                impl_walk_full_scan,
            });
        }

        for (task, meta) in impl_compute_tasks
            .into_iter()
            .zip(impl_compute_meta.into_iter())
        {
            let out = task
                .await
                .map_err(|err| eyre::eyre!("Implementation compute task failed: {err}"))?;
            total_code_files += out.code_files;
            total_code_units += out.total_units;
            all_search_rules.extend(out.all_search_rules);

            info!(
                "dashboard build impl processed spec={} impl={} refs={} warnings={} code_files={} code_units={} covered_units={} elapsed_ms={}",
                spec_name,
                out.impl_name,
                out.refs_len,
                meta.warning_count,
                out.code_files,
                out.total_units,
                out.covered_units,
                out.elapsed_ms
            );
            info!(
                "dashboard build impl cache spec={} impl={} walk_full_scan={} files_scanned={}",
                spec_name, out.impl_name, meta.impl_walk_full_scan, out.code_files
            );
            info!(
                "dashboard build impl phases spec={} impl={} scan_ms={} forward_ms={} reverse_ms={} render_ms=0",
                spec_name,
                out.impl_name,
                meta.scan_elapsed_ms,
                out.forward_elapsed_ms,
                out.reverse_elapsed_ms
            );

            forward_by_impl.insert(
                meta.impl_key.clone(),
                ApiSpecForward {
                    name: spec_name.clone(),
                    rules: out.api_rules,
                },
            );
            reverse_by_impl.insert(meta.impl_key.clone(), out.reverse_data);
            code_units_by_impl.insert(meta.impl_key, out.impl_code_units);
        }
        info!(
            "dashboard build spec done spec={} impls={} elapsed_ms={}",
            spec_name,
            spec_config.impls.len(),
            spec_start.elapsed().as_millis()
        );
    }

    // Deduplicate search rules by ID
    all_search_rules.sort_by(|a, b| a.id.cmp(&b.id));
    all_search_rules.dedup_by(|a, b| a.id == b.id);

    // Build search index with all sources and rules
    info!(
        "dashboard build search index deferred files={} rules={}",
        all_file_contents.len(),
        all_search_rules.len()
    );

    // Compute content hash for change detection (hash all forward/reverse data)
    let mut content_hash: u64 = 0;
    for (key, forward) in &forward_by_impl {
        let json = facet_json::to_string(forward).unwrap_or_default();
        content_hash ^= simple_hash(&format!("{:?}:{}", key, json));
    }
    for (key, reverse) in &reverse_by_impl {
        let json = facet_json::to_string(reverse).unwrap_or_default();
        content_hash ^= simple_hash(&format!("{:?}:{}", key, json));
    }

    let validation_by_impl = compute_validation_by_impl(
        &abs_root,
        &api_config,
        &forward_by_impl,
        &reverse_by_impl,
        &all_source_reqs_by_file,
        &test_files,
    );
    let workspace_diagnostics = compute_workspace_diagnostics(
        &abs_root,
        &api_config,
        &forward_by_impl,
        &all_source_reqs_by_file,
        &all_file_contents,
        &test_files,
    );

    let elapsed = build_start.elapsed();
    info!(
        "dashboard build done version={} specs={} impls={} rules={} refs={} code_files={} code_units={} cache_metadata_hits={} cache_hash_hits={} cache_misses={} reparsed_files={} cache_entries={} elapsed_ms={}",
        version,
        api_config.specs.len(),
        forward_by_impl.len(),
        total_extracted_rules,
        total_source_refs,
        total_code_files,
        total_code_units,
        cache_stats.metadata_hits,
        cache_stats.hash_hits,
        cache_stats.misses,
        cache_stats.reparsed,
        cache.source_files.len(),
        elapsed.as_millis()
    );

    Ok(DashboardData {
        config: api_config,
        forward_by_impl,
        reverse_by_impl,
        code_units_by_impl,
        specs_content_by_impl,
        spec_includes_by_name,
        search_files: all_file_contents,
        source_reqs_by_file: all_source_reqs_by_file,
        search_rules: all_search_rules,
        validation_by_impl,
        workspace_diagnostics,
        version,
        content_hash,
        delta: crate::server::Delta::default(),
        test_files,
    })
}

/// Simple FNV-1a hash for change detection
fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

async fn load_spec_content(
    root: &Path,
    patterns: &[&str],
    spec_name: &str,
    impl_name: &str,
    coverage: &BTreeMap<String, RuleCoverage>,
    specs_content: &mut BTreeMap<String, ApiSpecData>,
    overlay: &FileOverlay,
) -> Result<()> {
    use ignore::WalkBuilder;

    // Shared source file tracker for rule handler
    let current_source_file = Arc::new(Mutex::new(String::new()));

    // Get git status for files in the project
    let git_status = get_git_status(root);

    // Set up marq handlers for consistent rendering with coverage-aware rule rendering
    let rule_handler = TraceyRuleHandler::new(
        coverage.clone(),
        Arc::clone(&current_source_file),
        spec_name.to_string(),
        impl_name.to_string(),
        root.to_path_buf(),
        git_status,
    );
    let inline_code_handler =
        TraceyInlineCodeHandler::new(spec_name.to_string(), impl_name.to_string());
    let opts = RenderOptions::new()
        .with_default_handler(ArboriumHandler::new().with_language_header(true))
        .with_handler(&["aasvg"], AasvgHandler::new())
        .with_handler(&["pikchr"], PikruHandler::new())
        .with_handler(&["compare"], CompareHandler::new())
        .with_req_handler(rule_handler)
        .with_inline_code_handler(inline_code_handler);

    // Collect all matching files with their content and weight
    let mut files: Vec<(String, String, i32)> = Vec::new(); // (relative_path, content, weight)

    let walker = WalkBuilder::new(root)
        .follow_links(true)
        .hidden(false)
        .git_ignore(true)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();

        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }

        let relative = path.strip_prefix(root).unwrap_or(path);
        let relative_str = relative.to_string_lossy().to_string();

        // Check if path matches any of the patterns
        let matches_any = patterns.iter().any(|p| glob_match(&relative_str, p));
        if !matches_any {
            continue;
        }

        if let Ok(content) = read_file_with_overlay(path, overlay).await {
            // Parse frontmatter to get weight
            let weight = match parse_frontmatter(&content) {
                Ok((fm, _)) => fm.weight,
                Err(_) => 0, // Default weight if no frontmatter
            };
            files.push((relative_str, content, weight));
        }
    }

    // Sort by weight
    files.sort_by_key(|(_, _, weight)| *weight);

    // Concatenate all markdown files to render as one document
    // This ensures heading IDs are hierarchical across all files
    let mut combined_markdown = String::new();
    let mut first_source_file = String::new();

    for (i, (source_file, content, _weight)) in files.iter().enumerate() {
        if i == 0 {
            first_source_file = source_file.clone();
        }
        combined_markdown.push_str(content);
        combined_markdown.push_str("\n\n"); // Ensure separation between files
    }

    // Render the combined document once (so heading_stack works across files)
    // Set source_path so paragraphs get data-source-file attributes for click-to-edit
    // Must use absolute path for editor navigation to work correctly
    *current_source_file.lock().unwrap() = first_source_file.clone();
    let absolute_source_path = root.join(&first_source_file).display().to_string();
    let opts = opts.with_source_path(&absolute_source_path);
    let doc = render(&combined_markdown, &opts).await?;

    // Create a single section with all content
    // (Frontend concatenates sections anyway, this just simplifies tracking)
    let mut sections = Vec::new();
    if !files.is_empty() {
        sections.push(SpecSection {
            source_file: first_source_file,
            html: doc.html,
            weight: files[0].2,
        });
    }

    let all_elements = doc.elements;

    // Build outline from elements
    let outline = build_outline(&all_elements, coverage);

    if !sections.is_empty() {
        specs_content.insert(
            spec_name.to_string(),
            ApiSpecData {
                name: spec_name.to_string(),
                sections,
                outline,
            },
        );
    }

    Ok(())
}

pub async fn render_spec_content_for_impl(
    project_root: &Path,
    include_patterns: &[String],
    spec_name: &str,
    impl_name: &str,
    forward: &ApiSpecForward,
) -> Result<ApiSpecData> {
    let mut coverage: BTreeMap<String, RuleCoverage> = BTreeMap::new();
    for rule in &forward.rules {
        let rule_id_string = rule.id.to_string();
        let has_impl = !rule.impl_refs.is_empty();
        let has_verify = !rule.verify_refs.is_empty();
        let has_stale = rule.is_stale;
        let status = if has_stale {
            "stale"
        } else if has_impl && has_verify {
            "covered"
        } else if has_impl || has_verify {
            "partial"
        } else {
            "uncovered"
        };
        coverage.insert(
            rule_id_string,
            RuleCoverage {
                status,
                impl_refs: rule.impl_refs.clone(),
                verify_refs: rule.verify_refs.clone(),
            },
        );
    }

    let include_pattern_refs: Vec<&str> = include_patterns.iter().map(|s| s.as_str()).collect();
    let mut map = BTreeMap::new();
    load_spec_content(
        project_root,
        &include_pattern_refs,
        spec_name,
        impl_name,
        &coverage,
        &mut map,
        &FileOverlay::new(),
    )
    .await?;
    map.remove(spec_name)
        .ok_or_else(|| eyre::eyre!("Spec content not found for {spec_name}/{impl_name}"))
}

/// Build an outline with coverage info from document elements.
/// Returns a flat list of outline entries with both direct and aggregated coverage.
fn build_outline(
    elements: &[marq::DocElement],
    coverage: &BTreeMap<String, RuleCoverage>,
) -> Vec<OutlineEntry> {
    use marq::DocElement;

    // First pass: collect headings with their direct rule coverage
    let mut entries: Vec<OutlineEntry> = Vec::new();
    let mut current_heading_idx: Option<usize> = None;

    for element in elements {
        match element {
            DocElement::Heading(h) => {
                entries.push(OutlineEntry {
                    title: h.title.clone(),
                    slug: h.id.clone(),
                    level: h.level,
                    coverage: OutlineCoverage::default(),
                    aggregated: OutlineCoverage::default(),
                });
                current_heading_idx = Some(entries.len() - 1);
            }
            DocElement::Req(r) => {
                if let Some(idx) = current_heading_idx {
                    let cov = coverage.get(&r.id.to_string());
                    let has_impl = cov.is_some_and(|c| !c.impl_refs.is_empty());
                    let has_verify = cov.is_some_and(|c| !c.verify_refs.is_empty());

                    entries[idx].coverage.total += 1;
                    if has_impl {
                        entries[idx].coverage.impl_count += 1;
                    }
                    if has_verify {
                        entries[idx].coverage.verify_count += 1;
                    }
                }
            }
            DocElement::Paragraph(_) => {
                // Paragraphs don't contribute to outline coverage
            }
        }
    }

    // Second pass: aggregate coverage up the hierarchy
    // For each heading, its aggregated coverage includes:
    // - Its own direct coverage
    // - All coverage from headings with higher level numbers (deeper nesting) that follow it
    //   until we hit a heading with the same or lower level number

    // Start with direct coverage as the base for aggregated
    for entry in &mut entries {
        entry.aggregated = entry.coverage.clone();
    }

    // Process in reverse order to propagate child coverage up to parents
    for i in (0..entries.len()).rev() {
        let current_level = entries[i].level;

        // Look forward to find all children (headings with higher level until we hit same/lower level)
        let mut j = i + 1;
        while j < entries.len() && entries[j].level > current_level {
            // Only aggregate immediate children (next level down)
            // Children already have their subtree aggregated from the reverse pass
            if entries[j].level == current_level + 1 {
                let child_agg = entries[j].aggregated.clone();
                entries[i].aggregated.total += child_agg.total;
                entries[i].aggregated.impl_count += child_agg.impl_count;
                entries[i].aggregated.verify_count += child_agg.verify_count;
            }
            j += 1;
        }
    }

    entries
}

/// Simple glob pattern matching
///
/// Normalizes path separators to forward slashes for cross-platform compatibility.
pub fn glob_match(path: &str, pattern: &str) -> bool {
    // Normalize Windows backslashes to forward slashes
    let path = path.replace('\\', "/");

    if pattern == "**/*.rs" || pattern == "**/*.md" {
        let ext = pattern.rsplit('.').next().unwrap_or("");
        return path.ends_with(&format!(".{}", ext));
    }

    if let Some(rest) = pattern.strip_suffix("/**/*.rs") {
        return path.starts_with(rest) && path.ends_with(".rs");
    }
    if let Some(rest) = pattern.strip_suffix("/**/*.md") {
        return path.starts_with(rest) && path.ends_with(".md");
    }

    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path.starts_with(prefix);
    }

    if !pattern.contains('*') {
        return path == pattern;
    }

    // Fallback
    true
}
