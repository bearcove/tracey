//! tracey - Measure spec coverage in Rust codebases
//!
//! tracey parses Rust source files to find references to specification rules
//! (in the format `[rule.id]` in comments) and compares them against a spec
//! manifest to produce coverage reports.

mod config;
mod errors;
mod output;
mod search;
mod serve;
mod vite;

use config::Config;
use eyre::{Result, WrapErr};
use facet_args as args;
use output::{OutputFormat, render_report};
use owo_colors::OwoColorize;
use std::path::PathBuf;
use tracey_core::code_units::{CodeUnit, CodeUnitKind, CodeUnits, extract_rust};
use tracey_core::markdown::{MarkdownProcessor, MarkdownWarningKind, RulesManifest};
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

        /// Open HTML output in browser (writes to temp file)
        #[facet(args::named, default)]
        open: bool,
    },

    /// Start an interactive dashboard server with live reload
    Serve {
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

    /// Reverse coverage: show which code units lack spec references
    Rev {
        /// Path to config file (default: .config/tracey/config.kdl)
        #[facet(args::named, args::short = 'c', default)]
        config: Option<PathBuf>,

        /// Output format: text, json
        #[facet(args::named, args::short = 'f', default)]
        format: Option<String>,

        /// Only show uncovered code units
        #[facet(args::named, default)]
        uncovered: bool,

        /// Filter by code unit kind (function, struct, enum, trait, impl, etc.)
        #[facet(args::named, default)]
        kind: Option<String>,

        /// Filter by file path pattern (e.g., "src/channel")
        #[facet(args::named, default)]
        path: Option<String>,
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
            open,
        }) => run_matrix_command(
            config, format, uncovered, no_verify, level, status, prefix, output, open,
        ),
        Some(Command::Serve {
            config,
            port,
            open,
            dev,
        }) => serve::run(config, port.unwrap_or(3000), open, dev),
        Some(Command::Rev {
            config,
            format,
            uncovered,
            kind,
            path,
        }) => run_reverse_command(config, format, uncovered, kind, path),
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
    let mut all_warnings = Vec::new();

    for file_path in &files {
        eprintln!(
            "{} Processing {}...",
            "->".blue().bold(),
            file_path.display()
        );

        let content = std::fs::read_to_string(file_path)
            .wrap_err_with(|| format!("Failed to read {}", file_path.display()))?;

        let result = MarkdownProcessor::process_with_path(&content, Some(file_path))
            .wrap_err_with(|| format!("Failed to process {}", file_path.display()))?;

        eprintln!("   Found {} rules", result.rules.len().to_string().green());

        // Collect warnings
        all_warnings.extend(result.warnings);

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

    // Report RFC 2119 warnings
    if !all_warnings.is_empty() {
        eprintln!(
            "\n{} {} rule quality warnings:",
            "!".yellow().bold(),
            all_warnings.len()
        );
        for warning in &all_warnings {
            let message = match &warning.kind {
                MarkdownWarningKind::NoRfc2119Keyword => {
                    "no RFC 2119 keyword (MUST/SHOULD/MAY) - rule may be underspecified".to_string()
                }
                MarkdownWarningKind::NegativeRequirement(kw) => {
                    format!(
                        "contains {} - negative requirements are hard to verify, consider rephrasing as a positive requirement",
                        kw.as_str()
                    )
                }
            };
            eprintln!(
                "   {}:{} {} - {}",
                warning.file.display(),
                warning.line,
                warning.rule_id.yellow(),
                message
            );
        }
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
        // JSON output - use Facet struct for proper serialization
        #[derive(facet::Facet)]
        struct RefJson {
            rule_id: String,
            verb: String,
            line: usize,
            file: String,
        }

        let output: Vec<RefJson> = filtered_refs
            .iter()
            .map(|r| RefJson {
                rule_id: r.rule_id.clone(),
                verb: r.verb.as_str().to_string(),
                line: r.line,
                file: r.file.display().to_string(),
            })
            .collect();
        println!(
            "{}",
            facet_json::to_string_pretty(&output).expect("JSON serialization failed")
        );
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
        // JSON output - use Facet structs for proper serialization
        #[derive(facet::Facet)]
        struct FileRef {
            file: String,
            line: usize,
        }

        #[derive(facet::Facet)]
        struct RuleOutput {
            rule_id: String,
            #[facet(default)]
            url: Option<String>,
            references: std::collections::HashMap<String, Vec<FileRef>>,
        }

        let mut refs_by_verb: std::collections::HashMap<String, Vec<FileRef>> =
            std::collections::HashMap::new();
        for r in &all_refs {
            refs_by_verb
                .entry(r.verb.as_str().to_string())
                .or_default()
                .push(FileRef {
                    file: r
                        .file
                        .strip_prefix(&project_root)
                        .unwrap_or(&r.file)
                        .display()
                        .to_string(),
                    line: r.line,
                });
        }

        let output = RuleOutput {
            rule_id: rule_id.clone(),
            url: rule_url.clone(),
            references: refs_by_verb,
        };
        println!(
            "{}",
            facet_json::to_string_pretty(&output).expect("JSON serialization failed")
        );
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
    open_in_browser: bool,
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

    if open_in_browser {
        // --open flag: write to temp file and open in browser
        if !matches!(format, MatrixFormat::Html) {
            eyre::bail!("The --open flag requires --format=html");
        }

        // Generate a unique temp file
        let temp_dir = std::env::temp_dir();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let temp_path = temp_dir.join(format!("tracey-matrix-{}.html", timestamp));

        std::fs::write(&temp_path, &output_str)
            .wrap_err_with(|| format!("Failed to write temp file {}", temp_path.display()))?;

        eprintln!(
            "{} Opening {} in browser...",
            "->".blue().bold(),
            temp_path.display()
        );

        open::that(&temp_path)
            .wrap_err_with(|| format!("Failed to open {} in browser", temp_path.display()))?;
    } else if let Some(ref out_path) = output {
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

fn run_reverse_command(
    config: Option<PathBuf>,
    format: Option<String>,
    uncovered_only: bool,
    kind_filter: Option<String>,
    path_filter: Option<String>,
) -> Result<()> {
    use ignore::WalkBuilder;

    let project_root = find_project_root()?;
    let config_path = config.unwrap_or_else(|| project_root.join(".config/tracey/config.kdl"));
    let config = load_config(&config_path)?;

    let format = format.as_deref().unwrap_or("text");
    let is_json = format == "json";

    // Parse kind filter
    let kind_filter: Option<CodeUnitKind> = kind_filter.as_ref().and_then(|k| match k.as_str() {
        "function" | "fn" => Some(CodeUnitKind::Function),
        "struct" => Some(CodeUnitKind::Struct),
        "enum" => Some(CodeUnitKind::Enum),
        "trait" => Some(CodeUnitKind::Trait),
        "impl" => Some(CodeUnitKind::Impl),
        "module" | "mod" => Some(CodeUnitKind::Module),
        "const" => Some(CodeUnitKind::Const),
        "static" => Some(CodeUnitKind::Static),
        "type" => Some(CodeUnitKind::TypeAlias),
        "macro" => Some(CodeUnitKind::Macro),
        _ => None,
    });

    let mut all_units = CodeUnits::new();

    for spec_config in &config.specs {
        // Build include/exclude patterns
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

        // Walk source files and extract code units
        let walker = WalkBuilder::new(&project_root)
            .follow_links(true)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker.flatten() {
            let path = entry.path();

            // Only process Rust files
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }

            // Check include/exclude patterns
            let relative = path.strip_prefix(&project_root).unwrap_or(path);
            let relative_str = relative.to_string_lossy();

            let included = include
                .iter()
                .any(|pattern| matches_glob(&relative_str, pattern));
            let excluded = exclude
                .iter()
                .any(|pattern| matches_glob(&relative_str, pattern));

            if included
                && !excluded
                && let Ok(content) = std::fs::read_to_string(path)
            {
                let units = extract_rust(path, &content);
                all_units.extend(units);
            }
        }
    }

    // Apply filters
    let filtered_units: Vec<&CodeUnit> = all_units
        .units
        .iter()
        .filter(|u| {
            // Kind filter
            if let Some(ref k) = kind_filter
                && u.kind != *k
            {
                return false;
            }
            // Path filter
            if let Some(ref p) = path_filter {
                let path_str = u.file.to_string_lossy();
                if !path_str.contains(p) {
                    return false;
                }
            }
            // Uncovered filter
            if uncovered_only && !u.rule_refs.is_empty() {
                return false;
            }
            true
        })
        .collect();

    // Calculate stats
    let total = filtered_units.len();
    let covered = filtered_units
        .iter()
        .filter(|u| !u.rule_refs.is_empty())
        .count();
    let uncovered = total - covered;
    let coverage_pct = if total > 0 {
        (covered as f64 / total as f64) * 100.0
    } else {
        100.0
    };

    if is_json {
        // JSON output for AI agents
        #[derive(facet::Facet)]
        struct UnitJson {
            kind: String,
            name: Option<String>,
            file: String,
            start_line: usize,
            end_line: usize,
            rule_refs: Vec<String>,
            covered: bool,
        }

        #[derive(facet::Facet)]
        struct SummaryJson {
            total: usize,
            covered: usize,
            uncovered: usize,
            coverage_percent: f64,
        }

        #[derive(facet::Facet)]
        struct OutputJson {
            summary: SummaryJson,
            units: Vec<UnitJson>,
        }

        let json_units: Vec<UnitJson> = filtered_units
            .iter()
            .map(|u| UnitJson {
                kind: u.kind.as_str().to_string(),
                name: u.name.clone(),
                file: u.file.to_string_lossy().to_string(),
                start_line: u.start_line,
                end_line: u.end_line,
                rule_refs: u.rule_refs.clone(),
                covered: !u.rule_refs.is_empty(),
            })
            .collect();

        let output = OutputJson {
            summary: SummaryJson {
                total,
                covered,
                uncovered,
                coverage_percent: coverage_pct,
            },
            units: json_units,
        };

        println!(
            "{}",
            facet_json::to_string_pretty(&output).expect("JSON serialization failed")
        );
    } else {
        // Text output
        eprintln!("\n{} Reverse Coverage Report", "##".cyan().bold());
        eprintln!();
        eprintln!(
            "Coverage: {:.1}% ({}/{} code units have spec references)",
            coverage_pct, covered, total
        );
        eprintln!(
            "  {} covered, {} uncovered",
            covered.to_string().green(),
            uncovered.to_string().yellow()
        );
        eprintln!();

        if uncovered_only {
            eprintln!("{} Uncovered code units:", "->".blue().bold());
        } else {
            eprintln!("{} Code units:", "->".blue().bold());
        }
        eprintln!();

        for unit in &filtered_units {
            let relative_path = unit.file.strip_prefix(&project_root).unwrap_or(&unit.file);
            let name = unit.name.as_deref().unwrap_or("<anonymous>");
            let kind = unit.kind.as_str();

            if unit.rule_refs.is_empty() {
                eprintln!(
                    "   {} {}:{} {} {}",
                    "✗".red(),
                    relative_path.display(),
                    unit.start_line,
                    kind.dimmed(),
                    name.yellow()
                );
            } else if !uncovered_only {
                let refs = unit.rule_refs.join(", ");
                eprintln!(
                    "   {} {}:{} {} {} [{}]",
                    "✓".green(),
                    relative_path.display(),
                    unit.start_line,
                    kind.dimmed(),
                    name,
                    refs.cyan()
                );
            }
        }
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
    // Use Facet struct for proper serialization
    #[derive(facet::Facet)]
    struct MatrixRowJson {
        rule_id: String,
        url: String,
        #[facet(default)]
        text: Option<String>,
        #[facet(default)]
        status: Option<String>,
        #[facet(default)]
        level: Option<String>,
        #[facet(rename = "impl")]
        impl_refs: Vec<String>,
        verify: Vec<String>,
        depends: Vec<String>,
    }

    let json_rows: Vec<MatrixRowJson> = rows
        .iter()
        .map(|row| MatrixRowJson {
            rule_id: row.rule_id.clone(),
            url: row.url.clone(),
            text: row.text.clone(),
            status: row.status.clone(),
            level: row.level.clone(),
            impl_refs: row.impl_refs.clone(),
            verify: row.verify_refs.clone(),
            depends: row.depends_refs.clone(),
        })
        .collect();

    facet_json::to_string_pretty(&json_rows).expect("JSON serialization failed")
}

fn render_matrix_html(rows: &[MatrixRow], project_root: &std::path::Path) -> String {
    let mut output = String::new();

    // Get absolute project root for editor links
    let abs_root = if cfg!(not(windows)) {
        project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf())
    } else {
        // Canonicalize on windows converts the path to use extended length path syntax.
        // This is a lot less compatible with all sorts of things and can't deal with linux-style path separators.
        project_root.to_path_buf()
    };
    let root_str = abs_root.display().to_string();

    output.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    output.push_str("<meta charset=\"utf-8\">\n");
    output.push_str("<meta name=\"color-scheme\" content=\"light dark\">\n");
    output.push_str("<title>Traceability Matrix</title>\n");
    output.push_str("<link rel=\"preconnect\" href=\"https://fonts.googleapis.com\">\n");
    output.push_str("<link rel=\"preconnect\" href=\"https://fonts.gstatic.com\" crossorigin>\n");
    output.push_str("<link href=\"https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500&family=Public+Sans:wght@400;500;600&display=swap\" rel=\"stylesheet\">\n");
    output.push_str(
        "<script src=\"https://cdn.jsdelivr.net/npm/fuse.js@7.0.0/dist/fuse.min.js\"></script>\n",
    );
    output.push_str(
        "<script src=\"https://cdn.jsdelivr.net/npm/mark.js@8.11.1/dist/mark.min.js\"></script>\n",
    );
    output.push_str(
        "<script src=\"https://cdn.jsdelivr.net/npm/lucide@0.469.0/dist/umd/lucide.min.js\"></script>\n",
    );
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
  margin: 2rem auto;
  padding: 0 1rem;
  max-width: 1200px;
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
  max-width: 100%;
  table-layout: auto;
  font-family: 'IBM Plex Mono', monospace;
  font-size: 0.9em;
}
thead {
  display: none;
}
td {
  border: none;
  border-bottom: 1px solid light-dark(#e5e5e5, #292e42);
  padding: 12px 16px;
  text-align: left;
}
tr:hover {
  background-color: light-dark(#f5f5f7, #1f2335);
}
tr:target {
  background-color: light-dark(#fef9c3, #3d3520);
}
tr:target .anchor-link {
  opacity: 1;
  color: light-dark(#d97706, #e0af68);
}
tbody tr:not(.section-header) td:first-child {
  border-left: 3px solid transparent;
}
.covered td:first-child {
  border-left-color: light-dark(#16a34a, #9ece6a);
}
.partial td:first-child {
  border-left-color: light-dark(#2563eb, #7aa2f7);
}
.uncovered td:first-child {
  border-left-color: light-dark(#dc2626, #f7768e);
}
.stats {
  display: flex;
  gap: 2rem;
  margin-bottom: 1.5rem;
  font-family: 'Public Sans', system-ui, sans-serif;
}
.stat {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}
.stat-label {
  font-size: 0.85em;
  color: light-dark(#6b7280, #565f89);
}
.stat-value {
  font-size: 1.5em;
  font-weight: 600;
  font-family: 'IBM Plex Mono', monospace;
}
.stat-value-good {
  color: light-dark(#16a34a, #9ece6a);
}
.stat-value-partial {
  color: light-dark(#2563eb, #7aa2f7);
}
.stat-value-bad {
  color: light-dark(#dc2626, #f7768e);
}
.stat-clickable {
  cursor: pointer;
  transition: opacity 0.15s;
}
.stat-clickable:hover {
  opacity: 0.8;
}
.stat-clickable.active {
  text-decoration: underline;
  text-underline-offset: 4px;
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
#filter {
  padding: 0.5rem 0.75rem;
  font-family: inherit;
  font-size: 1rem;
  background: light-dark(#fff, #24283b);
  color: light-dark(#1a1b26, #a9b1d6);
  border: 1px solid light-dark(#d5d5db, #414868);
  border-radius: 6px;
  width: 300px;
}
#filter:focus {
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
.anchor-cell {
  vertical-align: middle;
  text-align: center;
  width: 3rem;
  padding: 0;
}
.anchor-link {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  min-height: 3rem;
  color: light-dark(#1a1b26, #a9b1d6);
  text-decoration: none;
  font-size: 1.4rem;
  font-weight: 600;
  opacity: 0.25;
}
.anchor-link:hover {
  opacity: 1;
  color: light-dark(#7aa2f7, #7aa2f7);
}
tr[id] {
  scroll-margin-top: 6rem;
}
.rule-cell {
  vertical-align: top;
  padding-left: 1rem;
}
.rule-id {
  font-size: 0.85em;
  margin-bottom: 0.5rem;
  display: flex;
  align-items: center;
  gap: 0.4rem;
}
.rule-icon {
  width: 1em;
  height: 1em;
  color: light-dark(#9ca3af, #565f89);
  flex-shrink: 0;
}
.rule-desc {
  font-size: 1em;
  color: light-dark(#374151, #a9b1d6);
  font-family: 'Public Sans', system-ui, sans-serif;
  margin-top: 0.5rem;
  line-height: 1.5;
}
.rule-tags {
  display: inline-flex;
  gap: 0.35rem;
  margin-left: 0.5rem;
}
.tag {
  font-size: 0.7em;
  padding: 0.1rem 0.4rem;
  border-radius: 4px;
  font-family: 'Public Sans', system-ui, sans-serif;
  font-weight: 500;
}
.tag-status {
  background: light-dark(#e8e8ed, #292e42);
  color: light-dark(#6b7280, #737aa2);
}
.tag-status-draft {
  background: light-dark(#e8e8ed, #292e42);
  color: light-dark(#6b7280, #565f89);
  font-style: italic;
}
.tag-status-deprecated {
  background: light-dark(#fef3c7, #3d3520);
  color: light-dark(#b45309, #e0af68);
}
.tag-status-removed {
  background: light-dark(#e5e7eb, #292e42);
  color: light-dark(#9ca3af, #414868);
}
.tag-level-must {
  background: light-dark(#fee2e2, #2d1f1f);
  color: light-dark(#dc2626, #f7768e);
}
.tag-level-should {
  background: light-dark(#fef3c7, #3d3520);
  color: light-dark(#d97706, #e0af68);
}
.tag-level-may {
  background: light-dark(#dbeafe, #1e2a4a);
  color: light-dark(#2563eb, #7aa2f7);
}
.refs-cell {
  vertical-align: top;
  font-size: 0.85em;
}
.ref-line {
  white-space: nowrap;
  margin-bottom: 0.35rem;
  display: flex;
  align-items: center;
  gap: 0.35rem;
}
.ref-line:last-child {
  margin-bottom: 0;
}
.ref-icon {
  width: 1em;
  height: 1em;
  flex-shrink: 0;
  opacity: 0.7;
}
.ref-icon-impl {
  color: light-dark(#16a34a, #9ece6a);
}
.ref-icon-verify {
  color: light-dark(#2563eb, #7aa2f7);
}
.ref-icon-depends {
  color: light-dark(#d97706, #e0af68);
}
.file-path {
  color: light-dark(#9ca3af, #565f89);
}
.file-name {
  color: light-dark(#2563eb, #7aa2f7);
}
.file-line {
  color: light-dark(#16a34a, #9ece6a);
}
label {
  color: light-dark(#374151, #9aa5ce);
}
.custom-dropdown {
  position: relative;
  display: inline-block;
}
.dropdown-selected {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 0.75rem;
  background: light-dark(#fff, #24283b);
  border: 1px solid light-dark(#d5d5db, #414868);
  border-radius: 6px;
  cursor: pointer;
  font-family: 'Public Sans', system-ui, sans-serif;
  font-size: 1rem;
  color: light-dark(#1a1b26, #a9b1d6);
  min-width: 100px;
}
.dropdown-selected:hover {
  background: light-dark(#f5f5f7, #292e42);
}
.dropdown-selected svg {
  width: 1rem;
  height: 1rem;
  flex-shrink: 0;
}
.dropdown-selected svg path {
  fill: currentColor;
}
.dropdown-selected .chevron {
  margin-left: auto;
  opacity: 0.5;
}
.dropdown-menu {
  position: absolute;
  top: 100%;
  left: 0;
  margin-top: 4px;
  background: light-dark(#fff, #24283b);
  border: 1px solid light-dark(#d5d5db, #414868);
  border-radius: 6px;
  box-shadow: 0 4px 12px light-dark(rgba(0,0,0,0.1), rgba(0,0,0,0.3));
  z-index: 100;
  min-width: 100%;
  display: none;
}
.custom-dropdown.open .dropdown-menu {
  display: block;
}
.dropdown-option {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 0.75rem;
  cursor: pointer;
  font-family: 'Public Sans', system-ui, sans-serif;
  font-size: 1rem;
  color: light-dark(#374151, #a9b1d6);
  white-space: nowrap;
}
.dropdown-option:first-child {
  border-radius: 5px 5px 0 0;
}
.dropdown-option:last-child {
  border-radius: 0 0 5px 5px;
}
.dropdown-option:hover {
  background: light-dark(#f5f5f7, #292e42);
}
.dropdown-option.active {
  background: light-dark(#e8e8ed, #292e42);
  color: light-dark(#1a1b26, #c0caf5);
}
.dropdown-option svg {
  width: 1rem;
  height: 1rem;
  flex-shrink: 0;
}
.dropdown-option svg path {
  fill: currentColor;
}
.level-dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.level-dot-all { background: light-dark(#6b7280, #737aa2); }
.level-dot-must { background: light-dark(#dc2626, #f7768e); }
.level-dot-should { background: light-dark(#d97706, #e0af68); }
.level-dot-may { background: light-dark(#2563eb, #7aa2f7); }
.filter-notice {
  display: none;
  margin-top: 1rem;
  padding: 0.75rem 1rem;
  background: light-dark(#f5f5f7, #24283b);
  border: 1px solid light-dark(#d5d5db, #414868);
  border-radius: 6px;
  font-family: 'Public Sans', system-ui, sans-serif;
  color: light-dark(#6b7280, #737aa2);
  text-align: center;
}
.filter-notice.visible {
  display: block;
}
.filter-notice a {
  color: light-dark(#2563eb, #7aa2f7);
  text-decoration: none;
  margin-left: 0.5rem;
}
.filter-notice a:hover {
  text-decoration: underline;
}
.section-header td {
  background: transparent;
  border: none;
  font-family: 'Public Sans', system-ui, sans-serif;
  font-weight: 600;
  font-size: 1.4em;
  color: light-dark(#1a1b26, #c0caf5);
  padding: 1.5rem 0 0.75rem 0;
  text-transform: capitalize;
  letter-spacing: 0.02em;
}
.section-header:hover {
  background: transparent;
}
code {
  background: light-dark(#e8e8ed, #292e42);
  padding: 0.1rem 0.3rem;
  border-radius: 3px;
  font-family: 'IBM Plex Mono', monospace;
  font-size: 0.9em;
}
kw-must, kw-must-not, kw-required, kw-shall, kw-shall-not {
  color: light-dark(#dc2626, #f7768e);
  font-weight: 600;
}
kw-should, kw-should-not, kw-recommended, kw-not-recommended {
  color: light-dark(#d97706, #e0af68);
  font-weight: 600;
}
kw-may, kw-optional {
  color: light-dark(#2563eb, #7aa2f7);
  font-weight: 600;
}
mark {
  background: light-dark(#fef08a, #3d3520);
  color: inherit;
  padding: 0.1rem 0.15rem;
  border-radius: 2px;
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
  idea: {{ name: "IntelliJ", urlTemplate: (path, line) => `idea://open?file=${{path}}&line=${{line}}` }},
  sublime: {{ name: "Sublime", urlTemplate: (path, line) => `subl://open?url=file://${{path}}&line=${{line}}` }},
  vim: {{ name: "Vim", urlTemplate: (path, line) => `mvim://open?url=file://${{path}}&line=${{line}}` }},
  emacs: {{ name: "Emacs", urlTemplate: (path, line) => `emacs://open?url=file://${{path}}&line=${{line}}` }},
}};

function getEditor() {{
  return localStorage.getItem('tracey-editor') || 'zed';
}}

function setEditor(editor) {{
  localStorage.setItem('tracey-editor', editor);
  updateAllLinks();
  updateEditorDropdown();
}}

function toggleDropdown(id) {{
  // Close other dropdowns first
  document.querySelectorAll('.custom-dropdown.open').forEach(d => {{
    if (d.id !== id) d.classList.remove('open');
  }});
  document.getElementById(id).classList.toggle('open');
}}

function selectEditor(editor) {{
  setEditor(editor);
  document.getElementById('editor-dropdown').classList.remove('open');
}}

function updateEditorDropdown() {{
  const editor = getEditor();
  const config = EDITORS[editor];
  // Update the selected display
  const option = document.querySelector(`#editor-dropdown .dropdown-option[data-editor="${{editor}}"]`);
  if (option) {{
    document.getElementById('editor-icon').innerHTML = option.querySelector('svg').outerHTML;
    document.getElementById('editor-name').textContent = config.name;
  }}
  // Update active state in menu
  document.querySelectorAll('#editor-dropdown .dropdown-option').forEach(opt => {{
    opt.classList.toggle('active', opt.dataset.editor === editor);
  }});
}}

const LEVELS = {{
  all: {{ name: 'All', dotClass: 'level-dot-all' }},
  must: {{ name: 'MUST', dotClass: 'level-dot-must' }},
  should: {{ name: 'SHOULD', dotClass: 'level-dot-should' }},
  may: {{ name: 'MAY', dotClass: 'level-dot-may' }},
}};

let currentLevel = 'all';

function selectLevel(level) {{
  currentLevel = level;
  updateLevelDropdown();
  filterTable();
  document.getElementById('level-dropdown').classList.remove('open');
}}

function updateLevelDropdown() {{
  const config = LEVELS[currentLevel];
  document.getElementById('level-icon').innerHTML = `<span class="level-dot ${{config.dotClass}}"></span>`;
  document.getElementById('level-name').textContent = config.name;
  // Update active state in menu
  document.querySelectorAll('#level-dropdown .dropdown-option').forEach(opt => {{
    opt.classList.toggle('active', opt.dataset.value === currentLevel);
  }});
}}

function clearFilters() {{
  // Clear text filter
  document.getElementById('filter').value = '';
  // Reset level filter
  currentLevel = 'all';
  updateLevelDropdown();
  // Clear coverage filter
  coverageFilter = null;
  document.querySelectorAll('.stat-clickable').forEach(s => s.classList.remove('active'));
  // Re-run filter
  filterTable();
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

let fuse = null;
let markInstance = null;
let ruleRows = [];
let coverageFilter = null; // 'no-impl' or 'no-verify' or null

function initSearch() {{
  const rows = document.querySelectorAll('tbody tr:not(.section-header)');
  ruleRows = Array.from(rows).map((row, index) => ({{
    index,
    row,
    ruleId: row.querySelector('.rule-id')?.textContent || '',
    desc: row.querySelector('.rule-desc')?.textContent || '',
    refs: row.querySelector('.refs-cell')?.textContent || '',
    hasImpl: !!row.querySelector('.ref-icon-impl'),
    hasVerify: !!row.querySelector('.ref-icon-verify'),
  }}));

  fuse = new Fuse(ruleRows, {{
    keys: ['ruleId', 'desc', 'refs'],
    threshold: 0.3,
    ignoreLocation: true,
    includeMatches: true,
  }});

  markInstance = new Mark(document.querySelector('tbody'));
}}

function setCoverageFilter(filter) {{
  const implStat = document.getElementById('stat-impl');
  const verifyStat = document.getElementById('stat-verify');

  if (coverageFilter === filter) {{
    // Toggle off
    coverageFilter = null;
    implStat?.classList.remove('active');
    verifyStat?.classList.remove('active');
  }} else {{
    coverageFilter = filter;
    implStat?.classList.toggle('active', filter === 'no-impl');
    verifyStat?.classList.toggle('active', filter === 'no-verify');
  }}
  filterTable();
}}

function filterTable() {{
  const filter = document.getElementById('filter').value;
  const levelFilter = currentLevel;
  const rows = document.querySelectorAll('tbody tr');

  // Clear previous highlights
  if (markInstance) {{
    markInstance.unmark();
  }}

  // Determine which rows match the search
  let matchingIndices = new Set();
  if (filter === '') {{
    // No search filter - all rows match
    ruleRows.forEach(r => matchingIndices.add(r.index));
  }} else if (fuse) {{
    // Fuzzy search
    const results = fuse.search(filter);
    results.forEach(result => matchingIndices.add(result.item.index));
  }}

  let currentSectionHeader = null;
  let currentSectionVisible = false;
  let totalRules = 0;
  let hiddenRules = 0;

  rows.forEach((row, idx) => {{
    if (row.classList.contains('section-header')) {{
      // Hide section header initially, show if any child matches
      if (currentSectionHeader && currentSectionVisible) {{
        currentSectionHeader.style.display = '';
      }}
      currentSectionHeader = row;
      currentSectionVisible = false;
      row.style.display = 'none';
      return;
    }}

    totalRules++;

    // Find the ruleRow index for this row
    const ruleRowIdx = ruleRows.findIndex(r => r.row === row);
    const matchesText = ruleRowIdx >= 0 && matchingIndices.has(ruleRowIdx);

    // Check level filter by looking for keyword elements in the row
    let matchesLevel = true;
    if (levelFilter === 'must') {{
      matchesLevel = !!row.querySelector('kw-must, kw-must-not, kw-required, kw-shall, kw-shall-not');
    }} else if (levelFilter === 'should') {{
      matchesLevel = !!row.querySelector('kw-should, kw-should-not, kw-recommended, kw-not-recommended');
    }} else if (levelFilter === 'may') {{
      matchesLevel = !!row.querySelector('kw-may, kw-optional');
    }}

    // Check coverage filter
    let matchesCoverage = true;
    if (coverageFilter && ruleRowIdx >= 0) {{
      const ruleRow = ruleRows[ruleRowIdx];
      if (coverageFilter === 'no-impl') {{
        matchesCoverage = !ruleRow.hasImpl;
      }} else if (coverageFilter === 'no-verify') {{
        matchesCoverage = !ruleRow.hasVerify;
      }}
    }}

    const visible = matchesText && matchesLevel && matchesCoverage;
    row.style.display = visible ? '' : 'none';

    if (visible) {{
      currentSectionVisible = true;
    }} else {{
      hiddenRules++;
    }}
  }});

  // Handle last section header
  if (currentSectionHeader && currentSectionVisible) {{
    currentSectionHeader.style.display = '';
  }}

  // Update filter notice
  const notice = document.getElementById('filter-notice');
  const noticeText = document.getElementById('filter-notice-text');
  if (hiddenRules > 0) {{
    noticeText.textContent = `${{hiddenRules}} rule${{hiddenRules === 1 ? '' : 's'}} hidden by filters`;
    notice.classList.add('visible');
  }} else {{
    notice.classList.remove('visible');
  }}

  // Highlight matches
  if (filter && markInstance) {{
    markInstance.mark(filter, {{
      separateWordSearch: false,
      accuracy: 'partially',
    }});
  }}
}}

document.addEventListener('DOMContentLoaded', () => {{
  updateLevelDropdown();
  updateEditorDropdown();
  updateAllLinks();
  initSearch();
  lucide.createIcons();
}});

document.addEventListener('keydown', (e) => {{
  if ((e.metaKey || e.ctrlKey) && e.key === 'k') {{
    e.preventDefault();
    document.getElementById('filter').focus();
  }}
}});

// Close dropdowns when clicking outside
document.addEventListener('click', (e) => {{
  document.querySelectorAll('.custom-dropdown').forEach(dropdown => {{
    if (!dropdown.contains(e.target)) {{
      dropdown.classList.remove('open');
    }}
  }});
}});
"#,
        root_str.replace('\\', "\\\\").replace('"', "\\\"")
    ));
    output.push_str("</script>\n");
    output.push_str("</head>\n<body>\n");
    output.push_str("<h1>Traceability Matrix</h1>\n");

    // Calculate coverage stats
    let total_rules = rows.len();
    let rules_with_impl = rows.iter().filter(|r| !r.impl_refs.is_empty()).count();
    let rules_with_verify = rows.iter().filter(|r| !r.verify_refs.is_empty()).count();
    let impl_pct = if total_rules > 0 {
        (rules_with_impl as f64 / total_rules as f64) * 100.0
    } else {
        0.0
    };
    let verify_pct = if total_rules > 0 {
        (rules_with_verify as f64 / total_rules as f64) * 100.0
    } else {
        0.0
    };

    // Color class based on percentage
    let pct_class = |pct: f64| {
        if pct >= 80.0 {
            "stat-value-good"
        } else if pct >= 50.0 {
            "stat-value-partial"
        } else {
            "stat-value-bad"
        }
    };

    output.push_str("<div class=\"stats\">\n");
    output.push_str(&format!(
        "<div class=\"stat\"><span class=\"stat-label\">Total Rules</span><span class=\"stat-value\">{}</span></div>\n",
        total_rules
    ));
    output.push_str(&format!(
        "<div class=\"stat stat-clickable\" id=\"stat-impl\" onclick=\"setCoverageFilter('no-impl')\" title=\"Click to show unimplemented rules\"><span class=\"stat-label\">Impl Coverage</span><span class=\"stat-value {}\">{:.1}%</span></div>\n",
        pct_class(impl_pct), impl_pct
    ));
    output.push_str(&format!(
        "<div class=\"stat stat-clickable\" id=\"stat-verify\" onclick=\"setCoverageFilter('no-verify')\" title=\"Click to show untested rules\"><span class=\"stat-label\">Test Coverage</span><span class=\"stat-value {}\">{:.1}%</span></div>\n",
        pct_class(verify_pct), verify_pct
    ));
    output.push_str("</div>\n");

    output.push_str("<div class=\"controls\">\n");
    output.push_str(
        "<input type=\"text\" id=\"filter\" placeholder=\"Filter rules...\" onkeyup=\"filterTable()\">\n",
    );
    // Update placeholder with platform-appropriate shortcut hint
    output.push_str(
        r#"<script>
(function() {
  const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
  const hint = isMac ? '⌘K' : 'Ctrl+K';
  document.getElementById('filter').placeholder = `Filter rules... (${hint})`;
})();
</script>
"#,
    );
    // Level dropdown
    output.push_str(r#"<div class="custom-dropdown" id="level-dropdown">"#);
    output
        .push_str(r#"<div class="dropdown-selected" onclick="toggleDropdown('level-dropdown')">"#);
    output.push_str(r#"<span id="level-icon"><span class="level-dot level-dot-all"></span></span><span id="level-name">All</span>"#);
    output.push_str(r#"<svg class="chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 9l6 6 6-6"/></svg>"#);
    output.push_str("</div>\n");
    output.push_str(r#"<div class="dropdown-menu">"#);
    output.push_str(r#"<div class="dropdown-option" data-value="all" onclick="selectLevel('all')"><span class="level-dot level-dot-all"></span><span>All</span></div>"#);
    output.push_str(r#"<div class="dropdown-option" data-value="must" onclick="selectLevel('must')"><span class="level-dot level-dot-must"></span><span>MUST</span></div>"#);
    output.push_str(r#"<div class="dropdown-option" data-value="should" onclick="selectLevel('should')"><span class="level-dot level-dot-should"></span><span>SHOULD</span></div>"#);
    output.push_str(r#"<div class="dropdown-option" data-value="may" onclick="selectLevel('may')"><span class="level-dot level-dot-may"></span><span>MAY</span></div>"#);
    output.push_str("</div></div>\n");
    // Editor dropdown
    output.push_str(r#"<div class="custom-dropdown" id="editor-dropdown">"#);
    output
        .push_str(r#"<div class="dropdown-selected" onclick="toggleDropdown('editor-dropdown')">"#);
    output.push_str(r#"<span id="editor-icon"></span><span id="editor-name"></span>"#);
    output.push_str(r#"<svg class="chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 9l6 6 6-6"/></svg>"#);
    output.push_str("</div>\n");
    output.push_str(r#"<div class="dropdown-menu">"#);
    // Zed
    output.push_str(
        r#"<div class="dropdown-option" data-editor="zed" onclick="selectEditor('zed')">"#,
    );
    output.push_str(r#"<svg viewBox="0 0 90 90" xmlns="http://www.w3.org/2000/svg"><path fill-rule="evenodd" clip-rule="evenodd" d="M8.4375 5.625C6.8842 5.625 5.625 6.8842 5.625 8.4375V70.3125H0V8.4375C0 3.7776 3.7776 0 8.4375 0H83.7925C87.551 0 89.4333 4.5442 86.7756 7.20186L40.3642 53.6133H53.4375V47.8125H59.0625V55.0195C59.0625 57.3495 57.1737 59.2383 54.8438 59.2383H34.7392L25.0712 68.9062H68.9062V33.75H74.5312V68.9062C74.5312 72.0128 72.0128 74.5312 68.9062 74.5312H19.4462L9.60248 84.375H81.5625C83.1158 84.375 84.375 83.1158 84.375 81.5625V19.6875H90V81.5625C90 86.2224 86.2224 90 81.5625 90H6.20749C2.44898 90 0.566723 85.4558 3.22438 82.7981L49.46 36.5625H36.5625V42.1875H30.9375V35.1562C30.9375 32.8263 32.8263 30.9375 35.1562 30.9375H55.085L64.9288 21.0938H21.0938V56.25H15.4688V21.0938C15.4688 17.9871 17.9871 15.4688 21.0938 15.4688H70.5538L80.3975 5.625H8.4375Z"/></svg>"#);
    output.push_str("<span>Zed</span></div>\n");
    // VS Code
    output.push_str(
        r#"<div class="dropdown-option" data-editor="vscode" onclick="selectEditor('vscode')">"#,
    );
    output.push_str(r#"<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg"><path d="M74.9 11L45.3 40.4L22.7 22.1L11 27.9V72.1L22.7 77.9L45.3 59.6L74.9 89L100 77.9V22.1L74.9 11ZM22.7 60.1V39.9L35.5 50L22.7 60.1ZM74.9 60.1L52.8 50L74.9 39.9V60.1Z"/></svg>"#);
    output.push_str("<span>VS Code</span></div>\n");
    // IntelliJ IDEA
    output.push_str(
        r#"<div class="dropdown-option" data-editor="idea" onclick="selectEditor('idea')">"#,
    );
    output.push_str(r#"<svg viewBox="0 0 128 128" xmlns="http://www.w3.org/2000/svg"><path d="M14 103h100V25H14v78zm8-70h12v62H22V33zm50 54H44v-8h28v8zm6-46h12v54H78V41z"/></svg>"#);
    output.push_str("<span>IntelliJ</span></div>\n");
    // Sublime Text
    output.push_str(
        r#"<div class="dropdown-option" data-editor="sublime" onclick="selectEditor('sublime')">"#,
    );
    output.push_str(r#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M21 3L3 9.25V11.5L21 17.75V15.25L8.25 11.5L21 7.75V3M3 12.25V14.5L21 20.75V18.25L8.25 14.5L3 12.25Z"/></svg>"#);
    output.push_str("<span>Sublime</span></div>\n");
    // Vim (MacVim)
    output.push_str(
        r#"<div class="dropdown-option" data-editor="vim" onclick="selectEditor('vim')">"#,
    );
    output.push_str(r#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M12 2L3 7V17L12 22L21 17V7L12 2M12 4.5L18 8V16L12 19.5L6 16V8L12 4.5M12 7L8 9.5V14.5L12 17L16 14.5V9.5L12 7Z"/></svg>"#);
    output.push_str("<span>Vim</span></div>\n");
    // Emacs
    output.push_str(
        r#"<div class="dropdown-option" data-editor="emacs" onclick="selectEditor('emacs')">"#,
    );
    output.push_str(r#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M12 2C6.48 2 2 6.48 2 12C2 17.52 6.48 22 12 22C17.52 22 22 17.52 22 12C22 6.48 17.52 2 12 2M12 4C16.42 4 20 7.58 20 12C20 16.42 16.42 20 12 20C7.58 20 4 16.42 4 12C4 7.58 7.58 4 12 4M8 7V17H16V15H10V7H8Z"/></svg>"#);
    output.push_str("<span>Emacs</span></div>\n");
    output.push_str("</div></div>\n");
    output.push_str("</div>\n");
    output.push_str("<table>\n");
    output.push_str("<thead>\n");
    output.push_str("<tr><th></th><th>Rule</th><th>References</th></tr>\n");
    output.push_str("</thead>\n");
    output.push_str("<tbody>\n");

    // Helper for light markdown rendering (bold, italic, code)
    let render_markdown = |text: &str| -> String {
        let escaped = html_escape::encode_text(text).to_string();
        // Process `code` first (backticks)
        let with_code = {
            let mut result = String::new();
            let chars = escaped.chars();
            let mut in_code = false;
            for c in chars {
                if c == '`' {
                    if in_code {
                        result.push_str("</code>");
                        in_code = false;
                    } else {
                        result.push_str("<code>");
                        in_code = true;
                    }
                } else {
                    result.push(c);
                }
            }
            // Close unclosed code tag
            if in_code {
                result.push_str("</code>");
            }
            result
        };
        // Process **bold** and *italic* (simplified, non-nested)
        let with_bold = with_code.replace("**", "\x00BOLD\x00");
        let mut bold_result = String::new();
        let mut in_bold = false;
        for part in with_bold.split("\x00BOLD\x00") {
            if in_bold {
                bold_result.push_str("</strong>");
            }
            bold_result.push_str(part);
            if !in_bold && with_bold.matches("\x00BOLD\x00").count() > 0 {
                bold_result.push_str("<strong>");
            }
            in_bold = !in_bold;
        }
        // Remove trailing <strong> if odd number of markers
        if bold_result.ends_with("<strong>") {
            bold_result.truncate(bold_result.len() - 8);
        }

        // Wrap RFC 2119 keywords in custom elements
        // Order matters: longer phrases first to avoid partial matches
        bold_result
            .replace("MUST NOT", "<kw-must-not>MUST NOT</kw-must-not>")
            .replace("SHALL NOT", "<kw-shall-not>SHALL NOT</kw-shall-not>")
            .replace("SHOULD NOT", "<kw-should-not>SHOULD NOT</kw-should-not>")
            .replace(
                "NOT RECOMMENDED",
                "<kw-not-recommended>NOT RECOMMENDED</kw-not-recommended>",
            )
            .replace("MUST", "<kw-must>MUST</kw-must>")
            .replace("REQUIRED", "<kw-required>REQUIRED</kw-required>")
            .replace("SHALL", "<kw-shall>SHALL</kw-shall>")
            .replace("SHOULD", "<kw-should>SHOULD</kw-should>")
            .replace(
                "RECOMMENDED",
                "<kw-recommended>RECOMMENDED</kw-recommended>",
            )
            .replace("MAY", "<kw-may>MAY</kw-may>")
            .replace("OPTIONAL", "<kw-optional>OPTIONAL</kw-optional>")
    };

    // Helper to format a single reference as a link
    let format_ref = |ref_type: &str, r: &str| -> String {
        // Lucide icon names and titles
        let (icon_name, icon_class, title) = match ref_type {
            "impl" => ("code-xml", "ref-icon-impl", "implements"),
            "verify" => ("circle-check", "ref-icon-verify", "verified by"),
            "depends" => ("package", "ref-icon-depends", "depends on"),
            _ => ("code-xml", "ref-icon-impl", "implements"),
        };
        let icon = format!(
            r#"<i data-lucide="{}" class="ref-icon {}"><title>{}</title></i>"#,
            icon_name, icon_class, title
        );

        // Parse "path:line" format
        if let Some((path, line)) = r.rsplit_once(':') {
            // Split path into directory and filename
            let (dir, filename) = if let Some(pos) = path.rfind(std::path::MAIN_SEPARATOR) {
                (&path[..=pos], &path[pos + 1..])
            } else {
                ("", path)
            };
            format!(
                "<div class=\"ref-line\">{}<a class=\"file-link\" data-path=\"{}\" data-line=\"{}\" href=\"#\"><span class=\"file-path\">{}</span><span class=\"file-name\">{}</span><span class=\"file-line\">:{}</span></a></div>",
                icon,
                html_escape::encode_double_quoted_attribute(path),
                line,
                html_escape::encode_text(dir),
                html_escape::encode_text(filename),
                line
            )
        } else {
            format!(
                "<div class=\"ref-line\">{}{}</div>",
                icon,
                html_escape::encode_text(r)
            )
        }
    };

    // Track current section for grouping
    let mut current_section: Option<String> = None;

    for row in rows {
        // Extract first path segment for section grouping
        let section = row
            .rule_id
            .split('.')
            .next()
            .unwrap_or(&row.rule_id)
            .to_string();

        // Emit section header if section changed
        if current_section.as_ref() != Some(&section) {
            output.push_str(&format!(
                "<tr class=\"section-header\"><td colspan=\"3\">{}</td></tr>\n",
                html_escape::encode_text(&section)
            ));
            current_section = Some(section);
        }

        let has_impl = !row.impl_refs.is_empty();
        let has_verify = !row.verify_refs.is_empty();
        let row_class = if has_impl && has_verify {
            "covered"
        } else if has_impl || has_verify {
            "partial"
        } else {
            "uncovered"
        };

        let status = row.status.as_deref();
        let level = row.level.as_deref();

        // Build tags HTML
        let mut tags_html = String::new();
        if status.is_some() || level.is_some() {
            tags_html.push_str("<span class=\"rule-tags\">");
            if let Some(s) = status {
                let tag_class = match s {
                    "draft" => "tag tag-status-draft",
                    "deprecated" => "tag tag-status-deprecated",
                    "removed" => "tag tag-status-removed",
                    _ => "tag tag-status",
                };
                tags_html.push_str(&format!(
                    "<span class=\"{}\">{}</span>",
                    tag_class,
                    html_escape::encode_text(s)
                ));
            }
            if let Some(l) = level {
                let tag_class = match l {
                    "must" => "tag tag-level-must",
                    "should" => "tag tag-level-should",
                    "may" => "tag tag-level-may",
                    _ => "tag tag-status",
                };
                tags_html.push_str(&format!(
                    "<span class=\"{}\">{}</span>",
                    tag_class,
                    html_escape::encode_text(l)
                ));
            }
            tags_html.push_str("</span>");
        }

        // Format rule ID as a link to the spec file (opened in editor)
        // Include a small markdown icon to indicate it's clickable
        let md_icon =
            r#"<i data-lucide="file-text" class="rule-icon"><title>Open in editor</title></i>"#;
        let rule_link = match (&row.source_file, row.source_line) {
            (Some(source_file), Some(source_line)) => {
                format!(
                    "{}<a href=\"#\" class=\"spec-link\" data-path=\"{}\" data-line=\"{}\">{}</a>",
                    md_icon,
                    html_escape::encode_double_quoted_attribute(source_file),
                    source_line,
                    html_escape::encode_text(&row.rule_id)
                )
            }
            (Some(source_file), None) => {
                format!(
                    "{}<a href=\"#\" class=\"spec-link\" data-path=\"{}\" data-line=\"1\">{}</a>",
                    md_icon,
                    html_escape::encode_double_quoted_attribute(source_file),
                    html_escape::encode_text(&row.rule_id)
                )
            }
            _ => html_escape::encode_text(&row.rule_id).to_string(),
        };

        // Format description with light markdown rendering
        let desc_html = match &row.text {
            Some(text) if !text.is_empty() => {
                format!("<div class=\"rule-desc\">{}</div>", render_markdown(text))
            }
            _ => String::new(),
        };

        // Build rule cell: ID + tags + description
        let rule_cell = format!(
            "<div class=\"rule-id\">{}{}</div>{}",
            rule_link, tags_html, desc_html
        );

        // Build references cell
        let mut refs_html = String::new();
        for r in &row.impl_refs {
            refs_html.push_str(&format_ref("impl", r));
        }
        for r in &row.verify_refs {
            refs_html.push_str(&format_ref("verify", r));
        }
        for r in &row.depends_refs {
            refs_html.push_str(&format_ref("depends", r));
        }
        if refs_html.is_empty() {
            refs_html.push_str("<span class=\"ref-type\">-</span>");
        }

        output.push_str(&format!(
            "<tr class=\"{}\" id=\"{}\"><td class=\"anchor-cell\"><a href=\"#{}\" class=\"anchor-link\">#</a></td><td class=\"rule-cell\">{}</td><td class=\"refs-cell\">{}</td></tr>\n",
            row_class,
            html_escape::encode_double_quoted_attribute(&row.rule_id),
            html_escape::encode_double_quoted_attribute(&row.rule_id),
            rule_cell,
            refs_html
        ));
    }

    output.push_str("</tbody>\n");
    output.push_str("</table>\n");
    output.push_str(r##"<div class="filter-notice" id="filter-notice"><span id="filter-notice-text"></span><a href="#" onclick="clearFilters(); return false;">Clear</a></div>"##);
    output.push_str("\n</body>\n</html>\n");

    output
}

/// Load a SpecManifest by extracting rules from markdown files matching a glob pattern
pub(crate) fn load_manifest_from_glob(
    root: &std::path::Path,
    pattern: &str,
) -> Result<SpecManifest> {
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

        let result = MarkdownProcessor::process_with_path(&content, Some(path))
            .wrap_err_with(|| format!("Failed to process {}", path.display()))?;

        // Display warnings for rule quality issues
        for warning in &result.warnings {
            let message = match &warning.kind {
                MarkdownWarningKind::NoRfc2119Keyword => "no RFC 2119 keyword".to_string(),
                MarkdownWarningKind::NegativeRequirement(kw) => {
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
