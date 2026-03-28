//! Static site export for tracey spec coverage data.
//!
//! `tracey export <output>` produces a fully self-contained directory of HTML
//! files that can be served by any static file host. No daemon or JavaScript
//! framework is required to view the exported pages.

use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr, eyre};
use tracey_api::{
    ApiCodeUnit, ApiConfig, ApiFileData, ApiReverseData, ApiRule, ApiSpecData, ApiSpecForward,
    OutlineEntry,
};

// r[impl export.output-structure]
// r[impl export.self-contained]
pub async fn run(
    root: Option<PathBuf>,
    _config_path: PathBuf,
    output: PathBuf,
    include_sources: bool,
) -> Result<()> {
    let project_root = match root {
        Some(r) => r,
        None => crate::find_project_root().wrap_err("finding project root")?,
    };

    let client = crate::daemon::new_client(project_root.clone());
    let config = client
        .config()
        .await
        .map_err(|e| eyre!("config RPC failed: {:?}", e))?;

    std::fs::create_dir_all(&output)
        .wrap_err_with(|| format!("creating output directory {}", output.display()))?;
    write_assets(&output).wrap_err("writing static assets")?;

    // Collect coverage stats for the landing page while exporting each pair
    let mut spec_cards: Vec<SpecCard> = Vec::new();

    for spec_info in &config.specs {
        for impl_name in &spec_info.implementations {
            let spec_name = &spec_info.name;
            eprintln!("Exporting {spec_name} × {impl_name}…");

            let forward = client
                .forward(spec_name.clone(), impl_name.clone())
                .await
                .map_err(|e| eyre!("forward RPC failed for {spec_name}/{impl_name}: {:?}", e))?
                .ok_or_else(|| eyre!("no forward data for {spec_name}/{impl_name}"))?;

            let reverse = client
                .reverse(spec_name.clone(), impl_name.clone())
                .await
                .map_err(|e| eyre!("reverse RPC failed for {spec_name}/{impl_name}: {:?}", e))?
                .ok_or_else(|| eyre!("no reverse data for {spec_name}/{impl_name}"))?;

            let spec_content = client
                .spec_content(spec_name.clone(), impl_name.clone())
                .await
                .map_err(|e| {
                    eyre!(
                        "spec_content RPC failed for {spec_name}/{impl_name}: {:?}",
                        e
                    )
                })?
                .ok_or_else(|| eyre!("no spec content for {spec_name}/{impl_name}"))?;

            // Collect stats for landing page
            let total = forward.rules.len();
            let implemented = forward
                .rules
                .iter()
                .filter(|r| !r.impl_refs.is_empty())
                .count();
            let tested = forward
                .rules
                .iter()
                .filter(|r| !r.verify_refs.is_empty())
                .count();
            spec_cards.push(SpecCard {
                spec_name: spec_name.clone(),
                impl_name: impl_name.clone(),
                total,
                implemented,
                tested,
            });

            let pair_dir = output.join(spec_name).join(impl_name);
            std::fs::create_dir_all(&pair_dir)
                .wrap_err_with(|| format!("creating directory {}", pair_dir.display()))?;

            std::fs::write(
                pair_dir.join("spec.html"),
                render_spec_page(
                    spec_name,
                    impl_name,
                    &spec_content,
                    &config,
                    include_sources,
                )
                .wrap_err_with(|| format!("rendering spec page for {spec_name}/{impl_name}"))?,
            )
            .wrap_err_with(|| format!("writing {spec_name}/{impl_name}/spec.html"))?;

            std::fs::write(
                pair_dir.join("coverage.html"),
                render_coverage_page(spec_name, impl_name, &forward, &config, include_sources)
                    .wrap_err_with(|| {
                        format!("rendering coverage page for {spec_name}/{impl_name}")
                    })?,
            )
            .wrap_err_with(|| format!("writing {spec_name}/{impl_name}/coverage.html"))?;

            if include_sources {
                std::fs::write(
                    pair_dir.join("sources.html"),
                    render_sources_index(spec_name, impl_name, &reverse, &config).wrap_err_with(
                        || format!("rendering sources index for {spec_name}/{impl_name}"),
                    )?,
                )
                .wrap_err_with(|| format!("writing {spec_name}/{impl_name}/sources.html"))?;

                let sources_dir = pair_dir.join("sources");
                for file_entry in &reverse.files {
                    let path = &file_entry.path;
                    let req = tracey_proto::FileRequest {
                        spec: spec_name.clone(),
                        impl_name: impl_name.clone(),
                        path: path.clone(),
                    };
                    if let Some(file_data) = client
                        .file(req)
                        .await
                        .map_err(|e| eyre!("file RPC failed for {path}: {:?}", e))?
                    {
                        let file_html = render_file_page(spec_name, impl_name, &file_data, &config)
                            .wrap_err_with(|| format!("rendering file page for {path}"))?;
                        let out_path = sources_dir.join(format!("{path}.html"));
                        if let Some(parent) = out_path.parent() {
                            std::fs::create_dir_all(parent).wrap_err_with(|| {
                                format!("creating directory {}", parent.display())
                            })?;
                        }
                        std::fs::write(&out_path, file_html)
                            .wrap_err_with(|| format!("writing {}", out_path.display()))?;
                    }
                }
            }
        }
    }

    // Generate landing page
    write_landing_page(&output, &project_root, &spec_cards)
        .await
        .wrap_err("writing landing page")?;

    eprintln!("\nDone! Static site written to: {}", output.display());
    eprintln!(
        "Serve with:  python3 -m http.server -d {}",
        output.display()
    );
    Ok(())
}

