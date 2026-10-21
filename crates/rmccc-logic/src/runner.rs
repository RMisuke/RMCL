//! Per-plugin helper management.
//!
//! The launcher keeps one helper per plugin (not one global helper): a crash or
//! timeout in plugin A must not disturb plugin B's loaded module, and `cancel`
//! has an obvious target. Helpers are spawned lazily on the first call and
//! replaced on the next call after a kill.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::abi::LogicLimits;
use crate::protocol::{LogicError, LogicErrorCode};
use crate::supervisor::LogicHost;

/// One helper per plugin id.
pub struct LogicRunner {
    executable: Option<PathBuf>,
    hosts: HashMap<String, LogicHost>,
    limits: LogicLimits,
}

impl LogicRunner {
    /// Resolve the helper the same way [`LogicHost::spawn_default`] does.
    pub fn new() -> Self {
        Self {
            executable: crate::supervisor::resolve_helper().ok(),
            hosts: HashMap::new(),
            limits: LogicLimits::default(),
        }
    }

    /// Explicit helper path (tests, or a configured install location).
    pub fn with_helper(executable: PathBuf) -> Self {
        Self {
            executable: Some(executable),
            hosts: HashMap::new(),
            limits: LogicLimits::default(),
        }
    }

    /// Override the per-call limits for every plugin.
    pub fn set_limits(&mut self, limits: LogicLimits) {
        self.limits = limits.clamped();
    }

    /// Whether a helper for `plugin_id` is currently running.
    pub fn is_running(&mut self, plugin_id: &str) -> bool {
        self.hosts
            .get_mut(plugin_id)
            .map(|host| host.is_alive())
            .unwrap_or(false)
    }

    /// Run `call` inside the plugin's module, loading it on first use.
    ///
    /// `wasm_path` is resolved by the caller (the launcher knows the installed
    /// directory); the helper never walks a directory tree.
    pub fn invoke(
        &mut self,
        plugin_id: &str,
        wasm_path: &Path,
        call: &str,
        input: Value,
        limits: LogicLimits,
    ) -> Result<Value, LogicError> {
        let limits = limits.clamped();
        self.ensure_loaded(plugin_id, wasm_path, limits)?;
        let host = self.hosts.get_mut(plugin_id).ok_or_else(|| {
            LogicError::new(LogicErrorCode::Internal, "helper disappeared between calls")
        })?;
        let retry_input = input.clone();
        match host.invoke(plugin_id, call, input, limits) {
            Ok(value) => Ok(value),
            // The helper may have been restarted (crash/timeout) or the module
            // unloaded: reload once, then surface the second failure.
            Err(error) if error.code == LogicErrorCode::NotLoaded => {
                let fresh = LogicHost::spawn(self.executable()?.as_path())?;
                self.hosts.insert(plugin_id.to_string(), fresh);
                let host = self.hosts.get_mut(plugin_id).expect("just inserted");
                host.load(plugin_id, wasm_path, limits)?;
                host.invoke(plugin_id, call, retry_input, limits)
            }
            Err(error) => Err(error),
        }
    }

    /// Kill the plugin's helper (best-effort task cancellation).
    ///
    /// The in-flight call, if any, fails with `internal`; the next call respawns.
    pub fn cancel(&mut self, plugin_id: &str) -> bool {
        match self.hosts.remove(plugin_id) {
            Some(mut host) => {
                host.kill();
                true
            }
            None => false,
        }
    }

    /// Drop every helper (app shutdown, plugin disabled/uninstalled).
    pub fn shutdown(&mut self) {
        for (_, mut host) in self.hosts.drain() {
            host.kill();
        }
    }

    fn ensure_loaded(
        &mut self,
        plugin_id: &str,
        wasm_path: &Path,
        limits: LogicLimits,
    ) -> Result<(), LogicError> {
        if self.is_running(plugin_id) {
            return Ok(());
        }
        if let Some(mut host) = self.hosts.remove(plugin_id) {
            host.kill();
        }
        let exe = self.executable()?;
        let mut host = LogicHost::spawn(&exe)?;
        host.load(plugin_id, wasm_path, limits)?;
        self.hosts.insert(plugin_id.to_string(), host);
        Ok(())
    }

    fn executable(&self) -> Result<PathBuf, LogicError> {
        self.executable.clone().ok_or_else(|| {
            LogicError::new(
                LogicErrorCode::Internal,
                "logic helper not found (set RMCCC_LOGIC_HOST or install rmccc-logic-host)",
            )
        })
    }
}

impl Default for LogicRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LogicRunner {
    fn drop(&mut self) {
        self.shutdown();
    }
}
