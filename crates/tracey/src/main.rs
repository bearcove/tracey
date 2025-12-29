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
use tracey_core::markdown::{MarkdownProcessor, RulesManifest};
use tracey_core::{CoverageReport, Rules, SpecManifest, WalkSources};

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

    /// Show which rules are referenced at a file or location
    At {
        /// File path, optionally with line number (e.g., "src/main.rs:42" or "src/main.rs:40-60")
        #[facet(args::positional)]
        location: String,

        /// Path to config file (default: .config/tracey/config.kdl)
        #[facet(args::named, args::short = 'c', default)]
        config: Option<PathBuf>,

        /// Output format: text, json
        #[facet(args::named, args::short = 'f', default)]
        format: Option<String>,
    },

    /// Show what code references a rule (impact analysis)
    Impact {
        /// Rule ID to analyze (e.g., "channel.id.allocation")
        #[facet(args::positional)]
        rule_id: String,

        /// Path to config file (default: .config/tracey/config.kdl)
        #[facet(args::named, args::short = 'c', default)]
        config: Option<PathBuf>,

        /// Output format: text, json
        #[facet(args::named, args::short = 'f', default)]
        format: Option<String>,
    },

    /// Generate a traceability matrix showing rules × code artifacts
    Matrix {
        /// Path to config file (default: .config/tracey/config.kdl)
        #[facet(args::named, args::short = 'c', default)]
        config: Option<PathBuf>,

        /// Output format: markdown, csv, json, html (default: markdown)
        #[facet(args::named, args::short = 'f', default)]
        format: Option<String>,

        /// Only show uncovered rules
        #[facet(args::named, default)]
        uncovered: bool,

        /// Only show rules missing verification/tests
        #[facet(args::named, default)]
        no_verify: bool,

        /// Filter by requirement level (must, should, may)
        #[facet(args::named, default)]
        level: Option<String>,

        /// Filter by status (draft, stable, deprecated, removed)
        #[facet(args::named, default)]
        status: Option<String>,

        /// Filter by rule ID prefix (e.g., "channel.")
        #[facet(args::named, default)]
        prefix: Option<String>,

        /// Output file (default: stdout)
        #[facet(args::named, args::short = 'o', default)]
        output: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    // Set up syntax highlighting for miette
    miette_arborium::install_global().ok();

    // Set up miette for fancy error reporting (ignore if already set)
    let _ = miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .context_lines(2)
                .tab_width(4)
                .build(),
        )
    }));

    let args: Args = match facet_args::from_std_args() {
        Ok(args) => args,
        Err(e) => {
            if e.is_help_request() {
                // Print help text directly (not as an error)
                if let Some(help) = e.help_text() {
                    println!("{}", help);
                }
                return Ok(());
            }
            // Real parsing error - report via miette for nice formatting
            let report = miette::Report::new(e);
            eprintln!("{:?}", report);
            std::process::exit(1);
        }
    };

    match args.command {
        Some(Command::Rules {
            files,
            base_url,
            output,
            markdown_out,
        }) => run_rules_command(files, base_url, output, markdown_out),
        Some(Command::At {
            location,
            config,
            format,
        }) => run_at_command(location, config, format),
        Some(Command::Impact {
            rule_id,
            config,
            format,
        }) => run_impact_command(rule_id, config, format),
        Some(Command::Matrix {
            config,
            format,
            uncovered,
            no_verify,
            level,
            status,
            prefix,
            output,
        }) => run_matrix_command(
            config, format, uncovered, no_verify, level, status, prefix, output,
        ),
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

        eprintln!("   Found {} rules", result.rules.len().to_string().green());

        // Build manifest for this file
        let source_file = file_path.to_string_lossy();
        let file_manifest = RulesManifest::from_rules(&result.rules, base_url, Some(&source_file));
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
            eprintln!("   Wrote transformed markdown to {}", out_file.display());
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
    // [impl config.path.default]
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

        // Load manifest from URL, local file, or markdown glob
        let manifest = match (
            &spec_config.rules_url,
            &spec_config.rules_file,
            &spec_config.rules_glob,
        ) {
            (Some(url), None, None) => {
                eprintln!(
                    "{} Fetching spec manifest for {}...",
                    "->".blue().bold(),
                    spec_name.cyan()
                );
                SpecManifest::fetch(&url.value)?
            }
            (None, Some(file), None) => {
                let file_path = config_dir.join(&file.path);
                eprintln!(
                    "{} Loading spec manifest for {} from {}...",
                    "->".blue().bold(),
                    spec_name.cyan(),
                    file_path.display()
                );
                SpecManifest::load(&file_path)?
            }
            (None, None, Some(glob)) => {
                eprintln!(
                    "{} Extracting rules for {} from markdown files matching {}...",
                    "->".blue().bold(),
                    spec_name.cyan(),
                    glob.pattern.cyan()
                );
                load_manifest_from_glob(&project_root, &glob.pattern)?
            }
            // [impl config.spec.source]
            (None, None, None) => {
                eyre::bail!(
                    "Spec '{}' has no rules source - please specify rules_url, rules_file, or rules_glob",
                    spec_name
                );
            }
            _ => {
                eyre::bail!(
                    "Spec '{}' has multiple rules sources - please specify only one of rules_url, rules_file, or rules_glob",
                    spec_name
                );
            }
        };

        eprintln!(
            "   Found {} rules in spec",
            manifest.len().to_string().green()
        );

        // Scan source files
        eprintln!("{} Scanning source files...", "->".blue().bold());

        // [impl config.spec.include]
        // [impl walk.default-include]
        let include: Vec<String> = if spec_config.include.is_empty() {
            // Default: include all supported source file types
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

        // [impl config.spec.exclude]
        // [impl walk.default-exclude]
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

/// Parse a location string like "src/main.rs", "src/main.rs:42", or "src/main.rs:40-60"
fn parse_location(location: &str) -> Result<(PathBuf, Option<usize>, Option<usize>)> {
    // Try to parse as path:line-end or path:line or just path
    if let Some((path, rest)) = location.rsplit_once(':') {
        // Check if it looks like a Windows path (e.g., C:\foo)
        if rest.chars().all(|c| c.is_ascii_digit() || c == '-') && !rest.is_empty() {
            if let Some((start, end)) = rest.split_once('-') {
                let start_line: usize = start
                    .parse()
                    .wrap_err_with(|| format!("Invalid start line number: {}", start))?;
                let end_line: usize = end
                    .parse()
                    .wrap_err_with(|| format!("Invalid end line number: {}", end))?;
                return Ok((PathBuf::from(path), Some(start_line), Some(end_line)));
            } else {
                let line: usize = rest
                    .parse()
                    .wrap_err_with(|| format!("Invalid line number: {}", rest))?;
                return Ok((PathBuf::from(path), Some(line), None));
            }
        }
    }
    Ok((PathBuf::from(location), None, None))
}

fn run_at_command(location: String, config: Option<PathBuf>, format: Option<String>) -> Result<()> {
    use std::collections::HashMap;
    use tracey_core::RefVerb;

    let (file_path, start_line, end_line) = parse_location(&location)?;

    // Make the path absolute using cwd (no project root assumption)
    let cwd = std::env::current_dir()?;
    let file_path = if file_path.is_absolute() {
        file_path
    } else {
        cwd.join(&file_path)
    };

    if !file_path.exists() {
        eyre::bail!("File not found: {}", file_path.display());
    }

    // Load config (optional for `at` command - only used for rule URLs)
    // Try to find project root for config, fall back to cwd
    let project_root = find_project_root().unwrap_or_else(|_| cwd.clone());
    let config_path = config.unwrap_or_else(|| project_root.join(".config/tracey/config.kdl"));
    let config = if config_path.exists() {
        Some(load_config(&config_path)?)
    } else {
        None
    };

    let is_json = format.as_deref() == Some("json");

    // Extract rules from just this file
    let content = std::fs::read_to_string(&file_path)?;
    let rules = Rules::extract_from_content(&file_path, &content);

    // Filter by line range if specified
    let filtered_refs: Vec<_> = rules
        .references
        .iter()
        .filter(|r| {
            if let (Some(start), Some(end)) = (start_line, end_line) {
                r.line >= start && r.line <= end
            } else if let Some(line) = start_line {
                r.line == line
            } else {
                true
            }
        })
        .collect();

    if is_json {
        // JSON output
        let output: Vec<_> = filtered_refs
            .iter()
            .map(|r| {
                serde_json::json!({
                    "rule_id": r.rule_id,
                    "verb": r.verb.as_str(),
                    "line": r.line,
                    "file": r.file.display().to_string(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // Text output - show path relative to cwd if possible
        let relative_path = file_path
            .strip_prefix(&cwd)
            .or_else(|_| file_path.strip_prefix(&project_root))
            .unwrap_or(&file_path);

        if filtered_refs.is_empty() {
            let location_desc = if let (Some(start), Some(end)) = (start_line, end_line) {
                format!("{}:{}-{}", relative_path.display(), start, end)
            } else if let Some(line) = start_line {
                format!("{}:{}", relative_path.display(), line)
            } else {
                relative_path.display().to_string()
            };
            println!(
                "{}",
                format!("No rule references found at {}", location_desc).dimmed()
            );
            return Ok(());
        }

        // Group by verb
        let mut by_verb: HashMap<RefVerb, Vec<&tracey_core::RuleReference>> = HashMap::new();
        for r in &filtered_refs {
            by_verb.entry(r.verb).or_default().push(r);
        }

        let location_desc = if let (Some(start), Some(end)) = (start_line, end_line) {
            format!("{}:{}-{}", relative_path.display(), start, end)
        } else if let Some(line) = start_line {
            format!("{}:{}", relative_path.display(), line)
        } else {
            relative_path.display().to_string()
        };

        println!("{}", location_desc.bold());

        // Try to load manifests to get rule descriptions/URLs (if config exists)
        let mut rule_urls: HashMap<String, String> = HashMap::new();
        if let Some(ref config) = config {
            for spec_config in &config.specs {
                if let Some(ref glob) = spec_config.rules_glob
                    && let Ok(manifest) = load_manifest_from_glob(&project_root, &glob.pattern)
                {
                    for (id, info) in manifest.rules {
                        rule_urls.insert(id, info.url);
                    }
                }
            }
        }

        for verb in [
            RefVerb::Impl,
            RefVerb::Verify,
            RefVerb::Depends,
            RefVerb::Related,
            RefVerb::Define,
        ] {
            if let Some(refs) = by_verb.get(&verb) {
                let verb_str = format!("{}:", verb.as_str());
                let rule_ids: Vec<_> = refs.iter().map(|r| r.rule_id.as_str()).collect();
                println!("  {} {}", verb_str.cyan(), rule_ids.join(", "));
            }
        }
    }

    Ok(())
}

fn run_impact_command(
    rule_id: String,
    config: Option<PathBuf>,
    format: Option<String>,
) -> Result<()> {
    use std::collections::HashMap;
    use tracey_core::RefVerb;

    // Find project root
    let project_root = find_project_root()?;

    // Load config
    let config_path = config.unwrap_or_else(|| project_root.join(".config/tracey/config.kdl"));
    let config = load_config(&config_path)?;

    let is_json = format.as_deref() == Some("json");

    // Collect all references across all specs
    let mut all_refs: Vec<tracey_core::RuleReference> = Vec::new();
    let mut rule_url: Option<String> = None;

    for spec_config in &config.specs {
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

        let rules = Rules::extract(
            WalkSources::new(&project_root)
                .include(include)
                .exclude(exclude),
        )?;

        // Filter to just this rule
        for r in rules.references {
            if r.rule_id == rule_id {
                all_refs.push(r);
            }
        }

        // Try to get URL for the rule
        if rule_url.is_none()
            && let Some(ref glob) = spec_config.rules_glob
            && let Ok(manifest) = load_manifest_from_glob(&project_root, &glob.pattern)
            && let Some(info) = manifest.rules.get(&rule_id)
        {
            rule_url = Some(info.url.clone());
        }
    }

    if is_json {
        // JSON output
        let mut by_verb: HashMap<&str, Vec<serde_json::Value>> = HashMap::new();
        for r in &all_refs {
            by_verb.entry(r.verb.as_str()).or_default().push(
                serde_json::json!({
                    "file": r.file.strip_prefix(&project_root).unwrap_or(&r.file).display().to_string(),
                    "line": r.line,
                })
            );
        }
        let output = serde_json::json!({
            "rule_id": rule_id,
            "url": rule_url,
            "references": by_verb,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // Text output
        println!("{} {}", "Rule:".bold(), rule_id.cyan());
        if let Some(url) = &rule_url {
            println!("{} {}", "URL:".bold(), url.dimmed());
        }
        println!();

        if all_refs.is_empty() {
            println!("{}", "No references found for this rule.".dimmed());
            return Ok(());
        }

        // Group by verb
        let mut by_verb: HashMap<RefVerb, Vec<&tracey_core::RuleReference>> = HashMap::new();
        for r in &all_refs {
            by_verb.entry(r.verb).or_default().push(r);
        }

        let verb_labels = [
            (RefVerb::Impl, "Implementation sites", "impl"),
            (RefVerb::Verify, "Verification sites", "verify"),
            (RefVerb::Depends, "Dependent code", "depends"),
            (RefVerb::Related, "Related code", "related"),
            (RefVerb::Define, "Definition sites", "define"),
        ];

        for (verb, label, _) in verb_labels {
            if let Some(refs) = by_verb.get(&verb) {
                println!("{} ({}):", label.bold(), verb.as_str().cyan());
                for r in refs {
                    let relative = r.file.strip_prefix(&project_root).unwrap_or(&r.file);
                    let location = format!("{}:{}", relative.display(), r.line);

                    // Add a note for depends references
                    if verb == RefVerb::Depends {
                        println!(
                            "  {} {}",
                            location.yellow(),
                            "- RECHECK IF RULE CHANGES".dimmed()
                        );
                    } else {
                        println!("  {}", location);
                    }
                }
                println!();
            }
        }
    }

    Ok(())
}

/// Matrix output format
#[derive(Debug, Clone, Copy, Default)]
enum MatrixFormat {
    #[default]
    Markdown,
    Csv,
    Json,
    Html,
}

impl MatrixFormat {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "markdown" | "md" => Some(Self::Markdown),
            "csv" => Some(Self::Csv),
            "json" => Some(Self::Json),
            "html" => Some(Self::Html),
            _ => None,
        }
    }
}

/// A row in the traceability matrix
#[derive(Debug, Clone)]
struct MatrixRow {
    rule_id: String,
    /// URL to the rule in the spec (for web links)
    url: String,
    /// Source file where the rule is defined (relative path)
    source_file: Option<String>,
    /// Line number where the rule is defined
    source_line: Option<usize>,
    /// The rule text/description
    text: Option<String>,
    status: Option<String>,
    level: Option<String>,
    impl_refs: Vec<String>,
    verify_refs: Vec<String>,
    depends_refs: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
fn run_matrix_command(
    config: Option<PathBuf>,
    format: Option<String>,
    uncovered_only: bool,
    no_verify_only: bool,
    level_filter: Option<String>,
    status_filter: Option<String>,
    prefix_filter: Option<String>,
    output: Option<PathBuf>,
) -> Result<()> {
    use tracey_core::RefVerb;

    let project_root = find_project_root()?;
    let config_path = config.unwrap_or_else(|| project_root.join(".config/tracey/config.kdl"));
    let config = load_config(&config_path)?;

    let format = format
        .as_ref()
        .and_then(|f| MatrixFormat::from_str(f))
        .unwrap_or_default();

    let config_dir = config_path
        .parent()
        .ok_or_else(|| eyre::eyre!("Config path has no parent directory"))?;

    let mut all_rows: Vec<MatrixRow> = Vec::new();

    for spec_config in &config.specs {
        let spec_name = &spec_config.name.value;

        // Load manifest
        let manifest = match (
            &spec_config.rules_url,
            &spec_config.rules_file,
            &spec_config.rules_glob,
        ) {
            (Some(url), None, None) => {
                eprintln!(
                    "{} Fetching spec manifest for {}...",
                    "->".blue().bold(),
                    spec_name.cyan()
                );
                SpecManifest::fetch(&url.value)?
            }
            (None, Some(file), None) => {
                let file_path = config_dir.join(&file.path);
                eprintln!(
                    "{} Loading spec manifest for {} from {}...",
                    "->".blue().bold(),
                    spec_name.cyan(),
                    file_path.display()
                );
                SpecManifest::load(&file_path)?
            }
            (None, None, Some(glob)) => {
                eprintln!(
                    "{} Extracting rules for {} from markdown files matching {}...",
                    "->".blue().bold(),
                    spec_name.cyan(),
                    glob.pattern.cyan()
                );
                load_manifest_from_glob(&project_root, &glob.pattern)?
            }
            (None, None, None) => {
                eyre::bail!(
                    "Spec '{}' has no rules source - please specify rules_url, rules_file, or rules_glob",
                    spec_name
                );
            }
            _ => {
                eyre::bail!(
                    "Spec '{}' has multiple rules sources - please specify only one of rules_url, rules_file, or rules_glob",
                    spec_name
                );
            }
        };

        // Scan source files
        eprintln!("{} Scanning source files...", "->".blue().bold());

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

        let rules = Rules::extract(
            WalkSources::new(&project_root)
                .include(include)
                .exclude(exclude),
        )?;

        // Build matrix rows
        for (rule_id, rule_info) in &manifest.rules {
            // Apply filters
            if let Some(ref prefix) = prefix_filter
                && !rule_id.starts_with(prefix)
            {
                continue;
            }

            if let Some(ref status) = status_filter
                && rule_info.status.as_deref() != Some(status)
            {
                continue;
            }

            if let Some(ref level) = level_filter
                && rule_info.level.as_deref() != Some(level)
            {
                continue;
            }

            // Collect references by verb
            let mut impl_refs = Vec::new();
            let mut verify_refs = Vec::new();
            let mut depends_refs = Vec::new();

            for r in &rules.references {
                if r.rule_id == *rule_id {
                    let relative = r.file.strip_prefix(&project_root).unwrap_or(&r.file);
                    let location = format!("{}:{}", relative.display(), r.line);
                    match r.verb {
                        RefVerb::Impl | RefVerb::Define => impl_refs.push(location),
                        RefVerb::Verify => verify_refs.push(location),
                        RefVerb::Depends | RefVerb::Related => depends_refs.push(location),
                    }
                }
            }

            // Apply uncovered/no-verify filters
            if uncovered_only && (!impl_refs.is_empty() || !verify_refs.is_empty()) {
                continue;
            }

            if no_verify_only && !verify_refs.is_empty() {
                continue;
            }

            all_rows.push(MatrixRow {
                rule_id: rule_id.clone(),
                url: rule_info.url.clone(),
                source_file: rule_info.source_file.clone(),
                source_line: rule_info.source_line,
                text: rule_info.text.clone(),
                status: rule_info.status.clone(),
                level: rule_info.level.clone(),
                impl_refs,
                verify_refs,
                depends_refs,
            });
        }
    }

    // Sort by rule ID
    all_rows.sort_by(|a, b| a.rule_id.cmp(&b.rule_id));

    // Generate output
    let output_str = match format {
        MatrixFormat::Markdown => render_matrix_markdown(&all_rows),
        MatrixFormat::Csv => render_matrix_csv(&all_rows),
        MatrixFormat::Json => render_matrix_json(&all_rows),
        MatrixFormat::Html => render_matrix_html(&all_rows, &project_root),
    };

    if let Some(ref out_path) = output {
        std::fs::write(out_path, &output_str)
            .wrap_err_with(|| format!("Failed to write {}", out_path.display()))?;
        eprintln!(
            "\n{} Wrote matrix to {}",
            "OK".green().bold(),
            out_path.display()
        );
    } else {
        print!("{}", output_str);
    }

    Ok(())
}

fn render_matrix_markdown(rows: &[MatrixRow]) -> String {
    let mut output = String::new();

    output.push_str("| Rule | Status | Level | impl | verify | depends |\n");
    output.push_str("|------|--------|-------|------|--------|--------|\n");

    for row in rows {
        let status = row.status.as_deref().unwrap_or("-");
        let level = row.level.as_deref().unwrap_or("-");
        let impl_str = if row.impl_refs.is_empty() {
            "-".to_string()
        } else {
            row.impl_refs.join(", ")
        };
        let verify_str = if row.verify_refs.is_empty() {
            "-".to_string()
        } else {
            row.verify_refs.join(", ")
        };
        let depends_str = if row.depends_refs.is_empty() {
            "-".to_string()
        } else {
            row.depends_refs.join(", ")
        };

        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            row.rule_id, status, level, impl_str, verify_str, depends_str
        ));
    }

    output
}

fn render_matrix_csv(rows: &[MatrixRow]) -> String {
    let mut output = String::new();

    output.push_str("rule,status,level,impl,verify,depends\n");

    for row in rows {
        let status = row.status.as_deref().unwrap_or("");
        let level = row.level.as_deref().unwrap_or("");
        let impl_str = row.impl_refs.join(";");
        let verify_str = row.verify_refs.join(";");
        let depends_str = row.depends_refs.join(";");

        // Escape fields that might contain commas
        let escape = |s: &str| {
            if s.contains(',') || s.contains('"') || s.contains('\n') {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else {
                s.to_string()
            }
        };

        output.push_str(&format!(
            "{},{},{},{},{},{}\n",
            escape(&row.rule_id),
            escape(status),
            escape(level),
            escape(&impl_str),
            escape(&verify_str),
            escape(&depends_str)
        ));
    }

    output
}

fn render_matrix_json(rows: &[MatrixRow]) -> String {
    let json_rows: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "rule_id": row.rule_id,
                "url": row.url,
                "text": row.text,
                "status": row.status,
                "level": row.level,
                "impl": row.impl_refs,
                "verify": row.verify_refs,
                "depends": row.depends_refs,
            })
        })
        .collect();

    serde_json::to_string_pretty(&json_rows).unwrap_or_else(|_| "[]".to_string())
}

fn render_matrix_html(rows: &[MatrixRow], project_root: &std::path::Path) -> String {
    let mut output = String::new();

    // Get absolute project root for editor links
    let abs_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let root_str = abs_root.display().to_string();

    output.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    output.push_str("<meta charset=\"utf-8\">\n");
    output.push_str("<meta name=\"color-scheme\" content=\"light dark\">\n");
    output.push_str("<title>Traceability Matrix</title>\n");
    output.push_str("<link rel=\"preconnect\" href=\"https://fonts.googleapis.com\">\n");
    output.push_str("<link rel=\"preconnect\" href=\"https://fonts.gstatic.com\" crossorigin>\n");
    output.push_str("<link href=\"https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500&family=Public+Sans:wght@400;500;600&display=swap\" rel=\"stylesheet\">\n");
    // Tokyo Night inspired color palette
    // Light: clean whites and grays with subtle blue tints
    // Dark: deep blue-grays from Tokyo Night
    output.push_str("<style>\n");
    output.push_str(
        r#":root {
  color-scheme: light dark;
}
body {
  font-family: 'Public Sans', system-ui, sans-serif;
  margin: 2rem;
  background: light-dark(#f5f5f7, #1a1b26);
  color: light-dark(#1a1b26, #a9b1d6);
}
h1 {
  font-weight: 600;
  color: light-dark(#1a1b26, #c0caf5);
}
table {
  border-collapse: collapse;
  width: 100%;
  font-family: 'IBM Plex Mono', monospace;
  font-size: 0.9em;
}
th, td {
  border: 1px solid light-dark(#d5d5db, #292e42);
  padding: 8px;
  text-align: left;
}
th {
  background-color: light-dark(#e8e8ed, #24283b);
  position: sticky;
  top: 0;
  font-family: 'Public Sans', system-ui, sans-serif;
  font-weight: 600;
  color: light-dark(#1a1b26, #c0caf5);
}
tr:nth-child(even) {
  background-color: light-dark(#fafafe, #1f2335);
}
tr:hover {
  background-color: light-dark(#e8e8f0, #292e42);
}
.covered {
  background-color: light-dark(#ddf4dd, #1a2f1a);
}
.covered:hover {
  background-color: light-dark(#c8ecc8, #243d24);
}
.partial {
  background-color: light-dark(#fff0d9, #2d2a1a);
}
.partial:hover {
  background-color: light-dark(#ffe4bf, #3d3824);
}
.uncovered {
  background-color: light-dark(#fde2e2, #2d1a1a);
}
.uncovered:hover {
  background-color: light-dark(#fcd0d0, #3d2424);
}
.status-draft {
  color: light-dark(#6b7280, #565f89);
  font-style: italic;
}
.status-deprecated {
  color: light-dark(#b45309, #e0af68);
  text-decoration: line-through;
}
.status-removed {
  color: light-dark(#9ca3af, #414868);
  text-decoration: line-through;
}
.controls {
  margin-bottom: 1rem;
  display: flex;
  gap: 1rem;
  align-items: center;
  font-family: 'Public Sans', system-ui, sans-serif;
}
#filter, #editor-select {
  padding: 0.5rem 0.75rem;
  font-family: inherit;
  background: light-dark(#fff, #24283b);
  color: light-dark(#1a1b26, #a9b1d6);
  border: 1px solid light-dark(#d5d5db, #414868);
  border-radius: 6px;
}
#filter {
  width: 300px;
}
#filter:focus, #editor-select:focus {
  outline: none;
  border-color: light-dark(#7aa2f7, #7aa2f7);
  box-shadow: 0 0 0 2px light-dark(rgba(122, 162, 247, 0.2), rgba(122, 162, 247, 0.3));
}
#filter::placeholder {
  color: light-dark(#9ca3af, #565f89);
}
.file-link, .spec-link {
  color: light-dark(#2563eb, #7aa2f7);
  text-decoration: none;
}
.file-link:hover, .spec-link:hover {
  text-decoration: underline;
  color: light-dark(#1d4ed8, #89b4fa);
}
.spec-link {
  font-weight: 500;
}
.desc {
  max-width: 400px;
  font-size: 0.9em;
  color: light-dark(#4b5563, #737aa2);
  font-family: 'Public Sans', system-ui, sans-serif;
}
label {
  color: light-dark(#374151, #9aa5ce);
}
"#,
    );
    output.push_str("</style>\n");

    // JavaScript for filtering and editor switching
    output.push_str("<script>\n");
    output.push_str(&format!(
        r#"const PROJECT_ROOT = "{}";

const EDITORS = {{
  zed: {{ name: "Zed", urlTemplate: (path, line) => `zed://file/${{path}}:${{line}}` }},
  vscode: {{ name: "VS Code", urlTemplate: (path, line) => `vscode://file/${{path}}:${{line}}` }},
}};

function getEditor() {{
  return localStorage.getItem('tracey-editor') || 'zed';
}}

function setEditor(editor) {{
  localStorage.setItem('tracey-editor', editor);
  updateAllLinks();
}}

function updateAllLinks() {{
  const editor = getEditor();
  const config = EDITORS[editor];
  // Update file links (impl/verify/depends columns)
  document.querySelectorAll('.file-link').forEach(link => {{
    const path = link.dataset.path;
    const line = link.dataset.line;
    const fullPath = PROJECT_ROOT + '/' + path;
    link.href = config.urlTemplate(fullPath, line);
  }});
  // Update spec links (rule column)
  document.querySelectorAll('.spec-link').forEach(link => {{
    const path = link.dataset.path;
    const line = link.dataset.line || '1';
    if (path) {{
      const fullPath = PROJECT_ROOT + '/' + path;
      link.href = config.urlTemplate(fullPath, line);
    }}
  }});
}}

function filterTable() {{
  const filter = document.getElementById('filter').value.toLowerCase();
  const rows = document.querySelectorAll('tbody tr');
  rows.forEach(row => {{
    const text = row.textContent.toLowerCase();
    row.style.display = text.includes(filter) ? '' : 'none';
  }});
}}

document.addEventListener('DOMContentLoaded', () => {{
  const select = document.getElementById('editor-select');
  select.value = getEditor();
  updateAllLinks();
}});
"#,
        root_str.replace('\\', "\\\\").replace('"', "\\\"")
    ));
    output.push_str("</script>\n");
    output.push_str("</head>\n<body>\n");
    output.push_str("<h1>Traceability Matrix</h1>\n");
    output.push_str("<div class=\"controls\">\n");
    output.push_str(
        "<input type=\"text\" id=\"filter\" placeholder=\"Filter rules...\" onkeyup=\"filterTable()\">\n",
    );
    output.push_str("<label for=\"editor-select\">Open in:</label>\n");
    output.push_str("<select id=\"editor-select\" onchange=\"setEditor(this.value)\">\n");
    output.push_str("<option value=\"zed\">Zed</option>\n");
    output.push_str("<option value=\"vscode\">VS Code</option>\n");
    output.push_str("</select>\n");
    output.push_str("</div>\n");
    output.push_str("<table>\n");
    output.push_str("<thead>\n");
    output.push_str(
        "<tr><th>Rule</th><th>Description</th><th>Status</th><th>Level</th><th>impl</th><th>verify</th><th>depends</th></tr>\n",
    );
    output.push_str("</thead>\n");
    output.push_str("<tbody>\n");

    // Helper to format file references as links
    let format_refs = |refs: &[String]| -> String {
        if refs.is_empty() {
            "-".to_string()
        } else {
            refs.iter()
                .map(|r| {
                    // Parse "path:line" format
                    if let Some((path, line)) = r.rsplit_once(':') {
                        format!(
                            "<a class=\"file-link\" data-path=\"{}\" data-line=\"{}\" href=\"#\">{}</a>",
                            html_escape::encode_double_quoted_attribute(path),
                            line,
                            html_escape::encode_text(r)
                        )
                    } else {
                        html_escape::encode_text(r).to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("<br>")
        }
    };

    for row in rows {
        let has_impl = !row.impl_refs.is_empty();
        let has_verify = !row.verify_refs.is_empty();
        let row_class = if has_impl && has_verify {
            "covered"
        } else if has_impl || has_verify {
            "partial"
        } else {
            "uncovered"
        };

        let status = row.status.as_deref().unwrap_or("-");
        let level = row.level.as_deref().unwrap_or("-");
        let status_class = match status {
            "draft" => "status-draft",
            "deprecated" => "status-deprecated",
            "removed" => "status-removed",
            _ => "",
        };

        // Format rule ID as a link to the spec file (opened in editor)
        let rule_cell = match (&row.source_file, row.source_line) {
            (Some(source_file), Some(source_line)) => {
                format!(
                    "<a href=\"#\" class=\"spec-link {}\" data-path=\"{}\" data-line=\"{}\">{}</a>",
                    status_class,
                    html_escape::encode_double_quoted_attribute(source_file),
                    source_line,
                    html_escape::encode_text(&row.rule_id)
                )
            }
            (Some(source_file), None) => {
                format!(
                    "<a href=\"#\" class=\"spec-link {}\" data-path=\"{}\" data-line=\"1\">{}</a>",
                    status_class,
                    html_escape::encode_double_quoted_attribute(source_file),
                    html_escape::encode_text(&row.rule_id)
                )
            }
            _ => {
                format!(
                    "<span class=\"{}\">{}</span>",
                    status_class,
                    html_escape::encode_text(&row.rule_id)
                )
            }
        };

        // Format description
        let desc_cell = match &row.text {
            Some(text) if !text.is_empty() => html_escape::encode_text(text).to_string(),
            _ => "-".to_string(),
        };

        let impl_str = format_refs(&row.impl_refs);
        let verify_str = format_refs(&row.verify_refs);
        let depends_str = format_refs(&row.depends_refs);

        output.push_str(&format!(
            "<tr class=\"{}\"><td>{}</td><td class=\"desc\">{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            row_class, rule_cell, desc_cell, status, level, impl_str, verify_str, depends_str
        ));
    }

    output.push_str("</tbody>\n");
    output.push_str("</table>\n");
    output.push_str("</body>\n</html>\n");

    output
}

/// Load a SpecManifest by extracting rules from markdown files matching a glob pattern
fn load_manifest_from_glob(root: &PathBuf, pattern: &str) -> Result<SpecManifest> {
    use ignore::WalkBuilder;
    use std::collections::HashMap;

    let mut rules_manifest = RulesManifest::new();
    let mut file_count = 0;

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

        let result = MarkdownProcessor::process(&content)
            .wrap_err_with(|| format!("Failed to process {}", path.display()))?;

        if !result.rules.is_empty() {
            eprintln!(
                "   {} {} rules from {}",
                "Found".green(),
                result.rules.len(),
                relative_str
            );
            file_count += 1;

            // Build manifest for this file (no base URL needed for coverage checking)
            let file_manifest = RulesManifest::from_rules(&result.rules, "", Some(&relative_str));
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

    if file_count == 0 {
        eyre::bail!(
            "No markdown files with rules found matching pattern '{}'",
            pattern
        );
    }

    // Convert RulesManifest to SpecManifest
    let spec_rules: HashMap<String, tracey_core::RuleInfo> = rules_manifest
        .rules
        .into_iter()
        .map(|(id, entry)| {
            (
                id,
                tracey_core::RuleInfo {
                    url: entry.url,
                    source_file: entry.source_file,
                    source_line: entry.source_line,
                    text: entry.text,
                    status: entry.status,
                    level: entry.level,
                    since: entry.since,
                    until: entry.until,
                    tags: entry.tags,
                },
            )
        })
        .collect();

    Ok(SpecManifest { rules: spec_rules })
}

/// Simple glob pattern matching
fn matches_glob(path: &str, pattern: &str) -> bool {
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

    let mut remaining = path;
    for part in parts {
        if let Some(idx) = remaining.find(part) {
            remaining = &remaining[idx + part.len()..];
        } else {
            return false;
        }
    }

    true
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
