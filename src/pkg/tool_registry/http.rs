//! HTTP Tool Runtime.
//! HTTP Tool Runtime.
//! HTTP Tool Runtime.
//!
//! HTTP tools are database-registered tools. `ToolPo.config` stores a JSON
//! serialized `HttpToolConfig`, and the registry turns that persistent metadata
//! into an executable `HttpCoreTool`.

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::tool_security::*;
use anyhow::anyhow;
use async_trait::async_trait;
use common::err;
use common::error::Result;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Method, Url, redirect};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::str::FromStr;
use std::time::Duration;

/// Protocol-level HTTP tool factory.
///
/// HTTP tools are database-registered and config-driven, so they do not need
/// one factory per tool id. A protocol-level factory keeps dependency injection
/// explicit while still constructing each executable tool from its `ToolPo`.
pub trait HttpToolFactory: Send + Sync {
    fn create(&self, po: ToolPo) -> Result<Box<dyn CoreTool>>;
}

#[derive(Debug, Clone, Default)]
pub struct DefaultHttpToolFactory;

impl HttpToolFactory for DefaultHttpToolFactory {
    fn create(&self, po: ToolPo) -> Result<Box<dyn CoreTool>> {
        create_tool(po)
    }
}

/// HTTP tool configuration stored in `ToolPo.config`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpToolConfig {
    /// HTTP method, e.g. GET/POST.
    pub method: String,
    /// Fixed URL template. The model must not supply raw URL at call time.
    pub url: String,

    /// Header template object.
    pub headers: Option<Value>,
    /// Query template object.
    pub query: Option<Value>,
    /// Body template object.
    pub body: Option<Value>,

    /// Per-tool timeout override.
    pub timeout_ms: Option<u64>,
    /// Maximum response bytes accepted by the runtime.
    pub response_max_bytes: Option<usize>,

    /// Accepted HTTP status codes. Defaults will be decided by runtime.
    pub allowed_status_codes: Option<Vec<u16>>,
    /// Optional JSON pointer used to extract a subset from JSON response.
    pub response_json_pointer: Option<String>,

    /// Domain allow-list for SSRF protection.
    pub allowed_domains: Option<Vec<String>>,
    /// Domain deny-list for SSRF protection.
    pub blocked_domains: Option<Vec<String>>,
    /// Explicit risk-acknowledgement switch for localhost/private-network targets.
    /// Defaults to false when omitted.
    pub allow_local_network: Option<bool>,
}

/// Executable HTTP core tool created from `ToolPo + HttpToolConfig`.
#[derive(Debug, Clone)]
pub struct HttpCoreTool {
    po: ToolPo,
    config: HttpToolConfig,
}

impl HttpCoreTool {
    /// Build an HTTP core tool from a persistent ToolPo.
    pub fn from_po(po: ToolPo) -> Result<Self> {
        let config: HttpToolConfig = serde_json::from_value(po.config.clone())
            .map_err(|e| anyhow!("invalid http tool config for {}: {}", po.id, e))?;

        validate_config(&config)?;

        Ok(Self { po, config })
    }

    pub fn config(&self) -> &HttpToolConfig {
        &self.config
    }
}

