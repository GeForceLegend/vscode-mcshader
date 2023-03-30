use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;

use logging::{error, info, warn};

use serde_json::Value;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::Parser;

mod data;
mod error;
mod service;

use crate::capability::ServerCapabilitiesFactroy;
use crate::commands::CommandList;
use crate::configuration::Configuration;
use crate::constant;
use crate::diagnostics_parser::DiagnosticsParser;
use crate::file::{IncludeFile, ShaderFile, TempFile};
use crate::notification;
use crate::opengl::OpenGlContext;

pub struct ServerData {
    extensions: RefCell<HashSet<String>>,
    roots: RefCell<HashSet<PathBuf>>,
    shader_packs: RefCell<HashSet<PathBuf>>,
    shader_files: RefCell<HashMap<PathBuf, ShaderFile>>,
    include_files: RefCell<HashMap<PathBuf, IncludeFile>>,
    temp_files: RefCell<HashMap<PathBuf, TempFile>>,
    tree_sitter_parser: RefCell<Parser>,
}

pub struct MinecraftLanguageServer {
    client: Client,
    command_list: CommandList,
    diagnostics_parser: DiagnosticsParser,
    server_data: Mutex<ServerData>,
    opengl_context: OpenGlContext,
    _log_guard: logging::GlobalLoggerGuard,
}

pub struct LanguageServerError;

impl MinecraftLanguageServer {
    pub fn new(
        client: Client, diagnostics_parser: DiagnosticsParser, opengl_context: OpenGlContext, parser: Parser,
    ) -> MinecraftLanguageServer {
        MinecraftLanguageServer {
            client,
            command_list: CommandList::new(),
            diagnostics_parser,
            server_data: Mutex::from(ServerData::new(parser)),
            opengl_context,
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
            .send_notification::<notification::StatusNotification>(notification::StatusNotificationParams {
                status: String::from("loading"),
                message,
                icon: String::from("$(loading~spin)"),
            })
            .await;
    }

    async fn set_status_ready(&self) {
        self.client
            .send_notification::<notification::StatusNotification>(notification::StatusNotificationParams {
                status: String::from("ready"),
                message: String::from("ready"),
                icon: String::from("$(check)"),
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

        let roots: HashSet<PathBuf>;
        if let Some(work_spaces) = params.workspace_folders {
            roots = work_spaces
                .iter()
                .map(|work_space| work_space.uri.to_file_path().unwrap())
                .collect();
        } else if let Some(uri) = params.root_uri {
            roots = HashSet::from([uri.to_file_path().unwrap()]);
        } else {
            roots = HashSet::new();
        }

        self.initial_scan(roots, constant::BASIC_EXTENSIONS.clone());

        initialize_result
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
        self.command_list.execute(&params.command, &params.arguments, &self.server_data)
    }

    #[logging_macro::with_trace_id]
    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        info!("Got updated configuration"; "config" => params.settings.as_object().unwrap().get("mcshader").unwrap().to_string());

        let mut config: Configuration = Configuration::new(&params.settings);

        let registrations: Vec<Registration> = config.generate_file_watch_registration();
        if let Err(err) = self.client.register_capability(registrations).await {
            warn!("Unable to registe file watch capability, error:{}", err);
        }

        match logging::Level::from_str(config.log_level.as_str()) {
            Ok(level) => logging::set_level(level),
            Err(_) => error!("Got unexpected log level from config"; "level" => &config.log_level),
        }

        config.extra_extension.extend(constant::BASIC_EXTENSIONS.clone());
        *self.server_data.lock().unwrap().extensions.borrow_mut() = config.extra_extension;
    }

    #[logging::with_trace_id]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let file_path = params.text_document.uri.to_file_path().unwrap();

        self.open_file(file_path);
    }

    #[logging::with_trace_id]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let file_path = params.text_document.uri.to_file_path().unwrap();

        self.change_file(&file_path, params.content_changes);
    }

    #[logging::with_trace_id]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let file_path = params.text_document.uri.to_file_path().unwrap();

        if let Some(diagnostics) = self.save_file(file_path, &self.diagnostics_parser, &self.opengl_context) {
            self.publish_diagnostic(diagnostics).await;
        }
    }

    #[logging::with_trace_id]
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let file_path = params.text_document.uri.to_file_path().unwrap();

        self.close_file(&file_path);
    }

    // Doesn't implemented yet
    // #[logging::with_trace_id]
    // async fn will_rename_files(&self, params: RenameFilesParams) -> Result<Option<WorkspaceEdit>> {
    //     let _ = params;
    //     error!("Got a workspace/willRenameFiles request, but it is not implemented");
    //     Err(Error::method_not_found())
    // }

    // Doesn't implemented yet, here for not reporting method not found
    #[logging::with_trace_id]
    async fn completion(&self, _params: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(None)
    }

    #[logging::with_trace_id]
    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let file_path = params.text_document.uri.to_file_path().unwrap();

        let result = self.document_links(&file_path, &self.diagnostics_parser, &self.opengl_context);
        self.publish_diagnostic(result.1).await;

        Ok(result.0)
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
        self.set_status_loading(String::from("Applying work space changes...")).await;

        self.update_work_spaces(params.event);

        self.set_status_ready().await;
    }

    #[logging_macro::with_trace_id]
    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        self.set_status_loading(String::from("Applying changes into file system...")).await;

        let diagnostics = self.update_watched_files(params.changes, &self.diagnostics_parser, &self.opengl_context);

        self.publish_diagnostic(diagnostics).await;
        self.set_status_ready().await;
    }
}
