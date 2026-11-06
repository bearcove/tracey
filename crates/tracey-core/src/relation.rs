//! StrictDoc-style `@relation(...)` source markers, resolved to tracey verbs.
//!
//! Shared by the tree-sitter and the text-based extraction paths so both
//! interpret markers identically.

use strictdoc_parser::{RelationRole, RelationScope};

/// The tracey verb a `@relation` marker maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelationVerb {
    Impl,
    Verify,
}

impl RelationVerb {
    #[cfg(feature = "reverse")]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            RelationVerb::Impl => "impl",
            RelationVerb::Verify => "verify",
        }
    }
}

/// A `@relation` marker found in a comment. `start..end` is the byte range
/// of the call within the scanned text.
#[derive(Debug)]
pub(crate) enum RelationMarker {
    /// Produces one reference per UID.
    Refs {
        start: usize,
        end: usize,
        verb: RelationVerb,
        uids: Vec<String>,
    },
    /// A malformed marker, or one whose role tracey doesn't map.
    Warning { start: usize, end: usize },
}

impl RelationMarker {
    pub(crate) fn start(&self) -> usize {
        match self {
            RelationMarker::Refs { start, .. } | RelationMarker::Warning { start, .. } => *start,
        }
    }
}

/// Find the `@relation` markers in a comment's text.
///
/// Recognition follows upstream StrictDoc (see
/// [`strictdoc_parser::find_relation_annotations`]): a marker must start a
/// comment line, so mentions in prose are ignored.
///
/// - No role, or an implements-like role (`Implements`, `Implementation`,
///   …) maps to `impl`; a verifies-like role (`Verifies`, `Verification`,
///   `Test`, …) maps to `verify`. Other roles produce a warning.
/// - `scope=range_end` closes the range opened by a `scope=range_start`
///   marker, which already produced the references, so it is skipped.
/// - Malformed markers produce a warning instead of being dropped silently.
pub(crate) fn relation_markers(text: &str) -> Vec<RelationMarker> {
    let mut out = Vec::new();
    for result in strictdoc_parser::find_relation_annotations(text) {
        let ann = match result {
            Ok(ann) => ann,
            Err(err) => {
                let end = err
                    .offset
                    .max(err.start + "@relation".len())
                    .min(text.len());
                out.push(RelationMarker::Warning {
                    start: err.start,
                    end,
                });
                continue;
            }
        };
        if ann.scope == Some(RelationScope::RangeEnd) {
            continue;
        }
        let verb = match ann.role_kind() {
            None | Some(RelationRole::Implements) => RelationVerb::Impl,
            Some(RelationRole::Verifies) => RelationVerb::Verify,
            Some(RelationRole::Refines | RelationRole::Other(_)) => {
                out.push(RelationMarker::Warning {
                    start: ann.start,
                    end: ann.end,
                });
                continue;
            }
        };
        out.push(RelationMarker::Refs {
            start: ann.start,
            end: ann.end,
            verb,
            uids: ann.uids,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(text: &str) -> Vec<(RelationVerb, Vec<String>)> {
        relation_markers(text)
            .into_iter()
            .filter_map(|m| match m {
                RelationMarker::Refs { verb, uids, .. } => Some((verb, uids)),
                RelationMarker::Warning { .. } => None,
            })
            .collect()
    }

    fn warnings(text: &str) -> usize {
        relation_markers(text)
            .iter()
            .filter(|m| matches!(m, RelationMarker::Warning { .. }))
            .count()
    }

    #[test]
    fn noun_and_verb_roles_map_to_verbs() {
        for (role, verb) in [
            ("Implements", RelationVerb::Impl),
            ("Implementation", RelationVerb::Impl),
            ("Verifies", RelationVerb::Verify),
            ("Verification", RelationVerb::Verify),
            ("Test", RelationVerb::Verify),
        ] {
            let text = format!("// @relation(CH-001, scope=function, role={role})");
            assert_eq!(
                refs(&text),
                vec![(verb, vec!["CH-001".to_string()])],
                "{role}"
            );
        }
        assert_eq!(refs("// @relation(CH-001)")[0].0, RelationVerb::Impl);
    }

    #[test]
    fn unmapped_roles_and_malformed_markers_warn() {
        assert_eq!(warnings("// @relation(CH-001, role=Refines)"), 1);
        assert_eq!(warnings("// @relation(CH-001, scope=bogus)"), 1);
        assert_eq!(warnings("// @relation(CH-001,scope=function)"), 1);
        assert_eq!(warnings("// @relation(CH-001"), 1);
    }

    #[test]
    fn all_scopes_count_but_range_end_is_not_doubled() {
        for scope in ["file", "class", "function", "line", "range_start"] {
            let text = format!("# @relation(CH-001, scope={scope})");
            assert_eq!(refs(&text).len(), 1, "{scope}");
        }
        assert!(relation_markers("# @relation(CH-001, scope=range_end)").is_empty());
    }

    #[test]
    fn prose_mentions_are_ignored() {
        assert!(relation_markers("// see @relation(CH-001) for details").is_empty());
        assert_eq!(refs("foo(); // @relation(CH-001, scope=line)").len(), 1);
    }
}
