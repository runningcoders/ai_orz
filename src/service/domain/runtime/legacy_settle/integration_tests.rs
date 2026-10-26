//! `legacy_settle` 的集成测试：**真实取数 + 真实落库**，只替换 LLM 触碰点
//!
//! 与 `tests.rs` 的分工：那边测图长什么样，这里测图跑起来怎样。
//!
//! ⚠️ LLM 一律走脚本后端（`ScriptedSettleBackend`），**绝不裸调真实模型**：
//! 否则测试是否通过取决于本机能不能连上 OpenAI，那是把「模型可用」当成
//! 「链路正确」的假绿。这里用退出信号驱动流转，而不是构造特定的模型响应。

use super::backend::{SettleBackend, SettleThinkInput};
use super::context::{PendingBatch, SettleDeps, SettleInput, build_deps_with_backend};
use super::nodes::{MARK_SETTLE_FAILED, NODE_FINALIZE, NODE_PREPARE, NODE_SETTLE};
use super::pending::{PENDING_MAX_ITEMS, build_pending_memories_summary};
use super::runner::{LegacySettleOutcome, run_legacy_settle};
use crate::models::agent::{Agent, AgentPo, AgentRuntimeConfig};
use crate::models::brain::Brain;
use crate::models::memory::{MemoryCreateParams, ShortTermMemoryIndexPo};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::request_context::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::types::{ThinkLoopResult, ThinkingOptions};
use crate::service::domain::runtime::{RuntimeDomain, RuntimeDomainImpl};
use async_trait::async_trait;
use common::enums::ThinkingScene;
use common::enums::{AgentStatus, MemoryStatus, MemoryType, ModelCapability, ProviderType};
use common::error::{Result, err};
use sqlx::SqlitePool;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// ==================== 脚本化的 LLM 后端 ====================

struct ScriptedSettleBackend {
    thinks: Mutex<VecDeque<ThinkLoopResult>>,
    pub calls: AtomicUsize,
}

impl std::fmt::Debug for ScriptedSettleBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptedSettleBackend")
            .field(
                "thinks_left",
                &self.thinks.lock().map(|q| q.len()).unwrap_or(0),
            )
            .field("calls", &self.calls.load(Ordering::SeqCst))
            .finish()
    }
}

