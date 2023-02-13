#![feature(option_get_or_insert_default)]
#![feature(linked_list_cursors)]

use server::MinecraftLanguageServer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

mod capability;
mod diagnostics_parser;
mod enhancer;
mod notification;
mod opengl;
mod server;
mod shader_file;

#[derive(Debug)]
struct Backend {
    client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult::default())
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    
    let opengl_content = opengl::OpenGlContext::new();
    let diagnostics_parser = diagnostics_parser::DiagnosticsParser::new(&opengl_content);

    let (service, socket) = LspService::new(|client|
        MinecraftLanguageServer::new(
            client,
            diagnostics_parser
        )
    );
    Server::new(stdin, stdout, socket)
        .serve(service)
        .await;
}
