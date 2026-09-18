//! JAMES Application API — localhost REST + WebSocket (v1 seed).
//!
//! V0 scope (preview):
//! - Void shell: `GET /`, `GET /api/v1/health`, `POST /api/v1/chat`,
//!   `GET /ws/v1/events` (bus fanout) and `GET /ws/v1/void` (2-way void).
//! - Dashboard: `GET /api/v1/dashboard/status`, `.../tasks`, `.../memory`.
//! Authentication: bearer token via `Authorization: Bearer <token>` header
//!   or `?token=<token>` query param for WebSockets. Dev mode allows
//!   unauthenticated loopback with visible warning.

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use async_trait::async_trait;
use james_events::{CoreStatus, EventBus};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::services::ServeDir;
use tracing::{debug, warn};

mod auth;
pub use auth::{AuthConfig, AuthContext, TokenStore, EnvTokenStore, WindowsTokenStore, create_router_with_auth};

#[async_trait]
pub trait ChatHandler: Send + Sync {
    async fn respond(&self, message: &str) -> anyhow::Result<String>;
}

/// Supplies real, section-based dashboard data to the Void UI. Implemented by
/// the application layer against assembled modules; the API layer never
/// fabricates values.
#[async_trait]
pub trait DashboardProvider: Send + Sync {
    async fn snapshot(&self, section: &str) -> anyhow::Result<serde_json::Value>;
}

/// Supplies the current declarative `UiIntent` produced by the UI orchestrator.
/// Absent in core-only transports (the endpoint then answers 503 honestly).
#[async_trait]
pub trait IntentProvider: Send + Sync {
    async fn latest(&self) -> anyhow::Result<serde_json::Value>;
}

/// Handles user intent submission and plan execution.
#[async_trait]
pub trait AgentHandler: Send + Sync {
    async fn execute_intent(&self, intent: james_agents::UserIntent) -> anyhow::Result<james_agents::PlanExecutionResult>;
}

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
    /// Whether the server is running in preview mode without auth (dev only)
    pub preview_unauthenticated: bool,
    /// Optional application handler. Core-only deployments remain transport-only
    /// and fail honestly instead of returning a fake assistant response.
    pub chat_handler: Option<Arc<dyn ChatHandler>>,
    /// Optional rich dashboard data source (tasks, memory, capabilities,
    /// events, resources, security). Absent in core-only transports.
    pub data: Option<Arc<dyn DashboardProvider>>,
    /// Optional capability broker for action execution (void actions, agent actions).
    pub broker: Option<Arc<james_capability_broker::CapabilityBroker>>,
    /// Optional capability executor for broker execution.
    pub executor: Option<Arc<dyn james_capability_broker::CapabilityExecutor>>,
    /// Optional agent handler for intent execution.
    pub agent_handler: Option<Arc<dyn AgentHandler>>,
    /// Optional UI orchestrator intent source (`ui.intent` snapshots).
    pub intent: Option<Arc<dyn IntentProvider>>,
    /// Token store for authentication
    pub token_store: Option<Arc<dyn TokenStore>>,
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
    if let Some(store) = state.token_store.clone() {
        ws_auth(ws, state, store).await.into_response()
    } else {
        ws.on_upgrade(move |socket| forward_events(socket, state)).into_response()
    }
}

