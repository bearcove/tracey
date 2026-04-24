# Plan: Replace Hand-Rolled AsciiDoc Parser with `asciidork-parser`

Replace the 921-line hand-rolled parser in `crates/tracey-core/src/spec/asciidoc.rs` with
`asciidork-parser` + `asciidork-dr-html-backend`. The public API surface of `asciidoc.rs`
does not change; the refactor is entirely internal to that file.

The goal is to reach the same relationship that Markdown has with `marq`/`pulldown-cmark`:
a thin adapter layer over a well-maintained external parser, rather than a bespoke
line-scanner that must be kept in sync with the AsciiDoc spec by hand.

---

## 1. Current architecture

```
asciidoc.rs (hand-rolled, 921 lines)
  parse()          — line-by-line scanner → marq::Document
  render_display() — same scanner, badge injection via RenderCtx closure
  parse_weight()   — scans :weight: attribute + YAML/TOML frontmatter
  diff_inline()    — delegates to marq::diff_markdown_inline (unchanged)
  extract_marker_prefix() / id_range_in_marker() — shared helpers (unchanged)
```

What the scanner handles today: headings, paragraphs, quote blocks (`____`), listing
(`----`), literal (`....`), passthrough (`++++`), line comments (`//`), block comments
(`////`), document attributes, YAML/TOML frontmatter, inline backtick spans.

What it does **not** handle: tables, admonitions, callouts, definition lists, ordered lists,
nested blocks, includes, conditionals, STEM, source highlighting, most inline formatting.

---

## 2. Target architecture

```
asciidoc.rs (thin adapter, ~200 lines)
  parse()          — asciidork-parser AST → walk → marq::Document  (cheap path)
  render_display() — asciidork-parser AST → TraceyAdocBackend → marq::Document
  parse_weight()   — read from asciidork AST document header attributes
  diff_inline()    — unchanged (delegates to marq)
  extract_marker_prefix() / id_range_in_marker() — unchanged (shared helpers)

asciidoc/backend.rs (new)
  TraceyAdocBackend — wraps asciidork-dr-html-backend, intercepts requirement
                      paragraphs/quote-blocks to inject <div> wrappers and badges
```

The `marq::Document` type (aliased as `SpecDoc` in `spec/mod.rs`) is unchanged — it
remains the shared output type for all formats, populated from the asciidork AST walk.

---

## 3. Dependency changes

### `crates/tracey-core/Cargo.toml`

Add to `[dependencies]`:

```toml
asciidork-parser  = { version = "0.38", optional = true }
asciidork-ast     = { version = "0.38", optional = true }
asciidork-backend = { version = "0.38", optional = true }
asciidork-dr-html-backend = { version = "0.38", optional = true }
```

Add a Cargo feature (mirroring the precedent for future heavy renderers):

```toml
[features]
asciidoc-spec = [
    "dep:asciidork-parser",
    "dep:asciidork-ast",
    "dep:asciidork-backend",
    "dep:asciidork-dr-html-backend",
]
```

Enable it by default in the binary:

```toml
# crates/tracey/Cargo.toml
tracey-core = { ..., features = ["asciidoc-spec"] }
```

**Note:** Verify whether `asciidork-parser` is `no_std`-compatible or requires `std`.
If bumpalo arena types appear in the public API (they do — the Parser and AST nodes are
arena-allocated), the library consumer must bring in `bumpalo` transitively. This is
handled automatically via Cargo's dependency resolution.

---

## 4. Understanding where requirement markers appear in the asciidork AST

Before writing the adapter, confirm the AST shapes for the two marker syntaxes:

**Paragraph form:**
```adoc
r[auth.login]
The system must validate tokens.
```
Expected asciidork AST: a `Block::Paragraph` whose first `InlineNode` is a text node
whose content starts with `r[auth.login]`.

**Quote-block form:**
```adoc
____
r[auth.login]
The system must validate tokens.
____
```
Expected asciidork AST: a `Block::Quote` (or similar delimited block) containing a
`Block::Paragraph` with the same leading marker.

**Verify this in commit 1** by writing a small test that prints the AST of each form
before writing any extraction logic. The exact type names (`Block`, `InlineNode`,
`InlineContent`, etc.) are in `asciidork-ast` — consult its docs at
`https://docs.rs/asciidork-ast`.

---

## 5. Implementation plan (4 commits)

