//! HTTP server for the tracey dashboard
//!
//! Serves a JSON API + static Preact SPA for interactive traceability exploration.
//!
//! ## API Endpoints
//!
//! - `GET /` - Static HTML shell that loads Preact app
//! - `GET /api/config` - Project info, spec names
//! - `GET /api/forward` - Forward traceability (rules → code refs)
//! - `GET /api/reverse` - Reverse traceability (file tree with coverage)
//! - `GET /api/file?path=...` - Source file content + coverage annotations
//! - `GET /api/spec?name=...` - Raw spec markdown content
//! - `GET /api/version` - Version number for live reload polling

// API types are constructed for JSON serialization
#![allow(dead_code)]

use axum::{
    Router,
    body::Body,
    extract::{FromRequestParts, Query, State, WebSocketUpgrade, ws},
    http::{Method, Request, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, patch, post},
};
use eyre::{Result, WrapErr};
use facet::Facet;
use facet_axum::Json;
use futures_util::{SinkExt, StreamExt};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use owo_colors::OwoColorize;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::watch;
use tower_http::cors::{Any, CorsLayer};
use tracey_core::code_units::CodeUnit;
use tracey_core::is_supported_extension;
use tracey_core::{RefVerb, ReqDefinition, Reqs};
use tracing::{debug, error, info, warn};

// Markdown rendering
use marq::{
    AasvgHandler, ArboriumHandler, PikruHandler, RenderOptions, ReqHandler, parse_frontmatter,
    render,
};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use crate::config::Config;
use crate::search::{self, SearchIndex};
use crate::vite::ViteServer;

// ============================================================================
// JSON API Types
// ============================================================================

// Re-export API types from tracey-api crate
pub use tracey_api::{
    ApiCodeRef, ApiCodeUnit, ApiConfig, ApiFileData, ApiFileEntry, ApiForwardData, ApiReverseData,
    ApiRule, ApiSpecData, ApiSpecForward, ApiSpecInfo, GitStatus, OutlineCoverage, OutlineEntry,
    SpecSection,
};

/// Search response
#[derive(Debug, Clone, Facet)]
struct ApiSearchResponse {
    query: String,
    results: Vec<crate::search::SearchResult>,
    available: bool,
}

// ============================================================================
// Server State
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
    /// Full-text search index for source files
    pub search_index: Box<dyn SearchIndex>,
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

/// Shared application state
#[derive(Clone)]
struct AppState {
    data: watch::Receiver<Arc<DashboardData>>,
    project_root: PathBuf,
    dev_mode: bool,
    vite_port: Option<u16>,
    /// Syntax highlighter for source files
    highlighter: Arc<Mutex<arborium::Highlighter>>,
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
    status: &'static str, // "covered", "partial", "uncovered"
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
            let coverage = self.coverage.get(&rule.id);
            let status = coverage.map(|c| c.status).unwrap_or("uncovered");

            // r[impl markdown.html.wbr] - insert <wbr> after dots for better line breaking
            let display_id = rule.id.replace('.', ".<wbr>");

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
                rule.id,
                self.spec_name, self.impl_name, rule.id, rule.id, source_file, rule.line, rule.id, display_id
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

pub async fn build_dashboard_data(
    project_root: &Path,
    config: &Config,
    version: u64,
    quiet: bool,
) -> Result<DashboardData> {
    build_dashboard_data_with_overlay(project_root, config, version, quiet, &FileOverlay::new())
        .await
}

pub async fn build_dashboard_data_with_overlay(
    project_root: &Path,
    config: &Config,
    version: u64,
    quiet: bool,
    overlay: &FileOverlay,
) -> Result<DashboardData> {
    use tracey_core::WalkSources;

    let abs_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());

    let mut api_config = ApiConfig {
        project_root: abs_root.display().to_string(),
        specs: Vec::new(),
    };

    let mut forward_by_impl: BTreeMap<ImplKey, ApiSpecForward> = BTreeMap::new();
    let mut reverse_by_impl: BTreeMap<ImplKey, ApiReverseData> = BTreeMap::new();
    let mut code_units_by_impl: BTreeMap<ImplKey, BTreeMap<PathBuf, Vec<CodeUnit>>> =
        BTreeMap::new();
    let mut specs_content_by_impl: BTreeMap<ImplKey, ApiSpecData> = BTreeMap::new();
    let mut all_file_contents: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut all_search_rules: Vec<search::RuleEntry> = Vec::new();

