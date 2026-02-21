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
    let mut base_offset = 0usize;

    for line in lines {
        let bytes = line.as_bytes();
        let mut remove = 0usize;
        while remove < min_indent && remove < bytes.len() && matches!(bytes[remove], b' ' | b'\t') {
            remove += 1;
        }

        normalized.push_str(&line[remove..]);
        for original_idx in (base_offset + remove)..(base_offset + line.len()) {
            index_map.push(original_idx);
        }

        base_offset += line.len();
    }

    (normalized, index_map)
}
