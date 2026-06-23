use common::api::{ToolDetail, ToolListItem};
use common::enums::ToolStatus;
use reqwest::Url;
use serde_json::{Map, Value};

use crate::models::tool::Tool;

pub(super) fn to_list_item(tool: &Tool) -> ToolListItem {
    ToolListItem {
        id: tool.po.id.clone(),
        name: tool.po.name.clone(),
        description: Some(tool.po.description.clone()),
        protocol: tool.po.protocol,
        control_mode: tool.po.control_mode,
        parameters_schema: tool.po.parameters_schema.clone(),
        tags: tool.po.get_tags(),
        status: tool.po.status,
        has_config: has_config(&tool.po.config),
        enabled: matches!(tool.po.status, ToolStatus::Enabled),
        created_by: tool.po.created_by.clone().unwrap_or_default(),
        created_at: tool.po.created_at,
        updated_at: tool.po.updated_at,
    }
}

pub(super) fn to_detail(tool: &Tool) -> ToolDetail {
    ToolDetail {
        id: tool.po.id.clone(),
        name: tool.po.name.clone(),
        description: tool.po.description.clone(),
        protocol: tool.po.protocol,
        control_mode: tool.po.control_mode,
        config: Some(redact_config(&tool.po.config)),
        has_config: has_config(&tool.po.config),
        parameters_schema: tool.po.parameters_schema.clone(),
        tags: tool.po.get_tags(),
        status: tool.po.status,
        enabled: matches!(tool.po.status, ToolStatus::Enabled),
        created_by: tool.po.created_by.clone(),
        updated_by: tool.po.updated_by.clone(),
        created_at: tool.po.created_at,
        updated_at: tool.po.updated_at,
    }
}

fn has_config(config: &Value) -> bool {
    !config.is_null()
        && !matches!(config, Value::Object(map) if map.is_empty())
        && !matches!(config, Value::String(value) if value.is_empty())
}

fn redact_config(config: &Value) -> Value {
    match config {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    if is_sensitive_config_key(key) {
                        (key.clone(), Value::String("[REDACTED]".to_string()))
                    } else if key.eq_ignore_ascii_case("url") {
                        (key.clone(), redact_url_value(value))
                    } else if is_http_value_container_key(key) {
                        (key.clone(), redact_values_preserving_shape(value))
                    } else {
                        (key.clone(), redact_config(value))
                    }
                })
                .collect::<Map<String, Value>>(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_config).collect()),
        value => value.clone(),
    }
}

fn redact_values_preserving_shape(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), redact_values_preserving_shape(value)))
                .collect::<Map<String, Value>>(),
        ),
        Value::Array(items) => {
            Value::Array(items.iter().map(redact_values_preserving_shape).collect())
        }
        Value::Null => Value::Null,
        _ => Value::String("[REDACTED]".to_string()),
    }
}

fn redact_url_value(value: &Value) -> Value {
    let Some(url) = value.as_str() else {
        return redact_config(value);
    };

    let Ok(mut parsed) = Url::parse(url) else {
        return Value::String("[REDACTED]".to_string());
    };

    if !parsed.username().is_empty() || parsed.password().is_some() {
        let _ = parsed.set_username("");
        let _ = parsed.set_password(None);
    }

    if parsed.query().is_some() {
        let pairs: Vec<(String, String)> = parsed
            .query_pairs()
            .map(|(key, _value)| (key.into_owned(), "[REDACTED]".to_string()))
            .collect();
        parsed.set_query(None);
        if !pairs.is_empty() {
            let mut query = parsed.query_pairs_mut();
            for (key, value) in pairs {
                query.append_pair(&key, &value);
            }
        }
    }

    Value::String(parsed.to_string())
}

fn is_http_value_container_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("headers")
        || key.eq_ignore_ascii_case("query")
        || key.eq_ignore_ascii_case("body")
}

fn is_sensitive_config_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace(['-', '_'], "");
    normalized.contains("authorization")
        || normalized.contains("apikey")
        || normalized.contains("token")
        || normalized.contains("secret")
        || normalized.contains("password")
        || normalized.contains("credential")
        || normalized.contains("cookie")
}
