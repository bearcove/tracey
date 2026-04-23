//! Spec document abstraction layer.
//!
//! Tracey supports spec documents written in multiple formats. This module
//! provides a small enum-dispatched facade over the per-format implementations
//! so callers never need to know which concrete parser is in play.
//!
//! Design rationale: enum + free functions, not a trait. There are only two
//! variants, every call site already has a `Path` to dispatch on, and this
//! avoids async-trait boxing. `RenderOptions` is marq-specific and does not
//! generalise cleanly to a trait.

use std::ffi::OsStr;
use std::path::Path;

mod markdown;
pub mod typst;

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
    seen: std::collections::HashMap<String, usize>,
}

impl SlugAllocator {
    /// Returns `base` if unseen, else `base-2`, `base-3`, ...
    ///
    /// `base` must be a `marq::slugify`-produced slug (or a literal known not
    /// to contain `--`). The slugifier collapses consecutive separators, so
    /// heading slugs cannot enter the [`REQ_ANCHOR_PREFIX`] namespace and the
    /// dashboard router can safely distinguish the two by prefix.
    pub fn alloc(&mut self, base: &str) -> String {
        debug_assert!(
            !base.starts_with(REQ_ANCHOR_PREFIX),
            "heading slug {base:?} entered the req-anchor namespace; slugifier invariant broken"
        );
        let n = self
            .seen
            .entry(base.to_string())
            .and_modify(|n| *n += 1)
            .or_insert(1);
        if *n == 1 {
            base.to_string()
        } else {
            format!("{base}-{n}")
        }
    }
}

/// Which spec dialect a file is written in.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpecFormat {
    /// CommonMark + tracey marker syntax, parsed via `marq`.
    Markdown,
    /// Typst markup with `#req(...)` calls.
    Typst,
}

impl SpecFormat {
    /// Classify a path by its extension.
    pub fn from_path(p: &Path) -> Option<Self> {
        Self::from_ext(p.extension()?)
    }

    /// Classify a bare extension (no leading dot).
    pub fn from_ext(ext: &OsStr) -> Option<Self> {
        match ext.to_str()? {
            "md" | "markdown" => Some(Self::Markdown),
            "typ" => Some(Self::Typst),
            _ => None,
        }
    }
}

/// Returns true if `ext` is a recognised spec-document extension.
pub fn is_spec_extension(ext: &OsStr) -> bool {
    SpecFormat::from_ext(ext).is_some()
}

/// Parse spec `content` into a [`SpecDoc`].
///
/// This is the cheap path: requirement definitions, doc elements, and source
/// spans are populated. The `html` field may be empty depending on backend.
pub async fn parse_spec(fmt: SpecFormat, content: &str) -> eyre::Result<SpecDoc> {
    match fmt {
        SpecFormat::Markdown => markdown::parse(content).await,
        SpecFormat::Typst => typst::parse(content).await,
    }
}

/// Render an inline diff of two spec snippets as markdown.
///
/// Removed runs are wrapped in `~~strikethrough~~`, added runs in `**bold**`.
/// Returns `None` only when a backend cannot diff at all (none currently).
pub fn diff_inline(fmt: SpecFormat, old: &str, new: &str) -> Option<String> {
    match fmt {
        SpecFormat::Markdown => Some(markdown::diff_inline(old, new)),
        SpecFormat::Typst => typst::diff_inline(old, new),
    }
}

/// Extract the sort weight from frontmatter / document metadata.
///
/// Returns `0` when no weight is declared or parsing fails.
pub fn parse_weight(fmt: SpecFormat, content: &str) -> i32 {
    match fmt {
        SpecFormat::Markdown => markdown::parse_weight(content),
        SpecFormat::Typst => typst::parse_weight(content),
    }
}

/// Extract the marker prefix (e.g. `"r"` from `r[foo.bar]`) at `span` in
/// `content`.
///
/// Returns `None` if the span is out of bounds or the marker is malformed.
pub fn extract_marker_prefix(fmt: SpecFormat, content: &str, span: SourceSpan) -> Option<String> {
    match fmt {
        SpecFormat::Markdown => markdown::extract_marker_prefix(content, span),
        SpecFormat::Typst => typst::extract_marker_prefix(content, span),
    }
}

/// Rewrite a marker string to point at `base+new_ver`.
///
/// Used by `tracey bump` to increment requirement versions in-place.
pub fn rewrite_marker(
    fmt: SpecFormat,
    marker_str: &str,
    base: &str,
    new_ver: u32,
) -> eyre::Result<String> {
    match fmt {
        SpecFormat::Markdown => markdown::rewrite_marker(marker_str, base, new_ver),
        SpecFormat::Typst => typst::rewrite_marker(marker_str, base, new_ver),
    }
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
    fn slugifier_cannot_enter_req_anchor_namespace() {
        // SlugAllocator inputs are always `marq::slugify`-produced. The
        // slugifier collapses consecutive separators, so no heading title can
        // produce a slug starting with `r--`. This test locks that invariant
        // so the debug_assert in `alloc()` stays unreachable.
        assert_eq!(marq::slugify("r--design"), "r-design");
        assert_eq!(marq::slugify("# R-- Design Review"), "r-design-review");
        for title in ["r--x", "r--", "--r--foo", "r - - bar"] {
            assert!(
                !marq::slugify(title).starts_with(REQ_ANCHOR_PREFIX),
                "marq::slugify({title:?}) produced a req-anchor-prefixed slug"
            );
        }
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
        let out = rewrite_marker(SpecFormat::Markdown, "r[auth.login]", "auth.login", 2).unwrap();
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
        let out = rewrite_marker(SpecFormat::Typst, "#req(\"auth.login\")", "auth.login", 2).unwrap();
        assert_eq!(out, "#req(\"auth.login+2\")");
    }
}
