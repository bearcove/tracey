//! tracey - Measure spec coverage in Rust codebases
//!
//! tracey parses Rust source files to find references to specification rules
//! (in the format `[rule.id]` in comments) and compares them against a spec
//! manifest to produce coverage reports.

mod config;
mod coverage;
mod lexer;
mod scanner;
mod spec;

use color_eyre::eyre::{Result, WrapErr};
use config::Config;
use coverage::CoverageReport;
use facet_args as args;
use lexer::RefVerb;
use owo_colors::OwoColorize;
use spec::SpecManifest;
use std::path::PathBuf;

/// CLI arguments
#[derive(Debug, facet::Facet)]
struct Args {
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
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let args: Args =
        facet_args::from_std_args().wrap_err("Failed to parse command line arguments")?;

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
            manifest.rules.len().to_string().green()
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

        let references = scanner::scan_directory(&project_root, &include, &exclude)?;

        eprintln!(
            "   Found {} rule references",
            references.len().to_string().green()
        );

        // Compute coverage
        let report = CoverageReport::compute(spec_name.clone(), &manifest, references);

        // Print report
        print_report(&report, args.verbose);

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

fn print_report(report: &CoverageReport, verbose: bool) {
    println!();
    println!(
        "{} {} Coverage Report",
        "##".bold(),
        report.spec_name.cyan().bold()
    );
    println!();

    // Coverage summary
    let percent = report.coverage_percent();
    let percent_str = format!("{:.1}%", percent);
    let color_percent = if percent >= 80.0 {
        percent_str.green().to_string()
    } else if percent >= 50.0 {
        percent_str.yellow().to_string()
    } else {
        percent_str.red().to_string()
    };

    println!(
        "Coverage: {} ({}/{} rules)",
        color_percent,
        report.covered_rules.len(),
        report.total_rules
    );

    // Show verb breakdown
    let verb_order = [
        RefVerb::Define,
        RefVerb::Impl,
        RefVerb::Verify,
        RefVerb::Depends,
        RefVerb::Related,
    ];
    let mut verb_counts: Vec<(&str, usize)> = Vec::new();
    for verb in &verb_order {
        if let Some(by_rule) = report.references_by_verb.get(verb) {
            let count: usize = by_rule.values().map(|v| v.len()).sum();
            if count > 0 {
                verb_counts.push((verb.as_str(), count));
            }
        }
    }
    if !verb_counts.is_empty() {
        let breakdown: Vec<String> = verb_counts
            .iter()
            .map(|(verb, count)| format!("{} {}", count, verb))
            .collect();
        println!("  References: {}", breakdown.join(", ").dimmed());
    }
    println!();

    // Invalid references (errors)
    if !report.invalid_references.is_empty() {
        println!(
            "{} Invalid References ({}):",
            "!".red().bold(),
            report.invalid_references.len()
        );
        for r in &report.invalid_references {
            println!(
                "  {} {}:{} - unknown rule [{} {}]",
                "-".red(),
                r.file,
                r.line,
                r.verb.as_str().dimmed(),
                r.rule_id.yellow()
            );
        }
        println!();
    }

    // Uncovered rules
    if !report.uncovered_rules.is_empty() {
        println!(
            "{} Uncovered Rules ({}):",
            "?".yellow().bold(),
            report.uncovered_rules.len()
        );

        let mut uncovered: Vec<_> = report.uncovered_rules.iter().collect();
        uncovered.sort();

        for rule_id in uncovered {
            println!("  {} [{}]", "-".yellow(), rule_id.dimmed());
        }
        println!();
    }

    // Verbose: show all references grouped by verb
    if verbose && !report.references_by_verb.is_empty() {
        for verb in &verb_order {
            if let Some(by_rule) = report.references_by_verb.get(verb) {
                if by_rule.is_empty() {
                    continue;
                }

                let total_refs: usize = by_rule.values().map(|v| v.len()).sum();
                let verb_icon = match verb {
                    RefVerb::Define => "◉",
                    RefVerb::Impl => "+",
                    RefVerb::Verify => "✓",
                    RefVerb::Depends => "→",
                    RefVerb::Related => "~",
                };
                let verb_color = match verb {
                    RefVerb::Define => verb.as_str().blue().to_string(),
                    RefVerb::Impl => verb.as_str().green().to_string(),
                    RefVerb::Verify => verb.as_str().cyan().to_string(),
                    RefVerb::Depends => verb.as_str().magenta().to_string(),
                    RefVerb::Related => verb.as_str().dimmed().to_string(),
                };

                println!(
                    "{} {} ({} references across {} rules):",
                    verb_icon.bold(),
                    verb_color,
                    total_refs,
                    by_rule.len()
                );

                let mut rules: Vec<_> = by_rule.keys().collect();
                rules.sort();

                for rule_id in rules {
                    let refs = &by_rule[rule_id];
                    println!("  [{}] ({} refs)", rule_id.green(), refs.len());
                    for r in refs {
                        println!("      {}:{}", r.file.dimmed(), r.line.to_string().dimmed());
                    }
                }
                println!();
            }
        }
    }
}
