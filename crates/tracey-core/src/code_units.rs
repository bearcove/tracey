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

use crate::positions::{ByteOffset, LineNumber, RefLocation};
use crate::{RuleId, parse_rule_id};
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
    pub req_refs: Vec<RuleId>,
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
        "c" | "h" => extract_c(path, source),
        "cpp" | "cc" | "cxx" | "hpp" => extract_cpp(path, source),
        "rb" => extract_ruby(path, source),
        "r" | "R" => extract_r(path, source),
        "dart" => extract_dart(path, source),
        "lua" => extract_lua(path, source),
        "asm" | "s" | "S" => extract_asm(path, source),
        "pl" | "pm" => extract_perl(path, source),
        "hs" | "lhs" => extract_haskell(path, source),
        "ex" | "exs" => extract_elixir(path, source),
        "erl" | "hrl" => extract_erlang(path, source),
        "clj" | "cljs" | "cljc" | "edn" => extract_clojure(path, source),
        "fs" | "fsi" | "fsx" => extract_fsharp(path, source),
        "vb" | "vbs" => extract_vb(path, source),
        "cob" | "cbl" | "cpy" => extract_cobol(path, source),
        "jl" => extract_julia(path, source),
        "d" => extract_d(path, source),
        "ps1" | "psm1" | "psd1" => extract_powershell(path, source),
        "cmake" => extract_cmake(path, source),
        "ml" | "mli" => extract_ocaml(path, source),
        "sh" | "bash" | "zsh" => extract_bash(path, source),
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

fn c_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_definition" => Some(CodeUnitKind::Function),
        "struct_specifier" => Some(CodeUnitKind::Struct),
        "enum_specifier" => Some(CodeUnitKind::Enum),
        "union_specifier" => Some(CodeUnitKind::Struct),
        _ => None,
    }
}

fn cpp_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_definition" => Some(CodeUnitKind::Function),
        "struct_specifier" => Some(CodeUnitKind::Struct),
        "class_specifier" => Some(CodeUnitKind::Struct),
        "enum_specifier" => Some(CodeUnitKind::Enum),
        "union_specifier" => Some(CodeUnitKind::Struct),
        "namespace_definition" => Some(CodeUnitKind::Module),
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

/// Extract code units from C source code
pub fn extract_c(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_c::language().into())
        .expect("Failed to load C grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, c_node_kind);
    units
}

/// Extract code units from C++ source code
pub fn extract_cpp(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_cpp::language().into())
        .expect("Failed to load C++ grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, cpp_node_kind);
    units
}

/// Extract code units from Ruby source code
pub fn extract_ruby(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_ruby::language().into())
        .expect("Failed to load Ruby grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, ruby_node_kind);
    units
}

fn ruby_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "method" | "singleton_method" => Some(CodeUnitKind::Function),
        "class" => Some(CodeUnitKind::Struct),
        "module" => Some(CodeUnitKind::Module),
        _ => None,
    }
}

/// Extract code units from R source code
pub fn extract_r(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_r::language().into())
        .expect("Failed to load R grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, r_node_kind);
    units
}

fn r_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_definition" => Some(CodeUnitKind::Function),
        _ => None,
    }
}

/// Extract code units from Dart source code
pub fn extract_dart(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_dart::language().into())
        .expect("Failed to load Dart grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, dart_node_kind);
    units
}

fn dart_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_signature" | "method_signature" => Some(CodeUnitKind::Function),
        "class_definition" => Some(CodeUnitKind::Struct),
        "enum_declaration" => Some(CodeUnitKind::Enum),
        "mixin_declaration" => Some(CodeUnitKind::Trait),
        "extension_declaration" => Some(CodeUnitKind::Impl),
        _ => None,
    }
}

/// Extract code units from Lua source code
pub fn extract_lua(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_lua::language().into())
        .expect("Failed to load Lua grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, lua_node_kind);
    units
}

fn lua_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_declaration" | "function_definition" => Some(CodeUnitKind::Function),
        _ => None,
    }
}

/// Extract code units from assembly source code
pub fn extract_asm(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_asm::language().into())
        .expect("Failed to load ASM grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, asm_node_kind);
    units
}

fn asm_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "label" => Some(CodeUnitKind::Function),
        "meta" => Some(CodeUnitKind::Macro),
        _ => None,
    }
}

/// Extract code units from MATLAB source code
pub fn extract_matlab(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_matlab::language().into())
        .expect("Failed to load MATLAB grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, matlab_node_kind);
    units
}

fn matlab_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_definition" => Some(CodeUnitKind::Function),
        "class_definition" => Some(CodeUnitKind::Struct),
        _ => None,
    }
}

/// Extract code units from Perl source code
pub fn extract_perl(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_perl::language().into())
        .expect("Failed to load Perl grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, perl_node_kind);
    units
}

fn perl_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function"
        | "method"
        | "method_declaration_statement"
        | "subroutine_declaration_statement" => Some(CodeUnitKind::Function),
        "package_statement" => Some(CodeUnitKind::Module),
        "class_statement" => Some(CodeUnitKind::Struct),
        "role_statement" => Some(CodeUnitKind::Trait),
        _ => None,
    }
}

/// Extract code units from Haskell source code
pub fn extract_haskell(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_haskell::language().into())
        .expect("Failed to load Haskell grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, haskell_node_kind);
    units
}

fn haskell_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function" | "bind" => Some(CodeUnitKind::Function),
        "data_type" | "newtype" => Some(CodeUnitKind::Struct),
        "class_decl" => Some(CodeUnitKind::Trait),
        "instance_decl" => Some(CodeUnitKind::Impl),
        "type_synomym" => Some(CodeUnitKind::TypeAlias),
        "module" => Some(CodeUnitKind::Module),
        _ => None,
    }
}

/// Extract code units from Elixir source code
///
/// Elixir uses `call` nodes with specific targets (def, defp, defmodule, etc.)
/// rather than dedicated node types for definitions.
pub fn extract_elixir(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_elixir::language().into())
        .expect("Failed to load Elixir grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_elixir_recursive(path, source, root, &mut units);
    units
}

fn elixir_call_kind(source: &str, node: Node) -> Option<CodeUnitKind> {
    if node.kind() != "call" {
        return None;
    }
    let target = node.child_by_field_name("target")?;
    let target_text = &source[target.byte_range()];
    match target_text {
        "def" | "defp" => Some(CodeUnitKind::Function),
        "defmodule" => Some(CodeUnitKind::Module),
        "defstruct" => Some(CodeUnitKind::Struct),
        "defprotocol" => Some(CodeUnitKind::Trait),
        "defimpl" => Some(CodeUnitKind::Impl),
        "defmacro" | "defmacrop" => Some(CodeUnitKind::Macro),
        _ => None,
    }
}

