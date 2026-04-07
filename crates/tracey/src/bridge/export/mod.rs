//! Static site export for tracey spec coverage data.
//!
//! Produces a directory of HTML files that can be served by any static file
//! host. No daemon or JavaScript framework is required to view the exported
//! pages.

mod components;
mod pages;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr, eyre};
use tracey_api::{ApiSpecData, ApiSpecForward};

// r[impl export.output.directory]
// r[impl export.output.overwrite]
// r[impl export.output.assets]
pub async fn run(
    root: Option<PathBuf>,
    _config_path: PathBuf,
    output: PathBuf,
    _include_sources: bool,
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

    // r[impl export.cli.create-dir]
    std::fs::create_dir_all(&output)
        .wrap_err_with(|| format!("creating output directory {}", output.display()))?;

    // r[impl export.cli.logging]
    write_assets(&output)?;

    // Build the sidebar tree from all specs
    let mut sidebar_entries: Vec<SidebarSpec> = Vec::new();
    let mut all_spec_data: Vec<SpecExportData> = Vec::new();

    for spec_info in &config.specs {
        for impl_name in &spec_info.implementations {
            let spec_name = &spec_info.name;
            eprintln!("  Fetching {spec_name} × {impl_name}…");

            let forward = client
                .forward(spec_name.clone(), impl_name.clone())
                .await
                .map_err(|e| eyre!("forward RPC failed for {spec_name}/{impl_name}: {:?}", e))?
                .ok_or_else(|| eyre!("no forward data for {spec_name}/{impl_name}"))?;

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

            sidebar_entries.push(build_sidebar_spec(spec_name, &spec_content));

            all_spec_data.push(SpecExportData {
                spec_name: spec_name.clone(),
                impl_name: impl_name.clone(),
                forward,
                spec_content,
            });
        }
    }

    // Read README
    let readme_html = read_readme(&project_root).await;

    // Write landing page
    let landing = pages::landing_page(&readme_html, &all_spec_data, &sidebar_entries);
    let landing_path = output.join("index.html");
    std::fs::write(&landing_path, landing.into_string()).wrap_err("writing index.html")?;
    eprintln!("  wrote {}", landing_path.display());

    // Write one page per spec (all sections concatenated)
    for data in &all_spec_data {
        let spec_dir = output.join(&data.spec_name);
        std::fs::create_dir_all(&spec_dir)
            .wrap_err_with(|| format!("creating directory {}", spec_dir.display()))?;

        let page = pages::spec_page(
            &data.spec_name,
            &data.impl_name,
            &data.forward,
            &data.spec_content,
            &sidebar_entries,
            &format!("{}/index.html", data.spec_name),
        );
        let page_path = spec_dir.join("index.html");
        std::fs::write(&page_path, page.into_string())
            .wrap_err_with(|| format!("writing {}", page_path.display()))?;
        eprintln!("  wrote {}", page_path.display());
    }

    eprintln!("\nDone! Static site written to: {}", output.display());
    Ok(())
}

/// Data collected for one spec×impl pair
pub(crate) struct SpecExportData {
    pub spec_name: String,
    pub impl_name: String,
    pub forward: ApiSpecForward,
    pub spec_content: ApiSpecData,
}

/// A spec in the sidebar tree
#[derive(Debug, Clone)]
pub(crate) struct SidebarSpec {
    pub name: String,
    pub href: String,
    pub headings: Vec<SidebarHeading>,
}

/// A heading in the sidebar
#[derive(Debug, Clone)]
pub(crate) struct SidebarHeading {
    pub title: String,
    pub slug: String,
    pub level: u8,
}

fn build_sidebar_spec(spec_name: &str, spec_content: &ApiSpecData) -> SidebarSpec {
    let headings: Vec<SidebarHeading> = spec_content
        .outline
        .iter()
        .filter(|e| e.level <= 2)
        .map(|e| SidebarHeading {
            title: e.title.clone(),
            slug: e.slug.clone(),
            level: e.level,
        })
        .collect();

    SidebarSpec {
        name: spec_name.to_string(),
        href: format!("{spec_name}/index.html"),
        headings,
    }
}

fn write_assets(output: &Path) -> Result<()> {
    let assets_dir = output.join("assets");
    std::fs::create_dir_all(&assets_dir)
        .wrap_err_with(|| format!("creating assets directory {}", assets_dir.display()))?;

    // r[impl export.style.dashboard-css]
    let css = format!("{}\n{}", crate::bridge::http::INDEX_CSS, EXTRA_CSS);
    std::fs::write(assets_dir.join("style.css"), css).wrap_err("writing assets/style.css")?;
    eprintln!("  wrote {}", assets_dir.join("style.css").display());

    std::fs::write(assets_dir.join("enhance.js"), ENHANCE_JS)
        .wrap_err("writing assets/enhance.js")?;
    eprintln!("  wrote {}", assets_dir.join("enhance.js").display());

    Ok(())
}

async fn read_readme(project_root: &Path) -> Option<String> {
    let readme_path = project_root.join("README.md");
    let content = std::fs::read_to_string(&readme_path).ok()?;
    let options = marq::RenderOptions::default();
    let doc = marq::render(&content, &options).await.ok()?;
    Some(doc.html)
}

/// Compute the relative path from a page to the site root.
/// e.g. "spec_name/index.html" -> ".."
/// e.g. "index.html" -> "."
pub(crate) fn relative_root(page_path: &str) -> String {
    let depth = page_path.matches('/').count();
    if depth == 0 {
        ".".to_string()
    } else {
        (0..depth).map(|_| "..").collect::<Vec<_>>().join("/")
    }
}

static EXTRA_CSS: &str = include_str!("extra.css");
static ENHANCE_JS: &str = include_str!("enhance.js");