### Commit 1 — Wire dependencies, add AST shape tests

- Add the four `asciidork-*` crates to `tracey-core/Cargo.toml` under `asciidoc-spec`
  feature flag.
- Add `asciidoc/mod.rs` sub-module structure:
  ```
  crates/tracey-core/src/spec/asciidoc/
    mod.rs        — current public API (parse, render_display, etc.) wired to new internals
    ast_walk.rs   — requirement/heading/span extraction from the asciidork AST
    backend.rs    — TraceyAdocBackend
  ```
  Keep the existing `asciidoc.rs` as a flat file until the new code is ready, then
  replace it with the directory form in the same commit.
- Add `#[cfg(feature = "asciidoc-spec")]` guards. When the feature is off, `parse()`
  returns a `<pre class="asciidoc-placeholder">` stub (mirrors the pattern established
  for Typst in `asciidoc-plan.md`).
- Write AST shape tests:
  ```rust
  #[cfg(all(test, feature = "asciidoc-spec"))]
  mod ast_shape_tests {
      // Parse paragraph-form and quote-block-form markers.
      // Walk the AST and print/assert the Block variant and InlineNode structure.
      // These tests exist to validate assumptions before extraction code is written.
  }
  ```
- **Nothing in the existing public API changes.** All existing tests continue to pass
  because `asciidoc.rs` still exists and is unchanged.

---

### Commit 2 — `ast_walk.rs`: requirement and heading extraction

Implement `ast_walk.rs` with a function:

```rust
pub struct AstWalkResult {
    pub reqs: Vec<ReqDefinition>,
    pub headings: Vec<Heading>,
    pub elements: Vec<DocElement>,
    pub inline_code_spans: Vec<InlineCodeSpan>,
    pub weight: i32,
}

pub fn walk(doc: &asciidork_ast::Document, source: &str, alloc: &mut SlugAllocator)
    -> eyre::Result<AstWalkResult>
```

**Heading extraction:**
- Walk `Block::Section` nodes (or equivalent in asciidork). Each section has a title
  and a nesting level.
- Call `alloc.alloc(&marq::slugify(&title))` to get a globally unique slug — same as
  the current hand-rolled code.
- Build `marq::Heading { title, id, level, line }`.

**Requirement extraction:**
- For each `Block::Paragraph` (and the first paragraph inside a `Block::Quote` /
  delimited quote block):
  - Collect the inline text of the first inline node.
  - Call the existing `parse_req_leading_marker(text)` helper (moved from the current
    `asciidoc.rs` — it is format-agnostic text logic, not parsing logic).
  - If it matches: parse the ID via `parse_req_marker_inner`, compute `anchor_id`,
    build `ReqDefinition`.
  - If it does not match: build a `DocElement::Paragraph`.
- Source spans: use the source location info from the asciidork AST nodes (they carry
  byte offsets) rather than the hand-rolled `find_line_offset_in_content` heuristic.
  Confirm asciidork exposes source locations — if not, fall back to searching `source`
  for the marker text.

**`parse_weight` extraction:**
- Read from the parsed document's header attributes (`document.header.attributes`
  or equivalent). Look for the `weight` attribute value. Fall back to scanning for
  YAML/TOML frontmatter using `marq::parse_frontmatter` as now.

**Inline code span extraction:**
- Walk inline nodes for monospace/backtick spans. In the asciidork AST these are
  `InlineNode::Mono` (or `InlineContent::Mono`) with source spans. Use those directly
  instead of the hand-rolled backtick scanner.

Write unit tests in `ast_walk.rs` covering the same cases as the current `asciidoc.rs`
inline tests:
- Heading extraction (level, title, slug)
- Paragraph-form requirement marker
- Quote-block-form requirement marker
- Inline-marker not treated as requirement (the marker is not at the leading position)
- Duplicate detection (same-file error)
- `parse_weight` from document attribute
- `parse_weight` from YAML frontmatter

---

### Commit 3 — `backend.rs`: `TraceyAdocBackend` + update `mod.rs`

**`backend.rs`**

Implement `TraceyAdocBackend` by implementing the `asciidork_backend::Backend` trait.
The backend wraps `asciidork_dr_html_backend::HtmlBackend` and intercepts blocks that
contain requirement markers.

