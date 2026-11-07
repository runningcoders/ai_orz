use common::api::{
    RuntimeReady, ToolCredentialSource, ToolCredentialSourceKind, ToolDetail, ToolListItem,
};
use common::enums::{ToolProtocol, ToolStatus};
use reqwest::Url;
use serde_json::{Map, Value};
use std::collections::HashMap;

use crate::models::tool::Tool;
use crate::pkg::RequestContext;

/// 批量判定工具运行时就绪状态（三层就绪提示体系第①层：清单级标志）。
///
/// 数据驱动判定在 domain `tool_readiness`（CLI 型 / key 型），带 TTL 缓存
/// （CLI 型 30s / key 型按用户 30s），列表高频调用无重复开销；判定异常
/// （best-effort Unknown）的工具不进入返回 map（`to_list_item` 落 Unknown 默认值）。
pub(crate) async fn probe_runtime_ready(
    ctx: &RequestContext,
    tools: &[Tool],
) -> HashMap<String, RuntimeReady> {
    let domain = crate::service::domain::runtime::domain();
    let tool_execution = domain.tool_execution();
    let mut ready = HashMap::new();
    for tool in tools {
        let status = tool_execution.tool_readiness(ctx, tool).await;
        // 判定异常 → Unknown，无需占用 map（默认值即 Unknown）
        if status != RuntimeReady::Unknown {
            ready.insert(tool.po.id.clone(), status);
        }
    }
    ready
}

pub(crate) fn to_list_item(tool: &Tool, runtime_ready: RuntimeReady) -> ToolListItem {
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
        runtime_ready,
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
        // 凭据需求聚合（D17/D28 统一入口）：Builtin 工厂静态声明 / Mcp·Http 从 config 解析；
        // 声明本身非敏感（kind/注入点），与 config 脱敏通道互不影响
        credential_requirements: crate::pkg::tool_registry::get_registry()
            .credential_requirements(&tool.po),
        // 继承来源由 handler 经 `attach_credential_source` 异步补齐（需要查 Server 名称）
        credential_source: None,
        parameters_schema: tool.po.parameters_schema.clone(),
        tags: tool.po.get_tags(),
        status: tool.po.status,
        enabled: matches!(tool.po.status, ToolStatus::Enabled),
        created_by: tool.po.created_by.clone(),
        updated_by: tool.po.updated_by.clone(),
        created_at: tool.po.created_at,
        updated_at: tool.po.updated_at,
        stats: tool.stats.clone(),
    }
}

/// 组装 MCP 工具的凭据需求继承来源（纯函数，便于护栏测试）
///
/// 只在「MCP 协议 + 工具自身无声明」时返回；工具行 config 只存
/// `{server_id, tool_name}`，声明属主在 Server 行，这里不复制声明内容。
pub(super) fn credential_source(
    tool: &Tool,
    server_name: Option<String>,
) -> Option<ToolCredentialSource> {
    if !matches!(tool.po.protocol, ToolProtocol::Mcp) {
        return None;
    }
    let server_id = serde_json::from_value::<common::config::McpToolConfig>(tool.po.config.clone())
        .ok()?
        .server_id;
    Some(ToolCredentialSource {
        kind: ToolCredentialSourceKind::McpServer,
        name: server_name
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| server_id.clone()),
        id: server_id,
    })
}

/// 详情回显补齐凭据需求继承来源（只回显 Server 标识，不改声明属主）
///
/// `to_detail` 是同步纯函数、无法查库；MCP Server 名称由此处 best-effort 补一次，
/// 查不到（Server 已删 / DB 异常）时回落为 server_id，详情接口不因此失败。
pub(super) async fn attach_credential_source(
    ctx: &RequestContext,
    tool: &Tool,
    detail: &mut ToolDetail,
) {
    // 工具已有自己的声明时不标注继承（HTTP / Shell / Builtin 工厂静态）
    if !detail.credential_requirements.is_empty() {
        return;
    }
    let Some(source) = credential_source(tool, None) else {
        return;
    };
    let server_name = crate::service::domain::finance::domain()
        .mcp_server_manage()
        .get_mcp_server(ctx.clone(), &source.id)
        .await
        .ok()
        .flatten()
        .map(|server| server.po.name);
    detail.credential_source = credential_source(tool, server_name);
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
