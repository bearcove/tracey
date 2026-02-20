/// Parsed rule ID with normalized version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedRuleId<'a> {
    /// Base rule ID without the version suffix.
    pub base: &'a str,
    /// Normalized version number (unversioned IDs are version 1).
    pub version: u32,
}

/// Relationship between a reference ID and a rule definition ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleIdMatch {
    /// Same base ID and same normalized version.
    Exact,
    /// Same base ID but reference points to an older version.
    Stale,
    /// Different base ID, newer version reference, or unparsable ID.
    NoMatch,
}

/// Parse a rule ID with optional `+N` suffix.
///
/// Examples:
/// - `auth.login` => base `auth.login`, version `1`
/// - `auth.login+2` => base `auth.login`, version `2`
pub fn parse_rule_id(id: &str) -> Option<ParsedRuleId<'_>> {
    if id.is_empty() {
        return None;
    }

    if let Some((base, version_str)) = id.rsplit_once('+') {
        if base.is_empty() || base.contains('+') || version_str.is_empty() {
            return None;
        }
        let version = version_str.parse::<u32>().ok()?;
        if version == 0 {
            return None;
        }
        Some(ParsedRuleId { base, version })
    } else if id.contains('+') {
        None
    } else {
        Some(ParsedRuleId {
            base: id,
            version: 1,
        })
    }
}

/// Compare a reference ID against a rule definition ID.
pub fn classify_reference_for_rule(rule_id: &str, reference_id: &str) -> RuleIdMatch {
    let Some(rule) = parse_rule_id(rule_id) else {
        return if rule_id == reference_id {
            RuleIdMatch::Exact
        } else {
            RuleIdMatch::NoMatch
        };
    };
    let Some(reference) = parse_rule_id(reference_id) else {
        return RuleIdMatch::NoMatch;
    };

    if rule.base != reference.base {
        return RuleIdMatch::NoMatch;
    }
    if rule.version == reference.version {
        RuleIdMatch::Exact
    } else if reference.version < rule.version {
        RuleIdMatch::Stale
    } else {
        RuleIdMatch::NoMatch
    }
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
        assert_eq!(
            classify_reference_for_rule("auth.login+2", "auth.login"),
            RuleIdMatch::Stale
        );
        assert_eq!(
            classify_reference_for_rule("auth.login+2", "auth.login+1"),
            RuleIdMatch::Stale
        );
    }

    #[test]
    fn classify_reference_detects_exact() {
        assert_eq!(
            classify_reference_for_rule("auth.login+2", "auth.login+2"),
            RuleIdMatch::Exact
        );
        assert_eq!(
            classify_reference_for_rule("auth.login", "auth.login+1"),
            RuleIdMatch::Exact
        );
    }

    #[test]
    fn classify_reference_detects_no_match() {
        assert_eq!(
            classify_reference_for_rule("auth.login+2", "auth.login+3"),
            RuleIdMatch::NoMatch
        );
        assert_eq!(
            classify_reference_for_rule("auth.login+2", "auth.logout"),
            RuleIdMatch::NoMatch
        );
    }
}
