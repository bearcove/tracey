//! Page-level templates for the static export.

use std::collections::HashMap;

use maud::{Markup, PreEscaped, html};
use tracey_api::{ApiSpecData, ApiSpecForward};

use super::components::page_shell;
use super::{SidebarSpec, SpecExportData};

// r[impl export.landing.readme]
// r[impl export.landing.spec-list]
pub(crate) fn landing_page(
    project_name: &str,
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

    page_shell(project_name, "index.html", sidebar_entries, content)
}

// r[impl export.spec-page.rendered-markdown]
// r[impl export.spec-page.anchors]
pub(crate) fn spec_page(
    spec_name: &str,
    impl_name: &str,
    section: &tracey_api::SpecSection,
    forward: &ApiSpecForward,
    _spec_data: &ApiSpecData,
    sidebar_entries: &[SidebarSpec],
    page_path: &str,
    project_root: &str,
    req_to_file: &HashMap<String, String>,
) -> Markup {
    let rules_by_id: HashMap<String, &tracey_api::ApiRule> = forward
        .rules
        .iter()
        .map(|r| (r.id.to_string(), r))
        .collect();

    let mut html_content = enhance_spec_html(&section.html, &rules_by_id);

    // r[impl export.output.link-rewrite]
    // Rewrite internal spec links: marq resolves relative .md links to absolute
    // paths like /project/docs/spec/filename/. We convert these back to relative
    // .html links for the static export.
    let spec_dir = std::path::Path::new(&section.source_file)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");
    let abs_spec_dir = if spec_dir.is_empty() {
        project_root.to_string()
    } else {
        format!("{project_root}/{spec_dir}")
    };
    html_content = rewrite_spec_links(&html_content, &abs_spec_dir);
    html_content = rewrite_md_links(&html_content);

    // r[impl export.spec-page.cross-links]
    // Rewrite dashboard requirement cross-links like /spec/impl/spec#r-req.id
    // to relative paths pointing to the correct HTML file.
    let dashboard_prefix = format!("/{spec_name}/{impl_name}/spec#r-");
    html_content =
        rewrite_req_cross_links(&html_content, &dashboard_prefix, req_to_file, page_path);

    let content = html! {
        .content.export-content.spec-page {
            .markdown {
                (PreEscaped(&html_content))
            }
        }
    };

    page_shell(spec_name, page_path, sidebar_entries, content)
}

/// Simple HTML redirect page for multi-file spec index.
pub(crate) fn redirect_page(target: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><meta http-equiv="refresh" content="0; url={target}"></head><body><a href="{target}">Redirect</a></body></html>"#
    )
}

/// Rewrite dashboard requirement cross-links to relative export links.
/// Dashboard links look like `href="/spec_name/impl_name/spec#r-req.id"` (with `--`
/// replacing dots in the anchor: `#r--req.id`).
/// We look up which file contains the requirement and produce a relative link.
fn rewrite_req_cross_links(
    html: &str,
    dashboard_prefix: &str,
    req_to_file: &HashMap<String, String>,
    current_page: &str,
) -> String {
    let mut result = String::with_capacity(html.len());
    let needle = "href=\"";
    let mut pos = 0;

    while let Some(rel) = html[pos..].find(needle) {
        let abs_start = pos + rel + needle.len();
        result.push_str(&html[pos..abs_start]);

        if let Some(end) = html[abs_start..].find('"') {
            let href = &html[abs_start..abs_start + end];

            // Check for dashboard prefix: /spec/impl/spec#r- or /spec/impl/spec#r--
            if let Some(rest) = href.strip_prefix(dashboard_prefix) {
                // The anchor uses -- to separate segments: r--req.id or r-req.id
                // Normalize: strip leading - (the dashboard uses #r--id format)
                let req_id_dashed = rest.trim_start_matches('-');
                // Convert dashes back to dots for lookup: req.id
                // Actually the dashboard keeps dots: #r--bam.record.flag_first
                let req_id = req_id_dashed;

                if let Some(target_file) = req_to_file.get(req_id) {
                    // Build relative link from current page to target file
                    let root = super::relative_root(current_page);
                    result.push_str(&format!("{root}/{target_file}#r-{req_id}"));
                    pos = abs_start + end;
                    continue;
                }
            }

            result.push_str(href);
            pos = abs_start + end;
        } else {
            pos = abs_start;
        }
    }

    result.push_str(&html[pos..]);
    result
}

