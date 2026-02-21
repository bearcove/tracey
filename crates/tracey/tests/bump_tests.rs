//! Integration tests for `tracey pre-commit` and `tracey bump`.
//!
//! Each test creates a real git repository in a temp directory, commits an
//! initial spec file, stages a modification, then exercises the bump logic
//! directly via the library API.

use std::fs;
use std::path::Path;
use std::process::Command;

use tracey::bump::{bump, detect_changed_rules, pre_commit};
use tracey::config::{Config, SpecConfig};

// ============================================================================
// Helpers
// ============================================================================

/// Create and configure a throwaway git repository.
fn git_init(dir: &Path) {
    let run = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git not found");
        assert!(status.success(), "git {args:?} failed");
    };

    run(&["init", "--initial-branch=main"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "Test"]);
}

/// Stage and commit everything in the repo.
fn git_commit_all(dir: &Path, message: &str) {
    let run = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git not found");
        assert!(status.success(), "git {args:?} failed");
    };
    run(&["add", "."]);
    run(&["commit", "-m", message]);
}

/// Stage a single file.
fn git_add(dir: &Path, path: &str) {
    let status = Command::new("git")
        .args(["add", path])
        .current_dir(dir)
        .status()
        .expect("git not found");
    assert!(status.success(), "git add {path} failed");
}

/// Build a minimal `Config` that treats `spec.md` as the sole spec file.
fn simple_config() -> Config {
    Config {
        specs: vec![SpecConfig {
            name: "test".to_string(),
            prefix: None,
            source_url: None,
            include: vec!["spec.md".to_string()],
            impls: vec![],
        }],
    }
}

const INITIAL_SPEC: &str = "\
# Spec

r[auth.login]
Users MUST provide valid credentials to log in.

r[auth.session]
Sessions MUST expire after 24 hours of inactivity.
";

// ============================================================================
// Tests
// ============================================================================

/// When a staged spec has no changes at all, detect_changed_rules returns empty.
#[tokio::test]
async fn test_no_changes_detects_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");

    // Stage the file again with identical content.
    git_add(root, "spec.md");

    let config = simple_config();
    let changes = detect_changed_rules(root, &config).await.unwrap();
    assert!(
        changes.is_empty(),
        "expected no changes, got {}",
        changes.len()
    );
}

/// When rule text changes but the version marker stays the same, it's detected.
#[tokio::test]
async fn test_text_change_without_bump_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");

    // Modify rule text without bumping the version.
    let modified = INITIAL_SPEC.replace(
        "Users MUST provide valid credentials to log in.",
        "Users MUST provide valid credentials and MFA to log in.",
    );
    fs::write(root.join("spec.md"), &modified).unwrap();
    git_add(root, "spec.md");

    let config = simple_config();
    let changes = detect_changed_rules(root, &config).await.unwrap();

    assert_eq!(changes.len(), 1, "expected exactly one changed rule");
    assert_eq!(changes[0].rule_id.base, "auth.login");
    assert_eq!(changes[0].rule_id.version, 1); // still at version 1
}

/// When rule text changes AND the version is bumped, it's not flagged.
#[tokio::test]
async fn test_text_change_with_bump_is_clean() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");

    // Bump the version AND update the text.
    let modified = INITIAL_SPEC
        .replace("r[auth.login]", "r[auth.login+2]")
        .replace(
            "Users MUST provide valid credentials to log in.",
            "Users MUST provide valid credentials and MFA to log in.",
        );
    fs::write(root.join("spec.md"), &modified).unwrap();
    git_add(root, "spec.md");

    let config = simple_config();
    let changes = detect_changed_rules(root, &config).await.unwrap();
    assert!(changes.is_empty(), "bumped rule should not be flagged");
}

/// `bump` rewrites the marker in the staged file and the new marker has version+1.
#[tokio::test]
async fn test_bump_increments_version_in_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");

    // Modify rule text without bumping.
    let modified = INITIAL_SPEC.replace(
        "Users MUST provide valid credentials to log in.",
        "Users MUST provide valid credentials and MFA to log in.",
    );
    fs::write(root.join("spec.md"), &modified).unwrap();
    git_add(root, "spec.md");

    let config = simple_config();
    let bumped = bump(root, &config).await.unwrap();

    assert_eq!(bumped.len(), 1);
    assert_eq!(bumped[0].base, "auth.login");
    assert_eq!(bumped[0].version, 2);

    // The file on disk should now contain the bumped marker.
    let content = fs::read_to_string(root.join("spec.md")).unwrap();
    assert!(
        content.contains("r[auth.login+2]"),
        "expected bumped marker in file, got:\n{content}"
    );
    assert!(
        !content.contains("r[auth.login]"),
        "old unversioned marker should be gone"
    );
}

