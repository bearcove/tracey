//! Optional smoke check: walk one or more checkouts of upstream StrictDoc
//! corpora and confirm the `.sdoc`→`ExtractedRule` bridge handles every
//! document without panicking and produces sensible counts.
//!
//! This test is `#[ignore]`d by default and reads corpus paths from the
//! `STRICTDOC_CORPUS` environment variable. Multiple paths may be passed,
//! separated by `:`. Run it with:
//!
//! ```text
//! STRICTDOC_CORPUS=/path/to/strictdoc:/path/to/reqmgmt \
//!   cargo test -p tracey --test sdoc_corpus_smoke -- --ignored --nocapture
//! ```
//!
//! Known-good corpora:
//! - `github.com/strictdoc-project/strictdoc` — the parser's own test
//!   suite (any tag from 0.21.0 onwards).
//! - `github.com/zephyrproject-rtos/reqmgmt` — Zephyr RTOS requirements
//!   management corpus in StrictDoc format.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Default)]
struct Counts {
    total: u32,
    ok: u32,
    err: u32,
    total_rules: u64,
    total_uidless: u64,
    markup_markdown_docs: u64,
    max_rules_in_doc: usize,
    max_path: String,
    errors: BTreeMap<String, u32>,
}

#[tokio::test]
#[ignore]
async fn upstream_strictdoc_corpus_smoke() {
    let Ok(roots_str) = std::env::var("STRICTDOC_CORPUS") else {
        eprintln!(
            "skipping: STRICTDOC_CORPUS env var not set; see test file docs"
        );
        return;
    };

    let roots: Vec<PathBuf> = roots_str
        .split(':')
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();
    if roots.is_empty() {
        eprintln!("skipping: STRICTDOC_CORPUS is empty after splitting on ':'");
        return;
    }

    let mut overall = Counts::default();

    for root in &roots {
        if !root.exists() {
            eprintln!("skipping: {} does not exist", root.display());
            continue;
        }
        eprintln!("\n=== Corpus: {} ===", root.display());
        let mut corpus = Counts::default();
        walk_corpus(root, &mut corpus, &mut overall).await;
        report(&corpus);
    }

    eprintln!("\n=== Overall ===");
    report(&overall);

    assert!(
        overall.total > 100,
        "expected at least one meaningful corpus, got total={}; check STRICTDOC_CORPUS paths",
        overall.total
    );
    // 95% bridge success across whatever corpora the caller pointed at.
    assert!(
        (overall.ok as u64) * 20 >= (overall.total as u64) * 19,
        "expected ≥95% successful bridge calls, got {}/{}",
        overall.ok,
        overall.total
    );
}

async fn walk_corpus(root: &Path, corpus: &mut Counts, overall: &mut Counts) {
    for entry in WalkDir::new(root) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|e| e.to_str()) != Some("sdoc") {
            continue;
        }
        corpus.total += 1;
        overall.total += 1;
        let display = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .display()
            .to_string();
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if content.contains("MARKUP: Markdown") {
            corpus.markup_markdown_docs += 1;
            overall.markup_markdown_docs += 1;
        }

        match tracey::sdoc::extract_rules_from_sdoc(&content, &display).await {
            Ok(rules) => {
                corpus.ok += 1;
                overall.ok += 1;
                let n = rules.len();
                corpus.total_rules += n as u64;
                overall.total_rules += n as u64;
                if n > corpus.max_rules_in_doc {
                    corpus.max_rules_in_doc = n;
                    corpus.max_path = display.clone();
                }
                if n > overall.max_rules_in_doc {
                    overall.max_rules_in_doc = n;
                    overall.max_path = display.clone();
                }
            }
            Err(e) => {
                corpus.err += 1;
                overall.err += 1;
                let short =
                    e.to_string().lines().next().unwrap_or("").to_string();
                *corpus.errors.entry(short.clone()).or_insert(0) += 1;
                *overall.errors.entry(short).or_insert(0) += 1;
            }
        }

        if let Ok(doc) = strictdoc_parser::parse(&content) {
            for view in doc.requirements_flat() {
                if view.uid().is_none() {
                    corpus.total_uidless += 1;
                    overall.total_uidless += 1;
                }
            }
        }
    }
}

fn report(c: &Counts) {
    eprintln!("Total .sdoc files visited: {}", c.total);
    eprintln!("  Parsed OK by bridge:    {}", c.ok);
    eprintln!("  Failed in bridge:       {}", c.err);
    eprintln!("Total requirements via bridge: {}", c.total_rules);
    eprintln!(
        "Requirements without UID (skipped by bridge): {}",
        c.total_uidless
    );
    eprintln!(
        "Max requirements in a single doc: {} ({})",
        c.max_rules_in_doc, c.max_path
    );
    eprintln!("Docs with MARKUP: Markdown:  {}", c.markup_markdown_docs);
    if !c.errors.is_empty() {
        eprintln!("Error shapes:");
        for (msg, count) in &c.errors {
            eprintln!("  [{count}x] {msg}");
        }
    }
}
