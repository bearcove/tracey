//! Unified full-text search for source files and spec rules
//!
//! This module provides full-text search across:
//! - Source code content (line-by-line)
//! - Spec rules (rule IDs and text)
//!
//! When the `search` feature is disabled, it falls back to simple substring matching.

use facet::Facet;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Result type for unified search
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum ResultKind {
    /// Source code line
    Source,
    /// Spec rule
    Rule,
}

/// A unified search result
#[derive(Debug, Clone, Facet)]
pub struct SearchResult {
    /// Type of result
    pub kind: ResultKind,
    /// For Source: file path. For Rule: rule ID
    pub id: String,
    /// For Source: line number. For Rule: None (use 0)
    pub line: usize,
    /// The matching content (line content or rule text)
    pub content: String,
    /// HTML snippet with highlighted matches (uses `<mark>` tags)
    pub highlighted: String,
    /// Relevance score
    pub score: f32,
}

/// A rule to be indexed
#[derive(Debug, Clone)]
pub struct RuleEntry {
    pub id: String,
    /// Raw markdown source (without r[...] marker)
    pub raw: String,
}

/// Search index abstraction
pub trait SearchIndex: Send + Sync {
    /// Search for a query string, returning up to `limit` results
    fn search(&self, query: &str, limit: usize) -> Vec<SearchResult>;

    /// Check if search is available
    fn is_available(&self) -> bool {
        true
    }
}

// ============================================================================
// Tantivy implementation (when 'search' feature is enabled)
// ============================================================================

#[cfg(feature = "search")]
mod tantivy_impl {
    use super::*;
    use tantivy::collector::TopDocs;
    use tantivy::query::QueryParser;
    use tantivy::schema::{
        Field, INDEXED, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions,
        Value,
    };
    use tantivy::snippet::SnippetGenerator;
    use tantivy::{Index, IndexWriter, ReloadPolicy, doc};

    pub struct TantivyIndex {
        #[allow(dead_code)]
        index: Index,
        reader: tantivy::IndexReader,
        query_parser: QueryParser,
        schema: Schema,
        content_field: Field,
    }

    impl TantivyIndex {
        /// Build a new tantivy index from source files and rules
        pub fn build(
            project_root: &Path,
            files: &BTreeMap<PathBuf, String>,
            rules: &[RuleEntry],
        ) -> eyre::Result<Self> {
            // Define schema
            let mut schema_builder = Schema::builder();

            // Text options with stemming and positions (needed for snippet generation)
            let text_options = TextOptions::default().set_stored().set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("en_stem")
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            );

            // Text options for rule IDs - use default tokenizer for split parts
            let rule_id_options = TextOptions::default().set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("default")
                    .set_index_option(IndexRecordOption::WithFreqs),
            );

            // "kind" field: "source" or "rule"
            let kind_field = schema_builder.add_text_field("kind", STRING | STORED);
            // "id" field: file path for source, rule ID for rules
            let id_field = schema_builder.add_text_field("id", STRING | STORED);
            // "line" field: line number for source (0 for rules)
            let line_field = schema_builder.add_u64_field("line", INDEXED | STORED);
            // "content" field: the searchable text content with stemming
            let content_field = schema_builder.add_text_field("content", text_options);
            // "rule_id" field: searchable rule ID with dot-separated parts (not stored)
            let rule_id_field = schema_builder.add_text_field("rule_id", rule_id_options);
            let schema = schema_builder.build();

            // Create index in RAM (small enough for most projects)
            let index = Index::create_in_ram(schema.clone());

            // Use single thread to avoid worker thread panics on some platforms
            let mut index_writer: IndexWriter = index.writer_with_num_threads(1, 15_000_000)?;

            // Index source files with context lines
            const CONTEXT_LINES: usize = 2;

            for (path, content) in files {
                let relative = path
                    .strip_prefix(project_root)
                    .unwrap_or(path)
                    .display()
                    .to_string();

                let lines: Vec<&str> = content.lines().collect();

                // Index each line with surrounding context
                for (idx, line_content) in lines.iter().enumerate() {
                    let line_num = idx + 1; // 1-indexed
                    let trimmed = line_content.trim();

                    // Skip empty lines and very short lines
                    if trimmed.len() < 3 {
                        continue;
                    }

                    // Build content with context: 2 lines before + current + 2 lines after
                    let start = idx.saturating_sub(CONTEXT_LINES);
                    let end = (idx + CONTEXT_LINES + 1).min(lines.len());
                    let content_with_context = lines[start..end].join("\n");

                    index_writer.add_document(doc!(
                        kind_field => "source",
                        id_field => relative.clone(),
                        line_field => line_num as u64,
                        content_field => content_with_context,
                    ))?;
                }
            }

