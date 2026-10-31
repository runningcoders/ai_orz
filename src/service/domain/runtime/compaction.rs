//! 上下文压缩流程
//!
//! `compact_context` 是 `RuntimeDomainImpl` 的私有辅助方法，
//! 不属于 `RuntimeAwakening` trait（只在 awaken 内部调用）。
//!
//! # 设计要点：复用主循环上下文
//!
//! 压缩**不重新拼装 Prompt**，而是直接复用主循环已有的完整 `messages` 数组，
//! 仅在尾部追加一条「压缩指令」伪 User 消息。三个好处：
//!
//! 1. **命中 prefix caching**：追加前的前缀与上一次模型调用完全一致，
//!    provider 侧（Anthropic / OpenAI / DeepSeek 等）可直接复用缓存，
//!    省去重新拼装 System（人设+技能）与通用上下文（画像/工作空间）的开销。
//! 2. **信息零损失**：模型看到的就是完整原始对话，不是被按条截断过的二手摘要。
//! 3. **【当前消息】天然在上下文里**：无需再单独传「本轮用户原始消息」区块。
//!
//! 触发时机：
//! - 主循环上下文溢出（`ContextOverflow`）→ 压缩后 `continue` 重建循环
//! - 思考轮次耗尽（`MaxRoundsExceeded`）→ 压缩产出作为给用户回复
//! - 正常完成（`Final`）→ 压缩落盘短期记忆
//!
//! # 与 sleep_and_settle 的边界
//!
//! 压缩**不是**睡觉：目标是「本次对话 → 一条短期记忆」，不查知识图谱、不建关系、
//! 不改记忆状态；也**完全不操作 Agent 运行时状态**（不像 sleep_and_settle 会
//! set_resting / set_idle），因为压缩发生在 awaken 主循环内部，Agent 必须保持 Busy。

use super::types::ThinkLoopParams;
use crate::models::agent::Agent;
use crate::models::cortex_types::ChatMessage;
use crate::models::events::AgentLoopEvent;
use crate::models::memory::{MemoryCreateParams, MemoryTrace, ShortTermMemoryIndexPo};
use crate::pkg::agent_runtime_state::AgentThinkRuntime;
use crate::pkg::request_context::RequestContext;
use crate::service::domain::runtime::{RuntimeDomain, RuntimeDomainImpl};
use common::enums::{MemoryRole, MemoryStatus, ThinkingScene};
use common::error::Result;
use std::sync::Arc;

use super::awakening::build_scene_tool_descriptors;
use super::think_loop::build_policy_for_scene;

/// 压缩场景的思考轮次上限（**溢出回环 / 正常收尾存档**用）
///
/// 压缩只需调用一次 `save_short_term_memory` 再输出 Final，给 3 轮足够容错
/// （工具参数写错时模型还有自纠机会）；同时它挡在用户等待的关键路径上，不宜给大预算。
///
/// ⚠️ 「轮次耗尽后的收尾回复」走的是另一份预算（Agent 的 `summary_max_rounds`），
/// 不消费本常量 —— 那条路径是唯一一次向用户交付结果的机会，允许配得更宽。
pub(crate) const COMPACT_MAX_ROUNDS: usize = 3;

/// 压缩场景允许递归调用的工具白名单守卫
///
/// 处于沉淀/压缩流程中时，模型再调 `settle_memory` 属于自我递归，
/// 直接跳过执行并回一条提示，避免「压缩里再触发一次完整沉淀」的雪崩。
pub(crate) const RECURSIVE_SETTLE_TOOL: &str = "settle_memory";

/// 短期记忆写入工具（沉淀场景禁止调用）
///
/// 待沉淀队列是本轮沉淀的**输入**：沉淀期间再写一条，框架刚把本批标记 Settled，
/// 回环的 `prepare` 又会立刻取到新写的这条 → 队列永远排不空，直到打满
/// `MAX_SETTLE_BATCHES` 并把整轮沉淀判失败重投（实测一次夜间沉淀因此跑了
/// 520 轮模型调用、耗时 2.5 小时）。压缩场景**不拦**：它的产出本来就是一条
/// 短期记忆。
pub(crate) const SHORT_TERM_WRITE_TOOL: &str = "save_short_term_memory";

