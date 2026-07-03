use crate::pkg::storage::{self, Storage, VectorStore};
/// 请求上下文（贯穿整个请求生命周期）
use axum::http;
use common::constants::http_header;
use sqlx::sqlite::SqlitePool;
use std::sync::Arc;

/// 请求上下文
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// 日志追踪 ID
    pub log_id: String,
    /// 当前用户 ID
    pub user_id: Option<String>,
    /// 当前用户名
    pub username: Option<String>,
    /// 当前组织 ID
    pub organization_id: Option<String>,

    /// 当前 Agent ID（可选，Agent 执行时有值）
    pub agent_id: Option<String>,
    /// 当前 Task ID（可选，Task 执行时有值）
    pub task_id: Option<String>,
    /// 当前 Project ID（可选，Project 上下文时有值）
    pub project_id: Option<String>,
    /// 当前 Model Provider ID（可选，Cortex 创建时有值）
    pub model_provider_id: Option<String>,
    /// 当前 Model 名称（可选，Cortex 创建时有值）
    pub model_name: Option<String>,

    /// 统一存储门面（SQLite + Vector）
    storage: Storage,
}

impl RequestContext {
    /// 从 header 中提取上下文
    pub fn from_headers(headers: &http::HeaderMap) -> Self {
        // 1. 优先从 header 获取 log_id
        let log_id = headers
            .get(http_header::LOG_ID)
            .and_then(|v: &http::HeaderValue| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| Self::generate_log_id());

        // 2. 从 header 获取用户信息
        let user_id = headers
            .get(http_header::USER_ID)
            .and_then(|v: &http::HeaderValue| v.to_str().ok())
            .map(|s| s.to_string());

        let username = headers
            .get(http_header::USERNAME)
            .and_then(|v: &http::HeaderValue| v.to_str().ok())
            .map(|s| s.to_string());

        // 3. 从 header 获取组织 ID（后续 JWT 解析结果会覆盖）
        let organization_id = headers
            .get(http_header::ORGANIZATION_ID)
            .and_then(|v: &http::HeaderValue| v.to_str().ok())
            .map(|s| s.to_string());

        Self {
            log_id,
            user_id,
            username,
            organization_id,
            agent_id: None,
            task_id: None,
            project_id: None,
            model_provider_id: None,
            model_name: None,
            storage: storage::get().clone(),
        }
    }

    /// 生成新的上下文（带自动生成的 log_id）
    pub fn new(user_id: Option<String>, username: Option<String>) -> Self {
        Self {
            log_id: Self::generate_log_id(),
            user_id,
            username,
            organization_id: None,
            agent_id: None,
            task_id: None,
            project_id: None,
            model_provider_id: None,
            model_name: None,
            storage: storage::get().clone(),
        }
    }

    /// 从指定 Storage 创建上下文（测试辅助，由 test_support 调用）
    pub fn from_storage(user_id: &str, storage: Storage) -> Self {
        Self {
            log_id: Self::generate_log_id(),
            user_id: Some(user_id.to_string()),
            username: None,
            organization_id: None,
            agent_id: None,
            task_id: None,
            project_id: None,
            model_provider_id: None,
            model_name: None,
            storage,
        }
    }

    /// 设置 log_id（用于中间件处理时覆盖自动生成的 log_id）
    pub fn set_log_id(&mut self, log_id: String) {
        self.log_id = log_id;
    }

    /// 设置组织 ID
    pub fn set_organization_id(&mut self, organization_id: impl Into<String>) {
        self.organization_id = Some(organization_id.into());
    }

    /// 设置 Agent ID
    pub fn set_agent_id(&mut self, agent_id: impl Into<String>) {
        self.agent_id = Some(agent_id.into());
    }

    /// 设置 Task ID
    pub fn set_task_id(&mut self, task_id: impl Into<String>) {
        self.task_id = Some(task_id.into());
    }

    /// 设置 Project ID
    pub fn set_project_id(&mut self, project_id: impl Into<String>) {
        self.project_id = Some(project_id.into());
    }

    /// 设置 Model Provider ID
    pub fn set_model_provider_id(&mut self, model_provider_id: impl Into<String>) {
        self.model_provider_id = Some(model_provider_id.into());
    }

    /// 设置 Model 名称
    pub fn set_model_name(&mut self, model_name: impl Into<String>) {
        self.model_name = Some(model_name.into());
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

    /// 获取当前 Agent ID
    pub fn agent_id(&self) -> Option<&String> {
        self.agent_id.as_ref()
    }

    /// 获取当前 Task ID
    pub fn task_id(&self) -> Option<&String> {
        self.task_id.as_ref()
    }

    /// 获取当前 Project ID
    pub fn project_id(&self) -> Option<&String> {
        self.project_id.as_ref()
    }

    /// 获取当前 Organization ID
    pub fn organization_id(&self) -> Option<&String> {
        self.organization_id.as_ref()
    }

    /// 获取当前 User ID
    pub fn user_id(&self) -> Option<&String> {
        self.user_id.as_ref()
    }

    /// 获取当前 Model Provider ID
    pub fn model_provider_id(&self) -> Option<&String> {
        self.model_provider_id.as_ref()
    }

    /// 获取当前 Model 名称
    pub fn model_name(&self) -> Option<&String> {
        self.model_name.as_ref()
    }

    /// 获取 DB pool
    pub fn db_pool(&self) -> &SqlitePool {
        // 从统一 Storage 获取，保持向后兼容
        self.storage.sqlite()
    }

    /// 获取向量存储（统一 Trait 接口）
    pub fn vector_store(&self) -> Arc<dyn VectorStore> {
        self.storage.vector().clone()
    }

    /// 获取统一存储门面
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// 获取统计模块
    pub fn stats(&self) -> &crate::pkg::stats::Stats {
        self.storage.stats()
    }

    /// 安全获取统计模块（返回 Option，避免未初始化时 panic）
    pub fn stats_opt(&self) -> Option<&crate::pkg::stats::Stats> {
        self.storage.stats_opt()
    }
}

/// 格式化时间戳为 YYYYMMDDHHmmss
pub fn format_timestamp(secs: u64) -> String {
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
