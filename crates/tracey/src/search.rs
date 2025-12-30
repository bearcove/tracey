//! Full-text search for source files using tantivy
//!
//! This module provides full-text search across source code content.
//! When the `search` feature is disabled, it falls back to simple substring matching.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A search result with file path, line number, and context
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub file: String,
    pub line: usize,
    pub content: String,
    pub score: f32,
}

impl SearchResult {
    pub fn to_json(&self) -> String {
        let content_escaped = crate::serve::json_string(&self.content);
        format!(
            r#"{{"file":{},"line":{},"content":{},"score":{}}}"#,
            crate::serve::json_string(&self.file),
            self.line,
            content_escaped,
            self.score
        )
    }
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
    use tantivy::schema::*;
    use tantivy::{Index, IndexWriter, ReloadPolicy, doc};

    pub struct TantivyIndex {
        #[allow(dead_code)]
        index: Index,
        reader: tantivy::IndexReader,
        query_parser: QueryParser,
        schema: Schema,
    }

    impl TantivyIndex {
        /// Build a new tantivy index from source files
        pub fn build(project_root: &Path, files: &BTreeMap<PathBuf, String>) -> eyre::Result<Self> {
            // Define schema
            let mut schema_builder = Schema::builder();
            let path_field = schema_builder.add_text_field("path", STRING | STORED);
            let line_field = schema_builder.add_u64_field("line", INDEXED | STORED);
            let content_field = schema_builder.add_text_field("content", TEXT | STORED);
            let schema = schema_builder.build();

            // Create index in RAM (small enough for most projects)
            let index = Index::create_in_ram(schema.clone());

            let mut index_writer: IndexWriter = index.writer(50_000_000)?;

            for (path, content) in files {
                let relative = path
                    .strip_prefix(project_root)
                    .unwrap_or(path)
                    .display()
                    .to_string();

                // Index each line separately for line-level results
                for (line_num, line_content) in content.lines().enumerate() {
                    let line_num = line_num + 1; // 1-indexed
                    let trimmed = line_content.trim();

                    // Skip empty lines and very short lines
                    if trimmed.len() < 3 {
                        continue;
                    }

                    index_writer.add_document(doc!(
                        path_field => relative.clone(),
                        line_field => line_num as u64,
                        content_field => line_content,
                    ))?;
                }
            }

            index_writer.commit()?;

            let reader = index
                .reader_builder()
                .reload_policy(ReloadPolicy::Manual)
                .try_into()?;

            let query_parser = QueryParser::for_index(&index, vec![content_field]);

            Ok(Self {
                index,
                reader,
                query_parser,
                schema,
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

            let path_field = self.schema.get_field("path").unwrap();
            let line_field = self.schema.get_field("line").unwrap();
            let content_field = self.schema.get_field("content").unwrap();

            top_docs
                .into_iter()
                .filter_map(|(score, doc_address)| {
                    let doc: tantivy::TantivyDocument = searcher.doc(doc_address).ok()?;

                    let file = doc.get_first(path_field)?.as_str()?.to_string();
                    let line = doc.get_first(line_field)?.as_u64()? as usize;
                    let content = doc.get_first(content_field)?.as_str()?.to_string();

                    Some(SearchResult {
                        file,
                        line,
                        content,
                        score,
                    })
                })
                .collect()
        }
    }
}

#[cfg(feature = "search")]
pub use tantivy_impl::TantivyIndex;

// ============================================================================
// Fallback implementation (simple substring matching)
// ============================================================================

/// Simple substring search fallback when tantivy is not available
pub struct SimpleIndex {
    /// All lines indexed as (file, line_num, content)
    lines: Vec<(String, usize, String)>,
}

impl SimpleIndex {
    /// Build a simple index from source files
    pub fn build(project_root: &Path, files: &BTreeMap<PathBuf, String>) -> Self {
        let mut lines = Vec::new();

        for (path, content) in files {
            let relative = path
                .strip_prefix(project_root)
                .unwrap_or(path)
                .display()
                .to_string();

            for (line_num, line_content) in content.lines().enumerate() {
                let line_num = line_num + 1;
                if line_content.trim().len() >= 3 {
                    lines.push((relative.clone(), line_num, line_content.to_string()));
                }
            }
        }

        Self { lines }
    }
}

impl SearchIndex for SimpleIndex {
    fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let query_lower = query.to_lowercase();

        self.lines
            .iter()
            .filter(|(_, _, content)| content.to_lowercase().contains(&query_lower))
            .take(limit)
            .map(|(file, line, content)| SearchResult {
                file: file.clone(),
                line: *line,
                content: content.clone(),
                score: 1.0,
            })
            .collect()
    }

    fn is_available(&self) -> bool {
        true
    }
}

/// Build the appropriate search index based on feature flags
#[cfg(feature = "search")]
pub fn build_index(project_root: &Path, files: &BTreeMap<PathBuf, String>) -> Box<dyn SearchIndex> {
    match TantivyIndex::build(project_root, files) {
        Ok(index) => Box::new(index),
        Err(e) => {
            eprintln!(
                "Warning: Failed to build tantivy index, falling back to simple search: {}",
                e
            );
            Box::new(SimpleIndex::build(project_root, files))
        }
    }
}

#[cfg(not(feature = "search"))]
pub fn build_index(project_root: &Path, files: &BTreeMap<PathBuf, String>) -> Box<dyn SearchIndex> {
    Box::new(SimpleIndex::build(project_root, files))
}
