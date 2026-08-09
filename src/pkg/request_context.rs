use crate::pkg::storage::{self, Storage, VectorStore};
/// 请求上下文（贯穿整个请求生命周期）
use ai_orz_macros::LogFields;
use axum::http;
use common::constants::http_header;
use common::enums::{CallerType, MessageRole};
use sqlx::sqlite::SqlitePool;
use std::sync::Arc;

/// 请求上下文
///
/// 【不可变约定】构建完成后即为不可变对象。
/// 如需修改，通过 `ctx.to_builder()` 克隆并重建。
///
/// 【字段分类】
/// - 追踪标识：log_id
/// - 用户身份：user_id, username, user_role
/// - 组织维度：organization_id
/// - 业务维度：agent_id, project_id, task_id
/// - 模型维度：model_provider_id, model_name
/// - 基础设施：storage（SQLite + Vector + Stats）
#[derive(Debug, Clone, LogFields)]
pub struct RequestContext {
    /// 日志追踪 ID
    #[log_field]
    pub log_id: String,
    /// 当前用户 ID
    #[log_field]
    pub user_id: Option<String>,
    /// 当前用户名
    #[log_field]
    pub username: Option<String>,
    /// 当前组织 ID
    #[log_field]
    pub organization_id: Option<String>,
    /// 当前用户角色（数值，对应 UserRole 枚举）
    #[log_field]
    pub user_role: Option<i32>,
    /// 调用方类型（User/Agent/System），默认 User
    #[log_field]
    pub caller_type: common::enums::CallerType,

    /// 当前 Agent ID（可选，Agent 执行时有值）
    #[log_field]
    pub agent_id: Option<String>,
    /// 当前 Task ID（可选，Task 执行时有值）
    #[log_field]
    pub task_id: Option<String>,
    /// 当前 Project ID（可选，Project 上下文时有值）
    #[log_field]
    pub project_id: Option<String>,
    /// 当前 Model Provider ID（可选，Cortex 创建时有值）
    #[log_field]
    pub model_provider_id: Option<String>,
    /// 当前 Model 名称（可选，Cortex 创建时有值）
    #[log_field]
    pub model_name: Option<String>,

    /// 当前工具调用 ID（可选，ToolCallDao::execute 单点注入/业务指定幂等键）
    #[log_field]
    pub tool_call_id: Option<String>,

    /// 统一存储门面（SQLite + Vector）
    storage: Storage,
}

// ==================== Builder ====================

/// RequestContext 构建器
///
/// 【使用方式】
/// ```ignore
/// // 从零构建
/// let ctx = RequestContext::builder()
///     .user_id("user-001")
///     .organization_id("org-001")
///     .build();
///
/// // 从现有上下文扩展
/// let new_ctx = ctx.to_builder()
///     .agent_id("agent-001")
///     .project_id("proj-001")
///     .build();
/// ```
#[derive(Debug)]
pub struct RequestContextBuilder {
    log_id: Option<String>,
    user_id: Option<String>,
    username: Option<String>,
    organization_id: Option<String>,
    user_role: Option<i32>,
    caller_type: Option<CallerType>,
    agent_id: Option<String>,
    task_id: Option<String>,
    project_id: Option<String>,
    model_provider_id: Option<String>,
    model_name: Option<String>,
    tool_call_id: Option<String>,
    storage: Option<Storage>,
}

impl RequestContextBuilder {
    pub fn new() -> Self {
        Self {
            log_id: None,
            user_id: None,
            username: None,
            organization_id: None,
            user_role: None,
            caller_type: None,
            agent_id: None,
            task_id: None,
            project_id: None,
            model_provider_id: None,
            model_name: None,
            tool_call_id: None,
            storage: None,
        }
    }

    pub fn log_id(mut self, log_id: impl Into<String>) -> Self {
        self.log_id = Some(log_id.into());
        self
    }

    pub fn user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    pub fn username(mut self, username: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self
    }

    pub fn organization_id(mut self, organization_id: impl Into<String>) -> Self {
        self.organization_id = Some(organization_id.into());
        self
    }

    pub fn user_role(mut self, role: i32) -> Self {
        self.user_role = Some(role);
        self
    }

    /// 设置调用方类型
    pub fn caller_type(mut self, ct: CallerType) -> Self {
        self.caller_type = Some(ct);
        self
    }

    /// 条件设置调用方类型（None 时跳过）
    pub fn try_caller_type(mut self, ct: Option<impl Into<CallerType>>) -> Self {
        if let Some(c) = ct {
            self.caller_type = Some(c.into());
        }
        self
    }

