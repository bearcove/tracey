//! StrictDoc (`.sdoc`) spec backend.
//!
//! Bridges `strictdoc_parser` onto [`SpecDoc`] so the format-agnostic
//! extraction and rendering paths handle `.sdoc` exactly like `.md` / `.typ`.

use std::ops::Range;

use marq::{DocElement, Heading, ReqDefinition, ReqMetadata, SourceSpan, parse_rule_id, slugify};
use strictdoc_parser::DocumentChild;

use super::{
    NoConfig, RenderInput, RenderOutput, RenderedSection, SpecBackend, SpecDoc, SpecFormat,
    req_anchor_id,
};

/// Synthetic marker prefix for `.sdoc` requirements.
///
/// `.sdoc` has no `r[...]`-style marker; this is the value `@relation(...)`
/// source markers must agree on for matching.
pub const SDOC_PREFIX: &str = "r";

/// StrictDoc backend.
pub struct Sdoc;

#[async_trait::async_trait]
impl SpecBackend for Sdoc {
    type Config = NoConfig;

    fn format(&self) -> SpecFormat {
        SpecFormat::Sdoc
    }
    fn name(&self) -> &'static str {
        "sdoc"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["sdoc"]
    }

    async fn parse(&self, content: &str) -> eyre::Result<SpecDoc> {
        parse(content).await
    }

    /// `.sdoc` has no inline marker syntax; the prefix is fixed.
    fn extract_marker_prefix(&self, _content: &str, _span: SourceSpan) -> Option<String> {
        Some(SDOC_PREFIX.to_owned())
    }

    /// `.sdoc` UIDs aren't bracketed markers, so `tracey bump` cannot rewrite
    /// them in place. Callers should treat sdoc specs as version-immutable or
    /// use StrictDoc's own tooling.
    fn id_range_in_marker(&self, _marker: &str) -> eyre::Result<Range<usize>> {
        Err(eyre::eyre!(
            "sdoc UIDs are not rewritable inline; use StrictDoc tooling for version bumps"
        ))
    }

    /// No format-aware diff; the daemon falls back to a plain text diff.
    fn diff_inline(&self, _old: &str, _new: &str) -> Option<String> {
        None
    }

    async fn render_html(
        &self,
        input: RenderInput<'_>,
        _cfg: &NoConfig,
    ) -> eyre::Result<RenderOutput> {
        let RenderInput {
            sources,
            badge_for,
            slugs,
            ..
        } = input;

        let mut sections = Vec::with_capacity(sources.len());
        for (idx, src) in sources.iter().enumerate() {
            let doc = strictdoc_parser::parse(src.content).map_err(|e| {
                eyre::eyre!("Failed to parse {} as StrictDoc: {}", src.path.display(), e)
            })?;
            let markup_is_markdown = is_markdown_markup(&doc);
            let source_path = src.path.to_string_lossy();

            let mut html = String::new();
            let mut elements = Vec::new();
            render_body(
                &doc.body,
                src.content,
                &source_path,
                markup_is_markdown,
                1,
                &badge_for,
                slugs,
                &mut html,
                &mut elements,
            )
            .await?;

            sections.push(RenderedSection {
                source_idx: idx,
                html,
                elements,
                head_injections: vec![],
            });
        }
        Ok(RenderOutput {
            sections,
            deps: vec![],
        })
    }
}

/// Walk `body` in document order, emitting [`Heading`] / [`ReqDefinition`]
/// elements and producing a [`SpecDoc`].
///
/// Per-requirement HTML is the rendered `STATEMENT` (via marq when
/// `OPTIONS: MARKUP: Markdown`, otherwise an escaped `<p>`). The doc-level
/// `html` is left empty — display HTML comes from [`Sdoc::render_html`].
pub(super) async fn parse(content: &str) -> eyre::Result<SpecDoc> {
    let doc = strictdoc_parser::parse(content)
        .map_err(|e| eyre::eyre!("Failed to parse StrictDoc: {}", e))?;
    let markup_is_markdown = is_markdown_markup(&doc);

    let mut reqs = Vec::new();
    let mut headings = Vec::new();
    let mut elements = Vec::new();
    walk_body(
        &doc.body,
        content,
        markup_is_markdown,
        1,
        &mut reqs,
        &mut headings,
        &mut elements,
    )
    .await;

    Ok(SpecDoc {
        raw_metadata: None,
        metadata_format: None,
        frontmatter: None,
        html: String::new(),
        headings,
        reqs,
        code_samples: vec![],
        elements,
        head_injections: vec![],
        inline_code_spans: vec![],
    })
}

fn is_markdown_markup(doc: &strictdoc_parser::Document) -> bool {
    doc.options
        .get("MARKUP")
        .is_some_and(|v| v.eq_ignore_ascii_case("Markdown"))
}

/// Recursive document-order walk used by [`parse`].
fn walk_body<'a>(
    body: &'a [DocumentChild],
    content: &'a str,
    markup_is_markdown: bool,
    depth: u8,
    reqs: &'a mut Vec<ReqDefinition>,
    headings: &'a mut Vec<Heading>,
    elements: &'a mut Vec<DocElement>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        for child in body {
            match child {
                DocumentChild::Section(s) => {
                    let h = Heading {
                        title: s.title.clone(),
                        id: slugify(&s.title),
                        level: depth,
                        line: s.span.line as usize,
                    };
                    headings.push(h.clone());
                    elements.push(DocElement::Heading(h));
                    walk_body(
                        &s.children,
                        content,
                        markup_is_markdown,
                        depth.saturating_add(1),
                        reqs,
                        headings,
                        elements,
                    )
                    .await;
                }
                DocumentChild::Requirement(r) => {
                    if let Some(def) = build_req(r, content, markup_is_markdown).await {
                        elements.push(DocElement::Req(def.clone()));
                        reqs.push(def);
                    }
                }
            }
        }
    })
}

