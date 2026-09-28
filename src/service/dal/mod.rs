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

/// 全量向量重建（`rebuild_vectors`）的每页条数
///
/// 各 DAL 按此大小逐页取出实体重建，避免一次性把全表载入内存
/// （messages 可能到十万级以上）。取值权衡：
/// - 过小 → 页数多、SQL round-trip 与分页开销占比高
/// - 过大 → 单页实体常驻内存，且给 embedding 侧的压力峰值更高
///
/// embedding 调用才是真正的瓶颈，200 条/页是与之匹配的粒度。
pub const VECTOR_REBUILD_PAGE_SIZE: usize = 200;

pub mod agent;
pub mod artifact;
pub mod attachment;
pub mod backup;
pub mod brain;
pub mod cron_trigger;
pub mod email;
pub mod lark;
pub mod log_query;
pub mod mcp_server;
pub mod mcp_tool;
pub mod memory;
pub mod message;
pub mod message_channel;
pub mod message_push;
pub mod model_provider;
pub mod ontology;
pub mod organization;
pub mod project;
pub mod skill;
pub mod task;
pub mod tool;
pub mod user;
pub mod wechat;

pub fn init_all() {
    agent::init();
    artifact::init();
    attachment::init();
    backup::init();
    brain::init();
    cron_trigger::init();
    log_query::init();
    memory::init();
    // ontology dal 组合 ontology dao + memory dao（观察者侧组合，对被观察者零感知）
    ontology::init();
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
    // wechat dal 同理（依赖 message_channel dal + wechat dao + user_credential dao）
    wechat::init();
    // email dal 同理（依赖 message_channel dal + email dao + user_credential dao + message dao）
    email::init();
}

#[cfg(test)]
pub(crate) mod artifact_test;
#[cfg(test)]
pub(crate) mod attachment_test;
#[cfg(test)]
pub(crate) mod brain_test;
#[cfg(test)]
pub(crate) mod lark_test;
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
