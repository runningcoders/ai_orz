//! Runtime Awakening 具体实现

use common::error::{err, Result};
use crate::models::agent::Agent;
use crate::models::memory::MemoryTrace;
use crate::models::message::Message;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::pkg::request_context::RequestContext;
use crate::pkg::stats::AgentAwakeEvent;
use crate::service::domain::runtime::{
    AwakeningResult, RuntimeAwakening, RuntimeDomain, RuntimeDomainImpl,
};

use crate::enrich_ctx;
use crate::record_event;

#[async_trait::async_trait]
impl RuntimeAwakening for RuntimeDomainImpl {
    /// 装配 Agent 的 Brain
    ///
    /// 根据 agent.kind 构造对应的 Brain：
    /// - Local: 从 agent.tools 分离出 Auto 工具注入 Rig（Manual 工具保留供 Prompt 使用），
    ///   通过 BrainDal.wake_brain 构造带 Cortex 的 Brain
    /// - External（Cli/Remote）: 构造不带 Cortex 的虚拟 Brain
    ///
    /// 工具由 hr_domain.get_agent(with_tools=true) 预先加载到 agent.tools。
    /// 幂等：如果 agent.brain 已存在则直接返回。
    async fn wake_agent_brain(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
    ) -> Result<()> {
        // 幂等：brain 已装配则跳过
        if agent.brain.is_some() {
            return Ok(());
        }

        let ctx = enrich_ctx!(&ctx, &*agent);

        // 工具已在 hr_domain.get_agent(with_tools=true) 时加载到 agent.tools
        // 从 agent.tools 中分离出 Auto 工具用于 Rig 注册，Manual 工具保留供 Prompt 使用
        // Tool 实体不可 Clone（含 dyn Trait），使用 partition 转移所有权
        let rig_tools = if agent.po.kind.is_local() {
            let all_tools = std::mem::take(&mut agent.tools);
            let (auto, manual): (Vec<_>, Vec<_>) = all_tools
                .into_iter()
                .partition(|t| matches!(t.po.control_mode, common::enums::ControlMode::Auto));
            agent.tools = manual;
            auto
        } else {
            Vec::new()
        };

        // 通过 BrainDal 构造 Brain（内部按 kind 分发）
        // memories 传空：awaken 时会独立加载 recent_memories 用于 prompt
        let brain = self.brain_dal()
            .wake_brain(ctx, &agent.po, Vec::new(), rig_tools)
            .await?;

        agent.set_brain(brain);
        Ok(())
    }

    async fn awaken(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        message: &Message,
    ) -> Result<AwakeningResult> {
        let start_time = std::time::SystemTime::now();

        // 设置 Agent 为忙碌状态
        AgentRuntimeStateManager::global()
            .set_busy(&agent.po.id, &message.po.id);

        // 补充 Agent 上下文到 ctx，后续调用链可复用
        let ctx = enrich_ctx!(&ctx, agent);

        // Step 1: 读取最近短期记忆
        let recent_memories = self
            .memory()
            .get_recent_context(ctx.clone(), &agent.po.id, None, 20)
            .await?;

        // Step 2: 预先构造 MemoryTrace 拿到 trace_id
        // 调用方负责组装 trace，RuntimeMemory 负责写入和补全信息
        use common::enums::MemoryRole;
        let mut trace = MemoryTrace::new(
            agent.po.id.clone(),
            ctx.log_id.clone(),
            ctx.uid(),
            ctx.organization_id.clone().unwrap_or_default(),
            MemoryRole::System,
            String::new(), // input 后续填充
            ctx.task_id().cloned(),
        );
        let trace_id = trace.id.clone();

        // Step 2.5: 工具已由 hr_domain.get_agent(with_tools=true) 加载到 agent.tools
        // wake_agent_brain 已将 Auto 工具移出（用于 Rig 注册），agent.tools 仅剩 Manual 工具
        // 直接提取 ToolPo 列表供 PromptBuilder 使用（builder 会按 tag 自动分块）
        let all_tools: Vec<crate::models::tool::ToolPo> = agent
            .tools()
            .iter()
            .map(|t| t.po.clone())
            .collect();

        // Step 2.6: 技能已由 hr_domain.get_agent(with_skills=true) 加载到 agent.skills
        // 技能只在 Agent 已安装的副本范围内（author_id = agent_id，排除 Expired）
        // 不匹配 match_keys 的技能不展示在 Prompt，由 Agent 通过 search_skill 神经工具按需加载
        let skill_pos: Vec<crate::models::skill::SkillPo> = agent
            .skills()
            .iter()
            .map(|s| s.po.clone())
            .collect();

        // Step 3: 拼装 Prompt（通过工厂方法获取对应 Agent 类型的 builder）
        // 统一注入，build 时按 tag 自动分块为神经工具/技能 → 常用工具/必加载技能 → 历史 → 当前消息
        let mut builder = self.prompt_builder(agent);
        builder.current_trace_id(&trace_id);
        builder.system_prompt(agent);
        builder.tools(&all_tools);
        builder.skills(&skill_pos);
        builder.history(&recent_memories);
        builder.current_message(message);

        // 【角色分工】只有客服类 Agent 才需要拼接用户喜好等信息
        // TODO: 后续从上层 Domain 传入用户画像信息
        let agent_roles = agent.po.get_roles();
        if agent_roles.contains(&"customer_service".to_string())
            || agent_roles.contains(&"客服".to_string())
        {
            // 预留：实际使用时从上层传入用户画像
            // builder.user_profile(user_profile_str);
        }

        let prompt = builder.build();

        // Step 4: 调用大脑思考
        // 统一走 BrainDal.think() 入口，方便审计、统计、监控
        let brain = agent
            .brain
            .as_ref()
            .ok_or_else(|| err!(Internal, "Agent 大脑未唤醒，请先调用 wake_brain()"))?;

        // 调用 think，先捕获结果（不立即 ?）
        let think_result = self.brain_dal().think(ctx.clone(), brain, &prompt).await;

        // 无论成功失败，最后都设置为 Idle
        AgentRuntimeStateManager::global()
            .set_idle(&agent.po.id);

        // 展开 Result，失败时也记录事件
        let raw_output = match think_result {
            Ok(output) => output,
            Err(e) => {
                // 记录唤醒失败事件
                let duration_ms = start_time.elapsed().map(|d| d.as_millis() as u64).unwrap_or(0);
                let _ = record_event!(ctx, AgentAwakeEvent {
                    agent_id: agent.po.id.clone(),
                    project_id: ctx.project_id().cloned(),
                    task_id: ctx.task_id().cloned(),
                    organization_id: ctx.organization_id.clone(),
                    user_id: Some(ctx.uid()),
                    message_id: Some(message.po.id.clone()),
                    call_count: 1,
                    duration_ms: duration_ms,
                    status: format!("failed: {}", e),
                });
                return Err(e);
            }
        };

        // Step 5: 回填 input 和 output，一次性写入完整 Trace
        trace.input = prompt.clone();
        trace.complete(raw_output.clone());

        // Step 6: 通过 RuntimeMemory 子模块写入
        // 架构：awakening → RuntimeMemory → MemoryDal → MemoryDao
        self.memory()
            .write_thinking_trace(ctx.clone(), trace)
            .await?;

        // Step 7: 记录 Agent 唤醒统计事件
        let duration_ms = start_time.elapsed().map(|d| d.as_millis() as u64).unwrap_or(0);
        let _ = record_event!(ctx, AgentAwakeEvent {
            agent_id: agent.po.id.clone(),
            project_id: ctx.project_id().cloned(),
            task_id: ctx.task_id().cloned(),
            organization_id: ctx.organization_id.clone(),
            user_id: Some(ctx.uid()),
            message_id: Some(message.po.id.clone()),
            call_count: 1,
            duration_ms: duration_ms,
            status: "success".to_string(),
        });

        // Step 8: 返回结果
        Ok(AwakeningResult {
            agent_id: agent.po.id.clone(),
            trace_ids: vec![trace_id],
            raw_input: prompt,
            raw_output,
        })
    }
}

