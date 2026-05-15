# Pluggable Spec Formats

**Status:** draft · **Branch:** `typst-spec` · **Supersedes:** ad-hoc `match SpecFormat` dispatch

## Problem

Adding a spec format today is shotgun surgery. PR #189 (AsciiDoc) and PR #191
(StrictDoc) each touch ~7 files outside their own module:

- `spec/mod.rs` — variant + 5 `match fmt` arms
- `data.rs:3074-3170` — 96-line HTML-render `match`
- `daemon/service.rs:775` — search-snippet render `match`
- `search.rs:226,317` — format ↔ string for tantivy
- `lib.rs`, `bump.rs` — `from_ext` / `is_spec_extension` updates

After the typst-spec rebase, `sdoc` is half-integrated: it has no
`SpecFormat` variant, is special-cased in `is_spec_extension`, and dispatches
via a parallel `extract_sdoc_rules_cached` rather than `parse_spec`.

## Goal

Adding a format = **one new module + ≤3 lines in `spec/mod.rs`**. Zero edits
to `data.rs`, `service.rs`, `search.rs`, `bump.rs`, `lib.rs`.

Non-goals: out-of-tree/third-party format crates; runtime (dylib) plugins.
All formats live under `crates/tracey-core/src/spec/`.

## Design

### `SpecBackend` trait

