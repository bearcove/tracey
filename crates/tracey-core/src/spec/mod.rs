//! Spec document abstraction layer.
//!
//! Tracey supports spec documents written in multiple formats. This module
//! provides a small enum-dispatched facade over the per-format implementations
//! so callers never need to know which concrete parser is in play.
//!
//! Backends implement [`SpecBackend`]; dispatch goes through the registry in
//! [`registry`].

use std::ffi::OsStr;
use std::ops::Range;
use std::path::{Path, PathBuf};

mod markdown;
mod registry;
pub mod typst;

pub use registry::{ErasedConfig, NoConfig, SpecConfigs};
pub use typst::TypstConfig;

// Re-export the marq types that callers interact with regardless of format.
// `SpecDoc` is a type alias for `marq::Document` — see Spike C in NOTES: all
// fields are public, so non-markdown backends can construct one directly.
pub use marq::{DocElement, InlineCodeSpan, ReqDefinition, RuleId as SpecRuleId, SourceSpan};

/// Parsed spec document. Every backend produces this shape.
pub type SpecDoc = marq::Document;

/// File extensions recognised as spec documents (any format).
pub const SPEC_EXTENSIONS: &[&str] = &["md", "markdown", "typ"];

/// Prefix for requirement anchor IDs in rendered HTML (`id="r--auth.login"`).
///
/// Every spec backend MUST emit [`ReqDefinition::anchor_id`] in this shape so
/// the dashboard, static export, and inter-spec links can address requirements
/// without knowing which backend produced them.
pub const REQ_ANCHOR_PREFIX: &str = "r--";

/// Build the HTML anchor id for a requirement (`"r--{id}"`).
pub fn req_anchor_id(id: &str) -> String {
    format!("{REQ_ANCHOR_PREFIX}{id}")
}

/// Recover the requirement id from an anchor id, or `None` if `anchor` is not a
/// requirement anchor.
pub fn req_anchor_to_id(anchor: &str) -> Option<&str> {
    anchor.strip_prefix(REQ_ANCHOR_PREFIX)
}

/// Allocates globally-unique heading slugs across a multi-file spec.
///
/// Each backend slugifies headings independently, so two files (or two format
/// runs) can produce the same slug. Threading a single allocator through the
/// whole render keeps every anchor unique without a post-hoc dedup pass.
#[derive(Default)]
pub struct SlugAllocator {
    /// Next suffix to try per base (1 = bare, 2 = `-2`, …).
    next: std::collections::HashMap<String, usize>,
    /// Every slug ever returned. A literal `foo-2` input must not collide with
    /// a suffix we already handed out for `foo`, so the suffix probe consults
    /// this set rather than just the per-base counter.
    emitted: std::collections::HashSet<String>,
}

impl SlugAllocator {
    /// Accepts any string; returns a slug unique among all prior `alloc`
    /// results and never starting with [`REQ_ANCHOR_PREFIX`].
    ///
    /// Inputs already in the requirement-anchor namespace are rewritten
    /// (`r--foo` → `h-foo`, bare `r--` → `section`) so heading anchors and
    /// requirement anchors stay disjoint regardless of how the caller built
    /// the slug — marq's hierarchical ids join parent and child with `--`, so
    /// `# R` + `## Design` legitimately yields `r--design`. Other `--` joins
    /// (e.g. `auth--login`) pass through untouched. Repeats are suffixed
    /// `-2`, `-3`, …, skipping any value already emitted.
    pub fn alloc(&mut self, base: &str) -> String {
        let base = match base.strip_prefix(REQ_ANCHOR_PREFIX) {
            Some("") => "section".to_owned(),
            Some(rest) => format!("h-{rest}"),
            None => base.to_owned(),
        };

        let mut n = self.next.get(&base).copied().unwrap_or(1);
        loop {
            let candidate = if n == 1 {
                base.clone()
            } else {
                format!("{base}-{n}")
            };
            n += 1;
            if self.emitted.insert(candidate.clone()) {
                self.next.insert(base, n);
                return candidate;
            }
        }
    }
}

/// Which spec dialect a file is written in.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SpecFormat {
    /// CommonMark + tracey marker syntax, parsed via `marq`.
    Markdown,
    /// Typst markup with `#req(...)` calls.
    Typst,
}

