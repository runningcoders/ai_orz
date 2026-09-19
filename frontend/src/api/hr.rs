//! HR 域 API - Agent 管理、技能管理、工具包/技能包管理

use common::api::{
    AgentListItem, AgentQueryRequest, BindToolToAgentRequest, CancelThinkingRequest,
    CancelThinkingResponse, CompleteAgentOffboardRequest, CreateAgentRequest, CreateAgentResponse,
    CreateExternalAgentRequest, CreateExternalAgentResponse, CreateOntologyClassRequest,
    CreateOntologyClassResponse, CreateOntologyRelationTypeRequest,
    CreateOntologyRelationTypeResponse, CreateOntologySynonymRequest,
    CreateOntologySynonymResponse, CreateSkillRequest, CreateSkillResponse,
    DeleteOntologySynonymResponse, DeleteSkillResponse, GetAgentRequest, GetAgentResponse,
    GetDriftDashboardRequest, GetDriftDashboardResponse, GetReceptionAgentResponse,
    GetSkillFileContentRequest, GetSkillResponse, InstallSkillPackRequest, InstallToolPackRequest,
    ListAgentsRequest, ListDriftClassDetailsRequest, ListDriftClassDetailsResponse,
    ListDriftRelationDetailsRequest, ListDriftRelationDetailsResponse,
    ListExpiredAgentSkillsRequest, ListExpiredAgentSkillsResponse, ListInstalledSkillPacksResponse,
    ListInstalledToolPacksResponse, ListOntologyClassesRequest, ListOntologyClassesResponse,
    ListOntologyLexiconResponse, ListOntologyRelationTypesRequest,
    ListOntologyRelationTypesResponse, ListOntologySynonymsRequest, ListOntologySynonymsResponse,
    OnboardAgentRequest, PagedResult, QueryMemoryParams, QueryMemoryResponse,
    RecommendSeedNodesParams, RecommendSeedNodesResponse, RestoreSkillRequest,
    RestoreSkillResponse, RetireOntologyClassResponse, RetireOntologyRelationTypeResponse,
    RuntimeStatusRequest, RuntimeStatusResponse, SearchAgentsRequest, SearchMemoryParams,
    SearchMemoryResponse, SearchSkillsRequest, SelectAgentCareerRequest, SkillListItem,
    SkillQueryRequest, StartAgentOffboardRequest, UnbindToolFromAgentRequest,
    UninstallSkillPackRequest, UninstallToolPackRequest, UpdateAgentRequest, UpdateAgentResponse,
    UpdateAgentStatusRequest, UpdateAgentStatusResponse, UpdateOntologyClassRequest,
    UpdateOntologyClassResponse, UpdateOntologyRelationTypeRequest,
    UpdateOntologyRelationTypeResponse, UpdateSkillFileContentRequest, UpdateSkillRequest,
    UpdateSkillResponse,
};
use common::enums::OntologyStatus;

