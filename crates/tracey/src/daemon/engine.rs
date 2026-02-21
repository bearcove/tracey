//! Core engine for the tracey daemon.
//!
//! r[impl daemon.state.vfs-overlay]
//! r[impl daemon.state.blocking-rebuild]
//! r[impl server.state.shared]
//! r[impl server.state.version]
//!
//! The engine owns the `DashboardData`, file watcher, and VFS overlay.
//! It provides blocking rebuild semantics - all requests wait during rebuild.

use eyre::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, watch};
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::data::{DashboardData, FileOverlay, build_dashboard_data_with_overlay};

/// The core tracey engine.
///
/// Owns the dashboard data, file watcher, and VFS overlay.
/// Provides blocking rebuild semantics via RwLock.
#[allow(dead_code)]
pub struct Engine {
    /// Current dashboard data, protected by RwLock for blocking rebuilds
    data: Arc<RwLock<Arc<DashboardData>>>,
    /// Sender for broadcasting data updates to subscribers
    update_tx: watch::Sender<Arc<DashboardData>>,
    /// Receiver for getting current data
    update_rx: watch::Receiver<Arc<DashboardData>>,
    /// VFS overlay for open documents (from LSP)
    vfs: Arc<RwLock<FileOverlay>>,
    /// Project root directory
    project_root: PathBuf,
    /// Path to config file
    config_path: PathBuf,
    /// Current config (reloaded on changes)
    config: Arc<RwLock<Config>>,
    /// Version counter
    version: Arc<std::sync::atomic::AtomicU64>,
    /// Current config error (if config file has errors)
    config_error: Arc<RwLock<Option<String>>>,
}

