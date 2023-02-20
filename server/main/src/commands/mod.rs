use std::collections::HashMap;

use serde_json::Value;

use crate::server::ServerData;

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

    pub fn execute(&self, command: &String, arguments: &[Value], server_data: &ServerData) -> Result<Value, String> {
        if let Some(command) = self.commands.get(command) {
            return command.run(arguments, server_data);
        }
        return Err("Invalid command".to_string());
    }
}

pub trait Command {
    fn run(&self, arguments: &[Value], server_data: &ServerData) -> Result<Value, String>;
}
