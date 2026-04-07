//! Reusable maud components for the static export.

use maud::{Markup, PreEscaped, html};

use super::{SidebarHeading, SidebarSpec, relative_root};

// r[impl export.style.light-dark]
// r[impl export.output.relative-links]
/// The outer HTML shell wrapping every page.
pub(crate) fn page_shell(
    title: &str,
    page_path: &str,
    sidebar_entries: &[SidebarSpec],
    content: Markup,
) -> Markup {
    let root = relative_root(page_path);

    html! {
        (PreEscaped("<!DOCTYPE html>"))
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " — tracey" }
                link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Recursive:slnt,wght,CASL,CRSV,MONO@-15..0,300..1000,0..1,0..1,0..1&display=swap";
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@arborium/arborium@2.4.6/dist/themes/base.css";
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@arborium/arborium@2.4.6/dist/themes/kanagawa-dragon.css";
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@arborium/arborium@2.4.6/dist/themes/github-light.css";
                link rel="stylesheet" href=(format!("{root}/assets/style.css"));
            }
            body {
                .layout {
                    // Static header bar
                    .header.export-header {
                        .header-inner {
                            // r[impl export.sidebar.mobile]
                            button #sidebar-toggle .sidebar-toggle { "☰" }
                            a .export-header-title href=(format!("{root}/index.html")) {
                                (title)
                            }
                        }
                    }
                    .main {
                        (sidebar(page_path, sidebar_entries))
                        .content {
                            (content)
                            footer .export-footer {
                                "Made with "
                                a href="https://tracey.bearcove.eu/" { "Tracey" }
                            }
                        }
                    }
                }
                script src=(format!("{root}/assets/enhance.js")) {}
            }
        }
    }
}

// r[impl export.sidebar.structure]
// r[impl export.sidebar.current-page]
// r[impl export.sidebar.links]
/// The page-tree sidebar.
pub(crate) fn sidebar(current_page: &str, specs: &[SidebarSpec]) -> Markup {
    html! {
        nav .sidebar {
            // README entry at the top
            a .sidebar-readme-link
              .active[current_page == "index.html"]
              href=(sidebar_href(current_page, "index.html"))
            {
                "README"
            }

            @for spec in specs {
                (sidebar_spec_entry(current_page, spec))
            }
        }
    }
}

fn sidebar_spec_entry(current_page: &str, spec: &SidebarSpec) -> Markup {
    let is_active = current_page.starts_with(&format!("{}/", spec.name));

    html! {
        // r[impl export.sidebar.collapsible]
        details .sidebar-section
                open[is_active]
                data-sidebar-key=(spec.name) {
            summary {
                a .sidebar-spec-link
                  .active[is_active]
                  href=(sidebar_href(current_page, &spec.href)) {
                    (spec.name)
                }
            }
            // Headings as a collapsible nested tree — h1s are collapsible
            (heading_tree(current_page, &spec.headings, &spec.href))
        }
    }
}

/// Build a nested tree from a flat list of headings.
/// Top-level (h1) headings are rendered as collapsible `<details>` elements
/// (closed by default). h2s nest inside them as plain links.
fn heading_tree(current_page: &str, headings: &[SidebarHeading], spec_href: &str) -> Markup {
    if headings.is_empty() {
        return html! {};
    }

    let min_level = headings.iter().map(|h| h.level).min().unwrap_or(1);

    // Group: each heading at min_level starts a group, deeper headings are children.
    // Headings that appear before the first min_level heading are promoted to
    // their own groups (rendered as standalone items).
    let mut groups: Vec<(&SidebarHeading, Vec<&SidebarHeading>)> = Vec::new();
    for heading in headings {
        if heading.level == min_level {
            groups.push((heading, Vec::new()));
        } else if let Some(last) = groups.last_mut() {
            last.1.push(heading);
        } else {
            // Orphan heading before any min_level parent — promote it
            groups.push((heading, Vec::new()));
        }
    }

    html! {
        ul .sidebar-tree.outline-tree {
            @for (parent, children) in &groups {
                @if children.is_empty() {
                    // Leaf heading — plain link
                    li {
                        a .sidebar-heading
                          href=(format!("{}#{}", sidebar_href(current_page, spec_href), parent.slug)) {
                            (parent.title)
                        }
                    }
                } @else {
                    // Heading with children — collapsible
                    li {
                        details .sidebar-subsection
                                data-sidebar-key=(parent.slug) {
                            summary {
                                a .sidebar-heading
                                  href=(format!("{}#{}", sidebar_href(current_page, spec_href), parent.slug)) {
                                    (parent.title)
                                }
                            }
                            @let child_headings: Vec<SidebarHeading> = children.iter().map(|h| (*h).clone()).collect();
                            (heading_tree(current_page, &child_headings, spec_href))
                        }
                    }
                }
            }
        }
    }
}

