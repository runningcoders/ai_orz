//! Handler: PUT /api/v1/tools/{id} - Update tool configuration

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateToolRequest, UpdateToolResponse};
use common::enums::{ToolProtocol, ToolStatus};

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::response::to_detail;
use common::error::{Result, bail_err, err};

/// Update an existing custom tool's configuration (name, description, credentials)
#[register_handler_tool(
    id = "update_tool",
    name = "Update Tool",
    description = "Update a custom tool's name, description, config, parameters schema, tags, or enabled state; only provided fields change. Built-in tools accept config edits only. Returns the updated tool detail; fails if the tool does not exist.",
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

    // Builtin 字段所有权保护（D28）：工厂所有权字段与启停别名不允许经
    // update_tool 修改，仅放行 config（走 validate_builtin_config 校验）；
    // Builtin 启停请走 update_tool_status 专用通道
    reject_builtin_field_edits(tool.po.protocol, &params)?;
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

/// Builtin 工具管理接口字段保护（D28：CLI 命令与行为参数为运维所有权字段）
///
/// 工厂所有权字段（name/description/protocol/control_mode/parameters_schema/tags）
/// 与启停别名 enabled 不允许经 update_tool 修改，仅放行 config（config 走
/// validate_builtin_config 校验）；Builtin 启停走 update_tool_status 专用通道。
fn reject_builtin_field_edits(protocol: ToolProtocol, params: &UpdateToolRequest) -> Result<()> {
    if !matches!(protocol, ToolProtocol::Builtin) {
        return Ok(());
    }
    let factory_field_edited = params.name.is_some()
        || params.description.is_some()
        || params.protocol.is_some()
        || params.control_mode.is_some()
        || params.parameters_schema.is_some()
        || params.tags.is_some()
        || params.enabled.is_some();
    if factory_field_edited {
        bail_err!(InvalidRequest, "内置工具仅支持修改 config");
    }
    Ok(())
}

/// Builtin 工具 config 已知字段轻量校验（D28：CLI 命令与行为参数进 PO config）
///
/// 规则本体单点在 common `validate_builtin_tool_config`（与前端表单提交前校验共用），
/// 此处仅包装为 Error；未知字段宽松保留，`config` 非对象（含 Null，存量 DB 兼容）直接通过。
fn validate_builtin_config(config: &serde_json::Value) -> Result<()> {
    common::models::validate_builtin_tool_config(config)
        .map_err(|msg| err!(InvalidRequest, "{}", msg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_builtin_field_edits_passes_non_builtin_protocol() {
        // 非 Builtin 工具不受该 guard 约束（字段编辑走既有语义）
        for protocol in [ToolProtocol::Http, ToolProtocol::Mcp] {
            let params = UpdateToolRequest {
                id: "any".to_string(),
                name: Some("renamed".to_string()),
                protocol: Some(ToolProtocol::Builtin),
                ..Default::default()
            };
            assert!(reject_builtin_field_edits(protocol, &params).is_ok());
        }
    }

    #[test]
    fn reject_builtin_field_edits_allows_config_only() {
        let params = UpdateToolRequest {
            id: "gh_cli".to_string(),
            config: Some(serde_json::json!({ "command": "gh" })),
            ..Default::default()
        };
        assert!(reject_builtin_field_edits(ToolProtocol::Builtin, &params).is_ok());

        // 全空参数同样放行（无任何修改意图）
        let empty = UpdateToolRequest {
            id: "gh_cli".to_string(),
            ..Default::default()
        };
        assert!(reject_builtin_field_edits(ToolProtocol::Builtin, &empty).is_ok());
    }

    #[test]
    fn reject_builtin_field_edits_rejects_factory_fields_and_enabled() {
        use common::enums::ControlMode;

        let cases: Vec<UpdateToolRequest> = vec![
            UpdateToolRequest {
                name: Some("renamed".to_string()),
                ..Default::default()
            },
            UpdateToolRequest {
                description: Some("new description".to_string()),
                ..Default::default()
            },
            UpdateToolRequest {
                protocol: Some(ToolProtocol::Builtin),
                ..Default::default()
            },
            UpdateToolRequest {
                control_mode: Some(ControlMode::Manual),
                ..Default::default()
            },
            UpdateToolRequest {
                parameters_schema: Some(serde_json::json!({"type": "object"})),
                ..Default::default()
            },
            UpdateToolRequest {
                tags: Some(vec!["tag".to_string()]),
                ..Default::default()
            },
            UpdateToolRequest {
                enabled: Some(false),
                ..Default::default()
            },
        ];
        for params in cases {
            let err = reject_builtin_field_edits(ToolProtocol::Builtin, &params).unwrap_err();
            assert_eq!(err.code_enum(), common::error::ErrorCode::InvalidRequest);
            assert!(err.to_string().contains("内置工具仅支持修改 config"));
        }
    }

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
