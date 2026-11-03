//! 思考循环中的新输入收集器
//!
//! 三处挂点（`poll_front` / `poll_round` / `poll_exit`）共用本模块的收集与
//! 游标推进逻辑。设计依据见
//! `docs/modules/02-Agent运行时/A-设计/inbox_poll_and_intent_revision_design.md`。
//!
//! 关键约束：
//! - 收集只读，不改消息状态；`Processed` 由唤醒成功后统一提交；
//! - `project_id` / `task_id` 是 `Option` **相等**比较（`None` 也是取值），
//!   不能用「空值跳过过滤」；
//! - `thread_id` 只作否决项：两端都有且不相等才拒绝；
//! - 合并前提是同一发送者 + 同一 `message_type`（v1 白名单仅 `Text`）。

use crate::models::message::Message;
use crate::pkg::RequestContext;
use crate::service::dao::message::MessageQuery;
use common::constants::message::DEFAULT_CONVERSATION_PROJECT_ID;
use common::enums::{MessageRole, MessageStatus, MessageType};
use common::error::Result;

/// 单次唤醒允许的最大注入批次
pub const MAX_INJECTIONS_PER_WAKE: usize = 3;
/// 单次唤醒允许的最大意图回退次数
pub const MAX_INTENT_REWINDS: usize = 2;
/// 单批注入允许吸收的最大字符数（每次 `take_batch` 独立计算）
pub const MAX_INJECTED_CHARS: usize = 2000;
/// 单批吸收的最大消息条数
pub const MAX_INJECTED_MESSAGES: usize = 5;
/// 吸收窗口：只吸收锚点后该时间窗内到达的消息（毫秒）
pub const INBOX_WINDOW_MS: i64 = 5 * 60 * 1000;

/// 唤醒锚点消息的结构快照
///
/// 收集器只依赖这些字段，避免在轮级图里持有整条消息。
#[derive(Debug, Clone)]
pub struct InboxAnchor {
    /// 接收方 Agent ID（`to_id`）
    pub agent_id: String,
    /// 发送方 ID
    pub from_id: String,
    /// 发送方角色
    pub from_role: MessageRole,
    /// 项目上下文（`None` 表示默认会话，必须同为 `None`）
    pub project_id: Option<String>,
    /// 任务上下文（`None` 表示未挂任务，必须同为 `None`）
    pub task_id: Option<String>,
    /// 话题键（仅作否决项）
    pub thread_id: Option<String>,
    /// 消息类型（v1 仅 `Text` 允许合并）
    pub message_type: MessageType,
    /// 锚点创建时间（毫秒）
    pub created_at: i64,
    /// 锚点消息 ID（同毫秒并列时的游标 tie-break）
    pub id: String,
    /// 组织 ID（租户隔离）
    pub organization_id: Option<String>,
}

impl InboxAnchor {
    /// 从锚点消息构造
    pub fn from_message(agent_id: &str, message: &Message) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            from_id: message.po.from_id.clone(),
            from_role: message.po.from_role,
            project_id: message.po.project_id.clone(),
            task_id: message.po.task_id.clone(),
            thread_id: message.po.thread_id.clone(),
            message_type: message.po.message_type,
            created_at: message.po.created_at,
            id: message.po.id.clone(),
            organization_id: message.po.organization_id.clone(),
        }
    }
}

/// 吸收游标：保证同一条消息在一轮唤醒内只被吸收一次
#[derive(Debug, Clone, Default)]
pub struct InboxCursor {
    /// 已吸收的最后一条消息创建时间
    pub created_at: i64,
    /// 已吸收的最后一条消息 ID
    pub id: String,
}

/// 被吸收的新输入
#[derive(Debug, Clone)]
pub struct InjectedMessage {
    /// 消息 ID（成功唤醒后置 `Processed`）
    pub id: String,
    /// 创建时间
    pub created_at: i64,
    /// 消息内容
    pub content: String,
}

impl InjectedMessage {
    /// 渲染为带序号的补充输入
    pub fn numbered_text(&self, index: usize) -> String {
        format!("（补充输入 #{}）\n{}", index + 1, self.content)
    }
}

