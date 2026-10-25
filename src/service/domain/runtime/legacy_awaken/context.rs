//! `legacy_awaken` 的上下文供给器 + 运行依赖容器
//!
//! ## ⚠️ 总线的承载边界（实测约束，写在这里避免后来人重踩）
//!
//! `ThinkBus` 只装 `serde_json::Value`，而 `Memory` / `Message` / `Agent` /
//! `Project` / `Task` 这些业务实体**没有实现 Serialize**（只有 `Debug + Clone`）。
//! 所以分工是：
//!
//! - **可无损 JSON 往返的** → 走 `ContextProvider` + 总线：工具描述符、技能、
//!   工作空间路径、近期/更早记忆摘要、资源行、图像 parts、提示词、初始消息序列。
//! - **不可序列化的实体** → 由节点在**构造期注入**依赖（`AwakenDeps`）直接持有，
//!   不进总线。`Vec<Memory>`（短期记忆实体、`builder.history()` 要的就是它）
//!   是唯一必须由 `PrepareNode` 亲自取的数。
//!
//! 这也是「两者轴的交替使用」的一个矛盾点：统一取数的价值在于消除散落查询，
//! 而不是把所有东西都塞进总线。

use crate::models::cortex_types::ToolDescriptor;
use crate::models::memory::MemoryTrace;
use crate::models::skill::SkillPo;
use crate::pkg::agent_runtime_state::AgentThinkRuntime;
use crate::pkg::paths;
use crate::pkg::policy::Policy;
use crate::pkg::request_context::RequestContext;
use crate::pkg::think_path::{
    ContextKey, ContextProvider, LoadDepth, LoadFailureMode, ProviderSet, Stimulus,
};
use crate::service::domain::runtime::RuntimeDomainImpl;
use crate::service::domain::runtime::types::{ThinkLoopProgress, ThinkingOptions};
use crate::{models::agent::Agent, models::message::Message};
use common::error::Result;
use common::error::err;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 跨装配轮次共享的压缩状态
///
/// 压缩的产物要喂给**下一轮** `Prepare`（且下一轮不再查历史记忆），
/// 因此它不是节点局部状态，而是 run 级共享槽位。
#[derive(Debug, Default, Clone)]
pub struct CompactionSlot {
    /// 上一轮工作压缩结果
    pub summary: Option<String>,
    /// 压缩落库的那条记忆 id（组装「更早的记忆」时要排除）
    pub memory_id: Option<String>,
}

/// 检索产物（总线 ContextKey::Knowledge 分区的载荷）
///
/// retrieve 写入完整字段；discover 回环时先写入「新词 + 思考摘要」，
/// 由下一次 retrieve 覆写为完整产物——两写者按节点顺序串行，无并发。
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetrievalPack {
    /// 本次实际用于检索的词（差集后的新词）
    pub query_terms: Vec<String>,
    /// 知识命中摘要行（每条一行，已截断）
    pub knowledge_hits: Vec<String>,
    /// 命中的思维方法技能（category=thinking_method）
    pub method_skills: Vec<SkillPo>,
    /// discover 产出的上一轮思考摘要（首轮为 None）
    pub prior_thinking_summary: Option<String>,
}

/// run 级检索状态：跨节点累积，visited 去重的唯一真相
#[derive(Debug, Default, Clone)]
pub struct RetrievalState {
    /// 已检索过的词（含 intent key_terms 与历次 discover 新词）
    pub queried_terms: Vec<String>,
    /// discover 已执行的循环次数（软预算，蓝图预算是硬护栏）
    pub discovery_rounds: usize,
}

/// discover 节点的模型输出契约
///
/// TODO(Task 6)：`parse_discovery_json` 落地后本类型才有真正的构造方，
/// 到时移除下方的 `expect`（它会在第一个构造方出现时报「期望未兑现」）。
#[expect(dead_code)]
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiscoveryOutcome {
    pub new_key_terms: Vec<String>,
    pub prior_thinking_summary: String,
}