            // Index rules - use markdown text directly for indexing and snippet generation
            for rule in rules {
                // r[impl dashboard.search.render-requirements]
                let searchable_content = format!("{} {}", rule.id, rule.raw);

                index_writer.add_document(doc!(
                    kind_field => "rule",
                    id_field => rule.id.clone(),
                    line_field => 0u64,
                    content_field => searchable_content,
                    rule_id_field => rule.id.clone(),
                ))?;
            }

            index_writer.commit()?;

            let reader = index
                .reader_builder()
                .reload_policy(ReloadPolicy::Manual)
                .try_into()?;

            // Search both content and rule_id fields, with rule_id boosted higher
            let mut query_parser =
                QueryParser::for_index(&index, vec![content_field, rule_id_field]);
            query_parser.set_field_boost(rule_id_field, 5.0); // Boost rule ID matches

            Ok(Self {
                index,
                reader,
                query_parser,
                schema,
                content_field,
            })
        }
    }

    impl SearchIndex for TantivyIndex {
        fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
            let searcher = self.reader.searcher();

            // Parse the query - handle errors gracefully
            let parsed_query = match self.query_parser.parse_query(query) {
                Ok(q) => q,
                Err(_) => {
                    // Fall back to searching for literal terms
                    match self.query_parser.parse_query(&format!("\"{}\"", query)) {
                        Ok(q) => q,
                        Err(_) => return vec![],
                    }
                }
            };

            let top_docs = match searcher.search(&parsed_query, &TopDocs::with_limit(limit)) {
                Ok(docs) => docs,
                Err(_) => return vec![],
            };

            // Create snippet generator for highlighting
            let snippet_generator =
                match SnippetGenerator::create(&searcher, &*parsed_query, self.content_field) {
                    Ok(mut sg) => {
                        sg.set_max_num_chars(200);
                        Some(sg)
                    }
                    Err(_) => None,
                };

            let kind_field = self.schema.get_field("kind").unwrap();
            let id_field = self.schema.get_field("id").unwrap();
            let line_field = self.schema.get_field("line").unwrap();
            let content_field = self.schema.get_field("content").unwrap();

            let mut results: Vec<SearchResult> = top_docs
                .into_iter()
                .filter_map(|(score, doc_address)| {
                    let doc: tantivy::TantivyDocument = searcher.doc(doc_address).ok()?;

                    let kind_str = doc.get_first(kind_field)?.as_str()?;
                    let kind = match kind_str {
                        "source" => ResultKind::Source,
                        "rule" => ResultKind::Rule,
                        _ => return None,
                    };
                    let id = doc.get_first(id_field)?.as_str()?.to_string();
                    let line = doc.get_first(line_field)?.as_u64()? as usize;
                    let content = doc.get_first(content_field)?.as_str()?.to_string();

                    // r[impl dashboard.search.render-requirements]
                    // r[impl dashboard.search.requirement-styling]
                    // Generate highlighted snippet with <mark> tags
                    let highlighted = snippet_generator
                        .as_ref()
                        .map(|sg| {
                            let mut snippet = sg.snippet(&content);
                            snippet.set_snippet_prefix_postfix("<mark>", "</mark>");
                            snippet.to_html()
                        })
                        .unwrap_or_else(|| html_escape(&content));

                    Some(SearchResult {
                        kind,
                        id,
                        line,
                        content,
                        highlighted,
                        score,
                    })
                })
                .collect();

            // r[impl dashboard.search.prioritize-spec]
            // Sort results: rules first (by score), then source files (by score)
            results.sort_by(|a, b| match (a.kind, b.kind) {
                (ResultKind::Rule, ResultKind::Source) => std::cmp::Ordering::Less,
                (ResultKind::Source, ResultKind::Rule) => std::cmp::Ordering::Greater,
                _ => b
                    .score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal),
            });

            results
        }
    }
}

/// Simple HTML escape for fallback
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(feature = "search")]
pub use tantivy_impl::TantivyIndex;

