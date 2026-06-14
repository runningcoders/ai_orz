//! Finance Domain 模块
//!
//! 财务领域模块，管理所有需要计费的外部能力配置与使用关系：
//! - ModelProvider - AI 模型提供商配置
//! - MessageChannel - 消息渠道配置
//! - ToolProvider - 外部工具提供商配置 + Agent 工具借用（绑定）关系

pub mod message_channel;
pub mod model_provider;
pub mod tool_provider;

#[cfg(test)]
mod model_provider_test;

#[cfg(test)]
mod message_channel_test;

#[cfg(test)]
mod tool_provider_test;

use crate::error::AppError;
use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::service::dal::brain::BrainDal;
use crate::service::dal::message_channel::MessageChannelDal;
use crate::service::dal::model_provider::ModelProviderDal;
use crate::service::dal::tool::ToolDal;
use async_trait::async_trait;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static FINANCE_DOMAIN: OnceLock<Arc<dyn FinanceDomain>> = OnceLock::new();

/// 获取 Finance Domain 单例
pub fn domain() -> Arc<dyn FinanceDomain> {
    FINANCE_DOMAIN.get().cloned().unwrap()
}

/// 创建新的 Finance Domain 实例（用于测试，每次测试创建独立实例保证隔离）
pub fn new(
    model_provider_dal: Arc<dyn ModelProviderDal>,
    message_channel_dal: Arc<dyn MessageChannelDal>,
    tool_dal: Arc<dyn ToolDal>,
    brain_dal: Arc<dyn BrainDal>,
) -> Arc<dyn FinanceDomain> {
    let domain =
        FinanceDomainImpl::new(model_provider_dal, message_channel_dal, tool_dal, brain_dal);
    Arc::new(domain)
}

/// 初始化 Finance Domain（使用全局单例 DAO）
pub fn init() {
    let finance_domain = FinanceDomainImpl::new(
        crate::service::dal::model_provider::dal(),
        crate::service::dal::message_channel::dal(),
        crate::service::dal::tool::dal(),
        crate::service::dal::brain::dal(),
    );
    let _ = FINANCE_DOMAIN.set(Arc::new(finance_domain));
}

// ==================== trait 定义 ====================

/// Finance Domain 总 trait
///
/// 聚合财务领域所有子功能 trait
pub trait FinanceDomain: Send + Sync {
    /// Model Provider 管理能力
    fn model_provider_manage(&self) -> &dyn ModelProviderManage;

    /// Message Channel 管理能力
    fn message_channel_manage(&self) -> &dyn MessageChannelManage;

    /// Tool Provider 管理能力（工具配置 + Agent 借用关系）
    fn tool_provider_manage(&self) -> &dyn ToolProviderManage;
}

/// Model Provider 管理 trait
///
/// 定义 Model Provider 相关的业务接口
#[async_trait]
pub trait ModelProviderManage: Send + Sync {
    /// 创建 Model Provider
    async fn create_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<(), AppError>;

    /// 获取 Model Provider
    async fn get_model_provider(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ModelProvider>, AppError>;

    /// 通用综合查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::model_provider::ModelProviderQuery,
    ) -> Result<Vec<ModelProvider>, AppError>;

    /// 列出所有 Model Provider
    async fn list_model_providers(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<ModelProvider>, AppError>;

    /// 更新 Model Provider
    async fn update_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<(), AppError>;

    /// 删除 Model Provider
    async fn delete_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<(), AppError>;

    /// 测试 Model Provider 连接
    async fn test_connection(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
        prompt: &str,
    ) -> Result<String, AppError>;
}

/// Message Channel 管理 trait
///
/// 定义 Message Channel 相关的业务接口
#[async_trait]
pub trait MessageChannelManage: Send + Sync {
    /// 创建 Message Channel
    async fn create_message_channel(
        &self,
        ctx: RequestContext,
        channel: &crate::models::message_channel::MessageChannel,
    ) -> Result<(), AppError>;

