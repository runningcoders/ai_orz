//! Runtime Awakening 具体实现
//!
//! 本文件只保留 `RuntimeAwakening` trait 的四大入口方法：
//! - `wake_agent_brain`：装配 Brain
//! - `awaken`：唤醒并跑完一次思考生命周期（**主体已迁到 `legacy_awaken` 图编排**，
//!   这里只做入参/返回值转换，见方法注释）
//! - `settle_pending`：沉睡沉淀记忆（**主体已迁到 `legacy_settle` 图编排**）
//! - `analyze_input_intent`：意图分析（stats 包裹 + 委托 inner）
//!
//! 共享类型在 `types.rs`，think loop 引擎在 `think_loop.rs`，
//! 意图分析核心 + JSON 解析在 `intent_analyze.rs`，总结退出在 `summary.rs`。

use crate::enrich_ctx;
use crate::models::agent::Agent;
use crate::models::cortex_types::{ImagePart, ToolDescriptor};
use crate::models::events::AgentLoopEvent;
use crate::models::memory::MemoryTrace;
use crate::models::message::Message;
use crate::pkg::agent_runtime_state::{AgentRuntimeStateManager, AgentThinkRuntime};
use crate::pkg::request_context::RequestContext;
use crate::service::dao::memory::{MemoryQuery, MemorySortOrder};
use crate::service::domain::runtime::{
    AwakeningResult, RuntimeAwakening, RuntimeDomain, RuntimeDomainImpl,
};
use common::enums::ThinkingScene;
use common::enums::{MemoryStatus, MemoryType};
use common::error::Result;
use std::sync::Arc;

use super::think_loop::build_policy_for_scene;

// ==================== re-export（保持外部引用路径不变）====================
// pub use 同时完成导入（供本文件内部使用）和 re-export（供外部 awakening::xxx 访问）

pub use super::intent_analyze::{extract_first_json_object, parse_intent_analysis_json};
pub use super::types::{IntentAnalysis, ThinkLoopResult, ThinkingOptions};

// ==================== 场景辅助函数（消除重复逻辑）====================

/// 创建思考运行时并注册到 StateManager，同时构造场景策略组
///
/// 在 awaken / sleep_and_settle / intent_analyze / summary 中重复出现的初始化逻辑：
/// 1. 创建 AgentThinkRuntime（绑定 trace_id）
/// 2. 注册到 AgentRuntimeStateManager（供 cancel-thinking / runtime-status 查询）
/// 3. 按场景构造策略组（用户取消 + 轮次上限 + 超时）
pub(super) fn init_think_runtime_and_policy(
    agent: &Agent,
    scene: ThinkingScene,
    trace_id: &str,
) -> (Arc<AgentThinkRuntime>, Box<dyn crate::pkg::policy::Policy>) {
    let think_runtime = Arc::new(AgentThinkRuntime::new(
        agent.po.id.clone(),
        trace_id.to_string(),
    ));
    AgentRuntimeStateManager::global().set_think_runtime(&agent.po.id, think_runtime.clone());
    let policy = build_policy_for_scene(agent, scene, think_runtime.cancel_flag());
    (think_runtime, policy)
}

/// 按场景过滤工具，构建 ToolDescriptor 列表
///
/// sleep_and_settle / intent_analyze / summary 中重复出现的工具过滤逻辑。
/// awaken 场景不过滤（用全量工具），不调用此函数。
pub(super) fn build_scene_tool_descriptors(
    agent: &Agent,
    scene: ThinkingScene,
) -> Vec<ToolDescriptor> {
    agent
        .tools()
        .iter()
        .filter(|t| {
            let tags = t.po.get_tags();
            scene.is_tool_allowed(&tags)
        })
        .map(ToolDescriptor::from)
        .collect()
}

/// 按场景过滤技能，返回 SkillPo 列表
///
/// sleep_and_settle / intent_analyze / summary 中重复出现的技能过滤逻辑。
/// awaken 场景不过滤技能（全量加载），不调用此函数。
/// 沉淀场景的「近期已沉淀记忆」参考条数
///
/// 只取少量：主要价值是衔接上次沉淀被预算截断的情况，让 Agent 看到「上一段沉淀到哪了」。
/// 完整上下文由 Agent 用 `search_memory` 按需检索，不在这里铺开。
const SETTLED_REFERENCE_LIMIT: usize = 5;

