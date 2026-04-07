//! Typst spec backend.
//!
//! Requirement extraction walks the tree-sitter parse tree produced by
//! `arborium-typst` and constructs a [`SpecDoc`] directly. This is the
//! lightweight path: it populates `reqs`, `headings`, `elements`, and
//! `inline_code_spans`. The `html` field is a plain `<pre>` placeholder;
//! Phase 8 wires the typst→HTML compiler behind a feature gate.
//!
//! Node-shape findings (Spike B, NOTES.txt):
//!   - `#req("ID", k: v)[body]` parses as
//!     `code > call > [item: call > [item: ident, group > string tagged*], content]`
//!   - `string` and `content` spans include their delimiters.
//!   - `heading` level = `child(0).kind().len()` (anonymous `=`, `==`, …).
//!   - `raw_span > blob` is inline code without backticks.

use arborium_tree_sitter::{Node, Parser};
use marq::{
    DocElement, Heading, InlineCodeSpan, ReqDefinition, ReqLevel, ReqMetadata, ReqStatus,
    SourceSpan,
};

use super::SpecDoc;

pub(super) async fn parse(content: &str) -> eyre::Result<SpecDoc> {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_typst::language().into())
        .map_err(|e| eyre::eyre!("failed to load typst grammar: {e}"))?;
    let tree = parser
        .parse(content, None)
        .ok_or_else(|| eyre::eyre!("typst parser returned no tree"))?;

    let bytes = content.as_bytes();
    let mut reqs: Vec<ReqDefinition> = Vec::new();
    let mut headings: Vec<Heading> = Vec::new();
    let mut inline_code_spans: Vec<InlineCodeSpan> = Vec::new();
    // (start_byte, element) — sorted into source order before emitting.
    let mut ordered: Vec<(usize, DocElement)> = Vec::new();

    walk(tree.root_node(), &mut |node| match node.kind() {
        "code" => {
            if let Some(req) = extract_req(node, bytes) {
                ordered.push((node.start_byte(), DocElement::Req(req.clone())));
                reqs.push(req);
                // Don't descend: nested `#req`/headings inside a req body are
                // part of that req's content, not independent doc elements.
                return false;
            }
            true
        }
        "heading" => {
            if let Some(h) = extract_heading(node, bytes) {
                ordered.push((node.start_byte(), DocElement::Heading(h.clone())));
                headings.push(h);
            }
            true
        }
        "raw_span" => {
            if let Some(span) = extract_inline_code(node, bytes) {
                inline_code_spans.push(span);
            }
            true
        }
        _ => true,
    });

    ordered.sort_by_key(|(start, _)| *start);
    let elements = ordered.into_iter().map(|(_, e)| e).collect();

    let html = format!(
        "<pre class=\"typst-placeholder\"><code>{}</code></pre>",
        html_escape::encode_text(content)
    );

    Ok(SpecDoc {
        raw_metadata: None,
        metadata_format: None,
        frontmatter: None,
        html,
        headings,
        reqs,
        code_samples: vec![],
        elements,
        head_injections: vec![],
        inline_code_spans,
    })
}

/// Depth-first pre-order walk of `node`, invoking `f` on every node.
/// `f` returns `false` to prune the subtree (skip children).
fn walk<'a>(node: Node<'a>, f: &mut impl FnMut(Node<'a>) -> bool) {
    if !f(node) {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, f);
    }
}

