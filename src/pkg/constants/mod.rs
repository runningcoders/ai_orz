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
    /// 日志追踪 ID（从 header 获取或自动生成）
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

    /// 生成新的 log_id（时间戳 + 随机后缀）
    pub fn generate_log_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let random: u32 = rand_simple();
        format!("{:x}-{:x}", timestamp, random)
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
        assert!(!log_id.is_empty());
        println!("Generated log_id: {}", log_id);
    }

    #[test]
    fn test_log_id_format() {
        let log_id = RequestContext::generate_log_id();
        // 格式：时间戳-随机数
        assert!(log_id.contains("-"));
    }
}
