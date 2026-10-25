//! Agent 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

pub mod association;
pub mod cancel_thinking;
pub mod complete_agent_offboard;
pub mod create_agent;
pub mod create_external_agent;
pub mod create_memory;
pub mod delete_agent;
pub mod delete_memory;
pub mod get_agent;
pub mod get_knowledge_graph;
pub mod get_reception_agent;
pub mod install_skill_pack;
pub mod install_tool_pack;
pub mod list_agents;
pub mod list_installed_skill_packs;
pub mod list_installed_tool_packs;
pub mod onboard_agent;
pub mod query_agents;
pub mod query_memory;
pub mod query_memory_traces;
pub mod query_think_traces;
pub mod recommend_seed_nodes;
pub mod runtime_list;
pub mod runtime_status;
pub mod save_long_term_memory;
pub mod save_short_term_memory;
pub mod search_agents;
pub mod search_memory;
pub mod select_agent_career;
pub mod settle_memory;
pub mod start_agent_offboard;
pub mod train_agent;
pub mod uninstall_skill_pack;
pub mod uninstall_tool_pack;
pub mod update_agent;
pub mod update_agent_status;
pub mod update_memory;

pub use cancel_thinking::cancel_thinking_handler;
pub use complete_agent_offboard::complete_agent_offboard_handler;
pub use create_agent::create_agent_handler;
pub use create_external_agent::create_external_agent_handler;
pub use create_memory::create_memory_handler;
pub use delete_agent::delete_agent_handler;
pub use delete_memory::delete_memory_handler;
pub use get_agent::get_agent_handler;
pub use get_knowledge_graph::get_knowledge_graph_handler;
pub use get_reception_agent::get_reception_agent_handler;
pub use install_skill_pack::install_skill_pack_handler;
pub use install_tool_pack::install_tool_pack_handler;
pub use list_agents::list_agents_handler;
pub use list_installed_skill_packs::list_installed_skill_packs_handler;
pub use list_installed_tool_packs::list_installed_tool_packs_handler;
pub use onboard_agent::onboard_agent_handler;
pub use query_agents::query_agents_handler;
pub use query_memory::query_memory_handler;
pub use query_memory_traces::{get_memory_trace_handler, query_memory_traces_handler};
pub use query_think_traces::query_think_traces_handler;
pub use recommend_seed_nodes::recommend_seed_nodes_handler;
pub use runtime_list::runtime_list_handler;
pub use runtime_status::runtime_status_handler;
pub use save_long_term_memory::save_long_term_memory_handler;
pub use save_short_term_memory::save_short_term_memory_handler;
pub use search_agents::search_agents_handler;
pub use search_memory::search_memory_handler;
pub use select_agent_career::select_agent_career_handler;
pub use settle_memory::settle_memory_handler;
pub use start_agent_offboard::start_agent_offboard_handler;
pub use train_agent::train_agent_handler;
pub use uninstall_skill_pack::uninstall_skill_pack_handler;
pub use uninstall_tool_pack::uninstall_tool_pack_handler;
pub use update_agent::update_agent_handler;
pub use update_agent_status::update_agent_status_handler;
pub use update_memory::update_memory_handler;

// ==================== 共享序列化 ====================

use crate::models::agent::{Agent, ExternalAgentConfig};
use crate::models::memory::{KnowledgeNodeRelationPo, MemoryCreateParams, MemoryPo};
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::domain as runtime_domain;
use common::api::{
    AgentCliConfig, AgentExternalConfigInfo, AgentRemoteConfig, AgentRuntimeConfigInfo,
    KnowledgePropertyParam, KnowledgeRelationParam, PropertyIssueEntry, RelationWriteResult,
    RelationWriteStatus, UpdateAgentStatusResponse,
};
use common::enums::ontology::PropertyTargetType;
use common::enums::{AgentKind, AgentRuntimeState, KnowledgeRelationStatus, MemoryStatus};
use common::error::{ErrorCode, Result};
use common::ontology_property::{PropertyEntry, PropertySaveReport, PropertyWriteIssue};

