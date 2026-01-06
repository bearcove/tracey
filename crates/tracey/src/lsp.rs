//! LSP server for tracey
//!
//! Provides IDE features for requirement traceability:
//! - Hover: show requirement text and coverage info
//! - Completions: suggest requirement IDs when typing r[...]
//! - Go-to-definition: jump from reference to spec definition
//!
//! r[impl lsp.lifecycle.stdio]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use eyre::Result;
use tokio::sync::watch;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::serve::DashboardData;
use marq::{RenderOptions, render};

// Semantic token types for requirement references
// r[impl lsp.semantic-tokens.prefix]
// r[impl lsp.semantic-tokens.verb]
// r[impl lsp.semantic-tokens.req-id]
const SEMANTIC_TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE, // 0: prefix (e.g., "r")
    SemanticTokenType::KEYWORD,   // 1: verb (impl, verify, depends, related)
    SemanticTokenType::VARIABLE,  // 2: requirement ID
];

const SEMANTIC_TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DEFINITION, // 0: for definitions in spec files
    SemanticTokenModifier::DECLARATION, // 1: for valid references
];

/// Run the LSP server over stdio
/// r[impl cli.lsp]
pub async fn run(root: Option<PathBuf>, config_path: Option<PathBuf>) -> Result<()> {
    use crate::serve::build_dashboard_data;

    // Determine project root
    let project_root = match root {
        Some(r) => r,
        None => crate::find_project_root()?,
    };

    // Load config
    let config_path = config_path.unwrap_or_else(|| project_root.join(".config/tracey/config.kdl"));
    let config = crate::load_config(&config_path)?;

    // Build initial dashboard data (quiet mode - no stderr output since LSP uses stdio)
    let initial_data = build_dashboard_data(&project_root, &config, 1, true).await?;

    // Create watch channel for data
    let (_data_tx, data_rx) = watch::channel(Arc::new(initial_data));

    // TODO: Add file watching to rebuild data on changes
    // For now, LSP just uses the initial data snapshot

    // Run LSP server
    run_lsp_server(data_rx, project_root).await
}

/// Internal: run the LSP server with pre-built data
async fn run_lsp_server(
    data_rx: watch::Receiver<Arc<DashboardData>>,
    project_root: PathBuf,
) -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client, data_rx, project_root));
    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}

struct Backend {
    #[allow(dead_code)]
    client: Client,
    /// Receiver for dashboard data updates
    data_rx: watch::Receiver<Arc<DashboardData>>,
    /// Project root for resolving paths
    project_root: PathBuf,
    /// Document content cache: uri -> content
    documents: std::sync::RwLock<HashMap<String, String>>,
}

