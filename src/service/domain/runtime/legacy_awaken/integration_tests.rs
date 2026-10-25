//! `legacy_awaken` 的集成测试：**真实装配 + 真实收尾**，只替换 LLM 触碰点
//!
//! 与 `tests.rs` 的分工：
//!
//! | 文件 | 测什么 |
//! |------|--------|
//! | `tests.rs` | 图**长什么样**：拓扑、预算、Signal ↔ 边的双向一致 |
//! | 本文件 | 图**跑起来怎样**：真 `ContextProvider` 装配 + 真 `Consolidate` 收尾 |
//!
//! ⚠️⚠️ LLM 触碰点一律走 `ScriptedBackend`（见 `backend.rs`），**绝不裸调 `domain`**。
//! 红线：集成测试报 `connect error ... api.openai.com` 默认当真 bug —— 若这里
//! 直接调 `run_think_loop`，测试是否通过就取决于本机能不能连上 OpenAI，
//! 那就是把「模型可用」当成了「链路正确」，是典型的假绿。
//!
//! 编排层之所以要 `backend` 这个缝，价值正在于此：**用退出信号驱动流转**，
//! 而不是靠构造特定的 LLM 响应去「诱使」链路走到某个分支。
//!
//! ⚠️ 蓝图已扩成完整思考图（entry=`intent`）：`intent` / `retrieve` 已是真正实现
//! （脚本后端 canned 返回空意图 → 检索短路，行为与骨架期等价）；
//! `discover` 尚是直通骨架（真正实现由后续任务替换）——max_rounds 路径上的
//! `discover#1` 仍是骨架占位。

use super::backend::{CompactInput, DiscoverInput, IntentInput, ThinkBackend, ThinkInput};
use super::blueprint::MAX_LOOP_ROUNDS;
use super::context::{AwakenDeps, RetrievalPack, build_deps_with_backend, read_bus};
use super::nodes::{EXIT_CANCELLED, EXIT_FINAL, EXIT_MAX_ROUNDS, NODE_CONSOLIDATE};
use super::runner::run_legacy_awaken;
use crate::models::agent::{Agent, AgentPo, AgentRuntimeConfig};
use crate::models::brain::Brain;
use crate::models::cortex_types::ChatMessage;
use crate::models::file::FileMeta;
use crate::models::memory::MemoryTrace;
use crate::models::message::Message;
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::think_path::{ContextKey, LoadDepth, NodeSignal, ThinkBus, ThinkNode};
use crate::service::domain::runtime::compaction::CompactOutcome;
use crate::service::domain::runtime::types::{IntentAnalysis, ThinkLoopResult, ThinkingOptions};
use async_trait::async_trait;
use common::enums::{AgentStatus, MessageRole, MessageType, ModelCapability, ProviderType};
use common::error::{Result, err};
use sqlx::SqlitePool;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// ==================== 脚本化的 LLM 后端 ====================

/// 按队列返回预设结果；队列耗尽视为**测试写错**（而不是链路的错）
///
/// ⚠️ `Debug` 是手写而非 derive：`CompactOutcome` 没实现 `Debug`，
/// 而 `ThinkBackend: Debug` 是这个 trait 的硬要求（诊断用）。
struct ScriptedBackend {
    thinks: Mutex<VecDeque<ThinkLoopResult>>,
    compacts: Mutex<VecDeque<CompactOutcome>>,
    pub think_calls: AtomicUsize,
    pub compact_calls: AtomicUsize,
}

impl std::fmt::Debug for ScriptedBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptedBackend")
            .field(
                "thinks_left",
                &self.thinks.lock().map(|q| q.len()).unwrap_or(0),
            )
            .field(
                "compacts_left",
                &self.compacts.lock().map(|q| q.len()).unwrap_or(0),
            )
            .field("think_calls", &self.think_calls.load(Ordering::SeqCst))
            .field("compact_calls", &self.compact_calls.load(Ordering::SeqCst))
            .finish()
    }
}

impl ScriptedBackend {
    fn new(thinks: Vec<ThinkLoopResult>) -> Arc<Self> {
        Arc::new(Self {
            thinks: Mutex::new(thinks.into()),
            compacts: Mutex::new(VecDeque::new()),
            think_calls: AtomicUsize::new(0),
            compact_calls: AtomicUsize::new(0),
        })
    }

    fn with_compacts(self: Arc<Self>, outcomes: Vec<CompactOutcome>) -> Arc<Self> {
        if let Ok(mut q) = self.compacts.lock() {
            *q = outcomes.into();
        }
        self
    }
}

