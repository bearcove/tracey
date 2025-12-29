//! Code unit extraction for reverse traceability
//!
//! This module provides tree-sitter based parsing to identify "code units"
//! (functions, structs, impls, etc.) and associate them with spec references
//! found in their comments.
//!
//! # What is reverse traceability?
//!
//! While forward traceability asks "what % of spec rules have implementations?",
//! reverse traceability asks "what % of code is linked to spec requirements?"
//!
//! This helps identify:
//! - Code that exists without being specified (potential spec gaps)
//! - Code added without updating the spec
//! - Potential dead code or technical debt

use arborium::tree_sitter::{Node, Parser};
use std::path::{Path, PathBuf};

/// A semantic unit of code (function, struct, impl, etc.)
#[derive(Debug, Clone)]
pub struct CodeUnit {
    /// The kind of code unit (e.g., "function", "struct", "impl")
    pub kind: CodeUnitKind,
    /// Name of the code unit (if it has one)
    pub name: Option<String>,
    /// File where this code unit is defined
    pub file: PathBuf,
    /// Line number where the code unit starts (1-indexed)
    pub start_line: usize,
    /// Line number where the code unit ends (1-indexed)
    pub end_line: usize,
    /// Byte offset where the code unit starts
    pub start_byte: usize,
    /// Byte offset where the code unit ends
    pub end_byte: usize,
    /// Rule IDs referenced in comments associated with this code unit
    pub rule_refs: Vec<String>,
}

/// The kind of code unit
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeUnitKind {
    /// A function or method
    Function,
    /// A struct definition
    Struct,
    /// An enum definition
    Enum,
    /// A trait definition
    Trait,
    /// An impl block
    Impl,
    /// A module
    Module,
    /// A constant
    Const,
    /// A static variable
    Static,
    /// A type alias
    TypeAlias,
    /// A macro definition
    Macro,
}

impl CodeUnitKind {
    /// Get the display name for this kind
    pub fn as_str(&self) -> &'static str {
        match self {
            CodeUnitKind::Function => "function",
            CodeUnitKind::Struct => "struct",
            CodeUnitKind::Enum => "enum",
            CodeUnitKind::Trait => "trait",
            CodeUnitKind::Impl => "impl",
            CodeUnitKind::Module => "module",
            CodeUnitKind::Const => "const",
            CodeUnitKind::Static => "static",
            CodeUnitKind::TypeAlias => "type",
            CodeUnitKind::Macro => "macro",
        }
    }
}

impl std::fmt::Display for CodeUnitKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Result of extracting code units from a source file
#[derive(Debug, Clone, Default)]
pub struct CodeUnits {
    /// All code units found
    pub units: Vec<CodeUnit>,
}

impl CodeUnits {
    /// Create an empty CodeUnits collection
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of code units
    pub fn len(&self) -> usize {
        self.units.len()
    }

    /// Whether there are no code units
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// Count of code units with at least one rule reference
    pub fn covered_count(&self) -> usize {
        self.units
            .iter()
            .filter(|u| !u.rule_refs.is_empty())
            .count()
    }

    /// Count of code units without any rule references
    pub fn uncovered_count(&self) -> usize {
        self.units.iter().filter(|u| u.rule_refs.is_empty()).count()
    }

    /// Reverse coverage percentage (0.0 to 100.0)
    pub fn coverage_percent(&self) -> f64 {
        if self.units.is_empty() {
            return 100.0;
        }
        (self.covered_count() as f64 / self.units.len() as f64) * 100.0
    }

    /// Get all uncovered code units
    pub fn uncovered(&self) -> impl Iterator<Item = &CodeUnit> {
        self.units.iter().filter(|u| u.rule_refs.is_empty())
    }

    /// Get all covered code units
    pub fn covered(&self) -> impl Iterator<Item = &CodeUnit> {
        self.units.iter().filter(|u| !u.rule_refs.is_empty())
    }

    /// Merge another CodeUnits into this one
    pub fn extend(&mut self, other: CodeUnits) {
        self.units.extend(other.units);
    }
}

/// Extract code units from Rust source code
pub fn extract_rust(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_rust::language().into())
        .expect("Failed to load Rust grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();

    // Walk the tree and extract code units
    extract_units_recursive(path, source, root, &mut units);

    units
}

fn extract_units_recursive(path: &Path, source: &str, node: Node, units: &mut CodeUnits) {
    // Check if this node is a code unit we care about
    if let Some(unit) = node_to_code_unit(path, source, node) {
        units.units.push(unit);
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_units_recursive(path, source, child, units);
    }
}

