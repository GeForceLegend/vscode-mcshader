use std::sync::{Mutex, MutexGuard};

use hashbrown::HashMap;
use serde_json::Value;
use tower_lsp::jsonrpc::Result;

use crate::server::{LanguageServerError, ServerData};

mod virtual_merge;

pub struct CommandList {
    commands: HashMap<String, Box<dyn Command + Sync + Send>>,
}

impl CommandList {
    pub fn new() -> CommandList {
        let mut command_list = CommandList { commands: HashMap::new() };
        command_list
            .commands
            .insert("virtualMerge".to_owned(), Box::new(virtual_merge::VirtualMerge {}));
        command_list
    }

    pub fn execute(&self, command: &str, arguments: &[Value], server_data: &Mutex<ServerData>) -> Result<Option<Value>> {
        let server_data = server_data.lock().unwrap();
        match self.commands.get(command) {
            Some(command) => command.run(arguments, &server_data),
            None => Err(LanguageServerError::invalid_command_error()),
        }
    }
}

pub trait Command {
    fn run(&self, arguments: &[Value], server_data: &MutexGuard<ServerData>) -> Result<Option<Value>>;
}