/// Compute a relative href from the current page to a target page.
/// Both paths are relative to the site root.
// r[impl export.output.relative-links]
pub(crate) fn sidebar_href(from: &str, to: &str) -> String {
    let root = relative_root(from);
    if to == "index.html" && from == "index.html" {
        return ".".to_string();
    }
    format!("{root}/{to}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relative_root() {
        assert_eq!(relative_root("index.html"), ".");
        assert_eq!(relative_root("spec/index.html"), "..");
        assert_eq!(relative_root("spec/sub/page.html"), "../..");
    }

    #[test]
    fn test_sidebar_href() {
        // From root to root
        assert_eq!(sidebar_href("index.html", "index.html"), ".");

        // From root to spec page
        assert_eq!(
            sidebar_href("index.html", "tracey/index.html"),
            "./tracey/index.html"
        );

        // From spec page to root
        assert_eq!(
            sidebar_href("tracey/index.html", "index.html"),
            "../index.html"
        );

        // From one spec to another
        assert_eq!(
            sidebar_href("tracey/index.html", "other/index.html"),
            "../other/index.html"
        );
    }

    #[test]
    fn test_page_shell_contains_doctype() {
        let markup = page_shell("Test", "index.html", &[], html! { p { "hello" } });
        let s = markup.into_string();
        assert!(s.starts_with("<!DOCTYPE html>"));
        assert!(s.contains("<title>Test — tracey</title>"));
        assert!(s.contains("./assets/style.css"));
        assert!(s.contains("hello"));
        // Header
        assert!(s.contains("sidebar-toggle"));
        // Footer
        assert!(s.contains("Made with"));
        assert!(s.contains("https://tracey.bearcove.eu/"));
    }

    #[test]
    fn test_page_shell_asset_paths_relative() {
        let shallow = page_shell("T", "index.html", &[], html! {}).into_string();
        assert!(shallow.contains("./assets/style.css"));
        assert!(shallow.contains("./assets/enhance.js"));

        let deep = page_shell("T", "spec/index.html", &[], html! {}).into_string();
        assert!(deep.contains("../assets/style.css"));
        assert!(deep.contains("../assets/enhance.js"));
    }

    #[test]
    fn test_sidebar_renders_readme_link() {
        let sidebar_markup = sidebar("index.html", &[]);
        let s = sidebar_markup.into_string();
        assert!(s.contains("README"));
        assert!(s.contains("active")); // README link should be active on index
    }

    #[test]
    fn test_sidebar_spec_entry() {
        let specs = vec![SidebarSpec {
            name: "myspec".to_string(),
            href: "myspec/index.html".to_string(),
            headings: vec![SidebarHeading {
                title: "Introduction".to_string(),
                slug: "introduction".to_string(),
                level: 2,
            }],
        }];

        let s = sidebar("index.html", &specs).into_string();
        assert!(s.contains("myspec"));
        assert!(s.contains("Introduction"));
        assert!(s.contains("#introduction"));
    }

    #[test]
    fn test_sidebar_nested_headings_with_collapsible_h1s() {
        let specs = vec![SidebarSpec {
            name: "myspec".to_string(),
            href: "myspec/index.html".to_string(),
            headings: vec![
                SidebarHeading {
                    title: "Language".to_string(),
                    slug: "language".to_string(),
                    level: 1,
                },
                SidebarHeading {
                    title: "Syntax".to_string(),
                    slug: "syntax".to_string(),
                    level: 2,
                },
                SidebarHeading {
                    title: "Tooling".to_string(),
                    slug: "tooling".to_string(),
                    level: 1,
                },
            ],
        }];

        let s = sidebar("index.html", &specs).into_string();

        // h1 "Language" with children should be in a collapsible <details>
        assert!(s.contains("sidebar-subsection"));
        assert!(s.contains("#language"));
        assert!(s.contains("#syntax"));
        assert!(s.contains("#tooling"));

        // Syntax should be nested inside Language's <details>
        let lang_pos = s.find("#language").unwrap();
        let syntax_pos = s.find("#syntax").unwrap();
        let tooling_pos = s.find("#tooling").unwrap();
        assert!(syntax_pos > lang_pos);
        assert!(tooling_pos > syntax_pos);

        // Nested list between Language and Syntax
        let between = &s[lang_pos..syntax_pos];
        assert!(between.contains("<ul"), "h2 should be in a nested <ul>");
    }

    #[test]
    fn test_sidebar_orphan_headings_before_first_h1() {
        // "Introduction" (h2) comes before the first h1 "Language"
        let specs = vec![SidebarSpec {
            name: "myspec".to_string(),
            href: "myspec/index.html".to_string(),
            headings: vec![
                SidebarHeading {
                    title: "Introduction".to_string(),
                    slug: "introduction".to_string(),
                    level: 2,
                },
                SidebarHeading {
                    title: "Language".to_string(),
                    slug: "language".to_string(),
                    level: 1,
                },
                SidebarHeading {
                    title: "Syntax".to_string(),
                    slug: "syntax".to_string(),
                    level: 2,
                },
            ],
        }];

        let s = sidebar("index.html", &specs).into_string();
        // Introduction must appear and come before Language
        let intro_pos = s
            .find("#introduction")
            .expect("Introduction must be in sidebar");
        let lang_pos = s.find("#language").unwrap();
        assert!(intro_pos < lang_pos);
    }

    #[test]
    fn test_sidebar_active_spec() {
        let specs = vec![SidebarSpec {
            name: "tracey".to_string(),
            href: "tracey/index.html".to_string(),
            headings: vec![],
        }];

        let s = sidebar("tracey/index.html", &specs).into_string();
        // The spec section should be open and the link active
        assert!(s.contains("open"));
        assert!(s.contains("sidebar-spec-link active"));
    }
}