#[async_trait]
impl CoreTool for HttpCoreTool {
    async fn call(&self, _ctx: RequestContext, args: Value) -> Result<Value> {
        execute_http_call(&self.config, self.po.parameters_schema.as_ref(), args)
            .await
            .map_err(|e| err!(ToolExecutionFailed, e.to_string()))
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}

/// Create an executable HTTP tool from ToolPo.
pub fn create_tool(po: ToolPo) -> Result<Box<dyn CoreTool>> {
    Ok(Box::new(HttpCoreTool::from_po(po)?))
}

/// Validate an HTTP ToolPo without constructing the executable runtime.
///
/// Management APIs call this before persisting HTTP tool configs so invalid or
/// unsafe definitions are rejected at configuration time, not only at runtime.
pub fn validate_tool_po_config(po: &ToolPo) -> Result<()> {
    let config: HttpToolConfig = serde_json::from_value(po.config.clone())
        .map_err(|e| anyhow!("invalid http tool config for {}: {}", po.id, e))?;
    validate_config(&config)
}

async fn execute_http_call(
    config: &HttpToolConfig,
    parameters_schema: Option<&Value>,
    args: Value,
) -> Result<Value> {
    validate_args_schema(parameters_schema, &args)?;

    let method = parse_supported_method(&config.method)?;
    let rendered_url = render_string_template(&config.url, &args)?;
    let url = Url::parse(&rendered_url).map_err(|e| anyhow!("invalid rendered http url: {}", e))?;
    let pinned_addresses = validate_target_url(
        config.allow_local_network,
        config.allowed_domains.as_ref(),
        config.blocked_domains.as_ref(),
        &url,
    )
    .await?;

    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("http url host is required"))
        .map_err(Into::<common::error::Error>::into)
        .map(|s| s.to_string())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms(config)?))
        .redirect(redirect::Policy::none())
        .no_proxy()
        .resolve_to_addrs(&host, &pinned_addresses)
        .build()
        .map_err(|e| {
            common::error::Error::new(common::error::ErrorCode::NetworkError, e.to_string())
        })?;

    let mut request = client.request(method.clone(), url);

    if let Some(query) = render_object_template(config.query.as_ref(), &args)? {
        request = request.query(&query);
    }

    if let Some(headers) = render_object_template(config.headers.as_ref(), &args)? {
        for (name, value) in headers {
            request = request.header(
                HeaderName::from_str(&name).map_err(|_| anyhow!("invalid http header name"))?,
                HeaderValue::from_str(&value).map_err(|_| anyhow!("invalid http header value"))?,
            );
        }
    }

    if method == Method::POST
        && let Some(body) = &config.body
    {
        request = request.json(&render_value_template(body, &args)?);
    }

    let mut response = request
        .send()
        .await
        .map_err(|_| anyhow!("http request failed"))?;
    let status = response.status().as_u16();
    let headers = sanitize_response_headers(response.headers());
    let allowed = config
        .allowed_status_codes
        .clone()
        .unwrap_or_else(|| vec![200, 201, 202, 204]);
    if !allowed.contains(&status) {
        return Err(anyhow!("unexpected http status code: {}", status)).map_err(Into::into);
    }

    let max_bytes = response_max_bytes(config)?;
    let bytes = read_limited_response_body(&mut response, max_bytes).await?;
    let content_length = bytes.len();

    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).to_string()))
    };

    let body = if let Some(pointer) = &config.response_json_pointer {
        body.pointer(pointer)
            .cloned()
            .ok_or_else(|| anyhow!("response json pointer not found"))
            .map_err(Into::<common::error::Error>::into)?
    } else {
        body
    };

    Ok(json!({
        "status": status,
        "headers": headers,
        "content_length": content_length,
        "body": body,
    }))
}

fn parse_supported_method(method: &str) -> Result<Method> {
    match method.to_ascii_uppercase().as_str() {
        "GET" => Ok(Method::GET),
        "POST" => Ok(Method::POST),
        other => Err(anyhow!("unsupported http method: {}", other).into()),
    }
}

fn validate_args_schema(parameters_schema: Option<&Value>, args: &Value) -> Result<()> {
    let Some(schema) = parameters_schema else {
        return Ok(());
    };

    let args_object = args.as_object().ok_or_else(|| -> common::error::Error {
        anyhow!("http tool args must be a JSON object").into()
    })?;

    if let Some(Value::Array(required)) = schema.get("required") {
        for name in required.iter().filter_map(Value::as_str) {
            if !args_object.contains_key(name) {
                return Err(anyhow!("unknown tool argument: {}", name).into());
            }
        }
    }

    if let Some(Value::Bool(false)) = schema.get("additionalProperties")
        && let Some(Value::Object(properties)) = schema.get("properties")
    {
        for name in args_object.keys() {
            if !properties.contains_key(name) {
                return Err(anyhow!("unknown tool argument: {}", name).into());
            }
        }
    }

    if let Some(Value::Object(properties)) = schema.get("properties") {
        for (name, value) in args_object {
            let Some(property_schema) = properties.get(name) else {
                continue;
            };

            validate_arg_value(name, value, property_schema)?;
        }
    }

    Ok(())
}

