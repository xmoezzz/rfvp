mod analysis;
mod config;
mod source;
mod syntax;
mod workspace;

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use analysis::{DiagnosticSeverityLite, SymbolDef, SymbolKindLite};
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tracing_subscriber::EnvFilter;
use workspace::{ChainResolution, DocumentData, WorkspaceState};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("luax_lsp=info".parse().unwrap()))
        .init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}

struct Backend {
    client: Client,
    state: Arc<Mutex<WorkspaceState>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(Mutex::new(WorkspaceState::new(None))),
        }
    }

    async fn publish_all_diagnostics(&self) {
        let payloads: Vec<(Url, Vec<Diagnostic>, i32)> = {
            let state = self.state.lock().await;
            state
                .docs
                .iter()
                .map(|(uri, doc)| (uri.clone(), diagnostics_for(doc), doc.version))
                .collect()
        };
        for (uri, diagnostics, version) in payloads {
            self.client.publish_diagnostics(uri, diagnostics, Some(version)).await;
        }
    }

    async fn with_doc<T>(&self, uri: &Url, f: impl FnOnce(&WorkspaceState, &DocumentData) -> T) -> Option<T> {
        let state = self.state.lock().await;
        let doc = state.document(uri)?;
        Some(f(&state, doc))
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let root = params
            .workspace_folders
            .as_ref()
            .and_then(|ws| ws.first())
            .and_then(|w| w.uri.to_file_path().ok())
            .or_else(|| params.root_uri.and_then(|u| u.to_file_path().ok()));
        let mut state = self.state.lock().await;
        *state = WorkspaceState::new(root);
        let _ = state.scan_workspace();
        drop(state);

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "luax-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string()]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                        legend: semantic_legend(),
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                        range: None,
                        ..Default::default()
                    }),
                ),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: Some(vec![",".to_string()]),
                    work_done_progress_options: Default::default(),
                }),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "luax-lsp initialized")
            .await;
        self.publish_all_diagnostics().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        let mut state = self.state.lock().await;
        state.upsert_open_document(doc.uri, doc.text, doc.version);
        drop(state);
        self.publish_all_diagnostics().await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            let mut state = self.state.lock().await;
            state.upsert_open_document(params.text_document.uri, change.text, params.text_document.version);
            drop(state);
            self.publish_all_diagnostics().await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut state = self.state.lock().await;
        state.remove_document(&params.text_document.uri);
        drop(state);
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let pos = params.text_document_position_params.position;
        Ok(self
            .with_doc(&params.text_document_position_params.text_document.uri, |state, doc| {
                let offset = doc.source.position_to_offset(pos);
                hover_for(state, doc, offset)
            })
            .await
            .flatten())
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        Ok(self
            .with_doc(&uri, |state, doc| {
                let offset = doc.source.position_to_offset(position);
                let items = completions_for(state, doc, offset);
                Some(CompletionResponse::Array(items))
            })
            .await
            .flatten())
    }

    async fn goto_definition(&self, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        Ok(self
            .with_doc(&uri, |state, doc| {
                let offset = doc.source.position_to_offset(position);
                goto_definition_for(state, doc, offset)
            })
            .await
            .flatten())
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        Ok(self
            .with_doc(&uri, |_state, doc| {
                let offset = doc.source.position_to_offset(position);
                references_for(doc, offset)
            })
            .await
            .flatten())
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;
        Ok(self
            .with_doc(&uri, |_state, doc| {
                let offset = doc.source.position_to_offset(position);
                rename_for(doc, offset, &new_name)
            })
            .await
            .flatten())
    }

    async fn document_symbol(&self, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>> {
        Ok(self
            .with_doc(&params.text_document.uri, |_state, doc| {
                Some(DocumentSymbolResponse::Nested(document_symbols_for(doc)))
            })
            .await
            .flatten())
    }

    async fn symbol(&self, params: WorkspaceSymbolParams) -> Result<Option<Vec<SymbolInformation>>> {
        let state = self.state.lock().await;
        Ok(Some(workspace_symbols_for(&state, &params.query)))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        Ok(self
            .with_doc(&params.text_document.uri, |_state, doc| {
                Some(SemanticTokensResult::Tokens(semantic_tokens_for(doc)))
            })
            .await
            .flatten())
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        Ok(self
            .with_doc(&uri, |state, doc| {
                let offset = doc.source.position_to_offset(position);
                signature_help_for(state, doc, offset)
            })
            .await
            .flatten())
    }
}

