# Plan: Add AsciiDoc support to Tracey

Add `.adoc` (AsciiDoc) as a second supported **spec-file** format alongside
Markdown. Code-file parsing (`prefix[verb req.id]` references inside source
code comments) is orthogonal and does not change.

**Key context**: PR #185 (Typst support; open, not yet merged) has already
designed and landed — in its branch — the exact multi-format abstraction this
change needs. This plan is deliberately structured to **sit on top of PR #185's
abstraction** so the two features are symmetric and trivially rebaseable
against each other. The AsciiDoc implementer should:

1. Rebase on top of `main`.
2. If PR #185 is already merged, just use the `tracey_core::spec` API.
3. If PR #185 is **not** yet merged, replicate the same API surface verbatim
   (not a similar one — identical names, identical signatures). That way the
   maintainer can merge either PR first and the other rebases cleanly, and
   the author of PR #185 does not have to re-review a parallel abstraction.

Every name and helper below is quoted from PR #185 by design. Stick to them.

---

## 1. The PR #185 abstraction — what we depend on

PR #185 introduces `crates/tracey-core/src/spec/mod.rs` with this public API
(reproduce exactly if #185 is not yet merged):

```rust
// Format classification
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpecFormat { Markdown, Typst /* + AsciiDoc (this PR) */ }

impl SpecFormat {
    pub fn from_path(p: &Path) -> Option<Self>;
    pub fn from_ext(ext: &OsStr) -> Option<Self>;
}

pub const SPEC_EXTENSIONS: &[&str] = &["md", "markdown", "typ"
                                       /* + "adoc", "asciidoc", "asc" */];
pub fn is_spec_extension(ext: &OsStr) -> bool;

// Shared document shape — marq::Document's fields are public, so every
// backend constructs one directly.
pub type SpecDoc = marq::Document;
pub use marq::{DocElement, InlineCodeSpan, ReqDefinition,
               RuleId as SpecRuleId, SourceSpan};

// Format-dispatched operations (free functions, not a trait — PR #185's
// stated rationale is that there are only a handful of call sites, every
// one has a Path, and async-trait boxing is avoided).
pub async fn parse_spec(fmt: SpecFormat, content: &str) -> eyre::Result<SpecDoc>;
pub fn diff_inline(fmt: SpecFormat, old: &str, new: &str) -> Option<String>;
pub fn parse_weight(fmt: SpecFormat, content: &str) -> i32;
pub fn extract_marker_prefix(fmt: SpecFormat, content: &str, span: SourceSpan)
    -> Option<String>;
pub fn id_range_in_marker(fmt: SpecFormat, marker_str: &str)
    -> eyre::Result<Range<usize>>;
pub fn rewrite_marker(marker_str: &str, id_range: Range<usize>,
                      base: &str, new_ver: u32) -> eyre::Result<String>;

// Anchor contract (both formats MUST emit this shape in ReqDefinition::anchor_id)
pub const REQ_ANCHOR_PREFIX: &str = "r--";
pub fn req_anchor_id(id: &str) -> String;
pub fn req_anchor_to_id(anchor: &str) -> Option<&str>;

// Cross-file heading-slug allocator — threaded through the whole render so
// heading anchors from different files / different formats never collide.
pub struct SlugAllocator { /* … */ }
impl SlugAllocator {
    pub fn alloc(&mut self, base: &str) -> String;
}
```

Per-format submodules (`spec/markdown.rs`, `spec/typst.rs`) implement the
private dispatch targets: `parse`, `diff_inline`, `parse_weight`,
`extract_marker_prefix`, `id_range_in_marker`. Heavy backends gate their
compiler behind a Cargo feature (`typst-spec` in PR #185).

PR #185 also extracts `rule_coverage_badge_html(rule, coverage, source_file,
spec_name, impl_name) -> (String, String)` in `crates/tracey/src/data.rs` so
non-marq backends can render coverage-aware requirement containers without
duplicating 70 lines of badge HTML.

`ExtractedRule` in `crates/tracey/src/lib.rs` grows a `pub format: SpecFormat`
field. `search::RuleEntry` and `search::SearchResult` gain format too, and
the search index stores "markdown" / "typst" strings.

---

## 2. What this plan adds

Introduce `SpecFormat::AsciiDoc` as a third variant and implement
`spec/asciidoc.rs`, the analogue of `spec/typst.rs`. Because PR #185 declared
`SpecFormat` as `#[non_exhaustive]` and left a fallthrough arm in
`load_spec_content`, the integration seam is narrow and clearly signposted.

### 2.1 Syntax for requirements in AsciiDoc

Mirror Markdown as closely as AsciiDoc's grammar allows. Two variants:

- **Paragraph form** — marker at column 0 as a standalone paragraph:

  ```adoc
  r[auth.token.validation]
  The system must validate tokens before granting access.
  ```

- **Quoted-block form** — AsciiDoc's `____` delimiter (or `[quote]` style
  block) for multi-paragraph bodies:

  ```adoc
  [quote]
  ____
  r[api.error.format]
  API errors must follow this format:

  [source,json]
  ----
  {"error": "message", "code": 400}
  ----
  ____
  ```