impl Engine {
    /// Create a new engine for the given project root.
    pub async fn new(project_root: PathBuf, config_path: PathBuf) -> Result<Self> {
        // Check for deprecated config files first
        let deprecated_error = Self::check_deprecated_configs(&project_root);

        // Load initial config - record errors but continue with empty config
        let (config, config_error) = if let Some(err) = deprecated_error {
            // Deprecated config found - use empty config and record error
            (Config::default(), Some(err))
        } else {
            match tokio::fs::read_to_string(&config_path).await {
                Ok(content) => match facet_styx::from_str(&content) {
                    Ok(config) => (config, None),
                    Err(e) => {
                        // Config has errors - use empty config and record error
                        let err =
                            format!("Config file {} has errors:\n{}", config_path.display(), e);
                        (Config::default(), Some(err))
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    info!(
                        "Config file {} not found, starting with empty config",
                        config_path.display()
                    );
                    (Config::default(), None)
                }
                Err(e) => {
                    // Can't read config - use empty config and record error
                    let err = format!("Config file {} not readable: {}", config_path.display(), e);
                    (Config::default(), Some(err))
                }
            }
        };

        // Build initial data
        let overlay = FileOverlay::new();
        let data =
            build_dashboard_data_with_overlay(&project_root, &config, 1, false, &overlay).await?;
        let data = Arc::new(data);

        // Create watch channel for broadcasting updates
        let (update_tx, update_rx) = watch::channel(Arc::clone(&data));

        Ok(Self {
            data: Arc::new(RwLock::new(data)),
            update_tx,
            update_rx,
            vfs: Arc::new(RwLock::new(overlay)),
            project_root,
            config_path,
            config: Arc::new(RwLock::new(config)),
            version: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            config_error: Arc::new(RwLock::new(config_error)),
        })
    }

    /// Get the current dashboard data.
    ///
    /// This acquires a read lock, blocking if a rebuild is in progress.
    pub async fn data(&self) -> Arc<DashboardData> {
        self.data.read().await.clone()
    }

    /// Get a receiver for data updates.
    #[allow(dead_code)]
    pub fn subscribe(&self) -> watch::Receiver<Arc<DashboardData>> {
        self.update_rx.clone()
    }

    /// Get the current version number.
    pub fn version(&self) -> u64 {
        self.version.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Register a file in the VFS overlay (from LSP didOpen).
    ///
    /// r[impl daemon.vfs.open]
    pub async fn vfs_open(&self, path: PathBuf, content: String) {
        let mut vfs = self.vfs.write().await;
        vfs.insert(path.clone(), content);
        debug!("VFS: opened {}", path.display());
        // Trigger rebuild
        drop(vfs);
        if let Err(e) = self.rebuild().await {
            error!("Rebuild failed after vfs_open: {}", e);
        }
    }

    /// Update a file in the VFS overlay (from LSP didChange).
    ///
    /// r[impl daemon.vfs.change]
    pub async fn vfs_change(&self, path: PathBuf, content: String) {
        let mut vfs = self.vfs.write().await;
        vfs.insert(path.clone(), content);
        debug!("VFS: changed {}", path.display());
        // Trigger rebuild
        drop(vfs);
        if let Err(e) = self.rebuild().await {
            error!("Rebuild failed after vfs_change: {}", e);
        }
    }

    /// Remove a file from the VFS overlay (from LSP didClose).
    ///
    /// r[impl daemon.vfs.close]
    pub async fn vfs_close(&self, path: PathBuf) {
        let mut vfs = self.vfs.write().await;
        vfs.remove(&path);
        debug!("VFS: closed {}", path.display());
        // Trigger rebuild
        drop(vfs);
        if let Err(e) = self.rebuild().await {
            error!("Rebuild failed after vfs_close: {}", e);
        }
    }

    /// Force a rebuild of the dashboard data.
    ///
    /// This acquires a write lock, blocking all reads until complete.
    /// Config errors are recorded but don't fail the rebuild - the previous
    /// config is retained.
    pub async fn rebuild(&self) -> Result<(u64, Duration)> {
        let start = Instant::now();

        // Reload config - record errors but continue with current config
        let (config, new_config_error) = match tokio::fs::read_to_string(&self.config_path).await {
            Ok(content) => match facet_styx::from_str(&content) {
                Ok(config) => (Some(config), None),
                Err(e) => {
                    let error_msg = format!(
                        "Config file {} has errors: {}",
                        self.config_path.display(),
                        e
                    );
                    warn!("{}", error_msg);
                    (None, Some(error_msg))
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Config file was deleted - use empty config
                info!(
                    "Config file {} not found, using empty config",
                    self.config_path.display()
                );
                (Some(Config::default()), None)
            }
            Err(e) => {
                let error_msg = format!(
                    "Config file {} not readable: {}",
                    self.config_path.display(),
                    e
                );
                warn!("{}", error_msg);
                (None, Some(error_msg))
            }
        };

        // Use new config if valid, otherwise keep the current one
        let config = match config {
            Some(cfg) => cfg,
            None => self.config.read().await.clone(),
        };

        // Update config error state
        {
            let mut err = self.config_error.write().await;
            *err = new_config_error;
        }

        // Get current VFS overlay
        let overlay = self.vfs.read().await.clone();

        // Increment version
        let new_version = self
            .version
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;

        // Build new data (this is the expensive part)
        let new_data = build_dashboard_data_with_overlay(
            &self.project_root,
            &config,
            new_version,
            true,
            &overlay,
        )
        .await?;
        let new_data = Arc::new(new_data);

        // Acquire write lock and update (blocks all reads)
        {
            let mut data = self.data.write().await;
            *data = Arc::clone(&new_data);
        }

        // Update config
        {
            let mut cfg = self.config.write().await;
            *cfg = config;
        }

        // Broadcast to subscribers
        let _ = self.update_tx.send(new_data);

        let elapsed = start.elapsed();
        info!(
            "Rebuild completed in {:?} (version {})",
            elapsed, new_version
        );

        Ok((new_version, elapsed))
    }

    /// Get the project root path.
    #[allow(dead_code)]
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Get the config path.
    #[allow(dead_code)]
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// Get the current config.
    #[allow(dead_code)]
    pub async fn config(&self) -> Config {
        self.config.read().await.clone()
    }

    /// Get the current config error, if any.
    pub async fn config_error(&self) -> Option<String> {
        self.config_error.read().await.clone()
    }

    /// Check for deprecated config files (YAML, KDL) and return an error message if found.
    fn check_deprecated_configs(project_root: &Path) -> Option<String> {
        let kdl_config = project_root.join(".config/tracey/config.kdl");
        let yaml_config = project_root.join(".config/tracey/config.yaml");

        let deprecated = if kdl_config.exists() {
            Some(("KDL", kdl_config))
        } else if yaml_config.exists() {
            Some(("YAML", yaml_config))
        } else {
            None
        };

        deprecated.map(|(format, path)| {
            let old_name = path.file_name().unwrap().to_string_lossy();
            format!(
                "Found deprecated config file: {path}\n\n\
                 Tracey now uses Styx configuration format.\n\n\
                 To migrate:\n\
                   1. Rename {old_name} to config.styx\n\
                   2. Convert the contents from {format} to Styx format\n\n\
                 Example Styx config:\n\n\
                 ========================================\n\
                 {example}\
                 ========================================",
                path = path.display(),
                example = indoc::indoc! {r#"
                    @schema {id crate:tracey-config@1, cli tracey}

                    specs (
                      {
                        name my-spec
                        include (docs/**/*.md)
                        impls (
                          {
                            name rust
                            include (src/**/*.rs)
                          }
                        )
                      }
                    )
                "#},
            )
        })
    }
}
