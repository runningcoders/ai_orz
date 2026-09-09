//! Message 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件。

pub mod list_messages;
pub mod search_messages;
pub mod send_message;
pub mod send_message_to_agent;
pub mod send_task_assignment_message;
pub mod subscribe_sse;

pub use list_messages::list_messages_handler;
pub use search_messages::search_messages_handler;
pub use send_message::send_message_handler;
pub use send_message_to_agent::send_message_to_agent_handler;
pub use send_task_assignment_message::send_task_assignment_message_handler;
pub use subscribe_sse::subscribe_sse_handler;

use crate::pkg::RequestContext;

/// 回复 id 自动兜底：Agent 在思考过程中调用消息工具且未显式指定 reply_to_id 时，
/// 默认挂到它当前正在处理的消息上（运行时状态里登记的 current_message_id），
/// 保证「消息唤醒 → Agent 回复」天然续链；非思考上下文（用户/HTTP 直调）返回 None。
pub(crate) fn auto_reply_to_id(ctx: &RequestContext, explicit: Option<&str>) -> Option<String> {
    if explicit.is_some() {
        return explicit.map(|s| s.to_string());
    }
    let caller = ctx.caller_id_or_system();
    if ctx.caller_role() != common::enums::MessageRole::Agent || caller.is_empty() {
        return None;
    }
    crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global()
        .get(&caller)
        .and_then(|info| info.current_message_id)
}
