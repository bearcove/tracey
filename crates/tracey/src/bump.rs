//! `tracey pre-commit` and `tracey bump` implementation.
//!
//! These commands work directly on the git index (staged files) and do not
//! require the daemon. They detect spec rules whose text was modified without
//! bumping the version number, and can automatically fix them.
//!
//! By default rules are compared between `HEAD` and the index. The `from`
//! revision can be overridden for both commands. `pre_commit` can compare
//! against a `to` revision instead of the index, and `bump` can compare
//! against the working tree (without re-staging) instead of the index.

use eyre::{Result, WrapErr, bail};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracey_core::{SpecFormat, id_range_in_marker, parse_spec, rewrite_marker};

use crate::config::Config;

/// The "new" side of a comparison against the `from` revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareTarget {
    /// The git index (staged changes).
    Index,
    /// The working tree.
    WorkingTree,
    /// A git revision.
    Revision(String),
}

/// A rule whose text changed in the compare target (by default the staged
/// index) but whose version was not bumped.
#[derive(Debug)]
pub struct ChangedRule {
    /// Spec file path, relative to project root.
    pub file: PathBuf,
    /// Spec dialect of `file`.
    pub format: SpecFormat,
    /// Rule ID as it appears in the compare target (version not yet bumped).
    /// Uses `marq::RuleId` since it comes directly from spec parsing.
    pub rule_id: marq::RuleId,
    /// Raw markdown text of the rule before the change (from the `from` revision).
    pub old_raw: String,
    /// Raw markdown text of the rule after the change (from the compare target).
    pub new_raw: String,
    /// Byte span of the `prefix[id]` marker in the **compare target** content.
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

    String::from_utf8(out.stdout)
        .wrap_err_with(|| format!("git {} output is not valid UTF-8", args.join(" ")))
}

/// Read a blob from the git object database via `git cat-file blob`.
/// `revision` is `""` for the index (`:path`) or e.g. `"HEAD"` for a commit.
/// Returns `None` if the object doesn't exist (new or deleted file).
/// Returns `Err` if the content is not valid UTF-8.
pub fn git_cat_file(project_root: &Path, revision: &str, path: &str) -> Result<Option<String>> {
    let spec = if revision.is_empty() {
        format!(":{path}")
    } else {
        format!("{revision}:{path}")
    };
    let out = std::process::Command::new("git")
        .args(["cat-file", "blob", &spec])
        .current_dir(project_root)
        .output()
        .wrap_err("failed to run git cat-file")?;

    if !out.status.success() {
        return Ok(None);
    }

    String::from_utf8(out.stdout)
        .map(Some)
        .wrap_err_with(|| format!("content of {spec} is not valid UTF-8"))
}

/// Read a file from the project filesystem.
/// Returns `None` if it doesn't exist.
/// Returns `Err` if it exists but cannot be read as UTF-8 text.
fn fs_cat_file(project_root: &Path, path: &str) -> Result<Option<String>> {
    let full_path = project_root.join(path);
    if !full_path.is_file() {
        return Ok(None);
    }
    std::fs::read_to_string(&full_path)
        .map(Some)
        .wrap_err_with(|| format!("failed to read {}", full_path.display()))
}

/// Resolve a git revision to a commit ID.
/// Returns `None` if the revision does not resolve to a commit.
fn resolve_commit(project_root: &Path, revision: &str) -> Option<String> {
    // Refuse anything git could interpret as an option.
    if revision.starts_with('-') {
        return None;
    }
    let out = std::process::Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{revision}^{{commit}}"),
        ])
        .current_dir(project_root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let id = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!id.is_empty()).then_some(id)
}

