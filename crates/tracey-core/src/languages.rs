//! Single registry of languages tracey can scan.
//!
//! Adding a language is a one-row edit to the [`define_languages!`] invocation
//! below — no other file needs to change. The macro derives both the flat
//! [`SUPPORTED_EXTENSIONS`] list (always compiled) and the tree-sitter
//! [`LANGUAGES`] table (behind `feature = "reverse"`) from the same rows, so
//! the three former dispatch sites can never drift out of sync again.
//!
//! Each row may also carry an `arborium` highlight-language name and a
//! `devicon` CSS class so the dashboard / daemon don't keep their own
//! ext-match tables.

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

/// Per-row UI metadata that is always compiled (no `reverse` feature gate):
/// `(extensions, arborium highlight name, devicon CSS class)`.
type LangMeta = (&'static [&'static str], Option<&'static str>, Option<&'static str>);

/// Arborium highlight-language name for a file extension, if the registry
/// row carries one. Rows whose extensions map to *different* highlight
/// languages (e.g. the ts/js/jsx row) deliberately leave this `None` so the
/// caller can fall through to a residual match.
pub fn arborium_for_ext(ext: &str) -> Option<&'static str> {
    LANG_META
        .iter()
        .find(|(exts, ..)| exts.contains(&ext))
        .and_then(|(_, a, _)| *a)
}

/// Devicon CSS class for a file extension, if the registry row carries one.
pub fn devicon_for_ext(ext: &str) -> Option<&'static str> {
    LANG_META
        .iter()
        .find(|(exts, ..)| exts.contains(&ext))
        .and_then(|(_, _, d)| *d)
}

/// Helper: turn an optional macro capture into `Some(lit)` / `None`.
macro_rules! __opt {
    () => {
        None
    };
    ($v:literal) => {
        Some($v)
    };
}

/// Expands a single declarative table into:
///   * `SUPPORTED_EXTENSIONS` — every extension from every row (plus lexer-only),
///   * `LANG_META` — per-row `(exts, arborium, devicon)` (always compiled),
///   * `LANGUAGES` — one [`Lang`] per row (only when `reverse` is enabled).
macro_rules! define_languages {
    (
        $(
            [$($ext:literal),+ $(,)?] => $grammar:path, $extract:path
            $(, arborium = $arb:literal)?
            $(, devicon = $dev:literal)?
            ;
        )*
        @lexer_only [$($lex:literal),* $(,)?]
    ) => {
        /// File extensions that tracey knows how to scan for requirement references.
        pub const SUPPORTED_EXTENSIONS: &[&str] = &[
            $( $($ext,)+ )*
            $( $lex, )*
        ];

        /// Per-row UI metadata (always compiled).
        static LANG_META: &[LangMeta] = &[
            $( (&[$($ext),+], __opt!($($arb)?), __opt!($($dev)?)), )*
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
    ["rs"]                                  => arborium_rust::language,       code_units::extract_rust,       arborium = "rust",       devicon = "devicon-rust-original";
    ["swift"]                               => arborium_swift::language,      code_units::extract_swift,      arborium = "swift",      devicon = "devicon-swift-plain";
    ["go"]                                  => arborium_go::language,         code_units::extract_go,         arborium = "go",         devicon = "devicon-go-plain";
    ["java"]                                => arborium_java::language,       code_units::extract_java,       arborium = "java",       devicon = "devicon-java-plain";
    ["py"]                                  => arborium_python::language,     code_units::extract_python,     arborium = "python",     devicon = "devicon-python-plain";
    // ts/js/jsx share one grammar but differ in highlight name + icon, so the
    // row leaves both `None` and callers fall through to a residual match.
    ["ts", "tsx", "js", "jsx", "mts", "cts"] => arborium_typescript::language, code_units::extract_typescript;
    ["php"]                                 => arborium_php::language,        code_units::extract_php,        arborium = "php",        devicon = "devicon-php-plain";
    ["c", "h"]                              => arborium_c::language,          code_units::extract_c,          arborium = "c",          devicon = "devicon-c-plain";
    ["cpp", "cc", "cxx", "hpp"]             => arborium_cpp::language,        code_units::extract_cpp,        arborium = "cpp",        devicon = "devicon-cplusplus-plain";
    ["rb"]                                  => arborium_ruby::language,       code_units::extract_ruby,       arborium = "ruby",       devicon = "devicon-ruby-plain";
    ["r", "R"]                              => arborium_r::language,          code_units::extract_r,          arborium = "r",          devicon = "devicon-r-plain";
    ["dart"]                                => arborium_dart::language,       code_units::extract_dart,       arborium = "dart",       devicon = "devicon-dart-plain";
    ["lua"]                                 => arborium_lua::language,        code_units::extract_lua,        arborium = "lua",        devicon = "devicon-lua-plain";
    ["asm", "s", "S"]                       => arborium_asm::language,        code_units::extract_asm,        arborium = "asm";
    ["pl", "pm"]                            => arborium_perl::language,       code_units::extract_perl,       arborium = "perl",       devicon = "devicon-perl-plain";
    ["hs", "lhs"]                           => arborium_haskell::language,    code_units::extract_haskell,    arborium = "haskell",    devicon = "devicon-haskell-plain";
    ["ex", "exs"]                           => arborium_elixir::language,     code_units::extract_elixir,     arborium = "elixir",     devicon = "devicon-elixir-plain";
    ["erl", "hrl"]                          => arborium_erlang::language,     code_units::extract_erlang,     arborium = "erlang",     devicon = "devicon-erlang-plain";
    ["clj", "cljs", "cljc", "edn"]          => arborium_clojure::language,    code_units::extract_clojure,    arborium = "clojure",    devicon = "devicon-clojure-plain";
    ["fs", "fsi", "fsx"]                    => arborium_fsharp::language,     code_units::extract_fsharp,     arborium = "fsharp",     devicon = "devicon-fsharp-plain";
    ["vb", "vbs"]                           => arborium_vb::language,         code_units::extract_vb,         arborium = "vb";
    ["cob", "cbl", "cpy"]                   => arborium_cobol::language,      code_units::extract_cobol,      arborium = "cobol";
    ["jl"]                                  => arborium_julia::language,      code_units::extract_julia,      arborium = "julia",      devicon = "devicon-julia-plain";
    ["d"]                                   => arborium_d::language,          code_units::extract_d,          arborium = "d";
    ["ps1", "psm1", "psd1"]                 => arborium_powershell::language, code_units::extract_powershell, arborium = "powershell", devicon = "devicon-powershell-plain";
    ["cmake"]                               => arborium_cmake::language,      code_units::extract_cmake,      arborium = "cmake";
    ["ml", "mli"]                           => arborium_ocaml::language,      code_units::extract_ocaml,      arborium = "ocaml",      devicon = "devicon-ocaml-plain";
    ["sh", "bash", "zsh"]                   => arborium_bash::language,       code_units::extract_bash,       arborium = "bash",       devicon = "devicon-bash-plain";
    ["nix"]                                 => arborium_nix::language,        code_units::extract_nix,        arborium = "nix";
    ["lean"]                                => arborium_lean::language,       code_units::extract_lean,       arborium = "lean";

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

    #[test]
    fn meta_lookups() {
        assert_eq!(arborium_for_ext("rs"), Some("rust"));
        assert_eq!(devicon_for_ext("rs"), Some("devicon-rust-original"));
        // ts row deliberately leaves both None.
        assert_eq!(arborium_for_ext("tsx"), None);
        assert_eq!(devicon_for_ext("jsx"), None);
        // Unknown ext.
        assert_eq!(arborium_for_ext("md"), None);
    }
}
