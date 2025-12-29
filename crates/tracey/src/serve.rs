//! HTTP server for the tracey dashboard
//!
//! Serves an interactive HTML dashboard showing both forward and reverse
//! traceability coverage, with live reload support.

use async_tiny::{Header, Response, Server};
use eyre::{Result, WrapErr};
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use owo_colors::OwoColorize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::watch;

use crate::config::Config;

/// State that gets recomputed when files change
struct DashboardState {
    /// The rendered HTML
    html: String,
    /// Version number (incremented on each rebuild)
    version: u64,
}

/// Run the serve command
pub fn run(config_path: Option<PathBuf>, port: u16, open_browser: bool) -> Result<()> {
    // Build a single-threaded tokio runtime (no macros)
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .wrap_err("Failed to create tokio runtime")?;

    rt.block_on(async move { run_server(config_path, port, open_browser).await })
}

async fn run_server(config_path: Option<PathBuf>, port: u16, open_browser: bool) -> Result<()> {
    // Find project root
    let project_root = crate::find_project_root()?;

    // Load config
    let config_path = config_path.unwrap_or_else(|| project_root.join(".config/tracey/config.kdl"));

    let config = crate::load_config(&config_path)?;

    // Initial build
    let version = Arc::new(AtomicU64::new(1));
    let initial_html = build_dashboard(&project_root, &config_path, &config)?;

    // Channel for broadcasting updates
    let (tx, rx) = watch::channel(DashboardState {
        html: initial_html,
        version: 1,
    });

    // Set up file watching
    let watch_version = version.clone();
    let watch_project_root = project_root.clone();
    let watch_config_path = config_path.clone();
    let watch_config = config.clone();

    let (debounce_tx, mut debounce_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Spawn file watcher in a blocking task
    std::thread::spawn(move || {
        let debounce_tx = debounce_tx;
        let mut debouncer = match new_debouncer(
            Duration::from_millis(200),
            move |_res: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                // Trigger rebuild (ignore send errors if receiver dropped)
                let _ = debounce_tx.blocking_send(());
            },
        ) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{} Failed to create file watcher: {}", "!".yellow(), e);
                return;
            }
        };

        // Watch source directories based on config
        for spec_config in &watch_config.specs {
            // Watch include patterns' parent directories
            let dirs_to_watch: Vec<PathBuf> = if spec_config.include.is_empty() {
                vec![watch_project_root.clone()]
            } else {
                spec_config
                    .include
                    .iter()
                    .map(|i| {
                        // Extract base directory from glob pattern
                        let pattern = &i.pattern;
                        if let Some(idx) = pattern.find("**") {
                            let base = &pattern[..idx];
                            if base.is_empty() {
                                watch_project_root.clone()
                            } else {
                                watch_project_root.join(base.trim_end_matches('/'))
                            }
                        } else {
                            watch_project_root.clone()
                        }
                    })
                    .collect()
            };

            for dir in dirs_to_watch {
                if dir.exists()
                    && let Err(e) = debouncer.watcher().watch(&dir, RecursiveMode::Recursive)
                {
                    eprintln!("{} Failed to watch {}: {}", "!".yellow(), dir.display(), e);
                }
            }
        }

        // Also watch the config file
        if let Some(parent) = watch_config_path.parent() {
            let _ = debouncer.watcher().watch(parent, RecursiveMode::Recursive);
        }

        // Keep the watcher alive
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    });

    // Spawn rebuild task
    let rebuild_tx = tx.clone();
    let rebuild_project_root = project_root.clone();
    let rebuild_config_path = config_path.clone();
    tokio::spawn(async move {
        while debounce_rx.recv().await.is_some() {
            // Reload config in case it changed
            let config = match crate::load_config(&rebuild_config_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{} Config reload error: {}", "!".yellow(), e);
                    continue;
                }
            };

            match build_dashboard(&rebuild_project_root, &rebuild_config_path, &config) {
                Ok(html) => {
                    let new_version = watch_version.fetch_add(1, Ordering::SeqCst) + 1;
                    eprintln!(
                        "{} Rebuilt dashboard (v{})",
                        "->".blue().bold(),
                        new_version
                    );
                    let _ = rebuild_tx.send(DashboardState {
                        html,
                        version: new_version,
                    });
                }
                Err(e) => {
                    eprintln!("{} Rebuild error: {}", "!".yellow(), e);
                }
            }
        }
    });

    // Start HTTP server
    let addr = format!("127.0.0.1:{}", port);
    let mut server = Server::http(&addr, true)
        .await
        .wrap_err_with(|| format!("Failed to bind to {}", addr))?;

    let url = format!("http://{}", addr);
    eprintln!(
        "\n{} Serving tracey dashboard at {}",
        "OK".green().bold(),
        url.cyan()
    );
    eprintln!("   Press Ctrl+C to stop\n");

    if open_browser && let Err(e) = open::that(&url) {
        eprintln!("{} Failed to open browser: {}", "!".yellow(), e);
    }

    // Handle requests
    loop {
        let Some(req) = server.next().await else {
            continue;
        };

        let path = req.url();
        let state = rx.borrow();

        let response = match path {
            "/" => {
                // Main dashboard
                Response::from_string(state.html.clone())
                    .with_content_type("text/html; charset=utf-8")
            }
            "/__version" => {
                // Version endpoint for polling
                Response::from_string(state.version.to_string())
                    .with_content_type("text/plain")
                    .with_header(Header::new("Cache-Control", "no-cache").unwrap())
            }
            _ => Response::from_string("Not Found").with_status_code(404),
        };

        if let Err(e) = req.respond(response) {
            eprintln!("{} Response error: {:?}", "!".yellow(), e);
        }
    }
}

