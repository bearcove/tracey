//! `tracey pre-commit` and `tracey bump` implementation.
//!
//! These commands work directly on the git index (staged files) and do not
//! require the daemon. They detect spec rules whose text was modified without
//! bumping the version number, and can automatically fix them.

use eyre::{Result, WrapErr, bail};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use marq::{RenderOptions, render};

use crate::config::Config;
use crate::matches_glob;

/// A rule whose text changed in the staged index but whose version was not bumped.
pub struct ChangedRule {
    /// Spec file path, relative to project root.
    pub file: PathBuf,
    /// Rule ID as it appears in the index (version not yet bumped).
    /// Uses `marq::RuleId` since it comes directly from spec parsing.
    pub rule_id: marq::RuleId,
    /// Raw markdown text of the rule before the change (from HEAD).
    pub old_raw: String,
    /// Raw markdown text of the rule after the change (from index).
    pub new_raw: String,
    /// Byte span of the `prefix[id]` marker in the **index** content.
    /// Used to rewrite the version in-place.
    pub marker_span: marq::SourceSpan,
}

/// Run a git command in the project root and capture stdout.
pub fn git_capture(project_root: &Path, args: &[&str]) -> Result<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()
        .wrap_err("failed to run git")?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }

    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Get a file's content from the git index (`:path`) or HEAD (`HEAD:path`).
/// Returns `None` if the file doesn't exist at that revision (e.g. new file).
pub fn git_show(project_root: &Path, revision: &str, path: &str) -> Result<Option<String>> {
    let spec = format!("{revision}:{path}");
    let out = std::process::Command::new("git")
        .args(["show", &spec])
        .current_dir(project_root)
        .output()
        .wrap_err("failed to run git show")?;

    if !out.status.success() {
        return Ok(None);
    }

    Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
}

/// Parse a spec markdown string and return a map from rule **base** ID → `ReqDefinition`.
async fn parse_spec_rules(content: &str) -> Result<HashMap<String, marq::ReqDefinition>> {
    let doc = render(content, &RenderOptions::default())
        .await
        .map_err(|e| eyre::eyre!("failed to parse spec: {e}"))?;

    Ok(doc
        .reqs
        .into_iter()
        .map(|r| (r.id.base.clone(), r))
        .collect())
}

/// Detect rules that are staged with a text change but no version bump.
///
/// For each staged spec file this function:
/// 1. Reads HEAD and index content via `git show`.
/// 2. Parses both with marq.
/// 3. Compares rules that share the same base ID: if `raw` changed but the
///    version number did not increase, the rule is reported as changed.
pub async fn detect_changed_rules(
    project_root: &Path,
    config: &Config,
) -> Result<Vec<ChangedRule>> {
    let staged_output = git_capture(project_root, &["diff", "--cached", "--name-only"])?;

    // Collect all spec include patterns.
    let spec_patterns: Vec<&str> = config
        .specs
        .iter()
        .flat_map(|s| s.include.iter().map(String::as_str))
        .collect();

    let mut changed_rules = Vec::new();

    for staged_file in staged_output.lines() {
        let staged_file = staged_file.trim();
        if staged_file.is_empty() {
            continue;
        }

        // Only consider files that match a spec include pattern.
        if !spec_patterns.iter().any(|p| matches_glob(staged_file, p)) {
            continue;
        }

        let old_content = git_show(project_root, "HEAD", staged_file)?;
        let new_content = match git_show(project_root, "", staged_file)? {
            Some(c) => c,
            None => continue, // deleted — nothing to check
        };

        let old_rules = match old_content {
            Some(ref c) => parse_spec_rules(c).await?,
            None => HashMap::new(), // new file
        };
        let new_rules = parse_spec_rules(&new_content).await?;

        for (base, new_req) in &new_rules {
            let Some(old_req) = old_rules.get(base) else {
                continue; // new rule, no prior version to compare against
            };

            // Text changed but version not bumped → needs a bump.
            if new_req.raw != old_req.raw && new_req.id.version == old_req.id.version {
                changed_rules.push(ChangedRule {
                    file: PathBuf::from(staged_file),
                    rule_id: new_req.id.clone(),
                    old_raw: old_req.raw.clone(),
                    new_raw: new_req.raw.clone(),
                    marker_span: new_req.marker_span,
                });
            }
        }
    }

    Ok(changed_rules)
}

