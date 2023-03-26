use std::{
    path::{PathBuf, MAIN_SEPARATOR_STR},
    collections::HashMap,
    sync::MutexGuard
};

use serde_json::Value;
use tower_lsp::jsonrpc::Result;

use crate::server::{ServerData, LanguageServerError};

use super::Command;

pub struct VirtualMerge {}

impl Command for VirtualMerge {
    fn run(&self, arguments: &[Value], server_data: &MutexGuard<ServerData>) -> Result<Option<Value>> {
        let value = arguments.get(0).unwrap();
        if !value.is_string() {
            return Err(LanguageServerError::invalid_argument_error());
        }
        let file_uri = value.to_string();
        #[cfg(target_os = "windows")]
        let file_path = PathBuf::from(file_uri.strip_prefix("\"/").unwrap().strip_suffix("\"").unwrap().replace("/", MAIN_SEPARATOR_STR));
        #[cfg(not(target_os = "windows"))]
        let file_path = PathBuf::from(file_uri.strip_prefix("\"").unwrap().strip_suffix("\"").unwrap().replace("/", MAIN_SEPARATOR_STR));

        let shader_files = server_data.shader_files().borrow();
        let include_files = server_data.include_files().borrow();
        let temp_files = server_data.temp_files().borrow();

        let content: String;
        let mut file_list = HashMap::new();
        if let Some(shader_file) = shader_files.get(&file_path) {
            content = shader_file.merge_shader_file(&include_files, &file_path, &mut file_list);
        }
        else if let Some(temp_file) = temp_files.get(&file_path) {
            content = match temp_file.merge_self(&file_path, &mut file_list) {
                Some(temp_content) => temp_content.1,
                None => return Err(LanguageServerError::not_shader_error()),
            }
        }
        else {
            return Err(LanguageServerError::not_shader_error());
        }

        Ok(Some(Value::String(content)))
    }
}
