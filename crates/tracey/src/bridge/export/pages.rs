//! Page-level templates for the static export.

use maud::{Markup, PreEscaped, html};
use tracey_api::{ApiSpecData, ApiSpecForward};

use super::components::page_shell;
use super::{SidebarSpec, SpecExportData};

// r[impl export.landing.readme]
// r[impl export.landing.spec-list]
pub(crate) fn landing_page(
    readme_html: &Option<String>,
    specs: &[SpecExportData],
    sidebar_entries: &[SidebarSpec],
) -> Markup {
    let content = html! {
        .content.export-content.landing-page {
            @if let Some(readme) = readme_html {
                .landing-readme.markdown {
                    (PreEscaped(readme))
                }
            } @else {
                .landing-readme {
                    h1 { "Specification Coverage" }
                    p { "This site contains rendered specifications with traceability coverage data." }
                }
            }

            .landing-specs {
                h2 { "Specifications" }
                .spec-card-grid {
                    @for data in specs {
                        @let total = data.forward.rules.len();
                        @let implemented = data.forward.rules.iter().filter(|r| !r.impl_refs.is_empty()).count();
                        @let tested = data.forward.rules.iter().filter(|r| !r.verify_refs.is_empty()).count();
                        @let href = if data.spec_content.sections.len() == 1 {
                            format!("{}/index.html", data.spec_name)
                        } else if let Some(first) = data.spec_content.sections.first() {
                            let stem = std::path::Path::new(&first.source_file)
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("index");
                            format!("{}/{stem}.html", data.spec_name)
                        } else {
                            format!("{}/index.html", data.spec_name)
                        };

                        a .spec-card href=(href) {
                            .spec-card-name { (&data.spec_name) }
                            @if specs.iter().any(|s| s.spec_name == data.spec_name && s.impl_name != data.impl_name) {
                                .spec-card-impl { "impl: " (&data.impl_name) }
                            }
                            .spec-card-stats {
                                (format!("{implemented}/{total} implemented, {tested}/{total} tested"))
                            }
                        }
                    }
                }
            }
        }
    };

    page_shell("Home", "index.html", sidebar_entries, content)
}

// r[impl export.spec-page.rendered-markdown]
// r[impl export.spec-page.anchors]
pub(crate) fn spec_page(
    spec_name: &str,
    _impl_name: &str,
    forward: &ApiSpecForward,
    spec_data: &ApiSpecData,
    sidebar_entries: &[SidebarSpec],
    page_path: &str,
) -> Markup {
    // Build a lookup from rule ID to its forward data
    let rules_by_id: std::collections::HashMap<String, &tracey_api::ApiRule> = forward
        .rules
        .iter()
        .map(|r| (r.id.to_string(), r))
        .collect();

    // Concatenate all sections into one page
    let mut all_html = String::new();
    for section in &spec_data.sections {
        let enhanced = enhance_spec_html(&section.html, &rules_by_id);
        all_html.push_str(&enhanced);
    }

    let title = if let Some(first_heading) = spec_data.outline.first() {
        first_heading.title.clone()
    } else {
        spec_name.to_string()
    };

    let content = html! {
        .content.export-content.spec-page {
            .markdown {
                (PreEscaped(&all_html))
            }
        }
    };

    page_shell(&title, page_path, sidebar_entries, content)
}

