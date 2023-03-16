use tower_lsp::lsp_types::*;
use tower_lsp::jsonrpc::Result;

pub struct ServerCapabilitiesFactroy {}

impl ServerCapabilitiesFactroy {
    pub fn initial_capabilities() -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![String::from("virtualMerge")],
                    work_done_progress_options: Default::default(),
                }),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                    // file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                    //     did_create: None,
                    //     will_create: None,
                    //     did_rename: Some(FileOperationRegistrationOptions {
                    //         filters: vec![FileOperationFilter{
                    //             scheme: Some(String::from("file")),
                    //             pattern: FileOperationPattern {
                    //                 glob: String::from("**/*.{vsh,gsh,fsh,csh,glsl}"),
                    //                 matches: Some(FileOperationPatternKind::File),
                    //                 options: None
                    //             }
                    //         }]
                    //     }),
                    //     will_rename: Some(FileOperationRegistrationOptions {
                    //         filters: vec![FileOperationFilter{
                    //             scheme: Some(String::from("file")),
                    //             pattern: FileOperationPattern {
                    //                 glob: String::from("**/*.{vsh,gsh,fsh,csh,glsl}"),
                    //                 matches: Some(FileOperationPatternKind::File),
                    //                 options: None
                    //             }
                    //         }]
                    //     }),
                    //     did_delete: None,
                    //     will_delete: None
                    // }),
                }),
                document_link_provider: Some(DocumentLinkOptions{
                    resolve_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions{
                        work_done_progress: None
                    }
                }),
                ..ServerCapabilities::default()
            },
            ..Default::default()
        })
    }
}
