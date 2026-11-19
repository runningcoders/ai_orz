//! 工具就绪判定的进程级 TTL 缓存（工具基础设施）
//!
//! 判定本身（CLI 可寻址 / 凭据命中）在同目录 [`super::tool_readiness`] 与
//! runtime 的取数逻辑里，本模块只负责**把结论缓存住**，并给写入方提供失效入口。
//! 二者同属「工具运行时判定」，故都在 `pkg::tool_registry` 下：纯判定函数
//! 在 `tool_readiness`，结论缓存与失效在这里。
//!
//! ## 为什么不是某个域
//!
//! 两份使用者的方向相反：runtime 是**读方**（列表请求复用上次判定），
//! finance 域是**失效方**（内置工具命令自愈写库后必须立刻让旧判定作废，否则
//! 最长 30s 内列表仍显示旧状态——「修了但看起来没修」）。失效关系是
//! 「数据被改了 → 缓存作废」，天然跨域；把它留在 runtime 会形成
//! `finance domain → runtime domain` 的域间调用。缓存是**基础设施**，
//! 与工具判定同包，两侧各自引用，域之间没有调用。
//!
//! ## 键约定
//!
//! CLI 型按 `tool_id`、key 型按 `tool_id|user_id`；失效按 `tool_id` 前缀整族清除。

use common::api::RuntimeReady;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 就绪判定缓存 TTL：窗口内重复列表请求复用上次判定
pub(crate) const TTL: Duration = Duration::from_secs(30);

/// 就绪判定缓存（key 见模块文档）
static CACHE: OnceLock<Mutex<HashMap<String, (RuntimeReady, Instant)>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, (RuntimeReady, Instant)>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 取未过期的判定结论；未命中 / 已过期返回 `None`
pub(crate) fn get(key: &str) -> Option<RuntimeReady> {
    let guard = cache().lock().unwrap();
    let (status, at) = guard.get(key)?;
    if at.elapsed() < TTL {
        Some(status.clone())
    } else {
        None
    }
}

/// 写入判定结论（同时刷新时间戳）
pub(crate) fn put(key: String, status: RuntimeReady) {
    cache()
        .lock()
        .unwrap()
        .insert(key, (status, Instant::now()));
}

/// 清除指定工具的就绪缓存（含 `tool_id|user_id` 全部变体）
///
/// 【生产可见 2026-10-21】内置工具就绪自愈写入新 `command` 后必须立刻失效。
pub fn invalidate(tool_id: &str) {
    let prefix = format!("{}|", tool_id);
    cache()
        .lock()
        .unwrap()
        .retain(|k, _| !k.starts_with(&prefix) && k != tool_id);
}

/// 测试辅助：把指定工具的缓存时间戳拨回 TTL 前（模拟过期触发重新判定）
#[cfg(test)]
pub(crate) fn expire(tool_id: &str) {
    let prefix = format!("{}|", tool_id);
    cache().lock().unwrap().iter_mut().for_each(|(k, (_, at))| {
        if k == tool_id || k.starts_with(&prefix) {
            *at = Instant::now() - TTL - Duration::from_secs(1);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_then_get_returns_status() {
        put("ut-tool".to_string(), RuntimeReady::Ready);
        assert_eq!(get("ut-tool"), Some(RuntimeReady::Ready));
    }

    #[test]
    fn expired_entry_is_not_returned() {
        put("ut-expired".to_string(), RuntimeReady::Ready);
        expire("ut-expired");
        assert_eq!(get("ut-expired"), None, "过期必须重新判定");
    }

    #[test]
    fn invalidate_clears_tool_and_its_user_variants() {
        put("ut-inval".to_string(), RuntimeReady::Ready);
        put("ut-inval|u1".to_string(), RuntimeReady::Ready);
        put("ut-inval-other".to_string(), RuntimeReady::Ready);
        invalidate("ut-inval");
        assert_eq!(get("ut-inval"), None);
        assert_eq!(get("ut-inval|u1"), None, "user 变体一并清除");
        assert!(
            get("ut-inval-other").is_some(),
            "前缀匹配必须以 `|` 为界，不能误伤同前缀的其他工具"
        );
        invalidate("ut-inval-other");
    }
}
