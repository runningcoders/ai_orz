//! HR 域 API - Agent 管理、技能管理、工具包/技能包管理

use common::api::{
    AgentListItem, AgentQueryRequest, BindToolToAgentRequest, CreateAgentRequest,
    CreateAgentResponse, CreateExternalAgentRequest, CreateExternalAgentResponse,
    CreateSkillRequest, CreateSkillResponse, DeleteSkillResponse, GetAgentRequest,
    GetAgentResponse, GetReceptionAgentResponse, GetSkillFileContentRequest, GetSkillResponse,
    InstallSkillPackRequest, InstallToolPackRequest, ListAgentsRequest, ListAgentsResponse,
    ListInstalledSkillPacksResponse, ListInstalledToolPacksResponse, ListSkillsRequest,
    ListSkillsResponse, PagedResult, QueryMemoryParams, QueryMemoryResponse, SearchMemoryParams,
    SearchMemoryResponse, SkillListItem, SkillQueryRequest, UnbindToolFromAgentRequest,
    UninstallSkillPackRequest, UninstallToolPackRequest, UpdateAgentRequest, UpdateAgentResponse,
    UpdateAgentStatusRequest, UpdateSkillFileContentRequest, UpdateSkillRequest,
    UpdateSkillResponse,
};

use super::{
    ApiError, api_delete, api_get, api_get_or_default, api_post, api_post_empty, api_put,
    api_put_empty,
};

// ===== Agent 管理 =====

pub async fn list_agents(req: ListAgentsRequest) -> Result<PagedResult<AgentListItem>, ApiError> {
    let url = super::build_pagination_url("/api/v1/hr/agents", &req.pagination);
    api_get(&url).await
}

pub async fn query_agents(req: &AgentQueryRequest) -> Result<PagedResult<AgentListItem>, ApiError> {
    api_post("/api/v1/hr/agents/query", req).await
}

/// 查询当前可用的前台 Agent（供前端显示推荐前台 Agent）
pub async fn get_reception_agent() -> Result<GetReceptionAgentResponse, ApiError> {
    api_get("/api/v1/hr/agents/reception").await
}

pub async fn search_agents(keyword: &str) -> Result<ListAgentsResponse, ApiError> {
    api_get_or_default(&format!("/api/v1/hr/agents/search?keyword={}", keyword)).await
}

pub async fn get_agent(req: GetAgentRequest) -> Result<GetAgentResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("with_stats", req.with_stats.map(|v| v.to_string())),
        (
            "with_model_call_stats",
            req.with_model_call_stats.map(|v| v.to_string()),
        ),
        (
            "stats_time_start",
            req.stats_time_start.map(|v| v.to_string()),
        ),
        ("stats_time_end", req.stats_time_end.map(|v| v.to_string())),
        ("stats_interval", req.stats_interval.clone()),
    ]);
    api_get(&format!("/api/v1/hr/agents/{}{}", req.id, qs)).await
}

pub async fn create_agent(req: CreateAgentRequest) -> Result<CreateAgentResponse, ApiError> {
    api_post("/api/v1/hr/agents", &req).await
}

pub async fn create_external_agent(
    req: CreateExternalAgentRequest,
) -> Result<CreateExternalAgentResponse, ApiError> {
    api_post("/api/v1/hr/agents/external", &req).await
}

pub async fn update_agent(req: UpdateAgentRequest) -> Result<UpdateAgentResponse, ApiError> {
    api_put(&format!("/api/v1/hr/agents/{}", req.id), &req).await
}

pub async fn update_agent_status(req: UpdateAgentStatusRequest) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": req.status as i32 });
    api_put_empty(&format!("/api/v1/hr/agents/{}/status", req.id), &body).await
}

pub async fn delete_agent(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/hr/agents/{}", id)).await
}

// ===== Agent 工具包管理 =====