    // r[impl config.impl.test_include]
    // Collect all test file patterns and find matching files
    let mut test_files: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    for spec_config in &config.specs {
        for impl_config in &spec_config.impls {
            let test_patterns: Vec<&str> = impl_config
                .test_include
                .iter()
                .map(|t| t.pattern.as_str())
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

    for spec_config in &config.specs {
        let spec_name = &spec_config.name.value;
        let include_patterns: Vec<&str> = spec_config
            .include
            .iter()
            .map(|i| i.pattern.as_str())
            .collect();

        // Validate that spec has at least one implementation
        if spec_config.impls.is_empty() {
            return Err(eyre::eyre!(
                "Spec '{}' has no implementations defined.\n\n\
                Add at least one impl block to your config:\n\n\
                spec {{\n    \
                    name \"{}\"\n    \
                    prefix \"{}\"\n    \
                    include \"docs/spec/**/*.md\"\n\n    \
                    impl {{\n        \
                        name \"main\"\n        \
                        include \"src/**/*.rs\"\n    \
                    }}\n\
                }}",
                spec_name,
                spec_name,
                spec_config.prefix.value
            ));
        }

        api_config.specs.push(ApiSpecInfo {
            name: spec_name.clone(),
            prefix: spec_config.prefix.value.clone(),
            source: Some(include_patterns.join(", ")),
            source_url: spec_config.source_url.as_ref().map(|u| u.value.clone()),
            implementations: spec_config
                .impls
                .iter()
                .map(|i| i.name.value.clone())
                .collect(),
        });

        // Extract requirements directly from markdown files (shared across impls)
        if !quiet {
            eprintln!(
                "   {} requirements from {:?}",
                "Extracting".green(),
                include_patterns
            );
        }
        let extracted_rules =
            crate::load_rules_from_globs(project_root, &include_patterns, quiet).await?;

        // Build data for each implementation
        for impl_config in &spec_config.impls {
            let impl_name = &impl_config.name.value;
            let impl_key: ImplKey = (spec_name.clone(), impl_name.clone());

            if !quiet {
                eprintln!("   {} {} implementation", "Scanning".green(), impl_name);
            }

            // Get include/exclude patterns for this impl
            // r[impl walk.default-include] - default to **/*.rs when no include patterns
            let include: Vec<String> = if impl_config.include.is_empty() {
                vec!["**/*.rs".to_string()]
            } else {
                impl_config
                    .include
                    .iter()
                    .map(|inc| inc.pattern.clone())
                    .collect()
            };
            let exclude: Vec<String> = impl_config
                .exclude
                .iter()
                .map(|exc| exc.pattern.clone())
                .collect();

            // r[impl ref.cross-workspace.paths]
            // Extract requirement references from this impl's source files
            let extraction_result = Reqs::extract(
                WalkSources::new(project_root)
                    .include(include.clone())
                    .exclude(exclude.clone()),
            )?;

            // r[impl ref.cross-workspace.cli-warnings]
            // Print warnings for missing cross-workspace paths
            for warning in &extraction_result.warnings {
                if !quiet {
                    eprintln!("{}", warning.yellow());
                }
            }

            let reqs = extraction_result.reqs;

            // Build forward data for this impl
            let mut api_rules = Vec::new();
            for extracted in &extracted_rules {
                let mut impl_refs = Vec::new();
                let mut verify_refs = Vec::new();
                let mut depends_refs = Vec::new();

                for r in &reqs.references {
                    // r[impl ref.prefix.coverage]
                    if r.prefix == spec_config.prefix.value && r.req_id == extracted.def.id {
                        // Canonicalize the reference file path for consistent matching
                        let canonical_ref =
                            r.file.canonicalize().unwrap_or_else(|_| r.file.clone());

                        // Compute relative path, preserving ../ for cross-workspace files
                        let relative_display =
                            if let Ok(rel) = canonical_ref.strip_prefix(&abs_root) {
                                rel.display().to_string()
                            } else {
                                // Cross-workspace file: compute relative path from abs_root
                                compute_relative_path(&abs_root, &canonical_ref)
                            };

                        let code_ref = ApiCodeRef {
                            file: relative_display,
                            line: r.line,
                        };
                        match r.verb {
                            RefVerb::Impl | RefVerb::Define => impl_refs.push(code_ref),
                            RefVerb::Verify => verify_refs.push(code_ref),
                            RefVerb::Depends | RefVerb::Related => depends_refs.push(code_ref),
                        }
                    }
                }

                api_rules.push(ApiRule {
                    id: extracted.def.id.clone(),
                    text: extracted.def.text.clone(),
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
                    impl_refs,
                    verify_refs,
                    depends_refs,
                });
            }

            // Sort rules by ID
            api_rules.sort_by(|a, b| a.id.cmp(&b.id));

            // Collect rules for search index (deduplicated later)
            for r in &api_rules {
                all_search_rules.push(search::RuleEntry {
                    id: r.id.clone(),
                    html: r.html.clone(),
                });
            }

            // Build coverage map for this impl
            let mut coverage: BTreeMap<String, RuleCoverage> = BTreeMap::new();
            for rule in &api_rules {
                let has_impl = !rule.impl_refs.is_empty();
                let has_verify = !rule.verify_refs.is_empty();
                let status = if has_impl && has_verify {
                    "covered"
                } else if has_impl || has_verify {
                    "partial"
                } else {
                    "uncovered"
                };
                coverage.insert(
                    rule.id.clone(),
                    RuleCoverage {
                        status,
                        impl_refs: rule.impl_refs.clone(),
                        verify_refs: rule.verify_refs.clone(),
                    },
                );
            }

            // Load spec content with coverage-aware rendering for this impl
            let mut impl_specs_content: BTreeMap<String, ApiSpecData> = BTreeMap::new();
            load_spec_content(
                project_root,
                &include_patterns,
                spec_name,
                impl_name,
                &coverage,
                &mut impl_specs_content,
                overlay,
            )
            .await?;
            if let Some(spec_data) = impl_specs_content.remove(spec_name) {
                specs_content_by_impl.insert(impl_key.clone(), spec_data);
            }

            forward_by_impl.insert(
                impl_key.clone(),
                ApiSpecForward {
                    name: spec_name.clone(),
                    rules: api_rules,
                },
            );

            // Extract code units for reverse traceability
            let mut impl_code_units: BTreeMap<PathBuf, Vec<CodeUnit>> = BTreeMap::new();

            // Separate include patterns into local and cross-workspace
            let (local_includes, cross_workspace_includes): (Vec<_>, Vec<_>) =
                include.iter().partition(|p| !p.starts_with("../"));

            // Helper to process a file
            // r[impl walk.extensions]
            let mut process_file = async |path: &Path, root: &Path, patterns: &[&String]| {
                if path.extension().is_some_and(is_supported_extension) {
                    let relative = path.strip_prefix(root).unwrap_or(path);
                    let relative_str = relative.to_string_lossy();

                    let included = patterns
                        .iter()
                        .any(|pattern| glob_match(&relative_str, pattern));

                    let excluded = exclude
                        .iter()
                        .any(|pattern| glob_match(&relative_str, pattern));

                    if included
                        && !excluded
                        && let Ok(content) = read_file_with_overlay(path, overlay).await
                    {
                        // Use canonicalized path as key for consistent lookups
                        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

                        let code_units = tracey_core::code_units::extract(path, &content);
                        if !code_units.is_empty() {
                            impl_code_units.insert(canonical.clone(), code_units.units);
                        }
                        // Also collect for search index
                        all_file_contents.insert(canonical, content);
                    }
                }
            };

            // Walk local patterns from project root
            if !local_includes.is_empty() || include.is_empty() {
                let walker = ignore::WalkBuilder::new(project_root)
                    .follow_links(true)
                    .hidden(false)
                    .git_ignore(true)
                    .build();

                for entry in walker.flatten() {
                    process_file(entry.path(), project_root, &local_includes).await;
                }
            }

            // Walk cross-workspace patterns
            for pattern in cross_workspace_includes {
                // Extract base path (e.g., "../marq" from "../marq/**/*.rs")
                let base_path =
                    if let Some(wildcard_pos) = pattern.find("**").or_else(|| pattern.find('*')) {
                        pattern[..wildcard_pos].trim_end_matches('/')
                    } else {
                        pattern.as_str()
                    };

                let resolved_path = project_root.join(base_path);

                // Check if path exists
                if !resolved_path.exists() {
                    eprintln!("Warning: Cross-workspace path not found: {}", base_path);
                    eprintln!("  Pattern: {}", pattern);
                    continue;
                }

                let walker = ignore::WalkBuilder::new(&resolved_path)
                    .follow_links(true)
                    .hidden(false)
                    .git_ignore(true)
                    .build();

                // Adjust pattern to be relative to resolved path
                let adjusted_pattern = if let Some(suffix) = pattern.strip_prefix(base_path) {
                    suffix.trim_start_matches('/').to_string()
                } else {
                    pattern.to_string()
                };

                for entry in walker.flatten() {
                    process_file(entry.path(), &resolved_path, &[&adjusted_pattern]).await;
                }
            }

            // Build reverse data for this impl
            let mut total_units = 0;
            let mut covered_units = 0;
            let mut file_entries = Vec::new();

            for (path, units) in &impl_code_units {
                // Compute relative path, preserving ../ for cross-workspace files
                let relative_display = if let Ok(rel) = path.strip_prefix(&abs_root) {
                    rel.display().to_string()
                } else {
                    compute_relative_path(&abs_root, path)
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

            reverse_by_impl.insert(
                impl_key.clone(),
                ApiReverseData {
                    total_units,
                    covered_units,
                    files: file_entries,
                },
            );

            code_units_by_impl.insert(impl_key, impl_code_units);
        }
    }

    // Deduplicate search rules by ID
    all_search_rules.sort_by(|a, b| a.id.cmp(&b.id));
    all_search_rules.dedup_by(|a, b| a.id == b.id);

    // Build search index with all sources and rules
    let search_index = search::build_index(project_root, &all_file_contents, &all_search_rules);

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

    Ok(DashboardData {
        config: api_config,
        forward_by_impl,
        reverse_by_impl,
        code_units_by_impl,
        specs_content_by_impl,
        search_index,
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

    // Set up marq handlers for consistent rendering with coverage-aware rule rendering
    // TODO: Add real git status checking
    let git_status = HashMap::new();
    let rule_handler = TraceyRuleHandler::new(
        coverage.clone(),
        Arc::clone(&current_source_file),
        spec_name.to_string(),
        impl_name.to_string(),
        root.to_path_buf(),
        git_status,
    );
    let opts = RenderOptions::new()
        .with_default_handler(ArboriumHandler::new())
        .with_handler(&["aasvg"], AasvgHandler::new())
        .with_handler(&["pikchr"], PikruHandler::new())
        .with_req_handler(rule_handler);

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
                    let cov = coverage.get(&r.id);
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
fn glob_match(path: &str, pattern: &str) -> bool {
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

// ============================================================================
// Static Assets (embedded from Vite build)
// ============================================================================

/// HTML shell from Vite build
const HTML_SHELL: &str = include_str!("../dashboard/dist/index.html");

/// JavaScript bundle from Vite build
const JS_BUNDLE: &str = include_str!("../dashboard/dist/assets/index.js");

/// CSS bundle from Vite build
const CSS_BUNDLE: &str = include_str!("../dashboard/dist/assets/index.css");

// ============================================================================
// Route Handlers
// ============================================================================

async fn api_config(State(state): State<AppState>) -> Json<ApiConfig> {
    let data = state.data.borrow().clone();
    Json(data.config.clone())
}

/// Helper to extract spec and impl from query params, with defaults
fn get_impl_key(params: &[(String, String)], config: &ApiConfig) -> Option<ImplKey> {
    let spec = params
        .iter()
        .find(|(k, _)| k == "spec")
        .map(|(_, v)| v.clone())
        .or_else(|| config.specs.first().map(|s| s.name.clone()))?;

    let impl_name = params
        .iter()
        .find(|(k, _)| k == "impl")
        .map(|(_, v)| v.clone())
        .or_else(|| {
            config
                .specs
                .iter()
                .find(|s| s.name == spec)
                .and_then(|s| s.implementations.first().cloned())
        })?;

    Some((spec, impl_name))
}

async fn api_forward(
    State(state): State<AppState>,
    Query(params): Query<Vec<(String, String)>>,
) -> impl IntoResponse {
    let data = state.data.borrow().clone();

    let Some(impl_key) = get_impl_key(&params, &data.config) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"error":"No specs configured","code":"no_specs"}"#,
            ))
            .unwrap()
            .into_response();
    };

    if let Some(forward) = data.forward_by_impl.get(&impl_key) {
        // Wrap in ApiForwardData for backward compatibility
        Json(ApiForwardData {
            specs: vec![forward.clone()],
        })
        .into_response()
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(format!(
                r#"{{"error":"Implementation not found: {} {}"}}"#,
                impl_key.0, impl_key.1
            )))
            .unwrap()
            .into_response()
    }
}

async fn api_reverse(
    State(state): State<AppState>,
    Query(params): Query<Vec<(String, String)>>,
) -> impl IntoResponse {
    let data = state.data.borrow().clone();

    let Some(impl_key) = get_impl_key(&params, &data.config) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"error":"No specs configured","code":"no_specs"}"#,
            ))
            .unwrap()
            .into_response();
    };

    if let Some(reverse) = data.reverse_by_impl.get(&impl_key) {
        Json(reverse.clone()).into_response()
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(format!(
                r#"{{"error":"Implementation not found: {} {}"}}"#,
                impl_key.0, impl_key.1
            )))
            .unwrap()
            .into_response()
    }
}

