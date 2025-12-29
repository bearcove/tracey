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

use async_tiny::{Header, Response, Server};
use eyre::{Result, WrapErr};
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use owo_colors::OwoColorize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::watch;
use tracey_core::code_units::CodeUnit;
use tracey_core::{RefVerb, Rules, SpecManifest};

use crate::config::Config;

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
    /// Version number (incremented on each rebuild)
    version: u64,
}

// ============================================================================
// JSON Serialization (manual, no serde)
// ============================================================================

fn json_string(s: &str) -> String {
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

    // Build reverse data summary
    let mut total_units = 0;
    let mut covered_units = 0;
    let mut file_entries = Vec::new();

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
    }

    // Sort files by path
    file_entries.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(DashboardData {
        config: api_config,
        forward: ApiForwardData {
            specs: forward_specs,
        },
        reverse: ApiReverseData {
            total_units,
            covered_units,
            files: file_entries,
        },
        code_units_by_file,
        specs_content,
        version,
    })
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
// Static HTML Shell
// ============================================================================

const HTML_SHELL: &str = include_str!("dashboard.html");

// ============================================================================
// HTTP Server
// ============================================================================

/// Run the serve command
pub fn run(config_path: Option<PathBuf>, port: u16, open_browser: bool) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .wrap_err("Failed to create tokio runtime")?;

    rt.block_on(async move { run_server(config_path, port, open_browser).await })
}

async fn run_server(config_path: Option<PathBuf>, port: u16, open_browser: bool) -> Result<()> {
    let project_root = crate::find_project_root()?;
    let config_path = config_path.unwrap_or_else(|| project_root.join(".config/tracey/config.kdl"));
    let config = crate::load_config(&config_path)?;

    let version = Arc::new(AtomicU64::new(1));

    // Initial build
    let initial_data = build_dashboard_data(&project_root, &config_path, &config, 1)?;

    // Channel for state updates
    let (tx, rx) = watch::channel(Arc::new(initial_data));

    // Clone for file watcher
    let watch_project_root = project_root.clone();
    let _watch_config_path = config_path.clone();
    let _watch_config = config.clone();

    let (debounce_tx, mut debounce_rx) = tokio::sync::mpsc::channel::<()>(1);

    // File watcher thread
    std::thread::spawn(move || {
        let debounce_tx = debounce_tx;
        let mut debouncer = match new_debouncer(
            Duration::from_millis(200),
            move |_res: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                let _ = debounce_tx.blocking_send(());
            },
        ) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{} Failed to create file watcher: {}", "!".yellow(), e);
                return;
            }
        };

        // Watch project root
        if let Err(e) = debouncer
            .watcher()
            .watch(&watch_project_root, RecursiveMode::Recursive)
        {
            eprintln!(
                "{} Failed to watch {}: {}",
                "!".yellow(),
                watch_project_root.display(),
                e
            );
        }

        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    });

    // Rebuild task
    let rebuild_tx = tx.clone();
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

            let new_version = rebuild_version.fetch_add(1, Ordering::SeqCst) + 1;

            match build_dashboard_data(
                &rebuild_project_root,
                &rebuild_config_path,
                &config,
                new_version,
            ) {
                Ok(data) => {
                    eprintln!(
                        "{} Rebuilt dashboard (v{})",
                        "->".blue().bold(),
                        new_version
                    );
                    let _ = rebuild_tx.send(Arc::new(data));
                }
                Err(e) => {
                    eprintln!("{} Rebuild error: {}", "!".yellow(), e);
                }
            }
        }
    });

    // Start server
    let addr = format!("127.0.0.1:{}", port);
    let mut server = Server::http(&addr, true)
        .await
        .wrap_err_with(|| format!("Failed to bind to {}", addr))?;

    let url = format!("http://{}", addr);
    eprintln!(
        "\n{} Serving tracey dashboard at {}\n   Press Ctrl+C to stop\n",
        "OK".green().bold(),
        url.cyan()
    );

    if open_browser && let Err(e) = open::that(&url) {
        eprintln!("{} Failed to open browser: {}", "!".yellow(), e);
    }

    loop {
        let Some(req) = server.next().await else {
            continue;
        };

        let state = rx.borrow().clone();
        let path = req.url();
        let project_root = project_root.clone();

        let response = match path {
            "/" => Response::from_string(HTML_SHELL).with_content_type("text/html; charset=utf-8"),

            "/api/config" => {
                Response::from_string(state.config.to_json()).with_content_type("application/json")
            }

            "/api/forward" => {
                Response::from_string(state.forward.to_json()).with_content_type("application/json")
            }

            "/api/reverse" => {
                Response::from_string(state.reverse.to_json()).with_content_type("application/json")
            }

            "/api/version" => Response::from_string(format!(r#"{{"version":{}}}"#, state.version))
                .with_content_type("application/json")
                .with_header(Header::new("Cache-Control", "no-cache").unwrap()),

            p if p.starts_with("/api/file?path=") => {
                let file_path = p.strip_prefix("/api/file?path=").unwrap_or("");
                let file_path = urlencoding::decode(file_path).unwrap_or_default();
                let full_path = project_root.join(file_path.as_ref());

                if let Some(units) = state.code_units_by_file.get(&full_path) {
                    let content = std::fs::read_to_string(&full_path).unwrap_or_default();
                    let relative = full_path
                        .strip_prefix(&project_root)
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

                    Response::from_string(file_data.to_json()).with_content_type("application/json")
                } else {
                    Response::from_string(r#"{"error":"File not found"}"#)
                        .with_status_code(404)
                        .with_content_type("application/json")
                }
            }

            p if p.starts_with("/api/spec?name=") => {
                let spec_name = p.strip_prefix("/api/spec?name=").unwrap_or("");
                let spec_name = urlencoding::decode(spec_name).unwrap_or_default();

                if let Some(spec_data) = state.specs_content.get(spec_name.as_ref()) {
                    Response::from_string(spec_data.to_json()).with_content_type("application/json")
                } else {
                    Response::from_string(r#"{"error":"Spec not found"}"#)
                        .with_status_code(404)
                        .with_content_type("application/json")
                }
            }

            _ => Response::from_string("Not Found").with_status_code(404),
        };

        if let Err(e) = req.respond(response) {
            eprintln!("{} Response error: {:?}", "!".yellow(), e);
        }
    }
}