/// 意图增量修订入参
#[derive(Debug, Clone)]
pub struct IntentRevision {
    /// 上一次完整意图结论
    pub previous: super::types::IntentAnalysis,
    /// 新增输入（按时间正序）
    pub inputs: Vec<String>,
}

/// 唤醒级收件箱状态
///
/// 跨 `poll_front` / 轮级 `poll_round` / `poll_exit` 共享，由 `AwakenDeps`
/// 持有并在 `DeliberateNode` 里克隆进轮级图。
#[derive(Debug)]
pub struct InboxState {
    anchor: InboxAnchor,
    cursor: InboxCursor,
    injections: usize,
    rewinds: usize,
    injected_ids: Vec<String>,
    pending_turn: Vec<InjectedMessage>,
    revision: Option<Vec<InjectedMessage>>,
}

impl InboxState {
    /// 以锚点消息初始化游标
    pub fn new(anchor: InboxAnchor) -> Self {
        let cursor = InboxCursor {
            created_at: anchor.created_at,
            id: anchor.id.clone(),
        };
        Self {
            anchor,
            cursor,
            injections: 0,
            rewinds: 0,
            injected_ids: Vec::new(),
            pending_turn: Vec::new(),
            revision: None,
        }
    }

    /// 锚点快照
    pub fn anchor(&self) -> &InboxAnchor {
        &self.anchor
    }

    /// 当前游标
    pub fn cursor(&self) -> &InboxCursor {
        &self.cursor
    }

    /// 已注入批次
    pub fn injections(&self) -> usize {
        self.injections
    }

    /// 已回退次数
    pub fn rewinds(&self) -> usize {
        self.rewinds
    }

    /// 已吸收消息 ID（唤醒成功后批量置 `Processed`）
    pub fn injected_ids(&self) -> &[String] {
        &self.injected_ids
    }

    /// 推进游标并记录吸收清单
    pub fn advance(&mut self, batch: &[InjectedMessage]) {
        let Some(last) = batch.last() else {
            return;
        };
        self.cursor = InboxCursor {
            created_at: last.created_at,
            id: last.id.clone(),
        };
        self.injected_ids
            .extend(batch.iter().map(|item| item.id.clone()));
    }

    /// 记录一次回退
    pub fn note_rewind(&mut self) {
        self.rewinds += 1;
    }

    /// 记录一次转向注入
    pub fn note_injection(&mut self) {
        self.injections += 1;
    }

    /// 暂存「回退失败 / 预算耗尽」时尾插进 Prompt 的新输入
    pub fn set_pending_turn(&mut self, batch: Vec<InjectedMessage>) {
        self.pending_turn = batch;
    }

    /// 清空待尾插输入
    pub fn clear_pending_turn(&mut self) {
        self.pending_turn.clear();
    }

    /// 待尾插输入的文本列表
    pub fn pending_turn_texts(&self) -> Vec<String> {
        self.pending_turn
            .iter()
            .enumerate()
            .map(|(index, item)| item.numbered_text(index))
            .collect()
    }

    /// 暂存意图回退输入
    pub fn set_revision(&mut self, batch: Vec<InjectedMessage>) {
        self.revision = Some(batch);
    }

    /// 取走意图回退输入（意图节点调用）
    pub fn take_revision(&mut self) -> Option<Vec<InjectedMessage>> {
        self.revision.take()
    }

    /// 意图修订失败时，把新输入降级为尾插转向消息
    pub fn fallback_revision(&mut self, batch: Vec<InjectedMessage>) {
        self.pending_turn = batch;
    }
}

/// 判断候选消息是否允许与锚点合并
///
/// SQL 已过滤大部分条件，这里保留完整判据用于防御与单测覆盖。
pub fn accepts(anchor: &InboxAnchor, message: &Message) -> bool {
    let po = &message.po;
    po.to_id == anchor.agent_id
        && po.from_id == anchor.from_id
        && po.from_role == anchor.from_role
        && po.project_id == anchor.project_id
        && po.task_id == anchor.task_id
        && po.message_type == anchor.message_type
        && po.file_type.is_none()
        && thread_allows(anchor.thread_id.as_deref(), po.thread_id.as_deref())
}

