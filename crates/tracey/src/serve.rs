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
    routing::get,
};
use eyre::{Result, WrapErr};
use futures_util::{SinkExt, StreamExt};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use owo_colors::OwoColorize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::watch;
use tower_http::cors::{Any, CorsLayer};
use tracey_core::code_units::CodeUnit;
use tracey_core::{RefVerb, Rules, SpecManifest};
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::search::{self, SearchIndex};
use crate::vite::ViteServer;

// ============================================================================
// JSON API Types
// ============================================================================

/// Project configuration info
#[derive(Debug, Clone)]
struct ApiConfig {
    project_root: String,
    specs: Vec<ApiSpecInfo>,
}

#[derive(Debug, Clone)]
struct ApiSpecInfo {
    name: String,
    /// Path to spec file(s) if local
    source: Option<String>,
}

/// Forward traceability: rules with their code references
#[derive(Debug, Clone)]
struct ApiForwardData {
    specs: Vec<ApiSpecForward>,
}

#[derive(Debug, Clone)]
struct ApiSpecForward {
    name: String,
    rules: Vec<ApiRule>,
}

#[derive(Debug, Clone)]
struct ApiRule {
    id: String,
    text: Option<String>,
    status: Option<String>,
    level: Option<String>,
    source_file: Option<String>,
    source_line: Option<usize>,
    impl_refs: Vec<ApiCodeRef>,
    verify_refs: Vec<ApiCodeRef>,
    depends_refs: Vec<ApiCodeRef>,
}

#[derive(Debug, Clone)]
struct ApiCodeRef {
    file: String,
    line: usize,
}

/// Reverse traceability: file tree with coverage info
#[derive(Debug, Clone)]
struct ApiReverseData {
    /// Total code units across all files
    total_units: usize,
    /// Code units with at least one rule reference
    covered_units: usize,
    /// File tree with coverage info
    files: Vec<ApiFileEntry>,
}

#[derive(Debug, Clone)]
struct ApiFileEntry {
    path: String,
    /// Number of code units in this file
    total_units: usize,
    /// Number of covered code units
    covered_units: usize,
}

/// Single file with full coverage details
#[derive(Debug, Clone)]
struct ApiFileData {
    path: String,
    content: String,
    /// Code units in this file with their coverage
    units: Vec<ApiCodeUnit>,
}

#[derive(Debug, Clone)]
struct ApiCodeUnit {
    kind: String,
    name: Option<String>,
    start_line: usize,
    end_line: usize,
    /// Rule references found in this code unit's comments
    rule_refs: Vec<String>,
}

/// Spec content
#[derive(Debug, Clone)]
struct ApiSpecData {
    name: String,
    /// Raw markdown content
    content: String,
    /// Source file path
    source_file: Option<String>,
}

// ============================================================================
// Server State
// ============================================================================

/// Computed dashboard data that gets rebuilt on file changes
struct DashboardData {
    config: ApiConfig,
    forward: ApiForwardData,
    reverse: ApiReverseData,
    /// All code units indexed by file path
    code_units_by_file: BTreeMap<PathBuf, Vec<CodeUnit>>,
    /// Spec content by name
    specs_content: BTreeMap<String, ApiSpecData>,
    /// Full-text search index for source files
    search_index: Box<dyn SearchIndex>,
    /// Version number (incremented only when content actually changes)
    version: u64,
    /// Hash of forward + reverse JSON for change detection
    content_hash: u64,
}

/// Shared application state
#[derive(Clone)]
struct AppState {
    data: watch::Receiver<Arc<DashboardData>>,
    project_root: PathBuf,
    dev_mode: bool,
    vite_port: Option<u16>,
}

// ============================================================================
// JSON Serialization (manual, no serde)
// ============================================================================

pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_opt_string(s: &Option<String>) -> String {
    match s {
        Some(s) => json_string(s),
        None => "null".to_string(),
    }
}

impl ApiConfig {
    fn to_json(&self) -> String {
        let specs: Vec<String> = self
            .specs
            .iter()
            .map(|s| {
                format!(
                    r#"{{"name":{},"source":{}}}"#,
                    json_string(&s.name),
                    json_opt_string(&s.source)
                )
            })
            .collect();
        format!(
            r#"{{"projectRoot":{},"specs":[{}]}}"#,
            json_string(&self.project_root),
            specs.join(",")
        )
    }
}

