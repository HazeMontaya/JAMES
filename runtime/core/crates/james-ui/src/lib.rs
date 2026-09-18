//! JAMES UI Orchestrator.
//!
//! Per `docs/architecture/ui-orchestration.md` the UI does not decide itself
//! which fachliche goals, permissions or execution steps matter. A backend
//! orchestrator consumes the real JAMES state (events from the bus, task
//! transitions, security decisions, platform telemetry) and projects it into a
//! declarative, renderer-independent `UiIntent`:
//!
//! ```json
//! {
//!   "mode": "research",
//!   "focus": "sources",
//!   "brain_state": "searching",
//!   "activity": 0.6,
//!   "show": ["brain", "progress", "sources", "findings"],
//!   "open": ["research_panel"],
//!   "actions": ["open_source", "pause_task"]
//! }
//! ```
//!
//! Every renderer (Void desktop, dashboard, web, embedded) renders this intent
//! instead of interpreting raw events itself. The brain state is only derived
//! from real JAMES state/events - never from randomness.

use std::sync::Arc;
use tokio::sync::mpsc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use james_events::{EventBus, EventEnvelope, EventSeverity, Event};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;
use tracing::debug;

/// Declarative UI intent produced by the orchestrator and consumed by every
/// Void renderer. All motion/rendering decisions follow from this value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiIntent {
    /// e.g. "void" | "research" | "task" | "security" | "system"
    pub mode: String,
    /// The contextual focus inside the mode, e.g. "sources", "tasks", "core"
    pub focus: String,
    /// Brain state machine state (lowercase), see `brain_states()`.
    pub brain_state: String,
    /// Real-system activity 0..1 (resources, event pressure), never random.
    pub activity: f64,
    /// Sections the renderer should show.
    pub show: Vec<String>,
    /// Panels/tabs the renderer should open.
    pub open: Vec<String>,
    /// Exposed actions the user may take.
    pub actions: Vec<String>,
    /// Machine-readable priority (higher = more important).
    pub priority: u8,
    /// Human-readable German reason behind the intent.
    pub reason: String,
    /// When the intent was computed.
    pub at: DateTime<Utc>,
    /// Correlates the intent to the driving event.
    pub correlation_id: Option<Uuid>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub plan_id: Option<String>,
    #[serde(default)]
    pub step_id: Option<String>,
    #[serde(default)]
    pub capability_id: Option<String>,
    #[serde(default)]
    pub verification_status: Option<String>,
}

impl Default for UiIntent {
    fn default() -> Self {
        let mut s = Self::idle();
        s.at = Utc::now();
        s
    }
}

impl UiIntent {
    pub fn idle() -> Self {
        Self {
            mode: "void".into(),
            focus: "core".into(),
            brain_state: "idle".into(),
            activity: 0.0,
            show: vec!["brain".into()],
            open: vec![],
            actions: vec![],
            priority: 0,
            reason: "System bereit; wartet auf Aufgaben und Eingaben.".into(),
            at: Utc::now(),
            correlation_id: None,
            agent_id: None,
            plan_id: None,
            step_id: None,
            capability_id: None,
            verification_status: None,
        }
    }
}

/// All brain states the orchestrator may emit (visual contract:
/// docs/architecture/void-visual-system.md section 5).
pub fn brain_states() -> &'static [&'static str] {
    &[
        "idle",
        "listening",
        "understanding",
        "thinking",
        "planning",
        "searching",
        "executing",
        "verifying",
        "completed",
        "waiting",
        "learning",
        "warning",
        "error",
        "recovering",
        "sleeping",
        "offline",
        "resource_limited",
        "security_lock",
    ]
}

