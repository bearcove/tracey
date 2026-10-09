//! Integration tests for `tracey pre-commit` and `tracey bump`.
//!
//! Each test creates a real git repository in a temp directory, commits an
//! initial spec file, stages a modification, then exercises the bump logic
//! directly via the library API.

use std::fs;
use std::path::Path;
use std::process::Command;

use tracey::bump::{CompareTarget, bump, detect_changed_rules, pre_commit};
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
            format: Default::default(),
            impls: vec![],
        }],
    }
}

/// Build a minimal `Config` that treats `spec.typ` as the sole spec file.
fn simple_typst_config() -> Config {
    Config {
        specs: vec![SpecConfig {
            name: "test".to_string(),
            prefix: None,
            source_url: None,
            include: vec!["spec.typ".to_string()],
            format: Default::default(),
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
    let changes = detect_changed_rules(root, &config, None, &CompareTarget::Index)
        .await
        .unwrap();
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
    let changes = detect_changed_rules(root, &config, None, &CompareTarget::Index)
        .await
        .unwrap();

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
    let changes = detect_changed_rules(root, &config, None, &CompareTarget::Index)
        .await
        .unwrap();
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
    let bumped = bump(root, &config, None, false).await.unwrap();

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

/// `bump` works on Typst spec files: `#req("id")[body]` → `#req("id+2")[body]`.
#[tokio::test]
async fn test_bump_typst_spec_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);

    let initial = "= Spec\n\n#req(\"test.rule\")[The system MUST validate input.]\n";
    fs::write(root.join("spec.typ"), initial).unwrap();
    git_commit_all(root, "initial");

    // Modify rule body without bumping the version.
    let modified = initial.replace(
        "The system MUST validate input.",
        "The system MUST validate and sanitise input.",
    );
    fs::write(root.join("spec.typ"), &modified).unwrap();
    git_add(root, "spec.typ");

    let config = simple_typst_config();
    let changes = detect_changed_rules(root, &config, None, &CompareTarget::Index)
        .await
        .unwrap();
    assert_eq!(changes.len(), 1, "expected exactly one changed rule");
    assert_eq!(changes[0].rule_id.base, "test.rule");

    let bumped = bump(root, &config, None, false).await.unwrap();
    assert_eq!(bumped.len(), 1);
    assert_eq!(bumped[0].base, "test.rule");
    assert_eq!(bumped[0].version, 2);

    // The file on disk should now contain the bumped marker.
    let content = fs::read_to_string(root.join("spec.typ")).unwrap();
    assert!(
        content.contains("#req(\"test.rule+2\")"),
        "expected bumped marker in file, got:\n{content}"
    );
    assert!(
        !content.contains("#req(\"test.rule\")["),
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
    let bumped = bump(root, &config, None, false).await.unwrap();

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
    let bumped = bump(root, &config, None, false).await.unwrap();

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
    let passed = pre_commit(root, &config, None, None).await.unwrap();
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
    let passed = pre_commit(root, &config, None, None).await.unwrap();
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
    let changes = detect_changed_rules(root, &config, None, &CompareTarget::Index)
        .await
        .unwrap();
    assert!(
        changes.is_empty(),
        "changes in non-spec files should be ignored"
    );
}

/// `detect_changed_rules` with no config (empty spec list) always returns empty.
#[tokio::test]
async fn test_no_config_is_noop() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");

    let modified = INITIAL_SPEC.replace(
        "Users MUST provide valid credentials to log in.",
        "Users MUST provide valid credentials and MFA to log in.",
    );
    fs::write(root.join("spec.md"), &modified).unwrap();
    git_add(root, "spec.md");

    let empty_config = Config { specs: vec![] };
    let changes = detect_changed_rules(root, &empty_config, None, &CompareTarget::Index)
        .await
        .unwrap();
    assert!(changes.is_empty(), "empty config should produce no changes");
}

/// Staging a spec file that contains no rule markers produces no changes.
#[tokio::test]
async fn test_spec_file_with_no_rules() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), "# Spec\n\nNo rules here.\n").unwrap();
    git_commit_all(root, "initial");

    fs::write(
        root.join("spec.md"),
        "# Spec\n\nStill no rules, but edited.\n",
    )
    .unwrap();
    git_add(root, "spec.md");

    let config = simple_config();
    let changes = detect_changed_rules(root, &config, None, &CompareTarget::Index)
        .await
        .unwrap();
    assert!(
        changes.is_empty(),
        "file with no rules should produce no changes"
    );
}

