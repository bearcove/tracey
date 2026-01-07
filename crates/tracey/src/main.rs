//! tracey - Measure spec coverage in Rust codebases
//!
//! tracey parses Rust source files to find references to specification rules
//! (in the format `[rule.id]` in comments) and compares them against a spec
//! manifest to produce coverage reports.

mod bridge;
mod config;
mod daemon;
mod lsp;
mod mcp;
mod search;
mod serve;
mod server;
mod vite;

use config::Config;
use eyre::{Result, WrapErr};
use facet_args as args;
use owo_colors::OwoColorize;
use std::path::PathBuf;
use tracey_core::ReqDefinition;

// Re-export from marq for rule extraction
use marq::{RenderOptions, render};

/// CLI arguments
#[derive(Debug, facet::Facet)]
struct Args {
    /// Subcommand to run
    #[facet(args::subcommand)]
    command: Option<Command>,
}

/// Subcommands
#[derive(Debug, facet::Facet)]
#[repr(u8)]
enum Command {
    /// Start the interactive web dashboard
    Web {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,

        /// Path to config file (default: .config/tracey/config.kdl)
        #[facet(args::named, args::short = 'c', default)]
        config: Option<PathBuf>,

        /// Port to serve on (default: 3000)
        #[facet(args::named, args::short = 'p', default)]
        port: Option<u16>,

        /// Open the dashboard in your browser
        #[facet(args::named, default)]
        open: bool,

        /// Development mode: serve dashboard from Vite dev server
        #[facet(args::named, default)]
        dev: bool,
    },

    /// Start the MCP server for AI assistants
    Mcp {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,

        /// Path to config file (default: .config/tracey/config.kdl)
        #[facet(args::named, args::short = 'c', default)]
        config: Option<PathBuf>,
    },

    /// Start the LSP server for editor integration
    Lsp {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,

        /// Path to config file (default: .config/tracey/config.kdl)
        #[facet(args::named, args::short = 'c', default)]
        config: Option<PathBuf>,
    },

    /// Start the tracey daemon (persistent server for this workspace)
    Daemon {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,

        /// Path to config file (default: .config/tracey/config.kdl)
        #[facet(args::named, args::short = 'c', default)]
        config: Option<PathBuf>,
    },

    /// Start LSP bridge (experimental, requires daemon running)
    LspBridge {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,

        /// Path to config file (default: .config/tracey/config.kdl)
        #[facet(args::named, args::short = 'c', default)]
        config: Option<PathBuf>,
    },

    /// Start HTTP bridge for dashboard (experimental, requires daemon running)
    WebBridge {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,

        /// Path to config file (default: .config/tracey/config.kdl)
        #[facet(args::named, args::short = 'c', default)]
        config: Option<PathBuf>,

        /// Port to listen on (default: 3000)
        #[facet(args::named, args::short = 'p', default)]
        port: Option<u16>,

        /// Open the dashboard in your browser
        #[facet(args::named, default)]
        open: bool,
    },

    /// Show daemon logs
    Logs {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,

        /// Follow log output (like tail -f)
        #[facet(args::named, args::short = 'f', default)]
        follow: bool,

        /// Number of lines to show (default: 50)
        #[facet(args::named, args::short = 'n', default)]
        lines: Option<usize>,
    },

    /// Start MCP bridge (experimental, requires daemon running)
    McpBridge {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,

        /// Path to config file (default: .config/tracey/config.kdl)
        #[facet(args::named, args::short = 'c', default)]
        config: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let args: Args = args::from_std_args()
        .map_err(miette::Report::new)
        .expect("failed to parse arguments");

    match args.command {
        // r[impl cli.web]
        Some(Command::Web {
            root,
            config,
            port,
            open,
            dev,
        }) => serve::run(root, config, port.unwrap_or(3000), open, dev),
        // r[impl cli.mcp]
        Some(Command::Mcp { root, config }) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(mcp::run(root, config))
        }
        // r[impl cli.lsp]
        Some(Command::Lsp { root, config }) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(lsp::run(root, config))
        }
        // r[impl daemon.cli.daemon]
        Some(Command::Daemon { root, config }) => {
            use tracing_subscriber::layer::SubscriberExt;
            use tracing_subscriber::util::SubscriberInitExt;

            let project_root = root.unwrap_or_else(|| find_project_root().unwrap_or_default());
            let config_path =
                config.unwrap_or_else(|| project_root.join(".config/tracey/config.kdl"));

            // Ensure .tracey directory exists for log file
            let tracey_dir = project_root.join(".tracey");
            std::fs::create_dir_all(&tracey_dir)?;

            // r[impl daemon.logs.file]
            // Set up file logging
            let log_path = tracey_dir.join("daemon.log");
            let log_file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)?;

            let filter = tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("tracey=info".parse().unwrap());