/// Internal state projection derived from the event stream.
#[derive(Debug, Clone, Default)]
struct Projection {
    active_tasks: usize,
    last_task_at: Option<DateTime<Utc>>,
    last_task_action: Option<String>,
    last_event_at: Option<DateTime<Utc>>,
    last_event_id: Option<Uuid>,
    last_error_at: Option<DateTime<Utc>>,
    last_error_type: Option<String>,
    last_warning_at: Option<DateTime<Utc>>,
    last_warning_type: Option<String>,
    last_security_at: Option<DateTime<Utc>>,
    last_security_type: Option<String>,
    security_decision: Option<String>, // ALLOW / DENY / ASK
    last_input_at: Option<DateTime<Utc>>,
    last_explore_at: Option<DateTime<Utc>>,
    last_learning_at: Option<DateTime<Utc>>,
    last_thinking_at: Option<DateTime<Utc>>,
    last_plan_at: Option<DateTime<Utc>>,
    agent_id: Option<String>,
    plan_id: Option<String>,
    step_id: Option<String>,
    capability_id: Option<String>,
    verification_status: Option<String>,
    cpu_usage: Option<f64>,
    ram_usage: Option<f64>,
    /// Events observed within the projection window.
    events_seen: u64,
    errors_seen: u64,
    warnings_seen: u64,
}

impl Projection {
    fn shift(&mut self, envelope: &EventEnvelope) {
        let ev = &envelope.event;
        let now = ev.timestamp;
        self.last_event_at = Some(now);
        self.last_event_id = Some(ev.event_id);
        self.events_seen += 1;

        match ev.severity {
            EventSeverity::Error | EventSeverity::Critical => {
                self.errors_seen += 1;
                self.last_error_at = Some(now);
                self.last_error_type = Some(ev.event_type.clone());
            }
            EventSeverity::Warn => {
                self.warnings_seen += 1;
                self.last_warning_at = Some(now);
                self.last_warning_type = Some(ev.event_type.clone());
            }
            _ => {}
        }

        let t = ev.event_type.as_str();
        if let Some(v) = ev.payload.get("agent_id").and_then(|v| v.as_str()) { self.agent_id = Some(v.to_string()); }
        if let Some(v) = ev.payload.get("plan_id").and_then(|v| v.as_str()) { self.plan_id = Some(v.to_string()); }
        if let Some(v) = ev.payload.get("step_id").and_then(|v| v.as_str()) { self.step_id = Some(v.to_string()); }
        if let Some(v) = ev.payload.get("capability_id").and_then(|v| v.as_str()) { self.capability_id = Some(v.to_string()); }

        if t.starts_with("agent.step.") {
            self.last_task_at = Some(now);
            self.last_task_action = Some(t.to_string());
            if t.ends_with("started") {
                self.active_tasks += 1;
                self.verification_status = Some("pending".into());
            } else if t.ends_with("completed") {
                self.active_tasks = self.active_tasks.saturating_sub(1);
                self.verification_status = Some("pending".into());
            } else if t.ends_with("failed") {
                self.active_tasks = self.active_tasks.saturating_sub(1);
                self.verification_status = Some("failed".into());
            } else if t.ends_with("retrying") {
                self.verification_status = Some("retrying".into());
            }
        } else if t.starts_with("task.") || t.starts_with("tools.") || t == "capability.run" || t == "capability.executed" {
            self.last_task_at = Some(now);
            self.last_task_action = Some(t.to_string());
            if t.contains("started") && !t.contains("another") {
                self.active_tasks += 1;
            } else if t.contains("completed") || t.contains("failed") || t.contains("cancelled") {
                self.active_tasks = self.active_tasks.saturating_sub(1);
            }
            self.last_plan_at = None; // executing/verifying replaces a plan pulse
        } else if t.starts_with("security.") || t.starts_with("approval.") || t.starts_with("permission.") {
            self.last_security_at = Some(now);
            self.last_security_type = Some(t.to_string());
            self.security_decision = extract_decision(&ev.payload);
        } else {
            match t {
                "void.user_input" | "input.text" | "input.voice" => {
                    self.last_input_at = Some(now);
                }
                "memory.read" | "memory.reads" | "memory.search" => {
                    self.last_learning_at = Some(now);
                }
                "memory.consolidated" | "learning.completed" | "module.learned" => {
                    self.last_learning_at = Some(now);
                }
                "ai.infer" | "ai.inference_started" | "thinking.start" | "agent.thinking" => {
                    self.last_thinking_at = Some(now);
                }
                "plan.created" | "agent.planning" | "workflow.planning" | "plan.updated" => {
                    self.last_plan_at = Some(now);
                }
                "search.started" | "webresearch.search" | "discovery.scan_started" => {
                    self.last_explore_at = Some(now);
                }
                "memory.stored" | "memory.store" | "memory.update" => {
                    self.last_learning_at = Some(now);
                }
                "platform.telemetry.snapshot" => {
                    self.last_event_at = Some(now);
                    self.observe_telemetry(&ev.payload);
                }
                _ => {}
            }
        }
    }