/// Build the full dashboard HTML
fn build_dashboard(project_root: &Path, config_path: &Path, config: &Config) -> Result<String> {
    use tracey_core::code_units::{CodeUnits, extract_rust};
    use tracey_core::{RefVerb, Rules, SpecManifest, WalkSources};

    let config_dir = config_path
        .parent()
        .ok_or_else(|| eyre::eyre!("Config path has no parent directory"))?;

    let mut all_forward_data = Vec::new();
    let mut all_reverse_data = CodeUnits::new();

    for spec_config in &config.specs {
        let spec_name = &spec_config.name.value;

        // Load manifest
        let manifest = match (
            &spec_config.rules_url,
            &spec_config.rules_file,
            &spec_config.rules_glob,
        ) {
            (Some(url), None, None) => SpecManifest::fetch(&url.value)?,
            (None, Some(file), None) => {
                let file_path = config_dir.join(&file.path);
                SpecManifest::load(&file_path)?
            }
            (None, None, Some(glob)) => {
                crate::load_manifest_from_glob(project_root, &glob.pattern)?
            }
            (None, None, None) => {
                eyre::bail!("Spec '{}' has no rules source", spec_name);
            }
            _ => {
                eyre::bail!("Spec '{}' has multiple rules sources", spec_name);
            }
        };

        // Get include/exclude patterns
        let include: Vec<String> = if spec_config.include.is_empty() {
            tracey_core::SUPPORTED_EXTENSIONS
                .iter()
                .map(|ext| format!("**/*.{}", ext))
                .collect()
        } else {
            spec_config
                .include
                .iter()
                .map(|i| i.pattern.clone())
                .collect()
        };

        let exclude: Vec<String> = if spec_config.exclude.is_empty() {
            vec!["target/**".to_string()]
        } else {
            spec_config
                .exclude
                .iter()
                .map(|e| e.pattern.clone())
                .collect()
        };

        // Extract rule references (forward traceability)
        let rules = Rules::extract(
            WalkSources::new(project_root)
                .include(include.clone())
                .exclude(exclude.clone()),
        )?;

        // Build forward data for this spec
        for (rule_id, rule_info) in &manifest.rules {
            let mut impl_refs = Vec::new();
            let mut verify_refs = Vec::new();

            for r in &rules.references {
                if r.rule_id == *rule_id {
                    let relative = r.file.strip_prefix(project_root).unwrap_or(&r.file);
                    let location = format!("{}:{}", relative.display(), r.line);
                    match r.verb {
                        RefVerb::Impl | RefVerb::Define => impl_refs.push(location),
                        RefVerb::Verify => verify_refs.push(location),
                        RefVerb::Depends | RefVerb::Related => {}
                    }
                }
            }

            all_forward_data.push(ForwardRow {
                spec: spec_name.clone(),
                rule_id: rule_id.clone(),
                url: rule_info.url.clone(),
                text: rule_info.text.clone(),
                status: rule_info.status.clone(),
                level: rule_info.level.clone(),
                source_file: rule_info.source_file.clone(),
                source_line: rule_info.source_line,
                impl_refs,
                verify_refs,
            });
        }

        // Extract code units (reverse traceability) - Rust files only for now
        let walker = ignore::WalkBuilder::new(project_root)
            .follow_links(true)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker.flatten() {
            let path = entry.path();

            // Only Rust files for now
            if path.extension().is_some_and(|e| e == "rs") {
                // Check include patterns
                let relative = path.strip_prefix(project_root).unwrap_or(path);
                let relative_str = relative.to_string_lossy();

                let included = include.is_empty()
                    || include.iter().any(|p| {
                        if let Some(ext) = p.strip_prefix("**/*.") {
                            relative_str.ends_with(&format!(".{}", ext))
                        } else {
                            true
                        }
                    });

                let excluded = exclude.iter().any(|p| {
                    if let Some(prefix) = p.strip_suffix("/**") {
                        relative_str.starts_with(prefix)
                    } else {
                        false
                    }
                });

                if included
                    && !excluded
                    && let Ok(content) = std::fs::read_to_string(path)
                {
                    let units = extract_rust(path, &content);
                    all_reverse_data.extend(units);
                }
            }
        }
    }

    // Sort forward data by rule ID
    all_forward_data.sort_by(|a, b| a.rule_id.cmp(&b.rule_id));

    // Render HTML
    Ok(render_dashboard_html(
        &all_forward_data,
        &all_reverse_data,
        project_root,
    ))
}

