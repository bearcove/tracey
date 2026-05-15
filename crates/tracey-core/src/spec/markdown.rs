//! Markdown spec backend — thin wrapper over `marq`.

use std::ops::Range;
use std::pin::Pin;

use marq::SourceSpan;

use super::{NoConfig, RenderInput, RenderOutput, RenderedSection, SpecBackend, SpecDoc, SpecFormat};

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
        let RenderInput {
            sources,
            root,
            badge_for,
            slugs,
            marq_opts,
        } = input;

        if sources.is_empty() {
            return Ok(RenderOutput {
                sections: vec![],
                deps: vec![],
            });
        }

        // Concatenate the run so heading IDs are hierarchical across files
        // (matches the pre-multi-format behaviour in `data.rs`).
        let mut combined = String::new();
        for src in sources {
            combined.push_str(src.content);
            combined.push_str("\n\n");
        }

        // The whole run is attributed to the first file for `data-source-file`
        // attributes and badge edit-links (existing behaviour).
        let abs_source = root.join(sources[0].path).display().to_string();

        // Caller supplies diagram / inline-code handlers via `marq_opts`; we
        // overwrite `source_path` and `req_handler` per-render. `RenderOptions`
        // is not `Clone`, so when no opts are provided we own a default.
        let mut local_opts;
        let opts: &mut marq::RenderOptions = match marq_opts {
            Some(o) => o,
            None => {
                local_opts = marq::RenderOptions::default();
                &mut local_opts
            }
        };
        opts.source_path = Some(abs_source.clone());
        // caller may have set its own req_handler; ours wins — badge injection is mandatory
        opts.req_handler = Some(std::sync::Arc::new(BadgeReqHandler {
            badge_for: badge_for.clone(),
            source_path: abs_source,
        }));

        let mut doc = marq::render(&combined, opts).await?;
        reslug_marq_html(&mut doc, slugs);

        Ok(RenderOutput {
            sections: vec![RenderedSection {
                source_idx: 0,
                html: doc.html,
                elements: doc.elements,
                head_injections: doc.head_injections,
            }],
            deps: Vec::new(),
        })
    }

    async fn render_inline(&self, text: &str) -> String {
        marq::render(text, &marq::RenderOptions::default())
            .await
            .map(|d| d.html)
            // degrade: snippet render must not fail the search response
            .unwrap_or_else(|_| html_escape::encode_text(text).into_owned())
    }
}

/// `marq::ReqHandler` that delegates to a [`BadgeFn`](super::BadgeFn).
struct BadgeReqHandler {
    badge_for: super::BadgeFn,
    source_path: String,
}

impl marq::ReqHandler for BadgeReqHandler {
    fn start<'a>(
        &'a self,
        rule: &'a marq::ReqDefinition,
    ) -> Pin<Box<dyn Future<Output = marq::Result<String>> + Send + 'a>> {
        let (open, _) = (self.badge_for)(rule, &self.source_path);
        Box::pin(async move { Ok(open) })
    }
    fn end<'a>(
        &'a self,
        rule: &'a marq::ReqDefinition,
    ) -> Pin<Box<dyn Future<Output = marq::Result<String>> + Send + 'a>> {
        // close-half is constant in practice; cheaper than threading state through the handler
        let (_, close) = (self.badge_for)(rule, &self.source_path);
        Box::pin(async move { Ok(close) })
    }
}

/// Re-thread marq's per-document heading slugs through the global allocator,
/// patching `<hN id="…">` in `doc.html` to match.
///
/// Ported sans the `tracing::warn!` on needle-miss (`tracey-core` has no
/// tracing dep) from `crates/tracey/src/data.rs:3104-3128`; see the rationale
/// there for the forward-cursor scan.
fn reslug_marq_html(doc: &mut SpecDoc, slugs: &mut super::SlugAllocator) {
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
async fn parse(content: &str) -> eyre::Result<SpecDoc> {
    let doc = marq::render(content, &marq::RenderOptions::default()).await?;
    Ok(doc)
}

/// Inline HTML diff of two markdown snippets.
fn diff_inline(old: &str, new: &str) -> String {
    marq::diff_markdown_inline(old, new)
}

/// Read the `weight` field from TOML/YAML frontmatter, defaulting to `0`.
fn parse_weight(content: &str) -> i32 {
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
fn extract_marker_prefix(content: &str, marker_span: SourceSpan) -> Option<String> {
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
fn id_range_in_marker(marker_str: &str) -> eyre::Result<std::ops::Range<usize>> {
    let open = marker_str
        .find('[')
        .ok_or_else(|| eyre::eyre!("malformed markdown marker: {}", marker_str))?;
    let close = marker_str
        .rfind(']')
        .ok_or_else(|| eyre::eyre!("malformed markdown marker: {}", marker_str))?;
    Ok(open + 1..close)
}
