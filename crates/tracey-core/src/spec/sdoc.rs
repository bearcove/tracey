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
#[derive(Default)]
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

    /// No format-aware diff; the daemon falls back to a `~~old~~ / current`
    /// markdown rendering so stale hovers still show both texts.
    fn diff_inline(&self, _old: &str, _new: &str) -> Option<String> {
        None
    }

    async fn render_html(
        &self,
        input: RenderInput<'_>,
        _cfg: &NoConfig,
    ) -> eyre::Result<RenderOutput> {
        let RenderInput {
            root,
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
            let source_path = root.join(src.path).display().to_string();

            let mut html = String::new();
            let mut elements = Vec::new();
            let mut state = RenderState {
                html: &mut html,
                badge_for: &badge_for,
                source_path: &source_path,
                slugs: &mut *slugs,
            };
            walk_body(
                &doc.body,
                src.content,
                markup_is_markdown,
                1,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut elements,
                Some(&mut state),
            )
            .await;

            sections.push(RenderedSection {
                source_idx: idx,
                html,
                elements,
                head_injections: vec![],
            });
        }
        Ok(RenderOutput { sections })
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
        None,
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
        source_map: Default::default(),
    })
}

fn is_markdown_markup(doc: &strictdoc_parser::Document) -> bool {
    doc.options
        .get("MARKUP")
        .is_some_and(|v| v.eq_ignore_ascii_case("Markdown"))
}

/// HTML-emission state for the optional render pass of [`walk_body`].
struct RenderState<'a> {
    html: &'a mut String,
    badge_for: &'a super::BadgeFn,
    source_path: &'a str,
    slugs: &'a mut super::SlugAllocator,
}

/// Recursive document-order walk shared by [`parse`] and [`Sdoc::render_html`].
///
/// Always accumulates `reqs` / `headings` / `elements`. When `render` is
/// `Some`, additionally emits HTML and threads heading slugs through the
/// cross-file [`SlugAllocator`](super::SlugAllocator); when `None`, heading
/// ids are the raw [`slugify`] result.
#[allow(clippy::too_many_arguments)]
fn walk_body<'a, 'r: 'a>(
    body: &'a [DocumentChild],
    content: &'a str,
    markup_is_markdown: bool,
    depth: u8,
    reqs: &'a mut Vec<ReqDefinition>,
    headings: &'a mut Vec<Heading>,
    elements: &'a mut Vec<DocElement>,
    mut render: Option<&'a mut RenderState<'r>>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        for child in body {
            match child {
                DocumentChild::Section(s) => {
                    let level = depth.min(6);
                    let base = slugify(&s.title);
                    let id = if let Some(st) = &mut render {
                        let id = st.slugs.alloc(&base);
                        st.html.push_str(&format!(
                            "<h{level} id=\"{id}\">{}</h{level}>\n",
                            html_escape::encode_text(&s.title)
                        ));
                        id
                    } else {
                        base
                    };
                    let h = Heading {
                        title: s.title.clone(),
                        id,
                        level,
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
                        render.as_deref_mut(),
                    )
                    .await;
                }
                DocumentChild::Requirement(r) => {
                    if let Some(def) = build_req(r, content, markup_is_markdown).await {
                        if let Some(st) = &mut render {
                            st.html.push_str(&(st.badge_for)(&def, st.source_path));
                            st.html.push_str(&def.html);
                            st.html.push_str(super::REQ_CONTAINER_CLOSE);
                            st.html.push('\n');
                        }
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
    let uid_field = req.field("UID")?;
    let uid = uid_field.value.text();
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
    // Use the `UID:` value span as the marker so LSP semantic tokens /
    // diagnostics get a non-zero range to anchor to.
    let uid_span = uid_field.value.span();
    Some(ReqDefinition {
        id: rule_id,
        anchor_id: req_anchor_id(uid),
        marker_span: SourceSpan {
            offset: uid_span.start,
            length: uid_span.end.saturating_sub(uid_span.start),
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

fn wrap_paragraph(text: &str) -> String {
    format!("<p>{}</p>", html_escape::encode_text(text))
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
