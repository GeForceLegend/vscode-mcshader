use tower_lsp::lsp_types::*;
use tower_lsp::jsonrpc::Result;

pub struct ServerCapabilitiesFactroy {
}

impl ServerCapabilitiesFactroy {
    pub fn initial_capabilities() -> Result<InitializeResult> {
        let shader_filter = Some(FileOperationRegistrationOptions{
            filters: Vec::from([FileOperationFilter{
                scheme: Some("file".to_string()),
                pattern: FileOperationPattern{
                    glob: "**/*.{vsh,gsh,fsh,csh}".to_string(),
                    matches: None,
                    options: None,
                },
            }])
        });
        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".to_string()]),
                    work_done_progress_options: Default::default(),
                    all_commit_characters: None,
                    ..Default::default()
                }),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["dummy.do_something".to_string()],
                    work_done_progress_options: Default::default(),
                }),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: Some(WorkspaceFileOperationsServerCapabilities{
                        did_create: shader_filter.clone(),
                        will_create: None,
                        did_rename: None,
                        will_rename: None,
                        did_delete: shader_filter.clone(),
                        will_delete: None,
                    }),
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