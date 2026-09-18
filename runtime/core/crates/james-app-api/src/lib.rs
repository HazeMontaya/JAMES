//! JAMES Application API — localhost REST + WebSocket (v1).
use std::sync::Arc;

use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, Path, Query, State},
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
use tracing::warn;

mod auth;
pub use auth::{AuthConfig, AuthContext, TokenStore, EnvTokenStore, WindowsTokenStore, create_router_with_auth};

#[async_trait]
pub trait ChatHandler: Send + Sync {
    async fn respond(&self, message: &str) -> anyhow::Result<String>;
}

#[async_trait]
pub trait DashboardProvider: Send + Sync {
    async fn snapshot(&self, section: &str) -> anyhow::Result<serde_json::Value>;
}

#[async_trait]
pub trait IntentProvider: Send + Sync {
    async fn latest(&self) -> anyhow::Result<serde_json::Value>;
}

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
    pub dashboard: Arc<RwLock<DashboardSnapshot>>,
    pub preview_unauthenticated: bool,
    pub chat_handler: Option<Arc<dyn ChatHandler>>,
    pub data: Option<Arc<dyn DashboardProvider>>,
    pub broker: Option<Arc<james_capability_broker::CapabilityBroker>>,
    pub executor: Option<Arc<dyn james_capability_broker::CapabilityExecutor>>,
    pub agent_handler: Option<Arc<dyn AgentHandler>>,
    pub intent: Option<Arc<dyn IntentProvider>>,
    pub token_store: Option<Arc<dyn TokenStore>>,
}

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
    Json(HealthBody {
        service: "james-app".to_string(),
        version: state.version.clone(),
        status: state.status.read().await.as_str().to_string(),
        preview_unauthenticated: state.preview_unauthenticated,
    })
}

async fn serve_index(State(state): State<AppState>) -> impl IntoResponse {
    let path = format!("{}/index.html", state.static_dir.trim_end_matches('/'));
    match tokio::fs::read_to_string(&path).await {
        Ok(html) => Html(html).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, format!("Void shell not found at {path} (build ui/void first)")).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct ChatRequest { message: String }

#[derive(Debug, Serialize)]
struct ChatResponse {
    message_id: String,
    role: String,
    content: String,
    timestamp: chrono::DateTime<chrono::Utc>,
}

async fn chat_submit(State(state): State<AppState>, Json(req): Json<ChatRequest>) -> Result<Json<ChatResponse>, (StatusCode,String)> {
    let content = req.message.trim();
    if content.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "message must not be empty".into()));
    }
    if let Some(handler) = state.chat_handler {
        let reply = handler.respond(content).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        return Ok(Json(ChatResponse {
            message_id: uuid::Uuid::now_v7().to_string(),
            role: "assistant".into(),
            content: reply,
            timestamp: chrono::Utc::now(),
        }));
    }
    let message_id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now();
    state.bus.publish(james_events::Event::new("void.user_input", "james-app")
        .with_payload(serde_json::json!({"message_id":message_id,"content":content,"timestamp":now})))
        .await.map_err(|e|(StatusCode::INTERNAL_SERVER_ERROR,e.to_string()))?;
    Ok(Json(ChatResponse { message_id, role:"user".into(), content:content.into(), timestamp:now }))
}

async fn dashboard_status(State(state): State<AppState>) -> Json<DashboardSnapshot> {
    Json(state.dashboard.read().await.clone())
}

async fn dashboard_tasks(State(state): State<AppState>) -> Json<DashboardSnapshot> {
    let s=state.dashboard.read().await.clone();
    Json(DashboardSnapshot{tasks_pending:s.tasks_pending,tasks_running:s.tasks_running,tasks_completed:s.tasks_completed,..Default::default()})
}

async fn dashboard_memory(State(state): State<AppState>) -> Json<DashboardSnapshot> {
    let s=state.dashboard.read().await.clone();
    Json(DashboardSnapshot{memory_entries:s.memory_entries,..Default::default()})
}

async fn data_section(Path(section): Path<String>, State(state): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode,String)> {
    match state.data {
        Some(provider) => provider.snapshot(&section).await.map(Json).map_err(|e|(StatusCode::INTERNAL_SERVER_ERROR,e.to_string())),
        None => Err((StatusCode::SERVICE_UNAVAILABLE,"data provider unavailable".into())),
    }
}

async fn ui_intent(State(state): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode,String)> {
    match state.intent {
        Some(provider) => provider.latest().await.map(Json).map_err(|e|(StatusCode::INTERNAL_SERVER_ERROR,e.to_string())),
        None => Err((StatusCode::SERVICE_UNAVAILABLE,"ui intent provider unavailable".into())),
    }
}

async fn agent_execute_intent(State(state): State<AppState>, Json(intent): Json<james_agents::UserIntent>) -> Result<Json<james_agents::PlanExecutionResult>, (StatusCode,String)> {
    match state.agent_handler {
        Some(handler) => handler.execute_intent(intent).await.map(Json).map_err(|e|(StatusCode::INTERNAL_SERVER_ERROR,e.to_string())),
        None => Err((StatusCode::SERVICE_UNAVAILABLE,"agent handler unavailable".into())),
    }
}

#[derive(Debug, Deserialize)]
struct WsAuthQuery { token: Option<String> }