/// 一次 legacy_awaken 的全部运行依赖
///
/// 节点通过持有它拿到「不可进总线」的强类型依赖。它是 Conductor 与节点之间
/// 唯一的旁路通道——**只有无法 JSON 化的东西才放这里**，其余一律走总线。
pub(crate) struct AwakenDeps {
    pub domain: Arc<RuntimeDomainImpl>,
    /// LLM 触碰点（思考 / 压缩）的可替换后端
    ///
    /// 生产注入 `LegacyBackend`；集成测试替换为脚本后端，从而在**不打模型**
    /// 的前提下跑完真实流转。也是后续下沉 `run_think_loop` 的替换点。
    pub backend: Arc<dyn super::backend::ThinkBackend>,
    pub ctx: RequestContext,
    pub agent: Arc<Agent>,
    pub message: Arc<Message>,
    pub options: Arc<ThinkingOptions>,
    pub trace_id: String,
    pub think_runtime: Arc<AgentThinkRuntime>,
    pub policy: Arc<dyn Policy>,
    pub progress: ThinkLoopProgress,
    pub trace: Mutex<MemoryTrace>,
    pub compaction: Mutex<CompactionSlot>,
    /// run 级检索状态：visited 词集合 + discover 轮次软预算（见 [`RetrievalState`]）
    pub retrieval: Mutex<RetrievalState>,
    /// 影子模式：跑完整链路但**不落库、不发通知**，只够成「有机质春雨对比」。
    pub shadow: bool,
    pub started_at: Instant,
}

impl std::fmt::Debug for AwakenDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwakenDeps")
            .field("agent_id", &self.agent.po.id)
            .field("trace_id", &self.trace_id)
            .field("shadow", &self.shadow)
            .finish()
    }
}

impl AwakenDeps {
    /// 把入站消息包成 run 的刺激（零 I/O）
    pub fn stimulus(&self) -> Stimulus {
        Stimulus {
            source: format!("{:?}", self.message.from_role()),
            text: self.message.content().to_string(),
            message_id: Some(self.message.po.id.clone()),
        }
    }
}

/// 工作空间路径（6 个）——原 `awakening.rs#L588-641` 的碎片
///
/// 只持有「拼路径需要的三样东西」而不是整个 `AwakenDeps`：
/// `legacy_settle` 没有 `Message`，也要复用同一份路径口径，不能为此各写一份。
#[derive(Debug)]
pub struct WorkspaceProvider {
    ctx: RequestContext,
    agent_id: String,
    project_id: Option<String>,
}

impl WorkspaceProvider {
    pub fn new(
        ctx: RequestContext,
        agent_id: impl Into<String>,
        project_id: Option<String>,
    ) -> Self {
        Self {
            ctx,
            agent_id: agent_id.into(),
            project_id,
        }
    }
}

#[async_trait::async_trait]
impl ContextProvider for WorkspaceProvider {
    fn key(&self) -> ContextKey {
        ContextKey::Runtime
    }

    async fn load(&self, _stimulus: &Stimulus, _depth: LoadDepth) -> Result<Value> {
        let base = crate::config::get().base_data_path();
        let uid = self.ctx.uid();
        let uid_ref = if uid.is_empty() {
            None
        } else {
            Some(uid.as_str())
        };
        let agent_id = &self.agent_id;
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
        let project_workspace = match (&self.project_id, !uid.is_empty()) {
            (Some(project_id), true) => Some(
                paths::user_project_workspace(&base, &uid, project_id)
                    .to_string_lossy()
                    .to_string(),
            ),
            _ => None,
        };
        Ok(json!({
            "default": default_workspace,
            "user_home": user_home,
            "user_shared": user_shared_workspace,
            "user_agent": user_agent_workspace,
            "agent": agent_workspace,
            "project": project_workspace,
        }))
    }
}

/// 工具描述符（OpenAI function calling 协议，可 JSON 往返）
#[derive(Debug)]
pub struct ToolsProvider {
    tools: Vec<ToolDescriptor>,
}

impl ToolsProvider {
    pub fn new(tools: Vec<ToolDescriptor>) -> Self {
        Self { tools }
    }
}

#[async_trait::async_trait]
impl ContextProvider for ToolsProvider {
    fn key(&self) -> ContextKey {
        ContextKey::Tools
    }

