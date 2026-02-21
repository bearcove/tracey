//! TraceyDaemon service implementation.
//!
//! Implements the roam RPC service by delegating to the Engine.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracey_core::{RuleId, RuleIdMatch, classify_reference_for_rule, parse_rule_id};
use tracey_proto::*;

use super::engine::Engine;
use super::watcher::WatcherState;
use crate::server::QueryEngine;
use roam::{Context, Tx};

// Re-export the generated dispatcher from tracey-proto
pub use tracey_proto::TraceyDaemonDispatcher;

const STALE_IMPLEMENTATION_MUST_CHANGE_PREFIX: &str = "Implementation must be changed to match updated rule text — and ONLY ONCE THAT'S DONE must the code annotation be bumped";

#[derive(Debug, Clone)]
struct HistoricalRuleText {
    commit: String,
    text: String,
}

/// Inner service state shared via Arc.
struct TraceyServiceInner {
    engine: Arc<Engine>,
    /// Syntax highlighter for source files
    highlighter: Mutex<arborium::Highlighter>,
    /// Watcher state for health monitoring
    watcher_state: Option<Arc<WatcherState>>,
    /// Start time for uptime calculation
    start_time: Instant,
    /// Shutdown signal sender
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

/// Service implementation wrapping the Engine.
///
/// This is a cheap-to-clone handle that wraps the inner state in an Arc.
#[derive(Clone)]
pub struct TraceyService {
    inner: Arc<TraceyServiceInner>,
}

impl TraceyService {
    /// Create a new service wrapping the given engine.
    pub fn new(engine: Arc<Engine>) -> Self {
        let (shutdown_tx, _) = tokio::sync::watch::channel(false);
        Self {
            inner: Arc::new(TraceyServiceInner {
                engine,
                highlighter: Mutex::new(arborium::Highlighter::new()),
                watcher_state: None,
                start_time: Instant::now(),
                shutdown_tx,
            }),
        }
    }

    /// Create a new service with watcher state for health monitoring.
    /// Returns the service and a shutdown receiver that signals when shutdown is requested.
    pub fn new_with_watcher(
        engine: Arc<Engine>,
        watcher_state: Arc<WatcherState>,
    ) -> (Self, tokio::sync::watch::Receiver<bool>) {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let service = Self {
            inner: Arc::new(TraceyServiceInner {
                engine,
                highlighter: Mutex::new(arborium::Highlighter::new()),
                watcher_state: Some(watcher_state),
                start_time: Instant::now(),
                shutdown_tx,
            }),
        };
        (service, shutdown_rx)
    }

    /// Set the watcher state (for lazy initialization).
    ///
    /// Note: This requires exclusive access to the inner state. If the Arc
    /// has been cloned, this will fail silently (watcher state won't be set).
    pub fn set_watcher_state(&mut self, state: Arc<WatcherState>) {
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            inner.watcher_state = Some(state);
        }
    }

    // Helper: resolve spec/impl from optional parameters
    fn resolve_spec_impl(
        &self,
        spec: Option<&str>,
        impl_name: Option<&str>,
        config: &ApiConfig,
    ) -> (String, String) {
        // If spec not provided, use first spec
        let spec_name = spec.map(String::from).unwrap_or_else(|| {
            config
                .specs
                .first()
                .map(|s| s.name.clone())
                .unwrap_or_default()
        });

        // If impl not provided, use first impl for that spec
        let impl_name = impl_name.map(String::from).unwrap_or_else(|| {
            config
                .specs
                .iter()
                .find(|s| s.name == spec_name)
                .and_then(|s| s.implementations.first().cloned())
                .unwrap_or_default()
        });

        (spec_name, impl_name)
    }
}

/// Escape HTML special characters.
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

/// Get arborium language name from file extension.
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

/// Implementation of the TraceyDaemon trait.
impl TraceyDaemon for TraceyService {
    /// Get coverage status for all specs/impls
    async fn status(&self, _cx: &Context) -> StatusResponse {
        let data = self.inner.engine.data().await;
        let query = QueryEngine::new(&data);
        let stats = query.status();

        StatusResponse {
            impls: stats
                .into_iter()
                .map(|(spec, impl_name, s)| ImplStatus {
                    spec,
                    impl_name,
                    total_rules: s.total_rules,
                    impl_rules: s.impl_covered,
                    verify_rules: s.verify_covered,
                    stale_rules: s.stale_covered,
                })
                .collect(),
        }
    }

    /// Get uncovered rules
    async fn uncovered(&self, _cx: &Context, req: UncoveredRequest) -> UncoveredResponse {
        let data = self.inner.engine.data().await;
        let query = QueryEngine::new(&data);

        // Find the spec/impl to query
        let (spec, impl_name) =
            self.resolve_spec_impl(req.spec.as_deref(), req.impl_name.as_deref(), &data.config);

        if let Some(result) = query.uncovered(&spec, &impl_name, req.prefix.as_deref()) {
            UncoveredResponse {
                spec: result.spec,
                impl_name: result.impl_name,
                total_rules: result.stats.total_rules,
                missing_impl_count: result.total_uncovered,
                by_section: result
                    .by_section
                    .into_iter()
                    .map(|(section, rules)| SectionRules {
                        section,
                        rules: rules
                            .into_iter()
                            .map(|r| tracey_proto::RuleRef {
                                id: r.id,
                                text: None, // RuleRef in server.rs doesn't have text
                            })
                            .collect(),
                    })
                    .collect(),
            }
        } else {
            UncoveredResponse {
                spec,
                impl_name,
                total_rules: 0,
                missing_impl_count: 0,
                by_section: vec![],
            }
        }
    }

    /// Get untested rules
    async fn untested(&self, _cx: &Context, req: UntestedRequest) -> UntestedResponse {
        let data = self.inner.engine.data().await;
        let query = QueryEngine::new(&data);

        let (spec, impl_name) =
            self.resolve_spec_impl(req.spec.as_deref(), req.impl_name.as_deref(), &data.config);

        if let Some(result) = query.untested(&spec, &impl_name, req.prefix.as_deref()) {
            UntestedResponse {
                spec: result.spec,
                impl_name: result.impl_name,
                total_rules: result.stats.total_rules,
                missing_verify_count: result.total_untested,
                by_section: result
                    .by_section
                    .into_iter()
                    .map(|(section, rules)| SectionRules {
                        section,
                        rules: rules
                            .into_iter()
                            .map(|r| tracey_proto::RuleRef {
                                id: r.id,
                                text: None,
                            })
                            .collect(),
                    })
                    .collect(),
            }
        } else {
            UntestedResponse {
                spec,
                impl_name,
                total_rules: 0,
                missing_verify_count: 0,
                by_section: vec![],
            }
        }
    }

    /// Get unmapped code
    async fn unmapped(&self, _cx: &Context, req: UnmappedRequest) -> UnmappedResponse {
        let data = self.inner.engine.data().await;
        let query = QueryEngine::new(&data);

        let (spec, impl_name) =
            self.resolve_spec_impl(req.spec.as_deref(), req.impl_name.as_deref(), &data.config);

        if let Some(result) = query.unmapped(&spec, &impl_name, req.path.as_deref()) {
            // Convert tree nodes to flat entries
            let mut entries = Vec::new();
            fn flatten_tree(node: &crate::server::FileTreeNode, entries: &mut Vec<UnmappedEntry>) {
                entries.push(UnmappedEntry {
                    path: node.path.clone(),
                    is_dir: node.is_dir,
                    total_units: node.total_units,
                    unmapped_units: node.total_units.saturating_sub(node.covered_units),
                    units: vec![], // Tree nodes don't have unit details
                });
                for child in &node.children {
                    flatten_tree(child, entries);
                }
            }
            for node in &result.tree {
                flatten_tree(node, &mut entries);
            }

            // If we have file details, add those units
            if let Some(details) = &result.file_details {
                // Find the entry for this file and update its units
                if let Some(entry) = entries.iter_mut().find(|e| e.path == details.path) {
                    entry.units = details
                        .units
                        .iter()
                        .filter(|u| !u.is_covered)
                        .map(|u| UnmappedUnit {
                            kind: u.kind.clone(),
                            name: u.name.clone(),
                            start_line: u.start_line,
                            end_line: u.end_line,
                        })
                        .collect();
                }
            }

            UnmappedResponse {
                spec: result.spec,
                impl_name: result.impl_name,
                total_units: result.total_units,
                unmapped_count: result.total_units.saturating_sub(result.covered_units),
                entries,
            }
        } else {
            UnmappedResponse {
                spec,
                impl_name,
                total_units: 0,
                unmapped_count: 0,
                entries: vec![],
            }
        }
    }

