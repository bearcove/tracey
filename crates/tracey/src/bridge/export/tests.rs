//! Integration-level tests for the export module.

use super::*;

#[test]
fn test_relative_root_depths() {
    assert_eq!(relative_root("index.html"), ".");
    assert_eq!(relative_root("spec/index.html"), "..");
    assert_eq!(relative_root("a/b/c.html"), "../..");
}

#[test]
fn test_build_sidebar_spec() {
    let spec_data = tracey_api::ApiSpecData {
        name: "test".to_string(),
        sections: vec![tracey_api::SpecSection {
            source_file: "spec.md".to_string(),
            html: String::new(),
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
            tracey_api::OutlineEntry {
                title: "Deep heading".to_string(),
                slug: "deep-heading".to_string(),
                level: 3, // Should be filtered out (only h1/h2)
                coverage: Default::default(),
                aggregated: Default::default(),
            },
        ],
        head_injections: vec![],
    };

    let sidebar = build_sidebar_spec("test", &spec_data);
    assert_eq!(sidebar.name, "test");
    assert_eq!(sidebar.href, "test/index.html");
    // Only h1 and h2 headings
    assert_eq!(sidebar.headings.len(), 2);
    assert_eq!(sidebar.headings[0].title, "Language");
    assert_eq!(sidebar.headings[0].level, 1);
    assert_eq!(sidebar.headings[1].title, "Syntax");
    assert_eq!(sidebar.headings[1].level, 2);
}
