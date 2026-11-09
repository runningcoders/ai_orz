//! Handler: POST /api/v1/tools - Create a new custom tool

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateToolRequest, CreateToolResponse};
use common::enums::{CallerType, ToolProtocol};

use crate::models::tool::Tool;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use common::error::{Result, bail_err, err};

/// Create a new custom tool (HTTP/MCP/Shell). Built-in tools cannot be created via this API.
#[register_handler_tool(
    id = "create_tool",
    name = "Create Custom Tool",
    description = "Create a custom HTTP or Shell tool with a name, description, config, parameters schema, and tags. Returns the new tool's id and metadata. Built-in tools cannot be created via this API. Agent-created tools may not carry reserved tags (neural/internal/admin).",
    params = "common::api::CreateToolRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn create_tool(
    ctx: RequestContext,
    params: CreateToolRequest,
) -> Result<CreateToolResponse> {
    // 创建者取调用方身份而非 uid()：Agent 上下文常无 user_id（纯 Agent / 系统
    // 触发），用 uid() 会误拒 Agent 自建工具；created_by 因此是 agent_id。
    let creator = ctx.caller_id_or_system();
    if matches!(params.protocol, ToolProtocol::Builtin) {
        bail_err!(
            InvalidRequest,
            "内置 Tool 由系统同步，不允许通过管理接口创建"
        );
    }
    // 用户自填 schema 的网关安全门（规则本体单点在 common，与前端表单提交前校验共用）：
    // 一个不合法的工具 schema 会让持有它的 Agent 每一轮模型调用都被整包拒绝
    if let Some(schema) = params.parameters_schema.as_ref() {
        common::models::validate_tool_parameters_schema(schema)
            .map_err(|msg| err!(InvalidRequest, "{}", msg))?;
    }

    let tags = params.tags.clone().unwrap_or_default();
    // Agent 自建工具的标签边界：neural 恒在工具面、admin 会被已装包的 Agent
    // 继承、internal 为系统自用，任一都会让 Agent 造出的工具越过自身授权面。
    // 仅约束 Agent 上下文；管理员 HTTP 创建保留既有运维能力。
    if ctx.caller_type() == CallerType::Agent {
        common::models::validate_agent_created_tool_tags(&tags)
            .map_err(|msg| err!(InvalidRequest, "{}", msg))?;
    }
    // 工具级凭据声明统一契约：顶层字段归并进 config（各协议存储位置不变，
    // 但创建任何协议的工具都走同一入口；MCP 有独立声明属主，明确拒绝
    // （Builtin 由本函数更早的协议闸门拒绝，不经此处））
    let mut config = params.config.clone().unwrap_or_default();
    common::models::apply_credential_requirements(
        &mut config,
        params.credential_requirements.as_deref(),
        common::models::tool_credential_scope(params.protocol),
    )
    .map_err(|msg| err!(InvalidRequest, "{}", msg))?;
    let mut tool_po = ToolPo::new(
        String::new(),
        params.name.clone(),
        params.description.clone(),
        params.protocol,
        config,
        params.parameters_schema.clone(),
        tags,
        Some(creator),
    );
    if let Some(control_mode) = params.control_mode {
        tool_po.control_mode = control_mode;
    }
    if let Some(status) = params.enabled {
        tool_po.status = if status {
            common::enums::tool::ToolStatus::Enabled
        } else {
            common::enums::tool::ToolStatus::Disabled
        };
    }
    let tool = Tool::from_po_for_management(tool_po);

    domain()
        .tool_provider_manage()
        .create_tool(ctx.clone(), &tool)
        .await?;

    Ok(CreateToolResponse {
        id: tool.po.id.clone(),
        name: tool.po.name.clone(),
        description: tool.po.description.clone(),
        tool_type: tool.po.protocol.to_string(),
        created_at: tool.po.created_at,
    })
}

#[cfg(test)]
mod tests {
    /// 护栏：`create_tool` 必须挂在出生可达的 `tool_management` 包。
    ///
    /// 「保留 Agent 创建工具的能力」完全由这条 tag 表达。改回 `admin` 会让
    /// Agent 建不了工具；同时挂 `admin,tool_management` 看似两全，实则让
    /// 装了 admin 包的 Agent 也拿到创建权，与受限包边界混在一起难以审计。
    /// 越权封堵另在 `install_tool_pack`（Agent 禁止自开受限包）。
    #[test]
    fn create_tool_is_reachable_from_base_tool_management_pack() {
        let registry = crate::pkg::tool_registry::get_registry();
        let factory = registry
            .get_builtin_factory("create_tool")
            .expect("create_tool 必须注册为内置工具");
        assert_eq!(
            factory.create_po().get_tags(),
            vec!["tool_management".to_string()],
            "create_tool 只允许挂 tool_management 包（改回 admin 会砍掉 Agent 自建工具能力）"
        );
    }
}
