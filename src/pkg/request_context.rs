/// 请求上下文（贯穿整个请求生命周期）

use axum::http;
use common::config::AppConfig;
use common::constants::http_header;
use sqlx::sqlite::SqlitePool;
use std::sync::Arc;
use crate::pkg::storage;

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

    /// DB 相关信息
    db_pool: SqlitePool,
    /// 应用配置全局引用
    config: Arc<AppConfig>,
}

impl RequestContext {
    /// 从 header 中提取上下文
    pub fn from_headers(headers: &http::HeaderMap, config: Arc<AppConfig>) -> Self {
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
            db_pool: storage::get().pool_owned(),
            config,
        }
    }

    /// 生成新的上下文（带自动生成的 log_id）
    pub fn new(user_id: Option<String>, username: Option<String>, config: Arc<AppConfig>) -> Self {
        Self {
            log_id: Self::generate_log_id(),
            user_id,
            username,
            organization_id: None,
            agent_id: None,
            task_id: None,
            project_id: None,
            db_pool: storage::get().pool_owned(),
            config,
        }
    }

    pub fn new_simple(user_id: &str,  db_pool: SqlitePool, config: Arc<AppConfig>) -> RequestContext {
        Self {
            log_id: Self::generate_log_id(),
            user_id: Some(user_id.to_string()),
            username: None,
            organization_id: None,
            agent_id: None,
            task_id: None,
            project_id: None,
            db_pool,
            config,
        }
    }

    /// 设置 log_id（用于中间件处理时覆盖自动生成的 log_id）
    pub fn set_log_id(&mut self, log_id: String) {
        self.log_id = log_id;
    }

    /// 设置组织 ID（JWT 解析结果会覆盖 header 中的值，以 JWT 为准）
    pub fn set_organization_id(&mut self, organization_id: String) {
        self.organization_id = Some(organization_id);
    }

    /// 设置当前 Agent ID
    pub fn set_agent_id(&mut self, agent_id: String) {
        self.agent_id = Some(agent_id);
    }

    /// 设置当前 Task ID
    pub fn set_task_id(&mut self, task_id: String) {
        self.task_id = Some(task_id);
    }

    /// 设置当前 Project ID
    pub fn set_project_id(&mut self, project_id: String) {
        self.project_id = Some(project_id);
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

    /// 获取应用配置
    pub fn app_config(&self) -> &AppConfig {
        &self.config
    }

    pub fn db_pool(&self) -> &SqlitePool {
        &self.db_pool
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


