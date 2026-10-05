//! tests 单元测试（拆分自 awakening.rs）
//!
//! 文件瘦身：原 2227 行 → 1500 行，测试体 727 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

// ==================== awaken 集成测试 ====================

use super::ThinkingOptions;
use crate::models::agent::{Agent, AgentPo};
use crate::models::brain::Brain;
use crate::models::file::FileMeta;
use crate::models::message::Message;
use crate::models::model_provider::ModelProvider;
use crate::models::skill::SkillPo;
use crate::pkg::RequestContext;
use crate::pkg::tool_tracing::logger::ToolCallLogger;
use crate::service::dal::brain::BrainDal;
use crate::service::domain::runtime::tool_execution_test::credential_stubs::{
    StubLarkCredentialDal, StubUserDal,
};
use async_trait::async_trait;
use common::enums::skill::SkillAuthorType;
use common::enums::{AgentStatus, MessageRole, MessageType, ModelCapability, ProviderType};
use sqlx::SqlitePool;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use uuid::Uuid;

use super::truncate_reason;

#[test]
fn truncate_reason_respects_limit_and_trims() {
    // 长文本：超过上限应被截断并追加 "..."
    let long = "x".repeat(500);
    let out = truncate_reason(&long, 200);
    assert!(out.ends_with("..."), "应追加省略号: {out}");
    assert_eq!(out.chars().count(), 203, "200 字符 + 3 点");

    // 短文本：不超过上限原样返回（含两端空白被 trim）
    let short = "  hello world  ";
    assert_eq!(truncate_reason(short, 200), "hello world");

    // 边界：恰好等于上限不截断
    let exact = "a".repeat(200);
    assert_eq!(truncate_reason(&exact, 200), exact);

    // Unicode：按字符而非字节截断，避免截断到多字节中间
    let uni = "中".repeat(300);
    let out_uni = truncate_reason(&uni, 200);
    assert_eq!(out_uni.chars().count(), 203);
    assert!(out_uni.ends_with("..."));
}

/// 捕获 Prompt 的 BrainDal Stub
///
/// 在 think() 调用时捕获传入的 prompt，返回固定响应
struct CapturingBrainDal {
    captured_prompt: Arc<Mutex<Option<String>>>,
}

impl CapturingBrainDal {
    fn new(captured_prompt: Arc<Mutex<Option<String>>>) -> Self {
        Self { captured_prompt }
    }
}

#[async_trait]
impl BrainDal for CapturingBrainDal {
    async fn wake_brain(
        &self,
        _ctx: RequestContext,
        _agent: &AgentPo,
        _memories: Vec<crate::models::memory::Memory>,
    ) -> common::error::Result<Brain> {
        unimplemented!("not needed by awaken skill tests")
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProvider,
        _prompt: &str,
    ) -> common::error::Result<String> {
        unimplemented!("not needed by awaken skill tests")
    }

    async fn think(
        &self,
        _ctx: RequestContext,
        _brain: &Brain,
        messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[crate::models::cortex_types::ToolDescriptor],
    ) -> common::error::Result<crate::models::cortex_types::ThinkResult> {
        // 把所有初始消息的 content 按顺序拼接（等价旧版扁平 build() 输出），
        // 保证原有断言（soul 是否注入 / skills 是否出现 / user_profile 是否携带）
        // 在 System/User 角色拆分后仍能通过。
        use crate::models::cortex_types::ChatMessage;
        let prompt: String = messages
            .iter()
            .filter_map(|m| match m {
                ChatMessage::System { content } => Some(content.as_str()),
                ChatMessage::User { content } => Some(content.as_str()),
                ChatMessage::Assistant { content, .. } => content.as_deref(),
                ChatMessage::Tool { content, .. } => Some(content.as_str()),
                // 批4 P1 新变体穷举波及臂（测试 mock，编译器强制；行为=取 text part）
                ChatMessage::UserMultimodal { text, .. } => Some(text.as_str()),
            })
            .collect::<Vec<_>>()
            .join("\n");
        *self.captured_prompt.lock().unwrap() = Some(prompt);
        Ok(crate::models::cortex_types::ThinkResult::Final {
            content: "mock response".to_string(),
            usage: crate::models::cortex_types::TokenUsage::default(),
        })
    }

