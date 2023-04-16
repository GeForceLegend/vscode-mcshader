use std::sync::MutexGuard;

use serde_json::Value;
use tower_lsp::jsonrpc::Result;

use crate::server::ServerData;

mod virtual_merge;

pub struct VirtualMerge {}

pub trait Command {
    fn run(&self, arguments: &[Value], server_data: &MutexGuard<ServerData>) -> Result<Option<Value>>;
}
