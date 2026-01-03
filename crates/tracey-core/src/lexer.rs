//! Rust lexer for extracting rule references from comments
//!
//! This module implements parsing of rule references from Rust source code.
//! It scans comments for patterns like `[verb rule.id]` or `[rule.id]`.

use crate::sources::Sources;
use eyre::Result;
use facet::Facet;
use std::path::{Path, PathBuf};

/// Byte span in source code
///
/// [impl ref.span.offset]
/// [impl ref.span.length]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct SourceSpan {
    /// Byte offset from start of file
    pub offset: usize,
    /// Byte length
    pub length: usize,
}

impl SourceSpan {
    pub fn new(offset: usize, length: usize) -> Self {
        Self { offset, length }
    }
}

/// The relationship type between code and a spec rule
///
/// [impl ref.verb.define]
/// [impl ref.verb.impl]
/// [impl ref.verb.verify]
/// [impl ref.verb.depends]
/// [impl ref.verb.related]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum RefVerb {
    /// Where the requirement is defined (typically in specs/docs)
    Define,
    /// Code that fulfills/implements the requirement
    Impl,
    /// Tests that verify the implementation matches the spec
    Verify,
    /// Strict dependency - must recheck if the referenced rule changes
    Depends,
    /// Loose connection - show when reviewing
    Related,
}

impl RefVerb {
    /// Parse a verb from its string representation
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "define" => Some(RefVerb::Define),
            "impl" => Some(RefVerb::Impl),
            "verify" => Some(RefVerb::Verify),
            "depends" => Some(RefVerb::Depends),
            "related" => Some(RefVerb::Related),
            _ => None,
        }
    }

    /// Get the string representation of this verb
    pub fn as_str(&self) -> &'static str {
        match self {
            RefVerb::Define => "define",
            RefVerb::Impl => "impl",
            RefVerb::Verify => "verify",
            RefVerb::Depends => "depends",
            RefVerb::Related => "related",
        }
    }
}

impl std::fmt::Display for RefVerb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A reference to a rule found in source code
///
/// [impl ref.span.file]
#[derive(Debug, Clone, Facet)]
pub struct RuleReference {
    /// The relationship type (impl, verify, depends, etc.)
    pub verb: RefVerb,
    /// The rule ID (e.g., "channel.id.allocation")
    pub rule_id: String,
    /// File where the reference was found
    pub file: PathBuf,
    /// Line number (1-indexed)
    pub line: usize,
    /// Byte span of the reference in source
    pub span: SourceSpan,
}

/// Warning during parsing
#[derive(Debug, Clone, Facet)]
pub struct ParseWarning {
    /// File where the warning occurred
    pub file: PathBuf,
    /// Line number (1-indexed)
    pub line: usize,
    /// Byte span of the problematic text
    pub span: SourceSpan,
    /// What kind of warning
    pub kind: WarningKind,
}

/// Types of parse warnings
///
/// [impl ref.verb.unknown]
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum WarningKind {
    /// Unknown verb in `[verb rule.id]`
    UnknownVerb(String),
    /// Malformed reference
    MalformedReference,
}

/// Collection of rule references extracted from source files
#[derive(Debug, Clone, Default, Facet)]
pub struct Rules {
    /// Valid rule references
    pub references: Vec<RuleReference>,
    /// Warnings encountered during parsing
    pub warnings: Vec<ParseWarning>,
}

impl Rules {
    /// Create an empty Rules collection
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of valid references
    pub fn len(&self) -> usize {
        self.references.len()
    }

    /// Whether there are no references
    pub fn is_empty(&self) -> bool {
        self.references.is_empty()
    }

    /// Extract rules from any source
    pub fn extract(sources: impl Sources) -> Result<Self> {
        sources.extract()
    }

    /// Extract rules from raw content (no I/O)
    pub fn extract_from_content(path: &Path, content: &str) -> Self {
        let mut rules = Rules::new();
        extract_from_content(path, content, &mut rules);
        rules
    }

    /// Merge another Rules into this one
    pub fn extend(&mut self, other: Rules) {
        self.references.extend(other.references);
        self.warnings.extend(other.warnings);
    }
}

