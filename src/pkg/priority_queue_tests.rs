use super::*;
use std::cmp::Reverse;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Item {
    key: String,
    rank: i64,
    note: String,
}

impl Item {
    fn new(key: &str, rank: i64, note: &str) -> Self {
        Self {
            key: key.to_string(),
            rank,
            note: note.to_string(),
        }
    }
}

impl RankedEntry for Item {
    type Rank = i64;
    fn key(&self) -> &str {
        &self.key
    }
    fn rank(&self) -> i64 {
        self.rank
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Scheduled {
    id: String,
    due_at: i64,
}

impl RankedEntry for Scheduled {
    type Rank = Reverse<i64>;
    fn key(&self) -> &str {
        &self.id
    }
    fn rank(&self) -> Reverse<i64> {
        Reverse(self.due_at)
    }
}

#[test]
fn upsert_dedups_and_updates_payload_without_new_heap_item() {
    let mut q = PriorityQueue::<Item>::new();
    assert!(q.upsert(Item::new("a", 1, "v1")));
    assert!(!q.upsert(Item::new("a", 1, "v2")));
    assert_eq!(q.len(), 1);
    // 载荷已更新，但堆项仍是同一个 seq
    assert_eq!(q.peek().unwrap().1.note, "v2");
    // rank 变化才追加新堆项
    assert!(q.upsert(Item::new("a", 5, "v3")));
    assert_eq!(q.peek().unwrap().1.rank, 5);
}

#[test]
fn stale_heap_item_is_skipped() {
    let mut q = PriorityQueue::<Scheduled>::new();
    q.upsert(Scheduled {
        id: "a".into(),
        due_at: 100,
    });
    q.upsert(Scheduled {
        id: "a".into(),
        due_at: 500,
    });
    // 旧堆项 100 先被 peek 到，但 seq 已失效 → 丢弃
    let due: Vec<_> = q
        .pop_all_where(&|e| e.due_at <= 200)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert!(due.is_empty());
    assert_eq!(q.peek().unwrap().1.due_at, 500);
}

#[test]
fn deferred_items_do_not_block_following_items() {
    let mut q = PriorityQueue::<Item>::new();
    q.upsert(Item::new("blocked", 10, ""));
    q.upsert(Item::new("ready", 5, ""));

    let picked = q.pop_where(&|e| e.key != "blocked");
    assert_eq!(picked.unwrap().0, "ready");
    // 延迟项原样留在队列里，不影响下一次 peek
    assert!(q.contains("blocked"));
    assert_eq!(q.peek().unwrap().0, "blocked");
}

#[test]
fn pop_all_where_follows_rank_order_and_removes_entries() {
    let mut q = PriorityQueue::<Scheduled>::new();
    q.upsert(Scheduled {
        id: "late".into(),
        due_at: 300,
    });
    q.upsert(Scheduled {
        id: "early".into(),
        due_at: 100,
    });
    q.upsert(Scheduled {
        id: "mid".into(),
        due_at: 200,
    });

    let due: Vec<_> = q
        .pop_all_where(&|e| e.due_at <= 200)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(due, vec!["early", "mid"]);
    assert_eq!(q.keys(), vec!["late".to_string()]);
    assert_eq!(q.peek().unwrap().1.due_at, 300);
}

#[test]
fn pop_all_at_least_stops_at_first_item_below_threshold() {
    let mut q = PriorityQueue::<Scheduled>::new();
    for (id, due_at) in [
        ("t1", 100),
        ("t2", 200),
        ("t3", 300),
        ("t4", 400),
        ("t5", 500),
    ] {
        q.upsert(Scheduled {
            id: id.into(),
            due_at,
        });
    }
    // rank = Reverse(due_at)，阈值 Reverse(300) 表示只取 due_at <= 300。
    let due: Vec<_> = q
        .pop_all_at_least(&Reverse(300))
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(due, vec!["t1", "t2", "t3"]);
    // 未到期项未被弹出，仍在队列里。
    assert_eq!(q.keys(), vec!["t4".to_string(), "t5".to_string()]);
}

#[test]
fn pop_all_at_least_skips_stale_heap_items_before_threshold_check() {
    let mut q = PriorityQueue::<Scheduled>::new();
    // t1 先入 100 再改到 500，堆里残留 rank=Reverse(100) 的旧快照。
    q.upsert(Scheduled {
        id: "t1".into(),
        due_at: 100,
    });
    q.upsert(Scheduled {
        id: "t1".into(),
        due_at: 500,
    });
    q.upsert(Scheduled {
        id: "t2".into(),
        due_at: 200,
    });
    // 旧快照 rank 最高但 seq 失效 → 丢弃，不能因它而提前停。
    let due: Vec<_> = q
        .pop_all_at_least(&Reverse(200))
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(due, vec!["t2"]);
    assert_eq!(q.keys(), vec!["t1".to_string()]);
    assert_eq!(q.peek().unwrap().1.due_at, 500);
}

#[test]
fn remove_clear_and_contains_use_authoritative_map() {
    let mut q = PriorityQueue::<Item>::new();
    q.upsert(Item::new("a", 1, ""));
    q.upsert(Item::new("b", 2, ""));
    assert_eq!(q.remove("a").unwrap().note, "");
    assert!(!q.contains("a"));
    // 堆里遗留的 a 项不会在 peek 时复活
    assert_eq!(q.peek().unwrap().0, "b");
    q.clear();
    assert!(q.is_empty());
    assert!(q.peek().is_none());
}

#[test]
fn shared_wrapper_supports_concurrent_upsert() {
    let q = Arc::new(SharedPriorityQueue::<Item>::new());
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let q = Arc::clone(&q);
            std::thread::spawn(move || {
                q.upsert(Item::new(&format!("k{i}"), i, ""));
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(q.len(), 8);
    assert_eq!(q.keys().len(), 8);
    assert_eq!(q.pop().unwrap().0, "k7");
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VersionedItem {
    id: String,
    due_at: i64,
    version: i64,
}

impl RankedEntry for VersionedItem {
    type Rank = i64;
    fn key(&self) -> &str {
        &self.id
    }
    fn rank(&self) -> i64 {
        self.due_at
    }
}

impl VersionedEntry for VersionedItem {
    type Version = i64;
    fn version(&self) -> i64 {
        self.version
    }
}

#[test]
fn upsert_if_newer_rejects_stale_and_equal_versions() {
    let mut q = PriorityQueue::<VersionedItem>::new();
    assert!(q.upsert_if_newer(VersionedItem {
        id: "a".into(),
        due_at: 100,
        version: 10,
    }));
    // 旧版本即使带更早的 rank 也必须整体拒绝：rank 和载荷都不动
    assert!(!q.upsert_if_newer(VersionedItem {
        id: "a".into(),
        due_at: 50,
        version: 9,
    }));
    let (_, top) = q.peek().unwrap();
    assert_eq!(top.due_at, 100);
    assert_eq!(top.version, 10);
    // 相等版本同样拒绝：同一快照的重复投递幂等
    assert!(!q.upsert_if_newer(VersionedItem {
        id: "a".into(),
        due_at: 70,
        version: 10,
    }));
    assert_eq!(q.peek().unwrap().1.due_at, 100);
}

#[test]
fn upsert_if_newer_accepts_newer_version_and_updates_payload() {
    let mut q = PriorityQueue::<VersionedItem>::new();
    q.upsert_if_newer(VersionedItem {
        id: "a".into(),
        due_at: 100,
        version: 10,
    });
    // 新版本走原 upsert：rank 变化 → true，旧堆项惰性失效
    assert!(q.upsert_if_newer(VersionedItem {
        id: "a".into(),
        due_at: 300,
        version: 11,
    }));
    assert_eq!(q.peek().unwrap().1.due_at, 300);
    assert_eq!(q.peek().unwrap().1.version, 11);
    // 新版本但 rank 未变 → 载荷更新、返回 false
    assert!(!q.upsert_if_newer(VersionedItem {
        id: "a".into(),
        due_at: 300,
        version: 12,
    }));
    assert_eq!(q.peek().unwrap().1.version, 12);
}

#[test]
fn shared_upsert_if_newer_is_atomic_per_key() {
    let q = Arc::new(SharedPriorityQueue::<VersionedItem>::new());
    let handles: Vec<_> = (0..8)
        .map(|v| {
            let q = Arc::clone(&q);
            std::thread::spawn(move || {
                q.upsert_if_newer(VersionedItem {
                    id: "a".into(),
                    due_at: v * 10,
                    version: v,
                });
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    // 比较与写入在同一把锁内：最终必定是最高版本胜出
    let (_, top) = q.peek().unwrap();
    assert_eq!(top.version, 7);
    assert_eq!(top.due_at, 70);
}