impl SpecFormat {
    /// The registered backend for this format.
    ///
    /// Crate-private because [`DynBackend`](registry::DynBackend) is the
    /// internal erasure layer; callers go through [`parse_spec`] etc.
    pub(crate) fn backend(self) -> &'static dyn registry::DynBackend {
        registry::BACKENDS
            .iter()
            .copied()
            .find(|b| b.format() == self)
            .expect("every SpecFormat variant has a registered backend")
    }

    /// Classify a path by its extension.
    pub fn from_path(p: &Path) -> Option<Self> {
        Self::from_ext(p.extension()?)
    }

    /// Classify a bare extension (no leading dot).
    pub fn from_ext(ext: &OsStr) -> Option<Self> {
        let s = ext.to_str()?;
        registry::BACKENDS
            .iter()
            .find(|b| b.extensions().contains(&s))
            .map(|b| b.format())
    }

    /// Stable lowercase identifier — used for the tantivy index, logs, and the
    /// `[spec.<name>]` config table key.
    pub fn name(self) -> &'static str {
        self.backend().name()
    }

    /// Inverse of [`name`](Self::name).
    pub fn from_name(name: &str) -> Option<Self> {
        registry::BACKENDS
            .iter()
            .find(|b| b.name() == name)
            .map(|b| b.format())
    }
}

/// Callback returning `(open_html, close_html)` to wrap a requirement body.
///
/// `Arc` + `'static` so backends can hand it to `marq::ReqHandler` (which
/// boxes handlers as `'static`). Callers must move owned data into the
/// closure rather than borrow from the stack.
pub type BadgeFn = std::sync::Arc<dyn Fn(&ReqDefinition) -> (String, String) + Send + Sync>;

/// One spec source file fed to [`SpecBackend::render_html`].
pub struct RenderSource<'a> {
    pub path: &'a Path,
    pub content: &'a str,
}

/// Inputs to [`SpecBackend::render_html`].
///
/// `sources` is a run of one or more same-format files. Backends that render
/// per-file (typst, sdoc) iterate; markdown concatenates the run to preserve
/// its unified-TOC behaviour.
pub struct RenderInput<'a> {
    pub sources: &'a [RenderSource<'a>],
    /// Project root, for resolving relative `#import` / `include::`.
    pub root: &'a Path,
    /// Coverage badge HTML to inject around each requirement: `(open, close)`.
    /// Typically wraps the body in `<div class="req-container">…badges…<body></div></div>`.
    ///
    /// `Arc` because marq's `with_req_handler` requires `'static`; the markdown
    /// backend clones this into a [`marq::ReqHandler`] adapter.
    pub badge_for: BadgeFn,
    /// Cross-file heading-slug deduplicator (shared across the whole build).
    pub slugs: &'a mut SlugAllocator,
}

/// Output of [`SpecBackend::render_html`].
#[derive(Debug)]
pub struct RenderOutput {
    pub html: String,
    /// Extra files read during render (imports/includes) — fed to the
    /// file-watcher and cache-key.
    pub deps: Vec<PathBuf>,
}

/// Per-format spec backend.
///
/// Format authors implement this trait once per format under
/// `crates/tracey-core/src/spec/<fmt>.rs`. The associated [`Config`] type
/// declares the backend's `[spec.<name>]` config-table schema; use
/// [`NoConfig`] when none is needed.
///
/// The trait is intentionally **not** object-safe (`Config` appears in
/// `render_html`'s signature) — dynamic dispatch goes through the private
/// `DynBackend` erasure layer in `registry.rs`, which downcasts the config.
///
/// [`Config`]: SpecBackend::Config
#[async_trait::async_trait]
pub trait SpecBackend: Send + Sync + 'static {
    /// Per-backend options deserialized from the `[spec.<name>]` table in
    /// `config.styx`. Use [`NoConfig`] if the backend has none.
    type Config: facet::Facet<'static> + Default + Send + Sync + 'static;

    /// Enum variant this backend implements. 1:1 with [`SpecFormat`].
    fn format(&self) -> SpecFormat;

    /// Stable lowercase identifier — must equal `self.format().name()`.
    fn name(&self) -> &'static str;

    /// File extensions (no leading dot) this backend claims.
    fn extensions(&self) -> &'static [&'static str];

    // ─── extraction (cheap path; no HTML, no config) ─────────────────

    /// Parse `content` into a [`SpecDoc`]. Populates reqs, elements, and
    /// source spans; `html` may be empty.
    async fn parse(&self, content: &str) -> eyre::Result<SpecDoc>;

    /// Extract the sort weight from frontmatter / document metadata.
    fn parse_weight(&self, _content: &str) -> i32 {
        0
    }

    /// Extract the marker prefix (e.g. `"r"` from `r[foo.bar]`) at `span`.
    fn extract_marker_prefix(&self, content: &str, span: SourceSpan) -> Option<String>;

    /// Locate the requirement-id literal within a marker string.
    fn id_range_in_marker(&self, marker: &str) -> eyre::Result<Range<usize>>;

    /// Render an inline diff of two spec snippets as markdown.
    fn diff_inline(&self, old: &str, new: &str) -> Option<String>;

    // ─── display (dashboard HTML; expensive, config-dependent) ───────

    /// Render `input.sources` to dashboard HTML with coverage badges.
    async fn render_html(
        &self,
        input: RenderInput<'_>,
        cfg: &Self::Config,
    ) -> eyre::Result<RenderOutput>;

    /// Render a short body fragment as inline HTML (search snippets, hovers).
    /// Default: HTML-escape only. Markdown overrides to run marq.
    async fn render_inline(&self, text: &str) -> String {
        html_escape::encode_text(text).into_owned()
    }
}