    async fn think_fast(
        &self,
        _ctx: RequestContext,
        _provider: &crate::models::model_provider::ModelProviderPo,
        _state: serde_json::Value,
        _questions: std::collections::BTreeMap<
            String,
            crate::models::cerebellum_types::CerebellumQuestion,
        >,
    ) -> common::error::Result<crate::models::cerebellum_types::ThinkFastResult> {
        unimplemented!("not needed by awaken skill tests")
    }

    async fn embed_entity(
        &self,
        _ctx: RequestContext,
        _entity: &dyn crate::models::vector::Vectorizable,
    ) -> common::error::Result<Option<crate::models::vector::VectorIndexParams>> {
        Ok(None)
    }

    async fn embed_text_for_search(
        &self,
        _ctx: RequestContext,
        _text: &str,
    ) -> common::error::Result<Option<crate::models::vector::VectorIndexParams>> {
        Ok(None)
    }
}

/// 在 `think()` 中直接返回 429（ModelRateLimited）的 BrainDal Stub
///
/// 用于验证异常退出兜底链路：429 应向上冒泡为 `Err`，
/// 且 `awaken` 必须在返回前落库一条 `loop_abort` 短期记忆、并在错误上挂 `abort_notice`。
struct FailingBrainDal;

#[async_trait]
impl BrainDal for FailingBrainDal {
    async fn wake_brain(
        &self,
        _ctx: RequestContext,
        _agent: &AgentPo,
        _memories: Vec<crate::models::memory::Memory>,
    ) -> common::error::Result<Brain> {
        unimplemented!("not needed by awaken abort test")
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProvider,
        _prompt: &str,
    ) -> common::error::Result<String> {
        unimplemented!("not needed by awaken abort test")
    }

    async fn think(
        &self,
        _ctx: RequestContext,
        _brain: &Brain,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[crate::models::cortex_types::ToolDescriptor],
    ) -> common::error::Result<crate::models::cortex_types::ThinkResult> {
        Err(common::error::Error::new(
            common::error::ErrorCode::ModelRateLimited,
            "chat completions rate limited (429): quota exceeded for this minute",
        ))
    }

    async fn think_fast(
        &self,
        _ctx: RequestContext,
        _provider: &crate::models::model_provider::ModelProviderPo,
        _state: serde_json::Value,
        _questions: std::collections::BTreeMap<
            String,
            crate::models::cerebellum_types::CerebellumQuestion,
        >,
    ) -> common::error::Result<crate::models::cerebellum_types::ThinkFastResult> {
        unimplemented!("not needed by awaken abort test")
    }

    async fn embed_entity(
        &self,
        _ctx: RequestContext,
        _entity: &dyn crate::models::vector::Vectorizable,
    ) -> common::error::Result<Option<crate::models::vector::VectorIndexParams>> {
        Ok(None)
    }

    async fn embed_text_for_search(
        &self,
        _ctx: RequestContext,
        _text: &str,
    ) -> common::error::Result<Option<crate::models::vector::VectorIndexParams>> {
        Ok(None)
    }
}

/// 初始化测试环境：所有 DAO + DAL 单例
fn init_awaken_test_env(pool: SqlitePool) -> RequestContext {
    // 必须先初始化 config（文件操作需要 base_data_path）
    let _ = crate::config::init();

    // 初始化所有 DAO
    crate::service::dao::agent::init();
    crate::service::dao::tool::init();
    crate::service::dao::skill::init();
    crate::service::dao::tool_call::init();
    crate::service::dao::model_provider::init();
    crate::service::dao::cortex::init();
    crate::service::dao::memory::init();
    crate::service::dao::mcp_server::init();

    // 初始化所有 DAL
    crate::service::dal::agent::init();
    crate::service::dal::tool::init();
    crate::service::dal::skill::init();
    crate::service::dal::model_provider::init();
    crate::service::dal::memory::init();
    crate::service::dal::mcp_tool::init();
    crate::service::dal::brain::init();

    crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
}

