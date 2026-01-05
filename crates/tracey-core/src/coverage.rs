//! Coverage analysis and reporting

use crate::lexer::{RefVerb, RuleReference, Rules};
use facet::Facet;
use std::collections::{HashMap, HashSet};

/// Coverage analysis results for a single spec
#[derive(Debug, Facet)]
pub struct CoverageReport {
    /// Name of the spec
    pub spec_name: String,

    /// Total number of rules in the spec
    pub total_rules: usize,

    /// Rules that are referenced at least once
    pub covered_rules: HashSet<String>,

    /// Rules that have no references (orphaned)
    pub uncovered_rules: HashSet<String>,

    /// References to rules that don't exist in the spec
    pub invalid_references: Vec<RuleReference>,

    /// All valid references, grouped by rule ID
    pub references_by_rule: HashMap<String, Vec<RuleReference>>,

    /// References grouped by verb type, then by rule ID
    pub references_by_verb: HashMap<RefVerb, HashMap<String, Vec<RuleReference>>>,
}

impl CoverageReport {
    /// Compute coverage from rules and a set of known rule IDs
    ///
    /// [impl coverage.compute.covered]
    /// [impl coverage.compute.uncovered]
    /// [impl coverage.compute.invalid]
    pub fn compute(
        spec_name: impl Into<String>,
        known_rule_ids: &HashSet<String>,
        rules: &Rules,
    ) -> Self {
        let spec_name = spec_name.into();
        let mut covered_rules = HashSet::new();
        let mut invalid_references = Vec::new();
        let mut references_by_rule: HashMap<String, Vec<RuleReference>> = HashMap::new();
        let mut references_by_verb: HashMap<RefVerb, HashMap<String, Vec<RuleReference>>> =
            HashMap::new();

        for reference in &rules.references {
            if known_rule_ids.contains(&reference.rule_id) {
                covered_rules.insert(reference.rule_id.clone());
                references_by_rule
                    .entry(reference.rule_id.clone())
                    .or_default()
                    .push(reference.clone());

                // Also group by verb
                references_by_verb
                    .entry(reference.verb)
                    .or_default()
                    .entry(reference.rule_id.clone())
                    .or_default()
                    .push(reference.clone());
            } else {
                invalid_references.push(reference.clone());
            }
        }

        let uncovered_rules: HashSet<String> =
            known_rule_ids.difference(&covered_rules).cloned().collect();

        CoverageReport {
            spec_name,
            total_rules: known_rule_ids.len(),
            covered_rules,
            uncovered_rules,
            invalid_references,
            references_by_rule,
            references_by_verb,
        }
    }

    /// Coverage percentage (0.0 - 100.0)
    ///
    /// [impl coverage.compute.percentage]
    pub fn coverage_percent(&self) -> f64 {
        if self.total_rules == 0 {
            return 100.0;
        }
        (self.covered_rules.len() as f64 / self.total_rules as f64) * 100.0
    }

    /// Whether the coverage is "passing" (no invalid refs, >= threshold coverage)
    pub fn is_passing(&self, threshold: f64) -> bool {
        self.invalid_references.is_empty() && self.coverage_percent() >= threshold
    }
}
