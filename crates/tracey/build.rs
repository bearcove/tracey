//! Build script for tracey - generates code and builds the dashboard

use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    // Generate TypeScript types for the dashboard
    generate_typescript_types();

    // Build dashboard (after TS types are generated)
    build_dashboard();
}

fn generate_typescript_types() {
    use facet_typescript::TypeScriptGenerator;
    use tracey_api::*;

    println!("cargo:rerun-if-changed=../tracey-api/src/lib.rs");

    let mut generator = TypeScriptGenerator::new();

    // Add all API types
    generator.add_type::<GitStatus>();
    generator.add_type::<ApiConfig>();
    generator.add_type::<ApiSpecInfo>();
    generator.add_type::<ApiForwardData>();
    generator.add_type::<ApiSpecForward>();
    generator.add_type::<ApiRule>();
    generator.add_type::<ApiCodeRef>();
    generator.add_type::<ApiReverseData>();
    generator.add_type::<ApiFileEntry>();
    generator.add_type::<ApiFileData>();
    generator.add_type::<ApiCodeUnit>();
    generator.add_type::<SpecSection>();
    generator.add_type::<OutlineCoverage>();
    generator.add_type::<OutlineEntry>();
    generator.add_type::<ApiSpecData>();
    generator.add_type::<ValidationResult>();
    generator.add_type::<ValidationError>();

    // Generate TypeScript code
    let typescript = generator.finish();

    // Add header comment
    let output = format!(
        "// This file is auto-generated from tracey-api Rust types\n\
         // DO NOT EDIT MANUALLY - changes will be overwritten on build\n\
         \n\
         {}\n",
        typescript
    );

    // Write to dashboard/src/api-types.ts
    let output_path = Path::new("src/bridge/http/dashboard/src/api-types.ts");
    fs::write(output_path, output).expect("Failed to write TypeScript types");
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
