//! Code unit extraction for reverse traceability
//!
//! This module provides tree-sitter based parsing to identify "code units"
//! (functions, structs, impls, etc.) and associate them with spec references
//! found in their comments.
//!
//! # What is reverse traceability?
//!
//! While forward traceability asks "what % of spec requirements have implementations?",
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
    /// Requirement IDs referenced in comments associated with this code unit
    pub req_refs: Vec<String>,
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

    /// Count of code units with at least one requirement reference
    pub fn covered_count(&self) -> usize {
        self.units.iter().filter(|u| !u.req_refs.is_empty()).count()
    }

    /// Count of code units without any requirement references
    pub fn uncovered_count(&self) -> usize {
        self.units.iter().filter(|u| u.req_refs.is_empty()).count()
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
        self.units.iter().filter(|u| u.req_refs.is_empty())
    }

    /// Get all covered code units
    pub fn covered(&self) -> impl Iterator<Item = &CodeUnit> {
        self.units.iter().filter(|u| !u.req_refs.is_empty())
    }

    /// Merge another CodeUnits into this one
    pub fn extend(&mut self, other: CodeUnits) {
        self.units.extend(other.units);
    }
}

/// Extract code units from source code, auto-detecting language from file extension
pub fn extract(path: &Path, source: &str) -> CodeUnits {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "rs" => extract_rust(path, source),
        "swift" => extract_swift(path, source),
        "go" => extract_go(path, source),
        "java" => extract_java(path, source),
        "py" => extract_python(path, source),
        "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" => extract_typescript(path, source),
        "php" => extract_php(path, source),
        _ => CodeUnits::new(),
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
    extract_units_recursive(path, source, root, &mut units, rust_node_kind);

    units
}

/// Extract code units from Swift source code
pub fn extract_swift(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_swift::language().into())
        .expect("Failed to load Swift grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, swift_node_kind);
    units
}

/// Extract code units from Go source code
pub fn extract_go(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_go::language().into())
        .expect("Failed to load Go grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, go_node_kind);
    units
}

/// Extract code units from Java source code
pub fn extract_java(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_java::language().into())
        .expect("Failed to load Java grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, java_node_kind);
    units
}

/// Extract code units from Python source code
pub fn extract_python(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_python::language().into())
        .expect("Failed to load Python grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, python_node_kind);
    units
}

/// Extract code units from TypeScript source code
pub fn extract_typescript(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_typescript::language().into())
        .expect("Failed to load TypeScript grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, typescript_node_kind);
    units
}

// Language-specific node kind mappings

fn rust_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_item" => Some(CodeUnitKind::Function),
        "struct_item" => Some(CodeUnitKind::Struct),
        "enum_item" => Some(CodeUnitKind::Enum),
        "trait_item" => Some(CodeUnitKind::Trait),
        "impl_item" => Some(CodeUnitKind::Impl),
        "mod_item" => Some(CodeUnitKind::Module),
        "const_item" => Some(CodeUnitKind::Const),
        "static_item" => Some(CodeUnitKind::Static),
        "type_item" => Some(CodeUnitKind::TypeAlias),
        "macro_definition" => Some(CodeUnitKind::Macro),
        _ => None,
    }
}

fn swift_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_declaration" => Some(CodeUnitKind::Function),
        "class_declaration" => Some(CodeUnitKind::Struct), // Swift class as struct-like
        "struct_declaration" => Some(CodeUnitKind::Struct),
        "enum_declaration" => Some(CodeUnitKind::Enum),
        "protocol_declaration" => Some(CodeUnitKind::Trait), // Swift protocol as trait-like
        "extension_declaration" => Some(CodeUnitKind::Impl), // Swift extension as impl-like
        _ => None,
    }
}

fn go_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_declaration" => Some(CodeUnitKind::Function),
        "method_declaration" => Some(CodeUnitKind::Function),
        "type_declaration" => Some(CodeUnitKind::Struct), // Could be struct or interface
        _ => None,
    }
}

fn java_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "method_declaration" => Some(CodeUnitKind::Function),
        "constructor_declaration" => Some(CodeUnitKind::Function),
        "class_declaration" => Some(CodeUnitKind::Struct),
        "interface_declaration" => Some(CodeUnitKind::Trait),
        "enum_declaration" => Some(CodeUnitKind::Enum),
        _ => None,
    }
}

