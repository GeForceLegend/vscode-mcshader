use std::collections::HashSet;

use serde::Deserialize;
use serde_json::{Value, from_value};
use tower_lsp::lsp_types::*;

#[derive(Deserialize)]
pub struct Configuration {
    #[serde(alias = "logLevel")]
    pub log_level: String,
    #[serde(alias = "extraExtension")]
    pub extra_extension: HashSet<String>,
}

impl Configuration {
    pub fn new(value: &Value) -> Configuration {
        from_value(value.as_object().unwrap().get("mcshader").unwrap().to_owned()).unwrap()
    }

    pub fn generate_file_watch_registration(&self) -> Vec<Registration> {
        let mut glsl_file_watcher_pattern = "**/*{vsh,gsh,fsh,csh,glsl".to_string();
        self.extra_extension
            .iter()
            .for_each(|extension| {
                glsl_file_watcher_pattern += &format!(",{}", extension);
            });
        glsl_file_watcher_pattern += "}";

        let did_change_watched_files = DidChangeWatchedFilesRegistrationOptions {
            watchers: Vec::from([FileSystemWatcher{
                glob_pattern: glsl_file_watcher_pattern,
                kind: Some(WatchKind::all())
            }]),
        };
        Vec::from([Registration{
            id: "workspace/didChangeWatchedFiles".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
            register_options: Some(serde_json::to_value(did_change_watched_files).unwrap()),
        }])
    }
}
