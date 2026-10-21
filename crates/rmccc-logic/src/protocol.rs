//! Line-delimited JSON-RPC between the launcher and the logic helper (ADR-007).
//!
//! One JSON object per line in both directions. The launcher is the only client;
//! the helper never initiates a message, so the protocol stays request/response
//! and a stuck call is always attributable to a request id.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::abi::{LogicLimits, LOGIC_API_VERSION};

/// Failure codes the helper (and the supervisor) may return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogicErrorCode {
    /// Malformed line, unknown field, bad id.
    BadRequest,
    /// `v` does not match [`LOGIC_API_VERSION`].
    ProtocolVersion,
    /// The helper does not implement the method.
    UnknownMethod,
    /// `invoke` before `load`, or the plugin id does not match.
    NotLoaded,
    /// A second `load` for the same id (one instance per helper in v1).
    AlreadyLoaded,
    /// The module imports something outside the allowlist.
    ImportNotAllowed,
    /// The module exceeds [`crate::abi::MAX_MODULE_BYTES`].
    ModuleTooLarge,
    /// The module failed to compile or instantiate.
    ModuleInvalid,
    /// The module ran out of fuel.
    FuelExhausted,
    /// The module trapped (unreachable, OOB memory, stack exhaustion, …).
    Trap,
    /// The call exceeded its wall-clock budget (reported by the supervisor).
    Timeout,
    /// Request/response payload over the limit.
    PayloadTooLarge,
    /// Anything the helper did not classify.
    Internal,
}

impl LogicErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BadRequest => "bad_request",
            Self::ProtocolVersion => "protocol_version",
            Self::UnknownMethod => "unknown_method",
            Self::NotLoaded => "not_loaded",
            Self::AlreadyLoaded => "already_loaded",
            Self::ImportNotAllowed => "import_not_allowed",
            Self::ModuleTooLarge => "module_too_large",
            Self::ModuleInvalid => "module_invalid",
            Self::FuelExhausted => "fuel_exhausted",
            Self::Trap => "trap",
            Self::Timeout => "timeout",
            Self::PayloadTooLarge => "payload_too_large",
            Self::Internal => "internal",
        }
    }
}

/// Error body shared by every response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicError {
    pub code: LogicErrorCode,
    pub message: String,
}

impl LogicError {
    pub fn new(code: LogicErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for LogicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for LogicError {}

/// Request payloads (the `params` of a [`Request`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "method",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RequestParams {
    /// Liveness/version handshake.
    Ping,
    /// Compile and instantiate `logic/<file>.wasm` from the installed plugin.
    Load {
        plugin_id: String,
        /// Absolute path to the `.wasm` (the launcher resolves it, the helper
        /// never walks a directory).
        path: String,
        #[serde(default)]
        limits: LogicLimits,
    },
    /// Call the loaded module.
    ///
    /// The field is `call` (not `method`) because `method` is the serde tag of
    /// this enum; the module receives it as `{"method": <call>, "input": …}`.
    Invoke {
        plugin_id: String,
        /// Logical operation name, passed through to the module.
        call: String,
        #[serde(default)]
        input: Value,
        #[serde(default)]
        limits: LogicLimits,
    },
    /// Drop the module and free its memory.
    Unload { plugin_id: String },
}

/// Keys a request line may carry. `serde(flatten)` and `deny_unknown_fields`
/// cannot be combined, so [`Request::parse`] enforces the strict shape itself.
const ALLOWED_REQUEST_KEYS: &[&str] = &[
    "v", "id", "method", "pluginId", "path", "limits", "call", "input",
];

/// One request line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub v: u32,
    pub id: u64,
    #[serde(flatten)]
    pub params: RequestParams,
}

impl Request {
    pub fn new(id: u64, params: RequestParams) -> Self {
        Self {
            v: LOGIC_API_VERSION,
            id,
            params,
        }
    }

    /// Serialize as one line (no trailing newline; the writer adds it).
    pub fn to_line(&self) -> Result<String, LogicError> {
        serde_json::to_string(self)
            .map_err(|e| LogicError::new(LogicErrorCode::Internal, e.to_string()))
    }

    pub fn parse(line: &str) -> Result<Self, LogicError> {
        let value: Value = serde_json::from_str(line)
            .map_err(|e| LogicError::new(LogicErrorCode::BadRequest, e.to_string()))?;
        let object = value.as_object().ok_or_else(|| {
            LogicError::new(LogicErrorCode::BadRequest, "request must be a JSON object")
        })?;
        for key in object.keys() {
            if !ALLOWED_REQUEST_KEYS.contains(&key.as_str()) {
                return Err(LogicError::new(
                    LogicErrorCode::BadRequest,
                    format!("unexpected request field `{key}`"),
                ));
            }
        }
        serde_json::from_value(value)
            .map_err(|e| LogicError::new(LogicErrorCode::BadRequest, e.to_string()))
    }
}

