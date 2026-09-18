//! JAMES API Authentication — bearer token middleware for REST + WebSocket.

use std::sync::Arc;
use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{header, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Router,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use uuid::Uuid;

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Enable authentication (true = required, false = dev mode)
    pub enabled: bool,
    /// Secret backend: "env" (JAMES_API_TOKEN) or "os" (Windows Credential Manager)
    pub backend: String,
    /// Token from environment (used when backend="env")
    #[serde(default)]
    pub token_env: String,
    /// Development mode: allow unauthenticated loopback with visible warning
    pub dev_mode: bool,
    /// Token TTL in seconds (0 = no expiry)
    pub token_ttl_secs: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            backend: "env".to_string(),
            token_env: "JAMES_API_TOKEN".to_string(),
            dev_mode: false,
            token_ttl_secs: 0,
        }
    }
}

/// Extracted authentication info from request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    /// Unique request identifier for correlation
    pub request_id: String,
    /// Authenticated caller identity
    pub identity: String,
    /// Granted scopes/permissions
    pub scopes: Vec<String>,
    /// Whether this request was authenticated (false in dev mode)
    pub authenticated: bool,
}

/// Token store interface
#[async_trait::async_trait]
pub trait TokenStore: Send + Sync {
    /// Validate a bearer token and return the associated identity + scopes
    async fn validate(&self, token: &str) -> Option<TokenInfo>;
    /// Get the configured token (for dev mode display)
    async fn get_token(&self) -> Option<String>;
}

/// Token validation result
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub identity: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Environment-based token store
pub struct EnvTokenStore {
    token: Option<String>,
    token_env: String,
}

impl EnvTokenStore {
    pub fn new(token_env: &str) -> Self {
        let token = std::env::var(token_env).ok().filter(|s| !s.is_empty());
        Self { token, token_env: token_env.to_string() }
    }
}

#[async_trait::async_trait]
impl TokenStore for EnvTokenStore {
    async fn validate(&self, token: &str) -> Option<TokenInfo> {
        self.token.as_ref().and_then(|t| {
            if t == token {
                Some(TokenInfo {
                    identity: "api-client".to_string(),
                    scopes: vec!["*".to_string()],
                    expires_at: None,
                })
            } else {
                None
            }
        })
    }

    async fn get_token(&self) -> Option<String> {
        self.token.clone()
    }
}

/// Windows Credential Manager token store (stub - implemented in platform crate)
#[cfg(windows)]
pub struct WindowsTokenStore {
    target_name: String,
}

#[cfg(windows)]
impl WindowsTokenStore {
    pub fn new(target_name: &str) -> Self {
        Self { target_name: target_name.to_string() }
    }
}

#[cfg(windows)]
#[async_trait::async_trait]
impl TokenStore for WindowsTokenStore {
    async fn validate(&self, _token: &str) -> Option<TokenInfo> {
        // TODO: Implement using windows-rs Credential Manager API
        None
    }

    async fn get_token(&self) -> Option<String> {
        None
    }
}

