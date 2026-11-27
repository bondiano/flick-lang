//! JSON configuration and structured result serialization for `flick-vm-wasm`.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use flick_vm::image::{LoadReport, UnloadReport};
use flick_vm::sandbox::{Capability, CapabilitySet, ResourceLimits, SandboxConfig};
use flick_vm::session::{EvalOutcome, StepOutcome};
use flick_vm::stdlib::{HostFetchRequest, HostFetchResponse};
use flick_vm::tier0::{HostMode, TestCaseResult};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::host::platform_wall_clock;

/// JSON-deserializable session configuration accepted by `VmSession`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionConfigJson {
    /// Preset policy: `"untrusted"` (default: no external caps, sandbox limits)
    /// or `"unrestricted"` (all capabilities, unlimited resources).
    #[serde(default)]
    pub preset: Option<String>,
    /// Explicit capability allowlist (e.g. `["clock", "random", "net"]`).
    #[serde(default)]
    pub capabilities: Option<Vec<String>>,
    /// Host service mode: `"live"` (default) or `"deterministic"`.
    #[serde(default)]
    pub host_mode: Option<String>,
    /// Optional per-evaluation fuel ceiling.
    #[serde(default)]
    pub fuel: Option<u64>,
    /// Optional live counted-heap byte limit.
    #[serde(default)]
    pub heap_bytes: Option<usize>,
    /// Optional wall-clock deadline in milliseconds (enforced via JS `performance.now()` on `wasm32`).
    #[serde(default)]
    pub wall_time_ms: Option<u64>,
    /// Optional maximum call-frame stack depth.
    #[serde(default)]
    pub max_stack_depth: Option<usize>,
    /// Optional CLI arguments exposed through `Host.Arguments.args`.
    #[serde(default)]
    pub program_args: Option<Vec<String>>,
}

/// Resolved native configuration built from [`SessionConfigJson`].
pub struct ResolvedSessionConfig {
    pub host_mode: HostMode,
    pub sandbox: SandboxConfig,
    pub program_args: Vec<String>,
}

/// Parses an optional JSON configuration string into [`ResolvedSessionConfig`].
pub fn parse_session_config(raw: Option<&str>) -> Result<ResolvedSessionConfig, String> {
    let cfg: SessionConfigJson = match raw {
        Some(text) if !text.trim().is_empty() => serde_json::from_str(text)
            .map_err(|err| format!("invalid session config JSON: {err}"))?,
        _ => SessionConfigJson::default(),
    };

    let mut sandbox = match cfg.preset.as_deref() {
        None | Some("untrusted") => {
            SandboxConfig::untrusted().with_limits(ResourceLimits::sandbox_defaults())
        }
        Some("unrestricted") => SandboxConfig::unrestricted(),
        Some(other) => {
            return Err(format!(
                "unknown preset `{other}` (expected `untrusted` or `unrestricted`)"
            ));
        }
    };

    if let Some(cap_names) = cfg.capabilities {
        let mut caps = CapabilitySet::pure();
        for name in cap_names {
            let cap: Capability = name.parse().map_err(|err| format!("{err}"))?;
            caps.grant(cap);
        }
        sandbox.capabilities = caps;
    }

    let mut limits: ResourceLimits = sandbox.limits;
    if let Some(fuel) = cfg.fuel {
        limits.fuel = Some(fuel);
    }
    if let Some(heap_bytes) = cfg.heap_bytes {
        limits.heap_bytes = Some(heap_bytes);
    }
    if let Some(wall_time_ms) = cfg.wall_time_ms {
        limits.wall_time_ms = Some(wall_time_ms);
    }
    if let Some(max_stack_depth) = cfg.max_stack_depth {
        limits.max_stack_depth = Some(max_stack_depth);
    }
    limits = limits.with_clock(platform_wall_clock());
    sandbox.limits = limits;

    let host_mode = match cfg.host_mode.as_deref() {
        None | Some("live") => HostMode::LiveOs,
        Some("deterministic") => HostMode::Deterministic,
        Some(other) => {
            return Err(format!(
                "unknown hostMode `{other}` (expected `live` or `deterministic`)"
            ));
        }
    };

    Ok(ResolvedSessionConfig {
        host_mode,
        sandbox,
        program_args: cfg.program_args.unwrap_or_default(),
    })
}

/// JSON request payload for `VmSession::host_fetch`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FetchRequestJson {
    pub url: String,
    #[serde(default = "default_get")]
    pub method: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body_base64: Option<String>,
    #[serde(default)]
    pub body_text: Option<String>,
}

fn default_get() -> String {
    "GET".to_owned()
}

