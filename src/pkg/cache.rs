//! 进程内 TTL 缓存（通用组件）
//!
//! 统一三件事，避免各模块各写一套 `Mutex<Option<(Instant, T)>`：
//! 1. **过期语义**：写入即给 TTL，读取时懒过期，`prune` 主动清扫；
//! 2. **可观测性**：命中 / 未命中 / 过期 / 写入 / 失效 / 清空计数；
//! 3. **管理入口**：按名称注册到进程级注册表，系统管理页可查询与清理。
//!
//! 边界：仅进程内、不持久化、不跨实例；值必须可克隆，键需可转字符串
//! （管理页展示用）。含敏感值的缓存不应注册到管理面，或应使用不可读的键。

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

/// 缓存计数（进程生命周期内累计，重启归零）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheCounters {
    /// 命中次数
    pub hits: u64,
    /// 未命中次数（含读取时发现已过期）
    pub misses: u64,
    /// 过期移除次数（读取懒过期 + `prune` 主动清扫）
    pub expirations: u64,
    /// 写入 / 覆盖次数
    pub inserts: u64,
    /// 主动失效次数
    pub invalidations: u64,
    /// 清空次数（非条目数）
    pub clears: u64,
}

/// 单个缓存的只读快照
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheStats {
    /// 注册名
    pub name: &'static str,
    /// 过期时长
    pub ttl: Duration,
    /// 当前条目数（含已过期但尚未清扫的）
    pub entries: usize,
    /// 其中已过期、等待下次读取或 `prune` 清理的条目数
    pub expired_entries: usize,
    /// 累计计数
    pub counters: CacheCounters,
}

struct Entry<V> {
    value: V,
    stored_at: Instant,
}

/// 通用 TTL 缓存
///
/// 所有方法都是同步的：调用方在异步流程里负责「未命中 → 取数 → 写回」，
/// 缓存本身不持有 future，避免把不同运行时的锁语义带进来。
pub struct TtlCache<K, V> {
    name: &'static str,
    ttl: Duration,
    entries: Mutex<HashMap<K, Entry<V>>>,
    hits: AtomicU64,
    misses: AtomicU64,
    expirations: AtomicU64,
    inserts: AtomicU64,
    invalidations: AtomicU64,
    clears: AtomicU64,
}

impl<K: Eq + Hash + Clone, V: Clone> TtlCache<K, V> {
    /// 创建未注册的缓存（测试或局部场景用）
    pub fn new(name: &'static str, ttl: Duration) -> Self {
        Self {
            name,
            ttl,
            entries: Mutex::new(HashMap::new()),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            expirations: AtomicU64::new(0),
            inserts: AtomicU64::new(0),
            invalidations: AtomicU64::new(0),
            clears: AtomicU64::new(0),
        }
    }

