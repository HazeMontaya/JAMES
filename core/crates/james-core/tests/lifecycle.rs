//! F1-05: L3 integration — full lifecycle through the public facade:
//! new -> subscribe -> start -> SYSTEM_STARTED observed -> stop.
//! Uses tight intervals so the test finishes in milliseconds and never
//! touches the real user profile more than health's default path does.

use james_core::{CoreConfig, CoreStatus, JamesCore};
use james_events::builtin_events;

#[tokio::test]
async fn test_core_startup_event_shutdown() {
    let mut config = CoreConfig::default();
    config.health_check_interval_secs = 3600;
    config.scheduler_tick_interval_secs = 3600;

    let mut core = JamesCore::new(config).await.unwrap();
    assert_eq!(core.status().await, CoreStatus::Starting);

    let mut rx = core
        .event_bus()
        .subscribe(builtin_events::SYSTEM_STARTED);

    core.start().await.unwrap();
    assert_eq!(core.status().await, CoreStatus::Running);

    let envelope = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .expect("SYSTEM_STARTED timeout")
        .expect("event channel closed");
    assert_eq!(envelope.event.event_type, builtin_events::SYSTEM_STARTED);

    core.stop().await.unwrap();
    assert_eq!(core.status().await, CoreStatus::Stopped);
}