/// 创建带 Brain 的测试 Agent
fn make_test_agent(agent_id: &str) -> Agent {
    let mut po = AgentPo::new(
        "Test Agent".to_string(),
        vec!["assistant".to_string()],
        "Test description".to_string(),
        vec!["chat".to_string()],
        "Test soul".to_string(),
        "provider-001".to_string(),
        "test-user".to_string(),
    );
    po.id = agent_id.to_string();
    po.status = AgentStatus::Onboarded;

    let mut agent = Agent::from_po(po);
    let model_provider_po = crate::models::model_provider::ModelProviderPo {
        id: "mock-provider".to_string(),
        name: "Mock Provider".to_string(),
        provider_type: ProviderType::OpenAI,
        model_name: "gpt-4".to_string(),
        capability: ModelCapability::Agent,
        api_key: "fake-key".to_string(),
        base_url: None,
        description: None,
        config: "{}".to_string(),
        status: common::enums::ModelProviderStatus::Normal,
        created_by: "test-user".to_string(),
        modified_by: "test-user".to_string(),
        created_at: 0,
        updated_at: 0,
    };
    let runtime_config = crate::models::agent::AgentRuntimeConfig::default();
    agent.brain = Some(Brain::new_local(
        agent_id.to_string(),
        "Test Agent".to_string(),
        runtime_config,
        model_provider_po,
        vec![],
    ));
    agent
}

/// 创建测试文本消息
fn make_test_message(content: &str) -> Message {
    Message::new_with_context(
        Uuid::now_v7().to_string(),
        None,
        None,
        "test-user".to_string(),
        "test-agent".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        content.to_string(),
        None,
        FileMeta::default(),
        None,
        None,
        None,
        "test-user".to_string(),
    )
}

/// 在数据库中为 Agent 创建技能副本
///
/// skills tags 包含 "assistant" 以匹配 Agent 的 role，确保出现在"必加载技能"区块
async fn create_skill_for_agent(
    ctx: RequestContext,
    agent_id: &str,
    name: &str,
    description: &str,
) {
    let skill_po = SkillPo::new(
        format!("skill-{}--{}", name.to_lowercase(), Uuid::new_v4()),
        name.to_string(),
        description.to_string(),
        vec!["assistant".to_string()],
        "test".to_string(),
        String::new(),
        agent_id.to_string(),
        SkillAuthorType::Agent,
        format!("skills/{}", name.to_lowercase()),
    );
    crate::service::dal::skill::dal()
        .create(ctx, &skill_po)
        .await
        .expect("创建测试技能失败");
}

/// 模拟 hr_domain.get_agent(with_skills=true) 的技能加载
///
/// 生产路径由 hr_domain 加载 Skill 业务实体写入 agent.skills，测试中直接查 DB 填充
async fn load_skills_to_agent(ctx: RequestContext, agent: &mut Agent) {
    use common::enums::SkillStatus;
    let skills = crate::service::dal::skill::dal()
        .query(
            ctx,
            crate::service::dao::skill::SkillQuery {
                author_id: Some(agent.po.id.clone()),
                exclude_status: Some(SkillStatus::Expired),
                ..Default::default()
            },
        )
        .await
        .expect("加载技能失败");
    agent.set_skills(skills.items);
}

