//! HR 域 API - Agent 管理、技能管理、工具包/技能包管理

use common::api::{
    CreateAgentRequest, CreateAgentResponse, CreateSkillRequest, CreateSkillResponse,
    DeleteSkillResponse, GetAgentResponse, GetSkillResponse, ListAgentsResponse,
    ListInstalledSkillPacksResponse, ListInstalledToolPacksResponse, ListSkillsResponse,
    ListToolsResponse, UpdateAgentRequest, UpdateAgentResponse, UpdateSkillRequest,
    UpdateSkillResponse,
};

use super::{api_delete, api_get, api_get_or_default, api_post, api_post_empty, api_put};

// ===== Agent 管理 =====

pub async fn list_agents() -> Result<ListAgentsResponse, String> {
    api_get_or_default("/api/v1/hr/agents").await
}

pub async fn search_agents(keyword: &str) -> Result<ListAgentsResponse, String> {
    api_get_or_default(&format!("/api/v1/hr/agents/search?keyword={}", keyword)).await
}

pub async fn get_agent(id: &str) -> Result<GetAgentResponse, String> {
    api_get(&format!("/api/v1/hr/agents/{}", id)).await
}

pub async fn create_agent(req: CreateAgentRequest) -> Result<CreateAgentResponse, String> {
    api_post("/api/v1/hr/agents", &req).await
}

pub async fn update_agent(id: &str, req: UpdateAgentRequest) -> Result<UpdateAgentResponse, String> {
    api_put(&format!("/api/v1/hr/agents/{}", id), &req).await
}

pub async fn update_agent_status(id: &str, status: i32) -> Result<(), String> {
    let body = serde_json::json!({ "status": status });
    super::api_put_empty(&format!("/api/v1/hr/agents/{}/status", id), &body).await
}

pub async fn delete_agent(id: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/hr/agents/{}", id)).await
}

// ===== Agent 工具包管理 =====

pub async fn list_installed_tool_packs(agent_id: &str) -> Result<ListInstalledToolPacksResponse, String> {
    api_get_or_default(&format!("/api/v1/hr/agents/{}/tool-packs", agent_id)).await
}

pub async fn install_tool_pack(agent_id: &str, tag: &str) -> Result<(), String> {
    let body = serde_json::json!({});
    super::api_post_empty(&format!("/api/v1/hr/agents/{}/tool-packs/{}", agent_id, tag), &body).await
}

pub async fn uninstall_tool_pack(agent_id: &str, tag: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/hr/agents/{}/tool-packs/{}", agent_id, tag)).await
}

// ===== Agent 技能包管理 =====

pub async fn list_installed_skill_packs(agent_id: &str) -> Result<ListInstalledSkillPacksResponse, String> {
    api_get_or_default(&format!("/api/v1/hr/agents/{}/skill-packs", agent_id)).await
}

pub async fn install_skill_pack(agent_id: &str, tag: &str) -> Result<(), String> {
    let body = serde_json::json!({});
    super::api_post_empty(&format!("/api/v1/hr/agents/{}/skill-packs/{}", agent_id, tag), &body).await
}

pub async fn uninstall_skill_pack(agent_id: &str, tag: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/hr/agents/{}/skill-packs/{}", agent_id, tag)).await
}

// ===== 技能库管理 =====

pub async fn list_skills() -> Result<ListSkillsResponse, String> {
    api_get_or_default("/api/v1/hr/skills").await
}

pub async fn search_skills(keyword: &str) -> Result<ListSkillsResponse, String> {
    api_get_or_default(&format!("/api/v1/hr/skills/search?keyword={}", keyword)).await
}

pub async fn get_skill(id: &str) -> Result<GetSkillResponse, String> {
    api_get(&format!("/api/v1/hr/skills/{}", id)).await
}

pub async fn create_skill(req: CreateSkillRequest) -> Result<CreateSkillResponse, String> {
    api_post("/api/v1/hr/skills", &req).await
}

pub async fn update_skill(id: &str, req: UpdateSkillRequest) -> Result<UpdateSkillResponse, String> {
    api_put(&format!("/api/v1/hr/skills/{}", id), &req).await
}

pub async fn delete_skill(id: &str) -> Result<DeleteSkillResponse, String> {
    // DELETE 返回 ApiResponse<DeleteSkillResponse>，需要走 json 解析
    let resp = super::client()
        .delete(&crate::config::current_config().api_url(&format!("/api/v1/hr/skills/{}", id)));
    let resp = match get_token_bearer(resp).send().await {
        Ok(r) => r,
        Err(e) => return Err(e.to_string()),
    };
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: common::api::ApiResponse<DeleteSkillResponse> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    api_resp.data.ok_or_else(|| "响应数据为空".to_string())
}

// ===== Agent 工具绑定 =====

pub async fn bind_tool_to_agent(agent_id: &str, tool_id: &str) -> Result<(), String> {
    let body = serde_json::json!({});
    api_post_empty(&format!("/api/v1/hr/agents/{}/tools/{}/bind", agent_id, tool_id), &body).await
}

pub async fn unbind_tool_from_agent(agent_id: &str, tool_id: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/hr/agents/{}/tools/{}/bind", agent_id, tool_id)).await
}

// ===== 工具列表（从 Finance 域重导出） =====

pub async fn list_tools() -> Result<ListToolsResponse, String> {
    api_get_or_default("/api/v1/finance/tools").await
}

fn get_token_bearer(req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    if let Some(token) = crate::store::auth::load_token() {
        req.bearer_auth(&token)
    } else {
        req
    }
}
