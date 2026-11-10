//! Brain DAL 模块
//!
//! 职责：从 ModelProvider 创建 Cortex，然后组合 Memory 创建完整的 Brain 实体
//! BrainDal 依赖 CortexDao 进行模型调用，然后组装成完整的 Brain
//! 合并了原来 CortexDal 的功能，不再重复拆分

use crate::models::agent::{AgentPo, ExternalAgentConfig};
use crate::models::brain::Brain;
use crate::models::cortex_types::{ModelCallOptions, ThinkResult, TokenUsage, ToolDescriptor};
use crate::models::model_provider::ModelProvider;
use crate::models::vector::{VectorIndexParams, Vectorizable};
use crate::pkg::RequestContext;
use crate::pkg::policy::{DecisionAnswers, DecisionRequest, DecisionValue, QuestionSpec};
use crate::pkg::stats::ModelCallEvent;
use crate::service::dao::agent_runtime;
use crate::service::dao::cortex;
use crate::service::dao::model_provider::ModelProviderDao;
use crate::service::dao::tool_call::ToolCallDao;
use async_trait::async_trait;
use common::api::cerebellum::{
    CerebellumAnswer, CerebellumQuestion, CerebellumUsage, DegradedReason, QuestionCriteria,
    QuestionType,
};
use common::enums::{AgentKind, ModelCapability};
use common::error::{Result, err};
use serde_json::Value;
use std::collections::BTreeMap;
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

// ==================== 决策结果 ====================

/// 一次小脑决策的出口：答案 + 降级原因 + 计量
///
/// ⚠️ 刻意**不是** `Result`：小脑是可选增强，任何失败都收敛为
/// `degraded = Some(..)` + 空 `answers`。调用方拿不到 `Err`，
/// 也就无法用一个 `?` 把「没配小脑」升级成主流程故障——
/// 这正是 `wake_brain` 里「小脑装配失败不能阻断唤醒」的同一条原则。
#[derive(Debug, Clone)]
pub struct DecisionOutcome {
    /// 各题结论；空表 = 无结论（调用方按「未命中」处理，**不得当作「否」**）
    pub answers: DecisionAnswers,
    /// 降级原因；`None` = 正常拿到结论
    pub degraded: Option<DegradedReason>,
    /// 本次调用耗时（毫秒，计量用）
    pub latency_ms: u64,
    /// token 用量（降级时为零值）
    pub usage: CerebellumUsage,
}

impl DecisionOutcome {
    /// 降级结果：空答案 + 指定原因
    ///
    /// 零调用路径（provider 未装配 / api_key 空 / 请求为空）统一走这里，
    /// 保证「降级」在类型上与「拿到结论但为空」无法混淆。
    fn degraded(reason: DegradedReason, latency_ms: u64) -> Self {
        Self {
            answers: DecisionAnswers::new(),
            degraded: Some(reason),
            latency_ms,
            usage: CerebellumUsage::default(),
        }
    }

    /// 轨迹口径的耗时：`None` = 本次**没有真正发出模型请求**
    ///
    /// 与 [`DecisionOutcome::latency_ms`] 的分工：后者是原始计量（无论是否调用都记），
    /// 前者只回答「这次到底问没问」。思考轨迹的 `latency_ms` 要的是后者——
    /// 把「没问」记成「问了 0ms」会让 p95 时延和跳过率同时失真。
    ///
    /// 零调用路径各自靠**不同机制**判 `None`（别混写成一条）：
    /// - 门 2/3/4（provider 未装配 / api_key 空 / DAO 未初始化）：统一以
    ///   [`DegradedReason::NoProvider`] 变体标记 ⇒ **靠变体判**，与耗时数值无关
    ///   （这三条返回的 `elapsed_ms()` 可能是 0 也可能是 1~2ms）。
    /// - 门 1（请求为空）：不设 `degraded`（它不是降级），唯一特征是
    ///   `latency_ms` 被固定写成 `0` ⇒ **靠 `latency_ms == 0` 判**。
    ///   真实 HTTP 往返不可能为 0ms；测试侧要表达「调用过」必须注入 ≥ 1ms。
    /// - 其余（正常 / `Timeout` / `CallError`）：即便失败也真实付了时延 ⇒ `Some(实测值)`。
    ///
    /// 新增零调用路径**必须同步本函数**，否则轨迹会把「没问」记成「问了」。
    pub fn traced_latency_ms(&self) -> Option<u64> {
        if matches!(self.degraded, Some(DegradedReason::NoProvider)) {
            return None;
        }
        (self.latency_ms > 0).then_some(self.latency_ms)
    }
}

