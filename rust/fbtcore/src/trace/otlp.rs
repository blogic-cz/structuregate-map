//! ONE RUN AS OTLP/JSON - the `ExportTraceServiceRequest` an OpenTelemetry Collector's `otlpjsonfile` receiver
//! reads one per line, so a trace file is shipped to any tracing backend without one more line of code here.
//! The shape is the protobuf JSON mapping: ids in lowercase hex, times and 64-bit integers as decimal STRINGS.

use super::Span;
use serde_json::{json, Value};

/// `SPAN_KIND_INTERNAL`: every span is work inside this process.
const INTERNAL: i64 = 1;
/// `STATUS_CODE_ERROR`, for a run that exited 2 - a run that could not answer. Exit 1 is a verdict, not a fault.
const ERROR: i64 = 2;

pub(super) fn encode(trace_id: &str, resource: &[(String, Value)], spans: &[Span], exit: i64) -> Value {
    let spans: Vec<Value> = spans
        .iter()
        .map(|span| {
            let mut out = json!({
                "traceId": trace_id,
                "spanId": span.id,
                "name": span.name,
                "kind": INTERNAL,
                "startTimeUnixNano": span.start_ns.to_string(),
                "endTimeUnixNano": span.end_ns.max(span.start_ns).to_string(),
                "attributes": attributes(&span.attributes),
            });
            match &span.parent {
                Some(parent) => out["parentSpanId"] = json!(parent),
                None if exit == 2 => out["status"] = json!({ "code": ERROR }),
                None => {}
            }
            out
        })
        .collect();
    json!({
        "resourceSpans": [{
            "resource": { "attributes": attributes(resource) },
            "scopeSpans": [{ "scope": { "name": "structuregate" }, "spans": spans }],
        }]
    })
}

/// What the OpenTelemetry SDKs read from the environment, read the same way: `OTEL_RESOURCE_ATTRIBUTES` as
/// `key=value,key=value` (a team, a CI job, an environment), and `OTEL_SERVICE_NAME` over both.
pub(super) fn from_environment() -> Vec<(String, Value)> {
    let mut pairs: Vec<(String, Value)> = Vec::new();
    let mut put = |key: &str, value: &str| {
        pairs.retain(|(k, _)| k != key);
        pairs.push((key.to_string(), Value::from(value)));
    };
    put("service.name", "structuregate");
    for pair in std::env::var("OTEL_RESOURCE_ATTRIBUTES").unwrap_or_default().split(',') {
        if let Some((key, value)) = pair.split_once('=')
            && !key.trim().is_empty()
        {
            put(key.trim(), value.trim());
        }
    }
    if let Some(name) = std::env::var("OTEL_SERVICE_NAME").ok().filter(|n| !n.trim().is_empty()) {
        put("service.name", name.trim());
    }
    pairs
}

fn attributes(pairs: &[(String, Value)]) -> Vec<Value> {
    pairs.iter().map(|(key, value)| json!({ "key": key, "value": any_value(value) })).collect()
}

/// A value as OTLP's `AnyValue`. An integer is a STRING here, as the protobuf JSON mapping writes an int64.
fn any_value(value: &Value) -> Value {
    match value {
        Value::Bool(b) => json!({ "boolValue": b }),
        Value::Number(n) if n.is_i64() || n.is_u64() => json!({ "intValue": n.to_string() }),
        Value::Number(n) => json!({ "doubleValue": n.as_f64() }),
        Value::Array(items) => json!({ "arrayValue": { "values": items.iter().map(any_value).collect::<Vec<_>>() } }),
        Value::String(s) => json!({ "stringValue": s }),
        other => json!({ "stringValue": other.to_string() }),
    }
}
