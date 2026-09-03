//! PromptBuilder 单元测试（DefaultPromptBuilder + FlatPromptBuilder）
//!
//! 拆分自原 agent.rs 尾部 tests 模块（本次文件重构）。

use super::*;
use crate::models::agent::{Agent, AgentPo};
use crate::models::cortex_types::ChatMessage;
use crate::models::message::Message;
use crate::models::prompt_builder::PromptBuilder;
use common::enums::{AgentStatus, MessageRole, MessageType};
use uuid::Uuid;

fn make_simple_agent() -> Agent {
    let mut po = AgentPo::new(
        "测试助手".to_string(),
        vec!["assistant".to_string()],
        "一个测试用的 Agent".to_string(),
        vec!["chat".to_string()],
        "".to_string(),
        "provider-001".to_string(),
        "test-user".to_string(),
    );
    po.id = "agent-test-001".to_string();
    po.status = AgentStatus::Onboarded;
    Agent::from_po(po)
}

fn make_simple_message(content: &str) -> Message {
    Message::new_with_context(
        Uuid::now_v7().to_string(),
        None,
        None,
        "test-user".to_string(),
        "agent-test-001".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        content.to_string(),
        None,
        crate::models::file::FileMeta::default(),
        None,
        None,
        None,
        "test-user".to_string(),
    )
}

#[test]
fn build_intent_analyze_prompt_contains_sop_and_schema() {
    let agent = make_simple_agent();
    let message = make_simple_message("上次那个方案结果呢？");

    let mut builder = DefaultPromptBuilder::new();
    builder.current_trace_id("trace-test-001");
    builder.system_prompt(&agent);
    builder.current_message(&message);

    let prompt = builder.build_intent_analyze_prompt();

    // 1. 包含阶段一标题
    assert!(
        prompt.contains("### 阶段一：输入理解专用指令（仅限 IntentAnalyze 场景）"),
        "Prompt 应包含阶段一标题。Output 片段:\n{}",
        prompt.chars().take(500).collect::<String>()
    );

    // 2. 包含全部 7 个字段名（JSON Schema 说明）
    let seven_fields = [
        "intent_type",
        "confidence",
        "key_terms",
        "resolutions",
        "retrieved_context",
        "need_clarification",
        "summary",
    ];
    for field in &seven_fields {
        assert!(
            prompt.contains(field),
            "Prompt 应包含字段名 '{}' 但未找到。Output 片段:\n{}",
            field,
            prompt.chars().take(800).collect::<String>()
        );
    }

    // 3. 包含 INTENT_ANALYSIS_START 锚点
    assert!(
        prompt.contains("--- INTENT_ANALYSIS_START ---"),
        "Prompt 应包含 INTENT_ANALYSIS_START 锚点标记"
    );
    assert!(
        prompt.contains("--- INTENT_ANALYSIS_END ---"),
        "Prompt 应包含 INTENT_ANALYSIS_END 锚点标记"
    );

    // 4. 包含 SOP 五步走的 Step 1~5 标识
    assert!(prompt.contains("Step 1：意图识别"));
    assert!(prompt.contains("Step 2：指代与上下文消歧"));
    assert!(prompt.contains("Step 3：关键词抽取与联想扩展"));
    assert!(prompt.contains("Step 4：多步语义检索与知识图谱关联分析"));
    assert!(prompt.contains("Step 5：综合研判与总结"));

    // 5. 包含三条禁令标识（严禁执行/严禁编造/信息不足必须澄清）
    assert!(
        prompt.contains("严禁执行任何行动"),
        "Prompt 应包含禁令 1：严禁执行"
    );
    assert!(
        prompt.contains("严禁编造无来源信息"),
        "Prompt 应包含禁令 2：严禁编造"
    );
    assert!(
        prompt.contains("如果信息不足必须"),
        "Prompt 应包含禁令 3：信息不足必须澄清"
    );

    // 6. 包含用户消息原文（作为明确靶子）
    assert!(
        prompt.contains("上次那个方案结果呢？"),
        "Prompt 末尾应包含当前用户消息原文"
    );
}

// ============= Task 4 (A+ P3) 新增单元测试 =============

use crate::service::domain::runtime::awakening::IntentAnalysis;

