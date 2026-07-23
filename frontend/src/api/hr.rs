//! HR 域 API - Agent 管理、技能管理、工具包/技能包管理

use common::api::{
    CreateAgentRequest, CreateAgentResponse, CreateExternalAgentRequest, CreateExternalAgentResponse,
    CreateSkillRequest, CreateSkillResponse,
    DeleteSkillResponse, GetAgentResponse, GetReceptionAgentResponse, GetSkillResponse, ListAgentsResponse,
    ListInstalledSkillPacksResponse, ListInstalledToolPacksResponse, ListSkillsResponse,
    ListToolsResponse, QueryMemoryParams, QueryMemoryResponse, SearchMemoryParams, SearchMemoryResponse,
    UpdateAgentRequest, UpdateAgentResponse, UpdateSkillRequest,
    UpdateSkillResponse,
};

use super::{api_delete, api_get, api_get_or_default, api_post, api_post_empty, api_put, api_put_empty, ApiError};

// ===== Agent 管理 =====

pub async fn list_agents() -> Result<ListAgentsResponse, ApiError> {
    api_get_or_default("/api/v1/hr/agents").await
}

/// 查询当前可用的前台 Agent（供前端显示推荐前台 Agent）
pub async fn get_reception_agent() -> Result<GetReceptionAgentResponse, ApiError> {
    api_get("/api/v1/hr/agents/reception").await
}

pub async fn search_agents(keyword: &str) -> Result<ListAgentsResponse, ApiError> {
    api_get_or_default(&format!("/api/v1/hr/agents/search?keyword={}", keyword)).await
}

pub async fn get_agent(id: &str, stats_options: Option<&super::StatsOptions>) -> Result<GetAgentResponse, ApiError> {
    let url = super::build_url_with_stats(&format!("/api/v1/hr/agents/{}", id), stats_options);
    api_get(&url).await
}

pub async fn create_agent(req: CreateAgentRequest) -> Result<CreateAgentResponse, ApiError> {
    api_post("/api/v1/hr/agents", &req).await
}

pub async fn create_external_agent(req: CreateExternalAgentRequest) -> Result<CreateExternalAgentResponse, ApiError> {
    api_post("/api/v1/hr/agents/external", &req).await
}

#[allow(dead_code)]
pub async fn update_agent(id: &str, req: UpdateAgentRequest) -> Result<UpdateAgentResponse, ApiError> {
    api_put(&format!("/api/v1/hr/agents/{}", id), &req).await
}

pub async fn update_agent_status(id: &str, status: i32) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": status });
    api_put_empty(&format!("/api/v1/hr/agents/{}/status", id), &body).await
}

pub async fn delete_agent(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/hr/agents/{}", id)).await
}

// ===== Agent 工具包管理 =====

pub async fn list_installed_tool_packs(agent_id: &str) -> Result<ListInstalledToolPacksResponse, ApiError> {
    api_get_or_default(&format!("/api/v1/hr/agents/{}/tool-packs", agent_id)).await
}

pub async fn install_tool_pack(agent_id: &str, tag: &str) -> Result<(), ApiError> {
    let body = serde_json::json!({});
    api_post_empty(&format!("/api/v1/hr/agents/{}/tool-packs/{}", agent_id, tag), &body).await
}

pub async fn uninstall_tool_pack(agent_id: &str, tag: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/hr/agents/{}/tool-packs/{}", agent_id, tag)).await
}

// ===== Agent 技能包管理 =====

pub async fn list_installed_skill_packs(agent_id: &str) -> Result<ListInstalledSkillPacksResponse, ApiError> {
    api_get_or_default(&format!("/api/v1/hr/agents/{}/skill-packs", agent_id)).await
}

pub async fn install_skill_pack(agent_id: &str, tag: &str) -> Result<(), ApiError> {
    let body = serde_json::json!({});
    api_post_empty(&format!("/api/v1/hr/agents/{}/skill-packs/{}", agent_id, tag), &body).await
}

pub async fn uninstall_skill_pack(agent_id: &str, tag: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/hr/agents/{}/skill-packs/{}", agent_id, tag)).await
}

// ===== 技能库管理 =====

pub async fn list_skills() -> Result<ListSkillsResponse, ApiError> {
    api_get_or_default("/api/v1/hr/skills").await
}

pub async fn search_skills(keyword: &str) -> Result<ListSkillsResponse, ApiError> {
    api_get_or_default(&format!("/api/v1/hr/skills/search?keyword={}", keyword)).await
}

#[allow(dead_code)]
pub async fn get_skill(id: &str) -> Result<GetSkillResponse, ApiError> {
    api_get(&format!("/api/v1/hr/skills/{}", id)).await
}

pub async fn create_skill(req: CreateSkillRequest) -> Result<CreateSkillResponse, ApiError> {
    api_post("/api/v1/hr/skills", &req).await
}

#[allow(dead_code)]
pub async fn update_skill(id: &str, req: UpdateSkillRequest) -> Result<UpdateSkillResponse, ApiError> {
    api_put(&format!("/api/v1/hr/skills/{}", id), &req).await
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
    let api_resp: common::api::ApiResponse<DeleteSkillResponse> = resp.json().await.map_err(|e| ApiError {
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

// ===== Agent 工具绑定 =====

pub async fn bind_tool_to_agent(agent_id: &str, tool_id: &str) -> Result<(), ApiError> {
    let body = serde_json::json!({});
    api_post_empty(&format!("/api/v1/hr/agents/{}/tools/{}/bind", agent_id, tool_id), &body).await
}

pub async fn unbind_tool_from_agent(agent_id: &str, tool_id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/hr/agents/{}/tools/{}/bind", agent_id, tool_id)).await
}

// ===== 工具列表（从 Finance 域重导出） =====

pub async fn list_tools() -> Result<ListToolsResponse, ApiError> {
    api_get_or_default("/api/v1/finance/tools").await
}

// ===== 记忆搜索 =====

pub async fn search_memory(query: &str, memory_type: Option<&str>) -> Result<SearchMemoryResponse, ApiError> {
    let params = SearchMemoryParams {
        query: query.to_string(),
        max_results: Some(20),
        memory_type: memory_type.map(|s| s.to_string()),
        traversal_depth: None,
        traversal_breadth: None,
        traversal_strategy: None,
        seed_node_ids: None,
    };
    api_post("/api/v1/hr/agents/search_memory", &params).await
}

pub async fn query_memory(agent_id: Option<&str>, memory_type: Option<&str>) -> Result<QueryMemoryResponse, ApiError> {
    let params = QueryMemoryParams {
        agent_id: agent_id.map(|s| s.to_string()),
        memory_type: memory_type.map(|s| s.to_string()),
        limit: Some(20),
    };
    api_post("/api/v1/hr/agents/query_memory", &params).await
}

pub async fn search_memory_with_traversal(query: &str, seed_node_ids: &[String], depth: i32) -> Result<SearchMemoryResponse, ApiError> {
    let params = SearchMemoryParams {
        query: query.to_string(),
        max_results: Some(50),
        memory_type: None,
        traversal_depth: Some(depth),
        traversal_breadth: Some(10),
        traversal_strategy: Some("breadth_first".to_string()),
        seed_node_ids: Some(seed_node_ids.to_vec()),
    };
    api_post("/api/v1/hr/agents/search_memory", &params).await
}
