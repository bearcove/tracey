//! LSP bridge for the tracey daemon.
//!
//! This module provides an LSP server that translates LSP protocol to
//! daemon RPC calls. It connects to the daemon as a client and forwards
//! all operations to the daemon.
//!
//! r[impl daemon.bridge.lsp]

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eyre::Result;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::daemon::{DaemonClient, new_client};
use tracey_core::{RefVerb, parse_rule_id};
use tracey_proto::*;

/// Convert roam RPC result to a simple Result
fn rpc<T, E: std::fmt::Debug>(res: Result<T, roam_stream::CallError<E>>) -> Result<T, String> {
    res.map_err(|e| format!("RPC error: {:?}", e))
}

// Semantic token types for requirement references
const SEMANTIC_TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE, // 0: prefix (e.g., "r")
    SemanticTokenType::KEYWORD,   // 1: verb (impl, verify, depends, related)
    SemanticTokenType::VARIABLE,  // 2: requirement ID
];

const SEMANTIC_TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DEFINITION, // 0: for definitions in spec files
    SemanticTokenModifier::DECLARATION, // 1: for valid references
];

/// Run the LSP bridge over stdio.
///
/// This function starts an LSP server that connects to the tracey daemon
/// for all operations.
///
/// r[impl lsp.lifecycle.stdio]
/// r[impl lsp.lifecycle.project-root]
pub async fn run(root: Option<PathBuf>, _config_path: PathBuf) -> Result<()> {
    // Determine project root
    let project_root = match root {
        Some(r) => r,
        None => crate::find_project_root()?,
    };

    // Run LSP server
    run_lsp_server(project_root).await
}

/// Internal: run the LSP server.
async fn run_lsp_server(project_root: PathBuf) -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let daemon_client = new_client(project_root.clone());

    let doc_state = Arc::new(Mutex::new(LspDocState {
        documents: HashMap::new(),
        files_with_diagnostics: HashSet::new(),
    }));

    let (service, socket) = LspService::new(|client| Backend {
        client,
        daemon_client: daemon_client.clone(),
        project_root: project_root.clone(),
        doc_state: Arc::clone(&doc_state),
    });
    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}

struct Backend {
    client: Client,
    daemon_client: DaemonClient,
    project_root: PathBuf,
    doc_state: Arc<Mutex<LspDocState>>,
}

/// Document-tracking state requiring mutual exclusion.
/// Only holds in-memory document content and diagnostic bookkeeping.
/// Never locked across an await point.
struct LspDocState {
    /// Document content cache: uri -> content
    documents: HashMap<String, String>,
    /// Files that have been published with non-empty diagnostics.
    /// Used to clear diagnostics when issues are fixed.
    files_with_diagnostics: HashSet<String>,
}

impl Backend {
    /// Get path and content for a document, for daemon calls.
    fn get_path_and_content(&self, uri: &Url) -> Option<(String, String)> {
        let state = self.doc_state.lock().unwrap();
        let content = state.documents.get(uri.as_str())?.clone();
        let path = uri.to_file_path().ok()?.to_string_lossy().into_owned();
        Some((path, content))
    }

