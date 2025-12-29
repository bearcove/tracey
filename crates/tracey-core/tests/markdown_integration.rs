//! Integration tests for markdown processing

use std::path::Path;
use tracey_core::markdown::{MarkdownProcessor, RulesManifest};

const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(FIXTURES_DIR).join(name)
}

fn read_fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name))
        .unwrap_or_else(|e| panic!("Failed to read fixture {}: {}", name, e))
}

#[test]
fn test_process_sample_spec() {
    let markdown = read_fixture("sample_spec.md");
    let result = MarkdownProcessor::process(&markdown).expect("Failed to process markdown");

    // Should find all 8 rules
    assert_eq!(result.rules.len(), 8, "Expected 8 rules in sample_spec.md");

    // Verify specific rules exist
    let rule_ids: Vec<&str> = result.rules.iter().map(|r| r.id.as_str()).collect();
    assert!(rule_ids.contains(&"channel.id.allocation"));
    assert!(rule_ids.contains(&"channel.id.parity"));
    assert!(rule_ids.contains(&"channel.lifecycle.open"));
    assert!(rule_ids.contains(&"channel.lifecycle.close"));
    assert!(rule_ids.contains(&"error.codes.range"));
    assert!(rule_ids.contains(&"error.propagation"));
    assert!(rule_ids.contains(&"perf.latency.p99"));
    assert!(rule_ids.contains(&"perf.throughput.minimum"));

    // Verify anchor IDs are generated correctly
    for rule in &result.rules {
        assert!(
            rule.anchor_id.starts_with("r-"),
            "Anchor ID should start with 'r-'"
        );
        assert_eq!(
            rule.anchor_id,
            format!("r-{}", rule.id),
            "Anchor ID should be 'r-' + rule ID"
        );
    }

    // Output should contain HTML divs for each rule
    assert!(result.output.contains("class=\"rule\""));
    assert!(result.output.contains("id=\"r-channel.id.allocation\""));
}

#[test]
fn test_duplicate_rules_error() {
    let markdown = read_fixture("duplicate_rules.md");
    let result = MarkdownProcessor::process(&markdown);

    assert!(result.is_err(), "Should fail on duplicate rules");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("duplicate rule identifier"),
        "Error message should mention duplicate: {}",
        err
    );
    assert!(
        err.contains("duplicate.rule"),
        "Error message should contain the rule ID: {}",
        err
    );
}

#[test]
fn test_generate_manifest_json() {
    let markdown = read_fixture("sample_spec.md");
    let result = MarkdownProcessor::process(&markdown).expect("Failed to process markdown");

    let manifest = RulesManifest::from_rules(&result.rules, "/spec/sample");
    let json = manifest.to_json();

    // Parse the JSON to verify it's valid
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Invalid JSON output");

    // Verify structure
    assert!(parsed.is_object());
    assert!(parsed.get("rules").is_some());
    let rules = parsed.get("rules").unwrap().as_object().unwrap();

    // Should have all 8 rules
    assert_eq!(rules.len(), 8);

    // Verify URL format
    let channel_rule = rules.get("channel.id.allocation").unwrap();
    let url = channel_rule.get("url").unwrap().as_str().unwrap();
    assert_eq!(url, "/spec/sample#r-channel.id.allocation");
}

#[test]
fn test_manifest_merge_detects_duplicates() {
    let markdown1 = r#"
r[shared.rule]
First definition.

r[unique.rule1]
Only in first.
"#;

    let markdown2 = r#"
r[shared.rule]
Duplicate definition.

r[unique.rule2]
Only in second.
"#;

    let result1 = MarkdownProcessor::process(markdown1).unwrap();
    let result2 = MarkdownProcessor::process(markdown2).unwrap();

    let mut manifest1 = RulesManifest::from_rules(&result1.rules, "/doc1");
    let manifest2 = RulesManifest::from_rules(&result2.rules, "/doc2");

    let duplicates = manifest1.merge(&manifest2);

    assert_eq!(duplicates.len(), 1);
    assert_eq!(duplicates[0].id, "shared.rule");
    assert_eq!(duplicates[0].first_url, "/doc1#r-shared.rule");
    assert_eq!(duplicates[0].second_url, "/doc2#r-shared.rule");

    // Manifest should still have all unique rules
    assert_eq!(manifest1.rules.len(), 3);
}

#[test]
fn test_extract_rules_only() {
    let markdown = read_fixture("sample_spec.md");
    let rules = MarkdownProcessor::extract_rules(&markdown).expect("Failed to extract rules");

    assert_eq!(rules.len(), 8);
}

#[test]
fn test_empty_document() {
    let result = MarkdownProcessor::process("").expect("Failed to process empty document");
    assert!(result.rules.is_empty());
    assert!(result.output.is_empty() || result.output.trim().is_empty());
}

#[test]
fn test_document_without_rules() {
    let markdown = r#"
# Just a Document

This document has no rules at all.

## Some Section

Just regular markdown content.
"#;

    let result = MarkdownProcessor::process(markdown).expect("Failed to process markdown");
    assert!(result.rules.is_empty());
    // Output should be mostly unchanged (just the content without transformation)
    assert!(result.output.contains("Just a Document"));
}

#[test]
fn test_html_output_structure() {
    let markdown = r#"
r[test.rule]
Rule content here.
"#;

    let result = MarkdownProcessor::process(markdown).unwrap();

    // Verify the HTML structure matches what dodeca expects
    assert!(result.output.contains("<div class=\"rule\""));
    assert!(result.output.contains("id=\"r-test.rule\""));
    assert!(result.output.contains("<a class=\"rule-link\""));
    assert!(result.output.contains("href=\"#r-test.rule\""));
    assert!(result.output.contains("title=\"test.rule\""));
    // Word break opportunities after dots
    assert!(result.output.contains("[test.<wbr>rule]"));
}

#[test]
fn test_rule_span_tracking() {
    let markdown = "r[first.rule]\nContent.\n\nr[second.rule]\nMore content.\n";

    let result = MarkdownProcessor::process(markdown).unwrap();

    assert_eq!(result.rules.len(), 2);

    // First rule should start at offset 0
    assert_eq!(result.rules[0].span.offset, 0);

    // Second rule should be after the first rule and content
    assert!(result.rules[1].span.offset > result.rules[0].span.offset);
}
