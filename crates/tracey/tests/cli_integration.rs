//! Integration tests that run the tracey binary

use std::path::Path;
use std::process::Command;

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

    // File should contain valid JSON with rules
    let content = std::fs::read_to_string(&output_file).expect("Failed to read output file");
    assert!(content.contains("\"rules\""), "Should have rules key");
    assert!(
        content.contains("\"channel.id.allocation\""),
        "Should contain rule IDs"
    );

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

    assert!(
        !output.status.success(),
        "Command should fail without files"
    );

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

// ============================================================================
// Tests for `tracey at` command
// ============================================================================

fn create_test_file(content: &str) -> (std::path::PathBuf, impl FnOnce()) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = std::env::temp_dir().join(format!("tracey_at_test_{}", id));
    let _ = std::fs::remove_dir_all(&temp_dir); // Clean up any leftovers
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
    let file_path = temp_dir.join("test.rs");
    std::fs::write(&file_path, content).expect("Failed to write test file");
    let cleanup_path = temp_dir.clone();
    (file_path, move || {
        let _ = std::fs::remove_dir_all(cleanup_path);
    })
}

#[test]
fn test_at_command_file() {
    let (file_path, cleanup) = create_test_file(
        r#"
// [impl test.rule.one]
fn foo() {}

// [verify test.rule.two]
fn bar() {}
"#,
    );

    let output = tracey_bin()
        .arg("at")
        .arg(&file_path)
        .output()
        .expect("Failed to run tracey");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Command should succeed: stdout={}, stderr={}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("test.rule.one"),
        "Should find test.rule.one: {}",
        stdout
    );
    assert!(
        stdout.contains("test.rule.two"),
        "Should find test.rule.two: {}",
        stdout
    );

    cleanup();
}

#[test]
fn test_at_command_with_line() {
    let (file_path, cleanup) = create_test_file(
        r#"// line 1
// [impl test.rule.one]
fn foo() {}

// [verify test.rule.two]
fn bar() {}
"#,
    );

    // Query specific line 2 where test.rule.one is
    let location = format!("{}:2", file_path.display());
    let output = tracey_bin()
        .arg("at")
        .arg(&location)
        .output()
        .expect("Failed to run tracey");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Command should succeed");
    assert!(
        stdout.contains("test.rule.one"),
        "Should find test.rule.one at line 2: {}",
        stdout
    );
    assert!(
        !stdout.contains("test.rule.two"),
        "Should NOT find test.rule.two at line 2: {}",
        stdout
    );

    cleanup();
}

#[test]
fn test_at_command_with_line_range() {
    let (file_path, cleanup) = create_test_file(
        r#"// line 1
// [impl test.rule.one]
// [impl test.rule.two]
fn foo() {}

// [verify test.rule.three]
fn bar() {}
"#,
    );

    // Query lines 2-3
    let location = format!("{}:2-3", file_path.display());
    let output = tracey_bin()
        .arg("at")
        .arg(&location)
        .output()
        .expect("Failed to run tracey");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Command should succeed");
    assert!(
        stdout.contains("test.rule.one"),
        "Should find test.rule.one: {}",
        stdout
    );
    assert!(
        stdout.contains("test.rule.two"),
        "Should find test.rule.two: {}",
        stdout
    );
    assert!(
        !stdout.contains("test.rule.three"),
        "Should NOT find test.rule.three: {}",
        stdout
    );

    cleanup();
}

#[test]
fn test_at_command_json_output() {
    let (file_path, cleanup) = create_test_file(
        r#"
// [impl test.rule.one]
fn foo() {}
"#,
    );

    let output = tracey_bin()
        .arg("at")
        .arg(&file_path)
        .arg("-f")
        .arg("json")
        .output()
        .expect("Failed to run tracey");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Command should succeed");

    // Should be valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");
    assert!(parsed.is_array(), "Should be an array");

    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 1, "Should have one reference");
    assert_eq!(arr[0]["rule_id"], "test.rule.one");
    assert_eq!(arr[0]["verb"], "impl");

    cleanup();
}

#[test]
fn test_at_command_no_refs() {
    let (file_path, cleanup) = create_test_file(
        r#"
// Just a regular comment
fn foo() {}
"#,
    );

    let output = tracey_bin()
        .arg("at")
        .arg(&file_path)
        .output()
        .expect("Failed to run tracey");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "Command should succeed");
    assert!(
        stdout.contains("No rule references found"),
        "Should indicate no refs: {}",
        stdout
    );

    cleanup();
}

#[test]
fn test_at_command_file_not_found() {
    let output = tracey_bin()
        .arg("at")
        .arg("/nonexistent/path/to/file.rs")
        .output()
        .expect("Failed to run tracey");

    assert!(
        !output.status.success(),
        "Command should fail for nonexistent file"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found")
            || stderr.contains("Not found")
            || stderr.contains("File not found"),
        "Should mention file not found: {}",
        stderr
    );
}
