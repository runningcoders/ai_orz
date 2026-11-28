//! 通用优先级队列基础实现。
//!
//! 只负责与场景无关的三件事：按 `rank` 排序的堆、`key → entry` 权威 map、
//! 同 key 更新时旧堆项惰性失效。场景逻辑（AOP 的门闩/退避/监控、cron 的
//! 到期判定/DB 兜底）由各自的封装层提供。
//!
//! `PriorityQueue` 是无锁内核（调用方持锁或独占）；`SharedPriorityQueue`
//! 是带 poison 自动恢复锁的线程安全包装。

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::{Mutex, MutexGuard};

/// 队列条目：以 `key` 唯一标识，`rank` 决定出队顺序。
///
/// BinaryHeap 是最大堆 —— `rank` 越大越先出。需要「时间早的先出」时，
/// 场景用 `Reverse(timestamp)` 作为 rank（见 cron 封装）。
pub trait RankedEntry: Clone {
    type Rank: Ord + Clone + std::fmt::Debug;
    fn key(&self) -> &str;
    fn rank(&self) -> Self::Rank;
}

/// 可选能力：条目带单调版本，支持「旧版本不得覆盖新版本」的入队门禁。
///
/// 与 [`RankedEntry`] 分离：`rank` 决定出队顺序，`version` 决定条目新鲜度，
/// 两者正交。不需要版本裁决的场景（如 AOP 事件队列 —— `event_id` 天然唯一，
/// 重复入队已被去重）无需实现本 trait，继续用普通 `upsert`。
pub trait VersionedEntry: RankedEntry {
    type Version: Ord + Clone;
    fn version(&self) -> Self::Version;
}

#[derive(Debug, Clone)]
struct HeapItem<R> {
    rank: R,
    /// 单调递增序号：同 key 更新时新堆项拿新 seq，旧项成为惰性垃圾；
    /// 同 rank 的不同 key 按入队先后决定顺序（先入先出）。
    seq: u64,
    key: String,
}

impl<R: Ord> PartialEq for HeapItem<R> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl<R: Ord> Eq for HeapItem<R> {}
impl<R: Ord> PartialOrd for HeapItem<R> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<R: Ord> Ord for HeapItem<R> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank
            .cmp(&other.rank)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

#[derive(Debug)]
struct Stored<E> {
    entry: E,
    /// 与堆项配对的序号；出队时不一致即为过期快照。
    seq: u64,
}

/// 无锁优先级队列内核。
#[derive(Debug)]
pub struct PriorityQueue<E: RankedEntry> {
    entries: HashMap<String, Stored<E>>,
    heap: BinaryHeap<HeapItem<E::Rank>>,
    next_seq: u64,
}

impl<E: RankedEntry> Default for PriorityQueue<E> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            heap: BinaryHeap::new(),
            next_seq: 0,
        }
    }
}

impl<E: RankedEntry> PriorityQueue<E> {
    pub fn new() -> Self {
        Self::default()
    }

    /// 写入条目。同 key 的 `rank` 未变时只替换载荷、不追加堆项；
    /// 变了则追加新堆项，旧堆项出队时按 seq 不一致丢弃。
    /// 返回 `rank` 是否发生变化（新增也算变化）。
    pub fn upsert(&mut self, entry: E) -> bool {
        let key = entry.key().to_string();
        let rank = entry.rank();
        let (seq, rank_changed) = match self.entries.get(&key) {
            Some(stored) if stored.entry.rank() == rank => (stored.seq, false),
            _ => {
                let seq = self.next_seq;
                self.next_seq = self.next_seq.wrapping_add(1);
                self.heap.push(HeapItem {
                    rank,
                    seq,
                    key: key.clone(),
                });
                (seq, true)
            }
        };
        self.entries.insert(key, Stored { entry, seq });
        rank_changed
    }

