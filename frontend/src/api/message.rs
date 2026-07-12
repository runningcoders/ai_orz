//! Message 域 API - 消息发送

use common::api::SendMessageToAgentParams;

use super::api_post;

/// 用户向 Agent 发送消息
pub async fn send_message_to_agent(params: SendMessageToAgentParams) -> Result<common::api::SendMessageToAgentResponse, String> {
    api_post("/api/v1/finance/messages/agents", &params).await
}
