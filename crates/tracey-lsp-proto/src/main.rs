//! Minimal LSP prototype to test if completions and go-to-definition work
//! in comments alongside rust-analyzer.
//!
//! Run with: cargo run -p tracey-lsp-proto

use std::collections::HashMap;
use std::sync::RwLock;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

/// Fake requirements database for testing
fn fake_requirements() -> HashMap<String, (String, String)> {
    let mut reqs = HashMap::new();
    // (req_id) -> (description, file:line where it's "defined")
    reqs.insert(
        "auth.token.validation".to_string(),
        (
            "The system must validate tokens before granting access.".to_string(),
            "docs/spec/auth.md:42".to_string(),
        ),
    );
    reqs.insert(
        "auth.token.refresh".to_string(),
        (
            "Tokens must be refreshable before expiration.".to_string(),
            "docs/spec/auth.md:67".to_string(),
        ),
    );
    reqs.insert(
        "auth.token.expiry".to_string(),
        (
            "Tokens must expire after the configured TTL.".to_string(),
            "docs/spec/auth.md:89".to_string(),
        ),
    );
    reqs.insert(
        "api.response.format".to_string(),
        (
            "API responses must use JSON format with proper content-type.".to_string(),
            "docs/spec/api.md:15".to_string(),
        ),
    );
    reqs.insert(
        "api.error.codes".to_string(),
        (
            "Errors must include appropriate HTTP status codes.".to_string(),
            "docs/spec/api.md:34".to_string(),
        ),
    );
    reqs
}