/// Rewrite absolute spec links back to relative HTML links.
/// marq resolves `[text](references.md)` to `href="/project/docs/spec/references/"`.
/// We detect links starting with the spec directory and convert to `./stem.html`.
fn rewrite_spec_links(html: &str, abs_spec_dir: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let needle = "href=\"";
    let mut pos = 0;

    // Normalize: ensure the prefix ends with /
    let prefix = if abs_spec_dir.ends_with('/') {
        abs_spec_dir.to_string()
    } else {
        format!("{abs_spec_dir}/")
    };

    while let Some(rel) = html[pos..].find(needle) {
        let abs_start = pos + rel + needle.len();
        result.push_str(&html[pos..abs_start]);

        if let Some(end) = html[abs_start..].find('"') {
            let href = &html[abs_start..abs_start + end];

            if let Some(rest) = href.strip_prefix(&prefix) {
                // Extract the stem — it's the path after the spec dir
                // Could be "references/" or "99-references/" or "references/#section"
                let (file_part, fragment) = if let Some(hash) = rest.find('#') {
                    (&rest[..hash], Some(&rest[hash..]))
                } else {
                    (rest, None)
                };

                // Strip trailing slash
                let stem = file_part.trim_end_matches('/');

                if !stem.is_empty() {
                    result.push_str(&format!("./{stem}.html"));
                    if let Some(frag) = fragment {
                        result.push_str(frag);
                    }
                    pos = abs_start + end;
                    continue;
                }
            }

            // Not a spec link, keep as-is
            result.push_str(href);
            pos = abs_start + end;
        } else {
            pos = abs_start;
        }
    }

    result.push_str(&html[pos..]);
    result
}