/// 把 Agent 实体序列化为「状态流转类」接口的统一响应
///
/// 通用状态流转 / 职业选择 / 入职三个语义化 handler 共用，
/// 避免同一份拼装逻辑复制三遍后各自漂移。
pub fn build_status_response(agent: &Agent) -> UpdateAgentStatusResponse {
    let capabilities: Vec<String> = agent.po.get_capabilities();
    let roles: Vec<String> = agent.po.get_roles();
    let kind = agent.po.kind;

    let external_config = match kind {
        AgentKind::Local => None,
        AgentKind::Cli | AgentKind::Remote => {
            let runtime_config = agent.po.get_runtime_config();
            match runtime_config.external_config {
                Some(ExternalAgentConfig::Cli {
                    command,
                    args,
                    work_dir,
                    env: _,
                    timeout_secs,
                    prompt_template,
                }) => Some(AgentExternalConfigInfo {
                    cli: Some(AgentCliConfig {
                        command,
                        args,
                        work_dir,
                        timeout_secs,
                        prompt_template,
                    }),
                    remote: None,
                }),
                Some(ExternalAgentConfig::Remote {
                    endpoint,
                    agent_name,
                    auth_token: _,
                    timeout_secs,
                }) => Some(AgentExternalConfigInfo {
                    cli: None,
                    remote: Some(AgentRemoteConfig {
                        endpoint,
                        agent_name,
                        timeout_secs,
                    }),
                }),
                None => None,
            }
        }
    };

    let (runtime_state, current_message_id, current_task_id, current_project_id) =
        match &agent.runtime_info {
            Some(info) => (
                info.state as i32,
                info.current_message_id.clone(),
                info.task_id.clone(),
                info.project_id.clone(),
            ),
            None => (AgentRuntimeState::Idle as i32, None, None, None),
        };

    // 上下文长度：最近一次 LLM 调用的 prompt token 数（纯内存，未思考过为 None）
    let context_length = agent
        .runtime_info
        .as_ref()
        .map(|info| info.context_length)
        .filter(|v| *v > 0);
    // 压缩阈值：与 ContextOverflowPolicy 同源，原始值直出（百分比由前端算）
    let context_length_threshold = agent
        .runtime_info
        .as_ref()
        .map(|info| info.context_length_threshold)
        .filter(|v| *v > 0);

    // 构造运行时配置信息（思考轮次 / 超时等用户可调参数）
    let runtime_config = {
        let rc = agent.po.get_runtime_config();
        Some(AgentRuntimeConfigInfo {
            max_thinking_depth: rc.max_thinking_depth,
            max_thinking_rounds: rc.max_thinking_rounds,
            intent_analyze_max_rounds: rc.intent_analyze_max_rounds,
            summary_max_rounds: rc.summary_max_rounds,
            think_timeout_secs: rc.think_timeout_secs,
            compaction_threshold_tokens: rc.compaction_threshold_tokens,
        })
    };

    UpdateAgentStatusResponse {
        id: agent.id().to_string(),
        name: agent.name().to_string(),
        roles,
        description: if agent.po.description.is_empty() {
            None
        } else {
            Some(agent.po.description.clone())
        },
        capabilities: if capabilities.is_empty() {
            None
        } else {
            Some(capabilities)
        },
        soul: if agent.po.soul.is_empty() {
            None
        } else {
            Some(agent.po.soul.clone())
        },
        kind: kind.to_string(),
        model_provider_id: agent.po.model_provider_id.clone(),
        external_config,
        runtime_config,
        status: agent.po.status as i32,
        created_at: agent.po.created_at,
        updated_at: agent.po.updated_at,
        runtime_state,
        current_message_id,
        current_task_id,
        current_project_id,
        context_length,
        context_length_threshold,
        tool_list: None,
        skill_list: None,
        stats: None,
        model_call_stats: None,
    }
}

/// 写入知识节点属性（部分成功语义：坏条目只跳过，不中断）
///
/// 门闩（类型 / 值域 / 基数 / 载体）全在 DAL，这里透传 `PropertySaveReport`，
/// 回执文案由调用方按 V1（字符串）/ V2（结构化）各自翻译。
/// 空入参直接返回空回执，不打扰 DAL。
///
/// `save_long_term_memory`（新建）与 `update_memory`（既有节点补 / 改）共用一份：
/// 属性是**独立于记忆主体**的一步写入——
/// - 写失败不该连累节点本身（节点已经建好 / 改好了，回滚代价远大于留一个缺属性的节点）；
/// - 但回执**必须**回给模型：静默吞掉会让它以为写成功了，下次不再补。
pub(crate) async fn save_node_properties(
    ctx: &RequestContext,
    node_id: &str,
    properties: Option<Vec<KnowledgePropertyParam>>,
) -> Result<PropertySaveReport> {
    let Some(properties) = properties else {
        return Ok(PropertySaveReport::default());
    };
    if properties.is_empty() {
        return Ok(PropertySaveReport::default());
    }

    let entries: Vec<PropertyEntry> = properties.into_iter().map(|p| p.into_entry()).collect();
    let report = runtime_domain()
        .memory()
        .save_entity_properties(ctx.clone(), PropertyTargetType::Node, node_id, entries)
        .await?;

    Ok(report)
}

