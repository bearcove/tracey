//! LSP bridge for the tracey daemon.
//!
//! This module provides an LSP server that translates LSP protocol to
//! daemon RPC calls. It connects to the daemon as a client and forwards
//! VFS operations (didOpen, didChange, didClose) to keep the daemon's
//! overlay in sync with the editor's state.
//!
//! r[impl daemon.bridge.lsp]

use std::collections::HashMap;
use std::path::PathBuf;

use eyre::Result;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::daemon::{DaemonClient, ensure_daemon_running};
use tracey_api::ApiSpecForward;
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
/// for data and VFS operations.
pub async fn run(root: Option<PathBuf>, _config_path: Option<PathBuf>) -> Result<()> {
    // Determine project root
    let project_root = match root {
        Some(r) => r,
        None => crate::find_project_root()?,
    };

    // Ensure daemon is running (or tell user to start it)
    ensure_daemon_running(&project_root).await?;

    // Connect to daemon
    let mut client = DaemonClient::connect(&project_root).await?;

    // Fetch initial data from daemon
    let config = client.config().await?;
    let mut forward_by_impl = HashMap::new();

    for spec_config in &config.specs {
        for impl_name in &spec_config.implementations {
            if let Some(forward) = client
                .forward(spec_config.name.clone(), impl_name.clone())
                .await?
            {
                forward_by_impl.insert((spec_config.name.clone(), impl_name.clone()), forward);
            }
        }
    }

    // Create initial data cache
    let initial_data = LspDataCache {
        forward_by_impl,
        config,
    };

    // Run LSP server
    run_lsp_server(initial_data, project_root).await
}

/// Cached data from daemon for LSP operations.
struct LspDataCache {
    /// Forward traceability data (rules → code references)
    forward_by_impl: HashMap<(String, String), ApiSpecForward>,
    /// Configuration
    config: ApiConfig,
}

/// Internal: run the LSP server with cached data.
async fn run_lsp_server(initial_data: LspDataCache, project_root: PathBuf) -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        state: tokio::sync::Mutex::new(LspState {
            data: initial_data,
            documents: HashMap::new(),
            project_root: project_root.clone(),
            daemon_client: None, // Will be connected on first VFS operation
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
    /// Cached data from daemon
    data: LspDataCache,
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

    /// Get all prefixes from config (for syntax validation).
    fn get_prefixes(&self) -> Vec<String> {
        self.data
            .config
            .specs
            .iter()
            .map(|s| s.prefix.clone())
            .collect()
    }

    /// Find rule definition by ID across all specs/impls.
    fn find_rule(&self, rule_id: &str) -> Option<(&str, &str, &tracey_api::ApiRule)> {
        for ((spec, impl_name), spec_data) in &self.data.forward_by_impl {
            for rule in &spec_data.rules {
                if rule.id == rule_id {
                    return Some((spec, impl_name, rule));
                }
            }
        }
        None
    }

    /// Get completion context (raw content inside r[...] or similar brackets).
    fn get_completion_context(&self, uri: &Url, position: Position) -> Option<String> {
        let content = self.documents.get(uri.as_str())?;
        let lines: Vec<&str> = content.lines().collect();
        let line = lines.get(position.line as usize)?;

        // Find the bracket context at cursor position
        let col = position.character as usize;
        if col > line.len() {
            return None;
        }

        // Look backwards for opening bracket
        let before = &line[..col];
        let open_pos = before.rfind("r[")?;
        let after_bracket = &before[open_pos + 2..];

        // Make sure we haven't closed the bracket yet
        if after_bracket.contains(']') {
            return None;
        }

        Some(after_bracket.to_string())
    }

    /// Store document content when opened.
    fn document_opened(&mut self, uri: &Url, content: String) {
        self.documents.insert(uri.to_string(), content);
    }

    /// Update document content when changed.
    fn document_changed(&mut self, uri: &Url, content: String) {
        self.documents.insert(uri.to_string(), content);
    }

    /// Get document content.
    fn get_document_content(&self, uri: &Url) -> Option<String> {
        self.documents.get(uri.as_str()).cloned()
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

    /// Compute diagnostics for a document.
    async fn compute_diagnostics(&self, _uri: &Url, content: &str) -> Vec<Diagnostic> {
        use tracey_core::{RefVerb, Reqs, WarningKind};

        let state = self.state().await;
        let mut diagnostics = Vec::new();
        let prefixes = state.get_prefixes();

        // Build line starts for byte offset to line/column conversion
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i + 1))
            .collect();

        // Helper to convert byte offset to (line, column)
        let offset_to_position = |offset: usize| -> Position {
            let line = match line_starts.binary_search(&offset) {
                Ok(line) => line,
                Err(line) => line.saturating_sub(1),
            };
            let line_start = line_starts.get(line).copied().unwrap_or(0);
            let column = offset.saturating_sub(line_start);
            Position {
                line: line as u32,
                character: column as u32,
            }
        };

        // Extract references from the content
        let reqs = Reqs::extract_from_content(std::path::Path::new(""), content);

        // Check each reference
        for reference in &reqs.references {
            // Check if the prefix is known
            if !prefixes.contains(&reference.prefix) {
                let start = offset_to_position(reference.span.offset);
                let end = offset_to_position(reference.span.offset + reference.span.length);
                diagnostics.push(Diagnostic {
                    range: Range { start, end },
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("unknown-prefix".into())),
                    source: Some("tracey".into()),
                    message: format!("Unknown prefix: {}", reference.prefix),
                    ..Default::default()
                });
                continue;
            }

            // Check if the rule exists (only for impl/verify/depends/related, not define)
            if !matches!(reference.verb, RefVerb::Define)
                && state.find_rule(&reference.req_id).is_none()
            {
                let start = offset_to_position(reference.span.offset);
                let end = offset_to_position(reference.span.offset + reference.span.length);

                let suggestion = suggest_similar_rules(&state, &reference.req_id);
                let suggestion_text = if suggestion.is_empty() {
                    String::new()
                } else {
                    format!(". Did you mean '{}'?", suggestion)
                };

                diagnostics.push(Diagnostic {
                    range: Range { start, end },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("broken-ref".into())),
                    source: Some("tracey".into()),
                    message: format!(
                        "Unknown requirement '{}'{}",
                        reference.req_id, suggestion_text
                    ),
                    ..Default::default()
                });
            }
        }

        // Add warnings from the extraction (unknown verbs, malformed refs)
        for warning in &reqs.warnings {
            match &warning.kind {
                WarningKind::UnknownVerb(v) => {
                    let start = offset_to_position(warning.span.offset);
                    let end = offset_to_position(warning.span.offset + warning.span.length);
                    diagnostics.push(Diagnostic {
                        range: Range { start, end },
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(NumberOrString::String("unknown-verb".into())),
                        source: Some("tracey".into()),
                        message: format!(
                            "Unknown verb '{}'. Valid verbs: impl, verify, depends, related",
                            v
                        ),
                        ..Default::default()
                    });
                }
                WarningKind::MalformedReference => {
                    let start = offset_to_position(warning.span.offset);
                    let end = offset_to_position(warning.span.offset + warning.span.length);
                    diagnostics.push(Diagnostic {
                        range: Range { start, end },
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(NumberOrString::String("malformed-ref".into())),
                        source: Some("tracey".into()),
                        message: "Malformed reference".to_string(),
                        ..Default::default()
                    });
                }
            }
        }

        diagnostics
    }

    /// Publish diagnostics for a document.
    async fn publish_diagnostics(&self, uri: Url, content: &str) {
        let diagnostics = self.compute_diagnostics(&uri, content).await;
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
}

