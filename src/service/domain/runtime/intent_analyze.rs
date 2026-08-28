//! 意图分析（Phase 1）
//!
//! `analyze_input_intent_inner` 是两阶段唤醒的 Phase 1 核心实现，
//! 跑一轮 IntentAnalyze 小循环产出结构化意图理解结果，**不执行任何业务动作**。
//!
//! JSON 解析函数（`parse_intent_analysis_json` 等）提供 6 级降级策略，
//! 尽量从 Agent 自由文本输出中提取结构化 IntentAnalysis。

use crate::models::agent::Agent;
use crate::models::message::Message;
use crate::pkg::paths;
use crate::pkg::request_context::RequestContext;
use crate::service::domain::runtime::{RuntimeDomain, RuntimeDomainImpl};
use common::enums::ThinkingScene;
use common::error::{Result, err};

use super::awakening::{
    build_scene_skills, build_scene_tool_descriptors, init_think_runtime_and_policy,
};
use super::types::config_resolve;
use super::types::{IntentAnalysis, ThinkingOptions};

impl RuntimeDomainImpl {
    /// analyze_input_intent 的核心实现（由 trait 方法委托，外层负责 stats 包裹）
    ///
    /// 流程：构造 IntentAnalyze 场景 → 读短期记忆 → 过滤技能 → 拼 Prompt →
    /// think loop（最多 2 轮）→ 解析 IntentAnalysis JSON
    pub(crate) async fn analyze_input_intent_inner(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        message: &Message,
        options: &ThinkingOptions,
        trace_id: &str,
    ) -> Result<IntentAnalysis> {
        // 1. 强制构造出 IntentAnalyze 场景专用 options（覆盖 scene）
        let mut analyze_opts = options.clone();
        analyze_opts.scene = ThinkingScene::IntentAnalyze;
        // 轮次和超时由 config_resolve 从 Agent 配置 + 系统配置解析，
        // 直接传入 run_think_loop，不经过 ThinkingOptions
        let scene = analyze_opts.scene;

        // 2. 查最近 20 条短期记忆做上下文（与 awaken 相同窗口，保证 Agent 有历史可读做消歧）
        let recent_memories = self
            .memory()
            .get_recent_context(ctx.clone(), &agent.po.id, 20)
            .await?;

        // 3. 按 IntentAnalyze 场景过滤技能（严格只保留理解类标签）
        let skill_pos = build_scene_skills(agent, scene);

        // 4. 构造 PromptBuilder（与 awaken 相同挂载链路，保证背景知识一致）
        let mut builder = self.prompt_builder(agent);
        builder.current_trace_id(trace_id);
        builder.system_prompt(agent);
        builder.skills(&skill_pos);
        let base = crate::config::get().base_data_path();
        let uid = ctx.uid();
        let uid_ref = if uid.is_empty() {
            None
        } else {
            Some(uid.as_str())
        };
        let default_workspace = paths::default_workspace(&base, uid_ref, Some(&agent.po.id))
            .to_string_lossy()
            .to_string();
        let user_home = if uid.is_empty() {
            paths::users_root_dir(&base).to_string_lossy().to_string()
        } else {
            paths::user_home(&base, &uid).to_string_lossy().to_string()
        };
        let user_shared_workspace = if uid.is_empty() {
            default_workspace.clone()
        } else {
            paths::user_shared_workspace(&base, &uid)
                .to_string_lossy()
                .to_string()
        };
        let user_agent_workspace = if uid.is_empty() {
            None
        } else {
            Some(
                paths::user_agent_workspace(&base, &uid, &agent.po.id)
                    .to_string_lossy()
                    .to_string(),
            )
        };
        let agent_workspace = Some(
            paths::agent_workspace(&base, &agent.po.id)
                .to_string_lossy()
                .to_string(),
        );
        let project_workspace =
            if let (Some(project), true) = (&analyze_opts.project, !uid.is_empty()) {
                Some(
                    paths::user_project_workspace(&base, &uid, &project.po.id)
                        .to_string_lossy()
                        .to_string(),
                )
            } else {
                None
            };
        builder.workspace_context(
            default_workspace,
            user_home,
            user_shared_workspace,
            user_agent_workspace,
            agent_workspace,
            project_workspace,
        );
        if let Some(project) = &analyze_opts.project {
            builder.project_context(project);
        }
        if let Some(task) = &analyze_opts.task {
            builder.task_context(task);
        }
        if let Some(user) = &analyze_opts.user_profile {
            builder.user_profile(user);
        }
        builder.history(&recent_memories);
        builder.current_message(message);

        // 5. 组装专用 Prompt（不是普通 build()）
        let _prompt = builder.build_intent_analyze_prompt();
        // P0-b：用 System + User 双角色拆分版初始消息
        let initial_messages = builder.build_intent_analyze_initial_messages();

        // 6. 取 Agent Brain（调用方需已通过 wake_agent_brain 装配）
        let brain = agent
            .brain
            .as_ref()
            .ok_or_else(|| err!(Internal, "Agent 大脑未唤醒，请先调用 wake_agent_brain()"))?;

        // 7. 按场景构建工具描述符列表（严格白名单，只允许理解类工具）
        let tool_descriptors = build_scene_tool_descriptors(agent, scene);

        // 8. 运行 think loop（轮次由配置决定，保证多步检索后有足够轮次输出 Final JSON）
        // IntentAnalyze 是 awaken 的 Phase 1 子流程，但同样是一个完整的 think_loop，
        // 需要独立的 think_runtime 和 policy（覆盖 awaken 的，awaken 循环中会重新设置）
        let (think_runtime, policy) =
            init_think_runtime_and_policy(agent, ThinkingScene::IntentAnalyze, trace_id);
        let think_result = self
            .run_think_loop(
                ctx.clone(),
                brain,
                initial_messages,
                &tool_descriptors,
                agent,
                ThinkingScene::IntentAnalyze,
                trace_id,
                config_resolve::intent_analyze_max_rounds(agent),
                0,
                config_resolve::think_timeout_secs(agent),
                Some(&think_runtime),
                Some(policy.as_ref()),
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
    let summary = val
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 至少要有 intent_type 或 summary 任一非空，才算解析出有效结果
    if intent_type.is_empty() && summary.is_empty() {
        return None;
    }

    Some(IntentAnalysis {
        intent_type,
        confidence,
        key_terms,
        resolutions,
        retrieved_context,
        need_clarification,
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
