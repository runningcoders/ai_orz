//! 进程内 nonce 去重（S2 联邦签名防重放，SSOT: docs/plan/联邦鉴权升级方案.md §四）
//!
//! 单实例是常态（方案「明确不做」：多实例需共享存储时再换 Redis）。
//! TTL 略大于时间戳窗口（±300s → 600s）；容量上限防内存膨胀，超限清理过期项。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// nonce 保留时长（大于时间窗 ±300s，留余量）
const NONCE_TTL: Duration = Duration::from_secs(600);
/// 容量上限：触发即清理过期项（常态并发远低于此）
const MAX_ENTRIES: usize = 65_536;

static NONCES: std::sync::OnceLock<Mutex<HashMap<String, Instant>>> = std::sync::OnceLock::new();

fn store() -> &'static Mutex<HashMap<String, Instant>> {
    NONCES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 生成随机 16 字节 hex nonce（§四：每次联邦签名请求一枚）
pub fn generate() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 原子「查重 + 登记」：返回 `false` = nonce 已出现过（重放，调用方 401）
pub fn check_and_insert(nonce: &str) -> bool {
    let mut map = store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = Instant::now();

    // 容量触发式清理（避免定时器/后台任务）
    if map.len() >= MAX_ENTRIES {
        map.retain(|_, seen| now.duration_since(*seen) < NONCE_TTL);
        // 极端情况下仍未腾出空间：整体清空（可用性优先，窗口内重放防护已尽力）
        if map.len() >= MAX_ENTRIES {
            map.clear();
        }
    }

    if map.contains_key(nonce) {
        return false;
    }
    map.insert(nonce.to_string(), now);
    true
}

/// 清理过期 nonce（返回剩余条数；仅测试与运维观测用）
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn retain_fresh() -> usize {
    let mut map = store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = Instant::now();
    map.retain(|_, seen| now.duration_since(*seen) < NONCE_TTL);
    map.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_insert_true_replay_false() {
        let nonce = format!("test-{}", uuid::Uuid::now_v7());
        assert!(check_and_insert(&nonce), "首次插入应成功");
        assert!(!check_and_insert(&nonce), "同 nonce 二次插入 = 重放");
    }

    #[test]
    fn distinct_nonces_independent() {
        let a = format!("a-{}", uuid::Uuid::now_v7());
        let b = format!("b-{}", uuid::Uuid::now_v7());
        assert!(check_and_insert(&a));
        assert!(check_and_insert(&b));
    }
}
