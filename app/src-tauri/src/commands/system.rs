//! Commandes générales de l'application.

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PingResponse {
    pub message: String,
    pub app_version: String,
    pub core_version: String,
}

/// Vérifie que l'IPC fonctionne et renvoie les versions.
#[tauri::command]
pub fn ping() -> PingResponse {
    tracing::debug!("ping");
    PingResponse {
        message: "pong".into(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        core_version: gram_core::VERSION.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_repond_pong() {
        let r = ping();
        assert_eq!(r.message, "pong");
        assert!(!r.app_version.is_empty());
        assert_eq!(r.core_version, gram_core::VERSION);
    }
}
