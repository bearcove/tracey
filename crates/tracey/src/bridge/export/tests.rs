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
fn test_build_sidebar_spec_single_file() {
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
        ],
        head_injections: vec![],
    };

    let sidebar = build_sidebar_spec("test", &spec_data);
    assert_eq!(sidebar.name, "test");
    assert_eq!(sidebar.href, "test/index.html");
    assert_eq!(sidebar.files.len(), 1);
    assert_eq!(sidebar.files[0].href, "test/index.html");
    assert_eq!(sidebar.files[0].headings.len(), 2);
}

#[test]
fn test_build_sidebar_spec_multi_file() {
    let spec_data = tracey_api::ApiSpecData {
        name: "big".to_string(),
        sections: vec![
            tracey_api::SpecSection {
                source_file: "docs/01-intro.md".to_string(),
                html: String::new(),
                weight: 0,
            },
            tracey_api::SpecSection {
                source_file: "docs/02-details.md".to_string(),
                html: String::new(),
                weight: 1,
            },
        ],
        outline: vec![],
        head_injections: vec![],
    };

    let sidebar = build_sidebar_spec("big", &spec_data);
    assert_eq!(sidebar.files.len(), 2);
    assert_eq!(sidebar.files[0].href, "big/01-intro.html");
    assert_eq!(sidebar.files[0].display_name, "01-intro");
    assert_eq!(sidebar.files[1].href, "big/02-details.html");
    // First file is the spec's main href
    assert_eq!(sidebar.href, "big/01-intro.html");
}