#[async_trait]
impl ThinkBackend for ScriptedBackend {
    async fn think(&self, _deps: &AwakenDeps, _input: ThinkInput) -> Result<ThinkLoopResult> {
        self.think_calls.fetch_add(1, Ordering::SeqCst);
        let front = self
            .thinks
            .lock()
            .map(|mut q| q.pop_front())
            .unwrap_or_else(|poisoned| poisoned.into_inner().pop_front());
        front.ok_or_else(|| {
            err!(
                Internal,
                "scripted backend exhausted: think() was called more times than scripted"
            )
        })
    }

    async fn compact(&self, _deps: &AwakenDeps, _input: CompactInput) -> Result<CompactOutcome> {
        self.compact_calls.fetch_add(1, Ordering::SeqCst);
        let front = self
            .compacts
            .lock()
            .map(|mut q| q.pop_front())
            .unwrap_or_else(|poisoned| poisoned.into_inner().pop_front());
        front.ok_or_else(|| {
            err!(
                Internal,
                "scripted backend exhausted: compact() was called more times than scripted"
            )
        })
    }

    async fn analyze_intent(
        &self,
        _deps: &AwakenDeps,
        _input: IntentInput,
    ) -> Result<IntentAnalysis> {
        // canned 空意图：key_terms 为空 → retrieve 差集为空短路，链路行为与
        // 骨架期等价。需要编排具体意图的用例由后续任务扩成序列。
        Ok(IntentAnalysis::default())
    }