// r[impl dashboard.api.version]
async fn api_version(State(state): State<AppState>) -> impl IntoResponse {
    let data = state.data.borrow().clone();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(format!(r#"{{"version":{}}}"#, data.version)))
        .unwrap()
}

/// Delta response for the API
#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
struct ApiDeltaResponse {
    /// Version when this delta was computed
    version: u64,
    /// Summary string of changes
    summary: String,
    /// Whether there are any changes
    has_changes: bool,
    /// Detailed changes per spec/impl
    changes: Vec<ApiImplDelta>,
}

#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
struct ApiImplDelta {
    /// Spec/impl key (e.g., "my-spec/rust")
    key: String,
    /// Coverage percentage change
    coverage_change: f64,
    /// Rules that became covered
    newly_covered: Vec<ApiCoverageChange>,
    /// Rules that lost coverage
    newly_uncovered: Vec<String>,
}

#[derive(Debug, Clone, Facet)]
#[facet(rename_all = "camelCase")]
struct ApiCoverageChange {
    rule_id: String,
    file: String,
    line: usize,
    ref_type: String,
}

async fn api_delta(State(state): State<AppState>) -> impl IntoResponse {
    let data = state.data.borrow().clone();
    let delta = &data.delta;

    let changes: Vec<ApiImplDelta> = delta
        .by_impl
        .iter()
        .filter(|(_, d)| !d.is_empty())
        .map(|(key, d)| ApiImplDelta {
            key: key.clone(),
            coverage_change: d.coverage_change(),
            newly_covered: d
                .newly_covered
                .iter()
                .map(|c| ApiCoverageChange {
                    rule_id: c.rule_id.clone(),
                    file: c.file.clone(),
                    line: c.line,
                    ref_type: c.ref_type.clone(),
                })
                .collect(),
            newly_uncovered: d.newly_uncovered.clone(),
        })
        .collect();

    let response = ApiDeltaResponse {
        version: data.version,
        summary: delta.summary(),
        has_changes: !delta.is_empty(),
        changes,
    };

    Json(response)
}

#[derive(Debug)]
struct FileQuery {
    path: String,
}

/// Get arborium language name from file extension
fn arborium_language(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?;
    match ext {
        // Rust
        "rs" => Some("rust"),
        // Go
        "go" => Some("go"),
        // C/C++
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Some("cpp"),
        // Web
        "js" | "mjs" | "cjs" => Some("javascript"),
        "ts" | "mts" | "cts" => Some("typescript"),
        "jsx" => Some("javascript"),
        "tsx" => Some("tsx"),
        // Python
        "py" => Some("python"),
        // Ruby
        "rb" => Some("ruby"),
        // Java/JVM
        "java" => Some("java"),
        "kt" | "kts" => Some("kotlin"),
        "scala" => Some("scala"),
        // Shell
        "sh" | "bash" | "zsh" => Some("bash"),
        // Config
        "json" => Some("json"),
        "yaml" | "yml" => Some("yaml"),
        "toml" => Some("toml"),
        "xml" => Some("xml"),
        // Web markup
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "scss" | "sass" => Some("scss"),
        // Markdown
        "md" | "markdown" => Some("markdown"),
        // SQL
        "sql" => Some("sql"),
        // Zig
        "zig" => Some("zig"),
        // Swift
        "swift" => Some("swift"),
        // Elixir
        "ex" | "exs" => Some("elixir"),
        // Haskell
        "hs" | "lhs" => Some("haskell"),
        // OCaml
        "ml" | "mli" => Some("ocaml"),
        // Lua
        "lua" => Some("lua"),
        // PHP
        "php" => Some("php"),
        // R
        "r" | "R" => Some("r"),
        _ => None,
    }
}

async fn api_file(
    State(state): State<AppState>,
    Query(params): Query<Vec<(String, String)>>,
) -> impl IntoResponse {
    let path = params
        .iter()
        .find(|(k, _)| k == "path")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let file_path = urlencoding::decode(&path).unwrap_or_default();
    let full_path = state.project_root.join(file_path.as_ref());
    // Canonicalize to handle cross-workspace paths like ../marq/...
    let full_path = full_path.canonicalize().unwrap_or(full_path);
    let data = state.data.borrow().clone();

    // Get impl key to find the right code units map
    let Some(impl_key) = get_impl_key(&params, &data.config) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"error":"No specs configured","code":"no_specs"}"#,
            ))
            .unwrap()
            .into_response();
    };

    let Some(code_units_by_file) = data.code_units_by_impl.get(&impl_key) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(format!(
                r#"{{"error":"Implementation not found: {} {}"}}"#,
                impl_key.0, impl_key.1
            )))
            .unwrap()
            .into_response();
    };

    if let Some(units) = code_units_by_file.get(&full_path) {
        let content = std::fs::read_to_string(&full_path).unwrap_or_default();
        let relative = full_path
            .strip_prefix(&state.project_root)
            .unwrap_or(&full_path)
            .display()
            .to_string();

        // Syntax highlight the content
        let html = if let Some(lang) = arborium_language(&relative) {
            let mut hl = state.highlighter.lock().unwrap();
            match hl.highlight(lang, &content) {
                Ok(highlighted) => highlighted,
                Err(_) => html_escape(&content),
            }
        } else {
            html_escape(&content)
        };

        let api_units: Vec<ApiCodeUnit> = units
            .iter()
            .map(|u| ApiCodeUnit {
                kind: format!("{:?}", u.kind).to_lowercase(),
                name: u.name.clone(),
                start_line: u.start_line,
                end_line: u.end_line,
                rule_refs: u.req_refs.clone(),
            })
            .collect();

        let file_data = ApiFileData {
            path: relative,
            content,
            html,
            units: api_units,
        };

        Json(file_data).into_response()
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"error":"File not found"}"#))
            .unwrap()
            .into_response()
    }
}

