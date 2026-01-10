//! LSP bridge for the tracey daemon.
//!
//! This module provides an LSP server that translates LSP protocol to
//! daemon RPC calls. It connects to the daemon as a client and forwards
//! all operations to the daemon.
//!
//! r[impl daemon.bridge.lsp]

use std::collections::HashMap;
use std::path::PathBuf;

use eyre::Result;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::daemon::DaemonClient;
use tracey_proto::*;

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
pub async fn run(root: Option<PathBuf>, _config_path: Option<PathBuf>) -> Result<()> {
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

    let (service, socket) = LspService::new(|client| Backend {
        client,
        state: tokio::sync::Mutex::new(LspState {
            documents: HashMap::new(),
            project_root: project_root.clone(),
            daemon_client: None,
        }),
    });
    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}

struct Backend {
    client: Client,
    state: tokio::sync::Mutex<LspState>,
}

struct LspState {
    /// Document content cache: uri -> content
    documents: HashMap<String, String>,
    /// Project root for resolving paths
    project_root: PathBuf,
    /// Client connection to daemon (lazy-initialized)
    daemon_client: Option<DaemonClient>,
}

impl LspState {
    /// Get or create daemon client connection.
    async fn get_daemon_client(&mut self) -> Result<&mut DaemonClient> {
        if self.daemon_client.is_none() {
            self.daemon_client = Some(DaemonClient::connect(&self.project_root).await?);
        }
        Ok(self.daemon_client.as_mut().unwrap())
    }

    /// Store document content when opened.
    fn document_opened(&mut self, uri: &Url, content: String) {
        self.documents.insert(uri.to_string(), content);
    }

    /// Update document content when changed.
    fn document_changed(&mut self, uri: &Url, content: String) {
        self.documents.insert(uri.to_string(), content);
    }

    /// Remove document when closed.
    fn document_closed(&mut self, uri: &Url) {
        self.documents.remove(uri.as_str());
    }
}