/// 框架落库时给压缩记忆打的标签（便于后续识别/统计这类记忆）
const COMPACTION_TAG: &str = "compaction";

/// 框架轻量落档时打的标签（跳过 LLM 压缩的那部分轮次）
const QUICK_TURN_TAG: &str = "quick_turn";

/// 轻量落档写入时的硬上限（兜底，不是分流阈值）
///
/// 分流由策略引擎按估算 token 数决定（见 [`build_compaction_policy`]）；
/// 这里兜一个字符上限，避免极端长原文在阈值配置过大时长期占用后续 prompt。
const QUICK_TURN_HARD_CAP_CHARS: usize = 1200;

/// 按字符（非字节）截断，保留字符边界
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let end = text
        .char_indices()
        .nth(max)
        .map(|(i, _)| i)
        .unwrap_or(text.len());
    format!("{}…", &text[..end])
}

/// 是否 CJK 字符（中日韩表意文字 / 假名 / 谚文 / 全角标点）
///
/// 只用于 token 估算：CJK 一字约等于 1 token，拉丁文约 4 字符 1 token。
fn is_cjk(ch: char) -> bool {
    matches!(
        ch,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{3040}'..='\u{30FF}'
            | '\u{AC00}'..='\u{D7AF}'
            | '\u{FF00}'..='\u{FFEF}'
    )
}