/// `bump` applied twice (v1→v2→v3) works correctly.
#[tokio::test]
async fn test_bump_from_existing_version() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);

    // Start with spec already at version 2.
    let v2_spec = INITIAL_SPEC.replace("r[auth.login]", "r[auth.login+2]");
    fs::write(root.join("spec.md"), &v2_spec).unwrap();
    git_commit_all(root, "initial at v2");

    // Modify text without bumping.
    let modified = v2_spec.replace(
        "Users MUST provide valid credentials to log in.",
        "Users MUST provide valid credentials, MFA, and passkeys to log in.",
    );
    fs::write(root.join("spec.md"), &modified).unwrap();
    git_add(root, "spec.md");

    let config = simple_config();
    let bumped = bump(root, &config).await.unwrap();

    assert_eq!(bumped.len(), 1);
    assert_eq!(bumped[0].version, 3);

    let content = fs::read_to_string(root.join("spec.md")).unwrap();
    assert!(content.contains("r[auth.login+3]"));
}

/// Multiple rules changed in the same file are all bumped.
#[tokio::test]
async fn test_bump_multiple_rules_in_one_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");

    let modified = INITIAL_SPEC
        .replace(
            "Users MUST provide valid credentials to log in.",
            "Users MUST provide valid credentials and MFA to log in.",
        )
        .replace(
            "Sessions MUST expire after 24 hours of inactivity.",
            "Sessions MUST expire after 8 hours of inactivity.",
        );
    fs::write(root.join("spec.md"), &modified).unwrap();
    git_add(root, "spec.md");

    let config = simple_config();
    let bumped = bump(root, &config).await.unwrap();

    assert_eq!(bumped.len(), 2);
    let bases: Vec<&str> = bumped.iter().map(|r| r.base.as_str()).collect();
    assert!(bases.contains(&"auth.login"), "auth.login should be bumped");
    assert!(
        bases.contains(&"auth.session"),
        "auth.session should be bumped"
    );

    let content = fs::read_to_string(root.join("spec.md")).unwrap();
    assert!(content.contains("r[auth.login+2]"));
    assert!(content.contains("r[auth.session+2]"));
}

/// `pre_commit` returns true (clean) when there are no unbumped changes.
#[tokio::test]
async fn test_pre_commit_passes_when_clean() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");

    // Stage file unchanged.
    git_add(root, "spec.md");

    let config = simple_config();
    let passed = pre_commit(root, &config).await.unwrap();
    assert!(passed, "pre-commit should pass with no changes");
}

/// `pre_commit` returns false when a rule text changed without a version bump.
#[tokio::test]
async fn test_pre_commit_fails_on_unbumped_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");

    let modified = INITIAL_SPEC.replace(
        "Users MUST provide valid credentials to log in.",
        "Users MUST provide valid credentials and a CAPTCHA to log in.",
    );
    fs::write(root.join("spec.md"), &modified).unwrap();
    git_add(root, "spec.md");

    let config = simple_config();
    let passed = pre_commit(root, &config).await.unwrap();
    assert!(
        !passed,
        "pre-commit should fail when rule text changed without bump"
    );
}

/// Non-spec files staged alongside spec changes don't cause false positives.
#[tokio::test]
async fn test_non_spec_staged_files_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    fs::write(root.join("README.md"), "hello").unwrap();
    git_commit_all(root, "initial");

    // Only stage the non-spec file with a change.
    fs::write(root.join("README.md"), "hello world").unwrap();
    git_add(root, "README.md");

    let config = simple_config();
    let changes = detect_changed_rules(root, &config).await.unwrap();
    assert!(
        changes.is_empty(),
        "changes in non-spec files should be ignored"
    );
}

/// After `bump` the file is re-staged, so a subsequent `pre_commit` passes.
#[tokio::test]
async fn test_bump_then_pre_commit_passes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");

    let modified = INITIAL_SPEC.replace(
        "Users MUST provide valid credentials to log in.",
        "Users MUST provide valid credentials and a CAPTCHA to log in.",
    );
    fs::write(root.join("spec.md"), &modified).unwrap();
    git_add(root, "spec.md");

    let config = simple_config();

    // Bump first.
    let bumped = bump(root, &config).await.unwrap();
    assert_eq!(bumped.len(), 1);

    // Now pre-commit should pass.
    let passed = pre_commit(root, &config).await.unwrap();
    assert!(passed, "pre-commit should pass after bump");
}