/// First ever commit (no HEAD): staged spec rules have no prior version to compare against.
#[tokio::test]
async fn test_initial_commit_no_head() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    // Stage the spec WITHOUT committing — no HEAD exists yet.
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_add(root, "spec.md");

    let config = simple_config();
    let changes = detect_changed_rules(root, &config, None, &CompareTarget::Index)
        .await
        .unwrap();
    assert!(
        changes.is_empty(),
        "with no HEAD there is nothing to compare against; expected no changes, got {}",
        changes.len()
    );
}

/// Renaming a spec file does not produce false positives.
///
/// git diff --cached --name-only reports the old name (deleted) and the new
/// name (added). The old name has no staged content (deleted), so it is
/// skipped. The new name has no HEAD content (new file), so it is also
/// treated as having no prior version.
#[tokio::test]
async fn test_renamed_spec_file_not_flagged() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");

    // Simulate a rename via git rm + write + git add.
    let status = Command::new("git")
        .args(["rm", "spec.md"])
        .current_dir(root)
        .status()
        .expect("git not found");
    assert!(status.success(), "git rm failed");

    fs::write(root.join("spec2.md"), INITIAL_SPEC).unwrap();
    git_add(root, "spec2.md");

    // A wildcard config that matches both names.
    let wildcard_config = Config {
        specs: vec![SpecConfig {
            name: "test".to_string(),
            prefix: None,
            source_url: None,
            include: vec!["**/*.md".to_string()],
            format: Default::default(),
            impls: vec![],
        }],
    };
    let changes = detect_changed_rules(root, &wildcard_config, None, &CompareTarget::Index)
        .await
        .unwrap();
    assert!(
        changes.is_empty(),
        "renamed spec file should not be flagged as having unbumped changes"
    );
}

/// Running `bump` twice in a row without staging new changes is a no-op on the
/// second call: the first bump increments the version, so the version check
/// no longer flags the rule.
#[tokio::test]
async fn test_bump_idempotency() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");

    let modified = INITIAL_SPEC.replace(
        "Users MUST provide valid credentials to log in.",
        "Users MUST provide valid credentials and MFA to log in.",
    );
    fs::write(root.join("spec.md"), &modified).unwrap();
    git_add(root, "spec.md");

    let config = simple_config();

    let first = bump(root, &config, None, false).await.unwrap();
    assert_eq!(first.len(), 1, "first bump should fix one rule");

    // Second call: the staged file now has the bumped marker, so version
    // differs from HEAD and the rule is not flagged again.
    let second = bump(root, &config, None, false).await.unwrap();
    assert!(
        second.is_empty(),
        "second bump with no new changes should be a no-op"
    );
}

/// Staged spec content that is not valid UTF-8 produces a clear error.
#[tokio::test]
async fn test_non_utf8_staged_content_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");

    // Append an invalid UTF-8 byte sequence to the staged version.
    let mut content = INITIAL_SPEC.as_bytes().to_vec();
    content.push(0xFF); // lone 0xFF is never valid UTF-8
    fs::write(root.join("spec.md"), &content).unwrap();
    git_add(root, "spec.md");

    let config = simple_config();
    let result = detect_changed_rules(root, &config, None, &CompareTarget::Index).await;
    assert!(
        result.is_err(),
        "non-UTF-8 staged content should return an error"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.to_lowercase().contains("utf-8") || msg.to_lowercase().contains("utf8"),
        "error should mention UTF-8, got: {msg}"
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
    let bumped = bump(root, &config, None, false).await.unwrap();
    assert_eq!(bumped.len(), 1);

    // Now pre-commit should pass.
    let passed = pre_commit(root, &config, None, None).await.unwrap();
    assert!(passed, "pre-commit should pass after bump");
}

