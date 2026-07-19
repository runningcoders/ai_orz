//! Runtime Awakening 具体实现

use common::error::{err, Result};
use crate::models::agent::Agent;
use crate::models::memory::MemoryTrace;
use crate::models::message::Message;
use crate::models::skill::SkillPo;
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
    /// - Local: 加载 builtin tools，通过 BrainDal.wake_brain 构造带 Cortex 的 Brain
    /// - External（Cli/Remote）: 构造不带 Cortex 的虚拟 Brain
    ///
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

        // Local agent 需要加载 tools 用于 Cortex 创建（rig 工具包装）
        // External agent 不需要 tools（BrainDal 内部走 new_external 分支）
        let tools = if agent.po.kind.is_local() {
            self.load_builtin_tools(ctx.clone(), agent).await?
        } else {
            Vec::new()
        };

        // 通过 BrainDal 构造 Brain（内部按 kind 分发）
        // memories 传空：awaken 时会独立加载 recent_memories 用于 prompt
        let brain = self.brain_dal()
            .wake_brain(ctx, &agent.po, Vec::new(), tools)
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

        // Step 2: 收集关联的 Trace ID 列表
        // 简易版：暂时为空，后续从 message.metadata 提取
        let trace_ids: Vec<String> = vec![];

        // Step 3: 预先构造 MemoryTrace 拿到 trace_id
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

        // Step 3.5: 加载内置工具（神经工具 + 已安装工具包）
        let builtin_tools = self.load_builtin_tools(ctx.clone(), agent).await?;
        let builtin_tool_prompts: Vec<String> = builtin_tools
            .iter()
            .map(|tool| tool.po.to_tool_prompt())
            .collect();

        // Step 3.6: 加载 Agent 技能副本用于 Prompt 注入
        let skills = self.load_agent_skills(ctx.clone(), &agent.po.id).await?;

        // Step 4: 拼装 Prompt（通过工厂方法获取对应 Agent 类型的 builder）
        let mut builder = self.prompt_builder(agent);
        builder.current_trace_id(&trace_id);
        builder.trace_ids(&trace_ids);
        builder.system_prompt(agent);
        builder.bound_tools(agent);
        builder.builtin_tools(&builtin_tool_prompts);
        builder.agent_skills(&skills);
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

        // Step 5: 调用大脑思考
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

        // Step 6: 回填 input 和 output，一次性写入完整 Trace
        trace.input = prompt.clone();
        trace.complete(raw_output.clone());

        // Step 7: 通过 RuntimeMemory 子模块写入
        // 架构：awakening → RuntimeMemory → MemoryDal → MemoryDao
        self.memory()
            .write_thinking_trace(ctx.clone(), trace)
            .await?;

        // Step 8: 记录 Agent 唤醒统计事件
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

        // Step 9: 返回结果
        Ok(AwakeningResult {
            agent_id: agent.po.id.clone(),
            trace_ids: vec![trace_id],
            raw_input: prompt,
            raw_output,
        })
    }
}

impl RuntimeDomainImpl {
    /// 加载所有内置工具（神经工具 + 已安装工具包工具）
    ///
    /// 内置工具是 Agent 天生具备或入职培训获得的能力，不需要显式绑定：
    /// - 神经工具：tags 包含 "neural"，所有 Agent 天生拥有
    /// - 已安装工具包工具：tags 与 agent.installed_tags 有交集
    ///
    /// 通过 SQL 层 tag 过滤（OR 语义）直接查询命中的工具，避免全量加载到内存。
    async fn load_builtin_tools(
        &self,
        ctx: RequestContext,
        agent: &Agent,
    ) -> Result<Vec<crate::models::tool::Tool>> {
        use crate::service::dao::tool::ToolQuery;
        use common::enums::ToolStatus;

        // 构建 tag 过滤列表：neural + agent 的 installed_tags
        // SQL 层使用 OR 语义，命中任一 tag 的工具都会被返回
        let mut tag_filter = vec!["neural".to_string()];
        tag_filter.extend(agent.po.get_installed_tags());

        self.tool_dal
            .query(
                ctx,
                ToolQuery {
                    tags: Some(tag_filter),
                    enabled_only: Some(true),
                    status: Some(ToolStatus::Enabled),
                    ..Default::default()
                },
            )
            .await
    }

