//! End-to-end tests for multi-root behavior in the LSP bridge.

use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::time::sleep;
use url::Url;

fn write_project(root: &Path, rule_id: &str) {
    std::fs::create_dir_all(root.join(".config/tracey")).expect("failed to create config dir");
    std::fs::create_dir_all(root.join("src")).expect("failed to create src dir");

    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"tracey-lsp-test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("failed to write Cargo.toml");

    std::fs::write(
        root.join(".config/tracey/config.styx"),
        "specs (\n  {\n    name test\n    include (spec.md)\n    impls (\n      {\n        name rust\n        include (src/**/*.rs)\n      }\n    )\n  }\n)\n",
    )
    .expect("failed to write config");

    std::fs::write(
        root.join("spec.md"),
        format!("# Spec\n\nr[{rule_id}]\nRule text.\n"),
    )
    .expect("failed to write spec");

    std::fs::write(
        root.join("src/lib.rs"),
        format!("/// r[impl {rule_id}]\npub fn implemented() {{}}\n"),
    )
    .expect("failed to write source");
}

fn tracey_bin_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tracey") {
        return PathBuf::from(path);
    }

    let current_exe = std::env::current_exe().expect("failed to get current exe path");
    let debug_dir = current_exe
        .parent()
        .and_then(|p| p.parent())
        .expect("failed to resolve target/debug directory");

    let mut binary = debug_dir.join("tracey");
    if cfg!(windows) {
        binary.set_extension("exe");
    }
    binary
}

async fn send_message(stdin: &mut ChildStdin, value: &serde_json::Value) {
    let body = serde_json::to_vec(value).expect("serialize message");
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    stdin
        .write_all(header.as_bytes())
        .await
        .expect("write LSP header");
    stdin.write_all(&body).await.expect("write LSP body");
    stdin.flush().await.expect("flush LSP stdin");
}

async fn read_message(stdout: &mut BufReader<ChildStdout>) -> serde_json::Value {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let bytes_read = stdout.read_line(&mut line).await.expect("read header line");
        assert!(bytes_read > 0, "unexpected EOF while reading LSP header");

        if line == "\r\n" {
            break;
        }

        let trimmed = line.trim();
        if let Some(value) = trimmed
            .strip_prefix("Content-Length:")
            .or_else(|| trimmed.strip_prefix("content-length:"))
        {
            content_length = Some(value.trim().parse::<usize>().expect("parse content length"));
        }
    }

    let content_length = content_length.expect("missing Content-Length");
    let mut body = vec![0u8; content_length];
    stdout.read_exact(&mut body).await.expect("read body");
    serde_json::from_slice(&body).expect("parse JSON body")
}

async fn request(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    id: i64,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    send_message(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }),
    )
    .await;

    loop {
        let msg = tokio::time::timeout(Duration::from_secs(10), read_message(stdout))
            .await
            .expect("timed out waiting for LSP response");
        if msg.get("id").and_then(|v| v.as_i64()) == Some(id) {
            return msg;
        }
    }
}

async fn request_no_params(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    id: i64,
    method: &str,
) -> serde_json::Value {
    send_message(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
        }),
    )
    .await;

    loop {
        let msg = tokio::time::timeout(Duration::from_secs(10), read_message(stdout))
            .await
            .expect("timed out waiting for LSP response");
        if msg.get("id").and_then(|v| v.as_i64()) == Some(id) {
            return msg;
        }
    }
}

/// Read messages until we get a publishDiagnostics notification for the given file URI,
/// or until we've read `max_messages` messages without finding one.
/// Returns the diagnostics array if found.
async fn wait_for_diagnostics(
    stdout: &mut BufReader<ChildStdout>,
    file_uri: &str,
    max_messages: usize,
    timeout_per_msg: Duration,
) -> Option<Vec<serde_json::Value>> {
    for _ in 0..max_messages {
        let msg = match tokio::time::timeout(timeout_per_msg, read_message(stdout)).await {
            Ok(msg) => msg,
            Err(_) => return None,
        };

        if msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics") {
            if let Some(params) = msg.get("params") {
                if params.get("uri").and_then(|u| u.as_str()) == Some(file_uri) {
                    let diags = params
                        .get("diagnostics")
                        .and_then(|d| d.as_array())
                        .cloned()
                        .unwrap_or_default();
                    return Some(diags);
                }
            }
        }
    }
    None
}