    fn observe_telemetry(&mut self, payload: &serde_json::Value) {
        let total = payload
            .pointer("/system/total_memory_bytes")
            .and_then(|v| v.as_u64());
        let free = payload
            .pointer("/system/free_memory_bytes")
            .and_then(|v| v.as_u64());
        if let (Some(total), Some(free)) = (total, free) {
            if total > 0 {
                let used = total.saturating_sub(free) as f64 / total as f64;
                self.ram_usage = Some((used * 100.0).round() / 100.0);
            }
        }
        if let Some(cpu) = payload.pointer("/system/cpu_usage") {
            self.cpu_usage = cpu.as_f64();
        }
    }
}

/// Correlate a derived UI intent with the latest causal event.
/// This keeps the renderer traceable to a real event instead of inventing an id.
fn last_correlation_id(p: &Projection, _brain: &str) -> Option<Uuid> {
    p.last_event_id
}

/// Extract an ALLOW/DENY/ASK decision from a security/approval payload.
fn extract_decision(payload: &serde_json::Value) -> Option<String> {
    let raw = payload
        .get("decision")
        .or_else(|| payload.get("result"))
        .or_else(|| payload.get("outcome"));
    raw.and_then(|v| v.as_str())
        .or_else(|| payload.get("policy").and_then(|v| v.as_str()))
        .map(|s| s.to_uppercase())
        .filter(|s| s == "ALLOW" || s == "DENY" || s == "ASK")
}

/// Which zones/sections to surface for a given intent focus.
fn sections_for(brain: &str) -> Vec<String> {
    match brain {
        "executing" | "verifying" => vec!["brain".into(), "progress".into(), "tasks".into()],
        "planning" => vec!["brain".into(), "plan".into(), "tasks".into()],
        "searching" => vec!["brain".into(), "progress".into(), "sources".into()],
        "learning" => vec!["brain".into(), "memory".into()],
        "warning" | "error" | "recovering" => vec!["brain".into(), "errors".into()],
        "security_lock" => vec!["brain".into(), "security".into()],
        "sleeping" => vec!["brain".into()],
        _ => vec!["brain".into()],
    }
}

/// Actions exposed by the intent.
fn actions_for(brain: &str) -> Vec<String> {
    match brain {
        "warning" | "error" | "recovering" => vec!["inspect_error".into()],
        "security_lock" => vec!["review_security".into()],
        "waiting" => vec!["resume_task".into()],
        _ => vec![],
    }
}

/// Compute the state machine priority for an ordering decision.
fn priority_for(brain: &str) -> u8 {
    match brain {
        "offline" => 90,
        "error" => 85,
        "warning" => 75,
        "security_lock" => 80,
        "recovering" => 70,
        "executing" => 60,
        "verifying" => 55,
        "planning" => 50,
        "searching" => 45,
        "learning" => 40,
        "thinking" => 35,
        "understanding" => 33,
        "listening" => 30,
        "waiting" => 25,
        "resource_limited" => 20,
        "completed" => 15,
        "sleeping" => 10,
        _ => 5,
    }
}

/// Time since an optional timestamp, saturating at zero (never negative).
fn since_secs(now: DateTime<Utc>, t: Option<DateTime<Utc>>) -> chrono::Duration {
    match t {
        Some(t) if t <= now => now.signed_duration_since(t),
        Some(_) => chrono::Duration::zero(),
        None => chrono::Duration::MAX,
    }
}