    /// 命中返回克隆值；过期条目顺手移除并计入过期 + 未命中
    pub fn get(&self, key: &K) -> Option<V> {
        let mut entries = self.lock_entries();
        match entries.get(key) {
            Some(entry) if !self.is_expired(entry) => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some(entry.value.clone())
            }
            Some(_) => {
                entries.remove(key);
                self.expirations.fetch_add(1, Ordering::Relaxed);
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
            None => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// 写入 / 覆盖（同时重置该条目的过期计时）
    pub fn insert(&self, key: K, value: V) {
        self.lock_entries().insert(
            key,
            Entry {
                value,
                stored_at: Instant::now(),
            },
        );
        self.inserts.fetch_add(1, Ordering::Relaxed);
    }

    /// 主动失效单个条目；返回是否存在（含已过期未清扫）
    pub fn invalidate(&self, key: &K) -> bool {
        let removed = self.lock_entries().remove(key).is_some();
        if removed {
            self.invalidations.fetch_add(1, Ordering::Relaxed);
        }
        removed
    }

    /// 清空全部条目；返回移除数量
    pub fn clear(&self) -> usize {
        let removed = {
            let mut entries = self.lock_entries();
            let len = entries.len();
            entries.clear();
            len
        };
        if removed > 0 {
            self.clears.fetch_add(1, Ordering::Relaxed);
        }
        removed
    }

    /// 清扫已过期条目；返回移除数量
    pub fn prune(&self) -> usize {
        let mut entries = self.lock_entries();
        let before = entries.len();
        let ttl = self.ttl;
        entries.retain(|_, entry| entry.stored_at.elapsed() < ttl);
        let removed = before - entries.len();
        self.expirations
            .fetch_add(removed as u64, Ordering::Relaxed);
        removed
    }

    /// 当前快照：条目 / 过期条目 / 累计计数
    pub fn stats(&self) -> CacheStats {
        let entries = self.lock_entries();
        let expired_entries = entries
            .values()
            .filter(|entry| self.is_expired(entry))
            .count();
        CacheStats {
            name: self.name,
            ttl: self.ttl,
            entries: entries.len(),
            expired_entries,
            counters: CacheCounters {
                hits: self.hits.load(Ordering::Relaxed),
                misses: self.misses.load(Ordering::Relaxed),
                expirations: self.expirations.load(Ordering::Relaxed),
                inserts: self.inserts.load(Ordering::Relaxed),
                invalidations: self.invalidations.load(Ordering::Relaxed),
                clears: self.clears.load(Ordering::Relaxed),
            },
        }
    }

    /// 键列表（排序 + 截断，仅供管理页展示；不含值）
    pub fn keys(&self) -> Vec<String>
    where
        K: ToString,
    {
        let mut keys: Vec<String> = self
            .lock_entries()
            .keys()
            .map(|key| key.to_string())
            .collect();
        keys.sort();
        keys.truncate(MAX_LISTED_KEYS);
        keys
    }

    fn lock_entries(&self) -> MutexGuard<'_, HashMap<K, Entry<V>>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn is_expired(&self, entry: &Entry<V>) -> bool {
        entry.stored_at.elapsed() >= self.ttl
    }
}

/// 管理页一次最多展示的键数量，避免大缓存把响应撑大
const MAX_LISTED_KEYS: usize = 200;

/// 类型擦除后的管理句柄
///
/// 只暴露「名称 / 统计 / 清理」这类安全操作，不暴露值读取入口。
pub trait CacheControl: Send + Sync {
    /// 注册名
    fn name(&self) -> &'static str;
    /// 统计快照
    fn stats(&self) -> CacheStats;
    /// 清空条目，返回移除数量
    fn clear(&self) -> usize;
    /// 清扫过期条目，返回移除数量
    fn prune(&self) -> usize;
    /// 键列表（排序 + 截断）
    fn keys(&self) -> Vec<String>;
}

impl<K, V> CacheControl for TtlCache<K, V>
where
    K: Eq + Hash + Clone + ToString + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn name(&self) -> &'static str {
        self.name
    }

    fn stats(&self) -> CacheStats {
        TtlCache::stats(self)
    }

    fn clear(&self) -> usize {
        TtlCache::clear(self)
    }

    fn prune(&self) -> usize {
        TtlCache::prune(self)
    }

    fn keys(&self) -> Vec<String> {
        TtlCache::keys(self)
    }
}