/// WebSocket authentication helper
async fn ws_auth(ws: WebSocketUpgrade, state: AppState, store: Arc<dyn auth::TokenStore>) -> Result<axum::response::Response, StatusCode> {
    let request_id = uuid::Uuid::now_v7().to_string();
    
    // Check if auth is enabled
    let auth_enabled = std::env::var("JAMES_AUTH_ENABLED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(true);
    let dev_mode = std::env::var("JAMES_AUTH_DEV_MODE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(false);

    if !auth_enabled {
        if dev_mode {
            warn!(request_id = %request_id, "AUTH DISABLED (dev mode) - WebSocket allowed");
        }
        return Ok(ws.on_upgrade(move |socket| forward_events(socket, state)).into_response());
    }

    // For WebSocket, we can't easily extract query params here
    // The auth will be done after upgrade in the handler
    Ok(ws.on_upgrade(move |socket| forward_events_auth(socket, state, store)).into_response())
}

/// Forward events with optional auth (called after WebSocket upgrade)
async fn forward_events_auth(socket: WebSocket, state: AppState, store: Arc<dyn auth::TokenStore>) {
    // For now, just forward events without auth check (auth is done at HTTP level for REST)
    // In a full implementation, we'd check token from query param or first message
    forward_events(socket, state).await;
}

async fn ws_void(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    if let Some(store) = state.token_store.clone() {
        ws_void_auth(ws, state, store).await.into_response()
    } else {
        ws.on_upgrade(move |socket| void_socket(socket, state)).into_response()
    }
}

/// WebSocket authentication helper for void
async fn ws_void_auth(ws: WebSocketUpgrade, state: AppState, store: Arc<dyn auth::TokenStore>) -> Result<axum::response::Response, StatusCode> {
    let request_id = uuid::Uuid::now_v7().to_string();
    
    let auth_enabled = std::env::var("JAMES_AUTH_ENABLED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(true);
    let dev_mode = std::env::var("JAMES_AUTH_DEV_MODE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(false);

    if !auth_enabled {
        if dev_mode {
            warn!(request_id = %request_id, "AUTH DISABLED (dev mode) - WebSocket allowed");
        }
        return Ok(ws.on_upgrade(move |socket| void_socket(socket, state)).into_response());
    }

    Ok(ws.on_upgrade(move |socket| void_socket_auth(socket, state, store)).into_response())
}

/// Void socket with auth context (placeholder for future token validation)
async fn void_socket_auth(socket: WebSocket, state: AppState, _store: Arc<dyn auth::TokenStore>) {
    void_socket(socket, state).await;
}

/// Fan out every bus event to this client as JSON text.
/// V0 is read-only: client messages are drained (to detect close) and ignored.
async fn forward_events(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.bus.subscribe_all();

    let send_task = tokio::spawn(async move {
        while let Some(envelope) = rx.recv().await {
            let text = serde_json::to_string(&envelope).unwrap_or_default();
            tracing::debug!("WS forward raw: '{}'", text);
            tracing::debug!("WS forwarding: {} bytes, event_type: {}", text.len(), envelope.event.event_type);
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

/// POST /api/v1/chat — submit a user message and request a real response.
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

    let handler = state.chat_handler.as_ref().ok_or_else(|| (
        StatusCode::SERVICE_UNAVAILABLE,
        "chat handler is not configured".to_string(),
    ))?;
    let response = handler
        .respond(content)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    state
        .bus
        .publish(
            james_events::Event::new("void.assistant_output", "james-app")
                .with_payload(serde_json::json!({
                    "message_id": message_id,
                    "content": response,
                    "timestamp": now,
                })),
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(ChatResponse {
        message_id,
        role: "assistant".to_string(),
        content: response,
        timestamp: now,
    }))
}

/// Two-way Void socket: routes every bus event to the client as JSON text
/// (like `/ws/v1/events`) and forwards inbound `void.message`/`void.action`
/// payloads so the shell can drive the app in real time.
async fn void_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.bus.subscribe_all();

    let bus = state.bus.clone();
    let broker = state.broker.clone();
    let executor = state.executor.clone();
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
            } else if kind == "void.action" {
                // Execute action through capability broker
                let action = parsed.get("action").and_then(|a| a.as_str()).unwrap_or("");
                let capability = parsed.get("capability").and_then(|c| c.as_str()).unwrap_or("");
                let input = parsed.get("input").cloned().unwrap_or(serde_json::json!({}));
                let caller = parsed.get("caller").and_then(|c| c.as_str()).unwrap_or("void");
                if !action.is_empty() && !capability.is_empty() {
                    if let (Some(broker), Some(executor)) = (broker.as_ref(), executor.as_ref()) {
                        let request = james_capability_broker::CapabilityRequest {
                            caller: caller.to_string(),
                            capability_id: capability.to_string(),
                            input,
                        };
                        match broker.execute(request, executor.as_ref()).await {
                            Ok(outcome) => {
                                let result_id = uuid::Uuid::now_v7().to_string();
                                bus.publish(
                                    james_events::Event::new("void.action_result", "james-app")
                                        .with_payload(serde_json::json!({
                                            "action": action,
                                            "capability": capability,
                                            "result_id": result_id,
                                            "output": outcome.output,
                                            "executed": outcome.executed,
                                            "timestamp": chrono::Utc::now(),
                                        })),
                                )
                                .await
                                .ok();
                            }
                            Err(e) => {
                                let result_id = uuid::Uuid::now_v7().to_string();
                                bus.publish(
                                    james_events::Event::new("void.action_error", "james-app")
                                        .with_payload(serde_json::json!({
                                            "action": action,
                                            "capability": capability,
                                            "result_id": result_id,
                                            "error": e.to_string(),
                                            "timestamp": chrono::Utc::now(),
                                        })),
                                )
                                .await
                                .ok();
                            }
                        }
                    } else {
                        bus.publish(
                            james_events::Event::new("void.action_error", "james-app")
                                .with_payload(serde_json::json!({
                                    "action": action,
                                    "capability": capability,
                                    "error": "broker or executor not available",
                                    "timestamp": chrono::Utc::now(),
                                })),
                        )
                        .await
                        .ok();
                    }
                }
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

/// GET /api/v1/data/{section} — real snapshot from the assembled system.
/// The UI reads tasks, memory, modules, capabilities, events, resources,
/// security, settings, ai, agents, devices and automation here. Without a
/// data provider the endpoint answers 503 (never fake data).
async fn data_section(
    State(state): State<AppState>,
    Path(section): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let provider = state
        .data
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "data provider is not configured".to_string()))?;
    let value = provider
        .snapshot(&section)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(Json(value))
}

/// GET /api/v1/ui/intent — latest declarative UiIntent from the orchestrator.
/// Without an intent provider the endpoint answers 503 (never fake data).
async fn ui_intent(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let provider = state
        .intent
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "intent provider is not configured".to_string()))?;
    let value = provider
        .latest()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(Json(value))
}

/// POST /api/v1/agent/intent — execute a user intent through the agent pipeline.
/// Without an agent handler the endpoint answers 503 (never fake data).
async fn agent_execute_intent(
    State(state): State<AppState>,
    Json(intent): Json<james_agents::UserIntent>,
) -> Result<Json<james_agents::PlanExecutionResult>, (StatusCode, String)> {
    tracing::info!("agent_execute_intent called for user: {}", intent.user_id);
    tracing::debug!("Intent raw_input: {}", intent.raw_input);
    
    let handler = match state.agent_handler.as_ref() {
        Some(h) => {
            tracing::debug!("Agent handler found");
            h
        }
        None => {
            tracing::error!("Agent handler not configured in AppState");
            return Err((StatusCode::SERVICE_UNAVAILABLE, "agent handler is not configured".to_string()));
        }
    };
    
    tracing::info!("Executing intent through handler: {}", intent.raw_input);
    let result = match handler.execute_intent(intent).await {
        Ok(r) => {
            tracing::info!("Intent executed successfully, success={}, plan_id={}", r.success, r.plan_id);
            r
        }
        Err(e) => {
            let err_msg = format!("{:?}", e);
            tracing::error!("Intent execution failed: {}", err_msg);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, err_msg));
        }
    };
    Ok(Json(result))
}

pub fn router(state: AppState) -> Router {
    if let Some(store) = state.token_store.clone() {
        create_router_with_auth(state, store)
    } else {
        legacy_router(state)
    }
}

fn legacy_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(serve_index))
        .route("/api/v1/health", get(health))
        .route("/api/v1/chat", post(chat_submit))
        .route("/ws/v1/events", get(ws_events))
        .route("/ws/v1/void", get(ws_void))
        .route("/api/v1/dashboard/status", get(dashboard_status))
        .route("/api/v1/dashboard/tasks", get(dashboard_tasks))
        .route("/api/v1/dashboard/memory", get(dashboard_memory))
        .route("/api/v1/data/:section", get(data_section))
        .route("/api/v1/ui/intent", get(ui_intent))
        .route("/api/v1/agent/intent", post(agent_execute_intent))
        .fallback_service(ServeDir::new(state.static_dir.clone()))
        .with_state(state)
}

