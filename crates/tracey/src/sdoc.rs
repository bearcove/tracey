//! Loader for StrictDoc (`.sdoc`) specification files.
//!
//! Bridges `strictdoc_parser::RequirementView` onto tracey's existing
//! [`crate::ExtractedRule`] shape so that downstream code paths (coverage,
//! daemon, queries) treat `.sdoc` and `.md` specs uniformly.

use eyre::{Result, eyre};
use marq::{RenderOptions, ReqDefinition, ReqMetadata, SourceSpan, parse_rule_id};

use crate::ExtractedRule;

/// Synthetic marker prefix used for requirements loaded from `.sdoc` files.
///
/// `.sdoc` has no `r[...]`-style marker, so this value is what
/// `@relation(...)` source markers must agree on for matching.
pub const SDOC_PREFIX: &str = "r";

/// Parse a `.sdoc` document and produce one [`ExtractedRule`] per
/// `[REQUIREMENT]` block.
pub async fn extract_rules_from_sdoc(
    content: &str,
    source_file: &str,
) -> Result<Vec<ExtractedRule>> {
    let doc = strictdoc_parser::parse(content)
        .map_err(|e| eyre!("Failed to parse {} as StrictDoc: {}", source_file, e))?;

    let markup_is_markdown = doc
        .options
        .get("MARKUP")
        .is_some_and(|v| v.eq_ignore_ascii_case("Markdown"));

    let mut rules = Vec::new();
    for view in doc.requirements_flat() {
        let Some(uid) = view.uid() else {
            continue;
        };
        let Some(rule_id) = parse_rule_id(uid) else {
            eprintln!(
                "Warning: invalid UID '{}' in {}, skipping requirement",
                uid, source_file
            );
            continue;
        };

        let req_span = view.requirement.span;
        let raw = content
            .get(req_span.start..req_span.end)
            .unwrap_or("")
            .to_string();

        let html = match view.statement() {
            Some(stmt) if markup_is_markdown => marq::render(stmt, &RenderOptions::default())
                .await
                .map(|d| d.html)
                .unwrap_or_else(|_| wrap_paragraph(stmt)),
            Some(stmt) => wrap_paragraph(stmt),
            None => String::new(),
        };

        let anchor_id = format!("r--{}", uid);
        let length = req_span.end.saturating_sub(req_span.start);

        let def = ReqDefinition {
            id: rule_id,
            anchor_id,
            marker_span: SourceSpan {
                offset: req_span.start,
                length: 0,
            },
            span: SourceSpan {
                offset: req_span.start,
                length,
            },
            line: req_span.line as usize,
            metadata: ReqMetadata::default(),
            raw,
            html,
        };

        let column = Some(compute_column(content, req_span.start));
        let section_title = view.section_path.last().map(|s| s.to_string());

        rules.push(ExtractedRule {
            def,
            source_file: source_file.to_string(),
            // TODO(sdoc-specformat): add `SpecFormat::Sdoc` and route extraction
            // through `parse_spec`; until then sdoc rules report as Markdown so
            // downstream rendering (search highlight, hover) treats body text
            // as plain prose via the existing wildcard arms.
            format: tracey_core::SpecFormat::Markdown,
            prefix: SDOC_PREFIX.to_string(),
            column,
            section: None,
            section_title,
        });
    }
    Ok(rules)
}

fn wrap_paragraph(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    out.push_str("<p>");
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out.push_str("</p>");
    out
}

fn compute_column(content: &str, byte_offset: usize) -> usize {
    let before = &content[..byte_offset.min(content.len())];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    before[line_start..].chars().count() + 1
}