```rust
pub struct TraceyAdocBackend<'a> {
    inner: asciidork_dr_html_backend::HtmlBackend,
    req_renderer: &'a dyn Fn(&ReqDefinition) -> (String, String),
    reqs: Vec<ReqDefinition>,        // populated during traversal
    headings: Vec<Heading>,
    elements: Vec<DocElement>,
    slug_alloc: &'a mut SlugAllocator,
    html: String,
}
```

The `Backend` trait has methods called for each AST node. For blocks that match
requirement paragraphs:
1. Emit the `open_html` from `req_renderer`.
2. Delegate the block's body rendering to `inner` (the dr-html backend).
3. Emit the `close_html`.

For all other blocks: delegate to `inner` unchanged.

This is structurally analogous to how marq's `TraceyRuleHandler` intercepts requirement
blocks during pulldown-cmark event processing. The key difference is that asciidork uses
a visitor/callback model over an AST rather than an event stream; the same intent applies.

**`mod.rs` — update `parse()` and `render_display()`**

`parse()` (cheap path, no badge injection):
```rust
pub(super) async fn parse(content: &str) -> eyre::Result<SpecDoc> {
    #[cfg(feature = "asciidoc-spec")]
    {
        let arena = bumpalo::Bump::new();
        let ast = asciidork_parser::Parser::new(content, &arena).parse()?;
        let mut alloc = SlugAllocator::new();
        let walk = ast_walk::walk(&ast, content, &mut alloc)?;
        let req_renderer = |req: &ReqDefinition| uncovered_div_html(req);
        let mut backend = backend::TraceyAdocBackend::new_cheap(&req_renderer, &mut alloc);
        backend.render(&ast)?;
        Ok(walk_and_backend_to_doc(walk, backend))
    }
    #[cfg(not(feature = "asciidoc-spec"))]
    {
        Ok(placeholder_doc(content))
    }
}
```

`render_display()` (rich path, badge injection):
```rust
pub async fn render_display(
    content: &str,
    _source_path: &Path,
    ctx: &RenderCtx<'_>,
    alloc: &mut SlugAllocator,
    _deps: &mut HashSet<PathBuf>,
) -> eyre::Result<SpecDoc> {
    #[cfg(feature = "asciidoc-spec")]
    {
        let arena = bumpalo::Bump::new();
        let ast = asciidork_parser::Parser::new(content, &arena).parse()?;
        let walk = ast_walk::walk(&ast, content, alloc)?;
        let mut backend = backend::TraceyAdocBackend::new_display(ctx.badge_for, alloc);
        backend.render(&ast)?;
        Ok(walk_and_backend_to_doc(walk, backend))
    }
    #[cfg(not(feature = "asciidoc-spec"))]
    {
        Ok(placeholder_doc(content))
    }
}
```

`walk_and_backend_to_doc` merges the `AstWalkResult` and backend output into a
`marq::Document`:
```rust
fn walk_and_backend_to_doc(walk: AstWalkResult, backend: TraceyAdocBackend) -> SpecDoc {
    marq::Document {
        raw_metadata: None,
        metadata_format: None,
        frontmatter: None,
        html: backend.html,
        headings: walk.headings,
        reqs: walk.reqs,
        code_samples: Vec::new(),
        elements: walk.elements,
        head_injections: Vec::new(),
        inline_code_spans: walk.inline_code_spans,
    }
}
```