/// Response for file range queries
// r[impl dashboard.editing.api.fetch-range-response]
#[derive(Debug, Clone, Facet)]
struct ApiFileRange {
    content: String,
    start: usize,
    end: usize,
    file_hash: String,
}

/// Error response
#[derive(Debug, Clone, Facet)]
struct ApiError {
    error: String,
}

/// Response for git check
// r[impl dashboard.editing.git.api]
#[derive(Debug, Clone, Facet)]
struct ApiGitCheck {
    in_git: bool,
}

/// Helper function to check if a file is in a git repository
// r[impl dashboard.editing.git.check-required]
fn is_file_in_git(file_path: &std::path::Path) -> bool {
    // Check if the file exists
    if !file_path.exists() {
        return false;
    }

    // Get the directory containing the file
    let dir = if file_path.is_dir() {
        file_path
    } else {
        file_path.parent().unwrap_or(file_path)
    };

    // Run `git rev-parse --is-inside-work-tree` in the file's directory
    std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// GET `/api/check-git?path=<path>`
///
/// Check if a file is in a git repository
// r[impl dashboard.editing.git.api]
async fn api_check_git(
    State(state): State<AppState>,
    Query(params): Query<Vec<(String, String)>>,
) -> impl IntoResponse {
    let path = params
        .iter()
        .find(|(k, _)| k == "path")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let file_path = urlencoding::decode(&path).unwrap_or_default();
    let full_path = state.project_root.join(file_path.as_ref());
    // Canonicalize to handle cross-workspace paths like ../marq/...
    let full_path = full_path.canonicalize().unwrap_or(full_path);

    Json(ApiGitCheck {
        in_git: is_file_in_git(&full_path),
    })
    .into_response()
}

/// GET `/api/file-range?path=<path>&start=<byte-start>&end=<byte-end>`
///
/// Fetch a byte range from a file
// r[impl dashboard.editing.api.fetch-range]
// r[impl dashboard.editing.api.range-validation]
// r[impl dashboard.editing.api.utf8-validation]
async fn api_file_range(
    State(state): State<AppState>,
    Query(params): Query<Vec<(String, String)>>,
) -> impl IntoResponse {
    let path = params
        .iter()
        .find(|(k, _)| k == "path")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let start: usize = params
        .iter()
        .find(|(k, _)| k == "start")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);

    let end: usize = params
        .iter()
        .find(|(k, _)| k == "end")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);

    let file_path = urlencoding::decode(&path).unwrap_or_default();
    let full_path = state.project_root.join(file_path.as_ref());
    // Canonicalize to handle cross-workspace paths like ../marq/...
    let full_path = full_path.canonicalize().unwrap_or(full_path);

    if let Ok(content) = std::fs::read(&full_path) {
        if end > start && end <= content.len() {
            let range_bytes = &content[start..end];
            if let Ok(text) = String::from_utf8(range_bytes.to_vec()) {
                // r[impl dashboard.editing.api.fetch-range-response]
                let file_hash = blake3::hash(&content).to_hex().to_string();
                return Json(ApiFileRange {
                    content: text,
                    start,
                    end,
                    file_hash,
                })
                .into_response();
            }
        }
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "Invalid byte range or non-UTF8 content".to_string(),
            }),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "File not found".to_string(),
            }),
        )
            .into_response()
    }
}

