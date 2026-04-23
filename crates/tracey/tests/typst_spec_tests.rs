//! Integration tests for typst spec extraction.
//!
//! `fixtures-typst/spec.typ` mirrors the rule IDs in `fixtures/spec.md` so the
//! two backends can be compared directly.

use std::path::PathBuf;

use tracey::data::render_spec_content_for_impl;
use tracey::load_rules_from_glob;
use tracey_api::ApiSpecForward;
use tracey_core::SpecFormat;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn fixtures_typst_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures-typst")
}

#[tokio::test]
async fn extracts_rules() {
    let root = fixtures_typst_dir();
    let rules = load_rules_from_glob(&root, "spec.typ", true)
        .await
        .expect("typst extraction failed");

    assert_eq!(rules.len(), 8, "expected 8 requirements in spec.typ");

    for r in &rules {
        assert_eq!(r.format, SpecFormat::Typst);
        assert_eq!(r.prefix, "r");
        assert!(r.section.is_some(), "every req sits under a heading");
    }

    // Spot-check one.
    let login = rules
        .iter()
        .find(|r| r.def.id.base == "auth.login")
        .expect("auth.login present");
    assert_eq!(login.section_title.as_deref(), Some("Authentication"));
}

#[tokio::test]
async fn same_rules_as_markdown() {
    let md_root = fixtures_dir();
    let md_rules = load_rules_from_glob(&md_root, "spec.md", true)
        .await
        .expect("markdown extraction failed");

    let typ_root = fixtures_typst_dir();
    let typ_rules = load_rules_from_glob(&typ_root, "spec.typ", true)
        .await
        .expect("typst extraction failed");

    let mut md_ids: Vec<_> = md_rules.iter().map(|r| r.def.id.clone()).collect();
    let mut typ_ids: Vec<_> = typ_rules.iter().map(|r| r.def.id.clone()).collect();
    md_ids.sort();
    typ_ids.sort();

    assert_eq!(
        md_ids, typ_ids,
        "typst and markdown fixtures should define the same rule IDs"
    );
}

/// Regression: a markdown-only spec must produce the same outline slugs after
/// the per-format render partitioning as it did when everything went through a
/// single combined `marq::render` call.
#[tokio::test]
async fn markdown_only_outline_slugs_unchanged() {
    let root = fixtures_dir();
    let forward = ApiSpecForward {
        name: "test".to_string(),
        rules: vec![],
    };
    let (spec, _) = render_spec_content_for_impl(&root, &["spec.md".to_string()], "test", "rust", None, &forward)
        .await
        .expect("render failed");

    let slugs: Vec<&str> = spec.outline.iter().map(|e| e.slug.as_str()).collect();
    // marq builds hierarchical slugs (parent--child); these are the exact values
    // produced by the original single-render path.
    assert_eq!(
        slugs,
        vec![
            "test-specification",
            "test-specification--authentication",
            "test-specification--data-validation",
            "test-specification--error-handling",
        ],
        "markdown-only outline slugs must match the single-render baseline"
    );

    // Single markdown run -> single section.
    assert_eq!(spec.sections.len(), 1);
    assert_eq!(spec.sections[0].source_file, "spec.md");
}

/// Regression: marq's hierarchical heading ids join parent and child with
/// `--`, so `# R` + `## Design` yields `r--design`. The allocator must move
/// that out of the requirement-anchor namespace and the HTML patch must hit
/// only the `<h2>`, never the req container `<div>`.
#[tokio::test]
async fn markdown_heading_under_r_avoids_req_anchor_namespace() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("spec.md"),
        "# R\n\nr[design]\nbody\n\n## Design\n",
    )
    .unwrap();

    let forward = ApiSpecForward {
        name: "test".to_string(),
        rules: vec![],
    };
    let (spec, _) = render_spec_content_for_impl(
        tmp.path(),
        &["spec.md".to_string()],
        "test",
        "rust",
        None,
        &forward,
    )
    .await
    .expect("render must not panic on r--design heading slug");

    let html = &spec.sections[0].html;

    // The h2 was re-slugged out of the `r--` namespace.
    let h2_id = html
        .split("<h2 id=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("h2 with id present");
    assert!(
        !h2_id.starts_with("r--"),
        "h2 id {h2_id:?} must not enter the req-anchor namespace"
    );
    assert!(
        !html.contains("<h2 id=\"r--design\""),
        "raw r--design heading id must be rewritten"
    );

    // The req container kept its own anchor, distinct from the heading.
    let div_id = html
        .split("class=\"req-container")
        .nth(1)
        .and_then(|s| s.split("id=\"").nth(1))
        .and_then(|s| s.split('"').next())
        .expect("req container with id present");
    assert_ne!(div_id, h2_id, "req container and h2 must have distinct ids");

    // Outline carries the rewritten slug so the anchor link resolves.
    let h2_slug = spec
        .outline
        .iter()
        .find(|e| e.level == 2)
        .map(|e| e.slug.as_str())
        .expect("h2 in outline");
    assert_eq!(h2_slug, h2_id, "outline slug must match HTML anchor");
}

