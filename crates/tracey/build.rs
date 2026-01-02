//! Build script for tracey - builds the dashboard if needed

use std::path::Path;
use std::process::Command;

fn main() {
    let dashboard_dir = Path::new("dashboard");
    let dist_dir = dashboard_dir.join("dist");

    // Re-run if dashboard source changes
    println!("cargo:rerun-if-changed=dashboard/src");
    println!("cargo:rerun-if-changed=dashboard/index.html");
    println!("cargo:rerun-if-changed=dashboard/package.json");
    println!("cargo:rerun-if-changed=dashboard/vite.config.ts");

    // Skip build if dist already exists (for faster incremental builds)
    // To force rebuild, delete the dist directory
    if dist_dir.join("index.html").exists()
        && dist_dir.join("assets/index.js").exists()
        && dist_dir.join("assets/index.css").exists()
    {
        return;
    }

    eprintln!("Building dashboard with pnpm...");

    // Install dependencies if needed
    let status = Command::new("pnpm")
        .args(["install", "--frozen-lockfile"])
        .current_dir(dashboard_dir)
        .status()
        .expect("Failed to run pnpm install - is pnpm installed?");

    if !status.success() {
        panic!("pnpm install failed");
    }

    // Build the dashboard
    let status = Command::new("pnpm")
        .args(["run", "build"])
        .current_dir(dashboard_dir)
        .status()
        .expect("Failed to run pnpm build");

    if !status.success() {
        panic!("pnpm build failed");
    }
}
