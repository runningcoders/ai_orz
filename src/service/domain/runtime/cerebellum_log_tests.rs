//! 小脑空请求链路的日志断言（笔③-2 · 只动测试面）
//!
//! 断言对象＝笔③-1（`ecb450c5`）在两个 `relevance` 场景的 `requests.is_empty()`
//! 早退分支新增的 `log_info!`：日志是否真的被打出来、字段取值是否等于业务输入
//! 派生值。取的是**原始值**（`candidates` / `usable` / `dropped`），不是只断差值——
//! 只断差值的断言在 `dropped` 被写死时也会绿。
//!
//! # 为什么单独一个测试文件
//!
//! 仓内约定是「每个 `*_tests.rs` 挂在各自生产模块内 + `use super::*`」，跨模块
//! 复用不到彼此的私有夹具（`BrainDal` 六个方法没有默认实现）。本文件挂在
//! `runtime` 下、自带一份最小 `MockBrainDal` ⇒ **零生产文件改动**（唯一改动是
//! `mod` 注册）。抬 `pub(crate) mod tests;` 也能复用夹具，但那要动生产文件。
//!
//! # 捕获器为什么可信（证据①的自我证明）
//!
//! 断言「某条日志存在」的前提是捕获器本身有效。所以第一个用例先用一条**既有
//! 生产日志**（`cerebellum call budget exhausted, fall back to original logic`，
//! 配额耗尽分支）证明捕获器能看见日志，再在同一套装置上断新增点位。
//!
//! 触发手法：`CerebellumQuota::new(1)` ＋ 测试内先手动 `try_acquire()` 一次
//! （`used` 0→1）⇒ 被测路径内 `try_acquire()` 返回 false ⇒ 确定性命中。
//! ⚠️ `CerebellumQuota::new(0)` 是**不限**（`limit == 0` 分支 `try_acquire()`
//! 恒真），不能用来触发这条日志。
//!
//! # 不可达点位
//!
//! `thinking_strength::select` 的 `requests.is_empty()` 分支在现配置下**结构不可达**
//! （`LEVELS` 是 11 档编译期常量，`PickOnePolicy::request` 只在选项为空时返回
//! None）⇒ 该点位**不在本文件断言范围内**，也不计入「已测」；它只由代码审查保证。
//!
//! # 并行安全
//!
//! `tracing::subscriber::with_default` 是 **thread-local** 覆盖，全程不碰
//! `set_global_default` ⇒ 其它测试文件在别的线程上仍走进程全局缺省，互不污染。
//! 被测的三条早退路径内部**无 `tokio::spawn`**（纯同步，在唯一 async 点
//! `dal.decide` 之前），future 全程在本线程 poll ⇒ 日志必然落在捕获线程上。

use super::thinking_strength;
use crate::models::brain::{Brain, CerebellumQuota};
use crate::models::memory::{LongTermKnowledgeNodePo, Memory, MemoryPo};
use crate::models::model_provider::ModelProviderPo;
use crate::models::skill::SkillPo;
use crate::pkg::RequestContext;
use crate::pkg::policy::{DecisionAnswers, DecisionRequest};
use crate::service::dal::brain::{BrainDal, DecisionOutcome};
use crate::service::domain::runtime::legacy_awaken::{memory_relevance, skill_relevance};
use async_trait::async_trait;
use common::api::cerebellum::{CerebellumUsage, DegradedReason};
use common::enums::{MemoryStatus, ThinkingScene};
use common::error::Result;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// ==================== 捕获器 ====================

/// 把 `tracing` 的输出收进内存缓冲（`Arc<Mutex<Vec<u8>>>` ⇒ `Clone` 后共享同一份）
#[derive(Clone, Default)]
struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl CaptureWriter {
    fn text(&self) -> String {
        let buf = self.0.lock().expect("capture lock");
        String::from_utf8_lossy(buf.as_slice()).into_owned()
    }

    /// 只取「含目标关键字的那一行」——不断日志总条数（并行/顺序变化会让总数抖动）
    fn lines_with(&self, needle: &str) -> Vec<String> {
        self.text()
            .lines()
            .filter(|line| line.contains(needle))
            .map(|line| line.to_string())
            .collect()
    }
}

impl std::io::Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("capture lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn capture_subscriber(writer: CaptureWriter) -> impl tracing::Subscriber + Send + Sync {
    tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .finish()
}

/// 手搓 `current_thread` runtime：`#[tokio::test]` 内不能 block_on，普通 `#[test]`
/// 里自建 rt 才能让 future 在本线程 poll（thread-local subscriber 才捕获得到日志）
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build current-thread runtime")
        .block_on(fut)
}