/// Full HTML rendering via the typst compiler: badges spliced in, heading IDs
/// injected, body extracted from `<body>`.
#[cfg(feature = "typst-spec")]
#[tokio::test]
async fn renders_html_with_badges() {
    let root = fixtures_typst_dir();
    let forward = ApiSpecForward {
        name: "test".to_string(),
        rules: vec![],
    };
    let (spec, deps) =
        render_spec_content_for_impl(&root, &["spec.typ".to_string()], "test", "rust", None, &forward)
            .await
            .expect("typst render failed");

    assert_eq!(spec.sections.len(), 1);
    let html = &spec.sections[0].html;

    // Compiler ran: no placeholder, no sentinel left behind.
    assert!(
        !html.contains("typst-placeholder"),
        "compiler output should replace the <pre> placeholder"
    );
    assert!(
        !html.contains("tracey-req"),
        "sentinel divs should be replaced by badge containers"
    );
    assert!(
        !html.contains("<body>"),
        "only body interior should be returned"
    );

    // Badge container spliced in (one per req; spot-check auth.login).
    assert!(html.contains("class=\"req-container req-uncovered\""));
    assert!(html.contains("id=\"r--auth.login\""));
    assert!(html.contains("data-req-id=\"auth.login\""));
    // Body content survives the splice.
    assert!(html.contains("Users MUST provide valid credentials"));
    // 8 reqs → 8 containers.
    assert_eq!(html.matches("class=\"req-container").count(), 8);

    // Heading IDs injected from tree-sitter slugs (`= Test Specification`
    // → h1, `== Authentication` → h2).
    assert!(html.contains("<h1 id=\"test-specification\">"));
    assert!(html.contains("<h2 id=\"authentication\">"));

    // Relative `#import "helper.typ"` resolved against the spec's directory.
    assert!(
        html.contains("helper content"),
        "binding from helper.typ should expand into output"
    );

    // The helper is reported as a project-relative dependency so the watcher
    // can rebuild when it changes even though it isn't a spec `include` glob
    // match itself.
    assert!(
        deps.contains(std::path::Path::new("helper.typ")),
        "deps should include project-relative helper.typ: {deps:?}"
    );
}

/// Low-level `render_display` smoke test: confirms the tracey-core entry point
/// works with a caller-supplied badge closure independent of the data layer.
#[cfg(feature = "typst-spec")]
#[tokio::test]
async fn render_display_direct() {
    let src = "= Title\n\n#req(\"x.y\")[Body text.]\n";
    let ctx = tracey_core::spec::typst::RenderCtx {
        badge_for: &|def| {
            (
                format!("<section data-id=\"{}\">", def.id),
                "</section>".to_string(),
            )
        },
    };
    let mut alloc = tracey_core::SlugAllocator::default();
    let mut deps = std::collections::HashSet::new();
    let doc = tracey_core::spec::typst::render_display(
        src,
        std::path::Path::new("test.typ"),
        None,
        &ctx,
        &mut alloc,
        &mut deps,
    )
    .await
    .expect("render_display failed");
    assert_eq!(doc.reqs.len(), 1);
    assert_eq!(doc.headings.len(), 1);
    assert!(doc.html.contains("<section data-id=\"x.y\">"));
    assert!(doc.html.contains("Body text."));
    assert!(doc.html.contains("<h1 id=\"title\">"));
}

