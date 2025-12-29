//! tracey - Measure spec coverage in Rust codebases
//!
//! tracey parses Rust source files to find references to specification rules
//! (in the format `[rule.id]` in comments) and compares them against a spec
//! manifest to produce coverage reports.

mod config;
mod errors;
mod output;

use config::Config;
use eyre::{Result, WrapErr};
use facet_args as args;
use output::{OutputFormat, render_report};
use owo_colors::OwoColorize;
use std::path::PathBuf;
use tracey_core::{CoverageReport, Rules, SpecManifest, WalkSources};
use tracey_core::markdown::{MarkdownProcessor, RulesManifest};

/// CLI arguments
#[derive(Debug, facet::Facet)]
struct Args {
    /// Subcommand to run
    #[facet(args::subcommand)]
    command: Option<Command>,

    /// Path to config file (default: .config/tracey/config.kdl)
    #[facet(args::named, args::short = 'c', default)]
    config: Option<PathBuf>,

    /// Only check, don't print detailed report (exit 1 if failing)
    #[facet(args::named, default)]
    check: bool,

    /// Minimum coverage percentage to pass (default: 0)
    #[facet(args::named, default)]
    threshold: Option<f64>,

    /// Show verbose output including all references
    #[facet(args::named, args::short = 'v', default)]
    verbose: bool,

    /// Output format: text, json, markdown, html
    #[facet(args::named, args::short = 'f', default)]
    format: Option<String>,
}

/// Subcommands
#[derive(Debug, facet::Facet)]
#[repr(u8)]
enum Command {
    /// Extract rules from markdown spec documents and generate _rules.json
    Rules {
        /// Markdown files to process
        #[facet(args::positional)]
        files: Vec<PathBuf>,

        /// Base URL for rule links (e.g., "/spec/core")
        #[facet(args::named, args::short = 'b', default)]
        base_url: Option<String>,

        /// Output file for _rules.json (default: stdout)
        #[facet(args::named, args::short = 'o', default)]
        output: Option<PathBuf>,

        /// Also output transformed markdown (to directory)
        #[facet(args::named, default)]
        markdown_out: Option<PathBuf>,
    },
}



fn main() -> Result<()> {
    // Set up miette for fancy error reporting
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .context_lines(2)
                .tab_width(4)
                .build(),
        )
    }))?;

    let args: Args =
        facet_args::from_std_args().wrap_err("Failed to parse command line arguments")?;

    match args.command {
        Some(Command::Rules { files, base_url, output, markdown_out }) => {
            run_rules_command(files, base_url, output, markdown_out)
        }
        None => run_coverage_command(args),
    }
}

fn run_rules_command(
    files: Vec<PathBuf>,
    base_url: Option<String>,
    output: Option<PathBuf>,
    markdown_out: Option<PathBuf>,
) -> Result<()> {
    if files.is_empty() {
        eyre::bail!("No markdown files specified. Usage: tracey rules <file.md>...");
    }

    let base_url = base_url.as_deref().unwrap_or("");
    let mut manifest = RulesManifest::new();
    let mut all_duplicates = Vec::new();

    for file_path in &files {
        eprintln!(
            "{} Processing {}...",
            "->".blue().bold(),
            file_path.display()
        );

        let content = std::fs::read_to_string(file_path)
            .wrap_err_with(|| format!("Failed to read {}", file_path.display()))?;

        let result = MarkdownProcessor::process(&content)
            .wrap_err_with(|| format!("Failed to process {}", file_path.display()))?;

        eprintln!(
            "   Found {} rules",
            result.rules.len().to_string().green()
        );

        // Build manifest for this file
        let file_manifest = RulesManifest::from_rules(&result.rules, base_url);
        let duplicates = manifest.merge(&file_manifest);

        if !duplicates.is_empty() {
            all_duplicates.extend(duplicates);
        }

        // Optionally write transformed markdown
        if let Some(ref out_dir) = markdown_out {
            std::fs::create_dir_all(out_dir)?;
            let out_file = out_dir.join(
                file_path
                    .file_name()
                    .ok_or_else(|| eyre::eyre!("Invalid file path"))?,
            );
            std::fs::write(&out_file, &result.output)
                .wrap_err_with(|| format!("Failed to write {}", out_file.display()))?;
            eprintln!(
                "   Wrote transformed markdown to {}",
                out_file.display()
            );
        }
    }

    // Report any duplicates
    if !all_duplicates.is_empty() {
        eprintln!(
            "\n{} Found {} duplicate rule IDs across files:",
            "!".yellow().bold(),
            all_duplicates.len()
        );
        for dup in &all_duplicates {
            eprintln!(
                "   {} defined at {} and {}",
                dup.id.red(),
                dup.first_url,
                dup.second_url
            );
        }
        eyre::bail!("Duplicate rule IDs found");
    }

    // Output the manifest
    let json = manifest.to_json();

    if let Some(ref out_path) = output {
        std::fs::write(out_path, &json)
            .wrap_err_with(|| format!("Failed to write {}", out_path.display()))?;
        eprintln!(
            "\n{} Wrote {} rules to {}",
            "OK".green().bold(),
            manifest.rules.len(),
            out_path.display()
        );
    } else {
        println!("{}", json);
    }

    Ok(())
}