/// Try to interpret a `code` node as a `#prefix("id", ..)[body]` requirement.
fn extract_req(code: Node<'_>, bytes: &[u8]) -> Option<ReqDefinition> {
    // code > call(outer) > [item: call(inner), content]
    let outer = find_child(code, "call")?;
    let inner = outer.child_by_field_name("item")?;
    if inner.kind() != "call" {
        return None;
    }
    let ident = inner.child_by_field_name("item")?;
    if ident.kind() != "ident" {
        return None;
    }
    let prefix = slice(bytes, ident.start_byte(), ident.end_byte());
    // v1 heuristic: typst's `#fn("str")[body]` is the universal call syntax,
    // so we'd otherwise pick up `#figure`, `#link`, etc. Downstream prefix
    // inference (tracey::data) hard-errors on mixed prefixes rather than
    // filtering, so reject obviously-too-long idents here. 5 chars admits
    // "r", "req", "rule", "spec"; revisit when config-driven prefixes land.
    if prefix.len() > 5 {
        return None;
    }

    // Positional + tagged args live under `group`.
    let group = find_child(inner, "group")?;
    let id_node = find_child(group, "string")?;
    let id_text = strip_delims(bytes, id_node)?;
    let id = marq::parse_rule_id(id_text)?;

    let metadata = extract_metadata(group, bytes);

    // Body content is optional; `#req("x")` with no `[body]` is still a definition.
    let (raw, span_end) = match find_child(outer, "content") {
        Some(body) => (
            strip_delims(bytes, body).unwrap_or("").to_string(),
            body.end_byte(),
        ),
        None => (String::new(), inner.end_byte()),
    };

    let code_start = code.start_byte();
    let marker_end = inner.end_byte();
    let line = code.start_position().row + 1;

    Some(ReqDefinition {
        anchor_id: format!("{}-{}", prefix, id),
        id,
        marker_span: SourceSpan {
            offset: code_start,
            length: marker_end - code_start,
        },
        span: SourceSpan {
            offset: code_start,
            length: span_end - code_start,
        },
        line,
        metadata,
        raw,
        html: String::new(),
    })
}

/// Parse `tagged` children of a call `group` into [`ReqMetadata`].
fn extract_metadata(group: Node<'_>, bytes: &[u8]) -> ReqMetadata {
    let mut meta = ReqMetadata::default();
    let mut cursor = group.walk();
    for child in group.children(&mut cursor) {
        if child.kind() != "tagged" {
            continue;
        }
        let Some(field) = child.child_by_field_name("field") else {
            continue;
        };
        let key = slice(bytes, field.start_byte(), field.end_byte());
        let Some(value_node) = field.next_named_sibling() else {
            continue;
        };
        let value = if value_node.kind() == "string" {
            strip_delims(bytes, value_node).unwrap_or("")
        } else {
            slice(bytes, value_node.start_byte(), value_node.end_byte())
        };
        match key {
            "level" => meta.level = ReqLevel::parse(value),
            "status" => meta.status = ReqStatus::parse(value),
            "since" => meta.since = Some(value.to_string()),
            "until" => meta.until = Some(value.to_string()),
            _ => {}
        }
    }
    meta
}

fn extract_heading(node: Node<'_>, bytes: &[u8]) -> Option<Heading> {
    // child(0) is the anonymous `=`/`==`/... token.
    let marker = node.child(0)?;
    let level = marker.kind().len() as u8;
    // Title is everything after the marker, trimmed. Using the byte slice
    // (rather than the `text` child) keeps rich-heading content intact for
    // slugification, matching the markdown backend's behaviour.
    let title = slice(bytes, marker.end_byte(), node.end_byte())
        .trim()
        .to_string();
    Some(Heading {
        id: marq::slugify(&title),
        title,
        level,
        line: node.start_position().row + 1,
    })
}

fn extract_inline_code(node: Node<'_>, bytes: &[u8]) -> Option<InlineCodeSpan> {
    let blob = find_child(node, "blob")?;
    Some(InlineCodeSpan {
        content: slice(bytes, blob.start_byte(), blob.end_byte()).to_string(),
        span: SourceSpan {
            offset: blob.start_byte(),
            length: blob.end_byte() - blob.start_byte(),
        },
    })
}

// ---- tree helpers ---------------------------------------------------------

fn find_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).find(|c| c.kind() == kind)
}