// ==================== DAL 接口 ====================

/// Brain DAL 接口
#[async_trait]
pub trait BrainDal: Send + Sync {
    /// 从 AgentPo 和记忆列表创建完整的 Brain
    ///
    /// - Local agent: 内部加载 ModelProvider，绑定到 Brain，并注入默认小脑
    ///   （`get_default_cerebellum_provider`，无启用记录 → None）
    /// - 外部 agent (Cli/Remote): 不绑定 ModelProvider，仅保存 runtime_config（小脑维持 None）
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
        options: &ModelCallOptions,
    ) -> Result<ThinkResult>;

    /// 小脑决策：把一批决策请求交给外部决策模型，返回结构化结论
    ///
    /// 本方法是 domain 层**唯一**的小脑入口，禁止 Domain 直连 DAO：
    /// provider 选择（取 `brain.cerebellum`）、协议双向转换、非空门禁、
    /// 超时、降级与计量全部收敛在这里。上层只看到
    /// `DecisionRequest` / `DecisionAnswers` 这一对纯数据契约，
    /// 不感知 `CerebellumQuestion` 等协议 DTO。
    ///
    /// ⚠️ **永不返回 `Err`**：小脑是可选增强，失败一律收敛进
    /// `DecisionOutcome::degraded`，主流程零阻断。
    async fn decide(
        &self,
        ctx: RequestContext,
        brain: &Brain,
        requests: Vec<DecisionRequest>,
    ) -> DecisionOutcome;

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
            // 多模态消息取 text part（防静默空 prompt；批4 方案 §2.4）
            crate::models::cortex_types::ChatMessage::UserMultimodal { text, .. } => {
                Some(text.as_str())
            }
            _ => None,
        })
        .unwrap_or("")
}

/// `DecisionRequest` → 协议 DTO
///
/// Pick / Grade / Presence 与 choice / score / noul 一一对应。
/// 这里唯一需要「翻译」的地方就是题型映射——策略只需声明语义，协议细节留在 DAL。
///
/// `pub(super)`：仅为了让 `dal::brain_test` 能对这组映射做契约测试
/// （`Presence` 误带 criteria 会让**整批**请求 400，是本层最典型的静默雷）。
pub(super) fn to_protocol(req: DecisionRequest) -> (Value, BTreeMap<String, CerebellumQuestion>) {
    let questions = req
        .questions
        .into_iter()
        .map(|(qid, spec)| {
            let question = match spec {
                QuestionSpec::Pick {
                    instructions,
                    options,
                } => CerebellumQuestion {
                    question_type: QuestionType::Choice,
                    instructions,
                    criteria: Some(QuestionCriteria::Choices(options)),
                },
                QuestionSpec::Grade {
                    instructions,
                    levels,
                } => CerebellumQuestion {
                    question_type: QuestionType::Score,
                    instructions,
                    criteria: Some(QuestionCriteria::Levels(levels)),
                },
                QuestionSpec::Presence { instructions } => CerebellumQuestion {
                    question_type: QuestionType::Noul,
                    instructions,
                    criteria: None,
                },
            };
            (qid, question)
        })
        .collect();
    (req.state, questions)
}

/// 协议答案 → 引擎结论
///
/// 三个变体一一对应，无损耗。`score` 的 `legend` / `probabilities` 不搬运：
/// 策略判据只看分数与置信度，多带的字段只会让契约变形。
pub(super) fn to_decision_value(answer: &CerebellumAnswer) -> DecisionValue {
    match answer {
        CerebellumAnswer::Choice {
            choice,
            confidence,
            probabilities,
        } => DecisionValue::Pick {
            chosen: choice.clone(),
            confidence: *confidence,
            weights: probabilities.clone(),
        },
        CerebellumAnswer::Score {
            score, confidence, ..
        } => DecisionValue::Grade {
            score: *score,
            confidence: *confidence,
        },
        CerebellumAnswer::Noul { noul } => DecisionValue::Presence { value: *noul },
    }
}

