//! Output formatting for coverage reports

use owo_colors::OwoColorize;
use serde::Serialize;
use tracey_core::{CoverageReport, RefVerb};

/// Output format
#[derive(Debug, Clone, Copy, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Markdown,
    Html,
}

impl OutputFormat {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            "markdown" | "md" => Some(Self::Markdown),
            "html" => Some(Self::Html),
            _ => None,
        }
    }
}

/// Render a coverage report in the specified format
pub fn render_report(report: &CoverageReport, format: OutputFormat, verbose: bool) -> String {
    match format {
        OutputFormat::Text => render_text(report, verbose),
        OutputFormat::Json => render_json(report),
        OutputFormat::Markdown => render_markdown(report, verbose),
        OutputFormat::Html => render_html(report, verbose),
    }
}

fn render_text(report: &CoverageReport, verbose: bool) -> String {
    let mut output = String::new();

    output.push('\n');
    output.push_str(&format!(
        "{} {} Coverage Report\n",
        "##".bold(),
        report.spec_name.cyan().bold()
    ));
    output.push('\n');

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

    output.push_str(&format!(
        "Coverage: {} ({}/{} rules)\n",
        color_percent,
        report.covered_rules.len(),
        report.total_rules
    ));

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
        output.push_str(&format!("  References: {}\n", breakdown.join(", ").dimmed()));
    }
    output.push('\n');

    // Invalid references (errors)
    if !report.invalid_references.is_empty() {
        output.push_str(&format!(
            "{} Invalid References ({}):\n",
            "!".red().bold(),
            report.invalid_references.len()
        ));
        for r in &report.invalid_references {
            output.push_str(&format!(
                "  {} {}:{} - unknown rule [{} {}]\n",
                "-".red(),
                r.file.display(),
                r.line,
                r.verb.as_str().dimmed(),
                r.rule_id.yellow()
            ));
        }
        output.push('\n');
    }

    // Uncovered rules
    if !report.uncovered_rules.is_empty() {
        output.push_str(&format!(
            "{} Uncovered Rules ({}):\n",
            "?".yellow().bold(),
            report.uncovered_rules.len()
        ));

        let mut uncovered: Vec<_> = report.uncovered_rules.iter().collect();
        uncovered.sort();

        for rule_id in uncovered {
            output.push_str(&format!("  {} [{}]\n", "-".yellow(), rule_id.dimmed()));
        }
        output.push('\n');
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

                output.push_str(&format!(
                    "{} {} ({} references across {} rules):\n",
                    verb_icon.bold(),
                    verb_color,
                    total_refs,
                    by_rule.len()
                ));

                let mut rules: Vec<_> = by_rule.keys().collect();
                rules.sort();

                for rule_id in rules {
                    let refs = &by_rule[rule_id];
                    output.push_str(&format!("  [{}] ({} refs)\n", rule_id.green(), refs.len()));
                    for r in refs {
                        output.push_str(&format!(
                            "      {}:{}\n",
                            r.file.display().to_string().dimmed(),
                            r.line.to_string().dimmed()
                        ));
                    }
                }
                output.push('\n');
            }
        }
    }

    output
}

#[derive(Serialize)]
struct JsonReport<'a> {
    spec_name: &'a str,
    total_rules: usize,
    covered_rules: usize,
    uncovered_rules: Vec<&'a String>,
    coverage_percent: f64,
    invalid_references: Vec<JsonReference<'a>>,
    references: Vec<JsonReference<'a>>,
}

#[derive(Serialize)]
struct JsonReference<'a> {
    verb: &'a str,
    rule_id: &'a str,
    file: String,
    line: usize,
}

fn render_json(report: &CoverageReport) -> String {
    let mut uncovered: Vec<_> = report.uncovered_rules.iter().collect();
    uncovered.sort();

    let json_report = JsonReport {
        spec_name: &report.spec_name,
        total_rules: report.total_rules,
        covered_rules: report.covered_rules.len(),
        uncovered_rules: uncovered,
        coverage_percent: report.coverage_percent(),
        invalid_references: report
            .invalid_references
            .iter()
            .map(|r| JsonReference {
                verb: r.verb.as_str(),
                rule_id: &r.rule_id,
                file: r.file.display().to_string(),
                line: r.line,
            })
            .collect(),
        references: report
            .references_by_rule
            .values()
            .flatten()
            .map(|r| JsonReference {
                verb: r.verb.as_str(),
                rule_id: &r.rule_id,
                file: r.file.display().to_string(),
                line: r.line,
            })
            .collect(),
    };

    serde_json::to_string_pretty(&json_report).unwrap_or_else(|_| "{}".to_string())
}

