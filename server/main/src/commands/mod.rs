use std::{collections::HashMap, sync::{Mutex, MutexGuard}};

use serde_json::Value;
use tower_lsp::jsonrpc::Result;

use crate::server::{ServerData, LanguageServerError};

mod virtual_merge;

pub struct CommandList {
    commands: HashMap<String, Box<dyn Command + Sync + Send>>
}

impl CommandList {
    pub fn new() -> CommandList {
        let mut command_list = CommandList {
            commands: HashMap::new(),
        };
        command_list.commands.insert("virtualMerge".into(), Box::new(virtual_merge::VirtualMerge{}));
        command_list
    }

    pub fn execute(&self, command: &String, arguments: &[Value], server_data: &Mutex<ServerData>) -> Result<Option<Value>> {
        let server_data = server_data.lock().unwrap();
        if let Some(command) = self.commands.get(command) {
            return command.run(arguments, &server_data);
        }
        return Err(LanguageServerError::invalid_command_error());
    }
}

pub trait Command {
    fn run(&self, arguments: &[Value], server_data: &MutexGuard<ServerData>) -> Result<Option<Value>>;
}