fn validate_arg_value(name: &str, value: &Value, property_schema: &Value) -> Result<()> {
    if let Some(Value::Array(allowed_values)) = property_schema.get("enum")
        && !allowed_values.iter().any(|allowed| allowed == value)
    {
        return Err(anyhow!("invalid enum value for tool argument {}", name).into());
    }

    let Some(expected_type) = property_schema.get("type").and_then(Value::as_str) else {
        return Ok(());
    };

    let valid = match expected_type {
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "null" => value.is_null(),
        _ => true,
    };

    if !valid {
        return Err(anyhow!(
            "invalid type for tool argument {}: expected {}",
            name,
            expected_type
        )
        .into());
    }

    Ok(())
}
fn render_object_template(
    template: Option<&Value>,
    args: &Value,
) -> Result<Option<Vec<(String, String)>>> {
    let Some(Value::Object(object)) = template else {
        return Ok(None);
    };

    let mut rendered = Vec::with_capacity(object.len());
    for (key, value) in object {
        rendered.push((key.clone(), render_scalar_template(value, args)?));
    }

    Ok(Some(rendered))
}

fn render_value_template(value: &Value, args: &Value) -> Result<Value> {
    match value {
        Value::String(s) => Ok(Value::String(render_string_template(s, args)?)),
        Value::Array(items) => items
            .iter()
            .map(|item| render_value_template(item, args))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array),
        Value::Object(object) => object
            .iter()
            .map(|(key, value)| Ok((key.clone(), render_value_template(value, args)?)))
            .collect::<Result<Map<String, Value>>>()
            .map(Value::Object),
        value => Ok(value.clone()),
    }
}

