//! 初始化系统接口
//!
//! 当系统还没有初始化时，调用这个接口创建第一个组织、超级管理员和默认 ModelProvider
//! Handler 层负责跨 domain 编排：organization domain 创建 org+user，finance domain 创建 provider

use crate::pkg::RequestContext;
use crate::service::domain::{finance, organization};
use ai_orz_macros::generate_http_handler;
use common::api::{CheckInitializedRequest, InitializeSystemRequest, InitializeSystemResponse};
use common::error::Result;

/// 检查系统是否已经初始化
#[generate_http_handler]
pub async fn check_initialized(
    ctx: RequestContext,
    _params: CheckInitializedRequest,
) -> Result<bool> {
    let domain = organization::domain();
    let initialized = domain.organization_manage().check_initialized(ctx).await?;
    Ok(initialized)
}

/// 初始化系统
#[generate_http_handler]
pub async fn initialize_system(
    ctx: RequestContext,
    params: InitializeSystemRequest,
) -> Result<InitializeSystemResponse> {
    // 1. organization domain 创建组织 + Owner
    let (org_id, user_id) = organization::domain()
        .organization_manage()
        .create_org_and_owner(ctx.clone(), params.clone())
        .await?;

    // 2. finance domain 创建 chat provider（Agent 思考用）
    let chat_provider = crate::models::model_provider::ModelProvider::new(
        params.chat_model.name,
        common::enums::ProviderType::from_i32(params.chat_model.provider_type),
        common::enums::ModelCapability::Agent,
        params.chat_model.model_name,
        params.chat_model.api_key,
        params.chat_model.base_url,
        params.chat_model.description,
        user_id.clone(),
    );
    let chat_provider_id = chat_provider.po.id.clone();
    finance::domain()
        .model_provider_manage()
        .create_model_provider(ctx.clone(), &chat_provider)
        .await?;

    // 3. finance domain 创建 embedding provider（向量索引用）
    //    embedding_model 为 None 时跳过 — 调用方不配置向量索引
    let embedding_provider_id = if let Some(embedding_config) = params.embedding_model {
        let embedding_provider = crate::models::model_provider::ModelProvider::new(
            embedding_config.name,
            common::enums::ProviderType::from_i32(embedding_config.provider_type),
            common::enums::ModelCapability::Embedding,
            embedding_config.model_name,
            embedding_config.api_key,
            embedding_config.base_url,
            embedding_config.description,
            user_id.clone(),
        );
        let provider_id = embedding_provider.po.id.clone();
        finance::domain()
            .model_provider_manage()
            .create_model_provider(ctx, &embedding_provider)
            .await?;
        Some(provider_id)
    } else {
        None
    };

    Ok(InitializeSystemResponse {
        organization_id: org_id,
        user_id,
        chat_provider_id,
        embedding_provider_id,
    })
}
