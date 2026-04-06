//! Model Provider 管理实现
//!
//! 放在这里保持和 hr/agent.rs 相同的结构
//! 所有具体业务实现都在本文件中完成

use crate::error::AppError;
use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::service::dal::cortex::dal as cortex_dal;
use crate::service::dal::model_provider::ModelProviderDalTrait;
use crate::service::domain::finance::{FinanceDomainImpl, ModelProviderManage};
use std::sync::Arc;

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

    fn wake_cortex(&self, ctx: RequestContext, id: &str, prompt: &str) -> Result<String, AppError> {
        // 1. 查询 Model Provider
        let provider = self.model_provider_dal.find_by_id(ctx.clone(), id)?
            .ok_or_else(|| AppError::NotFound(format!("ModelProvider {} not found", id)))?;

        // 2. 调用 Cortex DAL 唤醒 cortex 执行调用
        let result = cortex_dal().wake_cortex(ctx, &provider, prompt)?;

        Ok(result)
    }
}