    /// 原地修改命中条目的载荷。
    ///
    /// 若闭包改动了 `rank`，本方法会把条目按新 rank 重新入堆（旧堆项惰性失效）；
    /// 未改动则完全不动堆与 seq —— 适合「排序无关字段」的原地更新（如清退避）。
    /// 返回是否命中该 key。
    pub fn update_entry<F: FnOnce(&mut E)>(&mut self, key: &str, f: F) -> bool {
        let Some(stored) = self.entries.get_mut(key) else {
            return false;
        };
        let old_rank = stored.entry.rank();
        f(&mut stored.entry);
        let rank_changed = stored.entry.rank() != old_rank;
        // 显式释放借用，下方可能改 entries
        let _ = stored;
        if rank_changed {
            let entry = self
                .entries
                .remove(key)
                .expect("entry existed just above")
                .entry;
            self.upsert(entry);
        }
        true
    }

    /// 带版本门禁的写入：同 key 已有条目且新版本不更新（`<=`）时拒绝，返回 `false`。
    ///
    /// 门禁只决定「是否接纳」，不改变排序结构 —— 接受后仍走 [`Self::upsert`]：
    /// 权威 map 覆盖、追加新堆项、旧堆项靠 seq 惰性失效（即「删掉再加回去」，
    /// 堆顺序始终正确）。版本相等也拒绝：同一快照的重复投递是幂等的。
    ///
    /// 返回的 `bool` 仍是「rank 是否变化」：同 key 新版本但 rank 未变时，
    /// 载荷会更新（消费侧可拿到新版本），但返回 `false`。
    pub fn upsert_if_newer(&mut self, entry: E) -> bool
    where
        E: VersionedEntry,
    {
        if self
            .entries
            .get(entry.key())
            .is_some_and(|stored| entry.version() <= stored.entry.version())
        {
            return false;
        }
        self.upsert(entry)
    }

    /// 移除条目；堆里的对应项自然成为惰性垃圾。
    pub fn remove(&mut self, key: &str) -> Option<E> {
        self.entries.remove(key).map(|stored| stored.entry)
    }

