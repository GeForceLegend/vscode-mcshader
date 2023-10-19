use std::cell::RefCell;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use std::str::FromStr;
use std::sync::Mutex;

use logging::{error, info, warn};

use hashbrown::{HashMap, HashSet};
use serde_json::Value;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{request::*, *};
use tower_lsp::{Client, LanguageServer};
use tree_sitter::Parser;

mod change_file;
mod close_file;
mod document_links;
mod error;
mod find_definitions;
mod find_references;
mod list_symbols;
mod open_file;
mod rename_files;
mod save_file;
mod update_watched_files;
mod update_workspaces;
mod utility;

use crate::capability::ServerCapabilitiesFactroy;
use crate::configuration::Configuration;
use crate::constant::*;
use crate::file::*;
use crate::notification;
use crate::tree_parser::TreeParser;

pub type Diagnostics = HashMap<Url, Vec<Diagnostic>>;

/// Everything mutable in this struct.
/// By sending the Mutex of server data to snyc functions, we can handle it like single thread
pub struct ServerData {
    temp_lint: RefCell<bool>,
    extensions: RefCell<HashSet<String>>,
    shader_packs: RefCell<HashSet<PathBuf>>,
    workspace_files: RefCell<HashMap<PathBuf, WorkspaceFile>>,
    temp_files: RefCell<HashMap<PathBuf, TempFile>>,
    tree_sitter_parser: RefCell<Parser>,
}

impl ServerData {
    pub fn new() -> Self {
        let mut tree_sitter_parser = Parser::new();
        tree_sitter_parser.set_language(tree_sitter_glsl::language()).unwrap();
        ServerData {
            temp_lint: RefCell::new(false),
            extensions: RefCell::new(BASIC_EXTENSIONS.clone()),
            shader_packs: RefCell::new(HashSet::new()),
            workspace_files: RefCell::new(HashMap::new()),
            temp_files: RefCell::new(HashMap::new()),
            tree_sitter_parser: RefCell::new(tree_sitter_parser),
        }
    }

    pub fn workspace_files(&self) -> &RefCell<HashMap<PathBuf, WorkspaceFile>> {
        &self.workspace_files
    }

    pub fn temp_files(&self) -> &RefCell<HashMap<PathBuf, TempFile>> {
        &self.temp_files
    }
}

/// Other things that do not need to be mutable
pub struct MinecraftLanguageServer {
    client: Client,
    server_data: Mutex<ServerData>,
    _log_guard: logging::GlobalLoggerGuard,
}

pub struct LanguageServerError;

impl MinecraftLanguageServer {
    pub fn new(client: Client) -> MinecraftLanguageServer {
        MinecraftLanguageServer {
            client,
            server_data: Mutex::new(ServerData::new()),
            _log_guard: logging::init_logger(),
        }
    }

    async fn publish_diagnostic(&self, diagnostics: Diagnostics) {
        for (uri, diagnostics) in diagnostics {
            self.client.publish_diagnostics(uri, diagnostics, None).await;
        }
    }

    async fn set_status_loading(&self, message: String) {
        self.client
            .send_notification::<notification::StatusUpdate>(notification::StatusUpdateParams {
                status: "loading".to_owned(),
                message,
                icon: "$(loading~spin)".to_owned(),
            })
            .await;
    }

    async fn set_status_ready(&self) {
        self.client
            .send_notification::<notification::StatusUpdate>(notification::StatusUpdateParams {
                status: "ready".to_owned(),
                message: "ready".to_owned(),
                icon: "$(check)".to_owned(),
            })
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for MinecraftLanguageServer {
    #[logging::with_trace_id]
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        info!("Starting server...");

        let initialize_result = ServerCapabilitiesFactroy::initial_capabilities();

        let roots: Vec<PathBuf> = if let Some(workspaces) = params.workspace_folders {
            workspaces
                .iter()
                .map(|workspace| workspace.uri.to_file_path().unwrap())
                .collect::<Vec<_>>()
        } else if let Some(uri) = params.root_uri {
            vec![uri.to_file_path().unwrap(); 1]
        } else {
            vec![]
        };

        self.initial_scan(roots);

        Ok(initialize_result)
    }

    #[logging::with_trace_id]
    async fn initialized(&self, _params: InitializedParams) {
        self.set_status_ready().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    #[logging::with_trace_id]
    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<Value>> {
        let server_data = self.server_data.lock().unwrap();
        match COMMAND_LIST.get(params.command.as_str()) {
            Some(command) => command.run(&params.arguments, &server_data),
            None => Err(LanguageServerError::invalid_command_error()),
        }
    }

    #[logging::with_trace_id]
    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        info!("Got updated configuration"; "config" => params.settings.as_object().unwrap().get("mcshader").unwrap().to_string());

        let mut config = Configuration::new(&params.settings);

        let registrations = config.generate_file_watch_registration();
        if let Err(err) = self.client.register_capability(registrations).await {
            warn!("Unable to registe file watch capability, error:{}", err);
        }

        match logging::Level::from_str(&config.log_level) {
            Ok(level) => logging::set_level(level),
            Err(_) => error!("Got unexpected log level from config"; "level" => &config.log_level),
        }

        config.extra_extension.extend(BASIC_EXTENSIONS.clone());

        let server_data = self.server_data.lock().unwrap();
        *server_data.extensions.borrow_mut() = config.extra_extension;
        *server_data.temp_lint.borrow_mut() = config.temp_lint;
    }

    #[logging::with_trace_id]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        if let Some(diagnostics) = self.open_file(params) {
            self.publish_diagnostic(diagnostics).await;
        }
    }

    #[logging::with_trace_id]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(diagnostics) = self.change_file(params.text_document.uri, &params.content_changes) {
            self.publish_diagnostic(diagnostics).await;
        }
    }

    #[logging::with_trace_id]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        if let Some(diagnostics) = self.save_file(params.text_document.uri) {
            self.publish_diagnostic(diagnostics).await;
        }
    }

    #[logging::with_trace_id]
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        if let Some(diagnostics) = self.close_file(params.text_document.uri) {
            self.publish_diagnostic(diagnostics).await;
        }
    }

    #[logging::with_trace_id]
    async fn will_rename_files(&self, params: RenameFilesParams) -> Result<Option<WorkspaceEdit>> {
        Ok(self.rename_files(params))
    }

    #[logging::with_trace_id]
    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let result = match self.document_links(params.text_document.uri) {
            Some((document_links, diagnostics)) => {
                self.publish_diagnostic(diagnostics).await;
                Some(document_links)
            }
            None => None,
        };

        Ok(result)
    }

    #[logging::with_trace_id]
    async fn goto_definition(&self, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
        Ok(self.find_definitions(params).map(GotoDefinitionResponse::Array))
    }

    #[logging::with_trace_id]
    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        Ok(self.find_references(params))
    }

    #[logging::with_trace_id]
    async fn document_symbol(&self, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>> {
        Ok(self.list_symbols(params))
    }

    #[logging::with_trace_id]
    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        self.set_status_loading("Applying work space changes...".to_owned()).await;

        let diagnostics = self.update_workspaces(params.event);
        self.publish_diagnostic(diagnostics).await;

        self.set_status_ready().await;
    }

    #[logging::with_trace_id]
    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        self.set_status_loading("Applying changes into file system...".to_owned()).await;

        let diagnostics = self.update_watched_files(&params.changes);

        self.publish_diagnostic(diagnostics).await;
        self.set_status_ready().await;
    }
}
