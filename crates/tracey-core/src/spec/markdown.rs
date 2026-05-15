//! Markdown spec backend — thin wrapper over `marq`.

use std::ops::Range;
use std::pin::Pin;

use marq::SourceSpan;

use super::{NoConfig, RenderInput, RenderOutput, SpecBackend, SpecDoc, SpecFormat};

/// Markdown backend.
///
/// Parsing and inline-diff are direct `marq` calls. `render_html` concatenates
/// the run, renders once via `marq::render`, then re-threads heading slugs
/// through the shared [`SlugAllocator`](super::SlugAllocator).
pub struct Markdown;

#[async_trait::async_trait]
impl SpecBackend for Markdown {
    type Config = NoConfig;

    fn format(&self) -> SpecFormat {
        SpecFormat::Markdown
    }
    fn name(&self) -> &'static str {
        "markdown"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["md", "markdown"]
    }

    async fn parse(&self, content: &str) -> eyre::Result<SpecDoc> {
        parse(content).await
    }
    fn parse_weight(&self, content: &str) -> i32 {
        parse_weight(content)
    }
    fn extract_marker_prefix(&self, content: &str, span: SourceSpan) -> Option<String> {
        extract_marker_prefix(content, span)
    }
    fn id_range_in_marker(&self, marker: &str) -> eyre::Result<Range<usize>> {
        id_range_in_marker(marker)
    }
    fn diff_inline(&self, old: &str, new: &str) -> Option<String> {
        Some(diff_inline(old, new))
    }

    async fn render_html(
        &self,
        input: RenderInput<'_>,
        _cfg: &NoConfig,
    ) -> eyre::Result<RenderOutput> {
        // Concatenate the run so heading IDs are hierarchical across files
        // (matches the pre-multi-format behaviour in `data.rs`).
        let mut combined = String::new();
        for src in input.sources {
            combined.push_str(src.content);
            combined.push_str("\n\n");
        }

        // Minimal `RenderOptions`: a req-handler wrapping `input.badge_for` so
        // requirement bodies are bracketed by the caller's badge markup.
        //
        // The full `data.rs` pipeline additionally configures diagram handlers
        // (aasvg/pikchr/mermaid/compare), an inline-code handler, and
        // `opts.source_path` for `data-source-file` attributes — all of which
        // live in `crates/tracey/` and cannot be referenced here. Task 5 must
        // either move those handlers into `tracey-core` or extend `RenderInput`
        // to carry a pre-built `marq::RenderOptions`.
        let opts = marq::RenderOptions::new().with_req_handler(BadgeReqHandler {
            badge_for: input.badge_for.clone(),
        });
        let mut doc = marq::render(&combined, &opts).await?;

        reslug_marq_html(&mut doc, input.slugs);

        Ok(RenderOutput {
            html: doc.html,
            deps: Vec::new(),
        })
    }

    async fn render_inline(&self, text: &str) -> String {
        marq::render(text, &marq::RenderOptions::default())
            .await
            .map(|d| d.html)
            .unwrap_or_else(|_| html_escape::encode_text(text).into_owned())
    }
}

/// `marq::ReqHandler` that delegates to a [`BadgeFn`](super::BadgeFn).
struct BadgeReqHandler {
    badge_for: super::BadgeFn,
}

impl marq::ReqHandler for BadgeReqHandler {
    fn start<'a>(
        &'a self,
        rule: &'a marq::ReqDefinition,
    ) -> Pin<Box<dyn Future<Output = marq::Result<String>> + Send + 'a>> {
        let (open, _) = (self.badge_for)(rule);
        Box::pin(async move { Ok(open) })
    }
    fn end<'a>(
        &'a self,
        rule: &'a marq::ReqDefinition,
    ) -> Pin<Box<dyn Future<Output = marq::Result<String>> + Send + 'a>> {
        let (_, close) = (self.badge_for)(rule);
        Box::pin(async move { Ok(close) })
    }
}

/// Re-thread marq's per-document heading slugs through the global allocator,
/// patching `<hN id="…">` in `doc.html` to match.
///
/// Ported verbatim from `crates/tracey/src/data.rs:3104-3128`; see the
/// rationale there for the forward-cursor scan.
pub(super) fn reslug_marq_html(doc: &mut SpecDoc, slugs: &mut super::SlugAllocator) {
    let mut cursor = 0;
    for el in doc.elements.iter_mut() {
        if let marq::DocElement::Heading(h) = el {
            let new = slugs.alloc(&h.id);
            let needle = format!("<h{} id=\"{}\"", h.level, h.id);
            if let Some(rel) = doc.html[cursor..].find(&needle) {
                let abs = cursor + rel;
                if new != h.id {
                    let repl = format!("<h{} id=\"{new}\"", h.level);
                    doc.html.replace_range(abs..abs + needle.len(), &repl);
                    cursor = abs + repl.len();
                } else {
                    cursor = abs + needle.len();
                }
            }
            h.id = new;
        }
    }
}

/// Parse a markdown spec document via `marq::render` with default options.
pub(super) async fn parse(content: &str) -> eyre::Result<SpecDoc> {
    let doc = marq::render(content, &marq::RenderOptions::default()).await?;
    Ok(doc)
}

/// Inline HTML diff of two markdown snippets.
pub(super) fn diff_inline(old: &str, new: &str) -> String {
    marq::diff_markdown_inline(old, new)
}

/// Read the `weight` field from TOML/YAML frontmatter, defaulting to `0`.
pub(super) fn parse_weight(content: &str) -> i32 {
    match marq::parse_frontmatter(content) {
        Ok((fm, _)) => fm.weight,
        Err(_) => 0,
    }
}

/// Extract the marker prefix (text before `[`) from the marker substring at
/// `marker_span` in `content`.
///
/// Mirrors the logic at `crates/tracey/src/lib.rs` and
/// `crates/tracey/src/data.rs` so behaviour is identical once those call
/// sites are routed here.
pub(super) fn extract_marker_prefix(content: &str, marker_span: SourceSpan) -> Option<String> {
    let start = marker_span.offset;
    let end = start.checked_add(marker_span.length)?;
    let marker = content.get(start..end)?;
    let bracket = marker.find('[')?;
    let prefix = marker[..bracket].trim();
    if prefix.is_empty() {
        return None;
    }
    Some(prefix.to_string())
}

/// Locate the id literal between `[` and `]` in a `prefix[id]` marker.
///
/// Markdown markers have no inline metadata, so the bracket pair is unique and
/// a simple `find`/`rfind` suffices. See [`super::id_range_in_marker`].
pub(super) fn id_range_in_marker(marker_str: &str) -> eyre::Result<std::ops::Range<usize>> {
    let open = marker_str
        .find('[')
        .ok_or_else(|| eyre::eyre!("malformed markdown marker: {}", marker_str))?;
    let close = marker_str
        .rfind(']')
        .ok_or_else(|| eyre::eyre!("malformed markdown marker: {}", marker_str))?;
    Ok(open + 1..close)
}
