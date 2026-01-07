//! Core engine for the tracey daemon.
//!
//! The engine owns the `DashboardData`, file watcher, and VFS overlay.
//! It provides blocking rebuild semantics - all requests wait during rebuild.

use eyre::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, watch};
use tracing::{debug, error, info};

use crate::config::Config;
use crate::serve::{DashboardData, FileOverlay, build_dashboard_data_with_overlay};

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
}

impl Engine {
    /// Create a new engine for the given project root.
    pub async fn new(project_root: PathBuf, config_path: PathBuf) -> Result<Self> {
        // Load initial config
        let config_content = tokio::fs::read_to_string(&config_path).await?;
        let config: Config = facet_kdl::from_str(&config_content)?;

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
    pub async fn rebuild(&self) -> Result<(u64, Duration)> {
        let start = Instant::now();

        // Reload config
        let config_content = tokio::fs::read_to_string(&self.config_path).await?;
        let config: Config = facet_kdl::from_str(&config_content)?;

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
}