fn diagnostics_for(doc: &DocumentData) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for message in &doc.config_diagnostics {
        out.push(Diagnostic {
            range: doc.source.range(0, 0),
            severity: Some(DiagnosticSeverity::ERROR),
            message: message.clone(),
            source: Some("luax-lsp".to_string()),
            ..Default::default()
        });
    }
    for err in &doc.analysis.parse_errors {
        out.push(Diagnostic {
            range: doc.source.range(err.span.start, err.span.end),
            severity: Some(DiagnosticSeverity::ERROR),
            message: err.message.clone(),
            source: Some("luax-lsp".to_string()),
            ..Default::default()
        });
    }
    for lint in &doc.analysis.diagnostics {
        out.push(Diagnostic {
            range: doc.source.range(lint.span.start, lint.span.end),
            severity: Some(match lint.severity {
                DiagnosticSeverityLite::Error => DiagnosticSeverity::ERROR,
                DiagnosticSeverityLite::Warning => DiagnosticSeverity::WARNING,
                DiagnosticSeverityLite::Hint => DiagnosticSeverity::HINT,
            }),
            message: lint.message.clone(),
            source: Some("luax-lsp".to_string()),
            ..Default::default()
        });
    }
    out
}

fn hover_for(state: &WorkspaceState, doc: &DocumentData, offset: usize) -> Option<Hover> {
    if let Some(def) = doc.analysis.symbol_def_at(offset) {
        return Some(symbol_hover(doc, def));
    }
    if let Some(reference) = doc.analysis.symbol_ref_at(offset) {
        if let Some(def_id) = reference.def_id {
            if let Some(def) = doc.analysis.defs.iter().find(|d| d.id == def_id) {
                return Some(symbol_hover(doc, def));
            }
        }
    }
    if let Some(member) = doc.analysis.member_at(offset) {
        if let Some(def_id) = member.def_id {
            if let Some(def) = doc.analysis.defs.iter().find(|d| d.id == def_id) {
                return Some(symbol_hover(doc, def));
            }
        }
        if let Some(module_name) = &member.owner_module {
            if let Some(module_doc) = state.module_doc(module_name) {
                if let Some(def) = module_doc
                    .analysis
                    .defs
                    .iter()
                    .find(|d| d.parent.is_none() && d.name == member.name)
                {
                    return Some(symbol_hover(module_doc, def));
                }
            }
        }
    }
    None
}

fn symbol_hover(doc: &DocumentData, def: &SymbolDef) -> Hover {
    let mut text = format!("```luax\n{} {}\n```", symbol_kind_label(def.kind), def.name);
    if !def.detail.is_empty() {
        text.push_str(&format!("\n{}", def.detail));
    }
    if !def.documentation.is_empty() {
        text.push_str("\n\n");
        text.push_str(&def.documentation);
    }
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: text,
        }),
        range: Some(doc.source.range(def.span.start, def.span.end)),
    }
}

