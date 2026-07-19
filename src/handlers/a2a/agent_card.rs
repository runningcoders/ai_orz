//! Agent Card 端点
//!
//! GET /.well-known/agent.json
//! 公开路由，无需认证。返回组织级能力描述。

use axum::response::Json;
use common::api::a2a::{AgentCapabilities, AgentCard, AgentSkill};

/// Agent Card handler
///
/// 返回组织对外能力描述。对外只暴露一个统一入口，不列具体内部 Agent。
/// 配置通过全局单例 `crate::config::get()` 读取。
pub async fn get_agent_card() -> Json<AgentCard> {
    let config = crate::config::get();

    let card = AgentCard {
        name: "ai_orz 组织".to_string(),
        description: Some("ai_orz 组织对外能力入口".to_string()),
        version: config.a2a_server.protocol_version.clone(),
        url: config.a2a_server.endpoint.clone(),
        capabilities: AgentCapabilities {
            streaming: false,
            push_notifications: false,
        },
        skills: vec![AgentSkill {
            id: "chat".to_string(),
            name: "对话协作".to_string(),
            description: Some("与组织前台 Agent 对话".to_string()),
            tags: vec!["chat".to_string()],
        }],
        default_input_modes: vec!["text".to_string()],
        default_output_modes: vec!["text".to_string()],
    };

    Json(card)
}