    pub fn agent_id(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    pub fn task_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    pub fn project_id(mut self, project_id: impl Into<String>) -> Self {
        self.project_id = Some(project_id.into());
        self
    }

    pub fn model_provider_id(mut self, model_provider_id: impl Into<String>) -> Self {
        self.model_provider_id = Some(model_provider_id.into());
        self
    }

    pub fn model_name(mut self, model_name: impl Into<String>) -> Self {
        self.model_name = Some(model_name.into());
        self
    }

    pub fn tool_call_id(mut self, tool_call_id: impl Into<String>) -> Self {
        self.tool_call_id = Some(tool_call_id.into());
        self
    }

    pub fn try_user_id(mut self, user_id: Option<impl Into<String>>) -> Self {
        if let Some(v) = user_id {
            self.user_id = Some(v.into());
        }
        self
    }

    pub fn try_username(mut self, username: Option<impl Into<String>>) -> Self {
        if let Some(v) = username {
            self.username = Some(v.into());
        }
        self
    }

    pub fn try_organization_id(mut self, organization_id: Option<impl Into<String>>) -> Self {
        if let Some(v) = organization_id {
            self.organization_id = Some(v.into());
        }
        self
    }

    pub fn try_user_role(mut self, user_role: Option<i32>) -> Self {
        if let Some(v) = user_role {
            self.user_role = Some(v);
        }
        self
    }

    pub fn try_agent_id(mut self, agent_id: Option<impl Into<String>>) -> Self {
        if let Some(v) = agent_id {
            self.agent_id = Some(v.into());
        }
        self
    }

    pub fn try_task_id(mut self, task_id: Option<impl Into<String>>) -> Self {
        if let Some(v) = task_id {
            self.task_id = Some(v.into());
        }
        self
    }

    pub fn try_project_id(mut self, project_id: Option<impl Into<String>>) -> Self {
        if let Some(v) = project_id {
            self.project_id = Some(v.into());
        }
        self
    }

    pub fn try_model_provider_id(mut self, model_provider_id: Option<impl Into<String>>) -> Self {
        if let Some(v) = model_provider_id {
            self.model_provider_id = Some(v.into());
        }
        self
    }

    pub fn try_model_name(mut self, model_name: Option<impl Into<String>>) -> Self {
        if let Some(v) = model_name {
            self.model_name = Some(v.into());
        }
        self
    }

    pub fn try_tool_call_id(mut self, tool_call_id: Option<impl Into<String>>) -> Self {
        if let Some(v) = tool_call_id {
            self.tool_call_id = Some(v.into());
        }
        self
    }

    pub fn storage(mut self, storage: Storage) -> Self {
        self.storage = Some(storage);
        self
    }

    pub fn build(self) -> RequestContext {
        RequestContext {
            log_id: self.log_id.unwrap_or_else(RequestContext::generate_log_id),
            user_id: self.user_id,
            username: self.username,
            organization_id: self.organization_id,
            user_role: self.user_role,
            caller_type: self.caller_type.unwrap_or_default(),
            agent_id: self.agent_id,
            task_id: self.task_id,
            project_id: self.project_id,
            model_provider_id: self.model_provider_id,
            model_name: self.model_name,
            tool_call_id: self.tool_call_id,
            storage: self.storage.unwrap_or_else(|| storage::get().clone()),
        }
    }
}

impl Default for RequestContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestContext {
    /// 创建一个新的 Builder（从零构建）
    pub fn builder() -> RequestContextBuilder {
        RequestContextBuilder::new()
    }

    /// 从当前上下文克隆后创建 Builder（扩展已有上下文）
    pub fn to_builder(&self) -> RequestContextBuilder {
        RequestContextBuilder {
            log_id: Some(self.log_id.clone()),
            user_id: self.user_id.clone(),
            username: self.username.clone(),
            organization_id: self.organization_id.clone(),
            user_role: self.user_role,
            caller_type: Some(self.caller_type),
            agent_id: self.agent_id.clone(),
            task_id: self.task_id.clone(),
            project_id: self.project_id.clone(),
            model_provider_id: self.model_provider_id.clone(),
            model_name: self.model_name.clone(),
            tool_call_id: self.tool_call_id.clone(),
            storage: Some(self.storage.clone()),
        }
    }

    /// 从 header 中提取上下文
    pub fn from_headers(headers: &http::HeaderMap) -> Self {
        // 1. 优先从 header 获取 log_id
        let log_id = headers
            .get(http_header::LOG_ID)
            .and_then(|v: &http::HeaderValue| v.to_str().ok())
            .map(|s| s.to_string());

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

        // 4. 从 header 获取用户角色（JWT 中间件注入的 X-User-Role 数值）
        let user_role = headers
            .get(http_header::USER_ROLE)
            .and_then(|v: &http::HeaderValue| v.to_str().ok())
            .and_then(|s| s.parse::<i32>().ok());

        // 5. 解析 caller_type：优先从 X-Caller-Type header 读取
        let caller_type = headers
            .get(http_header::CALLER_TYPE)
            .and_then(|v: &http::HeaderValue| v.to_str().ok())
            .map(|s| match s.to_lowercase().as_str() {
                "user" | "0" => CallerType::User,
                "agent" | "1" => CallerType::Agent,
                "system" | "2" => CallerType::System,
                _ => CallerType::User,
            })
            .unwrap_or(CallerType::User);

        let mut builder = Self::builder();
        if let Some(id) = log_id {
            builder = builder.log_id(id);
        }
        if let Some(id) = user_id {
            builder = builder.user_id(id);
        }
        if let Some(name) = username {
            builder = builder.username(name);
        }
        if let Some(id) = organization_id {
            builder = builder.organization_id(id);
        }
        if let Some(role) = user_role {
            builder = builder.user_role(role);
        }
        builder = builder.caller_type(caller_type);
        builder.build()
    }

