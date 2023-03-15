use std::{
    path::PathBuf,
    collections::HashMap,
    sync::MutexGuard
};

use path_slash::PathBufExt;
use serde_json::Value;

use crate::server::ServerData;

use super::Command;

pub struct VirtualMerge {}

impl Command for VirtualMerge {
    fn run(&self, arguments: &[Value], server_data: &MutexGuard<ServerData>) -> Result<Value, String> {
        let value = arguments.get(0).unwrap();
        if !value.is_string() {
            return Err(String::from("Invalid arguments"));
        }
        let file_uri = value.to_string();
        #[cfg(target_os = "windows")]
        let file_path = PathBuf::from_slash(file_uri.strip_prefix("\"/").unwrap().strip_suffix("\"").unwrap());
        #[cfg(not(target_os = "windows"))]
        let file_path = PathBuf::from_slash(file_uri.strip_prefix("\"").unwrap().strip_suffix("\"").unwrap());

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
                None => return Err(String::from("This is not a base shader file")),
            }
        }
        else {
            return Err(String::from("This is not a base shader file"));
        }

        Ok(Value::String(content))
    }
}
