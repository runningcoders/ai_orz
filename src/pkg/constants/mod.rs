// ==================== 请求上下文 ====================

use serde::{Deserialize, Serialize};

/// AgentPo 状态枚举（用于软删除）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentPoStatus {
    Deleted = 0,
    Normal = 1,
}

impl AgentPoStatus {
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Deleted,
            _ => Self::Normal,
        }
    }

    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

impl serde::Serialize for AgentPoStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_i32(self.to_i32())
    }
}

impl<'de> serde::Deserialize<'de> for AgentPoStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = i32::deserialize(deserializer)?;
        Ok(Self::from_i32(v))
    }
}

impl Default for AgentPoStatus {
    fn default() -> Self {
        Self::Normal
    }
}

/// HTTP Header Keys（统一管理所有 header key）
pub mod http_header {
    /// 请求追踪 ID（用于日志串联）
    pub const LOG_ID: &str = "X-Log-Id";

    /// 当前用户 ID
    pub const USER_ID: &str = "X-User-Id";

    /// 当前用户名
    pub const USERNAME: &str = "X-Username";
}

/// 请求上下文（贯穿整个请求生命周期）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    /// 日志追踪 ID
    pub log_id: String,
    /// 当前用户 ID
    pub user_id: Option<String>,
    /// 当前用户名
    pub username: Option<String>,
}

impl RequestContext {
    /// 从 header 中提取上下文
    pub fn from_headers(headers: &axum::http::HeaderMap) -> Self {
        // 1. 优先从 header 获取 log_id
        let log_id = headers
            .get(http_header::LOG_ID)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| Self::generate_log_id());

        // 2. 从 header 获取用户信息
        let user_id = headers
            .get(http_header::USER_ID)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let username = headers
            .get(http_header::USERNAME)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        Self {
            log_id,
            user_id,
            username,
        }
    }

    /// 生成新的上下文（带自动生成的 log_id）
    pub fn new(user_id: Option<String>, username: Option<String>) -> Self {
        Self {
            log_id: Self::generate_log_id(),
            user_id,
            username,
        }
    }

    /// 生成新的 log_id
    ///
    /// 格式：年月日时分秒毫秒3位随机数，直接拼接无分隔符
    /// 示例：20260331011345000123
    pub fn generate_log_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        let secs = now.as_secs();
        let millis = now.subsec_millis();
        let random = rand_simple() % 1000; // 3位随机数

        // 格式：YYYYMMDDHHmmssSSSXXX（年月日时分秒毫秒3位随机）
        // 2026 03 31 01 13 45 000 123 -> 20260331011345000123
        format!("{}{:03}{:03}", format_timestamp(secs), millis, random)
    }

    /// 获取当前用户 ID（未登录返回空字符串）
    pub fn uid(&self) -> String {
        self.user_id.clone().unwrap_or_default()
    }

    /// 获取用户名（未登录返回空字符串）
    pub fn uname(&self) -> String {
        self.username.clone().unwrap_or_default()
    }
}

/// 格式化时间戳为 YYYYMMDDHHmmss
fn format_timestamp(secs: u64) -> String {
    // 将 Unix 时间戳转换为格式化字符串
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    // 简化：直接用纳秒构造
    // 更准确的方式是使用 chrono，但为了减少依赖，我们手动计算
    // 从 1970-01-01 开始计算
    let total_days = days as i64;

    // 基准日期 1970-01-01
    let mut year = 1970;
    let mut month = 1;
    let mut day = 1;

    // 加上天数
    let mut d = total_days;
    while d >= 365 {
        let leap = if is_leap_year(year) { 366 } else { 365 };
        if d >= leap {
            d -= leap;
            year += 1;
        } else {
            break;
        }
    }

    let days_in_months = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    for i in 0..12 {
        if d < days_in_months[i] {
            month = i + 1;
            day = d + 1;
            break;
        }
        d -= days_in_months[i];
    }

    format!(
        "{}{:02}{:02}{:02}{:02}{:02}",
        year, month, day, hours, minutes, seconds
    )
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// 生成简单随机数
fn rand_simple() -> u32 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};
    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    SystemTime::now().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    let time2 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u32;
    time2.wrapping_add(hasher.finish() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_log_id() {
        let log_id = RequestContext::generate_log_id();
        println!("Generated log_id: {}", log_id);
        assert_eq!(log_id.len(), 20); // 14 + 3 + 3 = 20
        assert!(log_id.chars().all(|c| c.is_ascii_digit())); // 纯数字
    }

    #[test]
    fn test_format_timestamp() {
        let ts = format_timestamp(1709258400); // 2024-03-31 12:00:00 UTC
        println!("Formatted: {}", ts);
    }

    #[test]
    fn test_request_context() {
        let ctx = RequestContext::new(Some("user1".to_string()), Some("test".to_string()));
        assert!(!ctx.log_id.is_empty());
        assert_eq!(ctx.uid(), "user1");
    }
}
