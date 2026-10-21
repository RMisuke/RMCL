//! WASM ABI of a logic plugin (ADR-007).
//!
//! A logic plugin is a **core** WASM module (not a Component Model component):
//! the host only needs a handful of functions, and the component toolchain costs
//! more than the whole budget allows.
//!
//! ```text
//! (import "rmccc" "log" (func (param i32 i32 i32)))          ;; the ONLY allowed import
//! (func (export "rmccc_alloc") (param i32) (result i32))      ;; len -> ptr
//! (func (export "rmccc_invoke") (param i32 i32) (result i64)) ;; in_ptr, in_len -> (out_ptr << 32 | out_len)
//! ```
//!
//! The host writes the request JSON into the module's memory, calls
//! `rmccc_invoke`, and reads the reply from the returned region. No other import
//! is reachable from the module: anything outside [`HOST_IMPORTS`] is rejected
//! **before** the module is instantiated.

/// Logic API major version. Independent of the app version and the UI API.
pub const LOGIC_API_VERSION: u32 = 1;

/// Import module namespace a plugin may reference.
pub const HOST_IMPORT_MODULE: &str = "rmccc";

/// The complete set of allowed imports in v1.
///
/// `log(level, ptr, len)` — level is `1=error 2=warn 3=info 4=debug`. Anything
/// else (filesystem, network, clocks, env) is a load-time rejection.
pub const HOST_IMPORTS: &[&str] = &["log"];

/// Export used to reserve `len` bytes inside the module's linear memory.
pub const EXPORT_ALLOC: &str = "rmccc_alloc";

/// Export invoked for every plugin call.
pub const EXPORT_INVOKE: &str = "rmccc_invoke";

/// Largest `.wasm` a plugin may ship (a logic plugin is a query helper, not a game).
pub const MAX_MODULE_BYTES: u64 = 2 * 1024 * 1024;

/// Largest request/response payload the host moves through linear memory.
pub const MAX_PAYLOAD_BYTES: usize = 256 * 1024;

/// Resource limits applied to one module instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogicLimits {
    /// Hard wall-clock limit for one `invoke` (host kills the helper on expiry).
    pub timeout_ms: u64,
    /// Linear-memory cap enforced by wasmi's `StoreLimits`.
    pub memory_bytes: u64,
    /// Wasmi fuel cap (deterministic instruction budget).
    pub fuel: u64,
    /// Maximum concurrent calls for the plugin (v1: always 1).
    pub max_concurrency: u8,
}

impl Default for LogicLimits {
    fn default() -> Self {
        Self {
            timeout_ms: 5_000,
            memory_bytes: 16 * 1024 * 1024,
            fuel: 50_000_000,
            max_concurrency: 1,
        }
    }
}

impl LogicLimits {
    /// Upper bounds the host will accept from a manifest/UI.
    pub fn clamped(mut self) -> Self {
        self.timeout_ms = self.timeout_ms.clamp(100, 30_000);
        self.memory_bytes = self.memory_bytes.clamp(1024 * 1024, 256 * 1024 * 1024);
        self.fuel = self.fuel.clamp(10_000, 2_000_000_000);
        self.max_concurrency = 1;
        self
    }
}
