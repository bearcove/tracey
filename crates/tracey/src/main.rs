//! tracey - Measure spec coverage in Rust codebases
//!
//! tracey parses Rust source files to find references to specification rules
//! (in the format `[rule.id]` in comments) and compares them against a spec
//! manifest to produce coverage reports.

mod config;
mod search;
mod serve;
mod vite;

use config::Config;
use eyre::{Result, WrapErr};
use facet_args as args;
use owo_colors::OwoColorize;
use std::path::PathBuf;
use tracey_core::RuleDefinition;

// Re-export from bearmark for rule extraction
use bearmark::{RenderOptions, render};

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
    Serve {
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
}

fn main() -> Result<()> {
    let args: Args = args::from_std_args().expect("failed to parse arguments");

    match args.command {
        Some(Command::Serve {
            root,
            config,
            port,
            open,
            dev,
        }) => serve::run(root, config, port.unwrap_or(3000), open, dev),
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
    {serve}     Start the interactive web dashboard

{options}:
    -h, --help      Show this help message

Run 'tracey <COMMAND> --help' for more information on a command."#,
        usage = "Usage".bold(),
        commands = "Commands".bold(),
        serve = "serve".cyan(),
        options = "Options".bold(),
    );
}

/// Load rules from markdown files matching a glob pattern.
/// Returns a Vec of (RuleDefinition, source_file) tuples.
pub(crate) async fn load_rules_from_glob(
    root: &std::path::Path,
    pattern: &str,
) -> Result<Vec<(RuleDefinition, String)>> {
    use ignore::WalkBuilder;
    use std::collections::HashSet;

    let mut rules: Vec<(RuleDefinition, String)> = Vec::new();
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

        let doc = render(&content, &RenderOptions::default())
            .await
            .map_err(|e| eyre::eyre!("Failed to process {}: {}", path.display(), e))?;

        if !doc.rules.is_empty() {
            eprintln!(
                "   {} {} rules from {}",
                "Found".green(),
                doc.rules.len(),
                relative_str
            );

            // Check for duplicates
            for rule in &doc.rules {
                if seen_ids.contains(&rule.id) {
                    eyre::bail!(
                        "Duplicate rule '{}' found in {}",
                        rule.id.red(),
                        relative_str
                    );
                }
                seen_ids.insert(rule.id.clone());
            }

            // Add rules with their source file
            for rule in doc.rules {
                rules.push((rule, relative_str.clone()));
            }
        }
    }

    Ok(rules)
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
                 rules_glob \"docs/**/*.md\"\n\
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