struct SpecCard {
    spec_name: String,
    impl_name: String,
    total: usize,
    implemented: usize,
    tested: usize,
}

// ============================================================================
// Assets
// ============================================================================

fn write_assets(output: &Path) -> Result<()> {
    let assets_dir = output.join("assets");
    std::fs::create_dir_all(&assets_dir)
        .wrap_err_with(|| format!("creating assets directory {}", assets_dir.display()))?;

    let full_css = format!("{}\n{}", crate::bridge::http::INDEX_CSS, STATIC_EXTRA_CSS);
    std::fs::write(assets_dir.join("style.css"), full_css).wrap_err("writing assets/style.css")?;
    std::fs::write(assets_dir.join("enhance.js"), ENHANCE_JS)
        .wrap_err("writing assets/enhance.js")?;
    Ok(())
}

// r[impl export.landing-page]
// r[impl export.landing-page.default-content]
// r[impl export.landing-page.readme]
// r[impl export.landing-page.spec-grid]
async fn write_landing_page(
    output: &Path,
    project_root: &Path,
    spec_cards: &[SpecCard],
) -> Result<()> {
    // Try to read and render README.md from the project root
    let readme_path = project_root.join("README.md");
    let intro_html = if readme_path.is_file() {
        let readme_content = std::fs::read_to_string(&readme_path).wrap_err("reading README.md")?;
        let opts = marq::RenderOptions::default();
        let doc = marq::render(&readme_content, &opts)
            .await
            .wrap_err("rendering README.md")?;
        doc.html
    } else {
        r#"<h1>Specifications</h1><p>Browse the exported specifications below.</p>"#.to_string()
    };

    // Build spec card grid
    let cards = spec_cards
        .iter()
        .map(|card| {
            let impl_class = stat_class(card.implemented, card.total);
            let test_class = stat_class(card.tested, card.total);
            let impl_arc = coverage_arc_svg(
                card.implemented,
                card.total,
                "var(--green)",
                &format!("Impl: {}/{}", card.implemented, card.total),
            );
            let test_arc = coverage_arc_svg(
                card.tested,
                card.total,
                "var(--blue)",
                &format!("Tests: {}/{}", card.tested, card.total),
            );
            format!(
                r#"<a class="spec-card" href="/{spec}/{impl_name}/spec.html">
  <div class="spec-card-header">
    <span class="spec-card-name">{spec_escaped}</span>
    <span class="spec-card-impl">{impl_escaped}</span>
  </div>
  <div class="spec-card-stats">
    <span class="spec-card-stat">{impl_arc} <span class="stat-value {impl_class}">{implemented}/{total}</span> implemented</span>
    <span class="spec-card-stat">{test_arc} <span class="stat-value {test_class}">{tested}/{total}</span> tested</span>
  </div>
</a>"#,
                spec = card.spec_name,
                impl_name = card.impl_name,
                spec_escaped = html_escape(&card.spec_name),
                impl_escaped = html_escape(&card.impl_name),
                implemented = card.implemented,
                tested = card.tested,
                total = card.total,
                impl_class = impl_class,
                test_class = test_class,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let content = format!(
        r#"<div class="landing-page">
  <div class="landing-readme markdown">{intro_html}</div>
  <div class="landing-grid-section">
    <h2>Specifications</h2>
    <div class="landing-grid">{cards}</div>
  </div>
</div>"#
    );

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta name="color-scheme" content="light dark">
  <title>Tracey</title>
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Recursive:slnt,wght,CASL,CRSV,MONO@-15..0,300..1000,0..1,0..1,0..1&display=swap" rel="stylesheet">
  <link rel="stylesheet" href="/assets/style.css">
</head>
<body>
  <div class="app-shell">
    <div class="layout">
      <header class="header">
        <div class="header-inner">
          <a href="/" class="logo">tracey</a>
        </div>
      </header>
      <div class="main">
        <div class="content">
          <div class="content-body">
            {content}
          </div>
        </div>
      </div>
    </div>
  </div>
</body>
</html>"#
    );

    std::fs::write(output.join("index.html"), html).wrap_err("writing index.html")?;
    Ok(())
}

// ============================================================================
// Page shell — uses dashboard CSS classes directly
// ============================================================================

// r[impl export.navigation]
#[allow(clippy::too_many_arguments)]
fn page_shell(
    title: &str,
    spec_name: &str,
    impl_name: &str,
    active_tab: &str, // "spec" | "coverage" | "sources"
    config: &ApiConfig,
    include_sources: bool,
    sidebar_html: &str,
    content_html: &str,
    head_extras: &str,
) -> String {
    // Spec/impl selector tabs in the header-pickers area
    let spec_links = config
        .specs
        .iter()
        .flat_map(|s| {
            s.implementations.iter().map(move |i| {
                let active = if s.name == spec_name && i == impl_name {
                    " active"
                } else {
                    ""
                };
                format!(
                    r#"<a href="/{}/{}/spec.html" class="spec-tab{active}">{} / {}</a>"#,
                    s.name,
                    i,
                    html_escape(&s.name),
                    html_escape(i),
                )
            })
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Nav tabs
    let tab = |label: &str, icon: &str, href: &str, key: &str| {
        let active = if active_tab == key { " active" } else { "" };
        format!(
            r#"<a href="{href}" class="nav-tab{active}"><span class="tab-icon"><i data-lucide="{icon}"></i></span><span>{label}</span></a>"#
        )
    };
    let sources_tab = if include_sources {
        tab(
            "Sources",
            "code-2",
            &format!("/{spec_name}/{impl_name}/sources.html"),
            "sources",
        )
    } else {
        String::new()
    };
    let nav_tabs = format!(
        "{}\n{}\n{sources_tab}",
        tab(
            "Specification",
            "file-text",
            &format!("/{spec_name}/{impl_name}/spec.html"),
            "spec"
        ),
        tab(
            "Coverage",
            "bar-chart-2",
            &format!("/{spec_name}/{impl_name}/coverage.html"),
            "coverage"
        ),
    );

    // Sidebar (omit the <aside> entirely when empty)
    let sidebar = if sidebar_html.is_empty() {
        String::new()
    } else {
        format!(
            r#"<aside class="sidebar" id="sidebar"><div class="sidebar-content">{sidebar_html}</div></aside>"#
        )
    };

    // Mobile sidebar toggle button (only rendered when sidebar has content)
    let sidebar_toggle = if sidebar_html.is_empty() {
        String::new()
    } else {
        r#"<button class="sidebar-toggle" id="sidebar-toggle" aria-label="Toggle sidebar"><i data-lucide="panel-left"></i></button>"#.to_string()
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta name="color-scheme" content="light dark">
  <title>{title} — Tracey</title>
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Recursive:slnt,wght,CASL,CRSV,MONO@-15..0,300..1000,0..1,0..1,0..1&display=swap" rel="stylesheet">
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@arborium/arborium@2.4.6/dist/themes/base.css">
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@arborium/arborium@2.4.6/dist/themes/kanagawa-dragon.css">
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@arborium/arborium@2.4.6/dist/themes/github-light.css">
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/gh/devicons/devicon@latest/devicon.min.css">
  <script src="https://cdn.jsdelivr.net/npm/lucide@0.469.0/dist/umd/lucide.min.js"></script>
  <link rel="stylesheet" href="/assets/style.css">
  {head_extras}
</head>
<body>
  <div class="app-shell">
    <div class="layout">
      <header class="header">
        <div class="header-inner">
          <div class="header-pickers">
            {spec_links}
          </div>
          <nav class="nav">
            {nav_tabs}
          </nav>
          {sidebar_toggle}
          <a href="https://tracey.bearcove.eu/" class="logo">tracey</a>
        </div>
      </header>
      <div class="main">
        {sidebar}
        <div class="content">
          <div class="content-body">
            {content_html}
          </div>
        </div>
      </div>
    </div>
  </div>
  <script src="/assets/enhance.js"></script>
</body>
</html>"#
    )
}

// ============================================================================
// Spec page
// ============================================================================

// r[impl export.spec-page]
fn render_spec_page(
    spec_name: &str,
    impl_name: &str,
    spec_data: &ApiSpecData,
    config: &ApiConfig,
    include_sources: bool,
) -> Result<String> {
    let sidebar = render_outline_sidebar(&spec_data.outline);
    let content = spec_data
        .sections
        .iter()
        .map(|s| rewrite_spec_html(&s.html, include_sources))
        .collect::<Vec<_>>()
        .join("\n");
    let head_extras = spec_data.head_injections.join("\n");

    Ok(page_shell(
        &format!("Spec: {spec_name}"),
        spec_name,
        impl_name,
        "spec",
        config,
        include_sources,
        &sidebar,
        &format!(r#"<div class="spec-page markdown">{content}</div>"#),
        &head_extras,
    ))
}

fn render_outline_sidebar(outline: &[OutlineEntry]) -> String {
    if outline.is_empty() {
        return String::new();
    }
    let items = outline
        .iter()
        .map(|e| {
            let depth = e.level;
            let cov = &e.aggregated;

            let is_complete = cov.total > 0 && cov.impl_count == cov.total;
            let is_incomplete = cov.total > 0 && cov.impl_count < cov.total;
            let status_class = if is_complete {
                " is-complete"
            } else if is_incomplete {
                " is-incomplete"
            } else {
                ""
            };

            // Inline SVG arcs (matches the dashboard CoverageArc component)
            let impl_arc = coverage_arc_svg(
                cov.impl_count,
                cov.total,
                "var(--green)",
                &format!("Impl: {}/{}", cov.impl_count, cov.total),
            );
            let verify_arc = coverage_arc_svg(
                cov.verify_count,
                cov.total,
                "var(--blue)",
                &format!("Tests: {}/{}", cov.verify_count, cov.total),
            );
            let badges = if cov.total > 0 {
                format!(r#"<span class="toc-badges">{impl_arc}{verify_arc}</span>"#)
            } else {
                String::new()
            };

            // Indent via padding on the toc-row
            let indent_style = if depth > 1 {
                format!(
                    r#" style="padding-inline-start: {}rem""#,
                    (depth as usize - 1) as f32 * 0.75 + 0.5
                )
            } else {
                String::new()
            };

            format!(
                r##"<li class="toc-item depth-{depth}{status_class}">
  <a class="toc-row" href="#{slug}"{indent_style}>
    <span class="toc-link">{title}</span>
    {badges}
  </a>
</li>"##,
                slug = e.slug,
                title = html_escape(&e.title),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(r#"<ul class="outline-tree">{items}</ul>"#)
}

/// Generate an inline SVG coverage arc (mirrors the dashboard CoverageArc component).
fn coverage_arc_svg(count: usize, total: usize, color: &str, title: &str) -> String {
    if total == 0 {
        return String::new();
    }
    let size = 20.0f32;
    let radius = (size - 4.0) / 2.0; // 8.0
    let center = size / 2.0; // 10.0
    let circumference = 2.0 * std::f32::consts::PI * radius; // ~50.27

    if count == total {
        // Complete: filled circle with checkmark
        return format!(
            r#"<svg class="coverage-arc coverage-arc--complete" width="{size}" height="{size}" viewBox="0 0 {size} {size}" title="{title}">
  <circle cx="{center}" cy="{center}" r="{radius}" fill="{color}" opacity="0.15"/>
  <path d="M{x1} {center} l2.5 2.5 l5 -5" fill="none" stroke="{color}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
</svg>"#,
            x1 = center - 4.0,
        );
    }

    let pct = count as f32 / total as f32;
    let dash = pct * circumference;
    format!(
        r#"<svg class="coverage-arc" width="{size}" height="{size}" viewBox="0 0 {size} {size}" title="{title}">
  <circle cx="{center}" cy="{center}" r="{radius}" fill="none" stroke="var(--border)" stroke-width="1.5"/>
  <circle cx="{center}" cy="{center}" r="{radius}" fill="none" stroke="{color}" stroke-width="3" stroke-dasharray="{dash:.2} {circumference:.2}" stroke-linecap="round" transform="rotate(-90 {center} {center})"/>
</svg>"#
    )
}

// ============================================================================
// Coverage page
// ============================================================================

// r[impl export.coverage-page]
fn render_coverage_page(
    spec_name: &str,
    impl_name: &str,
    forward: &ApiSpecForward,
    config: &ApiConfig,
    include_sources: bool,
) -> Result<String> {
    let total = forward.rules.len();
    let covered = forward
        .rules
        .iter()
        .filter(|r| !r.impl_refs.is_empty())
        .count();
    let tested = forward
        .rules
        .iter()
        .filter(|r| !r.verify_refs.is_empty())
        .count();

    let impl_class = stat_class(covered, total);
    let test_class = stat_class(tested, total);

    let stats = format!(
        r#"<div class="stats-bar">
  <div class="stat">
    <span class="stat-label">Implemented</span>
    <span class="stat-value {impl_class}">{covered}/{total}</span>
  </div>
  <div class="stat">
    <span class="stat-label">Tested</span>
    <span class="stat-value {test_class}">{tested}/{total}</span>
  </div>
</div>"#
    );

    let rows = forward
        .rules
        .iter()
        .map(|rule| render_rule_row(spec_name, impl_name, rule, include_sources))
        .collect::<Vec<_>>()
        .join("\n");

    let table = format!(
        r#"<table class="rules-table">
  <thead><tr>
    <th>Rule</th>
    <th>Impl</th>
    <th>Tests</th>
  </tr></thead>
  <tbody>{rows}</tbody>
</table>"#
    );

    Ok(page_shell(
        &format!("Coverage: {spec_name}/{impl_name}"),
        spec_name,
        impl_name,
        "coverage",
        config,
        include_sources,
        "",
        &format!(r#"<div class="padded-page">{stats}{table}</div>"#),
        "",
    ))
}

/// Render a source reference as a link (when sources are exported) or plain text.
fn source_ref_html(
    spec_name: &str,
    impl_name: &str,
    file: &str,
    line: usize,
    css_class: &str,
    include_sources: bool,
) -> String {
    let fname = file.rsplit('/').next().unwrap_or(file);
    let label = format!("{fname}:{line}");
    if include_sources {
        format!(
            r#"<a class="rule-ref {css_class}" href="/{spec_name}/{impl_name}/sources/{file}.html#line-{line}">{label}</a>"#
        )
    } else {
        format!(r#"<span class="rule-ref {css_class}">{label}</span>"#)
    }
}

fn render_rule_row(
    spec_name: &str,
    impl_name: &str,
    rule: &ApiRule,
    include_sources: bool,
) -> String {
    let id = rule.id.to_string();

    let impl_refs = rule
        .impl_refs
        .iter()
        .map(|r| {
            source_ref_html(
                spec_name,
                impl_name,
                &r.file,
                r.line,
                "impl",
                include_sources,
            )
        })
        .collect::<Vec<_>>()
        .join("<br>");

    let verify_refs = rule
        .verify_refs
        .iter()
        .map(|r| {
            source_ref_html(
                spec_name,
                impl_name,
                &r.file,
                r.line,
                "verify",
                include_sources,
            )
        })
        .collect::<Vec<_>>()
        .join("<br>");

    format!(
        r#"<tr>
  <td><div class="rule-id-row"><a class="rule-id" href="/{spec_name}/{impl_name}/spec.html#r--{id}">{id}</a></div></td>
  <td class="rule-refs">{impl_refs}</td>
  <td class="rule-refs">{verify_refs}</td>
</tr>"#
    )
}

fn stat_class(count: usize, total: usize) -> &'static str {
    if total == 0 {
        return "good";
    }
    let pct = count * 100 / total;
    match pct {
        80..=100 => "good",
        50..=79 => "warn",
        _ => "bad",
    }
}

// ============================================================================
// Sources index page
// ============================================================================

// r[impl export.sources]
fn render_sources_index(
    spec_name: &str,
    impl_name: &str,
    reverse: &ApiReverseData,
    config: &ApiConfig,
) -> Result<String> {
    let cov_class = stat_class(reverse.covered_units, reverse.total_units);
    let stats = format!(
        r#"<div class="stats-bar">
  <div class="stat">
    <span class="stat-label">Units covered</span>
    <span class="stat-value {cov_class}">{}/{}</span>
  </div>
</div>"#,
        reverse.covered_units, reverse.total_units,
    );

    let rows = reverse
        .files
        .iter()
        .map(|f| {
            let cov_pct = if f.total_units > 0 {
                f.covered_units * 100 / f.total_units
            } else {
                0
            };
            let fill_class = match cov_pct {
                80..=100 => "high",
                50..=79 => "med",
                _ => "low",
            };
            format!(
                r#"<tr>
  <td class="rule-id"><a href="/{spec_name}/{impl_name}/sources/{path}.html">{path_escaped}</a></td>
  <td class="cov-bar-wrap">
    <div class="cov-bar"><div class="cov-bar-fill {fill_class}" style="width:{cov_pct}%"></div></div>
  </td>
  <td class="rule-refs">{covered}/{total} units</td>
</tr>"#,
                path = f.path,
                path_escaped = html_escape(&f.path),
                covered = f.covered_units,
                total = f.total_units,
                cov_pct = cov_pct,
                fill_class = fill_class,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let table = format!(
        r#"<table class="rules-table">
  <thead><tr><th>File</th><th>Coverage</th><th>Units</th></tr></thead>
  <tbody>{rows}</tbody>
</table>"#
    );

    Ok(page_shell(
        &format!("Sources: {spec_name}/{impl_name}"),
        spec_name,
        impl_name,
        "sources",
        config,
        true, // sources index is only written when include_sources is true
        "",
        &format!(r#"<div class="padded-page">{stats}{table}</div>"#),
        "",
    ))
}

// ============================================================================
// Source file page
// ============================================================================

fn render_file_page(
    spec_name: &str,
    impl_name: &str,
    file: &ApiFileData,
    config: &ApiConfig,
) -> Result<String> {
    let lines = split_html_lines(&file.html);

    // Which lines have rule annotations
    let mut annotated: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for unit in &file.units {
        if !unit.rule_refs.is_empty() {
            annotated.insert(unit.start_line);
        }
    }

    let code_lines = lines
        .iter()
        .enumerate()
        .map(|(i, line_html)| {
            let n = i + 1;
            let covered = if annotated.contains(&n) { " covered" } else { "" };
            format!(
                r##"<tr id="line-{n}" class="code-line{covered}" data-line="{n}"><td class="line-number">{n}</td><td class="line-gutter"></td><td class="line-content"><code>{line_html}</code></td></tr>"##
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let sidebar = render_units_sidebar(&file.units);
    let header = format!(
        r#"<div class="file-view-header">{}</div>"#,
        html_escape(&file.path)
    );
    let code_view = format!(
        r#"<div class="code-view"><table class="code-table"><tbody>{code_lines}</tbody></table></div>"#
    );

    Ok(page_shell(
        &format!("{} — {spec_name}/{impl_name}", file.path),
        spec_name,
        impl_name,
        "sources",
        config,
        true, // file pages are only written when include_sources is true
        &sidebar,
        &format!("{header}{code_view}"),
        "",
    ))
}

fn render_units_sidebar(units: &[ApiCodeUnit]) -> String {
    let items: Vec<_> = units
        .iter()
        .filter_map(|u| {
            let name = u.name.as_deref()?;
            Some(format!(
                r##"<li><a href="#line-{}"><span class="units-kind">{}</span> {}</a></li>"##,
                u.start_line,
                html_escape(&u.kind),
                html_escape(name),
            ))
        })
        .collect();
    if items.is_empty() {
        return String::new();
    }
    format!(r#"<ul class="units-list">{}</ul>"#, items.join("\n"))
}

/// Split arborium syntax-highlighted HTML into per-line fragments.
///
/// Arborium outputs `<span class="...">` and `</span>` tags. Spans can cross
/// line boundaries. This splits on `\n`, closing open spans at the end of each
/// line and reopening them on the next, so each returned string is self-contained.
fn split_html_lines(html: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut open_spans: Vec<String> = Vec::new();
    let mut rest = html;

    while !rest.is_empty() {
        if let Some(tag_start) = rest.find('<') {
            let text = &rest[..tag_start];
            for ch in text.chars() {
                if ch == '\n' {
                    for _ in &open_spans {
                        current.push_str("</span>");
                    }
                    lines.push(std::mem::take(&mut current));
                    for s in &open_spans {
                        current.push_str(s);
                    }
                } else {
                    current.push(ch);
                }
            }
            rest = &rest[tag_start..];

            if rest.starts_with("</span>") {
                current.push_str("</span>");
                open_spans.pop();
                rest = &rest["</span>".len()..];
            } else if rest.starts_with("<span") {
                if let Some(end) = rest.find('>') {
                    let tag = &rest[..end + 1];
                    current.push_str(tag);
                    open_spans.push(tag.to_string());
                    rest = &rest[end + 1..];
                } else {
                    break;
                }
            } else {
                current.push('<');
                rest = &rest[1..];
            }
        } else {
            for ch in rest.chars() {
                if ch == '\n' {
                    for _ in &open_spans {
                        current.push_str("</span>");
                    }
                    lines.push(std::mem::take(&mut current));
                    for s in &open_spans {
                        current.push_str(s);
                    }
                } else {
                    current.push(ch);
                }
            }
            break;
        }
    }

    if !current.is_empty() {
        for _ in &open_spans {
            current.push_str("</span>");
        }
        lines.push(current);
    }

    lines
}

// ============================================================================
// URL rewriting — SPA hrefs → static file paths
// ============================================================================

/// Rewrite daemon SPA links to static file paths and strip edit buttons.
///
/// - `href="/{spec}/{impl}/sources/{file}:{line}"`:
///   - `include_sources=true`  → `href="...{file}.html#line-{line}"`
///   - `include_sources=false` → demote `<a>` to `<span>` (no dead links)
/// - `href="/{spec}/{impl}/spec#{id}"` → `href="...spec.html#{id}"`
/// - `<button class="req-edit">` → stripped entirely
fn rewrite_spec_html(html: &str, include_sources: bool) -> String {
    let mut output = String::with_capacity(html.len());
    let mut rest = html;
    let mut open_demoted: usize = 0;

    while !rest.is_empty() {
        let Some(lt_pos) = rest.find('<') else {
            output.push_str(rest);
            break;
        };

        output.push_str(&rest[..lt_pos]);
        rest = &rest[lt_pos..];

        // Closing </a> — may need to become </span>
        if rest.starts_with("</a>") {
            if open_demoted > 0 {
                output.push_str("</span>");
                open_demoted -= 1;
            } else {
                output.push_str("</a>");
            }
            rest = &rest[4..];
            continue;
        }

        // Opening <a …>
        if rest.starts_with("<a ") || rest.starts_with("<a\t") || rest.starts_with("<a\n") {
            if let Some(tag_end) = find_open_tag_end(rest) {
                let full_tag = &rest[..tag_end + 1];
                rest = &rest[tag_end + 1..];

                if let Some(href) = extract_href_value(full_tag) {
                    if let Some((file_part, line_part)) = parse_sources_link(href) {
                        if include_sources {
                            let new_href = format!("{}.html#line-{}", file_part, line_part);
                            output.push_str(&replace_href_in_tag(full_tag, href, &new_href));
                        } else {
                            output.push_str(&demote_a_to_span(full_tag, href));
                            open_demoted += 1;
                        }
                        continue;
                    }
                    if href.contains("/spec#") {
                        let new_href = href.replacen("/spec#", "/spec.html#", 1);
                        output.push_str(&replace_href_in_tag(full_tag, href, &new_href));
                        continue;
                    }
                }

                output.push_str(full_tag);
                continue;
            }
            output.push('<');
            rest = &rest[1..];
            continue;
        }

        // <button …> — strip req-edit buttons
        if rest.starts_with("<button") {
            if let Some(tag_end) = find_open_tag_end(rest) {
                let open_tag = &rest[..tag_end + 1];
                if open_tag.contains("req-edit") {
                    let after = &rest[tag_end + 1..];
                    if let Some(close_pos) = after.find("</button>") {
                        rest = &after[close_pos + "</button>".len()..];
                        continue;
                    }
                }
                output.push_str(open_tag);
                rest = &rest[tag_end + 1..];
                continue;
            }
            output.push('<');
            rest = &rest[1..];
            continue;
        }

        output.push('<');
        rest = &rest[1..];
    }

    output
}

fn find_open_tag_end(tag: &str) -> Option<usize> {
    let mut in_quote = false;
    let mut quote_char = b'"';
    for (i, &b) in tag.as_bytes().iter().enumerate() {
        match b {
            b'"' | b'\'' if !in_quote => {
                in_quote = true;
                quote_char = b;
            }
            c if in_quote && c == quote_char => in_quote = false,
            b'>' if !in_quote => return Some(i),
            _ => {}
        }
    }
    None
}

fn extract_href_value(tag: &str) -> Option<&str> {
    let pos = tag.find(" href=\"")?;
    let after = &tag[pos + 7..];
    let end = after.find('"')?;
    Some(&after[..end])
}

fn parse_sources_link(href: &str) -> Option<(&str, &str)> {
    href.find("/sources/")?;
    let colon_pos = href.rfind(':')?;
    let line_part = &href[colon_pos + 1..];
    if line_part.is_empty() || !line_part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((&href[..colon_pos], line_part))
}

fn replace_href_in_tag(tag: &str, old_href: &str, new_href: &str) -> String {
    let old = format!(" href=\"{}\"", old_href);
    let new = format!(" href=\"{}\"", new_href);
    tag.replacen(&old, &new, 1)
}

fn demote_a_to_span(tag: &str, href: &str) -> String {
    let href_attr = format!(" href=\"{}\"", href);
    let without_href = tag.replacen(&href_attr, "", 1);
    if let Some(rest) = without_href.strip_prefix("<a") {
        format!("<span{rest}")
    } else {
        without_href
    }
}

// ============================================================================
// Utilities
// ============================================================================

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ============================================================================
// Embedded assets
// ============================================================================

/// JavaScript written to assets/enhance.js.
const ENHANCE_JS: &str = r##"(function () {
  function highlightLine() {
    document.querySelectorAll("tr.code-line.highlighted").forEach(function (el) {
      el.classList.remove("highlighted");
    });
    var hash = location.hash;
    if (hash.startsWith("#line-")) {
      var el = document.getElementById(hash.slice(1));
      if (el) {
        el.classList.add("highlighted");
        el.scrollIntoView({ block: "center" });
      }
    }
  }
  window.addEventListener("hashchange", highlightLine);
  document.addEventListener("DOMContentLoaded", function () {
    if (typeof lucide !== "undefined") lucide.createIcons();
    highlightLine();
  });

  document.addEventListener("click", function (e) {
    var btn = e.target.closest("[data-req-id]");
    if (btn && btn.classList.contains("req-copy")) {
      navigator.clipboard.writeText(btn.dataset.reqId).catch(function () {});
    }
  });

  // Mobile sidebar toggle
  var toggle = document.getElementById("sidebar-toggle");
  var sidebar = document.getElementById("sidebar");
  if (toggle && sidebar) {
    var backdrop = document.createElement("div");
    backdrop.className = "sidebar-backdrop";
    document.body.appendChild(backdrop);

    function openSidebar() { sidebar.classList.add("open"); backdrop.classList.add("open"); }
    function closeSidebar() { sidebar.classList.remove("open"); backdrop.classList.remove("open"); }

    toggle.addEventListener("click", function () {
      sidebar.classList.contains("open") ? closeSidebar() : openSidebar();
    });
    backdrop.addEventListener("click", closeSidebar);
  }
})();
"##;

/// Extra CSS appended to the compiled dashboard CSS.
/// Only what isn't already covered by the dashboard stylesheet.
const STATIC_EXTRA_CSS: &str = r#"
/* ── Static export extras ─────────────────────────────────────── */

/* spec-tab links (header pickers) */
a.spec-tab { text-decoration: none; display: inline-block; }

/* Spec content wrapper */
.spec-page {
  padding: var(--space-6) var(--space-8);
  max-width: 900px;
}

/* Padded wrapper for coverage / sources-index pages */
.padded-page {
  padding: var(--space-4) var(--space-6);
}

/* File-view header bar */
.file-view-header {
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--border);
  background: var(--bg-secondary);
  font-size: var(--text-sm);
  font-family: var(--font-mono);
  color: var(--fg-muted);
  font-variation-settings: "MONO" 1, "CASL" 0;
}

/* Units list in source-file sidebar */
.units-list {
  list-style: none;
  margin: 0;
  padding: var(--space-2);
}
.units-list li { border-radius: 4px; }
.units-list li:hover { background: var(--hover); }
.units-list a {
  display: flex;
  gap: var(--space-2);
  align-items: baseline;
  padding: var(--space-1-5) var(--space-2);
  color: var(--fg-muted);
  text-decoration: none;
  font-size: var(--text-xs);
}
.units-list a:hover { color: var(--fg); }
.units-kind { color: var(--fg-dim); font-size: var(--text-2xs); }

/* Coverage bar in sources-index table */
.cov-bar-wrap { min-width: 80px; }
.cov-bar {
  height: 6px;
  border-radius: 3px;
  background: var(--red-dim, rgba(244,67,54,0.12));
  overflow: hidden;
}
.cov-bar-fill { height: 100%; border-radius: 3px; }
.cov-bar-fill.high { background: var(--green); }
.cov-bar-fill.med  { background: var(--yellow); }
.cov-bar-fill.low  { background: var(--red); }

/* r[impl export.mobile-sidebar] Mobile sidebar toggle */
.sidebar-toggle {
  display: none;
  background: none;
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--fg-muted);
  cursor: pointer;
  padding: var(--space-1) var(--space-2);
  align-items: center;
  justify-content: center;
}
.sidebar-toggle:hover { color: var(--fg); background: var(--hover); }
.sidebar-toggle svg { width: 18px; height: 18px; }

@media (max-width: 768px) {
  .sidebar-toggle { display: inline-flex; }

  .sidebar {
    position: fixed;
    top: 0;
    left: 0;
    bottom: 0;
    z-index: 100;
    width: 300px;
    max-width: 85vw;
    transform: translateX(-100%);
    transition: transform 0.2s ease;
    background: var(--bg);
    border-right: 1px solid var(--border);
    overflow-y: auto;
  }
  .sidebar.open { transform: translateX(0); }

  .sidebar-backdrop {
    display: none;
    position: fixed;
    inset: 0;
    z-index: 99;
    background: rgba(0, 0, 0, 0.4);
  }
  .sidebar-backdrop.open { display: block; }
}

/* Landing page */
.landing-page {
  padding: var(--space-6) var(--space-8);
  max-width: 1000px;
}
.landing-readme { margin-bottom: var(--space-8); }
.landing-grid-section h2 {
  font-size: var(--text-lg);
  margin-bottom: var(--space-4);
  color: var(--fg);
}
.landing-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: var(--space-4);
}
.spec-card {
  display: block;
  text-decoration: none;
  color: var(--fg);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: var(--space-4);
  transition: border-color 0.15s, box-shadow 0.15s;
}
.spec-card:hover {
  border-color: var(--accent);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
}
.spec-card-header {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
  margin-bottom: var(--space-3);
}
.spec-card-name {
  font-size: var(--text-base);
  font-weight: 600;
}
.spec-card-impl {
  font-size: var(--text-sm);
  color: var(--fg-muted);
}
.spec-card-stats {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}
.spec-card-stat {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--fg-muted);
}
"#;
