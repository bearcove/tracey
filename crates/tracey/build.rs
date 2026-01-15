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

    // Check if node is available
    let node_check = Command::new("node").arg("--version").output();

    match node_check {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            eprintln!("Found node {}", version.trim());
        }
        _ => {
            panic!(
                "\n\
                Node.js is required but not found!\n\
                \n\
                Install Node.js using one of the following methods:\n\
                \n\
                  # On macOS with Homebrew:\n\
                  brew install node\n\
                \n\
                  # On Windows with Scoop:\n\
                  scoop install nodejs\n\
                \n\
                  # On Windows with Chocolatey:\n\
                  choco install nodejs\n\
                \n\
                  # Using fnm (Fast Node Manager):\n\
                  curl -fsSL https://fnm.vercel.app/install | bash\n\
                  fnm install --lts\n\
                \n\
                  # Using nvm (Node Version Manager):\n\
                  curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash\n\
                  nvm install --lts\n\
                \n\
                See https://nodejs.org/en/download for more options.\n"
            );
        }
    }

    // Check if pnpm is available
    let pnpm_check = Command::new("pnpm").arg("--version").output();

    match pnpm_check {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            eprintln!("Found pnpm {}", version.trim());
        }
        _ => {
            panic!(
                "\n\
                pnpm is required but not found!\n\
                \n\
                Install pnpm using one of the following methods:\n\
                \n\
                  # Using npm (Node.js must be installed):\n\
                  npm install -g pnpm\n\
                \n\
                  # Using Corepack (Node.js 16.13+ includes Corepack):\n\
                  corepack enable pnpm\n\
                \n\
                  # On macOS with Homebrew:\n\
                  brew install pnpm\n\
                \n\
                  # On Windows with Scoop:\n\
                  scoop install pnpm\n\
                \n\
                  # Standalone script (POSIX):\n\
                  curl -fsSL https://get.pnpm.io/install.sh | sh -\n\
                \n\
                  # Standalone script (Windows PowerShell):\n\
                  iwr https://get.pnpm.io/install.ps1 -useb | iex\n\
                \n\
                See https://pnpm.io/installation for more options.\n"
            );
        }
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