/// 压缩后下一轮补充的「更早的记忆」条数
///
/// 只给少量：主体上下文是【上一轮工作压缩结果】，这里仅作连续性线索，
/// 多给会挤占预算。完整历史由 Agent 用 `search_memory` 按需检索。
const PAST_MEMORIES_LIMIT: usize = 5;

impl RuntimeDomainImpl {
    /// 查询最近若干条短期记忆摘要，作为压缩后下一轮的「更早的记忆」参考
    ///
    /// `exclude_id` 为本次压缩刚落库那条记忆的 id —— 它的内容已经在
    /// 【上一轮工作压缩结果】里，必须排除，否则同一份内容出现两次。
    ///
    /// 查询失败不阻断：参考块是锦上添花，缺了也能正常工作。
    // `pub(crate)`：`legacy_awaken` 编排的 MemoryProvider 复用同一查询口径
    // （压缩后的「更早的记忆」参考块），不能各自再写一份。
    pub(crate) async fn query_past_memories(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        exclude_id: Option<&str>,
    ) -> Vec<String> {
        // 多取一条：被排除的那条大概率是最新的一条，多取一条才能凑够 PAST_MEMORIES_LIMIT
        let result = self
            .memory()
            .query(
                ctx.clone(),
                MemoryQuery {
                    agent_id: Some(agent_id.to_string()),
                    memory_type: Some(MemoryType::ShortTerm),
                    limit: Some(PAST_MEMORIES_LIMIT + 1),
                    order: MemorySortOrder::RecentFirst,
                    ..Default::default()
                },
            )
            .await;

        match result {
            Ok(memories) => memories
                .iter()
                .filter(|m| match (&m.po, exclude_id) {
                    (crate::models::memory::MemoryPo::ShortTerm(st), Some(id)) => st.id != id,
                    _ => true,
                })
                .filter_map(|m| m.to_prompt_summary())
                .take(PAST_MEMORIES_LIMIT)
                .collect(),
            Err(e) => {
                log_warn!(
                    &ctx,
                    "awaken",
                    error = ?e,
                    "查询更早记忆失败，跳过参考区块"
                );
                Vec::new()
            }
        }
    }