#[derive(Debug)]
#[allow(dead_code)] // Fields will be used when we port matrix HTML features
struct ForwardRow {
    spec: String,
    rule_id: String,
    url: String,
    text: Option<String>,
    status: Option<String>,
    level: Option<String>,
    source_file: Option<String>,
    source_line: Option<usize>,
    impl_refs: Vec<String>,
    verify_refs: Vec<String>,
}

fn render_dashboard_html(
    forward_data: &[ForwardRow],
    reverse_data: &tracey_core::code_units::CodeUnits,
    project_root: &Path,
) -> String {
    let abs_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let root_str = abs_root.display().to_string();

    // Calculate stats
    let total_rules = forward_data.len();
    let rules_with_impl = forward_data
        .iter()
        .filter(|r| !r.impl_refs.is_empty())
        .count();
    let rules_with_verify = forward_data
        .iter()
        .filter(|r| !r.verify_refs.is_empty())
        .count();

    let total_code_units = reverse_data.len();

    let impl_pct = if total_rules > 0 {
        (rules_with_impl as f64 / total_rules as f64) * 100.0
    } else {
        100.0
    };
    let verify_pct = if total_rules > 0 {
        (rules_with_verify as f64 / total_rules as f64) * 100.0
    } else {
        100.0
    };
    let reverse_pct = reverse_data.coverage_percent();

    let mut html = String::new();

    // HTML head
    html.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"color-scheme\" content=\"light dark\">\n");
    html.push_str("<title>tracey - Coverage Dashboard</title>\n");
    html.push_str("<link rel=\"preconnect\" href=\"https://fonts.googleapis.com\">\n");
    html.push_str("<link rel=\"preconnect\" href=\"https://fonts.gstatic.com\" crossorigin>\n");
    html.push_str("<link href=\"https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500&family=Public+Sans:wght@400;500;600&display=swap\" rel=\"stylesheet\">\n");

    // CSS
    html.push_str("<style>\n");
    html.push_str(include_str!("serve_styles.css"));
    html.push_str("\n</style>\n");

    // JavaScript for live reload and interactivity
    html.push_str("<script>\n");
    html.push_str(&format!(
        "const PROJECT_ROOT = \"{}\";\n",
        root_str.replace('\\', "\\\\").replace('"', "\\\"")
    ));
    html.push_str(include_str!("serve_script.js"));
    html.push_str("\n</script>\n");

    html.push_str("</head>\n<body>\n");

    // Header
    html.push_str("<header>\n");
    html.push_str("<h1>tracey</h1>\n");
    html.push_str("<div class=\"live-indicator\" title=\"Live reload active\"><span class=\"live-dot\"></span>Live</div>\n");
    html.push_str("</header>\n");

    // Stats bar
    html.push_str("<div class=\"stats-bar\">\n");

    let pct_class = |pct: f64| {
        if pct >= 80.0 {
            "good"
        } else if pct >= 50.0 {
            "partial"
        } else {
            "bad"
        }
    };

    html.push_str(&format!(
        "<div class=\"stat\"><span class=\"stat-label\">Rules</span><span class=\"stat-value\">{}</span></div>\n",
        total_rules
    ));
    html.push_str(&format!(
        "<div class=\"stat\"><span class=\"stat-label\">Impl Coverage</span><span class=\"stat-value stat-{}\">{:.1}%</span></div>\n",
        pct_class(impl_pct), impl_pct
    ));
    html.push_str(&format!(
        "<div class=\"stat\"><span class=\"stat-label\">Test Coverage</span><span class=\"stat-value stat-{}\">{:.1}%</span></div>\n",
        pct_class(verify_pct), verify_pct
    ));
    html.push_str(&format!(
        "<div class=\"stat\"><span class=\"stat-label\">Code Units</span><span class=\"stat-value\">{}</span></div>\n",
        total_code_units
    ));
    html.push_str(&format!(
        "<div class=\"stat\"><span class=\"stat-label\">Reverse Coverage</span><span class=\"stat-value stat-{}\">{:.1}%</span></div>\n",
        pct_class(reverse_pct), reverse_pct
    ));

    html.push_str("</div>\n");

    // Tabs
    html.push_str("<div class=\"tabs\">\n");
    html.push_str("<button class=\"tab active\" onclick=\"showTab('forward')\">Forward Traceability</button>\n");
    html.push_str(
        "<button class=\"tab\" onclick=\"showTab('reverse')\">Reverse Traceability</button>\n",
    );
    html.push_str("</div>\n");

    // Forward traceability panel
    html.push_str("<div id=\"forward-panel\" class=\"panel active\">\n");
    html.push_str("<p class=\"panel-desc\">Which spec rules have implementations and tests?</p>\n");
    html.push_str("<table class=\"data-table\">\n");
    html.push_str("<thead><tr><th>Rule</th><th>Impl</th><th>Verify</th></tr></thead>\n");
    html.push_str("<tbody>\n");

    for row in forward_data {
        let coverage_class = if !row.impl_refs.is_empty() && !row.verify_refs.is_empty() {
            "covered"
        } else if !row.impl_refs.is_empty() || !row.verify_refs.is_empty() {
            "partial"
        } else {
            "uncovered"
        };

        html.push_str(&format!("<tr class=\"{}\">\n", coverage_class));

        // Rule cell
        html.push_str("<td class=\"rule-cell\">\n");
        html.push_str(&format!(
            "<div class=\"rule-id\">{}</div>\n",
            html_escape::encode_safe(&row.rule_id)
        ));
        if let Some(text) = &row.text {
            html.push_str(&format!(
                "<div class=\"rule-desc\">{}</div>\n",
                html_escape::encode_safe(text)
            ));
        }
        html.push_str("</td>\n");

        // Impl refs
        html.push_str("<td class=\"refs-cell\">\n");
        for loc in &row.impl_refs {
            if let Some((path, line)) = loc.rsplit_once(':') {
                html.push_str(&format!(
                    "<div class=\"ref-line\"><a class=\"file-link\" data-path=\"{}\" data-line=\"{}\">{}</a></div>\n",
                    html_escape::encode_safe(path),
                    line,
                    html_escape::encode_safe(loc)
                ));
            }
        }
        if row.impl_refs.is_empty() {
            html.push_str("<span class=\"no-refs\">-</span>\n");
        }
        html.push_str("</td>\n");

        // Verify refs
        html.push_str("<td class=\"refs-cell\">\n");
        for loc in &row.verify_refs {
            if let Some((path, line)) = loc.rsplit_once(':') {
                html.push_str(&format!(
                    "<div class=\"ref-line\"><a class=\"file-link\" data-path=\"{}\" data-line=\"{}\">{}</a></div>\n",
                    html_escape::encode_safe(path),
                    line,
                    html_escape::encode_safe(loc)
                ));
            }
        }
        if row.verify_refs.is_empty() {
            html.push_str("<span class=\"no-refs\">-</span>\n");
        }
        html.push_str("</td>\n");

        html.push_str("</tr>\n");
    }

    html.push_str("</tbody>\n</table>\n");
    html.push_str("</div>\n");

    // Reverse traceability panel
    html.push_str("<div id=\"reverse-panel\" class=\"panel\">\n");
    html.push_str(
        "<p class=\"panel-desc\">Which code units are linked to spec requirements?</p>\n",
    );
    html.push_str("<table class=\"data-table\">\n");
    html.push_str(
        "<thead><tr><th>Code Unit</th><th>Location</th><th>Linked Rules</th></tr></thead>\n",
    );
    html.push_str("<tbody>\n");

    for unit in &reverse_data.units {
        let coverage_class = if unit.rule_refs.is_empty() {
            "uncovered"
        } else {
            "covered"
        };

        let relative = unit.file.strip_prefix(project_root).unwrap_or(&unit.file);
        let location = format!("{}:{}", relative.display(), unit.start_line);

        html.push_str(&format!("<tr class=\"{}\">\n", coverage_class));

        // Code unit name
        html.push_str("<td class=\"code-unit-cell\">\n");
        html.push_str(&format!("<span class=\"unit-kind\">{}</span> ", unit.kind));
        if let Some(name) = &unit.name {
            html.push_str(&format!(
                "<span class=\"unit-name\">{}</span>",
                html_escape::encode_safe(name)
            ));
        }
        html.push_str("</td>\n");

        // Location
        html.push_str("<td class=\"location-cell\">\n");
        html.push_str(&format!(
            "<a class=\"file-link\" data-path=\"{}\" data-line=\"{}\">{}</a>\n",
            html_escape::encode_safe(&relative.display().to_string()),
            unit.start_line,
            html_escape::encode_safe(&location)
        ));
        html.push_str("</td>\n");

        // Linked rules
        html.push_str("<td class=\"rules-cell\">\n");
        if unit.rule_refs.is_empty() {
            html.push_str("<span class=\"no-refs\">No spec links</span>\n");
        } else {
            for rule_id in &unit.rule_refs {
                html.push_str(&format!(
                    "<span class=\"rule-tag\">{}</span>\n",
                    html_escape::encode_safe(rule_id)
                ));
            }
        }
        html.push_str("</td>\n");

        html.push_str("</tr>\n");
    }

    html.push_str("</tbody>\n</table>\n");
    html.push_str("</div>\n");

    html.push_str("</body>\n</html>\n");

    html
}
