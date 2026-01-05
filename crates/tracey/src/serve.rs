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

// Markdown rendering
use bearmark::{
    AasvgHandler, ArboriumHandler, PikruHandler, RenderOptions, RuleDefinition, RuleHandler,
    parse_frontmatter, render,
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
    /// Syntax-highlighted HTML content
    html: String,
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

/// A section of a spec (one source file)
#[derive(Debug, Clone)]
struct SpecSection {
    /// Source file path
    source_file: String,
    /// Rendered HTML content
    html: String,
    /// Weight for ordering (from frontmatter)
    weight: i32,
}

/// Coverage counts for an outline entry
#[derive(Debug, Clone, Default)]
struct OutlineCoverage {
    /// Number of rules with implementation refs
    impl_count: usize,
    /// Number of rules with verification refs
    verify_count: usize,
    /// Total number of rules
    total: usize,
}

/// An entry in the spec outline (heading with coverage info)
#[derive(Debug, Clone)]
struct OutlineEntry {
    /// Heading text
    title: String,
    /// Slug for linking
    slug: String,
    /// Heading level (1-6)
    level: u8,
    /// Direct coverage (rules directly under this heading)
    coverage: OutlineCoverage,
    /// Aggregated coverage (includes all nested rules)
    aggregated: OutlineCoverage,
}

/// Spec content (may span multiple files)
#[derive(Debug, Clone)]
struct ApiSpecData {
    name: String,
    /// Sections ordered by weight
    sections: Vec<SpecSection>,
    /// Outline with coverage info
    outline: Vec<OutlineEntry>,
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
    /// Syntax highlighter for source files
    highlighter: Arc<Mutex<arborium::Highlighter>>,
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
            r#"{{"path":{},"content":{},"html":{},"units":[{}]}}"#,
            json_string(&self.path),
            json_string(&self.content),
            json_string(&self.html),
            units.join(",")
        )
    }
}

impl SpecSection {
    fn to_json(&self) -> String {
        format!(
            r#"{{"sourceFile":{},"html":{}}}"#,
            json_string(&self.source_file),
            json_string(&self.html)
        )
    }
}

impl OutlineCoverage {
    fn to_json(&self) -> String {
        format!(
            r#"{{"implCount":{},"verifyCount":{},"total":{}}}"#,
            self.impl_count, self.verify_count, self.total
        )
    }
}

impl OutlineEntry {
    fn to_json(&self) -> String {
        format!(
            r#"{{"title":{},"slug":{},"level":{},"coverage":{},"aggregated":{}}}"#,
            json_string(&self.title),
            json_string(&self.slug),
            self.level,
            self.coverage.to_json(),
            self.aggregated.to_json()
        )
    }
}

impl ApiSpecData {
    fn to_json(&self) -> String {
        let sections: Vec<String> = self.sections.iter().map(|s| s.to_json()).collect();
        let outline: Vec<String> = self.outline.iter().map(|o| o.to_json()).collect();
        format!(
            r#"{{"name":{},"sections":[{}],"outline":[{}]}}"#,
            json_string(&self.name),
            sections.join(","),
            outline.join(",")
        )
    }
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
}

