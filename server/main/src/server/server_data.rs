use std::{
    sync::{Mutex, MutexGuard},
    collections::{HashSet, HashMap},
    path::PathBuf
};

use logging::info;
use tower_lsp::lsp_types::Diagnostic;
use url::Url;

use crate::constant;
use crate::diagnostics_parser::DiagnosticsParser;
use crate::opengl::OpenGlContext;
use crate::shader_file::{ShaderFile, IncludeFile};

pub struct ServerData {
    roots: Mutex<HashSet<PathBuf>>,
    shader_packs: Mutex<HashSet<PathBuf>>,
    shader_files: Mutex<HashMap<PathBuf, ShaderFile>>,
    include_files: Mutex<HashMap<PathBuf, IncludeFile>>,
}

impl ServerData {
    pub fn new() -> Self {
        ServerData {
            roots: Mutex::from(HashSet::new()),
            shader_packs: Mutex::from(HashSet::new()),
            shader_files: Mutex::from(HashMap::new()),
            include_files: Mutex::from(HashMap::new()),
        }
    }

    pub fn roots(&self) -> &Mutex<HashSet<PathBuf>>{
        &self.roots
    }

    pub fn shader_packs(&self) -> &Mutex<HashSet<PathBuf>>{
        &self.shader_packs
    }

    pub fn shader_files(&self) -> &Mutex<HashMap<PathBuf, ShaderFile>>{
        &self.shader_files
    }

    pub fn include_files(&self) -> &Mutex<HashMap<PathBuf, IncludeFile>>{
        &self.include_files
    }

    fn add_shader_file(&self, shader_files: &mut MutexGuard<HashMap<PathBuf, ShaderFile>>,
        include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>,
        pack_path: &PathBuf, file_path: PathBuf
    ) {
        if constant::DEFAULT_SHADERS.contains(file_path.file_name().unwrap().to_str().unwrap()) {
            let mut shader_file = ShaderFile::new(pack_path, &file_path);
            shader_file.read_file(include_files);
            shader_files.insert(file_path, shader_file);
        }
    }

    pub fn update_file(&self, shader_files: &mut MutexGuard<HashMap<PathBuf, ShaderFile>>,
        include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>, file_path: &PathBuf
    ) {
        if shader_files.contains_key(file_path) {
            let shader_file = shader_files.get_mut(file_path).unwrap();
            shader_file.clear_including_files();
            shader_file.read_file(include_files);
        }
        if include_files.contains_key(file_path) {
            let mut include_file = include_files.remove(file_path).unwrap();
            include_file.update_include(include_files);
            include_files.insert(file_path.clone(), include_file);
        }
    }

    pub fn remove_shader_file(&self, shader_files: &mut MutexGuard<HashMap<PathBuf, ShaderFile>>,
        include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>, file_path: &PathBuf
    ) {
        shader_files.remove(file_path);

        include_files
            .iter_mut()
            .for_each(|include_file| {
            let included_shaders = include_file.1.included_shaders_mut();
                if included_shaders.contains(file_path) {
                    included_shaders.remove(file_path);
                }
            });
    }

    pub fn scan_new_file(&self, shader_packs: &mut MutexGuard<HashSet<PathBuf>>,
        shader_files: &mut MutexGuard<HashMap<PathBuf, ShaderFile>>,
        include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>, file_path: PathBuf
    ) {
        let shader_packs = shader_packs.clone();
        for shader_pack in shader_packs {
            if file_path.starts_with(&shader_pack) {
                let relative_path = file_path.strip_prefix(&shader_pack).unwrap();
                if constant::DEFAULT_SHADERS.contains(relative_path.to_str().unwrap()) {
                    self.add_shader_file(shader_files, include_files, &shader_pack, file_path);
                }
                else {
                    let path_str = match relative_path.to_str().unwrap().split_once(std::path::MAIN_SEPARATOR_STR) {
                        Some(result) => result,
                        None => break
                    };
                    if constant::RE_DIMENSION_FOLDER.is_match(path_str.0) && constant::DEFAULT_SHADERS.contains(path_str.1) {
                        self.add_shader_file(shader_files, include_files, &shader_pack, file_path);
                    }
                }
                break;
            }
        }
    }