fn extract_elixir_recursive(path: &Path, source: &str, node: Node, units: &mut CodeUnits) {
    if let Some(kind) = elixir_call_kind(source, node) {
        let name = get_node_name(source, node).or_else(|| {
            // For Elixir calls, the name is the first argument
            node.child_by_field_name("arguments")
                .and_then(|args| {
                    let mut cursor = args.walk();
                    args.children(&mut cursor).next()
                })
                .map(|n| source[n.byte_range()].to_string())
        });

        let (req_refs, comment_start) = extract_req_refs_from_comments(source, node);
        let start_line = comment_start.unwrap_or_else(|| node.start_position().row + 1);
        let start_byte = if comment_start.is_some() {
            find_line_start_byte(source, start_line)
        } else {
            node.start_byte()
        };

        units.units.push(CodeUnit {
            kind,
            name,
            file: path.to_path_buf(),
            start_line,
            end_line: node.end_position().row + 1,
            start_byte,
            end_byte: node.end_byte(),
            req_refs,
        });
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_elixir_recursive(path, source, child, units);
    }
}

/// Extract code units from Erlang source code
pub fn extract_erlang(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_erlang::language().into())
        .expect("Failed to load Erlang grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, erlang_node_kind);
    units
}

fn erlang_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_clause" => Some(CodeUnitKind::Function),
        _ => None,
    }
}

/// Extract code units from Clojure source code
///
/// Clojure uses `list_lit` nodes where the first symbol determines the form type
/// (defn, def, defmacro, ns, defprotocol, defrecord, deftype).
pub fn extract_clojure(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_clojure::language().into())
        .expect("Failed to load Clojure grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_clojure_recursive(path, source, root, &mut units);
    units
}

fn clojure_list_kind(source: &str, node: Node) -> Option<CodeUnitKind> {
    if node.kind() != "list_lit" {
        return None;
    }
    // Find first sym_lit child
    let mut cursor = node.walk();
    let first_sym = node
        .children(&mut cursor)
        .find(|c| c.kind() == "sym_lit" || c.kind() == "sym_name")?;
    let sym_text = &source[first_sym.byte_range()];
    match sym_text {
        "defn" | "defn-" => Some(CodeUnitKind::Function),
        "def" | "defonce" => Some(CodeUnitKind::Const),
        "defmacro" => Some(CodeUnitKind::Macro),
        "ns" => Some(CodeUnitKind::Module),
        "defprotocol" => Some(CodeUnitKind::Trait),
        "defrecord" | "deftype" => Some(CodeUnitKind::Struct),
        _ => None,
    }
}

fn extract_clojure_recursive(path: &Path, source: &str, node: Node, units: &mut CodeUnits) {
    if let Some(kind) = clojure_list_kind(source, node) {
        // The second symbol child is the name
        let mut cursor = node.walk();
        let name = node
            .children(&mut cursor)
            .filter(|c| c.kind() == "sym_lit" || c.kind() == "sym_name")
            .nth(1)
            .map(|n| source[n.byte_range()].to_string());

        let (req_refs, comment_start) = extract_req_refs_from_comments(source, node);
        let start_line = comment_start.unwrap_or_else(|| node.start_position().row + 1);
        let start_byte = if comment_start.is_some() {
            find_line_start_byte(source, start_line)
        } else {
            node.start_byte()
        };

        units.units.push(CodeUnit {
            kind,
            name,
            file: path.to_path_buf(),
            start_line,
            end_line: node.end_position().row + 1,
            start_byte,
            end_byte: node.end_byte(),
            req_refs,
        });
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_clojure_recursive(path, source, child, units);
    }
}

/// Extract code units from F# source code
pub fn extract_fsharp(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_fsharp::language().into())
        .expect("Failed to load F# grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, fsharp_node_kind);
    units
}

fn fsharp_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_or_value_defn" => Some(CodeUnitKind::Function),
        "type_definition" => Some(CodeUnitKind::Struct),
        "module_defn" => Some(CodeUnitKind::Module),
        _ => None,
    }
}

/// Extract code units from Visual Basic source code
pub fn extract_vb(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_vb::language().into())
        .expect("Failed to load Visual Basic grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, vb_node_kind);
    units
}

fn vb_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "sub_block" | "function_block" => Some(CodeUnitKind::Function),
        "class_block" => Some(CodeUnitKind::Struct),
        "module_block" => Some(CodeUnitKind::Module),
        "enum_block" => Some(CodeUnitKind::Enum),
        _ => None,
    }
}

/// Extract code units from COBOL source code
pub fn extract_cobol(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_cobol::language().into())
        .expect("Failed to load COBOL grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, cobol_node_kind);
    units
}

fn cobol_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "paragraph_header" => Some(CodeUnitKind::Function),
        "section_header" => Some(CodeUnitKind::Module),
        _ => None,
    }
}

/// Extract code units from Julia source code
pub fn extract_julia(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_julia::language().into())
        .expect("Failed to load Julia grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, julia_node_kind);
    units
}

fn julia_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_definition" => Some(CodeUnitKind::Function),
        "macro_definition" => Some(CodeUnitKind::Macro),
        "struct_definition" => Some(CodeUnitKind::Struct),
        "module_definition" => Some(CodeUnitKind::Module),
        "abstract_definition" => Some(CodeUnitKind::Trait),
        "const_statement" => Some(CodeUnitKind::Const),
        _ => None,
    }
}

/// Extract code units from D source code
pub fn extract_d(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_d::language().into())
        .expect("Failed to load D grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, d_node_kind);
    units
}

fn d_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_declaration" => Some(CodeUnitKind::Function),
        "class_declaration" | "struct_declaration" => Some(CodeUnitKind::Struct),
        "enum_declaration" => Some(CodeUnitKind::Enum),
        "module_declaration" => Some(CodeUnitKind::Module),
        _ => None,
    }
}

/// Extract code units from PowerShell source code
pub fn extract_powershell(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_powershell::language().into())
        .expect("Failed to load PowerShell grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, powershell_node_kind);
    units
}

