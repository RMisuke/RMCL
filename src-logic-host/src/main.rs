//! Logic plugin host process (Phase 5, ADR-007).
//!
//! Speaks line-delimited JSON on stdin/stdout with the launcher. It owns the
//! only WASM engine in the product, so the launcher binary stays small; a crash
//! or an infinite loop here is contained by the supervisor (which kills and
//! respawns this process).
//!
//! Security posture:
//!
//! - modules may import **only** `rmccc.log` (checked before instantiation);
//! - no WASI, no filesystem, no network, no clocks;
//! - linear memory is capped by `StoreLimits`, instruction count by fuel;
//! - the payload is copied in/out of linear memory with explicit bounds checks.

use std::io::{BufRead, Write};

use rmccc_logic::abi::{
    EXPORT_ALLOC, EXPORT_INVOKE, HOST_IMPORTS, HOST_IMPORT_MODULE, LOGIC_API_VERSION,
    MAX_MODULE_BYTES, MAX_PAYLOAD_BYTES,
};
use rmccc_logic::protocol::{LogicError, LogicErrorCode, Request, RequestParams, Response};
use serde_json::{json, Value};
use wasmi::{
    Caller, Engine, Instance, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder,
};

/// Store data: the plugin id (for logs) plus the limits wasmi enforces.
struct HostState {
    plugin_id: String,
    limits: StoreLimits,
}

/// A successfully compiled and instantiated module.
struct Loaded {
    plugin_id: String,
    store: Store<HostState>,
    instance: Instance,
    memory: Memory,
    alloc: wasmi::TypedFunc<i32, i32>,
    invoke: wasmi::TypedFunc<(i32, i32), i64>,
}

fn main() {
    let mut subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_target(false);
    if std::env::var("RMCCC_LOGIC_LOG").is_ok() {
        subscriber = subscriber.with_max_level(tracing::Level::DEBUG);
    } else {
        subscriber = subscriber.with_max_level(tracing::Level::WARN);
    }
    let _ = subscriber.try_init();

    tracing::debug!(version = LOGIC_API_VERSION, "logic host starting");

    // Fuel metering is opt-in in wasmi; without it `Store::set_fuel` fails and
    // an infinite loop inside a module could only be stopped by killing us.
    let mut config = wasmi::Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut loaded: Option<Loaded> = None;

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let response = match Request::parse(&line) {
            Ok(request) => dispatch(&engine, &mut loaded, request),
            Err(error) => Response::err(0, error),
        };
        let Ok(text) = serde_json::to_string(&response) else {
            tracing::error!("could not serialize response");
            continue;
        };
        if writeln!(stdout, "{text}").is_err() || stdout.flush().is_err() {
            break;
        }
    }

    tracing::debug!("logic host exiting");
}

fn dispatch(engine: &Engine, loaded: &mut Option<Loaded>, request: Request) -> Response {
    let id = request.id;
    if request.v != LOGIC_API_VERSION {
        return Response::err(
            id,
            LogicError::new(
                LogicErrorCode::ProtocolVersion,
                format!(
                    "request v{} does not match logic API v{LOGIC_API_VERSION}",
                    request.v
                ),
            ),
        );
    }

    match request.params {
        RequestParams::Ping => Response::ok(
            id,
            json!({ "logicApiVersion": LOGIC_API_VERSION, "engine": "wasmi" }),
        ),
        RequestParams::Load {
            plugin_id,
            path,
            limits,
        } => match load(engine, &plugin_id, &path, limits) {
            Ok(new_loaded) => {
                *loaded = Some(new_loaded);
                Response::ok(id, json!({ "pluginId": plugin_id }))
            }
            Err(error) => Response::err(id, error),
        },
        RequestParams::Invoke {
            plugin_id,
            call,
            input,
            limits,
        } => {
            let Some(current) = loaded.as_mut() else {
                return Response::err(
                    id,
                    LogicError::new(LogicErrorCode::NotLoaded, "no module loaded"),
                );
            };
            if current.plugin_id != plugin_id {
                return Response::err(
                    id,
                    LogicError::new(
                        LogicErrorCode::NotLoaded,
                        format!("loaded plugin is {}", current.plugin_id),
                    ),
                );
            }
            match invoke(current, &call, &input, limits) {
                Ok(value) => Response::ok(id, value),
                Err(error) => Response::err(id, error),
            }
        }
        RequestParams::Unload { plugin_id } => match loaded.take() {
            Some(current) if current.plugin_id == plugin_id => {
                Response::ok(id, json!({ "unloaded": plugin_id }))
            }
            Some(current) => {
                *loaded = Some(current);
                Response::err(
                    id,
                    LogicError::new(
                        LogicErrorCode::NotLoaded,
                        format!("loaded plugin is not {plugin_id}"),
                    ),
                )
            }
            None => Response::ok(id, json!({ "unloaded": plugin_id })),
        },
    }
}

