//! 意图分析（Phase 1）
//!
//! `analyze_input_intent_inner` 是两阶段唤醒的 Phase 1 核心实现，
//! 跑一轮 IntentAnalyze 小循环产出结构化意图理解结果，**不执行任何业务动作**。
//!
//! JSON 解析函数（`parse_intent_analysis_json` 等）提供 6 级降级策略，
//! 尽量从 Agent 自由文本输出中提取结构化 IntentAnalysis。

use super::types::ThinkLoopParams;
use crate::models::agent::Agent;
use crate::models::cortex_types::ChatMessage;
use crate::models::message::Message;
use crate::models::skill::SkillPo;
use crate::pkg::request_context::RequestContext;
use crate::service::domain::runtime::RuntimeDomainImpl;
use common::enums::ThinkingScene;
use common::error::{Result, err};

use super::awakening::{
    PromptBaseInput, build_scene_tool_descriptors, fetch_message_business_blocks, fill_prompt_base,
    init_think_runtime_and_policy,
};
use super::think_loop::memory_index_budget_for;
use super::types::config_resolve;
use super::types::{IntentAnalysis, ThinkingOptions};

/// 意图识别阶段的可选追加/回写能力
pub(crate) struct IntentPhaseInput<'a> {
    /// 增量修订入参：上次完整结论 + 新增输入（首次识别为 `None`）
    pub revision: Option<&'a super::inbox::IntentRevision>,
    /// Phase 1 初始消息快照回写槽（由正式装配读取作为请求前缀）
    pub seed: Option<&'a std::sync::Mutex<Option<Vec<ChatMessage>>>>,
}

impl RuntimeDomainImpl {
    /// analyze_input_intent 的核心实现（由 trait 方法委托，外层负责 stats 包裹）
    ///
    /// 流程：构造 IntentAnalyze 场景 → 读短期记忆 → 过滤技能 → 拼 Prompt →
    /// think loop（轮次上限 = `[agent].intent_analyze_max_rounds`，见
    /// `common/config/ai_orz.toml`）→ 解析 IntentAnalysis JSON
    pub(crate) async fn analyze_input_intent_inner(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        message: &Message,
        options: &ThinkingOptions,
        trace_id: &str,
        extra: IntentPhaseInput<'_>,
    ) -> Result<IntentAnalysis> {
        // 0. 意图分析前的成本门（P3-a · K4-b）：零信息量输入本地短路；其余由小脑
        //    判「这条消息值不值得做意图分析」。跳过 = 降级到现状（key_terms 用用户
        //    原文），任何降级（无小脑 / 配额耗尽 / 调用失败 / 低置信）都走原逻辑。
        if let Some(skipped) = super::intent_gate::skip_if_pointless_with_default_dao(
            &ctx,
            agent.brain(),
            message.content(),
            trace_id,
        )
        .await
        {
            return Ok(skipped);
        }

        // 1. 强制构造出 IntentAnalyze 场景专用 options（覆盖 scene）
        let mut analyze_opts = options.clone();
        analyze_opts.scene = ThinkingScene::IntentAnalyze;

        // 2. 分页向前多查一批短期记忆做上下文（保证 Agent 有历史可读做消歧）
        //    ⚠️ T0 = 入站消息时间：同一次唤醒链内固定，Phase 1 快照与 Phase 2
        //    重建的 base 必须取到逐字节一致的记忆集合，否则前缀缓存整段失配。
        let recent_memories = self
            .load_memory_index_window(
                ctx.clone(),
                &agent.po.id,
                message.po.created_at,
                memory_index_budget_for(agent),
            )
            .await?;

        // 3. 全量技能 + 业务块：与正式装配同一来源，保证 base 逐字节一致
        let skills: Vec<SkillPo> = agent.skills().iter().map(|s| s.po.clone()).collect();
        let (message_thread, resource_lines, vision_parts) =
            fetch_message_business_blocks(&ctx, message).await;

        // 4. 唯一 base 装配（System + 公共上下文 + 历史 + 消息链 + 资源/视觉 + 当前消息）
        let mut builder = self.prompt_builder(agent);
        fill_prompt_base(
            builder.as_mut(),
            PromptBaseInput {
                ctx: &ctx,
                agent,
                message,
                options: &analyze_opts,
                trace_id,
                recent_memories: &recent_memories,
                skills: &skills,
                compacted: None,
                past_memories: &[],
                message_thread: &message_thread,
                resource_lines: &resource_lines,
                vision_parts: &vision_parts,
            },
        )
        .await;

        // 增量修订：上次结论 + 新增输入追加在 base 尾部（不插入 base 中间）
        if let Some(revision) = extra.revision {
            builder.intent_revision(&revision.previous, &revision.inputs);
        }

        // 5. 初始消息 = base + 意图指令尾插；把同一份快照交给 Phase 2 作为请求前缀
        let initial_messages = builder.build_intent_analyze_initial_messages();
        if let Some(seed) = extra.seed
            && let Ok(mut slot) = seed.lock()
        {
            *slot = Some(initial_messages.clone());
        }

        // 6. Agent Brain 由 run_think_loop 内部解析（调用方需已通过 wake_agent_brain 装配）

        // 7. 工具描述符与正式阶段完全一致（全量）；允许集合改在 run_dispatch 调用时拦截
        let tool_descriptors = build_scene_tool_descriptors(agent, ThinkingScene::Awaken);

        // 8. 运行 think loop（轮次由配置决定，保证多步检索后有足够轮次输出 Final JSON）
        // IntentAnalyze 是 awaken 的 Phase 1 子流程，但同样是一个完整的 think_loop，
        // 需要独立的 think_runtime 和 policy（覆盖 awaken 的，awaken 循环中会重新设置）
        let (think_runtime, policy) =
            init_think_runtime_and_policy(agent, ThinkingScene::IntentAnalyze, trace_id);
        let think_result = self
            .run_think_loop(
                ThinkLoopParams::new(
                    ctx.clone(),
                    agent,
                    ThinkingScene::IntentAnalyze,
                    trace_id,
                    initial_messages,
                    &tool_descriptors,
                )
                .with_rounds(config_resolve::intent_analyze_max_rounds(agent), 0)
                .with_monitoring(&think_runtime, &policy),
            )
            .await?;

        // 9. 取最终回答文本
        let final_text = match think_result {
            super::types::ThinkLoopResult::Final { content, .. } => content,
            super::types::ThinkLoopResult::ContextOverflow { .. } => {
                return Err(err!(
                    Internal,
                    "analyze_input_intent context overflow (unexpected for 2-round limit)"
                ));
            }
            super::types::ThinkLoopResult::MaxRoundsExceeded { .. } => {
                return Err(err!(
                    Internal,
                    "analyze_input_intent max rounds exceeded without Final (Agent failed to output JSON)"
                ));
            }
            super::types::ThinkLoopResult::Cancelled { .. } => {
                return Err(err!(
                    Internal,
                    "analyze_input_intent cancelled by user before Final"
                ));
            }
        };

        // 10. 解析 IntentAnalysis JSON（6 级降级，全部失败则返回 Err，由调用方降级）
        parse_intent_analysis_json(&final_text)
    }
}

