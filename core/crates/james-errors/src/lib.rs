//! JAMES error taxonomy: stable, greppable error codes.
//!
//! Format: `JAMES-<AREA>-<NNN>`, e.g. `JAMES-TASK-004`.
//! Every code carries severity, retryability, user visibility and a
//! recovery hint. Producers SHOULD prefix user-facing error strings with
//! the code in brackets: `[JAMES-TASK-004] task exceeded its timeout`.
//! Full enum adoption per component happens in C2+; this crate is the
//! single source of truth both phases share.

use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ErrorArea {
    Core,
    Task,
    Tool,
    Model,
    Module,
    Security,
    Storage,
    Ui,
}

impl ErrorArea {
    pub fn prefix(&self) -> &'static str {
        match self {
            ErrorArea::Core => "CORE",
            ErrorArea::Task => "TASK",
            ErrorArea::Tool => "TOOL",
            ErrorArea::Model => "MODEL",
            ErrorArea::Module => "MODULE",
            ErrorArea::Security => "SECURITY",
            ErrorArea::Storage => "STORAGE",
            ErrorArea::Ui => "UI",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Wire format is the plain `"JAMES-AREA-NNN"` string (see manual
/// Serialize/Deserialize below): the catalog entries borrow static
/// recovery texts, which borrowed deserialization cannot express.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorCode {
    pub area: ErrorArea,
    pub number: u16,
    pub severity: ErrorSeverity,
    /// Whether retrying the same operation can plausibly succeed.
    pub retryable: bool,
    /// Whether the message may be shown to end users as-is.
    pub user_visible: bool,
    /// Short recovery hint shown alongside the message.
    pub recovery: &'static str,
}

impl Serialize for ErrorCode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.code())
    }
}

impl<'de> Deserialize<'de> for ErrorCode {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        lookup(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown error code: {s}")))
    }
}

impl ErrorCode {
    pub const fn new(
        area: ErrorArea,
        number: u16,
        severity: ErrorSeverity,
        retryable: bool,
        user_visible: bool,
        recovery: &'static str,
    ) -> Self {
        Self {
            area,
            number,
            severity,
            retryable,
            user_visible,
            recovery,
        }
    }

    pub fn code(&self) -> String {
        format!("JAMES-{}-{:03}", self.area.prefix(), self.number)
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())
    }
}

macro_rules! codes {
    ($(($area:ident, $num:expr, $sev:ident, $retry:expr, $vis:expr, $rec:expr, $name:ident)),* $(,)?) => {
        $(pub const $name: ErrorCode = ErrorCode::new(
            ErrorArea::$area, $num, ErrorSeverity::$sev, $retry, $vis, $rec,
        );)*

        /// All defined codes, in declaration order.
        pub const ALL: &[ErrorCode] = &[$($name),*];

        /// Look up a code by its `JAMES-AREA-NNN` string.
        pub fn lookup(code: &str) -> Option<ErrorCode> {
            ALL.iter().find(|c| c.code() == code).copied()
        }
    };
}

