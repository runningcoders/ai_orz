//! Brain DAL 模块
//!
//! 职责：从 ModelProvider 创建 Cortex，然后组合 Memory 创建完整的 Brain 实体
//! BrainDal 依赖 CortexDao 创建 CortexTrait，然后组装成完整的 Brain
//! 合并了原来 CortexDal 的功能，不再重复拆分

use common::enums::AgentKind;
use common::error::{err, Result};
use crate::models::agent::{AgentPo, ExternalAgentConfig};
use crate::models::brain::{Brain, Cortex};
use crate::models::model_provider::ModelProvider;
use crate::models::tool::Tool;
use crate::pkg::RequestContext;
use crate::service::dao::agent_runtime;
use crate::service::dao::cortex;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::ModelProviderDao;
use crate::service::dao::tool_call::ToolCallDao;
use async_trait::async_trait;
use std::sync::{Arc, OnceLock};

use crate::enrich_ctx;
// ==================== 单例管理 ====================

static BRAIN_DAL: OnceLock<Arc<dyn BrainDal>> = OnceLock::new();

/// 获取 Brain DAL 单例
pub fn dal() -> Arc<dyn BrainDal> {
    BRAIN_DAL.get().cloned().unwrap()
}

/// 初始化 Brain DAL
pub fn init() {
    let http_client = reqwest::Client::new();
    let _ = BRAIN_DAL.set(new(
        cortex::dao(),
        crate::service::dao::tool_call::dao(),
        crate::service::dao::model_provider::dao(),
        http_client,
    ));
}

/// 创建 Brain DAL（返回 trait 对象）
pub fn new(
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    tool_call_dao: Arc<dyn ToolCallDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
    http_client: reqwest::Client,
) -> Arc<dyn BrainDal> {
    Arc::new(BrainDalImpl {
        cortex_dao,
        tool_call_dao,
        model_provider_dao,
        http_client,
    })
}

// ==================== DAL 接口 ====================

/// Brain DAL 接口
#[async_trait]
pub trait BrainDal: Send + Sync {
    /// 从 AgentPo、记忆列表和工具列表创建完整的 Brain
    ///
    /// - Local agent: 内部加载 ModelProvider，调用 CortexDao 创建 Cortex
    /// - 外部 agent (Cli/Remote): 不创建 Cortex，仅保存 runtime_config
    /// - memories: 记忆列表，已经由上层创建好
    /// - tools: 绑定到该 Agent 的工具列表，从注册中心动态加载
    /// - 返回完整的 Brain 实例
    async fn wake_brain(
        &self,
        ctx: RequestContext,
        agent: &AgentPo,
        memories: Vec<crate::models::memory::Memory>,
        tools: Vec<Tool>,
    ) -> Result<Brain>;

    /// 创建 Cortex 并测试连通性，执行一次 prompt 获取回答
    ///
    /// 用于测试模型提供商连接是否正常
    async fn test_connection(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
        prompt: &str,
    ) -> Result<String>;

    /// 让大脑思考，执行 prompt 获取回答
    ///
    /// 【统一入口】所有模型调用都经过此方法，方便：
    /// - 统一审计日志
    /// - Token 统计和成本核算
    /// - 限流/重试策略
    /// - 调用链追踪
    ///
    /// 内部根据 brain.kind 分发：
    /// - Local → CortexDao.prompt
    /// - Cli → execute_cli
    /// - Remote → execute_a2a
    async fn think(
        &self,
        ctx: RequestContext,
        brain: &Brain,
        prompt: &str,
    ) -> Result<String>;
}

// ==================== DAL 实现 ====================

/// Brain DAL 实现
struct BrainDalImpl {
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    tool_call_dao: Arc<dyn ToolCallDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
    http_client: reqwest::Client,
}

#[async_trait]
impl BrainDal for BrainDalImpl {
    async fn wake_brain(
        &self,
        _ctx: RequestContext,
        agent: &AgentPo,
        memories: Vec<crate::models::memory::Memory>,
        tools: Vec<Tool>,
    ) -> Result<Brain> {
        match agent.kind {
            AgentKind::Local => {
                let provider_po = self
                    .model_provider_dao
                    .find_by_id(_ctx.clone(), &agent.model_provider_id)
                    .await?
                    .ok_or_else(|| {
                        err!(
                            Internal,
                            "Agent {} 的 ModelProvider {} 不存在",
                            agent.id,
                            agent.model_provider_id
                        )
                    })?;
                let provider = ModelProvider::from_po(provider_po);
                let ctx = enrich_ctx!(&_ctx, &provider);

                let rig_tools = self.tool_call_dao.wrap_for_rig(&tools, ctx.clone());

                let cortex_trait = self
                    .cortex_dao
                    .create_cortex_trait(ctx, &provider.po, rig_tools)
                    .map_err(|e: anyhow::Error| {
                        err!(Internal, "failed to create cortex: {e}")
                            .with_source::<common::error::Error>(e.into())
                    })?;

                let cortex = Cortex::new(provider.clone(), cortex_trait);
                let runtime_config = agent.get_runtime_config();

                Ok(Brain::new_local(
                    agent.id.clone(),
                    agent.name.clone(),
                    runtime_config,
                    cortex,
                    memories,
                ))
            }
            AgentKind::Cli | AgentKind::Remote => {
                let runtime_config = agent.get_runtime_config();
                Ok(Brain::new_external(
                    agent.kind,
                    agent.id.clone(),
                    agent.name.clone(),
                    runtime_config,
                    memories,
                ))
            }
        }
    }