impl Backend {
    /// Lock state and get access to all LSP state.
    async fn state(&self) -> tokio::sync::MutexGuard<'_, LspState> {
        self.state.lock().await
    }

    /// Get path and content for a document, for daemon calls.
    async fn get_path_and_content(&self, uri: &Url) -> Option<(String, String)> {
        let state = self.state().await;
        let content = state.documents.get(uri.as_str())?.clone();
        let path = uri.to_file_path().ok()?.to_string_lossy().into_owned();
        Some((path, content))
    }

    /// Publish diagnostics for a document by calling daemon.
    ///
    /// r[impl lsp.diagnostics.broken-refs]
    /// r[impl lsp.diagnostics.broken-refs-message]
    /// r[impl lsp.diagnostics.unknown-prefix]
    /// r[impl lsp.diagnostics.unknown-prefix-message]
    /// r[impl lsp.diagnostics.unknown-verb]
    async fn publish_diagnostics(&self, uri: Url) {
        let Some((path, content)) = self.get_path_and_content(&uri).await else {
            return;
        };

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return;
        };

        let req = LspDocumentRequest { path, content };
        let Ok(daemon_diagnostics) = client.lsp_diagnostics(req).await else {
            return;
        };

        // Convert daemon diagnostics to LSP diagnostics
        let diagnostics: Vec<Diagnostic> = daemon_diagnostics
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

        drop(state); // Release lock before async call
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    /// Notify daemon that a file was opened.
    async fn notify_vfs_open(&self, uri: &Url, content: &str) {
        if let Ok(path) = uri.to_file_path() {
            let mut state = self.state().await;
            if let Ok(client) = state.get_daemon_client().await {
                let _ = client
                    .vfs_open(path.to_string_lossy().into_owned(), content.to_string())
                    .await;
            }
        }
    }

    /// Notify daemon that a file changed.
    async fn notify_vfs_change(&self, uri: &Url, content: &str) {
        if let Ok(path) = uri.to_file_path() {
            let mut state = self.state().await;
            if let Ok(client) = state.get_daemon_client().await {
                let _ = client
                    .vfs_change(path.to_string_lossy().into_owned(), content.to_string())
                    .await;
            }
        }
    }

    /// Notify daemon that a file was closed.
    async fn notify_vfs_close(&self, uri: &Url) {
        if let Ok(path) = uri.to_file_path() {
            let mut state = self.state().await;
            if let Ok(client) = state.get_daemon_client().await {
                let _ = client.vfs_close(path.to_string_lossy().into_owned()).await;
            }
        }
    }

    /// Publish diagnostics for all files in the workspace.
    ///
    /// r[impl lsp.diagnostics.workspace]
    async fn publish_workspace_diagnostics(&self) {
        let project_root = self.state().await.project_root.clone();

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return;
        };

        let Ok(all_diagnostics) = client.lsp_workspace_diagnostics().await else {
            return;
        };

        drop(state); // Release lock before async calls

        for file_diag in all_diagnostics {
            // Convert relative path to absolute and then to URI
            let abs_path = project_root.join(&file_diag.path);
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

            self.client
                .publish_diagnostics(uri, diagnostics, None)
                .await;
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

        // Publish workspace-wide diagnostics for all files on startup
        self.publish_workspace_diagnostics().await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let content = params.text_document.text.clone();
        self.state().await.document_opened(&uri, content.clone());
        self.notify_vfs_open(&uri, &content).await;
        self.publish_diagnostics(uri).await;
    }

    /// r[impl lsp.diagnostics.on-change]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Some(change) = params.content_changes.into_iter().next() {
            let content = change.text.clone();
            self.state().await.document_changed(&uri, content.clone());
            self.notify_vfs_change(&uri, &content).await;
            self.publish_diagnostics(uri).await;
        }
    }

    /// r[impl lsp.diagnostics.on-save]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.publish_diagnostics(uri).await;

        // Also refresh workspace-wide diagnostics, since saving one file
        // can affect diagnostics in other files (e.g., covering a requirement)
        self.publish_workspace_diagnostics().await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.state().await.document_closed(&uri);
        self.notify_vfs_close(&uri).await;
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    /// r[impl lsp.completions.verb]
    /// r[impl lsp.completions.req-id]
    /// r[impl lsp.completions.req-id-fuzzy]
    /// r[impl lsp.completions.req-id-preview]
    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let Some((path, content)) = self.get_path_and_content(uri).await else {
            return Ok(None);
        };

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return Ok(None);
        };

        let req = LspPositionRequest {
            path,
            content,
            line: position.line,
            character: position.character,
        };

        let Ok(completions) = client.lsp_completions(req).await else {
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

        let Some((path, content)) = self.get_path_and_content(uri).await else {
            return Ok(None);
        };

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return Ok(None);
        };

        let req = LspPositionRequest {
            path,
            content,
            line: position.line,
            character: position.character,
        };

        let Ok(Some(info)) = client.lsp_hover(req).await else {
            return Ok(None);
        };

        let project_root = self.state().await.project_root.clone();

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
                let abs_path = project_root.join(&r.file);
                if let Ok(uri) = Url::from_file_path(&abs_path) {
                    // Use file URI with line number fragment
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
                let abs_path = project_root.join(&r.file);
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

        let Some((path, content)) = self.get_path_and_content(uri).await else {
            return Ok(None);
        };

        let project_root = self.state().await.project_root.clone();

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return Ok(None);
        };

        let req = LspPositionRequest {
            path,
            content,
            line: position.line,
            character: position.character,
        };

        let Ok(locations) = client.lsp_definition(req).await else {
            return Ok(None);
        };

        if locations.is_empty() {
            return Ok(None);
        }

        let loc = &locations[0];
        let def_uri = Url::from_file_path(project_root.join(&loc.path))
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

        let Some((path, content)) = self.get_path_and_content(uri).await else {
            return Ok(None);
        };

        let project_root = self.state().await.project_root.clone();

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return Ok(None);
        };

        let req = LspPositionRequest {
            path,
            content,
            line: position.line,
            character: position.character,
        };

        let Ok(locations) = client.lsp_implementation(req).await else {
            return Ok(None);
        };

        if locations.is_empty() {
            return Ok(None);
        }

        let lsp_locations: Vec<Location> = locations
            .into_iter()
            .filter_map(|loc| {
                let uri = Url::from_file_path(project_root.join(&loc.path)).ok()?;
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

        let Some((path, content)) = self.get_path_and_content(uri).await else {
            return Ok(None);
        };

        let project_root = self.state().await.project_root.clone();

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return Ok(None);
        };

        let req = LspReferencesRequest {
            path,
            content,
            line: position.line,
            character: position.character,
            include_declaration: params.context.include_declaration,
        };

        let Ok(locations) = client.lsp_references(req).await else {
            return Ok(None);
        };

        if locations.is_empty() {
            return Ok(None);
        }

        let lsp_locations: Vec<Location> = locations
            .into_iter()
            .filter_map(|loc| {
                let uri = Url::from_file_path(project_root.join(&loc.path)).ok()?;
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

        let Some((path, content)) = self.get_path_and_content(uri).await else {
            return Ok(None);
        };

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return Ok(None);
        };

        let req = LspPositionRequest {
            path,
            content,
            line: position.line,
            character: position.character,
        };

        let Ok(locations) = client.lsp_document_highlight(req).await else {
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

        let Some((path, content)) = self.get_path_and_content(uri).await else {
            return Ok(None);
        };

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return Ok(None);
        };

        let req = LspDocumentRequest { path, content };

        let Ok(symbols) = client.lsp_document_symbols(req).await else {
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
        let project_root = self.state().await.project_root.clone();

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return Ok(None);
        };

        let Ok(symbols) = client.lsp_workspace_symbols(params.query).await else {
            return Ok(None);
        };

        if symbols.is_empty() {
            return Ok(None);
        }

        let lsp_symbols: Vec<SymbolInformation> = symbols
            .into_iter()
            .filter_map(|s| {
                // Try to construct a URI for the symbol
                let uri = Url::from_file_path(project_root.join("docs/spec")).ok()?;
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

        let Some((path, content)) = self.get_path_and_content(uri).await else {
            return Ok(None);
        };

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return Ok(None);
        };

        let req = LspPositionRequest {
            path,
            content,
            line: position.line,
            character: position.character,
        };

        let Ok(actions) = client.lsp_code_actions(req).await else {
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

    async fn code_lens(&self, params: CodeLensParams) -> LspResult<Option<Vec<CodeLens>>> {
        let uri = &params.text_document.uri;

        let Some((path, content)) = self.get_path_and_content(uri).await else {
            return Ok(None);
        };

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return Ok(None);
        };

        let req = LspDocumentRequest { path, content };

        let Ok(lenses) = client.lsp_code_lens(req).await else {
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

        let Some((path, content)) = self.get_path_and_content(uri).await else {
            return Ok(None);
        };

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return Ok(None);
        };

        let req = InlayHintsRequest {
            path,
            content,
            start_line: params.range.start.line,
            end_line: params.range.end.line,
        };

        let Ok(hints) = client.lsp_inlay_hints(req).await else {
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

        let Some((path, content)) = self.get_path_and_content(uri).await else {
            return Ok(None);
        };

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return Ok(None);
        };

        let req = LspPositionRequest {
            path,
            content,
            line: position.line,
            character: position.character,
        };

        let Ok(Some(result)) = client.lsp_prepare_rename(req).await else {
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

        let Some((path, content)) = self.get_path_and_content(uri).await else {
            return Ok(None);
        };

        let project_root = self.state().await.project_root.clone();

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return Ok(None);
        };

        let req = LspRenameRequest {
            path,
            content,
            line: position.line,
            character: position.character,
            new_name: params.new_name,
        };

        let Ok(edits) = client.lsp_rename(req).await else {
            return Ok(None);
        };

        if edits.is_empty() {
            return Ok(None);
        }

        // Group edits by file
        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
        for edit in edits {
            let uri = match Url::from_file_path(project_root.join(&edit.path)) {
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

        let Some((path, content)) = self.get_path_and_content(uri).await else {
            return Ok(None);
        };

        let mut state = self.state().await;
        let Ok(client) = state.get_daemon_client().await else {
            return Ok(None);
        };

        let req = LspDocumentRequest { path, content };

        let Ok(tokens) = client.lsp_semantic_tokens(req).await else {
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
