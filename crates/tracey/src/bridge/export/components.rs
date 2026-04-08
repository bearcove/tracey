//! Reusable maud components for the static export.

use maud::{Markup, PreEscaped, html};

use super::{SidebarFile, SidebarHeading, SidebarSpec, relative_root};

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
/// The page-tree sidebar, using the dashboard's TOC markup structure.
pub(crate) fn sidebar(current_page: &str, specs: &[SidebarSpec]) -> Markup {
    html! {
        nav .sidebar {
            .sidebar-content {
                // README entry
                ul .outline-tree {
                    li .toc-item.depth-0
                       .is-active[current_page == "index.html"] {
                        a .toc-row href=(sidebar_href(current_page, "index.html")) {
                            span .toc-link { "README" }
                        }
                    }
                }

                @for spec in specs {
                    (sidebar_spec_entry(current_page, spec))
                }
            }
            footer .export-footer {
                "Made with "
                a href="https://tracey.bearcove.eu/" { "Tracey" }
            }
        }
    }
}

fn sidebar_spec_entry(current_page: &str, spec: &SidebarSpec) -> Markup {
    let show_files = spec.files.len() > 1;

    html! {
        @if show_files {
            // Multi-file spec: show file entries, each with their own headings
            @for file in &spec.files {
                @let is_file_active = current_page == file.href;
                ul .outline-tree {
                    li .toc-item.depth-0
                       .is-active[is_file_active]
                       data-slug=(file.display_name) {
                        a .toc-row href=(sidebar_href(current_page, &file.href)) {
                            span .toc-link { (file.display_name) }
                        }
                    }
                }
                @if is_file_active {
                    (heading_tree(current_page, &file.headings, &file.href))
                }
            }
        } @else if let Some(file) = spec.files.first() {
            // Single-file spec: show headings directly
            (heading_tree(current_page, &file.headings, &file.href))
        }
    }
}

/// Build a nested tree from a flat list of headings using the dashboard's
/// `.toc-item` / `.toc-row` / `.toc-children` structure.
/// Folding is handled by JS (toggling a `.is-collapsed` class on `.toc-children`).
fn heading_tree(current_page: &str, headings: &[SidebarHeading], spec_href: &str) -> Markup {
    if headings.is_empty() {
        return html! {};
    }

    let min_level = headings.iter().map(|h| h.level).min().unwrap_or(1);

    // Group: each heading at min_level starts a group, deeper headings are children.
    let mut groups: Vec<(&SidebarHeading, Vec<&SidebarHeading>)> = Vec::new();
    for heading in headings {
        if heading.level == min_level {
            groups.push((heading, Vec::new()));
        } else if let Some(last) = groups.last_mut() {
            last.1.push(heading);
        } else {
            groups.push((heading, Vec::new()));
        }
    }

    html! {
        ul .outline-tree {
            @for (parent, children) in &groups {
                li .toc-item
                   .(format!("depth-{}", parent.level.saturating_sub(1)))
                   data-slug=(parent.slug) {
                    @if children.is_empty() {
                        a .toc-row
                          href=(format!("{}#{}", sidebar_href(current_page, spec_href), parent.slug)) {
                            span .toc-link { (parent.title) }
                        }
                    } @else {
                        .toc-row {
                            a .toc-link
                              href=(format!("{}#{}", sidebar_href(current_page, spec_href), parent.slug)) {
                                (parent.title)
                            }
                            button .toc-fold-btn title="Toggle section" { "+" }
                        }
                        @let child_headings: Vec<SidebarHeading> = children.iter().map(|h| (*h).clone()).collect();
                        ul .toc-children.is-collapsed {
                            @for child in &child_headings {
                                li .toc-item
                                   .(format!("depth-{}", child.level.saturating_sub(1)))
                                   data-slug=(child.slug) {
                                    a .toc-row
                                      href=(format!("{}#{}", sidebar_href(current_page, spec_href), child.slug)) {
                                        span .toc-link { (child.title) }
                                    }
                                }
                            }
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
    fn test_sidebar_renders_readme() {
        let s = sidebar("index.html", &[]).into_string();
        assert!(s.contains("README"));
        assert!(s.contains("toc-item"));
        assert!(s.contains("is-active")); // README active on index page
    }

    fn make_single_file_spec(name: &str, headings: Vec<SidebarHeading>) -> SidebarSpec {
        SidebarSpec {
            name: name.to_string(),
            href: format!("{name}/index.html"),
            files: vec![SidebarFile {
                display_name: "spec".to_string(),
                href: format!("{name}/index.html"),
                headings,
            }],
        }
    }

    #[test]
    fn test_sidebar_spec_headings() {
        let specs = vec![make_single_file_spec(
            "myspec",
            vec![SidebarHeading {
                title: "Introduction".to_string(),
                slug: "introduction".to_string(),
                level: 2,
            }],
        )];

        let s = sidebar("index.html", &specs).into_string();
        assert!(s.contains("Introduction"));
        assert!(s.contains("#introduction"));
        assert!(s.contains("toc-item"));
        assert!(s.contains("toc-row"));
        assert!(s.contains("toc-link"));
    }

    #[test]
    fn test_sidebar_nested_headings() {
        let specs = vec![make_single_file_spec(
            "myspec",
            vec![
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
        )];

        let s = sidebar("index.html", &specs).into_string();

        assert!(s.contains("#language"));
        assert!(s.contains("#syntax"));
        assert!(s.contains("#tooling"));

        let lang_pos = s.find("#language").unwrap();
        let syntax_pos = s.find("#syntax").unwrap();
        let tooling_pos = s.find("#tooling").unwrap();
        assert!(syntax_pos > lang_pos);
        assert!(tooling_pos > syntax_pos);

        let between = &s[lang_pos..syntax_pos];
        assert!(
            between.contains("toc-children"),
            "h2 should be in a .toc-children"
        );
    }

    #[test]
    fn test_sidebar_orphan_headings_before_first_h1() {
        let specs = vec![make_single_file_spec(
            "myspec",
            vec![
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
            ],
        )];

        let s = sidebar("index.html", &specs).into_string();
        let intro_pos = s
            .find("#introduction")
            .expect("Introduction must be in sidebar");
        let lang_pos = s.find("#language").unwrap();
        assert!(intro_pos < lang_pos);
    }

    #[test]
    fn test_sidebar_multi_file_spec() {
        let specs = vec![SidebarSpec {
            name: "big".to_string(),
            href: "big/intro.html".to_string(),
            files: vec![
                SidebarFile {
                    display_name: "intro".to_string(),
                    href: "big/intro.html".to_string(),
                    headings: vec![],
                },
                SidebarFile {
                    display_name: "details".to_string(),
                    href: "big/details.html".to_string(),
                    headings: vec![],
                },
            ],
        }];

        // When on the intro page, both files should appear
        let s = sidebar("big/intro.html", &specs).into_string();
        assert!(s.contains("intro"));
        assert!(s.contains("details"));
        // intro should be active
        assert!(s.contains("is-active"));
    }
}