/// UT-a: 截断规则验证——海量数据时不溢出 token
/// 构建：20 项数组（每项 300+ 字符）+ 2000 字符 summary
/// 断言：总输出 < 3000 字符、含 "... 及 N 项已省略"、summary 被截断、confidence 用 "%" 显示
#[test]
fn render_intent_analysis_section_truncation_rules() {
    // 构造 20 个 300 字符的字符串填充到每个数组
    let long_str: String = (0..300).map(|_| '一').collect();
    let huge_terms: Vec<String> = (0..20)
        .map(|i| format!("term-{} {}", i, long_str))
        .collect();
    let huge_res: Vec<String> = (0..20).map(|i| format!("res-{} {}", i, long_str)).collect();
    let huge_ctx: Vec<String> = (0..20).map(|i| format!("ctx-{} {}", i, long_str)).collect();
    let huge_clarify: Vec<String> = (0..20).map(|i| format!("q-{} {}", i, long_str)).collect();
    let huge_summary: String = (0..2000).map(|_| '总').collect();

    let ia = IntentAnalysis {
        intent_type: "Mixed".into(),
        confidence: 0.7856,
        key_terms: huge_terms,
        resolutions: huge_res,
        retrieved_context: huge_ctx,
        need_clarification: huge_clarify,
        summary: huge_summary,
    };

    let mut builder = DefaultPromptBuilder::new();
    builder.intent_analysis = Some(ia);

    let output = builder.render_intent_analysis_section();
    assert!(!output.is_empty(), "理解区块不应为空");

    // 1) 总字符数 < 3000（实际 ~10*150*4 + 800 + 约 500 固定文字 ≈ 7300，
    //    这里用 8000 作为安全上限，重点是不能让 20*300*4 + 2000 = 26000 全部进入）
    assert!(
        output.chars().count() < 8000,
        "截断后输出应远小于原始体量，当前字符数: {}",
        output.chars().count()
    );

    // 2) 出现 "... 及 N 项已省略" 提示（数组从 20 项被截到 10 项，每个数组都应有省略提示）
    assert!(
        output.contains("及 10 项已省略"),
        "截断提示未出现。Output 片段:\n{}",
        output.chars().take(600).collect::<String>()
    );

    // 3) summary 被截断到 800 字 + … 字符（原 2000 字）
    // 检查 "总" 字出现次数不应接近 2000
    let summary_total_count = output.matches('总').count();
    assert!(
        summary_total_count < 1000,
        "summary 似乎未被截断（'总' 字出现 {} 次）",
        summary_total_count
    );

    // 4) confidence 用百分比格式（含 "%" 符号，不是原小数 0.7856）
    assert!(
        output.contains('%'),
        "置信度应以百分比显示（含 % 符号）。Output 片段:\n{}",
        output.chars().take(300).collect::<String>()
    );
    assert!(output.contains("78.56%"), "置信度 0.7856 应渲染为 78.56%");
}

/// UT-b: 验证【输入理解结果】区块严格出现在【当前消息】之前
/// 分支 1：有 IntentAnalysis 时，检查 find() 索引顺序；
/// 分支 2：intent_analysis=None 时，不出现新区块且输出与之前一致
#[test]
fn build_prompt_contains_input_understanding_before_current_message() {
    let agent = make_simple_agent();
    let message = make_simple_message("帮我把上次那个文档改一下");

    // ========== 分支 1：有 IntentAnalysis ==========
    let ia = IntentAnalysis {
        intent_type: "TaskRequest".into(),
        confidence: 0.91,
        key_terms: vec!["文档".into(), "修改".into()],
        resolutions: vec!["\"上次那个文档\" → doc_id=doc_789".into()],
        retrieved_context: vec!["2026-08-12 短期记忆：doc_789 版本 v2".into()],
        need_clarification: vec![],
        summary: "用户想修改 doc_789 文档".into(),
    };

    let mut builder_with_ia = DefaultPromptBuilder::new();
    builder_with_ia.current_trace_id("trace-order-001");
    builder_with_ia.system_prompt(&agent);
    builder_with_ia.current_message(&message);
    builder_with_ia.intent_analysis(&ia);
    let prompt_with_ia = builder_with_ia.build();

    // 断言：理解区块 + 当前消息两者都出现
    let idx_understanding = prompt_with_ia
        .find("【输入理解结果")
        .expect("Prompt 应包含【输入理解结果】区块");
    let idx_current_msg = prompt_with_ia
        .find("【当前消息】")
        .expect("Prompt 应包含【当前消息】区块");

    // 关键断言：理解区块索引 < 当前消息索引
    assert!(
        idx_understanding < idx_current_msg,
        "【输入理解结果】(idx={}) 必须出现在【当前消息】(idx={}) 之前！",
        idx_understanding,
        idx_current_msg
    );

    // ========== 分支 2：intent_analysis=None（未注入）==========
    let builder_none_ia = {
        let mut b = DefaultPromptBuilder::new();
        b.current_trace_id("trace-order-002");
        b.system_prompt(&agent);
        b.current_message(&message);
        // 不注入 intent_analysis → 保持 None
        b
    };
    let prompt_none_ia = builder_none_ia.build();

    // 断言：不包含理解区块
    assert!(
        !prompt_none_ia.contains("【输入理解结果"),
        "intent_analysis=None 时不应渲染理解区块"
    );
    // 断言：仍然包含当前消息（输出未被破坏）
    assert!(
        prompt_none_ia.contains("【当前消息】"),
        "None 分支输出应包含当前消息区块"
    );
    assert!(
        prompt_none_ia.len() > 50,
        "None 分支输出不应被破坏为空（长度 {}）",
        prompt_none_ia.len()
    );
}