/// Project the current derived state into a declarative intent.
///
/// Derivation is causal:
/// - tasks/agents/capabilities drive executing/planning/searching/verifying
/// - inputs drive listening/understanding
/// - errors/warnings/security drive their own states
/// - resources may raise activity but never alone claim a cognitive state
///   (visual contract section 7).
fn project(p: &Projection, now: DateTime<Utc>) -> UiIntent {
    let since = |t: Option<DateTime<Utc>>, window: chrono::Duration| -> bool {
        since_secs(now, t) < window
    };

    // Real activity mix: resources above an idling baseline add intensity, but
    // only events may change the brain state itself.
    let activity = {
        let base = if p.events_seen > 0 { 0.15 } else { 0.0 };
        let ram = p.ram_usage.unwrap_or(0.0);
        let mem = (ram - 0.25).max(0.0) * 0.6;
        (base + mem).clamp(0.0, 1.0)
    };

    // Event pressure window; a busy burst raises activity even at moderate load.
    let burst = if since(p.last_event_at, chrono::Duration::seconds(15)) { 0.25 } else { 0.0 };
    let activity = (activity + burst).min(1.0);

    let decide = build_decider(p, now);
    let brain_state = decide();
    let priority = priority_for(&brain_state);
    let focus = focus_for(&brain_state, p);
    let mode = mode_for(&brain_state, p);
    let reason = reason_for(&brain_state, p, now);
    let correlation_id = if brain_state == "idle" { None } else { last_correlation_id(p, &brain_state) };
    let show = sections_for(&brain_state);
    let open = open_for(&brain_state, p);
    let actions = actions_for(&brain_state);

    UiIntent {
        mode,
        focus,
        brain_state,
        activity,
        show,
        open,
        actions,
        priority,
        reason,
        at: now,
        correlation_id,
        agent_id: p.agent_id.clone(),
        plan_id: p.plan_id.clone(),
        step_id: p.step_id.clone(),
        capability_id: p.capability_id.clone(),
        verification_status: p.verification_status.clone(),
    }
}

fn mode_for(brain: &str, p: &Projection) -> String {
    match brain {
        "security_lock" => "security".into(),
        "searching" => "research".into(),
        "executing" | "verifying" | "planning" | "completed" => "task".into(),
        "warning" | "error" | "recovering" | "resource_limited" => "system".into(),
        _ => {
            if p.last_task_at.is_some() { "task".into() } else { "void".into() }
        }
    }
}

fn focus_for(brain: &str, p: &Projection) -> String {
    match brain {
        "security_lock" => "security".into(),
        "searching" => "sources".into(),
        "planning" => "plan".into(),
        "executing" | "verifying" => "tasks".into(),
        "learning" => "memory".into(),
        "listening" | "understanding" => "input".into(),
        "warning" | "error" | "recovering" => "errors".into(),
        "resource_limited" => "resources".into(),
        "sleeping" | "idle" => "core".into(),
        _ => p.last_task_action.clone().unwrap_or_else(|| "core".into()),
    }
}