/// 进程级注册表
fn registry() -> &'static Mutex<Vec<&'static dyn CacheControl>> {
    static REGISTRY: OnceLock<Mutex<Vec<&'static dyn CacheControl>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

fn lock_registry() -> MutexGuard<'static, Vec<&'static dyn CacheControl>> {
    registry().lock().unwrap_or_else(|e| e.into_inner())
}

/// 注册并泄漏一个进程级缓存，返回 `'static` 句柄
///
/// 缓存随进程生命周期存在（与 `OnceLock` 语义一致）；同名重复注册会并存，
/// 管理面按名称查找时取先注册的一个。
pub fn register<K, V>(name: &'static str, ttl: Duration) -> &'static TtlCache<K, V>
where
    K: Eq + Hash + Clone + ToString + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    let cached: &'static TtlCache<K, V> = Box::leak(Box::new(TtlCache::new(name, ttl)));
    lock_registry().push(cached);
    cached
}

/// 所有已注册缓存的快照（按名称排序，同名按注册顺序）
pub fn all_stats() -> Vec<CacheStats> {
    let mut stats: Vec<CacheStats> = lock_registry().iter().map(|cache| cache.stats()).collect();
    stats.sort_by(|a, b| a.name.cmp(b.name));
    stats
}

/// 按名称查找已注册缓存
pub fn find(name: &str) -> Option<&'static dyn CacheControl> {
    lock_registry()
        .iter()
        .copied()
        .find(|cache| cache.name() == name)
}

/// 清空指定缓存；返回移除数量，未找到返回 `None`
pub fn clear(name: &str) -> Option<usize> {
    find(name).map(|cache| cache.clear())
}

/// 清扫指定缓存；返回移除数量，未找到返回 `None`
pub fn prune(name: &str) -> Option<usize> {
    find(name).map(|cache| cache.prune())
}

/// 指定缓存的键列表；未找到返回 `None`
pub fn keys(name: &str) -> Option<Vec<String>> {
    find(name).map(|cache| cache.keys())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(ttl: Duration) -> TtlCache<String, String> {
        TtlCache::new("test", ttl)
    }

    fn key(value: &str) -> String {
        value.to_string()
    }

    #[test]
    fn hit_miss_and_counters() {
        let cache = cache(Duration::from_secs(60));
        assert_eq!(cache.get(&key("a")), None);
        cache.insert(key("a"), "1".to_string());
        assert_eq!(cache.get(&key("a")), Some("1".to_string()));

        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.counters.hits, 1);
        assert_eq!(stats.counters.misses, 1);
        assert_eq!(stats.counters.inserts, 1);
    }

    /// TTL 为零 ⇒ 写入即过期；读取时顺手移除并计入过期
    #[test]
    fn entries_expire_when_past_ttl() {
        let cache = cache(Duration::ZERO);
        cache.insert(key("a"), "1".to_string());
        assert_eq!(cache.get(&key("a")), None);

        let stats = cache.stats();
        assert_eq!(stats.entries, 0);
        assert_eq!(stats.counters.expirations, 1);
    }

    #[test]
    fn prune_and_clear_remove_entries() {
        let expired = cache(Duration::ZERO);
        expired.insert(key("a"), "1".to_string());
        expired.insert(key("b"), "2".to_string());
        assert_eq!(expired.prune(), 2);
        assert_eq!(expired.stats().entries, 0);

        let live = cache(Duration::from_secs(60));
        live.insert(key("a"), "1".to_string());
        assert_eq!(live.clear(), 1);
        assert_eq!(live.stats().entries, 0);
        assert_eq!(live.stats().counters.clears, 1);
    }

    #[test]
    fn invalidate_reports_existence() {
        let cache = cache(Duration::from_secs(60));
        cache.insert(key("a"), "1".to_string());
        assert!(cache.invalidate(&key("a")));
        assert!(!cache.invalidate(&key("a")));
        assert_eq!(cache.stats().counters.invalidations, 1);
    }

    #[test]
    fn registry_exposes_registered_cache() {
        let registered: &'static TtlCache<String, String> =
            register("test.registry.visibility", Duration::from_secs(60));
        registered.insert(key("a"), "1".to_string());

        let found = find("test.registry.visibility").expect("已注册");
        assert_eq!(found.stats().entries, 1);
        assert_eq!(
            keys("test.registry.visibility"),
            Some(vec!["a".to_string()])
        );
        assert!(
            all_stats()
                .iter()
                .any(|s| s.name == "test.registry.visibility")
        );
        assert_eq!(clear("test.registry.visibility"), Some(1));
    }
}
