//! JAMES Runtime gRPC Client

pub mod proto {
    tonic::include_proto!("james.runtime.v1");
}

use proto::{
    CompletionRequest,
    james_runtime_client::JamesRuntimeClient,
};
use tonic::transport::{Channel, ClientTlsConfig};
use anyhow::Result;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RuntimeClient {
    client: JamesRuntimeClient<Channel>,
}

impl RuntimeClient {
    pub async fn connect(addr: impl Into<String>) -> Result<Self> {
        let channel = Channel::from_shared(addr.into())?
            .timeout(Duration::from_secs(10))
            .connect()
            .await?;

        Ok(Self {
            client: JamesRuntimeClient::new(channel),
        })
    }

    pub async fn connect_with_tls(addr: impl Into<String>, ca_cert: Vec<u8>) -> Result<Self> {
        let tls = ClientTlsConfig::new()
            .ca_certificate(tonic::transport::Certificate::from_pem(ca_cert))
            .domain_name("localhost");

        let channel = Channel::from_shared(addr.into())?
            .timeout(Duration::from_secs(10))
            .tls_config(tls)?
            .connect()
            .await?;

        Ok(Self {
            client: JamesRuntimeClient::new(channel),
        })
    }

    // ============ Completion ============

    pub async fn complete(&mut self, request: CompletionRequest) -> Result<proto::CompletionResponse> {
        let response = self.client.complete(request).await?;
        Ok(response.into_inner())
    }

    pub async fn stream_complete(&mut self, request: CompletionRequest) -> Result<tokio::sync::mpsc::Receiver<proto::Chunk>> {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let mut client = self.client.clone();

        tokio::spawn(async move {
            match client.stream_complete(request).await {
                Ok(response) => {
                    let mut stream = response.into_inner();
                    while let Ok(Some(chunk)) = stream.message().await {
                        if tx.send(chunk).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Stream error: {}", e);
                }
            }
        });

        Ok(rx)
    }

    // ============ Routing ============

    pub async fn route_model(&mut self, request: proto::RoutingRequest) -> Result<proto::RoutingDecision> {
        let response = self.client.route_model(request).await?;
        Ok(response.into_inner())
    }

    pub async fn select_runtime(&mut self, request: proto::RuntimeRequest) -> Result<proto::RuntimeSelection> {
        let response = self.client.select_runtime(request).await?;
        Ok(response.into_inner())
    }

    // ============ Hardware & Models ============

    pub async fn get_hardware_profile(&mut self) -> Result<proto::HardwareProfile> {
        let response = self.client.get_hardware_profile(()).await?;
        Ok(response.into_inner())
    }

    pub async fn get_model_registry(&mut self) -> Result<proto::ModelRegistry> {
        let response = self.client.get_model_registry(()).await?;
        Ok(response.into_inner())
    }

    // ============ Cost & Budget ============

    pub async fn get_cost_report(&mut self) -> Result<proto::CostReport> {
        let response = self.client.get_cost_report(proto::CostReport::default()).await?;
        Ok(response.into_inner())
    }

    pub async fn update_budget(&mut self, policy: proto::BudgetPolicy) -> Result<proto::BudgetStatus> {
        let response = self.client.update_budget(policy).await?;
        Ok(response.into_inner())
    }

    // ============ Events ============

    pub async fn stream_events(&mut self, filter: proto::EventFilter) -> Result<tonic::Streaming<proto::RuntimeEvent>> {
        let response = self.client.stream_events(filter).await?;
        Ok(response.into_inner())
    }
}

// ============ Builder Patterns ============

impl proto::CompletionRequest {
    pub fn new(model: impl Into<String>, messages: Vec<proto::ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: 0.7,
            max_tokens: 4096,
            stream: false,
            tools: vec![],
            runtime_hint: String::new(),
        }
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = temp;
        self
    }

    pub fn with_max_tokens(mut self, tokens: i32) -> Self {
        self.max_tokens = tokens;
        self
    }

    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    pub fn with_tools(mut self, tools: Vec<proto::Tool>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_runtime_hint(mut self, hint: impl Into<String>) -> Self {
        self.runtime_hint = hint.into();
        self
    }
}

impl proto::RoutingRequest {
    pub fn new(task: impl Into<String>) -> Self {
        Self {
            task: task.into(),
            required_capabilities: vec![],
            quality_tier: proto::QualityTier::Balanced as i32,
            privacy: proto::PrivacyLevel::PreferLocal as i32,
            latency_budget_ms: 0,
            cost_budget_per_1k: 0.0,
            context_length_needed: 0,
        }
    }

    pub fn with_capability(mut self, cap: impl Into<String>) -> Self {
        self.required_capabilities.push(cap.into());
        self
    }

    pub fn with_quality_tier(mut self, tier: proto::QualityTier) -> Self {
        self.quality_tier = tier as i32;
        self
    }

    pub fn with_latency_budget(mut self, ms: i64) -> Self {
        self.latency_budget_ms = ms;
        self
    }

    pub fn with_cost_budget(mut self, budget: f64) -> Self {
        self.cost_budget_per_1k = budget;
        self
    }

    pub fn with_context_length(mut self, tokens: i32) -> Self {
        self.context_length_needed = tokens;
        self
    }
}

impl proto::RuntimeRequest {
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            quantization: String::new(),
            offload: false,
            max_latency_ms: 0,
            max_vram_gb: 0.0,
        }
    }

    pub fn with_quantization(mut self, q: impl Into<String>) -> Self {
        self.quantization = q.into();
        self
    }

    pub fn with_offload(mut self, enable: bool) -> Self {
        self.offload = enable;
        self
    }

    pub fn with_max_vram(mut self, gb: f64) -> Self {
        self.max_vram_gb = gb;
        self
    }
}

impl proto::BudgetPolicy {
    pub fn new(daily: f64, monthly: f64, max_single: f64) -> Self {
        Self {
            daily_ai_budget_usd: daily,
            monthly_ai_budget_usd: monthly,
            max_single_request_usd: max_single,
            approved_models: vec![],
            approved_providers: vec![],
        }
    }

    pub fn approve_model(mut self, model: impl Into<String>) -> Self {
        self.approved_models.push(model.into());
        self
    }

    pub fn approve_provider(mut self, provider: impl Into<String>) -> Self {
        self.approved_providers.push(provider.into());
        self
    }
}

impl proto::ChatMessage {
    pub fn text(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            name: String::new(),
            tool_calls: vec![],
            tool_call_id: String::new(),
        }
    }
}