/// Rewrite `.md` links to `.html` in rendered HTML.
/// `href="./other-file.md"` → `href="./other-file.html"`
/// `href="./other-file.md#section"` → `href="./other-file.html#section"`
/// External links and non-`.md` links are left unchanged.
// r[impl export.output.link-rewrite]
fn rewrite_md_links(html: &str) -> String {
    // Replace .md" and .md# patterns in href attributes
    html.replace(".md\"", ".html\"").replace(".md#", ".html#")
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

        let markup = landing_page("myproject", &readme, &specs, &sidebar);
        let s = markup.into_string();

        assert!(s.contains("My Project"));
        assert!(s.contains("A great project"));
        assert!(s.contains("Specifications"));
    }

    #[test]
    fn test_landing_page_without_readme() {
        let specs = vec![];
        let sidebar = vec![];

        let markup = landing_page("myproject", &None, &specs, &sidebar);
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

        let markup = landing_page("myproject", &None, &specs, &sidebar);
        let s = markup.into_string();

        assert!(s.contains("myspec"));
        assert!(s.contains("1/2 implemented"));
        assert!(s.contains("1/2 tested"));
        assert!(s.contains("myspec/index.html"));
    }

    #[test]
    fn test_enhance_spec_html_adds_status_and_badge() {
        let html = r#"<div class="req-container" id="r-auth.login"><p>Users must log in</p></div>"#;
        let rule = make_rule("auth.login", vec![("src/auth.rs", 42)], vec![]);
        let rules: std::collections::HashMap<String, &ApiRule> =
            [("auth.login".to_string(), &rule)].into_iter().collect();

        let result = enhance_spec_html(html, &rules);

        assert!(result.contains("export-req-partial")); // has impl but no verify
        assert!(result.contains("export-req-id")); // badge injected
        assert!(result.contains("auth.login")); // req ID shown
    }

    #[test]
    fn test_enhance_spec_html_uncovered() {
        let html = r#"<div class="req-container" id="r-auth.logout"><p>Logout</p></div>"#;
        let rule = make_rule("auth.logout", vec![], vec![]);
        let rules: std::collections::HashMap<String, &ApiRule> =
            [("auth.logout".to_string(), &rule)].into_iter().collect();

        let result = enhance_spec_html(html, &rules);

        assert!(result.contains("export-req-uncovered"));
        assert!(result.contains("export-req-id"));
    }

    #[test]
    fn test_enhance_spec_html_fully_covered() {
        let html = r#"<div class="req-container" id="r-auth.both"><p>Both</p></div>"#;
        let rule = make_rule("auth.both", vec![("src/a.rs", 1)], vec![("tests/a.rs", 2)]);
        let rules: std::collections::HashMap<String, &ApiRule> =
            [("auth.both".to_string(), &rule)].into_iter().collect();

        let result = enhance_spec_html(html, &rules);

        assert!(result.contains("export-req-covered"));
        assert!(result.contains("export-req-id"));
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
    fn test_rewrite_md_links() {
        let html = concat!(
            r#"<a href="./99-references.md">References</a> "#,
            r#"<a href="./intro.md#section">Intro</a> "#,
            r#"<a href="https://example.com">External</a>"#,
        );
        let result = rewrite_md_links(html);

        assert!(result.contains("99-references.html"));
        assert!(result.contains("intro.html#section"));
        assert!(result.contains("https://example.com")); // external unchanged
        assert!(!result.contains(".md"));
    }

    #[test]
    fn test_rewrite_spec_links() {
        let abs_dir = "/home/user/project/docs/spec";

        // marq turns [text](references.md) into href="/home/user/project/docs/spec/references/"
        let html = concat!(
            r#"<a href="/home/user/project/docs/spec/references/">References</a> "#,
            r#"<a href="/home/user/project/docs/spec/99-references/">See refs</a> "#,
            r#"<a href="/home/user/project/docs/spec/intro/#section">Intro</a> "#,
            r#"<a href="https://example.com">External</a> "#,
            r#"<a href="/other/path/">Other</a>"#,
        );

        let result = rewrite_spec_links(html, abs_dir);

        assert!(
            result.contains("./references.html"),
            "should rewrite to relative .html: {result}"
        );
        assert!(result.contains("./99-references.html"));
        assert!(result.contains("./intro.html#section"));
        assert!(result.contains("https://example.com")); // external unchanged
        assert!(result.contains("/other/path/")); // non-spec path unchanged
    }

    #[test]
    fn test_rewrite_spec_links_with_trailing_slash() {
        let abs_dir = "/project/docs/spec/";
        let html = r#"<a href="/project/docs/spec/chapter/">Chapter</a>"#;
        let result = rewrite_spec_links(html, abs_dir);
        assert!(result.contains("./chapter.html"));
    }

    #[test]
    fn test_rewrite_req_cross_links() {
        let mut req_map = HashMap::new();
        req_map.insert(
            "bam.record.flag_first".to_string(),
            "seqair/2-bam-3-2-record.html".to_string(),
        );
        req_map.insert("auth.login".to_string(), "myspec/index.html".to_string());

        // Dashboard-style link: /spec/impl/spec#r--req.id
        let html = concat!(
            r#"<a href="/seqair/rust/spec#r--bam.record.flag_first">flag</a> "#,
            r#"<a href="/myspec/main/spec#r--auth.login">login</a> "#,
            r#"<a href="https://example.com">ext</a>"#,
        );

        let result = rewrite_req_cross_links(
            html,
            "/seqair/rust/spec#r-",
            &req_map,
            "seqair/0-general.html",
        );

        assert!(
            result.contains("2-bam-3-2-record.html#r-bam.record.flag_first"),
            "should rewrite cross-link: {result}"
        );
        // The myspec link has a different prefix, so it's not rewritten by this call
        assert!(result.contains("/myspec/main/spec#r--auth.login"));
        assert!(result.contains("https://example.com"));
    }

    #[test]
    fn test_rewrite_req_cross_links_same_file() {
        let mut req_map = HashMap::new();
        req_map.insert(
            "intro.overview".to_string(),
            "myspec/index.html".to_string(),
        );

        let html = r#"<a href="/myspec/main/spec#r--intro.overview">overview</a>"#;
        let result =
            rewrite_req_cross_links(html, "/myspec/main/spec#r-", &req_map, "myspec/index.html");

        // Should link to same file with anchor
        assert!(
            result.contains("myspec/index.html#r-intro.overview"),
            "should resolve: {result}"
        );
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