/// 属性回执 V1：`"<term_key>: <原因>"` 字符串数组（既有契约，过渡期保留）。
pub(crate) fn property_issue_strings(report: &PropertySaveReport) -> Vec<String> {
    report
        .issues
        .iter()
        .map(|i| format!("{}: {}", i.term_key, i.kind.describe()))
        .collect()
}

/// 属性回执 V2：结构化 `{code, message, field}`，与平台 `Error{code, msg, field}` 对齐
/// （复用既有 error 词表的属性域错误码；`field` 至少携带 `term_key`，
/// 类型不符时另带 `expected`——把原来只在 describe 文案里的上下文结构化带出来）。
pub(crate) fn property_issue_entries(report: &PropertySaveReport) -> Vec<PropertyIssueEntry> {
    report
        .issues
        .iter()
        .map(|i| {
            let code = match &i.kind {
                PropertyWriteIssue::TypeMismatch { .. } => ErrorCode::PropertyTypeMismatch,
                PropertyWriteIssue::NumberOutOfRange => ErrorCode::PropertyNumberOutOfRange,
                PropertyWriteIssue::NotInEnumValue => ErrorCode::PropertyNotInEnumValue,
                PropertyWriteIssue::CardinalityExceeded => ErrorCode::PropertyCardinalityExceeded,
                PropertyWriteIssue::TargetNotAllowed => ErrorCode::PropertyTargetNotAllowed,
                PropertyWriteIssue::UndefinedProperty => ErrorCode::PropertyUndefined,
            };
            let mut field = serde_json::json!({ "term_key": i.term_key });
            if let PropertyWriteIssue::TypeMismatch { expected } = &i.kind {
                field["expected"] = serde_json::Value::String(expected.as_str().to_string());
            }
            PropertyIssueEntry {
                code: code.code_str().to_string(),
                message: i.kind.describe(),
                field,
            }
        })
        .collect()
}

/// 逐条建边的 keyed 回执：`results` 与 `relation_ids` 严格一致（success 项的
/// `relation_id` 就是实际落库的边 ID，`relation_ids` 只收这些 ID）。
pub(crate) struct RelationBatchReceipt {
    pub results: Vec<RelationWriteResult>,
    pub relation_ids: Vec<String>,
}

/// 已知占位符哨兵（AMan 拍板⑤：占位符直接拒绝，绝不落半悬空边）。
/// 归一化到小写后精确匹配；其余格式非法的 ID 走 `invalid_node_id`。
fn is_placeholder_node_id(raw: &str) -> bool {
    matches!(
        raw.to_ascii_lowercase().as_str(),
        "self" | "kn_self" | "kn_new_self"
    )
}

/// 节点 ID 形态校验：知识节点 ID 一律 `kn_` 前缀（handler 生成路径唯一）。
/// 名称、空串、其他前缀都算格式非法。
fn is_valid_node_id(raw: &str) -> bool {
    raw.starts_with("kn_") && raw.len() > "kn_".len()
}