    fn offset_to_line_col(content: &str, offset: usize) -> (u32, u32) {
        let mut line: u32 = 0;
        let mut col: u32 = 0;
        for (i, ch) in content.char_indices() {
            if i >= offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    fn replacement_edits_for_file(
        content: &str,
        old_id: &tracey_core::RuleId,
        new_id: &str,
    ) -> Vec<TextEdit> {
        let reqs = tracey_core::Reqs::extract_from_content(&PathBuf::new(), content);
        let old_text = old_id.to_string();
        let mut edits = Vec::new();
        for reference in &reqs.references {
            if reference.verb == RefVerb::Define || reference.req_id != *old_id {
                continue;
            }
            let start = reference.span.offset;
            let end = start.saturating_add(reference.span.length);
            let Some(span_text) = content.get(start..end) else {
                continue;
            };
            let Some(local_idx) = span_text.find(&old_text) else {
                continue;
            };
            let abs_start = start + local_idx;
            let abs_end = abs_start + old_text.len();
            let (start_line, start_char) = Self::offset_to_line_col(content, abs_start);
            let (end_line, end_char) = Self::offset_to_line_col(content, abs_end);
            edits.push(TextEdit {
                range: Range {
                    start: Position {
                        line: start_line,
                        character: start_char,
                    },
                    end: Position {
                        line: end_line,
                        character: end_char,
                    },
                },
                new_text: new_id.to_string(),
            });
        }
        edits
    }

    async fn apply_unknown_requirement_rename(
        &self,
        old_rule: &str,
        new_rule: &str,
    ) -> LspResult<()> {
        let Some(old_id) = parse_rule_id(old_rule) else {
            return Ok(());
        };
        if parse_rule_id(new_rule).is_none() {
            return Ok(());
        }

        let walker = ignore::WalkBuilder::new(&self.project_root)
            .follow_links(true)
            .hidden(false)
            .git_ignore(true)
            .build();

        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
        for entry in walker.flatten() {
            let path = entry.path();
            let Some(ft) = entry.file_type() else {
                continue;
            };
            if !ft.is_file() {
                continue;
            }
            if path
                .extension()
                .is_none_or(|ext| !tracey_core::is_supported_extension(ext))
            {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(path) else {
                continue;
            };
            let edits = Self::replacement_edits_for_file(&content, &old_id, new_rule);
            if edits.is_empty() {
                continue;
            }
            let Ok(uri) = Url::from_file_path(path) else {
                continue;
            };
            changes.insert(uri, edits);
        }

        if changes.is_empty() {
            return Ok(());
        }

        let edit = WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        };
        let _ = self.client.apply_edit(edit).await?;
        Ok(())
    }

    /// Notify daemon that a file was opened.
    async fn notify_vfs_open(&self, uri: &Url, content: &str) {
        if let Ok(path) = uri.to_file_path() {
            let _ = self
                .daemon_client
                .vfs_open(path.to_string_lossy().into_owned(), content.to_string())
                .await;
        }
    }

    /// Notify daemon that a file changed.
    async fn notify_vfs_change(&self, uri: &Url, content: &str) {
        if let Ok(path) = uri.to_file_path() {
            let _ = self
                .daemon_client
                .vfs_change(path.to_string_lossy().into_owned(), content.to_string())
                .await;
        }
    }

    /// Notify daemon that a file was closed.
    async fn notify_vfs_close(&self, uri: &Url) {
        if let Ok(path) = uri.to_file_path() {
            let _ = self
                .daemon_client
                .vfs_close(path.to_string_lossy().into_owned())
                .await;
        }
    }

    async fn publish_workspace_diagnostics_with(
        client: &Client,
        daemon_client: &DaemonClient,
        project_root: &std::path::Path,
        doc_state: &Arc<Mutex<LspDocState>>,
    ) {
        let config_error = rpc(daemon_client.health().await)
            .ok()
            .and_then(|h| h.config_error);

        let all_diagnostics = match rpc(daemon_client.lsp_workspace_diagnostics().await) {
            Ok(d) => d,
            Err(_) => return,
        };

        // Workspace-wide mode: clear all previously published diagnostics first.
        let files_to_clear: Vec<String> = {
            let mut state = doc_state.lock().unwrap();
            let to_clear: Vec<String> = state.files_with_diagnostics.iter().cloned().collect();
            state.files_with_diagnostics.clear();
            to_clear
        };

        for path in files_to_clear {
            let Ok(uri) = Url::from_file_path(&path) else {
                continue;
            };
            client.publish_diagnostics(uri, vec![], None).await;
        }

        // Publish config error diagnostic on config file
        let config_path = project_root.join(".config/tracey/config.styx");
        if let Ok(uri) = Url::from_file_path(&config_path) {
            if let Some(error_msg) = config_error {
                let diagnostic = Diagnostic {
                    range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 0,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("config-error".into())),
                    source: Some("tracey".into()),
                    message: error_msg,
                    ..Default::default()
                };
                client
                    .publish_diagnostics(uri, vec![diagnostic], None)
                    .await;
            } else {
                client.publish_diagnostics(uri, vec![], None).await;
            }
        }

        // Publish diagnostics for all files in the latest rebuild snapshot.
        let mut published_paths = HashSet::new();
        for file_diag in all_diagnostics {
            let abs_path = project_root.join(&file_diag.path);
            let abs_path_str = abs_path.to_string_lossy().into_owned();
            let Ok(uri) = Url::from_file_path(&abs_path) else {
                continue;
            };

            let diagnostics: Vec<Diagnostic> = file_diag
                .diagnostics
                .into_iter()
                .map(|d| Diagnostic {
                    range: Range {
                        start: Position {
                            line: d.start_line,
                            character: d.start_char,
                        },
                        end: Position {
                            line: d.end_line,
                            character: d.end_char,
                        },
                    },
                    severity: Some(match d.severity.as_str() {
                        "error" => DiagnosticSeverity::ERROR,
                        "warning" => DiagnosticSeverity::WARNING,
                        "info" => DiagnosticSeverity::INFORMATION,
                        _ => DiagnosticSeverity::HINT,
                    }),
                    code: Some(NumberOrString::String(d.code)),
                    source: Some("tracey".into()),
                    message: d.message,
                    ..Default::default()
                })
                .collect();

            client.publish_diagnostics(uri, diagnostics, None).await;
            published_paths.insert(abs_path_str);
        }

        let mut state = doc_state.lock().unwrap();
        state.files_with_diagnostics = published_paths;
    }

    async fn watch_daemon_rebuilds(
        client: Client,
        daemon_client: DaemonClient,
        project_root: PathBuf,
        doc_state: Arc<Mutex<LspDocState>>,
    ) {
        let mut last_version: Option<u64> = None;

        loop {
            let (tx, mut rx) = roam::channel::<DataUpdate>();
            let subscribe_client = daemon_client.clone();
            let subscribe_task = tokio::spawn(async move { subscribe_client.subscribe(tx).await });

            loop {
                match rx.recv().await {
                    Ok(Some(update)) => {
                        if last_version == Some(update.version) {
                            continue;
                        }
                        last_version = Some(update.version);
                        Self::publish_workspace_diagnostics_with(
                            &client,
                            &daemon_client,
                            &project_root,
                            &doc_state,
                        )
                        .await;
                    }
                    Ok(None) | Err(_) => {
                        break;
                    }
                }
            }

            subscribe_task.abort();
            let _ = subscribe_task.await;
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// Publish diagnostics for all files in the workspace.
    async fn publish_workspace_diagnostics(&self) {
        Self::publish_workspace_diagnostics_with(
            &self.client,
            &self.daemon_client,
            &self.project_root,
            &self.doc_state,
        )
        .await;
    }

    /// Clear diagnostics for workspace files on startup so clients don't retain stale diagnostics
    /// from a previous LSP session.
    async fn clear_workspace_diagnostics_on_startup(&self) {
        {
            let mut state = self.doc_state.lock().unwrap();
            state.files_with_diagnostics.clear();
        }

        let config_path = self.project_root.join(".config/tracey/config.styx");
        if let Ok(uri) = Url::from_file_path(&config_path) {
            self.client.publish_diagnostics(uri, vec![], None).await;
        }

        let walker = ignore::WalkBuilder::new(&self.project_root)
            .follow_links(true)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            let Some(ft) = entry.file_type() else {
                continue;
            };
            if !ft.is_file() {
                continue;
            }
            let should_clear = path.extension().is_some_and(|ext| {
                ext == "md" || ext == "styx" || tracey_core::is_supported_extension(ext)
            });
            if !should_clear {
                continue;
            }
            let Ok(uri) = Url::from_file_path(path) else {
                continue;
            };
            self.client.publish_diagnostics(uri, vec![], None).await;
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    /// r[impl lsp.lifecycle.initialize]
    /// r[impl lsp.completions.trigger]
    async fn initialize(&self, _: InitializeParams) -> LspResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["[".to_string(), " ".to_string()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["tracey.renameUnknownRequirement".to_string()],
                    work_done_progress_options: Default::default(),
                }),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                inlay_hint_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: SEMANTIC_TOKEN_TYPES.to_vec(),
                                token_modifiers: SEMANTIC_TOKEN_MODIFIERS.to_vec(),
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "tracey LSP bridge initialized")
            .await;

        // Startup reset for clients that persist diagnostics across server restarts.
        self.clear_workspace_diagnostics_on_startup().await;

        // Publish workspace-wide diagnostics for all files on startup
        self.publish_workspace_diagnostics().await;

        tokio::spawn(Self::watch_daemon_rebuilds(
            self.client.clone(),
            self.daemon_client.clone(),
            self.project_root.clone(),
            Arc::clone(&self.doc_state),
        ));
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let content = params.text_document.text.clone();
        {
            let mut state = self.doc_state.lock().unwrap();
            state.documents.insert(uri.to_string(), content.clone());
        }
        self.notify_vfs_open(&uri, &content).await;
    }

    /// r[impl lsp.diagnostics.on-change]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Some(change) = params.content_changes.into_iter().next() {
            let content = change.text.clone();
            {
                let mut state = self.doc_state.lock().unwrap();
                state.documents.insert(uri.to_string(), content.clone());
            }
            self.notify_vfs_change(&uri, &content).await;
        }
    }

    /// r[impl lsp.diagnostics.on-save]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Some(content) = params.text {
            self.notify_vfs_change(&uri, &content).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        {
            let mut state = self.doc_state.lock().unwrap();
            state.documents.remove(uri.as_str());
        }
        self.notify_vfs_close(&uri).await;
    }

    /// r[impl lsp.completions.verb]
    /// r[impl lsp.completions.req-id]
    /// r[impl lsp.completions.req-id-fuzzy]
    /// r[impl lsp.completions.req-id-preview]
    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let Some((path, content)) = self.get_path_and_content(uri) else {
            return Ok(None);
        };

        let req = LspPositionRequest {
            path,
            content,
            line: position.line,
            character: position.character,
        };

        let Ok(completions) = rpc(self.daemon_client.lsp_completions(req).await) else {
            return Ok(None);
        };

        let items: Vec<CompletionItem> = completions
            .into_iter()
            .map(|c| CompletionItem {
                label: c.label,
                kind: Some(match c.kind.as_str() {
                    "verb" => CompletionItemKind::KEYWORD,
                    "rule" => CompletionItemKind::CONSTANT,
                    _ => CompletionItemKind::TEXT,
                }),
                detail: c.detail,
                documentation: c.documentation.map(Documentation::String),
                insert_text: c.insert_text,
                ..Default::default()
            })
            .collect();

        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(items)))
        }
    }

    /// r[impl lsp.hover.req-reference]
    /// r[impl lsp.hover.req-reference-format]
    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        tracing::debug!(
            uri = %uri,
            line = position.line,
            character = position.character,
            "hover request"
        );

        let Some((path, content)) = self.get_path_and_content(uri) else {
            tracing::debug!(uri = %uri, "hover: no path/content for uri");
            return Ok(None);
        };

        let req = LspPositionRequest {
            path,
            content,
            line: position.line,
            character: position.character,
        };

        let info = match rpc(self.daemon_client.lsp_hover(req).await) {
            Ok(Some(info)) => info,
            Ok(None) => {
                tracing::debug!(uri = %uri, line = position.line, character = position.character, "hover: no rule at position");
                return Ok(None);
            }
            Err(e) => {
                tracing::warn!(uri = %uri, error = %e, "hover: daemon RPC error");
                return Ok(None);
            }
        };

        // Format hover with spec info
        let mut markdown = format!("## {}\n\n{}", info.rule_id, info.raw);
        markdown.push_str(&format!("\n\n**Spec:** {}", info.spec_name));
        if let Some(url) = &info.spec_url {
            markdown.push_str(&format!(" ([source]({}))", url));
        }

        // Format impl refs as clickable links
        if !info.impl_refs.is_empty() {
            markdown.push_str("\n\n**Implementations:**");
            for r in &info.impl_refs {
                let abs_path = self.project_root.join(&r.file);
                if let Ok(uri) = Url::from_file_path(&abs_path) {
                    markdown.push_str(&format!("\n- [{}:{}]({}#L{})", r.file, r.line, uri, r.line));
                } else {
                    markdown.push_str(&format!("\n- {}:{}", r.file, r.line));
                }
            }
        }

        // Format verify refs as clickable links
        if !info.verify_refs.is_empty() {
            markdown.push_str("\n\n**Verifications:**");
            for r in &info.verify_refs {
                let abs_path = self.project_root.join(&r.file);
                if let Ok(uri) = Url::from_file_path(&abs_path) {
                    markdown.push_str(&format!("\n- [{}:{}]({}#L{})", r.file, r.line, uri, r.line));
                } else {
                    markdown.push_str(&format!("\n- {}:{}", r.file, r.line));
                }
            }
        }

        // Summary counts
        if info.impl_refs.is_empty() && info.verify_refs.is_empty() {
            markdown.push_str("\n\n*No implementations or verifications*");
        }

        // r[impl lsp.hover.tail-diff.format+2]
        // Show word-level diff from previous rule version
        if let Some(diff) = &info.version_diff {
            markdown.push_str("\n\n**Changes from previous version:**\n\n");
            markdown.push_str(diff);
        }

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: markdown,
            }),
            range: Some(Range {
                start: Position {
                    line: info.range_start_line,
                    character: info.range_start_char,
                },
                end: Position {
                    line: info.range_end_line,
                    character: info.range_end_char,
                },
            }),
        }))
    }

    /// r[impl lsp.goto.ref-to-def]
    /// r[impl lsp.goto.precise-location]
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let Some((path, content)) = self.get_path_and_content(uri) else {
            return Ok(None);
        };

        let req = LspPositionRequest {
            path,
            content,
            line: position.line,
            character: position.character,
        };

        let Ok(locations) = rpc(self.daemon_client.lsp_definition(req).await) else {
            return Ok(None);
        };

        if locations.is_empty() {
            return Ok(None);
        }

        let loc = &locations[0];
        let def_uri = Url::from_file_path(self.project_root.join(&loc.path))
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params("Invalid file path"))?;

        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri: def_uri,
            range: Range {
                start: Position {
                    line: loc.line,
                    character: loc.character,
                },
                end: Position {
                    line: loc.line,
                    character: loc.character,
                },
            },
        })))
    }

    /// r[impl lsp.goto.def-to-impl]
    async fn goto_implementation(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let Some((path, content)) = self.get_path_and_content(uri) else {
            return Ok(None);
        };

        let req = LspPositionRequest {
            path,
            content,
            line: position.line,
            character: position.character,
        };

        let Ok(locations) = rpc(self.daemon_client.lsp_implementation(req).await) else {
            return Ok(None);
        };

        if locations.is_empty() {
            return Ok(None);
        }

        let lsp_locations: Vec<Location> = locations
            .into_iter()
            .filter_map(|loc| {
                let uri = Url::from_file_path(self.project_root.join(&loc.path)).ok()?;
                Some(Location {
                    uri,
                    range: Range {
                        start: Position {
                            line: loc.line,
                            character: loc.character,
                        },
                        end: Position {
                            line: loc.line,
                            character: loc.character,
                        },
                    },
                })
            })
            .collect();

        Ok(Some(GotoDefinitionResponse::Array(lsp_locations)))
    }

    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let Some((path, content)) = self.get_path_and_content(uri) else {
            return Ok(None);
        };

        let req = LspReferencesRequest {
            path,
            content,
            line: position.line,
            character: position.character,
            include_declaration: params.context.include_declaration,
        };

        let Ok(locations) = rpc(self.daemon_client.lsp_references(req).await) else {
            return Ok(None);
        };

        if locations.is_empty() {
            return Ok(None);
        }

        let lsp_locations: Vec<Location> = locations
            .into_iter()
            .filter_map(|loc| {
                let uri = Url::from_file_path(self.project_root.join(&loc.path)).ok()?;
                Some(Location {
                    uri,
                    range: Range {
                        start: Position {
                            line: loc.line,
                            character: loc.character,
                        },
                        end: Position {
                            line: loc.line,
                            character: loc.character,
                        },
                    },
                })
            })
            .collect();

        Ok(Some(lsp_locations))
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> LspResult<Option<Vec<DocumentHighlight>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let Some((path, content)) = self.get_path_and_content(uri) else {
            return Ok(None);
        };

        let req = LspPositionRequest {
            path,
            content,
            line: position.line,
            character: position.character,
        };

        let Ok(locations) = rpc(self.daemon_client.lsp_document_highlight(req).await) else {
            return Ok(None);
        };

        if locations.is_empty() {
            return Ok(None);
        }

        let highlights: Vec<DocumentHighlight> = locations
            .into_iter()
            .map(|loc| DocumentHighlight {
                range: Range {
                    start: Position {
                        line: loc.line,
                        character: loc.character,
                    },
                    end: Position {
                        line: loc.line,
                        character: loc.character + 10, // Approximate length
                    },
                },
                kind: Some(DocumentHighlightKind::READ),
            })
            .collect();

        Ok(Some(highlights))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> LspResult<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;

        let Some((path, content)) = self.get_path_and_content(uri) else {
            return Ok(None);
        };

        let req = LspDocumentRequest { path, content };

        let Ok(symbols) = rpc(self.daemon_client.lsp_document_symbols(req).await) else {
            return Ok(None);
        };

        if symbols.is_empty() {
            return Ok(None);
        }

        let lsp_symbols: Vec<SymbolInformation> = symbols
            .into_iter()
            .map(|s| {
                #[allow(deprecated)]
                SymbolInformation {
                    name: s.name,
                    kind: SymbolKind::CONSTANT,
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri: uri.clone(),
                        range: Range {
                            start: Position {
                                line: s.start_line,
                                character: s.start_char,
                            },
                            end: Position {
                                line: s.end_line,
                                character: s.end_char,
                            },
                        },
                    },
                    container_name: Some(s.kind),
                }
            })
            .collect();

        Ok(Some(DocumentSymbolResponse::Flat(lsp_symbols)))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> LspResult<Option<Vec<SymbolInformation>>> {
        let Ok(symbols) = rpc(self.daemon_client.lsp_workspace_symbols(params.query).await) else {
            return Ok(None);
        };

        if symbols.is_empty() {
            return Ok(None);
        }

        let lsp_symbols: Vec<SymbolInformation> = symbols
            .into_iter()
            .filter_map(|s| {
                // Try to construct a URI for the symbol
                let uri = Url::from_file_path(self.project_root.join("docs/spec")).ok()?;
                #[allow(deprecated)]
                Some(SymbolInformation {
                    name: s.name,
                    kind: SymbolKind::CONSTANT,
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri,
                        range: Range {
                            start: Position {
                                line: s.start_line,
                                character: s.start_char,
                            },
                            end: Position {
                                line: s.end_line,
                                character: s.end_char,
                            },
                        },
                    },
                    container_name: Some(s.kind),
                })
            })
            .collect();

        Ok(Some(lsp_symbols))
    }

    async fn code_action(&self, params: CodeActionParams) -> LspResult<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let position = params.range.start;

        let Some((path, content)) = self.get_path_and_content(uri) else {
            return Ok(None);
        };

        let req = LspPositionRequest {
            path,
            content,
            line: position.line,
            character: position.character,
        };

        let Ok(actions) = rpc(self.daemon_client.lsp_code_actions(req).await) else {
            return Ok(None);
        };

        if actions.is_empty() {
            return Ok(None);
        }

        let lsp_actions: Vec<CodeActionOrCommand> = actions
            .into_iter()
            .map(|a| {
                CodeActionOrCommand::CodeAction(CodeAction {
                    title: a.title,
                    kind: Some(a.kind.into()),
                    is_preferred: Some(a.is_preferred),
                    command: Some(Command {
                        title: String::new(),
                        command: a.command,
                        arguments: Some(
                            a.arguments
                                .into_iter()
                                .map(serde_json::Value::String)
                                .collect(),
                        ),
                    }),
                    ..Default::default()
                })
            })
            .collect();

        Ok(Some(lsp_actions))
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> LspResult<Option<serde_json::Value>> {
        if params.command == "tracey.renameUnknownRequirement" {
            let args = params.arguments;
            if args.len() >= 2
                && let (Some(old_rule), Some(new_rule)) = (args[0].as_str(), args[1].as_str())
            {
                self.apply_unknown_requirement_rename(old_rule, new_rule)
                    .await?;
            }
        }
        Ok(None)
    }

    async fn code_lens(&self, params: CodeLensParams) -> LspResult<Option<Vec<CodeLens>>> {
        let uri = &params.text_document.uri;

        let Some((path, content)) = self.get_path_and_content(uri) else {
            return Ok(None);
        };

        let req = LspDocumentRequest { path, content };

        let Ok(lenses) = rpc(self.daemon_client.lsp_code_lens(req).await) else {
            return Ok(None);
        };

        if lenses.is_empty() {
            return Ok(None);
        }

        let lsp_lenses: Vec<CodeLens> = lenses
            .into_iter()
            .map(|l| CodeLens {
                range: Range {
                    start: Position {
                        line: l.line,
                        character: l.start_char,
                    },
                    end: Position {
                        line: l.line,
                        character: l.end_char,
                    },
                },
                command: Some(Command {
                    title: l.title,
                    command: l.command,
                    arguments: Some(
                        l.arguments
                            .into_iter()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                }),
                data: None,
            })
            .collect();

        Ok(Some(lsp_lenses))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> LspResult<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;

        let Some((path, content)) = self.get_path_and_content(uri) else {
            return Ok(None);
        };

        let req = InlayHintsRequest {
            path,
            content,
            start_line: params.range.start.line,
            end_line: params.range.end.line,
        };

        let Ok(hints) = rpc(self.daemon_client.lsp_inlay_hints(req).await) else {
            return Ok(None);
        };

        if hints.is_empty() {
            return Ok(None);
        }

        let lsp_hints: Vec<InlayHint> = hints
            .into_iter()
            .map(|h| InlayHint {
                position: Position {
                    line: h.line,
                    character: h.character,
                },
                label: InlayHintLabel::String(h.label),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: None,
                padding_left: Some(true),
                padding_right: None,
                data: None,
            })
            .collect();

        Ok(Some(lsp_hints))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> LspResult<Option<PrepareRenameResponse>> {
        let uri = &params.text_document.uri;
        let position = params.position;

        let Some((path, content)) = self.get_path_and_content(uri) else {
            return Ok(None);
        };

        let req = LspPositionRequest {
            path,
            content,
            line: position.line,
            character: position.character,
        };

        let Ok(Some(result)) = rpc(self.daemon_client.lsp_prepare_rename(req).await) else {
            return Ok(None);
        };

        Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range {
                start: Position {
                    line: result.start_line,
                    character: result.start_char,
                },
                end: Position {
                    line: result.end_line,
                    character: result.end_char,
                },
            },
            placeholder: result.placeholder,
        }))
    }

    async fn rename(&self, params: RenameParams) -> LspResult<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let Some((path, content)) = self.get_path_and_content(uri) else {
            return Ok(None);
        };

        let req = LspRenameRequest {
            path,
            content,
            line: position.line,
            character: position.character,
            new_name: params.new_name,
        };

        let Ok(edits) = rpc(self.daemon_client.lsp_rename(req).await) else {
            return Ok(None);
        };

        if edits.is_empty() {
            return Ok(None);
        }

        // Group edits by file
        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
        for edit in edits {
            let uri = match Url::from_file_path(self.project_root.join(&edit.path)) {
                Ok(u) => u,
                Err(_) => continue,
            };
            changes.entry(uri).or_default().push(TextEdit {
                range: Range {
                    start: Position {
                        line: edit.start_line,
                        character: edit.start_char,
                    },
                    end: Position {
                        line: edit.end_line,
                        character: edit.end_char,
                    },
                },
                new_text: edit.new_text,
            });
        }

        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }))
    }

    /// r[impl lsp.semantic-tokens.req-id]
    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;

        let Some((path, content)) = self.get_path_and_content(uri) else {
            return Ok(None);
        };

        let req = LspDocumentRequest { path, content };

        let Ok(tokens) = rpc(self.daemon_client.lsp_semantic_tokens(req).await) else {
            return Ok(None);
        };

        if tokens.is_empty() {
            return Ok(None);
        }

        // Convert to delta format
        let mut prev_line = 0u32;
        let mut prev_char = 0u32;
        let mut lsp_tokens = Vec::new();

        for token in tokens {
            let delta_line = token.line - prev_line;
            let delta_start = if delta_line == 0 {
                token.start_char - prev_char
            } else {
                token.start_char
            };

            lsp_tokens.push(SemanticToken {
                delta_line,
                delta_start,
                length: token.length,
                token_type: token.token_type,
                token_modifiers_bitset: token.modifiers,
            });

            prev_line = token.line;
            prev_char = token.start_char;
        }

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: lsp_tokens,
        })))
    }
}
