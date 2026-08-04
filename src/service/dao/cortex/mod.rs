//! Cortex DAO - 大脑皮层工厂
//!
//! 根据 Model Provider 创建 CortexTrait 实例，提供统一推理接口
//! 包含 create_cortex_trait 和 prompt（执行 prompt 获取回答）

use crate::models::brain::*;
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::request_context::RequestContext;
use ::rig::tool::DynamicTool;
use anyhow::Result;

/// Cortex DAO 工厂 trait
///
/// Cortex Dao 负责创建 CortexTrait 和执行推理/向量化，所有方法都传递 ctx
///
/// 【扩展点】`ctx` 参数当前在 RigCortexDao 实现中未使用（cortex 构造时已通过
/// RuntimeMonitoringHook 捕获 ctx 快照），但保留在 trait 签名中作为扩展点：
/// 若未来引入 brain 缓存（参见 awakening.rs TODO(brain-cache)），cortex 构造时
/// 捕获的 ctx 会变 stale，此时实现可改用传入的最新 ctx 刷新监控 hook。
#[async_trait::async_trait]
pub trait CortexDao: Send + Sync {
    /// ✅ 根据 Model Provider 创建 CortexTrait 实例，绑定已包装的 Rig 工具列表
    fn create_cortex_trait(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        rig_tools: Vec<DynamicTool>,
    ) -> Result<Box<dyn CortexTrait + Send + Sync>>;

    /// ✅ 执行 prompt：使用已创建的 CortexTrait 推理获取回答
    async fn prompt(
        &self,
        ctx: RequestContext,
        cortex: &dyn CortexTrait,
        prompt: &str,
    ) -> Result<String>;

    /// ✅ 文本转向量（仅返回原始向量数据）
    async fn embed_text_raw(
        &self,
        ctx: RequestContext,
        cortex: &dyn CortexTrait,
        text: &str,
    ) -> Result<Vec<f32>>;

    /// ✅ 实体向量化（返回完整 VectorIndexParams，用于索引场景）
    async fn embed_entity(
        &self,
        ctx: RequestContext,
        cortex: &dyn CortexTrait,
        entity: &dyn crate::models::vector::Vectorizable,
    ) -> Result<crate::models::vector::VectorIndexParams>;

    /// ✅ 文本转向量（返回完整 VectorIndexParams，用于搜索场景）
    async fn embed_text_for_search(
        &self,
        ctx: RequestContext,
        cortex: &dyn CortexTrait,
        text: &str,
    ) -> Result<crate::models::vector::VectorIndexParams>;
}

pub mod external;
mod rig;

pub use self::rig::{RigCortexDao, dao, init};

#[cfg(test)]
mod rig_test;