codes! {
    // ---- Core ----
    (Core, 1, High, false, true, "Fix the configuration file and restart.", CORE_INVALID_CONFIG),
    (Core, 2, High, false, true, "Restore a compatible config or upgrade JAMES.", CORE_MIGRATION_FAILED),
    (Core, 3, Medium, true, false, "Publish before shutdown completes; check lifecycle ordering.", CORE_BUS_STOPPED),
    (Core, 4, High, true, false, "Retry start; inspect component logs on repeat.", CORE_START_FAILED),
    (Core, 5, High, false, false, "Inspect shutdown logs; kill leftovers and restart.", CORE_STOP_FAILED),
    (Core, 6, Medium, true, false, "Increase timeout or reduce startup work.", CORE_STARTUP_TIMEOUT),
    (Core, 7, Critical, false, false, "Restart the process; file a bug with diagnostics output.", CORE_PANIC),
    (Core, 8, Medium, false, true, "Downgrade the file or upgrade JAMES first.", CORE_VERSION_TOO_NEW),
    // ---- Task ----
    (Task, 1, Low, false, true, "Use a valid task id from ListTasks.", TASK_NOT_FOUND),
    (Task, 2, Low, false, true, "Follow the Queued->Running->terminal state machine.", TASK_INVALID_TRANSITION),
    (Task, 3, Medium, false, true, "Remove the circular dependency and recreate the task.", TASK_DEPENDENCY_CYCLE),
    (Task, 4, Medium, true, true, "Retry with a larger timeout_secs.", TASK_TIMEOUT),
    (Task, 5, Medium, true, true, "Wait for capacity or raise max_concurrent_tasks.", TASK_QUEUE_FULL),
    (Task, 6, Medium, true, false, "Inspect the worker error; retry if transient.", TASK_WORKER_FAILED),
    (Task, 7, Low, false, true, "Only Queued/Running tasks can be cancelled.", TASK_CANCEL_REJECTED),
    (Task, 8, Low, false, true, "Only Failed tasks within max_retries can be retried.", TASK_RETRY_REJECTED),
    // ---- Tool ----
    (Tool, 1, Low, false, true, "Use a registered tool id from the registry.", TOOL_NOT_FOUND),
    (Tool, 2, High, false, true, "Request the capability or run with approval.", TOOL_PERMISSION_DENIED),
    (Tool, 3, Medium, false, true, "Adjust the request or obtain user approval.", TOOL_APPROVAL_DENIED),
    (Tool, 4, Medium, true, true, "Retry with a larger tool timeout.", TOOL_TIMEOUT),
    (Tool, 5, Medium, false, true, "Narrow the request so output fits the limit.", TOOL_OUTPUT_TOO_LARGE),
    (Tool, 6, Medium, true, false, "Inspect tool stderr/audit entry; retry if transient.", TOOL_EXECUTION_FAILED),
    (Tool, 7, Low, false, true, "Fix the input against the tool schema.", TOOL_INPUT_INVALID),
    (Tool, 8, High, false, false, "Kill the runaway execution; check sandbox limits.", TOOL_CANCELLED),
    // ---- Model ----
    (Model, 1, High, true, true, "Start the provider (e.g. ollama serve) and retry.", MODEL_PROVIDER_UNAVAILABLE),
    (Model, 2, High, false, true, "Fix credentials and retry.", MODEL_AUTH_FAILED),
    (Model, 3, Medium, true, true, "Back off and retry; lower request rate.", MODEL_RATE_LIMITED),
    (Model, 4, Medium, true, false, "Retry; escalate to a fallback provider on repeat.", MODEL_INVALID_RESPONSE),
    (Model, 5, Medium, false, true, "Shorten context or switch to a larger-context model.", MODEL_CONTEXT_TOO_LONG),
    (Model, 6, Medium, true, true, "Retry; prefer streaming for long generations.", MODEL_TIMEOUT),
    (Model, 7, Low, false, true, "Use a model with the required capability.", MODEL_CAPABILITY_MISSING),
    (Model, 8, Medium, true, false, "Retry with corrected parameters.", MODEL_REQUEST_INVALID),
    // ---- Module ----
    (Module, 1, High, false, true, "Fix manifest.toml against the schema.", MODULE_MANIFEST_INVALID),
    (Module, 2, High, false, true, "Install the named dependency module first.", MODULE_DEPENDENCY_MISSING),
    (Module, 3, High, false, false, "Inspect module logs; fix entry point and reload.", MODULE_LOAD_FAILED),
    (Module, 4, High, false, true, "Grant the permission or drop the capability.", MODULE_PERMISSION_DENIED),
    (Module, 5, Critical, false, false, "Module isolated as FAILED; core continues. Inspect logs.", MODULE_CRASHED),
    (Module, 6, Low, false, true, "Check name/version; list loaded modules.", MODULE_NOT_FOUND),
    // ---- Security ----
    (Security, 1, High, false, true, "Authenticate (bearer token) and retry.", SECURITY_UNAUTHENTICATED),
    (Security, 2, High, false, true, "Use an authorized identity or request access.", SECURITY_FORBIDDEN),
    (Security, 3, Medium, false, true, "Wait for user approval in the UI/CLI.", SECURITY_APPROVAL_REQUIRED),
    (Security, 4, Critical, false, false, "Rotate the exposed secret immediately; inspect audit log.", SECURITY_SECRET_LEAK),
    (Security, 5, Critical, false, false, "Stop writes; restore audit store from backup.", SECURITY_AUDIT_FAILURE),
    (Security, 6, High, false, false, "Denylisted path/target; adjust request scope.", SECURITY_POLICY_VIOLATION),
    // ---- Storage ----
    (Storage, 1, High, false, false, "Restore pre-migration backup and retry.", STORAGE_MIGRATION_FAILED),
    (Storage, 2, Critical, false, false, "Restore from backup; run integrity check.", STORAGE_CORRUPT),
    (Storage, 3, High, true, true, "Check disk/permissions; retry.", STORAGE_UNAVAILABLE),
    (Storage, 4, High, false, false, "Free space and retry the backup.", STORAGE_BACKUP_FAILED),
    (Storage, 5, Medium, true, false, "Retry the transaction.", STORAGE_TRANSACTION_FAILED),
    // ---- UI ----
    (Ui, 1, Medium, false, true, "Reconnect with a valid token.", UI_UNAUTHENTICATED),
    (Ui, 2, Medium, false, true, "Upgrade the UI to the supported contract version.", UI_CONTRACT_MISMATCH),
    (Ui, 3, Low, true, true, "Reconnect; state resyncs automatically.", UI_DISCONNECTED),
    (Ui, 4, Low, false, true, "Correct the command payload and retry.", UI_COMMAND_REJECTED),
}

