//! Markdown spec backend — thin wrapper over `marq`.

use marq::SourceSpan;

use super::SpecDoc;

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

/// Rebuild a `prefix[base+ver]` marker from its current text and a new
/// version number.
///
/// Mirrors the logic at `crates/tracey/src/bump.rs`.
pub(super) fn rewrite_marker(marker_str: &str, base: &str, new_ver: u32) -> eyre::Result<String> {
    let bracket = marker_str
        .find('[')
        .ok_or_else(|| eyre::eyre!("malformed marker: {}", marker_str))?;
    let prefix = &marker_str[..bracket];
    Ok(format!("{}[{}+{}]", prefix, base, new_ver))
}