pub fn parse_fetch_request(raw: &str) -> Result<HostFetchRequest, String> {
    let parsed: FetchRequestJson =
        serde_json::from_str(raw).map_err(|err| format!("invalid fetch request JSON: {err}"))?;
    let body = if let Some(b64) = parsed.body_base64 {
        Some(
            BASE64
                .decode(b64)
                .map_err(|err| format!("invalid bodyBase64: {err}"))?,
        )
    } else {
        parsed.body_text.map(String::into_bytes)
    };
    Ok(HostFetchRequest {
        url: parsed.url,
        method: parsed.method,
        headers: parsed.headers,
        body,
    })
}

pub fn encode_fetch_response(resp: &HostFetchResponse) -> Value {
    json!({
        "status": resp.status,
        "headers": resp.headers,
        "bodyBase64": BASE64.encode(&resp.body),
        "bodyText": String::from_utf8(resp.body.clone()).ok(),
    })
}

pub fn encode_load_report(
    report: &LoadReport,
    entry: Option<(&str, &str)>,
    root_layer: Option<(&str, &str)>,
) -> Value {
    let globals: Vec<Value> = report
        .loaded_globals
        .iter()
        .map(|g| json!({ "module": g.module, "name": g.name }))
        .collect();
    json!({
        "loadedModules": report.loaded_modules,
        "loadedGlobals": globals,
        "restoredValues": report.restored_values,
        "entry": entry.map(|(m, n)| json!({ "module": m, "name": n })),
        "rootLayer": root_layer.map(|(m, n)| json!({ "module": m, "name": n })),
    })
}

pub fn encode_unload_report(report: &UnloadReport) -> Value {
    json!({
        "module": report.module,
        "dependents": report.dependents,
        "removedGlobals": report.removed_globals,
        "remainingLiveSlots": report.remaining_live_slots,
    })
}

pub fn encode_eval_outcome(outcome: &EvalOutcome) -> Value {
    match outcome {
        EvalOutcome::Value { rendered, stdout } => json!({
            "kind": "value",
            "rendered": rendered,
            "stdout": stdout,
        }),
        EvalOutcome::Declaration { stdout } => json!({
            "kind": "declaration",
            "stdout": stdout,
        }),
        EvalOutcome::TypedError { rendered, stdout } => json!({
            "kind": "typedError",
            "rendered": rendered,
            "stdout": stdout,
        }),
        EvalOutcome::UnhandledEffect { effect, op, stdout } => json!({
            "kind": "unhandledEffect",
            "effect": effect,
            "op": op,
            "stdout": stdout,
        }),
        EvalOutcome::Interrupted { stdout } => json!({
            "kind": "interrupted",
            "stdout": stdout,
        }),
        EvalOutcome::CapabilityDenied {
            capability,
            binding,
            span,
            stdout,
        } => json!({
            "kind": "capabilityDenied",
            "capability": capability.as_str(),
            "binding": binding,
            "span": span.map(|s| json!({ "file": s.file, "start": s.start, "end": s.end })),
            "stdout": stdout,
        }),
        EvalOutcome::LimitExceeded {
            kind,
            message,
            stdout,
        } => json!({
            "kind": "limitExceeded",
            "limit": kind.as_str(),
            "message": message,
            "stdout": stdout,
        }),
        EvalOutcome::Fault { message, stdout } => json!({
            "kind": "fault",
            "message": message,
            "stdout": stdout,
        }),
    }
}

pub fn encode_step_outcome(outcome: &StepOutcome) -> Value {
    match outcome {
        StepOutcome::Yielded { fuel_used } => json!({
            "status": "yielded",
            "fuelUsed": fuel_used,
        }),
        StepOutcome::Completed(eval) => json!({
            "status": "completed",
            "outcome": encode_eval_outcome(eval),
        }),
        StepOutcome::LimitExceeded {
            kind,
            message,
            stdout,
            stderr,
        } => json!({
            "status": "limitExceeded",
            "limit": kind.as_str(),
            "message": message,
            "stdout": stdout,
            "stderr": stderr,
        }),
        StepOutcome::Faulted {
            message,
            stdout,
            stderr,
        } => json!({
            "status": "faulted",
            "message": message,
            "stdout": stdout,
            "stderr": stderr,
        }),
    }
}

#[derive(Serialize)]
struct TestCaseJson<'a> {
    name: &'a str,
    passed: bool,
    skipped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
}

pub fn encode_test_results(cases: &[TestCaseResult]) -> Value {
    let items: Vec<TestCaseJson<'_>> = cases
        .iter()
        .map(|case| TestCaseJson {
            name: &case.name,
            passed: case.passed,
            skipped: case.skipped,
            message: case.message.as_deref(),
        })
        .collect();
    json!(items)
}
