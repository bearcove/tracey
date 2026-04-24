//! Multi-format spec file support.
//!
//! This module provides format-dispatched operations for spec files (Markdown, AsciiDoc).
//! Both formats share the same `r[id]` marker syntax for requirement definitions.

use std::ffi::OsStr;
use std::ops::Range;
use std::path::Path;

pub mod asciidoc;
mod markdown;

// Re-export marq types as canonical spec types
pub use marq::{DocElement, InlineCodeSpan, ReqDefinition, RuleId as SpecRuleId, SourceSpan};
pub type SpecDoc = marq::Document;

/// Supported spec file format.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpecFormat {
    Markdown,
    AsciiDoc,
}

/// All file extensions recognized as spec files.
pub const SPEC_EXTENSIONS: &[&str] = &["md", "markdown", "adoc", "asciidoc", "asc"];

/// Returns true if the extension is a recognized spec file extension.
pub fn is_spec_extension(ext: &OsStr) -> bool {
    SPEC_EXTENSIONS.iter().any(|e| OsStr::new(e) == ext)
}

impl SpecFormat {
    /// Determine format from file path extension.
    pub fn from_path(p: &Path) -> Option<Self> {
        p.extension().and_then(Self::from_ext)
    }

    /// Determine format from file extension.
    pub fn from_ext(ext: &OsStr) -> Option<Self> {
        match ext.to_str()? {
            "md" | "markdown" => Some(SpecFormat::Markdown),
            "adoc" | "asciidoc" | "asc" => Some(SpecFormat::AsciiDoc),
            _ => None,
        }
    }

    /// A stable lowercase string name for this format (for search indexes, logs).
    pub fn as_str(self) -> &'static str {
        #[allow(unreachable_patterns)]
        match self {
            SpecFormat::Markdown => "markdown",
            SpecFormat::AsciiDoc => "asciidoc",
            _ => "unknown",
        }
    }
}

/// HTML anchor prefix for requirement anchors.
/// Every backend MUST use this prefix for `ReqDefinition::anchor_id`.
pub const REQ_ANCHOR_PREFIX: &str = "r--";

/// Compute the HTML anchor ID for a requirement ID string.
pub fn req_anchor_id(id: &str) -> String {
    format!("{}{}", REQ_ANCHOR_PREFIX, id)
}

/// Extract the requirement ID from an anchor ID (reverse of `req_anchor_id`).
pub fn req_anchor_to_id(anchor: &str) -> Option<&str> {
    anchor.strip_prefix(REQ_ANCHOR_PREFIX)
}

/// Slug allocator — ensures heading anchors are globally unique across a mixed-format spec.
///
/// Thread the same allocator through all per-file renderers in one `load_spec_content` call.
pub struct SlugAllocator {
    seen: std::collections::HashSet<String>,
}

impl Default for SlugAllocator {
    fn default() -> Self {
        Self {
            seen: std::collections::HashSet::new(),
        }
    }
}

impl SlugAllocator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a unique slug. Appends `-2`, `-3`, ... when `base` is already taken.
    pub fn alloc(&mut self, base: &str) -> String {
        if self.seen.insert(base.to_string()) {
            return base.to_string();
        }
        let mut n = 2u32;
        loop {
            let candidate = format!("{}-{}", base, n);
            if self.seen.insert(candidate.clone()) {
                return candidate;
            }
            n += 1;
        }
    }
}

/// Parse a spec file to extract requirements, headings, and placeholder HTML.
///
/// This is the cheap path used by LSP, bump, and coverage extraction.
/// It does not invoke coverage-aware rendering.
pub async fn parse_spec(fmt: SpecFormat, content: &str) -> eyre::Result<SpecDoc> {
    #[allow(unreachable_patterns)]
    match fmt {
        SpecFormat::Markdown => markdown::parse(content).await,
        SpecFormat::AsciiDoc => asciidoc::parse(content).await,
        _ => eyre::bail!("parse_spec not implemented for format {:?}", fmt),
    }
}