    /// Get details for a specific rule
    async fn rule(&self, _cx: &Context, rule_id: RuleId) -> Option<RuleInfo> {
        let data = self.inner.engine.data().await;
        let query = QueryEngine::new(&data);

        query.rule(&rule_id).map(|info| RuleInfo {
            id: info.id,
            raw: info.raw,
            html: info.html,
            source_file: info.source_file,
            source_line: info.source_line,
            coverage: info
                .coverage
                .into_iter()
                .map(|c| RuleCoverage {
                    spec: c.spec,
                    impl_name: c.impl_name,
                    impl_refs: c.impl_refs,
                    verify_refs: c.verify_refs,
                })
                .collect(),
        })
    }

    /// Get current configuration
    async fn config(&self, _cx: &Context) -> ApiConfig {
        let data = self.inner.engine.data().await;
        data.config.clone()
    }

    /// VFS: file opened
    async fn vfs_open(&self, _cx: &Context, path: String, content: String) {
        self.inner
            .engine
            .vfs_open(std::path::PathBuf::from(path), content)
            .await;
    }

    /// VFS: file changed
    async fn vfs_change(&self, _cx: &Context, path: String, content: String) {
        self.inner
            .engine
            .vfs_change(std::path::PathBuf::from(path), content)
            .await;
    }

    /// VFS: file closed
    async fn vfs_close(&self, _cx: &Context, path: String) {
        self.inner
            .engine
            .vfs_close(std::path::PathBuf::from(path))
            .await;
    }

    /// Force a rebuild
    async fn reload(&self, _cx: &Context) -> ReloadResponse {
        match self.inner.engine.rebuild().await {
            Ok((version, duration)) => ReloadResponse {
                version,
                rebuild_time_ms: duration.as_millis() as u64,
            },
            Err(e) => {
                tracing::error!("Reload failed: {}", e);
                ReloadResponse {
                    version: self.inner.engine.version(),
                    rebuild_time_ms: 0,
                }
            }
        }
    }

    /// Get current version
    async fn version(&self, _cx: &Context) -> u64 {
        self.inner.engine.version()
    }

    /// Get daemon health status
    async fn health(&self, _cx: &Context) -> HealthResponse {
        let version = self.inner.engine.version();
        let uptime_secs = self.inner.start_time.elapsed().as_secs();

        // Get config error if any
        let config_error = self.inner.engine.config_error().await;

        // Get watcher state if available
        let (
            watcher_active,
            watcher_error,
            watcher_last_event_ms,
            watcher_event_count,
            watched_directories,
        ) = if let Some(ref state) = self.inner.watcher_state {
            (
                state.is_active(),
                state.error(),
                state.last_event_ms(),
                state.event_count(),
                state
                    .watched_dirs()
                    .into_iter()
                    .map(|p| p.display().to_string())
                    .collect(),
            )
        } else {
            // No watcher state - return defaults
            (false, None, None, 0, vec![])
        };

        HealthResponse {
            version,
            watcher_active,
            watcher_error,
            config_error,
            watcher_last_event_ms,
            watcher_event_count,
            watched_directories,
            uptime_secs,
        }
    }

    /// Request the daemon to shut down gracefully
    async fn shutdown(&self, _cx: &Context) {
        tracing::info!("Shutdown requested via RPC");
        let _ = self.inner.shutdown_tx.send(true);
    }

    /// Subscribe to data updates
    async fn subscribe(&self, _cx: &Context, updates: Tx<DataUpdate>) {
        // Get a watch receiver from the engine
        let mut rx = self.inner.engine.subscribe();

        // Loop until the client disconnects or an error occurs
        loop {
            // Wait for a change in the data
            if rx.changed().await.is_err() {
                // Engine dropped the sender - shutting down
                break;
            }

            // Build the update message (clone to avoid holding the guard across await)
            let update = {
                let data = rx.borrow_and_update();

                // Convert server::Delta to proto::DeltaSummary
                // Flatten all impl deltas into a single summary
                let delta = if data.delta.is_empty() {
                    None
                } else {
                    let mut newly_covered = Vec::new();
                    let mut newly_uncovered = Vec::new();

                    for impl_delta in data.delta.by_impl.values() {
                        for change in &impl_delta.newly_covered {
                            newly_covered.push(CoverageChange {
                                rule_id: change.rule_id.clone(),
                                file: change.file.clone(),
                                line: change.line,
                            });
                        }
                        newly_uncovered.extend(impl_delta.newly_uncovered.iter().cloned());
                    }

                    Some(DeltaSummary {
                        newly_covered,
                        newly_uncovered,
                    })
                };

                DataUpdate {
                    version: data.version,
                    delta,
                }
            }; // Guard dropped here before the await

            // Send the update - if this fails, the client disconnected
            if updates.send(&update).await.is_err() {
                break;
            }
        }
    }

    /// Get forward traceability data
    async fn forward(
        &self,
        _cx: &Context,
        spec: String,
        impl_name: String,
    ) -> Option<ApiSpecForward> {
        let data = self.inner.engine.data().await;
        data.forward_by_impl.get(&(spec, impl_name)).cloned()
    }

    /// Get reverse traceability data
    async fn reverse(
        &self,
        _cx: &Context,
        spec: String,
        impl_name: String,
    ) -> Option<ApiReverseData> {
        let data = self.inner.engine.data().await;
        data.reverse_by_impl.get(&(spec, impl_name)).cloned()
    }

    /// Get file with syntax highlighting
    async fn file(&self, _cx: &Context, req: FileRequest) -> Option<ApiFileData> {
        let data = self.inner.engine.data().await;
        let project_root = self.inner.engine.project_root();

        let impl_key = (req.spec, req.impl_name);

        // Get the code units map for this impl
        let code_units_by_file = data.code_units_by_impl.get(&impl_key)?;

        // Resolve the file path - it may be relative or absolute
        let file_path = PathBuf::from(&req.path);
        let full_path = if file_path.is_absolute() {
            file_path
        } else {
            project_root.join(&file_path)
        };
        // Canonicalize to handle cross-workspace paths like ../marq/...
        let full_path = full_path.canonicalize().unwrap_or(full_path);

        // Look up code units for this file
        let units = code_units_by_file.get(&full_path)?;

        // Read file content
        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => return None,
        };

        // Get relative path for display
        let relative = full_path
            .strip_prefix(project_root)
            .unwrap_or(&full_path)
            .display()
            .to_string();

        // Syntax highlight the content
        let html = if let Some(lang) = arborium_language(&relative) {
            let mut hl = self.inner.highlighter.lock().unwrap();
            match hl.highlight(lang, &content) {
                Ok(highlighted) => highlighted,
                Err(_) => html_escape(&content),
            }
        } else {
            html_escape(&content)
        };

        // Convert code units to API format
        let api_units: Vec<ApiCodeUnit> = units
            .iter()
            .map(|u| ApiCodeUnit {
                kind: format!("{:?}", u.kind).to_lowercase(),
                name: u.name.clone(),
                start_line: u.start_line,
                end_line: u.end_line,
                rule_refs: u.req_refs.iter().map(|r| r.to_string()).collect(),
            })
            .collect();

