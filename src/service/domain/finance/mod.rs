//! Finance Domain 模块
//!
//! 财务领域模块，管理所有需要计费的外部能力配置与使用关系：
//! - ModelProvider - AI 模型提供商配置
//! - MessageChannel - 消息渠道配置
//! - ToolProvider - 外部工具提供商配置 + Agent 工具借用（绑定）关系

pub mod attachment;
pub mod mcp_server;
pub mod mcp_tool;
pub mod message_channel;
pub mod model_provider;
pub mod tool_provider;

#[cfg(test)]
mod attachment_test;

#[cfg(test)]
mod model_provider_test;

#[cfg(test)]
mod message_channel_test;

#[cfg(test)]
mod mcp_server_test;

#[cfg(test)]
mod tool_provider_test;

use common::error::Result;
use crate::models::attachment::{
    Attachment, AttachmentGetOptions, AttachmentReadResult, AttachmentTextContent,
    AttachmentUpload, TextAttachmentCreate, TextContentUpdate,
};
use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::service::dal::attachment::AttachmentDal;
use crate::service::dal::brain::BrainDal;
use crate::service::dal::mcp_server::McpServerDal;
use crate::service::dal::mcp_tool::McpToolDal;
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
    mcp_server_dal: Arc<dyn McpServerDal + Send + Sync>,
    mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
    tool_dal: Arc<dyn ToolDal>,
    brain_dal: Arc<dyn BrainDal>,
    attachment_dal: Arc<dyn AttachmentDal + Send + Sync>,
) -> Arc<dyn FinanceDomain> {
    let domain = FinanceDomainImpl::new(
        model_provider_dal,
        message_channel_dal,
        mcp_server_dal,
        mcp_tool_dal,
        tool_dal,
        brain_dal,
        attachment_dal,
    );
    Arc::new(domain)
}

