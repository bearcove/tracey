//! Dashboard data building.
//!
//! This module contains the core data structures and functions for building
//! the `DashboardData` that powers the tracey dashboard, MCP, and LSP.

#![allow(dead_code)]

use eyre::Result;
use owo_colors::OwoColorize;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use tracey_core::code_units::CodeUnit;
use tracey_core::is_supported_extension;
use tracey_core::{RefVerb, ReqDefinition, Reqs};

// Markdown rendering
use marq::{
    AasvgHandler, ArboriumHandler, InlineCodeHandler, PikruHandler, RenderOptions, ReqHandler,
    parse_frontmatter, render,
};

use crate::config::Config;
use crate::search::{self, SearchIndex};

// ============================================================================
// JSON API Types
// ============================================================================

// Re-export API types from tracey-api crate
pub use tracey_api::{
    ApiCodeRef, ApiCodeUnit, ApiConfig, ApiFileData, ApiFileEntry, ApiForwardData, ApiReverseData,
    ApiRule, ApiSpecData, ApiSpecForward, ApiSpecInfo, GitStatus, OutlineCoverage, OutlineEntry,
    SpecSection,
};

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

        // Match r[rule.id] pattern
        if !code.starts_with("r[") || !code.ends_with(']') {
            return None;
        }

        let rule_id = &code[2..code.len() - 1]; // Extract rule.id from r[rule.id]

        // Validate it looks like a rule ID (alphanumeric, dots, dashes, underscores)
        if rule_id.is_empty()
            || !rule_id
                .chars()
                .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
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

    for spec_config in &config.specs {
        let spec_name = &spec_config.name;
        let include_patterns: Vec<&str> = spec_config.include.iter().map(|i| i.as_str()).collect();

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
                spec_config.prefix
            ));
        }

        api_config.specs.push(ApiSpecInfo {
            name: spec_name.clone(),
            prefix: spec_config.prefix.clone(),
            source: Some(include_patterns.join(", ")),
            source_url: spec_config.source_url.clone(),
            implementations: spec_config.impls.iter().map(|i| i.name.clone()).collect(),
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
            let impl_name = &impl_config.name;
            let impl_key: ImplKey = (spec_name.clone(), impl_name.clone());

            if !quiet {
                eprintln!("   {} {} implementation", "Scanning".green(), impl_name);
            }

            // Get include/exclude patterns for this impl
            // r[impl walk.default-include] - default to **/*.rs when no include patterns
            let include: Vec<String> = if impl_config.include.is_empty() {
                vec!["**/*.rs".to_string()]
            } else {
                impl_config.include.to_vec()
            };
            let exclude: Vec<String> = impl_config.exclude.to_vec();

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
                    if r.prefix == spec_config.prefix && r.req_id == extracted.def.id {
                        // r[impl ref.cross-workspace.graceful]
                        // Canonicalize the reference file path for consistent matching
                        // Uses unwrap_or_else to gracefully handle missing files
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
                });
            }

            // Sort rules by ID
            api_rules.sort_by(|a, b| a.id.cmp(&b.id));

            // Collect rules for search index (deduplicated later)
            for r in &api_rules {
                all_search_rules.push(search::RuleEntry {
                    id: r.id.clone(),
                    raw: r.raw.clone(),
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
        .with_default_handler(ArboriumHandler::new())
        .with_handler(&["aasvg"], AasvgHandler::new())
        .with_handler(&["pikchr"], PikruHandler::new())
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
pub fn glob_match(path: &str, pattern: &str) -> bool {
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