/// Request body for updating a file range
// r[impl dashboard.editing.api.update-range]
// r[impl dashboard.editing.api.hash-conflict]
#[derive(Debug, Clone, Facet)]
struct ApiUpdateFileRange {
    path: String,
    start: usize,
    end: usize,
    content: String,
    file_hash: String,
}

/// Request body for previewing markdown
#[derive(Debug, Clone, Facet)]
struct ApiPreviewMarkdown {
    content: String,
}

/// Response for markdown preview
#[derive(Debug, Clone, Facet)]
struct ApiPreviewResponse {
    html: String,
}

/// PATCH /api/file-range
/// Update a byte range in a file
// r[impl dashboard.editing.api.update-range]
// r[impl dashboard.editing.api.range-validation]
// r[impl dashboard.editing.save.patch-file]
async fn api_update_file_range(
    State(state): State<AppState>,
    Json(req): Json<ApiUpdateFileRange>,
) -> impl IntoResponse {
    let file_path = urlencoding::decode(&req.path).unwrap_or_default();
    let full_path = state.project_root.join(file_path.as_ref());
    // Canonicalize to handle cross-workspace paths like ../marq/...
    let full_path = full_path.canonicalize().unwrap_or(full_path);

    if let Ok(original_content) = std::fs::read(&full_path) {
        // r[impl dashboard.editing.api.hash-conflict]
        // Check if file hash matches
        let current_hash = blake3::hash(&original_content).to_hex().to_string();
        if current_hash != req.file_hash {
            return (
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "File has changed since it was loaded. Please reload and try again."
                        .to_string(),
                }),
            )
                .into_response();
        }

        if req.end > req.start && req.end <= original_content.len() {
            // Build new content: before + new content + after
            let mut new_content = Vec::new();
            new_content.extend_from_slice(&original_content[..req.start]);
            new_content.extend_from_slice(req.content.as_bytes());
            new_content.extend_from_slice(&original_content[req.end..]);

            // Write back to file
            if std::fs::write(&full_path, &new_content).is_ok() {
                // r[impl dashboard.editing.api.update-range-response]
                let new_end = req.start + req.content.len();
                let new_hash = blake3::hash(&new_content).to_hex().to_string();
                return Json(ApiFileRange {
                    content: req.content,
                    start: req.start,
                    end: new_end,
                    file_hash: new_hash,
                })
                .into_response();
            } else {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: "Failed to write file".to_string(),
                    }),
                )
                    .into_response();
            }
        }
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "Invalid byte range".to_string(),
            }),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "File not found".to_string(),
            }),
        )
            .into_response()
    }
}

/// POST /api/preview-markdown
/// Render markdown to HTML for live preview
async fn api_preview_markdown(Json(req): Json<ApiPreviewMarkdown>) -> impl IntoResponse {
    // Use marq to render the markdown (same as dashboard)
    let options = marq::RenderOptions::default();
    match marq::render(&req.content, &options).await {
        Ok(result) => Json(ApiPreviewResponse { html: result.html }).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: format!("Failed to render markdown: {}", e),
            }),
        )
            .into_response(),
    }
}

async fn api_spec(
    State(state): State<AppState>,
    Query(params): Query<Vec<(String, String)>>,
) -> impl IntoResponse {
    let data = state.data.borrow().clone();

    // Get impl key (spec_name, impl_name)
    let Some(impl_key) = get_impl_key(&params, &data.config) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"error":"No specs configured","code":"no_specs"}"#,
            ))
            .unwrap()
            .into_response();
    };

    if let Some(spec_data) = data.specs_content_by_impl.get(&impl_key) {
        Json(spec_data.clone()).into_response()
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(format!(
                r#"{{"error":"Spec not found: {} {}"}}"#,
                impl_key.0, impl_key.1
            )))
            .unwrap()
            .into_response()
    }
}