/// Post-process spec HTML to:
/// 1. Strip dashboard-specific elements (badge links, edit buttons, data attributes)
/// 2. Inject export-specific coverage info (status classes, ref lists, icons)
fn enhance_spec_html(
    html: &str,
    rules: &std::collections::HashMap<String, &tracey_api::ApiRule>,
) -> String {
    let mut result = html.to_string();

    // Step 1: Strip dashboard-specific elements
    result = strip_dashboard_elements(&result);

    // Step 2: For each rule, add export status class and coverage refs
    for (req_id, rule) in rules {
        let impl_refs: Vec<String> = rule
            .impl_refs
            .iter()
            .map(|r| format!("{}:{}", r.file, r.line))
            .collect();
        let verify_refs: Vec<String> = rule
            .verify_refs
            .iter()
            .map(|r| format!("{}:{}", r.file, r.line))
            .collect();

        let has_impl = !impl_refs.is_empty();
        let has_verify = !verify_refs.is_empty();

        let status_class = match (has_impl, has_verify) {
            (true, true) => "export-req-covered",
            (true, false) | (false, true) => "export-req-partial",
            (false, false) => "export-req-uncovered",
        };

        // Build our replacement badge + coverage HTML
        let mut badge_html = format!(
            r#"<div class="req-badges-left export-req-badges"><span class="req-badge req-id export-req-id">{req_id}</span>"#
        );

        badge_html.push_str("</div>");

        // Coverage references
        // if has_impl || has_verify {
        //     badge_html.push_str(r#"<div class="export-req-refs">"#);
        //     if has_impl {
        //         badge_html.push_str(r#"<div><span class="export-req-refs-label">impl: </span>"#);
        //         for (i, r) in impl_refs.iter().enumerate() {
        //             if i > 0 {
        //                 badge_html.push_str(", ");
        //             }
        //             badge_html.push_str(&format!(r#"<span class="export-req-ref">{r}</span>"#));
        //         }
        //         badge_html.push_str("</div>");
        //     }
        //     if has_verify {
        //         badge_html.push_str(r#"<div><span class="export-req-refs-label">verify: </span>"#);
        //         for (i, r) in verify_refs.iter().enumerate() {
        //             if i > 0 {
        //                 badge_html.push_str(", ");
        //             }
        //             badge_html.push_str(&format!(r#"<span class="export-req-ref">{r}</span>"#));
        //         }
        //         badge_html.push_str("</div>");
        //     }
        //     badge_html.push_str("</div>");
        // }

        // Find the req-container for this ID (id="r-{req_id}" on the container div)
        let search = format!(r#"id="r-{req_id}""#);
        if let Some(pos) = result.find(&search) {
            // Add our status class to the req-container
            if let Some(class_pos) = result[..pos].rfind("class=\"") {
                let insert_pos = class_pos + 7;
                result.insert_str(insert_pos, &format!("{status_class} "));
            }

            // Insert our badge HTML right after the opening tag
            if let Some(tag_end) = result[pos..].find('>') {
                let insert_at = pos + tag_end + 1;
                result.insert_str(insert_at, &badge_html);
            }
        }
    }

    result
}

/// Remove dashboard-specific elements from the rendered spec HTML.
/// These are added by the daemon for the interactive dashboard but
/// don't belong in a static export.
fn strip_dashboard_elements(html: &str) -> String {
    let mut result = html.to_string();

    // Remove req-badges-left divs (dashboard badge groups with copy buttons and links)
    result = remove_divs_with_class(&result, "req-badges-left");

    // Remove req-badges-right divs (dashboard coverage badges)
    result = remove_divs_with_class(&result, "req-badges-right");

    // Remove data attributes that leak dashboard internals / local paths
    for attr in &[
        "data-br",
        "data-source-file",
        "data-source-line",
        "data-rule",
    ] {
        result = remove_attribute(&result, attr);
    }

    result
}

/// Remove all `<div class="{class}">...</div>` blocks, tracking depth.
fn remove_divs_with_class(html: &str, class: &str) -> String {
    let needle = format!(r#"class="{class}""#);
    let mut result = String::with_capacity(html.len());
    let mut remaining = html;

    while !remaining.is_empty() {
        // Check if we're at a <div that has our target class
        if remaining.starts_with("<div ") {
            let tag_end = remaining.find('>');
            let has_class = tag_end.is_some_and(|end| remaining[..end].contains(&needle));

            if has_class {
                // Skip this entire div by tracking nesting depth
                let mut depth = 0;
                let mut j = 0;
                while j < remaining.len() {
                    if remaining[j..].starts_with("<div") {
                        depth += 1;
                    }
                    if remaining[j..].starts_with("</div>") {
                        depth -= 1;
                        if depth == 0 {
                            j += "</div>".len();
                            break;
                        }
                    }
                    j += remaining[j..].chars().next().map_or(1, char::len_utf8);
                }
                // Skip trailing whitespace
                while j < remaining.len()
                    && remaining[j..].starts_with(|c: char| c.is_ascii_whitespace())
                {
                    j += 1;
                }
                remaining = &remaining[j..];
                continue;
            }
        }

        // Not a target div — copy one char and advance
        let c = remaining.chars().next().unwrap();
        result.push(c);
        remaining = &remaining[c.len_utf8()..];
    }

    result
}

/// Remove all occurrences of ` attr="value"` from HTML.
fn remove_attribute(html: &str, attr: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let needle = format!(r#" {attr}=""#);
    let mut pos = 0;

    while let Some(rel) = html[pos..].find(&needle) {
        let abs_start = pos + rel;
        result.push_str(&html[pos..abs_start]);

        // Find the closing quote after the value
        let value_start = abs_start + needle.len();
        if let Some(end) = html[value_start..].find('"') {
            pos = value_start + end + 1;
        } else {
            result.push_str(&needle);
            pos = value_start;
        }
    }

    result.push_str(&html[pos..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracey_api::{ApiCodeRef, ApiRule};
    use tracey_core::RuleId;

    fn make_rule(
        id: &str,
        impl_refs: Vec<(&str, usize)>,
        verify_refs: Vec<(&str, usize)>,
    ) -> ApiRule {
        ApiRule {
            id: RuleId::new(id, 1).unwrap(),
            raw: String::new(),
            html: format!("<p>Requirement {id}</p>"),
            status: None,
            level: None,
            source_file: None,
            source_line: None,
            source_column: None,
            section: None,
            section_title: None,
            impl_refs: impl_refs
                .into_iter()
                .map(|(f, l)| ApiCodeRef {
                    file: f.to_string(),
                    line: l,
                })
                .collect(),
            verify_refs: verify_refs
                .into_iter()
                .map(|(f, l)| ApiCodeRef {
                    file: f.to_string(),
                    line: l,
                })
                .collect(),
            depends_refs: vec![],
            is_stale: false,
            stale_refs: vec![],
        }
    }

    #[test]
    fn test_landing_page_with_readme() {
        let readme = Some("<h1>My Project</h1><p>A great project</p>".to_string());
        let specs = vec![];
        let sidebar = vec![];

        let markup = landing_page(&readme, &specs, &sidebar);
        let s = markup.into_string();

        assert!(s.contains("My Project"));
        assert!(s.contains("A great project"));
        assert!(s.contains("Specifications"));
    }

    #[test]
    fn test_landing_page_without_readme() {
        let specs = vec![];
        let sidebar = vec![];

        let markup = landing_page(&None, &specs, &sidebar);
        let s = markup.into_string();

        assert!(s.contains("Specification Coverage"));
    }

    #[test]
    fn test_landing_page_with_specs() {
        let specs = vec![SpecExportData {
            spec_name: "myspec".to_string(),
            impl_name: "main".to_string(),
            forward: ApiSpecForward {
                name: "myspec".to_string(),
                rules: vec![
                    make_rule(
                        "auth.login",
                        vec![("src/auth.rs", 10)],
                        vec![("tests/auth.rs", 5)],
                    ),
                    make_rule("auth.logout", vec![], vec![]),
                ],
            },
            spec_content: tracey_api::ApiSpecData {
                name: "myspec".to_string(),
                sections: vec![tracey_api::SpecSection {
                    source_file: "spec.md".to_string(),
                    html: String::new(),
                    weight: 0,
                }],
                outline: vec![],
                head_injections: vec![],
            },
        }];
        let sidebar = vec![];

        let markup = landing_page(&None, &specs, &sidebar);
        let s = markup.into_string();

        assert!(s.contains("myspec"));
        assert!(s.contains("1/2 implemented"));
        assert!(s.contains("1/2 tested"));
        assert!(s.contains("myspec/index.html"));
    }

    #[test]
    fn test_enhance_spec_html_adds_coverage() {
        let html = r#"<div class="req-container" id="r-auth.login"><p>Users must log in</p></div>"#;
        let rule = make_rule("auth.login", vec![("src/auth.rs", 42)], vec![]);
        let rules: std::collections::HashMap<String, &ApiRule> =
            [("auth.login".to_string(), &rule)].into_iter().collect();

        let result = enhance_spec_html(html, &rules);

        assert!(result.contains("export-req-partial")); // has impl but no verify
        assert!(result.contains("src/auth.rs:42"));
        assert!(result.contains("🔍")); // missing verify
        assert!(!result.contains("⚠️")); // has impl
    }

    #[test]
    fn test_enhance_spec_html_uncovered() {
        let html = r#"<div class="req-container" id="r-auth.logout"><p>Logout</p></div>"#;
        let rule = make_rule("auth.logout", vec![], vec![]);
        let rules: std::collections::HashMap<String, &ApiRule> =
            [("auth.logout".to_string(), &rule)].into_iter().collect();

        let result = enhance_spec_html(html, &rules);

        assert!(result.contains("export-req-uncovered"));
        assert!(result.contains("⚠️"));
        assert!(result.contains("🔍"));
    }

    #[test]
    fn test_enhance_spec_html_fully_covered() {
        let html = r#"<div class="req-container" id="r-auth.both"><p>Both</p></div>"#;
        let rule = make_rule("auth.both", vec![("src/a.rs", 1)], vec![("tests/a.rs", 2)]);
        let rules: std::collections::HashMap<String, &ApiRule> =
            [("auth.both".to_string(), &rule)].into_iter().collect();

        let result = enhance_spec_html(html, &rules);

        assert!(result.contains("export-req-covered"));
        assert!(!result.contains("⚠️"));
        assert!(!result.contains("🔍"));
        assert!(result.contains("src/a.rs:1"));
        assert!(result.contains("tests/a.rs:2"));
    }

    #[test]
    fn test_strip_dashboard_badges() {
        let html = concat!(
            r#"<div class="req-container" id="r-auth.login" data-br="100-200">"#,
            r#"<div class="req-badges-left"><div class="req-badge-group">"#,
            r#"<button class="req-badge req-copy" data-req-id="auth.login">copy</button>"#,
            r#"<a class="req-badge req-id" href="/tracey/main/spec#r--auth.login" "#,
            r#"data-rule="auth.login" data-source-file="/home/user/code/spec.md" "#,
            r#"data-source-line="42">auth.login</a>"#,
            r#"</div></div>"#,
            r#"<p>Content here</p>"#,
            r#"</div>"#,
        );

        let result = strip_dashboard_elements(html);

        // Dashboard badges should be gone
        assert!(!result.contains("req-badges-left"));
        assert!(!result.contains("req-badge-group"));
        assert!(!result.contains("req-copy"));
        assert!(!result.contains("/tracey/main/spec"));

        // Data attributes should be removed
        assert!(!result.contains("data-br"));
        assert!(!result.contains("data-source-file"));
        assert!(!result.contains("data-source-line"));
        assert!(!result.contains("data-rule"));

        // Content should remain
        assert!(result.contains("Content here"));
        assert!(result.contains("req-container"));
    }

    #[test]
    fn test_remove_attribute() {
        let html = r#"<div class="foo" data-br="100-200" id="bar">text</div>"#;
        let result = remove_attribute(html, "data-br");
        assert_eq!(result, r#"<div class="foo" id="bar">text</div>"#);
    }

    #[test]
    fn test_remove_divs_with_class() {
        let html = concat!(
            r#"<div class="keep">before</div>"#,
            r#"<div class="remove-me"><p>nested</p></div>"#,
            r#"<div class="keep">after</div>"#,
        );
        let result = remove_divs_with_class(html, "remove-me");
        assert!(result.contains("before"));
        assert!(result.contains("after"));
        assert!(!result.contains("remove-me"));
        assert!(!result.contains("nested"));
    }
}
