//! tracey - Measure spec coverage in Rust codebases
//!
//! tracey parses Rust source files to find references to specification rules
//! (in the format `[rule.id]` in comments) and compares them against a spec
//! manifest to produce coverage reports.

mod config;
mod errors;
mod manifest;
mod search;
mod serve;
mod vite;

use config::Config;
use eyre::{Result, WrapErr};
use facet_args as args;
use manifest::RulesManifest;
use owo_colors::OwoColorize;
use std::path::PathBuf;
use tracey_core::SpecManifest;

// Re-export from bearmark for rule extraction
use bearmark::{RuleWarningKind, extract_rules_only};

/// CLI arguments
#[derive(Debug, facet::Facet)]
struct Args {
    /// Subcommand to run
    #[facet(subcommand)]
    command: Option<Command>,
}

/// Subcommands
#[derive(Debug, facet::Facet)]
#[repr(u8)]
enum Command {
    /// Start the interactive web dashboard
    Serve {
        /// Project root directory (default: current directory)
        #[facet(positional, default)]
        root: Option<PathBuf>,

        /// Path to config file (default: .config/tracey/config.kdl)
        #[facet(named, short = 'c', default)]
        config: Option<PathBuf>,

        /// Port to serve on (default: 3000)
        #[facet(named, short = 'p', default)]
        port: Option<u16>,

        /// Open the dashboard in your browser
        #[facet(named, default)]
        open: bool,

        /// Development mode: serve dashboard from Vite dev server
        #[facet(named, default)]
        dev: bool,
    },
}

fn main() -> Result<()> {
    let args: Args =
        args::from_slice(&std::env::args().collect::<Vec<_>>()).expect("failed to parse arguments");

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

pub(crate) fn load_manifest_from_glob(
    root: &std::path::Path,
    pattern: &str,
) -> Result<SpecManifest> {
    use ignore::WalkBuilder;
    use std::collections::HashMap;

    let mut rules_manifest = RulesManifest::new();

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
        let relative_str = relative.to_string_lossy();

        if !matches_glob(&relative_str, pattern) {
            continue;
        }

        // Read and extract rules
        let content = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("Failed to read {}", path.display()))?;

        let result = extract_rules_only(&content, Some(path))
            .map_err(|e| eyre::eyre!("Failed to process {}: {}", path.display(), e))?;

        // Display warnings for rule quality issues
        for warning in &result.warnings {
            let message = match &warning.kind {
                RuleWarningKind::NoRfc2119Keyword => "no RFC 2119 keyword".to_string(),
                RuleWarningKind::NegativeRequirement(kw) => {
                    format!("{} — negative requirements are hard to test", kw.as_str())
                }
            };
            eprintln!(
                "   {} {}:{} {} - {}",
                "!".yellow(),
                relative_str,
                warning.line,
                warning.rule_id.yellow(),
                message
            );
        }

        if !result.rules.is_empty() {
            eprintln!(
                "   {} {} rules from {}",
                "Found".green(),
                result.rules.len(),
                relative_str
            );

            // Build manifest for this file (no base URL needed for coverage checking)
            let file_manifest = RulesManifest::from_rules(&result.rules, "", &relative_str);
            let duplicates = rules_manifest.merge(&file_manifest);

            if !duplicates.is_empty() {
                for dup in &duplicates {
                    eprintln!(
                        "   {} Duplicate rule '{}' in {}",
                        "!".yellow().bold(),
                        dup.id.red(),
                        relative_str
                    );
                }
                eyre::bail!(
                    "Found {} duplicate rule IDs in markdown files",
                    duplicates.len()
                );
            }
        }
    }

    // Convert BTreeMap to HashMap for SpecManifest
    let spec_rules: HashMap<String, tracey_core::RuleInfo> =
        rules_manifest.rules.into_iter().collect();

    Ok(SpecManifest { rules: spec_rules })
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
                 rules_url \"https://example.com/_rules.json\"\n\
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