#[sqlx::test]
async fn test_awaken_with_skills(pool: SqlitePool) {
    let ctx = init_awaken_test_env(pool);

    let agent_id = format!("agent-with-skills-{}", Uuid::now_v7());
    let mut agent = make_test_agent(&agent_id);

    // 为 Agent 创建 2 个技能副本
    create_skill_for_agent(
        ctx.clone(),
        &agent_id,
        "CodeReview",
        "审查代码质量并给出改进建议",
    )
    .await;
    create_skill_for_agent(ctx.clone(), &agent_id, "DocWriting", "编写清晰的技术文档").await;

    // 模拟 hr_domain.get_agent(with_skills=true) 加载技能到 agent.skills
    load_skills_to_agent(ctx.clone(), &mut agent).await;

    let message = make_test_message("请帮我审查这段代码");

    let captured_prompt = Arc::new(Mutex::new(None));
    let temp_dir = tempdir().expect("tempdir should be created");
    let runtime = crate::service::domain::runtime::new_with_all(
        Arc::new(CapturingBrainDal::new(captured_prompt.clone())),
        crate::service::dal::tool::dal(),
        crate::service::dal::mcp_tool::dal(),
        crate::service::dal::agent::dal(),
        Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
        Arc::new(StubUserDal::none()),
        Arc::new(StubLarkCredentialDal::none()),
    );

    let result = runtime
        .awakening()
        .awaken(ctx.clone(), &agent, &message, &ThinkingOptions::new())
        .await
        .expect("awaken 应该成功");

    let prompt = captured_prompt
        .lock()
        .unwrap()
        .clone()
        .expect("应该捕获到 prompt");

    // 验证 Prompt 包含"【必加载技能】"部分（tags 匹配 agent role "assistant"）
    assert!(
        prompt.contains("【必加载技能】"),
        "Prompt 应该包含【必加载技能】部分，实际: {}",
        prompt
    );
    // 验证两个技能都出现在 Prompt 中
    assert!(
        prompt.contains("CodeReview"),
        "Prompt 应该包含技能 CodeReview"
    );
    assert!(
        prompt.contains("审查代码质量并给出改进建议"),
        "Prompt 应该包含 CodeReview 的描述"
    );
    assert!(
        prompt.contains("DocWriting"),
        "Prompt 应该包含技能 DocWriting"
    );
    assert!(
        prompt.contains("编写清晰的技术文档"),
        "Prompt 应该包含 DocWriting 的描述"
    );

    // 验证返回结果
    assert_eq!(result.agent_id, agent_id);
    assert!(!result.raw_input.is_empty());
    assert_eq!(result.raw_output, "mock response");
}

#[sqlx::test]
async fn test_awaken_without_skills(pool: SqlitePool) {
    let ctx = init_awaken_test_env(pool);

    let agent_id = format!("agent-no-skills-{}", Uuid::now_v7());
    let agent = make_test_agent(&agent_id);

    // 不为 Agent 创建任何技能
    let message = make_test_message("你好");

    let captured_prompt = Arc::new(Mutex::new(None));
    let temp_dir = tempdir().expect("tempdir should be created");
    let runtime = crate::service::domain::runtime::new_with_all(
        Arc::new(CapturingBrainDal::new(captured_prompt.clone())),
        crate::service::dal::tool::dal(),
        crate::service::dal::mcp_tool::dal(),
        crate::service::dal::agent::dal(),
        Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
        Arc::new(StubUserDal::none()),
        Arc::new(StubLarkCredentialDal::none()),
    );

    let result = runtime
        .awakening()
        .awaken(ctx.clone(), &agent, &message, &ThinkingOptions::new())
        .await
        .expect("awaken 应该成功");

    let prompt = captured_prompt
        .lock()
        .unwrap()
        .clone()
        .expect("应该捕获到 prompt");

    // 验证 Prompt 不包含技能相关区块（Agent 无技能）
    assert!(
        !prompt.contains("【必加载技能】") && !prompt.contains("【神经技能】"),
        "Prompt 不应该包含技能区块（Agent 无技能），实际: {}",
        prompt
    );

    // 验证返回结果仍然正常
    assert_eq!(result.agent_id, agent_id);
    assert!(!result.raw_input.is_empty());
    assert_eq!(result.raw_output, "mock response");
}