fn powershell_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_statement" | "class_method_definition" => Some(CodeUnitKind::Function),
        "class_statement" => Some(CodeUnitKind::Struct),
        "enum_statement" => Some(CodeUnitKind::Enum),
        _ => None,
    }
}

/// Extract code units from CMake source code
pub fn extract_cmake(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_cmake::language().into())
        .expect("Failed to load CMake grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, cmake_node_kind);
    units
}

fn cmake_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_def" => Some(CodeUnitKind::Function),
        "macro_def" => Some(CodeUnitKind::Macro),
        _ => None,
    }
}

/// Extract code units from OCaml source code
pub fn extract_ocaml(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_ocaml::language().into())
        .expect("Failed to load OCaml grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, ocaml_node_kind);
    units
}

fn ocaml_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "value_definition" | "let_binding" => Some(CodeUnitKind::Function),
        "type_definition" => Some(CodeUnitKind::Struct),
        "module_definition" => Some(CodeUnitKind::Module),
        _ => None,
    }
}

/// Extract code units from Bash source code
pub fn extract_bash(path: &Path, source: &str) -> CodeUnits {
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_bash::language().into())
        .expect("Failed to load Bash grammar");

    let Some(tree) = parser.parse(source, None) else {
        return CodeUnits::new();
    };

    let mut units = CodeUnits::new();
    let root = tree.root_node();
    extract_units_recursive(path, source, root, &mut units, bash_node_kind);
    units
}

