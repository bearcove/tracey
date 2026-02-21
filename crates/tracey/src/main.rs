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

    /// Run query subcommands over daemon data from the terminal
    Query {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,

        /// Query command to run
        #[facet(args::subcommand)]
        query: QueryCommand,
    },

    /// Check staged spec changes and fail if any rule text changed without a version bump.
    /// Designed to be installed as a git pre-commit hook.
    PreCommit {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,

        /// Path to config file
        #[facet(args::named, args::short = 'c', default = ".config/tracey/config.styx")]
        config: PathBuf,
    },

    /// Bump version numbers of staged rules whose text changed, then re-stage the files.
    Bump {
        /// Project root directory (default: current directory)
        #[facet(args::positional, default)]
        root: Option<PathBuf>,

        /// Path to config file
        #[facet(args::named, args::short = 'c', default = ".config/tracey/config.styx")]
        config: PathBuf,
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

#[derive(Debug, facet::Facet)]
#[repr(u8)]
enum QueryCommand {
    /// coverage overview
    Status,

    /// List rules without implementation references
    Uncovered {
        /// Spec/impl to query (e.g., "my-spec/rust"). Optional if only one exists.
        #[facet(args::named, default)]
        spec_impl: Option<String>,

        /// Filter by rule ID prefix
        #[facet(args::named, default)]
        prefix: Option<String>,
    },

    /// List rules without verification references
    Untested {
        /// Spec/impl to query (e.g., "my-spec/rust"). Optional if only one exists.
        #[facet(args::named, default)]
        spec_impl: Option<String>,

        /// Filter by rule ID prefix
        #[facet(args::named, default)]
        prefix: Option<String>,
    },

    /// Show unmapped code units
    Unmapped {
        /// Spec/impl to query (e.g., "my-spec/rust"). Optional if only one exists.
        #[facet(args::named, default)]
        spec_impl: Option<String>,

        /// Directory or file path to zoom into
        #[facet(args::named, default)]
        path: Option<String>,
    },

    /// List stale references (code pointing to older rule versions)
    Stale {
        /// Spec/impl to query (e.g., "my-spec/rust"). Optional if only one exists.
        #[facet(args::named, default)]
        spec_impl: Option<String>,

        /// Filter by rule ID prefix
        #[facet(args::named, default)]
        prefix: Option<String>,
    },

    /// Show details about a specific rule
    Rule {
        /// Rule identifier to inspect
        #[facet(args::positional)]
        rule_id: String,
    },

    /// Display current configuration
    Config,

    /// Validate the spec and implementation
    Validate {
        /// Spec/impl to validate (e.g., "my-spec/rust"). Optional if only one exists.
        #[facet(args::named, default)]
        spec_impl: Option<String>,
    },
}

// Embed the config schema for zero-execution discovery by styx tooling
styx_embed::embed_outdir_file!("schema.styx");

#[tokio::main]
async fn main() -> Result<()> {
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
            init_tracing(TracingConfig {
                log_file: None,
                enable_console: true,
                console_ansi: true,
                default_filter: "tracey=info",
            })?;
            bridge::http::run(root, config, port, open, dev).await
        }
        // r[impl cli.mcp]
        // r[impl daemon.cli.mcp]
        Command::Mcp { root, config } => {
            let project_root = root.unwrap_or_else(|| find_project_root().unwrap_or_default());
            let log_path = bridge_log_path(&project_root, "mcp");
            write_bridge_start_marker(&log_path, "mcp", &project_root, &config)?;
            // MCP communicates over stdio, so logging must stay off stdio.
            init_tracing(TracingConfig {
                log_file: Some(log_path.clone()),
                enable_console: false,
                console_ansi: false,
                default_filter: "tracey=info",
            })?;
            tracing::info!(
                pid = std::process::id(),
                command = "mcp",
                project_root = %project_root.display(),
                config = %config.display(),
                log_file = %log_path.display(),
                "starting tracey bridge"
            );
            bridge::mcp::run(Some(project_root), config).await
        }
        // r[impl daemon.cli.lsp]
        Command::Lsp { root, config } => {
            let project_root = root.unwrap_or_else(|| find_project_root().unwrap_or_default());
            // LSP uses stdout for the wire protocol, so logs go to stderr.
            init_tracing(TracingConfig {
                log_file: None,
                enable_console: true,
                console_ansi: false,
                default_filter: "tracey=debug",
            })?;
            tracing::info!(
                pid = std::process::id(),
                command = "lsp",
                project_root = %project_root.display(),
                config = %config.display(),
                "starting tracey bridge"
            );
            bridge::lsp::run(Some(project_root), config).await
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
                enable_console: true,
                console_ansi: true,
                default_filter: "tracey=info",
            })?;

            daemon::run(project_root, config_path).await
        }
        // r[impl daemon.cli.logs]
        Command::Logs {
            root,
            follow,
            lines,
        } => show_logs(root, follow, lines.unwrap_or(50)),
        // r[impl daemon.cli.status]
        Command::Status { root } => show_status(root).await,
        // r[impl daemon.cli.kill]
        Command::Kill { root } => kill_daemon(root).await,

        // r[impl cli.skill.install]
        Command::Skill { action } => match action {
            SkillAction::Install { claude, codex } => install_skill(claude, codex),
        },

        // r[impl cli.pre-commit]
        Command::PreCommit { root, config } => {
            let project_root = root.unwrap_or_else(|| find_project_root().unwrap_or_default());
            let config_path = project_root.join(&config);
            let cfg = load_bump_config(&config_path);
            let passed = tracey::bump::pre_commit(&project_root, &cfg).await?;
            if !passed {
                std::process::exit(1);
            }
            Ok(())
        }

        // r[impl cli.bump]
        Command::Bump { root, config } => {
            let project_root = root.unwrap_or_else(|| find_project_root().unwrap_or_default());
            let config_path = project_root.join(&config);
            let cfg = load_bump_config(&config_path);
            let bumped = tracey::bump::bump(&project_root, &cfg).await?;
            if bumped.is_empty() {
                println!("No staged rule changes require a version bump.");
            } else {
                println!("Bumped {} rule(s):", bumped.len());
                for id in &bumped {
                    println!("  {id}");
                }
                println!();
                println!("Affected spec files have been re-staged. Review and commit.");
            }
            Ok(())
        }

        // r[impl daemon.cli.query]
        Command::Query { root, query } => {
            let project_root = root.unwrap_or_else(|| find_project_root().unwrap_or_default());
            let query_client =
                bridge::query::QueryClient::new(project_root, bridge::query::Caller::Cli);
            init_tracing(TracingConfig {
                log_file: None,
                enable_console: true,
                console_ansi: true,
                default_filter: "tracey=info",
            })?;

            let output = match query {
                QueryCommand::Status => query_client.status().await,
                QueryCommand::Uncovered { spec_impl, prefix } => {
                    query_client
                        .uncovered(spec_impl.as_deref(), prefix.as_deref())
                        .await
                }
                QueryCommand::Untested { spec_impl, prefix } => {
                    query_client
                        .untested(spec_impl.as_deref(), prefix.as_deref())
                        .await
                }
                QueryCommand::Unmapped { spec_impl, path } => {
                    query_client
                        .unmapped(spec_impl.as_deref(), path.as_deref())
                        .await
                }
                QueryCommand::Stale { spec_impl, prefix } => {
                    query_client
                        .stale(spec_impl.as_deref(), prefix.as_deref())
                        .await
                }
                QueryCommand::Rule { rule_id } => query_client.rule(&rule_id).await,
                QueryCommand::Config => query_client.config().await,
                QueryCommand::Validate { spec_impl } => {
                    query_client.validate(spec_impl.as_deref()).await
                }
            };

            println!("{}", output);
            Ok(())
        }
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
    /// If true, emit logs to console (stderr).
    enable_console: bool,
    /// If true, include ANSI color codes in console logs.
    console_ansi: bool,
    /// Default filter directive if RUST_LOG is not set.
    default_filter: &'static str,
}