#[sqlx::test]
async fn test_awaken_rate_limited_persists_abort_memory(pool: SqlitePool) {
    let ctx = init_awaken_test_env(pool);

    let agent_id = format!("agent-abort-{}", Uuid::now_v7());
    let agent = make_test_agent(&agent_id);
    let message = make_test_message("帮我把上周的方案整理成一页纸摘要");

    // 让 Brain 在第一次 think() 即返回 429，模拟限流导致思考循环异常退出
    let temp_dir = tempdir().expect("tempdir should be created");
    let runtime = crate::service::domain::runtime::new_with_all(
        Arc::new(FailingBrainDal),
        crate::service::dal::tool::dal(),
        crate::service::dal::mcp_tool::dal(),
        crate::service::dal::agent::dal(),
        Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
        Arc::new(StubUserDal::none()),
        Arc::new(StubLarkCredentialDal::none()),
    );

    let err = runtime
        .awakening()
        .awaken(ctx.clone(), &agent, &message, &ThinkingOptions::new())
        .await
        .expect_err("429 应当作为 Err 向上传播");

    // 1. 错误语义不变：code 仍是 ModelRateLimited
    assert_eq!(err.code_enum(), common::error::ErrorCode::ModelRateLimited);

    // 2. 错误上挂了给用户的一句话进度概览（abort_notice）
    let notice = err
        .field
        .as_ref()
        .and_then(|f| {
            f.extra
                .get(crate::service::domain::runtime::abort_summary::ABORT_NOTICE_FIELD)
        })
        .expect("错误应携带 abort_notice 字段");
    let notice = notice.as_str().expect("abort_notice 应为字符串");
    assert!(!notice.is_empty(), "abort_notice 不应为空");

    // 3. 短期记忆里应落库一条 loop_abort 存档
    let mems = crate::service::dal::memory::dal()
        .query(
            ctx.clone(),
            crate::service::dao::memory::MemoryQuery {
                agent_id: Some(agent_id.clone()),
                tags: Some(vec!["loop_abort".to_string()]),
                ..Default::default()
            },
        )
        .await
        .expect("查询短期记忆失败");
    assert!(!mems.is_empty(), "异常退出后应落库 loop_abort 短期记忆");

    // 存档内容应当包含用户原始消息与真实错误码，证明「信息不丢」
    let summary = mems[0].to_prompt_summary().expect("短期记忆应有 summary");
    assert!(
        summary.contains("帮我把上周的方案整理成一页纸摘要"),
        "存档应保留用户原始消息，实际: {summary}"
    );
    assert!(
        summary.contains("model_rate_limited"),
        "存档应记录真实错误码，实际: {summary}"
    );
}

#[sqlx::test]
async fn test_awaken_with_user_profile(pool: SqlitePool) {
    let ctx = init_awaken_test_env(pool);

    let agent_id = format!("agent-user-profile-{}", Uuid::now_v7());
    let agent = make_test_agent(&agent_id);
    let message = make_test_message("你好");

    // 构造带自述偏好的用户画像
    let mut user_po = crate::models::user::UserPo::new(
        "test-user".to_string(),
        "org-1".to_string(),
        "tester".to_string(),
        "测试用户".to_string(),
        "tester@example.com".to_string(),
        "hash".to_string(),
        common::enums::UserRole::Member,
        "system".to_string(),
    );
    user_po.preferences = "- 回复请用中文".to_string();

    let captured_prompt = Arc::new(Mutex::new(None));
    let temp_dir = tempdir().expect("tempdir should be created");
    let runtime = crate::service::domain::runtime::new_with_all(
        Arc::new(CapturingBrainDal::new(captured_prompt.clone())),
        crate::service::dal::tool::dal(),
        crate::service::dal::mcp_tool::dal(),
        crate::service::dal::agent::dal(),
        Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
        Arc::new(StubUserDal::none()),
        Arc::new(StubLarkCredentialDal::none()),
    );

    runtime
        .awakening()
        .awaken(
            ctx.clone(),
            &agent,
            &message,
            &ThinkingOptions::new().with_user_profile(user_po),
        )
        .await
        .expect("awaken 应该成功");

    let prompt = captured_prompt
        .lock()
        .unwrap()
        .clone()
        .expect("应该捕获到 prompt");

    // 【用户画像】区块含基础信息 + 自述偏好
    assert!(
        prompt.contains("【用户画像】"),
        "Prompt 应该包含【用户画像】区块，实际: {}",
        prompt
    );
    assert!(
        prompt.contains("【用户偏好】- 回复请用中文"),
        "【用户画像】应包含用户自述偏好，实际: {}",
        prompt
    );
}