async fn api_search(
    State(state): State<AppState>,
    Query(params): Query<Vec<(String, String)>>,
) -> impl IntoResponse {
    let query = params
        .iter()
        .find(|(k, _)| k == "q")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let query = urlencoding::decode(&query).unwrap_or_default();

    // Parse optional limit parameter
    let limit = params
        .iter()
        .find(|(k, _)| k == "limit")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(50usize);

    let data = state.data.borrow().clone();
    let results = data.search_index.search(&query, limit);
    let response = ApiSearchResponse {
        query: query.into_owned(),
        results,
        available: data.search_index.is_available(),
    };

    Json(response)
}

async fn serve_js() -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(Body::from(JS_BUNDLE))
        .unwrap()
}

async fn serve_css() -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(Body::from(CSS_BUNDLE))
        .unwrap()
}

async fn serve_html(State(state): State<AppState>) -> impl IntoResponse {
    if state.dev_mode {
        // In dev mode, proxy to Vite
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"error":"In dev mode, frontend is served by Vite"}"#,
            ))
            .unwrap();
    }
    Html(HTML_SHELL).into_response()
}

// ============================================================================
// Vite Proxy
// ============================================================================

/// Format headers for debug logging
fn format_headers(headers: &axum::http::HeaderMap) -> String {
    headers
        .iter()
        .map(|(k, v)| format!("  {}: {}", k, v.to_str().unwrap_or("<binary>")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Check if request has a WebSocket upgrade
fn has_ws(req: &Request<Body>) -> bool {
    // Check for Upgrade header with "websocket" value
    req.headers()
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
}

/// Proxy requests to Vite dev server (handles both HTTP and WebSocket)
async fn vite_proxy(State(state): State<AppState>, req: Request<Body>) -> Response<Body> {
    let vite_port = match state.vite_port {
        Some(p) => p,
        None => {
            warn!("Vite proxy request but vite server not running");
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::from("Vite server not running"))
                .unwrap();
        }
    };

    let method = req.method().clone();
    let original_uri = req.uri().to_string();
    let path = req.uri().path().to_string();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();

    // Log incoming request from browser
    info!(
        method = %method,
        uri = %original_uri,
        "=> browser request"
    );
    debug!(
        headers = %format_headers(req.headers()),
        "=> browser request headers"
    );

    // Check if this is a WebSocket upgrade request
    if has_ws(&req) {
        info!(uri = %original_uri, "=> detected websocket upgrade request");

        // Split into parts so we can extract WebSocketUpgrade
        let (mut parts, _body) = req.into_parts();

        // Log all request headers for websocket upgrade
        info!(
            headers = %format_headers(&parts.headers),
            "=> websocket upgrade request headers"
        );

        // Manually extract WebSocketUpgrade from request parts (like cove/home)
        let ws = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
            Ok(ws) => ws,
            Err(e) => {
                error!(error = %e, "!! failed to extract websocket upgrade");
                return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from(format!("WebSocket upgrade failed: {}", e)))
                    .unwrap();
            }
        };

        let target_uri = format!("ws://127.0.0.1:{}{}{}", vite_port, path, query);
        info!(target = %target_uri, "-> upgrading websocket to vite");

        return ws
            .protocols(["vite-hmr"])
            .on_upgrade(move |socket| async move {
                info!(path = %path, "websocket connection established, starting proxy");
                if let Err(e) = handle_vite_ws(socket, vite_port, &path, &query).await {
                    error!(error = %e, path = %path, "!! vite websocket proxy error");
                }
                info!(path = %path, "websocket connection closed");
            })
            .into_response();
    }

    // Regular HTTP proxy
    let target_uri = format!("http://127.0.0.1:{}{}{}", vite_port, path, query);

    let client = Client::builder(TokioExecutor::new()).build_http();

    let mut proxy_req_builder = Request::builder().method(req.method()).uri(&target_uri);

    // Copy headers (except Host)
    for (name, value) in req.headers() {
        if name != header::HOST {
            proxy_req_builder = proxy_req_builder.header(name, value);
        }
    }

    let proxy_req = proxy_req_builder.body(req.into_body()).unwrap();

    // Log outgoing request to Vite
    debug!(
        method = %proxy_req.method(),
        uri = %proxy_req.uri(),
        headers = %format_headers(proxy_req.headers()),
        "-> sending to vite"
    );

    match client.request(proxy_req).await {
        Ok(res) => {
            let status = res.status();

            // Log Vite's response
            info!(
                status = %status,
                path = %path,
                "<- vite response"
            );
            debug!(
                headers = %format_headers(res.headers()),
                "<- vite response headers"
            );

            let (parts, body) = res.into_parts();
            let response = Response::from_parts(parts, Body::new(body));

            // Log what we're sending back to browser
            debug!(
                status = %response.status(),
                headers = %format_headers(response.headers()),
                "<= responding to browser"
            );

            response
        }
        Err(e) => {
            error!(error = %e, target = %target_uri, "!! vite proxy error");
            let response = Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from(format!("Vite proxy error: {}", e)))
                .unwrap();

            info!(
                status = %response.status(),
                "<= responding to browser (error)"
            );

            response
        }
    }
}