/// 批次实际生效的**场景级**超时覆盖（毫秒）：同批多个覆盖取最大
///
/// 覆盖声明的语义是「本场景至少需要这么多」，同批多个覆盖时掐掉任何一个
/// 都等于让那个场景注定超时，故取 max；无覆盖返回 `None`，调用方回退
/// provider 配置 / 全局缺省。
///
/// 抽成顶层纯函数只为**可断言**：`decide` 里的超时值不能靠「读代码相信」，
/// 而 `tokio::time::timeout` 的实际等待时长在单测里没有干净的观测点。
///
/// **混批**：当前无混批路径 —— 全仓每个 `.decide(` 调用点都是「一个 scene 一组
/// request 单独发一次」，无 `extend`、无跨 scene 拼接，故本函数是防御性代码。
/// 将来若引入混批，必须改为 per-scene 独立 timeout：否则 `max` 会让本该 1s
/// 降级的场景白等 5s，那是净回归。
pub(crate) fn scene_timeout_override(requests: &[DecisionRequest]) -> Option<u64> {
    requests.iter().filter_map(|r| r.timeout_ms).max()
}

/// 超时取值梯子的**唯一**纯函数形态：场景覆盖 > provider 配置
///
/// 返回 `None` = 两侧都没声明，由调用方兜 `DEFAULT_CEREBELLUM_TIMEOUT_MS`。
/// 抽出来是为「两层各有锁」：`DecisionRequest.timeout_ms == Some(5000)` 只证明
/// 「声明到了请求上」，证不了「`decide` 真按它算」——某天 override 被摘掉、
/// 值悄悄落回 1000 时，只有这条梯子断言会红。
pub(crate) fn effective_timeout_ms(
    scene_override: Option<u64>,
    provider_timeout_ms: Option<u64>,
) -> Option<u64> {
    scene_override.or(provider_timeout_ms)
}