fn python_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_definition" => Some(CodeUnitKind::Function),
        "class_definition" => Some(CodeUnitKind::Struct),
        _ => None,
    }
}

fn typescript_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_declaration" => Some(CodeUnitKind::Function),
        "method_definition" => Some(CodeUnitKind::Function),
        "class_declaration" => Some(CodeUnitKind::Struct),
        "interface_declaration" => Some(CodeUnitKind::Trait),
        "type_alias_declaration" => Some(CodeUnitKind::TypeAlias),
        "enum_declaration" => Some(CodeUnitKind::Enum),
        _ => None,
    }
}

fn php_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_definition" => Some(CodeUnitKind::Function),
        "method_declaration" => Some(CodeUnitKind::Function),
        "class_declaration" => Some(CodeUnitKind::Struct),
        "interface_declaration" => Some(CodeUnitKind::Trait),
        "trait_declaration" => Some(CodeUnitKind::Trait),
        "enum_declaration" => Some(CodeUnitKind::Enum),
        _ => None,
    }
}

/// Extract code units from PHP source code
pub fn extract_php(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_php::language().into())
        .expect("Failed to load PHP grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, php_node_kind);
    units
}

fn extract_units_recursive<F>(
    path: &Path,
    source: &str,
    node: Node,
    units: &mut CodeUnits,
    node_kind_mapper: F,
) where
    F: Fn(&str) -> Option<CodeUnitKind> + Copy,
{
    // Check if this node is a code unit we care about
    if let Some(unit) = node_to_code_unit(path, source, node, &node_kind_mapper) {
        units.units.push(unit);
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_units_recursive(path, source, child, units, node_kind_mapper);
    }
}

// r[impl code-unit.definition]
fn node_to_code_unit<F>(
    path: &Path,
    source: &str,
    node: Node,
    node_kind_mapper: &F,
) -> Option<CodeUnit>
where
    F: Fn(&str) -> Option<CodeUnitKind>,
{
    let kind = node_kind_mapper(node.kind())?;

    // Get the name if available
    let name = get_node_name(source, node);

    // r[impl code-unit.refs.extraction]
    // r[impl code-unit.boundary.include-comments]
    // Find associated comments and extract requirement references
    // Also get the earliest comment line to extend the code unit's range
    let (req_refs, comment_start) = extract_req_refs_from_comments(source, node);

    // The code unit starts at the earliest associated comment (if any),
    // otherwise at the node itself
    let start_line = comment_start.unwrap_or_else(|| node.start_position().row + 1);
    let start_byte = if comment_start.is_some() {
        // Find the byte offset of the comment start line
        find_line_start_byte(source, start_line)
    } else {
        node.start_byte()
    };

    Some(CodeUnit {
        kind,
        name,
        file: path.to_path_buf(),
        start_line,
        end_line: node.end_position().row + 1,
        start_byte,
        end_byte: node.end_byte(),
        req_refs,
    })
}

/// Find the byte offset where a given line (1-indexed) starts
fn find_line_start_byte(source: &str, line: usize) -> usize {
    let mut current_line = 1;
    for (byte_pos, ch) in source.char_indices() {
        if current_line == line {
            return byte_pos;
        }
        if ch == '\n' {
            current_line += 1;
        }
    }
    0
}

fn get_node_name(source: &str, node: Node) -> Option<String> {
    // Try common field names used across languages for the identifier/name
    // Most tree-sitter grammars use "name" for the identifier field
    let name_node = node
        .child_by_field_name("name")
        .or_else(|| node.child_by_field_name("type")) // For impl blocks
        .or_else(|| {
            // For some languages, the first identifier child is the name
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|c| c.kind() == "identifier" || c.kind() == "type_identifier")
        });

    // Legacy Rust-specific handling (kept for compatibility)
    let name_node = name_node.or_else(|| match node.kind() {
        "function_item" => node.child_by_field_name("name"),
        "struct_item" => node.child_by_field_name("name"),
        "enum_item" => node.child_by_field_name("name"),
        "trait_item" => node.child_by_field_name("name"),
        "impl_item" => node.child_by_field_name("type"),
        "mod_item" => node.child_by_field_name("name"),
        "const_item" => node.child_by_field_name("name"),
        "static_item" => node.child_by_field_name("name"),
        "type_item" => node.child_by_field_name("name"),
        "macro_definition" => node.child_by_field_name("name"),
        _ => None,
    });

    name_node.map(|n| source[n.byte_range()].to_string())
}

