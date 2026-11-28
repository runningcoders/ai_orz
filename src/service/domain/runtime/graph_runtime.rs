//! 图级运行总线实例
//!
//! 站在整张图的视角持有跨节点共享状态，并把独立功能模块包装成自己的方法：
//! 节点只传本次任务需要的参数（如 `judge_ctx` / `brain`），共享数据（锚点、游标、
//! 预算总账、已判定集合）由运行时自己从总线取出。这样节点不必接触
//! `Arc<Mutex<InboxState>>` 这类实现细节，也不必依赖上层装配。
//!
//! ⚠️ 这里是**图内**共享面（生命周期随图）；出图即消费的数据仍然走返回值
//! （见 `runtime_data_bus_design.md` 的「读者在哪，载体就在哪」）。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use common::error::{Result, err};

use super::RoundCollected;
use super::inbox::{CollectOutcome, InboxState, InjectedMessage, collect};
use super::inbox_relevance::InboxJudgeContext;
use crate::models::brain::Brain;
use crate::models::cortex_types::ChatMessage;
use crate::models::produced_resource::ProducedResource;
use crate::pkg::request_context::RequestContext;

/// run 级检索状态：跨节点累积，visited 去重的唯一真相
#[derive(Debug, Default, Clone)]
pub(crate) struct RetrievalState {
    /// 已检索过的词（含 intent key_terms 与历次 discover 新词）
    pub queried_terms: Vec<String>,
    /// discover 已执行的循环次数（软预算，蓝图预算是硬护栏）
    pub discovery_rounds: usize,
}

/// 跨装配轮次共享的压缩状态
///
/// 压缩产物要喂给下一轮 `Prepare`（且下一轮不再查历史记忆），是跨节点共享槽位。
/// 定义从 `legacy_awaken::context` 下沉到这里，与 `RetrievalState` 同理：
/// 消除 runtime → legacy_awaken 的反向依赖。
#[derive(Debug, Default, Clone)]
pub(crate) struct CompactionSlot {
    /// 上一轮工作压缩结果
    pub summary: Option<String>,
    /// 压缩落库的那条记忆 id（组装「更早的记忆」时要排除）
    pub memory_id: Option<String>,
}

/// 图级运行总线实例：跨节点共享状态的所有者 + 功能模块的总线入口
///
/// ## 锁中毒策略（统一口径）
///
/// - **纯读取 / 快照类数据 getter**（`queried_terms` / `discovery_rounds` /
///   `compaction_slot` / `take_collected` / `inbox_*_take/_peek` 等）：中毒时
///   自愈取旧值继续。它们的共同点是「读一份可容忍不完整的副本」，让一次
///   无关线程 panic 打断整条思考链不值得。
/// - **会推进游标 / 扣预算 / 改收集器的变更入口**（`inbox_collect` 的读取、
///   `inbox_record_judged` / `inbox_advance_*` / `inbox_absorptions`）：中毒时
///   返回 `Err`。这类状态一旦不确定就可能导致重复吸收或预算错乱，必须让
///   调用方显式失败，而不是静默带着坏状态继续。
pub(crate) struct GraphRuntime {
    inbox: Arc<Mutex<InboxState>>,
    /// run 级业务产出收集器：dispatch 写入、跨周期累计、随结果移交消费侧
    collected: Arc<Mutex<RoundCollected>>,
    /// 小脑判定 DAL：`None` = 生产走真实单例；`Some` = 测试注入脚本替身
    /// ⚠️ 只为隔离外部模型，非跨节点共享数据；不参与 Debug 输出
    judge_dal: Option<Arc<dyn crate::service::dal::brain::BrainDal>>,
    /// run 级检索状态：visited 词集合 + discover 轮次软预算
    retrieval: Mutex<RetrievalState>,
    /// 本 run 真正发起检索的次数（短路不计）
    retrieve_searches: AtomicUsize,
    /// 跨装配轮次共享的压缩槽位：CompactNode 写、Prepare/Business 读
    compaction: Mutex<CompactionSlot>,
    /// Phase 1 初始消息快照槽：intent 写、Prepare 读走、poll 偷看
    intent_seed: Mutex<Option<Vec<ChatMessage>>>,
}

