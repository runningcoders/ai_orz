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
use crate::pkg::RequestContext;
use crate::service::domain::runtime::domain as runtime_domain;
use common::api::{
    AgentCliConfig, AgentExternalConfigInfo, AgentRemoteConfig, AgentRuntimeConfigInfo,
    KnowledgePropertyParam, UpdateAgentStatusResponse,
};
use common::enums::ontology::PropertyTargetType;
use common::enums::{AgentKind, AgentRuntimeState};
use common::error::Result;
use common::ontology_property::PropertyEntry;

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
/// 门闩（类型 / 值域 / 基数 / 载体）全在 DAL，这里只做「入参 → 回执文案」的翻译。
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
) -> Result<Vec<String>> {
    let Some(properties) = properties else {
        return Ok(Vec::new());
    };
    if properties.is_empty() {
        return Ok(Vec::new());
    }

    let entries: Vec<PropertyEntry> = properties.into_iter().map(|p| p.into_entry()).collect();
    let report = runtime_domain()
        .memory()
        .save_entity_properties(ctx.clone(), PropertyTargetType::Node, node_id, entries)
        .await?;

    Ok(report
        .issues
        .iter()
        .map(|i| format!("{}: {}", i.term_key, i.kind.describe()))
        .collect())
}