// ============================================================================
// Fallback implementation (simple substring matching)
// ============================================================================

/// Entry in the simple index
struct SimpleEntry {
    kind: ResultKind,
    id: String,
    line: usize,
    content: String,
}

/// Simple substring search fallback when tantivy is not available
pub struct SimpleIndex {
    entries: Vec<SimpleEntry>,
}

impl SimpleIndex {
    /// Build a simple index from source files and rules
    pub fn build(
        project_root: &Path,
        files: &BTreeMap<PathBuf, String>,
        rules: &[RuleEntry],
    ) -> Self {
        let mut entries = Vec::new();

        // Index source files with context lines
        const CONTEXT_LINES: usize = 2;

        for (path, content) in files {
            let relative = path
                .strip_prefix(project_root)
                .unwrap_or(path)
                .display()
                .to_string();

            let lines: Vec<&str> = content.lines().collect();

            for (idx, line_content) in lines.iter().enumerate() {
                let line_num = idx + 1;
                if line_content.trim().len() >= 3 {
                    // Build content with context
                    let start = idx.saturating_sub(CONTEXT_LINES);
                    let end = (idx + CONTEXT_LINES + 1).min(lines.len());
                    let content_with_context = lines[start..end].join("\n");

                    entries.push(SimpleEntry {
                        kind: ResultKind::Source,
                        id: relative.clone(),
                        line: line_num,
                        content: content_with_context,
                    });
                }
            }
        }

        // Index rules - use markdown text directly
        for rule in rules {
            let searchable_content = format!("{} {}", rule.id, rule.raw);
            entries.push(SimpleEntry {
                kind: ResultKind::Rule,
                id: rule.id.clone(),
                line: 0,
                content: searchable_content,
            });
        }

        Self { entries }
    }
}

impl SearchIndex for SimpleIndex {
    fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let query_lower = query.to_lowercase();

        let mut results: Vec<SearchResult> = self
            .entries
            .iter()
            .filter(|e| e.content.to_lowercase().contains(&query_lower))
            .take(limit)
            .map(|e| {
                // Simple case-insensitive highlighting
                let highlighted = highlight_simple(&e.content, query);
                SearchResult {
                    kind: e.kind,
                    id: e.id.clone(),
                    line: e.line,
                    content: e.content.clone(),
                    highlighted,
                    score: 1.0,
                }
            })
            .collect();

        // r[impl dashboard.search.prioritize-spec]
        // Sort results: rules first, then source files
        results.sort_by(|a, b| match (a.kind, b.kind) {
            (ResultKind::Rule, ResultKind::Source) => std::cmp::Ordering::Less,
            (ResultKind::Source, ResultKind::Rule) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        });

        results
    }

    fn is_available(&self) -> bool {
        true
    }
}

/// Simple case-insensitive highlighting for fallback
fn highlight_simple(content: &str, query: &str) -> String {
    let content_lower = content.to_lowercase();
    let query_lower = query.to_lowercase();

    let mut result = String::new();
    let mut last_end = 0;

    for (start, _) in content_lower.match_indices(&query_lower) {
        // Append text before match (escaped)
        result.push_str(&html_escape(&content[last_end..start]));
        // Append highlighted match
        result.push_str("<mark>");
        result.push_str(&html_escape(&content[start..start + query.len()]));
        result.push_str("</mark>");
        last_end = start + query.len();
    }

    // Append remaining text
    result.push_str(&html_escape(&content[last_end..]));
    result
}

/// Build the appropriate search index based on feature flags
#[cfg(feature = "search")]
pub fn build_index(
    project_root: &Path,
    files: &BTreeMap<PathBuf, String>,
    rules: &[RuleEntry],
) -> Box<dyn SearchIndex> {
    match TantivyIndex::build(project_root, files, rules) {
        Ok(index) => Box::new(index),
        Err(e) => {
            eprintln!(
                "Warning: Failed to build tantivy index, falling back to simple search: {}",
                e
            );
            Box::new(SimpleIndex::build(project_root, files, rules))
        }
    }
}

#[cfg(not(feature = "search"))]
pub fn build_index(
    project_root: &Path,
    files: &BTreeMap<PathBuf, String>,
    rules: &[RuleEntry],
) -> Box<dyn SearchIndex> {
    Box::new(SimpleIndex::build(project_root, files, rules))
}