async fn ws_events(ws: WebSocketUpgrade, Query(query): Query<WsAuthQuery>, State(state): State<AppState>) -> impl IntoResponse {
    if let Some(store)=state.token_store.clone() {
        match auth_ws_token(&store, query.token).await { Ok(()) => ws.on_upgrade(move|s|forward_events(s,state)).into_response(), Err(code)=>code.into_response() }
    } else { ws.on_upgrade(move|s|forward_events(s,state)).into_response() }
}

async fn auth_ws_token(store:&Arc<dyn TokenStore>, token:Option<String>)->Result<(),StatusCode>{
    let auth_enabled=std::env::var("JAMES_AUTH_ENABLED").ok().and_then(|v|v.parse().ok()).unwrap_or(true);
    let dev_mode=std::env::var("JAMES_AUTH_DEV_MODE").ok().and_then(|v|v.parse().ok()).unwrap_or(false);
    if !auth_enabled || dev_mode { return Ok(()); }
    if auth_enabled {
        let token=token.ok_or(StatusCode::UNAUTHORIZED)?;
        let info=store.validate(&token).await.ok_or(StatusCode::UNAUTHORIZED)?;
        if info.expires_at.map(|t|chrono::Utc::now()>t).unwrap_or(false){return Err(StatusCode::UNAUTHORIZED);}
    }
    Ok(())
}

async fn forward_events(socket: WebSocket, state: AppState) {
    let (mut sender,mut receiver)=socket.split();
    let mut rx=state.bus.subscribe_all();
    let task=tokio::spawn(async move {
        while let Some(event)=rx.recv().await {
            if sender.send(Message::Text(serde_json::to_string(&event).unwrap_or_default())).await.is_err(){break;}
        }
    });
    while receiver.next().await.is_some() {}
    task.abort();
}

async fn ws_void(ws:WebSocketUpgrade, Query(query):Query<WsAuthQuery>, State(state):State<AppState>)->impl IntoResponse{
    if let Some(store)=state.token_store.clone(){
        if let Err(code)=auth_ws_token(&store,query.token).await{return code.into_response();}
    }
    ws.on_upgrade(move|socket|void_socket(socket,state)).into_response()
}

async fn void_socket(socket:WebSocket,state:AppState){
    let (mut sender,mut receiver)=socket.split();
    let mut rx=state.bus.subscribe_all();
    let task=tokio::spawn(async move{
        while let Some(event)=rx.recv().await{
            if sender.send(Message::Text(serde_json::to_string(&event).unwrap_or_default())).await.is_err(){break;}
        }
    });
    let bus=state.bus.clone();
    while let Some(Ok(msg))=receiver.next().await{
        if let Message::Text(text)=msg{
            if let Ok(value)=serde_json::from_str::<serde_json::Value>(&text){
                let kind=value.get("type").and_then(|v|v.as_str()).unwrap_or("void.message");
                let content=value.get("content").and_then(|v|v.as_str()).or_else(||value.get("message").and_then(|v|v.as_str())).unwrap_or("");
                if kind=="void.message" && !content.is_empty(){
                    let _=bus.publish(james_events::Event::new("void.user_input","james-app")
                        .with_payload(serde_json::json!({"message_id":uuid::Uuid::now_v7().to_string(),"content":content,"timestamp":chrono::Utc::now()}))).await;
                }
            }
        }
    }
    task.abort();
}

pub fn router(state:AppState)->Router{
    Router::new()
        .route("/",get(serve_index))
        .route("/api/v1/health",get(health))
        .route("/api/v1/chat",post(chat_submit))
        .route("/api/v1/dashboard/status",get(dashboard_status))
        .route("/api/v1/dashboard/tasks",get(dashboard_tasks))
        .route("/api/v1/dashboard/memory",get(dashboard_memory))
        .route("/api/v1/data/:section",get(data_section))
        .route("/api/v1/ui/intent",get(ui_intent))
        .route("/api/v1/agent/intent",post(agent_execute_intent))
        .route("/ws/v1/events",get(ws_events))
        .route("/ws/v1/void",get(ws_void))
        .fallback_service(ServeDir::new(state.static_dir.clone()))
        .with_state(state)
}

pub async fn serve(listener:tokio::net::TcpListener,state:AppState)->anyhow::Result<()>{
    if state.preview_unauthenticated {
        warn!("JAMES API running in unauthenticated preview mode; keep binding loopback-only");
    }
    let app = if let Some(store) = state.token_store.clone() {
        create_router_with_auth(state, store)
    } else {
        router(state)
    };
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await?;
    Ok(())
}

pub fn preview_bind_allowed(bind:&str)->bool{matches!(bind,"127.0.0.1"|"localhost"|"::1")}

#[cfg(test)]
mod tests{
    use super::*;
    use tower::ServiceExt;
    async fn test_state()->AppState{
        let bus=Arc::new(EventBus::new(100)); bus.start().await.unwrap();
        AppState{bus,status:Arc::new(RwLock::new(CoreStatus::Running)),version:"test".into(),started_at:chrono::Utc::now(),
            static_dir:"missing".into(),dashboard:Arc::new(RwLock::new(DashboardSnapshot{core_status:"Running".into(),..Default::default()})),
            preview_unauthenticated:true,chat_handler:None,data:None,broker:None,executor:None,agent_handler:None,intent:None,token_store:None}
    }
    #[test] fn loopback_only(){assert!(preview_bind_allowed("127.0.0.1"));assert!(!preview_bind_allowed("0.0.0.0"));}
    #[tokio::test] async fn health_works(){
        let app=router(test_state().await);
        let res=app.oneshot(axum::http::Request::builder().uri("/api/v1/health").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(),StatusCode::OK);
    }
}
