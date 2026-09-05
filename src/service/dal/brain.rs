//! Brain DAL 模块
//!
//! 职责：从 ModelProvider 创建 Cortex，然后组合 Memory 创建完整的 Brain 实体
//! BrainDal 依赖 CortexDao 进行模型调用，然后组装成完整的 Brain
//! 合并了原来 CortexDal 的功能，不再重复拆分

use crate::models::agent::{AgentPo, ExternalAgentConfig};
use crate::models::brain::Brain;
use crate::models::cortex_types::{ThinkResult, TokenUsage, ToolDescriptor};
use crate::models::model_provider::ModelProvider;
use crate::models::vector::{VectorIndexParams, Vectorizable};
use crate::pkg::RequestContext;
use crate::service::dao::agent_runtime;
use crate::service::dao::cortex;
use crate::service::dao::model_provider::ModelProviderDao;
use crate::service::dao::tool_call::ToolCallDao;
use async_trait::async_trait;
use common::enums::AgentKind;
use common::error::{Result, err};
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
    // 此前为 `reqwest::Client::new()`（无超时）；构造职责收敛到 http 基建
    let http_client = crate::pkg::http::presets::outbound()
        .build()
        .expect("构建 Brain HTTP 客户端失败");
    let _ = BRAIN_DAL.set(new(
        crate::service::dao::tool_call::dao(),
        crate::service::dao::model_provider::dao(),
        http_client,
    ));
}

/// 创建 Brain DAL（返回 trait 对象）
pub fn new(
    tool_call_dao: Arc<dyn ToolCallDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
    http_client: reqwest::Client,
) -> Arc<dyn BrainDal> {
    Arc::new(BrainDalImpl {
        tool_call_dao,
        model_provider_dao,
        http_client,
    })
}

// ==================== DAL 接口 ====================

/// Brain DAL 接口
#[async_trait]
pub trait BrainDal: Send + Sync {
    /// 从 AgentPo 和记忆列表创建完整的 Brain
    ///
    /// - Local agent: 内部加载 ModelProvider，绑定到 Brain
    /// - 外部 agent (Cli/Remote): 不绑定 ModelProvider，仅保存 runtime_config
    /// - memories: 记忆列表，已经由上层创建好
    /// - 返回完整的 Brain 实例
    async fn wake_brain(
        &self,
        ctx: RequestContext,
        agent: &AgentPo,
        memories: Vec<crate::models::memory::Memory>,
    ) -> Result<Brain>;

    /// 测试模型提供商连通性，执行一次 prompt 获取回答
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
    /// - Local → native CortexDao.think（支持工具调用，返回 ThinkResult）
    /// - Cli → execute_cli（包装为 ThinkResult::Final）
    /// - Remote → execute_a2a（包装为 ThinkResult::Final）
    ///
    /// `tools` 参数仅 Local 分支使用（传递给模型进行 function calling）。
    /// `messages` 是完整的多轮对话历史（user → assistant(tool_calls) → tool(result)），
    /// 确保模型能看到之前的 tool_calls 和 tool 结果，从而正确终止循环。
    async fn think(
        &self,
        ctx: RequestContext,
        brain: &Brain,
        messages: &[crate::models::cortex_types::ChatMessage],
        tools: &[ToolDescriptor],
    ) -> Result<ThinkResult>;

    /// 向量化实体（domain 层入口）
    ///
    /// 内部查默认 Embedding Provider 后调 cortex 包级函数。
    /// 返回 `None` 表示无可用 provider，调用方降级跳过。
    async fn embed_entity(
        &self,
        ctx: RequestContext,
        entity: &dyn Vectorizable,
    ) -> Result<Option<VectorIndexParams>>;

    /// 向量化搜索关键词（domain 层入口）
    ///
    /// 与 `embed_entity` 类似，内部查默认 provider 后调用 cortex 包级函数。
    async fn embed_text_for_search(
        &self,
        ctx: RequestContext,
        text: &str,
    ) -> Result<Option<VectorIndexParams>>;
}

// ==================== DAL 实现 ====================

/// 从 messages 数组中提取最后一条 User 消息的内容
///
/// Cli/Remote agent 不支持多轮工具调用，仅将最后一条 user 消息作为 prompt 传入。
/// 如果没有 user 消息，返回空字符串。
fn extract_last_user_prompt(messages: &[crate::models::cortex_types::ChatMessage]) -> &str {
    messages
        .iter()
        .rev()
        .find_map(|m| match m {
            crate::models::cortex_types::ChatMessage::User { content } => Some(content.as_str()),
            _ => None,
        })
        .unwrap_or("")
}

/// Brain DAL 实现
#[allow(dead_code)]
struct BrainDalImpl {
    tool_call_dao: Arc<dyn ToolCallDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
    http_client: reqwest::Client,
}