// ============================================================================
// --from / --to / --unstaged
// ============================================================================

/// Run a git command and return its stdout.
fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git not found");
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8(out.stdout).expect("git output is not valid UTF-8")
}

/// `INITIAL_SPEC` with the `auth.login` text changed but not bumped.
fn unbumped_login_change() -> String {
    INITIAL_SPEC.replace(
        "Users MUST provide valid credentials to log in.",
        "Users MUST provide valid credentials and MFA to log in.",
    )
}

/// Create a repo with two commits: `INITIAL_SPEC`, then an unbumped change to
/// `auth.login`. The index and working tree match the second commit.
fn repo_with_committed_unbumped_change(root: &Path) {
    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");
    fs::write(root.join("spec.md"), unbumped_login_change()).unwrap();
    git_commit_all(root, "unbumped change");
}

/// An explicit `from` revision is compared against the index instead of HEAD.
#[tokio::test]
async fn test_from_revision_compares_against_index() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    repo_with_committed_unbumped_change(root);

    let config = simple_config();

    // Default (HEAD vs index): nothing staged, nothing flagged.
    let default = detect_changed_rules(root, &config, None, &CompareTarget::Index)
        .await
        .unwrap();
    assert!(default.is_empty(), "HEAD vs index should have no changes");

    let changes = detect_changed_rules(root, &config, Some("HEAD~1"), &CompareTarget::Index)
        .await
        .unwrap();
    assert_eq!(changes.len(), 1, "expected exactly one changed rule");
    assert_eq!(changes[0].rule_id.base, "auth.login");
}

/// An explicit `to` revision is read from the commit, not the index or working tree.
#[tokio::test]
async fn test_to_revision_ignores_index_and_working_tree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    repo_with_committed_unbumped_change(root);

    // Stage a fix and leave a different edit in the working tree; neither
    // should affect a comparison between two commits.
    let fixed = unbumped_login_change().replace("r[auth.login]", "r[auth.login+2]");
    fs::write(root.join("spec.md"), &fixed).unwrap();
    git_add(root, "spec.md");
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();

    let config = simple_config();
    let changes = detect_changed_rules(
        root,
        &config,
        Some("HEAD~1"),
        &CompareTarget::Revision("HEAD".to_string()),
    )
    .await
    .unwrap();
    assert_eq!(changes.len(), 1, "expected exactly one changed rule");
    assert_eq!(changes[0].rule_id.base, "auth.login");
    assert_eq!(changes[0].rule_id.version, 1);
}

/// With only `to` given, the comparison is from HEAD to that revision.
#[tokio::test]
async fn test_to_revision_defaults_from_to_head() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    repo_with_committed_unbumped_change(root);

    let config = simple_config();

    // HEAD vs HEAD: no changes.
    let same = detect_changed_rules(
        root,
        &config,
        None,
        &CompareTarget::Revision("HEAD".to_string()),
    )
    .await
    .unwrap();
    assert!(same.is_empty(), "HEAD vs HEAD should have no changes");

    // Move HEAD back so the unbumped change is only in the `to` revision.
    let tip = git_stdout(root, &["rev-parse", "HEAD"]).trim().to_string();
    git_stdout(root, &["reset", "--hard", "HEAD~1"]);
    let changes = detect_changed_rules(root, &config, None, &CompareTarget::Revision(tip))
        .await
        .unwrap();
    assert_eq!(changes.len(), 1, "expected exactly one changed rule");
    assert_eq!(changes[0].rule_id.base, "auth.login");
}