async fn handle_vite_ws(
    client_socket: ws::WebSocket,
    vite_port: u16,
    path: &str,
    query: &str,
) -> Result<()> {
    use tokio_tungstenite::connect_async_with_config;
    use tokio_tungstenite::tungstenite::http::Request;

    let vite_url = format!("ws://127.0.0.1:{}{}{}", vite_port, path, query);

    info!(vite_url = %vite_url, "-> connecting to vite websocket");

    // Build request with vite-hmr subprotocol
    let request = Request::builder()
        .uri(&vite_url)
        .header("Sec-WebSocket-Protocol", "vite-hmr")
        .header("Host", format!("127.0.0.1:{}", vite_port))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .body(())
        .unwrap();

    let connect_timeout = Duration::from_secs(5);
    let connect_result = tokio::time::timeout(
        connect_timeout,
        connect_async_with_config(request, None, false),
    )
    .await;

    let (vite_ws, response) = match connect_result {
        Ok(Ok((ws, resp))) => {
            info!(vite_url = %vite_url, "-> successfully connected to vite websocket");
            (ws, resp)
        }
        Ok(Err(e)) => {
            info!(vite_url = %vite_url, error = %e, "!! failed to connect to vite websocket");
            return Err(e.into());
        }
        Err(_) => {
            info!(vite_url = %vite_url, timeout_secs = ?connect_timeout.as_secs(), "!! timeout connecting to vite websocket");
            return Err(eyre::eyre!(
                "Timeout connecting to Vite WebSocket after {:?}",
                connect_timeout
            ));
        }
    };

    info!(
        status = %response.status(),
        "<- vite websocket connection established"
    );
    debug!(
        headers = %format_headers(response.headers()),
        "<- vite websocket response headers"
    );

    let (mut client_tx, mut client_rx) = client_socket.split();
    let (mut vite_tx, mut vite_rx) = vite_ws.split();

    info!("websocket proxy: starting bidirectional relay");

    // Bidirectional proxy
    let client_to_vite = async {
        while let Some(msg) = client_rx.next().await {
            match msg {
                Ok(ws::Message::Text(text)) => {
                    let text_str: String = text.to_string();
                    info!(
                        size = text_str.len(),
                        preview = %text_str.chars().take(100).collect::<String>(),
                        "=> forwarding text message to vite"
                    );
                    if vite_tx
                        .send(tokio_tungstenite::tungstenite::Message::Text(
                            text_str.into(),
                        ))
                        .await
                        .is_err()
                    {
                        info!("!! vite send failed (client_to_vite), breaking");
                        break;
                    }
                }
                Ok(ws::Message::Binary(data)) => {
                    let data_vec: Vec<u8> = data.to_vec();
                    info!(
                        size = data_vec.len(),
                        "=> forwarding binary message to vite"
                    );
                    if vite_tx
                        .send(tokio_tungstenite::tungstenite::Message::Binary(
                            data_vec.into(),
                        ))
                        .await
                        .is_err()
                    {
                        info!("!! vite send failed (client_to_vite), breaking");
                        break;
                    }
                }
                Ok(ws::Message::Close(_)) => {
                    info!("=> client closed connection");
                    break;
                }
                Err(e) => {
                    info!(error = %e, "!! client receive error, breaking");
                    break;
                }
                _ => {
                    debug!("=> ignoring other message type from client");
                }
            }
        }
        info!("client_to_vite relay ended");
    };

    let vite_to_client = async {
        while let Some(msg) = vite_rx.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    let text_str: String = text.to_string();
                    info!(
                        size = text_str.len(),
                        preview = %text_str.chars().take(100).collect::<String>(),
                        "<= forwarding text message to client"
                    );
                    if client_tx
                        .send(ws::Message::Text(text_str.into()))
                        .await
                        .is_err()
                    {
                        info!("!! client send failed (vite_to_client), breaking");
                        break;
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Binary(data)) => {
                    let data_vec: Vec<u8> = data.to_vec();
                    info!(
                        size = data_vec.len(),
                        "<= forwarding binary message to client"
                    );
                    if client_tx
                        .send(ws::Message::Binary(data_vec.into()))
                        .await
                        .is_err()
                    {
                        info!("!! client send failed (vite_to_client), breaking");
                        break;
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                    info!("<= vite closed connection");
                    break;
                }
                Err(e) => {
                    info!(error = %e, "!! vite receive error, breaking");
                    break;
                }
                _ => {
                    debug!("<= ignoring other message type from vite");
                }
            }
        }
        info!("vite_to_client relay ended");
    };

    tokio::select! {
        _ = client_to_vite => {
            info!("websocket proxy: client_to_vite completed first");
        }
        _ = vite_to_client => {
            info!("websocket proxy: vite_to_client completed first");
        }
    }

    info!("websocket proxy: connection closed, exiting");

    Ok(())
}

// ============================================================================
// HTTP Server
// ============================================================================

/// Run the serve command
pub fn run(
    project_root: Option<PathBuf>,
    config_path: Option<PathBuf>,
    port: u16,
    open_browser: bool,
    dev_mode: bool,
) -> Result<()> {
    // Initialize tracing
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // Default to info for our crate, warn for others
        EnvFilter::new("tracey=info,warn")
    });

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(true).with_level(true))
        .with(filter)
        .init();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .wrap_err("Failed to create tokio runtime")?;

    rt.block_on(
        async move { run_server(project_root, config_path, port, open_browser, dev_mode).await },
    )
}

