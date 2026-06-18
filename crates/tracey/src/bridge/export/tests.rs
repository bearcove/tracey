//! Integration-level tests for the export module.

use super::*;

#[test]
fn test_relative_root_depths() {
    assert_eq!(relative_root("index.html"), ".");
    assert_eq!(relative_root("spec/index.html"), "..");
    assert_eq!(relative_root("a/b/c.html"), "../..");
}

#[test]
fn test_section_stem() {
    assert_eq!(section_stem("docs/spec/tracey.md"), "tracey");
    assert_eq!(section_stem("spec.md"), "spec");
    assert_eq!(section_stem("path/to/99-references.md"), "99-references");
}

#[test]
fn test_extract_heading_ids() {
    let html = r#"<h1 id="intro">Intro</h1><p>text</p><h2 id="details">Details</h2><div id="not-a-heading">skip</div>"#;
    let ids = extract_heading_ids(html);
    assert!(ids.contains("intro"));
    assert!(ids.contains("details"));
    assert!(!ids.contains("not-a-heading"));
}

#[test]
fn test_build_sidebar_spec_single_file() {
    let spec_data = tracey_api::ApiSpecData {
        name: "test".to_string(),
        sections: vec![tracey_api::SpecSection {
            source_file: "spec.md".to_string(),
            html: r#"<h1 id="language">Language</h1><h2 id="syntax">Syntax</h2>"#.to_string(),
            weight: 0,
        }],
        outline: vec![
            tracey_api::OutlineEntry {
                title: "Language".to_string(),
                slug: "language".to_string(),
                level: 1,
                coverage: Default::default(),
                aggregated: Default::default(),
            },
            tracey_api::OutlineEntry {
                title: "Syntax".to_string(),
                slug: "syntax".to_string(),
                level: 2,
                coverage: Default::default(),
                aggregated: Default::default(),
            },
        ],
        head_injections: vec![],
    };

    let sidebar = build_sidebar_spec("test", &spec_data);
    assert_eq!(sidebar.name, "test");
    assert_eq!(sidebar.href, "test/index.html");
    assert_eq!(sidebar.files.len(), 1);
    assert_eq!(sidebar.files[0].href, "test/index.html");
    assert_eq!(sidebar.files[0].headings.len(), 2);
    assert_eq!(sidebar.files[0].headings[0].title, "Language");
    assert_eq!(sidebar.files[0].headings[1].title, "Syntax");
}

#[test]
fn test_build_sidebar_spec_multi_file() {
    // Two files, each with their own headings
    let spec_data = tracey_api::ApiSpecData {
        name: "big".to_string(),
        sections: vec![
            tracey_api::SpecSection {
                source_file: "docs/01-intro.md".to_string(),
                html: r#"<h1 id="introduction">Introduction</h1><h2 id="introduction--overview">Overview</h2>"#.to_string(),
                weight: 0,
            },
            tracey_api::SpecSection {
                source_file: "docs/02-details.md".to_string(),
                html: r#"<h1 id="details">Details</h1><h2 id="details--api">API</h2>"#.to_string(),
                weight: 1,
            },
        ],
        outline: vec![
            tracey_api::OutlineEntry {
                title: "Introduction".to_string(),
                slug: "introduction".to_string(),
                level: 1,
                coverage: Default::default(),
                aggregated: Default::default(),
            },
            tracey_api::OutlineEntry {
                title: "Overview".to_string(),
                slug: "introduction--overview".to_string(),
                level: 2,
                coverage: Default::default(),
                aggregated: Default::default(),
            },
            tracey_api::OutlineEntry {
                title: "Details".to_string(),
                slug: "details".to_string(),
                level: 1,
                coverage: Default::default(),
                aggregated: Default::default(),
            },
            tracey_api::OutlineEntry {
                title: "API".to_string(),
                slug: "details--api".to_string(),
                level: 2,
                coverage: Default::default(),
                aggregated: Default::default(),
            },
        ],
        head_injections: vec![],
    };

    let sidebar = build_sidebar_spec("big", &spec_data);
    assert_eq!(sidebar.files.len(), 2);

    // First file should have Introduction + Overview
    assert_eq!(sidebar.files[0].href, "big/01-intro.html");
    assert_eq!(sidebar.files[0].headings.len(), 2);
    assert_eq!(sidebar.files[0].headings[0].title, "Introduction");
    assert_eq!(sidebar.files[0].headings[1].title, "Overview");

    // Second file should have Details + API
    assert_eq!(sidebar.files[1].href, "big/02-details.html");
    assert_eq!(sidebar.files[1].headings.len(), 2);
    assert_eq!(sidebar.files[1].headings[0].title, "Details");
    assert_eq!(sidebar.files[1].headings[1].title, "API");

    // Headings should NOT leak across files
    let file0_slugs: Vec<&str> = sidebar.files[0]
        .headings
        .iter()
        .map(|h| h.slug.as_str())
        .collect();
    assert!(!file0_slugs.contains(&"details"));
    assert!(!file0_slugs.contains(&"details--api"));
}