/// Without the `typst-spec` feature, `render_display` errors gracefully.
#[cfg(not(feature = "typst-spec"))]
#[tokio::test]
async fn render_display_errors_without_feature() {
    let ctx = tracey_core::spec::typst::RenderCtx {
        badge_for: &|_| (String::new(), String::new()),
    };
    let mut alloc = tracey_core::SlugAllocator::default();
    let mut deps = std::collections::HashSet::<std::path::PathBuf>::new();
    let err = tracey_core::spec::typst::render_display(
        "= Title\n",
        std::path::Path::new("test.typ"),
        None,
        &ctx,
        &mut alloc,
        &mut deps,
    )
    .await
    .expect_err("should error without typst-spec");
    assert!(err.to_string().contains("typst-spec"));
}

/// Mixed-format spec: both rule sets surface in the outline, sections are in
/// declared order, and the typst section carries real rendered HTML.
#[cfg(feature = "typst-spec")]
#[tokio::test]
async fn mixed_format_spec() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("a.md"),
        "# Markdown Part\n\nr[mix.md]\nMarkdown body.\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("b.typ"),
        "= Typst Part\n\n#req(\"mix.typ\")[Typst body.]\n",
    )
    .unwrap();

    let forward = ApiSpecForward {
        name: "mix".to_string(),
        rules: vec![],
    };
    let (spec, _) = render_spec_content_for_impl(
        tmp.path(),
        &["*.md".to_string(), "*.typ".to_string()],
        "mix",
        "rust",
        None,
        &forward,
    )
    .await
    .expect("render failed");

    assert_eq!(spec.sections.len(), 2);
    assert_eq!(spec.sections[0].source_file, "a.md");
    assert_eq!(spec.sections[1].source_file, "b.typ");

    // Markdown section rendered via marq.
    assert!(spec.sections[0].html.contains("Markdown body."));
    // Typst section rendered via the compiler with badge container.
    assert!(spec.sections[1].html.contains("req-container"));
    assert!(spec.sections[1].html.contains("Typst body."));
    assert!(spec.sections[1].html.contains("<h1 id=\"typst-part\">"));

    // Both rules surface in the outline (under their respective headings).
    let slugs: Vec<&str> = spec.outline.iter().map(|e| e.slug.as_str()).collect();
    assert!(slugs.contains(&"markdown-part"));
    assert!(slugs.contains(&"typst-part"));
}

/// Mixed-format specs render in separate runs; colliding heading titles across
/// runs must get unique slugs in the merged outline.
#[tokio::test]
async fn mixed_format_outline_dedups_heading_slugs() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("a.md"),
        "# Shared\n\nr[mix.a]\nMarkdown body.\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("b.typ"),
        "= Shared\n\n#req(\"mix.b\")[Typst body.]\n",
    )
    .unwrap();

    let forward = ApiSpecForward {
        name: "mix".to_string(),
        rules: vec![],
    };
    let (spec, _) = render_spec_content_for_impl(
        tmp.path(),
        &["*.md".to_string(), "*.typ".to_string()],
        "mix",
        "rust",
        None,
        &forward,
    )
    .await
    .expect("render failed");

    // One section per run (md run = 1 file, typ file = 1 section).
    assert_eq!(spec.sections.len(), 2);
    assert_eq!(spec.sections[0].source_file, "a.md");
    assert_eq!(spec.sections[1].source_file, "b.typ");

    let slugs: Vec<&str> = spec.outline.iter().map(|e| e.slug.as_str()).collect();
    assert_eq!(
        slugs,
        vec!["shared", "shared-2"],
        "colliding heading slugs across format runs must be deduplicated"
    );
    // The deduplicated slug must also land in the rendered HTML, not just the
    // outline, so the anchor link actually resolves. (Without `typst-spec` the
    // typst section is a `<pre>` placeholder with no heading anchors at all.)
    #[cfg(feature = "typst-spec")]
    assert!(
        spec.sections[1].html.contains(r#"id="shared-2""#),
        "typst section HTML must carry the deduplicated anchor: {}",
        spec.sections[1].html
    );
}