fn node_to_code_unit(path: &Path, source: &str, node: Node) -> Option<CodeUnit> {
    let kind = match node.kind() {
        "function_item" => CodeUnitKind::Function,
        "struct_item" => CodeUnitKind::Struct,
        "enum_item" => CodeUnitKind::Enum,
        "trait_item" => CodeUnitKind::Trait,
        "impl_item" => CodeUnitKind::Impl,
        "mod_item" => CodeUnitKind::Module,
        "const_item" => CodeUnitKind::Const,
        "static_item" => CodeUnitKind::Static,
        "type_item" => CodeUnitKind::TypeAlias,
        "macro_definition" => CodeUnitKind::Macro,
        _ => return None,
    };

    // Get the name if available
    let name = get_node_name(source, node);

    // Find associated comments and extract rule references
    let rule_refs = extract_rule_refs_from_comments(source, node);

    Some(CodeUnit {
        kind,
        name,
        file: path.to_path_buf(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        rule_refs,
    })
}

fn get_node_name(source: &str, node: Node) -> Option<String> {
    // Different node types have the name in different child fields
    let name_node = match node.kind() {
        "function_item" => node.child_by_field_name("name"),
        "struct_item" => node.child_by_field_name("name"),
        "enum_item" => node.child_by_field_name("name"),
        "trait_item" => node.child_by_field_name("name"),
        "impl_item" => {
            // For impl, get the type being implemented
            node.child_by_field_name("type")
        }
        "mod_item" => node.child_by_field_name("name"),
        "const_item" => node.child_by_field_name("name"),
        "static_item" => node.child_by_field_name("name"),
        "type_item" => node.child_by_field_name("name"),
        "macro_definition" => node.child_by_field_name("name"),
        _ => None,
    };

    name_node.map(|n| source[n.byte_range()].to_string())
}

fn extract_rule_refs_from_comments(source: &str, node: Node) -> Vec<String> {
    let mut refs = Vec::new();

    // Look for comments that precede this node
    // Collect all siblings before this node, then walk backwards to find consecutive comments
    if let Some(parent) = node.parent() {
        let mut cursor = parent.walk();
        let mut preceding_siblings: Vec<Node> = Vec::new();

        for child in parent.children(&mut cursor) {
            if child.id() == node.id() {
                break;
            }
            preceding_siblings.push(child);
        }

        // Walk backwards through preceding siblings, collecting comments
        // Stop when we hit something that's not a comment or attribute
        for sibling in preceding_siblings.into_iter().rev() {
            let is_comment_like = matches!(
                sibling.kind(),
                "line_comment" | "block_comment" | "attribute_item"
            );
            if is_comment_like {
                collect_comment_refs(source, sibling, &mut refs);
            } else {
                // Stop at first non-comment node
                break;
            }
        }
    }

    // Check for doc comments and inner comments that are children of this node
    collect_inner_comment_refs(source, node, &mut refs);

    refs
}

/// Recursively collect comment refs from a node's children
fn collect_inner_comment_refs(source: &str, node: Node, refs: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "line_comment" | "block_comment" => {
                extract_refs_from_comment_text(source, child, refs);
            }
            // Doc comments are in attributes -> line_outer_doc_comment -> doc_comment
            "attributes"
            | "line_outer_doc_comment"
            | "block_outer_doc_comment"
            | "line_inner_doc_comment"
            | "block_inner_doc_comment" => {
                collect_inner_comment_refs(source, child, refs);
            }
            "doc_comment" => {
                // The actual content of a doc comment
                let text = &source[child.byte_range()];
                for cap in find_rule_refs(text) {
                    if !refs.contains(&cap) {
                        refs.push(cap);
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_comment_refs(source: &str, node: Node, refs: &mut Vec<String>) {
    match node.kind() {
        "line_comment" | "block_comment" => {
            extract_refs_from_comment_text(source, node, refs);
        }
        "attribute_item" => {
            // Could be a doc attribute, check children
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_comment_refs(source, child, refs);
            }
        }
        _ => {}
    }
}

fn extract_refs_from_comment_text(source: &str, node: Node, refs: &mut Vec<String>) {
    let text = &source[node.byte_range()];

    // Reuse the same pattern matching from the lexer
    // Look for [verb rule.id] or [rule.id] patterns
    for cap in find_rule_refs(text) {
        if !refs.contains(&cap) {
            refs.push(cap);
        }
    }
}

/// Extract rule IDs from comment text
fn find_rule_refs(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some((_, ch)) = chars.next() {
        if ch == '[' {
            // Try to parse a rule reference
            if let Some(rule_id) = try_parse_rule_ref(&mut chars) {
                refs.push(rule_id);
            }
        }
    }

    refs
}

fn try_parse_rule_ref(
    chars: &mut std::iter::Peekable<impl Iterator<Item = (usize, char)>>,
) -> Option<String> {
    // First char must be lowercase letter
    let first_char = chars.peek().map(|(_, c)| *c)?;
    if !first_char.is_ascii_lowercase() {
        return None;
    }

    let mut first_word = String::new();
    first_word.push(first_char);
    chars.next();

    // Read the first word
    while let Some(&(_, c)) = chars.peek() {
        if c == ']' || c == ' ' {
            break;
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.' {
            first_word.push(c);
            chars.next();
        } else {
            return None;
        }
    }

    // Check what follows
    match chars.peek().map(|(_, c)| *c) {
        Some(' ') => {
            // Might be [verb rule.id]
            let verbs = ["impl", "verify", "define", "depends", "related"];
            if verbs.contains(&first_word.as_str()) {
                chars.next(); // consume space

                // Read the rule ID
                let mut rule_id = String::new();
                let mut has_dot = false;

                // First char must be lowercase
                if let Some(&(_, c)) = chars.peek() {
                    if c.is_ascii_lowercase() {
                        rule_id.push(c);
                        chars.next();
                    } else {
                        return None;
                    }
                }

                while let Some(&(_, c)) = chars.peek() {
                    if c == ']' {
                        chars.next();
                        break;
                    } else if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
                        rule_id.push(c);
                        chars.next();
                    } else if c == '.' {
                        has_dot = true;
                        rule_id.push(c);
                        chars.next();
                    } else {
                        return None;
                    }
                }

                if has_dot && !rule_id.ends_with('.') && !rule_id.is_empty() {
                    return Some(rule_id);
                }
            }
            None
        }
        Some(']') => {
            chars.next(); // consume ]
            // [rule.id] format - must contain dot
            if first_word.contains('.') && !first_word.ends_with('.') {
                Some(first_word)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_function() {
        let source = r#"
fn foo() {}

fn bar() {}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        assert_eq!(units.len(), 2);
        assert_eq!(units.units[0].name.as_deref(), Some("foo"));
        assert_eq!(units.units[1].name.as_deref(), Some("bar"));
    }

    #[test]
    fn test_extract_struct() {
        let source = r#"
struct Foo {
    x: i32,
}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        assert_eq!(units.len(), 1);
        assert_eq!(units.units[0].kind, CodeUnitKind::Struct);
        assert_eq!(units.units[0].name.as_deref(), Some("Foo"));
    }

    #[test]
    fn test_extract_with_comment_ref() {
        let source = r#"
// [impl foo.bar]
fn do_thing() {}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        assert_eq!(units.len(), 1);
        assert_eq!(units.units[0].rule_refs, vec!["foo.bar"]);
    }

    #[test]
    fn test_extract_with_verb_ref() {
        let source = r#"
// [verify channel.id.parity]
#[test]
fn test_parity() {}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        assert_eq!(units.len(), 1);
        assert_eq!(units.units[0].rule_refs, vec!["channel.id.parity"]);
    }

    #[test]
    fn test_coverage_calculation() {
        let source = r#"
// [impl foo.bar]
fn covered() {}

fn uncovered() {}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        assert_eq!(units.len(), 2);
        assert_eq!(units.covered_count(), 1);
        assert_eq!(units.uncovered_count(), 1);
        assert_eq!(units.coverage_percent(), 50.0);
    }

    #[test]
    fn test_find_rule_refs() {
        assert_eq!(find_rule_refs("// [impl foo.bar]"), vec!["foo.bar"]);
        assert_eq!(find_rule_refs("// [foo.bar]"), vec!["foo.bar"]);
        assert_eq!(
            find_rule_refs("// [impl a.b] and [verify c.d]"),
            vec!["a.b", "c.d"]
        );
        assert!(find_rule_refs("// no refs here").is_empty());
        assert!(find_rule_refs("// [invalid]").is_empty()); // no dot
    }

    #[test]
    fn test_multiple_refs_same_unit() {
        let source = r#"
// [impl rule.one]
// [verify rule.two]
fn multi_ref() {}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        assert_eq!(units.len(), 1);
        // Should capture both refs
        assert!(units.units[0].rule_refs.contains(&"rule.one".to_string()));
        assert!(units.units[0].rule_refs.contains(&"rule.two".to_string()));
    }

    #[test]
    fn test_doc_comment_refs() {
        let source = r#"
/// Documentation for the function
/// [impl doc.ref]
fn documented() {}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        assert_eq!(units.len(), 1);
        assert_eq!(units.units[0].rule_refs, vec!["doc.ref"]);
    }

    #[test]
    fn test_impl_block() {
        let source = r#"
// [impl my.impl]
impl Foo {
    fn method(&self) {}
}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        // Should find both the impl and the method
        let impl_unit = units.units.iter().find(|u| u.kind == CodeUnitKind::Impl);
        assert!(impl_unit.is_some());
        assert_eq!(impl_unit.unwrap().rule_refs, vec!["my.impl"]);
    }

    #[test]
    fn test_all_code_unit_kinds() {
        let source = r#"
fn a_function() {}
struct AStruct {}
enum AnEnum {}
trait ATrait {}
impl ATrait for AStruct {}
mod a_module {}
const A_CONST: i32 = 0;
static A_STATIC: i32 = 0;
type AType = i32;
"#;
        let units = extract_rust(Path::new("test.rs"), source);

        let kinds: Vec<_> = units.units.iter().map(|u| u.kind).collect();
        assert!(kinds.contains(&CodeUnitKind::Function));
        assert!(kinds.contains(&CodeUnitKind::Struct));
        assert!(kinds.contains(&CodeUnitKind::Enum));
        assert!(kinds.contains(&CodeUnitKind::Trait));
        assert!(kinds.contains(&CodeUnitKind::Impl));
        assert!(kinds.contains(&CodeUnitKind::Module));
        assert!(kinds.contains(&CodeUnitKind::Const));
        assert!(kinds.contains(&CodeUnitKind::Static));
        assert!(kinds.contains(&CodeUnitKind::TypeAlias));
    }
}
