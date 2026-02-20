//! tracey - Measure spec coverage in Rust codebases
//!
//! tracey parses Rust source files to find references to specification rules
//! (in the format `[rule.id]` in comments) and compares them against a spec
//! manifest to produce coverage reports.

use eyre::{Result, WrapErr, eyre};
use figue::{self as args, FigueBuiltins};
use owo_colors::OwoColorize;
use std::path::PathBuf;

// Use the library crate
use tracey::{bridge, daemon, find_project_root};

/// CLI arguments
#[derive(Debug, facet::Facet)]
struct Args {
    /// Subcommand to run
    #[facet(args::subcommand)]
    command: Command,

    /// Standard CLI builtins (--help, --version, --completions)
    #[facet(flatten)]
    builtins: FigueBuiltins,
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

        /// Path to config file
        #[facet(args::named, args::short = 'c', default = ".config/tracey/config.styx")]
        config: PathBuf,

        /// Port to listen on (default: 3000)
        #[facet(args::named, args::short = 'p', default)]
        port: Option<u16>,

        /// Open the dashboard in your browser
        #[facet(args::named, default)]
        open: bool,

        /// Development mode: proxy assets from Vite dev server instead of serving embedded assets
        #[facet(args::named, default)]
        dev: bool,
    },

    /// Start the MCP server for AI assistants
    Mcp {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,

        /// Path to config file
        #[facet(args::named, args::short = 'c', default = ".config/tracey/config.styx")]
        config: PathBuf,
    },

    /// Start the LSP server for editor integration
    Lsp {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,

        /// Path to config file
        #[facet(args::named, args::short = 'c', default = ".config/tracey/config.styx")]
        config: PathBuf,
    },

    /// Start the tracey daemon (persistent server for this workspace)
    Daemon {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,

        /// Path to config file
        #[facet(args::named, args::short = 'c', default = ".config/tracey/config.styx")]
        config: PathBuf,
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

    /// Show daemon status
    Status {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,
    },

    /// Stop the running daemon
    Kill {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,
    },

    /// Manage the bundled AI skill
    Skill {
        /// Skill action to perform
        #[facet(args::subcommand)]
        action: SkillAction,
    },
}

/// Skill subcommands
#[derive(Debug, facet::Facet)]
#[repr(u8)]
enum SkillAction {
    /// Install the bundled Tracey skill for Claude and/or Codex
    Install {
        /// Install only for Claude Code
        #[facet(args::named, default)]
        claude: bool,

        /// Install only for Codex CLI
        #[facet(args::named, default)]
        codex: bool,
    },
}

// Embed the config schema for zero-execution discovery by styx tooling
styx_embed::embed_outdir_file!("schema.styx");

fn main() -> Result<()> {
    let config = args::builder::<Args>()
        .map_err(|e| eyre!("failed to initialize CLI parser: {e}"))?
        .cli(|cli| cli.args(std::env::args().skip(1)))
        .help(|h| {
            h.program_name(env!("CARGO_PKG_NAME"))
                .version(cli_version_text())
        })
        .build();
    let args: Args = args::Driver::new(config).run().unwrap();

    match args.command {
        // r[impl cli.web]
        // r[impl daemon.cli.web]
        Command::Web {
            root,
            config,
            port,
            open,
            dev,
        } => {
            init_tracing(TracingConfig { log_file: None })?;
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(bridge::http::run(
                root,
                config,
                port.unwrap_or(3000),
                open,
                dev,
            ))
        }
        // r[impl cli.mcp]
        // r[impl daemon.cli.mcp]
        Command::Mcp { root, config } => {
            // MCP communicates over stdio, so no tracing to stdout
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(bridge::mcp::run(root, config))
        }
        // r[impl daemon.cli.lsp]
        Command::Lsp { root, config } => {
            // LSP communicates over stdio, so no tracing to stdout
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(bridge::lsp::run(root, config))
        }
        // r[impl daemon.cli.daemon]
        Command::Daemon { root, config } => {
            let project_root = root.unwrap_or_else(|| find_project_root().unwrap_or_default());
            // r[impl config.path.default]
            let config_path = project_root.join(&config);

            // r[impl daemon.logs.file]
            let log_path = project_root.join(".tracey/daemon.log");
            init_tracing(TracingConfig {
                log_file: Some(log_path),
            })?;

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(daemon::run(project_root, config_path))
        }
        // r[impl daemon.cli.logs]
        Command::Logs {
            root,
            follow,
            lines,
        } => show_logs(root, follow, lines.unwrap_or(50)),
        // r[impl daemon.cli.status]
        Command::Status { root } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(show_status(root))
        }
        // r[impl daemon.cli.kill]
        Command::Kill { root } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(kill_daemon(root))
        }
        // r[impl cli.skill.install]
        Command::Skill { action } => match action {
            SkillAction::Install { claude, codex } => install_skill(claude, codex),
        },
    }
}