pub async fn list_installed_tool_packs(
    agent_id: &str,
) -> Result<ListInstalledToolPacksResponse, ApiError> {
    api_get_or_default(&format!("/api/v1/hr/agents/{}/tool-packs", agent_id)).await
}

pub async fn install_tool_pack(req: InstallToolPackRequest) -> Result<(), ApiError> {
    let body = serde_json::json!({});
    api_post_empty(
        &format!("/api/v1/hr/agents/{}/tool-packs/{}", req.agent_id, req.tag),
        &body,
    )
    .await
}

pub async fn uninstall_tool_pack(req: UninstallToolPackRequest) -> Result<(), ApiError> {
    api_delete(&format!(
        "/api/v1/hr/agents/{}/tool-packs/{}",
        req.agent_id, req.tag
    ))
    .await
}

// ===== Agent 技能包管理 =====

pub async fn list_installed_skill_packs(
    agent_id: &str,
) -> Result<ListInstalledSkillPacksResponse, ApiError> {
    api_get_or_default(&format!("/api/v1/hr/agents/{}/skill-packs", agent_id)).await
}

pub async fn install_skill_pack(req: InstallSkillPackRequest) -> Result<(), ApiError> {
    let body = serde_json::json!({});
    api_post_empty(
        &format!("/api/v1/hr/agents/{}/skill-packs/{}", req.agent_id, req.tag),
        &body,
    )
    .await
}

pub async fn uninstall_skill_pack(req: UninstallSkillPackRequest) -> Result<(), ApiError> {
    // 支持 delete_copies query 参数：true 表示同时删除 Agent 侧的技能副本
    let qs = super::build_query_string(&[(
        "delete_copies",
        req.delete_copies.map(|v| v.to_string()),
    )]);
    api_delete(&format!(
        "/api/v1/hr/agents/{}/skill-packs/{}{}",
        req.agent_id, req.tag, qs
    ))
    .await
}

// ===== Agent 单技能管理 =====

/// 列出 Agent 已安装的技能列表
pub async fn list_agent_skills(
    agent_id: &str,
) -> Result<common::api::ListAgentSkillsResponse, ApiError> {
    api_get_or_default(&format!("/api/v1/hr/agents/{}/skills", agent_id)).await
}

/// 将源技能安装到指定 Agent（创建 Agent 私有副本）
pub async fn install_skill_to_agent(
    req: common::api::InstallSkillToAgentRequest,
) -> Result<common::api::InstallSkillToAgentResponse, ApiError> {
    api_post(
        &format!("/api/v1/hr/agents/{}/skills/{}", req.agent_id, req.skill_id),
        &serde_json::json!({}),
    )
    .await
}

/// 从 Agent 目录卸载单个技能副本
pub async fn uninstall_skill_from_agent(
    req: common::api::UninstallSkillFromAgentRequest,
) -> Result<(), ApiError> {
    api_delete(&format!(
        "/api/v1/hr/agents/{}/skills/{}",
        req.agent_id, req.skill_id
    ))
    .await
}

// ===== 技能库管理 =====

pub async fn list_skills(req: ListSkillsRequest) -> Result<PagedResult<SkillListItem>, ApiError> {
    let url = super::build_pagination_url("/api/v1/hr/skills", &req.pagination);
    api_get(&url).await
}

pub async fn query_skills(req: &SkillQueryRequest) -> Result<PagedResult<SkillListItem>, ApiError> {
    api_post("/api/v1/hr/skills/query", req).await
}

pub async fn search_skills(keyword: &str) -> Result<ListSkillsResponse, ApiError> {
    api_get_or_default(&format!("/api/v1/hr/skills/search?keyword={}", keyword)).await
}

pub async fn get_skill(id: &str) -> Result<GetSkillResponse, ApiError> {
    api_get(&format!("/api/v1/hr/skills/{}", id)).await
}

pub async fn create_skill(req: CreateSkillRequest) -> Result<CreateSkillResponse, ApiError> {
    api_post("/api/v1/hr/skills", &req).await
}