/// Compute an inline diff between two versions of a spec rule's raw text.
///
/// Returns `None` if the format does not support inline diff.
pub fn diff_inline(fmt: SpecFormat, old: &str, new: &str) -> Option<String> {
    #[allow(unreachable_patterns)]
    match fmt {
        SpecFormat::Markdown => markdown::diff_inline(old, new),
        SpecFormat::AsciiDoc => asciidoc::diff_inline(old, new),
        _ => None,
    }
}

/// Parse the sort weight from spec file content.
///
/// Markdown: reads TOML/YAML frontmatter `weight` field.
/// AsciiDoc: reads `:weight: N` document attribute (or YAML/TOML frontmatter).
pub fn parse_weight(fmt: SpecFormat, content: &str) -> i32 {
    #[allow(unreachable_patterns)]
    match fmt {
        SpecFormat::Markdown => markdown::parse_weight(content),
        SpecFormat::AsciiDoc => asciidoc::parse_weight(content),
        _ => 0,
    }
}

/// Extract the marker prefix from a requirement marker at the given byte span.
///
/// E.g., for `r[auth.login]` with `span` covering the whole marker, returns `"r"`.
pub fn extract_marker_prefix(fmt: SpecFormat, content: &str, span: SourceSpan) -> Option<String> {
    #[allow(unreachable_patterns)]
    match fmt {
        SpecFormat::Markdown => markdown::extract_marker_prefix(content, span),
        SpecFormat::AsciiDoc => asciidoc::extract_marker_prefix(content, span),
        _ => None,
    }
}

/// Find the byte range of the ID within a marker string.
///
/// For `"r[auth.login+2]"`, returns `2..14` (the bytes of `auth.login+2`).
pub fn id_range_in_marker(fmt: SpecFormat, marker_str: &str) -> eyre::Result<Range<usize>> {
    #[allow(unreachable_patterns)]
    match fmt {
        SpecFormat::Markdown => markdown::id_range_in_marker(marker_str),
        SpecFormat::AsciiDoc => asciidoc::id_range_in_marker(marker_str),
        _ => eyre::bail!("id_range_in_marker not implemented for format {:?}", fmt),
    }
}

/// Rewrite a marker string, replacing the ID at `id_range` with `base+new_ver`.
///
/// Format-independent: all formats share the `prefix[base+version]` marker syntax.
pub fn rewrite_marker(
    marker_str: &str,
    id_range: Range<usize>,
    base: &str,
    new_ver: u32,
) -> eyre::Result<String> {
    if id_range.end > marker_str.len() {
        eyre::bail!(
            "id_range {:?} out of bounds for marker {:?}",
            id_range,
            marker_str
        );
    }
    let new_id = if new_ver == 1 {
        base.to_string()
    } else {
        format!("{}+{}", base, new_ver)
    };
    let mut result =
        String::with_capacity(marker_str.len() - (id_range.end - id_range.start) + new_id.len());
    result.push_str(&marker_str[..id_range.start]);
    result.push_str(&new_id);
    result.push_str(&marker_str[id_range.end..]);
    Ok(result)
}

/// Common implementation for marker prefix extraction (identical for all formats).
pub(crate) fn common_extract_marker_prefix(content: &str, span: SourceSpan) -> Option<String> {
    let start = span.offset;
    let end = start.checked_add(span.length)?;
    let marker = content.get(start..end)?;
    let bracket = marker.find('[')?;
    let prefix = marker[..bracket].trim();
    if prefix.is_empty() {
        return None;
    }
    Some(prefix.to_string())
}

