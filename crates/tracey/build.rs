//! Build script for tracey - builds the dashboard and generates roam dispatcher

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    // Generate roam dispatcher code
    generate_roam_dispatcher();

    // Build dashboard
    build_dashboard();
}

fn generate_roam_dispatcher() {
    println!("cargo:rerun-if-changed=../tracey-proto/src/lib.rs");

    let detail = tracey_proto::tracey_daemon_service_detail();
    let options = roam_codegen::targets::rust::RustCodegenOptions { tracing: true };
    let code = roam_codegen::targets::rust::generate_service_with_options(&detail, &options);

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("tracey_daemon_generated.rs");
    fs::write(&dest_path, code).unwrap();
}

fn build_dashboard() {
    // Dashboard is colocated with the HTTP bridge
    let dashboard_dir = Path::new("src/bridge/http/dashboard");
    let dist_dir = dashboard_dir.join("dist");

    // Re-run if dashboard source changes
    println!("cargo:rerun-if-changed=src/bridge/http/dashboard/src");
    println!("cargo:rerun-if-changed=src/bridge/http/dashboard/index.html");
    println!("cargo:rerun-if-changed=src/bridge/http/dashboard/package.json");
    println!("cargo:rerun-if-changed=src/bridge/http/dashboard/vite.config.ts");
    // Re-run if output is missing (so deleting dist triggers rebuild)
    println!("cargo:rerun-if-changed=src/bridge/http/dashboard/dist/index.html");
    println!("cargo:rerun-if-changed=src/bridge/http/dashboard/dist/assets/index.js");
    println!("cargo:rerun-if-changed=src/bridge/http/dashboard/dist/assets/index.css");

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
