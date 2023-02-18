use std::{
    sync::{Mutex, MutexGuard},
    collections::{HashSet, HashMap},
    path::{PathBuf, MAIN_SEPARATOR_STR}
};

use logging::info;
use tower_lsp::lsp_types::Diagnostic;
use url::Url;

use crate::constant;
use crate::diagnostics_parser::DiagnosticsParser;
use crate::opengl::OpenGlContext;
use crate::file::{ShaderFile, IncludeFile, TempFile};

pub fn extend_diagnostics(target: &mut HashMap<Url, Vec<Diagnostic>>, source: HashMap<Url, Vec<Diagnostic>>) {
    for file in source {
        if let Some(diagnostics) = target.get_mut(&file.0) {
            for diagnostic in file.1 {
                if !diagnostics.contains(&diagnostic) {
                    diagnostics.push(diagnostic);
                }
            }
        }
        else {
            target.insert(file.0, file.1);
        }
    }
}

pub struct ServerData {
    roots: Mutex<HashSet<PathBuf>>,
    shader_packs: Mutex<HashSet<PathBuf>>,
    shader_files: Mutex<HashMap<PathBuf, ShaderFile>>,
    include_files: Mutex<HashMap<PathBuf, IncludeFile>>,
    temp_files: Mutex<HashMap<PathBuf, TempFile>>,
}

