//! Tests for marq markdown rendering behavior.

use marq::{RenderOptions, render};

/// Test that marq correctly handles markdown with `<mark>` tags injected.
/// This simulates what tantivy's SnippetGenerator produces for search highlighting.
#[tokio::test]
async fn test_marq_renders_markdown_with_mark_tags() {
    // Simulate tantivy snippet output: raw markdown with <mark> tags for highlighting
    let markdown_with_marks = r#"> Removing an input record <mark>MUST</mark> behave as follows:
>
> 1. If the record does not exist, return an error"#;

    let opts = RenderOptions::default();
    let result = render(markdown_with_marks, &opts).await;

    assert!(result.is_ok(), "marq::render failed: {:?}", result.err());

    let doc = result.unwrap();
    let html = &doc.html;

    // Print for inspection
    eprintln!("Input markdown:\n{}\n", markdown_with_marks);
    eprintln!("Output HTML:\n{}\n", html);

    // Check that the blockquote was rendered (not literal '>' characters)
    assert!(
        html.contains("<blockquote"),
        "Expected blockquote tag, got: {}",
        html
    );

    // Check that the <mark> tag survived
    assert!(
        html.contains("<mark>") && html.contains("</mark>"),
        "Expected <mark> tags to be preserved, got: {}",
        html
    );

    // Check that MUST is inside the mark tag
    assert!(
        html.contains("<mark>MUST</mark>"),
        "Expected MUST to be wrapped in mark tags, got: {}",
        html
    );
}

/// Test inline code with mark tags
#[tokio::test]
async fn test_marq_renders_inline_code_with_mark_tags() {
    let markdown = "Use the `<mark>foo</mark>` function to do things.";

    let opts = RenderOptions::default();
    let result = render(markdown, &opts).await;

    assert!(result.is_ok(), "marq::render failed: {:?}", result.err());

    let doc = result.unwrap();
    eprintln!("Input: {}\nOutput: {}", markdown, doc.html);
}

/// Test emphasis with mark tags
#[tokio::test]
async fn test_marq_renders_emphasis_with_mark_tags() {
    let markdown = "This is **<mark>important</mark>** text.";

    let opts = RenderOptions::default();
    let result = render(markdown, &opts).await;

    assert!(result.is_ok(), "marq::render failed: {:?}", result.err());

    let doc = result.unwrap();
    eprintln!("Input: {}\nOutput: {}", markdown, doc.html);

    // Strong tag should be present
    assert!(
        doc.html.contains("<strong>"),
        "Expected strong tag, got: {}",
        doc.html
    );
}

/// Test a real-world search result snippet
#[tokio::test]
async fn test_marq_renders_real_search_snippet() {
    // This is what a real search result might look like
    let markdown = r#"Within a view, revisions <mark>MUST</mark> <mark>form</mark> a total order consistent with the "happens-after" relation."#;

    let opts = RenderOptions::default();
    let result = render(markdown, &opts).await;

    assert!(result.is_ok(), "marq::render failed: {:?}", result.err());

    let doc = result.unwrap();
    eprintln!("Input: {}\nOutput: {}", markdown, doc.html);

    // Both mark tags should survive
    assert!(
        html_contains_mark_around(&doc.html, "MUST"),
        "Expected MUST in mark tags"
    );
    assert!(
        html_contains_mark_around(&doc.html, "form"),
        "Expected form in mark tags"
    );
}

fn html_contains_mark_around(html: &str, word: &str) -> bool {
    html.contains(&format!("<mark>{}</mark>", word))
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

    // NEW WAY: use highlighted() ranges and insert marks ourselves
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
        new_way.contains("<mark>MUST</mark>"),
        "Should have mark tags around MUST, got: {}",
        new_way
    );

    // Now render through marq
    let opts = marq::RenderOptions::default();
    let result = marq::render(&new_way, &opts).await.unwrap();

    eprintln!("Marq output:\n{}\n", result.html);

    // Should have blockquote now!
    assert!(
        result.html.contains("<blockquote"),
        "Should render blockquote, got: {}",
        result.html
    );
    assert!(
        result.html.contains("<mark>MUST</mark>"),
        "Should preserve mark tags, got: {}",
        result.html
    );
}

/// Test helper that mirrors the actual implementation
fn insert_mark_tags_test(content: &str, ranges: &[std::ops::Range<usize>]) -> String {
    if ranges.is_empty() {
        return content.to_string();
    }

    let mut result = String::with_capacity(content.len() + ranges.len() * 13);
    let mut last_end = 0;

    for range in ranges {
        if range.start > last_end {
            result.push_str(&content[last_end..range.start]);
        }
        result.push_str("<mark>");
        result.push_str(&content[range.start..range.end]);
        result.push_str("</mark>");
        last_end = range.end;
    }

    if last_end < content.len() {
        result.push_str(&content[last_end..]);
    }

    result
}
