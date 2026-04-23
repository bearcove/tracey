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

use super::{SlugAllocator, SpecDoc};

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
/// 1. [`parse`] the raw content (tree-sitter) for `reqs` / spans.
/// 2. Prepend a prelude that maps `#req(id)[body]` → a sentinel `<div>` and
///    headings → sentinel `<hN data-base-slug="…">`; compile with
///    `typst::compile<HtmlDocument>`.
/// 3. Post-process the HTML string: replace each sentinel `<div>` with the
///    badge markup from [`RenderCtx::badge_for`], and rewrite each heading
///    sentinel to `<hN id="slug">` using the slugified `data-base-slug`.
///    `headings` and `elements` are rebuilt from sentinel order so headings
///    from `#heading(..)` calls or `#include`d files appear in the outline.
/// 4. Lift `<style>` / `<link>` from the compiler's `<head>` into
///    `head_injections`; return only the `<body>` interior as `html`.
///
/// `source_path` is the path to the spec file itself; relative `#import` /
/// `#include` paths resolve against its parent directory, and the file name is
/// used as the main module's virtual path so a sibling file with the same name
/// is not shadowed. Package imports (`@preview/...`) other than the tracey shim
/// resolve from `package_path` (vendored directory laid out as
/// `<namespace>/<name>/<version>/`), then the system typst data dir (`@local`
/// packages), then the system typst cache. Tracey never downloads packages.
///
/// `alloc` is the cross-file heading-slug allocator; every `tracey-h` sentinel
/// in the compiled HTML claims a slug from it so anchors stay unique across a
/// multi-file spec.
///
/// Behind the `typst-spec` feature. Without it, returns an error and callers
/// should fall back to [`parse`] (placeholder `<pre>` html).
#[cfg_attr(not(feature = "typst-spec"), allow(unused_variables))]
pub async fn render_display(
    content: &str,
    source_path: &std::path::Path,
    package_path: Option<&std::path::Path>,
    ctx: &RenderCtx<'_>,
    alloc: &mut SlugAllocator,
) -> eyre::Result<SpecDoc> {
    #[cfg(not(feature = "typst-spec"))]
    {
        Err(eyre::eyre!(
            "typst HTML rendering not compiled in (enable 'typst-spec' feature)"
        ))
    }
    #[cfg(feature = "typst-spec")]
    {
        compiler::render(content, source_path, package_path, ctx, alloc).await
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
    // Two tree shapes, depending on whether a `[body]` block is present:
    //   with body:    code > call(outer) > [item: call(inner) > [item: ident, group], content]
    //   without body: code > call(outer) > [item: ident, group]
    // Normalise so `inner` is always the call carrying `ident` + `group`.
    let outer = find_child(code, "call")?;
    let item = outer.child_by_field_name("item")?;
    let inner = if item.kind() == "call" { item } else { outer };
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
        anchor_id: super::req_anchor_id(&id.to_string()),
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

/// Reverse the five standard HTML character-reference escapes.
///
/// Typst's HTML serializer escapes attribute values; this recovers the original
/// text so it can be re-parsed (e.g. as a [`marq::RuleId`] from
/// `data-req-id="…"`). `&amp;` is decoded last so an escaped ampersand
/// (`&amp;lt;`) round-trips to `&lt;`, not `<`.
#[cfg_attr(not(feature = "typst-spec"), allow(dead_code))]
pub(super) fn html_unescape(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains('&') {
        return std::borrow::Cow::Borrowed(s);
    }
    std::borrow::Cow::Owned(
        s.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&amp;", "&"),
    )
}

// ---- typst compiler bridge (feature-gated) --------------------------------

#[cfg(feature = "typst-spec")]
mod compiler {
    use super::{RenderCtx, SlugAllocator, SpecDoc, extract_marker_prefix, parse};
    use std::collections::{BTreeSet, HashMap};
    use std::fmt::Write as _;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use typst::diag::{FileError, FileResult, PackageError};
    use typst::foundations::{Bytes, Datetime};
    use typst::syntax::package::PackageSpec;
    use typst::syntax::{FileId, Source, VirtualPath};
    use typst::text::{Font, FontBook};
    use typst::utils::LazyHash;
    use typst::{Feature, Features, Library, LibraryExt as _, World};
    use typst_html::HtmlDocument;
    use typst_kit::fonts::{FontSlot, Fonts};

    /// Subdirectory under the OS cache/data dir where typst stores packages
    /// (matches `typst_kit::package::DEFAULT_PACKAGES_SUBDIR`; reproduced here
    /// to avoid pulling typst-kit's `packages` feature, which drags in a
    /// network downloader).
    const TYPST_PACKAGES_SUBDIR: &str = "typst/packages";

    /// Sentinel CSS classes the prelude emits so post-processing can find
    /// requirement containers and headings without a full HTML parser.
    const REQ_SENTINEL: &str = "tracey-req";
    const HEADING_SENTINEL: &str = "tracey-h";

    /// Build the prelude prepended to every spec before compilation.
    ///
    /// Defines `#req` / `#r` to emit sentinel `<div>`s and rewrites top-level
    /// headings to sentinel `<hN data-base-slug="…" class="tracey-h">` tags.
    /// Typst attribute values must be strings, so the recursive `_ts` helper
    /// flattens the heading body to plain text (walking `.text` / `.children`
    /// / `.body` / `.child`, falling back to `repr` so even math headings
    /// yield a deterministic seed). Post-processing reads `data-base-slug`
    /// directly — no positional correlation with the tree-sitter parse — so
    /// headings emitted by `#heading(..)` calls or `#include`d files can't
    /// shift later slugs.
    ///
    /// `extra_prefixes` are additional aliases for `req` discovered by the
    /// tree-sitter parse (e.g. a spec that uses `#spec(...)` instead of
    /// `#req(...)`). `r` and `req` are always defined; duplicates are ignored.
    ///
    /// The `req` body sets a *nested* `#show heading:` rule that emits plain
    /// `<hN>` (no `tracey-h` class), so a heading written inside a requirement
    /// body never produces a sentinel and never claims a slug from
    /// [`assign_heading_ids`].
    fn build_prelude(extra_prefixes: &[&str]) -> String {
        // Body is the optional trailing content block. Typst has no
        // positional-with-default, so the sink collects it (and any tagged
        // metadata, which the HTML emitter ignores) and `args.pos().join()`
        // yields `none` when absent.
        let mut p = String::from(concat!(
            "#let _ts(it) = {\n",
            "  if type(it) == str { it }\n",
            "  else if type(it) != content { repr(it) }\n",
            "  else if it == [ ] { \" \" }\n",
            "  else if it.has(\"text\") { it.text }\n",
            "  else if it.has(\"children\") { it.children.map(_ts).join() }\n",
            "  else if it.has(\"body\") { _ts(it.body) }\n",
            "  else if it.has(\"child\") { _ts(it.child) }\n",
            "  else { repr(it) }\n",
            "}\n",
            "#let req(id, ..args) = html.elem(\n",
            "  \"div\", attrs: (class: \"tracey-req\", \"data-req-id\": id),\n",
            ")[\n",
            "  #show heading: it => html.elem(",
            "\"h\" + str(calc.min(it.level, 6)))[#it.body]\n",
            "  #args.pos().join()\n",
            "]\n",
            "#let r = req\n",
            "#show heading: it => html.elem(\n",
            "  \"h\" + str(calc.min(it.level, 6)),\n",
            "  attrs: (\"data-base-slug\": _ts(it.body), class: \"tracey-h\"),\n",
            ")[#it.body]\n",
        ));
        for prefix in extra_prefixes {
            if *prefix != "r" && *prefix != "req" {
                let _ = writeln!(p, "#let {prefix} = req");
            }
        }
        p
    }

    pub(super) async fn render(
        content: &str,
        source_path: &Path,
        package_path: Option<&Path>,
        ctx: &RenderCtx<'_>,
        alloc: &mut SlugAllocator,
    ) -> eyre::Result<SpecDoc> {
        // Structural extraction runs on the raw user content so spans / line
        // numbers point at the actual source file, not the prelude-shifted text.
        let mut doc = parse(content).await?;

        // Any prefix the parse recognised as a requirement marker must also be
        // bound in the prelude, or the typst compile will fail with "unknown
        // variable" the moment a spec uses anything other than `r` / `req`.
        let prefixes: BTreeSet<String> = doc
            .reqs
            .iter()
            .filter_map(|r| extract_marker_prefix(content, r.marker_span))
            .collect();
        let prefix_refs: Vec<&str> = prefixes.iter().map(String::as_str).collect();
        let prelude = build_prelude(&prefix_refs);

        let stripped = strip_tracey_imports(content);
        let mut full = String::with_capacity(prelude.len() + stripped.len());
        full.push_str(&prelude);
        full.push_str(&stripped);

        let world = SpecWorld::new(full, source_path, package_path.map(Path::to_path_buf));
        let compiled = typst::compile::<HtmlDocument>(&world);
        // Clear typst's global memoization cache so repeated compilations in a
        // long-running daemon don't accumulate unbounded memory.
        comemo::evict(0);
        let output = compiled.output.map_err(|errs| {
            let mut msg = String::from("typst compile failed:");
            for e in errs.iter() {
                let _ = write!(msg, "\n  {}", e.message);
                // Typst's `PackageError::NotFound` display is terse; add an
                // actionable, namespace-aware hint so it surfaces in the
                // dashboard error.
                if e.message.contains("package not found") {
                    let _ = write!(msg, "\n  hint: {}", package_not_found_hint(&e.message));
                }
            }
            eyre::eyre!(msg)
        })?;
        let html = typst_html::html(&output)
            .map_err(|e| eyre::eyre!("typst html serialize failed: {:?}", e))?;

        let (head, body) = split_head_body(&html);
        doc.head_injections = extract_head_injections(head);

        // Headings and document-order elements are rebuilt from the compiled
        // HTML's sentinels, replacing the tree-sitter-derived ones (which miss
        // `#heading(..)` calls and `#include`d files). Reqs are still looked
        // up from the tree-sitter parse so spans/line numbers stay accurate.
        let by_id: HashMap<marq::RuleId, &marq::ReqDefinition> =
            doc.reqs.iter().map(|r| (r.id.clone(), r)).collect();
        let (body, headings, elements) = assign_heading_ids(body, alloc, &by_id);
        doc.html = splice_req_badges(&body, &by_id, ctx);
        doc.headings = headings;
        doc.elements = elements;

        Ok(doc)
    }

    /// Namespace-aware hint for a "package not found" diagnostic. `message` is
    /// the typst error text, which embeds the full `@ns/name:ver` spec.
    fn package_not_found_hint(message: &str) -> String {
        let prefix = "tracey resolves typst packages offline only. ";
        if message.contains("@local/") {
            let where_ = dirs::data_dir()
                .map(|d| d.join(TYPST_PACKAGES_SUBDIR).display().to_string())
                .unwrap_or_else(|| "<data-dir>/typst/packages".into());
            format!(
                "{prefix}Place the package under `{where_}/local/<name>/<version>/`, \
                 or set `typst_package_path` in the spec config to a vendored \
                 package directory."
            )
        } else if message.contains("@preview/") {
            format!(
                "{prefix}Run `typst compile` once to populate the system cache, \
                 or set `typst_package_path` in the spec config to a vendored \
                 package directory."
            )
        } else {
            format!(
                "{prefix}Set `typst_package_path` in the spec config to a vendored \
                 package directory laid out as `<namespace>/<name>/<version>/`."
            )
        }
    }

    /// Minimal [`World`]: in-memory main source, embedded fonts, offline-only
    /// package resolution. Relative `#import` / `#include` resolve against
    /// `base_dir`; package imports (`@preview/...`, `@local/...`) resolve
    /// against `package_path` (vendored), then `package_data_path` (system data
    /// dir, where `@local` packages live), then `package_cache_path` (system
    /// cache, where `@preview` downloads land) — matching typst-kit's local
    /// probe order.
    struct SpecWorld {
        library: LazyHash<Library>,
        book: LazyHash<FontBook>,
        fonts: Vec<FontSlot>,
        main: Source,
        /// Directory the main spec file lives in; root for relative imports.
        base_dir: PathBuf,
        /// Vendored package root from config (`<ns>/<name>/<ver>/` layout).
        package_path: Option<PathBuf>,
        /// System typst package data dir (e.g. `~/.local/share/typst/packages`).
        package_data_path: Option<PathBuf>,
        /// System typst package cache (e.g. `~/.cache/typst/packages`).
        package_cache_path: Option<PathBuf>,
        /// Disk reads cached per [`FileId`] — typst may request the same file
        /// repeatedly during a compile. Errors are cached too so a missing
        /// import is reported once, not re-stat'd.
        sources: Mutex<HashMap<FileId, FileResult<Source>>>,
        files: Mutex<HashMap<FileId, FileResult<Bytes>>>,
    }

    impl SpecWorld {
        fn new(text: String, source_path: &Path, package_path: Option<PathBuf>) -> Self {
            let features: Features = [Feature::Html].into_iter().collect();
            let library = Library::builder().with_features(features).build();
            let fonts = Fonts::searcher()
                .include_system_fonts(false)
                .include_embedded_fonts(true)
                .search();
            // Use the real file name for the main module's vpath so a sibling
            // file with the same name (e.g. `#import "spec.typ"` next to a spec
            // that *is* `spec.typ` is fine, but `#import "spec.typ"` from
            // `api.typ` must hit disk, not the in-memory main).
            let main_name = source_path
                .file_name()
                .map(std::ffi::OsStr::to_os_string)
                .unwrap_or_else(|| "main.typ".into());
            let id = FileId::new(None, VirtualPath::new(main_name));
            let base_dir = source_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            Self {
                library: LazyHash::new(library),
                book: LazyHash::new(fonts.book),
                fonts: fonts.fonts,
                main: Source::new(id, text),
                base_dir,
                package_path,
                package_data_path: dirs::data_dir().map(|d| d.join(TYPST_PACKAGES_SUBDIR)),
                package_cache_path: dirs::cache_dir().map(|d| d.join(TYPST_PACKAGES_SUBDIR)),
                sources: Mutex::new(HashMap::new()),
                files: Mutex::new(HashMap::new()),
            }
        }

        /// Locate `spec`'s on-disk root by probing the vendored path, then the
        /// system data dir, then the system cache. No network. The directory
        /// must already exist.
        fn package_root(&self, spec: &PackageSpec) -> FileResult<PathBuf> {
            let subdir: PathBuf =
                [spec.namespace.as_str(), spec.name.as_str(), &spec.version.to_string()]
                    .iter()
                    .collect();
            for base in [&self.package_path, &self.package_data_path, &self.package_cache_path]
                .into_iter()
                .flatten()
            {
                let dir = base.join(&subdir);
                if dir.exists() {
                    return Ok(dir);
                }
            }
            Err(FileError::Package(PackageError::NotFound(spec.clone())))
        }

        /// Resolve `id` to an on-disk path and read it as raw bytes. Package ids
        /// resolve via [`package_root`]; non-package paths resolve under
        /// `base_dir` and may not escape it.
        fn read(&self, id: FileId) -> FileResult<Vec<u8>> {
            let root = match id.package() {
                Some(spec) => self.package_root(spec)?,
                None => self.base_dir.clone(),
            };
            let path = id.vpath().resolve(&root).ok_or(FileError::AccessDenied)?;
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

    /// Replace each `<hN data-base-slug="…" class="tracey-h">` with
    /// `<hN id="slug">`, building the heading list and the interleaved
    /// document-order element list directly from the compiled HTML.
    ///
    /// Heading slugs are derived from the `data-base-slug` attribute the
    /// prelude emits (HTML-unescaped, then [`marq::slugify`]d, then run
    /// through `alloc`), so every heading the typst compiler produced —
    /// markup `= Title`, `#heading(..)` calls, `#include`d files — gets a
    /// stable anchor regardless of what tree-sitter could see.
    ///
    /// Req sentinels (`tracey-req`) are *not* rewritten here (that's
    /// [`splice_req_badges`]'s job) but their positions are recorded so the
    /// returned `elements` interleaves headings and reqs in HTML order, which
    /// is what `build_outline` needs to attribute coverage to the right
    /// section. Reqs with no matching definition in `by_id` (e.g. defined in
    /// an `#include`d file tree-sitter didn't parse) are skipped.
    fn assign_heading_ids(
        body: &str,
        alloc: &mut SlugAllocator,
        by_id: &HashMap<marq::RuleId, &marq::ReqDefinition>,
    ) -> (String, Vec<marq::Heading>, Vec<marq::DocElement>) {
        const SLUG_ATTR: &str = "data-base-slug=\"";
        const ID_ATTR: &str = "data-req-id=\"";
        let h_needle = format!("class=\"{HEADING_SENTINEL}\"");
        let r_needle = format!("class=\"{REQ_SENTINEL}\"");

        let mut out = String::with_capacity(body.len());
        let mut headings = Vec::new();
        let mut elements = Vec::new();
        let mut cursor = 0;

        loop {
            let next_h = body[cursor..].find(&h_needle).map(|p| cursor + p);
            let next_r = body[cursor..].find(&r_needle).map(|p| cursor + p);
            // Req sentinel before the next heading sentinel: record it for
            // `elements` ordering and copy through unchanged.
            if let Some(r) = next_r
                && next_h.is_none_or(|h| r < h)
            {
                let tag_end = r + body[r..].find('>').unwrap_or(0);
                if let Some(def) = body[r..tag_end]
                    .find(ID_ATTR)
                    .and_then(|s| {
                        let v = &body[r + s + ID_ATTR.len()..tag_end];
                        v.find('"').map(|e| super::html_unescape(&v[..e]))
                    })
                    .and_then(|id| marq::parse_rule_id(&id))
                    .and_then(|rid| by_id.get(&rid))
                {
                    elements.push(marq::DocElement::Req((*def).clone()));
                }
                out.push_str(&body[cursor..r + r_needle.len()]);
                cursor = r + r_needle.len();
                continue;
            }
            let Some(pos) = next_h else { break };

            // `class="tracey-h"` sits inside a `<hN …>` tag; the most recent
            // `<` is the tag start (typst escapes `<` in attribute values).
            let tag_start = body[..pos].rfind('<').unwrap_or(pos);
            let tag_end = tag_start + body[tag_start..].find('>').unwrap_or(0);
            let tag = &body[tag_start..tag_end];
            let level = tag
                .strip_prefix("<h")
                .and_then(|s| s.bytes().next())
                .filter(u8::is_ascii_digit)
                .map_or(1, |b| b - b'0');

            let title = tag
                .find(SLUG_ATTR)
                .and_then(|s| {
                    let v = &tag[s + SLUG_ATTR.len()..];
                    v.find('"').map(|e| super::html_unescape(&v[..e]).into_owned())
                })
                .unwrap_or_default();
            let base = {
                let s = marq::slugify(&title);
                if s.is_empty() { "section".into() } else { s }
            };
            let slug = alloc.alloc(&base);

            out.push_str(&body[cursor..tag_start]);
            let _ = write!(out, "<h{level} id=\"{slug}\"");
            cursor = tag_end;

            let h = marq::Heading { id: slug, title, level, line: 0 };
            elements.push(marq::DocElement::Heading(h.clone()));
            headings.push(h);
        }
        out.push_str(&body[cursor..]);
        (out, headings, elements)
    }

    /// Replace each sentinel `<div class="tracey-req" data-req-id="X">…</div>`
    /// with `open_html …inner… close_html` from `ctx.badge_for`. The literal
    /// `X` is HTML-unescaped, parsed into a [`marq::RuleId`], and looked up in
    /// `by_id` (from the tree-sitter parse) so `data-req-id="a.b+1"` matches
    /// the version-1 definition keyed as `a.b`. If not found the sentinel
    /// wrapper is dropped and the inner body emitted verbatim. Nested `<div>`s
    /// inside the body are handled by depth-counting.
    fn splice_req_badges(
        body: &str,
        by_id: &HashMap<marq::RuleId, &marq::ReqDefinition>,
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
            let id = super::html_unescape(&after_prefix[..id_end]);
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

            match marq::parse_rule_id(&id).and_then(|rid| by_id.get(&rid)) {
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

        fn alloc() -> SlugAllocator {
            SlugAllocator::default()
        }

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
        async fn render_bodyless_req() {
            let src = "#r(\"a.b\")\n";
            let ctx = RenderCtx {
                badge_for: &|d| (format!("<OPEN {}>", d.id), "</CLOSE>".into()),
            };
            let doc = render(src, Path::new("test.typ"), None, &ctx, &mut alloc())
                .await
                .expect("body-less req should compile");
            assert_eq!(doc.reqs.len(), 1);
            assert!(doc.html.contains("<OPEN a.b>"), "html: {}", doc.html);
        }

        /// Regression: a custom marker prefix (anything other than `r`/`req`)
        /// must compile. Previously the static prelude only defined `r`/`req`,
        /// so `#spec(...)` failed with "unknown variable: spec".
        #[tokio::test]
        async fn render_with_custom_prefix_compiles() {
            let src = "#spec(\"a.b\")[body]\n";
            let ctx = RenderCtx {
                badge_for: &|d| (format!("<OPEN {}>", d.id), "</CLOSE>".into()),
            };
            let doc = render(src, Path::new("test.typ"), None, &ctx, &mut alloc())
                .await
                .expect("custom prefix should compile");
            assert_eq!(doc.reqs.len(), 1);
            assert!(doc.html.contains("<OPEN a.b>"), "html: {}", doc.html);
            assert!(doc.html.contains("body"));
        }

        /// Regression: a heading inside a `#req` body must not emit a
        /// `tracey-h` sentinel and must not steal the next top-level heading's
        /// slug. Previously `== Inner` would consume slug `b`, leaving `= B`
        /// unanchored.
        #[tokio::test]
        async fn render_heading_inside_req_body() {
            let src = "= A\n#req(\"x\")[\n== Inner\nbody\n]\n= B\n";
            let ctx = RenderCtx {
                badge_for: &|d| (format!("<OPEN {}>", d.id), "</CLOSE>".into()),
            };
            let doc = render(src, Path::new("test.typ"), None, &ctx, &mut alloc())
                .await
                .expect("render with inner heading");
            // Top-level headings get allocator slugs.
            assert!(doc.html.contains(r#"<h1 id="a">"#), "html: {}", doc.html);
            // `= B` is the second top-level heading and must get `id="b"`, not
            // be shifted by the inner `== Inner`.
            let b_pos = doc.html.rfind("<h1").expect("second h1");
            assert!(
                doc.html[b_pos..].starts_with(r#"<h1 id="b">"#),
                "last h1 must carry id=\"b\": {}",
                &doc.html[b_pos..]
            );
            // Inner heading is rendered as a plain <h2> with no sentinel class
            // and no allocator-issued id.
            assert!(
                !doc.html.contains("tracey-h"),
                "no sentinel class should survive post-processing"
            );
            assert!(doc.html.contains("<h2>Inner</h2>"), "html: {}", doc.html);
            // Outline entries match the HTML anchors.
            assert_eq!(doc.headings.len(), 2);
            assert_eq!(doc.headings[0].id, "a");
            assert_eq!(doc.headings[1].id, "b");
        }

        /// Regression: headings the tree-sitter pre-parse cannot see —
        /// `#heading(..)` calls and headings from `#include`d files — must not
        /// shift later markup-heading slugs. Each sentinel self-describes via
        /// `data-base-slug`, so `== Second` always gets `id="second"` regardless
        /// of how many compiler-only headings precede it. The outline picks up
        /// every emitted heading in HTML order.
        #[tokio::test]
        async fn render_heading_slugs_from_sentinels() {
            let dir = tempfile::tempdir().expect("tempdir");
            std::fs::write(dir.path().join("front.typ"), "= Preface\n").expect("write front");
            let src = concat!(
                "#include \"front.typ\"\n",
                "= *Bold* title\n",
                "= Q&A\n",
                "#heading(level: 2)[From Call]\n",
                "== Second\n",
                "#req(\"a.b\")[body]\n",
            );
            let ctx = RenderCtx {
                badge_for: &|d| (format!("<OPEN {}>", d.id), "</CLOSE>".into()),
            };
            let doc = render(src, &dir.path().join("main.typ"), None, &ctx, &mut alloc())
                .await
                .expect("render with compiler-only headings");

            // Every heading gets the slug derived from *its own* text.
            assert!(doc.html.contains(r#"<h1 id="preface">"#), "html: {}", doc.html);
            assert!(doc.html.contains(r#"<h1 id="bold-title">"#), "html: {}", doc.html);
            assert!(doc.html.contains(r#"<h1 id="q-a">"#), "html: {}", doc.html);
            assert!(doc.html.contains(r#"<h2 id="from-call">"#), "html: {}", doc.html);
            // The bug this guards against: previously `== Second` would receive
            // a slug shifted by the preceding `#include` + `#heading` sentinels.
            assert!(
                doc.html.contains(r#"<h2 id="second">"#),
                "markup heading slug must not be shifted: {}",
                doc.html
            );
            assert!(!doc.html.contains("tracey-h"));
            assert!(!doc.html.contains("data-base-slug"));

            // Outline (from `doc.elements`) lists every heading in HTML order,
            // including the ones tree-sitter couldn't see, and ends with the req.
            let ids: Vec<_> = doc.headings.iter().map(|h| h.id.as_str()).collect();
            assert_eq!(ids, ["preface", "bold-title", "q-a", "from-call", "second"]);
            // `_ts` flattens rich content to readable plain text for the
            // outline title (space between words preserved, entities decoded).
            assert_eq!(doc.headings[1].title, "Bold title");
            assert_eq!(doc.headings[2].title, "Q&A");
            assert_eq!(doc.elements.len(), 6);
            assert!(matches!(doc.elements[5], marq::DocElement::Req(_)));
            // All anchors unique and non-empty.
            let mut seen = std::collections::HashSet::new();
            for h in &doc.headings {
                assert!(!h.id.is_empty());
                assert!(seen.insert(h.id.clone()), "duplicate slug: {}", h.id);
            }
        }

        #[tokio::test]
        async fn render_with_package_import() {
            let src = "#import \"@preview/tracey:0.1.0\": r\n\n#r(\"a.b\")[Body.]\n";
            let ctx = RenderCtx {
                badge_for: &|d| (format!("<OPEN {}>", d.id), "</CLOSE>".into()),
            };
            let doc = render(src, Path::new("test.typ"), None, &ctx, &mut alloc())
                .await
                .expect("render with import");
            assert_eq!(doc.reqs.len(), 1);
            assert!(doc.html.contains("<OPEN a.b>"), "sentinel div spliced");
            assert!(doc.html.contains("Body."));
        }

        /// Relative `#import` resolves against the source file's directory: a
        /// helper file on disk is loaded and its definitions are usable from the
        /// in-memory main.
        #[tokio::test]
        async fn render_resolves_relative_import() {
            let dir = tempfile::tempdir().expect("tempdir");
            std::fs::write(dir.path().join("helper.typ"), "#let foo = [helper text]\n")
                .expect("write helper");
            let src = "#import \"helper.typ\": foo\n\n#req(\"a.b\")[Uses #foo here.]\n";
            let ctx = RenderCtx {
                badge_for: &|d| (format!("<OPEN {}>", d.id), "</CLOSE>".into()),
            };
            let doc = render(src, &dir.path().join("main.typ"), None, &ctx, &mut alloc())
                .await
                .expect("render with relative import");
            assert!(doc.html.contains("<OPEN a.b>"));
            assert!(
                doc.html.contains("helper text"),
                "imported binding should expand into output: {}",
                doc.html
            );
        }

        /// The main file's vpath is its real file name, so a sibling literally
        /// named `spec.typ` is read from disk rather than aliased to the
        /// in-memory main source. Regression for the hardcoded-`spec.typ` bug.
        #[tokio::test]
        async fn render_resolves_sibling_named_spec_typ() {
            let dir = tempfile::tempdir().expect("tempdir");
            std::fs::write(dir.path().join("spec.typ"), "#let foo = [sibling text]\n")
                .expect("write sibling");
            let src = "#import \"spec.typ\": foo\n\n#req(\"a.b\")[#foo]\n";
            let ctx = RenderCtx {
                badge_for: &|d| (format!("<OPEN {}>", d.id), "</CLOSE>".into()),
            };
            let doc = render(src, &dir.path().join("api.typ"), None, &ctx, &mut alloc())
                .await
                .expect("sibling spec.typ should resolve from disk");
            assert!(
                doc.html.contains("sibling text"),
                "sibling spec.typ should not be shadowed by main: {}",
                doc.html
            );
            assert!(doc.html.contains("<OPEN a.b>"));
        }

        /// Package import resolves from a vendored `<ns>/<name>/<ver>/` tree
        /// passed via `package_path`.
        #[tokio::test]
        async fn render_resolves_vendored_package() {
            let dir = tempfile::tempdir().expect("tempdir");
            let pkg = dir.path().join("preview/testpkg/0.1.0");
            std::fs::create_dir_all(&pkg).expect("mkdir");
            std::fs::write(
                pkg.join("typst.toml"),
                "[package]\nname = \"testpkg\"\nversion = \"0.1.0\"\nentrypoint = \"lib.typ\"\n",
            )
            .expect("write manifest");
            std::fs::write(pkg.join("lib.typ"), "#let hello = [world]\n").expect("write lib");

            let src =
                "#import \"@preview/testpkg:0.1.0\": hello\n= Test\n#r(\"a.b\")[#hello]\n";
            let ctx = RenderCtx {
                badge_for: &|d| (format!("<OPEN {}>", d.id), "</CLOSE>".into()),
            };
            let doc = render(src, Path::new("test.typ"), Some(dir.path()), &ctx, &mut alloc())
                .await
                .expect("render with vendored package");
            assert!(
                doc.html.contains("world"),
                "vendored binding should expand into output: {}",
                doc.html
            );
            assert!(doc.html.contains("<OPEN a.b>"));
        }

        // `@local` package resolution probes `dirs::data_dir()`. Exercising that
        // in-process would require mutating `XDG_DATA_HOME`, which is `unsafe`
        // in edition 2024 and racy under parallel test execution. The probe is a
        // one-line addition to the same loop covered by
        // `render_resolves_vendored_package`; the namespace-aware error hint is
        // checked below.

        /// A package not present anywhere yields a helpful error naming the
        /// package and the offline-only resolution. Hints are namespace-aware:
        /// `@preview` suggests populating the cache, `@local` points at the
        /// data dir, anything else falls back to the vendored-path hint.
        #[tokio::test]
        async fn render_rejects_unknown_package_import() {
            let dir = tempfile::tempdir().expect("tempdir");
            let ctx = RenderCtx {
                badge_for: &|_| (String::new(), String::new()),
            };

            let err = render(
                "#import \"@preview/tracey-nosuch:1.0.0\": x\n",
                Path::new("test.typ"),
                Some(dir.path()),
                &ctx,
                &mut alloc(),
            )
            .await
            .expect_err("unknown @preview package should not resolve");
            let msg = err.to_string();
            assert!(msg.contains("typst compile failed"));
            assert!(msg.contains("@preview/tracey-nosuch:1.0.0"), "names the package: {msg}");
            assert!(msg.contains("typst_package_path"), "actionable hint: {msg}");
            assert!(msg.contains("`typst compile`"), "@preview hints cache: {msg}");

            let err = render(
                "#import \"@local/tracey-nosuch:1.0.0\": x\n",
                Path::new("test.typ"),
                Some(dir.path()),
                &ctx,
                &mut alloc(),
            )
            .await
            .expect_err("unknown @local package should not resolve");
            let msg = err.to_string();
            assert!(msg.contains("@local/tracey-nosuch:1.0.0"), "names the package: {msg}");
            assert!(
                msg.contains("/local/<name>/<version>/"),
                "@local hints data dir layout: {msg}"
            );

            let err = render(
                "#import \"@custom/tracey-nosuch:1.0.0\": x\n",
                Path::new("test.typ"),
                Some(dir.path()),
                &ctx,
                &mut alloc(),
            )
            .await
            .expect_err("unknown @custom package should not resolve");
            let msg = err.to_string();
            assert!(
                msg.contains("<namespace>/<name>/<version>/"),
                "other ns hints vendored layout: {msg}"
            );
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
        fn heading_ids_from_base_slug_attr() {
            let body = r#"<h1 data-base-slug="A" class="tracey-h">A</h1><h2 data-base-slug="Q&amp;A" class="tracey-h">Q&amp;A</h2>"#;
            let (out, hs, els) = assign_heading_ids(body, &mut alloc(), &HashMap::new());
            assert_eq!(out, r#"<h1 id="a">A</h1><h2 id="q-a">Q&amp;A</h2>"#);
            assert_eq!(hs.len(), 2);
            assert_eq!(hs[0].id, "a");
            assert_eq!(hs[0].title, "A");
            assert_eq!(hs[0].level, 1);
            // HTML-unescaped before slugifying.
            assert_eq!(hs[1].id, "q-a");
            assert_eq!(hs[1].title, "Q&A");
            assert_eq!(els.len(), 2);
        }

        #[test]
        fn heading_ids_missing_slug_gets_placeholder() {
            // Sentinel with no `data-base-slug` (or one that slugifies empty)
            // still gets a valid, unique anchor.
            let body = r#"<h1 class="tracey-h">.</h1><h2 data-base-slug="" class="tracey-h">x</h2>"#;
            let (out, hs, _) = assign_heading_ids(body, &mut alloc(), &HashMap::new());
            assert_eq!(out, r#"<h1 id="section">.</h1><h2 id="section-2">x</h2>"#);
            assert_eq!(hs[0].id, "section");
            assert_eq!(hs[1].id, "section-2");
        }

        #[test]
        fn heading_ids_interleave_with_req_sentinels() {
            // Elements must reflect HTML order: H, R, H — so the outline
            // attributes the req to the first heading, not the second.
            let def = marq::ReqDefinition {
                id: marq::parse_rule_id("a.b").unwrap(),
                anchor_id: "r--a.b".into(),
                marker_span: marq::SourceSpan { offset: 0, length: 0 },
                span: marq::SourceSpan { offset: 0, length: 0 },
                line: 1,
                metadata: Default::default(),
                raw: String::new(),
                html: String::new(),
            };
            let by_id: HashMap<_, _> = [(def.id.clone(), &def)].into_iter().collect();
            let body = concat!(
                r#"<h1 data-base-slug="One" class="tracey-h">One</h1>"#,
                r#"<div class="tracey-req" data-req-id="a.b">body</div>"#,
                r#"<h1 data-base-slug="Two" class="tracey-h">Two</h1>"#,
            );
            let (out, hs, els) = assign_heading_ids(body, &mut alloc(), &by_id);
            // Req sentinel passes through unchanged for splice_req_badges.
            assert!(out.contains(r#"<div class="tracey-req" data-req-id="a.b">"#));
            assert_eq!(hs.len(), 2);
            assert_eq!(els.len(), 3);
            assert!(matches!(els[0], marq::DocElement::Heading(_)));
            assert!(matches!(els[1], marq::DocElement::Req(_)));
            assert!(matches!(els[2], marq::DocElement::Heading(_)));
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
            let by_id: HashMap<_, _> = [(def.id.clone(), &def)].into_iter().collect();
            let ctx = RenderCtx {
                badge_for: &|d| (format!("<OPEN {}>", d.id), "</CLOSE>".into()),
            };
            let body = r#"<p>x</p><div class="tracey-req" data-req-id="a.b">body<div>n</div></div><p>y</p>"#;
            let out = splice_req_badges(body, &by_id, &ctx);
            assert_eq!(out, "<p>x</p><OPEN a.b>body<div>n</div></CLOSE><p>y</p>");
        }

        /// Regression: `data-req-id="a.b+1"` (literal `+1` suffix) must match
        /// the version-1 definition keyed as `RuleId{base:"a.b",version:1}`.
        /// Previously `by_id` was keyed on `id.to_string()` (= `"a.b"`), so the
        /// literal `"a.b+1"` missed and the badge was silently dropped.
        #[test]
        fn splice_normalises_versioned_id() {
            let def = marq::ReqDefinition {
                id: marq::parse_rule_id("a.b+1").unwrap(),
                anchor_id: "r--a.b".into(),
                marker_span: marq::SourceSpan { offset: 0, length: 0 },
                span: marq::SourceSpan { offset: 0, length: 0 },
                line: 1,
                metadata: Default::default(),
                raw: String::new(),
                html: String::new(),
            };
            let by_id: HashMap<_, _> = [(def.id.clone(), &def)].into_iter().collect();
            let ctx = RenderCtx {
                badge_for: &|d| (format!("<OPEN {}>", d.anchor_id), "</CLOSE>".into()),
            };
            // Literal `+1` plus an HTML-escaped `&` in the base.
            let body = r#"<div class="tracey-req" data-req-id="a.b+1">body</div>"#;
            assert_eq!(
                splice_req_badges(body, &by_id, &ctx),
                "<OPEN r--a.b>body</CLOSE>"
            );
        }

        /// End-to-end: a `+1` suffix in source survives the typst→HTML→splice
        /// round trip and the badge wraps the body.
        #[tokio::test]
        async fn render_req_with_explicit_version_suffix() {
            let src = "#req(\"a.b+1\")[body text]\n";
            let ctx = RenderCtx {
                badge_for: &|d| {
                    (format!("<div id=\"{}\">", d.anchor_id), "</div>".into())
                },
            };
            let doc = render(src, Path::new("test.typ"), None, &ctx, &mut alloc())
                .await
                .expect("render with +1 suffix");
            assert_eq!(doc.reqs.len(), 1);
            assert_eq!(doc.reqs[0].anchor_id, "r--a.b");
            assert!(
                doc.html.contains("id=\"r--a.b\""),
                "badge container with normalised anchor must appear: {}",
                doc.html
            );
            assert!(doc.html.contains("body text"));
        }
    }
}

// ---- dispatch arms --------------------------------------------------------

/// Word-level inline diff of two typst rule bodies.
///
/// Produces markdown markup matching `marq::diff_markdown_inline`: removed
/// runs wrapped in `~~strikethrough~~`, added runs in `**bold**`. The output
/// is embedded into markdown LSP hovers / CLI output, so it intentionally
/// emits markdown rather than typst.
///
/// Typst rule bodies are predominantly prose, so we diff on whitespace-split
/// words rather than parsing the typst AST. This loses formatting nuance
/// (e.g. `*emph*` is treated as a word) but matches the granularity the
/// markdown backend offers and is good enough for "what changed in this
/// rule" hovers.
pub(super) fn diff_inline(old: &str, new: &str) -> Option<String> {
    let old_words: Vec<&str> = old.split_whitespace().collect();
    let new_words: Vec<&str> = new.split_whitespace().collect();

    // LCS table.
    let m = old_words.len();
    let n = new_words.len();
    let mut table = vec![0u32; (m + 1) * (n + 1)];
    let idx = |i: usize, j: usize| i * (n + 1) + j;
    for i in 1..=m {
        for j in 1..=n {
            table[idx(i, j)] = if old_words[i - 1] == new_words[j - 1] {
                table[idx(i - 1, j - 1)] + 1
            } else {
                table[idx(i - 1, j)].max(table[idx(i, j - 1)])
            };
        }
    }

    // Backtrack into (equal | removed | added) ops.
    #[derive(Clone, Copy)]
    enum Op<'a> {
        Eq(&'a str),
        Rm(&'a str),
        Add(&'a str),
    }
    let mut ops = Vec::with_capacity(m.max(n));
    let (mut i, mut j) = (m, n);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old_words[i - 1] == new_words[j - 1] {
            ops.push(Op::Eq(old_words[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || table[idx(i, j - 1)] >= table[idx(i - 1, j)]) {
            ops.push(Op::Add(new_words[j - 1]));
            j -= 1;
        } else {
            ops.push(Op::Rm(old_words[i - 1]));
            i -= 1;
        }
    }
    ops.reverse();

    // Render, coalescing consecutive removed/added runs so the markup reads
    // `~~old words~~ **new words**` rather than per-word noise.
    let mut out = String::new();
    let mut removed: Vec<&str> = Vec::new();
    let mut added: Vec<&str> = Vec::new();
    let push_sep = |out: &mut String| {
        if !out.is_empty() {
            out.push(' ');
        }
    };
    let flush = |out: &mut String, removed: &mut Vec<&str>, added: &mut Vec<&str>| {
        if !removed.is_empty() {
            push_sep(out);
            out.push_str("~~");
            out.push_str(&removed.join(" "));
            out.push_str("~~");
            removed.clear();
        }
        if !added.is_empty() {
            push_sep(out);
            out.push_str("**");
            out.push_str(&added.join(" "));
            out.push_str("**");
            added.clear();
        }
    };
    for op in ops {
        match op {
            Op::Eq(w) => {
                flush(&mut out, &mut removed, &mut added);
                push_sep(&mut out);
                out.push_str(w);
            }
            Op::Rm(w) => removed.push(w),
            Op::Add(w) => added.push(w),
        }
    }
    flush(&mut out, &mut removed, &mut added);

    Some(out)
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

/// Locate the positional id string inside a `#prefix("id", ..)` marker.
///
/// Re-parses `marker_str` with tree-sitter and returns the byte range of the
/// id *contents* (between the quotes). Named arguments are `tagged` nodes in
/// the parse tree, so a leading `level: "shall"` or `supersedes: "old.id"` is
/// skipped — the first *direct* `string` child of the argument `group` is the
/// positional id, regardless of where it sits in the argument list.
///
/// `marker_str` is the exact `marker_span` slice (e.g. `#req("a.b", level:
/// "shall")`, no `[body]`); it is small, so the throwaway parse is cheap.
pub(super) fn id_range_in_marker(marker_str: &str) -> eyre::Result<std::ops::Range<usize>> {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_typst::language().into())
        .map_err(|e| eyre::eyre!("failed to load typst grammar: {e}"))?;
    let tree = parser
        .parse(marker_str, None)
        .ok_or_else(|| eyre::eyre!("typst parser returned no tree"))?;

    let mut range = None;
    walk(tree.root_node(), &mut |node| {
        if range.is_some() {
            return false;
        }
        if node.kind() == "group" {
            if let Some(id) = find_child(node, "string") {
                // `string` node span includes the surrounding quotes.
                range = Some(id.start_byte() + 1..id.end_byte() - 1);
            }
            return false;
        }
        true
    });
    range.ok_or_else(|| {
        eyre::eyre!("malformed typst marker (no positional id string): {marker_str}")
    })
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
        assert_eq!(r0.anchor_id, "r--auth.login");
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
        assert_eq!(r1.anchor_id, "r--auth.session+2");
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

    /// Locate the id in a marker and splice via [`super::rewrite_marker`].
    fn rewrite(m: &str, base: &str, ver: u32) -> String {
        let r = id_range_in_marker(m).unwrap();
        super::super::rewrite_marker(m, r, base, ver).unwrap()
    }

    #[test]
    fn rewrites_marker() {
        assert_eq!(
            rewrite("#req(\"auth.login\")", "auth.login", 2),
            "#req(\"auth.login+2\")"
        );
        assert_eq!(
            rewrite("#r(\"auth.login+3\")", "auth.login", 4),
            "#r(\"auth.login+4\")"
        );
    }

    #[test]
    fn rewrites_marker_preserves_metadata() {
        assert_eq!(
            rewrite("#req(\"auth.login+1\", level: \"shall\")", "auth.login", 2),
            "#req(\"auth.login+2\", level: \"shall\")"
        );
        assert_eq!(
            rewrite("#r(\"a.b\", level: \"may\", status: \"draft\")", "a.b", 3),
            "#r(\"a.b+3\", level: \"may\", status: \"draft\")"
        );
    }

    /// Regression: a named string argument *before* the positional id must not
    /// be mistaken for the id. Previously `find('"')` grabbed the first quote,
    /// rewriting `level:` instead of the id.
    #[test]
    fn rewrites_marker_with_leading_named_string_arg() {
        assert_eq!(
            rewrite("#req(level: \"shall\", \"a.b\")", "a.b", 2),
            "#req(level: \"shall\", \"a.b+2\")"
        );
        // Named arg whose value looks like an id — must still be skipped.
        assert_eq!(
            rewrite("#req(alias: \"a.b.legacy\", \"a.b\")", "a.b", 2),
            "#req(alias: \"a.b.legacy\", \"a.b+2\")"
        );
        // Named arg whose value *contains* the base — must still be skipped.
        assert_eq!(
            rewrite(
                "#req(supersedes: \"auth.login\", \"auth.login+2\")",
                "auth.login",
                3
            ),
            "#req(supersedes: \"auth.login\", \"auth.login+3\")"
        );
    }

    #[test]
    fn id_range_on_non_marker_errors() {
        assert!(id_range_in_marker("not a marker").is_err());
        assert!(id_range_in_marker("#req(level: 1)").is_err());
    }

    #[test]
    fn html_unescape_roundtrip() {
        assert_eq!(html_unescape("a.b+1"), "a.b+1");
        assert_eq!(html_unescape("a&amp;b"), "a&b");
        assert_eq!(html_unescape("&lt;x&gt;&quot;y&quot;&#39;z&#39;"), "<x>\"y\"'z'");
        // Escaped ampersand stays single-decoded.
        assert_eq!(html_unescape("&amp;lt;"), "&lt;");
    }

    #[tokio::test]
    async fn parses_req_with_leading_named_arg() {
        // Named args are `tagged` nodes; the positional id `string` is a direct
        // child of `group` regardless of order, so `find_child` skips past the
        // `level:` value and locates `"a.b"`.
        let src = "#req(level: \"shall\", \"a.b\")[body]\n";
        let doc = parse(src).await.unwrap();
        assert_eq!(doc.reqs.len(), 1);
        let r = &doc.reqs[0];
        assert_eq!(r.id.base, "a.b");
        assert_eq!(r.metadata.level, Some(ReqLevel::Must));
        let m = &src[r.marker_span.offset..r.marker_span.offset + r.marker_span.length];
        assert_eq!(m, "#req(level: \"shall\", \"a.b\")");
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

    #[test]
    fn diff_inline_word_change() {
        let out = diff_inline("old text here", "new text here").unwrap();
        assert!(out.contains("~~old~~"), "strikes removed word: {out}");
        assert!(out.contains("**new**"), "bolds added word: {out}");
        assert!(out.contains("text here"), "keeps unchanged words: {out}");
    }

    #[test]
    fn diff_inline_identical() {
        let out = diff_inline("same text", "same text").unwrap();
        assert_eq!(out, "same text");
        assert!(!out.contains("~~"));
        assert!(!out.contains("**"));
    }

    #[test]
    fn diff_inline_coalesces_runs() {
        let out = diff_inline(
            "Sessions expire after one hour.",
            "Sessions expire after twenty four hours.",
        )
        .unwrap();
        assert!(
            out.contains("~~one hour.~~"),
            "coalesces removed run: {out}"
        );
        assert!(
            out.contains("**twenty four hours.**"),
            "coalesces added run: {out}"
        );
    }

    #[test]
    fn diff_inline_full_replace() {
        let out = diff_inline("alpha", "beta gamma").unwrap();
        assert_eq!(out, "~~alpha~~ **beta gamma**");
    }

    #[tokio::test]
    async fn long_prefixes_are_accepted() {
        // No length cap on the prefix ident — only the builtin denylist filters.
        let src = "#requirement(\"auth.login\")[Body]\n";
        let doc = parse(src).await.unwrap();
        assert_eq!(doc.reqs.len(), 1);
        assert_eq!(doc.reqs[0].id.base, "auth.login");
        assert_eq!(doc.reqs[0].anchor_id, "r--auth.login");
    }

    #[tokio::test]
    async fn parses_bodyless_req() {
        // Bare `#r("id")` with no `[body]` block parses as a single (non-nested)
        // call; it must still be extracted as a definition with empty body.
        let src = "#r(\"auth.stub\")\n";
        let doc = parse(src).await.unwrap();
        assert_eq!(doc.reqs.len(), 1);
        let r = &doc.reqs[0];
        assert_eq!(r.id.base, "auth.stub");
        assert_eq!(r.anchor_id, "r--auth.stub");
        assert_eq!(r.raw, "");
        // marker_span and span both cover exactly `#r("auth.stub")`.
        let m = &src[r.marker_span.offset..r.marker_span.offset + r.marker_span.length];
        assert_eq!(m, "#r(\"auth.stub\")");
        let s = &src[r.span.offset..r.span.offset + r.span.length];
        assert_eq!(s, "#r(\"auth.stub\")");
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
