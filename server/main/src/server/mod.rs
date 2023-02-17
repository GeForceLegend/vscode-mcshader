use std::collections::{HashSet, HashMap};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;

use logging::{error, info, warn};

use tower_lsp::jsonrpc::{Result, Error};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

mod data_manager;
mod server_data;

use crate::capability::ServerCapabilitiesFactroy;
use crate::configuration::Configuration;
use crate::constant;
use crate::diagnostics_parser::DiagnosticsParser;
use crate::notification;
use crate::opengl::OpenGlContext;

use self::data_manager::DataManager;
use self::server_data::ServerData;

pub struct MinecraftLanguageServer {
    pub client: Client,
    diagnostics_parser: DiagnosticsParser,
    extensions: Mutex<HashSet<String>>,
    server_data: ServerData,
    opengl_context: OpenGlContext,
    _log_guard: logging::GlobalLoggerGuard,
}

impl MinecraftLanguageServer {
    pub fn new(client: Client, diagnostics_parser: DiagnosticsParser, opengl_context: OpenGlContext) -> MinecraftLanguageServer {
        MinecraftLanguageServer {
            client,
            diagnostics_parser,
            extensions: Mutex::from(HashSet::new()),
            server_data: ServerData::new(),
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
            .send_notification::<notification::StatusNotification>(
                notification::StatusNotificationParams{
                    status: "loading".to_string(),
                    message,
                    icon: "$(loading~spin)".to_string(),
                }
            )
            .await;
    }

    async fn set_status_ready(&self) {
        self.client
            .send_notification::<notification::StatusNotification>(
                notification::StatusNotificationParams{
                    status: "ready".to_string(),
                    message: "ready".to_string(),
                    icon: "$(check)".to_string(),
                }
            )
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for MinecraftLanguageServer {
    #[logging::with_trace_id]
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        info!("starting server...");

        let initialize_result = ServerCapabilitiesFactroy::initial_capabilities();

        let mut roots: HashSet<PathBuf> = HashSet::new();
        if let Some(work_spaces) = params.workspace_folders {
            work_spaces.iter().for_each(|work_space| {
                roots.insert(work_space.uri.to_file_path().unwrap());
            });
        }
        else if let Some(uri) = params.root_uri {
            roots.insert(uri.to_file_path().unwrap());
        }

        self.server_data.initial_scan(roots);
        self.extensions.lock().unwrap().clone_from(&constant::BASIC_EXTENSIONS);

        initialize_result
    }

    #[logging::with_trace_id]
    async fn initialized(&self, _params: InitializedParams) {
        self.set_status_ready().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    #[logging_macro::with_trace_id]
    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        info!("got updated configuration"; "config" => params.settings.as_object().unwrap().get("mcshader").unwrap().to_string());

        let config: Configuration = Configuration::new(&params.settings);

        let mut new_extensions = constant::BASIC_EXTENSIONS.clone();
        new_extensions.extend(config.extra_extension.clone());
        self.extensions.lock().unwrap().clone_from(&new_extensions);

        let registrations: Vec<Registration> = Vec::from([
            config.generate_file_watch_registration()
        ]);
        if let Err(_err) = self.client.register_capability(registrations).await {
            warn!("Unable to registe file watch capability");
        }

        match logging::Level::from_str(config.log_level.as_str()) {
            Ok(level) => logging::set_level(level),
            Err(_) => error!("got unexpected log level from config"; "level" => &config.log_level),
        }
    }

    #[logging::with_trace_id]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.set_status_loading("Linting file...".to_string()).await;

        let file_path = params.text_document.uri.to_file_path().unwrap();

        if let Some(diagnostics) = self.server_data.open_file(&file_path, &self.diagnostics_parser, &self.opengl_context) {
            self.publish_diagnostic(diagnostics).await;    
        }

        self.set_status_ready().await;
    }

    #[logging::with_trace_id]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let file_path = params.text_document.uri.to_file_path().unwrap();

        self.server_data.change_file(&file_path, params.content_changes);
    }

    #[logging::with_trace_id]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.set_status_loading("Linting file...".to_string()).await;

        let file_path = params.text_document.uri.to_file_path().unwrap();

        let extensions = self.extensions.lock().unwrap().clone();
        if let Some(diagnostics) = self.server_data.save_file(&file_path, &extensions, &self.diagnostics_parser, &self.opengl_context) {
            self.publish_diagnostic(diagnostics).await;    
        }

        self.set_status_ready().await;
    }

    #[logging::with_trace_id]
    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let file_path = params.text_document.uri.to_file_path().unwrap();

        if let Some(include_links) = self.server_data.include_links(&file_path) {
            Ok(Some(include_links))
        }
        else {
            Err(Error::parse_error())
        }
    }

    #[logging::with_trace_id]
    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        self.set_status_loading("Applying work space changes...".to_string()).await;

        self.server_data.update_work_spaces(params.event);

        self.set_status_ready().await;
    }

    #[logging_macro::with_trace_id]
    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        self.set_status_loading("Applying changes into file system...".to_string()).await;

        let diagnostics = self.server_data.update_watched_files(params.changes, &self.diagnostics_parser, &self.opengl_context);

        self.publish_diagnostic(diagnostics).await;
        self.set_status_ready().await;
    }
}