    async fn load(&self, _stimulus: &Stimulus, _depth: LoadDepth) -> Result<Value> {
        serde_json::to_value(&self.tools)
            .map_err(|e| err!(Internal, "serialize tool descriptors failed: {e}"))
    }
}

/// 已安装技能副本（装了就进 Prompt，无 tag 匹配）
#[derive(Debug)]
pub struct SkillsProvider {
    skills: Vec<SkillPo>,
}

impl SkillsProvider {
    pub fn new(skills: Vec<SkillPo>) -> Self {
        Self { skills }
    }
}

#[async_trait::async_trait]
impl ContextProvider for SkillsProvider {
    fn key(&self) -> ContextKey {
        ContextKey::Skills
    }

    async fn load(&self, _stimulus: &Stimulus, _depth: LoadDepth) -> Result<Value> {
        serde_json::to_value(&self.skills)
            .map_err(|e| err!(Internal, "serialize skills failed: {e}"))
    }
}

/// 业务上下文：消息链 + 当前消息资源/视像 + 更早记忆参考
///
/// 三块都是「一次 DAL 查询的产出」，高级降级；任一块失败不影响唤醒主流程。
#[derive(Debug)]
pub struct BusinessProvider {
    deps: Arc<AwakenDeps>,
}

impl BusinessProvider {
    pub fn new(deps: Arc<AwakenDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait::async_trait]
impl ContextProvider for BusinessProvider {
    fn key(&self) -> ContextKey {
        ContextKey::Business
    }

    /// 取数失败不阻断：这些区块锦上添花，缺了也能正常工作
    fn on_failure(&self) -> LoadFailureMode {
        LoadFailureMode::Degrade
    }