/// One response line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Response {
    pub v: u32,
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<LogicError>,
}

impl Response {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            v: LOGIC_API_VERSION,
            id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, error: LogicError) -> Self {
        Self {
            v: LOGIC_API_VERSION,
            id,
            ok: false,
            result: None,
            error: Some(error),
        }
    }

    pub fn parse(line: &str) -> Result<Self, LogicError> {
        let response: Self = serde_json::from_str(line)
            .map_err(|e| LogicError::new(LogicErrorCode::BadRequest, e.to_string()))?;
        if response.v != LOGIC_API_VERSION {
            return Err(LogicError::new(
                LogicErrorCode::ProtocolVersion,
                format!(
                    "helper speaks logic API v{} but the launcher expects v{LOGIC_API_VERSION}",
                    response.v
                ),
            ));
        }
        Ok(response)
    }

    pub fn into_result(self, expected_id: u64) -> Result<Value, LogicError> {
        if self.id != expected_id {
            return Err(LogicError::new(
                LogicErrorCode::Internal,
                format!(
                    "response id {} does not match request {expected_id}",
                    self.id
                ),
            ));
        }
        if self.ok {
            Ok(self.result.unwrap_or(Value::Null))
        } else {
            Err(self.error.unwrap_or_else(|| {
                LogicError::new(
                    LogicErrorCode::Internal,
                    "helper returned ok=false without an error",
                )
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_round_trips_as_one_line() {
        let request = Request::new(7, RequestParams::Ping);
        let line = request.to_line().unwrap();
        assert!(!line.contains('\n'));
        assert_eq!(Request::parse(&line).unwrap(), request);
    }

    #[test]
    fn load_request_carries_limits_and_path() {
        let limits = LogicLimits {
            fuel: 1234,
            ..LogicLimits::default()
        };
        let request = Request::new(
            1,
            RequestParams::Load {
                plugin_id: "dev.example.logic".into(),
                path: "/plugins/dev.example.logic/1.0.0/logic/plugin.wasm".into(),
                limits,
            },
        );
        let line = request.to_line().unwrap();
        assert!(line.contains("\"method\":\"load\""), "{line}");
        let parsed = Request::parse(&line).unwrap();
        assert_eq!(parsed, request);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let err = Request::parse(r#"{"v":1,"id":1,"method":"ping","evil":true}"#).unwrap_err();
        assert_eq!(err.code, LogicErrorCode::BadRequest);
        assert!(err.message.contains("evil"), "{err}");
        // Unknown *method* is a distinct, also-strict failure.
        let err = Request::parse(r#"{"v":1,"id":1,"method":"melt"},"#.trim_end_matches(',').trim_end().trim_end_matches(',').to_string().as_str()).unwrap_err();
        assert_eq!(err.code, LogicErrorCode::BadRequest);
    }

    #[test]
    fn wrong_protocol_version_is_rejected_on_response() {
        let err = Response::parse(r#"{"v":9,"id":1,"ok":true,"result":null}"#).unwrap_err();
        assert_eq!(err.code, LogicErrorCode::ProtocolVersion);
    }

    #[test]
    fn response_id_must_match_the_request() {
        let response = Response::ok(2, serde_json::json!({"x": 1}));
        let err = response.into_result(1).unwrap_err();
        assert_eq!(err.code, LogicErrorCode::Internal);
    }

    #[test]
    fn ok_false_without_error_is_an_internal_error() {
        let response = Response {
            v: LOGIC_API_VERSION,
            id: 1,
            ok: false,
            result: None,
            error: None,
        };
        assert_eq!(
            response.into_result(1).unwrap_err().code,
            LogicErrorCode::Internal
        );
    }

    #[test]
    fn error_codes_have_stable_strings() {
        assert_eq!(
            LogicErrorCode::ImportNotAllowed.as_str(),
            "import_not_allowed"
        );
        assert_eq!(LogicErrorCode::FuelExhausted.as_str(), "fuel_exhausted");
        let json = serde_json::to_string(&LogicErrorCode::Timeout).unwrap();
        assert_eq!(json, "\"timeout\"");
    }

    #[test]
    fn limits_are_clamped_to_a_sane_range() {
        let clamped = LogicLimits {
            timeout_ms: 0,
            memory_bytes: 1,
            fuel: 0,
            max_concurrency: 8,
        }
        .clamped();
        assert_eq!(clamped.timeout_ms, 100);
        assert_eq!(clamped.memory_bytes, 1024 * 1024);
        assert_eq!(clamped.fuel, 10_000);
        assert_eq!(clamped.max_concurrency, 1, "v1 runs one call at a time");
    }
}
