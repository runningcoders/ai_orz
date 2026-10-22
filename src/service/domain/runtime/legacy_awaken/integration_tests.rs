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

use super::backend::{CompactInput, ThinkBackend, ThinkInput};
use super::context::{AwakenDeps, build_deps_with_backend};
use super::runner::run_legacy_awaken;
use crate::models::agent::{Agent, AgentPo, AgentRuntimeConfig};
use crate::models::brain::Brain;
use crate::models::cortex_types::ChatMessage;
use crate::models::file::FileMeta;
use crate::models::memory::MemoryTrace;
use crate::models::message::Message;
use crate::models::model_provider::ModelProviderPo;
use crate::service::domain::runtime::compaction::CompactOutcome;
use crate::service::domain::runtime::types::{ThinkLoopResult, ThinkingOptions};
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

    // ⚠️ 顺序不能变：`user::init()` 依赖 dao 已就绪，而 `runtime::init()` 内部
    // 取的是 user DAL 单例 —— 漏了这一步会在真实链路里 panic（unwrap on None）。
    crate::service::dal::user::init();

    // `AwakenDeps` 从 `domain_impl()` 拿具体类型句柄，依赖这里 set 进 OnceLock。
    // LLM 触碰点已被 `ScriptedBackend` 顶掉，所以用真实（默认构造的）domain 即可。
    crate::service::domain::runtime::init();

    crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
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

/// 组装一条链路并跑完；`shadow=true` 保证不落库、不发事件
async fn drive(
    ctx: crate::pkg::request_context::RequestContext,
    backend: Arc<ScriptedBackend>,
) -> super::runner::LegacyAwakenOutcome {
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
    let deps = build_deps_with_backend(
        backend,
        ctx,
        agent,
        make_message("帮我看下这个任务"),
        ThinkingOptions::new(),
        trace,
        true, // shadow：集成测试不应产生落库副作用
    );
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

    assert_eq!(outcome.fingerprint, "prepare#1>deliberate#1>consolidate#1");
    assert_eq!(outcome.exit, "final");
    assert_eq!(outcome.raw_output, "done");
    assert_eq!(outcome.steps, 3);
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
        "prepare#1>deliberate#1>compact#1>prepare#2>deliberate#2>consolidate#1"
    );
    assert_eq!(outcome.exit, "final");
    assert_eq!(outcome.raw_output, "recovered");
}

/// 轮次耗尽：`max_rounds` → `force_compact` → 收尾，产物即回复
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
        "prepare#1>deliberate#1>force_compact#1>consolidate#1"
    );
    assert_eq!(outcome.exit, "max_rounds");
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
    let deps = build_deps_with_backend(
        Arc::new(FailingBackend),
        ctx,
        agent,
        make_message("帮我看下这个任务"),
        ThinkingOptions::new(),
        trace,
        true,
    );

    let err = run_legacy_awaken(deps)
        .await
        .expect_err("deliberation failure must surface");
    assert!(
        format!("{err}").contains("deliberation boom"),
        "unexpected error: {err}"
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

    assert_eq!(outcome.fingerprint, "prepare#1>deliberate#1>consolidate#1");
    assert_eq!(outcome.exit, "cancelled");
    // 取消不该碰压缩（没有活儿要沉淀）
    assert_eq!(backend.compact_calls.load(Ordering::SeqCst), 0);
}
