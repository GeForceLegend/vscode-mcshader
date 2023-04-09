use tower_lsp::jsonrpc::{Error, ErrorCode};

use super::LanguageServerError;

impl LanguageServerError {
    #[inline]
    pub fn not_shader_error() -> Error {
        Error {
            code: ErrorCode::ServerError(-20002),
            message: "This is not a base shader file".to_owned(),
            data: None,
        }
    }

    #[inline]
    pub fn invalid_command_error() -> Error {
        Error {
            code: ErrorCode::ServerError(-20101),
            message: "Invalid command".to_owned(),
            data: None,
        }
    }

    #[inline]
    pub fn invalid_argument_error() -> Error {
        Error {
            code: ErrorCode::ServerError(-20102),
            message: "Invalid command argument".to_owned(),
            data: None,
        }
    }
}
