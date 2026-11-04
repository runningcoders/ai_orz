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
    /// 续跑追加标记：`poll_exit` 的 rethink 必须基于既有对话尾部追加，
    /// 不能重新装配 base（否则 Phase 1 前缀与意图结论会被重排/丢失）。
    ///
    /// 只有 `poll_exit` 会置位；`poll_front` / `poll_round` 不置位。
    /// `PrepareNode` 读取后立即消费，避免后续压缩回环误走追加。
    resume_append: bool,
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
            resume_append: false,
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

    /// `poll_exit` 判定需要重新思考：下一次 `prepare` 走「既有对话尾部追加」
    pub fn mark_resume_append(&mut self) {
        self.resume_append = true;
    }

    /// 取走续跑标记（`PrepareNode` 消费；未置位返回 false）
    pub fn take_resume_append(&mut self) -> bool {
        std::mem::replace(&mut self.resume_append, false)
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
#[path = "inbox_tests.rs"]
mod tests;