/// 估算一段文本的 token 数（不引真实 tokenizer，够策略分级用）
///
/// 规则：CJK 按 1 token/字；其余（拉丁字母/数字/ASCII 标点）按 4 字符/token，
/// 向上取整。混合文本按字符逐类累加，避免同一段中英夹杂时估算失真。
pub(crate) fn estimate_tokens(text: &str) -> usize {
    let mut cjk = 0usize;
    let mut other = 0usize;
    for ch in text.chars() {
        if is_cjk(ch) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk + other.div_ceil(4)
}

/// 估算「用户消息 + 助手回复」的合计 token 数（记忆分流依据）
pub(crate) fn estimate_turn_tokens(user_message: &str, reply: &str) -> usize {
    estimate_tokens(user_message.trim()) + estimate_tokens(reply.trim())
}

/// 构造记忆分流策略组（命中 = 本轮值得走 LLM 压缩）
///
/// 策略组现在是单条阈值策略，但保留组形态：后续加「小脑」判断（如
/// 语义重要性、任务类型）时直接往组里加，调用方无需改动。
pub(crate) fn build_compaction_policy(agent: &Agent) -> Arc<dyn crate::pkg::policy::Policy> {
    use crate::pkg::policy::builtin::CompactionThresholdPolicy;
    use crate::pkg::policy::policy_set;

    // 不存在「不压缩」场景：即使配置被误填成 0（策略引擎里 0 = 未启用），
    // 也至少保证按 1 处理，绝不静默退化成「全部原文记账」。
    let threshold = super::types::config_resolve::compaction_threshold_tokens(agent).max(1);
    Arc::from(policy_set! {
        CompactionThresholdPolicy(threshold as u64)
    })
}

/// 轻量落档的摘要文本（纯函数，便于单测）
///
/// 保留「用户说了什么 + 助手回了什么」的原始对照即可：它是下一轮唤醒
/// 注入【历史对话】时的可见内容，丢掉任何一侧都会让上下文断片。
/// 两侧全空返回 `None`（没有可记的东西，不写空记忆）。
///
/// 阈值内的短对话**保留全文**（"直接记下来"要名副其实）；只有超过硬上限的
/// 极长内容（仅直答/澄清可能触发）才两侧均分截断兜底。
fn build_quick_turn_summary(user_message: &str, reply: &str) -> Option<String> {
    let user = user_message.trim();
    let reply = reply.trim();
    if user.is_empty() && reply.is_empty() {
        return None;
    }

    let total = user.chars().count() + reply.chars().count();
    let (user, reply) = if total <= QUICK_TURN_HARD_CAP_CHARS {
        (user.to_string(), reply.to_string())
    } else {
        let half = QUICK_TURN_HARD_CAP_CHARS / 2;
        (
            truncate_chars(user, half),
            truncate_chars(reply, QUICK_TURN_HARD_CAP_CHARS - half),
        )
    };

    Some(if reply.is_empty() {
        format!("用户：{user}")
    } else {
        format!("用户：{user}\n助手：{reply}")
    })
}

/// 压缩摘要的起止标记
///
/// 压缩产出由**框架**落库（不交给模型调工具），因此要求模型把摘要包在标记里，
/// 框架从 Final 文本中解析。选这种朴素标记而非 JSON：压缩轮可能已被截断，
/// 结构化输出一旦不完整就整体解析失败，标记法至少能保住开头到结尾之间的内容。
const SUMMARY_OPEN: &str = "<compacted_summary>";
const SUMMARY_CLOSE: &str = "</compacted_summary>";

/// 「给用户的一句话」的起止标记（仅轮次耗尽场景）
///
/// 与摘要区块**物理分离**：摘要永不发给用户，回复只认这个标记。
/// 这样即使模型漏打摘要标记，也不可能把内部摘要当回复泄漏出去。
const USER_REPLY_OPEN: &str = "<user_reply>";
const USER_REPLY_CLOSE: &str = "</user_reply>";

/// 构造压缩指令（作为伪 User 消息追加到现有对话尾部）
///
/// 只要求模型**输出**摘要，不要求它调用任何工具 —— 落库由框架统一完成，
/// 这样 trace_ids / tags 由框架填写（保证正确），也杜绝「模型没调工具导致工作丢失」。
///
/// `with_user_reply` 为 true 时（轮次耗尽场景），额外要求模型在摘要之后
/// 再用一句话向用户说明进展，该文本会作为 `raw_output` 发给用户。
fn compact_instruction(with_user_reply: bool) -> String {
    let mut s = String::new();
    s.push_str("【上下文压缩任务】\n\n");
    s.push_str(
        "你的上下文已接近上限，需要把**本次对话**压缩成一份摘要，以便你在后续轮次中接着工作。\n\n",
    );

    s.push_str("## 压缩范围（**严格遵守**）\n\n");
    s.push_str("**只压缩「真正干的活儿」**：从【当前消息】开始的用户诉求，以及此后产生的全部\n");
    s.push_str("对话与工具调用过程 —— 即 Assistant 消息与工具结果：做了什么、查到了什么、\n");
    s.push_str("结论是什么、还有什么没做完。\n\n");
    s.push_str("**绝对不要压缩以下内容**（它们每轮都会由框架重新拼接注入，写进记忆纯属冗余）：\n");
    s.push_str("- System 区块：你的**人设 / 灵魂 / 技能方法论 / 回复规则指引**\n");
    s.push_str("- User 区块里的**通用上下文**：【用户画像】、【项目上下文】、【任务上下文】、\n");
    s.push_str("  【工作空间与路径约定】、【思考 Trace ID】\n");
    s.push_str("- 【历史对话】里的既有记忆条目\n");
    s.push_str("- 任何与**当前这一轮工作**无关的固定说明文字\n\n");
    s.push_str("判断标准很简单：**如果这段内容下一轮还会原样出现，就不要压缩它。**\n\n");

    s.push_str("## 你要做的\n\n");
    s.push_str("**直接输出摘要即可，不要调用任何工具** —— 保存由框架在收到你的输出后统一完成，\n");
    s.push_str("你不需要也不应该自己去写记忆。\n\n");
    s.push_str(&format!(
        "请把摘要放在 `{}` 与 `{}` 之间：\n\n",
        SUMMARY_OPEN, SUMMARY_CLOSE
    ));
    s.push_str(SUMMARY_OPEN);
    s.push('\n');
    s.push_str("已完成的工作 / 查到的关键事实 / 得出的结论 / 还没做完的待办\n");
    s.push_str(SUMMARY_CLOSE);
    s.push_str("\n\n");
    s.push_str("摘要要求：\n");
    s.push_str("- 提炼，不要流水账；保留后续接着做所必需的具体信息（文件路径、ID、参数、报错）\n");
    s.push_str("- 只写**这一轮**的事，历史记忆不必复述\n");

    if with_user_reply {
        s.push_str(&format!(
            "\n摘要区块**之外**，再用 `{}` 与 `{}` 包住**一句话**，告诉用户：\
             你进行到了哪一步、为什么停在这里。\n\
             这份文本会**直接发送给用户**（摘要区块不会）；没有这个标记就等于没有回复。\n",
            USER_REPLY_OPEN, USER_REPLY_CLOSE
        ));
        s.push_str(USER_REPLY_OPEN);
        s.push('\n');
        s.push_str("进行到了哪一步、为什么停在这里（一句话）\n");
        s.push_str(USER_REPLY_CLOSE);
        s.push('\n');
    } else {
        s.push_str("\n除摘要外不要输出其它内容。\n");
    }
    s
}

/// 从压缩 Final 中提取「给用户的一句话」
///
/// **只认显式标记**：摘要区块与回复区块物理分离，杜绝「模型漏打摘要标记时，
/// 整段内部摘要被当成回复发给用户」——那是最坏情况（用户收到的是
/// 「已完成的工作 / 还没做完的待办」这种内部口吻）。
///
/// 没有 `<user_reply>` 标记返回 `None`，调用方用兜底文案：宁可少说一句，
/// 也不把内部产物泄漏到用户侧。
pub(crate) fn extract_user_reply(final_text: &str) -> Option<String> {
    let start = final_text.find(USER_REPLY_OPEN)? + USER_REPLY_OPEN.len();
    let rest = &final_text[start..];
    let end = rest.find(USER_REPLY_CLOSE).unwrap_or(rest.len());
    let reply = rest[..end].trim();
    if reply.is_empty() {
        return None;
    }
    Some(reply.to_string())
}

/// 从压缩循环的 Final 文本里解析出压缩摘要
///
/// 找不到标记时返回 None —— 说明模型没按格式输出，调用方应据此降级
/// （此时本轮工作会丢失，必须告警）。
fn parse_compacted_summary(final_text: &str) -> Option<String> {
    let start = final_text.find(SUMMARY_OPEN)? + SUMMARY_OPEN.len();
    let rest = &final_text[start..];
    let end = rest.find(SUMMARY_CLOSE).unwrap_or(rest.len());
    let summary = rest[..end].trim();
    if summary.is_empty() {
        return None;
    }
    Some(summary.to_string())
}

/// 压缩流程的产出
pub(crate) struct CompactOutcome {
    /// 压缩循环最后输出的 Final 文本（轮次耗尽场景作为给用户的回复）
    pub final_text: String,
    /// 压缩出的内容，同时已由框架写入短期记忆
    ///
    /// None 表示模型没按标记格式输出，本轮工作未能留存（已记 warn）。
    /// 调用方（awaken）在 None 时应按「没有压缩结果」处理。
    pub compacted_summary: Option<String>,
    /// 框架落库那条压缩记忆的 id
    ///
    /// 下一轮组装「更早的记忆」参考块时要**排除它**：它的内容已经在
    /// 【上一轮工作压缩结果】里了，再放进参考块就是重复占用预算。
    pub compacted_memory_id: Option<String>,
}

impl RuntimeDomainImpl {
    /// 由框架把压缩摘要写入短期记忆
    ///
    /// 与交给模型调 `save_short_term_memory` 的区别：
    /// - `trace_ids` / `tags` 由框架填写，**保证正确**（模型经常漏填）
    /// - 只要解析到摘要就一定会落库，不存在「模型没调工具导致工作丢失」
    ///
    /// 失败只记 warn：压缩流程本身不因落库失败而中断，
    /// 下一轮仍会拿到 `compacted_summary`（只是记忆库里没有这条）。
    async fn persist_compacted_memory(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        summary: &str,
        trace_ids: &[String],
        parent_trace_id: &str,
    ) -> Option<String> {
        self.insert_short_term(
            ctx,
            agent_id,
            summary,
            &[COMPACTION_TAG],
            trace_ids,
            parent_trace_id,
        )
        .await
    }

    /// 轻量落档：不调模型，把本轮问答机械写进短期记忆
    ///
    /// 直答 / 澄清 / 闲聊路径跳过了 LLM 压缩，但**不能跳过记忆写入**：
    /// 唤醒时注入的「近期对话」读的就是短期记忆（`get_recent_context`）——
    /// 这里不留一条，下一轮追问在历史里看不到刚才那轮，表现为「聊过就忘」。
    ///
    /// 质量刻意低于压缩摘要（原文截断、不做归纳）：它只负责「别丢上下文」，
    /// 真正的经验沉淀仍由休息沉淀与显式 `save_short_term_memory` 负责。
    ///
    /// 返回落库的 memory_id；内容全空或落库失败返回 `None`（不阻断收尾）。
    pub(crate) async fn persist_turn_memory(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        user_message: &str,
        reply: &str,
        trace_id: &str,
    ) -> Option<String> {
        let summary = build_quick_turn_summary(user_message, reply)?;
        self.insert_short_term(
            ctx,
            agent_id,
            &summary,
            &[QUICK_TURN_TAG],
            &[trace_id.to_string()],
            trace_id,
        )
        .await
    }

    /// 短期记忆索引的统一落库口径（压缩摘要与轻量落档共用）
    ///
    /// `trace_ids` / `tags` 由框架填写而非模型：模型经常漏填，交给它就等于
    /// 放弃可追溯性。落库失败只记 warn，不阻断调用方后续流程。
    async fn insert_short_term(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        summary: &str,
        tags: &[&str],
        trace_ids: &[String],
        parent_trace_id: &str,
    ) -> Option<String> {
        let now = common::constants::utils::current_timestamp_ms();
        let memory_id = uuid::Uuid::now_v7().to_string();
        let index = ShortTermMemoryIndexPo {
            id: memory_id.clone(),
            agent_id: agent_id.to_string(),
            task_id: ctx.task_id().cloned(),
            role: "assistant".to_string(),
            summary: summary.to_string(),
            tags: serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string()),
            trace_ids: serde_json::to_string(trace_ids).unwrap_or_else(|_| "[]".to_string()),
            status: MemoryStatus::Active,
            created_at: now,
            updated_at: now,
        };

        match self
            .memory()
            .create(ctx.clone(), MemoryCreateParams::CreateShortTerm(index))
            .await
        {
            Ok(_) => Some(memory_id),
            Err(e) => {
                crate::log_warn!(
                    &ctx,
                    "compact",
                    parent_trace_id = %parent_trace_id,
                    error = ?e,
                    "框架写入短期记忆失败"
                );
                None
            }
        }
    }

    /// 上下文压缩：把当前工作对话压缩为一条短期记忆
    ///
    /// # 与 sleep_and_settle 的差异
    /// - 复用 `work_messages` 全量上下文（追加指令而非重建），命中 prefix caching
    /// - **不操作 Agent 运行时状态**（不 set_resting / set_busy / set_idle）
    /// - 轮次预算是**场景级**的，不从唤醒总轮次里扣：
    ///   溢出回环 → [`COMPACT_MAX_ROUNDS`]；轮次耗尽的收尾回复（`with_user_reply`）
    ///   → Agent 的 `summary_max_rounds`（至少 1 轮）
    ///
    /// # 产物去向
    /// - 摘要由**框架**写入短期记忆（trace_ids / tags 由框架填写，不依赖模型）
    /// - 同一份摘要通过 [`CompactOutcome::compacted_summary`] 返回，
    ///   供 awaken 主循环直接注入下一轮，不再依赖重新查询历史记忆
    ///
    /// # 返回
    /// [`CompactOutcome`]。压缩失败不向上抛错 —— 它不该阻断主流程，调用方按需降级。
    ///
    /// 不接收 `brain`：压缩循环内部与 run_think_loop 一致地从 `agent.brain` 解析。
    pub(crate) async fn compact_context(
        &self,
        ctx: RequestContext,
        work_messages: &[ChatMessage],
        agent: &Agent,
        parent_trace_id: &str,
        trace_ids: &[String],
        with_user_reply: bool,
    ) -> Result<CompactOutcome> {
        let scene = ThinkingScene::Compact;
        let start_time = std::time::Instant::now();

        // 场景内轮次预算（不从唤醒总轮次里扣）：
        // - 溢出回环 / 正常收尾存档 → 固定小预算
        // - 轮次耗尽的收尾回复 → Agent 的 `summary_max_rounds`（这是最后一次
        //   向用户交付结果的机会，允许单独放宽；`.max(1)` 保证它至少能说话）
        let max_rounds = if with_user_reply {
            super::types::config_resolve::summary_max_rounds(agent).max(1)
        } else {
            COMPACT_MAX_ROUNDS
        };

        // 1. 构造压缩 trace（不写 Agent 状态，仅用于审计追溯）
        let mut trace = MemoryTrace::new(
            agent.po.id.clone(),
            format!("compact-{}", parent_trace_id),
            ctx.uid(),
            ctx.organization_id.clone().unwrap_or_default(),
            MemoryRole::System,
            String::new(),
            ctx.task_id().cloned(),
        );
        let trace_id = trace.id.clone();

        // 2. 复用主循环完整上下文 + 追加压缩指令
        //
        // 关键：只 push，不重建。push 之前的前缀与上一次模型调用逐字节一致，
        // 因此 provider 侧 prefix caching 可直接命中。
        let mut messages = work_messages.to_vec();
        let instruction = compact_instruction(with_user_reply);
        messages.push(ChatMessage::user(&instruction));
        trace.input = instruction.clone();

        // 3. 局部思考运行时：仅用于取消信号与策略评估，**不注册到全局状态**
        //    （注册会覆盖 awaken 的 think_runtime，也会让 BusyGuard 的清理逻辑误伤主循环）
        let think_runtime = Arc::new(AgentThinkRuntime::new(
            agent.po.id.clone(),
            trace_id.clone(),
        ));
        let policy = build_policy_for_scene(agent, scene, think_runtime.cancel_flag());

        // 4. 发布循环启动事件
        let _ = crate::pkg::aop::publish(
            &ctx,
            AgentLoopEvent::started(&agent.po.id, &trace_id, "compact", None),
        )
        .await;

        // 5. 执行压缩循环（场景内小预算：与 Agent 的唤醒级 max_thinking_rounds 无关）
        let tool_descriptors = build_scene_tool_descriptors(agent, scene);
        let result = self
            .run_think_loop(
                ThinkLoopParams::new(
                    ctx.clone(),
                    agent,
                    scene,
                    &trace_id,
                    messages,
                    &tool_descriptors,
                )
                .with_rounds(max_rounds, 0)
                .with_monitoring(&think_runtime, &policy),
            )
            .await;

        let (output, compacted_summary) = match result {
            Ok(crate::service::domain::runtime::types::ThinkLoopResult::Final {
                content, ..
            }) => {
                let summary = parse_compacted_summary(&content);
                (content, summary)
            }
            Ok(other) => {
                crate::log_warn!(
                    &ctx,
                    "compact",
                    "compact loop did not reach Final: {:?}",
                    other
                );
                (String::new(), None)
            }
            Err(e) => {
                crate::log_warn!(&ctx, "compact", "compact loop failed: {:?}", e);
                (String::new(), None)
            }
        };

        // 5.5 由框架落库：trace_ids / tags 我们填，保证可追溯且不会漏写。
        //
        // 交给模型调工具时，它完全可能不调或填错参数，那份工作就静默丢了；
        // 框架主动写则不存在这个问题。
        let compacted_memory_id = if let Some(summary) = &compacted_summary {
            self.persist_compacted_memory(
                ctx.clone(),
                &agent.po.id,
                summary,
                trace_ids,
                parent_trace_id,
            )
            .await
        } else {
            // 模型没按格式输出 = 本轮工作丢失（轮次会被丢弃且无内容补上），必须告警
            crate::log_warn!(
                &ctx,
                "compact",
                "compaction produced no {}{} block; the compacted work will be lost",
                SUMMARY_OPEN,
                SUMMARY_CLOSE
            );
            None
        };

        // 6. 写 Trace
        trace.output = Some(output.clone());
        let _ = self
            .memory()
            .write_thinking_trace(ctx.clone(), trace)
            .await
            .map_err(|e| {
                crate::log_warn!(&ctx, "compact", "write compact trace failed: {:?}", e);
                e
            });

        let _ = crate::pkg::aop::publish(
            &ctx,
            AgentLoopEvent::finished(
                &agent.po.id,
                &trace_id,
                "compact",
                "success",
                start_time.elapsed().as_millis() as u64,
                None,
            ),
        )
        .await;

        Ok(CompactOutcome {
            final_text: output,
            compacted_summary,
            compacted_memory_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_instruction_forbids_compressing_boilerplate() {
        let s = compact_instruction(false);
        // 明确只压缩「真正干的活儿」
        assert!(s.contains("只压缩"));
        assert!(s.contains("真正干的活儿"));
        // 逐项排除每次都会重新注入的内容
        assert!(s.contains("人设"));
        assert!(s.contains("技能"));
        assert!(s.contains("【用户画像】"));
        assert!(s.contains("【项目上下文】"));
        assert!(s.contains("【任务上下文】"));
        assert!(s.contains("【工作空间与路径约定】"));
        assert!(s.contains("【历史对话】"));
        // 给出可执行的判断标准
        assert!(s.contains("下一轮还会原样出现"));
        // 框架落库：不再要求模型调工具
        assert!(s.contains("不要调用任何工具"));
        assert!(!s.contains("save_short_term_memory 写入"));
        // 要求用标记包裹输出
        assert!(s.contains(SUMMARY_OPEN));
        assert!(s.contains(SUMMARY_CLOSE));
    }

    #[test]
    fn compact_instruction_with_user_reply_demands_marker() {
        let s = compact_instruction(true);
        assert!(s.contains(USER_REPLY_OPEN));
        assert!(s.contains(USER_REPLY_CLOSE));
        assert!(s.contains("直接发送给用户"));
        // 无回复场景不该出现回复标记
        assert!(!compact_instruction(false).contains(USER_REPLY_OPEN));
    }

    /// 轻量落档必须同时留下「用户说了什么」和「助手回了什么」
    ///
    /// 它是跳过压缩后下一轮唤醒能看到的唯一上下文，任一侧丢失都会断片。
    #[test]
    fn quick_turn_summary_keeps_both_sides() {
        let s = build_quick_turn_summary("  你好  ", " 你好，我是助手 ").expect("非空");
        assert_eq!(s, "用户：你好\n助手：你好，我是助手");
    }

    /// 阈值内的短对话保留**全文**，不做任何截断
    ///
    /// 这是「按长度分流」的核心承诺：短内容就是原文记账，
    /// 一旦仍按单侧 200 字截断，用户说的后半句就丢了，等于白记。
    #[test]
    fn quick_turn_summary_keeps_short_turn_verbatim() {
        let user: String = (0..300).map(|_| '问').collect();
        let reply: String = (0..80).map(|_| '答').collect();
        let s = build_quick_turn_summary(&user, &reply).expect("非空");
        assert_eq!(s, format!("用户：{user}\n助手：{reply}"));
        assert!(!s.ends_with('…'), "阈值内不应截断: {s}");
    }

    /// 超过硬上限的极长内容两侧均分截断兜底
    ///
    /// 只有直答/澄清（没有 messages 可压缩）才可能走到这里；兜底是为了
    /// 不让一条原文长期占用后续 ~20 轮 prompt 预算。
    #[test]
    fn quick_turn_summary_caps_extremely_long_turn() {
        let long: String = (0..QUICK_TURN_HARD_CAP_CHARS + 50).map(|_| '字').collect();
        let s = build_quick_turn_summary(&long, "").expect("非空");
        assert!(s.starts_with("用户："));
        assert!(s.ends_with('…'), "超硬上限应带省略号: {s}");
        assert!(!s.contains('\n'), "没有回复时不应出现助手行");
        // 截断按字符而非字节：中文不能切在半个字上
        assert_eq!(
            s.chars().count(),
            "用户：".chars().count() + QUICK_TURN_HARD_CAP_CHARS / 2 + 1
        );
    }

    #[test]
    fn quick_turn_summary_rejects_all_empty() {
        assert!(build_quick_turn_summary("   ", "\n").is_none());
    }

    /// 中文按 1 token/字，英文按 4 字符/token，混合文本逐类累加
    #[test]
    fn estimate_tokens_counts_cjk_and_latin_separately() {
        // 纯中文：一字一 token
        assert_eq!(estimate_tokens("你好世界"), 4);
        // 纯英文：4 字符一 token（向上取整）
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        // 混合：5 汉字 + 8 拉丁字符 = 5 + 2
        assert_eq!(estimate_tokens("你好世界啊abcdefgh"), 7);
        // 空白不特殊处理，仍按拉丁字符计入
        assert_eq!(estimate_tokens("   "), 1);
        assert_eq!(estimate_tokens(""), 0);
    }

    /// 分流依据是「用户 + 助手」的合计，不是单侧
    ///
    /// 反例：2000 字报错 + 一句「改第 42 行」，只看回复会被误判成短对话。
    #[test]
    fn estimate_turn_tokens_sums_both_sides() {
        assert_eq!(estimate_turn_tokens("你好", "世界"), 4);
        assert_eq!(estimate_turn_tokens("  ", "  "), 0);
    }

    #[test]
    fn parse_compacted_summary_extracts_marked_block() {
        let text =
            format!("前面一段话\n{SUMMARY_OPEN}\n完成了 A，待办是 B\n{SUMMARY_CLOSE}\n后面一段话");
        assert_eq!(
            parse_compacted_summary(&text),
            Some("完成了 A，待办是 B".to_string())
        );
    }

    #[test]
    fn parse_compacted_summary_tolerates_missing_close() {
        // 输出被截断时至少保住开头到结尾之间的内容
        let text = format!("{SUMMARY_OPEN}\n完成了 A");
        assert_eq!(parse_compacted_summary(&text), Some("完成了 A".to_string()));
    }

    #[test]
    fn parse_compacted_summary_returns_none_when_absent_or_empty() {
        assert_eq!(parse_compacted_summary("没有标记的一段话"), None);
        let empty = format!("{SUMMARY_OPEN}\n   \n{SUMMARY_CLOSE}");
        assert_eq!(parse_compacted_summary(&empty), None);
    }

    #[test]
    fn extract_user_reply_takes_marked_sentence_only() {
        let text = format!(
            "{SUMMARY_OPEN}\n内部摘要：已完成 A，待办 B\n{SUMMARY_CLOSE}\n\
             {USER_REPLY_OPEN}\n已完成三件事，因轮次耗尽停止。\n{USER_REPLY_CLOSE}"
        );
        assert_eq!(
            extract_user_reply(&text),
            Some("已完成三件事，因轮次耗尽停止。".to_string())
        );
    }

    /// 最关键的一条防线：模型漏打任何标记时，**绝不能**把内部摘要当回复
    ///
    /// 旧实现 `strip_summary_block` 在无标记时原样返回整段，用户会收到
    /// 「已完成的工作 / 还没做完的待办」这种内部口吻的摘要。
    #[test]
    fn extract_user_reply_never_leaks_summary() {
        // 只有摘要、没有回复标记
        let summary_only = format!("{SUMMARY_OPEN}\n已完成 A，待办 B\n{SUMMARY_CLOSE}");
        assert_eq!(extract_user_reply(&summary_only), None);

        // 完全没标记：无法区分是「纯摘要」还是「纯回复」，一律不发给用户
        assert_eq!(extract_user_reply("已完成 A，待办 B"), None);

        // 标记内为空
        let empty = format!("{USER_REPLY_OPEN}\n  \n{USER_REPLY_CLOSE}");
        assert_eq!(extract_user_reply(&empty), None);
    }
}