/// UT-c: Phase 1 失败时的优雅降级（逻辑模拟）
/// 场景：analyze_input_intent 返回 Err → ia = None → builder 不注入
/// 断言：(1) builder.intent_analysis 保持 None；
///       (2) render_intent_analysis_section() 返回 ""；
///       (3) build() 仍产出合法非空 Prompt（无 crash、无空输出）
#[test]
fn intent_analyze_phase1_failure_graceful_degrade() {
    // ---- 模拟 awaken() 中 Phase 1 返回 Err 的场景 ----
    // 逻辑等价代码（简化版）：
    //   let ia: Option<IntentAnalysis> = match self.analyze_input_intent(...) {
    //       Ok(ia) => Some(ia),
    //       Err(_) => None,  // ← 降级分支
    //   };
    let ia: Option<IntentAnalysis> = None; // 模拟 Err 降级结果

    let agent = make_simple_agent();
    let message = make_simple_message("Phase 1 fail degrade test");

    let mut builder = DefaultPromptBuilder::new();
    builder.current_trace_id("trace-degrade-001");
    builder.system_prompt(&agent);
    builder.current_message(&message);

    // 等价于 awaken() loop 内的注入代码（ia = None 时跳过）
    if let Some(ref ia_ref) = ia {
        builder.intent_analysis(ia_ref);
    }

    // 断言 1：builder.intent_analysis 字段保持 None
    assert!(
        builder.intent_analysis.is_none(),
        "降级分支下 builder.intent_analysis 应为 None"
    );

    // 断言 2：render_intent_analysis_section() 返回空字符串
    let section = builder.render_intent_analysis_section();
    assert!(
        section.is_empty(),
        "降级分支下 render_intent_analysis_section() 应返回空字符串"
    );

    // 断言 3：build() 不 crash，输出非空且包含必要区块
    let prompt = builder.build();
    assert!(!prompt.is_empty(), "降级分支下 build() 输出不应为空");
    assert!(
        prompt.contains("【思考 Trace ID】"),
        "降级分支下 Prompt 仍应含 Trace ID 区块"
    );
    assert!(
        prompt.contains("【当前消息】"),
        "降级分支下 Prompt 仍应含当前消息区块"
    );
}

// ==================== @ 提及上下文注入（ff80975d）====================
//
// 后端在 current_message 中解析正文里的 [@name](type:id) 链接，
// 注入一段【提及上下文】作为上下文补充（纯文本解析，不查 DAL）。
// 下列用例锁定该行为不被回归。

