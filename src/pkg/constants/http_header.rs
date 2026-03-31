//! HTTP Header Keys（统一管理所有 header key）

/// 请求追踪 ID（用于日志串联）
pub const LOG_ID: &str = "X-Log-Id";

/// 当前用户 ID
pub const USER_ID: &str = "X-User-Id";

/// 当前用户名
pub const USERNAME: &str = "X-Username";