fn cli_version_text() -> String {
    let mut version = env!("CARGO_PKG_VERSION").to_string();

    if let Some(git_commit) = option_env!("TRACEY_GIT_COMMIT") {
        version.push_str(" (");
        version.push_str(git_commit);

        if let Some(build_date) = option_env!("TRACEY_BUILD_DATE") {
            version.push(' ');
            version.push_str(build_date);
        }

        version.push(')');
    } else if let Some(build_date) = option_env!("TRACEY_BUILD_DATE") {
        version.push_str(" (built ");
        version.push_str(build_date);
        version.push(')');
    }

    version
}

/// Configuration for tracing initialization.
struct TracingConfig {
    /// If Some, also log to this file (creating parent dirs as needed).
    log_file: Option<PathBuf>,
}

/// Initialize tracing with optional file logging.
fn init_tracing(config: TracingConfig) -> Result<()> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    // Use RUST_LOG from environment, default to info if not set
    let filter = match std::env::var("RUST_LOG") {
        Ok(_) => tracing_subscriber::EnvFilter::from_default_env(),
        Err(_) => tracing_subscriber::EnvFilter::new("tracey=info"),
    };

    let console_layer = tracing_subscriber::fmt::layer().with_ansi(true);

    if let Some(log_path) = config.log_file {
        // Ensure parent directory exists
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;

        let file_layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(log_file);

        tracing_subscriber::registry()
            .with(filter)
            .with(console_layer)
            .with(file_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(console_layer)
            .init();
    }

    Ok(())
}

/// r[impl daemon.cli.logs]
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

/// r[impl daemon.cli.status]
/// Show daemon status by connecting and calling health()
async fn show_status(root: Option<PathBuf>) -> Result<()> {
    let project_root = match root {
        Some(r) => r,
        None => find_project_root()?,
    };

    let endpoint = daemon::local_endpoint(&project_root);

    // Check if endpoint exists
    if !roam_local::endpoint_exists(&endpoint) {
        println!("{}: No daemon running", "Status".yellow());
        #[cfg(unix)]
        println!("  Socket: {} (not found)", endpoint.display());
        #[cfg(windows)]
        println!("  Endpoint: {} (not found)", endpoint);
        return Ok(());
    }

    // Try to connect without auto-starting
    match roam_local::connect(&endpoint).await {
        Ok(stream) => {
            // Create a minimal client to call health()
            use roam_stream::{Connector, HandshakeConfig, NoDispatcher, connect};

            struct DirectConnector {
                stream: std::sync::Mutex<Option<roam_local::LocalStream>>,
            }

            impl Connector for DirectConnector {
                type Transport = roam_local::LocalStream;
                async fn connect(&self) -> std::io::Result<Self::Transport> {
                    self.stream
                        .lock()
                        .unwrap()
                        .take()
                        .ok_or_else(|| std::io::Error::other("already connected"))
                }
            }

            let connector = DirectConnector {
                stream: std::sync::Mutex::new(Some(stream)),
            };
            let client = connect(connector, HandshakeConfig::default(), NoDispatcher);
            let client = tracey_proto::TraceyDaemonClient::new(client);

            match client.health().await {
                Ok(health) => {
                    println!("{}: Daemon is running", "Status".green());
                    println!("  Uptime: {}s", health.uptime_secs);
                    println!("  Data version: {}", health.version);
                    println!(
                        "  Watcher: {}",
                        if health.watcher_active {
                            "active".green().to_string()
                        } else {
                            "inactive".yellow().to_string()
                        }
                    );
                    if let Some(err) = &health.watcher_error {
                        println!("  Watcher error: {}", err.as_str().red());
                    }
                    if let Some(err) = &health.config_error {
                        println!("  Config error: {}", err.as_str().red());
                    }
                    println!("  File events: {}", health.watcher_event_count);
                    println!("  Watched dirs: {}", health.watched_directories.len());
                }
                Err(e) => {
                    println!("{}: Daemon connection failed", "Status".red());
                    println!("  Error: {}", e);
                }
            }
        }
        Err(_) => {
            println!("{}: Daemon not responding", "Status".yellow());
            #[cfg(unix)]
            println!(
                "  Socket exists at {} but cannot connect",
                endpoint.display()
            );
            #[cfg(windows)]
            println!("  Endpoint exists but cannot connect");
            println!("  The daemon may have crashed. Run 'tracey kill' to clean up.");
        }
    }

    Ok(())
}