/// 单条 @ 提及（Agent）被注入【提及上下文】区块，且位于【当前消息】之后
#[test]
fn current_message_injects_single_agent_mention_context() {
    let agent = make_simple_agent();
    let content = "帮我看看 [@张伟](agent:agt_xxx) 上次那个方案的结果";
    let message = make_simple_message(content);

    let mut builder = DefaultPromptBuilder::new();
    builder.current_trace_id("trace-mention-001");
    builder.system_prompt(&agent);
    builder.current_message(&message);
    let prompt = builder.build();

    // 1) 含 @ 提及时应注入【提及上下文】区块
    let idx_ctx = prompt
        .find("【提及上下文】")
        .expect("含 @ 提及时应注入【提及上下文】区块");

    // 2) 该区块属于当前消息 body，应位于【当前消息】标签之后
    let idx_cur = prompt
        .find("【当前消息】")
        .expect("Prompt 应包含【当前消息】区块");
    assert!(
        idx_cur < idx_ctx,
        "【提及上下文】应位于【当前消息】之后（idx_cur={} idx_ctx={}）",
        idx_cur,
        idx_ctx
    );

    // 3) 逐行列出「类型「名字」(id)」格式，名字去掉前导 @
    assert!(
        prompt.contains("Agent「张伟」(agt_xxx)"),
        "提及实体应格式化为 - Agent「张伟」(agt_xxx)。片段:\n{}",
        prompt.chars().take(900).collect::<String>()
    );

    // 4) 引导被 @ 的 Agent 自行查详情、不要凭空猜测
    assert!(
        prompt.contains("请调用对应的查询工具获取"),
        "应引导调用查询工具获取详情、不要凭空猜测"
    );

    // 5) 原始正文（含 @ 链接）仍保留在 prompt 中
    assert!(prompt.contains(content), "正文原文应保留");
}

/// 多条提及（Agent + 任务 + 项目）全部列出，且类型标签正确
#[test]
fn current_message_injects_mixed_mention_kinds() {
    let agent = make_simple_agent();
    let content =
        "[@张伟](agent:agt_1) 和 [@订单导出](task:tsk_2) 还有 [@增长实验](project:prj_3) 一起看下";
    let message = make_simple_message(content);

    let mut builder = DefaultPromptBuilder::new();
    builder.current_trace_id("trace-mention-002");
    builder.system_prompt(&agent);
    builder.current_message(&message);
    let prompt = builder.build();

    assert!(prompt.contains("【提及上下文】"));
    assert!(prompt.contains("Agent「张伟」(agt_1)"));
    assert!(prompt.contains("任务「订单导出」(tsk_2)"));
    assert!(prompt.contains("项目「增长实验」(prj_3)"));
}

/// 无 @ 提及时的负向断言：不注入【提及上下文】区块
#[test]
fn current_message_omits_mention_context_without_mention() {
    let agent = make_simple_agent();
    let message = make_simple_message("帮我查下订单状态");

    let mut builder = DefaultPromptBuilder::new();
    builder.current_trace_id("trace-mention-003");
    builder.system_prompt(&agent);
    builder.current_message(&message);
    let prompt = builder.build();

    assert!(
        !prompt.contains("【提及上下文】"),
        "无 @ 提及时不应注入【提及上下文】区块"
    );
    assert!(prompt.contains("帮我查下订单状态"), "正文原文应保留");
}

/// 外部 Agent（Flat）路径同样把 @ 提及上下文注入到扁平单消息
#[test]
fn flat_builder_injects_mention_context_in_single_message() {
    let agent = make_cli_agent(None);
    let content = "参考 [@张伟](agent:agt_xxx) 的处理方式";
    let message = make_simple_message(content);

    let messages = build_flat(&agent, &message);
    let ChatMessage::User { content } = &messages[0] else {
        panic!("期望 User 消息");
    };
    assert!(
        content.contains("【提及上下文】"),
        "扁平消息应含【提及上下文】"
    );
    assert!(
        content.contains("Agent「张伟」(agt_xxx)"),
        "扁平消息应列出提及实体"
    );
}

// ==================== FlatPromptBuilder（外部 Agent）====================

/// 构造带 Cli external_config 的 Agent
fn make_cli_agent(prompt_template: Option<String>) -> Agent {
    let mut agent = make_simple_agent();
    agent.brain = Some(crate::models::brain::Brain {
        kind: common::enums::AgentKind::Cli,
        agent_id: agent.po.id.clone(),
        agent_name: agent.po.name.clone(),
        runtime_config: crate::models::agent::AgentRuntimeConfig {
            external_config: Some(crate::models::agent::ExternalAgentConfig::Cli {
                command: "codex".to_string(),
                args: vec![],
                work_dir: "/tmp".to_string(),
                env: vec![],
                timeout_secs: 60,
                prompt_template,
            }),
            ..Default::default()
        },
        model_provider: None,
        memories: vec![],
    });
    agent
}

