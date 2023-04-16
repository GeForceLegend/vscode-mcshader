use tower_lsp::{LspService, Server};
use tree_sitter::Parser;

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

    let mut tree_sitter_parser = Parser::new();
    tree_sitter_parser.set_language(tree_sitter_glsl::language()).unwrap();

    let (service, socket) = LspService::new(|client| server::MinecraftLanguageServer::new(client, tree_sitter_parser));
    Server::new(stdin, stdout, socket).serve(service).await;
}
