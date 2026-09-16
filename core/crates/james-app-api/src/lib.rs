//! JAMES Application API — localhost REST + WebSocket (v1 seed).
//!
//! V0 scope (preview): `GET /api/v1/health`, `GET /ws/v1/events`
//! (read-only fanout of the shared EventBus), static file serving for the
//! Void shell. No authentication yet — the server binds loopback only and
//! REFUSES non-loopback binds until bearer auth lands (A7b). The `preview_
//! unauthenticated` flag and the startup warning make this explicit instead
//! of silently insecure.

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use james_events::{CoreStatus, EventBus};
use serde::Serialize;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    pub bus: Arc<EventBus>,
    pub status: Arc<RwLock<CoreStatus>>,
    pub version: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub static_dir: String,
    /// Always true in V0: preview mode without bearer auth (A7b closes it).
    pub preview_unauthenticated: bool,
}

#[derive(Serialize)]
struct HealthBody {
    service: String,
    version: String,
    status: String,
    preview_unauthenticated: bool,
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let status = state.status.read().await.clone();
    Json(HealthBody {
        service: "james-app".to_string(),
        version: state.version.clone(),
        status: status.as_str().to_string(),
        preview_unauthenticated: state.preview_unauthenticated,
    })
}

async fn serve_index(State(state): State<AppState>) -> impl IntoResponse {
    let path = format!("{}/index.html", state.static_dir.trim_end_matches('/'));
    match tokio::fs::read_to_string(&path).await {
        Ok(html) => Html(html).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            format!("Void shell not found at {path} (build interfaces/void first)"),
        )
            .into_response(),
    }
}

async fn ws_events(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| forward_events(socket, state))
}

/// Fan out every bus event to this client as JSON text.
/// V0 is read-only: client messages are drained (to detect close) and ignored.
async fn forward_events(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.bus.subscribe_all();

    let send_task = tokio::spawn(async move {
        while let Some(envelope) = rx.recv().await {
            let text = serde_json::to_string(&envelope).unwrap_or_default();
            if sender.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    });

    while receiver.next().await.is_some() {}
    send_task.abort();
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(serve_index))
        .route("/api/v1/health", get(health))
        .route("/ws/v1/events", get(ws_events))
        .fallback_service(ServeDir::new(state.static_dir.clone()))
        .with_state(state)
}

pub async fn serve(listener: tokio::net::TcpListener, state: AppState) -> anyhow::Result<()> {
    if state.preview_unauthenticated {
        tracing::warn!(
            "UNAUTHENTICATED local preview (no bearer auth yet — lands in A7b). \
             Refusing non-loopback binds; localhost only."
        );
    }
    axum::serve(listener, router(state)).await?;
    Ok(())
}

/// Fail-closed guard for the preview phase: without auth, only loopback.
pub fn preview_bind_allowed(bind: &str) -> bool {
    bind == "127.0.0.1" || bind == "localhost" || bind == "::1"
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        AppState {
            bus: Arc::new(EventBus::new(100)),
            status: Arc::new(RwLock::new(CoreStatus::Running)),
            version: "0.1.0-test".to_string(),
            started_at: chrono::Utc::now(),
            static_dir: "nonexistent-dir".to_string(),
            preview_unauthenticated: true,
        }
    }

    #[test]
    fn test_preview_refuses_non_loopback() {
        assert!(preview_bind_allowed("127.0.0.1"));
        assert!(preview_bind_allowed("localhost"));
        assert!(preview_bind_allowed("::1"));
        assert!(!preview_bind_allowed("0.0.0.0"));
        assert!(!preview_bind_allowed("192.168.1.10"));
        assert!(!preview_bind_allowed(""));
    }

    #[tokio::test]
    async fn test_health_reports_status() {
        let app = router(test_state());
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["service"], "james-app");
        assert_eq!(json["status"], "Running");
        assert_eq!(json["preview_unauthenticated"], true);
    }

    #[tokio::test]
    async fn test_index_falls_back_honestly_without_shell() {
        let app = router(test_state());
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