/// 初始化 Finance Domain（使用全局单例 DAO）
pub fn init() {
    let finance_domain = FinanceDomainImpl::new(
        crate::service::dal::model_provider::dal(),
        crate::service::dal::message_channel::dal(),
        crate::service::dal::mcp_server::dal(),
        crate::service::dal::mcp_tool::dal(),
        crate::service::dal::tool::dal(),
        crate::service::dal::brain::dal(),
        crate::service::dal::attachment::dal(),
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

    /// MCP Server 管理能力（外部 MCP Provider 配置）
    fn mcp_server_manage(&self) -> &dyn McpServerManage;

    /// MCP Tool 管理能力（从 MCP Provider 同步/管理工具）
    fn mcp_tool_manage(&self) -> &dyn McpToolManage;

    /// Attachment 管理能力（通用上传文件资产）
    fn attachment_manage(&self) -> &dyn AttachmentManage;
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
    ) -> Result<()>;

    /// 获取 Model Provider
    async fn get_model_provider(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ModelProvider>>;

    /// 获取 Model Provider（带附带信息选项）
    async fn get_model_provider_with_options(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::model_provider::ModelProviderFetchOptions,
    ) -> Result<Option<ModelProvider>>;

    /// 通用综合查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::model_provider::ModelProviderQuery,
    ) -> Result<Vec<ModelProvider>>;

    /// 列出所有 Model Provider
    async fn list_model_providers(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<ModelProvider>>;

    /// 更新 Model Provider
    async fn update_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<()>;

    /// 删除 Model Provider
    async fn delete_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<()>;

    /// 测试 Model Provider 连接
    async fn test_connection(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
        prompt: &str,
    ) -> Result<String>;

    /// 切换 Embedding Provider（原子操作：禁用旧 → 启用新 → 重建索引）
    async fn switch_embedding_provider(
        &self,
        ctx: RequestContext,
        new_provider_id: &str,
    ) -> Result<Option<ModelProvider>>;
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
    ) -> Result<()>;

    /// 获取 Message Channel
    async fn get_message_channel(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<crate::models::message_channel::MessageChannel>>;

    /// 通用综合查询
    async fn query_channels(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::message_channel::MessageChannelQuery,
    ) -> Result<Vec<crate::models::message_channel::MessageChannel>>;

    /// 列出所有 Message Channel
    async fn list_message_channels(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<crate::models::message_channel::MessageChannel>>;

    /// 更新 Message Channel
    async fn update_message_channel(
        &self,
        ctx: RequestContext,
        channel: &crate::models::message_channel::MessageChannel,
    ) -> Result<()>;

    /// 删除 Message Channel
    async fn delete_message_channel(
        &self,
        ctx: RequestContext,
        channel: &crate::models::message_channel::MessageChannel,
    ) -> Result<()>;

    /// 测试 Message Channel 连通性
    async fn test_message_channel(
        &self,
        ctx: RequestContext,
        channel: &crate::models::message_channel::MessageChannel,
    ) -> Result<()>;
}

/// Attachment 管理 trait
///
/// 定义通用上传文件资产相关的业务接口。
#[async_trait]
pub trait AttachmentManage: Send + Sync {
    /// 创建上传文件资产。
    async fn create_attachment(
        &self,
        ctx: RequestContext,
        upload: AttachmentUpload,
    ) -> Result<Attachment>;

    /// 创建小型 UTF-8 文本 Attachment。
    async fn create_text_attachment(
        &self,
        ctx: RequestContext,
        create: TextAttachmentCreate,
    ) -> Result<Attachment>;

    /// 获取上传文件资产。
    async fn get_attachment(
        &self,
        ctx: RequestContext,
        id: &str,
        options: AttachmentGetOptions,
    ) -> Result<Option<Attachment>>;

    /// 读取 Attachment UTF-8 文本内容。
    async fn get_attachment_text_content(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<AttachmentTextContent>>;

    /// 全量替换 Attachment UTF-8 文本内容。
    async fn update_attachment_text_content(
        &self,
        ctx: RequestContext,
        id: &str,
        update: TextContentUpdate,
    ) -> Result<Option<AttachmentTextContent>>;

    /// 查询上传文件资产。
    async fn query_attachments(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::attachment::AttachmentQuery,
    ) -> Result<Vec<Attachment>>;

    /// 删除上传文件资产。
    async fn delete_attachment(&self, ctx: RequestContext, id: &str) -> Result<()>;
}

/// MCP Server 管理 trait
///
/// 定义 MCP Server Provider 配置相关的业务接口
#[async_trait]
pub trait McpServerManage: Send + Sync {
    /// 创建 MCP Server
    async fn create_mcp_server(
        &self,
        ctx: RequestContext,
        server: &crate::models::mcp_server::McpServer,
    ) -> Result<()>;

    /// 获取 MCP Server
    async fn get_mcp_server(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<crate::models::mcp_server::McpServer>>;

    /// 通用综合查询
    async fn query_mcp_servers(
        &self,
        ctx: RequestContext,
        query: crate::models::mcp_server::McpServerQuery,
    ) -> Result<common::api::PagedResult<crate::models::mcp_server::McpServer>>;

    /// 列出所有 MCP Server
    async fn list_mcp_servers(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<crate::models::mcp_server::McpServer>>;

    /// 更新 MCP Server
    async fn update_mcp_server(
        &self,
        ctx: RequestContext,
        server: &crate::models::mcp_server::McpServer,
    ) -> Result<()>;

    /// 更新 MCP Server 状态
    async fn update_mcp_server_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: crate::models::mcp_server::McpServerStatus,
    ) -> Result<()>;

    /// 删除 MCP Server
    async fn delete_mcp_server(&self, ctx: RequestContext, id: &str) -> Result<()>;
}

/// MCP Tool 管理 trait
///
/// 定义 MCP Server 暴露工具的同步与查询接口。
#[async_trait]
pub trait McpToolManage: Send + Sync {
    /// 从指定 MCP Server 同步远端 tools/list 到本地 Tool 记录。
    async fn sync_mcp_tools(&self, ctx: RequestContext, server_id: &str)
    -> Result<usize>;

    /// 查询指定 MCP Server 绑定的本地 MCP Tool 记录。
    async fn list_mcp_tools_by_server(
        &self,
        ctx: RequestContext,
        params: common::api::ListMcpToolsByServerRequest,
    ) -> Result<common::api::PagedResult<crate::models::tool::Tool>>;
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
    ) -> Result<()>;

    /// 获取 Tool
    async fn get_tool(
        &self,
        ctx: RequestContext,
        tool_id: &str,
    ) -> Result<Option<crate::models::tool::Tool>>;

    /// 获取 Tool（带附带信息选项）
    async fn get_tool_with_options(
        &self,
        ctx: RequestContext,
        tool_id: &str,
        options: crate::service::dal::tool::ToolFetchOptions,
    ) -> Result<Option<crate::models::tool::Tool>>;

    /// 通用综合查询
    async fn query_tools(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::tool::ToolQuery,
    ) -> Result<Vec<crate::models::tool::Tool>>;

    /// 列出所有 Tool
    async fn list_tools(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<crate::models::tool::Tool>>;

    /// 更新 Tool
    async fn update_tool(
        &self,
        ctx: RequestContext,
        tool: &crate::models::tool::Tool,
    ) -> Result<()>;

    /// 删除 Tool
    async fn delete_tool(
        &self,
        ctx: RequestContext,
        tool: &crate::models::tool::Tool,
    ) -> Result<()>;

    /// ===== 工具借用（绑定）管理 =====
    /// Agent 借用工具（绑定）
    async fn bind_tool_to_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<()>;

    /// Agent 归还工具（解绑）
    async fn unbind_tool_from_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<()>;

    /// 获取 Agent 借用的所有工具 ID
    async fn get_agent_bound_tool_ids(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<String>>;

    /// 获取 Agent 借用的所有工具
    async fn list_agent_tools(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<crate::models::tool::Tool>>;

    /// 搜索工具（向量 + 关键词混合搜索）
    ///
    /// Agent 思考时选择工具用
    async fn search_tools(
        &self,
        ctx: RequestContext,
        params: crate::service::dao::tool::ToolSearch,
    ) -> Result<Vec<crate::models::tool::Tool>>;
}

// ==================== 实现 ====================

/// Finance Domain 实现
///
/// 聚合所有财务子功能实现
pub struct FinanceDomainImpl {
    pub model_provider_dal: Arc<dyn ModelProviderDal>,
    pub message_channel_dal: Arc<dyn MessageChannelDal>,
    pub mcp_server_dal: Arc<dyn McpServerDal + Send + Sync>,
    pub mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
    pub tool_dal: Arc<dyn ToolDal>,
    pub brain_dal: Arc<dyn BrainDal>,
    pub attachment_dal: Arc<dyn AttachmentDal + Send + Sync>,
}

impl FinanceDomainImpl {
    /// 创建 Domain 实例
    pub fn new(
        model_provider_dal: Arc<dyn ModelProviderDal>,
        message_channel_dal: Arc<dyn MessageChannelDal>,
        mcp_server_dal: Arc<dyn McpServerDal + Send + Sync>,
        mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
        tool_dal: Arc<dyn ToolDal>,
        brain_dal: Arc<dyn BrainDal>,
        attachment_dal: Arc<dyn AttachmentDal + Send + Sync>,
    ) -> Self {
        Self {
            model_provider_dal,
            message_channel_dal,
            mcp_server_dal,
            mcp_tool_dal,
            tool_dal,
            brain_dal,
            attachment_dal,
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

    fn mcp_server_manage(&self) -> &dyn McpServerManage {
        self
    }

    fn mcp_tool_manage(&self) -> &dyn McpToolManage {
        self
    }

    fn attachment_manage(&self) -> &dyn AttachmentManage {
        self
    }
}