fn render_scalar_template(value: &Value, args: &Value) -> Result<String> {
    match render_value_template(value, args)? {
        Value::String(s) => Ok(s),
        Value::Number(number) => Ok(number.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Null => Ok(String::new()),
        _ => Err(anyhow!("http query/header template must render to scalar").into()),
    }
}

fn render_string_template(template: &str, args: &Value) -> Result<String> {
    let mut rendered = template.to_string();
    if let Value::Object(object) = args {
        for (key, value) in object {
            let placeholder = format!("{{{{args.{}}}}}", key);
            let replacement = match value {
                Value::String(s) => s.clone(),
                Value::Number(number) => number.to_string(),
                Value::Bool(value) => value.to_string(),
                Value::Null => String::new(),
                other => other.to_string(),
            };
            rendered = rendered.replace(&placeholder, &replacement);
        }
    }

    if rendered.contains("{{") {
        return Err(anyhow!("unresolved or unsupported http template placeholder").into());
    }

    Ok(rendered)
}

fn validate_config(config: &HttpToolConfig) -> Result<()> {
    if config.method.trim().is_empty() {
        return Err(anyhow!("http tool method is required").into());
    }

    parse_supported_method(&config.method)?;

    if config.url.trim().is_empty() {
        return Err(anyhow!("http tool url is required").into());
    }

    validate_url_template_boundary(&config.url)?;
    validate_supported_placeholders(&config.url)?;
    validate_fixed_target_policy(config)?;
    validate_scalar_template_object("headers", config.headers.as_ref())?;
    validate_scalar_template_object("query", config.query.as_ref())?;
    validate_body_template(config.body.as_ref())?;
    validate_allowed_status_codes(config.allowed_status_codes.as_ref())?;
    validate_response_json_pointer(config.response_json_pointer.as_deref())?;
    timeout_ms(config)?;
    response_max_bytes(config)?;

    Ok(())
}

fn validate_body_template(template: Option<&Value>) -> Result<()> {
    let Some(template) = template else {
        return Ok(());
    };

    match template {
        Value::String(value) => validate_supported_placeholders(value),
        Value::Array(items) => {
            for item in items {
                validate_body_template(Some(item))?;
            }
            Ok(())
        }
        Value::Object(object) => {
            for (key, value) in object {
                if key.contains("{{") {
                    return Err(
                        anyhow!("http body template keys must not contain placeholders").into(),
                    );
                }
                validate_body_template(Some(value))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_allowed_status_codes(codes: Option<&Vec<u16>>) -> Result<()> {
    let Some(codes) = codes else {
        return Ok(());
    };
    if codes.is_empty() {
        return Err(anyhow!("http allowed_status_codes must not be empty").into());
    }
    if codes.iter().any(|code| !(100..=599).contains(code)) {
        return Err(anyhow!("http allowed_status_codes must be valid HTTP status codes").into());
    }
    Ok(())
}

fn validate_response_json_pointer(pointer: Option<&str>) -> Result<()> {
    let Some(pointer) = pointer else {
        return Ok(());
    };
    if !pointer.is_empty() && !pointer.starts_with('/') {
        return Err(anyhow!("http response_json_pointer must be a valid JSON pointer").into());
    }
    Ok(())
}

fn validate_fixed_target_policy(config: &HttpToolConfig) -> Result<()> {
    let parseable_url = url_template_with_placeholder_sentinels(&config.url)?;
    let parsed = Url::parse(&parseable_url).map_err(|_| anyhow!("invalid http tool url"))?;
    let host = normalize_domain(
        parsed
            .host_str()
            .ok_or_else(|| anyhow!("http url host is required"))?,
    );

    if let Some(blocked_domains) = &config.blocked_domains
        && blocked_domains
            .iter()
            .any(|domain| domain_matches(&host, domain))
    {
        return Err(anyhow!("blocked http domain").into());
    }

    if let Some(allowed_domains) = &config.allowed_domains
        && !allowed_domains
            .iter()
            .any(|domain| domain_matches(&host, domain))
    {
        return Err(anyhow!("http domain is not allowed").into());
    }

    if is_local_network_host(&host) && config.allow_local_network != Some(true) {
        return Err(anyhow!("local network http target requires allow_local_network=true").into());
    }

    Ok(())
}

fn url_template_with_placeholder_sentinels(url_template: &str) -> Result<String> {
    let mut output = String::with_capacity(url_template.len());
    let mut rest = url_template;
    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            return Err(anyhow!("unresolved or unsupported http template placeholder").into());
        };
        output.push_str("placeholder");
        rest = &after_start[end + 2..];
    }
    output.push_str(rest);
    Ok(output)
}

fn validate_scalar_template_object(field_name: &str, template: Option<&Value>) -> Result<()> {
    let Some(template) = template else {
        return Ok(());
    };

    let Value::Object(object) = template else {
        if template.is_null() {
            return Ok(());
        }
        return Err(anyhow!("http {} template must be an object", field_name).into());
    };

    for (key, value) in object {
        if key.contains("{{") {
            return Err(anyhow!(
                "http {} template keys must not contain placeholders",
                field_name
            )
            .into());
        }
        if field_name == "headers" {
            HeaderName::from_str(key).map_err(|_| -> common::error::Error {
                anyhow!("invalid http header name").into()
            })?;
        }
        match value {
            Value::String(template) => {
                validate_supported_placeholders(template)?;
                if field_name == "headers" && !template.contains("{{") {
                    HeaderValue::from_str(template).map_err(|_| -> common::error::Error {
                        anyhow!("invalid http header value").into()
                    })?;
                }
            }
            Value::Number(_) | Value::Bool(_) | Value::Null => {}
            _ => {
                return Err(anyhow!("http {} template values must be scalar", field_name).into());
            }
        }
    }

    Ok(())
}

fn validate_supported_placeholders(template: &str) -> Result<()> {
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            return Err(anyhow!("unresolved or unsupported http template placeholder").into());
        };
        let placeholder = &after_start[..end];
        if placeholder.trim() != placeholder
            || !placeholder.starts_with("args.")
            || placeholder.len() <= "args.".len()
        {
            return Err(anyhow!("unresolved or unsupported http template placeholder").into());
        }
        rest = &after_start[end + 2..];
    }

    if rest.contains("}}") {
        return Err(anyhow!("unresolved or unsupported http template placeholder").into());
    }

    Ok(())
}

fn timeout_ms(config: &HttpToolConfig) -> Result<u64> {
    let timeout_ms = config.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    if timeout_ms == 0 || timeout_ms > HARD_TIMEOUT_MS {
        return Err(anyhow!(
            "invalid http timeout_ms: {} (must be 1..={})",
            timeout_ms,
            HARD_TIMEOUT_MS
        )
        .into());
    }

    Ok(timeout_ms)
}

fn response_max_bytes(config: &HttpToolConfig) -> Result<usize> {
    let max_bytes = config
        .response_max_bytes
        .unwrap_or(DEFAULT_RESPONSE_MAX_BYTES);
    if max_bytes == 0 || max_bytes > HARD_RESPONSE_MAX_BYTES {
        return Err(anyhow!(
            "invalid http response_max_bytes: {} (must be 1..={})",
            max_bytes,
            HARD_RESPONSE_MAX_BYTES
        )
        .into());
    }

    Ok(max_bytes)
}

// Re-export common constants for backward compatibility
pub use crate::pkg::tool_registry::tool_security::{
    DEFAULT_RESPONSE_MAX_BYTES, DEFAULT_TIMEOUT_MS, HARD_RESPONSE_MAX_BYTES, HARD_TIMEOUT_MS,
};

#[cfg(test)]
#[path = "http_tests.rs"]
mod http_tests;
