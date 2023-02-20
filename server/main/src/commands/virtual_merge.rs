use std::{path::PathBuf, collections::HashMap};

use path_slash::PathBufExt;
use serde_json::Value;

use crate::server::ServerData;

use super::Command;

pub struct VirtualMerge {}

impl Command for VirtualMerge {
    fn run(&self, arguments: &[Value], server_data: &ServerData) -> Result<Value, String> {
        let value = arguments.get(0).unwrap();
        if !value.is_string() {
            return Err("Invalid arguments".to_string());
        }
        let file_uri = value.to_string();
        #[cfg(target_os = "windows")]
        let file_path = PathBuf::from_slash(file_uri.strip_prefix("\"/").unwrap().strip_suffix("\"").unwrap());
        #[cfg(not(target_os = "windows"))]
        let file_path = PathBuf::from_slash(file_uri.strip_prefix("\"").unwrap().strip_suffix("\"").unwrap());

        let shader_files = server_data.shader_files().lock().unwrap();
        let include_files = server_data.include_files().lock().unwrap();
        let temp_files = server_data.temp_files().lock().unwrap();

        let content: String;
        let mut file_list = HashMap::new();
        if let Some(shader_file) = shader_files.get(&file_path) {
            content = shader_file.merge_shader_file(&include_files, &file_path, &mut file_list);
        }
        else if let Some(temp_file) = temp_files.get(&file_path) {
            content = match temp_file.merge_self(&file_path, &mut file_list) {
                Some(temp_content) => temp_content.1,
                None => return Err("This is not a base shader file".to_string()),
            }
        }
        else {
            return Err("This is not a base shader file".to_string());
        }

        Ok(Value::String(content))
    }
}