/// Parse a spec document string and return a map from rule **base** ID → `ReqDefinition`.
async fn parse_spec_rules(
    fmt: SpecFormat,
    content: &str,
) -> Result<HashMap<String, marq::ReqDefinition>> {
    let doc = parse_spec(fmt, content)
        .await
        .map_err(|e| eyre::eyre!("failed to parse spec: {e}"))?;

    Ok(doc
        .reqs
        .into_iter()
        .map(|r| (r.id.base.clone(), r))
        .collect())
}

/// Detect rules whose text changed between `from` and `to` without a version bump.
///
/// `from` is the revision to compare against; `None` means `HEAD`. If `from`
/// is `None` and there is no `HEAD` (new repo, first commit), no changes are
/// reported. An explicit `from` that does not resolve to a commit is an error,
/// as is a `CompareTarget::Revision` that does not resolve.
///
/// For each spec file changed between `from` and `to` this function:
/// 1. Reads the `from` and `to` content.
/// 2. Parses both with marq.
/// 3. Compares rules that share the same base ID: if `raw` changed but the
///    version number did not increase, the rule is reported as changed.
pub async fn detect_changed_rules(
    project_root: &Path,
    config: &Config,
    from: Option<&str>,
    to: &CompareTarget,
) -> Result<Vec<ChangedRule>> {
    // Resolve an explicit `to` revision up front so that it is reported even
    // when there is no `HEAD`.
    let to = match to {
        CompareTarget::Revision(rev) => {
            CompareTarget::Revision(resolve_commit(project_root, rev).ok_or_else(|| {
                eyre::eyre!("could not resolve `to` revision `{rev}` to a commit")
            })?)
        }
        other => other.clone(),
    };

    let from_commit = match from {
        Some(rev) => resolve_commit(project_root, rev)
            .ok_or_else(|| eyre::eyre!("could not resolve `from` revision `{rev}` to a commit"))?,
        // Without a HEAD there is nothing to compare against — new repo, first commit.
        None => match resolve_commit(project_root, "HEAD") {
            Some(id) => id,
            None => return Ok(vec![]),
        },
    };

    let changed_output = match &to {
        CompareTarget::Index => git_capture(
            project_root,
            &["diff-index", "--name-only", "--cached", &from_commit],
        )?,
        CompareTarget::WorkingTree => {
            git_capture(project_root, &["diff-index", "--name-only", &from_commit])?
        }
        CompareTarget::Revision(to_commit) => git_capture(
            project_root,
            &["diff-tree", "-r", "--name-only", &from_commit, to_commit],
        )?,
    };

    // Collect all spec include patterns.
    let spec_patterns: Vec<&str> = config
        .specs
        .iter()
        .flat_map(|s| s.include.iter().map(String::as_str))
        .collect();

    let mut changed_rules = Vec::new();

    for changed_file in changed_output.lines() {
        let changed_file = changed_file.trim();
        if changed_file.is_empty() {
            continue;
        }

        // Only consider files that match a spec include pattern.
        if !spec_patterns.iter().any(|p| {
            globset::Glob::new(p)
                .map(|g| g.compile_matcher().is_match(changed_file))
                .unwrap_or(false)
        }) {
            continue;
        }

        // Skip files whose extension is not a recognised spec format. A glob
        // like `docs/spec/**/*` could otherwise match images / data files.
        let Some(fmt) = SpecFormat::from_path(Path::new(changed_file)) else {
            continue;
        };

        let old_content = git_cat_file(project_root, &from_commit, changed_file)?;
        let new_content = match &to {
            CompareTarget::Index => git_cat_file(project_root, "", changed_file)?,
            CompareTarget::WorkingTree => fs_cat_file(project_root, changed_file)?,
            CompareTarget::Revision(to_commit) => {
                git_cat_file(project_root, to_commit, changed_file)?
            }
        };
        let Some(new_content) = new_content else {
            continue; // deleted — nothing to check
        };

        let old_rules = match old_content {
            Some(ref c) => parse_spec_rules(fmt, c).await?,
            None => HashMap::new(), // new file
        };
        let new_rules = parse_spec_rules(fmt, &new_content).await?;

        for (base, new_req) in &new_rules {
            let Some(old_req) = old_rules.get(base) else {
                continue; // new rule, no prior version to compare against
            };

            // Text changed but version not bumped → needs a bump.
            if new_req.raw != old_req.raw && new_req.id.version == old_req.id.version {
                changed_rules.push(ChangedRule {
                    file: PathBuf::from(changed_file),
                    format: fmt,
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
/// `from` overrides the revision compared against (default `HEAD`), and `to`
/// compares against a revision instead of the index.
///
/// Prints diagnostics to stderr and returns whether the check passed.
pub async fn pre_commit(
    project_root: &Path,
    config: &Config,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<bool> {
    let target = match to {
        Some(rev) => CompareTarget::Revision(rev.to_string()),
        None => CompareTarget::Index,
    };
    let changes = detect_changed_rules(project_root, config, from, &target).await?;

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
    // `tracey bump` can only fix changes in the index, so only hint at it then.
    if to.is_none() {
        let bump_cmd = match from {
            Some(rev) => format!("tracey bump --from {rev}"),
            None => "tracey bump".to_string(),
        };
        eprintln!();
        eprintln!("Hint: run `{bump_cmd}` to automatically bump all changed rules, then re-stage.");
        eprintln!("      Or commit with --no-verify to skip this check.");
    }

    Ok(false)
}

/// Bump the version of every staged rule whose text changed, then re-stage the
/// affected files.
///
/// `from` overrides the revision compared against (default `HEAD`). If
/// `unstaged` is set, rules changed in the working tree are bumped instead,
/// and the affected files are written but not re-staged.
///
/// Edits are applied last-to-first within each file so that earlier byte
/// offsets are not invalidated by preceding edits.
pub async fn bump(
    project_root: &Path,
    config: &Config,
    from: Option<&str>,
    unstaged: bool,
) -> Result<Vec<marq::RuleId>> {
    let target = if unstaged {
        CompareTarget::WorkingTree
    } else {
        CompareTarget::Index
    };
    let changes = detect_changed_rules(project_root, config, from, &target).await?;

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
        // All changes in `indices` share the same file, hence the same format.
        let fmt = changes[indices[0]].format;
        // Read from the same source `detect_changed_rules` parsed so that
        // marker spans line up.
        let content = if unstaged {
            fs_cat_file(project_root, &file_str)?.ok_or_else(|| {
                eyre::eyre!("file disappeared from working tree: {}", file.display())
            })?
        } else {
            git_cat_file(project_root, "", &file_str)?
                .ok_or_else(|| eyre::eyre!("file disappeared from index: {}", file.display()))?
        };

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

            // Extract the current marker text and rebuild it with the new version.
            let span = change.marker_span;
            let marker_bytes = &bytes[span.offset..span.offset + span.length];
            let marker_str =
                std::str::from_utf8(marker_bytes).wrap_err("marker is not valid UTF-8")?;

            // Build the new marker, e.g. `r[auth.login+2]`.
            let id_range = id_range_in_marker(fmt, marker_str)?;
            let new_marker =
                rewrite_marker(marker_str, id_range, &change.rule_id.base, new_version)?;

            let start = span.offset;
            let end = start + span.length;
            bytes.splice(start..end, new_marker.into_bytes());

            bumped_ids.push(marq::RuleId {
                base: change.rule_id.base.clone(),
                version: new_version,
            });
        }

        // Write the modified content back, and re-stage unless bumping unstaged changes.
        let full_path = project_root.join(file.as_path());
        std::fs::write(&full_path, &bytes)
            .wrap_err_with(|| format!("failed to write {}", full_path.display()))?;

        if !unstaged {
            git_capture(project_root, &["update-index", "--add", "--", &file_str])
                .wrap_err_with(|| format!("failed to re-stage {}", file.display()))?;
        }
    }

    Ok(bumped_ids)
}
