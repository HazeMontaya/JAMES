//! Native module ABI contract.
//!
//! A JAMES native module is a shared library (`.dll` / `.so` / `.dylib`)
//! exporting the C ABI below. Symbols are resolved lazily; only
//! [`JAMES_MODULE_ABI_VERSION`] and `james_module_init` are required.

use std::ffi::{c_char, CString};
use std::os::raw;
use thiserror::Error;

/// The ABI version understood by this host.
pub const JAMES_MODULE_ABI_VERSION: u32 = 1;

/// The ABI version a loaded library must report.
pub const SUPPORTED_ABI_VERSIONS: &[u32] = &[1];

/// Native module functions resolved from the dynamic library.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeModuleAbi {
    /// Initializer. Required. `(manifest_json) -> status`.
    pub init: Option<NativeInitFn>,
    /// Start the module. `() -> status`.
    pub start: Option<NativeLifecycleFn>,
    /// Stop the module. `() -> status`.
    pub stop: Option<NativeLifecycleFn>,
    /// Health check. `() -> 1 if healthy else 0`.
    pub health: Option<NativeLifecycleFn>,
}

/// `james_module_init(const char* manifest_json) -> i32`
pub type NativeInitFn = unsafe extern "C" fn(manifest_json: *const raw::c_char) -> i32;

/// `james_module_start()/stop()/health_check() -> i32`
pub type NativeLifecycleFn = unsafe extern "C" fn() -> i32;

/// Errors produced while resolving or invoking the native ABI.
#[derive(Debug, Error)]
pub enum AbiError {
    #[error("library does not export james_module_abi_version")]
    MissingVersion,
    #[error("unsupported ABI version {0} (host expects {JAMES_MODULE_ABI_VERSION})")]
    UnsupportedVersion(u32),
    #[error("library does not export james_module_init")]
    MissingInit,
    #[error("james_module_init failed with status {0}")]
    InitFailed(i32),
    #[error("james_module_start failed with status {0}")]
    StartFailed(i32),
    #[error("james_module_stop failed with status {0}")]
    StopFailed(i32),
    #[error("manifest is not UTF-8")]
    InvalidManifestEncoding,
}

/// CString helper that keeps the manifest JSON alive across the FFI call.
pub struct ManifestCString(pub CString);

impl ManifestCString {
    pub fn new(manifest_json: &str) -> Result<Self, AbiError> {
        CString::new(manifest_json)
            .map(Self)
            .map_err(|_| AbiError::InvalidManifestEncoding)
    }

    pub fn as_ptr(&self) -> *const c_char {
        self.0.as_ptr()
    }
}