impl TraceyRuleHandler {
    fn new(
        coverage: BTreeMap<String, RuleCoverage>,
        current_source_file: Arc<Mutex<String>>,
    ) -> Self {
        Self {
            coverage,
            current_source_file,
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

impl RuleHandler for TraceyRuleHandler {
    fn start<'a>(
        &'a self,
        rule: &'a RuleDefinition,
    ) -> Pin<Box<dyn Future<Output = bearmark::Result<String>> + Send + 'a>> {
        Box::pin(async move {
            let coverage = self.coverage.get(&rule.id);
            let status = coverage.map(|c| c.status).unwrap_or("uncovered");

            // Insert <wbr> after dots for better line breaking
            let display_id = rule.id.replace('.', ".<wbr>");

            // Get current source file for this rule
            let source_file = self.current_source_file.lock().unwrap().clone();

            // Build the badges that pierce the top border
            let mut badges_html = String::new();

            // Rule ID badge (always present) - includes source location for editor navigation
            badges_html.push_str(&format!(
                r#"<a class="rule-badge rule-id" href="/spec/{}" data-rule="{}" data-source-file="{}" data-source-line="{}" title="{}">{}</a>"#,
                rule.id, rule.id, source_file, rule.line, rule.id, display_id
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
                    badges_html.push_str(&format!(
                        r#"<a class="rule-badge rule-impl" href="/sources/{}:{}" data-file="{}" data-line="{}" data-all-refs="{}" title="Implementation: {}:{}">{icon}{}:{}{}</a>"#,
                        r.file, r.line, r.file, r.line, all_refs_json, r.file, r.line, filename, r.line, count_suffix
                    ));
                }

                // Test/verify badge
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
                        r#"<a class="rule-badge rule-test" href="/sources/{}:{}" data-file="{}" data-line="{}" data-all-refs="{}" title="Test: {}:{}">{icon}{}:{}{}</a>"#,
                        r.file, r.line, r.file, r.line, all_refs_json, r.file, r.line, filename, r.line, count_suffix
                    ));
                }
            }

            // Render the opening of the rule container
            Ok(format!(
                r#"<div class="rule-container rule-{status}" id="{anchor}">
<div class="rule-badges">{badges}</div>
<div class="rule-content">"#,
                status = status,
                anchor = rule.anchor_id,
                badges = badges_html,
            ))
        })
    }

    fn end<'a>(
        &'a self,
        _rule: &'a RuleDefinition,
    ) -> Pin<Box<dyn Future<Output = bearmark::Result<String>> + Send + 'a>> {
        Box::pin(async move {
            // Close the rule container
            Ok("</div>\n</div>".to_string())
        })
    }
}

// ============================================================================
// Data Building
// ============================================================================

async fn build_dashboard_data(
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
            crate::load_manifest_from_glob(project_root, &glob.pattern)?
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

        // Build coverage map for this spec's rules
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

        // Load spec content with coverage-aware rendering (only for rules_glob sources)
        if let Some(glob) = &spec_config.rules_glob {
            load_spec_content(
                project_root,
                &glob.pattern,
                spec_name,
                &coverage,
                &mut specs_content,
            )
            .await?;
        }

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

async fn load_spec_content(
    root: &Path,
    pattern: &str,
    spec_name: &str,
    coverage: &BTreeMap<String, RuleCoverage>,
    specs_content: &mut BTreeMap<String, ApiSpecData>,
) -> Result<()> {
    use ignore::WalkBuilder;

    // Shared source file tracker for rule handler
    let current_source_file = Arc::new(Mutex::new(String::new()));

    // Set up bearmark handlers for consistent rendering with coverage-aware rule rendering
    let rule_handler = TraceyRuleHandler::new(coverage.clone(), Arc::clone(&current_source_file));
    let opts = RenderOptions::new()
        .with_default_handler(ArboriumHandler::new())
        .with_handler(&["aasvg"], AasvgHandler::new())
        .with_handler(&["pikchr"], PikruHandler::new())
        .with_rule_handler(rule_handler);

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

        if !glob_match(&relative_str, pattern) {
            continue;
        }

        if let Ok(content) = std::fs::read_to_string(path) {
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
    *current_source_file.lock().unwrap() = first_source_file.clone();
    let opts = opts.with_source_path(&first_source_file);
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
    elements: &[bearmark::DocElement],
    coverage: &BTreeMap<String, RuleCoverage>,
) -> Vec<OutlineEntry> {
    use bearmark::DocElement;

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
            DocElement::Rule(r) => {
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
    let data = state.data.borrow().clone();

    if let Some(units) = data.code_units_by_file.get(&full_path) {
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
                rule_refs: u.rule_refs.clone(),
            })
            .collect();

        let file_data = ApiFileData {
            path: relative,
            content,
            html,
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
    let config_path = config_path.unwrap_or_else(|| project_root.join(".config/tracey/config.kdl"));
    let config = crate::load_config(&config_path)?;

    let version = Arc::new(AtomicU64::new(1));

    // Initial build
    let initial_data = build_dashboard_data(&project_root, &config_path, &config, 1).await?;

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
                        if let std::path::Component::Normal(name) = component
                            && let Some(name_str) = name.to_str()
                            && ignored_paths.contains(&name_str)
                        {
                            return true;
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
            match build_dashboard_data(&rebuild_project_root, &rebuild_config_path, &config, 0)
                .await
            {
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
        highlighter: Arc::new(Mutex::new(arborium::Highlighter::new())),
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