impl std::fmt::Debug for GraphRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphRuntime")
            .field("inbox", &self.inbox)
            .field("collected", &self.collected)
            .field(
                "judge_dal",
                &self.judge_dal.as_ref().map(|_| "<dyn BrainDal>"),
            )
            .field("retrieval", &self.retrieval)
            .field("retrieve_searches", &self.retrieve_searches)
            .field("compaction", &self.compaction)
            .finish()
    }
}

impl GraphRuntime {
    pub(crate) fn new(
        inbox: Arc<Mutex<InboxState>>,
        collected: Arc<Mutex<RoundCollected>>,
    ) -> Self {
        Self {
            inbox,
            collected,
            judge_dal: None,
            retrieval: Mutex::new(RetrievalState::default()),
            retrieve_searches: AtomicUsize::new(0),
            compaction: Mutex::new(CompactionSlot::default()),
            intent_seed: Mutex::new(None),
        }
    }

    /// 已检索词快照（差集输入）；锁中毒取旧值继续
    pub(crate) fn queried_terms(&self) -> Vec<String> {
        match self.retrieval.lock() {
            Ok(state) => state.queried_terms.clone(),
            Err(poisoned) => poisoned.into_inner().queried_terms.clone(),
        }
    }

    /// 追加本次真实检索的新词（锁中毒自愈）
    pub(crate) fn record_queried_terms(&self, terms: Vec<String>) {
        match self.retrieval.lock() {
            Ok(mut state) => state.queried_terms.extend(terms),
            Err(poisoned) => poisoned.into_inner().queried_terms.extend(terms),
        }
    }

    /// discover 轮次快照（软预算判据）
    pub(crate) fn discovery_rounds(&self) -> usize {
        match self.retrieval.lock() {
            Ok(state) => state.discovery_rounds,
            Err(poisoned) => poisoned.into_inner().discovery_rounds,
        }
    }

    /// discover 成功一轮后递增软预算计数
    pub(crate) fn note_discovery_round(&self) {
        match self.retrieval.lock() {
            Ok(mut state) => state.discovery_rounds += 1,
            Err(poisoned) => poisoned.into_inner().discovery_rounds += 1,
        }
    }

