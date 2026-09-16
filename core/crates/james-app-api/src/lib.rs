//! JAMES Application API — localhost REST + WebSocket (v1 seed).
//!
//! V0 scope (preview):
//! - Void shell: `GET /`, `GET /api/v1/health`, `POST /api/v1/chat`,
//!   `GET /ws/v1/events` (bus fanout) and `GET /ws/v1/void` (2-way void).
//! - Dashboard: `GET /api/v1/dashboard/status`, `.../tasks`, `.../memory`.
//! No authentication yet — the server binds loopback only and REFUSES
//! non-loopback binds until bearer auth lands (A7b). The `preview_
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
    routing::{get, post},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use james_events::{CoreStatus, EventBus};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    pub bus: Arc<EventBus>,
    pub status: Arc<RwLock<CoreStatus>>,
    pub version: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub static_dir: String,
    /// Live dashboard snapshot (status/tasks/memory counters), refreshed by
    /// the application layer. Defaults to zero values in the API harness.
    pub dashboard: Arc<RwLock<DashboardSnapshot>>,
    /// Always true in V0: preview mode without bearer auth (A7b closes it).
    pub preview_unauthenticated: bool,
}

/// Snapshot data the Dashboard endpoints serve.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DashboardSnapshot {
    pub core_status: String,
    pub modules: usize,
    pub capabilities: usize,
    pub agents: usize,
    pub tasks_pending: usize,
    pub tasks_running: usize,
    pub tasks_completed: usize,
    pub memory_entries: usize,
    pub events_dropped: usize,
    pub uptime_secs: u64,
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

#[derive(Deserialize)]
struct ChatRequest {
    message: String,
}

#[derive(Serialize)]
struct ChatResponse {
    message_id: String,
    role: String,
    content: String,
    timestamp: chrono::DateTime<chrono::Utc>,
}

/// POST /api/v1/chat — submit a user message to the Void.
/// V0 echoes the message as an accepted ack and publishes a `void.user_input`
/// event so connected shells and the module layer can react. Real inference
/// lands with C2.
async fn chat_submit(State(state): State<AppState>, Json(req): Json<ChatRequest>) -> Result<Json<ChatResponse>, (StatusCode, String)> {
    let content = req.message.trim();
    if content.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "message must not be empty".to_string()));
    }
    let message_id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now();
    state
        .bus
        .publish(
            james_events::Event::new("void.user_input", "james-app")
                .with_payload(serde_json::json!({
                    "message_id": message_id,
                    "content": content,
                    "timestamp": now,
                })),
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(ChatResponse {
        message_id,
        role: "user".to_string(),
        content: content.to_string(),
        timestamp: now,
    }))
}

async fn ws_void(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| void_socket(socket, state))
}

/// Two-way Void socket: routes every bus event to the client as JSON text
/// (like `/ws/v1/events`) and forwards inbound `void.message` text payloads to
/// the `void.user_input` handler so the shell can drive the app in real time.
async fn void_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.bus.subscribe_all();

    let bus = state.bus.clone();
    let send_task = tokio::spawn(async move {
        while let Some(envelope) = rx.recv().await {
            let text = serde_json::to_string(&envelope).unwrap_or_default();
            if sender.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(Message::Text(text))) = receiver.next().await {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
            let kind = parsed.get("type").and_then(|t| t.as_str()).unwrap_or("void.message");
            let content = parsed
                .get("content")
                .and_then(|c| c.as_str())
                .or_else(|| parsed.get("message").and_then(|m| m.as_str()))
                .unwrap_or("");
            if kind == "void.message" && !content.is_empty() {
                let message_id = uuid::Uuid::now_v7().to_string();
                bus.publish(
                    james_events::Event::new("void.user_input", "james-app")
                        .with_payload(serde_json::json!({
                            "message_id": message_id,
                            "content": content,
                            "timestamp": chrono::Utc::now(),
                        })),
                )
                .await
                .ok();
            }
        }
    }

    send_task.abort();
}

