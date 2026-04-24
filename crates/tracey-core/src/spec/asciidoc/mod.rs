//! AsciiDoc spec backend — asciidork-parser based.
//!
//! Two-pass implementation:
//! 1. Walk the asciidork AST to extract requirements, headings, and inline code spans.
//! 2. Convert to HTML via `asciidork_dr_html_backend::convert()`, then post-process
//!    to inject `<div class="req-container">` wrappers and fix heading IDs.
//!
//! The public API matches the hand-rolled predecessor exactly — only the internals change.

#[cfg(feature = "asciidoc-spec")]
mod ast_walk;

use std::collections::HashSet;
use std::path::PathBuf;

use marq::{ReqDefinition, SourceSpan};

use super::{SlugAllocator, SpecDoc};
#[cfg(feature = "asciidoc-spec")]
use super::req_anchor_id;

// ============================================================================
// Public context for dashboard rendering
// ============================================================================

/// Context for coverage-aware AsciiDoc rendering.
pub struct RenderCtx<'a> {
    /// Closure returning `(open_html, close_html)` for each requirement container.
    pub badge_for: &'a (dyn Fn(&ReqDefinition) -> (String, String) + Sync),
}

// ============================================================================
// Public entry points
// ============================================================================

// r[impl asciidoc.html.div]
// r[impl asciidoc.html.anchor]
pub(super) async fn parse(content: &str) -> eyre::Result<SpecDoc> {
    #[cfg(feature = "asciidoc-spec")]
    {
        let req_renderer = |req: &ReqDefinition| {
            let anchor = req_anchor_id(&req.id.to_string());
            let open = format!(
                r#"<div class="req-container req-uncovered" id="{anchor}" data-br="{start}-{end}"><div class="req-content">"#,
                anchor = html_escape(&anchor),
                start = req.span.offset,
                end = req.span.offset + req.span.length,
            );
            (open, "</div>\n</div>".to_string())
        };
        parse_sync(content, &req_renderer, None)
    }
    #[cfg(not(feature = "asciidoc-spec"))]
    {
        Ok(placeholder_doc(content))
    }
}

// r[impl asciidoc.html.link]
pub async fn render_display(
    content: &str,
    _source_path: &std::path::Path,
    _ctx: &RenderCtx<'_>,
    _alloc: &mut SlugAllocator,
    _deps: &mut HashSet<PathBuf>,
) -> eyre::Result<SpecDoc> {
    #[cfg(feature = "asciidoc-spec")]
    {
        parse_sync(content, _ctx.badge_for, Some(_alloc))
    }
    #[cfg(not(feature = "asciidoc-spec"))]
    {
        Ok(placeholder_doc(content))
    }
}

// ============================================================================
// Per-format helpers (unchanged from predecessor)
// ============================================================================

pub(super) fn diff_inline(old: &str, new: &str) -> Option<String> {
    Some(marq::diff_markdown_inline(old, new))
}

// r[impl asciidoc.frontmatter.attributes]
pub(super) fn parse_weight(content: &str) -> i32 {
    // YAML/TOML frontmatter first
    if let Ok((fm, _)) = marq::parse_frontmatter(content) {
        if fm.weight != 0 {
            return fm.weight;
        }
    }
    // AsciiDoc `:weight: N` document attribute (line scan, pre-title only)
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('=') {
            break;
        }
        if let Some(w) = line
            .strip_prefix(":weight:")
            .and_then(|r| r.trim().parse::<i32>().ok())
        {
            return w;
        }
    }
    0
}

pub(super) fn extract_marker_prefix(content: &str, span: SourceSpan) -> Option<String> {
    super::common_extract_marker_prefix(content, span)
}

pub(super) fn id_range_in_marker(marker_str: &str) -> eyre::Result<std::ops::Range<usize>> {
    super::common_id_range_in_marker(marker_str)
}

// ============================================================================
// Core two-pass implementation
// ============================================================================

#[cfg(feature = "asciidoc-spec")]
fn parse_sync(
    content: &str,
    req_renderer: &dyn Fn(&ReqDefinition) -> (String, String),
    alloc: Option<&mut SlugAllocator>,
) -> eyre::Result<SpecDoc> {
    use asciidork_core::JobSettings;
    use asciidork_parser::prelude::*;
    use bumpalo::Bump;

    let arena = Bump::new();
    let mut parser = Parser::from_str(content, SourceFile::Tmp, &arena);
    // Non-strict: cross-file xrefs (<<anchor-in-other-file>>) are unresolvable
    // during per-file parsing but valid at page render time since all spec
    // files share one HTML page. Suppress parse errors and let them render
    // as <a href="#anchor"> links instead.
    parser.apply_job_settings(JobSettings { strict: false, ..JobSettings::default() });
    let parsed = parser
        .parse()
        .map_err(|e| eyre::eyre!("AsciiDoc parse error: {:?}", e))?;

    let mut owned_alloc = SlugAllocator::new();
    let alloc_ref = alloc.unwrap_or(&mut owned_alloc);

    // Pass 1: walk AST for extraction
    let walk = ast_walk::walk(&parsed.document, content, alloc_ref)?;

    // Pass 2: render HTML and post-process
    let html = asciidork_dr_html_backend::convert(parsed.document)
        .map_err(|e| eyre::eyre!("AsciiDoc HTML render error: {:?}", e))?;

    let content_html = extract_content_html(&html);
    let content_html = post_process_html(content_html, content, &walk, req_renderer);

    Ok(marq::Document {
        raw_metadata: None,
        metadata_format: None,
        frontmatter: None,
        html: content_html,
        headings: walk.headings,
        reqs: walk.reqs,
        code_samples: Vec::new(),
        elements: walk.elements,
        head_injections: Vec::new(),
        inline_code_spans: walk.inline_code_spans,
    })
}