/// Check if a diagnostics array contains a diagnostic with the given code.
fn diagnostics_contain_code(diagnostics: &[serde_json::Value], code: &str) -> bool {
    diagnostics
        .iter()
        .any(|d| d.get("code").and_then(|c| c.as_str()) == Some(code))
}

fn symbol_names(response: &serde_json::Value) -> Vec<String> {
    response
        .get("result")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    item.get("name")
                        .and_then(|name| name.as_str())
                        .map(ToOwned::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn test_lsp_workspace_folder_add_remove_updates_symbol_scope() {
    let project_a = tempfile::tempdir().expect("tempdir A");
    let project_b = tempfile::tempdir().expect("tempdir B");
    write_project(project_a.path(), "alpha.login");
    write_project(project_b.path(), "beta.payment");

    let project_a_uri = Url::from_directory_path(project_a.path())
        .expect("project A uri")
        .to_string();
    let project_b_uri = Url::from_directory_path(project_b.path())
        .expect("project B uri")
        .to_string();

    let mut child = Command::new(tracey_bin_path())
        .arg("lsp")
        .arg(project_a.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("failed to spawn tracey lsp");

    let mut stdin = child.stdin.take().expect("missing child stdin");
    let stdout = child.stdout.take().expect("missing child stdout");
    let mut stdout = BufReader::new(stdout);

    let initialize = request(
        &mut stdin,
        &mut stdout,
        1,
        "initialize",
        json!({
            "capabilities": {},
            "workspaceFolders": [
                { "uri": project_a_uri, "name": "project-a" }
            ]
        }),
    )
    .await;
    assert!(
        initialize.get("error").is_none(),
        "initialize failed: {initialize}"
    );

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }),
    )
    .await;

    let before_add = request(
        &mut stdin,
        &mut stdout,
        2,
        "workspace/symbol",
        json!({ "query": "beta.payment" }),
    )
    .await;
    assert!(
        !symbol_names(&before_add)
            .iter()
            .any(|n| n == "beta.payment"),
        "beta symbol unexpectedly visible before adding project B: {before_add}"
    );

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "workspace/didChangeWorkspaceFolders",
            "params": {
                "event": {
                    "added": [{ "uri": project_b_uri, "name": "project-b" }],
                    "removed": []
                }
            }
        }),
    )
    .await;

    let mut found_after_add = false;
    for id in 3..=22 {
        let response = request(
            &mut stdin,
            &mut stdout,
            id,
            "workspace/symbol",
            json!({ "query": "beta.payment" }),
        )
        .await;
        if symbol_names(&response).iter().any(|n| n == "beta.payment") {
            found_after_add = true;
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    assert!(
        found_after_add,
        "beta symbol was never visible after adding project B"
    );

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "workspace/didChangeWorkspaceFolders",
            "params": {
                "event": {
                    "added": [],
                    "removed": [{ "uri": project_b_uri, "name": "project-b" }]
                }
            }
        }),
    )
    .await;

    let mut gone_after_remove = false;
    for id in 23..=42 {
        let response = request(
            &mut stdin,
            &mut stdout,
            id,
            "workspace/symbol",
            json!({ "query": "beta.payment" }),
        )
        .await;
        if !symbol_names(&response).iter().any(|n| n == "beta.payment") {
            gone_after_remove = true;
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    assert!(
        gone_after_remove,
        "beta symbol still visible after removing project B"
    );

    let shutdown = request_no_params(&mut stdin, &mut stdout, 100, "shutdown").await;
    assert!(
        shutdown.get("error").is_none(),
        "shutdown failed: {shutdown}"
    );

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "exit"
        }),
    )
    .await;
}