async fn dashboard_status(State(state): State<AppState>) -> Json<DashboardSnapshot> {
    let snap = state.dashboard.read().await.clone();
    Json(snap)
}

async fn dashboard_tasks(State(state): State<AppState>) -> Json<DashboardSnapshot> {
    let snap = state.dashboard.read().await.clone();
    Json(DashboardSnapshot {
        tasks_pending: snap.tasks_pending,
        tasks_running: snap.tasks_running,
        tasks_completed: snap.tasks_completed,
        ..Default::default()
    })
}

async fn dashboard_memory(State(state): State<AppState>) -> Json<DashboardSnapshot> {
    let snap = state.dashboard.read().await.clone();
    Json(DashboardSnapshot {
        memory_entries: snap.memory_entries,
        ..Default::default()
    })
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(serve_index))
        .route("/api/v1/health", get(health))
        .route("/api/v1/chat", post(chat_submit))
        .route("/ws/v1/events", get(ws_events))
        .route("/ws/v1/void", get(ws_void))
        .route("/api/v1/dashboard/status", get(dashboard_status))
        .route("/api/v1/dashboard/tasks", get(dashboard_tasks))
        .route("/api/v1/dashboard/memory", get(dashboard_memory))
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

    async fn test_state() -> AppState {
        let bus = Arc::new(EventBus::new(100));
        bus.start().await.unwrap();
        AppState {
            bus,
            status: Arc::new(RwLock::new(CoreStatus::Running)),
            version: "0.1.0-test".to_string(),
            started_at: chrono::Utc::now(),
            static_dir: "nonexistent-dir".to_string(),
            dashboard: Arc::new(RwLock::new(DashboardSnapshot {
                core_status: "Running".to_string(),
                ..Default::default()
            })),
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
        let app = router(test_state().await);
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
        let app = router(test_state().await);
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

    #[tokio::test]
    async fn test_chat_submit_accepts_message() {
        let app = router(test_state().await);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/chat")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"message":"hello void"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "hello void");
        assert!(json["message_id"].as_str().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn test_chat_submit_rejects_empty_message() {
        let app = router(test_state().await);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/chat")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"message":"   "}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_dashboard_status_snapshot() {
        let mut state = test_state().await;
        state.dashboard = Arc::new(RwLock::new(DashboardSnapshot {
            core_status: "Running".to_string(),
            modules: 5,
            capabilities: 12,
            tasks_completed: 3,
            memory_entries: 7,
            uptime_secs: 42,
            ..Default::default()
        }));
        let app = router(state);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/dashboard/status")
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
        assert_eq!(json["modules"], 5);
        assert_eq!(json["memory_entries"], 7);
    }

    #[tokio::test]
    async fn test_dashboard_tasks_memory_endpoints() {
        let mut state = test_state().await;
        {
            let mut snap = state.dashboard.write().await;
            snap.tasks_pending = 2;
            snap.tasks_running = 1;
            snap.tasks_completed = 9;
            snap.memory_entries = 4;
        }
        let app = router(state);

        let tasks = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/dashboard/tasks")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(tasks.status(), StatusCode::OK);
        let tbody = axum::body::to_bytes(tasks.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let tjson: serde_json::Value = serde_json::from_slice(&tbody).unwrap();
        assert_eq!(tjson["tasks_pending"], 2);
        assert_eq!(tjson["tasks_completed"], 9);
        assert_eq!(tjson["memory_entries"], 0);

        let memory = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/dashboard/memory")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(memory.status(), StatusCode::OK);
        let mbody = axum::body::to_bytes(memory.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let mjson: serde_json::Value = serde_json::from_slice(&mbody).unwrap();
        assert_eq!(mjson["memory_entries"], 4);
        assert_eq!(mjson["tasks_pending"], 0);
    }
}