    async fn test_connection(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
        prompt: &str,
    ) -> Result<String> {
        let ctx = enrich_ctx!(&ctx, provider);
        let cortex_trait = self
            .cortex_dao
            .create_cortex_trait(ctx.clone(), &provider.po, Vec::new())
            .map_err(|e: anyhow::Error| {
                err!(Internal, "failed to create cortex: {e}")
                    .with_source::<common::error::Error>(e.into())
            })?;

        let runtime_config = crate::models::agent::AgentRuntimeConfig::default();
        let temp_brain = Brain::new_local(
            "test".to_string(),
            "test".to_string(),
            runtime_config,
            Cortex::new(provider.clone(), cortex_trait),
            Vec::new(),
        );

        self.think(ctx, &temp_brain, prompt).await
    }

    async fn think(
        &self,
        ctx: RequestContext,
        brain: &Brain,
        prompt: &str,
    ) -> Result<String> {
        let start = std::time::Instant::now();

        match brain.kind {
            AgentKind::Local => {
                let cortex = brain
                    .cortex
                    .as_ref()
                    .ok_or_else(|| err!(Internal, "Local brain 缺少 cortex"))?;
                let ctx = enrich_ctx!(&ctx, &cortex.model_provider);

                log_debug!(
                    ctx.clone(),
                    "brain_think",
                    "Brain think start, agent_id={}, kind=local, provider_id={}, model={}",
                    brain.agent_id,
                    cortex.model_provider.po.id,
                    cortex.model_provider.po.model_name,
                );

                let result = self
                    .cortex_dao
                    .prompt(ctx.clone(), cortex.cortex(), prompt)
                    .await
                    .map_err(|e: anyhow::Error| {
                        err!(Internal, "brain think failed: {e}")
                            .with_source::<common::error::Error>(e.into())
                    });

                log_debug!(
                    ctx.clone(),
                    "brain_think_complete",
                    "Brain think completed, agent_id={}, provider_id={}, elapsed={:?}",
                    brain.agent_id,
                    cortex.model_provider.po.id,
                    start.elapsed()
                );

                result
            }
            AgentKind::Cli => {
                log_debug!(
                    ctx.clone(),
                    "brain_think",
                    "Brain think start, agent_id={}, kind=cli, agent_name={}",
                    brain.agent_id,
                    brain.agent_name,
                );

                let external_config = brain
                    .runtime_config
                    .external_config
                    .as_ref()
                    .ok_or_else(|| err!(Internal, "Cli brain 缺少 external_config"))?;

                let result = match external_config {
                    ExternalAgentConfig::Cli {
                        command,
                        args,
                        work_dir,
                        env,
                        timeout_secs,
                        prompt_template: _,
                    } => {
                        agent_runtime::codex::execute_cli(
                            &brain.agent_id,
                            command,
                            args,
                            work_dir,
                            env,
                            *timeout_secs,
                            prompt,
                        )
                        .await
                    }
                    _ => Err(err!(Internal, "Cli agent 的 external_config 类型不匹配")),
                };

                log_debug!(
                    ctx.clone(),
                    "brain_think_complete",
                    "Brain think completed, agent_id={}, kind=cli, elapsed={:?}",
                    brain.agent_id,
                    start.elapsed()
                );

                result
            }
            AgentKind::Remote => {
                log_debug!(
                    ctx.clone(),
                    "brain_think",
                    "Brain think start, agent_id={}, kind=remote, agent_name={}",
                    brain.agent_id,
                    brain.agent_name,
                );

                let external_config = brain
                    .runtime_config
                    .external_config
                    .as_ref()
                    .ok_or_else(|| err!(Internal, "Remote brain 缺少 external_config"))?;

                let result = match external_config {
                    ExternalAgentConfig::Remote {
                        endpoint,
                        agent_name: _target_agent_name,
                        auth_token,
                        timeout_secs: _,
                    } => {
                        agent_runtime::a2a::execute_a2a_send(
                            &self.http_client,
                            &brain.agent_id,
                            endpoint,
                            auth_token,
                            prompt,
                        )
                        .await
                    }
                    _ => Err(err!(
                        Internal,
                        "Remote agent 的 external_config 类型不匹配"
                    )),
                };

                log_debug!(
                    ctx.clone(),
                    "brain_think_complete",
                    "Brain think completed, agent_id={}, kind=remote, elapsed={:?}",
                    brain.agent_id,
                    start.elapsed()
                );

                result
            }
        }
    }
}