fn reason_for(brain: &str, p: &Projection, now: DateTime<Utc>) -> String {
    let since = |t: Option<DateTime<Utc>>, window: chrono::Duration| -> bool {
        since_secs(now, t) < window
    };
    match brain {
        "error" => format!(
            "Ein Fehler ist erfasst ({}). Zustand wird stabilisiert.",
            p.last_error_type.clone().unwrap_or_else(|| "unbekannt".into())
        ),
        "warning" => format!(
            "Auffälligkeit erfasst ({}). Bordmechanismen prüfen die Lage.",
            p.last_warning_type.clone().unwrap_or_else(|| "unbekannt".into())
        ),
        "security_lock" => match p.security_decision.as_deref() {
            Some("ALLOW") => "Policy-Entscheidung: ALLOW. Freigabe ist erteilt und auditiert.".into(),
            Some("DENY") => "Policy-Entscheidung: DENY. Aktion wurde abgelehnt.".into(),
            Some("ASK") => "Policy-Entscheidung: ASK. Freigabe ist erforderlich.".into(),
            _ => "Sicherheitsentscheidung erwartet; kontrollierter Ring aktiv.".into(),
        },
        "recovering" => "JAMES stellt den konsistenten Zustand wieder her.".into(),
        "executing" => {
            if since(p.last_task_at, chrono::Duration::seconds(60)) {
                "Aktionen werden über den Capability-Broker ausgeführt und geprüft.".into()
            } else {
                "Laufende Arbeit wird fortgesetzt.".into()
            }
        }
        "verifying" => "Ergebnisse kehren zurück und werden eingespeist.".into(),
        "planning" => "Ein Plan wird in Schritte zerlegt und belegt.".into(),
        "searching" => "Quellen werden nach relevanten Fakten durchsucht.".into(),
        "learning" => "Neues Wissen wird gespeichert und konsolidiert.".into(),
        "thinking" => "Der Kern arbeitet still an einer Anfrage.".into(),
        "understanding" => "Eingaben werden eingeordnet.".into(),
        "listening" => "JAMES hört auf Eingaben.".into(),
        "waiting" => "Auf Eingabe oder Freigabe wird gewartet.".into(),
        "completed" => "Letzte Aufgabe ist abgeschlossen.".into(),
        "resource_limited" => {
            let cpu = p.cpu_usage.map(|c| format!("{:.0}% CPU", c * 100.0)).unwrap_or_default();
            let ram = p.ram_usage.map(|r| format!("{:.0}% RAM", r * 100.0)).unwrap_or_default();
            format!("Ressourcenlage eng: {cpu} {ram}. Aktivität wird gezügelt.")
        }
        "sleeping" => "System liegt im Standby; der Kern bleibt wach.".into(),
        "offline" => "Keine Verbindung zu JAMES.".into(),
        _ => "System bereit; wartet auf Aufgaben und Eingaben.".into(),
    }
}

fn open_for(brain: &str, p: &Projection) -> Vec<String> {
    if p.last_security_at.is_some() && brain == "security_lock" {
        return vec!["security_panel".into()];
    }
    match brain {
        "warning" | "error" | "recovering" => vec!["error_feed".into()],
        "searching" => vec!["sources_panel".into()],
        "planning" => vec!["plan_panel".into()],
        _ => vec![],
    }
}

// Build the ordered decision: highest-priority causal state wins.
fn build_decider<'a>(p: &'a Projection, now: DateTime<Utc>) -> Box<dyn Fn() -> String + 'a> {
    Box::new(move || {
        let since = |t: Option<DateTime<Utc>>, window: chrono::Duration| -> bool {
            since_secs(now, t) < window
        };
        if since(p.last_error_at, chrono::Duration::seconds(60)) {
            return "error".into();
        }
        if since(p.last_security_at, chrono::Duration::seconds(90)) {
            return "security_lock".into();
        }
        if since(p.last_warning_at, chrono::Duration::seconds(45)) {
            return "warning".into();
        }
        // A running task is the strongest causal signal for activity.
        if p.active_tasks > 0 && since(p.last_task_at, chrono::Duration::seconds(120)) {
            let action = p.last_task_action.as_deref().unwrap_or("");
            if action.contains("completed") || action.contains("done") {
                return "verifying".into();
            }
            return "executing".into();
        }
        if since(p.last_task_at, chrono::Duration::seconds(20)) {
            let action = p.last_task_action.as_deref().unwrap_or("");
            if action.contains("plan") { return "planning".into(); }
            if action.contains("search") { return "searching".into(); }
            if action.contains("completed") || action.contains("done") {
                return "verifying".into();
            }
            return "executing".into();
        }
        if since(p.last_plan_at, chrono::Duration::seconds(30)) {
            return "planning".into();
        }
        if since(p.last_explore_at, chrono::Duration::seconds(30)) {
            return "searching".into();
        }
        if since(p.last_learning_at, chrono::Duration::seconds(30)) {
            return "learning".into();
        }
        if since(p.last_thinking_at, chrono::Duration::seconds(30)) {
            return "thinking".into();
        }
        if since(p.last_input_at, chrono::Duration::seconds(8)) {
            return "listening".into();
        }
        if since(p.last_event_at, chrono::Duration::seconds(60)) {
            return "idle".into();
        }
        // No recent events at all: keep the fabric calm.
        "idle".into()
    })
}