impl ServerData {
    pub fn new() -> Self {
        ServerData {
            roots: Mutex::from(HashSet::new()),
            shader_packs: Mutex::from(HashSet::new()),
            shader_files: Mutex::from(HashMap::new()),
            include_files: Mutex::from(HashMap::new()),
            temp_files: Mutex::from(HashMap::new()),
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

    pub fn temp_files(&self) -> &Mutex<HashMap<PathBuf, TempFile>>{
        &self.temp_files
    }

    fn add_shader_file(&self, shader_files: &mut MutexGuard<HashMap<PathBuf, ShaderFile>>,
        include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>,
        pack_path: &PathBuf, file_path: PathBuf
    ) {
        let shader_file = ShaderFile::new(pack_path, &file_path, include_files);
        shader_files.insert(file_path, shader_file);
    }

    pub fn remove_shader_file(&self, shader_files: &mut MutexGuard<HashMap<PathBuf, ShaderFile>>,
        include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>, file_path: &PathBuf
    ) {
        shader_files.remove(file_path);

        include_files.values_mut()
            .for_each(|include_file|{
                let included_shaders = include_file.included_shaders_mut();
                included_shaders.remove(file_path);
            });
    }

    pub fn scan_new_file(&self, shader_packs: &mut MutexGuard<HashSet<PathBuf>>,
        shader_files: &mut MutexGuard<HashMap<PathBuf, ShaderFile>>,
        include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>, file_path: PathBuf
    ) -> bool {
        for shader_pack in shader_packs.iter() {
            if file_path.starts_with(&shader_pack) {
                let relative_path = file_path.strip_prefix(&shader_pack).unwrap();
                if constant::DEFAULT_SHADERS.contains(relative_path.to_str().unwrap()) {
                    self.add_shader_file(shader_files, include_files, &shader_pack, file_path);
                    return true;
                }
                else if let Some(result) = relative_path.to_str().unwrap().split_once(MAIN_SEPARATOR_STR) {
                    if constant::RE_DIMENSION_FOLDER.is_match(result.0) && constant::DEFAULT_SHADERS.contains(result.1) {
                        self.add_shader_file(shader_files, include_files, &shader_pack, file_path);
                        return true;
                    }
                }
                break;
            }
        }
        false
    }

    fn find_shader_packs(&self, curr_path: &PathBuf) -> HashSet<PathBuf> {
        let mut shader_packs: HashSet<PathBuf> = HashSet::new();
        for file in curr_path.read_dir().expect("read directory failed") {
            if let Ok(file) = file {
                let file_path = file.path();
                if file_path.is_dir() {
                    if file_path.file_name().unwrap() == "shaders" {
                        info!("Find shader pack {}", &file_path.to_str().unwrap());
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
        info!("Generating file framework on current root"; "root" => root.to_str().unwrap());

        let sub_shader_packs: HashSet<PathBuf>;
        if root.file_name().unwrap() == "shaders" {
            sub_shader_packs = HashSet::from([root.clone()]);
        }
        else {
            sub_shader_packs = self.find_shader_packs(root);
        }

        for shader_pack in &sub_shader_packs {
            for file in shader_pack.read_dir().expect("read work space failed") {
                if let Ok(file) = file {
                    let file_path = file.path();
                    if file_path.is_file() && constant::DEFAULT_SHADERS.contains(file_path.file_name().unwrap().to_str().unwrap()){
                        self.add_shader_file(shader_files, include_files, shader_pack, file_path);
                    }
                    else if constant::RE_DIMENSION_FOLDER.is_match(file_path.file_name().unwrap().to_str().unwrap()) {
                        for dim_file in file_path.read_dir().expect("read dimension folder failed") {
                            if let Ok(dim_file) = dim_file {
                                let file_path = dim_file.path();
                                if file_path.is_file() && constant::DEFAULT_SHADERS.contains(file_path.file_name().unwrap().to_str().unwrap()){
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

    pub fn lint_shader(&self, shader_files: &mut MutexGuard<HashMap<PathBuf, ShaderFile>>,
        include_files: &mut MutexGuard<HashMap<PathBuf, IncludeFile>>,
        file_path: &PathBuf, opengl_context: &OpenGlContext, diagnostics_parser: &DiagnosticsParser
    ) -> HashMap<Url, Vec<Diagnostic>> {
        if !file_path.exists() {
            self.remove_shader_file(shader_files, include_files, file_path);
            return HashMap::new();
        }
        let shader_file = shader_files.get(file_path).unwrap();

        let mut file_list: HashMap<String, PathBuf> = HashMap::new();
        let shader_content = shader_file.merge_shader_file(include_files, file_path, &mut file_list);

        let validation_result = opengl_context.validate_shader(shader_file.file_type(), &shader_content);

        match validation_result {
            Some(compile_log) => {
                info!("Compilation errors reported"; "errors" => format!("`{}`", compile_log.replace('\n', "\\n")), "tree_root" => file_path.to_str().unwrap());
                diagnostics_parser.parse_diagnostics(compile_log, file_list)
            },
            None => {
                info!("Compilation reported no errors"; "shader file" => file_path.to_str().unwrap());
                let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                diagnostics.entry(Url::from_file_path(file_path).unwrap()).or_default();
                for include_file in file_list {
                    diagnostics.entry(Url::from_file_path(&include_file.1).unwrap()).or_default();
                }
                diagnostics
            }
        }
    }

    pub fn temp_lint(&self, temp_file: &TempFile, file_path: &PathBuf, opengl_context: &OpenGlContext, diagnostics_parser: &DiagnosticsParser) -> HashMap<Url, Vec<Diagnostic>> {
        let mut file_list: HashMap<String, PathBuf> = HashMap::new();

        if let Some(result) = temp_file.merge_self(file_path, &mut file_list) {
            let validation_result = opengl_context.validate_shader(&result.1, &result.2);

            match validation_result {
                Some(compile_log) => {
                    info!("Compilation errors reported"; "errors" => format!("`{}`", compile_log.replace('\n', "\\n")), "tree_root" => result.0.to_str().unwrap());
                    diagnostics_parser.parse_diagnostics(compile_log, file_list)
                },
                None => {
                    info!("Compilation reported no errors"; "shader file" => result.0.to_str().unwrap());
                    let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
                    diagnostics.entry(Url::from_file_path(result.0).unwrap()).or_default();
                    for include_file in file_list {
                        diagnostics.entry(Url::from_file_path(&include_file.1).unwrap()).or_default();
                    }
                    diagnostics
                }
            }
        }
        else {
            HashMap::new()
        }
    }
}