fn bash_node_kind(kind: &str) -> Option<CodeUnitKind> {
    match kind {
        "function_definition" => Some(CodeUnitKind::Function),
        _ => None,
    }
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

/// Recursively unwrap C/C++ declarator chains to find the identifier node.
///
/// In tree-sitter-c/cpp, function names are nested inside declarator chains:
/// `function_definition` -> `function_declarator` -> `identifier`
/// This also handles pointer declarators, parenthesized declarators, etc.
fn find_declarator_name(node: Node) -> Option<Node> {
    match node.kind() {
        "identifier" | "field_identifier" | "qualified_identifier" | "type_identifier" => {
            Some(node)
        }
        "function_declarator"
        | "pointer_declarator"
        | "parenthesized_declarator"
        | "reference_declarator" => node
            .child_by_field_name("declarator")
            .and_then(find_declarator_name),
        _ => None,
    }
}

fn get_node_name(source: &str, node: Node) -> Option<String> {
    // Try common field names used across languages for the identifier/name
    // Most tree-sitter grammars use "name" for the identifier field
    let name_node = node
        .child_by_field_name("name")
        // C/C++: name is inside a declarator chain (must check before "type" fallback,
        // since C function_definition also has a "type" field for the return type)
        .or_else(|| {
            node.child_by_field_name("declarator")
                .and_then(find_declarator_name)
        })
        .or_else(|| node.child_by_field_name("type")) // For Rust impl blocks
        .or_else(|| {
            // For some languages, the first identifier child is the name
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|c| c.kind() == "identifier" || c.kind() == "type_identifier")
        })
        .or_else(|| {
            // Julia/similar: name is inside a signature or type_head child
            let wrapper = node.child_by_field_name("signature").or_else(|| {
                let mut cursor = node.walk();
                node.children(&mut cursor)
                    .find(|c| c.kind() == "signature" || c.kind() == "type_head")
            })?;
            // The wrapper may directly contain an identifier, or contain a
            // call_expression whose first identifier child is the name
            let mut cursor = wrapper.walk();
            wrapper
                .children(&mut cursor)
                .find(|c| c.kind() == "identifier")
                .or_else(|| {
                    let mut cursor2 = wrapper.walk();
                    let call = wrapper
                        .children(&mut cursor2)
                        .find(|c| c.kind() == "call_expression")?;
                    let mut cursor3 = call.walk();
                    call.children(&mut cursor3)
                        .find(|c| c.kind() == "identifier")
                })
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
fn extract_req_refs_from_comments(source: &str, node: Node) -> (Vec<RuleId>, Option<usize>) {
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
                    | "bracket_comment"        // CMake
                    | "documentation_comment" // Dart
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
fn collect_inner_comment_refs(source: &str, node: Node, refs: &mut Vec<RuleId>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "line_comment"
            | "block_comment"
            | "comment"
            | "multiline_comment"
            | "bracket_comment"
            | "documentation_comment" => {
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

fn collect_comment_refs(source: &str, node: Node, refs: &mut Vec<RuleId>) {
    match node.kind() {
        "line_comment"
        | "block_comment"
        | "comment"
        | "multiline_comment"
        | "bracket_comment"
        | "documentation_comment" => {
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

fn extract_refs_from_comment_text(source: &str, node: Node, refs: &mut Vec<RuleId>) {
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
fn find_req_refs(text: &str) -> Vec<RuleId> {
    let mut refs = Vec::new();
    let code_mask = crate::markdown::markdown_code_mask(text);
    let mut chars = text.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        if crate::markdown::is_code_index(idx, &code_mask) {
            continue;
        }
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
    pub req_id: RuleId,
    /// Line number (1-indexed)
    pub line: usize,
    /// Byte offset of the reference start
    pub byte_offset: usize,
    /// Byte length of the reference
    pub byte_length: usize,
}

impl RefLocation {
    fn into_full_ref(self, prefix: String, verb: String, req_id: RuleId) -> FullReqRef {
        FullReqRef {
            prefix,
            verb,
            req_id,
            line: self.line().as_usize(),
            byte_offset: self.span().offset().as_usize(),
            byte_length: self.span().length().as_usize(),
        }
    }

    fn into_warning(self) -> FullReqRefWarning {
        FullReqRefWarning {
            line: self.line().as_usize(),
            byte_offset: self.span().offset().as_usize(),
            byte_length: self.span().length().as_usize(),
        }
    }
}

/// Warning emitted while parsing references from comments.
#[derive(Debug, Clone)]
pub struct FullReqRefWarning {
    /// Line number (1-indexed)
    pub line: usize,
    /// Byte offset of the malformed reference start
    pub byte_offset: usize,
    /// Byte length of the malformed reference
    pub byte_length: usize,
}

/// Extracted references plus parser warnings.
#[derive(Debug, Clone, Default)]
pub struct ExtractedRefs {
    pub references: Vec<FullReqRef>,
    pub warnings: Vec<FullReqRefWarning>,
}

/// Extract ALL requirement references from a file using tree-sitter
///
/// r[impl ref.parser.tree-sitter]
/// r[impl ref.parser.languages]
/// r[impl ref.parser.unified]
pub fn extract_refs(path: &Path, source: &str) -> Vec<FullReqRef> {
    extract_refs_with_warnings(path, source).references
}

/// Extract all requirement references and malformed-reference warnings.
pub fn extract_refs_with_warnings(path: &Path, source: &str) -> ExtractedRefs {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let language = match ext {
        "rs" => arborium_rust::language(),
        "swift" => arborium_swift::language(),
        "go" => arborium_go::language(),
        "java" => arborium_java::language(),
        "py" => arborium_python::language(),
        "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" => arborium_typescript::language(),
        "php" => arborium_php::language(),
        "c" | "h" => arborium_c::language(),
        "cpp" | "cc" | "cxx" | "hpp" => arborium_cpp::language(),
        "rb" => arborium_ruby::language(),
        "r" | "R" => arborium_r::language(),
        "dart" => arborium_dart::language(),
        "lua" => arborium_lua::language(),
        "asm" | "s" | "S" => arborium_asm::language(),
        "pl" | "pm" => arborium_perl::language(),
        "hs" | "lhs" => arborium_haskell::language(),
        "ex" | "exs" => arborium_elixir::language(),
        "erl" | "hrl" => arborium_erlang::language(),
        "clj" | "cljs" | "cljc" | "edn" => arborium_clojure::language(),
        "fs" | "fsi" | "fsx" => arborium_fsharp::language(),
        "vb" | "vbs" => arborium_vb::language(),
        "cob" | "cbl" | "cpy" => arborium_cobol::language(),
        "jl" => arborium_julia::language(),
        "d" => arborium_d::language(),
        "ps1" | "psm1" | "psd1" => arborium_powershell::language(),
        "cmake" => arborium_cmake::language(),
        "ml" | "mli" => arborium_ocaml::language(),
        "sh" | "bash" | "zsh" => arborium_bash::language(),
        _ => return ExtractedRefs::default(),
    };

    let mut parser = Parser::new();
    parser
        .set_language(&language.into())
        .expect("Failed to load grammar");

    let Some(tree) = parser.parse(source, None) else {
        return ExtractedRefs::default();
    };

    // Pre-compute file-level code mask for doc-comment groups so that fenced
    // code blocks spanning multiple `///` / `//!` lines are properly masked.
    let file_code_mask = crate::markdown::compute_doc_comment_code_mask(source);

    let mut refs = Vec::new();
    let mut warnings = Vec::new();
    let mut ignore_state = IgnoreState::default();
    extract_refs_recursive(
        source,
        tree.root_node(),
        &mut refs,
        &mut warnings,
        &mut ignore_state,
        &file_code_mask,
    );
    ExtractedRefs {
        references: refs,
        warnings,
    }
}

/// State for tracking ignore directives across comment nodes.
///
/// r[impl ref.ignore.prefix]
#[derive(Default)]
struct IgnoreState {
    /// Skip the next line (set by @tracey:ignore-next-line)
    ignore_next_line: Option<LineNumber>,
    /// Currently inside an ignore block (set by @tracey:ignore-start)
    /// r[impl ref.ignore.block]
    in_ignore_block: bool,
}

/// Check if a comment contains ignore directives and update state accordingly.
///
/// Returns true if the current comment's refs should be extracted (not ignored).
fn check_ignore_directives(text: &str, line: LineNumber, state: &mut IgnoreState) -> bool {
    // Check for ignore directives
    // r[impl ref.ignore.next-line]
    if text.contains("@tracey:ignore-next-line") {
        state.ignore_next_line = Some(line);
        // Don't extract refs from directive comments themselves
        return false;
    }

    // r[impl ref.ignore.block]
    if text.contains("@tracey:ignore-start") {
        state.in_ignore_block = true;
        return false;
    }

    if text.contains("@tracey:ignore-end") {
        state.in_ignore_block = false;
        return false;
    }

    // Check if we're in an ignore block
    if state.in_ignore_block {
        return false;
    }

    // Check if previous line had ignore-next-line
    if let Some(ignore_line) = state.ignore_next_line {
        // Check if this comment is on the line immediately after the ignore directive
        if line.is_immediately_after(ignore_line) {
            state.ignore_next_line = None;
            return false;
        }
        // Clear the ignore if we've moved past it
        state.ignore_next_line = None;
    }

    true
}

fn extract_refs_recursive(
    source: &str,
    node: Node,
    refs: &mut Vec<FullReqRef>,
    warnings: &mut Vec<FullReqRefWarning>,
    ignore_state: &mut IgnoreState,
    file_code_mask: &[bool],
) {
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
            | "bracket_comment"
            | "documentation_comment"
            | "line_outer_doc_comment"
            | "line_inner_doc_comment"
            | "block_outer_doc_comment"
            | "block_inner_doc_comment"
    );

    if is_comment {
        let text = &source[node.byte_range()];
        let line = LineNumber::from_zero_based(node.start_position().row);
        let base_offset = ByteOffset::from_usize(node.start_byte());

        // Check ignore directives and determine if we should extract refs
        if check_ignore_directives(text, line, ignore_state) {
            extract_full_refs_from_text(text, line, base_offset, file_code_mask, refs, warnings);
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_refs_recursive(source, child, refs, warnings, ignore_state, file_code_mask);
    }
}

// r[impl ref.syntax.surrounding-text]
fn extract_full_refs_from_text(
    text: &str,
    line: LineNumber,
    base_offset: ByteOffset,
    file_code_mask: &[bool],
    refs: &mut Vec<FullReqRef>,
    warnings: &mut Vec<FullReqRefWarning>,
) {
    let code_mask = crate::markdown::markdown_code_mask(text);
    let mut chars = text.char_indices().peekable();
    let mut prev_ch: Option<char> = None;

    while let Some((start_idx, ch)) = chars.next() {
        // Check both per-text mask and file-level mask for doc-comment groups
        let file_idx = base_offset.as_usize() + start_idx;
        if crate::markdown::is_code_index(start_idx, &code_mask)
            || crate::markdown::is_code_index(file_idx, file_code_mask)
        {
            prev_ch = Some(ch);
            continue;
        }
        // Match prefix (lowercase alphanumeric) followed by '['
        // Only start a prefix scan when NOT preceded by a word character,
        // so that identifiers like `slot_count[i]` are not misinterpreted.
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            if prev_ch.is_some_and(|pc| pc.is_ascii_alphanumeric() || pc == '_') {
                prev_ch = Some(ch);
                continue;
            }
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
            match try_parse_full_ref(&mut chars) {
                Some(ParsedFullRef::Parsed {
                    verb,
                    req_id,
                    end_idx,
                }) => {
                    let location = RefLocation::from_relative_indices(
                        line,
                        base_offset,
                        prefix_start,
                        end_idx,
                    );
                    refs.push(location.into_full_ref(prefix, verb, req_id));
                }
                Some(ParsedFullRef::Malformed { end_idx }) => {
                    let location = RefLocation::from_relative_indices(
                        line,
                        base_offset,
                        prefix_start,
                        end_idx,
                    );
                    warnings.push(location.into_warning());
                }
                None => {}
            }
            prev_ch = Some(']');
        } else {
            prev_ch = Some(ch);
        }
    }
}

enum ParsedFullRef {
    Parsed {
        verb: String,
        req_id: RuleId,
        end_idx: usize,
    },
    Malformed {
        end_idx: usize,
    },
}

// r[impl ref.syntax.req-id]
fn try_parse_full_ref(
    chars: &mut std::iter::Peekable<impl Iterator<Item = (usize, char)>>,
) -> Option<ParsedFullRef> {
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
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.' || c == '+' {
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
                    } else if c.is_ascii_lowercase()
                        || c.is_ascii_digit()
                        || c == '-'
                        || c == '_'
                        || c == '+'
                        || c == '.'
                    {
                        req_id.push(c);
                        chars.next();
                    } else {
                        return None;
                    }
                }

                if is_valid_req_id(&req_id) {
                    return parse_rule_id(&req_id).map(|parsed| ParsedFullRef::Parsed {
                        verb,
                        req_id: parsed,
                        end_idx,
                    });
                }
                return Some(ParsedFullRef::Malformed { end_idx });
            }
            None
        }
        Some(']') => {
            chars.next(); // consume ]
            // [req.id] format - defaults to impl
            if is_valid_req_id(&first_word) {
                parse_rule_id(&first_word).map(|parsed| ParsedFullRef::Parsed {
                    verb: "impl".to_string(),
                    req_id: parsed,
                    end_idx,
                })
            } else {
                Some(ParsedFullRef::Malformed { end_idx })
            }
        }
        _ => None,
    }
}

fn try_parse_req_ref(
    chars: &mut std::iter::Peekable<impl Iterator<Item = (usize, char)>>,
) -> Option<RuleId> {
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
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.' || c == '+' {
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
                    } else if c.is_ascii_lowercase()
                        || c.is_ascii_digit()
                        || c == '-'
                        || c == '+'
                        || c == '.'
                    {
                        req_id.push(c);
                        chars.next();
                    } else {
                        return None;
                    }
                }

                if is_valid_req_id(&req_id) {
                    return parse_rule_id(&req_id);
                }
            }
            None
        }
        Some(']') => {
            chars.next(); // consume ]
            // [req.id] format
            if is_valid_req_id(&first_word) {
                parse_rule_id(&first_word)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_valid_req_id(req_id: &str) -> bool {
    let Some(parsed) = parse_rule_id(req_id) else {
        return false;
    };
    !parsed.base.ends_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_rule_id;

    fn rid(id: &str) -> RuleId {
        parse_rule_id(id).expect("valid rule id")
    }

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
    fn test_extract_refs_byte_span_uses_inclusive_end() {
        let source = "// r[foo.bar]\n";
        let refs = extract_refs(Path::new("test.rs"), source);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].byte_offset, 3);
        assert_eq!(refs[0].byte_length, "r[foo.bar]".len());
    }

    #[test]
    fn test_extract_warnings_byte_span_uses_inclusive_end() {
        let source = "// r[impl foo.]\n";
        let extracted = extract_refs_with_warnings(Path::new("test.rs"), source);
        assert!(extracted.references.is_empty());
        assert_eq!(extracted.warnings.len(), 1);
        assert_eq!(extracted.warnings[0].byte_offset, 3);
        assert_eq!(extracted.warnings[0].byte_length, "r[impl foo.]".len());
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
        assert_eq!(units.units[0].req_refs, vec![rid("foo.bar")]);
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
        assert_eq!(units.units[0].req_refs, vec![rid("channel.id.parity")]);
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
        assert_eq!(find_req_refs("// r[impl foo.bar]"), vec![rid("foo.bar")]);
        assert_eq!(find_req_refs("// [foo.bar]"), vec![rid("foo.bar")]);
        assert_eq!(
            find_req_refs("// r[impl a.b] and r[verify c.d]"),
            vec![rid("a.b"), rid("c.d")]
        );
        assert_eq!(
            find_req_refs("// r[impl auth.login+2] and r[verify auth.logout+3]"),
            vec![rid("auth.login+2"), rid("auth.logout+3")]
        );
        assert!(find_req_refs("// no refs here").is_empty());
        assert!(find_req_refs("// [invalid.]").is_empty()); // trailing dot
        assert!(find_req_refs("// r[impl auth.login+]").is_empty());
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
        assert!(units.units[0].req_refs.contains(&rid("req.one")));
        assert!(units.units[0].req_refs.contains(&rid("req.two")));
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
        assert_eq!(units.units[0].req_refs, vec![rid("doc.ref")]);
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
        assert_eq!(impl_unit.unwrap().req_refs, vec![rid("my.impl")]);
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
        assert_eq!(first.req_refs, vec![rid("first.test")]);

        // Second function: starts at line 8 (comment), ends at line 13
        let second = &units.units[1];
        assert_eq!(second.name.as_deref(), Some("test_second"));
        assert_eq!(
            second.start_line, 8,
            "test_second should start at line 8 (comment)"
        );
        assert_eq!(second.end_line, 13, "test_second should end at line 13");
        assert_eq!(second.req_refs, vec![rid("second.test")]);

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
        assert_eq!(func_unit.req_refs, vec![rid("swift.feature")]);

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
        assert_eq!(func_unit.req_refs, vec![rid("go.feature")]);

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
        assert_eq!(class_unit.req_refs, vec![rid("java.feature")]);

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
        assert_eq!(func_unit.req_refs, vec![rid("python.feature")]);

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
        assert_eq!(func_unit.req_refs, vec![rid("ts.feature")]);

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

    // =========================================================================
    // Ignore directive tests
    // =========================================================================

    #[test]
    fn test_ignore_next_line() {
        let source = r#"
// @tracey:ignore-next-line
// This comment mentions r[impl auth.login] but it should be ignored
fn example() {}
"#;
        let refs = extract_refs(Path::new("test.rs"), source);
        assert!(
            refs.is_empty(),
            "Expected no refs due to ignore-next-line, got {:?}",
            refs
        );
    }

    #[test]
    fn test_ignore_next_line_only_affects_next() {
        let source = r#"
// @tracey:ignore-next-line
// This r[impl ignored.ref] should be ignored
// But this r[impl visible.ref] should be extracted
fn example() {}
"#;
        let refs = extract_refs(Path::new("test.rs"), source);
        assert_eq!(refs.len(), 1, "Expected 1 ref, got {:?}", refs);
        assert_eq!(refs[0].req_id, "visible.ref");
    }

    #[test]
    fn test_ignore_block() {
        let source = r#"
// @tracey:ignore-start
// The fixtures have both r[impl auth.login] and o[impl api.fetch]
// These are just documentation, not actual references
// @tracey:ignore-end
fn test_validation() {}
"#;
        let refs = extract_refs(Path::new("test.rs"), source);
        assert!(
            refs.is_empty(),
            "Expected no refs due to ignore block, got {:?}",
            refs
        );
    }

    #[test]
    fn test_ignore_block_with_refs_after() {
        let source = r#"
// @tracey:ignore-start
// This r[impl ignored.one] is ignored
// This r[impl ignored.two] is also ignored
// @tracey:ignore-end
// But this r[impl visible.ref] should be extracted
fn example() {}
"#;
        let refs = extract_refs(Path::new("test.rs"), source);
        assert_eq!(refs.len(), 1, "Expected 1 ref, got {:?}", refs);
        assert_eq!(refs[0].req_id, "visible.ref");
    }

    #[test]
    fn test_ignore_block_with_refs_before() {
        let source = r#"
// This r[impl before.ref] should be extracted
// @tracey:ignore-start
// This r[impl ignored.ref] is ignored
// @tracey:ignore-end
fn example() {}
"#;
        let refs = extract_refs(Path::new("test.rs"), source);
        assert_eq!(refs.len(), 1, "Expected 1 ref, got {:?}", refs);
        assert_eq!(refs[0].req_id, "before.ref");
    }

    #[test]
    fn test_normal_refs_still_work() {
        let source = r#"
// r[impl normal.ref]
fn example() {}
"#;
        let refs = extract_refs(Path::new("test.rs"), source);
        assert_eq!(refs.len(), 1, "Expected 1 ref, got {:?}", refs);
        assert_eq!(refs[0].req_id, "normal.ref");
    }

    #[test]
    fn test_extract_single_segment_refs() {
        let source = r#"
// r[impl link]
// r[link]
fn example() {}
"#;
        let refs = extract_refs(Path::new("test.rs"), source);
        assert_eq!(refs.len(), 2, "Expected 2 refs, got {:?}", refs);
        assert_eq!(refs[0].req_id, "link");
        assert_eq!(refs[1].req_id, "link");
    }

    #[test]
    fn test_warn_on_malformed_refs() {
        let source = r#"
// r[impl link.]
// r[link.]
fn example() {}
"#;
        let extracted = extract_refs_with_warnings(Path::new("test.rs"), source);
        assert!(
            extracted.references.is_empty(),
            "Expected no refs, got {:?}",
            extracted.references
        );
        assert_eq!(extracted.warnings.len(), 2);
    }

    // =========================================================================
    // C language tests
    // =========================================================================

    #[test]
    fn test_c_code_units() {
        let source = r#"// r[impl c.feature]
void do_something(void) {
    printf("hello\n");
}

// r[verify c.test]
struct MyStruct {
    int x;
    int y;
};

enum Color {
    RED,
    GREEN,
    BLUE
};

union Data {
    int i;
    float f;
};
"#;
        let units = extract_c(Path::new("test.c"), source);

        // Function
        let func_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("do_something"));
        assert!(func_unit.is_some(), "Should find do_something function");
        let func_unit = func_unit.unwrap();
        assert_eq!(func_unit.kind, CodeUnitKind::Function);
        assert_eq!(func_unit.start_line, 1, "Should include comment");
        assert_eq!(func_unit.req_refs, vec!["c.feature"]);

        // Struct
        let struct_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyStruct"));
        assert!(struct_unit.is_some(), "Should find MyStruct");
        let struct_unit = struct_unit.unwrap();
        assert_eq!(struct_unit.kind, CodeUnitKind::Struct);

        // Enum
        let enum_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("Color"));
        assert!(enum_unit.is_some(), "Should find Color enum");
        assert_eq!(enum_unit.unwrap().kind, CodeUnitKind::Enum);

        // Union
        let union_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("Data"));
        assert!(union_unit.is_some(), "Should find Data union");
        assert_eq!(union_unit.unwrap().kind, CodeUnitKind::Struct);
    }

    #[test]
    fn test_c_extract_refs() {
        let source = r#"// r[impl buffer.alloc]
void* alloc_buffer(size_t size) {
    return malloc(size);
}
"#;
        let refs = extract_refs(Path::new("alloc.c"), source);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].req_id, "buffer.alloc");
        assert_eq!(refs[0].verb, "impl");
    }

    #[test]
    fn test_h_file_uses_c_grammar() {
        let source = r#"struct Point {
    int x;
    int y;
};