            // Create both console and file layers
            let console_layer = tracing_subscriber::fmt::layer().with_ansi(true);
            let file_layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(log_file);

            tracing_subscriber::registry()
                .with(filter)
                .with(console_layer)
                .with(file_layer)
                .init();

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(daemon::run(project_root, config_path))
        }
        // r[impl daemon.bridge.lsp]
        Some(Command::LspBridge { root, config }) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(bridge::lsp::run(root, config))
        }
        // r[impl daemon.bridge.http]
        Some(Command::WebBridge {
            root,
            config,
            port,
            open,
        }) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(bridge::http::run(root, config, port.unwrap_or(3000), open))
        }
        // r[impl daemon.bridge.mcp]
        Some(Command::McpBridge { root, config }) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(bridge::mcp::run(root, config))
        }
        // r[impl cli.logs]
        Some(Command::Logs {
            root,
            follow,
            lines,
        }) => show_logs(root, follow, lines.unwrap_or(50)),
        // r[impl cli.no-args]
        None => {
            print_help();
            Ok(())
        }
    }
}

fn print_help() {
    println!(
        r#"tracey - Measure spec coverage in Rust codebases

{usage}:
    tracey <COMMAND> [OPTIONS]

{commands}:
    {web}       Start the interactive web dashboard
    {mcp}       Start the MCP server for AI assistants
    {lsp}       Start the LSP server for editor integration
    {daemon}    Start the tracey daemon (persistent server)
    {logs}      Show daemon logs

{options}:
    -h, --help      Show this help message

Run 'tracey <COMMAND> --help' for more information on a command."#,
        usage = "Usage".bold(),
        commands = "Commands".bold(),
        web = "web".cyan(),
        mcp = "mcp".cyan(),
        lsp = "lsp".cyan(),
        daemon = "daemon".cyan(),
        logs = "logs".cyan(),
        options = "Options".bold(),
    );
}

/// r[impl cli.logs]
/// Show daemon logs from .tracey/daemon.log
fn show_logs(root: Option<PathBuf>, follow: bool, lines: usize) -> Result<()> {
    use std::io::{BufRead, BufReader, Seek, SeekFrom};

    let project_root = match root {
        Some(r) => r,
        None => find_project_root()?,
    };

    let log_path = project_root.join(".tracey/daemon.log");

    if !log_path.exists() {
        eprintln!(
            "{}: No daemon log found at {}",
            "Warning".yellow(),
            log_path.display()
        );
        eprintln!("Start the daemon with 'tracey daemon' to generate logs.");
        return Ok(());
    }

    let file = std::fs::File::open(&log_path)?;
    let reader = BufReader::new(file);

    // Read the last N lines
    let all_lines: Vec<String> = reader.lines().collect::<std::io::Result<_>>()?;

    let start = all_lines.len().saturating_sub(lines);
    for line in &all_lines[start..] {
        println!("{}", line);
    }

    if follow {
        // Re-open file for following
        let file = std::fs::File::open(&log_path)?;
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::End(0))?;

        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // No new data, sleep briefly
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Ok(_) => {
                    print!("{}", line);
                }
                Err(e) => {
                    eprintln!("Error reading log: {}", e);
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Extracted rule with source location info
pub(crate) struct ExtractedRule {
    pub def: ReqDefinition,
    pub source_file: String,
    /// 1-indexed column where the rule marker starts
    pub column: Option<usize>,
}

/// Compute 1-indexed column from byte offset in content
fn compute_column(content: &str, byte_offset: usize) -> usize {
    // Find the start of the line containing this offset
    let before = &content[..byte_offset.min(content.len())];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    // Column is the number of characters from line start to offset (1-indexed)
    before[line_start..].chars().count() + 1
}

/// Load rules from markdown files matching a glob pattern.
pub(crate) async fn load_rules_from_glob(
    root: &std::path::Path,
    pattern: &str,
    quiet: bool,
) -> Result<Vec<ExtractedRule>> {
    use ignore::WalkBuilder;
    use std::collections::HashSet;

    let mut rules: Vec<ExtractedRule> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    // Walk the directory tree
    let walker = WalkBuilder::new(root)
        .follow_links(true)
        .hidden(false)
        .git_ignore(true)
        .build();

    for entry in walker {
        let entry = entry?;
        let path = entry.path();

        // Only process .md files
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }

        // Check if the path matches the glob pattern
        let relative = path.strip_prefix(root).unwrap_or(path);
        let relative_str = relative.to_string_lossy().to_string();

        if !matches_glob(&relative_str, pattern) {
            continue;
        }

        // Read and render markdown to extract rules with HTML
        let content = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("Failed to read {}", path.display()))?;

        // marq implements markdown rule extraction:
        // r[impl markdown.syntax.marker] - r[rule.id] syntax
        // r[impl markdown.syntax.standalone] - rule on its own line
        // r[impl markdown.syntax.inline-ignored] - inline markers ignored
        // r[impl markdown.syntax.blockquote] - > r[rule.id] for multi-paragraph rules
        let doc = render(&content, &RenderOptions::default())
            .await
            .map_err(|e| eyre::eyre!("Failed to process {}: {}", path.display(), e))?;

        if !doc.reqs.is_empty() {
            if !quiet {
                eprintln!(
                    "   {} {} requirements from {}",
                    "Found".green(),
                    doc.reqs.len(),
                    relative_str
                );
            }

            // Check for duplicates
            // r[impl markdown.duplicates.same-file] - caught when marq returns duplicate reqs from single file
            // r[impl markdown.duplicates.cross-file] - caught via seen_ids persisting across files
            for req in &doc.reqs {
                if seen_ids.contains(&req.id) {
                    eyre::bail!(
                        "Duplicate requirement '{}' found in {}",
                        req.id.red(),
                        relative_str
                    );
                }
                seen_ids.insert(req.id.clone());
            }

            // Add requirements with their source file and computed column
            for req in doc.reqs {
                let column = Some(compute_column(&content, req.span.offset));
                rules.push(ExtractedRule {
                    def: req,
                    source_file: relative_str.clone(),
                    column,
                });
            }
        }
    }

    Ok(rules)
}