/// Extract the inner HTML of `<div id="content">` from the full asciidork output.
///
/// Returns the content between `<div id="content">` and `<div id="footer">`,
/// or the full HTML if the pattern is not found.
#[cfg(feature = "asciidoc-spec")]
fn extract_content_html(full_html: &str) -> &str {
    let content_marker = r#"<div id="content">"#;
    let footer_marker = r#"<div id="footer">"#;

    let Some(content_start) = full_html.find(content_marker) else {
        return full_html;
    };
    let inner_start = content_start + content_marker.len();

    let Some(footer_start) = full_html[inner_start..].find(footer_marker) else {
        // No footer — take everything after content marker until </body>
        if let Some(body_end) = full_html[inner_start..].find("</body>") {
            let raw = &full_html[inner_start..inner_start + body_end];
            return raw.strip_suffix("</div>").unwrap_or(raw);
        }
        return &full_html[inner_start..];
    };

    // Strip exactly the one trailing </div> that closes <div id="content">
    let before_footer = &full_html[inner_start..inner_start + footer_start];
    before_footer.strip_suffix("</div>").unwrap_or(before_footer)
}

/// Post-process the asciidork HTML:
/// 1. Replace req paragraphs with req-container divs.
/// 2. Replace asciidork heading IDs with our SlugAllocator-assigned slugs.
#[cfg(feature = "asciidoc-spec")]
fn post_process_html(
    content_html: &str,
    source: &str,
    walk: &ast_walk::WalkResult,
    req_renderer: &dyn Fn(&ReqDefinition) -> (String, String),
) -> String {
    let mut html = content_html.to_string();

    // Step 1: inject req-container wrappers
    for req in &walk.reqs {
        let marker_text = source
            .get(req.marker_span.offset..req.marker_span.offset + req.marker_span.length)
            .unwrap_or("");
        if !marker_text.is_empty() {
            html = replace_req_paragraph(&html, req, marker_text, req_renderer);
        }
    }

    // Step 2: replace asciidork section IDs with our slugs
    for (adoc_id, our_slug) in &walk.section_id_map {
        // Replace id="ADOC_ID" with id="OUR_SLUG"
        let from_id = format!(r#" id="{}""#, adoc_id);
        let to_id = format!(r#" id="{}""#, our_slug);
        html = html.replace(&from_id, &to_id);

        // Replace href="#ADOC_ID" with href="#OUR_SLUG" (for sectanchors etc.)
        let from_href = format!(" href=\"#{}\"", adoc_id);
        let to_href = format!(" href=\"#{}\"", our_slug);
        html = html.replace(&from_href, &to_href);
    }

    html
}

/// Replace a single requirement paragraph in the HTML with its req-container HTML.
///
/// Finds `<div class="paragraph"><p>MARKER_TEXT` and replaces the paragraph div
/// with the output of `req_renderer(req)`.
#[cfg(feature = "asciidoc-spec")]
fn replace_req_paragraph(
    html: &str,
    req: &ReqDefinition,
    marker_text: &str,
    req_renderer: &dyn Fn(&ReqDefinition) -> (String, String),
) -> String {
    let search = format!(r#"<div class="paragraph"><p>{}"#, marker_text);

    let Some(div_start) = html.find(&search) else {
        return html.to_string();
    };

    let after_marker = &html[div_start + search.len()..];

    let Some(body_end) = after_marker.find("</p></div>") else {
        return html.to_string();
    };

    let body_in_html = &after_marker[..body_end];
    // Strip leading space (the space asciidork puts between marker and body)
    let body_in_html = body_in_html.strip_prefix(' ').unwrap_or(body_in_html);

    let div_end = div_start + search.len() + body_end + "</p></div>".len();

    let (open_html, close_html) = req_renderer(req);
    let replacement = if body_in_html.is_empty() {
        format!("{open_html}{close_html}")
    } else {
        format!("{open_html}<p>{body_in_html}</p>\n{close_html}")
    };

    let mut result = String::with_capacity(html.len());
    result.push_str(&html[..div_start]);
    result.push_str(&replacement);
    result.push_str(&html[div_end..]);
    result
}

// ============================================================================
// Feature-off stub
// ============================================================================

#[cfg(not(feature = "asciidoc-spec"))]
fn placeholder_doc(content: &str) -> SpecDoc {
    marq::Document {
        raw_metadata: None,
        metadata_format: None,
        frontmatter: None,
        html: format!(
            r#"<pre class="asciidoc-placeholder">{}</pre>"#,
            html_escape(content)
        ),
        headings: Vec::new(),
        reqs: Vec::new(),
        code_samples: Vec::new(),
        elements: Vec::new(),
        head_injections: Vec::new(),
        inline_code_spans: Vec::new(),
    }
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}