/// Build a [`ReqDefinition`] from a `[REQUIREMENT]` block.
///
/// Returns `None` when the block lacks a `UID:` field or the UID does not
/// parse as a tracey rule id (matches the original behaviour: skip + warn).
async fn build_req(
    req: &strictdoc_parser::Requirement,
    content: &str,
    markup_is_markdown: bool,
) -> Option<ReqDefinition> {
    let uid = req.field_text("UID")?;
    let Some(rule_id) = parse_rule_id(uid) else {
        eprintln!("Warning: invalid StrictDoc UID '{uid}', skipping requirement");
        return None;
    };

    let req_span = req.span;
    let raw = content
        .get(req_span.start..req_span.end)
        .unwrap_or("")
        .to_string();

    let html = match req.field_text("STATEMENT") {
        Some(stmt) if markup_is_markdown => marq::render(stmt, &marq::RenderOptions::default())
            .await
            .map(|d| d.html)
            .unwrap_or_else(|_| wrap_paragraph(stmt)),
        Some(stmt) => wrap_paragraph(stmt),
        None => String::new(),
    };

    let length = req_span.end.saturating_sub(req_span.start);
    Some(ReqDefinition {
        id: rule_id,
        anchor_id: req_anchor_id(uid),
        marker_span: SourceSpan {
            offset: req_span.start,
            length: 0,
        },
        span: SourceSpan {
            offset: req_span.start,
            length,
        },
        line: req_span.line as usize,
        metadata: ReqMetadata::default(),
        raw,
        html,
    })
}

/// Recursive document-order walk used by [`Sdoc::render_html`] — emits HTML and
/// collects elements (with cross-file unique heading slugs).
#[allow(clippy::too_many_arguments)]
fn render_body<'a>(
    body: &'a [DocumentChild],
    content: &'a str,
    source_path: &'a str,
    markup_is_markdown: bool,
    depth: u8,
    badge_for: &'a super::BadgeFn,
    slugs: &'a mut super::SlugAllocator,
    html: &'a mut String,
    elements: &'a mut Vec<DocElement>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = eyre::Result<()>> + Send + 'a>> {
    Box::pin(async move {
        for child in body {
            match child {
                DocumentChild::Section(s) => {
                    let level = depth.min(6);
                    let id = slugs.alloc(&slugify(&s.title));
                    let h = Heading {
                        title: s.title.clone(),
                        id: id.clone(),
                        level,
                        line: s.span.line as usize,
                    };
                    html.push_str(&format!(
                        "<h{level} id=\"{id}\">{}</h{level}>\n",
                        html_escape::encode_text(&s.title)
                    ));
                    elements.push(DocElement::Heading(h));
                    render_body(
                        &s.children,
                        content,
                        source_path,
                        markup_is_markdown,
                        depth.saturating_add(1),
                        badge_for,
                        slugs,
                        html,
                        elements,
                    )
                    .await?;
                }
                DocumentChild::Requirement(r) => {
                    if let Some(def) = build_req(r, content, markup_is_markdown).await {
                        let (open, close) = badge_for(&def, source_path);
                        html.push_str(&open);
                        html.push_str(&def.html);
                        html.push_str(&close);
                        html.push('\n');
                        elements.push(DocElement::Req(def));
                    }
                }
            }
        }
        Ok(())
    })
}

fn wrap_paragraph(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    out.push_str("<p>");
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out.push_str("</p>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parse_yields_reqs_and_per_req_html() {
        let content = "[DOCUMENT]\nTITLE: T\n\nOPTIONS:\n  MARKUP: Markdown\n\n\
[REQUIREMENT]\nUID: BR-001\nSTATEMENT: The bridge **must** connect.\n";
        let doc = parse(content).await.unwrap();
        assert_eq!(doc.reqs.len(), 1);
        assert_eq!(doc.reqs[0].id.to_string(), "BR-001");
        assert!(doc.reqs[0].html.contains("<strong>"));
        assert_eq!(doc.elements.len(), 1);
    }

    #[tokio::test]
    async fn parse_emits_section_headings_in_order() {
        let content = "[DOCUMENT]\nTITLE: T\n\n\
[[SECTION]]\nTITLE: Outer\n\n\
[REQUIREMENT]\nUID: R-1\nSTATEMENT: a\n\n\
[[/SECTION]]\n";
        let doc = parse(content).await.unwrap();
        assert_eq!(doc.headings.len(), 1);
        assert_eq!(doc.headings[0].title, "Outer");
        assert_eq!(doc.headings[0].level, 1);
        // elements: Heading then Req
        assert!(matches!(doc.elements[0], DocElement::Heading(_)));
        assert!(matches!(doc.elements[1], DocElement::Req(_)));
    }

    #[test]
    fn extract_marker_prefix_is_fixed() {
        let span = SourceSpan {
            offset: 0,
            length: 0,
        };
        assert_eq!(
            Sdoc.extract_marker_prefix("anything", span).as_deref(),
            Some("r")
        );
    }
}
