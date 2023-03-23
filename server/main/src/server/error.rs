use tower_lsp::jsonrpc::{Error, ErrorCode};

use super::LanguageServerError;

impl LanguageServerError{
    #[inline]
    pub fn not_shader_error() -> Error {
        Error {
            code: ErrorCode::ServerError(-20002),
            message: String::from("This is not a base shader file"),
            data: None
        }
    }

    #[inline]
    pub fn invalid_command_error() -> Error {
        Error {
            code: ErrorCode::ServerError(-20101),
            message: String::from("Invalid command"),
            data: None
        }
    }

    #[inline]
    pub fn invalid_argument_error() -> Error {
        Error {
            code: ErrorCode::ServerError(-20102),
            message: String::from("Invalid command argument"),
            data: None
        }
    }
}
