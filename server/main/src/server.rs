use std::collections::{HashSet, HashMap, LinkedList};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;

use regex::Regex;
use logging::{error, info, warn};

use serde::Deserialize;
use serde_json::from_value;
use tower_lsp::jsonrpc::{Result, Error, ErrorCode};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use lazy_static::{lazy_static, __Deref};

use crate::capability::ServerCapabilitiesFactroy;
use crate::diagnostics_parser;
use crate::enhancer::{FromUrl, FromUri};
use crate::notification;
use crate::opengl;
use crate::shader_file;

lazy_static! {
    static ref RE_DIMENSION_FOLDER: Regex = Regex::new(r#"^world-?\d+"#).unwrap();
    static ref DEFAULT_SHADERS: HashSet<String> = {
        let mut set = HashSet::with_capacity(1716);
        for ext in ["fsh", "vsh", "gsh", "csh"] {
            set.insert(format!("composite.{}", ext));
            set.insert(format!("deferred.{}", ext));
            set.insert(format!("prepare.{}", ext));
            set.insert(format!("shadowcomp.{}", ext));
            for i in 1..=99 {
                let total_suffix = format!("{}.{}", i, ext);
                set.insert(format!("composite{}", total_suffix));
                set.insert(format!("deferred{}", total_suffix));
                set.insert(format!("prepare{}", total_suffix));
                set.insert(format!("shadowcomp{}", total_suffix));
            }
            set.insert(format!("composite_pre.{}", ext));
            set.insert(format!("deferred_pre.{}", ext));
            set.insert(format!("final.{}", ext));
            set.insert(format!("gbuffers_armor_glint.{}", ext));
            set.insert(format!("gbuffers_basic.{}", ext));
            set.insert(format!("gbuffers_beaconbeam.{}", ext));
            set.insert(format!("gbuffers_block.{}", ext));
            set.insert(format!("gbuffers_clouds.{}", ext));
            set.insert(format!("gbuffers_damagedblock.{}", ext));
            set.insert(format!("gbuffers_entities.{}", ext));
            set.insert(format!("gbuffers_entities_glowing.{}", ext));
            set.insert(format!("gbuffers_hand.{}", ext));
            set.insert(format!("gbuffers_hand_water.{}", ext));
            set.insert(format!("gbuffers_item.{}", ext));
            set.insert(format!("gbuffers_line.{}", ext));
            set.insert(format!("gbuffers_skybasic.{}", ext));
            set.insert(format!("gbuffers_skytextured.{}", ext));
            set.insert(format!("gbuffers_spidereyes.{}", ext));
            set.insert(format!("gbuffers_terrain.{}", ext));
            set.insert(format!("gbuffers_terrain_cutout.{}", ext));
            set.insert(format!("gbuffers_terrain_cutout_mip.{}", ext));
            set.insert(format!("gbuffers_terrain_solid.{}", ext));
            set.insert(format!("gbuffers_textured.{}", ext));
            set.insert(format!("gbuffers_textured_lit.{}", ext));
            set.insert(format!("gbuffers_water.{}", ext));
            set.insert(format!("gbuffers_weather.{}", ext));
            set.insert(format!("shadow.{}", ext));
            set.insert(format!("shadow_cutout.{}", ext));
            set.insert(format!("shadow_solid.{}", ext));
        }
        let base_char_num = 'a' as u8;
        for suffix_num in 0u8..=25u8 {
            let suffix_char = (base_char_num + suffix_num) as char;
            set.insert(format!("composite_{}.csh", suffix_char));
            set.insert(format!("deferred_{}.csh", suffix_char));
            set.insert(format!("prepare_{}.csh", suffix_char));
            set.insert(format!("shadowcomp_{}.csh", suffix_char));
            for i in 1..=99 {
                let total_suffix = format!("{}_{}", i, suffix_char);
                set.insert(format!("composite{}.csh", total_suffix));
                set.insert(format!("deferred{}.csh", total_suffix));
                set.insert(format!("prepare{}.csh", total_suffix));
                set.insert(format!("shadowcomp{}.csh", total_suffix));
            }
        }
        set
    };
}

pub struct MinecraftLanguageServer {
    pub client: Client,
    diagnostics_parser: diagnostics_parser::DiagnosticsParser,
    roots: Mutex<HashSet<PathBuf>>,
    shader_packs: Mutex<HashSet<PathBuf>>,
    shader_files: Mutex<HashMap<PathBuf, shader_file::ShaderFile>>,
    include_files: Mutex<HashMap<PathBuf, shader_file::IncludeFile>>,
    _log_guard: logging::GlobalLoggerGuard,
}

impl MinecraftLanguageServer {
    pub fn new(client: Client, diagnostics_parser: diagnostics_parser::DiagnosticsParser) -> MinecraftLanguageServer {
        MinecraftLanguageServer {
            client,
            diagnostics_parser,
            roots: Mutex::from(HashSet::new()),
            shader_packs: Mutex::from(HashSet::new()),
            shader_files: Mutex::from(HashMap::new()),
            include_files: Mutex::from(HashMap::new()),
            _log_guard: logging::init_logger(),
        }
    }

    fn add_shader_file(&self, shader_files: &mut HashMap<PathBuf, shader_file::ShaderFile>,
        include_files: &mut HashMap<PathBuf, shader_file::IncludeFile>, pack_path: &PathBuf, file_path: PathBuf
    ) {
        if DEFAULT_SHADERS.contains(file_path.file_name().unwrap().to_str().unwrap()) {
            let mut shader_file = shader_file::ShaderFile::new(pack_path, &file_path);
            shader_file.read_file(include_files);
            shader_files.insert(file_path, shader_file);
        }
    }

    fn update_file(&self, path: &PathBuf) {
        let mut shader_files = self.shader_files.lock().unwrap().deref().clone();
        let mut include_files = self.include_files.lock().unwrap().deref().clone();
        if shader_files.contains_key(path) {
            let shader_file = shader_files.get_mut(path).unwrap();
            shader_file.clear_including_files();
            shader_file.read_file(&mut include_files);
        }
        if include_files.contains_key(path) {
            let mut include_file = include_files.remove(path).unwrap();
            include_file.update_include(&mut include_files);
            include_files.insert(path.clone(), include_file);
        }
        *self.shader_files.lock().unwrap() = shader_files;
        *self.include_files.lock().unwrap() = include_files;
    }

    fn remove_shader_file(&self, shader_files: &mut HashMap<PathBuf, shader_file::ShaderFile>,
        include_files: &mut HashMap<PathBuf, shader_file::IncludeFile>, file_path: &PathBuf
    ) {
        shader_files.remove(file_path);
        for include_file in include_files {
            let included_shaders = include_file.1.included_shaders_mut();
            if included_shaders.contains(file_path) {
                included_shaders.remove(file_path);
            }
        }
    }

    fn find_shader_packs(&self, curr_path: &PathBuf) -> HashSet<PathBuf> {
        let mut work_spaces: HashSet<PathBuf> = HashSet::new();
        for file in curr_path.read_dir().expect("read directory failed") {
            if let Ok(file) = file {
                let file_path = file.path();
                if file_path.is_dir() {
                    let file_name = file_path.file_name().unwrap();
                    if file_name == "shaders" {
                        info!("find work space {}", &file_path.to_str().unwrap());
                        work_spaces.insert(file_path);
                    }
                    else {
                        work_spaces.extend(self.find_shader_packs(&file_path));
                    }
                }
            }
        }
        work_spaces
    }

    fn scan_new_root(&self, shader_files: &mut HashMap<PathBuf, shader_file::ShaderFile>,
        include_files: &mut HashMap<PathBuf, shader_file::IncludeFile>, root: &PathBuf
    ) {
        info!("generating file framework on current root"; "root" => root.to_str().unwrap());

        let shader_packs: HashSet<PathBuf> = self.find_shader_packs(root);

        for shader_pack in &shader_packs {
            for file in shader_pack.read_dir().expect("read work space failed") {
                if let Ok(file) = file {
                    let file_path = file.path();
                    if file_path.is_file() {
                        self.add_shader_file(shader_files, include_files, shader_pack, file_path);
                    }
                    else if file_path.is_dir() && RE_DIMENSION_FOLDER.is_match(file_path.file_name().unwrap().to_str().unwrap()) {
                        for dim_file in file_path.read_dir().expect("read dimension folder failed") {
                            if let Ok(dim_file) = dim_file {
                                let file_path = dim_file.path();
                                if file_path.is_file() {
                                    self.add_shader_file(shader_files, include_files, shader_pack, file_path);
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut total_shader_packs: HashSet<PathBuf> = self.shader_packs.lock().unwrap().clone();
        total_shader_packs.extend(shader_packs);
        *self.shader_packs.lock().unwrap() = total_shader_packs;
    }

    fn build_file_framework(&self) {
        let mut shader_files = self.shader_files.lock().unwrap().deref().clone();
        let mut include_files = self.include_files.lock().unwrap().deref().clone();

        let mut total_shader_packs: HashSet<PathBuf> = HashSet::new();
        for root in self.roots.lock().unwrap().clone() {
            info!("generating file framework on current root"; "root" => root.to_str().unwrap());

            let shader_packs: HashSet<PathBuf> = self.find_shader_packs(&root);
            for shader_pack in &shader_packs {
                for file in shader_pack.read_dir().expect("read work space failed") {
                    if let Ok(file) = file {
                        let file_path = file.path();
                        if file_path.is_file() {
                            self.add_shader_file(&mut shader_files, &mut include_files, shader_pack, file_path);
                        }
                        else if file_path.is_dir() && RE_DIMENSION_FOLDER.is_match(file_path.file_name().unwrap().to_str().unwrap()) {
                            for dim_file in file_path.read_dir().expect("read dimension folder failed") {
                                if let Ok(dim_file) = dim_file {
                                    let file_path = dim_file.path();
                                    if file_path.is_file() {
                                        self.add_shader_file(&mut shader_files, &mut include_files, shader_pack, file_path);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            total_shader_packs.extend(shader_packs);
        }
        *self.shader_packs.lock().unwrap() = total_shader_packs;
        *self.shader_files.lock().unwrap() = shader_files;
        *self.include_files.lock().unwrap() = include_files;
    }

    fn update_lint(&self, path: &PathBuf) -> HashMap<Url, Vec<Diagnostic>> {
        let opengl_context = opengl::OpenGlContext::new();
        let mut shader_files = self.shader_files.lock().unwrap().deref().clone();
        let mut include_files = self.include_files.lock().unwrap().deref().clone();
        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        if shader_files.contains_key(path) {
            diagnostics.extend(self.lint_shader(&mut shader_files, &mut include_files, path, &opengl_context));
        }
        if include_files.contains_key(path) {
            let include_file = include_files.get(path).unwrap();
            for shader_path in include_file.included_shaders().clone() {
                diagnostics.extend(self.lint_shader(&mut shader_files, &mut include_files, &shader_path, &opengl_context));
            }
        }
        *self.shader_files.lock().unwrap() = shader_files;
        *self.include_files.lock().unwrap() = include_files;
        diagnostics
    }

    fn lint_shader(&self, shader_files: &mut HashMap<PathBuf, shader_file::ShaderFile>,
        include_files: &mut HashMap<PathBuf, shader_file::IncludeFile>,
        path: &PathBuf, opengl_context: &opengl::OpenGlContext
    ) -> HashMap<Url, Vec<Diagnostic>> {
        if !path.exists() {
            self.remove_shader_file(shader_files, include_files, path);
            return HashMap::new();
        }
        let shader_file = shader_files.get(path).unwrap();

        let mut file_list: HashMap<String, PathBuf> = HashMap::new();
        let shader_content = shader_file.merge_shader_file(include_files, &mut file_list);

        let validation_result = opengl_context.validate_shader(shader_file.file_type(), &shader_content);

        // Copied from original file
        match &validation_result {
            Some(output) => {
                info!("compilation errors reported"; "errors" => format!("`{}`", output.replace('\n', "\\n")), "tree_root" => path.to_str().unwrap())
            }
            None => {
                info!("compilation reported no errors"; "tree_root" => path.to_str().unwrap());
                let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                diagnostics.entry(Url::from_file_path(path).unwrap()).or_default();
                for include_file in shader_file.including_files() {
                    diagnostics.entry(Url::from_file_path(&include_file.3).unwrap()).or_default();
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
        self.roots.lock().unwrap().extend(roots);

        initialize_result
    }

    #[logging::with_trace_id]
    async fn initialized(&self, _params: InitializedParams) {
        self.set_status_loading("Building file system...".to_string()).await;

        self.build_file_framework();

        self.set_status_ready().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    #[logging::with_trace_id]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.set_status_loading("Linting file...".to_string()).await;

        let file_path = PathBuf::from_url(params.text_document.uri);
        let diagnostics = self.update_lint(&file_path);
        self.publish_diagnostic(diagnostics).await;

        self.set_status_ready().await;
    }

    #[logging::with_trace_id]
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.set_status_loading("Linting file...".to_string()).await;

        let file_path = PathBuf::from_url(params.text_document.uri);
        self.update_file(&file_path);
        let diagnostics = self.update_lint(&file_path);
        self.publish_diagnostic(diagnostics).await;

        self.set_status_ready().await;
    }

    #[logging::with_trace_id]
    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let curr_doc = PathBuf::from_url(params.text_document.uri);

        let include_list: &LinkedList<(usize, usize, usize, PathBuf)>;
        let shader_files = self.shader_files.lock().unwrap().clone();
        let include_files = self.include_files.lock().unwrap().clone();
        if shader_files.contains_key(&curr_doc) {
            include_list = shader_files.get(&curr_doc).unwrap().including_files();
        }
        else if include_files.contains_key(&curr_doc) {
            include_list = include_files.get(&curr_doc).unwrap().including_files();
        }
        else {
            warn!("document not found in file system"; "path" => curr_doc.to_str().unwrap());
            return Err(Error::parse_error());
        }

        let include_links: Vec<DocumentLink> = include_list
            .iter()
            .map(|include_file| {
                let path = &include_file.3;
                let url = Url::from_file_path(path).unwrap();
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

        let mut roots = self.roots.lock().unwrap().clone();
        let mut shader_files = self.shader_files.lock().unwrap().clone();
        let mut include_files = self.include_files.lock().unwrap().clone();
        for removed_uri in params.event.removed {
            let removed_path = PathBuf::from_url(removed_uri.uri);
            roots.remove(&removed_path);
            let shader_packup = shader_files.clone();
            for shader in &shader_packup {
                if shader.0.starts_with(&removed_path) {
                    self.remove_shader_file(&mut shader_files, &mut include_files, shader.0);
                }
            }
        }
        for added_uri in params.event.added {
            let added_path = PathBuf::from_url(added_uri.uri);
            self.scan_new_root(&mut shader_files, &mut include_files, &added_path);
            roots.insert(added_path);
        }
        *self.shader_files.lock().unwrap() = shader_files;
        *self.include_files.lock().unwrap() = include_files;

        self.set_status_ready().await;
    }

    #[logging_macro::with_trace_id]
    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        #[derive(Deserialize)]
        struct Configuration {
            #[serde(alias = "logLevel")]
            log_level: String,
        }

        let config: Configuration = from_value(params.settings.as_object().unwrap().get("mcglsl").unwrap().to_owned()).unwrap();

        info!("got updated configuration"; "config" => params.settings.as_object().unwrap().get("mcglsl").unwrap().to_string());

        match logging::Level::from_str(config.log_level.as_str()) {
            Ok(level) => logging::set_level(level),
            Err(_) => error!("got unexpected log level from config"; "level" => &config.log_level),
        }
    }

    #[logging_macro::with_trace_id]
    async fn did_delete_files(&self, params: DeleteFilesParams) {
        self.set_status_loading("Deleting file from file system...".to_string()).await;

        let mut shader_files = self.shader_files.lock().unwrap().clone();
        let mut include_files = self.include_files.lock().unwrap().clone();

            // let file_url = Url::from_file_path(file_path).unwrap();
        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();

        for file in params.files {
            let file_path = PathBuf::from_uri(file.uri);
            diagnostics.insert(Url::from_file_path(&file_path).unwrap(), Vec::new());
            if shader_files.contains_key(&file_path) {
                self.remove_shader_file(&mut shader_files, &mut include_files, &file_path);
            }
        }
        *self.shader_files.lock().unwrap() = shader_files;
        *self.include_files.lock().unwrap() = include_files;

        self.publish_diagnostic(diagnostics).await;
        self.set_status_ready().await;
    }

    #[logging_macro::with_trace_id]
    async fn did_create_files(&self, params: CreateFilesParams) {
        self.set_status_loading("Adding file into file system...".to_string()).await;

        let shader_packs = self.shader_packs.lock().unwrap().clone();
        let mut shader_files = self.shader_files.lock().unwrap().clone();
        let mut include_files = self.include_files.lock().unwrap().clone();

        for file in params.files {
            let file_path = PathBuf::from_uri(file.uri);
            for shader_pack in &shader_packs {
                if file_path.starts_with(&shader_pack) {
                    let relative_path = file_path.strip_prefix(shader_pack).unwrap();
                    if DEFAULT_SHADERS.contains(relative_path.to_str().unwrap()) {
                        self.add_shader_file(&mut shader_files, &mut include_files, shader_pack, file_path);
                    }
                    else {
                        let path_str = relative_path.to_str().unwrap().split_once("/").unwrap();
                        if RE_DIMENSION_FOLDER.is_match(path_str.0) && DEFAULT_SHADERS.contains(path_str.1) {
                            self.add_shader_file(&mut shader_files, &mut include_files, shader_pack, file_path);
                        }
                    }
                    break;
                }
            }
        }
        *self.shader_files.lock().unwrap() = shader_files;
        *self.include_files.lock().unwrap() = include_files;

        self.set_status_ready().await;
    }
}