pub async fn update_skill(req: UpdateSkillRequest) -> Result<UpdateSkillResponse, ApiError> {
    api_put(&format!("/api/v1/hr/skills/{}", req.skill_id), &req).await
}

pub async fn delete_skill(id: &str) -> Result<DeleteSkillResponse, ApiError> {
    let resp = super::client()
        .delete(&crate::config::current_config().api_url(&format!("/api/v1/hr/skills/{}", id)))
        .send()
        .await
        .map_err(super::network_err)?;
    let status = resp.status();
    if !status.is_success() {
        super::handle_unauthorized(status.as_u16());
        return Err(super::parse_error_response(resp).await);
    }
    let api_resp: common::api::ApiResponse<DeleteSkillResponse> =
        resp.json().await.map_err(|e| ApiError {
            http_status: 200,
            error_code: None,
            message: e.to_string(),
        })?;
    if !api_resp.is_success() {
        return Err(ApiError {
            http_status: 200,
            error_code: None,
            message: api_resp.message,
        });
    }
    api_resp.data.ok_or_else(|| ApiError {
        http_status: 200,
        error_code: None,
        message: "响应数据为空".to_string(),
    })
}

/// 列出所有已发布技能的不重复 tag 列表
pub async fn list_skill_tags() -> Result<common::api::ListSkillTagsResponse, ApiError> {
    api_get("/api/v1/hr/skills/tags").await
}

// ===== Skill 文件管理 =====

/// 列出 Skill 的所有文件
pub async fn list_skill_files(
    skill_id: &str,
) -> Result<common::api::ListSkillFilesResponse, ApiError> {
    api_get(&format!("/api/v1/hr/skills/{}/files", skill_id)).await
}

/// 获取 Skill 文件内容（filename 可能含 /，需 URL 编码路径段）
pub async fn get_skill_file_content(
    req: GetSkillFileContentRequest,
) -> Result<common::api::GetSkillFileContentResponse, ApiError> {
    api_get(&format!(
        "/api/v1/hr/skills/{}/files/{}",
        req.skill_id, req.filename
    ))
    .await
}

/// 更新 Skill 文件内容（乐观锁字段前端置 None）
pub async fn update_skill_file_content(req: UpdateSkillFileContentRequest) -> Result<(), ApiError> {
    api_put_empty(
        &format!("/api/v1/hr/skills/{}/files/{}", req.skill_id, req.filename),
        &req,
    )
    .await
}

// ===== Agent 工具绑定 =====

pub async fn bind_tool_to_agent(req: BindToolToAgentRequest) -> Result<(), ApiError> {
    let body = serde_json::json!({});
    api_post_empty(
        &format!(
            "/api/v1/hr/agents/{}/tools/{}/bind",
            req.agent_id, req.tool_id
        ),
        &body,
    )
    .await
}

pub async fn unbind_tool_from_agent(req: UnbindToolFromAgentRequest) -> Result<(), ApiError> {
    api_delete(&format!(
        "/api/v1/hr/agents/{}/tools/{}/bind",
        req.agent_id, req.tool_id
    ))
    .await
}

// ===== 工具列表（从 Finance 域重导出） =====

pub use super::finance::list_tools;

// ===== 记忆搜索 =====

pub async fn search_memory(req: SearchMemoryParams) -> Result<SearchMemoryResponse, ApiError> {
    api_post("/api/v1/hr/agents/search_memory", &req).await
}

pub async fn query_memory(req: QueryMemoryParams) -> Result<QueryMemoryResponse, ApiError> {
    api_post("/api/v1/hr/agents/query_memory", &req).await
}

pub async fn search_memory_with_traversal(
    req: SearchMemoryParams,
) -> Result<SearchMemoryResponse, ApiError> {
    api_post("/api/v1/hr/agents/search_memory", &req).await
}