#[async_trait]
impl BrainDal for BrainDalImpl {
    async fn wake_brain(
        &self,
        ctx: RequestContext,
        agent: &AgentPo,
        memories: Vec<crate::models::memory::Memory>,
    ) -> Result<Brain> {
        match agent.kind {
            AgentKind::Local => {
                // Agent 与 model provider 是显式绑定关系：缺 model 配置就直接报错，
                // 不做隐式降级。缺 model 的 Agent 应在状态层面表达"未就绪"（如 Interviewing），
                // 使用方在绑定 model 前不应把消息路由给它。
                let provider_po = self
                    .model_provider_dao
                    .find_by_id(ctx.clone(), &agent.model_provider_id)
                    .await?
                    .ok_or_else(|| {
                        err!(
                            NotFound,
                            "Agent {} 缺少对话模型配置（model_provider_id={:?}），请先在「模型提供商管理」中绑定对话模型",
                            agent.id,
                            agent.model_provider_id
                        )
                    })?;

                let runtime_config = agent.get_runtime_config();

                Ok(Brain::new_local(
                    agent.id.clone(),
                    agent.name.clone(),
                    runtime_config,
                    provider_po,
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

        log_debug!(
            ctx.clone(),
            "brain_test_connection",
            "test_connection start, provider_id={}, model={}",
            provider.po.id,
            provider.po.model_name,
        );

        let dao = cortex::native::registry().get(provider.po.provider_type);
        let result = dao
            .think(
                ctx,
                &provider.po,
                &[crate::models::cortex_types::ChatMessage::user(prompt)],
                &[],
            )
            .await?;

        match result {
            ThinkResult::Final { content, .. } => Ok(content),
            ThinkResult::ToolCall { content, .. } => Ok(content.unwrap_or_default()),
        }
    }

    async fn think(
        &self,
        ctx: RequestContext,
        brain: &Brain,
        messages: &[crate::models::cortex_types::ChatMessage],
        tools: &[ToolDescriptor],
    ) -> Result<ThinkResult> {
        let start = std::time::Instant::now();

        match brain.kind {
            AgentKind::Local => {
                let provider = brain
                    .model_provider
                    .as_ref()
                    .ok_or_else(|| err!(Internal, "Local brain 缺少 model_provider"))?;
                let ctx = enrich_ctx!(&ctx, provider);

                log_debug!(
                    ctx.clone(),
                    "brain_think",
                    "Brain think start, agent_id={}, kind=local, provider_id={}, model={}",
                    brain.agent_id,
                    provider.id,
                    provider.model_name,
                );

                let dao = cortex::native::registry().get(provider.provider_type);
                let result = dao.think(ctx.clone(), provider, messages, tools).await;

                log_debug!(
                    ctx.clone(),
                    "brain_think_complete",
                    "Brain think completed, agent_id={}, provider_id={}, elapsed={:?}",
                    brain.agent_id,
                    provider.id,
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

                // Cli agent 不支持多轮工具调用，从 messages 提取最后一条 user 消息作为 prompt。
                //
                // 注意：这里能拿到**完整**提示词（含人设/技能），前提是 Cli Agent 配套的
                // `FlatPromptBuilder`（见 CodexAgentDal::prompt_builder）已把 System + User
                // 合成为单条 User 消息。若换成会产出角色分离结构的 builder，System 会在此丢失。
                //
                // `prompt_template` 不在此处应用——模板由 `FlatPromptBuilder` 在拼装阶段处理，
                // 支持 {system} / {user} / {prompt} 占位符（见其文档）。
                let prompt = extract_last_user_prompt(messages);

                let external_config = brain
                    .runtime_config
                    .external_config
                    .as_ref()
                    .ok_or_else(|| err!(Internal, "Cli brain 缺少 external_config"))?;

                let content = match external_config {
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
                        .await?
                    }
                    _ => return Err(err!(Internal, "Cli agent 的 external_config 类型不匹配")),
                };

                log_debug!(
                    ctx.clone(),
                    "brain_think_complete",
                    "Brain think completed, agent_id={}, kind=cli, elapsed={:?}",
                    brain.agent_id,
                    start.elapsed()
                );

                Ok(ThinkResult::Final {
                    content,
                    usage: TokenUsage::default(),
                })
            }
            AgentKind::Remote => {
                log_debug!(
                    ctx.clone(),
                    "brain_think",
                    "Brain think start, agent_id={}, kind=remote, agent_name={}",
                    brain.agent_id,
                    brain.agent_name,
                );

                // Remote agent 不支持多轮工具调用，从 messages 提取最后一条 user 消息作为 prompt
                let prompt = extract_last_user_prompt(messages);

                let external_config = brain
                    .runtime_config
                    .external_config
                    .as_ref()
                    .ok_or_else(|| err!(Internal, "Remote brain 缺少 external_config"))?;

                let content = match external_config {
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
                        .await?
                    }
                    _ => return Err(err!(Internal, "Remote agent 的 external_config 类型不匹配")),
                };

                log_debug!(
                    ctx.clone(),
                    "brain_think_complete",
                    "Brain think completed, agent_id={}, kind=remote, elapsed={:?}",
                    brain.agent_id,
                    start.elapsed()
                );

                Ok(ThinkResult::Final {
                    content,
                    usage: TokenUsage::default(),
                })
            }
        }
    }

    async fn embed_entity(
        &self,
        ctx: RequestContext,
        entity: &dyn Vectorizable,
    ) -> Result<Option<VectorIndexParams>> {
        let provider = match self
            .model_provider_dao
            .get_default_embedding_provider(ctx.clone())
            .await?
        {
            Some(p) => p,
            None => return Ok(None),
        };
        let params = cortex::embed_entity(ctx, &provider, entity).await?;
        Ok(Some(params))
    }

    async fn embed_text_for_search(
        &self,
        ctx: RequestContext,
        text: &str,
    ) -> Result<Option<VectorIndexParams>> {
        let provider = match self
            .model_provider_dao
            .get_default_embedding_provider(ctx.clone())
            .await?
        {
            Some(p) => p,
            None => return Ok(None),
        };
        let params = cortex::embed_text_for_search(ctx, &provider, text).await?;
        Ok(Some(params))
    }
}
