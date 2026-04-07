//! Typst spec backend — stubbed pending Phase 6.
//!
//! All entry points fail loudly so any premature wiring is caught
//! immediately. The real implementation lands once the typst compile
//! pipeline (Spike A) and arborium-typst extraction (Spike B) are
//! integrated.

use marq::SourceSpan;

use super::SpecDoc;

const STUB_MSG: &str = "typst spec support not yet implemented (phase 6)";

pub(super) async fn parse(_content: &str) -> eyre::Result<SpecDoc> {
    Err(eyre::eyre!(STUB_MSG))
}

pub(super) fn diff_inline(_old: &str, _new: &str) -> Option<String> {
    // No inline diff for typst in v1; callers fall back to plain text.
    None
}

pub(super) fn parse_weight(_content: &str) -> i32 {
    // Weight is deferred for typst (Q2): always sort with default weight.
    0
}

pub(super) fn extract_marker_prefix(_content: &str, _span: SourceSpan) -> Option<String> {
    // Unreachable until `parse` succeeds; return None rather than panic so a
    // stray call degrades gracefully.
    None
}

pub(super) fn rewrite_marker(
    _marker_str: &str,
    _base: &str,
    _new_ver: u32,
) -> eyre::Result<String> {
    Err(eyre::eyre!(STUB_MSG))
}