/// 断言失败时打印的诊断片段（避免把整个日志缓冲塞进 panic message）
fn head(text: &str) -> String {
    match text.char_indices().nth(1500) {
        Some((idx, _)) => format!("{}…", &text[..idx]),
        None => text.to_string(),
    }
}

// ==================== mock BrainDal ====================

/// 只计数的 `BrainDal`：本文件所有用例都应停在 `decide` **之前**
///
/// 刻意不用 `unimplemented!()` 当护栏 —— 一旦被测路径意外触达，`calls` 会变成 1，
/// 让「零请求」断言直接变红并保留同用例内其它断言的结论；`panic!` 会把同用例内
/// 其余断言的结论一起吞掉，反而更难定位。
struct MockBrainDal {
    calls: AtomicUsize,
}

impl MockBrainDal {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl BrainDal for MockBrainDal {
    async fn decide(
        &self,
        _ctx: RequestContext,
        _brain: &Brain,
        _requests: Vec<DecisionRequest>,
    ) -> DecisionOutcome {
        self.calls.fetch_add(1, Ordering::Relaxed);
        DecisionOutcome {
            answers: DecisionAnswers::new(),
            degraded: Some(DegradedReason::CallError),
            latency_ms: 0,
            usage: CerebellumUsage::default(),
        }
    }

    async fn wake_brain(
        &self,
        _ctx: RequestContext,
        _agent: &crate::models::agent::AgentPo,
        _memories: Vec<Memory>,
    ) -> Result<Brain> {
        unimplemented!("not needed by cerebellum log tests")
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _provider: &crate::models::model_provider::ModelProvider,
        _prompt: &str,
    ) -> Result<String> {
        unimplemented!("not needed by cerebellum log tests")
    }

    async fn think(
        &self,
        _ctx: RequestContext,
        _brain: &Brain,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[crate::models::cortex_types::ToolDescriptor],
        _options: &crate::models::cortex_types::ModelCallOptions,
    ) -> Result<crate::models::cortex_types::ThinkResult> {
        unimplemented!("not needed by cerebellum log tests")
    }

    async fn embed_entity(
        &self,
        _ctx: RequestContext,
        _entity: &dyn crate::models::vector::Vectorizable,
    ) -> Result<Option<crate::models::vector::VectorIndexParams>> {
        Ok(None)
    }

    async fn embed_text_for_search(
        &self,
        _ctx: RequestContext,
        _text: &str,
    ) -> Result<Option<crate::models::vector::VectorIndexParams>> {
        Ok(None)
    }
}

// ==================== 夹具 ====================

fn mock_provider() -> ModelProviderPo {
    ModelProviderPo::new(
        "Jev Cerebellum".to_string(),
        common::enums::ProviderType::Jev,
        common::enums::ModelCapability::Decision,
        "jev-latest".to_string(),
        "test-key".to_string(),
        None,
        None,
        "test".to_string(),
    )
}

/// 带 per-run 配额的 Brain（与生产装配口径一致：配额挂在 Brain 上）
fn brain_with_quota(limit: usize) -> Brain {
    let mut brain = Brain::new_local(
        "agent-1".to_string(),
        "Test Agent".to_string(),
        Default::default(),
        mock_provider(),
        vec![],
    );
    brain.cerebellum_quota = Arc::new(CerebellumQuota::new(limit));
    brain
}

fn test_ctx() -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx_from_global("test-user")
}

/// 长期知识节点候选：`desc` 为空且 `summary` 为空 ⇒ 判据文本为空 ⇒ 不进选项
fn node(id: &str, name: &str, desc: &str) -> Memory {
    Memory::new(MemoryPo::KnowledgeNode(LongTermKnowledgeNodePo {
        id: id.to_string(),
        agent_id: "agent-test".to_string(),
        node_name: name.to_string(),
        node_description: desc.to_string(),
        node_type: "concept".to_string(),
        summary: String::new(),
        tags: "[]".to_string(),
        user_tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: 0,
        updated_at: 0,
    }))
}

/// 方法技能候选：`name` / `description` / `category` 全空 ⇒ 判据文本为空 ⇒ 不进选项
fn skill(id: &str, name: &str, desc: &str, category: &str) -> SkillPo {
    SkillPo {
        id: id.to_string(),
        name: name.to_string(),
        description: desc.to_string(),
        category: category.to_string(),
        ..Default::default()
    }
}

// ==================== 证据①：捕获器自证非空 ====================

