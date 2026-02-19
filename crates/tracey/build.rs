//! Build script for tracey - generates code and builds the dashboard

use std::fs;
use std::path::Path;
use std::process::Command;
use time::OffsetDateTime;
use time::format_description::well_known::Iso8601;

/// Creates a Command that will work cross-platform.
/// On Windows, runs through `cmd /c` to handle PATH resolution for .cmd/.ps1/.exe variants.
/// On Unix, runs the command directly.
fn shell_command(program: &str) -> Command {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/c", program]);
        cmd
    }
    #[cfg(not(windows))]
    {
        Command::new(program)
    }
}

fn main() {
    emit_tracey_version_metadata();

    // Generate Styx schema for config (embedded in binary for tooling discovery)
    generate_styx_schema();

    // Generate TypeScript types for the dashboard
    generate_typescript_types();

    // Build dashboard (after TS types are generated)
    build_dashboard();
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let mut cmd = shell_command(program);
    cmd.args(args);
    cmd.output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn format_utc_date(date_time: OffsetDateTime) -> Option<String> {
    date_time
        .format(&Iso8601::DATE)
        .ok()
        .map(|value| value.to_string())
}

fn source_date_epoch() -> Option<String> {
    let epoch = env_var("SOURCE_DATE_EPOCH")?;

    // Zip-based packagers sometimes set the DOS epoch sentinel (1980-01-01),
    // which is not meaningful build metadata for users.
    if epoch == "315532800" {
        return None;
    }

    let timestamp = epoch.parse::<i64>().ok()?;
    let date_time = OffsetDateTime::from_unix_timestamp(timestamp).ok()?;
    format_utc_date(date_time)
}

fn emit_tracey_version_metadata() {
    let git_commit = command_output("git", &["rev-parse", "--short=12", "HEAD"]).or_else(|| {
        [
            "TRACEY_GIT_COMMIT",
            "GITHUB_SHA",
            "CI_COMMIT_SHA",
            "SOURCE_COMMIT",
        ]
        .iter()
        .find_map(|name| env_var(name))
        .map(|commit| commit.chars().take(12).collect::<String>())
    });

    if let Some(git_commit) = git_commit {
        println!("cargo:rustc-env=TRACEY_GIT_COMMIT={git_commit}");
    }

    let build_date = env_var("TRACEY_BUILD_DATE")
        .or_else(source_date_epoch)
        .or_else(|| {
            command_output(
                "git",
                &[
                    "show",
                    "-s",
                    "--date=format:%Y-%m-%d",
                    "--format=%cd",
                    "HEAD",
                ],
            )
        })
        .or_else(|| format_utc_date(OffsetDateTime::now_utc()));

    if let Some(build_date) = build_date {
        println!("cargo:rustc-env=TRACEY_BUILD_DATE={build_date}");
    }
}

fn generate_styx_schema() {
    facet_styx::GenerateSchema::<tracey_config::Config>::new()
        .crate_name("tracey-config")
        .version("1")
        .cli("tracey")
        .write("schema.styx");
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

    // Only write if content changed to avoid retriggering the build
    let output_path = Path::new("src/bridge/http/dashboard/src/api-types.ts");
    let should_write = match fs::read_to_string(output_path) {
        Ok(existing) => existing != output,
        Err(_) => true,
    };
    if should_write {
        fs::write(output_path, &output).expect("Failed to write TypeScript types");
    }
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
    let node_check = shell_command("node").arg("--version").output();

    match node_check {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            eprintln!("Found node {}", version.trim());
        }
        _ => {
            #[cfg(windows)]
            panic!(
                "\n\
                Node.js is required but not found!\n\
                \n\
                Install Node.js using Chocolatey:\n\
                \n\
                  # First, install Chocolatey (if not already installed):\n\
                  powershell -c \"irm https://community.chocolatey.org/install.ps1|iex\"\n\
                \n\
                  # Then install Node.js:\n\
                  choco install nodejs\n\
                \n\
                  # Verify installation:\n\
                  node -v\n\
                \n\
                See https://nodejs.org/en/download for more options.\n"
            );

            #[cfg(not(windows))]
            panic!(
                "\n\
                Node.js is required but not found!\n\
                \n\
                Install Node.js using one of the following methods:\n\
                \n\
                  # On macOS with Homebrew:\n\
                  brew install node\n\
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
    let pnpm_check = shell_command("pnpm").arg("version").output();

    match pnpm_check {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            eprintln!("Found pnpm {}", version.trim());
        }
        _ => {
            #[cfg(windows)]
            panic!(
                "\n\
                pnpm is required but not found!\n\
                \n\
                Install pnpm using one of the following methods:\n\
                \n\
                  # Using npm (recommended):\n\
                  npm install -g pnpm\n\
                \n\
                  # Using Chocolatey:\n\
                  choco install pnpm\n\
                \n\
                  # Using winget:\n\
                  winget install -e --id pnpm.pnpm\n\
                \n\
                  # Using Scoop:\n\
                  scoop install pnpm\n\
                \n\
                  # Verify installation:\n\
                  pnpm -v\n\
                \n\
                See https://pnpm.io/installation for more options.\n"
            );

            #[cfg(not(windows))]
            panic!(
                "\n\
                pnpm is required but not found!\n\
                \n\
                Install pnpm using one of the following methods:\n\
                \n\
                  # Using Corepack (recommended, included with Node.js 16.13+):\n\
                  corepack enable pnpm\n\
                \n\
                  # Using npm:\n\
                  npm install -g pnpm\n\
                \n\
                  # On macOS with Homebrew:\n\
                  brew install pnpm\n\
                \n\
                  # Standalone script:\n\
                  curl -fsSL https://get.pnpm.io/install.sh | sh -\n\
                \n\
                See https://pnpm.io/installation for more options.\n"
            );
        }
    }

    eprintln!("Building dashboard with pnpm...");

    // Install dependencies if needed
    let status = shell_command("pnpm")
        .args(["install", "--frozen-lockfile"])
        .current_dir(dashboard_dir)
        .status()
        .expect("Failed to run pnpm install - is pnpm installed?");

    if !status.success() {
        panic!("pnpm install failed");
    }

    // Build the dashboard
    let status = shell_command("pnpm")
        .args(["run", "build"])
        .current_dir(dashboard_dir)
        .status()
        .expect("Failed to run pnpm build");

    if !status.success() {
        panic!("pnpm build failed");
    }
}