const SKILL_MD: &str = include_str!("../../../skill/SKILL.md");
const SPEC_MD: &str = include_str!("../../../skill/references/tracey-spec.md");

/// r[impl cli.skill.install]
/// Install the bundled Tracey skill for Claude and/or Codex
fn install_skill(claude_only: bool, codex_only: bool) -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| eyre!("could not determine home directory"))?;

    // If neither flag is set, install for both
    let install_claude = !codex_only;
    let install_codex = !claude_only;

    let mut installed = Vec::new();

    if install_claude {
        let skill_dir = home.join(".claude/skills/tracey");
        install_skill_to(&skill_dir)?;
        installed.push(skill_dir);
    }

    if install_codex {
        let skill_dir = home.join(".codex/skills/tracey");
        install_skill_to(&skill_dir)?;
        installed.push(skill_dir);
    }

    println!("{}: Tracey skill installed", "Success".green());
    for path in &installed {
        println!("  {}", path.display());
    }

    Ok(())
}

fn install_skill_to(skill_dir: &std::path::Path) -> Result<()> {
    let refs_dir = skill_dir.join("references");
    std::fs::create_dir_all(&refs_dir)
        .wrap_err_with(|| format!("failed to create {}", refs_dir.display()))?;

    std::fs::write(skill_dir.join("SKILL.md"), SKILL_MD)
        .wrap_err_with(|| format!("failed to write {}", skill_dir.join("SKILL.md").display()))?;

    std::fs::write(refs_dir.join("tracey-spec.md"), SPEC_MD).wrap_err_with(|| {
        format!(
            "failed to write {}",
            refs_dir.join("tracey-spec.md").display()
        )
    })?;

    Ok(())
}

/// r[impl daemon.cli.kill]
/// Kill the running daemon by sending a shutdown request
async fn kill_daemon(root: Option<PathBuf>) -> Result<()> {
    let project_root = match root {
        Some(r) => r,
        None => find_project_root()?,
    };

    let endpoint = daemon::local_endpoint(&project_root);

    // Check if endpoint exists
    if !roam_local::endpoint_exists(&endpoint) {
        println!("{}: No daemon running", "Info".cyan());
        return Ok(());
    }

    // Try to connect and send shutdown
    match roam_local::connect(&endpoint).await {
        Ok(stream) => {
            use roam_stream::{Connector, HandshakeConfig, NoDispatcher, connect};

            struct DirectConnector {
                stream: std::sync::Mutex<Option<roam_local::LocalStream>>,
            }

            impl Connector for DirectConnector {
                type Transport = roam_local::LocalStream;
                async fn connect(&self) -> std::io::Result<Self::Transport> {
                    self.stream
                        .lock()
                        .unwrap()
                        .take()
                        .ok_or_else(|| std::io::Error::other("already connected"))
                }
            }

            let connector = DirectConnector {
                stream: std::sync::Mutex::new(Some(stream)),
            };
            let client = connect(connector, HandshakeConfig::default(), NoDispatcher);
            let client = tracey_proto::TraceyDaemonClient::new(client);

            match client.shutdown().await {
                Ok(()) => {
                    println!("{}: Shutdown signal sent", "Success".green());
                }
                Err(e) => {
                    // Connection may close before we get a response, that's OK
                    let err_str = e.to_string();
                    if err_str.contains("closed") {
                        println!("{}: Daemon stopped", "Success".green());
                    } else {
                        println!(
                            "{}: Error sending shutdown: {}",
                            "Warning".yellow(),
                            err_str
                        );
                    }
                }
            }
        }
        Err(_) => {
            // Socket exists but can't connect - clean it up
            println!(
                "{}: Daemon not responding, cleaning up stale socket",
                "Info".cyan()
            );
            let _ = roam_local::remove_endpoint(&endpoint);
            println!("{}: Cleaned up", "Success".green());
        }
    }

    Ok(())
}