fn load(
    engine: &Engine,
    plugin_id: &str,
    path: &str,
    limits: rmccc_logic::abi::LogicLimits,
) -> Result<Loaded, LogicError> {
    let limits = limits.clamped();
    let metadata = std::fs::metadata(path).map_err(|e| {
        LogicError::new(
            LogicErrorCode::BadRequest,
            format!("cannot stat {path}: {e}"),
        )
    })?;
    if metadata.len() > MAX_MODULE_BYTES {
        return Err(LogicError::new(
            LogicErrorCode::ModuleTooLarge,
            format!("{} bytes exceeds {MAX_MODULE_BYTES}", metadata.len()),
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| {
        LogicError::new(
            LogicErrorCode::BadRequest,
            format!("cannot read {path}: {e}"),
        )
    })?;

    let module = Module::new(engine, bytes.as_slice()).map_err(|e| {
        LogicError::new(
            LogicErrorCode::ModuleInvalid,
            format!("compile failed: {e}"),
        )
    })?;
    check_imports(&module)?;

    let mut store = Store::new(
        engine,
        HostState {
            plugin_id: plugin_id.to_string(),
            limits: StoreLimitsBuilder::new()
                .memory_size(limits.memory_bytes as usize)
                .build(),
        },
    );
    store.limiter(|state| &mut state.limits);
    store
        .set_fuel(limits.fuel)
        .map_err(|e| LogicError::new(LogicErrorCode::Internal, format!("fuel: {e}")))?;

    let mut linker: Linker<HostState> = Linker::new(engine);
    linker
        .func_wrap(
            HOST_IMPORT_MODULE,
            "log",
            |mut caller: Caller<'_, HostState>, level: i32, ptr: i32, len: i32| {
                let message = read_memory(&mut caller, ptr, len).unwrap_or_default();
                match level {
                    1 => tracing::error!(plugin = %caller.data().plugin_id, "{message}"),
                    2 => tracing::warn!(plugin = %caller.data().plugin_id, "{message}"),
                    4 => tracing::debug!(plugin = %caller.data().plugin_id, "{message}"),
                    _ => tracing::info!(plugin = %caller.data().plugin_id, "{message}"),
                }
            },
        )
        .map_err(|e| LogicError::new(LogicErrorCode::Internal, format!("link: {e}")))?;

    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|e| LogicError::new(LogicErrorCode::ModuleInvalid, format!("instantiate: {e}")))?
        .start(&mut store)
        .map_err(|e| LogicError::new(LogicErrorCode::Trap, format!("start: {e}")))?;

    let memory = instance.get_memory(&store, "memory").ok_or_else(|| {
        LogicError::new(
            LogicErrorCode::ModuleInvalid,
            "module does not export `memory`",
        )
    })?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&store, EXPORT_ALLOC)
        .map_err(|e| {
            LogicError::new(
                LogicErrorCode::ModuleInvalid,
                format!("{EXPORT_ALLOC}: {e}"),
            )
        })?;
    let invoke = instance
        .get_typed_func::<(i32, i32), i64>(&store, EXPORT_INVOKE)
        .map_err(|e| {
            LogicError::new(
                LogicErrorCode::ModuleInvalid,
                format!("{EXPORT_INVOKE}: {e}"),
            )
        })?;

    Ok(Loaded {
        plugin_id: plugin_id.to_string(),
        store,
        instance,
        memory,
        alloc,
        invoke,
    })
}

/// Reject any import outside the allowlist before the module can run.
fn check_imports(module: &Module) -> Result<(), LogicError> {
    for import in module.imports() {
        let module_name = import.module();
        let field = import.name();
        let allowed = module_name == HOST_IMPORT_MODULE && HOST_IMPORTS.contains(&field);
        if !allowed {
            return Err(LogicError::new(
                LogicErrorCode::ImportNotAllowed,
                format!("module imports `{module_name}`.`{field}`, which is not allowed"),
            ));
        }
    }
    Ok(())
}