fn render_markdown(report: &CoverageReport, verbose: bool) -> String {
    let mut output = String::new();

    output.push_str(&format!("# {} Coverage Report\n\n", report.spec_name));

    let percent = report.coverage_percent();
    output.push_str(&format!(
        "**Coverage:** {:.1}% ({}/{} rules)\n\n",
        percent,
        report.covered_rules.len(),
        report.total_rules
    ));

    // Invalid references
    if !report.invalid_references.is_empty() {
        output.push_str("## Invalid References\n\n");
        for r in &report.invalid_references {
            output.push_str(&format!(
                "- `{}:{}` - unknown rule `[{} {}]`\n",
                r.file.display(),
                r.line,
                r.verb.as_str(),
                r.rule_id
            ));
        }
        output.push('\n');
    }

    // Uncovered rules
    if !report.uncovered_rules.is_empty() {
        output.push_str("## Uncovered Rules\n\n");
        let mut uncovered: Vec<_> = report.uncovered_rules.iter().collect();
        uncovered.sort();
        for rule_id in uncovered {
            output.push_str(&format!("- `{}`\n", rule_id));
        }
        output.push('\n');
    }

    // Verbose: covered rules
    if verbose && !report.covered_rules.is_empty() {
        output.push_str("## Covered Rules\n\n");
        let mut covered: Vec<_> = report.covered_rules.iter().collect();
        covered.sort();
        for rule_id in covered {
            let refs = report.references_by_rule.get(rule_id);
            let count = refs.map(|r| r.len()).unwrap_or(0);
            output.push_str(&format!("- `{}` ({} references)\n", rule_id, count));
        }
        output.push('\n');
    }

    output
}

fn render_html(report: &CoverageReport, verbose: bool) -> String {
    let mut output = String::new();

    output.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    output.push_str("<meta charset=\"utf-8\">\n");
    output.push_str(&format!(
        "<title>{} Coverage Report</title>\n",
        report.spec_name
    ));
    output.push_str("<style>\n");
    output.push_str("body { font-family: system-ui, sans-serif; max-width: 800px; margin: 2rem auto; padding: 0 1rem; }\n");
    output.push_str(".good { color: green; }\n");
    output.push_str(".warn { color: orange; }\n");
    output.push_str(".bad { color: red; }\n");
    output.push_str("code { background: #f0f0f0; padding: 0.2em 0.4em; border-radius: 3px; }\n");
    output.push_str("</style>\n");
    output.push_str("</head>\n<body>\n");

    output.push_str(&format!("<h1>{} Coverage Report</h1>\n", report.spec_name));

    let percent = report.coverage_percent();
    let class = if percent >= 80.0 {
        "good"
    } else if percent >= 50.0 {
        "warn"
    } else {
        "bad"
    };
    output.push_str(&format!(
        "<p><strong>Coverage:</strong> <span class=\"{}\">{:.1}%</span> ({}/{} rules)</p>\n",
        class,
        percent,
        report.covered_rules.len(),
        report.total_rules
    ));

    // Invalid references
    if !report.invalid_references.is_empty() {
        output.push_str("<h2>Invalid References</h2>\n<ul>\n");
        for r in &report.invalid_references {
            output.push_str(&format!(
                "<li><code>{}:{}</code> - unknown rule <code>[{} {}]</code></li>\n",
                r.file.display(),
                r.line,
                r.verb.as_str(),
                r.rule_id
            ));
        }
        output.push_str("</ul>\n");
    }

    // Uncovered rules
    if !report.uncovered_rules.is_empty() {
        output.push_str("<h2>Uncovered Rules</h2>\n<ul>\n");
        let mut uncovered: Vec<_> = report.uncovered_rules.iter().collect();
        uncovered.sort();
        for rule_id in uncovered {
            output.push_str(&format!("<li><code>{}</code></li>\n", rule_id));
        }
        output.push_str("</ul>\n");
    }

    // Verbose: covered rules
    if verbose && !report.covered_rules.is_empty() {
        output.push_str("<h2>Covered Rules</h2>\n<ul>\n");
        let mut covered: Vec<_> = report.covered_rules.iter().collect();
        covered.sort();
        for rule_id in covered {
            let refs = report.references_by_rule.get(rule_id);
            let count = refs.map(|r| r.len()).unwrap_or(0);
            output.push_str(&format!(
                "<li><code>{}</code> ({} references)</li>\n",
                rule_id, count
            ));
        }
        output.push_str("</ul>\n");
    }

    output.push_str("</body>\n</html>\n");
    output
}
