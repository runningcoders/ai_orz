//! 飞书 tenant_access_token 缓存
//!
//! 缓存飞书应用级凭证（tenant_access_token），提前 5 分钟刷新避免临界过期。

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// token 提前刷新的缓冲时间
const REFRESH_BUFFER: Duration = Duration::from_secs(5 * 60);

/// tenant_access_token 缓存
///
/// 使用 `Arc<RwLock<TokenCache>>` 双重检查锁模式防止并发刷新。
#[derive(Debug, Clone, Default)]
pub struct TokenCache {
    token: Option<String>,
    /// 绝对过期时间点（fetch 时返回的 expire 时间）
    expire_at: Option<Instant>,
}

impl TokenCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// 获取未过期的 token（已考虑 5 分钟提前刷新缓冲）
    ///
    /// 返回 `Some(token)` 表示缓存命中，`None` 表示需要重新获取。
    pub fn get_valid_token(&self) -> Option<String> {
        match (&self.token, self.expire_at) {
            (Some(t), Some(expire)) => {
                if Instant::now() + REFRESH_BUFFER < expire {
                    Some(t.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// 更新缓存的 token 与过期时间
    ///
    /// `expire_in_secs` 是飞书返回的过期秒数（通常 7200s）。
    pub fn update(&mut self, token: String, expire_in_secs: u64) {
        self.token = Some(token);
        self.expire_at = Some(Instant::now() + Duration::from_secs(expire_in_secs));
    }

    /// 清空缓存（用于强制刷新或失败回退）
    pub fn invalidate(&mut self) {
        self.token = None;
        self.expire_at = None;
    }
}

/// 全局共享的 token 缓存句柄
pub type SharedTokenCache = Arc<RwLock<TokenCache>>;

/// 创建共享 token 缓存
pub fn shared() -> SharedTokenCache {
    Arc::new(RwLock::new(TokenCache::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_cache_returns_none() {
        let cache = TokenCache::new();
        assert!(cache.get_valid_token().is_none());
    }

    #[test]
    fn test_valid_token_returns_some() {
        let mut cache = TokenCache::new();
        cache.update("token_xxx".to_string(), 7200);
        assert_eq!(cache.get_valid_token().as_deref(), Some("token_xxx"));
    }

    #[test]
    fn test_near_expiry_returns_none() {
        let mut cache = TokenCache::new();
        // 缓存时间小于 5 分钟刷新缓冲，应视为过期
        cache.update("token_xxx".to_string(), 60);
        assert!(cache.get_valid_token().is_none());
    }

    #[test]
    fn test_invalidate_clears_cache() {
        let mut cache = TokenCache::new();
        cache.update("token_xxx".to_string(), 7200);
        cache.invalidate();
        assert!(cache.get_valid_token().is_none());
    }
}