impl ApiCodeRef {
    fn to_json(&self) -> String {
        format!(
            r#"{{"file":{},"line":{}}}"#,
            json_string(&self.file),
            self.line
        )
    }
}

impl ApiRule {
    fn to_json(&self) -> String {
        let impl_refs: Vec<String> = self.impl_refs.iter().map(|r| r.to_json()).collect();
        let verify_refs: Vec<String> = self.verify_refs.iter().map(|r| r.to_json()).collect();
        let depends_refs: Vec<String> = self.depends_refs.iter().map(|r| r.to_json()).collect();

        format!(
            r#"{{"id":{},"text":{},"status":{},"level":{},"sourceFile":{},"sourceLine":{},"implRefs":[{}],"verifyRefs":[{}],"dependsRefs":[{}]}}"#,
            json_string(&self.id),
            json_opt_string(&self.text),
            json_opt_string(&self.status),
            json_opt_string(&self.level),
            json_opt_string(&self.source_file),
            self.source_line
                .map(|n| n.to_string())
                .unwrap_or_else(|| "null".to_string()),
            impl_refs.join(","),
            verify_refs.join(","),
            depends_refs.join(",")
        )
    }
}

impl ApiForwardData {
    fn to_json(&self) -> String {
        let specs: Vec<String> = self
            .specs
            .iter()
            .map(|s| {
                let rules: Vec<String> = s.rules.iter().map(|r| r.to_json()).collect();
                format!(
                    r#"{{"name":{},"rules":[{}]}}"#,
                    json_string(&s.name),
                    rules.join(",")
                )
            })
            .collect();
        format!(r#"{{"specs":[{}]}}"#, specs.join(","))
    }
}

impl ApiFileEntry {
    fn to_json(&self) -> String {
        format!(
            r#"{{"path":{},"totalUnits":{},"coveredUnits":{}}}"#,
            json_string(&self.path),
            self.total_units,
            self.covered_units
        )
    }
}

impl ApiReverseData {
    fn to_json(&self) -> String {
        let files: Vec<String> = self.files.iter().map(|f| f.to_json()).collect();
        format!(
            r#"{{"totalUnits":{},"coveredUnits":{},"files":[{}]}}"#,
            self.total_units,
            self.covered_units,
            files.join(",")
        )
    }
}

impl ApiCodeUnit {
    fn to_json(&self) -> String {
        let refs: Vec<String> = self.rule_refs.iter().map(|r| json_string(r)).collect();
        format!(
            r#"{{"kind":{},"name":{},"startLine":{},"endLine":{},"ruleRefs":[{}]}}"#,
            json_string(&self.kind),
            json_opt_string(&self.name),
            self.start_line,
            self.end_line,
            refs.join(",")
        )
    }
}

impl ApiFileData {
    fn to_json(&self) -> String {
        let units: Vec<String> = self.units.iter().map(|u| u.to_json()).collect();
        format!(
            r#"{{"path":{},"content":{},"units":[{}]}}"#,
            json_string(&self.path),
            json_string(&self.content),
            units.join(",")
        )
    }
}

impl ApiSpecData {
    fn to_json(&self) -> String {
        format!(
            r#"{{"name":{},"content":{},"sourceFile":{}}}"#,
            json_string(&self.name),
            json_string(&self.content),
            json_opt_string(&self.source_file)
        )
    }
}

// ============================================================================
// Data Building
// ============================================================================

fn build_dashboard_data(
    project_root: &Path,
    config_path: &Path,
    config: &Config,
    version: u64,
) -> Result<DashboardData> {
    use tracey_core::WalkSources;
    use tracey_core::code_units::extract_rust;

    let abs_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());

    let config_dir = config_path
        .parent()
        .ok_or_else(|| eyre::eyre!("Config path has no parent directory"))?;

    let mut api_config = ApiConfig {
        project_root: abs_root.display().to_string(),
        specs: Vec::new(),
    };

    let mut forward_specs = Vec::new();
    let mut code_units_by_file: BTreeMap<PathBuf, Vec<CodeUnit>> = BTreeMap::new();
    let mut specs_content: BTreeMap<String, ApiSpecData> = BTreeMap::new();