fn slice(bytes: &[u8], start: usize, end: usize) -> &str {
    std::str::from_utf8(&bytes[start..end]).unwrap_or("")
}

/// Strip the first and last byte of a node's span (quotes / brackets).
fn strip_delims<'a>(bytes: &'a [u8], node: Node<'_>) -> Option<&'a str> {
    let start = node.start_byte();
    let end = node.end_byte();
    if end <= start + 1 {
        return Some("");
    }
    std::str::from_utf8(&bytes[start + 1..end - 1]).ok()
}

// ---- dispatch arms --------------------------------------------------------

pub(super) fn diff_inline(_old: &str, _new: &str) -> Option<String> {
    // No inline diff for typst in v1; callers fall back to plain text.
    None
}

pub(super) fn parse_weight(_content: &str) -> i32 {
    // Weight is deferred for typst (Q2): always sort with default weight.
    0
}

/// Extract the marker prefix (e.g. `"req"` from `#req("X")`) at `span` in
/// `content`.
pub(super) fn extract_marker_prefix(content: &str, span: SourceSpan) -> Option<String> {
    let start = span.offset;
    let end = start.checked_add(span.length)?;
    let marker = content.get(start..end)?;
    let after_hash = marker.trim_start_matches('#');
    let paren = after_hash.find('(')?;
    let prefix = after_hash[..paren].trim();
    if prefix.is_empty() {
        return None;
    }
    Some(prefix.to_string())
}

