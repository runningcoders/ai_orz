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
use crate::models::memory::{Memory, MemoryIndexPlan, MemoryTrace};
use crate::models::message::Message;
use crate::models::prompt_builder::PromptBuilder;
use crate::models::skill::SkillPo;
use crate::pkg::agent_runtime_state::{AgentRuntimeStateManager, AgentThinkRuntime};
use crate::pkg::paths;
use crate::pkg::request_context::RequestContext;
use crate::service::dao::memory::{MemoryQuery, MemorySortOrder};
use crate::service::domain::runtime::{
    AwakeningResult, RuntimeAwakening, RuntimeDomain, RuntimeDomainImpl,
};
use common::enums::ThinkingScene;
use common::enums::{MemoryStatus, MemoryType};
use common::error::Result;
use std::sync::Arc;

use super::think_loop::{build_policy_for_scene, memory_index_budget_for};

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
) -> (Arc<AgentThinkRuntime>, Arc<dyn crate::pkg::policy::Policy>) {
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
/// sleep_and_settle / intent_analyze / summary / awaken 共用的工具过滤逻辑。
/// awaken 场景**绑定即权限**，绑定到该 Agent 的工具全量注入（见
/// `ThinkingScene::is_tool_allowed`）；其余内部循环场景仍按 tag 白名单收敛，
/// 作为无人值守时的安全护栏。
pub(super) fn build_scene_tool_descriptors(
    agent: &Agent,
    scene: ThinkingScene,
) -> Vec<ToolDescriptor> {
    agent
        .tools()
        .iter()
        .filter(|t| scene.is_tool_allowed(&t.po.get_tags()))
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
    /// - 只取 5 条（历史对话索引块按字符预算加载，条数不固定），避免与
    ///   【待沉淀的短期记忆】重复占用预算
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

    let att_ids = collect_current_message_attachment_ids(message);
    let (_, ref_arts) = collect_resource_ref_ids(message.content());
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
        // 生效 MIME（泛型按文件名补齐）：决定了能否进 `image/*` 视觉白名单，
        // 历史 octet-stream 附件靠这里自愈，不必重传。
        let mime = po.effective_mime_type();
        let size = po.size;
        lines.push(attachment_resource_line(po, att_id));
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
                let desc = prompt_excerpt(&po.description, 64);
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

/// 当前消息引用的附件 ID（纯收集，不做存在性解析）：
/// 按正文协议引用顺序去重收集；历史存量兼容见下。
///
/// **正文引用是唯一事实源**：所有新写入路径（站内发送、飞书富文本、微信图片）
/// 都把附件以 `[文件名](attachment:id)` 并进正文，消息上不再有独立附件消息，
/// 也没有专用附件字段——需要「附件 ↔ 消息」关联时从正文解析。
///
/// `file_type` 分支只用于**历史存量**：早年附件是独立消息（`content` = 附件 id、
/// `file_type` 标记类型），这类消息可能因重投被再次唤醒。新消息 `file_type` 恒为
/// `None`（混排消息由正文表达附件），不会进入该分支。
///
/// `pub(crate)`：投递侧（`message::delivery`）用单测盯住「请求侧拼装改口径后
/// 附件仍能被唤醒侧收集到」，这是「附件有没有进模型」的唯一入口。
pub(crate) fn collect_current_message_attachment_ids(message: &Message) -> Vec<String> {
    let mut att_ids: Vec<String> = Vec::new();
    if message.file_meta().is_some() {
        att_ids.push(message.content().to_string());
    }
    let (ref_atts, _) = collect_resource_ref_ids(message.content());
    for id in ref_atts {
        if !att_ids.contains(&id) {
            att_ids.push(id);
        }
    }
    att_ids
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

/// 附件资源行：名称 + id + mime + 大小；已写入摘要时附带截断预览，
/// 让模型能直接判断是否需要重新识别/覆盖，无需额外查询。
fn attachment_resource_line(po: &crate::models::attachment::AttachmentPo, att_id: &str) -> String {
    let mut line = format!(
        "附件「{}」 (id: {att_id}) · {} · {} 字节",
        po.original_name,
        po.effective_mime_type(),
        po.size
    );
    let description = po.description.trim();
    if !description.is_empty() {
        line.push_str(&format!(" · 摘要：{}", prompt_excerpt(description, 64)));
    }
    line
}

/// 资源行摘录：trim 后超长截断并加省略号（附件与产物行统一口径）
fn prompt_excerpt(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() > max_chars {
        format!("{}…", trimmed.chars().take(max_chars).collect::<String>())
    } else {
        trimmed.to_string()
    }
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
            injected_ids: outcome.injected_ids,
            cancelled: outcome.exit == super::legacy_awaken::nodes::EXIT_CANCELLED,
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
            .analyze_input_intent_inner(
                ctx.clone(),
                agent,
                message,
                options,
                &trace_id,
                super::intent_analyze::IntentPhaseInput {
                    revision: None,
                    seed: None,
                },
            )
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

/// 工作空间路径的唯一计算入口（意图识别与正式装配共用，保证 base 逐字节一致）
///
/// 返回 `(default, user_home, user_shared, user_agent, agent, project)`，
/// 与 `PromptBuilder::workspace_context` 的入参顺序一致。
pub(crate) fn workspace_path_args(
    ctx: &RequestContext,
    agent_id: &str,
    project_id: Option<&str>,
) -> (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let base = crate::config::get().base_data_path();
    let uid = ctx.uid();
    let uid_ref = if uid.is_empty() {
        None
    } else {
        Some(uid.as_str())
    };
    let default_workspace = paths::default_workspace(&base, uid_ref, Some(agent_id))
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
            paths::user_agent_workspace(&base, &uid, agent_id)
                .to_string_lossy()
                .to_string(),
        )
    };
    let agent_workspace = Some(
        paths::agent_workspace(&base, agent_id)
            .to_string_lossy()
            .to_string(),
    );
    let project_workspace = match (project_id, !uid.is_empty()) {
        (Some(project_id), true) => Some(
            paths::user_project_workspace(&base, &uid, project_id)
                .to_string_lossy()
                .to_string(),
        ),
        _ => None,
    };
    (
        default_workspace,
        user_home,
        user_shared_workspace,
        user_agent_workspace,
        agent_workspace,
        project_workspace,
    )
}

/// 两阶段共享 base 的装配入参
///
/// 意图识别与正式装配都必须只通过 [`fill_prompt_base`] 挂载这段内容：
/// 两边的字段集合、顺序、渲染路径相同，才能保证 Phase 2 的请求前缀
/// 与 Phase 1 初始 Prompt 逐字节一致（prompt cache 才可能命中）。
pub(crate) struct PromptBaseInput<'a> {
    pub ctx: &'a RequestContext,
    pub agent: &'a Agent,
    pub message: &'a Message,
    pub options: &'a ThinkingOptions,
    /// 两阶段都用主 trace_id；`intent-{trace}` 只用于事件子轨迹
    pub trace_id: &'a str,
    pub recent_memories: &'a [Memory],
    /// 全量已安装技能（两阶段完全一致，不按场景裁剪）
    pub skills: &'a [SkillPo],
    /// 压缩摘要（仅在压缩回环后由正式装配传入）
    pub compacted: Option<&'a str>,
    /// 更早记忆参考（仅在已压缩时由正式装配传入）
    pub past_memories: &'a [String],
    pub message_thread: &'a [String],
    pub resource_lines: &'a [String],
    pub vision_parts: &'a [ImagePart],
}

/// 唯一 base 装配：System（人设 + 技能 + 最终回复规范）+ 公共上下文 +
/// 历史/压缩 + 消息链 + 资源/视觉 + trace + 当前消息
///
/// ⚠️ 这里**不得**出现意图分析结果、检索知识、思维方法或执行指令：
/// 那些属于「意图之后才确定」的追加内容，必须留在意图尾部或执行段。
pub(crate) async fn fill_prompt_base(builder: &mut dyn PromptBuilder, input: PromptBaseInput<'_>) {
    builder.current_trace_id(input.trace_id);
    builder.system_prompt(input.agent);
    builder.skills(input.skills);
    let (default, user_home, user_shared, user_agent, agent, project) = workspace_path_args(
        input.ctx,
        &input.agent.po.id,
        input.options.project.as_ref().map(|p| p.po.id.as_str()),
    );
    builder.workspace_context(default, user_home, user_shared, user_agent, agent, project);
    if let Some(project) = &input.options.project {
        builder.project_context(project);
    }
    if let Some(task) = &input.options.task {
        builder.task_context(task);
    }
    if let Some(user) = &input.options.user_profile {
        builder.user_profile(user);
    }
    // 本体词表（记忆图谱用词约定），三场景共用挂载点
    mount_ontology_lexicon(input.ctx, builder).await;
    // 索引计划在这里算（唯一 base 装配入口）：预算 = Agent 压缩阈值 × 10%，
    // 逐行成本复用总结阶段的 `estimate_tokens`，T0 取入站消息时间 —— 两阶段同源。
    // builder 只消费档位，不参与分档判断。
    builder.set_memory_index_plan(MemoryIndexPlan::build(
        input.recent_memories,
        Some(input.message.po.created_at),
        memory_index_budget_for(input.agent),
        super::compaction::estimate_tokens,
    ));
    builder.history(input.recent_memories);
    if let Some(compacted) = input.compacted {
        builder.compacted_context(compacted);
    }
    if !input.past_memories.is_empty() {
        builder.past_memories_reference(input.past_memories);
    }
    if !input.resource_lines.is_empty() {
        builder.set_resource_context_lines(input.resource_lines.to_vec());
    }
    // 本轮动态指引（回复仲裁 + 协作压力）独立成 append-only 尾部区块：不能
    // 并入【资源上下文】，更不能插到消息链 / 历史等稳定区块之前（会作废其后
    // 全部 prefix cache）。两阶段共用本函数装配 base，指引行因此在 Phase1/Phase2
    // 间天然一致。
    if !input.options.turn_guidance_lines.is_empty() {
        builder.set_turn_guidance_lines(input.options.turn_guidance_lines.clone());
    }
    if !input.vision_parts.is_empty() {
        builder.set_current_message_vision(input.vision_parts.to_vec());
    }
    builder.current_message(input.message);
    builder.message_thread(input.message_thread);
}

/// 当前消息业务上下文的唯一取数入口：消息链 + 资源行 + 视觉 parts
///
/// 意图识别与正式装配都从这里取同一份数据，避免两阶段各查一次、结果漂移。
/// 三块都是可选增强：查询失败只记日志并降级为空，不阻断唤醒主流程。
pub(crate) async fn fetch_message_business_blocks(
    ctx: &RequestContext,
    message: &Message,
) -> (Vec<String>, Vec<String>, Vec<ImagePart>) {
    let thread = match fetch_message_thread(ctx, message).await {
        Ok(items) => items,
        Err(e) => {
            log_warn!(
                ctx,
                "awaken",
                "fetch message thread failed (message_id={}): {:?}",
                message.po.id,
                e
            );
            Vec::new()
        }
    };
    let (mut resource_lines, vision_parts) = resolve_current_message_resources(ctx, message).await;
    append_attachment_description_guidance(message, &mut resource_lines);
    (thread, resource_lines, vision_parts)
}

/// 当前消息携带附件时，向资源上下文追加「写摘要」引导行
///
/// 意图分析阶段与正式装配共用同一份资源行，因此引导也必须由这里统一挂载。
fn append_attachment_description_guidance(message: &Message, lines: &mut Vec<String>) {
    if message.from_role() != common::enums::MessageRole::User {
        return;
    }
    if collect_current_message_attachment_ids(message).is_empty() {
        return;
    }

    lines.push(format!(
        "附件摘要要求：资源行已带「摘要」的附件**默认直接复用**，不必重复识别；但如果结合当前用户文字或上下文判断摘要可能失准（与附件实际内容不符，或用途已随本次意图变化），应重新识别并调用工具 `save_attachment_description` 覆盖。没有摘要的附件请在识别后调用该工具写入——`attachment_id` 取资源行括号内的附件 id；`content` 写客观内容摘要，`usage` 写结合当前用户文字推断的用途，`message_id` 填当前消息 ID `{}`。调用再次发生即覆盖旧摘要；需要完整摘要或用途时用 `get_attachment` 查询（`get_attachment_content` 取文件内容）。",
        message.po.id
    ));
}

/// 消息链上下文：尾部最多保留的近期消息条数（链头永远保留，见 fetch_message_thread）。
const MESSAGE_THREAD_RECENT_KEEP: usize = 6;

/// 尾部窗口每页扫描条数：比 K 多 1，给当前消息 / 被过滤项留出推进空间。
///
/// 单页不足 K 条时继续按 `offset` 向更早翻页，直到凑满 K 条或链已耗尽。
const MESSAGE_THREAD_SCAN_PAGE: usize = MESSAGE_THREAD_RECENT_KEEP + 1;

/// 拉取当前消息所属消息链的上下文条目（供 PromptBuilder 注入【消息链上下文】区块）。
///
/// 组装策略：链头消息（root，话题起点）信息量最大，永远保留；
/// 尾部保留最近 [`MESSAGE_THREAD_RECENT_KEEP`] 条（时间正序，紧邻当前消息）；
/// 中间被截断的部分由区块文案引导 Agent 用 list_messages 按 root_id 自行查询。
/// 当前消息自身已由【当前消息】区块承载，这里排除避免重复。
///
/// ⚠️ 两个必须同时成立的口径（否则「最近」会静默退化成「最早」）：
/// 1. **锚点上界**：只取 `created_at <= 当前消息` 的链内消息，晚于锚点的消息
///    属于本轮新输入，只能由收件箱走执行段追加，不能混进 base 前缀；
/// 2. **倒序取 K 再反转**：旧实现 `created_at ASC + LIMIT 60` 拿到的是**最早**
///    60 条，长链下尾部 6 条根本不是最近 6 条。
pub(crate) async fn fetch_message_thread(
    ctx: &RequestContext,
    message: &Message,
) -> Result<Vec<String>> {
    use crate::service::dao::message::MessageQuery;

    let Some(root_id) = message.po.root_id.as_deref() else {
        return Ok(Vec::new());
    };
    let dal = crate::service::dal::message::dal();

    // 链头单独按 id 取：稳定、且不会被尾部窗口挤掉
    let head = if root_id == message.po.id {
        None
    } else {
        dal.query(
            ctx.clone(),
            MessageQuery {
                id: Some(root_id.to_string()),
                organization_id: message.po.organization_id.clone(),
                ..Default::default()
            },
        )
        .await?
        .into_iter()
        .next()
    };

    // `created_before` 是开区间：`created_at + 1` 把锚点同毫秒消息也纳入，
    // 同毫秒的先后再由 id 在 `select_thread_items` 里裁定。
    //
    // 不能只取一页 K+1：链内可能夹着当前消息、同毫秒且 id 更大的消息或 Recalled，
    // 这些都会被过滤掉。这里按 offset 向更早翻页，直到凑满 K 条或链已耗尽，
    // 保证「最近 K 条」是真正的最近 K 条有效消息。
    let mut recent_desc: Vec<crate::models::message::MessagePo> =
        Vec::with_capacity(MESSAGE_THREAD_RECENT_KEEP);
    let mut offset = 0usize;
    loop {
        let batch = dal
            .query(
                ctx.clone(),
                MessageQuery {
                    root_id: Some(root_id.to_string()),
                    organization_id: message.po.organization_id.clone(),
                    created_before: Some(message.po.created_at.saturating_add(1)),
                    // 同毫秒用 id 兜底排序：反向 + 再在内存反转后正好是 (created_at, id) 正序，
                    // 与 `select_thread_items` 的过滤裁定口径一致。
                    order_by: Some("created_at DESC, id DESC".to_string()),
                    limit: Some(MESSAGE_THREAD_SCAN_PAGE),
                    offset: Some(offset),
                    ..Default::default()
                },
            )
            .await?;
        let batch_len = batch.len();
        for m in batch {
            if thread_message_precedes_anchor(&m.po, &message.po) {
                recent_desc.push(m.po);
                if recent_desc.len() >= MESSAGE_THREAD_RECENT_KEEP {
                    break;
                }
            }
        }
        if recent_desc.len() >= MESSAGE_THREAD_RECENT_KEEP || batch_len < MESSAGE_THREAD_SCAN_PAGE {
            break;
        }
        offset += batch_len;
    }

    Ok(select_thread_items(
        head.map(|m| m.po),
        recent_desc,
        &message.po,
    ))
}

/// 纯函数：从「链头 + 倒序尾部窗口」选出最终注入的消息（时间正序）。
///
/// 单独抽出来是为了能在不打 DB 的前提下验证两个边界：
/// 锚点之后（同毫秒按 id）的消息必须被剔除；超长链保留的是**最近** K 条。
fn select_thread_items(
    head: Option<crate::models::message::MessagePo>,
    recent_desc: Vec<crate::models::message::MessagePo>,
    anchor: &crate::models::message::MessagePo,
) -> Vec<String> {
    let mut recent: Vec<_> = recent_desc
        .into_iter()
        .rev()
        .filter(|m| thread_message_precedes_anchor(m, anchor))
        .collect();
    if recent.len() > MESSAGE_THREAD_RECENT_KEEP {
        recent.drain(..recent.len() - MESSAGE_THREAD_RECENT_KEEP);
    }

    let mut items = Vec::with_capacity(recent.len() + 1);
    let mut seen = std::collections::HashSet::new();
    if let Some(head) =
        head.filter(|h| h.id != anchor.id && h.status != common::enums::MessageStatus::Recalled)
    {
        seen.insert(head.id.clone());
        items.push(head.to_prompt());
    }
    for m in recent {
        if seen.insert(m.id.clone()) {
            items.push(m.to_prompt());
        }
    }
    items
}

/// 链内消息是否位于锚点之前（含同毫秒但 id 更小者）
fn thread_message_precedes_anchor(
    m: &crate::models::message::MessagePo,
    anchor: &crate::models::message::MessagePo,
) -> bool {
    if m.id == anchor.id || m.status == common::enums::MessageStatus::Recalled {
        return false;
    }
    (m.created_at, m.id.as_str()) < (anchor.created_at, anchor.id.as_str())
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