    /// 加载 Agent 的技能副本用于 Prompt 注入
    ///
    /// 通过 SkillDal 全局单例查询 Agent 的技能副本（author_id = agent_id），
    /// 返回 SkillPo 列表供 PromptBuilder.agent_skills() 注入"【可用技能】"部分。
    /// 与 memory 模块一样使用全局 DAL 单例，保持运行时数据加载的一致性。
    async fn load_agent_skills(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<SkillPo>> {
        use crate::service::dal::skill::dal as skill_dal;
        let skills = skill_dal().list_for_agent(ctx, agent_id).await?;
        Ok(skills.into_iter().map(|s| s.po).collect())
    }
}

/// 从全部工具中筛选内置工具（神经工具 + 已安装工具包工具）
///
/// 筛选规则：
/// 1. 神经工具：tags 包含 "neural"，所有 Agent 天生拥有
/// 2. 已安装工具包工具：tags 与 installed_tags 有交集
///
/// 保留用于单元测试和可能的回退；生产路径已改为 SQL 层 tag 过滤。
#[allow(dead_code)]
fn filter_builtin_tools(
    all_tools: Vec<crate::models::tool::Tool>,
    installed_tags: &[String],
) -> Vec<crate::models::tool::Tool> {
    all_tools
        .into_iter()
        .filter(|tool| {
            let tags = tool.po.get_tags();
            if tags.contains(&"neural".to_string()) {
                return true;
            }
            installed_tags.iter().any(|installed| tags.contains(installed))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::filter_builtin_tools;
    use crate::models::tool::{Tool, ToolPo};
    use common::enums::ToolProtocol;
    use serde_json::json;

    fn make_tool(tool_id: &str, tags: Vec<&str>) -> Tool {
        let po = ToolPo::new(
            tool_id.to_string(),
            tool_id.to_string(),
            "test tool".to_string(),
            ToolProtocol::Builtin,
            json!({}),
            Some(json!({ "type": "object" })),
            tags.into_iter().map(String::from).collect(),
            Some("test-user".to_string()),
        );
        Tool::from_po_for_management(po)
    }

    fn make_tags(tags: Vec<&str>) -> Vec<String> {
        tags.into_iter().map(String::from).collect()
    }

    #[test]
    fn filter_includes_neural_tools_regardless_of_installed_tags() {
        let tools = vec![
            make_tool("neural-1", vec!["neural"]),
            make_tool("external-1", vec!["external"]),
        ];
        let installed = make_tags(vec![]);

        let result = filter_builtin_tools(tools, &installed);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].po.id, "neural-1");
    }

    #[test]
    fn filter_includes_installed_tool_pack_tools() {
        let tools = vec![
            make_tool("neural-1", vec!["neural"]),
            make_tool("pm-1", vec!["project_management"]),
            make_tool("da-1", vec!["data_analysis"]),
            make_tool("external-1", vec!["external"]),
        ];
        let installed = make_tags(vec!["project_management"]);

        let result = filter_builtin_tools(tools, &installed);

        assert_eq!(result.len(), 2);
        let ids: Vec<&str> = result.iter().map(|t| t.po.id.as_str()).collect();
        assert!(ids.contains(&"neural-1"));
        assert!(ids.contains(&"pm-1"));
    }

    #[test]
    fn filter_excludes_tools_when_no_tags_installed() {
        let tools = vec![
            make_tool("neural-1", vec!["neural"]),
            make_tool("pm-1", vec!["project_management"]),
        ];
        let installed = make_tags(vec![]);

        let result = filter_builtin_tools(tools, &installed);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].po.id, "neural-1");
    }

    #[test]
    fn filter_includes_tool_with_multiple_tags_when_one_matches() {
        let tools = vec![
            // Tool has both "project_management" and "advanced" tags
            make_tool("multi-1", vec!["project_management", "advanced"]),
        ];
        let installed = make_tags(vec!["project_management"]);

        let result = filter_builtin_tools(tools, &installed);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].po.id, "multi-1");
    }

    /// 模拟 SQL 层 tag 过滤（OR 语义），与 ToolDao sqlite 实现的 json_each IN 逻辑一致
    ///
    /// 给定 tag_filter 列表，保留 tags 与 tag_filter 有任一交集的工具
    fn simulate_sql_tag_filter(
        tools: Vec<Tool>,
        tag_filter: &[String],
    ) -> Vec<Tool> {
        tools
            .into_iter()
            .filter(|tool| {
                let tags = tool.po.get_tags();
                tag_filter.iter().any(|tag| tags.contains(tag))
            })
            .collect()
    }

    /// 构建 load_builtin_tools 使用的 tag_filter：neural + installed_tags
    fn build_tag_filter(installed_tags: &[String]) -> Vec<String> {
        let mut tag_filter = vec!["neural".to_string()];
        tag_filter.extend(installed_tags.iter().cloned());
        tag_filter
    }

    /// 工具规格（id + tags），用于批量构建工具集（Tool 不可 clone，需要独立创建两份）
    fn make_tools_from_spec(spec: &[(&str, Vec<&str>)]) -> Vec<Tool> {
        spec.iter().map(|(id, tags)| make_tool(id, tags.clone())).collect()
    }

    #[test]
    fn sql_filter_equivalent_to_memory_filter_for_neural_only() {
        let spec: &[(&str, Vec<&str>)] = &[
            ("neural-1", vec!["neural"]),
            ("external-1", vec!["external"]),
            ("external-2", vec!["other"]),
        ];
        let installed = make_tags(vec![]);
        let tag_filter = build_tag_filter(&installed);

        let memory_result = filter_builtin_tools(make_tools_from_spec(spec), &installed);
        let sql_result = simulate_sql_tag_filter(make_tools_from_spec(spec), &tag_filter);

        let memory_ids: Vec<&str> = memory_result.iter().map(|t| t.po.id.as_str()).collect();
        let sql_ids: Vec<&str> = sql_result.iter().map(|t| t.po.id.as_str()).collect();
        assert_eq!(memory_ids, sql_ids);
        assert_eq!(sql_ids, vec!["neural-1"]);
    }

    #[test]
    fn sql_filter_equivalent_to_memory_filter_with_installed_tags() {
        let spec: &[(&str, Vec<&str>)] = &[
            ("neural-1", vec!["neural"]),
            ("pm-1", vec!["project_management"]),
            ("da-1", vec!["data_analysis"]),
            ("external-1", vec!["external"]),
            ("multi-1", vec!["project_management", "advanced"]),
        ];
        let installed = make_tags(vec!["project_management"]);
        let tag_filter = build_tag_filter(&installed);

        let memory_result = filter_builtin_tools(make_tools_from_spec(spec), &installed);
        let sql_result = simulate_sql_tag_filter(make_tools_from_spec(spec), &tag_filter);

        let mut memory_ids: Vec<&str> = memory_result.iter().map(|t| t.po.id.as_str()).collect();
        memory_ids.sort();
        let mut sql_ids: Vec<&str> = sql_result.iter().map(|t| t.po.id.as_str()).collect();
        sql_ids.sort();
        assert_eq!(memory_ids, sql_ids);
        assert!(sql_ids.contains(&"neural-1"));
        assert!(sql_ids.contains(&"pm-1"));
        assert!(sql_ids.contains(&"multi-1"));
        assert!(!sql_ids.contains(&"da-1"));
        assert!(!sql_ids.contains(&"external-1"));
    }

    #[test]
    fn sql_filter_equivalent_to_memory_filter_with_multiple_installed_tags() {
        let spec: &[(&str, Vec<&str>)] = &[
            ("neural-1", vec!["neural"]),
            ("pm-1", vec!["project_management"]),
            ("da-1", vec!["data_analysis"]),
            ("cross-1", vec!["project_management", "data_analysis"]),
            ("external-1", vec!["external"]),
        ];
        let installed = make_tags(vec!["project_management", "data_analysis"]);
        let tag_filter = build_tag_filter(&installed);

        let memory_result = filter_builtin_tools(make_tools_from_spec(spec), &installed);
        let sql_result = simulate_sql_tag_filter(make_tools_from_spec(spec), &tag_filter);

        let mut memory_ids: Vec<&str> = memory_result.iter().map(|t| t.po.id.as_str()).collect();
        memory_ids.sort();
        let mut sql_ids: Vec<&str> = sql_result.iter().map(|t| t.po.id.as_str()).collect();
        sql_ids.sort();
        assert_eq!(memory_ids, sql_ids);
        assert_eq!(sql_ids, vec!["cross-1", "da-1", "neural-1", "pm-1"]);
    }

    #[test]
    fn sql_filter_equivalent_to_memory_filter_empty_tools() {
        let spec: &[(&str, Vec<&str>)] = &[];
        let installed = make_tags(vec!["project_management"]);
        let tag_filter = build_tag_filter(&installed);

        let memory_result = filter_builtin_tools(make_tools_from_spec(spec), &installed);
        let sql_result = simulate_sql_tag_filter(make_tools_from_spec(spec), &tag_filter);

        assert_eq!(memory_result.len(), 0);
        assert_eq!(sql_result.len(), 0);
    }

    #[test]
    fn sql_filter_equivalent_to_memory_filter_tool_with_no_tags() {
        let spec: &[(&str, Vec<&str>)] = &[
            ("neural-1", vec!["neural"]),
            ("no-tags-1", vec![]),
        ];
        let installed = make_tags(vec!["project_management"]);
        let tag_filter = build_tag_filter(&installed);

        let memory_result = filter_builtin_tools(make_tools_from_spec(spec), &installed);
        let sql_result = simulate_sql_tag_filter(make_tools_from_spec(spec), &tag_filter);

        let memory_ids: Vec<&str> = memory_result.iter().map(|t| t.po.id.as_str()).collect();
        let sql_ids: Vec<&str> = sql_result.iter().map(|t| t.po.id.as_str()).collect();
        assert_eq!(memory_ids, sql_ids);
        assert_eq!(sql_ids, vec!["neural-1"]);
    }

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
    async fn create_skill_for_agent(ctx: RequestContext, agent_id: &str, name: &str, description: &str) {
        let skill_po = SkillPo::new(
            format!("skill-{}--{}", name.to_lowercase(), Uuid::new_v4()),
            name.to_string(),
            description.to_string(),
            vec!["test".to_string()],
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

    #[sqlx::test]
    async fn test_awaken_with_skills(pool: SqlitePool) {
        let ctx = init_awaken_test_env(pool);

        let agent_id = format!("agent-with-skills-{}", Uuid::now_v7());
        let agent = make_test_agent(&agent_id);

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

        // 验证 Prompt 包含"【可用技能】"部分
        assert!(
            prompt.contains("【可用技能】"),
            "Prompt 应该包含【可用技能】部分，实际: {}",
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

        // 验证 Prompt 不包含"【可用技能】"部分
        assert!(
            !prompt.contains("【可用技能】"),
            "Prompt 不应该包含【可用技能】部分（Agent 无技能），实际: {}",
            prompt
        );

        // 验证返回结果仍然正常
        assert_eq!(result.agent_id, agent_id);
        assert!(!result.raw_input.is_empty());
        assert_eq!(result.raw_output, "mock response");
    }
}
