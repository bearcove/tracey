//! AST walk for asciidork documents: extract requirements, headings, inline code spans.

use std::collections::HashSet;

use asciidork_ast::{
    AttrValue, Block, BlockContent, BlockContext, DocContent, Document, Inline, InlineNodes,
    ReadAttr, Section,
};
use marq::{
    DocElement, Heading, InlineCodeSpan, Paragraph, ReqDefinition, ReqMetadata, SourceSpan,
};

use crate::spec::{SlugAllocator, req_anchor_id};

/// Result of walking the asciidork AST.
pub struct WalkResult {
    pub reqs: Vec<ReqDefinition>,
    pub headings: Vec<Heading>,
    pub elements: Vec<DocElement>,
    pub inline_code_spans: Vec<InlineCodeSpan>,
    pub weight: i32,
    /// Mapping from asciidork section IDs to our allocated slugs.
    /// Each entry is `(asciidork_id, our_slug)`, in document order.
    /// Used to post-process the rendered HTML heading `id=` attributes.
    pub section_id_map: Vec<(String, String)>,
}

/// Walk the asciidork document AST to extract spec data.
pub fn walk<'arena>(
    doc: &Document<'arena>,
    source: &str,
    alloc: &mut SlugAllocator,
) -> eyre::Result<WalkResult> {
    let mut result = WalkResult {
        reqs: Vec::new(),
        headings: Vec::new(),
        elements: Vec::new(),
        inline_code_spans: Vec::new(),
        weight: 0,
        section_id_map: Vec::new(),
    };

    // Extract weight from document header attributes (:weight: N syntax)
    if let Some(AttrValue::String(w)) = doc.meta.get("weight") {
        result.weight = w.trim().parse::<i32>().unwrap_or(0);
    }

    let mut seen_bases = HashSet::new();

    match &doc.content {
        DocContent::Blocks(blocks) => {
            walk_blocks(blocks, source, alloc, &mut result, &mut seen_bases)?;
        }
        DocContent::Sections(sectioned) => {
            if let Some(preamble) = &sectioned.preamble {
                walk_blocks(preamble, source, alloc, &mut result, &mut seen_bases)?;
            }
            for section in &sectioned.sections {
                walk_section(section, source, alloc, &mut result, &mut seen_bases)?;
            }
        }
        DocContent::Parts(_) => {}
    }

    Ok(result)
}

fn walk_blocks<'arena>(
    blocks: &[Block<'arena>],
    source: &str,
    alloc: &mut SlugAllocator,
    result: &mut WalkResult,
    seen_bases: &mut HashSet<String>,
) -> eyre::Result<()> {
    for block in blocks {
        walk_block(block, source, alloc, result, seen_bases)?;
    }
    Ok(())
}

fn walk_section<'arena>(
    section: &Section<'arena>,
    source: &str,
    alloc: &mut SlugAllocator,
    result: &mut WalkResult,
    seen_bases: &mut HashSet<String>,
) -> eyre::Result<()> {
    let title = section.heading.plain_text().join("");
    let base_slug = marq::slugify(&title);
    let our_slug = alloc.alloc(&base_slug);

    if let Some(adoc_id) = &section.id {
        result.section_id_map.push((adoc_id.as_str().to_string(), our_slug.clone()));
    }

    let start_pos = section.loc.start_pos as usize;
    let line = byte_offset_to_line(source, start_pos);

    let heading = Heading {
        title: title.clone(),
        id: our_slug.clone(),
        level: section.level,
        line,
    };
    result.headings.push(heading.clone());
    result.elements.push(DocElement::Heading(heading));

    collect_spans_from_inlines(&section.heading, &mut result.inline_code_spans);
    walk_blocks(&section.blocks, source, alloc, result, seen_bases)?;
    Ok(())
}

fn walk_block<'arena>(
    block: &Block<'arena>,
    source: &str,
    alloc: &mut SlugAllocator,
    result: &mut WalkResult,
    seen_bases: &mut HashSet<String>,
) -> eyre::Result<()> {
    match &block.content {
        BlockContent::Simple(inlines) if block.context == BlockContext::Paragraph => {
            walk_paragraph(inlines, source, result, seen_bases)?;
        }
        BlockContent::Simple(inlines) => {
            collect_spans_from_inlines(inlines, &mut result.inline_code_spans);
        }
        BlockContent::Section(section) => {
            walk_section(section, source, alloc, result, seen_bases)?;
        }
        BlockContent::Compound(inner_blocks) => {
            walk_blocks(inner_blocks, source, alloc, result, seen_bases)?;
        }
        BlockContent::DocumentAttribute(key, AttrValue::String(w)) if key == "weight" => {
            result.weight = w.trim().parse::<i32>().unwrap_or(0);
        }
        _ => {}
    }
    Ok(())
}

