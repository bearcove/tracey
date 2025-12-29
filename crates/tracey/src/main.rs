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

    let args: Args =
        facet_args::from_std_args().wrap_err("Failed to parse command line arguments")?;

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
            let file_manifest = RulesManifest::from_rules(&result.rules, "");
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
        .map(|(id, entry)| (id, tracey_core::RuleInfo { url: entry.url }))
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