/// Simple Levenshtein distance for string similarity
fn strsim_distance(a: &str, b: &str) -> usize {
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

/// Result of finding a requirement at a position
struct ReqAtPosition {
    /// The requirement ID (e.g., "config.path.default")
    id: String,
    /// Range of the full reference including prefix and brackets (e.g., "r[config.path.default]")
    full_range: Range,
    /// Range of just the requirement ID (e.g., "config.path.default")
    id_range: Range,
}

impl Backend {
    fn new(
        client: Client,
        data_rx: watch::Receiver<Arc<DashboardData>>,
        project_root: PathBuf,
    ) -> Self {
        Self {
            client,
            data_rx,
            project_root,
            documents: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Get current dashboard data
    fn data(&self) -> Arc<DashboardData> {
        self.data_rx.borrow().clone()
    }

    /// Get all valid prefixes from configuration
    fn get_prefixes(&self) -> Vec<String> {
        let data = self.data();
        data.config.specs.iter().map(|s| s.prefix.clone()).collect()
    }

    /// Get all requirement IDs with their descriptions from current data
    fn get_requirements(&self) -> Vec<(String, String, String)> {
        let data = self.data();
        let mut reqs = Vec::new();

        // Collect from all specs
        for (impl_key, spec_data) in &data.forward_by_impl {
            for rule in &spec_data.rules {
                // (id, description/html, spec_name)
                reqs.push((rule.id.clone(), rule.html.clone(), impl_key.0.clone()));
            }
        }

        reqs
    }

    /// Compute diagnostics for a document
    /// r[impl lsp.diagnostics.broken-refs]
    /// r[impl lsp.diagnostics.unknown-prefix]
    /// r[impl lsp.diagnostics.unknown-verb]
    fn compute_diagnostics(&self, _uri: &Url, content: &str) -> Vec<Diagnostic> {
        use tracey_core::{RefVerb, Reqs, WarningKind};

        let mut diagnostics = Vec::new();
        let prefixes = self.get_prefixes();

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
            // r[impl lsp.diagnostics.unknown-prefix]
            if !prefixes.contains(&reference.prefix) {
                let start = offset_to_position(reference.span.offset);
                let end = offset_to_position(reference.span.offset + reference.span.length);
                diagnostics.push(Diagnostic {
                    range: Range { start, end },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("unknown-prefix".to_string())),
                    source: Some("tracey".to_string()),
                    message: format!(
                        "Unknown prefix '{}'. Available prefixes: {}",
                        reference.prefix,
                        prefixes.join(", ")
                    ),
                    ..Default::default()
                });
                continue;
            }

            // r[impl lsp.diagnostics.broken-refs]
            // Only check impl/verify/depends refs, not definitions
            if !matches!(reference.verb, RefVerb::Define)
                && self.find_requirement(&reference.req_id).is_none()
            {
                let start = offset_to_position(reference.span.offset);
                let end = offset_to_position(reference.span.offset + reference.span.length);

                // Find similar requirement IDs for suggestions
                let similar = self.find_similar_requirements(&reference.req_id, 3);
                let suggestion = if !similar.is_empty() {
                    format!(". Did you mean '{}'?", similar.join("', '"))
                } else {
                    String::new()
                };

                diagnostics.push(Diagnostic {
                    range: Range { start, end },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("broken-ref".to_string())),
                    source: Some("tracey".to_string()),
                    message: format!("Unknown requirement '{}'{}", reference.req_id, suggestion),
                    ..Default::default()
                });
            }
        }

        // r[impl lsp.diagnostics.unknown-verb]
        for warning in &reqs.warnings {
            if let WarningKind::UnknownVerb(verb) = &warning.kind {
                let start = offset_to_position(warning.span.offset);
                let end = offset_to_position(warning.span.offset + warning.span.length);
                diagnostics.push(Diagnostic {
                    range: Range { start, end },
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("unknown-verb".to_string())),
                    source: Some("tracey".to_string()),
                    message: format!(
                        "Unknown verb '{}'. Valid verbs: impl, verify, depends, related",
                        verb
                    ),
                    ..Default::default()
                });
            }
        }

        diagnostics
    }

    /// Find requirement IDs similar to the given ID (for suggestions)
    fn find_similar_requirements(&self, query: &str, limit: usize) -> Vec<String> {
        let reqs = self.get_requirements();
        let mut scored: Vec<(String, usize)> = reqs
            .iter()
            .filter_map(|(id, _, _)| {
                let distance = strsim_distance(query, id);
                // Only suggest if reasonably similar (distance < 5)
                if distance < 5 {
                    Some((id.clone(), distance))
                } else {
                    None
                }
            })
            .collect();
        scored.sort_by_key(|(_, d)| *d);
        scored.into_iter().take(limit).map(|(id, _)| id).collect()
    }

    /// Publish diagnostics for a document
    /// r[impl lsp.diagnostics.on-change]
    async fn publish_diagnostics(&self, uri: Url, content: &str) {
        let diagnostics = self.compute_diagnostics(&uri, content);
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    /// Find requirement by ID
    fn find_requirement(&self, req_id: &str) -> Option<RequirementInfo> {
        let data = self.data();

        for (impl_key, spec_data) in &data.forward_by_impl {
            for rule in &spec_data.rules {
                if rule.id == req_id {
                    return Some(RequirementInfo {
                        id: rule.id.clone(),
                        html: rule.html.clone(),
                        source_file: rule.source_file.clone().unwrap_or_default(),
                        source_line: rule.source_line,
                        source_column: rule.source_column,
                        impl_refs: rule.impl_refs.clone(),
                        verify_refs: rule.verify_refs.clone(),
                        depends_refs: rule.depends_refs.clone(),
                        spec_name: impl_key.0.clone(),
                    });
                }
            }
        }

        None
    }

    /// Find requirement reference at position in document
    fn find_req_at_position(&self, uri: &Url, position: Position) -> Option<ReqAtPosition> {
        let docs = self.documents.read().ok()?;
        let content = docs.get(uri.as_str())?;

        let lines: Vec<&str> = content.lines().collect();
        let line = lines.get(position.line as usize)?;

        // Find r[...] pattern at cursor position
        let mut in_bracket = false;
        let mut bracket_start = 0;

        for (i, c) in line.char_indices() {
            if !in_bracket {
                // Look for r[ pattern (or any single-char prefix followed by [)
                if line[i..].starts_with('[') && i > 0 {
                    let prev_char = line[..i].chars().last()?;
                    if prev_char.is_alphabetic() || prev_char.is_numeric() {
                        // Find start of prefix
                        let prefix_start = line[..i]
                            .char_indices()
                            .rev()
                            .take_while(|(_, c)| c.is_alphanumeric())
                            .last()
                            .map(|(idx, _)| idx)
                            .unwrap_or(i - 1);
                        bracket_start = prefix_start;
                        in_bracket = true;
                    }
                }
            } else if c == ']' {
                let bracket_end = i;
                // Check if position is within this range
                if position.character as usize >= bracket_start
                    && position.character as usize <= bracket_end
                {
                    // Extract the content between brackets
                    let bracket_open = line[bracket_start..].find('[')? + bracket_start;
                    let inner = &line[bracket_open + 1..bracket_end];

                    // Parse: might be "impl foo.bar" or just "foo.bar"
                    // For rename, we need to know where the ID starts within the inner content
                    let (req_id, id_offset) = if let Some(space_pos) = inner.find(' ') {
                        (inner[space_pos + 1..].trim(), space_pos + 1)
                    } else {
                        (inner.trim(), 0)
                    };

                    // Calculate ranges
                    let id_start = bracket_open + 1 + id_offset;
                    let id_end = id_start + req_id.len();

                    let full_range = Range {
                        start: Position {
                            line: position.line,
                            character: bracket_start as u32,
                        },
                        end: Position {
                            line: position.line,
                            character: (bracket_end + 1) as u32,
                        },
                    };

                    let id_range = Range {
                        start: Position {
                            line: position.line,
                            character: id_start as u32,
                        },
                        end: Position {
                            line: position.line,
                            character: id_end as u32,
                        },
                    };

                    return Some(ReqAtPosition {
                        id: req_id.to_string(),
                        full_range,
                        id_range,
                    });
                }
                in_bracket = false;
            }
        }

        None
    }

    /// Get raw content after prefix[ up to cursor (for completion logic)
    fn get_completion_context(&self, uri: &Url, position: Position) -> Option<String> {
        let docs = self.documents.read().ok()?;
        let content = docs.get(uri.as_str())?;

        let lines: Vec<&str> = content.lines().collect();
        let line = lines.get(position.line as usize)?;
        let col = position.character as usize;

        let before_cursor = &line[..col.min(line.len())];

        // Find the last prefix[ pattern before cursor
        // Look for patterns like r[, m[, h2[, etc.
        for (i, _) in before_cursor.char_indices().rev() {
            if before_cursor[i..].starts_with('[') && i > 0 {
                let after_bracket = &before_cursor[i + 1..];
                if !after_bracket.contains(']') {
                    return Some(after_bracket.to_string());
                }
            }
        }

        None
    }
}