    fn find_shader_packs(&self, curr_path: &PathBuf) -> HashSet<PathBuf> {
        let mut shader_packs: HashSet<PathBuf> = HashSet::new();
        for file in curr_path.read_dir().expect("read directory failed") {
            if let Ok(file) = file {
                let file_path = file.path();
                if file_path.is_dir() {
                    let file_name = file_path.file_name().unwrap();
                    if file_name == "shaders" {
                        info!("find shader pack {}", &file_path.to_str().unwrap());
                        shader_packs.insert(file_path);
                    }
                    else {
                        shader_packs.extend(self.find_shader_packs(&file_path));
                    }
                }
            }
        }
        shader_packs
    }

    pub fn scan_files_in_root(&self, shader_packs: &mut MutexGuard<HashSet<PathBuf>>,
        shader_files: &mut MutexGuard<HashMap<PathBuf, ShaderFile>>,
        include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>, root: &PathBuf
    ) {
        info!("generating file framework on current root"; "root" => root.to_str().unwrap());

        let sub_shader_packs: HashSet<PathBuf> = self.find_shader_packs(root);

        for shader_pack in &sub_shader_packs {
            for file in shader_pack.read_dir().expect("read work space failed") {
                if let Ok(file) = file {
                    let file_path = file.path();
                    if file_path.is_file() {
                        self.add_shader_file(shader_files, include_files, shader_pack, file_path);
                    }
                    else if file_path.is_dir() && constant::RE_DIMENSION_FOLDER.is_match(file_path.file_name().unwrap().to_str().unwrap()) {
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

        shader_packs.extend(sub_shader_packs);
    }

    pub fn update_lint(&self, shader_files: &mut MutexGuard<HashMap<PathBuf, ShaderFile>>,
        include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>,
        file_path: &PathBuf, diagnostics_parser: &DiagnosticsParser
    ) -> HashMap<Url, Vec<Diagnostic>> {
        let opengl_context = OpenGlContext::new();

        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();

        if shader_files.contains_key(file_path) {
            diagnostics.extend(self.lint_shader(shader_files, include_files, file_path, &opengl_context, diagnostics_parser));
        }

        let include_file = include_files.get(file_path);
        match include_file {
            Some(include_file) => {
                let include_shader_list = include_file.included_shaders().clone();
                for shader_path in include_shader_list {
                    diagnostics.extend(self.lint_shader(shader_files, include_files, &shader_path, &opengl_context, diagnostics_parser));
                }
            },
            None => {}
        }

        diagnostics
    }

    fn lint_shader(&self, shader_files: &mut MutexGuard<HashMap<PathBuf, ShaderFile>>,
        include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>,
        file_path: &PathBuf, opengl_context: &OpenGlContext, diagnostics_parser: &DiagnosticsParser
    ) -> HashMap<Url, Vec<Diagnostic>> {
        if !file_path.exists() {
            self.remove_shader_file(shader_files, include_files, file_path);
            return HashMap::new();
        }
        let shader_file = shader_files.get(file_path).unwrap();

        let mut file_list: HashMap<String, PathBuf> = HashMap::new();
        let shader_content = shader_file.merge_shader_file(include_files, &mut file_list);

        let validation_result = opengl_context.validate_shader(shader_file.file_type(), &shader_content);

        // Copied from original file
        match &validation_result {
            Some(output) => {
                info!("compilation errors reported"; "errors" => format!("`{}`", output.replace('\n', "\\n")), "tree_root" => file_path.to_str().unwrap())
            }
            None => {
                info!("compilation reported no errors"; "tree_root" => file_path.to_str().unwrap());
                let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                diagnostics.entry(Url::from_file_path(file_path).unwrap()).or_default();
                for include_file in &file_list {
                    diagnostics.entry(Url::from_file_path(include_file.1).unwrap()).or_default();
                }
                return diagnostics;
            },
        };

        diagnostics_parser.parse_diagnostics(validation_result.unwrap(), file_list)
    }
}
