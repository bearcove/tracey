//! Coverage analysis and reporting

use crate::lexer::{RefVerb, RuleReference};
use crate::spec::SpecManifest;
use std::collections::{HashMap, HashSet};

/// Coverage analysis results for a single spec
#[derive(Debug)]
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

    /// All valid references, grouped by rule ID (kept for API compatibility)
    #[allow(dead_code)]
    pub references_by_rule: HashMap<String, Vec<RuleReference>>,

    /// References grouped by verb type, then by rule ID
    pub references_by_verb: HashMap<RefVerb, HashMap<String, Vec<RuleReference>>>,
}

impl CoverageReport {
    /// Compute coverage from references and manifest
    pub fn compute(
        spec_name: String,
        manifest: &SpecManifest,
        references: Vec<RuleReference>,
    ) -> Self {
        let mut covered_rules = HashSet::new();
        let mut invalid_references = Vec::new();
        let mut references_by_rule: HashMap<String, Vec<RuleReference>> = HashMap::new();
        let mut references_by_verb: HashMap<RefVerb, HashMap<String, Vec<RuleReference>>> =
            HashMap::new();

        for reference in references {
            if manifest.has_rule(&reference.rule_id) {
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
                    .push(reference);
            } else {
                invalid_references.push(reference);
            }
        }

        let all_rules: HashSet<String> = manifest.rule_ids().map(|s| s.to_string()).collect();
        let uncovered_rules: HashSet<String> =
            all_rules.difference(&covered_rules).cloned().collect();

        CoverageReport {
            spec_name,
            total_rules: manifest.rules.len(),
            covered_rules,
            uncovered_rules,
            invalid_references,
            references_by_rule,
            references_by_verb,
        }
    }

    /// Coverage percentage (0.0 - 100.0)
    pub fn coverage_percent(&self) -> f64 {
        if self.total_rules == 0 {
            return 100.0;
        }
        (self.covered_rules.len() as f64 / self.total_rules as f64) * 100.0
    }

    /// Whether the coverage is "passing" (no invalid refs, > threshold coverage)
    pub fn is_passing(&self, threshold: f64) -> bool {
        self.invalid_references.is_empty() && self.coverage_percent() >= threshold
    }
}