/// Returns true if `ext` is a recognised spec-document extension.
///
/// `.sdoc` is special-cased pending a `SpecFormat::Sdoc` variant — StrictDoc
/// extraction currently lives in `tracey::sdoc` and is dispatched directly in
/// `data.rs` rather than via [`parse_spec`].
pub fn is_spec_extension(ext: &OsStr) -> bool {
    SpecFormat::from_ext(ext).is_some() || ext.to_str() == Some("sdoc")
}

/// Parse spec `content` into a [`SpecDoc`].
///
/// This is the cheap path: requirement definitions, doc elements, and source
/// spans are populated. The `html` field may be empty depending on backend.
pub async fn parse_spec(fmt: SpecFormat, content: &str) -> eyre::Result<SpecDoc> {
    fmt.backend().parse(content).await
}

/// Render an inline diff of two spec snippets as markdown.
///
/// Removed runs are wrapped in `~~strikethrough~~`, added runs in `**bold**`.
/// Returns `None` only when a backend cannot diff at all (none currently).
pub fn diff_inline(fmt: SpecFormat, old: &str, new: &str) -> Option<String> {
    fmt.backend().diff_inline(old, new)
}

/// Extract the sort weight from frontmatter / document metadata.
///
/// Returns `0` when no weight is declared or parsing fails.
pub fn parse_weight(fmt: SpecFormat, content: &str) -> i32 {
    fmt.backend().parse_weight(content)
}

/// Extract the marker prefix (e.g. `"r"` from `r[foo.bar]`) at `span` in
/// `content`.
///
/// Returns `None` if the span is out of bounds or the marker is malformed.
pub fn extract_marker_prefix(fmt: SpecFormat, content: &str, span: SourceSpan) -> Option<String> {
    fmt.backend().extract_marker_prefix(content, span)
}

/// Locate the requirement-id literal within a marker string.
///
/// Returns the byte range of the id *contents* (between the brackets/quotes,
/// delimiters excluded) so [`rewrite_marker`] can splice a new id in without
/// any string searching. Format-specific because typst markers may carry named
/// arguments before the positional id (`#req(level: "shall", "a.b")`), which
/// only the parser can disambiguate.
pub fn id_range_in_marker(fmt: SpecFormat, marker_str: &str) -> eyre::Result<Range<usize>> {
    fmt.backend().id_range_in_marker(marker_str)
}