/// Returns (requirement refs, earliest comment line if any)
fn extract_req_refs_from_comments(source: &str, node: Node) -> (Vec<String>, Option<usize>) {
    let mut refs = Vec::new();
    let mut earliest_comment_line: Option<usize> = None;

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
            // r[impl code-unit.boundary.include-comments]
            // Different languages use different node kinds for comments:
            // - Rust: line_comment, block_comment, attribute_item
            // - Swift/Go/TypeScript: comment
            // - Python: comment
            let is_comment_like = matches!(
                sibling.kind(),
                "line_comment"
                    | "block_comment"
                    | "comment"
                    | "attribute_item"
                    | "decorator"       // Python decorators
                    | "multiline_comment"
            );
            if is_comment_like {
                collect_comment_refs(source, sibling, &mut refs);
                // Track the earliest comment line (1-indexed)
                let sibling_line = sibling.start_position().row + 1;
                earliest_comment_line =
                    Some(earliest_comment_line.map_or(sibling_line, |l| l.min(sibling_line)));
            } else {
                // Stop at first non-comment node
                break;
            }
        }
    }

    // Check for doc comments and inner comments that are children of this node
    collect_inner_comment_refs(source, node, &mut refs);

    (refs, earliest_comment_line)
}

/// Recursively collect comment refs from a node's children
fn collect_inner_comment_refs(source: &str, node: Node, refs: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "line_comment" | "block_comment" | "comment" | "multiline_comment" => {
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
                for cap in find_req_refs(text) {
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
        "line_comment" | "block_comment" | "comment" | "multiline_comment" => {
            extract_refs_from_comment_text(source, node, refs);
        }
        "attribute_item" | "decorator" => {
            // Could be a doc attribute or decorator, check children
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
    // Look for [verb req.id] or [req.id] patterns
    for cap in find_req_refs(text) {
        if !refs.contains(&cap) {
            refs.push(cap);
        }
    }
}

/// Extract requirement IDs from comment text
fn find_req_refs(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some((_, ch)) = chars.next() {
        if ch == '[' {
            // Try to parse a requirement reference
            if let Some(req_id) = try_parse_req_ref(&mut chars) {
                refs.push(req_id);
            }
        }
    }

    refs
}

/// A full requirement reference with all metadata
#[derive(Debug, Clone)]
pub struct FullReqRef {
    /// The prefix identifying which spec (e.g., "r", "h2")
    pub prefix: String,
    /// The verb (impl, verify, depends, related, define)
    pub verb: String,
    /// The requirement ID
    pub req_id: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Byte offset of the reference start
    pub byte_offset: usize,
    /// Byte length of the reference
    pub byte_length: usize,
}

/// Extract ALL requirement references from a file using tree-sitter
///
/// r[impl ref.parser.tree-sitter]
/// r[impl ref.parser.languages]
/// r[impl ref.parser.unified]
pub fn extract_refs(path: &Path, source: &str) -> Vec<FullReqRef> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let language = match ext {
        "rs" => arborium_rust::language(),
        "swift" => arborium_swift::language(),
        "go" => arborium_go::language(),
        "java" => arborium_java::language(),
        "py" => arborium_python::language(),
        "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" => arborium_typescript::language(),
        "php" => arborium_php::language(),
        _ => return Vec::new(),
    };

    let mut parser = Parser::new();
    parser
        .set_language(&language.into())
        .expect("Failed to load grammar");

    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };

    let mut refs = Vec::new();
    extract_refs_recursive(source, tree.root_node(), &mut refs);
    refs
}

fn extract_refs_recursive(source: &str, node: Node, refs: &mut Vec<FullReqRef>) {
    // Check if this is a comment node
    // Different languages and comment styles:
    // - Rust: line_comment (//), block_comment (/* */),
    //         line_outer_doc_comment (///), line_inner_doc_comment (//!),
    //         block_outer_doc_comment (/** */), block_inner_doc_comment (/*! */)
    // - Swift/Go/TypeScript: comment
    // - Python: comment
    let is_comment = matches!(
        node.kind(),
        "line_comment"
            | "block_comment"
            | "comment"
            | "multiline_comment"
            | "line_outer_doc_comment"
            | "line_inner_doc_comment"
            | "block_outer_doc_comment"
            | "block_inner_doc_comment"
    );

    if is_comment {
        let text = &source[node.byte_range()];
        let line = node.start_position().row + 1;
        let base_offset = node.start_byte();
        extract_full_refs_from_text(text, line, base_offset, refs);
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_refs_recursive(source, child, refs);
    }
}

// r[impl ref.syntax.surrounding-text]
fn extract_full_refs_from_text(
    text: &str,
    line: usize,
    base_offset: usize,
    refs: &mut Vec<FullReqRef>,
) {
    let mut chars = text.char_indices().peekable();

    while let Some((start_idx, ch)) = chars.next() {
        // Match prefix (lowercase alphanumeric) followed by '['
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            let prefix_start = start_idx;
            let mut prefix = String::new();
            prefix.push(ch);

            // Continue reading prefix
            while let Some(&(_, next_ch)) = chars.peek() {
                if next_ch == '[' {
                    break;
                } else if next_ch.is_ascii_lowercase() || next_ch.is_ascii_digit() {
                    prefix.push(next_ch);
                    chars.next();
                } else {
                    break;
                }
            }

            // Check for '['
            if chars.peek().map(|(_, c)| *c) != Some('[') {
                continue;
            }
            chars.next(); // consume '['

            // Parse: [verb req.id] or [req.id]
            if let Some((verb, req_id, end_idx)) = try_parse_full_ref(&mut chars) {
                refs.push(FullReqRef {
                    prefix,
                    verb,
                    req_id,
                    line,
                    byte_offset: base_offset + prefix_start,
                    byte_length: end_idx - prefix_start + 1,
                });
            }
        }
    }
}

