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
//! - 合并前提是同一发送者 + 兼容的 `message_type`（锚点仅 `Text`；候选允许
//!   `Mixed`，它的正文附件引用会被合并进锚点的视觉输入，总张数仍受上限约束）。

use crate::models::message::Message;
use crate::pkg::RequestContext;
use crate::pkg::tokens::estimate_tokens;
use crate::service::dao::message::MessageQuery;
use common::constants::message::DEFAULT_CONVERSATION_PROJECT_ID;
use common::enums::{MessageRole, MessageStatus, MessageType};
use common::error::Result;
use std::collections::BTreeSet;

/// 单次唤醒允许的最大新输入吸收次数（回 intent 与尾插追加共用同一预算）
pub const MAX_ABSORPTIONS_PER_WAKE: usize = 3;

/// 单次 `collect` 最多拉取的批次数（硬护栏）
///
/// 预算与「被挡下」只能兜住「有相关消息」的情形：若库中积压了大量**不相关**
/// 的 `Pending` 候选，`blocked` 永远不置位、`batch_len == INBOX_SCAN_LIMIT`
/// 也永远成立，循环就会一直刷库、每批一次小脑调用。用批次数封顶，把单次
/// 挂点的扫描量收敛到可预期范围，剩余候选留给后续挂点或下一次唤醒。
pub const MAX_COLLECT_BATCHES: usize = 8;
// 语义判据（同上下文候选的相关性判断）与扫描上限见 `super::inbox_relevance`：
// 不设 SQL 时间上界，判定结果双向缓存；
// 单批吸收条数不另设硬上限，容量只由 `take_batch` 的 token 预算兜住。

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
    /// 锚点正文（超窗消息的相关性判断依据）
    pub content: String,
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
            content: message.po.content.clone(),
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
    /// 正文里的附件引用 ID（`attachment:` 协议）
    ///
    /// 只携带引用，不携带 base64：视觉 part 由唤醒装配时统一解析并做总量上限。
    pub attachment_ids: Vec<String>,
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
    /// 本唤醒已吸收的新输入批次（回 intent / 尾插共用）
    absorptions: usize,
    /// 本唤醒剩余的注入 token 预算（`None` = 无上限）。
    /// ⚠️ 三个挂点共享这一份总账：每次 collect 传入当前剩余值，collect 返回
    /// 本次**实际用量**，这里用 `剩余 -= 用量` 扣减。避免「每个挂点各领一份 5%」
    /// 造成 3×5% 超额；同时 `None` / 预算为零都能被 collect 按自身语义处理。
    remaining_inbox_tokens: Option<usize>,
    injected_ids: Vec<String>,
    /// 本唤醒内已参与语义判定的消息 ID（相关与不相关都记）。
    /// 下一批 SQL 拉取时排除它们，否则未吸收的候选会一直占满扫描配额、
    /// 新消息永远进不来。缓存只挡重复小脑调用，挡不住重复占位。
    judged_ids: BTreeSet<String>,
    /// 本唤醒已吸收消息携带的附件引用（跨 pending_turn / revision 持久保留，
    /// 因为两处批次的文本会被消费清空，但图片仍须随最终 Prompt 一起送达）
    absorbed_attachment_ids: Vec<String>,
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
            absorptions: 0,
            remaining_inbox_tokens: None,
            injected_ids: Vec::new(),
            judged_ids: BTreeSet::new(),
            absorbed_attachment_ids: Vec::new(),
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

    /// 本唤醒已吸收批次（供各挂点做统一预算封顶）
    pub fn absorptions(&self) -> usize {
        self.absorptions
    }

    /// 本唤醒剩余的注入 token 预算（`None` = 无上限）
    pub fn remaining_inbox_tokens(&self) -> Option<usize> {
        self.remaining_inbox_tokens
    }

    /// 初始化剩余预算（构造后由唤醒装配调用一次；`None` = 无上限）
    pub fn set_remaining_inbox_tokens(&mut self, budget: Option<usize>) {
        self.remaining_inbox_tokens = budget;
    }

    /// 按本次实际用量扣减剩余预算：`剩余 -= 用量`，下限 0。
    /// `None`（无上限）扣减后仍是 `None`，不产生「无上限突然变成有上限」的突变。
    pub fn spend_inbox_tokens(&mut self, used: usize) {
        if let Some(remaining) = self.remaining_inbox_tokens.as_mut() {
            *remaining = remaining.saturating_sub(used);
        }
    }

    /// 已吸收消息 ID（唤醒成功后批量置 `Processed`）
    pub fn injected_ids(&self) -> &[String] {
        &self.injected_ids
    }

    /// 本唤醒内已判定的消息 ID（下一批拉取时排除）
    pub fn judged_ids(&self) -> &BTreeSet<String> {
        &self.judged_ids
    }

    /// 本唤醒已吸收消息携带的附件引用 ID（供视觉装配合并）
    pub fn absorbed_attachment_ids(&self) -> &[String] {
        &self.absorbed_attachment_ids
    }

    /// 记录一批已判定的候选 ID
    pub fn note_judged(&mut self, ids: &[String]) {
        self.judged_ids.extend(ids.iter().cloned());
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
        for item in batch {
            for att_id in &item.attachment_ids {
                if !self.absorbed_attachment_ids.contains(att_id) {
                    self.absorbed_attachment_ids.push(att_id.clone());
                }
            }
        }
    }

    /// 记录一次吸收（不区分回 intent 还是尾插）
    pub fn note_absorption(&mut self) {
        self.absorptions += 1;
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
        && message_type_allows(
            anchor.message_type,
            po.message_type,
            &message.attachment_ref_ids(),
        )
        && po.file_type.is_none()
        && thread_allows(anchor.thread_id.as_deref(), po.thread_id.as_deref())
}