**Delete the hand-rolled parser code.** `parse_inner`, `flush_paragraph`,
`flush_quote_block`, `render_paragraph_html`, `render_blockquote_html`, `BlockState`,
`ParseOutput`, `collect_inline_code_spans`, `heading_level`, `find_line_offset_in_content`
all go away. The helpers that survive are:
- `parse_req_leading_marker` — moved to `ast_walk.rs`
- `parse_req_marker_inner` — moved to `ast_walk.rs`
- `html_escape` — removed (asciidork's backend handles escaping)
- `strip_frontmatter` / `find_frontmatter_end` — removed (handled by asciidork or by
  `marq::parse_frontmatter` in `parse_weight`)

The public-facing helpers (`diff_inline`, `parse_weight`, `extract_marker_prefix`,
`id_range_in_marker`, `RenderCtx`) are kept as-is. Their signatures do not change.

---

### Commit 4 — Tests, update existing fixtures

- Update the unit tests in `asciidoc/mod.rs` (currently in `asciidoc.rs`) to reflect
  the new implementation paths.
- Add tests covering things the hand-rolled parser could not handle:
  - Table rendering (no longer garbled)
  - Admonition blocks (`NOTE:`, `TIP:`, etc.)
  - Ordered and unordered lists
  - Inline formatting (bold, italic, monospace)
  - Nested quote blocks
  - Requirements inside list items (edge case — confirm expected behavior: should they
    be recognized as requirements or treated as list-item text? Mirror the Markdown rule:
    only column-0 / direct-block-child markers count.)
- Update `crates/tracey/tests/fixtures-asciidoc/spec.adoc` to include a table and an
  admonition block, verifying they render correctly in the integration test.
- Run `cargo test --test asciidoc_spec_tests` and `cargo test --test watcher_tests`.
- Verify `cargo clippy --all-targets --all-features` is clean.

---

## 6. Open questions to resolve in commit 1

1. **asciidork AST node names.** The exact names for `Block::Paragraph`,
   `Block::Quote`, `InlineNode`, and source span accessors must be confirmed from
   `asciidork-ast`'s docs. The plan above uses plausible names; adjust during
   implementation.

2. **Backend trait shape.** Confirm whether `asciidork-backend::Backend` uses a
   visitor/callback model (per-node methods) or a single `render(doc) -> String` call.
   If it is a single-call renderer, `TraceyAdocBackend` will need to pre-walk the AST
   for requirements and then post-process the rendered HTML to inject wrappers — less
   clean but workable.

3. **bumpalo arena lifetime.** The asciidork parser is arena-allocated. The arena must
   outlive the AST. Because `parse()` and `render_display()` are `async fn`, confirm
   that `bumpalo::Bump` can be created inside an async context (it is `!Send`, which
   may interact with async executors). If necessary, wrap the parse + walk in a
   `tokio::task::spawn_blocking` call.

4. **Source span availability.** If asciidork AST nodes do not carry byte offsets,
   `ReqDefinition::span` and `marker_span` cannot be populated accurately. The fallback
   is the existing `find_line_offset_in_content` heuristic. Prefer native offsets if
   available.

5. **`parse_weight` attribute name.** Confirm that asciidork exposes parsed document
   attributes in a form that allows reading arbitrary user-defined attributes (`:weight:
   10` is a custom attribute, not a built-in AsciiDoc one). If not exposed, fall back to
   scanning the raw content with `marq::parse_frontmatter` as today.

---

## 7. What does not change

| Thing | Status |
|---|---|
| `spec/mod.rs` public API | Unchanged |
| `SpecFormat::AsciiDoc` variant | Unchanged |
| `SPEC_EXTENSIONS` list | Unchanged |
| `diff_inline` | Unchanged (delegates to marq) |
| `extract_marker_prefix` / `id_range_in_marker` | Unchanged (shared helpers) |
| `RenderCtx` shape | Unchanged |
| `data.rs` `load_spec_content` | Unchanged |
| `lib.rs` extraction loop | Unchanged |
| `daemon/service.rs` | Unchanged |
| `search.rs` | Unchanged |
| All integration tests in `asciidoc_spec_tests.rs` | Must continue to pass |
| `REQ_ANCHOR_PREFIX = "r--"` contract | Unchanged |
| `SlugAllocator` threading | Unchanged — still passed into `render_display` |

---

## 8. Risks

**asciidork's requirement-marker handling.** `asciidork-parser` does not know about
tracey's `r[id]` syntax. If asciidork treats the leading `r[auth.login]` line as
something other than a plain paragraph (e.g., an inline macro, an anchor, or an
attribute reference), requirement extraction will fail silently. Verify in commit 1's
AST shape tests.

**bumpalo + async.** Arena-allocated parsers can be awkward in async contexts. If
`bumpalo::Bump` is `!Send`, `parse()` and `render_display()` may need to do the parse
work on a blocking thread and return only the `Send`-safe `marq::Document`.

**HTML output divergence.** `asciidork-dr-html-backend` produces Asciidoctor-compatible
HTML5 with its own class names and structure. Existing CSS in the tracey dashboard that
styles spec-content HTML may need updates to match the new element shapes (e.g., section
wrapper divs, admonition block structure). Audit the dashboard CSS after commit 3.

**Version stability.** `asciidork-parser` is at v0.38 with a fast release cadence. Pin
an exact version initially (`= "0.38"`) to avoid surprise breakage; relax to `"0"` once
the integration is stable.
