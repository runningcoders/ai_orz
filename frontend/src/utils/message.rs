//! 消息辅助工具 - 消息类型常量、角色映射、乐观消息辅助

use common::api::MessageListItem;

use super::time::now_ms;

/// 历史消息每页条数（初始加载与上拉翻页共用）
///
/// ⚠️ 这不是「最多能看到多少条」，而是「一次拉多少条」：对话页与工作台底部对话框
/// 都支持上拉续拉（`before_timestamp`），滚到顶会继续往前翻，直到确实没有更早的消息。
pub const HISTORY_PAGE_SIZE: usize = 50;

/// 历史翻页时「整页都被过滤空」允许连翻的最大页数
///
/// Global（后端 `project_id=None` 不过滤）与 Agent 私聊（to/from 是「或」关系）
/// 拿不到精确的服务端过滤，一页原始消息可能一条可见的都没有。若就此停手，
/// 用户会「滚到顶什么也没多出来」，而列表没变化就再也不会产生滚动事件。
pub const HISTORY_SCAN_MAX_PAGES: usize = 5;

/// 乐观用户消息的占位发送者 ID（见 [`build_optimistic_user_msg`]）
///
/// 后端推回真实消息前，气泡只能拿到这个占位符；比对「是不是我」时必须特判放行。
pub const OPTIMISTIC_USER_ID: &str = "user";

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

/// 解析消息接收方的展示名
///
/// 与 [`resolve_sender_name`] 对称：按 `to_role` 到对应名称表查 `to_id`。
/// - `to_role = 2`（System）→ 「系统」
/// - `to_role = 1`（Agent）→ Agent 名称表，查不到回退 `Agent + 短 ID`
/// - `to_role = 0`（User）→ 用户名称表；查不到即视为当前登录用户（本机单用户场景）
pub fn resolve_receiver_name(msg: &MessageListItem, agents: &NameMap, users: &NameMap) -> String {
    match msg.to_role {
        2 => "系统".to_string(),
        1 => agents
            .get(&msg.to_id)
            .cloned()
            .unwrap_or_else(|| short_id(&msg.to_id, "Agent")),
        0 => users
            .get(&msg.to_id)
            .cloned()
            .unwrap_or_else(|| "我".to_string()),
        _ => "未知".to_string(),
    }
}

/// 消息是否属于给定项目会话（`project_id = None` 表示「无项目的默认对话」）
///
/// 对话页与工作台的关系图对话框共用这一条判定 —— 两边的「历史加载 / 上拉翻页 /
/// SSE 推送」都必须与它同口径，否则会出现「实时推来的看得到、一刷新就没了」。
pub fn in_project_context(msg: &MessageListItem, project_id: Option<&str>) -> bool {
    match (project_id, msg.project_id.as_deref()) {
        (Some(a), Some(b)) => a == b,
        (None, None) => true,
        _ => false,
    }
}

/// 把内部对话上下文（`None` = 默认对话）翻译成**请求侧**的 `project_id` 取值
///
/// 后端 `project_id = None` 的语义是「不限制该条件 / 返回全部消息」，只有传
/// [`DEFAULT_CONVERSATION_PROJECT_ID`](common::constants::message::DEFAULT_CONVERSATION_PROJECT_ID)
/// 哨兵值才会被精确翻译成 `project_id IS NULL`。所以默认对话在发请求前必须翻一刀，
/// 否则拉回来的是全组织消息、只能靠前端过滤，`total` / `has_more` 会失真，
/// 一页被滤空还会让上拉卡在原地翻不到更早的历史。
///
/// ⚠️ 只对**默认对话**用。Agent 私聊的 `None` 含义是「不过滤 / 全部消息」——
/// 它没有项目域、另按 to/from 收窄，套用本函数会把带 `project_id` 的私聊消息丢掉。
///
/// ⚠️ 前端内部状态（`selected_project` / `chat_project_id`）的 `None` 语义**不变**：
/// `mention_kinds_for`、`is_project_mode` 等都靠 `is_some()` 判定「是否项目会话」，
/// 翻译只发生在 API 调用边界，且只在请求上用翻译后的值，过滤仍用内部值。
pub fn request_scope(project_id: Option<&str>) -> Option<String> {
    match project_id {
        Some(id) => Some(id.to_string()),
        None => Some(common::constants::message::DEFAULT_CONVERSATION_PROJECT_ID.to_string()),
    }
}

/// 消息是否「与当前用户有关」（用户自己发的，或发给该用户的）
///
/// 群聊（项目会话）里会混进 Agent 之间的横向交流，用户只是旁听者 ——
/// 这类消息用 `.message-bystander` 降透明度，免得刷屏时误以为都在找自己。
///
/// ⚠️ 乐观消息的 `from_id` 是占位符 [`OPTIMISTIC_USER_ID`]，不能直接与真实
/// `user_id` 比对，否则自己刚发出的消息一上来就被判成「旁观」而变灰。
pub fn involves_user(msg: &MessageListItem, user_id: &str) -> bool {
    fn user_side(role: i32, id: &str, user_id: &str) -> bool {
        if role != 0 {
            return false;
        }
        // user_id 未就绪（未登录 / 尚未回填）或占位 ID 时一律视为本人
        user_id.is_empty() || id == user_id || id == OPTIMISTIC_USER_ID
    }
    user_side(msg.from_role, &msg.from_id, user_id) || user_side(msg.to_role, &msg.to_id, user_id)
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

/// 用真实消息替换本地乐观气泡。
///
/// 匹配顺序：
/// 1. **按 message_id 精确对齐** —— 气泡在发送响应回来后已被重键为真实 ID
///    （见 `build_optimistic_user_msg` 各调用方），此时 ID 就是最稳的对应关系。
/// 2. 按 content 兜底 —— 覆盖尚未重键（仍是 `tmp_` 前缀）的气泡。
///
/// ⚠️ 只移除**本地气泡**（`tmp_` 前缀，或占位发送者 `OPTIMISTIC_USER_ID`）：
/// 真实消息的重复推送不得命中，否则会把列表里已有的同一条删掉再追加到末尾，
/// 表现为「消息跳到最后」。重复推送由调用方的 ID 去重拦掉。
/// 只移除第一条匹配，避免连发同内容消息时误删。
pub fn replace_tmp_with_real(msgs: &mut Vec<MessageListItem>, real_msg: &MessageListItem) {
    let is_provisional =
        |m: &MessageListItem| m.message_id.starts_with("tmp_") || m.from_id == OPTIMISTIC_USER_ID;

    if let Some(pos) = msgs
        .iter()
        .position(|m| is_provisional(m) && m.message_id == real_msg.message_id)
    {
        msgs.remove(pos);
        return;
    }

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
        from_id: OPTIMISTIC_USER_ID.to_string(),
        from_role: 0,
        to_id: to_agent_id.unwrap_or_default(),
        to_role: 1,
        message_type: MSG_TEXT,
        status: 3,
        content,
        reply_to_id: None,
        root_id: None,
        created_at: now_ms(),
        file_type: None,
        file_meta: None,
    }
}
