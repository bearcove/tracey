//! Tests for marq markdown rendering behavior.

use marq::{RenderOptions, render};
use tracey::search::{MARK_CLOSE, MARK_OPEN, pua_to_mark};

/// Test that marq passes PUA highlight sentinels through untouched, so the
/// post-render `pua_to_mark` step yields proper `<mark>` elements. This
/// simulates the search-result rendering pipeline for markdown rule bodies.
#[tokio::test]
async fn test_marq_passes_pua_sentinels_through_blockquote() {
    let markdown_with_marks = format!(
        "> Removing an input record {MARK_OPEN}MUST{MARK_CLOSE} behave as follows:\n\
         >\n\
         > 1. If the record does not exist, return an error"
    );

    let opts = RenderOptions::default();
    let doc = render(&markdown_with_marks, &opts).await.expect("render");
    let html = pua_to_mark(&doc.html);

    eprintln!("Input markdown:\n{}\n", markdown_with_marks);
    eprintln!("Output HTML:\n{}\n", html);

    assert!(
        html.contains("<blockquote"),
        "Expected blockquote tag, got: {html}"
    );
    assert!(
        html.contains("<mark>MUST</mark>"),
        "Expected MUST wrapped in mark tags, got: {html}"
    );
}

/// PUA sentinels inside `**emphasis**` survive marq and convert cleanly.
#[tokio::test]
async fn test_marq_passes_pua_sentinels_through_emphasis() {
    let markdown = format!("This is **{MARK_OPEN}important{MARK_CLOSE}** text.");

    let opts = RenderOptions::default();
    let doc = render(&markdown, &opts).await.expect("render");
    let html = pua_to_mark(&doc.html);

    eprintln!("Input: {}\nOutput: {}", markdown, html);
    assert!(html.contains("<strong>"), "Expected strong tag, got: {html}");
    assert!(
        html.contains("<mark>important</mark>"),
        "Expected mark inside strong, got: {html}"
    );
}

/// Multiple highlight runs in one snippet.
#[tokio::test]
async fn test_marq_real_search_snippet() {
    let markdown = format!(
        "Within a view, revisions {MARK_OPEN}MUST{MARK_CLOSE} {MARK_OPEN}form{MARK_CLOSE} \
         a total order consistent with the \"happens-after\" relation."
    );

    let opts = RenderOptions::default();
    let doc = render(&markdown, &opts).await.expect("render");
    let html = pua_to_mark(&doc.html);

    eprintln!("Input: {}\nOutput: {}", markdown, html);
    assert!(html.contains("<mark>MUST</mark>"), "got: {html}");
    assert!(html.contains("<mark>form</mark>"), "got: {html}");
}

/// Test what tantivy's SnippetGenerator actually produces
#[tokio::test]
async fn test_tantivy_snippet_output() {
    use tantivy::collector::TopDocs;
    use tantivy::query::QueryParser;
    use tantivy::schema::{STORED, Schema, TEXT, Value};
    use tantivy::snippet::SnippetGenerator;
    use tantivy::{Index, doc};

    // Create a simple schema
    let mut schema_builder = Schema::builder();
    let content_field = schema_builder.add_text_field("content", TEXT | STORED);
    let schema = schema_builder.build();

    // Create index in RAM
    let index = Index::create_in_ram(schema);
    let mut index_writer = index.writer(15_000_000).unwrap();

    // Add a document with markdown containing blockquote
    let markdown = r#"> Removing an input record MUST behave as follows:
>
> 1. If the record does not exist, return an error"#;

    index_writer
        .add_document(doc!(content_field => markdown))
        .unwrap();
    index_writer.commit().unwrap();

    let reader = index.reader().unwrap();
    let searcher = reader.searcher();

    // Search for "MUST"
    let query_parser = QueryParser::for_index(&index, vec![content_field]);
    let query = query_parser.parse_query("MUST").unwrap();

    let top_docs = searcher.search(&query, &TopDocs::with_limit(1)).unwrap();
    assert!(!top_docs.is_empty(), "Should find the document");

    let (_, doc_address) = &top_docs[0];
    let doc: tantivy::TantivyDocument = searcher.doc(*doc_address).unwrap();
    let content = doc.get_first(content_field).unwrap().as_str().unwrap();

    // Create snippet generator
    let mut sg = SnippetGenerator::create(&searcher, &*query, content_field).unwrap();
    sg.set_max_num_chars(500);

    let snippet = sg.snippet(content);

    // OLD WAY: to_html() escapes the content
    let mut snippet_clone = sg.snippet(content);
    snippet_clone.set_snippet_prefix_postfix("<mark>", "</mark>");
    let old_way = snippet_clone.to_html();

    eprintln!("Original markdown:\n{}\n", markdown);
    eprintln!("OLD (to_html, escapes content):\n{}\n", old_way);

    // NEW WAY: use highlighted() ranges and insert PUA sentinels ourselves
    let ranges = snippet.highlighted();
    let new_way = insert_mark_tags_test(content, ranges);

    eprintln!("NEW (insert_mark_tags, preserves markdown):\n{}\n", new_way);

    // Verify the new way preserves the > character
    assert!(
        new_way.contains("> Removing"),
        "Should preserve literal > for blockquote, got: {}",
        new_way
    );
    assert!(
        new_way.contains(&format!("{MARK_OPEN}MUST{MARK_CLOSE}")),
        "Should have PUA sentinels around MUST, got: {}",
        new_way
    );

    // Now render through marq, then swap sentinels for real <mark> tags.
    let opts = marq::RenderOptions::default();
    let rendered = marq::render(&new_way, &opts).await.unwrap();
    let result_html = pua_to_mark(&rendered.html);

    eprintln!("Marq output:\n{}\n", result_html);

    // Should have blockquote now!
    assert!(
        result_html.contains("<blockquote"),
        "Should render blockquote, got: {}",
        result_html
    );
    assert!(
        result_html.contains("<mark>MUST</mark>"),
        "Should yield mark tags after pua_to_mark, got: {}",
        result_html
    );
}

/// Test helper that mirrors `crate::search::tantivy_impl::insert_mark_tags`
/// (which is private to the `search` module).
fn insert_mark_tags_test(content: &str, ranges: &[std::ops::Range<usize>]) -> String {
    if ranges.is_empty() {
        return content.to_string();
    }

    let mut result = String::with_capacity(content.len() + ranges.len() * 6);
    let mut last_end = 0;

    for range in ranges {
        if range.start > last_end {
            result.push_str(&content[last_end..range.start]);
        }
        result.push(MARK_OPEN);
        result.push_str(&content[range.start..range.end]);
        result.push(MARK_CLOSE);
        last_end = range.end;
    }

    if last_end < content.len() {
        result.push_str(&content[last_end..]);
    }

    result
}