/// 捕获器先在**既有生产日志**上自证有效，再谈新增点位
///
/// 手法四件：`brain_with_quota(1)` ／ 测试内预耗尽一次（`used` 0→1）／
/// `limit=1`、`used=1` 可断 ／ 日志原文逐字匹配。
#[test]
fn capture_writer_self_proves_with_the_existing_quota_exhausted_log() {
    let writer = CaptureWriter::default();
    let subscriber = capture_subscriber(writer.clone());

    let (selection, calls) = tracing::subscriber::with_default(subscriber, || {
        block_on(async {
            crate::pkg::storage::test_support::init_for_test().await;
            let ctx = test_ctx();
            let brain = brain_with_quota(1);
            // 预耗尽唯一额度：used 0 → 1，被测路径内的 try_acquire() 必然 false
            assert!(brain.cerebellum_quota.try_acquire(), "首次占用必须成功");
            assert_eq!(brain.cerebellum_quota.used(), 1);
            let mock = Arc::new(MockBrainDal::new());
            let dal: Arc<dyn BrainDal> = mock.clone();
            let selection = thinking_strength::select(
                &ctx,
                dal,
                &brain,
                ThinkingScene::Awaken,
                Some("今天天气怎么样"),
                thinking_strength::StrengthNodeContext::default(),
            )
            .await;
            (selection, mock.calls())
        })
    });

    let hits = writer.lines_with("cerebellum call budget exhausted, fall back to original logic");
    assert_eq!(
        hits.len(),
        1,
        "既有生产日志应恰被打出一次，实际捕获：\n{}",
        head(&writer.text())
    );
    let line = &hits[0];
    assert!(line.contains("limit=1"), "配额上限字段缺失：{line}");
    assert!(line.contains("used=1"), "已用次数字段缺失：{line}");

    // 返回值同口径：仍是那条零调用早退
    assert!(selection.choice.is_none());
    assert_eq!(selection.latency_ms, None);
    assert_eq!(selection.degraded, Some(thinking_strength::DEGRADED_BUDGET));
    assert_eq!(calls, 0, "配额早退一次请求都不该发");
}

// ==================== 证据②：memory_relevance 早退日志的原始值 ====================

/// 正样本：`candidates=3` / `usable=1` / `dropped=2`（并列断三个原始值，不只断差值）
#[test]
fn memory_rerank_early_exit_logs_dropped_raw_values() {
    let writer = CaptureWriter::default();
    let subscriber = capture_subscriber(writer.clone());

    let (kept, latency, calls) = tracing::subscriber::with_default(subscriber, || {
        block_on(async {
            crate::pkg::storage::test_support::init_for_test().await;
            let ctx = test_ctx();
            let brain = brain_with_quota(0);
            let mock = Arc::new(MockBrainDal::new());
            let dal: Arc<dyn BrainDal> = mock.clone();
            // 3 条候选：2 条的 node_description 与 summary 都空 ⇒ 判据文本为空
            let candidates = vec![
                node("m1", "订单状态机", "订单从创建到完成"),
                node("m2", "空描述节点甲", ""),
                node("m3", "空描述节点乙", ""),
            ];
            let (kept, latency) =
                memory_relevance::rank(&ctx, dal, &brain, "订单怎么流转", &candidates, 5).await;
            (kept, latency, mock.calls())
        })
    });

    assert!(kept.is_none(), "早退必须保持老路径（None）");
    assert_eq!(latency, None, "零调用 ⇒ 轨迹耗时 None");
    assert_eq!(calls, 0, "候选不足 ⇒ 一次请求都不该发");

    let hits = writer.lines_with("memory rerank asked nothing, keep original order");
    assert_eq!(
        hits.len(),
        1,
        "早退日志应恰被打出一次，实际捕获：\n{}",
        head(&writer.text())
    );
    let line = &hits[0];
    assert!(
        line.contains("decision=\"empty_requests\""),
        "decision 字段：{line}"
    );
    assert!(line.contains("candidates=3"), "candidates 原始值：{line}");
    assert!(line.contains("usable=1"), "usable 原始值：{line}");
    assert!(line.contains("dropped=2"), "dropped 原始值：{line}");
    assert!(line.contains("keep=5"), "keep 入参：{line}");
}

/// 反样本：`candidates=1` / `usable=1` / `dropped=0`，且仍早退（`usable < 2`）
///
/// 这条是「`dropped` 被写死成 0 也会绿」的解药：它与正样本并列，两向都要成立。
#[test]
fn memory_rerank_early_exit_logs_zero_dropped_when_all_candidates_usable() {
    let writer = CaptureWriter::default();
    let subscriber = capture_subscriber(writer.clone());

    let (kept, latency, calls) = tracing::subscriber::with_default(subscriber, || {
        block_on(async {
            crate::pkg::storage::test_support::init_for_test().await;
            let ctx = test_ctx();
            let brain = brain_with_quota(0);
            let mock = Arc::new(MockBrainDal::new());
            let dal: Arc<dyn BrainDal> = mock.clone();
            // 唯一候选是有效候选 ⇒ 全部可用，但选项数仍 < 2 ⇒ 早退
            let candidates = vec![node("m1", "订单状态机", "订单从创建到完成")];
            let (kept, latency) =
                memory_relevance::rank(&ctx, dal, &brain, "订单怎么流转", &candidates, 5).await;
            (kept, latency, mock.calls())
        })
    });

    assert!(kept.is_none());
    assert_eq!(latency, None);
    assert_eq!(calls, 0);

    let hits = writer.lines_with("memory rerank asked nothing, keep original order");
    assert_eq!(
        hits.len(),
        1,
        "早退日志应恰被打出一次，实际捕获：\n{}",
        head(&writer.text())
    );
    let line = &hits[0];
    assert!(line.contains("candidates=1"), "candidates 原始值：{line}");
    assert!(line.contains("usable=1"), "usable 原始值：{line}");
    assert!(line.contains("dropped=0"), "dropped 原始值：{line}");
}

