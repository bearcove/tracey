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

/// Context for [`render_display`]: callbacks that the typst→HTML pipeline cannot
/// resolve on its own (coverage data lives in the `tracey` crate).
pub struct RenderCtx<'a> {
    /// Given a requirement definition, return `(open_html, close_html)` — the
    /// fully-rendered badge container that wraps the requirement body. The
    /// `ReqDefinition` carries `id`, `line`, `span`, and `anchor_id` so badge
    /// rendering has everything it needs without a second parse.
    ///
    /// `Sync` because [`render_display`] is async and the ctx is held across
    /// awaits inside `Send` futures.
    pub badge_for: &'a (dyn Fn(&ReqDefinition) -> (String, String) + Sync),
}

/// Compile `content` with the typst HTML backend and splice coverage badges in.
///
/// The pipeline:
/// 1. [`parse`] the raw content (tree-sitter) for `reqs` / `headings` / spans.
/// 2. Prepend a prelude that maps `#req(id)[body]` → a sentinel `<div>` and
///    headings → sentinel `<hN>`; compile with `typst::compile<HtmlDocument>`.
/// 3. Post-process the HTML string: replace each sentinel `<div>` with the
///    badge markup from [`RenderCtx::badge_for`], and inject `id="slug"` into
///    each sentinel heading using slugs from step 1.
/// 4. Lift `<style>` / `<link>` from the compiler's `<head>` into
///    `head_injections`; return only the `<body>` interior as `html`.
///
/// `base_dir` is the directory containing the spec file; relative `#import` /
/// `#include` paths resolve against it. Package imports (`@preview/...`) other
/// than the tracey shim are not resolved and will fail compilation.
///
/// Behind the `typst-spec` feature. Without it, returns an error and callers
/// should fall back to [`parse`] (placeholder `<pre>` html).
#[cfg_attr(not(feature = "typst-spec"), allow(unused_variables))]
pub async fn render_display(
    content: &str,
    base_dir: &std::path::Path,
    ctx: &RenderCtx<'_>,
) -> eyre::Result<SpecDoc> {
    #[cfg(not(feature = "typst-spec"))]
    {
        Err(eyre::eyre!(
            "typst HTML rendering not compiled in (enable 'typst-spec' feature)"
        ))
    }
    #[cfg(feature = "typst-spec")]
    {
        compiler::render(content, base_dir, ctx).await
    }
}

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