    /// 查询最近若干条**已沉淀**短期记忆的摘要，作为沉淀 prompt 的参考线索
    ///
    /// 与 `get_recent_context` 的区别：
    /// - 只取 `status = Settled` 的（已整合进图谱，不是待处理对象）
    /// - 条数远少于历史对话（5 vs 20），避免与【待沉淀的短期记忆】重复占用预算
    ///
    /// 查询失败不阻断沉淀流程——参考块是锦上添花，缺了也能正常工作。
    pub(crate) async fn query_settled_reference(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Vec<String> {
        let result = self
            .memory()
            .query(
                ctx.clone(),
                MemoryQuery {
                    agent_id: Some(agent_id.to_string()),
                    status: Some(MemoryStatus::Settled),
                    memory_type: Some(MemoryType::ShortTerm),
                    limit: Some(SETTLED_REFERENCE_LIMIT),
                    order: MemorySortOrder::RecentFirst,
                    ..Default::default()
                },
            )
            .await;

        match result {
            Ok(memories) => memories
                .iter()
                .filter_map(|m| m.to_prompt_summary())
                .collect(),
            Err(e) => {
                log_warn!(
                    &ctx,
                    "sleep_and_settle",
                    error = ?e,
                    "查询近期已沉淀记忆失败，跳过参考区块"
                );
                Vec::new()
            }
        }
    }
}

pub(super) fn build_scene_skills(
    agent: &Agent,
    scene: ThinkingScene,
) -> Vec<crate::models::skill::SkillPo> {
    agent
        .skills()
        .iter()
        .filter(|s| {
            let tags = s.po.parse_tags();
            scene.is_tool_allowed(&tags)
        })
        .map(|s| s.po.clone())
        .collect()
}

/// 本体词表注入（design §5.4）：awaken / settle / intent_analyze 三场景共用挂载
///
/// 优雅降级：本体子系统未初始化（`try_dal` 为 `None`）或词表查询失败时跳过注入，
/// 可选增强不阻断唤醒主流程；词表为空时 builder 渲染空区块，同样无感。
pub(super) async fn mount_ontology_lexicon(
    ctx: &RequestContext,
    builder: &mut dyn crate::models::prompt_builder::PromptBuilder,
) {
    let Some(ontology) = crate::service::dal::ontology::try_dal() else {
        return;
    };
    match ontology.load_lexicon_summary(ctx.clone()).await {
        Ok(summary) => builder.ontology_lexicon(&summary),
        Err(e) => log_warn!(ctx, "awaken", "加载本体词表失败，跳过词表注入: {:?}", e),
    }
}

/// 当前消息资源预解析（批4 §2.5：文本详情 + vision parts 双产出，一次 DAL 查询）
///
/// 仅处理 `from_role=User` 的当前消息（历史消息零注入 = token 裁剪口径）：
/// - 旁路文件消息：`content=attachment id` + `file_meta`（delivery.rs 落库口径）；
/// - Text/Mixed 协议引用：`[文本](attachment:id)` / `[文本](artifact:id)`（extract_refs）。
///
/// 内置上限：单图 ≤10MB、单条 ≤4 张、mime 白名单 image/*；
/// 超限或非图片 → 降级为文本占位行（不中断）；查询失败 → log_warn 跳过
/// （优雅降级，与 mount_ontology_lexicon 同构：可选增强不阻断唤醒主流程）。
///
/// 返回：(资源上下文文本详情行, 当前消息图像 parts)
pub(super) async fn resolve_current_message_resources(
    ctx: &RequestContext,
    message: &Message,
) -> (Vec<String>, Vec<ImagePart>) {
    let mut lines: Vec<String> = Vec::new();
    let mut images: Vec<ImagePart> = Vec::new();

    // 仅用户发来的当前消息注入（token 裁剪口径：历史/系统消息零图像 part）
    if message.from_role() != common::enums::MessageRole::User {
        return (lines, images);
    }

    let mut att_ids: Vec<String> = Vec::new();
    if message.file_meta().is_some() {
        // 旁路文件消息：content = attachment id
        att_ids.push(message.content().to_string());
    }
    let (ref_atts, ref_arts) = collect_resource_ref_ids(message.content());
    for id in ref_atts {
        if !att_ids.contains(&id) {
            att_ids.push(id);
        }
    }
    if att_ids.is_empty() && ref_arts.is_empty() {
        return (lines, images);
    }

    use base64::Engine as _;
    let attachment_dal = crate::service::dal::attachment::dal();
    let artifact_dal = crate::service::dal::artifact::dal();

    for att_id in &att_ids {
        let att = match attachment_dal.get_by_id(ctx.clone(), att_id).await {
            Ok(Some(att)) => att,
            Ok(None) => {
                lines.push(format!("附件「{att_id}」：未找到（可能已删除）"));
                continue;
            }
            Err(e) => {
                log_warn!(ctx, "awaken", "预解析附件 {att_id} 查询失败，跳过: {e:?}");
                continue;
            }
        };
        let po = &att.po;
        let mime = po.mime_type.clone();
        let size = po.size;
        lines.push(format!(
            "附件「{}」 · {} · {} 字节",
            po.original_name, mime, size
        ));
        if let Err(reason) = image_part_admission(&mime, size, images.len()) {
            lines.push(format!("  （{reason}）"));
            continue;
        }
        match attachment_dal.read_file(&att) {
            Ok(bytes) => {
                images.push(ImagePart {
                    mime_type: mime,
                    data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
                });
            }
            Err(e) => {
                log_warn!(
                    ctx,
                    "awaken",
                    "预解析附件 {att_id} 读取失败，仅保留文本详情: {e:?}"
                );
            }
        }
    }

    for art_id in &ref_arts {
        match artifact_dal.find_by_id(ctx.clone(), art_id).await {
            Ok(Some(art)) => {
                let po = &art.po;
                let raw_desc = po.description.trim();
                let desc: String = if raw_desc.chars().count() > 64 {
                    format!("{}…", raw_desc.chars().take(64).collect::<String>())
                } else {
                    raw_desc.to_string()
                };
                lines.push(format!(
                    "产物「{}」 · {} · {desc}",
                    po.name, po.file_meta.0.mime_type
                ));
            }
            Ok(None) => {
                lines.push(format!("产物「{art_id}」：未找到（可能已删除）"));
            }
            Err(e) => {
                log_warn!(ctx, "awaken", "预解析产物 {art_id} 查询失败，跳过: {e:?}");
            }
        }
    }

    (lines, images)
}

/// 协议引用 id 收集（纯函数，供单测）：attachment/artifact 分桶、去重保序；
/// mention 等其余 scheme 不在资源上下文范围
fn collect_resource_ref_ids(content: &str) -> (Vec<String>, Vec<String>) {
    let mut att_ids: Vec<String> = Vec::new();
    let mut art_ids: Vec<String> = Vec::new();
    for (r, _snapshot) in common::markdown_protocol::extract_refs(content) {
        let scheme = r.scheme.as_str();
        let id = r.id;
        match scheme {
            "attachment" if !att_ids.contains(&id) => att_ids.push(id),
            "artifact" if !art_ids.contains(&id) => art_ids.push(id),
            _ => {}
        }
    }
    (att_ids, art_ids)
}

/// 图像上限校验（纯函数，供单测）：mime 白名单 image/* + 单图 ≤10MB + 单条 ≤4 张；
/// 超限返回占位说明（降级文本，不中断）
fn image_part_admission(
    mime: &str,
    size: i64,
    current_images: usize,
) -> std::result::Result<(), String> {
    const MAX_IMAGE_BYTES: i64 = 10 * 1024 * 1024;
    const MAX_IMAGES_PER_MESSAGE: usize = 4;
    if !mime.starts_with("image/") {
        return Err(format!("mime「{mime}」不在 image/* 白名单，未注入视觉输入"));
    }
    if size > MAX_IMAGE_BYTES {
        return Err(format!(
            "图片 {size} 字节超过单图 {MAX_IMAGE_BYTES} 字节上限，未注入视觉输入"
        ));
    }
    if current_images >= MAX_IMAGES_PER_MESSAGE {
        return Err(format!(
            "图像数量已达单条 {MAX_IMAGES_PER_MESSAGE} 张上限，未注入视觉输入"
        ));
    }
    Ok(())
}

// ==================== RuntimeAwakening trait 实现 ====================

#[async_trait::async_trait]
impl RuntimeAwakening for RuntimeDomainImpl {
    /// 装配 Agent 的 Brain
    ///
    /// 根据 agent.kind 构造对应的 Brain：
    /// - Local: 通过 BrainDal.wake_brain 构造带 Cortex 的 Brain
    /// - External（Cli/Remote）: 构造不带 Cortex 的虚拟 Brain
    ///
    /// 工具不再在此层处理——agent.tools 保留全量工具，
    /// awaken/sleep_and_settle 时按场景过滤构建 ToolDescriptor 列表。
    ///
    /// 返回 enriched ctx：wake_brain 内部查询 ModelProvider 后会补充
    /// `model_provider_id` / `model_name`，调用方应使用返回的 ctx 替换原 ctx，
    /// 保证后续 awaken/think 链路的 ctx 字段完整（避免监控日志缺 model_name）。
    async fn wake_agent_brain(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
    ) -> Result<RequestContext> {
        // 幂等：brain 已装配则直接返回原 ctx（无需再 enrich provider 字段）
        if agent.brain.is_some() {
            return Ok(ctx);
        }

        let ctx = enrich_ctx!(&ctx, &*agent);

        // 工具不再分离：所有工具保留在 agent.tools 中，
        // awaken/sleep_and_settle 时按场景过滤展示，ToolDescriptor 列表在 think 时构建。
        //
        // TODO(brain-cache): 目前每条消息都重新加载 agent 并重建 Brain，
        // 存在性能浪费。若未来引入 brain 缓存，需重新评估 ctx 新鲜度问题。
        let brain = self
            .brain_dal()
            .wake_brain(ctx.clone(), &agent.po, Vec::new())
            .await?;

        // 从 brain.model_provider 提取配置重新 enrich ctx（仅 Local agent 有值），
        // 保证返回的 ctx 含 model_provider_id / model_name（供 awaken 的统计/trace 使用）。
        // 外部 agent（Cli/Remote）无 model_provider，ctx 保持原样。
        let ctx = match brain.model_provider.as_ref() {
            Some(provider) => enrich_ctx!(&ctx, provider),
            None => ctx,
        };

        // 入口留痕：消费侧调用本函数之前打的是 "brain not initialized, auto
        // waking brain"，这行与它成对 —— 两行之间只剩 wake_brain 的两次 DB 读。
        // 「置忙后零进展」类故障里，哪行在场就把冻结窗口对半切开：
        // 只有上行 ⇒ 冻在 wake_brain；两行都在 ⇒ 冻在更下游。
        //
        crate::log_info!(
            &ctx,
            "wake_agent_brain",
            "brain woken, agent_id={}, kind={:?}, model_provider={}",
            agent.po.id,
            brain.kind,
            brain
                .model_provider
                .as_ref()
                .map(|p| p.id.clone())
                .unwrap_or_default(),
        );
        agent.set_brain(brain);

        // 返回 enriched ctx（含 ModelProvider 字段：model_provider_id / model_name）
        Ok(ctx)
    }