What format authors implement. Has an associated `Config` type so per-backend
options (e.g. typst's `package_path`) are declared as a struct, validated at
config-load time, and reach `render_html` fully typed — no `toml::Table`
poking. `#[async_trait]` (already a workspace dep) for the two async methods.

```rust
#[async_trait]
pub trait SpecBackend: Send + Sync + 'static {
    /// Per-backend options deserialized from the `[spec.<name>]` table in
    /// `config.styx`. Use `()` if the backend has none.
    type Config: Facet<'static> + Default + Send + Sync + 'static;

    /// Enum variant this backend implements. 1:1 with `SpecFormat`.
    fn format(&self) -> SpecFormat;

    /// Stable lowercase identifier — tantivy field, logs, config table key.
    fn name(&self) -> &'static str;

    /// File extensions (no leading dot) this backend claims.
    fn extensions(&self) -> &'static [&'static str];

    // ─── extraction (cheap path; no HTML, no config) ─────────────────
    async fn parse(&self, content: &str) -> eyre::Result<SpecDoc>;
    fn parse_weight(&self, _content: &str) -> i32 { 0 }
    fn extract_marker_prefix(&self, content: &str, span: SourceSpan) -> Option<String>;
    fn id_range_in_marker(&self, marker: &str) -> eyre::Result<Range<usize>>;
    fn diff_inline(&self, old: &str, new: &str) -> Option<String>;

    // ─── display (dashboard HTML; expensive, config-dependent) ───────
    async fn render_html(
        &self,
        input: RenderInput<'_>,
        cfg: &Self::Config,
    ) -> eyre::Result<RenderOutput>;

    /// Render a short body fragment as inline HTML (search snippets, hovers).
    /// Default: HTML-escape only. Markdown overrides to run marq.
    async fn render_inline(&self, text: &str) -> String {
        html_escape::encode_text(text).into_owned()
    }
}
```

#### Object-safe dispatch layer

`type Config` makes `SpecBackend` non-object-safe (it appears in
`render_html`'s signature), so the registry can't hold `&dyn SpecBackend`
directly. A private erased trait bridges the gap; format authors never see it.

```rust
// spec/registry.rs — internal
#[async_trait]
trait DynBackend: Send + Sync + 'static {
    fn format(&self) -> SpecFormat;
    fn name(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];
    async fn parse(&self, content: &str) -> eyre::Result<SpecDoc>;
    fn parse_weight(&self, content: &str) -> i32;
    fn extract_marker_prefix(&self, content: &str, span: SourceSpan) -> Option<String>;
    fn id_range_in_marker(&self, marker: &str) -> eyre::Result<Range<usize>>;
    fn diff_inline(&self, old: &str, new: &str) -> Option<String>;
    async fn render_inline(&self, text: &str) -> String;

    /// Deserialize this backend's `[spec.<name>]` table. Called once at
    /// config load; result is stored alongside the backend.
    fn deserialize_config(&self, raw: Option<&styx::Value>) -> eyre::Result<ErasedConfig>;
    async fn render_html(
        &self,
        input: RenderInput<'_>,
        cfg: &ErasedConfig,
    ) -> eyre::Result<RenderOutput>;
}

pub struct ErasedConfig(Box<dyn Any + Send + Sync>);

#[async_trait]
impl<B: SpecBackend> DynBackend for B {
    // …forwarders…
    fn deserialize_config(&self, raw: Option<&styx::Value>) -> eyre::Result<ErasedConfig> {
        let cfg: B::Config = match raw {
            Some(v) => facet::from_value(v)?,
            None => B::Config::default(),
        };
        Ok(ErasedConfig(Box::new(cfg)))
    }
    async fn render_html(&self, input: RenderInput<'_>, cfg: &ErasedConfig) -> eyre::Result<RenderOutput> {
        let cfg = cfg.0.downcast_ref::<B::Config>()
            .expect("ErasedConfig was produced by this backend's deserialize_config");
        SpecBackend::render_html(self, input, cfg).await
    }
}
```

The `Any` round-trip is sealed inside `registry.rs`; both the format author
and the driver in `data.rs` work with concrete types.

### Unified render contract

```rust
pub struct RenderSource<'a> {
    pub path: &'a Path,
    pub content: &'a str,
}

pub struct RenderInput<'a> {
    /// One or more same-format files. Backends that render per-file
    /// (typst, sdoc, asciidoc) iterate; markdown concatenates the run
    /// to preserve its current unified-TOC behaviour.
    pub sources: &'a [RenderSource<'a>],
    /// Project root, for resolving relative `#import` / `include::`.
    pub root: &'a Path,
    /// Coverage badge HTML to inject next to each requirement.
    pub badge_for: &'a (dyn Fn(&ReqDefinition) -> String + Sync),
    /// Cross-file heading-slug deduplicator (shared across the whole build).
    pub slugs: &'a mut SlugAllocator,
}

pub struct RenderOutput {
    pub html: String,
    /// Extra files read during render (imports/includes) — fed to the
    /// file-watcher and cache-key.
    pub deps: Vec<PathBuf>,
}
```

The driver in `data.rs` groups consecutive same-format spec files and calls
`render_html` once per group. This replaces the 96-line `match` with ~15
lines of format-agnostic grouping.

### Registry & config flow

`SpecFormat` **stays a `Copy` enum** — it's stored on `ExtractedRule`,
serialized into tantivy, and pattern-matched in tests. Behaviour lives on the
trait; the enum is just a token.

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SpecFormat { Markdown, Typst, Sdoc, AsciiDoc }

/// Single source of truth. Adding a format = one line here + one variant above.
static BACKENDS: &[&(dyn DynBackend)] = &[
    &markdown::Markdown,
    &typst::Typst,
    &sdoc::Sdoc,
    // &asciidoc::AsciiDoc,   // PR #189
];

impl SpecFormat {
    pub(crate) fn backend(self) -> &'static dyn DynBackend {
        BACKENDS.iter().copied()
            .find(|b| b.format() == self)
            .expect("every SpecFormat variant has a registered backend")
    }
    pub fn from_ext(ext: &OsStr) -> Option<Self> {
        let s = ext.to_str()?;
        BACKENDS.iter().find(|b| b.extensions().contains(&s)).map(|b| b.format())
    }
    pub fn from_name(name: &str) -> Option<Self> {
        BACKENDS.iter().find(|b| b.name() == name).map(|b| b.format())
    }
    pub fn name(self) -> &'static str { self.backend().name() }
}
```

**Config flow.** `tracey-config` gains a `spec: BTreeMap<String, styx::Value>`
field holding raw `[spec.<name>]` tables. At build time `data.rs` constructs:

```rust
pub struct SpecConfigs(HashMap<SpecFormat, ErasedConfig>);

impl SpecConfigs {
    pub fn load(raw: &BTreeMap<String, styx::Value>) -> eyre::Result<Self> {
        BACKENDS.iter()
            .map(|b| Ok((b.format(), b.deserialize_config(raw.get(b.name()))?)))
            .collect::<Result<_>>()
            .map(Self)
    }
    pub fn get(&self, fmt: SpecFormat) -> &ErasedConfig { &self.0[&fmt] }
}
```

Unknown keys in `raw` are rejected (`"no spec backend named '{k}'"`), so
typos surface at startup. `SpecConfigs` is held on `BuildContext` and passed
to the render loop. Typst's existing `package_path` becomes
`TypstConfig { package_path: Option<PathBuf> }`.

The five existing free functions (`parse_spec`, `diff_inline`, …) become thin
shims: `pub async fn parse_spec(fmt, c) { fmt.backend().parse(c).await }`.
Kept for one release to avoid churning every call site at once; deprecated
afterwards.

### Derived helpers

- `is_spec_extension(ext)` → `SpecFormat::from_ext(ext).is_some()` (drops the
  `.sdoc` special-case bridge added in `4bc8f8b`).
- `SPEC_EXTENSIONS` const → deleted; the one test that uses it switches to
  iterating `BACKENDS.flat_map(extensions)`.

## Migration order

Each step compiles green.

1. **Trait + types** — add `SpecBackend`, `DynBackend` + blanket impl,
   `ErasedConfig`, `RenderInput/Output`, `SpecConfigs`, `BACKENDS` to
   `spec/{mod.rs,registry.rs}`. No callers yet.
2. **Markdown impl** — `struct Markdown; impl SpecBackend for Markdown`.
   `render_html` absorbs the concat + `marq::render` + slug-rewrite logic
   currently inline at `data.rs:3074-3130`.
3. **Typst impl** — wrap existing `render_display`; map its
   `(ctx, alloc, &mut deps)` signature onto `RenderInput`/`RenderOutput`.
4. **Facade rewrite** — replace 5 `match fmt` bodies with
   `fmt.backend().<op>()`. Derive `from_ext` from `BACKENDS`.
5. **Collapse `data.rs:3074`** — replace the big match with the
   group-by-format → `render_html` loop.
6. **Collapse `service.rs:775`** — replace snippet match with
   `fmt.backend().render_inline(...)`; PUA→`<mark>` substitution stays at the
   call site (search-specific, not format-specific).
7. **Collapse `search.rs:226/317`** — `fmt.name()` / `SpecFormat::from_name()`.
8. **Absorb sdoc** — move `crates/tracey/src/sdoc.rs` →
   `crates/tracey-core/src/spec/sdoc.rs`; convert `extract_rules_from_sdoc`
   into `parse()` returning `SpecDoc`; impl `SpecBackend`; add
   `SpecFormat::Sdoc`; delete `extract_sdoc_rules_cached` and the
   `data.rs:~1180` if/else.
9. **Cleanup** — drop `4bc8f8b`'s sdoc bridges; delete `SPEC_EXTENSIONS`;
   `cargo doc` pass on the new trait.

## What PR #189 (AsciiDoc) becomes

After this lands, James's PR shrinks to:

```text
crates/tracey-core/src/spec/asciidoc/{mod.rs,ast_walk.rs}   (unchanged)
crates/tracey-core/src/spec/mod.rs                          +2 lines
crates/tracey/tests/asciidoc_spec_tests.rs + fixtures/      (unchanged)
```

No edits to `data.rs`, `service.rs`, `search.rs`, `lib.rs`.

## Risks / open questions

- **`&mut SlugAllocator` across async** — `render_html` is async and takes
  `&mut` via `RenderInput`. Fine for sequential rendering; if we ever
  parallelise per-format groups, switch to `Arc<Mutex<SlugAllocator>>`.
- **`Facet<'static>` bound** — assumes `facet` can deserialize from a borrowed
  `styx::Value` subtree. If not, `SpecConfigs::load` falls back to
  re-serialize → `facet::from_str`. Verify in step 1.
- **sdoc → `SpecDoc`** — `extract_rules_from_sdoc` currently builds
  `ExtractedRule` directly (with `source_file`, `prefix`). Converting to
  `SpecDoc` means the section/column derivation in `lib.rs:173-210` must work
  for sdoc's spans. Needs a fixture test before step 8.
- **Markdown run-concat semantics** — moving concat into
  `Markdown::render_html` must preserve current TOC/anchor output exactly.
  Golden-file test against current `data.rs` output before step 5.
