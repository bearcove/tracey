//! Object-safe dispatch layer over [`SpecBackend`].
//!
//! [`SpecBackend`] is the typed surface format authors implement, but its
//! associated `Config` type makes it non-object-safe (it appears in
//! `render_html`'s signature). [`DynBackend`] mirrors every method with
//! `Config` erased to [`ErasedConfig`]; a blanket impl bridges the two so the
//! `Any` round-trip is sealed in this file. Both format authors and the
//! render driver in `data.rs` work with concrete types.

use std::any::Any;
use std::collections::HashMap;
use std::ops::Range;

use async_trait::async_trait;

use super::{RenderInput, RenderOutput, SourceSpan, SpecBackend, SpecDoc, SpecFormat};

/// Unit config for backends that have no `[spec.<name>]` options.
///
/// `()` does not implement [`facet::Facet`], so backends use this as
/// `type Config = NoConfig;` instead.
#[derive(Default, Debug, Clone, Copy, facet::Facet)]
pub struct NoConfig;

/// Type-erased backend config.
///
/// Produced by [`DynBackend::default_config`] / [`ErasedConfig::new`] and
/// consumed by [`DynBackend::render_html`]. The wrapped `Any` is always the
/// impl's `B::Config`; the downcast in the blanket impl is therefore
/// infallible by construction (panics on misuse).
pub struct ErasedConfig(Box<dyn Any + Send + Sync>);

impl ErasedConfig {
    /// Wrap a concrete `B::Config` for [`SpecConfigs::insert`]. The value MUST
    /// be the backend's `Config` type or `render_html` will panic on downcast.
    pub fn new<C: Any + Send + Sync>(cfg: C) -> Self {
        Self(Box::new(cfg))
    }
}

/// Object-safe mirror of [`SpecBackend`].
///
/// `pub(crate)` — callers outside `spec` go through [`SpecFormat`] or the
/// free-function shims; only `data.rs` (via `SpecConfigs`) interacts with this
/// directly once the render loop is migrated.
#[async_trait]
pub(crate) trait DynBackend: Send + Sync + 'static {
    fn format(&self) -> SpecFormat;
    fn name(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];

    async fn parse(&self, content: &str) -> eyre::Result<SpecDoc>;
    fn parse_weight(&self, content: &str) -> i32;
    fn extract_marker_prefix(&self, content: &str, span: SourceSpan) -> Option<String>;
    fn id_range_in_marker(&self, marker: &str) -> eyre::Result<Range<usize>>;
    fn diff_inline(&self, old: &str, new: &str) -> Option<String>;

    async fn render_html(
        &self,
        input: RenderInput<'_>,
        cfg: &ErasedConfig,
    ) -> eyre::Result<RenderOutput>;
    async fn render_inline(&self, text: &str) -> String;

    /// `B::Config::default()` boxed.
    ///
    /// `facet-styx` is string→struct only (no raw subtree value type), so
    /// there is no generic `deserialize_config(raw)` path. Config flows: styx
    /// file → typed `tracey_config::FormatConfig` → `crates/tracey` glue
    /// converts each field into the matching `B::Config` and calls
    /// [`SpecConfigs::insert`]. This method seeds defaults for backends the
    /// caller does not override.
    fn default_config(&self) -> ErasedConfig;
}

#[async_trait]
impl<B: SpecBackend> DynBackend for B {
    fn format(&self) -> SpecFormat {
        SpecBackend::format(self)
    }
    fn name(&self) -> &'static str {
        SpecBackend::name(self)
    }
    fn extensions(&self) -> &'static [&'static str] {
        SpecBackend::extensions(self)
    }

    async fn parse(&self, content: &str) -> eyre::Result<SpecDoc> {
        SpecBackend::parse(self, content).await
    }
    fn parse_weight(&self, content: &str) -> i32 {
        SpecBackend::parse_weight(self, content)
    }
    fn extract_marker_prefix(&self, content: &str, span: SourceSpan) -> Option<String> {
        SpecBackend::extract_marker_prefix(self, content, span)
    }
    fn id_range_in_marker(&self, marker: &str) -> eyre::Result<Range<usize>> {
        SpecBackend::id_range_in_marker(self, marker)
    }
    fn diff_inline(&self, old: &str, new: &str) -> Option<String> {
        SpecBackend::diff_inline(self, old, new)
    }

    async fn render_html(
        &self,
        input: RenderInput<'_>,
        cfg: &ErasedConfig,
    ) -> eyre::Result<RenderOutput> {
        let cfg = cfg
            .0
            .downcast_ref::<B::Config>()
            .expect("ErasedConfig was produced by this backend's deserialize_config");
        SpecBackend::render_html(self, input, cfg).await
    }
    async fn render_inline(&self, text: &str) -> String {
        SpecBackend::render_inline(self, text).await
    }

    fn default_config(&self) -> ErasedConfig {
        ErasedConfig(Box::new(B::Config::default()))
    }
}

/// Single source of truth for spec-format dispatch.
///
/// Adding a format = one entry here + one [`SpecFormat`] variant.
pub(crate) static BACKENDS: &[&dyn DynBackend] =
    &[&super::markdown::Markdown, &super::typst::Typst, &super::sdoc::Sdoc];

/// Per-spec-set config bundle for every registered backend.
///
/// Built per `tracey_config::SpecConfig` and passed to [`render_spec_html`].
/// Construct with [`Default`] (every backend at `Config::default()`) then
/// [`insert`](Self::insert) any non-default backends from the typed config
/// schema; see `tracey::data::build_spec_configs`.
///
/// [`render_spec_html`]: super::render_spec_html
pub struct SpecConfigs(HashMap<SpecFormat, ErasedConfig>);

impl Default for SpecConfigs {
    fn default() -> Self {
        Self(
            BACKENDS
                .iter()
                .map(|b| (b.format(), b.default_config()))
                .collect(),
        )
    }
}

impl SpecConfigs {
    /// Override the config for `fmt`.
    ///
    /// `cfg` MUST wrap the backend's `Config` type or `render_html` will panic
    /// on downcast.
    pub fn insert(&mut self, fmt: SpecFormat, cfg: ErasedConfig) {
        self.0.insert(fmt, cfg);
    }

    /// Panics if `fmt` has no registered backend (every variant should).
    pub fn get(&self, fmt: SpecFormat) -> &ErasedConfig {
        self.0
            .get(&fmt)
            .expect("every SpecFormat variant has a registered backend")
    }
}
