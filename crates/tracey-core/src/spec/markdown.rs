//! Markdown spec backend — thin delegation to marq.

use crate::spec::SourceSpan;

pub(super) async fn parse(content: &str) -> eyre::Result<super::SpecDoc> {
    marq::render(content, &marq::RenderOptions::default())
        .await
        .map_err(|e| eyre::eyre!("markdown parse error: {}", e))
}

pub(super) fn diff_inline(old: &str, new: &str) -> Option<String> {
    Some(marq::diff_markdown_inline(old, new))
}

pub(super) fn parse_weight(content: &str) -> i32 {
    marq::parse_frontmatter(content)
        .ok()
        .and_then(|(fm, _)| Some(fm.weight))
        .unwrap_or(0)
}

pub(super) fn extract_marker_prefix(content: &str, span: SourceSpan) -> Option<String> {
    super::common_extract_marker_prefix(content, span)
}

pub(super) fn id_range_in_marker(marker_str: &str) -> eyre::Result<std::ops::Range<usize>> {
    super::common_id_range_in_marker(marker_str)
}