fn walk_paragraph<'arena>(
    inlines: &InlineNodes<'arena>,
    source: &str,
    result: &mut WalkResult,
    seen_bases: &mut HashSet<String>,
) -> eyre::Result<()> {
    collect_spans_from_inlines(inlines, &mut result.inline_code_spans);

    let Some(first) = inlines.first() else {
        result.elements.push(DocElement::Paragraph(Paragraph { line: 0, offset: 0 }));
        return Ok(());
    };

    let first_text = match &first.content {
        Inline::Text(s) => s.as_str(),
        _ => {
            let line = byte_offset_to_line(source, first.loc.start as usize);
            result.elements.push(DocElement::Paragraph(Paragraph { line, offset: 0 }));
            return Ok(());
        }
    };

    let Some((_, id_str, marker_end)) = parse_req_leading_marker(first_text) else {
        let line = byte_offset_to_line(source, first.loc.start as usize);
        result.elements.push(DocElement::Paragraph(Paragraph { line, offset: 0 }));
        return Ok(());
    };

    let Some((req_id, metadata)) = parse_req_marker_inner(id_str) else {
        let line = byte_offset_to_line(source, first.loc.start as usize);
        result.elements.push(DocElement::Paragraph(Paragraph { line, offset: 0 }));
        return Ok(());
    };

    if seen_bases.contains(&req_id.base) {
        eyre::bail!("Duplicate requirement '{}' in AsciiDoc file", req_id.base);
    }
    seen_bases.insert(req_id.base.clone());

    let span_start = first.loc.start as usize;
    let span_end = inlines
        .last_loc()
        .map_or(span_start, |l| l.end as usize);
    let line = byte_offset_to_line(source, span_start);

    let marker_len = marker_end + 1;
    let anchor = req_anchor_id(&req_id.to_string());

    // Build raw body from the source span (preserves newlines)
    let block_source = source.get(span_start..span_end.min(source.len())).unwrap_or("");
    let after_marker = block_source.get(marker_len..).unwrap_or("").trim_start_matches([' ', '\t', '\n', '\r']);
    let raw = after_marker.trim_end().to_string();

    let req_html = render_req_body_html(&raw);

    let req = ReqDefinition {
        id: req_id,
        anchor_id: anchor,
        marker_span: SourceSpan {
            offset: span_start,
            length: marker_len,
        },
        span: SourceSpan {
            offset: span_start,
            length: span_end.saturating_sub(span_start),
        },
        line,
        metadata,
        raw,
        html: req_html,
    };

    result.elements.push(DocElement::Req(req.clone()));
    result.reqs.push(req);
    Ok(())
}

/// Collect inline code spans (backtick / mono spans) from inline nodes.
fn collect_spans_from_inlines<'arena>(
    inlines: &InlineNodes<'arena>,
    spans: &mut Vec<InlineCodeSpan>,
) {
    for node in inlines.iter() {
        match &node.content {
            Inline::Span(asciidork_ast::SpanKind::LitMono, _, inner)
            | Inline::Span(asciidork_ast::SpanKind::Mono, _, inner) => {
                let content = inner.plain_text().join("");
                let start = node.loc.start as usize;
                let end = node.loc.end as usize;
                spans.push(InlineCodeSpan {
                    content,
                    span: SourceSpan {
                        offset: start,
                        length: end.saturating_sub(start),
                    },
                });
                collect_spans_from_inlines(inner, spans);
            }
            Inline::Span(_, _, inner) => {
                collect_spans_from_inlines(inner, spans);
            }
            _ => {}
        }
    }
}

/// Convert a byte offset into a 1-based line number.
pub fn byte_offset_to_line(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())].matches('\n').count() + 1
}

/// Render requirement body as HTML (`<p>` wrapped plain text).
pub fn render_req_body_html(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let mut html = String::new();
    for para in raw.split("\n\n") {
        let trimmed = para.trim();
        if !trimmed.is_empty() {
            html.push_str("<p>");
            html.push_str(&html_escape(trimmed));
            html.push_str("</p>\n");
        }
    }
    html
}

/// Parse a requirement leading marker at the start of a line.
///
/// Returns `(prefix, id_str, marker_end_offset)` where `marker_end_offset`
/// is the index of the closing `]` (inclusive).
pub fn parse_req_leading_marker(line: &str) -> Option<(&str, &str, usize)> {
    let mut prefix_len = 0usize;
    for ch in line.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            prefix_len += ch.len_utf8();
        } else {
            break;
        }
    }
    if prefix_len == 0 || line.as_bytes().get(prefix_len) != Some(&b'[') {
        return None;
    }
    let close = line.find(']')?;
    if close <= prefix_len + 1 {
        return None; // empty brackets
    }
    let prefix = &line[..prefix_len];
    let id_str = &line[prefix_len + 1..close];
    Some((prefix, id_str, close))
}

/// Parse the inner content of a requirement marker (the text between `[` and `]`).
pub fn parse_req_marker_inner(inner: &str) -> Option<(marq::RuleId, ReqMetadata)> {
    let inner = inner.trim();
    let (id_part, attrs_str) = match inner.find(' ') {
        Some(idx) => (&inner[..idx], inner[idx + 1..].trim()),
        None => (inner, ""),
    };
    let req_id = marq::parse_rule_id(id_part)?;
    let mut metadata = ReqMetadata::default();
    if !attrs_str.is_empty() {
        for attr in attrs_str.split_whitespace() {
            if let Some((key, value)) = attr.split_once('=') {
                match key {
                    "status" => metadata.status = marq::ReqStatus::parse(value),
                    "level" => metadata.level = marq::ReqLevel::parse(value),
                    "since" => metadata.since = Some(value.to_string()),
                    "until" => metadata.until = Some(value.to_string()),
                    "tags" => {
                        metadata.tags =
                            value.split(',').map(|s| s.trim().to_string()).collect()
                    }
                    _ => {}
                }
            }
        }
    }
    Some((req_id, metadata))
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
