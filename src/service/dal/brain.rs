//! Brain DAL 模块
//!
//! 职责：从 ModelProvider 创建 Cortex，然后组合 Memory 创建完整的 Brain 实体
//! BrainDal 依赖 CortexDao 创建 CortexTrait，然后组装成完整的 Brain
//! 合并了原来 CortexDal 的功能，不再重复拆分

use anyhow::Result;
use crate::error::AppError;
use crate::models::brain::{Brain, Cortex, CortexTrait, Memory};
use crate::models::model_provider::ModelProvider;
use crate::models::tool::FullTool;
use crate::pkg::RequestContext;
use crate::service::dao::cortex::{CortexDao};
use std::sync::{Arc, OnceLock};
use async_trait::async_trait;
use futures_util::TryFutureExt;
use crate::service::dao::cortex;
// ==================== 单例管理 ====================

static BRAIN_DAL: OnceLock<Arc<dyn BrainDal>> = OnceLock::new();

/// 获取 Brain DAL 单例
pub fn dal() -> Arc<dyn BrainDal> {
    BRAIN_DAL.get().cloned().unwrap()
}

/// 初始化 Brain DAL
pub fn init() {
    let _ = BRAIN_DAL.set(new(
        cortex::dao(),
    ));
}

/// 创建 Brain DAL（返回 trait 对象）
pub fn new(
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
) -> Arc<dyn BrainDal> {
    Arc::new(BrainDalImpl {
        cortex_dao,
    })
}

// ==================== DAL 接口 ====================

/// Brain DAL 接口
#[async_trait]
pub trait BrainDal: Send + Sync {
    /// 从 ModelProvider、Memory 和工具列表创建完整的 Brain
    ///
    /// - BrainDal 内部调用 CortexDao 创建 Cortex
    /// - Memory 已经由上层创建好
    /// - tools: 绑定到该 Agent 的工具列表，从注册中心动态加载
    /// - 返回完整的 Brain 实例
    fn wake_brain(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
        memory: Memory,
        tools: Vec<FullTool>,
    ) -> Result<Brain, AppError>;

    /// 创建 Cortex 并测试连通性，执行一次 prompt 获取回答
    ///
    /// 用于测试模型提供商连接是否正常
    async fn test_connection(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
        prompt: &str,
    ) -> Result<String, AppError>;

    /// 对已存在的 Cortex 执行 prompt 获取回答
    ///
    /// 直接转发给 cortex_dao 异步执行
    async fn prompt_existing_cortex(
        &self,
        ctx: RequestContext,
        cortex: &dyn CortexTrait,
        prompt: &str,
    ) -> Result<String, AppError>;
}

// ==================== DAL 实现 ====================

/// Brain DAL 实现
struct BrainDalImpl {
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
}

impl BrainDalImpl {
    /// 创建 DAL 实例
    fn new(
        cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    ) -> Self {
        Self { cortex_dao }
    }
}

#[async_trait]
impl BrainDal for BrainDalImpl {
    fn wake_brain(
        &self,
        _ctx: RequestContext,
        provider: &ModelProvider,
        memory: Memory,
        tools: Vec<FullTool>,
    ) -> Result<Brain, AppError> {
        // 1. 创建 CortexTrait，传入工具列表
        let cortex_trait = self.cortex_dao.create_cortex_trait(_ctx, provider, tools)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // 2. 创建 Cortex 实体
        let cortex = Cortex::new(provider.clone(), cortex_trait);

        // 3. 组合 Cortex + Memory 创建 Brain
        let brain = Brain::new(cortex, memory);

        Ok(brain)
    }

    async fn test_connection(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
        prompt: &str,
    ) -> Result<String, AppError> {
        // 1. 创建 Cortex，测试连接不需要工具
        let cortex_trait = self.cortex_dao.create_cortex_trait(ctx.clone(), provider, Vec::new())
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // 2. 执行 prompt 获取回答
        self.prompt_existing_cortex(ctx, &*cortex_trait, prompt).await
    }

    async fn prompt_existing_cortex(
        &self,
        ctx: RequestContext,
        cortex: &dyn CortexTrait,
        prompt: &str,
    ) -> Result<String, AppError> {
        self.cortex_dao.prompt(ctx, cortex, prompt).await
            .map_err(|e| AppError::Internal(e.to_string()))
    }
}