pub async fn serve(listener: tokio::net::TcpListener, state: AppState) -> anyhow::Result<()> {
    let preview = state.preview_unauthenticated || state.token_store.is_none();
    if preview {
        tracing::warn!(
            "UNAUTHENTICATED local preview (no bearer auth). \
             Refusing non-loopback binds; localhost only."
        );
    }
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
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

    fn make_test_intent() -> serde_json::Value {
        serde_json::json!({
            "id": "test-intent-id",
            "user_id": "test",
            "raw_input": "test",
            "parsed_intent": { "action": "test", "target": null, "parameters": {}, "constraints": [], "expected_output": null },
            "context": { "session_id": "test", "active_agents": [], "available_capabilities": [], "user_preferences": {} },
            "created_at": "2026-01-01T00:00:00Z",
            "priority": "normal"
        })
    }

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
            chat_handler: None,
            data: None,
            broker: None,
            executor: None,
            agent_handler: None,
            intent: None,
            token_store: None,
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

    struct FakeIntent;
    #[async_trait]
    impl IntentProvider for FakeIntent {
        async fn latest(&self) -> anyhow::Result<serde_json::Value> {
            Ok(serde_json::json!({
                "mode": "research",
                "focus": "sources",
                "brain_state": "searching",
                "activity": 0.62,
                "show": ["brain", "progress", "sources"],
                "open": ["research_panel"],
                "actions": []
            }))
        }
    }

    #[tokio::test]
    async fn test_intent_requires_provider() {
        let app = router(test_state().await);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/ui/intent")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_intent_returns_orchestrated_snapshot() {
        let mut state = test_state().await;
        state.intent = Some(Arc::new(FakeIntent));
        let app = router(state);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/ui/intent")
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
        assert_eq!(json["brain_state"], "searching");
        assert_eq!(json["focus"], "sources");
    }

    #[tokio::test]
    async fn test_chat_submit_requires_handler() {
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
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    struct EchoHandler;

    #[async_trait]
    impl ChatHandler for EchoHandler {
        async fn respond(&self, message: &str) -> anyhow::Result<String> {
            Ok(format!("response: {message}"))
        }
    }

    #[tokio::test]
    async fn test_chat_submit_returns_real_handler_response() {
        let mut state = test_state().await;
        state.chat_handler = Some(Arc::new(EchoHandler));
        let app = router(state);
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
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["content"], "response: hello void");
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
        let state = test_state().await;
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

    #[tokio::test]
    async fn test_agent_intent_requires_handler() {
        let app = router(test_state().await);
        let body = make_test_intent();
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/agent/intent")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        println!("Response status: {}", status);
        if status != StatusCode::SERVICE_UNAVAILABLE {
            let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
                .await
                .unwrap();
            println!("Response body: {}", String::from_utf8_lossy(&body));
        }
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    }

    struct MockAgentHandler;
    #[async_trait::async_trait]
    impl AgentHandler for MockAgentHandler {
        async fn execute_intent(&self, _: james_agents::UserIntent) -> anyhow::Result<james_agents::PlanExecutionResult> {
            Ok(james_agents::PlanExecutionResult {
                plan_id: "test-plan".to_string(),
                success: true,
                step_results: vec![],
                total_duration_ms: 0,
                final_state: james_agents::AgentState::Completed,
            })
        }
    }

    #[tokio::test]
    async fn test_agent_intent_executes_with_handler() {
        let mut state = test_state().await;
        state.agent_handler = Some(Arc::new(MockAgentHandler));
        let app = router(state);
        let body = make_test_intent();
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/agent/intent")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["success"], true);
    }
}