#[derive(Debug)]
struct Backend {
    client: Client,
    /// Document content cache: uri -> content
    documents: RwLock<HashMap<String, String>>,
    /// Fake requirements for testing
    requirements: HashMap<String, (String, String)>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: RwLock::new(HashMap::new()),
            requirements: fake_requirements(),
        }
    }

    /// Find requirement reference at position, returns (req_id, range)
    fn find_req_at_position(&self, uri: &Url, position: Position) -> Option<(String, Range)> {
        let docs = self.documents.read().ok()?;
        let content = docs.get(uri.as_str())?;

        let lines: Vec<&str> = content.lines().collect();
        let line = lines.get(position.line as usize)?;

        // Simple pattern matching for r[...] or r[verb ...]
        // Look for pattern like r[impl foo.bar] or r[foo.bar]
        let mut in_bracket = false;
        let mut bracket_start = 0;

        for (i, c) in line.char_indices() {
            if !in_bracket {
                // Look for r[ pattern
                if c == 'r' && line[i..].starts_with("r[") {
                    bracket_start = i;
                    in_bracket = true;
                }
            } else if c == ']' {
                let bracket_end = i;
                // Check if position is within this range
                if position.character as usize >= bracket_start
                    && position.character as usize <= bracket_end
                {
                    // Extract the content between brackets
                    let inner = &line[bracket_start + 2..bracket_end];
                    // Parse: might be "impl foo.bar" or just "foo.bar"
                    let req_id = if let Some(space_pos) = inner.find(' ') {
                        inner[space_pos + 1..].trim()
                    } else {
                        inner.trim()
                    };

                    let range = Range {
                        start: Position {
                            line: position.line,
                            character: bracket_start as u32,
                        },
                        end: Position {
                            line: position.line,
                            character: (bracket_end + 1) as u32,
                        },
                    };

                    return Some((req_id.to_string(), range));
                }
                in_bracket = false;
            }
        }

        None
    }

    /// Get raw content after r[ up to cursor (for completion logic)
    fn get_raw_completion_context(&self, uri: &Url, position: Position) -> Option<String> {
        let docs = self.documents.read().ok()?;
        let content = docs.get(uri.as_str())?;

        let lines: Vec<&str> = content.lines().collect();
        let line = lines.get(position.line as usize)?;
        let col = position.character as usize;

        // Look backwards from cursor for r[ pattern
        let before_cursor = &line[..col.min(line.len())];

        // Find the last r[ before cursor
        if let Some(bracket_pos) = before_cursor.rfind("r[") {
            let after_bracket = &before_cursor[bracket_pos + 2..];
            // Check we're not past a closing bracket
            if !after_bracket.contains(']') {
                return Some(after_bracket.to_string());
            }
        }

        None
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // Enable completions
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["[".to_string(), ".".to_string()]),
                    ..Default::default()
                }),
                // Enable hover
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                // Enable go-to-definition
                definition_provider: Some(OneOf::Left(true)),
                // Sync full document content
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "tracey-lsp-proto".to_string(),
                version: Some("0.1.0".to_string()),
            }),
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let content = params.text_document.text;
        if let Ok(mut docs) = self.documents.write() {
            docs.insert(uri, content);
        }
        self.client
            .log_message(MessageType::INFO, "Document opened")
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        if let Some(change) = params.content_changes.into_iter().next() {
            if let Ok(mut docs) = self.documents.write() {
                docs.insert(uri, change.text);
            }
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        if let Ok(mut docs) = self.documents.write() {
            docs.remove(&uri);
        }
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        self.client
            .log_message(
                MessageType::INFO,
                format!("Completion requested at {:?}", position),
            )
            .await;

        // Get what's after r[ up to cursor
        let raw_context = self.get_raw_completion_context(uri, position);

        if let Some(raw) = &raw_context {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("Raw completion context: '{}'", raw),
                )
                .await;

            let verbs = ["impl", "verify", "depends", "related"];

            // Check if user is typing a verb (no space yet)
            let is_typing_verb = verbs.iter().any(|v| v.starts_with(raw) || raw == *v);

            if is_typing_verb && !raw.contains(' ') {
                // Offer verb completions
                let items: Vec<CompletionItem> = verbs
                    .iter()
                    .filter(|v| v.starts_with(raw))
                    .map(|v| CompletionItem {
                        label: v.to_string(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        detail: Some(format!("{} reference", v)),
                        insert_text: Some(format!("{} ", v)), // Add space after verb
                        ..Default::default()
                    })
                    .collect();
                if !items.is_empty() {
                    return Ok(Some(CompletionResponse::Array(items)));
                }
            }

            // Check if we have "verb " or "verb prefix" - offer requirement IDs
            let req_prefix = if let Some(space_pos) = raw.find(' ') {
                &raw[space_pos + 1..]
            } else if !is_typing_verb {
                // No verb, just typing requirement ID directly like r[auth.
                raw.as_str()
            } else {
                "" // Still typing verb
            };

            // Offer requirement completions
            let items: Vec<CompletionItem> = self
                .requirements
                .iter()
                .filter(|(id, _)| req_prefix.is_empty() || id.contains(req_prefix))
                .map(|(id, (desc, _))| CompletionItem {
                    label: id.clone(),
                    kind: Some(CompletionItemKind::REFERENCE),
                    detail: Some(desc.clone()),
                    documentation: Some(Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("**{}**\n\n{}", id, desc),
                    })),
                    ..Default::default()
                })
                .collect();

            if !items.is_empty() {
                return Ok(Some(CompletionResponse::Array(items)));
            }
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        if let Some((req_id, range)) = self.find_req_at_position(uri, position) {
            if let Some((desc, location)) = self.requirements.get(&req_id) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("### {}\n\n{}\n\n*Defined at: {}*", req_id, desc, location),
                    }),
                    range: Some(range),
                }));
            } else {
                // Unknown requirement
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("⚠️ **Unknown requirement:** `{}`", req_id),
                    }),
                    range: Some(range),
                }));
            }
        }

        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        self.client
            .log_message(
                MessageType::INFO,
                format!("Go to definition at {:?}", position),
            )
            .await;

        if let Some((req_id, _range)) = self.find_req_at_position(uri, position) {
            if let Some((_, location)) = self.requirements.get(&req_id) {
                // Parse location like "docs/spec/auth.md:42"
                let parts: Vec<&str> = location.split(':').collect();
                if parts.len() == 2 {
                    let file = parts[0];
                    let line: u32 = parts[1].parse().unwrap_or(1);

                    // Construct URI relative to workspace
                    // For prototype, just use file:// with current dir
                    let cwd = std::env::current_dir().unwrap_or_default();
                    let full_path = cwd.join(file);

                    if let Ok(target_uri) = Url::from_file_path(&full_path) {
                        return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                            uri: target_uri,
                            range: Range {
                                start: Position {
                                    line: line.saturating_sub(1),
                                    character: 0,
                                },
                                end: Position {
                                    line: line.saturating_sub(1),
                                    character: 0,
                                },
                            },
                        })));
                    }
                }
            }
        }

        Ok(None)
    }
}

#[tokio::main]
async fn main() {
    // Set up logging to stderr (LSP uses stdout for protocol)
    eprintln!("tracey-lsp-proto starting...");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