fn completions_for(state: &WorkspaceState, doc: &DocumentData, offset: usize) -> Vec<CompletionItem> {
    let ctx = completion_context(&doc.text, offset);
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();

    if let Some(owner_chain) = ctx.owner_chain.as_deref() {
        match state.resolve_chain(doc, offset, owner_chain) {
            ChainResolution::Symbol(def) => {
                for child_id in &def.children {
                    if let Some(child) = doc.analysis.defs.iter().find(|d| d.id == *child_id) {
                        push_completion(&mut items, &mut seen, completion_from_symbol(child));
                    }
                }
            }
            ChainResolution::Module(module_name) => {
                if let Some(module_doc) = state.module_doc(&module_name) {
                    for def in module_doc.analysis.defs.iter().filter(|d| d.parent.is_none()) {
                        push_completion(&mut items, &mut seen, completion_from_symbol(def));
                    }
                }
            }
            ChainResolution::CrossDocSymbol((module_doc, def)) => {
                for child_id in &def.children {
                    if let Some(child) = module_doc.analysis.defs.iter().find(|d| d.id == *child_id) {
                        push_completion(&mut items, &mut seen, completion_from_symbol(child));
                    }
                }
            }
            ChainResolution::None => {}
        }
        return filter_completion_prefix(items, &ctx.prefix);
    }

    for kw in KEYWORDS {
        push_completion(
            &mut items,
            &mut seen,
            CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                insert_text: Some(kw.to_string()),
                ..Default::default()
            },
        );
    }

    for snippet in snippet_items() {
        push_completion(&mut items, &mut seen, snippet);
    }

    for builtin in BUILTINS {
        push_completion(
            &mut items,
            &mut seen,
            CompletionItem {
                label: builtin.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                insert_text: Some(builtin.to_string()),
                ..Default::default()
            },
        );
    }

    for def in doc.analysis.visible_defs(offset) {
        push_completion(&mut items, &mut seen, completion_from_symbol(def));
    }

    for alias in &doc.analysis.aliases {
        if alias.visible.start <= offset && offset <= alias.visible.end {
            push_completion(
                &mut items,
                &mut seen,
                CompletionItem {
                    label: alias.name.clone(),
                    detail: Some(format!("module alias -> {}", alias.module_name)),
                    kind: Some(CompletionItemKind::MODULE),
                    ..Default::default()
                },
            );
        }
    }

    filter_completion_prefix(items, &ctx.prefix)
}

fn goto_definition_for(state: &WorkspaceState, doc: &DocumentData, offset: usize) -> Option<GotoDefinitionResponse> {
    if let Some(def) = doc.analysis.symbol_def_at(offset) {
        return Some(GotoDefinitionResponse::Scalar(Location::new(
            doc.uri.clone(),
            doc.source.range(def.span.start, def.span.end),
        )));
    }
    if let Some(reference) = doc.analysis.symbol_ref_at(offset) {
        if let Some(def_id) = reference.def_id {
            if let Some(def) = doc.analysis.defs.iter().find(|d| d.id == def_id) {
                return Some(GotoDefinitionResponse::Scalar(Location::new(
                    doc.uri.clone(),
                    doc.source.range(def.span.start, def.span.end),
                )));
            }
        }
    }
    if let Some(member) = doc.analysis.member_at(offset) {
        if let Some(def_id) = member.def_id {
            if let Some(def) = doc.analysis.defs.iter().find(|d| d.id == def_id) {
                return Some(GotoDefinitionResponse::Scalar(Location::new(
                    doc.uri.clone(),
                    doc.source.range(def.span.start, def.span.end),
                )));
            }
        }
        if let Some(module_name) = &member.owner_module {
            if let Some(module_doc) = state.module_doc(module_name) {
                if let Some(def) = module_doc.analysis.defs.iter().find(|d| d.parent.is_none() && d.name == member.name) {
                    return Some(GotoDefinitionResponse::Scalar(Location::new(
                        module_doc.uri.clone(),
                        module_doc.source.range(def.span.start, def.span.end),
                    )));
                }
            }
        }
    }
    None
}

fn references_for(doc: &DocumentData, offset: usize) -> Option<Vec<Location>> {
    let def_id = if let Some(def) = doc.analysis.symbol_def_at(offset) {
        Some(def.id)
    } else if let Some(reference) = doc.analysis.symbol_ref_at(offset) {
        reference.def_id
    } else if let Some(member) = doc.analysis.member_at(offset) {
        member.def_id
    } else {
        None
    }?;
    let mut out = Vec::new();
    if let Some(def) = doc.analysis.defs.iter().find(|d| d.id == def_id) {
        out.push(Location::new(doc.uri.clone(), doc.source.range(def.span.start, def.span.end)));
    }
    for reference in doc.analysis.refs.iter().filter(|r| r.def_id == Some(def_id)) {
        out.push(Location::new(doc.uri.clone(), doc.source.range(reference.span.start, reference.span.end)));
    }
    for member in doc.analysis.member_accesses.iter().filter(|r| r.def_id == Some(def_id)) {
        out.push(Location::new(doc.uri.clone(), doc.source.range(member.span.start, member.span.end)));
    }
    Some(out)
}