/// Load rules from multiple glob patterns
pub(crate) async fn load_rules_from_globs(
    root: &std::path::Path,
    patterns: &[&str],
    quiet: bool,
) -> Result<Vec<ExtractedRule>> {
    use std::collections::HashSet;

    let mut all_rules: Vec<ExtractedRule> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    for pattern in patterns {
        let rules = load_rules_from_glob(root, pattern, quiet).await?;

        // r[impl validation.duplicates]
        // Check for duplicates across patterns
        for extracted in rules {
            if seen_ids.contains(&extracted.def.id) {
                eyre::bail!(
                    "Duplicate requirement '{}' found in {}",
                    extracted.def.id.red(),
                    extracted.source_file
                );
            }
            seen_ids.insert(extracted.def.id.clone());
            all_rules.push(extracted);
        }
    }

    Ok(all_rules)
}

/// Simple glob pattern matching
fn matches_glob(path: &str, pattern: &str) -> bool {
    // Make path separators consistent in case of windows
    let path = path.replace('\\', "/");
    let pattern = pattern.replace('\\', "/");

    // Handle **/*.md pattern
    if pattern == "**/*.md" {
        return path.ends_with(".md");
    }

    // Handle prefix/**/*.md patterns like "docs/**/*.md"
    if let Some(rest) = pattern.strip_suffix("/**/*.md") {
        return path.starts_with(rest) && path.ends_with(".md");
    }

    // Handle prefix/** patterns
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path.starts_with(prefix);
    }

    // Handle exact matches
    if !pattern.contains('*') {
        return path == pattern;
    }

    // Fallback: simple contains check for the non-wildcard parts
    let parts: Vec<&str> = pattern.split('*').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return true;
    }

    let mut remaining = path.as_str();
    for part in parts {
        if let Some(idx) = remaining.find(part) {
            remaining = &remaining[idx + part.len()..];
        } else {
            return false;
        }
    }

    true
}

pub(crate) fn find_project_root() -> Result<PathBuf> {
    let mut current = std::env::current_dir()?;

    loop {
        if current.join("Cargo.toml").exists() {
            return Ok(current);
        }

        if !current.pop() {
            // No Cargo.toml found, use current directory
            return std::env::current_dir().wrap_err("Failed to get current directory");
        }
    }
}

pub(crate) fn load_config(path: &PathBuf) -> Result<Config> {
    if !path.exists() {
        eyre::bail!(
            "Config file not found at {}\n\n\
             Create a config file with your spec configuration:\n\n\
             spec {{\n    \
                 name \"my-spec\"\n    \
                 prefix \"r\"\n    \
                 include \"docs/**/*.md\"\n\n    \
                 impl {{\n        \
                     name \"main\"\n        \
                     include \"src/**/*.rs\"\n    \
                 }}\n\
             }}",
            path.display()
        );
    }

    let content = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("Failed to read config file: {}", path.display()))?;

    let config: Config = facet_kdl::from_str(&content)
        .wrap_err_with(|| format!("Failed to parse config file: {}", path.display()))?;

    Ok(config)
}

/// r[impl config.optional]
/// Load config if it exists, otherwise return default empty config.
/// This allows services to start without a config file.
pub(crate) fn load_config_or_default(path: &PathBuf) -> Config {
    if !path.exists() {
        return Config::default();
    }

    match std::fs::read_to_string(path) {
        Ok(content) => facet_kdl::from_str(&content).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}
