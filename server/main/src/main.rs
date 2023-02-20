#![feature(option_get_or_insert_default)]
#![feature(linked_list_cursors)]

use tower_lsp::{LspService, Server};

mod capability;
mod commands;
mod configuration;
mod constant;
mod diagnostics_parser;
mod file;
mod notification;
mod opengl;
mod server;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let opengl_content = opengl::OpenGlContext::new();
    let diagnostics_parser = diagnostics_parser::DiagnosticsParser::new(&opengl_content);

    let (service, socket) = LspService::new(|client|
        server::MinecraftLanguageServer::new(
            client,
            diagnostics_parser,
            opengl_content,
        )
    );
    Server::new(stdin, stdout, socket)
        .serve(service)
        .await;
}