    /// 该 key 是否有有效条目。
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// 全部有效 key（排序后返回，便于构造确定性的 SQL 排除条件）。
    pub fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.entries.keys().cloned().collect();
        keys.sort();
        keys
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.heap.clear();
    }

    /// 查看堆顶有效条目（不移除），顺带清掉惰性垃圾。
    pub fn peek(&mut self) -> Option<(String, E)> {
        loop {
            let item = self.heap.peek()?.clone();
            match self.entries.get(&item.key) {
                Some(stored) if stored.seq == item.seq => {
                    return Some((item.key, stored.entry.clone()));
                }
                _ => {
                    self.heap.pop();
                }
            }
        }
    }

    /// 取出一个满足 `can_take` 的最高优先级条目并从队列移除。
    /// 不满足的条目临时摘出、遍历结束后原样放回 —— 不会阻塞堆里后继的到期项。
    /// 取出的条目不再 `contains`（出队即移除，AOP 的「在飞」状态由场景自己记）。
    pub fn pop_where(&mut self, can_take: &dyn Fn(&E) -> bool) -> Option<(String, E)> {
        let mut deferred: Vec<HeapItem<E::Rank>> = Vec::new();
        let picked = loop {
            let Some(item) = self.heap.pop() else {
                break None;
            };
            let Some(stored) = self.entries.get(&item.key) else {
                // 载荷已删除：惰性垃圾，直接丢弃
                continue;
            };
            if stored.seq != item.seq {
                // 同 key 已更新到新 rank：旧堆项作废
                continue;
            }
            if !can_take(&stored.entry) {
                deferred.push(item);
                continue;
            }
            let key = item.key;
            let stored = self
                .entries
                .remove(&key)
                .expect("just checked presence above");
            break Some((key, stored.entry));
        };
        self.heap.extend(deferred);
        picked
    }

    /// 取出全部满足条件的条目，按优先级从高到低返回。
    pub fn pop_all_where(&mut self, can_take: &dyn Fn(&E) -> bool) -> Vec<(String, E)> {
        // 一次遍历完成：不满足的条目只摘出一次，结束后统一放回。
        // 逐条调用 pop_where 会对同一批延后项反复弹堆/回堆。
        let mut picked = Vec::new();
        let mut deferred: Vec<HeapItem<E::Rank>> = Vec::new();
        loop {
            let Some(item) = self.heap.pop() else {
                break;
            };
            let Some(stored) = self.entries.get(&item.key) else {
                continue;
            };
            if stored.seq != item.seq {
                continue;
            }
            if can_take(&stored.entry) {
                let key = item.key;
                let stored = self
                    .entries
                    .remove(&key)
                    .expect("just checked presence above");
                picked.push((key, stored.entry));
            } else {
                deferred.push(item);
            }
        }
        self.heap.extend(deferred);
        picked
    }

    /// 取出全部 `rank >= min_rank` 的条目，按优先级从高到低返回。
    ///
    /// BinaryHeap 按 rank 降序弹出：一旦堆顶**有效**条目的 rank 低于阈值，
    /// 后继只会更低，立即停止遍历 —— 未到期项只弹出这一个并放回，不会被
    /// 全堆扫描一遍。适用于「出队条件等价于 rank 越过阈值」的场景（cron 的
    /// 到期判定）；AOP 的 `not_before` 不在 rank 里，仍需用 `pop_where` 逐条 defer。
    ///
    /// 惰性垃圾（key 已删 / seq 过期）不参与阈值判断，直接丢弃后继续找有效堆顶。
    pub fn pop_all_at_least(&mut self, min_rank: &E::Rank) -> Vec<(String, E)> {
        let mut picked = Vec::new();
        let mut deferred: Vec<HeapItem<E::Rank>> = Vec::new();
        loop {
            let Some(item) = self.heap.pop() else {
                break;
            };
            let Some(stored) = self.entries.get(&item.key) else {
                continue;
            };
            if stored.seq != item.seq {
                continue;
            }
            if item.rank.cmp(min_rank) == Ordering::Less {
                // 堆顶已越过阈值，后继更低：放回这一项后停止。
                deferred.push(item);
                break;
            }
            let key = item.key;
            let stored = self
                .entries
                .remove(&key)
                .expect("just checked presence above");
            picked.push((key, stored.entry));
        }
        self.heap.extend(deferred);
        picked
    }

    /// 无条件取出一个最高优先级条目。
    pub fn pop(&mut self) -> Option<(String, E)> {
        self.pop_where(&|_| true)
    }
}

/// 带锁包装：poison 后自动取回内部数据，避免持锁 panic 把队列永久焊死。
#[derive(Debug)]
pub struct SharedPriorityQueue<E: RankedEntry> {
    inner: Mutex<PriorityQueue<E>>,
}

impl<E: RankedEntry> Default for SharedPriorityQueue<E> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(PriorityQueue::new()),
        }
    }
}

impl<E: RankedEntry> SharedPriorityQueue<E> {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, PriorityQueue<E>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn upsert(&self, entry: E) -> bool {
        self.lock().upsert(entry)
    }

    pub fn upsert_if_newer(&self, entry: E) -> bool
    where
        E: VersionedEntry,
    {
        self.lock().upsert_if_newer(entry)
    }

    pub fn remove(&self, key: &str) -> Option<E> {
        self.lock().remove(key)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.lock().contains(key)
    }

    pub fn keys(&self) -> Vec<String> {
        self.lock().keys()
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    pub fn clear(&self) {
        self.lock().clear();
    }

    pub fn peek(&self) -> Option<(String, E)> {
        self.lock().peek()
    }

    pub fn pop_where(&self, can_take: &dyn Fn(&E) -> bool) -> Option<(String, E)> {
        self.lock().pop_where(can_take)
    }

    pub fn pop_all_where(&self, can_take: &dyn Fn(&E) -> bool) -> Vec<(String, E)> {
        self.lock().pop_all_where(can_take)
    }

    pub fn pop_all_at_least(&self, min_rank: &E::Rank) -> Vec<(String, E)> {
        self.lock().pop_all_at_least(min_rank)
    }

    pub fn pop(&self) -> Option<(String, E)> {
        self.lock().pop()
    }
}

#[cfg(test)]
#[path = "priority_queue_tests.rs"]
mod tests;