// r[impl ref.syntax.req-id]
fn try_parse_full_ref(
    chars: &mut std::iter::Peekable<impl Iterator<Item = (usize, char)>>,
) -> Option<(String, String, usize)> {
    // First char must be lowercase letter
    let first_char = chars.peek().map(|(_, c)| *c)?;
    if !first_char.is_ascii_lowercase() {
        return None;
    }

    let mut first_word = String::new();
    first_word.push(first_char);
    chars.next();

    // Read the first word
    let mut end_idx = 0;
    while let Some(&(idx, c)) = chars.peek() {
        end_idx = idx;
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
            // Might be [verb req.id]
            let verbs = ["impl", "verify", "define", "depends", "related"];
            if verbs.contains(&first_word.as_str()) {
                let verb = first_word;
                chars.next(); // consume space

                // Read the requirement ID
                let mut req_id = String::new();
                let mut has_dot = false;

                // First char must be lowercase
                if let Some(&(_, c)) = chars.peek() {
                    if c.is_ascii_lowercase() {
                        req_id.push(c);
                        chars.next();
                    } else {
                        return None;
                    }
                }

                while let Some(&(idx, c)) = chars.peek() {
                    end_idx = idx;
                    if c == ']' {
                        chars.next();
                        break;
                    } else if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' {
                        req_id.push(c);
                        chars.next();
                    } else if c == '.' {
                        has_dot = true;
                        req_id.push(c);
                        chars.next();
                    } else {
                        return None;
                    }
                }

                if has_dot && !req_id.ends_with('.') && !req_id.is_empty() {
                    return Some((verb, req_id, end_idx));
                }
            }
            None
        }
        Some(']') => {
            chars.next(); // consume ]
            // [req.id] format - defaults to impl
            if first_word.contains('.') && !first_word.ends_with('.') {
                Some(("impl".to_string(), first_word, end_idx))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn try_parse_req_ref(
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
            // Might be [verb req.id]
            let verbs = ["impl", "verify", "define", "depends", "related"];
            if verbs.contains(&first_word.as_str()) {
                chars.next(); // consume space

                // Read the requirement ID
                let mut req_id = String::new();
                let mut has_dot = false;

                // First char must be lowercase
                if let Some(&(_, c)) = chars.peek() {
                    if c.is_ascii_lowercase() {
                        req_id.push(c);
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
                        req_id.push(c);
                        chars.next();
                    } else if c == '.' {
                        has_dot = true;
                        req_id.push(c);
                        chars.next();
                    } else {
                        return None;
                    }
                }

                if has_dot && !req_id.ends_with('.') && !req_id.is_empty() {
                    return Some(req_id);
                }
            }
            None
        }
        Some(']') => {
            chars.next(); // consume ]
            // [req.id] format - must contain dot
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
    fn test_extract_refs_doc_comment() {
        let source = r#"
/// Implements r[channel.id.parity] and r[channel.id.no-reuse]
fn next_channel_id() {}
"#;
        let refs = extract_refs(Path::new("test.rs"), source);
        assert_eq!(refs.len(), 2, "Expected 2 refs, got {:?}", refs);
        assert_eq!(refs[0].req_id, "channel.id.parity");
        assert_eq!(refs[1].req_id, "channel.id.no-reuse");
    }

    #[test]
    fn test_extract_refs_line_comment() {
        let source = r#"
// r[impl foo.bar]
fn do_thing() {}
"#;
        let refs = extract_refs(Path::new("test.rs"), source);
        assert_eq!(refs.len(), 1, "Expected 1 ref, got {:?}", refs);
        assert_eq!(refs[0].req_id, "foo.bar");
        assert_eq!(refs[0].verb, "impl");
    }

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
// r[impl foo.bar]
fn do_thing() {}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        assert_eq!(units.len(), 1);
        assert_eq!(units.units[0].req_refs, vec!["foo.bar"]);
    }

    #[test]
    fn test_extract_with_verb_ref() {
        let source = r#"
// r[verify channel.id.parity]
#[test]
fn test_parity() {}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        assert_eq!(units.len(), 1);
        assert_eq!(units.units[0].req_refs, vec!["channel.id.parity"]);
    }

    #[test]
    fn test_coverage_calculation() {
        let source = r#"
// r[impl foo.bar]
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
    fn test_find_req_refs() {
        assert_eq!(find_req_refs("// r[impl foo.bar]"), vec!["foo.bar"]);
        assert_eq!(find_req_refs("// [foo.bar]"), vec!["foo.bar"]);
        assert_eq!(
            find_req_refs("// r[impl a.b] and r[verify c.d]"),
            vec!["a.b", "c.d"]
        );
        assert!(find_req_refs("// no refs here").is_empty());
        assert!(find_req_refs("// [invalid]").is_empty()); // no dot
    }

    #[test]
    fn test_multiple_refs_same_unit() {
        let source = r#"
// r[impl req.one]
// r[verify req.two]
fn multi_ref() {}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        assert_eq!(units.len(), 1);
        // Should capture both refs
        assert!(units.units[0].req_refs.contains(&"req.one".to_string()));
        assert!(units.units[0].req_refs.contains(&"req.two".to_string()));
    }

    #[test]
    fn test_doc_comment_refs() {
        let source = r#"
/// Documentation for the function
/// r[impl doc.ref]
fn documented() {}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        assert_eq!(units.len(), 1);
        assert_eq!(units.units[0].req_refs, vec!["doc.ref"]);
    }

    #[test]
    fn test_impl_block() {
        let source = r#"
// r[impl my.impl]
impl Foo {
    fn method(&self) {}
}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        // Should find both the impl and the method
        let impl_unit = units.units.iter().find(|u| u.kind == CodeUnitKind::Impl);
        assert!(impl_unit.is_some());
        assert_eq!(impl_unit.unwrap().req_refs, vec!["my.impl"]);
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

    #[test]
    fn test_consecutive_functions_have_separate_boundaries() {
        // This mirrors the pattern from reqs.rs where multiple test functions
        // each have their own verify comment.
        // The start_line should include preceding comments/attributes.
        let source = r#"// r[verify first.test]
#[tokio::test]
async fn test_first() {
    let x = 1;
    assert!(x == 1);
}

// r[verify second.test]
#[tokio::test]
async fn test_second() {
    let y = 2;
    assert!(y == 2);
}

#[tokio::test]
async fn test_third() {
    let z = 3;
    assert!(z == 3);
}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        assert_eq!(units.len(), 3, "Should find 3 functions");

        // First function: starts at line 1 (comment), ends at line 6
        let first = &units.units[0];
        assert_eq!(first.name.as_deref(), Some("test_first"));
        assert_eq!(
            first.start_line, 1,
            "test_first should start at line 1 (comment)"
        );
        assert_eq!(first.end_line, 6, "test_first should end at line 6");
        assert_eq!(first.req_refs, vec!["first.test"]);

        // Second function: starts at line 8 (comment), ends at line 13
        let second = &units.units[1];
        assert_eq!(second.name.as_deref(), Some("test_second"));
        assert_eq!(
            second.start_line, 8,
            "test_second should start at line 8 (comment)"
        );
        assert_eq!(second.end_line, 13, "test_second should end at line 13");
        assert_eq!(second.req_refs, vec!["second.test"]);

        // Third function: starts at line 15 (attribute, no comment), ends at line 19
        let third = &units.units[2];
        assert_eq!(third.name.as_deref(), Some("test_third"));
        assert_eq!(
            third.start_line, 15,
            "test_third should start at line 15 (attribute)"
        );
        assert_eq!(third.end_line, 19, "test_third should end at line 19");
        assert!(third.req_refs.is_empty(), "test_third has no refs");
    }

    #[test]
    fn test_multiline_function_boundaries() {
        let source = r#"fn short() {}

fn longer() {
    let a = 1;
    let b = 2;
    let c = 3;
    println!("{}", a + b + c);
}

fn another_short() {}
"#;
        let units = extract_rust(Path::new("test.rs"), source);
        assert_eq!(units.len(), 3);

        assert_eq!(units.units[0].name.as_deref(), Some("short"));
        assert_eq!(units.units[0].start_line, 1);
        assert_eq!(units.units[0].end_line, 1);

        assert_eq!(units.units[1].name.as_deref(), Some("longer"));
        assert_eq!(units.units[1].start_line, 3);
        assert_eq!(units.units[1].end_line, 8);

        assert_eq!(units.units[2].name.as_deref(), Some("another_short"));
        assert_eq!(units.units[2].start_line, 10);
        assert_eq!(units.units[2].end_line, 10);
    }

    // r[verify code-unit.definition]
    // r[verify code-unit.boundary.include-comments]
    #[test]
    fn test_swift_code_units() {
        let source = r#"// r[impl swift.feature]
func doSomething() {
    print("hello")
}

// r[verify swift.test]
class MyClass {
    func method() {}
}

struct MyStruct {
    var x: Int
}

enum MyEnum {
    case a
    case b
}

protocol MyProtocol {
    func required()
}
"#;
        let units = extract_swift(Path::new("test.swift"), source);

        // Function
        let func_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("doSomething"));
        assert!(func_unit.is_some(), "Should find doSomething function");
        let func_unit = func_unit.unwrap();
        assert_eq!(func_unit.kind, CodeUnitKind::Function);
        assert_eq!(func_unit.start_line, 1, "Should include comment");
        assert_eq!(func_unit.req_refs, vec!["swift.feature"]);

        // Class
        let class_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyClass"));
        assert!(class_unit.is_some(), "Should find MyClass");
        assert_eq!(class_unit.unwrap().start_line, 6, "Should include comment");

        // Struct
        let struct_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyStruct"));
        assert!(struct_unit.is_some(), "Should find MyStruct");

        // Enum
        let enum_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyEnum"));
        assert!(enum_unit.is_some(), "Should find MyEnum");

        // Protocol
        let proto_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyProtocol"));
        assert!(proto_unit.is_some(), "Should find MyProtocol");
    }

    // r[verify code-unit.definition]
    // r[verify code-unit.boundary.include-comments]
    #[test]
    fn test_go_code_units() {
        let source = r#"package main

// r[impl go.feature]
func doSomething() {
    fmt.Println("hello")
}

// r[verify go.test]
func (s *Server) Handle() {
    // method
}

type MyStruct struct {
    x int
}
"#;
        let units = extract_go(Path::new("test.go"), source);

        // Function
        let func_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("doSomething"));
        assert!(func_unit.is_some(), "Should find doSomething function");
        let func_unit = func_unit.unwrap();
        assert_eq!(func_unit.kind, CodeUnitKind::Function);
        assert_eq!(func_unit.start_line, 3, "Should include comment");
        assert_eq!(func_unit.req_refs, vec!["go.feature"]);

        // Method
        let method_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("Handle"));
        assert!(method_unit.is_some(), "Should find Handle method");
        assert_eq!(method_unit.unwrap().start_line, 8, "Should include comment");

        // Type declaration (struct) - Go's type_declaration wraps type_spec
        // The name might not be directly accessible in the same way as other languages
        let type_units: Vec<_> = units
            .units
            .iter()
            .filter(|u| u.kind == CodeUnitKind::Struct)
            .collect();
        assert!(
            !type_units.is_empty(),
            "Should find at least one type declaration"
        );
    }

    // r[verify code-unit.definition]
    // r[verify code-unit.boundary.include-comments]
    #[test]
    fn test_java_code_units() {
        let source = r#"// r[impl java.feature]
public class MyClass {
    // r[impl java.method]
    public void doSomething() {
        System.out.println("hello");
    }

    public MyClass() {
        // constructor
    }
}

interface MyInterface {
    void required();
}

enum MyEnum {
    A, B, C
}
"#;
        let units = extract_java(Path::new("Test.java"), source);

        // Class
        let class_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyClass"));
        assert!(class_unit.is_some(), "Should find MyClass");
        let class_unit = class_unit.unwrap();
        assert_eq!(class_unit.kind, CodeUnitKind::Struct);
        assert_eq!(class_unit.start_line, 1, "Should include comment");
        assert_eq!(class_unit.req_refs, vec!["java.feature"]);

        // Method
        let method_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("doSomething"));
        assert!(method_unit.is_some(), "Should find doSomething method");
        assert_eq!(method_unit.unwrap().start_line, 3, "Should include comment");

        // Interface
        let iface_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyInterface"));
        assert!(iface_unit.is_some(), "Should find MyInterface");

        // Enum
        let enum_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyEnum"));
        assert!(enum_unit.is_some(), "Should find MyEnum");
    }

    // r[verify code-unit.definition]
    // r[verify code-unit.boundary.include-comments]
    #[test]
    fn test_python_code_units() {
        let source = r#"# r[impl python.feature]
def do_something():
    print("hello")

# r[verify python.test]
class MyClass:
    def method(self):
        pass
"#;
        let units = extract_python(Path::new("test.py"), source);

        // Function
        let func_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("do_something"));
        assert!(func_unit.is_some(), "Should find do_something function");
        let func_unit = func_unit.unwrap();
        assert_eq!(func_unit.kind, CodeUnitKind::Function);
        assert_eq!(func_unit.start_line, 1, "Should include comment");
        assert_eq!(func_unit.req_refs, vec!["python.feature"]);

        // Class
        let class_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyClass"));
        assert!(class_unit.is_some(), "Should find MyClass");
        assert_eq!(class_unit.unwrap().start_line, 5, "Should include comment");

        // Method inside class
        let method_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("method"));
        assert!(method_unit.is_some(), "Should find method inside class");
    }

    // r[verify code-unit.definition]
    // r[verify code-unit.boundary.include-comments]
    #[test]
    fn test_typescript_code_units() {
        let source = r#"// r[impl ts.feature]
function doSomething(): void {
    console.log("hello");
}

// r[verify ts.test]
class MyClass {
    method(): void {}
}

interface MyInterface {
    required(): void;
}

type MyType = string | number;

enum MyEnum {
    A,
    B,
}
"#;
        let units = extract_typescript(Path::new("test.ts"), source);

        // Function
        let func_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("doSomething"));
        assert!(func_unit.is_some(), "Should find doSomething function");
        let func_unit = func_unit.unwrap();
        assert_eq!(func_unit.kind, CodeUnitKind::Function);
        assert_eq!(func_unit.start_line, 1, "Should include comment");
        assert_eq!(func_unit.req_refs, vec!["ts.feature"]);

        // Class
        let class_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyClass"));
        assert!(class_unit.is_some(), "Should find MyClass");
        assert_eq!(class_unit.unwrap().start_line, 6, "Should include comment");

        // Interface
        let iface_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyInterface"));
        assert!(iface_unit.is_some(), "Should find MyInterface");

        // Type alias
        let type_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyType"));
        assert!(type_unit.is_some(), "Should find MyType");

        // Enum
        let enum_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyEnum"));
        assert!(enum_unit.is_some(), "Should find MyEnum");
    }
}
