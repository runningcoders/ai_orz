//! DAL 层（数据访问层）
//!
//! DAL 层是业务逻辑层，不关心具体的存储细节
//! 它组合多个 DAO 完成业务逻辑，使用业务对象而非 Po

/// 统一搜索参数
#[derive(Debug, Clone, Default)]
pub struct SearchParams {
    /// 搜索关键词
    pub keyword: String,
    /// 返回数量限制
    pub limit: usize,
    /// 分页偏移
    pub offset: Option<usize>,
    /// 是否只使用向量搜索
    pub vector_only: Option<bool>,
}

impl SearchParams {
    pub fn new(keyword: impl Into<String>, limit: usize) -> Self {
        Self {
            keyword: keyword.into(),
            limit,
            offset: None,
            vector_only: None,
        }
    }
}

pub mod agent;
pub mod artifact;
pub mod attachment;
pub mod backup;
pub mod brain;
pub mod cron_trigger;
pub mod lark;
pub mod log_query;
pub mod mcp_server;
pub mod mcp_tool;
pub mod memory;
pub mod message;
pub mod message_channel;
pub mod message_push;
pub mod model_provider;
pub mod organization;
pub mod project;
pub mod skill;
pub mod task;
pub mod tool;
pub mod user;

pub fn init_all() {
    agent::init();
    artifact::init();
    attachment::init();
    backup::init();
    brain::init();
    cron_trigger::init();
    log_query::init();
    memory::init();
    message::init();
    message_channel::init();
    model_provider::init();
    organization::init();
    project::init();
    skill::init();
    task::init();
    tool::init();
    mcp_server::init();
    mcp_tool::init();
    user::init();
    // lark dal 依赖 message_channel + agent dal，最后初始化
    lark::init();
}

#[cfg(test)]
pub(crate) mod agent_test;
#[cfg(test)]
pub(crate) mod artifact_test;
#[cfg(test)]
pub(crate) mod attachment_test;
#[cfg(test)]
pub(crate) mod brain_test;
#[cfg(test)]
pub(crate) mod mcp_server_test;
#[cfg(test)]
pub(crate) mod mcp_tool_test;
#[cfg(test)]
pub(crate) mod memory_test;
#[cfg(test)]
pub(crate) mod message_channel_test;
#[cfg(test)]
pub(crate) mod message_test;
#[cfg(test)]
pub(crate) mod model_provider_test;
#[cfg(test)]
pub(crate) mod organization_test;
#[cfg(test)]
pub(crate) mod project_test;
#[cfg(test)]
pub(crate) mod skill_test;
#[cfg(test)]
pub(crate) mod task_test;
#[cfg(test)]
pub(crate) mod tool_test;
#[cfg(test)]
pub(crate) mod user_test;