/// 建边批量写入的共享实现（`save_long_term_memory` 新建通道与 `update_memory`
/// 补边通道共用一份，保证两条路径校验口径与回执形态一致）。
///
/// 语义（AMan 拍板①②⑤）：
/// - 整批**永不 Err**：占位符 / 格式非法 / 批内重复 / 端点不存在 / 落库失败
///   全部只进 `RelationWriteResult.status = Failed`，调用方拿 200 + 逐项回执；
/// - key = `{index}:{source_node_id}->{target_node_id}`（index 为主、复合串辅助）；
/// - 占位符（SELF / kn_new_self 等）直接拒绝（`placeholder_node_id_rejected`）；
///   非 `kn_` 前缀拒绝（`invalid_node_id`）——绝不落半悬空边；
/// - 批内同键（source + target + relation_type 归一后）重复：首条照写
///   （版本化替换由 DAO 承担），后续条报 `duplicate_active_edge`；
/// - 端点存在性**批量预检**（一次查询核全部端点，只认 Active 节点）：
///   缺失按 source/target 分码拒绝（`source_node_not_found` / `target_node_not_found`）；
///   预检本身失败（基础设施错误）时，底层 Error 的 code/msg/field 原样带进
///   每个待写条目——把原来只打日志的错误带到外边来；
/// - 校验通过的条目逐条独立落库：单条底层失败不影响其他条目。
pub(crate) async fn write_relations_with_receipt(
    ctx: &RequestContext,
    relations: Vec<KnowledgeRelationParam>,
    now: i64,
) -> RelationBatchReceipt {
    struct Pending {
        key: String,
        param: KnowledgeRelationParam,
    }

    let mut results: Vec<RelationWriteResult> = Vec::with_capacity(relations.len());
    let mut pending: Vec<Pending> = Vec::new();

    // 收口：回执按 key 的 index 段排序，保证逐项结果与入参顺序一致
    // （静态拒绝 / 批内去重 / 存在性缺失 / 落库失败在不同阶段产出，统一归位）。
    fn finalize(
        results: Vec<RelationWriteResult>,
        relation_ids: Vec<String>,
    ) -> RelationBatchReceipt {
        let mut results = results;
        results.sort_by_key(|r| {
            r.key
                .split(':')
                .next()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(usize::MAX)
        });
        RelationBatchReceipt {
            results,
            relation_ids,
        }
    }

    // —— 第 1 步：静态校验（占位符 → ID 格式），失败项直接进回执 ——
    for (index, r) in relations.into_iter().enumerate() {
        let key = format!(
            "{}:{}->{}",
            index,
            r.source_node_id.trim(),
            r.target_node_id.trim()
        );
        let src = r.source_node_id.trim();
        let tgt = r.target_node_id.trim();
        let rejected = if is_placeholder_node_id(src) {
            Some((src, ErrorCode::PlaceholderNodeIdRejected))
        } else if is_placeholder_node_id(tgt) {
            Some((tgt, ErrorCode::PlaceholderNodeIdRejected))
        } else if !is_valid_node_id(src) {
            Some((src, ErrorCode::InvalidNodeId))
        } else if !is_valid_node_id(tgt) {
            Some((tgt, ErrorCode::InvalidNodeId))
        } else {
            None
        };
        match rejected {
            Some((offender, code)) => {
                let message = match code {
                    ErrorCode::PlaceholderNodeIdRejected => format!(
                        "节点 ID `{offender}` 是占位符（SELF / kn_new_self 等），拒绝建边——请改用实际知识节点 ID（kn_ 前缀）后重试本条"
                    ),
                    _ => format!(
                        "节点 ID `{offender}` 格式非法——应为实际存在的知识节点 ID（kn_ 前缀）"
                    ),
                };
                results.push(RelationWriteResult {
                    key,
                    status: RelationWriteStatus::Failed,
                    relation_id: None,
                    error_code: Some(code.code_str().to_string()),
                    message: Some(message),
                    field: Some(serde_json::json!({ "node_id": offender })),
                });
            }
            None => pending.push(Pending { key, param: r }),
        }
    }

    if pending.is_empty() {
        return finalize(results, Vec::new());
    }

    // —— 第 2 步：批内重复检测（同键只放行首条） ——
    let mut seen: std::collections::HashSet<(String, String, String)> =
        std::collections::HashSet::new();
    let mut deduped: Vec<Pending> = Vec::new();
    for p in pending {
        let norm_key = (
            p.param.source_node_id.trim().to_string(),
            p.param.target_node_id.trim().to_string(),
            p.param.relation_type.trim().to_lowercase(),
        );
        if !seen.insert(norm_key) {
            results.push(RelationWriteResult {
                key: p.key,
                status: RelationWriteStatus::Failed,
                relation_id: None,
                error_code: Some(ErrorCode::DuplicateActiveEdge.code_str().to_string()),
                message: Some(
                    "同批内重复的 (source, target, relation_type) 边：首条已受理，本条拒绝（版本化替换语义下同键只保留最后一条生效边）"
                        .to_string(),
                ),
                field: Some(serde_json::json!({
                    "source_node_id": p.param.source_node_id.trim(),
                    "target_node_id": p.param.target_node_id.trim(),
                    "relation_type": p.param.relation_type.trim(),
                })),
            });
            continue;
        }
        deduped.push(p);
    }

    if deduped.is_empty() {
        return finalize(results, Vec::new());
    }

    // —— 第 3 步：端点存在性批量预检（一次查询核全部端点，只认活跃节点） ——
    let mut ids: Vec<String> = deduped
        .iter()
        .flat_map(|p| {
            [
                p.param.source_node_id.trim().to_string(),
                p.param.target_node_id.trim().to_string(),
            ]
        })
        .collect();
    ids.sort();
    ids.dedup();

    let found: std::collections::HashSet<String> = match runtime_domain()
        .memory()
        .query(
            ctx.clone(),
            MemoryQuery {
                ids: Some(ids),
                ..Default::default()
            },
        )
        .await
    {
        Ok(memories) => memories
            .into_iter()
            .filter_map(|m| match m.po {
                MemoryPo::KnowledgeNode(kn) if kn.status == MemoryStatus::Active => Some(kn.id),
                _ => None,
            })
            .collect(),
        Err(e) => {
            // 预检基础设施失败：无法区分「缺失」与「故障」，把底层 Error 原样
            // 带进每个待写条目（200 + 全 failed），不整体上抛（拍板①）
            let code = e.code.code_str().to_string();
            let field = e.field.as_ref().and_then(|f| serde_json::to_value(f).ok());
            for p in deduped {
                results.push(RelationWriteResult {
                    key: p.key,
                    status: RelationWriteStatus::Failed,
                    relation_id: None,
                    error_code: Some(code.clone()),
                    message: Some(format!("节点存在性预检失败: {}", e.msg)),
                    field: field.clone(),
                });
            }
            return finalize(results, Vec::new());
        }
    };

    // —— 第 4 步：逐条独立落库（单条失败不影响其他条目） ——
    let mut relation_ids: Vec<String> = Vec::new();
    for p in deduped {
        let src = p.param.source_node_id.trim().to_string();
        let tgt = p.param.target_node_id.trim().to_string();
        if !found.contains(&src) {
            results.push(missing_endpoint(&p.key, true, &src));
            continue;
        }
        if !found.contains(&tgt) {
            results.push(missing_endpoint(&p.key, false, &tgt));
            continue;
        }

        let po = KnowledgeNodeRelationPo {
            id: format!("kr_{}", uuid::Uuid::now_v7().simple()),
            source_node_id: src,
            target_node_id: tgt,
            // 原文直落：词表外的标注原样保留（与既有口径一致，归一化只发生在 DAO 写侧）
            relation_type: p.param.relation_type.trim().to_string(),
            // 强度归一化在 DTO 上：非有限值丢弃、越界夹紧到 0.0~1.0，未给则 None
            weight: p.param.normalized_weight(),
            // 新建边即生效版本：修正会走「降级旧边 + 插入新边」，历史留库可追溯
            status: KnowledgeRelationStatus::Active,
            created_at: now,
            updated_at: now,
        };
        match runtime_domain()
            .memory()
            .create(ctx.clone(), MemoryCreateParams::CreateRelations(vec![po]))
            .await
        {
            Ok(created) => {
                let rid = created.into_iter().find_map(|m| match m.po {
                    MemoryPo::Relation(r) => Some(r.id),
                    _ => None,
                });
                match rid {
                    Some(rid) => {
                        relation_ids.push(rid.clone());
                        results.push(RelationWriteResult {
                            key: p.key,
                            status: RelationWriteStatus::Success,
                            relation_id: Some(rid),
                            error_code: None,
                            message: None,
                            field: None,
                        });
                    }
                    None => results.push(RelationWriteResult {
                        key: p.key,
                        status: RelationWriteStatus::Failed,
                        relation_id: None,
                        error_code: Some(ErrorCode::Internal.code_str().to_string()),
                        message: Some("落库返回中未找到关系边 ID（内部不一致）".to_string()),
                        field: None,
                    }),
                }
            }
            Err(e) => {
                // 底层/下游抛出的 error 直接赋值进当前条目（AMan 指导）：
                // code/msg/field 全量带回，不再只打日志
                let field = e.field.as_ref().and_then(|f| serde_json::to_value(f).ok());
                results.push(RelationWriteResult {
                    key: p.key,
                    status: RelationWriteStatus::Failed,
                    relation_id: None,
                    error_code: Some(e.code.code_str().to_string()),
                    message: Some(e.msg.clone()),
                    field,
                });
            }
        }
    }

    finalize(results, relation_ids)
}

/// 端点缺失的逐项失败回执（source / target 分码）。
fn missing_endpoint(key: &str, is_source: bool, node_id: &str) -> RelationWriteResult {
    let (code, role) = if is_source {
        (ErrorCode::SourceNodeNotFound, "源")
    } else {
        (ErrorCode::TargetNodeNotFound, "目标")
    };
    RelationWriteResult {
        key: key.to_string(),
        status: RelationWriteStatus::Failed,
        relation_id: None,
        error_code: Some(code.code_str().to_string()),
        message: Some(format!(
            "{role}节点 `{node_id}` 不存在或非活跃状态，拒绝建边"
        )),
        field: Some(serde_json::json!({ "node_id": node_id })),
    }
}
