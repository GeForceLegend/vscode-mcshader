use std::{
    path::{PathBuf, MAIN_SEPARATOR_STR},
    sync::MutexGuard,
};

use hashbrown::HashMap;
use serde_json::Value;
use tower_lsp::jsonrpc::Result;

use crate::server::{LanguageServerError, ServerData};
use crate::file::*;

use super::Command;

pub struct VirtualMerge {}

impl Command for VirtualMerge {
    fn run(&self, arguments: &[Value], server_data: &MutexGuard<ServerData>) -> Result<Option<Value>> {
        let value = arguments.get(0).unwrap();
        let file_uri = match value.as_str() {
            Some(uri) => uri,
            None => return Err(LanguageServerError::invalid_argument_error()),
        };

        #[cfg(target_os = "windows")]
        let file_path = PathBuf::from(file_uri.strip_prefix("/").unwrap().replace("/", MAIN_SEPARATOR_STR));
        #[cfg(not(target_os = "windows"))]
        let file_path = PathBuf::from(file_uri.replace("/", MAIN_SEPARATOR_STR));

        let workspace_files = server_data.workspace_files().borrow();
        // let shader_files = server_data.shader_files().borrow();
        // let include_files = server_data.include_files().borrow();
        // let temp_files = server_data.temp_files().borrow();

        let mut shader_content = String::new();
        let mut file_list = HashMap::new();
        if let Some(workspace_file) = workspace_files.get(&file_path) {
            match *workspace_file.file_type().borrow() {
                gl::NONE | gl::INVALID_ENUM => return Err(LanguageServerError::not_shader_error()),
                _ => {
                    workspace_file.merge_file(&workspace_files, &mut file_list, &mut shader_content, &file_path, &mut -1, 0);
                    preprocess_shader(&mut shader_content, workspace_file.pack_path());
                }
            }
        } else {
            return Err(LanguageServerError::not_shader_error());
        }
        // if let Some(shader_file) = shader_files.get(&file_path) {
        //     content = shader_file.merge_shader_file(&include_files, &file_path, &mut file_list);
        // } else if let Some(temp_file) = temp_files.get(&file_path) {
        //     content = match temp_file.merge_self(&file_path, &mut file_list) {
        //         Some(temp_content) => temp_content.1,
        //         None => return Err(LanguageServerError::not_shader_error()),
        //     }
        // } else {
        //     return Err(LanguageServerError::not_shader_error());
        // }

        Ok(Some(Value::String(shader_content)))
    }
}
