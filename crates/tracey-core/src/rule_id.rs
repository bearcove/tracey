use facet::Facet;
use std::fmt::{Display, Formatter};

/// Structured rule ID representation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
pub struct RuleId {
    /// Base rule ID without version suffix.
    pub base: String,
    /// Normalized version number (unversioned IDs are version 1).
    pub version: u32,
}

impl RuleId {
    pub fn new(base: impl Into<String>, version: u32) -> Option<Self> {
        if version == 0 {
            return None;
        }
        let base = base.into();
        if base.is_empty() || base.contains('+') {
            return None;
        }
        Some(Self { base, version })
    }

    /// Canonical string form (`base` for v1, `base+N` otherwise).
    pub fn canonical(&self) -> String {
        self.to_string()
    }

    /// Returns true if this rule's base starts with the given prefix.
    pub fn base_starts_with(&self, prefix: &str) -> bool {
        self.base.starts_with(prefix)
    }
}

impl Display for RuleId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.version == 1 {
            f.write_str(&self.base)
        } else {
            write!(f, "{}+{}", self.base, self.version)
        }
    }
}

impl PartialEq<&str> for RuleId {
    fn eq(&self, other: &&str) -> bool {
        parse_rule_id(other).is_some_and(|parsed| parsed == *self)
    }
}

impl PartialEq<RuleId> for &str {
    fn eq(&self, other: &RuleId) -> bool {
        parse_rule_id(self).is_some_and(|parsed| parsed == *other)
    }
}

impl AsRef<str> for RuleId {
    fn as_ref(&self) -> &str {
        &self.base
    }
}

/// Relationship between a reference ID and a rule definition ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleIdMatch {
    /// Same base ID and same normalized version.
    Exact,
    /// Same base ID but reference points to an older version.
    Stale,
    /// Different base ID or newer-version reference.
    NoMatch,
}

/// Parse a rule ID with optional `+N` suffix.
///
/// r[impl ref.syntax.version]
/// r[impl ref.syntax.version.implicit]
pub fn parse_rule_id(id: &str) -> Option<RuleId> {
    if id.is_empty() {
        return None;
    }

    if let Some((base, version_str)) = id.rsplit_once('+') {
        if base.is_empty() || base.contains('+') || version_str.is_empty() {
            return None;
        }
        let version = version_str.parse::<u32>().ok()?;
        RuleId::new(base, version)
    } else if id.contains('+') {
        None
    } else {
        RuleId::new(id, 1)
    }
}

/// Compare two structured rule IDs.
pub fn classify_reference_for_rule(rule_id: &RuleId, reference_id: &RuleId) -> RuleIdMatch {
    if rule_id.base != reference_id.base {
        return RuleIdMatch::NoMatch;
    }
    if rule_id.version == reference_id.version {
        RuleIdMatch::Exact
    } else if reference_id.version < rule_id.version {
        RuleIdMatch::Stale
    } else {
        RuleIdMatch::NoMatch
    }
}

/// Parse and compare string rule IDs.
pub fn classify_reference_for_rule_str(rule_id: &str, reference_id: &str) -> RuleIdMatch {
    let Some(rule) = parse_rule_id(rule_id) else {
        return RuleIdMatch::NoMatch;
    };
    let Some(reference) = parse_rule_id(reference_id) else {
        return RuleIdMatch::NoMatch;
    };
    classify_reference_for_rule(&rule, &reference)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rule_id_supports_implicit_v1() {
        let parsed = parse_rule_id("auth.login").expect("must parse");
        assert_eq!(parsed.base, "auth.login");
        assert_eq!(parsed.version, 1);
    }

    #[test]
    fn parse_rule_id_supports_explicit_version() {
        let parsed = parse_rule_id("auth.login+2").expect("must parse");
        assert_eq!(parsed.base, "auth.login");
        assert_eq!(parsed.version, 2);
    }

    #[test]
    fn parse_rule_id_rejects_invalid_suffix() {
        assert!(parse_rule_id("auth.login+").is_none());
        assert!(parse_rule_id("auth.login+0").is_none());
        assert!(parse_rule_id("auth.login+abc").is_none());
        assert!(parse_rule_id("auth+login+2").is_none());
    }

    #[test]
    fn classify_reference_detects_stale() {
        let rule = parse_rule_id("auth.login+2").expect("must parse");
        let old = parse_rule_id("auth.login").expect("must parse");
        let old2 = parse_rule_id("auth.login+1").expect("must parse");
        assert_eq!(classify_reference_for_rule(&rule, &old), RuleIdMatch::Stale);
        assert_eq!(
            classify_reference_for_rule(&rule, &old2),
            RuleIdMatch::Stale
        );
    }

    #[test]
    fn classify_reference_detects_exact() {
        let rule = parse_rule_id("auth.login+2").expect("must parse");
        let same = parse_rule_id("auth.login+2").expect("must parse");
        assert_eq!(
            classify_reference_for_rule(&rule, &same),
            RuleIdMatch::Exact
        );

        let rule = parse_rule_id("auth.login").expect("must parse");
        let same = parse_rule_id("auth.login+1").expect("must parse");
        assert_eq!(
            classify_reference_for_rule(&rule, &same),
            RuleIdMatch::Exact
        );
    }

    #[test]
    fn classify_reference_detects_no_match() {
        let rule = parse_rule_id("auth.login+2").expect("must parse");
        let newer = parse_rule_id("auth.login+3").expect("must parse");
        let other = parse_rule_id("auth.logout").expect("must parse");
        assert_eq!(
            classify_reference_for_rule(&rule, &newer),
            RuleIdMatch::NoMatch
        );
        assert_eq!(
            classify_reference_for_rule(&rule, &other),
            RuleIdMatch::NoMatch
        );
    }
}
