//! Handler: PUT /api/v1/tools/{id} - Update tool configuration

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateToolRequest, UpdateToolResponse};
use common::enums::{ToolProtocol, ToolStatus};

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;
use common::error::{Result, bail_err, err};

/// Update an existing custom tool's configuration (name, description, credentials, etc.)
#[register_handler_tool(
    id = "update_tool",
    name = "update_tool",
    description = "Update an existing custom tool's configuration (name, description, credentials, etc.)",
    params = "common::api::UpdateToolRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn update_tool(
    ctx: RequestContext,
    params: UpdateToolRequest,
) -> Result<UpdateToolResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let mut tool = domain()
        .tool_provider_manage()
        .get_tool(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "Tool {} not found", params.id))?;

    if matches!(tool.po.protocol, ToolProtocol::Builtin) {
        bail_err!(InvalidRequest, "内置 Tool 不允许通过管理接口修改");
    }
    if matches!(params.protocol, Some(ToolProtocol::Builtin)) {
        bail_err!(InvalidRequest, "非内置 Tool 不允许被修改为内置协议");
    }

    if let Some(name) = params.name {
        tool.po.name = name;
    }
    if let Some(description) = params.description {
        tool.po.description = description;
    }
    if let Some(protocol) = params.protocol {
        tool.po.protocol = protocol;
    }
    if let Some(control_mode) = params.control_mode {
        tool.po.control_mode = control_mode;
    }
    if let Some(config) = params.config {
        // Builtin config 轻量校验（D28：CLI 命令与行为参数为运维所有权字段，
        // sync 保留现场）；未知字段宽松保留，规则见 validate_builtin_config
        if matches!(tool.po.protocol, ToolProtocol::Builtin) {
            validate_builtin_config(&config)?;
        }
        tool.po.config = config;
    }
    if let Some(parameters_schema) = params.parameters_schema {
        tool.po.parameters_schema = Some(parameters_schema);
    }
    if let Some(tags) = params.tags {
        tool.po.tags = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
    }
    if let Some(enabled) = params.enabled {
        let target_status = if enabled {
            ToolStatus::Enabled
        } else {
            ToolStatus::Disabled
        };
        tool.transition_status(target_status, user_id.clone())
            .map_err(|e| err!(InvalidRequest, "{}", e))?;
    }
    tool.po.touch(Some(user_id));

    domain()
        .tool_provider_manage()
        .update_tool(ctx, &tool)
        .await?;

    Ok(to_detail(&tool))
}

/// Builtin 工具 config 已知字段轻量校验（D28：CLI 命令与行为参数进 PO config）
///
/// 仅校验已知字段的类型与取值（command 非空 string / timeout_ms·max_output_bytes
/// 正整数），未知字段宽松保留（不做白名单封闭，保持 config 扩展性）；
/// `config` 非对象（含 Null，存量 DB 兼容）时无已知字段可校验，直接通过。
fn validate_builtin_config(config: &serde_json::Value) -> Result<()> {
    let Some(object) = config.as_object() else {
        return Ok(());
    };
    for (key, value) in object {
        match key.as_str() {
            "command" if !value.as_str().is_some_and(|command| !command.is_empty()) => {
                bail_err!(InvalidRequest, "config.command 必须为非空字符串");
            }
            "timeout_ms" | "max_output_bytes"
                if !value.as_u64().is_some_and(|number| number > 0) =>
            {
                bail_err!(InvalidRequest, "config.{} 必须为正整数", key);
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_builtin_config_accepts_known_and_unknown_fields() {
        let config = serde_json::json!({
            "command": "/usr/local/bin/gh",
            "timeout_ms": 30_000,
            "max_output_bytes": 262_144,
            "install_hint": "brew install gh",
            "custom_field": "未知字段宽松保留"
        });
        assert!(validate_builtin_config(&config).is_ok());
    }

    #[test]
    fn validate_builtin_config_rejects_empty_or_non_string_command() {
        for command in [serde_json::json!(""), serde_json::json!(42)] {
            let config = serde_json::json!({ "command": command });
            let err = validate_builtin_config(&config).unwrap_err();
            assert_eq!(err.code_enum(), common::error::ErrorCode::InvalidRequest);
            assert!(err.to_string().contains("command"));
        }
    }

    #[test]
    fn validate_builtin_config_rejects_non_positive_timeout_ms() {
        for timeout_ms in [
            serde_json::json!(-1),
            serde_json::json!(0),
            serde_json::json!("30000"),
            serde_json::json!(1.5),
        ] {
            let config = serde_json::json!({ "timeout_ms": timeout_ms });
            let err = validate_builtin_config(&config).unwrap_err();
            assert_eq!(err.code_enum(), common::error::ErrorCode::InvalidRequest);
            assert!(err.to_string().contains("timeout_ms"));
        }
    }

    #[test]
    fn validate_builtin_config_rejects_non_positive_max_output_bytes() {
        let config = serde_json::json!({ "max_output_bytes": -1 });
        let err = validate_builtin_config(&config).unwrap_err();
        assert_eq!(err.code_enum(), common::error::ErrorCode::InvalidRequest);
        assert!(err.to_string().contains("max_output_bytes"));
    }

    #[test]
    fn validate_builtin_config_passes_non_object_config() {
        // 存量 DB builtin 工具 config 可能为 Null（D28 零迁移兼容）
        assert!(validate_builtin_config(&serde_json::Value::Null).is_ok());
        assert!(validate_builtin_config(&serde_json::json!("legacy")).is_ok());
    }
}
