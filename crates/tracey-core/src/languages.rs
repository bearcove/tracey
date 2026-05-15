//! Single registry of languages tracey can scan.
//!
//! Adding a language is a one-row edit to the [`define_languages!`] invocation
//! below — no other file needs to change. The macro derives both the flat
//! [`SUPPORTED_EXTENSIONS`] list (always compiled) and the tree-sitter
//! [`LANGUAGES`] table (behind `feature = "reverse"`) from the same rows, so
//! the three former dispatch sites can never drift out of sync again.

#[cfg(feature = "reverse")]
use crate::code_units::{self, CodeUnits};
#[cfg(feature = "reverse")]
use std::path::Path;

/// A language with full tree-sitter support (grammar + structural extraction).
#[cfg(feature = "reverse")]
pub struct Lang {
    /// File extensions this language handles (without leading dot).
    pub extensions: &'static [&'static str],
    /// Tree-sitter grammar for comment-aware ref extraction.
    pub grammar: fn() -> arborium_tree_sitter::LanguageFn,
    /// Structural extraction (functions, types, etc.) for reverse coverage.
    pub extract: fn(&Path, &str) -> CodeUnits,
}

/// Look up the language registered for a file extension.
#[cfg(feature = "reverse")]
pub fn for_ext(ext: &str) -> Option<&'static Lang> {
    LANGUAGES.iter().find(|l| l.extensions.contains(&ext))
}

/// Expands a single declarative table into:
///   * `SUPPORTED_EXTENSIONS` — every extension from every row (plus lexer-only),
///   * `LANGUAGES` — one [`Lang`] per row (only when `reverse` is enabled).
macro_rules! define_languages {
    (
        $( [$($ext:literal),+ $(,)?] => $grammar:path, $extract:path; )*
        @lexer_only [$($lex:literal),* $(,)?]
    ) => {
        /// File extensions that tracey knows how to scan for requirement references.
        pub const SUPPORTED_EXTENSIONS: &[&str] = &[
            $( $($ext,)+ )*
            $( $lex, )*
        ];

        /// One entry per language with tree-sitter support.
        #[cfg(feature = "reverse")]
        pub static LANGUAGES: &[Lang] = &[
            $( Lang {
                extensions: &[$($ext),+],
                grammar: $grammar,
                extract: $extract,
            }, )*
        ];
    };
}

define_languages! {
    ["rs"]                                  => arborium_rust::language,       code_units::extract_rust;
    ["swift"]                               => arborium_swift::language,      code_units::extract_swift;
    ["go"]                                  => arborium_go::language,         code_units::extract_go;
    ["java"]                                => arborium_java::language,       code_units::extract_java;
    ["py"]                                  => arborium_python::language,     code_units::extract_python;
    ["ts", "tsx", "js", "jsx", "mts", "cts"] => arborium_typescript::language, code_units::extract_typescript;
    ["php"]                                 => arborium_php::language,        code_units::extract_php;
    ["c", "h"]                              => arborium_c::language,          code_units::extract_c;
    ["cpp", "cc", "cxx", "hpp"]             => arborium_cpp::language,        code_units::extract_cpp;
    ["rb"]                                  => arborium_ruby::language,       code_units::extract_ruby;
    ["r", "R"]                              => arborium_r::language,          code_units::extract_r;
    ["dart"]                                => arborium_dart::language,       code_units::extract_dart;
    ["lua"]                                 => arborium_lua::language,        code_units::extract_lua;
    ["asm", "s", "S"]                       => arborium_asm::language,        code_units::extract_asm;
    ["pl", "pm"]                            => arborium_perl::language,       code_units::extract_perl;
    ["hs", "lhs"]                           => arborium_haskell::language,    code_units::extract_haskell;
    ["ex", "exs"]                           => arborium_elixir::language,     code_units::extract_elixir;
    ["erl", "hrl"]                          => arborium_erlang::language,     code_units::extract_erlang;
    ["clj", "cljs", "cljc", "edn"]          => arborium_clojure::language,    code_units::extract_clojure;
    ["fs", "fsi", "fsx"]                    => arborium_fsharp::language,     code_units::extract_fsharp;
    ["vb", "vbs"]                           => arborium_vb::language,         code_units::extract_vb;
    ["cob", "cbl", "cpy"]                   => arborium_cobol::language,      code_units::extract_cobol;
    ["jl"]                                  => arborium_julia::language,      code_units::extract_julia;
    ["d"]                                   => arborium_d::language,          code_units::extract_d;
    ["ps1", "psm1", "psd1"]                 => arborium_powershell::language, code_units::extract_powershell;
    ["cmake"]                               => arborium_cmake::language,      code_units::extract_cmake;
    ["ml", "mli"]                           => arborium_ocaml::language,      code_units::extract_ocaml;
    ["sh", "bash", "zsh"]                   => arborium_bash::language,       code_units::extract_bash;
    ["nix"]                                 => arborium_nix::language,        code_units::extract_nix;
    ["lean"]                                => arborium_lean::language,       code_units::extract_lean;

    // Extensions scanned by the text-based lexer fallback only (no tree-sitter
    // grammar wired up yet). Kept for backwards compatibility — promote to a
    // full row above once a grammar + extractor exist.
    @lexer_only ["m", "mm", "kt", "kts", "scala", "groovy", "cs", "zig"]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn every_extension_is_supported() {
        for ext in SUPPORTED_EXTENSIONS {
            assert!(
                crate::is_supported_extension(OsStr::new(ext)),
                "extension {ext:?} not reported as supported"
            );
        }
    }

    #[cfg(feature = "reverse")]
    #[test]
    fn every_language_is_reachable() {
        for lang in LANGUAGES {
            for ext in lang.extensions {
                assert!(
                    crate::is_supported_extension(OsStr::new(ext)),
                    "extension {ext:?} missing from SUPPORTED_EXTENSIONS"
                );
                assert!(
                    for_ext(ext).is_some(),
                    "for_ext({ext:?}) returned None for a registered language"
                );
            }
        }
    }

    #[test]
    fn no_duplicate_extensions() {
        // Iterates the full SUPPORTED_EXTENSIONS list (full rows + @lexer_only),
        // so promoting a lexer-only extension without removing it from the
        // @lexer_only block is caught here.
        let mut seen = std::collections::HashSet::new();
        for ext in SUPPORTED_EXTENSIONS {
            assert!(
                seen.insert(*ext),
                "extension {ext:?} appears more than once in the registry"
            );
        }
    }
}