/// Typst standard-library globals that must never be interpreted as requirement
/// marker prefixes.
///
/// Typst's `#fn("str")[body]` is the universal call syntax, so without this
/// filter `#image("foo.png")` or `#link("url")[text]` would be extracted as
/// requirements and poison downstream prefix inference (`tracey::data` hard-
/// errors on mixed prefixes rather than silently dropping them).
///
/// Sorted for `binary_search`. Covers the documented top-level functions from
/// the typst reference; if typst adds a new global that collides with a user's
/// chosen prefix, the "multiple requirement marker prefixes" error will surface
/// it explicitly.
const TYPST_BUILTINS: &[&str] = &[
    "align",
    "array",
    "assert",
    "bibliography",
    "block",
    "box",
    "bytes",
    "cbor",
    "circle",
    "cite",
    "colbreak",
    "columns",
    "counter",
    "csv",
    "curve",
    "datetime",
    "dictionary",
    "document",
    "duration",
    "ellipse",
    "emph",
    "enum",
    "eval",
    "figure",
    "float",
    "footnote",
    "gradient",
    "grid",
    "h",
    "heading",
    "hide",
    "highlight",
    "image",
    "include",
    "int",
    "json",
    "label",
    "layout",
    "line",
    "linebreak",
    "link",
    "list",
    "locate",
    "lorem",
    "lower",
    "measure",
    "metadata",
    "move",
    "numbering",
    "outline",
    "overline",
    "pad",
    "page",
    "pagebreak",
    "panic",
    "par",
    "parbreak",
    "path",
    "pattern",
    "place",
    "plugin",
    "polygon",
    "query",
    "quote",
    "raw",
    "read",
    "rect",
    "ref",
    "regex",
    "repeat",
    "repr",
    "rotate",
    "scale",
    "selector",
    "skew",
    "smallcaps",
    "smartquote",
    "square",
    "stack",
    "state",
    "str",
    "strike",
    "strong",
    "sub",
    "super",
    "table",
    "terms",
    "text",
    "tiling",
    "toml",
    "type",
    "underline",
    "upper",
    "v",
    "version",
    "xml",
    "yaml",
];

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
    // Reject typst stdlib globals so `#image(..)`, `#link(..)`, etc. are never
    // mistaken for requirement markers. Any other ident is a candidate; if a
    // spec genuinely mixes prefixes the inference step reports it.
    if TYPST_BUILTINS.binary_search(&prefix).is_ok() {
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

// ---- typst compiler bridge (feature-gated) --------------------------------

#[cfg(feature = "typst-spec")]
mod compiler {
    use super::{RenderCtx, SpecDoc, parse};
    use std::collections::HashMap;
    use std::fmt::Write as _;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use typst::diag::{FileError, FileResult};
    use typst::foundations::{Bytes, Datetime};
    use typst::syntax::{FileId, Source, VirtualPath};
    use typst::text::{Font, FontBook};
    use typst::utils::LazyHash;
    use typst::{Feature, Features, Library, LibraryExt as _, World};
    use typst_html::HtmlDocument;
    use typst_kit::fonts::{FontSlot, Fonts};

    /// Sentinel CSS classes the prelude emits so post-processing can find
    /// requirement containers and headings without a full HTML parser.
    const REQ_SENTINEL: &str = "tracey-req";
    const HEADING_SENTINEL: &str = "tracey-h";

    /// Prelude prepended to every spec before compilation. Defines `#req` /
    /// `#r` to emit sentinel `<div>`s and rewrites headings to sentinel
    /// `<hN>` tags. Slugs are *not* computed here (`it.body.text` fails on
    /// rich content); they're injected during post-processing from the
    /// tree-sitter parse output.
    const PRELUDE: &str = concat!(
        "#let req(id, ..meta, body) = html.elem(\n",
        "  \"div\", attrs: (class: \"tracey-req\", \"data-req-id\": id),\n",
        ")[#body]\n",
        "#let r = req\n",
        "#show heading: it => html.elem(\n",
        "  \"h\" + str(calc.min(it.level, 6)),\n",
        "  attrs: (class: \"tracey-h\"),\n",
        ")[#it.body]\n",
    );

    pub(super) async fn render(
        content: &str,
        base_dir: &Path,
        ctx: &RenderCtx<'_>,
    ) -> eyre::Result<SpecDoc> {
        // Structural extraction runs on the raw user content so spans / line
        // numbers point at the actual source file, not the prelude-shifted text.
        let mut doc = parse(content).await?;

        let stripped = strip_tracey_imports(content);
        let mut full = String::with_capacity(PRELUDE.len() + stripped.len());
        full.push_str(PRELUDE);
        full.push_str(&stripped);

        let world = SpecWorld::new(full, base_dir.to_path_buf());
        let compiled = typst::compile::<HtmlDocument>(&world);
        // Clear typst's global memoization cache so repeated compilations in a
        // long-running daemon don't accumulate unbounded memory.
        comemo::evict(0);
        let output = compiled.output.map_err(|errs| {
            let mut msg = String::from("typst compile failed:");
            for e in errs.iter() {
                let _ = write!(msg, "\n  {}", e.message);
            }
            eyre::eyre!(msg)
        })?;
        let html = typst_html::html(&output)
            .map_err(|e| eyre::eyre!("typst html serialize failed: {:?}", e))?;

        let (head, body) = split_head_body(&html);
        doc.head_injections = extract_head_injections(head);

        let slugs: Vec<&str> = doc.headings.iter().map(|h| h.id.as_str()).collect();
        let body = inject_heading_ids(body, &slugs);
        let by_id: std::collections::HashMap<String, &marq::ReqDefinition> =
            doc.reqs.iter().map(|r| (r.id.to_string(), r)).collect();
        doc.html = splice_req_badges(&body, &by_id, ctx);

        Ok(doc)
    }

    /// Minimal [`World`]: in-memory main source, embedded fonts, no package
    /// manager. Relative `#import` / `#include` resolve against `base_dir`;
    /// package imports (`@preview/...`) are rejected.
    struct SpecWorld {
        library: LazyHash<Library>,
        book: LazyHash<FontBook>,
        fonts: Vec<FontSlot>,
        main: Source,
        /// Directory the main spec file lives in; root for relative imports.
        base_dir: PathBuf,
        /// Disk reads cached per [`FileId`] — typst may request the same file
        /// repeatedly during a compile. Errors are cached too so a missing
        /// import is reported once, not re-stat'd.
        sources: Mutex<HashMap<FileId, FileResult<Source>>>,
        files: Mutex<HashMap<FileId, FileResult<Bytes>>>,
    }

    impl SpecWorld {
        fn new(text: String, base_dir: PathBuf) -> Self {
            let features: Features = [Feature::Html].into_iter().collect();
            let library = Library::builder().with_features(features).build();
            let fonts = Fonts::searcher()
                .include_system_fonts(false)
                .include_embedded_fonts(true)
                .search();
            let id = FileId::new(None, VirtualPath::new("spec.typ"));
            Self {
                library: LazyHash::new(library),
                book: LazyHash::new(fonts.book),
                fonts: fonts.fonts,
                main: Source::new(id, text),
                base_dir,
                sources: Mutex::new(HashMap::new()),
                files: Mutex::new(HashMap::new()),
            }
        }

        /// Resolve `id` to an on-disk path under `base_dir`, reading it as raw
        /// bytes. Package ids and paths that escape `base_dir` are rejected.
        fn read(&self, id: FileId) -> FileResult<Vec<u8>> {
            if id.package().is_some() {
                // No package manager; tracey's own package import is stripped
                // before compilation, anything else is unsupported.
                return Err(FileError::NotFound(id.vpath().as_rootless_path().into()));
            }
            let path = id
                .vpath()
                .resolve(&self.base_dir)
                .ok_or(FileError::AccessDenied)?;
            std::fs::read(&path).map_err(|e| FileError::from_io(e, &path))
        }
    }

    impl World for SpecWorld {
        fn library(&self) -> &LazyHash<Library> {
            &self.library
        }
        fn book(&self) -> &LazyHash<FontBook> {
            &self.book
        }
        fn main(&self) -> FileId {
            self.main.id()
        }
        fn source(&self, id: FileId) -> FileResult<Source> {
            if id == self.main.id() {
                return Ok(self.main.clone());
            }
            let mut cache = self.sources.lock().unwrap();
            cache
                .entry(id)
                .or_insert_with(|| {
                    let bytes = self.read(id)?;
                    let text = String::from_utf8(bytes)
                        .map_err(|_| FileError::InvalidUtf8)?;
                    Ok(Source::new(id, text))
                })
                .clone()
        }
        fn file(&self, id: FileId) -> FileResult<Bytes> {
            let mut cache = self.files.lock().unwrap();
            cache
                .entry(id)
                .or_insert_with(|| self.read(id).map(Bytes::new))
                .clone()
        }
        fn font(&self, index: usize) -> Option<Font> {
            self.fonts.get(index)?.get()
        }
        fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
            None
        }
    }

    /// Remove `#import "@preview/tracey:..."` / `#import "@local/tracey:..."`
    /// lines so the prelude's `#let r = ...` is the only definition in scope.
    /// Users add this import to make specs compile standalone; our [`World`]
    /// has no package manager, so leaving it in would fail compilation. Lines
    /// are blanked (not removed) so typst diagnostics keep their line numbers.
    fn strip_tracey_imports(content: &str) -> std::borrow::Cow<'_, str> {
        if !content.contains("/tracey:") {
            return std::borrow::Cow::Borrowed(content);
        }
        let mut out = String::with_capacity(content.len());
        let mut lines = content.split_inclusive('\n');
        while let Some(line) = lines.next() {
            let t = line.trim_start();
            let is_tracey_import = t.starts_with("#import ")
                && (t.contains("\"@preview/tracey:") || t.contains("\"@local/tracey:"));
            if !is_tracey_import {
                out.push_str(line);
                continue;
            }
            // Blank the import line, preserving the trailing newline.
            if line.ends_with('\n') {
                out.push('\n');
            }
            // Multi-line import list: `#import "...": (` … `)`. Consume and
            // blank continuation lines until the closing paren so we don't
            // leave orphaned `r, req,` tokens behind.
            if line.trim_end().ends_with('(') {
                for cont in lines.by_ref() {
                    if cont.ends_with('\n') {
                        out.push('\n');
                    }
                    if cont.trim_end().ends_with(')') {
                        break;
                    }
                }
            }
        }
        std::borrow::Cow::Owned(out)
    }

    // ---- HTML post-processing --------------------------------------------

    /// Extract `(head_inner, body_inner)` from a full HTML document. Falls back
    /// to `("", input)` if the markers aren't found (defensive: the typst
    /// serializer always emits them today).
    fn split_head_body(html: &str) -> (&str, &str) {
        let head = html
            .find("<head>")
            .and_then(|hs| html[hs + 6..].find("</head>").map(|he| &html[hs + 6..hs + 6 + he]))
            .unwrap_or("");
        let body = html
            .find("<body>")
            .and_then(|bs| html[bs + 6..].rfind("</body>").map(|be| &html[bs + 6..bs + 6 + be]))
            .unwrap_or(html);
        (head, body)
    }

    /// Lift `<style>` and `<link>` elements out of the compiler's `<head>`.
    fn extract_head_injections(head: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = head;
        while let Some(start) = rest.find("<style") {
            if let Some(end_rel) = rest[start..].find("</style>") {
                out.push(rest[start..start + end_rel + 8].to_string());
                rest = &rest[start + end_rel + 8..];
            } else {
                break;
            }
        }
        let mut rest = head;
        while let Some(start) = rest.find("<link") {
            if let Some(end_rel) = rest[start..].find('>') {
                out.push(rest[start..start + end_rel + 1].to_string());
                rest = &rest[start + end_rel + 1..];
            } else {
                break;
            }
        }
        out
    }

    /// Replace each `<hN class="tracey-h">` with `<hN id="slug">`, consuming
    /// `slugs` in document order. Headings beyond `slugs.len()` keep the tag
    /// but drop the sentinel class.
    fn inject_heading_ids(body: &str, slugs: &[&str]) -> String {
        let needle = format!(" class=\"{HEADING_SENTINEL}\"");
        let mut out = String::with_capacity(body.len());
        let mut rest = body;
        let mut idx = 0;
        while let Some(pos) = rest.find(&needle) {
            out.push_str(&rest[..pos]);
            if let Some(slug) = slugs.get(idx) {
                let _ = write!(out, " id=\"{slug}\"");
            }
            idx += 1;
            rest = &rest[pos + needle.len()..];
        }
        out.push_str(rest);
        out
    }

    /// Replace each sentinel `<div class="tracey-req" data-req-id="X">…</div>`
    /// with `open_html …inner… close_html` from `ctx.badge_for`. The `X` is
    /// looked up in `by_id` (from the tree-sitter parse); if not found the
    /// sentinel wrapper is dropped and the inner body emitted verbatim. Nested
    /// `<div>`s inside the body are handled by depth-counting.
    fn splice_req_badges(
        body: &str,
        by_id: &std::collections::HashMap<String, &marq::ReqDefinition>,
        ctx: &RenderCtx<'_>,
    ) -> String {
        let open_prefix = format!("<div class=\"{REQ_SENTINEL}\" data-req-id=\"");
        let mut out = String::with_capacity(body.len());
        let mut rest = body;
        while let Some(start) = rest.find(&open_prefix) {
            out.push_str(&rest[..start]);
            let after_prefix = &rest[start + open_prefix.len()..];
            // ID runs to the next quote; typst html-escapes attribute values so
            // a literal `"` cannot appear inside.
            let Some(id_end) = after_prefix.find('"') else {
                // Malformed — emit the rest verbatim and stop.
                out.push_str(&rest[start..]);
                return out;
            };
            let id = &after_prefix[..id_end];
            let Some(tag_end_rel) = after_prefix[id_end..].find('>') else {
                out.push_str(&rest[start..]);
                return out;
            };
            let inner_start = start + open_prefix.len() + id_end + tag_end_rel + 1;
            let Some(inner_len) = matching_div_end(&rest[inner_start..]) else {
                out.push_str(&rest[start..]);
                return out;
            };
            let inner = &rest[inner_start..inner_start + inner_len];

            match by_id.get(id) {
                Some(def) => {
                    let (open_html, close_html) = (ctx.badge_for)(def);
                    out.push_str(&open_html);
                    out.push_str(inner);
                    out.push_str(&close_html);
                }
                None => {
                    // tree-sitter and the compiler disagree (e.g. user redefined
                    // `#req`); pass the body through unwrapped.
                    out.push_str(inner);
                }
            }

            rest = &rest[inner_start + inner_len + "</div>".len()..];
        }
        out.push_str(rest);
        out
    }

    /// Given the text immediately after a `<div …>` open tag, return the byte
    /// length of the inner content up to (not including) the matching `</div>`.
    fn matching_div_end(s: &str) -> Option<usize> {
        let bytes = s.as_bytes();
        let mut depth: i32 = 1;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'<' {
                if s[i..].starts_with("</div>") {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                    i += 6;
                    continue;
                } else if s[i..].starts_with("<div") {
                    // Only count as a div open if followed by whitespace or `>`.
                    if let Some(b' ' | b'>' | b'\t' | b'\n' | b'/') = bytes.get(i + 4) {
                        depth += 1;
                    }
                }
            }
            i += 1;
        }
        None
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn strips_tracey_package_imports() {
            let src = "#import \"@preview/tracey:0.1.0\": r\n\
                       #import \"@local/tracey:0.1.0\": req\n\
                       #import \"@preview/other:1.0.0\": x\n\
                       = Title\n";
            let out = strip_tracey_imports(src);
            assert_eq!(out, "\n\n#import \"@preview/other:1.0.0\": x\n= Title\n");
            // No-op fast path returns borrowed.
            assert!(matches!(
                strip_tracey_imports("= Title\n"),
                std::borrow::Cow::Borrowed(_)
            ));
        }

        #[test]
        fn strips_multiline_tracey_import() {
            let src = "#import \"@preview/tracey:0.1.0\": (\n  r, req,\n)\n= Title\n";
            let out = strip_tracey_imports(src);
            // All three import lines blanked, line count preserved.
            assert_eq!(out, "\n\n\n= Title\n");
        }

        #[tokio::test]
        async fn render_with_package_import() {
            let src = "#import \"@preview/tracey:0.1.0\": r\n\n#r(\"a.b\")[Body.]\n";
            let ctx = RenderCtx {
                badge_for: &|d| (format!("<OPEN {}>", d.id), "</CLOSE>".into()),
            };
            let doc = render(src, Path::new("."), &ctx)
                .await
                .expect("render with import");
            assert_eq!(doc.reqs.len(), 1);
            assert!(doc.html.contains("<OPEN a.b>"), "sentinel div spliced");
            assert!(doc.html.contains("Body."));
        }

        /// Relative `#import` resolves against `base_dir`: a helper file on disk
        /// is loaded and its definitions are usable from the in-memory main.
        #[tokio::test]
        async fn render_resolves_relative_import() {
            let dir = tempfile::tempdir().expect("tempdir");
            std::fs::write(dir.path().join("helper.typ"), "#let foo = [helper text]\n")
                .expect("write helper");
            let src = "#import \"helper.typ\": foo\n\n#req(\"a.b\")[Uses #foo here.]\n";
            let ctx = RenderCtx {
                badge_for: &|d| (format!("<OPEN {}>", d.id), "</CLOSE>".into()),
            };
            let doc = render(src, dir.path(), &ctx)
                .await
                .expect("render with relative import");
            assert!(doc.html.contains("<OPEN a.b>"));
            assert!(
                doc.html.contains("helper text"),
                "imported binding should expand into output: {}",
                doc.html
            );
        }

        /// Non-tracey package imports still fail (no package manager).
        #[tokio::test]
        async fn render_rejects_unknown_package_import() {
            let src = "#import \"@preview/other:1.0.0\": x\n";
            let ctx = RenderCtx {
                badge_for: &|_| (String::new(), String::new()),
            };
            let err = render(src, Path::new("."), &ctx)
                .await
                .expect_err("unknown package should not resolve");
            assert!(err.to_string().contains("typst compile failed"));
        }

        #[test]
        fn matching_div_handles_nesting() {
            let s = "a<div>b</div>c</div>tail";
            assert_eq!(matching_div_end(s), Some("a<div>b</div>c".len()));
        }

        #[test]
        fn split_head_body_extracts_inner() {
            let html =
                "<!DOCTYPE html><html><head><style>x</style></head><body><p>hi</p></body></html>";
            let (h, b) = split_head_body(html);
            assert_eq!(h, "<style>x</style>");
            assert_eq!(b, "<p>hi</p>");
        }

        #[test]
        fn heading_ids_injected_in_order() {
            let body = r#"<h1 class="tracey-h">A</h1><h2 class="tracey-h">B</h2>"#;
            let out = inject_heading_ids(body, &["a", "b"]);
            assert_eq!(out, r#"<h1 id="a">A</h1><h2 id="b">B</h2>"#);
        }

        #[test]
        fn splice_replaces_sentinel_div() {
            let def = marq::ReqDefinition {
                id: marq::parse_rule_id("a.b").unwrap(),
                anchor_id: "req-a.b".into(),
                marker_span: marq::SourceSpan { offset: 0, length: 0 },
                span: marq::SourceSpan { offset: 0, length: 0 },
                line: 1,
                metadata: Default::default(),
                raw: String::new(),
                html: String::new(),
            };
            let by_id: std::collections::HashMap<_, _> =
                [("a.b".to_string(), &def)].into_iter().collect();
            let ctx = RenderCtx {
                badge_for: &|d| (format!("<OPEN {}>", d.id), "</CLOSE>".into()),
            };
            let body = r#"<p>x</p><div class="tracey-req" data-req-id="a.b">body<div>n</div></div><p>y</p>"#;
            let out = splice_req_badges(body, &by_id, &ctx);
            assert_eq!(out, "<p>x</p><OPEN a.b>body<div>n</div></CLOSE><p>y</p>");
        }
    }
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
    async fn typst_builtins_are_not_reqs() {
        // `#image`, `#link`, `#figure` all match the `#ident("str")[body]` call
        // shape but are typst stdlib functions, not requirement markers.
        let src = concat!(
            "#image(\"foo.png\")\n",
            "#link(\"https://example.com\")[click me]\n",
            "#figure(\"img.one\")[Caption]\n",
            "#req(\"real.one\")[Body]\n",
        );
        let doc = parse(src).await.unwrap();
        assert_eq!(doc.reqs.len(), 1);
        assert_eq!(doc.reqs[0].id.base, "real.one");
    }

    #[tokio::test]
    async fn long_prefixes_are_accepted() {
        // No length cap on the prefix ident — only the builtin denylist filters.
        let src = "#requirement(\"auth.login\")[Body]\n";
        let doc = parse(src).await.unwrap();
        assert_eq!(doc.reqs.len(), 1);
        assert_eq!(doc.reqs[0].id.base, "auth.login");
        assert_eq!(doc.reqs[0].anchor_id, "requirement-auth.login");
    }

    #[test]
    fn typst_builtins_list_is_sorted() {
        // `binary_search` requires a sorted slice; guard against accidental
        // mis-ordering when the list is extended.
        for w in TYPST_BUILTINS.windows(2) {
            assert!(w[0] < w[1], "TYPST_BUILTINS not sorted: {} >= {}", w[0], w[1]);
        }
    }

    #[tokio::test]
    async fn placeholder_html_escapes() {
        let doc = parse("= <script>").await.unwrap();
        assert!(doc.html.contains("&lt;script&gt;"));
        assert!(doc.html.starts_with("<pre class=\"typst-placeholder\">"));
    }
}