    /// 唤醒 Agent：一次完整思考生命周期的执行入口
    ///
    /// ⚠️ 主体在 [`legacy_awaken`] 图编排里 —— 忙碌状态 / Trace / 压缩回环 /
    /// 统计上报 / 异常兜底都在那儿。这里只剩三件事：
    ///   1. 预生成 `MemoryTrace` 拿到 trace_id（压缩 / 总结 / 兜底存档的
    ///      trace_ids 依赖列表都以它为锚点，编排内部不再自己造）；
    ///   2. 把 trait 的借用视角（`&self` / `&Agent` / `&Message`）转成图需要的
    ///      所有权 —— 节点是 `'static` 的 trait object，拿不到借用；
    ///   3. 把执行摘要回填成 [`AwakeningResult`]。
    async fn awaken(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        message: &Message,
        options: &ThinkingOptions,
    ) -> Result<AwakeningResult> {
        use common::enums::MemoryRole;

        // 补充 Agent 上下文到 ctx，后续调用链可复用
        let ctx = enrich_ctx!(&ctx, agent);

        crate::log_info!(
            &ctx,
            "awaken",
            "awaken entered, agent_id={}, message_id={}, max_rounds={:?}, has_project={}, has_task={}, has_profile={}",
            agent.po.id,
            message.po.id,
            options.max_thinking_rounds,
            options.project.is_some(),
            options.task.is_some(),
            options.user_profile.is_some(),
        );

        // 预先构造 MemoryTrace 拿到 trace_id（input / output 由收尾节点回填）
        let trace = MemoryTrace::new(
            agent.po.id.clone(),
            ctx.log_id.clone(),
            ctx.uid(),
            ctx.organization_id.clone().unwrap_or_default(),
            MemoryRole::System,
            String::new(),
            ctx.task_id().cloned(),
        );

        crate::log_info!(
            &ctx,
            "awaken",
            "trace prepared, agent_id={}, trace_id={}",
            agent.po.id,
            trace.id
        );

        let input = super::legacy_awaken::context::AwakenInput {
            // 节点要调 impl 级方法（`run_think_loop` / `query_past_memories`），
            // 必须拿到具体类型；trait 只给了 `&self`，而字段全是 Arc，克隆一份即可。
            //
            // ⚠️ 不能退化成取全局单例：`awaken` 跑的必须是**调用方手上那个实例**
            // 的依赖，否则测试注入的 stub DAL 会被单例顶掉。
            domain: Arc::new(self.clone()),
            ctx: ctx.clone(),
            agent: agent.clone(),
            message: message.clone(),
            options: options.clone(),
            trace,
            shadow: false, // 非影子：该落库落库、该发事件发事件
        };
        let deps = super::legacy_awaken::context::build_deps(input);
        crate::log_info!(
            &ctx,
            "awaken",
            "deps built, agent_id={}, trace_id={}, tools={}, skills={}",
            agent.po.id,
            deps.trace_id,
            agent.tools().len(),
            agent.skills().len(),
        );

        let outcome = super::legacy_awaken::runner::run_legacy_awaken(deps).await;
        match &outcome {
            Ok(o) => crate::log_info!(
                &ctx,
                "awaken",
                "graph finished, agent_id={}, fingerprint={}, steps={}, exit={}",
                agent.po.id,
                o.fingerprint,
                o.steps,
                o.exit
            ),
            Err(e) => crate::log_error!(
                &ctx,
                "awaken",
                "graph failed, agent_id={}, error={}",
                agent.po.id,
                e
            ),
        }
        let outcome = outcome?;

        // 路径指纹：排查「这次为什么走到这条分支」成本最低的线索。
        // 一次唤醒可能经历 prepare#1>deliberate#1>compact#1>prepare#2>…，
        // 单看 exit_reason 看不出回环走了几次。
        crate::log_info!(
            &ctx,
            "awaken",
            "think path finished: {} steps={} exit={}",
            outcome.fingerprint,
            outcome.steps,
            outcome.exit
        );

        Ok(AwakeningResult {
            agent_id: outcome.agent_id,
            trace_ids: outcome.trace_ids,
            raw_input: outcome.raw_input,
            raw_output: outcome.raw_output,
        })
    }