void process_point(struct Point* p) {}
"#;
        let units = extract(Path::new("point.h"), source);
        assert!(!units.is_empty(), "Should extract code units from .h file");

        let struct_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("Point"));
        assert!(struct_unit.is_some(), "Should find Point struct in .h file");
    }

    // =========================================================================
    // C++ language tests
    // =========================================================================

    #[test]
    fn test_cpp_code_units() {
        let source = r#"// r[impl cpp.feature]
void doSomething() {
    std::cout << "hello" << std::endl;
}

// r[verify cpp.test]
class MyClass {
public:
    void method() {}
};

struct MyStruct {
    int x;
};

enum MyEnum {
    A,
    B,
    C
};

namespace MyNamespace {
    void innerFunc() {}
}
"#;
        let units = extract_cpp(Path::new("test.cpp"), source);

        // Function
        let func_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("doSomething"));
        assert!(func_unit.is_some(), "Should find doSomething function");
        let func_unit = func_unit.unwrap();
        assert_eq!(func_unit.kind, CodeUnitKind::Function);
        assert_eq!(func_unit.start_line, 1, "Should include comment");
        assert_eq!(func_unit.req_refs, vec!["cpp.feature"]);

        // Class
        let class_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyClass"));
        assert!(class_unit.is_some(), "Should find MyClass");
        let class_unit = class_unit.unwrap();
        assert_eq!(class_unit.kind, CodeUnitKind::Struct);

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

        // Namespace
        let ns_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyNamespace"));
        assert!(ns_unit.is_some(), "Should find MyNamespace namespace");
        assert_eq!(ns_unit.unwrap().kind, CodeUnitKind::Module);

        // Inner function in namespace
        let inner_func = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("innerFunc"));
        assert!(
            inner_func.is_some(),
            "Should find innerFunc inside namespace"
        );
    }

    #[test]
    fn test_cpp_extract_refs() {
        let source = r#"// r[impl widget.render]
// r[depends ui.framework]
void render() {
    // rendering logic
}
"#;
        let refs = extract_refs(Path::new("widget.cpp"), source);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].req_id, "widget.render");
        assert_eq!(refs[1].req_id, "ui.framework");
    }

    #[test]
    fn test_hpp_file_uses_cpp_grammar() {
        let source = r#"class Widget {
public:
    void draw() {}
};
"#;
        let units = extract(Path::new("widget.hpp"), source);
        assert!(
            !units.is_empty(),
            "Should extract code units from .hpp file"
        );

        let class_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("Widget"));
        assert!(
            class_unit.is_some(),
            "Should find Widget class in .hpp file"
        );
        assert_eq!(class_unit.unwrap().kind, CodeUnitKind::Struct);
    }

    #[test]
    fn test_ruby_code_units() {
        let source = r#"# r[impl ruby.feature]
def do_something
  puts "hello"
end

# r[verify ruby.test]
class MyClass
  def method
  end
end

module MyModule
end
"#;
        let units = extract_ruby(Path::new("test.rb"), source);

        let func_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("do_something"));
        assert!(func_unit.is_some(), "Should find do_something");
        let func_unit = func_unit.unwrap();
        assert_eq!(func_unit.kind, CodeUnitKind::Function);
        assert_eq!(func_unit.req_refs, vec![rid("ruby.feature")]);

        let class_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyClass"));
        assert!(class_unit.is_some(), "Should find MyClass");
        assert_eq!(class_unit.unwrap().kind, CodeUnitKind::Struct);

        let module_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyModule"));
        assert!(module_unit.is_some(), "Should find MyModule");
        assert_eq!(module_unit.unwrap().kind, CodeUnitKind::Module);
    }

    #[test]
    fn test_r_code_units() {
        let source = r#"# r[impl r.feature]
do_something <- function(x) {
  print(x)
}
"#;
        let units = extract_r(Path::new("test.r"), source);
        assert!(!units.is_empty(), "Should extract code units from R source");
    }

    #[test]
    fn test_dart_code_units() {
        let source = r#"// r[impl dart.feature]
class MyClass {
  void doSomething() {}
}

enum Color { red, green, blue }
"#;
        let units = extract_dart(Path::new("test.dart"), source);
        assert!(
            !units.is_empty(),
            "Should extract code units from Dart source"
        );
    }

    #[test]
    fn test_lua_code_units() {
        let source = r#"-- r[impl lua.feature]
function do_something()
  print("hello")
end

local function helper()
end
"#;
        let units = extract_lua(Path::new("test.lua"), source);
        assert!(
            !units.is_empty(),
            "Should extract code units from Lua source"
        );
    }

    #[test]
    fn test_asm_code_units() {
        let source = r#"; r[impl asm.feature]
_start:
    mov eax, 1
    ret
"#;
        let units = extract_asm(Path::new("test.asm"), source);
        assert!(
            !units.is_empty(),
            "Should extract code units from ASM source"
        );
    }

    #[test]
    fn test_matlab_code_units() {
        let source = r#"% r[impl matlab.feature]
function result = do_something(x)
    result = x + 1;
end
"#;
        let units = extract_matlab(Path::new("test.mat"), source);
        assert!(
            !units.is_empty(),
            "Should extract code units from MATLAB source"
        );
    }

    #[test]
    fn test_perl_code_units() {
        let source = r#"# r[impl perl.feature]
sub do_something {
    print "hello\n";
}

package MyPackage;
"#;
        let units = extract_perl(Path::new("test.pl"), source);
        assert!(
            !units.is_empty(),
            "Should extract code units from Perl source"
        );
    }

    #[test]
    fn test_haskell_code_units() {
        let source = r#"-- r[impl haskell.feature]
module Main where

data Color = Red | Green | Blue

doSomething :: Int -> Int
doSomething x = x + 1
"#;
        let units = extract_haskell(Path::new("test.hs"), source);
        assert!(
            !units.is_empty(),
            "Should extract code units from Haskell source"
        );
    }

    #[test]
    fn test_elixir_code_units() {
        let source = r#"# r[impl elixir.feature]
defmodule MyModule do
  # r[verify elixir.test]
  def do_something do
    :ok
  end

  defp helper do
    :ok
  end
end
"#;
        let units = extract_elixir(Path::new("test.ex"), source);

        let module_unit = units.units.iter().find(|u| u.kind == CodeUnitKind::Module);
        assert!(module_unit.is_some(), "Should find defmodule");

        let func_units: Vec<_> = units
            .units
            .iter()
            .filter(|u| u.kind == CodeUnitKind::Function)
            .collect();
        assert!(!func_units.is_empty(), "Should find def/defp functions");
    }

    #[test]
    fn test_erlang_code_units() {
        let source = r#"% r[impl erlang.feature]
do_something(X) ->
    X + 1.
"#;
        let units = extract_erlang(Path::new("test.erl"), source);
        assert!(
            !units.is_empty(),
            "Should extract code units from Erlang source"
        );
    }

    #[test]
    fn test_clojure_code_units() {
        let source = r#"; r[impl clojure.feature]
(defn do-something [x]
  (+ x 1))

(def my-const 42)

(ns my-namespace)
"#;
        let units = extract_clojure(Path::new("test.clj"), source);

        let func_unit = units
            .units
            .iter()
            .find(|u| u.kind == CodeUnitKind::Function);
        assert!(func_unit.is_some(), "Should find defn");

        let const_unit = units.units.iter().find(|u| u.kind == CodeUnitKind::Const);
        assert!(const_unit.is_some(), "Should find def");

        let ns_unit = units.units.iter().find(|u| u.kind == CodeUnitKind::Module);
        assert!(ns_unit.is_some(), "Should find ns");
    }

    #[test]
    fn test_fsharp_code_units() {
        let source = r#"// r[impl fsharp.feature]
module MyModule =
    let doSomething x = x + 1

type MyType = { Name: string }
"#;
        let units = extract_fsharp(Path::new("test.fs"), source);
        assert!(
            !units.is_empty(),
            "Should extract code units from F# source"
        );
    }

    #[test]
    fn test_vb_code_units() {
        let source = r#"' r[impl vb.feature]
Module MyModule
    Sub DoSomething()
    End Sub

    Function GetValue() As Integer
        Return 42
    End Function
End Module

Class MyClass
End Class

Enum Color
    Red
    Green
End Enum
"#;
        let units = extract_vb(Path::new("test.vb"), source);
        assert!(
            !units.is_empty(),
            "Should extract code units from VB source"
        );
    }

    #[test]
    fn test_cobol_code_units() {
        let source = "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. HELLO.\n       PROCEDURE DIVISION.\n       MAIN-PARA.\n           DISPLAY \"Hello\".\n           STOP RUN.\n";
        let units = extract_cobol(Path::new("test.cob"), source);
        // COBOL parsing may or may not find paragraph_header depending on grammar;
        // at minimum, verify extraction doesn't panic
        let _ = units;
    }

    #[test]
    fn test_julia_code_units() {
        let source = r#"# r[impl julia.feature]
function do_something(x)
    x + 1
end

struct MyStruct
    field::Int
end

module MyModule
end

const MY_CONST = 42
"#;
        let units = extract_julia(Path::new("test.jl"), source);

        let func_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("do_something"));
        assert!(func_unit.is_some(), "Should find do_something");
        assert_eq!(func_unit.unwrap().kind, CodeUnitKind::Function);

        let struct_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyStruct"));
        assert!(struct_unit.is_some(), "Should find MyStruct");
        assert_eq!(struct_unit.unwrap().kind, CodeUnitKind::Struct);

        let module_unit = units
            .units
            .iter()
            .find(|u| u.name.as_deref() == Some("MyModule"));
        assert!(module_unit.is_some(), "Should find MyModule");
        assert_eq!(module_unit.unwrap().kind, CodeUnitKind::Module);
    }

    #[test]
    fn test_d_code_units() {
        let source = r#"// r[impl d.feature]
void doSomething() {
}

class MyClass {
}

struct MyStruct {
}

enum Color { red, green, blue }
"#;
        let units = extract_d(Path::new("test.d"), source);
        assert!(!units.is_empty(), "Should extract code units from D source");
    }

    #[test]
    fn test_powershell_code_units() {
        let source = r#"# r[impl powershell.feature]
function Do-Something {
    Write-Host "hello"
}

class MyClass {
    [void] DoMethod() {}
}

enum Color {
    Red
    Green
    Blue
}
"#;
        let units = extract_powershell(Path::new("test.ps1"), source);

        let func_unit = units
            .units
            .iter()
            .find(|u| u.kind == CodeUnitKind::Function);
        assert!(func_unit.is_some(), "Should find function");
    }

    #[test]
    fn test_cmake_code_units() {
        let source = r#"# r[impl cmake.feature]
function(do_something ARG)
    message(STATUS "hello")
endfunction()

macro(my_macro ARG)
    message(STATUS "macro")
endmacro()
"#;
        let units = extract_cmake(Path::new("test.cmake"), source);
        assert!(
            !units.is_empty(),
            "Should extract code units from CMake source"
        );
    }

    #[test]
    fn test_ocaml_code_units() {
        let source = r#"(* r[impl ocaml.feature] *)
let do_something x = x + 1

type color = Red | Green | Blue

module MyModule = struct
end
"#;
        let units = extract_ocaml(Path::new("test.ml"), source);
        assert!(
            !units.is_empty(),
            "Should extract code units from OCaml source"
        );
    }

    #[test]
    fn test_bash_code_units() {
        let source = r#"# r[impl bash.feature]
do_something() {
    echo "hello"
}

function helper {
    echo "helper"
}
"#;
        let units = extract_bash(Path::new("test.sh"), source);

        let func_units: Vec<_> = units
            .units
            .iter()
            .filter(|u| u.kind == CodeUnitKind::Function)
            .collect();
        assert!(
            !func_units.is_empty(),
            "Should find function definitions in bash"
        );
    }

    #[test]
    fn test_fenced_code_in_inner_doc_comments_treesitter() {
        let source = "//! ```text\n//! slot_count[i]\n//! ```\nfn main() {}\n";
        let refs = extract_refs_with_warnings(Path::new("test.rs"), source);
        assert_eq!(
            refs.references.len(),
            0,
            "fenced code in //! should produce no annotations"
        );
        assert_eq!(refs.warnings.len(), 0);
    }

    #[test]
    fn test_fenced_code_in_outer_doc_comments_treesitter() {
        let source = "/// ```rust\n/// // r[haha.hehe]\n/// ```\nfn foo() {}\n";
        let refs = extract_refs_with_warnings(Path::new("test.rs"), source);
        assert_eq!(
            refs.references.len(),
            0,
            "fenced code in /// should produce no annotations"
        );
        assert_eq!(refs.warnings.len(), 0);
    }

    #[test]
    fn test_identifier_before_bracket_not_annotation_treesitter() {
        let source = "/// slot_count[i] is fun\nfn foo() {}\n";
        let refs = extract_refs_with_warnings(Path::new("test.rs"), source);
        assert_eq!(
            refs.references.len(),
            0,
            "identifier[x] should not be an annotation"
        );
        assert_eq!(refs.warnings.len(), 0);
    }

    #[test]
    fn test_inline_backtick_in_doc_comment_treesitter() {
        let source = "/// `r[haha.hehe]` yey\nfn foo() {}\n";
        let refs = extract_refs_with_warnings(Path::new("test.rs"), source);
        assert_eq!(
            refs.references.len(),
            0,
            "inline code in /// should produce no annotations"
        );
    }

    #[test]
    fn test_legitimate_ref_after_whitespace_treesitter() {
        let source = "/// r[impl foo.bar]\nfn foo() {}\n";
        let refs = extract_refs_with_warnings(Path::new("test.rs"), source);
        assert_eq!(refs.references.len(), 1);
        assert_eq!(refs.references[0].prefix, "r");
        assert_eq!(refs.references[0].verb, "impl");
        assert_eq!(refs.references[0].req_id.to_string(), "foo.bar");
    }
}