/// 锚点只收 `Text`；候选的 `Mixed` 只在其正文确实带附件引用时兼容 ——
/// 否则它是另一种协议类型，不能并进同一轮。
fn message_type_allows(
    anchor: MessageType,
    candidate: MessageType,
    attachment_ids: &[String],
) -> bool {
    candidate == anchor
        || (anchor == MessageType::Text
            && candidate == MessageType::Mixed
            && !attachment_ids.is_empty())
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

/// 在 token 预算内截取一批消息
///
/// `budget_tokens` 由调用方按模型上下文容量的占比算好传入（`None` ⇒ 不设预算）：
/// 注入内容最终进的是模型输入，预算单位必须是 token，字符口径会在中英混排下
/// 差出 4 倍，且无法与容量直接比较。不再另设条数硬上限 —— 小脑判定相关的消息
/// 应全部吸收，容量只由 token 预算兜住。
#[cfg(test)]
pub fn take_batch(
    anchor: &InboxAnchor,
    cursor: &InboxCursor,
    rows: Vec<Message>,
    budget_tokens: Option<usize>,
) -> Vec<InjectedMessage> {
    take_batch_with_usage(anchor, cursor, rows, budget_tokens, 0).0
}

/// 带预算用量的批处理：`already_used` 是本唤醒前几批/前几个挂点已消耗的 token。
///
/// 返回 `(本批可注入消息, 累计已用 token, 是否被预算挡下)`：第三项让调用方
/// 在「剩余预算连最短一条都放不下」时立即结束分批，避免 `used_tokens` 停在
/// 非整除的余量上却继续拉库、空耗小脑判定。
fn take_batch_with_usage(
    anchor: &InboxAnchor,
    cursor: &InboxCursor,
    rows: Vec<Message>,
    budget_tokens: Option<usize>,
    already_used: usize,
) -> (Vec<InjectedMessage>, usize, bool) {
    let mut batch = Vec::new();
    let mut used_tokens = already_used;
    let mut blocked_by_budget = false;
    for message in rows {
        if !is_after_cursor(cursor, &message) || !accepts(anchor, &message) {
            continue;
        }
        let content_tokens = estimate_tokens(&message.po.content);
        let over_budget = match budget_tokens {
            Some(budget) => used_tokens + content_tokens > budget,
            None => false,
        };
        if over_budget {
            // 跳过超预算的单条，不能 break：否则一条超长消息会卡住整批，
            // 后面的短消息也能在同一批里被吸收。是否结束分批由第三项回报给
            // collect 决定（超长条本身保持 Pending，留给下一轮独立唤醒）。
            blocked_by_budget = true;
            continue;
        }
        used_tokens += content_tokens;
        let attachment_ids = message.attachment_ref_ids();
        batch.push(InjectedMessage {
            id: message.po.id,
            created_at: message.po.created_at,
            attachment_ids,
            content: message.po.content,
        });
    }
    (batch, used_tokens, blocked_by_budget)
}

/// 一次拉取的结果：可注入的消息 + 本批已判定的候选 ID + 本次实际 token 用量
#[derive(Debug)]
pub struct CollectOutcome {
    /// 通过语义判据、可注入本轮的消息
    pub injected: Vec<InjectedMessage>,
    /// 本批参与判定的全部候选 ID（含被拒的），调用方据此排除下一批
    pub judged: Vec<String>,
    /// 本次 `collect` **实际消耗**的 token（不含之前挂点已用的部分）。
    /// 调用方据此在外部做 `剩余 -= 实际用量` 更新总账。返回「实际用量」而非
    /// 「剩余量」，让无预算 / 预算为零等情形仍有明确语义可表达。
    pub used_tokens: usize,
}

/// 从 DB 循环分批拉取锚点之后、仍为 `Pending` 的同源输入
///
/// 只读：不推进游标、不改状态。`judged_ids` 由调用方从 `InboxState` 传入，
/// 排除本唤醒内已判定的候选，保证扫描窗口向后推进而不是每批重扫头部。
///
/// **分批机制**：每批取 [`INBOX_SCAN_LIMIT`](super::inbox_relevance::INBOX_SCAN_LIMIT)
/// 条 → 小脑多选判断（判定结果缓存复用）→ 相关的按 token 预算尾插。一批处理完
/// 继续取下一批，直到：① 取不满一批（DB 已无更多 Pending 候选）；② token 预算
/// 耗尽；③ 达到 [`MAX_COLLECT_BATCHES`] 硬上限。这样即使有 20+ 条待处理消息，
/// 也能在一次唤醒内全部判完，而不会只吃前 10 条、把后面的留到下一次唤醒；同时
/// 积压大量不相关候选时也只会扫描有限批次，不会无限刷库。预算跨批累加（同一份
/// 「思考空间占比」），不在每批重置。
// 8 个入参都是「本次判定现场」，无可打包的配置对象：共享参数（anchor/cursor/
// judged/budget）已由图总线收口为 `inbox_collect` 方法，节点与生产链路不直接调
// 本函数；余下 7 处是单测直调。结构体化只会给测试增加样板，故就地豁免。
#[allow(clippy::too_many_arguments)]
pub async fn collect(
    ctx: &RequestContext,
    anchor: &InboxAnchor,
    cursor: &InboxCursor,
    judged_ids: &BTreeSet<String>,
    judge_ctx: &super::inbox_relevance::InboxJudgeContext,
    // 思考现场携带的 Brain（优先 Agent.brain），小脑判定走统一入口
    brain: Option<&crate::models::brain::Brain>,
    // 本次可用的**剩余**预算（`None` = 无上限）。跨挂点的总账由调用方持有，
    // 每次把剩余值传进来，用返回的 `used_tokens` 扣减。
    budget_tokens: Option<usize>,
    // 小脑判定 DAL 的可替换入口：`None` = 生产走 `brain::dal()` 真实单例；
    // `Some` = 测试注入脚本替身，避免集成测试打到真实模型等待超时。
    judge_dal: Option<&dyn crate::service::dal::brain::BrainDal>,
) -> Result<CollectOutcome> {
    // v1 白名单仅 Text：其他协议类型各自配对，不能并进同一轮
    if anchor.message_type != MessageType::Text {
        return Ok(CollectOutcome {
            injected: Vec::new(),
            judged: Vec::new(),
            used_tokens: 0,
        });
    }

    let project_id = anchor
        .project_id
        .clone()
        .unwrap_or_else(|| DEFAULT_CONVERSATION_PROJECT_ID.to_string());

    let mut injected: Vec<InjectedMessage> = Vec::new();
    let mut judged: Vec<String> = Vec::new();
    // 本唤醒内已判定集合：初始为调用方传入的历史，循环内逐批累加，
    // 作为下一批的 `exclude_ids` 游标向后推进。
    let mut seen: BTreeSet<String> = judged_ids.iter().cloned().collect();
    // 本次 collect 的实际用量从 0 起算；之前挂点用掉的部分已在外部总账里扣过，
    // 这里的 `budget_tokens` 就是扣减后的剩余值。
    let mut used_tokens = 0usize;
    // 已拉取的批次数：防止「海量不相关 Pending」让循环永远刷不到边界。
    let mut batches = 0usize;

    loop {
        if batches >= MAX_COLLECT_BATCHES {
            break;
        }
        let query = MessageQuery {
            to_id: Some(anchor.agent_id.clone()),
            from_id: Some(anchor.from_id.clone()),
            from_role: Some(anchor.from_role),
            // None → 哨兵值，SQL 侧翻译为 `IS NULL`
            project_id: Some(project_id.clone()),
            task_id: anchor.task_id.clone(),
            task_id_is_null: anchor.task_id.is_none(),
            file_type_is_null: true,
            // thread_id 只作否决项：SQL 侧放行 NULL 或同线程，
            // 避免其他线程的消息先占满 limit、把合法消息挤出本批
            thread_id_compatible: anchor.thread_id.clone(),
            // Mixed + 附件引用与 Text 锚点兼容（图片会在视觉装配层合并）
            message_type_in: Some(vec![anchor.message_type, MessageType::Mixed]),
            // 硬性状态过滤：只查 Pending，已处理/失败/撤回的一律不进判据
            status_in: Some(vec![MessageStatus::Pending]),
            organization_id: anchor.organization_id.clone(),
            // 游标同毫秒并列用 ID 兜底，因此下界回退 1ms 后内存再精筛
            created_after: Some(cursor.created_at.saturating_sub(1)),
            // 不设 SQL 时间上界：同上下文的补充消息隔多久都该参与语义判断。
            // 用条数上限分批兜住扫描量，按时间正序保证先到的先判、游标稳步推进。
            created_before: None,
            limit: Some(super::inbox_relevance::INBOX_SCAN_LIMIT),
            order_by: Some("created_at ASC, id ASC".to_string()),
            // 排除本唤醒内已判定的候选：不吸收的消息不会被游标越过，
            // 不排除就会每批都从同一批头部重扫，后面的新消息永远进不来。
            exclude_ids: seen.iter().cloned().collect(),
            ..Default::default()
        };

        let rows = crate::service::dal::message::dal()
            .query(ctx.clone(), query)
            .await?;
        let batch_len = rows.len();
        batches += 1;
        if batch_len == 0 {
            break;
        }

        // 本批全部候选都记为「已判定」，无论最终放行还是拒绝：
        // 缓存只挡重复的小脑调用，SQL 排除才挡重复占位。
        for row in &rows {
            let id = row.po.id.clone();
            seen.insert(id.clone());
            judged.push(id);
        }

        // 同上下文候选一次性交给小脑多选判断，相关的放行。
        // 判定结果按 (anchor_id, message_id, 现场指纹) 双向缓存，多轮 poll 不重复调用。
        let related_ids = match judge_dal {
            Some(dal) => {
                super::inbox_relevance::rank_related(ctx, anchor, &rows, judge_ctx, brain, dal)
                    .await
            }
            None => {
                super::inbox_relevance::rank_related_with_default_dao(
                    ctx, anchor, &rows, judge_ctx, brain,
                )
                .await
            }
        };
        let related: Vec<Message> = rows
            .into_iter()
            .filter(|row| related_ids.contains(&row.po.id))
            .collect();

        let (batch, used, blocked) =
            take_batch_with_usage(anchor, cursor, related, budget_tokens, used_tokens);
        used_tokens = used;
        injected.extend(batch);

        // 收尾判据：① DB 已取不满一批 ⇒ 没有更多候选；② 本批有候选被预算挡下 ⇒
        // 继续拉也只会重复挡下、白耗小脑调用。用 `blocked` 而非 `used >= budget`：
        // 后者在 token 数不整除预算时永远不成立（被挡的条目不计入 used），
        // 导致循环刷完整库；前者只认「确有条目放不下」，整除/非整除都正确。
        if batch_len < super::inbox_relevance::INBOX_SCAN_LIMIT {
            break;
        }
        if blocked {
            break;
        }
    }

    Ok(CollectOutcome {
        injected,
        judged,
        used_tokens,
    })
}
#[cfg(test)]
#[path = "inbox_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "inbox_collect_tests.rs"]
mod collect_tests;