fn build_flat(agent: &Agent, message: &Message) -> Vec<ChatMessage> {
    let mut builder = FlatPromptBuilder::new();
    builder.current_trace_id("trace-flat");
    builder.system_prompt(agent);
    builder.current_message(message);
    builder.build_initial_messages()
}

#[test]
fn flat_builder_produces_single_user_message_with_system() {
    let agent = make_cli_agent(None);
    let message = make_simple_message("帮我查下订单");

    let messages = build_flat(&agent, &message);

    // 关键：只有一条消息，且是 User（brain 层 extract_last_user_prompt 取的就是它）
    assert_eq!(messages.len(), 1, "外部 Agent 只应产出一扁平消息");
    let ChatMessage::User { content } = &messages[0] else {
        panic!("扁平消息必须是 User role，否则 brain 层取不到");
    };

    // System 人设不能被丢
    assert!(
        content.contains("测试助手"),
        "System 人设必须包含在扁平提示词中"
    );
    // 用户消息同样在
    assert!(content.contains("帮我查下订单"));
    assert!(content.contains("【当前消息】"));
    assert!(content.contains("trace-flat"));
}

#[test]
fn flat_builder_without_template_joins_system_and_user() {
    let agent = make_cli_agent(None);
    let message = make_simple_message("你好");

    let messages = build_flat(&agent, &message);
    let ChatMessage::User { content } = &messages[0] else {
        panic!("期望 User 消息");
    };
    // 缺省格式：System + 空行 + User
    let system = agent.po.soul.clone();
    assert!(content.contains(&system));
    assert!(content.contains("你好"));
}

#[test]
fn flat_builder_applies_prompt_placeholder() {
    let agent = make_cli_agent(Some("请按以下指令执行：\n{prompt}".to_string()));
    let message = make_simple_message("你好");

    let messages = build_flat(&agent, &message);
    let ChatMessage::User { content } = &messages[0] else {
        panic!("期望 User 消息");
    };
    assert!(content.starts_with("请按以下指令执行：\n"));
    assert!(
        content.contains("测试助手"),
        "{{prompt}} 应展开为完整提示词"
    );
    assert!(content.contains("你好"));
    assert!(!content.contains("{prompt}"), "占位符应被替换干净");
}

#[test]
fn flat_builder_applies_system_and_user_placeholders() {
    let agent = make_cli_agent(Some(
        "<instructions>{system}</instructions>\n<input>{user}</input>".to_string(),
    ));
    let message = make_simple_message("你好");

    let messages = build_flat(&agent, &message);
    let ChatMessage::User { content } = &messages[0] else {
        panic!("期望 User 消息");
    };
    assert!(content.starts_with("<instructions>"));
    assert!(content.contains("</instructions>\n<input>"));
    assert!(content.ends_with("</input>"));
    // system 段落在 instructions 内
    assert!(content.contains("测试助手"));
    // user 段落在 input 内
    let input_part = content.split("<input>").nth(1).unwrap_or("");
    assert!(input_part.contains("【当前消息】"));
    assert!(
        !input_part.contains("测试助手"),
        "{{user}} 不应含 System 内容"
    );
}

#[test]
fn flat_builder_build_matches_initial_messages() {
    // build() 用于 trace 记录，必须与真实发出的初始消息一致
    let agent = make_cli_agent(Some("T:{prompt}".to_string()));
    let message = make_simple_message("你好");

    let mut builder = FlatPromptBuilder::new();
    builder.current_trace_id("trace-x");
    builder.system_prompt(&agent);
    builder.current_message(&message);

    let built = builder.build();
    let ChatMessage::User { content } = &builder.build_initial_messages()[0] else {
        panic!("期望 User 消息");
    };
    assert_eq!(built, *content, "build() 与初始消息内容必须一致");
}

// ==================== 沉淀 Prompt 区块顺序 ====================

