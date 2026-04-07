//! Integration tests for typst spec extraction.
//!
//! `fixtures-typst/spec.typ` mirrors the rule IDs in `fixtures/spec.md` so the
//! two backends can be compared directly.

use std::path::PathBuf;

use tracey::load_rules_from_glob;
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