/// `pre_commit` honours `from` and `to`.
#[tokio::test]
async fn test_pre_commit_from_and_to() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    repo_with_committed_unbumped_change(root);

    let config = simple_config();
    assert!(
        pre_commit(root, &config, None, None).await.unwrap(),
        "default pre-commit should pass with nothing staged"
    );
    assert!(
        !pre_commit(root, &config, Some("HEAD~1"), None)
            .await
            .unwrap(),
        "pre-commit from HEAD~1 should fail"
    );
    assert!(
        !pre_commit(root, &config, Some("HEAD~1"), Some("HEAD"))
            .await
            .unwrap(),
        "pre-commit from HEAD~1 to HEAD should fail"
    );
    assert!(
        pre_commit(root, &config, Some("HEAD"), Some("HEAD"))
            .await
            .unwrap(),
        "pre-commit from HEAD to HEAD should pass"
    );
}

/// An explicit `from` that does not resolve is an error.
#[tokio::test]
async fn test_unresolvable_from_is_error() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    repo_with_committed_unbumped_change(root);

    let config = simple_config();
    let result =
        detect_changed_rules(root, &config, Some("no-such-rev"), &CompareTarget::Index).await;
    let msg = result
        .expect_err("unresolvable from should error")
        .to_string();
    assert!(
        msg.contains("no-such-rev"),
        "error should name the revision, got: {msg}"
    );

    assert!(
        pre_commit(root, &config, Some("no-such-rev"), None)
            .await
            .is_err()
    );
    assert!(
        bump(root, &config, Some("no-such-rev"), false)
            .await
            .is_err()
    );
}

/// An explicit `to` that does not resolve is an error.
#[tokio::test]
async fn test_unresolvable_to_is_error() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    repo_with_committed_unbumped_change(root);

    let config = simple_config();
    let result = pre_commit(root, &config, None, Some("no-such-rev")).await;
    let msg = result
        .expect_err("unresolvable to should error")
        .to_string();
    assert!(
        msg.contains("no-such-rev"),
        "error should name the revision, got: {msg}"
    );
}

/// Explicit revisions are an error in a repo with no HEAD, unlike the default.
#[tokio::test]
async fn test_explicit_revisions_error_without_head() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_add(root, "spec.md");

    let config = simple_config();
    assert!(
        pre_commit(root, &config, Some("HEAD"), None).await.is_err(),
        "explicit from should error without HEAD"
    );
    assert!(
        pre_commit(root, &config, None, Some("HEAD")).await.is_err(),
        "explicit to should error without HEAD"
    );
    assert!(
        bump(root, &config, Some("HEAD"), false).await.is_err(),
        "explicit from should error without HEAD"
    );
}

/// Revisions that look like options are rejected rather than passed to git.
#[tokio::test]
async fn test_option_like_revision_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    repo_with_committed_unbumped_change(root);

    let config = simple_config();
    assert!(
        pre_commit(root, &config, Some("--all"), None)
            .await
            .is_err()
    );
    assert!(
        pre_commit(root, &config, None, Some("--all"))
            .await
            .is_err()
    );
}

/// `bump` with `from` bumps rules changed since that revision and re-stages them.
#[tokio::test]
async fn test_bump_from_revision() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    repo_with_committed_unbumped_change(root);

    let config = simple_config();
    let bumped = bump(root, &config, Some("HEAD~1"), false).await.unwrap();
    assert_eq!(bumped.len(), 1);
    assert_eq!(bumped[0].base, "auth.login");
    assert_eq!(bumped[0].version, 2);

    let expected = unbumped_login_change().replace("r[auth.login]", "r[auth.login+2]");
    assert_eq!(fs::read_to_string(root.join("spec.md")).unwrap(), expected);
    assert_eq!(
        git_stdout(root, &["show", ":spec.md"]),
        expected,
        "bumped file should be re-staged"
    );

    assert!(
        pre_commit(root, &config, Some("HEAD~1"), None)
            .await
            .unwrap(),
        "pre-commit from HEAD~1 should pass after bump from HEAD~1"
    );
}