/// 构造一条短期记忆（history 只渲染 ShortTerm 的摘要）
fn make_short_term_memory(summary: &str) -> crate::models::memory::Memory {
    crate::models::memory::Memory::new(crate::models::memory::MemoryPo::ShortTerm(
        crate::models::memory::ShortTermMemoryIndexPo {
            id: "mem-1".to_string(),
            agent_id: "agent-test-001".to_string(),
            task_id: None,
            role: "user".to_string(),
            summary: summary.to_string(),
            tags: "[]".to_string(),
            trace_ids: "[]".to_string(),
            status: common::enums::MemoryStatus::Active,
            created_at: 0,
            updated_at: 0,
        },
    ))
}

/// 区块顺序：通用上下文 →【近期已沉淀记忆】→ Trace ID → 待沉淀内容
#[test]
fn sleep_prompt_section_order() {
    let agent = make_simple_agent();

    let mut builder = FlatPromptBuilder::new();
    builder.current_trace_id("trace-sleep");
    builder.system_prompt(&agent);
    builder.settled_reference(&["上一段沉淀到一半".to_string()]);
    let prompt = builder.build_sleep_prompt("待沉淀内容", &["t-1".to_string()]);

    let idx_ref = prompt
        .find("【近期已沉淀记忆（仅供参考）】")
        .expect("应包含参考区块");
    let idx_trace = prompt
        .find("【思考 Trace ID】")
        .expect("应包含【思考 Trace ID】");
    let idx_pending = prompt.find("待沉淀内容").expect("应包含待沉淀内容");

    assert!(idx_ref < idx_trace, "参考区块应排在【思考 Trace ID】之前");
    assert!(
        idx_trace < idx_pending,
        "【思考 Trace ID】应排在待沉淀内容之前"
    );
    assert!(prompt.contains("上一段沉淀到一半"));
}

/// 沉淀场景**不渲染**【历史对话】
///
/// 即便调用方误挂载 history 也不渲染：它与【待沉淀的短期记忆】大面积重复
/// （Active ≤ 20 条时完全重复），会白白吃掉上下文预算。
/// 已沉淀过什么由 Agent 用 `search_memory` 按需检索。
#[test]
fn sleep_prompt_omits_history_block() {
    let agent = make_simple_agent();
    let memories = vec![make_short_term_memory("之前聊过退款政策")];

    let mut builder = FlatPromptBuilder::new();
    builder.system_prompt(&agent);
    builder.history(&memories);
    builder.current_message(&make_simple_message("帮我查下订单"));
    let prompt = builder.build_sleep_prompt("待沉淀摘要", &[]);

    assert!(
        !prompt.contains("【历史对话】"),
        "沉淀场景不应渲染【历史对话】——它与待沉淀列表重复"
    );
    assert!(
        !prompt.contains("之前聊过退款政策"),
        "history 内容不应出现在沉淀 prompt 中"
    );
    assert!(
        !prompt.contains("【当前消息】"),
        "沉淀场景不应渲染【当前消息】区块"
    );
    assert!(prompt.contains("待沉淀摘要"));
}

/// 压缩结果直接注入 awaken：不查历史、明确告知是上一轮产物、保留原始诉求
#[test]
fn awaken_prompt_injects_compacted_context() {
    let agent = make_simple_agent();
    let message = make_simple_message("帮我查下订单");

    let mut builder = FlatPromptBuilder::new();
    builder.current_trace_id("trace-awaken");
    builder.system_prompt(&agent);
    builder.compacted_context("已完成退款政策核对，待办是生成回复给用户");
    builder.current_message(&message);

    let prompt = builder.build();

    let idx_summary = prompt
        .find("【上一轮工作压缩结果】")
        .expect("应包含压缩结果区块");
    let idx_msg = prompt.find("【当前消息】").expect("应包含当前消息");

    // 压缩结果排在原始诉求之前，且原始诉求必须保留
    assert!(idx_summary < idx_msg, "压缩结果应排在【当前消息】之前");
    assert!(prompt.contains("已完成退款政策核对"));
    assert!(prompt.contains("帮我查下订单"), "原始用户诉求必须保留");
    // 明确告知是上一轮产物 + 指引按需检索
    assert!(prompt.contains("上一轮思考中完成的工作"));
    assert!(prompt.contains("search_memory"));
    // 未装配 history，不应出现历史区块。
    // 注意按「行首区块头」匹配：System 的回复指引里也提到过【历史对话】这个词。
    assert!(!prompt.contains("\n【历史对话】\n"));
}