/// `thread_id` 只作否决项：两端都有且不相等才拒绝
pub fn thread_allows(anchor: Option<&str>, candidate: Option<&str>) -> bool {
    match (anchor, candidate) {
        (Some(left), Some(right)) => left == right,
        _ => true,
    }
}

/// 候选消息是否严格晚于游标（同毫秒用 ID 做稳定 tie-break）
pub fn is_after_cursor(cursor: &InboxCursor, message: &Message) -> bool {
    message.po.created_at > cursor.created_at
        || (message.po.created_at == cursor.created_at && message.po.id > cursor.id)
}

/// 在条数与字符上限内截取一批消息
pub fn take_batch(
    anchor: &InboxAnchor,
    cursor: &InboxCursor,
    rows: Vec<Message>,
) -> Vec<InjectedMessage> {
    let mut batch = Vec::new();
    let mut chars = 0usize;
    for message in rows {
        if batch.len() >= MAX_INJECTED_MESSAGES {
            break;
        }
        if !is_after_cursor(cursor, &message) || !accepts(anchor, &message) {
            continue;
        }
        let content_chars = message.po.content.chars().count();
        if chars + content_chars > MAX_INJECTED_CHARS {
            // 跳过超预算的单条，不能 break：否则一条超长消息会卡住整批，
            // 后面的短消息也只能等下一次唤醒。被跳过的消息保持 Pending，
            // 它自己的事件会触发独立唤醒，不会丢。
            continue;
        }
        chars += content_chars;
        batch.push(InjectedMessage {
            id: message.po.id,
            created_at: message.po.created_at,
            content: message.po.content,
        });
    }
    batch
}