/// Common implementation for ID range extraction (identical for all formats).
pub(crate) fn common_id_range_in_marker(marker_str: &str) -> eyre::Result<Range<usize>> {
    let open = marker_str
        .find('[')
        .ok_or_else(|| eyre::eyre!("no '[' in marker: {:?}", marker_str))?;
    let close = marker_str
        .rfind(']')
        .ok_or_else(|| eyre::eyre!("no ']' in marker: {:?}", marker_str))?;
    if close <= open + 1 {
        eyre::bail!("empty or malformed marker: {:?}", marker_str);
    }
    Ok(open + 1..close)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::Path;

    #[test]
    fn test_is_spec_extension() {
        assert!(is_spec_extension(OsStr::new("md")));
        assert!(is_spec_extension(OsStr::new("markdown")));
        assert!(is_spec_extension(OsStr::new("adoc")));
        assert!(is_spec_extension(OsStr::new("asciidoc")));
        assert!(is_spec_extension(OsStr::new("asc")));
        assert!(!is_spec_extension(OsStr::new("rs")));
        assert!(!is_spec_extension(OsStr::new("txt")));
    }

    #[test]
    fn test_from_ext() {
        assert_eq!(
            SpecFormat::from_ext(OsStr::new("md")),
            Some(SpecFormat::Markdown)
        );
        assert_eq!(
            SpecFormat::from_ext(OsStr::new("markdown")),
            Some(SpecFormat::Markdown)
        );
        assert_eq!(
            SpecFormat::from_ext(OsStr::new("adoc")),
            Some(SpecFormat::AsciiDoc)
        );
        assert_eq!(
            SpecFormat::from_ext(OsStr::new("asciidoc")),
            Some(SpecFormat::AsciiDoc)
        );
        assert_eq!(
            SpecFormat::from_ext(OsStr::new("asc")),
            Some(SpecFormat::AsciiDoc)
        );
        assert_eq!(SpecFormat::from_ext(OsStr::new("rs")), None);
    }

    #[test]
    fn test_from_path() {
        assert_eq!(
            SpecFormat::from_path(Path::new("docs/spec.md")),
            Some(SpecFormat::Markdown)
        );
        assert_eq!(
            SpecFormat::from_path(Path::new("docs/spec.adoc")),
            Some(SpecFormat::AsciiDoc)
        );
        assert_eq!(SpecFormat::from_path(Path::new("src/main.rs")), None);
    }

    #[test]
    fn test_req_anchor_round_trip() {
        let id = "auth.token.validation";
        let anchor = req_anchor_id(id);
        assert_eq!(anchor, "r--auth.token.validation");
        assert_eq!(req_anchor_to_id(&anchor), Some(id));
        assert_eq!(req_anchor_to_id("not-an-anchor"), None);
    }

    #[test]
    fn test_slug_allocator() {
        let mut alloc = SlugAllocator::new();
        assert_eq!(alloc.alloc("my-heading"), "my-heading");
        assert_eq!(alloc.alloc("my-heading"), "my-heading-2");
        assert_eq!(alloc.alloc("my-heading"), "my-heading-3");
        assert_eq!(alloc.alloc("other"), "other");
    }

    #[test]
    fn test_id_range_in_marker() {
        // r[auth.login]
        let range = common_id_range_in_marker("r[auth.login]").unwrap();
        assert_eq!(&"r[auth.login]"[range.clone()], "auth.login");

        // r[auth.login+2]
        let range2 = common_id_range_in_marker("r[auth.login+2]").unwrap();
        assert_eq!(&"r[auth.login+2]"[range2.clone()], "auth.login+2");
    }

    #[test]
    fn test_rewrite_marker() {
        let marker = "r[auth.login]";
        let range = common_id_range_in_marker(marker).unwrap();
        let result = rewrite_marker(marker, range, "auth.login", 2).unwrap();
        assert_eq!(result, "r[auth.login+2]");

        let marker2 = "r[auth.login+2]";
        let range2 = common_id_range_in_marker(marker2).unwrap();
        let result2 = rewrite_marker(marker2, range2, "auth.login", 3).unwrap();
        assert_eq!(result2, "r[auth.login+3]");
    }
}
