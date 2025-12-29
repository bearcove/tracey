//! Integration tests that run the tracey binary

use std::process::Command;
use std::path::Path;

fn tracey_bin() -> Command {
    // Use cargo to find the binary
    Command::new(env!("CARGO_BIN_EXE_tracey"))
}

fn fixtures_dir() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tracey-core/tests/fixtures"
    ))
}

#[test]
fn test_rules_command_basic() {
    let output = tracey_bin()
        .arg("rules")
        .arg(fixtures_dir().join("sample_spec.md"))
        .output()
        .expect("Failed to run tracey");

    assert!(output.status.success(), "Command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should output JSON to stdout
    assert!(stdout.contains("\"rules\""), "Should output rules JSON");
    assert!(
        stdout.contains("channel.id.allocation"),
        "Should contain channel.id.allocation rule"
    );

    // Should log progress to stderr (note: output contains ANSI codes)
    assert!(
        stderr.contains("Processing"),
        "Should log processing: {}",
        stderr
    );
    assert!(
        stderr.contains("8") && stderr.contains("rules"),
        "Should find 8 rules: {}",
        stderr
    );
}

#[test]
fn test_rules_command_with_base_url() {
    let output = tracey_bin()
        .arg("rules")
        .arg("-b")
        .arg("/spec/test")
        .arg(fixtures_dir().join("sample_spec.md"))
        .output()
        .expect("Failed to run tracey");

    assert!(output.status.success(), "Command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // URLs should include the base URL
    assert!(
        stdout.contains("/spec/test#r-channel.id.allocation"),
        "Should include base URL in rule URLs: {}",
        stdout
    );
}

#[test]
fn test_rules_command_output_file() {
    let temp_dir = std::env::temp_dir();
    let output_file = temp_dir.join("tracey_test_rules.json");

    // Clean up from previous runs
    let _ = std::fs::remove_file(&output_file);

    let output = tracey_bin()
        .arg("rules")
        .arg("-o")
        .arg(&output_file)
        .arg(fixtures_dir().join("sample_spec.md"))
        .output()
        .expect("Failed to run tracey");

    assert!(output.status.success(), "Command should succeed");

    // File should be created
    assert!(output_file.exists(), "Output file should be created");

    // File should contain valid JSON
    let content = std::fs::read_to_string(&output_file).expect("Failed to read output file");
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("Output should be valid JSON");

    assert!(json.get("rules").is_some(), "Should have rules key");

    // Clean up
    let _ = std::fs::remove_file(&output_file);
}

#[test]
fn test_rules_command_duplicate_detection() {
    let output = tracey_bin()
        .arg("rules")
        .arg(fixtures_dir().join("duplicate_rules.md"))
        .output()
        .expect("Failed to run tracey");

    assert!(
        !output.status.success(),
        "Command should fail on duplicate rules"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("duplicate"),
        "Should mention duplicate: {}",
        stderr
    );
}

#[test]
fn test_rules_command_no_files() {
    let output = tracey_bin()
        .arg("rules")
        .output()
        .expect("Failed to run tracey");

    assert!(!output.status.success(), "Command should fail without files");

    // Error can be either from argument parsing or explicit check
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No markdown files") || stderr.contains("Error"),
        "Should fail with error: {}",
        stderr
    );
}

#[test]
fn test_rules_command_markdown_output() {
    let temp_dir = std::env::temp_dir().join("tracey_md_test");

    // Clean up from previous runs
    let _ = std::fs::remove_dir_all(&temp_dir);

    let output = tracey_bin()
        .arg("rules")
        .arg("--markdown-out")
        .arg(&temp_dir)
        .arg(fixtures_dir().join("sample_spec.md"))
        .output()
        .expect("Failed to run tracey");

    assert!(output.status.success(), "Command should succeed");

    // Directory should be created with transformed markdown
    let md_file = temp_dir.join("sample_spec.md");
    assert!(md_file.exists(), "Markdown output file should be created");

    let content = std::fs::read_to_string(&md_file).expect("Failed to read markdown output");

    // Should contain the transformed HTML divs
    assert!(
        content.contains("<div class=\"rule\""),
        "Should contain rule divs"
    );
    assert!(
        content.contains("id=\"r-channel.id.allocation\""),
        "Should contain rule anchors"
    );

    // Should NOT contain the original r[...] syntax
    assert!(
        !content.contains("r[channel.id.allocation]"),
        "Should not contain original rule syntax"
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_rules_command_multiple_files() {
    let output = tracey_bin()
        .arg("rules")
        .arg(fixtures_dir().join("sample_spec.md"))
        .arg(fixtures_dir().join("sample_spec.md")) // Same file twice should work (same rules)
        .output()
        .expect("Failed to run tracey");

    // This should fail because of duplicates across files
    assert!(
        !output.status.success(),
        "Should fail when same rules appear in multiple files"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("duplicate") || stderr.contains("Duplicate"),
        "Should mention duplicate: {}",
        stderr
    );
}