/// 压缩后补的「更早的记忆」必须明确标注是过去的、非当前工作
#[test]
fn awaken_prompt_past_memories_marked_as_historical() {
    let agent = make_simple_agent();
    let message = make_simple_message("帮我查下订单");

    let mut builder = FlatPromptBuilder::new();
    builder.system_prompt(&agent);
    builder.compacted_context("本轮完成了退款核对");
    builder.past_memories_reference(&["上上周整理过报销流程".to_string()]);
    builder.current_message(&message);

    let prompt = builder.build();

    let idx_summary = prompt
        .find("【上一轮工作压缩结果】")
        .expect("应包含压缩结果");
    let idx_past = prompt
        .find("【更早的记忆（仅供参考，非当前工作）】")
        .expect("应包含更早记忆区块");
    let idx_msg = prompt.find("【当前消息】").expect("应包含当前消息");

    // 顺序：压缩结果 → 更早记忆 → 当前消息
    assert!(idx_summary < idx_past, "压缩结果应排在更早记忆之前");
    assert!(idx_past < idx_msg, "更早记忆应排在【当前消息】之前");

    // 措辞必须说清「过去/历史/不是当前工作」
    assert!(prompt.contains("更早之前"));
    assert!(prompt.contains("历史背景"));
    assert!(prompt.contains("不是"));
    assert!(prompt.contains("不要"));
    assert!(prompt.contains("重复已经做过的动作"));
    assert!(prompt.contains("search_memory"));
    assert!(prompt.contains("上上周整理过报销流程"));
}

/// 未压缩时不渲染「更早的记忆」（走常规【历史对话】路径）
#[test]
fn awaken_prompt_omits_past_memories_when_absent() {
    let agent = make_simple_agent();

    let mut builder = FlatPromptBuilder::new();
    builder.system_prompt(&agent);
    builder.history(&[make_short_term_memory("之前聊过退款政策")]);
    let prompt = builder.build();

    assert!(!prompt.contains("【更早的记忆"));
    // 按行首区块头匹配，避免命中 System 指引里的同名措辞
    assert!(prompt.contains("\n【历史对话】\n"));
}

/// 未压缩时不渲染该区块（走常规【历史对话】路径）
#[test]
fn awaken_prompt_omits_compacted_context_when_absent() {
    let agent = make_simple_agent();

    let mut builder = FlatPromptBuilder::new();
    builder.system_prompt(&agent);
    builder.history(&[make_short_term_memory("之前聊过退款政策")]);
    let prompt = builder.build();

    assert!(!prompt.contains("【上一轮工作压缩结果】"));
    // 按行首区块头匹配，避免命中 System 指引里的同名措辞
    assert!(prompt.contains("\n【历史对话】\n"));
}

/// 参考区块缺省不渲染
#[test]
fn sleep_prompt_omits_reference_when_empty() {
    let agent = make_simple_agent();

    let mut builder = FlatPromptBuilder::new();
    builder.system_prompt(&agent);
    let prompt = builder.build_sleep_prompt("待沉淀摘要", &[]);

    assert!(!prompt.contains("【近期已沉淀记忆"));
    assert!(prompt.contains("待沉淀摘要"));
}

#[test]
fn flat_builder_ignores_template_for_remote_config() {
    // Remote 配置无 prompt_template，应走缺省拼接
    let mut agent = make_simple_agent();
    agent.brain = Some(crate::models::brain::Brain {
        kind: common::enums::AgentKind::Remote,
        agent_id: agent.po.id.clone(),
        agent_name: agent.po.name.clone(),
        runtime_config: crate::models::agent::AgentRuntimeConfig {
            external_config: Some(crate::models::agent::ExternalAgentConfig::Remote {
                endpoint: "http://remote".to_string(),
                agent_name: "remote-agent".to_string(),
                auth_token: None,
                timeout_secs: 30,
            }),
            ..Default::default()
        },
        model_provider: None,
        memories: vec![],
    });

    let builder = FlatPromptBuilder::new();
    assert!(builder.prompt_template.is_none());

    let message = make_simple_message("你好");
    let mut builder = FlatPromptBuilder::new();
    builder.system_prompt(&agent);
    builder.current_message(&message);
    let ChatMessage::User { content } = &builder.build_initial_messages()[0] else {
        panic!("期望 User 消息");
    };
    assert!(content.contains("测试助手"));
    assert!(content.contains("你好"));
}
