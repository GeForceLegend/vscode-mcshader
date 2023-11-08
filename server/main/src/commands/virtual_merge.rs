use std::path::{PathBuf, MAIN_SEPARATOR_STR};

use hashbrown::HashMap;

use crate::file::*;

use super::*;

impl Command for VirtualMerge {
    fn run(&self, arguments: &[Value], server_data: &MutexGuard<ServerData>) -> Result<Option<Value>> {
        let value = arguments.get(0).unwrap();
        let file_uri = match value.as_str() {
            Some(uri) => uri,
            None => return Err(LanguageServerError::invalid_argument_error()),
        };

        #[cfg(target_os = "windows")]
        let file_path = PathBuf::from(file_uri.strip_prefix('/').unwrap().replace('/', MAIN_SEPARATOR_STR));
        #[cfg(not(target_os = "windows"))]
        let file_path = PathBuf::from(file_uri);

        let workspace_files = server_data.workspace_files().borrow();
        let temp_files = server_data.temp_files().borrow();

        let content = if let Some((file_path, workspace_file)) = workspace_files.get_key_value(&file_path) {
            match *workspace_file.file_type().borrow() {
                gl::NONE | gl::INVALID_ENUM => return Err(LanguageServerError::not_shader_error()),
                _ => {
                    let mut content = String::new();
                    workspace_file.merge_file(&mut HashMap::new(), workspace_file, &mut content, file_path, &mut -1, 0);
                    preprocess_shader(&mut content, workspace_file.shader_pack().debug);
                    content
                }
            }
        } else if let Some(temp_file) = temp_files.get(&file_path) {
            match temp_file.merge_self(&file_path) {
                Some(mut temp_content) => {
                    preprocess_shader(&mut temp_content, temp_file.shader_pack().debug);
                    temp_content
                }
                None => return Err(LanguageServerError::not_shader_error()),
            }
        } else {
            return Err(LanguageServerError::not_shader_error());
        };

        Ok(Some(Value::String(content)))
    }
}
