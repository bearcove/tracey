use crate::positions::ByteOffset;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

pub(crate) fn markdown_code_mask(text: &str) -> Vec<bool> {
    let (normalized, index_map) = dedent_with_index_map(text);
    let parser = Parser::new_ext(&normalized, Options::all());

    let mut mask = vec![false; text.len()];
    let mut in_fenced_code_block = false;

    for (event, range) in parser.into_offset_iter() {
        let should_mark = match event {
            Event::Code(_) => true,
            Event::Start(Tag::CodeBlock(kind)) => {
                if matches!(kind, CodeBlockKind::Fenced(_)) {
                    in_fenced_code_block = true;
                    true
                } else {
                    false
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if in_fenced_code_block {
                    in_fenced_code_block = false;
                    true
                } else {
                    false
                }
            }
            _ => in_fenced_code_block,
        };

        if should_mark {
            for normalized_idx in range {
                if let Some(&original_idx) = index_map.get(normalized_idx)
                    && let Some(slot) = mask.get_mut(original_idx)
                {
                    *slot = true;
                }
            }
        }
    }

    mask
}

pub(crate) fn is_code_index(index: usize, code_mask: &[bool]) -> bool {
    code_mask.get(index).copied().unwrap_or(false)
}

/// Classify a line comment by its prefix type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineCommentKind {
    /// `//!` inner doc comment
    InnerDoc,
    /// `///` outer doc comment
    OuterDoc,
    /// `//` regular line comment
    Regular,
}

/// Strip the line-comment prefix (`//!`, `///`, or `//`) and optional following space.
/// Returns `(prefix_byte_length, content_after_prefix)`.
pub(crate) fn strip_line_comment_prefix(text: &str) -> (usize, &str) {
    let prefix_len = if text.starts_with("//!") || text.starts_with("///") {
        3
    } else if text.starts_with("//") {
        2
    } else {
        return (0, text);
    };
    let after_slashes = &text[prefix_len..];
    if let Some(rest) = after_slashes.strip_prefix(' ') {
        (prefix_len + 1, rest)
    } else {
        (prefix_len, after_slashes)
    }
}

/// Classify a comment string starting with `//`.
pub(crate) fn classify_line_comment(text: &str) -> Option<LineCommentKind> {
    if text.starts_with("//!") {
        Some(LineCommentKind::InnerDoc)
    } else if text.starts_with("///") && !text.starts_with("////") {
        Some(LineCommentKind::OuterDoc)
    } else if text.starts_with("//") {
        Some(LineCommentKind::Regular)
    } else {
        None
    }
}

/// Pre-compute a file-level mask of bytes that fall inside markdown code
/// (fenced code blocks or inline backtick spans) within doc-comment groups.
///
/// Consecutive line comments of the same kind (`///`, `//!`, `//`) are grouped,
/// their prefixes stripped, the bodies joined with `\n`, and
/// `markdown_code_mask` is run over the combined text. The resulting mask is
/// mapped back to file byte offsets so callers can check any byte position.
pub(crate) fn compute_doc_comment_code_mask(content: &str) -> Vec<bool> {
    let mut mask = vec![false; content.len()];

    // Pre-compute line byte offsets
    let mut line_offsets: Vec<usize> = vec![0];
    for (i, byte) in content.bytes().enumerate() {
        if byte == b'\n' {
            line_offsets.push(i + 1);
        }
    }

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let Some(comment_pos) = line.find("//") else {
            i += 1;
            continue;
        };
        let comment = &line[comment_pos..];
        let Some(kind) = classify_line_comment(comment) else {
            i += 1;
            continue;
        };

        // Collect consecutive lines of the same comment kind
        let group_start = i;
        let mut j = i + 1;
        while j < lines.len() {
            if let Some(cp) = lines[j].find("//") {
                let c = &lines[j][cp..];
                if classify_line_comment(c) == Some(kind) {
                    j += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Only need the combined mask for groups of 2+ lines
        // (single lines are already handled correctly by per-line masking)
        if j - group_start > 1 {
            let mut combined = String::new();
            // Maps each byte index in `combined` to a file byte offset
            let mut offset_map: Vec<usize> = Vec::new();

            for k in group_start..j {
                if k > group_start {
                    combined.push('\n');
                    offset_map.push(usize::MAX); // sentinel for virtual newline
                }
                let cp = lines[k].find("//").unwrap();
                let comment_text = &lines[k][cp..];
                let (prefix_len, stripped) = strip_line_comment_prefix(comment_text);
                let content_file_start = line_offsets[k] + cp + prefix_len;

                for bi in 0..stripped.len() {
                    offset_map.push(content_file_start + bi);
                }
                combined.push_str(stripped);
            }

            let code_mask = markdown_code_mask(&combined);

            for (ci, &is_code) in code_mask.iter().enumerate() {
                if is_code
                    && let Some(&file_idx) = offset_map.get(ci)
                    && file_idx != usize::MAX
                    && let Some(slot) = mask.get_mut(file_idx)
                {
                    *slot = true;
                }
            }
        }

        i = j;
    }

    mask
}

fn dedent_with_index_map(text: &str) -> (String, Vec<usize>) {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();

    let min_indent = lines
        .iter()
        .filter_map(|line| {
            let content = line.strip_suffix('\n').unwrap_or(line);
            if content.trim().is_empty() {
                return None;
            }
            Some(
                content
                    .bytes()
                    .take_while(|b| matches!(b, b' ' | b'\t'))
                    .count(),
            )
        })
        .min()
        .unwrap_or(0);

    let mut normalized = String::with_capacity(text.len());
    let mut index_map = Vec::with_capacity(text.len());
    let mut base_offset = ByteOffset::ZERO;

    for line in lines {
        let bytes = line.as_bytes();
        let mut remove = 0usize;
        while remove < min_indent && remove < bytes.len() && matches!(bytes[remove], b' ' | b'\t') {
            remove += 1;
        }

        normalized.push_str(&line[remove..]);
        let line_start = base_offset.add(remove).as_usize();
        let line_end = base_offset.add(line.len()).as_usize();
        for original_idx in line_start..line_end {
            index_map.push(original_idx);
        }

        base_offset = base_offset.add(line.len());
    }

    (normalized, index_map)
}