// ==================== IntentAnalysis JSON 解析（6 级降级）====================

/// 从 Agent Final 文本中按 5 级降级策略尽量提取并解析 IntentAnalysis JSON
///
/// # 降级策略
/// 1. 整段文本直接 JSON 反序列化
/// 2. 手动查找 ```json ... ``` 或 ``` ... ``` 代码块，提取内容再解析
/// 3. 查找 INTENT_ANALYSIS_START/END 锚点标记之间的内容
/// 4. 平衡括号法：从第一个 { 开始找到匹配的顶层 }，提取中间内容再解析
///    (含字段类型宽容修复：confidence 字符串→数字、缺省字段 Default)
/// 5. 取第一个 { 与最后一个 } 之间的子串尝试解析
/// 6. 全部失败 → 返回 Err（错误信息含文本前缀，便于调试日志）
pub fn parse_intent_analysis_json(text: &str) -> Result<IntentAnalysis> {
    let text = text.trim();
    if text.is_empty() {
        return Err(err!(Internal, "parse_intent_analysis_json: empty text"));
    }

    // ===== Level 1: 整段文本直接解析 =====
    if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(text) {
        return Ok(ia);
    }

    // ===== Level 2: 手动查找 ```json ... ``` 或 ``` ... ``` 代码块 =====
    let mut cursor = text;
    while let Some(start) = cursor.find("```") {
        let after_first = &cursor[start + 3..];
        // 跳过可选的 "json" 标识符 + 空白
        let after_lang = if let Some(rest) = after_first.strip_prefix("json") {
            rest.trim_start_matches([' ', '\n', '\r', '\t'])
        } else {
            after_first.trim_start_matches([' ', '\n', '\r', '\t'])
        };
        if let Some(end) = after_lang.find("```") {
            let inner = after_lang[..end].trim();
            if !inner.is_empty() {
                if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(inner) {
                    return Ok(ia);
                }
                // 如果直接 IntentAnalysis 失败，可能是锚点包裹或字段类型问题，
                // 进入宽容解析流程
                if let Some(ia) = try_lenient_parse(inner) {
                    return Ok(ia);
                }
            }
            // 继续在剩余文本中寻找下一组 ```
            cursor = &after_lang[end.saturating_add(3)..];
            continue;
        }
        break;
    }

    // ===== Level 3: 查找 INTENT_ANALYSIS_START/END 锚点之间的内容 =====
    if let Some(start_marker) = text.find("--- INTENT_ANALYSIS_START ---") {
        let after_start = &text[start_marker + "--- INTENT_ANALYSIS_START ---".len()..];
        if let Some(end_marker) = after_start.find("--- INTENT_ANALYSIS_END ---") {
            let inner = after_start[..end_marker].trim();
            if !inner.is_empty() {
                if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(inner) {
                    return Ok(ia);
                }
                if let Some(ia) = try_lenient_parse(inner) {
                    return Ok(ia);
                }
            }
        }
    }

    // ===== Level 4: 平衡括号法提取第一个完整 JSON 对象 =====
    if let Some(json_obj) = extract_first_json_object(text) {
        if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(json_obj) {
            return Ok(ia);
        }
        if let Some(ia) = try_lenient_parse(json_obj) {
            return Ok(ia);
        }
    }

    // ===== Level 5: 取第一个 { 到最后一个 } 之间的子串 =====
    let first_brace = text.find('{');
    let last_brace = text.rfind('}');
    if let (Some(first), Some(last)) = (first_brace, last_brace)
        && first < last
    {
        let inner = &text[first..=last];
        if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(inner) {
            return Ok(ia);
        }
        if let Some(ia) = try_lenient_parse(inner) {
            return Ok(ia);
        }
    }

    // ===== Level 6 (全部失败): 返回 Err 含文本前缀便于调试 =====
    let preview: String = text.chars().take(120).collect();
    Err(err!(
        Internal,
        "parse_intent_analysis_json: all strategies failed. Text prefix: {}",
        preview
    ))
}