fn rename_for(doc: &DocumentData, offset: usize, new_name: &str) -> Option<WorkspaceEdit> {
    let def_id = if let Some(def) = doc.analysis.symbol_def_at(offset) {
        Some(def.id)
    } else if let Some(reference) = doc.analysis.symbol_ref_at(offset) {
        reference.def_id
    } else if let Some(member) = doc.analysis.member_at(offset) {
        member.def_id
    } else {
        None
    }?;
    let mut edits = Vec::new();
    if let Some(def) = doc.analysis.defs.iter().find(|d| d.id == def_id) {
        edits.push(TextEdit {
            range: doc.source.range(def.span.start, def.span.end),
            new_text: new_name.to_string(),
        });
    }
    for reference in doc.analysis.refs.iter().filter(|r| r.def_id == Some(def_id)) {
        edits.push(TextEdit {
            range: doc.source.range(reference.span.start, reference.span.end),
            new_text: new_name.to_string(),
        });
    }
    for member in doc.analysis.member_accesses.iter().filter(|r| r.def_id == Some(def_id)) {
        edits.push(TextEdit {
            range: doc.source.range(member.span.start, member.span.end),
            new_text: new_name.to_string(),
        });
    }
    let mut changes = HashMap::new();
    changes.insert(doc.uri.clone(), edits);
    Some(WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    })
}

fn document_symbols_for(doc: &DocumentData) -> Vec<DocumentSymbol> {
    fn build(doc: &DocumentData, def: &SymbolDef) -> DocumentSymbol {
        let mut children = Vec::new();
        for child_id in &def.children {
            if let Some(child) = doc.analysis.defs.iter().find(|d| d.id == *child_id) {
                children.push(build(doc, child));
            }
        }
        DocumentSymbol {
            name: def.name.clone(),
            detail: if def.detail.is_empty() { None } else { Some(def.detail.clone()) },
            kind: symbol_kind(def.kind),
            tags: None,
            deprecated: None,
            range: doc.source.range(def.visible.start, def.visible.end),
            selection_range: doc.source.range(def.span.start, def.span.end),
            children: if children.is_empty() { None } else { Some(children) },
        }
    }

    doc.analysis
        .defs
        .iter()
        .filter(|d| d.parent.is_none() && !matches!(d.kind, SymbolKindLite::Module | SymbolKindLite::ConfigGlobal | SymbolKindLite::Syscall))
        .map(|d| build(doc, d))
        .collect()
}

fn workspace_symbols_for(state: &WorkspaceState, query: &str) -> Vec<SymbolInformation> {
    let mut out = Vec::new();
    for doc in state.docs.values() {
        for def in &doc.analysis.defs {
            if !query.is_empty() && !def.name.contains(query) {
                continue;
            }
            out.push(SymbolInformation {
                name: def.name.clone(),
                kind: symbol_kind(def.kind),
                tags: None,
                deprecated: None,
                location: Location::new(doc.uri.clone(), doc.source.range(def.span.start, def.span.end)),
                container_name: None,
            });
        }
    }
    out
}

fn semantic_tokens_for(doc: &DocumentData) -> SemanticTokens {
    let mut data = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;
    for token in &doc.analysis.tokens {
        let token_type = match token.kind {
            syntax::TokenKind::Comment => Some(7),
            syntax::TokenKind::String => Some(2),
            syntax::TokenKind::Number => Some(3),
            syntax::TokenKind::Function
            | syntax::TokenKind::If
            | syntax::TokenKind::Then
            | syntax::TokenKind::Else
            | syntax::TokenKind::ElseIf
            | syntax::TokenKind::End
            | syntax::TokenKind::While
            | syntax::TokenKind::Do
            | syntax::TokenKind::For
            | syntax::TokenKind::Repeat
            | syntax::TokenKind::Until
            | syntax::TokenKind::Return
            | syntax::TokenKind::Global
            | syntax::TokenKind::Local
            | syntax::TokenKind::Volatile
            | syntax::TokenKind::Break
            | syntax::TokenKind::And
            | syntax::TokenKind::Or
            | syntax::TokenKind::Not => Some(0),
            syntax::TokenKind::Ident => classify_identifier(doc, token.span.start),
            _ => None,
        };
        let Some(token_type) = token_type else {
            continue;
        };
        let start = doc.source.offset_to_position(token.span.start);
        let end = doc.source.offset_to_position(token.span.end);
        let delta_line = start.line - prev_line;
        let delta_start = if delta_line == 0 { start.character - prev_start } else { start.character };
        let length = end.character.saturating_sub(start.character).max(1);
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: 0,
        });
        prev_line = start.line;
        prev_start = start.character;
    }
    SemanticTokens {
        result_id: None,
        data,
    }
}

