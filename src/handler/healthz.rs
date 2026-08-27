use axum::response::{IntoResponse, Json};
use serde_json::json;

pub async fn healthz_handler() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "jxlify",
        "version": crate::config::DEFAULT_VERSION
    }))
}