    /// 扫描全部未沉淀短期记忆，按上下文预算分批沉淀（与 awaken 对称）
    ///
    /// awaken 是醒来响应外部消息，本方法是沉睡整理内部记忆。
    /// 主体在 [`legacy_settle`](super::legacy_settle) 图编排里：
    /// `prepare`（取一批 + 装配 Prompt）→ `settle`（思考 + 写 Trace）→ 回环，
    /// 直到没有未沉淀记忆才进 `finalize`。
    ///
    /// # 返回
    /// 本次真正沉淀的条数（仅 `Final` 终态的批次才计入，并被标记为 Settled）。
    /// 溢出 / 超轮次 / 取消的批次一律不标记，记忆保持 Active 交给下一次。
    async fn settle_pending(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        options: &ThinkingOptions,
        settle_limit: usize,
    ) -> Result<usize> {
        // 补充 Agent 上下文到 ctx，后续调用链可复用
        let ctx = enrich_ctx!(&ctx, agent);

        crate::log_info!(
            &ctx,
            "settle_pending",
            "settle entered, agent_id={}, settle_limit={}, has_project={}",
            agent.po.id,
            settle_limit,
            options.project.is_some(),
        );

        let input = super::legacy_settle::context::SettleInput {
            // 节点要调 impl 级方法（`run_think_loop` / `query_settled_reference`），
            // 必须拿到具体类型；trait 只给了 `&self`，字段全是 Arc，克隆一份即可。
            //
            // ⚠️ 不能退化成取全局单例：跑的是**调用方手上那个实例**的依赖，
            // 否则测试注入的 stub DAL 会被单例顶掉。
            domain: Arc::new(self.clone()),
            ctx: ctx.clone(),
            agent: agent.clone(),
            options: options.clone(),
            // 独立沉淀无父 trace；每批自己的 trace 在 prepare 节点里创建
            trace_ids: Vec::new(),
            settle_limit,
        };
        let deps = super::legacy_settle::context::build_deps(input);
        let outcome = super::legacy_settle::runner::run_legacy_settle(deps).await?;

        crate::log_info!(
            &ctx,
            "settle_pending",
            "settle finished, agent_id={}, fingerprint={}, steps={}, batches={}, attempts={}, settled={}, blocked={:?}",
            outcome.agent_id,
            outcome.fingerprint,
            outcome.steps,
            outcome.batches,
            outcome.attempts,
            outcome.settled_count,
            outcome.blocked
        );

        Ok(outcome.settled_count)
    }