/// 从 DB 拉取锚点之后、仍为 `Pending` 的同源输入
///
/// 只读：不推进游标、不改状态。
pub async fn collect(
    ctx: &RequestContext,
    anchor: &InboxAnchor,
    cursor: &InboxCursor,
) -> Result<Vec<InjectedMessage>> {
    // v1 白名单仅 Text：其他协议类型各自配对，不能并进同一轮
    if anchor.message_type != MessageType::Text {
        return Ok(Vec::new());
    }

    let project_id = anchor
        .project_id
        .clone()
        .unwrap_or_else(|| DEFAULT_CONVERSATION_PROJECT_ID.to_string());
    let query = MessageQuery {
        to_id: Some(anchor.agent_id.clone()),
        from_id: Some(anchor.from_id.clone()),
        from_role: Some(anchor.from_role),
        // None → 哨兵值，SQL 侧翻译为 `IS NULL`
        project_id: Some(project_id),
        task_id: anchor.task_id.clone(),
        task_id_is_null: anchor.task_id.is_none(),
        file_type_is_null: true,
        // thread_id 只作否决项：SQL 侧放行 NULL 或同线程，
        // 避免其他线程的消息先占满 limit、把合法消息挤出本批
        thread_id_compatible: anchor.thread_id.clone(),
        message_type: Some(anchor.message_type),
        status_in: Some(vec![MessageStatus::Pending]),
        organization_id: anchor.organization_id.clone(),
        // 游标同毫秒并列用 ID 兜底，因此下界回退 1ms 后内存再精筛
        created_after: Some(cursor.created_at.saturating_sub(1)),
        created_before: Some(
            anchor
                .created_at
                .saturating_add(INBOX_WINDOW_MS)
                .saturating_add(1),
        ),
        limit: Some(MAX_INJECTED_MESSAGES * 2),
        order_by: Some("created_at ASC, id ASC".to_string()),
        ..Default::default()
    };

    let rows = crate::service::dal::message::dal()
        .query(ctx.clone(), query)
        .await?;
    Ok(take_batch(anchor, cursor, rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::file::FileMeta;
    use common::enums::{FileType, MessageRole, MessageStatus, MessageType};
    use sqlx::types::Json;

    fn anchor(
        from_role: MessageRole,
        project_id: Option<&str>,
        task_id: Option<&str>,
        thread_id: Option<&str>,
        message_type: MessageType,
    ) -> InboxAnchor {
        InboxAnchor {
            agent_id: "agent-1".to_string(),
            from_id: "sender-1".to_string(),
            from_role,
            project_id: project_id.map(str::to_string),
            task_id: task_id.map(str::to_string),
            thread_id: thread_id.map(str::to_string),
            message_type,
            created_at: 1000,
            id: "msg-anchor".to_string(),
            organization_id: Some("org-1".to_string()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn message(
        id: &str,
        created_at: i64,
        from_id: &str,
        from_role: MessageRole,
        project_id: Option<&str>,
        task_id: Option<&str>,
        thread_id: Option<&str>,
        message_type: MessageType,
        file_type: Option<FileType>,
        content: &str,
    ) -> Message {
        Message::from_po(crate::models::message::MessagePo {
            id: id.to_string(),
            project_id: project_id.map(str::to_string),
            task_id: task_id.map(str::to_string),
            from_id: from_id.to_string(),
            to_id: "agent-1".to_string(),
            from_role,
            to_role: MessageRole::Agent,
            message_type,
            file_type,
            status: MessageStatus::Pending,
            content: content.to_string(),
            file_meta: Json(FileMeta::default()),
            reply_to_id: None,
            root_id: None,
            external_key: None,
            thread_id: thread_id.map(str::to_string),
            organization_id: Some("org-1".to_string()),
            created_by: "sender-1".to_string(),
            modified_by: "sender-1".to_string(),
            created_at,
            updated_at: created_at,
        })
    }

    fn default_cursor() -> InboxCursor {
        InboxCursor {
            created_at: 1000,
            id: "msg-anchor".to_string(),
        }
    }

    #[test]
    fn option_equality_covers_all_contexts() {
        // 默认对话：project/task 皆 None，只吸收同样为 None 的消息
        let a = anchor(MessageRole::User, None, None, None, MessageType::Text);
        assert!(accepts(
            &a,
            &message(
                "m1",
                1001,
                "sender-1",
                MessageRole::User,
                None,
                None,
                None,
                MessageType::Text,
                None,
                "继续"
            )
        ));
        assert!(!accepts(
            &a,
            &message(
                "m2",
                1001,
                "sender-1",
                MessageRole::User,
                Some("p1"),
                None,
                None,
                MessageType::Text,
                None,
                "串味"
            )
        ));

        // 有项目、未挂任务：任务必须仍为 None
        let b = anchor(MessageRole::User, Some("p1"), None, None, MessageType::Text);
        assert!(accepts(
            &b,
            &message(
                "m3",
                1001,
                "sender-1",
                MessageRole::User,
                Some("p1"),
                None,
                None,
                MessageType::Text,
                None,
                "ok"
            )
        ));
        assert!(!accepts(
            &b,
            &message(
                "m4",
                1001,
                "sender-1",
                MessageRole::User,
                Some("p1"),
                Some("t1"),
                None,
                MessageType::Text,
                None,
                "串任务"
            )
        ));
    }

    #[test]
    fn same_sender_any_role_can_merge() {
        for role in [MessageRole::User, MessageRole::Agent, MessageRole::System] {
            let a = anchor(role, None, None, None, MessageType::Text);
            let m = message(
                "m1",
                1001,
                "sender-1",
                role,
                None,
                None,
                None,
                MessageType::Text,
                None,
                "补充",
            );
            assert!(accepts(&a, &m), "同发送者同角色应允许合并: {role:?}");
        }
        // 发送角色不同即拒绝（即使 from_id 相同）
        let a = anchor(MessageRole::User, None, None, None, MessageType::Text);
        let m = message(
            "m1",
            1001,
            "sender-1",
            MessageRole::Agent,
            None,
            None,
            None,
            MessageType::Text,
            None,
            "不同角色",
        );
        assert!(!accepts(&a, &m));
    }

    #[test]
    fn thread_id_is_veto_only() {
        let a = anchor(
            MessageRole::User,
            None,
            None,
            Some("thread-a"),
            MessageType::Text,
        );
        let same = message(
            "m1",
            1001,
            "sender-1",
            MessageRole::User,
            None,
            None,
            Some("thread-a"),
            MessageType::Text,
            None,
            "同话题",
        );
        let other = message(
            "m2",
            1001,
            "sender-1",
            MessageRole::User,
            None,
            None,
            Some("thread-b"),
            MessageType::Text,
            None,
            "异话题",
        );
        let missing = message(
            "m3",
            1001,
            "sender-1",
            MessageRole::User,
            None,
            None,
            None,
            MessageType::Text,
            None,
            "无话题",
        );
        assert!(accepts(&a, &same));
        assert!(!accepts(&a, &other));
        assert!(accepts(&a, &missing), "任一端缺失不否决");
    }

    #[test]
    fn whitelist_rejects_non_text_and_attachments() {
        let a = anchor(MessageRole::User, None, None, None, MessageType::Text);
        let tool_call = message(
            "m1",
            1001,
            "sender-1",
            MessageRole::User,
            None,
            None,
            None,
            MessageType::ToolCallRequest,
            None,
            "tool",
        );
        let attachment = message(
            "m2",
            1001,
            "sender-1",
            MessageRole::User,
            None,
            None,
            None,
            MessageType::Text,
            Some(FileType::Image),
            "pic",
        );
        assert!(!accepts(&a, &tool_call));
        assert!(!accepts(&a, &attachment));
    }

    #[test]
    fn cursor_uses_id_tie_break_and_limits() {
        let a = anchor(MessageRole::User, None, None, None, MessageType::Text);
        let cursor = default_cursor();
        // 与游标同毫秒但 ID 更小 → 不算新消息
        let stale = message(
            "msg-0",
            1000,
            "sender-1",
            MessageRole::User,
            None,
            None,
            None,
            MessageType::Text,
            None,
            "旧",
        );
        assert!(!is_after_cursor(&cursor, &stale));
        // ID 更大 → 算新消息
        let fresh = message(
            "msg-z",
            1000,
            "sender-1",
            MessageRole::User,
            None,
            None,
            None,
            MessageType::Text,
            None,
            "新",
        );
        assert!(is_after_cursor(&cursor, &fresh));

        let rows: Vec<Message> = (0..7)
            .map(|i| {
                message(
                    &format!("m{i}"),
                    1001 + i,
                    "sender-1",
                    MessageRole::User,
                    None,
                    None,
                    None,
                    MessageType::Text,
                    None,
                    &"x".repeat(100),
                )
            })
            .collect();
        let batch = take_batch(&a, &cursor, rows);
        assert_eq!(batch.len(), MAX_INJECTED_MESSAGES);
        assert_eq!(batch.last().unwrap().id, "m4");
    }

    #[test]
    fn char_budget_skips_oversized_without_blocking_later() {
        let a = anchor(MessageRole::User, None, None, None, MessageType::Text);
        let cursor = default_cursor();
        let rows = vec![
            message(
                "m1",
                1001,
                "sender-1",
                MessageRole::User,
                None,
                None,
                None,
                MessageType::Text,
                None,
                &"x".repeat(1500),
            ),
            // 单条已超总预算：应跳过，而不是让整批停在它这里
            message(
                "m2",
                1002,
                "sender-1",
                MessageRole::User,
                None,
                None,
                None,
                MessageType::Text,
                None,
                &"y".repeat(2500),
            ),
            message(
                "m3",
                1003,
                "sender-1",
                MessageRole::User,
                None,
                None,
                None,
                MessageType::Text,
                None,
                &"z".repeat(400),
            ),
        ];
        let batch = take_batch(&a, &cursor, rows);
        let ids: Vec<&str> = batch.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, vec!["m1", "m3"], "超长单条被跳过，后续短消息仍应吸收");
        assert_eq!(
            batch
                .iter()
                .map(|i| i.content.chars().count())
                .sum::<usize>(),
            1900
        );
    }
}
