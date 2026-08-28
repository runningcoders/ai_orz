//! 消息辅助工具 - 消息类型常量、角色映射、乐观消息辅助

use common::api::MessageListItem;

use super::time::now_ms;

/// 消息类型常量
pub const MSG_TEXT: i32 = 0;
pub const MSG_IMAGE: i32 = 1;
pub const MSG_FILE: i32 = 2;
pub const MSG_AUDIO: i32 = 3;
pub const MSG_VIDEO: i32 = 4;
pub const MSG_TOOL_CALL_REQUEST: i32 = 5;
pub const MSG_TOOL_CALL_RESULT: i32 = 6;
pub const MSG_TASK_ASSIGNMENT: i32 = 9;

/// 判断是否为附件消息（图片/文件/音频/视频）
pub fn is_attachment_message(msg_type: i32) -> bool {
    matches!(msg_type, MSG_IMAGE | MSG_FILE | MSG_AUDIO | MSG_VIDEO)
}

/// 角色 → 头像字符（0=User, 1=Agent, 2=System）
pub fn role_avatar(role: i32) -> &'static str {
    match role {
        0 => "U",
        1 => "A",
        2 => "S",
        _ => "?",
    }
}

/// 角色 → 中文名（0=用户, 1=Agent, 2=系统）
///
/// 用于气泡头部的角色标签，以及发送者名字查不到时的兜底。
pub fn role_label(role: i32) -> &'static str {
    match role {
        0 => "用户",
        1 => "Agent",
        2 => "系统",
        _ => "未知",
    }
}

/// 角色 → CSS class 名（0=user, 1=agent, 2=system）
pub fn role_class(role: i32) -> &'static str {
    match role {
        0 => "user",
        1 => "agent",
        2 => "system",
        _ => "other",
    }
}

/// 名称查找表：实体 ID → 展示名
///
/// 由 `store::directory::Directory` 预载并缓存（Agent 全量 + 组织用户全量）。
pub type NameMap = std::collections::HashMap<String, String>;

/// 解析消息发送者的展示名
///
/// `MessageListItem` 只带 `from_id` / `from_role`，不含发送者名字，
/// 需要按角色到对应的名称表里查：
/// - `from_role = 2`（System）→ 直接返回「系统」
/// - `from_role = 1`（Agent）→ 查 Agent 名称表
/// - `from_role = 0`（User）→ 查用户名称表
///
/// 查不到时回退到 `角色 + ID 前 6 位`，**绝不返回 "user"/"agent" 这类英文角色码**
/// （历史上 message_bubble 曾直接把 `role_class()` 当可见文本渲染，气泡里会显示
/// "user"/"agent" 字样）。
pub fn resolve_sender_name(msg: &MessageListItem, agents: &NameMap, users: &NameMap) -> String {
    match msg.from_role {
        2 => "系统".to_string(),
        1 => agents
            .get(&msg.from_id)
            .cloned()
            .unwrap_or_else(|| short_id(&msg.from_id, "Agent")),
        0 => users
            .get(&msg.from_id)
            .cloned()
            // 查不到 → 就是当前登录用户：组织成员已由 `list_users` 全量预载，
            // 而乐观插入的自发消息 `from_id` 是占位符 "user"
            // （见 build_optimistic_user_msg），本就不该拿去查表。
            .unwrap_or_else(|| "我".to_string()),
        _ => "未知".to_string(),
    }
}

/// 回退展示：`<前缀> <ID 前 6 位>`，避免把一长串内部 ID 直接铺到 UI 上
pub fn short_id(id: &str, prefix: &str) -> String {
    if id.is_empty() {
        return prefix.to_string();
    }
    // 按字符边界截断，避免多字节字符被切坏
    let short: String = id.chars().take(6).collect();
    format!("{} {}", prefix, short)
}

/// 生成乐观消息的临时 ID（tmp_<ms>_<random>，避免同毫秒碰撞）
pub fn tmp_msg_id() -> String {
    let random = (js_sys::Math::random() * 1_000_000_000.0) as u32;
    format!("tmp_{}_{:09}", now_ms(), random)
}

/// 用真实消息替换同 content 的乐观消息（tmp_ 前缀）。
/// 只移除第一条匹配，避免连发同内容消息时误删。
/// 如果不存在匹配的 tmp_ 消息，则不做任何操作（真实消息可能是重复推送）。
pub fn replace_tmp_with_real(msgs: &mut Vec<MessageListItem>, real_msg: &MessageListItem) {
    if let Some(pos) = msgs
        .iter()
        .position(|m| m.message_id.starts_with("tmp_") && m.content == real_msg.content)
    {
        msgs.remove(pos);
    }
}

/// 构造乐观用户消息（发送成功后立即显示，SSE 真实消息到达后由 replace_tmp_with_real 替换）
pub fn build_optimistic_user_msg(
    content: String,
    project_id: Option<String>,
    task_id: Option<String>,
    to_agent_id: Option<String>,
) -> MessageListItem {
    MessageListItem {
        message_id: tmp_msg_id(),
        project_id,
        task_id,
        from_id: "user".to_string(),
        from_role: 0,
        to_id: to_agent_id.unwrap_or_default(),
        to_role: 1,
        message_type: MSG_TEXT,
        status: 3,
        content,
        reply_to_id: None,
        created_at: now_ms(),
        file_type: None,
        file_meta: None,
    }
}
