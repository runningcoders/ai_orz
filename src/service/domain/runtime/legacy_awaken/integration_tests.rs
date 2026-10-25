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
//! ⚠️ 蓝图已扩成完整思考图（entry=`intent`）：intent / retrieve / discover
//! 均已是真正实现——intent 降级为空意图时 retrieve 短路，discover 产出
//! 新词时回 retrieve 再检索，无新词时走 force_compact 压缩退出。

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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// ==================== 脚本化的 LLM 后端 ====================

/// 按队列返回预设结果；队列耗尽视为**测试写错**（而不是链路的错）
///
/// ⚠️ `Debug` 是手写而非 derive：`CompactOutcome` 没实现 `Debug`，
/// 而 `ThinkBackend: Debug` 是这个 trait 的硬要求（诊断用）。
struct ScriptedBackend {
    thinks: Mutex<VecDeque<ThinkLoopResult>>,
    compacts: Mutex<VecDeque<CompactOutcome>>,
    discovers: Mutex<VecDeque<String>>,
    intents: Mutex<VecDeque<IntentAnalysis>>,
    /// analyze_intent 直接报错（意图降级路径）；队列与它互斥使用
    fail_intent: AtomicBool,
    pub think_calls: AtomicUsize,
    pub compact_calls: AtomicUsize,
    pub discover_calls: AtomicUsize,
    pub intent_calls: AtomicUsize,
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
            .field(
                "discovers_left",
                &self.discovers.lock().map(|q| q.len()).unwrap_or(0),
            )
            .field(
                "intents_left",
                &self.intents.lock().map(|q| q.len()).unwrap_or(0),
            )
            .field("fail_intent", &self.fail_intent.load(Ordering::SeqCst))
            .field("think_calls", &self.think_calls.load(Ordering::SeqCst))
            .field("compact_calls", &self.compact_calls.load(Ordering::SeqCst))
            .field(
                "discover_calls",
                &self.discover_calls.load(Ordering::SeqCst),
            )
            .field("intent_calls", &self.intent_calls.load(Ordering::SeqCst))
            .finish()
    }
}

impl ScriptedBackend {
    fn new(thinks: Vec<ThinkLoopResult>) -> Arc<Self> {
        Arc::new(Self {
            thinks: Mutex::new(thinks.into()),
            compacts: Mutex::new(VecDeque::new()),
            discovers: Mutex::new(VecDeque::new()),
            intents: Mutex::new(VecDeque::new()),
            fail_intent: AtomicBool::new(false),
            think_calls: AtomicUsize::new(0),
            compact_calls: AtomicUsize::new(0),
            discover_calls: AtomicUsize::new(0),
            intent_calls: AtomicUsize::new(0),
        })
    }

    fn with_compacts(self: Arc<Self>, outcomes: Vec<CompactOutcome>) -> Arc<Self> {
        if let Ok(mut q) = self.compacts.lock() {
            *q = outcomes.into();
        }
        self
    }

    fn with_discovers(self: Arc<Self>, texts: Vec<String>) -> Arc<Self> {
        if let Ok(mut q) = self.discovers.lock() {
            *q = texts.into();
        }
        self
    }

    /// 编排 analyze_intent 的返回序列（真意图：带 key_terms，retrieve 会真正查库）
    fn with_intents(self: Arc<Self>, analyses: Vec<IntentAnalysis>) -> Arc<Self> {
        if let Ok(mut q) = self.intents.lock() {
            *q = analyses.into();
        }
        self
    }

    /// 让 analyze_intent 报错：意图降级路径（intent 节点写空意图继续走）
    fn with_failing_intent(self: Arc<Self>) -> Arc<Self> {
        self.fail_intent.store(true, Ordering::SeqCst);
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
        self.intent_calls.fetch_add(1, Ordering::SeqCst);
        // 意图降级路径：Err 由 intent 节点兜成空意图继续走
        if self.fail_intent.load(Ordering::SeqCst) {
            return Err(err!(Internal, "scripted intent failure"));
        }
        // 队列空 → 默认空意图（key_terms 为空 → retrieve 短路），照既有用例口径；
        // 需要真意图（retrieve 真正查库）的用例用 with_intents 编排
        let front = self
            .intents
            .lock()
            .map(|mut q| q.pop_front())
            .unwrap_or_else(|poisoned| poisoned.into_inner().pop_front());
        Ok(front.unwrap_or_default())
    }