    async fn discover(&self, _deps: &AwakenDeps, _input: DiscoverInput) -> Result<String> {
        // canned「无新发现」JSON：与 discover 骨架的恒退出行为等价
        Ok(r#"{"new_key_terms":[],"prior_thinking_summary":""}"#.to_string())
    }
}

// ==================== fixture ====================

fn init_env(pool: SqlitePool) -> crate::pkg::request_context::RequestContext {
    let _ = crate::config::init();
    // ⚠️ 用全量初始化而不是逐个补：漏掉任一个单例都会在链路深处
    // panic（`unwrap on None`），而报错位置看起来跟缺席的那个毫无关系。
    crate::service::dao::init_all();
    crate::service::dal::init_all();

    // `ToolCallLogger::get()` 未初始化会 panic，而它的 init 走 OnceCell、
    // 只认第一次 —— 用临时目录，避免真实数据目录被写脏。
    crate::pkg::tool_tracing::logger::ToolCallLogger::init(
        std::env::temp_dir().join("ai_orz_legacy_awaken_it"),
    );

    // ⚠️ 顺序不能变：`user::init()` 依赖 dao 已就绪，而 domain 内部取的
    // 是 user DAL 单例 —— 漏了这一步会在真实链路里 panic（unwrap on None）。
    crate::service::dal::user::init();

    crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
}

/// 本次用例专属的 domain 实例
///
/// ⚠️ 不能用全局单例：`AwakenDeps.domain` 必须绑在这个用例的 DB 池上，
/// 而 `sqlx::test` 每例一个池、且用例之间**共享同一个进程**——单例一旦被
/// 第一个用例 set 住，后面的用例就会拿着前任的连接跑（`RUNTIME_DOMAIN` 是
/// OnceLock，`set` 第二次必然失败）。
fn test_domain() -> std::sync::Arc<crate::service::domain::runtime::RuntimeDomainImpl> {
    // `RuntimeDomainImpl` 其余依赖（tool / user / lark）在内部取全局单例，
    // 所以 `dal::init_all()` 必须先跑完。
    std::sync::Arc::new(crate::service::domain::runtime::RuntimeDomainImpl::new(
        crate::service::dal::brain::dal(),
    ))
}

fn make_agent(agent_id: &str) -> Agent {
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
    let provider = ModelProviderPo {
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
    agent.brain = Some(Brain::new_local(
        agent_id.to_string(),
        "Test Agent".to_string(),
        AgentRuntimeConfig::default(),
        provider,
        vec![],
    ));
    agent
}

fn make_message(content: &str) -> Message {
    Message::new_with_context(
        uuid::Uuid::now_v7().to_string(),
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

/// 组装一次链路的运行依赖（`drive` 与节点级用例共用同一装配口径）
///
/// `shadow=true` 保证不落库、不发通知；后端由调用方注入（脚本/失败后端）。
fn make_deps(
    ctx: crate::pkg::request_context::RequestContext,
    backend: Arc<dyn ThinkBackend>,
) -> Arc<AwakenDeps> {
    let agent = make_agent(&format!("agent-{}", uuid::Uuid::now_v7()));
    let trace = MemoryTrace::new(
        agent.po.id.clone(),
        String::new(),
        "test-user".to_string(),
        String::new(),
        common::enums::MemoryRole::Assistant,
        String::new(),
        None,
    );
    let input = super::context::AwakenInput {
        domain: test_domain(),
        ctx,
        agent,
        message: make_message("帮我看下这个任务"),
        options: ThinkingOptions::new(),
        trace,
        shadow: true, // shadow：集成测试不应产生落库副作用
    };
    build_deps_with_backend(backend, input)
}

/// 组装一条链路并跑完
async fn drive(
    ctx: crate::pkg::request_context::RequestContext,
    backend: Arc<ScriptedBackend>,
) -> super::runner::LegacyAwakenOutcome {
    let deps = make_deps(ctx, backend);
    run_legacy_awaken(deps)
        .await
        .expect("legacy_awaken should complete")
}

fn user_msg() -> Vec<ChatMessage> {
    Vec::new()
}

fn final_result(content: &str) -> ThinkLoopResult {
    ThinkLoopResult::Final {
        content: content.to_string(),
        messages: user_msg(),
    }
}

fn compact_outcome(summary: &str, final_text: &str) -> CompactOutcome {
    CompactOutcome {
        final_text: final_text.to_string(),
        compacted_summary: Some(summary.to_string()),
        compacted_memory_id: Some(format!("mem-{}", uuid::Uuid::now_v7())),
    }
}

// ==================== 四个退出路径 ====================

/// 主路径：`final` —— 每个思考路径最终都该落到这里
#[sqlx::test]
async fn final_answer_walks_prepare_deliberate_consolidate(pool: SqlitePool) {
    let ctx = init_env(pool);
    let outcome = drive(ctx, ScriptedBackend::new(vec![final_result("done")])).await;

    assert_eq!(
        outcome.fingerprint,
        "intent#1>retrieve#1>prepare#1>deliberate#1>consolidate#1"
    );
    assert_eq!(outcome.exit, EXIT_FINAL);
    assert_eq!(outcome.raw_output, "done");
    assert_eq!(outcome.steps, 5);
}

/// 溢出回环：`overflow` → 压缩 → **重装配** → 再思考 → 完成
///
/// 这条才是新引擎相对老 `match` 的核心提升：回环是图上一条边，
/// 且 `prepare#2` / `deliberate#2` 在轨迹里是**可区分的两次出现**。
#[sqlx::test]
async fn overflow_loops_back_and_rethinks(pool: SqlitePool) {
    let ctx = init_env(pool);
    let backend = ScriptedBackend::new(vec![
        ThinkLoopResult::ContextOverflow {
            messages: user_msg(),
            input_tokens: 100,
            rounds_used: 2,
        },
        final_result("recovered"),
    ])
    .with_compacts(vec![compact_outcome("上一轮摘要", "")]);

    let outcome = drive(ctx, backend).await;

    assert_eq!(
        outcome.fingerprint,
        "intent#1>retrieve#1>prepare#1>deliberate#1>compact#1>prepare#2>deliberate#2>consolidate#1"
    );
    assert_eq!(outcome.exit, EXIT_FINAL);
    assert_eq!(outcome.raw_output, "recovered");
}

/// 轮次耗尽：`max_rounds` → `discover`（骨架期恒无新词）→ `force_compact` → 收尾
#[sqlx::test]
async fn max_rounds_exits_via_force_compact(pool: SqlitePool) {
    let ctx = init_env(pool);
    let backend = ScriptedBackend::new(vec![ThinkLoopResult::MaxRoundsExceeded {
        messages: user_msg(),
        total_rounds: 10,
    }]);

    let outcome = drive(ctx, backend).await;

    assert_eq!(
        outcome.fingerprint,
        "intent#1>retrieve#1>prepare#1>deliberate#1>discover#1>force_compact#1>consolidate#1"
    );
    assert_eq!(outcome.exit, EXIT_MAX_ROUNDS);
    // ForceCompact 的产物就是给用户的回复
    assert!(
        outcome.raw_output.contains("思考轮次耗尽"),
        "unexpected reply: {}",
        outcome.raw_output
    );
}

// ==================== 失败路径 ====================

/// 永远失败的后端：验证错误语义
#[derive(Debug)]
struct FailingBackend;

#[async_trait]
impl ThinkBackend for FailingBackend {
    async fn think(&self, _deps: &AwakenDeps, _input: ThinkInput) -> Result<ThinkLoopResult> {
        Err(err!(Internal, "deliberation boom"))
    }

    async fn compact(&self, _deps: &AwakenDeps, _input: CompactInput) -> Result<CompactOutcome> {
        Err(err!(Internal, "compaction boom"))
    }

    async fn analyze_intent(
        &self,
        _deps: &AwakenDeps,
        _input: IntentInput,
    ) -> Result<IntentAnalysis> {
        Err(err!(Internal, "intent analyze boom"))
    }

    async fn discover(&self, _deps: &AwakenDeps, _input: DiscoverInput) -> Result<String> {
        Err(err!(Internal, "discover boom"))
    }
}

/// 思考失败必须向上冒泡，不能被吞掉变成「正常结束」
///
/// 这条用例的价值在于区分两种失败形态：
/// - 节点返回 `Err` → run 中断、`Err` 冒泡（本例）
/// - 节点返回 `Fail` 信号但没有对应边 → run **静默终止**（see `tests.rs` 的一致性断言）
///
/// 后者的危害大得多，却没有任何报错信号 —— 所以要在拓扑层用测试锁住。
#[sqlx::test]
async fn deliberation_error_propagates(pool: SqlitePool) {
    let ctx = init_env(pool);
    let deps = make_deps(ctx, Arc::new(FailingBackend));

    let err = run_legacy_awaken(deps)
        .await
        .expect_err("deliberation failure must surface");
    assert!(
        format!("{err}").contains("deliberation boom"),
        "unexpected error: {err}"
    );
}

/// 回环预算耗尽必须**报错**，而不是「跑完了但什么都没做」
///
/// 锁的是 `runner::ensure_consolidated`：引擎把「没有合法候选」当成正常结束，
/// 而在有环的图里预算耗尽正是这种情形 —— 没有这道护栏，一次跑不完的唤醒会安静地
/// `Ok` 返回（不写 Trace、不报统计、不发事件、用户拿不到回复），日志里一行错都没有。
///
/// ⚠️ 这是**替换引入的新失效模式**：老 `awaken` 的同一段循环没有次数上限，
/// 所以老链路里不存在「走到一半不知道该去哪」的状态，也没有任何既有测试会替它看着。
#[sqlx::test]
async fn loop_budget_exhaustion_errors_instead_of_returning_ok(pool: SqlitePool) {
    let ctx = init_env(pool);
    // 一路溢出：`prepare / deliberate` 各能出现 8 次、`compact` 7 次，
    // 队列给够（多余的不会被消费），第 9 次 prepare 无处可去时应中断。
    let backend = ScriptedBackend::new(
        (0..MAX_LOOP_ROUNDS)
            .map(|_| ThinkLoopResult::ContextOverflow {
                messages: user_msg(),
                input_tokens: 100,
                rounds_used: 1,
            })
            .collect(),
    )
    .with_compacts(
        (0..MAX_LOOP_ROUNDS)
            .map(|i| compact_outcome(&format!("第 {i} 轮摘要"), ""))
            .collect(),
    );

    let deps = make_deps(ctx, backend);
    let err = run_legacy_awaken(deps)
        .await
        .expect_err("回环耗尽必须冒泡成错误，不能返回 Ok");
    let msg = format!("{err}");
    assert!(
        msg.contains(NODE_CONSOLIDATE),
        "错误信息应指明没走到收尾节点：{msg}"
    );
}

/// 取消：`cancelled` 直达收尾，走兜底存档语义且不进压缩回环
#[sqlx::test]
async fn cancelled_shortcuts_to_consolidate(pool: SqlitePool) {
    let ctx = init_env(pool);
    let backend = ScriptedBackend::new(vec![ThinkLoopResult::Cancelled {
        messages: user_msg(),
        total_rounds: 3,
    }]);

    let outcome = drive(ctx, Arc::clone(&backend)).await;

    assert_eq!(
        outcome.fingerprint,
        "intent#1>retrieve#1>prepare#1>deliberate#1>consolidate#1"
    );
    assert_eq!(outcome.exit, EXIT_CANCELLED);
    // 取消不该碰压缩（没有活儿要沉淀）
    assert_eq!(backend.compact_calls.load(Ordering::SeqCst), 0);
}

// ==================== 节点级行为：意图降级 / 检索短路 ====================

/// 意图识别失败必须降级为空产物继续走图，而不是把 Err 甩给引擎
///
/// 与 `deliberation_error_propagates` 的口径相反：思考（deliberate）失败要冒泡，
/// 意图识别失败只是「没有 Phase 1」，链路照常（老 awakening 行为的同一约定）。
#[sqlx::test]
async fn intent_node_writes_bus_and_degrades(pool: SqlitePool) {
    let ctx = init_env(pool);
    let deps = make_deps(ctx, Arc::new(FailingBackend));

    let node = super::intent_node::IntentNode::new(Arc::clone(&deps));
    let mut bus = ThinkBus::new();
    let outcome = node
        .run(&mut bus)
        .await
        .expect("intent node must not fail on backend error");

    assert!(matches!(outcome.signal, NodeSignal::Done));
    // 降级产物是「空结构字段」而不是「没有分区」：下游统一按空意图处理
    let ia = read_bus::<IntentAnalysis>(&bus, ContextKey::Intent)
        .expect("bus[Intent] must be present after degradation");
    assert!(ia.key_terms.is_empty());
    assert_eq!(ia.intent_type, "");
}