    async fn analyze_input_intent(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        message: &Message,
        options: &ThinkingOptions,
    ) -> Result<IntentAnalysis> {
        let start_time = std::time::SystemTime::now();
        let ctx = enrich_ctx!(&ctx, agent);
        let trace_id = format!("intent-analyze-{}", ctx.log_id);

        // 发布循环启动事件（与 awaken/sleep_and_settle 对齐，监控可区分 Phase 1）
        let _ = crate::pkg::aop::publish(
            &ctx,
            AgentLoopEvent::started(&agent.po.id, &trace_id, "intent-analyze", None),
        )
        .await;

        let result = self
            .analyze_input_intent_inner(ctx.clone(), agent, message, options, &trace_id)
            .await;

        let duration_ms = start_time
            .elapsed()
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let status = match &result {
            Ok(_) => "success".to_string(),
            Err(e) => format!("failed: {}", e),
        };

        // 发布循环完成事件（成功/失败均发布，监控可追踪 Phase 1 耗时与状态）
        let _ = crate::pkg::aop::publish(
            &ctx,
            AgentLoopEvent::finished(
                &agent.po.id,
                &trace_id,
                "intent-analyze",
                &status,
                duration_ms,
                None,
            ),
        )
        .await;

        result
    }
}

/// 上报到 `AgentAwakeEvent.exit_reason` 的错误原因最大字符数。
pub(crate) const AGENT_AWAKE_REASON_LIMIT: usize = 200;

/// 消息链上下文：尾部最多保留的近期消息条数（链头永远保留，见 fetch_message_thread）。
const MESSAGE_THREAD_RECENT_KEEP: usize = 6;

/// 消息链上下文：单次查询的最大条数（防止异常超长链拖垮 prompt 组装）。
const MESSAGE_THREAD_QUERY_LIMIT: usize = 60;

/// 拉取当前消息所属消息链的上下文条目（供 PromptBuilder 注入【消息链上下文】区块）。
///
/// 组装策略（用户拍板）：链头消息（root，话题起点）信息量最大，永远保留；
/// 尾部保留最近 [`MESSAGE_THREAD_RECENT_KEEP`] 条（时间正序，紧邻当前消息）；
/// 中间被截断的部分由区块文案引导 Agent 用 list_messages 按 root_id 自行查询。
/// 当前消息自身已由【当前消息】区块承载，这里排除避免重复。
pub(crate) async fn fetch_message_thread(
    ctx: &RequestContext,
    message: &Message,
) -> Result<Vec<String>> {
    use crate::service::dao::message::MessageQuery;

    let Some(root_id) = message.po.root_id.as_deref() else {
        return Ok(Vec::new());
    };

    let query = MessageQuery {
        root_id: Some(root_id.to_string()),
        organization_id: message.po.organization_id.clone(),
        order_by: Some("created_at ASC".to_string()),
        limit: Some(MESSAGE_THREAD_QUERY_LIMIT),
        ..Default::default()
    };
    let chain = crate::service::dal::message::dal()
        .query(ctx.clone(), query)
        .await?;

    // 排除当前消息自身（已由【当前消息】区块承载），排除已撤回的无效消息
    let chain: Vec<_> = chain
        .into_iter()
        .filter(|m| {
            m.po.id != message.po.id && m.po.status != common::enums::MessageStatus::Recalled
        })
        .collect();
    if chain.is_empty() {
        return Ok(Vec::new());
    }

    // 头消息（root）优先；尾部取最近 N 条；链头已落在尾部窗口时去重
    let head = chain.first().cloned();
    let tail: Vec<_> = if chain.len() > MESSAGE_THREAD_RECENT_KEEP {
        chain[chain.len() - MESSAGE_THREAD_RECENT_KEEP..].to_vec()
    } else {
        chain[1..].to_vec()
    };

    let mut items = Vec::with_capacity(tail.len() + 1);
    if let Some(head) = head {
        items.push(head.po.to_prompt());
    }
    for m in tail {
        items.push(m.po.to_prompt());
    }
    Ok(items)
}

/// 截断错误原因文本，避免过长动态文本进入 metric 造成存储膨胀与聚合碎片化。
pub(crate) fn truncate_reason(msg: &str, max_chars: usize) -> String {
    let trimmed = msg.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let taken: String = trimmed.chars().take(max_chars).collect();
    format!("{taken}...")
}
#[cfg(test)]
#[path = "awakening_tests.rs"]
mod tests;