#[test]
fn thinking_scene_tool_whitelist() {
    use common::enums::ThinkingScene;

    let scene = ThinkingScene::IntentAnalyze;

    // 允许：工具名 tag 包含 vector_search / query_memory / search / analyze
    let allowed_tags: Vec<String> = vec![
        "vector_search".into(),
        "query_memory".into(),
        "search_memory".into(),
        "analyze_text".into(),
    ];
    for tag in allowed_tags {
        assert!(
            scene.is_tool_allowed(&[tag]),
            "tag should be allowed in IntentAnalyze scene"
        );
    }

    // 禁止：工具名 tag 包含 shell_exec / lark_push
    let forbidden_tags: Vec<String> = vec![
        "shell_exec".into(),
        "lark_push".into(),
        "send_message".into(),
    ];
    for tag in forbidden_tags {
        assert!(
            !scene.is_tool_allowed(&[tag]),
            "tag should be forbidden in IntentAnalyze scene"
        );
    }
}

#[test]
fn intent_analysis_json_roundtrip() {
    use super::IntentAnalysis;
    use serde_json;

    let ia = IntentAnalysis {
        intent_type: "TaskRequest".into(),
        confidence: 0.85,
        key_terms: vec!["项目X".into(), "方案A".into(), "进度".into()],
        resolutions: vec!["\"上次那个方案\" → project=123, task=456".into()],
        retrieved_context: vec!["2026-08-10 方案 A/B 比较结论，推荐方案 A（相似度 0.88）".into()],
        need_clarification: vec![],
        summary: "用户想知道项目 X 方案 A 的当前推进进度".into(),
    };

    let json_str = serde_json::to_string(&ia).expect("serialize IntentAnalysis");
    let ia2: IntentAnalysis = serde_json::from_str(&json_str).expect("deserialize IntentAnalysis");

    assert_eq!(ia.intent_type, ia2.intent_type);
    assert!((ia.confidence - ia2.confidence).abs() < 0.0001);
    assert_eq!(ia.key_terms, ia2.key_terms);
    assert_eq!(ia.resolutions, ia2.resolutions);
    assert_eq!(ia.retrieved_context, ia2.retrieved_context);
    assert_eq!(ia.need_clarification, ia2.need_clarification);
    assert_eq!(ia.summary, ia2.summary);
}

#[test]
fn parse_intent_analysis_json_level4_fallback() {
    use super::{IntentAnalysis, parse_intent_analysis_json};

    // 模拟 LLM 输出：大量中文思考 + JSON 代码块包裹（Level 2 代码块降级）
    let input = r#"我先分析一下用户的意图...好的，现在整理成结构化结果：
用户明显是在追问之前的内容，我归类为 FollowUp 型。
以下是 JSON 输出：
```json
{
"intent_type": "FollowUp",
"confidence": 0.82,
"key_terms": ["项目X", "方案A", "进度", "上次那个方案"],
"resolutions": ["\"上次那个方案\" → project=proj_123, task=task_456"],
"retrieved_context": ["通过 search_memory 查到 2026-08-10 记忆：推荐方案 A"],
"need_clarification": [],
"summary": "用户想知道项目 X 中方案 A 的推进情况"
}
```
好的，以上就是我的分析结论。"#;

    let ia: IntentAnalysis = parse_intent_analysis_json(input)
        .expect("level4 fallback should parse successfully via code block extraction");

    assert_eq!(ia.intent_type, "FollowUp");
    assert!((ia.confidence - 0.82).abs() < 0.0001);
    assert_eq!(ia.key_terms.len(), 4);
    assert_eq!(ia.key_terms[0], "项目X");
    assert_eq!(ia.resolutions.len(), 1);
    assert!(ia.need_clarification.is_empty());
    assert_eq!(ia.summary, "用户想知道项目 X 中方案 A 的推进情况");
}