/// Extract rule references from source content into the Rules collection
pub(crate) fn extract_from_content(path: &Path, content: &str, rules: &mut Rules) {
    // Track line starts for computing line numbers from byte offsets
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(content.match_indices('\n').map(|(i, _)| i + 1))
        .collect();

    let get_line = |offset: usize| -> usize {
        match line_starts.binary_search(&offset) {
            Ok(line) => line + 1,
            Err(line) => line,
        }
    };

    // Scan for comments and extract references
    for (line_idx, line) in content.lines().enumerate() {
        let line_num = line_idx + 1;
        let line_start = line_starts.get(line_idx).copied().unwrap_or(0);

        // Check for line comments (// or ///)
        // [impl ref.comments.line]
        // [impl ref.comments.doc]
        if let Some(comment_pos) = line.find("//") {
            let comment = &line[comment_pos..];
            let comment_start = line_start + comment_pos;
            extract_references_from_text(path, comment, comment_start, line_num, rules);
        }
    }

    // Handle block comments /* */
    // [impl ref.comments.block]
    let mut in_block_comment = false;
    let mut block_start = 0;
    let mut block_line = 0;
    let mut i = 0;
    let bytes = content.as_bytes();

    while i < bytes.len() {
        if in_block_comment {
            if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                let block_content = &content[block_start..i];
                extract_references_from_text(path, block_content, block_start, block_line, rules);
                in_block_comment = false;
                i += 2;
                continue;
            }
        } else if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            in_block_comment = true;
            block_start = i + 2;
            block_line = get_line(i);
            i += 2;
            continue;
        }
        i += 1;
    }
}