/// Find similar rule IDs for suggestions.
fn suggest_similar_rules(state: &LspState, rule_id: &str) -> String {
    let mut suggestions: Vec<(usize, &str)> = Vec::new();

    for spec_data in state.data.forward_by_impl.values() {
        for rule in &spec_data.rules {
            let distance = levenshtein_distance(rule_id, &rule.id);
            if distance <= 3 {
                suggestions.push((distance, &rule.id));
            }
        }
    }

    suggestions.sort_by_key(|(d, _)| *d);
    suggestions
        .into_iter()
        .take(3)
        .map(|(_, id)| id)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Simple Levenshtein distance for string similarity.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
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
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let content = params.text_document.text.clone();
        self.state().await.document_opened(&uri, content.clone());
        self.notify_vfs_open(&uri, &content).await;
        self.publish_diagnostics(uri, &content).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Some(change) = params.content_changes.into_iter().next() {
            let content = change.text.clone();
            self.state().await.document_changed(&uri, content.clone());
            self.notify_vfs_change(&uri, &content).await;
            self.publish_diagnostics(uri, &content).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let content = self.state().await.get_document_content(&uri);
        if let Some(content) = content {
            self.publish_diagnostics(uri, &content).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.state().await.document_closed(&uri);
        self.notify_vfs_close(&uri).await;
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let state = self.state().await;
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let Some(raw) = state.get_completion_context(uri, position) else {
            return Ok(None);
        };

        let verbs = ["impl", "verify", "depends", "related"];

        // Check if user is typing a verb (no space yet)
        let is_typing_verb = verbs.iter().any(|v| v.starts_with(&raw) || raw == *v);

        // Offer verb completions
        if is_typing_verb && !raw.contains(' ') {
            let items: Vec<CompletionItem> = verbs
                .iter()
                .filter(|v| v.starts_with(&raw))
                .map(|v| CompletionItem {
                    label: v.to_string(),
                    kind: Some(CompletionItemKind::KEYWORD),
                    detail: Some(format!("{} reference", v)),
                    insert_text: Some(format!("{} ", v)),
                    ..Default::default()
                })
                .collect();
            if !items.is_empty() {
                return Ok(Some(CompletionResponse::Array(items)));
            }
        }

        // Extract requirement prefix (after verb if present)
        let req_prefix = if let Some(space_pos) = raw.find(' ') {
            &raw[space_pos + 1..]
        } else if !is_typing_verb {
            raw.as_str()
        } else {
            ""
        };

        // Suggest rule IDs
        let mut items: Vec<CompletionItem> = Vec::new();
        for spec_data in state.data.forward_by_impl.values() {
            for rule in &spec_data.rules {
                if rule.id.starts_with(req_prefix) || req_prefix.is_empty() {
                    items.push(CompletionItem {
                        label: rule.id.clone(),
                        kind: Some(CompletionItemKind::CONSTANT),
                        detail: Some(truncate_text(&rule.text, 60)),
                        documentation: Some(Documentation::String(rule.text.clone())),
                        ..Default::default()
                    });
                }
            }
        }

        // Limit and sort
        items.sort_by(|a, b| a.label.cmp(&b.label));
        items.truncate(50);

        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(items)))
        }
    }

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let state = self.state().await;
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Get document content
        let Some(content) = state.documents.get(uri.as_str()) else {
            return Ok(None);
        };

        // Find requirement reference at position
        let reqs = tracey_core::Reqs::extract_from_content(std::path::Path::new(""), content);
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i + 1))
            .collect();

        for reference in &reqs.references {
            let ref_line = match line_starts.binary_search(&reference.span.offset) {
                Ok(line) => line,
                Err(line) => line.saturating_sub(1),
            };

            if ref_line as u32 != position.line {
                continue;
            }

            let line_start = line_starts.get(ref_line).copied().unwrap_or(0);
            let ref_col_start = reference.span.offset.saturating_sub(line_start) as u32;
            let ref_col_end = ref_col_start + reference.span.length as u32;

            if position.character >= ref_col_start && position.character <= ref_col_end {
                // Found reference at cursor, show rule info
                if let Some((_, _, rule)) = state.find_rule(&reference.req_id) {
                    let markdown = format!(
                        "## {}\n\n{}\n\n*Verb: {}*",
                        rule.id,
                        rule.text,
                        reference.verb.as_str()
                    );
                    return Ok(Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: markdown,
                        }),
                        range: Some(Range {
                            start: Position {
                                line: position.line,
                                character: ref_col_start,
                            },
                            end: Position {
                                line: position.line,
                                character: ref_col_end,
                            },
                        }),
                    }));
                }
            }
        }

        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let state = self.state().await;
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Get document content
        let Some(content) = state.documents.get(uri.as_str()) else {
            return Ok(None);
        };

        // Find requirement reference at position
        let reqs = tracey_core::Reqs::extract_from_content(std::path::Path::new(""), content);
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i + 1))
            .collect();

        for reference in &reqs.references {
            let ref_line = match line_starts.binary_search(&reference.span.offset) {
                Ok(line) => line,
                Err(line) => line.saturating_sub(1),
            };

            if ref_line as u32 != position.line {
                continue;
            }

            let line_start = line_starts.get(ref_line).copied().unwrap_or(0);
            let ref_col_start = reference.span.offset.saturating_sub(line_start) as u32;
            let ref_col_end = ref_col_start + reference.span.length as u32;

            if position.character >= ref_col_start && position.character <= ref_col_end {
                // Found reference at cursor, jump to definition
                if let Some((_, _, rule)) = state.find_rule(&reference.req_id)
                    && let (Some(file), Some(line)) = (&rule.source_file, rule.source_line)
                {
                    let def_uri =
                        Url::from_file_path(state.project_root.join(file.trim_start_matches("./")))
                            .map_err(|_| {
                                tower_lsp::jsonrpc::Error::invalid_params("Invalid file path")
                            })?;

                    return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                        uri: def_uri,
                        range: Range {
                            start: Position {
                                line: line.saturating_sub(1) as u32,
                                character: 0,
                            },
                            end: Position {
                                line: line.saturating_sub(1) as u32,
                                character: 0,
                            },
                        },
                    })));
                }
            }
        }

        Ok(None)
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        let state = self.state().await;
        let uri = &params.text_document.uri;

        let Some(content) = state.documents.get(uri.as_str()) else {
            return Ok(None);
        };

        let reqs = tracey_core::Reqs::extract_from_content(std::path::Path::new(""), content);
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i + 1))
            .collect();

        let mut tokens: Vec<SemanticToken> = Vec::new();
        let mut prev_line = 0u32;
        let mut prev_char = 0u32;

        for reference in &reqs.references {
            let line = match line_starts.binary_search(&reference.span.offset) {
                Ok(l) => l,
                Err(l) => l.saturating_sub(1),
            };
            let line_start = line_starts.get(line).copied().unwrap_or(0);
            let col = reference.span.offset.saturating_sub(line_start);

            let delta_line = line as u32 - prev_line;
            let delta_start = if delta_line == 0 {
                col as u32 - prev_char
            } else {
                col as u32
            };

            // Token for the entire reference
            let is_valid = state.find_rule(&reference.req_id).is_some();
            tokens.push(SemanticToken {
                delta_line,
                delta_start,
                length: reference.span.length as u32,
                token_type: 2, // VARIABLE for requirement ID
                token_modifiers_bitset: if is_valid { 2 } else { 0 }, // DECLARATION if valid
            });

            prev_line = line as u32;
            prev_char = col as u32;
        }

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        })))
    }
}

/// Truncate text to a maximum length.
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len.saturating_sub(3)])
    }
}