/// Check staged spec changes and exit non-zero if any rule text changed without
/// a version bump. Intended to be called from a git pre-commit hook.
///
/// Prints diagnostics to stderr and returns whether the check passed.
pub async fn pre_commit(project_root: &Path, config: &Config) -> Result<bool> {
    let changes = detect_changed_rules(project_root, config).await?;

    if changes.is_empty() {
        return Ok(true);
    }

    for change in &changes {
        eprintln!(
            "error: rule `{}` body changed but version was not bumped",
            change.rule_id
        );
        eprintln!("  file: {}", change.file.display());
    }
    eprintln!();
    eprintln!("Hint: run `tracey bump` to automatically bump all changed rules, then re-stage.");
    eprintln!("      Or commit with --no-verify to skip this check.");

    Ok(false)
}

/// Bump the version of every staged rule whose text changed, then re-stage the
/// affected files.
///
/// Edits are applied last-to-first within each file so that earlier byte
/// offsets are not invalidated by preceding edits.
pub async fn bump(project_root: &Path, config: &Config) -> Result<Vec<marq::RuleId>> {
    let changes = detect_changed_rules(project_root, config).await?;

    if changes.is_empty() {
        return Ok(vec![]);
    }

    // Group changes by file.
    let mut by_file: HashMap<PathBuf, Vec<usize>> = HashMap::new();
    for (i, change) in changes.iter().enumerate() {
        by_file.entry(change.file.clone()).or_default().push(i);
    }

    let mut bumped_ids = Vec::new();

    for (file, indices) in &by_file {
        let file_str = file.to_string_lossy();
        let content = git_show(project_root, "", &file_str)?
            .ok_or_else(|| eyre::eyre!("file disappeared from index: {}", file.display()))?;

        let mut bytes = content.into_bytes();

        // Sort indices so we apply edits from last byte offset to first.
        let mut sorted_indices = indices.clone();
        sorted_indices.sort_by(|&a, &b| {
            changes[b]
                .marker_span
                .offset
                .cmp(&changes[a].marker_span.offset)
        });

        for &idx in &sorted_indices {
            let change = &changes[idx];
            let new_version = change.rule_id.version + 1;

            // Extract the prefix (chars before `[`) from the current marker bytes.
            let span = change.marker_span;
            let marker_bytes = &bytes[span.offset..span.offset + span.length];
            let marker_str =
                std::str::from_utf8(marker_bytes).wrap_err("marker is not valid UTF-8")?;
            let bracket = marker_str
                .find('[')
                .ok_or_else(|| eyre::eyre!("malformed marker: {}", marker_str))?;
            let prefix = &marker_str[..bracket];

            // Build the new marker, e.g. `r[auth.login+2]`.
            let new_marker = format!("{}[{}+{}]", prefix, change.rule_id.base, new_version);

            let start = span.offset;
            let end = start + span.length;
            bytes.splice(start..end, new_marker.into_bytes());

            bumped_ids.push(marq::RuleId {
                base: change.rule_id.base.clone(),
                version: new_version,
            });
        }

        // Write the modified content back and re-stage.
        let full_path = project_root.join(file.as_path());
        std::fs::write(&full_path, &bytes)
            .wrap_err_with(|| format!("failed to write {}", full_path.display()))?;

        git_capture(project_root, &["add", &file_str])
            .wrap_err_with(|| format!("failed to re-stage {}", file.display()))?;
    }

    Ok(bumped_ids)
}