    async fn load(&self, _stimulus: &Stimulus, _depth: LoadDepth) -> Result<Value> {
        let deps = &self.deps;
        let ctx = deps.ctx.clone();

        crate::log_info!(
            &ctx,
            "legacy_awaken",
            "business provider load start, agent_id={}, message_id={}",
            deps.agent.po.id,
            deps.message.po.id
        );

        let thread = match super::super::awakening::fetch_message_thread(&ctx, &deps.message).await
        {
            Ok(items) => items,
            Err(e) => {
                crate::log_warn!(
                    &ctx,
                    "legacy_awaken",
                    "fetch message thread failed (message_id={}): {:?}",
                    deps.message.po.id,
                    e
                );
                Vec::new()
            }
        };

        crate::log_info!(
            &ctx,
            "legacy_awaken",
            "message thread loaded, agent_id={}, n={}",
            deps.agent.po.id,
            thread.len()
        );

        let (resource_lines, vision_parts) =
            super::super::awakening::resolve_current_message_resources(&ctx, &deps.message).await;

        crate::log_info!(
            &ctx,
            "legacy_awaken",
            "current message resources resolved, agent_id={}, resource_lines={}, vision_parts={}",
            deps.agent.po.id,
            resource_lines.len(),
            vision_parts.len()
        );

        // 更早记忆参考：仅在已压缩时使用（压缩成果里没有的延续性线索）
        // ⚠️ `MutexGuard` 不能跨 await（future 会丢 Send）：先取快照再 drop
        let compacted = deps
            .compaction
            .lock()
            .map(|s| (s.summary.is_some(), s.memory_id.clone()))
            .unwrap_or((false, None));
        let past_memories = if compacted.0 {
            deps.domain
                .query_past_memories(ctx.clone(), &deps.agent.po.id, compacted.1.as_deref())
                .await
        } else {
            Vec::new()
        };

        Ok(json!({
            "message_thread": thread,
            "resource_lines": resource_lines,
            "vision_parts": vision_parts,
            "past_memories": past_memories,
        }))
    }
}

/// 一次 legacy_awaken 的运行入参
///
/// 直接塞进函数签名会超参数个数阈值，但那不是主要动机 —— 真正的好处是
/// 「调用方必须一次把唤醒所需的上下文备齐」：少一个字段就编译不过，
/// 而不是靠八个参数的顺序去猜哪个是 ctx 哪个是 trace。
pub(crate) struct AwakenInput {
    /// 发起唤醒的那个 domain 实例（**不取全局单例**，见 `build_deps` 注释）
    pub domain: Arc<RuntimeDomainImpl>,
    pub ctx: RequestContext,
    pub agent: Agent,
    pub message: Message,
    pub options: ThinkingOptions,
    /// 预生成的 Trace（只为拿到 trace_id，内容由收尾节点回填）
    pub trace: MemoryTrace,
    pub shadow: bool,
}

/// 装配一次 legacy_awaken 的运行依赖
///
/// `AwakenInput.domain` 由调用方显式传入，**不取全局单例**：
/// `AwakenDeps.domain` 必须就是发起唤醒的那个 domain 实例，否则测试注入的
/// stub DAL（以及未来的多实例场景）会被单例顶掉，链路悄悄跑到别的依赖上。
/// trait 方法只拿到 `&self`，所以那里要的是**被再包一层 Arc 的克隆**
///（`RuntimeDomainImpl` 全 Arc 字段，`Clone` 很廉价）。
pub(crate) fn build_deps(input: AwakenInput) -> Arc<AwakenDeps> {
    build_deps_with_backend(Arc::new(super::backend::LegacyBackend), input)
}

/// 同上，但 LLM 后端可由调用方指定
///
/// 集成测试用它注入脚本后端：**在不打模型的前提下**跑完真实装配 + 真实收尾。
/// 若直接裸调 `domain`，测试是否通过就取决于本机能否连上 OpenAI —— 那是假绿。
pub(crate) fn build_deps_with_backend(
    backend: Arc<dyn super::backend::ThinkBackend>,
    input: AwakenInput,
) -> Arc<AwakenDeps> {
    let AwakenInput {
        domain,
        ctx,
        agent,
        message,
        options,
        trace,
        shadow,
    } = input;
    let trace_id = trace.id.clone();
    let (think_runtime, policy) = super::super::awakening::init_think_runtime_and_policy(
        &agent,
        common::enums::ThinkingScene::Awaken,
        &trace_id,
    );
    Arc::new(AwakenDeps {
        domain,
        backend,
        ctx,
        agent: Arc::new(agent),
        message: Arc::new(message),
        options: Arc::new(options),
        trace_id,
        think_runtime,
        policy,
        progress: ThinkLoopProgress::new(),
        trace: Mutex::new(trace),
        compaction: Mutex::new(CompactionSlot::default()),
        retrieval: Mutex::new(RetrievalState::default()),
        shadow,
        started_at: Instant::now(),
    })
}

/// 装配一条 legacy_awaken 的上下文供给器集合
///
/// 注意 **没有 Memory provider**：`Vec<Memory>` 不可序列化（见文件头说明），
/// 历史记忆由 `PrepareNode` 亲自取。
pub fn build_providers(deps: Arc<AwakenDeps>) -> ProviderSet {
    let tools: Vec<ToolDescriptor> = deps
        .agent
        .tools()
        .iter()
        .map(crate::models::cortex_types::ToolDescriptor::from)
        .collect();
    let skills: Vec<SkillPo> = deps.agent.skills().iter().map(|s| s.po.clone()).collect();

    let workspace = WorkspaceProvider::new(
        deps.ctx.clone(),
        deps.agent.po.id.clone(),
        deps.options.project.as_ref().map(|p| p.po.id.clone()),
    );
    ProviderSet::new()
        .register(Arc::new(workspace))
        .register(Arc::new(BusinessProvider::new(Arc::clone(&deps))))
        .register(Arc::new(ToolsProvider::new(tools)))
        .register(Arc::new(SkillsProvider::new(skills)))
}

/// 从总线读可序列化分区的统一报错
pub fn read_bus<T: serde::de::DeserializeOwned>(
    bus: &crate::pkg::think_path::ThinkBus,
    key: ContextKey,
) -> Result<T> {
    let value = bus.get(key).cloned().unwrap_or(Value::Null);
    serde_json::from_value(value)
        .map_err(|e| err!(Internal, "decode bus key {} failed: {e}", key.as_str()))
}