impl ScriptedSettleBackend {
    fn new(thinks: Vec<ThinkLoopResult>) -> Arc<Self> {
        Arc::new(Self {
            thinks: Mutex::new(thinks.into()),
            calls: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl SettleBackend for ScriptedSettleBackend {
    async fn think(
        &self,
        _deps: &SettleDeps,
        _batch: &PendingBatch,
        _input: SettleThinkInput,
    ) -> Result<ThinkLoopResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let front = self
            .thinks
            .lock()
            .map(|mut q| q.pop_front())
            .unwrap_or_else(|poisoned| poisoned.into_inner().pop_front());
        front.ok_or_else(|| {
            err!(
                Internal,
                "scripted backend exhausted: think() called more times than scripted"
            )
        })
    }
}

/// 永远失败的后端：验证错误语义
#[derive(Debug)]
struct FailingBackend;

#[async_trait]
impl SettleBackend for FailingBackend {
    async fn think(
        &self,
        _deps: &SettleDeps,
        _batch: &PendingBatch,
        _input: SettleThinkInput,
    ) -> Result<ThinkLoopResult> {
        Err(err!(Internal, "settle boom"))
    }
}

/// 抢先标记后端：在 `think` 期间就把本批记忆置为已沉淀
///
/// 用来复现**标记未生效**这条被吞的路径：`mark_short_term_settled` 的内部
/// `update_short_term_index` 失败只 `log_warn` 并最终 `Ok(marked)`，所以 settle
/// 节点拿到的是 `Ok(0)` / `Ok(<ids.len())`，**不是 `Err`**。这里让记忆在 settle
/// 节点标记之前就已非 Active，制造出同样的返回值。
///
/// ⚠️ 这是唯一能证明「标记未生效不会回环」的场景：修复前它会一路 `SIGNAL_NEXT`
/// 把同一批记忆反复重打，直到 `MAX_SETTLE_BATCHES` 打满再 nack 重投。
#[derive(Debug)]
struct PreSettledBackend {
    /// 抢先标记前 N 条（`None` = 本批全部 → settle 节点拿到 `Ok(0)`）
    pre_mark: Option<usize>,
    pub calls: AtomicUsize,
    /// 最后一次 `think` 拿到的批次大小（便于断言批量，避免猜）
    pub last_batch_len: AtomicUsize,
}

#[async_trait]
impl SettleBackend for PreSettledBackend {
    async fn think(
        &self,
        deps: &SettleDeps,
        batch: &PendingBatch,
        _input: SettleThinkInput,
    ) -> Result<ThinkLoopResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.last_batch_len.store(batch.ids.len(), Ordering::SeqCst);
        let ids: Vec<String> = match self.pre_mark {
            Some(n) => batch.ids.iter().take(n).cloned().collect(),
            None => batch.ids.clone(),
        };
        deps.domain
            .memory()
            .mark_short_term_settled(deps.ctx.clone(), &deps.agent.po.id, &ids)
            .await?;
        Ok(final_result("pre-settled"))
    }
}

// ==================== fixture ====================

fn init_env_sync(pool: SqlitePool) -> RequestContext {
    let _ = crate::config::init();
    crate::service::dao::init_all();
    crate::service::dal::init_all();
    crate::pkg::tool_tracing::logger::ToolCallLogger::init(
        std::env::temp_dir().join("ai_orz_legacy_settle_it"),
    );
    crate::service::dal::user::init();
    crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
}

/// 初始化测试环境
///
/// 额外拉起全局 Storage：状态机变更会 fire-and-forget 地 `publish` 一条事件，
/// 其内部用 `RequestContext::new_system()` 取全局 Storage；不初始化会在
/// tokio 后台任务里 panic（测试仍绿，但日志被噪声污染）。
async fn init_env(pool: SqlitePool) -> RequestContext {
    crate::pkg::storage::test_support::init_for_test().await;
    init_env_sync(pool)
}

/// 本次用例专属 domain：必须绑在用例的 DB 池上，不能取全局单例
fn test_domain() -> Arc<RuntimeDomainImpl> {
    Arc::new(RuntimeDomainImpl::new(crate::service::dal::brain::dal()))
}

fn make_agent(agent_id: &str, context_length: Option<u32>) -> Agent {
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
    let config = match context_length {
        Some(len) => format!(r#"{{"recommended_context_length":{len}}}"#),
        None => "{}".to_string(),
    };
    let provider = ModelProviderPo {
        id: "mock-provider".to_string(),
        name: "Mock Provider".to_string(),
        provider_type: ProviderType::OpenAI,
        model_name: "gpt-4".to_string(),
        capability: ModelCapability::Agent,
        api_key: "fake-key".to_string(),
        base_url: None,
        description: None,
        config,
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

async fn seed(
    domain: &RuntimeDomainImpl,
    ctx: &RequestContext,
    agent_id: &str,
    id: &str,
    summary: String,
    offset_ms: i64,
) {
    let now = chrono::Utc::now().timestamp_millis() + offset_ms;
    domain
        .memory()
        .create(
            ctx.clone(),
            MemoryCreateParams::CreateShortTerm(ShortTermMemoryIndexPo {
                id: id.to_string(),
                agent_id: agent_id.to_string(),
                task_id: None,
                role: "user".to_string(),
                summary,
                tags: "[]".to_string(),
                trace_ids: "[]".to_string(),
                status: MemoryStatus::Active,
                created_at: now,
                updated_at: now,
            }),
        )
        .await
        .expect("seed memory");
}

async fn status_of(
    domain: &RuntimeDomainImpl,
    ctx: &RequestContext,
    agent_id: &str,
    id: &str,
) -> MemoryStatus {
    let list = domain
        .memory()
        .query(
            ctx.clone(),
            MemoryQuery {
                ids: Some(vec![id.to_string()]),
                agent_id: Some(agent_id.to_string()),
                memory_type: Some(MemoryType::ShortTerm),
                ..Default::default()
            },
        )
        .await
        .expect("query memory");
    super::pending::short_term_of(list.first().expect("memory should exist"))
        .expect("should be short term")
        .status
}

fn final_result(content: &str) -> ThinkLoopResult {
    ThinkLoopResult::Final {
        content: content.to_string(),
        messages: Vec::new(),
    }
}

/// 组装一条链路并跑完
async fn drive(
    domain: Arc<RuntimeDomainImpl>,
    ctx: RequestContext,
    agent: Agent,
    backend: Arc<dyn SettleBackend>,
) -> Result<LegacySettleOutcome> {
    let input = SettleInput {
        domain,
        ctx,
        agent,
        options: ThinkingOptions::for_scene(ThinkingScene::Settle),
        trace_ids: Vec::new(),
        settle_limit: PENDING_MAX_ITEMS,
    };
    run_legacy_settle(build_deps_with_backend(backend, input)).await
}

// ==================== 取数口径 ====================

/// 待沉淀队列按最早优先取，避免老记忆被新记忆挤出窗口
#[sqlx::test]
async fn pending_summary_uses_oldest_first(pool: SqlitePool) {
    let ctx = init_env(pool).await;
    let domain = test_domain();

    seed(&domain, &ctx, "agent-settle", "p-old", "摘要 old".into(), 0).await;
    seed(
        &domain,
        &ctx,
        "agent-settle",
        "p-mid",
        "摘要 mid".into(),
        10,
    )
    .await;
    seed(
        &domain,
        &ctx,
        "agent-settle",
        "p-new",
        "摘要 new".into(),
        20,
    )
    .await;

    let (summary, ids, truncated) =
        build_pending_memories_summary(&ctx, domain.memory(), "agent-settle", 10_000, 2)
            .await
            .expect("query pending")
            .expect("应有待沉淀记忆");

    assert_eq!(ids, vec!["p-old".to_string(), "p-mid".to_string()]);
    assert!(summary.contains("[id=p-old]"));
    assert!(!summary.contains("[id=p-new]"));
    // 候选集被条数护栏截断，保守标记为仍有剩余
    assert!(truncated);
}

/// 预算自适应：累计 token 数还在预算内就继续拼，而不是固定条数
#[sqlx::test]
async fn pending_summary_accumulates_within_budget(pool: SqlitePool) {
    let ctx = init_env(pool).await;
    let domain = test_domain();

    for i in 0..5 {
        seed(
            &domain,
            &ctx,
            "agent-settle",
            &format!("b-{i}"),
            format!("摘要 {i}"),
            i,
        )
        .await;
    }

    // 预算充足 → 全部纳入
    let (_summary, ids, truncated) = build_pending_memories_summary(
        &ctx,
        domain.memory(),
        "agent-settle",
        100_000,
        PENDING_MAX_ITEMS,
    )
    .await
    .expect("query pending")
    .expect("应有待沉淀记忆");
    assert_eq!(ids.len(), 5);
    assert!(!truncated);

    // 预算极小（20 token）→ 只纳入能放下的前几条，并标记截断
    let (summary, ids, truncated) = build_pending_memories_summary(
        &ctx,
        domain.memory(),
        "agent-settle",
        20,
        PENDING_MAX_ITEMS,
    )
    .await
    .expect("query pending")
    .expect("应有待沉淀记忆");
    assert!(truncated, "预算不足应标记截断");
    assert!(ids.len() < 5, "预算不足应少纳入：实际 {}", ids.len());
    assert!(summary.chars().count() <= 100, "不应明显超出预算");
    assert_eq!(ids[0], "b-0", "仍按最早优先");
}

/// 无待沉淀记忆时返回 None，调用方据此跳过
#[sqlx::test]
async fn pending_summary_returns_none_when_empty(pool: SqlitePool) {
    let ctx = init_env(pool).await;
    let domain = test_domain();
    assert!(
        build_pending_memories_summary(
            &ctx,
            domain.memory(),
            "agent-settle",
            10_000,
            PENDING_MAX_ITEMS,
        )
        .await
        .expect("query pending")
        .is_none()
    );
}

// ==================== 整条链路 ====================

/// 主路径：一次 run 扫完所有未沉淀记忆，**每批一条 Trace、全部标记 Settled**
#[sqlx::test]
async fn settles_every_pending_memory_across_batches(pool: SqlitePool) {
    let ctx = init_env(pool).await;
    let domain = test_domain();
    let agent_id = "agent-settle-multi";

    // 预算 = 1000 * 40% = 400 token；每条摘要 ~245 token → 每批只装得下一条
    let long_summary = "沉淀素材".repeat(60);
    for i in 0..3 {
        seed(
            &domain,
            &ctx,
            agent_id,
            &format!("m-{i}"),
            format!("{i}-{long_summary}"),
            i,
        )
        .await;
    }

    let backend = ScriptedSettleBackend::new(vec![
        final_result("batch 1"),
        final_result("batch 2"),
        final_result("batch 3"),
    ]);
    let outcome = drive(
        Arc::clone(&domain),
        ctx.clone(),
        make_agent(agent_id, Some(1000)),
        Arc::clone(&backend) as Arc<dyn SettleBackend>,
    )
    .await
    .expect("settle should complete");

    assert_eq!(
        outcome.fingerprint,
        format!(
            "{NODE_PREPARE}#1>{NODE_SETTLE}#1>{NODE_PREPARE}#2>{NODE_SETTLE}#2>{NODE_PREPARE}#3>{NODE_SETTLE}#3>{NODE_PREPARE}#4>{NODE_FINALIZE}#1"
        )
    );
    assert_eq!(outcome.settled_count, 3);
    assert_eq!(outcome.batches, 3);
    assert_eq!(outcome.attempts, 3);
    assert_eq!(backend.calls.load(Ordering::SeqCst), 3);
    assert!(outcome.blocked.is_none());

    for i in 0..3 {
        assert_eq!(
            status_of(&domain, &ctx, agent_id, &format!("m-{i}")).await,
            MemoryStatus::Settled,
            "m-{i} 应被标记为已沉淀"
        );
    }
}

/// 溢出批次：**不标记 Settled**，本轮立即结束（避免反复选中同一批）
#[sqlx::test]
async fn overflow_batch_stops_run_without_marking(pool: SqlitePool) {
    let ctx = init_env(pool).await;
    let domain = test_domain();
    let agent_id = "agent-settle-overflow";

    seed(&domain, &ctx, agent_id, "m-overflow", "溢出素材".into(), 0).await;

    let backend = ScriptedSettleBackend::new(vec![ThinkLoopResult::ContextOverflow {
        messages: Vec::new(),
        input_tokens: 100,
        rounds_used: 2,
    }]);
    let outcome = drive(
        Arc::clone(&domain),
        ctx.clone(),
        make_agent(agent_id, None),
        Arc::clone(&backend) as Arc<dyn SettleBackend>,
    )
    .await
    .expect("blocked run still reaches finalize");

    assert_eq!(
        outcome.fingerprint,
        format!("{NODE_PREPARE}#1>{NODE_SETTLE}#1>{NODE_FINALIZE}#1")
    );
    assert_eq!(outcome.settled_count, 0);
    assert_eq!(outcome.batches, 0);
    // 只尝试了一次就停：不会拿着同一批反复跑
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        outcome.blocked.as_deref(),
        Some(crate::service::domain::runtime::legacy_awaken::nodes::EXIT_OVERFLOW)
    );
    assert_eq!(
        status_of(&domain, &ctx, agent_id, "m-overflow").await,
        MemoryStatus::Active,
        "无效沉淀必须保持 Active 交给下一次"
    );
}

/// 后端失败必须向上冒泡，不能被吞成「正常结束」
#[sqlx::test]
async fn backend_error_propagates(pool: SqlitePool) {
    let ctx = init_env(pool).await;
    let domain = test_domain();
    let agent_id = "agent-settle-error";

    seed(&domain, &ctx, agent_id, "m-error", "错误素材".into(), 0).await;

    let err = drive(
        Arc::clone(&domain),
        ctx.clone(),
        make_agent(agent_id, None),
        Arc::new(FailingBackend),
    )
    .await
    .expect_err("backend failure must surface");
    assert!(
        format!("{err}").contains("settle boom"),
        "unexpected error: {err}"
    );
    assert_eq!(
        status_of(&domain, &ctx, agent_id, "m-error").await,
        MemoryStatus::Active,
        "失败批次不得翻状态"
    );
}

/// 标记未生效（`Ok(0)`）必须阻断本轮，**不得回环重打同一批**
///
/// ⚠️ 本模块最重要的护栏：`mark_short_term_settled` 会把 `update_short_term_index`
/// 的失败吞成 `Ok(0)`（内部只 `log_warn`），修复前 settle 节点不看 `marked` 就
/// `SIGNAL_NEXT`，于是同一批 Active 记忆被反复取回重打模型，直到
/// `MAX_SETTLE_BATCHES` 打满 → `ensure_finalized` 报错 → nack 重投雪崩。
#[sqlx::test]
async fn mark_zero_blocks_run_instead_of_looping(pool: SqlitePool) {
    let ctx = init_env(pool).await;
    let domain = test_domain();
    let agent_id = "agent-settle-mark-zero";

    for i in 0..2 {
        seed(
            &domain,
            &ctx,
            agent_id,
            &format!("mz-{i}"),
            format!("素材{i}"),
            i,
        )
        .await;
    }

    // 抢先标记本批全部 → settle 节点再标记时 `still_active` 为空 → `Ok(0)`
    let backend = Arc::new(PreSettledBackend {
        pre_mark: None,
        calls: AtomicUsize::new(0),
        last_batch_len: AtomicUsize::new(0),
    });
    let outcome = drive(
        Arc::clone(&domain),
        ctx.clone(),
        make_agent(agent_id, None),
        Arc::clone(&backend) as Arc<dyn SettleBackend>,
    )
    .await
    .expect("blocked run still reaches finalize");

    // 只打了一次模型就停：没有回环把同一批重打 64 遍
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        outcome.fingerprint,
        format!("{NODE_PREPARE}#1>{NODE_SETTLE}#1>{NODE_FINALIZE}#1")
    );
    assert_eq!(outcome.blocked.as_deref(), Some(MARK_SETTLE_FAILED));
    // 不得谎报：一条都没被本轮标记成功，就不能算进 memories
    assert_eq!(outcome.settled_count, 0);
    assert_eq!(outcome.batches, 0);
}

/// 部分标记（`Ok(marked) < ids.len()`）同样阻断，且只统计**真正标记**的条数
#[sqlx::test]
async fn partial_mark_blocks_run_and_counts_only_marked(pool: SqlitePool) {
    let ctx = init_env(pool).await;
    let domain = test_domain();
    let agent_id = "agent-settle-partial-mark";

    for i in 0..2 {
        seed(
            &domain,
            &ctx,
            agent_id,
            &format!("mp-{i}"),
            format!("素材{i}"),
            i,
        )
        .await;
    }

    // 只抢先标记第 1 条 → settle 节点再标记时只剩第 2 条 → marked=1 < items=2
    let backend = Arc::new(PreSettledBackend {
        pre_mark: Some(1),
        calls: AtomicUsize::new(0),
        last_batch_len: AtomicUsize::new(0),
    });
    let outcome = drive(
        Arc::clone(&domain),
        ctx.clone(),
        make_agent(agent_id, None),
        Arc::clone(&backend) as Arc<dyn SettleBackend>,
    )
    .await
    .expect("blocked run still reaches finalize");

    // 先钉住批次大小，避免靠猜（两条短摘要远在预算内，应同批）
    assert_eq!(
        backend.last_batch_len.load(Ordering::SeqCst),
        2,
        "本批应含 2 条"
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(outcome.blocked.as_deref(), Some(MARK_SETTLE_FAILED));
    // 顺序无关：两条最终都 Settled（1 条被后端抢先标记、1 条被 settle 节点标记），
    // 证明本轮跑完就干净收尾、没有把同一批反复取回重打。
    for id in ["mp-0", "mp-1"] {
        assert_eq!(
            status_of(&domain, &ctx, agent_id, id).await,
            MemoryStatus::Settled,
            "{id} 应已沉淀"
        );
    }
    // 只累加**真正翻掉状态**的 1 条（节点自己标记的），而不是本批的 2 条
    assert_eq!(outcome.settled_count, 1);
    // 阻断分支不计入 batches（与成功批次区分）
    assert_eq!(outcome.batches, 0);
}
