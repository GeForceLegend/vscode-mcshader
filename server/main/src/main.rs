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
mod tree_parser;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(server::MinecraftLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
