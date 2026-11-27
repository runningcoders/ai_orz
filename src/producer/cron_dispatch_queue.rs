//! Cron 触发器的进程内优先级派发队列。
//!
//! DB 仍是唯一权威源；本队列只做「提前唤醒」：DB 兜底周期性把「未来 5 分钟内
//! 到期」的触发器刷新进队列，秒级 tick 逐个取出到期项，由业务层按 id 查 DB
//! 校验（时间点 / 版本是否仍对应）后才领取执行。
//!
//! 排序 / 去重 / 惰性失效复用 [`SharedPriorityQueue`]；本层只定义场景条目
//! （`trigger_id → next_run_at` + DB `updated_at` 版本）与「到期」判定。
//! 版本有**两道防线**：入队门禁（基建层 `upsert_if_newer`）挡住乱序到达的旧
//! 事件；出队时版本随载荷带出，业务层按 id 回查 DB 再校验一次（防事件丢失 /
//! DB 被外部改动），见 `CronTriggerProducer::dispatch_due`。版本不参与排序。
//! 队列随 `CronTriggerProducer` 实例创建、销毁，不持久化、不跨实例。

use crate::pkg::priority_queue::{RankedEntry, SharedPriorityQueue, VersionedEntry};
use std::cmp::Reverse;

#[derive(Debug, Clone)]
pub struct ScheduleEntry {
    pub trigger_id: String,
    pub next_run_at: i64,
    /// 出队快照的 DB 版本（`cron_triggers.updated_at`）。消费时业务层按 id
    /// 回查，版本或时间对不上即视为「数据已变更」跳过执行；变更事件会带
    /// 新版本重新入队。
    pub version: i64,
}

impl RankedEntry for ScheduleEntry {
    type Rank = Reverse<i64>;
    fn key(&self) -> &str {
        &self.trigger_id
    }
    fn rank(&self) -> Reverse<i64> {
        Reverse(self.next_run_at)
    }
}

impl VersionedEntry for ScheduleEntry {
    type Version = i64;
    fn version(&self) -> i64 {
        self.version
    }
}

/// Cron 派发队列（producer 私有字段）。
#[derive(Debug, Default)]
pub struct CronDispatchQueue {
    inner: SharedPriorityQueue<ScheduleEntry>,
}

