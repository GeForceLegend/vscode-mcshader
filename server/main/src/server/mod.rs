use std::collections::{HashSet, HashMap};
use std::path::PathBuf;
use std::str::FromStr;

use logging::{error, info, warn};

use tower_lsp::jsonrpc::{Result, Error, ErrorCode};
use tower_lsp::lsp_types::{*, self};
use tower_lsp::{Client, LanguageServer};

mod data_manager;
mod server_data;

use crate::capability::ServerCapabilitiesFactroy;
use crate::configuration::Configuration;
use crate::constant;
use crate::diagnostics_parser::DiagnosticsParser;
use crate::enhancer::FromUrl;
use crate::notification;
use crate::opengl;
use crate::shader_file::{IncludeFile, ShaderFile};

use self::data_manager::DataManager;
use self::server_data::ServerData;

pub struct MinecraftLanguageServer {
    pub client: Client,
    diagnostics_parser: DiagnosticsParser,
    server_data: ServerData,
    _log_guard: logging::GlobalLoggerGuard,
}

impl MinecraftLanguageServer {
    pub fn new(client: Client, diagnostics_parser: DiagnosticsParser) -> MinecraftLanguageServer {
        MinecraftLanguageServer {
            client,
            diagnostics_parser,
            server_data: ServerData::new(),
            _log_guard: logging::init_logger(),
        }
    }

    fn temp_shader_pack(&self, file_path: &mut PathBuf) -> bool {
        while file_path.file_name().unwrap() != "shaders" {
            if !file_path.pop() {
                return false;
            }
        }
        true
    }

    fn temp_lint(&self, file_path: &PathBuf, pack_path: &PathBuf) -> HashMap<Url, Vec<Diagnostic>> {
        let opengl_context = opengl::OpenGlContext::new();

        let mut file_list: HashMap<String, PathBuf> = HashMap::new();
        let extension = match file_path.extension(){
            Some(extension) => extension,
            None => return HashMap::new()
        };
        let is_shader = constant::DEFAULT_SHADERS.contains(file_path.file_name().unwrap().to_str().unwrap());
        let file_type = if extension == "fsh" && is_shader {
                gl::FRAGMENT_SHADER
            } else if extension == "vsh" && is_shader {
                gl::VERTEX_SHADER
            } else if extension == "gsh" && is_shader {
                gl::GEOMETRY_SHADER
            } else if extension == "csh" && is_shader {
                gl::COMPUTE_SHADER
            } else {
                let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                diagnostics.entry(Url::from_file_path(file_path).unwrap()).or_default();
                return diagnostics;
            };

        let shader_content = ShaderFile::temp_merge_shader(file_path, pack_path, &mut file_list);
        let validation_result = opengl_context.validate_shader(&file_type, &shader_content);

        // Copied from original file
        match &validation_result {
            Some(output) => {
                info!("compilation errors reported"; "errors" => format!("`{}`", output.replace('\n', "\\n")), "tree_root" => file_path.to_str().unwrap())
            }
            None => {
                info!("compilation reported no errors"; "tree_root" => file_path.to_str().unwrap());
                let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                diagnostics.entry(Url::from_file_path(file_path).unwrap()).or_default();
                for include_file in file_list {
                    diagnostics.entry(Url::from_file_path(&include_file.1).unwrap()).or_default();
                }
                return diagnostics;
            },
        };

        self.diagnostics_parser.parse_diagnostics(validation_result.unwrap(), file_list)
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
        if params.workspace_folders.is_none() {
            let root = match params.root_uri {
                Some(uri) => PathBuf::from_url(uri),
                None => {
                    return Err(Error {
                        code: ErrorCode::InvalidParams,
                        message: "Must be in workspace".into(),
                        data: Some(serde_json::to_value(InitializeError { retry: false }).unwrap()),
                    });
                }
            };
            roots.insert(root);
        }
        else {
            for root in params.workspace_folders.unwrap() {
                roots.insert(PathBuf::from_url(root.uri));
            }
        }

        self.server_data.initial_scan(roots);

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

        let mut glsl_file_watcher_pattern = "**/*{vsh,gsh,fsh,csh,glsl".to_string();
        config.extra_extension
            .iter()
            .for_each(|extension| {
                glsl_file_watcher_pattern += &format!(",{}", extension);
            });
        glsl_file_watcher_pattern += "}";

        let did_change_watched_files = lsp_types::DidChangeWatchedFilesRegistrationOptions {
            watchers: Vec::from([FileSystemWatcher{
                glob_pattern: glsl_file_watcher_pattern,
                kind: Some(WatchKind::all())
            }]),
        };
        let registrations: Vec<Registration> = Vec::from([
            Registration{
                id: "workspace/didChangeWatchedFiles".to_string(),
                method: "workspace/didChangeWatchedFiles".to_string(),
                register_options: Some(serde_json::to_value(did_change_watched_files).unwrap()),
            }
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

        let file_path = PathBuf::from_url(params.text_document.uri);

        let diagnostics = match self.server_data.open_file(&file_path, &self.diagnostics_parser) {
            Some(diagnostics) => diagnostics,
            None => {
                warn!("Document not found in file system"; "path" => file_path.to_str().unwrap());
                info!("Trying to automanticly detect related shader pack path...");

                let mut shader_pack = file_path.clone();
                if !self.temp_shader_pack(&mut shader_pack) {
                    self.set_status_ready().await;
                    return;
                }
                info!("Found related shader pack path"; "path" => shader_pack.to_str().unwrap());

                self.temp_lint(&file_path, &shader_pack)
            }
        };

        self.publish_diagnostic(diagnostics).await;
        self.set_status_ready().await;
    }

    #[logging::with_trace_id]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.set_status_loading("Linting file...".to_string()).await;

        let file_path = PathBuf::from_url(params.text_document.uri);

        let diagnostics = match self.server_data.save_file(&file_path, &self.diagnostics_parser) {
            Some(diagnostics) => diagnostics,
            None => {
                let mut shader_pack = file_path.clone();
                if !self.temp_shader_pack(&mut shader_pack) {
                    self.set_status_ready().await;
                    return;
                }
                self.temp_lint(&file_path, &shader_pack)
            }
        };

        self.publish_diagnostic(diagnostics).await;
        self.set_status_ready().await;
    }

    #[logging::with_trace_id]
    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let file_path = PathBuf::from_url(params.text_document.uri);

        let include_list = match self.server_data.include_list(&file_path) {
            Some(includes) => includes,
            None => {
                let mut shader_pack = file_path.clone();
                if !self.temp_shader_pack(&mut shader_pack) {
                    return Err(Error::parse_error());
                }

                IncludeFile::temp_search_include(&shader_pack, &file_path)
            }
        };

        let include_links: Vec<DocumentLink> = include_list
            .iter()
            .map(|include_file| {
                let include_path = &include_file.3;
                let url = Url::from_file_path(include_path).unwrap();
                DocumentLink {
                    range: Range::new(
                        Position::new(u32::try_from(include_file.0).unwrap(), u32::try_from(include_file.1).unwrap()),
                        Position::new(u32::try_from(include_file.0).unwrap(), u32::try_from(include_file.2).unwrap()),
                    ),
                    tooltip: Some(url.path().to_string()),
                    target: Some(url),
                    data: None,
                }
            })
            .collect();

        Ok(Some(include_links))
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

        let diagnostics = self.server_data.update_watched_files(params.changes, &self.diagnostics_parser);

        self.publish_diagnostic(diagnostics).await;
        self.set_status_ready().await;
    }
}