        Some(ApiFileData {
            path: relative,
            content,
            html,
            units: api_units,
        })
    }

    /// Get rendered spec content
    async fn spec_content(
        &self,
        _cx: &Context,
        spec: String,
        impl_name: String,
    ) -> Option<ApiSpecData> {
        let data = self.inner.engine.data().await;
        data.specs_content_by_impl.get(&(spec, impl_name)).cloned()
    }

    /// Search rules and files
    async fn search(&self, _cx: &Context, query: String, limit: u32) -> Vec<SearchResult> {
        let data = self.inner.engine.data().await;
        let raw_results: Vec<_> = data
            .search_index
            .search(&query, limit as usize)
            .into_iter()
            .collect();

        let mut results = Vec::with_capacity(raw_results.len());
        for r in raw_results {
            use crate::search::ResultKind;
            let kind = match r.kind {
                ResultKind::Rule => "rule",
                ResultKind::Source => "source",
            };

            // For rules, render the markdown snippet to HTML
            let highlighted = if r.kind == ResultKind::Rule {
                let opts = marq::RenderOptions::default();
                match marq::render(&r.highlighted, &opts).await {
                    Ok(doc) => doc.html,
                    Err(_) => r.highlighted.clone(),
                }
            } else {
                r.highlighted.clone()
            };

            results.push(SearchResult {
                kind: kind.to_string(),
                id: r.id,
                line: r.line,
                content: Some(r.content),
                highlighted: Some(highlighted),
                score: r.score,
            });
        }

        results
    }

    /// Update a file range
    async fn update_file_range(
        &self,
        _cx: &Context,
        req: UpdateFileRangeRequest,
    ) -> Result<(), UpdateError> {
        let project_root = self.inner.engine.project_root();

        // Resolve the file path
        let file_path = PathBuf::from(&req.path);
        let full_path = if file_path.is_absolute() {
            file_path
        } else {
            project_root.join(&file_path)
        };

        // Read current file content
        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => {
                return Err(UpdateError {
                    message: format!("Failed to read file: {}", e),
                });
            }
        };

        // Compute hash and compare
        let current_hash = blake3::hash(content.as_bytes()).to_hex().to_string();
        if current_hash != req.file_hash {
            return Err(UpdateError {
                message: format!(
                    "File has been modified (expected hash {}, got {})",
                    req.file_hash, current_hash
                ),
            });
        }

        // Validate range
        if req.start > req.end || req.end > content.len() {
            return Err(UpdateError {
                message: format!(
                    "Invalid range: {}..{} (file length: {})",
                    req.start,
                    req.end,
                    content.len()
                ),
            });
        }

        // Replace the range
        let mut new_content =
            String::with_capacity(content.len() - (req.end - req.start) + req.content.len());
        new_content.push_str(&content[..req.start]);
        new_content.push_str(&req.content);
        new_content.push_str(&content[req.end..]);

        // Write back
        if let Err(e) = std::fs::write(&full_path, &new_content) {
            return Err(UpdateError {
                message: format!("Failed to write file: {}", e),
            });
        }

        Ok(())
    }

    /// Check if a path is a test file
    async fn is_test_file(&self, _cx: &Context, path: String) -> bool {
        let data = self.inner.engine.data().await;
        let path = std::path::PathBuf::from(path);
        data.test_files.contains(&path)
    }

    /// Validate the spec and implementation
    ///
    /// r[impl mcp.validation.check]
    async fn validate(&self, _cx: &Context, req: ValidateRequest) -> ValidationResult {
        let data = self.inner.engine.data().await;
        let project_root = self.inner.engine.project_root();

        let (spec, impl_name) =
            self.resolve_spec_impl(req.spec.as_deref(), req.impl_name.as_deref(), &data.config);

        let mut errors = Vec::new();

        // Get all rules for this spec/impl
        if let Some(forward_data) = data.forward_by_impl.get(&(spec.clone(), impl_name.clone())) {
            // Build a list of rule IDs for match classification.
            let known_rule_ids: Vec<RuleId> =
                forward_data.rules.iter().map(|r| r.id.clone()).collect();
            let rules_by_id: std::collections::HashMap<RuleId, &ApiRule> = forward_data
                .rules
                .iter()
                .map(|rule| (rule.id.clone(), rule))
                .collect();
            let mut stale_message_cache: std::collections::HashMap<
                (RuleId, RuleId),
                Option<HistoricalRuleText>,
            > = std::collections::HashMap::new();

            // r[impl config.multi-spec.unique-within-spec]
            // Check for duplicate rule IDs and duplicate bases (within this spec)
            let mut seen_ids: std::collections::HashMap<RuleId, (&Option<String>, Option<usize>)> =
                std::collections::HashMap::new();
            let mut seen_bases: std::collections::HashMap<
                String,
                (&RuleId, &Option<String>, Option<usize>),
            > = std::collections::HashMap::new();
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
                    });
                } else {
                    seen_bases.insert(
                        rule.id.base.clone(),
                        (&rule.id, &rule.source_file, rule.source_line),
                    );
                }
            }

            // Check each rule
            for rule in &forward_data.rules {
                // Check naming convention (dot-separated segments)
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
                    });
                }

                // r[impl config.impl.test_include.verify-only]
                // Check that impl references are not in test files
                for impl_ref in &rule.impl_refs {
                    let ref_path = project_root.join(&impl_ref.file);
                    if data.test_files.contains(&ref_path) {
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
                        });
                    }
                }

                // Check depends references exist
                for dep_ref in &rule.depends_refs {
                    // Extract rule ID from the file path (this is a simplification)
                    // In a full implementation, we'd track what rule ID each depends ref points to
                    // For now, we just note that depends references exist
                    let _ = dep_ref;
                }
            }

            // r[impl ref.prefix.unknown+2]
            // Check for references with unknown prefixes
            // This requires checking the reverse data for any files that have
            // references to rules not in the rule_ids set
            if let Some(reverse_data) = data.reverse_by_impl.get(&(spec.clone(), impl_name.clone()))
            {
                // Get all prefixes from the config
                let known_prefixes: std::collections::HashSet<&str> = data
                    .config
                    .specs
                    .iter()
                    .map(|s| s.prefix.as_str())
                    .collect();

                // r[impl ref.prefix.filter+2]
                // Find the prefix for the current spec being validated
                let current_spec_prefix: Option<&str> = data
                    .config
                    .specs
                    .iter()
                    .find(|s| s.name == spec)
                    .map(|s| s.prefix.as_str());
                // r[impl config.multi-spec.prefix-namespace+2]
                let known_rule_ids_for_prefix: Vec<RuleId> =
                    if let Some(prefix) = current_spec_prefix {
                        let spec_names: std::collections::HashSet<&str> = data
                            .config
                            .specs
                            .iter()
                            .filter(|s| s.prefix == prefix)
                            .map(|s| s.name.as_str())
                            .collect();

                        data.forward_by_impl
                            .iter()
                            .filter(|((spec_name, _), _)| spec_names.contains(spec_name.as_str()))
                            .flat_map(|(_, forward)| forward.rules.iter().map(|r| r.id.clone()))
                            .collect()
                    } else {
                        Vec::new()
                    };

                // Check files for unknown references
                for file_entry in &reverse_data.files {
                    let file_path = project_root.join(&file_entry.path);
                    if let Ok(content) = std::fs::read_to_string(&file_path) {
                        let reqs = tracey_core::Reqs::extract_from_content(&file_path, &content);
                        for reference in &reqs.references {
                            // Check if prefix is known
                            if !known_prefixes.contains(reference.prefix.as_str()) {
                                let mut available: Vec<_> =
                                    known_prefixes.iter().copied().collect();
                                available.sort_unstable();
                                errors.push(ValidationError {
                                    code: ValidationErrorCode::UnknownPrefix,
                                    message: format!(
                                        "Unknown prefix '{}' - available prefixes: {}",
                                        reference.prefix,
                                        available.join(", ")
                                    ),
                                    file: Some(file_entry.path.clone()),
                                    line: Some(reference.line),
                                    column: None,
                                    related_rules: vec![],
                                });
                            }
                            // r[impl ref.prefix.filter+2]
                            // Only validate references whose prefix matches the current spec
                            // Skip references that belong to a different spec (different prefix)
                            else if current_spec_prefix == Some(reference.prefix.as_str()) {
                                match classify_reference_against_known_rules(
                                    &reference.req_id,
                                    &known_rule_ids,
                                ) {
                                    KnownRuleMatch::Exact => {}
                                    KnownRuleMatch::Stale(current_rule_id) => {
                                        // r[impl validation.stale.message-prefix]
                                        let message = stale_requirement_message(
                                            project_root,
                                            &reference.req_id,
                                            rules_by_id.get(&current_rule_id).copied(),
                                            &mut stale_message_cache,
                                        )
                                        .await;
                                        errors.push(ValidationError {
                                            code: ValidationErrorCode::StaleRequirement,
                                            message,
                                            file: Some(file_entry.path.clone()),
                                            line: Some(reference.line),
                                            column: None,
                                            related_rules: vec![current_rule_id],
                                        });
                                    }
                                    KnownRuleMatch::Missing => {
                                        match classify_reference_against_known_rules(
                                            &reference.req_id,
                                            &known_rule_ids_for_prefix,
                                        ) {
                                            KnownRuleMatch::Exact | KnownRuleMatch::Stale(_) => {
                                                // Valid in another spec sharing this prefix.
                                                // It will be checked when that spec is validated.
                                            }
                                            KnownRuleMatch::Missing => {
                                                errors.push(ValidationError {
                                                    code: ValidationErrorCode::UnknownRequirement,
                                                    message: format!(
                                                        "Reference to unknown rule '{}'",
                                                        reference.req_id
                                                    ),
                                                    file: Some(file_entry.path.clone()),
                                                    line: Some(reference.line),
                                                    column: None,
                                                    related_rules: vec![],
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                            // References with different known prefixes are intentionally skipped
                            // They belong to a different spec and will be validated when that spec is checked
                        }
                    }
                }
            }

            // Check for circular dependencies
            // Build dependency graph and detect cycles
            let cycles = detect_circular_dependencies(forward_data);
            for cycle in cycles {
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
                });
            }
        }

        let error_count = errors.len();

        ValidationResult {
            spec,
            impl_name,
            errors,
            warning_count: 0,
            error_count,
        }
    }

    // =========================================================================
    // LSP Support Methods
    // =========================================================================

    /// Get hover info for a position in a file
    ///
    /// r[impl lsp.hover.prefix]
    async fn lsp_hover(&self, _cx: &Context, req: LspPositionRequest) -> Option<HoverInfo> {
        let data = self.inner.engine.data().await;
        let path = PathBuf::from(&req.path);

        // Find the rule at cursor position (works for both spec and source files)
        let rule_at_pos =
            find_rule_at_position(&path, &req.content, req.line, req.character).await?;

        // Look up the rule in our data
        let (spec_name, rule) = find_rule_in_data(&data, &rule_at_pos.req_id)?;

        // Get spec info for the prefix
        let spec_info = data.config.specs.iter().find(|s| &s.name == spec_name);
        let spec_url = spec_info.and_then(|s| s.source_url.clone());

        // Collect references
        let impl_refs: Vec<HoverRef> = rule
            .impl_refs
            .iter()
            .map(|r| HoverRef {
                file: r.file.clone(),
                line: r.line,
            })
            .collect();
        let verify_refs: Vec<HoverRef> = rule
            .verify_refs
            .iter()
            .map(|r| HoverRef {
                file: r.file.clone(),
                line: r.line,
            })
            .collect();

        let impl_count = impl_refs.len();
        let verify_count = verify_refs.len();

        // Calculate the range of the reference
        let (start_line, start_char, end_line, end_char) = span_to_range(
            &req.content,
            rule_at_pos.span_offset,
            rule_at_pos.span_length,
        );

        Some(HoverInfo {
            rule_id: rule.id.clone(),
            raw: rule.raw.clone(),
            spec_name: spec_name.clone(),
            spec_url,
            source_file: rule.source_file.clone(),
            impl_count,
            verify_count,
            impl_refs,
            verify_refs,
            range_start_line: start_line,
            range_start_char: start_char,
            range_end_line: end_line,
            range_end_char: end_char,
        })
    }

    /// Get definition location for a reference at a position
    ///
    /// r[impl lsp.goto.ref-to-def]
    async fn lsp_definition(&self, _cx: &Context, req: LspPositionRequest) -> Vec<LspLocation> {
        let data = self.inner.engine.data().await;
        let path = PathBuf::from(&req.path);

        // Find the rule at cursor position (works for both spec and source files)
        let Some(rule_at_pos) =
            find_rule_at_position(&path, &req.content, req.line, req.character).await
        else {
            return vec![];
        };

        let Some((_, rule)) = find_rule_in_data(&data, &rule_at_pos.req_id) else {
            return vec![];
        };

        // Return the definition location (where the rule is defined in the spec)
        if let (Some(file), Some(line)) = (&rule.source_file, rule.source_line) {
            vec![LspLocation {
                path: file.clone(),
                line: line.saturating_sub(1) as u32, // Convert to 0-indexed
                character: rule.source_column.unwrap_or(0) as u32,
            }]
        } else {
            vec![]
        }
    }

    /// Get implementation locations for a reference at a position
    ///
    /// r[impl lsp.impl.from-def]
    /// r[impl lsp.impl.from-ref]
    /// r[impl lsp.impl.multiple]
    async fn lsp_implementation(&self, _cx: &Context, req: LspPositionRequest) -> Vec<LspLocation> {
        let data = self.inner.engine.data().await;
        let path = PathBuf::from(&req.path);

        // Find the rule at cursor position (works for both spec and source files)
        let Some(rule_at_pos) =
            find_rule_at_position(&path, &req.content, req.line, req.character).await
        else {
            return vec![];
        };

        let Some((_, rule)) = find_rule_in_data(&data, &rule_at_pos.req_id) else {
            return vec![];
        };

        // Return all impl reference locations
        rule.impl_refs
            .iter()
            .map(|r| LspLocation {
                path: r.file.clone(),
                line: r.line.saturating_sub(1) as u32,
                character: 0,
            })
            .collect()
    }

    /// Get all references to a requirement
    ///
    /// r[impl lsp.references.from-definition]
    /// r[impl lsp.references.from-reference]
    /// r[impl lsp.references.include-type]
    async fn lsp_references(&self, _cx: &Context, req: LspReferencesRequest) -> Vec<LspLocation> {
        let data = self.inner.engine.data().await;
        let path = PathBuf::from(&req.path);

        // Find the rule at cursor position (works for both spec and source files)
        let Some(rule_at_pos) =
            find_rule_at_position(&path, &req.content, req.line, req.character).await
        else {
            return vec![];
        };

        let Some((_, rule)) = find_rule_in_data(&data, &rule_at_pos.req_id) else {
            return vec![];
        };

        let mut locations = Vec::new();

        // Include declaration (definition) if requested
        if req.include_declaration
            && let (Some(file), Some(line)) = (&rule.source_file, rule.source_line)
        {
            locations.push(LspLocation {
                path: file.clone(),
                line: line.saturating_sub(1) as u32,
                character: rule.source_column.unwrap_or(0) as u32,
            });
        }

        // Add all impl refs
        for r in &rule.impl_refs {
            locations.push(LspLocation {
                path: r.file.clone(),
                line: r.line.saturating_sub(1) as u32,
                character: 0,
            });
        }

        // Add all verify refs
        for r in &rule.verify_refs {
            locations.push(LspLocation {
                path: r.file.clone(),
                line: r.line.saturating_sub(1) as u32,
                character: 0,
            });
        }

        // Add all depends refs
        for r in &rule.depends_refs {
            locations.push(LspLocation {
                path: r.file.clone(),
                line: r.line.saturating_sub(1) as u32,
                character: 0,
            });
        }

        locations
    }

    /// Get completions for a position
    ///
    /// r[impl lsp.completions.verb]
    /// r[impl lsp.completions.req-id]
    /// r[impl lsp.completions.req-id-fuzzy]
    async fn lsp_completions(
        &self,
        _cx: &Context,
        req: LspPositionRequest,
    ) -> Vec<LspCompletionItem> {
        let data = self.inner.engine.data().await;

        // Get the text before cursor to determine completion context
        let lines: Vec<&str> = req.content.lines().collect();
        let Some(line) = lines.get(req.line as usize) else {
            return vec![];
        };

        let col = req.character as usize;
        let before_cursor = &line[..col.min(line.len())];

        // Check if we're inside a bracket pattern like r[...
        let mut completions = Vec::new();

        // Find the last prefix[ before cursor
        for prefix in &data.config.specs {
            let pattern = format!("{}[", prefix.prefix);
            if let Some(bracket_pos) = before_cursor.rfind(&pattern) {
                let after_bracket = &before_cursor[bracket_pos + pattern.len()..];

                // If we haven't closed the bracket and there's no space yet, suggest verbs
                if !after_bracket.contains(']') {
                    if !after_bracket.contains(' ') {
                        // Suggest verbs
                        for (verb, desc) in [
                            ("impl ", "Implementation of a requirement"),
                            ("verify ", "Test/verification of a requirement"),
                            ("depends ", "Dependency on another requirement"),
                            ("related ", "Related requirement"),
                        ] {
                            if verb.starts_with(after_bracket) || after_bracket.is_empty() {
                                completions.push(LspCompletionItem {
                                    label: verb.trim().to_string(),
                                    kind: "verb".to_string(),
                                    detail: Some(desc.to_string()),
                                    documentation: None,
                                    insert_text: Some(verb.to_string()),
                                });
                            }
                        }
                    }

                    // Also suggest rule IDs (after verb or directly)
                    let query = if let Some(space_pos) = after_bracket.find(' ') {
                        &after_bracket[space_pos + 1..]
                    } else {
                        after_bracket
                    };

                    // Find matching rules
                    for ((spec, _), forward_data) in &data.forward_by_impl {
                        for rule in &forward_data.rules {
                            if rule.id.base_starts_with(query) || query.is_empty() {
                                completions.push(LspCompletionItem {
                                    label: rule.id.to_string(),
                                    kind: "rule".to_string(),
                                    detail: Some(spec.clone()),
                                    documentation: Some(rule.raw.clone()),
                                    insert_text: None,
                                });
                            }
                        }
                    }
                }
                break;
            }
        }

        completions
    }

    /// Get diagnostics for a file
    ///
    /// r[impl lsp.diagnostics.orphaned]
    /// r[impl lsp.diagnostics.duplicate-definition]
    /// r[impl lsp.diagnostics.impl-in-test]
    async fn lsp_diagnostics(&self, _cx: &Context, req: LspDocumentRequest) -> Vec<LspDiagnostic> {
        let data = self.inner.engine.data().await;
        let path = PathBuf::from(&req.path);

        let mut diagnostics = Vec::new();

        // For markdown spec files, show coverage diagnostics for definitions
        if path.extension().is_some_and(|ext| ext == "md") {
            let options = marq::RenderOptions::default();
            if let Ok(doc) = marq::render(&req.content, &options).await {
                for def in &doc.reqs {
                    // Use marker_span for diagnostics (only squiggle the marker, not content)
                    let (start_line, start_char, end_line, end_char) =
                        span_to_range(&req.content, def.marker_span.offset, def.marker_span.length);

                    // Look up the rule to check coverage
                    if let Some(def_id) = parse_rule_id(&def.id.to_string())
                        && let Some((_, rule)) = find_rule_in_data(&data, &def_id)
                    {
                        let impl_count = rule.impl_refs.len();
                        let verify_count = rule.verify_refs.len();

                        if impl_count == 0 {
                            diagnostics.push(LspDiagnostic {
                                severity: "hint".to_string(),
                                code: "uncovered".to_string(),
                                message: "Requirement has no implementations".to_string(),
                                start_line,
                                start_char,
                                end_line,
                                end_char,
                            });
                        } else if verify_count == 0 {
                            diagnostics.push(LspDiagnostic {
                                severity: "hint".to_string(),
                                code: "untested".to_string(),
                                message: format!(
                                    "Requirement has {} impl but no verification",
                                    impl_count
                                ),
                                start_line,
                                start_char,
                                end_line,
                                end_char,
                            });
                        }
                    }
                }
            }
            return diagnostics;
        }

        // For source files, check references
        let reqs = tracey_core::Reqs::extract_from_content(&path, &req.content);

        // Check if this is a test file
        let is_test = data.test_files.contains(&path);

        // Build set of known prefixes
        let known_prefixes: std::collections::HashSet<_> = data
            .config
            .specs
            .iter()
            .map(|s| s.prefix.as_str())
            .collect();

        // Build known rules by prefix (using all implementations for each spec).
        let mut known_rules_by_prefix: std::collections::HashMap<&str, Vec<RuleId>> =
            std::collections::HashMap::new();
        let mut rules_by_id: std::collections::HashMap<RuleId, ApiRule> =
            std::collections::HashMap::new();
        for spec_cfg in &data.config.specs {
            let rule_ids = known_rules_by_prefix
                .entry(spec_cfg.prefix.as_str())
                .or_default();
            for ((spec_name, _), forward_data) in &data.forward_by_impl {
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
        let mut stale_message_cache: std::collections::HashMap<
            (RuleId, RuleId),
            Option<HistoricalRuleText>,
        > = std::collections::HashMap::new();
        let project_root = self.inner.engine.project_root();

        for reference in &reqs.references {
            let (start_line, start_char, end_line, end_char) =
                span_to_range(&req.content, reference.span.offset, reference.span.length);

            // Check for unknown prefix
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
                    // r[impl lsp.diagnostics.stale]
                    // r[impl lsp.diagnostics.stale.message-prefix]
                    // r[impl lsp.diagnostics.stale.diff]
                    let message = stale_requirement_message(
                        project_root,
                        &reference.req_id,
                        rules_by_id.get(&current_rule_id),
                        &mut stale_message_cache,
                    )
                    .await;
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
                    diagnostics.push(LspDiagnostic {
                        severity: "warning".to_string(),
                        code: "orphaned".to_string(),
                        message: format!("Unknown requirement: '{}'", reference.req_id),
                        start_line,
                        start_char,
                        end_line,
                        end_char,
                    });
                }
            }

            // Check for impl in test file
            if is_test && reference.verb == tracey_core::RefVerb::Impl {
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

        // Check warnings from parsing
        for warning in &reqs.warnings {
            let (start_line, start_char, end_line, end_char) =
                span_to_range(&req.content, warning.span.offset, warning.span.length);

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

        diagnostics
    }

    /// Get diagnostics for all files in the workspace
    async fn lsp_workspace_diagnostics(&self, _cx: &Context) -> Vec<LspFileDiagnostics> {
        let data = self.inner.engine.data().await;
        let project_root = self.inner.engine.project_root();
        let mut results = Vec::new();

        // Collect unique spec files from forward data
        let mut spec_files: std::collections::HashSet<String> = std::collections::HashSet::new();
        for forward_data in data.forward_by_impl.values() {
            for rule in &forward_data.rules {
                if let Some(source_file) = &rule.source_file {
                    spec_files.insert(source_file.clone());
                }
            }
        }

        // Process spec files
        for spec_file in &spec_files {
            let abs_path = project_root.join(spec_file);
            if let Ok(content) = tokio::fs::read_to_string(&abs_path).await {
                let req = LspDocumentRequest {
                    path: abs_path.to_string_lossy().to_string(),
                    content,
                };
                let diagnostics = self.lsp_diagnostics(_cx, req).await;
                if !diagnostics.is_empty() {
                    results.push(LspFileDiagnostics {
                        path: spec_file.clone(),
                        diagnostics,
                    });
                }
            }
        }

        // Collect unique implementation files from code_units
        let mut impl_files: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        for code_units_by_file in data.code_units_by_impl.values() {
            for file_path in code_units_by_file.keys() {
                impl_files.insert(file_path.clone());
            }
        }

        // Process implementation files
        for impl_file in &impl_files {
            if let Ok(content) = tokio::fs::read_to_string(impl_file).await {
                let req = LspDocumentRequest {
                    path: impl_file.to_string_lossy().to_string(),
                    content,
                };
                let diagnostics = self.lsp_diagnostics(_cx, req).await;
                if !diagnostics.is_empty() {
                    // Convert to relative path for consistency
                    let rel_path = impl_file
                        .strip_prefix(project_root)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| impl_file.to_string_lossy().to_string());
                    results.push(LspFileDiagnostics {
                        path: rel_path,
                        diagnostics,
                    });
                }
            }
        }

        results
    }

    /// Get document symbols (requirement references) in a file
    ///
    /// r[impl lsp.symbols.references]
    /// r[impl lsp.symbols.requirements]
    async fn lsp_document_symbols(&self, _cx: &Context, req: LspDocumentRequest) -> Vec<LspSymbol> {
        let path = PathBuf::from(&req.path);
        let mut symbols = Vec::new();

        // For spec files (markdown), return requirement definitions
        if path.extension().is_some_and(|ext| ext == "md") {
            let data = self.inner.engine.data().await;
            let project_root = self.inner.engine.project_root();

            // Get relative path for matching
            let relative_path = path
                .strip_prefix(project_root)
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| req.path.clone());

            // Find rules defined in this file
            for ((_, _), forward_data) in &data.forward_by_impl {
                for rule in &forward_data.rules {
                    if let Some(source_file) = &rule.source_file
                        && source_file == &relative_path
                    {
                        let line = rule.source_line.unwrap_or(1).saturating_sub(1) as u32;
                        let col = rule.source_column.unwrap_or(1).saturating_sub(1) as u32;
                        symbols.push(LspSymbol {
                            name: rule.id.to_string(),
                            kind: "requirement".to_string(),
                            start_line: line,
                            start_char: col,
                            end_line: line,
                            end_char: col + rule.id.to_string().len() as u32,
                        });
                    }
                }
            }
        } else {
            // For implementation files, extract references
            let reqs = tracey_core::Reqs::extract_from_content(&path, &req.content);
            for r in &reqs.references {
                let (start_line, start_char, end_line, end_char) =
                    span_to_range(&req.content, r.span.offset, r.span.length);
                symbols.push(LspSymbol {
                    name: r.req_id.to_string(),
                    kind: format!("{:?}", r.verb).to_lowercase(),
                    start_line,
                    start_char,
                    end_line,
                    end_char,
                });
            }
        }

        symbols
    }

    /// Search workspace for requirement IDs
    ///
    /// r[impl lsp.workspace-symbols.requirements]
    async fn lsp_workspace_symbols(&self, _cx: &Context, query: String) -> Vec<LspSymbol> {
        let data = self.inner.engine.data().await;
        let query_lower = query.to_lowercase();

        let mut symbols = Vec::new();
        for ((_, _), forward_data) in &data.forward_by_impl {
            for rule in &forward_data.rules {
                if rule.id.base.to_lowercase().contains(&query_lower) {
                    let (line, char) = if let Some(l) = rule.source_line {
                        (
                            l.saturating_sub(1) as u32,
                            rule.source_column.unwrap_or(0) as u32,
                        )
                    } else {
                        (0, 0)
                    };

                    symbols.push(LspSymbol {
                        name: rule.id.to_string(),
                        kind: "requirement".to_string(),
                        start_line: line,
                        start_char: char,
                        end_line: line,
                        end_char: char + rule.id.to_string().len() as u32,
                    });
                }
            }
        }

        symbols
    }

    /// Get semantic tokens for syntax highlighting
    ///
    /// r[impl lsp.semantic-tokens.prefix]
    /// r[impl lsp.semantic-tokens.verb]
    async fn lsp_semantic_tokens(
        &self,
        _cx: &Context,
        req: LspDocumentRequest,
    ) -> Vec<LspSemanticToken> {
        let path = PathBuf::from(&req.path);
        let data = self.inner.engine.data().await;

        // Build set of known rule IDs
        let known_rules: std::collections::HashSet<_> = data
            .forward_by_impl
            .values()
            .flat_map(|f| f.rules.iter().map(|r| r.id.clone()))
            .collect();

        let mut tokens = Vec::new();

        // For markdown spec files, tokenize requirement definitions
        if path.extension().is_some_and(|ext| ext == "md") {
            let options = marq::RenderOptions::default();
            if let Ok(doc) = marq::render(&req.content, &options).await {
                for def in &doc.reqs {
                    // Use marker_span for semantic tokens (only color the marker)
                    let (start_line, start_char, _, _) =
                        span_to_range(&req.content, def.marker_span.offset, def.marker_span.length);

                    // Definitions are always the DEFINITION modifier
                    tokens.push(LspSemanticToken {
                        line: start_line,
                        start_char,
                        length: def.marker_span.length as u32,
                        token_type: 2, // variable (req_id)
                        modifiers: 1,  // DEFINITION modifier
                    });
                }
            }
        } else {
            // For source files, tokenize references in comments
            let reqs = tracey_core::Reqs::extract_from_content(&path, &req.content);

            for reference in &reqs.references {
                let (start_line, start_char, _, _) =
                    span_to_range(&req.content, reference.span.offset, reference.span.length);

                // Token for the entire reference
                // Token type 0 = namespace (prefix), 1 = keyword (verb), 2 = variable (req_id)
                let is_valid = known_rules.contains(&reference.req_id);
                let modifier = if reference.verb == tracey_core::RefVerb::Define {
                    1 // DEFINITION modifier
                } else if is_valid {
                    2 // DECLARATION modifier
                } else {
                    0
                };

                tokens.push(LspSemanticToken {
                    line: start_line,
                    start_char,
                    length: reference.span.length as u32,
                    token_type: 2, // variable (req_id)
                    modifiers: modifier,
                });
            }
        }

        tokens
    }

    /// Get code lens items
    ///
    /// r[impl lsp.codelens.coverage]
    /// r[impl lsp.codelens.clickable]
    /// r[impl lsp.codelens.run-test]
    async fn lsp_code_lens(&self, _cx: &Context, req: LspDocumentRequest) -> Vec<LspCodeLens> {
        let data = self.inner.engine.data().await;
        let path = PathBuf::from(&req.path);

        let mut lenses = Vec::new();

        // For markdown spec files, show code lenses for requirement definitions
        if path.extension().is_some_and(|ext| ext == "md") {
            let options = marq::RenderOptions::default();
            if let Ok(doc) = marq::render(&req.content, &options).await {
                for def in &doc.reqs {
                    // Use marker_span for code lens positioning
                    let (start_line, start_char, _, end_char) =
                        span_to_range(&req.content, def.marker_span.offset, def.marker_span.length);

                    // Look up coverage for this rule
                    if let Some(def_id) = parse_rule_id(&def.id.to_string())
                        && let Some((_, rule)) = find_rule_in_data(&data, &def_id)
                    {
                        let impl_count = rule.impl_refs.len();
                        let verify_count = rule.verify_refs.len();

                        let title = if impl_count == 0 && verify_count == 0 {
                            "⚪ not implemented".to_string()
                        } else if verify_count == 0 {
                            format!("🟡 {} impl, no verify", impl_count)
                        } else {
                            format!("🟢 {} impl, {} verify", impl_count, verify_count)
                        };

                        lenses.push(LspCodeLens {
                            line: start_line,
                            start_char,
                            end_char,
                            title,
                            command: "tracey.showReferences".to_string(),
                            arguments: vec![def.id.to_string()],
                        });
                    }
                }
            }
        } else {
            // For source files, show code lenses for definition references
            let reqs = tracey_core::Reqs::extract_from_content(&path, &req.content);

            for reference in &reqs.references {
                // Only show code lens for definitions
                if reference.verb != tracey_core::RefVerb::Define {
                    continue;
                }

                let (start_line, start_char, _, end_char) =
                    span_to_range(&req.content, reference.span.offset, reference.span.length);

                // Look up coverage for this rule
                if let Some((_, rule)) = find_rule_in_data(&data, &reference.req_id) {
                    let impl_count = rule.impl_refs.len();
                    let verify_count = rule.verify_refs.len();

                    let title = if impl_count == 0 && verify_count == 0 {
                        "⚪ not implemented".to_string()
                    } else if verify_count == 0 {
                        format!("🟡 {} impl, no verify", impl_count)
                    } else {
                        format!("🟢 {} impl, {} verify", impl_count, verify_count)
                    };

                    lenses.push(LspCodeLens {
                        line: start_line,
                        start_char,
                        end_char,
                        title,
                        command: "tracey.showReferences".to_string(),
                        arguments: vec![reference.req_id.to_string()],
                    });
                }
            }
        }

        lenses
    }

    /// Get inlay hints for a range
    ///
    /// r[impl lsp.inlay.coverage-status]
    /// r[impl lsp.inlay.impl-count]
    async fn lsp_inlay_hints(&self, _cx: &Context, req: InlayHintsRequest) -> Vec<LspInlayHint> {
        let data = self.inner.engine.data().await;
        let path = PathBuf::from(&req.path);

        let mut hints = Vec::new();

        // For markdown spec files, show hints for requirement definitions
        if path.extension().is_some_and(|ext| ext == "md") {
            let options = marq::RenderOptions::default();
            if let Ok(doc) = marq::render(&req.content, &options).await {
                for def in &doc.reqs {
                    // Use marker_span for inlay hint positioning (after the marker)
                    let (line, _, _, end_char) =
                        span_to_range(&req.content, def.marker_span.offset, def.marker_span.length);

                    // Only show hints in the requested range
                    if line < req.start_line || line > req.end_line {
                        continue;
                    }

                    // Look up the rule to get impl/verify counts
                    if let Some(def_id) = parse_rule_id(&def.id.to_string())
                        && let Some((_, rule)) = find_rule_in_data(&data, &def_id)
                    {
                        let impl_count = rule.impl_refs.len();
                        let verify_count = rule.verify_refs.len();

                        let label = format!(" [{} impl, {} verify]", impl_count, verify_count);

                        hints.push(LspInlayHint {
                            line,
                            character: end_char,
                            label,
                        });
                    }
                }
            }
        } else {
            // For source files, show hints for references in comments
            let reqs = tracey_core::Reqs::extract_from_content(&path, &req.content);

            for reference in &reqs.references {
                let (line, _, _, end_char) =
                    span_to_range(&req.content, reference.span.offset, reference.span.length);

                // Only show hints in the requested range
                if line < req.start_line || line > req.end_line {
                    continue;
                }

                // Look up the rule
                if let Some((_, rule)) = find_rule_in_data(&data, &reference.req_id) {
                    let impl_count = rule.impl_refs.len();
                    let verify_count = rule.verify_refs.len();

                    let label = format!(" [{} impl, {} verify]", impl_count, verify_count);

                    hints.push(LspInlayHint {
                        line,
                        character: end_char,
                        label,
                    });
                }
            }
        }

        hints
    }

    /// Prepare rename (check if renaming is valid)
    ///
    /// r[impl lsp.rename.prepare]
    async fn lsp_prepare_rename(
        &self,
        _cx: &Context,
        req: LspPositionRequest,
    ) -> Option<PrepareRenameResult> {
        let data = self.inner.engine.data().await;
        let path = PathBuf::from(&req.path);

        // Find the rule at cursor position (works for both spec and source files)
        let rule_at_pos =
            find_rule_at_position(&path, &req.content, req.line, req.character).await?;

        // Check if the rule exists
        find_rule_in_data(&data, &rule_at_pos.req_id)?;

        // Calculate the range of just the rule ID within the reference
        // This is a simplification - we return the whole reference range
        let (start_line, start_char, end_line, end_char) = span_to_range(
            &req.content,
            rule_at_pos.span_offset,
            rule_at_pos.span_length,
        );

        Some(PrepareRenameResult {
            start_line,
            start_char,
            end_line,
            end_char,
            placeholder: rule_at_pos.req_id.to_string(),
        })
    }

    /// Execute rename
    ///
    /// r[impl lsp.rename.req-id]
    /// r[impl lsp.rename.validation]
    async fn lsp_rename(&self, _cx: &Context, req: LspRenameRequest) -> Vec<LspTextEdit> {
        let data = self.inner.engine.data().await;
        let path = PathBuf::from(&req.path);

        // Find the rule at cursor position (works for both spec and source files)
        let Some(rule_at_pos) =
            find_rule_at_position(&path, &req.content, req.line, req.character).await
        else {
            return vec![];
        };

        // Validate the new name follows naming convention
        let Some(parsed_new_name) = parse_rule_id(&req.new_name) else {
            return vec![];
        };
        if !is_valid_rule_id(&parsed_new_name) {
            return vec![];
        }

        let Some((_, rule)) = find_rule_in_data(&data, &rule_at_pos.req_id) else {
            return vec![];
        };

        let mut edits = Vec::new();

        // Edit in the definition
        if let (Some(file), Some(line)) = (&rule.source_file, rule.source_line) {
            edits.push(LspTextEdit {
                path: file.clone(),
                start_line: line.saturating_sub(1) as u32,
                start_char: rule.source_column.unwrap_or(0) as u32,
                end_line: line.saturating_sub(1) as u32,
                end_char: (rule.source_column.unwrap_or(0) + rule_at_pos.req_id.to_string().len())
                    as u32,
                new_text: req.new_name.clone(),
            });
        }

        // Edit in all impl refs
        for r in &rule.impl_refs {
            // We'd need to read these files and find the exact position
            // For now, just note the location
            edits.push(LspTextEdit {
                path: r.file.clone(),
                start_line: r.line.saturating_sub(1) as u32,
                start_char: 0, // Would need file content to calculate
                end_line: r.line.saturating_sub(1) as u32,
                end_char: 0,
                new_text: req.new_name.clone(),
            });
        }

        // Similar for verify_refs and depends_refs...

        edits
    }

    /// Get code actions for a position
    ///
    /// r[impl lsp.actions.create-requirement]
    /// r[impl lsp.actions.open-dashboard]
    async fn lsp_code_actions(&self, _cx: &Context, req: LspPositionRequest) -> Vec<LspCodeAction> {
        let data = self.inner.engine.data().await;
        let path = PathBuf::from(&req.path);

        let mut actions = Vec::new();

        // Check if we're on a rule (works for both spec and source files)
        if let Some(rule_at_pos) =
            find_rule_at_position(&path, &req.content, req.line, req.character).await
        {
            // Check if it's an orphaned reference
            if find_rule_in_data(&data, &rule_at_pos.req_id).is_none() {
                actions.push(LspCodeAction {
                    title: format!("Create requirement '{}'", rule_at_pos.req_id),
                    kind: "quickfix".to_string(),
                    command: "tracey.createRequirement".to_string(),
                    arguments: vec![rule_at_pos.req_id.to_string()],
                    is_preferred: true,
                });
            } else {
                // Open dashboard for this requirement
                actions.push(LspCodeAction {
                    title: "Open in dashboard".to_string(),
                    kind: "source".to_string(),
                    command: "tracey.openDashboard".to_string(),
                    arguments: vec![rule_at_pos.req_id.to_string()],
                    is_preferred: false,
                });
            }
        }

        actions
    }

    /// Get document highlight ranges (same requirement references)
    ///
    /// r[impl lsp.highlight.full-range]
    /// r[impl lsp.highlight.consistent]
    async fn lsp_document_highlight(
        &self,
        _cx: &Context,
        req: LspPositionRequest,
    ) -> Vec<LspLocation> {
        let path = PathBuf::from(&req.path);

        // Find the rule at cursor position (works for both spec and source files)
        let Some(rule_at_pos) =
            find_rule_at_position(&path, &req.content, req.line, req.character).await
        else {
            return vec![];
        };

        // For markdown files, highlight all definitions of the same rule (typically just one)
        if path.extension().is_some_and(|ext| ext == "md") {
            let options = marq::RenderOptions::default();
            if let Ok(doc) = marq::render(&req.content, &options).await {
                return doc
                    .reqs
                    .iter()
                    .filter(|r| {
                        parse_rule_id(&r.id.to_string()).is_some_and(|id| id == rule_at_pos.req_id)
                    })
                    .map(|r| {
                        let (start_line, start_char, _, _) =
                            span_to_range(&req.content, r.span.offset, r.span.length);
                        LspLocation {
                            path: req.path.clone(),
                            line: start_line,
                            character: start_char,
                        }
                    })
                    .collect();
            }
            return vec![];
        }

        // For source files, find all references to the same rule in this document
        let reqs = tracey_core::Reqs::extract_from_content(&path, &req.content);
        reqs.references
            .iter()
            .filter(|r| r.req_id == rule_at_pos.req_id)
            .map(|r| {
                let (start_line, start_char, _, _) =
                    span_to_range(&req.content, r.span.offset, r.span.length);
                LspLocation {
                    path: req.path.clone(),
                    line: start_line,
                    character: start_char,
                }
            })
            .collect()
    }

    // =========================================================================
    // Config Modification Methods (for MCP)
    // =========================================================================

    /// Add an exclude pattern to an implementation
    ///
    /// r[impl mcp.config.exclude]
    /// r[impl mcp.config.persist]
    async fn config_add_exclude(
        &self,
        _cx: &Context,
        req: ConfigPatternRequest,
    ) -> Result<(), String> {
        let data = self.inner.engine.data().await;
        let (spec_name, impl_name) =
            self.resolve_spec_impl(req.spec.as_deref(), req.impl_name.as_deref(), &data.config);

        // Load current config
        let config_path = self.inner.engine.config_path().to_path_buf();
        let mut config = match crate::load_config(&config_path) {
            Ok(c) => c,
            Err(e) => return Err(format!("Error loading config: {}", e)),
        };

        // Find the spec and impl
        let mut found = false;
        for spec in &mut config.specs {
            if spec.name == spec_name {
                for impl_ in &mut spec.impls {
                    if impl_.name == impl_name {
                        impl_.exclude.push(req.pattern.clone());
                        found = true;
                        break;
                    }
                }
                break;
            }
        }

        if !found {
            return Err(format!("Spec/impl '{}/{}' not found", spec_name, impl_name));
        }

        // Save config
        if let Err(e) = save_config(&config_path, &config) {
            return Err(format!("Error saving config: {}", e));
        }

        Ok(())
    }

    /// Add an include pattern to an implementation
    ///
    /// r[impl mcp.config.include]
    /// r[impl mcp.config.persist]
    async fn config_add_include(
        &self,
        _cx: &Context,
        req: ConfigPatternRequest,
    ) -> Result<(), String> {
        let data = self.inner.engine.data().await;
        let (spec_name, impl_name) =
            self.resolve_spec_impl(req.spec.as_deref(), req.impl_name.as_deref(), &data.config);

        // Load current config
        let config_path = self.inner.engine.config_path().to_path_buf();
        let mut config = match crate::load_config(&config_path) {
            Ok(c) => c,
            Err(e) => return Err(format!("Error loading config: {}", e)),
        };

        // Find the spec and impl
        let mut found = false;
        for spec in &mut config.specs {
            if spec.name == spec_name {
                for impl_ in &mut spec.impls {
                    if impl_.name == impl_name {
                        impl_.include.push(req.pattern.clone());
                        found = true;
                        break;
                    }
                }
                break;
            }
        }

        if !found {
            return Err(format!("Spec/impl '{}/{}' not found", spec_name, impl_name));
        }

        // Save config
        if let Err(e) = save_config(&config_path, &config) {
            return Err(format!("Error saving config: {}", e));
        }

        Ok(())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Information about a rule reference or definition at a cursor position
struct RuleAtPosition {
    /// The rule ID
    req_id: RuleId,
    /// Byte offset in the content
    span_offset: usize,
    /// Length in bytes
    span_length: usize,
}

/// Find a rule (reference or definition) at the given position.
///
/// For markdown spec files, uses marq to extract requirement definitions.
/// For source files, uses the lexer to extract references from comments.
async fn find_rule_at_position(
    path: &Path,
    content: &str,
    line: u32,
    character: u32,
) -> Option<RuleAtPosition> {
    if path.extension().is_some_and(|ext| ext == "md") {
        // Parse markdown to find requirement definitions
        let options = marq::RenderOptions::default();
        let doc = marq::render(content, &options).await.ok()?;

        let target_offset = line_col_to_offset(content, line, character)?;

        doc.reqs.iter().find_map(|r| {
            let start = r.span.offset;
            let end = r.span.offset + r.span.length;
            if target_offset >= start && target_offset < end {
                Some(RuleAtPosition {
                    req_id: parse_rule_id(&r.id.to_string())?,
                    span_offset: r.span.offset,
                    span_length: r.span.length,
                })
            } else {
                None
            }
        })
    } else {
        // Parse source file to find references in comments
        let reqs = tracey_core::Reqs::extract_from_content(path, content);
        let ref_at_pos = find_ref_at_position(&reqs, content, line, character)?;

        Some(RuleAtPosition {
            req_id: ref_at_pos.req_id.clone(),
            span_offset: ref_at_pos.span.offset,
            span_length: ref_at_pos.span.length,
        })
    }
}

/// Find a reference at the given position in the content (for source files only)
fn find_ref_at_position<'a>(
    reqs: &'a tracey_core::Reqs,
    content: &str,
    line: u32,
    character: u32,
) -> Option<&'a tracey_core::ReqReference> {
    let target_offset = line_col_to_offset(content, line, character)?;

    reqs.references.iter().find(|r| {
        let start = r.span.offset;
        let end = r.span.offset + r.span.length;
        target_offset >= start && target_offset < end
    })
}

/// Convert line/column (0-indexed) to byte offset
fn line_col_to_offset(content: &str, line: u32, col: u32) -> Option<usize> {
    let mut current_line = 0u32;
    let mut offset = 0usize;

    for (i, c) in content.char_indices() {
        if current_line == line {
            let line_start = offset;
            // Find the column within this line
            for (current_col, (j, ch)) in content[line_start..].char_indices().enumerate() {
                if ch == '\n' {
                    break;
                }
                if current_col as u32 == col {
                    return Some(line_start + j);
                }
            }
            // If col is at or past end of line, return end of line
            return Some(i);
        }
        if c == '\n' {
            current_line += 1;
        }
        offset = i + c.len_utf8();
    }

    // Handle last line
    if current_line == line {
        Some(offset)
    } else {
        None
    }
}

/// Convert byte offset and length to line/column range (0-indexed)
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

    // Handle end of file
    if !found_start {
        (line, col, line, col)
    } else {
        (start_line, start_col, line, col)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum KnownRuleMatch {
    Exact,
    Stale(RuleId),
    Missing,
}

// r[impl coverage.compute.stale]
// r[impl coverage.compute.stale.update]
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

/// Find a rule by ID in the engine data
fn find_rule_in_data<'a>(
    data: &'a crate::data::DashboardData,
    rule_id: &RuleId,
) -> Option<(&'a String, &'a ApiRule)> {
    let mut best_match: Option<(&'a String, &'a ApiRule)> = None;
    for ((spec, _), forward_data) in &data.forward_by_impl {
        for rule in &forward_data.rules {
            if rule.id.base == rule_id.base {
                match best_match {
                    Some((_, current)) if current.id.version >= rule.id.version => {}
                    _ => {
                        best_match = Some((spec, rule));
                    }
                }
            }
        }
    }
    best_match
}

fn run_git_capture(project_root: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

async fn find_rule_text_in_markdown(content: &str, rule_id: &RuleId) -> Option<String> {
    let options = marq::RenderOptions::default();
    let doc = marq::render(content, &options).await.ok()?;
    let rule_id = rule_id.to_string();
    doc.reqs
        .iter()
        .find(|req| req.id.to_string() == rule_id)
        .map(|req| req.raw.clone())
}

async fn load_previous_rule_text_from_git(
    project_root: &Path,
    source_file: &str,
    previous_rule_id: &RuleId,
) -> Option<HistoricalRuleText> {
    // r[impl validation.stale.diff]
    let commits = run_git_capture(project_root, &["log", "--format=%H", "--", source_file])?;

    for commit in commits.lines() {
        let show_arg = format!("{commit}:{source_file}");
        let content = run_git_capture(project_root, &["show", &show_arg]);
        let Some(content) = content else {
            continue;
        };

        if let Some(text) = find_rule_text_in_markdown(&content, previous_rule_id).await {
            return Some(HistoricalRuleText {
                commit: commit.to_string(),
                text,
            });
        }
    }

    None
}

enum DiffLine<'a> {
    Equal(&'a str),
    Remove(&'a str),
    Add(&'a str),
}

fn line_diff<'a>(old: &'a str, new: &'a str) -> Vec<DiffLine<'a>> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let n = old_lines.len();
    let m = new_lines.len();

    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if old_lines[i] == new_lines[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut i = 0;
    let mut j = 0;
    let mut out = Vec::new();
    while i < n && j < m {
        if old_lines[i] == new_lines[j] {
            out.push(DiffLine::Equal(old_lines[i]));
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            out.push(DiffLine::Remove(old_lines[i]));
            i += 1;
        } else {
            out.push(DiffLine::Add(new_lines[j]));
            j += 1;
        }
    }
    while i < n {
        out.push(DiffLine::Remove(old_lines[i]));
        i += 1;
    }
    while j < m {
        out.push(DiffLine::Add(new_lines[j]));
        j += 1;
    }

    out
}

fn append_indented_block(message: &mut String, title: &str, body: &str) {
    message.push('\n');
    message.push_str(title);
    message.push('\n');
    if body.is_empty() {
        message.push_str("  (empty)\n");
        return;
    }
    for line in body.lines() {
        message.push_str("  ");
        message.push_str(line);
        message.push('\n');
    }
}

fn build_rule_text_diff(old_text: &str, new_text: &str) -> String {
    let ops = line_diff(old_text, new_text);
    let mut out = String::new();
    for op in ops {
        match op {
            DiffLine::Equal(line) => {
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
            DiffLine::Remove(line) => {
                out.push('-');
                out.push_str(line);
                out.push('\n');
            }
            DiffLine::Add(line) => {
                out.push('+');
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

async fn stale_requirement_message(
    project_root: &Path,
    reference_rule_id: &RuleId,
    current_rule: Option<&ApiRule>,
    cache: &mut std::collections::HashMap<(RuleId, RuleId), Option<HistoricalRuleText>>,
) -> String {
    // r[impl validation.stale.message-prefix]
    let mut message = String::from(STALE_IMPLEMENTATION_MUST_CHANGE_PREFIX);

    let Some(current_rule) = current_rule else {
        message.push_str(". The referenced annotation is stale, but the latest matching rule could not be loaded.");
        return message;
    };

    message.push_str(&format!(
        ". Reference '{}' is stale; current rule is '{}'.",
        reference_rule_id, current_rule.id
    ));

    let Some(source_file) = current_rule.source_file.as_deref() else {
        message.push_str(
            "\n\nRule-text history is unavailable because the current rule source file is unknown.",
        );
        return message;
    };

    let key = (reference_rule_id.clone(), current_rule.id.clone());
    let historical = if let Some(entry) = cache.get(&key) {
        entry.clone()
    } else {
        let loaded =
            load_previous_rule_text_from_git(project_root, source_file, reference_rule_id).await;
        cache.insert(key.clone(), loaded.clone());
        loaded
    };

    let Some(previous) = historical else {
        // r[impl validation.stale.diff.fallback]
        message.push_str(
            "\n\nRule-text history is unavailable (git history is missing, shallow, or does not contain the older rule text).",
        );
        return message;
    };

    append_indented_block(&mut message, "Previous rule text:", &previous.text);
    append_indented_block(&mut message, "Current rule text:", &current_rule.raw);
    let diff = build_rule_text_diff(&previous.text, &current_rule.raw);
    append_indented_block(&mut message, "Diff:", &diff);
    message.push_str(&format!(
        "Previous rule text source commit: {}",
        previous.commit
    ));
    message
}

/// Save config to file
fn save_config(path: &Path, config: &crate::config::Config) -> eyre::Result<()> {
    use std::io::Write;
    let yaml_string = facet_yaml::to_string(config)?;
    let mut file = std::fs::File::create(path)?;
    file.write_all(yaml_string.as_bytes())?;
    Ok(())
}

/// Check if a rule ID follows the naming convention
fn is_valid_rule_id(id: &RuleId) -> bool {
    let base_id = &id.base;

    // Split by dots and check each segment
    for segment in base_id.split('.') {
        if segment.is_empty() {
            return false;
        }
        // Each segment must contain only lowercase letters, digits, or hyphens
        if !segment
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return false;
        }
        // Segment must start with a letter
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

/// Detect circular dependencies in the rule dependency graph
fn detect_circular_dependencies(forward_data: &ApiSpecForward) -> Vec<Vec<RuleId>> {
    use std::collections::{HashMap, HashSet};

    // Build adjacency list from depends_refs
    // Note: This is a simplified version - in a full implementation,
    // we'd need to track which rule ID each depends ref points to
    let mut graph: HashMap<RuleId, Vec<RuleId>> = HashMap::new();

    for rule in &forward_data.rules {
        // Initialize empty adjacency list for each rule
        graph.entry(rule.id.clone()).or_default();

        // For now, we can't easily extract dependency targets from depends_refs
        // since they only contain file:line references, not rule IDs.
        // A proper implementation would require parsing the depends comments
        // to extract the target rule IDs.
    }

    // Detect cycles using DFS
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
                    // Found a cycle
                    let cycle_start = path.iter().position(|n| n == neighbor).unwrap();
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
