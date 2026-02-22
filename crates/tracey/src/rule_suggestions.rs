use std::collections::{HashMap, HashSet};

use strsim::levenshtein;
use tracey_core::RuleId;

pub(crate) fn suggest_similar_rule_ids(
    reference_id: &RuleId,
    known_rule_ids: &[RuleId],
    limit: usize,
) -> Vec<RuleId> {
    let mut latest_by_base: HashMap<String, RuleId> = HashMap::new();
    for rule_id in known_rule_ids {
        let entry = latest_by_base
            .entry(rule_id.base.clone())
            .or_insert_with(|| rule_id.clone());
        if rule_id.version > entry.version {
            *entry = rule_id.clone();
        }
    }

    let target = reference_id.base.as_str();
    let target_segments: HashSet<&str> = target.split('.').collect();
    let mut scored: Vec<(i32, RuleId)> = latest_by_base
        .into_values()
        .filter_map(|candidate| {
            let cand = candidate.base.as_str();
            let dist = levenshtein(target, cand) as i32;
            let prefix = common_prefix_len(target, cand) as i32;
            let cand_segments: HashSet<&str> = cand.split('.').collect();
            let overlap = target_segments.intersection(&cand_segments).count() as i32;
            let version_delta = (candidate.version as i32 - reference_id.version as i32).abs();
            let score = overlap * 40 + prefix - dist * 4 - version_delta;

            let max_dist = (target.len().max(cand.len()) / 3).max(3) as i32;
            let passes = overlap > 0 || prefix >= 5 || dist <= max_dist;
            if passes {
                Some((score, candidate))
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|(sa, a), (sb, b)| sb.cmp(sa).then_with(|| b.version.cmp(&a.version)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, rule_id)| rule_id)
        .collect()
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}