fn signature_help_for(state: &WorkspaceState, doc: &DocumentData, offset: usize) -> Option<SignatureHelp> {
    let call = call_context(&doc.text, offset)?;
    let def = state.resolve_visible_symbol(doc, offset, &call.callee).or_else(|| {
        if let Some(module_name) = call.owner_chain.as_deref() {
            if let Some(module_doc) = state.module_doc(module_name) {
                return module_doc.analysis.defs.iter().find(|d| d.name == call.callee);
            }
        }
        None
    })?;
    let params: Vec<String> = doc
        .analysis
        .defs
        .iter()
        .filter(|d| d.parent == Some(def.id) && d.kind == SymbolKindLite::Parameter)
        .map(|d| d.name.clone())
        .collect();
    let label = if params.is_empty() {
        format!("{}()", def.name)
    } else {
        format!("{}({})", def.name, params.join(", "))
    };
    let parameters = params
        .iter()
        .map(|p| ParameterInformation {
            label: ParameterLabel::Simple(p.clone()),
            documentation: None,
        })
        .collect();
    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: if def.documentation.is_empty() {
                None
            } else {
                Some(Documentation::String(def.documentation.clone()))
            },
            parameters: Some(parameters),
            active_parameter: Some(call.arg_index as u32),
        }],
        active_signature: Some(0),
        active_parameter: Some(call.arg_index as u32),
    })
}

fn semantic_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::KEYWORD,
            SemanticTokenType::FUNCTION,
            SemanticTokenType::STRING,
            SemanticTokenType::NUMBER,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::PARAMETER,
            SemanticTokenType::PROPERTY,
            SemanticTokenType::COMMENT,
            SemanticTokenType::TYPE,
        ],
        token_modifiers: vec![],
    }
}

fn classify_identifier(doc: &DocumentData, offset: usize) -> Option<u32> {
    if let Some(def) = doc.analysis.symbol_def_at(offset) {
        return Some(match def.kind {
            SymbolKindLite::Function | SymbolKindLite::Syscall => 1,
            SymbolKindLite::Parameter => 5,
            SymbolKindLite::Field => 6,
            SymbolKindLite::Module => 8,
            _ => 4,
        });
    }
    if let Some(reference) = doc.analysis.symbol_ref_at(offset) {
        if let Some(def_id) = reference.def_id {
            if let Some(def) = doc.analysis.defs.iter().find(|d| d.id == def_id) {
                return Some(match def.kind {
                    SymbolKindLite::Function | SymbolKindLite::Syscall => 1,
                    SymbolKindLite::Parameter => 5,
                    SymbolKindLite::Field => 6,
                    SymbolKindLite::Module => 8,
                    _ => 4,
                });
            }
        }
        return Some(4);
    }
    if doc.analysis.member_at(offset).is_some() {
        return Some(6);
    }
    Some(4)
}

fn symbol_kind(kind: SymbolKindLite) -> SymbolKind {
    match kind {
        SymbolKindLite::Local | SymbolKindLite::Global | SymbolKindLite::ConfigGlobal => SymbolKind::VARIABLE,
        SymbolKindLite::Function | SymbolKindLite::Syscall => SymbolKind::FUNCTION,
        SymbolKindLite::Parameter => SymbolKind::VARIABLE,
        SymbolKindLite::Field => SymbolKind::FIELD,
        SymbolKindLite::Module => SymbolKind::MODULE,
    }
}

fn symbol_kind_label(kind: SymbolKindLite) -> &'static str {
    match kind {
        SymbolKindLite::Local => "local",
        SymbolKindLite::Global => "global",
        SymbolKindLite::Function => "function",
        SymbolKindLite::Parameter => "parameter",
        SymbolKindLite::Field => "field",
        SymbolKindLite::Module => "module",
        SymbolKindLite::ConfigGlobal => "config-global",
        SymbolKindLite::Syscall => "syscall",
    }
}

