use std::cell::RefCell;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;

use logging::{error, info, warn};

use hashbrown::{HashMap, HashSet};
use serde_json::Value;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::Parser;

mod data;
mod error;
mod service;

use crate::capability::ServerCapabilitiesFactroy;
use crate::configuration::Configuration;
use crate::constant::*;
use crate::file::*;
use crate::notification;

/// Everything mutable in this struct.
///
/// By sending the Mutex of server data to snyc functions, we can handle it like single thread
pub struct ServerData {
    extensions: RefCell<HashSet<String>>,
    shader_packs: RefCell<HashSet<PathBuf>>,
    workspace_files: RefCell<HashMap<PathBuf, WorkspaceFile>>,
    temp_files: RefCell<HashMap<PathBuf, TempFile>>,
    tree_sitter_parser: RefCell<Parser>,
}

/// Other things that do not need to be mutable
pub struct MinecraftLanguageServer {
    client: Client,
    server_data: Mutex<ServerData>,
    _log_guard: logging::GlobalLoggerGuard,
}

pub struct LanguageServerError;

impl MinecraftLanguageServer {
    pub fn new(client: Client, parser: Parser) -> MinecraftLanguageServer {
        MinecraftLanguageServer {
            client,
            server_data: Mutex::new(ServerData::new(parser)),
            _log_guard: logging::init_logger(),
        }
    }

    async fn publish_diagnostic(&self, diagnostics: HashMap<Url, Vec<Diagnostic>>) {
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

        let roots: HashSet<PathBuf> = if let Some(work_spaces) = params.workspace_folders {
            work_spaces
                .iter()
                .map(|work_space| work_space.uri.to_file_path().unwrap())
                .collect::<HashSet<_>>()
        } else if let Some(uri) = params.root_uri {
            HashSet::from([uri.to_file_path().unwrap()])
        } else {
            HashSet::new()
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
        match COMMAND_LIST.get(&params.command) {
            Some(command) => command.run(&params.arguments, &server_data),
            None => Err(LanguageServerError::invalid_command_error()),
        }
    }

    #[logging_macro::with_trace_id]
    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        info!("Got updated configuration"; "config" => params.settings.as_object().unwrap().get("mcshader").unwrap().to_string());

        let mut config: Configuration = Configuration::new(&params.settings);

        let registrations: Vec<Registration> = config.generate_file_watch_registration();
        if let Err(err) = self.client.register_capability(registrations).await {
            warn!("Unable to registe file watch capability, error:{}", err);
        }

        match logging::Level::from_str(&config.log_level) {
            Ok(level) => logging::set_level(level),
            Err(_) => error!("Got unexpected log level from config"; "level" => &config.log_level),
        }

        config.extra_extension.extend(BASIC_EXTENSIONS.clone());
        *self.server_data.lock().unwrap().extensions.borrow_mut() = config.extra_extension;
    }

    #[logging::with_trace_id]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let file_path = params.text_document.uri.to_file_path().unwrap();

        if let Some(diagnostics) = self.open_file(file_path) {
            self.publish_diagnostic(diagnostics).await;
        }
    }

    #[logging::with_trace_id]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(diagnostics) = self.change_file(params.text_document.uri, params.content_changes) {
            self.publish_diagnostic(diagnostics).await;
        }
    }

    #[logging::with_trace_id]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let file_path = params.text_document.uri.to_file_path().unwrap();

        if let Some(diagnostics) = self.save_file(file_path) {
            self.publish_diagnostic(diagnostics).await;
        }
    }

    #[logging::with_trace_id]
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        if let Some(diagnostics) = self.close_file(&params.text_document.uri) {
            self.publish_diagnostic(diagnostics).await;
        }
    }

    // Doesn't implemented yet
    // #[logging::with_trace_id]
    // async fn will_rename_files(&self, params: RenameFilesParams) -> Result<Option<WorkspaceEdit>> {
    //     let _ = params;
    //     error!("Got a workspace/willRenameFiles request, but it is not implemented");
    //     Err(Error::method_not_found())
    // }

    #[logging::with_trace_id]
    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let file_path = params.text_document.uri.to_file_path().unwrap();

        let result = self.document_links(&file_path);

        Ok(result)
    }

    #[logging::with_trace_id]
    async fn goto_definition(&self, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
        match self.find_definitions(params).unwrap() {
            Some(locatons) => Ok(Some(GotoDefinitionResponse::Array(locatons))),
            None => Ok(None),
        }
    }

    #[logging::with_trace_id]
    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        match self.find_references(params).unwrap() {
            Some(locatons) => Ok(Some(locatons)),
            None => Ok(None),
        }
    }

    #[logging::with_trace_id]
    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        self.set_status_loading("Applying work space changes...".to_owned()).await;

        let diagnostics = self.update_work_spaces(params.event);
        self.publish_diagnostic(diagnostics).await;

        self.set_status_ready().await;
    }

    #[logging_macro::with_trace_id]
    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        self.set_status_loading("Applying changes into file system...".to_owned()).await;

        let diagnostics = self.update_watched_files(params.changes);

        self.publish_diagnostic(diagnostics).await;
        self.set_status_ready().await;
    }
}