fn run_coverage_command(args: Args) -> Result<()> {
    // Find project root (look for Cargo.toml)
    let project_root = find_project_root()?;

    // Load config
    let config_path = args
        .config
        .unwrap_or_else(|| project_root.join(".config/tracey/config.kdl"));

    let config = load_config(&config_path)?;

    // Get the directory containing the config file for resolving relative paths
    let config_dir = config_path
        .parent()
        .ok_or_else(|| eyre::eyre!("Config path has no parent directory"))?;

    let threshold = args.threshold.unwrap_or(0.0);
    let format = args
        .format
        .as_ref()
        .and_then(|f| OutputFormat::from_str(f))
        .unwrap_or_default();

    let mut all_passing = true;

    for spec_config in &config.specs {
        let spec_name = &spec_config.name.value;

        // Load manifest from either URL or local file
        let manifest = match (&spec_config.rules_url, &spec_config.rules_file) {
            (Some(url), None) => {
                eprintln!(
                    "{} Fetching spec manifest for {}...",
                    "->".blue().bold(),
                    spec_name.cyan()
                );
                SpecManifest::fetch(&url.value)?
            }
            (None, Some(file)) => {
                let file_path = config_dir.join(&file.path);
                eprintln!(
                    "{} Loading spec manifest for {} from {}...",
                    "->".blue().bold(),
                    spec_name.cyan(),
                    file_path.display()
                );
                SpecManifest::load(&file_path)?
            }
            (Some(_), Some(_)) => {
                eyre::bail!(
                    "Spec '{}' has both rules_url and rules_file - please specify only one",
                    spec_name
                );
            }
            (None, None) => {
                eyre::bail!(
                    "Spec '{}' has neither rules_url nor rules_file - please specify one",
                    spec_name
                );
            }
        };

        eprintln!(
            "   Found {} rules in spec",
            manifest.len().to_string().green()
        );

        // Scan source files
        eprintln!("{} Scanning Rust files...", "->".blue().bold());

        let include: Vec<String> = if spec_config.include.is_empty() {
            vec!["**/*.rs".to_string()]
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

        let rules = Rules::extract(
            WalkSources::new(&project_root)
                .include(include)
                .exclude(exclude),
        )?;

        eprintln!(
            "   Found {} rule references",
            rules.len().to_string().green()
        );

        // Print any warnings
        if !rules.warnings.is_empty() {
            eprintln!(
                "{} {} parse warnings:",
                "!".yellow().bold(),
                rules.warnings.len()
            );
            errors::print_warnings(&rules.warnings, &|path| std::fs::read_to_string(path).ok());
        }

        // Compute coverage
        let report = CoverageReport::compute(spec_name, &manifest, &rules);

        // Print report
        let output = render_report(&report, format, args.verbose);
        print!("{}", output);

        if !report.is_passing(threshold) {
            all_passing = false;
        }
    }

    if args.check && !all_passing {
        std::process::exit(1);
    }

    Ok(())
}

fn find_project_root() -> Result<PathBuf> {
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

fn load_config(path: &PathBuf) -> Result<Config> {
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