    async fn discover(&self, _deps: &AwakenDeps, _input: DiscoverInput) -> Result<String> {
        // 与 thinks/compacts 同口径：队列耗尽 = 测试写错（discover 被调用了
        // 却没编排返回值），而不是链路的错
        self.discover_calls.fetch_add(1, Ordering::SeqCst);
        let front = self
            .discovers
            .lock()
            .map(|mut q| q.pop_front())
            .unwrap_or_else(|poisoned| poisoned.into_inner().pop_front());
        front.ok_or_else(|| {
            err!(
                Internal,
                "scripted backend exhausted: discover() was called more times than scripted"
            )
        })
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

/// 轮次耗尽：`max_rounds` → `discover`（无新词）→ `force_compact` → 收尾
#[sqlx::test]
async fn max_rounds_exits_via_force_compact(pool: SqlitePool) {
    let ctx = init_env(pool);
    let backend = ScriptedBackend::new(vec![ThinkLoopResult::MaxRoundsExceeded {
        messages: user_msg(),
        total_rounds: 10,
    }])
    // discover 真实现会调后端：canned「无新发现」JSON → 走 force_compact
    .with_discovers(vec![
        r#"{"new_key_terms":[],"prior_thinking_summary":""}"#.to_string(),
    ]);

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

// ==================== 新拓扑全链路 ====================

/// happy path（真意图版）：intent 带 key_terms → retrieve **真正查库**（visited 记账）
///
/// 与 `final_answer_walks_prepare_deliberate_consolidate`（空意图、retrieve 短路）
/// 互补：那条锁走线，这条锁「有词可查时机械检索真的发生」（含 DAL 查询路径）。
#[sqlx::test]
async fn happy_path_runs_intent_retrieve_deliberate(pool: SqlitePool) {
    let ctx = init_env(pool);
    let backend =
        ScriptedBackend::new(vec![final_result("done")]).with_intents(vec![IntentAnalysis {
            key_terms: vec!["测试".to_string()],
            summary: "用户要测试".to_string(),
            ..Default::default()
        }]);

    // 手动装配（不走 drive）：跑完还要检查 run 级检索状态的 visited 记账
    let deps = make_deps(ctx, backend.clone());
    let outcome = run_legacy_awaken(Arc::clone(&deps))
        .await
        .expect("happy path must complete");

    assert_eq!(
        outcome.fingerprint,
        "intent#1>retrieve#1>prepare#1>deliberate#1>consolidate#1"
    );
    assert_eq!(outcome.exit, EXIT_FINAL);
    assert_eq!(outcome.raw_output, "done");
    assert_eq!(backend.intent_calls.load(Ordering::SeqCst), 1);
    // visited 记账证明 retrieve 真查了词（而非空意图短路）
    let queried = deps
        .retrieval
        .lock()
        .map(|s| s.queried_terms.clone())
        .unwrap_or_default();
    assert_eq!(queried, vec!["测试".to_string()]);
}

/// 新发现回环：max_rounds → discover 有新词 → retrieve#2 → prepare#2 → 再思考完成
///
/// 新拓扑的核心增量（design §3.2「新发现循环边」）：轮次耗尽不再直接压缩退出，
/// 而是先问 discover「有没有值得再查的线索」——有就带着新词回检索装配再想一轮。
#[sqlx::test]
async fn discovery_loop_retrieves_again(pool: SqlitePool) {
    let ctx = init_env(pool);
    let backend = ScriptedBackend::new(vec![
        ThinkLoopResult::MaxRoundsExceeded {
            messages: user_msg(),
            total_rounds: 8,
        },
        final_result("recovered after loop"),
    ])
    .with_intents(vec![IntentAnalysis {
        key_terms: vec!["初始词".to_string()],
        ..Default::default()
    }])
    .with_discovers(vec![
        r#"{"new_key_terms":["新线索"],"prior_thinking_summary":"轨迹里出现了未检索的线索"}"#
            .to_string(),
    ]);

    let deps = make_deps(ctx, backend.clone());
    let outcome = run_legacy_awaken(Arc::clone(&deps))
        .await
        .expect("discovery loop must complete");

    assert_eq!(
        outcome.fingerprint,
        "intent#1>retrieve#1>prepare#1>deliberate#1>discover#1>retrieve#2>prepare#2>deliberate#2>consolidate#1"
    );
    assert_eq!(outcome.exit, EXIT_FINAL);
    assert_eq!(outcome.raw_output, "recovered after loop");
    assert_eq!(backend.discover_calls.load(Ordering::SeqCst), 1);
    // 回环新词被 retrieve 消费并记账（初始词 + 新线索各查一次）
    let queried = deps
        .retrieval
        .lock()
        .map(|s| s.queried_terms.clone())
        .unwrap_or_default();
    assert_eq!(queried, vec!["初始词".to_string(), "新线索".to_string()]);
    assert_eq!(
        deps.retrieval
            .lock()
            .map(|s| s.discovery_rounds)
            .unwrap_or(0),
        1,
        "new_findings 路径必须计一轮软预算"
    );
}

/// 意图分析失败 → 降级为空意图照常走完链路（intent 是增强，不是闸门）
#[sqlx::test]
async fn intent_failure_degrades_and_continues(pool: SqlitePool) {
    let ctx = init_env(pool);
    let backend = ScriptedBackend::new(vec![final_result("still done")]).with_failing_intent();

    let outcome = drive(ctx, backend.clone()).await;

    // 降级不炸链路：走线与空意图 happy path 完全一致，照常收尾
    assert_eq!(
        outcome.fingerprint,
        "intent#1>retrieve#1>prepare#1>deliberate#1>consolidate#1"
    );
    assert_eq!(outcome.exit, EXIT_FINAL);
    assert_eq!(outcome.raw_output, "still done");
    assert_eq!(backend.intent_calls.load(Ordering::SeqCst), 1);
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

/// visited 全命中过 → 差集为空 → 短路写空 pack，不产生检索动作
///
/// 回环语义的哨兵：discover 回环重新进 retrieve 时，已查过的词不二次检索。
#[sqlx::test]
async fn retrieve_node_short_circuits_on_no_new_terms(pool: SqlitePool) {
    let ctx = init_env(pool);
    let deps = make_deps(ctx, ScriptedBackend::new(vec![]));

    // 预置意图产物：全部关键词都已检索过
    let ia = IntentAnalysis {
        key_terms: vec!["已检索".to_string()],
        ..Default::default()
    };
    let mut bus = ThinkBus::new();
    bus.insert(
        ContextKey::Intent,
        LoadDepth::Full,
        serde_json::to_value(&ia).expect("encode intent"),
    );
    deps.retrieval
        .lock()
        .map(|mut s| s.queried_terms = vec!["已检索".to_string()])
        .unwrap_or(());

    let node = super::retrieve_node::RetrieveNode::new(Arc::clone(&deps));
    let outcome = node
        .run(&mut bus)
        .await
        .expect("retrieve node must not fail");

    assert!(matches!(outcome.signal, NodeSignal::Done));
    let pack = read_bus::<RetrievalPack>(&bus, ContextKey::Knowledge)
        .expect("bus[Knowledge] must be present");
    assert!(pack.query_terms.is_empty());
    assert!(pack.knowledge_hits.is_empty());
    assert!(pack.method_skills.is_empty());
    assert!(pack.prior_thinking_summary.is_none());
}

/// 有新词 → 机械检索路径走通：查库（空库零命中合法）+ visited 记账
#[sqlx::test]
async fn retrieve_node_records_new_terms_and_queries(pool: SqlitePool) {
    let ctx = init_env(pool);
    let deps = make_deps(ctx, ScriptedBackend::new(vec![]));

    let ia = IntentAnalysis {
        key_terms: vec!["新线索".to_string()],
        ..Default::default()
    };
    let mut bus = ThinkBus::new();
    bus.insert(
        ContextKey::Intent,
        LoadDepth::Full,
        serde_json::to_value(&ia).expect("encode intent"),
    );

    let node = super::retrieve_node::RetrieveNode::new(Arc::clone(&deps));
    let outcome = node
        .run(&mut bus)
        .await
        .expect("retrieve node must not fail");

    assert!(matches!(outcome.signal, NodeSignal::Done));
    let pack = read_bus::<RetrievalPack>(&bus, ContextKey::Knowledge)
        .expect("bus[Knowledge] must be present");
    assert_eq!(pack.query_terms, vec!["新线索".to_string()]);
    // 空库：零命中是合法结果（检索是增强，不是闸门）
    assert!(pack.knowledge_hits.is_empty());
    assert!(pack.method_skills.is_empty());
    // visited 记账：同词下次差集为空 → 短路（上一条用例锁的行为）
    let queried = deps
        .retrieval
        .lock()
        .map(|s| s.queried_terms.clone())
        .unwrap_or_default();
    assert_eq!(queried, vec!["新线索".to_string()]);
}

// ==================== 节点级行为：discover 回环 ====================

/// discover 软预算尽 → 直接 no_new_findings，模型一次都不被调
///
/// 锁的是软预算先于模型调用：省一次 discover 开销（蓝图硬预算是兜底，
/// 软预算的价值就是「不到万不得已不花这一次调用」）。
#[sqlx::test]
async fn discover_node_yields_no_new_findings_when_budget_exhausted(pool: SqlitePool) {
    let ctx = init_env(pool);
    // discovers 队列留空：一旦被调会报「script exhausted」——但软预算先退出，
    // discover_calls 必须是 0 才算软预算真正生效
    let backend = ScriptedBackend::new(vec![]);
    let deps = make_deps(ctx, backend.clone());
    deps.retrieval
        .lock()
        .map(|mut s| s.discovery_rounds = 2)
        .unwrap_or(());

    let node = super::discover_node::DiscoverNode::new(Arc::clone(&deps));
    let mut bus = ThinkBus::new();
    let outcome = node
        .run(&mut bus)
        .await
        .expect("discover node must not fail on exhausted budget");

    assert!(matches!(
        &outcome.signal,
        NodeSignal::Yield(tag) if tag == "no_new_findings"
    ));
    assert_eq!(
        backend.discover_calls.load(Ordering::SeqCst),
        0,
        "软预算尽时不得调模型"
    );
    // 退出不计轮次：状态原样（不像 new_findings 路径那样 +1）
    let rounds = deps
        .retrieval
        .lock()
        .map(|s| s.discovery_rounds)
        .unwrap_or(0);
    assert_eq!(rounds, 2);
}

/// 有新词 → Yield(new_findings) + 写「新词 + 思考摘要」过渡 pack + 轮次记账
///
/// 过渡 pack 是 discover → retrieve 的接力棒（design §3.4）：retrieve 会把它
/// 覆写成完整检索产物，prior_thinking_summary 透传到 prepare 消费。
#[sqlx::test]
async fn discover_node_writes_pack_and_yields_new_findings(pool: SqlitePool) {
    let ctx = init_env(pool);
    let backend = ScriptedBackend::new(vec![]).with_discovers(vec![
        // 新词里混一个已查过的词：验证 visited 差集去重
        r#"{"new_key_terms":["新线索","已检索"],"prior_thinking_summary":"已尝试检索并整理出三步方案"}"#
            .to_string(),
    ]);
    let deps = make_deps(ctx, backend.clone());
    deps.retrieval
        .lock()
        .map(|mut s| s.queried_terms = vec!["已检索".to_string()])
        .unwrap_or(());

    // 预置思考轨迹（Artifacts）：discover 的模型输入来自这里
    let artifacts = super::nodes::Artifacts {
        exit: super::nodes::EXIT_MAX_ROUNDS.to_string(),
        content: String::new(),
        messages: vec![
            ChatMessage::user("帮我看下这个任务"),
            ChatMessage::Assistant {
                content: Some("查了三个工具都没找到".to_string()),
                tool_calls: None,
            },
        ],
    };
    let mut bus = ThinkBus::new();
    bus.insert(
        ContextKey::Artifacts,
        LoadDepth::Full,
        serde_json::to_value(&artifacts).expect("encode artifacts"),
    );

    let node = super::discover_node::DiscoverNode::new(Arc::clone(&deps));
    let outcome = node
        .run(&mut bus)
        .await
        .expect("discover node must not fail");

    assert!(matches!(
        &outcome.signal,
        NodeSignal::Yield(tag) if tag == "new_findings"
    ));
    assert_eq!(backend.discover_calls.load(Ordering::SeqCst), 1);
    // 过渡 pack：只有差集后的新词 + 思考摘要，命中/技能留空（retrieve 覆写）
    let pack = read_bus::<RetrievalPack>(&bus, ContextKey::Knowledge)
        .expect("bus[Knowledge] must be present after discovery");
    assert_eq!(pack.query_terms, vec!["新线索".to_string()]);
    assert_eq!(
        pack.prior_thinking_summary.as_deref(),
        Some("已尝试检索并整理出三步方案")
    );
    assert!(pack.knowledge_hits.is_empty());
    assert!(pack.method_skills.is_empty());
    // 轮次记账：new_findings 才计 1（软预算的递增点）
    let rounds = deps
        .retrieval
        .lock()
        .map(|s| s.discovery_rounds)
        .unwrap_or(0);
    assert_eq!(rounds, 1);
}

/// discover 回环词必须被 retrieve 真正检索：过渡 pack 的 query_terms 是唯一新息
///
/// 这是回环接力的关键一环：retrieve 若只看 intent 关键词，discover 的新词
/// 永远不会被检索（intent 词早已在 visited 里，差集恒空）——回环形同虚设。
#[sqlx::test]
async fn retrieve_node_consumes_discovery_pack_terms(pool: SqlitePool) {
    let ctx = init_env(pool);
    let deps = make_deps(ctx, ScriptedBackend::new(vec![]));

    // 模拟 discover 已写过渡 pack：query_terms = 回环新词，prior = 思考摘要
    let discovery_pack = RetrievalPack {
        query_terms: vec!["回环新词".to_string()],
        knowledge_hits: Vec::new(),
        method_skills: Vec::new(),
        prior_thinking_summary: Some("上一轮已整理三步方案".to_string()),
    };
    let mut bus = ThinkBus::new();
    bus.insert(
        ContextKey::Knowledge,
        LoadDepth::Full,
        serde_json::to_value(&discovery_pack).expect("encode discovery pack"),
    );
    // intent 词已全部检索过：若无 pack 接力，差集为空 → 短路（上一组用例锁的行为）
    let ia = IntentAnalysis {
        key_terms: vec!["已查".to_string()],
        ..Default::default()
    };
    bus.insert(
        ContextKey::Intent,
        LoadDepth::Full,
        serde_json::to_value(&ia).expect("encode intent"),
    );
    deps.retrieval
        .lock()
        .map(|mut s| s.queried_terms = vec!["已查".to_string()])
        .unwrap_or(());

    let node = super::retrieve_node::RetrieveNode::new(Arc::clone(&deps));
    let outcome = node
        .run(&mut bus)
        .await
        .expect("retrieve node must not fail");

    assert!(matches!(outcome.signal, NodeSignal::Done));
    let pack = read_bus::<RetrievalPack>(&bus, ContextKey::Knowledge)
        .expect("bus[Knowledge] must be present");
    // 回环新词被真正检索（成为完整 pack 的 query_terms），而非短路
    assert_eq!(pack.query_terms, vec!["回环新词".to_string()]);
    // discover 的思考摘要透传：prepare 要消费的衔接线索
    assert_eq!(
        pack.prior_thinking_summary.as_deref(),
        Some("上一轮已整理三步方案")
    );
    // visited 记账：回环新词从此不再重复检索
    let queried = deps
        .retrieval
        .lock()
        .map(|s| s.queried_terms.clone())
        .unwrap_or_default();
    assert!(queried.contains(&"回环新词".to_string()));
}
