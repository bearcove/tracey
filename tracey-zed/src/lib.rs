//! Tracey extension for Zed editor.
//!
//! r[impl zed.extension.manifest+2]
//! r[impl zed.extension.language-server]
//! r[impl zed.extension.context-server]
//! r[impl zed.filetypes.source]
//! r[impl zed.filetypes.spec]
//! r[impl zed.filetypes.config]
//! r[impl zed.install.manual]
//! r[impl zed.install.extension-registry]
//!
//! This extension provides both language server (LSP) and context server (MCP)
//! support for tracey, enabling requirement traceability features in Zed.
//!
//! - The **LSP** provides diagnostics, hover, go-to-definition, etc. for
//!   requirement annotations in source code and spec files.
//! - The **MCP context server** exposes tracey's query tools (status, uncovered,
//!   untested, stale, unmapped, rule, config, reload, validate, etc.) to Zed's
//!   AI assistant panel.
//!
//! File type activation is configured in `extension.toml` via the `languages` list.
//! The context server is configured in `extension.toml` via the `context_servers` table.

use std::fs;
use zed_extension_api::{self as zed, ContextServerId, LanguageServerId, Project, Result};

/// The GitHub repository for tracey releases.
const GITHUB_REPO: &str = "bearcove/tracey";

/// Get the binary name for the current platform.
fn binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "tracey.exe"
    } else {
        "tracey"
    }
}

/// Get the asset name pattern for the current platform.
fn asset_name_pattern() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "tracey-aarch64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "tracey-x86_64-apple-darwin"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "tracey-x86_64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "tracey-aarch64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "tracey-x86_64-pc-windows-msvc"
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        "tracey-unknown"
    }
}

struct TraceyExtension {
    /// Cached path to the installed binary.
    cached_binary_path: Option<String>,
}

impl TraceyExtension {
    /// r[impl zed.install.binary]
    /// r[impl zed.install.binary-options]
    ///
    /// Ensure the tracey binary is available, downloading from GitHub releases
    /// if necessary. Both the LSP and MCP context server share this binary.
    ///
    /// When called from the language server path, `language_server_id` is
    /// provided so we can report installation status in the Zed UI. When called
    /// from the context server path we pass `None` (there is no status API for
    /// context servers).
    fn ensure_binary(
        &mut self,
        language_server_id: Option<&LanguageServerId>,
        worktree: Option<&zed::Worktree>,
    ) -> Result<String> {
        // Return cached path if we have it
        if let Some(path) = &self.cached_binary_path {
            return Ok(path.clone());
        }

        // Check if binary exists in PATH first (for local development).
        // Only possible when we have a worktree handle (LSP path).
        if let Some(wt) = worktree {
            if let Some(path) = wt.which(binary_name()) {
                self.cached_binary_path = Some(path.clone());
                return Ok(path);
            }
        }

        // Check if binary already exists in extension directory
        let binary_path = format!("./{}", binary_name());
        if fs::metadata(&binary_path).is_ok() {
            self.cached_binary_path = Some(binary_path.clone());
            return Ok(binary_path);
        }

        // Need to download — update status if we have an LSP id
        if let Some(lsp_id) = language_server_id {
            zed::set_language_server_installation_status(
                lsp_id,
                &zed::LanguageServerInstallationStatus::CheckingForUpdate,
            );
        }

        // Get latest release from GitHub
        let release = zed::latest_github_release(
            GITHUB_REPO,
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )
        .map_err(|e| format!("Failed to fetch latest release: {e}"))?;

        // Find the asset for our platform
        let asset_pattern = asset_name_pattern();
        let asset = release
            .assets
            .iter()
            .find(|a| a.name.contains(asset_pattern) && a.name.ends_with(".tar.gz"))
            .ok_or_else(|| {
                format!(
                    "No release asset found for platform '{}' in release {}",
                    asset_pattern, release.version
                )
            })?;

        // Download the asset
        if let Some(lsp_id) = language_server_id {
            zed::set_language_server_installation_status(
                lsp_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );
        }

        let download_path = format!("./tracey-{}.tar.gz", release.version);
        zed::download_file(
            &asset.download_url,
            &download_path,
            zed::DownloadedFileType::GzipTar,
        )
        .map_err(|e| format!("Failed to download tracey: {e}"))?;

        // Make binary executable
        zed::make_file_executable(&binary_path)
            .map_err(|e| format!("Failed to make tracey executable: {e}"))?;

        // Cache and return the path
        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

impl zed::Extension for TraceyExtension {
    fn new() -> Self {
        TraceyExtension {
            cached_binary_path: None,
        }
    }

    /// r[impl zed.extension.language-server-config]
    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let binary_path = self.ensure_binary(Some(language_server_id), Some(worktree))?;

        Ok(zed::Command {
            command: binary_path,
            args: vec!["lsp".to_string()],
            env: Default::default(),
        })
    }

    /// r[impl zed.extension.context-server-config]
    ///
    /// Returns the command used to start the tracey MCP context server.
    /// Zed will spawn this process and communicate with it over stdio using
    /// the Model Context Protocol.
    ///
    /// Note: zed_extension_api 0.5 exposes `Project::worktree_ids()` but no
    /// resolver back to a `Worktree`, so `which()` is unreachable here and the
    /// `$PATH` probe in `ensure_binary` is skipped. If the MCP server starts
    /// before the LSP, a `tracey` in `$PATH` is ignored in favour of the
    /// downloaded/extension-dir binary. Dev users wanting their local build
    /// should open a source file first so the LSP path caches the `$PATH` hit.
    fn context_server_command(
        &mut self,
        _context_server_id: &ContextServerId,
        _project: &Project,
    ) -> Result<zed::Command> {
        let binary_path = self.ensure_binary(None, None)?;

        Ok(zed::Command {
            command: binary_path,
            args: vec!["mcp".to_string()],
            env: Default::default(),
        })
    }
}

zed::register_extension!(TraceyExtension);