    /// 生成新的上下文（带自动生成的 log_id）
    pub fn new(user_id: Option<String>, username: Option<String>) -> Self {
        let mut builder = Self::builder();
        if let Some(id) = user_id {
            builder = builder.user_id(id);
        }
        if let Some(name) = username {
            builder = builder.username(name);
        }
        builder.build()
    }

    /// 创建 System 调用方的 ctx（用于 Cron、A2A 回调、AOP 调度等系统触发场景）
    pub fn new_system() -> Self {
        Self::builder().caller_type(CallerType::System).build()
    }

    /// 从指定 Storage 创建上下文（测试辅助，由 test_support 调用）
    pub fn from_storage(user_id: &str, storage: Storage) -> Self {
        Self::builder()
            .user_id(user_id.to_string())
            .storage(storage)
            .build()
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

    /// 获取当前 User Role（数值，对应 UserRole 枚举）
    pub fn user_role(&self) -> Option<i32> {
        self.user_role
    }

    /// 获取调用方类型
    pub fn caller_type(&self) -> CallerType {
        self.caller_type
    }

    /// 获取调用方 ID（用于 stats operator_id 等场景）
    ///
    /// - Agent → agent_id
    /// - User → user_id
    /// - System → None（系统触发无操作者 ID）
    pub fn caller_id(&self) -> Option<String> {
        match self.caller_type {
            CallerType::Agent => self.agent_id.clone(),
            CallerType::User => self.user_id.clone(),
            CallerType::System => None,
        }
    }

    /// 获取调用方 ID，System 场景回退为 "system"（用于消息发送 from_id 等场景）
    pub fn caller_id_or_system(&self) -> String {
        self.caller_id().unwrap_or_else(|| "system".to_string())
    }

    /// 调用方类型映射为 MessageRole（用于消息发送 from_role 场景）
    pub fn caller_role(&self) -> MessageRole {
        match self.caller_type {
            CallerType::Agent => MessageRole::Agent,
            CallerType::User => MessageRole::User,
            CallerType::System => MessageRole::System,
        }
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

    /// 获取当前工具调用 ID（由 ToolCallDao::execute 注入，或业务指定作为幂等键）
    pub fn tool_call_id(&self) -> Option<&String> {
        self.tool_call_id.as_ref()
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

    for (i, &dim) in days_in_months.iter().enumerate() {
        if d < dim {
            month = i + 1;
            day = d + 1;
            break;
        }
        d -= dim;
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

// ==================== 上下文增强 ====================

/// 上下文增强 trait
///
/// 实体自己声明如何把字段注入到 RequestContextBuilder。
/// 字段映射规则集中在实体定义处，调用方通过 `enrich_ctx!` 宏串联。
///
/// 【覆盖规则】
/// - 实体字段有值（Some）时，覆盖 builder 中已有的值
/// - 实体字段为 None 时，跳过，保留 builder 中已有值
///
/// 符合树形扩散模型：越靠近数据层的信息优先级越高。
///
/// 【设计约束】
/// 上下文只存简单信息（ID、名称等），业务实体通过方法参数显式传递。
/// RequestContext 永远不依赖 models 模块，避免循环引用。
pub trait EnrichContext {
    /// 将实体字段注入 builder，返回新的 builder
    fn enrich(&self, builder: RequestContextBuilder) -> RequestContextBuilder;
}

/// 上下文增强宏
///
/// 接受一个 ctx 和多个实体，依次调用每个实体的 `enrich` 方法，
/// 最后生成新的 RequestContext。
///
/// 【用法】
/// ```text
/// let new_ctx = enrich_ctx!(&ctx, &agent, &project, &task);
/// ```
///
/// 等价于：
/// ```text
/// let mut builder = ctx.to_builder();
/// builder = agent.enrich(builder);
/// builder = project.enrich(builder);
/// builder = task.enrich(builder);
/// let new_ctx = builder.build();
/// ```
#[macro_export]
macro_rules! enrich_ctx {
    ($ctx:expr, $($entity:expr),* $(,)?) => {{
        use $crate::pkg::request_context::EnrichContext;
        let mut builder = ($ctx).to_builder();
        $(
            builder = ($entity).enrich(builder);
        )*
        builder.build()
    }};
}
