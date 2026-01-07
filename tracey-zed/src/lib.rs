//! Tracey extension for Zed editor.
//!
//! r[impl zed.extension.manifest]
//! r[impl zed.extension.language-server]
//!
//! This extension provides language server support for tracey, enabling
//! requirement traceability features in Zed.

use std::fs;
use zed_extension_api::{self as zed, LanguageServerId, Result};

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
    ///
    /// Ensure the tracey binary is installed, downloading if necessary.
    fn ensure_binary_installed(&mut self, language_server_id: &LanguageServerId) -> Result<String> {
        // Return cached path if we have it
        if let Some(path) = &self.cached_binary_path {
            return Ok(path.clone());
        }

        // Check if binary already exists in extension directory
        let binary_path = format!("./{}", binary_name());
        if fs::metadata(&binary_path).is_ok() {
            self.cached_binary_path = Some(binary_path.clone());
            return Ok(binary_path);
        }

        // Need to download - update status
        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

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
        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::Downloading,
        );

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
        _worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let binary_path = self.ensure_binary_installed(language_server_id)?;

        Ok(zed::Command {
            command: binary_path,
            args: vec!["lsp".to_string()],
            env: Default::default(),
        })
    }
}

zed::register_extension!(TraceyExtension);