impl CronDispatchQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// 批量刷新 `(trigger_id, next_run_at, db_updated_at)`：同 id 版本更旧或相等
    /// 直接拒绝（乱序事件不能把新快照倒退回去）；版本更新则走基础 `upsert` ——
    /// rank 变了追加新堆项、旧堆项出队时按 seq 惰性丢弃。
    /// 返回 rank 实际变更条数（供日志与守卫测试观测）。
    pub fn refresh(&self, entries: impl IntoIterator<Item = (String, i64, i64)>) -> usize {
        entries
            .into_iter()
            .filter(|(id, next_run_at, version)| {
                self.inner.upsert_if_newer(ScheduleEntry {
                    trigger_id: id.clone(),
                    next_run_at: *next_run_at,
                    version: *version,
                })
            })
            .count()
    }

    /// 队列内全部有效 id（排序后供 DB 查询排除，避免重复入队）。
    pub fn ids(&self) -> Vec<String> {
        self.inner.keys()
    }

    /// 逐个取出一个到期项并移除，返回完整载荷 `(id, next_run_at, version)`。
    ///
    /// 每取一个堆会自动重排；调用方循环取到返回 `None` 为止即可处理完
    /// 「当前时间点之前」的全部条目，无需批量上限。堆顶未到期时
    /// `pop_where` 只摘出这一项并放回，不会全堆扫描。
    pub fn pop_one_due(&self, now: i64) -> Option<(String, i64, i64)> {
        self.inner
            .pop_where(&|entry| entry.next_run_at <= now)
            .map(|(id, entry)| (id, entry.next_run_at, entry.version))
    }

    /// 移除单个 id（配置变更 / 停用后立即作废缓存的旧时间点）。
    pub fn remove(&self, id: &str) -> bool {
        self.inner.remove(id).is_some()
    }

    /// 当前有效条目数（不含惰性垃圾）。
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// 堆顶时间（跳过惰性垃圾），供日志判断下一次唤醒时间。
    pub fn next_run_at(&self) -> Option<i64> {
        self.inner.peek().map(|(_, entry)| entry.next_run_at)
    }

    /// 清空（仅供测试 / 管理面兜底）。
    pub fn clear(&self) {
        self.inner.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue() -> CronDispatchQueue {
        CronDispatchQueue::new()
    }

    #[test]
    fn refresh_dedups_and_keeps_latest_time() {
        let q = queue();
        assert_eq!(
            q.refresh([
                ("a".into(), 100, 1),
                ("b".into(), 200, 1),
                ("a".into(), 300, 2),
            ]),
            3
        );
        assert_eq!(q.len(), 2);
        assert_eq!(q.next_run_at(), Some(200));
        // 未变化的时间不重复入堆
        assert_eq!(q.refresh([("a".into(), 300, 3)]), 0);
    }

    #[test]
    fn pop_one_due_carries_version_payload() {
        let q = queue();
        q.refresh([("a".into(), 100, 42)]);
        // 版本只做载荷：出队原样带出，供消费侧校验
        assert_eq!(q.pop_one_due(100), Some(("a".into(), 100, 42)));
        assert_eq!(q.pop_one_due(100), None);
    }

    #[test]
    fn refresh_new_version_overwrites_same_id() {
        let q = queue();
        q.refresh([("a".into(), 100, 10)]);
        // 同 id 新版本带新时间：接纳并按新 rank 追加堆项
        assert_eq!(q.refresh([("a".into(), 200, 11)]), 1);
        assert_eq!(q.pop_one_due(100), None);
        assert_eq!(q.pop_one_due(200), Some(("a".into(), 200, 11)));
    }

    #[test]
    fn refresh_rejects_older_and_equal_versions() {
        let q = queue();
        q.refresh([("a".into(), 100, 10)]);
        // 乱序到达的旧事件：时间和版本一起被拒绝，队列不倒退
        assert_eq!(q.refresh([("a".into(), 50, 9)]), 0);
        assert_eq!(q.next_run_at(), Some(100));
        // 同一快照重投：幂等拒绝
        assert_eq!(q.refresh([("a".into(), 70, 10)]), 0);
        assert_eq!(q.next_run_at(), Some(100));
        assert_eq!(q.pop_one_due(100), Some(("a".into(), 100, 10)));
    }

    #[test]
    fn stale_heap_entry_is_skipped() {
        let q = queue();
        q.refresh([("a".into(), 100, 1)]);
        q.refresh([("a".into(), 500, 2)]);
        // 旧堆项 100 先被 peek 到，但与权威 map 不一致 → 丢弃，不产生 due
        assert_eq!(q.pop_one_due(200), None);
        assert_eq!(q.next_run_at(), Some(500));
        assert_eq!(q.pop_one_due(500), Some(("a".into(), 500, 2)));
    }

    #[test]
    fn pop_one_due_is_chronological_and_removes_entries() {
        let q = queue();
        q.refresh([
            ("late".into(), 300, 1),
            ("early".into(), 100, 1),
            ("mid".into(), 200, 1),
        ]);
        assert_eq!(q.pop_one_due(200), Some(("early".into(), 100, 1)));
        assert_eq!(q.pop_one_due(200), Some(("mid".into(), 200, 1)));
        assert_eq!(q.pop_one_due(200), None);
        assert_eq!(q.ids(), vec!["late".to_string()]);
        assert_eq!(q.next_run_at(), Some(300));
    }

    #[test]
    fn head_not_due_means_nothing_is_popped() {
        let q = queue();
        q.refresh([("a".into(), 300, 1), ("b".into(), 500, 1)]);
        assert_eq!(q.pop_one_due(100), None);
        assert_eq!(q.ids(), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(q.next_run_at(), Some(300));
    }

    #[test]
    fn remove_drops_entry_and_heap_snapshot() {
        let q = queue();
        q.refresh([("a".into(), 100, 1), ("b".into(), 200, 1)]);
        assert!(q.remove("a"));
        assert!(!q.remove("a"));
        assert_eq!(q.pop_one_due(100), None);
        assert_eq!(q.ids(), vec!["b".to_string()]);
    }
}