#[test]
fn parse_intent_analysis_json_balanced_braces() {
    use super::{IntentAnalysis, extract_first_json_object, parse_intent_analysis_json};

    // 0. 测试 extract_first_json_object 基础能力：多个 JSON 时提取第一个
    let multi = r#"prefix {"a":1} middle {"b":2} suffix"#;
    assert_eq!(extract_first_json_object(multi), Some(r#"{"a":1}"#));

    // 1. 测试字符串内部的大括号不会干扰括号计数
    let with_inner_braces = r#"文本开头 {"key":"val{ue}"} 文本结尾"#;
    assert_eq!(
        extract_first_json_object(with_inner_braces),
        Some(r#"{"key":"val{ue}"}"#)
    );

    // 2. 平衡括号降级测试：有效 JSON 埋在中文散文里，无代码块
    let input = r#"经过 Step1 到 Step5 的仔细思考，我得出以下理解结论。
首先对用户意图进行归类，认为属于 Question 类型（问答型），置信度较高。
指代消解部分：没有明显的歧义短语，上下文清晰。
关键词抽取完毕。语义检索已完成，有如下结果摘要。
最终 JSON 结果如下：{"intent_type":"Question","confidence":0.9,"key_terms":["排期","项目X"],"resolutions":[],"retrieved_context":["查到项目X的排期计划：周五截止"],"need_clarification":["排期是指哪个版本的？（A：V1.2；B：V1.3）"],"summary":"用户询问项目X的排期，需要澄清版本信息"}如果还需要补充信息请及时告诉我。"#;

    let ia: IntentAnalysis = parse_intent_analysis_json(input)
        .expect("balanced braces fallback should parse successfully");

    assert_eq!(ia.intent_type, "Question");
    assert!((ia.confidence - 0.9).abs() < 0.0001);
    assert_eq!(ia.key_terms, vec!["排期", "项目X"]);
    assert_eq!(ia.need_clarification.len(), 1);
    assert!(ia.need_clarification[0].contains("排期是指哪个版本"));
    assert_eq!(ia.summary, "用户询问项目X的排期，需要澄清版本信息");
}

mod vision_resource_tests {
    // 本 mod 嵌在测试文件内，super 是「测试文件顶层」而非 awakening 模块，
    // 因此要显式从 awakening 导入这两个私有 helper
    use super::super::{collect_resource_ref_ids, image_part_admission};

    // ============================================================
    // 资源引用收集 / 图像准入（vision 携带机制的第二组用例）
    // ============================================================

    #[test]
    fn collect_refs_buckets_attachment_and_artifact() {
        let content = "看 [图](attachment:att_1) 和 [报告](artifact:art_2)，再 [图2](attachment:att_1) 与 [提及](user:u9)";
        let (atts, arts) = collect_resource_ref_ids(content);
        assert_eq!(atts, vec!["att_1".to_string()]);
        assert_eq!(arts, vec!["art_2".to_string()]);
    }

    #[test]
    fn collect_refs_empty_for_plain_text() {
        let (atts, arts) = collect_resource_ref_ids("纯文本无引用");
        assert!(atts.is_empty());
        assert!(arts.is_empty());
    }

    #[test]
    fn image_admission_accepts_within_limits() {
        assert!(image_part_admission("image/png", 1024, 0).is_ok());
        assert!(image_part_admission("image/jpeg", 10 * 1024 * 1024, 3).is_ok());
    }

    #[test]
    fn image_admission_degrades_beyond_limits() {
        // 非 image/* mime / 超单图 10MB / 超单条 4 张 —— 均降级占位不中断
        assert!(image_part_admission("application/pdf", 100, 0).is_err());
        assert!(image_part_admission("image/png", 10 * 1024 * 1024 + 1, 0).is_err());
        assert!(image_part_admission("image/png", 100, 4).is_err());
    }
}
