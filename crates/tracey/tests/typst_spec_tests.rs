//! Integration tests for typst spec extraction.
//!
//! `fixtures-typst/spec.typ` mirrors the rule IDs in `fixtures/spec.md` so the
//! two backends can be compared directly.

use std::path::PathBuf;

use tracey::data::render_spec_content_for_impl;
use tracey::load_rules_from_glob;
use tracey_api::ApiSpecForward;
use tracey_core::SpecFormat;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn fixtures_typst_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures-typst")
}

#[tokio::test]
async fn extracts_rules() {
    let root = fixtures_typst_dir();
    let rules = load_rules_from_glob(&root, "spec.typ", true)
        .await
        .expect("typst extraction failed");

    assert_eq!(rules.len(), 8, "expected 8 requirements in spec.typ");

    for r in &rules {
        assert_eq!(r.format, SpecFormat::Typst);
        assert_eq!(r.prefix, "req");
        assert!(r.section.is_some(), "every req sits under a heading");
    }

    // Spot-check one.
    let login = rules
        .iter()
        .find(|r| r.def.id.base == "auth.login")
        .expect("auth.login present");
    assert_eq!(login.section_title.as_deref(), Some("Authentication"));
}

#[tokio::test]
async fn same_rules_as_markdown() {
    let md_root = fixtures_dir();
    let md_rules = load_rules_from_glob(&md_root, "spec.md", true)
        .await
        .expect("markdown extraction failed");

    let typ_root = fixtures_typst_dir();
    let typ_rules = load_rules_from_glob(&typ_root, "spec.typ", true)
        .await
        .expect("typst extraction failed");

    let mut md_ids: Vec<_> = md_rules.iter().map(|r| r.def.id.clone()).collect();
    let mut typ_ids: Vec<_> = typ_rules.iter().map(|r| r.def.id.clone()).collect();
    md_ids.sort();
    typ_ids.sort();

    assert_eq!(
        md_ids, typ_ids,
        "typst and markdown fixtures should define the same rule IDs"
    );
}

/// Regression: a markdown-only spec must produce the same outline slugs after
/// the per-format render partitioning as it did when everything went through a
/// single combined `marq::render` call.
#[tokio::test]
async fn markdown_only_outline_slugs_unchanged() {
    let root = fixtures_dir();
    let forward = ApiSpecForward {
        name: "test".to_string(),
        rules: vec![],
    };
    let spec = render_spec_content_for_impl(&root, &["spec.md".to_string()], "test", "rust", &forward)
        .await
        .expect("render failed");

    let slugs: Vec<&str> = spec.outline.iter().map(|e| e.slug.as_str()).collect();
    // marq builds hierarchical slugs (parent--child); these are the exact values
    // produced by the original single-render path.
    assert_eq!(
        slugs,
        vec![
            "test-specification",
            "test-specification--authentication",
            "test-specification--data-validation",
            "test-specification--error-handling",
        ],
        "markdown-only outline slugs must match the single-render baseline"
    );

    // Single markdown run -> single section.
    assert_eq!(spec.sections.len(), 1);
    assert_eq!(spec.sections[0].source_file, "spec.md");
}

/// Mixed-format specs render in separate runs; colliding heading titles across
/// runs must get unique slugs in the merged outline.
#[tokio::test]
async fn mixed_format_outline_dedups_heading_slugs() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("a.md"),
        "# Shared\n\nr[mix.a]\nMarkdown body.\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("b.typ"),
        "= Shared\n\n#req(\"mix.b\")[Typst body.]\n",
    )
    .unwrap();

    let forward = ApiSpecForward {
        name: "mix".to_string(),
        rules: vec![],
    };
    let spec = render_spec_content_for_impl(
        tmp.path(),
        &["*.md".to_string(), "*.typ".to_string()],
        "mix",
        "rust",
        &forward,
    )
    .await
    .expect("render failed");

    // One section per run (md run = 1 file, typ file = 1 section).
    assert_eq!(spec.sections.len(), 2);
    assert_eq!(spec.sections[0].source_file, "a.md");
    assert_eq!(spec.sections[1].source_file, "b.typ");

    let slugs: Vec<&str> = spec.outline.iter().map(|e| e.slug.as_str()).collect();
    assert_eq!(
        slugs,
        vec!["shared", "shared-2"],
        "colliding heading slugs across format runs must be deduplicated"
    );
}
