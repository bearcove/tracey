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

/// Helper to parse JSON output and get a field as string
fn json_get_str<'a>(obj: &'a facet_value::Value, key: &str) -> &'a str {
    obj.as_object()
        .unwrap()
        .get(key)
        .unwrap()
        .as_string()
        .unwrap()
        .as_str()
}

/// Helper to parse JSON array output
fn parse_json_array(json: &str) -> facet_value::VArray {
    let parsed: facet_value::Value =
        facet_format_json::from_str(json).expect("Should be valid JSON");
    parsed.as_array().expect("Should be an array").clone()
}

// tracey[verify manifest.format.json]
// tracey[verify manifest.format.rules-key]
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

// tracey[verify manifest.format.rule-entry]
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

// tracey[verify markdown.duplicates.same-file]
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
        stderr.contains("No markdown files")
            || stderr.contains("missing_argument")
            || stderr.contains("<files>"),
        "Should fail with error: {}",
        stderr
    );
}

// tracey[verify markdown.html.div]
// tracey[verify markdown.html.anchor]
// tracey[verify markdown.html.link]
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

// tracey[verify markdown.duplicates.cross-file]
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
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!("tracey_at_test_{}_{}", timestamp, id));
    let _ = std::fs::remove_dir_all(&temp_dir); // Clean up any leftovers
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
    let file_path = temp_dir.join("test.rs");
    std::fs::write(&file_path, content).expect("Failed to write test file");
    let cleanup_path = temp_dir.clone();
    (file_path, move || {
        let _ = std::fs::remove_dir_all(cleanup_path);
    })
}

// tracey[verify ref.syntax.brackets]
// tracey[verify ref.syntax.verb]
// tracey[verify ref.verb.impl]
// tracey[verify ref.verb.verify]
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

// tracey[verify ref.span.offset]
// tracey[verify ref.span.file]
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

// tracey[verify ref.span.length]
// tracey[verify ref.syntax.rule-id]
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
    let arr = parse_json_array(&stdout);

    assert_eq!(arr.len(), 1, "Should have one reference");
    assert_eq!(json_get_str(&arr[0], "rule_id"), "test.rule.one");
    assert_eq!(json_get_str(&arr[0], "verb"), "impl");

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

// ============================================================================
// Tests for rule metadata (Issue #10)
// ============================================================================

fn create_temp_md_file(content: &str) -> (std::path::PathBuf, impl FnOnce()) {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!("tracey_md_test_{}_{}", timestamp, id));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
    let file_path = temp_dir.join("test_spec.md");
    std::fs::write(&file_path, content).expect("Failed to write test file");
    let cleanup_path = temp_dir.clone();
    (file_path, move || {
        let _ = std::fs::remove_dir_all(cleanup_path);
    })
}