/// Rebuild a `#prefix("base+ver", ..)` marker from its current text and a new
/// version number, preserving any trailing tagged arguments verbatim.
pub(super) fn rewrite_marker(marker_str: &str, base: &str, new_ver: u32) -> eyre::Result<String> {
    // Replace only the contents of the first string literal; everything else
    // (prefix, tagged metadata, closing paren) is kept byte-for-byte so
    // `tracey bump` doesn't silently delete `level:`/`status:` annotations.
    let open = marker_str
        .find('"')
        .ok_or_else(|| eyre::eyre!("malformed typst marker: {}", marker_str))?;
    let close_rel = marker_str[open + 1..]
        .find('"')
        .ok_or_else(|| eyre::eyre!("malformed typst marker: {}", marker_str))?;
    let close = open + 1 + close_rel;
    Ok(format!(
        "{}{}+{}{}",
        &marker_str[..=open],
        base,
        new_ver,
        &marker_str[close..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parses_reqs() {
        let src = r#"= Title

#req("auth.login")[Users MUST log in.]

== Subsection

#r("auth.session+2", level: "shall", status: "draft")[
  Sessions expire.
]
"#;
        let doc = parse(src).await.unwrap();

        assert_eq!(doc.reqs.len(), 2, "expected two requirements");
        assert_eq!(doc.headings.len(), 2);

        let r0 = &doc.reqs[0];
        assert_eq!(r0.id.base, "auth.login");
        assert_eq!(r0.id.version, 1);
        assert_eq!(r0.line, 3);
        assert_eq!(r0.anchor_id, "req-auth.login");
        assert_eq!(r0.raw, "Users MUST log in.");
        // marker_span covers `#req("auth.login")` only
        let m = &src[r0.marker_span.offset..r0.marker_span.offset + r0.marker_span.length];
        assert_eq!(m, "#req(\"auth.login\")");
        // span covers the whole call including [body]
        let s = &src[r0.span.offset..r0.span.offset + r0.span.length];
        assert_eq!(s, "#req(\"auth.login\")[Users MUST log in.]");

        let r1 = &doc.reqs[1];
        assert_eq!(r1.id.base, "auth.session");
        assert_eq!(r1.id.version, 2);
        assert_eq!(r1.line, 7);
        assert_eq!(r1.anchor_id, "r-auth.session+2");
        assert_eq!(r1.metadata.level, Some(ReqLevel::Must));
        assert_eq!(r1.metadata.status, Some(ReqStatus::Draft));

        // Elements interleave in source order: Heading, Req, Heading, Req
        assert_eq!(doc.elements.len(), 4);
        assert!(matches!(doc.elements[0], DocElement::Heading(_)));
        assert!(matches!(doc.elements[1], DocElement::Req(_)));
        assert!(matches!(doc.elements[2], DocElement::Heading(_)));
        assert!(matches!(doc.elements[3], DocElement::Req(_)));
    }

    #[tokio::test]
    async fn heading_levels() {
        let src = "= One\n== Two\n=== Three\n";
        let doc = parse(src).await.unwrap();
        assert_eq!(doc.headings.len(), 3);
        assert_eq!(doc.headings[0].level, 1);
        assert_eq!(doc.headings[0].title, "One");
        assert_eq!(doc.headings[0].id, "one");
        assert_eq!(doc.headings[1].level, 2);
        assert_eq!(doc.headings[2].level, 3);
    }

    #[tokio::test]
    async fn collects_inline_code() {
        let src = "See `r[impl auth.login]` for details.\n";
        let doc = parse(src).await.unwrap();
        assert_eq!(doc.inline_code_spans.len(), 1);
        assert_eq!(doc.inline_code_spans[0].content, "r[impl auth.login]");
    }

    #[test]
    fn extracts_prefix() {
        let content = "#req(\"auth.login\")[body]";
        let span = SourceSpan {
            offset: 0,
            length: "#req(\"auth.login\")".len(),
        };
        assert_eq!(
            extract_marker_prefix(content, span),
            Some("req".to_string())
        );

        let content = "#r(\"x\")";
        let span = SourceSpan {
            offset: 0,
            length: content.len(),
        };
        assert_eq!(extract_marker_prefix(content, span), Some("r".to_string()));
    }

    #[test]
    fn rewrites_marker() {
        let out = rewrite_marker("#req(\"auth.login\")", "auth.login", 2).unwrap();
        assert_eq!(out, "#req(\"auth.login+2\")");

        let out = rewrite_marker("#r(\"auth.login+3\")", "auth.login", 4).unwrap();
        assert_eq!(out, "#r(\"auth.login+4\")");
    }

    #[test]
    fn rewrites_marker_preserves_metadata() {
        let out = rewrite_marker(
            "#req(\"auth.login+1\", level: \"shall\")",
            "auth.login",
            2,
        )
        .unwrap();
        assert_eq!(out, "#req(\"auth.login+2\", level: \"shall\")");

        let out = rewrite_marker(
            "#r(\"a.b\", level: \"may\", status: \"draft\")",
            "a.b",
            3,
        )
        .unwrap();
        assert_eq!(out, "#r(\"a.b+3\", level: \"may\", status: \"draft\")");
    }

    #[tokio::test]
    async fn nested_req_in_body_not_double_extracted() {
        let src = r#"#req("outer")[
  Body mentions #req("inner")[nested] and a == Heading.
]
"#;
        let doc = parse(src).await.unwrap();
        assert_eq!(doc.reqs.len(), 1, "nested req must not be extracted");
        assert_eq!(doc.reqs[0].id.base, "outer");
        assert!(doc.headings.is_empty(), "heading inside req body is content");
    }

    #[tokio::test]
    async fn long_idents_are_not_reqs() {
        // `#figure("...")` matches the call shape but is a typst built-in.
        let src = "#figure(\"img.one\")[Caption]\n#req(\"real.one\")[Body]\n";
        let doc = parse(src).await.unwrap();
        assert_eq!(doc.reqs.len(), 1);
        assert_eq!(doc.reqs[0].id.base, "real.one");
    }

    #[tokio::test]
    async fn placeholder_html_escapes() {
        let doc = parse("= <script>").await.unwrap();
        assert!(doc.html.contains("&lt;script&gt;"));
        assert!(doc.html.starts_with("<pre class=\"typst-placeholder\">"));
    }
}
