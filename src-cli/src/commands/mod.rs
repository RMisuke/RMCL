//! Command handlers — one module per top-level command group.
//!
//! Every handler maps 1:1 to a backend capability on
//! [`crate::context::CliContext`], returns a structured `serde_json::Value`
//! (the JSON contract) and routes auxiliary text / progress / events through
//! [`crate::output::Output`]. Adding a new command = a new module here + a
//! variant in [`crate::app`] + a registry entry + a dispatch arm in
//! [`crate::dispatch`].

pub mod auth;
pub mod config;
pub mod launch;
pub mod modloader;
pub mod mods;
pub mod profile;
pub mod resource;
pub mod system;
pub mod version;
