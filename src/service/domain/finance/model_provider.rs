//! Model Provider 管理实现
//!
//! 放在这里保持和 hr/agent.rs 相同的结构
//! 所有具体业务实现都在本文件中完成

use crate::error::AppError;
use crate::models::model_provider::ModelProvider;
use common::constants::RequestContext;
use crate::service::dal::brain::dal as brain_dal;
use crate::service::dal::model_provider::ModelProviderDalTrait;
use super::{FinanceDomainImpl};
use async_trait::async_trait;
use super::ModelProviderManage;

#[async_trait]
impl ModelProviderManage for FinanceDomainImpl {
    fn create_model_provider(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError> {
        self.model_provider_dal.create(ctx, provider)
    }

    fn get_model_provider(&self, ctx: RequestContext, id: &str) -> Result<Option<ModelProvider>, AppError> {
        self.model_provider_dal.find_by_id(ctx, id)
    }

    fn list_model_providers(&self, ctx: RequestContext) -> Result<Vec<ModelProvider>, AppError> {
        self.model_provider_dal.find_all(ctx)
    }

    fn update_model_provider(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError> {
        self.model_provider_dal.update(ctx, provider)
    }

    fn delete_model_provider(&self, ctx: RequestContext, provider: &ModelProvider) -> Result<(), AppError> {
        self.model_provider_dal.delete(ctx, provider)
    }

    /// 唤醒 Cortex：创建临时 Cortex 并执行调用
    ///
    /// 上层已经查询好 Model Provider，直接传递进来
    /// - provider: 模型提供商
    /// - prompt: 调用提示词
    /// - 返回: 模型输出结果
    async fn wake_cortex(&self, ctx: RequestContext, provider: &ModelProvider, prompt: &str) -> Result<String, AppError> {
        // 直接调用 Brain DAL 测试连接执行调用
        brain_dal().test_connection(ctx, provider, prompt).await
    }
}