/// Orchestrator: consumes the event stream, keeps a causal projection and
/// publishes `ui.intent` snapshots. Live state lives in the bus; the
/// orchestrator never fabricates activity.
pub struct UiOrchestrator {
    bus: Arc<EventBus>,
    /// Events undelivered to the (possibly not yet spawned) worker. Taking the
    /// receiver only when the loop starts would drop events published between
    /// construction and spawn. `tokio::sync::Mutex` keeps the receiver `Sync`.
    inbox: tokio::sync::Mutex<Option<mpsc::Receiver<EventEnvelope>>>,
    projection: RwLock<Projection>,
    latest: RwLock<UiIntent>,
}

impl UiOrchestrator {
    pub fn new(bus: Arc<EventBus>) -> Self {
        let inbox = bus.subscribe_all();
        Self {
            bus,
            inbox: tokio::sync::Mutex::new(Some(inbox)),
            projection: RwLock::new(Projection::default()),
            latest: RwLock::new(UiIntent::idle()),
        }
    }

    /// Spawn the orchestrator loop: fold events into the projection, then
    /// publish an intent whenever the observable state changes.
    pub fn spawn(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let this = self.clone();
        tokio::spawn(async move {
            let mut rx = this
                .inbox
                .lock()
                .await
                .take()
                .expect("orchestrator spawned twice");
            debug!("james-ui: orchestrator subscribed to event bus");
            loop {
                match rx.recv().await {
                    Some(envelope) => {
                        this.projection.write().await.shift(&envelope);
                        let intent = {
                            let proj = this.projection.read().await;
                            project(&proj, Utc::now())
                        };
                        let changed = {
                            let last = this.latest.read().await;
                            last.brain_state != intent.brain_state
                                || last.activity != intent.activity
                                || last.focus != intent.focus
                                || last.open != intent.open
                                || last.agent_id != intent.agent_id
                                || last.plan_id != intent.plan_id
                                || last.step_id != intent.step_id
                                || last.capability_id != intent.capability_id
                                || last.verification_status != intent.verification_status
                        };
                        let mut latest = this.latest.write().await;
                        *latest = intent.clone();
                        drop(latest);
                        if changed {
                            let _ = this.publish(&intent).await;
                        }
                    }
                    None => {
                        debug!("james-ui: event stream closed, orchestrator exits");
                        break;
                    }
                }
            }
        })
    }

    async fn publish(&self, intent: &UiIntent) -> Result<()> {
        let mut ev = Event::new("ui.intent", "james-ui").with_payload(serde_json::to_value(intent)?);
        if let Some(cid) = intent.correlation_id {
            ev = ev.with_correlation_id(cid);
        }
        self.bus.publish(ev).await?;
        Ok(())
    }

    /// Latest computed intent (for REST + polling renderers).
    pub async fn latest(&self) -> UiIntent {
        self.latest.read().await.clone()
    }