    /// 记一次真实检索（差集为空的短路分支不调用）；返回累计次数
    pub(crate) fn note_retrieve_search(&self) -> usize {
        self.retrieve_searches.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub(crate) fn retrieve_search_count(&self) -> usize {
        self.retrieve_searches.load(Ordering::SeqCst)
    }

    /// 测试准备：直接设定已检索词（仅集成测试用，跳过差集推导）
    #[cfg(test)]
    pub(crate) fn set_queried_terms(&self, terms: Vec<String>) {
        if let Ok(mut state) = self.retrieval.lock() {
            state.queried_terms = terms;
        }
    }

    /// 测试准备：直接设定 discover 轮次（仅集成测试用）
    #[cfg(test)]
    pub(crate) fn set_discovery_rounds(&self, rounds: usize) {
        if let Ok(mut state) = self.retrieval.lock() {
            state.discovery_rounds = rounds;
        }
    }

    /// 注入小脑判定替身（仅集成测试用；生产恒 `None`）
    #[cfg(test)]
    pub(crate) fn set_judge_dal(&mut self, dal: Arc<dyn crate::service::dal::brain::BrainDal>) {
        self.judge_dal = Some(dal);
    }

    /// dispatch 写入本轮产出（空批次零开销）
    pub(crate) fn record_resources(&self, produced: Vec<ProducedResource>) {
        match self.collected.lock() {
            Ok(mut collected) => collected.record_resources(produced),
            // 锁中毒时收集器仍可读：产出是业务数据，宁可带着可能不完整的
            // 清单继续，也不静默丢弃（与迁移前 `deps.collected` 口径一致）
            Err(poisoned) => poisoned.into_inner().record_resources(produced),
        }
    }

    /// 图结束后取一份产出副本随结果移交（此后不再有写入者）
    pub(crate) fn take_collected(&self) -> RoundCollected {
        match self.collected.lock() {
            Ok(collected) => collected.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// poll 挂点共用的吸收次数（回 intent 与尾插共用一个额度）
    pub(crate) fn inbox_absorptions(&self) -> Result<usize> {
        let state = self
            .inbox
            .lock()
            .map_err(|_| err!(Internal, "inbox state lock poisoned"))?;
        Ok(state.absorptions())
    }

    /// 本唤醒剩余的注入 token 预算（`None` = 无上限）
    ///
    /// 挂点早退用：预算已归零时不必再查库、不必再调小脑判定。
    pub(crate) fn inbox_remaining_tokens(&self) -> Result<Option<usize>> {
        let state = self
            .inbox
            .lock()
            .map_err(|_| err!(Internal, "inbox state lock poisoned"))?;
        Ok(state.remaining_inbox_tokens())
    }

    /// inbox 模块的唯一入口：共享参数自己取，调用方只传本次判定的现场
    pub(crate) async fn inbox_collect(
        &self,
        ctx: &RequestContext,
        judge_ctx: &InboxJudgeContext,
        brain: Option<&Brain>,
    ) -> Result<CollectOutcome> {
        let (anchor, cursor, judged, remaining) = {
            let state = self
                .inbox
                .lock()
                .map_err(|_| err!(Internal, "inbox state lock poisoned"))?;
            (
                state.anchor().clone(),
                state.cursor().clone(),
                state.judged_ids().clone(),
                state.remaining_inbox_tokens(),
            )
        };
        collect(
            ctx,
            &anchor,
            &cursor,
            &judged,
            judge_ctx,
            brain,
            remaining,
            self.judge_dal.as_deref(),
        )
        .await
    }

    /// 回写本批判定结果与本次实际用量（`剩余 -= 用量` 的扣减在 state 内完成）
    pub(crate) fn inbox_record_judged(&self, judged: &[String], used_tokens: usize) -> Result<()> {
        let mut state = self
            .inbox
            .lock()
            .map_err(|_| err!(Internal, "inbox state lock poisoned"))?;
        state.note_judged(judged);
        state.spend_inbox_tokens(used_tokens);
        Ok(())
    }

    /// 回 intent：推进游标 + 记一次吸收 + 暂存增量修订输入
    pub(crate) fn inbox_advance_revision(&self, batch: Vec<InjectedMessage>) -> Result<()> {
        let mut state = self
            .inbox
            .lock()
            .map_err(|_| err!(Internal, "inbox state lock poisoned"))?;
        state.advance(&batch);
        state.note_absorption();
        state.set_revision(batch);
        Ok(())
    }

    /// 轮级 poll：只推进游标 + 记一次吸收。文本已在轮级消息里尾插，
    /// ⚠️ 不能写 `pending_turn`，否则下一轮进入 PrepareNode 时会重复渲染。
    pub(crate) fn inbox_advance_round(&self, batch: &[InjectedMessage]) -> Result<()> {
        let mut state = self
            .inbox
            .lock()
            .map_err(|_| err!(Internal, "inbox state lock poisoned"))?;
        state.advance(batch);
        state.note_absorption();
        Ok(())
    }

    /// 尾插补充：推进游标 + 记一次吸收；`poll_exit` 额外标记续跑追加
    pub(crate) fn inbox_advance_supplement(
        &self,
        batch: Vec<InjectedMessage>,
        resume_append: bool,
    ) -> Result<()> {
        let mut state = self
            .inbox
            .lock()
            .map_err(|_| err!(Internal, "inbox state lock poisoned"))?;
        state.advance(&batch);
        state.note_absorption();
        state.set_pending_turn(batch);
        if resume_append {
            state.mark_resume_append();
        }
        Ok(())
    }

    pub(crate) fn inbox_take_revision(&self) -> Option<Vec<InjectedMessage>> {
        self.inbox
            .lock()
            .ok()
            .and_then(|mut state| state.take_revision())
    }

    pub(crate) fn inbox_fallback_revision(&self, batch: Vec<InjectedMessage>) {
        if let Ok(mut state) = self.inbox.lock() {
            state.fallback_revision(batch);
        }
    }

    pub(crate) fn inbox_clear_pending_turn(&self) {
        if let Ok(mut state) = self.inbox.lock() {
            state.clear_pending_turn();
        }
    }

    /// PrepareNode 消费续跑标记与待尾插文本（读后即清，防重复渲染）
    pub(crate) fn inbox_take_resume_append_and_pending(&self) -> (bool, Vec<String>) {
        self.inbox
            .lock()
            .ok()
            .map(|mut state| {
                let resume = state.take_resume_append();
                let pending = state.pending_turn_texts();
                state.clear_pending_turn();
                (resume, pending)
            })
            .unwrap_or((false, Vec::new()))
    }

    /// 视觉装配合并用的附件引用快照
    pub(crate) fn inbox_absorbed_attachment_ids(&self) -> Vec<String> {
        self.inbox
            .lock()
            .map(|state| state.absorbed_attachment_ids().to_vec())
            .unwrap_or_default()
    }

    /// 唤醒成功后批量置 `Processed` 的消息 ID
    pub(crate) fn inbox_injected_ids(&self) -> Vec<String> {
        self.inbox
            .lock()
            .map(|state| state.injected_ids().to_vec())
            .unwrap_or_default()
    }

    // ── 压缩槽位（跨装配轮次共享，CompactNode 写 → Prepare/Business 读）──

    /// 压缩槽位快照（锁中毒取默认值继续）
    pub(crate) fn compaction_slot(&self) -> CompactionSlot {
        self.compaction
            .lock()
            .map(|slot| slot.clone())
            .unwrap_or_default()
    }

    /// 写入本轮压缩结果（锁中毒自愈）
    pub(crate) fn set_compaction_slot(&self, slot: CompactionSlot) {
        if let Ok(mut guard) = self.compaction.lock() {
            *guard = slot;
        }
    }

    // ── Phase 1 初始消息快照槽（intent 写 → Prepare 读走、poll 偷看）──

    /// 快照写回槽句柄：供意图 domain 函数写入（它仍只认 `&Mutex`，保持通用）。
    /// ⚠️ 这是执行缝回写口，不是节点直接读写的共享数据，故暴露句柄即可。
    pub(crate) fn intent_seed_slot(&self) -> &Mutex<Option<Vec<ChatMessage>>> {
        &self.intent_seed
    }

    /// 读走 Phase 1 快照（PrepareNode 装配用，读后即清防重复渲染）
    pub(crate) fn take_intent_seed(&self) -> Option<Vec<ChatMessage>> {
        self.intent_seed.lock().ok().and_then(|mut s| s.take())
    }

    /// 偷看 Phase 1 快照副本（poll 抽取判定上下文用，不清空）
    pub(crate) fn intent_seed_messages(&self) -> Option<Vec<ChatMessage>> {
        self.intent_seed.lock().ok().and_then(|s| s.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::domain::runtime::inbox::InboxAnchor;
    use common::enums::{MessageRole, MessageType};

    fn anchor() -> InboxAnchor {
        InboxAnchor {
            agent_id: "agent-1".to_string(),
            from_id: "sender-1".to_string(),
            from_role: MessageRole::User,
            project_id: None,
            task_id: None,
            thread_id: None,
            message_type: MessageType::Text,
            created_at: 1000,
            id: "msg-anchor".to_string(),
            content: "帮我看一下登录页".to_string(),
            organization_id: None,
        }
    }

    fn injected(id: &str, created_at: i64, content: &str) -> InjectedMessage {
        InjectedMessage {
            id: id.to_string(),
            created_at,
            content: content.to_string(),
            attachment_ids: Vec::new(),
        }
    }

    fn runtime(budget: Option<usize>) -> (Arc<GraphRuntime>, Arc<Mutex<InboxState>>) {
        let mut state = InboxState::new(anchor());
        state.set_remaining_inbox_tokens(budget);
        let inbox = Arc::new(Mutex::new(state));
        (
            Arc::new(GraphRuntime::new(
                Arc::clone(&inbox),
                RoundCollected::shared(),
            )),
            inbox,
        )
    }

    #[test]
    fn record_judged_spends_shared_budget() {
        let (runtime, inbox) = runtime(Some(120));
        runtime
            .inbox_record_judged(&["m1".to_string(), "m2".to_string()], 45)
            .expect("record");
        // 先释放 state 的锁再调用 runtime 方法：`inbox_absorptions` 会再取同一把
        // Mutex，持锁调用会自死锁（std Mutex 不可重入）。
        {
            let state = inbox.lock().expect("lock");
            assert_eq!(state.remaining_inbox_tokens(), Some(75));
            assert_eq!(state.judged_ids().len(), 2);
        }
        assert_eq!(runtime.inbox_absorptions().expect("absorptions"), 0);
    }

    #[test]
    fn revision_path_keeps_batch_for_intent_node() {
        let (runtime, inbox) = runtime(Some(100));
        runtime
            .inbox_advance_revision(vec![injected("m1", 1001, "改一下按钮颜色")])
            .expect("advance");
        assert_eq!(runtime.inbox_absorptions().expect("absorptions"), 1);
        let batch = runtime.inbox_take_revision().expect("revision");
        assert_eq!(batch.len(), 1);
        assert!(
            runtime.inbox_take_revision().is_none(),
            "取走后不再重复修订"
        );
        assert!(runtime.inbox_absorbed_attachment_ids().is_empty());
        assert_eq!(inbox.lock().expect("lock").cursor().id, "m1");
    }

    #[test]
    fn round_advance_does_not_pollute_pending_turn() {
        let (runtime, _) = runtime(None);
        runtime
            .inbox_advance_round(&[injected("m1", 1001, "补充一句")])
            .expect("advance");
        let (resume, pending) = runtime.inbox_take_resume_append_and_pending();
        assert_eq!(
            (resume, pending.len()),
            (false, 0),
            "轮级尾插不进 pending_turn"
        );
        assert_eq!(runtime.inbox_absorptions().expect("absorptions"), 1);
        assert_eq!(runtime.inbox_injected_ids(), vec!["m1".to_string()]);
    }

    #[test]
    fn supplement_on_exit_marks_resume_append() {
        let (runtime, _) = runtime(None);
        runtime
            .inbox_advance_supplement(vec![injected("m2", 1002, "再改一处")], true)
            .expect("advance");
        let (resume, pending) = runtime.inbox_take_resume_append_and_pending();
        assert!(resume);
        assert_eq!(pending, vec!["（补充输入 #1）\n再改一处".to_string()]);
        let (resume_again, pending_again) = runtime.inbox_take_resume_append_and_pending();
        assert!(!resume_again, "读取即清空");
        assert!(pending_again.is_empty());
    }

    #[test]
    fn collected_records_and_hands_off_in_order() {
        let (runtime, _) = runtime(None);
        let resource = |id: &str| ProducedResource {
            scheme: "attachment",
            id: id.to_string(),
            name: format!("{id}.png"),
        };
        runtime.record_resources(vec![resource("att_1")]);
        runtime.record_resources(vec![resource("att_2")]);

        let handed_off = runtime.take_collected();
        assert_eq!(handed_off.resources.len(), 2, "跨周期写入累加");
        assert_eq!(handed_off.resources[0].id, "att_1");
        assert_eq!(handed_off.resources[1].id, "att_2", "移交保持产出顺序");
    }

    #[test]
    fn compaction_slot_writes_then_reads_snapshot() {
        let (runtime, _) = runtime(None);
        assert!(runtime.compaction_slot().summary.is_none());
        runtime.set_compaction_slot(super::CompactionSlot {
            summary: Some("压缩摘要".to_string()),
            memory_id: Some("mem-1".to_string()),
        });
        let slot = runtime.compaction_slot();
        assert_eq!(slot.summary.as_deref(), Some("压缩摘要"));
        assert_eq!(slot.memory_id.as_deref(), Some("mem-1"));
    }

    #[test]
    fn intent_seed_take_clears_but_peek_does_not() {
        let (runtime, _) = runtime(None);
        let seed = crate::models::cortex_types::ChatMessage::user("意图前缀");
        *runtime.intent_seed_slot().lock().expect("lock") = Some(vec![seed]);
        assert_eq!(runtime.intent_seed_messages().expect("peek").len(), 1);
        assert_eq!(
            runtime.intent_seed_messages().expect("peek again").len(),
            1,
            "偷看不清空"
        );
        let taken = runtime.take_intent_seed().expect("take");
        assert_eq!(taken.len(), 1);
        assert!(runtime.take_intent_seed().is_none(), "读走即清空");
    }

    #[test]
    fn retrieval_state_accumulates_terms_and_rounds() {
        let (runtime, _) = runtime(None);
        assert!(runtime.queried_terms().is_empty());
        assert_eq!(runtime.discovery_rounds(), 0);
        assert_eq!(runtime.retrieve_search_count(), 0);

        runtime.record_queried_terms(vec!["甲".to_string(), "乙".to_string()]);
        runtime.record_queried_terms(vec!["丙".to_string()]);
        assert_eq!(
            runtime.queried_terms(),
            vec!["甲".to_string(), "乙".to_string(), "丙".to_string()],
            "跨轮追加保持顺序"
        );
        runtime.note_discovery_round();
        runtime.note_discovery_round();
        assert_eq!(runtime.discovery_rounds(), 2);
        assert_eq!(runtime.note_retrieve_search(), 1, "返回自增后的累计次数");
        assert_eq!(runtime.note_retrieve_search(), 2);
        assert_eq!(runtime.retrieve_search_count(), 2);
    }
}
