use hashbrown::HashSet;
use serde::Deserialize;
use serde_json::{from_value, Value};
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
        let mut glsl_file_pattern = "**/*.{vsh,gsh,fsh,csh,glsl".to_owned();
        let mut folder_pattern = "**/shaders/**/*[!{.vsh,.gsh,.fsh,.csh,.glsl".to_owned();
        self.extra_extension.iter().for_each(|extension| {
            glsl_file_pattern += ",";
            folder_pattern += ",.";
            glsl_file_pattern += extension;
            folder_pattern += extension;
        });
        glsl_file_pattern += "}";
        folder_pattern += "}]";

        let did_change_watched_files = DidChangeWatchedFilesRegistrationOptions {
            watchers: vec![
                FileSystemWatcher {
                    glob_pattern: GlobPattern::String(glsl_file_pattern),
                    kind: Some(WatchKind::all()),
                },
                FileSystemWatcher {
                    glob_pattern: GlobPattern::String(folder_pattern),
                    kind: Some(WatchKind::Delete),
                },
            ],
        };
        // let will_rename_files = FileOperationRegistrationOptions {
        //     filters: vec![FileOperationFilter {
        //         scheme: Some(String::from("file")),
        //         pattern: FileOperationPattern {
        //             glob: glsl_pattern,
        //             matches: Some(FileOperationPatternKind::File),
        //             options: None
        //         }
        //     }]
        // };
        vec![
            Registration {
                id: "workspace/didChangeWatchedFiles".to_owned(),
                method: "workspace/didChangeWatchedFiles".to_owned(),
                register_options: Some(serde_json::to_value(did_change_watched_files).unwrap()),
            },
            // Registration {
            //     id: String::from("workspace/willRenameFiles"),
            //     method: String::from("workspace/willRenameFiles"),
            //     register_options: Some(serde_json::to_value(will_rename_files).unwrap()),
            // },
        ]
    }
}