/// A user-facing error carrying a stable code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JamesError {
    pub code: ErrorCode,
    pub message: String,
}

impl JamesError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for JamesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {} (recovery: {})", self.code, self.message, self.code.recovery)
    }
}

impl std::error::Error for JamesError {}

/// Prefix helper for anyhow-style strings: `[JAMES-AREA-NNN] message`.
pub fn tagged(code: ErrorCode, message: impl AsRef<str>) -> String {
    format!("[{}] {}", code, message.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_every_code_resolves_by_lookup() {
        assert!(!ALL.is_empty());
        for code in ALL {
            let s = code.code();
            assert!(s.starts_with("JAMES-"), "{s}");
            let found = lookup(&s).unwrap_or_else(|| panic!("{code} not found"));
            assert_eq!(&found, code);
        }
    }

    #[test]
    fn test_lookup_rejects_unknown() {
        assert!(lookup("JAMES-NOPE-999").is_none());
        assert!(lookup("garbage").is_none());
    }

    #[test]
    fn test_code_format_matches_contract() {
        assert_eq!(TASK_TIMEOUT.code(), "JAMES-TASK-004");
        assert_eq!(CORE_BUS_STOPPED.code(), "JAMES-CORE-003");
        assert_eq!(SECURITY_SECRET_LEAK.code(), "JAMES-SECURITY-004");
    }

    #[test]
    fn test_spot_check_severity_and_retry() {
        assert_eq!(TASK_TIMEOUT.severity, ErrorSeverity::Medium);
        assert!(TASK_TIMEOUT.retryable);
        assert!(TASK_TIMEOUT.user_visible);

        assert_eq!(SECURITY_SECRET_LEAK.severity, ErrorSeverity::Critical);
        assert!(!SECURITY_SECRET_LEAK.retryable);
        assert!(!SECURITY_SECRET_LEAK.user_visible);

        assert!(!MODULE_CRASHED.retryable);
        assert!(MODEL_PROVIDER_UNAVAILABLE.retryable);
    }

    #[test]
    fn test_wire_format_roundtrip() {
        let e = JamesError::new(MODEL_TIMEOUT, "timed out");
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("JAMES-MODEL-006"), "{json}");
        let back: JamesError = serde_json::from_str(&json).unwrap();
        assert_eq!(back.code, MODEL_TIMEOUT);
        assert_eq!(back.message, "timed out");
    }

    #[test]
    fn test_display_carries_recovery() {
        let e = JamesError::new(TASK_NOT_FOUND, "no such task");
        let s = e.to_string();
        assert!(s.contains("[JAMES-TASK-001]"));
        assert!(s.contains("no such task"));
        assert!(s.contains("recovery:"));
    }
}