/// Extract rule references from a piece of text (comment content)
fn extract_references_from_text(
    path: &Path,
    text: &str,
    text_offset: usize,
    base_line: usize,
    rules: &mut Rules,
) {
    let mut chars = text.char_indices().peekable();

    while let Some((start_idx, ch)) = chars.next() {
        // [impl ref.syntax.brackets]
        if ch == '[' {
            let bracket_start = text_offset + start_idx;

            // Try to parse: [verb rule.id] or [rule.id]
            let mut first_word = String::new();
            let mut valid = true;

            // First char must be lowercase letter
            if let Some(&(_, first_char)) = chars.peek() {
                if first_char.is_ascii_lowercase() {
                    first_word.push(first_char);
                    chars.next();
                } else {
                    valid = false;
                }
            } else {
                valid = false;
            }

            if valid {
                // Read the first word (could be verb or start of rule ID)
                // [impl ref.syntax.rule-id]
                while let Some(&(_, c)) = chars.peek() {
                    if c == ']' || c == ' ' {
                        break;
                    } else if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.' {
                        first_word.push(c);
                        chars.next();
                    } else {
                        valid = false;
                        break;
                    }
                }
            }

            if !valid || first_word.is_empty() {
                continue;
            }

            // Check what follows
            if let Some(&(end_idx, next_char)) = chars.peek() {
                if next_char == ' ' {
                    // Space after first word - might be [verb rule.id]
                    // [impl ref.syntax.verb]
                    if let Some(verb) = RefVerb::parse(&first_word) {
                        chars.next(); // consume space

                        // Now read the rule ID
                        let mut rule_id = String::new();
                        let mut found_dot = false;

                        // First char of rule ID must be lowercase letter
                        if let Some(&(_, c)) = chars.peek() {
                            if c.is_ascii_lowercase() {
                                rule_id.push(c);
                                chars.next();
                            } else {
                                continue; // invalid, skip
                            }
                        }

                        // Continue reading rule ID
                        let mut final_idx = end_idx;
                        while let Some(&(idx, c)) = chars.peek() {
                            final_idx = idx;
                            if c == ']' {
                                chars.next();
                                break;
                            } else if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
                                rule_id.push(c);
                                chars.next();
                            } else if c == '.' {
                                found_dot = true;
                                rule_id.push(c);
                                chars.next();
                            } else {
                                break; // invalid char
                            }
                        }

                        // Validate rule ID
                        if found_dot && !rule_id.ends_with('.') && !rule_id.is_empty() {
                            let span = SourceSpan::new(bracket_start, final_idx - start_idx + 1);
                            rules.references.push(RuleReference {
                                verb,
                                rule_id,
                                file: path.to_path_buf(),
                                line: base_line,
                                span,
                            });
                        }
                    } else {
                        // Not a known verb - just ignore it. We only match rule
                        // references with known verbs: impl, verify, define, depends, related
                        // This avoids false positives on things like [payload bytes]
                    }
                } else if next_char == ']' {
                    // Immediate close - this is [rule.id] format (legacy)
                    // [impl ref.verb.default]
                    chars.next(); // consume ]

                    // Validate: must contain dot, not end with dot
                    if first_word.contains('.') && !first_word.ends_with('.') {
                        let span = SourceSpan::new(bracket_start, end_idx - start_idx + 1);
                        rules.references.push(RuleReference {
                            verb: RefVerb::Impl, // default to impl
                            rule_id: first_word,
                            file: path.to_path_buf(),
                            line: base_line,
                            span,
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_reference_legacy() {
        let content = r#"
            // See [channel.id.allocation] for details
            fn allocate_id() {}
        "#;

        let rules = Rules::extract_from_content(Path::new("test.rs"), content);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules.references[0].rule_id, "channel.id.allocation");
        assert_eq!(rules.references[0].verb, RefVerb::Impl);
    }

    #[test]
    fn test_extract_with_explicit_verb() {
        let content = r#"
            // [impl channel.id.allocation]
            fn allocate_id() {}

            // [verify channel.id.parity]
            #[test]
            fn test_parity() {}

            // [depends channel.framing]
            fn needs_framing() {}

            // [related channel.errors]
            fn handle_errors() {}

            // [define channel.id.format]
            // This is where we define the format
        "#;

        let rules = Rules::extract_from_content(Path::new("test.rs"), content);
        assert_eq!(rules.len(), 5);

        assert_eq!(rules.references[0].verb, RefVerb::Impl);
        assert_eq!(rules.references[0].rule_id, "channel.id.allocation");

        assert_eq!(rules.references[1].verb, RefVerb::Verify);
        assert_eq!(rules.references[1].rule_id, "channel.id.parity");

        assert_eq!(rules.references[2].verb, RefVerb::Depends);
        assert_eq!(rules.references[2].rule_id, "channel.framing");

        assert_eq!(rules.references[3].verb, RefVerb::Related);
        assert_eq!(rules.references[3].rule_id, "channel.errors");

        assert_eq!(rules.references[4].verb, RefVerb::Define);
        assert_eq!(rules.references[4].rule_id, "channel.id.format");
    }

    #[test]
    fn test_extract_multiple_references() {
        let content = r#"
            /// Implements [channel.id.parity] and [channel.id.no-reuse]
            fn next_channel_id() {}
        "#;

        let rules = Rules::extract_from_content(Path::new("test.rs"), content);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules.references[0].rule_id, "channel.id.parity");
        assert_eq!(rules.references[1].rule_id, "channel.id.no-reuse");
    }

    #[test]
    fn test_mixed_syntax() {
        let content = r#"
            // Legacy: [channel.id.one] and explicit: [verify channel.id.two]
            fn foo() {}
        "#;

        let rules = Rules::extract_from_content(Path::new("test.rs"), content);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules.references[0].rule_id, "channel.id.one");
        assert_eq!(rules.references[0].verb, RefVerb::Impl);
        assert_eq!(rules.references[1].rule_id, "channel.id.two");
        assert_eq!(rules.references[1].verb, RefVerb::Verify);
    }

    #[test]
    fn test_ignore_non_rule_brackets() {
        let content = r#"
            // array[0] is not a rule
            // [Some text] is not a rule either
            fn foo() {}
        "#;

        let rules = Rules::extract_from_content(Path::new("test.rs"), content);
        assert_eq!(rules.len(), 0);
    }

    #[test]
    fn test_unknown_verb_ignored() {
        // Unknown verbs are silently ignored to avoid false positives
        // on things like [payload bytes] in documentation
        let content = r#"
            // [frobnicate rule.id]
            // [payload bytes]
            fn foo() {}
        "#;

        let rules = Rules::extract_from_content(Path::new("test.rs"), content);
        assert_eq!(rules.len(), 0);
        assert_eq!(rules.warnings.len(), 0); // No warnings for unknown verbs
    }

    #[test]
    fn test_verb_display() {
        assert_eq!(RefVerb::Impl.to_string(), "impl");
        assert_eq!(RefVerb::Verify.to_string(), "verify");
        assert_eq!(RefVerb::Depends.to_string(), "depends");
        assert_eq!(RefVerb::Related.to_string(), "related");
        assert_eq!(RefVerb::Define.to_string(), "define");
    }

    #[test]
    fn test_span_tracking() {
        let content = "// [impl foo.bar]";
        let rules = Rules::extract_from_content(Path::new("test.rs"), content);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules.references[0].span.offset, 3); // after "// "
    }
}