struct RequirementInfo {
    id: String,
    html: String,
    source_file: String,
    source_line: Option<usize>,
    source_column: Option<usize>,
    impl_refs: Vec<crate::serve::ApiCodeRef>,
    verify_refs: Vec<crate::serve::ApiCodeRef>,
    depends_refs: Vec<crate::serve::ApiCodeRef>,
    #[allow(dead_code)]
    spec_name: String,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    // r[impl lsp.lifecycle.initialize]
    async fn initialize(&self, _: InitializeParams) -> LspResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // r[impl lsp.completions.trigger]
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "[".to_string(),
                        ".".to_string(),
                        " ".to_string(),
                    ]),
                    ..Default::default()
                }),
                // r[impl lsp.hover.req-reference]
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                // r[impl lsp.goto.ref-to-def]
                definition_provider: Some(OneOf::Left(true)),
                // r[impl lsp.highlight.full-range]
                document_highlight_provider: Some(OneOf::Left(true)),
                // r[impl lsp.impl.from-ref]
                implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
                // r[impl lsp.references.from-reference]
                references_provider: Some(OneOf::Left(true)),
                // r[impl lsp.workspace-symbols.requirements]
                workspace_symbol_provider: Some(OneOf::Left(true)),
                // r[impl lsp.symbols.requirements]
                document_symbol_provider: Some(OneOf::Left(true)),
                // r[impl lsp.rename.req-id]
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                // r[impl lsp.semantic-tokens.prefix]
                // r[impl lsp.semantic-tokens.verb]
                // r[impl lsp.semantic-tokens.req-id]
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: SEMANTIC_TOKEN_TYPES.to_vec(),
                                token_modifiers: SEMANTIC_TOKEN_MODIFIERS.to_vec(),
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: None,
                            ..Default::default()
                        },
                    ),
                ),
                // r[impl lsp.actions.create-requirement]
                // r[impl lsp.actions.open-dashboard]
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                // r[impl lsp.inlay.coverage-status]
                // r[impl lsp.inlay.impl-count]
                inlay_hint_provider: Some(OneOf::Left(true)),
                // r[impl lsp.codelens.coverage]
                // r[impl lsp.codelens.run-test]
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                // Sync full document content
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "tracey".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    // r[impl lsp.diagnostics.on-change]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let content = params.text_document.text.clone();
        if let Ok(mut docs) = self.documents.write() {
            docs.insert(uri.to_string(), content.clone());
        }
        // Publish diagnostics for the opened document
        self.publish_diagnostics(uri, &content).await;
    }

    // r[impl lsp.diagnostics.on-change]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Some(change) = params.content_changes.into_iter().next() {
            let content = change.text.clone();
            if let Ok(mut docs) = self.documents.write() {
                docs.insert(uri.to_string(), content.clone());
            }
            // Publish diagnostics for the changed document
            self.publish_diagnostics(uri, &content).await;
        }
    }

    // r[impl lsp.diagnostics.on-save]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        // Re-compute diagnostics on save for accurate results
        let content = {
            let docs = self.documents.read();
            docs.ok().and_then(|d| d.get(uri.as_str()).cloned())
        };
        if let Some(content) = content {
            self.publish_diagnostics(uri, &content).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Ok(mut docs) = self.documents.write() {
            docs.remove(uri.as_str());
        }
        // Clear diagnostics when file is closed
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    // r[impl lsp.completions.req-id]
    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let Some(raw) = self.get_completion_context(uri, position) else {
            return Ok(None);
        };

        let verbs = ["impl", "verify", "depends", "related"];

        // Check if user is typing a verb (no space yet)
        let is_typing_verb = verbs.iter().any(|v| v.starts_with(&raw) || raw == *v);

        // r[impl lsp.completions.verb]
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

        // r[impl lsp.completions.req-id-fuzzy]
        let requirements = self.get_requirements();
        let items: Vec<CompletionItem> = requirements
            .iter()
            .filter(|(id, _, _)| req_prefix.is_empty() || id.contains(req_prefix))
            .map(|(id, html, spec)| {
                // Strip HTML tags for plain text description
                let plain_text = html
                    .replace("<p>", "")
                    .replace("</p>", "")
                    .replace("<code>", "`")
                    .replace("</code>", "`");

                // r[impl lsp.completions.req-id-preview]
                CompletionItem {
                    label: id.clone(),
                    kind: Some(CompletionItemKind::REFERENCE),
                    detail: Some(format!("[{}]", spec)),
                    documentation: Some(Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("**{}**\n\n{}", id, plain_text),
                    })),
                    ..Default::default()
                }
            })
            .collect();

        if !items.is_empty() {
            return Ok(Some(CompletionResponse::Array(items)));
        }

        Ok(None)
    }

    // r[impl lsp.hover.req-reference]
    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let Some(req) = self.find_req_at_position(uri, position) else {
            return Ok(None);
        };

        if let Some(info) = self.find_requirement(&req.id) {
            // Strip HTML for markdown display
            let plain_text = info
                .html
                .replace("<p>", "")
                .replace("</p>", "\n\n")
                .replace("<code>", "`")
                .replace("</code>", "`");

            // r[impl lsp.hover.req-reference-format]
            // Coverage info (impl/test counts) is shown via inlay hints, so hover just shows the requirement text
            let content = format!(
                "### {}\n\n{}\n\n*Defined in: {}*",
                info.id,
                plain_text.trim(),
                info.source_file
            );

            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: content,
                }),
                range: Some(req.full_range),
            }));
        }

        // Unknown requirement
        // r[impl lsp.diagnostics.broken-refs-message]
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("⚠️ **Unknown requirement:** `{}`", req.id),
            }),
            range: Some(req.full_range),
        }))
    }

    // r[impl lsp.goto.ref-to-def]
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let Some(req) = self.find_req_at_position(uri, position) else {
            return Ok(None);
        };

        let Some(info) = self.find_requirement(&req.id) else {
            return Ok(None);
        };

        // Construct path to spec file
        let spec_path = self.project_root.join(&info.source_file);

        let Ok(target_uri) = Url::from_file_path(&spec_path) else {
            return Ok(None);
        };

        // Use source_line/column if available (convert from 1-indexed to 0-indexed)
        let line = info
            .source_line
            .map(|l| l.saturating_sub(1) as u32)
            .unwrap_or(0);
        let character = info
            .source_column
            .map(|c| c.saturating_sub(1) as u32)
            .unwrap_or(0);

        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri: target_uri,
            range: Range {
                start: Position { line, character },
                end: Position { line, character },
            },
        })))
    }

    // r[impl lsp.impl.from-ref]
    // r[impl lsp.impl.multiple]
    async fn goto_implementation(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let Some(req) = self.find_req_at_position(uri, position) else {
            return Ok(None);
        };

        let Some(info) = self.find_requirement(&req.id) else {
            return Ok(None);
        };

        if info.impl_refs.is_empty() {
            return Ok(None);
        }

        // Convert impl refs to locations
        let locations: Vec<Location> = info
            .impl_refs
            .iter()
            .filter_map(|r| {
                let path = self.project_root.join(&r.file);
                let uri = Url::from_file_path(&path).ok()?;
                let line = r.line.saturating_sub(1) as u32;
                Some(Location {
                    uri,
                    range: Range {
                        start: Position { line, character: 0 },
                        end: Position { line, character: 0 },
                    },
                })
            })
            .collect();

        if locations.is_empty() {
            return Ok(None);
        }

        // Return single location or array depending on count
        if locations.len() == 1 {
            Ok(Some(GotoDefinitionResponse::Scalar(
                locations.into_iter().next().unwrap(),
            )))
        } else {
            Ok(Some(GotoDefinitionResponse::Array(locations)))
        }
    }

    // r[impl lsp.references.from-reference]
    // r[impl lsp.references.from-definition]
    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let Some(req) = self.find_req_at_position(uri, position) else {
            return Ok(None);
        };

        let Some(info) = self.find_requirement(&req.id) else {
            return Ok(None);
        };

        let mut locations = Vec::new();

        // Helper to convert ApiCodeRef to Location
        let to_location = |r: &crate::serve::ApiCodeRef| -> Option<Location> {
            let path = self.project_root.join(&r.file);
            let uri = Url::from_file_path(&path).ok()?;
            let line = r.line.saturating_sub(1) as u32;
            Some(Location {
                uri,
                range: Range {
                    start: Position { line, character: 0 },
                    end: Position { line, character: 0 },
                },
            })
        };

        // Add definition location if include_declaration is true
        if params.context.include_declaration
            && !info.source_file.is_empty()
            && let Ok(uri) = Url::from_file_path(self.project_root.join(&info.source_file))
        {
            let line = info
                .source_line
                .map(|l| l.saturating_sub(1) as u32)
                .unwrap_or(0);
            let character = info
                .source_column
                .map(|c| c.saturating_sub(1) as u32)
                .unwrap_or(0);
            locations.push(Location {
                uri,
                range: Range {
                    start: Position { line, character },
                    end: Position { line, character },
                },
            });
        }

        // Add all impl refs
        locations.extend(info.impl_refs.iter().filter_map(to_location));

        // Add all verify refs
        locations.extend(info.verify_refs.iter().filter_map(to_location));

        // Add all depends refs
        locations.extend(info.depends_refs.iter().filter_map(to_location));

        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    // r[impl lsp.highlight.full-range]
    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> LspResult<Option<Vec<DocumentHighlight>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Find requirement at position and return its full range
        let Some(req) = self.find_req_at_position(uri, position) else {
            return Ok(None);
        };

        Ok(Some(vec![DocumentHighlight {
            range: req.full_range,
            kind: Some(DocumentHighlightKind::TEXT),
        }]))
    }

    // r[impl lsp.symbols.requirements]
    // r[impl lsp.symbols.references]
    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> LspResult<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.to_string();

        let docs = self.documents.read().ok();
        let content = docs.as_ref().and_then(|d| d.get(&uri));

        let Some(content) = content else {
            return Ok(None);
        };

        let mut symbols = Vec::new();

        // Scan for requirement references: prefix[verb? req.id]
        // Pattern: word followed by [ then content then ]
        for (line_num, line) in content.lines().enumerate() {
            let mut i = 0;
            let chars: Vec<char> = line.chars().collect();

            while i < chars.len() {
                // Look for opening bracket preceded by alphanumeric
                if chars[i] == '[' && i > 0 && chars[i - 1].is_alphanumeric() {
                    // Find start of prefix
                    let mut prefix_start = i - 1;
                    while prefix_start > 0 && chars[prefix_start - 1].is_alphanumeric() {
                        prefix_start -= 1;
                    }

                    // Find closing bracket
                    if let Some(close_pos) = chars[i + 1..].iter().position(|&c| c == ']') {
                        let close_idx = i + 1 + close_pos;
                        let inner: String = chars[i + 1..close_idx].iter().collect();

                        // Parse inner content - might be "verb req.id" or just "req.id"
                        let req_id = if let Some(space_pos) = inner.find(' ') {
                            inner[space_pos + 1..].trim().to_string()
                        } else {
                            inner.trim().to_string()
                        };

                        if !req_id.is_empty() && req_id.contains('.') {
                            #[allow(deprecated)]
                            symbols.push(SymbolInformation {
                                name: req_id,
                                kind: SymbolKind::CONSTANT,
                                tags: None,
                                deprecated: None,
                                location: Location {
                                    uri: Url::parse(&uri)
                                        .unwrap_or_else(|_| Url::parse("file:///unknown").unwrap()),
                                    range: Range {
                                        start: Position {
                                            line: line_num as u32,
                                            character: prefix_start as u32,
                                        },
                                        end: Position {
                                            line: line_num as u32,
                                            character: (close_idx + 1) as u32,
                                        },
                                    },
                                },
                                container_name: None,
                            });
                        }

                        i = close_idx + 1;
                        continue;
                    }
                }
                i += 1;
            }
        }

        if symbols.is_empty() {
            Ok(None)
        } else {
            #[allow(deprecated)]
            Ok(Some(DocumentSymbolResponse::Flat(symbols)))
        }
    }

    // r[impl lsp.workspace-symbols.requirements]
    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> LspResult<Option<Vec<SymbolInformation>>> {
        let query = params.query.to_lowercase();
        let data = self.data();

        let mut symbols = Vec::new();

        for spec_data in data.forward_by_impl.values() {
            for rule in &spec_data.rules {
                // Match if query is empty or requirement ID contains query
                if (query.is_empty() || rule.id.to_lowercase().contains(&query))
                    && let Some(ref source_file) = rule.source_file
                    && let Ok(uri) = Url::from_file_path(self.project_root.join(source_file))
                {
                    let line = rule
                        .source_line
                        .map(|l| l.saturating_sub(1) as u32)
                        .unwrap_or(0);
                    let character = rule
                        .source_column
                        .map(|c| c.saturating_sub(1) as u32)
                        .unwrap_or(0);

                    #[allow(deprecated)]
                    // SymbolInformation::deprecated is deprecated but required
                    symbols.push(SymbolInformation {
                        name: rule.id.clone(),
                        kind: SymbolKind::CONSTANT, // Using CONSTANT for requirements
                        tags: None,
                        deprecated: None,
                        location: Location {
                            uri,
                            range: Range {
                                start: Position { line, character },
                                end: Position { line, character },
                            },
                        },
                        container_name: Some("requirements".to_string()),
                    });
                }
            }
        }

        // Sort by ID for consistent ordering
        symbols.sort_by(|a, b| a.name.cmp(&b.name));

        // Limit results to avoid overwhelming the UI
        symbols.truncate(100);

        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(symbols))
        }
    }

    // r[impl lsp.rename.prepare]
    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> LspResult<Option<PrepareRenameResponse>> {
        let uri = &params.text_document.uri;
        let position = params.position;

        let Some(req) = self.find_req_at_position(uri, position) else {
            return Ok(None);
        };

        // Check if requirement exists
        if self.find_requirement(&req.id).is_none() {
            return Ok(None);
        }

        // Return the range of just the requirement ID (not the full r[...] reference)
        // This way the editor will pre-fill only the ID text, and the user types the new ID directly
        Ok(Some(PrepareRenameResponse::Range(req.id_range)))
    }

    // r[impl lsp.rename.req-id]
    async fn rename(&self, params: RenameParams) -> LspResult<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = &params.new_name;

        let Some(req) = self.find_req_at_position(uri, position) else {
            return Ok(None);
        };

        let Some(info) = self.find_requirement(&req.id) else {
            return Ok(None);
        };

        // r[impl lsp.rename.validation]
        // Validate new name format (should be dotted identifier)
        if !new_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return Err(tower_lsp::jsonrpc::Error::invalid_params(
                "Requirement ID must only contain alphanumeric characters, dots, hyphens, and underscores",
            ));
        }

        if !new_name.contains('.') {
            return Err(tower_lsp::jsonrpc::Error::invalid_params(
                "Requirement ID must contain at least one dot (e.g., 'section.name')",
            ));
        }

        // Check if new name conflicts with existing requirement
        if self.find_requirement(new_name).is_some() {
            return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
                "Requirement '{}' already exists",
                new_name
            )));
        }

        let mut changes: std::collections::HashMap<Url, Vec<TextEdit>> =
            std::collections::HashMap::new();

        // Helper to add a text edit
        let mut add_edit = |file: &str, line: usize, old_id: &str, new_id: &str| {
            let path = self.project_root.join(file);
            if let Ok(uri) = Url::from_file_path(&path)
                && let Ok(content) = std::fs::read_to_string(&path)
                && let Some(file_line) = content.lines().nth(line.saturating_sub(1))
                && let Some(id_pos) = file_line.find(old_id)
            {
                let line_num = line.saturating_sub(1) as u32;
                let edit = TextEdit {
                    range: Range {
                        start: Position {
                            line: line_num,
                            character: id_pos as u32,
                        },
                        end: Position {
                            line: line_num,
                            character: (id_pos + old_id.len()) as u32,
                        },
                    },
                    new_text: new_id.to_string(),
                };
                changes.entry(uri).or_default().push(edit);
            }
        };

        // Add edit for the definition in spec file
        if !info.source_file.is_empty()
            && let Some(line) = info.source_line
        {
            add_edit(&info.source_file, line, &req.id, new_name);
        }

        // Add edits for all impl refs
        for r in &info.impl_refs {
            add_edit(&r.file, r.line, &req.id, new_name);
        }

        // Add edits for all verify refs
        for r in &info.verify_refs {
            add_edit(&r.file, r.line, &req.id, new_name);
        }

        // Add edits for all depends refs
        for r in &info.depends_refs {
            add_edit(&r.file, r.line, &req.id, new_name);
        }

        if changes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            }))
        }
    }

    // r[impl lsp.semantic-tokens.prefix]
    // r[impl lsp.semantic-tokens.verb]
    // r[impl lsp.semantic-tokens.req-id]
    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        use tracey_core::{RefVerb, Reqs};

        let uri = &params.text_document.uri;

        let content = {
            let docs = self.documents.read();
            docs.ok().and_then(|d| d.get(uri.as_str()).cloned())
        };

        let Some(content) = content else {
            return Ok(None);
        };

        let prefixes = self.get_prefixes();
        let reqs = Reqs::extract_from_content(std::path::Path::new(""), &content);

        // Build line starts for byte offset to line/column conversion
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i + 1))
            .collect();

        let offset_to_line_col = |offset: usize| -> (u32, u32) {
            let line = match line_starts.binary_search(&offset) {
                Ok(line) => line,
                Err(line) => line.saturating_sub(1),
            };
            let line_start = line_starts.get(line).copied().unwrap_or(0);
            let column = offset.saturating_sub(line_start);
            (line as u32, column as u32)
        };

        // Collect tokens for each reference, sorted by position
        let mut token_data: Vec<(u32, u32, u32, u32, u32)> = Vec::new(); // (line, col, len, type, modifiers)

        for reference in &reqs.references {
            // Only process known prefixes
            if !prefixes.contains(&reference.prefix) {
                continue;
            }

            let (line, start_col) = offset_to_line_col(reference.span.offset);

            // Token for the entire reference: prefix[verb req_id] or prefix[req_id]
            // We'll highlight the whole thing as a requirement ID
            let is_definition = matches!(reference.verb, RefVerb::Define);
            let modifier_bits = if is_definition { 1 } else { 2 }; // DEFINITION or DECLARATION

            // Token type 2 = requirement ID (VARIABLE)
            token_data.push((
                line,
                start_col,
                reference.span.length as u32,
                2,
                modifier_bits,
            ));
        }

        // Sort by line, then column
        token_data.sort_by_key(|(line, col, _, _, _)| (*line, *col));

        // Convert to delta encoding
        let mut tokens = Vec::new();
        let mut prev_line = 0u32;
        let mut prev_col = 0u32;

        for (line, col, len, token_type, modifiers) in token_data {
            let delta_line = line - prev_line;
            let delta_col = if delta_line == 0 { col - prev_col } else { col };

            tokens.push(SemanticToken {
                delta_line,
                delta_start: delta_col,
                length: len,
                token_type,
                token_modifiers_bitset: modifiers,
            });

            prev_line = line;
            prev_col = col;
        }

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        })))
    }

    // r[impl lsp.actions.create-requirement]
    // r[impl lsp.actions.open-dashboard]
    async fn code_action(&self, params: CodeActionParams) -> LspResult<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let position = params.range.start;

        let Some(req) = self.find_req_at_position(uri, position) else {
            return Ok(None);
        };

        let mut actions = Vec::new();

        // Check if requirement exists
        let req_exists = self.find_requirement(&req.id).is_some();

        if !req_exists {
            // r[impl lsp.actions.create-requirement]
            // Offer to create the requirement in the spec file
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Create requirement '{}'", req.id),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: None,
                edit: None, // We'll use a command instead for complex logic
                command: Some(Command {
                    title: format!("Create requirement '{}'", req.id),
                    command: "tracey.createRequirement".to_string(),
                    arguments: Some(vec![serde_json::Value::String(req.id.clone())]),
                }),
                is_preferred: Some(true),
                disabled: None,
                data: None,
            }));
        }

        // r[impl lsp.actions.open-dashboard]
        // Always offer to open in dashboard
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: format!("Open '{}' in dashboard", req.id),
            kind: Some(CodeActionKind::SOURCE),
            diagnostics: None,
            edit: None,
            command: Some(Command {
                title: format!("Open '{}' in dashboard", req.id),
                command: "tracey.openInDashboard".to_string(),
                arguments: Some(vec![serde_json::Value::String(req.id.clone())]),
            }),
            is_preferred: None,
            disabled: None,
            data: None,
        }));

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }

    // r[impl lsp.inlay.coverage-status]
    // r[impl lsp.inlay.impl-count]
    async fn inlay_hint(&self, params: InlayHintParams) -> LspResult<Option<Vec<InlayHint>>> {
        use tracey_core::{RefVerb, Reqs};

        let uri = &params.text_document.uri;
        let is_markdown = uri.path().ends_with(".md");

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "inlay_hint called: uri={}, is_markdown={}",
                    uri, is_markdown
                ),
            )
            .await;

        let content = {
            let docs = match self.documents.read() {
                Ok(d) => d,
                Err(_) => return Ok(None),
            };
            docs.get(uri.as_str()).cloned()
        };

        let Some(content) = content else {
            self.client
                .log_message(MessageType::WARNING, "inlay_hint: no content found")
                .await;
            return Ok(None);
        };

        let prefixes = self.get_prefixes();

        // For markdown files, use marq to extract requirement definitions
        let markdown_reqs = if is_markdown {
            match render(&content, &RenderOptions::default()).await {
                Ok(doc) => {
                    self.client
                        .log_message(
                            MessageType::INFO,
                            format!("marq extracted {} reqs", doc.reqs.len()),
                        )
                        .await;
                    doc.reqs
                }
                Err(e) => {
                    self.client
                        .log_message(MessageType::ERROR, format!("marq extraction error: {}", e))
                        .await;
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        let reqs = Reqs::extract_from_content(std::path::Path::new(""), &content);

        // Build line starts for byte offset to line/column conversion
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i + 1))
            .collect();

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

        let mut hints = Vec::new();

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "inlay_hint range: lines {}-{}",
                    params.range.start.line, params.range.end.line
                ),
            )
            .await;

        // Log first few reqs to understand positioning
        for (i, req_def) in markdown_reqs.iter().take(5).enumerate() {
            let marker_line_end = content[req_def.span.offset..]
                .find('\n')
                .map(|n| req_def.span.offset + n)
                .unwrap_or(req_def.span.offset + req_def.span.length);
            let pos = offset_to_position(marker_line_end);
            self.client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "req[{}]: id={}, marq_line={}, span.offset={}, span.len={}, calculated_line={}",
                        i, req_def.id, req_def.line, req_def.span.offset, req_def.span.length, pos.line
                    ),
                )
                .await;
        }

        // Process markdown requirement definitions
        let mut in_range_count = 0;
        let mut in_range_ids: Vec<String> = Vec::new();
        for req_def in &markdown_reqs {
            // Position after the requirement marker line (NOT after all content)
            // span.length includes all content, but we want just the r[req.id] line
            // Find the end of the first line from span.offset
            let marker_line_end = content[req_def.span.offset..]
                .find('\n')
                .map(|n| req_def.span.offset + n)
                .unwrap_or(req_def.span.offset + req_def.span.length);
            let position = offset_to_position(marker_line_end);

            // Check if position is within requested range
            if position.line < params.range.start.line || position.line > params.range.end.line {
                continue;
            }
            in_range_count += 1;
            in_range_ids.push(format!("{}@L{}", req_def.id, position.line));

            // Look up the requirement coverage
            let label = if let Some(req_info) = self.find_requirement(&req_def.id) {
                let impl_count = req_info.impl_refs.len();
                let verify_count = req_info.verify_refs.len();

                // r[impl lsp.inlay.impl-count]
                if impl_count == 0 && verify_count == 0 {
                    " ← uncovered".to_string()
                } else {
                    format!(
                        " ← {} impl{}, {} test{}",
                        impl_count,
                        if impl_count == 1 { "" } else { "s" },
                        verify_count,
                        if verify_count == 1 { "" } else { "s" }
                    )
                }
            } else {
                " ← (unknown)".to_string()
            };

            hints.push(InlayHint {
                position,
                label: InlayHintLabel::String(label),
                kind: None,
                text_edits: None,
                tooltip: None,
                padding_left: Some(false),
                padding_right: Some(true),
                data: None,
            });
        }

        // Process source file refs (non-markdown)
        for reference in &reqs.references {
            // Only process known prefixes
            if !prefixes.contains(&reference.prefix) {
                continue;
            }

            // Position after the reference
            let end_offset = reference.span.offset + reference.span.length;
            let position = offset_to_position(end_offset);

            // Check if position is within requested range
            if position.line < params.range.start.line || position.line > params.range.end.line {
                continue;
            }

            // Look up the requirement
            let label = if let Some(req_info) = self.find_requirement(&reference.req_id) {
                let impl_count = req_info.impl_refs.len();
                let verify_count = req_info.verify_refs.len();

                match reference.verb {
                    // r[impl lsp.inlay.impl-count]
                    // Show implementation counts on definitions
                    RefVerb::Define => {
                        if impl_count == 0 && verify_count == 0 {
                            " ← uncovered".to_string()
                        } else {
                            format!(
                                " ← {} impl{}, {} test{}",
                                impl_count,
                                if impl_count == 1 { "" } else { "s" },
                                verify_count,
                                if verify_count == 1 { "" } else { "s" }
                            )
                        }
                    }
                    // r[impl lsp.inlay.coverage-status]
                    // Show coverage status icons on references
                    _ => {
                        if verify_count > 0 {
                            " ✓".to_string() // Has tests
                        } else if impl_count > 0 {
                            " ⚠".to_string() // Implemented but not tested
                        } else {
                            " ✗".to_string() // Not implemented
                        }
                    }
                }
            } else {
                // Unknown requirement
                match reference.verb {
                    RefVerb::Define => " ← (unknown)".to_string(),
                    _ => " ?".to_string(),
                }
            };

            hints.push(InlayHint {
                position,
                label: InlayHintLabel::String(label),
                kind: None,
                text_edits: None,
                tooltip: None,
                padding_left: Some(false),
                padding_right: Some(true),
                data: None,
            });
        }

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "inlay_hint: {} reqs in range, returning {} hints: {:?}",
                    in_range_count,
                    hints.len(),
                    in_range_ids
                ),
            )
            .await;

        if hints.is_empty() {
            Ok(None)
        } else {
            Ok(Some(hints))
        }
    }

    // r[impl lsp.codelens.coverage]
    // r[impl lsp.codelens.run-test]
    async fn code_lens(&self, params: CodeLensParams) -> LspResult<Option<Vec<CodeLens>>> {
        use tracey_core::{RefVerb, Reqs};

        let uri = &params.text_document.uri;
        let is_markdown = uri.path().ends_with(".md");

        let content = {
            let docs = match self.documents.read() {
                Ok(d) => d,
                Err(_) => return Ok(None),
            };
            docs.get(uri.as_str()).cloned()
        };

        let Some(content) = content else {
            return Ok(None);
        };

        let prefixes = self.get_prefixes();

        // For markdown files, use marq to extract requirement definitions
        let markdown_reqs = if is_markdown {
            match render(&content, &RenderOptions::default()).await {
                Ok(doc) => doc.reqs,
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };

        let reqs = Reqs::extract_from_content(std::path::Path::new(""), &content);

        // Build line starts for byte offset to line/column conversion
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i + 1))
            .collect();

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

        let mut lenses = Vec::new();

        // Process markdown requirement definitions
        for req_def in &markdown_reqs {
            // Range should be just the marker line, not the entire content block
            let start_pos = offset_to_position(req_def.span.offset);
            let marker_line_end = content[req_def.span.offset..]
                .find('\n')
                .map(|n| req_def.span.offset + n)
                .unwrap_or(req_def.span.offset + req_def.span.length);
            let end_pos = offset_to_position(marker_line_end);
            let range = Range {
                start: start_pos,
                end: end_pos,
            };

            // r[impl lsp.codelens.coverage]
            if let Some(req_info) = self.find_requirement(&req_def.id) {
                let impl_count = req_info.impl_refs.len();
                let verify_count = req_info.verify_refs.len();

                let title = format!(
                    "{} impl{}, {} test{}",
                    impl_count,
                    if impl_count == 1 { "" } else { "s" },
                    verify_count,
                    if verify_count == 1 { "" } else { "s" }
                );

                // r[impl lsp.codelens.clickable]
                lenses.push(CodeLens {
                    range,
                    command: Some(Command {
                        title,
                        command: "tracey.showReferences".to_string(),
                        arguments: Some(vec![serde_json::Value::String(req_def.id.clone())]),
                    }),
                    data: None,
                });
            }
        }

        // Process source file refs (non-markdown)
        for reference in &reqs.references {
            // Only process known prefixes
            if !prefixes.contains(&reference.prefix) {
                continue;
            }

            let start_pos = offset_to_position(reference.span.offset);
            let end_pos = offset_to_position(reference.span.offset + reference.span.length);
            let range = Range {
                start: start_pos,
                end: end_pos,
            };

            match reference.verb {
                // r[impl lsp.codelens.coverage]
                // Show coverage counts on requirement definitions
                RefVerb::Define => {
                    if let Some(req_info) = self.find_requirement(&reference.req_id) {
                        let impl_count = req_info.impl_refs.len();
                        let verify_count = req_info.verify_refs.len();

                        let title = format!(
                            "{} impl{}, {} test{}",
                            impl_count,
                            if impl_count == 1 { "" } else { "s" },
                            verify_count,
                            if verify_count == 1 { "" } else { "s" }
                        );

                        // r[impl lsp.codelens.clickable]
                        lenses.push(CodeLens {
                            range,
                            command: Some(Command {
                                title,
                                command: "tracey.showReferences".to_string(),
                                arguments: Some(vec![serde_json::Value::String(
                                    reference.req_id.clone(),
                                )]),
                            }),
                            data: None,
                        });
                    }
                }
                // r[impl lsp.codelens.run-test]
                // Offer to run test from verify refs
                RefVerb::Verify => {
                    lenses.push(CodeLens {
                        range,
                        command: Some(Command {
                            title: "▶ Run test".to_string(),
                            command: "tracey.runTest".to_string(),
                            arguments: Some(vec![
                                serde_json::Value::String(uri.to_string()),
                                serde_json::Value::Number((reference.line as i64).into()),
                            ]),
                        }),
                        data: None,
                    });
                }
                _ => {}
            }
        }

        if lenses.is_empty() {
            Ok(None)
        } else {
            Ok(Some(lenses))
        }
    }
}