async fn run_server(
    project_root: Option<PathBuf>,
    config_path: Option<PathBuf>,
    port: u16,
    open_browser: bool,
    dev_mode: bool,
) -> Result<()> {
    let project_root = match project_root {
        Some(root) => root
            .canonicalize()
            .wrap_err("Failed to canonicalize project root")?,
        None => crate::find_project_root()?,
    };
    // r[impl config.path.default]
    let config_path = config_path.unwrap_or_else(|| project_root.join(".config/tracey/config.kdl"));
    let config = crate::load_config(&config_path)?;

    let version = Arc::new(AtomicU64::new(1));

    // Initial build
    let initial_data = build_dashboard_data(&project_root, &config, 1, false).await?;

    // Channel for state updates
    let (tx, rx) = watch::channel(Arc::new(initial_data));

    // Start Vite dev server if in dev mode
    let vite_port = if dev_mode {
        let dashboard_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dashboard");
        let vite = ViteServer::start(&dashboard_dir).await?;
        Some(vite.port)
    } else {
        None
    };

    // Clone for file watcher
    let watch_project_root = project_root.clone();
    let watch_config = config.clone();

    let (debounce_tx, mut debounce_rx) = tokio::sync::mpsc::channel::<()>(1);

    // File watcher thread
    std::thread::spawn(move || {
        let debounce_tx = debounce_tx;
        let watch_root = watch_project_root.clone();
        let watch_root_for_closure = watch_root.clone();

        // r[impl server.watch.patterns-from-config]
        // Collect all include patterns from config (specs + impls)
        let mut include_patterns: Vec<String> = Vec::new();
        let mut exclude_patterns: Vec<String> = Vec::new();
        for spec in &watch_config.specs {
            // Spec include patterns (markdown files)
            for inc in &spec.include {
                include_patterns.push(inc.pattern.clone());
            }
            // Impl include/exclude patterns (source files)
            for impl_ in &spec.impls {
                for inc in &impl_.include {
                    include_patterns.push(inc.pattern.clone());
                }
                for exc in &impl_.exclude {
                    exclude_patterns.push(exc.pattern.clone());
                }
            }
        }
        // r[impl server.watch.config-file]
        // Also watch the config file itself
        include_patterns.push(".config/tracey/config.kdl".to_string());

        // r[impl server.watch.debounce]
        let mut debouncer = match new_debouncer(
            Duration::from_millis(200),
            move |res: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                // Check if path matches any include pattern (relative to watch root)
                let matches_include = |path: &Path| {
                    let relative = path
                        .strip_prefix(&watch_root_for_closure)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .replace('\\', "/");

                    for pattern in &include_patterns {
                        if glob_match(&relative, pattern) {
                            return true;
                        }
                    }
                    false
                };

                // r[impl server.watch.respect-excludes]
                // Check if path matches any exclude pattern
                let matches_exclude = |path: &Path| {
                    let relative = path
                        .strip_prefix(&watch_root_for_closure)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .replace('\\', "/");

                    for pattern in &exclude_patterns {
                        if glob_match(&relative, pattern) {
                            return true;
                        }
                    }
                    false
                };

                let should_trigger = |path: &Path| matches_include(path) && !matches_exclude(path);

                match res {
                    Ok(events) => {
                        let dominated_events: Vec<_> =
                            events.iter().filter(|e| should_trigger(&e.path)).collect();
                        if dominated_events.is_empty() {
                            debug!(
                                total = events.len(),
                                "all file events filtered out (not matching config patterns)"
                            );
                        } else {
                            info!(
                                count = dominated_events.len(),
                                paths = ?dominated_events.iter().map(|e| e.path.display().to_string()).collect::<Vec<_>>(),
                                "file change detected, triggering rebuild"
                            );
                            let _ = debounce_tx.blocking_send(());
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "file watcher error");
                    }
                };
            },
        ) {
            Ok(d) => d,
            Err(e) => {
                error!(error = %e, "failed to create file watcher");
                return;
            }
        };

        // Watch project root
        info!(path = %watch_root.display(), "starting file watcher");
        if let Err(e) = debouncer
            .watcher()
            .watch(&watch_root, RecursiveMode::Recursive)
        {
            error!(
                error = %e,
                path = %watch_root.display(),
                "failed to watch directory"
            );
        }

        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    });

    // Rebuild task
    let rebuild_tx = tx.clone();
    let rebuild_rx = rx.clone();
    let rebuild_project_root = project_root.clone();
    let rebuild_config_path = config_path.clone();
    let rebuild_version = version.clone();

    tokio::spawn(async move {
        while debounce_rx.recv().await.is_some() {
            let config = match crate::load_config(&rebuild_config_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{} Config reload error: {}", "!".yellow(), e);
                    continue;
                }
            };

            // Get current data for hash comparison and delta computation
            let old_data = rebuild_rx.borrow().clone();

            // Build with placeholder version (we'll set real version if hash changed)
            match build_dashboard_data(&rebuild_project_root, &config, 0, false).await {
                Ok(mut data) => {
                    // Only bump version if content actually changed
                    if data.content_hash != old_data.content_hash {
                        let new_version = rebuild_version.fetch_add(1, Ordering::SeqCst) + 1;
                        data.version = new_version;

                        // Compute delta between old and new data
                        let delta = crate::server::Delta::compute(&old_data, &data);

                        // Print rebuild message with delta summary
                        let delta_summary = delta.summary();
                        if delta.is_empty() {
                            eprintln!(
                                "{} Rebuilt dashboard (v{})",
                                "->".blue().bold(),
                                new_version
                            );
                        } else {
                            eprintln!(
                                "{} Rebuilt dashboard (v{}) - {}",
                                "->".blue().bold(),
                                new_version,
                                delta_summary.green()
                            );
                        }

                        // Store delta in the new data
                        data.delta = delta;

                        let _ = rebuild_tx.send(Arc::new(data));
                    }
                    // If hash is same, silently ignore the rebuild
                }
                Err(e) => {
                    eprintln!("{} Rebuild error: {}", "!".yellow(), e);
                }
            }
        }
    });

    let app_state = AppState {
        data: rx,
        project_root: project_root.clone(),
        dev_mode,
        vite_port,
        highlighter: Arc::new(Mutex::new(arborium::Highlighter::new())),
    };

    // Build router
    // r[impl dashboard.api.config]
    // r[impl dashboard.api.forward]
    // r[impl dashboard.api.reverse]
    // r[impl dashboard.api.version]
    // r[impl dashboard.api.file]
    // r[impl dashboard.api.spec]
    let mut app = Router::new()
        .route("/api/config", get(api_config))
        .route("/api/forward", get(api_forward))
        .route("/api/reverse", get(api_reverse))
        .route("/api/version", get(api_version))
        .route("/api/delta", get(api_delta))
        .route("/api/file", get(api_file))
        .route("/api/check-git", get(api_check_git))
        .route("/api/file-range", get(api_file_range))
        .route("/api/file-range", patch(api_update_file_range))
        .route("/api/preview-markdown", post(api_preview_markdown))
        .route("/api/spec", get(api_spec))
        .route("/api/search", get(api_search));

    if dev_mode {
        // In dev mode, proxy everything else to Vite (both HTTP and WebSocket)
        app = app.fallback(vite_proxy);
    } else {
        // In production mode, serve static assets
        app = app
            .route("/assets/{*path}", get(serve_static_asset))
            .fallback(serve_html);
    }

    // Add CORS for dev mode
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    let app = app.layer(cors).with_state(app_state);

    // Start server
    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .wrap_err_with(|| format!("Failed to bind to {}", addr))?;

    let url = format!("http://{}", addr);

    if dev_mode {
        eprintln!(
            "\n{} Dashboard running at {}\n",
            "OK".green().bold(),
            url.cyan()
        );
        eprintln!(
            "   {} Vite HMR enabled - changes will hot reload\n",
            "->".blue().bold()
        );
    } else {
        eprintln!(
            "\n{} Serving tracey dashboard at {}\n   Press Ctrl+C to stop\n",
            "OK".green().bold(),
            url.cyan()
        );
    }

    if open_browser && let Err(e) = open::that(&url) {
        eprintln!("{} Failed to open browser: {}", "!".yellow(), e);
    }

    axum::serve(listener, app).await.wrap_err("Server error")?;

    Ok(())
}

async fn serve_static_asset(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl IntoResponse {
    if path.ends_with(".js") {
        serve_js().await.into_response()
    } else if path.ends_with(".css") {
        serve_css().await.into_response()
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not found"))
            .unwrap()
    }
}