- **Inline markers are text, not definitions** — same rule as Markdown.

- **Heading ids** come from AsciiDoc auto-id, or an explicit `[[id]]` /
  `[#id]` anchor, then pass through `SlugAllocator::alloc` so they stay
  globally unique across the spec.

- **Frontmatter / weight** — AsciiDoc convention is a document attribute at
  the top, e.g. `:weight: 10`. `parse_weight` reads this. A `+++`/`---`
  TOML/YAML front matter should also be accepted (people mix).

- **Masks for marker extraction** — listing blocks (`----`, `....`), literal
  blocks, passthrough blocks (`++++`), and line/block comments (`//`,
  `////`) must all mask markers so example snippets aren't picked up.

- **`include::` directives** — v1: do not follow. Emit a `ParseWarning`.
  Tracking transitive includes is what PR #185 does for Typst (out-param
  `deps: &mut HashSet<PathBuf>`); AsciiDoc can adopt the same hook in a
  follow-up if users ask for it, without changing the public API shape.

Add a new section to `docs/content/spec/tracey.md` with rules
`r[asciidoc.syntax.marker]`, `r[asciidoc.syntax.inline-ignored]`,
`r[asciidoc.duplicates.same-file]`, `r[asciidoc.duplicates.cross-file]`,
`r[asciidoc.blocks.listing]`, `r[asciidoc.frontmatter.attributes]`,
`r[asciidoc.html.div]`, `r[asciidoc.html.anchor]`, `r[asciidoc.html.link]` —
parallel to the existing `markdown.*` rules.

### 2.2 Parser choice

Typst used `arborium-typst` (tree-sitter). Use the **same approach** for
AsciiDoc for symmetry with #185: `arborium` already wraps tree-sitter
grammars, and a `tree-sitter-asciidoc` grammar exists. Add
`arborium-asciidoc` to the workspace alongside `arborium-typst`. If that
grammar is missing or broken in practice, fall back to a minimal hand-rolled
parser — AsciiDoc's requirement-extraction subset (headings, paragraphs,
quoted blocks, listing blocks, comments, attribute entries) is small enough
to parse in ~300 lines, and the maintainer already accepted a hand-rolled
approach for Markdown's own text-based scanner. **Recommend tree-sitter**;
note the fallback in the PR description.

Heavy asciidoc rendering deps (if we end up using, e.g., `asciidork-*` as a
Phase-2 "pretty HTML" backend) go behind a Cargo feature
`asciidoc-spec` mirroring `typst-spec`. Phase 1 can render a `<pre>`
placeholder the way `spec::typst::parse` does in PR #185 (when `typst-spec`
is off) — this keeps the initial PR reviewable and unblocks everything
downstream; a nicer renderer can land as a follow-up.

### 2.3 Shape of `spec/asciidoc.rs`

Copy the file layout from PR #185's `spec/typst.rs` exactly:

```rust
// crates/tracey-core/src/spec/asciidoc.rs

pub struct RenderCtx<'a> {
    pub badge_for: &'a (dyn Fn(&ReqDefinition) -> (String, String) + Sync),
}

#[cfg_attr(not(feature = "asciidoc-spec"), allow(unused_variables))]
pub async fn render_display(
    content: &str,
    source_path: &std::path::Path,
    ctx: &RenderCtx<'_>,
    alloc: &mut SlugAllocator,
    // Include-resolution deps (empty in v1; kept for API symmetry with typst)
    deps: &mut std::collections::HashSet<std::path::PathBuf>,
) -> eyre::Result<SpecDoc>;

// Called by the parse_spec dispatcher — cheap path, no "pretty" rendering.
pub(super) async fn parse(content: &str) -> eyre::Result<SpecDoc>;
pub(super) fn diff_inline(old: &str, new: &str) -> Option<String>;
pub(super) fn parse_weight(content: &str) -> i32;
pub(super) fn extract_marker_prefix(content: &str, span: SourceSpan) -> Option<String>;
pub(super) fn id_range_in_marker(marker_str: &str) -> eyre::Result<std::ops::Range<usize>>;
```

Every caller already uses these names by dispatching through `SpecFormat`.

### 2.4 Integration points we touch

All of these are *already* format-aware in PR #185. Our work here is just
adding the AsciiDoc arms; if #185 isn't merged we're adding the same
plumbing #185 added.