    for spec_config in &config.specs {
        let spec_name = &spec_config.name.value;

        api_config.specs.push(ApiSpecInfo {
            name: spec_name.clone(),
            source: spec_config.rules_glob.as_ref().map(|g| g.pattern.clone()),
        });

        // Load manifest
        let manifest: SpecManifest = if let Some(rules_url) = &spec_config.rules_url {
            eprintln!(
                "   {} manifest from {}",
                "Fetching".green(),
                rules_url.value
            );
            SpecManifest::fetch(&rules_url.value)?
        } else if let Some(rules_file) = &spec_config.rules_file {
            let path = config_dir.join(&rules_file.path);
            SpecManifest::load(&path)?
        } else if let Some(glob) = &spec_config.rules_glob {
            eprintln!("   {} rules from {}", "Extracting".green(), glob.pattern);
            let manifest = crate::load_manifest_from_glob(project_root, &glob.pattern)?;

            // Also load spec content for each matched file
            load_spec_content(project_root, &glob.pattern, spec_name, &mut specs_content)?;

            manifest
        } else {
            eyre::bail!(
                "Spec '{}' has no rules_url, rules_file, or rules_glob",
                spec_name
            );
        };

        // Scan source files
        let include: Vec<String> = if spec_config.include.is_empty() {
            vec!["**/*.rs".to_string()]
        } else {
            spec_config
                .include
                .iter()
                .map(|i| i.pattern.clone())
                .collect()
        };
        let exclude: Vec<String> = spec_config
            .exclude
            .iter()
            .map(|e| e.pattern.clone())
            .collect();

        let rules = Rules::extract(
            WalkSources::new(project_root)
                .include(include.clone())
                .exclude(exclude.clone()),
        )?;

        // Build forward data for this spec
        let mut api_rules = Vec::new();
        for (rule_id, rule_info) in &manifest.rules {
            let mut impl_refs = Vec::new();
            let mut verify_refs = Vec::new();
            let mut depends_refs = Vec::new();

            for r in &rules.references {
                if r.rule_id == *rule_id {
                    let relative = r.file.strip_prefix(project_root).unwrap_or(&r.file);
                    let code_ref = ApiCodeRef {
                        file: relative.display().to_string(),
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
                id: rule_id.clone(),
                text: rule_info.text.clone(),
                status: rule_info.status.clone(),
                level: rule_info.level.clone(),
                source_file: rule_info.source_file.clone(),
                source_line: rule_info.source_line,
                impl_refs,
                verify_refs,
                depends_refs,
            });
        }

        // Sort rules by ID
        api_rules.sort_by(|a, b| a.id.cmp(&b.id));

        forward_specs.push(ApiSpecForward {
            name: spec_name.clone(),
            rules: api_rules,
        });

        // Extract code units for reverse traceability
        let walker = ignore::WalkBuilder::new(project_root)
            .follow_links(true)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker.flatten() {
            let path = entry.path();

            if path.extension().is_some_and(|e| e == "rs") {
                // Check include/exclude
                let relative = path.strip_prefix(project_root).unwrap_or(path);
                let relative_str = relative.to_string_lossy();

                let included = include
                    .iter()
                    .any(|pattern| glob_match(&relative_str, pattern));

                let excluded = exclude
                    .iter()
                    .any(|pattern| glob_match(&relative_str, pattern));

                if included
                    && !excluded
                    && let Ok(content) = std::fs::read_to_string(path)
                {
                    let code_units = extract_rust(path, &content);
                    if !code_units.is_empty() {
                        code_units_by_file.insert(path.to_path_buf(), code_units.units);
                    }
                }
            }
        }
    }

    // Build reverse data summary and collect file contents for search
    let mut total_units = 0;
    let mut covered_units = 0;
    let mut file_entries = Vec::new();
    let mut file_contents: BTreeMap<PathBuf, String> = BTreeMap::new();

    for (path, units) in &code_units_by_file {
        let relative = path.strip_prefix(project_root).unwrap_or(path);
        let file_total = units.len();
        let file_covered = units.iter().filter(|u| !u.rule_refs.is_empty()).count();

        total_units += file_total;
        covered_units += file_covered;

        file_entries.push(ApiFileEntry {
            path: relative.display().to_string(),
            total_units: file_total,
            covered_units: file_covered,
        });

        // Load file content for search index
        if let Ok(content) = std::fs::read_to_string(path) {
            file_contents.insert(path.clone(), content);
        }
    }

    // Sort files by path
    file_entries.sort_by(|a, b| a.path.cmp(&b.path));

    // Collect all rules for search index
    let search_rules: Vec<search::RuleEntry> = forward_specs
        .iter()
        .flat_map(|spec| {
            spec.rules.iter().map(|r| search::RuleEntry {
                id: r.id.clone(),
                text: r.text.clone(),
            })
        })
        .collect();

    // Build search index with sources and rules
    let search_index = search::build_index(project_root, &file_contents, &search_rules);

    let forward = ApiForwardData {
        specs: forward_specs,
    };
    let reverse = ApiReverseData {
        total_units,
        covered_units,
        files: file_entries,
    };

    // Compute content hash for change detection
    let forward_json = forward.to_json();
    let reverse_json = reverse.to_json();
    let content_hash = simple_hash(&forward_json) ^ simple_hash(&reverse_json);

    Ok(DashboardData {
        config: api_config,
        forward,
        reverse,
        code_units_by_file,
        specs_content,
        search_index,
        version,
        content_hash,
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

fn load_spec_content(
    root: &Path,
    pattern: &str,
    spec_name: &str,
    specs_content: &mut BTreeMap<String, ApiSpecData>,
) -> Result<()> {
    use ignore::WalkBuilder;

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
        let relative_str = relative.to_string_lossy();

        if !glob_match(&relative_str, pattern) {
            continue;
        }

        if let Ok(content) = std::fs::read_to_string(path) {
            specs_content.insert(
                spec_name.to_string(),
                ApiSpecData {
                    name: spec_name.to_string(),
                    content,
                    source_file: Some(relative_str.to_string()),
                },
            );
        }
    }

    Ok(())
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
const JS_BUNDLE: &str = include_str!("../dashboard/dist/assets/index-CPUJscSr.js");

/// CSS bundle from Vite build
const CSS_BUNDLE: &str = include_str!("../dashboard/dist/assets/index-DxtBNHQk.css");

// ============================================================================
// Route Handlers
// ============================================================================

async fn api_config(State(state): State<AppState>) -> impl IntoResponse {
    let data = state.data.borrow().clone();
    json_response(data.config.to_json())
}

async fn api_forward(State(state): State<AppState>) -> impl IntoResponse {
    let data = state.data.borrow().clone();
    json_response(data.forward.to_json())
}

async fn api_reverse(State(state): State<AppState>) -> impl IntoResponse {
    let data = state.data.borrow().clone();
    json_response(data.reverse.to_json())
}

async fn api_version(State(state): State<AppState>) -> impl IntoResponse {
    let data = state.data.borrow().clone();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(format!(r#"{{"version":{}}}"#, data.version)))
        .unwrap()
}

#[derive(Debug)]
struct FileQuery {
    path: String,
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
    let data = state.data.borrow().clone();

    if let Some(units) = data.code_units_by_file.get(&full_path) {
        let content = std::fs::read_to_string(&full_path).unwrap_or_default();
        let relative = full_path
            .strip_prefix(&state.project_root)
            .unwrap_or(&full_path)
            .display()
            .to_string();

        let api_units: Vec<ApiCodeUnit> = units
            .iter()
            .map(|u| ApiCodeUnit {
                kind: format!("{:?}", u.kind).to_lowercase(),
                name: u.name.clone(),
                start_line: u.start_line,
                end_line: u.end_line,
                rule_refs: u.rule_refs.clone(),
            })
            .collect();

        let file_data = ApiFileData {
            path: relative,
            content,
            units: api_units,
        };

        json_response(file_data.to_json())
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"error":"File not found"}"#))
            .unwrap()
    }
}

async fn api_spec(
    State(state): State<AppState>,
    Query(params): Query<Vec<(String, String)>>,
) -> impl IntoResponse {
    let name = params
        .iter()
        .find(|(k, _)| k == "name")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let spec_name = urlencoding::decode(&name).unwrap_or_default();
    let data = state.data.borrow().clone();

    if let Some(spec_data) = data.specs_content.get(spec_name.as_ref()) {
        json_response(spec_data.to_json())
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"error":"Spec not found"}"#))
            .unwrap()
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
    let results_json: Vec<String> = results.iter().map(|r| r.to_json()).collect();
    let json = format!(
        r#"{{"query":{},"results":[{}],"available":{}}}"#,
        json_string(&query),
        results_json.join(","),
        data.search_index.is_available()
    );

    json_response(json)
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

fn json_response(body: String) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
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

/// Check if request has a WebSocket upgrade (like cove/home's has_ws())
fn has_ws(req: &Request<Body>) -> bool {
    req.extensions()
        .get::<hyper::upgrade::OnUpgrade>()
        .is_some()
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
    use tokio_tungstenite::connect_async;

    let vite_url = format!("ws://127.0.0.1:{}{}{}", vite_port, path, query);

    let (vite_ws, _) = connect_async(&vite_url)
        .await
        .wrap_err("Failed to connect to Vite WebSocket")?;

    let (mut client_tx, mut client_rx) = client_socket.split();
    let (mut vite_tx, mut vite_rx) = vite_ws.split();

    // Bidirectional proxy
    let client_to_vite = async {
        while let Some(msg) = client_rx.next().await {
            match msg {
                Ok(ws::Message::Text(text)) => {
                    let text_str: String = text.to_string();
                    if vite_tx
                        .send(tokio_tungstenite::tungstenite::Message::Text(
                            text_str.into(),
                        ))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(ws::Message::Binary(data)) => {
                    let data_vec: Vec<u8> = data.to_vec();
                    if vite_tx
                        .send(tokio_tungstenite::tungstenite::Message::Binary(
                            data_vec.into(),
                        ))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(ws::Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    };

    let vite_to_client = async {
        while let Some(msg) = vite_rx.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    let text_str: String = text.to_string();
                    if client_tx
                        .send(ws::Message::Text(text_str.into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Binary(data)) => {
                    let data_vec: Vec<u8> = data.to_vec();
                    if client_tx
                        .send(ws::Message::Binary(data_vec.into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    };

    tokio::select! {
        _ = client_to_vite => {}
        _ = vite_to_client => {}
    }

    Ok(())
}

// ============================================================================
// HTTP Server
// ============================================================================

/// Run the serve command
pub fn run(
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

    rt.block_on(async move { run_server(config_path, port, open_browser, dev_mode).await })
}

async fn run_server(
    config_path: Option<PathBuf>,
    port: u16,
    open_browser: bool,
    dev_mode: bool,
) -> Result<()> {
    let project_root = crate::find_project_root()?;
    let config_path = config_path.unwrap_or_else(|| project_root.join(".config/tracey/config.kdl"));
    let config = crate::load_config(&config_path)?;

    let version = Arc::new(AtomicU64::new(1));

    // Initial build
    let initial_data = build_dashboard_data(&project_root, &config_path, &config, 1)?;

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

    let (debounce_tx, mut debounce_rx) = tokio::sync::mpsc::channel::<()>(1);

    // File watcher thread
    std::thread::spawn(move || {
        let debounce_tx = debounce_tx;
        let watch_root = watch_project_root.clone();

        let mut debouncer = match new_debouncer(
            Duration::from_millis(200),
            move |res: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                // Filter events to ignore node_modules, target, .git, and dashboard
                let ignored_paths = ["node_modules", "target", ".git", "dashboard", ".vite"];

                let is_ignored = |path: &Path| {
                    for component in path.components() {
                        if let std::path::Component::Normal(name) = component {
                            if let Some(name_str) = name.to_str() {
                                if ignored_paths.contains(&name_str) {
                                    return true;
                                }
                            }
                        }
                    }
                    false
                };

                match res {
                    Ok(events) => {
                        let dominated_events: Vec<_> =
                            events.iter().filter(|e| !is_ignored(&e.path)).collect();
                        if dominated_events.is_empty() {
                            debug!(
                                total = events.len(),
                                "all file events filtered out (ignored paths)"
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

            // Get current hash to compare
            let current_hash = rebuild_rx.borrow().content_hash;

            // Build with placeholder version (we'll set real version if hash changed)
            match build_dashboard_data(&rebuild_project_root, &rebuild_config_path, &config, 0) {
                Ok(mut data) => {
                    // Only bump version if content actually changed
                    if data.content_hash != current_hash {
                        let new_version = rebuild_version.fetch_add(1, Ordering::SeqCst) + 1;
                        data.version = new_version;
                        eprintln!(
                            "{} Rebuilt dashboard (v{})",
                            "->".blue().bold(),
                            new_version
                        );
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
    };

    // Build router
    let mut app = Router::new()
        .route("/api/config", get(api_config))
        .route("/api/forward", get(api_forward))
        .route("/api/reverse", get(api_reverse))
        .route("/api/version", get(api_version))
        .route("/api/file", get(api_file))
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

    if open_browser {
        if let Err(e) = open::that(&url) {
            eprintln!("{} Failed to open browser: {}", "!".yellow(), e);
        }
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
