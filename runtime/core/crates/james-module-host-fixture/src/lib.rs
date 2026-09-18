//! Test-only native module fixture.
//!
//! Exposes the JAMES native module ABI (see `james-module-host/src/abi.rs`)
//! so integration tests can verify real `dlopen`-based dynamic module loading.

use std::ffi::{c_char, CStr};
use std::sync::atomic::{AtomicI32, Ordering};

/// ABI version this fixture implements (must match the host's contract).
pub const JAMES_MODULE_ABI_VERSION: u32 = 1;

const OK: i32 = 0;

static INIT_COUNT: AtomicI32 = AtomicI32::new(0);
static STARTED: AtomicI32 = AtomicI32::new(0);
static LAST_MANIFEST_ID_LEN: AtomicI32 = AtomicI32::new(0);

/// Reports the ABI version implemented by this library.
#[no_mangle]
pub extern "C" fn james_module_abi_version() -> u32 {
    JAMES_MODULE_ABI_VERSION
}

/// Initializes the module with the manifest JSON.
#[no_mangle]
pub extern "C" fn james_module_init(manifest_json: *const c_char) -> i32 {
    if manifest_json.is_null() {
        return 1;
    }
    let raw = unsafe { CStr::from_ptr(manifest_json) };
    let Ok(text) = raw.to_str() else {
        return 2;
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(text) else {
        return 3;
    };
    if manifest.get("id").and_then(|v| v.as_str()).is_none() {
        return 4;
    }
    INIT_COUNT.fetch_add(1, Ordering::SeqCst);
    LAST_MANIFEST_ID_LEN.store(text.len() as i32, Ordering::SeqCst);
    OK
}

/// Starts the module runtime loop.
#[no_mangle]
pub extern "C" fn james_module_start() -> i32 {
    STARTED.store(1, Ordering::SeqCst);
    OK
}

/// Stops the module runtime loop.
#[no_mangle]
pub extern "C" fn james_module_stop() -> i32 {
    STARTED.store(0, Ordering::SeqCst);
    OK
}

/// Returns 1 if the module is healthy (started), 0 otherwise.
#[no_mangle]
pub extern "C" fn james_module_health_check() -> i32 {
    if STARTED.load(Ordering::SeqCst) == 1 {
        1
    } else {
        0
    }
}

/// Test accessors (not part of the ABI) for asserting state from the host side.
#[allow(dead_code)]
pub fn fixture_init_count() -> i32 {
    INIT_COUNT.load(Ordering::SeqCst)
}

#[allow(dead_code)]
pub fn fixture_last_manifest_len() -> i32 {
    LAST_MANIFEST_ID_LEN.load(Ordering::SeqCst)
}