| File | What changes |
|---|---|
| `crates/tracey-core/src/spec/mod.rs` | Add `AsciiDoc` variant to `SpecFormat`; add `"adoc" \| "asciidoc" \| "asc"` to `from_ext` and `SPEC_EXTENSIONS`; add dispatch arms in `parse_spec`, `diff_inline`, `parse_weight`, `extract_marker_prefix`, `id_range_in_marker`. Add `mod asciidoc; pub mod asciidoc;` (or `mod asciidoc; pub(crate) mod asciidoc;` — match #185's `pub mod typst;` so `tracey::data` can reach `spec::asciidoc::render_display` and `RenderCtx`). |
| `crates/tracey-core/src/spec/asciidoc.rs` | New file per §2.3. |
| `crates/tracey-core/Cargo.toml` | Add `arborium-asciidoc` (or chosen parser) to dependencies. If a heavier HTML renderer is used, add optional deps behind `asciidoc-spec` feature. |
| `crates/tracey-core/src/lib.rs` | No change if #185 already re-exports the spec module; otherwise replicate #185's re-exports. |
| `crates/tracey/src/data.rs` `load_spec_content` | Add `SpecFormat::AsciiDoc => { … }` arm next to the `Typst` arm (currently ~line 2895 in #185's diff). The arm mirrors the Typst arm: for each file call `spec::asciidoc::render_display` with a `RenderCtx` whose `badge_for` closure calls the already-extracted `rule_coverage_badge_html` helper. Reuse the same `SlugAllocator` threaded through markdown runs. No `deps` tracking needed in v1 — pass a throwaway `HashSet` (or only do include-tracking if the parser supports it). |
| `crates/tracey/src/data.rs` `devicon_class` | Add `"adoc" \| "asciidoc" => Some("devicon-asciidoctor-original")` (or a sensible fallback — devicon may not ship an AsciiDoc icon; pick from the existing set). |
| `crates/tracey/src/search.rs` | Add `"asciidoc"` to the `format_str` match and its reverse (`"asciidoc" => Some(SpecFormat::AsciiDoc)`). |
| `crates/tracey/src/daemon/service.rs` `arborium_language` | Add `"adoc" \| "asciidoc" => Some("asciidoc")`. |
| `crates/tracey/src/bump.rs` | No code change needed — already dispatches via `SpecFormat::from_path`. Only update the pathway that currently defaults to `SpecFormat::Markdown` on `unwrap_or` if we want AsciiDoc files to be picked up when their path-extension classification somehow fails (it won't, so leave as-is). |
| `crates/tracey/src/daemon/service.rs` (the LSP-ish handlers using `is_spec_extension` / `SpecFormat::from_path`) | No code change — already format-agnostic in #185. |
| `crates/tracey/src/daemon/watcher.rs` | No code change — purely glob-driven. A spec `include` of `**/*.adoc` just works. Add a unit test case. |
| `crates/tracey/src/bridge/lsp.rs` | No code change — uses `is_spec_extension`. |
| `crates/tracey-config/src/lib.rs` | Update the `SpecConfig::include` doc comment to mention `*.adoc` as another example. No schema change. If any AsciiDoc-specific attributes need config (e.g. attribute-set files, include search dirs), add an optional field symmetric to #185's `typst_package_path`. |
| `README.md`, `README.md.in`, `docs/content/guide/writing-specs.md`, `docs/content/guide/getting-started.md`, `docs/content/spec/tracey.md`, `crates/tracey/skill/references/tracey-spec.md` | Mention AsciiDoc alongside Markdown. Add the `asciidoc.*` self-spec rules. |

---

## 3. Design notes borrowed directly from PR #185

These are the non-obvious decisions #185 already made. Do not re-litigate;
just follow them.

- **`SlugAllocator` is threaded through the whole render, not per-format.**
  `load_spec_content` owns a single allocator and passes `&mut` into each
  per-format renderer. This is how heading anchors stay unique when a spec
  mixes formats. The allocator also normalises the `r--` requirement-anchor
  prefix so a `## Request` heading slug never collides with an
  `r[request]` req anchor. The AsciiDoc backend MUST call
  `alloc.alloc(base_slug)` for every heading and patch its HTML accordingly
  (see #185's in-place `replacen` on `<hN id="…">`).

- **`REQ_ANCHOR_PREFIX = "r--"` is THE contract.** Every backend emits
  `anchor_id = format!("r--{id}")` in `ReqDefinition::anchor_id`. The
  dashboard, static export (`bridge/export.rs`), and inline-code links
  (`TraceyInlineCodeHandler`) all use `req_anchor_id()`. Do not deviate.

- **`rule_coverage_badge_html` is the shared badge renderer.** #185 lifted
  it out of `TraceyRuleHandler::start` into a free function that takes
  `(&ReqDefinition, Option<&RuleCoverage>, &str source_file, &str spec_name,
  &str impl_name) -> (String open, String close)`. AsciiDoc's `RenderCtx`
  should delegate to this — do not reimplement badges.

- **Two-path rendering (cheap vs. pretty) with a feature gate.** #185's
  `spec::typst::parse` returns a `<pre class="typst-placeholder">`
  placeholder; `render_display` (feature-gated on `typst-spec`) does the
  real HTML compile. Follow the same pattern if pretty AsciiDoc rendering
  needs a heavy dependency. Make `asciidoc-spec` default-on in the
  binary build, off in library consumers.

- **`SpecFormat` is `#[non_exhaustive]`** and `load_spec_content` has an
  explicit `_ => eyre::bail!("rendering not implemented for spec format
  {run_fmt:?}")` arm. #185 says this is deliberate: future formats must be
  explicitly opted into the renderer before they can show up in the
  dashboard. Preserve that arm. Our change replaces that `bail!` *only* for
  the `AsciiDoc` variant, not globally.

- **Run-partitioning in `load_spec_content`.** #185 partitions the
  sorted-by-weight file list into **runs of consecutive same-format
  files**. Markdown runs are concatenated and rendered once (so marq's
  hierarchical heading-ids still span the whole run). Typst files render
  one-at-a-time. **AsciiDoc should render one-at-a-time** like Typst unless
  there is a concrete reason to concatenate — asciidoc documents don't
  benefit from shared-heading-hierarchy the way marq markdown does, and
  one-at-a-time rendering keeps the code simpler.

- **`ExtractedRule.format` and `search::RuleEntry.format`.** #185 adds
  these. Set them to `SpecFormat::AsciiDoc` in the extraction loop (see
  `extract_spec_rules_cached` in `data.rs` and `load_rules_from_glob` in
  `lib.rs`).

- **Historical diff uses the current path's format** (see `load_previous_
  rule_text_from_git`). #185 documents the known limitation: cross-format
  renames (e.g. `spec.md` → `spec.adoc`) will silently miss. This is the
  same limitation for AsciiDoc — call it out in a comment, don't try to fix.

---

## 4. Implementation order (3–4 commits, each reviewable)

### Commit 1 — `spec::asciidoc` module stub + variant

- Add `SpecFormat::AsciiDoc` to `spec/mod.rs` (extension list, `from_ext`,
  dispatch arms).
- Add `crates/tracey-core/src/spec/asciidoc.rs` with:
  - `parse` returning a minimal `SpecDoc` (no reqs, `<pre>` placeholder).
  - `diff_inline`, `parse_weight`, `extract_marker_prefix`,
    `id_range_in_marker` — each sufficient to pass the mirror of the
    existing markdown/typst tests in `spec/mod.rs`.
- Update unit tests in `spec/mod.rs` to cover AsciiDoc extensions, anchor
  round-trip, `from_path`, and `is_spec_extension`.
- **No downstream callers change.** Everything still dispatches through
  `SpecFormat::from_path` and gets a valid-but-empty doc for `.adoc` files.
  The binary compiles; existing tests still pass.

### Commit 2 — Real parser (reqs, headings, paragraphs, masks)

- Wire `arborium-asciidoc` (or the hand-rolled parser) in
  `spec/asciidoc.rs::parse`. Produce:
  - `reqs: Vec<ReqDefinition>` with proper `span`, `marker_span`,
    `anchor_id = req_anchor_id(&id)`, `line`, `raw`.
  - `elements: Vec<DocElement>` in document order.
  - `headings: Vec<Heading>` with deterministic base slugs (the
    `SlugAllocator` in `data.rs` will globalise them).
  - `inline_code_spans` from monospace/backtick spans.
  - `<pre>` placeholder HTML (same pattern as `typst::parse` when
    `typst-spec` is off).
- Implement the mask logic for listing blocks, literal blocks,
  passthroughs, and comments so markers inside them are ignored (mirror of
  `crates/tracey-core/src/markdown.rs::markdown_code_mask`).
- **Downstream callers still don't change.** LSP operations (document
  symbols, tokens, code lenses, inlay hints, document highlight) start
  working for `.adoc` files automatically because they all just call
  `parse_spec(fmt, content)` and walk `doc.reqs`.
- `bump` / `pre_commit` start working for `.adoc` files automatically —
  they already dispatch through `id_range_in_marker(fmt, …)` and
  `rewrite_marker`.

### Commit 3 — Dashboard rendering

- Add `AsciiDoc` arm to `load_spec_content`'s `match run_fmt` block in
  `crates/tracey/src/data.rs`, symmetric to the `Typst` arm.
  - Construct a `tracey_core::spec::asciidoc::RenderCtx { badge_for: &|def|
    rule_coverage_badge_html(def, coverage.get(&def.id.to_string()),
    &abs_source_str, spec_name, impl_name) }`.
  - Call `spec::asciidoc::render_display(content, &abs_source, &ctx, &mut
    slug_alloc, &mut throwaway_deps).await?`.
  - Append `SpecSection`, extend `all_elements` / `head_injections`.
- Add `"adoc" | "asciidoc"` branches to `devicon_class` and
  `arborium_language`.
- Add `"asciidoc"` to `search.rs` format-string round-trip.

### Commit 4 — Tests, docs, fixtures

- New `crates/tracey/tests/fixtures-asciidoc/` mirroring
  `fixtures-typst/`: `config.styx`, `spec.adoc`, sibling rust source.
- New `crates/tracey/tests/asciidoc_spec_tests.rs` mirroring
  `typst_spec_tests.rs`: parse, bump, LSP doc-symbols, inline diff,
  end-to-end dashboard render.
- Extend `watcher_tests.rs` with an `.adoc`-file edit case.
- Update docs per §2.4 table.
- Update the config help-text string in `data.rs` (currently
  `docs/spec/**/*.{md,typ}` post-#185) to `docs/spec/**/*.{md,typ,adoc}`.

---

## 5. Coordinating with PR #185

Two merge orders are possible; plan for both.

**Scenario A — #185 merges first.** Straightforward. This plan becomes pure
addition: one new variant, one new submodule, one new `match` arm, one new
test file. No abstraction work to duplicate.

**Scenario B — AsciiDoc PR ready before #185 merges.** Replicate #185's
abstraction verbatim (as §1 describes). Do not rename. Do not "improve".
Structure the AsciiDoc PR so it could logically stack on top of #185:

- Commit 1 of the AsciiDoc PR should introduce *only* the shared
  abstraction (the `spec/mod.rs` enum, helpers, `SlugAllocator`,
  `req_anchor_id`, `rule_coverage_badge_html` extraction, `ExtractedRule.
  format`, `search::RuleEntry.format`). That commit is byte-identical to
  the equivalent commits in #185 — it's literally the same change. The
  maintainer can merge whichever PR's abstraction-commit lands first; the
  other PR deletes it on rebase.
- Commits 2–4 add the AsciiDoc variant on top.

Either way, do not invent a parallel abstraction. The maintainer of #185 is
the same person who will review the AsciiDoc PR, and a competing design is
how second-format PRs die.

---

## 6. Non-goals

- Full Asciidoctor-Ruby feature parity (tables, admonitions, callouts,
  conditionals, math, footnotes). Unsupported features pass through as
  plain text. Add a `ParseWarning` for features we detect but ignore.
- Following `include::` directives in v1 (parameter kept in the API for
  symmetry, filled with an empty set).
- AsciiDoc as a **source-code language** — source parsing is already
  format-agnostic and isn't affected.
- Migration tooling. Existing markdown specs stay markdown; AsciiDoc is
  opt-in per spec via include patterns.

---

## 7. Open questions

1. **`arborium-asciidoc` availability.** Confirm the crate exists and parses
   the subset we need. If not, fall back to a hand-rolled parser (decide
   before commit 2; note in PR description).
2. **Devicon for AsciiDoc.** Check what devicon ships. If nothing clean,
   reuse the markdown icon or a neutral file-code icon — don't block review
   on this.
3. **Should AsciiDoc participate in the `SlugAllocator` the same way as
   Markdown, or does tree-sitter give us globally-unique IDs we can emit
   directly?** Consult #185's Typst implementation — it uses the allocator;
   do the same. Don't optimise prematurely.
4. **Feature flag.** Is the AsciiDoc rendering backend heavy enough to
   warrant `asciidoc-spec`, or can we compile it unconditionally? If tree-
   sitter + a small renderer, probably unconditional. If we pull in
   `asciidork-*`'s full rendering pipeline later, put *that* behind a flag
   symmetric to `typst-spec`.

---

## 8. Reviewer checklist (mirror PR #185's expectations)

- [ ] `SpecFormat::AsciiDoc` added with extension list, `from_ext`,
      dispatch arms; no other enum consumer broken (all sites pattern-match
      exhaustively via `#[non_exhaustive]` `_ =>` arms, so new variant
      additions are backward-compatible).
- [ ] `spec/asciidoc.rs` mirrors `spec/typst.rs` one-for-one: same public
      signatures, same `RenderCtx` shape, same cheap/pretty split.
- [ ] `rule_coverage_badge_html` reused, not reimplemented.
- [ ] `SlugAllocator` threaded through the AsciiDoc render; heading ids are
      globally unique across a mixed-format spec.
- [ ] Every `anchor_id` emitted starts with `REQ_ANCHOR_PREFIX`.
- [ ] `ExtractedRule.format`, `search::RuleEntry.format`, and the search
      index format-string are populated.
- [ ] `load_previous_rule_text_from_git` falls through correctly for
      `.adoc` files.
- [ ] Tests mirror the `typst_spec_tests.rs` coverage: parse, bump, LSP
      operations, end-to-end render, watcher rebuild on edit.
- [ ] New `asciidoc.*` self-spec rules added and covered in the fixtures.
- [ ] `cargo clippy --all-targets --all-features` clean.
- [ ] No reverts of #185 helpers; no parallel abstraction.