/// Rewrite a marker string to point at `base+new_ver`.
///
/// `id_range` is the byte range of the id contents within `marker_str` (from
/// [`id_range_in_marker`]); everything outside it — prefix, delimiters, named
/// arguments — is preserved byte-for-byte. Used by `tracey bump` to increment
/// requirement versions in-place.
pub fn rewrite_marker(
    marker_str: &str,
    id_range: Range<usize>,
    base: &str,
    new_ver: u32,
) -> eyre::Result<String> {
    if id_range.start > id_range.end || id_range.end > marker_str.len() {
        eyre::bail!("id range {id_range:?} out of bounds for marker {marker_str:?}");
    }
    Ok(format!(
        "{}{}+{}{}",
        &marker_str[..id_range.start],
        base,
        new_ver,
        &marker_str[id_range.end..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_allocator_suffixes_repeats() {
        let mut alloc = SlugAllocator::default();
        assert_eq!(alloc.alloc("a"), "a");
        assert_eq!(alloc.alloc("a"), "a-2");
        assert_eq!(alloc.alloc("b"), "b");
        assert_eq!(alloc.alloc("a"), "a-3");
    }

    #[test]
    fn slug_allocator_avoids_suffix_collision() {
        let mut alloc = SlugAllocator::default();
        assert_eq!(alloc.alloc("intro"), "intro");
        assert_eq!(alloc.alloc("intro-2"), "intro-2");
        // Second `intro` would naïvely yield `intro-2`, which is already
        // taken; the allocator must skip past it.
        assert_eq!(alloc.alloc("intro"), "intro-3");
    }

    #[test]
    fn slug_allocator_normalises_req_prefix() {
        let mut alloc = SlugAllocator::default();
        let a = alloc.alloc("r--design");
        assert!(!a.starts_with(REQ_ANCHOR_PREFIX), "got {a:?}");
        assert!(!a.is_empty());
        let b = alloc.alloc("r--");
        assert!(!b.starts_with(REQ_ANCHOR_PREFIX), "got {b:?}");
        assert!(!b.is_empty());
    }

    #[test]
    fn slug_allocator_preserves_hierarchical_slugs() {
        let mut alloc = SlugAllocator::default();
        // marq joins parent/child with `--`; only the literal `r--` prefix is
        // reserved, every other hierarchical id must pass through unchanged.
        assert_eq!(alloc.alloc("auth--login"), "auth--login");
    }

    #[test]
    fn req_anchor_roundtrip() {
        assert_eq!(req_anchor_id("auth.login"), "r--auth.login");
        assert_eq!(req_anchor_to_id("r--auth.login"), Some("auth.login"));
        assert_eq!(req_anchor_to_id("some-heading"), None);
    }

    #[test]
    fn from_path_classifies_extensions() {
        assert_eq!(
            SpecFormat::from_path(Path::new("doc.md")),
            Some(SpecFormat::Markdown)
        );
        assert_eq!(
            SpecFormat::from_path(Path::new("doc.markdown")),
            Some(SpecFormat::Markdown)
        );
        assert_eq!(
            SpecFormat::from_path(Path::new("doc.typ")),
            Some(SpecFormat::Typst)
        );
        assert_eq!(SpecFormat::from_path(Path::new("doc.rs")), None);
        assert_eq!(SpecFormat::from_path(Path::new("README")), None);
    }

    #[test]
    fn is_spec_extension_matches_constant() {
        for ext in SPEC_EXTENSIONS {
            assert!(
                is_spec_extension(OsStr::new(ext)),
                "{ext} should be recognised"
            );
        }
        assert!(!is_spec_extension(OsStr::new("rs")));
        assert!(!is_spec_extension(OsStr::new("txt")));
        assert!(!is_spec_extension(OsStr::new("")));
    }

    #[test]
    fn markdown_parse_weight_reads_frontmatter() {
        let md = "+++\nweight = 7\n+++\n# Body\n";
        assert_eq!(parse_weight(SpecFormat::Markdown, md), 7);
        assert_eq!(parse_weight(SpecFormat::Markdown, "# no frontmatter"), 0);
    }

    #[test]
    fn markdown_extract_marker_prefix_finds_bracket() {
        let content = "r[auth.login] body text";
        let span = SourceSpan {
            offset: 0,
            length: "r[auth.login]".len(),
        };
        assert_eq!(
            extract_marker_prefix(SpecFormat::Markdown, content, span),
            Some("r".to_string())
        );
    }

    #[test]
    fn markdown_rewrite_marker_bumps_version() {
        let m = "r[auth.login]";
        let r = id_range_in_marker(SpecFormat::Markdown, m).unwrap();
        assert_eq!(&m[r.clone()], "auth.login");
        let out = rewrite_marker(m, r, "auth.login", 2).unwrap();
        assert_eq!(out, "r[auth.login+2]");
    }

    #[tokio::test]
    async fn markdown_parse_roundtrips_single_req() {
        let md = "# Title\n\nr[auth.login]\nUsers must log in.\n";
        let doc = parse_spec(SpecFormat::Markdown, md).await.unwrap();
        assert_eq!(doc.reqs.len(), 1);
        assert_eq!(doc.reqs[0].id.base, "auth.login");
    }

    #[tokio::test]
    async fn typst_parse_roundtrips_single_req() {
        let typ = "= Title\n\n#req(\"auth.login\")[Users must log in.]\n";
        let doc = parse_spec(SpecFormat::Typst, typ).await.unwrap();
        assert_eq!(doc.reqs.len(), 1);
        assert_eq!(doc.reqs[0].id.base, "auth.login");
    }

    #[test]
    fn typst_extract_marker_prefix_finds_paren() {
        let content = "#req(\"auth.login\")[body]";
        let span = SourceSpan {
            offset: 0,
            length: "#req(\"auth.login\")".len(),
        };
        assert_eq!(
            extract_marker_prefix(SpecFormat::Typst, content, span),
            Some("req".to_string())
        );
    }

    #[test]
    fn typst_rewrite_marker_bumps_version() {
        let m = "#req(\"auth.login\")";
        let r = id_range_in_marker(SpecFormat::Typst, m).unwrap();
        assert_eq!(&m[r.clone()], "auth.login");
        let out = rewrite_marker(m, r, "auth.login", 2).unwrap();
        assert_eq!(out, "#req(\"auth.login+2\")");
    }
}