use super::{
    ApiError, api_delete, api_delete_with_response, api_get, api_get_or_default, api_post,
    api_post_empty, api_put, api_put_empty,
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

pub async fn search_agents(
    req: &SearchAgentsRequest,
) -> Result<PagedResult<AgentListItem>, ApiError> {
    api_post("/api/v1/hr/agents/search", req).await
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
        // 工具/技能扁平列表开关：后端以 query 读取，漏传会默认 false 导致
        // tool_list / skill_list 不装配，详情页"工具与技能"tab 仅显示 packs。
        ("with_tools", req.with_tools.map(|v| v.to_string())),
        ("with_skills", req.with_skills.map(|v| v.to_string())),
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
    // 直接用 DTO 结构体完整拼接 body（含 id + status）：
    // 前后端共用同一结构体，字段与序列化类型天然一致，避免手拼 json 导致
    // 序列化结果与后端反序列化不匹配（如 status 数字 vs 枚举名、缺 id）。
    api_put_empty(&format!("/api/v1/hr/agents/{}/status", req.id), &req).await
}

/// 职业选择（初创 → 面试中）：按 roles/capabilities 匹配安装个人能力
pub async fn select_agent_career(
    req: SelectAgentCareerRequest,
) -> Result<UpdateAgentStatusResponse, ApiError> {
    api_post(&format!("/api/v1/hr/agents/{}/career", req.id), &req).await
}

/// 入职（待入职 → 已入职）：包装入参决定本次安装哪些组织级包，
/// 不传 packs 时后端回退组织级配置
pub async fn onboard_agent(
    req: OnboardAgentRequest,
) -> Result<UpdateAgentStatusResponse, ApiError> {
    api_post(&format!("/api/v1/hr/agents/{}/onboard", req.id), &req).await
}

/// 发起离职（已入职 → 待离职）：进入交接期，不再接受新业务，
/// 已在运行的业务仍需完成
pub async fn start_agent_offboard(
    req: StartAgentOffboardRequest,
) -> Result<UpdateAgentStatusResponse, ApiError> {
    api_post(
        &format!("/api/v1/hr/agents/{}/offboard/start", req.id),
        &req,
    )
    .await
}

/// 完成离职（待离职 → 已离职）：业务交接后正式下线
pub async fn complete_agent_offboard(
    req: CompleteAgentOffboardRequest,
) -> Result<UpdateAgentStatusResponse, ApiError> {
    api_post(
        &format!("/api/v1/hr/agents/{}/offboard/complete", req.id),
        &req,
    )
    .await
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
    let qs =
        super::build_query_string(&[("delete_copies", req.delete_copies.map(|v| v.to_string()))]);
    api_delete(&format!(
        "/api/v1/hr/agents/{}/skill-packs/{}{}",
        req.agent_id, req.tag, qs
    ))
    .await
}

/// Agent 进修（在职学习入口）
///
/// 把 Agent 的能力补齐到其当前职业/组织要求的最新状态：
/// 1. 补修基础课：缺失基础包补装（neural / skill_management / tool_management）；
/// 2. 学习技能更新：已安装技能包检测新增已发布技能并重装补全；
/// 3. 补学新课：按已走过的状态机边重跑职业匹配（非初创）与组织要求包（仅已入职）。
pub async fn train_agent(agent_id: &str) -> Result<common::api::TrainAgentResponse, ApiError> {
    api_post(
        &format!("/api/v1/hr/agents/{}/train", agent_id),
        &serde_json::json!({}),
    )
    .await
}

// ===== Agent 单技能管理 =====

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

/// 拉取 Agent 名下**仅 Expired** 的技能副本（与 list_agent_skills 互斥）。
/// 前端「📦 已过期技能」虚拟 pack 首次点击时懒加载一次，后续本地缓存并信号维护。
pub async fn list_expired_agent_skills(
    req: ListExpiredAgentSkillsRequest,
) -> Result<ListExpiredAgentSkillsResponse, ApiError> {
    api_get(&format!(
        "/api/v1/hr/agents/{}/skills/expired",
        req.agent_id
    ))
    .await
}

/// 恢复一个 Expired 技能为 Draft；仅当前 status=Expired 时允许操作。
/// 返回恢复后的 SkillDetail，供前端把 skill 从过期列表移回主活动列表（局部刷新）。
pub async fn restore_skill(req: RestoreSkillRequest) -> Result<RestoreSkillResponse, ApiError> {
    api_post(
        &format!("/api/v1/hr/skills/{}/restore", req.skill_id),
        &serde_json::json!({}),
    )
    .await
}

// ===== 技能库管理 =====
//
// 列表场景统一走 query_skills（支持条件过滤 + 分页）；原 list_skills 语法糖接口无调用方，已移除。

pub async fn query_skills(req: &SkillQueryRequest) -> Result<PagedResult<SkillListItem>, ApiError> {
    api_post("/api/v1/hr/skills/query", req).await
}

pub async fn search_skills(
    req: &SearchSkillsRequest,
) -> Result<PagedResult<SkillListItem>, ApiError> {
    api_post("/api/v1/hr/skills/search", req).await
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
        .delete(crate::config::current_config().api_url(&format!("/api/v1/hr/skills/{}", id)))
        .send()
        .await
        .map_err(|e| super::network_err(e, &format!("/api/v1/hr/skills/{}", id)))?;
    let status = resp.status();
    if !status.is_success() {
        super::handle_unauthorized(status.as_u16());
        return Err(super::parse_error_response(resp, &format!("/api/v1/hr/skills/{}", id)).await);
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

/// 推荐知识图谱起点节点（按关联度数 Top N）
pub async fn recommend_seed_nodes(
    req: &RecommendSeedNodesParams,
) -> Result<RecommendSeedNodesResponse, ApiError> {
    api_post("/api/v1/hr/agents/recommend_seed_nodes", req).await
}

// ===== Agent 运行时 =====

/// 查询 Agent 运行时状态 + 思考运行时快照
/// GET /api/v1/hr/agents/{id}/runtime-status
pub async fn get_runtime_status(
    req: RuntimeStatusRequest,
) -> Result<RuntimeStatusResponse, ApiError> {
    api_get(&format!("/api/v1/hr/agents/{}/runtime-status", req.id)).await
}

/// 取消 Agent 正在进行的思考
/// POST /api/v1/hr/agents/{id}/cancel-thinking
pub async fn cancel_thinking(
    req: CancelThinkingRequest,
) -> Result<CancelThinkingResponse, ApiError> {
    api_post(
        &format!("/api/v1/hr/agents/{}/cancel-thinking", req.id),
        &(),
    )
    .await
}

// ===== 本体论词表管理 =====

/// OntologyStatus 的 query 取值 = serde JSON 变体名
///
/// 数字枚举无 rename：query 传 1/0 会被 Params 宏的类型推断转为 JSON Number，
/// serde 反序列化失败（纯 query 分支直接 400）；传小写也不匹配变体名（大小写敏感）。
fn ontology_status_query_value(status: OntologyStatus) -> String {
    match status {
        OntologyStatus::Active => "Active".to_string(),
        OntologyStatus::Retired => "Retired".to_string(),
    }
}

/// 列出实体类词条（分页 + 状态/关键词筛选）
/// GET /api/v1/hr/ontology/classes?status=&keyword=&limit=&offset=
pub async fn list_ontology_classes(
    req: ListOntologyClassesRequest,
) -> Result<ListOntologyClassesResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("status", req.status.map(ontology_status_query_value)),
        ("keyword", req.keyword.clone()),
        ("limit", req.pagination.limit.map(|v| v.to_string())),
        ("offset", req.pagination.offset.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/hr/ontology/classes{}", qs)).await
}

/// 创建实体类（term_key 重复创建报错）
/// POST /api/v1/hr/ontology/classes
pub async fn create_ontology_class(
    req: CreateOntologyClassRequest,
) -> Result<CreateOntologyClassResponse, ApiError> {
    api_post("/api/v1/hr/ontology/classes", &req).await
}

/// 更新实体类（term_key 创建后不可改）
/// PUT /api/v1/hr/ontology/classes/{id}
pub async fn update_ontology_class(
    req: UpdateOntologyClassRequest,
) -> Result<UpdateOntologyClassResponse, ApiError> {
    api_put(&format!("/api/v1/hr/ontology/classes/{}", req.id), &req).await
}

/// 退役实体类（软删除 status=Retired；历史存量引用仍可解释）
/// DELETE /api/v1/hr/ontology/classes/{id}
pub async fn retire_ontology_class(id: &str) -> Result<RetireOntologyClassResponse, ApiError> {
    api_delete_with_response(&format!("/api/v1/hr/ontology/classes/{}", id)).await
}

/// 列出关系类型词条（分页 + 状态/关键词筛选）
/// GET /api/v1/hr/ontology/relation-types?status=&keyword=&limit=&offset=
pub async fn list_ontology_relation_types(
    req: ListOntologyRelationTypesRequest,
) -> Result<ListOntologyRelationTypesResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("status", req.status.map(ontology_status_query_value)),
        ("keyword", req.keyword.clone()),
        ("limit", req.pagination.limit.map(|v| v.to_string())),
        ("offset", req.pagination.offset.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/hr/ontology/relation-types{}", qs)).await
}

/// 创建关系类型（term_key 重复创建报错）
/// POST /api/v1/hr/ontology/relation-types
pub async fn create_ontology_relation_type(
    req: CreateOntologyRelationTypeRequest,
) -> Result<CreateOntologyRelationTypeResponse, ApiError> {
    api_post("/api/v1/hr/ontology/relation-types", &req).await
}

/// 更新关系类型（term_key 创建后不可改）
/// PUT /api/v1/hr/ontology/relation-types/{id}
pub async fn update_ontology_relation_type(
    req: UpdateOntologyRelationTypeRequest,
) -> Result<UpdateOntologyRelationTypeResponse, ApiError> {
    api_put(
        &format!("/api/v1/hr/ontology/relation-types/{}", req.id),
        &req,
    )
    .await
}

/// 退役关系类型（软删除 status=Retired；历史存量边仍可解释）
/// DELETE /api/v1/hr/ontology/relation-types/{id}
pub async fn retire_ontology_relation_type(
    id: &str,
) -> Result<RetireOntologyRelationTypeResponse, ApiError> {
    api_delete_with_response(&format!("/api/v1/hr/ontology/relation-types/{}", id)).await
}

/// 列出同义映射（分页 + 目标词条种类/规范词筛选）
/// GET /api/v1/hr/ontology/synonyms?target_kind=&target_key=&limit=&offset=
pub async fn list_ontology_synonyms(
    req: ListOntologySynonymsRequest,
) -> Result<ListOntologySynonymsResponse, ApiError> {
    let qs = super::build_query_string(&[
        (
            "target_kind",
            req.target_kind.map(|k| k.as_str().to_string()),
        ),
        ("target_key", req.target_key.clone()),
        ("limit", req.pagination.limit.map(|v| v.to_string())),
        ("offset", req.pagination.offset.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/hr/ontology/synonyms{}", qs)).await
}

/// 创建同义映射（同一 (raw_term, target_kind) 重复创建报错）
/// POST /api/v1/hr/ontology/synonyms
pub async fn create_ontology_synonym(
    req: CreateOntologySynonymRequest,
) -> Result<CreateOntologySynonymResponse, ApiError> {
    api_post("/api/v1/hr/ontology/synonyms", &req).await
}

/// 删除同义映射（物理删除，相关词条自然回落漂移，下一次解析即生效）
/// DELETE /api/v1/hr/ontology/synonyms/{id}
pub async fn delete_ontology_synonym(id: &str) -> Result<DeleteOntologySynonymResponse, ApiError> {
    api_delete_with_response(&format!("/api/v1/hr/ontology/synonyms/{}", id)).await
}

/// 词表注入视图（管理页只读展示，与神经技能注入共用同一契约）
/// GET /api/v1/hr/ontology/lexicon
pub async fn list_ontology_lexicon() -> Result<ListOntologyLexiconResponse, ApiError> {
    api_get("/api/v1/hr/ontology/lexicon").await
}

/// 漂移看板（Top N 漂移词 / 关系覆盖率 / 漂移节点占比，SQLite 读路径惰性聚合）
/// GET /api/v1/hr/ontology/drift/dashboard?top_n=&agent_id=
pub async fn get_ontology_drift_dashboard(
    req: GetDriftDashboardRequest,
) -> Result<GetDriftDashboardResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("top_n", req.top_n.map(|v| v.to_string())),
        ("agent_id", req.agent_id.clone()),
    ]);
    api_get(&format!("/api/v1/hr/ontology/drift/dashboard{}", qs)).await
}

/// 漂移词下钻：关系（边）明细（raw_term 兼容漂移原文与规范词 key）
/// GET /api/v1/hr/ontology/drift/relations?raw_term=&agent_id=&limit=&offset=
pub async fn list_ontology_drift_relation_details(
    req: ListDriftRelationDetailsRequest,
) -> Result<ListDriftRelationDetailsResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("raw_term", Some(req.raw_term.clone())),
        ("agent_id", req.agent_id.clone()),
        ("limit", req.pagination.limit.map(|v| v.to_string())),
        ("offset", req.pagination.offset.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/hr/ontology/drift/relations{}", qs)).await
}

/// 漂移词下钻：节点明细（raw_term 语义同关系下钻）
/// GET /api/v1/hr/ontology/drift/classes?raw_term=&agent_id=&limit=&offset=
pub async fn list_ontology_drift_class_details(
    req: ListDriftClassDetailsRequest,
) -> Result<ListDriftClassDetailsResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("raw_term", Some(req.raw_term.clone())),
        ("agent_id", req.agent_id.clone()),
        ("limit", req.pagination.limit.map(|v| v.to_string())),
        ("offset", req.pagination.offset.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/hr/ontology/drift/classes{}", qs)).await
}