fn invoke(
    loaded: &mut Loaded,
    method: &str,
    input: &Value,
    limits: rmccc_logic::abi::LogicLimits,
) -> Result<Value, LogicError> {
    let limits = limits.clamped();
    let payload = serde_json::to_vec(&json!({
        "method": method,
        "input": input,
    }))
    .map_err(|e| LogicError::new(LogicErrorCode::Internal, e.to_string()))?;
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(LogicError::new(
            LogicErrorCode::PayloadTooLarge,
            format!("{} bytes exceeds {MAX_PAYLOAD_BYTES}", payload.len()),
        ));
    }

    // Fresh fuel per call: a hot loop cannot accumulate budget across calls.
    loaded
        .store
        .set_fuel(limits.fuel)
        .map_err(|e| LogicError::new(LogicErrorCode::Internal, format!("fuel: {e}")))?;

    let in_ptr = loaded
        .alloc
        .call(&mut loaded.store, payload.len() as i32)
        .map_err(|e| classify_trap(&loaded.plugin_id, e))?;
    write_memory(&mut loaded.store, &loaded.memory, in_ptr, &payload)?;

    let packed = loaded
        .invoke
        .call(&mut loaded.store, (in_ptr, payload.len() as i32))
        .map_err(|e| classify_trap(&loaded.plugin_id, e))?;
    let out_ptr = (packed >> 32) as i32;
    let out_len = (packed & 0xffff_ffff) as i32;
    if out_len < 0 || out_len as usize > MAX_PAYLOAD_BYTES {
        return Err(LogicError::new(
            LogicErrorCode::PayloadTooLarge,
            format!("module returned {out_len} bytes"),
        ));
    }
    let bytes = read_memory_store(&loaded.store, &loaded.memory, out_ptr, out_len)?;
    let _ = loaded.instance; // kept alive for the next call

    serde_json::from_slice(&bytes).map_err(|e| {
        LogicError::new(
            LogicErrorCode::Internal,
            format!("module reply is not JSON: {e}"),
        )
    })
}

fn classify_trap(plugin_id: &str, error: wasmi::Error) -> LogicError {
    let text = error.to_string();
    // Fuel exhaustion is the only trap we can classify reliably from wasmi's
    // message; everything else (unreachable, OOB, stack exhaustion, memory
    // limit) is a plain trap and is reported as such.
    let code = if text.contains("fuel") {
        LogicErrorCode::FuelExhausted
    } else {
        LogicErrorCode::Trap
    };
    LogicError::new(code, format!("{plugin_id}: {text}"))
}

fn write_memory(
    store: &mut Store<HostState>,
    memory: &Memory,
    ptr: i32,
    bytes: &[u8],
) -> Result<(), LogicError> {
    let start = ptr as usize;
    let end = start
        .checked_add(bytes.len())
        .ok_or_else(|| LogicError::new(LogicErrorCode::Trap, "pointer overflow"))?;
    let data = memory.data_mut(store);
    let Some(slot) = data.get_mut(start..end) else {
        return Err(LogicError::new(
            LogicErrorCode::Trap,
            format!("write out of bounds: {start}..{end} of {}", data.len()),
        ));
    };
    slot.copy_from_slice(bytes);
    Ok(())
}

fn read_memory_store(
    store: &Store<HostState>,
    memory: &Memory,
    ptr: i32,
    len: i32,
) -> Result<Vec<u8>, LogicError> {
    let start = ptr as usize;
    let end = start
        .checked_add(len.max(0) as usize)
        .ok_or_else(|| LogicError::new(LogicErrorCode::Trap, "pointer overflow"))?;
    let data = memory.data(store);
    let Some(slice) = data.get(start..end) else {
        return Err(LogicError::new(
            LogicErrorCode::Trap,
            format!("read out of bounds: {start}..{end} of {}", data.len()),
        ));
    };
    Ok(slice.to_vec())
}

fn read_memory(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> Option<String> {
    let memory = caller.get_export("memory")?.into_memory()?;
    let start = ptr as usize;
    let end = start.checked_add(len.max(0) as usize)?;
    let data = memory.data(&*caller);
    let slice = data.get(start..end)?;
    Some(String::from_utf8_lossy(slice).into_owned())
}
