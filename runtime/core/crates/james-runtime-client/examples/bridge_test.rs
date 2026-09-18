//! End-to-end bridge test: Rust client -> Python gRPC sidecar
use james_runtime_client::{proto, RuntimeClient};
use std::collections::HashSet;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = "http://127.0.0.1:38243";
    let mut client = RuntimeClient::connect(addr).await?;
    println!("[OK] connected to {addr}");

    // 1. Hardware profile
    let hw = client.get_hardware_profile().await?;
    println!(
        "[HW] gpu={} vram={:.1}GB cuda={:?} cores={} ram={:.1}GB",
        hw.gpu_name, hw.vram_gb, hw.cuda_version, hw.cpu_cores, hw.ram_gb
    );

    // 2. Model registry
    let reg = client.get_model_registry().await?;
    println!("[MODELS] count={}", reg.models.len());
    for m in reg.models.iter().take(3) {
        println!(
            "  - {} ({}B, {} params) tier={} q={}",
            m.id, m.parameters_b, m.provider, m.quality_tier, m.quantization
        );
    }

    // 3. Model routing
    let route = client
        .route_model(
            proto::RoutingRequest::new("text_generation")
                .with_capability("reasoning")
                .with_context_length(2048),
        )
        .await?;
    println!(
        "[ROUTING] model={} provider={} reason={}",
        route.model_id, route.provider, route.reasoning
    );

    // 4. Runtime selection
    let sel = client
        .select_runtime(proto::RuntimeRequest::new("llama-3.1-8b-instruct"))
        .await?;
    println!(
        "[RUNTIME] engine={} vram={:.1}GB fallbacks={:?}",
        sel.engine_type, sel.estimated_vram_gb, sel.fallback_engines
    );

    // 5. Streaming completion
    println!("[STREAM] calling stream_complete ...");
    let req = proto::CompletionRequest::new(
        "llama-3.1-8b-instruct",
        vec![proto::ChatMessage::text("user", "Say hello in one short sentence.")],
    )
    .with_temperature(0.7)
    .with_max_tokens(32);

    let mut rx = client.stream_complete(req).await?;
    let mut text = String::new();
    while let Some(chunk) = rx.recv().await {
        if let Some(choice) = chunk.choices.first() {
            if let Some(content) = choice.message.as_ref().map(|m| &m.content) {
                text.push_str(content);
                print!("{content}");
            }
        }
    }
    println!();
    println!("[STREAM-END] text={text:?}");

    // 6. Unary completion
    let req = proto::CompletionRequest::new(
        "llama-3.1-8b-instruct",
        vec![proto::ChatMessage::text("user", "What is 2+2? Answer with only the number.")],
    )
    .with_max_tokens(8);
    let resp = client.complete(req).await?;
    let content = resp
        .choices
        .first()
        .and_then(|c| c.message.as_ref())
        .map(|m| m.content.clone())
        .unwrap_or_default();
    println!("[COMPLETE] model={} answer={content:?}", resp.model);

    // 7. Event streaming (events emitted while a completion runs)
    println!("[EVENTS] subscribing + triggering completion ...");
    let mut evt = client
        .stream_events(proto::EventFilter::default())
        .await?;
    let req = proto::CompletionRequest::new(
        "llama-3.1-8b-instruct",
        vec![proto::ChatMessage::text("user", "Say ok")],
    )
    .with_max_tokens(4)
    .with_temperature(0.0);
    let _ = client.complete(req).await?;
    let mut count = 0u32;
    let mut seen_types: HashSet<String> = HashSet::new();
    loop {
        match evt.message().await {
            Ok(Some(ev)) => {
                count += 1;
                if seen_types.insert(ev.event_type.clone()) {
                    println!("  EVENT {} ts={}", ev.event_type, ev.timestamp);
                }
                if count >= 40 {
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }
    println!("[EVENTS] received {count} events, types={:?}", seen_types);

    // 8. Cost report + budget update
    let cost = client.get_cost_report().await?;
    println!(
        "[COST] total={:.6} models={} engines={} agents={}",
        cost.total_cost_usd,
        cost.cost_by_model.len(),
        cost.cost_by_engine.len(),
        cost.cost_by_agent.len()
    );
    let budget = client
        .update_budget(
            proto::BudgetPolicy::new(1.0, 30.0, 0.05)
                .approve_model("llama-3.1-8b-instruct")
                .approve_model("llama-3.3-70b-instruct"),
        )
        .await?;
    println!(
        "[BUDGET] daily_spent={:.4} daily_limit={:.2} monthly_spent={:.4} monthly_limit={:.2} daily_exceeded={}",
        budget.daily_spent, budget.daily_limit, budget.monthly_spent, budget.monthly_limit, budget.daily_exceeded
    );

    println!("[ALL OK] bridge test passed");
    Ok(())
}