    /// Manually project state (used by tests, React server renderers, etc.).
    pub async fn project_current(&self) -> UiIntent {
        let proj = self.projection.read().await;
        project(&proj, Utc::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_events::Event;

    fn envelope(ev: Event) -> EventEnvelope {
        EventEnvelope::new(ev)
    }

    fn at(ev: &mut Event, offset_secs: i64) -> Event {
        ev.timestamp = Utc::now() + chrono::Duration::seconds(offset_secs);
        ev.clone()
    }

    #[test]
    fn states_cover_visual_contract() {
        let states = brain_states();
        for required in [
            "idle", "listening", "understanding", "thinking", "planning",
            "searching", "executing", "verifying", "completed", "waiting",
            "learning", "warning", "error", "recovering", "sleeping",
            "offline", "resource_limited", "security_lock",
        ] {
            assert!(states.contains(&required), "missing brain state {required}");
        }
    }

    #[test]
    fn idle_default_is_calm() {
        let i = UiIntent::default();
        assert_eq!(i.brain_state, "idle");
        assert_eq!(i.priority, 0);
        assert!(i.activity <= 0.16);
    }

    #[tokio::test]
    async fn error_event_projects_error_intent() {
        let bus = Arc::new(EventBus::new(1024));
        bus.start().await.unwrap();
        let orch = Arc::new(UiOrchestrator::new(bus.clone()));
        let mut ev = Event::new("error.occurred", "test")
            .with_severity(EventSeverity::Error)
            .with_payload(serde_json::json!({ "error": "boom" }));
        let ev = at(&mut ev, 0);
        orch.projection.write().await.shift(&envelope(ev));
        let intent = orch.project_current().await;
        assert_eq!(intent.brain_state, "error");
        assert!(intent.priority >= 80);
    }

    #[tokio::test]
    async fn task_started_projects_executing() {
        let bus = Arc::new(EventBus::new(1024));
        bus.start().await.unwrap();
        let orch = Arc::new(UiOrchestrator::new(bus.clone()));
        let mut ev = Event::new("task.started", "test");
        let ev = at(&mut ev, 0);
        orch.projection.write().await.shift(&envelope(ev));
        let intent = orch.project_current().await;
        assert_eq!(intent.brain_state, "executing");
        assert!(intent.show.contains(&"tasks".to_string()));
    }

    #[tokio::test]
    async fn security_event_projects_lock() {
        let bus = Arc::new(EventBus::new(1024));
        bus.start().await.unwrap();
        let orch = Arc::new(UiOrchestrator::new(bus.clone()));
        let mut ev = Event::new("approval.requested", "test")
            .with_payload(serde_json::json!({ "decision": "ASK" }));
        let ev = at(&mut ev, 0);
        orch.projection.write().await.shift(&envelope(ev));
        let intent = orch.project_current().await;
        assert_eq!(intent.brain_state, "security_lock");
        assert_eq!(intent.mode, "security");
        assert!(intent.open.contains(&"security_panel".to_string()));
    }

    #[tokio::test]
    async fn telemetry_raises_activity_but_not_state() {
        let bus = Arc::new(EventBus::new(1024));
        bus.start().await.unwrap();
        let orch = Arc::new(UiOrchestrator::new(bus.clone()));
        let mut ev = Event::new("platform.telemetry.snapshot", "james-platform")
            .with_payload(serde_json::json!({
                "system": { "total_memory_bytes": 16000000000_u64, "free_memory_bytes": 1000000000_u64 },
            }));
        let ev = at(&mut ev, 0);
        orch.projection.write().await.shift(&envelope(ev.clone()));
        orch.projection.write().await.shift(&envelope(ev));
        let intent = orch.project_current().await;
        // High memory pressure lifts activity but cannot claim a cognitive state.
        assert!(intent.activity > 0.3);
        assert_eq!(intent.brain_state, "idle");
    }

    #[tokio::test]
    async fn spawn_publishes_ui_intent_events() {
        let bus = Arc::new(EventBus::new(1024));
        bus.start().await.unwrap();
        let orch = Arc::new(UiOrchestrator::new(bus.clone()));
        let mut rx = bus.subscribe("ui.intent");
        orch.spawn();

        let mut started = Event::new("task.started", "test");
        started.timestamp = Utc::now();
        bus.publish(started).await.unwrap();

        let received = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("ui.intent must be published")
            .expect("channel open");
        assert_eq!(received.event.event_type, "ui.intent");
        let intent: UiIntent = serde_json::from_value(received.event.payload).unwrap();
        assert_eq!(intent.brain_state, "executing");
    }

    #[tokio::test]
    async fn denial_decision_drives_reason() {
        let bus = Arc::new(EventBus::new(1024));
        bus.start().await.unwrap();
        let orch = Arc::new(UiOrchestrator::new(bus.clone()));
        let mut ev = Event::new("approval.decided", "test")
            .with_payload(serde_json::json!({ "decision": "DENY" }));
        let ev = at(&mut ev, 0);
        orch.projection.write().await.shift(&envelope(ev));
        let intent = orch.project_current().await;
        assert_eq!(intent.brain_state, "security_lock");
        assert!(intent.reason.contains("DENY"));
    }
}