#[cfg(test)]
mod tests {
    // ==================== awaken 集成测试 ====================

    use crate::models::agent::{Agent, AgentPo};
    use crate::models::brain::{Brain, Cortex, CortexTrait};
    use crate::models::file::FileMeta;
    use crate::models::message::Message;
    use crate::models::model_provider::ModelProvider;
    use crate::models::skill::SkillPo;
    use crate::pkg::RequestContext;
    use crate::pkg::tool_tracing::logger::ToolCallLogger;
    use crate::service::dal::brain::BrainDal;
    use async_trait::async_trait;
    use common::enums::{
        AgentStatus, MessageRole, MessageType, ModelCapability, ProviderType,
    };
    use common::enums::skill::SkillAuthorType;
    use sqlx::SqlitePool;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;
    use uuid::Uuid;

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
            _tools: Vec<crate::models::tool::Tool>,
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
            prompt: &str,
        ) -> common::error::Result<String> {
            *self.captured_prompt.lock().unwrap() = Some(prompt.to_string());
            Ok("mock response".to_string())
        }
    }

    /// Mock Cortex，仅用于构造 Brain（BrainDal 已 stub，Cortex 不会被实际调用）
    #[derive(Clone)]
    struct MockCortex;

    #[async_trait]
    impl CortexTrait for MockCortex {
        fn capability(&self) -> ModelCapability {
            ModelCapability::Agent
        }
        fn model_provider_id(&self) -> &str {
            "mock-provider"
        }
        fn model_name(&self) -> &str {
            "mock-model"
        }
        async fn prompt(&self, _prompt: &str) -> anyhow::Result<String> {
            Ok("mock response".to_string())
        }
        async fn embeddings(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.0; 3]).collect())
        }
        fn support_tools(&self) -> bool {
            false
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
        let model_provider = ModelProvider::new(
            "Mock Provider".to_string(),
            ProviderType::OpenAI,
            ModelCapability::Agent,
            "gpt-4".to_string(),
            "fake-key".to_string(),
            None,
            None,
            "test-user".to_string(),
        );
        let cortex = Cortex::new(model_provider, Box::new(MockCortex));
        let runtime_config = crate::models::agent::AgentRuntimeConfig::default();
        agent.brain = Some(Brain::new_local(
            agent_id.to_string(),
            "Test Agent".to_string(),
            runtime_config,
            cortex,
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
    async fn create_skill_for_agent(ctx: RequestContext, agent_id: &str, name: &str, description: &str) {
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
        agent.set_skills(skills);
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
        create_skill_for_agent(
            ctx.clone(),
            &agent_id,
            "DocWriting",
            "编写清晰的技术文档",
        )
        .await;

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
        );

        let result = runtime
            .awakening()
            .awaken(ctx.clone(), &agent, &message)
            .await
            .expect("awaken 应该成功");

        let prompt = captured_prompt.lock().unwrap().clone().expect("应该捕获到 prompt");

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
        );

        let result = runtime
            .awakening()
            .awaken(ctx.clone(), &agent, &message)
            .await
            .expect("awaken 应该成功");

        let prompt = captured_prompt.lock().unwrap().clone().expect("应该捕获到 prompt");

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
}