/// Authentication middleware for REST routes
pub async fn auth_middleware(
    State(store): State<Arc<dyn TokenStore>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let request_id = Uuid::now_v7().to_string();
    request.headers_mut().insert(
        "x-request-id",
        HeaderValue::from_str(&request_id).unwrap(),
    );

    // Check if auth is enabled via env var
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
            warn!(
                request_id = %request_id,
                remote = %addr,
                "AUTH DISABLED (dev mode) - request allowed without token"
            );
        }
        let ctx = AuthContext {
            request_id: request_id.clone(),
            identity: "anonymous".to_string(),
            scopes: vec!["*".to_string()],
            authenticated: false,
        };
        request.extensions_mut().insert(ctx);
        return Ok(next.run(request).await);
    }

    // Extract bearer token
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    let token = match auth_header {
        Some(t) if !t.is_empty() => t,
        _ => {
            if dev_mode && is_loopback(addr.ip()) {
                warn!(
                    request_id = %request_id,
                    remote = %addr,
                    "DEV MODE: Unauthenticated loopback request allowed (no bearer token)"
                );
                let ctx = AuthContext {
                    request_id: request_id.clone(),
                    identity: "dev-loopback".to_string(),
                    scopes: vec!["*".to_string()],
                    authenticated: false,
                };
                request.extensions_mut().insert(ctx);
                return Ok(next.run(request).await);
            }
            warn!(
                request_id = %request_id,
                remote = %addr,
                "Missing bearer token"
            );
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Validate token
    let token_info = match store.validate(token).await {
        Some(info) => info,
        None => {
            warn!(
                request_id = %request_id,
                remote = %addr,
                "Invalid token"
            );
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Check token expiry
    if let Some(expires) = token_info.expires_at {
        if chrono::Utc::now() > expires {
            warn!(
                request_id = %request_id,
                remote = %addr,
                identity = %token_info.identity,
                "Token expired"
            );
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    debug!(
        request_id = %request_id,
        remote = %addr,
        identity = %token_info.identity,
        "Authenticated request"
    );

    let ctx = AuthContext {
        request_id,
        identity: token_info.identity,
        scopes: token_info.scopes,
        authenticated: true,
    };
    request.extensions_mut().insert(ctx);

    Ok(next.run(request).await)
}

/// Extract auth context from request extensions
pub fn get_auth_context(request: &Request<Body>) -> Option<AuthContext> {
    request.extensions().get::<AuthContext>().cloned()
}

/// Check if IP is loopback
fn is_loopback(ip: std::net::IpAddr) -> bool {
    ip.is_loopback()
}

/// Create authenticated router with auth middleware applied to REST routes
pub fn create_router_with_auth(
    state: crate::AppState,
    store: Arc<dyn TokenStore>,
) -> Router {
    let protected = Router::new()
        .route("/api/v1/chat", axum::routing::post(crate::chat_submit))
        .route("/api/v1/dashboard/status", axum::routing::get(crate::dashboard_status))
        .route("/api/v1/dashboard/tasks", axum::routing::get(crate::dashboard_tasks))
        .route("/api/v1/dashboard/memory", axum::routing::get(crate::dashboard_memory))
        .route("/api/v1/data/:section", axum::routing::get(crate::data_section))
        .route("/api/v1/ui/intent", axum::routing::get(crate::ui_intent))
        .route("/api/v1/agent/intent", axum::routing::post(crate::agent_execute_intent))
        .layer(axum::middleware::from_fn_with_state(store.clone(), auth_middleware));

    // WebSocket routes - use existing handlers (they'll check auth inline)
    let ws_routes = Router::new()
        .route("/ws/v1/events", axum::routing::get(crate::ws_events))
        .route("/ws/v1/void", axum::routing::get(crate::ws_void));

    let public = Router::new()
        .route("/", axum::routing::get(crate::serve_index))
        .route("/api/v1/health", axum::routing::get(crate::health));

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(ws_routes)
        .fallback_service(tower_http::services::ServeDir::new(state.static_dir.clone()))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
    };
    use tower::ServiceExt;

    struct MockStore {
        valid_token: String,
    }

    #[async_trait::async_trait]
    impl TokenStore for MockStore {
        async fn validate(&self, token: &str) -> Option<TokenInfo> {
            if token == self.valid_token {
                Some(TokenInfo {
                    identity: "test-client".to_string(),
                    scopes: vec!["read".to_string(), "write".to_string()],
                    expires_at: None,
                })
            } else {
                None
            }
        }

        async fn get_token(&self) -> Option<String> {
            Some(self.valid_token.clone())
        }
    }

    // Simple test middleware that extracts token from header
    async fn test_auth_middleware(
        State(store): State<Arc<MockStore>>,
        mut request: Request<Body>,
        next: Next,
    ) -> Result<Response, StatusCode> {
        let request_id = Uuid::now_v7().to_string();
        request.headers_mut().insert(
            "x-request-id",
            HeaderValue::from_str(&request_id).unwrap(),
        );

        let auth_header = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "));

        let token = match auth_header {
            Some(t) if !t.is_empty() => t,
            _ => {
                return Err(StatusCode::UNAUTHORIZED);
            }
        };

        let token_info = match store.validate(token).await {
            Some(info) => info,
            None => return Err(StatusCode::UNAUTHORIZED),
        };

        let ctx = AuthContext {
            request_id,
            identity: token_info.identity,
            scopes: token_info.scopes,
            authenticated: true,
        };
        request.extensions_mut().insert(ctx);

        Ok(next.run(request).await)
    }

    #[tokio::test]
    async fn test_env_token_store() {
        std::env::set_var("TEST_TOKEN", "secret123");
        let store = EnvTokenStore::new("TEST_TOKEN");
        let info = store.validate("secret123").await;
        assert!(info.is_some());
        assert_eq!(info.unwrap().identity, "api-client");
        let info = store.validate("wrong").await;
        assert!(info.is_none());
    }

    #[tokio::test]
    async fn test_auth_middleware_allows_valid_token() {
        let store = Arc::new(MockStore {
            valid_token: "valid-token".to_string(),
        });

        let app = Router::new()
            .route("/test", axum::routing::get(|| async { "OK" }))
            .layer(axum::middleware::from_fn_with_state(store, test_auth_middleware));

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(header::AUTHORIZATION, "Bearer valid-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_middleware_rejects_invalid_token() {
        let store = Arc::new(MockStore {
            valid_token: "valid-token".to_string(),
        });

        let app = Router::new()
            .route("/test", axum::routing::get(|| async { "OK" }))
            .layer(axum::middleware::from_fn_with_state(store, test_auth_middleware));

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(header::AUTHORIZATION, "Bearer wrong-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_middleware_rejects_missing_token() {
        let store = Arc::new(MockStore {
            valid_token: "valid-token".to_string(),
        });

        let app = Router::new()
            .route("/test", axum::routing::get(|| async { "OK" }))
            .layer(axum::middleware::from_fn_with_state(store, test_auth_middleware));

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }
}