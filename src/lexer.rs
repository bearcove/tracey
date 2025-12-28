//! Rust lexer for extracting comments and finding rule references
//!
//! We use rustc's built-in lexer for tokenization, which gives us proper
//! handling of all Rust syntax edge cases.

use eyre::Result;
use std::path::Path;

/// The relationship type between code and a spec rule
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    pub fn from_str(s: &str) -> Option<Self> {
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
#[derive(Debug, Clone)]
pub struct RuleReference {
    /// The relationship type (impl, verify, depends, etc.)
    pub verb: RefVerb,
    /// The rule ID (e.g., "channel.id.allocation")
    pub rule_id: String,
    /// File where the reference was found
    pub file: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// The full comment text containing the reference
    #[allow(dead_code)]
    pub context: String,
}

/// Extract all rule references from a Rust source file
///
/// Looks for patterns like `[verb rule.id]` or `[rule.id]` in comments.
/// This matches the syntax used in code to reference spec rules:
/// - `// [impl channel.id.allocation]` - explicit implementation
/// - `// [verify channel.id.parity]` - test verification
/// - `// [depends channel.framing]` - strict dependency
/// - `// [related channel.errors]` - loose connection
/// - `// [channel.id.parity]` - legacy syntax, defaults to impl
pub fn extract_rule_references(path: &Path, content: &str) -> Result<Vec<RuleReference>> {
    let mut references = Vec::new();
    let file_str = path.display().to_string();

    // Simple approach: scan for comments and extract [rule.id] patterns
    // We look for both // and /// comments, as well as /* */ blocks

    for (line_idx, line) in content.lines().enumerate() {
        let line_num = line_idx + 1;

        // Check for line comments (// or ///)
        if let Some(comment_start) = line.find("//") {
            let comment = &line[comment_start..];
            extract_references_from_text(comment, &file_str, line_num, &mut references);
        }
    }

    // Also handle block comments /* */
    // For simplicity, we'll do a pass looking for block comments
    let mut in_block_comment = false;
    let mut block_comment_start_line = 0;
    let mut block_comment_content = String::new();

    for (line_idx, line) in content.lines().enumerate() {
        let line_num = line_idx + 1;

        if in_block_comment {
            if let Some(end_pos) = line.find("*/") {
                block_comment_content.push_str(&line[..end_pos]);
                extract_references_from_text(
                    &block_comment_content,
                    &file_str,
                    block_comment_start_line,
                    &mut references,
                );
                in_block_comment = false;
                block_comment_content.clear();
            } else {
                block_comment_content.push_str(line);
                block_comment_content.push('\n');
            }
        } else if let Some(start_pos) = line.find("/*") {
            in_block_comment = true;
            block_comment_start_line = line_num;
            let rest = &line[start_pos + 2..];
            if let Some(end_pos) = rest.find("*/") {
                // Single-line block comment
                let comment = &rest[..end_pos];
                extract_references_from_text(comment, &file_str, line_num, &mut references);
                in_block_comment = false;
            } else {
                block_comment_content.push_str(rest);
                block_comment_content.push('\n');
            }
        }
    }

    Ok(references)
}

/// Extract rule references from a piece of text (comment content)
///
/// Supports two syntax forms:
/// - `[verb rule.id]` - explicit verb (impl, verify, depends, related, define)
/// - `[rule.id]` - legacy syntax, defaults to impl
fn extract_references_from_text(
    text: &str,
    file: &str,
    line: usize,
    references: &mut Vec<RuleReference>,
) {
    let mut chars = text.char_indices().peekable();

    while let Some((_start_idx, ch)) = chars.next() {
        if ch == '[' {
            // Potential rule reference start
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
            if let Some(&(_, next_char)) = chars.peek() {
                if next_char == ' ' {
                    // Space after first word - might be [verb rule.id]
                    if let Some(verb) = RefVerb::from_str(&first_word) {
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
                        while let Some(&(_, c)) = chars.peek() {
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
                            references.push(RuleReference {
                                verb,
                                rule_id,
                                file: file.to_string(),
                                line,
                                context: text.trim().to_string(),
                            });
                        }
                    }
                    // If first word isn't a valid verb, skip this bracket
                } else if next_char == ']' {
                    // Immediate close - this is [rule.id] format (legacy)
                    chars.next(); // consume ]

                    // Validate: must contain dot, not end with dot
                    if first_word.contains('.') && !first_word.ends_with('.') {
                        references.push(RuleReference {
                            verb: RefVerb::Impl, // default to impl
                            rule_id: first_word,
                            file: file.to_string(),
                            line,
                            context: text.trim().to_string(),
                        });
                    }
                }
                // Any other char means this isn't a valid reference
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_extract_simple_reference_legacy() {
        let content = r#"
            // See [channel.id.allocation] for details
            fn allocate_id() {}
        "#;

        let refs = extract_rule_references(&PathBuf::from("test.rs"), content).unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].rule_id, "channel.id.allocation");
        assert_eq!(refs[0].verb, RefVerb::Impl); // legacy defaults to impl
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

        let refs = extract_rule_references(&PathBuf::from("test.rs"), content).unwrap();
        assert_eq!(refs.len(), 5);

        assert_eq!(refs[0].verb, RefVerb::Impl);
        assert_eq!(refs[0].rule_id, "channel.id.allocation");

        assert_eq!(refs[1].verb, RefVerb::Verify);
        assert_eq!(refs[1].rule_id, "channel.id.parity");

        assert_eq!(refs[2].verb, RefVerb::Depends);
        assert_eq!(refs[2].rule_id, "channel.framing");

        assert_eq!(refs[3].verb, RefVerb::Related);
        assert_eq!(refs[3].rule_id, "channel.errors");

        assert_eq!(refs[4].verb, RefVerb::Define);
        assert_eq!(refs[4].rule_id, "channel.id.format");
    }

    #[test]
    fn test_extract_multiple_references() {
        let content = r#"
            /// Implements [channel.id.parity] and [channel.id.no-reuse]
            fn next_channel_id() {}
        "#;

        let refs = extract_rule_references(&PathBuf::from("test.rs"), content).unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].rule_id, "channel.id.parity");
        assert_eq!(refs[1].rule_id, "channel.id.no-reuse");
    }

    #[test]
    fn test_mixed_syntax() {
        let content = r#"
            // Legacy: [channel.id.one] and explicit: [verify channel.id.two]
            fn foo() {}
        "#;

        let refs = extract_rule_references(&PathBuf::from("test.rs"), content).unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].rule_id, "channel.id.one");
        assert_eq!(refs[0].verb, RefVerb::Impl);
        assert_eq!(refs[1].rule_id, "channel.id.two");
        assert_eq!(refs[1].verb, RefVerb::Verify);
    }

    #[test]
    fn test_ignore_non_rule_brackets() {
        let content = r#"
            // array[0] is not a rule
            // [Some text] is not a rule either
            // [unknown-verb rule.id] is not valid
            fn foo() {}
        "#;

        let refs = extract_rule_references(&PathBuf::from("test.rs"), content).unwrap();
        assert_eq!(refs.len(), 0);
    }

    #[test]
    fn test_verb_display() {
        assert_eq!(RefVerb::Impl.to_string(), "impl");
        assert_eq!(RefVerb::Verify.to_string(), "verify");
        assert_eq!(RefVerb::Depends.to_string(), "depends");
        assert_eq!(RefVerb::Related.to_string(), "related");
        assert_eq!(RefVerb::Define.to_string(), "define");
    }
}