#[test]
fn test_rules_command_with_metadata() {
    let (file_path, cleanup) = create_temp_md_file(
        r#"# Test Spec

r[test.stable status=stable level=must since=1.0]
This is a stable rule.

r[test.draft status=draft]
This is a draft rule.

r[test.deprecated status=deprecated until=2.0 tags=legacy,migration]
This is deprecated.
"#,
    );

    let output = tracey_bin()
        .arg("rules")
        .arg(&file_path)
        .output()
        .expect("Failed to run tracey");

    assert!(output.status.success(), "Command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Check metadata is present in output
    assert!(
        stdout.contains("\"status\": \"stable\""),
        "Should contain stable status: {}",
        stdout
    );
    assert!(
        stdout.contains("\"level\": \"must\""),
        "Should contain must level: {}",
        stdout
    );
    assert!(
        stdout.contains("\"since\": \"1.0\""),
        "Should contain since version: {}",
        stdout
    );
    assert!(
        stdout.contains("\"status\": \"draft\""),
        "Should contain draft status: {}",
        stdout
    );
    assert!(
        stdout.contains("\"status\": \"deprecated\""),
        "Should contain deprecated status: {}",
        stdout
    );
    assert!(
        stdout.contains("\"until\": \"2.0\""),
        "Should contain until version: {}",
        stdout
    );
    assert!(
        stdout.contains("\"legacy\""),
        "Should contain legacy tag: {}",
        stdout
    );
    assert!(
        stdout.contains("\"migration\""),
        "Should contain migration tag: {}",
        stdout
    );

    cleanup();
}

#[test]
fn test_rules_command_invalid_status() {
    let (file_path, cleanup) = create_temp_md_file(
        r#"# Test Spec

r[test.rule status=invalid]
This has an invalid status.
"#,
    );

    let output = tracey_bin()
        .arg("rules")
        .arg(&file_path)
        .output()
        .expect("Failed to run tracey");

    assert!(
        !output.status.success(),
        "Command should fail with invalid status"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid status"),
        "Should mention invalid status: {}",
        stderr
    );

    cleanup();
}

#[test]
fn test_rules_command_invalid_level() {
    let (file_path, cleanup) = create_temp_md_file(
        r#"# Test Spec

r[test.rule level=invalid]
This has an invalid level.
"#,
    );

    let output = tracey_bin()
        .arg("rules")
        .arg(&file_path)
        .output()
        .expect("Failed to run tracey");

    assert!(
        !output.status.success(),
        "Command should fail with invalid level"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid level"),
        "Should mention invalid level: {}",
        stderr
    );

    cleanup();
}

#[test]
fn test_rules_command_unknown_attribute() {
    let (file_path, cleanup) = create_temp_md_file(
        r#"# Test Spec

r[test.rule unknown=value]
This has an unknown attribute.
"#,
    );

    let output = tracey_bin()
        .arg("rules")
        .arg(&file_path)
        .output()
        .expect("Failed to run tracey");

    assert!(
        !output.status.success(),
        "Command should fail with unknown attribute"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown attribute"),
        "Should mention unknown attribute: {}",
        stderr
    );

    cleanup();
}

// ============================================================================
// Tests for comment types (ref.comments.*)
// ============================================================================

// tracey[verify ref.comments.line]
#[test]
fn test_line_comments() {
    let (file_path, cleanup) = create_test_file(
        r#"
// [impl test.line.comment]
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

    let arr = parse_json_array(&stdout);
    assert_eq!(arr.len(), 1);
    assert_eq!(json_get_str(&arr[0], "rule_id"), "test.line.comment");

    cleanup();
}

// tracey[verify ref.comments.doc]
#[test]
fn test_doc_comments() {
    let (file_path, cleanup) = create_test_file(
        r#"
/// Documentation comment
/// [impl test.doc.comment]
fn foo() {}

//! Module-level doc
//! [impl test.inner.doc]
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

    let arr = parse_json_array(&stdout);
    assert_eq!(arr.len(), 2);

    let rule_ids: Vec<&str> = arr.iter().map(|r| json_get_str(r, "rule_id")).collect();
    assert!(rule_ids.contains(&"test.doc.comment"));
    assert!(rule_ids.contains(&"test.inner.doc"));

    cleanup();
}

// tracey[verify ref.comments.block]
#[test]
fn test_block_comments() {
    let (file_path, cleanup) = create_test_file(
        r#"
/*
 * Block comment
 * [impl test.block.comment]
 */
fn foo() {}

/* Single line block [impl test.single.block] */
fn bar() {}
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

    let arr = parse_json_array(&stdout);
    assert_eq!(arr.len(), 2);

    let rule_ids: Vec<&str> = arr.iter().map(|r| json_get_str(r, "rule_id")).collect();
    assert!(rule_ids.contains(&"test.block.comment"));
    assert!(rule_ids.contains(&"test.single.block"));

    cleanup();
}

// ============================================================================
// Tests for verb types (ref.verb.*)
// ============================================================================

// tracey[verify ref.verb.define]
#[test]
fn test_verb_define() {
    let (file_path, cleanup) = create_test_file(
        r#"
// [define test.definition]
// This defines a rule.
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

    let arr = parse_json_array(&stdout);
    assert_eq!(arr.len(), 1);
    assert_eq!(json_get_str(&arr[0], "rule_id"), "test.definition");
    assert_eq!(json_get_str(&arr[0], "verb"), "define");

    cleanup();
}

// tracey[verify ref.verb.depends]
#[test]
fn test_verb_depends() {
    let (file_path, cleanup) = create_test_file(
        r#"
// [depends test.dependency]
fn needs_other_rule() {}
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

    let arr = parse_json_array(&stdout);
    assert_eq!(arr.len(), 1);
    assert_eq!(json_get_str(&arr[0], "rule_id"), "test.dependency");
    assert_eq!(json_get_str(&arr[0], "verb"), "depends");

    cleanup();
}

// tracey[verify ref.verb.related]
#[test]
fn test_verb_related() {
    let (file_path, cleanup) = create_test_file(
        r#"
// [related test.related.rule]
fn related_code() {}
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

    let arr = parse_json_array(&stdout);
    assert_eq!(arr.len(), 1);
    assert_eq!(json_get_str(&arr[0], "rule_id"), "test.related.rule");
    assert_eq!(json_get_str(&arr[0], "verb"), "related");

    cleanup();
}

// tracey[verify ref.verb.default]
#[test]
fn test_verb_default() {
    let (file_path, cleanup) = create_test_file(
        r#"
// [test.no.verb]
fn default_is_impl() {}
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

    let arr = parse_json_array(&stdout);
    assert_eq!(arr.len(), 1);
    assert_eq!(json_get_str(&arr[0], "rule_id"), "test.no.verb");
    assert_eq!(json_get_str(&arr[0], "verb"), "impl"); // Default verb is impl

    cleanup();
}

// tracey[verify ref.verb.unknown]
#[test]
fn test_verb_unknown_warning() {
    let (file_path, cleanup) = create_test_file(
        r#"
// [unknownverb test.rule.id]
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

    // Unknown verb means no valid reference found
    let arr = parse_json_array(&stdout);
    assert_eq!(arr.len(), 0, "Unknown verb should not create a reference");

    cleanup();
}

// ============================================================================
// Tests for config (config.*)
// ============================================================================

fn create_temp_project() -> (std::path::PathBuf, impl FnOnce()) {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!("tracey_config_test_{}_{}", timestamp, id));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let cleanup_path = temp_dir.clone();
    (temp_dir, move || {
        let _ = std::fs::remove_dir_all(cleanup_path);
    })
}

// tracey[verify config.format.kdl]
// tracey[verify config.spec.name]
// tracey[verify config.spec.source]
#[test]
fn test_config_kdl_format() {
    let (temp_dir, cleanup) = create_temp_project();

    // Create a .config/tracey/config.kdl
    let config_dir = temp_dir.join(".config/tracey");
    std::fs::create_dir_all(&config_dir).unwrap();

    // Create a spec file
    let spec_dir = temp_dir.join("docs/spec");
    std::fs::create_dir_all(&spec_dir).unwrap();
    std::fs::write(
        spec_dir.join("test.md"),
        r#"
# Test Spec

r[test.rule.one]
First rule.
"#,
    )
    .unwrap();

    // Create a source file with reference
    std::fs::write(
        temp_dir.join("lib.rs"),
        r#"
// [impl test.rule.one]
fn foo() {}
"#,
    )
    .unwrap();

    // Write config in KDL format
    std::fs::write(
        config_dir.join("config.kdl"),
        r#"
spec {
    name "test-spec"
    rules_glob "docs/spec/**/*.md"
    include "**/*.rs"
}
"#,
    )
    .unwrap();

    // Run tracey matrix in the temp directory
    let output = tracey_bin()
        .current_dir(&temp_dir)
        .arg("matrix")
        .output()
        .expect("Failed to run tracey");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Command should succeed. stderr: {}",
        stderr
    );
    assert!(
        stdout.contains("test.rule.one"),
        "Should find rule in output: {}",
        stdout
    );

    cleanup();
}

// tracey[verify config.path.default]
#[test]
fn test_config_default_path() {
    let (temp_dir, cleanup) = create_temp_project();

    // Create a spec file (no config)
    let spec_dir = temp_dir.join("docs/spec");
    std::fs::create_dir_all(&spec_dir).unwrap();
    std::fs::write(
        spec_dir.join("test.md"),
        r#"
r[test.rule]
A rule.
"#,
    )
    .unwrap();

    // Run tracey without config - should fail looking for default path
    let output = tracey_bin()
        .current_dir(&temp_dir)
        .arg("matrix")
        .output()
        .expect("Failed to run tracey");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should fail because config file not found at default path
    assert!(!output.status.success(), "Should fail without config");
    assert!(
        stderr.contains(".config/tracey/config.kdl") || stderr.contains("Config file not found"),
        "Should mention default config path: {}",
        stderr
    );

    cleanup();
}

// tracey[verify config.spec.include]
// tracey[verify config.spec.exclude]
#[test]
fn test_config_include_exclude() {
    let (temp_dir, cleanup) = create_temp_project();

    // Create config
    let config_dir = temp_dir.join(".config/tracey");
    std::fs::create_dir_all(&config_dir).unwrap();

    // Create spec
    let spec_dir = temp_dir.join("docs/spec");
    std::fs::create_dir_all(&spec_dir).unwrap();
    std::fs::write(
        spec_dir.join("test.md"),
        r#"
r[test.rule]
A rule.
"#,
    )
    .unwrap();

    // Create source files
    let src_dir = temp_dir.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("lib.rs"), "// [impl test.rule]\n").unwrap();

    let excluded_dir = temp_dir.join("vendor");
    std::fs::create_dir_all(&excluded_dir).unwrap();
    std::fs::write(excluded_dir.join("external.rs"), "// [impl test.rule]\n").unwrap();

    // Config with include and exclude patterns
    std::fs::write(
        config_dir.join("config.kdl"),
        r#"
spec {
    name "test"
    rules_glob "docs/spec/**/*.md"
    include "src/**/*.rs"
    exclude "vendor/**"
}
"#,
    )
    .unwrap();

    let output = tracey_bin()
        .current_dir(&temp_dir)
        .arg("matrix")
        .output()
        .expect("Failed to run tracey");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "Should succeed: {}", stderr);
    // Should find the impl in src but not in vendor
    assert!(
        stdout.contains("src/lib.rs"),
        "Should find src file: {}",
        stdout
    );
    assert!(
        !stdout.contains("vendor"),
        "Should not include vendor: {}",
        stdout
    );

    cleanup();
}

// ============================================================================
// Tests for coverage computation (coverage.compute.*)
// ============================================================================

// tracey[verify coverage.compute.covered]
// tracey[verify coverage.compute.uncovered]
// tracey[verify coverage.compute.percentage]
#[test]
fn test_coverage_computation() {
    let (temp_dir, cleanup) = create_temp_project();

    // Create config
    let config_dir = temp_dir.join(".config/tracey");
    std::fs::create_dir_all(&config_dir).unwrap();

    // Create spec with 2 rules
    let spec_dir = temp_dir.join("docs/spec");
    std::fs::create_dir_all(&spec_dir).unwrap();
    std::fs::write(
        spec_dir.join("test.md"),
        r#"
r[test.covered]
This rule is covered.

r[test.uncovered]
This rule is NOT covered.
"#,
    )
    .unwrap();

    // Create source that covers only one rule
    std::fs::write(
        temp_dir.join("lib.rs"),
        r#"
// [impl test.covered]
fn covered_impl() {}
"#,
    )
    .unwrap();

    std::fs::write(
        config_dir.join("config.kdl"),
        r#"
spec {
    name "test"
    rules_glob "docs/spec/**/*.md"
    include "**/*.rs"
}
"#,
    )
    .unwrap();

    // Run with --uncovered to see uncovered rules
    let output = tracey_bin()
        .current_dir(&temp_dir)
        .arg("matrix")
        .arg("--uncovered")
        .output()
        .expect("Failed to run tracey");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "Should succeed: {}", stderr);
    // Only uncovered rules should be shown
    assert!(
        stdout.contains("test.uncovered"),
        "Should show uncovered rule: {}",
        stdout
    );
    assert!(
        !stdout.contains("test.covered"),
        "Should not show covered rule in --uncovered mode: {}",
        stdout
    );

    cleanup();
}

// tracey[verify coverage.compute.invalid]
#[test]
fn test_coverage_invalid_references() {
    let (temp_dir, cleanup) = create_temp_project();

    // Create config
    let config_dir = temp_dir.join(".config/tracey");
    std::fs::create_dir_all(&config_dir).unwrap();

    // Create spec with one rule
    let spec_dir = temp_dir.join("docs/spec");
    std::fs::create_dir_all(&spec_dir).unwrap();
    std::fs::write(
        spec_dir.join("test.md"),
        r#"
r[test.valid]
A valid rule.
"#,
    )
    .unwrap();

    // Create source with reference to non-existent rule
    std::fs::write(
        temp_dir.join("lib.rs"),
        r#"
// [impl test.nonexistent]
fn invalid_ref() {}
"#,
    )
    .unwrap();

    std::fs::write(
        config_dir.join("config.kdl"),
        r#"
spec {
    name "test"
    rules_glob "docs/spec/**/*.md"
    include "**/*.rs"
}
"#,
    )
    .unwrap();

    // Run tracey with verbose mode to see invalid references
    let output = tracey_bin()
        .current_dir(&temp_dir)
        .arg("--verbose")
        .output()
        .expect("Failed to run tracey");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    // Should warn about invalid reference (could be in stdout or stderr)
    assert!(
        combined.contains("test.nonexistent")
            || combined.contains("invalid")
            || combined.contains("Invalid"),
        "Should report invalid reference. stdout: {}, stderr: {}",
        stdout,
        stderr
    );

    cleanup();
}

// ============================================================================
// Tests for file walking (walk.*)
// ============================================================================

// tracey[verify walk.gitignore]
#[test]
fn test_walk_respects_gitignore() {
    let (temp_dir, cleanup) = create_temp_project();

    // Initialize git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&temp_dir)
        .output()
        .ok();

    // Create .gitignore
    std::fs::write(temp_dir.join(".gitignore"), "ignored/\n").unwrap();

    // Create config
    let config_dir = temp_dir.join(".config/tracey");
    std::fs::create_dir_all(&config_dir).unwrap();

    // Create spec
    let spec_dir = temp_dir.join("docs/spec");
    std::fs::create_dir_all(&spec_dir).unwrap();
    std::fs::write(
        spec_dir.join("test.md"),
        r#"
r[test.rule]
A rule.
"#,
    )
    .unwrap();

    // Create source files - one in normal location, one in gitignored location
    std::fs::write(temp_dir.join("lib.rs"), "// [impl test.rule]\n").unwrap();

    let ignored_dir = temp_dir.join("ignored");
    std::fs::create_dir_all(&ignored_dir).unwrap();
    std::fs::write(ignored_dir.join("file.rs"), "// [impl test.rule]\n").unwrap();

    std::fs::write(
        config_dir.join("config.kdl"),
        r#"
spec {
    name "test"
    rules_glob "docs/spec/**/*.md"
    include "**/*.rs"
}
"#,
    )
    .unwrap();

    let output = tracey_bin()
        .current_dir(&temp_dir)
        .arg("matrix")
        .output()
        .expect("Failed to run tracey");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "Should succeed: {}", stderr);
    // Should find lib.rs but not ignored/file.rs
    assert!(stdout.contains("lib.rs"), "Should find lib.rs: {}", stdout);
    assert!(
        !stdout.contains("ignored"),
        "Should not include gitignored files: {}",
        stdout
    );

    cleanup();
}

// tracey[verify walk.default-include]
// tracey[verify walk.default-exclude]
#[test]
fn test_walk_default_patterns() {
    let (temp_dir, cleanup) = create_temp_project();

    // Create config without include/exclude
    let config_dir = temp_dir.join(".config/tracey");
    std::fs::create_dir_all(&config_dir).unwrap();

    // Create spec
    let spec_dir = temp_dir.join("docs/spec");
    std::fs::create_dir_all(&spec_dir).unwrap();
    std::fs::write(
        spec_dir.join("test.md"),
        r#"
r[test.rule]
A rule.
"#,
    )
    .unwrap();

    // Create source file
    std::fs::write(temp_dir.join("lib.rs"), "// [impl test.rule]\n").unwrap();

    // Create target directory (should be excluded by default)
    let target_dir = temp_dir.join("target/debug");
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(target_dir.join("build.rs"), "// [impl test.rule]\n").unwrap();

    // Config without include/exclude - should use defaults
    std::fs::write(
        config_dir.join("config.kdl"),
        r#"
spec {
    name "test"
    rules_glob "docs/spec/**/*.md"
}
"#,
    )
    .unwrap();

    let output = tracey_bin()
        .current_dir(&temp_dir)
        .arg("matrix")
        .output()
        .expect("Failed to run tracey");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "Should succeed: {}", stderr);
    // Should find lib.rs (default includes **/*.rs)
    assert!(
        stdout.contains("lib.rs"),
        "Should find lib.rs with default include: {}",
        stdout
    );
    // Should not include target (default exclude)
    assert!(
        !stdout.contains("target"),
        "Should exclude target by default: {}",
        stdout
    );

    cleanup();
}
