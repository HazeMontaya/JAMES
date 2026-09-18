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
        Path, Query, State,
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
            format!("Void shell not found at {path} (build ui/void first)"),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct WsAuthQuery {
    token: Option<String>,
}

async fn ws_events(
    ws: WebSocketUpgrade,
    Query(query): Query<WsAuthQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Some(store) = state.token_store.clone() {
        ws_auth(ws, state, store, query.token).await.into_response()
    } else {
        ws.on_upgrade(move |socket| forward_events(socket, state)).into_response()
    }
}

/// WebSocket authentication helper
async fn ws_auth(ws: WebSocketUpgrade, state: AppState, store: Arc<dyn auth::TokenStore>, token: Option<String>) -> Result<axum::response::Response, StatusCode> {
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

    let token = token.ok_or(StatusCode::UNAUTHORIZED)?;
    let info = store.validate(&token).await.ok_or(StatusCode::UNAUTHORIZED)?;
    if let Some(expires) = info.expires_at {
        if chrono::Utc::now() > expires {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    Ok(ws.on_upgrade(move |socket| forward_events(socket, state)).into_response())
}

/// Forward events with optional auth (called after WebSocket upgrade)
async fn forward_events_auth(socket: WebSocket, state: AppState, store: Arc<dyn auth::TokenStore>) {
    // For now, just forward events without auth check (auth is done at HTTP level for REST)
    // In a full implementation, we'd check token from query param or first message
    forward_events(socket, state).await;
}

async fn ws_void(
    ws: WebSocketUpgrade,
    Query(query): Query<WsAuthQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Some(store) = state.token_store.clone() {
        ws_void_auth(ws, state, store, query.token).await.into_response()
    } else {
        ws.on_upgrade(move |socket| void_socket(socket, state)).into_response()
    }
}

/// WebSocket authentication helper for void
async fn ws_void_auth(ws: WebSocketUpgrade, state: AppState, store: Arc<dyn auth::TokenStore>, token: Option<String>) -> Result<axum::response::Response, StatusCode> {
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

    let token = token.ok_or(StatusCode::UNAUTHORIZED)?;
    let info = store.validate(&token).await.ok_or(StatusCode::UNAUTHORIZED)?;
    if let Some(expires) = info.expires_at {
        if chrono::Utc::now() > expires {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    Ok(ws.on_upgrade(move |socket| void_socket(socket, state)).into_response())
}

/// Fan out every bus event to this client as JSON text.
/// V0 is read-only: client messages are drained (to detect close) and ignored.
async fn forward_events(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.bus.subscribe_all();

    let send_task = tokio::spawn(async move {
        while let Some(envelope) = rx.recv().await {
            let text = serde_json::to_string(&envelope).unwrap_or_default();