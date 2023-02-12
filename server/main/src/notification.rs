use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::notification::Notification;

#[derive(Deserialize, Serialize)]
pub struct StatusNotificationParams {
    pub status: String,
    pub message: String,
    pub icon: String,
}

pub enum StatusNotification {}

impl Notification for StatusNotification {
    type Params = StatusNotificationParams;

    const METHOD: &'static str = "mcshader/status";
}