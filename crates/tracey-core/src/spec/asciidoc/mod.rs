//! AsciiDoc spec backend — asciidork-parser based.
//!
//! Two-pass implementation:
//! 1. Walk the asciidork AST to extract requirements, headings, and inline code spans.
//! 2. Convert to HTML via `asciidork_dr_html_backend::convert()`, then post-process
//!    to inject `<div class="req-container">` wrappers and fix heading IDs.

mod ast_walk;

use std::ops::Range;

use asciidork_core::JobSettings;
use asciidork_parser::prelude::*;
use bumpalo::Bump;
use marq::{ReqDefinition, SourceSpan};

use super::{
    BadgeFn, NoConfig, REQ_CONTAINER_CLOSE, RenderInput, RenderOutput, RenderedSection,
    SlugAllocator, SpecBackend, SpecDoc, SpecFormat, req_anchor_id,
};

/// AsciiDoc backend.
#[derive(Default)]
pub struct Asciidoc;

#[async_trait::async_trait]
impl SpecBackend for Asciidoc {
    type Config = NoConfig;

    fn format(&self) -> SpecFormat {
        SpecFormat::AsciiDoc
    }
    fn name(&self) -> &'static str {
        "asciidoc"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["adoc", "asciidoc"]
    }

    async fn parse(&self, content: &str) -> eyre::Result<SpecDoc> {
        let req_renderer = |req: &ReqDefinition| {
            let anchor = req_anchor_id(&req.id.to_string());
            let open = format!(
                r#"<div class="req-container req-uncovered" id="{anchor}" data-br="{start}-{end}"><div class="req-content">"#,
                anchor = html_escape(&anchor),
                start = req.span.offset,
                end = req.span.offset + req.span.length,
            );
            (open, REQ_CONTAINER_CLOSE.to_string())
        };
        parse_sync(content, &req_renderer, None)
    }

    fn parse_weight(&self, content: &str) -> i32 {
        // YAML/TOML frontmatter first
        if let Ok((fm, _)) = marq::parse_frontmatter(content) && fm.weight != 0 {
            return fm.weight;
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

    fn extract_marker_prefix(&self, content: &str, span: SourceSpan) -> Option<String> {
        let start = span.offset;
        let end = start.checked_add(span.length)?;
        let marker = content.get(start..end)?;
        let bracket = marker.find('[')?;
        let prefix = marker[..bracket].trim();
        if prefix.is_empty() {
            return None;
        }
        Some(prefix.to_string())
    }

    fn id_range_in_marker(&self, marker: &str) -> eyre::Result<Range<usize>> {
        let open = marker
            .find('[')
            .ok_or_else(|| eyre::eyre!("malformed asciidoc marker: {}", marker))?;
        let close = marker
            .rfind(']')
            .ok_or_else(|| eyre::eyre!("malformed asciidoc marker: {}", marker))?;
        Ok(open + 1..close)
    }

    fn diff_inline(&self, old: &str, new: &str) -> Option<String> {
        Some(marq::diff_markdown_inline(old, new))
    }

    async fn render_html(
        &self,
        input: RenderInput<'_>,
        _cfg: &NoConfig,
    ) -> eyre::Result<RenderOutput> {
        let RenderInput {
            sources,
            root,
            badge_for,
            slugs,
            deps: _,
            ..
        } = input;

        let mut sections = Vec::with_capacity(sources.len());
        let mut first_err: Option<eyre::Error> = None;

        for (idx, src) in sources.iter().enumerate() {
            let abs_source = root.join(src.path).display().to_string();
            let badge_for_clone: BadgeFn = badge_for.clone();
            let abs_source_clone = abs_source.clone();
            let req_renderer = move |req: &ReqDefinition| {
                let open = badge_for_clone(req, &abs_source_clone);
                (open, REQ_CONTAINER_CLOSE.to_string())
            };
            match parse_sync(src.content, &req_renderer, Some(slugs)) {
                Ok(doc) => sections.push(RenderedSection {
                    source_idx: idx,
                    html: doc.html,
                    elements: doc.elements,
                    head_injections: doc.head_injections,
                }),
                Err(e) if first_err.is_none() => first_err = Some(e),
                Err(_) => {}
            }
        }

        if let Some(e) = first_err {
            return Err(e);
        }
        Ok(RenderOutput { sections })
    }
}

fn parse_sync(
    content: &str,
    req_renderer: &dyn Fn(&ReqDefinition) -> (String, String),
    alloc: Option<&mut SlugAllocator>,
) -> eyre::Result<SpecDoc> {
    let arena = Bump::new();
    let mut parser = Parser::from_str(content, SourceFile::Tmp, &arena);
    // Non-strict: cross-file xrefs are unresolvable during per-file parsing
    // but valid at render time when all spec files share one HTML page.
    parser.apply_job_settings(JobSettings {
        strict: false,
        ..JobSettings::default()
    });
    let parsed = parser
        .parse()
        .map_err(|e| eyre::eyre!("AsciiDoc parse error: {:?}", e))?;

    let mut owned_alloc = SlugAllocator::default();
    let alloc_ref = alloc.unwrap_or(&mut owned_alloc);

    let walk = ast_walk::walk(&parsed.document, content, alloc_ref)?;

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
        source_map: Default::default(),
    })
}

/// Extract the inner HTML of `<div id="content">` from full asciidork output.
fn extract_content_html(full_html: &str) -> &str {
    let content_marker = r#"<div id="content">"#;
    let footer_marker = r#"<div id="footer">"#;

    let Some(content_start) = full_html.find(content_marker) else {
        return full_html;
    };
    let inner_start = content_start + content_marker.len();

    let Some(footer_start) = full_html[inner_start..].find(footer_marker) else {
        if let Some(body_end) = full_html[inner_start..].find("</body>") {
            let raw = &full_html[inner_start..inner_start + body_end];
            return raw.strip_suffix("</div>").unwrap_or(raw);
        }
        return &full_html[inner_start..];
    };

    let before_footer = &full_html[inner_start..inner_start + footer_start];
    before_footer.strip_suffix("</div>").unwrap_or(before_footer)
}

fn post_process_html(
    content_html: &str,
    source: &str,
    walk: &ast_walk::WalkResult,
    req_renderer: &dyn Fn(&ReqDefinition) -> (String, String),
) -> String {
    let mut html = content_html.to_string();

    for req in &walk.reqs {
        let marker_text = source
            .get(req.marker_span.offset..req.marker_span.offset + req.marker_span.length)
            .unwrap_or("");
        if !marker_text.is_empty() {
            html = replace_req_paragraph(&html, req, marker_text, req_renderer);
        }
    }

    for (adoc_id, our_slug) in &walk.section_id_map {
        let from_id = format!(r#" id="{}""#, adoc_id);
        let to_id = format!(r#" id="{}""#, our_slug);
        html = html.replace(&from_id, &to_id);

        let from_href = format!(" href=\"#{}\"", adoc_id);
        let to_href = format!(" href=\"#{}\"", our_slug);
        html = html.replace(&from_href, &to_href);
    }

    html
}

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