/// File watcher detects spec change on disk and updates diagnostics without manual reload.
///
/// 1. Create project: spec has only auth.login, source references auth.login + payment.checkout
/// 2. Open source file → diagnostics should show orphaned for payment.checkout
/// 3. Write updated spec to disk (adding payment.checkout) — no didChange, no reload
/// 4. Wait for watcher to fire and diagnostics to clear
#[tokio::test]
async fn test_lsp_file_watcher_detects_spec_change() {
    let project = tempfile::tempdir().expect("tempdir");
    let root = project.path();

    // Set up project: spec only has auth.login
    std::fs::create_dir_all(root.join(".config/tracey")).expect("create config dir");
    std::fs::create_dir_all(root.join("src")).expect("create src dir");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"tracey-watcher-test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("write Cargo.toml");
    std::fs::write(
        root.join(".config/tracey/config.styx"),
        "specs (\n  {\n    name test\n    include (spec.md)\n    impls (\n      {\n        name rust\n        include (src/**/*.rs)\n      }\n    )\n  }\n)\n",
    )
    .expect("write config");
    std::fs::write(
        root.join("spec.md"),
        "# Spec\n\nr[auth.login]\nUsers must log in.\n",
    )
    .expect("write spec");
    // Source references both auth.login and payment.checkout (orphaned)
    std::fs::write(
        root.join("src/lib.rs"),
        "/// r[impl auth.login]\n/// r[impl payment.checkout]\npub fn handler() {}\n",
    )
    .expect("write source");

    let project_uri = Url::from_directory_path(root)
        .expect("project uri")
        .to_string();
    let lib_uri = Url::from_file_path(root.join("src/lib.rs"))
        .expect("lib uri")
        .to_string();

    let mut child = Command::new(tracey_bin_path())
        .arg("lsp")
        .arg(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("failed to spawn tracey lsp");

    let mut stdin = child.stdin.take().expect("missing child stdin");
    let stdout = child.stdout.take().expect("missing child stdout");
    let mut stdout = BufReader::new(stdout);

    // Initialize
    let initialize = request(
        &mut stdin,
        &mut stdout,
        1,
        "initialize",
        json!({
            "capabilities": {},
            "workspaceFolders": [
                { "uri": project_uri, "name": "watcher-test" }
            ]
        }),
    )
    .await;
    assert!(
        initialize.get("error").is_none(),
        "initialize failed: {initialize}"
    );

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }),
    )
    .await;

    // Open the source file to trigger diagnostic publishing
    let lib_content = std::fs::read_to_string(root.join("src/lib.rs")).expect("read lib.rs");
    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": lib_uri,
                    "languageId": "rust",
                    "version": 1,
                    "text": lib_content
                }
            }
        }),
    )
    .await;

    // Wait for initial diagnostics — should contain orphaned for payment.checkout.
    // The LSP may send an empty diagnostics set first (clearing), then the real ones,
    // so we keep polling until we see orphaned diagnostics.
    let mut found_orphaned = false;
    for _ in 0..60 {
        match wait_for_diagnostics(&mut stdout, &lib_uri, 20, Duration::from_millis(500)).await {
            Some(diags) if diagnostics_contain_code(&diags, "orphaned") => {
                found_orphaned = true;
                break;
            }
            _ => {
                sleep(Duration::from_millis(200)).await;
            }
        }
    }
    assert!(
        found_orphaned,
        "Expected orphaned diagnostic for payment.checkout initially"
    );

    // Write updated spec to disk — add payment.checkout rule.
    // Do NOT send any LSP notification about the spec file.
    // The file watcher should detect this change and trigger a rebuild.
    std::fs::write(
        root.join("spec.md"),
        "# Spec\n\nr[auth.login]\nUsers must log in.\n\nr[payment.checkout]\nUsers can check out.\n",
    )
    .expect("update spec on disk");

    // Poll for updated diagnostics: the watcher should fire, rebuild, and push
    // diagnostics where payment.checkout is no longer orphaned.
    let mut orphan_cleared = false;
    for _ in 0..60 {
        match wait_for_diagnostics(&mut stdout, &lib_uri, 20, Duration::from_millis(500)).await {
            Some(diags) if !diagnostics_contain_code(&diags, "orphaned") => {
                orphan_cleared = true;
                break;
            }
            _ => {
                sleep(Duration::from_millis(200)).await;
            }
        }
    }
    assert!(
        orphan_cleared,
        "File watcher did not clear orphaned diagnostic after spec change on disk"
    );

    // Clean shutdown
    let shutdown = request_no_params(&mut stdin, &mut stdout, 200, "shutdown").await;
    assert!(
        shutdown.get("error").is_none(),
        "shutdown failed: {shutdown}"
    );

    send_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "exit"
        }),
    )
    .await;
}
