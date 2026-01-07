//! tracey - Measure spec coverage in Rust codebases
//!
//! tracey parses Rust source files to find references to specification rules
//! (in the format `[rule.id]` in comments) and compares them against a spec
//! manifest to produce coverage reports.

use eyre::Result;
use facet_args as args;
use owo_colors::OwoColorize;
use std::path::PathBuf;

// Use the library crate
use tracey::{bridge, daemon, find_project_root};

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

        /// Port to listen on (default: 3000)
        #[facet(args::named, args::short = 'p', default)]
        port: Option<u16>,

        /// Open the dashboard in your browser
        #[facet(args::named, default)]
        open: bool,
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
}

fn main() -> Result<()> {
    let args: Args = args::from_std_args()
        .map_err(miette::Report::new)
        .expect("failed to parse arguments");

    match args.command {
        // r[impl cli.web]
        // r[impl daemon.cli.web]
        Some(Command::Web {
            root,
            config,
            port,
            open,
        }) => {
            init_tracing();
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(bridge::http::run(root, config, port.unwrap_or(3000), open))
        }
        // r[impl cli.mcp]
        // r[impl daemon.cli.mcp]
        Some(Command::Mcp { root, config }) => {
            // MCP communicates over stdio, so no tracing to stdout
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(bridge::mcp::run(root, config))
        }
        // r[impl cli.lsp]
        // r[impl daemon.cli.lsp]
        Some(Command::Lsp { root, config }) => {
            // LSP communicates over stdio, so no tracing to stdout
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(bridge::lsp::run(root, config))
        }
        // r[impl daemon.cli.daemon]
        Some(Command::Daemon { root, config }) => {
            use tracing_subscriber::layer::SubscriberExt;
            use tracing_subscriber::util::SubscriberInitExt;

            let project_root = root.unwrap_or_else(|| find_project_root().unwrap_or_default());
            // r[impl config.path.default]
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

            // Use RUST_LOG from environment, default to info if not set
            // Crash on invalid RUST_LOG - don't silently fall back
            let filter = match std::env::var("RUST_LOG") {
                Ok(_) => tracing_subscriber::EnvFilter::from_default_env(),
                Err(_) => tracing_subscriber::EnvFilter::new("tracey=info"),
            };

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
        // r[impl daemon.cli.logs]
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

/// Initialize tracing for bridges (console output only).
fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("tracey=info".parse().unwrap()),
        )
        .init();
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

    // r[impl daemon.cli.logs.lines]
    // Read the last N lines
    let all_lines: Vec<String> = reader.lines().collect::<std::io::Result<_>>()?;

    let start = all_lines.len().saturating_sub(lines);
    for line in &all_lines[start..] {
        println!("{}", line);
    }

    // r[impl daemon.cli.logs.follow]
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
