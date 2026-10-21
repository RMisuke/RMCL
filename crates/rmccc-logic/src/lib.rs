//! Logic plugin runtime: ABI, wire protocol and the supervisor that owns the
//! helper process (Phase 5, ADR-007).
//!
//! The launcher never links a WASM engine. [`supervisor::LogicHost`] spawns
//! `rmccc-logic-host`, speaks line-delimited JSON over its stdio, enforces a
//! wall-clock timeout per call and kills/restarts the helper when it hangs or
//! crashes. Isolation therefore has two layers: process (kernel) and module
//! (wasmi memory/fuel/import limits inside the helper).

pub mod abi;
pub mod protocol;
pub mod runner;
pub mod supervisor;

pub use abi::{LogicLimits, LOGIC_API_VERSION};
pub use protocol::{LogicError, LogicErrorCode, Request, RequestParams, Response};
pub use runner::LogicRunner;
pub use supervisor::LogicHost;