    /// 获取 Message Channel
    async fn get_message_channel(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<crate::models::message_channel::MessageChannel>, AppError>;

    /// 通用综合查询
    async fn query_channels(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::message_channel::MessageChannelQuery,
    ) -> Result<Vec<crate::models::message_channel::MessageChannel>, AppError>;

    /// 列出所有 Message Channel
    async fn list_message_channels(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<crate::models::message_channel::MessageChannel>, AppError>;

    /// 更新 Message Channel
    async fn update_message_channel(
        &self,
        ctx: RequestContext,
        channel: &crate::models::message_channel::MessageChannel,
    ) -> Result<(), AppError>;

    /// 删除 Message Channel
    async fn delete_message_channel(
        &self,
        ctx: RequestContext,
        channel: &crate::models::message_channel::MessageChannel,
    ) -> Result<(), AppError>;

    /// 测试 Message Channel 连通性
    async fn test_message_channel(
        &self,
        ctx: RequestContext,
        channel: &crate::models::message_channel::MessageChannel,
    ) -> Result<(), AppError>;
}

/// Tool Provider 管理 trait
///
/// 定义 Tool Provider 相关的业务接口
#[async_trait]
pub trait ToolProviderManage: Send + Sync {
    /// 创建 Tool
    async fn create_tool(
        &self,
        ctx: RequestContext,
        tool: &crate::models::tool::Tool,
    ) -> Result<(), AppError>;

    /// 获取 Tool
    async fn get_tool(
        &self,
        ctx: RequestContext,
        tool_id: &str,
    ) -> Result<Option<crate::models::tool::Tool>, AppError>;

    /// 通用综合查询
    async fn query_tools(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::tool::ToolQuery,
    ) -> Result<Vec<crate::models::tool::Tool>, AppError>;

    /// 列出所有 Tool
    async fn list_tools(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<crate::models::tool::Tool>, AppError>;

    /// 更新 Tool
    async fn update_tool(
        &self,
        ctx: RequestContext,
        tool: &crate::models::tool::Tool,
    ) -> Result<(), AppError>;

    /// 删除 Tool
    async fn delete_tool(
        &self,
        ctx: RequestContext,
        tool: &crate::models::tool::Tool,
    ) -> Result<(), AppError>;

    /// ===== 工具借用（绑定）管理 =====
    /// Agent 借用工具（绑定）
    async fn bind_tool_to_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<(), AppError>;

    /// Agent 归还工具（解绑）
    async fn unbind_tool_from_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<(), AppError>;

    /// 获取 Agent 借用的所有工具 ID
    async fn get_agent_bound_tool_ids(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<String>, AppError>;

    /// 获取 Agent 借用的所有工具
    async fn list_agent_tools(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<crate::models::tool::Tool>, AppError>;

    /// 搜索工具（向量 + 关键词混合搜索）
    ///
    /// Agent 思考时选择工具用
    async fn search_tools(
        &self,
        ctx: RequestContext,
        params: crate::service::dao::tool::ToolSearch,
    ) -> Result<Vec<crate::models::tool::Tool>, AppError>;
}

// ==================== 实现 ====================

/// Finance Domain 实现
///
/// 聚合所有财务子功能实现
pub struct FinanceDomainImpl {
    pub model_provider_dal: Arc<dyn ModelProviderDal>,
    pub message_channel_dal: Arc<dyn MessageChannelDal>,
    pub tool_dal: Arc<dyn ToolDal>,
    pub brain_dal: Arc<dyn BrainDal>,
}

impl FinanceDomainImpl {
    /// 创建 Domain 实例
    pub fn new(
        model_provider_dal: Arc<dyn ModelProviderDal>,
        message_channel_dal: Arc<dyn MessageChannelDal>,
        tool_dal: Arc<dyn ToolDal>,
        brain_dal: Arc<dyn BrainDal>,
    ) -> Self {
        Self {
            model_provider_dal,
            message_channel_dal,
            tool_dal,
            brain_dal,
        }
    }
}

impl FinanceDomain for FinanceDomainImpl {
    fn model_provider_manage(&self) -> &dyn ModelProviderManage {
        self
    }

    fn message_channel_manage(&self) -> &dyn MessageChannelManage {
        self
    }

    fn tool_provider_manage(&self) -> &dyn ToolProviderManage {
        self
    }
}