/// provider 显式超时**被场景覆盖**时的留痕值：`Some(provider)` = 「配了但没生效」
///
/// 语义刻意收窄（否则会刷噪声、最终被运维学会忽略）：**仅当**两侧都显式声明
/// 且**不相等**时才返回 `Some(provider_timeout_ms)`。三种情况一律 `None`：
/// - 场景侧没覆盖 —— provider 值是生效的，不是被忽略；
/// - provider 侧没配 —— 无值可忽略；
/// - 两侧相等 —— 配了也生效了，报 warn 是假警报。
pub(crate) fn ignored_provider_timeout_ms(
    scene_override: Option<u64>,
    provider_timeout_ms: Option<u64>,
) -> Option<u64> {
    match (scene_override, provider_timeout_ms) {
        (Some(scene), Some(provider)) if scene != provider => Some(provider),
        _ => None,
    }
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

                let mut brain = Brain::new_local(
                    agent.id.clone(),
                    agent.name.clone(),
                    runtime_config,
                    provider_po,
                    memories,
                );
                // 小脑装配（B2 二期）：Local Agent 注入默认小脑（status 单启用即默认）。
                //
                // ⚠️⚠️ 装配失败**不能阻断唤醒**（原写法是 `?`，已改）。小脑是
                // **可选增强**（B3 快判断路由，命中与否都只是优化），主模型思考
                // 完全不依赖它。用 `?` 会把「小脑查不到」升级成整个 `wake_brain`
                // 失败 ⇒ Agent 置忙后零进展，而消费侧 `wake_agent_brain` 的
                // `inspect_err` 只 set_idle 不打日志，现场只剩沉默。
                // ⇒ 失败降级为 `None`（= 无小脑），主流程照常走普通思考。
                brain.cerebellum = match self
                    .model_provider_dao
                    .get_default_cerebellum_provider(ctx.clone())
                    .await
                {
                    Ok(provider) => provider,
                    Err(e) => {
                        log_warn!(
                            &ctx,
                            "wake_brain",
                            "load default cerebellum provider failed, degrade to no-cerebellum: {}",
                            e
                        );
                        None
                    }
                };
                Ok(brain)
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
            "test_connection start, provider_id={}, model={}, capability={:?}",
            provider.po.id,
            provider.po.model_name,
            provider.capability(),
        );

        // 能力决定探针形态：向量化模型没有 chat 端点，小脑模型不兼容 OpenAI 协议，
        // 统一按 Agent 路径发 think 只会在上游换来一个与配置无关的 400。
        match provider.capability() {
            ModelCapability::Embedding => {
                // 固定短句 + 维度校验：向量模型能连通的最小证据就是「返回非空向量」
                let start = std::time::Instant::now();
                let dao = cortex::native::registry().embed_dao(&provider.po);
                let vectors = dao
                    .embed(ctx, &provider.po, &["connectivity test".to_string()])
                    .await?;
                let dim = vectors.first().map(Vec::len).unwrap_or(0);
                if dim == 0 {
                    return Err(err!(
                        Internal,
                        "embedding test returned empty vector (model={})",
                        provider.po.model_name
                    ));
                }
                Ok(format!(
                    "向量化连通成功：维度 {dim}，耗时 {} ms",
                    start.elapsed().as_millis()
                ))
            }
            ModelCapability::Decision => {
                let start = std::time::Instant::now();
                let questions = BTreeMap::from([(
                    "connectivity".to_string(),
                    CerebellumQuestion {
                        question_type: QuestionType::Noul,
                        instructions: format!("这是一次连通性测试，输入内容：{prompt}"),
                        criteria: None,
                    },
                )]);
                let result = crate::service::dao::cerebellum::try_dao()
                    .ok_or_else(|| {
                        err!(
                            Internal,
                            "cerebellum dao not initialized; cannot test decision provider"
                        )
                    })?
                    .think_fast(
                        ctx,
                        &provider.po,
                        serde_json::json!({ "prompt": prompt }),
                        questions,
                    )
                    .await?;
                let noul = result.answers.get("connectivity").and_then(|a| a.as_noul());
                Ok(match noul {
                    Some(p) => format!(
                        "小脑判定连通成功：noul={p:.3}，耗时 {} ms",
                        start.elapsed().as_millis()
                    ),
                    None => format!(
                        "小脑判定连通成功（未返回 noul 答案），耗时 {} ms",
                        start.elapsed().as_millis()
                    ),
                })
            }
            ModelCapability::Agent => {
                let dao = cortex::native::registry().chat_dao(&provider.po);
                let result = dao
                    .think(
                        ctx,
                        &provider.po,
                        &[crate::models::cortex_types::ChatMessage::user(prompt)],
                        &[],
                        // 连通性测试不经过思考循环：options 内 thinking_intensity=None，
                        // 由请求体构建层回退到 provider 默认档位
                        &ModelCallOptions::default(),
                    )
                    .await?;

                match result {
                    ThinkResult::Final { content, .. } => Ok(content),
                    ThinkResult::ToolCall { content, .. } => Ok(content.unwrap_or_default()),
                }
            }
        }
    }

    async fn think(
        &self,
        ctx: RequestContext,
        brain: &Brain,
        messages: &[crate::models::cortex_types::ChatMessage],
        tools: &[ToolDescriptor],
        options: &ModelCallOptions,
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

                let dao = cortex::native::registry().chat_dao(provider);
                let result = dao
                    .think(ctx.clone(), provider, messages, tools, options)
                    .await;

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
                    reasoning_items: Vec::new(),
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
                    reasoning_items: Vec::new(),
                })
            }
        }
    }

    async fn decide(
        &self,
        ctx: RequestContext,
        brain: &Brain,
        requests: Vec<DecisionRequest>,
    ) -> DecisionOutcome {
        let started = std::time::Instant::now();
        let elapsed_ms = || started.elapsed().as_millis() as u64;

        // 门 1：请求为空 = 本轮没有决策诉求（策略组 prepare 全返回空）。
        // 这是**正常无操作**，不是降级——`degraded: None` + 空答案。
        if requests.is_empty() {
            return DecisionOutcome {
                answers: DecisionAnswers::new(),
                degraded: None,
                latency_ms: 0,
                usage: CerebellumUsage::default(),
            };
        }

        // 门 2：小脑未装配（外部 Agent 恒 None；Local 无启用 provider 亦为 None）
        let Some(provider) = brain.cerebellum.as_ref() else {
            return DecisionOutcome::degraded(DegradedReason::NoProvider, elapsed_ms());
        };

        // 门 3：api_key 空 ⇒ 零调用降级。拿空凭据打网络最轻是白等一个超时，
        // 最重是错误顺着 future 传播进主循环。
        if provider.api_key.trim().is_empty() {
            log_warn!(
                ctx,
                "brain_decide",
                "cerebellum provider api_key is empty, degrade: provider_id={}",
                provider.id
            );
            return DecisionOutcome::degraded(DegradedReason::NoProvider, elapsed_ms());
        }

        // 门 4：DAO 单例未初始化 ⇒ 零调用降级。
        // `dao()` 未初始化是 `expect` panic，会顺着 future 把整轮思考一起炸掉。
        let Some(dao) = crate::service::dao::cerebellum::try_dao() else {
            log_warn!(
                ctx,
                "brain_decide",
                "cerebellum dao not initialized, degrade"
            );
            return DecisionOutcome::degraded(DegradedReason::NoProvider, elapsed_ms());
        };

        // 单一超时预算：**场景级覆盖 > provider.config.timeout_ms > 全局缺省**
        // （DEFAULT_CEREBELLUM_TIMEOUT_MS = 3000ms；`chain_reply` 另行声明
        // 5000ms 场景覆盖，用于多读链内上下文）。
        // 整个批次共享——多场景同轮触发时不该各付一次超时。
        // 取值梯子抽成 `effective_timeout_ms`：两层各有锁，防 override 被摘掉后
        // 值悄悄落回缺省而无人发觉。
        let scene_timeout_ms = scene_timeout_override(&requests);
        let provider_timeout_ms =
            crate::service::dao::cerebellum::client::provider_timeout_ms(&ctx, provider);
        let timeout_ms = effective_timeout_ms(scene_timeout_ms, provider_timeout_ms)
            .unwrap_or(crate::service::dao::cerebellum::DEFAULT_CEREBELLUM_TIMEOUT_MS);
        let timeout_source = if scene_timeout_ms.is_some() {
            "scene_override"
        } else {
            "provider_or_default"
        };

        // provider 侧显式 `timeout_ms` 被场景覆盖时必须留痕：否则运维改了配置
        // 没反应，是黑箱。只在「两侧都显式且不等」时打（见纯函数 doc），
        // 相等时不报 —— 那种 warn 迟早被学会忽略，等于没做。
        let ignored_provider_ms =
            ignored_provider_timeout_ms(scene_timeout_ms, provider_timeout_ms);
        if let Some(ignored_ms) = ignored_provider_ms {
            log_warn!(
                ctx,
                "brain_decide",
                timeout_source = timeout_source,
                ignored_provider_timeout_ms = ignored_ms,
                "provider timeout_ms configured but overridden by scene override: provider_id={}",
                provider.id
            );
        }

        // 场景口径：`DecisionRequest.scene` 是唯一能区分「谁问的」的信息，
        // 降级 / 超时日志里必须带上它，否则多场景并存后无法定位是哪一处
        // 在反复失败（agent 侧只看得到「小脑降级」一条）。
        let scenes: Vec<&'static str> = requests.iter().map(|r| r.scene).collect();

        // 当前逐条发送（每场景一个 state）。协议支持一次请求内多题并行，
        // 但不同场景的 state 不同，跨场景合并在协议层没有干净的表示；
        // 同 scene 且 state 相同的合并留作后续优化，此处不为它发明 state 结构。
        let batch = async {
            let mut answers = DecisionAnswers::new();
            let mut usage = CerebellumUsage::default();
            for req in requests {
                // 空题请求必然是装配侧的 bug：服务端会以 400 打回，并因
                // client 的「任一题不合法即整批失败」把同批其它场景一起带走。
                // 这里直接丢弃该条，只坏它自己。
                if req.questions.is_empty() {
                    log_warn!(
                        ctx,
                        "brain_decide",
                        "empty questions in decision request, skipped: scene={}",
                        req.scene
                    );
                    continue;
                }
                let (state, questions) = to_protocol(req);
                // 容量侧自截**只在装配处做一次**：`cerebellum_context::render_within`
                // 按比例表（`share::FRAME_BP` / `share::BODY_BP`）把正文裁到容量以内。
                // 此处曾有一层 `shrink_state_to_capacity` 兜底，但它只认
                // `last_user_message` 裸键，而 7 个接入场景已全部改为 `context` 单键
                // ⇒ 该路径永久空转、从未生效，已删除（生产行为零变化）。
                let call_started = std::time::Instant::now();
                let result = dao
                    .think_fast(ctx.clone(), provider, state, questions)
                    .await?;
                // 端到端耗时：从发起到拿到应答（含建连、TLS、等待与 JSON 解析）。
                // 只有成功路径有值——`?` 之上失败/超时早已传播出 batch，
                // 不会留下半截耗时，对应「调用失败不产生脏计数」。
                let call_duration_ms = call_started.elapsed().as_millis() as u64;
                // 一次真实 HTTP 请求记一条：`?` 之上失败/超时早已传播出 batch，
                // 能走到这里的必然是一次成功调用 —— 失败与降级不产生脏计数。
                // 也正是它把 p95 从「扫思考轨迹反推」搬到了 DuckDB 聚合层。
                let _ = crate::record_event!(
                    ctx,
                    ModelCallEvent::decision(
                        chrono::Utc::now().timestamp_millis(),
                        provider.id.clone(),
                        provider.model_name.clone(),
                        result.usage.input_tokens,
                        result.usage.output_tokens,
                        Some(call_duration_ms),
                    )
                );
                usage.input_tokens += result.usage.input_tokens;
                usage.output_tokens += result.usage.output_tokens;
                for (qid, answer) in result.answers {
                    // qid 命名空间是全局的（`Metrics::decision(qid)` 不带 scene），
                    // 两个场景用了同一个 qid 会静默互相覆盖。覆盖留在明面上。
                    if answers.get(&qid).is_some() {
                        log_warn!(
                            ctx,
                            "brain_decide",
                            "duplicate decision qid across scenes, later one wins: qid={}",
                            qid
                        );
                    }
                    answers = answers.with(qid, to_decision_value(&answer));
                }
            }
            Ok::<_, common::error::Error>((answers, usage))
        };

        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), batch).await {
            Err(_) => {
                log_warn!(
                    ctx,
                    "brain_decide",
                    timeout_source = timeout_source,
                    ignored_provider_timeout_ms = ?ignored_provider_ms,
                    "cerebellum decide timeout: {}ms, scenes={:?}",
                    timeout_ms,
                    scenes
                );
                DecisionOutcome::degraded(DegradedReason::Timeout, elapsed_ms())
            }
            Ok(Err(e)) => {
                log_warn!(
                    ctx,
                    "brain_decide",
                    error = ?e,
                    scenes = ?scenes,
                    "cerebellum decide failed"
                );
                DecisionOutcome::degraded(DegradedReason::CallError, elapsed_ms())
            }
            Ok(Ok((answers, usage))) => DecisionOutcome {
                answers,
                degraded: None,
                latency_ms: elapsed_ms(),
                usage,
            },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::cortex_types::{ChatMessage, ImagePart};

    #[test]
    fn extract_last_user_prompt_multimodal_takes_text_part() {
        let messages = vec![
            ChatMessage::user("第一条"),
            ChatMessage::user_multimodal(
                "看图",
                vec![ImagePart {
                    mime_type: "image/png".to_string(),
                    data_base64: "AAAA".to_string(),
                }],
            ),
        ];
        assert_eq!(extract_last_user_prompt(&messages), "看图");
    }

    #[test]
    fn extract_last_user_prompt_plain_user_unchanged() {
        let messages = vec![ChatMessage::user("纯文本")];
        assert_eq!(extract_last_user_prompt(&messages), "纯文本");
    }

    #[test]
    fn extract_last_user_prompt_empty_when_no_user() {
        let messages = vec![ChatMessage::system("sys")];
        assert_eq!(extract_last_user_prompt(&messages), "");
    }
}