// ==================== 证据②（同形）：skill_relevance 早退日志的原始值 ====================

/// 正样本：`candidates=2` / `usable=1` / `dropped=1`
#[test]
fn skill_rerank_early_exit_logs_dropped_raw_values() {
    let writer = CaptureWriter::default();
    let subscriber = capture_subscriber(writer.clone());

    let (kept, latency, reason, calls) = tracing::subscriber::with_default(subscriber, || {
        block_on(async {
            crate::pkg::storage::test_support::init_for_test().await;
            let ctx = test_ctx();
            let brain = brain_with_quota(0);
            let mock = Arc::new(MockBrainDal::new());
            let dal: Arc<dyn BrainDal> = mock.clone();
            // 2 条候选：1 条的 name/description/category 全空 ⇒ 判据文本为空
            let skills = vec![
                skill("s1", "反过度工程", "评估该不该写这么多", "coding_standards"),
                skill("s2", "", "", ""),
            ];
            let (kept, latency, reason) =
                skill_relevance::rank(&ctx, dal, &brain, "要不要加这一层抽象", &skills, 2).await;
            (kept, latency, reason, mock.calls())
        })
    });

    assert!(kept.is_none(), "早退必须保持老路径（None）");
    assert_eq!(latency, None, "零调用 ⇒ 轨迹耗时 None");
    assert_eq!(
        reason.as_deref(),
        Some("empty_requests"),
        "判据被剔到只剩 1 条属「问都没问出去」，不能折叠成无可用答案"
    );
    assert_eq!(calls, 0);

    let hits = writer.lines_with("skill rerank asked nothing, keep original order");
    assert_eq!(
        hits.len(),
        1,
        "早退日志应恰被打出一次，实际捕获：\n{}",
        head(&writer.text())
    );
    let line = &hits[0];
    assert!(
        line.contains("decision=\"empty_requests\""),
        "decision 字段：{line}"
    );
    assert!(line.contains("candidates=2"), "candidates 原始值：{line}");
    assert!(line.contains("usable=1"), "usable 原始值：{line}");
    assert!(line.contains("dropped=1"), "dropped 原始值：{line}");
    assert!(line.contains("keep=2"), "keep 入参：{line}");
}

/// 反样本：`candidates=1` / `usable=1` / `dropped=0`
#[test]
fn skill_rerank_early_exit_logs_zero_dropped_when_all_candidates_usable() {
    let writer = CaptureWriter::default();
    let subscriber = capture_subscriber(writer.clone());

    let (kept, latency, reason, calls) = tracing::subscriber::with_default(subscriber, || {
        block_on(async {
            crate::pkg::storage::test_support::init_for_test().await;
            let ctx = test_ctx();
            let brain = brain_with_quota(0);
            let mock = Arc::new(MockBrainDal::new());
            let dal: Arc<dyn BrainDal> = mock.clone();
            let skills = vec![skill(
                "s1",
                "反过度工程",
                "评估该不该写这么多",
                "coding_standards",
            )];
            let (kept, latency, reason) =
                skill_relevance::rank(&ctx, dal, &brain, "要不要加这一层抽象", &skills, 2).await;
            (kept, latency, reason, mock.calls())
        })
    });

    assert!(kept.is_none());
    assert_eq!(latency, None);
    assert_eq!(
        reason.as_deref(),
        Some("empty_requests"),
        "单候选的直接放行由调用方处理，rank 仍须如实回报没发请求"
    );
    assert_eq!(calls, 0);

    let hits = writer.lines_with("skill rerank asked nothing, keep original order");
    assert_eq!(
        hits.len(),
        1,
        "早退日志应恰被打出一次，实际捕获：\n{}",
        head(&writer.text())
    );
    let line = &hits[0];
    assert!(line.contains("candidates=1"), "candidates 原始值：{line}");
    assert!(line.contains("usable=1"), "usable 原始值：{line}");
    assert!(line.contains("dropped=0"), "dropped 原始值：{line}");
}
