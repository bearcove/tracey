//! Common test utilities.

#![allow(dead_code)]

use std::path::PathBuf;

/// Get the path to the test fixtures directory.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Create a temporary directory for test isolation.
pub fn create_temp_project() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("Failed to create temp dir");

    // Copy fixtures to temp dir
    let fixtures = fixtures_dir();

    // Copy spec.md
    std::fs::copy(fixtures.join("spec.md"), temp.path().join("spec.md"))
        .expect("Failed to copy spec.md");

    // Copy config.kdl
    std::fs::copy(fixtures.join("config.kdl"), temp.path().join("config.kdl"))
        .expect("Failed to copy config.kdl");

    // Create src directory and copy source files
    std::fs::create_dir_all(temp.path().join("src")).expect("Failed to create src dir");
    std::fs::copy(fixtures.join("src/lib.rs"), temp.path().join("src/lib.rs"))
        .expect("Failed to copy lib.rs");
    std::fs::copy(
        fixtures.join("src/tests.rs"),
        temp.path().join("src/tests.rs"),
    )
    .expect("Failed to copy tests.rs");

    temp
}