/// Initialize tracing with optional file logging.
fn init_tracing(config: TracingConfig) -> Result<()> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    // Use RUST_LOG from environment, default to info if not set
    let filter = match std::env::var("RUST_LOG") {
        Ok(_) => tracing_subscriber::EnvFilter::from_default_env(),
        Err(_) => tracing_subscriber::EnvFilter::new(config.default_filter),
    };

    let console_layer = config.enable_console.then(|| {
        tracing_subscriber::fmt::layer()
            .with_ansi(config.console_ansi)
            .with_writer(std::io::stderr)
    });

    let file_layer = if let Some(log_path) = config.log_file {
        // Ensure parent directory exists
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;

        Some(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(log_file),
        )
    } else {
        None
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    Ok(())
}

/// Build a bridge log path with process ID in the filename.
fn bridge_log_path(project_root: &std::path::Path, bridge: &str) -> PathBuf {
    project_root
        .join(".tracey")
        .join(format!("{bridge}-{}.log", std::process::id()))
}

/// Write a startup marker so bridge launches are visible even before first tracing event.
fn write_bridge_start_marker(
    log_path: &std::path::Path,
    command: &str,
    project_root: &std::path::Path,
    config_path: &std::path::Path,
) -> Result<()> {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    writeln!(
        log_file,
        "[ts={} pid={}] starting {} root={} config={} cwd={}",
        now,
        std::process::id(),
        command,
        project_root.display(),
        config_path.display(),
        std::env::current_dir()?.display()
    )?;

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
    use roam_stream::{Connector, HandshakeConfig, NoDispatcher, connect};
    use std::time::Duration;

    let project_root = match root {
        Some(r) => r,
        None => find_project_root()?,
    };

    let endpoint = daemon::local_endpoint(&project_root);

    // Try to connect without auto-starting
    let stream = match roam_local::connect(&endpoint).await {
        Ok(s) => s,
        Err(_) => {
            println!("{}: No daemon running", "Status".yellow());
            return Ok(());
        }
    };

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

    let client = tracey_proto::TraceyDaemonClient::new(connect(
        DirectConnector {
            stream: std::sync::Mutex::new(Some(stream)),
        },
        HandshakeConfig::default(),
        NoDispatcher,
    ));

    match tokio::time::timeout(Duration::from_secs(1), client.health()).await {
        Ok(Ok(health)) => {
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
        Ok(Err(e)) => {
            println!("{}: Daemon connection failed", "Status".red());
            println!("  Error: {e}");
        }
        Err(_) => {
            println!(
                "{}: Daemon not responding (health check timed out)",
                "Status".yellow()
            );
            println!("  The daemon may be stuck. Run 'tracey kill' to restart it.");
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

/// Load config for pre-commit / bump: silently no-op if missing, warn if unparseable.
fn load_bump_config(config_path: &std::path::Path) -> tracey::config::Config {
    if !config_path.exists() {
        return tracey::config::Config::default();
    }
    match tracey::load_config(&config_path.to_path_buf()) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!(
                "warning: failed to parse tracey config at {}: {e}",
                config_path.display()
            );
            tracey::config::Config::default()
        }
    }
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