/// 宽容解析：先 parse 成 serde_json::Value，再手动按字段提取并做类型宽容转换
/// （解决 Agent 偶发把 confidence 写成字符串、数组里混非字符串等问题）
fn try_lenient_parse(s: &str) -> Option<IntentAnalysis> {
    use serde_json::Value;

    let val: Value = serde_json::from_str(s).ok()?;
    let intent_type = val
        .get("intent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let confidence = val
        .get("confidence")
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
        })
        .unwrap_or(0.0) as f32;
    let extract_str_arr = |key: &str| -> Vec<String> {
        val.get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| match x {
                        Value::String(s) => Some(s.clone()),
                        Value::Number(n) => Some(n.to_string()),
                        Value::Bool(b) => Some(b.to_string()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let key_terms = extract_str_arr("key_terms");
    let resolutions = extract_str_arr("resolutions");
    let retrieved_context = extract_str_arr("retrieved_context");
    let need_clarification = extract_str_arr("need_clarification");
    let direct_answer = val
        .get("direct_answer")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let summary = val
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 至少要有 intent_type / summary / 直答答复任一非空，才算解析出有效结果
    if intent_type.is_empty() && summary.is_empty() && direct_answer.is_empty() {
        return None;
    }

    Some(IntentAnalysis {
        intent_type,
        confidence,
        key_terms,
        resolutions,
        retrieved_context,
        need_clarification,
        direct_answer,
        summary,
    })
}

/// 简易括号匹配：从字符串中找到第一个顶层的 { ... } 完整 JSON 对象
///
/// 支持字符串内部出现大括号的情况：遇到未转义的双引号进入字符串模式，
/// 字符串内部的 {} 不计入括号计数。
pub fn extract_first_json_object(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let mut depth = 0;
            let start = i;
            let mut in_string = false;
            let mut escape = false;
            while i < bytes.len() {
                let b = bytes[i];
                if escape {
                    escape = false;
                    i += 1;
                    continue;
                }
                if b == b'\\' {
                    escape = true;
                    i += 1;
                    continue;
                }
                if b == b'"' {
                    in_string = !in_string;
                    i += 1;
                    continue;
                }
                if !in_string {
                    if b == b'{' {
                        depth += 1;
                    } else if b == b'}' {
                        depth -= 1;
                        if depth == 0 {
                            let end = i + 1;
                            return Some(&s[start..end]);
                        }
                    }
                }
                i += 1;
            }
            return None;
        }
        i += 1;
    }
    None
}