/// `bump` with `unstaged` bumps working tree changes and does not stage them.
#[tokio::test]
async fn test_unstaged_bump_does_not_restage() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");

    fs::write(root.join("spec.md"), unbumped_login_change()).unwrap();

    let config = simple_config();

    // The default (index) comparison sees nothing.
    assert!(bump(root, &config, None, false).await.unwrap().is_empty());

    let bumped = bump(root, &config, None, true).await.unwrap();
    assert_eq!(bumped.len(), 1);
    assert_eq!(bumped[0].base, "auth.login");
    assert_eq!(bumped[0].version, 2);

    assert_eq!(
        fs::read_to_string(root.join("spec.md")).unwrap(),
        unbumped_login_change().replace("r[auth.login]", "r[auth.login+2]")
    );
    assert_eq!(
        git_stdout(root, &["show", ":spec.md"]),
        INITIAL_SPEC,
        "index should not be updated by an unstaged bump"
    );
}

/// `bump` with `unstaged` keeps both staged and unstaged edits.
#[tokio::test]
async fn test_unstaged_bump_preserves_staged_and_unstaged_edits() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");

    let staged = unbumped_login_change();
    fs::write(root.join("spec.md"), &staged).unwrap();
    git_add(root, "spec.md");

    let working = staged.replace(
        "Sessions MUST expire after 24 hours of inactivity.",
        "Sessions MUST expire after 12 hours of inactivity.",
    );
    fs::write(root.join("spec.md"), &working).unwrap();

    let config = simple_config();
    let bumped = bump(root, &config, None, true).await.unwrap();
    let mut bases: Vec<&str> = bumped.iter().map(|r| r.base.as_str()).collect();
    bases.sort();
    assert_eq!(bases, ["auth.login", "auth.session"]);

    assert_eq!(
        fs::read_to_string(root.join("spec.md")).unwrap(),
        working
            .replace("r[auth.login]", "r[auth.login+2]")
            .replace("r[auth.session]", "r[auth.session+2]")
    );
    assert_eq!(git_stdout(root, &["show", ":spec.md"]), staged);
}

/// Running `bump` with `unstaged` twice is a no-op on the second call.
#[tokio::test]
async fn test_unstaged_bump_idempotency() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    git_init(root);
    fs::write(root.join("spec.md"), INITIAL_SPEC).unwrap();
    git_commit_all(root, "initial");
    fs::write(root.join("spec.md"), unbumped_login_change()).unwrap();

    let config = simple_config();
    assert_eq!(bump(root, &config, None, true).await.unwrap().len(), 1);
    assert!(
        bump(root, &config, None, true).await.unwrap().is_empty(),
        "second unstaged bump should be a no-op"
    );
}

/// `bump` with `unstaged` and `from` compares the working tree against `from`.
#[tokio::test]
async fn test_unstaged_bump_from_revision() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    repo_with_committed_unbumped_change(root);

    let working = unbumped_login_change().replace(
        "Sessions MUST expire after 24 hours of inactivity.",
        "Sessions MUST expire after 12 hours of inactivity.",
    );
    fs::write(root.join("spec.md"), &working).unwrap();

    let config = simple_config();
    let bumped = bump(root, &config, Some("HEAD~1"), true).await.unwrap();
    let mut bases: Vec<&str> = bumped.iter().map(|r| r.base.as_str()).collect();
    bases.sort();
    assert_eq!(bases, ["auth.login", "auth.session"]);

    assert_eq!(
        git_stdout(root, &["show", ":spec.md"]),
        unbumped_login_change(),
        "index should not be updated by an unstaged bump"
    );
}