#[cfg(test)]
mod traced_latency_tests {
    use super::*;

    fn outcome(degraded: Option<DegradedReason>, latency_ms: u64) -> DecisionOutcome {
        DecisionOutcome {
            answers: DecisionAnswers::new(),
            degraded,
            latency_ms,
            usage: CerebellumUsage::default(),
        }
    }

    #[test]
    fn no_provider_is_not_a_call_regardless_of_elapsed() {
        // 三个零调用门（provider 未装配 / api_key 空 / DAO 未初始化）统一以
        // `NoProvider` 变体标记 ⇒ **靠变体判**，不靠耗时数值。
        assert_eq!(
            outcome(Some(DegradedReason::NoProvider), 0).traced_latency_ms(),
            None
        );
        assert_eq!(
            outcome(Some(DegradedReason::NoProvider), 3).traced_latency_ms(),
            None,
            "零调用门也返回 elapsed_ms()，所以不能用 latency>0 判「调用过」"
        );
    }

    #[test]
    fn empty_request_gate_is_not_a_call() {
        // 门 1（请求为空）：不设 degraded（不是降级），唯一特征是 latency 固定 0
        assert_eq!(outcome(None, 0).traced_latency_ms(), None);
    }

    #[test]
    fn real_calls_keep_measured_latency_even_when_degraded() {
        // 正常 / 超时 / 调用失败都真实付了时延 ⇒ 即便失败也记实测值
        assert_eq!(outcome(None, 7).traced_latency_ms(), Some(7));
        assert_eq!(
            outcome(Some(DegradedReason::Timeout), 12).traced_latency_ms(),
            Some(12)
        );
        assert_eq!(
            outcome(Some(DegradedReason::CallError), 3).traced_latency_ms(),
            Some(3)
        );
    }
}
