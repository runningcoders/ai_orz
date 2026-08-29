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

/// 压缩场景的思考轮次上限
///
/// 压缩只需调用一次 `save_short_term_memory` 再输出 Final，给 3 轮足够容错
/// （工具参数写错时模型还有自纠机会）；同时它挡在用户等待的关键路径上，不宜给大预算。
const COMPACT_MAX_ROUNDS: usize = 3;

/// 压缩场景允许递归调用的工具白名单守卫
///
/// 处于沉淀/压缩流程中时，模型再调 `settle_memory` 属于自我递归，
/// 直接跳过执行并回一条提示，避免「压缩里再触发一次完整沉淀」的雪崩。
pub(crate) const RECURSIVE_SETTLE_TOOL: &str = "settle_memory";

/// 框架落库时给压缩记忆打的标签（便于后续识别/统计这类记忆）
const COMPACTION_TAG: &str = "compaction";

/// 压缩摘要的起止标记
///
/// 压缩产出由**框架**落库（不交给模型调工具），因此要求模型把摘要包在标记里，
/// 框架从 Final 文本中解析。选这种朴素标记而非 JSON：压缩轮可能已被截断，
/// 结构化输出一旦不完整就整体解析失败，标记法至少能保住开头到结尾之间的内容。
const SUMMARY_OPEN: &str = "<compacted_summary>";
const SUMMARY_CLOSE: &str = "</compacted_summary>";

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
        s.push_str(
            "\n摘要之后，另起一段**用一句话**告诉用户：你进行到了哪一步、为什么停在这里。\n\
             这条文本会直接发送给用户（摘要部分不会）。\n",
        );
    } else {
        s.push_str("\n除摘要外不要输出其它内容。\n");
    }
    s
}

/// 去掉 Final 文本里的摘要区块，只留面向用户的正文
///
/// `with_user_reply` 场景下模型会同时输出摘要与「给用户的一句话」，
/// 摘要是内部产物，不该发给用户，这里把它剥掉。
pub(crate) fn strip_summary_block(final_text: &str) -> String {
    let Some(start) = final_text.find(SUMMARY_OPEN) else {
        return final_text.trim().to_string();
    };
    let after = &final_text[start..];
    let end = after
        .find(SUMMARY_CLOSE)
        .map(|i| i + SUMMARY_CLOSE.len())
        .unwrap_or(after.len());
    format!("{}{}", &final_text[..start], &after[end..])
        .trim()
        .to_string()
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
        let now = common::constants::utils::current_timestamp_ms();
        let memory_id = uuid::Uuid::now_v7().to_string();
        let index = ShortTermMemoryIndexPo {
            id: memory_id.clone(),
            agent_id: agent_id.to_string(),
            task_id: ctx.task_id().cloned(),
            role: "assistant".to_string(),
            summary: summary.to_string(),
            tags: serde_json::to_string(&vec![COMPACTION_TAG]).unwrap_or_else(|_| "[]".to_string()),
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
                    "框架写入压缩记忆失败"
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
    /// - 轮次预算固定为 [`COMPACT_MAX_ROUNDS`]，与 Agent 的 max_thinking_rounds 无关
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
        let _ = crate::pkg::aop::publish(AgentLoopEvent::started(
            &agent.po.id,
            &trace_id,
            "compact",
            None,
        ))
        .await;

        // 5. 执行压缩循环（小轮次预算，与 Agent 的 max_thinking_rounds 无关）
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
                .with_rounds(COMPACT_MAX_ROUNDS, 0)
                .with_monitoring(&think_runtime, policy.as_ref()),
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

        let _ = crate::pkg::aop::publish(AgentLoopEvent::finished(
            &agent.po.id,
            &trace_id,
            "compact",
            "success",
            start_time.elapsed().as_millis() as u64,
            None,
        ))
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
    fn compact_instruction_with_user_reply_adds_sentence() {
        let s = compact_instruction(true);
        assert!(s.contains("告诉用户"));
        assert!(s.contains("用一句话"));
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
    fn strip_summary_block_removes_internal_summary() {
        let text =
            format!("{SUMMARY_OPEN}\n内部摘要\n{SUMMARY_CLOSE}\n已完成三件事，因轮次耗尽停止。");
        assert_eq!(strip_summary_block(&text), "已完成三件事，因轮次耗尽停止。");
    }

    #[test]
    fn strip_summary_block_keeps_text_without_marker() {
        assert_eq!(strip_summary_block("只是一句话"), "只是一句话");
    }
}
