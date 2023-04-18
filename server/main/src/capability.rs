use tower_lsp::lsp_types::*;

pub struct ServerCapabilitiesFactroy {}

impl ServerCapabilitiesFactroy {
    pub fn initial_capabilities() -> InitializeResult {
        InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::INCREMENTAL)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["virtualMerge".to_owned(); 1],
                    work_done_progress_options: Default::default(),
                }),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                        // will_rename: Some(FileOperationRegistrationOptions {
                        //     filters: vec![FileOperationFilter{
                        //         scheme: Some(String::from("file")),
                        //         pattern: FileOperationPattern {
                        //             glob: String::from("**/*.{vsh,gsh,fsh,csh,glsl}"),
                        //             matches: Some(FileOperationPatternKind::File),
                        //             options: None
                        //         }
                        //     }]
                        // }),
                        ..Default::default()
                    }),
                }),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions { work_done_progress: None },
                }),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