fn completion_from_symbol(def: &SymbolDef) -> CompletionItem {
    CompletionItem {
        label: def.name.clone(),
        kind: Some(match def.kind {
            SymbolKindLite::Function | SymbolKindLite::Syscall => CompletionItemKind::FUNCTION,
            SymbolKindLite::Field => CompletionItemKind::FIELD,
            SymbolKindLite::Module => CompletionItemKind::MODULE,
            _ => CompletionItemKind::VARIABLE,
        }),
        detail: if def.detail.is_empty() { None } else { Some(def.detail.clone()) },
        documentation: if def.documentation.is_empty() {
            None
        } else {
            Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: def.documentation.clone(),
            }))
        },
        ..Default::default()
    }
}

fn push_completion(items: &mut Vec<CompletionItem>, seen: &mut BTreeSet<String>, item: CompletionItem) {
    if seen.insert(item.label.clone()) {
        items.push(item);
    }
}

fn filter_completion_prefix(items: Vec<CompletionItem>, prefix: &str) -> Vec<CompletionItem> {
    if prefix.is_empty() {
        return items;
    }
    items
        .into_iter()
        .filter(|item| item.label.starts_with(prefix))
        .collect()
}

struct CompletionContext {
    owner_chain: Option<String>,
    prefix: String,
}

fn completion_context(text: &str, offset: usize) -> CompletionContext {
    let safe = offset.min(text.len());
    let bytes = text.as_bytes();
    let mut i = safe;
    while i > 0 && is_ident_byte(bytes[i - 1]) {
        i -= 1;
    }
    let prefix = text[i..safe].to_string();
    if i > 0 && bytes[i - 1] == b'.' {
        let mut j = i - 1;
        while j > 0 && (is_ident_byte(bytes[j - 1]) || bytes[j - 1] == b'.') {
            j -= 1;
        }
        let owner_chain = text[j..i - 1].trim().to_string();
        return CompletionContext {
            owner_chain: if owner_chain.is_empty() { None } else { Some(owner_chain) },
            prefix,
        };
    }
    CompletionContext {
        owner_chain: None,
        prefix,
    }
}

struct CallContext {
    owner_chain: Option<String>,
    callee: String,
    arg_index: usize,
}

fn call_context(text: &str, offset: usize) -> Option<CallContext> {
    let safe = offset.min(text.len());
    let before = &text[..safe];
    let open = before.rfind('(')?;
    let call_head = before[..open].trim_end();
    let arg_index = before[open + 1..].chars().filter(|&c| c == ',').count();
    let mut end = call_head.len();
    while end > 0 && is_ident_byte(call_head.as_bytes()[end - 1]) {
        end -= 1;
    }
    let tail = &call_head[end..];
    let prefix = &call_head[..end];
    if tail.is_empty() {
        return None;
    }
    if prefix.ends_with('.') {
        Some(CallContext {
            owner_chain: Some(prefix[..prefix.len() - 1].to_string()),
            callee: tail.to_string(),
            arg_index,
        })
    } else {
        Some(CallContext {
            owner_chain: None,
            callee: tail.to_string(),
            arg_index,
        })
    }
}

fn snippet_items() -> Vec<CompletionItem> {
    vec![
        snippet("function", "function ${1:name}(${2})\n  ${0}\nend"),
        snippet("local", "local ${1:name} = ${0}"),
        snippet("global", "global ${1:name} = ${0}"),
        snippet("volatile global", "volatile global ${1:name} = ${0}"),
        snippet("if", "if ${1:cond} then\n  ${0}\nend"),
        snippet("while", "while ${1:cond} do\n  ${0}\nend"),
        snippet("for", "for ${1:i} = ${2:1}, ${3:n} do\n  ${0}\nend"),
    ]
}

fn snippet(label: &str, body: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::SNIPPET),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        insert_text: Some(body.to_string()),
        ..Default::default()
    }
}

fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "global", "if",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "volatile", "while",
];

const BUILTINS: &[&str] = &["assert", "ipairs", "pairs", "print", "require", "tonumber", "tostring", "type"];
